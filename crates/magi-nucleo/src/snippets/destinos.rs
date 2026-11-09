//! Resolución de destinos de un snippet (F6 §7.1) y cuándo exige
//! deliberación: los hosts que salen de sus etiquetas y hosts sueltos, sin
//! repetir, y los motivos por los que hay que deliberar (crítico, más de un
//! host o hosts con verificaciones activas).
//!
//! Estas funciones puras viven aquí, en `destinos`; `snippets::mod` las
//! reexporta (`pub use destinos::*;`) para que las rutas de siempre
//! (`magi_nucleo::snippets::resolver` y compañía) sigan valiendo igual en el
//! escritorio y en el móvil.

use std::collections::{BTreeSet, HashMap};

use crate::deliberacion::Verificaciones;
use crate::modelo::{Grupo, Host};

use super::Destino;

/// ¿Este host coincide con los destinos del snippet (etiqueta o host suelto)?
fn coincide_con_destinos(host: &Host, etiquetas: &BTreeSet<&str>, sueltos: &BTreeSet<i64>) -> bool {
    sueltos.contains(&host.id)
        || host
            .etiquetas
            .iter()
            .any(|etiqueta| etiquetas.contains(etiqueta.to_lowercase().as_str()))
}

/// Hosts a los que apunta un snippet: los que tienen alguna etiqueta destino
/// más los sueltos, sin duplicados y en el orden de la vista Hosts (por grupo
/// y nombre, «sin grupo» al final). Los escritorios remotos (RDP, VNC y voxl)
/// nunca son destino de un snippet: se omiten.
pub fn resolver<'a>(destinos: &[Destino], hosts: &'a [Host], grupos: &[Grupo]) -> Vec<&'a Host> {
    let etiquetas: BTreeSet<&str> = destinos
        .iter()
        .filter_map(|destino| match destino {
            Destino::Etiqueta(etiqueta) => Some(etiqueta.as_str()),
            Destino::Host { .. } => None,
        })
        .collect();
    let sueltos: BTreeSet<i64> = destinos
        .iter()
        .filter_map(|destino| match destino {
            Destino::Host { id, .. } => Some(*id),
            Destino::Etiqueta(_) => None,
        })
        .collect();
    let posicion_grupo: HashMap<i64, usize> = grupos
        .iter()
        .enumerate()
        .map(|(posicion, grupo)| (grupo.id, posicion))
        .collect();
    let mut resueltos: Vec<&Host> = hosts
        .iter()
        .filter(|host| host.tipo.es_ssh() && coincide_con_destinos(host, &etiquetas, &sueltos))
        .collect();
    resueltos.sort_by(|a, b| {
        let grupo = |host: &Host| {
            host.grupo_id
                .and_then(|id| posicion_grupo.get(&id).copied())
                .unwrap_or(usize::MAX)
        };
        grupo(a)
            .cmp(&grupo(b))
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    resueltos
}

/// Escritorios (hosts que no son SSH) a los que apuntaría un snippet: la
/// interfaz los omite y avisa de que no ejecutan snippets.
pub fn escritorios_omitidos(destinos: &[Destino], hosts: &[Host]) -> usize {
    let etiquetas: BTreeSet<&str> = destinos
        .iter()
        .filter_map(|destino| match destino {
            Destino::Etiqueta(etiqueta) => Some(etiqueta.as_str()),
            Destino::Host { .. } => None,
        })
        .collect();
    let sueltos: BTreeSet<i64> = destinos
        .iter()
        .filter_map(|destino| match destino {
            Destino::Host { id, .. } => Some(*id),
            Destino::Etiqueta(_) => None,
        })
        .collect();
    hosts
        .iter()
        .filter(|host| !host.tipo.es_ssh() && coincide_con_destinos(host, &etiquetas, &sueltos))
        .count()
}

/// ¿Apunta el snippet a este host?
pub fn apunta_a(destinos: &[Destino], hosts: &[Host], grupos: &[Grupo], host_id: i64) -> bool {
    resolver(destinos, hosts, grupos)
        .iter()
        .any(|host| host.id == host_id)
}

/// Destino resumido para la lista: «todos» si resuelve a toda la flota, la
/// etiqueta (o el host) si solo hay uno y «web +2» si hay más.
pub fn resumen_destino(destinos: &[Destino], resueltos: usize, total_hosts: usize) -> String {
    if resueltos > 0 && resueltos == total_hosts && total_hosts > 1 {
        return "todos".to_string();
    }
    let mut nombres = destinos.iter().map(|destino| match destino {
        Destino::Etiqueta(etiqueta) => etiqueta.clone(),
        Destino::Host { nombre, .. } => nombre.clone(),
    });
    match (nombres.next(), destinos.len()) {
        (None, _) => "—".to_string(),
        (Some(primero), 1) => primero,
        (Some(primero), total) => format!("{primero} +{}", total - 1),
    }
}

/// Por qué una ejecución pasa por la deliberación MAGI (§7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotivoDeliberacion {
    Critico,
    VariosHosts(usize),
    /// Hosts con alguna verificación previa activa.
    Verificaciones(Vec<String>),
    /// Sincronización con «borrar» marcado (Fase 8): lo que borrará en el
    /// destino.
    Borrar(usize),
}

