//! Túneles del servidor de sesiones: locales (`TcpListener` + `direct-tcpip`),
//! remotos (`tcpip_forward` + `forwarded-tcpip`) y dinámicos (SOCKS5), todos
//! sobre la conexión del pool del host.
//!
//! El CRUD de `TUNELES` lo hace el cliente; aquí solo se lee la tabla y se
//! ejecuta. Un túnel activo cuenta como canal del pool (mantiene viva la
//! conexión) pero **no** como pestaña ni SFTP para el ciclo automático, que se
//! levanta con el primer canal de pestaña o SFTP del host y se para con el
//! último.
//!
//! La difusión va por donde manda el checklist: cada cambio de estado sale de
//! inmediato y los contadores, como mucho dos veces por segundo y solo si
//! cambian (la revisora compara con lo último enviado).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use russh::client::Handle;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch, Mutex};
use tracing::{info, warn};

use crate::conexion::cliente::Cliente;
use crate::conexion::reenvios::{Contadores, Reenvio, Reenvios};
use crate::modelo::{Host, ResultadoRegistro, TipoTunel, Tunel};
use crate::protocolo::{EstadoTunelRemoto, InfoTunel, MensajeServidor, OrigenTunel};
use crate::registro;

use super::conexiones::{self, ConexionTomada, PoliticaConexion, UsoCanal};
use super::{difusion, sesiones, EstadoServidor};

/// Cada cuánto se revisan las conexiones caídas y se refrescan los contadores.
pub const REVISION: Duration = Duration::from_millis(500);

/// Plazo de las operaciones que hablan con el host (pedir y cancelar reenvíos).
const PLAZO: Duration = Duration::from_secs(10);

/// Un túnel levantado, con lo que hace falta para pararlo.
pub struct TunelActivo {
    pub tunel_id: i64,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    pub tipo: TipoTunel,
    /// La escucha configurada en `TUNELES`.
    pub escucha: String,
    /// La que está escuchando de verdad (puerto 0, o el que confirma el host).
    pub escucha_efectiva: String,
    pub destino: Option<String>,
    pub automatico: bool,
    pub estado: EstadoTunelRemoto,
    pub origen: OrigenTunel,
    /// Ventana que lo pidió; 0 si lo levantó el ciclo automático.
    pub solicitante: u32,
    pub desde: i64,
    pub ultimo_error: Option<String>,
    pub contadores: Arc<Contadores>,
    /// Canal contado en la conexión del pool: se suelta por identidad.
    conexion: Option<ConexionTomada>,
    /// Manda parar al bucle que acepta conexiones (local y dinámico).
    parada: Option<oneshot::Sender<()>>,
    /// Avisa a las copias en curso de que el túnel se para: hay que cortarlas.
    cancelacion: watch::Sender<bool>,
    /// Reenvío remoto registrado en la conexión: la clave que confirmó el host.
    reenvio: Option<(String, u32)>,
}

impl TunelActivo {
    /// Lo que se difunde de este túnel.
    fn info(&self) -> InfoTunel {
        let (conexiones, aceptadas, subidos, bajados) = self.contadores.instantanea();
        InfoTunel {
            tunel_id: self.tunel_id,
            host_id: self.host_id,
            host_nombre: self.host_nombre.clone(),
            nombre: self.nombre.clone(),
            tipo: self.tipo.como_texto().to_string(),
            escucha: self.escucha.clone(),
            escucha_efectiva: self.escucha_efectiva.clone(),
            destino: self.destino.clone(),
            automatico: self.automatico,
            estado: self.estado,
            origen: Some(self.origen),
            solicitante: Some(self.solicitante),
            conexiones,
            aceptadas,
            bytes_subidos: subidos,
            bytes_bajados: bajados,
            desde: Some(self.desde),
            ultimo_error: self.ultimo_error.clone(),
        }
    }
}

