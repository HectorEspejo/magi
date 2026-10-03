//! Diálogo de deliberación MAGI (Fase 6): las comprobaciones de cada host en
//! paralelo con plazo duro, el consenso por unanimidad, `Ctrl+K` como única
//! tecla que ejecuta y `f` para forzar con un motivo que queda registrado.
//!
//! La deliberación vive en `App.deliberacion` (no en el hueco de diálogos):
//! una pregunta del servidor puede taparla un momento sin perderla. Lleva
//! fijado el plan desde que se abre (T33); los resultados de otra
//! deliberación (otro `token`) se descartan. El plan es un snippet (F6) o una
//! sincronización de directorio (F8, `snippet_id` NULL en `DELIBERACIONES`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};

use crate::deliberacion::estado::{consenso, AccionMaquina, Consenso, MaquinaDeliberacion};
use crate::deliberacion::salud::{self, DecisionSalud};
use crate::deliberacion::{
    self, Comprobacion, ComprobacionesHost, Fuentes, Limites, NuevaDeliberacion,
    ResultadoComprobacion, ResultadoDeliberacion, Trabajo, Veredicto,
};
use crate::modelo::{Host, ResultadoRegistro};
use crate::protocolo::DeliberacionLanzada;
use crate::snippets::MotivoDeliberacion;
use crate::ui::disposicion::Lista;

use super::lanzar::{ModoLanzamiento, PlanEjecucion};
use super::sincronizar::PlanSincronizacion;
use super::{App, Evento};

/// Largo máximo del dato de una celda al guardarlo (viene del remoto o de un
/// comando local: se sanea y se acota antes de pintarlo o guardarlo).
const LARGO_DETALLE: usize = 120;

/// Identificador de cada deliberación abierta en esta ventana.
static SIGUIENTE_TOKEN: AtomicU64 = AtomicU64::new(1);

/// Las comprobaciones en marcha: se abortan al cerrar la deliberación (el
/// proceso de CASPER-3 muere con su guardia).
struct TareasComprobacion(Vec<tokio::task::AbortHandle>);

impl Drop for TareasComprobacion {
    fn drop(&mut self) {
        for tarea in &self.0 {
            tarea.abort();
        }
    }
}

/// Lo que se delibera: la ejecución de un snippet (F6) o una sincronización
/// de directorio (F8). Fijado al abrir la deliberación (T33).
#[derive(Debug, Clone, PartialEq)]
pub enum PlanDeliberado {
    Snippet(PlanEjecucion),
    Sincronizacion(Box<PlanSincronizacion>),
}

impl From<PlanEjecucion> for PlanDeliberado {
    fn from(plan: PlanEjecucion) -> Self {
        PlanDeliberado::Snippet(plan)
    }
}

impl From<PlanSincronizacion> for PlanDeliberado {
    fn from(plan: PlanSincronizacion) -> Self {
        PlanDeliberado::Sincronizacion(Box::new(plan))
    }
}

impl PlanDeliberado {
    /// La acción que queda en `DELIBERACIONES`: «reiniciar nginx →
    /// hetzner-01, hetzner-02» o «sync web-prod → hetzner-01: 15 ficheros, 2
    /// borrados».
    pub fn accion(&self) -> String {
        match self {
            PlanDeliberado::Snippet(plan) => plan.accion(),
            PlanDeliberado::Sincronizacion(plan) => plan.accion(false),
        }
    }

    /// Hosts que se comprueban, en el orden del plan.
    pub fn hosts(&self) -> Vec<(i64, String)> {
        match self {
            PlanDeliberado::Snippet(plan) => plan.hosts.clone(),
            PlanDeliberado::Sincronizacion(plan) => vec![plan.host()],
        }
    }

    /// El snippet que se ejecuta; ninguno en una sincronización.
    pub fn snippet_id(&self) -> Option<i64> {
        match self {
            PlanDeliberado::Snippet(plan) => Some(plan.snippet_id),
            PlanDeliberado::Sincronizacion(_) => None,
        }
    }

