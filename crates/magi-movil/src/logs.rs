//! Logs del móvil (informe §4.4, §5.1, §6.5 y §7.4): suscripciones por
//! objetivo con servicios.sh logs, anillo en memoria (10 000 líneas y 8 MiB) y
//! ventanas ya filtradas de hasta 500 líneas (D278).
//!
//! El anillo vive aquí y nunca cruza la FFI entero: la app pide ventanas
//! acotadas y recibe avisos como mucho cada 100 ms con el total, las nuevas,
//! las omitidas y el estado. El seguimiento se para cerrando la entrada
//! estándar del script (fin de stdin), que es como el host corta journalctl.
//!
//! Cada suscripción conserva su uso de la conexión del host mientras dura: si
//! el host cae, el vigilante avisa y la suscripción acaba en «se cortó la
//! conexión» (§7.2). Al soltarse el uso, la conexión queda en gracia.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use magi_nucleo::servicios::{lectura, limites, orden as nucleo_orden, NivelLog, TipoObjetivo};
use tokio::sync::mpsc;

use crate::api::{
    ErrorNucleo, EstadoLogs, EstadoSuscripcionLog, FiltroLogs, LineaLogMovil, MotivoFinLog,
    NivelLogMovil, ObjetivoMovil, VentanaLogs,
};
use crate::estado::EstadoNucleo;
use crate::servicios;

/// Tope de líneas del anillo de una suscripción (D278).
pub const MAX_LINEAS: usize = limites::LINEAS_LOG_POR_DEFECTO;
/// Tope de bytes del anillo de una suscripción (D278).
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// Líneas iniciales por defecto cuando la app no pide otra cosa.
pub const LINEAS_INICIALES: u32 = 200;
/// Líneas iniciales máximas que admite el script.
pub const LINEAS_INICIALES_MAX: u32 = 5000;
/// Líneas máximas de una ventana (informe §5.1).
pub const VENTANA_MAX: u32 = 500;
/// Cada cuánto, como mucho, se avisa a la app de las novedades (informe §5.2).
pub const AVISO_MS: u64 = 100;
/// Tiempo que se queda una suscripción parada antes de poder descartarse.
const DESCARTE_PARADA_SEG: u64 = 10 * 60;
/// Suscripciones paradas que se conservan como mucho (las más nuevas).
const PARADAS_MAXIMAS: usize = 8;
/// Después de pedir el fin de la entrada estándar, este es el plazo para que el
/// host corte su journalctl antes de cerrar el canal por las bravas.
const ESPERA_PARADA_SEG: u64 = 10;

// ===========================================================================
// Anillo de líneas (informe §7.4)
// ===========================================================================

/// Anillo de líneas de una suscripción: 10 000 líneas y 8 MiB como mucho; las
/// que se descartan se cuentan para enseñar «… N líneas omitidas». Cada línea
/// lleva un número que no se repite: la vista en pausa se ancla a él y no se
/// mueve cuando el anillo descarta las viejas.
pub struct AnilloLogs {
    lineas: VecDeque<LineaLogMovil>,
    bytes: usize,
    siguiente: u64,
    omitidas: u64,
}

impl Default for AnilloLogs {
    fn default() -> Self {
        Self::nuevo()
    }
}

impl AnilloLogs {
    pub fn nuevo() -> Self {
        Self {
            lineas: VecDeque::new(),
            bytes: 0,
            siguiente: 0,
            omitidas: 0,
        }
    }

    /// Mete una línea ya leída del host (texto recortado a 16 KiB antes). Si el
    /// anillo pasa de sus topes, descarta las más viejas y las cuenta.
    pub fn meter(&mut self, linea: LineaLogMovil) -> u64 {
        let mut linea = linea;
        // El texto ya viene recortado por la lectura, pero el anillo también se
        // protege: nunca se guarda una línea mayor que el tope.
        if linea.texto.len() > limites::LINEA_LOG_MAX_BYTES {
            linea.texto =
                lectura::recortar_utf8(&linea.texto, limites::LINEA_LOG_MAX_BYTES).to_string();
        }
        let numero = self.siguiente;
        self.siguiente = self.siguiente.saturating_add(1);
        linea.numero = numero;
        self.bytes = self.bytes.saturating_add(linea.texto.len());
        self.lineas.push_back(linea);
        while self.lineas.len() > MAX_LINEAS || self.bytes > MAX_BYTES {
            let Some(vieja) = self.lineas.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(vieja.texto.len());
            self.omitidas = self.omitidas.saturating_add(1);
        }
        numero
    }

