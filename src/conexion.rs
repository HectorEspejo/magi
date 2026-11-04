//! Puente del escritorio con la conexión SSH del núcleo: traduce los eventos
//! de la conexión al evento de la TUI y aporta el usuario local del proceso.

use tokio::sync::mpsc;

pub use magi_nucleo::conexion::*;

use crate::app::Evento;

/// Nombre del usuario local, para las conexiones sin usuario en la ficha.
pub fn usuario_local() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string())
}

/// Lanza la tarea tokio de la sesión y devuelve el canal de comandos.
pub fn lanzar(
    runtime: &tokio::runtime::Runtime,
    plan: PlanConexion,
    tx: mpsc::UnboundedSender<Evento>,
) -> mpsc::UnboundedSender<ComandoConexion> {
    let (tx_eventos, mut rx_eventos) = mpsc::unbounded_channel();
    let comandos = magi_nucleo::conexion::lanzar(runtime, plan, tx_eventos);
    runtime.spawn(async move {
        while let Some(evento) = rx_eventos.recv().await {
            if tx.send(Evento::Conexion(evento)).is_err() {
                break;
            }
        }
    });
    comandos
}
