//! Servidor de sesiones (`magi --servidor`): custodia sesiones SSH y el pool
//! de conexiones, hablante por un socket Unix con el protocolo JSON por
//! líneas. No dialoga: reenvía huellas, frases y contraseñas al cliente
//! solicitante. Escribe en SQLite solo `REGISTRO` y `HOSTS.ultimo_estado` /
//! `ultima_conexion_en`.

pub mod cliente_remoto;
pub mod conexiones;
pub mod difusion;
pub mod ejecuciones;
pub mod sesiones;
pub mod sftp;
pub mod socks5;
pub mod transferencias;
pub mod tuneles;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context as _, Result};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tokio_stream::StreamExt as _;
use tokio_util::codec::{FramedRead, LinesCodec};
use tracing::{info, warn};

use crate::almacen::Almacen;
use crate::config::{Config, Rutas};
use crate::modelo::{ResultadoRegistro, UltimoEstado};
use crate::protocolo::{
    codificar, decodificar, MensajeCliente, MensajeServidor, VERSION_PROTOCOLO,
};
use russh::ChannelMsg;

use cliente_remoto::{ClienteRemoto, Tamano};
use conexiones::Pool;
use sesiones::difundir_lista;
use sesiones::{ComandoSesion, DecisionDialogo, Pendiente, Sesion};

/// Órdenes de escritura para el hilo con la base de datos: el único escritor
/// de SQLite dentro del proceso servidor. Se ejecutan en orden, así que una
/// `Barrera` confirmada garantiza que lo anterior ya está escrito.
pub enum OrdenBd {
    Anotar {
        tipo: String,
        host_id: Option<i64>,
        identidad_id: Option<i64>,
        detalle: String,
        resultado: ResultadoRegistro,
    },
    MarcarConexion {
        host_id: i64,
    },
    MarcarEstado {
        host_id: i64,
        estado: Option<UltimoEstado>,
    },
    /// Contesta cuando todo lo encolado antes está escrito (T21: un `Hecho`
    /// de una operación que anota se envía con la fila ya en `REGISTRO`).
    Barrera(tokio::sync::oneshot::Sender<()>),
    /// Cierra la base de datos (checkpoint del WAL) y contesta: al apagarse no
    /// se pierde ninguna anotación.
    Terminar(tokio::sync::oneshot::Sender<()>),
    /// Rellena `DELIBERACIONES.ejecucion_resultado` al terminar una ejecución
    /// deliberada (una sola vez; excepción documentada a T18).
    ResultadoDeliberacion {
        deliberacion_id: i64,
        resultado: crate::deliberacion::EjecucionResultado,
    },
}

/// Plazo para que el escritor confirme una barrera (el doble del
/// `busy_timeout` de SQLite).
const PLAZO_ESCRITURA: Duration = Duration::from_secs(10);

/// Plazo total del apagado limpio: nada del remoto puede colgarlo.
const PLAZO_APAGADO: Duration = Duration::from_secs(10);

/// Plazo de un `Ejecutar` (el sondeo de Flota sobre la conexión viva). Por
/// debajo del del cliente (`flota::TIMEOUT_SEG`): el servidor contesta antes
/// de que el cliente se rinda, y así este aún puede caer a su conexión
/// efímera.
const PLAZO_EJECUTAR: Duration = Duration::from_secs(4);

/// Parte del plazo anterior para abrir el canal `exec`.
const PLAZO_CANAL_EJECUTAR: Duration = Duration::from_secs(2);

/// Estado compartido del servidor, siempre bajo `Mutex`.
pub struct EstadoServidor {
    pub rutas: Rutas,
    pub config: Config,
    /// Segundos de gracia antes de apagarse sin clientes ni sesiones.
    pub gracia: u64,
    pub bd: std::sync::mpsc::Sender<OrdenBd>,
    /// Almacén de solo lectura para las fichas (la escritura va por `bd`).
    pub lectura: Arc<std::sync::Mutex<Almacen>>,
    pub sesiones: HashMap<u32, Sesion>,
    pub clientes: HashMap<u32, ClienteRemoto>,
    pub pool: Pool,
    pub pendientes: HashMap<u32, Pendiente>,
    pub siguiente_sesion_id: u32,
    pub siguiente_cliente_id: u32,
    pub vacio_desde: Option<std::time::Instant>,
    pub tx_apagar: Option<mpsc::UnboundedSender<()>>,
    /// Cerrojo por host de `conexion_para_canal` (corrección 3b): cubre
    /// «mirar el pool / conectar / guardar» para todos los subsistemas.
    pub cerrojos_conexion: HashMap<i64, Arc<tokio::sync::Mutex<()>>>,
    /// El servidor se está apagando: no se admite trabajo nuevo.
    pub apagando: bool,
    /// Pasa a `true` cuando el apagado limpio termina: quien lo pida mientras
    /// está en curso (otro `Parar`, un `SIGTERM`) espera a que acabe.
    pub apagado: tokio::sync::watch::Sender<bool>,
    /// Canales SFTP abiertos, uno por host y compartidos por todas las ventanas.
    pub sftp: HashMap<i64, sftp::SftpHost>,
    /// Cerrojo de apertura del canal SFTP por host: dos ventanas que pidan el
    /// mismo host a la vez comparten un único canal.
    pub sftp_cerrojos: HashMap<i64, Arc<tokio::sync::Mutex<()>>>,
    /// Cola de transferencias, lo único de la vista Archivos que sobrevive a la
    /// ventana.
    pub transferencias: transferencias::Cola,
    /// Marca sucia de la difusión de la cola: el progreso se coalesce a 4/s.
    pub difusion_cola: difusion::DifusionCola,
    /// Aviso al supervisor de la cola de que hay algo que despachar.
    pub tx_cola: Option<mpsc::UnboundedSender<()>>,
    /// Túneles levantados, por id de `TUNELES`.
    pub tuneles: HashMap<i64, tuneles::TunelActivo>,
    /// Ejecuciones de snippets (Fase 6), en curso y terminadas de la última
    /// hora.
    pub ejecuciones: ejecuciones::Ejecuciones,
    /// Reenvíos remotos registrados, compartidos con los handlers de russh.
    pub reenvios: Arc<crate::conexion::reenvios::Reenvios>,
    /// Túneles automáticos que el usuario paró a mano: no se vuelven a levantar
    /// hasta que el host se quede sin pestañas ni SFTP (el siguiente ciclo).
    pub tuneles_parados: std::collections::HashSet<(i64, i64)>,
}

/// Ruta del socket del servidor.
pub fn ruta_socket(rutas: &Rutas) -> PathBuf {
    rutas.dir_runtime().join("servidor.sock")
}

/// Ruta del fichero de bloqueo del servidor.
pub fn ruta_lock(rutas: &Rutas) -> PathBuf {
    rutas.dir_runtime().join("servidor.lock")
}

// ---------------------------------------------------------------- arranque

/// Punto de entrada de `magi --servidor`: prepara directorio, lock y socket,
/// sirve a los clientes y se apaga cuando no quedan sesiones ni clientes
/// durante la gracia.
pub async fn arrancar(rutas: Rutas, config: Config) -> Result<()> {
    let directorio = rutas.dir_runtime();
    std::fs::create_dir_all(&directorio)
        .with_context(|| format!("creando {}", directorio.display()))?;
    std::fs::set_permissions(&directorio, std::fs::Permissions::from_mode(0o700))
        .with_context(|| "poniendo los permisos 700 del directorio de ejecución")?;

    // Bloqueo exclusivo: un segundo servidor termina sin tocar el socket.
    // El `Flock` se conserva vivo hasta la salida; al caer, libera el lock.
    let fichero_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(ruta_lock(&rutas))
        .context("abriendo servidor.lock")?;
    let _bloqueo = match nix::fcntl::Flock::lock(fichero_lock, nix::fcntl::FlockArg::LockExclusiveNonblock)
    {
        Ok(bloqueo) => bloqueo,
        Err((_fichero, error)) => bail!(
            "ya hay un servidor de sesiones en marcha ({error}); usa «magi servidor estado» para verlo"
        ),
    };

    // El socket se crea con umask 077 para que solo el usuario pueda abrirlo.
    let ruta_socket = ruta_socket(&rutas);
    let anterior = nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
    if ruta_socket.exists() {
        // Con el lock en la mano, un socket que queda es huérfano.
        std::fs::remove_file(&ruta_socket).ok();
    }
    let listener = UnixListener::bind(&ruta_socket)
        .with_context(|| format!("abriendo el socket {}", ruta_socket.display()))?;
    nix::sys::stat::umask(anterior);

    let almacen =
        Almacen::abrir(&rutas.base_datos()).context("abriendo la base de datos del servidor")?;
    let lectura = Arc::new(std::sync::Mutex::new(Almacen::abrir(&rutas.base_datos())?));
    // Los temporales de una ejecución anterior interrumpida no se heredan.
    sftp::preparar_temporales(&rutas);
    let estado = Arc::new(tokio::sync::Mutex::new(EstadoServidor {
        rutas: rutas.clone(),
        gracia: config.servidor.gracia_apagado_seg,
        config,
        bd: lanzar_escritor_bd(almacen),
        lectura,
        sesiones: HashMap::new(),
        clientes: HashMap::new(),
        pool: Pool::default(),
        pendientes: HashMap::new(),
        siguiente_sesion_id: 1,
        siguiente_cliente_id: 1,
        vacio_desde: None,
        tx_apagar: None,
        cerrojos_conexion: HashMap::new(),
        apagando: false,
        apagado: tokio::sync::watch::channel(false).0,
        sftp: HashMap::new(),
        sftp_cerrojos: HashMap::new(),
        transferencias: transferencias::Cola::default(),
        difusion_cola: difusion::DifusionCola::default(),
        tx_cola: None,
        tuneles: HashMap::new(),
        ejecuciones: ejecuciones::Ejecuciones::default(),
        reenvios: Arc::new(crate::conexion::reenvios::Reenvios::default()),
        tuneles_parados: std::collections::HashSet::new(),
    }));

    let (tx_cola, rx_cola) = mpsc::unbounded_channel::<()>();
    estado.lock().await.tx_cola = Some(tx_cola);

    let (tx_apagar, mut rx_apagar) = mpsc::unbounded_channel::<()>();
    estado.lock().await.tx_apagar = Some(tx_apagar);

    // Registro de arranque con la BD ya abierta.
    if let Err(error) = anotar(
        &estado,
        crate::registro::SERVIDOR_ARRANCADO,
        "servidor de sesiones en marcha",
    )
    .await
    {
        warn!("no se pudo anotar el arranque: {error}");
    }
    info!(socket = %ruta_socket.display(), "servidor arrancado");

    // Revisoras de inactividad (servidor) y de vencimiento (pool y canales SFTP).
    tokio::spawn(revisar_inactividad(estado.clone()));
    tokio::spawn(revisar_pool(estado.clone()));
    tokio::spawn(sftp::revisar(estado.clone()));
    tokio::spawn(revisar_cola(estado.clone()));
    tokio::spawn(transferencias::supervisor(estado.clone(), rx_cola));
    tokio::spawn(tuneles::revisar(estado.clone()));
    tokio::spawn(ejecuciones::revisar(estado.clone()));

    // SIGTERM es un `Parar`: apagado limpio con sus anotaciones (3b). Así un
    // MAGI de otra versión puede parar este servidor sin hablar su protocolo.
    let mut terminar =
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(senal) => Some(senal),
            Err(error) => {
                warn!("no se pudo atender SIGTERM: {error}");
                None
            }
        };

    loop {
        tokio::select! {
            _ = rx_apagar.recv() => break,
            _ = esperar_senal(&mut terminar) => {
                info!("SIGTERM recibido: apagado limpio");
                apagar_limpio(&estado).await;
                break;
            }
            aceptado = listener.accept() => match aceptado {
                Ok((stream, _)) => {
                    let estado_tarea = estado.clone();
                    tokio::spawn(tarea_conexion(stream, estado_tarea));
                }
                Err(error) => warn!("error aceptando un cliente: {error}"),
            },
        }
    }

    // Salida: socket y lock fuera, aviso anotado y la base de datos cerrada
    // con todo lo pendiente escrito.
    let _ = std::fs::remove_file(&ruta_socket);
    let _ = std::fs::remove_file(ruta_lock(&rutas));
    let detalle = "servidor detenido";
    if let Err(error) = anotar(&estado, crate::registro::SERVIDOR_DETENIDO, detalle).await {
        warn!("no se pudo anotar la parada: {error}");
    }
    cerrar_escritor(&estado).await;
    info!("{detalle}");
    Ok(())
}

