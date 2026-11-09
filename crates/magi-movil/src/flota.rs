//! Flota del móvil (informe §4.1, §5.1 y §6.1-§6.2): el sondeo de los hosts
//! con semáforo de 8 y 5 s por host, la lista completa que pinta la pantalla de
//! inicio y el bloque FLOTA de la ficha.
//!
//! Las aperturas del sondeo son automáticas: usan la conexión del host del
//! gestor de conexiones y nunca abren diálogos; una huella sin aceptar o la
//! falta de credenciales dan error con instrucción (informe §7.2). Los
//! resultados se guardan en `SONDEOS` (20 por host, con `fallos_json`) y se
//! difunden por `OyenteNucleo::flota` al empezar y al terminar cada sondeo.
//!
//! El sondeo nunca toca `HOSTS.ultimo_estado`: solo el registro anota el paso
//! a CAÍDA (`sondeo_fallido`) y la vuelta (`sondeo_recuperado`), como en el
//! escritorio.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::almacen::Almacen;
use magi_nucleo::flota::{
    aplicar_metricas, comando_de_sondeo, evaluar, parsear, tasa_red, EstadoFlota, Metricas,
    Umbrales, CONCURRENTES, TIMEOUT_SEG,
};
use magi_nucleo::modelo::{Host, ResultadoSondeo, Sondeo};
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::servicios::{MotorNombre, TipoFallo};
use tokio::sync::Semaphore;

use crate::api::{
    DetalleFlota, ErrorNucleo, EstadoFlotaMovil, FalloFlota, HostFlota, MetricasFlota,
    MotorNombreMovil, ResumenMetricas, SnippetAlConectarMovil, TipoFalloMovil, VerificacionesMovil,
    VigiladoFlota,
};
use crate::estado::{con_almacen, con_almacen_async, EstadoNucleo};

/// Tope por flujo de la salida del sondeo que se acumula (2 MiB): el script
/// solo manda líneas `MAGI_*`, así que sobra de largo.
const SALIDA_MAX_BYTES: usize = 2 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Sondeos en curso: un host no se sondea dos veces a la vez (informe §4.1)
// ---------------------------------------------------------------------------

/// Hosts con un sondeo en curso ahora mismo, por uuid. Vive en el módulo
/// porque es efímero y no viaja en la base de datos.
fn sondeos_en_curso() -> &'static Mutex<HashSet<String>> {
    static EN_CURSO: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    EN_CURSO.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Marca de un sondeo en curso: al soltarse (también si la tarea se cancela)
/// se quita el host del mapa para que se pueda volver a sondear.
struct MarcaSondeo {
    uuid: String,
}

impl Drop for MarcaSondeo {
    fn drop(&mut self) {
        if let Ok(mut en_curso) = sondeos_en_curso().lock() {
            en_curso.remove(&self.uuid);
        }
    }
}

/// Marca el host como «sondeando». None si ya había un sondeo en curso: en ese
/// caso se salta, no se solapa.
fn marcar_en_curso(host_uuid: &str) -> Option<MarcaSondeo> {
    let mut en_curso = sondeos_en_curso().lock().ok()?;
    if !en_curso.insert(host_uuid.to_string()) {
        return None;
    }
    Some(MarcaSondeo {
        uuid: host_uuid.to_string(),
    })
}

/// ¿Hay un sondeo en curso de ese host? Es lo que pinta la fila como
/// «sondeando».
fn esta_sondeando(host_uuid: &str) -> bool {
    sondeos_en_curso()
        .lock()
        .map(|en_curso| en_curso.contains(host_uuid))
        .unwrap_or(false)
}

/// Semáforo global del sondeo: como mucho `CONCURRENTES` hosts a la vez, sumen
/// los lotes que sumen (informe §4.1).
fn semaforo() -> &'static Semaphore {
    static SEMAFORO: OnceLock<Semaphore> = OnceLock::new();
    SEMAFORO.get_or_init(|| Semaphore::new(CONCURRENTES))
}

// ---------------------------------------------------------------------------
// API de Flota (informe §5.1)
// ---------------------------------------------------------------------------

/// Lista completa de Flota con el mismo filtro por subcadena que la lista de
/// Hosts de la Fase 11 (informe §5.1): estado, culpables, resumen de métricas,
/// fallos, sesiones abiertas, hora del último sondeo y motivo o duración.
pub fn hosts_flota(
    estado: &EstadoNucleo,
    filtro: Option<&str>,
) -> Result<Vec<HostFlota>, ErrorNucleo> {
    let filas = recoger_filas_de(estado)?;
    Ok(construir_flota(estado, filas, filtro))
}

