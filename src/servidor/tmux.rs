//! tmux del servidor (Fase 15, reescrito en la Fase 16): ejecutar el script
//! `terminal_tmux.sh` por un canal `exec` de la conexión (comprobar, crear,
//! estado, historial, terminar…), la lista de sesiones del host y terminar una.
//! La pestaña se engancha en `sesiones.rs`, con el canal con PTY.
//!
//! Todo pasa por los scripts embebidos del núcleo: el nombre, la carpeta y el
//! comando se validan en Rust y en el script y van siempre como argumentos,
//! nunca interpolados en una orden de tmux. El script grande viaja por la
//! entrada estándar del `exec` (D357): en la línea de órdenes solo van el
//! arranque mínimo y los argumentos. Lo que devuelve el script es entrada no
//! fiable y lo lee `magi_nucleo::sesiones_persistentes::lectura`.
//!
//! Ni la salida del script, ni los títulos, ni los comandos van al log o a
//! REGISTRO: solo los nombres de las sesiones y los códigos de salida.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use russh::client::Handle;
use tracing::warn;

use magi_nucleo::sesiones_persistentes::lectura::{
    self, LecturaComprobar, LecturaCrear, LecturaEstado, LecturaPreparar, ResultadoCrear,
};
use magi_nucleo::terminal::tmux::{self, OrdenTmux, ResultadoTerminar};

use crate::conexion::cliente::Cliente;
use crate::modelo::Host;
use crate::protocolo::{MensajeServidor, ResultadoTmux, ServidorTmux};

use super::conexiones::{self, PoliticaConexion, UsoCanal};
use super::EstadoServidor;

/// Plazo para abrir el canal `exec` (el de la orden lo da `OrdenTmux::plazo`).
const PLAZO_CANAL: Duration = Duration::from_secs(10);

/// Plazo para cerrar el canal `exec` (si el host no contesta, no se espera).
const PLAZO_CIERRE_CANAL: Duration = Duration::from_secs(2);

/// Tope de lo que se guarda de la salida de un script (entrada no fiable).
const TOPE_SALIDA: usize = 4 * 1024 * 1024;

/// Tope de lo que se guarda de la salida de `historial`: el que acepta el
/// núcleo. Se conserva el final (lo más reciente); el núcleo lo alinea después
/// a una línea entera.
const TOPE_SALIDA_HISTORIAL: usize = lectura::MAXIMO_HISTORIAL;

/// Lo que el servidor guarda de la salida de un script y cómo la recorta si se
/// pasa del tope.
#[derive(Clone, Copy)]
enum Recorte {
    /// Se queda el principio (lo normal: las salidas son cabeceras y listas).
    Cabeza(usize),
    /// Se queda el final (el historial: lo más reciente).
    Cola(usize),
}

/// Lanza el `exec` de un canal con la orden ya armada (`OrdenTmux::argumentos`):
/// la línea de órdenes y, si la orden lleva script, el script ENTERO por la
/// entrada estándar, cerrándola después. El arranque lee hasta el fin de la
/// entrada antes de ejecutar nada, así que el `eof` va siempre tras el último
/// byte. Sin plazo propio: lo pone quien llama alrededor (la apertura del canal
/// y el envío cuentan para el de la orden). Lo comparten el vigilante y las
/// órdenes de este módulo.
pub(super) async fn lanzar_exec(
    canal: &mut russh::Channel<russh::client::Msg>,
    argumentos: &[String],
) -> Result<(), String> {
    canal
        .exec(true, tmux::linea_de_exec(argumentos).into_bytes())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(entrada) = tmux::entrada_del_exec(argumentos) {
        // Un host que rechaza el `exec` (una shell restringida…) cierra el canal
        // y el envío falla antes de que llegue su `Failure`: el motivo que ve el
        // usuario es el mismo que con un `exec` rechazado.
        canal
            .data_bytes(entrada.as_bytes().to_vec())
            .await
            .map_err(|error| format!("el host no admitió ejecutar el comando de tmux ({error})"))?;
    }
    let _ = canal.eof().await;
    Ok(())
}

