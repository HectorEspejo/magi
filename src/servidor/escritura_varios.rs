//! Escritura en varios del servidor (Fase 15): concede la escritura sobre un
//! conjunto de pestañas, recomprueba la deliberación y escribe solo en lo
//! concedido.
//!
//! Una concesión es trabajo de la ventana que la pidió: se termina con su
//! ventana, si cae una pestaña concedida, si el cliente la apaga o pasados
//! 10 minutos sin envíos (`7.7). El servidor anota `escritura_varios_*` y
//! rellena `DELIBERACIONES.ejecucion_resultado` si la concesión deliberó;
//! nunca guarda lo tecleado.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tracing::warn;

use magi_nucleo::deliberacion::{EjecucionResultado, ResultadoDeliberacion};

use crate::modelo::ResultadoRegistro;
use crate::protocolo::{BytesPestana, DeliberacionLanzada, MensajeServidor};

use super::conexiones;
use super::ejecuciones::{self, DeliberacionFijada};
use super::sesiones::ComandoSesion;
use super::{difusion, EstadoServidor};

/// Sin envíos durante este tiempo, la escritura en varios se apaga sola.
pub const INACTIVIDAD: Duration = Duration::from_secs(10 * 60);

/// Cada cuánto se revisa la inactividad de las concesiones.
const REVISION: Duration = Duration::from_secs(5);

/// Una escritura en varios concedida.
pub struct Concesion {
    pub id: u32,
    pub cliente: u32,
    pub sesiones: Vec<u32>,
    /// Hosts de las pestañas concedidas, ordenados por id y sin repetir.
    pub hosts: Vec<i64>,
    /// La deliberación que la autorizó, si hizo falta.
    pub deliberacion: Option<DeliberacionFijada>,
    pub desde: i64,
    pub envios: u64,
    pub ultimo_envio: Instant,
}

/// Las concesiones vivas del servidor.
#[derive(Default)]
pub struct Escrituras {
    pub activas: HashMap<u32, Concesion>,
    siguiente_id: u32,
    /// Deliberaciones ya concedidas en este proceso (aunque la concesión haya
    /// terminado): una deliberación autoriza una sola escritura.
    consumidas: HashSet<i64>,
}

impl Escrituras {
    fn nuevo_id(&mut self) -> u32 {
        self.siguiente_id += 1;
        self.siguiente_id
    }

    /// ¿Autorizó ya esta deliberación una escritura en varios?
    pub fn deliberacion_usada(&self, deliberacion_id: i64) -> bool {
        self.consumidas.contains(&deliberacion_id)
            || self.activas.values().any(|concesion| {
                concesion
                    .deliberacion
                    .as_ref()
                    .is_some_and(|fijada| fijada.registro.id == deliberacion_id)
            })
    }

    /// ¿Hay alguna escritura en varios viva?
    pub fn hay_activas(&self) -> bool {
        !self.activas.is_empty()
    }

    /// Saca todas las concesiones (para el apagado).
    fn vaciar(&mut self) -> Vec<Concesion> {
        self.activas
            .drain()
            .map(|(_, concesion)| concesion)
            .collect()
    }
}

/// ¿Autorizó ya esta deliberación una escritura en varios (activa o ya
/// consumida en este proceso)? La usa `ejecuciones::deliberacion_usada` para
/// que «ya usada» valga en los dos sentidos.
pub(crate) fn deliberacion_usada(estado: &EstadoServidor, deliberacion_id: i64) -> bool {
    estado.escrituras.deliberacion_usada(deliberacion_id)
}

