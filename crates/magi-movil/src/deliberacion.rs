//! Deliberación MAGI del móvil (informe §4.6, §5.1 y §7.7): MELCHIOR-1 con
//! el último sondeo del móvil, BALTHASAR-2 por SFTP (`sftp_lectura`) y
//! CASPER-3 como «solo en el escritorio», en paralelo y con 2 s cada una.
//! Aquí también se resuelve la deliberación (aprobar, forzar o cancelar), se
//! guarda la fila de DELIBERACIONES y se valida que una deliberación autoriza
//! una sola acción (T62).
//!
//! Todo es automático (D266): las comprobaciones abren conexiones sin diálogos
//! y una comprobación que no se puede hacer en el móvil rechaza con su motivo
//! (T116), nunca se omite. Los proveedores no releen el estado al confirmar:
//! el plan del núcleo (`PlanDeliberado`) queda fijado al abrir la
//! deliberación (T33).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::deliberacion::comprobaciones::{
    lanzar, saneado, Futuro, Limites, NoDisponible, Proveedores, ResultadoComprobacion, Trabajo,
};
use magi_nucleo::deliberacion::consenso::{self, Consenso};
use magi_nucleo::deliberacion::salud::{self, DecisionSalud};
use magi_nucleo::deliberacion::{
    Comprobacion, ComprobacionesHost, EjecucionResultado, NuevaDeliberacion, PlanDeliberado,
    PlanEjecucion, PlanServicio, ResultadoDeliberacion, Veredicto, Verificaciones,
};
use magi_nucleo::modelo::{ResultadoSondeo, Sondeo};
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::servicios::{Accion, Objetivo, TipoObjetivo};
use magi_nucleo::sftp_lectura::{self, EntradaListado};

use crate::api::{
    AccionMovil, DecisionDeliberacion, ErrorNucleo, EstadoDeliberacion, FilaHostDeliberacion,
    ObjetivoMovil, PlanDeliberadoMovil, TablaDeliberacion, TipoObjetivoMovil, VeredictoMovil,
};
use crate::estado::{con_almacen, con_almacen_async, EstadoNucleo};

/// Plazo de cada comprobación, medido desde el inicio de la deliberación
/// (informe §4.6 y T43).
const LIMITE_SEG: u64 = 2;
/// Caracteres mínimos del motivo de un forzado (informe §7.11).
const MOTIVO_MIN: usize = 10;
/// Lo máximo que se guarda del dato de una celda (viene del remoto: se sanea y
/// se acota antes de enseñarlo o guardarlo).
const LARGO_DETALLE: usize = 120;
/// Plazo del listado SFTP de BALTHASAR-2: el de la comprobación entera.
const PLAZO_SFTP: Duration = Duration::from_secs(LIMITE_SEG);
/// Deliberaciones resueltas que se conservan en memoria sin usar. Una
/// deliberación autoriza una sola acción: las que nadie usa son basura y se
/// podan al abrir otras.
const MAXIMO_EN_MEMORIA: usize = 64;

// ---------------------------------------------------------------------------
// Deliberaciones de esta sesión
// ---------------------------------------------------------------------------

/// Una deliberación abierta por esta sesión del móvil: el plan fijado, la
/// tabla ya resuelta y si ya se usó. Vive en `EstadoNucleo` mientras la app
/// pueda necesitarla (`resolver`, `consumir` y `registrar_resultado`).
pub struct DeliberacionViva {
    /// El plan del núcleo fijado al abrir (T33); de ahí salen la acción y
    /// los hosts de la fila de DELIBERACIONES.
    plan: PlanDeliberado,
    /// Identidad de la acción que autoriza (ver `huella_movil`): es lo que se
    /// compara al consumirla.
    huella: String,
    /// Una fila por host, con los veredictos ya saneados.
    filas: Vec<ComprobacionesHost>,
    /// Recuento de las celdas activas.
    consenso: Consenso,
    /// Caracteres mínimos del motivo del forzado.
    motivo_min: usize,
    /// Id de la fila de DELIBERACIONES, una vez resuelta.
    bd_id: Option<i64>,
    /// Cómo se resolvió; None mientras siga sin resolver.
    resultado: Option<ResultadoDeliberacion>,
    /// Una deliberación autoriza una sola acción (T62).
    usada: bool,
    /// Para poder podar las viejas que nadie usa.
    creada: Instant,
}

// ---------------------------------------------------------------------------
// Proveedores del móvil (D270)
// ---------------------------------------------------------------------------

/// Los proveedores del móvil para las comprobaciones (informe §7.7): el sondeo
/// por la conexión automática del gestor, el listado del directorio de backups
/// por `sftp_lectura` y CASPER-3 como no disponible («solo en el
/// escritorio»). Se construye con el plan ya fijado: `hosts` relaciona cada
/// id del inventario con el uuid de la app.
struct ProveedoresMovil {
    estado: Arc<EstadoNucleo>,
    hosts: HashMap<i64, String>,
}

impl Proveedores for ProveedoresMovil {
    fn sondear(&self, host_id: i64, excluir: Vec<String>) -> Futuro<Sondeo> {
        let estado = self.estado.clone();
        let uuid = self.hosts.get(&host_id).cloned();
        Box::pin(async move {
            let Some(uuid) = uuid else {
                return sondeo_de_error(host_id, "el host ya no existe en el inventario");
            };
            match crate::flota::sondear_host(&estado, &uuid).await {
                Ok(mut sondeo) => {
                    // MELCHIOR-1 no cuenta el estado del propio objetivo en
                    // una acción de servicio (§4.6).
                    if !excluir.is_empty() {
                        sondeo
                            .servicios
                            .retain(|unidad, _| !excluir.contains(unidad));
                    }
                    sondeo
                }
                Err(error) => sondeo_de_error(host_id, &texto_de_error(&error)),
            }
        })
    }