/// Lo que se difunde de los túneles: solo los que el servidor tiene en memoria
/// (activos, activando, parando y caídos). Las filas de `TUNELES` sin nadie
/// detrás las conoce el cliente por su propia lectura de la tabla.
pub fn lista(estado: &EstadoServidor) -> Vec<InfoTunel> {
    let mut lista: Vec<InfoTunel> = estado.tuneles.values().map(TunelActivo::info).collect();
    lista.sort_by(|a, b| {
        a.host_nombre
            .cmp(&b.host_nombre)
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    lista
}

/// Envía la lista ahora mismo. Se usa en los cambios de estado, que no esperan
/// al ritmo de los contadores.
pub async fn difundir(estado: &Arc<Mutex<EstadoServidor>>) {
    let estado_bloqueado = estado.lock().await;
    if estado_bloqueado.clientes.is_empty() {
        return;
    }
    let lista = lista(&estado_bloqueado);
    difusion::difundir(
        &estado_bloqueado.clientes,
        MensajeServidor::Tuneles { lista },
    );
}

/// ¿Hay algún túnel activo o levantándose? Un túnel vivo impide que el
/// servidor se apague por inactividad.
pub fn hay_activos(estado: &EstadoServidor) -> bool {
    estado
        .tuneles
        .values()
        .any(|activo| activo.estado.en_marcha())
}

// ---------------------------------------------------------------- activar

/// Levanta un túnel. Si ya estaba en marcha se ignora (el usuario lo pidió dos
/// veces); si estaba caído, esto es un relanzamiento y los contadores vuelven a
/// cero. Sin solicitante (el ciclo automático) la conexión no dialoga: si
/// necesitara credenciales, el túnel queda caído con el motivo.
pub async fn activar(
    estado: &Arc<Mutex<EstadoServidor>>,
    tunel_id: i64,
    origen: OrigenTunel,
    solicitante: Option<u32>,
) -> Result<(), String> {
    let id_ventana = solicitante.unwrap_or(0);
    {
        let estado_bloqueado = estado.lock().await;
        if let Some(activo) = estado_bloqueado.tuneles.get(&tunel_id) {
            if activo.estado.en_marcha() {
                return Ok(());
            }
        }
    }

    // El almacén se lee **sin** el mutex del estado en la mano: es E/S de
    // SQLite y podría tardar (hasta el `busy_timeout`), y dejaría parado a todo
    // el servidor (mensajes, revisora, difusión) mientras tanto.
    let lectura = estado.lock().await.lectura.clone();
    let (tunel, host, todos_los_hosts) = {
        let lectura = lectura.lock().map_err(|_| "almacén envenenado")?;
        let tunel = lectura
            .obtener_tunel(tunel_id)
            .map_err(|_| "ese túnel ya no existe".to_string())?;
        let host = lectura
            .obtener_host(tunel.host_id)
            .map_err(|_| "el host del túnel ya no existe".to_string())?;
        let todos_los_hosts: HashMap<i64, Host> = lectura
            .listar_hosts()
            .unwrap_or_default()
            .into_iter()
            .map(|host| (host.id, host))
            .collect();
        (tunel, host, todos_los_hosts)
    };
    let (rutas, id_solicitud) = {
        let mut estado_bloqueado = estado.lock().await;
        let rutas = estado_bloqueado.rutas.clone();
        // El id de solicitud sale del mismo contador que las sesiones: sirve
        // para los diálogos de conexión y nunca choca con un id de sesión.
        let id_solicitud = estado_bloqueado.siguiente_sesion_id;
        estado_bloqueado.siguiente_sesion_id += 1;
        (rutas, id_solicitud)
    };

    // El túnel queda en «activando» desde ya: la vista lo enseña y nadie lo
    // levanta dos veces mientras se abre la conexión. La comprobación y la
    // reserva van en el mismo bloqueo: si no, dos pulsaciones rápidas del
    // usuario colarían dos levantamientos del mismo túnel.
    // Los contadores son nuevos en cada activación: sirven también de
    // identidad para saber, al terminar, si la entrada sigue siendo la suya.
    let contadores = Arc::new(Contadores::default());
    let identidad = contadores.clone();
    let (cancelacion, aviso) = watch::channel(false);
    {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            return Err(conexiones::APAGANDO.to_string());
        }
        if let Some(activo) = estado_bloqueado.tuneles.get(&tunel_id) {
            if activo.estado.en_marcha() {
                return Ok(());
            }
        }
        // El ciclo automático solo levanta con el host en uso, comprobado en
        // el mismo bloqueo que la inserción: si la pestaña se cerró mientras
        // tanto, su `canales_cambiaron` ya no encontraría este túnel.
        if origen == OrigenTunel::Automatico && !hay_canales_con(&estado_bloqueado, tunel.host_id) {
            return Ok(());
        }
        estado_bloqueado.tuneles.insert(
            tunel_id,
            TunelActivo {
                tunel_id,
                host_id: tunel.host_id,
                host_nombre: host.nombre.clone(),
                nombre: tunel.nombre.clone(),
                tipo: tunel.tipo,
                escucha: tunel.escucha.clone(),
                escucha_efectiva: tunel.escucha.clone(),
                destino: tunel.destino.clone(),
                automatico: tunel.automatico,
                estado: EstadoTunelRemoto::Activando,
                origen,
                solicitante: id_ventana,
                desde: Utc::now().timestamp(),
                ultimo_error: None,
                contadores: contadores.clone(),
                conexion: None,
                parada: None,
                cancelacion,
                reenvio: None,
            },
        );
    }
    difundir(estado).await;

    // Dos túneles del mismo host que se levantan a la vez comparten una única
    // conexión: el cerrojo por host lo pone `conexion_para_canal`.
    let politica = match solicitante {
        Some(solicitante) => PoliticaConexion::Interactiva {
            solicitante,
            id_solicitud,
        },
        None => PoliticaConexion::NoInteractiva {
            solicitante: None,
            id_solicitud,
        },
    };
    let levantado = levantar(
        estado,
        &tunel,
        &host,
        &todos_los_hosts,
        &rutas,
        politica,
        contadores,
        aviso,
    )
    .await;

    match levantado {
        Ok(levantado) => {
            let mut estado_bloqueado = estado.lock().await;
            let bd = estado_bloqueado.bd.clone();
            // La entrada tiene que seguir siendo la activación que empezamos:
            // si mientras abría se borró el túnel, se canceló por irse la
            // ventana que lo pidió o se marcó caído, lo levantado no lo quiere
            // nadie y se suelta.
            let sigue = estado_bloqueado
                .tuneles
                .get(&tunel_id)
                .is_some_and(|activo| {
                    activo.estado == EstadoTunelRemoto::Activando
                        && Arc::ptr_eq(&activo.contadores, &identidad)
                })
                && (origen != OrigenTunel::Automatico
                    || hay_canales_con(&estado_bloqueado, tunel.host_id));
            if !sigue {
                drop(estado_bloqueado);
                soltar_levantado(estado, tunel.host_id, levantado).await;
                return Ok(());
            }
            let Some(activo) = estado_bloqueado.tuneles.get_mut(&tunel_id) else {
                drop(estado_bloqueado);
                soltar_levantado(estado, tunel.host_id, levantado).await;
                return Ok(());
            };
            activo.escucha_efectiva = levantado.escucha_efectiva.clone();
            activo.estado = EstadoTunelRemoto::Activo;
            activo.ultimo_error = None;
            activo.desde = Utc::now().timestamp();
            activo.conexion = Some(levantado.conexion);
            activo.parada = levantado.parada;
            activo.reenvio = levantado.reenvio;
            let _ = bd.send(super::OrdenBd::Anotar {
                tipo: registro::TUNEL_ABIERTO.to_string(),
                host_id: Some(tunel.host_id),
                identidad_id: None,
                detalle: format!(
                    "{} · {} · {} → {}",
                    tunel.nombre,
                    tunel.tipo.etiqueta(),
                    activo.escucha_efectiva,
                    tunel.destino.as_deref().unwrap_or("(socks5)")
                ),
                resultado: ResultadoRegistro::Ok,
            });
            info!(
                host = %host.nombre,
                tunel = %tunel.nombre,
                escucha = %activo.escucha_efectiva,
                "túnel activo"
            );
        }
        Err(motivo) => {
            // Solo si la entrada sigue siendo esta activación: una parada, un
            // relanzamiento o el ciclo automático pueden haberla sustituido.
            let es_la_suya = estado
                .lock()
                .await
                .tuneles
                .get(&tunel_id)
                .is_some_and(|activo| Arc::ptr_eq(&activo.contadores, &identidad));
            if es_la_suya {
                caido(estado, tunel_id, &motivo, true).await;
                difundir(estado).await;
            }
            return Err(motivo);
        }
    }
    difundir(estado).await;
    Ok(())
}

/// Lo que deja levantado un túnel: con qué conexión, escuchando dónde y cómo
/// pararlo.
struct Levantado {
    conexion: ConexionTomada,
    escucha_efectiva: String,
    parada: Option<oneshot::Sender<()>>,
    reenvio: Option<(String, u32)>,
}

/// Abre la conexión del host y levanta el túnel según su tipo.
#[allow(clippy::too_many_arguments)]
async fn levantar(
    estado: &Arc<Mutex<EstadoServidor>>,
    tunel: &Tunel,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &crate::config::Rutas,
    politica: PoliticaConexion,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) -> Result<Levantado, String> {
    // Si el túnel se para mientras espera el cerrojo o la conexión, la
    // apertura se abandona: no quedan diálogos para un túnel que ya no existe.
    let mut parado = aviso.clone();
    let conexion = tokio::select! {
        conexion = conexiones::conexion_para_canal(
            estado,
            host,
            todos_los_hosts,
            rutas,
            politica,
            UsoCanal::Subsistema,
        ) => conexion?,
        _ = parado.wait_for(|parar| *parar) => {
            return Err("el túnel se paró mientras se levantaba".to_string());
        }
    };
    let handle = conexion.handle.clone();

    let resultado = levantar_con(tunel, handle, contadores, estado, aviso).await;
    match resultado {
        Ok((escucha_efectiva, parada, reenvio)) => Ok(Levantado {
            conexion,
            escucha_efectiva,
            parada,
            reenvio,
        }),
        Err(motivo) => {
            conexion.soltar().await;
            Err(motivo)
        }
    }
}

type Senal = Option<oneshot::Sender<()>>;

/// Levanta el túnel sobre una conexión ya tomada.
async fn levantar_con(
    tunel: &Tunel,
    handle: Arc<Handle<Cliente>>,
    contadores: Arc<Contadores>,
    estado: &Arc<Mutex<EstadoServidor>>,
    aviso: watch::Receiver<bool>,
) -> Result<(String, Senal, Option<(String, u32)>), String> {
    let (direccion, puerto) = partir_escucha(&tunel.escucha)?;
    // La fila la escribe el cliente; el servidor la ejecuta, así que comprueba
    // aquí lo que de verdad importa: sin destino, un local o un remoto dejarían
    // un agujero que acepta conexiones y no las lleva a ninguna parte.
    let destino = match (tunel.tipo, tunel.destino.as_deref()) {
        (TipoTunel::Dinamico, _) => None,
        (_, Some(destino)) if crate::modelo::partir_direccion_puerto(destino).is_some() => {
            Some(destino.to_string())
        }
        (_, Some(destino)) => {
            return Err(format!("el destino «{destino}» no es válido"));
        }
        (tipo, None) => {
            return Err(format!(
                "un túnel {} necesita destino y la fila no lo tiene",
                tipo.etiqueta()
            ));
        }
    };
    match tunel.tipo {
        TipoTunel::Local => {
            let destino = destino.unwrap_or_default();
            let listener = TcpListener::bind((direccion.as_str(), puerto))
                .await
                .map_err(|error| bind_fallido(&tunel.escucha, &error))?;
            let efectiva = escucha_de(&listener, &direccion)?;
            let parada = servir_local(listener, handle, destino, contadores, aviso);
            Ok((efectiva, Some(parada), None))
        }
        TipoTunel::Dinamico => {
            let listener = TcpListener::bind((direccion.as_str(), puerto))
                .await
                .map_err(|error| bind_fallido(&tunel.escucha, &error))?;
            let efectiva = escucha_de(&listener, &direccion)?;
            let parada = servir_socks(listener, handle, contadores, aviso);
            Ok((efectiva, Some(parada), None))
        }
        TipoTunel::Remoto => {
            let respuesta = tokio::time::timeout(
                PLAZO,
                handle.tcpip_forward(direccion.clone(), u32::from(puerto)),
            )
            .await
            .map_err(|_| "se agotó el plazo pidiendo el reenvío".to_string())
            .and_then(|resultado| {
                resultado.map_err(|error| match error {
                    russh::Error::RequestDenied => {
                        format!("el host rechazó el reenvío de {}", tunel.escucha)
                    }
                    otro => format!("no se pudo pedir el reenvío: {otro}"),
                })
            })?;
            // El host solo contesta con el puerto cuando se le pidió el 0; si
            // se le pidió uno concreto, la respuesta viene vacía y russh la
            // traduce a 0. Quedarse con ese 0 dejaría el reenvío registrado en
            // un puerto que no existe: los canales del host se rechazarían y la
            // escucha no se podría cancelar.
            let efectivo = if puerto == 0 {
                respuesta
            } else {
                u32::from(puerto)
            };
            let reenvios: Arc<Reenvios> = estado.lock().await.reenvios.clone();
            reenvios.registrar(
                tunel.host_id,
                &direccion,
                efectivo,
                Arc::new(Reenvio {
                    tunel_id: tunel.id,
                    destino: destino.unwrap_or_default(),
                    contadores,
                    cancelacion: aviso,
                }),
            );
            Ok((
                direccion_y_puerto(&direccion, efectivo),
                None,
                Some((direccion, efectivo)),
            ))
        }
    }
}

/// Un túnel que se estaba levantando se queda sin quien conteste a los
/// diálogos: pasa a caído en vez de esperar al plazo.
pub async fn caido_por_solicitante(
    estado: &Arc<Mutex<EstadoServidor>>,
    tunel_id: i64,
    motivo: &str,
) {
    caido(estado, tunel_id, motivo, false).await;
    difundir(estado).await;
}

/// Deja el túnel en «caído» con el motivo, lo anota y suelta lo que sostuviera.
/// `fallido` distingue no haber llegado a levantarse de caerse después.
async fn caido(estado: &Arc<Mutex<EstadoServidor>>, tunel_id: i64, motivo: &str, fallido: bool) {
    let (host_id, nombre, conexion, reenvio) = {
        let mut estado_bloqueado = estado.lock().await;
        let bd = estado_bloqueado.bd.clone();
        let Some(activo) = estado_bloqueado.tuneles.get_mut(&tunel_id) else {
            return;
        };
        activo.estado = EstadoTunelRemoto::Caido;
        activo.ultimo_error = Some(motivo.to_string());
        activo.parada = None;
        // Las conexiones abiertas de un túnel caído no tienen a quién servir.
        let _ = activo.cancelacion.send(true);
        let conexion = activo.conexion.take();
        let reenvio = activo.reenvio.take();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: if fallido {
                registro::TUNEL_FALLIDO.to_string()
            } else {
                registro::TUNEL_CERRADO.to_string()
            },
            host_id: Some(activo.host_id),
            identidad_id: None,
            detalle: format!("{}: {motivo}", activo.nombre),
            resultado: ResultadoRegistro::Error,
        });
        (activo.host_id, activo.nombre.clone(), conexion, reenvio)
    };
    warn!(tunel = %nombre, "túnel caído: {motivo}");
    // El reenvío del host deja de estar registrado: sus canales ya no se
    // aceptan, y si la conexión sigue viva se cancela la escucha remota.
    if let Some((direccion, puerto)) = reenvio {
        estado
            .lock()
            .await
            .reenvios
            .quitar(host_id, &direccion, puerto);
        if let Some(conexion) = &conexion {
            cancelar_reenvio(&conexion.handle, &direccion, puerto).await;
        }
    }
    if let Some(conexion) = conexion {
        conexion.soltar().await;
    }
}

