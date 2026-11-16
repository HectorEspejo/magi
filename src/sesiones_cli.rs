//! La CLI de las sesiones persistentes (Fase 16, §5.5): `magi sesion` y
//! `magi sesiones`.
//!
//! Aquí vive lo que no habla con el servidor y se puede probar sin red:
//!
//! - **`magi sesion <nombre> [--host <host>]`**: resuelve de qué ficha se
//!   trata (`resolver_sesion`) con mensajes que dicen qué hacer (varios hosts,
//!   ninguna ficha, nombre parecido). Lo que abre MAGI engancharse lo hace
//!   `app::ejecutar` con `AperturaInicial::Sesion`.
//! - **`magi sesiones [--json]`**: arma las filas (`armar_filas`) con el
//!   estado que difunde el servidor y las pinta como tabla (`texto_tabla`) o
//!   como JSON (`json`). La conversación con el servidor (lo arranca si hace
//!   falta, como `magi tunel`) está en `main.rs`; las reglas de cuándo un
//!   estado es de fiar (`lista_fresca`) están aquí.
//!
//! Una operación sin diálogos: lo que el servidor no puede comprobar sin
//! preguntar sale `inalcanzable`, y lo que no llega a tiempo, `sin_comprobar`.
//! Nunca `terminada` por no haber podido mirar. Ni contenido de las sesiones
//! ni comandos: solo el título que ya viaja por el protocolo.
//!
//! # Salida JSON de `magi sesiones --json`
//!
//! Una lista (vacía si no hay fichas) con un objeto por ficha, ordenada por
//! host y nombre, con estos campos y en este orden. Son estables: solo se
//! añaden campos al final.
//!
//! | Campo | Tipo | Significado |
//! |---|---|---|
//! | `uuid` | texto | Identidad de la ficha (la misma en todos los equipos) |
//! | `host` | texto | Nombre del host en el inventario |
//! | `nombre` | texto | Nombre de la sesión en el tmux de MAGI |
//! | `estado` | texto | `viva_libre`, `viva_enganchada`, `terminada`, `sin_comprobar`, `inalcanzable` o `sin_tmux` |
//! | `equipo` | texto o `null` | Quién la tiene: la etiqueta del equipo si está enganchada; `null` si está libre o no consta |
//! | `actividad` | entero o `null` | Última actividad, en segundos Unix según el reloj de este equipo (corregido con la hora del host); `null` si no vive o el host no la dio |
//! | `atencion` | booleano | La sesión pide atención (la campana de tmux) |
//! | `titulo` | texto o `null` | Título del programa que corre dentro, si vive |
//! | `comprobada_en` | entero o `null` | Cuándo se miró su host (segundos Unix); `null` en `sin_comprobar` |

use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use magi_nucleo::sesiones_persistentes::validadores::{validar_nombre, ErrorFicha};
use magi_nucleo::sesiones_persistentes::{Ficha, VersionTmux};

use crate::modelo::Host;
use crate::protocolo::{ErrorComprobacion, EstadoFicha, FichaVista, ListaPersistentes};

/// Un host comprobado hace menos de esto no se vuelve a mirar: el servidor
/// no repite una comprobación reciente (§5.2), así que un estado de esa
/// antigüedad ya es la respuesta a nuestra consulta.
pub const REPETICION_MINIMA_SEG: i64 = 5;

/// Cuántas sugerencias de nombre parecido se dan como mucho.
const MAXIMO_PARECIDAS: usize = 5;

// ------------------------------------------------------------ magi sesion

/// La sesión a la que hay que engancharse, resuelta contra el inventario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SesionResuelta {
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
}

/// Por qué `magi sesion` no sabe a qué sesión engancharse. `Display` dice qué
/// hacer, en castellano.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorResolucion {
    /// El nombre no cumple `^[A-Za-z0-9_-]{1,64}$`.
    NombreNoValido { nombre: String, motivo: ErrorFicha },
    /// `--host` no es un host del inventario.
    HostNoExiste { host: String },
    /// No hay ficha con ese nombre (en ese host, si se dio `--host`), con las
    /// de nombre parecido como sugerencia: `(host, nombre)`.
    SinFicha {
        nombre: String,
        host: Option<String>,
        parecidas: Vec<(String, String)>,
    },
    /// El nombre está en varios hosts y no se dijo cuál.
    Ambigua { nombre: String, hosts: Vec<String> },
}

impl fmt::Display for ErrorResolucion {
    fn fmt(&self, formato: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorResolucion::NombreNoValido { nombre, motivo } => write!(
                formato,
                "«{nombre}» no es un nombre de sesión válido: {motivo}"
            ),
            ErrorResolucion::HostNoExiste { host } => {
                write!(formato, "no existe el host «{host}»")
            }
            ErrorResolucion::SinFicha {
                nombre,
                host,
                parecidas,
            } => {
                match host {
                    Some(host) => write!(
                        formato,
                        "el host «{host}» no tiene ninguna sesión persistente «{nombre}»."
                    )?,
                    None => write!(
                        formato,
                        "no hay ninguna sesión persistente «{nombre}» en el inventario."
                    )?,
                }
                if !parecidas.is_empty() {
                    let lista: Vec<String> = parecidas
                        .iter()
                        .map(|(host, nombre)| format!("{nombre} (en {host})"))
                        .collect();
                    write!(formato, "\nParecidas: {}.", lista.join(", "))?;
                }
                write!(
                    formato,
                    "\nMira las que hay con «magi sesiones» o crea una desde la TUI."
                )
            }
            ErrorResolucion::Ambigua { nombre, hosts } => write!(
                formato,
                "la sesión «{nombre}» está en varios hosts ({}).\nIndica cuál con --host <host>.",
                hosts.join(", ")
            ),
        }
    }
}

