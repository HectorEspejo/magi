//! Huellas de known_hosts que viajan con el inventario (Fase 10, §4.4).
//!
//! Al exportar, las claves registradas de cada host del inventario. Al
//! importar, una huella se añade solo si este equipo no tiene ninguna para ese
//! host; si tiene otra, nunca se sustituye sola: queda un conflicto de tipo
//! huella. Se reutiliza la lectura de conexion/huellas.rs, que entiende las
//! entradas hasheadas.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use russh::keys::known_hosts::known_host_keys_path;

use crate::sincronizacion::formato::HuellaDocumento;
use crate::sincronizacion::fusion::ConflictoFusion;

/// Clave pública en base64, sin el tipo ni el comentario.
pub fn clave_base64(clave: &russh::keys::PublicKey) -> Result<String> {
    let linea = clave
        .to_openssh()
        .context("serializando una clave pública")?;
    Ok(linea
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string())
}

/// Fecha en que este equipo aceptó la última huella del host: la última
/// anotación de REGISTRO de tipo huella o, si no hay, la última conexión.
pub fn aceptada_en(conexion: &Connection, host_id: i64) -> Result<String> {
    let fecha: Option<String> = conexion
        .query_row(
            "SELECT fecha FROM REGISTRO
              WHERE host_id = ?1 AND tipo IN ('huella_aceptada', 'huella_sustituida')
              ORDER BY id DESC LIMIT 1",
            params![host_id],
            |fila| fila.get(0),
        )
        .ok();
    if let Some(fecha) = fecha {
        return Ok(fecha);
    }
    Ok(conexion
        .query_row(
            "SELECT COALESCE(ultima_conexion_en, '') FROM HOSTS WHERE id = ?1",
            params![host_id],
            |fila| fila.get(0),
        )
        .unwrap_or_default())
}

/// Exporta las huellas de todos los hosts del inventario.
pub fn exportar(conexion: &Connection, ruta_known_hosts: &Path) -> Result<Vec<HuellaDocumento>> {
    let mut sentencia = conexion.prepare(
        "SELECT id, uuid, direccion, puerto FROM HOSTS WHERE uuid IS NOT NULL ORDER BY id",
    )?;
    let filas = sentencia.query_map([], |fila| {
        Ok((
            fila.get::<_, i64>(0)?,
            fila.get::<_, String>(1)?,
            fila.get::<_, String>(2)?,
            fila.get::<_, i64>(3)? as u16,
        ))
    })?;
    let hosts: Vec<(i64, String, String, u16)> = filas.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut huellas = Vec::new();
    for (id, uuid, direccion, puerto) in hosts {
        let aceptada = aceptada_en(conexion, id)?;
        let entradas = known_host_keys_path(&direccion, puerto, ruta_known_hosts)
            .with_context(|| format!("leyendo {}", ruta_known_hosts.display()))?;
        for (_, clave) in entradas {
            huellas.push(HuellaDocumento {
                host: uuid.clone(),
                tipo: crate::conexion::huellas::tipo_clave(&clave),
                clave: clave_base64(&clave)?,
                aceptada_en: aceptada.clone(),
            });
        }
    }
    Ok(huellas)
}

/// Lo que hay que hacer al importar huellas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultadoHuellas {
    /// Hosts a los que se añadió su primera huella.
    pub anadidas: Vec<String>,
    /// Conflictos de tipo huella.
    pub conflictos: Vec<ConflictoFusion>,
    /// Avisos legibles (hosts inexistentes, ficheros que no se pueden leer).
    pub avisos: Vec<String>,
}

/// Importa las huellas de un documento: añade las que este equipo no tiene y
/// deja conflicto si ya había otra distinta. No toca known_hosts salvo para
/// añadir. `equipos` mapea uuid del equipo → (nombre, generado_en).
pub fn importar(
    _conexion: &Connection,
    _ruta_known_hosts: &Path,
    _entrantes: &[HuellaDocumento],
    _equipos: &BTreeMap<String, (String, String)>,
) -> Result<ResultadoHuellas> {
    anyhow::bail!("la importación de huellas de la fase 10 aún no está implementada")
}

/// Acepta la huella de otro equipo para un host: sustituye la de este equipo,
/// con copia previa de known_hosts.
pub fn aceptar(
    _conexion: &Connection,
    _ruta_known_hosts: &Path,
    _host_uuid: &str,
    _tipo: &str,
    _clave: &str,
) -> Result<()> {
    anyhow::bail!("aceptar una huella de la fase 10 aún no está implementado")
}
