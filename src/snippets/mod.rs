//! Snippets (Fase 6): comandos guardados con variables, dirigidos a etiquetas
//! de host (por nombre) y a hosts sueltos. Aquí vive el modelo y la lógica
//! pura: validación, resolución de destinos y cuándo hace falta deliberar.
//! El CRUD está en `almacen::snippets`; la ejecución, en el servidor.

pub mod salida;
pub mod variables;

use std::collections::{BTreeSet, HashMap};

use crate::deliberacion::Verificaciones;
use crate::modelo::{Grupo, Host};

/// Timeout de un snippet, en segundos.
pub const TIMEOUT_MIN: u32 = 5;
pub const TIMEOUT_MAX: u32 = 3600;
pub const TIMEOUT_DEFECTO: u32 = 60;

/// Un destino del snippet: una etiqueta de host (por nombre: una etiqueta sin
/// hosts resuelve a cero hosts, no rompe el snippet) o un host suelto.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Destino {
    Etiqueta(String),
    Host { id: i64, nombre: String },
}

/// Un snippet tal como está en `SNIPPETS` y `SNIPPET_DESTINOS`.
#[derive(Debug, Clone, PartialEq)]
pub struct Snippet {
    pub id: i64,
    pub nombre: String,
    pub comando: String,
    pub descripcion: String,
    /// Clasificación propia del snippet (no son etiquetas de host).
    pub etiquetas: Vec<String>,
    pub critico: bool,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub usado_veces: i64,
    pub ultimo_uso_en: Option<String>,
    pub creado_en: String,
    pub actualizado_en: String,
    pub destinos: Vec<Destino>,
}

impl Snippet {
    /// Los datos editables del snippet.
    pub fn datos(&self) -> DatosSnippet {
        DatosSnippet {
            nombre: self.nombre.clone(),
            comando: self.comando.clone(),
            descripcion: self.descripcion.clone(),
            etiquetas: self.etiquetas.clone(),
            critico: self.critico,
            timeout_seg: self.timeout_seg,
            parar_al_fallo: self.parar_al_fallo,
            destinos: self.destinos.clone(),
        }
    }

    /// Variables del comando, en orden de aparición.
    pub fn variables(&self) -> Vec<variables::Variable> {
        variables::detectar(&self.comando)
    }

    /// ¿Puede ser «snippet al conectar» de un host? Solo si no es crítico ni
    /// tiene variables (se escribe en la pestaña sin preguntar nada).
    pub fn apto_al_conectar(&self) -> bool {
        !self.critico && self.variables().is_empty()
    }
}

/// Lo que se edita de un snippet (sin id, uso ni marcas de tiempo).
#[derive(Debug, Clone, PartialEq)]
pub struct DatosSnippet {
    pub nombre: String,
    pub comando: String,
    pub descripcion: String,
    pub etiquetas: Vec<String>,
    pub critico: bool,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub destinos: Vec<Destino>,
}

impl Default for DatosSnippet {
    fn default() -> Self {
        Self {
            nombre: String::new(),
            comando: String::new(),
            descripcion: String::new(),
            etiquetas: Vec::new(),
            critico: false,
            timeout_seg: TIMEOUT_DEFECTO,
            parar_al_fallo: false,
            destinos: Vec::new(),
        }
    }
}

/// Normaliza lo que escribe el usuario: nombre y descripción recortados,
/// etiquetas y destinos por etiqueta en minúsculas y sin duplicados. El
/// comando se guarda tal cual, salvo los saltos finales.
pub fn normalizar(mut datos: DatosSnippet) -> DatosSnippet {
    datos.nombre = datos.nombre.trim().to_string();
    datos.descripcion = datos.descripcion.trim().to_string();
    datos.comando = datos.comando.trim_end_matches(['\n', '\r']).to_string();
    let etiquetas: BTreeSet<String> = datos
        .etiquetas
        .iter()
        .flat_map(|etiqueta| etiqueta.split_whitespace())
        .map(|etiqueta| etiqueta.to_lowercase())
        .collect();
    datos.etiquetas = etiquetas.into_iter().collect();
    let mut destinos: Vec<Destino> = Vec::new();
    for destino in datos.destinos {
        let destino = match destino {
            Destino::Etiqueta(etiqueta) => {
                let etiqueta = etiqueta.trim().to_lowercase();
                if etiqueta.is_empty() {
                    continue;
                }
                Destino::Etiqueta(etiqueta)
            }
            host => host,
        };
        let repetido = destinos.iter().any(|visto| match (visto, &destino) {
            (Destino::Etiqueta(a), Destino::Etiqueta(b)) => a == b,
            (Destino::Host { id: a, .. }, Destino::Host { id: b, .. }) => a == b,
            _ => false,
        });
        if !repetido {
            destinos.push(destino);
        }
    }
    datos.destinos = destinos;
    datos
}

