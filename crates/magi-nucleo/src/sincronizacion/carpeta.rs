//! Carpeta compartida de la sincronización (Fase 10): lectura y escritura
//! atómica de los ficheros magi-<uuid>.age y cerrojo entre procesos.
//!
//! Solo se leen nombres estrictos `magi-<uuid>` + `.age`: los temporales, las
//! copias en conflicto de las herramientas de sincronización
//! (magi-<uuid>.sync-conflict-...age) y cualquier otro fichero se ignoran.
//!
//! Aquí vive también el servicio inyectable `CarpetaSincronizacion` (Fase 11):
//! la pasada en modo lector no abre la carpeta por su cuenta, la recibe como
//! trait. El trait solo lista y lee, así que esa pasada no puede escribir en la
//! carpeta ni por error. `CarpetaDisco` lo implementa sobre el sistema de
//! ficheros (escritorio y pruebas); el móvil lo implementa sobre SAF.

use std::fs;
use std::io::Read as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

use crate::rutas::Rutas;
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

/// Tamaño máximo de un fichero de sincronización (T81): uno mayor se rechaza
/// sin cargarlo en memoria. El escritor y el lector comparten este tope.
pub const TAMANO_MAXIMO: u64 = 16 * 1024 * 1024;

/// Lo que una plataforma lista de la carpeta compartida. Es la entrada no
/// fiable que consume la pasada en modo lector (§7.3): además de esta lista,
/// la pasada vuelve a validar el nombre y el tamaño (T81), porque la
/// implementación puede devolver cualquier cosa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaCarpeta {
    /// Nombre del fichero, sin ruta.
    pub nombre: String,
    /// Tamaño en bytes.
    pub tamano: u64,
    /// Última modificación, en milisegundos desde la época. 0 si la
    /// plataforma no la da (SAF puede no darla).
    pub modificado_ms: i64,
}

/// La carpeta compartida como servicio inyectable: listar y leer, nada más.
///
/// La pasada en modo lector nunca escribe en la carpeta, así que el trait no
/// tiene métodos de escritura ni de borrado: lo que no existe en el trait no
/// se puede llamar. El escritorio y las pruebas lo implementan sobre el
/// sistema de ficheros (`CarpetaDisco`); el móvil, sobre SAF
/// (`DocumentsContract`).
pub trait CarpetaSincronizacion: Send + Sync {
    /// Lista las entradas de la carpeta. Una carpeta que no existe no es un
    /// error: devuelve una lista vacía.
    fn listar(&self) -> Result<Vec<EntradaCarpeta>, String>;

    /// Lee un fichero entero. Debe fallar sin leerlo si ocupa más de `maximo`
    /// bytes, y no debe seguir enlaces simbólicos (T81).
    fn leer(&self, nombre: &str, maximo: u64) -> Result<Vec<u8>, String>;
}

/// La carpeta compartida en el sistema de ficheros local (escritorio y
/// pruebas). Filtra las entradas con las mismas reglas que la pasada:
/// `magi-<uuid>.age` canónico, fichero normal (nunca un enlace) y tamaño
/// máximo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarpetaDisco {
    ruta: PathBuf,
}

impl CarpetaDisco {
    pub fn nueva(ruta: impl Into<PathBuf>) -> Self {
        Self { ruta: ruta.into() }
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }
}

impl CarpetaSincronizacion for CarpetaDisco {
    fn listar(&self) -> Result<Vec<EntradaCarpeta>, String> {
        let entradas = match fs::read_dir(&self.ruta) {
            Ok(entradas) => entradas,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("leyendo {}: {error}", self.ruta.display())),
        };
        let mut ficheros = Vec::new();
        for entrada in entradas {
            let entrada =
                entrada.map_err(|error| format!("leyendo {}: {error}", self.ruta.display()))?;
            // Metadatos sin seguir el enlace: un enlace simbólico no es un
            // fichero de equipo y no se lee.
            let Ok(metadatos) = fs::symlink_metadata(entrada.path()) else {
                continue;
            };
            if !metadatos.is_file() {
                continue;
            }
            let nombre_os = entrada.file_name();
            let Some(nombre) = nombre_os.to_str() else {
                continue;
            };
            if uuid_de_nombre(nombre).is_none() {
                continue;
            }
            // Un fichero por encima del tope no se lista: no es un fichero de
            // equipo legible (T81). La pasada lo vuelve a comprobar de todas
            // formas.
            if metadatos.len() > TAMANO_MAXIMO {
                continue;
            }
            let modificado_ms = metadatos
                .modified()
                .ok()
                .and_then(|fecha| fecha.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duracion| i64::try_from(duracion.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or(0);
            ficheros.push(EntradaCarpeta {
                nombre: nombre.to_string(),
                tamano: metadatos.len(),
                modificado_ms,
            });
        }
        ficheros.sort_by(|a, b| a.nombre.cmp(&b.nombre));
        Ok(ficheros)
    }

    fn leer(&self, nombre: &str, maximo: u64) -> Result<Vec<u8>, String> {
        // Solo un nombre canónico puede construir la ruta: nada de barras,
        // `..` ni sufijos con los que salirse de la carpeta.
        if uuid_de_nombre(nombre).is_none() {
            return Err(format!(
                "nombre de fichero de sincronización no válido: «{nombre}»"
            ));
        }
        let ruta = self.ruta.join(nombre);
        let fichero = fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&ruta)
            .map_err(|error| format!("abriendo {}: {error}", ruta.display()))?;
        let mut bytes = Vec::new();
        // Se pide un byte de más: si llega, el fichero supera el tope y se
        // descarta sin terminar de cargarlo en memoria.
        let mut lector = fichero.take(maximo.saturating_add(1));
        lector
            .read_to_end(&mut bytes)
            .map_err(|error| format!("leyendo {}: {error}", ruta.display()))?;
        if bytes.len() as u64 > maximo {
            return Err(format!(
                "el fichero {} ocupa más de {maximo} bytes (T81)",
                ruta.display()
            ));
        }
        Ok(bytes)
    }
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

