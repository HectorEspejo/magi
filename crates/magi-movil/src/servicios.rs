//! Servicios, contenedores y acciones del móvil (informe §4.2, §4.3, §5.1,
//! §7.3 y §7.5): hace lo que el servidor de sesiones del escritorio, en el
//! teléfono.
//!
//! Todo lo remoto va con el script embebido del núcleo
//! (magi_nucleo::servicios::orden::SCRIPT, aquí servicios.sh) por la conexión
//! del host del gestor del móvil; los argumentos los compone y valida el
//! núcleo. Este módulo nunca mete datos del inventario en el texto de una
//! orden.
//!
//! También vive aquí la contraseña de sudo en memoria (por host, Zeroizing,
//! sudo_memoria_min sin uso, solo las acciones la renuevan) y la contabilidad
//! de las acciones en curso (una a la vez por objetivo), que se difunde por
//! OyenteNucleo.acciones_servicio antes del efecto.
//!
//! Interfaz para el resto de pistas:
//! - sudo_en_memoria(estado, host) para el listado (ListadoServicios).
//! - emitir_acciones(estado): lo llama el motor de ejecuciones si le hace falta.
//! - olvidar_sudo_al_bloquear(estado): lo llama la app al bloquear MAGI.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::deliberacion::EjecucionResultado;
use magi_nucleo::modelo::{self, Host};
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::servicios::{
    acciones as nucleo_acciones, lectura, limites, orden as nucleo_orden, validadores, Accion,
    EstadoArranque, EstadoContenedor, FaseAccion, Listado, MotorEstado, MotorNombre, Objetivo,
    OrdenPestana, PermisoDiario, Privilegio, ResultadoAccion, ResultadoServicio, Salud, Systemd,
    TipoFallo, TipoObjetivo,
};
use zeroize::Zeroizing;

use crate::api::{
    AccionMovil, AccionServicioMovil, Consumo, ContenedorMovil, DetalleContenedorMovil,
    DetalleServicios, ErrorNucleo, EstadoArranqueMovil, EstadoContenedorMovil, FalloFlota,
    FaseAccionMovil, FilaTodaLaFlota, HostTodaLaFlota, InfoAccionMovil, ListadoServicios,
    MotorEstadoMovil, MotorMovil, MotorNombreMovil, ObjetivoMovil, OrdenPestanaMovil,
    PermisoDiarioMovil, PlanDeliberadoMovil, PrivilegioMovil, PropiedadMovil, ProyectoMovil,
    RelojHostMovil, ResultadoAccionMovil, ResultadoServicioMovil, ResultadoSudo, SaludMovil,
    SystemdMovil, TimerMovil, TipoFalloMovil, TipoHostMovil, TipoObjetivoMovil, TodaLaFlota,
    UnidadMovil,
};
use crate::conexiones::{self, EventoCanal, SalidaRemota, UsoConexion};
use crate::estado::{con_almacen, EstadoNucleo};

/// Tope de salida de un listado o de un detalle: los 2 MiB de la entrada no
/// fiable de la Fase 13 (§8).
const TOPE_LISTADO: usize = limites::LISTADO_MAX_BYTES;
/// Tope de salida de las subórdenes cortas (privilegio y sudo).
const TOPE_SUDO: usize = limites::SALIDA_COMPROBACION_MAX_BYTES;
/// Tope de salida de una acción (fases, mensajes, últimas líneas y resultado).
const TOPE_ACCION: usize = limites::LISTADO_MAX_BYTES;
/// Plazo del detalle: el script tiene 10 s para systemctl cat más el show y las
/// dependencias; 20 s cubre la red y el canal.
const PLAZO_DETALLE_SEG: u64 = 20;
/// Plazo de sudo: el script da 15 s a sudo -S -v.
const PLAZO_SUDO_SEG: u64 = 20;
/// Plazo del consumo: docker stats --no-stream puede tardar 20 s.
const PLAZO_CONSUMO_SEG: u64 = 30;

// ===========================================================================
// Contraseña de sudo en memoria (informe §7.5, D268)
// ===========================================================================

/// Una contraseña de sudo recordada: el secreto y su último uso.
struct ClaveSudo {
    clave: Zeroizing<String>,
    ultimo_uso: Instant,
}

/// Contraseñas de sudo en memoria, por host (informe §7.5): solo viven aquí,
/// en Zeroizing, hasta sudo_memoria_min sin uso (0 = pedirla siempre), al
/// bloquearse MAGI, si el host la rechaza o con «Olvidar». Solo las acciones la
/// renuevan (D260 de la Fase 13); los listados, el consumo, el detalle y los
/// logs la usan sin alargar su vida.
#[derive(Default)]
pub struct SudoEnMemoria {
    claves: HashMap<String, ClaveSudo>,
}

impl SudoEnMemoria {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// La contraseña vigente del host, sin renovarla. Con minutos = 0 (pedirla
    /// siempre) no se guarda ninguna: si había, se olvida.
    pub fn vigente(&mut self, host_uuid: &str, minutos: u64) -> Option<Zeroizing<String>> {
        if minutos == 0 {
            self.olvidar(host_uuid);
            return None;
        }
        let caducada = self.claves.get(host_uuid).is_some_and(|clave| {
            clave.ultimo_uso.elapsed() >= Duration::from_secs(minutos.saturating_mul(60))
        });
        if caducada {
            self.olvidar(host_uuid);
        }
        self.claves.get(host_uuid).map(|clave| clave.clave.clone())
    }

    /// La contraseña para una acción: si está vigente, renueva su último uso.
    pub fn para_accion(&mut self, host_uuid: &str, minutos: u64) -> Option<Zeroizing<String>> {
        let clave = self.vigente(host_uuid, minutos)?;
        if let Some(guardada) = self.claves.get_mut(host_uuid) {
            guardada.ultimo_uso = Instant::now();
        }
        Some(clave)
    }

    /// Guarda la contraseña del host tras comprobarla con servicios.sh sudo.
    pub fn guardar(&mut self, host_uuid: &str, clave: Zeroizing<String>) {
        self.claves.insert(
            host_uuid.to_string(),
            ClaveSudo {
                clave,
                ultimo_uso: Instant::now(),
            },
        );
    }

    /// Olvida la contraseña de un host (se borra con Zeroizing al soltarla).
    pub fn olvidar(&mut self, host_uuid: &str) {
        self.claves.remove(host_uuid);
    }

    /// Olvida todas las contraseñas (bloqueo de MAGI, §7.5).
    pub fn olvidar_todas(&mut self) {
        self.claves.clear();
    }
}

/// La contraseña de sudo vigente de un host, sin renovarla (para el listado y
/// como interfaz interna de otras pistas, informe §5.1).
pub fn sudo_en_memoria(estado: &EstadoNucleo, host_uuid: &str) -> Option<Zeroizing<String>> {
    let minutos = crate::ajustes::actuales(estado).sudo_memoria_min;
    estado
        .sudo_en_memoria
        .lock()
        .ok()?
        .vigente(host_uuid, minutos)
}

/// La contraseña para una orden: renovar solo en las acciones (D260).
pub(crate) fn password_para(
    estado: &EstadoNucleo,
    host_uuid: &str,
    renovar: bool,
) -> Option<Zeroizing<String>> {
    let minutos = crate::ajustes::actuales(estado).sudo_memoria_min;
    let mut memoria = estado.sudo_en_memoria.lock().ok()?;
    if renovar {
        memoria.para_accion(host_uuid, minutos)
    } else {
        memoria.vigente(host_uuid, minutos)
    }
}

/// La primera línea de la entrada estándar del script es la contraseña (F13
/// §7.3): se manda la contraseña y su salto de línea.
pub(crate) fn entrada_con_password(
    password: &Option<Zeroizing<String>>,
) -> Option<Zeroizing<Vec<u8>>> {
    let password = password.as_ref()?;
    let mut entrada = Zeroizing::new(Vec::with_capacity(password.len() + 1));
    entrada.extend_from_slice(password.as_bytes());
    entrada.push(b'\n');
    Some(entrada)
}

/// Olvida todas las contraseñas de sudo: lo llama la app al bloquear MAGI
/// (informe §7.5 y D268).
pub fn olvidar_sudo_al_bloquear(estado: &EstadoNucleo) {
    if let Ok(mut memoria) = estado.sudo_en_memoria.lock() {
        memoria.olvidar_todas();
    }
}

/// Olvida la contraseña de sudo de un host («Olvidar contraseña de sudo»).
pub fn sudo_olvidar(estado: &EstadoNucleo, host_uuid: &str) -> Result<(), ErrorNucleo> {
    if let Ok(mut memoria) = estado.sudo_en_memoria.lock() {
        memoria.olvidar(host_uuid);
    }
    Ok(())
}

// ===========================================================================
// Acciones en curso (informe §3.1 y §5.2)
// ===========================================================================

/// Una acción de servicio en curso. Se difunde en la lista completa de
/// OyenteNucleo.acciones_servicio antes del efecto (T40).
pub struct AccionEnCurso {
    pub id: u64,
    pub host_uuid: String,
    pub host_nombre: String,
    pub objetivo: ObjetivoMovil,
    pub accion: AccionMovil,
    pub fase: FaseAccionMovil,
    pub deliberacion_id: Option<u64>,
    /// Época local en milisegundos en la que empezó.
    pub desde_ms: i64,
}

impl AccionEnCurso {
    /// La fila que ve la app (informe §3.1).
    pub fn a_vista(&self) -> AccionServicioMovil {
        AccionServicioMovil {
            id: self.id,
            host_uuid: Some(self.host_uuid.clone()),
            host_nombre: self.host_nombre.clone(),
            objetivo: self.objetivo.clone(),
            accion: self.accion,
            fase: self.fase,
            deliberacion_id: self.deliberacion_id,
            desde_ms: self.desde_ms,
        }
    }
}

/// Difunde la lista completa de acciones en curso (§5.2).
pub fn emitir_acciones(estado: &EstadoNucleo) {
    let mut lista: Vec<AccionServicioMovil> = match estado.acciones_servicio_en_curso.lock() {
        Ok(acciones) => acciones.values().map(AccionEnCurso::a_vista).collect(),
        Err(_) => return,
    };
    lista.sort_by_key(|accion| accion.id);
    estado.retrollamadas.oyente.acciones_servicio(lista);
}

/// Actualiza la fase de una acción en curso y la difunde, si sigue viva.
pub fn actualizar_fase(estado: &EstadoNucleo, id: u64, fase: FaseAccionMovil) {
    let difundir = match estado.acciones_servicio_en_curso.lock() {
        Ok(mut acciones) => match acciones.get_mut(&id) {
            Some(accion) if accion.fase != fase => {
                accion.fase = fase;
                true
            }
            _ => false,
        },
        Err(_) => false,
    };
    if difundir {
        emitir_acciones(estado);
    }
}

// ===========================================================================
// Peticiones de listado en vuelo (informe §5.1)
// ===========================================================================

