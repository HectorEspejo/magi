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
    CausaAperturaTmux, ColoresTema, EstadoSesionRemota, InfoAviso, InfoReconexion, InfoSesion,
    MensajeServidor, PeticionTmux, Secreto, ServidorTmux, Soltada, TmuxPestana, VersionTmux,
};
use magi_nucleo::conexion::reconexion::{self, Causa};
use magi_nucleo::sesiones_persistentes::lectura::ResultadoCrear;
use magi_nucleo::sesiones_persistentes::nombres::nombre_automatico;
use magi_nucleo::sesiones_persistentes::validadores;
use magi_nucleo::terminal::emulador::PantallaEmulada;
use magi_nucleo::terminal::historial::lineas_de_historial;
use magi_nucleo::terminal::tmux::{
    OrdenTmux, MAXIMO_LINEAS_HISTORIAL_TMUX, SALIDA_ANTIGUO, SALIDA_NO_EXISTE, SALIDA_SIN_TMUX,
    SALIDA_SOLTADA,
};

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

/// Valida la sesión de tmux que pide el cliente (repite lo que valida él) y la
/// normaliza: la carpeta o el comando vacíos son «sin valor». En el servidor
/// habitual solo se engancha: crear solo vale en el propio de MAGI. `Err`
/// lleva el nombre (saneado, para el mensaje) y la instrucción.
pub fn validar_peticion_tmux(mut peticion: PeticionTmux) -> Result<PeticionTmux, (String, String)> {
    let nombre_saneado: String = peticion
        .nombre
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(validadores::MAXIMO_NOMBRE)
        .collect();
    let fallo = |mensaje: String| (nombre_saneado.clone(), mensaje);
    if validadores::validar_nombre(&peticion.nombre).is_err() {
        return Err(fallo(format!(
            "nombre de tmux no admitido: «{nombre_saneado}»"
        )));
    }
    if let Some(crear) = peticion.crear.as_mut() {
        if peticion.servidor != ServidorTmux::Magi {
            return Err(fallo(
                "las sesiones nuevas solo se crean en el tmux de MAGI".to_string(),
            ));
        }
        crear.carpeta = crear.carpeta.take().filter(|carpeta| !carpeta.is_empty());
        crear.comando = crear.comando.take().filter(|comando| !comando.is_empty());
        if let Some(carpeta) = &crear.carpeta {
            validadores::validar_carpeta(carpeta).map_err(|error| fallo(error.to_string()))?;
        }
        if let Some(comando) = &crear.comando {
            validadores::validar_comando(comando).map_err(|error| fallo(error.to_string()))?;
        }
    }
    Ok(peticion)
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
    /// Nombre de la sesión de tmux a la que está enganchada la pestaña. Es el
    /// estado vivo: la reconexión sale de aquí y no de lo que se pidió al
    /// abrirla (un host marcado pone el nombre automático al abrir).
    pub tmux: Option<String>,
    /// Qué servidor de tmux (Fase 16): solo vale con `tmux`. Es persistente si
    /// es el propio de MAGI.
    pub tmux_servidor: ServidorTmux,
    /// Versión de tmux que dio el host al abrir (la barra de la pestaña la
    /// enseña).
    pub tmux_version: Option<VersionTmux>,
    /// Engancharse sin soltar a los demás (lo que pidió el usuario al abrir o
    /// al volver): la reconexión automática lo respeta y no quita a nadie.
    pub tmux_compartir: bool,
    /// Otro equipo se enganchó a la sesión y la pestaña quedó caída con su
    /// última pantalla: quién la tiene y desde cuándo. Se quita al volver.
    pub soltada: Option<Soltada>,
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
    /// El remoto avisó (campana, OSC 9 u OSC 777) y nadie la ha atendido desde
    /// entonces (Fase 17, §4.7). Es lo que decide el aviso de Android de una
    /// pestaña, y solo lo pone el emulador del remoto, no los avisos que el
    /// propio servidor guarda en `aviso`. Solo se usa en Termux. Se quita al
    /// enfocarla alguna ventana y al escribir en ella.
    pub atencion_remota: bool,
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
            tmux: self.tmux.as_ref().map(|nombre| TmuxPestana {
                servidor: self.tmux_servidor,
                nombre: nombre.clone(),
                version: self.tmux_version,
                soltada: self.soltada.clone(),
            }),
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

    /// Alguien atiende la pestaña (la enfoca una ventana o escribe en ella):
    /// quita la marca de atención del remoto. Devuelve si estaba puesta.
    pub fn atendida(&mut self) -> bool {
        std::mem::take(&mut self.atencion_remota)
    }

    /// El remoto pide atención y nadie la mira: pone la marca. Devuelve si
    /// cambió (estaba quitada); con la marca ya puesta no hay nada nuevo que
    /// decir a los avisos de Android.
    pub fn pedir_atencion(&mut self) -> bool {
        !std::mem::replace(&mut self.atencion_remota, true)
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
    // Una pestaña que aparece, se cae o se cierra cambia los avisos de Android
    // que tocan (y se retira el de la que ya no está).
    if let Some(termux) = &estado.termux {
        termux.despertar_avisos();
    }
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
    /// La sesión de tmux que pidió el cliente al abrir (crearla o engancharse).
    /// `None`: la decide la marca `sesiones_persistentes` del host. Al
    /// reconectar no se usa: manda el estado vivo de la sesión (`Sesion.tmux`).
    pub tmux: Option<PeticionTmux>,
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

        // 2. tmux: comprobar, crear o mirar el estado de la sesión del host.
        //    Una pestaña normal se abre siempre; una con tmux pedida no se
        //    degrada en silencio (no queda pestaña y el cliente decide), y la
        //    de un host marcado sí cae a un shell normal con un aviso.
        let preparado = preparar_tmux(&estado, &datos, &handle, reintento, auto).await;
        // La pestaña pudo cerrarse mientras se hablaba con el host (por ejemplo,
        // creando la sesión): no se abre un canal para nadie.
        if datos.cancelar.is_cancelled() {
            tomada.soltar().await;
            return;
        }
        let plan = match preparado {
            Preparado::Normal => None,
            Preparado::Enganchar(plan) => Some(plan),
            Preparado::Fallida(causa, mensaje) => {
                tomada.soltar().await;
                apertura_tmux_fallida(&estado, &datos, causa, mensaje).await;
                return;
            }
            Preparado::Soltada(por_equipo) => {
                // La reconexión automática no le quita la sesión a otro equipo.
                marcar_soltada(&estado, sesion_id, por_equipo).await;
                tomada.soltar().await;
                return;
            }
            Preparado::Terminada(motivo) => {
                tomada.soltar().await;
                sesion_terminada_en_el_host(&estado, &datos, &motivo).await;
                return;
            }
            Preparado::SinComprobar(motivo) => {
                // La pestaña queda caída con la instrucción (la sesión del host
                // no se toca): se reintenta a mano con prefijo r.
                tomada.soltar().await;
                if auto {
                    cerrar_intento(&estado, sesion_id, &motivo, intento_auto).await;
                } else {
                    caida(&estado, sesion_id, &motivo).await;
                }
                return;
            }
        };

        // 3. Canal con PTY y COLORTERM. Una pestaña con tmux no abre un shell
        //    para escribir el script: lo lanza con `exec` (de otro modo el eco
        //    de sus 24 KB llenaría la pantalla y el historial). Si falla, la
        //    conexión se suelta.
        let arranque = match &plan {
            Some(plan) => Arranque::Exec(plan.comando.clone()),
            None => Arranque::Shell,
        };
        let mut canal =
            match abrir_con_plazo_terminal(&handle, datos.cols, datos.filas, arranque).await {
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

        // 4. Comandos iniciales. En una pestaña normal se escriben ya (al
        //    reconectar, solo los que `repetir`). En una con tmux, solo en una
        //    sesión nueva sin comando propio y DESPUÉS del enganche (cuando
        //    tmux ya ha pintado): al engancharse a una sesión que ya existía
        //    no se escribe nada, el programa sigue dentro.
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
        let mut diferidos: Vec<String> = Vec::new();
        match &plan {
            Some(plan) => {
                if plan.nueva && !plan.con_comando_propio {
                    diferidos = comandos;
                }
            }
            None => {
                for texto in &comandos {
                    let escrito = tokio::time::timeout(
                        PLAZO_CANAL,
                        canal.data_bytes(texto.clone().into_bytes()),
                    )
                    .await;
                    if !matches!(escrito, Ok(Ok(()))) {
                        warn!(sesion = sesion_id, "no se pudo escribir el comando inicial");
                        break;
                    }
                }
            }
        }

        // 5. Pestaña abierta: estado, difusión y anotación.
        let tmux_nombre = plan.as_ref().map(|plan| plan.nombre.clone());
        let primera_en_conexion = marcar_abierta(
            &estado,
            sesion_id,
            &tomada,
            plan.as_ref().map(TmuxAbierta::de_plan),
        )
        .await;
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
            plan.is_some(),
            diferidos,
        )
        .await;
        // Otro equipo se llevó la sesión: la pestaña queda soltada con su
        // última pantalla. Se mira quién la tiene con esta misma conexión
        // antes de soltarla.
        if let FinSesion::Soltada = fin {
            if let Some(plan) = &plan {
                sesion_soltada(&estado, sesion_id, &handle, plan).await;
            }
            tomada.soltar().await;
            return;
        }
        tomada.soltar().await;
        match fin {
            FinSesion::Cerrada | FinSesion::Soltada => return,
            FinSesion::TmuxFallo {
                causa,
                mensaje,
                cerrar,
            } => {
                if cerrar {
                    sesion_tmux_cerrada(&estado, &datos, causa, mensaje).await;
                } else {
                    caida(&estado, sesion_id, &mensaje).await;
                }
                return;
            }
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

/// Cómo arranca el canal con PTY de la pestaña.
enum Arranque {
    /// El shell de acceso del usuario.
    Shell,
    /// Un comando con `exec` (el enganche de tmux): el código de salida del
    /// canal es el del script.
    Exec(String),
}

/// Abre el canal con PTY de la pestaña y pide `COLORTERM=truecolor` (el host
/// puede ignorarlo). El plazo lo pone quien llama.
async fn abrir_con_plazo_terminal(
    handle: &Arc<Handle<Cliente>>,
    cols: u16,
    filas: u16,
    arranque: Arranque,
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
        match arranque {
            Arranque::Shell => canal
                .request_shell(true)
                .await
                .map_err(|error| format!("no se pudo pedir la shell: {error}"))?,
            Arranque::Exec(comando) => canal
                .exec(true, comando.into_bytes())
                .await
                .map_err(|error| format!("no se pudo lanzar tmux: {error}"))?,
        }
        Ok::<Channel<russh::client::Msg>, String>(canal)
    };
    match tokio::time::timeout(PLAZO_CANAL, abrir).await {
        Ok(Ok(canal)) => Ok(canal),
        Ok(Err(motivo)) => Err(motivo),
        Err(_) => Err("el host no abrió el canal de la pestaña a tiempo".to_string()),
    }
}

// ---------------------------------------------------------------- tmux

/// La sesión de tmux a la que va a engancharse la pestaña (o está ya).
struct PlanEnganche {
    servidor: ServidorTmux,
    nombre: String,
    /// Versión de tmux del host, si la dio la comprobación.
    version: Option<VersionTmux>,
    /// Sin soltar a los demás clientes.
    compartir: bool,
    /// El comando de `exec` con el que se engancha (el script y sus
    /// argumentos, ya escapados).
    comando: String,
    /// La creó esta apertura.
    nueva: bool,
    /// Se creó con un comando de arranque propio: los comandos iniciales de
    /// la pestaña (snippet al conectar, comando inicial) no se escriben.
    con_comando_propio: bool,
}

/// Lo que la pestaña guarda de su sesión de tmux al quedar abierta.
struct TmuxAbierta {
    servidor: ServidorTmux,
    nombre: String,
    version: Option<VersionTmux>,
    compartir: bool,
}

impl TmuxAbierta {
    fn de_plan(plan: &PlanEnganche) -> Self {
        Self {
            servidor: plan.servidor,
            nombre: plan.nombre.clone(),
            version: plan.version,
            compartir: plan.compartir,
        }
    }
}

/// Qué hace la apertura o la reconexión con tmux.
enum Preparado {
    /// Una pestaña normal (sin tmux pedido, o el host marcado sin tmux utilizable,
    /// o una reconexión a una sesión del tmux habitual que ya no existe: con su
    /// aviso en la pestaña).
    Normal,
    /// Engancharse a la sesión.
    Enganchar(PlanEnganche),
    /// Lo pedido no se puede abrir: no queda pestaña (con la causa y la
    /// instrucción para el cliente).
    Fallida(CausaAperturaTmux, String),
    /// Reconexión automática: la sesión la tiene otro equipo y no se le quita.
    Soltada(Option<String>),
    /// Reconexión: la sesión ya no existe en el host; la pestaña se cierra.
    Terminada(String),
    /// Reconexión: no se pudo mirar la sesión; la pestaña queda caída.
    SinComprobar(String),
}

/// Pone un aviso en la pestaña (con el tope de uno cada 5 s) sin esperar.
fn avisar_en_pestana(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    texto: String,
) {
    let estado = estado.clone();
    tokio::spawn(async move {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
            if sesion.anotar_aviso(&texto, Utc::now().timestamp()) {
                difundir_lista(&mut estado_bloqueado);
            }
        }
    });
}

/// Prepara la sesión de tmux antes de abrir el PTY de la pestaña (§4.1 a §4.4
/// del informe de la Fase 16). Nunca dialoga: la conexión ya está abierta y
/// todo va por canales `exec` con plazo.
async fn preparar_tmux(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    handle: &Arc<Handle<Cliente>>,
    es_reconexion: bool,
    auto: bool,
) -> Preparado {
    if es_reconexion {
        return preparar_reconexion(estado, datos, handle, auto).await;
    }
    match datos.tmux.clone() {
        Some(peticion) => preparar_pedida(estado, datos, handle, peticion).await,
        None if datos.host.sesiones_persistentes => {
            preparar_host_marcado(estado, datos, handle).await
        }
        None => Preparado::Normal,
    }
}

/// El comando de `exec` con el que se engancha la pestaña. `None` si algún
/// dato no es válido (nunca pasa con una etiqueta de equipo y un nombre ya
/// validados).
fn comando_de_enganche(
    servidor: ServidorTmux,
    nombre: &str,
    soltar: bool,
    equipo: &str,
) -> Option<String> {
    OrdenTmux::Enganchar {
        servidor,
        nombre,
        soltar,
        equipo,
    }
    .comando_exec()
}

/// Un `crear` sin carpeta ni comando con el tamaño y el historial de la
/// pestaña (el host marcado) o con lo que pidió el cliente.
fn orden_crear<'a>(
    datos: &DatosApertura,
    nombre: &'a str,
    carpeta: Option<&'a str>,
    comando: Option<&'a str>,
) -> OrdenTmux<'a> {
    OrdenTmux::Crear {
        nombre,
        carpeta: carpeta.filter(|carpeta| !carpeta.is_empty()),
        comando: comando.filter(|comando| !comando.is_empty()),
        lineas_historial: u32::try_from(datos.historial_lineas)
            .unwrap_or(u32::MAX)
            .min(MAXIMO_LINEAS_HISTORIAL_TMUX),
        columnas: datos.cols,
        filas: datos.filas,
    }
}