    /// Líneas que hay ahora mismo en el anillo.
    pub fn total(&self) -> u64 {
        self.lineas.len() as u64
    }

    /// Líneas descartadas desde el principio (recuento acumulado).
    pub fn omitidas(&self) -> u64 {
        self.omitidas
    }

    pub fn lineas(&self) -> &VecDeque<LineaLogMovil> {
        &self.lineas
    }

    /// Siguiente número que se repartirá (para pruebas).
    pub fn siguiente(&self) -> u64 {
        self.siguiente
    }
}

/// ¿La línea pasa el filtro de la app? (prioridad máxima del nivel, texto sin
/// distinguir mayúsculas y, en contenedores, solo la salida de errores)
fn visible(linea: &LineaLogMovil, filtro: &FiltroLogs) -> bool {
    let maxima = match filtro.nivel {
        NivelLogMovil::Todo => NivelLog::Todo.prioridad_maxima(),
        NivelLogMovil::Info => NivelLog::Info.prioridad_maxima(),
        NivelLogMovil::Aviso => NivelLog::Aviso.prioridad_maxima(),
        NivelLogMovil::Error => NivelLog::Error.prioridad_maxima(),
    };
    if linea.prioridad > maxima {
        return false;
    }
    if filtro.solo_stderr && !linea.stderr {
        return false;
    }
    let consulta = filtro.texto.trim();
    if consulta.is_empty() {
        return true;
    }
    linea
        .texto
        .to_lowercase()
        .contains(&consulta.to_lowercase())
}

/// Una ventana ya filtrada del anillo: hasta 500 líneas, desde un número (en
/// pausa, para que la vista no se mueva) o desde el final, con el total que
/// cumple el filtro y las omitidas acumuladas (informe §7.4).
pub fn ventana_de_anillo(
    anillo: &AnilloLogs,
    estado: EstadoSuscripcionLog,
    motivo: Option<MotivoFinLog>,
    filtro: &FiltroLogs,
) -> VentanaLogs {
    let visibles: Vec<&LineaLogMovil> = anillo
        .lineas()
        .iter()
        .filter(|linea| visible(linea, filtro))
        .collect();
    let total = visibles.len() as u64;
    let cuantas = (filtro.cuantas.max(1) as usize).min(VENTANA_MAX as usize);
    let lineas: Vec<LineaLogMovil> = match filtro.desde {
        None => visibles
            .iter()
            .skip(visibles.len().saturating_sub(cuantas))
            .map(|linea| (*linea).clone())
            .collect(),
        Some(desde) => {
            let inicio = visibles
                .iter()
                .position(|linea| linea.numero >= desde)
                .unwrap_or(visibles.len());
            visibles
                .iter()
                .skip(inicio)
                .take(cuantas)
                .map(|linea| (*linea).clone())
                .collect()
        }
    };
    VentanaLogs {
        lineas,
        total,
        omitidas: anillo.omitidas(),
        estado,
        motivo,
    }
}

// ===========================================================================
// Suscripción viva
// ===========================================================================

/// La orden que la app manda a la tarea de una suscripción.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdenLog {
    /// Cerrar la entrada estándar del script (fin de stdin): el host corta
    /// journalctl y el seguimiento acaba en «parado».
    Parar,
    /// Cerrar el canal ya mismo (se sustituye la suscripción o se descarta).
    Cerrar,
}

/// Contadores del aviso a la app: lo que se acumula hasta el siguiente aviso.
#[derive(Default)]
struct Avisos {
    nuevas: u64,
    omitidas: u64,
    estado_anunciado: Option<EstadoSuscripcionLog>,
}

/// Una suscripción de logs viva (informe §3.1): por objetivo, con su anillo, su
/// estado y el canal de órdenes para pararla.
pub struct SuscripcionLog {
    pub id: u64,
    pub host_uuid: String,
    pub host_nombre: String,
    pub objetivo: ObjetivoMovil,
    anillo: Mutex<AnilloLogs>,
    estado: Mutex<EstadoSuscripcionLog>,
    motivo: Mutex<Option<MotivoFinLog>>,
    avisos: Mutex<Avisos>,
    ordenes: mpsc::UnboundedSender<OrdenLog>,
    /// Motivo de la caída de la conexión, si la hubo.
    caida: Mutex<Option<String>>,
    /// Cuándo quedó parada o terminada (para descartarla más tarde).
    terminada_en: Mutex<Option<Instant>>,
}

