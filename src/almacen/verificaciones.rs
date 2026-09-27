//! `VERIFICACIONES_HOST` (Fase 6): qué comprobaciones exige cada host antes de
//! una ejecución. Una fila por host, creada al activar la primera; al borrar
//! el host cae en cascada.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, Row};

use crate::deliberacion::{validar_verificaciones, DatosVerificaciones, Verificaciones};
use crate::modelo::fecha_ahora;

const SELECCION: &str = "SELECT host_id, salud, backup, backup_ruta, backup_patron, tests, \
     tests_comando, actualizado_en FROM VERIFICACIONES_HOST";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Verificaciones> {
    Ok(Verificaciones {
        host_id: fila.get(0)?,
        salud: fila.get::<_, i64>(1)? != 0,
        backup: fila.get::<_, i64>(2)? != 0,
        backup_ruta: fila.get(3)?,
        backup_patron: fila.get(4)?,
        tests: fila.get::<_, i64>(5)? != 0,
        tests_comando: fila.get(6)?,
        actualizado_en: fila.get(7)?,
    })
}

pub fn de_host(conexion: &Connection, host_id: i64) -> Result<Option<Verificaciones>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} WHERE host_id = ?1"))?;
    let mut filas = sentencia.query_map(params![host_id], mapear)?;
    match filas.next() {
        Some(fila) => Ok(Some(fila?)),
        None => Ok(None),
    }
}

/// Todas las verificaciones, por host.
pub fn por_host(conexion: &Connection) -> Result<HashMap<i64, Verificaciones>> {
    let mut sentencia = conexion.prepare(SELECCION)?;
    let filas = sentencia.query_map([], mapear)?;
    let mut mapa = HashMap::new();
    for fila in filas {
        let verificaciones = fila?;
        mapa.insert(verificaciones.host_id, verificaciones);
    }
    Ok(mapa)
}

/// Guarda lo que exige un host. Sin ninguna activa y sin fila previa no se
/// crea nada; con fila, se actualiza (conserva ruta, patrón y comando).
pub fn guardar(conexion: &Connection, host_id: i64, datos: &DatosVerificaciones) -> Result<()> {
    let datos = datos.clone().normalizada();
    validar_verificaciones(&datos).map_err(|motivo| anyhow!(motivo))?;
    if !datos.alguna_activa() && de_host(conexion, host_id)?.is_none() {
        return Ok(());
    }
    conexion.execute(
        "INSERT INTO VERIFICACIONES_HOST (host_id, salud, backup, backup_ruta, backup_patron, \
         tests, tests_comando, actualizado_en) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(host_id) DO UPDATE SET salud = excluded.salud, backup = excluded.backup, \
         backup_ruta = excluded.backup_ruta, backup_patron = excluded.backup_patron, \
         tests = excluded.tests, tests_comando = excluded.tests_comando, \
         actualizado_en = excluded.actualizado_en",
        params![
            host_id,
            datos.salud as i64,
            datos.backup as i64,
            datos.backup_ruta,
            datos.backup_patron,
            datos.tests as i64,
            datos.tests_comando,
            fecha_ahora()
        ],
    )?;
    Ok(())
}
