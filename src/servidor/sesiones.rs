//! Registro de sesiones del servidor: ciclo de vida completo de cada sesión
//! SSH (apertura con diálogos reenviados al solicitante, teclas, pantallas,
//! tamaños compartidos, caída, reconexión automática y cierre) y la lista que
//! se difunde a los clientes.
//!
//! Desde la Fase 15 cada pestaña tiene un `PantallaEmulada` del núcleo, el
//! único que responde a las consultas del remoto (en el mismo lote en el que
//! llegan) y el que guarda el historial. El servidor custodia también el
//! título, los avisos, el foco agregado de las ventanas, el portapapeles del
//! remoto y el nombre de la sesión de tmux.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use russh::client::Handle;
use russh::{Channel, ChannelMsg};
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{info, warn};
use zeroize::Zeroizing;

use crate::conexion::cliente::Cliente;
use crate::conexion::EventoConexion;
use crate::config::Rutas;
use crate::modelo::{EstadoSesion, Host};
use crate::protocolo::{
    ColoresTema, EstadoSesionRemota, InfoAviso, InfoReconexion, InfoSesion, MensajeServidor,
    Secreto,
};
use magi_nucleo::conexion::reconexion::{self, Causa};
use magi_nucleo::terminal::emulador::PantallaEmulada;
use magi_nucleo::terminal::historial::lineas_de_historial;

use super::cliente_remoto::Tamano;
use super::conexiones::{self, ConexionTomada, PoliticaConexion, UsoCanal};
use super::difusion;
use super::EstadoServidor;

/// Tiempo máximo que espera el servidor una decisión del solicitante.
const TIMEOUT_DECISION: Duration = Duration::from_secs(5 * 60);

/// Plazo para abrir el canal de la pestaña (pty y shell) sobre una conexión
/// ya autenticada.
const PLAZO_CANAL: Duration = Duration::from_secs(10);

/// Plazo de un intento de reconexión automática (`5.2: 20 s por intento).
const PLAZO_INTENTO_RECONEXION: Duration = Duration::from_secs(20);

/// Un aviso del remoto como mucho cada este tiempo, por pestaña (`7.13).
const AVISO_SEG: i64 = 5;

/// Lo que dura la última petición de portapapeles de un host sin permiso.
pub const PORTAPAPELES_PENDIENTE_SEG: i64 = 60;

/// Tope de un comando inicial (lo manda el cliente y se escribe en la shell).
pub const TOPE_COMANDO_INICIAL: usize = 64 * 1024;

/// Longitud máxima del nombre de una pestaña (`7.13).
pub const NOMBRE_PESTANA_MAX: usize = 32;

/// Tope de columnas que acepta el servidor de un tamaño que llega de fuera.
/// Coincide con el del núcleo, que recorta el parser a 1000×2000: un tamaño
/// desmedido no puede llegar al PTY ni hinchar el historial ni el códec.
pub const COLS_MAX: u16 = 1000;

/// Tope de filas que acepta el servidor de un tamaño que llega de fuera.
pub const FILAS_MAX: u16 = 2000;

/// Sanea un tamaño de terminal que llega de fuera (mín. 2×1 y con los topes
/// del servidor). El cliente ya manda el tamaño del contenido del panel, pero
/// no se fía de él.
#[must_use]
pub fn sanear_tamano(tamano: Tamano) -> Tamano {
    Tamano {
        cols: tamano.cols.clamp(2, COLS_MAX),
        filas: tamano.filas.clamp(1, FILAS_MAX),
    }
}

/// Generación de cada diálogo pendiente: el temporizador de uno no puede
/// borrar el siguiente que ocupe el mismo id (otro intento de frase, p. ej.).
static GENERACION_DIALOGO: AtomicU64 = AtomicU64::new(1);

/// La pantalla de una pestaña, compartida con las tareas que la capturan.
pub type PantallaCompartida = Arc<std::sync::Mutex<PantallaEmulada>>;

/// Comandos que el estado del servidor encola hacia la tarea de una sesión.
#[derive(Debug)]
pub enum ComandoSesion {
    Teclas(Vec<u8>),
    /// Tamaño mínimo recalculado que debe aplicarse al remoto y al parser,
    /// con el tope de historial que le toca a esas columnas.
    AplicarTamano {
        cols: u16,
        filas: u16,
        historial: usize,
    },
    Cerrar,
}

/// Un diálogo de conexión esperando la respuesta de un cliente.
pub enum DialogoPendiente {
    Huella(oneshot::Sender<bool>),
    Frase(oneshot::Sender<Option<Zeroizing<String>>>),
    Contrasena(oneshot::Sender<Option<(Zeroizing<String>, bool)>>),
    /// Contraseña de una conexión automática: solo del llavero del
    /// solicitante, que contesta sin diálogo.
    Llavero(oneshot::Sender<Option<Zeroizing<String>>>),
}

/// Una decisión pendiente: el diálogo y el cliente que puede responderla. El
/// solicitante se guarda aquí y no se busca en la sesión porque una apertura
/// de canal SFTP (Fase 4) también dialoga y no tiene sesión propia.
pub struct Pendiente {
    pub solicitante: u32,
    pub dialogo: DialogoPendiente,
    /// Para que el temporizador solo retire el diálogo que programó.
    pub generacion: u64,
}

/// Respuesta de un diálogo que llega por el protocolo desde un cliente.
pub enum DecisionDialogo {
    Huella(bool),
    Frase(Secreto),
    Contrasena(Secreto, bool),
}

/// Una sesión SSH custodiada por el servidor.
pub struct Sesion {
    pub id: u32,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    pub estado: EstadoSesionRemota,
    pub motivo: Option<String>,
    pub identidad: String,
    /// Posición en la lista de pestañas (la fija `MoverSesion`).
    pub orden: u64,
    /// Adjuntos: cliente → tamaño de ventana que reportó.
    pub adjuntos: HashMap<u32, Tamano>,
    pub solicitante: u32,
    pub abierta_en: i64,
    pub ultima_actividad: i64,
    pub actividad_no_vista: bool,
    /// Conexión SSH de esta sesión (para `Ejecutar` y cerrarla si es propia).
    pub handle: Option<Arc<Handle<Cliente>>>,
    /// Cancela la apertura en curso (la pestaña se cierra, la ventana que la
    /// pidió se va o el servidor se apaga): suelta el cerrojo del host y la
    /// conexión a medias en vez de seguir dialogando para nadie.
    pub cancelar_apertura: tokio_util::sync::CancellationToken,
    /// Está reabriendo tras una caída: sigue contando para el ciclo automático
    /// de los túneles como cuando estaba caída.
    pub reconectando: bool,
    /// Lo que se escribe al abrir la shell; al reconectar, solo lo que
    /// `repetir` (el snippet al conectar).
    pub comandos_iniciales: Vec<crate::protocolo::ComandoInicial>,
    /// Tamaño vigente (último aplicado al remoto).
    pub tamano: Tamano,
    /// Ventana que explica el relleno de cada adjunto, tal como se le envió la
    /// última vez (una pestaña compartida puede tener las columnas impuestas
    /// por una ventana y las filas por otra).
    pub ventana_minima: HashMap<u32, Option<u32>>,
    /// Pantalla de la pestaña: historial y emulador que responde al remoto.
    pub pantalla: PantallaCompartida,
    pub tx_comandos: mpsc::UnboundedSender<ComandoSesion>,
    // ---------------------------------------------------------- Fase 15
    /// Título que puso el programa remoto (OSC 0 u OSC 2), saneado.
    pub titulo: Option<String>,
    /// Último aviso del remoto o del servidor (campana, OSC 9, OSC 777…).
    pub aviso: Option<InfoAviso>,
    /// Época del último aviso admitido (uno cada 5 s por pestaña).
    pub ultimo_aviso_en: i64,
    /// Nombre de la sesión de tmux, si la pestaña es persistente.
    pub tmux: Option<String>,
    /// Estado de la reconexión automática, mientras dura.
    pub reconexion: Option<InfoReconexion>,
    /// Ventanas que tienen esta pestaña en el panel enfocado y el terminal
    /// exterior enfocado (el foco se agrega: si alguna la tiene, la hay).
    pub enfocadas: HashSet<u32>,
    /// Última ventana que la tuvo enfocada (destino de `Portapapeles`).
    pub ventana_enfocada: Option<u32>,
    /// Colores del tema de la primera ventana que los mandó (OSC 10/11/12).
    pub colores: Option<ColoresTema>,
    /// Última petición de portapapeles del remoto sin permiso (texto y época
    /// en la que caduca).
    pub portapapeles_pendiente: Option<(String, i64)>,
    /// Despierta al bucle de reconexión para intentarlo ya (prefijo r).
    pub adelantar: Option<watch::Sender<u64>>,
    /// La pestaña está concedida en una escritura en varios activa (la marca
    /// «»» de §7.14, que ven todas las ventanas).
    pub escritura_varios: bool,
}