/// Ejecuta el script por un canal `exec` de la conexión y devuelve su salida
/// (stdout y stderr juntos) y el código. El canal se cierra siempre.
async fn ejecutar_en_handle(
    handle: &Arc<Handle<Cliente>>,
    argumentos: &[String],
    plazo: Duration,
    recorte: Recorte,
) -> Result<(Vec<u8>, i32), String> {
    let limite = tokio::time::Instant::now() + plazo;
    let apertura = handle.clone();
    let mut canal =
        conexiones::abrir_con_plazo(
            PLAZO_CANAL,
            async move { apertura.channel_open_session().await },
        )
        .await?;
    let mut salida = Vec::new();
    let mut codigo = None;
    let trabajo = async {
        lanzar_exec(&mut canal, argumentos).await?;
        loop {
            match canal.wait().await {
                Some(russh::ChannelMsg::Data { data })
                | Some(russh::ChannelMsg::ExtendedData { data, .. }) => match recorte {
                    Recorte::Cabeza(tope) => {
                        if salida.len() < tope {
                            let sitio = tope - salida.len();
                            salida.extend_from_slice(&data[..data.len().min(sitio)]);
                        }
                    }
                    Recorte::Cola(tope) => {
                        salida.extend_from_slice(&data);
                        // Se recorta con holgura (el doble) para no mover los
                        // bytes a cada bloque.
                        if salida.len() > tope * 2 {
                            salida.drain(..salida.len() - tope);
                        }
                    }
                },
                Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                    codigo = Some(exit_status as i32);
                }
                // El host rechazó el `exec` (un comando demasiado largo para su
                // servidor, una shell restringida…): sin esto la salida vacía
                // se leería como «el host no tiene tmux».
                Some(russh::ChannelMsg::Failure) => {
                    return Err("el host no admitió ejecutar el comando de tmux".to_string());
                }
                Some(russh::ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        Ok::<(), String>(())
    };
    let resultado = tokio::time::timeout_at(limite, trabajo).await;
    // El canal se cierra pase lo que pase (en una conexión compartida, uno
    // abierto de más agotaría `MaxSessions`), pero sin esperar sin plazo.
    let _ = tokio::time::timeout(PLAZO_CIERRE_CANAL, canal.close()).await;
    match resultado {
        Err(_) => Err("se agotó el plazo de tmux".to_string()),
        Ok(Err(motivo)) => Err(motivo),
        Ok(Ok(())) => {
            if let Recorte::Cola(tope) = recorte {
                if salida.len() > tope {
                    salida.drain(..salida.len() - tope);
                }
            }
            Ok((salida, codigo.unwrap_or(-1)))
        }
    }
}

/// Ejecuta una orden del script por un canal `exec` con el plazo de T32.
/// `Err` si la orden no es válida, no se puede ejecutar por `exec` (la de
/// `enganchar` va en el PTY) o el host no contesta a tiempo.
pub async fn ejecutar_orden(
    handle: &Arc<Handle<Cliente>>,
    orden: &OrdenTmux<'_>,
) -> Result<(Vec<u8>, i32), String> {
    let argumentos = orden
        .argumentos()
        .ok_or_else(|| "orden de tmux no válida".to_string())?;
    let plazo = orden
        .plazo()
        .ok_or_else(|| "esa orden de tmux no se ejecuta por exec".to_string())?;
    let recorte = match orden {
        OrdenTmux::Historial { .. } => Recorte::Cola(TOPE_SALIDA_HISTORIAL),
        _ => Recorte::Cabeza(TOPE_SALIDA),
    };
    ejecutar_en_handle(handle, &argumentos, plazo, recorte).await
}

// ----------------------------------------------------------- las órdenes

/// `comprobar <servidor> <nombre>`: versión de tmux y si la sesión existe.
pub async fn comprobar(
    handle: &Arc<Handle<Cliente>>,
    servidor: ServidorTmux,
    nombre: &str,
) -> Result<LecturaComprobar, String> {
    let (salida, codigo) =
        ejecutar_orden(handle, &OrdenTmux::Comprobar { servidor, nombre }).await?;
    // `comprobar` sale con 0 cuando ha dicho todo lo que sabe: con otro código
    // (o sin decir cómo salió) la salida puede ser una cabecera sin lo demás, y
    // leerla daría «no hay tmux» o «la sesión no existe» sin saberlo.
    if codigo != 0 {
        return Err(format!(
            "el host no devolvió la comprobación de tmux completa (código {codigo})"
        ));
    }
    Ok(lectura::leer_comprobar(&String::from_utf8_lossy(&salida)))
}

/// `preparar <servidor> <nombre>`: la primera mitad de engancharse. `Err` solo
/// si no se pudo hablar con el host; lo que el script dice (la sesión ya no
/// existe, sin tmux, tmux antiguo) va en el resultado, con el código de salida,
/// y una salida sin código de salida (un canal cortado) es `Error`, nunca «no
/// existe».
pub async fn preparar(
    handle: &Arc<Handle<Cliente>>,
    servidor: ServidorTmux,
    nombre: &str,
) -> Result<LecturaPreparar, String> {
    let (salida, codigo) =
        ejecutar_orden(handle, &OrdenTmux::Preparar { servidor, nombre }).await?;
    let codigo = (codigo >= 0).then_some(codigo);
    Ok(lectura::leer_preparar(
        &String::from_utf8_lossy(&salida),
        codigo,
    ))
}

/// `estado`: las sesiones de los dos servidores. `Err` si el host no contesta,
/// si el script no terminó bien o si lo que devuelve no es su salida.
///
/// `estado` sale con 0 cuando ha dicho todo lo que sabe. Con otro código (le
/// falta `awk` al host, se cortó a mitad) o sin decir cómo salió, lo escrito
/// puede ser la versión de tmux y ninguna sesión: la reconexión cerraría una
/// pestaña cuya sesión sigue viva. Sin una salida completa no se sabe cómo
/// están (como en el vigilante).
pub async fn estado(handle: &Arc<Handle<Cliente>>) -> Result<LecturaEstado, String> {
    let (salida, codigo) = ejecutar_orden(handle, &OrdenTmux::Estado).await?;
    if codigo != 0 {
        return Err(format!(
            "el host no devolvió el estado de tmux completo (código {codigo})"
        ));
    }
    lectura::leer_estado_bytes(&salida)
        .ok_or_else(|| "el host no devolvió el estado de tmux".to_string())
}

/// Lo que dice `crear`, más lo que tmux o el shell escribieron además de las
/// marcas del script (un posible motivo del error, ya saneado).
pub struct CreadaEnElHost {
    pub lectura: LecturaCrear,
    pub detalle: Option<String>,
}

/// `crear`: la sesión en el tmux propio, sin engancharse.
pub async fn crear(
    handle: &Arc<Handle<Cliente>>,
    orden: &OrdenTmux<'_>,
) -> Result<CreadaEnElHost, String> {
    let (salida, codigo) = ejecutar_orden(handle, orden).await?;
    let texto = String::from_utf8_lossy(&salida);
    let lectura = lectura::leer_crear(&texto, Some(codigo));
    let detalle = if lectura.resultado == ResultadoCrear::Error {
        detalle_de_error(&texto)
    } else {
        None
    };
    Ok(CreadaEnElHost { lectura, detalle })
}

/// La primera línea de la salida que no es una marca del script (lo que tmux
/// escribió por su error), sin caracteres de control y de hasta 200
/// caracteres. Va al mensaje de error que ve el usuario, nunca al log.
fn detalle_de_error(salida: &str) -> Option<String> {
    salida
        .lines()
        .map(str::trim)
        .find(|linea| !linea.is_empty() && !linea.starts_with("MAGI_"))
        .map(|linea| {
            linea
                .chars()
                .filter(|caracter| !caracter.is_control())
                .take(200)
                .collect::<String>()
        })
        .filter(|linea| !linea.is_empty())
}

/// `historial`: el historial con colores, ya acotado a 8 MiB.
pub async fn historial(
    handle: &Arc<Handle<Cliente>>,
    servidor: ServidorTmux,
    nombre: &str,
    lineas: u32,
) -> Result<String, String> {
    let (salida, codigo) = ejecutar_orden(
        handle,
        &OrdenTmux::Historial {
            servidor,
            nombre,
            lineas,
        },
    )
    .await?;
    match codigo {
        0 => {
            let texto = String::from_utf8_lossy(&salida);
            Ok(lectura::acotar_historial(&texto).to_string())
        }
        tmux::SALIDA_NO_EXISTE => Err(format!("la sesión «{nombre}» ya no existe")),
        tmux::SALIDA_SIN_TMUX => Err("el host ya no tiene tmux".to_string()),
        tmux::SALIDA_ANTIGUO => Err("tmux demasiado antiguo (3.0 o posterior)".to_string()),
        otro => Err(format!("tmux no pudo leer el historial (código {otro})")),
    }
}

/// `vista`: lo que muestra ahora la sesión (las últimas `lineas` visibles, en
/// texto plano), ya acotado y sin caracteres de control salvo el salto de
/// línea. Lo usa una pestaña soltada para conservar su última pantalla. El
/// contenido nunca va al log: los errores solo dicen el motivo.
pub async fn vista(
    handle: &Arc<Handle<Cliente>>,
    servidor: ServidorTmux,
    nombre: &str,
    lineas: u32,
) -> Result<String, String> {
    let (salida, codigo) = ejecutar_orden(
        handle,
        &OrdenTmux::Vista {
            servidor,
            nombre,
            lineas,
        },
    )
    .await?;
    match codigo {
        0 => Ok(lectura::acotar_vista(&String::from_utf8_lossy(&salida))),
        tmux::SALIDA_NO_EXISTE => Err(format!("la sesión «{nombre}» ya no existe")),
        tmux::SALIDA_SIN_TMUX => Err("el host ya no tiene tmux".to_string()),
        tmux::SALIDA_ANTIGUO => Err("tmux demasiado antiguo (3.0 o posterior)".to_string()),
        otro => Err(format!("tmux no pudo leer la pantalla (código {otro})")),
    }
}

/// La etiqueta de este equipo para `@magi_equipo` (la misma que ponen los
/// nombres automáticos).
pub async fn etiqueta_del_equipo(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) -> String {
    tmux::etiqueta_equipo(&nombre_equipo(estado).await)
}

/// El nombre de este equipo: el de la sincronización (fila `es_este = 1` de
/// EQUIPOS) o, sin sincronización, el del sistema.
pub async fn nombre_equipo(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) -> String {
    let lectura = estado.lock().await.lectura.clone();
    lectura
        .lock()
        .ok()
        .and_then(|almacen| {
            almacen
                .conexion()
                .query_row("SELECT nombre FROM EQUIPOS WHERE es_este = 1", [], |fila| {
                    fila.get::<_, String>(0)
                })
                .ok()
        })
        .filter(|nombre| !nombre.trim().is_empty())
        .unwrap_or_else(crate::config::nombre_actual)
}

// ---------------------------------------------------------------- mensajes

/// `ListarTmux`: las sesiones de los dos servidores del host, leídas con
/// `estado` por el pool (abriendo la conexión con los diálogos de siempre si
/// no hay ninguna viva: lo pidió el usuario).
pub async fn manejar_listar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
) {
    let Some((host, todos_los_hosts, rutas, id_solicitud)) = datos_de_host(estado, host_id).await
    else {
        responder_error(estado, cliente_id, peticion_id, "el host ya no existe").await;
        return;
    };
    let tomada = conexiones::conexion_para_canal(
        estado,
        &host,
        &todos_los_hosts,
        &rutas,
        PoliticaConexion::Interactiva {
            solicitante: cliente_id,
            id_solicitud,
        },
        UsoCanal::Subsistema,
    )
    .await;
    let tomada = match tomada {
        Ok(tomada) => tomada,
        Err(motivo) => {
            responder_error(estado, cliente_id, peticion_id, &motivo).await;
            return;
        }
    };
    let resultado = self::estado(&tomada.handle).await;
    tomada.soltar().await;
    match resultado {
        Ok(lectura) => {
            let sin_tmux = lectura
                .tmux
                .is_none()
                .then(|| format!("sin tmux en «{}»", host.nombre));
            responder(
                estado,
                cliente_id,
                MensajeServidor::SesionesTmux {
                    peticion_id,
                    host_id,
                    sesiones: lectura.sesiones,
                    sin_tmux,
                },
            )
            .await;
        }
        Err(motivo) => responder_error(estado, cliente_id, peticion_id, &motivo).await,
    }
}

/// `TerminarTmux`: termina una sesión del host (el cliente ya confirmó). El
/// script siempre apunta a `=<nombre>`: terminar «magi-a-1» con «magi-a-10»
/// viva da `NoExiste` y no toca la otra. El efecto se anota cuando se ha
/// producido y la respuesta sale después de que el escritor lo confirme.
pub async fn manejar_terminar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    servidor: ServidorTmux,
    nombre: &str,
) {
    if !tmux::nombre_valido(nombre) {
        responder_error(
            estado,
            cliente_id,
            peticion_id,
            "nombre de tmux no admitido",
        )
        .await;
        return;
    }
    let Some((host, todos_los_hosts, rutas, id_solicitud)) = datos_de_host(estado, host_id).await
    else {
        responder_error(estado, cliente_id, peticion_id, "el host ya no existe").await;
        return;
    };
    let tomada = conexiones::conexion_para_canal(
        estado,
        &host,
        &todos_los_hosts,
        &rutas,
        PoliticaConexion::Interactiva {
            solicitante: cliente_id,
            id_solicitud,
        },
        UsoCanal::Subsistema,
    )
    .await;
    let tomada = match tomada {
        Ok(tomada) => tomada,
        Err(motivo) => {
            responder_error(estado, cliente_id, peticion_id, &motivo).await;
            return;
        }
    };
    let resultado = ejecutar_orden(&tomada.handle, &OrdenTmux::Terminar { servidor, nombre })
        .await
        .map(|(salida, _)| lectura::leer_terminar(&String::from_utf8_lossy(&salida)));
    tomada.soltar().await;
    match resultado {
        Ok(ResultadoTerminar::Ok) => {
            let bd = estado.lock().await.bd.clone();
            let _ = bd.send(super::OrdenBd::Anotar {
                tipo: crate::registro::TMUX_TERMINADA.to_string(),
                host_id: Some(host_id),
                identidad_id: None,
                detalle: format!("«{}» · {nombre}", host.nombre),
                resultado: crate::modelo::ResultadoRegistro::Ok,
            });
            // La respuesta sale con la fila ya escrita (T21).
            super::confirmar_escritura(estado).await;
            responder(
                estado,
                cliente_id,
                MensajeServidor::TmuxTerminada {
                    peticion_id,
                    servidor,
                    resultado: ResultadoTmux::Ok,
                },
            )
            .await;
        }
        Ok(ResultadoTerminar::NoExiste) => {
            responder(
                estado,
                cliente_id,
                MensajeServidor::TmuxTerminada {
                    peticion_id,
                    servidor,
                    resultado: ResultadoTmux::NoExiste,
                },
            )
            .await;
        }
        Err(motivo) => responder_error(estado, cliente_id, peticion_id, &motivo).await,
    }
}

