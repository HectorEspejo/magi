//! Cliente del servidor de sesiones: conexión al socket con autolanzado del
//! servidor (`setsid`), saludo versionado, tarea de lectura que convierte
//! mensajes en eventos de la UI, tarea de escritura y puente para `Ejecutar`.

pub mod pantallas;
pub mod ventana;

use std::collections::HashMap;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures_util::SinkExt as _;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};
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

/// Conexión del cliente con el servidor de sesiones.
#[derive(Clone)]
pub struct Cliente {
    pub tx: mpsc::UnboundedSender<MensajeCliente>,
    esperas: Arc<Mutex<HashMap<i64, Vec<oneshot::Sender<ResultadoEjecutar>>>>>,
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
        Self {
            tx,
            esperas: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn enviar(&self, mensaje: MensajeCliente) {
        let _ = self.tx.send(mensaje);
    }

    /// Pide al servidor ejecutar un comando en la conexión viva del host
    /// (sondeo). Con `SinSesion` el llamador cae a su conexión efímera.
    pub async fn ejecutar(&self, host_id: i64, comando: &str) -> ResultadoEjecutar {
        let (tx_respuesta, respuesta) = oneshot::channel();
        {
            let mut esperas = self.esperas.lock().await;
            esperas.entry(host_id).or_default().push(tx_respuesta);
        }
        self.enviar(MensajeCliente::Ejecutar {
            host_id,
            comando: comando.to_string(),
        });
        match respuesta.await {
            Ok(resultado) => resultado,
            Err(_) => ResultadoEjecutar::SinSesion,
        }
    }
}

/// Conecta con el servidor, lanzándolo si no está; errores legibles.
pub enum FalloConexion {
    /// El servidor habla otra versión del protocolo: la TUI arranca sin
    /// sesiones y lo explica en la barra.
    VersionIncompatible,
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
        MensajeServidor::Bienvenida { .. } => {
            // Tareas de lectura y escritura sobre el socket ya saludado.
            let (tx_mensajes, rx_mensajes) = mpsc::unbounded_channel();
            let cliente = Cliente {
                tx: tx_mensajes,
                esperas: Arc::new(Mutex::new(HashMap::new())),
            };
            tokio::spawn(tarea_escritura(rx_mensajes, salida));
            tokio::spawn(tarea_lectura(entrada, tx_eventos, cliente.esperas.clone()));
            Ok(cliente)
        }
        MensajeServidor::VersionIncompatible { .. } => Err(FalloConexion::VersionIncompatible),
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
    esperas: Arc<Mutex<HashMap<i64, Vec<oneshot::Sender<ResultadoEjecutar>>>>>,
) {
    let pantallas = pantallas::Pantallas::default();
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
                    Ok(MensajeServidor::Ejecutado { host_id, salida, codigo }) => {
                        resolver(esperas.clone(), host_id, ResultadoEjecutar::Salida { salida, codigo }).await;
                    }
                    Ok(MensajeServidor::SinSesion { host_id }) => {
                        resolver(esperas.clone(), host_id, ResultadoEjecutar::SinSesion).await;
                    }
                    Ok(mensaje) => {
                        let _ = tx_eventos.send(crate::app::Evento::Servidor(mensaje));
                    }
                    Err(motivo) => {
                        let _ = tx_eventos.send(crate::app::Evento::Servidor(MensajeServidor::Error {
                            mensaje: format!("mensaje corrupto del servidor: {motivo}"),
                        }));
                    }
                },
                // EOF del socket sin Adios: el servidor ha caído.
                _ => {
                    let _ = tx_eventos.send(crate::app::Evento::ServidorCaido);
                    return;
                }
            },
        }
    }
}

async fn resolver(
    esperas: Arc<Mutex<HashMap<i64, Vec<oneshot::Sender<ResultadoEjecutar>>>>>,
    host_id: i64,
    resultado: ResultadoEjecutar,
) {
    if let Some(listos) = esperas.lock().await.remove(&host_id) {
        for listo in listos {
            let _ = listo.send(resultado.clone());
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
        .stdout(Stdio::from(salida_log))
        .process_group(0);
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
fn lock_libre(ruta: &std::path::Path) -> bool {
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
        FalloConexion::VersionIncompatible => {
            "el servidor es de otra versión de protocolo; ejecuta «magi servidor parar» y vuelve a abrir"
                .to_string()
        }
        FalloConexion::Inaccesible(motivo) => motivo.clone(),
    }
}