/// Fallo de un listado en forma clonable: ErrorNucleo no es Clone y las
/// peticiones que esperan al mismo listado en vuelo tienen que recibirlo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalloListado {
    HuellaSinAceptar,
    SinCredenciales,
    NoResponde,
    General(String),
}

impl FalloListado {
    fn desde(error: &ErrorNucleo) -> Self {
        match error {
            ErrorNucleo::HuellaSinAceptar => FalloListado::HuellaSinAceptar,
            ErrorNucleo::SinCredenciales => FalloListado::SinCredenciales,
            ErrorNucleo::NoResponde => FalloListado::NoResponde,
            otro => FalloListado::General(otro.to_string()),
        }
    }

    fn a_error(&self) -> ErrorNucleo {
        match self {
            FalloListado::HuellaSinAceptar => ErrorNucleo::HuellaSinAceptar,
            FalloListado::SinCredenciales => ErrorNucleo::SinCredenciales,
            FalloListado::NoResponde => ErrorNucleo::NoResponde,
            FalloListado::General(detalle) => ErrorNucleo::General {
                detalle: detalle.clone(),
            },
        }
    }
}

type ResultadoListado = Result<Arc<ListadoServicios>, FalloListado>;

/// Una petición de listado en vuelo de un host: las que llegan mientras tanto
/// no abren otra, esperan a esta y devuelven su mismo resultado (§5.1).
pub struct SolicitudListado {
    emisor: tokio::sync::watch::Sender<Option<ResultadoListado>>,
}

/// Toma el turno de listado de un host. Devuelve si hay que ejecutarlo (true)
/// o si ya hay otro en vuelo y basta con esperar su resultado (false).
fn iniciar_solicitud(
    estado: &EstadoNucleo,
    host_uuid: &str,
) -> Result<(bool, tokio::sync::watch::Receiver<Option<ResultadoListado>>), ErrorNucleo> {
    let mut solicitudes = estado
        .solicitudes_listado
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "estado de solicitudes de listado envenenado".to_string(),
        })?;
    if let Some(solicitud) = solicitudes.get(host_uuid) {
        return Ok((false, solicitud.emisor.subscribe()));
    }
    let (emisor, receptor) = tokio::sync::watch::channel(None);
    solicitudes.insert(host_uuid.to_string(), Arc::new(SolicitudListado { emisor }));
    Ok((true, receptor))
}

/// Publica el resultado de la petición en vuelo y la retira del mapa. Se hace
/// antes de devolver, para que las que esperaban no se queden colgadas.
fn terminar_solicitud(estado: &EstadoNucleo, host_uuid: &str, resultado: ResultadoListado) {
    let solicitud = estado
        .solicitudes_listado
        .lock()
        .ok()
        .and_then(|mut solicitudes| solicitudes.remove(host_uuid));
    if let Some(solicitud) = solicitud {
        let _ = solicitud.emisor.send(Some(resultado));
    }
}

/// Espera el resultado de la petición de listado en vuelo de un host.
async fn esperar_solicitud(
    mut receptor: tokio::sync::watch::Receiver<Option<ResultadoListado>>,
) -> Result<ListadoServicios, ErrorNucleo> {
    let espera = async {
        loop {
            if let Some(resultado) = receptor.borrow().clone() {
                return resultado;
            }
            if receptor.changed().await.is_err() {
                return Err(FalloListado::General(
                    "la petición de listado se perdió".to_string(),
                ));
            }
        }
    };
    // La petición en vuelo tiene su propio plazo de 10 s; este margen solo
    // evita que una tarea perdida deje colgada a la app.
    match tokio::time::timeout(Duration::from_secs(limites::PLAZO_LISTADO_SEG + 20), espera).await {
        Ok(resultado) => resultado
            .map(|listado| (*listado).clone())
            .map_err(|fallo| fallo.a_error()),
        Err(_) => Err(ErrorNucleo::NoResponde),
    }
}

// ===========================================================================
// Caché de listados y privilegios
// ===========================================================================

/// El último listado bueno de un host (lo usa preparar_accion, que es síncrona,
/// y el consumo para saber el motor).
pub(crate) fn listado_cacheado(estado: &EstadoNucleo, host_uuid: &str) -> Option<ListadoServicios> {
    estado
        .listados_servicios
        .lock()
        .ok()?
        .get(host_uuid)
        .cloned()
}

fn cachear_listado(estado: &EstadoNucleo, host_uuid: &str, listado: ListadoServicios) {
    if let Ok(mut listados) = estado.listados_servicios.lock() {
        listados.insert(host_uuid.to_string(), listado);
    }
}

/// El privilegio ya averiguado del host, si se conoce.
pub(crate) fn privilegio_cacheado(estado: &EstadoNucleo, host_uuid: &str) -> Option<Privilegio> {
    estado
        .privilegios_servicios
        .lock()
        .ok()?
        .get(host_uuid)
        .copied()
}

fn guardar_privilegio(estado: &EstadoNucleo, host_uuid: &str, privilegio: Privilegio) {
    if let Ok(mut privilegios) = estado.privilegios_servicios.lock() {
        privilegios.insert(host_uuid.to_string(), privilegio);
    }
}

/// El privilegio ya averiguado del host o, si no se conoce, el que diga ahora
/// el host (servicios.sh privilegio, §7.3).
pub(crate) async fn privilegio_de(
    estado: &EstadoNucleo,
    uso: &UsoConexion,
    host_uuid: &str,
) -> Result<Privilegio, ErrorNucleo> {
    if let Some(privilegio) = privilegio_cacheado(estado, host_uuid) {
        return Ok(privilegio);
    }
    averiguar_privilegio(estado, uso, host_uuid).await
}

/// Ejecuta la suborden privilegio y guarda lo que diga el host, sin mirar la
/// caché: el listado lo hace en cada refresco porque el privilegio puede
/// cambiar en el host (por ejemplo, tras dar la contraseña de sudo) y de él
/// dependen los botones y la contraseña que se pide.
pub(crate) async fn averiguar_privilegio(
    estado: &EstadoNucleo,
    uso: &UsoConexion,
    host_uuid: &str,
) -> Result<Privilegio, ErrorNucleo> {
    let suborden = nucleo_orden::Suborden::Privilegio;
    let comando = nucleo_orden::comando(&suborden, Privilegio::SinSudo);
    let salida = conexiones::ejecutar_remoto(
        uso,
        &comando,
        None,
        Duration::from_secs(limites::PLAZO_LISTADO_SEG),
        TOPE_SUDO,
    )
    .await?;
    if salida.codigo != Some(0) {
        return Err(error_de_salida(&salida));
    }
    let texto = String::from_utf8_lossy(&salida.stdout);
    let (_, privilegio) = lectura::privilegio(&texto).map_err(|error| ErrorNucleo::General {
        detalle: format!("no se pudo leer el privilegio en el host: {error}"),
    })?;
    guardar_privilegio(estado, host_uuid, privilegio);
    Ok(privilegio)
}

// ===========================================================================
// Lectura del host: host, motores y errores
// ===========================================================================

/// El host del inventario, comprobando que es SSH (solo ellos tienen servicios).
pub(crate) fn host_ssh(estado: &EstadoNucleo, host_uuid: &str) -> Result<Host, ErrorNucleo> {
    let uuid = host_uuid.to_string();
    let host = con_almacen(estado, move |almacen| {
        movil_bd::host_por_uuid(almacen.conexion(), &uuid)
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
    if !host.tipo.es_ssh() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: format!(
                "«{}» es un escritorio y sus servicios se abren en el ordenador",
                host.nombre
            ),
        });
    }
    Ok(host)
}

/// Motores de contenedores de los ajustes, en el formato que espera el script
/// (docker,podman, docker, podman o -).
fn motores_para_listar(estado: &EstadoNucleo) -> String {
    let preferencia: Vec<MotorNombre> = crate::ajustes::motores(estado)
        .iter()
        .filter_map(|motor| MotorNombre::desde_texto(motor))
        .collect();
    let motores = nucleo_orden::motores_para_listar(&preferencia);
    if nucleo_orden::motores_validos(&motores) {
        motores
    } else {
        "-".to_string()
    }
}

/// El mensaje legible de un fallo del script (§6 de la Fase 13): sus líneas
/// ERROR en la salida de errores o el código de salida.
fn mensaje_de_error(texto: &str, codigo: Option<u32>) -> Option<String> {
    for linea in texto.lines().rev() {
        let linea = linea.trim();
        if let Some(resto) = linea.strip_prefix("ERROR ") {
            return Some(match resto.trim() {
                "objetivo" => "el objetivo no es válido para acciones".to_string(),
                "privilegio" => "el privilegio del script no es válido".to_string(),
                "sin_privilegio" => {
                    "sin privilegios en el host para esa orden: da la contraseña de sudo"
                        .to_string()
                }
                "sin_permiso" => {
                    "sin permiso en el host para leer eso: da la contraseña de sudo".to_string()
                }
                "sin_ficheros" => "no se conocen los ficheros de Compose del proyecto".to_string(),
                "sin_temporales" => {
                    "al host le faltan mktemp o mkfifo para seguir los logs".to_string()
                }
                otro => format!("el host devolvió «{otro}»"),
            });
        }
    }
    match codigo {
        Some(64) => Some("el host rechazó un argumento de la orden".to_string()),
        Some(66) => Some("el objetivo ya no existe en el host".to_string()),
        Some(69) => {
            Some("sin permiso en el host para leer eso: da la contraseña de sudo".to_string())
        }
        Some(70) => Some("al host le faltan temporales para seguir los logs".to_string()),
        Some(77) => {
            Some("sin privilegios en el host para esa orden: da la contraseña de sudo".to_string())
        }
        _ => None,
    }
}

/// Error legible de una suborden que terminó con código distinto de cero.
fn error_de_salida(salida: &SalidaRemota) -> ErrorNucleo {
    let errores = String::from_utf8_lossy(&salida.stderr);
    if let Some(mensaje) = mensaje_de_error(&errores, salida.codigo) {
        return ErrorNucleo::General { detalle: mensaje };
    }
    ErrorNucleo::General {
        detalle: match salida.codigo {
            Some(codigo) => format!("el host devolvió el código {codigo}"),
            None => "el host cerró el canal sin decir cómo terminó".to_string(),
        },
    }
}

/// Traduce el agotamiento del plazo de una orden en «no responde», que es lo
/// que pinta la app (§7.2).
fn traducir_plazo(error: ErrorNucleo) -> ErrorNucleo {
    match &error {
        ErrorNucleo::General { detalle } if detalle.contains("plazo") => ErrorNucleo::NoResponde,
        _ => error,
    }
}

// ===========================================================================
// Toda la flota (informe §4.2 y §5.1): sin abrir conexiones
// ===========================================================================

