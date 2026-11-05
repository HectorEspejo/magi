//! Puente entre los eventos de una fuente (RDP o VNC) y los mensajes del
//! visor (Fase 12, §7.5, D225).
//!
//! El puente es una tarea por sesión:
//! - acumula las zonas sucias y las agrupa a 60 fps; cada lote sale como
//!   `0x05` Rect con los píxeles RGBA del búfer del escritorio (`0x06` Copia
//!   cuando no hay pintura pendiente, para no mandar píxeles) y `0x07`
//!   VideoRdp con el H.264 Annex-B de Windows tal cual;
//! - `0x08` Marco cierra el lote y `0x03` lleva el cursor remoto;
//! - guarda un búfer del escritorio con los rectángulos recibidos para
//!   repintar entero con `v,1` o `kf`, y para que el visor pueda entrar a
//!   mitad de sesión;
//! - aplica contrapresión de 8 MB: si el visor va lento no se encola más y las
//!   zonas pendientes salen una sola vez con su estado final (el sucio es una
//!   lista acotada de regiones, no se acumulan píxeles por evento); ningún
//!   mensaje individual pasa de la cola (un rectángulo mayor se trocea por
//!   filas);
//! - con v,0 deja de mandar imagen (rectángulos, copias, cursor y vídeo) y
//!   avisa a la fuente y al servidor; al volver con v,1 repinta desde el búfer.
//!   El cliente RDP actual todavía ignora OrdenEscritorio::Visible, así que
//!   tras una pausa con H.264 el keyframe lo tiene que mandar Windows por su
//!   cuenta (limitación anotada en la revisión adversarial, M7);
//! - valida toda orden del visor (tamaños 200×200..8192×8192, coordenadas
//!   dentro del escritorio, comandos conocidos y texto de hasta 1 MB) y lo que
//!   no encaja se descarta sin pánico;
//! - avisa al servidor de escritorios de lo que le toca: sesión abierta,
//!   caída o cerrada, visor conectado o desconectado, navegador sin H.264 y
//!   reconectar.
//!
//! La decodificación no vive aquí: los eventos llegan ya decodificados desde
//! el hilo de la fuente, dentro de EventoEscritorio.

use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use serde_json::json;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};

use super::visor_http::{AvisoVisor, Tiempos};
use crate::escritorio::{
    EventoEscritorio, ImagenEscritorio, OrdenEscritorio, Region, TipoHost, LADO_MAX,
};

/// Marcos por segundo a los que se agrupan las zonas sucias.
pub const FPS_MAX: u32 = 60;
/// Bytes máximos pendientes hacia el visor (contrapresión, §7.5).
pub const COLA_MAX: usize = 8 * 1024 * 1024;
/// Zonas sucias máximas que se acumulan (memoria acotada por diseño).
pub const ZONAS_MAX: usize = 64;
/// Lado máximo del cursor remoto.
pub const CURSOR_MAX: u16 = 256;
/// Texto de portapapeles máximo (1 MB UTF-8).
pub const PORTAPAPELES_MAX: usize = 1024 * 1024;
/// Tamaño máximo de un marco H.264 de entrada (entrada no fiable).
pub const H264_MAX: usize = 16 * 1024 * 1024;
/// Mensajes que caben en la cola hacia el visor antes de aplicar contrapresión.
pub const MENSAJES_MAX: usize = 256;
/// Tamaño máximo de una zona sucia: una cuarta parte de la cola. Así caben
/// varias en vuelo y el lote avanza aunque el visor vaya lento (si una zona
/// ocupara la cola entera, el troceo no podría progresar nunca).
const MAX_REGION: usize = COLA_MAX / 4;
/// Lado mínimo que el visor puede pedir con `sz` (§7.13).
pub const LADO_VISOR_MIN: u16 = 200;

const TIPO_CURSOR: u8 = 0x03;
const TIPO_RECT: u8 = 0x05;
const TIPO_COPIA: u8 = 0x06;
const TIPO_VIDEO: u8 = 0x07;
const TIPO_MARCO: u8 = 0x08;

// ---------------------------------------------------------------- compartido

/// Estado que el puente publica para la lista Escritorios, la gracia y los
/// handlers HTTP: visor conectado sí o no, fps y tamaño actual.
pub struct Compartido {
    conectado: AtomicBool,
    fps: AtomicU16,
    ancho: AtomicU16,
    alto: AtomicU16,
    imagen: RwLock<ImagenEscritorio>,
}

impl Compartido {
    /// Estado inicial de una sesión con el tamaño y la imagen de apertura.
    pub fn nuevo(ancho: u16, alto: u16, imagen: ImagenEscritorio) -> Self {
        Self {
            conectado: AtomicBool::new(false),
            fps: AtomicU16::new(0),
            ancho: AtomicU16::new(ancho),
            alto: AtomicU16::new(alto),
            imagen: RwLock::new(imagen),
        }
    }

    pub fn conectado(&self) -> bool {
        self.conectado.load(Ordering::Relaxed)
    }

    pub fn fps(&self) -> u16 {
        self.fps.load(Ordering::Relaxed)
    }

    pub fn ancho(&self) -> u16 {
        self.ancho.load(Ordering::Relaxed)
    }

    pub fn alto(&self) -> u16 {
        self.alto.load(Ordering::Relaxed)
    }

    pub fn imagen(&self) -> ImagenEscritorio {
        *self.imagen.read().unwrap_or_else(|roto| roto.into_inner())
    }

    /// Fija el tamaño que se publica (voxl no pasa por el bucle del puente).
    pub fn fijar_tamano(&self, ancho: u16, alto: u16) {
        self.ancho.store(ancho, Ordering::Relaxed);
        self.alto.store(alto, Ordering::Relaxed);
    }

    /// Fija la imagen que se publica (voxl no pasa por el bucle del puente).
    pub fn fijar_imagen(&self, imagen: ImagenEscritorio) {
        *self.imagen.write().unwrap_or_else(|roto| roto.into_inner()) = imagen;
    }
}

// ---------------------------------------------------------------- salida

/// Mensaje que el puente entrega a la tarea que escribe el WebSocket.
#[derive(Debug, Clone)]
pub enum MensajeVisor {
    Binario(Vec<u8>),
    Texto(String),
    Cerrar,
}

impl MensajeVisor {
    fn bytes(&self) -> usize {
        match self {
            MensajeVisor::Binario(datos) => datos.len(),
            MensajeVisor::Texto(texto) => texto.len(),
            MensajeVisor::Cerrar => 0,
        }
    }
}

/// Cola acotada hacia el visor con la cuenta de bytes pendientes (§7.5).
///
/// La tarea del WebSocket consume y va descontando; el puente solo encola si
/// lo pendiente cabe en COLA_MAX, así que un visor lento hace que las zonas se
/// acumulen como regiones sucias y salgan una sola vez.
#[derive(Clone)]
pub struct SalidaVisor {
    tx: mpsc::Sender<MensajeVisor>,
    pendientes: Arc<AtomicUsize>,
    aviso: Arc<Notify>,
    cerrada: Arc<AtomicBool>,
}