impl Sesion {
    /// Resumen para la difusión `Sesiones`.
    pub fn info(&self) -> InfoSesion {
        InfoSesion {
            id: self.id,
            nombre: self.nombre.clone(),
            host_id: self.host_id,
            host_nombre: self.host_nombre.clone(),
            estado: self.estado,
            motivo: self.motivo.clone(),
            identidad: self.identidad.clone(),
            abierta_en: self.abierta_en,
            ventanas: self.adjuntos.len() as u32,
            actividad_no_vista: self.actividad_no_vista,
            reconexion: self.reconexion.clone(),
            titulo: self.titulo.clone(),
            aviso: self.aviso.clone(),
            tmux: self.tmux.clone(),
            escritura_varios: self.escritura_varios,
        }
    }

    /// ¿La pestaña está abierta?
    pub fn abierta(&self) -> bool {
        self.estado == EstadoSesionRemota::Abierta
    }

    /// ¿Alguna ventana la tiene enfocada?
    pub fn enfocada(&self) -> bool {
        !self.enfocadas.is_empty()
    }

    /// Anota un aviso local (del servidor) o del remoto, con el tope de uno
    /// cada 5 s por pestaña. Devuelve si se admitió (para difundir la lista).
    pub fn anotar_aviso(&mut self, texto: &str, hora: i64) -> bool {
        if hora - self.ultimo_aviso_en < AVISO_SEG {
            return false;
        }
        self.ultimo_aviso_en = hora;
        self.aviso = Some(InfoAviso {
            texto: texto.to_string(),
            hora,
        });
        true
    }
}

/// Construye la lista de sesiones en el orden de las pestañas (el que fija
/// `MoverSesion`) para la difusión.
pub fn lista(sesiones: &HashMap<u32, Sesion>) -> Vec<InfoSesion> {
    let mut ordenadas: Vec<&Sesion> = sesiones.values().collect();
    ordenadas.sort_by_key(|sesion| (sesion.orden, sesion.id));
    ordenadas.iter().map(|sesion| sesion.info()).collect()
}

/// Difunde la lista completa de sesiones a todos los clientes. Un cambio de
/// verdad sale ya y descarta la difusión cosmética pendiente (título/aviso).
pub fn difundir_lista(estado: &mut EstadoServidor) {
    estado.difusion_sesiones.limpiar();
    let mensajes = lista(&estado.sesiones);
    difusion::difundir(
        &estado.clientes,
        MensajeServidor::Sesiones { lista: mensajes },
    );
}

/// Pestañas de un host que cuentan para el ciclo automático de los túneles:
/// las abiertas, las caídas y las que están reconectando. Los túneles y el
/// canal SFTP van aparte.
pub fn canales_de_pestana(estado: &EstadoServidor, host_id: i64) -> usize {
    estado
        .sesiones
        .values()
        .filter(|sesion| {
            sesion.host_id == host_id
                && (matches!(
                    sesion.estado,
                    EstadoSesionRemota::Abierta | EstadoSesionRemota::Caida
                ) || (sesion.estado == EstadoSesionRemota::Abriendo && sesion.reconectando))
        })
        .count()
}

/// Nombre de pestaña para una sesión nueva al host: `host` si no hay otra
/// sesión viva, `host (n)` con el menor n ≥ 2 libre en caso contrario.
pub fn nombre_de_pestaña(sesiones: &HashMap<u32, Sesion>, host: &Host) -> String {
    let del_host: Vec<&Sesion> = sesiones
        .values()
        .filter(|sesion| sesion.host_id == host.id)
        .collect();
    if del_host.is_empty() {
        return host.nombre.clone();
    }
    let usados: Vec<u32> = del_host
        .iter()
        .filter_map(|sesion| {
            sesion
                .nombre
                .strip_prefix(&format!("{} (", host.nombre))
                .and_then(|resto| resto.strip_suffix(')'))
                .and_then(|numero| numero.parse::<u32>().ok())
        })
        .collect();
    let mut numero = 2;
    while usados.contains(&numero) {
        numero += 1;
    }
    format!("{} ({numero})", host.nombre)
}

/// Sanea y hace único el nombre de una pestaña (`7.13): 1-32 caracteres sin
/// controles y, si ya lo tiene otra, se le añade « (n)».
#[must_use]
pub fn nombre_renombrado(
    sesiones: &HashMap<u32, Sesion>,
    sesion_id: u32,
    nombre: &str,
) -> Option<String> {
    let limpio: String = nombre
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(NOMBRE_PESTANA_MAX)
        .collect();
    let limpio = limpio.trim();
    if limpio.is_empty() {
        return None;
    }
    let usado_por_otra = |candidato: &str| {
        sesiones
            .values()
            .any(|sesion| sesion.id != sesion_id && sesion.nombre == candidato)
    };
    if !usado_por_otra(limpio) {
        return Some(limpio.to_string());
    }
    let mut numero = 2;
    loop {
        let sufijo = format!(" ({numero})");
        let base: String = limpio
            .chars()
            .take(NOMBRE_PESTANA_MAX - sufijo.chars().count())
            .collect();
        let candidato = format!("{base}{sufijo}");
        if !usado_por_otra(&candidato) {
            return Some(candidato);
        }
        numero += 1;
    }
}

/// Tamaño de la sesión: mínimo de los adjuntos, columna a columna y fila a
/// fila; sin adjuntos se conserva el último. Solo cuentan los adjuntos: si la
/// ventana pequeña crece o se va, el tamaño sube (antes partía del último
/// aplicado y nunca podía crecer).
pub fn tamano_minimo(adjuntos: &HashMap<u32, Tamano>, actual: Tamano) -> Tamano {
    adjuntos
        .values()
        .copied()
        .reduce(|menor, tamano| Tamano {
            cols: menor.cols.min(tamano.cols),
            filas: menor.filas.min(tamano.filas),
        })
        .unwrap_or(actual)
}

/// Ventana que explica el relleno de `destino`: si le sobran columnas, la
/// que impone las columnas; si solo le sobran filas, la que impone las filas;
/// a igualdad, la de id menor. Si no le sobra nada, es ella misma (el aviso
/// «mín. esta ventana» solo se pinta cuando hay relleno).
pub fn ventana_minima_para(
    adjuntos: &HashMap<u32, Tamano>,
    tamano: Tamano,
    destino: u32,
) -> Option<u32> {
    let propio = adjuntos.get(&destino)?;
    let impone = |columnas: bool| {
        adjuntos
            .iter()
            .filter(|(_, otro)| {
                if columnas {
                    otro.cols == tamano.cols
                } else {
                    otro.filas == tamano.filas
                }
            })
            .map(|(cliente_id, _)| *cliente_id)
            .min()
    };
    if propio.cols > tamano.cols {
        impone(true)
    } else if propio.filas > tamano.filas {
        impone(false)
    } else {
        Some(destino)
    }
}

// ---------------------------------------------------------------- pantalla

/// Crea la pantalla de una pestaña: historial de vt100 con los topes
/// configurados para esas columnas, y el emulador del núcleo respondiendo a
/// las consultas del remoto (solo el servidor responde).
#[must_use]
pub fn pantalla_nueva(filas: u16, cols: u16, historial: usize) -> PantallaCompartida {
    let mut pantalla = PantallaEmulada::nueva(filas.max(1), cols.max(1), historial, true);
    pantalla
        .emulador_mut()
        .fijar_version(env!("CARGO_PKG_VERSION"));
    Arc::new(std::sync::Mutex::new(pantalla))
}

/// Tope de historial que le toca a una pestaña con estas columnas.
#[must_use]
pub fn tope_historial(historial_lineas: u64, historial_mib: u64, cols: u16) -> usize {
    lineas_de_historial(historial_lineas as usize, historial_mib, cols)
}

// ---------------------------------------------------------------- apertura

/// Datos fijos para abrir una sesión, calculados bajo bloqueo.
pub struct DatosApertura {
    pub host: Host,
    pub todos_los_hosts: HashMap<i64, Host>,
    pub rutas: Rutas,
    /// Pantalla del servidor para la sesión (se conserva al reconectar).
    pub pantalla: PantallaCompartida,
    pub cols: u16,
    pub filas: u16,
    /// Topes de historial de `[terminal]` (se recalculan al redimensionar).
    pub historial_lineas: u64,
    pub historial_mib: u64,
    pub sesion_id: u32,
    pub solicitante: u32,
    /// Es una reconexión: el id y el nombre ya existen. Se escribe la línea de
    /// «reconectada» y solo se repiten los comandos con `repetir`.
    pub reconexion: bool,
    /// Nombre de tmux pedido (pestaña nueva desde la lista) o el de la sesión
    /// al reconectar. `None`: lo decide `tmux_persistente`.
    pub tmux: Option<String>,
    /// La de la sesión: cancelarla aborta la apertura.
    pub cancelar: tokio_util::sync::CancellationToken,
    /// Textos que se escriben en la shell, en orden (al reconectar, solo los
    /// que `repetir`).
    pub comandos: Vec<crate::protocolo::ComandoInicial>,
}

/// Lanza la tarea de una sesión: toma la conexión (la del pool con
/// `multiplexar`, o una propia; los diálogos van al solicitante), sirve el
/// bucle de teclas y pantallas y, si la red cae y el host lo permite, la
/// reconecta sola hasta `reconexion_max_min`. La sesión ya debe estar
/// insertada en el estado.
pub fn lanzar(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: DatosApertura,
    rx_comandos: mpsc::UnboundedReceiver<ComandoSesion>,
) {
    tokio::spawn(async move {
        abrir_y_servir(estado, datos, rx_comandos).await;
    });
}