    fn listar(&self, host_id: i64, ruta: String) -> Futuro<Result<Vec<EntradaListado>, String>> {
        let estado = self.estado.clone();
        let uuid = self.hosts.get(&host_id).cloned();
        Box::pin(async move {
            let Some(uuid) = uuid else {
                return Err("el host ya no existe en el inventario".to_string());
            };
            // Automática: las comprobaciones nunca abren diálogos (T15).
            let uso = match crate::conexiones::conexion_automatica(&estado, &uuid).await {
                Ok(uso) => uso,
                Err(error) => return Err(texto_de_error(&error)),
            };
            match sftp_lectura::listar(&uso.handle, &ruta, PLAZO_SFTP).await {
                Ok(listado) => Ok(listado.entradas),
                Err(motivo) => Err(motivo),
            }
        })
    }

    fn tests(
        &self,
        _host_id: i64,
        _comando: String,
    ) -> Futuro<Result<(bool, String), NoDisponible>> {
        // En el móvil no hay comando local que valga: la comprobación rechaza
        // con su motivo y nunca se omite (T116, D270).
        Box::pin(async { Err(NoDisponible::nuevo("solo en el escritorio")) })
    }
}

/// Un sondeo con error dentro, para que MELCHIOR-1 lo enseñe como CAÍDA con su
/// motivo (el sondeo nunca falla: los errores van dentro).
fn sondeo_de_error(host_id: i64, motivo: &str) -> Sondeo {
    let mut sondeo = Sondeo::vacio(host_id);
    sondeo.resultado = ResultadoSondeo::Error;
    sondeo.error = Some(motivo.to_string());
    sondeo
}

