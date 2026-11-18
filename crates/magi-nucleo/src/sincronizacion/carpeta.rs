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

impl CarpetaDisco {
    /// Cuerpo de [`CarpetaSincronizacion::listar`] con la lectura de metadatos
    /// inyectable (superficie de pruebas: es la única forma de comprobar que
    /// lo ajeno no se toca).
    ///
    /// El filtro de nombre va antes de los metadatos: la carpeta compartida
    /// puede tener miles de ficheros que no son de MAGI y, en un
    /// almacenamiento lento (FUSE), cada `stat` de más se paga en cada
    /// sondeo. Solo los nombres `magi-<uuid>.age` canónicos llegan a
    /// `leer_metadatos`.
    #[doc(hidden)]
    pub fn listar_con(
        &self,
        leer_metadatos: &dyn Fn(&Path) -> std::io::Result<fs::Metadata>,
    ) -> Result<Vec<EntradaCarpeta>, String> {
        let entradas = match fs::read_dir(&self.ruta) {
            Ok(entradas) => entradas,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("leyendo {}: {error}", self.ruta.display())),
        };
        let mut ficheros = Vec::new();
        for entrada in entradas {
            let entrada =
                entrada.map_err(|error| format!("leyendo {}: {error}", self.ruta.display()))?;
            let nombre_os = entrada.file_name();
            let Some(nombre) = nombre_os.to_str() else {
                continue;
            };
            if uuid_de_nombre(nombre).is_none() {
                continue;
            }
            // Metadatos sin seguir el enlace: un enlace simbólico no es un
            // fichero de equipo y no se lee.
            let Ok(metadatos) = leer_metadatos(&entrada.path()) else {
                continue;
            };
            if !metadatos.is_file() {
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
}

impl CarpetaSincronizacion for CarpetaDisco {
    fn listar(&self) -> Result<Vec<EntradaCarpeta>, String> {
        self.listar_con(&|ruta| fs::symlink_metadata(ruta))
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
        let fichero = abrir_fichero_normal(&ruta)
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

/// Lo que la pasada ve de la carpeta compartida: los ficheros de equipo y lo
/// que se dejó sin leer (con el motivo, para el aviso).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListadoCarpeta {
    pub ficheros: Vec<FicheroCarpeta>,
    /// Entradas con nombre de fichero de equipo que no se pudieron leer
    /// (permisos, error de entrada/salida): se omiten con su motivo en vez de
    /// tumbar el listado de todas las demás.
    pub omitidos: Vec<String>,
}

/// Lista los ficheros de equipo de la carpeta, con su hash. Una carpeta que no
/// existe no es un error: devuelve vacío.
pub fn listar(carpeta: &Path) -> Result<Vec<FicheroCarpeta>> {
    Ok(listar_con_omitidos(carpeta)?.ficheros)
}

/// Como [`listar`], pero dice también qué entradas con nombre de fichero de
/// equipo se omitieron por no poder leerse.
///
/// La carpeta es entrada no fiable (en Termux es almacenamiento compartido
/// donde escribe cualquier app con permiso): una entrada con nombre de MAGI
/// que no es un fichero normal (carpeta, FIFO, dispositivo, enlace) no cuenta;
/// una que supera [`TAMANO_MAXIMO`] se lista sin leerla, con un hash que no es
/// el de su contenido para que la pasada la dé por dañada; y una que no se
/// puede leer se omite con su motivo. Ninguna tumba el listado de las demás.
pub fn listar_con_omitidos(carpeta: &Path) -> Result<ListadoCarpeta> {
    let entradas = match fs::read_dir(carpeta) {
        Ok(entradas) => entradas,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ListadoCarpeta::default())
        }
        Err(error) => return Err(error).with_context(|| format!("leyendo {}", carpeta.display())),
    };
    let mut listado = ListadoCarpeta::default();
    for entrada in entradas {
        let entrada = match entrada {
            Ok(entrada) => entrada,
            Err(error) => {
                // Tras un error, `read_dir` termina: lo ya listado vale.
                listado
                    .omitidos
                    .push(format!("leyendo {}: {error}", carpeta.display()));
                continue;
            }
        };
        let ruta = entrada.path();
        let Some(nombre) = ruta.file_name().and_then(|nombre| nombre.to_str()) else {
            continue;
        };
        let Some(uuid) = uuid_de_nombre(nombre) else {
            continue;
        };
        // Metadatos sin seguir el enlace: un enlace simbólico en la carpeta no
        // se sigue (podría apuntar fuera y leer/escribir un fichero que no es
        // del usuario; la carpeta la comparte la herramienta de
        // sincronización, no hace falta la clave para crear enlaces) y solo un
        // fichero normal es un fichero de equipo.
        let metadatos = match entrada.metadata() {
            Ok(metadatos) => metadatos,
            // Desapareció entre el listado y la consulta.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                listado
                    .omitidos
                    .push(motivo_omitido(&ruta, &anyhow::Error::from(error)));
                continue;
            }
        };
        if !metadatos.is_file() {
            continue;
        }
        if metadatos.len() > TAMANO_MAXIMO {
            listado.ficheros.push(FicheroCarpeta {
                uuid,
                ruta,
                hash: hash_de_fichero_grande(metadatos.len()),
            });
            continue;
        }
        match leer_sin_enlaces(&ruta) {
            Ok(bytes) => listado.ficheros.push(FicheroCarpeta {
                uuid,
                ruta,
                hash: cifrado::sha256_hex(&bytes),
            }),
            // Creció por encima del tope mientras se leía: mismo trato.
            Err(error) if error.downcast_ref::<FicheroDemasiadoGrande>().is_some() => {
                listado.ficheros.push(FicheroCarpeta {
                    uuid,
                    ruta,
                    hash: hash_de_fichero_grande(TAMANO_MAXIMO + 1),
                });
            }
            Err(error) => listado.omitidos.push(motivo_omitido(&ruta, &error)),
        }
    }
    listado.ficheros.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    Ok(listado)
}

