//! Snippets y resultados del móvil (informe §4.5, §5.1 y §7.6): la lista y el
//! detalle de los snippets sincronizados, la preparación con las variables
//! escapadas, el motor de ejecuciones (semáforo de 8, conexión automática por
//! host, timeout del snippet y parar al primer fallo) y la salida de cada host.
//!
//! La ejecución reutiliza la conexión automática del gestor de conexiones y su
//! ejecutor remoto acotado (1 MiB por flujo), los estados del núcleo
//! (snippets::ejecucion) y las funciones puras de destinos y variables. La
//! deliberación del móvil aún es un punto aislado (la implementa la pista de
//! deliberación): aquí solo se exige que una ejecución que delibera traiga su
//! id, y el cierre de la deliberación queda pendiente de esa pista.

use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use magi_nucleo::deliberacion::EjecucionResultado;
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::snippets as nucleo_snippets;
use magi_nucleo::snippets::ejecucion::{
    contar_bytes, recortar_flujo, Ejecucion, EstadoHostEjecucion, HostEjecucion, CONCURRENTES,
    MAXIMO_SALIDA, VIDA_SEG,
};
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinSet;

use crate::api::{
    EjecucionMovil, ErrorNucleo, EstadoHostEjecucionMovil, HostEjecucionMovil,
    PreparacionEjecucion, ResultadoEjecucionMovil, SalidaEjecucion, VariableSnippetMovil,
};
use crate::conexiones::{self, OyenteConexion, UsoConexion};
use crate::estado::EstadoNucleo;

/// Ahora en época (segundos), para la retención de las ejecuciones.
fn ahora_epoca() -> i64 {
    magi_nucleo::modelo::fecha_ahora_epoca()
}

// ------------------------------------------------------------- deliberación

/// Una ejecución que delibera exige el id de su deliberación; el id se valida
/// y se consume después con crate::deliberacion::consumir (una deliberación
/// autoriza una sola acción, T62).
fn comprobar_deliberacion(
    _estado: &EstadoNucleo,
    requerida: bool,
    id: Option<u64>,
) -> Result<Option<u64>, ErrorNucleo> {
    if requerida && id.is_none() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "esta ejecución exige deliberación MAGI: delibera y vuelve a lanzarla"
                .to_string(),
        });
    }
    Ok(id)
}

// ------------------------------------------------------------- motor

/// Salida guardada de un host: hasta 1 MiB por flujo (lo que dejó el ejecutor
/// remoto) y si hubo que descartar algo.
#[derive(Debug, Default)]
struct SalidaHost {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncada: bool,
    stderr_truncada: bool,
}

/// Cómo acabó un host: su estado final y lo que se guarda de él.
struct FinHost {
    estado: EstadoHostEjecucion,
    codigo: Option<i32>,
    error: Option<String>,
    salida: SalidaHost,
    fin: Option<Instant>,
    /// Si hay que anotar snippet_ejecutado en REGISTRO (no para omitidos).
    anotar: bool,
}

impl FinHost {
    /// Un host que acaba sin código ni salida (error, cancelación u omisión).
    fn nuevo(estado: EstadoHostEjecucion, fin: Option<Instant>) -> Self {
        Self {
            estado,
            codigo: None,
            error: None,
            salida: SalidaHost::default(),
            fin,
            anotar: true,
        }
    }
}

/// Una ejecución en memoria: el estado común del núcleo (estados de host,
/// resultado y tiempos) más lo que solo tiene el móvil: uuid de los hosts,
/// salida acotada, inicios para medir y la cancelación.
struct EjecucionEnMemoria {
    ejecucion: Ejecucion,
    hosts_uuid: Vec<String>,
    salida: Vec<SalidaHost>,
    inicios: Vec<Option<Instant>>,
    snippet_uuid: String,
    variables: Vec<VariableSnippetMovil>,
    forzada: bool,
    cancelacion: watch::Sender<bool>,
}

impl EjecucionEnMemoria {
    /// DTO sin la salida, como lo recibe OyenteNucleo::ejecuciones.
    fn publica(&self) -> EjecucionMovil {
        let hosts = self
            .ejecucion
            .hosts
            .iter()
            .enumerate()
            .map(|(indice, host)| HostEjecucionMovil {
                host_uuid: self.hosts_uuid.get(indice).cloned().unwrap_or_default(),
                host_nombre: host.host_nombre.clone(),
                estado: mapear_estado(host.estado),
                codigo: host.codigo,
                duracion_ms: host.duracion_ms,
                bytes_stdout: host.bytes_stdout,
                bytes_stderr: host.bytes_stderr,
                error: host.error.clone(),
            })
            .collect();
        EjecucionMovil {
            id: self.ejecucion.id,
            snippet_uuid: (!self.snippet_uuid.is_empty()).then(|| self.snippet_uuid.clone()),
            snippet_nombre: self.ejecucion.snippet_nombre.clone(),
            hosts,
            timeout_seg: self.ejecucion.timeout_seg,
            parar_al_fallo: self.ejecucion.parar_al_fallo,
            deliberacion_id: self
                .ejecucion
                .deliberacion_id
                .and_then(|id| u64::try_from(id).ok()),
            creada_en: self.ejecucion.creada_en,
            terminada_en: self.ejecucion.terminada_en,
            resultado: self.ejecucion.resultado().map(mapear_resultado),
            cancelada: self.ejecucion.cancelada,
            forzada: self.forzada,
        }
    }
}

/// Registro de ejecuciones del móvil. Vive en EstadoNucleo.ejecuciones.
pub struct MotorEjecuciones {
    lista: BTreeMap<u64, EjecucionEnMemoria>,
    siguiente_id: u64,
}