/// Fallos de todos los hosts según el último sondeo del móvil, con su hora, y
/// los hosts SSH con su estado (informe §5.1 y §6.4). No abre conexiones.
pub fn toda_la_flota(estado: &EstadoNucleo) -> Result<TodaLaFlota, ErrorNucleo> {
    let hosts = crate::flota::hosts_flota(estado, None)?;
    let fallos = con_almacen(estado, |almacen| -> anyhow::Result<Vec<FilaTodaLaFlota>> {
        let mut salida = Vec::new();
        for fila in movil_bd::hosts(almacen.conexion())? {
            if fila.uuid.is_empty() || !fila.host.tipo.es_ssh() {
                continue;
            }
            let Some(sondeo) = almacen.ultimo_sondeo(fila.host.id)? else {
                continue;
            };
            let sondeo_en = crate::inventario::fecha_a_epoca(&sondeo.fecha);
            let Some(fallos) = sondeo.fallos.as_ref() else {
                continue;
            };
            for fallo in fallos.lista() {
                salida.push(FilaTodaLaFlota {
                    host_uuid: fila.uuid.clone(),
                    host_nombre: fila.host.nombre.clone(),
                    fallo: FalloFlota {
                        tipo: match fallo.tipo {
                            TipoFallo::Servicio => TipoFalloMovil::Servicio,
                            TipoFallo::Contenedor => TipoFalloMovil::Contenedor,
                        },
                        nombre: fallo.nombre,
                        estado: fallo.estado,
                        motor: fallo.motor.map(motor_nombre_a_vista),
                    },
                    sondeo_en,
                });
            }
        }
        Ok(salida)
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los fallos de los sondeos: {error:#}"),
    })?;
    let hosts: Vec<HostTodaLaFlota> = hosts
        .into_iter()
        .filter(|host| host.tipo == TipoHostMovil::Ssh)
        .map(|host| HostTodaLaFlota {
            host_uuid: host.uuid,
            host_nombre: host.nombre,
            estado: host.estado,
            motivo: host.motivo,
            fallos: host.fallos,
            sondeo_en: host.ultimo_sondeo_en,
        })
        .collect();
    Ok(TodaLaFlota { fallos, hosts })
}

// ===========================================================================
// Listado de servicios (informe §4.2 y §5.1)
// ===========================================================================

/// servicios.sh listar (10 s) con los motores de los ajustes. Con
/// refresco = false (lo pide el usuario) puede abrir la conexión con diálogos;
/// con true es automática y no dialoga. Si ya hay una petición en vuelo para
/// ese host, espera a esa y devuelve su resultado (informe §5.1).
pub async fn listar(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    refresco: bool,
) -> Result<ListadoServicios, ErrorNucleo> {
    let (primero, receptor) = iniciar_solicitud(estado, host_uuid)?;
    if !primero {
        return esperar_solicitud(receptor).await;
    }
    // Si la corrutina de la app se cancela a mitad del listado, la guardia
    // publica un fallo y retira la solicitud: las que esperaban no se quedan
    // colgadas y el host se puede volver a listar.
    let mut guardia = GuardiaListado::nuevo(estado, host_uuid);
    let resultado = ejecutar_listado(estado, host_uuid, refresco).await;
    let publicable = match &resultado {
        Ok(listado) => Ok(Arc::new(listado.clone())),
        Err(error) => Err(FalloListado::desde(error)),
    };
    guardia.publicar(publicable);
    resultado
}

/// Publica el resultado de la solicitud de listado en vuelo al salir de
/// listar, también si la tarea se cancela antes de terminar.
struct GuardiaListado<'a> {
    estado: &'a EstadoNucleo,
    host_uuid: String,
    publicada: bool,
}

impl<'a> GuardiaListado<'a> {
    fn nuevo(estado: &'a EstadoNucleo, host_uuid: &str) -> Self {
        Self {
            estado,
            host_uuid: host_uuid.to_string(),
            publicada: false,
        }
    }

    fn publicar(&mut self, resultado: ResultadoListado) {
        terminar_solicitud(self.estado, &self.host_uuid, resultado);
        self.publicada = true;
    }
}

impl Drop for GuardiaListado<'_> {
    fn drop(&mut self) {
        if !self.publicada {
            terminar_solicitud(
                self.estado,
                &self.host_uuid,
                Err(FalloListado::General(
                    "la petición de listado se canceló".to_string(),
                )),
            );
        }
    }
}

async fn ejecutar_listado(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    refresco: bool,
) -> Result<ListadoServicios, ErrorNucleo> {
    let host = host_ssh(estado, host_uuid)?;
    let uso = if refresco {
        conexiones::conexion_automatica(estado, host_uuid).await?
    } else {
        conexiones::conexion_interactiva(estado, host_uuid).await?
    };
    let privilegio = averiguar_privilegio(estado, &uso, host_uuid).await?;
    let suborden = nucleo_orden::Suborden::Listar {
        motores: motores_para_listar(estado),
    };
    nucleo_orden::validar_suborden(&suborden)
        .map_err(|motivo| ErrorNucleo::General { detalle: motivo })?;
    let comando = nucleo_orden::comando(&suborden, privilegio);
    let password = password_para(estado, host_uuid, false);
    let entrada = entrada_con_password(&password);
    let salida = conexiones::ejecutar_remoto(
        &uso,
        &comando,
        entrada.as_deref().map(|entrada| entrada.as_slice()),
        Duration::from_secs(limites::PLAZO_LISTADO_SEG),
        TOPE_LISTADO,
    )
    .await
    .map_err(traducir_plazo)?;
    drop(uso);
    if salida.codigo != Some(0) {
        return Err(error_de_salida(&salida));
    }
    let mut texto = String::from_utf8_lossy(&salida.stdout).to_string();
    if salida.stdout_truncada {
        // Como el servidor: se cierra el bloque para que el lector sepa que la
        // lista queda incompleta y la marque (§8 de la Fase 13).
        texto.push_str("MAGI_FIN\n");
    }
    let listado = lectura::listado(&texto, &modelo::servicios_de(&host)).map_err(|error| {
        ErrorNucleo::General {
            detalle: format!("no se pudo leer el listado de servicios: {error}"),
        }
    })?;
    guardar_privilegio(estado, host_uuid, listado.privilegio);
    let hora = Some(chrono::Utc::now().timestamp());
    let sudo = sudo_en_memoria(estado, host_uuid).is_some();
    let vista = listado_a_vista(&listado, sudo, hora);
    cachear_listado(estado, host_uuid, vista.clone());
    Ok(vista)
}

// ===========================================================================
// Consumo y detalle (informe §5.1 y §6.4)
// ===========================================================================