/// Sondea en segundo plano esos hosts (con la lista vacía, todos los `ssh`):
/// semáforo de 8 y 5 s por host; un host que ya se está sondeando se salta.
/// Los resultados llegan por `OyenteNucleo::flota` (informe §5.1).
pub async fn sondear(
    estado: &std::sync::Arc<EstadoNucleo>,
    hosts: Vec<String>,
) -> Result<(), ErrorNucleo> {
    let objetivos = hosts_a_sondear(estado, hosts).await?;
    let manejador = estado.runtime.handle().clone();
    for host_uuid in objetivos {
        let estado = estado.clone();
        manejador.spawn(async move {
            let Some(marca) = marcar_en_curso(&host_uuid) else {
                return;
            };
            emitir_flota(&estado).await;
            let permiso = semaforo().acquire().await;
            let _ = sondear_marcado(&estado, &host_uuid).await;
            drop(permiso);
            drop(marca);
            emitir_flota(&estado).await;
        });
    }
    Ok(())
}

/// Sondea un solo host y devuelve su `Sondeo` (5 s en total, informe §7.2).
/// Lo usa también MELCHIOR-1 para la deliberación. Si ya hay un sondeo en
/// curso de ese host devuelve `Ocupado`: no se solapan.
pub async fn sondear_host(
    estado: &std::sync::Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<Sondeo, ErrorNucleo> {
    let Some(marca) = marcar_en_curso(host_uuid) else {
        return Err(ErrorNucleo::Ocupado {
            detalle: format!("ya se está sondeando el host {host_uuid}"),
        });
    };
    emitir_flota(estado).await;
    let resultado = sondear_marcado(estado, host_uuid).await;
    drop(marca);
    emitir_flota(estado).await;
    resultado
}

/// Último sondeo guardado de un host (informe §5.1). None si no hay o no se
/// puede leer.
pub fn ultimo_sondeo(estado: &EstadoNucleo, host_uuid: &str) -> Option<Sondeo> {
    let uuid = host_uuid.to_string();
    let resultado = con_almacen(estado, move |almacen| -> anyhow::Result<Option<Sondeo>> {
        let Some(id) = movil_bd::id_por_uuid(almacen.conexion(), Tabla::Hosts, &uuid)? else {
            return Ok(None);
        };
        almacen.ultimo_sondeo(id)
    });
    resultado.ok()?.ok().flatten()
}

/// Bloque FLOTA de la ficha (informe §6.2): métricas completas, vigilados con
/// su estado, fallos con nombres y hora y duración del sondeo.
pub fn detalle(
    estado: &EstadoNucleo,
    host_uuid: &str,
) -> Result<Option<DetalleFlota>, ErrorNucleo> {
    let uuid = host_uuid.to_string();
    let datos = con_almacen(
        estado,
        move |almacen| -> anyhow::Result<Option<(Host, Vec<Sondeo>)>> {
            let Some(host) = movil_bd::host_por_uuid(almacen.conexion(), &uuid)? else {
                return Ok(None);
            };
            let sondeos = almacen.ultimos_sondeos(host.id, 2)?;
            Ok(Some((host, sondeos)))
        },
    )
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo la flota del host: {error:#}"),
    })?;
    let Some((host, sondeos)) = datos else {
        return Ok(None);
    };
    Ok(Some(construir_detalle(estado, host_uuid, &host, &sondeos)))
}

/// Verificaciones previas del host, de solo lectura (informe §6.2). None si el
/// host o la fila no existen.
pub fn verificaciones(
    estado: &EstadoNucleo,
    host_uuid: &str,
) -> Result<Option<VerificacionesMovil>, ErrorNucleo> {
    let uuid = host_uuid.to_string();
    con_almacen(
        estado,
        move |almacen| -> anyhow::Result<Option<VerificacionesMovil>> {
            let Some(host) = movil_bd::host_por_uuid(almacen.conexion(), &uuid)? else {
                return Ok(None);
            };
            let Some(verificaciones) = almacen.verificaciones_de_host(host.id)? else {
                return Ok(None);
            };
            Ok(Some(VerificacionesMovil {
                salud: verificaciones.salud,
                backup: verificaciones.backup,
                backup_ruta: verificaciones.backup_ruta,
                backup_patron: verificaciones.backup_patron,
                tests: verificaciones.tests,
                tests_comando: verificaciones.tests_comando,
            }))
        },
    )
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo las verificaciones del host: {error:#}"),
    })
}