/// Cancela la escucha remota del host. Con plazo: el host puede estar en un
/// agujero negro y esto no puede colgar al que lo llama.
async fn cancelar_reenvio(handle: &Arc<Handle<Cliente>>, direccion: &str, puerto: u32) {
    let cancelacion = tokio::time::timeout(
        PLAZO,
        handle.cancel_tcpip_forward(direccion.to_string(), puerto),
    )
    .await;
    match cancelacion {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(puerto, "no se pudo cancelar el reenvío: {error}"),
        Err(_) => warn!(puerto, "el host no contestó a la cancelación del reenvío"),
    }
}

/// Suelta lo que dejó levantado una activación que ya no quiere nadie: el
/// listener se cierra solo al soltar su señal (el `select!` de `servir_*` cae
/// por ese lado), aquí quedan el reenvío remoto y la conexión.
async fn soltar_levantado(
    estado: &Arc<Mutex<EstadoServidor>>,
    host_id: i64,
    mut levantado: Levantado,
) {
    if let Some((direccion, puerto)) = levantado.reenvio.take() {
        estado
            .lock()
            .await
            .reenvios
            .quitar(host_id, &direccion, puerto);
        cancelar_reenvio(&levantado.conexion.handle, &direccion, puerto).await;
    }
    levantado.conexion.soltar().await;
}