/// Traduce el resultado de `crear` en un fallo con su causa y su instrucción.
fn fallo_de_crear(
    resultado: ResultadoCrear,
    detalle: Option<String>,
    host: &str,
    nombre: &str,
    carpeta: Option<&str>,
) -> (CausaAperturaTmux, String) {
    match resultado {
        ResultadoCrear::SinTmux => (
            CausaAperturaTmux::SinTmux,
            format!("instala tmux en el host «{host}»"),
        ),
        ResultadoCrear::Antiguo => (
            CausaAperturaTmux::TmuxAntiguo,
            format!("tmux demasiado antiguo (3.0 o posterior) en «{host}»"),
        ),
        ResultadoCrear::Existe => (
            CausaAperturaTmux::Existe,
            format!("ya hay una sesión «{nombre}» en «{host}»"),
        ),
        ResultadoCrear::SinCarpeta => (
            CausaAperturaTmux::SinCarpeta,
            format!(
                "la carpeta {} no existe en «{host}»",
                carpeta.unwrap_or("indicada")
            ),
        ),
        ResultadoCrear::Creada | ResultadoCrear::Error => (
            CausaAperturaTmux::Otra,
            match detalle {
                Some(detalle) => {
                    format!("tmux no pudo crear la sesión «{nombre}» en «{host}»: {detalle}")
                }
                None => format!("tmux no pudo crear la sesión «{nombre}» en «{host}»"),
            },
        ),
    }
}

