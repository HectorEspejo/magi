//! Bloque TERMUX de `magi diagnostico` (§6.5, §7.9) y las líneas generales que
//! cambian en Termux.
//!
//! Igual que el resto del diagnóstico, en dos mitades:
//!
//! - [`observar`] mira Termux (versiones, Termux:API con `termux-battery-status`,
//!   almacenamiento, fila de teclas, marcas del estado, fecha del llavero) y lo
//!   deja en un [`DatosTermux`]. No lee `std::env` (todo se inyecta), no
//!   escribe nada y no abre el llavero ni pide la frase maestra: del llavero
//!   solo mira si `llavero.age` existe y cuándo se modificó. ([`observar_interrumpible`]
//!   es la de la línea de comandos: lo mismo, pero se puede interrumpir con
//!   Ctrl+C mientras espera a Termux:API.)
//! - [`bloque`], [`llavero`], [`portapapeles`], [`terminal`] y
//!   [`ruta_con_tmpdir`] lo interpretan sin tocar el sistema, para probarlos
//!   con un `DatosTermux` hecho a mano.
//!
//! Las líneas con `·` ([`Estado::Info`]) son lo que MAGI no puede comprobar
//! desde Termux (procesos en segundo plano y batería): informan, no cuentan
//! como avisos ni cambian el código de salida. Nada de lo que devuelve una
//! orden o un fichero (entrada no fiable) llega al log; en pantalla, solo
//! saneado y acotado.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Datelike as _, Local, NaiveDate, Utc};
use magi_nucleo::llavero_fichero::NOMBRE_FICHERO;
use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};

use crate::config::{Config, Rutas};
use crate::diagnostico::{combinar, ruta_legible, Comprobacion, Datos, Estado, ServidorObservado};

use super::estado::{self, AvisoServidor, Marca};
use super::ordenes::{ApiProceso, Ejecucion, FalloOrden, Ordenes};
use super::portapapeles::TOPE_OSC52;
use super::propiedades::Propiedades;
use super::rutas::EntornoTermux;
use super::teclas::{self, EstadoFila};

/// La orden con la que se comprueba Termux:API: solo lee el estado de la
/// batería, sin efectos (§5.2).
pub const ORDEN_API: &str = "termux-battery-status";

/// Ruta, bajo `$HOME`, del almacenamiento compartido que crea
/// `termux-setup-storage`.
pub const ALMACENAMIENTO: &str = "storage/shared";

/// Prefijo de `TERMUX_VERSION` en el Termux de Google Play (R171).
const VERSION_GOOGLE_PLAY: &str = "googleplay.";

/// Tope de caracteres de un texto de fuera (variable de entorno, destino de un
/// enlace, error de una orden) que se enseña en pantalla.
const TOPE_TEXTO: usize = 200;

/// Lo que se instala para que Termux:API funcione (R170).
const ARREGLO_API: &str = "instala el paquete termux-api («pkg install termux-api») y la app Termux:API, de la misma fuente que Termux";

/// Cómo se pone la fila de teclas de MAGI.
const ARREGLO_TECLAS: &str = "magi termux teclas --instalar";

const MESES: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

// ------------------------------------------------------------------- datos

/// Cómo respondió Termux:API a `termux-battery-status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiObservada {
    /// Respondió bien, en `ms` milisegundos.
    Responde { ms: u64 },
    /// La orden no está en el `PATH`: falta el paquete `termux-api`.
    Ausente,
    /// No respondió en el plazo (falta la app Termux:API o está parada).
    SinRespuesta { plazo_ms: u64 },
    /// Falló: un código distinto de 0 o no se pudo lanzar. El motivo, en una
    /// línea.
    Falla { motivo: String },
}

/// El almacenamiento compartido (`~/storage/shared`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlmacenamientoObservado {
    /// No existe: falta `termux-setup-storage`.
    Ausente,
    /// Existe. `destino` es a dónde apunta el enlace (`None` si es un
    /// directorio normal) y `accesible`, si se puede abrir.
    Presente {
        destino: Option<String>,
        accesible: bool,
    },
}

/// El fichero del llavero (`llavero.age`), sin abrirlo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlaveroObservado {
    /// No existe todavía: se crea al guardar el primer secreto.
    SinCrear,
    /// Existe. `modificado` es el día de su última escritura, en la hora
    /// local (`None` si no se pudo saber o no es una fecha razonable). No es
    /// el de su creación: en Android `Metadata::created()` no está soportado
    /// y `llavero.age` se reescribe entero, de forma atómica, en cada guardado.
    Fichero { modificado: Option<NaiveDate> },
    /// Existe pero no se pudo mirar (no es un fichero normal, permisos).
    NoLegible(String),
}

/// Lo que se observa de Termux (§6.5). Se construye con [`observar`] en
/// producción y a mano en las pruebas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatosTermux {
    /// `TERMUX_VERSION`; el de Google Play empieza por `googleplay.`.
    pub version_termux: Option<String>,
    /// `ro.build.version.release` («15»).
    pub android_release: Option<String>,
    /// `ro.build.version.sdk` («35»).
    pub android_sdk: Option<u32>,
    /// Arquitectura del binario («aarch64»).
    pub arquitectura: String,
    pub api: ApiObservada,
    pub almacenamiento: AlmacenamientoObservado,
    /// Cómo está la fila de teclas en `termux.properties`.
    pub teclas: EstadoFila,
    /// El `termux.properties` que usa Termux (aunque aún no exista).
    pub fichero_teclas: Option<PathBuf>,
    /// La marca `wake-lock` del estado: MAGI tiene tomado el wake lock.
    pub wake_lock_tomado: bool,
    /// El fichero `servidor` del estado dice que Termux:API está apagada en
    /// el servidor.
    pub api_apagada_en_servidor: bool,
    pub llavero: LlaveroObservado,
    /// Desde Termux no se puede saber si Android limita los procesos en
    /// segundo plano ni si Termux está exento de la optimización de batería:
    /// con la bandera puesta, esas dos líneas salen como información (`·`).
    pub ajustes_android_sin_comprobar: bool,
}

