//! Llavero de fichero con frase maestra (Fase 17, Termux): `llavero.age`.
//!
//! Detrás de `LlaveroSecreto` y `LlaveroHost`, como el llavero del sistema.
//! El fichero es un `age` con una sola entrada scrypt (2^17 al crear, hasta
//! 2^20 al abrir) y dentro un JSON `{"formato": 1, "entradas": {…}}`. La frase
//! maestra solo vive en la memoria de este objeto (`Zeroizing`, NFC): nunca va
//! a disco, al log, a REGISTRO ni al servidor. El núcleo no lee el entorno:
//! la ruta se inyecta.
//!
//! Todo lo que gasta scrypt (`crear`, `desbloquear`, `cambiar_frase` y los
//! `guardar` que reescriben el fichero) bloquea el hilo ~1 s en un móvil:
//! quien lo llame desde una interfaz lo hace en `spawn_blocking`.
//!
//! Dos cerrojos, para que la interfaz nunca espere a scrypt: `interno` guarda
//! el estado (frase, entradas, serie de fallos) y se toma solo un instante,
//! así que `estado` y `recuperar` son baratos aunque otro hilo esté
//! descifrando; `operacion` serializa lo lento (`crear`, `desbloquear`,
//! `guardar`…) dentro de este objeto, y el `flock` de `<ruta>.lock` lo
//! serializa entre ventanas (procesos) y entre objetos que comparten ruta.
//!
//! Distinguir una frase incorrecta de un fichero dañado: la entrada scrypt
//! autentica la clave de fichero, así que si no descifra es la frase (o una
//! entrada estropeada, que es lo mismo desde fuera) y el fichero no se toca;
//! si descifra y lo demás falla (MAC de la cabecera, el flujo, el JSON, el
//! formato), la frase es buena y el fichero está dañado.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use age::secrecy::SecretString;
use nix::fcntl::{Flock, FlockArg};
use serde::de::{Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use crate::llavero::{LlaveroHost, LlaveroSecreto};

/// Nombre del fichero en el directorio de datos.
pub const NOMBRE_FICHERO: &str = "llavero.age";
/// Mínimo de caracteres de la frase maestra, ya en NFC.
pub const MIN_FRASE: usize = 12;
/// Máximo de bytes de la frase maestra.
pub const MAX_FRASE_BYTES: usize = 1024;
/// Factor de trabajo de scrypt al crear o recifrar (2^17: ~128 MiB).
pub const LOG_N_CREAR: u8 = 17;
/// Factor máximo que se acepta al abrir (2^20): un fichero manipulado no deja
/// MAGI calculando minutos.
pub const LOG_N_MAX: u8 = 20;
/// Tope del contenido descifrado y del fichero (1 MiB): más es «dañado».
pub const TOPE_BYTES: usize = 1024 * 1024;
/// Fallos seguidos que imponen la espera.
pub const FALLOS_POR_SERIE: u32 = 5;
/// Espera tras una serie de fallos.
pub const ESPERA_SERIE: Duration = Duration::from_secs(30);

/// Texto de `guardar` cuando el fichero todavía no existe: hay que crearlo
/// antes con la frase (diálogo CREAR LA FRASE MAESTRA).
pub const SIN_CREAR: &str = "no hay llavero: crea la frase maestra primero";

/// Versión del JSON de dentro.
const FORMATO: u64 = 1;
/// Cuánto se espera al `flock` de otra ventana antes de rendirse.
const PLAZO_CERROJO: Duration = Duration::from_secs(10);
/// Pausa entre intentos de tomar el `flock`.
const PAUSA_CERROJO: Duration = Duration::from_millis(20);
/// Lo que se lee del fichero para mirar su cabecera sin abrirlo: la cabecera
/// de un llavero válido ocupa unos 200 bytes.
const BYTES_CABECERA: u64 = 4096;
/// Primera línea de todo fichero age.
const LINEA_AGE: &[u8] = b"age-encryption.org/v1\n";

/// En qué estado está el fichero y este objeto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoFichero {
    /// No hay `llavero.age`.
    Inexistente,
    /// Hay fichero legible, pero la frase no está en memoria.
    Cerrado,
    /// Frase y entradas en memoria.
    Abierto,
    /// La cabecera o el contenido son ilegibles; el motivo, en una línea.
    Danado(String),
}

/// Por qué falla una operación del llavero de fichero. Nunca lleva la frase ni
/// entradas del llavero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorLlavero {
    /// Operación que exige llavero abierto y está cerrado.
    Bloqueado,
    /// Operación que exige fichero y no lo hay.
    Inexistente,
    /// `crear` con un fichero que ya existe.
    YaExiste,
    /// La frase no abre el fichero (que no cambia). `intentos_seguidos` cuenta
    /// la serie; cada quinto fallo impone la espera y el llamante anota
    /// `llavero_desbloqueo_fallido`.
    FraseIncorrecta { intentos_seguidos: u32 },
    /// La frase no cumple las reglas (corta, vacía, larga o no coinciden): el
    /// texto dice cuál.
    FraseInvalida(String),
    /// Hay una espera en curso por una serie de fallos.
    Espera { segundos: u64 },
    /// Cabecera o contenido ilegibles (o factor de trabajo por encima del tope).
    Danado(String),
    /// Error de entrada/salida, con la causa en castellano.
    Io(String),
    /// `apartar_danado` con un fichero que ya no está dañado (otra ventana lo
    /// restauró o lo recreó mientras el diálogo seguía abierto).
    NoEstaDanado,
    /// `borrar` con un fichero dañado: un dañado se aparta, nunca se borra.
    NoSeBorraDanado,
}