/// Bucle de vida de una pestaña: abre, sirve y reintenta si la red cae y el
/// host tiene `reconectar`. Los intentos automáticos no dialogan.
async fn abrir_y_servir(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    mut datos: DatosApertura,
    mut rx_comandos: mpsc::UnboundedReceiver<ComandoSesion>,
) {
    let sesion_id = datos.sesion_id;
    let host_id = datos.host.id;
    // ¿Este ciclo viene de una reconexión (manual o automática)?
    let mut reintento = datos.reconexion;
    // El último intento de reconexión fue automático (sin diálogos).
    let mut auto = false;
    let mut intento_auto = 0u32;
    let mut directa: Option<ConexionTomada> = None;
    loop {
        // 1. Conexión (la reutiliza el intento de reconexión si ya la dejó).
        let tomada = match directa.take() {
            Some(tomada) => tomada,
            None => {
                let politica = PoliticaConexion::Interactiva {
                    solicitante: datos.solicitante,
                    id_solicitud: sesion_id,
                };
                match abrir_conexion(&estado, &datos, politica).await {
                    Ok(tomada) => tomada,
                    Err(motivo) => {
                        if auto {
                            cerrar_intento(&estado, sesion_id, &motivo, intento_auto).await;
                        } else {
                            apertura_fallida(&estado, &datos, &motivo).await;
                        }
                        return;
                    }
                }
            }
        };
        let handle = tomada.handle.clone();

        // El parser se pone al tamaño con el que se va a abrir el PTY (al
        // reconectar pudo cambiar mientras estaba caída).
        if let Ok(mut pantalla) = datos.pantalla.lock() {
            let historial = tope_historial(datos.historial_lineas, datos.historial_mib, datos.cols);
            pantalla.redimensionar(datos.filas, datos.cols, historial);
        }

        // 2. tmux: listar/comprobar y abrir la sesión del host. Es opcional:
        //    si algo va mal, se sigue con un shell normal y un aviso.
        let preparado = preparar_tmux(&estado, sesion_id, &datos, &handle, reintento).await;
        let tmux_nuevo = preparado.as_ref().is_some_and(|(_, nuevo)| *nuevo);
        let tmux_nombre = preparado.map(|(nombre, _)| nombre);

        // 3. Canal con PTY y COLORTERM. Si falla, la conexión se suelta.
        let mut canal = match abrir_con_plazo_terminal(&handle, datos.cols, datos.filas).await {
            Ok(canal) => canal,
            Err(motivo) => {
                tomada.soltar().await;
                if auto {
                    cerrar_intento(&estado, sesion_id, &motivo, intento_auto).await;
                } else {
                    apertura_fallida(&estado, &datos, &motivo).await;
                }
                return;
            }
        };

        // 4. Comandos iniciales: primero la orden de tmux (solo si la sesión
        //    es nueva), después lo que diga el cliente. Al engancharse a una
        //    sesión de tmux que ya existía no se escribe nada: el programa
        //    sigue dentro.
        let es_reconexion = reintento;
        let comandos: Vec<String> = if es_reconexion {
            datos
                .comandos
                .iter()
                .filter(|comando| comando.repetir)
                .map(|comando| comando.texto.clone())
                .collect()
        } else {
            datos
                .comandos
                .iter()
                .map(|comando| comando.texto.clone())
                .collect()
        };
        let escribir_comandos = match tmux_nombre.as_deref() {
            Some(nombre) => match super::tmux::orden_abrir(nombre) {
                Some(linea) => {
                    let _ = escribir_canal(&mut canal, linea.as_bytes()).await;
                    tmux_nuevo
                }
                None => true,
            },
            None => true,
        };
        if escribir_comandos {
            for texto in &comandos {
                let escrito =
                    tokio::time::timeout(PLAZO_CANAL, canal.data_bytes(texto.clone().into_bytes()))
                        .await;
                if !matches!(escrito, Ok(Ok(()))) {
                    warn!(sesion = sesion_id, "no se pudo escribir el comando inicial");
                    break;
                }
            }
        }

        // 5. Pestaña abierta: estado, difusión y anotación.
        let primera_en_conexion =
            marcar_abierta(&estado, sesion_id, &tomada, tmux_nombre.clone()).await;
        marcar_conexion_bd(
            &estado,
            host_id,
            es_reconexion,
            primera_en_conexion,
            &datos.host,
            &tomada.identidad,
            auto,
            intento_auto,
        )
        .await;
        if es_reconexion {
            anunciar_reconectada(&estado, sesion_id, tmux_nombre.as_deref()).await;
        }
        info!(
            sesion = sesion_id,
            host = %datos.host.nombre,
            reutilizada = !tomada.nueva,
            "sesión abierta en el servidor"
        );
        difundir_lista(&mut *estado.lock().await);
        let estado_tuneles = estado.clone();
        tokio::spawn(async move {
            super::tuneles::canales_cambiaron(&estado_tuneles, host_id).await;
        });

        // 6. Servir hasta que se cierre o la conexión caiga.
        let fin = bucle_sesion(
            &estado,
            sesion_id,
            &mut canal,
            &datos.pantalla,
            &mut rx_comandos,
        )
        .await;
        tomada.soltar().await;
        match fin {
            FinSesion::Cerrada => return,
            FinSesion::Caida(motivo) => {
                let causa = reconexion::clasificar(&motivo);
                let reintentar = causa.recuperable() && datos.host.reconectar;
                if !reintentar {
                    caida(&estado, sesion_id, &motivo).await;
                    return;
                }
                // 7. Reconexión automática, sin diálogos.
                match esperar_y_reintentar(&estado, &mut datos, &mut rx_comandos, causa, &motivo)
                    .await
                {
                    Ok((tomada, manual, intento)) => {
                        directa = Some(tomada);
                        reintento = true;
                        auto = !manual;
                        intento_auto = intento;
                    }
                    Err(FinReintento::Cerrada) => return,
                    Err(FinReintento::Agotada(motivo)) => {
                        caida(&estado, sesion_id, &motivo).await;
                        return;
                    }
                }
            }
        }
    }
}

/// Abre la conexión (o reutiliza la del pool), con los diálogos al
/// solicitante que diga la política. El canal de la pestaña se abre después.
async fn abrir_conexion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    politica: PoliticaConexion,
) -> Result<ConexionTomada, String> {
    tokio::select! {
        tomada = conexiones::conexion_para_canal(
            estado,
            &datos.host,
            &datos.todos_los_hosts,
            &datos.rutas,
            politica,
            UsoCanal::Pestana,
        ) => tomada,
        _ = datos.cancelar.cancelled() => Err("la pestaña se cerró".to_string()),
    }
}

/// Abre el canal con PTY de la pestaña y pide `COLORTERM=truecolor` (el host
/// puede ignorarlo). El plazo lo pone quien llama.
async fn abrir_con_plazo_terminal(
    handle: &Arc<Handle<Cliente>>,
    cols: u16,
    filas: u16,
) -> Result<Channel<russh::client::Msg>, String> {
    let apertura = handle.clone();
    let abrir = async move {
        let canal = apertura
            .channel_open_session()
            .await
            .map_err(|error| format!("no se pudo abrir el canal de la pestaña: {error}"))?;
        // want_reply = false: un host que no admita COLORTERM no debe
        // retrasar ni romper la apertura.
        let _ = canal.set_env(false, "COLORTERM", "truecolor").await;
        canal
            .request_pty(
                false,
                "xterm-256color",
                u32::from(cols),
                u32::from(filas),
                0,
                0,
                &[],
            )
            .await
            .map_err(|error| format!("no se pudo pedir el PTY: {error}"))?;
        canal
            .request_shell(true)
            .await
            .map_err(|error| format!("no se pudo pedir la shell: {error}"))?;
        Ok::<Channel<russh::client::Msg>, String>(canal)
    };
    match tokio::time::timeout(PLAZO_CANAL, abrir).await {
        Ok(Ok(canal)) => Ok(canal),
        Ok(Err(motivo)) => Err(motivo),
        Err(_) => Err("el host no abrió el canal de la pestaña a tiempo".to_string()),
    }
}

/// Escribe en el canal con plazo (para los comandos iniciales).
async fn escribir_canal(
    canal: &mut Channel<russh::client::Msg>,
    bytes: &[u8],
) -> Result<(), String> {
    match tokio::time::timeout(PLAZO_CANAL, canal.data_bytes(bytes.to_vec())).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(format!("se perdió la conexión: {error}")),
        Err(_) => Err("el host no aceptó la escritura a tiempo".to_string()),
    }
}