/// Valida un snippet ya normalizado: nombre no vacío, comando no vacío,
/// timeout 5-3600, al menos un destino y variables fuera de comillas.
pub fn validar(datos: &DatosSnippet) -> Result<(), String> {
    if datos.nombre.is_empty() {
        return Err("el snippet necesita un nombre".to_string());
    }
    if datos.nombre.chars().count() > 80 {
        return Err("el nombre no puede pasar de 80 caracteres".to_string());
    }
    if datos.nombre.chars().any(char::is_control) {
        return Err("el nombre no puede llevar caracteres de control".to_string());
    }
    if datos.comando.trim().is_empty() {
        return Err("el comando no puede estar vacío".to_string());
    }
    if datos.comando.contains('\0') {
        return Err("el comando no puede llevar el carácter nulo".to_string());
    }
    if !(TIMEOUT_MIN..=TIMEOUT_MAX).contains(&datos.timeout_seg) {
        return Err(format!(
            "el timeout debe estar entre {TIMEOUT_MIN} y {TIMEOUT_MAX} segundos"
        ));
    }
    if datos.destinos.is_empty() {
        return Err("el snippet necesita al menos un destino (etiqueta o host)".to_string());
    }
    variables::validar_contexto(&datos.comando)?;
    Ok(())
}

/// Hosts a los que apunta un snippet: los que tienen alguna etiqueta destino
/// más los sueltos, sin duplicados y en el orden de la vista Hosts (por grupo
/// y nombre, «sin grupo» al final).
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
        .filter(|host| {
            sueltos.contains(&host.id)
                || host
                    .etiquetas
                    .iter()
                    .any(|etiqueta| etiquetas.contains(etiqueta.to_lowercase().as_str()))
        })
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

/// ¿Casa el snippet con la consulta del filtro `/`? Por nombre, comando,
/// etiquetas del snippet o etiqueta destino; varias palabras, todas.
pub fn coincide(snippet: &Snippet, consulta: &str) -> bool {
    let consulta = crate::modelo::normalizar_busqueda(consulta);
    let mut campos = vec![
        crate::modelo::normalizar_busqueda(&snippet.nombre),
        crate::modelo::normalizar_busqueda(&snippet.comando),
        crate::modelo::normalizar_busqueda(&snippet.descripcion),
    ];
    campos.extend(
        snippet
            .etiquetas
            .iter()
            .map(|etiqueta| crate::modelo::normalizar_busqueda(etiqueta)),
    );
    campos.extend(snippet.destinos.iter().filter_map(|destino| match destino {
        Destino::Etiqueta(etiqueta) => Some(crate::modelo::normalizar_busqueda(etiqueta)),
        Destino::Host { .. } => None,
    }));
    consulta
        .split_whitespace()
        .all(|palabra| campos.iter().any(|campo| campo.contains(palabra)))
}

/// Por qué una ejecución pasa por la deliberación MAGI (§7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotivoDeliberacion {
    Critico,
    VariosHosts(usize),
    /// Hosts con alguna verificación previa activa.
    Verificaciones(Vec<String>),
}