/// Espera a la señal si se pudo registrar; si no, nunca termina.
async fn esperar_senal(senal: &mut Option<tokio::signal::unix::Signal>) {
    match senal {
        Some(senal) => {
            senal.recv().await;
        }
        None => std::future::pending::<()>().await,
    }
}

/// Anota un evento del servidor en `REGISTRO` a través del escritor.
async fn anotar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    tipo: &str,
    detalle: &str,
) -> Result<(), String> {
    // Se usa el canal; el hilo de la BD valida el tipo igual que `registro::anotar`.
    let bd = estado.lock().await.bd.clone();
    bd.send(OrdenBd::Anotar {
        tipo: tipo.to_string(),
        host_id: None,
        identidad_id: None,
        detalle: detalle.to_string(),
        resultado: ResultadoRegistro::Ok,
    })
    .map_err(|_| "el escritor de la base de datos no está".to_string())
}

/// Espera a que el escritor confirme todo lo encolado hasta ahora. Se llama
/// **sin** el mutex del estado tomado. Si no contesta en plazo se sigue igual
/// (el efecto remoto ya se ha producido) avisando en el log.
pub(crate) async fn confirmar_escritura(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) -> bool {
    let bd = estado.lock().await.bd.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    if bd.send(OrdenBd::Barrera(tx)).is_err() {
        return false;
    }
    match tokio::time::timeout(PLAZO_ESCRITURA, rx).await {
        Ok(Ok(())) => true,
        _ => {
            warn!("el escritor de la base de datos no confirmó a tiempo");
            false
        }
    }
}

/// Cierra la base de datos del escritor esperando a que vacíe su cola.
async fn cerrar_escritor(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let bd = estado.lock().await.bd.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    if bd.send(OrdenBd::Terminar(tx)).is_err() {
        return;
    }
    if tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .is_err()
    {
        warn!("la base de datos del servidor no se cerró a tiempo");
    }
}

/// Hilo con la única `Connection` de escritura del proceso servidor.
fn lanzar_escritor_bd(almacen: Almacen) -> std::sync::mpsc::Sender<OrdenBd> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut confirmar_cierre = None;
        while let Ok(orden) = rx.recv() {
            match orden {
                OrdenBd::Barrera(listo) => {
                    let _ = listo.send(());
                }
                OrdenBd::Terminar(listo) => {
                    confirmar_cierre = Some(listo);
                    break;
                }
                orden => {
                    if let Err(error) = ejecutar_orden_bd(almacen.conexion(), orden) {
                        warn!("no se pudo escribir en la base de datos: {error}");
                    }
                }
            }
        }
        if let Err(error) = almacen.cerrar() {
            warn!("no se pudo cerrar la base de datos del servidor: {error}");
        }
        if let Some(listo) = confirmar_cierre {
            let _ = listo.send(());
        }
    });
    tx
}

fn ejecutar_orden_bd(conexion: &rusqlite::Connection, orden: OrdenBd) -> Result<()> {
    match orden {
        OrdenBd::Anotar {
            tipo,
            host_id,
            identidad_id,
            detalle,
            resultado,
        } => {
            match crate::registro::anotar(
                conexion,
                &tipo,
                host_id,
                identidad_id,
                &detalle,
                resultado,
            ) {
                // El host se borró mientras tanto (una ejecución en curso, una
                // sesión que se cierra): la fila se guarda sin host, con el
                // nombre en el detalle, en vez de perderse.
                Err(error) if host_id.is_some() && es_fallo_de_clave_ajena(&error) => {
                    crate::registro::anotar(
                        conexion,
                        &tipo,
                        None,
                        identidad_id,
                        &detalle,
                        resultado,
                    )
                }
                otro => otro,
            }
        }
        OrdenBd::MarcarConexion { host_id } => {
            crate::almacen::hosts::marcar_conexion(conexion, host_id)
        }
        OrdenBd::MarcarEstado { host_id, estado } => {
            crate::almacen::hosts::marcar_estado(conexion, host_id, estado)
        }
        OrdenBd::ResultadoDeliberacion {
            deliberacion_id,
            resultado,
        } => crate::almacen::deliberaciones::fijar_resultado_ejecucion(
            conexion,
            deliberacion_id,
            resultado,
        )
        .map(|_| ()),
        OrdenBd::Barrera(listo) | OrdenBd::Terminar(listo) => {
            let _ = listo.send(());
            Ok(())
        }
    }
}

/// ¿Falló la escritura por una clave ajena (una fila que ya no existe)?
fn es_fallo_de_clave_ajena(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<rusqlite::Error>()
        .is_some_and(|error| {
            matches!(
                error,
                rusqlite::Error::SqliteFailure(fallo, _)
                    if fallo.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
            )
        })
}

/// Apaga el servidor si no hay clientes ni sesiones durante la gracia. Las
/// transferencias en curso cuentan: cerrar la última ventana no puede dejar a
/// medias una copia que el usuario encargó. Y un túnel levantado también:
/// apagarse lo tiraría sin que nadie lo haya pedido.
async fn revisar_inactividad(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let gracia = Duration::from_secs(estado.lock().await.gracia);
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let apagar = {
            let mut estado_bloqueado = estado.lock().await;
            let ocupado = !estado_bloqueado.clientes.is_empty()
                || !estado_bloqueado.sesiones.is_empty()
                || estado_bloqueado.transferencias.hay_vivas()
                || tuneles::hay_activos(&estado_bloqueado)
                || estado_bloqueado.ejecuciones.hay_en_curso();
            if !ocupado {
                match estado_bloqueado.vacio_desde {
                    None => {
                        estado_bloqueado.vacio_desde = Some(std::time::Instant::now());
                        false
                    }
                    Some(desde) => desde.elapsed() >= gracia,
                }
            } else {
                estado_bloqueado.vacio_desde = None;
                false
            }
        };
        if apagar {
            apagar_limpio(&estado).await;
            return;
        }
    }
}

/// Difunde la cola cuando toca (como mucho cuatro veces por segundo para el
/// progreso, de inmediato en cada cambio de estado) y purga las terminadas que
/// ya han cumplido su hora.
async fn revisar_cola(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    loop {
        // Un cambio de estado no espera al ritmo: se mira enseguida. El
        // progreso, en cambio, sale como mucho cuatro veces por segundo.
        let espera = {
            let estado_bloqueado = estado.lock().await;
            if estado_bloqueado.difusion_cola.inmediato {
                difusion::ESPERA_INMEDIATA
            } else {
                difusion::RITMO_COLA
            }
        };
        tokio::time::sleep(espera).await;
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado
            .transferencias
            .purgar_caducadas(crate::modelo::fecha_ahora_epoca())
            > 0
        {
            // Lo que desaparece de la cola también hay que decirlo: si no, las
            // ventanas seguirían enseñando lo purgado.
            estado_bloqueado.difusion_cola.marcar(false);
        }
        if !estado_bloqueado
            .difusion_cola
            .toca(std::time::Instant::now())
        {
            continue;
        }
        let lista = estado_bloqueado.transferencias.info();
        difusion::difundir(
            &estado_bloqueado.clientes,
            MensajeServidor::Transferencias { lista },
        );
    }
}

/// Cierra las conexiones del pool que agotaron su gracia sin canales.
async fn revisar_pool(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let vencidas = estado.lock().await.pool.recoger_vencidas();
        for (_, handle, saltos) in vencidas {
            conexiones::desconectar(handle, saltos).await;
        }
    }
}

