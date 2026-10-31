//! Equipos de la sincronización del inventario (Fase 10).
//!
//! Un equipo es un MAGI más que escribe su fichero en la carpeta compartida.
//! Solo la fila de este equipo tiene `es_este = 1` y `reloj_ms`; las demás
//! guardan lo que se sabe de sus ficheros. `estado` y las enumeraciones se
//! validan en código, nunca con `CHECK` en SQLite.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

/// Estado del equipo dueño de este inventario.
pub const ESTADO_ESTE: &str = "este";
/// El equipo tiene un fichero legible y al día.
pub const ESTADO_AL_DIA: &str = "al_dia";
/// Su fichero no se descifra con la clave de este equipo.
pub const ESTADO_ILEGIBLE: &str = "ilegible";
/// Su fichero se descifra pero la firma no cuadra o está cortado.
pub const ESTADO_DANADO: &str = "danado";
/// Su fichero usa un `formato` más nuevo que el que conoce este binario.
pub const ESTADO_FORMATO_NUEVO: &str = "formato_nuevo";
/// Este equipo lo ha quitado de la sincronización.
pub const ESTADO_QUITADO: &str = "quitado";

/// `plataforma` de este equipo: los valores que entiende el formato.
pub fn plataforma_actual() -> &'static str {
    match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "android" => "android",
        _ => "linux",
    }
}

/// Nombre por defecto de este equipo: el hostname, o «magi» si no se puede
/// leer. Nunca es vacío: el nombre viaja en el fichero.
pub fn nombre_actual() -> String {
    match nix::unistd::gethostname() {
        Ok(nombre) => {
            let texto = nombre.to_string_lossy().trim().to_string();
            let texto = texto.split('.').next().unwrap_or(&texto).trim().to_string();
            if texto.is_empty() {
                "magi".to_string()
            } else {
                texto
            }
        }
        Err(_) => "magi".to_string(),
    }
}

/// Fila de `EQUIPOS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Equipo {
    pub id: i64,
    pub uuid: String,
    pub nombre: String,
    pub plataforma: String,
    pub es_este: bool,
    pub app_version: Option<String>,
    pub ultimo_fichero_en: Option<String>,
    pub ultima_importacion_en: Option<String>,
    pub hash_ultimo_fichero: Option<String>,
    pub estado: String,
    pub quitado_en: Option<String>,
    pub reloj_ms: Option<i64>,
}

const SELECCION: &str = "
    SELECT id, uuid, nombre, plataforma, es_este, app_version,
           ultimo_fichero_en, ultima_importacion_en, hash_ultimo_fichero,
           estado, quitado_en, reloj_ms
      FROM EQUIPOS";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Equipo> {
    Ok(Equipo {
        id: fila.get(0)?,
        uuid: fila.get(1)?,
        nombre: fila.get(2)?,
        plataforma: fila.get(3)?,
        es_este: fila.get::<_, i64>(4)? != 0,
        app_version: fila.get(5)?,
        ultimo_fichero_en: fila.get(6)?,
        ultima_importacion_en: fila.get(7)?,
        hash_ultimo_fichero: fila.get(8)?,
        estado: fila.get(9)?,
        quitado_en: fila.get(10)?,
        reloj_ms: fila.get(11)?,
    })
}

/// ¿Es un estado de equipo conocido?
pub fn estado_valido(texto: &str) -> bool {
    matches!(
        texto,
        ESTADO_ESTE
            | ESTADO_AL_DIA
            | ESTADO_ILEGIBLE
            | ESTADO_DANADO
            | ESTADO_FORMATO_NUEVO
            | ESTADO_QUITADO
    )
}

