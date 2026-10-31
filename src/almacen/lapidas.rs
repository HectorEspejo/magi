//! Lápidas de los elementos borrados (Fase 10).
//!
//! Un borrado viaja como una versión más: la lápida guarda el `uuid`, el tipo,
//! el nombre para mostrar, la versión del borrado y su vector. Se conservan
//! siempre: son pocas y evitan que un equipo que vuelve tras meses resucite lo
//! borrado (D131).

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};

/// Tipos de elemento que pueden tener lápida. Los mismos que viajan.
pub const TIPOS: [&str; 6] = [
    "grupo",
    "host",
    "identidad",
    "tunel",
    "snippet",
    "sincronizacion_dir",
];

/// Fila de `LAPIDAS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lapida {
    pub uuid: String,
    pub tipo: String,
    pub nombre: String,
    pub version: String,
    pub vector: String,
    pub borrado_en: String,
}

const SELECCION: &str = "SELECT uuid, tipo, nombre, version, vector, borrado_en FROM LAPIDAS";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Lapida> {
    Ok(Lapida {
        uuid: fila.get(0)?,
        tipo: fila.get(1)?,
        nombre: fila.get(2)?,
        version: fila.get(3)?,
        vector: fila.get(4)?,
        borrado_en: fila.get(5)?,
    })
}

pub fn tipo_valido(texto: &str) -> bool {
    TIPOS.contains(&texto)
}

pub fn listar(conexion: &Connection) -> Result<Vec<Lapida>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} ORDER BY uuid"))?;
    let filas = sentencia.query_map([], mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn listar_de_tipo(conexion: &Connection, tipo: &str) -> Result<Vec<Lapida>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} WHERE tipo = ?1 ORDER BY uuid"))?;
    let filas = sentencia.query_map(params![tipo], mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn obtener(conexion: &Connection, uuid: &str) -> Result<Option<Lapida>> {
    Ok(conexion
        .query_row(
            &format!("{SELECCION} WHERE uuid = ?1"),
            params![uuid],
            mapear,
        )
        .optional()?)
}

pub fn contar(conexion: &Connection) -> Result<i64> {
    Ok(conexion.query_row("SELECT COUNT(*) FROM LAPIDAS", [], |fila| fila.get(0))?)
}

/// Escribe (o actualiza) una lápida. La importación aplica el ganador de la
/// fusión, así que la fila que llega es la definitiva; si el mismo `uuid`
/// vuelve a borrarse tras resucitar, se queda la versión más nueva.
pub fn guardar(conexion: &Connection, lapida: &Lapida) -> Result<()> {
    if !tipo_valido(&lapida.tipo) {
        anyhow::bail!("tipo de lápida desconocido: «{}»", lapida.tipo);
    }
    conexion.execute(
        "INSERT INTO LAPIDAS (uuid, tipo, nombre, version, vector, borrado_en)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(uuid) DO UPDATE SET
             tipo = excluded.tipo,
             nombre = excluded.nombre,
             version = excluded.version,
             vector = excluded.vector,
             borrado_en = excluded.borrado_en",
        params![
            lapida.uuid,
            lapida.tipo,
            lapida.nombre,
            lapida.version,
            lapida.vector,
            lapida.borrado_en
        ],
    )?;
    Ok(())
}

/// Borra las lápidas de un tipo (solo pruebas y desactivación completa de la
/// sincronización: en el uso normal no se borra ninguna).
pub fn borrar_de_tipo(conexion: &Connection, tipo: &str) -> Result<usize> {
    Ok(conexion.execute("DELETE FROM LAPIDAS WHERE tipo = ?1", params![tipo])?)
}