/// Apagado con limpieza (por `Parar`, `SIGTERM` o inactividad). Primero todo
/// lo que anota o marca, sin tocar la red: túneles retirados con su cierre,
/// sesiones cerradas con el suyo, transferencias abortadas, pool vaciado.
/// Después, con plazo y en paralelo, lo que habla con los hosts. Quien lo pida
/// con otro ya en curso espera a que ese termine.
async fn apagar_limpio(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let en_curso = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            Some(estado_bloqueado.apagado.subscribe())
        } else {
            estado_bloqueado.apagando = true;
            None
        }
    };
    if let Some(mut terminado) = en_curso {
        let _ = tokio::time::timeout(
            PLAZO_APAGADO + Duration::from_secs(2),
            terminado.wait_for(|listo| *listo),
        )
        .await;
        return;
    }

    // Fase 1: anotaciones y estado, sin red.
    ejecuciones::cancelar_todas_al_apagar(estado).await;
    let tuneles = tuneles::retirar_todos(estado, "el servidor se apaga").await;
    let (conexiones, rutas) = {
        let mut estado_bloqueado = estado.lock().await;
        let bd = estado_bloqueado.bd.clone();
        for (_, sesion) in estado_bloqueado.sesiones.drain() {
            // Cada sesión anota su cierre aquí: su tarea ya no la encontrará.
            sesion.cancelar_apertura.cancel();
            let abriendo = sesion.estado == crate::protocolo::EstadoSesionRemota::Abriendo;
            let _ = bd.send(OrdenBd::Anotar {
                tipo: if abriendo {
                    crate::registro::CONEXION_FALLIDA.to_string()
                } else {
                    crate::registro::SESION_CERRADA.to_string()
                },
                host_id: Some(sesion.host_id),
                identidad_id: None,
                detalle: "el servidor se apaga".to_string(),
                resultado: if abriendo {
                    ResultadoRegistro::Error
                } else {
                    ResultadoRegistro::Ok
                },
            });
            let _ = sesion.tx_comandos.send(ComandoSesion::Cerrar);
        }
        // Una transferencia en curso se queda sin canal: pasa a error y el
        // motor borra su parcial.
        estado_bloqueado
            .transferencias
            .abortar_todas("servidor detenido");
        estado_bloqueado.pendientes.clear();
        estado_bloqueado.clientes.clear();
        (
            estado_bloqueado.pool.vaciar(),
            estado_bloqueado.rutas.clone(),
        )
    };

    // Fase 2: la red, en paralelo y con plazo total.
    let red = async {
        let mut tareas: Vec<tokio::task::JoinHandle<()>> = tuneles
            .into_iter()
            .map(|tunel| tokio::spawn(tuneles::cerrar_retirado(tunel)))
            .collect();
        sftp::cerrar_todos(estado).await;
        tareas.extend(
            conexiones
                .into_iter()
                .map(|(handle, saltos)| tokio::spawn(conexiones::desconectar(handle, saltos))),
        );
        for tarea in tareas {
            let _ = tarea.await;
        }
    };
    if tokio::time::timeout(PLAZO_APAGADO, red).await.is_err() {
        warn!("el apagado no cerró todas las conexiones en plazo; se sale igualmente");
    }
    // Los temporales no sobreviven al servidor.
    sftp::vaciar_temporales(&rutas);
    let mut estado_bloqueado = estado.lock().await;
    let _ = estado_bloqueado.apagado.send(true);
    if let Some(tx_apagar) = estado_bloqueado.tx_apagar.take() {
        let _ = tx_apagar.send(());
    }
}

// ---------------------------------------------------------------- clientes

/// Vida de una conexión de cliente: saludo versionado, bucle de lectura y
/// limpieza al desconectar.
async fn tarea_conexion(stream: UnixStream, estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let (lectura, escritura) = stream.into_split();
    let mut entrada = FramedRead::new(lectura, crate::protocolo::codec());
    let tx_escritura = cliente_remoto::lanzar_escritura(escritura);

    // Saludo: el primer mensaje debe ser `Hola` con la versión correcta.
    let saludo = match tokio::time::timeout(Duration::from_secs(10), entrada.next()).await {
        Ok(Some(Ok(linea))) => decodificar::<MensajeCliente>(&linea),
        _ => return,
    };
    let pid_cliente = match saludo {
        Ok(MensajeCliente::Hola { version, pid }) if version == VERSION_PROTOCOLO => pid,
        Ok(MensajeCliente::Hola { version, .. }) => {
            info!("un cliente v{version} no coopera con este servidor v{VERSION_PROTOCOLO}");
            // El cliente recibe la versión del servidor: es lo que le permite
            // decir si el que se quedó atrás es él o el servidor.
            let _ = enviar_linea(
                &tx_escritura,
                MensajeServidor::VersionIncompatible {
                    version: VERSION_PROTOCOLO,
                    pid: Some(std::process::id()),
                },
            );
            return;
        }
        _ => {
            let _ = enviar_linea(
                &tx_escritura,
                MensajeServidor::Error {
                    mensaje: "se esperaba el saludo Hola".to_string(),
                    peticion_id: None,
                },
            );
            return;
        }
    };

    // Cliente admitido: registro y bienvenida con el estado del servidor.
    let cliente_id = {
        let mut estado_bloqueado = estado.lock().await;
        let cliente_id = estado_bloqueado.siguiente_cliente_id;
        estado_bloqueado.siguiente_cliente_id += 1;
        estado_bloqueado.clientes.insert(
            cliente_id,
            ClienteRemoto::nuevo(
                cliente_id,
                VERSION_PROTOCOLO,
                pid_cliente,
                tx_escritura.clone(),
            ),
        );
        cliente_id
    };
    info!(cliente = cliente_id, pid = pid_cliente, "cliente conectado");
    bienvenida(&estado, cliente_id).await;

    // Bucle de mensajes: un mensaje desconocido o mal formado cierra solo a
    // este cliente, nunca el servidor.
    let mut seguir = true;
    while seguir {
        let linea = match entrada.next().await {
            Some(Ok(linea)) => linea,
            _ => break,
        };
        match decodificar::<MensajeCliente>(&linea) {
            Ok(mensaje) => {
                seguir = manejar_mensaje(&estado, cliente_id, mensaje).await;
            }
            Err(motivo) => {
                let _ = enviar_linea(
                    &tx_escritura,
                    MensajeServidor::Error {
                        mensaje: format!("mensaje no válido: {motivo}"),
                        peticion_id: None,
                    },
                );
                break;
            }
        }
    }
    info!(cliente = cliente_id, "cliente desconectado");
    limpiar_cliente(&estado, cliente_id).await;
}

fn enviar_linea(tx: &mpsc::UnboundedSender<MensajeServidor>, mensaje: MensajeServidor) -> bool {
    tx.send(mensaje).is_ok()
}

/// `Bienvenida` con la versión, el pid del servidor, las sesiones vivas y la
/// cola de transferencias (para que una ventana nueva la vea entera).
async fn bienvenida(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, cliente_id: u32) {
    let estado_bloqueado = estado.lock().await;
    let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) else {
        return;
    };
    let mensaje = MensajeServidor::Bienvenida {
        version: VERSION_PROTOCOLO,
        pid: std::process::id(),
        cliente_id,
        clientes: estado_bloqueado.clientes.len() as u32,
        sesiones: sesiones::lista(&estado_bloqueado.sesiones),
        transferencias: estado_bloqueado.transferencias.info(),
        tuneles: tuneles::lista(&estado_bloqueado),
        ejecuciones: estado_bloqueado.ejecuciones.info(),
    };
    difusion::enviar(cliente, mensaje);
}

