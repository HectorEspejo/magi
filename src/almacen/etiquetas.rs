use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::modelo::Etiqueta;

pub fn listar(conexion: &Connection) -> Result<Vec<Etiqueta>> {
    let mut sentencia = conexion.prepare("SELECT id, nombre FROM ETIQUETAS ORDER BY nombre")?;
    let filas = sentencia.query_map([], |fila| {
        Ok(Etiqueta {
            id: fila.get(0)?,
            nombre: fila.get(1)?,
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Normaliza una lista de etiquetas: minúsculas, sin vacíos ni duplicados.
pub fn normalizar(nombres: &[String]) -> Vec<String> {
    let mut normalizadas = Vec::new();
    for nombre in nombres {
        let nombre = nombre.trim().to_lowercase();
        if nombre.is_empty() || normalizadas.contains(&nombre) {
            continue;
        }
        normalizadas.push(nombre);
    }
    normalizadas
}

pub fn de_host(conexion: &Connection, host_id: i64) -> Result<Vec<String>> {
    let mut sentencia = conexion.prepare(
        "SELECT e.nombre FROM HOST_ETIQUETAS he
         JOIN ETIQUETAS e ON e.id = he.etiqueta_id
         WHERE he.host_id = ?1
         ORDER BY e.nombre",
    )?;
    let filas = sentencia.query_map(params![host_id], |fila| fila.get(0))?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Etiquetas de todos los hosts indexadas por `host_id`.
pub fn por_host(conexion: &Connection) -> Result<HashMap<i64, Vec<String>>> {
    let mut sentencia = conexion.prepare(
        "SELECT he.host_id, e.nombre FROM HOST_ETIQUETAS he
         JOIN ETIQUETAS e ON e.id = he.etiqueta_id
         ORDER BY e.nombre",
    )?;
    let filas = sentencia.query_map([], |fila| {
        Ok((fila.get::<_, i64>(0)?, fila.get::<_, String>(1)?))
    })?;
    let mut mapa: HashMap<i64, Vec<String>> = HashMap::new();
    for fila in filas {
        let (host_id, nombre) = fila?;
        mapa.entry(host_id).or_default().push(nombre);
    }
    Ok(mapa)
}

/// Sustituye las etiquetas de un host creando las que falten.
pub fn fijar_de_host(conexion: &Connection, host_id: i64, nombres: &[String]) -> Result<()> {
    let normalizadas = normalizar(nombres);
    let transaccion = conexion.unchecked_transaction()?;
    transaccion.execute(
        "DELETE FROM HOST_ETIQUETAS WHERE host_id = ?1",
        params![host_id],
    )?;
    for nombre in &normalizadas {
        transaccion.execute(
            "INSERT OR IGNORE INTO ETIQUETAS (nombre) VALUES (?1)",
            params![nombre],
        )?;
        let etiqueta_id: i64 = transaccion.query_row(
            "SELECT id FROM ETIQUETAS WHERE nombre = ?1",
            params![nombre],
            |fila| fila.get(0),
        )?;
        transaccion.execute(
            "INSERT OR IGNORE INTO HOST_ETIQUETAS (host_id, etiqueta_id) VALUES (?1, ?2)",
            params![host_id, etiqueta_id],
        )?;
    }
    transaccion.commit()?;
    limpiar_huerfanas(conexion)?;
    Ok(())
}

/// Elimina las etiquetas que se han quedado sin hosts.
pub fn limpiar_huerfanas(conexion: &Connection) -> Result<()> {
    conexion.execute(
        "DELETE FROM ETIQUETAS
         WHERE id NOT IN (SELECT etiqueta_id FROM HOST_ETIQUETAS)",
        [],
    )?;
    Ok(())
}