// ---------------------------------------------------------------- recogida

/// Observa Termux sin modificar nada. Todo el entorno se inyecta: las rutas
/// y el `PATH` (`entorno`), las propiedades de Android, las variables del
/// proceso (`TERMUX_VERSION`) y el `plazo` de `termux-battery-status`
/// (`[termux] api_plazo_seg`). Bloquea hasta ese plazo como mucho: es una
/// orden de línea de comandos, nunca se llama desde la interfaz.
pub fn observar(
    rutas: &Rutas,
    config: &Config,
    entorno: &EntornoTermux,
    propiedades: &Propiedades,
    variables: &BTreeMap<String, String>,
    plazo: Duration,
) -> DatosTermux {
    let ordenes = Ordenes::nuevas(entorno, plazo, Arc::new(ApiProceso::default()));
    // Sin función de cancelación `observar_api` nunca devuelve `None`; si lo
    // hiciera, sería una orden que no llegó a terminar.
    let api = observar_api(&ordenes, plazo, None).unwrap_or_else(|| ApiObservada::Falla {
        motivo: "la orden no terminó".to_string(),
    });
    armar(rutas, config, entorno, propiedades, variables, api)
}

/// Lo mismo que [`observar`], para la línea de comandos: la espera a Termux:API
/// se puede interrumpir con Ctrl+C (`SIGINT`), `SIGTERM` o `SIGHUP`.
///
/// Sin esto, la señal mataría a `magi` sin pasar por el `Drop` que mata a
/// `termux-battery-status`: cada orden corre en su propio grupo de procesos (el
/// Ctrl+C de la terminal solo llega al de `magi`) y quedaría huérfana, con sus
/// hijos, hasta que ella sola terminara. Con Termux:API parada, que es justo lo
/// que el diagnóstico quiere detectar, eso es lo normal.
///
/// Escribe una línea en la salida de errores («comprobando Termux:API…») antes
/// de esperar, para que quien mira sepa a qué se espera, y solo durante esa
/// espera cambia lo que hacen las señales (después las deja como estaban). Si
/// llega una, mata la orden y su grupo, recoge al hijo y devuelve el
/// [`Interrumpido`] con el código de salida que le toca.
pub fn observar_interrumpible(
    rutas: &Rutas,
    config: &Config,
    entorno: &EntornoTermux,
    propiedades: &Propiedades,
    variables: &BTreeMap<String, String>,
    plazo: Duration,
) -> Result<DatosTermux, Interrumpido> {
    let ordenes = Ordenes::nuevas(entorno, plazo, Arc::new(ApiProceso::default()));
    // Sin ventana de error (terminal cerrada) no hay a quién avisar: se ignora.
    let _ = writeln!(std::io::stderr(), "{MENSAJE_COMPROBANDO_API}");
    let api = {
        let _guarda = GuardaSenales::instalar();
        observar_api(&ordenes, plazo, Some(&|| senal_recibida().is_some()))
    };
    // Una señal que llegó justo al terminar también cuenta: quien la mandó
    // quería parar.
    if let Some(senal) = senal_recibida() {
        return Err(Interrumpido { senal });
    }
    match api {
        Some(api) => Ok(armar(rutas, config, entorno, propiedades, variables, api)),
        // Cancelada sin señal: no puede ocurrir (solo la cancela la señal).
        None => Err(Interrumpido {
            senal: Signal::SIGINT as i32,
        }),
    }
}

/// Lo que se escribe en la salida de errores antes de esperar a Termux:API.
pub const MENSAJE_COMPROBANDO_API: &str = "comprobando Termux:API…";

/// Todo lo que se observa de Termux salvo la propia llamada a Termux:API.
fn armar(
    rutas: &Rutas,
    config: &Config,
    entorno: &EntornoTermux,
    propiedades: &Propiedades,
    variables: &BTreeMap<String, String>,
    api: ApiObservada,
) -> DatosTermux {
    DatosTermux {
        version_termux: variables
            .get("TERMUX_VERSION")
            .map(|version| sanear(version, 40))
            .filter(|version| !version.is_empty()),
        android_release: propiedades
            .version_android()
            .map(|release| sanear(&release, 40))
            .filter(|release| !release.is_empty()),
        android_sdk: propiedades.sdk(),
        arquitectura: std::env::consts::ARCH.to_string(),
        api,
        almacenamiento: observar_almacenamiento(&rutas.hogar),
        teclas: teclas::estado_fila(entorno, &config.prefijo_escape),
        fichero_teclas: teclas::ruta_properties(entorno),
        wake_lock_tomado: estado::hay_marca(rutas, Marca::WakeLock),
        api_apagada_en_servidor: estado::leer_servidor(rutas)
            .is_some_and(|servidor| servidor.avisos.contains(&AvisoServidor::ApiApagada)),
        llavero: observar_llavero(&rutas.datos.join(NOMBRE_FICHERO)),
        ajustes_android_sin_comprobar: true,
    }
}