/// Despacha un mensaje de un cliente; devuelve si la conexión continúa.
async fn manejar_mensaje(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    mensaje: MensajeCliente,
) -> bool {
    match mensaje {
        MensajeCliente::AbrirSesion {
            host_id,
            cols,
            filas,
            comandos_iniciales,
        } => {
            abrir_sesion(estado, cliente_id, host_id, cols, filas, comandos_iniciales).await;
        }
        MensajeCliente::Adjuntar {
            sesion_id,
            cols,
            filas,
        } => {
            adjuntar(estado, cliente_id, sesion_id, Tamano { cols, filas }).await;
        }
        MensajeCliente::Desadjuntar { sesion_id } => {
            desadjuntar(estado, cliente_id, sesion_id).await;
        }
        MensajeCliente::Teclas { sesion_id, bytes } => {
            let mut estado_bloqueado = estado.lock().await;
            if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
                if sesion.adjuntos.contains_key(&cliente_id) {
                    let _ = sesion.tx_comandos.send(ComandoSesion::Teclas(bytes));
                } else {
                    warn!(sesion = sesion_id, "teclas de un cliente no adjunto");
                }
            }
        }
        MensajeCliente::Redimensionar {
            sesion_id,
            cols,
            filas,
        } => {
            redimensionar_adjunto(estado, cliente_id, sesion_id, Tamano { cols, filas }).await;
        }
        MensajeCliente::Cerrar { sesion_id } => {
            cerrar_sesion(estado, cliente_id, sesion_id).await;
        }
        MensajeCliente::Reconectar { sesion_id } => {
            reconectar(estado, cliente_id, sesion_id).await;
        }
        MensajeCliente::DecisionHuella {
            sesion_id,
            decision,
        } => {
            sesiones::resolver_decision(
                estado,
                sesion_id,
                cliente_id,
                DecisionDialogo::Huella(decision),
            )
            .await;
        }
        MensajeCliente::Frase { sesion_id, frase } => {
            sesiones::resolver_decision(
                estado,
                sesion_id,
                cliente_id,
                DecisionDialogo::Frase(frase),
            )
            .await;
        }
        MensajeCliente::Contrasena {
            sesion_id,
            contrasena,
            recordar,
        } => {
            sesiones::resolver_decision(
                estado,
                sesion_id,
                cliente_id,
                DecisionDialogo::Contrasena(contrasena, recordar),
            )
            .await;
        }
        // El sondeo por la conexión viva va a su propia tarea: un host lento
        // no puede parar el bucle de lectura de este cliente.
        MensajeCliente::Ejecutar {
            host_id,
            comando,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                ejecutar(&estado_tarea, cliente_id, host_id, &comando, peticion_id).await;
            });
        }
        // Un túnel puede tardar en levantarse (abrir la conexión, pedir el
        // reenvío, los diálogos de siempre), así que va a una tarea propia: el
        // bucle de lectura de este cliente no puede quedarse esperando.
        MensajeCliente::ActivarTunel {
            tunel_id,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                activar_tunel(&estado_tarea, cliente_id, tunel_id, peticion_id).await;
            });
        }
        MensajeCliente::PararTunel {
            tunel_id,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                parar_tunel(&estado_tarea, cliente_id, tunel_id, peticion_id).await;
            });
        }
        MensajeCliente::RelanzarTunel {
            tunel_id,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                relanzar_tunel(&estado_tarea, cliente_id, tunel_id, peticion_id).await;
            });
        }
        MensajeCliente::RecargarTuneles { host_id } => {
            // Parar túneles habla con el host (cancelar reenvíos) y espera a las
            // copias: mejor en su tarea que en el bucle de lectura de este
            // cliente.
            let estado_tarea = estado.clone();
            tokio::spawn(async move { tuneles::recargar(&estado_tarea, host_id).await });
        }
        // Las operaciones de archivos van a una tarea propia: la apertura de
        // un canal puede tardar (diálogos, red) y el bucle de lectura de este
        // cliente no puede quedarse esperando o su respuesta no llegaría
        // nunca.
        MensajeCliente::AbrirSftp {
            host_id,
            peticion_id,
            no_interactivo,
        } => {
            let estado_tarea = estado.clone();
            let modo = if no_interactivo {
                sftp::ModoSftp::Comprobacion
            } else {
                sftp::ModoSftp::Archivos
            };
            tokio::spawn(async move {
                abrir_sftp(&estado_tarea, cliente_id, host_id, peticion_id, modo).await
            });
        }
        MensajeCliente::ListarDir {
            host_id,
            ruta,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                listar_dir(&estado_tarea, cliente_id, host_id, ruta, peticion_id).await;
            });
        }
        MensajeCliente::Transferir {
            host_id,
            direccion,
            elementos,
            politica,
            borrar_origen,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                transferir(
                    &estado_tarea,
                    cliente_id,
                    host_id,
                    direccion,
                    elementos,
                    politica,
                    borrar_origen,
                )
                .await;
            });
        }
        MensajeCliente::CancelarTransferencia { id } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move { cancelar_transferencia(&estado_tarea, id).await });
        }
        MensajeCliente::LimpiarTransferencias => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move { limpiar_transferencias(&estado_tarea).await });
        }
        MensajeCliente::BorrarRemoto {
            host_id,
            rutas,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                borrar_remoto(&estado_tarea, cliente_id, host_id, rutas, peticion_id).await;
            });
        }
        MensajeCliente::RenombrarRemoto {
            host_id,
            de,
            a,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                renombrar_remoto(&estado_tarea, cliente_id, host_id, de, a, peticion_id).await;
            });
        }
        MensajeCliente::CrearDirRemoto {
            host_id,
            ruta,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                crear_dir_remoto(&estado_tarea, cliente_id, host_id, ruta, peticion_id).await;
            });
        }
        MensajeCliente::DescargarTemporal {
            host_id,
            ruta,
            peticion_id,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                descargar_temporal(&estado_tarea, cliente_id, host_id, ruta, peticion_id).await;
            });
        }
        MensajeCliente::BorrarTemporal { ruta } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move { borrar_temporal(&estado_tarea, cliente_id, ruta).await });
        }
        // Una ejecución abre conexiones y espera a los hosts: en su tarea.
        MensajeCliente::LanzarEjecucion {
            peticion_id,
            snippet_id,
            nombre,
            comando,
            host_ids,
            timeout_seg,
            parar_al_fallo,
            deliberacion,
        } => {
            let estado_tarea = estado.clone();
            tokio::spawn(async move {
                let peticion = ejecuciones::PeticionLanzar {
                    peticion_id,
                    snippet_id,
                    nombre,
                    comando,
                    host_ids,
                    timeout_seg,
                    parar_al_fallo,
                    deliberacion,
                };
                match ejecuciones::lanzar(&estado_tarea, cliente_id, peticion).await {
                    Ok(_) => {
                        responder(
                            &estado_tarea,
                            cliente_id,
                            MensajeServidor::Hecho { peticion_id },
                        )
                        .await;
                    }
                    Err(motivo) => {
                        responder_error(&estado_tarea, cliente_id, motivo, Some(peticion_id)).await;
                    }
                }
            });
        }
        MensajeCliente::CancelarEjecucion { id } => {
            ejecuciones::cancelar(estado, id).await;
        }
        MensajeCliente::LimpiarEjecuciones => {
            ejecuciones::limpiar(estado).await;
        }
        MensajeCliente::PedirSalida {
            ejecucion_id,
            host_id,
        } => {
            ejecuciones::pedir_salida(estado, cliente_id, ejecucion_id, host_id).await;
        }
        MensajeCliente::Listar => {
            let lista = sesiones::lista(&estado.lock().await.sesiones);
            let estado_bloqueado = estado.lock().await;
            if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
                difusion::enviar(cliente, MensajeServidor::Sesiones { lista });
            }
        }
        MensajeCliente::Parar => {
            info!(cliente = cliente_id, "parando el servidor por orden");
            apagar_limpio(estado).await;
            return false;
        }
        MensajeCliente::Adios => {
            return false;
        }
        // El saludo ya se consumió en `tarea_conexion`.
        MensajeCliente::Hola { .. } => {
            let estado_bloqueado = estado.lock().await;
            if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
                difusion::enviar(
                    cliente,
                    MensajeServidor::Error {
                        mensaje: "saludo duplicado".to_string(),
                        peticion_id: None,
                    },
                );
            }
        }
    }
    true
}

/// Limpieza al caer o despedirse un cliente: desadjuntar, cancelar aperturas
/// que solicitó y difundir el cambio.
async fn limpiar_cliente(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, cliente_id: u32) {
    let mut estado_bloqueado = estado.lock().await;
    let mut cambios_tamano: Vec<(u32, ComandoSesion)> = Vec::new();
    for (sesion_id, sesion) in estado_bloqueado.sesiones.iter_mut() {
        if sesion.adjuntos.remove(&cliente_id).is_some() {
            let nuevo = sesiones::tamano_minimo(&sesion.adjuntos, sesion.tamano);
            if nuevo != sesion.tamano {
                sesion.tamano = nuevo;
                cambios_tamano.push((
                    *sesion_id,
                    ComandoSesion::AplicarTamano(nuevo.cols, nuevo.filas),
                ));
            }
        }
    }
    for (sesion_id, comando) in cambios_tamano {
        let (ids, tamano) = {
            let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
                continue;
            };
            let _ = sesion.tx_comandos.send(comando);
            let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
            (ids, sesion.tamano)
        };
        difusion::difundir_a_adjuntos(
            &estado_bloqueado.clientes,
            &ids,
            MensajeServidor::Redimensionada {
                sesion_id,
                cols: tamano.cols,
                filas: tamano.filas,
                ventana_minima: None,
            },
        );
    }
    // Diálogos que solo esta ventana podía contestar (sesiones, SFTP,
    // túneles, ejecuciones): se sueltan y sus aperturas fallan enseguida en
    // vez de retener el cerrojo del host hasta el plazo.
    estado_bloqueado
        .pendientes
        .retain(|_, pendiente| pendiente.solicitante != cliente_id);
    // Aperturas que este cliente solicitó: se cancelan («ventana cerrada»).
    let caidas: Vec<u32> = estado_bloqueado
        .sesiones
        .iter()
        .filter(|(_, sesion)| {
            sesion.solicitante == cliente_id
                && sesion.estado == crate::protocolo::EstadoSesionRemota::Abriendo
        })
        .map(|(id, _)| *id)
        .collect();
    let mut hosts_sin_canal: Vec<i64> = Vec::new();
    for sesion_id in caidas {
        estado_bloqueado.pendientes.remove(&sesion_id);
        if let Some(sesion) = estado_bloqueado.sesiones.remove(&sesion_id) {
            sesion.cancelar_apertura.cancel();
            hosts_sin_canal.push(sesion.host_id);
            let bd = estado_bloqueado.bd.clone();
            let _ = bd.send(OrdenBd::Anotar {
                tipo: crate::registro::CONEXION_FALLIDA.to_string(),
                host_id: Some(sesion.host_id),
                identidad_id: None,
                detalle: "ventana cerrada".to_string(),
                resultado: ResultadoRegistro::Error,
            });
        }
    }
    // Túneles que esta ventana estaba levantando: sin ella no hay quien
    // conteste a los diálogos, así que se cancelan en vez de quedarse en
    // «activando» hasta que venza el plazo.
    let activando: Vec<i64> = estado_bloqueado
        .tuneles
        .values()
        .filter(|activo| {
            activo.solicitante == cliente_id
                && activo.estado == crate::protocolo::EstadoTunelRemoto::Activando
        })
        .map(|activo| activo.tunel_id)
        .collect();
    estado_bloqueado.clientes.remove(&cliente_id);
    difundir_lista(&mut estado_bloqueado);
    drop(estado_bloqueado);
    for tunel_id in activando {
        tuneles::caido_por_solicitante(estado, tunel_id, "la ventana que lo pidió se cerró").await;
    }
    // Las pestañas de la ventana que se va sueltan su canal: si eran el último
    // del host, sus túneles automáticos se paran.
    for host_id in hosts_sin_canal {
        let estado_tuneles = estado.clone();
        tokio::spawn(async move { tuneles::canales_cambiaron(&estado_tuneles, host_id).await });
    }
}

// ---------------------------------------------------------------- sesiones

