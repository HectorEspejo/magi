//! La carpeta de sincronización que elige el usuario (Fase 17, §4.3 y §7.10).
//!
//! Fuera de Termux no cambia nada: se expande `~`, se exige que sea un
//! directorio y se usa lo que el usuario escribió, como en la Fase 10.
//!
//! En Termux la carpeta casi siempre cuelga del almacenamiento compartido de
//! Android, el único que ven las otras apps (Syncthing, por ejemplo):
//! `termux-setup-storage` crea `~/storage/shared`, un enlace a
//! `/storage/emulated/0`. Lo que se guarda en `config.toml` es la ruta ya
//! resuelta con `realpath` (`~/storage/shared/Sync/magi` →
//! `/storage/emulated/0/Sync/magi`), para que no dependa de un enlace que otra
//! `termux-setup-storage` puede rehacer, y se exige que exista y se pueda
//! escribir antes de seguir. Dentro de la carpeta valen las reglas de la
//! Fase 10: nada de enlaces simbólicos y solo nombres con el `uuid` canónico.

use std::path::{Path, PathBuf};

use crate::conexion::cliente::expandir_home;

/// Lo que hay que hacer cuando no hay `~/storage`.
pub const INSTRUCCION_ALMACENAMIENTO: &str = "ejecuta termux-setup-storage y vuelve";

/// Resuelve el texto que el usuario escribió en CARPETA DE SINCRONIZACIÓN y
/// devuelve la ruta que se usa (y se guarda). Con el motivo legible si no
/// vale.
///
/// `termux` lo decide quien llama (`termux::es_termux()`): aquí no se pregunta
/// por la plataforma.
pub fn resolver_carpeta(texto: &str, hogar: &Path, termux: bool) -> Result<PathBuf, String> {
    let ruta = expandir_home(texto.trim(), hogar);
    if !termux {
        return if ruta.is_dir() {
            Ok(ruta)
        } else {
            Err(no_existe(&ruta))
        };
    }
    resolver_en_termux(&ruta, hogar)
}

fn no_existe(ruta: &Path) -> String {
    format!(
        "la carpeta {} no existe o no es un directorio",
        ruta.display()
    )
}

fn resolver_en_termux(ruta: &Path, hogar: &Path) -> Result<PathBuf, String> {
    // Sin `~/storage` no hay almacenamiento compartido al que llegar: la
    // instrucción vale más que «no existe», que es lo que diría después.
    let cuelga_de_storage = ruta.starts_with(hogar.join("storage"));
    if cuelga_de_storage && !hay_almacenamiento(hogar) {
        return Err(format!("no hay ~/storage: {INSTRUCCION_ALMACENAMIENTO}"));
    }
    let real = match std::fs::canonicalize(ruta) {
        Ok(real) => real,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(no_existe(ruta)),
        // Android no deja leer el almacenamiento compartido (`~/storage/shared`
        // existe, o la ruta es `/storage/...` a pelo): el permiso de
        // almacenamiento no está concedido.
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                && (cuelga_de_storage || es_almacenamiento_de_android(ruta)) =>
        {
            return Err(format!(
                "sin permiso para leer {}: {INSTRUCCION_ALMACENAMIENTO}",
                ruta.display()
            ))
        }
        Err(error) => {
            return Err(format!("no se puede resolver {}: {error}", ruta.display()));
        }
    };
    if !real.is_dir() {
        return Err(no_existe(ruta));
    }
    crate::sincronizacion::equipos::comprobar_carpeta(&real)
        .map_err(|error| format!("{error:#}"))?;
    Ok(real)
}

/// ¿Es una ruta directa al almacenamiento compartido de Android
/// (`/storage/emulated/0/...` o `/sdcard/...`)?
fn es_almacenamiento_de_android(ruta: &Path) -> bool {
    ruta.starts_with("/storage") || ruta.starts_with("/sdcard")
}

/// ¿Existe `~/storage` (aunque sea un enlace)? Un error distinto de «no
/// existe» (permisos, por ejemplo) no lo da por ausente: ya lo dirá el paso
/// siguiente con su propio motivo.
fn hay_almacenamiento(hogar: &Path) -> bool {
    match std::fs::symlink_metadata(hogar.join("storage")) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}