impl Default for MotorEjecuciones {
    fn default() -> Self {
        Self::nuevo()
    }
}

impl MotorEjecuciones {
    pub fn nuevo() -> Self {
        Self {
            lista: BTreeMap::new(),
            siguiente_id: 0,
        }
    }

    /// Registra la ejecución y devuelve su id y el receptor de cancelación.
    fn insertar(
        &mut self,
        mut ejecucion: Ejecucion,
        hosts_uuid: Vec<String>,
        snippet_uuid: String,
        variables: Vec<VariableSnippetMovil>,
        forzada: bool,
    ) -> (u64, watch::Receiver<bool>) {
        self.siguiente_id += 1;
        let id = self.siguiente_id;
        ejecucion.id = id;
        let (cancelacion, receptor) = watch::channel(false);
        let hosts = ejecucion.hosts.len();
        self.lista.insert(
            id,
            EjecucionEnMemoria {
                ejecucion,
                hosts_uuid,
                salida: (0..hosts).map(|_| SalidaHost::default()).collect(),
                inicios: vec![None; hosts],
                snippet_uuid,
                variables,
                forzada,
                cancelacion,
            },
        );
        (id, receptor)
    }

    /// Cancela: los que esperan pasan a omitidos y los que están en marcha
    /// reciben el aviso (sus tareas cierran el canal y quedan cancelados).
    fn cancelar(&mut self, id: u64) -> bool {
        let Some(en_memoria) = self.lista.get_mut(&id) else {
            return false;
        };
        if en_memoria.ejecucion.terminada() {
            return false;
        }
        en_memoria.ejecucion.cancelada = true;
        for host in &mut en_memoria.ejecucion.hosts {
            if host.estado == EstadoHostEjecucion::EnCola {
                host.estado = EstadoHostEjecucion::Omitido;
            }
        }
        let _ = en_memoria.cancelacion.send(true);
        true
    }

    /// Olvida las ejecuciones terminadas hace más de una hora.
    fn purgar(&mut self, ahora: i64) -> usize {
        let antes = self.lista.len();
        self.lista.retain(|_, en_memoria| {
            en_memoria
                .ejecucion
                .terminada_en
                .is_none_or(|fin| ahora - fin < VIDA_SEG)
        });
        antes - self.lista.len()
    }

    /// Quita las terminadas (las en curso siguen). Devuelve cuántas.
    fn limpiar(&mut self) -> usize {
        let antes = self.lista.len();
        self.lista
            .retain(|_, en_memoria| en_memoria.ejecucion.terminada_en.is_none());
        antes - self.lista.len()
    }

    fn lista_publica(&self) -> Vec<EjecucionMovil> {
        self.lista
            .values()
            .map(EjecucionEnMemoria::publica)
            .collect()
    }

    #[cfg(test)]
    fn en_curso(&self) -> usize {
        self.lista
            .values()
            .filter(|en_memoria| en_memoria.ejecucion.terminada_en.is_none())
            .count()
    }
}

fn mapear_estado(estado: EstadoHostEjecucion) -> EstadoHostEjecucionMovil {
    match estado {
        EstadoHostEjecucion::EnCola => EstadoHostEjecucionMovil::EnCola,
        EstadoHostEjecucion::Conectando => EstadoHostEjecucionMovil::Conectando,
        EstadoHostEjecucion::Ejecutando => EstadoHostEjecucionMovil::Ejecutando,
        EstadoHostEjecucion::Ok => EstadoHostEjecucionMovil::Ok,
        EstadoHostEjecucion::Fallo => EstadoHostEjecucionMovil::Fallo,
        EstadoHostEjecucion::Error => EstadoHostEjecucionMovil::Error,
        EstadoHostEjecucion::Cancelado => EstadoHostEjecucionMovil::Cancelado,
        EstadoHostEjecucion::Omitido => EstadoHostEjecucionMovil::Omitido,
    }
}

fn mapear_resultado(resultado: EjecucionResultado) -> ResultadoEjecucionMovil {
    match resultado {
        EjecucionResultado::Ok => ResultadoEjecucionMovil::Ok,
        EjecucionResultado::Parcial => ResultadoEjecucionMovil::Parcial,
        EjecucionResultado::Error => ResultadoEjecucionMovil::Error,
        EjecucionResultado::Cancelada => ResultadoEjecucionMovil::Cancelada,
    }
}

/// Difunde la lista completa de ejecuciones al oyente (sin la salida).
fn emitir(estado: &EstadoNucleo) {
    let lista = match estado.ejecuciones.lock() {
        Ok(mut motor) => {
            motor.purgar(ahora_epoca());
            motor.lista_publica()
        }
        Err(_) => return,
    };
    estado.retrollamadas.oyente.ejecuciones(lista);
}

/// Una anotación de REGISTRO pendiente: (host_id, detalle, correcto).
type Anotacion = (i64, String, bool);

/// Detalle de snippet_ejecutado: snippet, host, estado, código, duración y
/// bytes. Nunca la salida ni el comando (T44).
fn detalle_host(snippet: &str, host: &HostEjecucion, truncada: bool) -> String {
    let mut detalle = format!(
        "«{snippet}» · {} · {}",
        host.host_nombre,
        host.estado.texto()
    );
    if let Some(codigo) = host.codigo {
        detalle.push_str(&format!(" · código {codigo}"));
    }
    if host.duracion_ms > 0 {
        detalle.push_str(&format!(
            " · {}",
            nucleo_snippets::salida::duracion_legible(host.duracion_ms as u64)
        ));
    }
    detalle.push_str(&format!(" · {} B", host.bytes_stdout + host.bytes_stderr));
    if truncada {
        detalle.push_str(" · salida truncada");
    }
    if let Some(error) = &host.error {
        detalle.push_str(&format!(
            " · {}",
            nucleo_snippets::salida::sanear_linea(error, 120)
        ));
    }
    detalle
}