/// Snippet que se escribe al conectar, de solo lectura (informe §6.2 y §7.9).
/// El móvil vuelve a comprobarlo antes de escribirlo; aquí se dice si es apto
/// (no crítico y sin variables) para que la ficha pueda avisar.
pub fn snippet_al_conectar(
    estado: &EstadoNucleo,
    host_uuid: &str,
) -> Result<Option<SnippetAlConectarMovil>, ErrorNucleo> {
    let uuid = host_uuid.to_string();
    con_almacen(
        estado,
        move |almacen| -> anyhow::Result<Option<SnippetAlConectarMovil>> {
            let Some(host) = movil_bd::host_por_uuid(almacen.conexion(), &uuid)? else {
                return Ok(None);
            };
            let Some(snippet_id) = host.snippet_al_conectar_id else {
                return Ok(None);
            };
            let Ok(snippet) = almacen.obtener_snippet(snippet_id) else {
                // El escritorio pudo borrarlo: la ficha no enseña nada.
                return Ok(None);
            };
            let uuid = movil_bd::uuid_por_id(almacen.conexion(), Tabla::Snippets)?
                .get(&snippet_id)
                .cloned()
                .unwrap_or_default();
            Ok(Some(SnippetAlConectarMovil {
                uuid,
                nombre: snippet.nombre.clone(),
                valido: snippet.apto_al_conectar(),
            }))
        },
    )
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el snippet al conectar: {error:#}"),
    })
}

/// Cerrojo de las emisiones de Flota: la foto de la base de datos y la
/// entrega al oyente van juntas, para que dos sondeos que terminan a la vez no
/// entreguen sus listas completas en orden inverso y la app se quede con una
/// foto vieja (aviso de revisión adversarial: la lista siempre es completa,
/// pero una entrega tardía de una foto anterior perdía la última actualización
/// en la vista hasta el refresco siguiente).
fn emision() -> &'static tokio::sync::Mutex<()> {
    static EMISION: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    EMISION.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Difunde la lista completa de Flota por `OyenteNucleo::flota` (informe §5.2).
pub async fn emitir_flota(estado: &std::sync::Arc<EstadoNucleo>) {
    let _guardia = emision().lock().await;
    let Ok(resultado) = con_almacen_async(estado, recoger_filas).await else {
        return;
    };
    let Ok(filas) = resultado else {
        return;
    };
    let lista = construir_flota(estado, filas, None);
    estado.retrollamadas.oyente.flota(lista);
}

// ---------------------------------------------------------------------------
// Sondeo de un host
// ---------------------------------------------------------------------------

/// Sondea un host ya marcado como «sondeando»: abre la conexión automática,
/// ejecuta el script del núcleo con los motores de los ajustes y guarda el
/// sondeo con sus transiciones de registro.
async fn sondear_marcado(
    estado: &std::sync::Arc<EstadoNucleo>,
    host_uuid: &str,
) -> Result<Sondeo, ErrorNucleo> {
    let uuid = host_uuid.to_string();
    let host = con_almacen_async(estado, move |almacen| {
        movil_bd::host_por_uuid(almacen.conexion(), &uuid)
    })
    .await
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el host a sondear: {error:#}"),
    })?;
    let Some(host) = host else {
        // El host se borró mientras se lanzaba el sondeo: se descarta.
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("el host {host_uuid}"),
        });
    };
    if !host.tipo.es_ssh() {
        return Err(ErrorNucleo::EstadoInvalido {
            detalle: "los hosts que no son SSH no se sondean".to_string(),
        });
    }
    let motores = crate::ajustes::motores(estado);
    let umbrales = crate::ajustes::umbrales(estado);
    let mut sondeo = Sondeo::vacio(host.id);
    let inicio = Instant::now();
    // El plazo de 5 s cubre la conexión (o su reutilización) y el comando
    // entero (informe §7.2).
    let intento = tokio::time::timeout(
        Duration::from_secs(TIMEOUT_SEG),
        intento_sondeo(estado, host_uuid, &host, &motores),
    )
    .await;
    match intento {
        Ok(Ok(metricas)) => aplicar_metricas(&mut sondeo, metricas),
        Ok(Err(error)) => {
            sondeo.resultado = ResultadoSondeo::Error;
            sondeo.error = Some(error.to_string());
        }
        Err(_) => {
            sondeo.resultado = ResultadoSondeo::Error;
            sondeo.error = Some(format!("el host no responde en {TIMEOUT_SEG} s"));
        }
    }
    sondeo.duracion_ms = inicio.elapsed().as_millis() as i64;
    let sondeo = guardar_sondeo(estado, sondeo, umbrales).await?;
    Ok(sondeo)
}

