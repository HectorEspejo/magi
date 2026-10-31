//! Copias de seguridad de `magi.db` antes de importar el inventario (Fase 10).
//!
//! Se hacen con `VACUUM INTO` (una copia consistente sin cerrar la base) en
//! `~/.local/share/magi/copias/`, con permisos 600, y se conservan las últimas
//! `[sincronizacion] copias`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::config::Rutas;

/// `<datos>/copias`.
pub fn directorio(rutas: &Rutas) -> PathBuf {
    rutas.datos.join("copias")
}

/// Copia `magi.db` con `VACUUM INTO` y rota las copias antiguas. Devuelve la
/// ruta de la copia nueva.
pub fn copiar(conexion: &Connection, rutas: &Rutas, conservar: u64) -> Result<PathBuf> {
    let directorio = directorio(rutas);
    std::fs::create_dir_all(&directorio)
        .with_context(|| format!("creando {}", directorio.display()))?;
    crate::almacen::crear_directorio_privado_publico(&directorio)?;
    let marca = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut ruta = directorio.join(format!("magi-{marca}.db"));
    let mut sufijo = 2u32;
    while ruta.exists() {
        ruta = directorio.join(format!("magi-{marca}-{sufijo}.db"));
        sufijo += 1;
    }
    conexion
        .execute("VACUUM INTO ?1", params![ruta.to_string_lossy()])
        .with_context(|| format!("copiando la base de datos en {}", ruta.display()))?;
    crate::ficheros::asegurar_permisos(&ruta, 0o600)?;
    rotar(&directorio, conservar)?;
    Ok(ruta)
}

/// Lista de copias, de la más antigua a la más nueva (el nombre lleva la
/// fecha y ordena).
pub fn listar(directorio: &Path) -> Vec<PathBuf> {
    let mut copias: Vec<PathBuf> = std::fs::read_dir(directorio)
        .map(|entradas| {
            entradas
                .filter_map(|entrada| entrada.ok().map(|entrada| entrada.path()))
                .filter(|ruta| {
                    ruta.file_name()
                        .and_then(|nombre| nombre.to_str())
                        .map(|nombre| nombre.starts_with("magi-") && nombre.ends_with(".db"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    copias.sort();
    copias
}

fn rotar(directorio: &Path, conservar: u64) -> Result<()> {
    let conservar = conservar.max(1) as usize;
    let copias = listar(directorio);
    if copias.len() <= conservar {
        return Ok(());
    }
    for ruta in &copias[..copias.len() - conservar] {
        std::fs::remove_file(ruta)
            .with_context(|| format!("borrando la copia antigua {}", ruta.display()))?;
    }
    Ok(())
}