impl std::fmt::Display for ErrorLlavero {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bloqueado => f.write_str(crate::llavero::BLOQUEADO),
            Self::Inexistente => f.write_str("no hay llavero"),
            Self::YaExiste => f.write_str("ya hay un llavero"),
            Self::FraseIncorrecta { .. } => f.write_str("frase incorrecta"),
            Self::FraseInvalida(motivo) | Self::Danado(motivo) | Self::Io(motivo) => {
                f.write_str(motivo)
            }
            Self::Espera { segundos } => write!(f, "espera {segundos} s antes de reintentar"),
            Self::NoEstaDanado => f.write_str("el llavero ya no está dañado"),
            Self::NoSeBorraDanado => f.write_str(
                "el llavero está dañado: no se borra, se aparta (queda como llavero.age.danado-<fecha>)",
            ),
        }
    }
}

impl std::error::Error for ErrorLlavero {}

/// Lo que se enseña de un fichero sin abrirlo (`magi llavero estado`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoFichero {
    pub ruta: PathBuf,
    pub bytes: u64,
    /// Última modificación, en segundos desde la época Unix.
    pub modificado: i64,
}

// ---------------------------------------------------------------------------
// Frase maestra
// ---------------------------------------------------------------------------

/// NFC en un búfer `Zeroizing` con capacidad de sobra (la forma normal puede
/// ser más larga que el texto: algunos caracteres se descomponen), para que no
/// haya realojos que dejen copias de la frase sin borrar. Rechaza de entrada
/// lo que no puede acabar dentro del tope, sin reservar memoria sin medida.
fn normalizar(texto: &str) -> Result<Zeroizing<String>, ErrorLlavero> {
    // NFC nunca encoge un texto a menos de un tercio de sus bytes (tres jamo
    // de Hangul, tres bytes cada uno, componen una sílaba de tres bytes).
    if texto.len() > MAX_FRASE_BYTES * 3 {
        return Err(frase_larga());
    }
    let mut salida = Zeroizing::new(String::with_capacity(texto.len() * 4));
    salida.extend(texto.nfc());
    Ok(salida)
}

fn frase_larga() -> ErrorLlavero {
    ErrorLlavero::FraseInvalida(format!(
        "la frase es demasiado larga (como mucho {MAX_FRASE_BYTES} bytes)"
    ))
}

/// Valida y normaliza una frase maestra: NFC, no solo espacios, entre
/// `MIN_FRASE` caracteres y `MAX_FRASE_BYTES` bytes y, si se da `repetida`, la
/// misma. Los mensajes no llevan nunca la frase.
pub fn preparar_frase(
    frase: &str,
    repetida: Option<&str>,
) -> Result<Zeroizing<String>, ErrorLlavero> {
    let frase = normalizar(frase)?;
    if let Some(repetida) = repetida {
        let repetida = normalizar(repetida)?;
        if *frase != *repetida {
            return Err(ErrorLlavero::FraseInvalida(
                "las frases no coinciden".to_string(),
            ));
        }
    }
    if frase.trim().is_empty() {
        return Err(ErrorLlavero::FraseInvalida(
            "la frase no puede estar vacía ni ser solo espacios".to_string(),
        ));
    }
    if frase.chars().count() < MIN_FRASE {
        return Err(ErrorLlavero::FraseInvalida(format!(
            "la frase necesita al menos {MIN_FRASE} caracteres"
        )));
    }
    if frase.len() > MAX_FRASE_BYTES {
        return Err(frase_larga());
    }
    Ok(frase)
}

/// La frase con la que se intenta abrir: solo NFC y lo mínimo para no gastar
/// scrypt en vano. Las reglas de longitud mínima son de crearla: abrir con una
/// frase corta es simplemente una frase incorrecta.
fn frase_para_abrir(frase: &str) -> Result<Zeroizing<String>, ErrorLlavero> {
    let frase = normalizar(frase)?;
    if frase.is_empty() {
        return Err(ErrorLlavero::FraseInvalida(
            "escribe la frase maestra".to_string(),
        ));
    }
    if frase.len() > MAX_FRASE_BYTES {
        return Err(frase_larga());
    }
    Ok(frase)
}

// ---------------------------------------------------------------------------
// Ficheros
// ---------------------------------------------------------------------------

/// Lo que cambia cuando alguien reescribe el fichero (inodo, tamaño y fechas):
/// permite a `estado` reconocer un fichero sin leerlo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Firma {
    dispositivo: u64,
    inodo: u64,
    bytes: u64,
    modificado: (i64, i64),
    cambiado: (i64, i64),
}

impl Firma {
    fn de(meta: &fs::Metadata) -> Self {
        Self {
            dispositivo: meta.dev(),
            inodo: meta.ino(),
            bytes: meta.len(),
            modificado: (meta.mtime(), meta.mtime_nsec()),
            cambiado: (meta.ctime(), meta.ctime_nsec()),
        }
    }
}

/// El fichero tal como se leyó, con la firma de ese mismo fichero abierto.
struct Lectura {
    bytes: Vec<u8>,
    firma: Firma,
}

type Resumen = [u8; 32];

fn resumen_de(bytes: &[u8]) -> Resumen {
    Sha256::digest(bytes).into()
}

/// Texto en castellano de un error de E/S (sin las rutas ni el texto de libc).
fn texto_io(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "no existe".to_string(),
        std::io::ErrorKind::PermissionDenied => "permiso denegado".to_string(),
        _ => match error.raw_os_error() {
            Some(codigo) => format!("error del sistema {codigo}"),
            None => "error de entrada/salida".to_string(),
        },
    }
}

fn error_io(accion: &str, error: &std::io::Error) -> ErrorLlavero {
    ErrorLlavero::Io(format!("no se pudo {accion}: {}", texto_io(error)))
}

/// Lee `llavero.age` con tope. `None` si no existe. Lo que no es un fichero
/// normal, o pasa del tope, es «dañado».
fn leer_fichero(ruta: &Path) -> Result<Option<Lectura>, ErrorLlavero> {
    let meta = match fs::metadata(ruta) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error_io("leer el llavero", &error)),
    };
    if !meta.is_file() {
        return Err(ErrorLlavero::Danado(
            "llavero.age no es un fichero normal".to_string(),
        ));
    }
    let mut fichero = match fs::File::open(ruta) {
        Ok(fichero) => fichero,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error_io("abrir el llavero", &error)),
    };
    // La firma sale del fichero ya abierto: la misma que los bytes que se leen.
    let firma = match fichero.metadata() {
        Ok(meta) => Firma::de(&meta),
        Err(error) => return Err(error_io("leer el llavero", &error)),
    };
    if firma.bytes > TOPE_BYTES as u64 {
        return Err(llavero_demasiado_grande());
    }
    let mut bytes = Vec::with_capacity(firma.bytes as usize + 1);
    if let Err(error) = (&mut fichero)
        .take(TOPE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
    {
        return Err(error_io("leer el llavero", &error));
    }
    if bytes.len() > TOPE_BYTES {
        return Err(llavero_demasiado_grande());
    }
    Ok(Some(Lectura { bytes, firma }))
}

