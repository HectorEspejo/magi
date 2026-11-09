//! Gestor de conexiones del móvil (Fase 14, D277): una sola conexión viva por
//! host para todo (sesiones, túneles, sondeo, listados de servicios, acciones,
//! suscripciones de log, ejecuciones y el SFTP de BALTHASAR-2).
//!
//! La conexión lleva un contador de usos y la política con la que se abrió. El
//! último uso la deja en gracia GRACIA_SEG segundos: si alguien la pide en ese
//! margen se reutiliza sin autenticar otra vez y, si no, un temporizador la
//! cierra. Todas las aperturas de un host pasan por el cerrojo por host para
//! que nunca haya dos conexiones vivas al mismo host.
//!
//! También expone el ejecutor remoto compartido (un canal exec con entrada
//! estándar, plazo total y tope por flujo) que usan el sondeo, los servicios,
//! el sudo y las ejecuciones.
//!
//! Cada uso es un UsoConexion que se suelta por identidad y de forma automática
//! al soltarse (RAII), también en todas las ramas de error: así una sesión o un
//! túnel no pueden dejar el contador descuadrado. El cierre por gracia
//! comprueba que la conexión sigue teniendo cero usos antes de cerrarla.
//!
//! El vigilante de la Fase 11 (cada VIGILANTE_MS, con handle.is_closed()) se
//! conserva y cubre todo lo nuevo: al caer la conexión, las sesiones pasan a
//! caídas, los túneles a caídos y todo lo demás se entera por el registro de
//! OyenteConexion (acciones en curso, logs y ejecuciones).
//!
//! Hay dos políticas de apertura: Politica::Interactiva (la pide el usuario y
//! puede abrir los diálogos de huella y contraseña) y Politica::Automatica
//! (sondeo, refrescos, consumo, comprobaciones de la deliberación y
//! ejecuciones: nunca abre un diálogo; los fallos de huella o de credenciales
//! vuelven con instrucción). La traducción de esos fallos vive aislada en
//! traducir_error, que es el único punto que habrá que cambiar cuando api.rs
//! tenga las variantes tipadas de ErrorNucleo.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use magi_nucleo::almacen::{hosts as hosts_bd, movil as movil_bd};
use magi_nucleo::conexion::abrir_con_plazo;
use magi_nucleo::conexion::cliente::{ClaveMovil, Cliente, Contexto};
use magi_nucleo::conexion::reenvios::Reenvios;
use magi_nucleo::conexion::salto::{
    conectar_cadena, conectar_cadena_movil, construir_cadena, Transporte,
};
use magi_nucleo::conexion::{EventoConexion, FuenteContrasena};
use magi_nucleo::modelo::Host;
use magi_nucleo::registro::{self, ResultadoRegistro};
use russh::client::Handle;
use russh::ChannelMsg;
use tokio::sync::{mpsc, oneshot, watch};

use crate::api::{ErrorNucleo, EstadoSesion, PeticionDialogo, RespuestaDialogo};
use crate::estado::{con_almacen, EstadoNucleo};

/// Sin usos, la conexión sigue viva este tiempo antes de cerrarse (D277): más
/// que el refresco de Flota (30 s), para no reconectar en cada vuelta.
pub const GRACIA_SEG: u64 = 60;

/// Cada cuánto comprueba el vigilante que el host no haya cerrado la conexión
/// (T97; es el valor que ya usaba la Fase 11).
pub const VIGILANTE_MS: u64 = 500;

/// Plazo para abrir un canal exec y para escribir en su entrada estándar.
const PLAZO_CANAL: Duration = Duration::from_secs(10);

/// Motivo con el que se marcan las sesiones y los túneles cuando el host cierra
/// la conexión sin que nadie la soltara.
const MOTIVO_CAIDA: &str = "la conexión con el host se ha perdido; pulsa Relanzar";

/// Política con la que se abre una conexión (D277). Se guarda en la conexión:
/// una conexión abierta en automático se reutiliza tal cual si después la pide
/// el usuario, y viceversa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Politica {
    /// La pide el usuario (conectar, activar un túnel, abrir los servicios de
    /// un host, una acción o los logs): puede abrir los diálogos de la Fase 11.
    Interactiva,
    /// Sondeo, refrescos, consumo, comprobaciones de la deliberación y
    /// ejecuciones: nunca abre un diálogo; los fallos de huella o de
    /// credenciales vuelven con instrucción (T15, T49).
    Automatica,
}

impl Politica {
    /// ¿Puede dialogar mientras abre? Es el valor de Contexto.interactivo del
    /// núcleo.
    pub fn interactiva(self) -> bool {
        matches!(self, Politica::Interactiva)
    }

    /// Texto para mensajes y registro.
    pub fn etiqueta(self) -> &'static str {
        match self {
            Politica::Interactiva => "interactiva",
            Politica::Automatica => "automática",
        }
    }
}

/// Quien está usando la conexión de un host y debe enterarse de su caída
/// (T97): las pistas de acciones, logs y ejecuciones se suscriben al empezar y
/// se dan de baja al terminar.
///
/// El aviso llega desde el vigilante, sin bloquear: el oyente solo apunta el
/// motivo y despierta a su tarea, que decide qué estado deja lo que estaba en
/// curso.
pub trait OyenteConexion: Send + Sync {
    /// El host cerró la conexión o la red cayó. El motivo es el texto que se
    /// enseña al usuario.
    fn conexion_caida(&self, host_uuid: &str, motivo: &str);
}

/// Reloj inyectable de la gracia: en producción, el del sistema; en las
/// pruebas, uno manual que avanza a mano.
trait Reloj: Send + Sync {
    fn ahora(&self) -> Instant;
}

/// Reloj real (el del sistema).
struct RelojReal;

impl Reloj for RelojReal {
    fn ahora(&self) -> Instant {
        Instant::now()
    }
}

/// Contabilidad pura de usos y gracia de una conexión: sin E/S, sin runtime y
/// con el reloj inyectado, para poder probarla sin red.
#[derive(Debug, Default)]
struct Contabilidad {
    usos: usize,
    gracia_hasta: Option<Instant>,
}

impl Contabilidad {
    fn usos(&self) -> usize {
        self.usos
    }

    /// Toma un uso. Si la conexión estaba en gracia, la gracia se cancela: se
    /// reutiliza sin autenticar otra vez (§7.2).
    fn tomar(&mut self) {
        self.usos = self.usos.saturating_add(1);
        self.gracia_hasta = None;
    }

    /// Suelta un uso por identidad. Devuelve true si era el último y la
    /// conexión queda en gracia. Un soltar de más (usos ya a cero) no descuenta
    /// dos veces ni reinicia la gracia.
    fn soltar(&mut self, ahora: Instant) -> bool {
        if self.usos == 0 {
            return false;
        }
        self.usos -= 1;
        if self.usos == 0 {
            self.gracia_hasta = Some(ahora + Duration::from_secs(GRACIA_SEG));
            true
        } else {
            false
        }
    }

    /// ¿Está en gracia y aún no ha vencido?
    fn en_gracia(&self, ahora: Instant) -> bool {
        self.usos == 0 && self.gracia_hasta.is_some_and(|hasta| ahora < hasta)
    }

    /// ¿Se puede tomar ahora? Viva (usos > 0) o en gracia vigente.
    fn disponible(&self, ahora: Instant) -> bool {
        self.usos > 0 || self.en_gracia(ahora)
    }

    /// ¿Venció la gracia sin que nadie la tomara?
    fn gracia_vencida(&self, ahora: Instant) -> bool {
        self.usos == 0 && self.gracia_hasta.is_some_and(|hasta| ahora >= hasta)
    }

