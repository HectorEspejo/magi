//! Mecánica de los túneles local y dinámico: aceptar conexiones en el
//! loopback y llevar cada una a un canal direct-tcpip del host.
//!
//! La comparten el servidor de sesiones del escritorio y el gestor del móvil.

use std::sync::Arc;
use std::time::Duration;

use russh::client::Handle;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch};
use tracing::warn;

use crate::conexion::abrir_con_plazo;
use crate::conexion::cliente::Cliente;
use crate::conexion::reenvios::{copiar_hasta_que_paren, Contadores};
use crate::tuneles::socks5;

/// Plazo de las operaciones que hablan con el host.
pub const PLAZO: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------- servir

/// Acepta conexiones locales y abre un canal `direct-tcpip` por cada una hacia
/// el destino. La señal de parada cierra el listener.
pub fn servir_local(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    destino: String,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) -> oneshot::Sender<()> {
    let (parada, senal) = oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            _ = aceptar_local(listener, handle, destino, contadores, aviso) => {}
            _ = senal => {}
        }
    });
    parada
}

async fn aceptar_local(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    destino: String,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) {
    let Some((host_destino, puerto_destino)) = crate::modelo::partir_direccion_puerto(&destino)
    else {
        return;
    };
    loop {
        let Ok((flujo, origen)) = listener.accept().await else {
            return;
        };
        let handle = handle.clone();
        let destino = host_destino.clone();
        let contadores = contadores.clone();
        let mut aviso = aviso.clone();
        tokio::spawn(async move {
            let _ = flujo.set_nodelay(true);
            let apertura = handle.clone();
            let (hacia, ip_origen) = (destino.clone(), origen.ip().to_string());
            let canal = abrir_con_plazo(PLAZO, async move {
                apertura
                    .channel_open_direct_tcpip(
                        hacia,
                        u32::from(puerto_destino),
                        ip_origen,
                        u32::from(origen.port()),
                    )
                    .await
            })
            .await;
            let canal = match canal {
                Ok(canal) => canal,
                Err(error) => {
                    // El host no quiere abrir ese destino: cae solo esta
                    // conexión, el túnel sigue vivo.
                    contadores.anotar_error(format!("el host rechazó {destino}: {error}"));
                    warn!(destino = %destino, "el host rechazó el canal: {error}");
                    return;
                }
            };
            contadores.abrir();
            let mut flujo = flujo;
            let mut canal = canal.into_stream();
            copiar_hasta_que_paren(&mut flujo, &mut canal, &contadores, &mut aviso).await;
            contadores.cerrar();
        });
    }
}

/// Igual que el local, pero hablando SOCKS5 con quien se conecta.
pub fn servir_socks(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) -> oneshot::Sender<()> {
    let (parada, senal) = oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            _ = aceptar_socks(listener, handle, contadores, aviso) => {}
            _ = senal => {}
        }
    });
    parada
}

async fn aceptar_socks(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) {
    loop {
        let Ok((mut flujo, origen)) = listener.accept().await else {
            return;
        };
        let handle = handle.clone();
        let contadores = contadores.clone();
        let mut aviso = aviso.clone();
        tokio::spawn(async move {
            let _ = flujo.set_nodelay(true);
            let (destino, puerto) = match socks5::negociar(&mut flujo).await {
                socks5::Peticion::Conectar(destino, puerto) => (destino, u32::from(puerto)),
                socks5::Peticion::NoSoportada | socks5::Peticion::Invalida => return,
            };
            let apertura = handle.clone();
            let (hacia, ip_origen) = (destino.clone(), origen.ip().to_string());
            let canal = abrir_con_plazo(PLAZO, async move {
                apertura
                    .channel_open_direct_tcpip(hacia, puerto, ip_origen, u32::from(origen.port()))
                    .await
            })
            .await;
            let canal = match canal {
                Ok(canal) => canal,
                Err(error) => {
                    contadores.anotar_error(format!("el host rechazó {destino}:{puerto}"));
                    let _ = socks5::responder(&mut flujo, socks5::codigo_rechazo()).await;
                    warn!(destino = %destino, "el host rechazó el canal SOCKS: {error}");
                    return;
                }
            };
            if socks5::responder(&mut flujo, socks5::OK).await.is_err() {
                return;
            }
            contadores.abrir();
            let mut canal = canal.into_stream();
            copiar_hasta_que_paren(&mut flujo, &mut canal, &contadores, &mut aviso).await;
            contadores.cerrar();
        });
    }
}