/// Prepara la sesión de tmux antes de abrir el PTY. Devuelve el nombre y si
/// hay que crearla (para escribir el snippet al conectar). Nunca falla: si
/// algo va mal, se abre un shell normal con un aviso en la pestaña.
async fn preparar_tmux(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    datos: &DatosApertura,
    handle: &Arc<Handle<Cliente>>,
    es_reconexion: bool,
) -> Option<(String, bool)> {
    let pedido = datos.tmux.clone();
    if pedido.is_none() && !datos.host.tmux_persistente {
        return None;
    }
    let avisar = |estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, texto: String| {
        let estado = estado.clone();
        tokio::spawn(async move {
            let mut estado_bloqueado = estado.lock().await;
            if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
                if sesion.anotar_aviso(&texto, Utc::now().timestamp()) {
                    difundir_lista(&mut estado_bloqueado);
                }
            }
        });
    };
    if es_reconexion {
        let nombre = pedido?;
        match super::tmux::comprobar(handle, &nombre).await {
            Ok((version, existe)) if version.is_some() && existe => Some((nombre, false)),
            Ok((version, _)) => {
                avisar(
                    estado,
                    if version.is_none() {
                        format!("sin tmux en «{}»: shell normal", datos.host.nombre)
                    } else {
                        format!(
                            "la sesión de tmux «{nombre}» se perdió: shell normal en «{}»",
                            datos.host.nombre
                        )
                    },
                );
                None
            }
            Err(motivo) => {
                warn!(sesion = sesion_id, "no se pudo comprobar tmux: {motivo}");
                avisar(
                    estado,
                    format!(
                        "no se pudo comprobar tmux en «{}»: shell normal ({motivo})",
                        datos.host.nombre
                    ),
                );
                None
            }
        }
    } else if let Some(nombre) = pedido {
        // Pedida desde la lista de tmux: se engancha tal cual.
        Some((nombre, false))
    } else {
        // Pestaña nueva con sesión persistente: se busca un nombre libre.
        match super::tmux::listar(handle).await {
            Ok(listado) => {
                let Some(_version) = listado.tmux else {
                    avisar(
                        estado,
                        format!("sin tmux en «{}»: shell normal", datos.host.nombre),
                    );
                    return None;
                };
                let existentes: Vec<String> = listado
                    .sesiones
                    .iter()
                    .map(|sesion| sesion.nombre.clone())
                    .collect();
                let equipo = super::tmux::nombre_equipo(estado).await;
                let nombre = magi_nucleo::terminal::tmux::nombre_nuevo(&equipo, &existentes);
                Some((nombre, true))
            }
            Err(motivo) => {
                warn!(sesion = sesion_id, "no se pudo listar tmux: {motivo}");
                avisar(
                    estado,
                    format!(
                        "no se pudo hablar con tmux en «{}»: shell normal",
                        datos.host.nombre
                    ),
                );
                None
            }
        }
    }
}

/// Marca la sesión abierta con la conexión tomada y su identidad. Devuelve si
/// es la primera pestaña abierta de esa conexión (para no repetir la
/// anotación de conexión).
async fn marcar_abierta(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    tomada: &ConexionTomada,
    tmux: Option<String>,
) -> bool {
    let mut estado_bloqueado = estado.lock().await;
    let otra_en_la_misma = estado_bloqueado.sesiones.values().any(|sesion| {
        sesion.id != sesion_id
            && sesion.estado == EstadoSesionRemota::Abierta
            && sesion
                .handle
                .as_ref()
                .is_some_and(|handle| Arc::ptr_eq(handle, &tomada.handle))
    });
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
        return false;
    };
    sesion.estado = EstadoSesionRemota::Abierta;
    sesion.reconectando = false;
    sesion.reconexion = None;
    sesion.adelantar = None;
    sesion.motivo = None;
    sesion.identidad = tomada.identidad.clone();
    sesion.abierta_en = Utc::now().timestamp();
    sesion.handle = Some(tomada.handle.clone());
    if tmux.is_some() {
        sesion.tmux = tmux;
    }
    !otra_en_la_misma
}

/// Anota en REGISTRO la conexión abierta o la reconexión, y marca el host.
#[allow(clippy::too_many_arguments)]
async fn marcar_conexion_bd(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    reconexion: bool,
    primera_en_conexion: bool,
    host: &Host,
    identidad: &str,
    auto: bool,
    intento_auto: u32,
) {
    let bd = estado.lock().await.bd.clone();
    if reconexion || primera_en_conexion {
        let detalle = if reconexion && auto {
            format!("automática · intento {intento_auto} · «{}»", host.nombre)
        } else {
            format!("sesión abierta con «{}» · {identidad}", host.nombre)
        };
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: if reconexion {
                crate::registro::SESION_RECONECTADA.to_string()
            } else {
                crate::registro::CONEXION_ABIERTA.to_string()
            },
            host_id: Some(host_id),
            identidad_id: None,
            detalle,
            resultado: crate::modelo::ResultadoRegistro::Ok,
        });
    }
    let _ = bd.send(super::OrdenBd::MarcarConexion { host_id });
    let _ = bd.send(super::OrdenBd::MarcarEstado {
        host_id,
        estado: Some(crate::modelo::UltimoEstado::Ok),
    });
}

/// Añade al parser del servidor (y a las ventanas adjuntas) la línea atenuada
/// de la reconexión. No llega nunca al remoto.
async fn anunciar_reconectada(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    tmux: Option<&str>,
) {
    let hora = chrono::Local::now().format("%H:%M");
    let detalle = match tmux {
        Some(nombre) => format!("tmux {nombre}"),
        None => "shell nuevo".to_string(),
    };
    let texto = format!("\r\n\x1b[2m── reconectada {hora} · {detalle} ──\x1b[0m\r\n");
    let (pantalla, ids) = {
        let estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.sesiones.get(&sesion_id) else {
            return;
        };
        let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
        (sesion.pantalla.clone(), ids)
    };
    if let Ok(mut parser) = pantalla.lock() {
        parser.procesar(texto.as_bytes());
    }
    if ids.is_empty() {
        return;
    }
    let estado_bloqueado = estado.lock().await;
    difusion::difundir_a_adjuntos(
        &estado_bloqueado.clientes,
        &ids,
        MensajeServidor::Datos {
            sesion_id,
            bytes: texto.into_bytes(),
        },
    );
}

// ---------------------------------------------------------------- bucle

enum FinSesion {
    /// Se cerró (exit del remoto o desde la ventana): ya se quitó del estado.
    Cerrada,
    /// La conexión cayó sin exit: puede reintentarse.
    Caida(String),
}

enum FinReintento {
    /// La pestaña se cerró mientras esperaba (o el servidor se apaga).
    Cerrada,
    /// Pasado `reconexion_max_min` sin éxito.
    Agotada(String),
}

/// Bucle de una sesión abierta: teclas y tamaños de los adjuntos hacia el
/// remoto, datos del remoto hacia los adjuntos y el parser del servidor, que
/// responde a las consultas en el mismo lote.
async fn bucle_sesion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    canal: &mut Channel<russh::client::Msg>,
    pantalla: &PantallaCompartida,
    rx_comandos: &mut mpsc::UnboundedReceiver<ComandoSesion>,
) -> FinSesion {
    let mut vio_eof = false;
    // Llegó el estado de salida del remoto: no es una caída de red, es un
    // cierre normal de la pestaña aunque el canal tarde en cerrarse.
    let mut vio_exit = false;
    loop {
        tokio::select! {
            comando = rx_comandos.recv() => match comando {
                Some(ComandoSesion::Teclas(bytes)) => {
                    // Una escritura sin plazo dejaría el bucle colgado si el
                    // remoto deja de aceptar datos.
                    match tokio::time::timeout(PLAZO_CANAL, canal.data_bytes(bytes)).await {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            return FinSesion::Caida(format!("se perdió la conexión: {error}"));
                        }
                        Err(_) => {
                            return FinSesion::Caida(
                                "el remoto dejó de aceptar datos".to_string(),
                            );
                        }
                    }
                }
                Some(ComandoSesion::AplicarTamano { cols, filas, historial }) => {
                    let _ = tokio::time::timeout(
                        PLAZO_CANAL,
                        canal.window_change(u32::from(cols), u32::from(filas), 0, 0),
                    )
                    .await;
                    if let Ok(mut parser) = pantalla.lock() {
                        parser.redimensionar(filas, cols, historial);
                    }
                }
                Some(ComandoSesion::Cerrar) | None => {
                    let _ = tokio::time::timeout(PLAZO_CANAL, canal.eof()).await;
                    let _ = tokio::time::timeout(PLAZO_CANAL, canal.close()).await;
                    cerrada(estado, sesion_id, "cerrada desde la ventana").await;
                    return FinSesion::Cerrada;
                }
            },
            mensaje = canal.wait() => match mensaje {
                Some(ChannelMsg::Data { data }) => {
                    if !procesar_datos(estado, sesion_id, pantalla, &data, canal).await {
                        return FinSesion::Caida(
                            "el remoto dejó de aceptar las respuestas".to_string(),
                        );
                    }
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if !procesar_datos(estado, sesion_id, pantalla, &data, canal).await {
                        return FinSesion::Caida(
                            "el remoto dejó de aceptar las respuestas".to_string(),
                        );
                    }
                }
                Some(ChannelMsg::Eof) => vio_eof = true,
                Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::ExitSignal { .. }) => {
                    vio_exit = true;
                }
                Some(ChannelMsg::Close) => {
                    cerrada(estado, sesion_id, "el remoto cerró la sesión").await;
                    return FinSesion::Cerrada;
                }
                None => {
                    if vio_eof || vio_exit {
                        cerrada(estado, sesion_id, "el remoto cerró la sesión").await;
                        return FinSesion::Cerrada;
                    }
                    return FinSesion::Caida("la conexión se ha perdido".to_string());
                }
                _ => {}
            },
        }
    }
}