impl MotivoDeliberacion {
    pub fn texto(&self) -> String {
        match self {
            MotivoDeliberacion::Critico => "crítico".to_string(),
            MotivoDeliberacion::VariosHosts(cuantos) => format!("{cuantos} hosts"),
            MotivoDeliberacion::Verificaciones(hosts) => {
                format!("verificaciones en {}", hosts.join(", "))
            }
            MotivoDeliberacion::Borrar(cuantos) => format!("borrar en destino ({cuantos})"),
        }
    }
}

/// Motivos para deliberar: crítico, más de un host o algún host con
/// verificaciones activas. Vacío = se lanza sin deliberación.
pub fn motivos_deliberacion(
    critico: bool,
    hosts: &[(i64, String)],
    verificaciones: &HashMap<i64, Verificaciones>,
) -> Vec<MotivoDeliberacion> {
    let mut motivos = Vec::new();
    if critico {
        motivos.push(MotivoDeliberacion::Critico);
    }
    if hosts.len() > 1 {
        motivos.push(MotivoDeliberacion::VariosHosts(hosts.len()));
    }
    let con_verificaciones: Vec<String> = hosts
        .iter()
        .filter(|(id, _)| {
            verificaciones
                .get(id)
                .is_some_and(Verificaciones::alguna_activa)
        })
        .map(|(_, nombre)| nombre.clone())
        .collect();
    if !con_verificaciones.is_empty() {
        motivos.push(MotivoDeliberacion::Verificaciones(con_verificaciones));
    }
    motivos
}

#[cfg(test)]
mod pruebas {
    use super::*;

    use crate::snippets::{normalizar, DatosSnippet};

    fn host(id: i64, nombre: &str, grupo_id: Option<i64>, etiquetas: &[&str]) -> Host {
        let mut host = crate::modelo::host_de_prueba();
        host.id = id;
        host.nombre = nombre.to_string();
        host.grupo_id = grupo_id;
        host.etiquetas = etiquetas.iter().map(|e| e.to_string()).collect();
        host
    }

    fn grupo(id: i64, nombre: &str) -> Grupo {
        Grupo {
            id,
            nombre: nombre.to_string(),
            orden: id,
            plegado: false,
            creado_en: String::new(),
        }
    }

    fn nombres(hosts: Vec<&Host>) -> Vec<String> {
        hosts.iter().map(|host| host.nombre.clone()).collect()
    }

    /// AC (checklist l. 30): etiqueta «web» + hetzner-01 suelto que también es
    /// «web» → hetzner-01 una sola vez.
    #[test]
    fn un_host_por_etiqueta_y_suelto_sale_una_vez() {
        let hosts = vec![
            host(1, "hetzner-01", None, &["web"]),
            host(2, "hetzner-02", None, &["web"]),
            host(3, "db", None, &["db"]),
        ];
        let destinos = vec![
            Destino::Etiqueta("web".to_string()),
            Destino::Host {
                id: 1,
                nombre: "hetzner-01".to_string(),
            },
        ];
        assert_eq!(
            nombres(resolver(&destinos, &hosts, &[])),
            vec!["hetzner-01", "hetzner-02"]
        );
    }