/// `IniciarEscrituraVarios`: recomprueba pestañas abiertas, hosts y
/// deliberación válida y sin usar; responde `EscrituraVariosIniciada` o
/// `Error`.
pub async fn iniciar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    sesiones_ids: Vec<u32>,
    deliberacion_id: Option<i64>,
) {
    // 1. Las pestañas tienen que existir y estar abiertas, y ser dos o más
    //    distintas (una repetida no es un panel más).
    let mut vistas = HashSet::new();
    let sesiones_ids: Vec<u32> = sesiones_ids
        .into_iter()
        .filter(|sesion_id| vistas.insert(*sesion_id))
        .collect();
    let mut pestanas: Vec<(u32, i64, String)> = Vec::new();
    {
        let estado_bloqueado = estado.lock().await;
        for sesion_id in &sesiones_ids {
            let Some(sesion) = estado_bloqueado.sesiones.get(sesion_id) else {
                drop(estado_bloqueado);
                responder_error(
                    estado,
                    cliente_id,
                    peticion_id,
                    "una de las pestañas ya no existe",
                )
                .await;
                return;
            };
            if !sesion.abierta() {
                drop(estado_bloqueado);
                responder_error(
                    estado,
                    cliente_id,
                    peticion_id,
                    "todas las pestañas tienen que estar abiertas",
                )
                .await;
                return;
            }
            pestanas.push((*sesion_id, sesion.host_id, sesion.host_nombre.clone()));
        }
    }
    if pestanas.len() < 2 {
        responder_error(
            estado,
            cliente_id,
            peticion_id,
            "hace falta más de un panel para escribir en varios",
        )
        .await;
        return;
    }
    let mut hosts: Vec<i64> = pestanas.iter().map(|(_, host, _)| *host).collect();
    hosts.sort_unstable();
    hosts.dedup();

    // 2. ¿Hace falta deliberación? (más de un host o verificaciones activas)
    let necesita = hosts.len() > 1 || verificaciones_activas(estado, &hosts).await;
    let fijada = if necesita {
        let Some(deliberacion_id) = deliberacion_id else {
            responder_error(
                estado,
                cliente_id,
                peticion_id,
                "esta escritura en varios necesita una deliberación",
            )
            .await;
            return;
        };
        match validar_deliberacion(estado, deliberacion_id, &hosts).await {
            Ok(fijada) => Some(fijada),
            Err(motivo) => {
                responder_error(estado, cliente_id, peticion_id, &motivo).await;
                return;
            }
        }
    } else {
        None
    };

    // 3. La concesión, comprobando en el mismo bloqueo que no la ha tomado
    //    otra ventana mientras se leía la deliberación.
    let id = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            drop(estado_bloqueado);
            responder_error(estado, cliente_id, peticion_id, conexiones::APAGANDO).await;
            return;
        }
        for (sesion_id, _, _) in &pestanas {
            if !estado_bloqueado
                .sesiones
                .get(sesion_id)
                .is_some_and(|sesion| sesion.abierta())
            {
                drop(estado_bloqueado);
                responder_error(
                    estado,
                    cliente_id,
                    peticion_id,
                    "una pestaña dejó de estar abierta",
                )
                .await;
                return;
            }
        }
        if let Some(fijada) = &fijada {
            if ejecuciones::deliberacion_usada(&estado_bloqueado, fijada.registro.id) {
                drop(estado_bloqueado);
                responder_error(
                    estado,
                    cliente_id,
                    peticion_id,
                    "esa deliberación ya autorizó otra acción",
                )
                .await;
                return;
            }
        }
        let id = estado_bloqueado.escrituras.nuevo_id();
        if let Some(fijada) = &fijada {
            estado_bloqueado
                .escrituras
                .consumidas
                .insert(fijada.registro.id);
        }
        estado_bloqueado.escrituras.activas.insert(
            id,
            Concesion {
                id,
                cliente: cliente_id,
                sesiones: pestanas
                    .iter()
                    .map(|(sesion_id, _, _)| *sesion_id)
                    .collect(),
                hosts: hosts.clone(),
                deliberacion: fijada.clone(),
                desde: Utc::now().timestamp(),
                envios: 0,
                ultimo_envio: Instant::now(),
            },
        );
        // La marca «»» que ven todas las ventanas (§7.14): solo las pestañas
        // concedidas, y la lista se difunde entera al cambiar.
        for sesion_id in pestanas.iter().map(|(sesion_id, _, _)| *sesion_id) {
            if let Some(sesion) = estado_bloqueado.sesiones.get_mut(&sesion_id) {
                sesion.escritura_varios = true;
            }
        }
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::ESCRITURA_VARIOS_ACTIVADA.to_string(),
            host_id: pestanas.first().map(|(_, host, _)| *host),
            identidad_id: None,
            detalle: format!(
                "escritura en varios: {} · {} pestaña(s) · {}",
                nombres_desde_hosts(&pestanas),
                pestanas.len(),
                match &fijada {
                    Some(fijada) => format!("deliberación #{}", fijada.registro.id),
                    None => "confirmada".to_string(),
                }
            ),
            resultado: ResultadoRegistro::Ok,
        });
        id
    };
    super::sesiones::difundir_lista(&mut *estado.lock().await);

    responder(
        estado,
        cliente_id,
        MensajeServidor::EscrituraVariosIniciada { peticion_id, id },
    )
    .await;
}

