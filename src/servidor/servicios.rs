//! Servidor de servicios, contenedores y logs (Fase 13, §3.3, §3.4,
//! §4.2-§4.5, §5.2, §7.3-§7.6 y §8).
//!
//! Todo lo remoto va por el pool de conexiones con el script embebido
//! `servicios.sh` (`orden::comando`), por canales `exec` abiertos con
//! `UsoCanal::Servicios`; la `ConexionTomada` se suelta por identidad en
//! todas las ramas. Aquí viven las acciones en curso (trabajo custodiado que
//! sobrevive a la ventana que las pidió) y las suscripciones de log (que no
//! son trabajo: se cortan con `PararLog`, con el cliente o con el servidor).
//!
//! Plazos (T32): 10 s para listado, consumo, detalle y comprobación de sudo;
//! 20 s para la comprobación de configuración (dentro de la acción) y
//! `espera_seg + 30 s` para toda una acción. Los logs no tienen plazo: duran
//! lo que la suscripción.
//!
//! Nada de lo que llega del host va al log ni a REGISTRO (T44): solo los
//! metadatos de la acción, y nunca su salida.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::{client::Handle, ChannelMsg};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

use magi_nucleo::servicios as nucleo;

use crate::conexion::cliente::Cliente;
use crate::escritorio::TipoHost;
use crate::modelo::{Host, ResultadoRegistro};
use crate::protocolo::{
    Accion, ComprobacionSudo, InfoAccion, LineaLog, MensajeCliente, MensajeServidor, MotivoFinLog,
    Objetivo, Privilegio, ResultadoServicio, Secreto, TipoObjetivo,
};

use super::conexiones::{ConexionTomada, PoliticaConexion, UsoCanal};
use super::{ejecuciones, EstadoServidor};

/// Plazo para abrir un canal `exec` sobre una conexión ya tomada.
const PLAZO_CANAL: Duration = Duration::from_secs(5);

/// Tope de la salida de un listado (§8: 2 MiB).
const TOPE_LISTADO: usize = nucleo::limites::LISTADO_MAX_BYTES;

/// Tope de la salida de un consumo (un JSON pequeño por contenedor).
const TOPE_CONSUMO: usize = 512 * 1024;

/// Tope de la salida de un detalle (texto de la unidad o de `inspect`).
const TOPE_DETALLE: usize = nucleo::limites::TEXTO_MAX_BYTES + 256 * 1024;

/// Tope de la salida de una comprobación de sudo.
const TOPE_SUDO: usize = 64 * 1024;

/// Tope de la salida de una acción: el resultado y las últimas líneas.
const TOPE_ACCION: usize = 1024 * 1024;

/// Tope de lo que se guarda de la salida de errores de un canal.
const TOPE_ERRORES: usize = 64 * 1024;

/// Tope de una línea del diario o de un contenedor (§8: 16 KiB).
const TOPE_LINEA: usize = nucleo::limites::LINEA_LOG_MAX_BYTES;

/// Líneas por lote de logs (500).
const LOTE_LOGS: usize = nucleo::limites::LOTE_LOG_MAX;

/// Tamaño máximo de la cola de una suscripción antes de descartar las más
/// viejas (4 MiB).
const COLA_LOGS: usize = nucleo::limites::COLA_LOG_MAX_BYTES;

/// Cada cuánto se manda un lote de líneas de log (100 ms).
const RITMO_LOGS: Duration = Duration::from_millis(100);

/// Tope de líneas iniciales que se aceptan al abrir un seguimiento.
const LINEAS_INICIALES_MAX: u32 = 5000;

/// Tope del lote de `LineasLog` ya codificado en JSON: deja margen bajo
/// `LINEA_MAXIMA` aunque las líneas vengan llenas de comillas o controles que
/// el JSON escapa (M4 de la revisión).
const TOPE_LOTE_CODIFICADO: usize = 2 * 1024 * 1024;

/// Una acción de servicio en curso (§3.3). Es trabajo del servidor: sigue
/// aunque se cierre la ventana que la pidió y se difunde en
/// `AccionesServicio{lista}` (T27).
pub struct AccionEnCurso {
    /// Lo que se difunde: id (`peticion_id`), host, objetivo, acción, fase y
    /// desde.
    pub info: InfoAccion,
    /// El cliente que la pidió: recibe `AccionServicioHecha`.
    pub solicitante: u32,
    /// Espera máxima al estado final (`[servicios] espera_seg`).
    pub espera_seg: u64,
    /// Instante local en el que empezó, para la duración de REGISTRO.
    pub desde: Instant,
    /// La deliberación que la autoriza, si la hubo (T62).
    pub deliberacion: Option<ejecuciones::DeliberacionFijada>,
    /// Para cancelar la tarea al apagar el servidor: el efecto remoto sigue en
    /// el host, pero no se anota (§8).
    pub cancelar: tokio_util::sync::CancellationToken,
}

/// Una suscripción de log (§3.4): un canal `exec` con el seguimiento abierto
/// y el cliente al que se mandan sus `LineasLog`. No es trabajo del servidor.
pub struct Suscripcion {
    pub peticion_id: u64,
    pub solicitante: u32,
    pub host_id: i64,
    pub objetivo: Objetivo,
    /// Cerrarla cierra el canal (fin de la entrada estándar del script).
    pub cancelar: tokio_util::sync::CancellationToken,
    /// La conexión del pool que sostiene el canal: vive mientras dure la
    /// suscripción y se suelta al quitar la entrada del mapa, en todas las
    /// ramas (fin normal, `PararLog`, cliente caído o apagado). Mientras
    /// esté, el canal cuenta para el pool y su gracia no la cierra.
    tomada: Option<ConexionTomada>,
}

/// Clave de una acción o suscripción de servicios: el cliente y su
/// `peticion_id`. Cada ventana numera sus peticiones desde el mismo rango, así
/// que dos ventanas pueden repetir el id: el cliente forma parte de la clave
/// (como en los escritorios de la Fase 12).
type ClaveServicio = (u32, u64);

/// Estado en memoria del servidor para la vista Servicios.
#[derive(Default)]
pub struct Servicios {
    /// Acciones en curso, por (cliente, `peticion_id`).
    acciones: HashMap<ClaveServicio, AccionEnCurso>,
    /// Suscripciones de log, por (cliente, `peticion_id`).
    logs: HashMap<ClaveServicio, Suscripcion>,
    /// Deliberaciones ya consumidas por una acción terminada cuya anotación
    /// aún no ha pasado la `Barrera`: mientras estén aquí siguen contando
    /// como usadas (B7 de la revisión).
    deliberaciones_en_vuelo: std::collections::HashSet<i64>,
    /// Último `MAGI_PRIV` visto por host (pista para el script, §7.3).
    privilegios: HashMap<i64, Privilegio>,
    /// Último motor de contenedores visto por host (`PedirConsumo` no lo trae
    /// y el listado sí).
    motores: HashMap<i64, nucleo::MotorNombre>,
}

impl Servicios {
    /// La lista completa que se difunde con `AccionesServicio{lista}`.
    pub fn acciones(&self) -> Vec<InfoAccion> {
        let mut lista: Vec<InfoAccion> = self
            .acciones
            .values()
            .map(|accion| accion.info.clone())
            .collect();
        lista.sort_by_key(|accion| accion.id);
        lista
    }

    /// ¿Hay alguna acción en curso? Cuenta como trabajo del servidor (no se
    /// apaga por inactividad con una acción a medias).
    pub fn hay_acciones(&self) -> bool {
        !self.acciones.is_empty()
    }

    /// Cuántas acciones hay en curso (para `TrabajoServidor`).
    pub fn cuantas_acciones(&self) -> u32 {
        self.acciones.len() as u32
    }

    /// El privilegio con el que se pasa la pista al script en ese host. Por
    /// defecto `sin_sudo`: se averigua con la suborden `privilegio` la
    /// primera vez que hace falta.
    pub fn privilegio(&self, host_id: i64) -> Privilegio {
        self.privilegio_opt(host_id).unwrap_or(Privilegio::SinSudo)
    }

    /// El privilegio ya averiguado del host, si se conoce.
    pub fn privilegio_opt(&self, host_id: i64) -> Option<Privilegio> {
        self.privilegios.get(&host_id).copied()
    }

    /// El motor de contenedores del último listado de ese host.
    pub fn motor(&self, host_id: i64) -> Option<nucleo::MotorNombre> {
        self.motores.get(&host_id).copied()
    }

    /// ¿Autorizó ya esta deliberación una acción de servicio en curso o una
    /// terminada cuya anotación aún no ha pasado la Barrera? (T62: una
    /// deliberación autoriza una sola cosa.)
    pub fn deliberacion_usada(&self, deliberacion_id: i64) -> bool {
        self.deliberaciones_en_vuelo.contains(&deliberacion_id)
            || self.acciones.values().any(|accion| {
                accion
                    .deliberacion
                    .as_ref()
                    .is_some_and(|fijada| fijada.registro.id == deliberacion_id)
            })
    }
}

// ---------------------------------------------------------------- despacho

/// Despacha un mensaje de la vista Servicios. Se llama en una tarea propia por
/// mensaje (ver `manejar_mensaje`), así que puede esperar al host.
pub async fn manejar(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    mensaje: MensajeCliente,
) -> bool {
    match mensaje {
        MensajeCliente::ListarServicios {
            peticion_id,
            host_id,
            sudo,
        } => {
            listar(estado, cliente_id, peticion_id, host_id, sudo).await;
        }
        MensajeCliente::PedirConsumo {
            peticion_id,
            host_id,
            sudo,
        } => {
            consumo(estado, cliente_id, peticion_id, host_id, sudo).await;
        }
        MensajeCliente::DetalleServicio {
            peticion_id,
            host_id,
            objetivo,
            sudo,
        } => {
            detalle(estado, cliente_id, peticion_id, host_id, objetivo, sudo).await;
        }
        MensajeCliente::ComprobarSudo {
            peticion_id,
            host_id,
            contrasena,
        } => {
            comprobar_sudo(estado, cliente_id, peticion_id, host_id, contrasena).await;
        }
        MensajeCliente::AccionServicio {
            peticion_id,
            host_id,
            objetivo,
            accion,
            deliberacion,
            sudo,
        } => {
            accion_servicio(
                estado,
                cliente_id,
                peticion_id,
                host_id,
                objetivo,
                accion,
                deliberacion,
                sudo,
            )
            .await;
        }
        MensajeCliente::SeguirLog {
            peticion_id,
            host_id,
            objetivo,
            lineas_iniciales,
            sudo,
        } => {
            seguir_log(
                estado,
                cliente_id,
                peticion_id,
                host_id,
                objetivo,
                lineas_iniciales,
                sudo,
            )
            .await;
        }
        MensajeCliente::PararLog { peticion_id } => {
            parar_log(estado, cliente_id, peticion_id).await;
        }
        otro => {
            debug!("mensaje inesperado en el servidor de servicios: {otro:?}");
        }
    }
    true
}

