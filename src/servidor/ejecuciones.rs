//! Ejecuciones de snippets en el servidor de sesiones (Fase 6). El cliente
//! resuelve destinos, variables y deliberación y manda el comando ya
//! sustituido y los hosts; aquí se ejecuta sin diálogos, por `exec`, en hasta
//! ocho hosts a la vez, con timeout, «parar al primer fallo» y cancelación.
//!
//! La salida de cada host se guarda en memoria con tope (1 MiB por flujo) y se
//! entrega a quien la pida con `PedirSalida`; nunca va a `REGISTRO` ni al log
//! (T44). Las ejecuciones terminadas se conservan una hora o hasta
//! `LimpiarEjecuciones`, y sobreviven a la ventana.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh::ChannelMsg;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::conexion::cliente::Cliente;
use crate::deliberacion::{
    detalle_registro, EjecucionResultado, RegistroDeliberacion, ResultadoDeliberacion,
};
use crate::modelo::{Host, ResultadoRegistro};
use crate::protocolo::{
    DeliberacionLanzada, EstadoEjecucion, EstadoHostEjecucion, InfoEjecucion, InfoEjecucionHost,
    MensajeServidor,
};

use super::conexiones::{self, PoliticaConexion, UsoCanal};
use super::{difusion, EstadoServidor, OrdenBd};

/// Hosts que se ejecutan a la vez en una ejecución.
pub const MAX_PARALELO: usize = 8;

/// Tope de lo que se guarda de cada flujo (stdout, stderr) por host.
pub const TOPE_SALIDA: usize = 1024 * 1024;

/// Segundos que se conserva una ejecución terminada.
pub const RETENCION_SEG: i64 = 3600;

/// Plazo para abrir el canal `exec` sobre la conexión ya tomada.
const PLAZO_CANAL: Duration = Duration::from_secs(10);

/// Timeout admitido para una ejecución, en segundos.
const TIMEOUT: std::ops::RangeInclusive<u32> =
    crate::snippets::TIMEOUT_MIN..=crate::snippets::TIMEOUT_MAX;

// ---------------------------------------------------------------- salida

/// Salida de un host: fuera del mutex del estado, para que leer del canal no
/// espere nunca a nadie (una espera aquí pararía la conexión compartida).
#[derive(Default)]
pub struct SalidaHost {
    stdout: std::sync::Mutex<Vec<u8>>,
    stderr: std::sync::Mutex<Vec<u8>>,
    bytes_stdout: AtomicU64,
    bytes_stderr: AtomicU64,
    truncada: AtomicBool,
}

impl SalidaHost {
    fn anotar(&self, error: bool, datos: &[u8]) {
        let (bufer, contador) = if error {
            (&self.stderr, &self.bytes_stderr)
        } else {
            (&self.stdout, &self.bytes_stdout)
        };
        contador.fetch_add(datos.len() as u64, Ordering::Relaxed);
        let Ok(mut bufer) = bufer.lock() else {
            return;
        };
        let cabe = TOPE_SALIDA.saturating_sub(bufer.len());
        if datos.len() > cabe {
            self.truncada.store(true, Ordering::Relaxed);
        }
        bufer.extend_from_slice(&datos[..datos.len().min(cabe)]);
    }

    /// Copia de lo guardado: stdout, stderr y si se truncó.
    pub fn copia(&self) -> (Vec<u8>, Vec<u8>, bool) {
        let stdout = self.stdout.lock().map(|b| b.clone()).unwrap_or_default();
        let stderr = self.stderr.lock().map(|b| b.clone()).unwrap_or_default();
        (stdout, stderr, self.truncada.load(Ordering::Relaxed))
    }

    fn bytes(&self) -> (u64, u64) {
        (
            self.bytes_stdout.load(Ordering::Relaxed),
            self.bytes_stderr.load(Ordering::Relaxed),
        )
    }
}

// ---------------------------------------------------------------- estado

/// La deliberación que autorizó la ejecución, leída y validada al lanzar y
/// fijada: al terminar no se relee (T33).
#[derive(Debug, Clone)]
pub struct DeliberacionFijada {
    pub registro: RegistroDeliberacion,
    pub forzada: bool,
}

/// Un host de una ejecución.
pub struct EjecucionHost {
    pub host_id: i64,
    pub nombre: String,
    pub estado: EstadoHostEjecucion,
    pub codigo: Option<i32>,
    inicio: Option<Instant>,
    inicio_ms: Option<i64>,
    fin: Option<Instant>,
    pub salida: Arc<SalidaHost>,
    pub error: Option<String>,
}

impl EjecucionHost {
    fn nuevo(host_id: i64, nombre: String) -> Self {
        Self {
            host_id,
            nombre,
            estado: EstadoHostEjecucion::EnCola,
            codigo: None,
            inicio: None,
            inicio_ms: None,
            fin: None,
            salida: Arc::new(SalidaHost::default()),
            error: None,
        }
    }

    fn duracion(&self) -> Option<Duration> {
        let inicio = self.inicio?;
        Some(self.fin.unwrap_or_else(Instant::now).duration_since(inicio))
    }

    fn info(&self) -> InfoEjecucionHost {
        let (bytes_stdout, bytes_stderr) = self.salida.bytes();
        InfoEjecucionHost {
            host_id: self.host_id,
            nombre: self.nombre.clone(),
            estado: self.estado,
            codigo: self.codigo,
            inicio_ms: self.inicio_ms,
            duracion_ms: self.duracion().map(|d| d.as_millis() as u64),
            bytes_stdout,
            bytes_stderr,
            truncada: self.salida.truncada.load(Ordering::Relaxed),
            error: self.error.clone(),
        }
    }
}

