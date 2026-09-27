//! Pool de conexiones `host_id → Handle` del servidor y su único punto de
//! apertura, `conexion_para_canal` (corrección 3b de la Fase 6).
//!
//! Todo lo que necesita una conexión (pestañas, canales SFTP, túneles, el
//! `Ejecutar` del sondeo y las ejecuciones de snippets) la pide aquí. Un
//! cerrojo por host cubre «mirar el pool / conectar / guardar», así que dos
//! aperturas simultáneas del mismo host comparten una sola conexión y el pool
//! **nunca desplaza** una conexión viva ni en gracia.
//!
//! Quién comparte: todo lo que no es pestaña usa siempre la conexión del pool;
//! las pestañas, solo si el host tiene `multiplexar` (`usa_pool`). Una pestaña
//! sin `multiplexar` abre su propia conexión, que no entra en el pool y se
//! cierra con ella. Una conexión del pool sin canales se cierra tras la gracia
//! de 30 s.

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh::Disconnect;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, warn};

use crate::conexion::cliente::{Cliente, Contexto};
use crate::conexion::salto::Transporte;
use crate::conexion::{EventoConexion, FuenteContrasena};
use crate::config::Rutas;
use crate::modelo::{EstadoSesion, Host};
use crate::protocolo::EstadoSesionRemota;

use super::EstadoServidor;

/// Gracia de una conexión del pool sin canales antes de cerrarla.
pub const GRACIA_SIN_CANALES: Duration = Duration::from_secs(30);

/// Lo más que espera una apertura no interactiva a que otra del mismo host
/// (que puede estar esperando una decisión del usuario) suelte el cerrojo.
pub const PLAZO_CERROJO_NO_INTERACTIVO: Duration = Duration::from_secs(30);

/// Plazo total de una conexión no interactiva: DNS, TCP, saludo SSH y
/// autenticación. Una interactiva no lo tiene (espera al usuario).
pub const PLAZO_CONEXION_NO_INTERACTIVA: Duration = Duration::from_secs(30);

/// Plazo de red de una conexión interactiva (DNS, TCP, saludo SSH y
/// autenticación) sin contar el tiempo que pasa esperando al usuario en un
/// diálogo: un host que acepta TCP y no habla SSH no retiene el cerrojo.
pub const PLAZO_RED_INTERACTIVA: Duration = Duration::from_secs(20);

/// Plazo para desconectar: el remoto puede no contestar nunca.
const PLAZO_DESCONEXION: Duration = Duration::from_secs(5);

/// Motivo cuando el servidor ya no admite trabajo nuevo.
pub const APAGANDO: &str = "el servidor se está apagando";

/// Motivo de `SoloViva` cuando el host no tiene ninguna conexión abierta.
pub const SIN_CONEXION_VIVA: &str = "no hay ninguna conexión viva con el host";

pub struct Entrada {
    pub handle: Arc<Handle<Cliente>>,
    /// Conexiones intermedias de la cadena de saltos, también compartidas.
    pub saltos: Vec<Arc<Handle<Cliente>>>,
    /// Identidad con la que se autenticó (la enseña la pestaña que reutiliza).
    pub identidad: String,
    pub canales: u32,
    pub sin_canales_desde: Option<Instant>,
}

#[derive(Default)]
pub struct Pool {
    entradas: HashMap<i64, Entrada>,
}

/// Conexión vencida que devuelve `recoger_vencidas` para desconectarla.
pub type Vencida = (i64, Arc<Handle<Cliente>>, Vec<Arc<Handle<Cliente>>>);

/// Conexión lista para cerrar, con sus saltos.
pub type Cerrable = (Arc<Handle<Cliente>>, Vec<Arc<Handle<Cliente>>>);

impl Pool {
    /// Devuelve la conexión viva del host para abrir otro canal, contándolo.
    /// Una conexión cerrada no se reutiliza: se queda para `retirar_muerta`.
    pub fn reutilizar(&mut self, host_id: i64) -> Option<(Arc<Handle<Cliente>>, String)> {
        let entrada = self.entradas.get_mut(&host_id)?;
        if entrada.handle.is_closed() {
            return None;
        }
        entrada.canales += 1;
        entrada.sin_canales_desde = None;
        Some((entrada.handle.clone(), entrada.identidad.clone()))
    }

