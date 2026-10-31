//! Reloj híbrido y versiones de los elementos que viajan (Fase 10).
//!
//! Cada elemento que viaja lleva version (<ms con 13 cifras>.<uuid del
//! equipo>) y vector (JSON con la última edición vista de cada equipo). Toda
//! escritura del almacén sobre un campo que viaja llama aquí para subir la
//! versión; los campos propios del equipo no la tocan.
//!
//! La importación **no** usa este camino: escribe las versiones del fichero
//! tal cual, en su propia transacción.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::almacen::equipos;

/// Tablas del inventario que llevan uuid, version y vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tabla {
    Grupos,
    Hosts,
    Identidades,
    Tuneles,
    Snippets,
    SincronizacionesDir,
}

impl Tabla {
    /// Nombre de la tabla en SQLite (constante del código, nunca de fuera).
    pub fn nombre(self) -> &'static str {
        match self {
            Tabla::Grupos => "GRUPOS",
            Tabla::Hosts => "HOSTS",
            Tabla::Identidades => "IDENTIDADES",
            Tabla::Tuneles => "TUNELES",
            Tabla::Snippets => "SNIPPETS",
            Tabla::SincronizacionesDir => "SINCRONIZACIONES_DIR",
        }
    }

    /// Tipo del elemento en el formato de sincronización.
    pub fn tipo_elemento(self) -> &'static str {
        match self {
            Tabla::Grupos => "grupo",
            Tabla::Hosts => "host",
            Tabla::Identidades => "identidad",
            Tabla::Tuneles => "tunel",
            Tabla::Snippets => "snippet",
            Tabla::SincronizacionesDir => "sincronizacion_dir",
        }
    }

    pub fn desde_tipo_elemento(texto: &str) -> Option<Self> {
        match texto {
            "grupo" => Some(Tabla::Grupos),
            "host" => Some(Tabla::Hosts),
            "identidad" => Some(Tabla::Identidades),
            "tunel" => Some(Tabla::Tuneles),
            "snippet" => Some(Tabla::Snippets),
            "sincronizacion_dir" => Some(Tabla::SincronizacionesDir),
            _ => None,
        }
    }

    pub fn todas() -> [Tabla; 6] {
        [
            Tabla::Grupos,
            Tabla::Hosts,
            Tabla::Identidades,
            Tabla::Tuneles,
            Tabla::Snippets,
            Tabla::SincronizacionesDir,
        ]
    }
}

/// Milisegundos desde la época Unix.
pub fn ms_ahora() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duracion| duracion.as_millis() as i64)
        .unwrap_or(0)
}

/// Formato de una versión: ms con 13 cifras, punto y uuid del equipo. El
/// orden se decide comparando el número, no el texto.
pub fn formatear_version(ms: i64, equipo: &str) -> String {
    format!("{ms:013}.{equipo}")
}

/// Milisegundos de una versión; None si no tiene la forma esperada.
pub fn ms_de_version(version: &str) -> Option<i64> {
    let (ms, uuid) = version.split_once('.')?;
    if uuid.is_empty() || ms.is_empty() || !ms.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    ms.parse().ok()
}

/// UUID del equipo que hizo la versión.
pub fn uuid_de_version(version: &str) -> Option<&str> {
    version.split_once('.').map(|(_, uuid)| uuid)
}

/// Vector JSON a mapa ordenado. Un texto corrupto cuenta como vector vacío.
pub fn vector_desde_json(texto: &str) -> BTreeMap<String, i64> {
    serde_json::from_str(texto).unwrap_or_default()
}

pub fn vector_a_json(vector: &BTreeMap<String, i64>) -> String {
    serde_json::to_string(vector).unwrap_or_else(|_| "{}".to_string())
}

/// Emite el siguiente milisegundo del reloj híbrido, max(ahora, reloj + 1),
/// y lo deja guardado en EQUIPOS.reloj_ms. Devuelve los milisegundos y el
/// uuid de este equipo.
pub fn emitir(conexion: &Connection) -> Result<(i64, String)> {
    let equipo = equipos::este(conexion)?;
    let ahora = ms_ahora();
    let ms = ahora.max(equipo.reloj_ms.unwrap_or(0).saturating_add(1));
    conexion
        .execute(
            "UPDATE EQUIPOS SET reloj_ms = ?1 WHERE id = ?2",
            params![ms, equipo.id],
        )
        .context("actualizando el reloj híbrido")?;
    Ok((ms, equipo.uuid))
}

