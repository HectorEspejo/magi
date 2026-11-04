use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use rusqlite::{params, Connection, Row};

use crate::modelo::{ResultadoSondeo, Sondeo};

const SELECCION: &str = "
    SELECT id, host_id, fecha, resultado, error, nucleos, carga_1m, carga_5m,
           carga_15m, mem_total_kb, mem_disponible_kb, disco_total_kb,
           disco_usado_kb, red_rx_bytes, red_tx_bytes, uptime_seg,
           servicios_json, duracion_ms
      FROM SONDEOS";

/// Número de sondeos que se conservan por host.
pub const LIMITE_POR_HOST: i64 = 20;

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Sondeo> {
    let servicios_json: Option<String> = fila.get(16)?;
    let servicios: BTreeMap<String, String> = servicios_json
        .as_deref()
        .and_then(|texto| serde_json::from_str(texto).ok())
        .unwrap_or_default();
    Ok(Sondeo {
        id: fila.get(0)?,
        host_id: fila.get(1)?,
        fecha: fila.get(2)?,
        resultado: ResultadoSondeo::desde_texto(&fila.get::<_, String>(3)?),
        error: fila.get(4)?,
        nucleos: fila.get(5)?,
        carga_1m: fila.get(6)?,
        carga_5m: fila.get(7)?,
        carga_15m: fila.get(8)?,
        mem_total_kb: fila.get(9)?,
        mem_disponible_kb: fila.get(10)?,
        disco_total_kb: fila.get(11)?,
        disco_usado_kb: fila.get(12)?,
        red_rx_bytes: fila.get(13)?,
        red_tx_bytes: fila.get(14)?,
        uptime_seg: fila.get(15)?,
        servicios,
        duracion_ms: fila.get(17)?,
    })
}

/// Guarda un sondeo y purga los que superan los 20 últimos del host.
pub fn guardar(conexion: &Connection, sondeo: &Sondeo) -> Result<i64> {
    let servicios_json = if sondeo.servicios.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&sondeo.servicios)?)
    };
    conexion.execute(
        "INSERT INTO SONDEOS (
            host_id, fecha, resultado, error, nucleos, carga_1m, carga_5m,
            carga_15m, mem_total_kb, mem_disponible_kb, disco_total_kb,
            disco_usado_kb, red_rx_bytes, red_tx_bytes, uptime_seg,
            servicios_json, duracion_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            sondeo.host_id,
            sondeo.fecha,
            sondeo.resultado.como_texto(),
            sondeo.error,
            sondeo.nucleos,
            sondeo.carga_1m,
            sondeo.carga_5m,
            sondeo.carga_15m,
            sondeo.mem_total_kb,
            sondeo.mem_disponible_kb,
            sondeo.disco_total_kb,
            sondeo.disco_usado_kb,
            sondeo.red_rx_bytes,
            sondeo.red_tx_bytes,
            sondeo.uptime_seg,
            servicios_json,
            sondeo.duracion_ms,
        ],
    )?;
    let id = conexion.last_insert_rowid();
    conexion.execute(
        "DELETE FROM SONDEOS
          WHERE host_id = ?1
            AND id NOT IN (
                SELECT id FROM SONDEOS WHERE host_id = ?1
                 ORDER BY id DESC LIMIT ?2
            )",
        params![sondeo.host_id, LIMITE_POR_HOST],
    )?;
    Ok(id)
}

/// Último sondeo de un host, si existe.
pub fn ultimo(conexion: &Connection, host_id: i64) -> Result<Option<Sondeo>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE host_id = ?1 ORDER BY id DESC LIMIT 1"
    ))?;
    let mut filas = sentencia.query_map(params![host_id], mapear)?;
    match filas.next() {
        Some(sondeo) => Ok(Some(sondeo?)),
        None => Ok(None),
    }
}

/// Últimos `limite` sondeos de un host, del más reciente al más antiguo.
pub fn ultimos(conexion: &Connection, host_id: i64, limite: i64) -> Result<Vec<Sondeo>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE host_id = ?1 ORDER BY id DESC LIMIT ?2"
    ))?;
    let filas = sentencia.query_map(params![host_id, limite], mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Último sondeo de cada host, para arrancar la vista Flota.
pub fn ultimos_por_host(conexion: &Connection) -> Result<HashMap<i64, Sondeo>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION}
          WHERE id IN (SELECT MAX(id) FROM SONDEOS GROUP BY host_id)"
    ))?;
    let filas = sentencia.query_map([], mapear)?;
    let mut mapa = HashMap::new();
    for sondeo in filas {
        let sondeo = sondeo?;
        mapa.insert(sondeo.host_id, sondeo);
    }
    Ok(mapa)
}

pub fn contar(conexion: &Connection) -> Result<i64> {
    let total = conexion.query_row("SELECT COUNT(*) FROM SONDEOS", [], |fila| fila.get(0))?;
    Ok(total)
}