    /// Quita la entrada del host **solo si su conexión está cerrada**, para
    /// que el llamador la desconecte y abra otra. Una viva no se toca.
    pub fn retirar_muerta(&mut self, host_id: i64) -> Option<Cerrable> {
        if !self
            .entradas
            .get(&host_id)
            .is_some_and(|entrada| entrada.handle.is_closed())
        {
            return None;
        }
        self.entradas
            .remove(&host_id)
            .map(|entrada| (entrada.handle, entrada.saltos))
    }

    /// Guarda un transporte recién abierto, con el canal que acaba de abrirse
    /// contado. **Nunca desplaza**: si el host ya tiene entrada (viva, en
    /// gracia o muerta sin retirar), devuelve el transporte al llamador, que
    /// lo usará como conexión propia.
    pub fn guardar(&mut self, host_id: i64, transporte: Transporte) -> Result<(), Transporte> {
        if self.entradas.contains_key(&host_id) {
            return Err(transporte);
        }
        self.entradas.insert(
            host_id,
            Entrada {
                handle: transporte.handle,
                saltos: transporte.saltos.into_iter().map(Arc::new).collect(),
                identidad: transporte.identidad.descripcion,
                canales: 1,
                sin_canales_desde: None,
            },
        );
        Ok(())
    }

    /// Descuenta un canal **solo si la entrada sigue siendo esa conexión**.
    /// Con dos conexiones al mismo host que se turnan en el pool, descontar a
    /// ciegas podría dejar a cero el contador de otra viva (y cerrarla).
    pub fn liberar_si_es(&mut self, host_id: i64, handle: &Arc<Handle<Cliente>>) {
        let Some(entrada) = self.entradas.get_mut(&host_id) else {
            return;
        };
        if !Arc::ptr_eq(&entrada.handle, handle) {
            return;
        }
        entrada.canales = entrada.canales.saturating_sub(1);
        if entrada.canales == 0 {
            entrada.sin_canales_desde = Some(Instant::now());
        }
    }

    /// Quita las conexiones sin canales que agotaron la gracia (o que ya
    /// están cerradas) para que el llamador las desconecte fuera del bloqueo.
    pub fn recoger_vencidas(&mut self) -> Vec<Vencida> {
        let mut vencidas = Vec::new();
        let ids: Vec<i64> = self
            .entradas
            .iter()
            .filter(|(_, entrada)| {
                entrada.canales == 0
                    && (entrada.handle.is_closed()
                        || entrada
                            .sin_canales_desde
                            .is_some_and(|desde| desde.elapsed() >= GRACIA_SIN_CANALES))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(entrada) = self.entradas.remove(&id) {
                vencidas.push((id, entrada.handle, entrada.saltos));
            }
        }
        vencidas
    }

    /// Quita y devuelve todas las conexiones (para el apagado).
    pub fn vaciar(&mut self) -> Vec<Cerrable> {
        self.entradas
            .drain()
            .map(|(_, entrada)| (entrada.handle, entrada.saltos))
            .collect()
    }

    /// Canales contados en la conexión del host (pruebas y diagnóstico).
    pub fn canales(&self, host_id: i64) -> Option<u32> {
        self.entradas.get(&host_id).map(|entrada| entrada.canales)
    }

    /// ¿Hay entrada para el host?
    pub fn tiene(&self, host_id: i64) -> bool {
        self.entradas.contains_key(&host_id)
    }
}

// ---------------------------------------------------------------- apertura

/// Cómo se abre una conexión que haga falta abrir.
#[derive(Debug, Clone, Copy)]
pub enum PoliticaConexion {
    /// Diálogos (huella, frase, contraseña) al solicitante. `id_solicitud` es
    /// el id con el que viajan los diálogos por el protocolo.
    Interactiva { solicitante: u32, id_solicitud: u32 },
    /// Sin diálogos: huella desconocida o frase son un error con instrucción;
    /// la contraseña solo sale del llavero del solicitante (si lo hay).
    NoInteractiva {
        solicitante: Option<u32>,
        id_solicitud: u32,
    },
    /// Solo una conexión que ya exista: la del pool o, si no, la de una
    /// pestaña abierta (prestada, sin contarla). Nunca conecta.
    SoloViva,
}

/// Para qué se quiere la conexión: decide si se comparte (`usa_pool`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsoCanal {
    Pestana,
    Subsistema,
}