fn llavero_demasiado_grande() -> ErrorLlavero {
    ErrorLlavero::Danado("el fichero pasa de 1 MiB, que es más de lo que admite un llavero".into())
}

/// Ruta del cerrojo: `<ruta>.lock`, junto al llavero.
fn ruta_cerrojo(ruta: &Path) -> PathBuf {
    let mut texto = ruta.as_os_str().to_owned();
    texto.push(".lock");
    PathBuf::from(texto)
}

/// El `flock` exclusivo de `<ruta>.lock`: se suelta al salir de ámbito.
struct Cerrojo {
    _flock: Flock<fs::File>,
}

// ---------------------------------------------------------------------------
// Cabecera, descifrado y JSON
// ---------------------------------------------------------------------------

/// Mira la cabecera sin gastar scrypt: que sea un age con una sola entrada
/// scrypt y con un factor de trabajo que MAGI acepta. `Err` lleva el motivo.
fn validar_cabecera(datos: &[u8]) -> Result<(), String> {
    let Some(resto) = datos.strip_prefix(LINEA_AGE) else {
        return Err("el fichero no es un llavero de MAGI (no tiene cabecera age)".to_string());
    };
    let linea = resto
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let linea = std::str::from_utf8(linea)
        .map_err(|_| "la cabecera age del llavero está dañada".to_string())?;
    let partes: Vec<&str> = linea.split(' ').collect();
    let [flecha, tipo, _sal, log_n] = partes[..] else {
        return Err("el llavero no está cifrado con una frase (sin entrada scrypt)".to_string());
    };
    if flecha != "->" || tipo != "scrypt" {
        return Err("el llavero no está cifrado con una frase (sin entrada scrypt)".to_string());
    }
    let log_n: u32 = log_n
        .parse()
        .map_err(|_| "la cabecera age del llavero está dañada (factor de trabajo)".to_string())?;
    if log_n > u32::from(LOG_N_MAX) {
        return Err(format!(
            "el factor de trabajo del llavero (2^{log_n}) supera el máximo admitido (2^{LOG_N_MAX})"
        ));
    }
    // La estructura completa la comprueba age: una sola entrada y de tipo scrypt.
    let descifrador = age::Decryptor::new_buffered(datos)
        .map_err(|_| "la cabecera age del llavero está incompleta o dañada".to_string())?;
    if !descifrador.is_scrypt() {
        return Err("el llavero no está cifrado con una sola entrada de frase".to_string());
    }
    Ok(())
}

/// Resultado de intentar abrir unos bytes con una frase.
enum ErrorAbrir {
    /// La entrada scrypt no descifra: frase incorrecta (el fichero no cambia).
    FraseIncorrecta,
    /// La frase puede ser buena, pero lo demás no se lee.
    Danado(String),
}

/// Explica un error de age sin pasar por su `Display` (carga textos traducidos
/// que pueden no estar: regla de CLAUDE.md).
fn texto_error_descifrado(error: &age::DecryptError) -> String {
    use age::DecryptError;
    match error {
        DecryptError::ExcessiveWork { required, .. } => format!(
            "el factor de trabajo del llavero (2^{required}) supera el máximo admitido (2^{LOG_N_MAX})"
        ),
        DecryptError::InvalidHeader | DecryptError::UnknownFormat => {
            "la cabecera age del llavero está dañada".to_string()
        }
        DecryptError::InvalidMac => {
            "la cabecera del llavero está alterada (MAC no válido)".to_string()
        }
        DecryptError::NoMatchingKeys => "el llavero no está cifrado con una frase".to_string(),
        DecryptError::DecryptionFailed | DecryptError::KeyDecryptionFailed => {
            "el contenido cifrado del llavero está dañado".to_string()
        }
        DecryptError::Io(_) => "el contenido cifrado del llavero está dañado o incompleto".into(),
    }
}

/// Descifra el fichero con la frase. El factor de trabajo de la cabecera se
/// comprueba antes de derivar nada (`validar_cabecera` y el tope de la
/// identidad), así que un fichero forjado no gasta tiempo ni memoria.
fn descifrar(datos: &[u8], frase: &str) -> Result<Zeroizing<Vec<u8>>, ErrorAbrir> {
    validar_cabecera(datos).map_err(ErrorAbrir::Danado)?;
    let descifrador = age::Decryptor::new_buffered(datos).map_err(|_| {
        ErrorAbrir::Danado("la cabecera age del llavero está incompleta o dañada".to_string())
    })?;
    let mut identidad = age::scrypt::Identity::new(SecretString::from(frase.to_owned()));
    identidad.set_max_work_factor(LOG_N_MAX);
    let mut lector = match descifrador.decrypt(std::iter::once(&identidad as &dyn age::Identity)) {
        Ok(lector) => lector,
        Err(age::DecryptError::DecryptionFailed) => return Err(ErrorAbrir::FraseIncorrecta),
        Err(otro) => return Err(ErrorAbrir::Danado(texto_error_descifrado(&otro))),
    };
    // Con capacidad de sobra para no realojar (cada realojo dejaría una copia
    // del texto en claro sin borrar): el claro no pasa del tamaño del cifrado.
    let mut claro = Zeroizing::new(Vec::with_capacity(datos.len().min(TOPE_BYTES) + 64));
    if lector
        .by_ref()
        .take(TOPE_BYTES as u64 + 1)
        .read_to_end(&mut claro)
        .is_err()
    {
        return Err(ErrorAbrir::Danado(
            "el contenido cifrado del llavero está dañado o incompleto".to_string(),
        ));
    }
    if claro.len() > TOPE_BYTES {
        return Err(ErrorAbrir::Danado(
            "el contenido del llavero pasa de 1 MiB".to_string(),
        ));
    }
    Ok(claro)
}