/// Crea la sesión en estado «abriendo» y lanza su tarea de conexión.
async fn abrir_sesion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    cols: u16,
    filas: u16,
    comandos_iniciales: Vec<crate::protocolo::ComandoInicial>,
) {
    // Lo que se escribe en la shell viene del cliente: se acota.
    let comandos_iniciales: Vec<crate::protocolo::ComandoInicial> = comandos_iniciales
        .into_iter()
        .filter(|comando| {
            !comando.texto.is_empty()
                && comando.texto.len() <= sesiones::TOPE_COMANDO_INICIAL
                && !comando.texto.contains('\0')
        })
        .collect();
    let (host, todos_los_hosts, rutas) = {
        let estado_bloqueado = estado.lock().await;
        let Ok(lectura) = estado_bloqueado.lectura.lock() else {
            return;
        };
        let todos_los_hosts: HashMap<i64, crate::modelo::Host> = lectura
            .listar_hosts()
            .unwrap_or_default()
            .into_iter()
            .map(|host| (host.id, host))
            .collect();
        (
            lectura.obtener_host(host_id),
            todos_los_hosts,
            estado_bloqueado.rutas.clone(),
        )
    };
    let Ok(host) = host else {
        let estado_bloqueado = estado.lock().await;
        if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
            difusion::enviar(
                cliente,
                MensajeServidor::Error {
                    mensaje: "el host ya no existe".to_string(),
                    peticion_id: None,
                },
            );
        }
        return;
    };

    let mut estado_bloqueado = estado.lock().await;
    if estado_bloqueado.apagando {
        return;
    }
    let sesion_id = estado_bloqueado.siguiente_sesion_id;
    estado_bloqueado.siguiente_sesion_id += 1;
    let nombre = sesiones::nombre_de_pestaña(&estado_bloqueado.sesiones, &host);
    let (tx_comandos, rx_comandos) = mpsc::unbounded_channel();
    // Tamaños degenerados (un terminal sin tamaño aún) paniquean al parser.
    let tamano = Tamano {
        cols: cols.max(2),
        filas: filas.max(1),
    };
    let pantalla = crate::conexion::terminal::nuevo(filas, cols);
    let cancelar = tokio_util::sync::CancellationToken::new();
    estado_bloqueado.sesiones.insert(
        sesion_id,
        Sesion {
            id: sesion_id,
            host_id,
            host_nombre: host.nombre.clone(),
            nombre,
            estado: crate::protocolo::EstadoSesionRemota::Abriendo,
            motivo: Some("abriendo".to_string()),
            identidad: String::new(),
            adjuntos: HashMap::new(),
            solicitante: cliente_id,
            abierta_en: chrono::Utc::now().timestamp(),
            ultima_actividad: chrono::Utc::now().timestamp(),
            actividad_no_vista: false,
            handle: None,
            cancelar_apertura: cancelar.clone(),
            reconectando: false,
            comandos_iniciales: comandos_iniciales.clone(),
            tamano,
            pantalla: pantalla.clone(),
            tx_comandos,
        },
    );
    estado_bloqueado.vacio_desde = None;
    let datos = sesiones::DatosApertura {
        host,
        todos_los_hosts,
        rutas,
        pantalla: pantalla.clone(),
        cols,
        filas,
        sesion_id,
        solicitante: cliente_id,
        reconexion: false,
        cancelar,
        comandos: comandos_iniciales
            .into_iter()
            .map(|comando| comando.texto)
            .collect(),
    };
    // Los túneles automáticos del host se levantan cuando la pestaña queda
    // abierta (lo hace su tarea), no mientras sus diálogos esperan.
    sesiones::lanzar(estado.clone(), datos, rx_comandos);
    sesiones::difundir_lista(&mut estado_bloqueado);
}

/// Recalcula el tamaño de la sesión y lo aplica si cambió.
async fn aplicar_tamano(estado_bloqueado: &mut EstadoServidor, sesion_id: u32) {
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
        return;
    };
    let nuevo = sesiones::tamano_minimo(&sesion.adjuntos, sesion.tamano);
    if nuevo != sesion.tamano {
        sesion.tamano = nuevo;
        let _ = sesion
            .tx_comandos
            .send(ComandoSesion::AplicarTamano(nuevo.cols, nuevo.filas));
        let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
        // La ventana que impone el mínimo, para el aviso de la barra.
        let minima = sesion
            .adjuntos
            .iter()
            .filter(|(_, tamano)| tamano.cols == nuevo.cols && tamano.filas == nuevo.filas)
            .map(|(cliente_id, _)| *cliente_id)
            .min();
        difusion::difundir_a_adjuntos(
            &estado_bloqueado.clientes,
            &ids,
            MensajeServidor::Redimensionada {
                sesion_id,
                cols: nuevo.cols,
                filas: nuevo.filas,
                ventana_minima: minima,
            },
        );
    }
}

async fn adjuntar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    sesion_id: u32,
    tamano: Tamano,
) {
    let tamano = Tamano {
        cols: tamano.cols.max(2),
        filas: tamano.filas.max(1),
    };
    let mut estado_bloqueado = estado.lock().await;
    let volcado = {
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return;
        };
        sesion.adjuntos.insert(cliente_id, tamano);
        sesion.actividad_no_vista = false;
        sesion
            .pantalla
            .lock()
            .map(|parser| parser.screen().contents_formatted())
            .unwrap_or_default()
    };
    let (cols, filas) = {
        let Some(sesion) = estado_bloqueado.sesiones.get(&sesion_id) else {
            return;
        };
        (sesion.tamano.cols, sesion.tamano.filas)
    };
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        difusion::enviar(
            cliente,
            MensajeServidor::PantallaCompleta {
                sesion_id,
                bytes: volcado,
                cols,
                filas,
            },
        );
    }
    aplicar_tamano(&mut estado_bloqueado, sesion_id).await;
    sesiones::difundir_lista(&mut estado_bloqueado);
}

async fn desadjuntar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    sesion_id: u32,
) {
    let mut estado_bloqueado = estado.lock().await;
    if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
        sesion.adjuntos.remove(&cliente_id);
    }
    aplicar_tamano(&mut estado_bloqueado, sesion_id).await;
    sesiones::difundir_lista(&mut estado_bloqueado);
}

async fn redimensionar_adjunto(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    sesion_id: u32,
    tamano: Tamano,
) {
    let tamano = Tamano {
        cols: tamano.cols.max(2),
        filas: tamano.filas.max(1),
    };
    let mut estado_bloqueado = estado.lock().await;
    if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
        if sesion.adjuntos.contains_key(&cliente_id) {
            sesion.adjuntos.insert(cliente_id, tamano);
        }
    }
    aplicar_tamano(&mut estado_bloqueado, sesion_id).await;
}

/// Cierra una sesión: si abría, cancela; si está caída, se elimina; si está
/// abierta, el bucle hace el trabajo al recibir `Cerrar`. Un id que no es de
/// sesión es el de una apertura de SFTP, túnel o ejecución cuyo diálogo el
/// solicitante rechaza: se suelta para que falle enseguida.
async fn cerrar_sesion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    sesion_id: u32,
) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get(&sesion_id) else {
        drop(estado_bloqueado);
        sesiones::cancelar_dialogo(estado, sesion_id, cliente_id).await;
        return;
    };
    match sesion.estado {
        crate::protocolo::EstadoSesionRemota::Abriendo
        | crate::protocolo::EstadoSesionRemota::Caida => {
            let fallida_apertura = sesion.estado == crate::protocolo::EstadoSesionRemota::Abriendo;
            let host_id = sesion.host_id;
            // La apertura en curso se abandona: suelta el cerrojo del host.
            sesion.cancelar_apertura.cancel();
            estado_bloqueado.pendientes.remove(&sesion_id);
            estado_bloqueado.sesiones.remove(&sesion_id);
            let bd = estado_bloqueado.bd.clone();
            let _ = bd.send(OrdenBd::Anotar {
                tipo: if fallida_apertura {
                    crate::registro::CONEXION_FALLIDA.to_string()
                } else {
                    crate::registro::SESION_CERRADA.to_string()
                },
                host_id: Some(host_id),
                identidad_id: None,
                detalle: if fallida_apertura {
                    "ventana cerrada".to_string()
                } else {
                    "cerrada desde la ventana".to_string()
                },
                resultado: if fallida_apertura {
                    ResultadoRegistro::Error
                } else {
                    ResultadoRegistro::Ok
                },
            });
            sesiones::difundir_lista(&mut estado_bloqueado);
            drop(estado_bloqueado);
            // Se ha ido una pestaña: si era la última de ese host, sus túneles
            // automáticos se paran.
            let estado_tuneles = estado.clone();
            tokio::spawn(async move {
                tuneles::canales_cambiaron(&estado_tuneles, host_id).await;
            });
        }
        crate::protocolo::EstadoSesionRemota::Abierta => {
            let _ = sesion.tx_comandos.send(ComandoSesion::Cerrar);
        }
        crate::protocolo::EstadoSesionRemota::Cerrada => {}
    }
}

/// Reconecta una sesión caída: conexión nueva, mismo id y nombre. Si el host
/// ya no existe, se rechaza y la pestaña solo puede cerrarse.
async fn reconectar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    sesion_id: u32,
) {
    let (nombre_host, estado_actual, rutas) = {
        let estado_bloqueado = estado.lock().await;
        match estado_bloqueado.sesiones.get(&sesion_id) {
            Some(sesion) => (
                sesion.host_nombre.clone(),
                sesion.estado,
                estado_bloqueado.rutas.clone(),
            ),
            None => return,
        }
    };
    if estado_actual != crate::protocolo::EstadoSesionRemota::Caida {
        let estado_bloqueado = estado.lock().await;
        if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
            difusion::enviar(
                cliente,
                MensajeServidor::Error {
                    mensaje: format!("«{nombre_host}» ya está abierta"),
                    peticion_id: None,
                },
            );
        }
        return;
    }
    let (host, todos_los_hosts) = {
        let estado_bloqueado = estado.lock().await;
        let Ok(lectura) = estado_bloqueado.lectura.lock() else {
            return;
        };
        let todos: HashMap<i64, crate::modelo::Host> = lectura
            .listar_hosts()
            .unwrap_or_default()
            .into_iter()
            .map(|host| (host.id, host))
            .collect();
        (
            lectura.obtener_host(host_id_de(&estado_bloqueado, sesion_id)),
            todos,
        )
    };
    let Ok(host) = host else {
        // Host borrado: la sesión sigue caída y solo puede cerrarse.
        let estado_bloqueado = estado.lock().await;
        if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
            difusion::enviar(
                cliente,
                MensajeServidor::Estado {
                    sesion_id,
                    estado: crate::protocolo::EstadoSesionRemota::Caida,
                    motivo: Some("el host ya no existe en el inventario".to_string()),
                },
            );
        }
        return;
    };

    // Comprobar que sigue caída y marcarla «abriendo» en el mismo bloqueo: dos
    // `Reconectar` seguidos no pueden lanzar dos aperturas.
    let (tx_comandos, rx_comandos) = mpsc::unbounded_channel();
    let cancelar = tokio_util::sync::CancellationToken::new();
    let (cols, filas, pantalla, comandos) = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            return;
        }
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return;
        };
        if sesion.estado != crate::protocolo::EstadoSesionRemota::Caida {
            return;
        }
        let (cols, filas) = (sesion.tamano.cols, sesion.tamano.filas);
        let pantalla = crate::conexion::terminal::nuevo(filas, cols);
        sesion.estado = crate::protocolo::EstadoSesionRemota::Abriendo;
        sesion.reconectando = true;
        sesion.cancelar_apertura = cancelar.clone();
        sesion.motivo = Some("reconectando".to_string());
        sesion.handle = None;
        sesion.solicitante = cliente_id;
        sesion.tx_comandos = tx_comandos;
        sesion.pantalla = pantalla.clone();
        // Al reconectar solo se repite lo que es «al conectar».
        let comandos: Vec<String> = sesion
            .comandos_iniciales
            .iter()
            .filter(|comando| comando.repetir)
            .map(|comando| comando.texto.clone())
            .collect();
        (cols, filas, pantalla, comandos)
    };
    let datos = sesiones::DatosApertura {
        host,
        todos_los_hosts,
        rutas,
        pantalla,
        cols,
        filas,
        sesion_id,
        solicitante: cliente_id,
        reconexion: true,
        cancelar,
        comandos,
    };
    sesiones::lanzar(estado.clone(), datos, rx_comandos);
    sesiones::difundir_lista(&mut *estado.lock().await);
}

