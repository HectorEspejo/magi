pub mod estado;
pub mod parser;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::{ChannelMsg, Disconnect};
use tokio::sync::{mpsc, Semaphore};
use tracing::warn;

use crate::conexion::cliente::{Cliente, Contexto};
use crate::conexion::salto::{conectar_cadena, construir_cadena};
use crate::conexion::RegistroSesiones;
use crate::modelo::{Host, ResultadoSondeo, Sondeo};

/// Script ejecutado en el host remoto. Solo depende de `sh` y coreutils;
/// sin `systemctl` los servicios quedan como `unknown`.
pub const SCRIPT: &str = include_str!("sondeo.sh");

/// Sondeos simultáneos y tiempo máximo por host (DNS, TCP, autenticación y
/// comando incluidos).
pub const CONCURRENTES: usize = 8;
pub const TIMEOUT_SEG: u64 = 5;

/// Todo lo necesario para sondear un host, calculado en el hilo de la UI.
pub struct PeticionSondeo {
    pub host: Host,
    pub todos_los_hosts: HashMap<i64, Host>,
    pub known_hosts: PathBuf,
    pub dir_ssh: PathBuf,
    pub hogar: PathBuf,
    pub usuario_local: String,
    pub servicios: Vec<String>,
}

/// Sondea un host y devuelve su `Sondeo` (nunca falla: los errores van dentro).
pub async fn sondear(peticion: &PeticionSondeo, registro: &RegistroSesiones) -> Sondeo {
    let inicio = Instant::now();
    let mut sondeo = Sondeo::vacio(peticion.host.id);
    match tokio::time::timeout(
        Duration::from_secs(TIMEOUT_SEG),
        intento(peticion, registro),
    )
    .await
    {
        Ok(Ok(metricas)) => {
            rellenar(&mut sondeo, metricas);
        }
        Ok(Err(motivo)) => {
            sondeo.resultado = ResultadoSondeo::Error;
            sondeo.error = Some(motivo);
        }
        Err(_) => {
            sondeo.resultado = ResultadoSondeo::Error;
            sondeo.error = Some(format!("timeout tras {TIMEOUT_SEG} s"));
        }
    }
    sondeo.duracion_ms = inicio.elapsed().as_millis() as i64;
    sondeo
}

/// Sondeo con sesión viva: pide `Ejecutar` al servidor (sobre la conexión del
/// pool o de cualquier sesión abierta); con `SinSesion` cae a la conexión
/// efímera de la Fase 2.
pub async fn sondear_via_servidor(
    peticion: &PeticionSondeo,
    cliente: &crate::cliente::Cliente,
) -> Sondeo {
    let inicio = Instant::now();
    let mut sondeo = Sondeo::vacio(peticion.host.id);
    let comando = comando_de_sondeo(&peticion.servicios);
    match cliente.ejecutar(peticion.host.id, &comando).await {
        crate::cliente::ResultadoEjecutar::Salida { salida, codigo } => {
            if codigo == 0 && !salida.trim().is_empty() {
                rellenar(&mut sondeo, parser::parsear(&salida));
            } else {
                sondeo.resultado = ResultadoSondeo::Error;
                sondeo.error = Some(format!("el sondeo devolvió el código {codigo}"));
            }
        }
        crate::cliente::ResultadoEjecutar::SinSesion => {
            // Sin conexión viva al host: efímera como en la Fase 2.
            let registro_vacio = crate::conexion::registro_sesiones();
            return sondear(peticion, &registro_vacio).await;
        }
    }
    sondeo.duracion_ms = inicio.elapsed().as_millis() as i64;
    sondeo
}

fn rellenar(sondeo: &mut Sondeo, metricas: parser::Metricas) {
    sondeo.nucleos = metricas.nucleos;
    sondeo.carga_1m = metricas.carga_1m;
    sondeo.carga_5m = metricas.carga_5m;
    sondeo.carga_15m = metricas.carga_15m;
    sondeo.mem_total_kb = metricas.mem_total_kb;
    sondeo.mem_disponible_kb = metricas.mem_disponible_kb;
    sondeo.disco_total_kb = metricas.disco_total_kb;
    sondeo.disco_usado_kb = metricas.disco_usado_kb;
    sondeo.red_rx_bytes = metricas.red_rx_bytes;
    sondeo.red_tx_bytes = metricas.red_tx_bytes;
    sondeo.uptime_seg = metricas.uptime_seg;
    let tiene_metricas = metricas.tiene_metricas();
    sondeo.servicios = metricas.servicios;
    sondeo.resultado = if tiene_metricas {
        ResultadoSondeo::Ok
    } else {
        ResultadoSondeo::SinMetricas
    };
}

