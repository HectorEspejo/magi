//! Órdenes de Termux (`termux-*`): sin shell, con plazo, con tope de salida y
//! con apagado por proceso de Termux:API (§5.2, §7.2; D384, T139).
//!
//! Ninguna orden se espera en el bucle de la interfaz: van en tareas tokio o
//! en `spawn_blocking`. Si Termux:API no está o no responde, esa función se
//! apaga en este proceso hasta que reinicie y el aviso sale una sola vez.
//!
//! Cada orden corre en su propio grupo de procesos: al vencer el plazo se mata
//! el grupo entero (no solo el programa, que puede haber lanzado otros) y se
//! recoge al hijo para no dejar zombis. La entrada se escribe desde un hilo
//! aparte (puede llegar a 512 KiB y el hijo puede no leerla) y las dos salidas
//! se leen con tope desde otros dos: quien llama nunca espera más de `plazo`.
//! Lo que devuelve el programa es entrada no fiable: nunca llega al log.

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use super::rutas::EntornoTermux;

/// Tope de la salida que se lee de una orden (entrada no fiable).
pub const TOPE_SALIDA: usize = 64 * 1024;

/// Las órdenes que dependen de la app Termux:API (el resto es de termux-tools).
pub const DE_LA_API: [&str; 4] = [
    "termux-clipboard-set",
    "termux-notification",
    "termux-notification-remove",
    "termux-battery-status",
];

/// Máximo de caracteres del texto de error de una orden (§7.2).
const LARGO_ERROR: usize = 200;

/// Cada cuánto se mira si el hijo ha terminado cuando nada más despierta al
/// hilo que espera (el fin de las salidas lo despierta antes).
const LATIDO: Duration = Duration::from_millis(10);

/// Lo que se espera a que se cierren las salidas una vez terminado el
/// programa: un nieto que las heredó no alarga la orden más que esto.
const GRACIA_TRAS_SALIDA: Duration = Duration::from_millis(200);

/// Lo que se espera a recoger al hijo tras matar su grupo (un SIGKILL no
/// tarda; el tope es solo por si el proceso estuviera atascado en el núcleo).
const ESPERA_TRAS_MATAR: Duration = Duration::from_secs(2);

/// Estado de Termux:API en un proceso (cliente o servidor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EstadoApi {
    #[default]
    SinComprobar,
    Disponible,
    /// Falta el paquete `termux-api` (la orden no está en el `PATH`).
    Ausente,
    /// La orden no responde en el plazo o falla: falta la app o está parada.
    NoResponde,
}

impl EstadoApi {
    pub fn apagada(self) -> bool {
        matches!(self, Self::Ausente | Self::NoResponde)
    }
}

/// El estado de Termux:API de un proceso, compartido entre sus tareas.
#[derive(Debug, Default)]
pub struct ApiProceso {
    estado: Mutex<EstadoApi>,
}

impl ApiProceso {
    /// El cerrojo del estado; uno envenenado se recupera, porque el estado es
    /// un valor suelto y no puede quedar a medias.
    fn cerrojo(&self) -> MutexGuard<'_, EstadoApi> {
        self.estado
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
    }

    pub fn estado(&self) -> EstadoApi {
        *self.cerrojo()
    }

    pub fn apagada(&self) -> bool {
        self.estado().apagada()
    }

    /// Apaga la función con un motivo (`Ausente` o `NoResponde`). Devuelve
    /// `true` solo la primera vez: quien lo recibe da el aviso de la barra.
    /// Un apagado no se revierte hasta que el proceso reinicie, y un motivo
    /// que no es de apagado (`Disponible`, `SinComprobar`) no apaga nada.
    pub fn apagar(&self, motivo: EstadoApi) -> bool {
        if !motivo.apagada() {
            return false;
        }
        let mut estado = self.cerrojo();
        if estado.apagada() {
            return false;
        }
        *estado = motivo;
        true
    }

    /// Marca Termux:API como disponible tras una respuesta correcta. Si ya
    /// estaba apagada, sigue apagada: un apagado no se revierte.
    pub fn marcar_disponible(&self) {
        let mut estado = self.cerrojo();
        if !estado.apagada() {
            *estado = EstadoApi::Disponible;
        }
    }
}