fn host_id_de(estado: &EstadoServidor, sesion_id: u32) -> i64 {
    estado
        .sesiones
        .get(&sesion_id)
        .map(|sesion| sesion.host_id)
        .unwrap_or(0)
}

/// `Ejecutar` (el sondeo de Flota): un canal `exec` sobre la conexión viva del
/// host, tomada por la vía única (`SoloViva`: la del pool o, si no, la de una
/// pestaña abierta; nunca conecta). Contesta `Ejecutado` o `SinSesion` con el
/// mismo `peticion_id`; sin conexión viva el cliente cae a su efímera.
async fn ejecutar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    comando: &str,
    peticion_id: u64,
) {
    let (host, rutas) = {
        let estado_bloqueado = estado.lock().await;
        let host = estado_bloqueado
            .lectura
            .lock()
            .ok()
            .and_then(|lectura| lectura.obtener_host(host_id).ok());
        (host, estado_bloqueado.rutas.clone())
    };
    let tomada = match host {
        Some(host) => {
            conexiones::conexion_para_canal(
                estado,
                &host,
                &HashMap::new(),
                &rutas,
                conexiones::PoliticaConexion::SoloViva,
                conexiones::UsoCanal::Subsistema,
            )
            .await
        }
        None => Err("el host ya no existe".to_string()),
    };
    let respuesta = match tomada {
        Ok(tomada) => {
            let resultado = ejecutar_en_handle(&tomada.handle, comando, PLAZO_EJECUTAR).await;
            tomada.soltar().await;
            match resultado {
                Ok((salida, codigo)) => MensajeServidor::Ejecutado {
                    host_id,
                    peticion_id,
                    salida,
                    codigo,
                },
                // Una conexión que se está cerrando hace caer el exec: el
                // cliente repite con su conexión efímera.
                Err(motivo) => {
                    warn!(host_id, "el exec del sondeo falló: {motivo}");
                    MensajeServidor::SinSesion {
                        host_id,
                        peticion_id,
                    }
                }
            }
        }
        Err(_) => MensajeServidor::SinSesion {
            host_id,
            peticion_id,
        },
    };
    responder(estado, cliente_id, respuesta).await;
}