impl SuscripcionLog {
    pub fn estado(&self) -> EstadoSuscripcionLog {
        self.estado
            .lock()
            .map(|estado| *estado)
            .unwrap_or(EstadoSuscripcionLog::ConexionCaida)
    }

    pub fn motivo(&self) -> Option<MotivoFinLog> {
        self.motivo.lock().ok().and_then(|motivo| *motivo)
    }

    fn fijar_fin(&self, estado: EstadoSuscripcionLog, motivo: MotivoFinLog) {
        if let Ok(mut actual) = self.estado.lock() {
            *actual = estado;
        }
        if let Ok(mut actual) = self.motivo.lock() {
            *actual = Some(motivo);
        }
        if let Ok(mut terminada) = self.terminada_en.lock() {
            *terminada = Some(Instant::now());
        }
    }

    /// ¿Ya terminó (parada, terminada o caída)?
    pub fn terminada(&self) -> bool {
        self.estado() != EstadoSuscripcionLog::Siguiendo
    }

    fn descartable(&self, ahora: Instant) -> bool {
        self.terminada_en
            .lock()
            .ok()
            .and_then(|terminada| *terminada)
            .is_some_and(|terminada| {
                ahora.saturating_duration_since(terminada)
                    >= Duration::from_secs(DESCARTE_PARADA_SEG)
            })
    }

    /// Cuándo quedó terminada, si ya terminó.
    fn terminada_en(&self) -> Option<Instant> {
        self.terminada_en
            .lock()
            .ok()
            .and_then(|terminada| *terminada)
            .filter(|_| self.terminada())
    }
}

/// Oyente de la caída de la conexión del host de una suscripción: apunta el
/// motivo para que la tarea de lectura acabe en «se cortó la conexión».
struct OyenteLogs {
    host_uuid: String,
    suscripcion: Arc<SuscripcionLog>,
}

impl OyenteLogs {
    fn nuevo(host_uuid: &str, suscripcion: Arc<SuscripcionLog>) -> Self {
        Self {
            host_uuid: host_uuid.to_string(),
            suscripcion,
        }
    }
}

impl crate::conexiones::OyenteConexion for OyenteLogs {
    fn conexion_caida(&self, host_uuid: &str, motivo: &str) {
        if host_uuid != self.host_uuid {
            return;
        }
        if let Ok(mut caida) = self.suscripcion.caida.lock() {
            *caida = Some(motivo.to_string());
        }
    }
}

// ===========================================================================
// API de logs (informe §5.1)
// ===========================================================================