/// Llama a `termux-battery-status` con el plazo de las órdenes. Su salida (el
/// estado de la batería) no se mira ni se guarda: solo importa que responda.
/// `None` si `cancelada` pidió parar a medias.
fn observar_api(
    ordenes: &Ordenes,
    plazo: Duration,
    cancelada: Option<&dyn Fn() -> bool>,
) -> Option<ApiObservada> {
    let inicio = Instant::now();
    let resultado = match cancelada {
        Some(cancelada) => match ordenes.ejecutar_cancelable(&[ORDEN_API], None, cancelada) {
            Ejecucion::Terminada(resultado) => resultado,
            Ejecucion::Cancelada => return None,
        },
        None => ordenes.ejecutar(&[ORDEN_API], None),
    };
    Some(match resultado {
        Ok(_) => ApiObservada::Responde {
            ms: milisegundos(inicio.elapsed()),
        },
        Err(FalloOrden::NoInstalada(_)) => ApiObservada::Ausente,
        Err(FalloOrden::SinRespuesta) => ApiObservada::SinRespuesta {
            plazo_ms: milisegundos(plazo),
        },
        Err(fallo) => ApiObservada::Falla {
            motivo: sanear(&fallo.to_string(), TOPE_TEXTO),
        },
    })
}

// ---------------------------------------------------------------- señales

/// La señal que interrumpió la espera (0: ninguna). Solo se escribe desde el
/// manejador de señales, con una operación atómica.
static SENAL_RECIBIDA: AtomicI32 = AtomicI32::new(0);

/// Las señales con las que se para a un proceso desde la terminal: Ctrl+C, la
/// orden `kill` y el cierre de la sesión de Termux.
const SENALES_DE_PARADA: [Signal; 3] = [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP];

/// El manejador: solo apunta qué señal llegó. Un `store` atómico es de lo poco
/// que se puede hacer en un manejador de señales (no reserva memoria ni toma
/// cerrojos).
extern "C" fn al_recibir_senal(senal: nix::libc::c_int) {
    SENAL_RECIBIDA.store(senal, Ordering::SeqCst);
}

fn senal_recibida() -> Option<i32> {
    match SENAL_RECIBIDA.load(Ordering::SeqCst) {
        0 => None,
        senal => Some(senal),
    }
}

/// Mientras vive, las señales de parada solo apuntan que llegaron
/// ([`senal_recibida`]); al soltarla, cada una vuelve a lo que hacía.
struct GuardaSenales {
    previas: Vec<(Signal, SigAction)>,
}

impl GuardaSenales {
    fn instalar() -> Self {
        SENAL_RECIBIDA.store(0, Ordering::SeqCst);
        let accion = SigAction::new(
            SigHandler::Handler(al_recibir_senal),
            SaFlags::empty(),
            SigSet::empty(),
        );
        let previas = SENALES_DE_PARADA
            .into_iter()
            .filter_map(|senal| {
                // SAFETY: el manejador solo hace un `store` atómico, que es
                // seguro dentro de un manejador de señales, y `accion` vive
                // lo que dura la llamada.
                unsafe { sigaction(senal, &accion) }
                    .ok()
                    .map(|previa| (senal, previa))
            })
            .collect();
        Self { previas }
    }
}

impl Drop for GuardaSenales {
    fn drop(&mut self) {
        for (senal, previa) in &self.previas {
            // SAFETY: se restituye la disposición que había antes de
            // instalar la nuestra, tal como la devolvió el sistema.
            let _ = unsafe { sigaction(*senal, previa) };
        }
    }
}

/// `magi diagnostico` se interrumpió con una señal mientras esperaba a
/// Termux:API: no hay informe que imprimir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interrumpido {
    /// La señal recibida (`SIGINT` = 2).
    pub senal: i32,
}

impl Interrumpido {
    /// El código de salida de un proceso parado por una señal, como lo cuenta
    /// una shell: 128 + el número de la señal (130 con Ctrl+C).
    #[must_use]
    pub fn codigo_salida(self) -> i32 {
        128 + self.senal
    }
}

fn milisegundos(duracion: Duration) -> u64 {
    u64::try_from(duracion.as_millis()).unwrap_or(u64::MAX)
}

/// `~/storage/shared` y, si es un enlace, a dónde apunta. Solo mira: abrir el
/// directorio es lo más que se hace.
fn observar_almacenamiento(hogar: &Path) -> AlmacenamientoObservado {
    let ruta = hogar.join(ALMACENAMIENTO);
    let Ok(metadatos) = std::fs::symlink_metadata(&ruta) else {
        return AlmacenamientoObservado::Ausente;
    };
    let destino = metadatos
        .file_type()
        .is_symlink()
        .then(|| std::fs::read_link(&ruta).ok())
        .flatten()
        .map(|destino| sanear(&destino.to_string_lossy(), TOPE_TEXTO));
    // `metadata` sigue el enlace: un enlace roto o sin permiso no es accesible.
    let accesible = std::fs::metadata(&ruta).is_ok_and(|datos| datos.is_dir())
        && std::fs::read_dir(&ruta).is_ok();
    AlmacenamientoObservado::Presente { destino, accesible }
}