/// Una ejecución: un comando ya resuelto en una lista de hosts.
pub struct Ejecucion {
    pub id: u32,
    pub peticion_id: u64,
    pub solicitante: u32,
    pub snippet_id: Option<i64>,
    pub nombre: String,
    pub hosts: Vec<EjecucionHost>,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub deliberacion: Option<DeliberacionFijada>,
    pub creada_en: i64,
    pub terminada_en: Option<i64>,
    pub estado: EstadoEjecucion,
    cancelada: bool,
    token: CancellationToken,
}

impl Ejecucion {
    fn info(&self) -> InfoEjecucion {
        InfoEjecucion {
            id: self.id,
            peticion_id: self.peticion_id,
            solicitante: self.solicitante,
            snippet_id: self.snippet_id,
            nombre: self.nombre.clone(),
            hosts: self.hosts.iter().map(EjecucionHost::info).collect(),
            timeout_seg: self.timeout_seg,
            parar_al_fallo: self.parar_al_fallo,
            deliberacion_id: self.deliberacion.as_ref().map(|d| d.registro.id),
            forzada: self.deliberacion.as_ref().is_some_and(|d| d.forzada),
            estado: self.estado,
            creada_en: self.creada_en,
            terminada_en: self.terminada_en,
        }
    }
}

/// Lo que hay que anotar de un host que llegó a su estado final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnotacionHost {
    pub host_id: i64,
    pub detalle: String,
    pub correcto: bool,
}

/// Lo que hay que anotar al cerrar una ejecución deliberada.
#[derive(Debug, Clone)]
pub struct CierreEjecucion {
    pub resultado: EjecucionResultado,
    pub deliberacion: Option<DeliberacionFijada>,
}

/// Registro de ejecuciones del servidor (en `EstadoServidor.ejecuciones`).
#[derive(Default)]
pub struct Ejecuciones {
    lista: BTreeMap<u32, Ejecucion>,
    siguiente_id: u32,
    /// Difusión coalescida: inmediata en cambios de estado, como la cola.
    pub difusion: difusion::DifusionCola,
}

impl Ejecuciones {
    fn insertar(&mut self, mut ejecucion: Ejecucion) -> u32 {
        self.siguiente_id += 1;
        ejecucion.id = self.siguiente_id;
        let id = ejecucion.id;
        self.lista.insert(id, ejecucion);
        self.difusion.marcar(true);
        id
    }

    /// ¿Queda alguna en curso? (el servidor no se apaga por inactividad).
    pub fn hay_en_curso(&self) -> bool {
        self.lista
            .values()
            .any(|ejecucion| ejecucion.estado == EstadoEjecucion::EnCurso)
    }

    /// ¿Usa ya alguna ejecución esta deliberación?
    fn deliberacion_usada(&self, deliberacion_id: i64) -> bool {
        self.lista.values().any(|ejecucion| {
            ejecucion
                .deliberacion
                .as_ref()
                .is_some_and(|d| d.registro.id == deliberacion_id)
        })
    }

    pub fn info(&self) -> Vec<InfoEjecucion> {
        self.lista.values().map(Ejecucion::info).collect()
    }

    /// El host `indice` empieza a conectar, salvo que ya no le toque (omitido
    /// por «parar al fallo» o cancelado): entonces devuelve `false`.
    pub fn marcar_conectando(&mut self, id: u32, indice: usize, ahora_ms: i64) -> bool {
        let Some(ejecucion) = self.lista.get_mut(&id) else {
            return false;
        };
        if ejecucion.cancelada {
            return false;
        }
        let Some(host) = ejecucion.hosts.get_mut(indice) else {
            return false;
        };
        if host.estado != EstadoHostEjecucion::EnCola {
            return false;
        }
        host.estado = EstadoHostEjecucion::Conectando;
        host.inicio = Some(Instant::now());
        host.inicio_ms = Some(ahora_ms);
        self.difusion.marcar(true);
        true
    }

    pub fn marcar_ejecutando(&mut self, id: u32, indice: usize) {
        if let Some(host) = self
            .lista
            .get_mut(&id)
            .and_then(|ejecucion| ejecucion.hosts.get_mut(indice))
        {
            if host.estado == EstadoHostEjecucion::Conectando {
                host.estado = EstadoHostEjecucion::Ejecutando;
                self.difusion.marcar(true);
            }
        }
    }

    /// Lleva un host a su estado final. Solo la primera vez devuelve lo que
    /// hay que anotar (un host final no cambia). Con «parar al primer fallo»,
    /// un `fallo` o `error` deja omitidos los que aún esperan en cola.
    pub fn finalizar_host(
        &mut self,
        id: u32,
        indice: usize,
        estado: EstadoHostEjecucion,
        codigo: Option<i32>,
        error: Option<String>,
    ) -> Option<AnotacionHost> {
        let ejecucion = self.lista.get_mut(&id)?;
        let nombre_snippet = ejecucion.nombre.clone();
        let host = ejecucion.hosts.get_mut(indice)?;
        if host.estado.es_final() {
            return None;
        }
        host.estado = estado;
        host.codigo = codigo;
        host.error = error;
        host.fin = Some(Instant::now());
        let anotacion = (estado != EstadoHostEjecucion::Omitido).then(|| AnotacionHost {
            host_id: host.host_id,
            detalle: detalle_host(&nombre_snippet, host),
            correcto: estado == EstadoHostEjecucion::Ok,
        });
        if ejecucion.parar_al_fallo
            && matches!(
                estado,
                EstadoHostEjecucion::Fallo | EstadoHostEjecucion::Error
            )
        {
            for otro in &mut ejecucion.hosts {
                if otro.estado == EstadoHostEjecucion::EnCola {
                    otro.estado = EstadoHostEjecucion::Omitido;
                }
            }
        }
        self.difusion.marcar(true);
        anotacion
    }

