//! Estado compartido del núcleo móvil: almacén, runtime de tokio, llavero,
//! firmante, sesiones, conexiones por host, túneles, sincronización y diálogos.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use magi_nucleo::almacen::Almacen;
use magi_nucleo::conexion::cliente::Cliente;
use magi_nucleo::conexion::reenvios::{Contadores, Reenvios};
use magi_nucleo::modelo::TipoTunel;
use magi_nucleo::rutas::{Equipo as EquipoNucleo, Rutas as RutasNucleo};
use magi_nucleo::trabajo_vivo::TrabajoVivo;
use magi_nucleo::tuneles::escucha::EscuchaMovil;
use russh::client::Handle;
use tokio::sync::{mpsc, oneshot, watch};

use crate::api::{
    ClavePublicaMovil, ErrorNucleo, EstadoSesion, PeticionDialogo, Retrollamadas, Sesion,
};
use crate::clave::{FirmanteNucleo, LlaveroNucleo};
use crate::dialogos::Dialogos;
use crate::escritor::Escritor;

/// Una conexión viva de un host, compartida por sus sesiones y túneles.
pub struct ConexionHost {
    pub handle: Arc<Handle<Cliente>>,
    pub saltos: Vec<Handle<Cliente>>,
    pub reenvios: Arc<Reenvios>,
    /// Canal de eventos de la conexión: mantenerlo vivo conserva el bucle que
    /// traduce huellas y contraseñas a diálogos de la app.
    pub tx: mpsc::UnboundedSender<magi_nucleo::conexion::EventoConexion>,
}

/// Órdenes de la app hacia una sesión viva.
pub enum ComandoSesion {
    Datos(Vec<u8>),
    Redimensionar(u16, u16),
    Desplazar(i32),
    Cerrar,
}

/// Sesión viva del móvil. El parser y los tramos viven en la tarea de la
/// sesión; aquí queda lo que la app necesita consultar en cualquier momento.
pub struct SesionViva {
    pub id: u64,
    pub host_uuid: String,
    pub host_nombre: String,
    pub estado: Mutex<EstadoSesion>,
    pub detalle: Mutex<Option<String>>,
    pub comandos: Mutex<mpsc::UnboundedSender<ComandoSesion>>,
    pub pantalla: Arc<Mutex<vt100::Parser>>,
    pub abierta_en: Option<i64>,
}

impl SesionViva {
    pub fn estado(&self) -> EstadoSesion {
        self.estado
            .lock()
            .map(|estado| *estado)
            .unwrap_or(EstadoSesion::Caida)
    }

    pub fn fijar_estado(&self, nuevo: EstadoSesion, detalle: Option<String>) {
        if let Ok(mut estado) = self.estado.lock() {
            *estado = nuevo;
        }
        if let Ok(mut actual) = self.detalle.lock() {
            *actual = detalle;
        }
    }

    /// Modo 2004 del terminal (pegado entre corchetes) según el parser.
    pub fn pegado_entre_corchetes(&self) -> bool {
        self.pantalla
            .lock()
            .map(|parser| parser.screen().bracketed_paste())
            .unwrap_or(false)
    }

    /// Envía una orden a la tarea de la sesión.
    pub fn ordenar(&self, comando: ComandoSesion) {
        if let Ok(comandos) = self.comandos.lock() {
            let _ = comandos.send(comando);
        }
    }

    /// Texto de una selección rectangular de celdas (base 0).
    pub fn texto_seleccion(&self, desde: (u16, u16), hasta: (u16, u16)) -> Option<String> {
        let parser = self.pantalla.lock().ok()?;
        let pantalla = parser.screen();
        let (filas, columnas) = pantalla.size();
        let (mut fila_a, mut col_a) = (desde.1, desde.0);
        let (mut fila_b, mut col_b) = (hasta.1, hasta.0);
        if (fila_a, col_a) > (fila_b, col_b) {
            std::mem::swap(&mut fila_a, &mut fila_b);
            std::mem::swap(&mut col_a, &mut col_b);
        }
        let mut lineas = Vec::new();
        for fila in fila_a..=fila_b.min(filas.saturating_sub(1)) {
            let inicio = if fila == fila_a { col_a } else { 0 };
            let fin = if fila == fila_b {
                col_b
            } else {
                columnas.saturating_sub(1)
            };
            let mut linea = String::new();
            for columna in inicio..=fin.min(columnas.saturating_sub(1)) {
                if let Some(celda) = pantalla.cell(fila, columna) {
                    linea.push_str(celda.contents());
                }
            }
            lineas.push(linea.trim_end().to_string());
        }
        Some(lineas.join("\n"))
    }
}