    /// La acción del diálogo: «reiniciar nginx → 3 hosts (en pestaña)» o la
    /// de la sincronización entera.
    pub fn accion_visible(&self, ascii: bool) -> String {
        match self {
            PlanDeliberado::Snippet(plan) => {
                let destino = match plan.hosts.as_slice() {
                    [(_, host)] => host.clone(),
                    hosts => format!("{} hosts", hosts.len()),
                };
                let pestana = if plan.modo == ModoLanzamiento::Pestanas {
                    " (en pestaña)"
                } else {
                    ""
                };
                format!(
                    "{} {} {destino}{pestana}",
                    plan.nombre,
                    if ascii { "->" } else { "→" }
                )
            }
            PlanDeliberado::Sincronizacion(plan) => plan.accion(ascii),
        }
    }

    /// La acción del título compacto: «reiniciar nginx → 3» o «sync web-prod
    /// → hetzner-01».
    pub fn accion_compacta(&self, ascii: bool) -> String {
        match self {
            PlanDeliberado::Snippet(plan) => {
                let destino = match plan.hosts.as_slice() {
                    [(_, host)] => host.clone(),
                    hosts => hosts.len().to_string(),
                };
                format!(
                    "{} {} {destino}",
                    plan.nombre,
                    if ascii { "->" } else { "→" }
                )
            }
            PlanDeliberado::Sincronizacion(plan) => plan.accion_corta(ascii),
        }
    }
}

/// Una deliberación abierta.
pub struct DeliberacionAbierta {
    pub token: u64,
    pub plan: PlanDeliberado,
    /// Por qué hay que deliberar («crítico», «3 hosts»…).
    pub motivos: Vec<String>,
    /// Una fila por host, en el orden del plan.
    pub filas: Vec<ComprobacionesHost>,
    pub maquina: MaquinaDeliberacion,
    /// Fila seleccionada (`↑` `↓` recorren los hosts si no caben).
    pub seleccion: usize,
    /// Primera fila de hosts visible en el último pintado (el diálogo la
    /// recalcula en cada pintado a partir de esta y de la selección).
    pub desplazamiento: usize,
    pub backup_horas: u64,
    pub inicio: Instant,
    _tareas: TareasComprobacion,
}

impl DeliberacionAbierta {
    pub fn consenso(&self) -> Consenso {
        consenso(self.filas.iter().flat_map(|fila| {
            Comprobacion::TODAS
                .iter()
                .map(move |comprobacion| fila.veredicto(*comprobacion))
        }))
    }

    fn celda(&mut self, host_id: i64, comprobacion: Comprobacion) -> Option<&mut Veredicto> {
        let fila = self.filas.iter_mut().find(|fila| fila.host_id == host_id)?;
        Some(match comprobacion {
            Comprobacion::Salud => &mut fila.salud,
            Comprobacion::Backup => &mut fila.backup,
            Comprobacion::Tests => &mut fila.tests,
        })
    }

    /// Con todas las celdas resueltas pasa a aprobada o bloqueada.
    fn resolver(&mut self) {
        let consenso = self.consenso();
        self.maquina.resolver(&consenso);
    }

    /// La fila que se inserta en `DELIBERACIONES` al resolver.
    fn nueva(
        &self,
        snippet_id: Option<i64>,
        resultado: ResultadoDeliberacion,
        motivo: Option<String>,
    ) -> NuevaDeliberacion {
        NuevaDeliberacion {
            snippet_id,
            accion: self.plan.accion(),
            hosts: self.plan.hosts(),
            comprobaciones: self.filas.clone(),
            resultado,
            bloqueada: self.consenso().rechazos > 0,
            motivo,
            usuario: deliberacion::usuario_del_sistema(),
        }
    }
}

/// Qué hace falta para una celda: ya decidida, o un trabajo que lanzar.
enum Celda {
    Decidida(Veredicto),
    Trabajo(Trabajo),
}