/// CPU y memoria de los contenedores en marcha (stats --no-stream). Solo se
/// pide con la sección Contenedores a la vista; es automática (§7.2).
pub async fn consumo(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<Vec<Consumo>, ErrorNucleo> {
    let host = host_ssh(estado, host_uuid)?;
    let listado =
        listado_cacheado(estado, host_uuid).ok_or_else(|| ErrorNucleo::EstadoInvalido {
            detalle: format!(
                "abre los servicios de «{}» antes de pedir el consumo",
                host.nombre
            ),
        })?;
    let motor = match &listado.motor {
        MotorEstadoMovil::Disponible(motor) => match motor.nombre {
            MotorNombreMovil::Docker => MotorNombre::Docker,
            MotorNombreMovil::Podman => MotorNombre::Podman,
        },
        _ => {
            return Err(ErrorNucleo::EstadoInvalido {
                detalle: format!(
                    "«{}» no tiene un motor de contenedores disponible",
                    host.nombre
                ),
            })
        }
    };
    let uso = conexiones::conexion_automatica(estado, host_uuid).await?;
    let privilegio = privilegio_de(estado, &uso, host_uuid).await?;
    let suborden = nucleo_orden::Suborden::Consumo { motor };
    nucleo_orden::validar_suborden(&suborden)
        .map_err(|motivo| ErrorNucleo::General { detalle: motivo })?;
    let comando = nucleo_orden::comando(&suborden, privilegio);
    let password = password_para(estado, host_uuid, false);
    let entrada = entrada_con_password(&password);
    let salida = conexiones::ejecutar_remoto(
        &uso,
        &comando,
        entrada.as_deref().map(|entrada| entrada.as_slice()),
        Duration::from_secs(PLAZO_CONSUMO_SEG),
        TOPE_LISTADO,
    )
    .await
    .map_err(traducir_plazo)?;
    drop(uso);
    if salida.codigo != Some(0) {
        return Err(error_de_salida(&salida));
    }
    let texto = String::from_utf8_lossy(&salida.stdout);
    let consumos = lectura::consumo(&texto).map_err(|error| ErrorNucleo::General {
        detalle: format!("no se pudo leer el consumo: {error}"),
    })?;
    Ok(consumos
        .into_iter()
        .map(|consumo| Consumo {
            id: consumo.id,
            cpu_pct: consumo.cpu_pct,
            memoria_bytes: consumo.memoria_bytes,
            memoria_limite_bytes: consumo.memoria_limite_bytes,
        })
        .collect())
}

/// Propiedades, dependencias y texto de la unidad o de inspect (informe §5.1 y
/// §6.4).
pub async fn detalle(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    objetivo_vista: &ObjetivoMovil,
) -> Result<DetalleServicios, ErrorNucleo> {
    host_ssh(estado, host_uuid)?;
    let objetivo = objetivo_a_nucleo(objetivo_vista);
    nucleo_orden::validar_objetivo(&objetivo)
        .map_err(|motivo| ErrorNucleo::EstadoInvalido { detalle: motivo })?;
    let uso = conexiones::conexion_interactiva(estado, host_uuid).await?;
    let privilegio = privilegio_de(estado, &uso, host_uuid).await?;
    let suborden = nucleo_orden::Suborden::Detalle(objetivo.clone());
    nucleo_orden::validar_suborden(&suborden)
        .map_err(|motivo| ErrorNucleo::General { detalle: motivo })?;
    let comando = nucleo_orden::comando(&suborden, privilegio);
    let password = password_para(estado, host_uuid, false);
    let entrada = entrada_con_password(&password);
    let salida = conexiones::ejecutar_remoto(
        &uso,
        &comando,
        entrada.as_deref().map(|entrada| entrada.as_slice()),
        Duration::from_secs(PLAZO_DETALLE_SEG),
        TOPE_LISTADO,
    )
    .await
    .map_err(traducir_plazo)?;
    drop(uso);
    if salida.codigo != Some(0) {
        return Err(error_de_salida(&salida));
    }
    let mut texto = String::from_utf8_lossy(&salida.stdout).to_string();
    if salida.stdout_truncada {
        texto.push_str("\nMAGI_FIN\n");
    }
    let detalle = lectura::detalle(&texto, &objetivo).map_err(|error| ErrorNucleo::General {
        detalle: format!("no se pudo leer el detalle: {error}"),
    })?;
    Ok(detalle_a_vista(detalle))
}

// ===========================================================================
// Preparación de una acción (informe §4.3 y §5.1)
// ===========================================================================

/// Lo que la app necesita saber antes de confirmar o deliberar una acción: si
/// se permite en el estado actual del último listado, si delibera, si hace
/// falta la contraseña de sudo, qué comprobación se aplicará y si la unidad es
/// crítica (informe §4.3 y §5.1).
pub fn preparar_accion(
    estado: &EstadoNucleo,
    host_uuid: &str,
    objetivo_vista: &ObjetivoMovil,
    accion_vista: AccionMovil,
) -> Result<InfoAccionMovil, ErrorNucleo> {
    let host = host_ssh(estado, host_uuid)?;
    let objetivo = objetivo_a_nucleo(objetivo_vista);
    let accion = accion_a_nucleo(accion_vista);
    if nucleo_orden::validar_objetivo(&objetivo).is_err() || !accion_valida(objetivo.tipo, accion) {
        return Ok(InfoAccionMovil {
            permitida: false,
            motivo: Some("esa acción no se puede hacer sobre ese objetivo".to_string()),
            delibera: false,
            pide_sudo: false,
            comprobacion: None,
            critica: false,
            hosts_criticos: Vec::new(),
        });
    }
    let listado =
        listado_cacheado(estado, host_uuid).ok_or_else(|| ErrorNucleo::EstadoInvalido {
            detalle: format!(
                "abre los servicios de «{}» antes de actuar sobre ellos",
                host.nombre
            ),
        })?;
    let permitidas = acciones_permitidas(&listado, &objetivo);
    let permitida = permitidas.contains(&accion_vista);
    let motivo = if permitida {
        None
    } else {
        let permitidas_nucleo: Vec<Accion> = permitidas
            .iter()
            .map(|accion| accion_a_nucleo(*accion))
            .collect();
        nucleo_acciones::motivo_no_permitida(&objetivo, accion, &permitidas_nucleo)
    };
    let delibera = nucleo_acciones::exige_deliberacion(&objetivo, accion);
    let pide_sudo = listado.privilegio == PrivilegioMovil::SudoConClave
        && sudo_en_memoria(estado, host_uuid).is_none();
    let comprobacion = if nucleo_acciones::comprueba_configuracion(&objetivo, accion) {
        let propia = comprobacion_propia(estado, host.id, &objetivo);
        validadores::descripcion(&validadores::para_objetivo(&objetivo, propia.as_deref()))
    } else {
        None
    };
    let criticas: Vec<String> = limites::CRITICAS_POR_DEFECTO
        .iter()
        .map(|patron| patron.to_string())
        .collect();
    let critica = matches!(objetivo.tipo, TipoObjetivo::Unidad | TipoObjetivo::Timer)
        && nucleo_acciones::es_critica(&criticas, &objetivo.nombre);
    let hosts_criticos = if critica {
        hosts_que_van_a_traves(estado, host.id)
    } else {
        Vec::new()
    };
    Ok(InfoAccionMovil {
        permitida,
        motivo,
        delibera,
        pide_sudo,
        comprobacion,
        critica,
        hosts_criticos,
    })
}

/// ¿La acción es posible en ese tipo de objetivo? (El estado se mira con el
/// listado; esto solo comprueba la coherencia del tipo.)
fn accion_valida(tipo: TipoObjetivo, accion: Accion) -> bool {
    match tipo {
        TipoObjetivo::Timer => accion == Accion::Lanzar,
        TipoObjetivo::Unidad => accion != Accion::Lanzar,
        TipoObjetivo::Contenedor | TipoObjetivo::Proyecto => {
            matches!(accion, Accion::Arrancar | Accion::Parar | Accion::Reiniciar)
        }
    }
}

/// Las acciones permitidas del objetivo según el último listado (las calcula
/// el núcleo con el estado; la app solo ofrece estas).
fn acciones_permitidas(listado: &ListadoServicios, objetivo: &Objetivo) -> Vec<AccionMovil> {
    let acciones = match objetivo.tipo {
        TipoObjetivo::Unidad => listado
            .unidades
            .iter()
            .find(|unidad| unidad.nombre == objetivo.nombre)
            .map(|unidad| unidad.acciones.clone()),
        TipoObjetivo::Timer => listado
            .timers
            .iter()
            .find(|timer| timer.nombre == objetivo.nombre)
            .map(|timer| timer.acciones.clone()),
        TipoObjetivo::Contenedor => listado
            .contenedores
            .iter()
            .find(|contenedor| {
                contenedor.id == objetivo.nombre || contenedor.nombre == objetivo.nombre
            })
            .map(|contenedor| contenedor.acciones.clone()),
        TipoObjetivo::Proyecto => listado
            .proyectos
            .iter()
            .find(|proyecto| proyecto.nombre == objetivo.nombre)
            .map(|proyecto| proyecto.acciones.clone()),
    };
    acciones.unwrap_or_default()
}

/// La comprobación propia del objetivo en COMPROBACIONES_SERVICIO, si la hay y
/// es válida (una orden que no vale se ignora, §7.11).
fn comprobacion_propia(estado: &EstadoNucleo, host_id: i64, objetivo: &Objetivo) -> Option<String> {
    let tipo = objetivo.tipo;
    let nombre = objetivo.nombre.clone();
    let comprobacion = con_almacen(estado, move |almacen| {
        almacen.obtener_comprobacion(host_id, tipo, &nombre)
    })
    .ok()?
    .ok()?
    .map(|comprobacion| comprobacion.comando)?;
    if nucleo_orden::validar_comando_propio(&comprobacion).is_err() {
        return None;
    }
    Some(comprobacion)
}

/// La comprobación de configuración que se aplicará antes de una acción (§7.5),
/// ya en la forma que espera el script.
fn comprobacion_de(
    estado: &EstadoNucleo,
    host_id: i64,
    objetivo: &Objetivo,
    accion: Accion,
) -> nucleo_orden::Comprobacion {
    if !nucleo_acciones::comprueba_configuracion(objetivo, accion) {
        return nucleo_orden::Comprobacion::Ninguna;
    }
    let propia = comprobacion_propia(estado, host_id, objetivo);
    match validadores::para_objetivo(objetivo, propia.as_deref()) {
        validadores::Comprobacion::Ninguna => nucleo_orden::Comprobacion::Ninguna,
        validadores::Comprobacion::Conocida(_) => nucleo_orden::Comprobacion::Conocida,
        validadores::Comprobacion::Propia(comando) => nucleo_orden::Comprobacion::Propia(comando),
    }
}

/// Nombres de los hosts que van a través de este (salto_host_id), para el aviso
/// de unidad crítica (§7.9 de la Fase 13).
fn hosts_que_van_a_traves(estado: &EstadoNucleo, host_id: i64) -> Vec<String> {
    con_almacen(estado, move |almacen| -> anyhow::Result<Vec<String>> {
        Ok(almacen
            .listar_hosts()?
            .into_iter()
            .filter(|host| host.salto_host_id == Some(host_id))
            .map(|host| host.nombre)
            .collect())
    })
    .ok()
    .and_then(|resultado| resultado.ok())
    .unwrap_or_default()
}

// ===========================================================================
// Acción de servicio (informe §4.3 y §5.1)
// ===========================================================================

/// Oyente de la caída de la conexión mientras una acción está en curso: el
/// vigilante avisa y la acción acaba en error con su motivo (§7.2).
struct OyenteAccion {
    host_uuid: String,
    caida: std::sync::Mutex<Option<String>>,
}

impl OyenteAccion {
    fn nuevo(host_uuid: &str) -> Self {
        Self {
            host_uuid: host_uuid.to_string(),
            caida: std::sync::Mutex::new(None),
        }
    }

    fn motivo(&self) -> Option<String> {
        self.caida.lock().ok().and_then(|caida| caida.clone())
    }
}

impl conexiones::OyenteConexion for OyenteAccion {
    fn conexion_caida(&self, host_uuid: &str, motivo: &str) {
        if host_uuid != self.host_uuid {
            return;
        }
        if let Ok(mut caida) = self.caida.lock() {
            *caida = Some(motivo.to_string());
        }
    }
}

/// Ejecuta la acción, la anota y la difunde por
/// OyenteNucleo.accion_servicio_hecha (informe §4.3 y §5.1). El trabajo corre
/// en el runtime del núcleo y no en el hilo de la llamada: aunque Android
/// cancele la corrutina, la acción termina, se anota y llega su resultado por
/// la retrollamada (D279: las acciones siguen en segundo plano).
pub async fn accion(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    objetivo_vista: &ObjetivoMovil,
    accion_vista: AccionMovil,
    deliberacion_id: Option<u64>,
) -> Result<ResultadoServicioMovil, ErrorNucleo> {
    let host = host_ssh(estado, host_uuid)?;
    let objetivo = objetivo_a_nucleo(objetivo_vista);
    let accion = accion_a_nucleo(accion_vista);
    nucleo_orden::validar_objetivo(&objetivo)
        .map_err(|motivo| ErrorNucleo::EstadoInvalido { detalle: motivo })?;
    if !accion_valida(objetivo.tipo, accion) {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: format!("«{}» no admite {}", objetivo.nombre, accion.texto()),
        });
    }
    // Se recomprueba todo (T38): permitida, deliberación válida y una sola
    // acción por objetivo. No se confía en lo que decidió la app.
    let info = preparar_accion(estado, host_uuid, objetivo_vista, accion_vista)?;
    if !info.permitida {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: info
                .motivo
                .unwrap_or_else(|| "la acción no está permitida en este momento".to_string()),
        });
    }
    if info.pide_sudo {
        return Err(ErrorNucleo::SinPermiso);
    }
    let exige = nucleo_acciones::exige_deliberacion(&objetivo, accion);
    if exige && deliberacion_id.is_none() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: format!(
                "{} {} deja algo parado: hace falta deliberar antes",
                accion.texto(),
                objetivo.nombre
            ),
        });
    }
    let id_accion = estado.siguiente_id();
    let fase_inicial = if comprobacion_de(estado, host.id, &objetivo, accion)
        == nucleo_orden::Comprobacion::Ninguna
    {
        FaseAccionMovil::Ejecutando
    } else {
        FaseAccionMovil::Comprobando
    };
    let en_curso = AccionEnCurso {
        id: id_accion,
        host_uuid: host_uuid.to_string(),
        host_nombre: host.nombre.clone(),
        objetivo: objetivo_vista.clone(),
        accion: accion_vista,
        fase: fase_inicial,
        deliberacion_id,
        desde_ms: chrono::Utc::now().timestamp_millis(),
    };
    {
        let mut acciones =
            estado
                .acciones_servicio_en_curso
                .lock()
                .map_err(|_| ErrorNucleo::General {
                    detalle: "estado de acciones en curso envenenado".to_string(),
                })?;
        if acciones
            .values()
            .any(|otra| otra.host_uuid == host_uuid && otra.objetivo == *objetivo_vista)
        {
            return Err(ErrorNucleo::EstadoInvalido {
                detalle: format!(
                    "hay otra acción en curso sobre {} {}",
                    objetivo_vista.tipo.texto(),
                    objetivo_vista.nombre
                ),
            });
        }
        acciones.insert(id_accion, en_curso);
    }
    // La deliberación se consume DESPUÉS de ocupar el turno: si ya hay otra
    // acción en curso, la deliberación no se quema (T62). Una deliberación
    // autoriza una sola acción: existe, es de esta acción y no está usada.
    if let Some(id) = deliberacion_id {
        let plan = PlanDeliberadoMovil::Servicio {
            host: host_uuid.to_string(),
            objetivo: objetivo_vista.clone(),
            accion: accion_vista,
            comprobacion: info.comprobacion.clone(),
        };
        if let Err(error) = crate::deliberacion::consumir(estado, id, &plan) {
            if let Ok(mut acciones) = estado.acciones_servicio_en_curso.lock() {
                acciones.remove(&id_accion);
            }
            return Err(error);
        }
    }
    // La acción en curso se difunde antes de su efecto (T40).
    emitir_acciones(estado);
    let oyente = Arc::new(OyenteAccion::nuevo(host_uuid));
    let oyente_trait: Arc<dyn conexiones::OyenteConexion> = oyente.clone();
    estado.suscribir_oyente_conexion(oyente_trait.clone());
    let (emisor, receptor) = tokio::sync::oneshot::channel();
    let estado_tarea = estado.clone();
    let host_tarea = host.clone();
    let host_uuid_tarea = host_uuid.to_string();
    let objetivo_tarea = objetivo.clone();
    let objetivo_vista_tarea = objetivo_vista.clone();
    let info_tarea = info.clone();
    let oyente_tarea = oyente.clone();
    let oyente_trait_tarea = oyente_trait;
    estado.runtime.spawn(async move {
        let desde = Instant::now();
        let resultado = ejecutar_accion(
            &estado_tarea,
            &host_tarea,
            &host_uuid_tarea,
            &objetivo_tarea,
            accion,
            &info_tarea,
            id_accion,
            &oyente_tarea,
        )
        .await;
        estado_tarea.quitar_oyente_conexion(&oyente_trait_tarea);
        // Si el host rechazó la contraseña de sudo, se olvida y la siguiente
        // acción la vuelve a pedir (§6.5 de la Fase 13).
        if resultado.mensaje.contains("sudo rechazó la contraseña") {
            let _ = sudo_olvidar(&estado_tarea, &host_uuid_tarea);
        }
        // REGISTRO y deliberación: siempre, también si falla o la para la
        // comprobación (§4.3).
        anotar_accion(
            &estado_tarea,
            host_tarea.id,
            &host_tarea.nombre,
            &objetivo_vista_tarea,
            accion_vista,
            deliberacion_id,
            &resultado,
            desde.elapsed(),
        );
        if let Some(id) = deliberacion_id {
            let desenlace = if resultado.resultado == ResultadoAccionMovil::Hecho {
                EjecucionResultado::Ok
            } else {
                EjecucionResultado::Error
            };
            let _ = crate::deliberacion::registrar_resultado(&estado_tarea, id, desenlace);
        }
        if let Ok(mut acciones) = estado_tarea.acciones_servicio_en_curso.lock() {
            acciones.remove(&id_accion);
        }
        emitir_acciones(&estado_tarea);
        // El resultado llega aunque la pantalla que la pidió ya no esté (§5.2).
        estado_tarea
            .retrollamadas
            .oyente
            .accion_servicio_hecha(resultado.clone());
        let _ = emisor.send(resultado);
    });
    match receptor.await {
        Ok(resultado) => Ok(resultado),
        Err(_) => Err(ErrorNucleo::Cancelado),
    }
}

