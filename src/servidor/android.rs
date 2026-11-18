//! Lo que el servidor hace por Android cuando corre en Termux (Fase 17,
//! §4.6, §4.7 y desviación 2): el wake lock, los avisos de la barra de
//! notificaciones y lo que le dice a la barra del cliente por el fichero de
//! estado `servidor`.
//!
//! Todo esto cuelga de `EstadoServidor.termux`, que es `None` fuera de Termux:
//! con `None` el servidor es exactamente el del escritorio. No toca el
//! protocolo (sigue en la versión 10): el servidor habla con Android por las
//! órdenes `termux-*` de `termux::ordenes`, siempre con plazo y sin shell, y
//! con el cliente por el fichero de estado.
//!
//! - **Wake lock.** El conductor de `termux::wake_lock`; `revisar_inactividad`
//!   le dice cada segundo si hay trabajo (`TrabajoServidor` no vacío) y el
//!   apagado le dice que termine.
//! - **Avisos.** Una sola tarea reconciliadora ([`reconciliar`]), despertada
//!   por un `Notify` y con un tic de unos 3 s, compara los avisos que tocan
//!   ([`avisos::deseados`]) con los publicados y publica o retira la
//!   diferencia. Los ids publicados se anotan en el estado antes de publicar,
//!   para que un servidor que muera deje el rastro de lo que hay que retirar.
//!   Nada del inventario, del remoto ni de la salida de un programa va en un
//!   aviso, ni al log.
//! - **Barra.** [`BarraServidor`] guarda el `arranque` y los avisos nuevos
//!   (Termux:API apagada, sin wake lock, límite de procesos) en el fichero
//!   `servidor`, que el cliente mira cada pocos segundos.
//!
//! Termux:API es del proceso: si una orden suya no responde o falla, la
//! función se apaga en este servidor (se dice una vez en la barra) y no se
//! publica ni se retira nada más; lo publicado queda en el estado y lo retira
//! el siguiente servidor.
//!
//! Retirar avisos (la purga de arranque, el ciclo y el cierre) lanza como
//! mucho [`RETIRADAS_A_LA_VEZ`] órdenes a la vez y empieza por una sola, que
//! sondea Termux:API: en un móvil, cada `termux-notification-remove` arranca
//! un proceso de Android, y decenas a la vez dan plazos vencidos falsos que
//! apagarían la función.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use tracing::{debug, warn};

use crate::config::{Config, Rutas};
use crate::protocolo::{EstadoFicha, ServidorTmux};
use crate::termux::avisos::{self, Deseado, EntradaFicha, EntradaPestana};
use crate::termux::estado::{self, AvisoServidor, EstadoServidorTermux};
use crate::termux::ordenes::FalloOrden;
use crate::termux::wake_lock::{DiceAvisoBarra, WakeLock, GRACIA_SOLTAR};
use crate::termux::{ApiProceso, EntornoTermux, Ordenes, Propiedades};

use super::EstadoServidor;

/// Cada cuánto mira la tarea de avisos si algo cambió aunque nadie la haya
/// despertado (§4.7: un tic de unos 3 s).
pub const TIC_AVISOS: Duration = Duration::from_secs(3);

/// Órdenes de retirada de avisos que corren a la vez como mucho (purga de
/// arranque, ciclo y cierre).
pub const RETIRADAS_A_LA_VEZ: usize = 4;

/// La orden que publica los avisos: sin ella (el paquete `termux-api` solo se
/// sugiere) no hay avisos de Android posibles.
const ORDEN_DE_AVISOS: &str = "termux-notification";