fn partir_escucha(escucha: &str) -> Result<(String, u16), String> {
    crate::modelo::partir_direccion_puerto(escucha)
        .ok_or_else(|| format!("la escucha «{escucha}» no es válida"))
}

fn escucha_de(listener: &TcpListener, direccion: &str) -> Result<String, String> {
    let puerto = listener
        .local_addr()
        .map(|direccion| direccion.port())
        .map_err(|error| format!("no se pudo leer el puerto: {error}"))?;
    Ok(direccion_y_puerto(direccion, u32::from(puerto)))
}

fn direccion_y_puerto(direccion: &str, puerto: u32) -> String {
    crate::modelo::juntar_direccion_puerto(direccion, puerto.min(u32::from(u16::MAX)) as u16)
}

/// Motivo legible para un bind que falla, con lo que el usuario puede hacer.
fn bind_fallido(escucha: &str, error: &std::io::Error) -> String {
    let puerto = crate::modelo::partir_direccion_puerto(escucha)
        .map(|(_, puerto)| puerto)
        .unwrap_or_default();
    match error.kind() {
        std::io::ErrorKind::AddrInUse => {
            format!("el puerto {puerto} ya está en uso en esta máquina")
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("no hay permiso para escuchar en el puerto {puerto}")
        }
        _ => format!("no se pudo escuchar en {escucha}: {error}"),
    }
}

