//! Equipos de la carpeta: crear, unir, quitar y rotar la clave (Fase 10,
//! §4.1, §4.3 y §7.4).
//!
//! La clave de sincronización vive solo en el llavero. Estas operaciones
//! necesitan la TUI o la CLI: el llavero y la carpeta son las fuentes de la
//! verdad.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::config::Rutas;
use crate::sincronizacion::cifrado::ClaveSincronizacion;
use crate::sincronizacion::formato::Documento;

/// Equipos quitados del universo: uuid → fecha del borrado (la más reciente).
pub fn equipos_quitados(documentos: &[Documento]) -> BTreeMap<String, String> {
    let mut quitados: BTreeMap<String, String> = BTreeMap::new();
    for documento in documentos {
        for quitado in &documento.equipos_quitados {
            let entrada = quitados
                .entry(quitado.uuid.clone())
                .or_insert_with(|| quitado.quitado_en.clone());
            if quitado.quitado_en > *entrada {
                *entrada = quitado.quitado_en.clone();
            }
        }
    }
    quitados
}

/// Crea la sincronización en una carpeta vacía: genera la clave, la guarda en
/// el llavero y devuelve el texto para mostrarla (la exportación la hace la
/// primera pasada). La carpeta se guarda en config.toml.
///
/// Implementación pendiente en la Fase 10.
pub fn crear(
    _conexion: &Connection,
    _rutas: &Rutas,
    _carpeta: &Path,
    _config: &mut crate::config::Config,
) -> Result<ClaveSincronizacion> {
    anyhow::bail!("crear la sincronización de la fase 10 aún no está implementado")
}

/// Une este equipo a una carpeta con ficheros de otros equipos, con la clave
/// pegada por el usuario. La valida descifrando y comprobando la firma de un
/// fichero; si no abre, no guarda nada.
///
/// Implementación pendiente en la Fase 10.
pub fn unir(
    _conexion: &Connection,
    _rutas: &Rutas,
    _carpeta: &Path,
    _texto_clave: &str,
    _config: &mut crate::config::Config,
) -> Result<ClaveSincronizacion> {
    anyhow::bail!("unir la sincronización de la fase 10 aún no está implementado")
}

/// Quita un equipo: confirmación, equipos_quitados, borra su fichero y rota
/// la clave. La anterior queda solo para leer.
///
/// Implementación pendiente en la Fase 10.
pub fn quitar(
    _conexion: &Connection,
    _rutas: &Rutas,
    _config: &mut crate::config::Config,
    _uuid_equipo: &str,
) -> Result<ClaveSincronizacion> {
    anyhow::bail!("quitar un equipo de la fase 10 aún no está implementado")
}

/// Rota la clave sin quitar ningún equipo.
///
/// Implementación pendiente en la Fase 10.
pub fn rotar(
    _conexion: &Connection,
    _rutas: &Rutas,
    _config: &mut crate::config::Config,
) -> Result<ClaveSincronizacion> {
    anyhow::bail!("rotar la clave de la fase 10 aún no está implementado")
}

/// Desactiva la sincronización en este equipo: borra su fichero y la clave del
/// llavero y quita carpeta de config.toml. magi.db queda intacta.
///
/// Implementación pendiente en la Fase 10.
pub fn desactivar(
    _conexion: &Connection,
    _rutas: &Rutas,
    _config: &mut crate::config::Config,
) -> Result<()> {
    anyhow::bail!("desactivar la sincronización de la fase 10 aún no está implementado")
}

/// Guarda en el llavero la clave de sincronización (y, al rotar, la anterior).
pub fn guardar_en_llavero(clave: &ClaveSincronizacion) -> Result<(), String> {
    let texto = clave.texto();
    crate::llavero::guardar_secreto(crate::llavero::CUENTA_SINCRONIZACION, texto.as_str())
}

/// Lee la clave actual del llavero, si la hay.
pub fn clave_del_llavero() -> Result<Option<ClaveSincronizacion>, String> {
    let Some(texto) = crate::llavero::recuperar_secreto(crate::llavero::CUENTA_SINCRONIZACION)?
    else {
        return Ok(None);
    };
    ClaveSincronizacion::desde_texto(texto.as_str())
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Lee la clave anterior (solo para leer tras una rotación), si la hay.
pub fn clave_anterior_del_llavero() -> Result<Option<ClaveSincronizacion>, String> {
    let Some(texto) =
        crate::llavero::recuperar_secreto(crate::llavero::CUENTA_SINCRONIZACION_ANTERIOR)?
    else {
        return Ok(None);
    };
    ClaveSincronizacion::desde_texto(texto.as_str())
        .map(Some)
        .map_err(|error| error.to_string())
}