/// La regla de quién comparte la conexión del host, en un solo sitio: todo lo
/// que no es pestaña (SFTP, túneles, sondeo, ejecuciones) usa siempre el pool;
/// las pestañas, solo con `multiplexar`.
pub fn usa_pool(host: &Host, uso: UsoCanal) -> bool {
    uso == UsoCanal::Subsistema || host.multiplexar
}

/// De dónde salió una conexión tomada: decide cómo se suelta.
enum OrigenConexion {
    /// Del pool, con un canal contado: se descuenta por identidad.
    Pool,
    /// Propia de quien la pidió: se cierra al soltarla.
    Propia(Transporte),
    /// De una pestaña abierta, sin contarla (solo `SoloViva`): no se toca.
    Prestada,
}

/// Una conexión lista para abrir canales. Se suelta con `soltar`, que
/// descuenta el canal del pool por identidad o cierra la conexión propia; si
/// se pierde sin soltar, `Drop` hace lo mismo en segundo plano.
pub struct ConexionTomada {
    pub host_id: i64,
    pub handle: Arc<Handle<Cliente>>,
    /// Se abrió para esta toma: no había ninguna que reutilizar.
    pub nueva: bool,
    /// Identidad con la que está autenticada la conexión.
    pub identidad: String,
    origen: Option<OrigenConexion>,
    estado: Weak<Mutex<EstadoServidor>>,
}

impl std::fmt::Debug for ConexionTomada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConexionTomada")
            .field("host_id", &self.host_id)
            .field("nueva", &self.nueva)
            .field("del_pool", &self.del_pool())
            .finish()
    }
}

impl ConexionTomada {
    fn nueva_toma(
        estado: &Arc<Mutex<EstadoServidor>>,
        host_id: i64,
        handle: Arc<Handle<Cliente>>,
        identidad: String,
        nueva: bool,
        origen: OrigenConexion,
    ) -> Self {
        Self {
            host_id,
            handle,
            nueva,
            identidad,
            origen: Some(origen),
            estado: Arc::downgrade(estado),
        }
    }

    /// ¿Es un canal contado en la conexión del pool?
    pub fn del_pool(&self) -> bool {
        matches!(self.origen, Some(OrigenConexion::Pool))
    }

    /// ¿Es una conexión propia, que se cerrará al soltarla?
    pub fn propia(&self) -> bool {
        matches!(self.origen, Some(OrigenConexion::Propia(_)))
    }

    /// Suelta la conexión: descuenta el canal del pool (por identidad, nunca
    /// a ciegas) o cierra la propia. La prestada no se toca.
    pub async fn soltar(mut self) {
        let origen = self.origen.take();
        soltar_origen(&self.estado, self.host_id, &self.handle, origen).await;
    }
}

impl Drop for ConexionTomada {
    fn drop(&mut self) {
        let Some(origen) = self.origen.take() else {
            return;
        };
        if matches!(origen, OrigenConexion::Prestada) {
            return;
        }
        // Red de seguridad: quien la tomó no la soltó (una tarea abortada, una
        // rama de error olvidada). Se suelta en segundo plano.
        debug!(
            host_id = self.host_id,
            "conexión tomada sin soltar: se suelta sola"
        );
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let estado = self.estado.clone();
        let host_id = self.host_id;
        let handle = self.handle.clone();
        runtime.spawn(async move {
            soltar_origen(&estado, host_id, &handle, Some(origen)).await;
        });
    }
}

async fn soltar_origen(
    estado: &Weak<Mutex<EstadoServidor>>,
    host_id: i64,
    handle: &Arc<Handle<Cliente>>,
    origen: Option<OrigenConexion>,
) {
    match origen {
        Some(OrigenConexion::Pool) => {
            if let Some(estado) = estado.upgrade() {
                estado.lock().await.pool.liberar_si_es(host_id, handle);
            }
        }
        Some(OrigenConexion::Propia(transporte)) => {
            desconectar(
                transporte.handle,
                transporte.saltos.into_iter().map(Arc::new).collect(),
            )
            .await;
        }
        Some(OrigenConexion::Prestada) | None => {}
    }
}