/// `EscribirVarios`: solo en las pestañas concedidas; si alguna no lo está o
/// se cayó, se rechaza entero y se termina la concesión.
pub async fn escribir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    id: u32,
    pestanas: Vec<BytesPestana>,
) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(concesion) = estado_bloqueado.escrituras.activas.get(&id) else {
        return;
    };
    if concesion.cliente != cliente_id {
        warn!(
            cliente = cliente_id,
            concesion = id,
            "un cliente que no es el dueño escribió en varios"
        );
        return;
    }
    let mut motivo = None;
    for pestana in &pestanas {
        if !concesion.sesiones.contains(&pestana.sesion_id) {
            motivo = Some("pestaña no concedida".to_string());
            break;
        }
        match estado_bloqueado.sesiones.get(&pestana.sesion_id) {
            Some(sesion) if sesion.abierta() => {}
            Some(_) => {
                motivo = Some("una pestaña concedida se cayó".to_string());
                break;
            }
            None => {
                motivo = Some("una pestaña concedida se cerró".to_string());
                break;
            }
        }
    }
    if let Some(motivo) = motivo {
        drop(estado_bloqueado);
        terminar(estado, id, &motivo, EjecucionResultado::Cancelada).await;
        return;
    }
    for pestana in &pestanas {
        if let Some(sesion) = estado_bloqueado.sesiones.get(&pestana.sesion_id) {
            let _ = sesion
                .tx_comandos
                .send(ComandoSesion::Teclas(pestana.bytes.clone()));
        }
    }
    if let Some(concesion) = estado_bloqueado.escrituras.activas.get_mut(&id) {
        concesion.envios += 1;
        concesion.ultimo_envio = Instant::now();
    }
}

/// `TerminarEscrituraVarios`: la apaga la ventana que la tenía concedida.
pub async fn terminar_cliente(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    id: u32,
    motivo: &str,
) {
    {
        let estado_bloqueado = estado.lock().await;
        let Some(concesion) = estado_bloqueado.escrituras.activas.get(&id) else {
            return;
        };
        if concesion.cliente != cliente_id {
            warn!(
                cliente = cliente_id,
                concesion = id,
                "un cliente que no es el dueño terminó la escritura en varios"
            );
            return;
        }
    }
    terminar(estado, id, motivo, EjecucionResultado::Ok).await;
}