/// Apertura pedida por el cliente (`AbrirSesion.tmux`): comprobar, crear si
/// hace falta y enganchar. Si algo falla, no hay pestaña.
async fn preparar_pedida(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    handle: &Arc<Handle<Cliente>>,
    peticion: PeticionTmux,
) -> Preparado {
    let host = datos.host.nombre.as_str();
    let nombre = peticion.nombre.as_str();
    let comprobada = match super::tmux::comprobar(handle, peticion.servidor, nombre).await {
        Ok(comprobada) => comprobada,
        Err(motivo) => {
            warn!(
                sesion = datos.sesion_id,
                "no se pudo comprobar tmux: {motivo}"
            );
            return Preparado::Fallida(
                CausaAperturaTmux::Conexion,
                format!("no se pudo hablar con tmux en «{host}»: {motivo}"),
            );
        }
    };
    let Some(version) = comprobada.tmux else {
        return Preparado::Fallida(
            CausaAperturaTmux::SinTmux,
            format!("instala tmux en el host «{host}»"),
        );
    };
    if peticion.servidor == ServidorTmux::Magi && version < VersionTmux::MINIMA {
        return Preparado::Fallida(
            CausaAperturaTmux::TmuxAntiguo,
            format!("tmux demasiado antiguo (3.0 o posterior) en «{host}»"),
        );
    }
    // Al tmux habitual no se le suelta a nadie: el propio terminal del usuario
    // puede estar enganchado a esa sesión (la Fase 15 compartía).
    let soltar = peticion.servidor == ServidorTmux::Magi && !peticion.compartir;
    let mut nueva = false;
    let mut con_comando_propio = false;
    if let Some(crear) = &peticion.crear {
        if peticion.servidor != ServidorTmux::Magi {
            return Preparado::Fallida(
                CausaAperturaTmux::Otra,
                "las sesiones nuevas solo se crean en el tmux de MAGI".to_string(),
            );
        }
        if comprobada.existe {
            return Preparado::Fallida(
                CausaAperturaTmux::Existe,
                format!("ya hay una sesión «{nombre}» en «{host}»"),
            );
        }
        let orden = orden_crear(
            datos,
            nombre,
            crear.carpeta.as_deref(),
            crear.comando.as_deref(),
        );
        if orden.argumentos().is_none() {
            return Preparado::Fallida(
                CausaAperturaTmux::Otra,
                "los datos de la sesión no son válidos".to_string(),
            );
        }
        match super::tmux::crear(handle, &orden).await {
            Err(motivo) => {
                warn!(
                    sesion = datos.sesion_id,
                    "no se pudo crear la sesión de tmux: {motivo}"
                );
                return Preparado::Fallida(
                    CausaAperturaTmux::Conexion,
                    format!("no se pudo hablar con tmux en «{host}»: {motivo}"),
                );
            }
            Ok(creada) if creada.lectura.resultado != ResultadoCrear::Creada => {
                let (causa, mensaje) = fallo_de_crear(
                    creada.lectura.resultado,
                    creada.detalle,
                    host,
                    nombre,
                    crear.carpeta.as_deref(),
                );
                return Preparado::Fallida(causa, mensaje);
            }
            Ok(_) => {}
        }
        nueva = true;
        con_comando_propio = crear.comando.as_deref().is_some_and(|c| !c.is_empty());
    } else if !comprobada.existe {
        return Preparado::Fallida(
            CausaAperturaTmux::Otra,
            format!("la sesión «{nombre}» ya no existe en «{host}»"),
        );
    }
    let equipo = super::tmux::etiqueta_del_equipo(estado).await;
    let Some(comando) = comando_de_enganche(peticion.servidor, nombre, soltar, &equipo) else {
        return Preparado::Fallida(
            CausaAperturaTmux::Otra,
            "los datos de la sesión no son válidos".to_string(),
        );
    };
    Preparado::Enganchar(PlanEnganche {
        servidor: peticion.servidor,
        nombre: peticion.nombre,
        version: Some(version),
        compartir: !soltar && peticion.servidor == ServidorTmux::Magi,
        comando,
        nueva,
        con_comando_propio,
    })
}

