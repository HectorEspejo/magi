//! `magi sincronizar` y `magi sincronizar estado` (Fase 10, con la corrección
//! 10b: R83).
//!
//! Antes de aplicar borrados de hosts y túneles que llegan por la
//! sincronización, la orden pregunta al servidor de sesiones qué tiene en marcha
//! (la consulta de `magi servidor estado`, con plazo y sin arrancarlo nunca):
//!
//! - sin servidor, aplica (sin servidor no hay pestañas, túneles ni
//!   transferencias que cerrar);
//! - con el servidor de esta versión, aplaza los borrados de los hosts que
//!   tienen trabajo vivo (y los de sus túneles);
//! - con un servidor de otra versión o que no responde, aplaza todos los de
//!   hosts y túneles: «sin respuesta» no es «sin trabajo».
//!
//! Un borrado aplazado no es un error (código de salida 0): queda como conflicto
//! `borrado` y se resuelve desde la TUI, como siempre. Aquí vive el cuerpo de las
//! dos órdenes para poder probarlo con un llavero en memoria; `main.rs` solo
//! abre el llavero real, purga la clave anterior y vuelca la salida.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension as _};

use crate::config::{Config, Rutas};
use crate::llavero::LlaveroSecreto;
use crate::servidor::{consultar_trabajo, ConsultaServidor};
use crate::sincronizacion::{
    carpeta, EstadoPasada, OpcionesPasada, PoliticaBorrados, ResultadoPasada, ESPERA_CERROJO,
};
use magi_nucleo::almacen::{conflictos, equipos as equipos_bd};

/// Lo que una orden saca por la salida estándar y por la de errores, y su código
/// de salida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalidaOrden {
    pub codigo: i32,
    pub salida: String,
    pub errores: String,
}

impl SalidaOrden {
    fn nueva() -> Self {
        Self {
            codigo: 0,
            salida: String::new(),
            errores: String::new(),
        }
    }

    fn con_error(codigo: i32, mensaje: impl AsRef<str>) -> Self {
        Self {
            codigo,
            salida: String::new(),
            errores: format!("{}\n", mensaje.as_ref()),
        }
    }

    /// Escribe la salida donde toca (estándar y errores).
    pub fn volcar(&self) {
        print!("{}", self.salida);
        eprint!("{}", self.errores);
    }
}

/// Qué se dice cuando no se pudo preguntar al servidor.
pub const SIN_RESPUESTA: &str = "no se pudo preguntar al servidor";

/// Pregunta al servidor de sesiones desde una orden de la CLI, que es síncrona:
/// levanta un runtime solo para eso. No se puede llamar desde dentro de otro
/// runtime de tokio (las pruebas asíncronas la llaman con `spawn_blocking`).
pub fn consultar_en_runtime(rutas: &Rutas, plazo: Duration) -> ConsultaServidor {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(consultar_trabajo(rutas, plazo)),
        Err(error) => ConsultaServidor::Fallo(format!(
            "no se pudo preparar la consulta al servidor: {error}"
        )),
    }
}

/// id del host al que pertenece un elemento que puede tener trabajo vivo: el
/// propio host, o el host de un túnel. `None` si el elemento ya no está.
fn host_de(conexion: &Connection, tipo: &str, uuid: &str) -> rusqlite::Result<Option<i64>> {
    let consulta = match tipo {
        "host" => "SELECT id FROM HOSTS WHERE uuid = ?1",
        "tunel" => "SELECT host_id FROM TUNELES WHERE uuid = ?1",
        _ => return Ok(None),
    };
    conexion
        .query_row(consulta, params![uuid], |fila| fila.get(0))
        .optional()
}