    /// Cancela: los hosts en cola quedan omitidos y el token avisa a los que
    /// están en marcha (que cierran su canal y quedan cancelados).
    pub fn cancelar(&mut self, id: u32) -> bool {
        let Some(ejecucion) = self.lista.get_mut(&id) else {
            return false;
        };
        if ejecucion.estado != EstadoEjecucion::EnCurso {
            return false;
        }
        ejecucion.cancelada = true;
        for host in &mut ejecucion.hosts {
            if host.estado == EstadoHostEjecucion::EnCola {
                host.estado = EstadoHostEjecucion::Omitido;
            }
        }
        ejecucion.token.cancel();
        self.difusion.marcar(true);
        true
    }

    /// Si todos los hosts terminaron, cierra la ejecución (una sola vez) y
    /// devuelve su resultado para anotar.
    pub fn cerrar_si_termino(&mut self, id: u32, ahora: i64) -> Option<CierreEjecucion> {
        let ejecucion = self.lista.get_mut(&id)?;
        if ejecucion.estado != EstadoEjecucion::EnCurso
            || !ejecucion.hosts.iter().all(|host| host.estado.es_final())
        {
            return None;
        }
        ejecucion.estado = if ejecucion.cancelada {
            EstadoEjecucion::Cancelada
        } else {
            EstadoEjecucion::Terminada
        };
        ejecucion.terminada_en = Some(ahora);
        let ok = ejecucion
            .hosts
            .iter()
            .filter(|host| host.estado == EstadoHostEjecucion::Ok)
            .count();
        self.difusion.marcar(true);
        Some(CierreEjecucion {
            resultado: EjecucionResultado::de_hosts(ok, ejecucion.hosts.len(), ejecucion.cancelada),
            deliberacion: ejecucion.deliberacion.clone(),
        })
    }

    /// Salida guardada de un host.
    pub fn salida(&self, id: u32, host_id: i64) -> Option<(Vec<u8>, Vec<u8>, bool)> {
        let ejecucion = self.lista.get(&id)?;
        let host = ejecucion
            .hosts
            .iter()
            .find(|host| host.host_id == host_id)?;
        Some(host.salida.copia())
    }

    /// Quita las terminadas de hace más de una hora. Devuelve cuántas.
    pub fn purgar_caducadas(&mut self, ahora: i64) -> usize {
        let antes = self.lista.len();
        self.lista.retain(|_, ejecucion| {
            ejecucion
                .terminada_en
                .is_none_or(|fin| ahora - fin < RETENCION_SEG)
        });
        antes - self.lista.len()
    }

    /// `LimpiarEjecuciones`: quita las terminadas (las en curso siguen).
    pub fn limpiar(&mut self) -> usize {
        let antes = self.lista.len();
        self.lista
            .retain(|_, ejecucion| ejecucion.estado == EstadoEjecucion::EnCurso);
        antes - self.lista.len()
    }

    /// Apagado del servidor: todo lo que queda en curso se da por cancelado
    /// aquí mismo (con sus anotaciones), sin esperar a la red. Las tareas de
    /// los hosts, al volver, ya no encuentran nada que finalizar.
    pub fn cancelar_todas(&mut self, ahora: i64) -> (Vec<AnotacionHost>, Vec<CierreEjecucion>) {
        let ids: Vec<u32> = self
            .lista
            .values()
            .filter(|ejecucion| ejecucion.estado == EstadoEjecucion::EnCurso)
            .map(|ejecucion| ejecucion.id)
            .collect();
        let mut anotaciones = Vec::new();
        let mut cierres = Vec::new();
        for id in ids {
            self.cancelar(id);
            let indices: Vec<usize> = self.lista[&id]
                .hosts
                .iter()
                .enumerate()
                .filter(|(_, host)| host.estado.en_marcha())
                .map(|(indice, _)| indice)
                .collect();
            for indice in indices {
                anotaciones.extend(self.finalizar_host(
                    id,
                    indice,
                    EstadoHostEjecucion::Cancelado,
                    None,
                    Some("el servidor se apaga".to_string()),
                ));
            }
            cierres.extend(self.cerrar_si_termino(id, ahora));
        }
        (anotaciones, cierres)
    }
}

/// Detalle de `snippet_ejecutado`: snippet, host, estado, código, duración y
/// bytes. Nunca la salida ni el comando.
fn detalle_host(snippet: &str, host: &EjecucionHost) -> String {
    let (stdout, stderr) = host.salida.bytes();
    let mut detalle = format!("«{snippet}» · {} · {}", host.nombre, host.estado.texto());
    if let Some(codigo) = host.codigo {
        detalle.push_str(&format!(" · código {codigo}"));
    }
    if let Some(duracion) = host.duracion() {
        detalle.push_str(&format!(
            " · {}",
            crate::snippets::salida::duracion_legible(duracion.as_millis() as u64)
        ));
    }
    detalle.push_str(&format!(" · {} B", stdout + stderr));
    if host.salida.truncada.load(Ordering::Relaxed) {
        detalle.push_str(" · salida truncada");
    }
    if let Some(error) = &host.error {
        detalle.push_str(&format!(
            " · {}",
            crate::snippets::salida::sanear_linea(error, 120)
        ));
    }
    detalle
}