/// Intentos de nombre automático antes de rendirse: otra ventana puede haber
/// creado el mismo nombre entre mirar y crear.
const INTENTOS_NOMBRE_AUTOMATICO: usize = 8;

/// Pestaña nueva en un host con `sesiones_persistentes`: nombre automático
/// `<equipo>-<n>`, sesión en el shell de acceso y enganche, sin diálogo. Sin
/// tmux utilizable, cae a una pestaña normal con un aviso.
async fn preparar_host_marcado(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    handle: &Arc<Handle<Cliente>>,
) -> Preparado {
    let sesion_id = datos.sesion_id;
    let host = datos.host.nombre.as_str();
    let lectura = match super::tmux::estado(handle).await {
        Ok(lectura) => lectura,
        Err(motivo) => {
            warn!(sesion = sesion_id, "no se pudo mirar tmux: {motivo}");
            avisar_en_pestana(
                estado,
                sesion_id,
                format!("no se pudo hablar con tmux en «{host}»: esta pestaña no es persistente"),
            );
            return Preparado::Normal;
        }
    };
    let Some(version) = lectura.tmux else {
        avisar_en_pestana(
            estado,
            sesion_id,
            format!("{host} no tiene tmux: esta pestaña no es persistente"),
        );
        return Preparado::Normal;
    };
    if version < VersionTmux::MINIMA {
        avisar_en_pestana(
            estado,
            sesion_id,
            format!(
                "tmux demasiado antiguo (3.0 o posterior) en {host}: esta pestaña no es persistente"
            ),
        );
        return Preparado::Normal;
    }
    let equipo = super::tmux::etiqueta_del_equipo(estado).await;
    let mut en_host: Vec<String> = lectura
        .sesiones
        .iter()
        .filter(|sesion| sesion.servidor == ServidorTmux::Magi)
        .map(|sesion| sesion.nombre.clone())
        .collect();
    // Si no se pueden leer las fichas, sus nombres reservados son un
    // desconocido: elegir uno a ciegas chocaría después con el UNIQUE.
    let Some(en_fichas) = nombres_de_fichas(estado, datos.host.id).await else {
        avisar_en_pestana(
            estado,
            sesion_id,
            format!("no se pudieron leer las sesiones de «{host}»: esta pestaña no es persistente"),
        );
        return Preparado::Normal;
    };
    for _ in 0..INTENTOS_NOMBRE_AUTOMATICO {
        let nombre = nombre_automatico(&equipo, &en_host, &en_fichas);
        let orden = orden_crear(datos, &nombre, None, None);
        match super::tmux::crear(handle, &orden).await {
            Ok(creada) => match creada.lectura.resultado {
                ResultadoCrear::Creada => {
                    let Some(comando) =
                        comando_de_enganche(ServidorTmux::Magi, &nombre, true, &equipo)
                    else {
                        return Preparado::Normal;
                    };
                    return Preparado::Enganchar(PlanEnganche {
                        servidor: ServidorTmux::Magi,
                        nombre,
                        version: creada.lectura.tmux.or(Some(version)),
                        compartir: false,
                        comando,
                        nueva: true,
                        con_comando_propio: false,
                    });
                }
                ResultadoCrear::Existe => en_host.push(nombre),
                _ => {
                    avisar_en_pestana(
                        estado,
                        sesion_id,
                        format!("no se pudo crear la sesión en «{host}»: esta pestaña no es persistente"),
                    );
                    return Preparado::Normal;
                }
            },
            Err(motivo) => {
                warn!(
                    sesion = sesion_id,
                    "no se pudo crear la sesión de tmux: {motivo}"
                );
                avisar_en_pestana(
                    estado,
                    sesion_id,
                    format!(
                        "no se pudo hablar con tmux en «{host}»: esta pestaña no es persistente"
                    ),
                );
                return Preparado::Normal;
            }
        }
    }
    avisar_en_pestana(
        estado,
        sesion_id,
        format!("no se encontró un nombre libre en «{host}»: esta pestaña no es persistente"),
    );
    Preparado::Normal
}

/// Nombres de las fichas de sesiones persistentes de un host (las terminadas
/// también cuentan: su nombre sigue reservado). Se lee de SQLite sin escribir;
/// `None` si no se pudo leer.
async fn nombres_de_fichas(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<Vec<String>> {
    let lectura = estado.lock().await.lectura.clone();
    lectura.lock().ok().and_then(|almacen| {
        crate::almacen::sesiones_persistentes::nombres_de_host(almacen.conexion(), host_id).ok()
    })
}

/// La sesión de tmux de la pestaña tal como está viva ahora: servidor, nombre
/// y si se enganchó compartiendo. Es de donde sale la reconexión.
async fn tmux_vivo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
) -> Option<(ServidorTmux, String, bool)> {
    let estado_bloqueado = estado.lock().await;
    let sesion = estado_bloqueado.sesiones.get(&sesion_id)?;
    Some((
        sesion.tmux_servidor,
        sesion.tmux.clone()?,
        sesion.tmux_compartir,
    ))
}