/// Canal `exec` sobre una conexión viva, con plazo; devuelve la salida
/// (stdout y stderr juntos) y el código de salida. Se lee hasta el cierre del
/// canal: el `exit-status` llega después del EOF. El canal se cierra siempre.
async fn ejecutar_en_handle(
    handle: &Arc<russh::client::Handle<crate::conexion::cliente::Cliente>>,
    comando: &str,
    plazo: Duration,
) -> Result<(String, i32), String> {
    /// Tope de lo que se guarda de la salida del sondeo.
    const TOPE: usize = 4 * 1024 * 1024;
    let limite = tokio::time::Instant::now() + plazo;
    // Una conexión medio muerta se delata al abrir el canal: plazo corto, y
    // el cliente cae a su conexión efímera antes de rendirse.
    let apertura = handle.clone();
    let mut canal = conexiones::abrir_con_plazo(PLAZO_CANAL_EJECUTAR, async move {
        apertura.channel_open_session().await
    })
    .await?;
    let mut salida = Vec::new();
    let mut codigo = None;
    let trabajo = async {
        canal
            .exec(true, comando.as_bytes().to_vec())
            .await
            .map_err(|error| error.to_string())?;
        let _ = canal.eof().await;
        loop {
            match canal.wait().await {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if salida.len() < TOPE {
                        salida.extend_from_slice(&data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => codigo = Some(exit_status as i32),
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        Ok::<(), String>(())
    };
    let resultado = tokio::time::timeout_at(limite, trabajo).await;
    let _ = canal.close().await;
    match resultado {
        Err(_) => Err("se agotó el plazo del comando".to_string()),
        Ok(Err(motivo)) => Err(motivo),
        Ok(Ok(())) => Ok((
            String::from_utf8_lossy(&salida).to_string(),
            codigo.unwrap_or(-1),
        )),
    }
}

// ---------------------------------------------------------------- archivos

/// Responde a un cliente concreto (si sigue conectado).
async fn responder(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    mensaje: MensajeServidor,
) {
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        difusion::enviar(cliente, mensaje);
    }
}

async fn responder_error(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    mensaje: String,
    peticion_id: Option<u64>,
) {
    responder(
        estado,
        cliente_id,
        MensajeServidor::Error {
            mensaje,
            peticion_id,
        },
    )
    .await;
}

// ---------------------------------------------------------------- túneles

/// `ActivarTunel`: levanta el túnel y contesta `Hecho` o `Error`. El estado va
/// aparte, por la difusión `Tuneles`.
async fn activar_tunel(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    tunel_id: i64,
    peticion_id: u64,
) {
    let resultado = tuneles::activar(
        estado,
        tunel_id,
        crate::protocolo::OrigenTunel::Manual,
        Some(cliente_id),
    )
    .await;
    // La respuesta sale con la anotación ya escrita (T21).
    confirmar_escritura(estado).await;
    match resultado {
        Ok(()) => responder(estado, cliente_id, MensajeServidor::Hecho { peticion_id }).await,
        Err(motivo) => responder_error(estado, cliente_id, motivo, Some(peticion_id)).await,
    }
}

/// `PararTunel`: para el túnel. Sobre uno caído, descarta el error.
async fn parar_tunel(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    tunel_id: i64,
    peticion_id: u64,
) {
    let caido = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .tuneles
            .get(&tunel_id)
            .is_some_and(|activo| activo.estado == crate::protocolo::EstadoTunelRemoto::Caido)
    };
    if caido {
        tuneles::descartar(estado, tunel_id).await;
    } else {
        tuneles::parar(estado, tunel_id, "parado a mano", true)
            .await
            .ok();
    }
    confirmar_escritura(estado).await;
    responder(estado, cliente_id, MensajeServidor::Hecho { peticion_id }).await;
}

/// `RelanzarTunel`: vuelve a levantar un túnel caído, con los contadores a cero.
async fn relanzar_tunel(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    tunel_id: i64,
    peticion_id: u64,
) {
    let resultado = tuneles::relanzar(estado, tunel_id, cliente_id).await;
    confirmar_escritura(estado).await;
    match resultado {
        Ok(()) => responder(estado, cliente_id, MensajeServidor::Hecho { peticion_id }).await,
        Err(motivo) => responder_error(estado, cliente_id, motivo, Some(peticion_id)).await,
    }
}

/// `AbrirSftp`: deja el canal listo y contesta con el directorio de inicio del
/// usuario remoto.
async fn abrir_sftp(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    peticion_id: Option<u64>,
    modo: sftp::ModoSftp,
) {
    match sftp::asegurar(estado, host_id, cliente_id, modo).await {
        Ok((_, dir_inicio)) => {
            responder(
                estado,
                cliente_id,
                MensajeServidor::SftpAbierto {
                    host_id,
                    dir_inicio,
                    peticion_id,
                },
            )
            .await;
        }
        Err(motivo) => responder_error(estado, cliente_id, motivo, peticion_id).await,
    }
}

/// Sesión del canal del host, o el error que hay que contestar.
async fn sesion_o_error(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    peticion_id: u64,
) -> Option<Arc<russh_sftp::client::SftpSession>> {
    match sftp::canal_abierto(estado, host_id).await {
        Some(sesion) => Some(sesion),
        None => {
            responder_error(
                estado,
                cliente_id,
                "no hay canal SFTP abierto para ese host".to_string(),
                Some(peticion_id),
            )
            .await;
            None
        }
    }
}

/// Traduce el fallo de una operación SFTP y, si el canal se perdió, lo cierra
/// y marca en error sus transferencias.
async fn fallo_sftp(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    motivo: String,
) -> String {
    if motivo.contains("se cayó la conexión") {
        sftp::perdido(estado, host_id).await;
    }
    motivo
}

async fn listar_dir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    ruta: String,
    peticion_id: u64,
) {
    let Some(sesion) = sesion_o_error(estado, cliente_id, host_id, peticion_id).await else {
        return;
    };
    match sftp::listar(&sesion, &ruta).await {
        Ok(entradas) => {
            responder(
                estado,
                cliente_id,
                MensajeServidor::DirListado {
                    host_id,
                    ruta,
                    entradas,
                    peticion_id,
                },
            )
            .await;
        }
        Err(motivo) => {
            let motivo = fallo_sftp(estado, host_id, motivo).await;
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        }
    }
}

async fn borrar_remoto(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    rutas: Vec<String>,
    peticion_id: u64,
) {
    let Some(sesion) = sesion_o_error(estado, cliente_id, host_id, peticion_id).await else {
        return;
    };
    for ruta in &rutas {
        if let Err(motivo) = sftp::borrar(&sesion, ruta).await {
            let motivo = fallo_sftp(estado, host_id, motivo).await;
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    }
    // El efecto ya se ha producido: se anota ahora.
    let estado_bloqueado = estado.lock().await;
    let _ = estado_bloqueado.bd.send(OrdenBd::Anotar {
        tipo: crate::registro::BORRADO_REMOTO.to_string(),
        host_id: Some(host_id),
        identidad_id: None,
        detalle: format!(
            "borrado remoto de {} ruta(s) · {}",
            rutas.len(),
            rutas.join(", ")
        ),
        resultado: ResultadoRegistro::Ok,
    });
    drop(estado_bloqueado);
    // `Hecho` sale con la fila ya escrita (T21): quien lo reciba puede leer
    // `REGISTRO` y encontrarla.
    confirmar_escritura(estado).await;
    responder(estado, cliente_id, MensajeServidor::Hecho { peticion_id }).await;
}

async fn renombrar_remoto(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    de: String,
    a: String,
    peticion_id: u64,
) {
    let Some(sesion) = sesion_o_error(estado, cliente_id, host_id, peticion_id).await else {
        return;
    };
    match sftp::renombrar(&sesion, &de, &a).await {
        Ok(()) => {
            responder(estado, cliente_id, MensajeServidor::Hecho { peticion_id }).await;
        }
        Err(motivo) => {
            let motivo = fallo_sftp(estado, host_id, motivo).await;
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        }
    }
}

async fn crear_dir_remoto(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    ruta: String,
    peticion_id: u64,
) {
    let Some(sesion) = sesion_o_error(estado, cliente_id, host_id, peticion_id).await else {
        return;
    };
    match sftp::crear_dir(&sesion, &ruta).await {
        Ok(()) => {
            responder(estado, cliente_id, MensajeServidor::Hecho { peticion_id }).await;
        }
        Err(motivo) => {
            let motivo = fallo_sftp(estado, host_id, motivo).await;
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        }
    }
}

async fn descargar_temporal(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    ruta: String,
    peticion_id: u64,
) {
    let Some(sesion) = sesion_o_error(estado, cliente_id, host_id, peticion_id).await else {
        return;
    };
    let rutas = estado.lock().await.rutas.clone();
    match sftp::copia_temporal(&rutas, &sesion, &ruta, peticion_id).await {
        Ok(temporal) => {
            responder(
                estado,
                cliente_id,
                MensajeServidor::RutaTemporal {
                    ruta: temporal,
                    peticion_id,
                },
            )
            .await;
        }
        Err(motivo) => {
            let motivo = fallo_sftp(estado, host_id, motivo).await;
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        }
    }
}

async fn borrar_temporal(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    ruta: String,
) {
    let rutas = estado.lock().await.rutas.clone();
    if let Err(motivo) = sftp::borrar_temporal(&rutas, &ruta) {
        responder_error(estado, cliente_id, motivo, None).await;
    }
}

async fn transferir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    direccion: crate::protocolo::Direccion,
    elementos: Vec<crate::protocolo::ElementoTransferencia>,
    politica: crate::protocolo::Politica,
    borrar_origen: bool,
) {
    // El canal debe estar abierto: la vista Archivos lo pide al entrar.
    if sftp::canal_abierto(estado, host_id).await.is_none() {
        if let Err(motivo) =
            sftp::asegurar(estado, host_id, cliente_id, sftp::ModoSftp::Archivos).await
        {
            responder_error(estado, cliente_id, motivo, None).await;
            return;
        }
    }
    match transferencias::encolar(
        estado,
        cliente_id,
        host_id,
        direccion,
        elementos,
        politica,
        borrar_origen,
    )
    .await
    {
        Ok(_) => {}
        Err(motivo) => responder_error(estado, cliente_id, motivo, None).await,
    }
}

async fn cancelar_transferencia(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, id: u32) {
    let mut estado_bloqueado = estado.lock().await;
    if estado_bloqueado.transferencias.cancelar(id) {
        estado_bloqueado.difusion_cola.marcar(true);
    }
}

async fn limpiar_transferencias(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let mut estado_bloqueado = estado.lock().await;
    if estado_bloqueado.transferencias.limpiar() > 0 {
        estado_bloqueado.difusion_cola.marcar(true);
    }
}

// ---------------------------------------------------------------- CLI

/// Imprime la cola de transferencias: en curso y en cola, por host.
fn imprime_cola(transferencias: &[crate::protocolo::InfoTransferencia]) {
    let vivas: Vec<&crate::protocolo::InfoTransferencia> = transferencias
        .iter()
        .filter(|fila| !fila.estado.terminada())
        .collect();
    if vivas.is_empty() {
        println!("Sin transferencias en curso ni en cola.");
        return;
    }
    let en_curso = vivas
        .iter()
        .filter(|fila| fila.estado == crate::protocolo::EstadoTransferencia::EnCurso)
        .count();
    let en_cola = vivas.len() - en_curso;
    println!(
        "Cola de transferencias: {en_curso} en curso · {en_cola} en cola · {} terminada(s) conservada(s)",
        transferencias.len() - vivas.len()
    );
    println!(
        "{:<5} {:<10} {:<9} {:<22} {:<22} PROGRESO",
        "ID", "ESTADO", "DIRECCIÓN", "HOST", "ORIGEN → DESTINO"
    );
    for fila in vivas {
        println!(
            "{:<5} {:<10} {:<9} {:<22} {:<22} {} % · {} B de {} B",
            fila.id,
            fila.estado.texto(),
            fila.direccion.texto(),
            recortar_texto(&fila.host_nombre, 22),
            recortar_texto(&format!("{} → {}", fila.origen, fila.destino), 22),
            fila.porcentaje(),
            fila.bytes_hechos,
            fila.bytes_total,
        );
    }
}

/// Túneles activos con su escucha, destino, host, conexiones y tráfico.
fn imprime_tuneles(tuneles: &[crate::protocolo::InfoTunel]) {
    let activos: Vec<crate::protocolo::InfoTunel> = tuneles
        .iter()
        .filter(|tunel| tunel.estado != crate::protocolo::EstadoTunelRemoto::Inactivo)
        .cloned()
        .collect();
    if activos.is_empty() {
        println!("Sin túneles activos.");
        return;
    }
    println!("Túneles activos: {}", activos.len());
    println!(
        "{:<8} {:<10} {:<7} {:<22} {:<22} {:<8} {:<9} TRÁFICO",
        "ID", "ESTADO", "TIPO", "ESCUCHA", "DESTINO", "HOST", "CONEX."
    );
    for tunel in &activos {
        let destino = tunel
            .destino
            .clone()
            .unwrap_or_else(|| "(socks5)".to_string());
        println!(
            "{:<8} {:<10} {:<7} {:<22} {:<22} {:<8} {:<9} ↓ {} ↑ {}",
            tunel.tunel_id,
            tunel.estado.texto(),
            tunel.tipo,
            recortar_texto(tunel.escucha_mostrada(), 22),
            recortar_texto(&destino, 22),
            recortar_texto(&tunel.host_nombre, 8),
            format!("{} ({})", tunel.conexiones, tunel.aceptadas),
            crate::archivos::tamano_legible(tunel.bytes_bajados),
            crate::archivos::tamano_legible(tunel.bytes_subidos),
        );
    }
    if let Some(caido) = activos
        .iter()
        .find(|tunel| tunel.estado == crate::protocolo::EstadoTunelRemoto::Caido)
    {
        if let Some(error) = &caido.ultimo_error {
            println!("Último fallo: {} · {error}", caido.nombre);
        }
    }
}

/// Ejecuciones de snippets en curso: snippet, hosts y progreso.
fn imprime_ejecuciones(ejecuciones: &[crate::protocolo::InfoEjecucion]) {
    let en_curso: Vec<&crate::protocolo::InfoEjecucion> = ejecuciones
        .iter()
        .filter(|ejecucion| ejecucion.estado == crate::protocolo::EstadoEjecucion::EnCurso)
        .collect();
    if en_curso.is_empty() {
        println!("Sin ejecuciones de snippets en curso.");
        return;
    }
    println!("Ejecuciones en curso: {}", en_curso.len());
    println!(
        "{:<5} {:<28} {:<7} {:<8} {:<8} PROGRESO",
        "ID", "SNIPPET", "HOSTS", "OK", "FALLO"
    );
    for ejecucion in en_curso {
        let cuenta = |estados: &[crate::protocolo::EstadoHostEjecucion]| {
            ejecucion
                .hosts
                .iter()
                .filter(|host| estados.contains(&host.estado))
                .count()
        };
        use crate::protocolo::EstadoHostEjecucion as E;
        let terminados = ejecucion
            .hosts
            .iter()
            .filter(|host| host.estado.es_final())
            .count();
        println!(
            "{:<5} {:<28} {:<7} {:<8} {:<8} {terminados}/{} terminados",
            ejecucion.id,
            recortar_texto(&ejecucion.nombre, 28),
            ejecucion.hosts.len(),
            cuenta(&[E::Ok]),
            cuenta(&[E::Fallo, E::Error]),
            ejecucion.hosts.len(),
        );
    }
}

/// Recorta un texto a un ancho, con puntos suspensivos si sobra.
fn recortar_texto(texto: &str, ancho: usize) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    let recortado: String = texto.chars().take(ancho.saturating_sub(1)).collect();
    format!("{recortado}…")
}