/// Abre la suscripción de logs de un objetivo y devuelve su id (informe §5.1):
/// hay una por objetivo y la nueva sustituye a la anterior. La conexión es
/// interactiva porque los logs los pide el usuario (§7.2).
pub async fn seguir(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    objetivo_vista: &ObjetivoMovil,
    lineas: u32,
) -> Result<u64, ErrorNucleo> {
    let host = servicios::host_ssh(estado, host_uuid)?;
    let objetivo = servicios::objetivo_a_nucleo(objetivo_vista);
    if objetivo.tipo == TipoObjetivo::Proyecto {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "un proyecto Compose no tiene logs propios: elige un contenedor".to_string(),
        });
    }
    nucleo_orden::validar_objetivo(&objetivo)
        .map_err(|motivo| ErrorNucleo::EstadoInvalido { detalle: motivo })?;
    let lineas_iniciales = if lineas == 0 {
        LINEAS_INICIALES
    } else {
        lineas.min(LINEAS_INICIALES_MAX)
    };
    let uso = crate::conexiones::conexion_interactiva(estado, host_uuid).await?;
    let privilegio = match servicios::privilegio_cacheado(estado, host_uuid) {
        Some(privilegio) => privilegio,
        None => servicios::privilegio_de(estado, &uso, host_uuid).await?,
    };
    let suborden = nucleo_orden::Suborden::Logs {
        objetivo: objetivo.clone(),
        lineas_iniciales,
    };
    nucleo_orden::validar_suborden(&suborden)
        .map_err(|motivo| ErrorNucleo::General { detalle: motivo })?;
    let comando = nucleo_orden::comando(&suborden, privilegio);
    let password = servicios::password_para(estado, host_uuid, false);
    let mut canal = crate::conexiones::abrir_canal_remoto(&uso, &comando).await?;
    // La primera línea de stdin es SIEMPRE la contraseña (vacía si no hay):
    // el script la lee con read antes de lanzar journalctl y, como aquí la
    // entrada se queda abierta, no se puede sustituir por un EOF. El fin de
    // stdin es lo que para el seguimiento (logs_parar).
    match servicios::entrada_con_password(&password) {
        Some(entrada) => {
            let _ = canal.escribir(entrada.as_slice()).await;
        }
        None => {
            let _ = canal.escribir(b"\n").await;
        }
    }
    // La suscripción anterior del objetivo solo se sustituye cuando el canal
    // nuevo ya está abierto: si la apertura falla (huella, credenciales,
    // canal), sus líneas no se pierden.
    limpiar_suscripciones(estado);
    sustituir_suscripcion(estado, host_uuid, objetivo_vista);
    let id = estado.siguiente_id();
    let (emisor, receptor) = mpsc::unbounded_channel();
    let suscripcion = Arc::new(SuscripcionLog {
        id,
        host_uuid: host_uuid.to_string(),
        host_nombre: host.nombre.clone(),
        objetivo: objetivo_vista.clone(),
        anillo: Mutex::new(AnilloLogs::nuevo()),
        estado: Mutex::new(EstadoSuscripcionLog::Siguiendo),
        motivo: Mutex::new(None),
        avisos: Mutex::new(Avisos::default()),
        ordenes: emisor,
        caida: Mutex::new(None),
        terminada_en: Mutex::new(None),
    });
    estado
        .suscripciones_logs
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "estado de suscripciones de logs envenenado".to_string(),
        })?
        .insert(id, suscripcion.clone());
    let oyente = Arc::new(OyenteLogs::nuevo(host_uuid, suscripcion.clone()));
    let oyente_trait: Arc<dyn crate::conexiones::OyenteConexion> = oyente;
    estado.suscribir_oyente_conexion(oyente_trait.clone());
    let estado_lectura = estado.clone();
    let suscripcion_lectura = suscripcion.clone();
    estado.runtime.spawn(async move {
        tarea_suscripcion(
            estado_lectura,
            suscripcion_lectura,
            canal,
            receptor,
            uso,
            oyente_trait,
        )
        .await;
    });
    let estado_avisos = estado.clone();
    let suscripcion_avisos = suscripcion.clone();
    estado.runtime.spawn(async move {
        tarea_avisos(estado_avisos, suscripcion_avisos).await;
    });
    Ok(id)
}

/// Hasta 500 líneas ya filtradas, desde un número de línea o desde el final,
/// con el total, las omitidas y el estado (informe §5.1 y §7.4). Devuelve
/// error si la suscripción ya no existe.
pub fn ventana(
    estado: &EstadoNucleo,
    suscripcion: u64,
    filtro: &FiltroLogs,
) -> Result<VentanaLogs, ErrorNucleo> {
    let suscripcion = estado
        .suscripciones_logs
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "estado de suscripciones de logs envenenado".to_string(),
        })?
        .get(&suscripcion)
        .cloned()
        .ok_or_else(|| ErrorNucleo::NoEncontrado {
            detalle: "la suscripción de logs".to_string(),
        })?;
    let anillo = suscripcion
        .anillo
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "anillo de logs envenenado".to_string(),
        })?;
    let estado_suscripcion = suscripcion.estado();
    let motivo = suscripcion.motivo();
    Ok(ventana_de_anillo(
        &anillo,
        estado_suscripcion,
        motivo,
        filtro,
    ))
}

/// Cierra el canal (fin de la entrada estándar del script) y deja las líneas en
/// el anillo hasta que se suelte la pantalla.
pub fn parar(estado: &EstadoNucleo, suscripcion: u64) -> Result<(), ErrorNucleo> {
    let suscripcion = estado
        .suscripciones_logs
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "estado de suscripciones de logs envenenado".to_string(),
        })?
        .get(&suscripcion)
        .cloned()
        .ok_or_else(|| ErrorNucleo::NoEncontrado {
            detalle: "la suscripción de logs".to_string(),
        })?;
    if !suscripcion.terminada() {
        let _ = suscripcion.ordenes.send(OrdenLog::Parar);
    }
    Ok(())
}