/// Por qué se aplaza el borrado de un host o de un túnel según lo que dijo el
/// servidor; `None` si se puede aplicar.
///
/// Un túnel se aplaza si su host tiene trabajo vivo. Con `Fallo`, todo host y
/// todo túnel se aplaza: no se sabe lo que hay en marcha.
pub fn motivo_de_aplazo(
    consulta: &ConsultaServidor,
    conexion: &Connection,
    tipo: &str,
    uuid: &str,
) -> Option<String> {
    if tipo != "host" && tipo != "tunel" {
        return None;
    }
    match consulta {
        ConsultaServidor::Sin => None,
        ConsultaServidor::Fallo(_) => Some(SIN_RESPUESTA.to_string()),
        ConsultaServidor::Trabajo(hosts) => {
            let host_id = match host_de(conexion, tipo, uuid) {
                Ok(Some(host_id)) => host_id,
                // Ya no está en el inventario: no hay nada que cerrar.
                Ok(None) => return None,
                // Sin poder mirarlo, mejor no borrar.
                Err(_) => return Some("no se pudo comprobar el host en el inventario".to_string()),
            };
            let trabajo = hosts.get(&host_id).filter(|trabajo| trabajo.hay())?;
            let de_quien = if tipo == "tunel" {
                "su host tiene"
            } else {
                "tiene"
            };
            Some(format!(
                "{de_quien} trabajo vivo en el servidor de sesiones: {}",
                trabajo.describir()
            ))
        }
    }
}

fn plural(cantidad: usize, singular: &str, plural: &str) -> String {
    if cantidad == 1 {
        format!("1 {singular}")
    } else {
        format!("{cantidad} {plural}")
    }
}

/// `magi sincronizar`: una pasada del inventario cifrado sin abrir la TUI.
///
/// `llavero` es el ya abierto por quien llama: con la sincronización activa
/// siempre viene; sin carpeta no se lee el llavero (la pasada devolverá «sin
/// configurar» y no hay clave que buscar). Pregunta al servidor de sesiones con
/// el plazo dado, antes de tomar el cerrojo, y solo si hay una pasada que hacer.
///
/// Código de salida: 0 si todo va bien (también con borrados aplazados), 1 con
/// error y 2 si hace falta decidir algo (borrados masivos o clave nueva) sin
/// haber aplicado nada.
pub fn sincronizar(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    llavero: Option<&dyn LlaveroSecreto>,
    plazo: Duration,
) -> SalidaOrden {
    ejecutar_pasada(conexion, rutas, config, llavero, || {
        consultar_en_runtime(rutas, plazo)
    })
}

/// El cuerpo de [`sincronizar`] con la consulta al servidor inyectada (solo se
/// llama si hay una pasada que hacer, antes de tomar el cerrojo): las pruebas
/// le dan respuestas hechas.
pub fn ejecutar_pasada(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    llavero: Option<&dyn LlaveroSecreto>,
    consultar: impl FnOnce() -> ConsultaServidor,
) -> SalidaOrden {
    let claves = match llavero {
        Some(llavero) => match crate::sincronizacion::equipos::claves_para_leer(config, llavero) {
            Ok(claves) => claves,
            Err(error) => {
                return SalidaOrden::con_error(
                    1,
                    format!("no se pudo leer la clave de sincronización del llavero: {error}"),
                );
            }
        },
        None => Vec::new(),
    };
    // Sin clave la pasada no hace nada (sin configurar o sin clave): no hay
    // borrados que decidir ni motivo para llamar al servidor.
    let consulta = if claves.is_empty() {
        ConsultaServidor::Sin
    } else {
        consultar()
    };
    let _cerrojo = match carpeta::adquirir_cerrojo(rutas, Some(ESPERA_CERROJO)) {
        Ok(Some(cerrojo)) => cerrojo,
        Ok(None) => {
            return SalidaOrden::con_error(
                1,
                "otro proceso está sincronizando ahora mismo; no se ha hecho nada (inténtalo dentro de un momento)",
            );
        }
        Err(error) => {
            return SalidaOrden::con_error(
                1,
                format!("no se pudo tomar el cerrojo de sincronización: {error:#}"),
            );
        }
    };
    // Por qué se aplazó cada borrado, para decirlo al final.
    let motivos: RefCell<BTreeMap<String, String>> = RefCell::new(BTreeMap::new());
    let trabajo_vivo = |tipo: &str, uuid: &str| -> bool {
        match motivo_de_aplazo(&consulta, conexion, tipo, uuid) {
            Some(motivo) => {
                motivos.borrow_mut().insert(uuid.to_string(), motivo);
                true
            }
            None => false,
        }
    };
    let opciones = OpcionesPasada {
        politica_borrados: PoliticaBorrados::Preguntar,
        trabajo_vivo: &trabajo_vivo,
    };
    match crate::sincronizacion::pasada(conexion, rutas, config, &claves, &opciones) {
        Ok(resultado) => {
            let propio = equipos_bd::este(conexion)
                .ok()
                .map(|equipo| carpeta::nombre_fichero(&equipo.uuid));
            texto_de_la_pasada(
                &resultado,
                &consulta,
                &motivos.borrow(),
                config.sincronizacion.carpeta.as_deref().unwrap_or(""),
                propio.as_deref(),
            )
        }
        Err(error) => SalidaOrden::con_error(1, format!("la sincronización falló: {error:#}")),
    }
}

