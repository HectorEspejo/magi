//! Cliente del servidor de sesiones: conexión al socket con autolanzado del
//! servidor (`setsid`), saludo versionado, tarea de lectura que convierte
//! mensajes en eventos de la UI, tarea de escritura y peticiones con respuesta
//! esperable (`Ejecutar` y las que vengan), casadas por `peticion_id`.

pub mod pantallas;
pub mod ventana;

use std::collections::HashMap;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures_util::SinkExt as _;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt as _;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{info, warn};

use crate::config::Rutas;
use crate::protocolo::{
    codificar, decodificar, MensajeCliente, MensajeServidor, VERSION_PROTOCOLO,
};

use crate::servidor;

/// Resultado de un `Ejecutar` pendiente del sondeo.
#[derive(Debug, Clone)]
pub enum ResultadoEjecutar {
    Salida { salida: String, codigo: i32 },
    SinSesion,
}

/// Primer `peticion_id` de las peticiones esperables (`Cliente::peticion`).
/// Cada subsistema tiene su rango (T39): Archivos empieza en 0, los túneles en
/// 2^40 y las ejecuciones en 2^44; nadie más usa de 2^48 en adelante.
pub const RANGO_ESPERAS: u64 = 1 << 48;

/// Por qué no llegó la respuesta de una petición.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalloPeticion {
    /// No hay servidor al que enviarla (caído o de otra versión).
    SinServidor,
    /// El servidor se cayó con la petición en vuelo.
    ServidorCaido,
}

/// Peticiones en vuelo por `peticion_id`. `None` tras el EOF del socket: una
/// petición que llegue después falla al momento en vez de esperar a nadie.
type Esperas = Arc<std::sync::Mutex<Option<HashMap<u64, oneshot::Sender<MensajeServidor>>>>>;

/// Conexión del cliente con el servidor de sesiones.
#[derive(Clone)]
pub struct Cliente {
    pub tx: mpsc::UnboundedSender<MensajeCliente>,
    /// Peticiones en vuelo que esperan su respuesta, por `peticion_id`.
    esperas: Esperas,
    siguiente_peticion: Arc<AtomicU64>,
    /// Registro de pantallas compartido con la tarea de lectura: es el que
    /// pinta la UI, de modo que los datos del remoto acaban en su parser.
    pantallas: pantallas::Pantallas,
}

/// Quita la espera de una petición cuando quien la hizo deja de esperar (plazo
/// vencido, tarea abortada): si no, el mapa crecería con cada una.
struct GuardiaEspera {
    esperas: Esperas,
    peticion_id: u64,
}

impl Drop for GuardiaEspera {
    fn drop(&mut self) {
        if let Ok(mut esperas) = self.esperas.lock() {
            if let Some(esperas) = esperas.as_mut() {
                esperas.remove(&self.peticion_id);
            }
        }
    }
}

impl Default for Cliente {
    fn default() -> Self {
        Self::sin_servidor()
    }
}

impl Cliente {
    /// Cliente sin servidor (versión incompatible o caído): los envíos se
    /// descartan sin error.
    pub fn sin_servidor() -> Self {
        let (tx, _rx) = mpsc::unbounded_channel();
        Self::con_canal(tx, pantallas::Pantallas::default())
    }

