//! `DELIBERACIONES` (Fase 6): una fila por deliberación resuelta. La inserta
//! el cliente al decidir (aprobada, forzada o cancelada); el servidor solo
//! rellena `ejecucion_resultado` al terminar la ejecución, por su hilo
//! escritor (excepción documentada a T18). Las filas no se editan salvo ese
//! campo, y solo una vez.

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, Row};

use crate::deliberacion::{
    ComprobacionesHost, EjecucionResultado, NuevaDeliberacion, RegistroDeliberacion,
    ResultadoDeliberacion,
};
use crate::modelo::fecha_ahora;

const SELECCION: &str = "SELECT id, fecha, snippet_id, accion, hosts_json, comprobaciones_json, \
     resultado, bloqueada, motivo, usuario, ejecucion_resultado FROM DELIBERACIONES";

fn fallo(columna: usize, motivo: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(columna, rusqlite::types::Type::Text, motivo.into())
}

fn mapear(fila: &Row<'_>) -> rusqlite::Result<RegistroDeliberacion> {
    let hosts: String = fila.get(4)?;
    let hosts: Vec<(i64, String)> =
        serde_json::from_str(&hosts).map_err(|error| fallo(4, error.to_string()))?;
    let comprobaciones: String = fila.get(5)?;
    let comprobaciones: Vec<ComprobacionesHost> =
        serde_json::from_str(&comprobaciones).map_err(|error| fallo(5, error.to_string()))?;
    let resultado: String = fila.get(6)?;
    let resultado = ResultadoDeliberacion::desde_texto(&resultado)
        .ok_or_else(|| fallo(6, format!("resultado desconocido: «{resultado}»")))?;
    let ejecucion: Option<String> = fila.get(10)?;
    let ejecucion_resultado =
        match ejecucion {
            None => None,
            Some(texto) => Some(EjecucionResultado::desde_texto(&texto).ok_or_else(|| {
                fallo(10, format!("resultado de ejecución desconocido: «{texto}»"))
            })?),
        };
    Ok(RegistroDeliberacion {
        id: fila.get(0)?,
        fecha: fila.get(1)?,
        snippet_id: fila.get(2)?,
        accion: fila.get(3)?,
        hosts,
        comprobaciones,
        resultado,
        bloqueada: fila.get::<_, i64>(7)? != 0,
        motivo: fila.get(8)?,
        usuario: fila.get(9)?,
        ejecucion_resultado,
    })
}

/// Inserta una deliberación resuelta. Una forzada sin motivo no se guarda.
pub fn crear(conexion: &Connection, nueva: &NuevaDeliberacion) -> Result<i64> {
    let motivo = nueva
        .motivo
        .as_deref()
        .map(str::trim)
        .filter(|motivo| !motivo.is_empty());
    if nueva.resultado == ResultadoDeliberacion::Forzada && motivo.is_none() {
        return Err(anyhow!("una deliberación forzada necesita motivo"));
    }
    conexion.execute(
        "INSERT INTO DELIBERACIONES (fecha, snippet_id, accion, hosts_json, comprobaciones_json, \
         resultado, bloqueada, motivo, usuario, ejecucion_resultado) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
        params![
            fecha_ahora(),
            nueva.snippet_id,
            nueva.accion,
            serde_json::to_string(&nueva.hosts)?,
            serde_json::to_string(&nueva.comprobaciones)?,
            nueva.resultado.como_texto(),
            nueva.bloqueada as i64,
            motivo,
            nueva.usuario,
        ],
    )?;
    Ok(conexion.last_insert_rowid())
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<RegistroDeliberacion> {
    let deliberacion =
        conexion.query_row(&format!("{SELECCION} WHERE id = ?1"), params![id], mapear)?;
    Ok(deliberacion)
}

/// Rellena el resultado de la ejecución una sola vez; devuelve si lo escribió
/// (una segunda escritura, o una fila que ya no existe, no hace nada).
pub fn fijar_resultado_ejecucion(
    conexion: &Connection,
    id: i64,
    resultado: EjecucionResultado,
) -> Result<bool> {
    let cambiadas = conexion.execute(
        "UPDATE DELIBERACIONES SET ejecucion_resultado = ?1 \
         WHERE id = ?2 AND ejecucion_resultado IS NULL",
        params![resultado.como_texto(), id],
    )?;
    Ok(cambiadas == 1)
}