    #[test]
    fn el_servicio_de_carpeta_lista_solo_ficheros_de_magi_y_ordenados() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        let uuid_a = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
        let uuid_b = "11111111-1111-4111-8111-111111111111";
        let uuid_c = "33333333-3333-4333-8333-333333333333";
        std::fs::write(carpeta.join(nombre_fichero(uuid_b)), b"bbbb").unwrap();
        std::fs::write(carpeta.join(nombre_fichero(uuid_a)), b"aaaaaa").unwrap();
        // Todo esto se ignora: copia en conflicto, temporal, mayúsculas,
        // subcarpeta con nombre canónico y un fichero ajeno.
        std::fs::write(
            carpeta.join(format!("magi-{uuid_a}.sync-conflict-20260101-000000.age")),
            b"x",
        )
        .unwrap();
        std::fs::write(carpeta.join(format!(".magi-{uuid_a}.age.tmp")), b"x").unwrap();
        let canonico = carpeta.join(nombre_fichero(uuid_a));
        let en_mayusculas = carpeta.join(format!("magi-{}.age", uuid_a.to_uppercase()));
        std::fs::write(&en_mayusculas, b"x").unwrap();
        // APFS (macOS por defecto) es insensible a mayúsculas: ese nombre es el
        // mismo fichero que el canónico y la escritura lo pisa. Se restaura y
        // se sigue; el caso del nombre en mayúsculas solo existe en sistemas
        // sensibles a mayúsculas.
        let insensible = std::fs::metadata(&canonico)
            .map(|meta| meta.len() != 6)
            .unwrap_or(false);
        if insensible {
            std::fs::write(&canonico, b"aaaaaa").unwrap();
        }
        std::fs::create_dir_all(carpeta.join(nombre_fichero(uuid_c))).unwrap();
        std::fs::write(carpeta.join("otra-cosa.txt"), b"x").unwrap();

        let servicio = CarpetaDisco::nueva(&carpeta);
        let entradas = servicio.listar().unwrap();
        assert_eq!(entradas.len(), 2);
        // Ordenadas por nombre: primero el uuid que empieza por 1.
        assert_eq!(entradas[0].nombre, nombre_fichero(uuid_b));
        assert_eq!(entradas[0].tamano, 4);
        assert_eq!(entradas[1].nombre, nombre_fichero(uuid_a));
        assert_eq!(entradas[1].tamano, 6);
        assert_eq!(
            servicio
                .leer(&nombre_fichero(uuid_a), TAMANO_MAXIMO)
                .unwrap(),
            b"aaaaaa"
        );
        assert!(servicio.leer(&nombre_fichero(uuid_a), 5).is_err());
        assert!(servicio
            .leer(
                &format!("magi-{uuid_a}.sync-conflict-20260101-000000.age"),
                TAMANO_MAXIMO
            )
            .is_err());
        assert!(servicio.leer("../fuera.age", TAMANO_MAXIMO).is_err());
        // Una carpeta que no existe no es un error: está vacía.
        assert!(CarpetaDisco::nueva(temporal.path().join("no-existe"))
            .listar()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn el_servicio_de_carpeta_no_sigue_enlaces_simbolicos() {
        use std::os::unix::fs::symlink;
        let temporal = tempfile::tempdir().unwrap();
        let fuera = temporal.path().join("fuera.age");
        std::fs::write(&fuera, b"contenido de fuera").unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        let nombre = nombre_fichero("2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b");
        symlink(&fuera, carpeta.join(&nombre)).unwrap();
        let servicio = CarpetaDisco::nueva(&carpeta);
        assert!(
            servicio.listar().unwrap().is_empty(),
            "el enlace no se lista"
        );
        assert!(servicio.leer(&nombre, TAMANO_MAXIMO).is_err(), "O_NOFOLLOW");
    }

    #[test]
    fn el_servicio_de_carpeta_rechaza_un_fichero_por_encima_del_maximo() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        let nombre = nombre_fichero("2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b");
        let fichero = std::fs::File::create(carpeta.join(&nombre)).unwrap();
        fichero.set_len(TAMANO_MAXIMO + 1).unwrap();
        let servicio = CarpetaDisco::nueva(&carpeta);
        assert!(
            servicio.listar().unwrap().is_empty(),
            "un fichero por encima del tope no se lista"
        );
        assert!(servicio.leer(&nombre, TAMANO_MAXIMO).is_err());
    }
}