/// Versión y vector de un elemento nuevo: solo la entrada de este equipo.
pub fn version_nueva(conexion: &Connection) -> Result<(String, String)> {
    let (ms, uuid_equipo) = emitir(conexion)?;
    let mut vector = BTreeMap::new();
    vector.insert(uuid_equipo.clone(), ms);
    Ok((formatear_version(ms, &uuid_equipo), vector_a_json(&vector)))
}

/// Versión y vector de una edición: conserva el vector heredado y sube la
/// entrada de este equipo.
pub fn version_de_edicion(
    conexion: &Connection,
    vector_anterior: &str,
) -> Result<(String, String)> {
    let (ms, uuid_equipo) = emitir(conexion)?;
    let mut vector = vector_desde_json(vector_anterior);
    vector.insert(uuid_equipo.clone(), ms);
    Ok((formatear_version(ms, &uuid_equipo), vector_a_json(&vector)))
}

/// Versión y vector de una lápida: parte del vector del elemento borrado para
/// que el borrado domine las ediciones que ya había visto.
pub fn version_de_lapida(conexion: &Connection, vector_anterior: &str) -> Result<(String, String)> {
    version_de_edicion(conexion, vector_anterior)
}

/// Sube a la mayor versión vista al importar, para que las ediciones
/// posteriores de este equipo sean más recientes que lo importado.
pub fn al_importar(conexion: &Connection, mayor_ms: i64) -> Result<()> {
    let equipo = equipos::este(conexion)?;
    let reloj = equipo.reloj_ms.unwrap_or(0).max(mayor_ms);
    equipos::fijar_reloj(conexion, equipo.id, reloj)
}

/// Asigna uuid, version y vector a una fila recién insertada. En IDENTIDADES
/// el uuid es determinista (UUIDv5 de la huella) y lo pasa quien inserta; en
/// el resto se genera un v4.
pub fn registrar_nuevo(
    conexion: &Connection,
    tabla: Tabla,
    id: i64,
    uuid: Option<&str>,
) -> Result<String> {
    let uuid = match uuid {
        Some(uuid) => uuid.to_string(),
        None => uuid::Uuid::new_v4().to_string(),
    };
    let (version, vector) = version_nueva(conexion)?;
    conexion.execute(
        &format!(
            "UPDATE {} SET uuid = ?1, version = ?2, vector = ?3 WHERE id = ?4",
            tabla.nombre()
        ),
        params![uuid, version, vector, id],
    )?;
    Ok(uuid)
}

/// Sube la versión de una fila por su id. Una fila sin uuid no se toca.
pub fn subir(conexion: &Connection, tabla: Tabla, id: i64) -> Result<()> {
    let anterior: Option<(Option<String>, Option<String>)> = conexion
        .query_row(
            &format!("SELECT uuid, vector FROM {} WHERE id = ?1", tabla.nombre()),
            params![id],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .optional()?;
    let Some((Some(uuid), vector)) = anterior else {
        return Ok(());
    };
    if uuid.is_empty() {
        return Ok(());
    }
    let vector = vector.unwrap_or_else(|| "{}".to_string());
    let (version, vector) = version_de_edicion(conexion, &vector)?;
    conexion.execute(
        &format!(
            "UPDATE {} SET version = ?1, vector = ?2 WHERE id = ?3",
            tabla.nombre()
        ),
        params![version, vector, id],
    )?;
    Ok(())
}

/// Datos que viajan de una fila, serializados a JSON canónico, para comparar
/// antes y después de una escritura. None si la fila no existe.
pub fn datos_de(conexion: &Connection, tabla: Tabla, id: i64) -> Result<Option<String>> {
    crate::sincronizacion::elementos::datos_de_fila(conexion, tabla, id)
}

/// Sube la versión de una fila solo si cambió lo que viaja. 'antes' es el
/// JSON que devolvió datos_de antes de la escritura. Devuelve si subió.
pub fn subir_si_cambio(
    conexion: &Connection,
    tabla: Tabla,
    id: i64,
    antes: Option<&str>,
) -> Result<bool> {
    let despues = datos_de(conexion, tabla, id)?;
    if antes == despues.as_deref() {
        return Ok(false);
    }
    subir(conexion, tabla, id)?;
    Ok(true)
}

/// Sube la versión de varias filas (p. ej. los hosts a los que una cascada de
/// SQLite les ha puesto una referencia a nulo).
pub fn subir_varias(conexion: &Connection, tabla: Tabla, ids: &[i64]) -> Result<()> {
    for id in ids {
        subir(conexion, tabla, *id)?;
    }
    Ok(())
}

/// Reloj híbrido de este equipo.
pub fn reloj_de_este(conexion: &Connection) -> Result<i64> {
    Ok(equipos::este(conexion)?.reloj_ms.unwrap_or(0))
}
