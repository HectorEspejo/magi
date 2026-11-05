//! Servidor HTTP y WebSocket del visor (Fase 12, §5.1, §7.3 y §7.4).
//!
//! Vive dentro de magi --servidor y escucha SOLO en 127.0.0.1 (puerto libre o
//! [escritorio] puerto_visor). El servidor de escritorios lo arranca con la
//! primera sesión y lo para con la última.
//!
//! - Enlaces de un solo uso de 256 bits y 60 s, gastados al canjearlos aunque
//!   el canje falle, anulados al cerrar la sesión; el canje da una clave por
//!   sesión (que invalida la anterior) o, en voxl, la URL del túnel.
//! - Host comprobado en todas las rutas (anti DNS rebinding) y Origin
//!   obligatorio en /api/; CSP estricta y nada cargado de fuera.
//! - Los ficheros del visor van embebidos en el binario: compilar MAGI no
//!   necesita Node.
//! - Un visor por sesión: el nuevo sustituye al anterior, que recibe el estado
//!   otra_ventana y se le cierra el socket.
//! - El WebSocket exige un primer mensaje de hola con la clave en 5 s; después
//!   el puente manda hello y los marcos.
//!
//! Este módulo guarda el estado que consulta el servidor de escritorios (visor
//! conectado sí o no, fps y tamaño) y le avisa por un canal de lo que necesita:
//! sesión abierta, caída o cerrada, visor conectado o desconectado, navegador
//! sin H.264 y reconectar.
//!
//! El ciclo de vida del listener (arrancar y parar) va serializado por una
//! generación, para que una carrera entre ambos no deje el puerto publicado sin
//! listener ni un listener vivo con el puerto a cero (revisión adversarial,
//! M4). Cada escritura del WebSocket lleva plazo: un visor que deja de leer no
//! bloquea la tarea ni impide que empiece la gracia (M5).

use std::collections::{HashMap, VecDeque};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use super::puente::{
    self, ComandoEstado, ComandoPuente, Compartido, InstruccionVisor, MensajeVisor, SalidaVisor,
};
use crate::escritorio::{
    EventoEscritorio, ImagenEscritorio, InfoEscritorio, OrdenEscritorio, TipoHost,
};

// ---------------------------------------------------------------- constantes

/// Vida de un enlace de un solo uso (§7.3).
pub const VIDA_ENLACE: Duration = Duration::from_secs(60);
/// Canjes fallidos que bloquean el endpoint.
pub const MAX_FALLOS_CANJE: usize = 10;
/// Ventana en la que se cuentan los canjes fallidos.
pub const VENTANA_FALLOS: Duration = Duration::from_secs(60);
/// Duración del bloqueo 429.
pub const BLOQUEO_FALLOS: Duration = Duration::from_secs(60);
/// Plazo para el primer mensaje del visor.
pub const ESPERA_HOLA: Duration = Duration::from_secs(5);
/// Cada cuánto se manda el ping de latencia.
pub const INTERVALO_PING: Duration = Duration::from_secs(1);
/// Plazo de cada escritura en el WebSocket del visor: un cliente congelado que
/// no lee no puede bloquear la tarea para siempre (revisión adversarial, M5).
pub const PLAZO_ENVIO: Duration = Duration::from_secs(10);

/// Mensaje exacto del enlace gastado (checklist de la Fase 12).
pub const MENSAJE_ENLACE_USADO: &str = "enlace usado o caducado: ábrelo otra vez desde MAGI";

/// Cabeceras de seguridad de todas las respuestas (§5.1).
const CSP: &str = "default-src 'self'; script-src 'self'; worker-src 'self'; connect-src 'self'; img-src 'self' blob: data:; style-src 'self'; frame-ancestors 'none'; base-uri 'none'";

/// Página del visor (HTML estático, embebido en el binario).
const ENTRAR_HTML: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../visor/entrar.html"
));
/// Bundle del visor (generado por scripts/generar-visor.sh y versionado).
const VISOR_JS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../visor/dist/visor.js"
));
/// Worker del visor.
const TRABAJADOR_JS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../visor/dist/trabajador.js"
));
/// Hoja de estilos del visor.
const VISOR_CSS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../visor/dist/visor.css"
));

/// Límite del cuerpo de /api/canjear: solo lleva un enlace.
const CUERPO_CANJE_MAX: usize = 16 * 1024;

// ---------------------------------------------------------------- tiempos

/// Tiempos del servidor del visor. Los reales son las constantes; las pruebas
/// usan valores cortos para no esperar (doc(hidden), como el resto de la
/// superficie de prueba del proyecto).
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct Tiempos {
    pub vida_enlace: Duration,
    pub ventana_fallos: Duration,
    pub bloqueo_fallos: Duration,
    pub espera_hola: Duration,
    pub intervalo_ping: Duration,
    pub intervalo_marcos: Duration,
    /// Plazo de cada escritura en el WebSocket (revisión adversarial, M5).
    pub plazo_envio: Duration,
}

impl Default for Tiempos {
    fn default() -> Self {
        Self {
            vida_enlace: VIDA_ENLACE,
            ventana_fallos: VENTANA_FALLOS,
            bloqueo_fallos: BLOQUEO_FALLOS,
            espera_hola: ESPERA_HOLA,
            intervalo_ping: INTERVALO_PING,
            intervalo_marcos: Duration::from_millis(1000 / u64::from(puente::FPS_MAX)),
            plazo_envio: PLAZO_ENVIO,
        }
    }
}