/// Veredicto con el dato saneado (texto de un remoto o de un comando local).
fn saneado(veredicto: Veredicto) -> Veredicto {
    let limpiar = |detalle: String| crate::snippets::salida::sanear_linea(&detalle, LARGO_DETALLE);
    match veredicto {
        Veredicto::Aprueba { detalle, ms } => Veredicto::Aprueba {
            detalle: limpiar(detalle),
            ms,
        },
        Veredicto::Rechaza { detalle, ms } => Veredicto::Rechaza {
            detalle: limpiar(detalle),
            ms,
        },
        otro => otro,
    }
}

/// Mueve la fila seleccionada `paso` filas, sin salirse de la tabla.
fn mover_seleccion(abierta: &mut DeliberacionAbierta, paso: i64) {
    let ultima = abierta.filas.len().saturating_sub(1) as i64;
    abierta.seleccion = (abierta.seleccion as i64 + paso).clamp(0, ultima) as usize;
}

fn rechazo(detalle: &str) -> Celda {
    Celda::Decidida(Veredicto::Rechaza {
        detalle: detalle.to_string(),
        ms: 0,
    })
}

impl App {
    /// El plan necesita deliberación (crítico, más de un host, hosts con
    /// verificaciones o una sincronización que borra): abre el diálogo MAGI y
    /// lanza las comprobaciones.
    pub(super) fn abrir_deliberacion(
        &mut self,
        plan: impl Into<PlanDeliberado>,
        motivos: Vec<MotivoDeliberacion>,
    ) {
        let plan = plan.into();
        if self.deliberacion.is_some() {
            self.mensaje("ya hay una deliberación MAGI abierta", true);
            return;
        }
        let verificaciones = match self.almacen.verificaciones_por_host() {
            Ok(verificaciones) => verificaciones,
            Err(error) => {
                self.mensaje(
                    format!("no se pudieron leer las verificaciones previas: {error}"),
                    true,
                );
                return;
            }
        };
        let limites = Limites::desde_config(&self.config.deliberacion);
        let umbrales = self.config.flota.umbrales.clone();
        let todos: HashMap<i64, Host> = self
            .hosts
            .iter()
            .cloned()
            .map(|host| (host.id, host))
            .collect();
        let mut filas = Vec::new();
        let mut trabajos = Vec::new();
        for (host_id, nombre) in &plan.hosts() {
            let verificacion = verificaciones.get(host_id).cloned().unwrap_or_default();
            let mut celdas = Vec::new();
            for comprobacion in Comprobacion::TODAS {
                let celda = if !verificacion.activa(comprobacion) {
                    Celda::Decidida(Veredicto::NoActiva)
                } else {
                    match comprobacion {
                        Comprobacion::Salud => match salud::con_ultimo(
                            self.sondeos.get(host_id),
                            limites.salud_max,
                            &umbrales,
                        ) {
                            DecisionSalud::Aprueba(detalle) => {
                                Celda::Decidida(Veredicto::Aprueba { detalle, ms: 0 })
                            }
                            DecisionSalud::Rechaza(detalle) => {
                                Celda::Decidida(Veredicto::Rechaza { detalle, ms: 0 })
                            }
                            DecisionSalud::Sondear => match todos.get(host_id) {
                                Some(host) => Celda::Trabajo(Trabajo::Salud(Box::new(
                                    self.peticion_de_sondeo(host.clone(), &todos),
                                ))),
                                None => rechazo("el host ya no existe"),
                            },
                        },
                        Comprobacion::Backup => match verificacion.backup_ruta.clone() {
                            Some(ruta) => Celda::Trabajo(Trabajo::Backup {
                                ruta,
                                patron: verificacion.backup_patron.clone(),
                            }),
                            None => rechazo("sin ruta de backup configurada"),
                        },
                        Comprobacion::Tests => match verificacion.tests_comando.clone() {
                            Some(comando) => Celda::Trabajo(Trabajo::Tests {
                                comando,
                                dir: self.rutas.hogar.clone(),
                            }),
                            None => rechazo("sin comando de tests configurado"),
                        },
                    }
                };
                celdas.push(match celda {
                    Celda::Decidida(veredicto) => saneado(veredicto),
                    Celda::Trabajo(trabajo) => {
                        trabajos.push((*host_id, comprobacion, trabajo));
                        Veredicto::Pendiente
                    }
                });
            }
            let mut celdas = celdas.into_iter();
            filas.push(ComprobacionesHost {
                host_id: *host_id,
                host: nombre.clone(),
                salud: celdas.next().unwrap_or(Veredicto::NoActiva),
                backup: celdas.next().unwrap_or(Veredicto::NoActiva),
                tests: celdas.next().unwrap_or(Veredicto::NoActiva),
            });
        }
        let token = SIGUIENTE_TOKEN.fetch_add(1, Ordering::Relaxed);
        let eventos = self.eventos_tx.clone();
        let enviar: Arc<dyn Fn(ResultadoComprobacion) + Send + Sync> = Arc::new(move |resultado| {
            let _ = eventos.send(Evento::Deliberacion(resultado));
        });
        let tareas = deliberacion::lanzar(
            self.runtime.handle(),
            token,
            trabajos,
            Fuentes::reales(self.servidor.clone()),
            limites,
            umbrales,
            tokio::time::Instant::now() + limites.limite,
            enviar,
        );
        let mut abierta = DeliberacionAbierta {
            token,
            plan,
            motivos: motivos.iter().map(MotivoDeliberacion::texto).collect(),
            filas,
            maquina: MaquinaDeliberacion::nueva(limites.motivo_min),
            seleccion: 0,
            desplazamiento: 0,
            backup_horas: limites.backup_horas,
            inicio: Instant::now(),
            _tareas: TareasComprobacion(tareas),
        };
        // Sin nada que esperar (todo decidido o nada activo) ya se resuelve.
        abierta.resolver();
        self.deliberacion = Some(abierta);
    }

