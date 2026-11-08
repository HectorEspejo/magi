//! Transporte del cliente RDP: el flujo que llega del servidor de sesiones no
//! es Sync (es un Box<dyn AsyncRead + AsyncWrite + Send>), pero varias piezas de
//! IronRDP piden Sync durante el saludo (connect_begin). Se envuelve en un
//! Mutex, que es Sync cuando el flujo es Send, y se delegan las lecturas y
//! escrituras con el cerrojo tomado solo durante cada poll.
//!
//! Además, el servidor puede mandar bytes del siguiente PDU pegados al último
//! del saludo X.224; se guardan y se entregan antes que los del TLS.

use std::io;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::client::TlsStream;

use crate::escritorio::Flujo;

/// Flujo RDP equivalente a Flujo pero Sync y Unpin.
pub(crate) struct FlujoUnido {
    interior: Mutex<Flujo>,
}

impl FlujoUnido {
    pub(crate) fn nuevo(flujo: Flujo) -> Self {
        Self {
            interior: Mutex::new(flujo),
        }
    }

    fn cerrojo(&self) -> std::sync::MutexGuard<'_, Flujo> {
        // Un pánico de otra tarea no puede dejarnos sin flujo: se recupera el
        // interior envenenado en vez de entrar en pánico.
        self.interior
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
    }
}

impl AsyncRead for FlujoUnido {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut flujo = self.get_mut().cerrojo();
        Pin::new(&mut *flujo).poll_read(cx, buf)
    }
}

impl AsyncWrite for FlujoUnido {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut flujo = self.get_mut().cerrojo();
        Pin::new(&mut *flujo).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut flujo = self.get_mut().cerrojo();
        Pin::new(&mut *flujo).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut flujo = self.get_mut().cerrojo();
        Pin::new(&mut *flujo).poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.cerrojo().is_write_vectored()
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let mut flujo = self.get_mut().cerrojo();
        Pin::new(&mut *flujo).poll_write_vectored(cx, bufs)
    }
}

/// Lo que sobró del saludo X.224, delante del flujo.
///
/// Se entrega antes que los bytes del flujo para no perder el principio del
/// saludo TLS cuando el servidor lo manda pegado a la confirmación de seguridad.
pub(crate) struct FlujoConSobrante {
    sobrante: Vec<u8>,
    posicion: usize,
    flujo: FlujoUnido,
}

impl FlujoConSobrante {
    pub(crate) fn nuevo(flujo: FlujoUnido, sobrante: Vec<u8>) -> Self {
        Self {
            sobrante,
            posicion: 0,
            flujo,
        }
    }
}

impl AsyncRead for FlujoConSobrante {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let pendiente = self.sobrante.len().saturating_sub(self.posicion);
        if pendiente > 0 {
            let cuantos = pendiente.min(buf.remaining());
            let desde = self.posicion;
            let hasta = desde + cuantos;
            buf.put_slice(&self.sobrante[desde..hasta]);
            self.posicion = hasta;
            if self.posicion == self.sobrante.len() {
                self.sobrante.clear();
                self.posicion = 0;
            }
            return Poll::Ready(Ok(()));
        }

        Pin::new(&mut self.flujo).poll_read(cx, buf)
    }
}

impl AsyncWrite for FlujoConSobrante {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.flujo).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.flujo).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.flujo).poll_shutdown(cx)
    }
}

/// Flujo completo de una sesión RDP, ya con TLS.
pub(crate) type FlujoFramed = ironrdp_tokio::MovableTokioFramed<TlsStream<FlujoConSobrante>>;
