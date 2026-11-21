//! Las filas del bloque «Persistentes» (Fase 16, §6.1 del informe): se derivan
//! de la última lista que difundió el servidor (`ListaPersistentes`), nunca se
//! guardan y no inventan estados: todo sale del estado derivado del núcleo
//! (`sesiones_persistentes::estado`) que ya trae cada `FichaVista`.
//!
//! Primero van las fichas de todos los hosts (por host y nombre) y, detrás, las
//! sesiones sin ficha del tmux de MAGI («sin ficha») y las del tmux habitual
//! del usuario («de tu tmux») de cada host comprobado.

use crate::protocolo::{
    ErrorComprobacion, EstadoFicha, HostVisto, ListaPersistentes, ServidorTmux, SesionVista,
    VersionTmux,
};
use magi_nucleo::sesiones_persistentes::estado::antigua;

/// Qué es una fila del bloque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoFila {
    /// Una ficha (`SESIONES_PERSISTENTES`).
    Ficha,
    /// Una sesión del tmux de MAGI que no tiene ficha («sin ficha»).
    SinFicha,
    /// Una sesión del tmux habitual del usuario («de tu tmux»).
    Habitual,
}

/// Identidad de una fila: la selección la sigue aunque la lista se refresque y
/// las filas cambien de sitio.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClaveFila {
    pub tipo: TipoFila,
    pub host_id: i64,
    pub nombre: String,
}

/// Una sesión que el usuario acaba de terminar: hasta que el host se vuelva a
/// comprobar, su fila se ve terminada (el host dijo `ok`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminadaLocal {
    pub host_id: i64,
    pub servidor: ServidorTmux,
    pub nombre: String,
    /// Segundos Unix del momento en que el host respondió.
    pub epoca: i64,
}

/// Lo que hace falta para actuar sobre una fila y que un diálogo la lleve
/// fijada: no se vuelve a leer al confirmar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjetivoSesion {
    pub tipo: TipoFila,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    /// `uuid` de la ficha; `None` en las sesiones sin ficha y en las del tmux
    /// habitual.
    pub uuid: Option<String>,
    pub estado: EstadoFicha,
    /// Clientes enganchados cuando se miró.
    pub clientes: u32,
    /// Equipo que la tenía cuando se miró.
    pub equipo: Option<String>,
}

impl ObjetivoSesion {
    /// El servidor de tmux al que pertenece la sesión.
    #[must_use]
    pub fn servidor(&self) -> ServidorTmux {
        match self.tipo {
            TipoFila::Habitual => ServidorTmux::Habitual,
            TipoFila::Ficha | TipoFila::SinFicha => ServidorTmux::Magi,
        }
    }
}

/// Una fila del bloque, con todo lo que hace falta para pintarla y actuar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilaPersistente {
    pub tipo: TipoFila,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    pub uuid: Option<String>,
    /// El de la ficha; en las demás, `VivaEnganchada` o `VivaLibre` según los
    /// clientes enganchados.
    pub estado: EstadoFicha,
    pub sesion: Option<SesionVista>,
    /// Cuándo se comprobó su host (segundos Unix, hora de este equipo; 0 si
    /// nunca).
    pub comprobada_en: i64,
    /// Hora del host al comprobar, para la actividad relativa.
    pub ahora_host: i64,
    pub error: Option<ErrorComprobacion>,
    pub tmux: Option<VersionTmux>,
    /// Más de `antigua_dias` sin actividad.
    pub antigua: bool,
}

impl FilaPersistente {
    #[must_use]
    pub fn clave(&self) -> ClaveFila {
        ClaveFila {
            tipo: self.tipo,
            host_id: self.host_id,
            nombre: self.nombre.clone(),
        }
    }

    /// Los datos fijos con los que actúan los diálogos.
    #[must_use]
    pub fn objetivo(&self) -> ObjetivoSesion {
        ObjetivoSesion {
            tipo: self.tipo,
            host_id: self.host_id,
            host_nombre: self.host_nombre.clone(),
            nombre: self.nombre.clone(),
            uuid: self.uuid.clone(),
            estado: self.estado,
            clientes: self.sesion.as_ref().map_or(0, |sesion| sesion.clientes),
            equipo: self
                .sesion
                .as_ref()
                .and_then(|sesion| sesion.equipo.clone()),
        }
    }

