use std::collections::HashMap;
use std::path::PathBuf;

use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

use crate::modelo::{EstadoSesion, Host};

pub mod cliente;
pub mod huellas;
pub mod reenvios;
pub mod salto;
pub mod terminal;

pub use terminal::Pantalla;

/// Handles de las sesiones vivas, para que el sondeo abra un canal `exec`
/// sobre la misma conexión en vez de abrir una efímera.
pub type RegistroSesiones = std::sync::Arc<
    tokio::sync::Mutex<HashMap<i64, std::sync::Arc<russh::client::Handle<cliente::Cliente>>>>,
>;

pub fn registro_sesiones() -> RegistroSesiones {
    std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()))
}

/// Todo lo que necesita la tarea de conexión, calculado en el hilo de la UI.
pub struct PlanConexion {
    pub host: Host,
    pub todos_los_hosts: HashMap<i64, Host>,
    pub known_hosts: PathBuf,
    pub dir_ssh: PathBuf,
    pub hogar: PathBuf,
    pub usuario_local: String,
    pub solo_prueba: bool,
    pub cols: u16,
    pub filas: u16,
    pub registro: RegistroSesiones,
    /// Llavero de contraseñas de host, inyectado por la plataforma.
    pub llavero: std::sync::Arc<dyn crate::llavero::LlaveroHost>,
}

/// Comandos de la UI hacia la sesión.
#[derive(Debug)]
pub enum ComandoConexion {
    Teclas(Vec<u8>),
    Redimensionar(u16, u16),
    Cerrar,
}

/// Eventos de la sesión hacia la UI.
pub enum EventoConexion {
    Estado {
        host_id: i64,
        estado: EstadoSesion,
    },
    HuellaDesconocida {
        host: String,
        tipo: String,
        huella: String,
        responder: oneshot::Sender<bool>,
    },
    HuellaCambiada {
        host: String,
        tipo: String,
        anterior: String,
        nueva: String,
        responder: oneshot::Sender<bool>,
    },
    PideFrase {
        host: String,
        intento: u8,
        responder: oneshot::Sender<Option<Zeroizing<String>>>,
    },
    /// El host autentica con contraseña. La respuesta incluye si se debe
    /// guardar en el llavero del sistema.
    PideContrasena {
        host: String,
        intento: u8,
        recordar_por_defecto: bool,
        responder: oneshot::Sender<Option<(Zeroizing<String>, bool)>>,
    },
    /// Conexión automática (ejecución, túnel automático) de un host con
    /// contraseña en el llavero: el solicitante la resuelve desde su llavero,
    /// sin diálogo. `None` si no la tiene.
    PideContrasenaLlavero {
        host: String,
        usuario: String,
        responder: oneshot::Sender<Option<Zeroizing<String>>>,
    },
    /// Resultado de intentar guardar la contraseña en el llavero.
    ContrasenaGuardada {
        host_id: i64,
        ok: bool,
        motivo: Option<String>,
    },
    /// Una huella nueva o sustituida se escribió en `known_hosts`.
    HuellaRegistrada {
        host_id: i64,
        anterior: Option<String>,
        nueva: String,
    },
    Abierta {
        host_id: i64,
        pantalla: Pantalla,
        identidad: String,
        huella: Option<String>,
        cols: u16,
        filas: u16,
    },
    Pantalla,
    PruebaOk {
        host_id: i64,
        identidad: String,
        huella: Option<String>,
    },
    Cerrada {
        host_id: i64,
        motivo: Option<String>,
    },
    Error {
        host_id: i64,
        motivo: String,
    },
    Cancelada {
        host_id: i64,
    },
}

/// De dónde sale una contraseña cuando el host autentica con una.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuenteContrasena {
    /// El propio proceso la lee del llavero del sistema (conexiones efímeras
    /// del cliente: prueba de ficha y sondeo).
    Llavero,
    /// La aporta el cliente solicitante por el protocolo (sesiones del
    /// servidor); el servidor jamás toca el llavero.
    Solicitante,
    /// Conexiones automáticas del servidor (ejecuciones de snippets): el
    /// solicitante la saca de su llavero, sin diálogo; si no la tiene, la
    /// conexión falla con instrucción.
    SolicitanteLlavero,
}

/// Lanza la tarea tokio de la sesión y devuelve el canal de comandos. La tarea
/// emite EventoConexion; cada plataforma decide cómo llevarlo a su bucle.
pub fn lanzar(
    runtime: &tokio::runtime::Runtime,
    plan: PlanConexion,
    tx: mpsc::UnboundedSender<EventoConexion>,
) -> mpsc::UnboundedSender<ComandoConexion> {
    let (tx_comandos, rx_comandos) = mpsc::unbounded_channel();
    runtime.spawn(async move {
        cliente::sesion_completa(plan, rx_comandos, tx).await;
    });
    tx_comandos
}