/// Estado vivo de un túnel en el móvil (diagrama de §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoTunelVivo {
    Inactivo,
    Activando,
    Activo,
    Caido(String),
}

/// Un túnel que la app ha pedido activar; sigue en el mapa también cuando
/// está caído para enseñar su error y poder relanzarlo.
pub struct TunelActivo {
    pub uuid: String,
    pub host_uuid: String,
    pub host_nombre: String,
    pub nombre: String,
    pub tipo: TipoTunel,
    pub escucha: EscuchaMovil,
    pub escucha_efectiva: Mutex<String>,
    pub destino: Option<String>,
    pub automatico: bool,
    pub estado: Mutex<EstadoTunelVivo>,
    pub contadores: Arc<Contadores>,
    pub parada: Mutex<Option<oneshot::Sender<()>>>,
    pub cancelacion: watch::Sender<bool>,
    pub reenvio: Mutex<Option<(String, u32)>>,
    pub puerto_efectivo: Mutex<Option<u16>>,
    /// Identificador numérico para el registro de reenvíos del host.
    pub id_numerico: i64,
}

impl TunelActivo {
    pub fn estado(&self) -> EstadoTunelVivo {
        self.estado
            .lock()
            .map(|estado| estado.clone())
            .unwrap_or(EstadoTunelVivo::Caido("estado no disponible".to_string()))
    }

    pub fn fijar_estado(&self, nuevo: EstadoTunelVivo) {
        if let Ok(mut estado) = self.estado.lock() {
            *estado = nuevo;
        }
    }
}

/// Fase de la sincronización en el móvil (diagrama de §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaseSincro {
    SinConfigurar,
    SinClave,
    Activa,
    Sincronizando,
    EsperandoClave,
    ErrorCarpeta,
}

/// Estado en memoria de la sincronización del móvil.
pub struct EstadoSincro {
    pub carpeta_uri: Option<String>,
    pub carpeta_nombre: Option<String>,
    pub fase: FaseSincro,
    pub huella_clave: Option<String>,
    pub ultima_pasada: Option<i64>,
    pub avisos: Vec<String>,
    pub error: Option<String>,
}

impl Default for EstadoSincro {
    fn default() -> Self {
        Self {
            carpeta_uri: None,
            carpeta_nombre: None,
            fase: FaseSincro::SinConfigurar,
            huella_clave: None,
            ultima_pasada: None,
            avisos: Vec::new(),
            error: None,
        }
    }
}

/// Todo el estado del núcleo móvil, compartido por las llamadas de la app.
pub struct EstadoNucleo {
    pub retrollamadas: Retrollamadas,
    pub rutas: RutasNucleo,
    pub equipo: EquipoNucleo,
    pub app_version: String,
    pub runtime: tokio::runtime::Runtime,
    pub escritor: Escritor,
    pub llavero: Arc<LlaveroNucleo>,
    pub firmante: Arc<FirmanteNucleo>,
    pub dialogos: Dialogos,
    pub sesiones: Mutex<HashMap<u64, Arc<SesionViva>>>,
    pub conexiones: tokio::sync::Mutex<HashMap<String, Arc<ConexionHost>>>,
    pub cerrojos: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub tuneles: Mutex<HashMap<String, Arc<TunelActivo>>>,
    pub sincro: tokio::sync::Mutex<EstadoSincro>,
    pub clave_publica: Mutex<Option<ClavePublicaMovil>>,
    /// Último error legible por host (solo para la lista; no va a magi.db).
    pub ultimo_error: Mutex<HashMap<String, String>>,
    pub siguiente_id: AtomicU64,
}