/// Cuerpo de la acción: toma la conexión, ejecuta servicios.sh accion siguiendo
/// sus fases en vivo y deja el resultado.
#[allow(clippy::too_many_arguments)]
async fn ejecutar_accion(
    estado: &Arc<EstadoNucleo>,
    host: &Host,
    host_uuid: &str,
    objetivo: &Objetivo,
    accion: Accion,
    info: &InfoAccionMovil,
    id_accion: u64,
    oyente: &Arc<OyenteAccion>,
) -> ResultadoServicioMovil {
    let uso = match conexiones::conexion_interactiva(estado, host_uuid).await {
        Ok(uso) => uso,
        Err(error) => {
            return resultado_error(
                host_uuid,
                &host.nombre,
                objetivo,
                accion,
                info,
                error.to_string(),
            )
        }
    };
    let privilegio = match privilegio_de(estado, &uso, host_uuid).await {
        Ok(privilegio) => privilegio,
        Err(error) => {
            return resultado_error(
                host_uuid,
                &host.nombre,
                objetivo,
                accion,
                info,
                error.to_string(),
            )
        }
    };
    let espera_seg = crate::ajustes::actuales(estado).servicios_espera_seg.clamp(
        crate::ajustes::SERVICIOS_ESPERA_SEG_MIN,
        crate::ajustes::SERVICIOS_ESPERA_SEG_MAX,
    );
    let suborden = nucleo_orden::Suborden::Accion {
        objetivo: objetivo.clone(),
        accion,
        espera_seg: espera_seg as u32,
        comprobacion: comprobacion_de(estado, host.id, objetivo, accion),
    };
    if let Err(motivo) = nucleo_orden::validar_suborden(&suborden) {
        return resultado_error(host_uuid, &host.nombre, objetivo, accion, info, motivo);
    }
    let comando = nucleo_orden::comando(&suborden, privilegio);
    // La contraseña de sudo de una acción renueva su vida (D260 de la F13).
    let password = password_para(estado, host_uuid, true);
    let entrada = entrada_con_password(&password);
    let mut canal = match conexiones::abrir_canal_remoto(&uso, &comando).await {
        Ok(canal) => canal,
        Err(error) => {
            return resultado_error(
                host_uuid,
                &host.nombre,
                objetivo,
                accion,
                info,
                error.to_string(),
            )
        }
    };
    if let Some(entrada) = entrada.as_ref() {
        let _ = canal.escribir(entrada.as_slice()).await;
    }
    // El script solo lee la contraseña; cerrar la entrada evita que ninguna
    // orden se quede esperando por la entrada estándar.
    let _ = canal.cerrar_entrada().await;
    // El plazo total de una acción es espera_seg + 30 s e incluye la
    // comprobación de configuración (que dentro del script tiene sus 20 s).
    let limite = Instant::now() + Duration::from_secs(espera_seg + limites::MARGEN_ACCION_SEG);
    let mut salida = SalidaRemota::default();
    let mut pendiente: Vec<u8> = Vec::new();
    let mut agotado = false;
    loop {
        let restante = limite.saturating_duration_since(Instant::now());
        if restante.is_zero() {
            agotado = true;
            break;
        }
        match tokio::time::timeout(restante, canal.eventos.recv()).await {
            Err(_) => {
                agotado = true;
                break;
            }
            Ok(None) => break,
            Ok(Some(EventoCanal::Stdout(datos))) => {
                salida.stdout_truncada |= acumular(&mut salida.stdout, &datos, TOPE_ACCION);
                // Las fases que canta el script se difunden en vivo.
                for linea in extraer_lineas(&mut pendiente, &datos) {
                    if let Some(fase) = lectura::fase(&linea) {
                        actualizar_fase(estado, id_accion, fase_a_vista(fase));
                    }
                }
            }
            Ok(Some(EventoCanal::Stderr(datos))) => {
                salida.stderr_truncada |= acumular(&mut salida.stderr, &datos, TOPE_ACCION);
            }
            Ok(Some(EventoCanal::Codigo(codigo))) => salida.codigo = Some(codigo),
            Ok(Some(EventoCanal::Fin)) => break,
        }
    }
    canal.cerrar();
    drop(uso);
    interpretar_accion(
        host_uuid,
        &host.nombre,
        objetivo,
        accion,
        info,
        salida,
        oyente.motivo(),
        agotado,
    )
}

/// Construye un resultado de error con su mensaje.
fn resultado_error(
    host_uuid: &str,
    host_nombre: &str,
    objetivo: &Objetivo,
    accion: Accion,
    info: &InfoAccionMovil,
    mensaje: String,
) -> ResultadoServicioMovil {
    ResultadoServicioMovil {
        host_uuid: host_uuid.to_string(),
        host_nombre: host_nombre.to_string(),
        objetivo: objetivo_a_vista(objetivo),
        accion: accion_a_vista(accion),
        resultado: ResultadoAccionMovil::Error,
        estado: String::new(),
        segundos: 0.0,
        mensaje,
        lineas: Vec::new(),
        comprobacion: info.comprobacion.clone(),
    }
}

/// Interpreta la salida de servicios.sh accion (§7.4) y la convierte en el
/// resultado que se enseña y se anota.
#[allow(clippy::too_many_arguments)]
fn interpretar_accion(
    host_uuid: &str,
    host_nombre: &str,
    objetivo: &Objetivo,
    accion: Accion,
    info: &InfoAccionMovil,
    salida: SalidaRemota,
    caida: Option<String>,
    agotado: bool,
) -> ResultadoServicioMovil {
    let errores = String::from_utf8_lossy(&salida.stderr).to_string();
    let texto = String::from_utf8_lossy(&salida.stdout).to_string();
    let mut resultado = match lectura::accion(&texto, objetivo) {
        Ok(mut leido) => {
            leido.comprobacion = info.comprobacion.clone();
            leido
        }
        Err(error_lectura) => {
            let mensaje = if let Some(motivo) = &caida {
                format!(
                    "se cortó la conexión: {motivo}. Puede que la orden siguiera en el host; refresca para ver el estado."
                )
            } else if agotado {
                format!("sin respuesta de «{host_nombre}»; la orden puede seguir en el host")
            } else if salida.stdout_truncada {
                format!(
                    "la salida del host pasó de {} MiB y no se pudo leer el resultado",
                    TOPE_ACCION / (1024 * 1024)
                )
            } else if let Some(mensaje) = mensaje_de_error(&errores, salida.codigo) {
                mensaje
            } else {
                format!("no se pudo leer el resultado: {error_lectura}")
            };
            ResultadoServicio {
                resultado: ResultadoAccion::Error,
                mensaje,
                ..ResultadoServicio::default()
            }
        }
    };
    // Una contraseña de sudo rechazada se olvida y se dice qué hacer (§6.5 de
    // la Fase 13).
    if resultado.resultado != ResultadoAccion::Hecho && sudo_rechazada(&resultado) {
        resultado.mensaje = ErrorNucleo::SudoRechazada.to_string();
    }
    let resultado_vista = match resultado.resultado {
        ResultadoAccion::Hecho => ResultadoAccionMovil::Hecho,
        ResultadoAccion::Fallido => ResultadoAccionMovil::Fallido,
        ResultadoAccion::SinConfirmar => ResultadoAccionMovil::SinConfirmar,
        ResultadoAccion::RechazadaConfiguracion => ResultadoAccionMovil::RechazadaConfiguracion,
        ResultadoAccion::Error => ResultadoAccionMovil::Error,
    };
    ResultadoServicioMovil {
        host_uuid: host_uuid.to_string(),
        host_nombre: host_nombre.to_string(),
        objetivo: objetivo_a_vista(objetivo),
        accion: accion_a_vista(accion),
        resultado: resultado_vista,
        estado: resultado.estado,
        segundos: resultado.segundos,
        mensaje: resultado.mensaje,
        lineas: resultado.lineas,
        comprobacion: info.comprobacion.clone(),
    }
}

