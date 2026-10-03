//! Registro de sesiones del servidor: ciclo de vida completo de cada sesión
//! SSH (apertura con diálogos reenviados al solicitante, teclas, pantallas,
//! tamaños compartidos, caída y reconexión) y la lista que se difunde a los
//! clientes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use russh::client::Handle;
use russh::{Channel, ChannelMsg};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use zeroize::Zeroizing;

use crate::conexion::cliente::{abrir_canal, Cliente};
use crate::conexion::terminal::{self, Pantalla};
use crate::conexion::EventoConexion;
use crate::config::Rutas;
use crate::modelo::{EstadoSesion, Host};
use crate::protocolo::{EstadoSesionRemota, InfoSesion, MensajeServidor, Secreto};

use super::cliente_remoto::Tamano;
use super::conexiones::{self, ConexionTomada, PoliticaConexion, UsoCanal};
use super::difusion;
use super::EstadoServidor;

/// Tiempo máximo que espera el servidor una decisión del solicitante.
const TIMEOUT_DECISION: Duration = Duration::from_secs(5 * 60);

/// Plazo para abrir el canal de la pestaña (pty y shell) sobre una conexión
/// ya autenticada.
const PLAZO_CANAL: Duration = Duration::from_secs(10);

/// Tope de un comando inicial (lo manda el cliente y se escribe en la shell).
pub const TOPE_COMANDO_INICIAL: usize = 64 * 1024;

/// Generación de cada diálogo pendiente: el temporizador de uno no puede
/// borrar el siguiente que ocupe el mismo id (otro intento de frase, p. ej.).
static GENERACION_DIALOGO: AtomicU64 = AtomicU64::new(1);

/// Comandos que el estado del servidor encola hacia la tarea de una sesión.
#[derive(Debug)]
pub enum ComandoSesion {
    Teclas(Vec<u8>),
    /// Tamaño mínimo recalculado que debe aplicarse al remoto y al parser.
    AplicarTamano(u16, u16),
    Cerrar,
}

/// Un diálogo de conexión esperando la respuesta de un cliente.
pub enum DialogoPendiente {
    Huella(oneshot::Sender<bool>),
    Frase(oneshot::Sender<Option<Zeroizing<String>>>),
    Contrasena(oneshot::Sender<Option<(Zeroizing<String>, bool)>>),
    /// Contraseña de una conexión automática: solo del llavero del
    /// solicitante, que contesta sin diálogo.
    Llavero(oneshot::Sender<Option<Zeroizing<String>>>),
}

