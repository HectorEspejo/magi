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
    let uuid = resto.strip_suffix(EXTENSION_FICHERO)?;
    // Estricto: solo el uuid v4/v5 con guiones, sin sufijos de Syncthing ni
    // temporales.
    let uuid = uuid.parse::<uuid::Uuid>().ok()?;
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
        if !ruta.is_file() {
            continue;
        }
        let Some(nombre) = ruta.file_name().and_then(|nombre| nombre.to_str()) else {
            continue;
        };
        let Some(uuid) = uuid_de_nombre(nombre) else {
            continue;
        };
        let bytes = fs::read(&ruta).with_context(|| format!("leyendo {}", ruta.display()))?;
        ficheros.push(FicheroCarpeta {
            uuid,
            ruta,
            hash: cifrado::sha256_hex(&bytes),
        });
    }
    ficheros.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    Ok(ficheros)
}

/// Lee un fichero entero.
pub fn leer(ruta: &Path) -> Result<Vec<u8>> {
    fs::read(ruta).with_context(|| format!("leyendo {}", ruta.display()))
}

/// Escribe el fichero de un equipo de forma atómica y con permisos 600.
pub fn escribir(carpeta: &Path, uuid: &str, contenido: &[u8]) -> Result<PathBuf> {
    let ruta = carpeta.join(nombre_fichero(uuid));
    crate::ficheros::escribir_atomico(&ruta, contenido, 0o600)?;
    Ok(ruta)
}

/// Borra el fichero de un equipo. Solo se usa al quitar ese equipo.
pub fn borrar(carpeta: &Path, uuid: &str) -> Result<bool> {
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
    fs::create_dir_all(&directorio).with_context(|| format!("creando {}", directorio.display()))?;
    let ruta = directorio.join("sincronizacion.lock");
    let fichero = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&ruta)
        .with_context(|| format!("abriendo {}", ruta.display()))?;
    let limite = std::time::Instant::now() + espera.unwrap_or(Duration::ZERO);
    loop {
        match Flock::lock(fichero, FlockArg::LockExclusiveNonblock) {
            Ok(flock) => return Ok(Some(Cerrojo { _flock: flock })),
            Err((devuelto, _errno)) => {
                if std::time::Instant::now() >= limite {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(100));
                // El Flock devuelve el fichero: se reintenta con él.
                return intentar_de_nuevo(devuelto, espera, limite);
            }
        }
    }
}

fn intentar_de_nuevo(
    mut fichero: fs::File,
    espera: Option<Duration>,
    limite: std::time::Instant,
) -> Result<Option<Cerrojo>> {
    loop {
        match Flock::lock(fichero, FlockArg::LockExclusiveNonblock) {
            Ok(flock) => return Ok(Some(Cerrojo { _flock: flock })),
            Err((devuelto, _errno)) => {
                fichero = devuelto;
                if std::time::Instant::now() >= limite {
                    let _ = espera;
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
