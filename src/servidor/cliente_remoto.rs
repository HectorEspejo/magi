//! Un cliente TUI adjunto al servidor: su identidad, las sesiones a las que
//! está adjunto con el tamaño de ventana que reporta, y el canal de escritura
//! hacia su socket.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

/// Cuántos mensajes encolados hacia el socket del cliente están pendientes de
/// escribirse. Lo usa la vista Servicios para no mandar lotes de log a un
/// solicitante que no da abasto (contrapresión, §7.6): con el escritor
/// ocupado, el lote espera en la cola del servidor, que descarta las líneas
/// más viejas cuando pasa de 4 MiB.
#[derive(Debug, Clone, Default)]
pub struct Pendientes(Arc<AtomicUsize>);

impl Pendientes {
    pub fn nuevos() -> Self {
        Self::default()
    }

    /// Mensajes recibidos por la tarea de escritura y aún no escritos.
    pub fn actual(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    /// ¿El escritor está libre para admitir otro mensaje?
    pub fn hay_hueco(&self) -> bool {
        self.actual() == 0
    }

    fn incrementar(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    fn decrementar(&self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
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
    /// Mensajes encolados pendientes de escribir (contrapresión de logs).
    pub pendientes: Pendientes,
}

impl ClienteRemoto {
    pub fn nuevo(
        id: u32,
        version: u32,
        pid: u32,
        tx: mpsc::UnboundedSender<MensajeServidor>,
        pendientes: Pendientes,
    ) -> Self {
        Self {
            id,
            version,
            pid,
            adjunto_a: HashMap::new(),
            tx,
            pendientes,
        }
    }

    /// ¿Su socket admite más mensajes? Los logs solo mandan lote cuando el
    /// escritor ha vaciado lo anterior.
    pub fn admite_mas(&self) -> bool {
        self.pendientes.hay_hueco()
    }
}

/// Lanza la tarea que drena la cola de mensajes hacia el socket del cliente.
/// Al caer la cola el socket se cierra. Devuelve también el contador de
/// mensajes pendientes.
pub fn lanzar_escritura(
    escritura: OwnedWriteHalf,
) -> (mpsc::UnboundedSender<MensajeServidor>, Pendientes) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let pendientes = Pendientes::nuevos();
    let cuenta = pendientes.clone();
    tokio::spawn(async move {
        let mut salida = FramedWrite::new(escritura, protocolo::codec());
        while let Some(mensaje) = rx.recv().await {
            cuenta.incrementar();
            let escrito = match protocolo::codificar(&mensaje) {
                Ok(linea) => salida.send(linea.as_str()).await.is_ok(),
                Err(_) => false,
            };
            cuenta.decrementar();
            if !escrito {
                break;
            }
        }
    });
    (tx, pendientes)
}