/// Un intento de sondeo por la conexión automática del host.
async fn intento_sondeo(
    estado: &std::sync::Arc<EstadoNucleo>,
    host_uuid: &str,
    host: &Host,
    motores: &[String],
) -> Result<Metricas, ErrorNucleo> {
    let uso = crate::conexiones::conexion_automatica(estado, host_uuid).await?;
    let servicios = magi_nucleo::modelo::servicios_de(host);
    let comando = comando_de_sondeo(&servicios, motores);
    let salida = crate::conexiones::ejecutar_remoto(
        &uso,
        &comando,
        None,
        Duration::from_secs(TIMEOUT_SEG),
        SALIDA_MAX_BYTES,
    )
    .await?;
    if salida.stdout.iter().all(u8::is_ascii_whitespace) {
        return Err(ErrorNucleo::General {
            detalle: format!(
                "el sondeo no devolvió nada (código {})",
                codigo_texto(salida.codigo)
            ),
        });
    }
    Ok(parsear(&String::from_utf8_lossy(&salida.stdout)))
}

/// Guarda el sondeo por el hilo escritor y anota las transiciones de caída y
/// recuperación antes de insertarlo (el estado previo se mira sin la fila
/// nueva).
async fn guardar_sondeo(
    estado: &std::sync::Arc<EstadoNucleo>,
    sondeo: Sondeo,
    umbrales: Umbrales,
) -> Result<Sondeo, ErrorNucleo> {
    con_almacen_async(estado, move |almacen| -> anyhow::Result<Sondeo> {
        // El sondeo es el dato: se guarda primero (el efecto) y la transición
        // se anota después. Si la anotación fallara, el sondeo no se pierde
        // (aviso de revisión adversarial).
        let previo = almacen.ultimo_sondeo(sondeo.host_id)?;
        almacen.guardar_sondeo(&sondeo)?;
        registrar_transiciones(almacen, previo.as_ref(), &sondeo, &umbrales);
        Ok(sondeo)
    })
    .await
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("guardando el sondeo: {error:#}"),
    })
}

/// Anota `sondeo_fallido` al pasar a CAÍDA y `sondeo_recuperado` al volver,
/// nunca un aviso por cada refresco de un host ya caído. `HOSTS.ultimo_estado`
/// no se toca. Se llama después de guardar el sondeo, con el anterior: un fallo
/// de la anotación no se propaga (el sondeo ya está guardado).
fn registrar_transiciones(
    almacen: &mut Almacen,
    previo: Option<&Sondeo>,
    sondeo: &Sondeo,
    umbrales: &Umbrales,
) {
    let (estado_previo, _) = evaluar(previo, umbrales);
    let (estado_nuevo, _) = evaluar(Some(sondeo), umbrales);
    if estado_nuevo == EstadoFlota::Caida && estado_previo != EstadoFlota::Caida {
        let motivo = sondeo
            .error
            .clone()
            .unwrap_or_else(|| "error de sondeo".to_string());
        let _ = registro::anotar(
            almacen.conexion(),
            registro::SONDEO_FALLIDO,
            Some(sondeo.host_id),
            None,
            &motivo,
            ResultadoRegistro::Error,
        );
    } else if estado_previo == EstadoFlota::Caida && estado_nuevo != EstadoFlota::Caida {
        let _ = registro::anotar(
            almacen.conexion(),
            registro::SONDEO_RECUPERADO,
            Some(sondeo.host_id),
            None,
            "el host responde de nuevo",
            ResultadoRegistro::Ok,
        );
    }
}

/// Hosts que hay que sondear: los pedidos (o todos), solo `ssh` y solo si
/// siguen en el inventario.
async fn hosts_a_sondear(
    estado: &std::sync::Arc<EstadoNucleo>,
    hosts: Vec<String>,
) -> Result<Vec<String>, ErrorNucleo> {
    let pedidos: Option<HashSet<String>> = if hosts.is_empty() {
        None
    } else {
        Some(hosts.into_iter().collect())
    };
    con_almacen_async(estado, move |almacen| -> anyhow::Result<Vec<String>> {
        let mut uuids = Vec::new();
        for fila in movil_bd::hosts(almacen.conexion())? {
            if fila.uuid.is_empty() || !fila.host.tipo.es_ssh() {
                continue;
            }
            if let Some(pedidos) = &pedidos {
                if !pedidos.contains(&fila.uuid) {
                    continue;
                }
            }
            uuids.push(fila.uuid);
        }
        Ok(uuids)
    })
    .await
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los hosts a sondear: {error:#}"),
    })
}

