//! CRUD de `SINCRONIZACIONES_DIR` (Fase 8). Lo hace el cliente, que además
//! escribe `ultima_ejecucion_en` al lanzar una guardada; el servidor solo
//! escribe `ultimo_resultado` al terminar, por su hilo escritor (D101).

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, Row};

use crate::modelo::{
    fecha_ahora, validar_sincronizacion, DatosSincronizacion, ResultadoSincronizacion,
    Sincronizacion,
};
use crate::protocolo::Direccion;

const SELECCION: &str = "SELECT s.id, s.host_id, h.nombre, s.nombre, s.ruta_local, \
     s.ruta_remota, s.direccion, s.borrar, s.exclusiones, s.ultima_ejecucion_en, \
     s.ultimo_resultado, s.creado_en, s.actualizado_en \
     FROM SINCRONIZACIONES_DIR s JOIN HOSTS h ON h.id = s.host_id";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Sincronizacion> {
    let direccion: String = fila.get(6)?;
    // Una dirección desconocida no se interpreta como la otra: invertirla
    // borraría en el lado equivocado. La fila se salta con aviso.
    let Some(direccion) = Direccion::desde_texto(&direccion) else {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            format!("dirección de sincronización desconocida: «{direccion}»").into(),
        ));
    };
    let exclusiones: String = fila.get(8)?;
    let ultimo: Option<String> = fila.get(10)?;
    Ok(Sincronizacion {
        id: fila.get(0)?,
        host_id: fila.get(1)?,
        host_nombre: fila.get(2)?,
        nombre: fila.get(3)?,
        ruta_local: fila.get(4)?,
        ruta_remota: fila.get(5)?,
        direccion,
        borrar: fila.get::<_, i64>(7)? != 0,
        exclusiones: partir_exclusiones(&exclusiones),
        ultima_ejecucion_en: fila.get(9)?,
        ultimo_resultado: ultimo
            .as_deref()
            .and_then(ResultadoSincronizacion::desde_texto),
        creado_en: fila.get(11)?,
        actualizado_en: fila.get(12)?,
    })
}

/// Patrones guardados, uno por línea, sin líneas vacías.
pub fn partir_exclusiones(texto: &str) -> Vec<String> {
    texto
        .lines()
        .map(str::trim)
        .filter(|linea| !linea.is_empty())
        .map(str::to_string)
        .collect()
}

fn juntar_exclusiones(patrones: &[String]) -> String {
    patrones
        .iter()
        .map(|patron| patron.trim())
        .filter(|patron| !patron.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Las filas legibles; una ilegible no deja sin listar a las demás.
fn recoger(
    filas: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<Sincronizacion>>,
) -> Vec<Sincronizacion> {
    let mut lista = Vec::new();
    for fila in filas {
        match fila {
            Ok(sincronizacion) => lista.push(sincronizacion),
            Err(error) => tracing::warn!("sincronización ilegible, se salta: {error}"),
        }
    }
    lista
}

/// Todas, ordenadas por host y nombre (`magi sincronizaciones`, paleta).
pub fn listar(conexion: &Connection) -> Result<Vec<Sincronizacion>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} ORDER BY h.nombre, s.nombre"))?;
    let filas = sentencia.query_map([], mapear)?;
    Ok(recoger(filas))
}

/// Las de un host, por nombre (`L`).
pub fn de_host(conexion: &Connection, host_id: i64) -> Result<Vec<Sincronizacion>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE s.host_id = ?1 ORDER BY s.nombre"
    ))?;
    let filas = sentencia.query_map(params![host_id], mapear)?;
    Ok(recoger(filas))
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Sincronizacion> {
    Ok(conexion.query_row(&format!("{SELECCION} WHERE s.id = ?1"), params![id], mapear)?)
}

pub fn crear(conexion: &Connection, datos: &DatosSincronizacion) -> Result<i64> {
    validar_sincronizacion(datos).map_err(|motivo| anyhow!(motivo))?;
    let nombre = datos.nombre.trim().to_string();
    let ahora = fecha_ahora();
    conexion
        .execute(
            "INSERT INTO SINCRONIZACIONES_DIR (host_id, nombre, ruta_local, ruta_remota, \
             direccion, borrar, exclusiones, creado_en, actualizado_en) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                datos.host_id,
                nombre,
                datos.ruta_local.trim(),
                datos.ruta_remota.trim(),
                datos.direccion.texto(),
                datos.borrar as i64,
                juntar_exclusiones(&datos.exclusiones),
                ahora
            ],
        )
        .map_err(|error| conflicto(error, &nombre))?;
    Ok(conexion.last_insert_rowid())
}

pub fn actualizar(conexion: &Connection, id: i64, datos: &DatosSincronizacion) -> Result<()> {
    validar_sincronizacion(datos).map_err(|motivo| anyhow!(motivo))?;
    let nombre = datos.nombre.trim().to_string();
    let cambiadas = conexion
        .execute(
            "UPDATE SINCRONIZACIONES_DIR SET host_id = ?1, nombre = ?2, ruta_local = ?3, \
             ruta_remota = ?4, direccion = ?5, borrar = ?6, exclusiones = ?7, \
             actualizado_en = ?8 WHERE id = ?9",
            params![
                datos.host_id,
                nombre,
                datos.ruta_local.trim(),
                datos.ruta_remota.trim(),
                datos.direccion.texto(),
                datos.borrar as i64,
                juntar_exclusiones(&datos.exclusiones),
                fecha_ahora(),
                id
            ],
        )
        .map_err(|error| conflicto(error, &nombre))?;
    if cambiadas == 0 {
        return Err(anyhow!("esa sincronización ya no existe"));
    }
    Ok(())
}

pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute(
        "DELETE FROM SINCRONIZACIONES_DIR WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// El cliente acaba de lanzar la guardada.
pub fn marcar_ejecucion(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute(
        "UPDATE SINCRONIZACIONES_DIR SET ultima_ejecucion_en = ?1 WHERE id = ?2",
        params![fecha_ahora(), id],
    )?;
    Ok(())
}

/// El servidor terminó de ejecutarla (por su hilo escritor). Una fila que ya
/// no existe no es un error: se borró mientras corría.
pub fn fijar_resultado(
    conexion: &Connection,
    id: i64,
    resultado: ResultadoSincronizacion,
) -> Result<()> {
    conexion.execute(
        "UPDATE SINCRONIZACIONES_DIR SET ultimo_resultado = ?1 WHERE id = ?2",
        params![resultado.como_texto(), id],
    )?;
    Ok(())
}

fn conflicto(error: rusqlite::Error, nombre: &str) -> anyhow::Error {
    if crate::almacen::es_conflicto_unico(&error) {
        anyhow!("ya existe una sincronización «{nombre}» en ese host")
    } else {
        anyhow::Error::new(error)
    }
}