// ---------------------------------------------------------------- servir

/// Acepta conexiones locales y abre un canal `direct-tcpip` por cada una hacia
/// el destino. La señal de parada cierra el listener.
fn servir_local(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    destino: String,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) -> oneshot::Sender<()> {
    let (parada, senal) = oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            _ = aceptar_local(listener, handle, destino, contadores, aviso) => {}
            _ = senal => {}
        }
    });
    parada
}

async fn aceptar_local(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    destino: String,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) {
    let Some((host_destino, puerto_destino)) = crate::modelo::partir_direccion_puerto(&destino)
    else {
        return;
    };
    loop {
        let Ok((flujo, origen)) = listener.accept().await else {
            return;
        };
        let handle = handle.clone();
        let destino = host_destino.clone();
        let contadores = contadores.clone();
        let mut aviso = aviso.clone();
        tokio::spawn(async move {
            let _ = flujo.set_nodelay(true);
            let apertura = handle.clone();
            let (hacia, ip_origen) = (destino.clone(), origen.ip().to_string());
            let canal = conexiones::abrir_con_plazo(PLAZO, async move {
                apertura
                    .channel_open_direct_tcpip(
                        hacia,
                        u32::from(puerto_destino),
                        ip_origen,
                        u32::from(origen.port()),
                    )
                    .await
            })
            .await;
            let canal = match canal {
                Ok(canal) => canal,
                Err(error) => {
                    // El host no quiere abrir ese destino: cae solo esta
                    // conexión, el túnel sigue vivo.
                    contadores.anotar_error(format!("el host rechazó {destino}: {error}"));
                    warn!(destino = %destino, "el host rechazó el canal: {error}");
                    return;
                }
            };
            contadores.abrir();
            let mut flujo = flujo;
            let mut canal = canal.into_stream();
            crate::conexion::reenvios::copiar_hasta_que_paren(
                &mut flujo,
                &mut canal,
                &contadores,
                &mut aviso,
            )
            .await;
            contadores.cerrar();
        });
    }
}

/// Igual que el local, pero hablando SOCKS5 con quien se conecta.
fn servir_socks(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) -> oneshot::Sender<()> {
    let (parada, senal) = oneshot::channel();
    tokio::spawn(async move {
        tokio::select! {
            _ = aceptar_socks(listener, handle, contadores, aviso) => {}
            _ = senal => {}
        }
    });
    parada
}

async fn aceptar_socks(
    listener: TcpListener,
    handle: Arc<Handle<Cliente>>,
    contadores: Arc<Contadores>,
    aviso: watch::Receiver<bool>,
) {
    loop {
        let Ok((mut flujo, origen)) = listener.accept().await else {
            return;
        };
        let handle = handle.clone();
        let contadores = contadores.clone();
        let mut aviso = aviso.clone();
        tokio::spawn(async move {
            let _ = flujo.set_nodelay(true);
            let (destino, puerto) = match super::socks5::negociar(&mut flujo).await {
                super::socks5::Peticion::Conectar(destino, puerto) => (destino, u32::from(puerto)),
                super::socks5::Peticion::NoSoportada | super::socks5::Peticion::Invalida => return,
            };
            let apertura = handle.clone();
            let (hacia, ip_origen) = (destino.clone(), origen.ip().to_string());
            let canal = conexiones::abrir_con_plazo(PLAZO, async move {
                apertura
                    .channel_open_direct_tcpip(hacia, puerto, ip_origen, u32::from(origen.port()))
                    .await
            })
            .await;
            let canal = match canal {
                Ok(canal) => canal,
                Err(error) => {
                    contadores.anotar_error(format!("el host rechazó {destino}:{puerto}"));
                    let _ =
                        super::socks5::responder(&mut flujo, super::socks5::codigo_rechazo()).await;
                    warn!(destino = %destino, "el host rechazó el canal SOCKS: {error}");
                    return;
                }
            };
            if super::socks5::responder(&mut flujo, super::socks5::OK)
                .await
                .is_err()
            {
                return;
            }
            contadores.abrir();
            let mut canal = canal.into_stream();
            crate::conexion::reenvios::copiar_hasta_que_paren(
                &mut flujo,
                &mut canal,
                &contadores,
                &mut aviso,
            )
            .await;
            contadores.cerrar();
        });
    }
}