    /// Cliente de pruebas: lo que la App envía al servidor queda en el
    /// receptor devuelto.
    #[doc(hidden)]
    pub fn de_prueba() -> (Self, mpsc::UnboundedReceiver<MensajeCliente>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self::con_canal(tx, pantallas::Pantallas::default()), rx)
    }

    fn con_canal(
        tx: mpsc::UnboundedSender<MensajeCliente>,
        pantallas: pantallas::Pantallas,
    ) -> Self {
        Self {
            tx,
            esperas: Arc::new(std::sync::Mutex::new(Some(HashMap::new()))),
            siguiente_peticion: Arc::new(AtomicU64::new(RANGO_ESPERAS)),
            pantallas,
        }
    }

    /// El registro de pantallas que alimenta la tarea de lectura.
    pub fn pantallas(&self) -> pantallas::Pantallas {
        self.pantallas.clone()
    }

    pub fn enviar(&self, mensaje: MensajeCliente) {
        let _ = self.tx.send(mensaje);
    }

    /// Envía una petición con un `peticion_id` propio y espera su respuesta
    /// (la que lleve ese id: `Ejecutado`, `SinSesion`, `Error`…). El plazo lo
    /// pone quien llama; si deja de esperar, la respuesta tardía se descarta.
    pub async fn peticion(
        &self,
        construir: impl FnOnce(u64) -> MensajeCliente,
    ) -> Result<MensajeServidor, FalloPeticion> {
        let peticion_id = self.siguiente_peticion.fetch_add(1, Ordering::Relaxed);
        let (tx_respuesta, respuesta) = oneshot::channel();
        {
            let Ok(mut esperas) = self.esperas.lock() else {
                return Err(FalloPeticion::ServidorCaido);
            };
            match esperas.as_mut() {
                Some(esperas) => {
                    esperas.insert(peticion_id, tx_respuesta);
                }
                None => return Err(FalloPeticion::ServidorCaido),
            }
        }
        let _guardia = GuardiaEspera {
            esperas: self.esperas.clone(),
            peticion_id,
        };
        if self.tx.send(construir(peticion_id)).is_err() {
            return Err(FalloPeticion::SinServidor);
        }
        respuesta.await.map_err(|_| FalloPeticion::ServidorCaido)
    }

    /// Pide al servidor ejecutar un comando en la conexión viva del host
    /// (sondeo). Con `SinSesion` el llamador cae a su conexión efímera.
    pub async fn ejecutar(&self, host_id: i64, comando: &str) -> ResultadoEjecutar {
        let respuesta = self
            .peticion(|peticion_id| MensajeCliente::Ejecutar {
                host_id,
                comando: comando.to_string(),
                peticion_id,
            })
            .await;
        match respuesta {
            Ok(MensajeServidor::Ejecutado { salida, codigo, .. }) => {
                ResultadoEjecutar::Salida { salida, codigo }
            }
            _ => ResultadoEjecutar::SinSesion,
        }
    }
}

/// `peticion_id` de una respuesta a una petición esperable, si lo es.
fn peticion_de_respuesta(mensaje: &MensajeServidor) -> Option<u64> {
    let peticion_id = match mensaje {
        MensajeServidor::Ejecutado { peticion_id, .. }
        | MensajeServidor::SinSesion { peticion_id, .. }
        | MensajeServidor::Hecho { peticion_id }
        | MensajeServidor::DirListado { peticion_id, .. } => *peticion_id,
        MensajeServidor::Error {
            peticion_id: Some(peticion_id),
            ..
        }
        | MensajeServidor::SftpAbierto {
            peticion_id: Some(peticion_id),
            ..
        } => *peticion_id,
        _ => return None,
    };
    (peticion_id >= RANGO_ESPERAS).then_some(peticion_id)
}

/// Conecta con el servidor, lanzándolo si no está; errores legibles.
#[derive(Debug)]
pub enum FalloConexion {
    /// El servidor habla otra versión de protocolo: la TUI arranca sin
    /// sesiones y lo explica en la barra. Lleva la versión **del servidor** y
    /// su pid (del mensaje o, si es antiguo, del socket).
    VersionIncompatible { version: u32, pid: Option<u32> },
    /// No hay manera de llegar a un servidor.
    Inaccesible(String),
}

/// Conecta al socket del servidor; si no existe lo lanza desacoplado y espera
/// hasta 2 s. Socket presente sin respuesta → huérfano (borrar y relanzar) o
/// lock ocupado (tres reintentos de 1 s).
pub async fn conectar(
    rutas: &Rutas,
    tx_eventos: mpsc::UnboundedSender<crate::app::Evento>,
) -> Result<Cliente, FalloConexion> {
    let ruta = servidor::ruta_socket(rutas);
    match UnixStream::connect(&ruta).await {
        Ok(stream) => saludar(stream, tx_eventos).await,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            lanzar_servidor(rutas);
            if esperar_socket(&ruta, Duration::from_secs(2)).await {
                match UnixStream::connect(&ruta).await {
                    Ok(stream) => saludar(stream, tx_eventos).await,
                    Err(error) => Err(FalloConexion::Inaccesible(format!(
                        "el servidor no respondió tras arrancar: {error}"
                    ))),
                }
            } else {
                Err(FalloConexion::Inaccesible(
                    "el servidor de sesiones no ha arrancado en 2 s; revisa \
                     ~/.local/state/magi/logs/servidor.log"
                        .to_string(),
                ))
            }
        }
        Err(_) => {
            // Socket presente que no responde: huérfano o servidor colgado.
            if lock_libre(&servidor::ruta_lock(rutas)) {
                info!("socket huérfano sin servidor: se borra y se relanza");
                std::fs::remove_file(&ruta).ok();
                lanzar_servidor(rutas);
                if esperar_socket(&ruta, Duration::from_secs(2)).await {
                    return match UnixStream::connect(&ruta).await {
                        Ok(stream) => saludar(stream, tx_eventos).await,
                        Err(error) => Err(FalloConexion::Inaccesible(error.to_string())),
                    };
                }
                return Err(FalloConexion::Inaccesible(
                    "el servidor de sesiones no ha arrancado en 2 s".to_string(),
                ));
            }
            // Lock ocupado: el servidor está a medias; se reintenta.
            for _ in 0..3 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if let Ok(stream) = UnixStream::connect(&ruta).await {
                    return saludar(stream, tx_eventos).await;
                }
            }
            Err(FalloConexion::Inaccesible(
                "servidor sin socket: magi servidor parar".to_string(),
            ))
        }
    }
}