/// Reconexión de una pestaña con tmux (§4.4): mira el estado de la sesión con
/// `estado` y decide. La automática nunca le quita la sesión a otro equipo; la
/// manual (prefijo `r` o volver desde una pestaña soltada) manda el usuario.
async fn preparar_reconexion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    handle: &Arc<Handle<Cliente>>,
    auto: bool,
) -> Preparado {
    let sesion_id = datos.sesion_id;
    let host = datos.host.nombre.as_str();
    // El nombre sale del estado vivo de la pestaña, no de lo que se pidió al
    // abrirla (un host marcado pone el nombre automático al abrir).
    let Some((servidor, nombre, compartir)) = tmux_vivo(estado, sesion_id).await else {
        return Preparado::Normal;
    };
    let lectura = match super::tmux::estado(handle).await {
        Ok(lectura) => lectura,
        Err(motivo) => {
            warn!(sesion = sesion_id, "no se pudo comprobar tmux: {motivo}");
            return Preparado::SinComprobar(format!(
                "no se pudo comprobar tmux en «{host}»: {motivo} · prefijo r para reintentar"
            ));
        }
    };
    let perdida = |texto: String| match servidor {
        // Una sesión propia que ya no existe: la pestaña se cierra, como al
        // hacer `exit`. En la del tmux habitual, la Fase 15 seguía con un
        // shell normal y un aviso.
        ServidorTmux::Magi => Preparado::Terminada(texto),
        ServidorTmux::Habitual => {
            avisar_en_pestana(estado, sesion_id, format!("{texto}: shell normal"));
            Preparado::Normal
        }
    };
    let Some(version) = lectura.tmux else {
        return perdida(format!("«{host}» ya no tiene tmux"));
    };
    if servidor == ServidorTmux::Magi && version < VersionTmux::MINIMA {
        return Preparado::SinComprobar(format!(
            "tmux demasiado antiguo (3.0 o posterior) en «{host}»"
        ));
    }
    let Some(sesion) = lectura
        .sesiones
        .iter()
        .find(|sesion| sesion.servidor == servidor && sesion.nombre == nombre)
    else {
        return perdida(format!("la sesión «{nombre}» ya no existe en «{host}»"));
    };
    let equipo = super::tmux::etiqueta_del_equipo(estado).await;
    // Solo en el tmux propio hay equipos y se suelta a nadie.
    let soltar = servidor == ServidorTmux::Magi && !compartir;
    if servidor == ServidorTmux::Magi && auto && sesion.clientes > 0 {
        // Clientes enganchados: si el último equipo en engancharse es este, son
        // los de la conexión caída y se vuelve; si no, la tiene otro.
        if sesion.equipo.as_deref() != Some(equipo.as_str()) {
            return Preparado::Soltada(sesion.equipo.clone());
        }
    }
    let Some(comando) = comando_de_enganche(servidor, &nombre, soltar, &equipo) else {
        return Preparado::SinComprobar("los datos de la sesión no son válidos".to_string());
    };
    Preparado::Enganchar(PlanEnganche {
        servidor,
        nombre,
        version: Some(version),
        compartir,
        comando,
        nueva: false,
        con_comando_propio: false,
    })
}

/// Marca la sesión abierta con la conexión tomada y su identidad. Devuelve si
/// es la primera pestaña abierta de esa conexión (para no repetir la
/// anotación de conexión).
async fn marcar_abierta(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    tomada: &ConexionTomada,
    tmux: Option<TmuxAbierta>,
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
    // Volver a engancharse quita la marca de «soltada». Una pestaña que cae a
    // un shell normal (la del tmux habitual que ya no existe) deja de ser de
    // tmux.
    sesion.soltada = None;
    match tmux {
        Some(tmux) => {
            sesion.tmux = Some(tmux.nombre);
            sesion.tmux_servidor = tmux.servidor;
            sesion.tmux_version = tmux.version;
            sesion.tmux_compartir = tmux.compartir;
        }
        None => {
            sesion.tmux = None;
            sesion.tmux_version = None;
            sesion.tmux_compartir = false;
        }
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
    // Un shell nuevo empieza sin modos de teclado: lo que el programa de antes
    // dejó puesto (`modifyOtherKeys` y las banderas de kitty) se apaga en el
    // emulador del servidor y en los de las ventanas (el texto va a los dos),
    // antes de leer nada del canal nuevo. Con tmux no: es él quien los vuelve a
    // pedir al engancharse.
    let reinicio = if tmux.is_none() {
        "\x1b[>4;0m\x1b[<99u"
    } else {
        ""
    };
    let texto = format!("{reinicio}\r\n\x1b[2m── reconectada {hora} · {detalle} ──\x1b[0m\r\n");
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
    /// Una pestaña de tmux cuyo script salió con 100: otro equipo se enganchó
    /// a la sesión, que sigue viva. La pestaña queda soltada.
    Soltada,
    /// tmux no pudo (o dejó de poder): la sesión ya no existe, no hay tmux o es
    /// antiguo (la pestaña se cierra), o el cliente de tmux no pudo
    /// engancharse (queda caída). No es una caída de red: no se reintenta.
    TmuxFallo {
        causa: CausaAperturaTmux,
        mensaje: String,
        cerrar: bool,
    },
}

/// Lo que se espera a que tmux pinte antes de escribir los comandos iniciales
/// si no llega nada del canal.
const PLAZO_COMANDOS_DIFERIDOS: Duration = Duration::from_secs(2);

enum FinReintento {
    /// La pestaña se cerró mientras esperaba (o el servidor se apaga).
    Cerrada,
    /// Pasado `reconexion_max_min` sin éxito.
    Agotada(String),
}

/// Bucle de una sesión abierta: teclas y tamaños de los adjuntos hacia el
/// remoto, datos del remoto hacia los adjuntos y el parser del servidor, que
/// responde a las consultas en el mismo lote.
///
/// Con `tmux`, el canal es el del `exec` del script de enganche y su código de
/// salida dice cómo acabó (0: la sesión terminó, 100: te soltaron, 75, 74 y
/// 76: no puede). `diferidos` son los comandos iniciales de una sesión nueva:
/// se escriben cuando llega lo primero que pinta tmux (o a los 2 s).
async fn bucle_sesion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    canal: &mut Channel<russh::client::Msg>,
    pantalla: &PantallaCompartida,
    rx_comandos: &mut mpsc::UnboundedReceiver<ComandoSesion>,
    tmux: bool,
    mut diferidos: Vec<String>,
) -> FinSesion {
    let mut vio_eof = false;
    // Llegó el estado de salida del remoto: no es una caída de red, es un
    // cierre normal de la pestaña aunque el canal tarde en cerrarse.
    let mut vio_exit = false;
    let mut codigo_salida: Option<u32> = None;
    let mut limite_diferidos =
        (!diferidos.is_empty()).then(|| tokio::time::Instant::now() + PLAZO_COMANDOS_DIFERIDOS);
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
                    if !diferidos.is_empty() {
                        limite_diferidos = None;
                        escribir_diferidos(sesion_id, canal, &mut diferidos).await;
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
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    vio_exit = true;
                    codigo_salida = Some(exit_status);
                }
                Some(ChannelMsg::ExitSignal { .. }) => {
                    vio_exit = true;
                }
                Some(ChannelMsg::Failure) if tmux => {
                    // El host rechazó el `exec` y no va a cerrar el canal (con
                    // su PTY): se cierra aquí o seguiría ocupando uno de los
                    // canales de la conexión, que puede ser del pool.
                    let _ = tokio::time::timeout(PLAZO_CANAL, canal.close()).await;
                    return FinSesion::TmuxFallo {
                        causa: CausaAperturaTmux::Otra,
                        mensaje: "el host no admitió lanzar tmux en la pestaña".to_string(),
                        cerrar: true,
                    };
                }
                Some(ChannelMsg::Close) => {
                    return fin_de_canal(estado, sesion_id, tmux, codigo_salida).await;
                }
                None => {
                    if vio_eof || vio_exit {
                        return fin_de_canal(estado, sesion_id, tmux, codigo_salida).await;
                    }
                    return FinSesion::Caida("la conexión se ha perdido".to_string());
                }
                _ => {}
            },
            _ = tokio::time::sleep_until(limite_diferidos.unwrap_or_else(tokio::time::Instant::now)),
                if limite_diferidos.is_some() =>
            {
                limite_diferidos = None;
                escribir_diferidos(sesion_id, canal, &mut diferidos).await;
            }
        }
    }
}

