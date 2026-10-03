//! Exclusiones de una sincronización (Fase 8, §7.2), con la semántica de
//! gitignore del crate `ignore` (el mismo motor que usa git, R48).
//!
//! El orden manda: primero `[archivos] excluir`, después el `.magiignore` de la
//! raíz del origen y por último las extras del diálogo o de la guardada. Como
//! en gitignore, gana el último patrón que casa (un `!patrón` posterior vuelve
//! a incluir). Una ruta excluida no se crea, no se actualiza y **nunca** se
//! borra en el destino: el mismo filtro se aplica a los dos árboles.
//!
//! Lo comparten el cliente (árbol local) y el servidor (`ListarArbol`).

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// Filtro compilado. Las rutas se le pasan relativas a la raíz del árbol, con
/// `/` como separador.
#[derive(Debug, Clone)]
pub struct Exclusiones {
    filtro: Gitignore,
    /// Patrones que trajo el `.magiignore` (para el diálogo SINCRONIZAR).
    patrones_magiignore: usize,
}

impl Exclusiones {
    /// Compila los tres grupos en su orden. Un patrón que no se entiende es un
    /// error con su texto: sincronizar con un filtro a medias podría borrar lo
    /// que el usuario quiso proteger.
    pub fn nueva(
        por_defecto: &[String],
        magiignore: Option<&str>,
        extras: &[String],
    ) -> Result<Self, String> {
        // La raíz es ficticia: solo se le pasan rutas relativas.
        let mut constructor = GitignoreBuilder::new("/");
        let mut patrones_magiignore = 0;
        for patron in por_defecto {
            anadir(&mut constructor, patron, "[archivos] excluir")?;
        }
        if let Some(texto) = magiignore {
            for linea in texto.lines() {
                if es_patron(linea) {
                    patrones_magiignore += 1;
                }
                anadir(&mut constructor, linea, ".magiignore")?;
            }
        }
        for patron in extras {
            anadir(&mut constructor, patron, "exclusiones")?;
        }
        let filtro = constructor
            .build()
            .map_err(|error| format!("exclusiones no válidas: {error}"))?;
        Ok(Self {
            filtro,
            patrones_magiignore,
        })
    }

    /// Sin ningún patrón: no excluye nada.
    pub fn ninguna() -> Self {
        Self::nueva(&[], None, &[]).expect("un filtro vacío siempre compila")
    }

    /// ¿Queda fuera esta ruta (o alguno de sus directorios padre)? `relativa`
    /// no lleva `/` inicial; `es_dir` decide si casan los patrones con `/`
    /// final.
    pub fn excluida(&self, relativa: &str, es_dir: bool) -> bool {
        let relativa = relativa.trim_start_matches('/');
        if relativa.is_empty() {
            return false;
        }
        self.filtro
            .matched_path_or_any_parents(Path::new(relativa), es_dir)
            .is_ignore()
    }

    /// Patrones efectivos que trajo el `.magiignore`.
    pub fn patrones_magiignore(&self) -> usize {
        self.patrones_magiignore
    }
}

/// Patrones efectivos de un `.magiignore` (sin vacías ni comentarios), para
/// enseñar el recuento antes de compilar nada.
pub fn contar_patrones(texto: &str) -> usize {
    texto.lines().filter(|linea| es_patron(linea)).count()
}

fn es_patron(linea: &str) -> bool {
    let linea = linea.trim();
    !linea.is_empty() && !linea.starts_with('#')
}

fn anadir(constructor: &mut GitignoreBuilder, patron: &str, origen: &str) -> Result<(), String> {
    constructor
        .add_line(None, patron)
        .map(|_| ())
        .map_err(|error| {
            format!(
                "patrón no válido en {origen} («{}»): {error}",
                patron.trim()
            )
        })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn patrones(lista: &[&str]) -> Vec<String> {
        lista.iter().map(|patron| patron.to_string()).collect()
    }

    fn por_defecto() -> Vec<String> {
        crate::config::EXCLUIR_POR_DEFECTO
            .iter()
            .map(|patron| patron.to_string())
            .collect()
    }

    #[test]
    fn las_exclusiones_por_defecto_quitan_lo_de_siempre() {
        let filtro = Exclusiones::nueva(&por_defecto(), None, &[]).unwrap();
        assert!(filtro.excluida(".git", true));
        assert!(
            filtro.excluida(".git/config", false),
            "lo de dentro también"
        );
        assert!(filtro.excluida("web/node_modules", true));
        assert!(filtro.excluida("web/node_modules/react/index.js", false));
        assert!(filtro.excluida("src/__pycache__", true));
        assert!(filtro.excluida(".DS_Store", false));
        assert!(filtro.excluida("static/.DS_Store", false));
        assert!(filtro.excluida(".magiignore", false));
        assert!(!filtro.excluida("main.py", false));
        assert!(!filtro.excluida("static/app.js", false));
        // `target/` solo casa con directorios.
        assert!(filtro.excluida("target", true));
        assert!(!filtro.excluida("target", false));
    }

    #[test]
    fn el_orden_manda_y_la_negacion_vuelve_a_incluir() {
        let filtro = Exclusiones::nueva(
            &patrones(&["*.log"]),
            Some("# comentario\n\n!importante.log\n"),
            &patrones(&["tmp/"]),
        )
        .unwrap();
        assert!(filtro.excluida("error.log", false));
        assert!(
            !filtro.excluida("importante.log", false),
            "el .magiignore puede reincluir lo que excluye la configuración"
        );
        assert!(filtro.excluida("tmp", true));
        assert!(filtro.excluida("tmp/a/b.txt", false));
        assert_eq!(filtro.patrones_magiignore(), 1);

        // Unas extras posteriores ganan al .magiignore.
        let filtro = Exclusiones::nueva(
            &[],
            Some("!importante.log\n"),
            &patrones(&["importante.log"]),
        )
        .unwrap();
        assert!(filtro.excluida("importante.log", false));
    }

    /// AC del checklist: `.env` excluido sigue en el destino aunque se borre lo
    /// que sobra (el plan nunca ve lo excluido).
    #[test]
    fn un_patron_anclado_solo_casa_en_la_raiz() {
        let filtro =
            Exclusiones::nueva(&[], None, &patrones(&["/.env", "config/*.secret"])).unwrap();
        assert!(filtro.excluida(".env", false));
        assert!(!filtro.excluida("app/.env", false));
        assert!(filtro.excluida("config/db.secret", false));
        assert!(!filtro.excluida("otra/config/db.secret", false));
    }

    #[test]
    fn sin_patrones_no_se_excluye_nada() {
        let filtro = Exclusiones::ninguna();
        assert!(!filtro.excluida(".git", true));
        assert!(!filtro.excluida("", true), "la raíz nunca se excluye");
    }

    #[test]
    fn el_recuento_del_magiignore_ignora_comentarios_y_vacias() {
        assert_eq!(contar_patrones("# a\n\n*.log\n tmp/ \n!b\n"), 3);
        assert_eq!(contar_patrones(""), 0);
    }
}