/// La **única** forma de obtener una conexión para abrir un canal: pestañas,
/// SFTP, túneles, `Ejecutar` y ejecuciones de snippets.
///
/// 1. Vía rápida: si se comparte, la conexión viva del pool (reutilizar no
///    conecta, así que no hace falta el cerrojo).
/// 2. `SoloViva`: la de una pestaña abierta, prestada; si no hay, error.
/// 3. Con el cerrojo del host tomado: otra mirada al pool (quien tenía el
///    cerrojo pudo dejarla), retirada de una muerta, conexión nueva según la
///    política y, si se comparte, a guardar en el pool.
pub async fn conexion_para_canal(
    estado: &Arc<Mutex<EstadoServidor>>,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &Rutas,
    politica: PoliticaConexion,
    uso: UsoCanal,
) -> Result<ConexionTomada, String> {
    if estado.lock().await.apagando {
        return Err(APAGANDO.to_string());
    }
    let compartida = usa_pool(host, uso);
    if compartida || matches!(politica, PoliticaConexion::SoloViva) {
        if let Some(tomada) = tomar_del_pool(estado, host.id).await {
            return Ok(tomada);
        }
    }
    if let PoliticaConexion::SoloViva = politica {
        return prestada_de_pestana(estado, host.id)
            .await
            .ok_or_else(|| SIN_CONEXION_VIVA.to_string());
    }
    if !compartida {
        // Pestaña sin `multiplexar`: conexión propia que no entra en el pool,
        // así que no compite con nadie por el cerrojo del host.
        let transporte = conectar(estado, host, todos_los_hosts, rutas, politica).await?;
        let handle = transporte.handle.clone();
        let identidad = transporte.identidad.descripcion.clone();
        return Ok(ConexionTomada::nueva_toma(
            estado,
            host.id,
            handle,
            identidad,
            true,
            OrigenConexion::Propia(transporte),
        ));
    }

    let cerrojo = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .cerrojos_conexion
            .entry(host.id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guardia = match politica {
        PoliticaConexion::NoInteractiva { .. } => {
            tokio::time::timeout(PLAZO_CERROJO_NO_INTERACTIVO, cerrojo.lock_owned())
                .await
                .map_err(|_| {
                    format!(
                        "otra apertura de «{}» espera una decisión del usuario; inténtalo de nuevo",
                        host.nombre
                    )
                })?
        }
        _ => cerrojo.lock_owned().await,
    };

    // Quien tenía el cerrojo pudo dejar la conexión en el pool.
    if let Some(tomada) = tomar_del_pool(estado, host.id).await {
        return Ok(tomada);
    }
    // Una entrada muerta no se reutiliza ni se desplaza: se retira y se cierra
    // (fuera del bloqueo; sus usuarios la sueltan por identidad sin efecto).
    let muerta = estado.lock().await.pool.retirar_muerta(host.id);
    if let Some((handle, saltos)) = muerta {
        tokio::spawn(desconectar(handle, saltos));
    }

    let transporte = conectar(estado, host, todos_los_hosts, rutas, politica).await?;
    let handle = transporte.handle.clone();
    let identidad = transporte.identidad.descripcion.clone();
    let guardado = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            // El pool ya se vació: guardarla la dejaría sin cerrar.
            Guardado::Apagando(transporte)
        } else {
            match estado_bloqueado.pool.guardar(host.id, transporte) {
                Ok(()) => Guardado::EnElPool,
                Err(transporte) => Guardado::Ocupado(transporte),
            }
        }
    };
    match guardado {
        Guardado::EnElPool => Ok(ConexionTomada::nueva_toma(
            estado,
            host.id,
            handle,
            identidad,
            true,
            OrigenConexion::Pool,
        )),
        Guardado::Ocupado(transporte) => {
            // Con el cerrojo tomado nadie más guarda este host: no debería
            // pasar. Aun así no se desplaza nada: la conexión queda propia.
            warn!(host = %host.nombre, "el pool ya tenía entrada con el cerrojo tomado");
            Ok(ConexionTomada::nueva_toma(
                estado,
                host.id,
                handle,
                identidad,
                true,
                OrigenConexion::Propia(transporte),
            ))
        }
        Guardado::Apagando(transporte) => {
            desconectar(
                transporte.handle,
                transporte.saltos.into_iter().map(Arc::new).collect(),
            )
            .await;
            Err(APAGANDO.to_string())
        }
    }
}