/// `magi servidor estado`: pid, versión de protocolo, sesiones, clientes y la
/// cola de transferencias (en curso y en cola por host).
pub async fn estado_cli(rutas: &Rutas) -> Result<i32> {
    let ruta = ruta_socket(rutas);
    let mut stream = match UnixStream::connect(&ruta).await {
        Ok(stream) => stream,
        Err(_) => {
            println!(
                "No hay servidor de sesiones en marcha ({}).",
                ruta.display()
            );
            return Ok(1);
        }
    };
    let pid_par = pid_del_par(&stream);
    saludo_y_envio(&mut stream).await?;
    match leer_respuesta(&mut stream).await? {
        MensajeServidor::Bienvenida {
            version,
            pid,
            clientes,
            sesiones,
            transferencias,
            tuneles,
            ejecuciones,
            ..
        } => {
            println!(
                "Servidor pid {pid} · protocolo {version} · {clientes} cliente(s) conectado(s)"
            );
            if sesiones.is_empty() {
                println!("Sin sesiones abiertas.");
            } else {
                println!(
                    "{:<6} {:<10} {:<24} {:<12} {:<8} IDENTIDAD",
                    "ID", "ESTADO", "PESTAÑA", "DESDE", "VENT."
                );
                for sesion in sesiones {
                    let desde = chrono::DateTime::from_timestamp(sesion.abierta_en, 0)
                        .map(|fecha| fecha.format("%H:%M").to_string())
                        .unwrap_or_default();
                    println!(
                        "{:<6} {:<10} {:<24} {:<12} {:<8} {}",
                        sesion.id,
                        formato_estado(&sesion.estado),
                        sesion.nombre,
                        desde,
                        sesion.ventanas,
                        sesion.identidad,
                    );
                }
            }
            imprime_cola(&transferencias);
            imprime_tuneles(&tuneles);
            imprime_ejecuciones(&ejecuciones);
            Ok(0)
        }
        MensajeServidor::VersionIncompatible { version, pid } => {
            let pid = pid.or(pid_par);
            println!(
                "El servidor{} habla la versión de protocolo {version} y este MAGI la {VERSION_PROTOCOLO}: ciérralo con «magi servidor parar» y vuelve a abrir.",
                pid.map(|pid| format!(" (pid {pid})")).unwrap_or_default()
            );
            Ok(1)
        }
        MensajeServidor::Error { mensaje, .. } => {
            println!("Error del servidor: {mensaje}");
            Ok(1)
        }
        _ => {
            println!("Respuesta inesperada del servidor.");
            Ok(1)
        }
    }
}

/// `magi servidor parar`: cierra las sesiones y apaga el servidor, con
/// confirmación si hay sesiones vivas (salvo `--si`). Ante un servidor de
/// otra versión, muestra su pid y le envía `SIGTERM` tras confirmar.
pub async fn parar_cli(rutas: &Rutas, si: bool) -> Result<i32> {
    let ruta = ruta_socket(rutas);
    let mut stream = match UnixStream::connect(&ruta).await {
        Ok(stream) => stream,
        Err(_) => {
            println!(
                "No hay servidor de sesiones en marcha ({}).",
                ruta.display()
            );
            return Ok(1);
        }
    };
    // El pid del otro extremo del socket, por si no habla nuestro protocolo.
    let pid_par = pid_del_par(&stream);
    saludo_y_envio(&mut stream).await?;
    let (cuantas, tuneles_vivos, ejecuciones_vivas) = match leer_respuesta(&mut stream).await? {
        MensajeServidor::Bienvenida {
            sesiones,
            tuneles,
            ejecuciones,
            ..
        } => (
            sesiones.len(),
            tuneles
                .iter()
                .filter(|tunel| tunel.estado != crate::protocolo::EstadoTunelRemoto::Inactivo)
                .count(),
            ejecuciones
                .iter()
                .filter(|ejecucion| ejecucion.estado == crate::protocolo::EstadoEjecucion::EnCurso)
                .count(),
        ),
        MensajeServidor::VersionIncompatible { version, pid } => {
            drop(stream);
            // El pid del kernel antes que el que dice el propio servidor.
            return parar_otra_version(rutas, version, pid_par.or(pid), si).await;
        }
        _ => {
            println!("Respuesta inesperada del servidor.");
            return Ok(1);
        }
    };
    if (cuantas > 0 || tuneles_vivos > 0 || ejecuciones_vivas > 0) && !si {
        let mut aviso = format!("Hay {cuantas} sesión(es) abierta(s)");
        if tuneles_vivos > 0 {
            aviso.push_str(&format!(", {tuneles_vivos} túnel(es) levantado(s)"));
        }
        if ejecuciones_vivas > 0 {
            aviso.push_str(&format!(
                " y {ejecuciones_vivas} ejecución(es) de snippets en curso (se cancelarán)"
            ));
        }
        print!("{aviso}. ¿Cerrarlas y apagar el servidor? [s/N] ");
        use std::io::Write as _;
        std::io::stdout().flush().ok();
        let mut respuesta = String::new();
        if std::io::stdin().read_line(&mut respuesta).is_err()
            || !respuesta.trim().eq_ignore_ascii_case("s")
        {
            println!("No se ha parado el servidor.");
            return Ok(0);
        }
    }
    let linea = codificar(&MensajeCliente::Parar).context("serializando Parar")?;
    stream.write_all(linea.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;
    // El servidor cierra el socket al terminar: se lee hasta el fin.
    let mut resto = Vec::new();
    let _ = stream.read_to_end(&mut resto).await;
    println!("Servidor detenido.");
    Ok(0)
}

/// Un servidor de otra versión no entiende `Parar`: se le manda `SIGTERM`
/// (con confirmación salvo `--si`) y se espera a que suelte el lock.
async fn parar_otra_version(
    rutas: &Rutas,
    version: u32,
    pid: Option<u32>,
    si: bool,
) -> Result<i32> {
    let Some(pid) = pid else {
        println!(
            "El servidor en marcha habla la versión de protocolo {version} y este MAGI la {VERSION_PROTOCOLO}, y no se pudo averiguar su pid. Búscalo con «pgrep -f 'magi --servidor'» y páralo con «kill <pid>»."
        );
        return Ok(1);
    };
    println!(
        "El servidor en marcha (pid {pid}) habla la versión de protocolo {version} y este MAGI la {VERSION_PROTOCOLO}."
    );
    if !si {
        print!(
            "¿Enviarle SIGTERM? Sus sesiones se cerrarán{}. [s/N] ",
            if version < VERSION_PROTOCOLO {
                " (un servidor de una versión anterior no las anota al cerrarlas)"
            } else {
                ""
            }
        );
        use std::io::Write as _;
        std::io::stdout().flush().ok();
        let mut respuesta = String::new();
        if std::io::stdin().read_line(&mut respuesta).is_err()
            || !respuesta.trim().eq_ignore_ascii_case("s")
        {
            println!("No se ha parado el servidor.");
            return Ok(0);
        }
    }
    // La pregunta no tiene plazo: el servidor pudo pararse solo mientras
    // tanto (y su pid, reutilizarse). Si el lock ya está libre, no se manda
    // nada a nadie.
    if crate::cliente::lock_libre(&ruta_lock(rutas)) {
        let _ = std::fs::remove_file(ruta_socket(rutas));
        println!("El servidor ya se había detenido (pid {pid}).");
        return Ok(0);
    }
    if parar_por_senal(rutas, pid).await? {
        println!("Servidor detenido (pid {pid}).");
        Ok(0)
    } else {
        println!("El servidor no se detuvo en 10 s; prueba «kill -9 {pid}».");
        Ok(1)
    }
}

/// Pid del proceso al otro lado del socket (`SO_PEERCRED` en Linux,
/// `LOCAL_PEERPID` en macOS), solo si es del mismo usuario y no somos
/// nosotros: es el servidor al que se le podría mandar una señal.
pub fn pid_del_par(stream: &UnixStream) -> Option<u32> {
    let credencial = stream.peer_cred().ok()?;
    let pid = credencial.pid()?;
    if pid <= 1 || pid as u32 == std::process::id() {
        return None;
    }
    if credencial.uid() != nix::unistd::getuid().as_raw() {
        return None;
    }
    Some(pid as u32)
}

/// Manda `SIGTERM` al servidor y espera (hasta 10 s) a que el proceso
/// termine y el lock quede libre. Si murió sin limpiar (una versión anterior
/// no atiende la señal), se borra su socket huérfano. Devuelve si se detuvo.
pub async fn parar_por_senal(rutas: &Rutas, pid: u32) -> Result<bool> {
    let pid_nix = nix::unistd::Pid::from_raw(pid as i32);
    match nix::sys::signal::kill(pid_nix, nix::sys::signal::Signal::SIGTERM) {
        Ok(()) => {}
        // Ya no existe: se paró solo entretanto.
        Err(nix::errno::Errno::ESRCH) => {}
        Err(error) => {
            return Err(anyhow::anyhow!("enviando SIGTERM al pid {pid}: {error}"));
        }
    }
    let inicio = std::time::Instant::now();
    while inicio.elapsed() < Duration::from_secs(10) {
        let vivo = nix::sys::signal::kill(pid_nix, None).is_ok();
        if !vivo && crate::cliente::lock_libre(&ruta_lock(rutas)) {
            let _ = std::fs::remove_file(ruta_socket(rutas));
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(false)
}

async fn saludo_y_envio(stream: &mut UnixStream) -> Result<()> {
    let saludo = MensajeCliente::Hola {
        version: VERSION_PROTOCOLO,
        pid: std::process::id(),
    };
    let linea = codificar(&saludo).context("serializando el saludo")?;
    stream.write_all(linea.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;
    Ok(())
}

/// Lee una línea de respuesta del socket y la decodifica (con timeout).
async fn leer_respuesta(stream: &mut UnixStream) -> Result<MensajeServidor> {
    let futura = async {
        let mut lector = FramedRead::new(
            stream,
            LinesCodec::new_with_max_length(crate::protocolo::LINEA_MAXIMA),
        );
        match lector.next().await {
            Some(Ok(linea)) => Ok(linea),
            Some(Err(error)) => Err(anyhow::anyhow!("respuesta no válida del servidor: {error}")),
            None => Err(anyhow::anyhow!("el servidor cerró la conexión")),
        }
    };
    let linea = tokio::time::timeout(Duration::from_secs(5), futura)
        .await
        .map_err(|_| anyhow::anyhow!("el servidor no respondió en 5 s"))??;
    decodificar::<MensajeServidor>(&linea).map_err(|motivo| anyhow::anyhow!("{motivo}"))
}

fn formato_estado(estado: &crate::protocolo::EstadoSesionRemota) -> &'static str {
    use crate::protocolo::EstadoSesionRemota::*;
    match estado {
        Abriendo => "abriendo",
        Abierta => "abierta",
        Caida => "caída",
        Cerrada => "cerrada",
    }
}

use std::os::unix::fs::PermissionsExt as _;