impl std::error::Error for ErrorResolucion {}

/// Decide a qué ficha se refiere `magi sesion <nombre> [--host <host>]`.
///
/// Valida el nombre antes de mirar nada. Con `--host`, el nombre del host es
/// exacto (como en `magi <host>`) y la ficha es la de ese host. Sin él, el
/// nombre tiene que estar en un solo host: si está en varios, pide `--host` y
/// dice cuáles; si no está en ninguno, dice las fichas de nombre parecido.
pub fn resolver_sesion(
    nombre: &str,
    host: Option<&str>,
    hosts: &[Host],
    fichas: &[Ficha],
) -> Result<SesionResuelta, ErrorResolucion> {
    validar_nombre(nombre).map_err(|motivo| ErrorResolucion::NombreNoValido {
        nombre: nombre.to_string(),
        motivo,
    })?;
    let nombres: HashMap<i64, &str> = hosts
        .iter()
        .map(|host| (host.id, host.nombre.as_str()))
        .collect();

    let con_ese_nombre: Vec<&Ficha> = fichas
        .iter()
        .filter(|ficha| ficha.nombre == nombre && nombres.contains_key(&ficha.host_id))
        .collect();

    if let Some(pedido) = host {
        let Some(elegido) = hosts.iter().find(|candidato| candidato.nombre == pedido) else {
            return Err(ErrorResolucion::HostNoExiste {
                host: pedido.to_string(),
            });
        };
        return match con_ese_nombre
            .iter()
            .find(|ficha| ficha.host_id == elegido.id)
        {
            Some(ficha) => Ok(SesionResuelta {
                host_id: elegido.id,
                host_nombre: elegido.nombre.clone(),
                nombre: ficha.nombre.clone(),
            }),
            None => Err(ErrorResolucion::SinFicha {
                nombre: nombre.to_string(),
                host: Some(elegido.nombre.clone()),
                parecidas: parecidas(nombre, fichas, &nombres),
            }),
        };
    }

    match con_ese_nombre.as_slice() {
        [] => Err(ErrorResolucion::SinFicha {
            nombre: nombre.to_string(),
            host: None,
            parecidas: parecidas(nombre, fichas, &nombres),
        }),
        [ficha] => Ok(SesionResuelta {
            host_id: ficha.host_id,
            host_nombre: nombres[&ficha.host_id].to_string(),
            nombre: ficha.nombre.clone(),
        }),
        varias => {
            let mut hosts: Vec<String> = varias
                .iter()
                .map(|ficha| nombres[&ficha.host_id].to_string())
                .collect();
            hosts.sort();
            Err(ErrorResolucion::Ambigua {
                nombre: nombre.to_string(),
                hosts,
            })
        }
    }
}

/// Fichas de nombre parecido al buscado, como `(host, nombre)`: las que lo
/// contienen o están contenidas, y las que difieren en pocas letras (una
/// errata). Las más cercanas primero y solo unas pocas.
fn parecidas(
    buscado: &str,
    fichas: &[Ficha],
    nombres_de_host: &HashMap<i64, &str>,
) -> Vec<(String, String)> {
    let buscado = buscado.to_lowercase();
    let mut candidatas: Vec<(usize, String, String)> = fichas
        .iter()
        .filter_map(|ficha| {
            let host = nombres_de_host.get(&ficha.host_id)?;
            let candidata = ficha.nombre.to_lowercase();
            let distancia = distancia_de_edicion(&buscado, &candidata);
            let mayor = buscado.chars().count().max(candidata.chars().count());
            let contiene = candidata.contains(&buscado) || buscado.contains(&candidata);
            (contiene || distancia <= (mayor / 3).max(1)).then(|| {
                // Contener cuenta como casi igual, pero detrás de una errata de
                // una letra.
                let cercania = if contiene {
                    distancia.min(2)
                } else {
                    distancia
                };
                (cercania, host.to_string(), ficha.nombre.clone())
            })
        })
        .collect();
    candidatas.sort();
    candidatas.dedup();
    candidatas
        .into_iter()
        .take(MAXIMO_PARECIDAS)
        .map(|(_, host, nombre)| (host, nombre))
        .collect()
}

/// Distancia de Levenshtein entre dos textos (por caracteres).
fn distancia_de_edicion(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut anterior: Vec<usize> = (0..=b.len()).collect();
    for (i, caracter_a) in a.iter().enumerate() {
        let mut actual = vec![i + 1];
        for (j, caracter_b) in b.iter().enumerate() {
            let sustitucion = anterior[j] + usize::from(caracter_a != caracter_b);
            actual.push(sustitucion.min(anterior[j + 1] + 1).min(actual[j] + 1));
        }
        anterior = actual;
    }
    anterior[b.len()]
}

// ---------------------------------------------------------- magi sesiones

/// Una fila de `magi sesiones`: lo que se sabe de una ficha. Es también el
/// objeto de `--json` (ver el cuadro de campos arriba).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilaSesion {
    pub uuid: String,
    pub host: String,
    pub nombre: String,
    pub estado: EstadoFicha,
    pub equipo: Option<String>,
    pub actividad: Option<i64>,
    pub atencion: bool,
    pub titulo: Option<String>,
    pub comprobada_en: Option<i64>,
}