// ---------------------------------------------------------------- lanzar

/// Lo que llega en `LanzarEjecucion`.
pub struct PeticionLanzar {
    pub peticion_id: u64,
    pub snippet_id: Option<i64>,
    pub nombre: String,
    pub comando: String,
    pub host_ids: Vec<i64>,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub deliberacion: Option<DeliberacionLanzada>,
}

/// Todo lo fijado al lanzar: el comando, los hosts leídos del inventario y
/// con qué abrir su conexión.
struct Plan {
    comando: Arc<str>,
    hosts: Vec<Option<Host>>,
    todos: HashMap<i64, Host>,
    rutas: crate::config::Rutas,
    timeout: Duration,
    solicitante: u32,
    token: CancellationToken,
}

/// Valida y registra una ejecución y lanza su despachador. Devuelve el id o
/// el motivo del rechazo (se contesta `Error` con el `peticion_id`).
pub async fn lanzar(
    estado: &Arc<Mutex<EstadoServidor>>,
    solicitante: u32,
    peticion: PeticionLanzar,
) -> Result<u32, String> {
    if peticion.nombre.trim().is_empty() {
        return Err("la ejecución necesita el nombre del snippet".to_string());
    }
    if peticion.comando.trim().is_empty() || peticion.comando.contains('\0') {
        return Err("el comando está vacío o no es válido".to_string());
    }
    if !TIMEOUT.contains(&peticion.timeout_seg) {
        return Err(format!(
            "el timeout debe estar entre {} y {} segundos",
            TIMEOUT.start(),
            TIMEOUT.end()
        ));
    }
    let mut host_ids: Vec<i64> = Vec::new();
    for id in &peticion.host_ids {
        if !host_ids.contains(id) {
            host_ids.push(*id);
        }
    }
    if host_ids.is_empty() {
        return Err("la ejecución no tiene hosts".to_string());
    }

    // El inventario y la deliberación se leen sin el mutex del estado: es E/S
    // de SQLite y podría tardar hasta el `busy_timeout`.
    let (lectura, rutas) = {
        let estado_bloqueado = estado.lock().await;
        (
            estado_bloqueado.lectura.clone(),
            estado_bloqueado.rutas.clone(),
        )
    };
    let (todos, deliberacion) = {
        let lectura = lectura
            .lock()
            .map_err(|_| "almacén envenenado".to_string())?;
        let todos: HashMap<i64, Host> = lectura
            .listar_hosts()
            .map_err(|error| format!("no se pudo leer el inventario: {error}"))?
            .into_iter()
            .map(|host| (host.id, host))
            .collect();
        let deliberacion = match &peticion.deliberacion {
            Some(lanzada) => Some(validar_deliberacion(&lectura, lanzada)?),
            None => None,
        };
        (todos, deliberacion)
    };
    let hosts: Vec<Option<Host>> = host_ids.iter().map(|id| todos.get(id).cloned()).collect();
    let token = CancellationToken::new();
    let ejecucion = Ejecucion {
        id: 0,
        peticion_id: peticion.peticion_id,
        solicitante,
        snippet_id: peticion.snippet_id,
        nombre: peticion.nombre.clone(),
        hosts: host_ids
            .iter()
            .zip(&hosts)
            .map(|(id, host)| {
                let nombre = host
                    .as_ref()
                    .map(|host| host.nombre.clone())
                    .unwrap_or_else(|| format!("host {id}"));
                EjecucionHost::nuevo(*id, nombre)
            })
            .collect(),
        timeout_seg: peticion.timeout_seg,
        parar_al_fallo: peticion.parar_al_fallo,
        deliberacion,
        creada_en: crate::modelo::fecha_ahora_epoca(),
        terminada_en: None,
        estado: EstadoEjecucion::EnCurso,
        cancelada: false,
        token: token.clone(),
    };
    let id = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            return Err(conexiones::APAGANDO.to_string());
        }
        if let Some(fijada) = &ejecucion.deliberacion {
            if estado_bloqueado
                .ejecuciones
                .deliberacion_usada(fijada.registro.id)
            {
                return Err("esa deliberación ya autorizó otra ejecución".to_string());
            }
        }
        estado_bloqueado.vacio_desde = None;
        estado_bloqueado.ejecuciones.insertar(ejecucion)
    };
    info!(
        ejecucion = id,
        snippet = %peticion.nombre,
        hosts = host_ids.len(),
        "ejecución lanzada"
    );
    difundir(estado).await;
    let plan = Plan {
        comando: Arc::from(peticion.comando.as_str()),
        hosts,
        todos,
        rutas,
        timeout: Duration::from_secs(u64::from(peticion.timeout_seg)),
        solicitante,
        token,
    };
    let estado_despacho = estado.clone();
    tokio::spawn(async move { despachar(estado_despacho, id, plan).await });
    Ok(id)
}