/// Una decisión pendiente: el diálogo y el cliente que puede responderla. El
/// solicitante se guarda aquí y no se busca en la sesión porque una apertura
/// de canal SFTP (Fase 4) también dialoga y no tiene sesión propia.
pub struct Pendiente {
    pub solicitante: u32,
    pub dialogo: DialogoPendiente,
    /// Para que el temporizador solo retire el diálogo que programó.
    pub generacion: u64,
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
    /// Cancela la apertura en curso (la pestaña se cierra, la ventana que la
    /// pidió se va o el servidor se apaga): suelta el cerrojo del host y la
    /// conexión a medias en vez de seguir dialogando para nadie.
    pub cancelar_apertura: tokio_util::sync::CancellationToken,
    /// Está reabriendo tras una caída: sigue contando para el ciclo automático
    /// de los túneles como cuando estaba caída.
    pub reconectando: bool,
    /// Lo que se escribe al abrir la shell; al reconectar, solo lo que
    /// `repetir` (el snippet al conectar).
    pub comandos_iniciales: Vec<crate::protocolo::ComandoInicial>,
    /// Tamaño vigente (último aplicado al remoto).
    pub tamano: Tamano,
    /// Ventana que explica el relleno de cada adjunto, tal como se le envió la
    /// última vez (una pestaña compartida puede tener las columnas impuestas
    /// por una ventana y las filas por otra).
    pub ventana_minima: HashMap<u32, Option<u32>>,
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

/// Pestañas de un host que cuentan para el ciclo automático de los túneles:
/// las abiertas y las caídas. Los túneles y el canal SFTP van aparte.
pub fn canales_de_pestana(estado: &EstadoServidor, host_id: i64) -> usize {
    // Las caídas siguen contando: la pestaña está ahí y se puede reconectar,
    // así que el túnel no tiene por qué caerse con ella. Las que aún abren no
    // cuentan: sus túneles automáticos se levantan cuando la pestaña queda
    // abierta, para no competir con sus diálogos por la conexión del host.
    estado
        .sesiones
        .values()
        .filter(|sesion| {
            sesion.host_id == host_id
                && (matches!(
                    sesion.estado,
                    EstadoSesionRemota::Abierta | EstadoSesionRemota::Caida
                ) || (sesion.estado == EstadoSesionRemota::Abriendo && sesion.reconectando))
        })
        .count()
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

/// Tamaño de la sesión: mínimo de los adjuntos, columna a columna y fila a
/// fila; sin adjuntos se conserva el último. Solo cuentan los adjuntos: si la
/// ventana pequeña crece o se va, el tamaño sube (antes partía del último
/// aplicado y nunca podía crecer).
pub fn tamano_minimo(adjuntos: &HashMap<u32, Tamano>, actual: Tamano) -> Tamano {
    adjuntos
        .values()
        .copied()
        .reduce(|menor, tamano| Tamano {
            cols: menor.cols.min(tamano.cols),
            filas: menor.filas.min(tamano.filas),
        })
        .unwrap_or(actual)
}

/// Ventana que explica el relleno de `destino`: si le sobran columnas, la
/// que impone las columnas; si solo le sobran filas, la que impone las filas;
/// a igualdad, la de id menor. Si no le sobra nada, es ella misma (el aviso
/// «mín. esta ventana» solo se pinta cuando hay relleno).
pub fn ventana_minima_para(
    adjuntos: &HashMap<u32, Tamano>,
    tamano: Tamano,
    destino: u32,
) -> Option<u32> {
    let propio = adjuntos.get(&destino)?;
    let impone = |columnas: bool| {
        adjuntos
            .iter()
            .filter(|(_, otro)| {
                if columnas {
                    otro.cols == tamano.cols
                } else {
                    otro.filas == tamano.filas
                }
            })
            .map(|(cliente_id, _)| *cliente_id)
            .min()
    };
    if propio.cols > tamano.cols {
        impone(true)
    } else if propio.filas > tamano.filas {
        impone(false)
    } else {
        Some(destino)
    }
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
    /// La de la sesión: cancelarla aborta la apertura.
    pub cancelar: tokio_util::sync::CancellationToken,
    /// Textos que se escriben en la shell en esta apertura, en orden.
    pub comandos: Vec<String>,
}

/// Lanza la tarea de una sesión: toma la conexión (la del pool con
/// `multiplexar`, o una propia; los diálogos van al solicitante) y sirve el
/// bucle de teclas y pantallas hasta el cierre. La sesión ya debe estar
/// insertada en el estado.
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

    // Corrección 3b: la pestaña pide la conexión por la única vía, con el
    // cerrojo del host. Con `multiplexar` reutiliza la viva del pool (sin
    // autenticar) y nunca desplaza nada; sin él abre una propia. Si la pestaña
    // se cierra mientras tanto, la apertura se abandona (y con ella el
    // cerrojo y la conexión a medias).
    let tomada = tokio::select! {
        tomada = conexiones::conexion_para_canal(
            &estado,
            &datos.host,
            &datos.todos_los_hosts,
            &datos.rutas,
            PoliticaConexion::Interactiva {
                solicitante: datos.solicitante,
                id_solicitud: sesion_id,
            },
            UsoCanal::Pestana,
        ) => tomada,
        _ = datos.cancelar.cancelled() => return,
    };
    let tomada = match tomada {
        Ok(tomada) => tomada,
        Err(motivo) => {
            apertura_fallida(&estado, &datos, &motivo).await;
            return;
        }
    };
    let handle = tomada.handle.clone();
    let (cols, filas) = (datos.cols, datos.filas);
    let canal = conexiones::abrir_con_plazo(PLAZO_CANAL, async move {
        abrir_canal(&handle, cols, filas).await
    })
    .await;
    let mut canal = match canal {
        Ok(canal) => canal,
        Err(motivo) => {
            tomada.soltar().await;
            apertura_fallida(&estado, &datos, &motivo).await;
            return;
        }
    };
    // Comandos iniciales (snippet al conectar, «abrir en pestaña»): se
    // escriben antes de dar la pestaña por abierta, tras la shell.
    for texto in &datos.comandos {
        let escrito =
            tokio::time::timeout(PLAZO_CANAL, canal.data_bytes(texto.clone().into_bytes())).await;
        if !matches!(escrito, Ok(Ok(()))) {
            warn!(sesion = sesion_id, "no se pudo escribir el comando inicial");
            break;
        }
    }

    let primera_en_conexion = {
        let mut estado_bloqueado = estado.lock().await;
        let otra_en_la_misma = estado_bloqueado.sesiones.values().any(|sesion| {
            sesion.id != sesion_id
                && sesion.estado == EstadoSesionRemota::Abierta
                && sesion
                    .handle
                    .as_ref()
                    .is_some_and(|handle| Arc::ptr_eq(handle, &tomada.handle))
        });
        let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) else {
            // La sesión se cerró mientras abría: se suelta lo tomado (por
            // identidad; la conexión del pool sigue sirviendo a los demás).
            drop(estado_bloqueado);
            let _ = canal.close().await;
            tomada.soltar().await;
            return;
        };
        sesion.estado = EstadoSesionRemota::Abierta;
        sesion.reconectando = false;
        sesion.motivo = None;
        sesion.identidad = tomada.identidad.clone();
        sesion.abierta_en = Utc::now().timestamp();
        sesion.handle = Some(tomada.handle.clone());
        !otra_en_la_misma
    };
    let bd = estado.lock().await.bd.clone();
    // `conexion_abierta` una sola vez por conexión: una segunda pestaña que
    // reutiliza la del pool no la vuelve a anotar.
    if reconexion || primera_en_conexion {
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
                datos.host.nombre, tomada.identidad
            ),
            resultado: crate::modelo::ResultadoRegistro::Ok,
        });
    }
    let _ = bd.send(super::OrdenBd::MarcarConexion { host_id });
    let _ = bd.send(super::OrdenBd::MarcarEstado {
        host_id,
        estado: Some(crate::modelo::UltimoEstado::Ok),
    });
    info!(
        sesion = sesion_id,
        host = %datos.host.nombre,
        reutilizada = !tomada.nueva,
        "sesión abierta en el servidor"
    );
    difundir_lista(&mut *estado.lock().await);
    // La pestaña ya cuenta para el ciclo automático de los túneles del host.
    let estado_tuneles = estado.clone();
    tokio::spawn(async move {
        super::tuneles::canales_cambiaron(&estado_tuneles, host_id).await;
    });

    bucle_sesion(
        estado,
        sesion_id,
        &mut canal,
        &datos.pantalla,
        &mut rx_comandos,
        tomada,
    )
    .await;
}