/// ¿Es el estado de esta ficha la respuesta a una consulta hecha en
/// `umbral` (o después)? Una ficha nunca comprobada (0) no lo es.
fn es_fresca(vista: &FichaVista, umbral: i64) -> bool {
    vista.comprobada_en > 0 && vista.comprobada_en >= umbral
}

/// El umbral de frescura de una consulta hecha en la hora `peticion` (segundos
/// Unix): lo que el servidor comprobó hace menos de
/// `REPETICION_MINIMA_SEG` antes ya cuenta, porque no lo repite.
#[must_use]
pub fn umbral_de_frescura(peticion: i64) -> i64 {
    peticion.saturating_sub(REPETICION_MINIMA_SEG)
}

/// ¿Trae esta lista el estado fresco de todas las fichas? Una ventana nueva
/// recibe primero la lista que había al irse la última ventana, que puede ser
/// vieja: no es la respuesta hasta que cada ficha se haya comprobado después
/// de la consulta.
#[must_use]
pub fn lista_fresca(fichas: &[Ficha], lista: &ListaPersistentes, umbral: i64) -> bool {
    fichas.iter().all(|ficha| {
        lista
            .fichas
            .iter()
            .find(|vista| vista.uuid == ficha.uuid)
            .is_some_and(|vista| es_fresca(vista, umbral))
    })
}

/// Arma las filas de `magi sesiones`, ordenadas por host y nombre.
///
/// `lista` es lo último que difundió el servidor (`None` si no hubo
/// servidor). Una ficha cuyo estado no es fresco (`umbral`, ver
/// `umbral_de_frescura`) sale `sin_comprobar`: nunca se enseña un estado viejo
/// como si fuera de ahora, y nunca `terminada` por no haber podido mirar.
#[must_use]
pub fn armar_filas(
    fichas: &[Ficha],
    hosts: &[Host],
    lista: Option<&ListaPersistentes>,
    umbral: i64,
) -> Vec<FilaSesion> {
    let nombres: HashMap<i64, &str> = hosts
        .iter()
        .map(|host| (host.id, host.nombre.as_str()))
        .collect();
    let mut filas: Vec<FilaSesion> = fichas
        .iter()
        .map(|ficha| {
            let host = nombres.get(&ficha.host_id).map_or_else(
                || format!("#{}", ficha.host_id),
                |nombre| nombre.to_string(),
            );
            let vista = lista
                .and_then(|lista| lista.fichas.iter().find(|vista| vista.uuid == ficha.uuid))
                .filter(|vista| es_fresca(vista, umbral));
            let Some(vista) = vista else {
                return FilaSesion {
                    uuid: ficha.uuid.clone(),
                    host,
                    nombre: ficha.nombre.clone(),
                    estado: EstadoFicha::SinComprobar,
                    equipo: None,
                    actividad: None,
                    atencion: false,
                    titulo: None,
                    comprobada_en: None,
                };
            };
            let sesion = vista.sesion.as_ref();
            // La actividad viene en la hora del host: se pasa a la de este
            // equipo con la hora que dio el host al comprobar, para que dos
            // hosts con el reloj distinto no dejen horas incomparables.
            let ahora_del_host = lista
                .and_then(|lista| {
                    lista
                        .hosts
                        .iter()
                        .find(|visto| visto.host_id == ficha.host_id)
                })
                .map(|visto| visto.ahora);
            // Una hora del host o una actividad que no se pudo leer valen 0
            // (`lectura::numero_i64`): sin ellas no se sabe cuándo fue, y no
            // se inventa («ahora» o «hace 20000 d»).
            let actividad = match (sesion, ahora_del_host) {
                (Some(sesion), Some(ahora)) if ahora > 0 && sesion.actividad > 0 => Some(
                    vista
                        .comprobada_en
                        .saturating_sub(ahora.saturating_sub(sesion.actividad)),
                ),
                _ => None,
            };
            FilaSesion {
                uuid: ficha.uuid.clone(),
                host,
                nombre: ficha.nombre.clone(),
                estado: vista.estado,
                equipo: sesion
                    .filter(|_| vista.estado == EstadoFicha::VivaEnganchada)
                    .and_then(|sesion| sesion.equipo.clone()),
                actividad,
                atencion: sesion.is_some_and(|sesion| sesion.campana),
                titulo: sesion.and_then(|sesion| sesion.titulo.clone()),
                comprobada_en: Some(vista.comprobada_en),
            }
        })
        .collect();
    filas.sort_by(|a, b| (&a.host, &a.nombre).cmp(&(&b.host, &b.nombre)));
    filas
}