impl SalidaVisor {
    /// Crea la cola hacia el visor y su extremo de lectura.
    pub fn nueva() -> (SalidaVisor, mpsc::Receiver<MensajeVisor>) {
        let (tx, rx) = mpsc::channel(MENSAJES_MAX);
        (
            SalidaVisor {
                tx,
                pendientes: Arc::new(AtomicUsize::new(0)),
                aviso: Arc::new(Notify::new()),
                cerrada: Arc::new(AtomicBool::new(false)),
            },
            rx,
        )
    }

    /// Encola un mensaje. Falso si no hay sitio (contrapresión) o si la tarea
    /// del WebSocket ya no escucha.
    ///
    /// Un mensaje individual nunca puede pasar de COLA_MAX (revisión
    /// adversarial, C2): un rectángulo mayor se trocea por filas antes de
    /// llegar aquí, así que la contrapresión es un tope duro también para el
    /// primero de la sesión.
    pub fn enviar(&self, mensaje: MensajeVisor) -> bool {
        let bytes = mensaje.bytes();
        if bytes > COLA_MAX {
            return false;
        }
        let antes = self.pendientes.fetch_add(bytes, Ordering::Relaxed);
        if antes > 0 && antes.saturating_add(bytes) > COLA_MAX {
            self.pendientes.fetch_sub(bytes, Ordering::Relaxed);
            return false;
        }
        match self.tx.try_send(mensaje) {
            Ok(()) => {
                self.aviso.notify_one();
                true
            }
            Err(_) => {
                self.pendientes.fetch_sub(bytes, Ordering::Relaxed);
                false
            }
        }
    }

    pub fn binario(&self, datos: Vec<u8>) -> bool {
        self.enviar(MensajeVisor::Binario(datos))
    }

    pub fn texto(&self, texto: String) -> bool {
        self.enviar(MensajeVisor::Texto(texto))
    }

    /// Pide a la tarea del WebSocket que cierre: lo que quede en la cola se
    /// descarta y el socket manda close.
    pub fn cerrar(&self) {
        self.cerrada.store(true, Ordering::Relaxed);
        self.aviso.notify_one();
    }

    pub fn cerrada(&self) -> bool {
        self.cerrada.load(Ordering::Relaxed)
    }

    /// Bytes pendientes de escribir en el WebSocket.
    pub fn pendiente(&self) -> usize {
        self.pendientes.load(Ordering::Relaxed)
    }

    /// La tarea del WebSocket descuenta lo que ya ha escrito.
    pub fn descontar(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let mut actual = self.pendientes.load(Ordering::Relaxed);
        loop {
            let nuevo = actual.saturating_sub(bytes);
            match self.pendientes.compare_exchange_weak(
                actual,
                nuevo,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(leido) => actual = leido,
            }
        }
    }

    /// Aviso para el select de la tarea del WebSocket: hay algo en la cola.
    pub fn aviso(&self) -> &Notify {
        &self.aviso
    }
}

// ---------------------------------------------------------------- comandos

/// Una orden interpretada del visor, ya validada.
#[derive(Debug, Clone, PartialEq)]
pub enum InstruccionVisor {
    /// Va a la fuente (RDP o VNC) como OrdenEscritorio.
    Orden(OrdenEscritorio),
    /// La pestaña se oculta (v,0) o vuelve (v,1).
    Visible(bool),
    /// Pantalla completa (kf): repintar el escritorio entero.
    Repintar,
    /// Respuesta al ping (po,<s>): época en milisegundos que se mandó.
    Latencia(u64),
    /// El visor pide reconectar (rec).
    Reconectar,
}

/// Lo que la tarea del WebSocket manda al bucle del puente.
pub enum ComandoPuente {
    /// Un visor conecta (el primero o el que sustituye al anterior).
    Conectar {
        generacion: u64,
        salida: SalidaVisor,
        h264: bool,
    },
    /// La pestaña se oculta o vuelve.
    Visible(bool),
    /// Repintar el escritorio entero.
    Repintar,
    /// Respuesta al ping.
    Latencia { s: u64 },
    /// Una orden validada del visor para la fuente.
    Orden(OrdenEscritorio),
    /// La fuente cambia (reconexión sin recrear el visor ni su clave).
    Fuente {
        eventos: mpsc::Receiver<EventoEscritorio>,
        ordenes: mpsc::Sender<OrdenEscritorio>,
    },
}

/// Avisos de ciclo de vida del visor. Van por un canal sin pérdida (unbounded)
/// para que un canal de comandos lleno de teclas y ratón no pierda un caído, un
/// cierre o un «otra ventana» (revisión adversarial, m9).
pub enum ComandoEstado {
    /// El visor de esa generación se ha ido.
    Desconectar { generacion: u64 },
    /// Avisa al visor del estado (sin cerrarle el socket si cerrar es falso).
    Echar {
        estado: String,
        motivo: Option<String>,
        cerrar: bool,
    },
    /// Fin de la sesión: avisa al visor, cierra y termina el bucle.
    Cerrar {
        estado: String,
        motivo: Option<String>,
    },
}

// ---------------------------------------------------------------- búfer

/// Búfer RGBA del escritorio: la única copia de los píxeles, para repintar y
/// para no pedirlos a la fuente.
pub struct Bufer {
    ancho: u16,
    alto: u16,
    pixeles: Vec<u8>,
}

impl Bufer {
    pub fn nuevo(ancho: u16, alto: u16) -> Self {
        Self {
            ancho,
            alto,
            pixeles: vec![0; Self::tamano(ancho, alto)],
        }
    }

    fn tamano(ancho: u16, alto: u16) -> usize {
        usize::from(ancho) * usize::from(alto) * 4
    }

    pub fn ancho(&self) -> u16 {
        self.ancho
    }

    pub fn alto(&self) -> u16 {
        self.alto
    }

    /// Redimensiona el búfer (negro). Si el tamaño no cambia, no toca nada.
    pub fn redimensionar(&mut self, ancho: u16, alto: u16) {
        if ancho == 0 || alto == 0 || ancho > LADO_MAX || alto > LADO_MAX {
            return;
        }
        if ancho == self.ancho && alto == self.alto {
            return;
        }
        self.ancho = ancho;
        self.alto = alto;
        self.pixeles = vec![0; Self::tamano(ancho, alto)];
    }

    /// ¿La zona cabe entera en el búfer?
    pub fn dentro(&self, region: &Region) -> bool {
        !region.vacia() && region.dentro_de(self.ancho, self.alto)
    }

    /// Pinta una zona con sus píxeles RGBA. Falso si no cabe o no cuadra el
    /// tamaño (entrada no fiable: se descarta).
    pub fn pintar(&mut self, region: &Region, pixeles: &[u8]) -> bool {
        if !self.dentro(region) {
            return false;
        }
        let esperado = usize::from(region.ancho) * usize::from(region.alto) * 4;
        if pixeles.len() != esperado {
            return false;
        }
        let fila = usize::from(region.ancho) * 4;
        for y in 0..usize::from(region.alto) {
            let destino = (usize::from(region.y) + y) * usize::from(self.ancho) * 4
                + usize::from(region.x) * 4;
            let origen = y * fila;
            self.pixeles[destino..destino + fila].copy_from_slice(&pixeles[origen..origen + fila]);
        }
        true
    }