/// Lleva un host a su estado final (una sola vez) y devuelve lo que hay que
/// anotar. Con «parar al primer fallo», un fallo o un error dejan omitidos los
/// que aún esperan en cola (los que ya están conectando siguen).
fn fijar_final(
    en_memoria: &mut EjecucionEnMemoria,
    indice: usize,
    fin_host: FinHost,
) -> Option<Anotacion> {
    let FinHost {
        estado: estado_final,
        codigo,
        error,
        salida,
        fin,
        anotar,
    } = fin_host;
    if en_memoria
        .ejecucion
        .hosts
        .get(indice)
        .map(|host| host.estado.es_final())
        .unwrap_or(true)
    {
        return None;
    }
    let snippet_nombre = en_memoria.ejecucion.snippet_nombre.clone();
    let inicio = en_memoria.inicios.get(indice).copied().flatten();
    let duracion_ms = inicio
        .map(|inicio| {
            let transcurrido = fin
                .unwrap_or_else(Instant::now)
                .saturating_duration_since(inicio)
                .as_millis();
            transcurrido.min(i64::MAX as u128) as i64
        })
        .unwrap_or(0);
    let bytes_stdout = contar_bytes(&salida.stdout);
    let bytes_stderr = contar_bytes(&salida.stderr);
    let truncada = salida.stdout_truncada || salida.stderr_truncada;
    let anotacion = {
        let host = &mut en_memoria.ejecucion.hosts[indice];
        host.estado = estado_final;
        host.codigo = codigo;
        host.error = error;
        host.duracion_ms = duracion_ms;
        host.bytes_stdout = bytes_stdout;
        host.bytes_stderr = bytes_stderr;
        let detalle = detalle_host(&snippet_nombre, host, truncada);
        (
            host.host_id,
            detalle,
            estado_final == EstadoHostEjecucion::Ok,
        )
    };
    en_memoria.salida[indice] = salida;
    if en_memoria.ejecucion.parar_al_fallo
        && matches!(
            estado_final,
            EstadoHostEjecucion::Fallo | EstadoHostEjecucion::Error
        )
    {
        for otro in &mut en_memoria.ejecucion.hosts {
            if otro.estado == EstadoHostEjecucion::EnCola {
                otro.estado = EstadoHostEjecucion::Omitido;
            }
        }
    }
    if anotar && estado_final != EstadoHostEjecucion::Omitido {
        Some(anotacion)
    } else {
        None
    }
}

fn anotar_host(estado: &EstadoNucleo, anotacion: Anotacion) {
    let (host_id, detalle, correcto) = anotacion;
    let _ = estado.escritor.con_almacen(move |almacen| {
        registro::anotar(
            almacen.conexion(),
            registro::SNIPPET_EJECUTADO,
            Some(host_id),
            None,
            &detalle,
            if correcto {
                ResultadoRegistro::Ok
            } else {
                ResultadoRegistro::Error
            },
        )
    });
}

// ------------------------------------------------------------ ejecutar

/// Plan fijo de una ejecución lanzada: el comando ya resuelto, los hosts y el
/// plazo. Nada de esto se relee durante la ejecución.
struct PlanEjecucion {
    comando: Arc<str>,
    /// (host_id, nombre, uuid) en orden FIFO.
    hosts: Vec<(i64, String, String)>,
    timeout: Duration,
    cancelacion: watch::Receiver<bool>,
}

/// Oyente de las caídas de conexión para una ejecución concreta: si un host se
/// cae, sus hosts en marcha acaban en error con el motivo (T97).
struct OyenteEjecucion {
    estado: Weak<EstadoNucleo>,
    ejecucion: u64,
}

impl OyenteConexion for OyenteEjecucion {
    fn conexion_caida(&self, host_uuid: &str, motivo: &str) {
        let Some(estado) = self.estado.upgrade() else {
            return;
        };
        caida_de_conexion(&estado, self.ejecucion, host_uuid, motivo);
    }
}