/// Aviso al servidor: se apaga. Corta las suscripciones de log y cancela las
/// acciones en curso (sus efectos siguen en el host, pero no se anotan: §8).
pub async fn apagar(estado: &Arc<Mutex<EstadoServidor>>) {
    let mut retiradas = Vec::new();
    {
        let mut estado_bloqueado = estado.lock().await;
        let logs = estado_bloqueado.servicios.logs.len();
        let acciones = estado_bloqueado.servicios.acciones.len();
        if logs > 0 || acciones > 0 {
            warn!(
                suscripciones = logs,
                acciones, "se cortan las acciones y los logs de servicios al apagar"
            );
        }
        for (_, suscripcion) in estado_bloqueado.servicios.logs.drain() {
            suscripcion.cancelar.cancel();
            retiradas.push(suscripcion);
        }
        for (_, accion) in estado_bloqueado.servicios.acciones.drain() {
            accion.cancelar.cancel();
        }
    }
    // Las conexiones de las suscripciones se sueltan fuera del bloqueo.
    for suscripcion in retiradas {
        soltar_suscripcion(suscripcion).await;
    }
}

/// Suelta la conexión de una suscripción retirada del mapa (por identidad).
async fn soltar_suscripcion(suscripcion: Suscripcion) {
    if let Some(tomada) = suscripcion.tomada {
        tomada.soltar().await;
    }
}

/// Un cliente se ha desconectado: sus suscripciones de log se cortan (las
/// acciones siguen: son trabajo del servidor).
pub async fn cliente_desconectado(estado: &Arc<Mutex<EstadoServidor>>, cliente_id: u32) {
    let mut retiradas = Vec::new();
    {
        let mut estado_bloqueado = estado.lock().await;
        let claves: Vec<ClaveServicio> = estado_bloqueado
            .servicios
            .logs
            .keys()
            .filter(|(cliente, _)| *cliente == cliente_id)
            .copied()
            .collect();
        for clave in claves {
            if let Some(suscripcion) = estado_bloqueado.servicios.logs.remove(&clave) {
                debug!(
                    peticion_id = clave.1,
                    host_id = suscripcion.host_id,
                    "suscripción de log cortada al caer el cliente"
                );
                suscripcion.cancelar.cancel();
                retiradas.push(suscripcion);
            }
        }
    }
    for suscripcion in retiradas {
        soltar_suscripcion(suscripcion).await;
    }
}

/// ¿Autorizó ya esta deliberación una acción de servicio en curso? La usa
/// `ejecuciones::deliberacion_usada` para que «ya usada» valga en los dos
/// sentidos (T62).
pub fn deliberacion_usada(estado: &EstadoServidor, deliberacion_id: i64) -> bool {
    estado.servicios.deliberacion_usada(deliberacion_id)
}

/// ¿La petición pertenece al rango de la vista Servicios?
pub fn peticion_del_rango(peticion_id: u64) -> bool {
    (crate::protocolo::RANGO_SERVICIOS..crate::protocolo::RANGO_ESCRITORIOS).contains(&peticion_id)
}

// ---------------------------------------------------------------- comunes

/// Responde un `Error` a un cliente concreto, con el `peticion_id` de la
/// petición que lo provocó.
async fn responder_error(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    mensaje: impl Into<String>,
    peticion_id: Option<u64>,
) {
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        super::difusion::enviar(
            cliente,
            MensajeServidor::Error {
                mensaje: mensaje.into(),
                peticion_id,
            },
        );
    }
}

/// Responde a un cliente concreto, si sigue conectado.
async fn responder(estado: &Arc<Mutex<EstadoServidor>>, cliente_id: u32, mensaje: MensajeServidor) {
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        super::difusion::enviar(cliente, mensaje);
    }
}

/// Difunde la lista completa de acciones en curso (T27).
async fn difundir_acciones(estado: &Arc<Mutex<EstadoServidor>>) {
    let estado_bloqueado = estado.lock().await;
    super::difusion::difundir(
        &estado_bloqueado.clientes,
        MensajeServidor::AccionesServicio {
            lista: estado_bloqueado.servicios.acciones(),
        },
    );
}

/// Carga el host de una petición de servicios: tiene que existir y ser de tipo
/// `ssh` (§2: solo los hosts ssh tienen servicios).
async fn host_de(estado: &Arc<Mutex<EstadoServidor>>, host_id: i64) -> Result<Host, String> {
    let lectura = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado.lectura.clone()
    };
    let lectura = lectura
        .lock()
        .map_err(|_| "almacén envenenado".to_string())?;
    let host = lectura
        .obtener_host(host_id)
        .map_err(|_| format!("el host #{host_id} ya no existe"))?;
    if host.tipo != TipoHost::Ssh {
        return Err(format!(
            "«{}» no es un host ssh: solo los hosts ssh tienen servicios",
            host.nombre
        ));
    }
    Ok(host)
}

/// Todos los hosts del inventario, para la cadena de saltos.
async fn todos_los_hosts(
    estado: &Arc<Mutex<EstadoServidor>>,
) -> Result<HashMap<i64, Host>, String> {
    let lectura = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado.lectura.clone()
    };
    let lectura = lectura
        .lock()
        .map_err(|_| "almacén envenenado".to_string())?;
    Ok(lectura
        .listar_hosts()
        .map_err(|error| format!("no se pudo leer el inventario: {error}"))?
        .into_iter()
        .map(|host| (host.id, host))
        .collect())
}

/// Reparte un identificador de solicitud para los diálogos de conexión.
async fn nueva_solicitud(estado: &Arc<Mutex<EstadoServidor>>) -> u32 {
    let mut estado_bloqueado = estado.lock().await;
    let id = estado_bloqueado.siguiente_sesion_id;
    estado_bloqueado.siguiente_sesion_id += 1;
    id
}

/// Toma una conexión del pool para un canal de servicios. Se suelta con
/// `soltar` por identidad (o sola, con el `Drop`).
///
/// No lleva plazo propio: `conexion_para_canal` ya limita la red (DNS, TCP y
/// saludo) y los diálogos de huella, frase o contraseña esperan al usuario lo
/// que haga falta (regla del proyecto: el plazo de 10 s solo cubre la red).
/// El plazo de la operación se aplica luego al canal y a su salida.
async fn tomar_conexion(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    host: &Host,
) -> Result<ConexionTomada, String> {
    let todos = todos_los_hosts(estado).await?;
    let rutas = estado.lock().await.rutas.clone();
    let id_solicitud = nueva_solicitud(estado).await;
    super::conexiones::conexion_para_canal(
        estado,
        host,
        &todos,
        &rutas,
        PoliticaConexion::Interactiva {
            solicitante: cliente_id,
            id_solicitud,
        },
        UsoCanal::Servicios,
    )
    .await
}

// ---------------------------------------------------------------- canales

/// Lo que el lector de un canal `exec` de servicios ve.
enum EventoCanal {
    /// Datos de la salida estándar (MAGI_*, línea a línea o en bruto).
    Stdout(Vec<u8>),
    /// Datos de la salida de errores.
    Stderr(Vec<u8>),
    /// El canal terminó.
    Fin(DesenlaceCanal),
}

/// Cómo terminó un canal `exec` de servicios.
#[derive(Debug, Clone, Default)]
struct DesenlaceCanal {
    codigo: Option<i32>,
    senal: Option<String>,
    fallo: Option<String>,
    tiempo: bool,
    cancelado: bool,
}

