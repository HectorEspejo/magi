use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};

use crate::almacen::versiones;
use crate::modelo::{fecha_ahora, Grupo};

pub fn listar(conexion: &Connection) -> Result<Vec<Grupo>> {
    let mut sentencia = conexion.prepare(
        "SELECT id, nombre, orden, plegado, creado_en FROM GRUPOS ORDER BY orden, nombre",
    )?;
    let filas = sentencia.query_map([], |fila| {
        Ok(Grupo {
            id: fila.get(0)?,
            nombre: fila.get(1)?,
            orden: fila.get(2)?,
            plegado: fila.get::<_, i64>(3)? != 0,
            creado_en: fila.get(4)?,
        })
    })?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Grupo> {
    let grupo = conexion.query_row(
        "SELECT id, nombre, orden, plegado, creado_en FROM GRUPOS WHERE id = ?1",
        params![id],
        mapear,
    )?;
    Ok(grupo)
}

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Grupo> {
    Ok(Grupo {
        id: fila.get(0)?,
        nombre: fila.get(1)?,
        orden: fila.get(2)?,
        plegado: fila.get::<_, i64>(3)? != 0,
        creado_en: fila.get(4)?,
    })
}

pub fn por_nombre(conexion: &Connection, nombre: &str) -> Result<Option<Grupo>> {
    let mut sentencia = conexion
        .prepare("SELECT id, nombre, orden, plegado, creado_en FROM GRUPOS WHERE nombre = ?1")?;
    let mut filas = sentencia.query_map(params![nombre], mapear)?;
    match filas.next() {
        Some(grupo) => Ok(Some(grupo?)),
        None => Ok(None),
    }
}

/// Crea un grupo al final del orden. Devuelve error legible si el nombre ya
/// existe.
pub fn crear(conexion: &Connection, nombre: &str) -> Result<i64> {
    let nombre = nombre.trim();
    let transaccion = conexion.unchecked_transaction()?;
    let orden: i64 = transaccion
        .query_row(
            "SELECT COALESCE(MAX(orden), 0) + 1 FROM GRUPOS",
            [],
            |fila| fila.get(0),
        )
        .context("calculando el orden del grupo")?;
    transaccion
        .execute(
            "INSERT INTO GRUPOS (nombre, orden, plegado, creado_en) VALUES (?1, ?2, 0, ?3)",
            params![nombre, orden, fecha_ahora()],
        )
        .map_err(|error| {
            if crate::almacen::es_conflicto_unico(&error) {
                anyhow::anyhow!("ya existe un grupo con el nombre «{nombre}»")
            } else {
                anyhow::Error::new(error)
            }
        })?;
    let id = transaccion.last_insert_rowid();
    // Un grupo nuevo nace con uuid, versión y vector (Fase 10).
    versiones::registrar_nuevo(&transaccion, versiones::Tabla::Grupos, id, None)?;
    transaccion.commit()?;
    Ok(id)
}

pub fn renombrar(conexion: &Connection, id: i64, nombre: &str) -> Result<()> {
    let nombre = nombre.trim();
    let antes = versiones::datos_de(conexion, versiones::Tabla::Grupos, id)?;
    conexion
        .execute(
            "UPDATE GRUPOS SET nombre = ?1 WHERE id = ?2",
            params![nombre, id],
        )
        .map_err(|error| {
            if crate::almacen::es_conflicto_unico(&error) {
                anyhow::anyhow!("ya existe un grupo con el nombre «{nombre}»")
            } else {
                anyhow::Error::new(error)
            }
        })?;
    versiones::subir_si_cambio(conexion, versiones::Tabla::Grupos, id, antes.as_deref())?;
    Ok(())
}

/// Borra el grupo; sus hosts pasan a «sin grupo» (ON DELETE SET NULL).
pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    let transaccion = conexion.unchecked_transaction()?;
    let Some(grupo) = crate::almacen::fila_para_lapida(&transaccion, versiones::Tabla::Grupos, id)?
    else {
        return Ok(());
    };
    // Los hosts que se quedan sin grupo cuentan como editados (ON DELETE SET
    // NULL): se captura su contenido antes del borrado (Fase 10).
    let hosts: Vec<i64> = {
        let mut sentencia =
            transaccion.prepare("SELECT id FROM HOSTS WHERE grupo_id = ?1 ORDER BY id")?;
        let filas = sentencia.query_map(params![id], |fila| fila.get(0))?;
        filas.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let antes_hosts: Vec<Option<String>> = hosts
        .iter()
        .map(|host_id| versiones::datos_de(&transaccion, versiones::Tabla::Hosts, *host_id))
        .collect::<Result<Vec<_>>>()?;
    crate::almacen::dejar_lapida(
        &transaccion,
        versiones::Tabla::Grupos,
        &grupo.uuid,
        &grupo.nombre,
        &grupo.vector,
    )?;
    transaccion.execute("DELETE FROM GRUPOS WHERE id = ?1", params![id])?;
    for (host_id, antes) in hosts.iter().zip(antes_hosts.iter()) {
        versiones::subir_si_cambio(
            &transaccion,
            versiones::Tabla::Hosts,
            *host_id,
            antes.as_deref(),
        )?;
    }
    transaccion.commit()?;
    Ok(())
}

pub fn alternar_plegado(conexion: &Connection, id: i64, plegado: bool) -> Result<()> {
    conexion.execute(
        "UPDATE GRUPOS SET plegado = ?1 WHERE id = ?2",
        params![plegado as i64, id],
    )?;
    Ok(())
}

/// Mueve un grupo una posición arriba (`-1`) o abajo (`+1`) intercambiando el
/// orden con el vecino.
pub fn mover(conexion: &Connection, id: i64, direccion: i8) -> Result<()> {
    let grupos = listar(conexion)?;
    let Some(posicion) = grupos.iter().position(|grupo| grupo.id == id) else {
        return Ok(());
    };
    let destino = match direccion {
        -1 if posicion > 0 => posicion - 1,
        1 if posicion + 1 < grupos.len() => posicion + 1,
        _ => return Ok(()),
    };
    let a = &grupos[posicion];
    let b = &grupos[destino];
    let transaccion = conexion.unchecked_transaction()?;
    // El orden viaja: mover un grupo es una edición de los dos que
    // intercambian posición (Fase 10).
    let antes_a = versiones::datos_de(&transaccion, versiones::Tabla::Grupos, a.id)?;
    let antes_b = versiones::datos_de(&transaccion, versiones::Tabla::Grupos, b.id)?;
    transaccion.execute(
        "UPDATE GRUPOS SET orden = ?1 WHERE id = ?2",
        params![b.orden, a.id],
    )?;
    transaccion.execute(
        "UPDATE GRUPOS SET orden = ?1 WHERE id = ?2",
        params![a.orden, b.id],
    )?;
    versiones::subir_si_cambio(
        &transaccion,
        versiones::Tabla::Grupos,
        a.id,
        antes_a.as_deref(),
    )?;
    versiones::subir_si_cambio(
        &transaccion,
        versiones::Tabla::Grupos,
        b.id,
        antes_b.as_deref(),
    )?;
    transaccion.commit()?;
    Ok(())
}

pub fn hosts_en_grupo(conexion: &Connection, id: i64) -> Result<i64> {
    let total = conexion.query_row(
        "SELECT COUNT(*) FROM HOSTS WHERE grupo_id = ?1",
        params![id],
        |fila| fila.get(0),
    )?;
    Ok(total)
}
