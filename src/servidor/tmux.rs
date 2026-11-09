//! tmux del servidor (Fase 15): comprobar, listar y terminar sesiones del
//! host por un canal `exec` del pool, y armar la orden con la que se abre una
//! sesión nueva en el PTY de la pestaña.
//!
//! Todo pasa por el script embebido `terminal_tmux.sh` del núcleo: el nombre
//! se valida en Rust y en el script y va siempre como argumento, nunca
//! interpolado en una orden de tmux. Lo que devuelve `listar` es entrada no
//! fiable y lo lee `magi_nucleo::terminal::tmux`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use russh::client::Handle;
use tracing::warn;

use magi_nucleo::terminal::tmux::{self, Listado, ResultadoTerminar, Suborden};

use crate::conexion::cliente::Cliente;
use crate::modelo::Host;
use crate::protocolo::{InfoSesionTmux, MensajeServidor, ResultadoTmux};

use super::conexiones::{self, PoliticaConexion, UsoCanal};
use super::EstadoServidor;

/// Plazo de las operaciones de tmux que hablan con el host (`5.2: 10 s).
pub const PLAZO: Duration = Duration::from_secs(10);

/// Tope de lo que se guarda de la salida del script (entrada no fiable).
const TOPE_SALIDA: usize = 4 * 1024 * 1024;

/// La orden que se escribe en el PTY de una pestaña para abrir o enganchar su
/// sesión de tmux: `exec sh -c <script> magi-tmux abrir <nombre>`. Devuelve
/// `None` si el nombre no es válido (nunca se interpola).
#[must_use]
pub fn orden_abrir(nombre: &str) -> Option<String> {
    let argumentos = tmux::orden(Suborden::Abrir, Some(nombre))?;
    let linea = argumentos
        .iter()
        .map(|argumento| shell_escape::unix::escape(argumento.as_str().into()).to_string())
        .collect::<Vec<String>>()
        .join(" ");
    Some(format!("exec {linea}\n"))
}

/// Ejecuta el script por un canal `exec` de la conexión y devuelve su salida
/// (stdout y stderr juntos) y el código. El canal se cierra siempre.
pub async fn ejecutar_en_handle(
    handle: &Arc<Handle<Cliente>>,
    argumentos: &[String],
    plazo: Duration,
) -> Result<(String, i32), String> {
    let limite = tokio::time::Instant::now() + plazo;
    let comando = argumentos
        .iter()
        .map(|argumento| shell_escape::unix::escape(argumento.as_str().into()).to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let apertura = handle.clone();
    let mut canal =
        conexiones::abrir_con_plazo(PLAZO, async move { apertura.channel_open_session().await })
            .await?;
    let mut salida = Vec::new();
    let mut codigo = None;
    let trabajo = async {
        canal
            .exec(true, comando.into_bytes())
            .await
            .map_err(|error| error.to_string())?;
        let _ = canal.eof().await;
        loop {
            match canal.wait().await {
                Some(russh::ChannelMsg::Data { data })
                | Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                    if salida.len() < TOPE_SALIDA {
                        salida.extend_from_slice(&data);
                    }
                }
                Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                    codigo = Some(exit_status as i32);
                }
                Some(russh::ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        Ok::<(), String>(())
    };
    let resultado = tokio::time::timeout_at(limite, trabajo).await;
    let _ = canal.close().await;
    match resultado {
        Err(_) => Err("se agotó el plazo de tmux".to_string()),
        Ok(Err(motivo)) => Err(motivo),
        Ok(Ok(())) => Ok((
            String::from_utf8_lossy(&salida).to_string(),
            codigo.unwrap_or(-1),
        )),
    }
}

/// Lee la salida de `comprobar`: la versión de tmux (si la hay) y si la
/// sesión existe.
#[must_use]
pub fn leer_comprobar(salida: &str) -> (Option<String>, bool) {
    let mut version = None;
    let mut existe = false;
    for linea in salida.lines() {
        let linea = linea.trim_end_matches('\r');
        if let Some(valor) = linea.strip_prefix("MAGI_TMUX=") {
            let valor = valor.trim();
            if !valor.is_empty() && valor != "no" {
                version = Some(valor.chars().take(64).collect::<String>());
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_SESION=") {
            existe = valor.trim() == "existe";
        }
    }
    (version, existe)
}

/// `comprobar <nombre>`: hay tmux y la sesión existe.
pub async fn comprobar(
    handle: &Arc<Handle<Cliente>>,
    nombre: &str,
) -> Result<(Option<String>, bool), String> {
    let Some(argumentos) = tmux::orden(Suborden::Comprobar, Some(nombre)) else {
        return Err("nombre de tmux no válido".to_string());
    };
    let (salida, _codigo) = ejecutar_en_handle(handle, &argumentos, PLAZO).await?;
    Ok(leer_comprobar(&salida))
}

/// `listar`: las sesiones de tmux del host, ya validadas.
pub async fn listar(handle: &Arc<Handle<Cliente>>) -> Result<Listado, String> {
    let Some(argumentos) = tmux::orden(Suborden::Listar, None) else {
        return Err("orden de tmux no válida".to_string());
    };
    let (salida, _codigo) = ejecutar_en_handle(handle, &argumentos, PLAZO).await?;
    Ok(tmux::leer_listado(&salida))
}

/// El nombre de este equipo para los nombres de tmux: el de la sincronización
/// (fila `es_este = 1` de EQUIPOS) o, sin sincronización, el del sistema.
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

/// `ListarTmux`: lista las sesiones del host por el pool (abriendo la
/// conexión con los diálogos de siempre si no hay ninguna viva).
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
    let resultado = listar(&tomada.handle).await;
    tomada.soltar().await;
    match resultado {
        Ok(listado) => {
            let sesiones: Vec<InfoSesionTmux> = listado.sesiones;
            let sin_tmux = listado
                .tmux
                .is_none()
                .then(|| format!("sin tmux en «{}»", host.nombre));
            responder(
                estado,
                cliente_id,
                MensajeServidor::SesionesTmux {
                    peticion_id,
                    host_id,
                    sesiones,
                    sin_tmux,
                },
            )
            .await;
        }
        Err(motivo) => responder_error(estado, cliente_id, peticion_id, &motivo).await,
    }
}

/// `TerminarTmux`: termina una sesión del host (el cliente ya confirmó).
pub async fn manejar_terminar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
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
    let resultado = match tmux::orden(Suborden::Terminar, Some(nombre)) {
        Some(argumentos) => ejecutar_en_handle(&tomada.handle, &argumentos, PLAZO)
            .await
            .map(|(salida, _)| tmux::leer_terminar(&salida)),
        None => Err("nombre de tmux no válido".to_string()),
    };
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
            responder(
                estado,
                cliente_id,
                MensajeServidor::TmuxTerminada {
                    peticion_id,
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