/// Abre un canal `exec` con el script de servicios, escribe la contraseña de
/// sudo en la primera línea de la entrada estándar (vacía si no hay) y va
/// mandando lo que llega por `EventoCanal`. El canal se cierra siempre.
///
/// `plazo` es el plazo de lectura (None en los logs: duran lo que la
/// suscripción). `cancelar` cierra el canal desde fuera (PararLog o el
/// apagado). `cerrar_entrada` manda el fin de la entrada estándar después de
/// la contraseña (todo menos `logs`, que espera a que MAGI cierre el canal).
fn lanzar_canal(
    handle: Arc<Handle<Cliente>>,
    comando: String,
    contrasena: Option<Secreto>,
    plazo: Option<Duration>,
    cancelar: Option<tokio_util::sync::CancellationToken>,
    cerrar_entrada: bool,
) -> mpsc::UnboundedReceiver<EventoCanal> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let inicio = Instant::now();
        let apertura = handle.clone();
        let canal = super::conexiones::abrir_con_plazo(PLAZO_CANAL, async move {
            apertura.channel_open_session().await
        })
        .await;
        let mut canal = match canal {
            Ok(canal) => canal,
            Err(motivo) => {
                let _ = tx.send(EventoCanal::Fin(DesenlaceCanal {
                    fallo: Some(format!("no se pudo abrir el canal: {motivo}")),
                    ..DesenlaceCanal::default()
                }));
                return;
            }
        };
        if let Err(error) = canal.exec(true, comando.into_bytes()).await {
            let _ = tx.send(EventoCanal::Fin(DesenlaceCanal {
                fallo: Some(format!("el host rechazó la orden: {error}")),
                ..DesenlaceCanal::default()
            }));
            let _ = canal.close().await;
            return;
        }
        // La primera línea de stdin es siempre la contraseña (vacía si no
        // hay). Se construye en un `Zeroizing` y no se registra jamás (T25).
        let clave = contrasena.map(|secreto| secreto.0);
        let mut linea = Zeroizing::new(String::with_capacity(1));
        if let Some(clave) = &clave {
            linea.push_str(clave.as_str());
        }
        linea.push('\n');
        let escrito = canal.data_bytes(linea.as_bytes().to_vec()).await;
        drop(linea);
        drop(clave);
        if escrito.is_err() {
            let _ = tx.send(EventoCanal::Fin(DesenlaceCanal {
                fallo: Some("el host cerró la entrada del script".to_string()),
                ..DesenlaceCanal::default()
            }));
            let _ = canal.close().await;
            return;
        }
        if cerrar_entrada {
            let _ = canal.eof().await;
        }

        let limite = plazo.map(|plazo| inicio + plazo);
        let mut desenlace = DesenlaceCanal::default();
        loop {
            let lectura = async {
                match limite {
                    Some(limite) => {
                        let restante = limite.saturating_duration_since(Instant::now());
                        match tokio::time::timeout(restante, canal.wait()).await {
                            Ok(mensaje) => Ok(mensaje),
                            Err(_) => Err(()),
                        }
                    }
                    None => Ok(canal.wait().await),
                }
            };
            let cancelacion = async {
                match &cancelar {
                    Some(cancelar) => cancelar.cancelled().await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! {
                _ = cancelacion => {
                    desenlace.cancelado = true;
                    break;
                }
                leido = lectura => match leido {
                    Err(()) => {
                        desenlace.tiempo = true;
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(mensaje)) => match mensaje {
                        ChannelMsg::Data { data } => {
                            if tx.send(EventoCanal::Stdout(data.to_vec())).is_err() {
                                break;
                            }
                        }
                        ChannelMsg::ExtendedData { data, .. } => {
                            if tx.send(EventoCanal::Stderr(data.to_vec())).is_err() {
                                break;
                            }
                        }
                        ChannelMsg::ExitStatus { exit_status } => {
                            desenlace.codigo = Some(exit_status as i32);
                        }
                        ChannelMsg::ExitSignal { signal_name, .. } => {
                            desenlace.senal = Some(format!("{signal_name:?}"));
                        }
                        ChannelMsg::Failure => {
                            desenlace.fallo =
                                Some("el host rechazó la orden".to_string());
                        }
                        ChannelMsg::Close => break,
                        _ => {}
                    },
                },
            }
        }
        // El fin de la entrada estándar es lo que ve el script para cortar el
        // seguimiento en el host (logs) y cerrar su canal (T32). Se le deja un
        // instante para que lo haga antes de forzar el cierre: en un cierre
        // normal el remoto ya lo habrá cerrado él solo.
        let _ = canal.eof().await;
        if desenlace.cancelado {
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    match canal.wait().await {
                        None | Some(ChannelMsg::Close) => break,
                        // Si la tarea que leía ya se fue (cancelada), lo que
                        // llegue se descarta, pero se espera al cierre del
                        // remoto para no matarlo antes de que corte el
                        // seguimiento en el host.
                        Some(ChannelMsg::Data { data }) => {
                            let _ = tx.send(EventoCanal::Stdout(data.to_vec()));
                        }
                        Some(ChannelMsg::ExtendedData { data, .. }) => {
                            let _ = tx.send(EventoCanal::Stderr(data.to_vec()));
                        }
                        _ => {}
                    }
                }
            })
            .await;
        }
        let _ = tokio::time::timeout(Duration::from_secs(5), canal.close()).await;
        let _ = tx.send(EventoCanal::Fin(desenlace));
    });
    rx
}

/// Salida acumulada de una suborden del script, acotada.
#[derive(Default)]
struct SalidaCanal {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    codigo: Option<i32>,
    senal: Option<String>,
    fallo: Option<String>,
    tiempo: bool,
    truncada: bool,
}

impl SalidaCanal {
    fn anadir(&mut self, datos: &[u8], es_stdout: bool, tope: usize) {
        let destino = if es_stdout {
            &mut self.stdout
        } else {
            &mut self.stderr
        };
        let tope = if es_stdout { tope } else { TOPE_ERRORES };
        if destino.len() >= tope {
            self.truncada = true;
            return;
        }
        let libres = tope - destino.len();
        if datos.len() > libres {
            destino.extend_from_slice(&datos[..libres]);
            self.truncada = true;
        } else {
            destino.extend_from_slice(datos);
        }
    }