// ---------------------------------------------------------------- voxl

/// Datos de la sesión voxl que el canje necesita para devolver la URL del
/// túnel: la página del propio voxl lleva su contraseña en el parámetro token
/// (R111) y deja de valer al cerrar la sesión.
#[derive(Debug, Clone)]
pub struct DatosVoxl {
    /// Puerto del túnel de la sesión en 127.0.0.1.
    pub puerto_tunel: u16,
    /// Contraseña de voxl de esta sesión (Zeroizing).
    pub clave: Zeroizing<String>,
}

impl DatosVoxl {
    /// URL con la que el navegador habla con voxl por el túnel de la sesión.
    pub fn url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/?token={}",
            self.puerto_tunel,
            self.clave.as_str()
        )
    }
}

// ---------------------------------------------------------------- avisos

/// Lo que el puente cuenta al servidor de escritorios.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvisoVisor {
    /// La fuente ha emitido el abierto: la sesión pasa a abierto y se emite
    /// el enlace.
    Abierto { id: u32 },
    /// La conexión se ha caído, con el motivo legible.
    Caido { id: u32, motivo: String },
    /// El remoto cerró limpiamente.
    Cerrado { id: u32, motivo: String },
    /// Hay un visor WebSocket conectado (h264: si el navegador decodifica
    /// H.264).
    VisorConectado { id: u32, h264: bool },
    /// El visor se ha ido (el sustituido por otro también avisa).
    VisorDesconectado { id: u32 },
    /// Sesión RDP con AVC420 y navegador sin H.264: hay que reconectar sin
    /// AVC420 (D211).
    SinH264 { id: u32 },
    /// El visor pide reconectar (orden rec).
    Reconectar { id: u32 },
    /// La pestaña del visor se ha ocultado (v,0) o ha vuelto (v,1).
    Visible { id: u32, visible: bool },
}

// ---------------------------------------------------------------- errores

/// Error al arrancar el servidor del visor.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorVisor {
    /// No se pudo escuchar en 127.0.0.1 (puerto ocupado, sin permisos…).
    #[error("{0}")]
    Puerto(String),
}

// ---------------------------------------------------------------- registro

struct Enlace {
    sesion: u32,
    caduca: Instant,
}

#[derive(Default)]
struct Fallos {
    instantes: VecDeque<Instant>,
    bloqueado_hasta: Option<Instant>,
}

struct Servidor {
    tarea: JoinHandle<()>,
}

/// Estado del ciclo de vida del listener. La generación se incrementa al parar
/// para que un arranque en curso no publique un puerto ya parado (M4).
struct Ciclo {
    generacion: u64,
}

/// Registro de sesiones del visor: enlaces, claves, fallos de canje y el
/// listener HTTP/WebSocket. Lo comparte el servidor de escritorios.
pub struct RegistroVisor {
    sesiones: RwLock<HashMap<u32, Arc<SesionVisor>>>,
    enlaces: Mutex<HashMap<String, Enlace>>,
    /// Sha-256 de la clave vigente de cada sesión, para no guardar la clave
    /// dos veces ni recorrer sesiones en cada WebSocket.
    claves: Mutex<HashMap<[u8; 32], u32>>,
    fallos: Mutex<Fallos>,
    servidor: Mutex<Option<Servidor>>,
    ciclo: Mutex<Ciclo>,
    arranque: tokio::sync::Mutex<()>,
    puerto_actual: AtomicU16,
    avisos: mpsc::Sender<AvisoVisor>,
    tiempos: Tiempos,
    siguiente_generacion: AtomicU64,
}

impl RegistroVisor {
    /// Crea el registro y el extremo de lectura de los avisos al servidor de
    /// escritorios (canal acotado: hay que drenarlo en el bucle del servidor).
    pub fn nuevo() -> (Arc<RegistroVisor>, mpsc::Receiver<AvisoVisor>) {
        Self::nuevo_con_tiempos(Tiempos::default())
    }

    /// Igual, con tiempos a medida para las pruebas.
    #[doc(hidden)]
    pub fn nuevo_con_tiempos(tiempos: Tiempos) -> (Arc<RegistroVisor>, mpsc::Receiver<AvisoVisor>) {
        let (avisos, receptor) = mpsc::channel(64);
        let registro = Arc::new(RegistroVisor {
            sesiones: RwLock::new(HashMap::new()),
            enlaces: Mutex::new(HashMap::new()),
            claves: Mutex::new(HashMap::new()),
            fallos: Mutex::new(Fallos::default()),
            servidor: Mutex::new(None),
            ciclo: Mutex::new(Ciclo { generacion: 0 }),
            arranque: tokio::sync::Mutex::new(()),
            puerto_actual: AtomicU16::new(0),
            avisos,
            tiempos,
            siguiente_generacion: AtomicU64::new(1),
        });
        (registro, receptor)
    }