/// Se queda con la lista de suscripciones del objetivo (la nueva sustituye a
/// la anterior, §4.4).
fn sustituir_suscripcion(estado: &EstadoNucleo, host_uuid: &str, objetivo: &ObjetivoMovil) {
    let sustituidas: Vec<Arc<SuscripcionLog>> = match estado.suscripciones_logs.lock() {
        Ok(mut suscripciones) => {
            let ids: Vec<u64> = suscripciones
                .iter()
                .filter(|(_, suscripcion)| {
                    suscripcion.host_uuid == host_uuid && suscripcion.objetivo == *objetivo
                })
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| suscripciones.remove(&id))
                .collect()
        }
        Err(_) => return,
    };
    for suscripcion in sustituidas {
        let _ = suscripcion.ordenes.send(OrdenLog::Cerrar);
    }
}

/// Descarta las suscripciones paradas de más de diez minutos y, si hay más de
/// PARADAS_MAXIMAS paradas, se queda con las más nuevas (decisión técnica de
/// task-7: el anillo no se puede quedar para siempre en una app móvil).
fn limpiar_suscripciones(estado: &EstadoNucleo) {
    let ahora = Instant::now();
    let Ok(mut suscripciones) = estado.suscripciones_logs.lock() else {
        return;
    };
    suscripciones.retain(|_, suscripcion| !suscripcion.descartable(ahora));
    if suscripciones.len() <= PARADAS_MAXIMAS {
        return;
    }
    let mut paradas: Vec<(u64, Instant)> = suscripciones
        .iter()
        .filter_map(|(id, suscripcion)| suscripcion.terminada_en().map(|cuando| (*id, cuando)))
        .collect();
    paradas.sort_by_key(|(_, cuando)| *cuando);
    let sobran = suscripciones.len().saturating_sub(PARADAS_MAXIMAS);
    for (id, _) in paradas.into_iter().take(sobran) {
        suscripciones.remove(&id);
    }
}

// ===========================================================================
// Tareas de la suscripción
// ===========================================================================