    /// Copia una zona ya pintada (CopyRect). Falso si no cabe.
    pub fn copiar(
        &mut self,
        origen: (u16, u16),
        destino: (u16, u16),
        ancho: u16,
        alto: u16,
    ) -> bool {
        let zona_origen = Region::nueva(origen.0, origen.1, ancho, alto);
        let zona_destino = Region::nueva(destino.0, destino.1, ancho, alto);
        if !self.dentro(&zona_origen) || !self.dentro(&zona_destino) {
            return false;
        }
        // Temporal para que el solapamiento no pise lo que aún hay que leer.
        let Some(tmp) = self.recorte(&zona_origen) else {
            return false;
        };
        self.pintar(&zona_destino, &tmp)
    }

    /// Píxeles RGBA de una zona, copiados fila a fila.
    pub fn recorte(&self, region: &Region) -> Option<Vec<u8>> {
        if !self.dentro(region) {
            return None;
        }
        let fila = usize::from(region.ancho) * 4;
        let mut salida = Vec::with_capacity(fila * usize::from(region.alto));
        for y in 0..usize::from(region.alto) {
            let inicio = (usize::from(region.y) + y) * usize::from(self.ancho) * 4
                + usize::from(region.x) * 4;
            salida.extend_from_slice(&self.pixeles[inicio..inicio + fila]);
        }
        Some(salida)
    }
}

// ---------------------------------------------------------------- zonas

/// Zonas sucias: una lista acotada de regiones que se ensancha al tocarse.
pub struct Sucio {
    regiones: Vec<Region>,
}

impl Sucio {
    pub fn nuevo() -> Self {
        Self {
            regiones: Vec::new(),
        }
    }

    pub fn vacia(&self) -> bool {
        self.regiones.is_empty()
    }

    pub fn vaciar(&mut self) {
        self.regiones.clear();
    }

    /// Añade una zona, fusionándola con las que solapan o tocan. Si ya hay
    /// demasiadas, se funde con la que menos crezca: la memoria queda acotada.
    /// Las zonas que quedan se parten para que ninguna pase de MAX_REGION.
    pub fn unir(&mut self, region: Region) {
        if region.vacia() {
            return;
        }
        let mut nueva = region;
        let mut i = 0;
        while i < self.regiones.len() {
            if tocan(&self.regiones[i], &nueva) {
                nueva = self.regiones.swap_remove(i).unir(&nueva);
                i = 0;
            } else {
                i += 1;
            }
        }
        for trozo in dividir_si_grande(nueva) {
            self.insertar(trozo);
        }
    }

    /// Inserta un trozo; si ya no cabe, lo funde con la zona que menos crezca
    /// (y la vuelve a partir si el resultado es grande).
    fn insertar(&mut self, trozo: Region) {
        if self.regiones.len() < ZONAS_MAX {
            self.regiones.push(trozo);
            return;
        }
        let mut elegida = 0;
        let mut menor = u32::MAX;
        for (indice, existente) in self.regiones.iter().enumerate() {
            let coste = area(&existente.unir(&trozo)) - area(existente);
            if coste < menor {
                menor = coste;
                elegida = indice;
            }
        }
        let fusion = self.regiones.swap_remove(elegida).unir(&trozo);
        for parte in dividir_si_grande(fusion) {
            self.regiones.push(parte);
        }
    }

    /// Quita las zonas ya mandadas (o descartadas) para que el siguiente lote
    /// siga avanzando en vez de reenviarlas para siempre.
    pub fn quitar(&mut self, enviadas: &[Region]) {
        self.regiones.retain(|region| !enviadas.contains(region));
    }

    /// Copia de las zonas que hay que mandar (como mucho max).
    pub fn simplificar(&self, max: usize) -> Vec<Region> {
        self.regiones.iter().take(max).copied().collect()
    }
}

fn area(region: &Region) -> u32 {
    u32::from(region.ancho) * u32::from(region.alto)
}

/// ¿Dos zonas solapan o comparten borde? (fusión barata, sin píxeles).
fn tocan(a: &Region, b: &Region) -> bool {
    let ax2 = u32::from(a.x) + u32::from(a.ancho);
    let ay2 = u32::from(a.y) + u32::from(a.alto);
    let bx2 = u32::from(b.x) + u32::from(b.ancho);
    let by2 = u32::from(b.y) + u32::from(b.alto);
    u32::from(a.x) <= bx2 && u32::from(b.x) <= ax2 && u32::from(a.y) <= by2 && u32::from(b.y) <= ay2
}

// ---------------------------------------------------------------- bucle

/// Datos con los que se lanza el bucle del puente de una sesión.
pub struct ConfigPuente {
    pub id: u32,
    pub tipo: TipoHost,
    pub redimensionable: bool,
    pub portapapeles: bool,
    pub tiempos: Tiempos,
    pub compartido: Arc<Compartido>,
    pub avisos: mpsc::Sender<AvisoVisor>,
    pub comandos: mpsc::Receiver<ComandoPuente>,
    /// Avisos de ciclo de vida sin pérdida (m9).
    pub estados: mpsc::UnboundedReceiver<ComandoEstado>,
    pub eventos: mpsc::Receiver<EventoEscritorio>,
    pub ordenes: mpsc::Sender<OrdenEscritorio>,
}

/// Lanza el bucle del puente en el runtime de la sesión.
pub fn lanzar(config: ConfigPuente) -> JoinHandle<()> {
    tokio::spawn(Bucle::nuevo(config).ejecutar())
}

struct VisorConectado {
    generacion: u64,
    salida: SalidaVisor,
    h264: bool,
}

struct Bucle {
    id: u32,
    tipo: TipoHost,
    redimensionable: bool,
    portapapeles: bool,
    tiempos: Tiempos,
    compartido: Arc<Compartido>,
    avisos: mpsc::Sender<AvisoVisor>,
    comandos: mpsc::Receiver<ComandoPuente>,
    estados: mpsc::UnboundedReceiver<ComandoEstado>,
    eventos: Option<mpsc::Receiver<EventoEscritorio>>,
    ordenes: Option<mpsc::Sender<OrdenEscritorio>>,
    bufer: Bufer,
    sucio: Sucio,
    visor: Option<VisorConectado>,
    visible: bool,
    marcos: u32,
}

enum Accion {
    Comando(Option<ComandoPuente>),
    Estado(Option<ComandoEstado>),
    Evento(Option<EventoEscritorio>),
    Tick,
    Segundo,
    Ping,
}

impl Bucle {
    fn nuevo(config: ConfigPuente) -> Self {
        let ancho = config.compartido.ancho();
        let alto = config.compartido.alto();
        Self {
            id: config.id,
            tipo: config.tipo,
            redimensionable: config.redimensionable,
            portapapeles: config.portapapeles,
            tiempos: config.tiempos,
            compartido: config.compartido,
            avisos: config.avisos,
            comandos: config.comandos,
            estados: config.estados,
            eventos: Some(config.eventos),
            ordenes: Some(config.ordenes),
            bufer: Bufer::nuevo(ancho, alto),
            sucio: Sucio::nuevo(),
            visor: None,
            visible: true,
            marcos: 0,
        }
    }