async fn saludar(
    stream: UnixStream,
    tx_eventos: mpsc::UnboundedSender<crate::app::Evento>,
) -> Result<Cliente, FalloConexion> {
    let pid_par = servidor::pid_del_par(&stream);
    let (lectura, escritura) = stream.into_split();
    let mut salida = FramedWrite::new(escritura, crate::protocolo::codec());
    let saludo = MensajeCliente::Hola {
        version: VERSION_PROTOCOLO,
        pid: std::process::id(),
    };
    let linea =
        codificar(&saludo).map_err(|error| FalloConexion::Inaccesible(error.to_string()))?;
    salida
        .send(linea.as_str())
        .await
        .map_err(|error| FalloConexion::Inaccesible(error.to_string()))?;

    let mut entrada = FramedRead::new(lectura, crate::protocolo::codec());
    let primero = tokio::time::timeout(Duration::from_secs(5), entrada.next())
        .await
        .map_err(|_| {
            FalloConexion::Inaccesible("el servidor no respondió al saludo".to_string())
        })?;
    let linea = match primero {
        Some(Ok(linea)) => linea,
        Some(Err(error)) => {
            return Err(FalloConexion::Inaccesible(format!(
                "respuesta corrupta del servidor: {error}"
            )))
        }
        None => {
            return Err(FalloConexion::Inaccesible(
                "el servidor cerró la conexión en el saludo".to_string(),
            ))
        }
    };
    match decodificar::<MensajeServidor>(&linea)
        .map_err(|error| FalloConexion::Inaccesible(error.to_string()))?
    {
        mensaje @ MensajeServidor::Bienvenida { .. } => {
            // La bienvenida también va a la UI: con ella reconcilia las
            // sesiones que el servidor ya custodiaba.
            let _ = tx_eventos.send(crate::app::Evento::Servidor(mensaje));
            // Tareas de lectura y escritura sobre el socket ya saludado.
            let (tx_mensajes, rx_mensajes) = mpsc::unbounded_channel();
            let pantallas = pantallas::Pantallas::default();
            let cliente = Cliente::con_canal(tx_mensajes, pantallas.clone());
            tokio::spawn(tarea_escritura(rx_mensajes, salida));
            tokio::spawn(tarea_lectura(
                entrada,
                tx_eventos,
                cliente.esperas.clone(),
                pantallas,
            ));
            Ok(cliente)
        }
        MensajeServidor::VersionIncompatible { version, pid } => {
            Err(FalloConexion::VersionIncompatible {
                version,
                pid: pid.or(pid_par),
            })
        }
        otro => Err(FalloConexion::Inaccesible(format!(
            "respuesta inesperada al saludo: {otro:?}"
        ))),
    }
}

/// Drena la cola de mensajes hacia el servidor.
async fn tarea_escritura(
    mut rx: mpsc::UnboundedReceiver<MensajeCliente>,
    mut salida: FramedWrite<tokio::net::unix::OwnedWriteHalf, tokio_util::codec::LinesCodec>,
) {
    while let Some(mensaje) = rx.recv().await {
        let Ok(linea) = codificar(&mensaje) else {
            break;
        };
        if salida.send(linea.as_str()).await.is_err() {
            break;
        }
    }
}