    /// Arranca el listener en 127.0.0.1. Con el puerto 0 elige uno libre. Es
    /// idempotente: si ya está en marcha devuelve el puerto actual.
    pub async fn arrancar(self: &Arc<Self>, puerto: u16) -> Result<u16, ErrorVisor> {
        let _cerrojo = self.arranque.lock().await;
        if let Some(actual) = self.puerto() {
            return Ok(actual);
        }
        // Generación del ciclo de vida: si parar() entra mientras se hace el
        // bind, este arranque no publica nada (M4).
        let generacion = match self.ciclo.lock() {
            Ok(ciclo) => ciclo.generacion,
            Err(roto) => roto.into_inner().generacion,
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, puerto))
            .await
            .map_err(|error| {
                ErrorVisor::Puerto(format!(
                    "no se pudo abrir el servidor del visor en 127.0.0.1:{puerto}: {error}"
                ))
            })?;
        let real = listener
            .local_addr()
            .map_err(|error| ErrorVisor::Puerto(format!("sin dirección local: {error}")))?
            .port();
        {
            let mut ciclo = match self.ciclo.lock() {
                Ok(ciclo) => ciclo,
                Err(roto) => roto.into_inner(),
            };
            if ciclo.generacion != generacion {
                // parar() ganó la carrera: el listener se cierra al salir.
                return Err(ErrorVisor::Puerto(
                    "el servidor del visor se paró mientras arrancaba; reinténtalo".to_string(),
                ));
            }
            let aplicacion = self.router();
            let tarea = tokio::spawn(async move {
                if let Err(error) = axum::serve(listener, aplicacion).await {
                    tracing::warn!(%error, "el servidor del visor se ha parado");
                }
            });
            if let Ok(mut guardia) = self.servidor.lock() {
                *guardia = Some(Servidor { tarea });
            }
            // El puerto se publica solo cuando el servidor ya está instalado.
            self.puerto_actual.store(real, Ordering::Relaxed);
            ciclo.generacion = ciclo.generacion.wrapping_add(1);
        }
        tracing::info!(puerto = real, "servidor del visor en marcha en 127.0.0.1");
        Ok(real)
    }

    /// Puerto en el que escucha el servidor del visor, si está en marcha.
    pub fn puerto(&self) -> Option<u16> {
        match self.puerto_actual.load(Ordering::Relaxed) {
            0 => None,
            puerto => Some(puerto),
        }
    }

    /// Para el listener. Solo se llama cuando no quedan sesiones.
    ///
    /// Toma el mismo cerrojo de ciclo de vida que arrancar(): si un arranque
    /// está a medias, su generación cambia y no publica el puerto (M4).
    pub fn parar(&self) {
        let mut ciclo = match self.ciclo.lock() {
            Ok(ciclo) => ciclo,
            Err(roto) => roto.into_inner(),
        };
        ciclo.generacion = ciclo.generacion.wrapping_add(1);
        if let Ok(mut guardia) = self.servidor.lock() {
            if let Some(servidor) = guardia.take() {
                servidor.tarea.abort();
            }
        }
        self.puerto_actual.store(0, Ordering::Relaxed);
    }

    /// Para el listener si no queda ninguna sesión de escritorio (el servidor
    /// lo llama en cada baja).
    pub fn parar_si_vacio(&self) {
        let vacio = self
            .sesiones
            .read()
            .map(|sesiones| sesiones.is_empty())
            .unwrap_or(false);
        if vacio {
            self.parar();
        }
    }

    /// Da de alta una sesión RDP o VNC con su par de canales con la fuente.
    /// Debe llamarse dentro del runtime tokio (lanza el bucle del puente).
    pub fn alta(
        &self,
        info: InfoEscritorio,
        portapapeles: bool,
        eventos: mpsc::Receiver<EventoEscritorio>,
        ordenes: mpsc::Sender<OrdenEscritorio>,
    ) -> Arc<SesionVisor> {
        let (comandos, comandos_rx) = mpsc::channel(64);
        let (estados, estados_rx) = mpsc::unbounded_channel();
        let imagen = info.imagen.unwrap_or(ImagenEscritorio::Rectangulos);
        let compartido = Arc::new(Compartido::nuevo(info.ancho, info.alto, imagen));
        let sesion = Arc::new(SesionVisor {
            id: info.id,
            tipo: info.tipo,
            redimensionable: info.redimensionable,
            base: RwLock::new(info),
            compartido: compartido.clone(),
            voxl: RwLock::new(None),
            comandos,
            estados,
            avisos: self.avisos.clone(),
            hash_clave: Mutex::new(None),
            tarea: Mutex::new(None),
        });
        let config = puente::ConfigPuente {
            id: sesion.id,
            tipo: sesion.tipo,
            redimensionable: sesion.redimensionable,
            portapapeles,
            tiempos: self.tiempos.clone(),
            compartido,
            avisos: self.avisos.clone(),
            comandos: comandos_rx,
            estados: estados_rx,
            eventos,
            ordenes,
        };
        let tarea = puente::lanzar(config);
        if let Ok(mut guardia) = sesion.tarea.lock() {
            *guardia = Some(tarea);
        }
        if let Ok(mut sesiones) = self.sesiones.write() {
            sesiones.insert(sesion.id, sesion.clone());
        }
        sesion
    }

    /// Da de alta una sesión voxl: no tiene puente ni WebSocket propio (el
    /// navegador va a la página de voxl por el túnel de la sesión).
    pub fn alta_voxl(&self, info: InfoEscritorio, datos: DatosVoxl) -> Arc<SesionVisor> {
        let (comandos, _comandos_rx) = mpsc::channel(64);
        let (estados, _estados_rx) = mpsc::unbounded_channel();
        let imagen = info.imagen.unwrap_or(ImagenEscritorio::Voxl);
        let compartido = Arc::new(Compartido::nuevo(info.ancho, info.alto, imagen));
        let sesion = Arc::new(SesionVisor {
            id: info.id,
            tipo: info.tipo,
            redimensionable: info.redimensionable,
            base: RwLock::new(info),
            compartido,
            voxl: RwLock::new(Some(datos)),
            comandos,
            estados,
            avisos: self.avisos.clone(),
            hash_clave: Mutex::new(None),
            tarea: Mutex::new(None),
        });
        if let Ok(mut sesiones) = self.sesiones.write() {
            sesiones.insert(sesion.id, sesion.clone());
        }
        sesion
    }

    /// Da de baja una sesión: anula sus enlaces pendientes y su clave, avisa
    /// al visor, aborta el puente y para el listener si ya no queda ninguna.
    pub fn baja(&self, id: u32) {
        let sesion = match self.sesiones.write() {
            Ok(mut sesiones) => sesiones.remove(&id),
            Err(roto) => roto.into_inner().remove(&id),
        };
        if let Ok(mut enlaces) = self.enlaces.lock() {
            enlaces.retain(|_, enlace| enlace.sesion != id);
        }
        if let Some(sesion) = sesion {
            self.olvidar_clave(&sesion);
            sesion.cerrar("cerrado", None);
            sesion.soltar_tarea();
        }
        self.parar_si_vacio();
    }

    /// Sustituye la InfoEscritorio que difunde el servidor (estado, fase,
    /// motivo, tamaño inicial…). Conserva visor/fps/tamaño/imagen del puente.
    pub fn actualizar_info(&self, info: InfoEscritorio) -> bool {
        let Some(sesion) = self.sesion(info.id) else {
            return false;
        };
        sesion.actualizar_info(info);
        true
    }

    /// Sesión por id.
    pub fn sesion(&self, id: u32) -> Option<Arc<SesionVisor>> {
        self.sesiones
            .read()
            .ok()
            .and_then(|sesiones| sesiones.get(&id).cloned())
    }

    /// Todas las sesiones, como lista para difundir la lista Escritorios.
    pub fn lista(&self) -> Vec<InfoEscritorio> {
        let mut lista: Vec<InfoEscritorio> = self
            .sesiones
            .read()
            .map(|sesiones| sesiones.values().map(|sesion| sesion.info()).collect())
            .unwrap_or_default();
        lista.sort_by_key(|info| info.id);
        lista
    }

    /// ¿Hay un visor WebSocket conectado a la sesión? (para la gracia; en
    /// voxl lo cuenta el servidor por las conexiones del túnel).
    pub fn visor_conectado(&self, id: u32) -> bool {
        self.sesion(id)
            .map(|sesion| sesion.visor_conectado())
            .unwrap_or(false)
    }

    /// Emite un enlace de un solo uso para la sesión y devuelve la URL. None
    /// si la sesión no existe o el servidor no está en marcha.
    pub fn enlace(&self, id: u32) -> Option<String> {
        let puerto = self.puerto()?;
        self.sesion(id)?;
        let token = token_aleatorio()?;
        let ahora = Instant::now();
        let Ok(mut enlaces) = self.enlaces.lock() else {
            return None;
        };
        enlaces.retain(|_, enlace| enlace.caduca > ahora);
        enlaces.insert(
            token.clone(),
            Enlace {
                sesion: id,
                caduca: ahora + self.tiempos.vida_enlace,
            },
        );
        Some(format!("http://127.0.0.1:{puerto}/entrar?t={token}"))
    }

    // ------------------------------------------------------------ claves

    fn sesion_por_clave(&self, clave: &str) -> Option<Arc<SesionVisor>> {
        let hash = hash_clave(clave);
        let id = {
            let claves = self.claves.lock().ok()?;
            *claves.get(&hash)?
        };
        self.sesion(id)
    }

    /// Clave nueva para la sesión: invalida la anterior (§7.3).
    fn nueva_clave(&self, sesion: &SesionVisor) -> Option<String> {
        let clave = token_aleatorio()?;
        let hash = hash_clave(&clave);
        {
            let Ok(mut claves) = self.claves.lock() else {
                return None;
            };
            if let Some(anterior) = sesion.tomar_hash_clave() {
                claves.remove(&anterior);
            }
            claves.insert(hash, sesion.id);
        }
        sesion.poner_clave(hash, Zeroizing::new(clave.clone()));
        Some(clave)
    }

    fn olvidar_clave(&self, sesion: &SesionVisor) {
        if let Some(hash) = sesion.tomar_hash_clave() {
            if let Ok(mut claves) = self.claves.lock() {
                claves.remove(&hash);
            }
        }
    }

    // ------------------------------------------------------------ canje

    fn bloqueado(&self, ahora: Instant) -> bool {
        let Ok(mut fallos) = self.fallos.lock() else {
            return false;
        };
        if let Some(hasta) = fallos.bloqueado_hasta {
            if ahora < hasta {
                return true;
            }
            fallos.bloqueado_hasta = None;
            fallos.instantes.clear();
        }
        while let Some(primero) = fallos.instantes.front() {
            if ahora.duration_since(*primero) > self.tiempos.ventana_fallos {
                fallos.instantes.pop_front();
            } else {
                break;
            }
        }
        false
    }

    fn anotar_fallo(&self, ahora: Instant) {
        let Ok(mut fallos) = self.fallos.lock() else {
            return;
        };
        fallos.instantes.push_back(ahora);
        while fallos.instantes.len() > MAX_FALLOS_CANJE {
            fallos.instantes.pop_front();
        }
        if fallos.instantes.len() >= MAX_FALLOS_CANJE {
            fallos.bloqueado_hasta = Some(ahora + self.tiempos.bloqueo_fallos);
        }
    }

    // ------------------------------------------------------------ router

    fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route("/", get(pagina_visor))
            .route("/entrar", get(pagina_visor))
            .route("/visor.js", get(visor_js))
            .route("/trabajador.js", get(trabajador_js))
            .route("/visor.css", get(visor_css))
            .route("/api/salud", get(salud))
            .route(
                "/api/canjear",
                post(canjear).layer(DefaultBodyLimit::max(CUERPO_CANJE_MAX)),
            )
            .route("/api/ws", get(ws_visor))
            .layer(middleware::from_fn_with_state(self.clone(), seguridad))
            .with_state(self.clone())
    }
}

