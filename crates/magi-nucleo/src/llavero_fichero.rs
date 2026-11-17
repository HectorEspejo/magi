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
//! CONTRATO-S1-LLAVERO-FICHERO: el cuerpo lo completa la pista P1 del Sprint 1.
//! Las firmas públicas de este módulo son el contrato con las pistas de la
//! interfaz y de la CLI: no las cambies sin avisar.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// Valida y normaliza una frase maestra: NFC, no solo espacios, entre
/// `MIN_FRASE` caracteres y `MAX_FRASE_BYTES` bytes y, si se da `repetida`, la
/// misma. CONTRATO-S1-LLAVERO-FICHERO.
pub fn preparar_frase(
    frase: &str,
    repetida: Option<&str>,
) -> Result<Zeroizing<String>, ErrorLlavero> {
    let _ = (frase, repetida);
    Err(ErrorLlavero::Io("CONTRATO-S1-LLAVERO-FICHERO".into()))
}

/// Qué se sabe de un fichero sin abrirlo: `None` si no existe.
/// CONTRATO-S1-LLAVERO-FICHERO.
pub fn info_fichero(ruta: &Path) -> Result<Option<InfoFichero>, ErrorLlavero> {
    let _ = ruta;
    Err(ErrorLlavero::Io("CONTRATO-S1-LLAVERO-FICHERO".into()))
}

/// Reloj inyectable de las esperas de la serie de fallos.
pub type Reloj = Arc<dyn Fn() -> Instant + Send + Sync>;

/// El llavero de fichero. Seguro entre hilos: el estado va tras un `Mutex`.
pub struct LlaveroFichero {
    ruta: PathBuf,
    // CONTRATO-S1-LLAVERO-FICHERO: el estado interno (frase, entradas, firma
    // del fichero leído, serie de fallos, factor y reloj) lo define la pista P1.
    #[allow(dead_code)]
    log_n_crear: u8,
    #[allow(dead_code)]
    reloj: Reloj,
}

impl std::fmt::Debug for LlaveroFichero {
    /// Nunca imprime la frase ni las entradas.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlaveroFichero")
            .field("ruta", &self.ruta)
            .finish_non_exhaustive()
    }
}

impl LlaveroFichero {
    /// El llavero de producción: `ruta` es `llavero.age` en el directorio de
    /// datos (el llamante la calcula; el núcleo no lee el entorno).
    pub fn nuevo(ruta: PathBuf) -> Self {
        Self::con_ajustes(ruta, LOG_N_CREAR, Arc::new(Instant::now))
    }

    /// Para las pruebas: factor de trabajo bajo y reloj propio. Con el factor
    /// de producción en depuración, scrypt tarda decenas de segundos.
    pub fn con_ajustes(ruta: PathBuf, log_n_crear: u8, reloj: Reloj) -> Self {
        Self {
            ruta,
            log_n_crear,
            reloj,
        }
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }

    /// Estado actual. Barato: no gasta scrypt; mira si el fichero existe, si
    /// la cabecera es legible y si la frase está en memoria.
    /// CONTRATO-S1-LLAVERO-FICHERO.
    pub fn estado(&self) -> EstadoFichero {
        EstadoFichero::Inexistente
    }

    /// Crea `llavero.age` vacío con esa frase (pedida dos veces) y lo deja
    /// abierto. Falla con `YaExiste` si hay fichero. CONTRATO-S1-LLAVERO-FICHERO.
    pub fn crear(&self, frase: &str, repetida: &str) -> Result<(), ErrorLlavero> {
        let _ = (frase, repetida);
        Err(ErrorLlavero::Io("CONTRATO-S1-LLAVERO-FICHERO".into()))
    }

    /// Abre el fichero con la frase y deja frase y entradas en memoria.
    /// Con la frase incorrecta el fichero no cambia. CONTRATO-S1-LLAVERO-FICHERO.
    pub fn desbloquear(&self, frase: &str) -> Result<(), ErrorLlavero> {
        let _ = frase;
        Err(ErrorLlavero::Io("CONTRATO-S1-LLAVERO-FICHERO".into()))
    }

    /// Borra de la memoria la frase y las entradas (`zeroize`). No toca el
    /// fichero. CONTRATO-S1-LLAVERO-FICHERO.
    pub fn bloquear(&self) {}

    /// Recifra con la frase nueva, bajo el cerrojo y con escritura atómica.
    /// Abre con la actual aunque esté bloqueado. CONTRATO-S1-LLAVERO-FICHERO.
    pub fn cambiar_frase(
        &self,
        actual: &str,
        nueva: &str,
        repetida: &str,
    ) -> Result<(), ErrorLlavero> {
        let _ = (actual, nueva, repetida);
        Err(ErrorLlavero::Io("CONTRATO-S1-LLAVERO-FICHERO".into()))
    }

    /// Borra el fichero (y la memoria). No exige la frase: la confirmación la
    /// pide la interfaz. CONTRATO-S1-LLAVERO-FICHERO.
    pub fn borrar(&self) -> Result<(), ErrorLlavero> {
        Err(ErrorLlavero::Io("CONTRATO-S1-LLAVERO-FICHERO".into()))
    }

    /// Aparta un fichero dañado como `llavero.age.danado-<AAAA-MM-DD>` (con
    /// `-2`, `-3`… si existe) y devuelve la ruta de la copia. Nunca borra.
    /// CONTRATO-S1-LLAVERO-FICHERO.
    pub fn apartar_danado(&self) -> Result<PathBuf, ErrorLlavero> {
        Err(ErrorLlavero::Io("CONTRATO-S1-LLAVERO-FICHERO".into()))
    }

    /// Fallos seguidos de la serie en curso (para el mensaje de la interfaz).
    /// CONTRATO-S1-LLAVERO-FICHERO.
    pub fn intentos_seguidos(&self) -> u32 {
        0
    }
}

impl LlaveroSecreto for LlaveroFichero {
    /// Sin fichero: `Err(SIN_CREAR)`; cerrado: `Err(BLOQUEADO)`; abierto:
    /// relee bajo `flock`, aplica y escribe atómico. CONTRATO-S1-LLAVERO-FICHERO.
    fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String> {
        let _ = (cuenta, valor);
        Err("CONTRATO-S1-LLAVERO-FICHERO".into())
    }

    /// Sin fichero: `Ok(None)`; cerrado: `Err(BLOQUEADO)`; abierto: de la
    /// memoria. CONTRATO-S1-LLAVERO-FICHERO.
    fn recuperar(&self, cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        let _ = cuenta;
        Err("CONTRATO-S1-LLAVERO-FICHERO".into())
    }

    /// Como `guardar`: devuelve si existía. CONTRATO-S1-LLAVERO-FICHERO.
    fn olvidar(&self, cuenta: &str) -> Result<bool, String> {
        let _ = cuenta;
        Err("CONTRATO-S1-LLAVERO-FICHERO".into())
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