// ---------------------------------------------------------------------------
// Construcción de la lista y del detalle (puro, sin E/S)
// ---------------------------------------------------------------------------

/// Datos de un host recogidos de la base de datos, antes de evaluarlos.
struct FilaFlota {
    uuid: String,
    host: Host,
    sondeos: Vec<Sondeo>,
    grupo_uuid: Option<String>,
    /// Posición del grupo en la lista (para ordenar como la vista Hosts).
    posicion_grupo: usize,
}

fn recoger_filas_de(estado: &EstadoNucleo) -> Result<Vec<FilaFlota>, ErrorNucleo> {
    con_almacen(estado, recoger_filas)
        .map_err(|error| ErrorNucleo::General {
            detalle: error.to_string(),
        })?
        .map_err(|error| ErrorNucleo::General {
            detalle: format!("leyendo la flota: {error:#}"),
        })
}

fn recoger_filas(almacen: &mut Almacen) -> anyhow::Result<Vec<FilaFlota>> {
    let conexion = almacen.conexion();
    let hosts = movil_bd::hosts(conexion)?;
    let grupos = movil_bd::uuid_por_id(conexion, Tabla::Grupos)?;
    let posiciones: HashMap<String, usize> = movil_bd::grupos(conexion)?
        .into_iter()
        .enumerate()
        .map(|(posicion, grupo)| (grupo.uuid, posicion))
        .collect();
    let mut filas = Vec::new();
    for fila in hosts {
        // Un host sin uuid no se puede referenciar desde la app: se descarta.
        if fila.uuid.is_empty() {
            continue;
        }
        let grupo_uuid = fila.host.grupo_id.and_then(|id| grupos.get(&id).cloned());
        let posicion_grupo = grupo_uuid
            .as_ref()
            .and_then(|uuid| posiciones.get(uuid).copied())
            .unwrap_or(usize::MAX);
        let sondeos = almacen.ultimos_sondeos(fila.host.id, 2)?;
        filas.push(FilaFlota {
            uuid: fila.uuid,
            host: fila.host,
            sondeos,
            grupo_uuid,
            posicion_grupo,
        });
    }
    filas.sort_by(|a, b| {
        a.posicion_grupo
            .cmp(&b.posicion_grupo)
            .then_with(|| a.host.nombre.cmp(&b.host.nombre))
    });
    Ok(filas)
}

fn construir_flota(
    estado: &EstadoNucleo,
    filas: Vec<FilaFlota>,
    filtro: Option<&str>,
) -> Vec<HostFlota> {
    let umbrales = crate::ajustes::umbrales(estado);
    let consulta = filtro.map(magi_nucleo::modelo::normalizar_busqueda);
    let mut lista = Vec::with_capacity(filas.len());
    for fila in filas {
        if let Some(consulta) = &consulta {
            if !crate::inventario::host_coincide(&fila.host, consulta) {
                continue;
            }
        }
        let sondeo = fila.sondeos.first();
        let anterior = fila.sondeos.get(1);
        let (estado_flota, culpables, motivo) = estado_de(
            sondeo,
            esta_sondeando(&fila.uuid),
            fila.host.tipo.es_ssh(),
            &umbrales,
        );
        lista.push(HostFlota {
            uuid: fila.uuid.clone(),
            nombre: fila.host.nombre.clone(),
            direccion: fila.host.direccion.clone(),
            puerto: u32::from(fila.host.puerto),
            tipo: crate::inventario::tipo_a_vista(fila.host.tipo),
            grupo_uuid: fila.grupo_uuid.clone(),
            etiquetas: fila.host.etiquetas.clone(),
            estado: estado_flota,
            culpables,
            resumen: sondeo.map(|sondeo| resumen_de_sondeo(sondeo, anterior)),
            fallos: sondeo
                .and_then(|sondeo| sondeo.fallos.as_ref())
                .map(|fallos| fallos.total())
                .unwrap_or(0),
            sesiones: estado.recuento_sesiones(&fila.uuid).0 as u32,
            ultimo_sondeo_en: sondeo
                .and_then(|sondeo| crate::inventario::fecha_a_epoca(&sondeo.fecha)),
            motivo,
            duracion_ms: sondeo.and_then(duracion_de_sondeo),
        });
    }
    lista
}