/// Lee el canal del host por líneas y las mete en el anillo. Al acabar deja el
/// estado y el motivo de fin (cerrado, terminado, sin permiso, conexión caída,
/// no encontrado) y suelta el uso de la conexión.
async fn tarea_suscripcion(
    estado: Arc<EstadoNucleo>,
    suscripcion: Arc<SuscripcionLog>,
    mut canal: crate::conexiones::CanalRemoto,
    mut ordenes: mpsc::UnboundedReceiver<OrdenLog>,
    _uso: crate::conexiones::UsoConexion,
    oyente: Arc<dyn crate::conexiones::OyenteConexion>,
) {
    let contenedor = suscripcion.objetivo.tipo == crate::api::TipoObjetivoMovil::Contenedor;
    let mut pendiente_stdout: Vec<u8> = Vec::new();
    let mut pendiente_stderr: Vec<u8> = Vec::new();
    let mut codigo: Option<u32> = None;
    let mut parada_pedida = false;
    let mut cerrada_a_proposito = false;
    let mut limite_parada: Option<Instant> = None;
    loop {
        tokio::select! {
            orden = ordenes.recv() => match orden {
                Some(OrdenLog::Parar) => {
                    if !parada_pedida {
                        parada_pedida = true;
                        limite_parada = Some(Instant::now() + Duration::from_secs(ESPERA_PARADA_SEG));
                        let _ = canal.cerrar_entrada().await;
                    }
                }
                Some(OrdenLog::Cerrar) | None => {
                    cerrada_a_proposito = true;
                    break;
                }
            },
            evento = canal.eventos.recv() => match evento {
                None | Some(crate::conexiones::EventoCanal::Fin) => break,
                Some(crate::conexiones::EventoCanal::Codigo(salida)) => codigo = Some(salida),
                Some(crate::conexiones::EventoCanal::Stdout(datos)) => {
                    for linea in servicios::extraer_lineas(&mut pendiente_stdout, &datos) {
                        if let Some(linea) = lectura::linea_de_log(&linea, contenedor) {
                            meter_linea(
                                &suscripcion,
                                LineaLogMovil {
                                    numero: 0,
                                    hora: linea.hora,
                                    prioridad: linea.prioridad,
                                    stderr: linea.stderr,
                                    texto: linea.texto,
                                },
                            );
                        }
                    }
                }
                Some(crate::conexiones::EventoCanal::Stderr(datos)) => {
                    // La salida de errores del script son sus ERROR …: no son
                    // líneas de log y no se enseñan.
                    for _linea in servicios::extraer_lineas(&mut pendiente_stderr, &datos) {}
                }
            },
            _ = dormir_hasta(limite_parada) => {
                // El host no cortó su proceso a tiempo: se cierra el canal.
                break;
            },
        }
    }
    // Una última línea sin salto de línea no se pierde: se procesa al cerrar
    // (el pendiente de errores del script se descarta).
    if !pendiente_stdout.is_empty() {
        let linea = String::from_utf8_lossy(&pendiente_stdout).into_owned();
        pendiente_stdout.clear();
        if let Some(linea) = lectura::linea_de_log(&linea, contenedor) {
            let numero = suscripcion
                .anillo
                .lock()
                .map(|anillo| anillo.siguiente())
                .unwrap_or(0);
            meter_linea(
                &suscripcion,
                LineaLogMovil {
                    numero,
                    hora: linea.hora,
                    prioridad: linea.prioridad,
                    stderr: linea.stderr,
                    texto: linea.texto,
                },
            );
        }
    }
    canal.cerrar();
    let caida = suscripcion
        .caida
        .lock()
        .ok()
        .and_then(|caida| caida.clone());
    let (estado_final, motivo_final) = if parada_pedida || cerrada_a_proposito {
        (EstadoSuscripcionLog::Parada, MotivoFinLog::Cerrado)
    } else if caida.is_some() {
        (
            EstadoSuscripcionLog::ConexionCaida,
            MotivoFinLog::ConexionCaida,
        )
    } else {
        match codigo {
            Some(0) => (EstadoSuscripcionLog::Terminada, MotivoFinLog::Terminado),
            Some(66) => (
                EstadoSuscripcionLog::NoEncontrada,
                MotivoFinLog::NoEncontrado,
            ),
            Some(69) => (EstadoSuscripcionLog::SinPermiso, MotivoFinLog::SinPermiso),
            _ => (
                EstadoSuscripcionLog::ConexionCaida,
                MotivoFinLog::ConexionCaida,
            ),
        }
    };
    suscripcion.fijar_fin(estado_final, motivo_final);
    estado.quitar_oyente_conexion(&oyente);
    // El uso de la conexión se suelta aquí (el parámetro _uso se cae al acabar
    // la tarea), en todas las ramas de salida.
}

/// Un temporizador que no vence si no hay límite: se usa en el select de la
/// espera de parada.
async fn dormir_hasta(limite: Option<Instant>) {
    match limite {
        Some(limite) => tokio::time::sleep_until(tokio::time::Instant::from_std(limite)).await,
        None => std::future::pending().await,
    }
}

/// Mete una línea en el anillo de la suscripción y apunta sus contadores para
/// el siguiente aviso.
fn meter_linea(suscripcion: &SuscripcionLog, linea: LineaLogMovil) {
    let (nuevas, omitidas) = match suscripcion.anillo.lock() {
        Ok(mut anillo) => {
            let antes = anillo.omitidas();
            anillo.meter(linea);
            (1, anillo.omitidas() - antes)
        }
        Err(_) => return,
    };
    if let Ok(mut avisos) = suscripcion.avisos.lock() {
        avisos.nuevas = avisos.nuevas.saturating_add(nuevas);
        avisos.omitidas = avisos.omitidas.saturating_add(omitidas);
    }
}