/// Cifra el texto en claro con la frase y el factor dado: age con una sola
/// entrada scrypt. El factor se fija siempre (si no, age lo calibra midiendo
/// el tiempo de la máquina).
fn cifrar(claro: &[u8], frase: &str, log_n: u8) -> Result<Vec<u8>, ErrorLlavero> {
    let fallo = || ErrorLlavero::Io("no se pudo cifrar el llavero".to_string());
    let mut destinatario = age::scrypt::Recipient::new(SecretString::from(frase.to_owned()));
    destinatario.set_work_factor(log_n);
    let cifrador =
        age::Encryptor::with_recipients(std::iter::once(&destinatario as &dyn age::Recipient))
            .map_err(|_| fallo())?;
    let mut salida = Vec::with_capacity(claro.len() + 512);
    let mut escritor = cifrador.wrap_output(&mut salida).map_err(|_| fallo())?;
    escritor.write_all(claro).map_err(|_| fallo())?;
    escritor.finish().map_err(|_| fallo())?;
    Ok(salida)
}

/// Entradas del llavero: los valores se borran al soltarse.
type Entradas = BTreeMap<String, Zeroizing<String>>;

/// Un secreto leído del JSON, directo a un búfer `Zeroizing`.
struct Secreto(Zeroizing<String>);

impl<'de> Deserialize<'de> for Secreto {
    fn deserialize<D: Deserializer<'de>>(deserializador: D) -> Result<Self, D::Error> {
        struct Texto;
        impl Visitor<'_> for Texto {
            type Value = Secreto;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("un texto")
            }
            fn visit_str<E>(self, valor: &str) -> Result<Secreto, E> {
                Ok(Secreto(Zeroizing::new(valor.to_owned())))
            }
            fn visit_string<E>(self, valor: String) -> Result<Secreto, E> {
                Ok(Secreto(Zeroizing::new(valor)))
            }
        }
        deserializador.deserialize_str(Texto)
    }
}

/// Solo el campo `formato`: se lee antes que el resto para distinguir un
/// formato de otra versión de un JSON roto, sin copiar ningún secreto.
#[derive(serde::Deserialize)]
struct SoloFormato {
    formato: u64,
}

#[derive(serde::Deserialize)]
struct Contenido {
    entradas: BTreeMap<String, Secreto>,
}

/// Lee el JSON de dentro. Los errores de serde no se enseñan: podrían citar
/// parte del contenido.
fn leer_entradas(claro: &[u8]) -> Result<Entradas, String> {
    let invalido = || "el contenido del llavero no es un JSON válido".to_string();
    let SoloFormato { formato } = serde_json::from_slice(claro).map_err(|_| invalido())?;
    if formato != FORMATO {
        return Err(format!(
            "el llavero es de un formato desconocido ({formato}); este MAGI entiende el {FORMATO}"
        ));
    }
    let contenido: Contenido = serde_json::from_slice(claro).map_err(|_| invalido())?;
    Ok(contenido
        .entradas
        .into_iter()
        .map(|(cuenta, secreto)| (cuenta, secreto.0))
        .collect())
}

/// El JSON de dentro, para serializar sin copiar los secretos.
struct Documento<'a>(&'a Entradas);

struct EntradasRef<'a>(&'a Entradas);

impl Serialize for EntradasRef<'_> {
    fn serialize<S: Serializer>(&self, serializador: S) -> Result<S::Ok, S::Error> {
        serializador.collect_map(
            self.0
                .iter()
                .map(|(cuenta, valor)| (cuenta.as_str(), valor.as_str())),
        )
    }
}

impl Serialize for Documento<'_> {
    fn serialize<S: Serializer>(&self, serializador: S) -> Result<S::Ok, S::Error> {
        let mut mapa = serializador.serialize_map(Some(2))?;
        mapa.serialize_entry("formato", &FORMATO)?;
        mapa.serialize_entry("entradas", &EntradasRef(self.0))?;
        mapa.end()
    }
}

/// El JSON en un búfer `Zeroizing` con capacidad para el peor caso (seis
/// bytes por byte de texto, el escape `\u00XX`): sin realojos que dejen copias.
fn serializar(entradas: &Entradas) -> Result<Zeroizing<Vec<u8>>, ErrorLlavero> {
    let capacidad = 64
        + entradas
            .iter()
            .map(|(cuenta, valor)| 6 * (cuenta.len() + valor.len()) + 16)
            .sum::<usize>();
    let mut salida = Zeroizing::new(Vec::with_capacity(capacidad));
    serde_json::to_writer(&mut *salida, &Documento(entradas))
        .map_err(|_| ErrorLlavero::Io("no se pudo preparar el llavero para guardarlo".into()))?;
    Ok(salida)
}