// ---------------------------------------------------------------- sesión

/// Una sesión de escritorio vista por el servidor del visor: su estado, su
/// clave, sus enlaces y el canal con el puente.
pub struct SesionVisor {
    id: u32,
    tipo: TipoHost,
    redimensionable: bool,
    base: RwLock<InfoEscritorio>,
    compartido: Arc<Compartido>,
    voxl: RwLock<Option<DatosVoxl>>,
    comandos: mpsc::Sender<ComandoPuente>,
    /// Avisos de ciclo de vida sin pérdida (m9): no comparten el canal de
    /// comandos, que puede ir lleno de teclas y ratón.
    estados: mpsc::UnboundedSender<ComandoEstado>,
    avisos: mpsc::Sender<AvisoVisor>,
    hash_clave: Mutex<Option<[u8; 32]>>,
    tarea: Mutex<Option<JoinHandle<()>>>,
}

impl SesionVisor {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn tipo(&self) -> TipoHost {
        self.tipo
    }

    pub fn es_voxl(&self) -> bool {
        self.tipo == TipoHost::Voxl
    }

    /// Foto de la sesión para la lista Escritorios: lo que fija el servidor
    /// más lo que lleva el puente (visor, fps y tamaño).
    pub fn info(&self) -> InfoEscritorio {
        let mut info = self
            .base
            .read()
            .map(|info| info.clone())
            .unwrap_or_else(|roto| roto.into_inner().clone());
        info.visor = self.compartido.conectado();
        info.fps = self.compartido.fps();
        info.ancho = self.compartido.ancho();
        info.alto = self.compartido.alto();
        info.imagen = Some(self.compartido.imagen());
        info
    }