/// Texto legible de un error del núcleo para la celda o el listado.
fn texto_de_error(error: &ErrorNucleo) -> String {
    match error {
        ErrorNucleo::Ocupado { .. } => "hay un sondeo en marcha; vuelve a deliberar".to_string(),
        otro => otro.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Deliberar (informe §4.6 y §5.1)
// ---------------------------------------------------------------------------

/// Qué hacer con una celda: ya está decidida o hay que lanzar un trabajo.
enum Celda {
    Decidida(Veredicto),
    Trabajo(Trabajo),
}

/// Hace las comprobaciones del plan en paralelo (2 s cada una) y devuelve la
/// tabla, el consenso y el estado (informe §5.1 y §6.6). La deliberación queda
/// en memoria para `resolver`.
pub async fn deliberar(
    estado: &Arc<EstadoNucleo>,
    plan: PlanDeliberadoMovil,
) -> Result<TablaDeliberacion, ErrorNucleo> {
    let huella = huella_movil(&plan);
    let (plan, hosts) = preparar_plan(estado, plan).await?;
    let verificaciones = verificaciones_de(estado, &plan).await?;
    let limites = limites_de(estado);
    let umbrales = crate::ajustes::umbrales(estado);
    // En una acción de servicio, MELCHIOR-1 no cuenta el estado del propio
    // objetivo (§4.6): ni el sondeo viejo ni el nuevo que se haga ahora.
    let excluir = exclusiones(&plan);

    let mut filas = Vec::new();
    let mut trabajos = Vec::new();
    for (host_id, nombre) in plan.hosts() {
        let verificacion = verificaciones.get(&host_id).cloned().unwrap_or_default();
        let mut celdas = Vec::new();
        for comprobacion in Comprobacion::TODAS {
            let celda = if !verificacion.activa(comprobacion) {
                Celda::Decidida(Veredicto::NoActiva)
            } else {
                match comprobacion {
                    Comprobacion::Salud => {
                        let ultimo = crate::flota::ultimo_sondeo(estado, &hosts[&host_id]).map(
                            |mut sondeo| {
                                if !excluir.is_empty() {
                                    sondeo
                                        .servicios
                                        .retain(|unidad, _| !excluir.contains(unidad));
                                }
                                sondeo
                            },
                        );
                        match salud::con_ultimo(ultimo.as_ref(), limites.salud_max, &umbrales) {
                            DecisionSalud::Aprueba(detalle) => {
                                Celda::Decidida(Veredicto::Aprueba { detalle, ms: 0 })
                            }
                            DecisionSalud::Rechaza(detalle) => {
                                Celda::Decidida(Veredicto::Rechaza { detalle, ms: 0 })
                            }
                            DecisionSalud::Sondear => Celda::Trabajo(Trabajo::Salud {
                                excluir: excluir.clone(),
                            }),
                        }
                    }
                    Comprobacion::Backup => match verificacion.backup_ruta.clone() {
                        Some(ruta) => Celda::Trabajo(Trabajo::Backup {
                            ruta,
                            patron: verificacion.backup_patron.clone(),
                        }),
                        None => rechazo("sin ruta de backup configurada"),
                    },
                    Comprobacion::Tests => match verificacion.tests_comando.clone() {
                        Some(comando) => Celda::Trabajo(Trabajo::Tests { comando }),
                        None => rechazo("sin comando de tests configurado"),
                    },
                }
            };
            celdas.push(match celda {
                Celda::Decidida(veredicto) => saneado(veredicto, LARGO_DETALLE),
                Celda::Trabajo(trabajo) => {
                    trabajos.push((host_id, comprobacion, trabajo));
                    Veredicto::Pendiente
                }
            });
        }
        let mut celdas = celdas.into_iter();
        filas.push(ComprobacionesHost {
            host_id,
            host: nombre,
            salud: celdas.next().unwrap_or(Veredicto::NoActiva),
            backup: celdas.next().unwrap_or(Veredicto::NoActiva),
            tests: celdas.next().unwrap_or(Veredicto::NoActiva),
        });
    }

    let id = estado.siguiente_id();
    let esperados = trabajos.len();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ResultadoComprobacion>();
    let enviar: Arc<dyn Fn(ResultadoComprobacion) + Send + Sync> = Arc::new(move |resultado| {
        let _ = tx.send(resultado);
    });
    let proveedores: Arc<dyn Proveedores> = Arc::new(ProveedoresMovil {
        estado: estado.clone(),
        hosts: hosts.clone(),
    });
    let limite = tokio::time::Instant::now() + limites.limite;
    let _tareas = lanzar(
        estado.runtime.handle(),
        id,
        trabajos,
        proveedores,
        limites,
        umbrales,
        limite,
        enviar,
    );
    // Cada trabajo tiene su propio plazo: siempre llega un resultado por
    // trabajo. El margen solo cubre el envío por el canal; si algo se perdiera,
    // las celdas pendientes se cierran como rechazo y no se queda colgado.
    let plazo_resultados = limite + Duration::from_millis(500);
    let mut recibidos = 0;
    while recibidos < esperados {
        match tokio::time::timeout_at(plazo_resultados, rx.recv()).await {
            Ok(Some(resultado)) => {
                recibidos += 1;
                if resultado.token == id {
                    aplicar_resultado(&mut filas, resultado);
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    completar_pendientes(&mut filas);

    let cuenta = consenso_de(&filas);
    let tabla = TablaDeliberacion {
        id,
        filas: filas
            .iter()
            .map(|fila| fila_a_movil(fila, &hosts))
            .collect(),
        aprobadas: cuenta.aprobadas as u32,
        activas: cuenta.activas as u32,
        rechazos: cuenta.rechazos as u32,
        pendientes: cuenta.pendientes as u32,
        unanime: cuenta.unanime(),
        estado: estado_de_tabla(&cuenta),
    };
    guardar_en_memoria(estado, id, plan, huella, filas, cuenta, limites.motivo_min);
    Ok(tabla)
}

/// Convierte lo que manda la app al plan del núcleo, fijando los ids del
/// inventario: el host por su uuid y, en un snippet, su id de SNIPPETS.
async fn preparar_plan(
    estado: &Arc<EstadoNucleo>,
    plan: PlanDeliberadoMovil,
) -> Result<(PlanDeliberado, HashMap<i64, String>), ErrorNucleo> {
    match plan {
        PlanDeliberadoMovil::Snippet { preparacion } => {
            if let Some(error) = preparacion.errores.first() {
                return Err(ErrorNucleo::EstadoInvalido {
                    detalle: format!(
                        "la preparación tiene errores ({}): {}",
                        error.campo, error.mensaje
                    ),
                });
            }
            if preparacion.hosts.is_empty() {
                return Err(ErrorNucleo::EstadoInvalido {
                    detalle: "la ejecución no tiene ningún host".to_string(),
                });
            }
            let snippet_uuid = preparacion.snippet_uuid.clone();
            let uuids: Vec<String> = preparacion
                .hosts
                .iter()
                .map(|host| host.uuid.clone())
                .collect();
            let uuids_consulta = uuids.clone();
            let datos = con_almacen_async(estado, move |almacen| {
                let conexion = almacen.conexion();
                let snippet_id = movil_bd::id_por_uuid(conexion, Tabla::Snippets, &snippet_uuid)?;
                let mut hosts = Vec::with_capacity(uuids_consulta.len());
                for uuid in &uuids_consulta {
                    hosts.push(movil_bd::host_por_uuid(conexion, uuid)?);
                }
                Ok::<_, anyhow::Error>((snippet_id, hosts))
            })
            .await
            .map_err(|error| ErrorNucleo::General {
                detalle: format!("leyendo el plan de la ejecución: {error}"),
            })?
            .map_err(|error| ErrorNucleo::General {
                detalle: format!("leyendo el plan de la ejecución: {error:#}"),
            })?;
            let (snippet_id, hosts) = datos;
            let snippet_id = snippet_id.ok_or_else(|| ErrorNucleo::NoEncontrado {
                detalle: format!("el snippet «{}»", preparacion.snippet_nombre),
            })?;
            let mut lista = Vec::with_capacity(hosts.len());
            let mut mapa = HashMap::with_capacity(hosts.len());
            for (posicion, host) in hosts.into_iter().enumerate() {
                let nombre = preparacion
                    .hosts
                    .get(posicion)
                    .map(|host| host.nombre.clone())
                    .unwrap_or_default();
                let host = host.ok_or_else(|| ErrorNucleo::NoEncontrado {
                    detalle: format!("el host «{nombre}»"),
                })?;
                if !host.tipo.es_ssh() {
                    return Err(ErrorNucleo::EstadoInvalido {
                        detalle: format!("«{}» es un escritorio: no ejecuta snippets", host.nombre),
                    });
                }
                mapa.insert(host.id, uuids[posicion].clone());
                lista.push((host.id, host.nombre));
            }
            let valores: BTreeMap<String, String> = preparacion
                .valores
                .into_iter()
                .map(|propiedad| (propiedad.clave, propiedad.valor))
                .collect();
            let plan = PlanEjecucion {
                snippet_id,
                nombre: preparacion.snippet_nombre,
                comando: preparacion.comando,
                hosts: lista,
                timeout_seg: preparacion.timeout_seg,
                parar_al_fallo: preparacion.parar_al_fallo,
                critico: preparacion.critico,
                valores,
            };
            Ok((PlanDeliberado::Snippet(plan), mapa))
        }
        PlanDeliberadoMovil::Servicio {
            host,
            objetivo,
            accion,
            comprobacion,
        } => {
            let uuid = host.clone();
            let consulta = uuid.clone();
            let encontrado = con_almacen_async(estado, move |almacen| {
                movil_bd::host_por_uuid(almacen.conexion(), &consulta)
            })
            .await
            .map_err(|error| ErrorNucleo::General {
                detalle: format!("leyendo el host de la acción: {error}"),
            })?
            .map_err(|error| ErrorNucleo::General {
                detalle: format!("leyendo el host de la acción: {error:#}"),
            })?;
            let Some(host) = encontrado else {
                return Err(ErrorNucleo::NoEncontrado {
                    detalle: format!("el host {uuid}"),
                });
            };
            if !host.tipo.es_ssh() {
                return Err(ErrorNucleo::EstadoInvalido {
                    detalle: format!("«{}» es un escritorio: no tiene servicios", host.nombre),
                });
            }
            let plan = PlanServicio {
                host_id: host.id,
                host_nombre: host.nombre,
                objetivo: objetivo_de(&objetivo),
                accion: accion_de(accion),
                comprobacion,
            };
            let mut mapa = HashMap::new();
            mapa.insert(host.id, uuid);
            Ok((PlanDeliberado::Servicio(plan), mapa))
        }
    }
}

/// Verificaciones previas de los hosts del plan, por id de host.
async fn verificaciones_de(
    estado: &Arc<EstadoNucleo>,
    plan: &PlanDeliberado,
) -> Result<HashMap<i64, Verificaciones>, ErrorNucleo> {
    let ids: Vec<i64> = plan.hosts().into_iter().map(|(id, _)| id).collect();
    con_almacen_async(estado, move |almacen| {
        let mut mapa = almacen.verificaciones_por_host()?;
        mapa.retain(|host_id, _| ids.contains(host_id));
        Ok::<_, anyhow::Error>(mapa)
    })
    .await
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo las verificaciones previas: {error}"),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo las verificaciones previas: {error:#}"),
    })
}

/// Objetivo del núcleo (tipo y nombre) a partir del que manda la app.
fn objetivo_de(objetivo: &ObjetivoMovil) -> Objetivo {
    let tipo = match objetivo.tipo {
        TipoObjetivoMovil::Unidad => TipoObjetivo::Unidad,
        TipoObjetivoMovil::Timer => TipoObjetivo::Timer,
        TipoObjetivoMovil::Contenedor => TipoObjetivo::Contenedor,
        TipoObjetivoMovil::Proyecto => TipoObjetivo::Proyecto,
    };
    Objetivo {
        tipo,
        nombre: objetivo.nombre.clone(),
    }
}

/// Acción del núcleo a partir de la que manda la app.
fn accion_de(accion: AccionMovil) -> Accion {
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

/// Unidades que MELCHIOR-1 no cuenta: en una acción de servicio, el objetivo y,
/// si es un timer, su servicio.
fn exclusiones(plan: &PlanDeliberado) -> Vec<String> {
    let PlanDeliberado::Servicio(plan) = plan else {
        return Vec::new();
    };
    let mut nombres = vec![plan.objetivo.nombre.clone()];
    if plan.objetivo.tipo == TipoObjetivo::Timer {
        if let Some(base) = plan.objetivo.nombre.strip_suffix(".timer") {
            nombres.push(format!("{base}.service"));
        }
    }
    nombres
}

/// Plazos y mínimos de la deliberación: las horas del backup y los minutos de
/// salud de los ajustes, y el plazo y motivo fijos del informe.
fn limites_de(estado: &EstadoNucleo) -> Limites {
    let ajustes = crate::ajustes::actuales(estado);
    Limites::nuevos(
        ajustes.deliberacion_backup_horas,
        ajustes.deliberacion_salud_max_min,
        LIMITE_SEG,
        MOTIVO_MIN,
    )
}

/// Una celda decidida que rechaza sin trabajo que lanzar.
fn rechazo(detalle: &str) -> Celda {
    Celda::Decidida(Veredicto::Rechaza {
        detalle: detalle.to_string(),
        ms: 0,
    })
}

/// La celda del veredicto de una comprobación en una fila.
fn veredicto_mut(fila: &mut ComprobacionesHost, comprobacion: Comprobacion) -> &mut Veredicto {
    match comprobacion {
        Comprobacion::Salud => &mut fila.salud,
        Comprobacion::Backup => &mut fila.backup,
        Comprobacion::Tests => &mut fila.tests,
    }
}

/// Aplica el resultado de una comprobación a su celda (solo si sigue
/// pendiente: un resultado repetido no la pisa).
fn aplicar_resultado(filas: &mut [ComprobacionesHost], resultado: ResultadoComprobacion) {
    let Some(fila) = filas
        .iter_mut()
        .find(|fila| fila.host_id == resultado.host_id)
    else {
        return;
    };
    let celda = veredicto_mut(fila, resultado.comprobacion);
    if *celda == Veredicto::Pendiente {
        *celda = saneado(resultado.veredicto, LARGO_DETALLE);
    }
}

/// Cierra las celdas que se quedaran sin resultado (no debería pasar: cada
/// trabajo tiene plazo) como rechazo con el dato, para que nunca queden
/// colgadas (T43).
fn completar_pendientes(filas: &mut [ComprobacionesHost]) {
    for fila in filas.iter_mut() {
        for comprobacion in Comprobacion::TODAS {
            let celda = veredicto_mut(fila, comprobacion);
            if *celda == Veredicto::Pendiente {
                *celda = Veredicto::Rechaza {
                    detalle: "sin resultado (plazo vencido)".to_string(),
                    ms: 0,
                };
            }
        }
    }
}

/// Recuento del consenso de la tabla.
fn consenso_de(filas: &[ComprobacionesHost]) -> Consenso {
    consenso::consenso(filas.iter().flat_map(|fila| {
        Comprobacion::TODAS
            .iter()
            .map(move |comprobacion| fila.veredicto(*comprobacion))
    }))
}

/// Estado del diálogo: APROBADO con unanimidad de las activas y BLOQUEADO en
/// otro caso (informe §6.6). Una deliberación en curso no llega aquí: se
/// espera a que todas las comprobaciones terminen.
fn estado_de_tabla(cuenta: &Consenso) -> EstadoDeliberacion {
    if cuenta.unanime() {
        EstadoDeliberacion::Aprobado
    } else {
        EstadoDeliberacion::Bloqueado
    }
}

/// La fila de la app a partir de la del núcleo, con el uuid del host.
fn fila_a_movil(fila: &ComprobacionesHost, hosts: &HashMap<i64, String>) -> FilaHostDeliberacion {
    FilaHostDeliberacion {
        host_uuid: hosts.get(&fila.host_id).cloned(),
        host_nombre: fila.host.clone(),
        salud: veredicto_a_movil(&fila.salud),
        backup: veredicto_a_movil(&fila.backup),
        tests: veredicto_a_movil(&fila.tests),
    }
}

/// Un veredicto del núcleo al de la app (espejo).
fn veredicto_a_movil(veredicto: &Veredicto) -> VeredictoMovil {
    match veredicto {
        Veredicto::NoActiva => VeredictoMovil::NoActiva,
        Veredicto::Pendiente => VeredictoMovil::Pendiente,
        Veredicto::Aprueba { detalle, ms } => VeredictoMovil::Aprueba {
            detalle: detalle.clone(),
            ms: *ms,
        },
        Veredicto::Rechaza { detalle, ms } => VeredictoMovil::Rechaza {
            detalle: detalle.clone(),
            ms: *ms,
        },
    }
}

/// Guarda la deliberación resuelta en memoria, podando las viejas que nadie
/// usa si el mapa crece.
fn guardar_en_memoria(
    estado: &EstadoNucleo,
    id: u64,
    plan: PlanDeliberado,
    huella: String,
    filas: Vec<ComprobacionesHost>,
    consenso: Consenso,
    motivo_min: usize,
) {
    let Ok(mut memorias) = estado.deliberaciones.lock() else {
        return;
    };
    if memorias.len() >= MAXIMO_EN_MEMORIA {
        podar(&mut memorias);
    }
    memorias.insert(
        id,
        DeliberacionViva {
            plan,
            huella,
            filas,
            consenso,
            motivo_min,
            bd_id: None,
            resultado: None,
            usada: false,
            creada: Instant::now(),
        },
    );
}

/// Quita la mitad de las deliberaciones cuando se llena el mapa: primero las
/// que ya se usaron y luego las más viejas que nadie usó.
fn podar(memorias: &mut HashMap<u64, DeliberacionViva>) {
    let mut claves: Vec<u64> = memorias.keys().copied().collect();
    claves.sort_by_key(|clave| {
        let deliberacion = &memorias[clave];
        (deliberacion.usada, deliberacion.creada)
    });
    let sobran = memorias.len().saturating_sub(MAXIMO_EN_MEMORIA / 2);
    for clave in claves.into_iter().take(sobran) {
        memorias.remove(&clave);
    }
}

// ---------------------------------------------------------------------------
// Resolver y validar la deliberación (informe §5.1 y §7.7)
// ---------------------------------------------------------------------------

/// Resuelve una deliberación: aprueba, fuerza con motivo o cancela, guarda la
/// fila de DELIBERACIONES y devuelve su id (informe §5.1 y §7.7).
pub fn resolver(
    estado: &EstadoNucleo,
    id: u64,
    decision: DecisionDeliberacion,
) -> Result<u64, ErrorNucleo> {
    let mut memorias = estado
        .deliberaciones
        .lock()
        .map_err(|_| ErrorNucleo::Ocupado {
            detalle: "el registro de deliberaciones no está disponible".to_string(),
        })?;
    let Some(deliberacion) = memorias.get_mut(&id) else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("la deliberación #{id}; vuelve a deliberar"),
        });
    };
    validar_resolucion(deliberacion, &decision)?;
    let (resultado, motivo) = match &decision {
        DecisionDeliberacion::Aprobar => (ResultadoDeliberacion::Aprobada, None),
        DecisionDeliberacion::Forzar { motivo } => (
            ResultadoDeliberacion::Forzada,
            Some(motivo.trim().to_string()),
        ),
        DecisionDeliberacion::Cancelar => (ResultadoDeliberacion::Cancelada, None),
    };
    let nueva = NuevaDeliberacion {
        snippet_id: deliberacion.plan.snippet_id(),
        accion: deliberacion.plan.accion(),
        hosts: deliberacion.plan.hosts(),
        comprobaciones: deliberacion.filas.clone(),
        resultado,
        bloqueada: deliberacion.consenso.rechazos > 0,
        motivo,
        usuario: usuario_de(estado),
    };
    let bd_id = crear_fila(estado, nueva)?;
    deliberacion.resultado = Some(resultado);
    deliberacion.bd_id = Some(bd_id);
    Ok(bd_id as u64)
}

/// Usuario de auditoría de DELIBERACIONES en el móvil: «móvil (<equipo>)»
/// (D282: en Android no hay un usuario del sistema que diga quién fue).
fn usuario_de(estado: &EstadoNucleo) -> String {
    format!("móvil ({})", estado.equipo.nombre)
}

/// Inserta la fila de DELIBERACIONES y, si se canceló, anota
/// `deliberacion_cancelada` en el registro. Lo aprobado y lo forzado se
/// anotan al terminar la ejecución (`registrar_resultado`).
fn crear_fila(estado: &EstadoNucleo, nueva: NuevaDeliberacion) -> Result<i64, ErrorNucleo> {
    let cancelada = nueva.resultado == ResultadoDeliberacion::Cancelada;
    let creada = con_almacen(estado, move |almacen| -> anyhow::Result<i64> {
        let id = almacen.crear_deliberacion(&nueva)?;
        if cancelada {
            let registro = almacen.obtener_deliberacion(id)?;
            let detalle = magi_nucleo::deliberacion::detalle_registro(&registro, None);
            registro::anotar(
                almacen.conexion(),
                registro::DELIBERACION_CANCELADA,
                None,
                None,
                &detalle,
                ResultadoRegistro::Ok,
            )?;
        }
        Ok(id)
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("guardando la deliberación: {error}"),
    })?;
    creada.map_err(|error| ErrorNucleo::General {
        detalle: format!("guardando la deliberación: {error:#}"),
    })
}

/// Valida que se puede resolver: sin resolver aún, con todas las celdas
/// terminadas y con la decisión coherente con el consenso.
fn validar_resolucion(
    deliberacion: &DeliberacionViva,
    decision: &DecisionDeliberacion,
) -> Result<(), ErrorNucleo> {
    if deliberacion.bd_id.is_some() || deliberacion.resultado.is_some() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "esa deliberación ya está resuelta; vuelve a deliberar".to_string(),
        });
    }
    if deliberacion.consenso.pendientes > 0 {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "aún hay comprobaciones en marcha: espera a que terminen".to_string(),
        });
    }
    match decision {
        DecisionDeliberacion::Cancelar => Ok(()),
        DecisionDeliberacion::Aprobar => {
            if deliberacion.consenso.unanime() {
                Ok(())
            } else {
                Err(ErrorNucleo::EstadoInvalido {
                    detalle: "hay comprobaciones que rechazan: hay que forzar con un motivo"
                        .to_string(),
                })
            }
        }
        DecisionDeliberacion::Forzar { motivo } => {
            if deliberacion.consenso.unanime() {
                return Err(ErrorNucleo::EstadoInvalido {
                    detalle: "la deliberación está aprobada: no hace falta forzar".to_string(),
                });
            }
            if motivo.trim().chars().count() < deliberacion.motivo_min {
                return Err(ErrorNucleo::EstadoInvalido {
                    detalle: format!(
                        "el motivo del forzado debe tener al menos {} caracteres",
                        deliberacion.motivo_min
                    ),
                });
            }
            Ok(())
        }
    }
}

