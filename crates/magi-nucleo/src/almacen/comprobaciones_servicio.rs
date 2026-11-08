//! CRUD de COMPROBACIONES_SERVICIO (Fase 13, §3.1): comprobaciones propias
//! de configuración que sustituyen a la comprobación conocida antes de
//! recargar o reiniciar un servicio o un proyecto Compose.
//!
//! Viajan con el inventario como elemento propio "comprobacion_servicio"
//! (D249): toda escritura de un campo que viaja sube la versión con el reloj
//! híbrido y un borrado deja lápida. Solo los hosts de tipo "ssh" tienen
//! comprobaciones.
//!
//! La validación del objetivo y de la orden es la de §7.11 y vive en
//! servicios::orden (la comparten el almacén, el servidor y el cliente):
//! validar_objetivo y validar_comando_propio.

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::almacen::versiones::{self, Tabla};
use crate::modelo::{fecha_ahora, ComprobacionServicio};
use crate::servicios::{Objetivo, TipoObjetivo};

const SELECCION: &str = "SELECT id, host_id, objetivo_tipo, objetivo, comando, \
                         actualizado_en, uuid, version, vector \
                           FROM COMPROBACIONES_SERVICIO";

/// Columnas de una fila, en el orden de SELECCION.
fn mapear(fila: &Row<'_>) -> rusqlite::Result<Option<ComprobacionServicio>> {
    let tipo_texto: String = fila.get(2)?;
    // Un objetivo que no sea unidad ni proyecto (o un tipo del protocolo de
    // servicios) no es una comprobación: la fila se salta, no se interpreta.
    let Some(tipo) = TipoObjetivo::desde_texto(&tipo_texto) else {
        return Ok(None);
    };
    if !matches!(tipo, TipoObjetivo::Unidad | TipoObjetivo::Proyecto) {
        return Ok(None);
    }
    Ok(Some(ComprobacionServicio {
        id: fila.get(0)?,
        host_id: fila.get(1)?,
        objetivo: Objetivo {
            tipo,
            nombre: fila.get(3)?,
        },
        comando: fila.get(4)?,
        actualizado_en: fila.get(5)?,
        uuid: texto_opcional(fila.get(6)?),
        version: texto_opcional(fila.get(7)?),
        vector: texto_opcional(fila.get(8)?),
    }))
}

fn texto_opcional(valor: Option<String>) -> Option<String> {
    valor.filter(|texto| !texto.trim().is_empty())
}

/// Todas las comprobaciones, ordenadas por host y objetivo.
pub fn listar(conexion: &Connection) -> Result<Vec<ComprobacionServicio>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} ORDER BY host_id, objetivo_tipo, objetivo"
    ))?;
    let filas = sentencia.query_map([], mapear)?;
    recoger(filas)
}

/// Comprobaciones de un host, ordenadas por tipo y objetivo.
pub fn de_host(conexion: &Connection, host_id: i64) -> Result<Vec<ComprobacionServicio>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE host_id = ?1 ORDER BY objetivo_tipo, objetivo"
    ))?;
    let filas = sentencia.query_map(params![host_id], mapear)?;
    recoger(filas)
}

/// La comprobación de un objetivo concreto de un host, si existe.
pub fn obtener(
    conexion: &Connection,
    host_id: i64,
    tipo: TipoObjetivo,
    objetivo: &str,
) -> Result<Option<ComprobacionServicio>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE host_id = ?1 AND objetivo_tipo = ?2 AND objetivo = ?3"
    ))?;
    let mut filas = sentencia.query_map(params![host_id, tipo.texto(), objetivo.trim()], mapear)?;
    match filas.next() {
        Some(fila) => Ok(fila?),
        None => Ok(None),
    }
}

/// Crea o actualiza (upsert por host, tipo y objetivo) la comprobación propia
/// de un objetivo. Devuelve su id.
///
/// Al crear deja uuid, versión y vector de este equipo; al editar sube la
/// versión con el reloj híbrido solo si la orden cambió. Un host que no existe
/// o que no es ssh se rechaza.
pub fn guardar(
    conexion: &Connection,
    host_id: i64,
    tipo: TipoObjetivo,
    objetivo: &str,
    comando: &str,
) -> Result<i64> {
    let objetivo = objetivo.trim();
    let comando = comando.trim();
    validar(tipo, objetivo, comando).map_err(|motivo| anyhow!(motivo))?;
    comprobar_host_ssh(conexion, host_id)?;
    let transaccion = crate::almacen::transaccion(conexion)?;
    let existente: Option<i64> = transaccion
        .query_row(
            "SELECT id FROM COMPROBACIONES_SERVICIO
              WHERE host_id = ?1 AND objetivo_tipo = ?2 AND objetivo = ?3",
            params![host_id, tipo.texto(), objetivo],
            |fila| fila.get(0),
        )
        .optional()?;
    let id = match existente {
        Some(id) => {
            let antes = versiones::datos_de(&transaccion, Tabla::ComprobacionesServicio, id)?;
            transaccion.execute(
                "UPDATE COMPROBACIONES_SERVICIO
                    SET comando = ?1, actualizado_en = ?2
                  WHERE id = ?3",
                params![comando, fecha_ahora(), id],
            )?;
            versiones::subir_si_cambio(
                &transaccion,
                Tabla::ComprobacionesServicio,
                id,
                antes.as_deref(),
            )?;
            id
        }
        None => {
            transaccion
                .execute(
                    "INSERT INTO COMPROBACIONES_SERVICIO
                        (host_id, objetivo_tipo, objetivo, comando, actualizado_en)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![host_id, tipo.texto(), objetivo, comando, fecha_ahora()],
                )
                .map_err(|error| {
                    if crate::almacen::es_conflicto_unico(&error) {
                        anyhow!("ya hay una comprobación para ese objetivo en este host")
                    } else {
                        anyhow::Error::new(error)
                    }
                })?;
            let id = transaccion.last_insert_rowid();
            // Una comprobación nueva nace con uuid, versión y vector (Fase 10).
            versiones::registrar_nuevo(&transaccion, Tabla::ComprobacionesServicio, id, None)?;
            id
        }
    };
    transaccion.commit()?;
    Ok(id)
}