fn construir_detalle(
    estado: &EstadoNucleo,
    host_uuid: &str,
    host: &Host,
    sondeos: &[Sondeo],
) -> DetalleFlota {
    let umbrales = crate::ajustes::umbrales(estado);
    let sondeo = sondeos.first();
    let anterior = sondeos.get(1);
    let (estado_flota, culpables, motivo) = estado_de(
        sondeo,
        esta_sondeando(host_uuid),
        host.tipo.es_ssh(),
        &umbrales,
    );
    let (vigilados, aviso) = vigilados_de(host, sondeo);
    DetalleFlota {
        estado: estado_flota,
        culpables,
        motivo,
        metricas: sondeo.map(|sondeo| metricas_de_sondeo(sondeo, anterior)),
        vigilados,
        fallos: fallos_de_sondeo(sondeo),
        ultimo_sondeo_en: sondeo.and_then(|sondeo| crate::inventario::fecha_a_epoca(&sondeo.fecha)),
        duracion_ms: sondeo.and_then(duracion_de_sondeo),
        aviso,
    }
}

/// Estado de Flota de un host con su motivo legible (informe §6.1).
fn estado_de(
    sondeo: Option<&Sondeo>,
    en_curso: bool,
    es_ssh: bool,
    umbrales: &Umbrales,
) -> (EstadoFlotaMovil, Vec<String>, Option<String>) {
    if !es_ssh {
        return (
            EstadoFlotaMovil::SinSondeo,
            Vec::new(),
            Some("se abre en el ordenador".to_string()),
        );
    }
    if en_curso {
        return (EstadoFlotaMovil::Sondeando, Vec::new(), None);
    }
    let Some(sondeo) = sondeo else {
        return (
            EstadoFlotaMovil::Fria,
            Vec::new(),
            Some("sin sondeo".to_string()),
        );
    };
    let (estado, culpables) = evaluar(Some(sondeo), umbrales);
    let estado = match estado {
        EstadoFlota::Fria => EstadoFlotaMovil::Fria,
        EstadoFlota::Caida => EstadoFlotaMovil::Caida,
        EstadoFlota::Alcanzable => EstadoFlotaMovil::Alcanzable,
        EstadoFlota::Carga => EstadoFlotaMovil::Carga,
        EstadoFlota::Nominal => EstadoFlotaMovil::Nominal,
    };
    let motivo = match estado {
        EstadoFlotaMovil::Caida => Some(
            sondeo
                .error
                .clone()
                .unwrap_or_else(|| "error de sondeo".to_string()),
        ),
        _ => None,
    };
    (estado, culpables, motivo)
}

/// Resumen de métricas para la segunda línea de la fila.
fn resumen_de_sondeo(sondeo: &Sondeo, anterior: Option<&Sondeo>) -> ResumenMetricas {
    let (red_rx_bps, red_tx_bps) = anterior
        .and_then(|anterior| tasa_red(sondeo, anterior))
        .map(|(rx, tx)| (Some(rx), Some(tx)))
        .unwrap_or((None, None));
    ResumenMetricas {
        nucleos: sondeo
            .nucleos
            .and_then(|nucleos| u32::try_from(nucleos).ok()),
        carga_1m: sondeo.carga_1m,
        memoria_pct: sondeo.memoria_pct(),
        disco_pct: sondeo.disco_pct(),
        red_rx_bps,
        red_tx_bps,
        uptime_seg: sondeo.uptime_seg,
    }
}

/// Métricas completas del bloque FLOTA de la ficha.
fn metricas_de_sondeo(sondeo: &Sondeo, anterior: Option<&Sondeo>) -> MetricasFlota {
    let resumen = resumen_de_sondeo(sondeo, anterior);
    MetricasFlota {
        nucleos: resumen.nucleos,
        carga_1m: sondeo.carga_1m,
        carga_5m: sondeo.carga_5m,
        carga_15m: sondeo.carga_15m,
        memoria_total_kb: sondeo.mem_total_kb,
        memoria_disponible_kb: sondeo.mem_disponible_kb,
        memoria_pct: resumen.memoria_pct,
        disco_total_kb: sondeo.disco_total_kb,
        disco_usado_kb: sondeo.disco_usado_kb,
        disco_pct: resumen.disco_pct,
        red_rx_bytes: sondeo.red_rx_bytes,
        red_tx_bytes: sondeo.red_tx_bytes,
        red_rx_bps: resumen.red_rx_bps,
        red_tx_bps: resumen.red_tx_bps,
        uptime_seg: sondeo.uptime_seg,
    }
}