/// Qué pasó al intentar dejar una conexión nueva en el pool.
enum Guardado {
    EnElPool,
    /// Ya había entrada (no debería con el cerrojo): queda como propia.
    Ocupado(Transporte),
    /// El servidor se apaga: se cierra.
    Apagando(Transporte),
}

async fn tomar_del_pool(
    estado: &Arc<Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<ConexionTomada> {
    let reutilizada = estado.lock().await.pool.reutilizar(host_id);
    reutilizada.map(|(handle, identidad)| {
        ConexionTomada::nueva_toma(
            estado,
            host_id,
            handle,
            identidad,
            false,
            OrigenConexion::Pool,
        )
    })
}

/// La conexión de una pestaña abierta del host, prestada: vive lo que viva la
/// pestaña y no se cuenta ni se cierra al soltarla.
async fn prestada_de_pestana(
    estado: &Arc<Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<ConexionTomada> {
    let (handle, identidad) = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .sesiones
            .values()
            .filter(|sesion| {
                sesion.host_id == host_id && sesion.estado == EstadoSesionRemota::Abierta
            })
            .find_map(|sesion| {
                sesion
                    .handle
                    .as_ref()
                    .filter(|handle| !handle.is_closed())
                    .map(|handle| (handle.clone(), sesion.identidad.clone()))
            })?
    };
    Some(ConexionTomada::nueva_toma(
        estado,
        host_id,
        handle,
        identidad,
        false,
        OrigenConexion::Prestada,
    ))
}

/// Conecta la cadena del host según la política. Los diálogos de una
/// interactiva van al solicitante por el puente; una no interactiva no
/// dialoga y tiene plazo total.
async fn conectar(
    estado: &Arc<Mutex<EstadoServidor>>,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &Rutas,
    politica: PoliticaConexion,
) -> Result<Transporte, String> {
    let (id_solicitud, solicitante, interactivo) = match politica {
        PoliticaConexion::Interactiva {
            solicitante,
            id_solicitud,
        } => (id_solicitud, Some(solicitante), true),
        PoliticaConexion::NoInteractiva {
            solicitante,
            id_solicitud,
        } => (id_solicitud, solicitante, false),
        PoliticaConexion::SoloViva => return Err(SIN_CONEXION_VIVA.to_string()),
    };
    let reenvios = estado.lock().await.reenvios.clone();
    let (tx_eventos, rx_eventos) = mpsc::unbounded_channel::<EventoConexion>();
    // El puente traduce los diálogos al solicitante y se deja vivo: el handler
    // de russh conserva un clon del canal de eventos mientras la conexión
    // viva, así que muere solo cuando la conexión cae.
    tokio::spawn(super::sesiones::puente_eventos(
        estado.clone(),
        id_solicitud,
        solicitante,
        rx_eventos,
    ));
    let cadena = crate::conexion::salto::construir_cadena(host, todos_los_hosts)
        .map_err(|error| error.to_string())?;
    let _ = tx_eventos.send(EventoConexion::Estado {
        host_id: host.id,
        estado: EstadoSesion::Conectando,
    });
    let contexto = Contexto {
        known_hosts: rutas.fichero_known_hosts(),
        dir_ssh: rutas.dir_ssh(),
        hogar: rutas.hogar.clone(),
        usuario_local: crate::conexion::usuario_local(),
        tx: tx_eventos.clone(),
        interactivo,
        fuente_contrasena: if interactivo {
            FuenteContrasena::Solicitante
        } else {
            FuenteContrasena::SolicitanteLlavero
        },
        // Cualquier conexión del pool puede sostener túneles remotos del host.
        reenvios: Some(reenvios),
    };
    let conexion = crate::conexion::salto::conectar_cadena(&cadena, &contexto);
    if interactivo {
        // El plazo de red solo corre mientras no hay un diálogo esperando al
        // usuario (esos tienen sus 5 minutos aparte).
        let vigilante = vigilar_plazo_de_red(estado, id_solicitud);
        return tokio::select! {
            resultado = conexion => resultado.map_err(|error| error.to_string()),
            _ = vigilante => Err(format!(
                "«{}» no respondió en {} s (sin contar los diálogos)",
                host.nombre,
                PLAZO_RED_INTERACTIVA.as_secs()
            )),
        };
    }
    match tokio::time::timeout(PLAZO_CONEXION_NO_INTERACTIVA, conexion).await {
        Ok(resultado) => resultado.map_err(|error| error.to_string()),
        Err(_) => Err(format!(
            "se agotó el plazo conectando con «{}» ({} s)",
            host.nombre,
            PLAZO_CONEXION_NO_INTERACTIVA.as_secs()
        )),
    }
}

/// Termina cuando la conexión lleva `PLAZO_RED_INTERACTIVA` de tiempo de red
/// sin un diálogo pendiente de esa solicitud.
async fn vigilar_plazo_de_red(estado: &Arc<Mutex<EstadoServidor>>, id_solicitud: u32) {
    let paso = Duration::from_millis(250);
    let mut sin_dialogo = Duration::ZERO;
    while sin_dialogo < PLAZO_RED_INTERACTIVA {
        tokio::time::sleep(paso).await;
        if !estado.lock().await.pendientes.contains_key(&id_solicitud) {
            sin_dialogo += paso;
        }
    }
}

/// Abre un canal con plazo sin dejarlo huérfano: la apertura va en su propia
/// tarea y, si el plazo vence, una tarea de rescate espera a que el host la
/// confirme para cerrarla (en russh un `Channel` no se cierra al soltarse, y
/// en una conexión compartida los canales huérfanos agotarían `MaxSessions`).
pub async fn abrir_con_plazo<F, E>(
    plazo: Duration,
    apertura: F,
) -> Result<russh::Channel<russh::client::Msg>, String>
where
    F: std::future::Future<Output = Result<russh::Channel<russh::client::Msg>, E>> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let mut tarea = tokio::spawn(apertura);
    match tokio::time::timeout(plazo, &mut tarea).await {
        Ok(Ok(Ok(canal))) => Ok(canal),
        Ok(Ok(Err(error))) => Err(error.to_string()),
        Ok(Err(error)) => Err(format!("la apertura del canal falló: {error}")),
        Err(_) => {
            tokio::spawn(async move {
                // Termina sola: si la conexión cae, la apertura devuelve error.
                if let Ok(Ok(canal)) = tarea.await {
                    let _ = canal.close().await;
                }
            });
            Err("el host no abrió el canal a tiempo".to_string())
        }
    }
}