/// Lo que se le da al servidor para que se porte como el de Termux. En
/// producción sale del proceso ([`Self::del_proceso`]); las pruebas lo
/// inyectan con unos dobles de `termux-*` y unas propiedades de Android de
/// mentira, y aceleran los tiempos.
#[derive(Debug, Clone)]
pub struct OpcionesTermux {
    /// El entorno del que sale el `PATH` donde se buscan las órdenes `termux-*`.
    pub entorno: EntornoTermux,
    /// Las propiedades de Android (el SDK decide el aviso del límite de
    /// procesos).
    pub propiedades: Propiedades,
    /// Cuánto sigue tomado el wake lock tras quedar sin trabajo: 30 s; las
    /// pruebas lo acortan.
    pub gracia_wake_lock: Duration,
    /// El tic de la tarea de avisos: 3 s; las pruebas lo acortan.
    pub tic_avisos: Duration,
    /// Cuenta las veces que se despierta a la tarea de avisos. Las pruebas
    /// guardan una copia del `Arc` para leerla: una salida con muchas
    /// campanas no debe despertarla una vez por lote.
    pub despertares: Arc<AtomicU64>,
}

impl OpcionesTermux {
    /// Con los tiempos de producción.
    #[must_use]
    pub fn nuevas(entorno: EntornoTermux, propiedades: Propiedades) -> Self {
        Self {
            entorno,
            propiedades,
            gracia_wake_lock: GRACIA_SOLTAR,
            tic_avisos: TIC_AVISOS,
            despertares: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Las de este proceso, solo si es un MAGI de Termux (`es_termux()`).
    #[must_use]
    pub fn del_proceso() -> Option<Self> {
        crate::termux::es_termux()
            .then(|| Self::nuevas(EntornoTermux::del_proceso(), Propiedades::del_sistema()))
    }
}

// ------------------------------------------------------------------- barra

/// Lo que el servidor le dice a la barra del cliente (desviación 2): el
/// fichero de estado `servidor`, con el `arranque` de este servidor y los
/// avisos que ha ido teniendo. Lo reescribe entero cada vez que cambia.
pub struct BarraServidor {
    rutas: Rutas,
    estado: StdMutex<EstadoServidorTermux>,
}

impl BarraServidor {
    /// Escribe el fichero con el `arranque` de ahora y sin avisos: un cliente
    /// que leyera lo de un servidor anterior no debe contarlo.
    #[must_use]
    pub fn nueva(rutas: Rutas) -> Arc<Self> {
        let barra = Arc::new(Self {
            rutas,
            estado: StdMutex::new(EstadoServidorTermux {
                arranque: chrono::Utc::now().timestamp(),
                avisos: Vec::new(),
            }),
        });
        barra.escribir(&barra.cerrojo());
        barra
    }

    fn cerrojo(&self) -> std::sync::MutexGuard<'_, EstadoServidorTermux> {
        self.estado
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
    }

    fn escribir(&self, estado: &EstadoServidorTermux) {
        if let Err(error) = estado::escribir_servidor(&self.rutas, estado) {
            warn!("no se pudo escribir el estado del servidor para la barra: {error:#}");
        }
    }

    /// Añade un aviso (cada uno una sola vez por servidor).
    pub fn decir(&self, aviso: AvisoServidor) {
        let mut estado = self.cerrojo();
        if estado.avisos.contains(&aviso) {
            return;
        }
        estado.avisos.push(aviso);
        self.escribir(&estado);
    }

    /// Los avisos dichos hasta ahora.
    #[must_use]
    pub fn avisos(&self) -> Vec<AvisoServidor> {
        self.cerrojo().avisos.clone()
    }

    /// Al terminar: el siguiente cliente no debe leer lo que dijo un servidor
    /// que ya no está.
    pub fn quitar(&self) {
        if let Err(error) = estado::quitar_servidor(&self.rutas) {
            warn!("no se pudo quitar el estado del servidor: {error:#}");
        }
    }
}

// ------------------------------------------------------------ el servidor

/// Lo de Termux del servidor: las órdenes, el conductor del wake lock, la
/// barra y los avisos publicados.
pub struct TermuxServidor {
    pub ordenes: Arc<Ordenes>,
    pub wake_lock: Arc<WakeLock>,
    pub barra: Arc<BarraServidor>,
    rutas: Rutas,
    despertar: Notify,
    /// Cuántas veces se ha despertado a la tarea de avisos (la misma cuenta
    /// que `OpcionesTermux::despertares`, para las pruebas: una salida con
    /// muchas campanas no debe lanzar un ciclo por lote).
    despertares: Arc<AtomicU64>,
    tic_avisos: Duration,
    /// Los ids de los avisos publicados. Un solo dueño a la vez (el cerrojo
    /// cubre un ciclo entero): el ciclo de la tarea, la purga y el cierre no
    /// se pisan.
    publicados: Mutex<BTreeSet<String>>,
    /// El servidor ya está terminando: la tarea de avisos y la purga no
    /// lanzan órdenes nuevas (solo las del propio cierre).
    terminando: AtomicBool,
    /// ¿Está `termux-notification` en el `PATH` de las órdenes? Se mira una
    /// vez, al arrancar, sin ejecutarla.
    notificacion_instalada: bool,
}

impl TermuxServidor {
    /// Arma lo de Termux del servidor: las órdenes con el plazo de
    /// `[termux] api_plazo_seg` y su propio estado de Termux:API, la barra
    /// (que escribe el fichero `servidor` con el `arranque`) y el conductor
    /// del wake lock. Los ids que dejó un servidor anterior (estado `avisos`)
    /// quedan como publicados: la tarea de avisos los retira al empezar.
    #[must_use]
    pub fn nuevo(opciones: OpcionesTermux, config: &Config, rutas: &Rutas) -> Arc<Self> {
        let notificacion_instalada =
            orden_en_el_path(ORDEN_DE_AVISOS, opciones.entorno.path.as_deref());
        let ordenes = Arc::new(Ordenes::nuevas(
            &opciones.entorno,
            config.termux.api_plazo(),
            Arc::new(ApiProceso::default()),
        ));
        let barra = BarraServidor::nueva(rutas.clone());
        let decir: DiceAvisoBarra = {
            let barra = Arc::clone(&barra);
            Arc::new(move |aviso| barra.decir(aviso))
        };
        let wake_lock = WakeLock::nuevo(
            Arc::clone(&ordenes),
            rutas.clone(),
            config.termux.wake_lock_auto(),
            opciones.gracia_wake_lock,
            opciones.propiedades.sdk(),
            decir,
        );
        Arc::new(Self {
            ordenes,
            wake_lock,
            barra,
            rutas: rutas.clone(),
            despertar: Notify::new(),
            despertares: opciones.despertares,
            tic_avisos: opciones.tic_avisos,
            publicados: Mutex::new(estado::leer_avisos(rutas)),
            terminando: AtomicBool::new(false),
            notificacion_instalada,
        })
    }