/// Convierte los mensajes del servidor en eventos de la UI y alimenta los
/// parsers de las pestañas con coalescencia a ~30 fps.
async fn tarea_lectura(
    mut entrada: FramedRead<tokio::net::unix::OwnedReadHalf, crate::protocolo::LinesCodec>,
    tx_eventos: mpsc::UnboundedSender<crate::app::Evento>,
    esperas: Esperas,
    pantallas: pantallas::Pantallas,
) {
    let mut sucias: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut intervalo = tokio::time::interval(Duration::from_millis(33));
    loop {
        tokio::select! {
            _ = intervalo.tick() => {
                if !sucias.is_empty() {
                    let ids: Vec<u32> = sucias.drain().collect();
                    let _ = tx_eventos.send(crate::app::Evento::Pantallas(ids));
                }
            }
            mensaje = entrada.next() => match mensaje {
                Some(Ok(linea)) => match decodificar::<MensajeServidor>(&linea) {
                    Ok(MensajeServidor::Datos { sesion_id, bytes }) => {
                        pantallas.procesar(sesion_id, &bytes);
                        sucias.insert(sesion_id);
                    }
                    Ok(MensajeServidor::PantallaCompleta { sesion_id, bytes, cols, filas }) => {
                        pantallas.volcar(sesion_id, filas, cols, &bytes);
                        let _ = tx_eventos.send(crate::app::Evento::Pantallas(vec![sesion_id]));
                    }
                    // El tamaño nuevo se aplica aquí, en orden con los datos:
                    // lo que el remoto pinte después ya llega con ese tamaño.
                    // La App recibe el mensaje igualmente (barra y relleno).
                    Ok(mensaje @ MensajeServidor::Redimensionada { sesion_id, cols, filas, .. }) => {
                        pantallas.redimensionar(sesion_id, filas, cols);
                        let _ = tx_eventos.send(crate::app::Evento::Servidor(mensaje));
                    }
                    Ok(mensaje) => {
                        // La respuesta de una petición esperable va a quien la
                        // espera; si ya no espera (plazo vencido), se tira: no
                        // es de nadie más.
                        if let Some(peticion_id) = peticion_de_respuesta(&mensaje) {
                            let espera = esperas.lock().ok().and_then(|mut esperas| {
                                esperas
                                    .as_mut()
                                    .and_then(|esperas| esperas.remove(&peticion_id))
                            });
                            if let Some(espera) = espera {
                                let _ = espera.send(mensaje);
                            }
                            continue;
                        }
                        let _ = tx_eventos.send(crate::app::Evento::Servidor(mensaje));
                    }
                    Err(motivo) => {
                        let _ = tx_eventos.send(crate::app::Evento::Servidor(
                            MensajeServidor::Error {
                                mensaje: format!("mensaje corrupto del servidor: {motivo}"),
                                peticion_id: None,
                            },
                        ));
                    }
                },
                // EOF del socket sin Adios: el servidor ha caído. Quien
                // esperaba una respuesta deja de esperar.
                _ => {
                    if let Ok(mut esperas) = esperas.lock() {
                        *esperas = None;
                    }
                    let _ = tx_eventos.send(crate::app::Evento::ServidorCaido);
                    return;
                }
            },
        }
    }
}

// ---------------------------------------------------------------- arranque

/// Lanza `magi --servidor` desacoplado: sesión propia (`setsid`), stdio al
/// log del servidor del día. Devuelve aunque el servidor tarde en arrancar.
pub fn lanzar_servidor(rutas: &Rutas) {
    let Ok(ejecutable) = std::env::current_exe() else {
        warn!("no se pudo determinar el ejecutable para lanzar el servidor");
        return;
    };
    let _ = std::fs::create_dir_all(rutas.dir_logs());
    let fecha = crate::modelo::fecha_hoy();
    let ruta_log = rutas.dir_logs().join(format!("servidor.log.{fecha}"));
    let salida_log = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ruta_log)
    {
        Ok(fichero) => fichero,
        Err(error) => {
            warn!("no se pudo abrir el log del servidor: {error}");
            return;
        }
    };
    let log_errores = salida_log.try_clone();
    let mut comando = Command::new(ejecutable);
    comando
        .arg("--servidor")
        .stdin(Stdio::null())
        .stdout(Stdio::from(salida_log));
    if let Ok(errores) = log_errores {
        comando.stderr(Stdio::from(errores));
    } else {
        comando.stderr(Stdio::null());
    }
    // setsid: sesión propia para sobrevivir a la ventana que lo lanzó.
    unsafe {
        comando.pre_exec(|| {
            nix::unistd::setsid()
                .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?;
            Ok(())
        });
    }
    match comando.spawn() {
        Ok(hijo) => {
            let _ = hijo.id(); // el pid del servidor lo sabrá el saludo
            info!("servidor lanzado desacoplado");
        }
        Err(error) => warn!("no se pudo lanzar el servidor: {error}"),
    }
}