    /// Resultado de una comprobación. El sondeo nuevo se guarda como
    /// cualquier otro (aunque la deliberación ya se haya cerrado).
    pub(super) fn resultado_deliberacion(&mut self, resultado: ResultadoComprobacion) {
        if let Some(sondeo) = resultado.sondeo {
            self.registrar_sondeo(sondeo);
        }
        let Some(abierta) = self.deliberacion.as_mut() else {
            return;
        };
        if abierta.token != resultado.token {
            return;
        }
        if let Some(celda) = abierta.celda(resultado.host_id, resultado.comprobacion) {
            if *celda == Veredicto::Pendiente {
                *celda = saneado(resultado.veredicto);
            }
        }
        abierta.resolver();
    }

    /// Teclas con la deliberación abierta: solo `Ctrl+K` ejecuta. `↑` `↓`
    /// recorren los hosts y `PgUp` `PgDn` los pasan de página en página
    /// (las filas que se ven en el último pintado).
    pub(super) fn tecla_deliberacion(&mut self, tecla: KeyEvent) {
        let ventana = self.disposicion.lista(Lista::DeliberacionHosts);
        let pagina = self.disposicion.filas(Lista::DeliberacionHosts) as i64;
        let Some(abierta) = self.deliberacion.as_mut() else {
            return;
        };
        // Las teclas parten de lo que se ve: el pintado pudo mover la ventana
        // (otro tamaño de terminal).
        if let Some(ventana) = ventana {
            abierta.desplazamiento = ventana.inicio;
        }
        let pagina = match tecla.code {
            KeyCode::PageUp => Some(-pagina),
            KeyCode::PageDown => Some(pagina),
            _ => None,
        };
        if let Some(paso) = pagina {
            mover_seleccion(abierta, paso);
            return;
        }
        match abierta.maquina.pulsar(&tecla) {
            AccionMaquina::Nada => {}
            AccionMaquina::Ayuda => self.ayuda = true,
            AccionMaquina::Mover(paso) => mover_seleccion(abierta, i64::from(paso)),
            AccionMaquina::Cancelar => self.cancelar_deliberacion(),
            AccionMaquina::Ejecutar { forzada, motivo } => {
                self.confirmar_deliberacion(forzada, motivo)
            }
        }
    }

