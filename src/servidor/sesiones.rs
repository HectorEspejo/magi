//! Registro de sesiones del servidor: ciclo de vida completo de cada sesión
//! SSH (apertura con diálogos reenviados al solicitante, teclas, pantallas,
//! tamaños compartidos, caída y reconexión) y la lista que se difunde a los
//! clientes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use russh::client::Handle;
use russh::{Channel, ChannelMsg, Disconnect};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use zeroize::Zeroizing;

use crate::conexion::cliente::{abrir_canal, Cliente, Contexto, IdentidadUsada};
use crate::conexion::salto::{conectar_cadena, construir_cadena, Transporte};
use crate::conexion::terminal::{self, Pantalla};
use crate::conexion::{EventoConexion, FuenteContrasena};
use crate::config::Rutas;
use crate::modelo::{EstadoSesion, Host};
use crate::protocolo::{EstadoSesionRemota, InfoSesion, MensajeServidor, Secreto};

use super::cliente_remoto::Tamano;
use super::difusion;
use super::EstadoServidor;

/// Tiempo máximo que espera el servidor una decisión del solicitante.
const TIMEOUT_DECISION: Duration = Duration::from_secs(5 * 60);

/// Comandos que el estado del servidor encola hacia la tarea de una sesión.
#[derive(Debug)]
pub enum ComandoSesion {
    Teclas(Vec<u8>),
    /// Tamaño mínimo recalculado que debe aplicarse al remoto y al parser.
    AplicarTamano(u16, u16),
    Cerrar,
}

/// Una decisión pendiente de un diálogo cuyo destinatario es el solicitante.
pub enum Pendiente {
    Huella(oneshot::Sender<bool>),
    Frase(oneshot::Sender<Option<Zeroizing<String>>>),
    Contrasena(oneshot::Sender<Option<(Zeroizing<String>, bool)>>),
}

/// Respuesta de un diálogo que llega por el protocolo desde un cliente.
pub enum DecisionDialogo {
    Huella(bool),
    Frase(Secreto),
    Contrasena(Secreto, bool),
}

/// Una sesión SSH custodiada por el servidor.
pub struct Sesion {
    pub id: u32,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    pub estado: EstadoSesionRemota,
    pub motivo: Option<String>,
    pub identidad: String,
    /// Adjuntos: cliente → tamaño de ventana que reportó.
    pub adjuntos: HashMap<u32, Tamano>,
    pub solicitante: u32,
    pub abierta_en: i64,
    pub ultima_actividad: i64,
    pub actividad_no_vista: bool,
    /// Conexión SSH de esta sesión (para `Ejecutar` y cerrarla si es propia).
    pub handle: Option<Arc<Handle<Cliente>>>,
    /// Tamaño vigente (último aplicado al remoto).
    pub tamano: Tamano,
    /// Pantalla del servidor de la sesión (volcado al adjuntar).
    pub pantalla: Pantalla,
    pub tx_comandos: mpsc::UnboundedSender<ComandoSesion>,
}

impl Sesion {
    /// Resumen para la difusión `Sesiones`.
    pub fn info(&self) -> InfoSesion {
        InfoSesion {
            id: self.id,
            nombre: self.nombre.clone(),
            host_id: self.host_id,
            host_nombre: self.host_nombre.clone(),
            estado: self.estado,
            motivo: self.motivo.clone(),
            identidad: self.identidad.clone(),
            abierta_en: self.abierta_en,
            ventanas: self.adjuntos.len() as u32,
            actividad_no_vista: self.actividad_no_vista,
        }
    }
}

/// Construye la lista de sesiones ordenada por id para la difusión.
pub fn lista(sesiones: &HashMap<u32, Sesion>) -> Vec<InfoSesion> {
    let mut infos: Vec<InfoSesion> = sesiones.values().map(Sesion::info).collect();
    infos.sort_by_key(|info| info.id);
    infos
}

/// Difunde la lista completa de sesiones a todos los clientes.
pub fn difundir_lista(estado: &mut EstadoServidor) {
    let mensajes = lista(&estado.sesiones);
    difusion::difundir(
        &estado.clientes,
        MensajeServidor::Sesiones { lista: mensajes },
    );
}