/// Lee y valida la fila de la deliberación: la escribió otro proceso y lo que
/// autoriza es una ejecución (T38).
fn validar_deliberacion(
    lectura: &crate::almacen::Almacen,
    lanzada: &DeliberacionLanzada,
) -> Result<DeliberacionFijada, String> {
    let registro = lectura.obtener_deliberacion(lanzada.id).map_err(|_| {
        format!(
            "la deliberación #{} no existe o no se puede leer",
            lanzada.id
        )
    })?;
    let coherente = match registro.resultado {
        ResultadoDeliberacion::Aprobada => !lanzada.forzada,
        ResultadoDeliberacion::Forzada => {
            lanzada.forzada
                && registro
                    .motivo
                    .as_deref()
                    .is_some_and(|m| !m.trim().is_empty())
        }
        ResultadoDeliberacion::Cancelada => false,
    };
    if !coherente {
        return Err(format!(
            "la deliberación #{} no autoriza esta ejecución",
            lanzada.id
        ));
    }
    if registro.ejecucion_resultado.is_some() {
        return Err(format!(
            "la deliberación #{} ya se usó en otra ejecución",
            lanzada.id
        ));
    }
    Ok(DeliberacionFijada {
        registro,
        forzada: lanzada.forzada,
    })
}

/// Recorre los hosts en orden (FIFO) con el semáforo de ocho: a cada uno le
/// toca su turno o, si «parar al fallo» o una cancelación lo dejaron fuera,
/// se salta. Al final cierra la ejecución.
async fn despachar(estado: Arc<Mutex<EstadoServidor>>, id: u32, plan: Plan) {
    let plan = Arc::new(plan);
    let semaforo = Arc::new(Semaphore::new(MAX_PARALELO));
    let mut tareas = tokio::task::JoinSet::new();
    for (indice, host) in plan.hosts.iter().enumerate() {
        let permiso = tokio::select! {
            permiso = semaforo.clone().acquire_owned() => match permiso {
                Ok(permiso) => permiso,
                Err(_) => break,
            },
            _ = plan.token.cancelled() => break,
        };
        let toca = estado.lock().await.ejecuciones.marcar_conectando(
            id,
            indice,
            chrono::Utc::now().timestamp_millis(),
        );
        if !toca {
            continue;
        }
        difundir(&estado).await;
        let Some(host) = host.clone() else {
            finalizar(
                &estado,
                id,
                indice,
                EstadoHostEjecucion::Error,
                None,
                Some("el host ya no existe en el inventario".to_string()),
            )
            .await;
            continue;
        };
        let estado_host = estado.clone();
        let plan_host = plan.clone();
        tareas.spawn(async move {
            let _permiso = permiso;
            ejecutar_en_host(&estado_host, id, indice, &host, &plan_host).await;
        });
    }
    while tareas.join_next().await.is_some() {}
    cerrar(&estado, id).await;
}

/// Un host: su conexión (del pool o nueva, sin diálogos), el `exec` y su
/// estado final.
async fn ejecutar_en_host(
    estado: &Arc<Mutex<EstadoServidor>>,
    id: u32,
    indice: usize,
    host: &Host,
    plan: &Plan,
) {
    let (id_solicitud, salida) = {
        let mut estado_bloqueado = estado.lock().await;
        let id_solicitud = estado_bloqueado.siguiente_sesion_id;
        estado_bloqueado.siguiente_sesion_id += 1;
        let salida = estado_bloqueado
            .ejecuciones
            .lista
            .get(&id)
            .and_then(|ejecucion| ejecucion.hosts.get(indice))
            .map(|host| host.salida.clone())
            .unwrap_or_default();
        (id_solicitud, salida)
    };
    let tomada = tokio::select! {
        tomada = conexiones::conexion_para_canal(
            estado,
            host,
            &plan.todos,
            &plan.rutas,
            PoliticaConexion::NoInteractiva {
                solicitante: Some(plan.solicitante),
                id_solicitud,
            },
            UsoCanal::Subsistema,
        ) => tomada,
        _ = plan.token.cancelled() => {
            finalizar(estado, id, indice, EstadoHostEjecucion::Cancelado, None, None).await;
            return;
        }
    };
    let tomada = match tomada {
        Ok(tomada) => tomada,
        Err(motivo) => {
            finalizar(
                estado,
                id,
                indice,
                EstadoHostEjecucion::Error,
                None,
                Some(motivo),
            )
            .await;
            return;
        }
    };
    estado
        .lock()
        .await
        .ejecuciones
        .marcar_ejecutando(id, indice);
    difundir(estado).await;
    let fin = ejecutar_exec(
        &tomada.handle,
        &plan.comando,
        plan.timeout,
        &plan.token,
        &salida,
    )
    .await;
    tomada.soltar().await;
    let (estado_final, codigo, error) = match fin {
        FinExec::Codigo(0) => (EstadoHostEjecucion::Ok, Some(0), None),
        FinExec::Codigo(codigo) => (EstadoHostEjecucion::Fallo, Some(codigo), None),
        FinExec::Senal(senal) => (
            EstadoHostEjecucion::Fallo,
            None,
            Some(format!("terminado por la señal {senal}")),
        ),
        FinExec::Tiempo => (
            EstadoHostEjecucion::Error,
            None,
            Some("tiempo agotado".to_string()),
        ),
        FinExec::Cancelado => (EstadoHostEjecucion::Cancelado, None, None),
        FinExec::Fallo(motivo) => (EstadoHostEjecucion::Error, None, Some(motivo)),
    };
    finalizar(estado, id, indice, estado_final, codigo, error).await;
}

/// Cómo acabó un `exec`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FinExec {
    Codigo(i32),
    Senal(String),
    Tiempo,
    Cancelado,
    Fallo(String),
}

