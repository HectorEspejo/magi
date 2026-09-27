//! CRUD de `SNIPPETS` y `SNIPPET_DESTINOS` (Fase 6). Lo hace el cliente; el
//! servidor de sesiones nunca lee estas tablas: recibe la ejecución ya
//! resuelta (comando sustituido y hosts).

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, Row};

use crate::modelo::fecha_ahora;
use crate::snippets::{normalizar, validar, DatosSnippet, Destino, Snippet};

const SELECCION: &str =
    "SELECT id, nombre, comando, descripcion, etiquetas, critico, timeout_seg, \
     parar_al_fallo, usado_veces, ultimo_uso_en, creado_en, actualizado_en FROM SNIPPETS";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Snippet> {
    let timeout: i64 = fila.get(6)?;
    // Un timeout fuera de rango (base tocada a mano) no se ejecuta tal cual.
    let timeout_seg = u32::try_from(timeout)
        .ok()
        .filter(|segundos| {
            (crate::snippets::TIMEOUT_MIN..=crate::snippets::TIMEOUT_MAX).contains(segundos)
        })
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Integer,
                format!("timeout fuera de rango: {timeout}").into(),
            )
        })?;
    let etiquetas: String = fila.get(4)?;
    Ok(Snippet {
        id: fila.get(0)?,
        nombre: fila.get(1)?,
        comando: fila.get(2)?,
        descripcion: fila.get(3)?,
        etiquetas: etiquetas.split_whitespace().map(str::to_string).collect(),
        critico: fila.get::<_, i64>(5)? != 0,
        timeout_seg,
        parar_al_fallo: fila.get::<_, i64>(7)? != 0,
        usado_veces: fila.get(8)?,
        ultimo_uso_en: fila.get(9)?,
        creado_en: fila.get(10)?,
        actualizado_en: fila.get(11)?,
        destinos: Vec::new(),
    })
}