/// Reparte un bloque de datos del remoto: el emulador del servidor (que
/// responde a las consultas en este mismo lote), los avisos, el título, el
/// portapapeles y la difusión a los adjuntos.
async fn procesar_datos(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    pantalla: &PantallaCompartida,
    datos: &[u8],
    canal: &mut Channel<russh::client::Msg>,
) -> bool {
    let (respuestas, titulo, aviso, portapapeles) = {
        let Ok(mut parser) = pantalla.lock() else {
            return true;
        };
        let respuestas = parser.procesar(datos);
        let titulo = parser.emulador_mut().tomar_titulo();
        let aviso = parser.emulador_mut().tomar_aviso();
        // Una lectura del portapapeles del remoto no se responde nunca.
        let _ = parser.emulador_mut().tomar_portapapeles_pedido();
        // El emulador del núcleo guarda solo la última copia OSC 52 del lote:
        // si un programa manda varias de golpe, las intermedias se pierden.
        // Es una limitación conocida (la cola acotada vive en el emulador, que
        // es del núcleo); se entrega la última, que es la que el usuario ve.
        let portapapeles = parser.emulador_mut().tomar_portapapeles();
        (respuestas, titulo, aviso, portapapeles)
    };
    // Las respuestas van antes que los datos a las ventanas, en el mismo lote
    // y una sola vez por pestaña (la tiene el servidor, no los clientes).
    if !respuestas.is_empty() {
        match tokio::time::timeout(PLAZO_CANAL, canal.data_bytes(respuestas)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!(
                    sesion = sesion_id,
                    "no se pudieron escribir las respuestas: {error}"
                );
                return false;
            }
            Err(_) => {
                warn!(
                    sesion = sesion_id,
                    "el remoto no aceptó las respuestas a tiempo"
                );
                return false;
            }
        }
    }
    let (ids, cosmetico) = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return true;
        };
        sesion.ultima_actividad = Utc::now().timestamp();
        let mut cosmetico = false;
        if let Some(titulo) = titulo {
            if sesion.titulo.as_deref() != Some(titulo.as_str()) {
                sesion.titulo = Some(titulo);
                cosmetico = true;
            }
        }
        if let Some(aviso) = aviso {
            if sesion.anotar_aviso(&aviso.texto, aviso.hora) {
                cosmetico = true;
            }
        }
        let ids = if sesion.adjuntos.is_empty() {
            sesion.actividad_no_vista = true;
            Vec::new()
        } else {
            sesion.adjuntos.keys().copied().collect::<Vec<u32>>()
        };
        (ids, cosmetico)
    };
    // Título y aviso no cambian nada de lo que decide el cliente: la lista se
    // difunde coalescida (una por segundo como mucho).
    if cosmetico {
        estado.lock().await.difusion_sesiones.marcar();
    }
    if let Some(texto) = portapapeles {
        let estado_portapapeles = estado.clone();
        tokio::spawn(async move {
            entregar_portapapeles(&estado_portapapeles, sesion_id, texto).await;
        });
    }
    if ids.is_empty() {
        return true;
    }
    let estado_bloqueado = estado.lock().await;
    difusion::difundir_a_adjuntos(
        &estado_bloqueado.clientes,
        &ids,
        MensajeServidor::Datos {
            sesion_id,
            bytes: datos.to_vec(),
        },
    );
    true
}

// ---------------------------------------------------------------- reconexión

/// Espera la siguiente espera de reconexión y reintenta; el intento se puede
/// adelantar con `Reconectar` (prefijo r) o con la vuelta de una suspensión.
/// Devuelve la conexión ya tomada (el PTY lo abre el bucle de fuera) y si el
/// intento fue manual.
async fn esperar_y_reintentar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &mut DatosApertura,
    rx_comandos: &mut mpsc::UnboundedReceiver<ComandoSesion>,
    causa: Causa,
    motivo_caida: &str,
) -> Result<(ConexionTomada, bool, u32), FinReintento> {
    let sesion_id = datos.sesion_id;
    let cancelar = datos.cancelar.clone();
    let desde = Utc::now().timestamp();
    let max_min = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .config
            .terminal
            .reconexion_max_min_efectivo()
    };
    let mut intento = 0u32;
    let mut pared_antes = Utc::now().timestamp();
    let mut mono_antes = Instant::now();
    // El canal de adelanto del intento (lo dispara `Reconectar`).
    let mut adelanto = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return Err(FinReintento::Cerrada);
        };
        sesion.estado = EstadoSesionRemota::Abriendo;
        sesion.reconectando = true;
        sesion.motivo = Some(format!("reconectando: {motivo_caida}"));
        sesion.handle = None;
        let (tx, rx) = watch::channel(0u64);
        sesion.adelantar = Some(tx);
        rx
    };
    // La conexión cayó: los túneles temporales de la pestaña mueren con ella y
    // su escritura en varios termina (se puede volver a pedir al reconectar).
    super::escritura_varios::sesion_acabo(estado, sesion_id, "la conexión cayó").await;
    super::tuneles_temporales::sesion_acabo(estado, sesion_id, "la conexión cayó").await;
    loop {
        intento += 1;
        let espera_seg = reconexion::espera(intento);
        let proximo_en = Utc::now().timestamp() + espera_seg as i64;
        {
            let mut estado_bloqueado = estado.lock().await;
            let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
                return Err(FinReintento::Cerrada);
            };
            sesion.reconexion = Some(InfoReconexion {
                intento,
                proximo_en,
                desde,
                causa: causa.texto().to_string(),
            });
        }
        difundir_lista(&mut *estado.lock().await);
        let fin_espera = tokio::time::Instant::now() + Duration::from_secs(espera_seg);
        let mut manual = false;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(fin_espera) => break,
                _ = cancelar.cancelled() => return Err(FinReintento::Cerrada),
                cambio = adelanto.changed() => {
                    if cambio.is_err() {
                        return Err(FinReintento::Cerrada);
                    }
                    manual = true;
                    break;
                }
                comando = rx_comandos.recv() => match comando {
                    Some(ComandoSesion::AplicarTamano { cols, filas, .. }) => {
                        datos.cols = cols;
                        datos.filas = filas;
                    }
                    Some(ComandoSesion::Cerrar) | None => return Err(FinReintento::Cerrada),
                    Some(ComandoSesion::Teclas(_)) => {}
                },
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let ahora_pared = Utc::now().timestamp();
                    let ahora_mono = Instant::now();
                    if reconexion::vuelve_de_suspension(
                        (pared_antes, mono_antes),
                        (ahora_pared, ahora_mono),
                    ) {
                        break;
                    }
                    pared_antes = ahora_pared;
                    mono_antes = ahora_mono;
                }
            }
        }
        if cancelar.is_cancelled() {
            return Err(FinReintento::Cerrada);
        }
        let transcurrido_min = (Utc::now().timestamp() - desde) / 60;
        if transcurrido_min >= max_min as i64 {
            let (bd, nombre_pestana) = {
                let estado_bloqueado = estado.lock().await;
                (
                    estado_bloqueado.bd.clone(),
                    estado_bloqueado
                        .sesiones
                        .get(&sesion_id)
                        .map(|sesion| sesion.nombre.clone())
                        .unwrap_or_default(),
                )
            };
            let _ = bd.send(super::OrdenBd::Anotar {
                tipo: crate::registro::RECONEXION_AGOTADA.to_string(),
                host_id: Some(datos.host.id),
                identidad_id: None,
                detalle: format!(
                    "«{}» · pestaña «{nombre_pestana}» · {intento} intento(s) · última causa: {}",
                    datos.host.nombre,
                    causa.texto()
                ),
                resultado: crate::modelo::ResultadoRegistro::Error,
            });
            return Err(FinReintento::Agotada(
                "reconexión agotada: el host no volvió".to_string(),
            ));
        }
        // El intento: con diálogos solo si el usuario lo pidió con prefijo r.
        let politica = {
            let estado_bloqueado = estado.lock().await;
            let Some(sesion) = estado_bloqueado.sesiones.get(&sesion_id) else {
                return Err(FinReintento::Cerrada);
            };
            if manual {
                PoliticaConexion::Interactiva {
                    solicitante: sesion.solicitante,
                    id_solicitud: sesion_id,
                }
            } else {
                let solicitante = Some(sesion.solicitante)
                    .filter(|cliente| estado_bloqueado.clientes.contains_key(cliente));
                PoliticaConexion::NoInteractiva {
                    solicitante,
                    id_solicitud: sesion_id,
                }
            }
        };
        // Esperar a que otra pestaña del mismo host acabe de abrir (puede
        // tener el cerrojo del host) no cuenta como tiempo del intento (§7.9).
        esperar_otra_apertura(estado, datos.host.id, sesion_id, &cancelar).await;
        let intento_resultado = tokio::time::timeout(
            PLAZO_INTENTO_RECONEXION,
            abrir_conexion(estado, datos, politica),
        )
        .await;
        match intento_resultado {
            Ok(Ok(tomada)) => return Ok((tomada, manual, intento)),
            Ok(Err(motivo)) => {
                let causa_intento = reconexion::clasificar(&motivo);
                if !causa_intento.recuperable() {
                    anotar_caida_no_recuperable(estado, datos, &motivo).await;
                    return Err(FinReintento::Cerrada);
                }
            }
            Err(_) => {}
        }
        pared_antes = Utc::now().timestamp();
        mono_antes = Instant::now();
        // Se quedó sin sesión (la cerró el usuario): fin.
        if !estado.lock().await.sesiones.contains_key(&sesion_id) {
            return Err(FinReintento::Cerrada);
        }
    }
}

