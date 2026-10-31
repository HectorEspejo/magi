//! Carpeta compartida de la sincronización (Fase 10): lectura y escritura
//! atómica de los ficheros magi-<uuid>.age y cerrojo entre procesos.
//!
//! Solo se leen nombres estrictos `magi-<uuid>` + `.age`: los temporales, las
//! copias en conflicto de las herramientas de sincronización
//! (magi-<uuid>.sync-conflict-...age) y cualquier otro fichero se ignoran.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

use crate::config::Rutas;
use crate::sincronizacion::cifrado;
use crate::sincronizacion::formato::{EXTENSION_FICHERO, PREFIJO_FICHERO};

/// Fichero de un equipo en la carpeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FicheroCarpeta {
    pub uuid: String,
    pub ruta: PathBuf,
    /// SHA-256 del fichero cifrado.
    pub hash: String,
}

/// Nombre canónico del fichero de un equipo.
pub fn nombre_fichero(uuid: &str) -> String {
    format!("{PREFIJO_FICHERO}{uuid}{EXTENSION_FICHERO}")
}

/// uuid de un nombre de fichero estricto; None para cualquier otra cosa.
pub fn uuid_de_nombre(nombre: &str) -> Option<String> {
    let resto = nombre.strip_prefix(PREFIJO_FICHERO)?;
    let datos = resto.strip_suffix(EXTENSION_FICHERO)?;
    // Estricto: solo el uuid v4/v5 con guiones, sin sufijos de Syncthing ni
    // temporales.
    let uuid = datos.parse::<uuid::Uuid>().ok()?;
    // Solo la forma canónica en minúsculas y con guiones: así no se aceptan
    // variantes (mayúsculas, sin guiones, con llaves, URN) que un tercero
    // podría usar para colar un nombre que no es el que escribe MAGI.
    if uuid.hyphenated().to_string() != datos {
        return None;
    }
    Some(uuid.to_string())
}

/// Lista los ficheros de equipo de la carpeta, con su hash. Una carpeta que no
/// existe no es un error: devuelve vacío.
pub fn listar(carpeta: &Path) -> Result<Vec<FicheroCarpeta>> {
    let entradas = match fs::read_dir(carpeta) {
        Ok(entradas) => entradas,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("leyendo {}", carpeta.display())),
    };
    let mut ficheros = Vec::new();
    for entrada in entradas {
        let entrada = entrada.with_context(|| format!("leyendo {}", carpeta.display()))?;
        let ruta = entrada.path();
        // Un enlace simbólico en la carpeta no se sigue: podría apuntar fuera y
        // leer/escribir un fichero que no es del usuario (la carpeta la
        // comparte la herramienta de sincronización, no hace falta la clave
        // para crear enlaces).
        let es_enlace = entrada
            .file_type()
            .map(|tipo| tipo.is_symlink())
            .unwrap_or(true);
        if es_enlace {
            continue;
        }
        let Some(nombre) = ruta.file_name().and_then(|nombre| nombre.to_str()) else {
            continue;
        };
        let Some(uuid) = uuid_de_nombre(nombre) else {
            continue;
        };
        let bytes =
            leer_sin_enlaces(&ruta).with_context(|| format!("leyendo {}", ruta.display()))?;
        ficheros.push(FicheroCarpeta {
            uuid,
            ruta,
            hash: cifrado::sha256_hex(&bytes),
        });
    }
    ficheros.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    Ok(ficheros)
}

/// Lee un fichero entero, sin seguir enlaces simbólicos.
pub fn leer(ruta: &Path) -> Result<Vec<u8>> {
    leer_sin_enlaces(ruta).with_context(|| format!("leyendo {}", ruta.display()))
}

fn leer_sin_enlaces(ruta: &Path) -> Result<Vec<u8>> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut opciones = fs::OpenOptions::new();
    opciones.read(true);
    // O_NOFOLLOW: si el nombre es un enlace, open falla en vez de seguirlo.
    opciones.custom_flags(nix::libc::O_NOFOLLOW);
    let mut fichero = opciones
        .open(ruta)
        .with_context(|| format!("abriendo {}", ruta.display()))?;
    let mut bytes = Vec::new();
    fichero
        .read_to_end(&mut bytes)
        .with_context(|| format!("leyendo {}", ruta.display()))?;
    Ok(bytes)
}

/// Comprueba que un uuid es canónico antes de construir una ruta con él.
fn uuid_canonico(uuid: &str) -> Result<()> {
    let canonico = uuid
        .parse::<uuid::Uuid>()
        .map(|parseado| parseado.hyphenated().to_string());
    if canonico.as_deref() != Ok(uuid) {
        anyhow::bail!("uuid no canónico en un fichero de sincronización: «{uuid}»");
    }
    Ok(())
}