/// Host, inventario, rutas y un id de solicitud para los diálogos de conexión.
async fn datos_de_host(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<(Host, HashMap<i64, Host>, crate::config::Rutas, u32)> {
    let (lectura, rutas) = {
        let estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            return None;
        }
        (
            estado_bloqueado.lectura.clone(),
            estado_bloqueado.rutas.clone(),
        )
    };
    let (host, todos_los_hosts) = {
        let lectura = lectura.lock().ok()?;
        let host = lectura.obtener_host(host_id).ok()?;
        let todos = lectura
            .listar_hosts()
            .unwrap_or_default()
            .into_iter()
            .map(|host| (host.id, host))
            .collect();
        (host, todos)
    };
    let id_solicitud = {
        let mut estado_bloqueado = estado.lock().await;
        let id = estado_bloqueado.siguiente_sesion_id;
        estado_bloqueado.siguiente_sesion_id += 1;
        id
    };
    Some((host, todos_los_hosts, rutas, id_solicitud))
}

async fn responder(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    mensaje: MensajeServidor,
) {
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        super::difusion::enviar(cliente, mensaje);
    }
}

async fn responder_error(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    motivo: &str,
) {
    warn!(cliente = cliente_id, "tmux: {motivo}");
    responder(
        estado,
        cliente_id,
        MensajeServidor::Error {
            mensaje: motivo.to_string(),
            peticion_id: Some(peticion_id),
        },
    )
    .await;
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_detalle_del_error_salta_las_marcas_y_limpia_los_controles() {
        let salida = "MAGI_TMUX=tmux 3.4\n\ncreate session failed: x\u{7}y\nMAGI_RESULTADO=error\n";
        assert_eq!(
            detalle_de_error(salida).as_deref(),
            Some("create session failed: xy")
        );
        assert_eq!(detalle_de_error("MAGI_RESULTADO=error\n"), None);
        assert_eq!(detalle_de_error(""), None);
        let larga = format!("{}\n", "a".repeat(500));
        assert_eq!(detalle_de_error(&larga).unwrap().chars().count(), 200);
    }
}