/// Vigilados del host con su estado en el último sondeo; las unidades que no
/// casan con el patrón válido se ignoran con aviso en la ficha (§7.11).
fn vigilados_de(host: &Host, sondeo: Option<&Sondeo>) -> (Vec<VigiladoFlota>, Option<String>) {
    if !host.tipo.es_ssh() {
        return (Vec::new(), None);
    }
    let mut vigilados = Vec::new();
    let mut ignoradas = Vec::new();
    for unidad in magi_nucleo::modelo::servicios_de(host) {
        if !magi_nucleo::modelo::unidad_valida(&unidad) {
            ignoradas.push(unidad);
            continue;
        }
        let estado = sondeo
            .and_then(|sondeo| sondeo.servicios.get(&unidad).cloned())
            .unwrap_or_else(|| "unknown".to_string());
        vigilados.push(VigiladoFlota {
            nombre: unidad,
            activo: estado == "active",
            estado,
        });
    }
    let aviso = (!ignoradas.is_empty()).then(|| {
        format!(
            "se ignoran unidades vigiladas no válidas: {}",
            ignoradas.join(", ")
        )
    });
    (vigilados, aviso)
}

/// Fallos del último sondeo con sus nombres (informe §6.4).
fn fallos_de_sondeo(sondeo: Option<&Sondeo>) -> Vec<FalloFlota> {
    let Some(fallos) = sondeo.and_then(|sondeo| sondeo.fallos.as_ref()) else {
        return Vec::new();
    };
    fallos
        .lista()
        .into_iter()
        .map(|fallo| FalloFlota {
            tipo: match fallo.tipo {
                TipoFallo::Servicio => TipoFalloMovil::Servicio,
                TipoFallo::Contenedor => TipoFalloMovil::Contenedor,
            },
            nombre: fallo.nombre,
            estado: fallo.estado,
            motor: fallo.motor.map(motor_a_vista),
        })
        .collect()
}

fn motor_a_vista(motor: MotorNombre) -> MotorNombreMovil {
    match motor {
        MotorNombre::Docker => MotorNombreMovil::Docker,
        MotorNombre::Podman => MotorNombreMovil::Podman,
    }
}

/// La duración solo se enseña si se llegó a medir (los sondeos anteriores a la
/// Fase 14 pueden traerla a 0).
fn duracion_de_sondeo(sondeo: &Sondeo) -> Option<i64> {
    (sondeo.duracion_ms > 0).then_some(sondeo.duracion_ms)
}