/// Avisa a la app, como mucho cada 100 ms, del total, las nuevas, las omitidas
/// y el estado de la suscripción (informe §5.2). Termina cuando la
/// suscripción ya no está en el mapa o ha terminado.
async fn tarea_avisos(estado: Arc<EstadoNucleo>, suscripcion: Arc<SuscripcionLog>) {
    loop {
        tokio::time::sleep(Duration::from_millis(AVISO_MS)).await;
        // Si la suscripción se sustituyó o se descartó, la tarea termina.
        let viva = estado
            .suscripciones_logs
            .lock()
            .ok()
            .and_then(|suscripciones| suscripciones.get(&suscripcion.id).cloned())
            .is_some_and(|actual| Arc::ptr_eq(&actual, &suscripcion));
        if !viva {
            return;
        }
        let estado_actual = suscripcion.estado();
        let terminada = suscripcion.terminada();
        let (nuevas, omitidas, anunciar) = match suscripcion.avisos.lock() {
            Ok(mut avisos) => {
                let nuevas = std::mem::take(&mut avisos.nuevas);
                let omitidas = std::mem::take(&mut avisos.omitidas);
                let anunciar =
                    nuevas > 0 || omitidas > 0 || avisos.estado_anunciado != Some(estado_actual);
                avisos.estado_anunciado = Some(estado_actual);
                (nuevas, omitidas, anunciar)
            }
            Err(_) => return,
        };
        if anunciar {
            let total = suscripcion
                .anillo
                .lock()
                .map(|anillo| anillo.total())
                .unwrap_or(0);
            estado.retrollamadas.oyente.logs(EstadoLogs {
                suscripcion: suscripcion.id,
                total,
                nuevas,
                omitidas,
                estado: estado_actual,
            });
        }
        if terminada && !anunciar {
            return;
        }
        if terminada {
            // Último aviso del fin: ya se ha mandado el estado final.
            return;
        }
    }
}

// ===========================================================================
// Pruebas
// ===========================================================================

#[cfg(test)]
mod pruebas {
    use super::*;

    fn linea(numero: u64, prioridad: u8, stderr: bool, texto: &str) -> LineaLogMovil {
        LineaLogMovil {
            numero,
            hora: None,
            prioridad,
            stderr,
            texto: texto.to_string(),
        }
    }

    fn filtro(texto: &str, nivel: NivelLogMovil, desde: Option<u64>, cuantas: u32) -> FiltroLogs {
        FiltroLogs {
            texto: texto.to_string(),
            nivel,
            solo_stderr: false,
            desde,
            cuantas,
        }
    }

    #[test]
    fn el_anillo_descarta_las_mas_viejas_y_las_cuenta() {
        let mut anillo = AnilloLogs::nuevo();
        for indice in 0..(MAX_LINEAS as u64 + 5) {
            anillo.meter(linea(indice, 6, false, "hola"));
        }
        assert_eq!(anillo.total(), MAX_LINEAS as u64);
        assert_eq!(anillo.omitidas(), 5);
        assert_eq!(anillo.siguiente(), MAX_LINEAS as u64 + 5);
        // El número de la primera que queda es el 5 (no se reutiliza).
        assert_eq!(anillo.lineas().front().map(|linea| linea.numero), Some(5));
        assert_eq!(
            anillo.lineas().back().map(|linea| linea.numero),
            Some(MAX_LINEAS as u64 + 4)
        );
    }

    #[test]
    fn el_anillo_tambien_se_corta_por_bytes() {
        let mut anillo = AnilloLogs::nuevo();
        let texto = "x".repeat(1024);
        let cuantas = MAX_BYTES / texto.len() + 10;
        for indice in 0..cuantas as u64 {
            anillo.meter(linea(indice, 6, false, &texto));
        }
        assert!(anillo.total() < cuantas as u64);
        assert!(anillo.omitidas() > 0);
        let bytes: usize = anillo.lineas().iter().map(|linea| linea.texto.len()).sum();
        assert!(bytes <= MAX_BYTES);
    }

    #[test]
    fn una_linea_demasiado_larga_se_recorta_al_meterla() {
        let mut anillo = AnilloLogs::nuevo();
        let texto = "á".repeat(limites::LINEA_LOG_MAX_BYTES);
        anillo.meter(linea(0, 6, false, &texto));
        let guardada = anillo.lineas().front().expect("una línea");
        assert!(guardada.texto.len() <= limites::LINEA_LOG_MAX_BYTES);
        assert!(guardada.texto.is_char_boundary(guardada.texto.len()));
    }

