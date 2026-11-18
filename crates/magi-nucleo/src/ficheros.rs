use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{Context, Result};

/// Escritura atómica (temporal en el mismo directorio + rename) con permisos
/// explícitos. Crea los directorios padre si faltan. Si algo falla después de
/// crear el temporal, el temporal se borra: nunca queda basura junto al
/// destino.
pub fn escribir_atomico(ruta: &Path, contenido: &[u8], modo: u32) -> Result<()> {
    escribir_atomico_con(ruta, contenido, modo, false, ajustar_permisos).map(|_| ())
}

/// Como [`escribir_atomico`], pero tolera que el sistema de ficheros no admita
/// `chmod` (almacenamiento compartido de Android, FAT, SAF): si el `chmod`
/// falla, la escritura sigue y se devuelve `true`. Misma atomicidad y misma
/// limpieza del temporal; cualquier otro fallo sigue siendo un error.
pub fn escribir_atomico_tolerante(ruta: &Path, contenido: &[u8], modo: u32) -> Result<bool> {
    escribir_atomico_con(ruta, contenido, modo, true, ajustar_permisos)
}

/// El `chmod` de verdad: lo que usan todas las escrituras atómicas. Es público
/// solo para que quien inyecta otro (las pruebas de una escritura que no puede
/// aplicar permisos) pueda pasar el de siempre en las demás capas.
#[doc(hidden)]
pub fn ajustar_permisos(ruta: &Path, modo: u32) -> std::io::Result<()> {
    fs::set_permissions(ruta, fs::Permissions::from_mode(modo))
}

/// Borra el temporal al salir de ámbito salvo que se desarme: así todas las
/// ramas de error (escritura, sincronización, permisos, renombrado) limpian
/// igual y ninguna se olvida.
struct TemporalPendiente<'a> {
    ruta: &'a Path,
    armado: bool,
}

impl Drop for TemporalPendiente<'_> {
    fn drop(&mut self) {
        if self.armado {
            let _ = fs::remove_file(self.ruta);
        }
    }
}

/// Cuerpo común. Devuelve `true` si el `chmod` falló y se ignoró (solo en modo
/// tolerante). `ajustar` es el `chmod`, inyectable para probar esa rama: en
/// Linux no hay forma de que un `chmod` sobre un fichero propio falle sin un
/// sistema de ficheros como el almacenamiento compartido de Android, así que
/// las pruebas de las capas de arriba lo inyectan (superficie de pruebas).
#[doc(hidden)]
pub fn escribir_atomico_con(
    ruta: &Path,
    contenido: &[u8],
    modo: u32,
    tolerante: bool,
    ajustar: impl Fn(&Path, u32) -> std::io::Result<()>,
) -> Result<bool> {
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
    let mut fichero = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(modo)
        .open(&temporal)
        .with_context(|| format!("creando {}", temporal.display()))?;
    // Desde aquí el temporal existe: la guarda lo borra si algo falla.
    let mut pendiente = TemporalPendiente {
        ruta: &temporal,
        armado: true,
    };
    fichero
        .write_all(contenido)
        .with_context(|| format!("escribiendo {}", temporal.display()))?;
    fichero
        .sync_all()
        .with_context(|| format!("sincronizando {}", temporal.display()))?;
    drop(fichero);
    let mut permisos_ignorados = false;
    if let Err(error) = ajustar(&temporal, modo) {
        if tolerante {
            permisos_ignorados = true;
        } else {
            return Err(error)
                .with_context(|| format!("ajustando permisos de {}", temporal.display()));
        }
    }
    fs::rename(&temporal, ruta)
        .with_context(|| format!("renombrando {} → {}", temporal.display(), ruta.display()))?;
    pendiente.armado = false;
    if let Ok(dir) = fs::File::open(directorio) {
        let _ = dir.sync_all();
    }
    Ok(permisos_ignorados)
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

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    /// Nombres del directorio, para comprobar que no queda ningún temporal.
    fn nombres(directorio: &Path) -> Vec<String> {
        let mut nombres: Vec<String> = fs::read_dir(directorio)
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        nombres.sort();
        nombres
    }

    #[test]
    fn escribe_con_permisos_y_sin_dejar_temporales() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("a").join("dato.bin");
        escribir_atomico(&ruta, b"uno", 0o600).unwrap();
        assert_eq!(fs::read(&ruta).unwrap(), b"uno");
        assert_eq!(fs::metadata(&ruta).unwrap().mode() & 0o777, 0o600);
        // Sobrescribir reemplaza el contenido y tampoco deja nada.
        escribir_atomico(&ruta, b"dos", 0o600).unwrap();
        assert_eq!(fs::read(&ruta).unwrap(), b"dos");
        assert_eq!(nombres(&dir.path().join("a")), vec!["dato.bin"]);
    }

    #[test]
    fn si_el_rename_falla_no_queda_el_temporal() {
        let dir = tempfile::tempdir().unwrap();
        // El destino es un directorio: rename(fichero, directorio) falla.
        let destino = dir.path().join("destino");
        fs::create_dir(&destino).unwrap();
        let error = escribir_atomico(&destino, b"contenido", 0o600).unwrap_err();
        assert!(
            format!("{error:#}").contains("renombrando"),
            "el error dice qué falló: {error:#}"
        );
        assert_eq!(nombres(dir.path()), vec!["destino"], "sin temporales");
        assert!(destino.is_dir(), "el destino no se toca");
    }

    #[test]
    fn si_el_chmod_falla_se_borra_el_temporal_y_no_se_publica() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("dato.bin");
        let error = escribir_atomico_con(&ruta, b"x", 0o600, false, |_, _| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("ajustando permisos"));
        assert!(nombres(dir.path()).is_empty(), "ni destino ni temporal");
    }

    #[test]
    fn el_modo_tolerante_ignora_el_chmod_y_lo_dice() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("dato.bin");
        let ignorado = escribir_atomico_con(&ruta, b"x", 0o600, true, |_, _| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        })
        .unwrap();
        assert!(ignorado, "el chmod falló y se ignoró");
        assert_eq!(fs::read(&ruta).unwrap(), b"x");
        assert_eq!(nombres(dir.path()), vec!["dato.bin"], "sin temporales");
    }

    #[test]
    fn tolerante_con_chmod_correcto_devuelve_false() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("dato.bin");
        assert!(!escribir_atomico_tolerante(&ruta, b"a", 0o600).unwrap());
        assert_eq!(fs::metadata(&ruta).unwrap().mode() & 0o777, 0o600);
    }

    #[test]
    fn tolerante_tambien_limpia_si_el_rename_falla() {
        let dir = tempfile::tempdir().unwrap();
        let destino = dir.path().join("destino");
        fs::create_dir(&destino).unwrap();
        assert!(escribir_atomico_tolerante(&destino, b"c", 0o600).is_err());
        assert_eq!(nombres(dir.path()), vec!["destino"]);
    }
}