/// Destinos de todos los snippets (o de uno), en el orden en que se guardaron.
/// Una fila con etiqueta y host a la vez, o sin ninguno, es ilegible: se salta.
fn destinos(conexion: &Connection, snippet_id: Option<i64>) -> Result<HashMap<i64, Vec<Destino>>> {
    let mut sentencia = conexion.prepare(
        "SELECT d.snippet_id, d.etiqueta, d.host_id, h.nombre \
         FROM SNIPPET_DESTINOS d LEFT JOIN HOSTS h ON h.id = d.host_id \
         WHERE ?1 IS NULL OR d.snippet_id = ?1 ORDER BY d.id",
    )?;
    let filas = sentencia.query_map(params![snippet_id], |fila| {
        Ok((
            fila.get::<_, i64>(0)?,
            fila.get::<_, Option<String>>(1)?,
            fila.get::<_, Option<i64>>(2)?,
            fila.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut mapa: HashMap<i64, Vec<Destino>> = HashMap::new();
    for fila in filas {
        let (snippet, etiqueta, host_id, host_nombre) = fila?;
        let destino = match (etiqueta, host_id, host_nombre) {
            (Some(etiqueta), None, _) => Destino::Etiqueta(etiqueta),
            (None, Some(id), Some(nombre)) => Destino::Host { id, nombre },
            _ => {
                tracing::warn!(snippet, "destino de snippet ilegible, se salta");
                continue;
            }
        };
        mapa.entry(snippet).or_default().push(destino);
    }
    Ok(mapa)
}

/// Filas leídas, saltando las ilegibles: un snippet roto no puede dejar sin
/// listar a los demás.
fn recoger(
    filas: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<Snippet>>,
) -> Vec<Snippet> {
    let mut snippets = Vec::new();
    for fila in filas {
        match fila {
            Ok(snippet) => snippets.push(snippet),
            Err(error) => tracing::warn!("snippet ilegible, se salta: {error}"),
        }
    }
    snippets
}

/// Todos los snippets con sus destinos, por nombre.
pub fn listar(conexion: &Connection) -> Result<Vec<Snippet>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} ORDER BY nombre"))?;
    let filas = sentencia.query_map([], mapear)?;
    let mut snippets = recoger(filas);
    let mut destinos = destinos(conexion, None)?;
    for snippet in &mut snippets {
        snippet.destinos = destinos.remove(&snippet.id).unwrap_or_default();
    }
    Ok(snippets)
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Snippet> {
    let mut snippet =
        conexion.query_row(&format!("{SELECCION} WHERE id = ?1"), params![id], mapear)?;
    snippet.destinos = destinos(conexion, Some(id))?
        .remove(&id)
        .unwrap_or_default();
    Ok(snippet)
}

/// Snippet por nombre exacto (los atajos de Flota y la CLI lo buscan así).
pub fn por_nombre(conexion: &Connection, nombre: &str) -> Result<Option<Snippet>> {
    let id: Option<i64> = conexion
        .query_row(
            "SELECT id FROM SNIPPETS WHERE nombre = ?1",
            params![nombre.trim()],
            |fila| fila.get(0),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            otro => Err(otro),
        })?;
    id.map(|id| obtener(conexion, id)).transpose()
}

/// Normaliza y valida; el error de nombre repetido se dice claro.
fn preparar(datos: &DatosSnippet) -> Result<DatosSnippet> {
    let datos = normalizar(datos.clone());
    validar(&datos).map_err(|motivo| anyhow!(motivo))?;
    Ok(datos)
}

fn conflicto(nombre: &str, error: rusqlite::Error) -> anyhow::Error {
    if crate::almacen::es_conflicto_unico(&error) {
        anyhow!("ya existe un snippet con el nombre «{nombre}»")
    } else {
        anyhow::Error::new(error)
    }
}

fn insertar_destinos(conexion: &Connection, snippet_id: i64, destinos: &[Destino]) -> Result<()> {
    for destino in destinos {
        match destino {
            Destino::Etiqueta(etiqueta) => conexion.execute(
                "INSERT INTO SNIPPET_DESTINOS (snippet_id, etiqueta, host_id) VALUES (?1, ?2, NULL)",
                params![snippet_id, etiqueta],
            )?,
            Destino::Host { id, .. } => conexion.execute(
                "INSERT INTO SNIPPET_DESTINOS (snippet_id, etiqueta, host_id) VALUES (?1, NULL, ?2)",
                params![snippet_id, id],
            )?,
        };
    }
    Ok(())
}

/// Crea un snippet con sus destinos, en una transacción.
pub fn crear(conexion: &Connection, datos: &DatosSnippet) -> Result<i64> {
    let datos = preparar(datos)?;
    let transaccion = conexion.unchecked_transaction()?;
    let ahora = fecha_ahora();
    transaccion
        .execute(
            "INSERT INTO SNIPPETS (nombre, comando, descripcion, etiquetas, critico, timeout_seg, \
             parar_al_fallo, usado_veces, ultimo_uso_en, creado_en, actualizado_en) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, NULL, ?8, ?8)",
            params![
                datos.nombre,
                datos.comando,
                datos.descripcion,
                datos.etiquetas.join(" "),
                datos.critico as i64,
                datos.timeout_seg,
                datos.parar_al_fallo as i64,
                ahora
            ],
        )
        .map_err(|error| conflicto(&datos.nombre, error))?;
    let id = transaccion.last_insert_rowid();
    insertar_destinos(&transaccion, id, &datos.destinos)?;
    transaccion.commit()?;
    Ok(id)
}

/// Actualiza un snippet y sustituye sus destinos, en una transacción.
pub fn actualizar(conexion: &Connection, id: i64, datos: &DatosSnippet) -> Result<()> {
    let datos = preparar(datos)?;
    let transaccion = conexion.unchecked_transaction()?;
    let cambiadas = transaccion
        .execute(
            "UPDATE SNIPPETS SET nombre = ?1, comando = ?2, descripcion = ?3, etiquetas = ?4, \
             critico = ?5, timeout_seg = ?6, parar_al_fallo = ?7, actualizado_en = ?8 \
             WHERE id = ?9",
            params![
                datos.nombre,
                datos.comando,
                datos.descripcion,
                datos.etiquetas.join(" "),
                datos.critico as i64,
                datos.timeout_seg,
                datos.parar_al_fallo as i64,
                fecha_ahora(),
                id
            ],
        )
        .map_err(|error| conflicto(&datos.nombre, error))?;
    if cambiadas == 0 {
        return Err(anyhow!("ese snippet ya no existe"));
    }
    transaccion.execute(
        "DELETE FROM SNIPPET_DESTINOS WHERE snippet_id = ?1",
        params![id],
    )?;
    insertar_destinos(&transaccion, id, &datos.destinos)?;
    transaccion.commit()?;
    Ok(())
}

/// Borra el snippet: sus destinos caen en cascada, los hosts que lo tenían
/// como «snippet al conectar» se quedan sin él y las deliberaciones conservan
/// la acción en texto (`SET NULL`).
pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute("DELETE FROM SNIPPETS WHERE id = ?1", params![id])?;
    Ok(())
}

/// Una ejecución más (lo anota el cliente al lanzar).
pub fn marcar_uso(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute(
        "UPDATE SNIPPETS SET usado_veces = usado_veces + 1, ultimo_uso_en = ?1 WHERE id = ?2",
        params![fecha_ahora(), id],
    )?;
    Ok(())
}

/// Hosts que tienen este snippet como «snippet al conectar» (id y nombre).
pub fn hosts_con_snippet_al_conectar(conexion: &Connection, id: i64) -> Result<Vec<(i64, String)>> {
    let mut sentencia = conexion.prepare(
        "SELECT id, nombre FROM HOSTS WHERE snippet_al_conectar_id = ?1 ORDER BY nombre",
    )?;
    let filas = sentencia.query_map(params![id], |fila| Ok((fila.get(0)?, fila.get(1)?)))?;
    Ok(filas.collect::<rusqlite::Result<Vec<(i64, String)>>>()?)
}
