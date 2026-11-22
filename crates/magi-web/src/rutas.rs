//! Rutas públicas de la web (informe §5.1) y enlaces relativos entre ellas.
//!
//! Los enlaces internos son relativos para que la web funcione igual en
//! `magi.erratia.com` y en `hectorespejo.github.io/magi/`; solo el 404 usa
//! rutas absolutas.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Idioma {
    Es,
    En,
}

impl Idioma {
    pub const TODOS: [Idioma; 2] = [Idioma::Es, Idioma::En];

    /// Valor de `lang` y `hreflang`: `es` o `en`.
    pub fn codigo(self) -> &'static str {
        match self {
            Idioma::Es => "es",
            Idioma::En => "en",
        }
    }

    pub fn otro(self) -> Idioma {
        match self {
            Idioma::Es => Idioma::En,
            Idioma::En => Idioma::Es,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pagina {
    Portada,
    Descargas,
    Versiones,
    NoEncontrada,
}

impl Pagina {
    pub const TODAS: [Pagina; 4] = [
        Pagina::Portada,
        Pagina::Descargas,
        Pagina::Versiones,
        Pagina::NoEncontrada,
    ];
}

/// Carpeta del manual dentro del sitio.
pub const RUTA_MANUAL: &str = "manual/";

/// Ruta de la página dentro del sitio, sin barra inicial:
/// `""`, `"descargas/"`, `"versiones/"`, `"en/"`, `"en/download/"`, `"en/releases/"`.
/// El 404 es el mismo fichero en los dos idiomas: `"404.html"`.
pub fn ruta(pagina: Pagina, idioma: Idioma) -> &'static str {
    match (pagina, idioma) {
        (Pagina::Portada, Idioma::Es) => "",
        (Pagina::Descargas, Idioma::Es) => "descargas/",
        (Pagina::Versiones, Idioma::Es) => "versiones/",
        (Pagina::Portada, Idioma::En) => "en/",
        (Pagina::Descargas, Idioma::En) => "en/download/",
        (Pagina::Versiones, Idioma::En) => "en/releases/",
        (Pagina::NoEncontrada, _) => "404.html",
    }
}

/// Ruta del fichero generado: `ruta` más `index.html` (o `404.html`).
pub fn fichero(pagina: Pagina, idioma: Idioma) -> String {
    let ruta = ruta(pagina, idioma);
    if ruta.is_empty() || ruta.ends_with('/') {
        format!("{ruta}index.html")
    } else {
        ruta.to_string()
    }
}

/// Prefijo relativo hasta la raíz del sitio desde la ruta de una página:
/// `""` para `""`, `"../"` para `"descargas/"`, `"../../"` para `"en/download/"`.
///
/// Cuenta las carpetas por las que hay que subir: todos los tramos menos el
/// último, que es el nombre de un fichero (`404.html`) o está vacío (`descargas/`).
pub fn hasta_raiz(ruta: &str) -> String {
    let ruta = ruta.trim_start_matches('/');
    let tramos: Vec<&str> = ruta.split('/').collect();
    let carpetas = tramos[..tramos.len() - 1]
        .iter()
        .filter(|tramo| !tramo.is_empty() && **tramo != ".")
        .count();
    "../".repeat(carpetas)
}

/// Enlace relativo desde la página en `desde` a la ruta `hasta` (ambas relativas a
/// la raíz del sitio): de `descargas/` a `en/download/` es `../en/download/`.
pub fn relativa(desde: &str, hasta: &str) -> String {
    format!("{}{}", hasta_raiz(desde), hasta.trim_start_matches('/'))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_rutas_del_informe() {
        let esperadas = [
            (Pagina::Portada, Idioma::Es, ""),
            (Pagina::Descargas, Idioma::Es, "descargas/"),
            (Pagina::Versiones, Idioma::Es, "versiones/"),
            (Pagina::Portada, Idioma::En, "en/"),
            (Pagina::Descargas, Idioma::En, "en/download/"),
            (Pagina::Versiones, Idioma::En, "en/releases/"),
            (Pagina::NoEncontrada, Idioma::Es, "404.html"),
            (Pagina::NoEncontrada, Idioma::En, "404.html"),
        ];
        for (pagina, idioma, ruta_esperada) in esperadas {
            assert_eq!(ruta(pagina, idioma), ruta_esperada);
        }
    }

    #[test]
    fn el_fichero_lleva_index_o_es_el_404() {
        assert_eq!(fichero(Pagina::Portada, Idioma::Es), "index.html");
        assert_eq!(
            fichero(Pagina::Descargas, Idioma::En),
            "en/download/index.html"
        );
        assert_eq!(fichero(Pagina::NoEncontrada, Idioma::En), "404.html");
    }

    #[test]
    fn hasta_raiz_cuenta_carpetas() {
        assert_eq!(hasta_raiz(""), "");
        assert_eq!(hasta_raiz("descargas/"), "../");
        assert_eq!(hasta_raiz("en/download/"), "../../");
        assert_eq!(hasta_raiz("404.html"), "");
        assert_eq!(hasta_raiz("manual/estado.html"), "../");
        assert_eq!(hasta_raiz("/en/"), "../");
    }

    #[test]
    fn relativa_de_los_ejemplos_del_contrato() {
        assert_eq!(relativa("descargas/", "en/download/"), "../en/download/");
        assert_eq!(relativa("en/download/", "descargas/"), "../../descargas/");
        assert_eq!(relativa("", ""), "");
    }
}
