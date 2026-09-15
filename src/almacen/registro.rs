use anyhow::Result;
use rusqlite::{params, Connection, Row};

use crate::modelo::{EntradaRegistro, ResultadoRegistro};
use crate::registro::FiltroRegistro;

const SELECCION: &str = "
    SELECT r.id, r.fecha, r.tipo, r.host_id, h.nombre, r.identidad_id,
           i.alias, r.detalle, r.resultado
      FROM REGISTRO r
      LEFT JOIN HOSTS h ON h.id = r.host_id
      LEFT JOIN IDENTIDADES i ON i.id = r.identidad_id";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<EntradaRegistro> {
    Ok(EntradaRegistro {
        id: fila.get(0)?,
        fecha: fila.get(1)?,
        tipo: fila.get(2)?,
        host_id: fila.get(3)?,
        host_nombre: fila.get(4)?,
        identidad_id: fila.get(5)?,
        identidad_alias: fila.get(6)?,
        detalle: fila.get(7)?,
        resultado: ResultadoRegistro::desde_texto(&fila.get::<_, String>(8)?),
    })
}

/// Cláusula `WHERE` y parámetros del filtro de la vista Registro.
fn condiciones(filtro: &FiltroRegistro) -> (String, Vec<String>) {
    let mut condiciones = Vec::new();
    let mut parametros = Vec::new();
    if !filtro.tipos.is_empty() {
        let marcadores: Vec<String> = filtro
            .tipos
            .iter()
            .enumerate()
            .map(|(indice, _)| format!("?{}", parametros.len() + indice + 1))
            .collect();
        condiciones.push(format!("r.tipo IN ({})", marcadores.join(", ")));
        parametros.extend(filtro.tipos.iter().map(|tipo| tipo.to_string()));
    }
    let texto = filtro.texto.trim();
    if !texto.is_empty() {
        let marcador = format!("?{}", parametros.len() + 1);
        condiciones.push(format!(
            "(r.tipo LIKE {marcador} OR r.detalle LIKE {marcador} OR h.nombre LIKE {marcador})"
        ));
        parametros.push(format!("%{texto}%"));
    }
    if condiciones.is_empty() {
        (String::new(), parametros)
    } else {
        (format!(" WHERE {}", condiciones.join(" AND ")), parametros)
    }
}

pub fn contar(conexion: &Connection, filtro: &FiltroRegistro) -> Result<i64> {
    let (where_sql, parametros) = condiciones(filtro);
    let sql = format!(
        "SELECT COUNT(*)
           FROM REGISTRO r
           LEFT JOIN HOSTS h ON h.id = r.host_id{where_sql}"
    );
    let total = conexion.query_row(&sql, rusqlite::params_from_iter(parametros), |fila| {
        fila.get(0)
    })?;
    Ok(total)
}

/// Página del registro, de más reciente a más antiguo.
pub fn listar(
    conexion: &Connection,
    filtro: &FiltroRegistro,
    limite: i64,
    desplazamiento: i64,
) -> Result<Vec<EntradaRegistro>> {
    let (where_sql, mut parametros) = condiciones(filtro);
    let sql = format!(
        "{SELECCION}{where_sql} ORDER BY r.id DESC LIMIT ?{} OFFSET ?{}",
        parametros.len() + 1,
        parametros.len() + 2
    );
    parametros.push(limite.to_string());
    parametros.push(desplazamiento.to_string());
    let mut sentencia = conexion.prepare(&sql)?;
    let filas = sentencia.query_map(rusqlite::params_from_iter(parametros), mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Todas las entradas (sin paginar) para la exportación, con filtro `desde`.
pub fn listar_todas(conexion: &Connection, desde: Option<&str>) -> Result<Vec<EntradaRegistro>> {
    let (sql, parametros) = match desde {
        Some(desde) => (
            format!("{SELECCION} WHERE r.fecha >= ?1 ORDER BY r.id DESC"),
            vec![desde.to_string()],
        ),
        None => (format!("{SELECCION} ORDER BY r.id DESC"), Vec::new()),
    };
    let mut sentencia = conexion.prepare(&sql)?;
    let filas = sentencia.query_map(rusqlite::params_from_iter(parametros), mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Borra las entradas anteriores a la fecha indicada. Devuelve las borradas.
pub fn purgar_anteriores(conexion: &Connection, fecha_corte: &str) -> Result<usize> {
    let borradas = conexion.execute(
        "DELETE FROM REGISTRO WHERE fecha < ?1",
        params![fecha_corte],
    )?;
    Ok(borradas)
}

pub fn contar_anteriores(conexion: &Connection, fecha_corte: &str) -> Result<i64> {
    let total = conexion.query_row(
        "SELECT COUNT(*) FROM REGISTRO WHERE fecha < ?1",
        params![fecha_corte],
        |fila| fila.get(0),
    )?;
    Ok(total)
}