/// Por qué falla una orden de Termux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalloOrden {
    /// Termux:API está apagada en este proceso: ni se intenta.
    ApiApagada,
    /// El programa no está en el `PATH`.
    NoInstalada(String),
    /// No respondió en el plazo (se mató su grupo de procesos).
    SinRespuesta,
    /// Terminó con un código distinto de 0; `error` son 200 caracteres como mucho.
    Codigo { codigo: Option<i32>, error: String },
    /// Un fallo local, que no es de Termux:API: un argumento no válido, no
    /// poder lanzar el programa o crear un hilo. No apaga la API.
    Io(String),
}

impl std::fmt::Display for FalloOrden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiApagada => f.write_str("Termux:API apagada"),
            Self::NoInstalada(programa) => write!(f, "{programa} no está instalado"),
            Self::SinRespuesta => f.write_str("no responde"),
            Self::Codigo { error, .. } if error.is_empty() => f.write_str("la orden falló"),
            Self::Codigo { error, .. } => f.write_str(error),
            Self::Io(causa) => f.write_str(causa),
        }
    }
}

impl std::error::Error for FalloOrden {}

/// Cómo termina una orden cancelable ([`Ordenes::ejecutar_cancelable`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ejecucion {
    /// Terminó (o ni se lanzó) por sí sola: el mismo resultado que
    /// [`Ordenes::ejecutar`].
    Terminada(Result<Vec<u8>, FalloOrden>),
    /// Quien llama la canceló: se mató su grupo de procesos y se recogió al
    /// hijo antes de volver.
    Cancelada,
}

/// Lanzador de órdenes de Termux de un proceso.
#[derive(Debug, Clone)]
pub struct Ordenes {
    path: Option<OsString>,
    plazo: Duration,
    api: Arc<ApiProceso>,
}

impl Ordenes {
    /// `plazo` es `[termux] api_plazo_seg` (2-30 s). El `PATH` sale del
    /// entorno inyectado, nunca de `std::env`.
    pub fn nuevas(entorno: &EntornoTermux, plazo: Duration, api: Arc<ApiProceso>) -> Self {
        Self {
            path: entorno.path.clone(),
            plazo,
            api,
        }
    }

    pub fn api(&self) -> &Arc<ApiProceso> {
        &self.api
    }

    /// Ejecuta `argumentos[0]` con el resto como argumentos sueltos (sin
    /// shell), con `entrada` por la entrada estándar, el plazo y el tope de
    /// salida; al vencer el plazo mata el grupo de procesos. Bloquea el hilo
    /// hasta `plazo`: llámala desde una tarea, nunca desde el bucle de la UI.
    ///
    /// `argumentos[0]` es el nombre de la orden, sin `/`: se busca en el
    /// `PATH` inyectado. Con las órdenes de [`DE_LA_API`], el resultado
    /// actualiza el estado de Termux:API (ausente, no responde o
    /// disponible); las demás nunca lo tocan. Un fallo de E/S propio (un
    /// argumento con NUL, sin recursos para lanzar) vuelve como
    /// [`FalloOrden::Io`] sin apagar nada: no es culpa de la API.
    pub fn ejecutar(
        &self,
        argumentos: &[&str],
        entrada: Option<&[u8]>,
    ) -> Result<Vec<u8>, FalloOrden> {
        self.ejecutar_avisando(argumentos, entrada).0
    }