    #[test]
    fn el_orden_es_el_de_la_vista_hosts() {
        let grupos = vec![grupo(10, "producción"), grupo(20, "pruebas")];
        let hosts = vec![
            host(1, "a-suelto", None, &["x"]),
            host(2, "b-pruebas", Some(20), &["x"]),
            host(3, "c-prod", Some(10), &["x"]),
            host(4, "a-prod", Some(10), &["x"]),
        ];
        let destinos = vec![Destino::Etiqueta("x".to_string())];
        assert_eq!(
            nombres(resolver(&destinos, &hosts, &grupos)),
            vec!["a-prod", "c-prod", "b-pruebas", "a-suelto"]
        );
    }

    #[test]
    fn una_etiqueta_sin_hosts_resuelve_a_cero() {
        let hosts = vec![host(1, "uno", None, &["web"])];
        let destinos = vec![Destino::Etiqueta("nadie".to_string())];
        assert!(resolver(&destinos, &hosts, &[]).is_empty());
        let fantasma = vec![Destino::Host {
            id: 99,
            nombre: "borrado".to_string(),
        }];
        assert!(resolver(&fantasma, &hosts, &[]).is_empty());
    }

    #[test]
    fn las_etiquetas_se_comparan_sin_mayusculas() {
        let hosts = vec![host(1, "uno", None, &["Web"])];
        let datos = normalizar(DatosSnippet {
            destinos: vec![Destino::Etiqueta(" WEB ".to_string())],
            ..Default::default()
        });
        assert_eq!(resolver(&datos.destinos, &hosts, &[]).len(), 1);
    }

    #[test]
    fn apunta_a_solo_los_hosts_ssh_resueltos() {
        let hosts = vec![
            host(1, "uno", None, &["web"]),
            host(2, "dos", None, &["db"]),
        ];
        let destinos = vec![Destino::Etiqueta("web".to_string())];
        assert!(apunta_a(&destinos, &hosts, &[], 1));
        assert!(!apunta_a(&destinos, &hosts, &[], 2));
    }

    #[test]
    fn los_escritorios_remotos_se_cuentan_como_omitidos() {
        let mut escritorio = host(2, "rdp-01", None, &["web"]);
        escritorio.tipo = crate::escritorio::TipoHost::Rdp;
        let hosts = vec![host(1, "web-01", None, &["web"]), escritorio];
        let destinos = vec![Destino::Etiqueta("web".to_string())];
        assert_eq!(nombres(resolver(&destinos, &hosts, &[])), vec!["web-01"]);
        assert_eq!(escritorios_omitidos(&destinos, &hosts), 1);
    }

    #[test]
    fn cuando_se_delibera() {
        let mut verificaciones = HashMap::new();
        verificaciones.insert(
            2,
            Verificaciones {
                host_id: 2,
                salud: true,
                ..Default::default()
            },
        );
        let uno = vec![(1, "uno".to_string())];
        assert!(motivos_deliberacion(false, &uno, &verificaciones).is_empty());
        assert_eq!(
            motivos_deliberacion(true, &uno, &verificaciones),
            vec![MotivoDeliberacion::Critico]
        );
        let dos = vec![(1, "uno".to_string()), (2, "dos".to_string())];
        assert_eq!(
            motivos_deliberacion(false, &dos, &verificaciones),
            vec![
                MotivoDeliberacion::VariosHosts(2),
                MotivoDeliberacion::Verificaciones(vec!["dos".to_string()])
            ]
        );
        let solo_dos = vec![(2, "dos".to_string())];
        assert_eq!(
            motivos_deliberacion(false, &solo_dos, &verificaciones).len(),
            1
        );
    }

    #[test]
    fn el_resumen_del_destino() {
        let web = vec![Destino::Etiqueta("web".to_string())];
        assert_eq!(resumen_destino(&web, 3, 12), "web");
        assert_eq!(resumen_destino(&web, 12, 12), "todos");
        let varios = vec![
            Destino::Etiqueta("web".to_string()),
            Destino::Host {
                id: 1,
                nombre: "db".to_string(),
            },
        ];
        assert_eq!(resumen_destino(&varios, 4, 12), "web +1");
    }
}
