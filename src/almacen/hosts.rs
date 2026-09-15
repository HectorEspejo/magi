use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};

use crate::almacen::etiquetas;
use crate::modelo::{fecha_ahora, DatosHost, Host, IdentidadRef, Origen, UltimoEstado};

const SELECCION: &str = "
    SELECT h.id, h.nombre, h.grupo_id, h.direccion, h.puerto, h.usuario,
           h.identidad_ref, h.salto_host_id, h.multiplexar, h.keepalive_seg,
           h.opciones_extra, h.origen, h.ultimo_estado, h.ultima_conexion_en,
           h.creado_en, h.actualizado_en, g.nombre, s.nombre
      FROM HOSTS h
      LEFT JOIN GRUPOS g ON g.id = h.grupo_id
      LEFT JOIN HOSTS  s ON s.id = h.salto_host_id";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Host> {
    Ok(Host {
        id: fila.get(0)?,
        nombre: fila.get(1)?,
        grupo_id: fila.get(2)?,
        direccion: fila.get(3)?,
        puerto: fila.get::<_, i64>(4)? as u16,
        usuario: fila.get(5)?,
        identidad_ref: IdentidadRef::desde_bd(fila.get::<_, Option<String>>(6)?.as_deref()),
        salto_host_id: fila.get(7)?,
        multiplexar: fila.get::<_, i64>(8)? != 0,
        keepalive_seg: fila.get::<_, Option<i64>>(9)?.map(|valor| valor as u32),
        opciones_extra: fila.get(10)?,
        origen: Origen::desde_texto(&fila.get::<_, String>(11)?),
        ultimo_estado: UltimoEstado::desde_texto(fila.get::<_, Option<String>>(12)?.as_deref()),
        ultima_conexion_en: fila.get(13)?,
        creado_en: fila.get(14)?,
        actualizado_en: fila.get(15)?,
        etiquetas: Vec::new(),
        grupo_nombre: fila.get(16)?,
        salto_nombre: fila.get(17)?,
    })
}

pub fn listar(conexion: &Connection) -> Result<Vec<Host>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} ORDER BY h.nombre"))?;
    let filas = sentencia.query_map([], mapear)?;
    let mut hosts = filas.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut etiquetas_por_host = etiquetas::por_host(conexion)?;
    for host in &mut hosts {
        if let Some(nombres) = etiquetas_por_host.remove(&host.id) {
            host.etiquetas = nombres;
        }
    }
    Ok(hosts)
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Host> {
    let mut host =
        conexion.query_row(&format!("{SELECCION} WHERE h.id = ?1"), params![id], mapear)?;
    host.etiquetas = etiquetas::de_host(conexion, id)?;
    Ok(host)
}

pub fn por_nombre(conexion: &Connection, nombre: &str) -> Result<Option<Host>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} WHERE h.nombre = ?1"))?;
    let mut filas = sentencia.query_map(params![nombre], mapear)?;
    match filas.next() {
        Some(host) => Ok(Some(host?)),
        None => Ok(None),
    }
}

pub fn existe_nombre(conexion: &Connection, nombre: &str, excluyendo: Option<i64>) -> Result<bool> {
    let total: i64 = match excluyendo {
        Some(id) => conexion.query_row(
            "SELECT COUNT(*) FROM HOSTS WHERE nombre = ?1 AND id <> ?2",
            params![nombre, id],
            |fila| fila.get(0),
        )?,
        None => conexion.query_row(
            "SELECT COUNT(*) FROM HOSTS WHERE nombre = ?1",
            params![nombre],
            |fila| fila.get(0),
        )?,
    };
    Ok(total > 0)
}

pub fn crear(conexion: &Connection, datos: &DatosHost, origen: Origen) -> Result<i64> {
    let ahora = fecha_ahora();
    let transaccion = conexion.unchecked_transaction()?;
    transaccion
        .execute(
            "INSERT INTO HOSTS (
                nombre, grupo_id, direccion, puerto, usuario, identidad_ref,
                salto_host_id, multiplexar, keepalive_seg, opciones_extra,
                origen, creado_en, actualizado_en
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
            params![
                datos.nombre.trim(),
                datos.grupo_id,
                datos.direccion.trim(),
                datos.puerto as i64,
                datos.usuario.as_deref().filter(|u| !u.is_empty()),
                datos.identidad_ref.a_bd(),
                datos.salto_host_id,
                datos.multiplexar as i64,
                datos.keepalive_seg.map(|valor| valor as i64),
                datos.opciones_extra,
                origen.como_texto(),
                ahora,
            ],
        )
        .map_err(|error| {
            if crate::almacen::es_conflicto_unico(&error) {
                anyhow::anyhow!("ya existe un host con el nombre «{}»", datos.nombre.trim())
            } else {
                anyhow::Error::new(error)
            }
        })?;
    let id = transaccion.last_insert_rowid();
    transaccion.commit()?;
    etiquetas::fijar_de_host(conexion, id, &datos.etiquetas)?;
    Ok(id)
}