// ---------------------------------------------------------------- parar

/// Para un túnel: pasa por `parando`, cierra el listener o cancela el reenvío,
/// corta las conexiones abiertas, suelta la conexión del pool y anota
/// `tunel_cerrado` con los totales.
///
/// `a_mano` distingue la orden del usuario del ciclo automático: un automático
/// que el usuario para no vuelve hasta el siguiente ciclo (hasta que el host se
/// quede sin pestañas ni SFTP y vuelva a empezar).
pub async fn parar(
    estado: &Arc<Mutex<EstadoServidor>>,
    tunel_id: i64,
    motivo: &str,
    a_mano: bool,
) -> Result<(), String> {
    let activo = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(activo) = estado_bloqueado.tuneles.get_mut(&tunel_id) else {
            return Ok(());
        };
        activo.estado = EstadoTunelRemoto::Parando;
        let a_mano = a_mano && activo.origen == OrigenTunel::Automatico;
        let host_id = activo.host_id;
        match estado_bloqueado.tuneles.remove(&tunel_id) {
            Some(activo) => {
                if a_mano {
                    // Un automático parado a mano no vuelve hasta el ciclo
                    // siguiente.
                    estado_bloqueado.tuneles_parados.insert((host_id, tunel_id));
                }
                activo
            }
            None => return Ok(()),
        }
    };
    // Se enseña el «parando» antes de cerrar: el cierre puede tardar (cancelar
    // un reenvío, cortar conexiones) y el usuario ve que está en ello.
    difundir(estado).await;
    cerrar(estado, activo, motivo).await;
    difundir(estado).await;
    Ok(())
}

/// Suelta todo lo que sostiene un túnel ya sacado del mapa.
async fn cerrar(estado: &Arc<Mutex<EstadoServidor>>, mut activo: TunelActivo, motivo: &str) {
    if let Some(parada) = activo.parada.take() {
        // El bucle de aceptación cierra el listener al recibir la señal.
        let _ = parada.send(());
    }
    // Y las conexiones ya establecidas se cortan: si no, seguirían llevando
    // tráfico de un túnel que ya no existe (y sin contarlo).
    let _ = activo.cancelacion.send(true);
    if let Some((direccion, puerto)) = activo.reenvio.take() {
        estado
            .lock()
            .await
            .reenvios
            .quitar(activo.host_id, &direccion, puerto);
        if let Some(conexion) = &activo.conexion {
            // Cancelar el reenvío no toca a las pestañas que compartan la
            // conexión: solo deja de aceptar conexiones en ese puerto.
            cancelar_reenvio(&conexion.handle, &direccion, puerto).await;
        }
    }
    let (_, aceptadas, subidos, bajados) = activo.contadores.instantanea();
    let bd = estado.lock().await.bd.clone();
    let _ = bd.send(super::OrdenBd::Anotar {
        tipo: registro::TUNEL_CERRADO.to_string(),
        host_id: Some(activo.host_id),
        identidad_id: None,
        detalle: format!(
            "{} · {motivo} · {aceptadas} conexión(es) · ↓ {} · ↑ {}",
            activo.nombre,
            crate::archivos::tamano_legible(bajados),
            crate::archivos::tamano_legible(subidos)
        ),
        resultado: ResultadoRegistro::Ok,
    });
    if let Some(conexion) = activo.conexion.take() {
        conexion.soltar().await;
    }
}

/// Vuelve a levantar un túnel caído, con los contadores a cero.
pub async fn relanzar(
    estado: &Arc<Mutex<EstadoServidor>>,
    tunel_id: i64,
    solicitante: u32,
) -> Result<(), String> {
    {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(activo) = estado_bloqueado.tuneles.get(&tunel_id) {
            if activo.estado == EstadoTunelRemoto::Activo {
                return Ok(());
            }
        }
        estado_bloqueado.tuneles.remove(&tunel_id);
    }
    activar(estado, tunel_id, OrigenTunel::Manual, Some(solicitante)).await
}

/// Descarta el error de un túnel caído: la fila vuelve a «inactivo» sin
/// levantarlo.
pub async fn descartar(estado: &Arc<Mutex<EstadoServidor>>, tunel_id: i64) {
    let quitado = {
        let mut estado_bloqueado = estado.lock().await;
        match estado_bloqueado.tuneles.get(&tunel_id) {
            Some(activo) if activo.estado == EstadoTunelRemoto::Caido => {
                estado_bloqueado.tuneles.remove(&tunel_id).is_some()
            }
            _ => false,
        }
    };
    if quitado {
        difundir(estado).await;
    }
}

// ---------------------------------------------------------------- cambios

