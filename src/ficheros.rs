use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{Context, Result};

/// Escritura atómica (temporal en el mismo directorio + rename) con permisos
/// explícitos. Crea los directorios padre si faltan.
pub fn escribir_atomico(ruta: &Path, contenido: &[u8], modo: u32) -> Result<()> {
    let directorio = ruta
        .parent()
        .with_context(|| format!("la ruta {} no tiene directorio padre", ruta.display()))?;
    fs::create_dir_all(directorio).with_context(|| format!("creando {}", directorio.display()))?;
    let nombre = ruta
        .file_name()
        .and_then(|n| n.to_str())
        .context("nombre de fichero no válido")?;
    // Temporal único y creado con create_new: un enlace simbólico preparado
    // en el nombre del temporal no se sigue (create_new falla) y dos procesos
    // no se pisan. En la carpeta de sincronización esto importa: la carpeta la
    // comparte otro programa y el atacante no necesita la clave para plantar
    // un enlace.
    let marca = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duracion| duracion.as_nanos())
        .unwrap_or(0);
    let temporal = directorio.join(format!(".{nombre}.{}.{marca}.tmp", std::process::id()));
    {
        let mut fichero = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(modo)
            .open(&temporal)
            .with_context(|| format!("creando {}", temporal.display()))?;
        fichero
            .write_all(contenido)
            .with_context(|| format!("escribiendo {}", temporal.display()))?;
        fichero
            .sync_all()
            .with_context(|| format!("sincronizando {}", temporal.display()))?;
    }
    fs::set_permissions(&temporal, fs::Permissions::from_mode(modo))?;
    fs::rename(&temporal, ruta)
        .with_context(|| format!("renombrando {} → {}", temporal.display(), ruta.display()))?;
    if let Ok(dir) = fs::File::open(directorio) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Ajusta los permisos de un fichero existente.
pub fn asegurar_permisos(ruta: &Path, modo: u32) -> Result<()> {
    if ruta.exists() {
        fs::set_permissions(ruta, fs::Permissions::from_mode(modo))
            .with_context(|| format!("ajustando permisos de {}", ruta.display()))?;
    }
    Ok(())
}

/// Copia de seguridad de un fichero con sufijo de fecha. Devuelve la ruta de
/// la copia si se creó.
pub fn copia_con_fecha(ruta: &Path) -> Result<Option<std::path::PathBuf>> {
    if !ruta.exists() {
        return Ok(None);
    }
    let marca = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let nombre = ruta
        .file_name()
        .and_then(|n| n.to_str())
        .context("nombre de fichero no válido")?;
    // Dos copias en el mismo segundo no se pisan: se añade un sufijo.
    let mut copia = ruta.with_file_name(format!("{nombre}.bak-{marca}"));
    let mut sufijo = 2u32;
    while copia.exists() {
        copia = ruta.with_file_name(format!("{nombre}.bak-{marca}-{sufijo}"));
        sufijo += 1;
    }
    fs::copy(ruta, &copia)
        .with_context(|| format!("copiando {} → {}", ruta.display(), copia.display()))?;
    Ok(Some(copia))
}
