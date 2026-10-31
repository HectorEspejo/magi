//! Diálogos del núcleo móvil: peticiones que van a Kotlin por
//! OyenteNucleo::dialogo (huella nueva, huella cambiada y contraseña) y
//! respuestas que vuelven por Nucleo::responder_dialogo.
//!
//! Cada diálogo lleva un id correlacionado con la operación que lo espera; una
//! respuesta de una petición que ya no está en vuelo se descarta.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::oneshot;

use crate::api::RespuestaDialogo;

/// Diálogos abiertos y su canal de respuesta.
#[derive(Default)]
pub struct Dialogos {
    abiertos: Mutex<HashMap<u64, oneshot::Sender<RespuestaDialogo>>>,
    siguiente: AtomicU64,
}

impl Dialogos {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Registra un diálogo y devuelve su id y el receptor de la respuesta.
    pub fn registrar(&self) -> (u64, oneshot::Receiver<RespuestaDialogo>) {
        let id = self.siguiente.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        if let Ok(mut abiertos) = self.abiertos.lock() {
            abiertos.insert(id, tx);
        }
        (id, rx)
    }

    /// Entrega la respuesta a quien espera. Falso si el diálogo ya no está en
    /// vuelo (se descarta sin ruido).
    pub fn responder(&self, id: u64, respuesta: RespuestaDialogo) -> bool {
        let emisor = self
            .abiertos
            .lock()
            .ok()
            .and_then(|mut abiertos| abiertos.remove(&id));
        match emisor {
            Some(emisor) => emisor.send(respuesta).is_ok(),
            None => false,
        }
    }

    /// Descarta un diálogo que ya no interesa (la operación se canceló).
    pub fn descartar(&self, id: u64) {
        if let Ok(mut abiertos) = self.abiertos.lock() {
            abiertos.remove(&id);
        }
    }

    /// Cancela todos los diálogos (al cerrar el núcleo o borrar datos).
    pub fn cancelar_todos(&self) {
        if let Ok(mut abiertos) = self.abiertos.lock() {
            abiertos.clear();
        }
    }
}