/// Desconecta una conexión (destino y saltos) descartando errores y con
/// plazo: puede que el remoto ya la haya cerrado o que no conteste.
pub async fn desconectar(handle: Arc<Handle<Cliente>>, saltos: Vec<Arc<Handle<Cliente>>>) {
    let cierre = async {
        if let Err(error) = handle.disconnect(Disconnect::ByApplication, "", "").await {
            debug!("no se pudo cerrar una conexión: {error}");
        }
        for salto in saltos {
            if let Err(error) = salto.disconnect(Disconnect::ByApplication, "", "").await {
                debug!("no se pudo cerrar una conexión de salto: {error}");
            }
        }
    };
    if tokio::time::timeout(PLAZO_DESCONEXION, cierre)
        .await
        .is_err()
    {
        warn!("el remoto no contestó al cierre de la conexión");
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_gracia_sin_canales_es_de_30_segundos() {
        assert_eq!(GRACIA_SIN_CANALES, Duration::from_secs(30));
    }

    fn host(multiplexar: bool) -> Host {
        let mut host = crate::modelo::host_de_prueba();
        host.multiplexar = multiplexar;
        host
    }

    /// La regla de la corrección 3b: todo lo que no es pestaña comparte; las
    /// pestañas, solo con `multiplexar`.
    #[test]
    fn solo_las_pestanas_dependen_de_multiplexar() {
        assert!(usa_pool(&host(false), UsoCanal::Subsistema));
        assert!(usa_pool(&host(true), UsoCanal::Subsistema));
        assert!(usa_pool(&host(true), UsoCanal::Pestana));
        assert!(!usa_pool(&host(false), UsoCanal::Pestana));
    }
}