/// Borra una comprobación dejando su lápida. Una fila sin uuid (que no debería
/// existir) se borra sin lápida: no puede viajar.
pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    let transaccion = crate::almacen::transaccion(conexion)?;
    let fila: Option<(Option<String>, Option<String>, String)> = transaccion
        .query_row(
            "SELECT uuid, vector, objetivo FROM COMPROBACIONES_SERVICIO WHERE id = ?1",
            params![id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .optional()?;
    let Some((uuid, vector, objetivo)) = fila else {
        return Ok(());
    };
    let Some(uuid) = uuid.filter(|uuid| !uuid.trim().is_empty()) else {
        transaccion.execute(
            "DELETE FROM COMPROBACIONES_SERVICIO WHERE id = ?1",
            params![id],
        )?;
        transaccion.commit()?;
        return Ok(());
    };
    crate::almacen::dejar_lapida(
        &transaccion,
        Tabla::ComprobacionesServicio,
        &uuid,
        &objetivo,
        vector.as_deref().unwrap_or("{}"),
    )?;
    transaccion.execute(
        "DELETE FROM COMPROBACIONES_SERVICIO WHERE id = ?1",
        params![id],
    )?;
    transaccion.commit()?;
    Ok(())
}

/// Valida una comprobación propia completa (§7.11): objetivo según su tipo y
/// orden de una sola línea. Usa los validadores compartidos de
/// servicios::orden. Devuelve el primer error legible.
pub fn validar(tipo: TipoObjetivo, objetivo: &str, comando: &str) -> Result<(), String> {
    match tipo {
        TipoObjetivo::Unidad | TipoObjetivo::Proyecto => {
            let objetivo = Objetivo {
                tipo,
                nombre: objetivo.trim().to_string(),
            };
            crate::servicios::orden::validar_objetivo(&objetivo)?;
        }
        otro => {
            return Err(format!(
                "un objetivo de tipo «{}» no admite comprobación propia",
                otro.texto()
            ))
        }
    }
    crate::servicios::orden::validar_comando_propio(comando)
}

/// Aviso si la orden empieza por sudo (§7.11): no hace falta, se ejecuta con
/// los privilegios de la acción; se acepta igualmente.
pub fn aviso_sudo(comando: &str) -> Option<String> {
    if comando.trim_start().starts_with("sudo") {
        Some(
            "no hace falta sudo: la comprobación se ejecuta con los privilegios de la acción"
                .to_string(),
        )
    } else {
        None
    }
}

/// Rechaza la comprobación si el host no existe o no es ssh (T100).
fn comprobar_host_ssh(conexion: &Connection, host_id: i64) -> Result<()> {
    let tipo: Option<String> = conexion
        .query_row(
            "SELECT tipo FROM HOSTS WHERE id = ?1",
            params![host_id],
            |fila| fila.get(0),
        )
        .optional()?;
    match tipo.as_deref() {
        None => Err(anyhow!("el host {host_id} no existe")),
        Some("ssh") => Ok(()),
        Some(tipo) => Err(anyhow!(
            "solo los hosts SSH tienen comprobaciones de servicio (este host es «{tipo}»)"
        )),
    }
}

/// Recoge las filas válidas y avisa de las que no se pueden interpretar: una
/// fila corrupta no puede dejar sin listar todas las demás.
#[allow(clippy::type_complexity)]
fn recoger(
    filas: rusqlite::MappedRows<
        '_,
        impl FnMut(&Row<'_>) -> rusqlite::Result<Option<ComprobacionServicio>>,
    >,
) -> Result<Vec<ComprobacionServicio>> {
    let mut comprobaciones = Vec::new();
    for fila in filas {
        match fila? {
            Some(comprobacion) => comprobaciones.push(comprobacion),
            None => tracing::warn!("comprobación de servicio ilegible, se salta"),
        }
    }
    Ok(comprobaciones)
}