/// Lanza la ejecución de un snippet preparado y devuelve su id (informe §5.1
/// y §4.5). El comando se recalcula del snippet guardado: nunca se ejecuta el
/// que la app manda en la preparación.
pub async fn ejecutar(
    estado: &Arc<EstadoNucleo>,
    preparacion: PreparacionEjecucion,
    parar_al_fallo: bool,
    deliberacion: Option<u64>,
) -> Result<u64, ErrorNucleo> {
    let elegidos: Vec<String> = preparacion
        .hosts
        .iter()
        .map(|host| host.uuid.clone())
        .collect();
    let preparada = crate::snippets::preparar_interno(
        estado,
        &preparacion.snippet_uuid,
        &elegidos,
        &preparacion.variables,
    )?;
    if let Some(error) = preparada.publica.errores.first() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: format!("no se puede ejecutar: {}", error.mensaje),
        });
    }
    let deliberacion = comprobar_deliberacion(estado, preparada.publica.delibera, deliberacion)?;
    // Una deliberación vale para una sola acción: se valida contra la
    // preparación exacta que se va a ejecutar (snippet, comando y hosts) y se
    // marca usada antes de lanzar.
    if let Some(deliberacion_id) = deliberacion {
        let plan = crate::api::PlanDeliberadoMovil::Snippet {
            preparacion: preparada.publica.clone(),
        };
        crate::deliberacion::consumir(estado, deliberacion_id, &plan)?;
    }

    // usado_veces + 1 al lanzar (campo propio del móvil, no viaja).
    let snippet_id = preparada.snippet_id;
    let _ = estado
        .escritor
        .con_almacen(move |almacen| almacen.marcar_uso_snippet(snippet_id));

    let hosts_uuid = preparada.hosts_uuid();
    let ejecucion = Ejecucion {
        id: 0,
        snippet_id: Some(preparada.snippet_id),
        snippet_nombre: preparada.publica.snippet_nombre.clone(),
        comando: preparada.publica.comando.clone(),
        hosts: preparada
            .hosts
            .iter()
            .map(|(id, nombre, _)| HostEjecucion::nuevo(*id, nombre.clone()))
            .collect(),
        timeout_seg: preparada.publica.timeout_seg,
        parar_al_fallo,
        deliberacion_id: deliberacion.and_then(|id| i64::try_from(id).ok()),
        creada_en: ahora_epoca(),
        terminada_en: None,
        cancelada: false,
    };
    let (id, cancelacion) = {
        let mut motor = estado
            .ejecuciones
            .lock()
            .map_err(|_| ErrorNucleo::General {
                detalle: "estado de ejecuciones envenenado".to_string(),
            })?;
        motor.insertar(
            ejecucion,
            hosts_uuid.clone(),
            preparada.publica.snippet_uuid.clone(),
            preparada.publica.variables.clone(),
            false,
        )
    };
    emitir(estado);

    let plan = Arc::new(PlanEjecucion {
        comando: Arc::from(preparada.publica.comando.as_str()),
        hosts: preparada.hosts,
        timeout: Duration::from_secs(u64::from(preparada.publica.timeout_seg)),
        cancelacion,
    });
    let oyente: Arc<dyn OyenteConexion> = Arc::new(OyenteEjecucion {
        estado: Arc::downgrade(estado),
        ejecucion: id,
    });
    estado.suscribir_oyente_conexion(oyente.clone());
    let estado_tarea = estado.clone();
    estado.runtime.spawn(async move {
        despachar(estado_tarea, id, plan, oyente).await;
    });
    Ok(id)
}

/// Recorre los hosts en orden (FIFO) con el semáforo de ocho. A cada uno le
/// toca su turno o, si «parar al fallo» o una cancelación lo dejaron fuera, se
/// salta. Al final cierra la ejecución.
/// Semáforo de la ejecución: como mucho ocho hosts a la vez (§7.6).
fn semaforo_de_ejecuciones() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(CONCURRENTES))
}

async fn despachar(
    estado: Arc<EstadoNucleo>,
    id: u64,
    plan: Arc<PlanEjecucion>,
    oyente: Arc<dyn OyenteConexion>,
) {
    let semaforo = semaforo_de_ejecuciones();
    let mut tareas: JoinSet<()> = JoinSet::new();
    let mut cancelacion = plan.cancelacion.clone();
    for (indice, host) in plan.hosts.iter().enumerate() {
        let permiso = tokio::select! {
            permiso = semaforo.clone().acquire_owned() => match permiso {
                Ok(permiso) => permiso,
                Err(_) => break,
            },
            _ = esperar_cancelacion(&mut cancelacion) => break,
        };
        if !marcar_conectando(&estado, id, indice) {
            drop(permiso);
            continue;
        }
        emitir(&estado);
        let estado_host = estado.clone();
        let plan_host = plan.clone();
        let host = host.clone();
        tareas.spawn(async move {
            let _permiso = permiso;
            ejecutar_en_host(&estado_host, id, indice, &host, &plan_host).await;
        });
    }
    while tareas.join_next().await.is_some() {}
    estado.quitar_oyente_conexion(&oyente);
    marcar_huerfanos(&estado, id);
    cerrar_ejecucion(&estado, id);
}

/// Un host: su conexión automática (sin diálogos), el ejecutor remoto con
/// plazo y cancelación, y su estado final.
async fn ejecutar_en_host(
    estado: &Arc<EstadoNucleo>,
    id: u64,
    indice: usize,
    host: &(i64, String, String),
    plan: &PlanEjecucion,
) {
    let host_uuid = host.2.clone();
    let mut cancelacion = plan.cancelacion.clone();
    let uso: UsoConexion = tokio::select! {
        resultado = conexiones::conexion_automatica(estado, &host_uuid) => match resultado {
            Ok(uso) => uso,
            Err(error) => {
                let mut fin_host = FinHost::nuevo(EstadoHostEjecucion::Error, None);
                fin_host.error = Some(error.to_string());
                finalizar(estado, id, indice, fin_host);
                return;
            }
        },
        _ = esperar_cancelacion(&mut cancelacion) => {
            finalizar(
                estado,
                id,
                indice,
                FinHost::nuevo(EstadoHostEjecucion::Cancelado, None),
            );
            return;
        }
    };
    if !marcar_ejecutando(estado, id, indice) {
        // Se canceló mientras conectaba: el estado final ya lo puso quien
        // canceló (o el cierre).
        return;
    }
    emitir(estado);
    let resultado = conexiones::ejecutar_remoto_cancelable(
        &uso,
        &plan.comando,
        None,
        plan.timeout,
        MAXIMO_SALIDA,
        cancelacion,
    )
    .await;
    drop(uso);
    let fin = Instant::now();
    match resultado {
        Ok(salida) => {
            let guardada = SalidaHost {
                stdout: salida.stdout,
                stderr: salida.stderr,
                stdout_truncada: salida.stdout_truncada,
                stderr_truncada: salida.stderr_truncada,
            };
            match salida.codigo {
                Some(0) => finalizar(
                    estado,
                    id,
                    indice,
                    FinHost {
                        estado: EstadoHostEjecucion::Ok,
                        codigo: Some(0),
                        error: None,
                        salida: guardada,
                        fin: Some(fin),
                        anotar: true,
                    },
                ),
                Some(codigo) => finalizar(
                    estado,
                    id,
                    indice,
                    FinHost {
                        estado: EstadoHostEjecucion::Fallo,
                        codigo: i32::try_from(codigo).ok(),
                        error: None,
                        salida: guardada,
                        fin: Some(fin),
                        anotar: true,
                    },
                ),
                None => finalizar(
                    estado,
                    id,
                    indice,
                    FinHost {
                        estado: EstadoHostEjecucion::Error,
                        codigo: None,
                        error: Some("el host cerró el canal sin código de salida".to_string()),
                        salida: guardada,
                        fin: Some(fin),
                        anotar: true,
                    },
                ),
            }
        }
        Err(ErrorNucleo::Cancelado) => finalizar(
            estado,
            id,
            indice,
            FinHost::nuevo(EstadoHostEjecucion::Cancelado, Some(fin)),
        ),
        Err(error) => {
            let mut fin_host = FinHost::nuevo(EstadoHostEjecucion::Error, Some(fin));
            fin_host.error = Some(error.to_string());
            finalizar(estado, id, indice, fin_host);
        }
    }
}