    /// El snippet del plan si aún existe: si se borró con el diálogo abierto,
    /// la fila de `DELIBERACIONES` conserva la acción en texto (7.9).
    fn snippet_vigente(&self, snippet_id: i64) -> Option<i64> {
        self.almacen
            .obtener_snippet(snippet_id)
            .ok()
            .map(|snippet| snippet.id)
    }

    /// El snippet del plan si aún existe (ninguno en una sincronización).
    fn snippet_del_plan(&self, plan: &PlanDeliberado) -> Option<i64> {
        plan.snippet_id()
            .and_then(|snippet_id| self.snippet_vigente(snippet_id))
    }

    /// `Ctrl+K` con consenso o forzada: inserta la fila y lanza el plan con
    /// la deliberación que lo autoriza.
    fn confirmar_deliberacion(&mut self, forzada: bool, motivo: Option<String>) {
        let Some(abierta) = self.deliberacion.take() else {
            return;
        };
        let resultado = if forzada {
            ResultadoDeliberacion::Forzada
        } else {
            ResultadoDeliberacion::Aprobada
        };
        let snippet_id = self.snippet_del_plan(&abierta.plan);
        let nueva = abierta.nueva(snippet_id, resultado, motivo.clone());
        match self.almacen.crear_deliberacion(&nueva) {
            Ok(id) => {
                let plan = abierta.plan.clone();
                drop(abierta);
                let deliberacion = Some(DeliberacionLanzada {
                    id,
                    forzada,
                    motivo,
                });
                match plan {
                    PlanDeliberado::Snippet(plan) => self.lanzar_plan(plan, deliberacion),
                    PlanDeliberado::Sincronizacion(plan) => {
                        self.lanzar_sincronizacion(*plan, deliberacion)
                    }
                }
            }
            Err(error) => {
                // Sin fila el servidor rechazaría la ejecución: no se lanza.
                self.mensaje(
                    format!(
                        "no se pudo registrar la deliberación: {error}; no se ha ejecutado nada"
                    ),
                    true,
                );
            }
        }
    }

    /// `Esc`: fila con resultado «cancelada» y `deliberacion_cancelada` en el
    /// registro. No se ejecuta nada.
    fn cancelar_deliberacion(&mut self) {
        let Some(abierta) = self.deliberacion.take() else {
            return;
        };
        let snippet_id = self.snippet_del_plan(&abierta.plan);
        let nueva = abierta.nueva(snippet_id, ResultadoDeliberacion::Cancelada, None);
        drop(abierta);
        match self.almacen.crear_deliberacion(&nueva) {
            Ok(id) => {
                let detalle = match self.almacen.obtener_deliberacion(id) {
                    Ok(fila) => deliberacion::detalle_registro(&fila, None),
                    Err(_) => format!("deliberación #{id} · {}", nueva.accion),
                };
                self.anotar(
                    crate::registro::DELIBERACION_CANCELADA,
                    None,
                    None,
                    &detalle,
                    ResultadoRegistro::Ok,
                );
                self.mensaje("deliberación cancelada: no se ha ejecutado nada", false);
            }
            Err(error) => self.mensaje(
                format!("deliberación cancelada, pero no se pudo registrar: {error}"),
                true,
            ),
        }
    }
}

impl DeliberacionAbierta {
    /// Una deliberación sin tareas, para probar el diálogo (también desde
    /// las pruebas de `tests/`).
    #[doc(hidden)]
    pub fn de_prueba(
        plan: impl Into<PlanDeliberado>,
        filas: Vec<ComprobacionesHost>,
        motivo_min: usize,
    ) -> Self {
        let mut abierta = Self {
            token: 0,
            plan: plan.into(),
            motivos: vec!["crítico".to_string()],
            filas,
            maquina: MaquinaDeliberacion::nueva(motivo_min),
            seleccion: 0,
            desplazamiento: 0,
            backup_horas: 24,
            inicio: Instant::now(),
            _tareas: TareasComprobacion(Vec::new()),
        };
        abierta.resolver();
        abierta
    }
}