/// Escribe los comandos iniciales que esperaban a que tmux pintara.
async fn escribir_diferidos(
    sesion_id: u32,
    canal: &mut Channel<russh::client::Msg>,
    diferidos: &mut Vec<String>,
) {
    for texto in std::mem::take(diferidos) {
        let escrito = tokio::time::timeout(PLAZO_CANAL, canal.data_bytes(texto.into_bytes())).await;
        if !matches!(escrito, Ok(Ok(()))) {
            warn!(sesion = sesion_id, "no se pudo escribir el comando inicial");
            return;
        }
    }
}

/// El canal de la pestaña terminó con el estado de salida (o sin él). En una
/// pestaña de tmux, el código del script dice cómo (§7.4); en una normal, es
/// el `exit` de siempre.
async fn fin_de_canal(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    tmux: bool,
    codigo: Option<u32>,
) -> FinSesion {
    if !tmux {
        cerrada(estado, sesion_id, "el remoto cerró la sesión").await;
        return FinSesion::Cerrada;
    }
    let fallo = |causa, mensaje: &str, cerrar| FinSesion::TmuxFallo {
        causa,
        mensaje: mensaje.to_string(),
        cerrar,
    };
    match codigo.map(|codigo| codigo as i32) {
        Some(SALIDA_SOLTADA) => FinSesion::Soltada,
        Some(SALIDA_NO_EXISTE) => fallo(
            CausaAperturaTmux::Otra,
            "la sesión de tmux ya no existe en el host",
            true,
        ),
        Some(SALIDA_SIN_TMUX) => fallo(CausaAperturaTmux::SinTmux, "instala tmux en el host", true),
        Some(SALIDA_ANTIGUO) => fallo(
            CausaAperturaTmux::TmuxAntiguo,
            "tmux demasiado antiguo (3.0 o posterior)",
            true,
        ),
        Some(otro) if otro != 0 => FinSesion::TmuxFallo {
            causa: CausaAperturaTmux::Otra,
            mensaje: format!(
                "tmux no pudo engancharse (código {otro}) · prefijo r para reintentar"
            ),
            cerrar: false,
        },
        // Sin código de salida (solo una señal, o el canal se cerró sin
        // `exit-status`) no se sabe si la sesión terminó: tratarlo como
        // terminada cerraría una pestaña de una sesión que puede seguir viva.
        // Es una caída, y la reconexión mira el host.
        None => FinSesion::Caida("la conexión se ha perdido".to_string()),
        Some(_) => {
            cerrada(estado, sesion_id, "la sesión de tmux terminó").await;
            FinSesion::Cerrada
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
        // Solo en Termux: la marca de atención del aviso de Android.
        let termux = estado_bloqueado.termux.clone();
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
            // Un aviso del emulador del remoto (no los internos del servidor,
            // que también pasan por `aviso`) que nadie mira pide atención. El
            // tope de uno cada 5 s es de lo que se enseña, no de esto. Solo
            // la transición a «pide atención» despierta a la tarea de avisos:
            // una salida con cientos de campanas no lanza un ciclo por lote.
            if let Some(termux) = &termux {
                if !crate::termux::avisos::mirando(sesion.enfocada()) && sesion.pedir_atencion() {
                    termux.despertar_avisos();
                }
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

// ---------------------------------------------------------------- tmux: fin

/// Avisa solo al solicitante de que la pestaña de tmux no pudo abrirse (o
/// dejó de poder). Si la ventana ya no está, no hay a quién avisar.
fn avisar_fallo_de_tmux(
    estado_bloqueado: &EstadoServidor,
    solicitante: u32,
    host_id: i64,
    nombre: Option<&str>,
    causa: CausaAperturaTmux,
    mensaje: &str,
) {
    if let Some(cliente) = estado_bloqueado.clientes.get(&solicitante) {
        difusion::enviar(
            cliente,
            MensajeServidor::AperturaTmuxFallida {
                host_id,
                nombre: nombre.unwrap_or_default().to_string(),
                causa,
                mensaje: mensaje.to_string(),
            },
        );
    }
}

/// La apertura pedida con tmux no pudo hacerse (sin tmux, tmux antiguo, ya
/// existe, sin carpeta…): no queda pestaña. El servidor no anota nada (no se
/// produjo ningún efecto) ni toca el estado del host: la conexión estaba bien.
/// El solicitante recibe `AperturaTmuxFallida` con la causa y la instrucción
/// para ofrecer engancharse a la existente, cambiar el nombre o abrir una
/// pestaña normal.
async fn apertura_tmux_fallida(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    causa: CausaAperturaTmux,
    mensaje: String,
) {
    // Solo la causa va al log: el mensaje puede llevar la carpeta del usuario.
    warn!(
        sesion = datos.sesion_id,
        host = %datos.host.nombre,
        "apertura de tmux fallida: {causa:?}"
    );
    let nombre = datos.tmux.as_ref().map(|tmux| tmux.nombre.clone());
    let mut estado_bloqueado = estado.lock().await;
    estado_bloqueado.pendientes.remove(&datos.sesion_id);
    if estado_bloqueado.sesiones.remove(&datos.sesion_id).is_some() {
        avisar_fallo_de_tmux(
            &estado_bloqueado,
            datos.solicitante,
            datos.host.id,
            nombre.as_deref(),
            causa,
            &mensaje,
        );
        avisar_fallida(&mut estado_bloqueado, datos.sesion_id, &mensaje);
    }
}

/// Una reconexión encontró que la sesión de tmux ya no existe en el host: la
/// pestaña se cierra como al hacer `exit` y el solicitante sabe por qué.
async fn sesion_terminada_en_el_host(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    motivo: &str,
) {
    let nombre = tmux_vivo(estado, datos.sesion_id)
        .await
        .map(|(_, nombre, _)| nombre);
    {
        let estado_bloqueado = estado.lock().await;
        avisar_fallo_de_tmux(
            &estado_bloqueado,
            datos.solicitante,
            datos.host.id,
            nombre.as_deref(),
            CausaAperturaTmux::Otra,
            motivo,
        );
    }
    cerrada(estado, datos.sesion_id, "la sesión de tmux terminó").await;
}

/// El script de enganche salió con 75, 74 o 76 con la pestaña ya abierta (la
/// sesión desapareció entre comprobar y engancharse): se cierra y se avisa.
async fn sesion_tmux_cerrada(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    causa: CausaAperturaTmux,
    mensaje: String,
) {
    let nombre = tmux_vivo(estado, datos.sesion_id)
        .await
        .map(|(_, nombre, _)| nombre)
        .or_else(|| datos.tmux.as_ref().map(|tmux| tmux.nombre.clone()));
    {
        let estado_bloqueado = estado.lock().await;
        avisar_fallo_de_tmux(
            &estado_bloqueado,
            datos.solicitante,
            datos.host.id,
            nombre.as_deref(),
            causa,
            &mensaje,
        );
    }
    cerrada(estado, datos.sesion_id, &mensaje).await;
}

/// Texto del motivo de una pestaña soltada.
fn motivo_soltada(por_equipo: Option<&str>) -> String {
    match por_equipo {
        Some(equipo) => format!("soltada: la usa {equipo}"),
        None => "soltada: la usa otro equipo".to_string(),
    }
}

/// La pestaña queda caída con su última pantalla y sin conexión, marcada como
/// soltada (nadie la cierra: se vuelve con `Reconectar` o se cierra a mano).
/// Difunde la lista ya, antes de cualquier trabajo lento. Devuelve el instante
/// en que se soltó (para no pisar una soltada posterior).
async fn marcar_soltada(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    por_equipo: Option<String>,
) -> i64 {
    let desde = Utc::now().timestamp();
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
        return desde;
    };
    sesion.estado = EstadoSesionRemota::Caida;
    sesion.reconectando = false;
    sesion.reconexion = None;
    sesion.adelantar = None;
    sesion.motivo = Some(motivo_soltada(por_equipo.as_deref()));
    // La conexión la suelta quien llama, por identidad.
    sesion.handle = None;
    sesion.soltada = Some(Soltada { por_equipo, desde });
    let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
    let aviso = MensajeServidor::Estado {
        sesion_id,
        estado: EstadoSesionRemota::Caida,
        motivo: sesion.motivo.clone(),
    };
    difusion::difundir_a_adjuntos(&estado_bloqueado.clientes, &ids, aviso);
    estado_bloqueado.pendientes.remove(&sesion_id);
    difundir_lista(&mut estado_bloqueado);
    drop(estado_bloqueado);
    // Sin canal, la escritura en varios y los túneles temporales de la pestaña
    // terminan como en una caída (se pueden volver a pedir al volver).
    super::escritura_varios::sesion_acabo(estado, sesion_id, "la sesión se soltó").await;
    super::tuneles_temporales::sesion_acabo(estado, sesion_id, "la sesión se soltó").await;
    desde
}

/// El script de enganche salió con 100: otro equipo se llevó la sesión. La
/// pestaña pasa a soltada y se difunde ya; después, por la misma conexión y a la
/// vez, se mira con `estado` quién la tiene ahora y con `vista` qué muestra. La
/// última pantalla se deja en el emulador de la pestaña antes de difundir quién
/// la tiene (si la captura falla, la pestaña queda soltada como estaba).
async fn sesion_soltada(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    handle: &Arc<Handle<Cliente>>,
    plan: &PlanEnganche,
) {
    let desde = marcar_soltada(estado, sesion_id, None).await;
    let Some(filas) = filas_de_la_pantalla(estado, sesion_id).await else {
        // La pestaña se cerró mientras tanto: no queda nada que actualizar.
        return;
    };
    let (por_equipo, vista) = tokio::join!(
        equipo_que_la_tiene(sesion_id, handle, plan),
        super::tmux::vista(handle, plan.servidor, &plan.nombre, filas)
    );
    match vista {
        Ok(texto) => pintar_ultima_pantalla(estado, sesion_id, desde, &texto).await,
        // No es un error: la pestaña queda soltada como estaba.
        Err(motivo) => warn!(
            sesion = sesion_id,
            "no se pudo leer la última pantalla de la sesión soltada: {motivo}"
        ),
    }
    if por_equipo.is_none() {
        return;
    }
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
        return;
    };
    // El usuario pudo volver o cerrar mientras tanto.
    if sesion.estado != EstadoSesionRemota::Caida {
        return;
    }
    let Some(soltada) = sesion.soltada.as_mut() else {
        return;
    };
    if soltada.desde != desde {
        return;
    }
    soltada.por_equipo = por_equipo.clone();
    sesion.motivo = Some(motivo_soltada(por_equipo.as_deref()));
    difundir_lista(&mut estado_bloqueado);
}

/// El equipo que tiene ahora la sesión de una pestaña soltada, si se sabe:
/// el que dejó su marca en `@magi_equipo` y tiene algún cliente enganchado.
async fn equipo_que_la_tiene(
    sesion_id: u32,
    handle: &Arc<Handle<Cliente>>,
    plan: &PlanEnganche,
) -> Option<String> {
    match super::tmux::estado(handle).await {
        Ok(lectura) => lectura
            .sesiones
            .iter()
            .find(|sesion| sesion.servidor == plan.servidor && sesion.nombre == plan.nombre)
            .filter(|sesion| sesion.clientes > 0)
            .and_then(|sesion| sesion.equipo.clone()),
        Err(motivo) => {
            warn!(
                sesion = sesion_id,
                "no se pudo ver quién tiene la sesión: {motivo}"
            );
            None
        }
    }
}

/// Las filas del emulador de la pestaña, que son las que se piden a `vista`.
/// `None` si la pestaña ya no existe.
async fn filas_de_la_pantalla(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
) -> Option<u32> {
    let pantalla = estado
        .lock()
        .await
        .sesiones
        .get(&sesion_id)?
        .pantalla
        .clone();
    let filas = pantalla.lock().ok()?.filas();
    Some(u32::from(filas).max(1))
}

/// Los bytes que dejan en un emulador la última pantalla de una sesión: salir
/// de la pantalla alternativa si estaba activa (así la vista cae en la
/// principal, que sobrevive a los volcados de `Adjuntar` y a los rehaceres
/// aplazados), borrar la pantalla (con el `[detached (from session …)]` que
/// escribió el cliente de tmux al soltarlo) y pintar las últimas `filas` líneas
/// de la vista desde arriba. La vista ya viene sin caracteres de control.
fn bytes_de_vista(vista: &str, filas: u16, alternativa: bool) -> Vec<u8> {
    let mut lineas: Vec<&str> = vista.lines().collect();
    while lineas.last().is_some_and(|linea| linea.trim().is_empty()) {
        lineas.pop();
    }
    let maximo = usize::from(filas.max(1));
    if lineas.len() > maximo {
        lineas.drain(..lineas.len() - maximo);
    }
    let mut bytes = Vec::with_capacity(vista.len() + 32);
    if alternativa {
        bytes.extend_from_slice(b"\x1b[?1049l");
    }
    bytes.extend_from_slice(b"\x1b[0m\x1b[2J\x1b[H");
    bytes.extend_from_slice(lineas.join("\r\n").as_bytes());
    bytes
}

/// Deja la última pantalla de la sesión en el emulador de la pestaña soltada
/// y se la manda a las ventanas adjuntas (el mismo texto, como la línea de
/// reconexión). El cliente de tmux al que le quitaron la sesión salió de la
/// pantalla alternativa y escribió su `[detached …]` en la normal: el host es
/// la fuente de verdad de lo que muestra la sesión, así que se sustituye por su
/// vista. Solo si la pestaña sigue soltada por la misma soltada: si el usuario
/// ya volvió (o cerró, o la soltaron otra vez), esta captura llegó tarde y no
/// debe pisar lo que pinta tmux. Se hace con el bloqueo del estado tomado para
/// que `Reconectar` no pueda colarse entre la comprobación y el pintado.
async fn pintar_ultima_pantalla(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    desde: i64,
    vista: &str,
) {
    let estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get(&sesion_id) else {
        return;
    };
    let sigue_soltada = sesion.estado == EstadoSesionRemota::Caida
        && sesion
            .soltada
            .as_ref()
            .is_some_and(|soltada| soltada.desde == desde);
    if !sigue_soltada {
        return;
    }
    let bytes = {
        let Ok(mut parser) = sesion.pantalla.lock() else {
            return;
        };
        let alternativa = parser.screen().alternate_screen();
        let bytes = bytes_de_vista(vista, parser.filas(), alternativa);
        // El texto es nuestro y no lleva consultas: no hay respuestas que
        // escribir al remoto (que además ya no tiene canal).
        let _ = parser.procesar(&bytes);
        bytes
    };
    let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
    if ids.is_empty() {
        return;
    }
    difusion::difundir_a_adjuntos(
        &estado_bloqueado.clientes,
        &ids,
        MensajeServidor::Datos { sesion_id, bytes },
    );
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
        let termux = estado_bloqueado.termux.clone();
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return;
        };
        let antes = sesion.enfocada();
        if enfocada {
            sesion.enfocadas.insert(cliente_id);
            sesion.ventana_enfocada = Some(cliente_id);
            // Una ventana que la enfoca la está atendiendo: se retira su aviso
            // de Android.
            if sesion.atendida() {
                if let Some(termux) = &termux {
                    termux.despertar_avisos();
                }
            }
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

    // ------------------------------------------------ última pantalla soltada

    /// Un emulador con lo que deja el cliente de tmux al que le quitan la
    /// sesión: sale de la pantalla alternativa y escribe su aviso en la normal.
    fn pantalla_tras_soltar() -> PantallaEmulada {
        let mut pantalla = PantallaEmulada::nueva(24, 80, 1000, true);
        pantalla.procesar(b"antes de tmux\r\n");
        pantalla.procesar(b"\x1b[?1049h\x1b[2J\x1b[Hlo que pintaba tmux");
        pantalla.procesar(b"\x1b[?1049l\x1b[2J\x1b[H[detached (from session magi-a-1)]\r\n");
        pantalla
    }

    #[test]
    fn la_vista_sustituye_al_aviso_de_soltado() {
        let mut pantalla = pantalla_tras_soltar();
        assert!(pantalla.screen().contents().contains("[detached"));
        let bytes = bytes_de_vista("$ make test\nok 12 tests\n", 24, false);
        pantalla.procesar(&bytes);
        let contenido = pantalla.screen().contents();
        assert_eq!(contenido, "$ make test\nok 12 tests");
        assert!(!pantalla.screen().alternate_screen());
    }

    #[test]
    fn la_vista_se_pinta_desde_arriba_y_solo_con_las_ultimas_filas() {
        let mut pantalla = PantallaEmulada::nueva(3, 20, 100, true);
        let bytes = bytes_de_vista("uno\ndos\ntres\ncuatro\n\n   \n", 3, false);
        pantalla.procesar(&bytes);
        assert_eq!(pantalla.screen().contents(), "dos\ntres\ncuatro");
        assert_eq!(pantalla.screen().cell(0, 0).unwrap().contents(), "d");
    }

    #[test]
    fn una_vista_vacia_solo_borra_la_pantalla() {
        let mut pantalla = pantalla_tras_soltar();
        let bytes = bytes_de_vista("", 24, false);
        pantalla.procesar(&bytes);
        assert_eq!(pantalla.screen().contents(), "");
    }

    #[test]
    fn solo_sale_de_la_pantalla_alternativa_si_estaba_en_ella() {
        assert!(!bytes_de_vista("x", 24, false)
            .windows(8)
            .any(|ventana| ventana == b"\x1b[?1049l"));
        assert!(bytes_de_vista("x", 24, true).starts_with(b"\x1b[?1049l"));
    }

    /// Una vista capturada con la pantalla alternativa activa cae en la
    /// principal y no se pierde: ni con los borrados y rehaceres que quedaron
    /// aplazados mientras estaba activa, ni en el volcado de `Adjuntar`.
    #[test]
    fn una_vista_pintada_con_la_alternativa_activa_no_se_pierde() {
        let mut pantalla = PantallaEmulada::nueva(24, 80, 1000, true);
        pantalla.procesar(b"principal\r\n");
        // La pantalla alternativa activa, con un ED 3 y un rehacer aplazados.
        pantalla.procesar(b"\x1b[?1049h\x1b[2J\x1b[Hpintaba tmux\x1b[3J");
        pantalla.redimensionar(24, 80, 10);
        assert!(pantalla.screen().alternate_screen());

        let bytes = bytes_de_vista("$ make test\nok 12 tests\n", 24, true);
        pantalla.procesar(&bytes);
        assert!(!pantalla.screen().alternate_screen());
        assert_eq!(pantalla.screen().contents(), "$ make test\nok 12 tests");

        // Lo siguiente que procese el parser aplica lo aplazado: la vista sigue.
        pantalla.procesar(b"");
        assert_eq!(pantalla.screen().contents(), "$ make test\nok 12 tests");
        // El volcado de `Adjuntar`, que es lo que ve una ventana nueva.
        let volcado = pantalla.screen().state_formatted();
        let mut ventana = PantallaEmulada::nueva(24, 80, 1000, false);
        ventana.procesar(&volcado);
        assert_eq!(ventana.screen().contents(), "$ make test\nok 12 tests");
        // Y las ventanas adjuntas, que reciben los mismos bytes, quedan igual.
        let mut adjunta = PantallaEmulada::nueva(24, 80, 1000, false);
        adjunta.procesar(b"\x1b[?1049hotra cosa");
        adjunta.procesar(&bytes);
        assert_eq!(adjunta.screen().contents(), "$ make test\nok 12 tests");
    }
}