/// Escribe el fichero de un equipo de forma atómica y con permisos 600.
pub fn escribir(carpeta: &Path, uuid: &str, contenido: &[u8]) -> Result<PathBuf> {
    uuid_canonico(uuid)?;
    let ruta = carpeta.join(nombre_fichero(uuid));
    crate::ficheros::escribir_atomico(&ruta, contenido, 0o600)?;
    Ok(ruta)
}

/// Borra el fichero de un equipo. Solo se usa al quitar ese equipo.
pub fn borrar(carpeta: &Path, uuid: &str) -> Result<bool> {
    uuid_canonico(uuid)?;
    let ruta = carpeta.join(nombre_fichero(uuid));
    match fs::remove_file(&ruta) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("borrando {}", ruta.display())),
    }
}

/// ¿La carpeta tiene algún fichero de MAGI? Decide entre crear y unir.
pub fn hay_ficheros(carpeta: &Path) -> Result<bool> {
    Ok(!listar(carpeta)?.is_empty())
}

/// Cerrojo entre procesos: solo uno sincroniza a la vez.
pub struct Cerrojo {
    _flock: Flock<fs::File>,
}

/// Intenta tomar el cerrojo. Con `espera` reintenta hasta ese plazo (para la
/// CLI); sin ella es no bloqueante. Devuelve None si otro proceso lo tiene.
pub fn adquirir_cerrojo(rutas: &Rutas, espera: Option<Duration>) -> Result<Option<Cerrojo>> {
    let directorio = rutas.dir_runtime();
    // 700: el cerrojo y el socket viven en un directorio del usuario; nadie
    // más debe poder crear el fichero del cerrojo por delante.
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    if directorio.exists() {
        let _ = fs::set_permissions(&directorio, std::fs::Permissions::from_mode(0o700));
    } else {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directorio)
            .with_context(|| format!("creando {}", directorio.display()))?;
    }
    let ruta = directorio.join("sincronizacion.lock");
    let fichero = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&ruta)
        .with_context(|| format!("abriendo {}", ruta.display()))?;
    let limite = std::time::Instant::now() + espera.unwrap_or(Duration::ZERO);
    let mut fichero = fichero;
    loop {
        match Flock::lock(fichero, FlockArg::LockExclusiveNonblock) {
            Ok(flock) => return Ok(Some(Cerrojo { _flock: flock })),
            Err((devuelto, _errno)) => {
                // El Flock devuelve el fichero: se reintenta con él.
                fichero = devuelto;
                if std::time::Instant::now() >= limite {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Fecha ISO 8601 local, para `generado_en`.
pub fn fecha_iso_ahora() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
        .to_string()
}

#[cfg(test)]
mod pruebas_tmp {
    use super::*;

    #[test]
    fn tmp_listar() {
        let dir = std::env::temp_dir().join(format!("magi-tmp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("magi-11111111-1111-4111-8111-111111111111.age"),
            b"x",
        )
        .unwrap();
        eprintln!(
            "uuid_de_nombre: {:?}",
            uuid_de_nombre("magi-11111111-1111-4111-8111-111111111111.age")
        );
        eprintln!(
            "file_type: {:?}",
            std::fs::read_dir(&dir)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .file_type()
        );
        eprintln!("listar: {:?}", listar(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_los_nombres_canonicos_cuentan() {
        let uuid = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
        assert_eq!(
            uuid_de_nombre(&format!("magi-{uuid}.age")),
            Some(uuid.to_string())
        );
        // Mayúsculas, sin guiones, URN o con sufijos: se ignoran.
        assert_eq!(
            uuid_de_nombre("magi-2F9A1C3E-4B5D-4E6F-8A7B-9C0D1E2F3A4B.age"),
            None
        );
        assert_eq!(
            uuid_de_nombre("magi-2f9a1c3e4b5d4e6f8a7b9c0d1e2f3a4b.age"),
            None
        );
        assert_eq!(
            uuid_de_nombre("magi-2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b.sync-conflict.age"),
            None
        );
        assert_eq!(
            uuid_de_nombre("magi-2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b.age.tmp"),
            None
        );
    }

    #[test]
    fn un_enlace_simbolico_en_la_carpeta_no_se_lee() {
        use std::os::unix::fs::symlink;
        let temporal = tempfile::tempdir().unwrap();
        let fuera = temporal.path().join("fuera.age");
        std::fs::write(&fuera, b"contenido de fuera").unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        symlink(
            &fuera,
            carpeta.join("magi-2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b.age"),
        )
        .unwrap();
        assert!(listar(&carpeta).unwrap().is_empty());
    }
}
