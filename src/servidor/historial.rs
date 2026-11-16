//! Captura del historial de una pestaña para el modo historial del cliente
//! (Fase 15): `CapturarHistorial` → `HistorialLote`.
//!
//! El historial vive en el servidor (el de vt100) y el cliente solo lo ve en
//! capturas temporales. La captura es el historial más la pantalla principal,
//! en líneas con sus tramos y `envuelta`; los lotes se cortan por tamaño
//! codificado, nunca por número de líneas, para no pasar el máximo del códec.
//! Plazo de 5 s (`5.2).
//!
//! Desde la Fase 16, en una pestaña enganchada a una sesión de tmux el
//! historial que importa es el de tmux, no el de vt100 (tmux usa la pantalla
//! alternativa): se lee con la suborden `historial` del script (plazo de 5 s),
//! se pasa por un emulador para sacar sus tramos de estilo y se manda con el
//! mismo troceado y los mismos topes (8 MiB y 64 tramos por fila).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use magi_nucleo::terminal::emulador::PantallaEmulada;
use magi_nucleo::terminal::historial::{capturar, Captura, LineaHistorial};
use magi_nucleo::terminal::tmux::MAXIMO_LINEAS_CAPTURA;

use crate::protocolo::{MensajeServidor, ServidorTmux};

use super::conexiones::{self, PoliticaConexion, UsoCanal};
use super::difusion;
use super::sesiones::{tope_historial, PantallaCompartida};
use super::EstadoServidor;

/// Lo que hace falta para leer el historial de tmux de una pestaña.
struct DeTmux {
    host_id: i64,
    servidor: ServidorTmux,
    nombre: String,
    cols: u16,
    filas: u16,
    historial_lineas: u64,
    historial_mib: u64,
}

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
    let (pantalla, de_tmux) = {
        let mut estado_bloqueado = estado.lock().await;
        let datos = match estado_bloqueado.sesiones.get(&sesion_id) {
            Some(sesion) => {
                // Con tmux y la pestaña abierta, el historial es el de tmux.
                // Caída o soltada no hay conexión: vale la última pantalla.
                let de_tmux = match &sesion.tmux {
                    Some(nombre) if sesion.abierta() => Some(DeTmux {
                        host_id: sesion.host_id,
                        servidor: sesion.tmux_servidor,
                        nombre: nombre.clone(),
                        cols: sesion.tamano.cols,
                        filas: sesion.tamano.filas,
                        historial_lineas: estado_bloqueado
                            .config
                            .terminal
                            .historial_lineas_efectivo()
                            as u64,
                        historial_mib: estado_bloqueado.config.terminal.historial_mib_efectivo(),
                    }),
                    _ => None,
                };
                (sesion.pantalla.clone(), de_tmux)
            }
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
        datos
    };
    let resultado = capturar_y_enviar(
        estado,
        cliente_id,
        peticion_id,
        sesion_id,
        pantalla,
        de_tmux,
    )
    .await;
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
    de_tmux: Option<DeTmux>,
) -> Result<(), String> {
    let captura = match de_tmux {
        Some(de_tmux) => capturar_de_tmux(estado, de_tmux).await?,
        None => capturar_con_plazo(pantalla).await?,
    };
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

/// Lee el historial de tmux del host (`historial`, plazo de 5 s) por una
/// conexión viva (la del pool o la de una pestaña abierta: nunca abre una ni
/// dialoga) y lo convierte en una captura con tramos de estilo. El texto de
/// tmux (historial y pantalla, con colores, sin cortar por el ancho) pasa por
/// un emulador del tamaño de la pestaña: así las líneas largas salen envueltas
/// como en la pantalla y el modo historial busca y copia igual que en la
/// Fase 15. Ni el texto ni los títulos van al log.
async fn capturar_de_tmux(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    de_tmux: DeTmux,
) -> Result<Captura, String> {
    const ERROR: &str = "no se pudo leer el historial de tmux";
    let (host, rutas) = {
        let estado_bloqueado = estado.lock().await;
        let host = estado_bloqueado
            .lectura
            .lock()
            .ok()
            .and_then(|lectura| lectura.obtener_host(de_tmux.host_id).ok());
        (host, estado_bloqueado.rutas.clone())
    };
    let host = host.ok_or_else(|| format!("{ERROR}: el host ya no existe"))?;
    let tomada = conexiones::conexion_para_canal(
        estado,
        &host,
        &HashMap::new(),
        &rutas,
        PoliticaConexion::SoloViva,
        UsoCanal::Subsistema,
    )
    .await
    .map_err(|motivo| format!("{ERROR}: {motivo}"))?;
    let lineas = u32::try_from(de_tmux.historial_lineas)
        .unwrap_or(u32::MAX)
        .clamp(1, MAXIMO_LINEAS_CAPTURA);
    let texto =
        super::tmux::historial(&tomada.handle, de_tmux.servidor, &de_tmux.nombre, lineas).await;
    // La conexión se suelta por identidad en todas las ramas.
    tomada.soltar().await;
    let texto = texto.map_err(|motivo| format!("{ERROR}: {motivo}"))?;
    let tope = tope_historial(
        de_tmux.historial_lineas,
        de_tmux.historial_mib,
        de_tmux.cols,
    );
    let (filas, cols) = (de_tmux.filas.max(1), de_tmux.cols.max(2));
    let tarea = tokio::task::spawn_blocking(move || {
        // El historial de una captura de tmux cabe entero: filas de la pantalla
        // más las líneas pedidas (con el tope de memoria de las pestañas).
        let mut emulador = PantallaEmulada::nueva(filas, cols, tope.max(1), false);
        emulador.procesar(&con_retorno_de_carro(&texto));
        capturar(&mut emulador)
    });
    match tokio::time::timeout(PLAZO_CAPTURA, tarea).await {
        Ok(Ok(captura)) => Ok(captura),
        Ok(Err(error)) => Err(format!("{ERROR}: {error}")),
        Err(_) => Err(format!("{ERROR}: se agotó el plazo")),
    }
}

/// El texto de `capture-pane` separa las líneas con `\n`: para el emulador
/// hace falta `\r\n`. Sin el salto final, que dejaría una fila vacía de más.
fn con_retorno_de_carro(texto: &str) -> Vec<u8> {
    let texto = texto.strip_suffix('\n').unwrap_or(texto);
    texto.replace('\n', "\r\n").into_bytes()
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