pub fn actualizar(conexion: &Connection, id: i64, datos: &DatosHost) -> Result<()> {
    conexion
        .execute(
            "UPDATE HOSTS SET
                nombre = ?1, grupo_id = ?2, direccion = ?3, puerto = ?4,
                usuario = ?5, identidad_ref = ?6, salto_host_id = ?7,
                multiplexar = ?8, keepalive_seg = ?9, opciones_extra = ?10,
                actualizado_en = ?11
             WHERE id = ?12",
            params![
                datos.nombre.trim(),
                datos.grupo_id,
                datos.direccion.trim(),
                datos.puerto as i64,
                datos.usuario.as_deref().filter(|u| !u.is_empty()),
                datos.identidad_ref.a_bd(),
                datos.salto_host_id,
                datos.multiplexar as i64,
                datos.keepalive_seg.map(|valor| valor as i64),
                datos.opciones_extra,
                fecha_ahora(),
                id,
            ],
        )
        .map_err(|error| {
            if crate::almacen::es_conflicto_unico(&error) {
                anyhow::anyhow!("ya existe un host con el nombre «{}»", datos.nombre.trim())
            } else {
                anyhow::Error::new(error)
            }
        })?;
    etiquetas::fijar_de_host(conexion, id, &datos.etiquetas)?;
    Ok(())
}

pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute("DELETE FROM HOSTS WHERE id = ?1", params![id])?;
    etiquetas::limpiar_huerfanas(conexion)?;
    Ok(())
}

pub fn mover_a_grupo(conexion: &Connection, id: i64, grupo_id: Option<i64>) -> Result<()> {
    conexion.execute(
        "UPDATE HOSTS SET grupo_id = ?1, actualizado_en = ?2 WHERE id = ?3",
        params![grupo_id, fecha_ahora(), id],
    )?;
    Ok(())
}

pub fn marcar_origen(conexion: &Connection, id: i64, origen: Origen) -> Result<()> {
    conexion.execute(
        "UPDATE HOSTS SET origen = ?1, actualizado_en = ?2 WHERE id = ?3",
        params![origen.como_texto(), fecha_ahora(), id],
    )?;
    Ok(())
}

pub fn fijar_salto(conexion: &Connection, id: i64, salto_host_id: Option<i64>) -> Result<()> {
    conexion.execute(
        "UPDATE HOSTS SET salto_host_id = ?1, actualizado_en = ?2 WHERE id = ?3",
        params![salto_host_id, fecha_ahora(), id],
    )?;
    Ok(())
}

pub fn anadir_opciones_extra(conexion: &Connection, id: i64, linea: &str) -> Result<()> {
    let actuales: String = conexion.query_row(
        "SELECT opciones_extra FROM HOSTS WHERE id = ?1",
        params![id],
        |fila| fila.get(0),
    )?;
    let nuevas = if actuales.trim().is_empty() {
        linea.to_string()
    } else {
        format!("{}\n{}", actuales.trim_end(), linea)
    };
    conexion.execute(
        "UPDATE HOSTS SET opciones_extra = ?1, actualizado_en = ?2 WHERE id = ?3",
        params![nuevas, fecha_ahora(), id],
    )?;
    Ok(())
}

/// Cuántos hosts usan a este como salto.
pub fn dependientes_de_salto(conexion: &Connection, id: i64) -> Result<i64> {
    let total = conexion.query_row(
        "SELECT COUNT(*) FROM HOSTS WHERE salto_host_id = ?1",
        params![id],
        |fila| fila.get(0),
    )?;
    Ok(total)
}

/// Marca una conexión abierta con éxito.
pub fn marcar_conexion(conexion: &Connection, id: i64) -> Result<()> {
    let ahora = fecha_ahora();
    conexion.execute(
        "UPDATE HOSTS SET ultimo_estado = 'ok', ultima_conexion_en = ?1,
         actualizado_en = ?1 WHERE id = ?2",
        params![ahora, id],
    )?;
    Ok(())
}

/// Marca el resultado del último intento.
pub fn marcar_estado(conexion: &Connection, id: i64, estado: Option<UltimoEstado>) -> Result<()> {
    conexion.execute(
        "UPDATE HOSTS SET ultimo_estado = ?1, actualizado_en = ?2 WHERE id = ?3",
        params![estado.map(UltimoEstado::como_texto), fecha_ahora(), id],
    )?;
    Ok(())
}

pub fn contar(conexion: &Connection) -> Result<i64> {
    let total = conexion
        .query_row("SELECT COUNT(*) FROM HOSTS", [], |fila| fila.get(0))
        .context("contando hosts")?;
    Ok(total)
}
