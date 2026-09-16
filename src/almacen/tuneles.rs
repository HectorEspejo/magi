//! CRUD de `TUNELES`. Lo hace el cliente; el servidor de sesiones solo lee la
//! tabla con su conexión de solo lectura y ejecuta. El estado en vivo (activo,
//! caído, contadores) no se guarda aquí.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, Row};

use crate::modelo::{
    fecha_ahora, juntar_direccion_puerto, partir_direccion_puerto, validar_tunel, DatosTunel,
    TipoTunel, Tunel,
};

const SELECCION: &str =
    "SELECT t.id, t.host_id, h.nombre, t.nombre, t.tipo, t.escucha, t.destino, \
     t.automatico, t.creado_en, t.actualizado_en \
     FROM TUNELES t JOIN HOSTS h ON h.id = t.host_id";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Tunel> {
    let tipo: String = fila.get(4)?;
    // Un tipo desconocido no se interpreta como otro cualquiera: el servidor
    // ejecuta lo que lee, y tratar un remoto como local abriría un puerto en
    // esta máquina. Se avisa y esa fila no se usa.
    let Some(tipo) = TipoTunel::desde_texto(&tipo) else {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            format!("tipo de túnel desconocido: «{tipo}»").into(),
        ));
    };
    Ok(Tunel {
        id: fila.get(0)?,
        host_id: fila.get(1)?,
        host_nombre: fila.get(2)?,
        nombre: fila.get(3)?,
        tipo,
        escucha: fila.get(5)?,
        destino: fila.get(6)?,
        automatico: fila.get::<_, i64>(7)? != 0,
        creado_en: fila.get(8)?,
        actualizado_en: fila.get(9)?,
    })
}

/// Todas las filas leídas, saltando las que no se puedan interpretar: una fila
/// con un tipo desconocido (base tocada a mano o escrita por otra versión) no
/// puede dejar sin listar ni sin exportar todos los demás túneles.
fn recoger(
    filas: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<Tunel>>,
) -> Vec<Tunel> {
    let mut tuneles = Vec::new();
    for fila in filas {
        match fila {
            Ok(tunel) => tuneles.push(tunel),
            Err(error) => tracing::warn!("túnel ilegible, se salta: {error}"),
        }
    }
    tuneles
}

/// Todos los túneles, ordenados por host y nombre (el orden de la vista).
pub fn listar(conexion: &Connection) -> Result<Vec<Tunel>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} ORDER BY h.nombre, t.nombre"))?;
    let filas = sentencia.query_map([], mapear)?;
    Ok(recoger(filas))
}

/// Túneles de un host, por nombre.
pub fn de_host(conexion: &Connection, host_id: i64) -> Result<Vec<Tunel>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE t.host_id = ?1 ORDER BY t.nombre"
    ))?;
    let filas = sentencia.query_map(params![host_id], mapear)?;
    Ok(recoger(filas))
}

