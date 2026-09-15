//! Servidor de sesiones (`magi --servidor`): custodia sesiones SSH y el pool
//! de conexiones, hablante por un socket Unix con el protocolo JSON por
//! líneas. No dialoga: reenvía huellas, frases y contraseñas al cliente
//! solicitante. Escribe en SQLite solo `REGISTRO` y `HOSTS.ultimo_estado` /
//! `ultima_conexion_en`.

pub mod cliente_remoto;
pub mod conexiones;
pub mod difusion;
pub mod sesiones;

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
use tracing::{error, info, warn};

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
/// de SQLite dentro del proceso servidor.
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
}

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
}

/// Ruta del socket del servidor.
pub fn ruta_socket(rutas: &Rutas) -> PathBuf {
    rutas.dir_runtime().join("servidor.sock")
}

/// Ruta del fichero de bloqueo del servidor.
pub fn ruta_lock(rutas: &Rutas) -> PathBuf {
    rutas.dir_runtime().join("servidor.lock")
}

/// Un pánico del servidor se anota en el log y el proceso sale con código 2:
/// el servidor no restaura terminales porque no las tiene.
fn instalar_hook_panico() {
    std::panic::set_hook(Box::new(|informacion| {
        error!("pánico del servidor: {informacion}");
        std::process::exit(2);
    }));
}

// ---------------------------------------------------------------- arranque

/// Punto de entrada de `magi --servidor`: prepara directorio, lock y socket,
/// sirve a los clientes y se apaga cuando no quedan sesiones ni clientes
/// durante la gracia.
pub async fn arrancar(rutas: Rutas, config: Config) -> Result<()> {
    instalar_hook_panico();
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
    }));

    let (tx_apagar, mut rx_apagar) = mpsc::unbounded_channel::<()>();
    estado.lock().await.tx_apagar = Some(tx_apagar);

    // Registro de arranque con la BD ya abierta.
    if let Err(error) = anotar(
        &estado,
        crate::registro::SERVIDOR_ARRANCADO,
        "servidor de sesiones en marcha",
    ) {
        warn!("no se pudo anotar el arranque: {error}");
    }
    info!(socket = %ruta_socket.display(), "servidor arrancado");

    // Revisoras de inactividad (servidor) y de vencimiento (pool).
    tokio::spawn(revisar_inactividad(estado.clone()));
    tokio::spawn(revisar_pool(estado.clone()));

    loop {
        tokio::select! {
            _ = rx_apagar.recv() => break,
            aceptado = listener.accept() => match aceptado {
                Ok((stream, _)) => {
                    let estado_tarea = estado.clone();
                    tokio::spawn(tarea_conexion(stream, estado_tarea));
                }
                Err(error) => warn!("error aceptando un cliente: {error}"),
            },
        }
    }

    // Salida: socket y lock fuera, aviso anotado.
    let _ = std::fs::remove_file(&ruta_socket);
    let _ = std::fs::remove_file(ruta_lock(&rutas));
    let detalle = "servidor detenido";
    let _ = anotar(&estado, crate::registro::SERVIDOR_DETENIDO, detalle);
    info!("{detalle}");
    Ok(())
}

/// Anota un evento en `REGISTRO` a través del escritor del proceso.
fn anotar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    tipo: &str,
    detalle: &str,
) -> Result<(), String> {
    // Se usa el canal; el hilo de la BD valida el tipo igual que `registro::anotar`.
    estado
        .try_lock()
        .map_err(|_| "servidor ocupado".to_string())?
        .bd
        .send(OrdenBd::Anotar {
            tipo: tipo.to_string(),
            host_id: None,
            identidad_id: None,
            detalle: detalle.to_string(),
            resultado: ResultadoRegistro::Ok,
        })
        .map_err(|_| "el escritor de la base de datos no está".to_string())
}

/// Hilo con la única `Connection` de escritura del proceso servidor.
fn lanzar_escritor_bd(almacen: Almacen) -> std::sync::mpsc::Sender<OrdenBd> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(orden) = rx.recv() {
            if let Err(error) = ejecutar_orden_bd(almacen.conexion(), orden) {
                warn!("no se pudo escribir en la base de datos: {error}");
            }
        }
        if let Err(error) = almacen.cerrar() {
            warn!("no se pudo cerrar la base de datos del servidor: {error}");
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
        } => crate::registro::anotar(conexion, &tipo, host_id, identidad_id, &detalle, resultado),
        OrdenBd::MarcarConexion { host_id } => {
            crate::almacen::hosts::marcar_conexion(conexion, host_id)
        }
        OrdenBd::MarcarEstado { host_id, estado } => {
            crate::almacen::hosts::marcar_estado(conexion, host_id, estado)
        }
    }
}