/// Mira `llavero.age` sin abrirlo: nunca lee su contenido, nunca gasta scrypt
/// ni pide la frase. Un fichero con basura sale como cualquier otro.
fn observar_llavero(ruta: &Path) -> LlaveroObservado {
    match std::fs::metadata(ruta) {
        // Solo se mira cuándo se modificó: la fecha de creación no existe en
        // Android (`Metadata::created()` da `Unsupported`) y el fichero se
        // reescribe entero en cada guardado.
        Ok(metadatos) if metadatos.is_file() => LlaveroObservado::Fichero {
            modificado: metadatos.modified().ok().and_then(dia_local),
        },
        Ok(_) => LlaveroObservado::NoLegible("no es un fichero normal".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LlaveroObservado::SinCrear,
        Err(error) => LlaveroObservado::NoLegible(error.kind().to_string()),
    }
}

/// Primer segundo (UTC) que se da por una fecha razonable: el año 1 (antes, la
/// conversión a hora local podría desbordar el calendario).
const SEGUNDO_MINIMO: i64 = -62_135_596_800;

/// Último segundo (UTC) que se da por una fecha razonable: el final del año
/// 9999, muy lejos de los ±262 000 años que admite el calendario de `chrono`.
const SEGUNDO_MAXIMO: i64 = 253_402_300_799;

/// El día, en la hora local, de un instante. `None` si el instante no se puede
/// representar o no es una fecha razonable (años 1 a 9999): la fecha de un
/// fichero es entrada no fiable (`touch -d @9000000000000`) y convertirla sin
/// comprobar, con `DateTime::from(SystemTime)`, entra en pánico.
fn dia_local(momento: SystemTime) -> Option<NaiveDate> {
    let segundos = match momento.duration_since(UNIX_EPOCH) {
        Ok(despues) => i64::try_from(despues.as_secs()).ok()?,
        // Antes de 1970: `duration_since` da cuánto falta para la época; con
        // fracción de segundo, el día es el del segundo anterior.
        Err(antes) => {
            let antes = antes.duration();
            let enteros = i64::try_from(antes.as_secs()).ok()?;
            -enteros - i64::from(antes.subsec_nanos() > 0)
        }
    };
    if !(SEGUNDO_MINIMO..=SEGUNDO_MAXIMO).contains(&segundos) {
        return None;
    }
    let utc = DateTime::<Utc>::from_timestamp(segundos, 0)?;
    Some(utc.with_timezone(&Local).date_naive())
}

/// Un texto de fuera en una sola línea: sin caracteres de control ni de
/// dirección de texto y de `tope` caracteres como mucho.
fn sanear(texto: &str, tope: usize) -> String {
    let limpio: String = texto
        .chars()
        .map(|caracter| {
            if caracter.is_control()
                || matches!(caracter, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
            {
                ' '
            } else {
                caracter
            }
        })
        .take(tope)
        .collect();
    limpio.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ------------------------------------------------------------ interpretación

/// Qué ajustar en Android para que no mate los procesos de Termux (§7.9):
/// nada antes de Android 12 (SDK 31); las órdenes de `adb` de Android 12 y de
/// 12L o 13; y, desde Android 14, un ajuste de las opciones de desarrollador.
#[must_use]
pub fn segundo_plano(sdk: u32) -> Option<&'static str> {
    match sdk {
        0..=30 => None,
        31 => Some(
            "adb shell device_config set_sync_disabled_for_tests persistent · \
             adb shell device_config put activity_manager max_phantom_processes 2147483647",
        ),
        32..=33 => {
            Some("adb shell settings put global settings_enable_monitor_phantom_procs false")
        }
        _ => Some("Opciones de desarrollador → «Desactivar restricciones de procesos secundarios»"),
    }
}

/// La línea «Termux»: `!` con el Termux de Google Play (no soportado, R171) y
/// `✓` con cualquier otro.
#[must_use]
pub fn linea_termux(
    version: Option<&str>,
    release: Option<&str>,
    sdk: Option<u32>,
    arquitectura: &str,
) -> Comprobacion {
    let google_play = version.is_some_and(|version| version.starts_with(VERSION_GOOGLE_PLAY));
    let sdk = sdk.map_or_else(|| "?".to_string(), |sdk| sdk.to_string());
    let base = format!(
        "{} · Android {} (API {sdk}) · {arquitectura}",
        version.unwrap_or("?"),
        release.unwrap_or("?"),
    );
    if google_play {
        Comprobacion::aviso(
            "Termux",
            format!("{base} · no soportado: usa el Termux de F-Droid o GitHub"),
            None,
        )
    } else {
        Comprobacion::correcto("Termux", base)
    }
}

/// El bloque TERMUX, en el orden de §6.5.
#[must_use]
pub fn bloque(datos: &Datos, termux: &DatosTermux, config: &Config) -> Vec<Comprobacion> {
    let hogar = datos.rutas.hogar.as_path();
    let mut lineas = vec![
        linea_termux(
            termux.version_termux.as_deref(),
            termux.android_release.as_deref(),
            termux.android_sdk,
            &termux.arquitectura,
        ),
        linea_api(&termux.api),
        linea_almacenamiento(&termux.almacenamiento, hogar),
        linea_teclas(termux, hogar),
        linea_wake_lock(datos, termux, config),
        linea_avisos(datos, termux, config),
    ];
    if termux.ajustes_android_sin_comprobar {
        lineas.extend(linea_segundo_plano(termux.android_sdk));
        lineas.push(linea_bateria());
    }
    lineas
}

fn linea_api(api: &ApiObservada) -> Comprobacion {
    match api {
        ApiObservada::Responde { ms } => {
            Comprobacion::correcto("Termux:API", format!("responde ({})", segundos(*ms)))
        }
        ApiObservada::Ausente => Comprobacion::aviso(
            "Termux:API",
            format!("falta el paquete termux-api (no hay {ORDEN_API})"),
            Some(ARREGLO_API.to_string()),
        ),
        ApiObservada::SinRespuesta { plazo_ms } => Comprobacion::aviso(
            "Termux:API",
            format!("no responde en {}", segundos(*plazo_ms)),
            Some(ARREGLO_API.to_string()),
        ),
        ApiObservada::Falla { motivo } => Comprobacion::aviso(
            "Termux:API",
            format!("falla: {motivo}"),
            Some(ARREGLO_API.to_string()),
        ),
    }
}

fn linea_almacenamiento(almacenamiento: &AlmacenamientoObservado, hogar: &Path) -> Comprobacion {
    let ruta = ruta_legible(&hogar.join(ALMACENAMIENTO), hogar);
    match almacenamiento {
        AlmacenamientoObservado::Ausente => Comprobacion::aviso(
            "almacenamiento",
            format!("{ruta} no existe"),
            Some("ejecuta termux-setup-storage".to_string()),
        ),
        AlmacenamientoObservado::Presente {
            destino,
            accesible: true,
        } => Comprobacion::correcto(
            "almacenamiento",
            match destino {
                Some(destino) => format!("{ruta} → {destino}"),
                None => ruta,
            },
        ),
        AlmacenamientoObservado::Presente {
            destino,
            accesible: false,
        } => Comprobacion::aviso(
            "almacenamiento",
            match destino {
                Some(destino) => format!("{ruta} → {destino}: sin acceso"),
                None => format!("{ruta}: sin acceso"),
            },
            Some("ejecuta termux-setup-storage y concede el permiso a Termux".to_string()),
        ),
    }
}

/// La fila de teclas: `✓` si es la de MAGI y `·` en cualquier otro caso (la
/// fila es opcional: nunca es un error ni un aviso).
fn linea_teclas(termux: &DatosTermux, hogar: &Path) -> Comprobacion {
    let fichero = termux.fichero_teclas.as_deref().map_or_else(
        || "termux.properties".to_string(),
        |ruta| ruta_legible(ruta, hogar),
    );
    let instalar = Some(ARREGLO_TECLAS.to_string());
    match &termux.teclas {
        EstadoFila::LaDeMagi => {
            Comprobacion::correcto("teclas", format!("la fila de MAGI ({fichero})"))
        }
        EstadoFila::SinFichero => Comprobacion::info(
            "teclas",
            format!("no hay {fichero}: Termux usa su fila"),
            instalar,
        ),
        EstadoFila::SinExtraKeys => Comprobacion::info(
            "teclas",
            format!("{fichero} sin extra-keys: Termux usa su fila"),
            instalar,
        ),
        EstadoFila::Otra => Comprobacion::info(
            "teclas",
            format!("tu propia fila en {fichero} (MAGI no la toca sin preguntar)"),
            instalar,
        ),
        EstadoFila::Ilegible(motivo) => Comprobacion::info(
            "teclas",
            format!("{fichero} no se puede leer ({})", sanear(motivo, 120)),
            None,
        ),
    }
}

/// Lo que el diagnóstico sabe del servidor para leer las marcas del estado: el
/// fichero de avisos y la marca del wake lock solo dicen algo del servidor que
/// los escribió, y de un servidor que no contesta como es debido (de otra
/// versión, o ocupado) no se sabe si sigue ahí.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServidorSabido {
    /// Contestó como un servidor de esta versión.
    EnMarcha,
    /// No hay socket: se sabe que no hay servidor.
    Ausente,
    /// El socket existe, pero no se pudo comprobar. Dice por qué.
    Incierto(&'static str),
}

fn servidor_sabido(servidor: &ServidorObservado) -> ServidorSabido {
    match servidor {
        ServidorObservado::EnMarcha { .. } => ServidorSabido::EnMarcha,
        ServidorObservado::Apagado => ServidorSabido::Ausente,
        ServidorObservado::OtraVersion { .. } => ServidorSabido::Incierto("es de otra versión"),
        ServidorObservado::SinRespuesta => {
            ServidorSabido::Incierto("no responde (puede estar ocupado)")
        }
    }
}

/// El wake lock: el modo de `[termux] wake_lock` y si MAGI lo tiene tomado (la
/// marca del estado). Una marca sin servidor es de uno que murió sin soltarlo:
/// lo suelta el siguiente. Solo se dice eso cuando se SABE que no hay servidor
/// (sin socket): si no contesta o es de otra versión, puede seguir vivo y con
/// trabajo, y no se recomienda soltar nada. Con `nunca`, una marca es el rastro
/// de un wake lock que MAGI tomó antes y que sigue sin soltarse.
fn linea_wake_lock(datos: &Datos, termux: &DatosTermux, config: &Config) -> Comprobacion {
    let valor = config.termux.wake_lock.trim();
    let tomado = termux.wake_lock_tomado;
    let servidor = servidor_sabido(&datos.servidor);
    let suelta_el_siguiente =
        "abre MAGI: el siguiente servidor lo suelta (o ejecuta termux-wake-unlock)".to_string();
    let mut linea = if valor == "nunca" {
        match (tomado, servidor) {
            (false, _) => (
                Estado::Correcto,
                "nunca · MAGI no toca el wake lock ([termux] wake_lock)".to_string(),
                None,
            ),
            (true, ServidorSabido::Ausente) => (
                Estado::Aviso,
                "nunca · MAGI no lo toma, pero un servidor anterior dejó uno tomado".to_string(),
                Some(suelta_el_siguiente),
            ),
            (true, _) => (
                Estado::Info,
                "nunca · MAGI no lo toma, pero queda la marca de uno que tomó antes".to_string(),
                Some(
                    "reinicia el servidor (magi servidor parar): al arrancar lo suelta".to_string(),
                ),
            ),
        }
    } else {
        match (tomado, servidor) {
            (true, ServidorSabido::EnMarcha) => (
                Estado::Correcto,
                "auto · tomado ahora (hay trabajo o lo hubo hace nada)".to_string(),
                None,
            ),
            (true, ServidorSabido::Ausente) => (
                Estado::Aviso,
                "auto · tomado por un servidor que ya no está".to_string(),
                Some(suelta_el_siguiente),
            ),
            (true, ServidorSabido::Incierto(motivo)) => (
                Estado::Info,
                format!(
                    "auto · tomado; el servidor {motivo}: no se puede saber si sigue con trabajo"
                ),
                None,
            ),
            (false, _) => (
                Estado::Correcto,
                "auto · ahora sin trabajo".to_string(),
                None,
            ),
        }
    };
    if !matches!(valor, "auto" | "nunca") {
        linea = combinar(
            linea,
            (
                Estado::Aviso,
                "[termux] wake_lock no vale: se usa «auto»".to_string(),
                Some("pon «auto» o «nunca» en [termux] wake_lock de config.toml".to_string()),
            ),
        );
    }
    Comprobacion {
        nombre: "wake lock",
        estado: linea.0,
        detalle: linea.1,
        arreglo: linea.2,
        nota: None,
    }
}

/// Los avisos de Android: activados o desactivados en la configuración, sin
/// Termux:API (entonces solo salen en la barra de MAGI) y Termux:API apagada en
/// el servidor (que entonces no puede publicarlos).
fn linea_avisos(datos: &Datos, termux: &DatosTermux, config: &Config) -> Comprobacion {
    if !config.termux.avisos {
        return Comprobacion::correcto("avisos", "desactivados ([termux] avisos)".to_string());
    }
    // Sin Termux:API no hay avisos de Android, estén activados o no. El aviso
    // ya lo da la línea de Termux:API, con su arreglo: aquí solo se informa de
    // lo que eso cambia, sin contarlo dos veces.
    if !matches!(termux.api, ApiObservada::Responde { .. }) {
        return Comprobacion::info(
            "avisos",
            "activados, pero sin Termux:API solo salen en la barra de MAGI".to_string(),
            None,
        );
    }
    // El fichero del servidor solo vale mientras el servidor está en marcha:
    // lo borra al terminar y el siguiente lo reescribe sin avisos.
    let servidor_vivo = servidor_sabido(&datos.servidor) == ServidorSabido::EnMarcha;
    if termux.api_apagada_en_servidor && servidor_vivo {
        return Comprobacion::aviso(
            "avisos",
            "activados, pero Termux:API está apagada en el servidor: sin avisos de Android"
                .to_string(),
            Some(
                "arregla Termux:API y para el servidor (magi servidor parar) para que vuelva a probarla"
                    .to_string(),
            ),
        );
    }
    Comprobacion::correcto("avisos", "activados".to_string())
}

/// Lo que Android puede hacer con los procesos de Termux en segundo plano
/// (R166): información, nunca un aviso. Antes de Android 12 no hay límite y
/// no sale la línea.
fn linea_segundo_plano(sdk: Option<u32>) -> Option<Comprobacion> {
    let (cuando, ajuste) = match sdk {
        Some(sdk) => {
            let ajuste = segundo_plano(sdk)?;
            let cuando = match sdk {
                31 => "En Android 12:",
                32..=33 => "En Android 12L y 13:",
                _ => "En Android 14 o posterior:",
            };
            (cuando, ajuste)
        }
        // Sin la versión de Android no se sabe cuál toca: se da el ajuste
        // de las versiones nuevas y se dice que no se pudo leer.
        None => (
            "No se pudo leer tu versión de Android. En Android 14 o posterior:",
            segundo_plano(u32::MAX)?,
        ),
    };
    let ajuste = ajuste.replace(" · ", "\n");
    Some(
        Comprobacion::info(
            "segundo plano",
            format!("Android 12+ puede cerrar procesos de Termux. {cuando}"),
            None,
        )
        .con_nota(Some(ajuste)),
    )
}

fn linea_bateria() -> Comprobacion {
    Comprobacion::info(
        "batería",
        "quita la optimización de batería de Termux".to_string(),
        None,
    )
    .con_nota(Some(
        "(y, según el fabricante, permite su actividad en segundo plano)".to_string(),
    ))
}

/// El llavero de Termux: el fichero con frase maestra. Solo se sabe si existe
/// y cuándo se modificó por última vez (es lo que se puede saber sin abrirlo:
/// la frase no se pide ni el fichero se abre); la fecha de creación no la da
/// Android.
#[must_use]
pub fn llavero(termux: &DatosTermux) -> Comprobacion {
    match &termux.llavero {
        LlaveroObservado::SinCrear => Comprobacion::correcto(
            "llavero",
            "fichero con frase maestra (sin crear aún)".to_string(),
        ),
        LlaveroObservado::Fichero {
            modificado: Some(dia),
        } => Comprobacion::correcto(
            "llavero",
            format!(
                "fichero con frase maestra (modificado el {})",
                fecha_corta(*dia)
            ),
        ),
        LlaveroObservado::Fichero { modificado: None } => {
            Comprobacion::correcto("llavero", "fichero con frase maestra".to_string())
        }
        LlaveroObservado::NoLegible(motivo) => Comprobacion::aviso(
            "llavero",
            format!("llavero.age no se puede leer: {}", sanear(motivo, 120)),
            Some("mira «magi llavero estado»".to_string()),
        ),
    }
}

/// El portapapeles de Termux: `termux-clipboard-set` si Termux:API responde y,
/// si no, OSC 52 con su tope.
#[must_use]
pub fn portapapeles(termux: &DatosTermux) -> Comprobacion {
    match termux.api {
        ApiObservada::Responde { .. } => Comprobacion::correcto(
            "portapapeles",
            "termux-clipboard-set (Termux:API)".to_string(),
        ),
        ref otra => {
            // El tope de OSC 52 está en base64: el texto que cabe es 3/4.
            let kib = TOPE_OSC52 / 4 * 3 / 1024;
            let motivo = if *otra == ApiObservada::Ausente {
                "sin Termux:API"
            } else {
                "Termux:API no responde"
            };
            Comprobacion::correcto(
                "portapapeles",
                format!("OSC 52 (hasta unos {kib} KiB de texto) · {motivo}"),
            )
        }
    }
}

/// La línea «terminal»: en Termux no hay otra ventana que abrir.
#[must_use]
pub fn terminal() -> Comprobacion {
    Comprobacion::info(
        "terminal",
        "en Termux no aplica: abre otra sesión de Termux para otra ventana".to_string(),
        None,
    )
}

/// Una ruta como la enseña el diagnóstico en Termux: lo que cuelga de
/// `$TMPDIR` va como `$TMPDIR/…` (el directorio del socket es
/// `$TMPDIR/magi-<uid>`) y lo demás, como en el escritorio.
#[must_use]
pub fn ruta_con_tmpdir(ruta: &Path, datos: &Datos) -> String {
    let tmpdir = datos
        .variables
        .get("TMPDIR")
        .map(Path::new)
        .filter(|tmpdir| tmpdir.is_absolute());
    if let Some(resto) = tmpdir.and_then(|tmpdir| ruta.strip_prefix(tmpdir).ok()) {
        return if resto.as_os_str().is_empty() {
            "$TMPDIR".to_string()
        } else {
            format!("$TMPDIR/{}", resto.display())
        };
    }
    ruta_legible(ruta, &datos.rutas.hogar)
}

/// «17 nov 2026».
fn fecha_corta(dia: NaiveDate) -> String {
    let mes = MESES.get(dia.month0() as usize).copied().unwrap_or("???");
    format!("{} {mes} {}", dia.day(), dia.year())
}

/// Una duración en segundos con una cifra decimal y coma: «0,4 s», «5 s».
fn segundos(ms: u64) -> String {
    if ms < 50 {
        return "menos de 0,1 s".to_string();
    }
    if ms % 1000 == 0 {
        return format!("{} s", ms / 1000);
    }
    let decimas = (ms + 50) / 100;
    format!("{},{} s", decimas / 10, decimas % 10)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn segundo_plano_por_version_de_android() {
        for sdk in [24, 29, 30] {
            assert_eq!(
                segundo_plano(sdk),
                None,
                "SDK {sdk}: sin límite de procesos"
            );
        }
        let doce = segundo_plano(31).expect("Android 12");
        assert!(
            doce.contains("set_sync_disabled_for_tests persistent")
                && doce.contains("max_phantom_processes 2147483647"),
            "{doce}"
        );
        for sdk in [32, 33] {
            assert_eq!(
                segundo_plano(sdk),
                Some("adb shell settings put global settings_enable_monitor_phantom_procs false"),
                "SDK {sdk}"
            );
        }
        for sdk in [34, 35, 99] {
            assert_eq!(
                segundo_plano(sdk),
                Some("Opciones de desarrollador → «Desactivar restricciones de procesos secundarios»"),
                "SDK {sdk}"
            );
        }
    }

    #[test]
    fn la_linea_de_termux_marca_el_de_google_play() {
        let bueno = linea_termux(Some("0.118.3"), Some("15"), Some(35), "aarch64");
        assert_eq!(bueno.estado, Estado::Correcto);
        assert_eq!(bueno.detalle, "0.118.3 · Android 15 (API 35) · aarch64");
        assert_eq!(bueno.arreglo, None);

        let play = linea_termux(
            Some("googleplay.2026.11.1"),
            Some("15"),
            Some(35),
            "aarch64",
        );
        assert_eq!(play.estado, Estado::Aviso);
        assert!(
            play.detalle
                .ends_with(" · no soportado: usa el Termux de F-Droid o GitHub"),
            "{}",
            play.detalle
        );

        let sin_nada = linea_termux(None, None, None, "aarch64");
        assert_eq!(sin_nada.estado, Estado::Correcto);
        assert_eq!(sin_nada.detalle, "? · Android ? (API ?) · aarch64");
    }

    #[test]
    fn las_duraciones_van_con_una_cifra_y_coma() {
        assert_eq!(segundos(0), "menos de 0,1 s");
        assert_eq!(segundos(49), "menos de 0,1 s");
        assert_eq!(segundos(50), "0,1 s");
        assert_eq!(segundos(380), "0,4 s");
        assert_eq!(segundos(1960), "2,0 s");
        assert_eq!(segundos(5000), "5 s");
        assert_eq!(segundos(12_345), "12,3 s");
    }

    #[test]
    fn la_fecha_del_llavero_es_corta_y_en_castellano() {
        let dia = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        assert_eq!(fecha_corta(dia), "17 nov 2026");
        let dia = NaiveDate::from_ymd_opt(2027, 1, 3).unwrap();
        assert_eq!(fecha_corta(dia), "3 ene 2027");
        let dia = NaiveDate::from_ymd_opt(2027, 12, 31).unwrap();
        assert_eq!(fecha_corta(dia), "31 dic 2027");
    }

    #[test]
    fn la_fecha_de_un_fichero_fuera_de_rango_no_entra_en_panico() {
        // `touch -d @9000000000000`: el año 287168, más allá de los ±262 143
        // años del calendario. Convertirlo con `DateTime::from(SystemTime)`
        // entra en pánico.
        let lejos = UNIX_EPOCH + Duration::from_secs(9_000_000_000_000);
        assert_eq!(dia_local(lejos), None);
        let muy_antes = UNIX_EPOCH - Duration::from_secs(9_000_000_000_000);
        assert_eq!(dia_local(muy_antes), None);
        // Lo más lejos que llegue un `SystemTime` y los límites de lo razonable.
        if let Some(maximo) = UNIX_EPOCH.checked_add(Duration::from_secs(i64::MAX as u64)) {
            assert_eq!(dia_local(maximo), None);
        }
        let pasado_el_9999 = UNIX_EPOCH + Duration::from_secs(253_402_300_800);
        assert_eq!(dia_local(pasado_el_9999), None);
        let antes_del_1 = UNIX_EPOCH - Duration::from_secs(62_135_596_801);
        assert_eq!(dia_local(antes_del_1), None);

        // Lo razonable sigue dando su día (±1 por la zona horaria).
        let cerca = |momento: SystemTime, esperado: (i32, u32, u32)| {
            let dia = dia_local(momento).expect("una fecha razonable");
            let esperado = NaiveDate::from_ymd_opt(esperado.0, esperado.1, esperado.2).unwrap();
            assert!(
                (dia - esperado).num_days().abs() <= 1,
                "{dia} lejos de {esperado}"
            );
        };
        // 2026-11-17 12:00:00 UTC.
        cerca(
            UNIX_EPOCH + Duration::from_secs(1_794_916_800),
            (2026, 11, 17),
        );
        cerca(UNIX_EPOCH, (1970, 1, 1));
        // Antes de 1970, con fracción de segundo: 1969-12-31 23:59:59,5 UTC.
        cerca(UNIX_EPOCH - Duration::from_millis(500), (1969, 12, 31));
        // El final del año 9999 sigue siendo representable, sin pánico.
        assert!(dia_local(UNIX_EPOCH + Duration::from_secs(253_402_300_799)).is_some());
    }

    #[test]
    fn el_llavero_con_una_fecha_absurda_no_tira_el_diagnostico() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join("llavero.age");
        std::fs::write(&ruta, b"x").unwrap();
        let lejos = filetime::FileTime::from_unix_time(9_000_000_000_000, 0);
        if filetime::set_file_mtime(&ruta, lejos).is_err() {
            // Un sistema de ficheros que no admite esa fecha no puede darla.
            return;
        }
        // Sin pánico, con o sin fecha según lo que el sistema de ficheros
        // guarde de ella.
        assert!(matches!(
            observar_llavero(&ruta),
            LlaveroObservado::Fichero { .. }
        ));
    }

    #[test]
    fn el_codigo_de_salida_de_una_interrupcion_es_el_de_una_shell() {
        assert_eq!(Interrumpido { senal: 2 }.codigo_salida(), 130);
        assert_eq!(Interrumpido { senal: 15 }.codigo_salida(), 143);
        assert_eq!(Interrumpido { senal: 1 }.codigo_salida(), 129);
    }

    /// La guarda apunta la señal que llega mientras vive y la suelta al caer.
    /// Es la única prueba de este módulo que toca las señales del proceso (la
    /// bandera es global).
    #[test]
    fn la_guarda_de_senales_apunta_la_senal_y_se_deshace_al_soltarla() {
        {
            let _guarda = GuardaSenales::instalar();
            assert_eq!(senal_recibida(), None, "empieza sin señal");
            nix::sys::signal::raise(Signal::SIGTERM).expect("raise");
            // La entrega a un manejador es inmediata en el hilo que la manda
            // (SIGTERM dirigido al propio hilo con `raise`).
            assert_eq!(senal_recibida(), Some(Signal::SIGTERM as i32));
        }
        // Soltada, vuelve a lo de antes: una guarda nueva empieza limpia.
        let _otra = GuardaSenales::instalar();
        assert_eq!(senal_recibida(), None);
    }

    #[test]
    fn el_texto_de_fuera_sale_en_una_linea_y_acotado() {
        assert_eq!(sanear("a\x1b[31mb\n\tc  d", 100), "a [31mb c d");
        assert_eq!(sanear("x\u{202E}y", 100), "x y");
        assert_eq!(sanear(&"ñ".repeat(500), 200).chars().count(), 200);
        assert_eq!(sanear("  \n", 10), "");
    }

    #[test]
    fn el_llavero_se_mira_sin_abrirlo() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join("llavero.age");
        assert_eq!(observar_llavero(&ruta), LlaveroObservado::SinCrear);
        // Con basura dentro es un fichero como otro cualquiera.
        std::fs::write(&ruta, b"esto no es age").unwrap();
        assert!(matches!(
            observar_llavero(&ruta),
            LlaveroObservado::Fichero {
                modificado: Some(_)
            }
        ));
        // Un directorio donde debería haber un fichero no es legible.
        let directorio = temporal.path().join("otro");
        std::fs::create_dir(&directorio).unwrap();
        assert!(matches!(
            observar_llavero(&directorio),
            LlaveroObservado::NoLegible(_)
        ));
    }

    #[test]
    fn el_almacenamiento_distingue_ausente_enlace_y_sin_acceso() {
        let temporal = tempfile::tempdir().unwrap();
        let hogar = temporal.path();
        assert_eq!(
            observar_almacenamiento(hogar),
            AlmacenamientoObservado::Ausente
        );

        std::fs::create_dir(hogar.join("storage")).unwrap();
        let destino = temporal.path().join("emulated");
        std::fs::create_dir(&destino).unwrap();
        std::os::unix::fs::symlink(&destino, hogar.join("storage/shared")).unwrap();
        assert_eq!(
            observar_almacenamiento(hogar),
            AlmacenamientoObservado::Presente {
                destino: Some(destino.to_string_lossy().into_owned()),
                accesible: true,
            }
        );

        // Un enlace roto (permiso retirado, tarjeta fuera) no es accesible.
        std::fs::remove_dir(&destino).unwrap();
        assert!(matches!(
            observar_almacenamiento(hogar),
            AlmacenamientoObservado::Presente {
                accesible: false,
                destino: Some(_)
            }
        ));
    }
}