/// Texto del aviso de una entrada omitida.
fn motivo_omitido(ruta: &Path, error: &anyhow::Error) -> String {
    format!(
        "el fichero {} no se pudo leer y se omite: {error:#}",
        ruta.display()
    )
}

/// El «hash» de un fichero por encima del tope: no es el de su contenido (no
/// se lee), solo una marca estable que no coincide con ningún hash guardado.
/// La pasada lo ve demasiado grande al abrirlo y lo da por dañado.
fn hash_de_fichero_grande(tamano: u64) -> String {
    cifrado::sha256_hex(format!("magi:fichero-demasiado-grande:{tamano}").as_bytes())
}

/// Un fichero de equipo por encima de [`TAMANO_MAXIMO`].
#[derive(Debug)]
struct FicheroDemasiadoGrande;

impl std::fmt::Display for FicheroDemasiadoGrande {
    fn fmt(&self, formato: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formato,
            "ocupa más del máximo de {TAMANO_MAXIMO} bytes (T81)"
        )
    }
}

impl std::error::Error for FicheroDemasiadoGrande {}

/// Lee un fichero entero, sin seguir enlaces simbólicos. Falla sin leerlo si
/// no es un fichero normal o supera [`TAMANO_MAXIMO`].
pub fn leer(ruta: &Path) -> Result<Vec<u8>> {
    leer_sin_enlaces(ruta).with_context(|| format!("leyendo {}", ruta.display()))
}

/// Abre un fichero de la carpeta solo si es un fichero normal: sin seguir
/// enlaces (`O_NOFOLLOW`: si el nombre es un enlace, `open` falla en vez de
/// seguirlo) y sin quedarse esperando en un FIFO que haya sustituido al
/// fichero entre el listado y la apertura (`O_NONBLOCK`, que en un fichero
/// normal no hace nada).
fn abrir_fichero_normal(ruta: &Path) -> std::io::Result<fs::File> {
    let fichero = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(ruta)?;
    if !fichero.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no es un fichero normal",
        ));
    }
    Ok(fichero)
}