    /// Como [`Self::ejecutar`], y además dice si esta llamada acaba de apagar
    /// Termux:API (la primera vez en este proceso): quien lo recibe da el
    /// aviso de la barra una sola vez.
    pub fn ejecutar_avisando(
        &self,
        argumentos: &[&str],
        entrada: Option<&[u8]>,
    ) -> (Result<Vec<u8>, FalloOrden>, bool) {
        // Sin función de cancelación nunca sale `None`; si saliera, sería una
        // orden que no llegó a terminar y se dice como un fallo local.
        self.ejecutar_con(argumentos, entrada, None)
            .unwrap_or_else(|| (Err(FalloOrden::Io("orden cancelada".into())), false))
    }

    /// Como [`Self::ejecutar`], pero cancelable: mientras espera a la orden
    /// mira `cancelada` cada pocos milisegundos y, en cuanto devuelve `true`,
    /// mata el grupo de procesos entero, recoge al hijo y vuelve con
    /// [`Ejecucion::Cancelada`]. Una cancelación no es un fallo de Termux:API:
    /// no cambia su estado (ni la apaga ni la da por disponible).
    ///
    /// Es para quien espera a la orden con el usuario delante (`magi
    /// diagnostico`), que puede interrumpirla con una señal: un proceso que
    /// muere sin pasar por aquí dejaría la orden y sus hijos huérfanos, porque
    /// cada orden corre en su propio grupo y el Ctrl+C de la terminal no lo
    /// alcanza. Las demás llamadas no cambian.
    pub fn ejecutar_cancelable(
        &self,
        argumentos: &[&str],
        entrada: Option<&[u8]>,
        cancelada: &dyn Fn() -> bool,
    ) -> Ejecucion {
        match self.ejecutar_con(argumentos, entrada, Some(cancelada)) {
            Some((resultado, _)) => Ejecucion::Terminada(resultado),
            None => Ejecucion::Cancelada,
        }
    }

    /// El cuerpo común: `None` es una orden cancelada (solo con `cancelada`).
    fn ejecutar_con(
        &self,
        argumentos: &[&str],
        entrada: Option<&[u8]>,
        cancelada: Option<&dyn Fn() -> bool>,
    ) -> Option<(Result<Vec<u8>, FalloOrden>, bool)> {
        let Some(&programa) = argumentos.first() else {
            return Some((Err(FalloOrden::Io("orden vacía".into())), false));
        };
        // Ya cancelada antes de empezar: no se lanza nada.
        if cancelada.is_some_and(|cancelada| cancelada()) {
            return None;
        }
        // Un NUL no cabe en un argumento de proceso: el sistema lo rechazaría
        // al lanzar. Es un fallo de quien llama, no de Termux:API, así que se
        // dice antes de tocar nada (ni el estado de la API ni el programa) y
        // sin copiar el argumento, que puede ser texto del usuario.
        if let Some(posicion) = argumentos
            .iter()
            .position(|argumento| argumento.contains('\0'))
        {
            return Some((
                Err(FalloOrden::Io(format!(
                    "el argumento {posicion} lleva un carácter nulo"
                ))),
                false,
            ));
        }
        let de_api = DE_LA_API.contains(&programa);
        if de_api && self.api.apagada() {
            return Some((Err(FalloOrden::ApiApagada), false));
        }
        // Solo una instrucción de la API apaga la función, y solo una vez.
        let apagar = |motivo: EstadoApi| de_api && self.api.apagar(motivo);

        let Some(ruta) = buscar_en_path(programa, self.path.as_deref()) else {
            let recien = apagar(EstadoApi::Ausente);
            return Some((Err(FalloOrden::NoInstalada(programa.to_string())), recien));
        };

        Some(
            match correr(&ruta, &argumentos[1..], entrada, self.plazo, cancelada) {
                Ok(Fin::Salida(salida)) => {
                    if de_api {
                        self.api.marcar_disponible();
                    }
                    (Ok(salida), false)
                }
                Ok(Fin::Codigo { codigo, error }) => (
                    Err(FalloOrden::Codigo { codigo, error }),
                    apagar(EstadoApi::NoResponde),
                ),
                Ok(Fin::PlazoVencido) => {
                    (Err(FalloOrden::SinRespuesta), apagar(EstadoApi::NoResponde))
                }
                // Cancelada por quien llama: no es culpa de Termux:API y no
                // cambia su estado.
                Ok(Fin::Cancelada) => return None,
                // Una carrera con el borrado del programa: es como si no
                // estuviera.
                Err(error @ FalloOrden::NoInstalada(_)) => (Err(error), apagar(EstadoApi::Ausente)),
                // Un fallo al lanzar, al crear un hilo o al esperar (sin
                // recursos, permisos, argumento no válido) no lo causa
                // Termux:API: se informa, pero la función sigue disponible.
                // Solo apagan «no instalada», el plazo vencido y un código
                // distinto de 0 (§7.2).
                Err(error) => (Err(error), false),
            },
        )
    }