/// Túneles agrupados por host, para la exportación (una sola consulta).
pub fn por_host(conexion: &Connection) -> Result<HashMap<i64, Vec<Tunel>>> {
    let mut mapa: HashMap<i64, Vec<Tunel>> = HashMap::new();
    for tunel in listar(conexion)? {
        mapa.entry(tunel.host_id).or_default().push(tunel);
    }
    Ok(mapa)
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Tunel> {
    let tunel = conexion.query_row(&format!("{SELECCION} WHERE t.id = ?1"), params![id], mapear)?;
    Ok(tunel)
}

/// Túnel de un host por nombre. Lo usa la CLI (`magi tunel activar <host> <nombre>`).
pub fn por_nombre(conexion: &Connection, host_id: i64, nombre: &str) -> Result<Option<Tunel>> {
    let mut sentencia = conexion.prepare(&format!(
        "{SELECCION} WHERE t.host_id = ?1 AND t.nombre = ?2"
    ))?;
    let mut filas = sentencia.query_map(params![host_id, nombre.trim()], mapear)?;
    match filas.next() {
        Some(tunel) => Ok(Some(tunel?)),
        None => Ok(None),
    }
}

/// Nombre del túnel que choca con este tipo y esta escucha, si lo hay.
///
/// Dos túneles del mismo tipo con la misma escucha se pisan en la misma
/// máquina: en local y dinámico la escucha es la de MAGI (cualquier host), y
/// en remoto la del host (solo el mismo host).
pub fn duplicado(
    conexion: &Connection,
    tipo: TipoTunel,
    escucha: &str,
    host_id: i64,
    excluyendo: Option<i64>,
) -> Result<Option<String>> {
    // El puerto 0 es «el que quede libre»: dos túneles así no se pisan.
    if crate::modelo::partir_direccion_puerto(escucha).is_some_and(|(_, puerto)| puerto == 0) {
        return Ok(None);
    }
    let mismo_host = matches!(tipo, TipoTunel::Remoto);
    // `?4` es «solo choca dentro del mismo host» (los remotos): en local y
    // dinámico la escucha es de esta máquina, así que choca en cualquier host.
    let mut sentencia = conexion.prepare(
        "SELECT h.nombre, t.nombre FROM TUNELES t JOIN HOSTS h ON h.id = t.host_id \
         WHERE t.tipo = ?1 AND t.escucha = ?2 AND t.id <> ?3 AND (?4 = 0 OR t.host_id = ?5) \
         LIMIT 1",
    )?;
    let mut filas = sentencia.query_map(
        params![
            tipo.como_texto(),
            escucha,
            excluyendo.unwrap_or(-1),
            mismo_host as i64,
            host_id
        ],
        |fila| Ok((fila.get::<_, String>(0)?, fila.get::<_, String>(1)?)),
    )?;
    match filas.next() {
        Some(fila) => {
            let (host, nombre) = fila?;
            Ok(Some(format!("{host} · {nombre}")))
        }
        None => Ok(None),
    }
}

/// Crea un túnel. Valida los datos y rechaza el duplicado de escucha.
pub fn crear(conexion: &Connection, datos: &DatosTunel) -> Result<i64> {
    let (nombre, tipo, escucha, destino) = normalizar(datos)?;
    if let Some(choque) = duplicado(conexion, tipo, &escucha, datos.host_id, None)? {
        return Err(anyhow!(
            "ya hay un túnel {} escuchando en {escucha} ({choque})",
            tipo.etiqueta()
        ));
    }
    let ahora = fecha_ahora();
    conexion
        .execute(
            "INSERT INTO TUNELES (host_id, nombre, tipo, escucha, destino, automatico, creado_en, actualizado_en) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                datos.host_id,
                nombre,
                tipo.como_texto(),
                escucha,
                destino,
                datos.automatico as i64,
                ahora
            ],
        )
        .map_err(|error| {
            if crate::almacen::es_conflicto_unico(&error) {
                anyhow!("ya existe un túnel con el nombre «{nombre}» en ese host")
            } else {
                anyhow::Error::new(error)
            }
        })?;
    Ok(conexion.last_insert_rowid())
}

pub fn actualizar(conexion: &Connection, id: i64, datos: &DatosTunel) -> Result<()> {
    let (nombre, tipo, escucha, destino) = normalizar(datos)?;
    if let Some(choque) = duplicado(conexion, tipo, &escucha, datos.host_id, Some(id))? {
        return Err(anyhow!(
            "ya hay un túnel {} escuchando en {escucha} ({choque})",
            tipo.etiqueta()
        ));
    }
    conexion
        .execute(
            "UPDATE TUNELES SET host_id = ?1, nombre = ?2, tipo = ?3, escucha = ?4, destino = ?5, \
             automatico = ?6, actualizado_en = ?7 WHERE id = ?8",
            params![
                datos.host_id,
                nombre,
                tipo.como_texto(),
                escucha,
                destino,
                datos.automatico as i64,
                fecha_ahora(),
                id
            ],
        )
        .map_err(|error| {
            if crate::almacen::es_conflicto_unico(&error) {
                anyhow!("ya existe un túnel con el nombre «{nombre}» en ese host")
            } else {
                anyhow::Error::new(error)
            }
        })?;
    Ok(())
}

pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute("DELETE FROM TUNELES WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn alternar_automatico(conexion: &Connection, id: i64, automatico: bool) -> Result<()> {
    conexion.execute(
        "UPDATE TUNELES SET automatico = ?1, actualizado_en = ?2 WHERE id = ?3",
        params![automatico as i64, fecha_ahora(), id],
    )?;
    Ok(())
}

/// Valida y deja los campos en su forma canónica: nombre recortado, escucha
/// `dirección:puerto` y destino solo donde toca.
fn normalizar(datos: &DatosTunel) -> Result<(String, TipoTunel, String, Option<String>)> {
    validar_tunel(datos).map_err(|motivo| anyhow!(motivo))?;
    let nombre = datos.nombre.trim().to_string();
    let escucha = canonica(&datos.escucha).expect("la escucha ya está validada");
    let destino = if datos.tipo.lleva_destino() {
        Some(canonica(datos.destino.as_deref().unwrap_or_default()).expect("destino validado"))
    } else {
        None
    };
    Ok((nombre, datos.tipo, escucha, destino))
}

/// `dirección:puerto` en la forma que se guarda y se compara.
pub fn canonica(texto: &str) -> Option<String> {
    let (direccion, puerto) = partir_direccion_puerto(texto)?;
    Some(juntar_direccion_puerto(&direccion, puerto))
}