/// Comprueba que la deliberación existe en esta sesión, se resolvió a favor, es
/// de esta acción y no está usada; la marca usada (T62). La llama la pista de
/// la acción o de la ejecución antes de tocar el host.
pub fn consumir(
    estado: &EstadoNucleo,
    deliberacion_id: u64,
    plan: &PlanDeliberadoMovil,
) -> Result<(), ErrorNucleo> {
    let Ok(id) = i64::try_from(deliberacion_id) else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("la deliberación #{deliberacion_id}"),
        });
    };
    let mut memorias = estado
        .deliberaciones
        .lock()
        .map_err(|_| ErrorNucleo::Ocupado {
            detalle: "el registro de deliberaciones no está disponible".to_string(),
        })?;
    let Some(deliberacion) = memorias
        .values_mut()
        .find(|deliberacion| deliberacion.bd_id == Some(id))
    else {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: format!("la deliberación #{id} no existe en esta sesión; vuelve a deliberar"),
        });
    };
    validar_consumo(deliberacion, &huella_movil(plan))?;
    deliberacion.usada = true;
    Ok(())
}

/// Valida una deliberación contra la acción que la quiere consumir.
fn validar_consumo(deliberacion: &DeliberacionViva, huella: &str) -> Result<(), ErrorNucleo> {
    if deliberacion.resultado == Some(ResultadoDeliberacion::Cancelada) {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "la deliberación se canceló: vuelve a deliberar".to_string(),
        });
    }
    if deliberacion.resultado.is_none() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "la deliberación aún no se ha resuelto; vuelve a deliberar".to_string(),
        });
    }
    if deliberacion.usada {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "esa deliberación ya autorizó otra acción: vuelve a deliberar".to_string(),
        });
    }
    if deliberacion.huella != huella {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "la deliberación no es de esta acción: vuelve a deliberar".to_string(),
        });
    }
    Ok(())
}