    /// ¿Toca cerrarla por gracia? Nunca con usos > 0.
    fn cerrar_por_gracia(&self, ahora: Instant) -> bool {
        self.gracia_vencida(ahora)
    }
}

/// Una conexión viva de un host, compartida por todo lo que la usa.
pub struct ConexionHost {
    pub handle: Arc<Handle<Cliente>>,
    pub saltos: Vec<Handle<Cliente>>,
    pub reenvios: Arc<Reenvios>,
    /// Canal de eventos de la conexión: mantenerlo vivo conserva el bucle que
    /// traduce huellas y contraseñas a diálogos de la app.
    pub tx: mpsc::UnboundedSender<EventoConexion>,
    /// uuid de los hosts de salto de la cadena (cuentan como trabajo vivo).
    pub saltos_uuids: Vec<String>,
    /// Política con la que se abrió (D277).
    politica: Politica,
    contabilidad: Mutex<Contabilidad>,
}

impl ConexionHost {
    /// Política con la que se abrió esta conexión.
    pub fn politica(&self) -> Politica {
        self.politica
    }

    /// Usos vivos en este momento (0 = en gracia o a punto de cerrarse).
    pub fn usos(&self) -> usize {
        self.contabilidad
            .lock()
            .map(|contabilidad| contabilidad.usos())
            .unwrap_or(0)
    }

    fn tomar_uso(&self) {
        if let Ok(mut contabilidad) = self.contabilidad.lock() {
            contabilidad.tomar();
        }
    }

    fn soltar_uso(&self, ahora: Instant) -> bool {
        self.contabilidad
            .lock()
            .map(|mut contabilidad| contabilidad.soltar(ahora))
            .unwrap_or(false)
    }

    fn disponible(&self, ahora: Instant) -> bool {
        self.contabilidad
            .lock()
            .map(|contabilidad| contabilidad.disponible(ahora))
            .unwrap_or(false)
    }

    fn cerrar_por_gracia(&self, ahora: Instant) -> bool {
        self.contabilidad
            .lock()
            .map(|contabilidad| contabilidad.cerrar_por_gracia(ahora))
            .unwrap_or(false)
    }
}

/// Uso tomado de una conexión (RAII): al soltarse descuenta por identidad y, si
/// era el último, deja la conexión en gracia.
///
/// Se suelta solo con Drop y una única vez por uso: no hay método que lo
/// consuma antes, así que nadie puede descontar dos veces. Como guarda el Arc
/// de ConexionHost concreto, soltar un uso de una conexión antigua no toca la
/// conexión nueva que ocupe el mismo host.
pub struct UsoConexion {
    conexion: Arc<ConexionHost>,
    host_uuid: String,
    gestor: Arc<GestorConexiones>,
}

impl UsoConexion {
    fn nuevo(
        gestor: Arc<GestorConexiones>,
        host_uuid: String,
        conexion: Arc<ConexionHost>,
    ) -> Self {
        Self {
            conexion,
            host_uuid,
            gestor,
        }
    }

    /// La conexión que respalda el uso.
    pub fn conexion(&self) -> &Arc<ConexionHost> {
        &self.conexion
    }

    /// uuid del host de la conexión.
    pub fn host_uuid(&self) -> &str {
        &self.host_uuid
    }
}

impl std::ops::Deref for UsoConexion {
    type Target = ConexionHost;

    fn deref(&self) -> &ConexionHost {
        &self.conexion
    }
}

impl Drop for UsoConexion {
    fn drop(&mut self) {
        let ahora = self.gestor.reloj.ahora();
        if self.conexion.soltar_uso(ahora) {
            self.gestor
                .programar_cierre_en_gracia(self.host_uuid.clone(), self.conexion.clone());
        }
    }
}

/// Gestor de conexiones del móvil: dueño del mapa por host, de los cerrojos de
/// apertura y de los temporizadores de gracia.
pub struct GestorConexiones {
    conexiones: Mutex<HashMap<String, Arc<ConexionHost>>>,
    cerrojos: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    reloj: Arc<dyn Reloj>,
    manejador: tokio::runtime::Handle,
}

impl GestorConexiones {
    /// Gestor con el reloj del sistema y el runtime donde corren el vigilante y
    /// el temporizador de gracia.
    pub fn nuevo(manejador: tokio::runtime::Handle) -> Self {
        Self {
            conexiones: Mutex::new(HashMap::new()),
            cerrojos: Mutex::new(HashMap::new()),
            reloj: Arc::new(RelojReal),
            manejador,
        }
    }

    /// Toma un uso de la conexión del host (F14, D277). Es el único camino para
    /// obtener una conexión: sesiones, túneles, sondeo, servicios, logs,
    /// ejecuciones y el SFTP de BALTHASAR-2.
    ///
    /// Si hay una viva o en gracia se reutiliza sin autenticar otra vez. Si no,
    /// se abre con la política pedida bajo el cerrojo del host, con un vigilante
    /// propio, y se publica con el uso ya tomado: nadie puede verla con cero
    /// usos y robársela al temporizador de gracia.
    pub async fn conexion_para_host(
        self: &Arc<Self>,
        estado: &Arc<EstadoNucleo>,
        host_uuid: &str,
        politica: Politica,
    ) -> Result<UsoConexion, ErrorNucleo> {
        if let Some(uso) = self.tomar_viva(host_uuid) {
            return Ok(uso);
        }
        let cerrojo = self.cerrojo_de_host(host_uuid);
        let _guardia = cerrojo.lock().await;
        // Otra tarea pudo abrirla mientras esperábamos el cerrojo.
        if let Some(uso) = self.tomar_viva(host_uuid) {
            return Ok(uso);
        }
        // Una gracia vencida que el temporizador aún no ha retirado no debe
        // impedir abrir otra (una sola conexión por host).
        if let Some(vencida) = self.retirar_vencida(host_uuid) {
            self.cerrar(vencida);
        }
        let conexion = self.abrir(estado, host_uuid, politica).await?;
        conexion.tomar_uso();
        self.vigilar(estado.clone(), host_uuid.to_string(), &conexion);
        if let Ok(mut conexiones) = self.conexiones.lock() {
            conexiones.insert(host_uuid.to_string(), conexion.clone());
        }
        Ok(self.uso(host_uuid, conexion))
    }

    /// La conexión viva (o en gracia) del host, sin tomar un uso. Para
    /// inspecciones internas que no deben alargar la vida de la conexión.
    pub fn viva(&self, host_uuid: &str) -> Option<Arc<ConexionHost>> {
        self.conexiones
            .lock()
            .ok()
            .and_then(|conexiones| conexiones.get(host_uuid).cloned())
    }