fn leer_sin_enlaces(ruta: &Path) -> Result<Vec<u8>> {
    let fichero =
        abrir_fichero_normal(ruta).with_context(|| format!("abriendo {}", ruta.display()))?;
    let tamano = fichero
        .metadata()
        .with_context(|| format!("consultando {}", ruta.display()))?
        .len();
    if tamano > TAMANO_MAXIMO {
        return Err(FicheroDemasiadoGrande.into());
    }
    let mut bytes = Vec::new();
    // Se pide un byte de más: si llega, el fichero creció por encima del tope
    // y se descarta sin terminar de cargarlo en memoria.
    fichero
        .take(TAMANO_MAXIMO + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("leyendo {}", ruta.display()))?;
    if bytes.len() as u64 > TAMANO_MAXIMO {
        return Err(FicheroDemasiadoGrande.into());
    }
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

/// Como [`escribir`], pero en una carpeta que puede no admitir `chmod` (el
/// almacenamiento compartido de Android): un 600 que no se puede aplicar no es
/// un error. Devuelve la ruta y si el `chmod` falló y se ignoró. El resto de
/// fallos (crear, escribir, renombrar) siguen siéndolo, y la escritura sigue
/// siendo atómica y sin temporales sueltos.
pub fn escribir_tolerante(carpeta: &Path, uuid: &str, contenido: &[u8]) -> Result<(PathBuf, bool)> {
    escribir_con_ajuste(
        carpeta,
        uuid,
        contenido,
        true,
        &crate::ficheros::ajustar_permisos,
    )
}

/// Cuerpo común de [`escribir`] y [`escribir_tolerante`], con el `chmod`
/// inyectable (superficie de pruebas: en Linux un `chmod` sobre un fichero
/// propio no falla).
#[doc(hidden)]
pub fn escribir_con_ajuste(
    carpeta: &Path,
    uuid: &str,
    contenido: &[u8],
    tolerante: bool,
    ajustar: &dyn Fn(&Path, u32) -> std::io::Result<()>,
) -> Result<(PathBuf, bool)> {
    uuid_canonico(uuid)?;
    let ruta = carpeta.join(nombre_fichero(uuid));
    let ignorado =
        crate::ficheros::escribir_atomico_con(&ruta, contenido, 0o600, tolerante, ajustar)?;
    Ok((ruta, ignorado))
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
    fn el_listado_solo_pide_metadatos_de_los_nombres_que_pueden_ser_de_magi() {
        use std::os::unix::fs::symlink;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        let uuid_a = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
        let uuid_b = "11111111-1111-4111-8111-111111111111";
        let uuid_c = "33333333-3333-4333-8333-333333333333";
        let uuid_d = "44444444-4444-4444-8444-444444444444";
        // Candidatos (su nombre es el de un fichero de equipo): un fichero
        // bueno, otro vacío, una carpeta y un enlace con nombre canónico.
        std::fs::write(carpeta.join(nombre_fichero(uuid_a)), b"aaaaaa").unwrap();
        std::fs::write(carpeta.join(nombre_fichero(uuid_b)), b"").unwrap();
        std::fs::create_dir_all(carpeta.join(nombre_fichero(uuid_c))).unwrap();
        symlink(
            temporal.path().join("no-existe.age"),
            carpeta.join(nombre_fichero(uuid_d)),
        )
        .unwrap();
        // Todo lo demás es ajeno: miles de ficheros de otra cosa en una
        // carpeta compartida, copias en conflicto y temporales.
        for numero in 0..200 {
            std::fs::write(carpeta.join(format!("foto-{numero}.jpg")), b"x").unwrap();
        }
        std::fs::write(
            carpeta.join(format!("magi-{uuid_a}.sync-conflict-20260101-000000.age")),
            b"x",
        )
        .unwrap();
        std::fs::write(carpeta.join(format!(".magi-{uuid_a}.age.tmp")), b"x").unwrap();
        std::fs::write(carpeta.join("notas.txt"), b"x").unwrap();

        let servicio = CarpetaDisco::nueva(&carpeta);
        let consultados = AtomicUsize::new(0);
        let entradas = servicio
            .listar_con(&|ruta| {
                consultados.fetch_add(1, Ordering::SeqCst);
                // Lo único que se consulta lleva nombre de fichero de equipo.
                let nombre = ruta.file_name().and_then(|nombre| nombre.to_str()).unwrap();
                assert!(
                    uuid_de_nombre(nombre).is_some(),
                    "se pidieron los metadatos de un fichero ajeno: {nombre}"
                );
                std::fs::symlink_metadata(ruta)
            })
            .unwrap();
        assert_eq!(
            consultados.load(Ordering::SeqCst),
            4,
            "un stat por candidato y ninguno por lo ajeno"
        );
        // Y el resultado es el de siempre: fichero bueno y vacío; ni la
        // carpeta ni el enlace.
        assert_eq!(
            entradas
                .iter()
                .map(|entrada| (entrada.nombre.clone(), entrada.tamano))
                .collect::<Vec<_>>(),
            vec![(nombre_fichero(uuid_b), 0), (nombre_fichero(uuid_a), 6)]
        );
        assert_eq!(servicio.listar().unwrap(), entradas);
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

    /// Ejecuta `cuerpo` en un hilo y falla si no termina en 20 s: un FIFO
    /// mal tratado dejaría el `open` esperando para siempre.
    fn sin_colgarse<T: Send + 'static>(cuerpo: impl FnOnce() -> T + Send + 'static) -> T {
        let (enviar, recibir) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = enviar.send(cuerpo());
        });
        recibir
            .recv_timeout(Duration::from_secs(20))
            .expect("la operación sobre la carpeta se quedó colgada")
    }

    fn hacer_fifo(ruta: &Path) {
        nix::unistd::mkfifo(ruta, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
    }

    const UUID_A: &str = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
    const UUID_B: &str = "11111111-1111-4111-8111-111111111111";
    const UUID_C: &str = "33333333-3333-4333-8333-333333333333";
    const UUID_D: &str = "44444444-4444-4444-8444-444444444444";

    #[test]
    fn el_listado_ignora_lo_que_con_nombre_de_magi_no_es_un_fichero_normal() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        std::fs::write(carpeta.join(nombre_fichero(UUID_A)), b"aaaaaa").unwrap();
        // Una carpeta, un FIFO y un enlace con nombre de fichero de equipo: en
        // el almacenamiento compartido de Termux escribe cualquier app.
        std::fs::create_dir_all(carpeta.join(nombre_fichero(UUID_B))).unwrap();
        hacer_fifo(&carpeta.join(nombre_fichero(UUID_C)));
        std::os::unix::fs::symlink(
            temporal.path().join("no-existe"),
            carpeta.join(nombre_fichero(UUID_D)),
        )
        .unwrap();

        let ruta = carpeta.clone();
        let listado = sin_colgarse(move || listar_con_omitidos(&ruta).unwrap());
        assert_eq!(
            listado
                .ficheros
                .iter()
                .map(|fichero| fichero.uuid.as_str())
                .collect::<Vec<_>>(),
            vec![UUID_A],
            "solo el fichero normal cuenta"
        );
        assert!(listado.omitidos.is_empty(), "{:?}", listado.omitidos);
        assert_eq!(listar(&carpeta).unwrap(), listado.ficheros);
        // Decidir entre crear y unir tampoco se tumba por ellos.
        assert!(hay_ficheros(&carpeta).unwrap());
        std::fs::remove_file(carpeta.join(nombre_fichero(UUID_A))).unwrap();
        assert!(
            !hay_ficheros(&carpeta).unwrap(),
            "una carpeta, un FIFO y un enlace no son ficheros de equipo"
        );
    }

    #[test]
    fn leer_un_fifo_o_una_carpeta_con_nombre_de_magi_falla_sin_colgarse() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        let fifo = carpeta.join(nombre_fichero(UUID_C));
        hacer_fifo(&fifo);
        let directorio = carpeta.join(nombre_fichero(UUID_B));
        std::fs::create_dir_all(&directorio).unwrap();
        // Un FIFO que sustituya al fichero entre el listado y la lectura no
        // deja el `open` esperando a un escritor que no va a llegar.
        let ruta = fifo.clone();
        assert!(sin_colgarse(move || leer(&ruta)).is_err());
        let ruta = directorio.clone();
        assert!(sin_colgarse(move || leer(&ruta)).is_err());
        let servicio = CarpetaDisco::nueva(&carpeta);
        assert!(
            sin_colgarse(move || servicio.leer(&nombre_fichero(UUID_C), TAMANO_MAXIMO)).is_err()
        );
    }

    #[test]
    fn un_fichero_disperso_por_encima_del_tope_se_lista_sin_leerlo() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        std::fs::write(carpeta.join(nombre_fichero(UUID_A)), b"aaaaaa").unwrap();
        // 300 MiB de ceros que no ocupan disco: si se leyera entero, el hash
        // sería el de su contenido y la memoria, la de 300 MiB.
        let grande = carpeta.join(nombre_fichero(UUID_B));
        let tamano = 300 * 1024 * 1024;
        std::fs::File::create(&grande)
            .unwrap()
            .set_len(tamano)
            .unwrap();

        let ruta = carpeta.clone();
        let listado = sin_colgarse(move || listar_con_omitidos(&ruta).unwrap());
        assert!(listado.omitidos.is_empty(), "{:?}", listado.omitidos);
        let uuids: Vec<&str> = listado
            .ficheros
            .iter()
            .map(|fichero| fichero.uuid.as_str())
            .collect();
        assert_eq!(uuids, vec![UUID_B, UUID_A], "se listan los dos");
        assert_eq!(
            listado.ficheros[0].hash,
            hash_de_fichero_grande(tamano),
            "el grande lleva la marca, no el hash de su contenido"
        );
        assert_eq!(
            listado.ficheros[1].hash,
            cifrado::sha256_hex(b"aaaaaa"),
            "el normal sigue llevando el hash de su contenido"
        );
        // Y nadie lo lee: ni `leer` ni el servicio de carpeta.
        let error = format!("{:#}", leer(&grande).unwrap_err());
        assert!(error.contains("máximo"), "{error}");
        assert!(CarpetaDisco::nueva(&carpeta)
            .leer(&nombre_fichero(UUID_B), TAMANO_MAXIMO)
            .is_err());
    }

    #[test]
    fn un_fichero_ilegible_se_omite_con_su_motivo_y_no_tumba_el_listado() {
        use std::os::unix::fs::PermissionsExt as _;
        if nix::unistd::Uid::current().is_root() {
            return; // root lee lo que quiera: la prueba no puede dar el permiso denegado
        }
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        std::fs::write(carpeta.join(nombre_fichero(UUID_A)), b"aaaaaa").unwrap();
        let ilegible = carpeta.join(nombre_fichero(UUID_B));
        std::fs::write(&ilegible, b"bbbb").unwrap();
        std::fs::set_permissions(&ilegible, std::fs::Permissions::from_mode(0o000)).unwrap();

        let listado =
            listar_con_omitidos(&carpeta).expect("un fichero ilegible no tumba el listado");
        assert_eq!(
            listado
                .ficheros
                .iter()
                .map(|fichero| fichero.uuid.as_str())
                .collect::<Vec<_>>(),
            vec![UUID_A],
            "el legible se lista"
        );
        assert_eq!(listado.omitidos.len(), 1, "{:?}", listado.omitidos);
        assert!(
            listado.omitidos[0].contains(&nombre_fichero(UUID_B)),
            "el aviso dice qué fichero: {}",
            listado.omitidos[0]
        );
        assert_eq!(listar(&carpeta).unwrap().len(), 1);
    }

    fn sin_chmod(_ruta: &Path, _modo: u32) -> std::io::Result<()> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }

    #[test]
    fn la_escritura_tolerante_solo_perdona_el_chmod() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path();
        let uuid = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
        // Estricta: el chmod que falla es un error y no queda ni el temporal.
        assert!(escribir_con_ajuste(carpeta, uuid, b"a", false, &sin_chmod).is_err());
        assert_eq!(std::fs::read_dir(carpeta).unwrap().count(), 0);
        // Tolerante: se escribe y se dice que se ignoró.
        let (ruta, ignorado) = escribir_con_ajuste(carpeta, uuid, b"b", true, &sin_chmod).unwrap();
        assert!(ignorado);
        assert_eq!(std::fs::read(&ruta).unwrap(), b"b");
        assert_eq!(std::fs::read_dir(carpeta).unwrap().count(), 1);
        // Con un chmod que funciona no hay nada que ignorar.
        let (_, ignorado) = escribir_tolerante(carpeta, uuid, b"c").unwrap();
        assert!(!ignorado);
        // Un uuid que no es canónico se rechaza igual de tolerante.
        assert!(escribir_tolerante(carpeta, "../fuera", b"x").is_err());
    }
}