/// Avisos con instrucción sobre los hosts que no se pudieron mirar o no tienen
/// tmux, para quien lee la salida. Van a la salida de errores: la tabla y el
/// JSON quedan limpios.
#[must_use]
pub fn avisos(
    filas: &[FilaSesion],
    lista: Option<&ListaPersistentes>,
    hosts: &[Host],
) -> Vec<String> {
    let mut avisos = Vec::new();
    let nombres: HashMap<i64, &str> = hosts
        .iter()
        .map(|host| (host.id, host.nombre.as_str()))
        .collect();
    if let Some(lista) = lista {
        let mut vistos: Vec<_> = lista
            .hosts
            .iter()
            // Solo los hosts con alguna fila comprobada de verdad: lo que el
            // servidor sabía de uno antes de la consulta puede ser viejo, y
            // de un host que aún no se ha mirado no se dice nada (sus filas
            // salen `sin_comprobar` y el aviso general lo explica).
            .filter(|visto| {
                nombres.get(&visto.host_id).is_some_and(|nombre| {
                    filas
                        .iter()
                        .any(|fila| fila.host == *nombre && fila.comprobada_en.is_some())
                })
            })
            .collect();
        vistos.sort_by_key(|visto| visto.host_id);
        for visto in vistos {
            let nombre = nombres[&visto.host_id];
            match (visto.error, visto.tmux) {
                (Some(ErrorComprobacion::NecesitaDialogo), _) => avisos.push(format!(
                    "«{nombre}» necesita una huella, una frase o una contraseña y esta orden no puede preguntarlas: conéctate a mano desde MAGI y vuelve a mirar."
                )),
                (Some(ErrorComprobacion::SinRespuesta), _) => avisos.push(format!(
                    "«{nombre}» no responde: sus sesiones salen «inalcanzable», no «terminada»."
                )),
                (Some(ErrorComprobacion::Plazo), _) => avisos.push(format!(
                    "«{nombre}» no respondió a tiempo: sus sesiones salen «inalcanzable», no «terminada»."
                )),
                (None, None) => avisos.push(format!(
                    "«{nombre}» no tiene tmux (se necesita la 3.0 o posterior): instálalo en el host."
                )),
                (None, Some(version)) if version < VersionTmux::MINIMA => avisos.push(format!(
                    "«{nombre}» tiene un tmux demasiado antiguo (se necesita la 3.0 o posterior): actualízalo en el host."
                )),
                (None, Some(_)) => {}
            }
        }
    }
    if filas
        .iter()
        .any(|fila| fila.estado == EstadoFicha::SinComprobar)
    {
        avisos.push(
            "Hay sesiones «sin_comprobar»: el servidor no las miró a tiempo. Vuelve a lanzar «magi sesiones»."
                .to_string(),
        );
    }
    avisos
}

/// La salida de `--json`: la lista de filas, con sangría y un salto de línea
/// al final.
#[must_use]
pub fn json(filas: &[FilaSesion]) -> String {
    let mut texto = serde_json::to_string_pretty(filas).unwrap_or_else(|_| "[]".to_string());
    texto.push('\n');
    texto
}

/// Cómo se llama cada estado en la tabla.
fn etiqueta_de_estado(estado: EstadoFicha) -> &'static str {
    match estado {
        EstadoFicha::VivaLibre => "libre",
        EstadoFicha::VivaEnganchada => "enganchada",
        EstadoFicha::Terminada => "terminada",
        EstadoFicha::SinComprobar => "sin comprobar",
        EstadoFicha::Inalcanzable => "inalcanzable",
        EstadoFicha::SinTmux => "sin tmux",
    }
}

/// «hace 3 min», «hace 2 h»…: la edad de una hora respecto a `ahora`.
fn hace(segundos: i64) -> String {
    match segundos {
        ..=4 => "ahora".to_string(),
        5..=59 => format!("hace {segundos} s"),
        60..=3599 => format!("hace {} min", segundos / 60),
        3600..=86_399 => format!("hace {} h", segundos / 3600),
        _ => format!("hace {} d", segundos / 86_400),
    }
}