impl MotivoDeliberacion {
    pub fn texto(&self) -> String {
        match self {
            MotivoDeliberacion::Critico => "crítico".to_string(),
            MotivoDeliberacion::VariosHosts(cuantos) => format!("{cuantos} hosts"),
            MotivoDeliberacion::Verificaciones(hosts) => {
                format!("verificaciones en {}", hosts.join(", "))
            }
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

/// Texto que se escribe en la pestaña como comando inicial: sin saltos
/// finales y con uno solo al final (lo que haría el usuario con `↵`).
pub fn texto_comando_inicial(comando: &str) -> String {
    format!("{}\n", comando.trim_end_matches(['\n', '\r']))
}

/// Listado de `magi snippets`: nombre, destinos resueltos, crítico y último
/// uso.
pub fn listado_cli(snippets: &[Snippet], hosts: &[Host], grupos: &[Grupo]) -> String {
    if snippets.is_empty() {
        return "Sin snippets: créalos en la TUI con F8 y n.\n".to_string();
    }
    let mut texto = format!(
        "{:<28} {:<9} {:<6} {:<25} DESTINOS\n",
        "SNIPPET", "CRÍTICO", "USOS", "ÚLTIMO USO"
    );
    for snippet in snippets {
        let resueltos = resolver(&snippet.destinos, hosts, grupos);
        let nombres: Vec<&str> = resueltos.iter().map(|host| host.nombre.as_str()).collect();
        let destinos = if nombres.is_empty() {
            format!(
                "{} → ningún host",
                resumen_destino(&snippet.destinos, 0, hosts.len())
            )
        } else {
            format!(
                "{} → {}",
                resumen_destino(&snippet.destinos, nombres.len(), hosts.len()),
                nombres.join(", ")
            )
        };
        texto.push_str(&format!(
            "{:<28} {:<9} {:<6} {:<25} {}\n",
            recortar(&snippet.nombre, 28),
            if snippet.critico { "sí" } else { "no" },
            snippet.usado_veces,
            snippet.ultimo_uso_en.as_deref().unwrap_or("nunca"),
            destinos
        ));
    }
    texto
}

fn recortar(texto: &str, ancho: usize) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    let recortado: String = texto.chars().take(ancho.saturating_sub(1)).collect();
    format!("{recortado}…")
}

#[cfg(test)]
mod pruebas {
    use super::*;

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
    fn normalizar_limpia_etiquetas_y_destinos() {
        let datos = normalizar(DatosSnippet {
            nombre: "  reiniciar nginx ".to_string(),
            comando: "systemctl restart nginx\n\n".to_string(),
            etiquetas: vec!["Web  nginx".to_string(), "web".to_string()],
            destinos: vec![
                Destino::Etiqueta("Web".to_string()),
                Destino::Etiqueta("web".to_string()),
                Destino::Etiqueta("  ".to_string()),
            ],
            ..Default::default()
        });
        assert_eq!(datos.nombre, "reiniciar nginx");
        assert_eq!(datos.comando, "systemctl restart nginx");
        assert_eq!(datos.etiquetas, vec!["nginx", "web"]);
        assert_eq!(datos.destinos, vec![Destino::Etiqueta("web".to_string())]);
    }

    #[test]
    fn validar_exige_lo_minimo() {
        let bueno = DatosSnippet {
            nombre: "x".to_string(),
            comando: "uptime".to_string(),
            destinos: vec![Destino::Etiqueta("web".to_string())],
            ..Default::default()
        };
        assert!(validar(&bueno).is_ok());
        assert!(validar(&DatosSnippet {
            nombre: String::new(),
            ..bueno.clone()
        })
        .is_err());
        assert!(validar(&DatosSnippet {
            comando: "  ".to_string(),
            ..bueno.clone()
        })
        .is_err());
        assert!(validar(&DatosSnippet {
            timeout_seg: 4,
            ..bueno.clone()
        })
        .is_err());
        assert!(validar(&DatosSnippet {
            timeout_seg: 3601,
            ..bueno.clone()
        })
        .is_err());
        assert!(validar(&DatosSnippet {
            destinos: Vec::new(),
            ..bueno.clone()
        })
        .is_err());
        assert!(validar(&DatosSnippet {
            comando: "grep '{{x}}' f".to_string(),
            ..bueno
        })
        .is_err());
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

    #[test]
    fn el_filtro_mira_nombre_comando_etiquetas_y_destino() {
        let snippet = Snippet {
            id: 1,
            nombre: "Reiniciar nginx".to_string(),
            comando: "systemctl restart nginx".to_string(),
            descripcion: String::new(),
            etiquetas: vec!["servicios".to_string()],
            critico: true,
            timeout_seg: 60,
            parar_al_fallo: false,
            usado_veces: 0,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: vec![Destino::Etiqueta("produccion".to_string())],
        };
        assert!(coincide(&snippet, "reiniciar"));
        assert!(coincide(&snippet, "systemctl"));
        assert!(coincide(&snippet, "servicios"));
        assert!(coincide(&snippet, "producción"));
        assert!(coincide(&snippet, "nginx restart"));
        assert!(!coincide(&snippet, "postgres"));
    }

    #[test]
    fn al_conectar_solo_sin_variables_ni_critico() {
        let mut snippet = Snippet {
            id: 1,
            nombre: "x".to_string(),
            comando: "tmux attach".to_string(),
            descripcion: String::new(),
            etiquetas: Vec::new(),
            critico: false,
            timeout_seg: 60,
            parar_al_fallo: false,
            usado_veces: 0,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: vec![Destino::Etiqueta("web".to_string())],
        };
        assert!(snippet.apto_al_conectar());
        snippet.comando = "cd {{dir}}".to_string();
        assert!(!snippet.apto_al_conectar());
        snippet.comando = "tmux attach".to_string();
        snippet.critico = true;
        assert!(!snippet.apto_al_conectar());
    }

    #[test]
    fn el_comando_inicial_acaba_en_un_salto() {
        assert_eq!(texto_comando_inicial("tmux a\n\n"), "tmux a\n");
        assert_eq!(texto_comando_inicial("ls"), "ls\n");
    }
}