    fn texto(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn errores(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    fn aplicar_fin(&mut self, desenlace: DesenlaceCanal) {
        self.codigo = desenlace.codigo;
        self.senal = desenlace.senal;
        self.fallo = desenlace.fallo;
        self.tiempo = desenlace.tiempo;
    }
}

/// Ejecuta una suborden de `servicios.sh` en el host y devuelve su salida.
/// La conexión se suelta en todas las ramas (por identidad).
async fn ejecutar_suborden(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    host: &Host,
    suborden: &nucleo::orden::Suborden,
    contrasena: Option<&Secreto>,
    plazo: Duration,
    tope: usize,
) -> Result<SalidaCanal, String> {
    let tomada = tomar_conexion(estado, cliente_id, host).await?;
    // El plazo de la operación empieza cuando la conexión ya está tomada (los
    // diálogos de conexión no cuentan).
    let resultado = async {
        let privilegio = estado.lock().await.servicios.privilegio(host.id);
        let comando = nucleo::orden::comando(suborden, privilegio);
        let restante = plazo.max(Duration::from_millis(500));
        let mut eventos = lanzar_canal(
            tomada.handle.clone(),
            comando,
            contrasena.cloned(),
            Some(restante),
            None,
            true,
        );
        let limite = tokio::time::Instant::now() + restante;
        let mut salida = SalidaCanal::default();
        loop {
            match tokio::time::timeout_at(limite, eventos.recv()).await {
                Err(_) => {
                    salida.tiempo = true;
                    break;
                }
                Ok(None) => {
                    salida.tiempo = true;
                    break;
                }
                Ok(Some(EventoCanal::Stdout(datos))) => salida.anadir(&datos, true, tope),
                Ok(Some(EventoCanal::Stderr(datos))) => salida.anadir(&datos, false, tope),
                Ok(Some(EventoCanal::Fin(desenlace))) => {
                    salida.aplicar_fin(desenlace);
                    break;
                }
            }
        }
        if salida.tiempo {
            return Err(format!(
                "«{}» no respondió en {} s",
                host.nombre,
                plazo.as_secs()
            ));
        }
        if let Some(fallo) = &salida.fallo {
            return Err(fallo.clone());
        }
        Ok(salida)
    }
    .await;
    tomada.soltar().await;
    resultado
}

/// El privilegio real del host: la primera vez se averigua con la suborden
/// `privilegio` del script y se guarda como pista para las demás órdenes
/// (§7.3). Sin diálogos y con el plazo de un listado.
async fn privilegio_conocido(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    host: &Host,
) -> Result<Privilegio, String> {
    if let Some(privilegio) = estado.lock().await.servicios.privilegio_opt(host.id) {
        return Ok(privilegio);
    }
    let salida = ejecutar_suborden(
        estado,
        cliente_id,
        host,
        &nucleo::orden::Suborden::Privilegio,
        None,
        Duration::from_secs(nucleo::limites::PLAZO_LISTADO_SEG),
        TOPE_SUDO,
    )
    .await?;
    let (_, privilegio) = nucleo::lectura::privilegio(&salida.texto()).map_err(|error| {
        format!(
            "no se pudo leer el privilegio en «{}»: {error}",
            host.nombre
        )
    })?;
    estado
        .lock()
        .await
        .servicios
        .privilegios
        .insert(host.id, privilegio);
    Ok(privilegio)
}

/// Traduce la salida de errores del script a un mensaje legible (§6).
fn mensaje_de_error(salida: &SalidaCanal) -> String {
    let errores = salida.errores();
    for linea in errores.lines().rev() {
        let linea = linea.trim();
        if let Some(codigo) = linea.strip_prefix("ERROR ") {
            return match codigo.trim() {
                "objetivo" => "el objetivo no es válido para acciones".to_string(),
                "privilegio" => "el privilegio del script no es válido".to_string(),
                "sin_privilegio" => {
                    "sin privilegios en el host para esa orden; dale la contraseña de sudo con «!»"
                        .to_string()
                }
                "sin_permiso" => {
                    "sin permiso en el host para leer eso; dale la contraseña de sudo con «!»"
                        .to_string()
                }
                otro => format!("el host devolvió «{otro}»"),
            };
        }
    }
    if let Some(senal) = &salida.senal {
        return format!("la orden terminó por la señal {senal}");
    }
    match salida.codigo {
        Some(codigo) => format!("el host devolvió el código {codigo}"),
        None => "el host cerró el canal sin decir cómo terminó".to_string(),
    }
}

// ---------------------------------------------------------------- listado

async fn listar(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    sudo: Option<Secreto>,
) {
    if !peticion_del_rango(peticion_id) {
        responder_error(
            estado,
            cliente_id,
            "petición de servicios fuera de su rango",
            Some(peticion_id),
        )
        .await;
        return;
    }
    let host = match host_de(estado, host_id).await {
        Ok(host) => host,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    // La pista de privilegio del script: se averigua la primera vez y se
    // guarda por host; el listado la refresca con su `MAGI_PRIV` (§7.3).
    if let Err(motivo) = privilegio_conocido(estado, cliente_id, &host).await {
        debug!(host_id, %motivo, "sin privilegio averiguado; se lista con sin_sudo");
    }
    let motores = {
        let config = estado.lock().await.config.clone();
        let preferencia: Vec<nucleo::MotorNombre> = config
            .servicios
            .contenedores
            .iter()
            .filter_map(|motor| nucleo::MotorNombre::desde_texto(motor))
            .collect();
        nucleo::orden::motores_para_listar(&preferencia)
    };
    let vigiladas = crate::modelo::servicios_de(&host);
    let suborden = nucleo::orden::Suborden::Listar { motores };
    let salida = match ejecutar_suborden(
        estado,
        cliente_id,
        &host,
        &suborden,
        sudo.as_ref(),
        Duration::from_secs(nucleo::limites::PLAZO_LISTADO_SEG),
        TOPE_LISTADO,
    )
    .await
    {
        Ok(salida) => salida,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    let mut texto = salida.texto();
    if salida.truncada {
        // Se corta lo que sobra y se cierra el bloque para que el lector
        // pueda decir «lista incompleta» en vez de romperse (T38).
        if !texto.ends_with('\n') {
            texto.push('\n');
        }
        texto.push_str("MAGI_FIN\n");
    }
    match nucleo::lectura::listado(&texto, &vigiladas) {
        Ok(mut listado) => {
            if salida.truncada {
                listado.incompleto = true;
                listado
                    .avisos
                    .push("lista incompleta: la salida del host pasó de 2 MiB".to_string());
            }
            {
                let mut estado_bloqueado = estado.lock().await;
                estado_bloqueado
                    .servicios
                    .privilegios
                    .insert(host_id, listado.privilegio);
                if let Some(motor) = listado.motor.disponible() {
                    estado_bloqueado
                        .servicios
                        .motores
                        .insert(host_id, motor.nombre);
                } else {
                    estado_bloqueado.servicios.motores.remove(&host_id);
                }
            }
            responder(
                estado,
                cliente_id,
                MensajeServidor::Servicios {
                    peticion_id,
                    host_id,
                    listado,
                },
            )
            .await;
        }
        Err(error) => {
            let detalle = if salida.errores().trim().is_empty() {
                error.to_string()
            } else {
                mensaje_de_error(&salida)
            };
            responder_error(
                estado,
                cliente_id,
                format!("no se pudo leer el listado de «{}»: {detalle}", host.nombre),
                Some(peticion_id),
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------- consumo

async fn consumo(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    sudo: Option<Secreto>,
) {
    if !peticion_del_rango(peticion_id) {
        responder_error(
            estado,
            cliente_id,
            "petición de servicios fuera de su rango",
            Some(peticion_id),
        )
        .await;
        return;
    }
    let host = match host_de(estado, host_id).await {
        Ok(host) => host,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    if let Err(motivo) = privilegio_conocido(estado, cliente_id, &host).await {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let motor = estado.lock().await.servicios.motor(host_id);
    let Some(motor) = motor else {
        responder_error(
            estado,
            cliente_id,
            format!(
                "todavía no se conoce el motor de contenedores de «{}»; pide el listado primero",
                host.nombre
            ),
            Some(peticion_id),
        )
        .await;
        return;
    };
    let suborden = nucleo::orden::Suborden::Consumo { motor };
    let salida = match ejecutar_suborden(
        estado,
        cliente_id,
        &host,
        &suborden,
        sudo.as_ref(),
        Duration::from_secs(nucleo::limites::PLAZO_LISTADO_SEG),
        TOPE_CONSUMO,
    )
    .await
    {
        Ok(salida) => salida,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    match nucleo::lectura::consumo(&salida.texto()) {
        Ok(consumos) => {
            responder(
                estado,
                cliente_id,
                MensajeServidor::Consumo {
                    peticion_id,
                    host_id,
                    consumos,
                },
            )
            .await;
        }
        Err(error) => {
            responder_error(
                estado,
                cliente_id,
                format!("no se pudo leer el consumo de «{}»: {error}", host.nombre),
                Some(peticion_id),
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------- detalle

async fn detalle(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    objetivo: Objetivo,
    sudo: Option<Secreto>,
) {
    if !peticion_del_rango(peticion_id) {
        responder_error(
            estado,
            cliente_id,
            "petición de servicios fuera de su rango",
            Some(peticion_id),
        )
        .await;
        return;
    }
    if let Err(motivo) = nucleo::orden::validar_objetivo(&objetivo) {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let host = match host_de(estado, host_id).await {
        Ok(host) => host,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    if let Err(motivo) = privilegio_conocido(estado, cliente_id, &host).await {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let suborden = nucleo::orden::Suborden::Detalle(objetivo.clone());
    let salida = match ejecutar_suborden(
        estado,
        cliente_id,
        &host,
        &suborden,
        sudo.as_ref(),
        Duration::from_secs(nucleo::limites::PLAZO_LISTADO_SEG),
        TOPE_DETALLE,
    )
    .await
    {
        Ok(salida) => salida,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    match nucleo::lectura::detalle(&salida.texto(), &objetivo) {
        Ok(detalle) => {
            responder(
                estado,
                cliente_id,
                MensajeServidor::Detalle {
                    peticion_id,
                    host_id,
                    detalle,
                },
            )
            .await;
        }
        Err(error) => {
            responder_error(
                estado,
                cliente_id,
                format!("no se pudo leer el detalle en «{}»: {error}", host.nombre),
                Some(peticion_id),
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------- sudo

async fn comprobar_sudo(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    contrasena: Secreto,
) {
    if !peticion_del_rango(peticion_id) {
        responder_error(
            estado,
            cliente_id,
            "petición de servicios fuera de su rango",
            Some(peticion_id),
        )
        .await;
        return;
    }
    let host = match host_de(estado, host_id).await {
        Ok(host) => host,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    if let Err(motivo) = privilegio_conocido(estado, cliente_id, &host).await {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let suborden = nucleo::orden::Suborden::Sudo;
    let salida = match ejecutar_suborden(
        estado,
        cliente_id,
        &host,
        &suborden,
        Some(&contrasena),
        Duration::from_secs(nucleo::limites::PLAZO_LISTADO_SEG),
        TOPE_SUDO,
    )
    .await
    {
        Ok(salida) => salida,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    match nucleo::lectura::sudo(&salida.texto()) {
        Ok(resultado) => {
            if resultado == ComprobacionSudo::Ok {
                // Ya se sabe que la contraseña vale: la pista para las
                // siguientes órdenes es `sudo_con_clave`, salvo que el
                // listado ya supiera que no hace falta sudo.
                let mut estado_bloqueado = estado.lock().await;
                let actual = estado_bloqueado.servicios.privilegio(host_id);
                if matches!(actual, Privilegio::SinSudo | Privilegio::SudoConClave) {
                    estado_bloqueado
                        .servicios
                        .privilegios
                        .insert(host_id, Privilegio::SudoConClave);
                }
            }
            responder(
                estado,
                cliente_id,
                MensajeServidor::SudoComprobado {
                    peticion_id,
                    resultado,
                },
            )
            .await;
        }
        Err(error) => {
            responder_error(
                estado,
                cliente_id,
                format!(
                    "no se pudo comprobar la contraseña de sudo en «{}»: {error}",
                    host.nombre
                ),
                Some(peticion_id),
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------- acción

/// Comprobación propia de COMPROBACIONES_SERVICIO para un objetivo (§7.5).
/// La fila la escribe otro proceso: se valida antes de usarla y, si no vale,
/// se ignora.
async fn comprobacion_propia(
    estado: &Arc<Mutex<EstadoServidor>>,
    host_id: i64,
    objetivo: &Objetivo,
) -> Option<String> {
    let tipo = match objetivo.tipo {
        TipoObjetivo::Unidad | TipoObjetivo::Timer => "unidad",
        TipoObjetivo::Proyecto => "proyecto",
        TipoObjetivo::Contenedor => return None,
    };
    let lectura = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado.lectura.clone()
    };
    let lectura = match lectura.lock() {
        Ok(lectura) => lectura,
        Err(_) => return None,
    };
    let comando: Option<String> = {
        let mut sentencia = lectura
            .conexion()
            .prepare_cached(
                "SELECT comando FROM COMPROBACIONES_SERVICIO \
                 WHERE host_id = ?1 AND objetivo_tipo = ?2 AND objetivo = ?3",
            )
            .ok()?;
        sentencia
            .query_row(rusqlite::params![host_id, tipo, objetivo.nombre], |fila| {
                fila.get(0)
            })
            .ok()
    };
    let comando = comando?;
    if nucleo::orden::validar_comando_propio(&comando).is_err() {
        // Ni el objetivo ni la orden van al log (T44): solo que la fila no
        // vale y se ignora.
        warn!(host_id, "comprobación propia no válida: se ignora");
        return None;
    }
    Some(comando)
}

/// La comprobación de configuración que toca antes de una acción (§7.5): la
/// propia sustituye a la conocida.
async fn comprobacion_de(
    estado: &Arc<Mutex<EstadoServidor>>,
    host_id: i64,
    objetivo: &Objetivo,
    accion: Accion,
) -> nucleo::orden::Comprobacion {
    if !nucleo::acciones::comprueba_configuracion(objetivo, accion) {
        return nucleo::orden::Comprobacion::Ninguna;
    }
    let propia = comprobacion_propia(estado, host_id, objetivo).await;
    match nucleo::validadores::para_objetivo(objetivo, propia.as_deref()) {
        nucleo::validadores::Comprobacion::Ninguna => nucleo::orden::Comprobacion::Ninguna,
        nucleo::validadores::Comprobacion::Conocida(_) => nucleo::orden::Comprobacion::Conocida,
        nucleo::validadores::Comprobacion::Propia(comando) => {
            nucleo::orden::Comprobacion::Propia(comando)
        }
    }
}

/// ¿La acción es posible en ese tipo de objetivo? (El estado no se mira en el
/// servidor, §7.11; esto es solo coherencia del protocolo.)
fn accion_valida(tipo: TipoObjetivo, accion: Accion) -> bool {
    match tipo {
        TipoObjetivo::Timer => accion == Accion::Lanzar,
        TipoObjetivo::Unidad => accion != Accion::Lanzar,
        TipoObjetivo::Contenedor | TipoObjetivo::Proyecto => {
            matches!(accion, Accion::Arrancar | Accion::Parar | Accion::Reiniciar)
        }
    }
}

/// Valida la deliberación de una acción de servicio: existe, es de esta acción
/// (objetivo y host) y no está usada (T62). Sin el mutex del estado: es E/S de
/// SQLite y podría tardar hasta el `busy_timeout`.
async fn validar_deliberacion(
    estado: &Arc<Mutex<EstadoServidor>>,
    lanzada: &crate::protocolo::DeliberacionLanzada,
    objetivo: &Objetivo,
    accion: Accion,
    host: &Host,
) -> Result<ejecuciones::DeliberacionFijada, String> {
    let lectura = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado.lectura.clone()
    };
    let lectura = lectura
        .lock()
        .map_err(|_| "almacén envenenado".to_string())?;
    let fijada = ejecuciones::validar_deliberacion(&lectura, lanzada)?;
    if fijada.registro.snippet_id.is_some() {
        return Err(format!(
            "la deliberación #{} no es de una acción de servicio",
            lanzada.id
        ));
    }
    let esperada = format!("{} {} → {}", accion.texto(), objetivo.nombre, host.nombre);
    if fijada.registro.accion != esperada {
        return Err(format!(
            "la deliberación #{} no autoriza esa acción",
            lanzada.id
        ));
    }
    if fijada.registro.hosts.len() != 1 || fijada.registro.hosts[0].0 != host.id {
        return Err(format!(
            "la deliberación #{} no es de este host",
            lanzada.id
        ));
    }
    Ok(fijada)
}

#[allow(clippy::too_many_arguments)]
async fn accion_servicio(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    objetivo: Objetivo,
    accion: Accion,
    deliberacion: Option<crate::protocolo::DeliberacionLanzada>,
    sudo: Option<Secreto>,
) {
    if !peticion_del_rango(peticion_id) {
        responder_error(
            estado,
            cliente_id,
            "petición de servicios fuera de su rango",
            Some(peticion_id),
        )
        .await;
        return;
    }
    if let Err(motivo) = nucleo::orden::validar_objetivo(&objetivo) {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    if !accion_valida(objetivo.tipo, accion) {
        responder_error(
            estado,
            cliente_id,
            format!("«{}» no admite {}", objetivo.nombre, accion.texto()),
            Some(peticion_id),
        )
        .await;
        return;
    }
    let host = match host_de(estado, host_id).await {
        Ok(host) => host,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    // El servidor recomprueba qué exige deliberación (§7.2, T62): el cliente
    // no puede saltárselo.
    let exige = nucleo::acciones::exige_deliberacion(&objetivo, accion);
    let fijada = match &deliberacion {
        Some(lanzada) => {
            match validar_deliberacion(estado, lanzada, &objetivo, accion, &host).await {
                Ok(fijada) => Some(fijada),
                Err(motivo) => {
                    responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
                    return;
                }
            }
        }
        None if exige => {
            responder_error(
                estado,
                cliente_id,
                format!(
                    "{} {} deja algo parado: hace falta deliberar antes",
                    accion.texto(),
                    objetivo.nombre
                ),
                Some(peticion_id),
            )
            .await;
            return;
        }
        None => None,
    };
    if let Err(motivo) = privilegio_conocido(estado, cliente_id, &host).await {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let comprobacion = comprobacion_de(estado, host_id, &objetivo, accion).await;
    let espera_seg = {
        let config = estado.lock().await.config.clone();
        config.servicios.espera_seg.clamp(5, 300)
    };
    let suborden = nucleo::orden::Suborden::Accion {
        objetivo: objetivo.clone(),
        accion,
        espera_seg: espera_seg as u32,
        comprobacion,
    };
    let fase_inicial = match &suborden {
        nucleo::orden::Suborden::Accion { comprobacion, .. } => match comprobacion {
            nucleo::orden::Comprobacion::Ninguna => nucleo::FaseAccion::Ejecutando,
            _ => nucleo::FaseAccion::Comprobando,
        },
        _ => nucleo::FaseAccion::Ejecutando,
    };
    let cancelar = tokio_util::sync::CancellationToken::new();
    let info = InfoAccion {
        id: peticion_id,
        host_id,
        host_nombre: host.nombre.clone(),
        objetivo: objetivo.clone(),
        accion,
        fase: fase_inicial,
        desde_ms: chrono::Utc::now().timestamp_millis(),
        solicitante: cliente_id,
    };
    // Una sola acción por objetivo (§3.8, T27) y una sola deliberación por
    // acción (T62): las dos cosas se miran bajo el mismo bloqueo que inserta.
    {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            drop(estado_bloqueado);
            responder_error(
                estado,
                cliente_id,
                super::conexiones::APAGANDO.to_string(),
                Some(peticion_id),
            )
            .await;
            return;
        }
        let ocupado = estado_bloqueado
            .servicios
            .acciones
            .values()
            .any(|otra| otra.info.host_id == host_id && otra.info.objetivo == objetivo);
        if ocupado {
            drop(estado_bloqueado);
            responder_error(
                estado,
                cliente_id,
                "hay otra acción en curso sobre ese objetivo".to_string(),
                Some(peticion_id),
            )
            .await;
            return;
        }
        if let Some(fijada) = &fijada {
            if estado_bloqueado
                .servicios
                .deliberacion_usada(fijada.registro.id)
                || ejecuciones::deliberacion_usada(&estado_bloqueado, fijada.registro.id)
            {
                drop(estado_bloqueado);
                responder_error(
                    estado,
                    cliente_id,
                    ejecuciones::YA_USADA.to_string(),
                    Some(peticion_id),
                )
                .await;
                return;
            }
        }
        estado_bloqueado.vacio_desde = None;
        estado_bloqueado.servicios.acciones.insert(
            (cliente_id, peticion_id),
            AccionEnCurso {
                info: info.clone(),
                solicitante: cliente_id,
                espera_seg,
                desde: Instant::now(),
                deliberacion: fijada,
                cancelar: cancelar.clone(),
            },
        );
    }
    // La acción en curso se difunde antes de su efecto (T40).
    difundir_acciones(estado).await;
    info!(
        peticion_id,
        host = %host.nombre,
        accion = accion.texto(),
        "acción de servicio en curso"
    );
    let estado_tarea = estado.clone();
    tokio::spawn(async move {
        ejecutar_accion(
            estado_tarea,
            cliente_id,
            peticion_id,
            host,
            suborden,
            sudo,
            espera_seg,
            cancelar,
        )
        .await;
    });
}

/// Cuerpo de una acción: toma la conexión, ejecuta el script mientras sigue
/// las fases, interpreta el resultado y lo lleva a su fin (REGISTRO,
/// deliberación, difusión y respuesta).
#[allow(clippy::too_many_arguments)]
async fn ejecutar_accion(
    estado: Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host: Host,
    suborden: nucleo::orden::Suborden,
    sudo: Option<Secreto>,
    espera_seg: u64,
    cancelar: tokio_util::sync::CancellationToken,
) {
    // El plazo total de una acción es `espera_seg + 30 s` e incluye la
    // comprobación de configuración (que dentro del script tiene sus 20 s).
    // La conexión no entra en ese plazo: los diálogos de huella, frase o
    // contraseña no tienen plazo (regla del proyecto).
    let plazo = Duration::from_secs(espera_seg + nucleo::limites::MARGEN_ACCION_SEG);
    let resultado = match tomar_conexion(&estado, cliente_id, &host).await {
        Ok(tomada) => {
            let limite = tokio::time::Instant::now() + plazo;
            let privilegio = estado.lock().await.servicios.privilegio(host.id);
            let comando = nucleo::orden::comando(&suborden, privilegio);
            let restante = plazo.max(Duration::from_millis(500));
            let mut eventos = lanzar_canal(
                tomada.handle.clone(),
                comando,
                sudo,
                Some(restante),
                Some(cancelar.clone()),
                true,
            );
            let mut salida = SalidaCanal::default();
            let mut pendiente = Vec::new();
            let fin = loop {
                if cancelar.is_cancelled() {
                    break None;
                }
                match tokio::time::timeout_at(limite, eventos.recv()).await {
                    Err(_) => {
                        salida.tiempo = true;
                        break Some(None);
                    }
                    Ok(None) => {
                        salida.tiempo = true;
                        break Some(None);
                    }
                    Ok(Some(EventoCanal::Stdout(datos))) => {
                        salida.anadir(&datos, true, TOPE_ACCION);
                        // Las fases que canta el script se difunden en vivo.
                        for linea in extraer_lineas(&mut pendiente, &datos) {
                            if let Some(fase) = nucleo::lectura::fase(&linea) {
                                actualizar_fase(&estado, cliente_id, peticion_id, fase).await;
                            }
                        }
                    }
                    Ok(Some(EventoCanal::Stderr(datos))) => {
                        salida.anadir(&datos, false, TOPE_ACCION);
                    }
                    Ok(Some(EventoCanal::Fin(desenlace))) => {
                        salida.aplicar_fin(desenlace);
                        break Some(Some(salida));
                    }
                }
            };
            // Se suelta la conexión por identidad en todas las ramas (T34).
            tomada.soltar().await;
            match fin {
                None => None,
                Some(None) => Some(Err(format!(
                    "sin respuesta de «{}»; la orden puede seguir en el host",
                    host.nombre
                ))),
                Some(Some(salida)) => Some(resultado_de_salida(&salida, &suborden)),
            }
        }
        Err(motivo) => Some(Err(motivo)),
    };
    let Some(resultado) = resultado else {
        // Cancelada por el apagado: el efecto sigue en el host y REGISTRO no
        // la anota (§8).
        return;
    };
    terminar_accion(&estado, cliente_id, peticion_id, resultado).await;
}

/// El objetivo y la acción de una acción en curso, para interpretar la salida.
fn objetivo_y_accion(suborden: &nucleo::orden::Suborden) -> Option<(&Objetivo, Accion)> {
    match suborden {
        nucleo::orden::Suborden::Accion {
            objetivo, accion, ..
        } => Some((objetivo, *accion)),
        _ => None,
    }
}

/// Interpreta la salida de la suborden `accion` (§5.1, §7.4).
fn resultado_de_salida(
    salida: &SalidaCanal,
    suborden: &nucleo::orden::Suborden,
) -> Result<ResultadoServicio, String> {
    let Some((objetivo, _)) = objetivo_y_accion(suborden) else {
        return Err("la orden no era una acción".to_string());
    };
    match nucleo::lectura::accion(&salida.texto(), objetivo) {
        Ok(resultado) => Ok(resultado),
        Err(error) => {
            if salida.errores().trim().is_empty() && salida.codigo == Some(124) {
                return Ok(ResultadoServicio {
                    resultado: nucleo::ResultadoAccion::SinConfirmar,
                    mensaje: "la orden no terminó a tiempo: puede seguir en el host".to_string(),
                    ..ResultadoServicio::default()
                });
            }
            let mut resultado = ResultadoServicio {
                resultado: nucleo::ResultadoAccion::Error,
                mensaje: if salida.truncada {
                    // B2 de la revisión: sin MAGI_RESULTADO y con la salida
                    // cortada, se dice explícitamente que se perdió.
                    format!(
                        "la salida del host pasó de {} MiB y no se pudo leer el resultado",
                        TOPE_ACCION / (1024 * 1024)
                    )
                } else if salida.errores().trim().is_empty() {
                    format!("no se pudo leer el resultado: {error}")
                } else {
                    mensaje_de_error(salida)
                },
                ..ResultadoServicio::default()
            };
            if salida.tiempo {
                resultado.mensaje = format!(
                    "sin respuesta del host; la orden puede seguir: {}",
                    resultado.mensaje
                );
            }
            Ok(resultado)
        }
    }
}

/// Actualiza la fase de una acción en curso y la difunde (T27).
async fn actualizar_fase(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    fase: nucleo::FaseAccion,
) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(accion) = estado_bloqueado
        .servicios
        .acciones
        .get_mut(&(cliente_id, peticion_id))
    else {
        return;
    };
    if accion.info.fase == fase {
        return;
    }
    accion.info.fase = fase;
    let lista = estado_bloqueado.servicios.acciones();
    super::difusion::difundir(
        &estado_bloqueado.clientes,
        MensajeServidor::AccionesServicio { lista },
    );
}

/// Saca del búfer las líneas completas que haya y deja el resto para el
/// siguiente trozo. Una línea sin fin no puede crecer sin límite (§8).
fn extraer_lineas(pendiente: &mut Vec<u8>, datos: &[u8]) -> Vec<String> {
    pendiente.extend_from_slice(datos);
    let mut lineas = Vec::new();
    while let Some(posicion) = pendiente.iter().position(|byte| *byte == b'\n') {
        let linea: Vec<u8> = pendiente.drain(..=posicion).collect();
        lineas.push(String::from_utf8_lossy(&linea[..linea.len().saturating_sub(1)]).into_owned());
    }
    if pendiente.len() > TOPE_ACCION {
        pendiente.clear();
    }
    lineas
}

/// Acumula un trozo de salida y entrega por líneas completas lo que haya.
fn acumular_lineas(pendiente: &mut Vec<u8>, datos: &[u8], mut al_linea: impl FnMut(&str)) {
    for linea in extraer_lineas(pendiente, datos) {
        al_linea(&linea);
    }
}

/// Lleva una acción a su fin: REGISTRO y deliberación tras la barrera, lista
/// de acciones en curso actualizada y `AccionServicioHecha` al solicitante.
async fn terminar_accion(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    resultado: Result<ResultadoServicio, String>,
) {
    let resultado = match resultado {
        Ok(resultado) => resultado,
        Err(mensaje) => ResultadoServicio {
            resultado: nucleo::ResultadoAccion::Error,
            mensaje,
            ..ResultadoServicio::default()
        },
    };
    // La anotación se encola antes de quitar la acción del mapa y la
    // deliberación queda marcada «en vuelo» hasta que la Barrera confirma la
    // escritura: así no hay una ventana en la que parezca no usada (B7).
    let (accion, en_vuelo) = {
        let mut estado_bloqueado = estado.lock().await;
        let en_vuelo = estado_bloqueado
            .servicios
            .acciones
            .get(&(cliente_id, peticion_id))
            .and_then(|accion| accion.deliberacion.as_ref())
            .map(|fijada| fijada.registro.id);
        if let Some(id) = en_vuelo {
            estado_bloqueado
                .servicios
                .deliberaciones_en_vuelo
                .insert(id);
        }
        if let Some(accion) = estado_bloqueado
            .servicios
            .acciones
            .get(&(cliente_id, peticion_id))
        {
            anotar_accion(&estado_bloqueado, accion, &resultado);
        }
        (
            estado_bloqueado
                .servicios
                .acciones
                .remove(&(cliente_id, peticion_id)),
            en_vuelo,
        )
    };
    let Some(accion) = accion else {
        // La acción ya no estaba (cancelada): se retira la marca si se puso.
        if let Some(id) = en_vuelo {
            estado
                .lock()
                .await
                .servicios
                .deliberaciones_en_vuelo
                .remove(&id);
        }
        return;
    };
    // La respuesta sale con la fila ya en REGISTRO (T47).
    let _ = super::confirmar_escritura(estado).await;
    if let Some(id) = en_vuelo {
        estado
            .lock()
            .await
            .servicios
            .deliberaciones_en_vuelo
            .remove(&id);
    }
    difundir_acciones(estado).await;
    info!(
        peticion_id,
        resultado = resultado.resultado.texto(),
        "acción de servicio terminada"
    );
    responder(
        estado,
        accion.solicitante,
        MensajeServidor::AccionServicioHecha {
            peticion_id,
            resultado,
        },
    )
    .await;
}

/// Anota una acción terminada en REGISTRO (§3.6): objetivo, acción,
/// resultado, duración y deliberación; nunca la salida de las órdenes (T44).
fn anotar_accion(estado: &EstadoServidor, accion: &AccionEnCurso, resultado: &ResultadoServicio) {
    let duracion = format!("{:.1}", accion.desde.elapsed().as_secs_f32()).replace('.', ",");
    // Objetivo (tipo y nombre), acción, resultado, duración y deliberación;
    // nunca la salida de las órdenes (T44).
    let mut detalle = format!(
        "{} · {} {} · {} · {} · {} s",
        accion.info.host_nombre,
        accion.info.objetivo.tipo.texto(),
        accion.info.objetivo.nombre,
        accion.info.accion.texto(),
        resultado.resultado.texto(),
        duracion
    );
    if let Some(fijada) = &accion.deliberacion {
        detalle.push_str(&format!(" · deliberación #{}", fijada.registro.id));
        let desenlace = if resultado.resultado.es_ok() {
            crate::deliberacion::EjecucionResultado::Ok
        } else {
            crate::deliberacion::EjecucionResultado::Error
        };
        ejecuciones::anotar_deliberacion(&estado.bd, fijada, desenlace);
    }
    let _ = estado.bd.send(super::OrdenBd::Anotar {
        tipo: crate::registro::SERVICIO_ACCION.to_string(),
        host_id: Some(accion.info.host_id),
        identidad_id: None,
        detalle,
        resultado: if resultado.resultado.es_ok() {
            ResultadoRegistro::Ok
        } else {
            ResultadoRegistro::Error
        },
    });
}

// ---------------------------------------------------------------- logs

/// Cola de una suscripción de log con contrapresión (§7.6): si crece por
/// encima de 4 MiB se descartan las líneas más viejas y se cuentan.
#[derive(Default)]
struct ColaSuscripcion {
    lineas: VecDeque<LineaLog>,
    bytes: usize,
    omitidas: u32,
}

impl ColaSuscripcion {
    fn anadir(&mut self, linea: LineaLog) {
        self.bytes += linea.texto.len();
        self.lineas.push_back(linea);
        while self.bytes > COLA_LOGS && self.lineas.len() > 1 {
            if let Some(vieja) = self.lineas.pop_front() {
                self.bytes -= vieja.texto.len();
                self.omitidas = self.omitidas.saturating_add(1);
            }
        }
        if self.lineas.len() > 1 && self.bytes > COLA_LOGS {
            // Una sola línea que ya pasa del tope: se queda (recortada) y se
            // cuentan las que se descarten a partir de ahora.
            self.omitidas = self.omitidas.saturating_add(0);
        }
    }

    /// Saca un lote de hasta 500 líneas con las omitidas acumuladas, sin pasar
    /// de `presupuesto` bytes una vez codificado en JSON. Siempre saca al
    /// menos una línea (cada una va acotada a 16 KiB antes).
    fn lote(&mut self, presupuesto: usize) -> (Vec<LineaLog>, u32) {
        let mut lote = Vec::with_capacity(LOTE_LOGS.min(self.lineas.len()));
        let mut codificados = 0usize;
        while lote.len() < LOTE_LOGS {
            let Some(frente) = self.lineas.front() else {
                break;
            };
            let tamano = tamano_codificado(frente);
            if !lote.is_empty() && codificados.saturating_add(tamano) > presupuesto {
                break;
            }
            let Some(linea) = self.lineas.pop_front() else {
                break;
            };
            self.bytes -= linea.texto.len();
            codificados = codificados.saturating_add(tamano);
            lote.push(linea);
        }
        let omitidas = std::mem::take(&mut self.omitidas);
        (lote, omitidas)
    }

    fn vacia(&self) -> bool {
        self.lineas.is_empty()
    }
}

/// Tamaño que ocupa una línea al codificarse dentro de un `LineasLog` (con el
/// escapado de JSON), más la coma que la separa de la siguiente.
fn tamano_codificado(linea: &LineaLog) -> usize {
    serde_json::to_string(linea)
        .map(|texto| texto.len())
        .unwrap_or(linea.texto.len().saturating_mul(2))
        .saturating_add(1)
}

/// Una línea de log de lo que llega por stdout del script, según el objetivo.
fn linea_de_log(objetivo: &Objetivo, linea: &str) -> Option<LineaLog> {
    let mut linea =
        nucleo::lectura::linea_de_log(linea, objetivo.tipo == TipoObjetivo::Contenedor)?;
    if linea.texto.len() > TOPE_LINEA {
        let recortado = nucleo::lectura::recortar_utf8(&linea.texto, TOPE_LINEA).to_string();
        linea.texto = recortado;
    }
    Some(linea)
}

async fn seguir_log(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: i64,
    objetivo: Objetivo,
    lineas_iniciales: u32,
    sudo: Option<Secreto>,
) {
    if !peticion_del_rango(peticion_id) {
        responder_error(
            estado,
            cliente_id,
            "petición de servicios fuera de su rango",
            Some(peticion_id),
        )
        .await;
        return;
    }
    if let Err(motivo) = nucleo::orden::validar_objetivo(&objetivo) {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    if matches!(objetivo.tipo, TipoObjetivo::Proyecto) {
        responder_error(
            estado,
            cliente_id,
            "un proyecto Compose no tiene logs propios: elige un contenedor".to_string(),
            Some(peticion_id),
        )
        .await;
        return;
    }
    let host = match host_de(estado, host_id).await {
        Ok(host) => host,
        Err(motivo) => {
            responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
            return;
        }
    };
    if let Err(motivo) = privilegio_conocido(estado, cliente_id, &host).await {
        responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let lineas_iniciales = lineas_iniciales.min(LINEAS_INICIALES_MAX);
    let suborden = nucleo::orden::Suborden::Logs {
        objetivo: objetivo.clone(),
        lineas_iniciales,
    };
    let cancelar = tokio_util::sync::CancellationToken::new();
    // Una suscripción nueva del mismo cliente al mismo objetivo sustituye a la
    // anterior (§3.8).
    let anterior = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            drop(estado_bloqueado);
            responder_error(
                estado,
                cliente_id,
                super::conexiones::APAGANDO.to_string(),
                Some(peticion_id),
            )
            .await;
            return;
        }
        let clave = estado_bloqueado
            .servicios
            .logs
            .iter()
            .find(|((cliente, _), suscripcion)| {
                *cliente == cliente_id
                    && suscripcion.host_id == host_id
                    && suscripcion.objetivo == objetivo
            })
            .map(|(clave, _)| *clave);
        if let Some(clave) = clave {
            estado_bloqueado.servicios.logs.remove(&clave)
        } else {
            None
        }
    };
    if let Some(anterior) = anterior {
        anterior.cancelar.cancel();
        responder(
            estado,
            cliente_id,
            MensajeServidor::FinLog {
                peticion_id: anterior.peticion_id,
                motivo: MotivoFinLog::Cerrado,
            },
        )
        .await;
        soltar_suscripcion(anterior).await;
    }
    {
        let mut estado_bloqueado = estado.lock().await;
        if !estado_bloqueado.clientes.contains_key(&cliente_id) {
            // La ventana se fue mientras se preparaba el seguimiento: no se
            // deja una suscripción huérfana (M1 de la revisión).
            return;
        }
        estado_bloqueado.vacio_desde = None;
        estado_bloqueado.servicios.logs.insert(
            (cliente_id, peticion_id),
            Suscripcion {
                peticion_id,
                solicitante: cliente_id,
                host_id,
                objetivo: objetivo.clone(),
                cancelar: cancelar.clone(),
                tomada: None,
            },
        );
    }
    let estado_tarea = estado.clone();
    tokio::spawn(async move {
        ejecutar_log(
            estado_tarea,
            peticion_id,
            cliente_id,
            host,
            suborden,
            sudo,
            cancelar,
        )
        .await;
    });
}

/// Cuerpo de una suscripción de log: abre el canal, lee líneas y manda lotes
/// cada 100 ms hasta que se cierra el panel o termina el proceso remoto.
async fn ejecutar_log(
    estado: Arc<Mutex<EstadoServidor>>,
    peticion_id: u64,
    cliente_id: u32,
    host: Host,
    suborden: nucleo::orden::Suborden,
    sudo: Option<Secreto>,
    cancelar: tokio_util::sync::CancellationToken,
) {
    let objetivo = match &suborden {
        nucleo::orden::Suborden::Logs { objetivo, .. } => objetivo.clone(),
        _ => return,
    };
    let tomada = match tomar_conexion(&estado, cliente_id, &host).await {
        Ok(tomada) => tomada,
        Err(motivo) => {
            debug!(peticion_id, %motivo, "no se pudo abrir el seguimiento de logs");
            terminar_log(
                &estado,
                cliente_id,
                peticion_id,
                Some(MotivoFinLog::ConexionCaida),
            )
            .await;
            return;
        }
    };
    let privilegio = estado.lock().await.servicios.privilegio(host.id);
    let comando = nucleo::orden::comando(&suborden, privilegio);
    let handle = tomada.handle.clone();
    // La conexión vive con la suscripción mientras dura el seguimiento: así el
    // canal sigue contado en el pool y su gracia no la cierra (crítico 2 de la
    // revisión). Se suelta al quitar la entrada del mapa, en todas las ramas.
    let mut tomada = Some(tomada);
    {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(suscripcion) = estado_bloqueado
            .servicios
            .logs
            .get_mut(&(cliente_id, peticion_id))
        {
            suscripcion.tomada = tomada.take();
        }
    }
    if tomada.is_some() {
        // La suscripción se paró (o el cliente se fue) mientras se abría la
        // conexión: no se abre ningún canal.
        if let Some(tomada) = tomada {
            tomada.soltar().await;
        }
        return;
    }
    if cancelar.is_cancelled() {
        // PararLog llegó mientras se tomaba la conexión; la entrada ya la
        // quitó quien canceló (y soltó su conexión).
        return;
    }
    let mut eventos = lanzar_canal(handle, comando, sudo, None, Some(cancelar.clone()), false);
    let mut cola = ColaSuscripcion::default();
    let mut pendiente = Vec::new();
    let mut errores = Vec::new();
    let mut motivo = MotivoFinLog::Terminado;
    let mut intervalo = tokio::time::interval(RITMO_LOGS);
    intervalo.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancelar.cancelled() => {
                // PararLog (o el cliente, o el apagado) ya lo ha contado.
                return;
            }
            _ = intervalo.tick() => {
                mandar_lote(&estado, cliente_id, peticion_id, &mut cola).await;
            }
            evento = eventos.recv() => match evento {
                None => break,
                Some(EventoCanal::Stdout(datos)) => {
                    acumular_lineas(&mut pendiente, &datos, |linea| {
                        if let Some(linea) = linea_de_log(&objetivo, linea) {
                            cola.anadir(linea);
                        }
                    });
                }
                Some(EventoCanal::Stderr(datos)) => {
                    if errores.len() < TOPE_ERRORES {
                        errores.extend_from_slice(&datos);
                    }
                }
                Some(EventoCanal::Fin(desenlace)) => {
                    motivo = motivo_de_fin(&desenlace, &errores);
                    break;
                }
            },
        }
    }
    // Lo que quedara en el búfer y en la cola sale en el último lote.
    if !pendiente.is_empty() {
        let texto = String::from_utf8_lossy(&pendiente).into_owned();
        if let Some(linea) = linea_de_log(&objetivo, &texto) {
            cola.anadir(linea);
        }
    }
    mandar_lote(&estado, cliente_id, peticion_id, &mut cola).await;
    terminar_log(&estado, cliente_id, peticion_id, Some(motivo)).await;
}

/// ¿El socket del solicitante admite más mensajes? (contrapresión, §7.6)
async fn admite_mas(estado: &Arc<Mutex<EstadoServidor>>, cliente_id: u32) -> bool {
    let estado_bloqueado = estado.lock().await;
    estado_bloqueado
        .clientes
        .get(&cliente_id)
        .is_some_and(|cliente| cliente.admite_mas())
}

/// Manda el lote pendiente de una suscripción, si lo hay y el socket admite
/// más: si no, el lote espera y el host sigue llenando la cola (que descarta
/// las líneas más viejas por encima de 4 MiB).
async fn mandar_lote(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    cola: &mut ColaSuscripcion,
) {
    if cola.vacia() || !admite_mas(estado, cliente_id).await {
        return;
    }
    let (lineas, omitidas) = cola.lote(TOPE_LOTE_CODIFICADO);
    responder(
        estado,
        cliente_id,
        MensajeServidor::LineasLog {
            peticion_id,
            lineas,
            omitidas,
        },
    )
    .await;
}

/// Motivo del fin de un seguimiento (§5.2).
fn motivo_de_fin(desenlace: &DesenlaceCanal, errores: &[u8]) -> MotivoFinLog {
    let errores = String::from_utf8_lossy(errores);
    if errores.contains("sin_permiso") || errores.contains("Permission denied") {
        return MotivoFinLog::SinPermiso;
    }
    if errores.contains("ERROR objetivo") {
        return MotivoFinLog::NoEncontrado;
    }
    if desenlace.cancelado {
        return MotivoFinLog::Cerrado;
    }
    if desenlace.fallo.is_some() {
        return MotivoFinLog::ConexionCaida;
    }
    MotivoFinLog::Terminado
}

/// Quita una suscripción y manda `FinLog` (si el panel sigue abierto).
/// Suelta la conexión que la sostenía.
async fn terminar_log(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    motivo: Option<MotivoFinLog>,
) {
    let suscripcion = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .servicios
            .logs
            .remove(&(cliente_id, peticion_id))
    };
    let Some(suscripcion) = suscripcion else {
        return;
    };
    if let Some(motivo) = motivo {
        responder(
            estado,
            suscripcion.solicitante,
            MensajeServidor::FinLog {
                peticion_id,
                motivo,
            },
        )
        .await;
    }
    soltar_suscripcion(suscripcion).await;
}

/// `PararLog`: cierra el canal y contesta con el fin de la suscripción.
async fn parar_log(estado: &Arc<Mutex<EstadoServidor>>, cliente_id: u32, peticion_id: u64) {
    let suscripcion = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .servicios
            .logs
            .remove(&(cliente_id, peticion_id))
    };
    let Some(suscripcion) = suscripcion else {
        return;
    };
    suscripcion.cancelar.cancel();
    responder(
        estado,
        suscripcion.solicitante,
        MensajeServidor::FinLog {
            peticion_id,
            motivo: MotivoFinLog::Cerrado,
        },
    )
    .await;
    soltar_suscripcion(suscripcion).await;
}

// ---------------------------------------------------------------- pruebas

#[cfg(test)]
mod pruebas {
    use super::*;

    fn linea(texto: &str) -> LineaLog {
        LineaLog {
            hora: Some(1_700_000_000),
            prioridad: 6,
            stderr: false,
            texto: texto.to_string(),
        }
    }

    #[test]
    fn el_rango_de_servicios_no_se_solapa_con_el_de_escritorios() {
        assert!(peticion_del_rango(crate::protocolo::RANGO_SERVICIOS));
        assert!(peticion_del_rango(crate::protocolo::RANGO_SERVICIOS + 7));
        assert!(!peticion_del_rango(crate::protocolo::RANGO_ESCRITORIOS));
        assert!(!peticion_del_rango(0));
        assert!(!peticion_del_rango(1 << 48));
    }

    #[test]
    fn la_cola_de_logs_descarta_las_mas_viejas_con_recuento() {
        let mut cola = ColaSuscripcion::default();
        // Cada línea de 1 MiB: a la quinta se pasa del tope de 4 MiB.
        let grande = "x".repeat(1024 * 1024);
        for _ in 0..5 {
            cola.anadir(linea(&grande));
        }
        assert!(cola.bytes <= COLA_LOGS, "la cola no pasa de 4 MiB");
        assert!(cola.omitidas > 0, "se cuentan las descartadas");
        assert_eq!(cola.lineas.len(), 4);
    }

    #[test]
    fn el_lote_saca_como_mucho_500_lineas_y_resetea_las_omitidas() {
        let mut cola = ColaSuscripcion::default();
        for numero in 0..700 {
            cola.anadir(linea(&numero.to_string()));
        }
        let (lote, omitidas) = cola.lote(TOPE_LOTE_CODIFICADO);
        assert_eq!(lote.len(), LOTE_LOGS);
        assert_eq!(omitidas, 0);
        assert_eq!(cola.lineas.len(), 200);
        let (lote, _) = cola.lote(TOPE_LOTE_CODIFICADO);
        assert_eq!(lote.len(), 200);
        assert!(cola.vacia());
    }

    #[test]
    fn acumular_lineas_entrega_solo_lineas_completas() {
        let mut pendiente = Vec::new();
        let mut vistas = Vec::new();
        acumular_lineas(&mut pendiente, b"MAGI_FASE=compro", |linea| {
            vistas.push(linea.to_string())
        });
        assert!(vistas.is_empty());
        acumular_lineas(&mut pendiente, b"bando\nMAGI_FASE=ejecu", |linea| {
            vistas.push(linea.to_string())
        });
        assert_eq!(vistas, vec!["MAGI_FASE=comprobando"]);
        acumular_lineas(&mut pendiente, b"tando\n", |linea| {
            vistas.push(linea.to_string())
        });
        assert_eq!(vistas[1], "MAGI_FASE=ejecutando");
        assert!(pendiente.is_empty());
    }

    #[test]
    fn las_acciones_solo_valen_en_su_tipo_de_objetivo() {
        assert!(accion_valida(TipoObjetivo::Timer, Accion::Lanzar));
        assert!(!accion_valida(TipoObjetivo::Timer, Accion::Parar));
        assert!(accion_valida(TipoObjetivo::Unidad, Accion::Recargar));
        assert!(!accion_valida(TipoObjetivo::Unidad, Accion::Lanzar));
        assert!(accion_valida(TipoObjetivo::Contenedor, Accion::Parar));
        assert!(!accion_valida(TipoObjetivo::Contenedor, Accion::Recargar));
        assert!(accion_valida(TipoObjetivo::Proyecto, Accion::Reiniciar));
    }

    #[test]
    fn el_tope_de_una_salida_corta_y_marca_truncada() {
        let mut salida = SalidaCanal::default();
        salida.anadir(b"1234567890", true, 6);
        assert_eq!(salida.stdout, b"123456");
        assert!(salida.truncada);
        salida.anadir(b"mas", true, 6);
        assert_eq!(salida.stdout, b"123456");
    }

    #[test]
    fn los_errores_del_script_se_traducen_a_mensajes() {
        let mut salida = SalidaCanal::default();
        salida.anadir(b"ERROR objetivo\n", false, TOPE_ACCION);
        assert!(mensaje_de_error(&salida).contains("no es válido"));
        let mut salida = SalidaCanal::default();
        salida.anadir(b"ERROR sin_privilegio\n", false, TOPE_ACCION);
        assert!(mensaje_de_error(&salida).contains("sin privilegios"));
        let mut salida = SalidaCanal::default();
        salida.anadir(b"ERROR sin_permiso\n", false, TOPE_ACCION);
        assert!(mensaje_de_error(&salida).contains("sin permiso"));
        // Sin ERROR, el código de salida.
        let salida = SalidaCanal {
            codigo: Some(3),
            ..SalidaCanal::default()
        };
        assert!(mensaje_de_error(&salida).contains("código 3"));
        // Una señal manda sobre el código.
        let salida = SalidaCanal {
            codigo: Some(255),
            senal: Some("SIGTERM".to_string()),
            ..SalidaCanal::default()
        };
        assert!(mensaje_de_error(&salida).contains("SIGTERM"));
    }

    #[test]
    fn una_accion_terminada_se_lee_con_su_estado() {
        let objetivo = Objetivo::unidad("app.service");
        let suborden = nucleo::orden::Suborden::Accion {
            objetivo: objetivo.clone(),
            accion: Accion::Reiniciar,
            espera_seg: 30,
            comprobacion: nucleo::orden::Comprobacion::Ninguna,
        };
        let mut salida = SalidaCanal::default();
        salida.anadir(
            b"MAGI_FASE=ejecutando\nMAGI_FASE=esperando\nMAGI_RESULTADO=hecho active 2.3\nMAGI_LINEA arrancado\nMAGI_FIN\n",
            true,
            TOPE_ACCION,
        );
        let resultado = resultado_de_salida(&salida, &suborden).expect("resultado");
        assert_eq!(resultado.resultado, nucleo::ResultadoAccion::Hecho);
        assert_eq!(resultado.estado, "active");
        assert_eq!(resultado.lineas, vec!["arrancado".to_string()]);
        assert!(nucleo::lectura::fase("MAGI_FASE=comprobando").is_some());
    }

    #[test]
    fn una_accion_sin_resultado_es_un_error() {
        let objetivo = Objetivo::unidad("app.service");
        let suborden = nucleo::orden::Suborden::Accion {
            objetivo,
            accion: Accion::Parar,
            espera_seg: 30,
            comprobacion: nucleo::orden::Comprobacion::Ninguna,
        };
        let mut salida = SalidaCanal::default();
        salida.anadir(b"basura\n", true, TOPE_ACCION);
        salida.codigo = Some(1);
        let resultado = resultado_de_salida(&salida, &suborden).expect("resultado");
        assert_eq!(resultado.resultado, nucleo::ResultadoAccion::Error);
    }

    #[test]
    fn el_fin_de_un_log_se_clasifica() {
        let mut desenlace = DesenlaceCanal::default();
        assert_eq!(motivo_de_fin(&desenlace, b""), MotivoFinLog::Terminado);
        assert_eq!(
            motivo_de_fin(&desenlace, b"ERROR sin_permiso\n"),
            MotivoFinLog::SinPermiso
        );
        assert_eq!(
            motivo_de_fin(&desenlace, b"ERROR objetivo\n"),
            MotivoFinLog::NoEncontrado
        );
        desenlace.cancelado = true;
        assert_eq!(motivo_de_fin(&desenlace, b""), MotivoFinLog::Cerrado);
        let desenlace = DesenlaceCanal {
            fallo: Some("se cayó".to_string()),
            ..DesenlaceCanal::default()
        };
        assert_eq!(motivo_de_fin(&desenlace, b""), MotivoFinLog::ConexionCaida);
    }

    /// B2: si la salida se cortó y no hay MAGI_RESULTADO, el mensaje lo dice.
    #[test]
    fn una_salida_truncada_sin_resultado_lo_dice() {
        let objetivo = Objetivo::unidad("app.service");
        let suborden = nucleo::orden::Suborden::Accion {
            objetivo,
            accion: Accion::Reiniciar,
            espera_seg: 30,
            comprobacion: nucleo::orden::Comprobacion::Ninguna,
        };
        let mut salida = SalidaCanal::default();
        salida.anadir(
            b"MAGI_FASE=ejecutando
",
            true,
            16,
        );
        salida.truncada = true;
        let resultado = resultado_de_salida(&salida, &suborden).expect("resultado");
        assert_eq!(resultado.resultado, nucleo::ResultadoAccion::Error);
        assert!(
            resultado.mensaje.contains("pasó de"),
            "{}",
            resultado.mensaje
        );
    }

    /// M4: el lote se corta por tamaño codificado, también con comillas.
    #[test]
    fn el_lote_no_pasa_de_linea_maxima_codificado() {
        let mut cola = ColaSuscripcion::default();
        // Cada línea son 8 KiB de comillas: el JSON las escapa al doble.
        let comillas = "\"".repeat(TOPE_LINEA / 2);
        for _ in 0..500 {
            cola.anadir(linea(&comillas));
        }
        assert!(cola.bytes > TOPE_LOTE_CODIFICADO, "hay cola de sobra");
        let (lote, _) = cola.lote(TOPE_LOTE_CODIFICADO);
        assert!(!lote.is_empty(), "siempre sale al menos una línea");
        assert!(lote.len() <= LOTE_LOGS);
        let mensaje = crate::protocolo::MensajeServidor::LineasLog {
            peticion_id: 1,
            lineas: lote,
            omitidas: 0,
        };
        let codificado = crate::protocolo::codificar(&mensaje).expect("codifica");
        assert!(
            codificado.len() < crate::protocolo::LINEA_MAXIMA,
            "el lote codificado pasa de LINEA_MAXIMA: {}",
            codificado.len()
        );
        assert!(
            codificado.len() <= TOPE_LOTE_CODIFICADO + 128 * 1024,
            "el lote respeta el presupuesto con margen"
        );
    }

    /// B7: una deliberación terminada pero aún sin pasar la Barrera sigue
    /// contando como usada.
    #[test]
    fn una_deliberacion_en_vuelo_cuenta_como_usada() {
        let mut servicios = Servicios::default();
        assert!(!servicios.deliberacion_usada(9));
        servicios.deliberaciones_en_vuelo.insert(9);
        assert!(servicios.deliberacion_usada(9));
        servicios.deliberaciones_en_vuelo.remove(&9);
        assert!(!servicios.deliberacion_usada(9));
    }
}