/// Termina una concesión: anota duración y envíos, rellena la deliberación y
/// difunde `EscrituraVariosTerminada`. Nunca guarda lo tecleado.
async fn terminar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    id: u32,
    motivo: &str,
    resultado: EjecucionResultado,
) {
    // El motivo lo manda el cliente: se sanea antes de ir a REGISTRO y a las
    // demás ventanas (nunca lleva lo tecleado, pero sí puede traer controles).
    let motivo = limpiar_motivo(motivo);
    let Some(concesion) = ({
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado.escrituras.activas.remove(&id)
    }) else {
        return;
    };
    let duracion = (Utc::now().timestamp() - concesion.desde).max(0);
    let duracion_texto = if duracion >= 60 {
        format!("{} min", duracion / 60)
    } else {
        format!("{duracion} s")
    };
    let bd = estado.lock().await.bd.clone();
    let _ = bd.send(super::OrdenBd::Anotar {
        tipo: crate::registro::ESCRITURA_VARIOS_TERMINADA.to_string(),
        host_id: concesion.hosts.first().copied(),
        identidad_id: None,
        detalle: format!(
            "{duracion_texto} · {} envío(s) · {motivo}",
            concesion.envios
        ),
        resultado: if resultado == EjecucionResultado::Error {
            ResultadoRegistro::Error
        } else {
            ResultadoRegistro::Ok
        },
    });
    if let Some(fijada) = &concesion.deliberacion {
        ejecuciones::anotar_deliberacion(&bd, fijada, resultado);
    }
    let estado_bloqueado = {
        let mut estado_bloqueado = estado.lock().await;
        // La marca «»» desaparece de las pestañas concedidas en cuanto termina,
        // sea cual sea la causa, y la lista nueva va a todas las ventanas.
        for sesion_id in &concesion.sesiones {
            if let Some(sesion) = estado_bloqueado.sesiones.get_mut(sesion_id) {
                sesion.escritura_varios = false;
            }
        }
        super::sesiones::difundir_lista(&mut estado_bloqueado);
        estado_bloqueado
    };
    difusion::difundir(
        &estado_bloqueado.clientes,
        MensajeServidor::EscrituraVariosTerminada {
            id,
            motivo: motivo.clone(),
        },
    );
}

/// Motivo legible y sin controles para REGISTRO y para las ventanas: como
/// mucho 80 caracteres.
fn limpiar_motivo(motivo: &str) -> String {
    let limpio: String = motivo
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(80)
        .collect();
    let limpio = limpio.trim();
    if limpio.is_empty() {
        "terminada".to_string()
    } else {
        limpio.to_string()
    }
}

/// Una pestaña concedida se cerró o cayó: su concesión termina.
pub async fn sesion_acabo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    motivo: &str,
) {
    let ids: Vec<u32> = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escrituras
            .activas
            .values()
            .filter(|concesion| concesion.sesiones.contains(&sesion_id))
            .map(|concesion| concesion.id)
            .collect()
    };
    for id in ids {
        terminar(estado, id, motivo, EjecucionResultado::Cancelada).await;
    }
}

/// La ventana que la pidió se cerró: sus concesiones terminan.
pub async fn cliente_acabo(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, cliente_id: u32) {
    let ids: Vec<u32> = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escrituras
            .activas
            .values()
            .filter(|concesion| concesion.cliente == cliente_id)
            .map(|concesion| concesion.id)
            .collect()
    };
    for id in ids {
        terminar(estado, id, "ventana cerrada", EjecucionResultado::Cancelada).await;
    }
}

/// Revisora de inactividad: a los 10 min sin envíos se apaga sola.
pub async fn revisar(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    loop {
        tokio::time::sleep(REVISION).await;
        let ids: Vec<u32> = {
            let estado_bloqueado = estado.lock().await;
            estado_bloqueado
                .escrituras
                .activas
                .values()
                .filter(|concesion| concesion.ultimo_envio.elapsed() >= INACTIVIDAD)
                .map(|concesion| concesion.id)
                .collect()
        };
        for id in ids {
            terminar(&estado, id, "inactividad", EjecucionResultado::Cancelada).await;
        }
    }
}

/// Concesiones que quedan al apagar el servidor (se anotan y se vacían).
pub async fn apagar(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let concesiones = {
        let mut estado_bloqueado = estado.lock().await;
        let concesiones = estado_bloqueado.escrituras.vaciar();
        // Al apagarse, las marcas «»» también desaparecen de la lista que
        // reciban las ventanas que sigan conectadas.
        for concesion in &concesiones {
            for sesion_id in &concesion.sesiones {
                if let Some(sesion) = estado_bloqueado.sesiones.get_mut(sesion_id) {
                    sesion.escritura_varios = false;
                }
            }
        }
        concesiones
    };
    if !concesiones.is_empty() {
        super::sesiones::difundir_lista(&mut *estado.lock().await);
    }
    for concesion in concesiones {
        let bd = estado.lock().await.bd.clone();
        let duracion = (Utc::now().timestamp() - concesion.desde).max(0);
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::ESCRITURA_VARIOS_TERMINADA.to_string(),
            host_id: concesion.hosts.first().copied(),
            identidad_id: None,
            detalle: format!(
                "{} s · {} envío(s) · el servidor se apaga",
                duracion, concesion.envios
            ),
            resultado: ResultadoRegistro::Ok,
        });
        if let Some(fijada) = &concesion.deliberacion {
            ejecuciones::anotar_deliberacion(&bd, fijada, EjecucionResultado::Cancelada);
        }
    }
}