/// La tabla de `magi sesiones`: estado, nombre, host, quién la tiene,
/// actividad, atención (`!`) y título. `ahora` es la hora de este equipo
/// (segundos Unix), para decir cuánto hace de la actividad.
#[must_use]
pub fn texto_tabla(filas: &[FilaSesion], ahora: i64) -> String {
    if filas.is_empty() {
        return "No hay sesiones persistentes: créalas desde la TUI (prefijo N, en Sesiones).\n"
            .to_string();
    }
    let celdas: Vec<[String; 6]> = filas
        .iter()
        .map(|fila| {
            let equipo = match (fila.estado, &fila.equipo) {
                (_, Some(equipo)) => equipo.clone(),
                (EstadoFicha::VivaEnganchada, None) => "?".to_string(),
                _ => "—".to_string(),
            };
            [
                etiqueta_de_estado(fila.estado).to_string(),
                fila.nombre.clone(),
                fila.host.clone(),
                equipo,
                fila.actividad
                    .map_or_else(|| "—".to_string(), |hora| hace(ahora.saturating_sub(hora))),
                if fila.atencion { "!" } else { " " }.to_string(),
            ]
        })
        .collect();
    let cabecera = ["ESTADO", "NOMBRE", "HOST", "EQUIPO", "ACTIVIDAD", "!"];
    let mut anchos: [usize; 6] = cabecera.map(|titulo| titulo.chars().count());
    for fila in &celdas {
        for (ancho, celda) in anchos.iter_mut().zip(fila) {
            *ancho = (*ancho).max(celda.chars().count());
        }
    }
    let mut texto = format!("Sesiones persistentes: {}\n", filas.len());
    let linea = |campos: [&str; 6], titulo: &str| {
        let mut linea = String::new();
        for (campo, ancho) in campos.iter().zip(anchos) {
            linea.push_str(&format!("{campo:<ancho$}  "));
        }
        linea.push_str(titulo);
        linea.truncate(linea.trim_end().len());
        linea.push('\n');
        linea
    };
    texto.push_str(&linea(cabecera, "TÍTULO"));
    for (fila, celda) in filas.iter().zip(&celdas) {
        texto.push_str(&linea(
            [
                &celda[0], &celda[1], &celda[2], &celda[3], &celda[4], &celda[5],
            ],
            fila.titulo.as_deref().unwrap_or(""),
        ));
    }
    texto
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::protocolo::{HostVisto, ServidorTmux};
    use magi_nucleo::sesiones_persistentes::{OrigenFicha, SesionVista, VersionTmux};

    fn host(id: i64, nombre: &str) -> Host {
        Host {
            id,
            nombre: nombre.to_string(),
            grupo_id: None,
            direccion: "192.0.2.10".to_string(),
            puerto: 22,
            tipo: magi_nucleo::escritorio::TipoHost::Ssh,
            escritorio_dominio: None,
            escritorio_opciones: Default::default(),
            huella_escritorio: None,
            usuario: Some("usuario".to_string()),
            identidad_ref: crate::modelo::IdentidadRef::Auto,
            salto_host_id: None,
            multiplexar: false,
            keepalive_seg: None,
            opciones_extra: String::new(),
            servicios: String::new(),
            origen: crate::modelo::Origen::Manual,
            ultimo_estado: None,
            ultima_conexion_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            etiquetas: Vec::new(),
            grupo_nombre: None,
            salto_nombre: None,
            sftp_dir_local: None,
            sftp_dir_remoto: None,
            snippet_al_conectar_id: None,
            reconectar: true,
            tmux_persistente: false,
            portapapeles_remoto: false,
            sesiones_persistentes: false,
        }
    }

    fn ficha(id: i64, uuid: &str, host_id: i64, nombre: &str) -> Ficha {
        Ficha {
            id,
            uuid: uuid.to_string(),
            host_id,
            nombre: nombre.to_string(),
            carpeta: None,
            comando: Some("claude".to_string()),
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Manual,
            creada_por: "portatil".to_string(),
            creada_en: String::new(),
            actualizado_en: String::new(),
        }
    }

    fn sesion(nombre: &str, clientes: u32, equipo: Option<&str>, campana: bool) -> SesionVista {
        SesionVista {
            servidor: ServidorTmux::Magi,
            nombre: nombre.to_string(),
            clientes,
            equipo: equipo.map(str::to_string),
            actividad: 1_000_000 - 120,
            creada: 1_000_000 - 3_600,
            campana,
            ventanas: 1,
            titulo: Some("claude".to_string()),
        }
    }

    fn vista(
        uuid: &str,
        host_id: i64,
        nombre: &str,
        estado: EstadoFicha,
        sesion: Option<SesionVista>,
        comprobada_en: i64,
    ) -> FichaVista {
        FichaVista {
            uuid: uuid.to_string(),
            host_id,
            nombre: nombre.to_string(),
            estado,
            sesion,
            comprobada_en,
        }
    }

    fn visto(host_id: i64, comprobada_en: i64) -> HostVisto {
        HostVisto {
            host_id,
            comprobada_en,
            ahora: 1_000_000,
            tmux: Some(VersionTmux::nueva(3, 4)),
            error: None,
            sin_ficha: Vec::new(),
            habitual: Vec::new(),
        }
    }

    #[test]
    fn un_nombre_unico_se_resuelve_sin_pedir_host() {
        let hosts = [host(1, "web-01"), host(2, "db-01")];
        let fichas = [ficha(1, "u1", 1, "agente"), ficha(2, "u2", 2, "build")];
        let resuelta = resolver_sesion("build", None, &hosts, &fichas).unwrap();
        assert_eq!(resuelta.host_id, 2);
        assert_eq!(resuelta.host_nombre, "db-01");
        assert_eq!(resuelta.nombre, "build");
    }

    #[test]
    fn un_nombre_en_varios_hosts_pide_host_y_dice_cuales() {
        let hosts = [host(1, "web-01"), host(2, "db-01")];
        let fichas = [ficha(1, "u1", 1, "agente"), ficha(2, "u2", 2, "agente")];
        let error = resolver_sesion("agente", None, &hosts, &fichas).unwrap_err();
        assert_eq!(
            error,
            ErrorResolucion::Ambigua {
                nombre: "agente".to_string(),
                hosts: vec!["db-01".to_string(), "web-01".to_string()],
            }
        );
        let texto = error.to_string();
        assert!(texto.contains("db-01, web-01"), "{texto}");
        assert!(texto.contains("--host"), "{texto}");
        // Con --host se resuelve y ya no es ambiguo.
        let resuelta = resolver_sesion("agente", Some("web-01"), &hosts, &fichas).unwrap();
        assert_eq!(resuelta.host_id, 1);
    }

    #[test]
    fn sin_ficha_lo_dice_y_sugiere_las_parecidas() {
        let hosts = [host(1, "web-01"), host(2, "db-01")];
        let fichas = [
            ficha(1, "u1", 1, "agente-api"),
            ficha(2, "u2", 2, "backup"),
            ficha(3, "u3", 1, "otra-cosa"),
        ];
        let error = resolver_sesion("agente", None, &hosts, &fichas).unwrap_err();
        let ErrorResolucion::SinFicha {
            host, parecidas, ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert!(host.is_none());
        assert_eq!(
            parecidas,
            &[("web-01".to_string(), "agente-api".to_string())]
        );
        assert!(error.to_string().contains("agente-api (en web-01)"));
        // Una errata de una letra también.
        let error = resolver_sesion("bakcup", None, &hosts, &fichas).unwrap_err();
        let ErrorResolucion::SinFicha { parecidas, .. } = error else {
            panic!("no es SinFicha");
        };
        assert_eq!(parecidas, [("db-01".to_string(), "backup".to_string())]);
        // Y mayúsculas distintas.
        let error = resolver_sesion("BACKUP", None, &hosts, &fichas).unwrap_err();
        let ErrorResolucion::SinFicha { parecidas, .. } = error else {
            panic!("no es SinFicha");
        };
        assert_eq!(parecidas, [("db-01".to_string(), "backup".to_string())]);
    }

    #[test]
    fn con_host_la_ficha_es_la_de_ese_host() {
        let hosts = [host(1, "web-01"), host(2, "db-01")];
        let fichas = [ficha(1, "u1", 1, "agente")];
        let error = resolver_sesion("agente", Some("db-01"), &hosts, &fichas).unwrap_err();
        let ErrorResolucion::SinFicha {
            host, parecidas, ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!(host.as_deref(), Some("db-01"));
        // La que existe en otro host se sugiere con su host.
        assert_eq!(parecidas, &[("web-01".to_string(), "agente".to_string())]);
        assert!(error.to_string().contains("«db-01»"));
        // Un host que no existe no es lo mismo que un host sin la sesión.
        let error = resolver_sesion("agente", Some("nube"), &hosts, &fichas).unwrap_err();
        assert_eq!(
            error,
            ErrorResolucion::HostNoExiste {
                host: "nube".to_string()
            }
        );
    }

    #[test]
    fn un_nombre_que_no_vale_se_rechaza_antes_de_mirar_nada() {
        let hosts = [host(1, "web-01")];
        let fichas = [ficha(1, "u1", 1, "agente")];
        for malo in ["", "con espacio", "ñandú", "a;b", &"x".repeat(65)] {
            let error = resolver_sesion(malo, Some("no-existe"), &hosts, &fichas).unwrap_err();
            assert!(
                matches!(error, ErrorResolucion::NombreNoValido { .. }),
                "{malo:?}: {error:?}"
            );
        }
    }

    #[test]
    fn las_sesiones_se_distinguen_por_nombre_exacto() {
        // tmux casa prefijos en `-t`; aquí nunca: «agente» no es «agente-api».
        let hosts = [host(1, "web-01")];
        let fichas = [ficha(1, "u1", 1, "agente-api")];
        assert!(matches!(
            resolver_sesion("agente", None, &hosts, &fichas),
            Err(ErrorResolucion::SinFicha { .. })
        ));
    }

    #[test]
    fn la_distancia_de_edicion_cuenta_letras() {
        assert_eq!(distancia_de_edicion("agente", "agente"), 0);
        assert_eq!(distancia_de_edicion("agente", "agnete"), 2);
        assert_eq!(distancia_de_edicion("agente", "agent"), 1);
        assert_eq!(distancia_de_edicion("", "abc"), 3);
    }

    #[test]
    fn un_estado_viejo_no_se_enseña_como_si_fuera_de_ahora() {
        let hosts = [host(1, "web-01")];
        let fichas = [ficha(1, "u1", 1, "agente")];
        // La lista que había al irse la última ventana: comprobada hace un
        // minuto, con la sesión «terminada».
        let lista = ListaPersistentes {
            fichas: vec![vista(
                "u1",
                1,
                "agente",
                EstadoFicha::Terminada,
                None,
                1_000_000 - 60,
            )],
            hosts: vec![visto(1, 1_000_000 - 60)],
        };
        let umbral = umbral_de_frescura(1_000_000);
        assert!(!lista_fresca(&fichas, &lista, umbral));
        let filas = armar_filas(&fichas, &hosts, Some(&lista), umbral);
        assert_eq!(filas[0].estado, EstadoFicha::SinComprobar);
        assert_eq!(filas[0].comprobada_en, None);
        // Una comprobación de hace 4 s sí es de fiar: el servidor no la
        // repite antes de 5 s.
        let reciente = ListaPersistentes {
            fichas: vec![vista(
                "u1",
                1,
                "agente",
                EstadoFicha::VivaLibre,
                Some(sesion("agente", 0, None, false)),
                1_000_000 - 4,
            )],
            hosts: vec![visto(1, 1_000_000 - 4)],
        };
        assert!(lista_fresca(&fichas, &reciente, umbral));
        let filas = armar_filas(&fichas, &hosts, Some(&reciente), umbral);
        assert_eq!(filas[0].estado, EstadoFicha::VivaLibre);
        // Sin servidor, todo sin comprobar.
        let filas = armar_filas(&fichas, &hosts, None, umbral);
        assert_eq!(filas[0].estado, EstadoFicha::SinComprobar);
    }

    #[test]
    fn una_ficha_nunca_comprobada_no_vale_aunque_el_umbral_sea_bajo() {
        let fichas = [ficha(1, "u1", 1, "agente")];
        let lista = ListaPersistentes {
            fichas: vec![vista("u1", 1, "agente", EstadoFicha::SinComprobar, None, 0)],
            hosts: Vec::new(),
        };
        assert!(!lista_fresca(&fichas, &lista, 0));
        // Y una ficha que la lista no menciona tampoco.
        assert!(!lista_fresca(
            &fichas,
            &ListaPersistentes::default(),
            umbral_de_frescura(10)
        ));
    }

    #[test]
    fn un_host_que_no_responde_sale_inalcanzable_nunca_terminada() {
        let hosts = [host(1, "web-01")];
        let fichas = [ficha(1, "u1", 1, "agente")];
        let mut host_visto = visto(1, 1_000_000);
        host_visto.error = Some(ErrorComprobacion::NecesitaDialogo);
        host_visto.tmux = None;
        let lista = ListaPersistentes {
            fichas: vec![vista(
                "u1",
                1,
                "agente",
                EstadoFicha::Inalcanzable,
                None,
                1_000_000,
            )],
            hosts: vec![host_visto],
        };
        let filas = armar_filas(&fichas, &hosts, Some(&lista), umbral_de_frescura(1_000_000));
        assert_eq!(filas[0].estado, EstadoFicha::Inalcanzable);
        assert_eq!(filas[0].comprobada_en, Some(1_000_000));
        let avisos = avisos(&filas, Some(&lista), &hosts);
        assert!(
            avisos
                .iter()
                .any(|aviso| aviso.contains("«web-01»") && aviso.contains("conéctate a mano")),
            "{avisos:?}"
        );
    }

    #[test]
    fn el_aviso_de_un_host_no_sale_con_lo_que_el_servidor_sabia_de_antes() {
        let hosts = [host(1, "web-01")];
        let fichas = [ficha(1, "u1", 1, "agente")];
        // El servidor recuerda un error de hace un minuto; la consulta de
        // ahora no llegó a mirar el host: la fila sale `sin_comprobar`.
        let mut viejo = visto(1, 1_000_000 - 60);
        viejo.error = Some(ErrorComprobacion::SinRespuesta);
        viejo.tmux = None;
        let lista = ListaPersistentes {
            fichas: vec![vista(
                "u1",
                1,
                "agente",
                EstadoFicha::Inalcanzable,
                None,
                1_000_000 - 60,
            )],
            hosts: vec![viejo],
        };
        let filas = armar_filas(&fichas, &hosts, Some(&lista), umbral_de_frescura(1_000_000));
        assert_eq!(filas[0].estado, EstadoFicha::SinComprobar);
        let avisos = avisos(&filas, Some(&lista), &hosts);
        assert_eq!(avisos.len(), 1, "{avisos:?}");
        assert!(avisos[0].contains("sin_comprobar"), "{avisos:?}");
    }

    #[test]
    fn un_host_sin_tmux_o_con_uno_antiguo_dice_que_hacer() {
        let hosts = [host(1, "web-01"), host(2, "db-01")];
        let fichas = [ficha(1, "u1", 1, "agente"), ficha(2, "u2", 2, "build")];
        let mut sin_tmux = visto(1, 1_000_000);
        sin_tmux.tmux = None;
        let mut antiguo = visto(2, 1_000_000);
        antiguo.tmux = Some(VersionTmux::nueva(2, 9));
        let lista = ListaPersistentes {
            fichas: vec![
                vista("u1", 1, "agente", EstadoFicha::SinTmux, None, 1_000_000),
                vista("u2", 2, "build", EstadoFicha::SinTmux, None, 1_000_000),
            ],
            hosts: vec![sin_tmux, antiguo],
        };
        let filas = armar_filas(&fichas, &hosts, Some(&lista), 999_990);
        assert!(filas.iter().all(|fila| fila.estado == EstadoFicha::SinTmux));
        let avisos = avisos(&filas, Some(&lista), &hosts);
        assert!(
            avisos
                .iter()
                .any(|aviso| aviso.contains("«web-01» no tiene tmux")),
            "{avisos:?}"
        );
        assert!(
            avisos
                .iter()
                .any(|aviso| aviso.contains("«db-01» tiene un tmux demasiado antiguo")),
            "{avisos:?}"
        );
    }

    #[test]
    fn las_filas_llevan_quien_la_tiene_la_atencion_y_la_actividad_en_hora_local() {
        let hosts = [host(1, "web-01")];
        let fichas = [
            ficha(1, "u1", 1, "agente"),
            ficha(2, "u2", 1, "build"),
            ficha(3, "u3", 1, "viejo"),
        ];
        let lista = ListaPersistentes {
            fichas: vec![
                vista(
                    "u1",
                    1,
                    "agente",
                    EstadoFicha::VivaEnganchada,
                    Some(sesion("agente", 1, Some("portatil"), true)),
                    1_000_000 + 100,
                ),
                vista(
                    "u2",
                    1,
                    "build",
                    EstadoFicha::VivaLibre,
                    // Libre: el último equipo que la tuvo no es quien la tiene.
                    Some(sesion("build", 0, Some("sobremesa"), false)),
                    1_000_000 + 100,
                ),
                vista(
                    "u3",
                    1,
                    "viejo",
                    EstadoFicha::Terminada,
                    None,
                    1_000_000 + 100,
                ),
            ],
            hosts: vec![visto(1, 1_000_000 + 100)],
        };
        let filas = armar_filas(&fichas, &hosts, Some(&lista), 1_000_000);
        let agente = &filas[0];
        assert_eq!(agente.nombre, "agente");
        assert_eq!(agente.equipo.as_deref(), Some("portatil"));
        assert!(agente.atencion);
        assert_eq!(agente.titulo.as_deref(), Some("claude"));
        // Hace 120 s según el host; comprobada en 1_000_100 → 1_000_100 - 120.
        assert_eq!(agente.actividad, Some(1_000_100 - 120));
        assert_eq!(filas[1].equipo, None, "libre: nadie la tiene");
        assert!(!filas[1].atencion);
        assert_eq!(filas[2].estado, EstadoFicha::Terminada);
        assert_eq!(filas[2].actividad, None);
        assert_eq!(filas[2].titulo, None);
    }

    #[test]
    fn sin_hora_del_host_o_sin_actividad_no_se_inventa_cuando_fue() {
        let hosts = [host(1, "web-01")];
        let fichas = [ficha(1, "u1", 1, "agente")];
        let con = |ahora_del_host: i64, actividad: i64| {
            let mut sesion = sesion("agente", 0, None, false);
            sesion.actividad = actividad;
            let mut visto = visto(1, 1_000_100);
            visto.ahora = ahora_del_host;
            let lista = ListaPersistentes {
                fichas: vec![vista(
                    "u1",
                    1,
                    "agente",
                    EstadoFicha::VivaLibre,
                    Some(sesion),
                    1_000_100,
                )],
                hosts: vec![visto],
            };
            armar_filas(&fichas, &hosts, Some(&lista), 1_000_000)[0].actividad
        };
        assert_eq!(con(1_000_000, 999_880), Some(1_000_100 - 120));
        // Un host que no dio su hora (0) o una actividad ilegible (0).
        assert_eq!(con(0, 999_880), None);
        assert_eq!(con(1_000_000, 0), None);
    }

    #[test]
    fn la_tabla_enseña_las_columnas_y_el_json_es_estable() {
        let hosts = [host(1, "web-01"), host(2, "db-01")];
        let fichas = [ficha(1, "u1", 1, "agente-api"), ficha(2, "u2", 2, "backup")];
        let lista = ListaPersistentes {
            fichas: vec![
                vista(
                    "u1",
                    1,
                    "agente-api",
                    EstadoFicha::VivaEnganchada,
                    Some(sesion("agente-api", 1, Some("portatil"), true)),
                    1_000_000,
                ),
                vista("u2", 2, "backup", EstadoFicha::Terminada, None, 1_000_000),
            ],
            hosts: vec![visto(1, 1_000_000), visto(2, 1_000_000)],
        };
        let filas = armar_filas(&fichas, &hosts, Some(&lista), 999_990);
        // Ordenadas por host y nombre: db-01 primero.
        assert_eq!(filas[0].host, "db-01");
        let tabla = texto_tabla(&filas, 1_000_000);
        let lineas: Vec<&str> = tabla.lines().collect();
        assert_eq!(lineas[0], "Sesiones persistentes: 2");
        assert!(lineas[1].starts_with("ESTADO"), "{tabla}");
        assert!(lineas[1].ends_with("TÍTULO"), "{tabla}");
        assert!(lineas[2].starts_with("terminada"), "{tabla}");
        assert!(lineas[3].contains("enganchada"), "{tabla}");
        assert!(lineas[3].contains("portatil"), "{tabla}");
        assert!(lineas[3].contains("hace 2 min"), "{tabla}");
        assert!(lineas[3].contains('!'), "{tabla}");
        assert!(lineas[3].ends_with("claude"), "{tabla}");

        let json = json(&filas);
        let valor: serde_json::Value = serde_json::from_str(&json).unwrap();
        let primera = valor[0].as_object().unwrap();
        let claves: Vec<&str> = primera.keys().map(String::as_str).collect();
        // El orden de las claves es el de la estructura, no el alfabético.
        let en_texto: Vec<usize> = [
            "\"uuid\"",
            "\"host\"",
            "\"nombre\"",
            "\"estado\"",
            "\"equipo\"",
            "\"actividad\"",
            "\"atencion\"",
            "\"titulo\"",
            "\"comprobada_en\"",
        ]
        .iter()
        .map(|clave| json.find(clave).unwrap_or_else(|| panic!("falta {clave}")))
        .collect();
        assert!(en_texto.windows(2).all(|par| par[0] < par[1]), "{json}");
        assert_eq!(claves.len(), 9, "{claves:?}");
        assert_eq!(valor[0]["estado"], "terminada");
        assert_eq!(valor[0]["equipo"], serde_json::Value::Null);
        assert_eq!(valor[1]["estado"], "viva_enganchada");
        assert_eq!(valor[1]["equipo"], "portatil");
        assert_eq!(valor[1]["atencion"], true);
        assert_eq!(valor[1]["comprobada_en"], 1_000_000);
    }

    #[test]
    fn sin_fichas_el_texto_lo_dice_y_el_json_es_una_lista_vacia() {
        assert!(texto_tabla(&[], 0).contains("No hay sesiones persistentes"));
        assert_eq!(json(&[]), "[]\n");
    }

    #[test]
    fn la_edad_de_la_actividad_se_dice_en_la_unidad_que_toca() {
        assert_eq!(hace(0), "ahora");
        assert_eq!(hace(30), "hace 30 s");
        assert_eq!(hace(125), "hace 2 min");
        assert_eq!(hace(7_300), "hace 2 h");
        assert_eq!(hace(200_000), "hace 2 d");
        assert_eq!(hace(-50), "ahora", "un reloj adelantado no da negativos");
    }
}