    /// Lo mismo desde código asíncrono (en `spawn_blocking`).
    pub async fn ejecutar_async(
        &self,
        argumentos: Vec<String>,
        entrada: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, FalloOrden> {
        self.ejecutar_async_avisando(argumentos, entrada).await.0
    }

    /// Como [`Self::ejecutar_async`], con el aviso de [`Self::ejecutar_avisando`].
    pub async fn ejecutar_async_avisando(
        &self,
        argumentos: Vec<String>,
        entrada: Option<Vec<u8>>,
    ) -> (Result<Vec<u8>, FalloOrden>, bool) {
        let ordenes = self.clone();
        // La entrada puede ser texto del portapapeles: se borra al soltarla.
        let entrada = entrada.map(Zeroizing::new);
        match tokio::task::spawn_blocking(move || {
            let referencias: Vec<&str> = argumentos.iter().map(String::as_str).collect();
            ordenes.ejecutar_avisando(&referencias, entrada.as_deref().map(Vec::as_slice))
        })
        .await
        {
            Ok(resultado) => resultado,
            Err(error) => (Err(FalloOrden::Io(error.to_string())), false),
        }
    }
}

// ------------------------------------------------------------ resolución

/// Busca la orden en el `PATH` inyectado: el primer fichero ejecutable que se
/// llame así. Una orden con `/` no se busca (nunca se ejecuta una ruta que
/// llegue de fuera) y las entradas vacías del `PATH` no cuentan como «aquí».
pub(crate) fn buscar_en_path(programa: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    if programa.is_empty() || programa.contains('/') || programa.contains('\0') {
        return None;
    }
    std::env::split_paths(path?)
        .filter(|directorio| !directorio.as_os_str().is_empty())
        .map(|directorio| directorio.join(programa))
        .find(|candidato| es_ejecutable(candidato))
}

fn es_ejecutable(ruta: &Path) -> bool {
    std::fs::metadata(ruta)
        .map(|datos| datos.is_file() && datos.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

// ------------------------------------------------------------- ejecución

/// Cómo terminó una orden que se llegó a lanzar.
enum Fin {
    Salida(Vec<u8>),
    Codigo {
        codigo: Option<i32>,
        error: String,
    },
    PlazoVencido,
    /// La canceló quien llama (solo con función de cancelación).
    Cancelada,
}

/// Un proceso hijo que no se queda atrás: si se suelta sin haberlo recogido
/// (un error, un pánico), se mata su grupo y se recoge.
struct Vigilado {
    hijo: Child,
    /// El grupo (el hijo es su líder: `process_group(0)`).
    grupo: Option<i32>,
    recogido: bool,
}

impl Vigilado {
    fn nuevo(hijo: Child) -> Self {
        let grupo = i32::try_from(hijo.id()).ok().filter(|pid| *pid > 1);
        Self {
            hijo,
            grupo,
            recogido: false,
        }
    }

    /// SIGKILL a todo el grupo (y, por si el grupo no se pudo señalar, al
    /// hijo). Mientras el hijo no esté recogido su pid no se puede reutilizar.
    fn matar(&mut self) {
        let senalado = self.grupo.is_some_and(|pid| {
            nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            )
            .is_ok()
        });
        if !senalado {
            let _ = self.hijo.kill();
        }
    }

    /// Recoge al hijo si ya ha terminado, sin esperar.
    fn comprobar(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let estado = self.hijo.try_wait()?;
        self.recogido |= estado.is_some();
        Ok(estado)
    }

    /// Espera a recoger al hijo hasta `plazo`.
    fn recoger(&mut self, plazo: Duration) {
        let limite = Instant::now() + plazo;
        while !self.recogido && Instant::now() < limite {
            match self.comprobar() {
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

impl Drop for Vigilado {
    fn drop(&mut self) {
        if !self.recogido {
            self.matar();
            self.recoger(Duration::from_millis(500));
        }
    }
}

/// Lee una tubería hasta el final guardando como mucho `TOPE_SALIDA` bytes
/// (el resto se descarta, pero se sigue leyendo: un hijo que escribe más no se
/// queda bloqueado), y avisa al terminar.
fn leer_con_tope<R: Read>(mut origen: R, destino: &Mutex<Vec<u8>>) {
    let mut trozo = [0u8; 8 * 1024];
    loop {
        match origen.read(&mut trozo) {
            Ok(0) => break,
            Ok(leidos) => {
                let mut guardado = destino
                    .lock()
                    .unwrap_or_else(|envenenado| envenenado.into_inner());
                let sitio = TOPE_SALIDA.saturating_sub(guardado.len());
                guardado.extend_from_slice(&trozo[..leidos.min(sitio)]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

/// Lo que hay guardado en un búfer de lectura.
fn recoger_bufer(bufer: &Mutex<Vec<u8>>) -> Vec<u8> {
    std::mem::take(
        &mut *bufer
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner()),
    )
}

/// Lanza `ruta` y espera su fin, sus salidas, el plazo o, si hay función de
/// cancelación, que devuelva `true` (lo que ocurra antes).
fn correr(
    ruta: &Path,
    argumentos: &[&str],
    entrada: Option<&[u8]>,
    plazo: Duration,
    cancelada: Option<&dyn Fn() -> bool>,
) -> Result<Fin, FalloOrden> {
    let limite = Instant::now() + plazo;
    let mut orden = Command::new(ruta);
    orden
        .args(argumentos)
        .stdin(if entrada.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Grupo propio: al vencer el plazo muere con todo lo que lance.
        .process_group(0);
    let hijo = lanzar(&mut orden).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => FalloOrden::NoInstalada(nombre_de(ruta)),
        _ => FalloOrden::Io(format!("no se pudo lanzar {}: {error}", nombre_de(ruta))),
    })?;
    let mut vigilado = Vigilado::nuevo(hijo);

    let (aviso, recibir) = mpsc::channel::<()>();
    let salida = Arc::new(Mutex::new(Vec::new()));
    let errores = Arc::new(Mutex::new(Vec::new()));
    let mut lectores = 0usize;

    if let Some(tuberia) = vigilado.hijo.stdout.take() {
        lanzar_lector(tuberia, &salida, &aviso, "salida")?;
        lectores += 1;
    }
    if let Some(tuberia) = vigilado.hijo.stderr.take() {
        lanzar_lector(tuberia, &errores, &aviso, "errores")?;
        lectores += 1;
    }
    if let (Some(entrada), Some(mut tuberia)) = (entrada, vigilado.hijo.stdin.take()) {
        // La entrada puede ser un texto privado: la copia se borra al soltarla.
        let datos = Zeroizing::new(entrada.to_vec());
        std::thread::Builder::new()
            .name("termux-entrada".into())
            .spawn(move || {
                // Si el hijo no la lee y termina, la tubería se rompe y esto
                // acaba solo; al soltarla, el hijo ve el fin de la entrada.
                let _ = tuberia.write_all(&datos);
            })
            .map_err(|error| FalloOrden::Io(format!("no se pudo escribir la entrada: {error}")))?;
    }

    // Espera: el fin de cada lector despierta este hilo; el latido solo cubre
    // el caso de un hijo que sigue vivo con sus salidas ya cerradas.
    let mut lectores_vivos = lectores;
    let mut estado: Option<ExitStatus> = None;
    let mut terminado_en: Option<Instant> = None;
    loop {
        if estado.is_none() {
            match vigilado.comprobar() {
                Ok(Some(fin)) => {
                    estado = Some(fin);
                    terminado_en = Some(Instant::now());
                }
                Ok(None) => {}
                Err(error) => {
                    return Err(FalloOrden::Io(format!(
                        "no se pudo esperar a la orden: {error}"
                    )))
                }
            }
        }
        let ahora = Instant::now();
        if estado.is_some() && (lectores_vivos == 0 || ahora >= limite) {
            break;
        }
        if let Some(fin) = terminado_en {
            if ahora >= fin + GRACIA_TRAS_SALIDA {
                break;
            }
        }
        if estado.is_none() && ahora >= limite {
            vigilado.matar();
            vigilado.recoger(ESPERA_TRAS_MATAR);
            return Ok(Fin::PlazoVencido);
        }
        // Cancelada mientras corre: igual que el plazo, mata el grupo entero.
        // Con el hijo ya terminado no hay nada que matar (su pid podría
        // reutilizarse): el bucle acaba solo en `GRACIA_TRAS_SALIDA`.
        if estado.is_none() && cancelada.is_some_and(|cancelada| cancelada()) {
            vigilado.matar();
            vigilado.recoger(ESPERA_TRAS_MATAR);
            return Ok(Fin::Cancelada);
        }
        let hasta = match terminado_en {
            Some(fin) => (fin + GRACIA_TRAS_SALIDA).min(limite),
            None => limite,
        };
        let espera = LATIDO.min(hasta.saturating_duration_since(ahora));
        match recibir.recv_timeout(espera) {
            Ok(()) => lectores_vivos = lectores_vivos.saturating_sub(1),
            // `aviso` sigue vivo aquí: no hay desconexión que atender.
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
    }
    drop(aviso);

    let Some(estado) = estado else {
        return Err(FalloOrden::Io("la orden no terminó".into()));
    };
    if estado.success() {
        Ok(Fin::Salida(recoger_bufer(&salida)))
    } else {
        Ok(Fin::Codigo {
            codigo: estado.code(),
            error: linea_de_error(&recoger_bufer(&errores)),
        })
    }
}

/// Lanza el proceso. Un programa que se está reescribiendo en ese instante
/// (una actualización del paquete) da «archivo de texto ocupado»: es pasajero,
/// así que se reintenta unas pocas veces antes de darlo por fallo.
fn lanzar(orden: &mut Command) -> std::io::Result<Child> {
    let mut intentos = 0;
    loop {
        match orden.spawn() {
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy && intentos < 5 =>
            {
                intentos += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            resultado => return resultado,
        }
    }
}

/// Lanza el hilo que lee una tubería de salida y avisa al terminar.
fn lanzar_lector<R: Read + Send + 'static>(
    tuberia: R,
    destino: &Arc<Mutex<Vec<u8>>>,
    aviso: &mpsc::Sender<()>,
    nombre: &str,
) -> Result<(), FalloOrden> {
    let destino = Arc::clone(destino);
    let aviso = aviso.clone();
    std::thread::Builder::new()
        .name(format!("termux-{nombre}"))
        .spawn(move || {
            leer_con_tope(tuberia, &destino);
            let _ = aviso.send(());
        })
        .map(|_| ())
        .map_err(|error| FalloOrden::Io(format!("no se pudo leer la salida: {error}")))
}

/// El nombre de la orden (sin su directorio), para los mensajes.
fn nombre_de(ruta: &Path) -> String {
    ruta.file_name()
        .map(|nombre| nombre.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// El error de una orden en una sola línea: sin caracteres de control ni de
/// dirección de texto, con los espacios colapsados y de `LARGO_ERROR`
/// caracteres como mucho.
fn linea_de_error(stderr: &[u8]) -> String {
    let texto = String::from_utf8_lossy(stderr);
    let mut linea = String::new();
    let mut caracteres = 0usize;
    let mut separar = false;
    for caracter in texto.chars() {
        if caracter.is_control()
            || caracter.is_whitespace()
            || matches!(caracter, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
        {
            separar = !linea.is_empty();
            continue;
        }
        if separar {
            linea.push(' ');
            caracteres += 1;
            separar = false;
        }
        if caracteres >= LARGO_ERROR {
            break;
        }
        linea.push(caracter);
        caracteres += 1;
    }
    linea.truncate(linea.trim_end().len());
    linea
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn apagar_dice_si_es_la_primera_vez_y_no_se_revierte() {
        let api = ApiProceso::default();
        assert_eq!(api.estado(), EstadoApi::SinComprobar);
        assert!(!api.apagar(EstadoApi::Disponible), "no es un apagado");
        assert!(!api.apagada());
        assert!(api.apagar(EstadoApi::NoResponde), "la primera vez avisa");
        assert!(!api.apagar(EstadoApi::Ausente), "la segunda ya no");
        assert_eq!(
            api.estado(),
            EstadoApi::NoResponde,
            "conserva el primer motivo"
        );
        api.marcar_disponible();
        assert!(api.apagada(), "un apagado no se revierte");
    }

    #[test]
    fn marcar_disponible_solo_cambia_si_no_esta_apagada() {
        let api = ApiProceso::default();
        api.marcar_disponible();
        assert_eq!(api.estado(), EstadoApi::Disponible);
        assert!(api.apagar(EstadoApi::Ausente));
        assert_eq!(api.estado(), EstadoApi::Ausente);
    }

    #[test]
    fn la_linea_de_error_no_lleva_control_ni_pasa_de_200() {
        let sucio = b"\x1b[31mfallo\x1b[0m\n  con\tdos   lineas\r\n\x07\xe2\x80\xaeoculto";
        let linea = linea_de_error(sucio);
        assert!(!linea.chars().any(char::is_control), "{linea:?}");
        assert!(!linea.contains('\u{202E}'), "{linea:?}");
        assert_eq!(linea, "[31mfallo [0m con dos lineas oculto");

        let largo = "ñ".repeat(500);
        let linea = linea_de_error(largo.as_bytes());
        assert_eq!(linea.chars().count(), 200);
        assert!(linea_de_error(b"  \n\t ").is_empty());
        assert!(linea_de_error(&[0xff, 0xfe]).chars().count() <= 200);
    }

    #[test]
    fn la_busqueda_en_el_path_no_acepta_rutas_ni_vacios() {
        let path = OsString::from("/usr/bin:/bin");
        assert_eq!(buscar_en_path("", Some(path.as_os_str())), None);
        assert_eq!(buscar_en_path("../bin/sh", Some(path.as_os_str())), None);
        assert_eq!(buscar_en_path("/bin/sh", Some(path.as_os_str())), None);
        assert_eq!(buscar_en_path("sh", None), None);
        assert!(buscar_en_path("sh", Some(path.as_os_str())).is_some());
    }

    #[test]
    fn el_display_de_los_fallos_es_el_texto_de_la_barra() {
        assert_eq!(FalloOrden::ApiApagada.to_string(), "Termux:API apagada");
        assert_eq!(
            FalloOrden::NoInstalada("termux-open-url".into()).to_string(),
            "termux-open-url no está instalado"
        );
        let vacio = FalloOrden::Codigo {
            codigo: Some(1),
            error: String::new(),
        };
        assert_eq!(vacio.to_string(), "la orden falló");
    }
}