    async fn ejecutar(mut self) {
        let mut tick = interval(self.tiempos.intervalo_marcos);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut segundo = interval(Duration::from_secs(1));
        segundo.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut ping = interval(self.tiempos.intervalo_ping);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            let accion = tokio::select! {
                comando = self.comandos.recv() => Accion::Comando(comando),
                estado = self.estados.recv() => Accion::Estado(estado),
                evento = siguiente_evento(&mut self.eventos) => Accion::Evento(evento),
                _ = tick.tick() => Accion::Tick,
                _ = segundo.tick() => Accion::Segundo,
                _ = ping.tick() => Accion::Ping,
            };
            match accion {
                Accion::Comando(None) => break,
                Accion::Comando(Some(comando)) => {
                    if !self.comando(comando).await {
                        break;
                    }
                }
                // Sin dueño (el registro soltó el emisor) no hay a quién
                // avisar: el bucle termina.
                Accion::Estado(None) => break,
                Accion::Estado(Some(estado)) => {
                    if !self.estado(estado).await {
                        break;
                    }
                }
                Accion::Evento(None) => self.eventos = None,
                Accion::Evento(Some(evento)) => self.evento(evento).await,
                Accion::Tick => self.lote(),
                Accion::Segundo => {
                    let fps = self.marcos.min(u32::from(u16::MAX)) as u16;
                    self.compartido.fps.store(fps, Ordering::Relaxed);
                    self.marcos = 0;
                }
                Accion::Ping => self.ping(),
            }
        }
        if let Some(visor) = self.visor.take() {
            visor.salida.cerrar();
        }
    }

    // ------------------------------------------------------------ comandos

    /// Procesa un comando. Falso si el bucle debe terminar.
    async fn comando(&mut self, comando: ComandoPuente) -> bool {
        match comando {
            ComandoPuente::Conectar {
                generacion,
                salida,
                h264,
            } => self.conectar(generacion, salida, h264).await,
            ComandoPuente::Visible(visible) => {
                self.visible = visible;
                self.orden(OrdenEscritorio::Visible(visible)).await;
                let _ = self
                    .avisos
                    .send(AvisoVisor::Visible {
                        id: self.id,
                        visible,
                    })
                    .await;
                if visible {
                    self.repintar_entero();
                }
            }
            ComandoPuente::Repintar => self.repintar_entero(),
            ComandoPuente::Latencia { s } => self.latencia(s),
            ComandoPuente::Orden(orden) => self.orden(orden).await,
            ComandoPuente::Fuente { eventos, ordenes } => {
                self.eventos = Some(eventos);
                self.ordenes = Some(ordenes);
                if let Some(visor) = &self.visor {
                    let _ = visor
                        .salida
                        .texto(json!({"t": "estado", "estado": "conectando"}).to_string());
                }
                self.mandar_hello();
                self.repintar_entero();
            }
        }
        true
    }

    /// Procesa un aviso de ciclo de vida (canal sin pérdida). Falso si el
    /// bucle debe terminar.
    async fn estado(&mut self, estado: ComandoEstado) -> bool {
        match estado {
            ComandoEstado::Desconectar { generacion } => {
                let es_el_actual = self
                    .visor
                    .as_ref()
                    .map(|visor| visor.generacion == generacion)
                    .unwrap_or(false);
                if es_el_actual {
                    self.visor = None;
                    self.compartido.conectado.store(false, Ordering::Relaxed);
                    let _ = self
                        .avisos
                        .send(AvisoVisor::VisorDesconectado { id: self.id })
                        .await;
                }
            }
            ComandoEstado::Echar {
                estado,
                motivo,
                cerrar,
            } => self.echar(&estado, motivo.as_deref(), cerrar).await,
            ComandoEstado::Cerrar { estado, motivo } => {
                self.echar(&estado, motivo.as_deref(), true).await;
                return false;
            }
        }
        true
    }

    async fn conectar(&mut self, generacion: u64, salida: SalidaVisor, h264: bool) {
        if let Some(anterior) = self.visor.take() {
            let _ = anterior
                .salida
                .texto(json!({"t": "estado", "estado": "otra_ventana"}).to_string());
            anterior.salida.cerrar();
        }
        self.visor = Some(VisorConectado {
            generacion,
            salida,
            h264,
        });
        self.compartido.conectado.store(true, Ordering::Relaxed);
        let _ = self
            .avisos
            .send(AvisoVisor::VisorConectado { id: self.id, h264 })
            .await;
        if !h264 && self.tipo == TipoHost::Rdp && self.compartido.imagen() == ImagenEscritorio::H264
        {
            let _ = self.avisos.send(AvisoVisor::SinH264 { id: self.id }).await;
        }
        self.mandar_hello();
        self.repintar_entero();
    }

    async fn echar(&mut self, estado: &str, motivo: Option<&str>, cerrar: bool) {
        let Some(visor) = self.visor.take() else {
            return;
        };
        let mut aviso = json!({"t": "estado", "estado": estado});
        if let Some(motivo) = motivo {
            aviso["motivo"] = json!(motivo);
        }
        let _ = visor.salida.texto(aviso.to_string());
        if cerrar {
            let _ = visor.salida.texto(json!({"t": "close"}).to_string());
            visor.salida.cerrar();
            self.compartido.conectado.store(false, Ordering::Relaxed);
        } else {
            self.visor = Some(visor);
        }
    }

    async fn orden(&mut self, orden: OrdenEscritorio) {
        if let Some(ordenes) = &self.ordenes {
            let _ = ordenes.send(orden).await;
        }
    }

    async fn avisar(&mut self, aviso: AvisoVisor) {
        let _ = self.avisos.send(aviso).await;
    }

    // ------------------------------------------------------------ eventos

    async fn evento(&mut self, evento: EventoEscritorio) {
        match evento {
            EventoEscritorio::Abierto => {
                self.avisar(AvisoVisor::Abierto { id: self.id }).await;
            }
            EventoEscritorio::Rect {
                x,
                y,
                ancho,
                alto,
                pixeles,
            } => {
                let region = Region::nueva(x, y, ancho, alto);
                if self.bufer.pintar(&region, &pixeles) {
                    self.sucio.unir(region);
                }
            }
            EventoEscritorio::Copia {
                origen,
                destino,
                ancho,
                alto,
            } => {
                let region = Region::nueva(destino.0, destino.1, ancho, alto);
                if !self.bufer.copiar(origen, destino, ancho, alto) {
                    return;
                }
                if self.visor.is_none() {
                    return;
                }
                if self.sucio.vacia() {
                    self.mandar_copia(origen, destino, ancho, alto);
                } else {
                    // Con pintura pendiente, mejor repintar desde el búfer.
                    self.sucio.unir(region);
                }
            }
            EventoEscritorio::Video {
                marco,
                regiones,
                h264,
            } => self.video(marco, regiones, h264),
            EventoEscritorio::Tamano { ancho, alto } => self.tamano(ancho, alto),
            EventoEscritorio::Cursor {
                visible,
                punto,
                ancho,
                alto,
                pixeles,
                ..
            } => self.cursor(visible, punto, ancho, alto, &pixeles),
            EventoEscritorio::Portapapeles(texto) => {
                if !self.portapapeles || texto.len() > PORTAPAPELES_MAX {
                    return;
                }
                if let Some(visor) = &self.visor {
                    let _ = visor
                        .salida
                        .texto(json!({"t": "clip", "texto": texto}).to_string());
                }
            }
            // El remoto pide el portapapeles local: el visor manda el suyo
            // cuando el usuario pega (no hay forma de leerlo sin un gesto).
            EventoEscritorio::PedirPortapapeles => {}
            EventoEscritorio::Imagen(imagen) => {
                *self
                    .compartido
                    .imagen
                    .write()
                    .unwrap_or_else(|roto| roto.into_inner()) = imagen;
                if self.tipo == TipoHost::Rdp && imagen == ImagenEscritorio::H264 {
                    let sin_h264 = self
                        .visor
                        .as_ref()
                        .map(|visor| !visor.h264)
                        .unwrap_or(false);
                    if sin_h264 {
                        self.avisar(AvisoVisor::SinH264 { id: self.id }).await;
                    }
                }
                self.mandar_hello();
            }
            EventoEscritorio::Caido(motivo) => {
                self.avisar(AvisoVisor::Caido {
                    id: self.id,
                    motivo,
                })
                .await;
            }
            EventoEscritorio::Cerrado(motivo) => {
                self.avisar(AvisoVisor::Cerrado {
                    id: self.id,
                    motivo,
                })
                .await;
            }
            EventoEscritorio::Fin => self.eventos = None,
        }
    }

    fn mandar_hello(&self) {
        let Some(visor) = &self.visor else {
            return;
        };
        let _ = visor.salida.texto(
            json!({
                "t": "hello",
                "w": self.bufer.ancho(),
                "h": self.bufer.alto(),
                "tipo": self.tipo.como_texto(),
                "imagen": texto_imagen(self.compartido.imagen()),
                "redimensionable": self.redimensionable,
                "portapapeles": self.portapapeles,
            })
            .to_string(),
        );
    }

    fn tamano(&mut self, ancho: u16, alto: u16) {
        if ancho == 0 || alto == 0 || ancho > LADO_MAX || alto > LADO_MAX {
            return;
        }
        self.bufer.redimensionar(ancho, alto);
        self.compartido.ancho.store(ancho, Ordering::Relaxed);
        self.compartido.alto.store(alto, Ordering::Relaxed);
        if let Some(visor) = &self.visor {
            let _ = visor
                .salida
                .texto(json!({"t": "tam", "w": ancho, "h": alto}).to_string());
        }
        self.repintar_entero();
    }

    fn video(&mut self, marco: u32, regiones: Vec<Region>, h264: Vec<u8>) {
        if h264.is_empty() || h264.len() > H264_MAX {
            return;
        }
        *self
            .compartido
            .imagen
            .write()
            .unwrap_or_else(|roto| roto.into_inner()) = ImagenEscritorio::H264;
        let Some(visor) = &self.visor else {
            return;
        };
        // Con la pestaña oculta (v,0) no se manda vídeo (revisión adversarial,
        // M7). Al volver (v,1) el decodificador del navegador arranca de cero
        // y necesita un keyframe: lo tiene que pedir la fuente; el cliente RDP
        // actual todavía ignora OrdenEscritorio::Visible (ver el doc del
        // módulo), así que hasta entonces la imagen puede tardar en volver.
        if !self.visible {
            return;
        }
        // El navegador sin H.264 no decodifica nada: el servidor reconectará
        // sin AVC420 (D211) y mientras tanto esos marcos no se mandan.
        if !visor.h264 {
            return;
        }
        let dentro: Vec<Region> = regiones
            .into_iter()
            .filter(|region| self.bufer.dentro(region))
            .take(ZONAS_MAX)
            .collect();
        if dentro.is_empty() {
            return;
        }
        let mut mensaje = Vec::with_capacity(7 + dentro.len() * 8 + h264.len());
        mensaje.push(TIPO_VIDEO);
        mensaje.extend_from_slice(&marco.to_be_bytes());
        mensaje.extend_from_slice(&(dentro.len() as u16).to_be_bytes());
        for region in &dentro {
            escribir_region(&mut mensaje, region);
        }
        mensaje.extend_from_slice(&h264);
        if visor.salida.binario(mensaje) {
            // Los marcos de vídeo también cuentan para los fps (m8).
            self.marcos = self.marcos.saturating_add(1);
        }
    }

    fn cursor(&mut self, visible: bool, punto: (u16, u16), ancho: u16, alto: u16, pixeles: &[u8]) {
        // Con la pestaña oculta tampoco se manda el cursor (M7).
        if !self.visible {
            return;
        }
        let Some(visor) = &self.visor else {
            return;
        };
        // Oculto se manda como 0×0: el visor entiende que no hay cursor.
        let (ancho, alto, punto) = if visible && ancho > 0 && alto > 0 {
            (ancho, alto, punto)
        } else {
            (0, 0, (0, 0))
        };
        if ancho > CURSOR_MAX || alto > CURSOR_MAX {
            return;
        }
        let esperado = usize::from(ancho) * usize::from(alto) * 4;
        if pixeles.len() != esperado {
            return;
        }
        let mut mensaje = Vec::with_capacity(9 + esperado);
        mensaje.push(TIPO_CURSOR);
        mensaje.extend_from_slice(&ancho.to_be_bytes());
        mensaje.extend_from_slice(&alto.to_be_bytes());
        mensaje.extend_from_slice(&punto.0.to_be_bytes());
        mensaje.extend_from_slice(&punto.1.to_be_bytes());
        mensaje.extend_from_slice(pixeles);
        let _ = visor.salida.binario(mensaje);
    }

    /// CopyRect aislado (sin pintura pendiente): se manda y se presenta con un
    /// Marco inmediato, porque si no la copia no se ve hasta el siguiente lote
    /// (revisión adversarial, m1).
    fn mandar_copia(&self, origen: (u16, u16), destino: (u16, u16), ancho: u16, alto: u16) {
        if !self.visible {
            return;
        }
        let Some(visor) = &self.visor else {
            return;
        };
        let mut mensaje = Vec::with_capacity(13);
        mensaje.push(TIPO_COPIA);
        for valor in [origen.0, origen.1, destino.0, destino.1, ancho, alto] {
            mensaje.extend_from_slice(&valor.to_be_bytes());
        }
        if visor.salida.binario(mensaje) {
            let _ = visor.salida.binario(vec![TIPO_MARCO]);
        }
    }

    /// Marca el escritorio entero como sucio y lo manda si cabe. Con imagen
    /// H.264 el búfer no tiene los píxeles (el vídeo no se decodifica): el
    /// repintado lo resuelve el decodificador del navegador con su keyframe.
    fn repintar_entero(&mut self) {
        if self.compartido.imagen() == ImagenEscritorio::H264 {
            return;
        }
        let ancho = self.bufer.ancho();
        let alto = self.bufer.alto();
        if ancho == 0 || alto == 0 {
            return;
        }
        self.sucio.unir(Region::nueva(0, 0, ancho, alto));
        self.lote();
    }

    /// Manda las zonas sucias (Rect + Marco) mientras el visor admite datos.
    /// Las zonas ya mandadas (o descartadas por quedar fuera del búfer tras un
    /// cambio de tamaño) se quitan del sucio: si no, un lote grande no podría
    /// avanzar porque cada intento empezaría por las mismas zonas (C2). Lo que
    /// no cabe se queda para el siguiente tick con su estado final.
    fn lote(&mut self) {
        if !self.visible || self.sucio.vacia() {
            return;
        }
        let Some(visor) = &self.visor else {
            return;
        };
        if visor.salida.pendiente() >= COLA_MAX {
            return;
        }
        let regiones = self.sucio.simplificar(ZONAS_MAX);
        let mut completadas: Vec<Region> = Vec::new();
        for region in &regiones {
            // Tras cambiar de tamaño puede haber zonas fuera del búfer: se
            // descarta solo lo que sobra y se manda la parte válida (m7).
            let Some(recorte) = recortar_region(region, self.bufer.ancho(), self.bufer.alto())
            else {
                completadas.push(*region);
                continue;
            };
            if !self.enviar_region(&recorte, visor) {
                break;
            }
            completadas.push(*region);
        }
        if completadas.is_empty() {
            return;
        }
        // El Marco cierra el lote; si no se puede encolar, se reintenta todo
        // (los rects duplicados del reintento no se presentan sin Marco).
        if !visor.salida.binario(vec![TIPO_MARCO]) {
            return;
        }
        self.sucio.quitar(&completadas);
        self.marcos = self.marcos.saturating_add(1);
    }

    /// Manda una zona; si el Rect pasa de COLA_MAX se trocea por filas para
    /// que la contrapresión sea un tope duro también para un escritorio
    /// grande (revisión adversarial, C2).
    fn enviar_region(&self, region: &Region, visor: &VisorConectado) -> bool {
        let bytes = 9 + usize::from(region.ancho) * usize::from(region.alto) * 4;
        if bytes <= COLA_MAX {
            let Some(pixeles) = self.bufer.recorte(region) else {
                return true;
            };
            let mut mensaje = Vec::with_capacity(9 + pixeles.len());
            mensaje.push(TIPO_RECT);
            escribir_region(&mut mensaje, region);
            mensaje.extend_from_slice(&pixeles);
            return visor.salida.binario(mensaje);
        }
        for trozo in trozos(region, COLA_MAX) {
            let Some(pixeles) = self.bufer.recorte(&trozo) else {
                continue;
            };
            let mut mensaje = Vec::with_capacity(9 + pixeles.len());
            mensaje.push(TIPO_RECT);
            escribir_region(&mut mensaje, &trozo);
            mensaje.extend_from_slice(&pixeles);
            if !visor.salida.binario(mensaje) {
                return false;
            }
        }
        true
    }

    fn ping(&mut self) {
        let Some(visor) = &self.visor else {
            return;
        };
        let _ = visor
            .salida
            .texto(json!({"t": "ping", "s": ahora_ms()}).to_string());
    }

    fn latencia(&mut self, s: u64) {
        let ahora = ahora_ms();
        if s == 0 || s > ahora {
            return;
        }
        let ms = (ahora - s).min(60_000);
        if let Some(visor) = &self.visor {
            let _ = visor
                .salida
                .texto(json!({"t": "latencia", "ms": ms}).to_string());
        }
    }
}

