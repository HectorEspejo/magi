//! CRUD de SESIONES_PERSISTENTES (Fase 16, §3.1): las fichas de las sesiones
//! que viven en el tmux propio de MAGI en el host.
//!
//! Las fichas viajan con el inventario como elemento propio
//! `sesion_persistente` (D328): toda escritura de un campo que viaja sube la
//! versión con el reloj híbrido, una ficha nueva nace con uuid, versión y
//! vector, y un borrado deja lápida. Solo los hosts de tipo `ssh` tienen
//! fichas.
//!
//! Lo escribe el cliente, y solo cuando la sesión ya existe en el host (D341):
//! el servidor lee esta tabla y nunca escribe en ella. Borrar una ficha nunca
//! termina la sesión de tmux (el almacén ni siquiera sabe del host), y
//! renombrarla solo cambia la ficha: renombrar la sesión en el host es del
//! servidor, y va antes.
//!
//! La lectura valida cada fila (la pudo escribir otro proceso o llegar por la
//! sincronización) y salta, con un aviso en el log, las que no se pueden
//! interpretar: una fila corrupta no puede dejar sin listar las demás. El
//! aviso nunca lleva el comando ni la nota.

use anyhow::{anyhow, Error, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::almacen::versiones::{self, Tabla};
use crate::escritorio::TipoHost;
use crate::modelo::fecha_ahora;
use crate::sesiones_persistentes::validadores::{
    validar_datos, validar_host, validar_nombre, ErrorFicha,
};
use crate::sesiones_persistentes::{DatosFicha, Ficha, OrigenFicha};

const SELECCION: &str = "SELECT s.id, s.uuid, s.host_id, s.nombre, s.carpeta, s.comando, \
                         s.comando_reanudar, s.nota, s.origen, s.creada_por, s.creada_en, \
                         s.actualizado_en, h.tipo \
                           FROM SESIONES_PERSISTENTES s \
                           JOIN HOSTS h ON h.id = s.host_id";

/// Una fila de la consulta: la ficha, o por qué no se puede interpretar.
type FilaLeida = std::result::Result<Ficha, String>;

/// Columnas de una fila, en el orden de SELECCION. Una fila que no pasa la
/// validación no es un error de la consulta: se devuelve el motivo para
/// saltarla.
fn mapear(fila: &Row<'_>) -> rusqlite::Result<FilaLeida> {
    let id: i64 = fila.get(0)?;
    let uuid: Option<String> = fila.get(1)?;
    let host_id: i64 = fila.get(2)?;
    let origen: String = fila.get(8)?;
    let tipo_host: String = fila.get(12)?;
    let datos = DatosFicha {
        nombre: fila.get(3)?,
        carpeta: fila.get(4)?,
        comando: fila.get(5)?,
        comando_reanudar: fila.get(6)?,
        nota: fila.get(7)?,
        origen: OrigenFicha::desde_texto(&origen).unwrap_or(OrigenFicha::Manual),
        creada_por: fila.get(9)?,
    };
    let creada_en: String = fila.get(10)?;
    let actualizado_en: String = fila.get(11)?;

    let Some(uuid) = uuid.filter(|uuid| !uuid.trim().is_empty()) else {
        return Ok(Err(format!("la ficha {id} no tiene uuid")));
    };
    if TipoHost::desde_texto(&tipo_host) != Some(TipoHost::Ssh) {
        return Ok(Err(format!(
            "la ficha {id} cuelga de un host que no es ssh"
        )));
    }
    if OrigenFicha::desde_texto(&origen).is_none() {
        return Ok(Err(format!("la ficha {id} tiene un origen desconocido")));
    }
    // La misma validación que al escribir, con los campos recortados: una
    // ficha que llegó de otro equipo con una carpeta, un comando o una nota
    // fuera de los límites no se usa.
    let datos = match validar_datos(&datos) {
        Ok(datos) => datos,
        Err(error) => return Ok(Err(format!("la ficha {id} no es válida: {error}"))),
    };
    Ok(Ok(Ficha {
        id,
        uuid,
        host_id,
        nombre: datos.nombre,
        carpeta: datos.carpeta,
        comando: datos.comando,
        comando_reanudar: datos.comando_reanudar,
        nota: datos.nota,
        origen: datos.origen,
        creada_por: datos.creada_por,
        creada_en,
        actualizado_en,
    }))
}

/// Lee las fichas de una consulta, saltando (con un aviso) las ilegibles. Un
/// error real de SQLite sí se propaga.
fn consultar(
    conexion: &Connection,
    resto: &str,
    parametros: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Ficha>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} {resto}"))?;
    let filas = sentencia.query_map(parametros, mapear)?;
    let mut fichas = Vec::new();
    for fila in filas {
        match fila {
            Ok(Ok(ficha)) => fichas.push(ficha),
            Ok(Err(motivo)) => {
                tracing::warn!("ficha de sesión persistente ilegible, se salta: {motivo}")
            }
            Err(
                error @ (rusqlite::Error::FromSqlConversionFailure(..)
                | rusqlite::Error::InvalidColumnType(..)),
            ) => tracing::warn!("ficha de sesión persistente ilegible, se salta: {error}"),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(fichas)
}

/// Todas las fichas, ordenadas por host y nombre (el orden de la lista).
pub fn listar(conexion: &Connection) -> Result<Vec<Ficha>> {
    consultar(conexion, "ORDER BY h.nombre, s.nombre", &[])
}

/// Fichas de un host, por nombre.
pub fn de_host(conexion: &Connection, host_id: i64) -> Result<Vec<Ficha>> {
    consultar(
        conexion,
        "WHERE s.host_id = ?1 ORDER BY s.nombre",
        &[&host_id],
    )
}

/// Una ficha por su id. `None` si no existe o no se puede leer.
pub fn obtener(conexion: &Connection, id: i64) -> Result<Option<Ficha>> {
    Ok(consultar(conexion, "WHERE s.id = ?1", &[&id])?
        .into_iter()
        .next())
}

/// La ficha de un host con ese nombre exacto (tmux distingue mayúsculas y
/// casa prefijos en `-t`; aquí nunca).
pub fn por_nombre(conexion: &Connection, host_id: i64, nombre: &str) -> Result<Option<Ficha>> {
    Ok(consultar(
        conexion,
        "WHERE s.host_id = ?1 AND s.nombre = ?2",
        &[&host_id, &nombre],
    )?
    .into_iter()
    .next())
}

/// Nombres de todas las fichas de un host, también las que no se pueden leer
/// por otro motivo: siguen ocupando su nombre (`UNIQUE(host_id, nombre)`) y las
/// terminadas lo reservan para los nombres automáticos.
pub fn nombres_de_host(conexion: &Connection, host_id: i64) -> Result<Vec<String>> {
    let mut sentencia = conexion
        .prepare("SELECT nombre FROM SESIONES_PERSISTENTES WHERE host_id = ?1 ORDER BY nombre")?;
    let filas = sentencia.query_map(params![host_id], |fila| fila.get(0))?;
    Ok(filas.collect::<rusqlite::Result<Vec<String>>>()?)
}

/// Rechaza la ficha si el host no existe o no es `ssh` (T100). El error es un
/// `ErrorFicha` dentro del `anyhow::Error`, para que el diálogo lo reconozca.
pub(crate) fn comprobar_host_ssh(conexion: &Connection, host_id: i64) -> Result<()> {
    let tipo: Option<String> = conexion
        .query_row(
            "SELECT tipo FROM HOSTS WHERE id = ?1",
            params![host_id],
            |fila| fila.get(0),
        )
        .optional()?;
    let Some(tipo) = tipo else {
        return Err(Error::new(ErrorFicha::HostNoExiste { host_id }));
    };
    match TipoHost::desde_texto(&tipo) {
        Some(tipo) => validar_host(tipo).map_err(Error::new),
        None => Err(anyhow!(
            "el host {host_id} es de un tipo desconocido «{tipo}»"
        )),
    }
}

/// Crea la ficha de una sesión que ya existe en el host y devuelve su id.
///
/// Valida los datos, exige un host `ssh` y deja uuid, versión y vector de
/// este equipo. `creada_en` y `actualizado_en` son la hora de ahora.
pub fn crear(conexion: &Connection, host_id: i64, datos: &DatosFicha) -> Result<i64> {
    let datos = validar_datos(datos).map_err(Error::new)?;
    let transaccion = crate::almacen::transaccion(conexion)?;
    comprobar_host_ssh(&transaccion, host_id)?;
    let ahora = fecha_ahora();
    transaccion
        .execute(
            "INSERT INTO SESIONES_PERSISTENTES
                (host_id, nombre, carpeta, comando, comando_reanudar, nota, origen,
                 creada_por, creada_en, actualizado_en)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                host_id,
                datos.nombre,
                datos.carpeta,
                datos.comando,
                datos.comando_reanudar,
                datos.nota,
                datos.origen.texto(),
                datos.creada_por,
                ahora,
            ],
        )
        .map_err(|error| nombre_en_uso_o(error, &datos.nombre))?;
    let id = transaccion.last_insert_rowid();
    // Una ficha nueva nace con uuid, versión y vector (Fase 10).
    versiones::registrar_nuevo(&transaccion, Tabla::SesionesPersistentes, id, None)?;
    transaccion.commit()?;
    Ok(id)
}

/// Edita nombre, carpeta, comando, comando de reanudación y nota. Solo sube la
/// versión (y `actualizado_en`) si algo cambió. `origen` y `creada_por` se
/// fijan al crear y no se editan. Renombrar aquí solo cambia la ficha: el
/// nombre de la sesión en el host lo cambia el servidor, antes.
///
/// Compara lo editado antes y después en vez de pasar por
/// `versiones::subir_si_cambio`: así la versión sube igual con independencia
/// de cómo serialice la sincronización los datos de una ficha.
pub fn actualizar(conexion: &Connection, id: i64, datos: &DatosFicha) -> Result<()> {
    let datos = validar_datos(datos).map_err(Error::new)?;
    let transaccion = crate::almacen::transaccion(conexion)?;
    let Some(antes) = obtener(&transaccion, id)? else {
        return Err(Error::new(ErrorFicha::FichaNoExiste { id }));
    };
    if antes.nombre == datos.nombre
        && antes.carpeta == datos.carpeta
        && antes.comando == datos.comando
        && antes.comando_reanudar == datos.comando_reanudar
        && antes.nota == datos.nota
    {
        return Ok(());
    }
    transaccion
        .execute(
            "UPDATE SESIONES_PERSISTENTES
                SET nombre = ?1, carpeta = ?2, comando = ?3, comando_reanudar = ?4,
                    nota = ?5, actualizado_en = ?6
              WHERE id = ?7",
            params![
                datos.nombre,
                datos.carpeta,
                datos.comando,
                datos.comando_reanudar,
                datos.nota,
                fecha_ahora(),
                id,
            ],
        )
        .map_err(|error| nombre_en_uso_o(error, &datos.nombre))?;
    versiones::subir(&transaccion, Tabla::SesionesPersistentes, id)?;
    transaccion.commit()?;
    Ok(())
}

/// Cambia solo el nombre de la ficha (versión nueva). El de la sesión en el
/// host lo cambia el servidor con `RenombrarTmux`; si la sesión ya terminó,
/// solo se renombra la ficha. Mismo nombre: no hace nada.
pub fn renombrar(conexion: &Connection, id: i64, nombre: &str) -> Result<()> {
    let nombre = nombre.trim();
    validar_nombre(nombre).map_err(Error::new)?;
    let transaccion = crate::almacen::transaccion(conexion)?;
    let Some(antes) = obtener(&transaccion, id)? else {
        return Err(Error::new(ErrorFicha::FichaNoExiste { id }));
    };
    if antes.nombre == nombre {
        return Ok(());
    }
    transaccion
        .execute(
            "UPDATE SESIONES_PERSISTENTES SET nombre = ?1, actualizado_en = ?2 WHERE id = ?3",
            params![nombre, fecha_ahora(), id],
        )
        .map_err(|error| nombre_en_uso_o(error, nombre))?;
    versiones::subir(&transaccion, Tabla::SesionesPersistentes, id)?;
    transaccion.commit()?;
    Ok(())
}

/// Borra la ficha dejando su lápida. Nunca termina la sesión del host: si
/// vivía, sigue viva y sin ficha. Una fila sin uuid (que no debería existir)
/// se borra sin lápida: no puede viajar.
pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    let transaccion = crate::almacen::transaccion(conexion)?;
    let fila: Option<(Option<String>, Option<String>, String)> = transaccion
        .query_row(
            "SELECT uuid, vector, nombre FROM SESIONES_PERSISTENTES WHERE id = ?1",
            params![id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .optional()?;
    let Some((uuid, vector, nombre)) = fila else {
        // No hay fila que borrar: ni lápida ni cambios.
        return Ok(());
    };
    if let Some(uuid) = uuid.filter(|uuid| !uuid.trim().is_empty()) {
        crate::almacen::dejar_lapida(
            &transaccion,
            Tabla::SesionesPersistentes,
            &uuid,
            &nombre,
            vector.as_deref().unwrap_or("{}"),
        )?;
    }
    transaccion.execute(
        "DELETE FROM SESIONES_PERSISTENTES WHERE id = ?1",
        params![id],
    )?;
    transaccion.commit()?;
    Ok(())
}

/// Convierte el choque de `UNIQUE(host_id, nombre)` en un error que el
/// diálogo reconoce; cualquier otro error pasa tal cual.
fn nombre_en_uso_o(error: rusqlite::Error, nombre: &str) -> Error {
    if crate::almacen::es_conflicto_unico(&error) {
        Error::new(ErrorFicha::NombreEnUso {
            nombre: nombre.to_string(),
        })
    } else {
        Error::new(error)
    }
}