    /// La sesión pide atención (la campana de tmux, D335). Las del tmux
    /// habitual no cuentan: MAGI no las vigila.
    #[must_use]
    pub fn atencion(&self) -> bool {
        self.tipo != TipoFila::Habitual && self.sesion.as_ref().is_some_and(|sesion| sesion.campana)
    }

    /// La sesión existe ahora mismo en el host.
    #[must_use]
    pub fn viva(&self) -> bool {
        matches!(
            self.estado,
            EstadoFicha::VivaLibre | EstadoFicha::VivaEnganchada
        )
    }

    /// Alguien la tiene enganchada.
    #[must_use]
    pub fn enganchada(&self) -> bool {
        self.sesion
            .as_ref()
            .is_some_and(|sesion| sesion.clientes > 0)
    }

    /// Segundos desde la última actividad, con la hora del host y no la de
    /// este equipo (los relojes no tienen por qué coincidir).
    #[must_use]
    pub fn actividad_hace(&self) -> Option<i64> {
        self.sesion
            .as_ref()
            .map(|sesion| self.ahora_host.saturating_sub(sesion.actividad).max(0))
    }

    /// El título del programa que corre dentro, si lo hay. Un título igual al
    /// nombre del host (el que tmux pone por defecto) no cuenta nada: no se
    /// enseña (D370).
    #[must_use]
    pub fn titulo(&self) -> Option<&str> {
        self.sesion
            .as_ref()
            .and_then(|sesion| sesion.titulo.as_deref())
            .filter(|titulo| {
                !titulo.trim().is_empty()
                    && !titulo.trim().eq_ignore_ascii_case(self.host_nombre.trim())
            })
    }

    /// Por qué no se pudo mirar el host de la fila (solo en las fichas cuyo
    /// estado no se pudo derivar). Nunca se dice «terminada».
    #[must_use]
    pub fn motivo(&self) -> Option<String> {
        match self.estado {
            EstadoFicha::SinComprobar => Some("sin comprobar todavía".to_string()),
            EstadoFicha::Inalcanzable => Some(
                match self.error {
                    Some(ErrorComprobacion::NecesitaDialogo) => "sin comprobar: conéctate a mano",
                    Some(ErrorComprobacion::SinRespuesta) => "sin comprobar: el host no responde",
                    Some(ErrorComprobacion::Plazo) => "sin comprobar: el host no respondió en 10 s",
                    None => "sin comprobar",
                }
                .to_string(),
            ),
            EstadoFicha::SinTmux => Some(match self.tmux {
                Some(version) => format!(
                    "tmux {}.{} es anterior a 3.0: actualízalo en el host",
                    version.mayor, version.menor
                ),
                None => "el host no tiene tmux: instálalo (3.0 o posterior)".to_string(),
            }),
            _ => None,
        }
    }
}

/// Texto de «hace N» para una duración en segundos: `hace 20 s`, `hace 4 m`,
/// `hace 3 h`, `hace 9 d`.
#[must_use]
pub fn hace(segundos: i64) -> String {
    let segundos = segundos.max(0);
    if segundos < 60 {
        format!("hace {segundos} s")
    } else if segundos < 3600 {
        format!("hace {} m", segundos / 60)
    } else if segundos < 86_400 {
        format!("hace {} h", segundos / 3600)
    } else {
        format!("hace {} d", segundos / 86_400)
    }
}

/// Cómo se llama, en la columna ESTADO, quien tiene la sesión: «aquí» si es
/// este equipo, su etiqueta si es otro y «?» si no consta.
#[must_use]
pub fn quien_la_tiene(equipo: Option<&str>, este_equipo: &str) -> String {
    match equipo {
        Some(equipo) if equipo == este_equipo => "aquí".to_string(),
        Some(equipo) => equipo.to_string(),
        None => "?".to_string(),
    }
}