/// ¿El fallo fue porque sudo rechazó la contraseña? (§6.5 de la Fase 13) En ese
/// caso la memoria se olvida en el llamador.
fn sudo_rechazada(resultado: &ResultadoServicio) -> bool {
    let mut texto = resultado.mensaje.clone();
    for linea in &resultado.lineas {
        texto.push('\n');
        texto.push_str(linea);
    }
    let minusculas = texto.to_lowercase();
    minusculas.contains("incorrect password")
        || minusculas.contains("sorry, try again")
        || minusculas.contains("contraseña incorrecta")
        || minusculas.contains("authentication failure")
        || minusculas.contains("no password was provided")
}

/// Anota la acción terminada en REGISTRO (§3.6 de la Fase 13): objetivo,
/// acción, resultado, duración y deliberación; nunca la salida de las órdenes
/// (T44).
#[allow(clippy::too_many_arguments)]
fn anotar_accion(
    estado: &EstadoNucleo,
    host_id: i64,
    host_nombre: &str,
    objetivo: &ObjetivoMovil,
    accion: AccionMovil,
    deliberacion_id: Option<u64>,
    resultado: &ResultadoServicioMovil,
    duracion: Duration,
) {
    let duracion = format!("{:.1}", duracion.as_secs_f32()).replace('.', ",");
    let mut detalle = format!(
        "{} · {} {} · {} · {} · {} s",
        host_nombre,
        objetivo.tipo.texto(),
        objetivo.nombre,
        accion.texto(),
        resultado.resultado.texto(),
        duracion
    );
    if let Some(id) = deliberacion_id {
        detalle.push_str(&format!(" · deliberación #{id}"));
    }
    let resultado_registro = if resultado.resultado == ResultadoAccionMovil::Hecho {
        ResultadoRegistro::Ok
    } else {
        ResultadoRegistro::Error
    };
    let _ = estado.escritor.con_almacen(move |almacen| {
        registro::anotar(
            almacen.conexion(),
            registro::SERVICIO_ACCION,
            Some(host_id),
            None,
            &detalle,
            resultado_registro,
        )
    });
}

/// Saca del búfer las líneas completas que haya y deja el resto para el
/// siguiente trozo. Una línea sin fin no puede crecer sin límite (§8).
pub(crate) fn extraer_lineas(pendiente: &mut Vec<u8>, datos: &[u8]) -> Vec<String> {
    pendiente.extend_from_slice(datos);
    let mut lineas = Vec::new();
    while let Some(posicion) = pendiente.iter().position(|byte| *byte == b'\n') {
        let linea: Vec<u8> = pendiente.drain(..=posicion).collect();
        lineas.push(String::from_utf8_lossy(&linea[..linea.len().saturating_sub(1)]).into_owned());
    }
    if pendiente.len() > TOPE_ACCION {
        pendiente.clear();
    }
    lineas
}

/// Acumula datos en un flujo con tope, sin pasar nunca de él. Devuelve si hubo
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

// ===========================================================================
// Sudo (informe §5.1 y §7.5)
// ===========================================================================

/// Comprueba la contraseña de sudo (servicios.sh sudo): ok (queda en memoria
/// hasta caducar), rechazada o sin_sudo.
pub async fn sudo_comprobar(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    contrasena: Vec<u8>,
) -> Result<ResultadoSudo, ErrorNucleo> {
    host_ssh(estado, host_uuid)?;
    let contrasena = Zeroizing::new(contrasena);
    let texto = std::str::from_utf8(&contrasena).map_err(|_| ErrorNucleo::EstadoInvalido {
        detalle: "la contraseña de sudo no es texto UTF-8".to_string(),
    })?;
    if texto.contains('\n') || texto.contains('\r') {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "la contraseña de sudo no puede tener saltos de línea".to_string(),
        });
    }
    let clave = Zeroizing::new(texto.to_string());
    let uso = conexiones::conexion_interactiva(estado, host_uuid).await?;
    let suborden = nucleo_orden::Suborden::Sudo;
    let comando = nucleo_orden::comando(&suborden, Privilegio::SinSudo);
    let entrada = entrada_con_password(&Some(clave.clone()));
    let salida = conexiones::ejecutar_remoto(
        &uso,
        &comando,
        entrada.as_deref().map(|entrada| entrada.as_slice()),
        Duration::from_secs(PLAZO_SUDO_SEG),
        TOPE_SUDO,
    )
    .await
    .map_err(traducir_plazo)?;
    drop(uso);
    if salida.codigo != Some(0) {
        return Err(error_de_salida(&salida));
    }
    let texto_salida = String::from_utf8_lossy(&salida.stdout);
    let comprobacion = lectura::sudo(&texto_salida).map_err(|error| ErrorNucleo::General {
        detalle: format!("no se pudo leer la comprobación de sudo: {error}"),
    })?;
    match comprobacion {
        lectura::ComprobacionSudo::Ok => {
            if let Ok(mut memoria) = estado.sudo_en_memoria.lock() {
                memoria.guardar(host_uuid, clave);
            }
            Ok(ResultadoSudo::Ok)
        }
        lectura::ComprobacionSudo::Rechazada => {
            let _ = sudo_olvidar(estado, host_uuid);
            Ok(ResultadoSudo::Rechazada)
        }
        lectura::ComprobacionSudo::SinSudo => {
            let _ = sudo_olvidar(estado, host_uuid);
            Ok(ResultadoSudo::SinSudo)
        }
    }
}

// ===========================================================================
// Sesiones con orden (informe §5.1 y D286)
// ===========================================================================

/// Abre una sesión con PTY para una OrdenPestana del núcleo (informe §5.1): la
/// orden la arma magi-movil desde el tipo cerrado, la sesión se nombra
/// «host · objetivo (shell/logs)» y no lleva snippet al conectar (D286).
pub async fn abrir_sesion_con_orden(
    estado: &Arc<EstadoNucleo>,
    host_uuid: &str,
    orden_vista: OrdenPestanaMovil,
) -> Result<u64, ErrorNucleo> {
    let host = host_ssh(estado, host_uuid)?;
    let orden = orden_pestana_a_nucleo(orden_vista);
    let privilegio = match privilegio_cacheado(estado, host_uuid) {
        Some(privilegio) => privilegio,
        None => {
            let uso = conexiones::conexion_interactiva(estado, host_uuid).await?;
            let privilegio = privilegio_de(estado, &uso, host_uuid).await?;
            drop(uso);
            privilegio
        }
    };
    let comando = nucleo_orden::comando_pestana(&orden, privilegio).ok_or_else(|| {
        ErrorNucleo::EstadoInvalido {
            detalle: "el nombre del objetivo no se puede usar en una orden".to_string(),
        }
    })?;
    let (objetivo, sufijo) = match &orden {
        OrdenPestana::ShellContenedor { id, .. } => (id.clone(), "shell"),
        OrdenPestana::SeguirDiario { unidad } => (unidad.clone(), "logs"),
        OrdenPestana::SeguirContenedor { id, .. } => (id.clone(), "logs"),
    };
    let nombre = format!("{} · {} ({sufijo})", host.nombre, objetivo);
    crate::gestor::abrir_sesion_con_orden(estado, host_uuid, nombre, comando).await
}

fn orden_pestana_a_nucleo(orden: OrdenPestanaMovil) -> OrdenPestana {
    match orden {
        OrdenPestanaMovil::ShellContenedor { motor, id } => OrdenPestana::ShellContenedor {
            motor: motor_nombre_a_nucleo(motor),
            id,
        },
        OrdenPestanaMovil::SeguirDiario { unidad } => OrdenPestana::SeguirDiario { unidad },
        OrdenPestanaMovil::SeguirContenedor { motor, id } => OrdenPestana::SeguirContenedor {
            motor: motor_nombre_a_nucleo(motor),
            id,
        },
    }
}

// ===========================================================================
// Conversiones entre el núcleo y el contrato UniFFI
// ===========================================================================

pub(crate) fn tipo_objetivo_a_nucleo(tipo: TipoObjetivoMovil) -> TipoObjetivo {
    match tipo {
        TipoObjetivoMovil::Unidad => TipoObjetivo::Unidad,
        TipoObjetivoMovil::Timer => TipoObjetivo::Timer,
        TipoObjetivoMovil::Contenedor => TipoObjetivo::Contenedor,
        TipoObjetivoMovil::Proyecto => TipoObjetivo::Proyecto,
    }
}

pub(crate) fn tipo_objetivo_a_vista(tipo: TipoObjetivo) -> TipoObjetivoMovil {
    match tipo {
        TipoObjetivo::Unidad => TipoObjetivoMovil::Unidad,
        TipoObjetivo::Timer => TipoObjetivoMovil::Timer,
        TipoObjetivo::Contenedor => TipoObjetivoMovil::Contenedor,
        TipoObjetivo::Proyecto => TipoObjetivoMovil::Proyecto,
    }
}

pub(crate) fn objetivo_a_nucleo(objetivo: &ObjetivoMovil) -> Objetivo {
    Objetivo {
        tipo: tipo_objetivo_a_nucleo(objetivo.tipo),
        nombre: objetivo.nombre.clone(),
    }
}

pub(crate) fn objetivo_a_vista(objetivo: &Objetivo) -> ObjetivoMovil {
    ObjetivoMovil {
        tipo: tipo_objetivo_a_vista(objetivo.tipo),
        nombre: objetivo.nombre.clone(),
    }
}

pub(crate) fn accion_a_nucleo(accion: AccionMovil) -> Accion {
    match accion {
        AccionMovil::Arrancar => Accion::Arrancar,
        AccionMovil::Parar => Accion::Parar,
        AccionMovil::Reiniciar => Accion::Reiniciar,
        AccionMovil::Recargar => Accion::Recargar,
        AccionMovil::Activar => Accion::Activar,
        AccionMovil::Desactivar => Accion::Desactivar,
        AccionMovil::Lanzar => Accion::Lanzar,
    }
}