/// ¿Alguna de estas verificaciones previas está activa? (T41: si la hay, la
/// escritura en varios delibera.)
async fn verificaciones_activas(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    hosts: &[i64],
) -> bool {
    let lectura = estado.lock().await.lectura.clone();
    let verificaciones = lectura
        .lock()
        .ok()
        .and_then(|almacen| almacen.verificaciones_por_host().ok())
        .unwrap_or_default();
    hosts.iter().any(|host| {
        verificaciones
            .get(host)
            .is_some_and(|verificaciones| verificaciones.alguna_activa())
    })
}

/// Lee y valida la deliberación de escritura en varios: resultado aprobada o
/// forzada, sin usar, con la fila de una escritura en varios (snippet NULL) y
/// con exactamente los hosts de las pestañas concedidas, ordenados por id.
async fn validar_deliberacion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    deliberacion_id: i64,
    hosts: &[i64],
) -> Result<DeliberacionFijada, String> {
    let lectura = estado.lock().await.lectura.clone();
    let registro = {
        let lectura = lectura.lock().map_err(|_| "almacén envenenado")?;
        lectura.obtener_deliberacion(deliberacion_id).map_err(|_| {
            format!("la deliberación #{deliberacion_id} no existe o no se puede leer")
        })?
    };
    if registro.snippet_id.is_some() {
        return Err(format!(
            "la deliberación #{deliberacion_id} no autoriza una escritura en varios"
        ));
    }
    if !registro
        .accion
        .to_lowercase()
        .starts_with("escritura en varios")
    {
        return Err(format!(
            "la deliberación #{deliberacion_id} no es de una escritura en varios"
        ));
    }
    let forzada = match registro.resultado {
        ResultadoDeliberacion::Aprobada => false,
        ResultadoDeliberacion::Forzada => {
            if registro
                .motivo
                .as_deref()
                .is_none_or(|motivo| motivo.trim().is_empty())
            {
                return Err(format!(
                    "la deliberación #{deliberacion_id} está forzada sin motivo"
                ));
            }
            true
        }
        ResultadoDeliberacion::Cancelada => {
            return Err(format!("la deliberación #{deliberacion_id} se canceló"));
        }
    };
    let lanzada = DeliberacionLanzada {
        id: deliberacion_id,
        forzada,
        motivo: registro.motivo.clone(),
    };
    let fijada = {
        let lectura = lectura.lock().map_err(|_| "almacén envenenado")?;
        ejecuciones::validar_deliberacion(&lectura, &lanzada)?
    };
    let mut del_hosts: Vec<i64> = fijada.registro.hosts.iter().map(|(id, _)| *id).collect();
    del_hosts.sort_unstable();
    del_hosts.dedup();
    if del_hosts != hosts {
        return Err(format!(
            "la deliberación #{deliberacion_id} no corresponde a estas pestañas"
        ));
    }
    Ok(fijada)
}

fn nombres_desde_hosts(pestanas: &[(u32, i64, String)]) -> String {
    let mut nombres: Vec<String> = Vec::new();
    for (_, _, nombre) in pestanas {
        if !nombres.contains(nombre) {
            nombres.push(nombre.clone());
        }
    }
    nombres.join(" · ")
}

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
    peticion_id: u64,
    motivo: &str,
) {
    responder(
        estado,
        cliente_id,
        MensajeServidor::Error {
            mensaje: motivo.to_string(),
            peticion_id: Some(peticion_id),
        },
    )
    .await;
}