impl EstadoNucleo {
    /// Abre la base de datos, arranca el runtime y prepara los servicios.
    pub fn nuevo(
        rutas: RutasNucleo,
        equipo: EquipoNucleo,
        retrollamadas: Retrollamadas,
    ) -> Result<Arc<Self>, ErrorNucleo> {
        let app_version = env!("CARGO_PKG_VERSION").to_string();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("magi-tokio")
            .enable_all()
            .build()
            .map_err(|error| ErrorNucleo::General {
                detalle: format!("no se pudo arrancar el runtime: {error}"),
            })?;
        let escritor = Escritor::arrancar(&rutas.base_datos(), &equipo, &app_version)
            .map_err(|detalle| ErrorNucleo::General { detalle })?;
        let llavero = Arc::new(LlaveroNucleo::nuevo(
            retrollamadas.llavero.clone(),
            escritor.clone(),
        ));
        let firmante = Arc::new(FirmanteNucleo::nuevo(retrollamadas.firmante.clone()));
        Ok(Arc::new(Self {
            retrollamadas,
            rutas,
            equipo,
            app_version,
            runtime,
            escritor,
            llavero,
            firmante,
            dialogos: Dialogos::nuevo(),
            sesiones: Mutex::new(HashMap::new()),
            conexiones: tokio::sync::Mutex::new(HashMap::new()),
            cerrojos: Mutex::new(HashMap::new()),
            tuneles: Mutex::new(HashMap::new()),
            sincro: tokio::sync::Mutex::new(EstadoSincro::default()),
            clave_publica: Mutex::new(None),
            ultimo_error: Mutex::new(HashMap::new()),
            siguiente_id: AtomicU64::new(1),
        }))
    }

