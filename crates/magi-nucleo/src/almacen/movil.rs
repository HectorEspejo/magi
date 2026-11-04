//! Consultas propias del móvil (Fase 11): inventario de solo lectura con
//! uuid, plegado de grupos y los campos que solo escribe el teléfono.
//!
//! La app identifica hosts, grupos y túneles por su uuid: los ids cambian al
//! rehacer el inventario y no sirven como referencia estable.

use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::almacen::versiones::Tabla;
use crate::almacen::{etiquetas, grupos as grupos_bd, hosts as hosts_bd, tuneles as tuneles_bd};
use crate::modelo::{Host, Tunel, UltimoEstado};

/// Un grupo del inventario con su plegado y el número de hosts.
#[derive(Debug, Clone)]
pub struct GrupoInventario {
    pub uuid: String,
    pub nombre: String,
    pub orden: i64,
    pub plegado: bool,
    pub hosts: i64,
}

/// Un host del inventario con el uuid que usa la app.
#[derive(Debug, Clone)]
pub struct HostInventario {
    pub uuid: String,
    pub host: Host,
}

/// Inventario completo para la pantalla de Hosts del móvil.
#[derive(Debug, Clone)]
pub struct Inventario {
    pub grupos: Vec<GrupoInventario>,
    pub hosts: Vec<HostInventario>,
}

/// Grupos con su plegado y el número de hosts, en el orden de la vista.
pub fn grupos(conexion: &Connection) -> Result<Vec<GrupoInventario>> {
    let mut sentencia = conexion.prepare(
        "SELECT g.id, g.uuid, g.nombre, g.orden, g.plegado,
                (SELECT COUNT(*) FROM HOSTS h WHERE h.grupo_id = g.id)
           FROM GRUPOS g ORDER BY g.orden, g.nombre",
    )?;
    let filas = sentencia.query_map([], |fila| {
        Ok((
            fila.get::<_, i64>(0)?,
            fila.get::<_, Option<String>>(1)?,
            fila.get::<_, String>(2)?,
            fila.get::<_, i64>(3)?,
            fila.get::<_, i64>(4)?,
            fila.get::<_, i64>(5)?,
        ))
    })?;
    let mut lista = Vec::new();
    for fila in filas {
        let (_id, uuid, nombre, orden, plegado, hosts) = fila?;
        let Some(uuid) = uuid.filter(|uuid| !uuid.is_empty()) else {
            continue;
        };
        lista.push(GrupoInventario {
            uuid,
            nombre,
            orden,
            plegado: plegado != 0,
            hosts,
        });
    }
    Ok(lista)
}

/// Hosts con etiquetas y uuid, en el orden de la vista (por nombre).
pub fn hosts(conexion: &Connection) -> Result<Vec<HostInventario>> {
    let mut lista = hosts_bd::listar(conexion)?;
    let uuids = uuid_por_id(conexion, Tabla::Hosts)?;
    Ok(lista
        .drain(..)
        .map(|host| HostInventario {
            uuid: uuids.get(&host.id).cloned().unwrap_or_default(),
            host,
        })
        .collect())
}

/// Inventario completo.
pub fn inventario(conexion: &Connection) -> Result<Inventario> {
    Ok(Inventario {
        grupos: grupos(conexion)?,
        hosts: hosts(conexion)?,
    })
}

/// uuid de cada fila de una tabla que viaja, por id.
pub fn uuid_por_id(conexion: &Connection, tabla: Tabla) -> Result<HashMap<i64, String>> {
    let mut sentencia = conexion.prepare(&format!(
        "SELECT id, uuid FROM {} WHERE uuid IS NOT NULL AND uuid != ''",
        tabla.nombre()
    ))?;
    let filas = sentencia.query_map([], |fila| {
        Ok((fila.get::<_, i64>(0)?, fila.get::<_, String>(1)?))
    })?;
    let mut mapa = HashMap::new();
    for fila in filas {
        let (id, uuid) = fila?;
        mapa.insert(id, uuid);
    }
    Ok(mapa)
}

/// id de una fila por su uuid. None si no existe o no tiene uuid.
pub fn id_por_uuid(conexion: &Connection, tabla: Tabla, uuid: &str) -> Result<Option<i64>> {
    let id = conexion
        .query_row(
            &format!("SELECT id FROM {} WHERE uuid = ?1", tabla.nombre()),
            params![uuid],
            |fila| fila.get::<_, i64>(0),
        )
        .optional()
        .with_context(|| format!("buscando el uuid {uuid} en {}", tabla.nombre()))?;
    Ok(id)
}

/// Plegar o desplegar un grupo por uuid. Falso si el grupo no existe.
pub fn plegar_grupo(conexion: &Connection, uuid: &str, plegado: bool) -> Result<bool> {
    let Some(id) = id_por_uuid(conexion, Tabla::Grupos, uuid)? else {
        return Ok(false);
    };
    grupos_bd::alternar_plegado(conexion, id, plegado)?;
    Ok(true)
}

/// Host completo por uuid, con sus etiquetas.
pub fn host_por_uuid(conexion: &Connection, uuid: &str) -> Result<Option<Host>> {
    let Some(id) = id_por_uuid(conexion, Tabla::Hosts, uuid)? else {
        return Ok(None);
    };
    let mut host = hosts_bd::obtener(conexion, id)?;
    host.etiquetas = etiquetas::por_host(conexion)?
        .remove(&host.id)
        .unwrap_or_default();
    Ok(Some(host))
}

/// Túnel completo por uuid.
pub fn tunel_por_uuid(conexion: &Connection, uuid: &str) -> Result<Option<Tunel>> {
    let Some(id) = id_por_uuid(conexion, Tabla::Tuneles, uuid)? else {
        return Ok(None);
    };
    Ok(Some(tuneles_bd::obtener(conexion, id)?))
}

/// Fija el resultado del último intento de conexión de un host (campo propio
/// del móvil; no sube versión ni viaja).
pub fn marcar_estado(
    conexion: &Connection,
    uuid: &str,
    estado: Option<UltimoEstado>,
) -> Result<bool> {
    let Some(id) = id_por_uuid(conexion, Tabla::Hosts, uuid)? else {
        return Ok(false);
    };
    hosts_bd::marcar_estado(conexion, id, estado)?;
    Ok(true)
}

/// Marca la última conexión correcta de un host (campo propio del móvil).
pub fn marcar_conexion(conexion: &Connection, uuid: &str) -> Result<bool> {
    let Some(id) = id_por_uuid(conexion, Tabla::Hosts, uuid)? else {
        return Ok(false);
    };
    hosts_bd::marcar_conexion(conexion, id)?;
    Ok(true)
}

/// uuid de un host por su nombre. None si no existe o no tiene uuid.
pub fn uuid_por_nombre(conexion: &Connection, nombre: &str) -> Result<Option<String>> {
    let uuid = conexion
        .query_row(
            "SELECT uuid FROM HOSTS WHERE nombre = ?1",
            params![nombre],
            |fila| fila.get::<_, Option<String>>(0),
        )
        .optional()
        .with_context(|| format!("buscando el host {nombre}"))?;
    Ok(uuid.flatten().filter(|uuid| !uuid.is_empty()))
}
