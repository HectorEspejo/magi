//! Lo de Termux que vive en la App (Fase 17): las órdenes `termux-*` del
//! cliente (portapapeles y enlaces) y lo que el servidor dice por la barra.
//!
//! CONTRATO-S2-APP-TERMUX: lo completa la pista de portapapeles y enlaces del
//! Sprint 2. Aquí solo está el estado, para que la `App` lo lleve desde el
//! principio y ninguna otra pista tenga que añadir campos a `app.rs`.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::termux::estado::AvisoServidor;
use crate::termux::{ApiProceso, EntornoTermux, Ordenes};

/// Cada cuánto mira el cliente el fichero de estado del servidor (solo en
/// Termux, §5.2 y desviación 2).
pub const RITMO_ESTADO_SERVIDOR: Duration = Duration::from_secs(5);

/// El estado de Termux de una ventana de MAGI.
pub struct EstadoTermuxApp {
    /// Las órdenes `termux-*` del cliente, con su Termux:API apagable por
    /// proceso. Se pueden sustituir en las pruebas por unas con dobles.
    pub ordenes: Arc<Ordenes>,
    /// Qué servidor (su `arranque`) dijo ya cuáles de sus avisos en la barra:
    /// cada uno se dice una sola vez por servidor.
    pub avisos_dichos: Option<(i64, BTreeSet<AvisoServidor>)>,
    /// Última vez que se leyó el fichero de estado del servidor.
    pub ultima_lectura: Option<Instant>,
}

impl EstadoTermuxApp {
    /// Con el entorno del proceso y el plazo de `[termux] api_plazo_seg`.
    #[must_use]
    pub fn nuevo(plazo: Duration) -> Self {
        Self {
            ordenes: Arc::new(Ordenes::nuevas(
                &EntornoTermux::del_proceso(),
                plazo,
                Arc::new(ApiProceso::default()),
            )),
            avisos_dichos: None,
            ultima_lectura: None,
        }
    }
}