    /// Siguiente identificador de sesión o diálogo.
    pub fn siguiente_id(&self) -> u64 {
        self.siguiente_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Cerrojo por host para no abrir dos conexiones a la vez.
    pub fn cerrojo_de_host(&self, host_uuid: &str) -> Arc<tokio::sync::Mutex<()>> {
        if let Ok(mut cerrojos) = self.cerrojos.lock() {
            return cerrojos
                .entry(host_uuid.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone();
        }
        Arc::new(tokio::sync::Mutex::new(()))
    }

    /// Clave pública del móvil, calculada una vez del Keystore.
    pub fn clave_publica(&self) -> Result<ClavePublicaMovil, ErrorNucleo> {
        if let Ok(cache) = self.clave_publica.lock() {
            if let Some(clave) = cache.as_ref() {
                return Ok(clave.clone());
            }
        }
        let clave = crate::clave::clave_publica_movil(self.firmante.as_ref(), &self.equipo)?;
        if let Ok(mut cache) = self.clave_publica.lock() {
            *cache = Some(clave.clone());
        }
        Ok(clave)
    }

    /// Sesiones del host: (abiertas, abriendo, caídas).
    pub fn recuento_sesiones(&self, host_uuid: &str) -> (usize, usize, usize) {
        let (mut abiertas, mut abriendo, mut caidas) = (0, 0, 0);
        if let Ok(sesiones) = self.sesiones.lock() {
            for sesion in sesiones.values() {
                if sesion.host_uuid != host_uuid {
                    continue;
                }
                match sesion.estado() {
                    EstadoSesion::Abierta => abiertas += 1,
                    EstadoSesion::Abriendo => abriendo += 1,
                    EstadoSesion::Caida => caidas += 1,
                }
            }
        }
        (abiertas, abriendo, caidas)
    }

    /// Publica la lista completa de sesiones al oyente.
    pub fn emitir_sesiones(&self) {
        let mut lista: Vec<Sesion> = match self.sesiones.lock() {
            Ok(sesiones) => sesiones
                .values()
                .map(|sesion| Sesion {
                    id: sesion.id,
                    host_uuid: sesion.host_uuid.clone(),
                    host_nombre: sesion.host_nombre.clone(),
                    estado: sesion.estado(),
                    detalle: sesion
                        .detalle
                        .lock()
                        .ok()
                        .and_then(|detalle| detalle.clone()),
                    abierta_en: sesion.abierta_en,
                })
                .collect(),
            Err(_) => return,
        };
        lista.sort_by_key(|sesion| sesion.id);
        self.retrollamadas.oyente.sesiones(lista);
    }

    /// Publica el estado de la sincronización (lo pinta Ajustes y el ⇅).
    pub fn emitir_sincronizacion(&self) {
        if let Ok(estado) = crate::sincro::estado_publico(self) {
            self.retrollamadas.oyente.sincronizacion(estado);
        }
    }

    /// Guarda el último error legible de un host, para la lista.
    pub fn anotar_error(&self, host_uuid: &str, motivo: &str) {
        if let Ok(mut errores) = self.ultimo_error.lock() {
            errores.insert(host_uuid.to_string(), motivo.to_string());
        }
    }

    /// Borra el último error de un host (conexión correcta).
    pub fn limpiar_error(&self, host_uuid: &str) {
        if let Ok(mut errores) = self.ultimo_error.lock() {
            errores.remove(host_uuid);
        }
    }

    /// Instantánea del trabajo vivo para la pasada de sincronización (§7.4).
    pub fn trabajo_vivo(&self) -> InstantaneaTrabajoVivo {
        let mut hosts = HashSet::new();
        if let Ok(sesiones) = self.sesiones.lock() {
            for sesion in sesiones.values() {
                hosts.insert(sesion.host_uuid.clone());
            }
        }
        let mut tuneles = HashSet::new();
        if let Ok(activos) = self.tuneles.lock() {
            for (uuid, tunel) in activos.iter() {
                hosts.insert(tunel.host_uuid.clone());
                if matches!(
                    tunel.estado(),
                    EstadoTunelVivo::Activo | EstadoTunelVivo::Activando
                ) {
                    tuneles.insert(uuid.clone());
                }
            }
        }
        InstantaneaTrabajoVivo { hosts, tuneles }
    }
}

/// Trabajo vivo del móvil en un instante dado (sesiones abiertas y túneles
/// activos), para que la pasada de sincronización aplace sus borrados.
pub struct InstantaneaTrabajoVivo {
    pub hosts: HashSet<String>,
    pub tuneles: HashSet<String>,
}

impl TrabajoVivo for InstantaneaTrabajoVivo {
    fn tiene_trabajo(&self, tipo: &str, uuid: &str) -> bool {
        match tipo {
            "host" => self.hosts.contains(uuid),
            "tunel" => self.tuneles.contains(uuid),
            _ => false,
        }
    }
}

/// Registro rápido de la base de datos para consultas de la app.
pub fn con_almacen<T: Send + 'static>(
    estado: &EstadoNucleo,
    tarea: impl FnOnce(&mut Almacen) -> T + Send + 'static,
) -> Result<T, ErrorNucleo> {
    estado
        .escritor
        .con_almacen(tarea)
        .map_err(|detalle| ErrorNucleo::General { detalle })
}

/// Igual que con_almacen, desde una función async.
pub async fn con_almacen_async<T: Send + 'static>(
    estado: &EstadoNucleo,
    tarea: impl FnOnce(&mut Almacen) -> T + Send + 'static,
) -> Result<T, ErrorNucleo> {
    estado
        .escritor
        .con_almacen_async(tarea)
        .await
        .map_err(|detalle| ErrorNucleo::General { detalle })
}

/// Petición de diálogo pendiente: se envía al oyente y se espera la respuesta.
pub async fn pedir_dialogo(
    estado: &EstadoNucleo,
    construir: impl FnOnce(u64) -> PeticionDialogo,
) -> Option<crate::api::RespuestaDialogo> {
    let (id, receptor) = estado.dialogos.registrar();
    estado.retrollamadas.oyente.dialogo(construir(id));
    receptor.await.ok()
}