/// Identidad de lo que autoriza un plan: el snippet (uuid, comando y hosts) o
/// la acción de servicio (host, objetivo y acción). Se compara al consumir la
/// deliberación: lo que no cambia la acción (p. ej. «parar al primer fallo» en
/// una ejecución o la comprobación que aplica un servicio) no entra.
fn huella_movil(plan: &PlanDeliberadoMovil) -> String {
    match plan {
        PlanDeliberadoMovil::Snippet { preparacion } => {
            let hosts: Vec<&str> = preparacion
                .hosts
                .iter()
                .map(|host| host.uuid.as_str())
                .collect();
            format!(
                "snippet:{}:{}:{}",
                preparacion.snippet_uuid,
                preparacion.comando,
                hosts.join(",")
            )
        }
        PlanDeliberadoMovil::Servicio {
            host,
            objetivo,
            accion,
            ..
        } => format!(
            "servicio:{}:{}:{}:{}",
            host,
            objetivo_de(objetivo).tipo.texto(),
            objetivo.nombre,
            accion_de(*accion).texto()
        ),
    }
}

/// Fija `DELIBERACIONES.ejecucion_resultado` y anota `deliberacion_aprobada`
/// o `deliberacion_forzada` con el resultado (una sola vez). La llama la
/// pista de la acción o de la ejecución al terminar.
pub fn registrar_resultado(
    estado: &EstadoNucleo,
    deliberacion_id: u64,
    resultado: EjecucionResultado,
) -> Result<(), ErrorNucleo> {
    let Ok(id) = i64::try_from(deliberacion_id) else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("la deliberación #{deliberacion_id}"),
        });
    };
    let escrito = estado
        .escritor
        .con_almacen(move |almacen| -> anyhow::Result<()> {
            if !almacen.fijar_resultado_deliberacion(id, resultado)? {
                return Ok(());
            }
            let registro = almacen.obtener_deliberacion(id)?;
            let tipo = match registro.resultado {
                ResultadoDeliberacion::Forzada => registro::DELIBERACION_FORZADA,
                ResultadoDeliberacion::Aprobada => registro::DELIBERACION_APROBADA,
                // Una cancelada no se ejecuta: no hay nada que anotar aquí.
                ResultadoDeliberacion::Cancelada => return Ok(()),
            };
            let detalle = magi_nucleo::deliberacion::detalle_registro(&registro, Some(resultado));
            let resultado_registro = if resultado == EjecucionResultado::Ok {
                ResultadoRegistro::Ok
            } else {
                ResultadoRegistro::Error
            };
            registro::anotar(
                almacen.conexion(),
                tipo,
                None,
                None,
                &detalle,
                resultado_registro,
            )
        })
        .map_err(|error| ErrorNucleo::General {
            detalle: format!("guardando el resultado de la deliberación: {error}"),
        })?;
    escrito.map_err(|error| ErrorNucleo::General {
        detalle: format!("guardando el resultado de la deliberación: {error:#}"),
    })?;
    if let Ok(mut memorias) = estado.deliberaciones.lock() {
        memorias.retain(|_, deliberacion| deliberacion.bd_id != Some(id));
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::api::{HostDestinoMovil, PreparacionEjecucion};

    fn preparacion(comando: &str, hosts: &[(&str, &str)]) -> PreparacionEjecucion {
        PreparacionEjecucion {
            snippet_uuid: "snippet-1".to_string(),
            snippet_nombre: "reiniciar nginx".to_string(),
            comando: comando.to_string(),
            hosts: hosts
                .iter()
                .map(|(uuid, nombre)| HostDestinoMovil {
                    uuid: uuid.to_string(),
                    nombre: nombre.to_string(),
                })
                .collect(),
            variables: Vec::new(),
            errores: Vec::new(),
            delibera: true,
            motivos: vec!["crítico".to_string()],
            critico: true,
            timeout_seg: 60,
            parar_al_fallo: false,
            valores: Vec::new(),
        }
    }

    fn plan_snippet(comando: &str, hosts: &[(&str, &str)]) -> PlanDeliberadoMovil {
        PlanDeliberadoMovil::Snippet {
            preparacion: preparacion(comando, hosts),
        }
    }

    fn plan_servicio(accion: AccionMovil) -> PlanDeliberadoMovil {
        PlanDeliberadoMovil::Servicio {
            host: "uuid-web-01".to_string(),
            objetivo: ObjetivoMovil {
                tipo: TipoObjetivoMovil::Unidad,
                nombre: "app.service".to_string(),
            },
            accion,
            comprobacion: Some("nginx -t".to_string()),
        }
    }

    fn nucleo_snippet(comando: &str, hosts: &[(i64, &str)]) -> PlanDeliberado {
        PlanDeliberado::Snippet(PlanEjecucion {
            snippet_id: 7,
            nombre: "reiniciar nginx".to_string(),
            comando: comando.to_string(),
            hosts: hosts
                .iter()
                .map(|(id, nombre)| (*id, nombre.to_string()))
                .collect(),
            timeout_seg: 60,
            parar_al_fallo: false,
            critico: true,
            valores: BTreeMap::new(),
        })
    }

    fn nucleo_servicio(accion: Accion) -> PlanDeliberado {
        PlanDeliberado::Servicio(PlanServicio {
            host_id: 3,
            host_nombre: "web-01".to_string(),
            objetivo: Objetivo::unidad("app.service"),
            accion,
            comprobacion: Some("nginx -t".to_string()),
        })
    }

    fn filas(aprueban: usize, rechazan: usize) -> Vec<ComprobacionesHost> {
        let mut fila = ComprobacionesHost {
            host_id: 3,
            host: "web-01".to_string(),
            salud: Veredicto::NoActiva,
            backup: Veredicto::NoActiva,
            tests: Veredicto::NoActiva,
        };
        for celda in [&mut fila.salud, &mut fila.backup] {
            if aprueban > 0 {
                *celda = Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 10,
                };
            } else {
                *celda = Veredicto::Pendiente;
            }
        }
        if rechazan > 0 {
            fila.tests = Veredicto::Rechaza {
                detalle: "solo en el escritorio".to_string(),
                ms: 1,
            };
        }
        vec![fila]
    }

    fn deliberacion(
        lista: Vec<ComprobacionesHost>,
        plan: PlanDeliberado,
        huella: String,
    ) -> DeliberacionViva {
        let consenso = consenso_de(&lista);
        DeliberacionViva {
            plan,
            huella,
            filas: lista,
            consenso,
            motivo_min: MOTIVO_MIN,
            bd_id: None,
            resultado: None,
            usada: false,
            creada: Instant::now(),
        }
    }

    /// La deliberación ya resuelta a favor, lista para consumir.
    fn resuelta(plan: PlanDeliberado, huella: String) -> DeliberacionViva {
        let mut deliberacion = viva(1, 0);
        deliberacion.plan = plan;
        deliberacion.huella = huella;
        deliberacion.bd_id = Some(1);
        deliberacion.resultado = Some(ResultadoDeliberacion::Aprobada);
        deliberacion
    }

    fn viva(aprueban: usize, rechazan: usize) -> DeliberacionViva {
        deliberacion(
            filas(aprueban, rechazan),
            nucleo_snippet("true", &[(3, "web-01")]),
            "x".to_string(),
        )
    }

    #[test]
    fn el_consenso_aprobado_y_bloqueado() {
        let aprobada = consenso_de(&filas(1, 0));
        assert!(aprobada.unanime());
        assert_eq!(estado_de_tabla(&aprobada), EstadoDeliberacion::Aprobado);

        let bloqueada = consenso_de(&filas(1, 1));
        assert!(!bloqueada.unanime());
        assert_eq!(bloqueada.rechazos, 1);
        assert_eq!(estado_de_tabla(&bloqueada), EstadoDeliberacion::Bloqueado);
    }

    #[test]
    fn aprobar_exige_unanime() {
        assert!(validar_resolucion(&viva(1, 0), &DecisionDeliberacion::Aprobar).is_ok());
        assert!(validar_resolucion(&viva(1, 1), &DecisionDeliberacion::Aprobar).is_err());
    }

    #[test]
    fn forzar_exige_motivo_suficiente_y_estar_bloqueada() {
        let bloqueada = viva(1, 1);
        let corto = DecisionDeliberacion::Forzar {
            motivo: "   ok   ".to_string(),
        };
        assert!(validar_resolucion(&bloqueada, &corto).is_err());
        let suficiente = DecisionDeliberacion::Forzar {
            motivo: "  backup revisado a mano  ".to_string(),
        };
        assert!(validar_resolucion(&bloqueada, &suficiente).is_ok());
        assert!(validar_resolucion(&viva(1, 0), &suficiente).is_err());
    }

    #[test]
    fn no_se_resuelve_dos_veces_ni_con_celdas_pendientes() {
        let mut aprobada = viva(1, 0);
        assert!(validar_resolucion(&aprobada, &DecisionDeliberacion::Aprobar).is_ok());
        aprobada.bd_id = Some(9);
        assert!(validar_resolucion(&aprobada, &DecisionDeliberacion::Cancelar).is_err());

        let mut pendiente = viva(1, 0);
        pendiente.consenso.pendientes = 1;
        assert!(validar_resolucion(&pendiente, &DecisionDeliberacion::Aprobar).is_err());
    }

    #[test]
    fn una_deliberacion_autoriza_una_sola_accion() {
        let plan = plan_snippet("systemctl restart nginx", &[("uuid-web-01", "web-01")]);
        let mut deliberacion = resuelta(
            nucleo_snippet("systemctl restart nginx", &[(3, "web-01")]),
            huella_movil(&plan),
        );
        assert!(validar_consumo(&deliberacion, &huella_movil(&plan)).is_ok());
        deliberacion.usada = true;
        assert!(validar_consumo(&deliberacion, &huella_movil(&plan)).is_err());
    }

    #[test]
    fn una_deliberacion_cancelada_o_sin_resolver_no_vale() {
        let plan = plan_servicio(AccionMovil::Reiniciar);
        let huella = huella_movil(&plan);
        let mut deliberacion = deliberacion(
            filas(1, 1),
            nucleo_servicio(Accion::Reiniciar),
            huella.clone(),
        );
        deliberacion.resultado = Some(ResultadoDeliberacion::Cancelada);
        assert!(validar_consumo(&deliberacion, &huella).is_err());

        let mut sin_resolver = deliberacion;
        sin_resolver.resultado = None;
        assert!(validar_consumo(&sin_resolver, &huella).is_err());
    }

    #[test]
    fn la_deliberacion_tiene_que_ser_de_la_misma_accion() {
        let plan = plan_snippet("systemctl restart nginx", &[("uuid-web-01", "web-01")]);
        let deliberacion = resuelta(
            nucleo_snippet("systemctl restart nginx", &[(3, "web-01")]),
            huella_movil(&plan),
        );
        // Otra acción sobre el mismo snippet es otra acción.
        let otra_accion = plan_snippet("systemctl stop nginx", &[("uuid-web-01", "web-01")]);
        assert!(validar_consumo(&deliberacion, &huella_movil(&otra_accion)).is_err());
        // Otro host, tampoco.
        let otros_hosts = plan_snippet("systemctl restart nginx", &[("uuid-web-02", "web-02")]);
        assert!(validar_consumo(&deliberacion, &huella_movil(&otros_hosts)).is_err());
        // «Parar al primer fallo» no cambia la acción: se puede alternar.
        let mut con_parar = plan.clone();
        if let PlanDeliberadoMovil::Snippet { preparacion } = &mut con_parar {
            preparacion.parar_al_fallo = true;
        }
        assert!(validar_consumo(&deliberacion, &huella_movil(&con_parar)).is_ok());
    }

    #[test]
    fn la_huella_del_servicio_no_cambia_por_la_comprobacion() {
        let plan = plan_servicio(AccionMovil::Reiniciar);
        let mut otra = plan_servicio(AccionMovil::Reiniciar);
        if let PlanDeliberadoMovil::Servicio { comprobacion, .. } = &mut otra {
            *comprobacion = None;
        }
        assert_eq!(huella_movil(&plan), huella_movil(&otra));
        assert_ne!(
            huella_movil(&plan),
            huella_movil(&plan_servicio(AccionMovil::Parar))
        );
    }

    #[test]
    fn las_exclusiones_del_servicio_llevan_el_objetivo_y_su_servicio() {
        let mut plan = nucleo_servicio(Accion::Reiniciar);
        let PlanDeliberado::Servicio(servicio) = &mut plan else {
            panic!("un servicio");
        };
        servicio.objetivo = Objetivo {
            tipo: TipoObjetivo::Timer,
            nombre: "backup.timer".to_string(),
        };
        assert_eq!(exclusiones(&plan), ["backup.timer", "backup.service"]);
        assert!(exclusiones(&nucleo_snippet("true", &[(3, "web-01")])).is_empty());
    }
}