    pub fn visor_conectado(&self) -> bool {
        self.compartido.conectado()
    }

    pub fn fps(&self) -> u16 {
        self.compartido.fps()
    }

    pub fn tamano(&self) -> (u16, u16) {
        (self.compartido.ancho(), self.compartido.alto())
    }

    pub fn ancho(&self) -> u16 {
        self.compartido.ancho()
    }

    pub fn alto(&self) -> u16 {
        self.compartido.alto()
    }

    /// Datos de voxl de la sesión (puerto del túnel y contraseña).
    pub fn datos_voxl(&self) -> Option<DatosVoxl> {
        self.voxl
            .read()
            .map(|datos| datos.clone())
            .unwrap_or_else(|roto| roto.into_inner().clone())
    }

    /// Actualiza los datos del túnel de voxl (si cambia al reconectar).
    pub fn actualizar_voxl(&self, datos: DatosVoxl) {
        if let Ok(mut voxl) = self.voxl.write() {
            *voxl = Some(datos);
        }
    }

    /// Manda al visor el estado de la sesión (conectando, caido, cerrado…).
    /// No le cierra el socket: en caído el visor enseña Reconectar.
    ///
    /// Va por el canal de estados sin pérdida (m9): el canal de comandos puede
    /// ir lleno de teclas y ratón y un try_send perdería el aviso.
    pub fn estado_visor(&self, estado: &str, motivo: Option<&str>) {
        let _ = self.estados.send(ComandoEstado::Echar {
            estado: estado.to_string(),
            motivo: motivo.map(str::to_string),
            cerrar: false,
        });
    }

    /// Cambia la fuente (reconexión) sin recrear el visor ni su clave: el
    /// WebSocket sigue abierto y el puente cambia el par de canales.
    pub async fn renovar_fuente(
        &self,
        eventos: mpsc::Receiver<EventoEscritorio>,
        ordenes: mpsc::Sender<OrdenEscritorio>,
    ) -> bool {
        self.comandos
            .send(ComandoPuente::Fuente { eventos, ordenes })
            .await
            .is_ok()
    }

    // ------------------------------------------------------------ interno

    fn actualizar_info(&self, info: InfoEscritorio) {
        if let Ok(mut base) = self.base.write() {
            *base = info.clone();
        }
        // En voxl no hay puente que lleve el tamaño y la imagen.
        if self.es_voxl() {
            self.compartido.fijar_tamano(info.ancho, info.alto);
            if let Some(imagen) = info.imagen {
                self.compartido.fijar_imagen(imagen);
            }
        }
    }

    fn cerrar(&self, estado: &str, motivo: Option<&str>) {
        let _ = self.estados.send(ComandoEstado::Cerrar {
            estado: estado.to_string(),
            motivo: motivo.map(str::to_string),
        });
    }