/// Espera a que otra pestaña del mismo host termine su apertura, que puede
/// tener el cerrojo del host. §7.9 dice que esperar al cerrojo no cuenta como
/// fallo del intento: desde aquí no se ve el cerrojo, así que se usa como
/// señal que otra pestaña del host esté «abriendo». Se espera como mucho 30 s
/// (el plazo del cerrojo no interactivo) y fuera del plazo del intento.
async fn esperar_otra_apertura(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    sesion_id: u32,
    cancelar: &tokio_util::sync::CancellationToken,
) {
    let inicio = Instant::now();
    loop {
        let ocupado = {
            let estado_bloqueado = estado.lock().await;
            estado_bloqueado.sesiones.values().any(|sesion| {
                sesion.id != sesion_id
                    && sesion.host_id == host_id
                    && sesion.estado == EstadoSesionRemota::Abriendo
            })
        };
        if !ocupado || inicio.elapsed() >= Duration::from_secs(30) {
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            _ = cancelar.cancelled() => return,
        }
    }
}

/// Un intento de reconexión falló por algo que necesita al usuario: la
/// pestaña queda caída con la instrucción.
async fn anotar_caida_no_recuperable(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    motivo: &str,
) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&datos.sesion_id) else {
        return;
    };
    sesion.estado = EstadoSesionRemota::Caida;
    sesion.reconectando = false;
    sesion.reconexion = None;
    sesion.adelantar = None;
    sesion.motivo = Some(format!("necesita tu respuesta: prefijo r · {motivo}"));
    let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
    let aviso = MensajeServidor::Estado {
        sesion_id: datos.sesion_id,
        estado: EstadoSesionRemota::Caida,
        motivo: sesion.motivo.clone(),
    };
    difusion::difundir_a_adjuntos(&estado_bloqueado.clientes, &ids, aviso);
    difundir_lista(&mut estado_bloqueado);
}

/// Un intento de reconexión no llegó a abrir la pestaña: se deja caída (no se
/// borra: la sesión se custodia hasta que el usuario la cierre).
async fn cerrar_intento(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    motivo: &str,
    intento: u32,
) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
        return;
    };
    sesion.estado = EstadoSesionRemota::Caida;
    sesion.reconectando = false;
    sesion.reconexion = None;
    sesion.adelantar = None;
    sesion.motivo = Some(motivo.to_string());
    let host_id = sesion.host_id;
    let detalle = format!("automática · intento {intento} · {motivo}");
    let bd = estado_bloqueado.bd.clone();
    let _ = bd.send(super::OrdenBd::Anotar {
        tipo: crate::registro::SESION_CERRADA.to_string(),
        host_id: Some(host_id),
        identidad_id: None,
        detalle,
        resultado: crate::modelo::ResultadoRegistro::Error,
    });
    difundir_lista(&mut estado_bloqueado);
}

// ---------------------------------------------------------------- cierre

/// `exit` del remoto o cierre desde una ventana: la sesión se elimina.
async fn cerrada(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, sesion_id: u32, motivo: &str) {
    let mut host_id = None;
    let mut estado_bloqueado = estado.lock().await;
    if let Some(sesion) = estado_bloqueado.sesiones.remove(&sesion_id) {
        host_id = Some(sesion.host_id);
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::SESION_CERRADA.to_string(),
            host_id: Some(sesion.host_id),
            identidad_id: None,
            detalle: motivo.to_string(),
            resultado: crate::modelo::ResultadoRegistro::Ok,
        });
        // El canal del pool (o la conexión propia) lo suelta el bucle de la
        // sesión con su `ConexionTomada`, por identidad.
    }
    estado_bloqueado.pendientes.remove(&sesion_id);
    difundir_lista(&mut estado_bloqueado);
    drop(estado_bloqueado);
    if let Some(host_id) = host_id {
        // Una pestaña cerrada termina su escritura en varios y sus túneles
        // temporales; si era la última del host, sus túneles automáticos.
        super::escritura_varios::sesion_acabo(estado, sesion_id, "la pestaña se cerró").await;
        super::tuneles_temporales::sesion_acabo(estado, sesion_id, "la pestaña se cerró").await;
        super::tuneles::canales_cambiaron(estado, host_id).await;
    }
}

/// Red caída o EOF inesperado: la sesión se conserva hasta reconectar o
/// cerrar. Los túneles temporales de la pestaña mueren con la conexión.
async fn caida(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, sesion_id: u32, motivo: &str) {
    let mut estado_bloqueado = estado.lock().await;
    let (host_id, ids) = {
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            estado_bloqueado.pendientes.remove(&sesion_id);
            return;
        };
        sesion.estado = EstadoSesionRemota::Caida;
        sesion.reconectando = false;
        sesion.reconexion = None;
        sesion.adelantar = None;
        sesion.motivo = Some(motivo.to_string());
        // El canal del pool lo suelta el bucle de la sesión justo después.
        sesion.handle = None;
        (
            sesion.host_id,
            sesion.adjuntos.keys().copied().collect::<Vec<u32>>(),
        )
    };
    let aviso = MensajeServidor::Estado {
        sesion_id,
        estado: EstadoSesionRemota::Caida,
        motivo: Some(motivo.to_string()),
    };
    difusion::difundir_a_adjuntos(&estado_bloqueado.clientes, &ids, aviso);
    let bd = estado_bloqueado.bd.clone();
    let _ = bd.send(super::OrdenBd::Anotar {
        tipo: crate::registro::SESION_CERRADA.to_string(),
        host_id: Some(host_id),
        identidad_id: None,
        detalle: format!("caída: {motivo}"),
        resultado: crate::modelo::ResultadoRegistro::Error,
    });
    estado_bloqueado.pendientes.remove(&sesion_id);
    difundir_lista(&mut estado_bloqueado);
    drop(estado_bloqueado);
    super::escritura_varios::sesion_acabo(estado, sesion_id, "una pestaña se cayó").await;
    super::tuneles_temporales::sesion_acabo(estado, sesion_id, "la conexión cayó").await;
}

/// La apertura no llegó a abrir la pestaña: se quita, se anota y se avisa.
/// Solo anota quien retira la sesión: si ya la quitó otro (se cerró desde la
/// ventana, se fue la ventana, se apaga el servidor), ese lo anotó.
async fn apertura_fallida(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    motivo: &str,
) {
    warn!(sesion = datos.sesion_id, host = %datos.host.nombre, "apertura fallida: {motivo}");
    let mut estado_bloqueado = estado.lock().await;
    estado_bloqueado.pendientes.remove(&datos.sesion_id);
    let seguia = estado_bloqueado.sesiones.remove(&datos.sesion_id).is_some();
    if seguia {
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::CONEXION_FALLIDA.to_string(),
            host_id: Some(datos.host.id),
            identidad_id: None,
            detalle: motivo.to_string(),
            resultado: crate::modelo::ResultadoRegistro::Error,
        });
        let _ = bd.send(super::OrdenBd::MarcarEstado {
            host_id: datos.host.id,
            estado: Some(crate::modelo::UltimoEstado::Error),
        });
    }
    avisar_fallida(&mut estado_bloqueado, datos.sesion_id, motivo);
    drop(estado_bloqueado);
    if seguia && datos.reconexion {
        // Una reconexión fallida se lleva la pestaña, que contaba para el
        // ciclo automático de los túneles del host.
        let estado_tuneles = estado.clone();
        let host_id = datos.host.id;
        tokio::spawn(async move {
            super::tuneles::canales_cambiaron(&estado_tuneles, host_id).await;
        });
    }
}

/// Avisa a todos los clientes de que la apertura acabó sin pestaña.
fn avisar_fallida(estado_bloqueado: &mut EstadoServidor, sesion_id: u32, motivo: &str) {
    difusion::difundir(
        &estado_bloqueado.clientes,
        MensajeServidor::Estado {
            sesion_id,
            estado: EstadoSesionRemota::Cerrada,
            motivo: Some(motivo.to_string()),
        },
    );
    difundir_lista(estado_bloqueado);
}

// ---------------------------------------------------------------- foco y portapapeles