    /// Avisa a la tarea de avisos de que algo pudo cambiar (la atención de una
    /// pestaña, la lista de sesiones o la del vigilante). No espera a nada.
    /// Cada llamada cuesta un ciclo completo de la tarea: quien la hace solo
    /// la hace cuando algo cambia de verdad.
    pub fn despertar_avisos(&self) {
        self.despertares.fetch_add(1, Ordering::Relaxed);
        self.despertar.notify_one();
    }

    /// ¿Está terminando el servidor? Desde ese momento solo el cierre lanza
    /// órdenes de avisos.
    #[must_use]
    pub fn terminando(&self) -> bool {
        self.terminando.load(Ordering::Acquire)
    }

    /// ¿Se publican avisos de Android? Con `[termux] avisos = true` y
    /// Termux:API sin apagar. Con ventanas, es lo que decide si la tarea de
    /// avisos publica; sin ellas, el vigilante de las sesiones persistentes
    /// pide además [`Self::vigila_sin_ventanas`].
    #[must_use]
    pub fn avisos_activos(&self, config: &Config) -> bool {
        config.termux.avisos && !self.ordenes.api().apagada()
    }

    /// ¿Debe el vigilante de las sesiones persistentes trabajar aunque no haya
    /// ventanas? Solo si los avisos están activos y hay con qué publicarlos:
    /// sin `termux-notification` en el `PATH` (el paquete `termux-api` es
    /// opcional) sondear el host sin ventanas no serviría de nada, y el
    /// servidor sigue como el del escritorio (§4.7, punto 4). No se ejecuta
    /// nada para saberlo ni se dice nada en la barra.
    #[must_use]
    pub fn vigila_sin_ventanas(&self, config: &Config) -> bool {
        self.notificacion_instalada && self.avisos_activos(config)
    }