/// La salida de una pasada (§5.3 de la Fase 9b): una línea por tema, los avisos
/// con `!` y un resumen al final. Un borrado aplazado no es un error.
fn texto_de_la_pasada(
    resultado: &ResultadoPasada,
    consulta: &ConsultaServidor,
    motivos: &BTreeMap<String, String>,
    carpeta_configurada: &str,
    fichero_propio: Option<&str>,
) -> SalidaOrden {
    let mut orden = SalidaOrden::nueva();
    let salida = &mut orden.salida;
    match &resultado.estado {
        EstadoPasada::AlDia => {
            salida.push_str(&format!("Sincronización · carpeta {carpeta_configurada}\n"));
            let mut equipos: Vec<String> = resultado.equipos_importados.clone();
            equipos.extend(
                resultado
                    .equipos_al_dia
                    .iter()
                    .map(|nombre| format!("{nombre} (al día)")),
            );
            if !equipos.is_empty() {
                salida.push_str(&format!("  importados: {}\n", equipos.join(" · ")));
            }
            match &resultado.resumen {
                Some(resumen) => salida.push_str(&format!(
                    "  cambios: {} creados · {} actualizados · {} borrados · {} conflictos · {} huellas\n",
                    resumen.creados,
                    resumen.actualizados,
                    resumen.borrados,
                    resumen.conflictos,
                    resumen.huellas
                )),
                None => salida.push_str("  sin cambios\n"),
            }
            for aviso in &resultado.avisos {
                salida.push_str(&format!("  ! {aviso}\n"));
            }
            if !resultado.aplazados.is_empty() {
                if let ConsultaServidor::Fallo(detalle) = consulta {
                    salida.push_str(&format!(
                        "  aplazados los borrados de hosts y túneles: {SIN_RESPUESTA} ({detalle})\n"
                    ));
                }
            }
            for aplazado in &resultado.aplazados {
                let que = if aplazado.tipo == "tunel" {
                    format!("el túnel «{}»", aplazado.nombre)
                } else {
                    aplazado.nombre.clone()
                };
                let motivo = motivos
                    .get(&aplazado.uuid)
                    .map(String::as_str)
                    .unwrap_or("tiene trabajo vivo");
                salida.push_str(&format!("  aplazado: borrar {que} ({motivo})\n"));
            }
            if resultado.exportado {
                match fichero_propio {
                    Some(nombre) => salida.push_str(&format!("  exportado: {nombre}\n")),
                    None => salida.push_str("  exportado: el fichero de este equipo\n"),
                }
            }
            if resultado.conflictos_pendientes > 0 {
                salida.push_str(&format!(
                    "  conflictos pendientes: {} (resuélvelos desde la vista CONFLICTOS de la TUI)\n",
                    resultado.conflictos_pendientes
                ));
            }
            let mut resumen = Vec::new();
            if !resultado.avisos.is_empty() {
                resumen.push(plural(resultado.avisos.len(), "aviso", "avisos"));
            }
            if !resultado.aplazados.is_empty() {
                resumen.push(plural(
                    resultado.aplazados.len(),
                    "borrado aplazado",
                    "borrados aplazados",
                ));
            }
            if resumen.is_empty() {
                salida.push_str("sin avisos\n");
            } else {
                salida.push_str(&format!("{}\n", resumen.join(" · ")));
            }
        }
        otro => {
            // Lo demás no es una pasada hecha: los avisos que haya y el motivo.
            for aviso in &resultado.avisos {
                salida.push_str(&format!("! {aviso}\n"));
            }
            let (codigo, mensaje) = match otro {
                EstadoPasada::SinConfigurar => (
                    1,
                    "la sincronización no está configurada: añade [sincronizacion] carpeta a config.toml o configúrala desde la vista SINCRONIZACIÓN".to_string(),
                ),
                EstadoPasada::SinClave => (
                    1,
                    "hay carpeta de sincronización pero no hay clave en el llavero: abre la TUI para crear la sincronización o para unirte con la clave de la carpeta".to_string(),
                ),
                EstadoPasada::Quitado => (
                    1,
                    "este equipo ha sido quitado de la sincronización; vuelve a unirte como equipo nuevo desde la vista SINCRONIZACIÓN".to_string(),
                ),
                EstadoPasada::EsperandoClave(nombre) => (
                    2,
                    format!(
                        "el fichero del equipo «{nombre}» no se abre con la clave de este equipo: introduce la clave nueva desde la vista SINCRONIZACIÓN; no se ha aplicado nada"
                    ),
                ),
                EstadoPasada::DecisionPendiente(pendientes) => {
                    let mut texto = format!(
                        "importación pendiente de confirmación: se borrarían {} de {} hosts:",
                        pendientes.hosts.len(),
                        pendientes.total_hosts
                    );
                    for host in &pendientes.hosts {
                        texto.push_str(&format!("\n  · {host}"));
                    }
                    texto.push_str(&format!(
                        "\ncopia de magi.db guardada antes de aplicar nada: {}\nabre la TUI (vista SINCRONIZACIÓN) para decidir; no se ha aplicado nada",
                        pendientes.copia.display()
                    ));
                    (2, texto)
                }
                EstadoPasada::Error(mensaje) => (1, format!("error de sincronización: {mensaje}")),
                EstadoPasada::AlDia => unreachable!("el caso de la pasada hecha va arriba"),
            };
            orden.codigo = codigo;
            orden.errores = format!("{mensaje}\n");
        }
    }
    orden
}