/// Nombre de pestaña para una sesión nueva al host: `host` si no hay otra
/// sesión viva, `host (n)` con el menor n ≥ 2 libre en caso contrario.
pub fn nombre_de_pestaña(sesiones: &HashMap<u32, Sesion>, host: &Host) -> String {
    let del_host: Vec<&Sesion> = sesiones
        .values()
        .filter(|sesion| sesion.host_id == host.id)
        .collect();
    if del_host.is_empty() {
        return host.nombre.clone();
    }
    let usados: Vec<u32> = del_host
        .iter()
        .filter_map(|sesion| {
            sesion
                .nombre
                .strip_prefix(&format!("{} (", host.nombre))
                .and_then(|resto| resto.strip_suffix(')'))
                .and_then(|numero| numero.parse::<u32>().ok())
        })
        .collect();
    let mut numero = 2;
    while usados.contains(&numero) {
        numero += 1;
    }
    format!("{} ({numero})", host.nombre)
}

/// Tamaño de la sesión: mínimo de los adjuntos; sin adjuntos se conserva el
/// último.
pub fn tamano_minimo(adjuntos: &HashMap<u32, Tamano>, actual: Tamano) -> Tamano {
    adjuntos.values().fold(actual, |menor, tamano| Tamano {
        cols: menor.cols.min(tamano.cols),
        filas: menor.filas.min(tamano.filas),
    })
}

// ---------------------------------------------------------------- apertura

/// Datos fijos para abrir una sesión, calculados bajo bloqueo.
pub struct DatosApertura {
    pub host: Host,
    pub todos_los_hosts: HashMap<i64, Host>,
    pub rutas: Rutas,
    /// Pantalla del servidor para la sesión (volcado al adjuntar).
    pub pantalla: Pantalla,
    pub cols: u16,
    pub filas: u16,
    pub sesion_id: u32,
    pub solicitante: u32,
    /// Es una reconexión: el id y el nombre ya existen.
    pub reconexion: bool,
}

/// Lanza la tarea de una sesión: abre la conexión (flujo de la Fase 1 con los
/// diálogos reenviados al solicitante) y sirve el bucle de teclas y pantallas
/// hasta el cierre. La sesión ya debe estar insertada en el estado.
pub fn lanzar(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: DatosApertura,
    rx_comandos: mpsc::UnboundedReceiver<ComandoSesion>,
) {
    tokio::spawn(async move {
        abrir_y_servir(estado, datos, rx_comandos).await;
    });
}