    fn soltar_tarea(&self) {
        let Ok(mut guardia) = self.tarea.lock() else {
            return;
        };
        let Some(tarea) = guardia.take() else {
            return;
        };
        // Se le deja un instante para mandar el estado y el close al visor.
        if let Ok(manejador) = tokio::runtime::Handle::try_current() {
            manejador.spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                tarea.abort();
            });
        } else {
            tarea.abort();
        }
    }

    fn poner_clave(&self, hash: [u8; 32], clave: Zeroizing<String>) {
        // La clave no se registra nunca en el log (T44).
        drop(clave);
        if let Ok(mut guardia) = self.hash_clave.lock() {
            *guardia = Some(hash);
        }
    }

    fn tomar_hash_clave(&self) -> Option<[u8; 32]> {
        self.hash_clave
            .lock()
            .ok()
            .and_then(|mut guardia| guardia.take())
    }

    /// Echa al visor anterior al canjear un enlace nuevo (§7.3).
    fn echar_visor_anterior(&self) {
        let _ = self.estados.send(ComandoEstado::Echar {
            estado: "otra_ventana".to_string(),
            motivo: None,
            cerrar: true,
        });
    }

    fn conectar_visor(&self, generacion: u64, salida: SalidaVisor, h264: bool) -> bool {
        self.comandos
            .try_send(ComandoPuente::Conectar {
                generacion,
                salida,
                h264,
            })
            .is_ok()
    }

    fn desconectar_visor(&self, generacion: u64) {
        let _ = self.estados.send(ComandoEstado::Desconectar { generacion });
    }

    async fn avisar(&self, aviso: AvisoVisor) {
        let _ = self.avisos.send(aviso).await;
    }
}

// ---------------------------------------------------------------- seguridad

/// Middleware común: Host en todas las rutas (anti DNS rebinding), Origin
/// obligatorio en /api/ y las cabeceras de seguridad en toda respuesta.
async fn seguridad(
    State(registro): State<Arc<RegistroVisor>>,
    peticion: Request,
    siguiente: Next,
) -> Response {
    let Some(puerto) = registro.puerto() else {
        let mut respuesta = (
            StatusCode::SERVICE_UNAVAILABLE,
            "el servidor del visor no está en marcha",
        )
            .into_response();
        cabeceras_seguridad(&mut respuesta);
        return respuesta;
    };
    let host_ok = host_valido(peticion.headers(), puerto);
    let es_api = peticion.uri().path().starts_with("/api/");
    let origen_ok = !es_api || origen_valido(peticion.headers(), puerto);
    let mut respuesta = if host_ok && origen_ok {
        siguiente.run(peticion).await
    } else {
        (StatusCode::FORBIDDEN, "prohibido").into_response()
    };
    cabeceras_seguridad(&mut respuesta);
    respuesta
}

fn host_valido(cabeceras: &HeaderMap, puerto: u16) -> bool {
    let Some(host) = cabeceras
        .get(header::HOST)
        .and_then(|valor| valor.to_str().ok())
    else {
        return false;
    };
    host == format!("127.0.0.1:{puerto}") || host == format!("localhost:{puerto}")
}

fn origen_valido(cabeceras: &HeaderMap, puerto: u16) -> bool {
    let Some(origen) = cabeceras
        .get(header::ORIGIN)
        .and_then(|valor| valor.to_str().ok())
    else {
        return false;
    };
    origen == format!("http://127.0.0.1:{puerto}") || origen == format!("http://localhost:{puerto}")
}

fn cabeceras_seguridad(respuesta: &mut Response) {
    let cabeceras = respuesta.headers_mut();
    cabeceras.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    cabeceras.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    cabeceras.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    cabeceras.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

// ---------------------------------------------------------------- handlers

async fn pagina_visor() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        ENTRAR_HTML,
    )
        .into_response()
}

async fn visor_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        VISOR_JS,
    )
        .into_response()
}

async fn trabajador_js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        TRABAJADOR_JS,
    )
        .into_response()
}

async fn visor_css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        VISOR_CSS,
    )
        .into_response()
}

async fn salud() -> Response {
    (StatusCode::OK, "ok").into_response()
}

#[derive(Deserialize)]
struct PeticionCanje {
    enlace: String,
}

/// Normaliza lo que llega en el canje: la página del visor manda el token de
/// la URL, pero también se admite la URL de /entrar entera.
fn normalizar_enlace(enlace: &str) -> &str {
    match enlace.split_once("?t=") {
        Some((_, token)) => token,
        None => enlace,
    }
}

/// POST /api/canjear: gasta el enlace SIEMPRE que se intenta canjear y da la
/// clave por sesión (o la URL de voxl). 403 si no vale y 429 tras diez fallos.
async fn canjear(State(registro): State<Arc<RegistroVisor>>, cuerpo: Bytes) -> Response {
    let peticion: PeticionCanje = match serde_json::from_slice(&cuerpo) {
        Ok(peticion) => peticion,
        Err(_) => {
            return respuesta_json(
                StatusCode::BAD_REQUEST,
                json!({"error": "petición inválida"}),
            )
        }
    };
    let token = normalizar_enlace(&peticion.enlace);
    if token.is_empty() || token.len() > 256 {
        registro.anotar_fallo(Instant::now());
        return respuesta_enlace_usado();
    }
    let ahora = Instant::now();
    if registro.bloqueado(ahora) {
        return respuesta_json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error": "demasiados intentos: espera un minuto"}),
        );
    }
    // Se gasta aunque el resto falle (§7.3).
    let enlace = match registro.enlaces.lock() {
        Ok(mut enlaces) => enlaces.remove(token),
        Err(roto) => roto.into_inner().remove(token),
    };
    let Some(enlace) = enlace else {
        registro.anotar_fallo(ahora);
        return respuesta_enlace_usado();
    };
    if enlace.caduca <= ahora {
        registro.anotar_fallo(ahora);
        return respuesta_enlace_usado();
    }
    let Some(sesion) = registro.sesion(enlace.sesion) else {
        registro.anotar_fallo(ahora);
        return respuesta_enlace_usado();
    };
    if sesion.es_voxl() {
        let Some(datos) = sesion.datos_voxl() else {
            registro.anotar_fallo(ahora);
            return respuesta_enlace_usado();
        };
        return respuesta_json(StatusCode::OK, json!({"voxl": datos.url()}));
    }
    let Some(clave) = registro.nueva_clave(&sesion) else {
        return respuesta_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error": "no se pudo generar la clave del visor"}),
        );
    };
    sesion.echar_visor_anterior();
    // La página del visor pinta el campo escritorio en la barra como nombre.
    respuesta_json(
        StatusCode::OK,
        json!({
            "clave": clave,
            "escritorio": sesion.info().nombre,
            "tipo": sesion.tipo().como_texto(),
        }),
    )
}

