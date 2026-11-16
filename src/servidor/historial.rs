//! Captura del historial de una pestaña para el modo historial del cliente
//! (Fase 15): `CapturarHistorial` → `HistorialLote`.
//!
//! El historial vive en el servidor (el de vt100) y el cliente solo lo ve en
//! capturas temporales. La captura es el historial más la pantalla principal,
//! en líneas con sus tramos y `envuelta`; los lotes se cortan por tamaño
//! codificado, nunca por número de líneas, para no pasar el máximo del códec.
//! Plazo de 5 s (`5.2).

use std::sync::Arc;
use std::time::Duration;

use magi_nucleo::terminal::historial::{capturar, LineaHistorial};

use crate::protocolo::MensajeServidor;

use super::difusion;
use super::sesiones::PantallaCompartida;
use super::EstadoServidor;

/// Plazo de una captura (`5.2).
pub const PLAZO_CAPTURA: Duration = Duration::from_secs(5);

/// Tamaño codificado máximo de un lote: por debajo del máximo del códec
/// (4 MiB) con margen para el JSON del sobre.
const TAMANO_LOTE: usize = 3 * 1024 * 1024;

/// Responde a `CapturarHistorial` con uno o varios `HistorialLote` (solo al
/// solicitante). Si la pestaña ya no existe, contesta `Error`. Solo puede
/// haber una captura en vuelo por ventana y pestaña: una segunda petición
/// mientras la primera se está troceando se rechaza en vez de acumular dos
/// capturas de 10 000 líneas en memoria.
pub async fn manejar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    sesion_id: u32,
) {
    let pantalla = {
        let mut estado_bloqueado = estado.lock().await;
        let pantalla = match estado_bloqueado.sesiones.get(&sesion_id) {
            Some(sesion) => sesion.pantalla.clone(),
            None => {
                drop(estado_bloqueado);
                responder_error(
                    estado,
                    cliente_id,
                    peticion_id,
                    "la pestaña ya no existe; no se puede capturar su historial",
                )
                .await;
                return;
            }
        };
        if !estado_bloqueado
            .capturas_en_vuelo
            .insert((cliente_id, sesion_id))
        {
            drop(estado_bloqueado);
            responder_error(
                estado,
                cliente_id,
                peticion_id,
                "ya hay una captura del historial de esa pestaña en curso",
            )
            .await;
            return;
        }
        pantalla
    };
    let resultado = capturar_y_enviar(estado, cliente_id, peticion_id, sesion_id, pantalla).await;
    estado
        .lock()
        .await
        .capturas_en_vuelo
        .remove(&(cliente_id, sesion_id));
    if let Err(motivo) = resultado {
        responder_error(estado, cliente_id, peticion_id, &motivo).await;
    }
}

/// Captura y trocea; no manda nada más si la pestaña o la ventana dejan de
/// estar vivas por el camino.
async fn capturar_y_enviar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    sesion_id: u32,
    pantalla: PantallaCompartida,
) -> Result<(), String> {
    let captura = capturar_con_plazo(pantalla).await?;
    let total = captura.lineas.len() as u32;
    let lotes = trocear(&captura.lineas);
    let numero_lotes = lotes.len();
    for (indice, lineas) in lotes.into_iter().enumerate() {
        let ultimo = indice + 1 == numero_lotes;
        let mensaje = MensajeServidor::HistorialLote {
            peticion_id,
            sesion_id,
            lote: indice as u32,
            ultimo,
            total,
            lineas,
        };
        let enviado = {
            let estado_bloqueado = estado.lock().await;
            // La pestaña pudo cerrarse mientras se troceaba: no se sigue
            // enviando una captura que el cliente ya no podrá aplicar.
            if !estado_bloqueado.sesiones.contains_key(&sesion_id) {
                false
            } else {
                match estado_bloqueado.clientes.get(&cliente_id) {
                    Some(cliente) => {
                        difusion::enviar(cliente, mensaje);
                        true
                    }
                    None => false,
                }
            }
        };
        if !enviado {
            break;
        }
    }
    Ok(())
}

/// Captura en un hilo aparte (la petición no bloquea el bucle del servidor) con
/// el plazo de 5 s. La vista del historial se deja al final.
async fn capturar_con_plazo(
    pantalla: PantallaCompartida,
) -> Result<magi_nucleo::terminal::historial::Captura, String> {
    let tarea = tokio::task::spawn_blocking(move || {
        let Ok(mut pantalla) = pantalla.lock() else {
            return None;
        };
        Some(capturar(&mut pantalla))
    });
    match tokio::time::timeout(PLAZO_CAPTURA, tarea).await {
        Ok(Ok(Some(captura))) => Ok(captura),
        Ok(Ok(None)) => Err("no se pudo leer la pantalla de la pestaña".to_string()),
        Ok(Err(error)) => Err(format!("la captura del historial falló: {error}")),
        Err(_) => Err("se agotó el plazo de la captura del historial".to_string()),
    }
}

/// Trocea las líneas por tamaño codificado (una línea muy grande va sola).
fn trocear(lineas: &[LineaHistorial]) -> Vec<Vec<LineaHistorial>> {
    let mut lotes = Vec::new();
    let mut actual: Vec<LineaHistorial> = Vec::new();
    let mut bytes = 0usize;
    for linea in lineas {
        let tamano = serde_json::to_string(linea)
            .map(|texto| texto.len())
            .unwrap_or(TAMANO_LOTE + 1)
            + 2;
        if !actual.is_empty() && bytes + tamano > TAMANO_LOTE {
            lotes.push(std::mem::take(&mut actual));
            bytes = 0;
        }
        bytes += tamano;
        actual.push(linea.clone());
    }
    if !actual.is_empty() || lotes.is_empty() {
        lotes.push(actual);
    }
    lotes
}

async fn responder_error(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    motivo: &str,
) {
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        difusion::enviar(
            cliente,
            MensajeServidor::Error {
                mensaje: motivo.to_string(),
                peticion_id: Some(peticion_id),
            },
        );
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn una_captura_vacia_va_en_un_lote_vacio() {
        let lotes = trocear(&[]);
        assert_eq!(lotes.len(), 1);
        assert!(lotes[0].is_empty());
    }
}