async fn abrir_y_servir(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: DatosApertura,
    mut rx_comandos: mpsc::UnboundedReceiver<ComandoSesion>,
) {
    let sesion_id = datos.sesion_id;
    let host_id = datos.host.id;
    let reconexion = datos.reconexion;
    let (tx_eventos, rx_eventos) = mpsc::unbounded_channel::<EventoConexion>();
    let puente = tokio::spawn(puente_eventos(
        estado.clone(),
        sesion_id,
        datos.solicitante,
        rx_eventos,
    ));

    let apertura = abrir_conexion(&datos, &tx_eventos).await;
    drop(tx_eventos);
    let _ = puente.await;

    match apertura {
        Ok((transporte, mut canal, pantalla, identidad)) => {
            let multiplexar = datos.host.multiplexar;
            let handle = transporte.handle.clone();
            // Con `multiplexar` la conexión queda en el pool y sobrevive a la
            // sesión; sin él, es conexión propia y se cierra con ella.
            let propio = if multiplexar {
                estado.lock().await.pool.guardar(host_id, transporte);
                None
            } else {
                Some(transporte)
            };
            {
                let mut estado_bloqueado = estado.lock().await;
                let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
                    // La sesión se cerró mientras abría: limpia y termina.
                    super::conexiones::desconectar(handle, Vec::new()).await;
                    desconectar_propio(propio).await;
                    return;
                };
                sesion.estado = EstadoSesionRemota::Abierta;
                sesion.motivo = None;
                sesion.identidad = identidad.descripcion.clone();
                sesion.abierta_en = Utc::now().timestamp();
                sesion.handle = Some(handle.clone());
            }
            let bd = estado.lock().await.bd.clone();
            let _ = bd.send(super::OrdenBd::Anotar {
                tipo: if reconexion {
                    crate::registro::SESION_RECONECTADA.to_string()
                } else {
                    crate::registro::CONEXION_ABIERTA.to_string()
                },
                host_id: Some(host_id),
                identidad_id: None,
                detalle: format!(
                    "sesión abierta con «{}» · {}",
                    datos.host.nombre, identidad.descripcion
                ),
                resultado: crate::modelo::ResultadoRegistro::Ok,
            });
            let _ = bd.send(super::OrdenBd::MarcarConexion { host_id });
            let _ = bd.send(super::OrdenBd::MarcarEstado {
                host_id,
                estado: Some(crate::modelo::UltimoEstado::Ok),
            });
            info!(sesion = sesion_id, host = %datos.host.nombre, "sesión abierta en el servidor");
            difundir_lista(&mut *estado.lock().await);

            bucle_sesion(
                estado,
                sesion_id,
                &mut canal,
                &pantalla,
                &mut rx_comandos,
                propio,
            )
            .await;
        }
        Err(motivo) => {
            warn!(sesion = sesion_id, host = %datos.host.nombre, "apertura fallida: {motivo}");
            let bd = estado.lock().await.bd.clone();
            let _ = bd.send(super::OrdenBd::Anotar {
                tipo: crate::registro::CONEXION_FALLIDA.to_string(),
                host_id: Some(host_id),
                identidad_id: None,
                detalle: motivo.clone(),
                resultado: crate::modelo::ResultadoRegistro::Error,
            });
            let _ = bd.send(super::OrdenBd::MarcarEstado {
                host_id,
                estado: Some(crate::modelo::UltimoEstado::Error),
            });
            fallida(&estado, sesion_id, &motivo).await;
        }
    }
}

/// Marca una apertura como fallida: elimina la sesión, descarta cualquier
/// decisión pendiente y avisa a todos los clientes con el motivo.
async fn fallida(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, sesion_id: u32, motivo: &str) {
    let mut estado_bloqueado = estado.lock().await;
    estado_bloqueado.pendientes.remove(&sesion_id);
    estado_bloqueado.sesiones.remove(&sesion_id);
    difusion::difundir(
        &estado_bloqueado.clientes,
        MensajeServidor::Estado {
            sesion_id,
            estado: EstadoSesionRemota::Cerrada,
            motivo: Some(motivo.to_string()),
        },
    );
    difundir_lista(&mut estado_bloqueado);
}

/// Conecta la cadena completa y abre el pty con la shell.
async fn abrir_conexion(
    datos: &DatosApertura,
    tx_eventos: &mpsc::UnboundedSender<EventoConexion>,
) -> Result<
    (
        Transporte,
        Channel<russh::client::Msg>,
        Pantalla,
        IdentidadUsada,
    ),
    String,
> {
    emitir_estado(tx_eventos, EstadoSesion::Resolviendo);
    let cadena =
        construir_cadena(&datos.host, &datos.todos_los_hosts).map_err(|error| error.to_string())?;
    emitir_estado(tx_eventos, EstadoSesion::Conectando);
    let contexto = Contexto {
        known_hosts: datos.rutas.fichero_known_hosts(),
        dir_ssh: datos.rutas.dir_ssh(),
        hogar: datos.rutas.hogar.clone(),
        usuario_local: crate::conexion::usuario_local(),
        tx: tx_eventos.clone(),
        interactivo: true,
        fuente_contrasena: FuenteContrasena::Solicitante,
    };
    let transporte = conectar_cadena(&cadena, &contexto)
        .await
        .map_err(|error| error.to_string())?;
    let pantalla = datos.pantalla.clone();
    let canal = match abrir_canal(&transporte, datos.cols, datos.filas).await {
        Ok(canal) => canal,
        Err(error) => {
            let _ = transporte
                .handle
                .disconnect(Disconnect::ByApplication, "", "")
                .await;
            return Err(error.to_string());
        }
    };
    let identidad = IdentidadUsada {
        descripcion: transporte.identidad.descripcion.clone(),
        huella: transporte.identidad.huella.clone(),
    };
    Ok((transporte, canal, pantalla, identidad))
}