/// Canal `exec` con stdout y stderr separados, hasta el cierre del canal (el
/// código llega tras el EOF de los datos), con plazo y cancelación. El canal
/// se cierra siempre de forma explícita: en russh no se cierra al soltarse.
async fn ejecutar_exec(
    handle: &Arc<Handle<Cliente>>,
    comando: &str,
    plazo: Duration,
    token: &CancellationToken,
    salida: &SalidaHost,
) -> FinExec {
    let apertura = handle.clone();
    let canal = tokio::select! {
        canal = conexiones::abrir_con_plazo(PLAZO_CANAL, async move {
            apertura.channel_open_session().await
        }) => canal,
        _ = token.cancelled() => return FinExec::Cancelado,
    };
    let mut canal = match canal {
        Ok(canal) => canal,
        Err(motivo) => return FinExec::Fallo(format!("no se pudo abrir el canal: {motivo}")),
    };
    let limite = tokio::time::Instant::now() + plazo;
    let trabajo = async {
        canal
            .exec(true, comando.as_bytes().to_vec())
            .await
            .map_err(|error| FinExec::Fallo(error.to_string()))?;
        // No interactivo: el comando no recibe nada por stdin.
        let _ = canal.eof().await;
        let mut codigo = None;
        let mut senal = None;
        loop {
            match canal.wait().await {
                Some(ChannelMsg::Data { data }) => salida.anotar(false, &data),
                Some(ChannelMsg::ExtendedData { data, .. }) => salida.anotar(true, &data),
                Some(ChannelMsg::ExitStatus { exit_status }) => codigo = Some(exit_status as i32),
                Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                    senal = Some(format!("{signal_name:?}"));
                }
                Some(ChannelMsg::Failure) => {
                    return Err(FinExec::Fallo("el host rechazó el comando".to_string()));
                }
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        Ok((codigo, senal))
    };
    let desenlace = tokio::select! {
        resultado = tokio::time::timeout_at(limite, trabajo) => match resultado {
            Ok(Ok((Some(codigo), _))) => FinExec::Codigo(codigo),
            Ok(Ok((None, Some(senal)))) => FinExec::Senal(senal),
            Ok(Ok((None, None))) => FinExec::Fallo("el host cerró el canal sin código de salida".to_string()),
            Ok(Err(fin)) => fin,
            Err(_) => FinExec::Tiempo,
        },
        _ = token.cancelled() => FinExec::Cancelado,
    };
    let _ = tokio::time::timeout(Duration::from_secs(5), canal.close()).await;
    desenlace
}

/// Lleva un host a su estado final, lo anota (si toca) y difunde.
async fn finalizar(
    estado: &Arc<Mutex<EstadoServidor>>,
    id: u32,
    indice: usize,
    estado_final: EstadoHostEjecucion,
    codigo: Option<i32>,
    error: Option<String>,
) {
    let (anotacion, bd) = {
        let mut estado_bloqueado = estado.lock().await;
        let anotacion =
            estado_bloqueado
                .ejecuciones
                .finalizar_host(id, indice, estado_final, codigo, error);
        (anotacion, estado_bloqueado.bd.clone())
    };
    if let Some(anotacion) = anotacion {
        anotar_host(&bd, anotacion);
    }
    difundir(estado).await;
}

fn anotar_host(bd: &std::sync::mpsc::Sender<OrdenBd>, anotacion: AnotacionHost) {
    let _ = bd.send(OrdenBd::Anotar {
        tipo: crate::registro::SNIPPET_EJECUTADO.to_string(),
        host_id: Some(anotacion.host_id),
        identidad_id: None,
        detalle: anotacion.detalle,
        resultado: if anotacion.correcto {
            ResultadoRegistro::Ok
        } else {
            ResultadoRegistro::Error
        },
    });
}

/// Cierra la ejecución si todos sus hosts terminaron: con deliberación,
/// rellena `DELIBERACIONES.ejecucion_resultado` y anota
/// `deliberacion_aprobada` o `deliberacion_forzada`.
async fn cerrar(estado: &Arc<Mutex<EstadoServidor>>, id: u32) {
    let (cierre, bd) = {
        let mut estado_bloqueado = estado.lock().await;
        let cierre = estado_bloqueado
            .ejecuciones
            .cerrar_si_termino(id, crate::modelo::fecha_ahora_epoca());
        (cierre, estado_bloqueado.bd.clone())
    };
    if let Some(cierre) = cierre {
        info!(
            ejecucion = id,
            resultado = cierre.resultado.como_texto(),
            "ejecución terminada"
        );
        anotar_cierre(&bd, cierre);
    }
    difundir(estado).await;
}

fn anotar_cierre(bd: &std::sync::mpsc::Sender<OrdenBd>, cierre: CierreEjecucion) {
    let Some(fijada) = cierre.deliberacion else {
        return;
    };
    let _ = bd.send(OrdenBd::ResultadoDeliberacion {
        deliberacion_id: fijada.registro.id,
        resultado: cierre.resultado,
    });
    let _ = bd.send(OrdenBd::Anotar {
        tipo: if fijada.forzada {
            crate::registro::DELIBERACION_FORZADA.to_string()
        } else {
            crate::registro::DELIBERACION_APROBADA.to_string()
        },
        host_id: None,
        identidad_id: None,
        detalle: detalle_registro(&fijada.registro, Some(cierre.resultado)),
        resultado: if cierre.resultado == EjecucionResultado::Ok {
            ResultadoRegistro::Ok
        } else {
            ResultadoRegistro::Error
        },
    });
}

// ---------------------------------------------------------------- órdenes

/// `CancelarEjecucion`.
pub async fn cancelar(estado: &Arc<Mutex<EstadoServidor>>, id: u32) {
    if estado.lock().await.ejecuciones.cancelar(id) {
        difundir(estado).await;
    }
}

/// `LimpiarEjecuciones`.
pub async fn limpiar(estado: &Arc<Mutex<EstadoServidor>>) {
    if estado.lock().await.ejecuciones.limpiar() > 0 {
        difundir(estado).await;
    }
}

/// `PedirSalida`: contesta solo a quien la pide; si ya no existe, nada.
pub async fn pedir_salida(
    estado: &Arc<Mutex<EstadoServidor>>,
    cliente_id: u32,
    ejecucion_id: u32,
    host_id: i64,
) {
    let estado_bloqueado = estado.lock().await;
    let Some((stdout, stderr, truncada)) =
        estado_bloqueado.ejecuciones.salida(ejecucion_id, host_id)
    else {
        return;
    };
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        difusion::enviar(
            cliente,
            MensajeServidor::Salida {
                ejecucion_id,
                host_id,
                stdout: crate::protocolo::SalidaRemota(stdout),
                stderr: crate::protocolo::SalidaRemota(stderr),
                truncada,
            },
        );
    }
}