pub fn listar(conexion: &Connection) -> Result<Vec<Equipo>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} ORDER BY es_este DESC, nombre"))?;
    let filas = sentencia.query_map([], mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Equipo> {
    conexion
        .query_row(&format!("{SELECCION} WHERE id = ?1"), params![id], mapear)
        .with_context(|| format!("leyendo el equipo {id}"))
}

/// La fila de este equipo (la única con `es_este = 1`).
pub fn este(conexion: &Connection) -> Result<Equipo> {
    conexion
        .query_row(&format!("{SELECCION} WHERE es_este = 1"), [], mapear)
        .context("no hay fila de este equipo en EQUIPOS")
}

pub fn por_uuid(conexion: &Connection, uuid: &str) -> Result<Option<Equipo>> {
    Ok(conexion
        .query_row(
            &format!("{SELECCION} WHERE uuid = ?1"),
            params![uuid],
            mapear,
        )
        .optional()?)
}

/// Alta de un equipo que aparece en la carpeta (nunca de este equipo: ese lo
/// crea la migración 7).
pub fn crear(conexion: &Connection, uuid: &str, nombre: &str, plataforma: &str) -> Result<i64> {
    conexion.execute(
        "INSERT INTO EQUIPOS (uuid, nombre, plataforma, es_este, estado)
         VALUES (?1, ?2, ?3, 0, ?4)",
        params![uuid, nombre, plataforma, ESTADO_AL_DIA],
    )?;
    Ok(conexion.last_insert_rowid())
}

pub fn fijar_estado(conexion: &Connection, id: i64, estado: &str) -> Result<()> {
    if !estado_valido(estado) {
        anyhow::bail!("estado de equipo desconocido: «{estado}»");
    }
    conexion.execute(
        "UPDATE EQUIPOS SET estado = ?1 WHERE id = ?2",
        params![estado, id],
    )?;
    Ok(())
}

/// Datos del último fichero leído de un equipo ajeno.
pub fn fijar_ultimo_fichero(
    conexion: &Connection,
    id: i64,
    hash: &str,
    generado_en: &str,
    app_version: Option<&str>,
) -> Result<()> {
    conexion.execute(
        "UPDATE EQUIPOS SET hash_ultimo_fichero = ?1, ultimo_fichero_en = ?2,
                app_version = ?3, estado = ?4
          WHERE id = ?5",
        params![hash, generado_en, app_version, ESTADO_AL_DIA, id],
    )?;
    Ok(())
}

/// SHA-256 del contenido canónico del fichero propio (para no reexportar sin
/// cambios) y fecha de la última importación aplicada.
pub fn fijar_hash_propio(conexion: &Connection, id: i64, hash: &str) -> Result<()> {
    conexion.execute(
        "UPDATE EQUIPOS SET hash_ultimo_fichero = ?1 WHERE id = ?2",
        params![hash, id],
    )?;
    Ok(())
}

pub fn fijar_importacion(conexion: &Connection, id: i64, generado_en: &str) -> Result<()> {
    conexion.execute(
        "UPDATE EQUIPOS SET ultima_importacion_en = ?1 WHERE id = ?2",
        params![generado_en, id],
    )?;
    Ok(())
}

pub fn fijar_app_version(conexion: &Connection, id: i64, app_version: &str) -> Result<()> {
    conexion.execute(
        "UPDATE EQUIPOS SET app_version = ?1 WHERE id = ?2",
        params![app_version, id],
    )?;
    Ok(())
}

pub fn fijar_reloj(conexion: &Connection, id: i64, reloj_ms: i64) -> Result<()> {
    conexion.execute(
        "UPDATE EQUIPOS SET reloj_ms = ?1 WHERE id = ?2",
        params![reloj_ms, id],
    )?;
    Ok(())
}

pub fn renombrar(conexion: &Connection, id: i64, nombre: &str) -> Result<()> {
    conexion.execute(
        "UPDATE EQUIPOS SET nombre = ?1 WHERE id = ?2",
        params![nombre.trim(), id],
    )?;
    Ok(())
}

/// Marca un equipo como quitado (su fichero se ignora en este equipo).
pub fn marcar_quitado(conexion: &Connection, id: i64, quitado_en: &str) -> Result<()> {
    conexion.execute(
        "UPDATE EQUIPOS SET estado = ?1, quitado_en = ?2 WHERE id = ?3",
        params![ESTADO_QUITADO, quitado_en, id],
    )?;
    Ok(())
}

/// Quita la fila de un equipo desconocido cuyo fichero ya no está (solo se
/// usa al desactivar; los equipos conocidos nunca se borran, se marcan).
pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute(
        "DELETE FROM EQUIPOS WHERE id = ?1 AND es_este = 0",
        params![id],
    )?;
    Ok(())
}