/// Apaga el servidor si no hay clientes ni sesiones durante la gracia.
async fn revisar_inactividad(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let gracia = Duration::from_secs(estado.lock().await.gracia);
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let apagar = {
            let mut estado_bloqueado = estado.lock().await;
            if estado_bloqueado.clientes.is_empty() && estado_bloqueado.sesiones.is_empty() {
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

/// Apagado inmediato con limpieza: cierra sesiones y clientes y señaliza.
async fn apagar_limpio(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let mut estado_bloqueado = estado.lock().await;
    for (_, sesion) in estado_bloqueado.sesiones.drain() {
        let _ = sesion.tx_comandos.send(ComandoSesion::Cerrar);
    }
    estado_bloqueado.pendientes.clear();
    estado_bloqueado.clientes.clear();
    let conexiones = estado_bloqueado.pool.vaciar();
    if let Some(tx_apagar) = estado_bloqueado.tx_apagar.take() {
        let _ = tx_apagar.send(());
    }
    drop(estado_bloqueado);
    for (handle, saltos) in conexiones {
        conexiones::desconectar(handle, saltos).await;
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
            let _ = enviar_linea(
                &tx_escritura,
                MensajeServidor::VersionIncompatible { version },
            );
            return;
        }
        _ => {
            let _ = enviar_linea(
                &tx_escritura,
                MensajeServidor::Error {
                    mensaje: "se esperaba el saludo Hola".to_string(),
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
            Ok(mensaje) => seguir = manejar_mensaje(&estado, cliente_id, mensaje).await,
            Err(motivo) => {
                let _ = enviar_linea(
                    &tx_escritura,
                    MensajeServidor::Error {
                        mensaje: format!("mensaje no válido: {motivo}"),
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

/// `Bienvenida` con la versión, el pid del servidor y las sesiones vivas.
async fn bienvenida(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, cliente_id: u32) {
    let estado_bloqueado = estado.lock().await;
    let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) else {
        return;
    };
    let mensaje = MensajeServidor::Bienvenida {
        version: VERSION_PROTOCOLO,
        pid: std::process::id(),
        clientes: estado_bloqueado.clientes.len() as u32,
        sesiones: sesiones::lista(&estado_bloqueado.sesiones),
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
        } => {
            abrir_sesion(estado, cliente_id, host_id, cols, filas).await;
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
            cerrar_sesion(estado, sesion_id).await;
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
        MensajeCliente::Ejecutar { host_id, comando } => {
            ejecutar(estado, cliente_id, host_id, &comando).await;
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
            },
        );
    }
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
    for sesion_id in caidas {
        estado_bloqueado.pendientes.remove(&sesion_id);
        if let Some(sesion) = estado_bloqueado.sesiones.remove(&sesion_id) {
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
    estado_bloqueado.clientes.remove(&cliente_id);
    difundir_lista(&mut estado_bloqueado);
}

// ---------------------------------------------------------------- sesiones

/// Crea la sesión en estado «abriendo» y lanza su tarea de conexión.
async fn abrir_sesion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    cols: u16,
    filas: u16,
) {
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
                },
            );
        }
        return;
    };

    let mut estado_bloqueado = estado.lock().await;
    let sesion_id = estado_bloqueado.siguiente_sesion_id;
    estado_bloqueado.siguiente_sesion_id += 1;
    let nombre = sesiones::nombre_de_pestaña(&estado_bloqueado.sesiones, &host);
    let (tx_comandos, rx_comandos) = mpsc::unbounded_channel();
    let tamano = Tamano { cols, filas };
    let pantalla = crate::conexion::terminal::nuevo(filas, cols);
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
    };
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
        difusion::difundir_a_adjuntos(
            &estado_bloqueado.clientes,
            &ids,
            MensajeServidor::Redimensionada {
                sesion_id,
                cols: nuevo.cols,
                filas: nuevo.filas,
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
    let mut estado_bloqueado = estado.lock().await;
    if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
        if sesion.adjuntos.contains_key(&cliente_id) {
            sesion.adjuntos.insert(cliente_id, tamano);
        }
    }
    aplicar_tamano(&mut estado_bloqueado, sesion_id).await;
}

/// Cierra una sesión: si abría, cancela; si está caída, se elimina; si está
/// abierta, el bucle hace el trabajo al recibir `Cerrar`.
async fn cerrar_sesion(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, sesion_id: u32) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get(&sesion_id) else {
        return;
    };
    match sesion.estado {
        crate::protocolo::EstadoSesionRemota::Abriendo
        | crate::protocolo::EstadoSesionRemota::Caida => {
            let fallida_apertura = sesion.estado == crate::protocolo::EstadoSesionRemota::Abriendo;
            let host_id = sesion.host_id;
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

    let (cols, filas) = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            return;
        };
        sesion.estado = crate::protocolo::EstadoSesionRemota::Abriendo;
        sesion.motivo = Some("reconectando".to_string());
        sesion.handle = None;
        (sesion.tamano.cols, sesion.tamano.filas)
    };
    let (tx_comandos, rx_comandos) = mpsc::unbounded_channel();
    {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
            sesion.tx_comandos = tx_comandos;
        }
    }
    let pantalla = crate::conexion::terminal::nuevo(filas, cols);
    {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
            sesion.pantalla = pantalla.clone();
        }
    }
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

/// Ejecuta un comando en un canal `exec` de la conexión del pool o de
/// cualquier sesión abierta al host. Devuelve `Ejecutado` o `SinSesion`.
async fn ejecutar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    host_id: i64,
    comando: &str,
) {
    // Buscar conexión: primero el pool, luego cualquier sesión abierta al host.
    let handle = {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(handle) = estado_bloqueado.pool.reutilizar(host_id) {
            Some((handle, true))
        } else {
            estado_bloqueado
                .sesiones
                .values()
                .find(|sesion| sesion.host_id == host_id && sesion.handle.is_some())
                .and_then(|sesion| sesion.handle.clone())
                .map(|handle| (handle, false))
        }
    };
    let Some((handle, del_pool)) = handle else {
        let estado_bloqueado = estado.lock().await;
        if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
            difusion::enviar(cliente, MensajeServidor::SinSesion { host_id });
        }
        return;
    };
    let resultado = ejecutar_en_handle(&handle, comando).await;
    if del_pool {
        estado.lock().await.pool.liberar(host_id);
    }
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        match resultado {
            Ok((salida, codigo)) => difusion::enviar(
                cliente,
                MensajeServidor::Ejecutado {
                    host_id,
                    salida,
                    codigo,
                },
            ),
            // Una conexión del pool que se está cerrando hace caer el exec:
            // el cliente repite con su conexión efímera.
            Err(_) => difusion::enviar(cliente, MensajeServidor::SinSesion { host_id }),
        }
    }
}