async fn siguiente_evento(
    eventos: &mut Option<mpsc::Receiver<EventoEscritorio>>,
) -> Option<EventoEscritorio> {
    match eventos {
        Some(receptor) => receptor.recv().await,
        None => std::future::pending().await,
    }
}

/// Parte una zona si no cabe en MAX_REGION (bandas horizontales).
fn dividir_si_grande(region: Region) -> Vec<Region> {
    let pixeles = usize::from(region.ancho) * usize::from(region.alto);
    let max_pixeles = MAX_REGION.saturating_sub(9) / 4;
    if pixeles <= max_pixeles || region.ancho == 0 {
        return vec![region];
    }
    let filas = (max_pixeles / usize::from(region.ancho)).max(1);
    let filas = u16::try_from(filas).unwrap_or(region.alto);
    let mut salida = Vec::new();
    let mut y = 0u16;
    while y < region.alto {
        let alto = filas.min(region.alto - y);
        salida.push(Region::nueva(region.x, region.y + y, region.ancho, alto));
        y += alto;
    }
    salida
}

/// Recorta una zona al tamaño actual del búfer. None si no queda nada.
fn recortar_region(region: &Region, ancho: u16, alto: u16) -> Option<Region> {
    if region.vacia() || ancho == 0 || alto == 0 {
        return None;
    }
    let x = u32::from(region.x).min(u32::from(ancho));
    let y = u32::from(region.y).min(u32::from(alto));
    let x2 = (u32::from(region.x) + u32::from(region.ancho)).min(u32::from(ancho));
    let y2 = (u32::from(region.y) + u32::from(region.alto)).min(u32::from(alto));
    if x2 <= x || y2 <= y {
        return None;
    }
    Some(Region::nueva(
        x as u16,
        y as u16,
        u16::try_from(x2 - x).ok()?,
        u16::try_from(y2 - y).ok()?,
    ))
}

