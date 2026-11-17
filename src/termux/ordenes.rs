//! Órdenes de Termux (`termux-*`): sin shell, con plazo, con tope de salida y
//! con apagado por proceso de Termux:API (§5.2, §7.2; D384, T139).
//!
//! Ninguna orden se espera en el bucle de la interfaz: van en tareas tokio o
//! en `spawn_blocking`. Si Termux:API no está o no responde, esa función se
//! apaga en este proceso hasta que reinicie y el aviso sale una sola vez.
//!
//! CONTRATO-S1-TERMUX-ORDENES: el cuerpo lo completa la pista P2 del Sprint 1.

use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::rutas::EntornoTermux;

/// Tope de la salida que se lee de una orden (entrada no fiable).
pub const TOPE_SALIDA: usize = 64 * 1024;

/// Las órdenes que dependen de la app Termux:API (el resto es de termux-tools).
pub const DE_LA_API: [&str; 4] = [
    "termux-clipboard-set",
    "termux-notification",
    "termux-notification-remove",
    "termux-battery-status",
];

/// Estado de Termux:API en un proceso (cliente o servidor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EstadoApi {
    #[default]
    SinComprobar,
    Disponible,
    /// Falta el paquete `termux-api` (la orden no está en el `PATH`).
    Ausente,
    /// La orden no responde en el plazo o falla: falta la app o está parada.
    NoResponde,
}

impl EstadoApi {
    pub fn apagada(self) -> bool {
        matches!(self, Self::Ausente | Self::NoResponde)
    }
}

/// El estado de Termux:API de un proceso, compartido entre sus tareas.
#[derive(Debug, Default)]
pub struct ApiProceso {
    estado: Mutex<EstadoApi>,
}

impl ApiProceso {
    pub fn estado(&self) -> EstadoApi {
        self.estado.lock().map(|e| *e).unwrap_or_default()
    }

    pub fn apagada(&self) -> bool {
        self.estado().apagada()
    }

    /// Apaga la función con un motivo (`Ausente` o `NoResponde`). Devuelve
    /// `true` solo la primera vez: quien lo recibe da el aviso de la barra.
    /// CONTRATO-S1-TERMUX-ORDENES.
    pub fn apagar(&self, motivo: EstadoApi) -> bool {
        let _ = motivo;
        false
    }

    /// Marca Termux:API como disponible tras una respuesta correcta.
    /// CONTRATO-S1-TERMUX-ORDENES.
    pub fn marcar_disponible(&self) {}
}

/// Por qué falla una orden de Termux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalloOrden {
    /// Termux:API está apagada en este proceso: ni se intenta.
    ApiApagada,
    /// El programa no está en el `PATH`.
    NoInstalada(String),
    /// No respondió en el plazo (se mató su grupo de procesos).
    SinRespuesta,
    /// Terminó con un código distinto de 0; `error` son 200 caracteres como mucho.
    Codigo {
        codigo: Option<i32>,
        error: String,
    },
    Io(String),
}

impl std::fmt::Display for FalloOrden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiApagada => f.write_str("Termux:API apagada"),
            Self::NoInstalada(programa) => write!(f, "{programa} no está instalado"),
            Self::SinRespuesta => f.write_str("no responde"),
            Self::Codigo { error, .. } if error.is_empty() => f.write_str("la orden falló"),
            Self::Codigo { error, .. } => f.write_str(error),
            Self::Io(causa) => f.write_str(causa),
        }
    }
}

impl std::error::Error for FalloOrden {}

/// Lanzador de órdenes de Termux de un proceso.
#[derive(Debug, Clone)]
pub struct Ordenes {
    #[allow(dead_code)]
    path: Option<OsString>,
    #[allow(dead_code)]
    plazo: Duration,
    api: Arc<ApiProceso>,
}

impl Ordenes {
    /// `plazo` es `[termux] api_plazo_seg` (2-30 s). El `PATH` sale del
    /// entorno inyectado, nunca de `std::env`.
    pub fn nuevas(entorno: &EntornoTermux, plazo: Duration, api: Arc<ApiProceso>) -> Self {
        Self {
            path: entorno.path.clone(),
            plazo,
            api,
        }
    }

    pub fn api(&self) -> &Arc<ApiProceso> {
        &self.api
    }

    /// Ejecuta `argumentos[0]` con el resto como argumentos sueltos (sin
    /// shell), con `entrada` por la entrada estándar, el plazo y el tope de
    /// salida; al vencer el plazo mata el grupo de procesos. Bloquea el hilo
    /// hasta `plazo`: llámala desde una tarea, nunca desde el bucle de la UI.
    /// CONTRATO-S1-TERMUX-ORDENES.
    pub fn ejecutar(
        &self,
        argumentos: &[&str],
        entrada: Option<&[u8]>,
    ) -> Result<Vec<u8>, FalloOrden> {
        let _ = (argumentos, entrada);
        Err(FalloOrden::Io("CONTRATO-S1-TERMUX-ORDENES".into()))
    }

    /// Lo mismo desde código asíncrono (en `spawn_blocking`).
    pub async fn ejecutar_async(
        &self,
        argumentos: Vec<String>,
        entrada: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, FalloOrden> {
        let ordenes = self.clone();
        tokio::task::spawn_blocking(move || {
            let referencias: Vec<&str> = argumentos.iter().map(String::as_str).collect();
            ordenes.ejecutar(&referencias, entrada.as_deref())
        })
        .await
        .map_err(|error| FalloOrden::Io(error.to_string()))?
    }
}