fn emitir_estado(tx: &mpsc::UnboundedSender<EventoConexion>, estado: EstadoSesion) {
    let _ = tx.send(EventoConexion::Estado { host_id: 0, estado });
}

/// Traduce los eventos de la tarea de conexión a mensajes del protocolo:
/// diálogos solo al solicitante (con respuesta diferida), estados al resto.
async fn puente_eventos(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    solicitante: u32,
    mut rx: mpsc::UnboundedReceiver<EventoConexion>,
) {
    while let Some(evento) = rx.recv().await {
        match evento {
            EventoConexion::Estado { estado: fino, .. } => {
                let motivo = motivo_de_estado(fino).to_string();
                {
                    let mut estado_bloqueado = estado.lock().await;
                    if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
                        sesion.motivo = Some(motivo.clone());
                    }
                }
                let mut estado_bloqueado = estado.lock().await;
                difundir_lista(&mut estado_bloqueado);
            }
            EventoConexion::HuellaDesconocida {
                host,
                tipo,
                huella,
                responder,
            } => {
                decidir(
                    &estado,
                    sesion_id,
                    solicitante,
                    Pendiente::Huella(responder),
                    MensajeServidor::HuellaDesconocida {
                        sesion_id,
                        host,
                        tipo_clave: tipo,
                        huella,
                    },
                )
                .await;
            }
            EventoConexion::HuellaCambiada {
                host,
                tipo,
                anterior,
                nueva,
                responder,
            } => {
                decidir(
                    &estado,
                    sesion_id,
                    solicitante,
                    Pendiente::Huella(responder),
                    MensajeServidor::HuellaCambiada {
                        sesion_id,
                        host,
                        tipo_clave: tipo,
                        anterior,
                        nueva,
                    },
                )
                .await;
            }
            EventoConexion::PideFrase {
                host,
                intento,
                responder,
            } => {
                decidir(
                    &estado,
                    sesion_id,
                    solicitante,
                    Pendiente::Frase(responder),
                    MensajeServidor::PideFrase {
                        sesion_id,
                        host,
                        intento,
                    },
                )
                .await;
            }
            EventoConexion::PideContrasena {
                host,
                intento,
                recordar_por_defecto,
                responder,
            } => {
                decidir(
                    &estado,
                    sesion_id,
                    solicitante,
                    Pendiente::Contrasena(responder),
                    MensajeServidor::PideContrasena {
                        sesion_id,
                        host,
                        intento,
                        recordar_por_defecto,
                    },
                )
                .await;
            }
            // La huella ya se escribió en known_hosts: el efecto se anota aquí.
            EventoConexion::HuellaRegistrada {
                host_id,
                anterior,
                nueva,
            } => {
                let (tipo, detalle) = match anterior {
                    Some(anterior) => (
                        crate::registro::HUELLA_SUSTITUIDA,
                        format!("anterior {anterior} · nueva {nueva}"),
                    ),
                    None => (
                        crate::registro::HUELLA_ACEPTADA,
                        format!("huella nueva {nueva}"),
                    ),
                };
                let bd = estado.lock().await.bd.clone();
                let _ = bd.send(super::OrdenBd::Anotar {
                    tipo: tipo.to_string(),
                    host_id: (host_id != 0).then_some(host_id),
                    identidad_id: None,
                    detalle,
                    resultado: crate::modelo::ResultadoRegistro::Ok,
                });
            }
            // Eventos que en el servidor no aplican: la apertura no pasa por
            // `sesion_completa` y las pantallas las difunde el bucle.
            EventoConexion::ContrasenaGuardada { .. }
            | EventoConexion::Abierta { .. }
            | EventoConexion::Pantalla
            | EventoConexion::PruebaOk { .. }
            | EventoConexion::Cerrada { .. }
            | EventoConexion::Error { .. }
            | EventoConexion::Cancelada { .. } => {}
        }
    }
}