async fn intento(
    peticion: &PeticionSondeo,
    registro: &RegistroSesiones,
) -> Result<parser::Metricas, String> {
    let comando = comando_de_sondeo(&peticion.servicios);
    let viva = registro.lock().await.get(&peticion.host.id).cloned();
    if let Some(handle) = viva {
        match ejecutar_en_handle(&handle, &comando).await {
            Ok(salida) => return Ok(parser::parsear(&salida)),
            Err(error) => warn!(
                host = %peticion.host.nombre,
                "el canal sobre la sesión viva falló ({error}); se usa conexión efímera"
            ),
        }
    }
    let cadena = construir_cadena(&peticion.host, &peticion.todos_los_hosts)
        .map_err(|error| error.to_string())?;
    let nombre_salto = cadena
        .get(cadena.len().saturating_sub(2))
        .map(|host| host.nombre.clone());
    let contexto = Contexto {
        known_hosts: peticion.known_hosts.clone(),
        dir_ssh: peticion.dir_ssh.clone(),
        hogar: peticion.hogar.clone(),
        usuario_local: peticion.usuario_local.clone(),
        tx: canal_testigo(),
        interactivo: false,
        fuente_contrasena: crate::conexion::FuenteContrasena::Llavero,
        // El sondeo no acepta reenvíos: no es una conexión de túneles.
        reenvios: None,
    };
    let transporte = conectar_cadena(&cadena, &contexto).await.map_err(|error| {
        let motivo = error.to_string();
        match &nombre_salto {
            Some(salto) if cadena.len() > 1 => format!("salto {salto}: {motivo}"),
            _ => motivo,
        }
    })?;
    let resultado = ejecutar_en_handle(&transporte.handle, &comando).await;
    let _ = transporte
        .handle
        .disconnect(Disconnect::ByApplication, "", "")
        .await;
    resultado.map(|salida| parser::parsear(&salida))
}

/// El sondeo nunca dialoga, pero `Contexto` exige un canal de eventos por
/// compatibilidad: se descarta nada más crear el contexto.
fn canal_testigo() -> mpsc::UnboundedSender<crate::conexion::EventoConexion> {
    let (tx, rx) = mpsc::unbounded_channel();
    drop(rx);
    tx
}

/// Ejecuta el comando en un canal `exec` y devuelve su salida.
async fn ejecutar_en_handle(
    handle: &russh::client::Handle<Cliente>,
    comando: &str,
) -> Result<String, String> {
    let mut canal = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    canal
        .exec(true, comando.as_bytes().to_vec())
        .await
        .map_err(|error| error.to_string())?;
    let mut salida = Vec::new();
    loop {
        match canal.wait().await {
            Some(ChannelMsg::Data { data }) => salida.extend_from_slice(&data),
            Some(ChannelMsg::ExtendedData { data, .. }) => salida.extend_from_slice(&data),
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    let _ = canal.close().await;
    Ok(String::from_utf8_lossy(&salida).to_string())
}

/// Lanza un sondeo por host respetando el semáforo de 8 concurrentes. Los
/// hosts con sesión viva van por el servidor (`Ejecutar`); el resto, por
/// conexión efímera.
pub fn lanzar_lote(
    runtime: &tokio::runtime::Runtime,
    peticiones: Vec<PeticionSondeo>,
    tx: mpsc::UnboundedSender<Sondeo>,
    con_sesion: std::collections::HashSet<i64>,
    cliente: crate::cliente::Cliente,
) {
    let semaforo = Arc::new(Semaphore::new(CONCURRENTES));
    for peticion in peticiones {
        let tx = tx.clone();
        let semaforo = semaforo.clone();
        let con_sesion = con_sesion.clone();
        let cliente = cliente.clone();
        runtime.spawn(async move {
            let _permiso = semaforo.acquire().await;
            let sondeo = if con_sesion.contains(&peticion.host.id) {
                sondear_via_servidor(&peticion, &cliente).await
            } else {
                sondear(&peticion, &crate::conexion::registro_sesiones()).await
            };
            let _ = tx.send(sondeo);
        });
    }
}

/// Comando `exec`: el script embebido viaja entrecomillado y las unidades del
/// inventario se pasan como argumentos validados, nunca interpolados.
pub fn comando_de_sondeo(servicios: &[String]) -> String {
    let mut comando = format!("sh -c {} magi", entrecomillar(SCRIPT));
    for servicio in servicios {
        comando.push(' ');
        comando.push_str(&entrecomillar(servicio));
    }
    comando
}

fn entrecomillar(texto: &str) -> String {
    format!("'{}'", texto.replace('\'', "'\\''"))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_comando_entrecomilla_el_script_y_los_servicios() {
        let comando = comando_de_sondeo(&["nginx".to_string(), "magi@.service".to_string()]);
        assert!(comando.starts_with("sh -c '#!/bin/sh"));
        assert!(comando.contains(" magi 'nginx' 'magi@.service'"));
    }

    #[test]
    fn las_comillas_simples_del_script_se_escapan() {
        let comando = comando_de_sondeo(&[]);
        assert!(comando.contains("'\\''"));
        assert!(comando.starts_with("sh -c '"));
    }
}