/// El cliente ha tocado `TUNELES`: se paran los túneles de ese host que ya no
/// existan o hayan cambiado de forma, y el ciclo automático se reajusta.
pub async fn recargar(estado: &Arc<Mutex<EstadoServidor>>, host_id: i64) {
    // Si el host ya no está en el inventario (lo borraron), sus túneles se van
    // con él: no hay nada que releer.
    let existe = {
        let lectura = estado.lock().await.lectura.clone();
        let Ok(lectura) = lectura.lock() else {
            return;
        };
        lectura.obtener_host(host_id).is_ok()
    };
    if !existe {
        parar_de_host(estado, host_id, "el host se borró").await;
        return;
    }
    let filas = tuneles_de_host(estado, host_id).await;
    let a_parar: Vec<i64> = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .tuneles
            .values()
            .filter(|activo| activo.host_id == host_id)
            .filter(
                |activo| match filas.iter().find(|fila| fila.id == activo.tunel_id) {
                    // Un cambio de escucha, destino o tipo deja el túnel vivo
                    // apuntando a otra cosa: se para (el cliente decide si relanza).
                    Some(fila) => {
                        fila.escucha != activo.escucha
                            || fila.destino != activo.destino
                            || fila.tipo != activo.tipo
                    }
                    None => true,
                },
            )
            .map(|activo| activo.tunel_id)
            .collect()
    };
    for tunel_id in a_parar {
        let _ = parar(estado, tunel_id, "el túnel cambió o se borró", false).await;
    }
    if hay_canales(estado, host_id).await {
        activar_automaticos(estado, host_id).await;
    }
}

/// Ciclo automático: con el primer canal de pestaña o SFTP del host se levantan
/// sus túneles automáticos; con el último, se paran los que levantó el ciclo.
///
/// Un túnel no cuenta como canal para este cómputo, así que un túnel manual no
/// impide que los automáticos se paren ni un automático mantiene vivo a otro.
pub async fn canales_cambiaron(estado: &Arc<Mutex<EstadoServidor>>, host_id: i64) {
    if hay_canales(estado, host_id).await {
        activar_automaticos(estado, host_id).await;
    } else {
        let a_parar: Vec<i64> = {
            let estado_bloqueado = estado.lock().await;
            estado_bloqueado
                .tuneles
                .values()
                .filter(|activo| {
                    activo.host_id == host_id
                        && activo.origen == OrigenTunel::Automatico
                        && activo.estado.en_marcha()
                })
                .map(|activo| activo.tunel_id)
                .collect()
        };
        for tunel_id in a_parar {
            let _ = parar(estado, tunel_id, "última sesión cerrada", false).await;
        }
        // El ciclo se cierra: lo que el usuario paró a mano deja de estarlo
        // para el próximo (la próxima pestaña lo vuelve a levantar).
        estado
            .lock()
            .await
            .tuneles_parados
            .retain(|(host, _)| *host != host_id);
    }
}

/// ¿Tiene este host alguna pestaña o canal SFTP vivo? Los túneles no cuentan.
async fn hay_canales(estado: &Arc<Mutex<EstadoServidor>>, host_id: i64) -> bool {
    hay_canales_con(&*estado.lock().await, host_id)
}

/// Lo mismo, con el estado ya bloqueado.
fn hay_canales_con(estado: &EstadoServidor, host_id: i64) -> bool {
    sesiones::canales_de_pestana(estado, host_id) > 0 || estado.sftp.contains_key(&host_id)
}

/// Levanta los túneles automáticos del host que no estén ya en marcha. Antes
/// de cada uno se vuelve a mirar si el host sigue en uso: levantar uno puede
/// tardar y la pestaña puede haberse cerrado entretanto.
async fn activar_automaticos(estado: &Arc<Mutex<EstadoServidor>>, host_id: i64) {
    for fila in tuneles_de_host(estado, host_id).await {
        if !fila.automatico {
            continue;
        }
        if !hay_canales(estado, host_id).await {
            return;
        }
        let parado_a_mano = estado
            .lock()
            .await
            .tuneles_parados
            .contains(&(host_id, fila.id));
        if parado_a_mano {
            continue;
        }
        let ya_activo = {
            let estado_bloqueado = estado.lock().await;
            estado_bloqueado
                .tuneles
                .get(&fila.id)
                .is_some_and(|activo| activo.estado.en_marcha())
        };
        if !ya_activo {
            // El ciclo automático no dialoga nunca: si necesita credenciales, el
            // túnel queda caído con el motivo y el usuario decide.
            let _ = activar(estado, fila.id, OrigenTunel::Automatico, None).await;
        }
    }
}

/// Filas de `TUNELES` de un host, del almacén de solo lectura.
async fn tuneles_de_host(estado: &Arc<Mutex<EstadoServidor>>, host_id: i64) -> Vec<Tunel> {
    let lectura = estado.lock().await.lectura.clone();
    let Ok(lectura) = lectura.lock() else {
        return Vec::new();
    };
    match lectura.tuneles_de_host(host_id) {
        Ok(filas) => filas,
        // Una fila ilegible (un tipo que no conocemos) deja al host sin
        // túneles: mejor no ejecutar lo que no se entiende.
        Err(error) => {
            warn!(host_id, "no se pudieron leer los túneles del host: {error}");
            Vec::new()
        }
    }
}

/// Para todos los túneles de un host (al borrarlo o al irse su conexión).
pub async fn parar_de_host(estado: &Arc<Mutex<EstadoServidor>>, host_id: i64, motivo: &str) {
    let ids: Vec<i64> = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .tuneles
            .values()
            .filter(|activo| activo.host_id == host_id)
            .map(|activo| activo.tunel_id)
            .collect()
    };
    for tunel_id in ids {
        let _ = parar(estado, tunel_id, motivo, false).await;
    }
}

/// Lo que queda de un túnel retirado al apagar: lo que toca la red.
pub struct TunelRetirado {
    conexion: Option<ConexionTomada>,
    reenvio: Option<(String, u32)>,
}

