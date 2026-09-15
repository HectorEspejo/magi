//! Envío de mensajes a los clientes conectados. Las pantallas y datos solo
//! van a clientes adjuntos; los estados y la lista de sesiones se difunden.

use std::collections::HashMap;

use crate::protocolo::MensajeServidor;

use super::cliente_remoto::ClienteRemoto;

/// Envía un mensaje a un cliente concreto; si su cola ya no existe, se ignora.
pub fn enviar(cliente: &ClienteRemoto, mensaje: MensajeServidor) {
    let _ = cliente.tx.send(mensaje);
}

/// Envía el mismo mensaje a todos los clientes conectados.
pub fn difundir(clientes: &HashMap<u32, ClienteRemoto>, mensaje: MensajeServidor) {
    for cliente in clientes.values() {
        let _ = cliente.tx.send(mensaje.clone());
    }
}

/// Envía un mensaje solo a los clientes adjuntos a una sesión.
pub fn difundir_a_adjuntos(
    clientes: &HashMap<u32, ClienteRemoto>,
    adjuntos: &[u32],
    mensaje: MensajeServidor,
) {
    for id in adjuntos {
        if let Some(cliente) = clientes.get(id) {
            let _ = cliente.tx.send(mensaje.clone());
        }
    }
}
