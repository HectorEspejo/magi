//! Un cliente TUI adjunto al servidor: su identidad, las sesiones a las que
//! está adjunto con el tamaño de ventana que reporta, y el canal de escritura
//! hacia su socket.

use std::collections::HashMap;

use futures_util::SinkExt as _;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;
use tokio_util::codec::FramedWrite;

use crate::protocolo::{self, MensajeServidor};

/// Tamaño de ventana que un cliente adjunto reporta para una sesión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tamano {
    pub cols: u16,
    pub filas: u16,
}

/// Estado de un cliente conectado, visto por el servidor.
pub struct ClienteRemoto {
    pub id: u32,
    pub version: u32,
    pub pid: u32,
    /// Sesiones a las que está adjunto y el tamaño que reportó para cada una.
    pub adjunto_a: HashMap<u32, Tamano>,
    /// Cola hacia la tarea que escribe en su socket.
    pub tx: mpsc::UnboundedSender<MensajeServidor>,
}

impl ClienteRemoto {
    pub fn nuevo(
        id: u32,
        version: u32,
        pid: u32,
        tx: mpsc::UnboundedSender<MensajeServidor>,
    ) -> Self {
        Self {
            id,
            version,
            pid,
            adjunto_a: HashMap::new(),
            tx,
        }
    }
}

/// Lanza la tarea que drena la cola de mensajes hacia el socket del cliente.
/// Al caer la cola el socket se cierra.
pub fn lanzar_escritura(escritura: OwnedWriteHalf) -> mpsc::UnboundedSender<MensajeServidor> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut salida = FramedWrite::new(escritura, protocolo::codec());
        while let Some(mensaje) = rx.recv().await {
            let Ok(linea) = protocolo::codificar(&mensaje) else {
                break;
            };
            if salida.send(linea.as_str()).await.is_err() {
                break;
            }
        }
    });
    tx
}