/// Canal `exec` sobre una conexión viva; devuelve salida y código de salida.
async fn ejecutar_en_handle(
    handle: &russh::client::Handle<crate::conexion::cliente::Cliente>,
    comando: &str,
) -> Result<(String, i32), String> {
    let mut canal = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    canal
        .exec(true, comando.as_bytes().to_vec())
        .await
        .map_err(|error| error.to_string())?;
    let mut salida = Vec::new();
    let mut codigo = 0;
    loop {
        match canal.wait().await {
            Some(ChannelMsg::Data { data }) => salida.extend_from_slice(&data),
            Some(ChannelMsg::ExtendedData { data, .. }) => salida.extend_from_slice(&data),
            Some(ChannelMsg::ExitStatus { exit_status }) => codigo = exit_status as i32,
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    let _ = canal.close().await;
    Ok((String::from_utf8_lossy(&salida).to_string(), codigo))
}

// ---------------------------------------------------------------- CLI

/// `magi servidor estado`: pid, versión de protocolo, sesiones y clientes.
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
    saludo_y_envio(&mut stream).await?;
    match leer_respuesta(&mut stream).await? {
        MensajeServidor::Bienvenida {
            version,
            pid,
            clientes,
            sesiones,
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
            Ok(0)
        }
        MensajeServidor::VersionIncompatible { version } => {
            println!(
                "El servidor habla la versión de protocolo {version} y este MAGI la {VERSION_PROTOCOLO}: ciérralo con «magi servidor parar» y vuelve a abrir."
            );
            Ok(1)
        }
        MensajeServidor::Error { mensaje } => {
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
/// confirmación si hay sesiones vivas (salvo `--si`).
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
    saludo_y_envio(&mut stream).await?;
    let cuantas = match leer_respuesta(&mut stream).await? {
        MensajeServidor::Bienvenida { sesiones, .. } => sesiones.len(),
        MensajeServidor::VersionIncompatible { version } => {
            println!(
                "El servidor habla la versión de protocolo {version} y este MAGI la {VERSION_PROTOCOLO}; no se puede parar con este comando."
            );
            return Ok(1);
        }
        _ => {
            println!("Respuesta inesperada del servidor.");
            return Ok(1);
        }
    };
    if cuantas > 0 && !si {
        print!("Hay {cuantas} sesión(es) abierta(s). ¿Cerrarlas y apagar el servidor? [s/N] ");
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
