//! Conflictos de la sincronización del inventario (Fase 10).
//!
//! Un conflicto guarda lo que hizo falta para decidir: tipo (`elemento`,
//! `nombre`, `huella`, `borrado` u `huerfano`), elemento afectado, la versión
//! local que perdió (JSON completo) y quién ganó. Los resueltos se conservan
//! 90 días y luego se purgan. Las enumeraciones se validan en código.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};

/// Una versión local perdió frente a una concurrente con contenido distinto.
pub const TIPO_ELEMENTO: &str = "elemento";
/// Dos elementos distintos acabaron con el mismo nombre natural.
pub const TIPO_NOMBRE: &str = "nombre";
/// Este equipo ya tenía otra huella para el host y no se sustituye sola.
pub const TIPO_HUELLA: &str = "huella";
/// Un borrado entrante no se aplicó porque el elemento tiene trabajo vivo.
pub const TIPO_BORRADO: &str = "borrado";
/// Un túnel o una sincronización de directorio cuyo host no existe.
pub const TIPO_HUERFANO: &str = "huerfano";

pub const TIPOS: [&str; 5] = [
    TIPO_ELEMENTO,
    TIPO_NOMBRE,
    TIPO_HUELLA,
    TIPO_BORRADO,
    TIPO_HUERFANO,
];

pub const RESOLUCION_RECUPERADO: &str = "recuperado";
pub const RESOLUCION_DESCARTADO: &str = "descartado";
pub const RESOLUCION_ACEPTADO: &str = "aceptado";

/// Días que se conserva un conflicto resuelto antes de purgarse.
pub const DIAS_CONSERVACION_RESUELTOS: i64 = 90;

/// Fila de `CONFLICTOS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflicto {
    pub id: i64,
    pub tipo: String,
    pub elemento_tipo: String,
    pub uuid: String,
    pub nombre: String,
    pub version_perdedora_json: Option<String>,
    pub version_ganadora: Option<String>,
    pub equipo_ganador: Option<String>,
    pub detectado_en: String,
    pub resuelto_en: Option<String>,
    pub resolucion: Option<String>,
}

/// Datos para crear un conflicto nuevo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NuevoConflicto {
    pub tipo: String,
    pub elemento_tipo: String,
    pub uuid: String,
    pub nombre: String,
    pub version_perdedora_json: Option<String>,
    pub version_ganadora: Option<String>,
    pub equipo_ganador: Option<String>,
}

const SELECCION: &str = "
    SELECT id, tipo, elemento_tipo, uuid, nombre, version_perdedora_json,
           version_ganadora, equipo_ganador, detectado_en, resuelto_en, resolucion
      FROM CONFLICTOS";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Conflicto> {
    Ok(Conflicto {
        id: fila.get(0)?,
        tipo: fila.get(1)?,
        elemento_tipo: fila.get(2)?,
        uuid: fila.get(3)?,
        nombre: fila.get(4)?,
        version_perdedora_json: fila.get(5)?,
        version_ganadora: fila.get(6)?,
        equipo_ganador: fila.get(7)?,
        detectado_en: fila.get(8)?,
        resuelto_en: fila.get(9)?,
        resolucion: fila.get(10)?,
    })
}

pub fn tipo_valido(texto: &str) -> bool {
    TIPOS.contains(&texto)
}

pub fn resolucion_valida(texto: &str) -> bool {
    matches!(
        texto,
        RESOLUCION_RECUPERADO | RESOLUCION_DESCARTADO | RESOLUCION_ACEPTADO
    )
}