/// La apertura no llegó a abrir la pestaña: se quita, se anota y se avisa.
/// Solo anota quien retira la sesión: si ya la quitó otro (se cerró desde la
/// ventana, se fue la ventana, se apaga el servidor), ese lo anotó.
async fn apertura_fallida(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    datos: &DatosApertura,
    motivo: &str,
) {
    warn!(sesion = datos.sesion_id, host = %datos.host.nombre, "apertura fallida: {motivo}");
    let mut estado_bloqueado = estado.lock().await;
    estado_bloqueado.pendientes.remove(&datos.sesion_id);
    let seguia = estado_bloqueado.sesiones.remove(&datos.sesion_id).is_some();
    if seguia {
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::CONEXION_FALLIDA.to_string(),
            host_id: Some(datos.host.id),
            identidad_id: None,
            detalle: motivo.to_string(),
            resultado: crate::modelo::ResultadoRegistro::Error,
        });
        let _ = bd.send(super::OrdenBd::MarcarEstado {
            host_id: datos.host.id,
            estado: Some(crate::modelo::UltimoEstado::Error),
        });
    }
    avisar_fallida(&mut estado_bloqueado, datos.sesion_id, motivo);
    drop(estado_bloqueado);
    if seguia && datos.reconexion {
        // Una reconexión fallida se lleva la pestaña, que contaba para el
        // ciclo automático de los túneles del host.
        let estado_tuneles = estado.clone();
        let host_id = datos.host.id;
        tokio::spawn(async move {
            super::tuneles::canales_cambiaron(&estado_tuneles, host_id).await;
        });
    }
}