/// Parte una zona en bandas horizontales cuyo Rect cabe en el máximo de bytes.
fn trozos(region: &Region, max: usize) -> Vec<Region> {
    let fila = usize::from(region.ancho) * 4;
    if fila == 0 || region.alto == 0 {
        return Vec::new();
    }
    let filas = (max.saturating_sub(9) / fila).clamp(1, usize::from(region.alto));
    let filas = u16::try_from(filas).unwrap_or(region.alto);
    let mut salida = Vec::new();
    let mut y = 0u16;
    while y < region.alto {
        let alto = filas.min(region.alto - y);
        salida.push(Region::nueva(region.x, region.y + y, region.ancho, alto));
        y += alto;
    }
    salida
}

fn escribir_region(mensaje: &mut Vec<u8>, region: &Region) {
    for valor in [region.x, region.y, region.ancho, region.alto] {
        mensaje.extend_from_slice(&valor.to_be_bytes());
    }
}

fn texto_imagen(imagen: ImagenEscritorio) -> &'static str {
    match imagen {
        ImagenEscritorio::H264 => "h264",
        ImagenEscritorio::Rectangulos => "rectangulos",
        ImagenEscritorio::Voxl => "voxl",
    }
}

fn ahora_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duracion| duracion.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- órdenes

/// Interpreta una orden de texto del visor (entrada no fiable). None si no se
/// entiende o no cumple los límites: el puente la descarta sin pánico.
pub fn interpretar_orden(texto: &str, ancho: u16, alto: u16) -> Option<InstruccionVisor> {
    if texto.len() > PORTAPAPELES_MAX + 64 {
        return None;
    }
    let (comando, resto) = match texto.split_once(',') {
        Some((comando, resto)) => (comando, Some(resto)),
        None => (texto, None),
    };
    match comando {
        "kc" => {
            let mut partes = resto?.splitn(3, ',');
            let keysym: u32 = partes.next()?.parse().ok()?;
            let codigo = partes.next()?;
            let abajo = match partes.next()? {
                "1" => true,
                "0" => false,
                _ => return None,
            };
            if keysym > 0x0110_FFFF || (!codigo.is_empty() && !codigo_conocido(codigo)) {
                return None;
            }
            if keysym == 0 && codigo.is_empty() {
                return None;
            }
            Some(InstruccionVisor::Orden(OrdenEscritorio::Tecla {
                codigo: codigo.to_string(),
                keysym,
                abajo,
            }))
        }
        "m" => {
            let mut partes = resto?.split(',');
            let x: u16 = partes.next()?.parse().ok()?;
            let y: u16 = partes.next()?.parse().ok()?;
            if partes.next().is_some() {
                return None;
            }
            if ancho > 0 && alto > 0 && (x >= ancho || y >= alto) {
                return None;
            }
            Some(InstruccionVisor::Orden(OrdenEscritorio::Raton { x, y }))
        }
        "b" => {
            let mut partes = resto?.split(',');
            let boton: u8 = partes.next()?.parse().ok()?;
            let abajo = match partes.next()? {
                "1" => true,
                "0" => false,
                _ => return None,
            };
            // El visor mapea los botones DOM 3/4 (atrás/adelante) a 8/9; el
            // RFB solo tiene 8 botones y descartará el 9, pero RDP sí usa
            // ambos (revisión adversarial, m2).
            if partes.next().is_some() || boton > 9 {
                return None;
            }
            Some(InstruccionVisor::Orden(OrdenEscritorio::Boton {
                boton,
                abajo,
            }))
        }
        "sp" => {
            let mut partes = resto?.split(',');
            let dx: f32 = partes.next()?.parse().ok()?;
            let dy: f32 = partes.next()?.parse().ok()?;
            if partes.next().is_some()
                || !dx.is_finite()
                || !dy.is_finite()
                || dx.abs() > 10_000.0
                || dy.abs() > 10_000.0
            {
                return None;
            }
            Some(InstruccionVisor::Orden(OrdenEscritorio::Rueda { dx, dy }))
        }
        "r" => Some(InstruccionVisor::Orden(OrdenEscritorio::SoltarTodo)),
        "c" => {
            let texto = resto?;
            if texto.len() > PORTAPAPELES_MAX {
                return None;
            }
            Some(InstruccionVisor::Orden(OrdenEscritorio::Portapapeles(
                texto.to_string(),
            )))
        }
        "sz" => {
            let mut partes = resto?.split(',');
            let ancho: u16 = partes.next()?.parse().ok()?;
            let alto: u16 = partes.next()?.parse().ok()?;
            if partes.next().is_some()
                || !(LADO_VISOR_MIN..=LADO_MAX).contains(&ancho)
                || !(LADO_VISOR_MIN..=LADO_MAX).contains(&alto)
            {
                return None;
            }
            Some(InstruccionVisor::Orden(OrdenEscritorio::Tamano {
                ancho,
                alto,
            }))
        }
        "v" => match resto? {
            "1" => Some(InstruccionVisor::Visible(true)),
            "0" => Some(InstruccionVisor::Visible(false)),
            _ => None,
        },
        "kf" => Some(InstruccionVisor::Repintar),
        "cad" => Some(InstruccionVisor::Orden(OrdenEscritorio::CtrlAltSupr)),
        "win" => Some(InstruccionVisor::Orden(OrdenEscritorio::TeclaWindows)),
        "po" => {
            let s: u64 = resto?.parse().ok()?;
            Some(InstruccionVisor::Latencia(s))
        }
        "rec" => Some(InstruccionVisor::Reconectar),
        _ => None,
    }
}