/// Marca el host como conectando, si sigue en cola y la ejecución no se
/// canceló. Las tareas que esperaban turno y ya no toca se saltan.
fn marcar_conectando(estado: &EstadoNucleo, id: u64, indice: usize) -> bool {
    let Ok(mut motor) = estado.ejecuciones.lock() else {
        return false;
    };
    let Some(en_memoria) = motor.lista.get_mut(&id) else {
        return false;
    };
    if en_memoria.ejecucion.cancelada {
        return false;
    }
    let Some(host) = en_memoria.ejecucion.hosts.get_mut(indice) else {
        return false;
    };
    if host.estado != EstadoHostEjecucion::EnCola {
        return false;
    }
    host.estado = EstadoHostEjecucion::Conectando;
    en_memoria.inicios[indice] = Some(Instant::now());
    true
}

/// Pasa el host de conectando a ejecutando. Falso si dejó de tocar (se
/// canceló o quedó omitido mientras abría la conexión).
fn marcar_ejecutando(estado: &EstadoNucleo, id: u64, indice: usize) -> bool {
    let Ok(mut motor) = estado.ejecuciones.lock() else {
        return false;
    };
    let Some(en_memoria) = motor.lista.get_mut(&id) else {
        return false;
    };
    let Some(host) = en_memoria.ejecucion.hosts.get_mut(indice) else {
        return false;
    };
    if host.estado != EstadoHostEjecucion::Conectando {
        return false;
    }
    host.estado = EstadoHostEjecucion::Ejecutando;
    true
}

/// Lleva un host a su estado final, lo anota (si toca) y difunde.
fn finalizar(estado: &Arc<EstadoNucleo>, id: u64, indice: usize, fin_host: FinHost) {
    let anotacion = {
        let Ok(mut motor) = estado.ejecuciones.lock() else {
            return;
        };
        let Some(en_memoria) = motor.lista.get_mut(&id) else {
            return;
        };
        fijar_final(en_memoria, indice, fin_host)
    };
    if let Some(anotacion) = anotacion {
        anotar_host(estado, anotacion);
    }
    emitir(estado);
}

/// Un host de la ejecución perdió su conexión: los que estaban conectando o
/// ejecutando acaban en error con el motivo, y con «parar al fallo» se omiten
/// los que esperan. La tarea del host, al volver, ya no cambia el estado final.
fn caida_de_conexion(estado: &Arc<EstadoNucleo>, id: u64, host_uuid: &str, motivo: &str) {
    let anotaciones = {
        let Ok(mut motor) = estado.ejecuciones.lock() else {
            return;
        };
        let Some(en_memoria) = motor.lista.get_mut(&id) else {
            return;
        };
        let indices: Vec<usize> = en_memoria
            .hosts_uuid
            .iter()
            .enumerate()
            .filter(|(indice, uuid)| {
                uuid.as_str() == host_uuid
                    && en_memoria
                        .ejecucion
                        .hosts
                        .get(*indice)
                        .map(|host| {
                            matches!(
                                host.estado,
                                EstadoHostEjecucion::Conectando | EstadoHostEjecucion::Ejecutando
                            )
                        })
                        .unwrap_or(false)
            })
            .map(|(indice, _)| indice)
            .collect();
        let mut anotaciones = Vec::new();
        for indice in indices {
            let mut fin_host = FinHost::nuevo(EstadoHostEjecucion::Error, Some(Instant::now()));
            fin_host.error = Some(motivo.to_string());
            anotaciones.extend(fijar_final(en_memoria, indice, fin_host));
        }
        anotaciones
    };
    for anotacion in anotaciones {
        anotar_host(estado, anotacion);
    }
    emitir(estado);
    // Si la caída terminó todos los hosts, no se espera al plazo del que
    // todavía corre: la ejecución se cierra ya y deja de estar «en curso».
    cerrar_ejecucion(estado, id);
}