    #[test]
    fn la_ventana_sin_desde_devuelve_las_ultimas_y_se_acota() {
        let mut anillo = AnilloLogs::nuevo();
        for indice in 0..1_000 {
            anillo.meter(linea(indice, 6, false, &format!("línea {indice}")));
        }
        let ventana = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("", NivelLogMovil::Todo, None, 60),
        );
        assert_eq!(ventana.lineas.len(), 60);
        assert_eq!(ventana.lineas[0].texto, "línea 940");
        assert_eq!(ventana.lineas[59].texto, "línea 999");
        assert_eq!(ventana.total, 1_000);
        // Por más que pida la app, como mucho 500.
        let ventana = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("", NivelLogMovil::Todo, None, 5_000),
        );
        assert_eq!(ventana.lineas.len(), VENTANA_MAX as usize);
    }

    #[test]
    fn la_ventana_con_desde_se_ancla_al_numero_sin_moverse() {
        let mut anillo = AnilloLogs::nuevo();
        for indice in 0..100 {
            anillo.meter(linea(indice, 6, false, &format!("línea {indice}")));
        }
        let en_pausa = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("", NivelLogMovil::Todo, Some(40), 10),
        );
        assert_eq!(en_pausa.lineas[0].numero, 40);
        // Llegan más líneas: la vista en pausa sigue enseñando las mismas.
        for indice in 100..120 {
            anillo.meter(linea(indice, 6, false, &format!("línea {indice}")));
        }
        let otra_vez = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("", NivelLogMovil::Todo, Some(40), 10),
        );
        assert_eq!(otra_vez.lineas[0].numero, 40);
        assert_eq!(otra_vez.lineas[9].numero, 49);
        // La cabecera que dice cuántas hay sigue creciendo.
        assert_eq!(otra_vez.total, 120);
    }

    #[test]
    fn el_filtro_de_texto_no_distingue_mayusculas() {
        let mut anillo = AnilloLogs::nuevo();
        anillo.meter(linea(0, 6, false, "Arrancando app"));
        anillo.meter(linea(1, 6, false, "otra cosa"));
        let ventana = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("arrancando", NivelLogMovil::Todo, None, 10),
        );
        assert_eq!(ventana.total, 1);
        assert_eq!(ventana.lineas[0].texto, "Arrancando app");
    }

    #[test]
    fn el_filtro_por_nivel_deja_los_de_aviso_y_error() {
        let mut anillo = AnilloLogs::nuevo();
        anillo.meter(linea(0, 6, false, "info"));
        anillo.meter(linea(1, 4, false, "aviso"));
        anillo.meter(linea(2, 3, false, "error"));
        anillo.meter(linea(3, 0, false, "emergencia"));
        let aviso = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("", NivelLogMovil::Aviso, None, 10),
        );
        assert_eq!(aviso.total, 3);
        let error = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("", NivelLogMovil::Error, None, 10),
        );
        assert_eq!(error.total, 2);
        let info = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Siguiendo,
            None,
            &filtro("", NivelLogMovil::Info, None, 10),
        );
        assert_eq!(info.total, 4);
    }

    #[test]
    fn el_filtro_solo_stderr_de_contenedores() {
        let mut anillo = AnilloLogs::nuevo();
        anillo.meter(linea(0, 6, false, "salida"));
        anillo.meter(linea(1, 3, true, "error del contenedor"));
        let mut con_stderr = filtro("", NivelLogMovil::Todo, None, 10);
        con_stderr.solo_stderr = true;
        let ventana =
            ventana_de_anillo(&anillo, EstadoSuscripcionLog::Siguiendo, None, &con_stderr);
        assert_eq!(ventana.total, 1);
        assert_eq!(ventana.lineas[0].texto, "error del contenedor");
    }

    #[test]
    fn la_ventana_lleva_las_omitidas_y_el_motivo_del_fin() {
        let mut anillo = AnilloLogs::nuevo();
        for indice in 0..(MAX_LINEAS as u64 + 3) {
            anillo.meter(linea(indice, 6, false, "hola"));
        }
        let ventana = ventana_de_anillo(
            &anillo,
            EstadoSuscripcionLog::Parada,
            Some(MotivoFinLog::Cerrado),
            &filtro("", NivelLogMovil::Todo, None, 10),
        );
        assert_eq!(ventana.omitidas, 3);
        assert_eq!(ventana.estado, EstadoSuscripcionLog::Parada);
        assert_eq!(ventana.motivo, Some(MotivoFinLog::Cerrado));
    }

    #[test]
    fn una_suscripcion_nueva_hereda_anillo_vacio_y_siguiente_en_cero() {
        let anillo = AnilloLogs::nuevo();
        assert_eq!(anillo.total(), 0);
        assert_eq!(anillo.omitidas(), 0);
        assert_eq!(anillo.siguiente(), 0);
    }
}