pub(crate) fn accion_a_vista(accion: Accion) -> AccionMovil {
    match accion {
        Accion::Arrancar => AccionMovil::Arrancar,
        Accion::Parar => AccionMovil::Parar,
        Accion::Reiniciar => AccionMovil::Reiniciar,
        Accion::Recargar => AccionMovil::Recargar,
        Accion::Activar => AccionMovil::Activar,
        Accion::Desactivar => AccionMovil::Desactivar,
        Accion::Lanzar => AccionMovil::Lanzar,
    }
}

pub(crate) fn motor_nombre_a_vista(motor: MotorNombre) -> MotorNombreMovil {
    match motor {
        MotorNombre::Docker => MotorNombreMovil::Docker,
        MotorNombre::Podman => MotorNombreMovil::Podman,
    }
}

fn motor_nombre_a_nucleo(motor: MotorNombreMovil) -> MotorNombre {
    match motor {
        MotorNombreMovil::Docker => MotorNombre::Docker,
        MotorNombreMovil::Podman => MotorNombre::Podman,
    }
}

fn sistema_a_vista(sistema: &Systemd) -> SystemdMovil {
    match sistema {
        Systemd::Disponible(version) => SystemdMovil::Disponible(version.clone()),
        Systemd::Ausente => SystemdMovil::Ausente,
        Systemd::Antiguo(version) => SystemdMovil::Antiguo(version.clone()),
    }
}

fn privilegio_a_vista(privilegio: Privilegio) -> PrivilegioMovil {
    match privilegio {
        Privilegio::Root => PrivilegioMovil::Root,
        Privilegio::SudoSinClave => PrivilegioMovil::SudoSinClave,
        Privilegio::SudoConClave => PrivilegioMovil::SudoConClave,
        Privilegio::SinSudo => PrivilegioMovil::SinSudo,
    }
}

fn permiso_diario_a_vista(permiso: PermisoDiario) -> PermisoDiarioMovil {
    match permiso {
        PermisoDiario::Todo => PermisoDiarioMovil::Todo,
        PermisoDiario::Propio => PermisoDiarioMovil::Propio,
    }
}

fn motor_estado_a_vista(motor: &MotorEstado) -> MotorEstadoMovil {
    match motor {
        MotorEstado::No => MotorEstadoMovil::No,
        MotorEstado::SinPermiso => MotorEstadoMovil::SinPermiso,
        MotorEstado::Disponible(motor) => MotorEstadoMovil::Disponible(MotorMovil {
            nombre: motor_nombre_a_vista(motor.nombre),
            version: motor.version.clone(),
        }),
    }
}

fn estado_arranque_a_vista(estado: EstadoArranque) -> EstadoArranqueMovil {
    match estado {
        EstadoArranque::Activado => EstadoArranqueMovil::Activado,
        EstadoArranque::Desactivado => EstadoArranqueMovil::Desactivado,
        EstadoArranque::Estatico => EstadoArranqueMovil::Estatico,
        EstadoArranque::Indirecto => EstadoArranqueMovil::Indirecto,
        EstadoArranque::Generado => EstadoArranqueMovil::Generado,
        EstadoArranque::Transitorio => EstadoArranqueMovil::Transitorio,
        EstadoArranque::Alias => EstadoArranqueMovil::Alias,
        EstadoArranque::Enmascarado => EstadoArranqueMovil::Enmascarado,
        EstadoArranque::Desconocido => EstadoArranqueMovil::Desconocido,
    }
}

fn estado_contenedor_a_vista(estado: EstadoContenedor) -> EstadoContenedorMovil {
    match estado {
        EstadoContenedor::Creado => EstadoContenedorMovil::Creado,
        EstadoContenedor::EnMarcha => EstadoContenedorMovil::EnMarcha,
        EstadoContenedor::Parado => EstadoContenedorMovil::Parado,
        EstadoContenedor::Reiniciando => EstadoContenedorMovil::Reiniciando,
        EstadoContenedor::Pausado => EstadoContenedorMovil::Pausado,
        EstadoContenedor::Muerto => EstadoContenedorMovil::Muerto,
        EstadoContenedor::Desconocido => EstadoContenedorMovil::Desconocido,
    }
}

fn salud_a_vista(salud: Salud) -> SaludMovil {
    match salud {
        Salud::Ninguna => SaludMovil::Ninguna,
        Salud::Sana => SaludMovil::Sana,
        Salud::Insegura => SaludMovil::Insegura,
        Salud::Comprobando => SaludMovil::Comprobando,
    }
}

fn fase_a_vista(fase: FaseAccion) -> FaseAccionMovil {
    match fase {
        FaseAccion::Comprobando => FaseAccionMovil::Comprobando,
        FaseAccion::Ejecutando => FaseAccionMovil::Ejecutando,
        FaseAccion::Esperando => FaseAccionMovil::Esperando,
    }
}

/// El valor de una propiedad de la vista por su clave (vacío si no está).
fn propiedad<'a>(props: &'a [PropiedadMovil], clave: &str) -> &'a str {
    props
        .iter()
        .find(|propiedad| propiedad.clave == clave)
        .map(|propiedad| propiedad.valor.as_str())
        .unwrap_or("")
}

fn props_a_vista(props: &BTreeMap<String, String>) -> Vec<PropiedadMovil> {
    props
        .iter()
        .map(|(clave, valor)| PropiedadMovil {
            clave: clave.clone(),
            valor: valor.clone(),
        })
        .collect()
}