/// ¿Está libre el lock del servidor? Un fichero que no existe es libre.
pub(crate) fn lock_libre(ruta: &std::path::Path) -> bool {
    match std::fs::OpenOptions::new().read(true).open(ruta) {
        Err(_) => true,
        Ok(fichero) => {
            match nix::fcntl::Flock::lock(fichero, nix::fcntl::FlockArg::LockExclusiveNonblock) {
                // El lock se libera al caer el guardia: estaba libre.
                Ok(bloqueo) => {
                    drop(bloqueo);
                    true
                }
                Err(_) => false,
            }
        }
    }
}

/// Espera a que el socket acepte conexiones, hasta el plazo dado.
pub async fn esperar_socket(ruta: &std::path::Path, plazo: Duration) -> bool {
    let inicio = std::time::Instant::now();
    while inicio.elapsed() < plazo {
        if UnixStream::connect(ruta).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Comprueba la conexión y devuelve el error legible para mensajes.
pub fn describe_fallo(fallo: &FalloConexion) -> String {
    match fallo {
        FalloConexion::VersionIncompatible { version, pid } => {
            let pid = pid.map(|pid| format!(" (pid {pid})")).unwrap_or_default();
            if *version < crate::protocolo::VERSION_PROTOCOLO {
                format!(
                    "el servidor{pid} habla el protocolo {version} y esta MAGI el {}: es de una versión anterior, ejecuta «magi servidor parar» y vuelve a abrir",
                    crate::protocolo::VERSION_PROTOCOLO
                )
            } else {
                format!(
                    "el servidor{pid} habla el protocolo {version}, más nuevo que el {} de esta MAGI",
                    crate::protocolo::VERSION_PROTOCOLO
                )
            }
        }
        FalloConexion::Inaccesible(motivo) => motivo.clone(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Sin servidor, una petición falla al momento en vez de esperar.
    #[tokio::test]
    async fn sin_servidor_una_peticion_falla_al_momento() {
        let cliente = Cliente::sin_servidor();
        let respuesta = cliente
            .peticion(|peticion_id| MensajeCliente::Ejecutar {
                host_id: 1,
                comando: "true".to_string(),
                peticion_id,
            })
            .await;
        assert_eq!(respuesta.unwrap_err(), FalloPeticion::SinServidor);
    }

    /// Tras el EOF del socket (esperas cerradas), una petición nueva no se
    /// queda esperando una respuesta que no va a llegar.
    #[tokio::test]
    async fn tras_el_eof_una_peticion_falla_al_momento() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let cliente = Cliente::con_canal(tx, pantallas::Pantallas::default());
        *cliente.esperas.lock().unwrap() = None;
        let respuesta = tokio::time::timeout(
            Duration::from_secs(1),
            cliente.peticion(|peticion_id| MensajeCliente::Ejecutar {
                host_id: 1,
                comando: "true".to_string(),
                peticion_id,
            }),
        )
        .await
        .expect("la petición se quedó esperando");
        assert_eq!(respuesta.unwrap_err(), FalloPeticion::ServidorCaido);
    }

    /// Una espera abandonada (plazo vencido) no se queda en el mapa.
    #[tokio::test]
    async fn una_espera_abandonada_se_retira() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let cliente = Cliente::con_canal(tx, pantallas::Pantallas::default());
        let _ = tokio::time::timeout(
            Duration::from_millis(20),
            cliente.peticion(|peticion_id| MensajeCliente::Ejecutar {
                host_id: 1,
                comando: "true".to_string(),
                peticion_id,
            }),
        )
        .await;
        let pendientes = cliente
            .esperas
            .lock()
            .unwrap()
            .as_ref()
            .map(HashMap::len)
            .unwrap_or_default();
        assert_eq!(pendientes, 0);
    }
}