/// Apagado: cancela todas las en curso y anota lo que les toque, sin red.
pub async fn cancelar_todas_al_apagar(estado: &Arc<Mutex<EstadoServidor>>) {
    let (anotaciones, cierres, bd) = {
        let mut estado_bloqueado = estado.lock().await;
        let (anotaciones, cierres) = estado_bloqueado
            .ejecuciones
            .cancelar_todas(crate::modelo::fecha_ahora_epoca());
        (anotaciones, cierres, estado_bloqueado.bd.clone())
    };
    if !anotaciones.is_empty() {
        warn!(
            hosts = anotaciones.len(),
            "ejecuciones canceladas por el apagado"
        );
    }
    for anotacion in anotaciones {
        anotar_host(&bd, anotacion);
    }
    for cierre in cierres {
        anotar_cierre(&bd, cierre);
    }
}

// ---------------------------------------------------------------- difusión

/// Marca la lista como cambiada: la revisora la manda enseguida.
async fn difundir(estado: &Arc<Mutex<EstadoServidor>>) {
    estado.lock().await.ejecuciones.difusion.marcar(true);
}

/// Revisora: difunde `Ejecuciones` cuando toca (enseguida tras un cambio de
/// estado, como mucho cuatro veces por segundo) y purga las caducadas.
pub async fn revisar(estado: Arc<Mutex<EstadoServidor>>) {
    loop {
        let espera = {
            let estado_bloqueado = estado.lock().await;
            if estado_bloqueado.ejecuciones.difusion.inmediato {
                difusion::ESPERA_INMEDIATA
            } else {
                difusion::RITMO_COLA
            }
        };
        tokio::time::sleep(espera).await;
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado
            .ejecuciones
            .purgar_caducadas(crate::modelo::fecha_ahora_epoca())
            > 0
        {
            estado_bloqueado.ejecuciones.difusion.marcar(false);
        }
        // Mientras hay alguna en curso, las duraciones cambian: se refresca
        // al ritmo normal.
        if estado_bloqueado.ejecuciones.hay_en_curso() {
            estado_bloqueado.ejecuciones.difusion.marcar(false);
        }
        if !estado_bloqueado
            .ejecuciones
            .difusion
            .toca(std::time::Instant::now())
        {
            continue;
        }
        let lista = estado_bloqueado.ejecuciones.info();
        difusion::difundir(
            &estado_bloqueado.clientes,
            MensajeServidor::Ejecuciones { lista },
        );
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ejecucion(hosts: usize, parar_al_fallo: bool) -> Ejecucion {
        Ejecucion {
            id: 0,
            peticion_id: 1,
            solicitante: 1,
            snippet_id: None,
            nombre: "prueba".to_string(),
            hosts: (0..hosts)
                .map(|indice| EjecucionHost::nuevo(indice as i64 + 1, format!("h{indice}")))
                .collect(),
            timeout_seg: 60,
            parar_al_fallo,
            deliberacion: None,
            creada_en: 0,
            terminada_en: None,
            estado: EstadoEjecucion::EnCurso,
            cancelada: false,
            token: CancellationToken::new(),
        }
    }

    fn estados(registro: &Ejecuciones, id: u32) -> Vec<EstadoHostEjecucion> {
        registro.lista[&id]
            .hosts
            .iter()
            .map(|host| host.estado)
            .collect()
    }

    #[test]
    fn parar_al_fallo_deja_omitidos_los_que_esperan() {
        let mut registro = Ejecuciones::default();
        let id = registro.insertar(ejecucion(4, true));
        assert!(registro.marcar_conectando(id, 0, 0));
        assert!(registro.marcar_conectando(id, 1, 0));
        registro.finalizar_host(id, 1, EstadoHostEjecucion::Fallo, Some(1), None);
        assert_eq!(
            estados(&registro, id),
            vec![
                EstadoHostEjecucion::Conectando,
                EstadoHostEjecucion::Fallo,
                EstadoHostEjecucion::Omitido,
                EstadoHostEjecucion::Omitido,
            ]
        );
        // Los omitidos ya no arrancan; el que estaba en marcha termina.
        assert!(!registro.marcar_conectando(id, 2, 0));
        registro.finalizar_host(id, 0, EstadoHostEjecucion::Ok, Some(0), None);
        let cierre = registro.cerrar_si_termino(id, 10).expect("cerrada");
        assert_eq!(cierre.resultado, EjecucionResultado::Parcial);
    }

    #[test]
    fn un_host_final_no_se_finaliza_ni_se_anota_dos_veces() {
        let mut registro = Ejecuciones::default();
        let id = registro.insertar(ejecucion(1, false));
        registro.marcar_conectando(id, 0, 0);
        assert!(registro
            .finalizar_host(id, 0, EstadoHostEjecucion::Ok, Some(0), None)
            .is_some());
        assert!(registro
            .finalizar_host(id, 0, EstadoHostEjecucion::Error, None, None)
            .is_none());
        assert!(registro.cerrar_si_termino(id, 1).is_some());
        assert!(registro.cerrar_si_termino(id, 2).is_none());
    }

    #[test]
    fn el_omitido_no_se_anota() {
        let mut registro = Ejecuciones::default();
        let id = registro.insertar(ejecucion(1, false));
        assert!(registro
            .finalizar_host(id, 0, EstadoHostEjecucion::Omitido, None, None)
            .is_none());
    }

    #[test]
    fn cancelar_omite_la_cola_y_espera_a_los_que_corren() {
        let mut registro = Ejecuciones::default();
        let id = registro.insertar(ejecucion(3, false));
        registro.marcar_conectando(id, 0, 0);
        assert!(registro.cancelar(id));
        assert_eq!(
            estados(&registro, id),
            vec![
                EstadoHostEjecucion::Conectando,
                EstadoHostEjecucion::Omitido,
                EstadoHostEjecucion::Omitido,
            ]
        );
        assert!(registro.cerrar_si_termino(id, 1).is_none());
        registro.finalizar_host(id, 0, EstadoHostEjecucion::Cancelado, None, None);
        let cierre = registro.cerrar_si_termino(id, 1).unwrap();
        assert_eq!(cierre.resultado, EjecucionResultado::Cancelada);
        assert_eq!(registro.lista[&id].estado, EstadoEjecucion::Cancelada);
    }

    #[test]
    fn el_resultado_segun_los_hosts() {
        let mut registro = Ejecuciones::default();
        let id = registro.insertar(ejecucion(2, false));
        for indice in 0..2 {
            registro.marcar_conectando(id, indice, 0);
            registro.finalizar_host(id, indice, EstadoHostEjecucion::Ok, Some(0), None);
        }
        assert_eq!(
            registro.cerrar_si_termino(id, 1).unwrap().resultado,
            EjecucionResultado::Ok
        );
        let id = registro.insertar(ejecucion(1, false));
        registro.finalizar_host(id, 0, EstadoHostEjecucion::Error, None, None);
        assert_eq!(
            registro.cerrar_si_termino(id, 1).unwrap().resultado,
            EjecucionResultado::Error
        );
    }

    #[test]
    fn las_terminadas_caducan_a_la_hora_y_las_en_curso_no() {
        let mut registro = Ejecuciones::default();
        let terminada = registro.insertar(ejecucion(1, false));
        registro.finalizar_host(terminada, 0, EstadoHostEjecucion::Ok, Some(0), None);
        registro.cerrar_si_termino(terminada, 100);
        let en_curso = registro.insertar(ejecucion(1, false));
        assert_eq!(registro.purgar_caducadas(100 + RETENCION_SEG - 1), 0);
        assert_eq!(registro.purgar_caducadas(100 + RETENCION_SEG), 1);
        assert!(registro.lista.contains_key(&en_curso));
        assert_eq!(registro.limpiar(), 0);
    }

    #[test]
    fn la_salida_se_corta_al_tope_y_cuenta_los_bytes() {
        let salida = SalidaHost::default();
        salida.anotar(false, &vec![b'a'; TOPE_SALIDA - 10]);
        salida.anotar(false, &[b'b'; 20]);
        salida.anotar(true, b"error");
        let (stdout, stderr, truncada) = salida.copia();
        assert_eq!(stdout.len(), TOPE_SALIDA);
        assert_eq!(stderr, b"error");
        assert!(truncada);
        assert_eq!(salida.bytes(), (TOPE_SALIDA as u64 + 10, 5));
    }

    #[test]
    fn el_detalle_no_lleva_la_salida() {
        let mut host = EjecucionHost::nuevo(1, "hetzner-01".to_string());
        host.salida.anotar(false, b"secreto");
        host.estado = EstadoHostEjecucion::Ok;
        host.codigo = Some(0);
        let detalle = detalle_host("reiniciar nginx", &host);
        assert!(detalle.contains("«reiniciar nginx» · hetzner-01 · ok · código 0"));
        assert!(detalle.contains("7 B"));
        assert!(!detalle.contains("secreto"));
    }

    #[test]
    fn apagar_cancela_y_anota_lo_que_esta_en_marcha() {
        let mut registro = Ejecuciones::default();
        let id = registro.insertar(ejecucion(3, false));
        registro.marcar_conectando(id, 0, 0);
        registro.marcar_conectando(id, 1, 0);
        registro.marcar_ejecutando(id, 1);
        let (anotaciones, cierres) = registro.cancelar_todas(5);
        assert_eq!(anotaciones.len(), 2);
        assert_eq!(cierres.len(), 1);
        assert_eq!(cierres[0].resultado, EjecucionResultado::Cancelada);
        assert!(!registro.hay_en_curso());
    }
}