/// ¿Es un código de tecla conocido de la tabla única (o vacío en teclas
/// muertas e IME)?
fn codigo_conocido(codigo: &str) -> bool {
    super::teclas::scancode(codigo).is_some() || super::teclas::keysym_de_codigo(codigo).is_some()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_zonas_se_fusionan_al_tocarse() {
        let mut sucio = Sucio::nuevo();
        sucio.unir(Region::nueva(0, 0, 10, 10));
        sucio.unir(Region::nueva(10, 0, 10, 10));
        assert_eq!(
            sucio.simplificar(ZONAS_MAX),
            vec![Region::nueva(0, 0, 20, 10)]
        );
        sucio.unir(Region::nueva(50, 50, 5, 5));
        assert_eq!(sucio.simplificar(ZONAS_MAX).len(), 2);
        sucio.vaciar();
        assert!(sucio.vacia());
    }

    #[test]
    fn las_zonas_no_crecen_sin_limite() {
        let mut sucio = Sucio::nuevo();
        for i in 0..(ZONAS_MAX * 4) {
            let x = (i as u16 * 7) % 500;
            let y = (i as u16 * 13) % 500;
            sucio.unir(Region::nueva(x, y, 3, 3));
        }
        assert!(sucio.simplificar(ZONAS_MAX).len() <= ZONAS_MAX);
    }

    #[test]
    fn las_zonas_grandes_se_parten_y_se_pueden_quitar() {
        // C2: una zona mayor que MAX_REGION se parte para que el lote pueda
        // avanzar; las zonas mandadas se quitan y el resto sigue pendiente.
        let mut sucio = Sucio::nuevo();
        sucio.unir(Region::nueva(0, 0, 1000, 1000));
        let zonas = sucio.simplificar(ZONAS_MAX);
        assert!(zonas.len() >= 2, "la zona grande se parte");
        let max_pixeles = (MAX_REGION - 9) / 4;
        let mut area_total = 0usize;
        for zona in &zonas {
            let pixeles = usize::from(zona.ancho) * usize::from(zona.alto);
            assert!(pixeles <= max_pixeles);
            area_total += pixeles;
        }
        assert_eq!(area_total, 1_000_000);
        sucio.quitar(&zonas[..1]);
        assert_eq!(sucio.simplificar(ZONAS_MAX).len(), zonas.len() - 1);
        assert!(!sucio.vacia());
    }

    #[test]
    fn el_bufer_pinta_copia_y_recorta() {
        let mut bufer = Bufer::nuevo(4, 4);
        let pixeles: Vec<u8> = (0..16).map(|valor| valor as u8).collect();
        assert!(bufer.pintar(&Region::nueva(0, 0, 2, 2), &pixeles));
        assert!(!bufer.pintar(&Region::nueva(3, 3, 2, 2), &pixeles));
        assert!(!bufer.pintar(&Region::nueva(0, 0, 2, 2), &[0, 0, 0]));
        assert_eq!(
            bufer.recorte(&Region::nueva(0, 0, 2, 2)),
            Some(pixeles.clone())
        );
        assert!(bufer.copiar((0, 0), (2, 2), 2, 2));
        assert_eq!(
            bufer.recorte(&Region::nueva(2, 2, 2, 2)),
            Some(pixeles.clone())
        );
        assert!(!bufer.copiar((3, 0), (0, 0), 2, 2));
    }

    #[test]
    fn las_ordenes_del_visor_se_validan() {
        assert_eq!(
            interpretar_orden("kc,97,KeyA,1", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Tecla {
                codigo: "KeyA".to_string(),
                keysym: 97,
                abajo: true
            }))
        );
        assert_eq!(
            interpretar_orden("kc,65293,,0", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Tecla {
                codigo: String::new(),
                keysym: 65293,
                abajo: false
            }))
        );
        assert!(interpretar_orden("kc,0,,1", 800, 600).is_none());
        assert!(interpretar_orden("kc,97,TeclaInventada,1", 800, 600).is_none());
        assert_eq!(
            interpretar_orden("m,10,20", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Raton {
                x: 10,
                y: 20
            }))
        );
        assert!(interpretar_orden("m,900,20", 800, 600).is_none());
        assert_eq!(
            interpretar_orden("b,1,0", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Boton {
                boton: 1,
                abajo: false
            }))
        );
        // Los botones 8 y 9 (atrás/adelante del navegador) se aceptan: RDP los
        // usa; RFB es quien descarta el 9 (m2).
        assert_eq!(
            interpretar_orden("b,8,1", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Boton {
                boton: 8,
                abajo: true
            }))
        );
        assert!(interpretar_orden("b,9,0", 800, 600).is_some());
        assert!(interpretar_orden("b,10,0", 800, 600).is_none());
        assert_eq!(
            interpretar_orden("sp,1.5,-2.5", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Rueda {
                dx: 1.5,
                dy: -2.5
            }))
        );
        assert!(interpretar_orden("sp,NaN,1", 800, 600).is_none());
        assert_eq!(
            interpretar_orden("r", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::SoltarTodo))
        );
        assert_eq!(
            interpretar_orden("c,hola, mundo", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Portapapeles(
                "hola, mundo".to_string()
            )))
        );
        assert_eq!(
            interpretar_orden("sz,800,600", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::Tamano {
                ancho: 800,
                alto: 600
            }))
        );
        assert!(interpretar_orden("sz,100,600", 800, 600).is_none());
        assert!(interpretar_orden("sz,9000,600", 800, 600).is_none());
        assert_eq!(
            interpretar_orden("v,0", 800, 600),
            Some(InstruccionVisor::Visible(false))
        );
        assert_eq!(
            interpretar_orden("v,1", 800, 600),
            Some(InstruccionVisor::Visible(true))
        );
        assert_eq!(
            interpretar_orden("kf", 800, 600),
            Some(InstruccionVisor::Repintar)
        );
        assert_eq!(
            interpretar_orden("cad", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::CtrlAltSupr))
        );
        assert_eq!(
            interpretar_orden("win", 800, 600),
            Some(InstruccionVisor::Orden(OrdenEscritorio::TeclaWindows))
        );
        assert_eq!(
            interpretar_orden("po,1234", 800, 600),
            Some(InstruccionVisor::Latencia(1234))
        );
        assert_eq!(
            interpretar_orden("rec", 800, 600),
            Some(InstruccionVisor::Reconectar)
        );
        assert!(interpretar_orden("inventado", 800, 600).is_none());
        assert!(interpretar_orden("", 800, 600).is_none());
    }

    #[test]
    fn la_salida_aplica_contrapresion_de_ocho_megas() {
        let (salida, mut receptor) = SalidaVisor::nueva();
        let medio = vec![0u8; COLA_MAX / 2];
        assert!(salida.binario(medio.clone()));
        assert!(salida.binario(medio.clone()));
        assert_eq!(salida.pendiente(), COLA_MAX);
        // Con 8 MB pendientes no se encola nada más: el sucio espera.
        assert!(!salida.binario(medio.clone()));
        // La tarea del WebSocket escribe y descuenta: vuelve a haber sitio.
        let _ = receptor.try_recv();
        salida.descontar(COLA_MAX / 2);
        assert!(salida.binario(medio));
        assert!(!salida.cerrada());
        salida.cerrar();
        assert!(salida.cerrada());
    }

    #[test]
    fn ningun_mensaje_individual_pasa_de_la_cola() {
        // C2: también el primero; un rectángulo mayor se trocea antes.
        let (salida, mut receptor) = SalidaVisor::nueva();
        assert!(!salida.binario(vec![0u8; COLA_MAX + 1]));
        assert_eq!(salida.pendiente(), 0);
        assert!(salida.binario(vec![0u8; COLA_MAX]));
        assert_eq!(salida.pendiente(), COLA_MAX);
        let _ = receptor.try_recv();
        salida.descontar(COLA_MAX);
        assert_eq!(salida.pendiente(), 0);
    }

    #[test]
    fn los_textos_tambien_se_descuentan() {
        // C2: los textos (ping, clip…) suman en la cola; el consumidor tiene
        // que descontarlos como los binarios o el visor se congela.
        let (salida, mut receptor) = SalidaVisor::nueva();
        let clip = "x".repeat(1024 * 1024);
        for _ in 0..4 {
            assert!(salida.texto(clip.clone()));
            let mensaje = receptor.try_recv().expect("mensaje encolado");
            let MensajeVisor::Texto(texto) = mensaje else {
                panic!("se esperaba un texto");
            };
            salida.descontar(texto.len());
        }
        assert_eq!(salida.pendiente(), 0);
        // Y sigue habiendo sitio para la imagen.
        assert!(salida.binario(vec![0u8; 1024]));
    }

    #[test]
    fn los_trozos_cubren_la_zona_y_caben_en_la_cola() {
        let region = Region::nueva(0, 0, 2048, 2048);
        let partes = trozos(&region, COLA_MAX);
        assert!(partes.len() >= 3, "un rectángulo de 16 MB se trocea");
        let mut filas = 0u32;
        for parte in &partes {
            assert_eq!(parte.ancho, 2048);
            assert_eq!(parte.x, 0);
            let bytes = 9 + usize::from(parte.ancho) * usize::from(parte.alto) * 4;
            assert!(bytes <= COLA_MAX, "cada trozo cabe en la cola");
            filas += u32::from(parte.alto);
        }
        assert_eq!(filas, 2048);
        // Una zona pequeña no se trocea.
        assert_eq!(trozos(&Region::nueva(0, 0, 10, 10), COLA_MAX).len(), 1);
    }

    #[test]
    fn el_recorte_deja_la_zona_dentro_del_bufer() {
        assert_eq!(
            recortar_region(&Region::nueva(10, 10, 20, 20), 100, 100),
            Some(Region::nueva(10, 10, 20, 20))
        );
        assert_eq!(
            recortar_region(&Region::nueva(90, 90, 20, 20), 100, 100),
            Some(Region::nueva(90, 90, 10, 10))
        );
        assert_eq!(
            recortar_region(&Region::nueva(200, 0, 10, 10), 100, 100),
            None
        );
        assert_eq!(recortar_region(&Region::nueva(0, 0, 0, 10), 100, 100), None);
    }

    #[test]
    fn el_texto_del_portapapeles_tiene_limite() {
        let grande = format!("c,{}", "a".repeat(PORTAPAPELES_MAX + 1));
        assert!(interpretar_orden(&grande, 800, 600).is_none());
        let justo = format!("c,{}", "a".repeat(PORTAPAPELES_MAX));
        assert!(interpretar_orden(&justo, 800, 600).is_some());
        let enorme = "a".repeat(PORTAPAPELES_MAX + 100);
        assert!(interpretar_orden(&enorme, 800, 600).is_none());
    }
}
