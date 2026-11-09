//! Snippets (Fase 6): comandos guardados con variables, dirigidos a etiquetas
//! de host (por nombre) y a hosts sueltos. Aquí vive el modelo y la
//! validación; la resolución de destinos y cuándo deliberar, en `destinos`; y
//! los estados de una ejecución, en `ejecucion`. El CRUD está en
//! `almacen::snippets`; la ejecución en sí, en cada plataforma.

pub mod destinos;
pub mod ejecucion;
pub mod salida;
pub mod variables;

pub use destinos::*;

use std::collections::BTreeSet;

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