/// Guarda una decisión pendiente, avisa al solicitante y al resto de clientes,
/// y programa el timeout de 5 minutos.
async fn decidir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    solicitante: u32,
    pendiente: Pendiente,
    mensaje: MensajeServidor,
) {
    let mut estado_bloqueado = estado.lock().await;
    estado_bloqueado.pendientes.insert(sesion_id, pendiente);
    let mut otros: Vec<u32> = Vec::new();
    for (id_cliente, cliente) in &estado_bloqueado.clientes {
        if *id_cliente == solicitante {
            difusion::enviar(cliente, mensaje.clone());
        } else {
            otros.push(*id_cliente);
        }
    }
    for id in otros {
        if let Some(cliente) = estado_bloqueado.clientes.get(&id) {
            difusion::enviar(
                cliente,
                MensajeServidor::Estado {
                    sesion_id,
                    estado: EstadoSesionRemota::Abriendo,
                    motivo: Some("esperando decisión en otra ventana".to_string()),
                },
            );
        }
    }
    // Timeout: si la decisión no llega, se descarta el canal de respuesta y
    // la apertura se cancela sola.
    let estado_timeout = estado.clone();
    tokio::spawn(async move {
        tokio::time::sleep(TIMEOUT_DECISION).await;
        let mut estado_bloqueado = estado_timeout.lock().await;
        if estado_bloqueado.pendientes.remove(&sesion_id).is_some() {
            warn!(sesion = sesion_id, "decisión sin respuesta tras 5 minutos");
        }
    });
}

/// Resuelve una decisión que llega por el protocolo. Solo el solicitante de
/// la sesión puede responder; devuelve si se aceptó la respuesta.
pub async fn resolver_decision(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    cliente_id: u32,
    decision: DecisionDialogo,
) -> bool {
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get(&sesion_id) else {
        warn!(
            sesion = sesion_id,
            "decisión para una sesión que ya no existe"
        );
        return false;
    };
    if sesion.solicitante != cliente_id {
        warn!(
            sesion = sesion_id,
            cliente = cliente_id,
            "un cliente que no es el solicitante intentó responder a un diálogo"
        );
        return false;
    }
    match (estado_bloqueado.pendientes.remove(&sesion_id), decision) {
        (Some(Pendiente::Huella(responder)), DecisionDialogo::Huella(d)) => {
            let _ = responder.send(d);
            true
        }
        (Some(Pendiente::Frase(responder)), DecisionDialogo::Frase(frase)) => {
            let _ = responder.send(Some(frase.0));
            true
        }
        (
            Some(Pendiente::Contrasena(responder)),
            DecisionDialogo::Contrasena(contrasena, recordar),
        ) => {
            let _ = responder.send(Some((contrasena.0, recordar)));
            true
        }
        (pendiente, _) => {
            if let Some(sin_usar) = pendiente {
                drop(sin_usar);
            }
            warn!(sesion = sesion_id, "decisión sin diálogo pendiente");
            false
        }
    }
}

fn motivo_de_estado(estado: EstadoSesion) -> &'static str {
    match estado {
        EstadoSesion::Resolviendo => "resolviendo",
        EstadoSesion::Conectando => "conectando",
        EstadoSesion::VerificandoHuella => "verificando huella",
        EstadoSesion::Autenticando => "autenticando",
        _ => "abriendo",
    }
}

/// Bucle de una sesión abierta: teclas y tamaños de los adjuntos hacia el
/// remoto, datos del remoto hacia los adjuntos y el parser del servidor.
async fn bucle_sesion(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    canal: &mut Channel<russh::client::Msg>,
    pantalla: &Pantalla,
    rx_comandos: &mut mpsc::UnboundedReceiver<ComandoSesion>,
    propio: Option<Transporte>,
) {
    let mut vio_eof = false;
    loop {
        tokio::select! {
            comando = rx_comandos.recv() => match comando {
                Some(ComandoSesion::Teclas(bytes)) => {
                    if let Err(error) = canal.data_bytes(bytes).await {
                        caida(&estado, sesion_id, &format!("se perdió la conexión: {error}")).await;
                        desconectar_propio(propio).await;
                        return;
                    }
                }
                Some(ComandoSesion::AplicarTamano(cols, filas)) => {
                    let _ = canal.window_change(u32::from(cols), u32::from(filas), 0, 0).await;
                    terminal::redimensionar(pantalla, filas, cols);
                }
                Some(ComandoSesion::Cerrar) | None => {
                    let _ = canal.eof().await;
                    let _ = canal.close().await;
                    cerrada(&estado, sesion_id, "cerrada desde la ventana").await;
                    desconectar_propio(propio).await;
                    return;
                }
            },
            mensaje = canal.wait() => match mensaje {
                Some(ChannelMsg::Data { data }) => {
                    procesar_datos(&estado, sesion_id, pantalla, &data).await;
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    procesar_datos(&estado, sesion_id, pantalla, &data).await;
                }
                Some(ChannelMsg::Eof) => vio_eof = true,
                Some(ChannelMsg::Close) => {
                    cerrada(&estado, sesion_id, "el remoto cerró la sesión").await;
                    desconectar_propio(propio).await;
                    return;
                }
                None => {
                    if vio_eof {
                        cerrada(&estado, sesion_id, "el remoto cerró la sesión").await;
                    } else {
                        caida(&estado, sesion_id, "la conexión se ha perdido").await;
                    }
                    desconectar_propio(propio).await;
                    return;
                }
                _ => {}
            },
        }
    }
}