/// `magi sincronizar estado`: la carpeta, los equipos y los conflictos, y los
/// borrados aplazados con su motivo. No descifra nada ni lee la clave del
/// llavero. Solo pregunta al servidor si hay borrados aplazados, y con el mismo
/// plazo y las mismas reglas que la pasada.
pub fn estado(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    plazo: Duration,
) -> anyhow::Result<SalidaOrden> {
    estado_con(conexion, config, &rutas.hogar, || {
        consultar_en_runtime(rutas, plazo)
    })
}

/// El cuerpo de [`estado`] con la consulta al servidor inyectada (solo se llama
/// si hay algún borrado aplazado).
pub fn estado_con(
    conexion: &Connection,
    config: &Config,
    hogar: &std::path::Path,
    consultar: impl FnOnce() -> ConsultaServidor,
) -> anyhow::Result<SalidaOrden> {
    let hay_aplazados = config.sincronizacion.carpeta.is_some()
        && conflictos::listar_pendientes(conexion)?
            .iter()
            .any(|conflicto| conflicto.tipo == conflictos::TIPO_BORRADO);
    let consulta = if hay_aplazados {
        consultar()
    } else {
        ConsultaServidor::Sin
    };
    let explicar = |conflicto: &conflictos::Conflicto| -> String {
        match motivo_de_aplazo(&consulta, conexion, &conflicto.elemento_tipo, &conflicto.uuid) {
            Some(motivo) => match &consulta {
                ConsultaServidor::Fallo(detalle) => format!("{motivo} ({detalle})"),
                _ => motivo,
            },
            None => "ya no tiene trabajo vivo: acéptalo (a) o descártalo (d) en la vista CONFLICTOS de la TUI"
                .to_string(),
        }
    };
    let texto = crate::sincronizacion::estado_texto(conexion, config, hogar, &explicar)?;
    Ok(SalidaOrden {
        codigo: 0,
        salida: texto,
        errores: String::new(),
    })
}