fn respuesta_enlace_usado() -> Response {
    respuesta_json(
        StatusCode::FORBIDDEN,
        json!({"error": MENSAJE_ENLACE_USADO}),
    )
}

fn respuesta_json(estado: StatusCode, valor: serde_json::Value) -> Response {
    (
        estado,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        valor.to_string(),
    )
        .into_response()
}

async fn ws_visor(ws: WebSocketUpgrade, State(registro): State<Arc<RegistroVisor>>) -> Response {
    ws.on_upgrade(move |socket| atender_visor(socket, registro))
}

struct Hola {
    clave: String,
    h264: bool,
}

/// Espera el primer mensaje de hola con la clave en el plazo. Si no llega o no
/// es válido, None (el socket se cierra).
async fn esperar_hola(socket: &mut WebSocket, plazo: Duration) -> Option<Hola> {
    let limite = tokio::time::Instant::now() + plazo;
    loop {
        match tokio::time::timeout_at(limite, socket.recv()).await {
            Err(_) => return None,
            Ok(None) => return None,
            Ok(Some(Err(_))) => return None,
            Ok(Some(Ok(Message::Text(texto)))) => return interpretar_hola(texto.as_str()),
            Ok(Some(Ok(Message::Ping(datos)))) => {
                let _ = enviar_ws(socket, Message::Pong(datos), plazo).await;
            }
            Ok(Some(Ok(Message::Pong(_)))) => {}
            Ok(Some(Ok(Message::Close(_)))) => return None,
            Ok(Some(Ok(Message::Binary(_)))) => return None,
        }
    }
}

fn interpretar_hola(texto: &str) -> Option<Hola> {
    #[derive(Deserialize)]
    struct MensajeHola {
        t: String,
        clave: String,
        #[serde(default)]
        h264: bool,
    }
    let mensaje: MensajeHola = serde_json::from_str(texto).ok()?;
    if mensaje.t != "hola" {
        return None;
    }
    if mensaje.clave.is_empty()
        || mensaje.clave.len() > 128
        || !mensaje.clave.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return None;
    }
    Some(Hola {
        clave: mensaje.clave,
        h264: mensaje.h264,
    })
}

enum Entrada {
    Mensaje(Option<Result<Message, axum::Error>>),
    Salida,
}

/// Escribe un mensaje en el WebSocket con plazo. Falso si el visor no lee a
/// tiempo o el socket ya no está: un navegador congelado no puede bloquear la
/// tarea para siempre (revisión adversarial, M5).
async fn enviar_ws(socket: &mut WebSocket, mensaje: Message, plazo: Duration) -> bool {
    match tokio::time::timeout(plazo, socket.send(mensaje)).await {
        Ok(Ok(())) => true,
        _ => {
            tracing::debug!("el visor no lee a tiempo: se cierra el WebSocket");
            false
        }
    }
}