/// Las filas del bloque a partir de la lista del servidor.
///
/// - `nombre_host`: cómo se llama cada host (la lista solo trae ids).
/// - `antigua_dias`: `[persistentes] antigua_dias` ya acotado.
/// - `terminadas`: sesiones que el usuario acaba de terminar y que el servidor
///   aún no ha vuelto a mirar.
/// - `filtro`: texto de `/`; vacío, todas.
#[must_use]
pub fn construir_filas(
    lista: &ListaPersistentes,
    nombre_host: &dyn Fn(i64) -> String,
    antigua_dias: u64,
    terminadas: &[TerminadaLocal],
    filtro: &str,
) -> Vec<FilaPersistente> {
    let host_visto = |host_id: i64| -> Option<&HostVisto> {
        lista.hosts.iter().find(|host| host.host_id == host_id)
    };
    // Una terminación local solo vale mientras el host no se haya vuelto a
    // mirar después de ella.
    let terminada_local = |host_id: i64, servidor: ServidorTmux, nombre: &str, mirada: i64| {
        terminadas.iter().any(|terminada| {
            terminada.host_id == host_id
                && terminada.servidor == servidor
                && terminada.nombre == nombre
                && mirada <= terminada.epoca
        })
    };

    let mut fichas: Vec<FilaPersistente> = lista
        .fichas
        .iter()
        .map(|ficha| {
            let visto = host_visto(ficha.host_id);
            let (estado, sesion) = if ficha.sesion.is_some()
                && terminada_local(
                    ficha.host_id,
                    ServidorTmux::Magi,
                    &ficha.nombre,
                    ficha.comprobada_en,
                ) {
                (EstadoFicha::Terminada, None)
            } else {
                (ficha.estado, ficha.sesion.clone())
            };
            let ahora_host = visto.map_or(0, |host| host.ahora);
            FilaPersistente {
                tipo: TipoFila::Ficha,
                host_id: ficha.host_id,
                host_nombre: nombre_host(ficha.host_id),
                nombre: ficha.nombre.clone(),
                uuid: Some(ficha.uuid.clone()),
                estado,
                antigua: antigua(sesion.as_ref(), ahora_host, antigua_dias),
                sesion,
                comprobada_en: ficha.comprobada_en,
                ahora_host,
                error: visto.and_then(|host| host.error),
                tmux: visto.and_then(|host| host.tmux),
            }
        })
        .collect();
    fichas.sort_by(|a, b| {
        (a.host_nombre.to_lowercase(), a.nombre.as_str())
            .cmp(&(b.host_nombre.to_lowercase(), b.nombre.as_str()))
    });

    let mut hosts: Vec<&HostVisto> = lista.hosts.iter().collect();
    hosts.sort_by_key(|host| (nombre_host(host.host_id).to_lowercase(), host.host_id));
    let mut extras = Vec::new();
    for host in hosts {
        // Un host que no se pudo mirar no trae sesiones: nada que enseñar.
        if host.error.is_some() {
            continue;
        }
        for (tipo, servidor, sesiones) in [
            (TipoFila::SinFicha, ServidorTmux::Magi, &host.sin_ficha),
            (TipoFila::Habitual, ServidorTmux::Habitual, &host.habitual),
        ] {
            let mut sesiones: Vec<&SesionVista> = sesiones.iter().collect();
            sesiones.sort_by(|a, b| a.nombre.cmp(&b.nombre));
            for sesion in sesiones {
                if terminada_local(host.host_id, servidor, &sesion.nombre, host.comprobada_en) {
                    continue;
                }
                extras.push(FilaPersistente {
                    tipo,
                    host_id: host.host_id,
                    host_nombre: nombre_host(host.host_id),
                    nombre: sesion.nombre.clone(),
                    uuid: None,
                    estado: if sesion.clientes > 0 {
                        EstadoFicha::VivaEnganchada
                    } else {
                        EstadoFicha::VivaLibre
                    },
                    antigua: antigua(Some(sesion), host.ahora, antigua_dias),
                    sesion: Some(sesion.clone()),
                    comprobada_en: host.comprobada_en,
                    ahora_host: host.ahora,
                    error: None,
                    tmux: host.tmux,
                });
            }
        }
    }

    let mut filas = fichas;
    filas.extend(extras);
    let filtro = filtro.trim().to_lowercase();
    if !filtro.is_empty() {
        filas.retain(|fila| {
            let pajar = format!(
                "{} {} {}",
                fila.nombre,
                fila.host_nombre,
                fila.titulo().unwrap_or("")
            )
            .to_lowercase();
            pajar.contains(&filtro)
        });
    }
    filas
}