/// Qué se sabe de un fichero sin abrirlo: `None` si no existe.
pub fn info_fichero(ruta: &Path) -> Result<Option<InfoFichero>, ErrorLlavero> {
    match fs::metadata(ruta) {
        Ok(meta) if meta.is_file() => Ok(Some(InfoFichero {
            ruta: ruta.to_path_buf(),
            bytes: meta.len(),
            modificado: meta.mtime(),
        })),
        Ok(_) => Err(ErrorLlavero::Danado(
            "llavero.age no es un fichero normal".to_string(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error_io("leer el llavero", &error)),
    }
}

// ---------------------------------------------------------------------------
// El llavero
// ---------------------------------------------------------------------------

/// Reloj inyectable de las esperas de la serie de fallos.
pub type Reloj = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Lo que cambia con el uso, tras un cerrojo que se toma solo un instante.
struct Interno {
    /// La frase maestra, en NFC. `Some` es «abierto».
    frase: Option<Zeroizing<String>>,
    entradas: Entradas,
    /// Firma del fichero del que salen `entradas` (o que escribimos).
    firma: Option<Firma>,
    /// SHA-256 de ese fichero: distingue «otra ventana escribió» sin scrypt
    /// aunque el sistema de ficheros reutilice inodos o tenga fechas toscas.
    resumen: Option<Resumen>,
    /// Sube con cada bloqueo: una operación lenta que empezó antes no reabre
    /// lo que el usuario acaba de cerrar.
    generacion: u64,
    /// Contenido ilegible visto con la frase buena (MAC, flujo, JSON o
    /// formato): con la cabecera sola no se ve, así que se recuerda la firma
    /// del fichero en el que se vio. Si el fichero cambia, deja de valer.
    danado: Option<(Firma, String)>,
    /// Fallos seguidos de la serie en curso.
    fallos: u32,
    espera_hasta: Option<Instant>,
}

impl Interno {
    /// Borra de la memoria la frase y las entradas.
    fn vaciar(&mut self) {
        self.frase = None;
        self.entradas.clear();
        self.firma = None;
        self.resumen = None;
        self.generacion += 1;
    }
}

/// El llavero de fichero. Seguro entre hilos: el estado va tras un `Mutex`.
pub struct LlaveroFichero {
    ruta: PathBuf,
    log_n_crear: u8,
    reloj: Reloj,
    interno: Mutex<Interno>,
    /// Serializa las operaciones lentas dentro de este objeto.
    operacion: Mutex<()>,
}

impl std::fmt::Debug for LlaveroFichero {
    /// Nunca imprime la frase ni las entradas.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlaveroFichero")
            .field("ruta", &self.ruta)
            .finish_non_exhaustive()
    }
}

impl Drop for LlaveroFichero {
    fn drop(&mut self) {
        self.interno().vaciar();
    }
}

impl LlaveroFichero {
    /// El llavero de producción: `ruta` es `llavero.age` en el directorio de
    /// datos (el llamante la calcula; el núcleo no lee el entorno).
    pub fn nuevo(ruta: PathBuf) -> Self {
        Self::con_ajustes(ruta, LOG_N_CREAR, Arc::new(Instant::now))
    }

    /// Para las pruebas: factor de trabajo bajo (entre 4 y `LOG_N_MAX`) y reloj
    /// propio. Con el factor de producción en depuración, scrypt tarda decenas
    /// de segundos si no se optimiza.
    pub fn con_ajustes(ruta: PathBuf, log_n_crear: u8, reloj: Reloj) -> Self {
        Self {
            ruta,
            log_n_crear: log_n_crear.clamp(4, LOG_N_MAX),
            reloj,
            interno: Mutex::new(Interno {
                frase: None,
                entradas: Entradas::new(),
                firma: None,
                resumen: None,
                generacion: 0,
                danado: None,
                fallos: 0,
                espera_hasta: None,
            }),
            operacion: Mutex::new(()),
        }
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }

    fn interno(&self) -> MutexGuard<'_, Interno> {
        self.interno.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn operacion(&self) -> MutexGuard<'_, ()> {
        self.operacion
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// El fichero ya no existe: se olvida lo que había en memoria, salvo que
    /// una operación de este objeto esté en curso. Esas ya miran el fichero
    /// por sí mismas, y borrar aquí podría deshacer un `crear` que acaba de
    /// escribirlo (el `stat` que vio «no existe» es anterior a su `rename`).
    fn olvidar_memoria_sin_fichero(&self) {
        if let Ok(_libre) = self.operacion.try_lock() {
            self.interno().vaciar();
        }
    }

    /// Toma el `flock` de `<ruta>.lock`, con plazo: otra ventana lo suelta en
    /// cuanto termina de escribir, y si no, no se espera para siempre. El plazo
    /// va por el reloj real: el inyectado es solo el de la serie de fallos.
    fn tomar_cerrojo(&self) -> Result<Cerrojo, ErrorLlavero> {
        if let Some(directorio) = self.ruta.parent() {
            fs::create_dir_all(directorio)
                .map_err(|error| error_io("preparar la carpeta", &error))?;
        }
        let ruta = ruta_cerrojo(&self.ruta);
        let mut fichero = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .mode(0o600)
            // Un enlace plantado en el nombre del cerrojo no se sigue.
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&ruta)
            .map_err(|error| error_io("abrir el cerrojo del llavero", &error))?;
        let limite = Instant::now() + PLAZO_CERROJO;
        loop {
            match Flock::lock(fichero, FlockArg::LockExclusiveNonblock) {
                Ok(flock) => return Ok(Cerrojo { _flock: flock }),
                Err((_, errno)) if errno != nix::errno::Errno::EWOULDBLOCK => {
                    // ENOLCK, EOPNOTSUPP…: el sistema de ficheros no sabe
                    // cerrar; esperar no lo arregla ni «otra ventana» es cierto.
                    return Err(ErrorLlavero::Io(format!(
                        "no se pudo bloquear el llavero: error del sistema {}",
                        errno as i32
                    )));
                }
                Err((devuelto, _)) => {
                    // El Flock devuelve el fichero: se reintenta con él.
                    fichero = devuelto;
                    if Instant::now() >= limite {
                        return Err(ErrorLlavero::Io(
                            "otra ventana de MAGI está usando el llavero; vuelve a intentarlo en unos segundos"
                                .to_string(),
                        ));
                    }
                    std::thread::sleep(PAUSA_CERROJO);
                }
            }
        }
    }

    /// Escribe el fichero con `escribir_atomico` (600) y devuelve su firma.
    /// Siempre con el `flock` tomado.
    fn escribir(&self, cifrado: &[u8]) -> Result<Firma, ErrorLlavero> {
        if cifrado.len() > TOPE_BYTES {
            return Err(ErrorLlavero::Io(
                "el llavero pasaría de 1 MiB: no hay sitio para más".to_string(),
            ));
        }
        crate::ficheros::escribir_atomico(&self.ruta, cifrado, 0o600).map_err(|error| {
            let causa = error
                .chain()
                .find_map(|causa| causa.downcast_ref::<std::io::Error>())
                .map(texto_io)
                .unwrap_or_else(|| "error de escritura".to_string());
            ErrorLlavero::Io(format!("no se pudo escribir el llavero: {causa}"))
        })?;
        fs::metadata(&self.ruta)
            .map(|meta| Firma::de(&meta))
            .map_err(|error| error_io("leer el llavero", &error))
    }

    /// Si hay una espera en curso, cuántos segundos quedan (sin gastar scrypt).
    fn comprobar_espera(&self) -> Result<(), ErrorLlavero> {
        let ahora = (self.reloj)();
        let mut interno = self.interno();
        if let Some(hasta) = interno.espera_hasta {
            if ahora < hasta {
                let resto = hasta.saturating_duration_since(ahora);
                let segundos = resto.as_secs() + u64::from(resto.subsec_nanos() > 0);
                return Err(ErrorLlavero::Espera { segundos });
            }
            interno.espera_hasta = None;
        }
        Ok(())
    }

    /// Cuenta un fallo de frase; al quinto de cada serie impone la espera.
    fn registrar_fallo(&self) -> u32 {
        let ahora = (self.reloj)();
        let mut interno = self.interno();
        interno.fallos = interno.fallos.saturating_add(1);
        if interno.fallos % FALLOS_POR_SERIE == 0 {
            interno.espera_hasta = Some(ahora + ESPERA_SERIE);
        }
        interno.fallos
    }

    /// Anota que el fichero con esa firma tiene el contenido ilegible (con la
    /// frase buena) y devuelve el error que lo cuenta.
    fn recordar_danado(&self, firma: Firma, motivo: String) -> ErrorLlavero {
        self.interno().danado = Some((firma, motivo.clone()));
        ErrorLlavero::Danado(motivo)
    }

    /// Estado actual. Barato: no gasta scrypt; mira si el fichero existe, si
    /// la cabecera es legible y si la frase está en memoria.
    ///
    /// Con la frase en memoria, que otra ventana reescriba el fichero no lo
    /// cierra: sigue abierto mientras la cabecera se lea (las entradas se
    /// releen al siguiente `guardar`). Si el fichero desaparece, se olvida lo
    /// que había en memoria.
    pub fn estado(&self) -> EstadoFichero {
        let meta = match fs::metadata(&self.ruta) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.olvidar_memoria_sin_fichero();
                return EstadoFichero::Inexistente;
            }
            Err(error) => {
                return EstadoFichero::Danado(format!(
                    "no se puede leer el llavero: {}",
                    texto_io(&error)
                ))
            }
        };
        if !meta.is_file() {
            return EstadoFichero::Danado("llavero.age no es un fichero normal".to_string());
        }
        if meta.len() > TOPE_BYTES as u64 {
            return EstadoFichero::Danado(llavero_demasiado_grande().to_string());
        }
        let firma = Firma::de(&meta);
        {
            let interno = self.interno();
            if let Some((vista, motivo)) = &interno.danado {
                if *vista == firma {
                    return EstadoFichero::Danado(motivo.clone());
                }
            }
            if interno.frase.is_some() && interno.firma == Some(firma) {
                return EstadoFichero::Abierto;
            }
        }
        // Firma nueva o frase ausente: se mira la cabecera (sin scrypt).
        let mut cabecera = Vec::new();
        let leida = fs::File::open(&self.ruta)
            .and_then(|fichero| fichero.take(BYTES_CABECERA).read_to_end(&mut cabecera));
        if let Err(error) = leida {
            return EstadoFichero::Danado(format!(
                "no se puede leer el llavero: {}",
                texto_io(&error)
            ));
        }
        if let Err(motivo) = validar_cabecera(&cabecera) {
            return EstadoFichero::Danado(motivo);
        }
        if self.interno().frase.is_some() {
            EstadoFichero::Abierto
        } else {
            EstadoFichero::Cerrado
        }
    }

    /// Crea `llavero.age` vacío con esa frase (pedida dos veces) y lo deja
    /// abierto. Falla con `YaExiste` si hay fichero.
    pub fn crear(&self, frase: &str, repetida: &str) -> Result<(), ErrorLlavero> {
        let frase = preparar_frase(frase, Some(repetida))?;
        let _operacion = self.operacion();
        let _cerrojo = self.tomar_cerrojo()?;
        match fs::symlink_metadata(&self.ruta) {
            Ok(_) => return Err(ErrorLlavero::YaExiste),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error_io("leer el llavero", &error)),
        }
        let entradas = Entradas::new();
        let generacion = self.interno().generacion;
        let cifrado = cifrar(&serializar(&entradas)?, &frase, self.log_n_crear)?;
        let firma = self.escribir(&cifrado)?;
        let mut interno = self.interno();
        if interno.generacion != generacion {
            // Se bloqueó mientras scrypt trabajaba: el fichero ya está creado,
            // pero lo cerrado no se reabre.
            return Err(ErrorLlavero::Bloqueado);
        }
        interno.frase = Some(frase);
        interno.entradas = entradas;
        interno.firma = Some(firma);
        interno.resumen = Some(resumen_de(&cifrado));
        interno.fallos = 0;
        interno.espera_hasta = None;
        Ok(())
    }

    /// Abre el fichero con la frase y deja frase y entradas en memoria.
    /// Con la frase incorrecta el fichero no cambia.
    pub fn desbloquear(&self, frase: &str) -> Result<(), ErrorLlavero> {
        self.comprobar_espera()?;
        let frase = frase_para_abrir(frase)?;
        let _operacion = self.operacion();
        // Otro hilo pudo imponer la espera mientras esperábamos el turno.
        self.comprobar_espera()?;
        let generacion = self.interno().generacion;
        let lectura = leer_fichero(&self.ruta)?.ok_or(ErrorLlavero::Inexistente)?;
        let claro = match descifrar(&lectura.bytes, &frase) {
            Ok(claro) => claro,
            Err(ErrorAbrir::FraseIncorrecta) => {
                return Err(ErrorLlavero::FraseIncorrecta {
                    intentos_seguidos: self.registrar_fallo(),
                })
            }
            Err(ErrorAbrir::Danado(motivo)) => {
                return Err(self.recordar_danado(lectura.firma, motivo));
            }
        };
        let entradas = match leer_entradas(&claro) {
            Ok(entradas) => entradas,
            Err(motivo) => return Err(self.recordar_danado(lectura.firma, motivo)),
        };
        let mut interno = self.interno();
        if interno.generacion != generacion {
            // «Bloquear» llegó mientras scrypt trabajaba: lo cerrado no se
            // reabre solo.
            return Err(ErrorLlavero::Bloqueado);
        }
        interno.frase = Some(frase);
        interno.entradas = entradas;
        interno.firma = Some(lectura.firma);
        interno.resumen = Some(resumen_de(&lectura.bytes));
        interno.fallos = 0;
        interno.espera_hasta = None;
        Ok(())
    }

    /// Borra de la memoria la frase y las entradas (`zeroize`). No toca el
    /// fichero.
    pub fn bloquear(&self) {
        self.interno().vaciar();
    }

    /// Recifra con la frase nueva, bajo el cerrojo y con escritura atómica.
    /// Abre con la actual aunque esté bloqueado; si la actual es incorrecta,
    /// cuenta como un intento fallido de la serie.
    pub fn cambiar_frase(
        &self,
        actual: &str,
        nueva: &str,
        repetida: &str,
    ) -> Result<(), ErrorLlavero> {
        let nueva = preparar_frase(nueva, Some(repetida))?;
        self.comprobar_espera()?;
        let actual = frase_para_abrir(actual)?;
        let _operacion = self.operacion();
        self.comprobar_espera()?;
        let _cerrojo = self.tomar_cerrojo()?;
        let generacion = self.interno().generacion;
        let lectura = leer_fichero(&self.ruta)?.ok_or(ErrorLlavero::Inexistente)?;
        let claro = match descifrar(&lectura.bytes, &actual) {
            Ok(claro) => claro,
            Err(ErrorAbrir::FraseIncorrecta) => {
                return Err(ErrorLlavero::FraseIncorrecta {
                    intentos_seguidos: self.registrar_fallo(),
                })
            }
            Err(ErrorAbrir::Danado(motivo)) => {
                return Err(self.recordar_danado(lectura.firma, motivo));
            }
        };
        let entradas = match leer_entradas(&claro) {
            Ok(entradas) => entradas,
            Err(motivo) => return Err(self.recordar_danado(lectura.firma, motivo)),
        };
        drop(claro);
        let cifrado = cifrar(&serializar(&entradas)?, &nueva, self.log_n_crear)?;
        let firma = self.escribir(&cifrado)?;
        let mut interno = self.interno();
        if interno.generacion != generacion {
            // Se bloqueó mientras scrypt trabajaba: el fichero ya tiene la
            // frase nueva, pero lo cerrado no se reabre.
            return Err(ErrorLlavero::Bloqueado);
        }
        interno.frase = Some(nueva);
        interno.entradas = entradas;
        interno.firma = Some(firma);
        interno.resumen = Some(resumen_de(&cifrado));
        interno.fallos = 0;
        interno.espera_hasta = None;
        Ok(())
    }

    /// Borra el fichero (y la memoria). No exige la frase: la confirmación la
    /// pide la interfaz.
    ///
    /// Un fichero dañado no se borra (se aparta con `apartar_danado`): se
    /// mira bajo el cerrojo, con el fichero que hay ahora y no con el que vio
    /// un diálogo hace rato.
    pub fn borrar(&self) -> Result<(), ErrorLlavero> {
        let _operacion = self.operacion();
        let _cerrojo = self.tomar_cerrojo()?;
        if matches!(self.estado(), EstadoFichero::Danado(_)) {
            return Err(ErrorLlavero::NoSeBorraDanado);
        }
        match fs::remove_file(&self.ruta) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error_io("borrar el llavero", &error)),
        }
        self.interno().vaciar();
        Ok(())
    }

    /// Aparta un fichero dañado como `llavero.age.danado-<AAAA-MM-DD>` (con
    /// `-2`, `-3`… si existe) y devuelve la ruta de la copia. Nunca borra.
    pub fn apartar_danado(&self) -> Result<PathBuf, ErrorLlavero> {
        let nombre = self
            .ruta
            .file_name()
            .and_then(|nombre| nombre.to_str())
            .ok_or_else(|| ErrorLlavero::Io("la ruta del llavero no es válida".to_string()))?
            .to_string();
        let _operacion = self.operacion();
        let _cerrojo = self.tomar_cerrojo()?;
        match fs::symlink_metadata(&self.ruta) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ErrorLlavero::Inexistente)
            }
            Err(error) => return Err(error_io("leer el llavero", &error)),
        }
        // Solo se aparta lo que sigue dañado: otra ventana pudo apartarlo y
        // crear uno bueno mientras el diálogo seguía abierto. («Inexistente»
        // con algo en la ruta es un enlace roto: eso sí se aparta.)
        if matches!(
            self.estado(),
            EstadoFichero::Abierto | EstadoFichero::Cerrado
        ) {
            return Err(ErrorLlavero::NoEstaDanado);
        }
        let fecha = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut copia = self.ruta.with_file_name(format!("{nombre}.danado-{fecha}"));
        let mut sufijo = 2u32;
        // Con el cerrojo tomado, nadie más de MAGI crea copias a la vez.
        while fs::symlink_metadata(&copia).is_ok() {
            copia = self
                .ruta
                .with_file_name(format!("{nombre}.danado-{fecha}-{sufijo}"));
            sufijo += 1;
        }
        fs::rename(&self.ruta, &copia).map_err(|error| error_io("apartar el llavero", &error))?;
        let mut interno = self.interno();
        interno.vaciar();
        interno.danado = None;
        Ok(copia)
    }

    /// Fallos seguidos de la serie en curso (para el mensaje de la interfaz).
    pub fn intentos_seguidos(&self) -> u32 {
        self.interno().fallos
    }

    /// Lo común de `guardar` y `olvidar`: con el llavero abierto, bajo el
    /// cerrojo, relee el fichero si otra ventana lo cambió, aplica `cambio` a
    /// las entradas y, si pide escribir, lo escribe atómico. Errores como
    /// texto, que es lo que habla `LlaveroSecreto`.
    fn modificar<R>(&self, cambio: impl FnOnce(&mut Entradas) -> (R, bool)) -> Result<R, String> {
        let _operacion = self.operacion();
        // 1. Hay fichero y la frase está en memoria.
        match fs::metadata(&self.ruta) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.interno().vaciar();
                return Err(SIN_CREAR.to_string());
            }
            Err(error) => return Err(error_io("leer el llavero", &error).to_string()),
        }
        let (frase, generacion) = {
            let interno = self.interno();
            match &interno.frase {
                Some(frase) => (Zeroizing::new(frase.to_string()), interno.generacion),
                None => return Err(crate::llavero::BLOQUEADO.to_string()),
            }
        };
        // 2. Bajo el cerrojo, el fichero que hay ahora.
        let _cerrojo = self.tomar_cerrojo().map_err(|error| error.to_string())?;
        let lectura = match leer_fichero(&self.ruta) {
            Ok(Some(lectura)) => lectura,
            Ok(None) => {
                self.interno().vaciar();
                return Err(SIN_CREAR.to_string());
            }
            Err(error) => return Err(error.to_string()),
        };
        let resumen = resumen_de(&lectura.bytes);
        // 3. Las entradas de partida: las de memoria si el fichero es el que
        // leímos; si otra ventana escribió, lo releído con nuestra frase.
        // La comparación y la copia van en un solo bloqueo del estado: si el
        // usuario bloquea justo en medio, la copia saldría vacía y se
        // escribiría un llavero sin el resto de las claves. Con otra
        // generación (se bloqueó), se relee del fichero.
        let de_memoria = {
            let interno = self.interno();
            (interno.generacion == generacion && interno.resumen == Some(resumen))
                .then(|| interno.entradas.clone())
        };
        let mut entradas = if let Some(entradas) = de_memoria {
            entradas
        } else {
            match descifrar(&lectura.bytes, &frase) {
                Ok(claro) => match leer_entradas(&claro) {
                    Ok(entradas) => entradas,
                    Err(motivo) => {
                        return Err(self.recordar_danado(lectura.firma, motivo).to_string())
                    }
                },
                Err(ErrorAbrir::FraseIncorrecta) => {
                    // Otra ventana cambió la frase: la nuestra ya no vale. Se
                    // cierra para que la interfaz pida la nueva, y no se
                    // escribe nada.
                    let mut interno = self.interno();
                    if interno.generacion == generacion {
                        interno.vaciar();
                    }
                    return Err(
                        "la frase maestra se cambió desde otra ventana: el llavero se ha bloqueado, desbloquéalo con la frase nueva (Ctrl+P)"
                            .to_string(),
                    );
                }
                Err(ErrorAbrir::Danado(motivo)) => {
                    return Err(self.recordar_danado(lectura.firma, motivo).to_string());
                }
            }
        };
        // 4. El cambio.
        let (resultado, escribir) = cambio(&mut entradas);
        let (firma, resumen) = if escribir {
            let cifrado = cifrar(
                &serializar(&entradas).map_err(|error| error.to_string())?,
                &frase,
                self.log_n_crear,
            )
            .map_err(|error| error.to_string())?;
            let firma = self.escribir(&cifrado).map_err(|error| error.to_string())?;
            (firma, resumen_de(&cifrado))
        } else {
            (lectura.firma, resumen)
        };
        // 5. La memoria sigue al fichero, salvo que se bloqueara entre medias.
        let mut interno = self.interno();
        if interno.generacion == generacion {
            interno.entradas = entradas;
            interno.firma = Some(firma);
            interno.resumen = Some(resumen);
        }
        Ok(resultado)
    }
}