/// La ventana tiene o deja de tener la pestaña enfocada. El foco se agrega:
/// solo cuando cambia entre «alguna ventana» y «ninguna» se avisa al remoto
/// (con 1004 activo) con `CSI I` o `CSI O`.
pub async fn foco(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    sesion_id: u32,
    enfocada: bool,
) {
    let avisar = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return;
        };
        let antes = sesion.enfocada();
        if enfocada {
            sesion.enfocadas.insert(cliente_id);
            sesion.ventana_enfocada = Some(cliente_id);
        } else {
            sesion.enfocadas.remove(&cliente_id);
        }
        let despues = sesion.enfocada();
        let limpiar_aviso = enfocada && sesion.aviso.take().is_some();
        let secuencia = if antes != despues {
            let pedido = sesion
                .pantalla
                .lock()
                .map(|pantalla| pantalla.emulador().modos().foco)
                .unwrap_or(false);
            if pedido {
                Some(if despues {
                    b"\x1b[I".to_vec()
                } else {
                    b"\x1b[O".to_vec()
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(bytes) = &secuencia {
            let _ = sesion
                .tx_comandos
                .send(ComandoSesion::Teclas(bytes.clone()));
        }
        limpiar_aviso
    };
    if avisar {
        difundir_lista(&mut *estado.lock().await);
    }
}

/// Al irse una ventana se olvida su foco en todas las pestañas, con el mismo
/// aviso agregado al remoto.
pub async fn olvidar_foco(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, cliente_id: u32) {
    let mut avisos: Vec<(u32, Vec<u8>)> = Vec::new();
    {
        let mut estado_bloqueado = estado.lock().await;
        for sesion in estado_bloqueado.sesiones.values_mut() {
            if !sesion.enfocadas.remove(&cliente_id) {
                continue;
            }
            if sesion.ventana_enfocada == Some(cliente_id) {
                sesion.ventana_enfocada = None;
            }
            if sesion.enfocadas.is_empty() {
                let pedido = sesion
                    .pantalla
                    .lock()
                    .map(|pantalla| pantalla.emulador().modos().foco)
                    .unwrap_or(false);
                if pedido {
                    avisos.push((sesion.id, b"\x1b[O".to_vec()));
                }
            }
        }
    }
    for (sesion_id, bytes) in avisos {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
            let _ = sesion.tx_comandos.send(ComandoSesion::Teclas(bytes));
        }
    }
}

/// Lee la columna `portapapeles_remoto` del host (se mira en cada petición).
async fn host_permite_portapapeles(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> bool {
    let lectura = estado.lock().await.lectura.clone();
    lectura
        .lock()
        .ok()
        .and_then(|almacen| almacen.obtener_host(host_id).ok())
        .is_some_and(|host| host.portapapeles_remoto)
}

/// Entrega al portapapeles del cliente la copia que pidió el remoto por
/// OSC 52. Sin permiso, solo se guarda la última petición 60 s y se avisa.
pub async fn entregar_portapapeles(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    texto: String,
) {
    let Some(host_id) = estado
        .lock()
        .await
        .sesiones
        .get(&sesion_id)
        .map(|sesion| sesion.host_id)
    else {
        return;
    };
    if host_permite_portapapeles(estado, host_id).await {
        let destino = {
            let mut estado_bloqueado = estado.lock().await;
            let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
                return;
            };
            // A la ventana con la pestaña enfocada o, si no hay, a cualquier
            // adjunto; si esa ya no está, no hay a quién entregarlo.
            let ventana = sesion.ventana_enfocada;
            let adjuntos: Vec<u32> = sesion.adjuntos.keys().copied().collect();
            sesion.portapapeles_pendiente = None;
            ventana
                .filter(|cliente| estado_bloqueado.clientes.contains_key(cliente))
                .or_else(|| {
                    adjuntos
                        .into_iter()
                        .find(|cliente| estado_bloqueado.clientes.contains_key(cliente))
                })
        };
        if let Some(cliente_id) = destino {
            let estado_bloqueado = estado.lock().await;
            if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
                difusion::enviar(cliente, MensajeServidor::Portapapeles { sesion_id, texto });
            }
        }
        return;
    }
    let (ids, bytes) = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return;
        };
        let bytes = texto.len() as u64;
        sesion.portapapeles_pendiente =
            Some((texto, Utc::now().timestamp() + PORTAPAPELES_PENDIENTE_SEG));
        let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
        (ids, bytes)
    };
    let estado_bloqueado = estado.lock().await;
    difusion::difundir_a_adjuntos(
        &estado_bloqueado.clientes,
        &ids,
        MensajeServidor::PortapapelesPedido { sesion_id, bytes },
    );
}

/// `PortapapelesPermitido`: guarda el permiso en `HOSTS` (hilo escritor) y
/// entrega la petición pendiente si no han pasado 60 s.
pub async fn permitir_portapapeles(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    sesion_id: u32,
) {
    // El permiso lo escribe el cliente en HOSTS (es su ficha, §4.10); el
    // servidor solo entrega la petición pendiente.
    let entrega = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return;
        };
        let ventana = sesion.ventana_enfocada;
        match sesion.portapapeles_pendiente.take() {
            Some((texto, caduca)) if Utc::now().timestamp() <= caduca => {
                let destino = ventana
                    .filter(|cliente| estado_bloqueado.clientes.contains_key(cliente))
                    .unwrap_or(cliente_id);
                Some((destino, texto))
            }
            _ => None,
        }
    };
    if let Some((destino, texto)) = entrega {
        let estado_bloqueado = estado.lock().await;
        if let Some(cliente) = estado_bloqueado.clientes.get(&destino) {
            difusion::enviar(cliente, MensajeServidor::Portapapeles { sesion_id, texto });
        }
    }
}

// ---------------------------------------------------------------- renombrar y mover

/// Renombra una pestaña para todas las ventanas, con nombre único «(n)».
pub async fn renombrar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    nombre: &str,
) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(final_nombre) = nombre_renombrado(&estado_bloqueado.sesiones, sesion_id, nombre)
    else {
        return;
    };
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
        return;
    };
    if sesion.nombre == final_nombre {
        return;
    }
    sesion.nombre = final_nombre;
    difundir_lista(&mut estado_bloqueado);
}

/// Mueve una pestaña a otra posición (0 = la primera) para todas las ventanas.
pub async fn mover(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    posicion: u32,
) {
    let mut estado_bloqueado = estado.lock().await;
    let mut ordenadas: Vec<(u64, u32)> = estado_bloqueado
        .sesiones
        .values()
        .map(|sesion| (sesion.orden, sesion.id))
        .collect();
    ordenadas.sort_unstable();
    let Some(indice) = ordenadas.iter().position(|(_, id)| *id == sesion_id) else {
        return;
    };
    let (_, id) = ordenadas.remove(indice);
    let destino = (posicion as usize).min(ordenadas.len());
    ordenadas.insert(destino, (u64::MAX, id));
    for (orden, (_, id)) in ordenadas.into_iter().enumerate() {
        if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&id) {
            sesion.orden = orden as u64;
        }
    }
    difundir_lista(&mut estado_bloqueado);
}

// ---------------------------------------------------------------- diálogos

/// Traduce los eventos de la tarea de conexión a mensajes del protocolo:
/// diálogos solo al solicitante (con respuesta diferida), estados al resto.
/// La usan igual las sesiones, las aperturas de canal SFTP, los túneles y las
/// ejecuciones: `peticion_id` es el id de la solicitud de conexión. Sin
/// solicitante (un túnel automático), todo diálogo se contesta «no» al
/// momento: una operación automática no dialoga.
pub async fn puente_eventos(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    solicitante: Option<u32>,
    mut rx: mpsc::UnboundedReceiver<EventoConexion>,
) {
    while let Some(evento) = rx.recv().await {
        let Some(solicitante) = solicitante else {
            // Soltar el evento suelta su canal de respuesta: la conexión lo
            // lee como «cancelado». Los avisos sí se anotan.
            if let EventoConexion::HuellaRegistrada { .. } = evento {
                anotar_huella(&estado, evento).await;
            }
            continue;
        };
        match evento {
            EventoConexion::Estado { estado: fino, .. } => {
                let motivo = motivo_de_estado(fino).to_string();
                {
                    let mut estado_bloqueado = estado.lock().await;
                    if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&peticion_id) {
                        sesion.motivo = Some(motivo.clone());
                    }
                }
                let mut estado_bloqueado = estado.lock().await;
                difundir_lista(&mut estado_bloqueado);
            }
            EventoConexion::HuellaDesconocida {
                host,
                tipo,
                huella,
                responder,
            } => {
                decidir(
                    &estado,
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Huella(responder),
                    MensajeServidor::HuellaDesconocida {
                        sesion_id: peticion_id,
                        host,
                        tipo_clave: tipo,
                        huella,
                    },
                    TIMEOUT_DECISION,
                )
                .await;
            }
            EventoConexion::HuellaCambiada {
                host,
                tipo,
                anterior,
                nueva,
                responder,
            } => {
                decidir(
                    &estado,
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Huella(responder),
                    MensajeServidor::HuellaCambiada {
                        sesion_id: peticion_id,
                        host,
                        tipo_clave: tipo,
                        anterior,
                        nueva,
                    },
                    TIMEOUT_DECISION,
                )
                .await;
            }
            EventoConexion::PideFrase {
                host,
                intento,
                responder,
            } => {
                decidir(
                    &estado,
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Frase(responder),
                    MensajeServidor::PideFrase {
                        sesion_id: peticion_id,
                        host,
                        intento,
                    },
                    TIMEOUT_DECISION,
                )
                .await;
            }
            EventoConexion::PideContrasena {
                host,
                intento,
                recordar_por_defecto,
                responder,
            } => {
                decidir(
                    &estado,
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Contrasena(responder),
                    MensajeServidor::PideContrasena {
                        sesion_id: peticion_id,
                        host,
                        intento,
                        recordar_por_defecto,
                    },
                    TIMEOUT_DECISION,
                )
                .await;
            }
            EventoConexion::PideContrasenaLlavero {
                host,
                usuario,
                responder,
            } => {
                decidir(
                    &estado,
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Llavero(responder),
                    MensajeServidor::PideLlavero {
                        sesion_id: peticion_id,
                        host,
                        usuario,
                    },
                    crate::conexion::cliente::PLAZO_LLAVERO,
                )
                .await;
            }
            // La huella ya se escribió en known_hosts: el efecto se anota aquí.
            evento @ EventoConexion::HuellaRegistrada { .. } => {
                anotar_huella(&estado, evento).await;
            }
            // Eventos que en el servidor no aplican: la apertura no pasa por
            // `sesion_completa` y las pantallas las difunde el bucle.
            EventoConexion::ContrasenaGuardada { .. }
            | EventoConexion::Abierta { .. }
            | EventoConexion::Pantalla
            | EventoConexion::PruebaOk { .. }
            | EventoConexion::Cerrada { .. }
            | EventoConexion::Error { .. }
            | EventoConexion::Cancelada { .. } => {}
        }
    }
}