/// El listado del núcleo al contrato de la app, con las acciones ya calculadas,
/// la contraseña de sudo en memoria y la hora de la lectura.
fn listado_a_vista(
    listado: &Listado,
    sudo_en_memoria: bool,
    hora: Option<i64>,
) -> ListadoServicios {
    let reloj = listado.reloj;
    // Orden de la F13 (§7.1): fallidas primero, luego las vigiladas (★) y
    // después el resto; dentro de cada grupo, por nombre. La app dibuja una
    // línea entre las de arriba y el resto.
    let mut unidades: Vec<UnidadMovil> = listado
        .unidades
        .iter()
        .map(|unidad| UnidadMovil {
            nombre: unidad.nombre.clone(),
            props: props_a_vista(&unidad.props),
            vigilada: unidad.vigilada,
            acciones: nucleo_acciones::acciones_de_unidad(unidad)
                .into_iter()
                .map(accion_a_vista)
                .collect(),
            arranque: estado_arranque_a_vista(unidad.arranque()),
            desde_en: unidad.desde_epoch(reloj),
            pid: unidad.pid_principal(),
            memoria_bytes: unidad.memoria_bytes(),
            tareas: unidad
                .prop("TasksCurrent")
                .parse::<u64>()
                .ok()
                .and_then(|tareas| u32::try_from(tareas).ok()),
        })
        .collect();
    unidades.sort_by(|a, b| {
        let grupo = |unidad: &UnidadMovil| -> u8 {
            let activo = propiedad(&unidad.props, "ActiveState");
            if activo == "failed" {
                0
            } else if unidad.vigilada {
                1
            } else {
                2
            }
        };
        grupo(a)
            .cmp(&grupo(b))
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    let timers: Vec<TimerMovil> = listado
        .timers
        .iter()
        .map(|timer| {
            let servicio = timer.servicio_activado();
            let estado_servicio = servicio.as_ref().and_then(|nombre| {
                listado
                    .unidades
                    .iter()
                    .find(|unidad| &unidad.nombre == nombre)
                    .map(|unidad| unidad.activo())
            });
            TimerMovil {
                nombre: timer.nombre.clone(),
                props: props_a_vista(&timer.props),
                servicio,
                acciones: nucleo_acciones::acciones_de_timer(timer, estado_servicio)
                    .into_iter()
                    .map(accion_a_vista)
                    .collect(),
                proxima_en: timer.proxima_epoch(),
                ultima_en: timer.ultima_epoch(reloj),
            }
        })
        .collect();
    let mut timers = timers;
    // Los timers, como las unidades: fallidos primero y luego por nombre.
    timers.sort_by(|a, b| {
        let fallido = |timer: &TimerMovil| propiedad(&timer.props, "ActiveState") == "failed";
        fallido(b)
            .cmp(&fallido(a))
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    let contenedores: Vec<ContenedorMovil> = listado
        .contenedores
        .iter()
        .map(|contenedor| ContenedorMovil {
            id: contenedor.id.clone(),
            nombre: contenedor.nombre.clone(),
            imagen: contenedor.imagen.clone(),
            estado: estado_contenedor_a_vista(contenedor.estado),
            codigo_salida: contenedor.codigo_salida,
            salud: salud_a_vista(contenedor.salud),
            puertos: contenedor.puertos.clone(),
            proyecto: contenedor.proyecto.clone(),
            ficheros: contenedor.ficheros.clone(),
            servicio: contenedor.servicio.clone(),
            orden: contenedor.orden.clone(),
            etiquetas: props_a_vista(&contenedor.etiquetas),
            acciones: nucleo_acciones::acciones_de_contenedor(contenedor)
                .into_iter()
                .map(accion_a_vista)
                .collect(),
        })
        .collect();
    let proyectos: Vec<ProyectoMovil> = listado
        .proyectos
        .iter()
        .map(|proyecto| {
            let del_proyecto: Vec<magi_nucleo::servicios::Contenedor> = listado
                .contenedores
                .iter()
                .filter(|contenedor| contenedor.proyecto.as_deref() == Some(&proyecto.nombre))
                .cloned()
                .collect();
            ProyectoMovil {
                nombre: proyecto.nombre.clone(),
                ficheros: proyecto.ficheros.clone(),
                acciones: nucleo_acciones::acciones_de_proyecto(&del_proyecto)
                    .into_iter()
                    .map(accion_a_vista)
                    .collect(),
            }
        })
        .collect();
    ListadoServicios {
        systemd: sistema_a_vista(&listado.systemd),
        privilegio: privilegio_a_vista(listado.privilegio),
        diario: permiso_diario_a_vista(listado.diario),
        reloj: RelojHostMovil {
            epoch: listado.reloj.epoch,
            uptime_seg: listado.reloj.uptime_seg,
        },
        motor: motor_estado_a_vista(&listado.motor),
        unidades,
        timers,
        contenedores,
        proyectos,
        avisos: listado.avisos.clone(),
        incompleto: listado.incompleto,
        sudo_en_memoria,
        hora,
    }
}

/// El detalle del núcleo al contrato de la app.
fn detalle_a_vista(detalle: magi_nucleo::servicios::Detalle) -> DetalleServicios {
    let contenedor = detalle
        .contenedor
        .as_ref()
        .map(|contenedor| DetalleContenedorMovil {
            id: contenedor.id.clone(),
            nombre: contenedor.nombre.clone(),
            imagen: contenedor.imagen.clone(),
            creado: contenedor.creado.clone(),
            estado: estado_contenedor_a_vista(contenedor.estado),
            codigo_salida: contenedor.codigo_salida,
            reinicios: contenedor.reinicios,
            salud: salud_a_vista(contenedor.salud),
            puertos: contenedor.puertos.clone(),
            montajes: contenedor.montajes.clone(),
            redes: contenedor.redes.clone(),
            proyecto: contenedor.proyecto.clone(),
            orden: contenedor.orden.clone(),
            texto: contenedor.texto.clone(),
        });
    DetalleServicios {
        objetivo: objetivo_a_vista(&detalle.objetivo),
        props: props_a_vista(&detalle.props),
        necesita: detalle.necesita,
        necesitada: detalle.necesitada,
        texto: detalle.texto,
        contenedor,
    }
}

// ===========================================================================
// Pruebas
// ===========================================================================

#[cfg(test)]
mod pruebas {
    use super::*;

    fn unidad(nombre: &str, props: &[(&str, &str)]) -> magi_nucleo::servicios::Unidad {
        magi_nucleo::servicios::Unidad {
            nombre: nombre.to_string(),
            props: props
                .iter()
                .map(|(clave, valor)| (clave.to_string(), valor.to_string()))
                .collect(),
            vigilada: false,
        }
    }

    fn listado_vacio() -> Listado {
        Listado::vacio(Privilegio::SudoConClave)
    }

    #[test]
    fn el_sudo_de_un_host_se_guarda_y_solo_lo_renuevan_las_acciones() {
        let mut memoria = SudoEnMemoria::nuevo();
        assert!(memoria.vigente("web-01", 5).is_none());
        memoria.guardar("web-01", Zeroizing::new("secreta".to_string()));
        assert_eq!(
            memoria.vigente("web-01", 5).map(|clave| clave.to_string()),
            Some("secreta".to_string())
        );
        // Otro host no la ve.
        assert!(memoria.vigente("web-02", 5).is_none());
        // Un uso de refresco no la renueva; una acción sí (se comprueba que el
        // último uso avanza sin esperar cinco minutos).
        if let Some(clave) = memoria.claves.get_mut("web-01") {
            clave.ultimo_uso = Instant::now() - Duration::from_secs(4 * 60);
        }
        let antes = memoria
            .claves
            .get("web-01")
            .map(|clave| clave.ultimo_uso)
            .expect("guardada");
        assert!(memoria.vigente("web-01", 5).is_some());
        let despues = memoria
            .claves
            .get("web-01")
            .map(|clave| clave.ultimo_uso)
            .expect("guardada");
        assert_eq!(antes, despues, "vigente no renueva");
        assert!(memoria.para_accion("web-01", 5).is_some());
        let tras_accion = memoria
            .claves
            .get("web-01")
            .map(|clave| clave.ultimo_uso)
            .expect("guardada");
        assert!(tras_accion > antes, "para_accion renueva el último uso");
    }

    #[test]
    fn el_sudo_caduca_a_los_minutos_sin_uso_y_se_olvida() {
        let mut memoria = SudoEnMemoria::nuevo();
        memoria.guardar("web-01", Zeroizing::new("secreta".to_string()));
        if let Some(clave) = memoria.claves.get_mut("web-01") {
            clave.ultimo_uso = Instant::now() - Duration::from_secs(5 * 60);
        }
        assert!(memoria.vigente("web-01", 5).is_none());
        assert!(
            !memoria.claves.contains_key("web-01"),
            "la caducada se olvida"
        );
        memoria.guardar("web-01", Zeroizing::new("secreta".to_string()));
        memoria.olvidar("web-01");
        assert!(memoria.vigente("web-01", 5).is_none());
        memoria.guardar("web-01", Zeroizing::new("secreta".to_string()));
        memoria.guardar("web-02", Zeroizing::new("otra".to_string()));
        memoria.olvidar_todas();
        assert!(memoria.vigente("web-01", 5).is_none());
        assert!(memoria.vigente("web-02", 5).is_none());
    }

    #[test]
    fn con_cero_minutos_la_contrasena_no_se_guarda_nunca() {
        let mut memoria = SudoEnMemoria::nuevo();
        memoria.guardar("web-01", Zeroizing::new("secreta".to_string()));
        assert!(memoria.vigente("web-01", 0).is_none());
        assert!(!memoria.claves.contains_key("web-01"));
    }

    #[test]
    fn la_entrada_lleva_la_contrasena_y_su_salto_de_linea() {
        let password = Some(Zeroizing::new("secreta".to_string()));
        let entrada = entrada_con_password(&password).expect("hay contraseña");
        assert_eq!(entrada.as_slice(), b"secreta\n");
        assert!(entrada_con_password(&None).is_none());
    }

    #[test]
    fn el_listado_traduce_unidades_timers_y_acciones() {
        let mut listado = listado_vacio();
        listado.reloj = magi_nucleo::servicios::RelojHost {
            epoch: 1_000_000,
            uptime_seg: 5_000,
        };
        listado.unidades = vec![
            unidad(
                "app.service",
                &[
                    ("ActiveState", "active"),
                    ("UnitFileState", "enabled"),
                    ("CanReload", "yes"),
                    ("ExecMainPID", "812"),
                    ("MemoryCurrent", "1048576"),
                    ("TasksCurrent", "6"),
                    ("ActiveEnterTimestampMonotonic", "4900000000"),
                ],
            ),
            unidad("copia.service", &[("ActiveState", "failed")]),
        ];
        listado.timers = vec![magi_nucleo::servicios::Timer {
            nombre: "backup.timer".to_string(),
            props: [
                ("ActiveState".to_string(), "active".to_string()),
                (
                    "NextElapseUSecRealtime".to_string(),
                    "1000100000000".to_string(),
                ),
                (
                    "LastTriggerUSecMonotonic".to_string(),
                    "4900000000".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            servicio: Some("app.service".to_string()),
        }];
        let vista = listado_a_vista(&listado, true, Some(123));
        assert_eq!(vista.hora, Some(123));
        assert!(vista.sudo_en_memoria);
        assert_eq!(vista.reloj.epoch, 1_000_000);
        let app = vista
            .unidades
            .iter()
            .find(|unidad| unidad.nombre == "app.service")
            .expect("app.service");
        assert_eq!(app.pid, Some(812));
        assert_eq!(app.memoria_bytes, Some(1_048_576));
        assert_eq!(app.tareas, Some(6));
        assert_eq!(app.arranque, EstadoArranqueMovil::Activado);
        assert_eq!(app.desde_en, Some(999_900));
        assert!(app.acciones.contains(&AccionMovil::Parar));
        assert!(app.acciones.contains(&AccionMovil::Recargar));
        // Las tareas «infinity» se descartan.
        let mut con_infinito = listado.clone();
        con_infinito.unidades[0]
            .props
            .insert("TasksCurrent".to_string(), u64::MAX.to_string());
        let vista = listado_a_vista(&con_infinito, false, None);
        let app_infinito = vista
            .unidades
            .iter()
            .find(|unidad| unidad.nombre == "app.service")
            .expect("app.service");
        assert_eq!(app_infinito.tareas, None);
        // Orden de la F13: la fallida primero y después el resto por nombre.
        assert_eq!(
            vista
                .unidades
                .iter()
                .map(|unidad| unidad.nombre.as_str())
                .collect::<Vec<_>>(),
            vec!["copia.service", "app.service"]
        );
        // El timer dice su servicio, su próxima y su última ejecución.
        let timer = &vista.timers[0];
        assert_eq!(timer.servicio.as_deref(), Some("app.service"));
        assert_eq!(timer.proxima_en, Some(1_000_100));
        assert_eq!(timer.ultima_en, Some(999_900));
        // app.service está activo: el timer no se puede lanzar.
        assert!(timer.acciones.is_empty());
    }

    #[test]
    fn las_acciones_permitidas_salen_del_listado_y_su_motivo_del_nucleo() {
        let mut listado = listado_vacio();
        listado.unidades = vec![unidad(
            "app.service",
            &[("ActiveState", "active"), ("CanReload", "no")],
        )];
        let listado = listado_a_vista(&listado, false, None);
        let objetivo = Objetivo::unidad("app.service");
        let permitidas = acciones_permitidas(&listado, &objetivo);
        assert!(permitidas.contains(&AccionMovil::Parar));
        assert!(!permitidas.contains(&AccionMovil::Recargar));
        let permitidas_nucleo: Vec<Accion> = permitidas
            .iter()
            .map(|accion| accion_a_nucleo(*accion))
            .collect();
        let motivo =
            nucleo_acciones::motivo_no_permitida(&objetivo, Accion::Recargar, &permitidas_nucleo);
        assert_eq!(motivo.as_deref(), Some("app.service no admite recargar"));
        // Un objetivo que no está en el listado no tiene acciones.
        let otro = Objetivo::contenedor("panel");
        assert!(acciones_permitidas(&listado, &otro).is_empty());
    }

    #[test]
    fn el_mensaje_de_error_del_script_se_traduce() {
        assert_eq!(
            mensaje_de_error("ERROR sin_permiso", Some(69)).as_deref(),
            Some("sin permiso en el host para leer eso: da la contraseña de sudo")
        );
        assert_eq!(
            mensaje_de_error("", Some(66)).as_deref(),
            Some("el objetivo ya no existe en el host")
        );
        assert_eq!(
            mensaje_de_error("ERROR sin_temporales", Some(70)).as_deref(),
            Some("al host le faltan mktemp o mkfifo para seguir los logs")
        );
        assert!(mensaje_de_error("", Some(0)).is_none());
    }

    #[test]
    fn una_accion_que_deja_algo_parado_delibera_y_una_que_no_no() {
        assert!(nucleo_acciones::exige_deliberacion(
            &Objetivo::unidad("app.service"),
            Accion::Reiniciar
        ));
        assert!(!nucleo_acciones::exige_deliberacion(
            &Objetivo::unidad("app.service"),
            Accion::Recargar
        ));
        assert!(nucleo_acciones::exige_deliberacion(
            &Objetivo::proyecto("web"),
            Accion::Parar
        ));
    }

    #[test]
    fn el_uuid_y_las_fases_se_traducen_en_las_dos_direcciones() {
        for accion in [
            Accion::Arrancar,
            Accion::Parar,
            Accion::Reiniciar,
            Accion::Recargar,
            Accion::Activar,
            Accion::Desactivar,
            Accion::Lanzar,
        ] {
            assert_eq!(accion_a_nucleo(accion_a_vista(accion)), accion);
        }
        for tipo in [
            TipoObjetivo::Unidad,
            TipoObjetivo::Timer,
            TipoObjetivo::Contenedor,
            TipoObjetivo::Proyecto,
        ] {
            assert_eq!(tipo_objetivo_a_nucleo(tipo_objetivo_a_vista(tipo)), tipo);
        }
        assert_eq!(
            fase_a_vista(FaseAccion::Comprobando),
            FaseAccionMovil::Comprobando
        );
        assert_eq!(
            fase_a_vista(FaseAccion::Ejecutando),
            FaseAccionMovil::Ejecutando
        );
        assert_eq!(
            fase_a_vista(FaseAccion::Esperando),
            FaseAccionMovil::Esperando
        );
    }
}