/// Cuántas filas son fichas y cuántas no tienen ficha (las del tmux de MAGI y
/// las del habitual), para la cabecera «PERSISTENTES · N · M sin ficha».
#[must_use]
pub fn contar(filas: &[FilaPersistente]) -> (usize, usize) {
    let fichas = filas
        .iter()
        .filter(|fila| fila.tipo == TipoFila::Ficha)
        .count();
    (fichas, filas.len() - fichas)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::protocolo::FichaVista;

    fn sesion(servidor: ServidorTmux, nombre: &str, clientes: u32, actividad: i64) -> SesionVista {
        SesionVista {
            servidor,
            nombre: nombre.to_string(),
            id: None,
            clientes,
            equipo: None,
            actividad,
            creada: 0,
            campana: false,
            ventanas: 1,
            titulo: None,
        }
    }

    fn nombres(host_id: i64) -> String {
        match host_id {
            1 => "web-01".to_string(),
            2 => "dev-01".to_string(),
            otro => format!("host {otro}"),
        }
    }

    #[test]
    fn hace_formatea_segundos_minutos_horas_y_dias() {
        assert_eq!(hace(0), "hace 0 s");
        assert_eq!(hace(59), "hace 59 s");
        assert_eq!(hace(60), "hace 1 m");
        assert_eq!(hace(3599), "hace 59 m");
        assert_eq!(hace(3600), "hace 1 h");
        assert_eq!(hace(86_399), "hace 23 h");
        assert_eq!(hace(86_400 * 9), "hace 9 d");
        assert_eq!(hace(-5), "hace 0 s");
    }

    #[test]
    fn las_fichas_van_primero_y_las_sesiones_sin_ficha_detras() {
        let lista = ListaPersistentes {
            fichas: vec![
                FichaVista {
                    uuid: "u2".to_string(),
                    host_id: 2,
                    nombre: "refactor".to_string(),
                    estado: EstadoFicha::VivaLibre,
                    sesion: Some(sesion(ServidorTmux::Magi, "refactor", 0, 100)),
                    comprobada_en: 1000,
                },
                FichaVista {
                    uuid: "u1".to_string(),
                    host_id: 1,
                    nombre: "agente-api".to_string(),
                    estado: EstadoFicha::Terminada,
                    sesion: None,
                    comprobada_en: 1000,
                },
            ],
            hosts: vec![HostVisto {
                host_id: 1,
                comprobada_en: 1000,
                ahora: 500,
                tmux: Some(VersionTmux::nueva(3, 5)),
                error: None,
                sin_ficha: vec![sesion(ServidorTmux::Magi, "portatil-3", 0, 100)],
                habitual: vec![sesion(ServidorTmux::Habitual, "trabajo", 1, 100)],
            }],
        };
        let filas = construir_filas(&lista, &nombres, 7, &[], "");
        let vistas: Vec<(TipoFila, &str)> = filas
            .iter()
            .map(|fila| (fila.tipo, fila.nombre.as_str()))
            .collect();
        // dev-01 va antes que web-01.
        assert_eq!(
            vistas,
            vec![
                (TipoFila::Ficha, "refactor"),
                (TipoFila::Ficha, "agente-api"),
                (TipoFila::SinFicha, "portatil-3"),
                (TipoFila::Habitual, "trabajo"),
            ]
        );
        assert_eq!(contar(&filas), (2, 2));
    }

    #[test]
    fn una_terminacion_local_vale_hasta_que_se_vuelve_a_mirar_el_host() {
        let lista = ListaPersistentes {
            fichas: vec![FichaVista {
                uuid: "u1".to_string(),
                host_id: 1,
                nombre: "api".to_string(),
                estado: EstadoFicha::VivaLibre,
                sesion: Some(sesion(ServidorTmux::Magi, "api", 0, 100)),
                comprobada_en: 1000,
            }],
            hosts: vec![HostVisto {
                host_id: 1,
                comprobada_en: 1000,
                ahora: 500,
                tmux: Some(VersionTmux::nueva(3, 5)),
                error: None,
                sin_ficha: vec![sesion(ServidorTmux::Magi, "otra", 0, 100)],
                habitual: Vec::new(),
            }],
        };
        let terminadas = vec![
            TerminadaLocal {
                host_id: 1,
                servidor: ServidorTmux::Magi,
                nombre: "api".to_string(),
                epoca: 1005,
            },
            TerminadaLocal {
                host_id: 1,
                servidor: ServidorTmux::Magi,
                nombre: "otra".to_string(),
                epoca: 1005,
            },
        ];
        let filas = construir_filas(&lista, &nombres, 7, &terminadas, "");
        assert_eq!(filas.len(), 1, "la sin ficha terminada se oculta");
        assert_eq!(filas[0].estado, EstadoFicha::Terminada);
        assert!(filas[0].sesion.is_none());

        // El host se volvió a mirar después: manda lo que dice el host.
        let mut lista = lista;
        lista.fichas[0].comprobada_en = 1006;
        lista.hosts[0].comprobada_en = 1006;
        let filas = construir_filas(&lista, &nombres, 7, &terminadas, "");
        assert_eq!(filas.len(), 2);
        assert_eq!(filas[0].estado, EstadoFicha::VivaLibre);
    }

    #[test]
    fn el_filtro_mira_nombre_host_y_titulo() {
        let mut con_titulo = sesion(ServidorTmux::Magi, "refactor", 0, 100);
        con_titulo.titulo = Some("Claude Code".to_string());
        let lista = ListaPersistentes {
            fichas: vec![
                FichaVista {
                    uuid: "u1".to_string(),
                    host_id: 1,
                    nombre: "api".to_string(),
                    estado: EstadoFicha::SinComprobar,
                    sesion: None,
                    comprobada_en: 0,
                },
                FichaVista {
                    uuid: "u2".to_string(),
                    host_id: 2,
                    nombre: "refactor".to_string(),
                    estado: EstadoFicha::VivaLibre,
                    sesion: Some(con_titulo),
                    comprobada_en: 1,
                },
            ],
            hosts: Vec::new(),
        };
        assert_eq!(construir_filas(&lista, &nombres, 7, &[], "CLAUDE").len(), 1);
        assert_eq!(construir_filas(&lista, &nombres, 7, &[], "web").len(), 1);
        assert_eq!(construir_filas(&lista, &nombres, 7, &[], "api").len(), 1);
        assert_eq!(construir_filas(&lista, &nombres, 7, &[], "nada").len(), 0);
    }

    #[test]
    fn un_titulo_igual_al_nombre_del_host_no_se_enseña_ni_cuenta_para_el_filtro() {
        // tmux pone por defecto el nombre de la máquina como título: no cuenta
        // nada del programa de dentro (D370).
        let titulo = |texto: &str| {
            let mut con_titulo = sesion(ServidorTmux::Magi, "refactor", 0, 100);
            con_titulo.titulo = Some(texto.to_string());
            let lista = ListaPersistentes {
                fichas: vec![FichaVista {
                    uuid: "u2".to_string(),
                    host_id: 2,
                    nombre: "refactor".to_string(),
                    estado: EstadoFicha::VivaLibre,
                    sesion: Some(con_titulo),
                    comprobada_en: 1,
                }],
                hosts: Vec::new(),
            };
            construir_filas(&lista, &nombres, 7, &[], "")
                .remove(0)
                .titulo()
                .map(str::to_string)
        };
        assert_eq!(titulo("dev-01"), None);
        assert_eq!(
            titulo("  DEV-01 "),
            None,
            "sin distinguir mayúsculas ni bordes"
        );
        assert_eq!(titulo("Claude Code"), Some("Claude Code".to_string()));
        assert_eq!(
            titulo("dev-01: ~/proyectos"),
            Some("dev-01: ~/proyectos".to_string())
        );
        assert_eq!(titulo("   "), None);
    }

    #[test]
    fn un_host_sin_comprobar_nunca_da_la_ficha_por_terminada() {
        let lista = ListaPersistentes {
            fichas: vec![FichaVista {
                uuid: "u1".to_string(),
                host_id: 1,
                nombre: "api".to_string(),
                estado: EstadoFicha::Inalcanzable,
                sesion: None,
                comprobada_en: 1000,
            }],
            hosts: vec![HostVisto {
                host_id: 1,
                comprobada_en: 1000,
                ahora: 0,
                tmux: None,
                error: Some(ErrorComprobacion::NecesitaDialogo),
                sin_ficha: Vec::new(),
                habitual: Vec::new(),
            }],
        };
        let filas = construir_filas(&lista, &nombres, 7, &[], "");
        assert_eq!(filas.len(), 1);
        assert_eq!(
            filas[0].motivo().as_deref(),
            Some("sin comprobar: conéctate a mano")
        );
    }
}