/// Si una tarea de host se perdió (pánico), no deja la ejecución colgada: lo
/// que quedó a medias acaba en error y lo que esperaba, omitido.
fn marcar_huerfanos(estado: &EstadoNucleo, id: u64) {
    let anotaciones = {
        let Ok(mut motor) = estado.ejecuciones.lock() else {
            return;
        };
        let Some(en_memoria) = motor.lista.get_mut(&id) else {
            return;
        };
        let indices: Vec<usize> = en_memoria
            .ejecucion
            .hosts
            .iter()
            .enumerate()
            .filter(|(_, host)| !host.estado.es_final())
            .map(|(indice, _)| indice)
            .collect();
        let mut anotaciones = Vec::new();
        for indice in indices {
            let estado_actual = en_memoria.ejecucion.hosts[indice].estado;
            let mut fin_host = match estado_actual {
                EstadoHostEjecucion::EnCola => {
                    let mut fin_host = FinHost::nuevo(EstadoHostEjecucion::Omitido, None);
                    fin_host.anotar = false;
                    fin_host
                }
                _ => {
                    let mut fin_host =
                        FinHost::nuevo(EstadoHostEjecucion::Error, Some(Instant::now()));
                    fin_host.error = Some("la ejecución se interrumpió".to_string());
                    fin_host
                }
            };
            fin_host.fin = Some(Instant::now());
            anotaciones.extend(fijar_final(en_memoria, indice, fin_host));
        }
        anotaciones
    };
    for anotacion in anotaciones {
        anotar_host(estado, anotacion);
    }
}

/// Cierra la ejecución si todos sus hosts terminaron: fija terminada_en, hace
/// el cierre de la deliberación (punto aislado) y difunde.
fn cerrar_ejecucion(estado: &Arc<EstadoNucleo>, id: u64) {
    let cierre = {
        let Ok(mut motor) = estado.ejecuciones.lock() else {
            return;
        };
        let Some(en_memoria) = motor.lista.get_mut(&id) else {
            return;
        };
        if en_memoria.ejecucion.terminada_en.is_some() || !en_memoria.ejecucion.terminada() {
            return;
        }
        en_memoria.ejecucion.terminada_en = Some(ahora_epoca());
        (
            en_memoria.ejecucion.deliberacion_id,
            en_memoria.ejecucion.resultado(),
            en_memoria.forzada,
        )
    };
    // El efecto ya se produjo: el cierre de la deliberación (ejecucion_resultado
    // y anotación deliberacion_*) se guarda antes de difundir la lista, para
    // que una retrollamada que fallara no se lleve por delante el resultado
    // (aviso de revisión adversarial: pérdida de datos del cierre). Si la fila
    // ya no está o ya se cerró, no hay nada que hacer.
    if let (Some(deliberacion), Some(resultado)) = (cierre.0, cierre.1) {
        if let Ok(deliberacion) = u64::try_from(deliberacion) {
            let _ = crate::deliberacion::registrar_resultado(estado, deliberacion, resultado);
        }
    }
    emitir(estado);
}

/// Espera a que la cancelación pase a true, también si ya lo estaba al
/// empezar.
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

// ------------------------------------------------------ órdenes de la app

/// Ejecuciones en curso y terminadas, sin la salida (informe §5.1).
pub fn listar(estado: &EstadoNucleo) -> Result<Vec<EjecucionMovil>, ErrorNucleo> {
    let mut motor = estado
        .ejecuciones
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "estado de ejecuciones envenenado".to_string(),
        })?;
    motor.purgar(ahora_epoca());
    Ok(motor.lista_publica())
}

/// Salida de un host de una ejecución, sin secuencias de escape (informe
/// §6.9). La lista pública no la lleva: se pide aparte.
pub fn salida(
    estado: &EstadoNucleo,
    ejecucion: u64,
    host_uuid: &str,
) -> Result<SalidaEjecucion, ErrorNucleo> {
    let motor = estado
        .ejecuciones
        .lock()
        .map_err(|_| ErrorNucleo::General {
            detalle: "estado de ejecuciones envenenado".to_string(),
        })?;
    let en_memoria = motor
        .lista
        .get(&ejecucion)
        .ok_or_else(|| ErrorNucleo::NoEncontrado {
            detalle: format!("la ejecución {ejecucion}"),
        })?;
    let indice = en_memoria
        .hosts_uuid
        .iter()
        .position(|uuid| uuid == host_uuid)
        .ok_or_else(|| ErrorNucleo::NoEncontrado {
            detalle: format!("el host {host_uuid} en la ejecución {ejecucion}"),
        })?;
    let host = &en_memoria.ejecucion.hosts[indice];
    let guardada = &en_memoria.salida[indice];
    let (stdout, truncada_stdout) = recortar_flujo(&guardada.stdout);
    let (stderr, truncada_stderr) = recortar_flujo(&guardada.stderr);
    Ok(SalidaEjecucion {
        host_uuid: host_uuid.to_string(),
        host_nombre: host.host_nombre.clone(),
        codigo: host.codigo,
        stdout,
        stderr,
        stdout_truncada: truncada_stdout || guardada.stdout_truncada,
        stderr_truncada: truncada_stderr || guardada.stderr_truncada,
        duracion_ms: host.duracion_ms,
    })
}

/// Cancela una ejecución en curso (informe §5.1).
pub fn cancelar(estado: &EstadoNucleo, id: u64) -> Result<(), ErrorNucleo> {
    {
        let mut motor = estado
            .ejecuciones
            .lock()
            .map_err(|_| ErrorNucleo::General {
                detalle: "estado de ejecuciones envenenado".to_string(),
            })?;
        if !motor.lista.contains_key(&id) {
            return Err(ErrorNucleo::NoEncontrado {
                detalle: format!("la ejecución {id}"),
            });
        }
        motor.cancelar(id);
    }
    emitir(estado);
    Ok(())
}