    /// Los ids publicados ahora (para las pruebas).
    pub async fn publicados(&self) -> BTreeSet<String> {
        self.publicados.lock().await.clone()
    }
}

/// ¿Hay un `programa` ejecutable en ese `PATH`? Es la regla con la que
/// `termux::ordenes` resuelve una orden al lanzarla, sin lanzarla (la misma
/// función: una orden con `/` no se busca y las entradas vacías del `PATH` no
/// cuentan como «aquí»).
fn orden_en_el_path(programa: &str, path: Option<&OsStr>) -> bool {
    crate::termux::ordenes::buscar_en_path(programa, path).is_some()
}

// ---------------------------------------------------------- lo que se mira

/// Lo que la tarea necesita del estado del servidor para decidir, copiado
/// con el cerrojo y soltado enseguida.
struct Instantanea {
    fichas: Vec<EntradaFicha>,
    pestanas: Vec<EntradaPestana>,
    /// Nombres de host que ya conoce el servidor por sus pestañas.
    hosts: HashMap<i64, String>,
    lectura: Arc<std::sync::Mutex<crate::almacen::Almacen>>,
    avisos_activos: bool,
}

fn instantanea(estado: &EstadoServidor, termux: &TermuxServidor) -> Instantanea {
    let fichas = estado
        .persistentes
        .lista_actual()
        .fichas
        .iter()
        .map(|ficha| EntradaFicha {
            uuid: ficha.uuid.clone(),
            host_id: ficha.host_id,
            nombre: ficha.nombre.clone(),
            pide: match ficha.estado {
                EstadoFicha::VivaLibre | EstadoFicha::VivaEnganchada => {
                    Some(ficha.sesion.as_ref().is_some_and(|sesion| sesion.campana))
                }
                EstadoFicha::Terminada => Some(false),
                // No se sabe: un aviso ya publicado se conserva.
                EstadoFicha::SinComprobar | EstadoFicha::Inalcanzable | EstadoFicha::SinTmux => {
                    None
                }
            },
        })
        .collect();
    let pestanas = estado
        .sesiones
        .values()
        .map(|sesion| EntradaPestana {
            id: sesion.id,
            nombre: sesion.nombre.clone(),
            host_id: sesion.host_id,
            host: sesion.host_nombre.clone(),
            persistente: sesion
                .tmux
                .clone()
                .filter(|_| sesion.tmux_servidor == ServidorTmux::Magi),
            pide: sesion.atencion_remota,
            enfocada: sesion.enfocada(),
        })
        .collect();
    let hosts = estado
        .sesiones
        .values()
        .map(|sesion| (sesion.host_id, sesion.host_nombre.clone()))
        .collect();
    Instantanea {
        fichas,
        pestanas,
        hosts,
        lectura: Arc::clone(&estado.lectura),
        avisos_activos: termux.avisos_activos(&estado.config),
    }
}

/// El nombre de un host para el texto de un aviso: el de una pestaña abierta
/// en él o, si no, el del inventario. Sin nada, el texto genérico de
/// `avisos::texto`.
async fn nombre_de_host(instantanea: &Instantanea, host_id: i64) -> String {
    if let Some(nombre) = instantanea.hosts.get(&host_id) {
        return nombre.clone();
    }
    let lectura = Arc::clone(&instantanea.lectura);
    tokio::task::spawn_blocking(move || {
        lectura
            .lock()
            .ok()
            .and_then(|almacen| almacen.obtener_host(host_id).ok())
            .map(|host| host.nombre)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

// ------------------------------------------------------ la tarea de avisos

/// La tarea reconciliadora de los avisos de Android. Se lanza una vez al
/// arrancar el servidor y termina cuando éste se apaga. Primero retira lo que
/// dejó un servidor anterior (estado `avisos`); luego, en cada despertar o
/// tic, deja publicado exactamente lo que toca.
pub async fn reconciliar(estado: Arc<Mutex<EstadoServidor>>) {
    let Some(termux) = estado.lock().await.termux.clone() else {
        return;
    };
    purgar_los_de_antes(&termux).await;
    loop {
        if estado.lock().await.apagando {
            return;
        }
        ciclo(&estado, &termux).await;
        tokio::select! {
            () = termux.despertar.notified() => {}
            () = tokio::time::sleep(termux.tic_avisos) => {}
        }
    }
}

/// Guarda los ids publicados (o borra el fichero si no queda ninguno).
async fn guardar(rutas: &Rutas, ids: &BTreeSet<String>) {
    let rutas = rutas.clone();
    let ids = ids.clone();
    match tokio::task::spawn_blocking(move || estado::guardar_avisos(&rutas, &ids)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!("no se pudo guardar el estado de los avisos: {error:#}"),
        Err(error) => warn!("no se pudo guardar el estado de los avisos: {error}"),
    }
}

/// Dice en la barra y en el log que Termux:API se ha apagado en este
/// servidor. Solo se llama con el `true` que devuelve la propia orden (la
/// primera vez).
fn api_apagada(termux: &TermuxServidor) {
    warn!("Termux:API no responde en el servidor: sin avisos de Android hasta que reinicie");
    termux.barra.decir(AvisoServidor::ApiApagada);
}

/// Retira los avisos que dejó un servidor anterior, antes de publicar nada.
async fn purgar_los_de_antes(termux: &Arc<TermuxServidor>) {
    let mut publicados = termux.publicados.lock().await;
    if publicados.is_empty() {
        return;
    }
    let ids: Vec<String> = publicados.iter().cloned().collect();
    debug!(
        cuantos = ids.len(),
        "retirando avisos de un servidor anterior"
    );
    retirar(termux, &mut publicados, ids, false).await;
}

/// Retira los avisos dados. La primera orden va sola, como sondeo de
/// Termux:API; luego, como mucho [`RETIRADAS_A_LA_VEZ`] a la vez. Un aviso que
/// no se pudo retirar sigue en el estado (lo retirará otro servidor); si una
/// orden apaga Termux:API, se dice en la barra y no se lanza ninguna más.
///
/// Con el servidor ya terminando no se lanza nada nuevo, salvo que lo pida el
/// propio cierre (`desde_el_cierre`): una purga o un ciclo largos no pueden
/// seguir lanzando órdenes mientras el cierre espera su turno.
async fn retirar(
    termux: &Arc<TermuxServidor>,
    publicados: &mut BTreeSet<String>,
    ids: Vec<String>,
    desde_el_cierre: bool,
) {
    use futures_util::StreamExt as _;
    let mut pendientes = ids.into_iter();
    if let Some(primero) = pendientes.next() {
        if let Some(id) = retirar_uno(termux, primero, desde_el_cierre).await {
            publicados.remove(&id);
        }
    }
    let mut resto = futures_util::stream::iter(pendientes)
        .map(|id| retirar_uno(termux, id, desde_el_cierre))
        .buffer_unordered(RETIRADAS_A_LA_VEZ);
    while let Some(retirado) = resto.next().await {
        if let Some(id) = retirado {
            publicados.remove(&id);
        }
    }
    drop(resto);
    guardar(&termux.rutas, publicados).await;
}

/// Una orden de retirada. Devuelve el id si el aviso quedó retirado; `None`
/// si no se lanzó (Termux:API apagada o servidor terminando) o falló.
async fn retirar_uno(
    termux: &Arc<TermuxServidor>,
    id: String,
    desde_el_cierre: bool,
) -> Option<String> {
    if termux.ordenes.api().apagada() || (!desde_el_cierre && termux.terminando()) {
        return None;
    }
    let (resultado, apagada_ahora) = termux
        .ordenes
        .ejecutar_async_avisando(avisos::argumentos_retirar(&id), None)
        .await;
    if apagada_ahora {
        api_apagada(termux);
    }
    resultado.is_ok().then_some(id)
}

/// Un ciclo: lo que toca tener publicado frente a lo publicado.
async fn ciclo(estado: &Arc<Mutex<EstadoServidor>>, termux: &Arc<TermuxServidor>) {
    let vista = {
        let estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            return;
        }
        instantanea(&estado_bloqueado, termux)
    };
    // Sin Termux:API no se puede publicar ni retirar: lo publicado queda en el
    // estado para el siguiente servidor.
    if termux.ordenes.api().apagada() {
        return;
    }
    let mut publicados = termux.publicados.lock().await;
    if termux.terminando() {
        return;
    }
    // Con `avisos = false` no se publica nada (y se retira lo que hubiera).
    let deseados: BTreeMap<String, Deseado> = if vista.avisos_activos {
        avisos::deseados(
            &vista.fichas,
            &vista.pestanas,
            &publicados,
            avisos::FOCO_FIABLE,
        )
    } else {
        BTreeMap::new()
    };
    let plan = avisos::planificar(&deseados, &publicados);
    if plan.retirar.is_empty() && plan.publicar.is_empty() {
        return;
    }
    // Primero se retira y luego se publica: una sesión que cambia de aviso
    // (de pestaña a ficha) no deja los dos a la vez.
    retirar(termux, &mut publicados, plan.retirar, false).await;
    for deseado in plan.publicar {
        if termux.ordenes.api().apagada() || termux.terminando() {
            break;
        }
        let host = match &deseado.host {
            Some(host) => host.clone(),
            None => nombre_de_host(&vista, deseado.host_id).await,
        };
        publicar(termux, &mut publicados, &deseado, &host).await;
    }
}

/// Publica un aviso. El id se anota en el estado antes de la orden: si el
/// servidor muriera en medio, el siguiente sabría que hay que retirarlo.
async fn publicar(
    termux: &Arc<TermuxServidor>,
    publicados: &mut BTreeSet<String>,
    deseado: &Deseado,
    host: &str,
) {
    if !estado::id_de_aviso_valido(&deseado.id) {
        return;
    }
    publicados.insert(deseado.id.clone());
    guardar(&termux.rutas, publicados).await;
    let (resultado, apagada_ahora) = termux
        .ordenes
        .ejecutar_async_avisando(
            avisos::argumentos_publicar(&deseado.id, &deseado.titulo, host),
            None,
        )
        .await;
    match resultado {
        Ok(_) => {
            // Nunca el id ni el texto: solo cuántos hay.
            debug!(publicados = publicados.len(), "aviso de Android publicado");
        }
        // La orden pudo publicar el aviso antes de que se matara su grupo de
        // procesos por vencer el plazo: el id se queda anotado para que lo
        // retire la siguiente purga o el cierre (retirar uno que no existe no
        // hace daño; un aviso sin id anotado no lo retiraría nadie).
        Err(FalloOrden::SinRespuesta) => {}
        // Fallos que dicen que no se publicó nada: no se anota.
        Err(_) => {
            publicados.remove(&deseado.id);
            guardar(&termux.rutas, publicados).await;
        }
    }
    // La barra lo dice cuando el estado ya es el bueno.
    if apagada_ahora {
        api_apagada(termux);
    }
}

// ------------------------------------------------------------------ cierre

/// Al terminar el servidor: retira todos los avisos que tiene publicados y
/// suelta el wake lock si lo tiene tomado, en paralelo y cada cosa con el
/// plazo de la orden más 3 s (§7.5): un Termux:API que no contesta no alarga
/// el apagado más que eso. No toca el fichero `servidor` (lo quita quien
/// cierra el servidor al final).
pub async fn terminar(estado: &Arc<Mutex<EstadoServidor>>) {
    let (termux, plazo) = {
        let estado_bloqueado = estado.lock().await;
        (
            estado_bloqueado.termux.clone(),
            estado_bloqueado.config.termux.api_plazo(),
        )
    };
    let Some(termux) = termux else {
        return;
    };
    // Antes de esperar ningún cerrojo: lo que esté lanzando órdenes (la purga
    // o un ciclo) deja de hacerlo y suelta su turno al cierre.
    termux.terminando.store(true, Ordering::Release);
    let limite = plazo + Duration::from_secs(3);
    let (avisos, wake_lock) = tokio::join!(
        tokio::time::timeout(limite, retirar_todos(&termux)),
        tokio::time::timeout(limite, termux.wake_lock.terminar()),
    );
    if avisos.is_err() {
        warn!("no se retiraron a tiempo los avisos de Android; el siguiente servidor los retirará");
    }
    if wake_lock.is_err() {
        warn!("no se soltó a tiempo el wake lock; el siguiente servidor lo soltará");
    }
}

/// Retira todos los avisos publicados, con [`RETIRADAS_A_LA_VEZ`] órdenes a la
/// vez como mucho, y no deja que la tarea de avisos publique más (`terminar`
/// ya ha marcado que el servidor termina).
async fn retirar_todos(termux: &Arc<TermuxServidor>) {
    let mut publicados = termux.publicados.lock().await;
    if publicados.is_empty() || termux.ordenes.api().apagada() {
        return;
    }
    let ids: Vec<String> = publicados.iter().cloned().collect();
    retirar(termux, &mut publicados, ids, true).await;
}

// ---------------------------------------------------------------- pruebas

#[cfg(test)]
mod pruebas {
    use super::*;

    fn rutas_en(base: &std::path::Path) -> Rutas {
        Rutas::nuevas(
            base.join("datos"),
            base.join("config"),
            base.join("estado"),
            base.join("hogar"),
            base.join("run"),
        )
    }

    #[test]
    fn la_barra_escribe_el_arranque_sin_avisos_y_los_anade_una_vez() {
        let base = tempfile::tempdir().unwrap();
        let rutas = rutas_en(base.path());
        let barra = BarraServidor::nueva(rutas.clone());
        let inicial = estado::leer_servidor(&rutas).expect("el fichero existe desde el arranque");
        assert!(inicial.arranque > 1_700_000_000);
        assert!(inicial.avisos.is_empty());

        barra.decir(AvisoServidor::SinWakeLock);
        barra.decir(AvisoServidor::SinWakeLock);
        barra.decir(AvisoServidor::ApiApagada);
        let leido = estado::leer_servidor(&rutas).unwrap();
        assert_eq!(leido.arranque, inicial.arranque, "el mismo servidor");
        assert_eq!(
            leido.avisos,
            vec![AvisoServidor::SinWakeLock, AvisoServidor::ApiApagada]
        );
        assert_eq!(barra.avisos(), leido.avisos);

        barra.quitar();
        assert_eq!(estado::leer_servidor(&rutas), None);
        barra.quitar();
    }

    #[test]
    fn la_busqueda_de_una_orden_en_el_path_pide_un_ejecutable_y_no_sigue_rutas() {
        use std::os::unix::fs::PermissionsExt as _;
        let base = tempfile::tempdir().unwrap();
        let bin = base.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let con_modo = |nombre: &str, modo: u32| {
            let ruta = bin.join(nombre);
            std::fs::write(&ruta, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(modo)).unwrap();
        };
        con_modo("termux-notification", 0o755);
        con_modo("termux-sin-permiso", 0o644);
        std::fs::create_dir_all(bin.join("termux-directorio")).unwrap();
        // Una entrada vacía del PATH no cuenta como «aquí».
        let path = std::env::join_paths([std::path::PathBuf::new(), bin.clone()]).unwrap();
        assert!(orden_en_el_path("termux-notification", Some(&path)));
        assert!(!orden_en_el_path("termux-sin-permiso", Some(&path)));
        assert!(!orden_en_el_path("termux-directorio", Some(&path)));
        assert!(!orden_en_el_path("termux-no-esta", Some(&path)));
        assert!(!orden_en_el_path("termux-notification", None));
        assert!(!orden_en_el_path("", Some(&path)));
        let con_barra = format!("{}/termux-notification", bin.display());
        assert!(!orden_en_el_path(&con_barra, Some(&path)), "nada con /");
        assert!(!orden_en_el_path("../bin/termux-notification", Some(&path)));
        // Con solo una entrada vacía no se busca en el directorio actual.
        let vacio = std::env::join_paths([std::path::PathBuf::new()]).unwrap();
        assert!(!orden_en_el_path("Cargo.toml", Some(&vacio)));
    }
}