impl LlaveroSecreto for LlaveroFichero {
    /// Sin fichero: `Err(SIN_CREAR)`; cerrado: `Err(BLOQUEADO)`; abierto:
    /// relee bajo `flock`, aplica y escribe atómico.
    fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String> {
        let valor = Zeroizing::new(valor.to_owned());
        self.modificar(|entradas| {
            entradas.insert(cuenta.to_owned(), valor);
            ((), true)
        })
    }

    /// Sin fichero: `Ok(None)`; cerrado: `Err(BLOQUEADO)`; abierto: de la
    /// memoria.
    fn recuperar(&self, cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        match fs::metadata(&self.ruta) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.olvidar_memoria_sin_fichero();
                return Ok(None);
            }
            Err(error) => return Err(error_io("leer el llavero", &error).to_string()),
        }
        let interno = self.interno();
        if interno.frase.is_none() {
            return Err(crate::llavero::BLOQUEADO.to_string());
        }
        Ok(interno
            .entradas
            .get(cuenta)
            .map(|valor| Zeroizing::new(valor.to_string())))
    }

    /// Como `guardar`: devuelve si existía. Si no existía, no reescribe nada.
    fn olvidar(&self, cuenta: &str) -> Result<bool, String> {
        self.modificar(|entradas| {
            let existia = entradas.remove(cuenta).is_some();
            (existia, existia)
        })
    }
}

impl LlaveroHost for LlaveroFichero {
    /// La cuenta es `usuario@host`, igual que `LlaveroEnMemoria`.
    fn guardar_contrasena(
        &self,
        host: &str,
        usuario: &str,
        contrasena: &str,
    ) -> Result<(), String> {
        LlaveroSecreto::guardar(self, &format!("{usuario}@{host}"), contrasena)
    }

    fn recuperar_contrasena(
        &self,
        host: &str,
        usuario: &str,
    ) -> Result<Option<Zeroizing<String>>, String> {
        LlaveroSecreto::recuperar(self, &format!("{usuario}@{host}"))
    }
}