/// Avisa a todos los clientes de que la apertura acabó sin pestaña.
fn avisar_fallida(estado_bloqueado: &mut EstadoServidor, sesion_id: u32, motivo: &str) {
    difusion::difundir(
        &estado_bloqueado.clientes,
        MensajeServidor::Estado {
            sesion_id,
            estado: EstadoSesionRemota::Cerrada,
            motivo: Some(motivo.to_string()),
        },
    );
    difundir_lista(estado_bloqueado);
}

/// Traduce los eventos de la tarea de conexión a mensajes del protocolo:
/// diálogos solo al solicitante (con respuesta diferida), estados al resto.
/// La usan igual las sesiones, las aperturas de canal SFTP, los túneles y las
/// ejecuciones: `peticion_id` es el id de la solicitud de conexión. Sin
/// solicitante (un túnel automático), todo diálogo se contesta «no» al
/// momento: una operación automática no dialoga.
pub async fn puente_eventos(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    solicitante: Option<u32>,
    mut rx: mpsc::UnboundedReceiver<EventoConexion>,
) {
    while let Some(evento) = rx.recv().await {
        let Some(solicitante) = solicitante else {
            // Soltar el evento suelta su canal de respuesta: la conexión lo
            // lee como «cancelado». Los avisos sí se anotan.
            if let EventoConexion::HuellaRegistrada { .. } = evento {
                anotar_huella(&estado, evento).await;
            }
            continue;
        };
        match evento {
            EventoConexion::Estado { estado: fino, .. } => {
                let motivo = motivo_de_estado(fino).to_string();
                {
                    let mut estado_bloqueado = estado.lock().await;
                    if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&peticion_id) {
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
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Huella(responder),
                    MensajeServidor::HuellaDesconocida {
                        sesion_id: peticion_id,
                        host,
                        tipo_clave: tipo,
                        huella,
                    },
                    TIMEOUT_DECISION,
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
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Huella(responder),
                    MensajeServidor::HuellaCambiada {
                        sesion_id: peticion_id,
                        host,
                        tipo_clave: tipo,
                        anterior,
                        nueva,
                    },
                    TIMEOUT_DECISION,
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
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Frase(responder),
                    MensajeServidor::PideFrase {
                        sesion_id: peticion_id,
                        host,
                        intento,
                    },
                    TIMEOUT_DECISION,
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
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Contrasena(responder),
                    MensajeServidor::PideContrasena {
                        sesion_id: peticion_id,
                        host,
                        intento,
                        recordar_por_defecto,
                    },
                    TIMEOUT_DECISION,
                )
                .await;
            }
            EventoConexion::PideContrasenaLlavero {
                host,
                usuario,
                responder,
            } => {
                decidir(
                    &estado,
                    peticion_id,
                    solicitante,
                    DialogoPendiente::Llavero(responder),
                    MensajeServidor::PideLlavero {
                        sesion_id: peticion_id,
                        host,
                        usuario,
                    },
                    crate::conexion::cliente::PLAZO_LLAVERO,
                )
                .await;
            }
            // La huella ya se escribió en known_hosts: el efecto se anota aquí.
            evento @ EventoConexion::HuellaRegistrada { .. } => {
                anotar_huella(&estado, evento).await;
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

/// La huella ya está en known_hosts: se anota el efecto.
async fn anotar_huella(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, evento: EventoConexion) {
    let EventoConexion::HuellaRegistrada {
        host_id,
        anterior,
        nueva,
    } = evento
    else {
        return;
    };
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

/// Guarda una decisión pendiente, avisa al solicitante y al resto de clientes,
/// y programa su plazo (5 minutos si espera al usuario, 10 s si es el
/// llavero). `peticion_id` es el id de la solicitud de conexión: el de una
/// sesión o el de una apertura de SFTP, túnel o ejecución. Si el solicitante
/// ya no está conectado, nadie va a contestar: se descarta al momento.
pub async fn decidir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    solicitante: u32,
    dialogo: DialogoPendiente,
    mensaje: MensajeServidor,
    plazo: Duration,
) {
    let mut estado_bloqueado = estado.lock().await;
    if !estado_bloqueado.clientes.contains_key(&solicitante) {
        drop(dialogo);
        return;
    }
    let generacion = GENERACION_DIALOGO.fetch_add(1, Ordering::Relaxed);
    estado_bloqueado.pendientes.insert(
        peticion_id,
        Pendiente {
            solicitante,
            dialogo,
            generacion,
        },
    );
    let mut otros: Vec<u32> = Vec::new();
    for (id_cliente, cliente) in &estado_bloqueado.clientes {
        if *id_cliente == solicitante {
            difusion::enviar(cliente, mensaje.clone());
        } else {
            otros.push(*id_cliente);
        }
    }
    // Al resto solo se le avisa cuando la solicitud es una sesión: una apertura
    // de canal SFTP no tiene pestaña que enseñar en las demás ventanas.
    if estado_bloqueado.sesiones.contains_key(&peticion_id) {
        for id in otros {
            if let Some(cliente) = estado_bloqueado.clientes.get(&id) {
                difusion::enviar(
                    cliente,
                    MensajeServidor::Estado {
                        sesion_id: peticion_id,
                        estado: EstadoSesionRemota::Abriendo,
                        motivo: Some("esperando decisión en otra ventana".to_string()),
                    },
                );
            }
        }
    }
    // Plazo: si la decisión no llega, se descarta el canal de respuesta y la
    // apertura se cancela sola. Solo se retira el diálogo que se programó: el
    // mismo id puede estar ya en su siguiente intento.
    let estado_timeout = estado.clone();
    tokio::spawn(async move {
        tokio::time::sleep(plazo).await;
        let mut estado_bloqueado = estado_timeout.lock().await;
        let es_el_mismo = estado_bloqueado
            .pendientes
            .get(&peticion_id)
            .is_some_and(|pendiente| pendiente.generacion == generacion);
        if es_el_mismo {
            estado_bloqueado.pendientes.remove(&peticion_id);
            warn!(
                peticion = peticion_id,
                "decisión sin respuesta en {} s",
                plazo.as_secs()
            );
        }
    });
}

/// El solicitante renuncia a un diálogo pendiente (`Cerrar` sobre un id que
/// no es de sesión: una apertura de SFTP, túnel o ejecución). Se suelta su
/// canal de respuesta y la apertura falla enseguida en vez de esperar al plazo.
pub async fn cancelar_dialogo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    cliente_id: u32,
) -> bool {
    let mut estado_bloqueado = estado.lock().await;
    let es_suyo = estado_bloqueado
        .pendientes
        .get(&peticion_id)
        .is_some_and(|pendiente| pendiente.solicitante == cliente_id);
    if es_suyo {
        estado_bloqueado.pendientes.remove(&peticion_id);
    }
    es_suyo
}

/// Resuelve una decisión que llega por el protocolo. Solo el solicitante de la
/// solicitud puede responder; devuelve si se aceptó la respuesta.
pub async fn resolver_decision(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    peticion_id: u32,
    cliente_id: u32,
    decision: DecisionDialogo,
) -> bool {
    let mut estado_bloqueado = estado.lock().await;
    let Some(pendiente) = estado_bloqueado.pendientes.get(&peticion_id) else {
        warn!(peticion = peticion_id, "decisión sin diálogo pendiente");
        return false;
    };
    if pendiente.solicitante != cliente_id {
        warn!(
            peticion = peticion_id,
            cliente = cliente_id,
            "un cliente que no es el solicitante intentó responder a un diálogo"
        );
        return false;
    }
    let Some(pendiente) = estado_bloqueado.pendientes.remove(&peticion_id) else {
        return false;
    };
    match (pendiente.dialogo, decision) {
        (DialogoPendiente::Huella(responder), DecisionDialogo::Huella(d)) => {
            let _ = responder.send(d);
            true
        }
        (DialogoPendiente::Frase(responder), DecisionDialogo::Frase(frase)) => {
            let _ = responder.send(Some(frase.0));
            true
        }
        (
            DialogoPendiente::Contrasena(responder),
            DecisionDialogo::Contrasena(contrasena, recordar),
        ) => {
            let _ = responder.send(Some((contrasena.0, recordar)));
            true
        }
        (DialogoPendiente::Llavero(responder), DecisionDialogo::Contrasena(contrasena, _)) => {
            let _ = responder.send(Some(contrasena.0));
            true
        }
        (dialogo, _) => {
            drop(dialogo);
            warn!(
                peticion = peticion_id,
                "decisión que no casa con el diálogo"
            );
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
    tomada: ConexionTomada,
) {
    let mut vio_eof = false;
    loop {
        tokio::select! {
            comando = rx_comandos.recv() => match comando {
                Some(ComandoSesion::Teclas(bytes)) => {
                    if let Err(error) = canal.data_bytes(bytes).await {
                        caida(&estado, sesion_id, &format!("se perdió la conexión: {error}")).await;
                        tomada.soltar().await;
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
                    tomada.soltar().await;
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
                    tomada.soltar().await;
                    return;
                }
                None => {
                    if vio_eof {
                        cerrada(&estado, sesion_id, "el remoto cerró la sesión").await;
                    } else {
                        caida(&estado, sesion_id, "la conexión se ha perdido").await;
                    }
                    tomada.soltar().await;
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
    let mut host_id = None;
    let mut estado_bloqueado = estado.lock().await;
    if let Some(sesion) = estado_bloqueado.sesiones.remove(&sesion_id) {
        host_id = Some(sesion.host_id);
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::SESION_CERRADA.to_string(),
            host_id: Some(sesion.host_id),
            identidad_id: None,
            detalle: motivo.to_string(),
            resultado: crate::modelo::ResultadoRegistro::Ok,
        });
        // El canal del pool (o la conexión propia) lo suelta el bucle de la
        // sesión con su `ConexionTomada`, por identidad.
    }
    estado_bloqueado.pendientes.remove(&sesion_id);
    difundir_lista(&mut estado_bloqueado);
    drop(estado_bloqueado);
    if let Some(host_id) = host_id {
        // Si era la última pestaña o canal de ese host, sus túneles automáticos
        // se paran con ella.
        super::tuneles::canales_cambiaron(estado, host_id).await;
    }
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
        sesion.reconectando = false;
        sesion.motivo = Some(motivo.to_string());
        // El canal del pool lo suelta el bucle de la sesión justo después.
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

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tamano(cols: u16, filas: u16) -> Tamano {
        Tamano { cols, filas }
    }

    #[test]
    fn el_minimo_crece_cuando_crece_la_unica_ventana() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(200, 56));
        assert_eq!(tamano_minimo(&adjuntos, tamano(100, 26)), tamano(200, 56));
    }

    #[test]
    fn el_minimo_encoge_con_la_ventana() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(80, 20));
        assert_eq!(tamano_minimo(&adjuntos, tamano(100, 26)), tamano(80, 20));
    }

    #[test]
    fn sin_adjuntos_se_conserva_el_ultimo() {
        assert_eq!(
            tamano_minimo(&HashMap::new(), tamano(100, 26)),
            tamano(100, 26)
        );
        assert_eq!(
            ventana_minima_para(&HashMap::new(), tamano(100, 26), 1),
            None
        );
    }

    #[test]
    fn con_dos_ventanas_manda_la_menor_por_componente() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(200, 30));
        adjuntos.insert(2, tamano(120, 46));
        let minimo = tamano_minimo(&adjuntos, tamano(10, 10));
        assert_eq!(minimo, tamano(120, 30));
        // A la 1 le sobran columnas (las impone la 2); a la 2, filas (las
        // impone la 1): cada una ve la otra.
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 1), Some(2));
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 2), Some(1));
    }

    #[test]
    fn la_ventana_que_impone_todo_se_ve_a_si_misma() {
        let mut adjuntos = HashMap::new();
        adjuntos.insert(1, tamano(200, 46));
        adjuntos.insert(2, tamano(100, 26));
        let minimo = tamano_minimo(&adjuntos, tamano(10, 10));
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 1), Some(2));
        assert_eq!(ventana_minima_para(&adjuntos, minimo, 2), Some(2));
        adjuntos.insert(3, tamano(100, 26));
        assert_eq!(
            ventana_minima_para(&adjuntos, minimo, 1),
            Some(2),
            "empate: id menor"
        );
    }
}