/// Apagado del servidor, primera fase (sin red): saca todos los túneles,
/// corta listeners y copias, olvida sus reenvíos y anota `tunel_cerrado` de
/// los activos con sus totales. Lo que habla con el host va aparte
/// (`cerrar_retirado`), para que un host que no contesta no se coma el plazo
/// del apagado antes de anotar nada.
pub async fn retirar_todos(
    estado: &Arc<Mutex<EstadoServidor>>,
    motivo: &str,
) -> Vec<TunelRetirado> {
    let (activos, bd, reenvios) = {
        let mut estado_bloqueado = estado.lock().await;
        let activos: Vec<TunelActivo> = estado_bloqueado
            .tuneles
            .drain()
            .map(|(_, activo)| activo)
            .collect();
        (
            activos,
            estado_bloqueado.bd.clone(),
            estado_bloqueado.reenvios.clone(),
        )
    };
    let mut retirados = Vec::new();
    for mut activo in activos {
        if let Some(parada) = activo.parada.take() {
            let _ = parada.send(());
        }
        let _ = activo.cancelacion.send(true);
        let reenvio = activo.reenvio.take();
        if let Some((direccion, puerto)) = &reenvio {
            reenvios.quitar(activo.host_id, direccion, *puerto);
        }
        // Un caído ya anotó su caída y uno que se levantaba no llegó a abrir.
        if activo.estado == EstadoTunelRemoto::Activo {
            let (_, aceptadas, subidos, bajados) = activo.contadores.instantanea();
            let _ = bd.send(super::OrdenBd::Anotar {
                tipo: registro::TUNEL_CERRADO.to_string(),
                host_id: Some(activo.host_id),
                identidad_id: None,
                detalle: format!(
                    "{} · {motivo} · {aceptadas} conexión(es) · ↓ {} · ↑ {}",
                    activo.nombre,
                    crate::archivos::tamano_legible(bajados),
                    crate::archivos::tamano_legible(subidos)
                ),
                resultado: ResultadoRegistro::Ok,
            });
        }
        retirados.push(TunelRetirado {
            conexion: activo.conexion.take(),
            reenvio,
        });
    }
    retirados
}

/// Segunda fase: cancela la escucha remota (con plazo) y suelta la conexión.
pub async fn cerrar_retirado(retirado: TunelRetirado) {
    if let (Some(conexion), Some((direccion, puerto))) = (&retirado.conexion, &retirado.reenvio) {
        cancelar_reenvio(&conexion.handle, direccion, *puerto).await;
    }
    if let Some(conexion) = retirado.conexion {
        conexion.soltar().await;
    }
}

// ---------------------------------------------------------------- revisora

/// Vigila cada medio segundo: los túneles cuya conexión del pool se cayó pasan
/// a «caído» y, si la lista cambió, se difunde. Los contadores van solos en esa
/// misma lista, así que como mucho salen dos veces por segundo.
pub async fn revisar(estado: Arc<Mutex<EstadoServidor>>) {
    let mut ultima: Vec<InfoTunel> = Vec::new();
    loop {
        tokio::time::sleep(REVISION).await;

        let caidos: Vec<(i64, String)> = {
            let estado_bloqueado = estado.lock().await;
            estado_bloqueado
                .tuneles
                .values()
                .filter(|activo| activo.estado == EstadoTunelRemoto::Activo)
                .filter(|activo| {
                    activo
                        .conexion
                        .as_ref()
                        .is_some_and(|conexion| conexion.handle.is_closed())
                })
                .map(|activo| {
                    (
                        activo.tunel_id,
                        "se cayó la conexión con el host".to_string(),
                    )
                })
                .collect()
        };
        for (tunel_id, motivo) in caidos {
            caido(&estado, tunel_id, &motivo, false).await;
            difundir(&estado).await;
        }

        // El error de una conexión suelta (un canal rechazado, por ejemplo) se
        // sube al túnel para que se vea en el detalle.
        let lista_ahora = {
            let mut estado_bloqueado = estado.lock().await;
            for activo in estado_bloqueado.tuneles.values_mut() {
                if let Some(error) = activo.contadores.error() {
                    if activo.ultimo_error.as_deref() != Some(error.as_str()) {
                        activo.ultimo_error = Some(error);
                    }
                }
            }
            if estado_bloqueado.clientes.is_empty() {
                ultima = lista(&estado_bloqueado);
                continue;
            }
            lista(&estado_bloqueado)
        };
        // Solo si cambia: es el ritmo de los contadores, dos por segundo.
        if lista_ahora != ultima {
            let estado_bloqueado = estado.lock().await;
            difusion::difundir(
                &estado_bloqueado.clientes,
                MensajeServidor::Tuneles {
                    lista: lista_ahora.clone(),
                },
            );
            ultima = lista_ahora;
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_motivo_de_un_bind_ocupado_se_entiende() {
        let error = std::io::Error::from(std::io::ErrorKind::AddrInUse);
        let motivo = bind_fallido("127.0.0.1:5432", &error);
        assert!(motivo.contains("5432"), "{motivo}");
        assert!(motivo.contains("uso"), "{motivo}");
    }

    #[test]
    fn un_puerto_privilegiado_lo_dice_claro() {
        let error = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let motivo = bind_fallido("0.0.0.0:80", &error);
        assert!(motivo.contains("permiso"), "{motivo}");
    }
}