    /// Copia de las conexiones del mapa, para la instantánea de trabajo vivo.
    /// Si el mapa está ocupado en ese instante devuelve una lista vacía, como
    /// hacía la Fase 11.
    pub fn vivas(&self) -> Vec<(String, Arc<ConexionHost>)> {
        self.conexiones
            .try_lock()
            .map(|conexiones| {
                conexiones
                    .iter()
                    .map(|(uuid, conexion)| (uuid.clone(), conexion.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Cierra y retira la conexión del host a propósito (por ejemplo, al
    /// reconectar una sesión caída). Los usos vivos de esa conexión siguen
    /// contando sobre ella, pero el mapa ya no la ofrece: al soltarse no se
    /// cierra nada más ni se reprograma nada.
    pub fn forzar_cierre(&self, host_uuid: &str) -> bool {
        let conexion = self
            .conexiones
            .lock()
            .ok()
            .and_then(|mut conexiones| conexiones.remove(host_uuid));
        match conexion {
            Some(conexion) => {
                self.cerrar(conexion);
                true
            }
            None => false,
        }
    }

    /// Cerrojo por host para no abrir dos conexiones a la vez.
    fn cerrojo_de_host(&self, host_uuid: &str) -> Arc<tokio::sync::Mutex<()>> {
        if let Ok(mut cerrojos) = self.cerrojos.lock() {
            return cerrojos
                .entry(host_uuid.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone();
        }
        Arc::new(tokio::sync::Mutex::new(()))
    }

    /// Toma un uso si la conexión está viva o en gracia vigente.
    fn tomar_viva(self: &Arc<Self>, host_uuid: &str) -> Option<UsoConexion> {
        let ahora = self.reloj.ahora();
        let conexiones = self.conexiones.lock().ok()?;
        let conexion = conexiones.get(host_uuid)?;
        if !conexion.disponible(ahora) {
            return None;
        }
        conexion.tomar_uso();
        Some(self.uso(host_uuid, conexion.clone()))
    }

    /// Retira del mapa una conexión en gracia vencida (identidad incluida) para
    /// poder abrir otra debajo del cerrojo.
    fn retirar_vencida(&self, host_uuid: &str) -> Option<Arc<ConexionHost>> {
        let ahora = self.reloj.ahora();
        let mut conexiones = self.conexiones.lock().ok()?;
        let vencida = conexiones
            .get(host_uuid)
            .map(|conexion| !conexion.disponible(ahora))
            .unwrap_or(false);
        if vencida {
            conexiones.remove(host_uuid)
        } else {
            None
        }
    }

    fn uso(self: &Arc<Self>, host_uuid: &str, conexion: Arc<ConexionHost>) -> UsoConexion {
        UsoConexion::nuevo(Arc::clone(self), host_uuid.to_string(), conexion)
    }

    /// Programa el cierre de una conexión que acaba de quedarse sin usos. Al
    /// vencer la gracia comprueba identidad y usos bajo el cerrojo del mapa:
    /// una conexión que alguien volvió a pedir (usos > 0) no se cierra.
    fn programar_cierre_en_gracia(
        self: &Arc<Self>,
        host_uuid: String,
        conexion: Arc<ConexionHost>,
    ) {
        let gestor = Arc::clone(self);
        self.manejador.spawn(async move {
            tokio::time::sleep(Duration::from_secs(GRACIA_SEG)).await;
            let ahora = gestor.reloj.ahora();
            let retirada = {
                let Ok(mut conexiones) = gestor.conexiones.lock() else {
                    return;
                };
                let misma = conexiones
                    .get(&host_uuid)
                    .map(|actual| Arc::ptr_eq(actual, &conexion))
                    .unwrap_or(false);
                if !misma || !conexion.cerrar_por_gracia(ahora) {
                    return;
                }
                conexiones.remove(&host_uuid)
            };
            if let Some(retirada) = retirada {
                gestor.cerrar(retirada);
            }
        });
    }

    /// Vigila que la conexión siga viva cada VIGILANTE_MS (T97). Al caer, la
    /// retira del mapa (por identidad), marca sesiones y túneles y avisa a los
    /// oyentes.
    fn vigilar(
        self: &Arc<Self>,
        estado: Arc<EstadoNucleo>,
        host_uuid: String,
        conexion: &Arc<ConexionHost>,
    ) {
        let debil = Arc::downgrade(conexion);
        let handle = conexion.handle.clone();
        let manejador = self.manejador.clone();
        let gestor = Arc::clone(self);
        manejador.spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(VIGILANTE_MS)).await;
                if debil.upgrade().is_none() {
                    // La conexión se soltó a propósito.
                    return;
                }
                if handle.is_closed() {
                    gestor.conexion_perdida(&estado, &host_uuid, &debil).await;
                    return;
                }
            }
        });
    }

    /// La conexión del host se cerró sin que nadie la parara: se retira del
    /// mapa, las sesiones pasan a caídas, los túneles a caídos y los oyentes
    /// reciben el motivo.
    async fn conexion_perdida(
        self: &Arc<Self>,
        estado: &Arc<EstadoNucleo>,
        host_uuid: &str,
        debil: &Weak<ConexionHost>,
    ) {
        {
            let Some(viva) = debil.upgrade() else {
                return;
            };
            let Ok(mut conexiones) = self.conexiones.lock() else {
                return;
            };
            let coincide = conexiones
                .get(host_uuid)
                .map(|actual| Arc::ptr_eq(actual, &viva))
                .unwrap_or(false);
            if !coincide {
                return;
            }
            conexiones.remove(host_uuid);
        }
        marcar_sesiones_caidas(estado, host_uuid, MOTIVO_CAIDA);
        crate::gestor::marcar_tuneles_caidos(estado, host_uuid, MOTIVO_CAIDA);
        estado.anotar_error(host_uuid, MOTIVO_CAIDA);
        let host_uuid_bd = host_uuid.to_string();
        let _ = estado.escritor.con_almacen(move |almacen| {
            movil_bd::marcar_estado(
                almacen.conexion(),
                &host_uuid_bd,
                Some(magi_nucleo::modelo::UltimoEstado::Error),
            )
        });
        estado.avisar_conexion_caida(host_uuid, MOTIVO_CAIDA);
    }

    /// Desconecta el handle con plazo corto, en segundo plano.
    fn cerrar(&self, conexion: Arc<ConexionHost>) {
        let manejador = self.manejador.clone();
        manejador.spawn(async move {
            let handle = conexion.handle.clone();
            let _ = tokio::time::timeout(
                Duration::from_secs(5),
                handle.disconnect(russh::Disconnect::ByApplication, "", ""),
            )
            .await;
        });
    }

    /// Abre la conexión del host con la política pedida. No la publica en el
    /// mapa: eso lo hace conexion_para_host con el cerrojo tomado.
    async fn abrir(
        self: &Arc<Self>,
        estado: &Arc<EstadoNucleo>,
        host_uuid: &str,
        politica: Politica,
    ) -> Result<Arc<ConexionHost>, ErrorNucleo> {
        let (host, otros, uuids) = con_almacen(estado, {
            let host_uuid = host_uuid.to_string();
            move |almacen| {
                let conexion = almacen.conexion();
                let host = movil_bd::host_por_uuid(conexion, &host_uuid)?;
                let otros = hosts_bd::listar(conexion)?;
                let uuids =
                    movil_bd::uuid_por_id(conexion, magi_nucleo::almacen::versiones::Tabla::Hosts)?;
                Ok::<_, anyhow::Error>((host, otros, uuids))
            }
        })
        .map_err(|error| ErrorNucleo::General {
            detalle: error.to_string(),
        })?
        .map_err(|error| ErrorNucleo::General {
            detalle: format!("leyendo el host: {error:#}"),
        })?;
        let Some(host) = host else {
            return Err(ErrorNucleo::NoEncontrado {
                detalle: format!("el host {host_uuid}"),
            });
        };
        let mapa: HashMap<i64, Host> = otros.into_iter().map(|host| (host.id, host)).collect();
        let cadena = construir_cadena(&host, &mapa).map_err(|error| ErrorNucleo::General {
            detalle: error.to_string(),
        })?;
        let saltos_uuids: Vec<String> = cadena
            .iter()
            .skip(1)
            .filter_map(|salto| uuids.get(&salto.id).cloned())
            .collect();
        let (tx, rx) = mpsc::unbounded_channel::<EventoConexion>();
        lanzar_bucle_eventos(estado.clone(), rx);
        let reenvios = Arc::new(Reenvios::default());
        let contexto = contexto(estado, &host, politica, tx.clone(), Some(reenvios.clone()));
        let clave = clave_movil(estado)?;
        let transporte = conectar_cadena_movil(&cadena, &contexto, &clave)
            .await
            .map_err(|error| {
                let motivo = error.to_string();
                let traducido = traducir_error(&motivo, politica);
                // El último error de la lista enseña la instrucción de la
                // política automática, no el texto crudo del núcleo.
                estado.anotar_error(host_uuid, &traducido.to_string());
                registrar_fallo(estado, host.id, &motivo);
                traducido
            })?;
        estado.limpiar_error(host_uuid);
        let identidad = transporte.identidad.descripcion.clone();
        let host_id = host.id;
        let _ = estado.escritor.con_almacen(move |almacen| {
            registro::anotar(
                almacen.conexion(),
                registro::CONEXION_ABIERTA,
                Some(host_id),
                None,
                &format!("móvil · {identidad}"),
                ResultadoRegistro::Ok,
            )
        });
        Ok(Arc::new(ConexionHost {
            handle: transporte.handle,
            saltos: transporte.saltos,
            reenvios,
            tx,
            saltos_uuids,
            politica,
            contabilidad: Mutex::new(Contabilidad::default()),
        }))
    }
}

/// Toma un uso de la conexión del host con la política pedida (F14, D277). Es
/// el punto de entrada que usan sesiones, túneles, sondeo, servicios, logs,
/// ejecuciones y el SFTP de BALTHASAR-2.
pub async fn conexion_para_host(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    politica: Politica,
) -> Result<UsoConexion, ErrorNucleo> {
    estado
        .conexiones
        .conexion_para_host(estado, host_uuid, politica)
        .await
}

/// Toma un uso interactivo (lo pidió el usuario y puede dialogar).
pub async fn conexion_interactiva(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<UsoConexion, ErrorNucleo> {
    conexion_para_host(estado, host_uuid, Politica::Interactiva).await
}

/// Toma un uso automático (sondeo, refresco, comprobación o ejecución): nunca
/// abre diálogos; los fallos de huella o credenciales vuelven con instrucción.
pub async fn conexion_automatica(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<UsoConexion, ErrorNucleo> {
    conexion_para_host(estado, host_uuid, Politica::Automatica).await
}

/// Salida de una orden remota lanzada con ejecutar_remoto, con cada flujo
/// acotado al tope pedido.
#[derive(Debug, Default)]
pub struct SalidaRemota {
    /// Código de salida, si el host lo mandó antes de cerrar el canal.
    pub codigo: Option<u32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Verdadero si se descartó salida por pasar del tope.
    pub stdout_truncada: bool,
    pub stderr_truncada: bool,
}

/// Lanza una orden remota por un canal exec de la conexión del host. La usan
/// el sondeo, los servicios, el sudo y las ejecuciones de snippets.
///
/// El comando va ya resuelto por el núcleo (sus scripts embebidos y argumentos
/// validados): aquí nunca se interpola nada del inventario. La entrada, si la
/// hay, se escribe entera y se manda EOF después (es como los scripts del host
/// saben que no hay más; la contraseña de sudo viaja en su primera línea).
///
/// El plazo cubre la apertura del canal, el exec, la escritura y la lectura
/// enteras. Al vencer se cierra el canal en segundo plano y se devuelve un
/// error legible. Cada flujo se acumula como mucho tope_por_flujo bytes: lo que
/// sobra se descarta y se marca como truncado, sin reservar más memoria.
pub async fn ejecutar_remoto(
    conexion: &UsoConexion,
    comando: &str,
    entrada: Option<&[u8]>,
    plazo: Duration,
    tope_por_flujo: usize,
) -> Result<SalidaRemota, ErrorNucleo> {
    ejecutar_remoto_con(conexion, comando, entrada, plazo, tope_por_flujo, None).await
}

/// Igual que ejecutar_remoto, pero se puede cancelar: al recibir true se cierra
/// el canal y se devuelve ErrorNucleo::Cancelado. Lo usa el motor de
/// ejecuciones de snippets para cancelar sin dejar el canal huérfano.
pub async fn ejecutar_remoto_cancelable(
    conexion: &UsoConexion,
    comando: &str,
    entrada: Option<&[u8]>,
    plazo: Duration,
    tope_por_flujo: usize,
    cancelacion: watch::Receiver<bool>,
) -> Result<SalidaRemota, ErrorNucleo> {
    ejecutar_remoto_con(
        conexion,
        comando,
        entrada,
        plazo,
        tope_por_flujo,
        Some(cancelacion),
    )
    .await
}

/// Implementación común del ejecutor remoto, con o sin cancelación.
async fn ejecutar_remoto_con(
    conexion: &UsoConexion,
    comando: &str,
    entrada: Option<&[u8]>,
    plazo: Duration,
    tope_por_flujo: usize,
    mut cancelacion: Option<watch::Receiver<bool>>,
) -> Result<SalidaRemota, ErrorNucleo> {
    if conexion.handle.is_closed() {
        return Err(ErrorNucleo::General {
            detalle: "la conexión con el host está cerrada".to_string(),
        });
    }
    if cancelacion
        .as_ref()
        .is_some_and(|cancelacion| *cancelacion.borrow())
    {
        return Err(ErrorNucleo::Cancelado);
    }
    let inicio = Instant::now();
    let limite = inicio + plazo;
    let handle = conexion.handle.clone();
    let mut canal = abrir_con_plazo(plazo, async move { handle.channel_open_session().await })
        .await
        .map_err(|detalle| ErrorNucleo::General {
            detalle: format!("no se pudo abrir el canal: {detalle}"),
        })?;
    let restante = limite.saturating_duration_since(Instant::now());
    let mut cancelado = false;
    let resultado = if restante.is_zero() {
        None
    } else {
        let operacion = operacion_remota(&mut canal, comando, entrada, tope_por_flujo);
        match cancelacion.as_mut() {
            Some(cancelacion) => {
                tokio::select! {
                    resultado = tokio::time::timeout(restante, operacion) => resultado.ok(),
                    _ = esperar_cancelacion(cancelacion) => {
                        cancelado = true;
                        None
                    }
                }
            }
            None => tokio::time::timeout(restante, operacion).await.ok(),
        }
    };
    // El canal se cierra siempre y en segundo plano: en una conexión
    // compartida, un canal huérfano agotaría MaxSessions (F8).
    cerrar_canal_en_segundo_plano(conexion, canal);
    if cancelado {
        return Err(ErrorNucleo::Cancelado);
    }
    match resultado {
        Some(Ok(salida)) => Ok(salida),
        Some(Err(error)) => Err(error),
        None => Err(ErrorNucleo::General {
            detalle: "se agotó el plazo de la orden remota".to_string(),
        }),
    }
}

/// Espera a que la cancelación pase a true, también si ya lo estaba al
/// empezar. Si el emisor desaparece no se cancela nunca (el motor de
/// ejecuciones mantiene el emisor hasta terminar).
async fn esperar_cancelacion(cancelacion: &mut watch::Receiver<bool>) {
    if *cancelacion.borrow() {
        return;
    }
    while cancelacion.changed().await.is_ok() {
        if *cancelacion.borrow() {
            return;
        }
    }
    std::future::pending::<()>().await;
}

/// Exec, entrada y lectura de la salida de una orden remota. El plazo total lo
/// pone quien llama (ejecutar_remoto).
async fn operacion_remota(
    canal: &mut russh::Channel<russh::client::Msg>,
    comando: &str,
    entrada: Option<&[u8]>,
    tope_por_flujo: usize,
) -> Result<SalidaRemota, ErrorNucleo> {
    canal
        .exec(true, comando.as_bytes().to_vec())
        .await
        .map_err(|error| ErrorNucleo::General {
            detalle: format!("no se pudo lanzar la orden: {error}"),
        })?;
    if let Some(entrada) = entrada {
        if !entrada.is_empty() {
            canal
                .data_bytes(entrada.to_vec())
                .await
                .map_err(|error| ErrorNucleo::General {
                    detalle: format!("el host cerró la entrada de la orden: {error}"),
                })?;
        }
    }
    canal.eof().await.map_err(|error| ErrorNucleo::General {
        detalle: format!("el host cerró la entrada de la orden: {error}"),
    })?;
    let mut salida = SalidaRemota::default();
    loop {
        match canal.wait().await {
            Some(ChannelMsg::Data { data }) => {
                salida.stdout_truncada |= acumular(&mut salida.stdout, &data, tope_por_flujo);
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                salida.stderr_truncada |= acumular(&mut salida.stderr, &data, tope_por_flujo);
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                salida.codigo = Some(exit_status);
            }
            Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                return Err(ErrorNucleo::General {
                    detalle: format!("el host cortó la orden con la señal {signal_name:?}"),
                });
            }
            Some(ChannelMsg::Failure) => {
                return Err(ErrorNucleo::General {
                    detalle: "el host rechazó la orden".to_string(),
                });
            }
            Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    Ok(salida)
}

/// Acumula datos en un flujo con tope, sin pasar nunca de el. Devuelve si hubo
/// que descartar algo (flujo truncado).
fn acumular(destino: &mut Vec<u8>, datos: &[u8], tope: usize) -> bool {
    if destino.len() >= tope {
        return !datos.is_empty();
    }
    let hueco = tope - destino.len();
    if datos.len() > hueco {
        destino.extend_from_slice(&datos[..hueco]);
        true
    } else {
        destino.extend_from_slice(datos);
        false
    }
}

/// Cierra el canal en segundo plano, pase lo que pase con la lectura.
fn cerrar_canal_en_segundo_plano(
    conexion: &UsoConexion,
    canal: russh::Channel<russh::client::Msg>,
) {
    conexion.gestor.manejador.spawn(async move {
        let _ = canal.close().await;
    });
}

/// Eventos de un canal exec en streaming (servicios: fases de una acción y
/// logs).
#[derive(Debug)]
pub enum EventoCanal {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    /// Código de salida del comando, si el host lo manda.
    Codigo(u32),
    /// El host cerró el canal (o la conexión se perdió).
    Fin,
}

/// Orden para la tarea que escribe en el canal.
enum OrdenCanal {
    Escribir(Vec<u8>, oneshot::Sender<Result<(), String>>),
    CerrarEntrada(oneshot::Sender<Result<(), String>>),
    Cerrar,
}

/// Canal exec en streaming sobre la conexión del host: la entrada estándar se
/// queda abierta hasta cerrar_entrada o cerrar, y la salida llega por eventos.
/// Lo usan los logs (suscripción larga con la contraseña de sudo en la primera
/// línea) y las fases de una acción de servicio.
///
/// El consumidor debe vaciar eventos con diligencia: el canal entrega sin tope
/// y quien lo lee aplica sus propios límites (el anillo de logs). El canal se
/// cierra en segundo plano al soltarlo (Drop) y al llamar a cerrar, también si
/// el consumidor de eventos desaparece: nunca queda un canal huérfano en una
/// conexión compartida (F8).
pub struct CanalRemoto {
    ordenes: mpsc::UnboundedSender<OrdenCanal>,
    pub eventos: mpsc::UnboundedReceiver<EventoCanal>,
}

impl CanalRemoto {
    /// Escribe en la entrada estándar del comando (por ejemplo la contraseña
    /// de sudo y su salto de línea).
    pub async fn escribir(&mut self, datos: &[u8]) -> Result<(), ErrorNucleo> {
        let (responder, receptor) = oneshot::channel();
        if self
            .ordenes
            .send(OrdenCanal::Escribir(datos.to_vec(), responder))
            .is_err()
        {
            return Err(canal_cerrado());
        }
        receptor
            .await
            .map_err(|_| canal_cerrado())?
            .map_err(|detalle| ErrorNucleo::General { detalle })
    }

    /// Manda el fin de la entrada estándar (EOF): es como los scripts del host
    /// saben que deben terminar (logs_parar).
    pub async fn cerrar_entrada(&mut self) -> Result<(), ErrorNucleo> {
        let (responder, receptor) = oneshot::channel();
        if self
            .ordenes
            .send(OrdenCanal::CerrarEntrada(responder))
            .is_err()
        {
            return Err(canal_cerrado());
        }
        receptor
            .await
            .map_err(|_| canal_cerrado())?
            .map_err(|detalle| ErrorNucleo::General { detalle })
    }

    /// Cierra el canal en segundo plano.
    pub fn cerrar(self) {
        let _ = self.ordenes.send(OrdenCanal::Cerrar);
    }
}

impl Drop for CanalRemoto {
    fn drop(&mut self) {
        // Si no se llamó a cerrar, la tarea cierra el canal igualmente.
        let _ = self.ordenes.send(OrdenCanal::Cerrar);
    }
}

fn canal_cerrado() -> ErrorNucleo {
    ErrorNucleo::General {
        detalle: "el canal remoto se cerró".to_string(),
    }
}

/// Abre un canal exec en streaming: el comando va ya resuelto por el núcleo y
/// la entrada estándar queda abierta para que el que llama escriba lo que
/// necesite. El plazo cubre solo la apertura y el exec (una suscripción de
/// logs dura horas; el consumidor pone su propio plazo al esperar eventos).
pub async fn abrir_canal_remoto(
    conexion: &UsoConexion,
    comando: &str,
) -> Result<CanalRemoto, ErrorNucleo> {
    if conexion.handle.is_closed() {
        return Err(ErrorNucleo::General {
            detalle: "la conexión con el host está cerrada".to_string(),
        });
    }
    let handle = conexion.handle.clone();
    let canal = abrir_con_plazo(
        PLAZO_CANAL,
        async move { handle.channel_open_session().await },
    )
    .await
    .map_err(|detalle| ErrorNucleo::General {
        detalle: format!("no se pudo abrir el canal: {detalle}"),
    })?;
    let exec =
        tokio::time::timeout(PLAZO_CANAL, canal.exec(true, comando.as_bytes().to_vec())).await;
    match exec {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            cerrar_canal_en_segundo_plano(conexion, canal);
            return Err(ErrorNucleo::General {
                detalle: format!("no se pudo lanzar la orden: {error}"),
            });
        }
        Err(_) => {
            cerrar_canal_en_segundo_plano(conexion, canal);
            return Err(ErrorNucleo::General {
                detalle: "se agotó el plazo lanzando la orden".to_string(),
            });
        }
    }
    let (ordenes, receptor_ordenes) = mpsc::unbounded_channel();
    let (emisor_eventos, eventos) = mpsc::unbounded_channel();
    let manejador = conexion.gestor.manejador.clone();
    manejador.spawn(async move {
        tarea_canal_remoto(canal, receptor_ordenes, emisor_eventos).await;
    });
    Ok(CanalRemoto { ordenes, eventos })
}

/// Bucle del canal: atiende las órdenes de la entrada estándar y traduce los
/// mensajes del host a eventos. Al salir cierra el canal siempre.
async fn tarea_canal_remoto(
    mut canal: russh::Channel<russh::client::Msg>,
    mut ordenes: mpsc::UnboundedReceiver<OrdenCanal>,
    eventos: mpsc::UnboundedSender<EventoCanal>,
) {
    loop {
        tokio::select! {
            orden = ordenes.recv() => match orden {
                Some(OrdenCanal::Escribir(datos, responder)) => {
                    let envio = match tokio::time::timeout(
                        PLAZO_CANAL,
                        canal.data_bytes(datos),
                    )
                    .await
                    {
                        Ok(Ok(())) => Ok(()),
                        Ok(Err(error)) => {
                            Err(format!("no se pudo escribir en el canal: {error}"))
                        }
                        Err(_) => {
                            Err("se agotó el plazo escribiendo en el canal".to_string())
                        }
                    };
                    let _ = responder.send(envio);
                }
                Some(OrdenCanal::CerrarEntrada(responder)) => {
                    let envio =
                        match tokio::time::timeout(PLAZO_CANAL, canal.eof()).await {
                            Ok(Ok(())) => Ok(()),
                            Ok(Err(error)) => {
                                Err(format!("no se pudo cerrar la entrada: {error}"))
                            }
                            Err(_) => {
                                Err("se agotó el plazo cerrando la entrada".to_string())
                            }
                        };
                    let _ = responder.send(envio);
                }
                Some(OrdenCanal::Cerrar) | None => break,
            },
            mensaje = canal.wait() => match mensaje {
                Some(ChannelMsg::Data { data }) => {
                    if eventos.send(EventoCanal::Stdout(data.to_vec())).is_err() {
                        break;
                    }
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if eventos.send(EventoCanal::Stderr(data.to_vec())).is_err() {
                        break;
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    let _ = eventos.send(EventoCanal::Codigo(exit_status));
                }
                Some(ChannelMsg::Failure) => {
                    let _ = eventos.send(EventoCanal::Fin);
                    break;
                }
                Some(ChannelMsg::Close) | None => {
                    let _ = eventos.send(EventoCanal::Fin);
                    break;
                }
                _ => {}
            },
        }
    }
    let _ = tokio::time::timeout(PLAZO_CANAL, canal.close()).await;
}

/// Contexto del núcleo para una apertura: la política decide interactivo y la
/// contraseña sale del llavero del móvil.
pub(crate) fn contexto(
    estado: &EstadoNucleo,
    host: &Host,
    politica: Politica,
    tx: mpsc::UnboundedSender<EventoConexion>,
    reenvios: Option<Arc<Reenvios>>,
) -> Contexto {
    Contexto {
        known_hosts: estado.rutas.fichero_known_hosts(),
        dir_ssh: estado.rutas.dir_ssh(),
        hogar: estado.rutas.hogar.clone(),
        usuario_local: host
            .usuario
            .clone()
            .unwrap_or_else(|| "usuario".to_string()),
        tx,
        interactivo: politica.interactiva(),
        fuente_contrasena: FuenteContrasena::Llavero,
        reenvios,
        llavero: estado.llavero.clone(),
    }
}

/// La clave del móvil lista para autenticar.
pub(crate) fn clave_movil(estado: &EstadoNucleo) -> Result<ClaveMovil, ErrorNucleo> {
    let publica = estado.clave_publica()?;
    Ok(ClaveMovil {
        linea_publica: publica.linea_openssh,
        firmante: estado.firmante.clone(),
    })
}

/// Abre una conexión efímera de la cadena con la política pedida, fuera del
/// mapa del host: la usan «instalar mi clave» y su comprobación, que necesitan
/// autenticarse con contraseña al margen de la conexión compartida.
pub(crate) async fn conectar_efimera(
    estado: &Arc<EstadoNucleo>,
    cadena: &[Host],
    host: &Host,
    politica: Politica,
    con_clave_movil: bool,
) -> Result<Transporte, ErrorNucleo> {
    let (tx, rx) = mpsc::unbounded_channel::<EventoConexion>();
    lanzar_bucle_eventos(estado.clone(), rx);
    let contexto = contexto(estado, host, politica, tx, None);
    let resultado = if con_clave_movil {
        let clave = clave_movil(estado)?;
        conectar_cadena_movil(cadena, &contexto, &clave).await
    } else {
        conectar_cadena(cadena, &contexto).await
    };
    resultado.map_err(|error| traducir_error(&error.to_string(), politica))
}

/// Marca caídas las sesiones abiertas del host.
fn marcar_sesiones_caidas(estado: &EstadoNucleo, host_uuid: &str, motivo: &str) {
    if let Ok(sesiones) = estado.sesiones.lock() {
        for sesion in sesiones.values() {
            if sesion.host_uuid != host_uuid {
                continue;
            }
            if matches!(
                sesion.estado(),
                EstadoSesion::Abierta | EstadoSesion::Abriendo
            ) {
                sesion.fijar_estado(EstadoSesion::Caida, Some(motivo.to_string()));
            }
        }
    }
    estado.emitir_sesiones();
}

/// Traduce el fallo de una apertura automática a un error con instrucción
/// (D277, §7.2). Las conexiones interactivas conservan el motivo: sus fallos se
/// resuelven con los diálogos de la Fase 11.
///
/// Este es el único punto donde se decide qué instrucción ve la app; si
/// cambian las variantes tipadas de ErrorNucleo solo hay que cambiar aquí.
fn traducir_error(motivo: &str, politica: Politica) -> ErrorNucleo {
    let motivo = motivo.trim();
    if politica == Politica::Interactiva || motivo.is_empty() {
        return ErrorNucleo::General {
            detalle: motivo.to_string(),
        };
    }
    let minusculas = motivo.to_lowercase();
    if minusculas.contains("huella desconocida") || minusculas.contains("huella cambiada") {
        return ErrorNucleo::HuellaSinAceptar;
    }
    if minusculas.contains("contraseña")
        || minusculas.contains("credencial")
        || minusculas.contains("no acepta la clave del móvil")
    {
        return ErrorNucleo::SinCredenciales;
    }
    if minusculas.contains("agotó")
        || minusculas.contains("no se pudo conectar")
        || minusculas.contains("saludo ssh")
        || minusculas.contains("abriendo el canal")
        || minusculas.contains("timed out")
        || minusculas.contains("timeout")
        || minusculas.contains("io error")
        || minusculas.contains("connection reset")
        || minusculas.contains("connection refused")
        || minusculas.contains("network")
        || minusculas.contains("no route")
    {
        // §4.1 y §7.2 dan la instrucción exacta «no responde en 5 s». La
        // variante tipada NoResponde de api.rs dice «el host no responde»:
        // cuando se alinee, esto pasa a ser ErrorNucleo::NoResponde.
        return ErrorNucleo::General {
            detalle: "no responde en 5 s".to_string(),
        };
    }
    // Sin instrucción concreta (configuración rota, host que ya no está): se
    // deja el motivo tal cual, que también es accionable.
    ErrorNucleo::General {
        detalle: motivo.to_string(),
    }
}

/// Traduce los eventos de conexión a diálogos de la app y respuestas.
pub(crate) fn lanzar_bucle_eventos(
    estado: Arc<EstadoNucleo>,
    mut eventos: mpsc::UnboundedReceiver<EventoConexion>,
) {
    let manejador = estado.runtime.handle().clone();
    manejador.spawn(async move {
        while let Some(evento) = eventos.recv().await {
            match evento {
                EventoConexion::HuellaDesconocida {
                    host,
                    tipo,
                    huella,
                    responder,
                } => {
                    let datos = datos_host_por_direccion(&estado, &host);
                    let (id, receptor) = estado.dialogos.registrar();
                    estado
                        .retrollamadas
                        .oyente
                        .dialogo(PeticionDialogo::HuellaNueva {
                            id,
                            host_uuid: datos.as_ref().map(|d| d.0.clone()).unwrap_or_default(),
                            host_nombre: datos
                                .as_ref()
                                .map(|d| d.1.clone())
                                .unwrap_or_else(|| host.clone()),
                            tipo,
                            sha256: huella,
                        });
                    let aceptado = matches!(receptor.await, Ok(RespuestaDialogo::AceptarHuella));
                    let _ = responder.send(aceptado);
                }
                EventoConexion::HuellaCambiada {
                    host,
                    tipo,
                    anterior,
                    nueva,
                    responder,
                } => {
                    let datos = datos_host_por_direccion(&estado, &host);
                    let nombre = datos
                        .as_ref()
                        .map(|d| d.1.clone())
                        .unwrap_or_else(|| host.clone());
                    let (id, receptor) = estado.dialogos.registrar();
                    estado
                        .retrollamadas
                        .oyente
                        .dialogo(PeticionDialogo::HuellaCambiada {
                            id,
                            host_uuid: datos.as_ref().map(|d| d.0.clone()).unwrap_or_default(),
                            host_nombre: nombre.clone(),
                            tipo,
                            anterior,
                            nueva,
                            nombre_a_escribir: nombre.clone(),
                        });
                    let sustituir = match receptor.await {
                        Ok(RespuestaDialogo::SustituirHuella { nombre_escrito }) => {
                            nombre_escrito.trim() == nombre.trim()
                        }
                        _ => false,
                    };
                    let _ = responder.send(sustituir);
                }
                EventoConexion::PideContrasena {
                    host,
                    intento,
                    recordar_por_defecto,
                    responder,
                } => {
                    let datos = datos_host_por_direccion(&estado, &host);
                    let (id, receptor) = estado.dialogos.registrar();
                    estado
                        .retrollamadas
                        .oyente
                        .dialogo(PeticionDialogo::Contrasena {
                            id,
                            host_uuid: datos.as_ref().map(|d| d.0.clone()).unwrap_or_default(),
                            host_nombre: datos
                                .as_ref()
                                .map(|d| d.1.clone())
                                .unwrap_or_else(|| host.clone()),
                            usuario: datos.as_ref().map(|d| d.2.clone()).unwrap_or_default(),
                            direccion: host,
                            intentos_restantes: u32::from(4u8.saturating_sub(intento)),
                            recordar_ofrecido: recordar_por_defecto,
                        });
                    match receptor.await {
                        Ok(RespuestaDialogo::Contrasena { bytes, recordar }) => {
                            let bytes = zeroize::Zeroizing::new(bytes);
                            match std::str::from_utf8(&bytes) {
                                Ok(texto) => {
                                    let contrasena = zeroize::Zeroizing::new(texto.to_string());
                                    let _ = responder.send(Some((contrasena, recordar)));
                                }
                                Err(_) => {
                                    // El contrato exige texto UTF-8: no se
                                    // corrompe una contraseña en silencio.
                                    let _ = responder.send(None);
                                }
                            }
                        }
                        _ => {
                            let _ = responder.send(None);
                        }
                    }
                }
                EventoConexion::PideContrasenaLlavero {
                    host,
                    usuario,
                    responder,
                } => {
                    let contrasena = magi_nucleo::llavero::LlaveroHost::recuperar_contrasena(
                        estado.llavero.as_ref(),
                        &host,
                        &usuario,
                    )
                    .ok()
                    .flatten();
                    let _ = responder.send(contrasena);
                }
                EventoConexion::PideFrase { responder, .. } => {
                    let _ = responder.send(None);
                }
                EventoConexion::HuellaRegistrada {
                    host_id,
                    anterior,
                    nueva,
                } => {
                    let tipo = if anterior.is_some() {
                        registro::HUELLA_SUSTITUIDA
                    } else {
                        registro::HUELLA_ACEPTADA
                    };
                    let detalle = match &anterior {
                        Some(anterior) => format!("anterior {anterior}; nueva {nueva}"),
                        None => format!("nueva {nueva}"),
                    };
                    let _ = estado.escritor.con_almacen(move |almacen| {
                        registro::anotar(
                            almacen.conexion(),
                            tipo,
                            Some(host_id),
                            None,
                            &detalle,
                            ResultadoRegistro::Ok,
                        )
                    });
                }
                _ => {}
            }
        }
    });
}

fn datos_host_por_direccion(
    estado: &EstadoNucleo,
    direccion: &str,
) -> Option<(String, String, String)> {
    let direccion = direccion.to_string();
    con_almacen(estado, move |almacen| {
        let conexion = almacen.conexion();
        let hosts = hosts_bd::listar(conexion).ok()?;
        let host = hosts.into_iter().find(|host| host.direccion == direccion)?;
        let uuid = movil_bd::uuid_por_id(conexion, magi_nucleo::almacen::versiones::Tabla::Hosts)
            .ok()?
            .get(&host.id)
            .cloned()
            .unwrap_or_default();
        Some((
            uuid,
            host.nombre.clone(),
            host.usuario.clone().unwrap_or_default(),
        ))
    })
    .ok()
    .flatten()
}

fn registrar_fallo(estado: &EstadoNucleo, host_id: i64, motivo: &str) {
    let motivo = motivo.to_string();
    let _ = estado.escritor.con_almacen(move |almacen| {
        registro::anotar(
            almacen.conexion(),
            registro::CONEXION_FALLIDA,
            Some(host_id),
            None,
            &motivo,
            ResultadoRegistro::Error,
        )
    });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Reloj manual: la gracia se prueba sin dormir ni tocar la red.
    struct RelojManual {
        inicio: Instant,
        desfase: Mutex<Duration>,
    }

    impl RelojManual {
        fn nuevo() -> Self {
            Self {
                inicio: Instant::now(),
                desfase: Mutex::new(Duration::ZERO),
            }
        }

        fn avanzar(&self, segundos: u64) {
            if let Ok(mut desfase) = self.desfase.lock() {
                *desfase += Duration::from_secs(segundos);
            }
        }
    }

    impl Reloj for RelojManual {
        fn ahora(&self) -> Instant {
            self.inicio + *self.desfase.lock().expect("el reloj de la prueba")
        }
    }

    #[test]
    fn dos_usos_no_cierran_la_conexion_al_soltar_uno() {
        let reloj = RelojManual::nuevo();
        let mut contabilidad = Contabilidad::default();
        contabilidad.tomar();
        contabilidad.tomar();
        assert!(!contabilidad.soltar(reloj.ahora()));
        assert_eq!(contabilidad.usos(), 1);
        assert!(!contabilidad.en_gracia(reloj.ahora()));
        assert!(contabilidad.disponible(reloj.ahora()));
        // Con un uso vivo la conexión no vence ni aunque pase una hora.
        reloj.avanzar(3_600);
        assert!(contabilidad.disponible(reloj.ahora()));
        assert!(!contabilidad.gracia_vencida(reloj.ahora()));
    }

    #[test]
    fn el_ultimo_uso_deja_la_conexion_en_gracia_sesenta_segundos() {
        let reloj = RelojManual::nuevo();
        let mut contabilidad = Contabilidad::default();
        contabilidad.tomar();
        contabilidad.tomar();
        assert!(!contabilidad.soltar(reloj.ahora()));
        assert!(contabilidad.soltar(reloj.ahora()));
        assert_eq!(contabilidad.usos(), 0);
        assert!(contabilidad.en_gracia(reloj.ahora()));
        assert!(contabilidad.disponible(reloj.ahora()));
        reloj.avanzar(GRACIA_SEG - 1);
        assert!(contabilidad.en_gracia(reloj.ahora()));
        assert!(contabilidad.disponible(reloj.ahora()));
        reloj.avanzar(1);
        assert!(!contabilidad.en_gracia(reloj.ahora()));
        assert!(!contabilidad.disponible(reloj.ahora()));
        assert!(contabilidad.gracia_vencida(reloj.ahora()));
    }

    #[test]
    fn pedir_en_gracia_reutiliza_la_conexion_sin_autenticar_otra_vez() {
        let reloj = RelojManual::nuevo();
        let mut contabilidad = Contabilidad::default();
        contabilidad.tomar();
        assert!(contabilidad.soltar(reloj.ahora()));
        assert!(contabilidad.en_gracia(reloj.ahora()));
        // Alguien la pide dentro de la gracia (el refresco de Flota a los 30 s).
        reloj.avanzar(30);
        assert!(contabilidad.disponible(reloj.ahora()));
        contabilidad.tomar();
        assert_eq!(contabilidad.usos(), 1);
        assert!(!contabilidad.en_gracia(reloj.ahora()));
        assert!(contabilidad.disponible(reloj.ahora()));
        // Al volver a soltarla, la gracia empieza de nuevo.
        assert!(contabilidad.soltar(reloj.ahora()));
        assert!(contabilidad.en_gracia(reloj.ahora()));
    }

    #[test]
    fn la_gracia_vencida_cierra_la_conexion() {
        let reloj = RelojManual::nuevo();
        let mut contabilidad = Contabilidad::default();
        contabilidad.tomar();
        assert!(!contabilidad.cerrar_por_gracia(reloj.ahora()));
        assert!(contabilidad.soltar(reloj.ahora()));
        reloj.avanzar(GRACIA_SEG);
        assert!(contabilidad.cerrar_por_gracia(reloj.ahora()));
    }

    #[test]
    fn el_cierre_por_gracia_nunca_cierra_una_conexion_con_usos() {
        let reloj = RelojManual::nuevo();
        let mut contabilidad = Contabilidad::default();
        contabilidad.tomar();
        contabilidad.tomar();
        assert!(!contabilidad.soltar(reloj.ahora()));
        reloj.avanzar(GRACIA_SEG * 10);
        assert_eq!(contabilidad.usos(), 1);
        assert!(!contabilidad.cerrar_por_gracia(reloj.ahora()));
    }

    #[test]
    fn soltar_por_identidad_no_descuenta_dos_veces_ni_reinicia_la_gracia() {
        let reloj = RelojManual::nuevo();
        let mut contabilidad = Contabilidad::default();
        contabilidad.tomar();
        assert!(contabilidad.soltar(reloj.ahora()));
        let vencimiento = contabilidad.gracia_hasta;
        reloj.avanzar(10);
        // Un segundo soltar del mismo uso no descuenta (usos ya a cero) ni
        // alarga la gracia.
        assert!(!contabilidad.soltar(reloj.ahora()));
        assert_eq!(contabilidad.usos(), 0);
        assert_eq!(contabilidad.gracia_hasta, vencimiento);
        assert!(contabilidad.en_gracia(reloj.ahora()));
        reloj.avanzar(GRACIA_SEG - 10);
        assert!(contabilidad.gracia_vencida(reloj.ahora()));
    }

    #[test]
    fn una_apertura_automatica_traduce_los_fallos_con_instruccion() {
        let huella = traducir_error(
            "huella desconocida de 192.0.2.10 en known_hosts; conéctate una vez con ↵",
            Politica::Automatica,
        );
        assert!(matches!(huella, ErrorNucleo::HuellaSinAceptar));
        assert_eq!(
            huella.to_string(),
            "huella sin aceptar: conéctate una vez desde la ficha"
        );
        let credenciales = traducir_error(
            "«web-01» no acepta la clave del móvil y no hay contraseña guardada en este móvil",
            Politica::Automatica,
        );
        assert!(matches!(credenciales, ErrorNucleo::SinCredenciales));
        assert_eq!(
            credenciales.to_string(),
            "sin credenciales: instala tu clave o guarda la contraseña"
        );
        let red = traducir_error(
            "se agotó el tiempo de conexión con 192.0.2.10:22 (10 s)",
            Politica::Automatica,
        );
        assert_eq!(red.to_string(), "no responde en 5 s");
        // Un fallo sin instrucción concreta conserva el motivo: también es
        // accionable (configuración rota, host que ya no existe).
        let otro = traducir_error(
            "la cadena de saltos supera los 3 niveles",
            Politica::Automatica,
        );
        assert_eq!(otro.to_string(), "la cadena de saltos supera los 3 niveles");
    }

    #[test]
    fn una_apertura_interactiva_conserva_el_motivo_del_nucleo() {
        // En interactiva los diálogos de la Fase 11 resuelven el fallo: no se
        // sustituye por una instrucción.
        let error = traducir_error(
            "huella desconocida de 192.0.2.10 en known_hosts",
            Politica::Interactiva,
        );
        assert_eq!(
            error.to_string(),
            "huella desconocida de 192.0.2.10 en known_hosts"
        );
        assert!(Politica::Interactiva.interactiva());
        assert!(!Politica::Automatica.interactiva());
    }

    #[test]
    fn el_tope_de_un_flujo_corta_sin_pasarse() {
        let mut buffer = Vec::new();
        assert!(!acumular(&mut buffer, b"abc", 5));
        assert_eq!(buffer, b"abc");
        assert!(!acumular(&mut buffer, b"de", 5));
        assert_eq!(buffer, b"abcde");
        assert!(!acumular(&mut buffer, b"", 5));
        assert!(acumular(&mut buffer, b"f", 5));
        assert_eq!(buffer, b"abcde");
        assert_eq!(buffer.len(), 5);
    }

    #[test]
    fn un_tope_cero_no_acumula_nada() {
        let mut buffer = Vec::new();
        assert!(acumular(&mut buffer, b"x", 0));
        assert!(buffer.is_empty());
        assert!(!acumular(&mut buffer, b"", 0));
    }

    #[test]
    fn lo_que_llega_en_varios_tramos_se_corta_una_sola_vez() {
        let mut buffer = Vec::new();
        assert!(!acumular(&mut buffer, b"1234", 6));
        assert!(!acumular(&mut buffer, b"56", 6));
        assert!(acumular(&mut buffer, b"789", 6));
        assert_eq!(buffer, b"123456");
    }

    #[test]
    fn cada_conexion_lleva_su_propia_cuenta() {
        let reloj = RelojManual::nuevo();
        let mut primera = Contabilidad::default();
        let mut segunda = Contabilidad::default();
        primera.tomar();
        segunda.tomar();
        segunda.tomar();
        assert!(primera.soltar(reloj.ahora()));
        // Soltar un uso de la primera no descuenta los de la segunda.
        assert_eq!(segunda.usos(), 2);
        assert!(!segunda.en_gracia(reloj.ahora()));
        assert!(primera.en_gracia(reloj.ahora()));
    }
}
