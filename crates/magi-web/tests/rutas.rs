//! Pruebas de `rutas`: las rutas del informe y los enlaces relativos entre ellas.

use magi_web::rutas::{fichero, hasta_raiz, relativa, ruta, Idioma, Pagina, RUTA_MANUAL};

/// Todas las páginas con ruta propia, más dos del manual, como puntos de partida y de llegada.
fn todas_las_rutas() -> Vec<String> {
    let mut rutas = Vec::new();
    for pagina in Pagina::TODAS {
        for idioma in Idioma::TODOS {
            let r = ruta(pagina, idioma).to_string();
            if !rutas.contains(&r) {
                rutas.push(r);
            }
        }
    }
    rutas.push(RUTA_MANUAL.to_string());
    rutas.push(format!("{RUTA_MANUAL}estado.html"));
    rutas
}

/// Lo que haría un navegador: resuelve `href` desde la página en `desde` y devuelve
/// la ruta resultante dentro del sitio (sin barra inicial). Independiente de `relativa`.
fn resolver(desde: &str, href: &str) -> String {
    let mut carpetas: Vec<&str> = desde.split('/').collect();
    carpetas.pop(); // el último tramo es el fichero (o está vacío)
    let mut tramos: Vec<&str> = carpetas.into_iter().filter(|t| !t.is_empty()).collect();
    for tramo in href.split('/') {
        match tramo {
            "" | "." => {}
            ".." => {
                assert!(
                    tramos.pop().is_some(),
                    "{href:?} desde {desde:?} se sale de la raíz"
                );
            }
            otro => tramos.push(otro),
        }
    }
    let mut resuelta = tramos.join("/");
    if href.ends_with('/') && !resuelta.is_empty() {
        resuelta.push('/');
    }
    resuelta
}

#[test]
fn las_rutas_son_las_del_informe() {
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
    for (pagina, idioma, esperada) in esperadas {
        assert_eq!(ruta(pagina, idioma), esperada, "{pagina:?} {idioma:?}");
    }
}

#[test]
fn el_fichero_de_cada_pagina_es_unico_salvo_el_404() {
    let mut vistos = std::collections::BTreeSet::new();
    for pagina in Pagina::TODAS {
        for idioma in Idioma::TODOS {
            let f = fichero(pagina, idioma);
            assert!(
                f.ends_with("index.html") || f == "404.html",
                "{pagina:?} {idioma:?}: {f}"
            );
            if pagina != Pagina::NoEncontrada {
                assert!(vistos.insert(f.clone()), "{f} repetido");
            }
            // La profundidad del fichero es la de su ruta.
            assert_eq!(hasta_raiz(&f), hasta_raiz(ruta(pagina, idioma)));
        }
    }
    assert_eq!(fichero(Pagina::NoEncontrada, Idioma::Es), "404.html");
    assert_eq!(fichero(Pagina::Portada, Idioma::Es), "index.html");
    assert_eq!(fichero(Pagina::Portada, Idioma::En), "en/index.html");
}

#[test]
fn hasta_raiz_de_los_ejemplos_del_contrato() {
    assert_eq!(hasta_raiz(""), "");
    assert_eq!(hasta_raiz("descargas/"), "../");
    assert_eq!(hasta_raiz("en/download/"), "../../");
    assert_eq!(hasta_raiz("404.html"), "");
}

#[test]
fn las_rutas_relativas_entre_idiomas_son_correctas() {
    // Los ejemplos del informe.
    assert_eq!(relativa("descargas/", "en/download/"), "../en/download/");
    assert_eq!(relativa("en/download/", "descargas/"), "../../descargas/");
    assert_eq!(relativa("", ""), "");
    assert_eq!(relativa("", "en/"), "en/");
    assert_eq!(relativa("en/", ""), "../");
    assert_eq!(relativa("en/releases/", "versiones/"), "../../versiones/");
    assert_eq!(relativa("versiones/", "manual/"), "../manual/");
    assert_eq!(relativa("en/", "manual/"), "../manual/");

    // Todas las combinaciones: lo que resuelve un navegador es la ruta de destino.
    let rutas = todas_las_rutas();
    assert!(rutas.len() >= 8);
    for desde in &rutas {
        for hasta in &rutas {
            let enlace = relativa(desde, hasta);
            assert!(
                !enlace.starts_with('/') && !enlace.contains("://"),
                "{desde:?} → {hasta:?}: {enlace:?} no es relativo"
            );
            assert_eq!(
                &resolver(desde, &enlace),
                hasta,
                "de {desde:?} a {hasta:?} salió {enlace:?}"
            );
        }
    }

    // Y por parejas página-idioma, como las usa el selector de idioma.
    for pagina in [Pagina::Portada, Pagina::Descargas, Pagina::Versiones] {
        for idioma in Idioma::TODOS {
            let desde = ruta(pagina, idioma);
            let hasta = ruta(pagina, idioma.otro());
            assert_eq!(resolver(desde, &relativa(desde, hasta)), hasta);
        }
    }
}

#[test]
fn la_raiz_sirve_para_llegar_a_la_hoja_de_estilos_desde_cualquier_pagina() {
    for pagina in Pagina::TODAS {
        for idioma in Idioma::TODOS {
            let desde = ruta(pagina, idioma);
            let hoja = format!("{}estilos/web.css", hasta_raiz(desde));
            assert_eq!(resolver(desde, &hoja), "estilos/web.css", "{desde:?}");
        }
    }
}