/// Atiende un visor: valida el hola, busca la sesión por su clave, registra la
/// conexión y reenvía los mensajes en los dos sentidos hasta que se cierra.
async fn atender_visor(mut socket: WebSocket, registro: Arc<RegistroVisor>) {
    let plazo_envio = registro.tiempos.plazo_envio;
    let Some(hola) = esperar_hola(&mut socket, registro.tiempos.espera_hola).await else {
        let _ = enviar_ws(&mut socket, Message::Close(None), plazo_envio).await;
        return;
    };
    let Some(sesion) = registro.sesion_por_clave(&hola.clave) else {
        let _ = enviar_ws(&mut socket, Message::Close(None), plazo_envio).await;
        return;
    };
    let generacion = registro
        .siguiente_generacion
        .fetch_add(1, Ordering::Relaxed);
    let (salida, mut salida_rx) = SalidaVisor::nueva();
    if !sesion.conectar_visor(generacion, salida.clone(), hola.h264) {
        let _ = enviar_ws(&mut socket, Message::Close(None), plazo_envio).await;
        return;
    }
    tracing::debug!(escritorio = sesion.id(), "visor conectado");
    let mut cerrado = false;
    while !cerrado {
        let entrada = tokio::select! {
            mensaje = socket.recv() => Entrada::Mensaje(mensaje),
            _ = salida.aviso().notified() => Entrada::Salida,
        };
        match entrada {
            Entrada::Mensaje(None) | Entrada::Mensaje(Some(Err(_))) => break,
            Entrada::Mensaje(Some(Ok(Message::Text(texto)))) => {
                procesar_orden(&sesion, texto.as_str()).await;
            }
            Entrada::Mensaje(Some(Ok(Message::Binary(_)))) => {
                // El visor no manda binarios: se descarta.
            }
            Entrada::Mensaje(Some(Ok(Message::Ping(datos)))) => {
                if !enviar_ws(&mut socket, Message::Pong(datos), plazo_envio).await {
                    break;
                }
            }
            Entrada::Mensaje(Some(Ok(Message::Pong(_)))) => {}
            Entrada::Mensaje(Some(Ok(Message::Close(_)))) => break,
            Entrada::Salida => {}
        }
        // Vaciar la cola del puente (fuera del select: aquí ya no hay préstamo
        // sobre el socket). Todo lo que sale de la cola se descuenta del
        // contador de bytes pendientes, también los textos (C2): si no, los
        // ping y los clip acaban congelando el puente a los 8 MB.
        loop {
            match salida_rx.try_recv() {
                Ok(MensajeVisor::Binario(datos)) => {
                    let bytes = datos.len();
                    let enviado =
                        enviar_ws(&mut socket, Message::Binary(datos.into()), plazo_envio).await;
                    salida.descontar(bytes);
                    if !enviado {
                        cerrado = true;
                        break;
                    }
                }
                Ok(MensajeVisor::Texto(texto)) => {
                    let bytes = texto.len();
                    let enviado =
                        enviar_ws(&mut socket, Message::Text(texto.into()), plazo_envio).await;
                    salida.descontar(bytes);
                    if !enviado {
                        cerrado = true;
                        break;
                    }
                }
                Ok(MensajeVisor::Cerrar) => {
                    let _ = enviar_ws(&mut socket, Message::Close(None), plazo_envio).await;
                    cerrado = true;
                    break;
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    cerrado = true;
                    break;
                }
            }
        }
        if salida.cerrada() {
            let _ = enviar_ws(&mut socket, Message::Close(None), plazo_envio).await;
            break;
        }
    }
    sesion.desconectar_visor(generacion);
}

async fn procesar_orden(sesion: &SesionVisor, texto: &str) {
    let Some(instruccion) = puente::interpretar_orden(texto, sesion.ancho(), sesion.alto()) else {
        return;
    };
    match instruccion {
        InstruccionVisor::Orden(orden) => {
            let _ = sesion.comandos.send(ComandoPuente::Orden(orden)).await;
        }
        InstruccionVisor::Visible(visible) => {
            let _ = sesion.comandos.send(ComandoPuente::Visible(visible)).await;
        }
        InstruccionVisor::Repintar => {
            let _ = sesion.comandos.send(ComandoPuente::Repintar).await;
        }
        InstruccionVisor::Latencia(s) => {
            let _ = sesion.comandos.send(ComandoPuente::Latencia { s }).await;
        }
        InstruccionVisor::Reconectar => {
            sesion
                .avisar(AvisoVisor::Reconectar { id: sesion.id() })
                .await;
        }
    }
}

// ---------------------------------------------------------------- utilidades

/// 256 bits del generador del sistema en base64 URL-safe sin relleno.
fn token_aleatorio() -> Option<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn hash_clave(clave: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digesto = Sha256::digest(clave.as_bytes());
    let mut salida = [0u8; 32];
    salida.copy_from_slice(digesto.as_slice());
    salida
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_url_de_voxl_lleva_el_token_del_tunel() {
        let datos = DatosVoxl {
            puerto_tunel: 41234,
            clave: Zeroizing::new("abc123".to_string()),
        };
        assert_eq!(datos.url(), "http://127.0.0.1:41234/?token=abc123");
    }

    #[test]
    fn el_token_es_aleatorio_y_largo() {
        let primero = token_aleatorio().unwrap();
        let segundo = token_aleatorio().unwrap();
        assert_eq!(primero.len(), 43);
        assert_ne!(primero, segundo);
    }

    #[test]
    fn el_hash_de_la_clave_es_estable() {
        assert_eq!(hash_clave("hola"), hash_clave("hola"));
        assert_ne!(hash_clave("hola"), hash_clave("adios"));
    }

    #[test]
    fn el_hola_del_visor_se_valida() {
        assert_eq!(
            interpretar_hola(r#"{"t":"hola","clave":"abc","h264":true}"#).map(|hola| hola.clave),
            Some("abc".to_string())
        );
        assert!(interpretar_hola(r#"{"t":"hola","clave":"abc"}"#).is_some());
        assert!(interpretar_hola(r#"{"t":"adios","clave":"abc"}"#).is_none());
        assert!(interpretar_hola(r#"{"t":"hola","clave":""}"#).is_none());
        assert!(interpretar_hola("no es json").is_none());
    }

    #[test]
    fn el_host_y_el_origen_se_comprueban_estricto() {
        let mut cabeceras = HeaderMap::new();
        cabeceras.insert(header::HOST, HeaderValue::from_static("127.0.0.1:4242"));
        assert!(host_valido(&cabeceras, 4242));
        assert!(!host_valido(&cabeceras, 4243));
        cabeceras.insert(header::HOST, HeaderValue::from_static("localhost:4242"));
        assert!(host_valido(&cabeceras, 4242));
        cabeceras.insert(header::HOST, HeaderValue::from_static("evil.example:4242"));
        assert!(!host_valido(&cabeceras, 4242));

        let mut cabeceras = HeaderMap::new();
        assert!(!origen_valido(&cabeceras, 4242));
        cabeceras.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:4242"),
        );
        assert!(origen_valido(&cabeceras, 4242));
        cabeceras.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example"),
        );
        assert!(!origen_valido(&cabeceras, 4242));
    }
}