pub fn listar_pendientes(conexion: &Connection) -> Result<Vec<Conflicto>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE resuelto_en IS NULL ORDER BY detectado_en, id"
    ))?;
    let filas = sentencia.query_map([], mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn listar(conexion: &Connection, incluir_resueltos: bool) -> Result<Vec<Conflicto>> {
    let sentencia = if incluir_resueltos {
        format!("{SELECCION} ORDER BY detectado_en DESC, id DESC")
    } else {
        format!("{SELECCION} WHERE resuelto_en IS NULL ORDER BY detectado_en, id")
    };
    let mut sentencia = conexion.prepare(&sentencia)?;
    let filas = sentencia.query_map([], mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Option<Conflicto>> {
    Ok(conexion
        .query_row(&format!("{SELECCION} WHERE id = ?1"), params![id], mapear)
        .optional()?)
}

pub fn contar_pendientes(conexion: &Connection) -> Result<i64> {
    Ok(conexion.query_row(
        "SELECT COUNT(*) FROM CONFLICTOS WHERE resuelto_en IS NULL",
        [],
        |fila| fila.get(0),
    )?)
}

/// Crea un conflicto pendiente. No se duplica mientras siga pendiente el mismo
/// elemento con la misma versión ganadora: una pasada repetida no llena la
/// lista.
pub fn crear_si_nuevo(conexion: &Connection, nuevo: &NuevoConflicto) -> Result<Option<i64>> {
    if !tipo_valido(&nuevo.tipo) {
        anyhow::bail!("tipo de conflicto desconocido: «{}»", nuevo.tipo);
    }
    let duplicado: i64 = conexion.query_row(
        "SELECT COUNT(*) FROM CONFLICTOS
          WHERE resuelto_en IS NULL AND tipo = ?1 AND uuid = ?2
                AND COALESCE(version_ganadora, '') = COALESCE(?3, '')",
        params![nuevo.tipo, nuevo.uuid, nuevo.version_ganadora],
        |fila| fila.get(0),
    )?;
    if duplicado > 0 {
        return Ok(None);
    }
    // Si el usuario ya descartó este mismo conflicto para la misma versión
    // ganadora, no se le vuelve a preguntar. Los borrados aplazados son la
    // excepción: «descartar» en ellos significa «decidir más adelante».
    if nuevo.tipo != TIPO_BORRADO {
        let descartado: i64 = conexion.query_row(
            "SELECT COUNT(*) FROM CONFLICTOS
              WHERE resolucion = ?1 AND tipo = ?2 AND uuid = ?3
                    AND COALESCE(version_ganadora, '') = COALESCE(?4, '')",
            params![
                RESOLUCION_DESCARTADO,
                nuevo.tipo,
                nuevo.uuid,
                nuevo.version_ganadora
            ],
            |fila| fila.get(0),
        )?;
        if descartado > 0 {
            return Ok(None);
        }
    }
    conexion.execute(
        "INSERT INTO CONFLICTOS (
            tipo, elemento_tipo, uuid, nombre, version_perdedora_json,
            version_ganadora, equipo_ganador, detectado_en
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            nuevo.tipo,
            nuevo.elemento_tipo,
            nuevo.uuid,
            nuevo.nombre,
            nuevo.version_perdedora_json,
            nuevo.version_ganadora,
            nuevo.equipo_ganador,
            crate::modelo::fecha_ahora(),
        ],
    )?;
    Ok(Some(conexion.last_insert_rowid()))
}

/// Marca un conflicto como resuelto.
pub fn resolver(conexion: &Connection, id: i64, resolucion: &str) -> Result<()> {
    if !resolucion_valida(resolucion) {
        anyhow::bail!("resolución de conflicto desconocida: «{resolucion}»");
    }
    conexion.execute(
        "UPDATE CONFLICTOS SET resuelto_en = ?1, resolucion = ?2 WHERE id = ?3",
        params![crate::modelo::fecha_ahora(), resolucion, id],
    )?;
    Ok(())
}

/// Purga los conflictos resueltos hace más de 90 días. Devuelve cuántos.
pub fn purgar_resueltos(conexion: &Connection) -> Result<usize> {
    let corte = chrono::Local::now() - chrono::Duration::days(DIAS_CONSERVACION_RESUELTOS);
    let corte = corte.format("%Y-%m-%dT%H:%M:%S%:z").to_string();
    Ok(conexion.execute(
        "DELETE FROM CONFLICTOS WHERE resuelto_en IS NOT NULL AND resuelto_en < ?1",
        params![corte],
    )?)
}