/// Reparte un bloque de datos del remoto: parser del servidor y difusión a
/// los adjuntos; sin adjuntos se marca actividad sin ver.
async fn procesar_datos(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    pantalla: &Pantalla,
    datos: &[u8],
) {
    if let Ok(mut parser) = pantalla.lock() {
        parser.process(datos);
    }
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
        return;
    };
    sesion.ultima_actividad = Utc::now().timestamp();
    if sesion.adjuntos.is_empty() {
        sesion.actividad_no_vista = true;
        return;
    }
    let ids: Vec<u32> = sesion.adjuntos.keys().copied().collect();
    let mensaje = MensajeServidor::Datos {
        sesion_id,
        bytes: datos.to_vec(),
    };
    difusion::difundir_a_adjuntos(&estado_bloqueado.clientes, &ids, mensaje);
}

/// `exit` del remoto o cierre desde una ventana: la sesión se elimina.
async fn cerrada(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, sesion_id: u32, motivo: &str) {
    let mut estado_bloqueado = estado.lock().await;
    if let Some(sesion) = estado_bloqueado.sesiones.remove(&sesion_id) {
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::SESION_CERRADA.to_string(),
            host_id: Some(sesion.host_id),
            identidad_id: None,
            detalle: motivo.to_string(),
            resultado: crate::modelo::ResultadoRegistro::Ok,
        });
        if sesion.handle.is_some() {
            estado_bloqueado.pool.liberar(sesion.host_id);
        }
    }
    estado_bloqueado.pendientes.remove(&sesion_id);
    difundir_lista(&mut estado_bloqueado);
}

/// Red caída o EOF inesperado: la sesión se conserva hasta reconectar o cerrar.
async fn caida(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, sesion_id: u32, motivo: &str) {
    let mut estado_bloqueado = estado.lock().await;
    let (host_id, ids) = {
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            estado_bloqueado.pendientes.remove(&sesion_id);
            return;
        };
        sesion.estado = EstadoSesionRemota::Caida;
        sesion.motivo = Some(motivo.to_string());
        sesion.handle = None;
        (
            sesion.host_id,
            sesion.adjuntos.keys().copied().collect::<Vec<u32>>(),
        )
    };
    let aviso = MensajeServidor::Estado {
        sesion_id,
        estado: EstadoSesionRemota::Caida,
        motivo: Some(motivo.to_string()),
    };
    difusion::difundir_a_adjuntos(&estado_bloqueado.clientes, &ids, aviso);
    let bd = estado_bloqueado.bd.clone();
    let _ = bd.send(super::OrdenBd::Anotar {
        tipo: crate::registro::SESION_CERRADA.to_string(),
        host_id: Some(host_id),
        identidad_id: None,
        detalle: format!("caída: {motivo}"),
        resultado: crate::modelo::ResultadoRegistro::Error,
    });
    estado_bloqueado.pendientes.remove(&sesion_id);
    difundir_lista(&mut estado_bloqueado);
}

async fn desconectar_propio(propio: Option<Transporte>) {
    if let Some(transporte) = propio {
        let saltos = transporte.saltos.into_iter().map(Arc::new).collect();
        super::conexiones::desconectar(transporte.handle, saltos).await;
    }
}