/// La huella ya está en known_hosts: se anota el efecto.
async fn anotar_huella(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, evento: EventoConexion) {
    let EventoConexion::HuellaRegistrada {
        host_id,
        anterior,
        nueva,
    } = evento
    else {
        return;
    };
    let (tipo, detalle) = match anterior {
        Some(anterior) => (
            crate::registro::HUELLA_SUSTITUIDA,
            format!("anterior {anterior} · nueva {nueva}"),
        ),
        None => (
            crate::registro::HUELLA_ACEPTADA,
            format!("huella nueva {nueva}"),
        ),
    };
    let bd = estado.lock().await.bd.clone();
    let _ = bd.send(super::OrdenBd::Anotar {
        tipo: tipo.to_string(),
        host_id: (host_id != 0).then_some(host_id),
        identidad_id: None,
        detalle,
        resultado: crate::modelo::ResultadoRegistro::Ok,
    });
}

/// Guarda una decisión pendiente, avisa al solicitante y al resto de clientes,
/// y programa su plazo (5 minutos si espera al usuario, 10 s si es el
/// llavero). `peticion_id` es el id de la solicitud de conexión: el de una
/// sesión o el de una apertura de SFTP, túnel o ejecución. Si el solicitante
/// ya no está conectado, nadie va a contestar: se descarta al momento.
pub async fn decidir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    solicitante: u32,
    dialogo: DialogoPendiente,
    mensaje: MensajeServidor,
    plazo: Duration,
) {
    let mut estado_bloqueado = estado.lock().await;
    if !estado_bloqueado.clientes.contains_key(&solicitante) {
        drop(dialogo);
        return;
    }
    let generacion = GENERACION_DIALOGO.fetch_add(1, Ordering::Relaxed);
    estado_bloqueado.pendientes.insert(
        peticion_id,
        Pendiente {
            solicitante,
            dialogo,
            generacion,
        },
    );
    let mut otros: Vec<u32> = Vec::new();
    for (id_cliente, cliente) in &estado_bloqueado.clientes {
        if *id_cliente == solicitante {
            difusion::enviar(cliente, mensaje.clone());
        } else {
            otros.push(*id_cliente);
        }
    }
    // Al resto solo se le avisa cuando la solicitud es una sesión: una apertura
    // de canal SFTP no tiene pestaña que enseñar en las demás ventanas.
    if estado_bloqueado.sesiones.contains_key(&peticion_id) {
        for id in otros {
            if let Some(cliente) = estado_bloqueado.clientes.get(&id) {
                difusion::enviar(
                    cliente,
                    MensajeServidor::Estado {
                        sesion_id: peticion_id,
                        estado: EstadoSesionRemota::Abriendo,
                        motivo: Some("esperando decisión en otra ventana".to_string()),
                    },
                );
            }
        }
    }
    // Plazo: si la decisión no llega, se descarta el canal de respuesta y la
    // apertura se cancela sola. Solo se retira el diálogo que se programó: el
    // mismo id puede estar ya en su siguiente intento.
    let estado_timeout = estado.clone();
    tokio::spawn(async move {
        tokio::time::sleep(plazo).await;
        let mut estado_bloqueado = estado_timeout.lock().await;
        let es_el_mismo = estado_bloqueado
            .pendientes
            .get(&peticion_id)
            .is_some_and(|pendiente| pendiente.generacion == generacion);
        if es_el_mismo {
            estado_bloqueado.pendientes.remove(&peticion_id);
            warn!(
                peticion = peticion_id,
                "decisión sin respuesta en {} s",
                plazo.as_secs()
            );
        }
    });
}

/// El solicitante renuncia a un diálogo pendiente (`Cerrar` sobre un id que
/// no es de sesión: una apertura de SFTP, túnel o ejecución). Se suelta su
/// canal de respuesta y la apertura falla enseguida en vez de esperar al plazo.
pub async fn cancelar_dialogo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    cliente_id: u32,
) -> bool {
    let mut estado_bloqueado = estado.lock().await;
    let es_suyo = estado_bloqueado
        .pendientes
        .get(&peticion_id)
        .is_some_and(|pendiente| pendiente.solicitante == cliente_id);
    if es_suyo {
        estado_bloqueado.pendientes.remove(&peticion_id);
    }
    es_suyo
}

/// Resuelve una decisión que llega por el protocolo. Solo el solicitante de la
/// solicitud puede responder; devuelve si se aceptó la respuesta.
pub async fn resolver_decision(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    cliente_id: u32,
    decision: DecisionDialogo,
) -> bool {
    let mut estado_bloqueado = estado.lock().await;
    let Some(pendiente) = estado_bloqueado.pendientes.get(&peticion_id) else {
        warn!(peticion = peticion_id, "decisión sin diálogo pendiente");
        return false;
    };
    if pendiente.solicitante != cliente_id {
        warn!(
            peticion = peticion_id,
            cliente = cliente_id,
            "un cliente que no es el solicitante intentó responder a un diálogo"
        );
        return false;
    }
    let Some(pendiente) = estado_bloqueado.pendientes.remove(&peticion_id) else {
        return false;
    };
    match (pendiente.dialogo, decision) {
        (DialogoPendiente::Huella(responder), DecisionDialogo::Huella(d)) => {
            let _ = responder.send(d);
            true
        }
        (DialogoPendiente::Frase(responder), DecisionDialogo::Frase(frase)) => {
            let _ = responder.send(Some(frase.0));
            true
        }
        (
            DialogoPendiente::Contrasena(responder),
            DecisionDialogo::Contrasena(contrasena, recordar),
        ) => {
            let _ = responder.send(Some((contrasena.0, recordar)));
            true
        }
        (DialogoPendiente::Llavero(responder), DecisionDialogo::Contrasena(contrasena, _)) => {
            let _ = responder.send(Some(contrasena.0));
            true
        }
        (dialogo, _) => {
            drop(dialogo);
            warn!(
                peticion = peticion_id,
                "decisión que no casa con el diálogo"
            );
            false
        }
    }
}

fn motivo_de_estado(estado: EstadoSesion) -> &'static str {
    match estado {
        EstadoSesion::Resolviendo => "resolviendo",
        EstadoSesion::Conectando => "conectando",
        EstadoSesion::VerificandoHuella => "verificando huella",
        EstadoSesion::Autenticando => "autenticando",
        _ => "abriendo",
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tamano(cols: u16, filas: u16) -> Tamano {
        Tamano { cols, filas }
    }

    #[test]
    fn el_minimo_crece_cuando_crece_la_unica_ventana() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(200, 56));
        assert_eq!(tamano_minimo(&adjuntos, tamano(100, 26)), tamano(200, 56));
    }

    #[test]
    fn el_minimo_encoge_con_la_ventana() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(80, 20));
        assert_eq!(tamano_minimo(&adjuntos, tamano(100, 26)), tamano(80, 20));
    }

    #[test]
    fn sin_adjuntos_se_conserva_el_ultimo() {
        assert_eq!(
            tamano_minimo(&HashMap::new(), tamano(100, 26)),
            tamano(100, 26)
        );
        assert_eq!(
            ventana_minima_para(&HashMap::new(), tamano(100, 26), 1),
            None
        );
    }

    #[test]
    fn con_dos_ventanas_manda_la_menor_por_componente() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(200, 30));
        adjuntos.insert(2, tamano(120, 46));
        let minimo = tamano_minimo(&adjuntos, tamano(10, 10));
        assert_eq!(minimo, tamano(120, 30));
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 1), Some(2));
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 2), Some(1));
    }

    #[test]
    fn la_ventana_que_impone_todo_se_ve_a_si_misma() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(200, 46));
        adjuntos.insert(2, tamano(100, 26));
        let minimo = tamano_minimo(&adjuntos, tamano(10, 10));
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 1), Some(2));
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 2), Some(2));
        adjuntos.insert(3, tamano(100, 26));
        assert_eq!(
            ventana_minima_para(&adjuntos, minimo, 1),
            Some(2),
            "empate: id menor"
        );
    }
}