/// Prepara otra vez una ejecución terminada; volverá a deliberar si hace falta
/// (informe §5.1).
pub fn repetir(estado: &EstadoNucleo, id: u64) -> Result<PreparacionEjecucion, ErrorNucleo> {
    let (snippet_uuid, hosts, variables) = {
        let motor = estado
            .ejecuciones
            .lock()
            .map_err(|_| ErrorNucleo::General {
                detalle: "estado de ejecuciones envenenado".to_string(),
            })?;
        let en_memoria = motor
            .lista
            .get(&id)
            .ok_or_else(|| ErrorNucleo::NoEncontrado {
                detalle: format!("la ejecución {id}"),
            })?;
        (
            en_memoria.snippet_uuid.clone(),
            en_memoria.hosts_uuid.clone(),
            en_memoria.variables.clone(),
        )
    };
    crate::snippets::preparar_interno(estado, &snippet_uuid, &hosts, &variables)
        .map(|preparada| preparada.publica)
}

/// Olvida las ejecuciones terminadas (las en curso siguen).
pub fn limpiar(estado: &EstadoNucleo) -> Result<(), ErrorNucleo> {
    {
        let mut motor = estado
            .ejecuciones
            .lock()
            .map_err(|_| ErrorNucleo::General {
                detalle: "estado de ejecuciones envenenado".to_string(),
            })?;
        motor.limpiar();
    }
    emitir(estado);
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ejecucion_de_prueba(hosts: usize, parar_al_fallo: bool) -> Ejecucion {
        Ejecucion {
            id: 0,
            snippet_id: Some(1),
            snippet_nombre: "reiniciar nginx".to_string(),
            comando: "systemctl restart nginx".to_string(),
            hosts: (0..hosts)
                .map(|indice| {
                    HostEjecucion::nuevo(indice as i64 + 1, format!("web-0{}", indice + 1))
                })
                .collect(),
            timeout_seg: 60,
            parar_al_fallo,
            deliberacion_id: None,
            creada_en: 0,
            terminada_en: None,
            cancelada: false,
        }
    }

    fn insertar(motor: &mut MotorEjecuciones, hosts: usize, parar_al_fallo: bool) -> u64 {
        let uuids = (0..hosts)
            .map(|indice| format!("uuid-{}", indice + 1))
            .collect();
        let (id, _receptor) = motor.insertar(
            ejecucion_de_prueba(hosts, parar_al_fallo),
            uuids,
            "snippet-uuid".to_string(),
            Vec::new(),
            false,
        );
        id
    }

    #[test]
    fn el_motor_publica_los_hosts_en_cola_sin_resultado() {
        let mut motor = MotorEjecuciones::nuevo();
        let id = insertar(&mut motor, 2, false);
        let lista = motor.lista_publica();
        assert_eq!(lista.len(), 1);
        let ejecucion = &lista[0];
        assert_eq!(ejecucion.id, id);
        assert_eq!(ejecucion.snippet_uuid.as_deref(), Some("snippet-uuid"));
        assert_eq!(ejecucion.hosts.len(), 2);
        assert!(ejecucion
            .hosts
            .iter()
            .all(|host| host.estado == EstadoHostEjecucionMovil::EnCola));
        assert_eq!(ejecucion.hosts[0].host_uuid, "uuid-1");
        assert!(ejecucion.resultado.is_none());
        assert!(ejecucion.terminada_en.is_none());
        assert_eq!(motor.en_curso(), 1);
    }

    #[test]
    fn parar_al_fallo_omite_los_que_esperan_sin_tocar_los_que_estan_en_marcha() {
        let mut motor = MotorEjecuciones::nuevo();
        let id = insertar(&mut motor, 4, true);
        let en_memoria = motor.lista.get_mut(&id).expect("ejecución");
        en_memoria.ejecucion.hosts[2].estado = EstadoHostEjecucion::Ejecutando;
        let anotacion = fijar_final(
            en_memoria,
            1,
            FinHost {
                estado: EstadoHostEjecucion::Fallo,
                codigo: Some(1),
                error: None,
                salida: SalidaHost::default(),
                fin: None,
                anotar: true,
            },
        );
        assert!(anotacion.is_some());
        assert_eq!(
            en_memoria.ejecucion.hosts[0].estado,
            EstadoHostEjecucion::Omitido
        );
        assert_eq!(
            en_memoria.ejecucion.hosts[1].estado,
            EstadoHostEjecucion::Fallo
        );
        assert_eq!(
            en_memoria.ejecucion.hosts[2].estado,
            EstadoHostEjecucion::Ejecutando
        );
        assert_eq!(
            en_memoria.ejecucion.hosts[3].estado,
            EstadoHostEjecucion::Omitido
        );
        assert!(!en_memoria.ejecucion.terminada());
    }

    #[test]
    fn un_host_no_cambia_dos_veces_de_estado_final() {
        let mut motor = MotorEjecuciones::nuevo();
        let id = insertar(&mut motor, 1, false);
        let en_memoria = motor.lista.get_mut(&id).expect("ejecución");
        assert!(fijar_final(
            en_memoria,
            0,
            FinHost {
                estado: EstadoHostEjecucion::Ok,
                codigo: Some(0),
                error: None,
                salida: SalidaHost::default(),
                fin: None,
                anotar: true,
            },
        )
        .is_some());
        assert!(fijar_final(
            en_memoria,
            0,
            FinHost {
                estado: EstadoHostEjecucion::Error,
                codigo: None,
                error: Some("tarde".to_string()),
                salida: SalidaHost::default(),
                fin: None,
                anotar: true,
            },
        )
        .is_none());
        assert_eq!(
            en_memoria.ejecucion.hosts[0].estado,
            EstadoHostEjecucion::Ok
        );
        assert!(en_memoria.ejecucion.terminada());
        assert_eq!(
            en_memoria.ejecucion.resultado(),
            Some(EjecucionResultado::Ok)
        );
    }

    #[test]
    fn cancelar_omite_la_cola_marca_cancelada_y_avisa() {
        let mut motor = MotorEjecuciones::nuevo();
        let uuids = vec!["uuid-1".to_string(), "uuid-2".to_string()];
        let (id, receptor) = motor.insertar(
            ejecucion_de_prueba(2, false),
            uuids,
            "snippet-uuid".to_string(),
            Vec::new(),
            false,
        );
        motor.lista.get_mut(&id).expect("ejecución").ejecucion.hosts[1].estado =
            EstadoHostEjecucion::Ejecutando;
        assert!(motor.cancelar(id));
        let en_memoria = motor.lista.get(&id).expect("ejecución");
        assert!(en_memoria.ejecucion.cancelada);
        assert_eq!(
            en_memoria.ejecucion.hosts[0].estado,
            EstadoHostEjecucion::Omitido
        );
        assert_eq!(
            en_memoria.ejecucion.hosts[1].estado,
            EstadoHostEjecucion::Ejecutando
        );
        assert!(*receptor.borrow());
        // Cancelar una ejecución ya terminada no hace nada.
        {
            let en_memoria = motor.lista.get_mut(&id).expect("ejecución");
            en_memoria.ejecucion.hosts[1].estado = EstadoHostEjecucion::Cancelado;
            en_memoria.ejecucion.terminada_en = Some(1);
        }
        assert!(!motor.cancelar(id));
    }

    #[test]
    fn limpiar_quita_las_terminadas_y_purgar_las_viejas() {
        let mut motor = MotorEjecuciones::nuevo();
        let terminada = insertar(&mut motor, 1, false);
        let en_curso = insertar(&mut motor, 1, false);
        {
            let en_memoria = motor.lista.get_mut(&terminada).expect("terminada");
            en_memoria.ejecucion.hosts[0].estado = EstadoHostEjecucion::Ok;
            en_memoria.ejecucion.terminada_en = Some(100);
        }
        assert_eq!(motor.en_curso(), 1);
        assert_eq!(motor.purgar(100 + VIDA_SEG - 1), 0);
        assert_eq!(motor.purgar(100 + VIDA_SEG), 1);
        assert_eq!(motor.en_curso(), 1);
        assert_eq!(motor.limpiar(), 0);
        {
            let en_memoria = motor.lista.get_mut(&en_curso).expect("en curso");
            en_memoria.ejecucion.hosts[0].estado = EstadoHostEjecucion::Ok;
            en_memoria.ejecucion.terminada_en = Some(200);
        }
        assert_eq!(motor.limpiar(), 1);
        assert!(motor.lista_publica().is_empty());
    }

    #[tokio::test]
    async fn el_semaforo_no_deja_pasar_a_mas_de_ocho_hosts_a_la_vez() {
        let semaforo = semaforo_de_ejecuciones();
        assert_eq!(semaforo.available_permits(), CONCURRENTES);
        let mut permisos = Vec::new();
        for _ in 0..CONCURRENTES {
            permisos.push(
                semaforo
                    .clone()
                    .try_acquire_owned()
                    .expect("debe caber el octavo"),
            );
        }
        assert!(
            semaforo.clone().try_acquire_owned().is_err(),
            "el noveno no puede entrar"
        );
        drop(permisos.pop());
        assert!(semaforo.try_acquire_owned().is_ok());
    }

    #[test]
    fn el_detalle_de_registro_no_lleva_la_salida_ni_el_comando() {
        let mut host = HostEjecucion::nuevo(7, "web-01".to_string());
        host.estado = EstadoHostEjecucion::Fallo;
        host.codigo = Some(2);
        host.duracion_ms = 1_500;
        host.bytes_stdout = 120;
        host.bytes_stderr = 8;
        host.error = Some("la conexión se ha perdido".to_string());
        let detalle = detalle_host("reiniciar nginx", &host, true);
        assert!(detalle.contains("«reiniciar nginx»"));
        assert!(detalle.contains("web-01"));
        assert!(detalle.contains("fallo"));
        assert!(detalle.contains("código 2"));
        assert!(detalle.contains("1,5 s"));
        assert!(detalle.contains("128 B"));
        assert!(detalle.contains("salida truncada"));
        assert!(!detalle.contains("systemctl"));
    }

    #[test]
    fn los_estados_y_resultados_se_mapean_al_contrato() {
        assert_eq!(
            mapear_estado(EstadoHostEjecucion::EnCola),
            EstadoHostEjecucionMovil::EnCola
        );
        assert_eq!(
            mapear_estado(EstadoHostEjecucion::Error),
            EstadoHostEjecucionMovil::Error
        );
        assert_eq!(
            mapear_resultado(EjecucionResultado::Parcial),
            ResultadoEjecucionMovil::Parcial
        );
        assert_eq!(
            mapear_resultado(EjecucionResultado::Cancelada),
            ResultadoEjecucionMovil::Cancelada
        );
    }

    #[test]
    fn la_preparacion_escapa_las_variables_y_marca_la_vacia() {
        // Sin base de datos no se puede preparar de verdad; al menos las
        // funciones puras del núcleo que usa la preparación se comportan como
        // espera el motor.
        let mut valores = BTreeMap::new();
        valores.insert("red".to_string(), "prod; rm -rf /".to_string());
        let comando =
            nucleo_snippets::variables::sustituir("echo {{red}}", &valores).expect("sustitución");
        assert_eq!(comando, "echo 'prod; rm -rf /'");
        let variable = nucleo_snippets::variables::Variable {
            nombre: "sin_defecto".to_string(),
            defecto: None,
        };
        assert!(nucleo_snippets::variables::valor_efectivo(&variable, "").is_err());
    }
}