/// Código de salida legible para el error del sondeo.
fn codigo_texto(codigo: Option<u32>) -> String {
    codigo
        .map(|codigo| codigo.to_string())
        .unwrap_or_else(|| "desconocido".to_string())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use magi_nucleo::servicios::{FallosContenedores, FallosSondeo, FallosUnidades};

    fn sondeo_ok() -> Sondeo {
        let mut sondeo = Sondeo::vacio(1);
        sondeo.nucleos = Some(4);
        sondeo.carga_1m = Some(1.0);
        sondeo.carga_5m = Some(1.0);
        sondeo.carga_15m = Some(1.0);
        sondeo.mem_total_kb = Some(1000);
        sondeo.mem_disponible_kb = Some(380);
        sondeo.disco_total_kb = Some(1000);
        sondeo.disco_usado_kb = Some(280);
        sondeo.uptime_seg = Some(14 * 86_400 + 2 * 3_600);
        sondeo.fecha = "2026-11-09T12:00:00+01:00".to_string();
        sondeo.duracion_ms = 640;
        sondeo
    }

    #[test]
    fn el_estado_sale_del_sondeo_y_de_los_umbrales() {
        let umbrales = Umbrales::default();
        assert_eq!(
            estado_de(None, false, true, &umbrales),
            (
                EstadoFlotaMovil::Fria,
                Vec::new(),
                Some("sin sondeo".to_string())
            )
        );
        assert_eq!(
            estado_de(Some(&sondeo_ok()), false, true, &umbrales),
            (EstadoFlotaMovil::Nominal, Vec::new(), None)
        );
        assert_eq!(
            estado_de(Some(&sondeo_ok()), true, true, &umbrales).0,
            EstadoFlotaMovil::Sondeando
        );
        assert_eq!(
            estado_de(Some(&sondeo_ok()), false, false, &umbrales).0,
            EstadoFlotaMovil::SinSondeo
        );

        let mut caido = sondeo_ok();
        caido.resultado = ResultadoSondeo::Error;
        caido.error = Some("se agotó el plazo".to_string());
        assert_eq!(
            estado_de(Some(&caido), false, true, &umbrales),
            (
                EstadoFlotaMovil::Caida,
                Vec::new(),
                Some("se agotó el plazo".to_string())
            )
        );

        let mut sin_metricas = sondeo_ok();
        sin_metricas.resultado = ResultadoSondeo::SinMetricas;
        assert_eq!(
            estado_de(Some(&sin_metricas), false, true, &umbrales).0,
            EstadoFlotaMovil::Alcanzable
        );

        let mut con_memoria = sondeo_ok();
        con_memoria.mem_disponible_kb = Some(50);
        let (estado, culpables, motivo) = estado_de(Some(&con_memoria), false, true, &umbrales);
        assert_eq!(estado, EstadoFlotaMovil::Carga);
        assert_eq!(culpables, vec!["memoria"]);
        assert!(motivo.is_none());
    }

    #[test]
    fn el_resumen_calcula_porcentajes_y_tasas() {
        let anterior = sondeo_ok();
        let mut actual = sondeo_ok();
        actual.fecha = "2026-11-09T12:00:10+01:00".to_string();
        actual.red_rx_bytes = Some(1000);
        actual.red_tx_bytes = Some(2000);
        let mut antes_con_red = anterior.clone();
        antes_con_red.red_rx_bytes = Some(0);
        antes_con_red.red_tx_bytes = Some(0);
        let resumen = resumen_de_sondeo(&actual, Some(&antes_con_red));
        assert_eq!(resumen.nucleos, Some(4));
        assert_eq!(resumen.carga_1m, Some(1.0));
        // Los porcentajes salen de una división en coma flotante: se comparan
        // con tolerancia, como en las pruebas del núcleo.
        assert!((resumen.memoria_pct.expect("memoria") - 62.0).abs() < 0.01);
        assert!((resumen.disco_pct.expect("disco") - 28.0).abs() < 0.01);
        assert_eq!(resumen.red_rx_bps, Some(100.0));
        assert_eq!(resumen.red_tx_bps, Some(200.0));
        assert_eq!(resumen.uptime_seg, Some(14 * 86_400 + 2 * 3_600));

        // Sin sondeo anterior no hay tasa de red.
        let sin_anterior = resumen_de_sondeo(&actual, None);
        assert!(sin_anterior.red_rx_bps.is_none());
        assert!(sin_anterior.red_tx_bps.is_none());
    }

    #[test]
    fn las_metricas_completas_llevan_los_bytes() {
        let mut sondeo = sondeo_ok();
        sondeo.red_rx_bytes = Some(1234);
        sondeo.red_tx_bytes = Some(5678);
        let metricas = metricas_de_sondeo(&sondeo, None);
        assert_eq!(metricas.nucleos, Some(4));
        assert_eq!(metricas.carga_5m, Some(1.0));
        assert_eq!(metricas.carga_15m, Some(1.0));
        assert_eq!(metricas.memoria_total_kb, Some(1000));
        assert_eq!(metricas.disco_usado_kb, Some(280));
        assert_eq!(metricas.red_rx_bytes, Some(1234));
        assert_eq!(metricas.red_tx_bytes, Some(5678));
    }

    #[test]
    fn los_fallos_del_sondeo_se_mapean_con_su_tipo_y_motor() {
        let mut sondeo = sondeo_ok();
        sondeo.fallos = Some(FallosSondeo {
            unidades: FallosUnidades {
                total: 2,
                nombres: vec!["copia.service".to_string()],
            },
            contenedores: Some(FallosContenedores {
                motor: Some(MotorNombre::Docker),
                total: 1,
                nombres: vec!["panel".to_string()],
            }),
        });
        let fallos = fallos_de_sondeo(Some(&sondeo));
        assert_eq!(fallos.len(), 2);
        assert_eq!(fallos[0].tipo, TipoFalloMovil::Servicio);
        assert_eq!(fallos[0].nombre, "copia.service");
        assert_eq!(fallos[0].estado, "fallido");
        assert_eq!(fallos[1].tipo, TipoFalloMovil::Contenedor);
        assert_eq!(fallos[1].motor, Some(MotorNombreMovil::Docker));
    }

    #[test]
    fn la_duracion_solo_sale_si_se_midio() {
        let mut sondeo = sondeo_ok();
        assert_eq!(duracion_de_sondeo(&sondeo), Some(640));
        sondeo.duracion_ms = 0;
        assert_eq!(duracion_de_sondeo(&sondeo), None);
    }

    #[test]
    fn un_host_sin_sondeo_no_tiene_resumen_ni_fallos() {
        assert!(fallos_de_sondeo(None).is_empty());
        assert_eq!(
            estado_de(None, false, true, &Umbrales::default()).0,
            EstadoFlotaMovil::Fria
        );
    }
}
