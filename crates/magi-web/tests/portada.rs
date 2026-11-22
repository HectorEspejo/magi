//! Pruebas de la portada (pista P2C): `Capturas::cargar` y `portada::renderizar`
//! con las plantillas y los textos reales de `web/` y los fragmentos de ejemplo de
//! `tests/fijos/capturas/`.
//!
//! Lo de la portada se prueba con `cargar_con` y `renderizar_con`, a los que se pasa
//! un validador de prueba, para saber que la portada llama a los validadores y hace
//! caso de lo que dicen; al final, unas pruebas usan los validadores de verdad
//! (`capturas::validar_fragmento` y `capturas::validar_css`).

use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use magi_web::capturas::IDS;
use magi_web::comprobar::ListaPermitida;
use magi_web::datos::{Canal, Plataformas};
use magi_web::descargas::{EleccionDescargas, VersionElegida};
use magi_web::enlaces::{clasificar, Destino};
use magi_web::html::{self, Token};
use magi_web::paginas::portada::{self, Capturas};
use magi_web::paginas::Entorno;
use magi_web::plantillas::{escapar_html, Plantillas};
use magi_web::rutas::{self, Idioma, Pagina};
use magi_web::textos::Textos;
use magi_web::{ErrorWeb, Resultado};

fn web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web")
}

fn fijos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fijos/capturas")
}

// ───────────────────────── validadores de prueba ─────────────────────────

/// Un validador de fragmentos para las pruebas: solo `<pre class="captura"
/// aria-hidden="true">`, `<span class="cN">` y texto escapado. No sustituye al de
/// verdad (`capturas::validar_fragmento`): sirve para probar que la portada lo
/// llama y hace caso de lo que dice.
fn fragmento_de_prueba(html: &str) -> Resultado<()> {
    let raro = |que: &str| ErrorWeb::dato_raro("fragmento (validador de prueba)", que);
    let Some(resto) = html.strip_prefix("<pre class=\"captura\" aria-hidden=\"true\">") else {
        return Err(raro("no empieza por el <pre> de una captura"));
    };
    let Some(cuerpo) = resto.strip_suffix("</pre>") else {
        return Err(raro("no termina en </pre>"));
    };
    let mut i = 0;
    let bytes = cuerpo.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'<' => {
                let resto = &cuerpo[i..];
                if let Some(despues) = resto.strip_prefix("<span class=\"c") {
                    let cifras = despues.bytes().take_while(u8::is_ascii_digit).count();
                    if cifras == 0 || !despues[cifras..].starts_with("\">") {
                        return Err(raro("un <span> que no es de la forma class=\"cN\""));
                    }
                    i += "<span class=\"c".len() + cifras + 2;
                } else if resto.starts_with("</span>") {
                    i += "</span>".len();
                } else {
                    return Err(raro("una etiqueta que no es <span> ni </span>"));
                }
            }
            b'&' => {
                let resto = &cuerpo[i..];
                let entidad = ["&amp;", "&lt;", "&gt;"]
                    .into_iter()
                    .find(|e| resto.starts_with(e));
                match entidad {
                    Some(e) => i += e.len(),
                    None => return Err(raro("un & sin escapar")),
                }
            }
            b'>' => return Err(raro("un > sin escapar")),
            _ => i += 1,
        }
    }
    Ok(())
}

fn css_de_prueba(css: &str) -> Resultado<()> {
    if css.contains("url(") || css.contains("@import") {
        return Err(ErrorWeb::dato_raro(
            "capturas.css (validador de prueba)",
            "lleva url( o @import",
        ));
    }
    Ok(())
}

fn cargar_fijos() -> Capturas {
    Capturas::cargar_con(&fijos(), &fragmento_de_prueba, &css_de_prueba).expect("fijos válidos")
}

// ───────────────────────── mundo ─────────────────────────

struct Mundo {
    plantillas: Plantillas,
    textos: Textos,
    plataformas: Plataformas,
}

fn mundo() -> Mundo {
    Mundo {
        plantillas: Plantillas::cargar(&web().join("plantillas")).expect("plantillas reales"),
        textos: Textos::cargar(&web().join("textos")).expect("textos reales"),
        plataformas: Plataformas::cargar(&web().join("datos/plataformas.toml"))
            .expect("plataformas reales"),
    }
}

impl Mundo {
    fn entorno(&self) -> Entorno<'_> {
        Entorno {
            plantillas: &self.plantillas,
            textos: &self.textos,
            plataformas: &self.plataformas,
            version_manual: "1.6.1",
        }
    }
}

fn version(canal: Canal, etiqueta: &str, numero: &str) -> VersionElegida {
    VersionElegida {
        canal,
        etiqueta: etiqueta.to_string(),
        numero: numero.to_string(),
        fecha: "2026-11-22T11:04:48Z".to_string(),
        url: format!("https://github.com/HectorEspejo/magi/releases/tag/{etiqueta}"),
    }
}

/// Escritorio 1.6.1, Termux un paso por detrás (1.6.0) y la app sin publicar.
fn eleccion() -> EleccionDescargas {
    EleccionDescargas {
        escritorio: version(Canal::Escritorio, "v1.6.1", "1.6.1"),
        termux: Some(version(Canal::Termux, "v1.6.0", "1.6.0")),
        app: None,
        artefactos: Vec::new(),
        homebrew: false,
        aur: false,
    }
}

fn pagina(m: &Mundo, idioma: Idioma, eleccion: &EleccionDescargas) -> String {
    portada::renderizar_con(
        &m.entorno(),
        idioma,
        eleccion,
        &cargar_fijos(),
        &fragmento_de_prueba,
    )
    .expect("portada")
}

// ───────────────────────── utilidades de HTML ─────────────────────────

/// Atributos `href` y `src` de la página, tal cual están escritos.
fn enlaces(html: &str) -> Vec<String> {
    let mut salida = Vec::new();
    for token in html::tokenizar(html).expect("HTML que se puede leer") {
        if let Token::Etiqueta(e) = token {
            for atributo in ["href", "src"] {
                if let Some(valor) = e.atributo(atributo) {
                    salida.push(valor.to_string());
                }
            }
        }
    }
    salida
}

/// El trozo de HTML desde `desde` hasta la primera aparición de `hasta` (incluida).
fn trozo<'a>(html: &'a str, desde: &str, hasta: &str) -> &'a str {
    let inicio = html
        .find(desde)
        .unwrap_or_else(|| panic!("no está «{desde}» en el HTML:\n{html}"));
    let resto = &html[inicio..];
    let fin = resto
        .find(hasta)
        .unwrap_or_else(|| panic!("no está «{hasta}» después de «{desde}»"));
    &resto[..fin + hasta.len()]
}

fn ocurrencias(texto: &str, aguja: &str) -> usize {
    texto.matches(aguja).count()
}

/// Un texto de `web/textos/`.
fn texto(m: &Mundo, idioma: Idioma, clave: &str) -> String {
    m.textos
        .texto(idioma, clave)
        .unwrap_or_else(|| panic!("falta el texto {clave}"))
        .to_string()
}

/// Copia los fijos a una carpeta temporal para estropearlos.
fn copiar_fijos() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("carpeta temporal");
    for entrada in fs::read_dir(fijos()).expect("fijos") {
        let ruta = entrada.expect("entrada").path();
        fs::copy(&ruta, dir.path().join(ruta.file_name().expect("nombre"))).expect("copia");
    }
    dir
}

// ───────────────────────── capturas: figuras y descripciones ─────────────────────────

#[test]
fn la_portada_lleva_las_seis_capturas_en_figuras_con_descripcion() {
    let m = mundo();
    let capturas = cargar_fijos();
    let abre = regex::Regex::new(
        r#"<figure class="captura-figura captura-([a-z-]+)" role="img" aria-label="([^"]*)">"#,
    )
    .expect("expresión");
    for idioma in Idioma::TODOS {
        let html = pagina(&m, idioma, &eleccion());
        // La rejilla: las seis, en el orden de IDS, cada una con su título, su
        // descripción como texto alternativo y debajo, a la vista, y su fragmento.
        let rejilla = trozo(&html, "<div class=\"capturas-rejilla\">", "</section>");
        let figuras: Vec<_> = abre.captures_iter(rejilla).collect();
        let ids: Vec<&str> = figuras
            .iter()
            .map(|c| c.get(1).expect("id").as_str())
            .collect();
        assert_eq!(ids, IDS, "{idioma:?}: las figuras y su orden");
        assert_eq!(ocurrencias(rejilla, "</figure>"), 6, "{idioma:?}");

        let mut titulos = BTreeSet::new();
        for id in IDS {
            let titulo = escapar_html(&texto(&m, idioma, &format!("captura.{id}.titulo")));
            let descripcion =
                escapar_html(&texto(&m, idioma, &format!("captura.{id}.descripcion")));
            assert!(!titulo.is_empty() && !descripcion.is_empty());
            assert!(titulos.insert(titulo.clone()), "título repetido: {titulo}");
            let figura = trozo(
                rejilla,
                &format!("<figure class=\"captura-figura captura-{id}\""),
                "</figure>",
            );
            assert!(
                figura.contains(&format!("aria-label=\"{descripcion}\"")),
                "{idioma:?} {id}: el texto alternativo es la descripción: {figura}"
            );
            assert!(
                figura.contains(&format!(
                    "<figcaption class=\"captura-titulo\">{titulo}</figcaption>"
                )),
                "{idioma:?} {id}: {figura}"
            );
            assert!(
                figura.contains(&format!(
                    "<p class=\"captura-descripcion\">{descripcion}</p>"
                )),
                "{idioma:?} {id}: la descripción también se ve: {figura}"
            );
            // El fragmento va tal cual, sin escapar otra vez (`&amp;` sigue siendo `&amp;`).
            assert!(
                figura.contains(&capturas.fragmentos[id]),
                "{idioma:?} {id}: no lleva su fragmento intacto"
            );
            assert!(figura.contains(&format!("marca-{id}")), "{idioma:?} {id}");
            assert!(
                !figura.contains("&amp;amp;"),
                "{idioma:?} {id}: doble escape"
            );
        }

        // El texto alternativo describe la vista: no lee los bordes de caja.
        for captura in &figuras {
            let etiqueta = &captura[2];
            assert!(
                !etiqueta
                    .chars()
                    .any(|c| ('\u{2500}'..='\u{257F}').contains(&c)),
                "{idioma:?}: bordes de caja en un aria-label: {etiqueta}"
            );
        }
    }
}

#[test]
fn el_pre_de_cada_captura_va_oculto_al_lector_de_pantalla() {
    let m = mundo();
    for idioma in Idioma::TODOS {
        let html = pagina(&m, idioma, &eleccion());
        // Seis en la rejilla y una más, la de Flota, en lugar de la animación.
        assert_eq!(ocurrencias(&html, "<pre"), 7, "{idioma:?}");
        assert_eq!(
            ocurrencias(&html, "<pre class=\"captura\" aria-hidden=\"true\">"),
            7,
            "{idioma:?}: todo <pre> va con aria-hidden"
        );
        // Y cada uno dentro de una figura que es una imagen con nombre: sin figuras,
        // no queda ningún <pre> en la página.
        let mut sin_figuras = html.clone();
        while let Some(inicio) = sin_figuras.find("<figure ") {
            let fin = sin_figuras[inicio..]
                .find("</figure>")
                .map(|f| inicio + f + "</figure>".len())
                .expect("la figura se cierra");
            sin_figuras.replace_range(inicio..fin, "");
        }
        assert!(
            !sin_figuras.contains("<pre"),
            "{idioma:?}: hay un <pre> fuera de una figura"
        );
        assert_eq!(ocurrencias(&html, "<figure "), 7, "{idioma:?}");
        assert_eq!(
            ocurrencias(&html, "role=\"img\" aria-label=\""),
            7,
            "{idioma:?}: cada figura es una imagen con nombre"
        );
    }
}

// ───────────────────────── animación y movimiento reducido ─────────────────────────

/// Ancho y alto de un GIF, de su cabecera.
fn medidas_del_gif() -> (u16, u16) {
    let ruta = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/demo/magi.gif");
    let bytes = fs::read(&ruta).unwrap_or_else(|e| panic!("{}: {e}", ruta.display()));
    assert_eq!(&bytes[..3], b"GIF", "{} no es un GIF", ruta.display());
    (
        u16::from_le_bytes([bytes[6], bytes[7]]),
        u16::from_le_bytes([bytes[8], bytes[9]]),
    )
}

#[test]
fn la_animacion_va_en_details_open_con_lazy() {
    let m = mundo();
    let (ancho, alto) = medidas_del_gif();
    for (idioma, raiz, resumen, alt) in [
        (
            Idioma::Es,
            "",
            "Animación de la demo",
            "MAGI en una terminal: la flota",
        ),
        (
            Idioma::En,
            "../",
            "Demo animation",
            "MAGI in a terminal: the fleet",
        ),
    ] {
        let html = pagina(&m, idioma, &eleccion());
        let demo = trozo(&html, "<details open class=\"demo\">", "</details>");
        assert!(
            demo.contains(&format!("<summary>{resumen}</summary>")),
            "{idioma:?}: {demo}"
        );
        let imagen = trozo(demo, "<img ", ">");
        assert!(
            imagen.contains(&format!("src=\"{raiz}demo/magi.gif\"")),
            "{idioma:?}: {imagen}"
        );
        assert!(imagen.contains("loading=\"lazy\""), "{imagen}");
        assert!(imagen.contains("decoding=\"async\""), "{imagen}");
        // Las medidas de la imagen son las del GIF: sin saltos al cargar.
        assert!(
            imagen.contains(&format!("width=\"{ancho}\" height=\"{alto}\"")),
            "las medidas no son las del GIF ({ancho}×{alto}): {imagen}"
        );
        assert!(
            imagen.contains(&format!("alt=\"{alt}")),
            "{idioma:?}: {imagen}"
        );
        // Es el único plegable de la página, y no hay JavaScript que lo gobierne.
        assert_eq!(ocurrencias(&html, "<details"), 1, "{idioma:?}");
        assert!(!html.contains("<script"));
    }
}

/// Una regla de CSS: dónde está (`@media …` o nada), su selector y sus declaraciones
/// como `propiedad: valor` en minúsculas y con los blancos normalizados.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Regla {
    contexto: Option<String>,
    selector: String,
    declaraciones: Vec<String>,
}

fn normalizar(texto: &str) -> String {
    texto
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn sin_comentarios(css: &str) -> String {
    let mut salida = String::new();
    let mut resto = css;
    while let Some(inicio) = resto.find("/*") {
        salida.push_str(&resto[..inicio]);
        match resto[inicio + 2..].find("*/") {
            Some(fin) => resto = &resto[inicio + 2 + fin + 2..],
            None => return salida,
        }
    }
    salida.push_str(resto);
    salida
}

/// Reglas de una CSS, con su `@media` o `@supports` si lo tienen. Las demás reglas
/// `@` (`@font-face`) se saltan.
fn reglas_de_css(css: &str, contexto: Option<&str>, salida: &mut Vec<Regla>) {
    let mut resto = css;
    while let Some(llave) = resto.find('{') {
        let preludio = normalizar(&resto[..llave]);
        let mut profundidad = 1;
        let mut fin = None;
        for (i, c) in resto[llave + 1..].char_indices() {
            match c {
                '{' => profundidad += 1,
                '}' => {
                    profundidad -= 1;
                    if profundidad == 0 {
                        fin = Some(llave + 1 + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let fin = fin.expect("las llaves de la CSS están emparejadas");
        let cuerpo = &resto[llave + 1..fin];
        if preludio.starts_with("@media") || preludio.starts_with("@supports") {
            reglas_de_css(cuerpo, Some(&preludio), salida);
        } else if !preludio.starts_with('@') {
            let declaraciones: Vec<String> = cuerpo
                .split(';')
                .map(normalizar)
                .filter(|d| !d.is_empty())
                .map(|d| match d.split_once(':') {
                    Some((p, v)) => format!("{}: {}", p.trim(), v.trim()),
                    None => d,
                })
                .collect();
            for selector in preludio.split(',') {
                salida.push(Regla {
                    contexto: contexto.map(str::to_string),
                    selector: selector.trim().to_string(),
                    declaraciones: declaraciones.clone(),
                });
            }
        }
        resto = &resto[fin + 1..];
    }
}

fn reglas_reales() -> Vec<Regla> {
    let css = fs::read_to_string(web().join("estilos/web.css")).expect("web.css");
    let mut reglas = Vec::new();
    reglas_de_css(&sin_comentarios(&css), None, &mut reglas);
    reglas
}

fn tiene(reglas: &[Regla], contexto: Option<&str>, selector: &str, declaracion: &str) -> bool {
    reglas.iter().any(|r| {
        r.contexto.as_deref() == contexto
            && r.selector == selector
            && r.declaraciones.iter().any(|d| d == declaracion)
    })
}

const MOVIMIENTO_REDUCIDO: &str = "@media (prefers-reduced-motion: reduce)";

#[test]
fn el_lector_de_css_de_las_pruebas_distingue_donde_esta_cada_regla() {
    let mut reglas = Vec::new();
    reglas_de_css(
        &sin_comentarios(
            "/* x { color: red } */ .a { Display : none; } \
             @font-face { font-family: X; } \
             @media (prefers-reduced-motion: reduce) { .a, .b { display: block } }",
        ),
        None,
        &mut reglas,
    );
    assert!(tiene(&reglas, None, ".a", "display: none"));
    assert!(!tiene(&reglas, None, ".a", "display: block"));
    assert!(tiene(
        &reglas,
        Some(MOVIMIENTO_REDUCIDO),
        ".a",
        "display: block"
    ));
    assert!(tiene(
        &reglas,
        Some(MOVIMIENTO_REDUCIDO),
        ".b",
        "display: block"
    ));
    assert!(!tiene(&reglas, None, ".b", "display: block"));
    assert!(reglas.iter().all(|r| r.selector != "font-family"));
}

#[test]
fn con_reduced_motion_la_captura_de_flota_sustituye_a_la_animacion() {
    let m = mundo();
    for idioma in Idioma::TODOS {
        let html = pagina(&m, idioma, &eleccion());
        // El bloque con la captura de Flota es hermano de la animación, no hijo: al
        // ocultarse la una, no se oculta la otra.
        let fin_demo = html.find("</details>").expect("fin de la animación");
        let inicio_estatica = html
            .find("<div class=\"demo-estatica\">")
            .expect("el bloque con la captura de Flota");
        assert!(fin_demo < inicio_estatica, "{idioma:?}");
        let estatica = trozo(&html, "<div class=\"demo-estatica\">", "</figure>");
        assert!(
            estatica.contains("captura-figura captura-flota"),
            "{estatica}"
        );
        assert!(estatica.contains("marca-flota"), "{estatica}");
        assert!(estatica.contains("role=\"img\""), "{estatica}");
        assert!(!estatica.contains("<img"), "{estatica}");
        assert!(!trozo(&html, "<details open class=\"demo\">", "</details>").contains("<figure"));
    }

    // La CSS: por defecto se ve la animación y no la captura; con movimiento
    // reducido, al revés (y Flota no sale otra vez en la rejilla).
    let reglas = reglas_reales();
    assert!(tiene(&reglas, None, ".demo-estatica", "display: none"));
    assert!(!tiene(&reglas, None, ".demo", "display: none"));
    assert!(tiene(
        &reglas,
        Some(MOVIMIENTO_REDUCIDO),
        ".demo",
        "display: none"
    ));
    assert!(tiene(
        &reglas,
        Some(MOVIMIENTO_REDUCIDO),
        ".demo-estatica",
        "display: block"
    ));
    assert!(tiene(
        &reglas,
        Some(MOVIMIENTO_REDUCIDO),
        ".capturas-rejilla .captura-flota",
        "display: none"
    ));
}

#[test]
fn las_capturas_caben_en_su_marco_y_la_pagina_no_se_desplaza() {
    let reglas = reglas_reales();
    assert!(tiene(
        &reglas,
        None,
        ".captura-figura",
        "container-type: inline-size"
    ));
    assert!(tiene(&reglas, None, ".captura-figura", "min-width: 0"));
    // El texto de la captura se escala con el ancho de su marco (100 columnas) y, si
    // aun así no cabe legible, se desplaza dentro del marco.
    let pre = ".captura-figura pre";
    assert!(tiene(&reglas, None, pre, "overflow-x: auto"));
    assert!(tiene(&reglas, None, pre, "white-space: pre"));
    assert!(tiene(&reglas, None, pre, "margin: 0"));
    assert!(
        reglas.iter().any(|r| r.selector == pre
            && r.declaraciones
                .iter()
                .any(|d| d.starts_with("font-size: max(9px,") && d.contains("cqw"))),
        "falta el tamaño de fuente en cqw con un mínimo de 9px"
    );
    // Lo de la captura (colores y fondo) lo pone capturas.css: web.css no lo pisa.
    for r in reglas
        .iter()
        .filter(|r| r.selector.contains(".captura-figura"))
    {
        assert!(
            r.declaraciones
                .iter()
                .all(|d| !d.starts_with("background") && !d.starts_with("color:")),
            "{r:?} pisa los colores de la captura"
        );
    }
    assert!(tiene(&reglas, None, ".demo img", "max-width: 100%"));
}

#[test]
fn una_columna_de_la_rejilla_nunca_es_mas_estrecha_que_una_captura() {
    // Cien columnas de 0,6 em al tamaño mínimo de letra, más el borde del marco: con
    // menos ancho, la captura se cortaría en una columna aunque en una sola cabría.
    let reglas = reglas_reales();
    let minimo_de_letra: f64 = reglas
        .iter()
        .filter(|r| r.selector == ".captura-figura pre")
        .flat_map(|r| r.declaraciones.iter())
        .find_map(|d| d.strip_prefix("font-size: max(").map(str::to_string))
        .and_then(|d| d.split("px").next().and_then(|n| n.trim().parse().ok()))
        .expect("el tamaño mínimo de letra de la captura");
    let necesario = 100.0 * 0.6 * minimo_de_letra + 2.0;

    let columnas = regex::Regex::new(r"minmax\(min\(100%, ([0-9.]+)rem\)").expect("expresión");
    let declaracion = reglas
        .iter()
        .filter(|r| r.selector == ".capturas-rejilla")
        .flat_map(|r| r.declaraciones.iter())
        .find(|d| d.starts_with("grid-template-columns:"))
        .expect("las columnas de la rejilla");
    let minimo_de_columna: f64 = columnas
        .captures(declaracion)
        .unwrap_or_else(|| panic!("columna mínima en rem: {declaracion}"))[1]
        .parse()
        .expect("número");
    // 1 rem = 16 px (el tamaño de letra por defecto del navegador).
    assert!(
        minimo_de_columna * 16.0 >= necesario,
        "las columnas de {minimo_de_columna}rem no dejan sitio a una captura ({necesario} px)"
    );
}

// ───────────────────────── botón, textos y enlaces ─────────────────────────

#[test]
fn el_boton_principal_lleva_la_version_del_escritorio() {
    let m = mundo();
    // Termux y la app van por detrás o por delante: el botón es del escritorio.
    let mut e = eleccion();
    e.escritorio = version(Canal::Escritorio, "v2.3.4", "2.3.4");
    e.app = Some(version(Canal::App, "android-v0.2.0", "0.2.0"));
    for (idioma, boton, manual) in [
        (
            Idioma::Es,
            "<a class=\"boton\" href=\"descargas/\">Descargar 2.3.4</a>",
            "<a class=\"boton boton-secundario\" href=\"manual/\" hreflang=\"es\">Leer el manual</a>",
        ),
        (
            Idioma::En,
            "<a class=\"boton\" href=\"../en/download/\">Download 2.3.4</a>",
            "<a class=\"boton boton-secundario\" href=\"../manual/\" hreflang=\"es\">Read the manual (Spanish)</a>",
        ),
    ] {
        let html = pagina(&m, idioma, &e);
        assert!(html.contains(boton), "{idioma:?}:\n{html}");
        assert!(html.contains(manual), "{idioma:?}:\n{html}");
        assert_eq!(ocurrencias(&html, "class=\"boton\""), 1, "un solo botón principal");
        assert!(!html.contains("Descargar 0.2.0") && !html.contains("Download 0.2.0"));
        assert!(!html.contains("Descargar 1.6"), "{idioma:?}");
    }
}

#[test]
fn la_version_del_boton_se_escapa() {
    let m = mundo();
    let mut e = eleccion();
    let rara = "1.6.1<img src=x onerror=alert(1)>\"&'";
    e.escritorio = version(Canal::Escritorio, "v1.6.1", rara);
    for (idioma, verbo) in [(Idioma::Es, "Descargar"), (Idioma::En, "Download")] {
        let html = pagina(&m, idioma, &e);
        assert!(
            html.contains(&format!(
                "{verbo} 1.6.1&lt;img src=x onerror=alert(1)&gt;&quot;&amp;&#x27;</a>"
            )),
            "{idioma:?}:\n{html}"
        );
        assert!(!html.contains("<img src=x"), "{idioma:?}: HTML sin escapar");
        assert!(!html.contains("onerror=alert(1)>"), "{idioma:?}");
    }

    // Lo que no es un número de versión no llega a la página.
    for mala in ["", "   ", "1.6.1\n<h1>", &"9".repeat(65)] {
        let mut e = eleccion();
        e.escritorio = version(Canal::Escritorio, "v1.6.1", mala);
        let error = portada::renderizar_con(
            &m.entorno(),
            Idioma::Es,
            &e,
            &cargar_fijos(),
            &fragmento_de_prueba,
        )
        .expect_err("una versión rara no se enseña");
        assert!(error.to_string().contains("número de versión"), "{error}");
    }
}

#[test]
fn la_version_de_termux_y_la_de_la_app_tambien_se_comprueban_y_se_escapan() {
    let m = mundo();
    let pintar = |e: &EleccionDescargas| {
        portada::renderizar_con(
            &m.entorno(),
            Idioma::Es,
            e,
            &cargar_fijos(),
            &fragmento_de_prueba,
        )
    };
    for mala in ["", "1.6\n0", &"9".repeat(65)] {
        let mut e = eleccion();
        e.termux = Some(version(Canal::Termux, "v1.6.0", mala));
        let error = pintar(&e).expect_err("la versión de Termux es rara");
        assert!(error.to_string().contains("número de versión"), "{error}");

        let mut e = eleccion();
        e.app = Some(version(Canal::App, "android-v0.2.0", mala));
        let error = pintar(&e).expect_err("la versión de la app es rara");
        assert!(error.to_string().contains("número de versión"), "{error}");
    }

    let mut e = eleccion();
    e.termux = Some(version(Canal::Termux, "v1.6.0", "1.6.0<b>&"));
    e.app = Some(version(Canal::App, "android-v0.2.0", "0.2.0<i>"));
    let html = pintar(&e).expect("versiones raras pero escapables");
    assert!(html.contains("1.6.0&lt;b&gt;&amp;"), "{html}");
    assert!(html.contains("0.2.0&lt;i&gt;"), "{html}");
    assert!(!html.contains("<b>&") && !html.contains("<i>"), "{html}");
}

#[test]
fn la_introduccion_de_las_plataformas_usa_los_nombres_de_sus_estados() {
    // El texto explica los estados con las mismas palabras con que se rotulan.
    let m = mundo();
    for idioma in Idioma::TODOS {
        let introduccion = texto(&m, idioma, "portada.plataformas.introduccion").to_lowercase();
        for estado in ["en_pruebas", "beta"] {
            let nombre = texto(&m, idioma, &format!("descargas.estado.{estado}")).to_lowercase();
            assert!(
                introduccion.contains(&nombre),
                "{idioma:?}: la introducción no dice «{nombre}»: {introduccion}"
            );
        }
    }
}

#[test]
fn la_portada_en_ingles_dice_manual_spanish() {
    let m = mundo();
    let en = pagina(&m, Idioma::En, &eleccion());
    let enlaces_en = trozo(&en, "<nav class=\"portada-enlaces\"", "</nav>");
    assert!(enlaces_en.contains(">Manual (Spanish)</a>"), "{enlaces_en}");
    assert!(en.contains("Read the manual (Spanish)"));
    assert!(
        enlaces_en.contains("blob/main/CONTRIBUTING.en.md\""),
        "{enlaces_en}"
    );
    assert!(!enlaces_en.contains("CONTRIBUTING.md\""));
    assert!(en.contains("<html lang=\"en\">"));

    let es = pagina(&m, Idioma::Es, &eleccion());
    let enlaces_es = trozo(&es, "<nav class=\"portada-enlaces\"", "</nav>");
    assert!(enlaces_es.contains(">Manual</a>"), "{enlaces_es}");
    assert!(!es.contains("Spanish"));
    assert!(
        enlaces_es.contains("blob/main/CONTRIBUTING.md\""),
        "{enlaces_es}"
    );
    assert!(es.contains("<html lang=\"es\">"));
}

#[test]
fn la_portada_dice_que_es_para_quien_las_plataformas_y_la_licencia() {
    let m = mundo();
    let mut con_app = eleccion();
    con_app.app = Some(version(Canal::App, "android-v0.2.0", "0.2.0"));
    for (idioma, que_es, telemetria, sin_publicar) in [
        (
            Idioma::Es,
            "Qué es MAGI y para quién",
            "sin telemetría",
            "aún sin publicar",
        ),
        (
            Idioma::En,
            "What MAGI is and who it is for",
            "no telemetry",
            "not published yet",
        ),
    ] {
        let html = pagina(&m, idioma, &eleccion());
        assert!(html.contains(que_es), "{idioma:?}");
        assert!(html.contains(telemetria), "{idioma:?}");
        assert!(
            html.contains("<code>MIT OR Apache-2.0</code>"),
            "{idioma:?}"
        );
        // Cada plataforma con su estado en texto, nunca solo con color.
        let lista = trozo(&html, "<ul class=\"portada-plataformas\">", "</ul>");
        for (familia, estado_clase) in [
            ("linux", "estado-estable"),
            ("macos", "estado-en_pruebas"),
            ("termux", "estado-en_pruebas"),
            ("android", "estado-beta"),
        ] {
            let fila = trozo(lista, &format!("<li id=\"plataforma-{familia}\">"), "</li>");
            let texto_estado = fila
                .split("class=\"estado ")
                .nth(1)
                .and_then(|r| r.split_once('>'))
                .and_then(|(clase, resto)| resto.split_once("</span>").map(|(t, _)| (clase, t)))
                .unwrap_or_else(|| panic!("{familia}: sin estado en texto: {fila}"));
            assert_eq!(
                texto_estado.0.trim_end_matches('"'),
                estado_clase,
                "{familia}"
            );
            assert!(!texto_estado.1.trim().is_empty(), "{familia}");
        }
        // Linux es estable: no manda a «Qué se ha probado»; las demás, sí.
        assert!(!trozo(lista, "<li id=\"plataforma-linux\">", "</li>").contains("estado.html"));
        let probado = rutas::relativa(rutas::ruta(Pagina::Portada, idioma), "manual/estado.html");
        for familia in ["macos", "termux", "android"] {
            let fila = trozo(lista, &format!("<li id=\"plataforma-{familia}\">"), "</li>");
            assert!(
                fila.contains(&format!("<a href=\"{probado}\" hreflang=\"es\">")),
                "{familia}: {fila}"
            );
        }
        // Las versiones salen de la elección: Termux va un paso por detrás y la app no ha salido.
        assert!(trozo(lista, "<li id=\"plataforma-linux\">", "</li>").contains("1.6.1"));
        assert!(trozo(lista, "<li id=\"plataforma-termux\">", "</li>").contains("1.6.0"));
        assert!(trozo(lista, "<li id=\"plataforma-android\">", "</li>").contains(sin_publicar));
    }
    let html = pagina(&m, Idioma::Es, &con_app);
    let lista = trozo(&html, "<ul class=\"portada-plataformas\">", "</ul>");
    assert!(trozo(lista, "<li id=\"plataforma-android\">", "</li>").contains("0.2.0"));
}

#[test]
fn la_portada_declara_idioma_titulo_y_hoja_de_capturas() {
    let m = mundo();
    for (idioma, titulo, raiz, alternativa) in [
        (Idioma::Es, "MAGI · Gestor SSH de terminal", "", "en/"),
        (Idioma::En, "MAGI · Terminal SSH manager", "../", "../"),
    ] {
        let html = pagina(&m, idioma, &eleccion());
        assert!(
            html.contains(&format!("<title>{titulo}</title>")),
            "{idioma:?}"
        );
        assert_eq!(ocurrencias(&html, "<h1"), 1, "{idioma:?}");
        assert!(html.contains("<h1 class=\"titulo-pagina\" id=\"portada-titulo\">MAGI</h1>"));
        // La hoja de las capturas va en el <head>, después de la de la web.
        let cabeza = trozo(&html, "<head>", "</head>");
        let web_css = cabeza.find("estilos/web.css").expect("web.css");
        let capturas_css = cabeza
            .find(&format!(
                "<link rel=\"stylesheet\" href=\"{raiz}capturas/capturas.css\">"
            ))
            .expect("capturas.css");
        assert!(web_css < capturas_css, "{idioma:?}");
        assert!(
            cabeza.contains(&format!(
                "hreflang=\"{}\" href=\"{alternativa}\"",
                idioma.otro().codigo()
            )),
            "{idioma:?}: {cabeza}"
        );
    }
}

#[test]
fn la_portada_no_lleva_style_ni_script() {
    let m = mundo();
    for idioma in Idioma::TODOS {
        let html = pagina(&m, idioma, &eleccion());
        for token in html::tokenizar(&html).expect("HTML que se puede leer") {
            match token {
                Token::Etiqueta(e) => {
                    assert!(
                        !["script", "style", "iframe", "object", "embed", "form", "base"]
                            .contains(&e.nombre.as_str()),
                        "{idioma:?}: lleva <{}>",
                        e.nombre
                    );
                    for (nombre, _) in &e.atributos {
                        assert!(
                            nombre != "style" && !nombre.starts_with("on"),
                            "{idioma:?}: <{}> lleva {nombre}",
                            e.nombre
                        );
                    }
                }
                Token::Crudo { etiqueta, .. } => {
                    assert!(
                        etiqueta != "script" && etiqueta != "style",
                        "{idioma:?}: lleva <{etiqueta}>"
                    );
                }
                _ => {}
            }
        }
        assert!(!html.to_lowercase().contains("javascript:"), "{idioma:?}");
    }
}

#[test]
fn los_enlaces_externos_son_https_y_de_la_lista_permitida() {
    let m = mundo();
    let lista = ListaPermitida::por_defecto();
    for idioma in Idioma::TODOS {
        let html = pagina(&m, idioma, &eleccion());
        let externos: Vec<String> = enlaces(&html)
            .into_iter()
            .filter(|h| h.contains("://") || h.starts_with("//"))
            .collect();
        assert!(!externos.is_empty());
        for url in &externos {
            assert!(
                url.starts_with("https://"),
                "{idioma:?}: «{url}» no es https"
            );
            assert!(
                lista.permite(url),
                "{idioma:?}: «{url}» no está en la lista permitida"
            );
        }
        // Los que la portada tiene que ofrecer: GitHub, seguridad y contribuir.
        let contribuir = match idioma {
            Idioma::Es => "https://github.com/HectorEspejo/magi/blob/main/CONTRIBUTING.md",
            Idioma::En => "https://github.com/HectorEspejo/magi/blob/main/CONTRIBUTING.en.md",
        };
        for esperado in [
            "https://github.com/HectorEspejo/magi",
            "https://github.com/HectorEspejo/magi/security/policy",
            contribuir,
        ] {
            assert!(
                externos.iter().any(|u| u == esperado),
                "{idioma:?}: falta el enlace a {esperado}"
            );
        }
        // Nada con `http://` ni protocolo implícito en ningún atributo ni en el texto.
        assert!(!html.contains("http://"), "{idioma:?}");
    }
}

#[test]
fn los_enlaces_internos_son_relativos() {
    let m = mundo();
    for idioma in Idioma::TODOS {
        let html = pagina(&m, idioma, &eleccion());
        let desde = rutas::fichero(Pagina::Portada, idioma);
        let prefijo = rutas::hasta_raiz(rutas::ruta(Pagina::Portada, idioma));
        let internos: Vec<String> = enlaces(&html)
            .into_iter()
            .filter(|h| !h.contains("://") && !h.starts_with("//"))
            .collect();
        assert!(internos.len() >= 8, "{idioma:?}: {internos:?}");
        let mut destinos = BTreeSet::new();
        for href in &internos {
            assert!(!href.starts_with('/'), "{idioma:?}: «{href}» es absoluto");
            match clasificar(&desde, href) {
                Destino::Interno { ruta, .. } => {
                    destinos.insert(ruta);
                }
                Destino::Ancla(_) => {}
                otro => panic!("{idioma:?}: «{href}» no es un enlace interno: {otro:?}"),
            }
            if !href.starts_with('#') {
                // Relativo a la propia página: sube hasta la raíz con `../` o no sube.
                assert!(
                    prefijo.is_empty() && !href.starts_with("../") || href.starts_with(&prefijo),
                    "{idioma:?}: «{href}» no cuelga de «{prefijo}»"
                );
            }
        }
        let descargas = rutas::fichero(Pagina::Descargas, idioma);
        for esperado in [
            "demo/magi.gif",
            "capturas/capturas.css",
            "estilos/web.css",
            "manual/index.html",
            "manual/estado.html",
            descargas.as_str(),
        ] {
            assert!(
                destinos.contains(esperado),
                "{idioma:?}: no enlaza a {esperado}: {destinos:?}"
            );
        }
    }
}

// ───────────────────────── textos ─────────────────────────

#[test]
fn los_textos_de_la_portada_y_de_las_capturas_tienen_las_mismas_claves() {
    let m = mundo();
    let de_la_portada = |idioma: Idioma| -> BTreeSet<String> {
        m.textos
            .claves(idioma)
            .into_iter()
            .filter(|c| c.starts_with("portada.") || c.starts_with("captura."))
            .collect()
    };
    let es = de_la_portada(Idioma::Es);
    let en = de_la_portada(Idioma::En);
    assert_eq!(es, en, "claves distintas entre es.toml y en.toml");

    // Las seis capturas llevan título y descripción, en los dos idiomas.
    for id in IDS {
        for campo in ["titulo", "descripcion"] {
            assert!(
                es.contains(&format!("captura.{id}.{campo}")),
                "{id} {campo}"
            );
        }
    }
    assert_eq!(
        es.iter().filter(|c| c.starts_with("captura.")).count(),
        IDS.len() * 2,
        "capturas de más o de menos en los textos: {es:?}"
    );
    for clave in es.iter().chain(en.iter()) {
        for idioma in Idioma::TODOS {
            let Some(valor) = m.textos.texto(idioma, clave) else {
                continue;
            };
            assert!(!valor.trim().is_empty(), "{clave} ({idioma:?}) está vacío");
            assert!(
                !valor.contains('<') && !valor.contains('>'),
                "{clave} ({idioma:?}) lleva HTML"
            );
        }
    }
    // El texto alternativo describe la vista: sin los bordes de caja.
    for id in IDS {
        for idioma in Idioma::TODOS {
            let descripcion = texto(&m, idioma, &format!("captura.{id}.descripcion"));
            assert!(
                !descripcion
                    .chars()
                    .any(|c| ('\u{2500}'..='\u{257F}').contains(&c)),
                "{id} ({idioma:?}): bordes de caja en la descripción"
            );
        }
    }
    // Los textos de la portada que da el informe, tal cual.
    assert_eq!(
        texto(&m, Idioma::Es, "captura.flota.descripcion"),
        "Todos tus hosts y su estado, de un vistazo."
    );
    assert_eq!(
        texto(&m, Idioma::Es, "captura.sesion-paneles.descripcion"),
        "Pestañas y paneles que siguen vivos al cerrar la ventana."
    );
}

// ───────────────────────── cargar: ficheros ─────────────────────────

#[test]
fn un_fichero_acaba_en_un_salto_de_linea_y_el_fragmento_no() {
    // Con los validadores de verdad: no admiten nada tras `</pre>`, así que lo que
    // llega a la página es el fragmento sin el salto de línea del fichero.
    for id in IDS {
        let dir = copiar_fijos();
        let ruta = dir.path().join(format!("{id}.html"));
        let sin_salto = fs::read_to_string(&ruta).expect("fijo");
        assert!(!sin_salto.ends_with('\n'), "{id}: el fijo no lleva salto");

        fs::write(&ruta, format!("{sin_salto}\n")).expect("con salto");
        let capturas = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
            .unwrap_or_else(|e| panic!("{id} con un salto final: {e}"));
        assert_eq!(capturas.fragmentos[id], sin_salto, "{id}");

        // Solo se quita uno: dos saltos no son «un fichero de texto», son basura.
        fs::write(&ruta, format!("{sin_salto}\n\n")).expect("con dos");
        let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
            .expect_err("dos saltos finales");
        assert!(error.to_string().contains(&format!("{id}.html")), "{error}");
    }
}

#[test]
fn las_capturas_del_repositorio_cargan_con_los_validadores_de_verdad() {
    let capturas = Capturas::cargar(&web().join("capturas")).expect("web/capturas");
    assert_eq!(capturas.fragmentos.len(), IDS.len());
    for id in IDS {
        assert!(!capturas.fragmentos[id].ends_with('\n'), "{id}");
    }
}

#[test]
fn falta_una_captura_y_falla_diciendo_cual() {
    for id in IDS {
        let dir = copiar_fijos();
        fs::remove_file(dir.path().join(format!("{id}.html"))).expect("quita la captura");
        let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
            .expect_err("falta una captura");
        let mensaje = error.to_string();
        assert!(mensaje.contains(&format!("«{id}»")), "{id}: {mensaje}");
        assert!(mensaje.contains(&format!("{id}.html")), "{id}: {mensaje}");
        assert!(mensaje.contains("generar-capturas.sh"), "{id}: {mensaje}");
        // Y dice qué es: una que falta, no una que está mal.
        assert!(mensaje.contains("falta"), "{id}: {mensaje}");
    }

    // También `capturas.css`, y la carpeta entera.
    let dir = copiar_fijos();
    fs::remove_file(dir.path().join("capturas.css")).expect("quita la hoja");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("falta la hoja");
    assert!(error.to_string().contains("capturas.css"), "{error}");
    let error = Capturas::cargar_con(
        &dir.path().join("no-existe"),
        &fragmento_de_prueba,
        &css_de_prueba,
    )
    .expect_err("falta la carpeta");
    assert!(error.to_string().contains("no existe"), "{error}");

    // Y al pintar: un mapa al que le falta una de las seis no se pinta a medias.
    let m = mundo();
    let mut capturas = cargar_fijos();
    capturas.fragmentos.remove("deliberacion");
    let error = portada::renderizar_con(
        &m.entorno(),
        Idioma::Es,
        &eleccion(),
        &capturas,
        &fragmento_de_prueba,
    )
    .expect_err("falta una captura");
    assert!(error.to_string().contains("«deliberacion»"), "{error}");
}

#[test]
fn cargar_lee_las_seis_capturas_y_nada_mas() {
    let capturas = cargar_fijos();
    assert_eq!(
        capturas
            .fragmentos
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        {
            let mut ordenados = IDS.to_vec();
            ordenados.sort_unstable();
            ordenados
        }
    );
    for id in IDS {
        let leido = fs::read_to_string(fijos().join(format!("{id}.html"))).expect("fijo");
        assert_eq!(capturas.fragmentos[id], leido, "{id}");
    }

    // Un fichero que sobra en la carpeta no entra.
    let dir = copiar_fijos();
    fs::write(
        dir.path().join("otra.html"),
        "<pre class=\"captura\" aria-hidden=\"true\">otra</pre>",
    )
    .expect("fichero de más");
    let capturas =
        Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba).expect("carga");
    assert_eq!(capturas.fragmentos.len(), IDS.len());
    assert!(!capturas.fragmentos.contains_key("otra"));
}

#[test]
fn un_fragmento_con_script_no_se_inserta() {
    // Un fragmento con un <script>, con un atributo style, con otra etiqueta o con
    // un & sin escapar no se carga: el error dice qué fichero es.
    for malo in [
        "<pre class=\"captura\" aria-hidden=\"true\"><script>alert(1)</script></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><span style=\"color:red\">x</span></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><img src=x onerror=alert(1)></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\">a & b</pre>",
        "<pre class=\"captura\">sin aria-hidden</pre>",
        "<pre class=\"captura\" aria-hidden=\"true\">sin cerrar",
        "<div>no es un pre</div>",
    ] {
        let dir = copiar_fijos();
        fs::write(dir.path().join("archivos.html"), malo).expect("fragmento malo");
        let error =
            Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba).expect_err(malo);
        assert!(
            error.to_string().contains("archivos.html"),
            "{malo}: {error}"
        );

        // Y aunque alguien monte el mapa a mano, saltándose `cargar`, lo para al pintar.
        let m = mundo();
        let mut capturas = cargar_fijos();
        capturas
            .fragmentos
            .insert("archivos".to_string(), malo.to_string());
        let error = portada::renderizar_con(
            &m.entorno(),
            Idioma::Es,
            &eleccion(),
            &capturas,
            &fragmento_de_prueba,
        )
        .expect_err(malo);
        assert!(
            error.to_string().contains("archivos.html"),
            "{malo}: {error}"
        );
    }

    // La hoja también se valida.
    let dir = copiar_fijos();
    fs::write(
        dir.path().join("capturas.css"),
        ".captura{background:url(https://ejemplo.org/x.png)}",
    )
    .expect("hoja mala");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("hoja con url(");
    assert!(error.to_string().contains("capturas.css"), "{error}");
}

#[test]
fn el_validador_se_llama_con_cada_fragmento_al_cargar_y_al_pintar() {
    let llamadas = Cell::new(0);
    let contador = |html: &str| -> Resultado<()> {
        llamadas.set(llamadas.get() + 1);
        fragmento_de_prueba(html)
    };
    let capturas = Capturas::cargar_con(&fijos(), &contador, &css_de_prueba).expect("carga");
    assert_eq!(llamadas.get(), IDS.len(), "uno por captura al cargar");

    let m = mundo();
    llamadas.set(0);
    portada::renderizar_con(&m.entorno(), Idioma::Es, &eleccion(), &capturas, &contador)
        .expect("portada");
    assert_eq!(llamadas.get(), IDS.len(), "uno por captura al pintar");
}

#[test]
fn un_fragmento_demasiado_grande_o_que_no_es_utf8_se_rechaza() {
    let dir = copiar_fijos();
    let grande = format!(
        "<pre class=\"captura\" aria-hidden=\"true\">{}</pre>",
        "x".repeat(300 * 1024)
    );
    fs::write(dir.path().join("flota.html"), grande).expect("fragmento grande");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("pasa del tope");
    assert!(error.to_string().contains("tope"), "{error}");
    assert!(error.to_string().contains("flota.html"), "{error}");

    let dir = copiar_fijos();
    fs::write(dir.path().join("tuneles.html"), [0xFF, 0xFE, 0x3C]).expect("no es UTF-8");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("no es UTF-8");
    assert!(error.to_string().contains("UTF-8"), "{error}");

    // El tope es exacto: un fragmento que lo llena entero se lee y uno que lo pasa
    // en un solo byte, no.
    let dir = copiar_fijos();
    let abre = "<pre class=\"captura\" aria-hidden=\"true\">";
    let relleno = |bytes: usize| {
        format!(
            "{abre}{}</pre>",
            "x".repeat(bytes - abre.len() - "</pre>".len())
        )
    };
    fs::write(dir.path().join("flota.html"), relleno(256 * 1024)).expect("justo en el tope");
    Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect("un fragmento de exactamente el tope se lee");
    fs::write(dir.path().join("flota.html"), relleno(256 * 1024 + 1)).expect("pasa en un byte");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("pasa del tope en un byte");
    assert!(error.to_string().contains("tope"), "{error}");

    // La hoja tiene su propio tope (2 MiB), con su nombre en el error.
    let dir = copiar_fijos();
    fs::write(
        dir.path().join("capturas.css"),
        format!(".captura{{color:#fff}}{}", " ".repeat(2 * 1024 * 1024)),
    )
    .expect("hoja grande");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("la hoja pasa del tope");
    assert!(error.to_string().contains("capturas.css"), "{error}");
    assert!(error.to_string().contains("tope"), "{error}");

    // Una captura que es una carpeta no es un fichero.
    let dir = copiar_fijos();
    fs::remove_file(dir.path().join("servicios.html")).expect("quita");
    fs::create_dir(dir.path().join("servicios.html")).expect("carpeta");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("una carpeta");
    assert!(
        error.to_string().contains("no es un fichero normal"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn cargar_no_sigue_enlaces_simbolicos() {
    use std::os::unix::fs::symlink;

    // Un fragmento que es un enlace a otro fichero (aunque sea válido).
    let dir = copiar_fijos();
    let aparte = tempfile::tempdir().expect("carpeta aparte");
    let destino = aparte.path().join("fuera.html");
    fs::copy(fijos().join("flota.html"), &destino).expect("copia");
    fs::remove_file(dir.path().join("flota.html")).expect("quita");
    symlink(&destino, dir.path().join("flota.html")).expect("enlace");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("enlace simbólico");
    assert!(error.to_string().contains("enlace simbólico"), "{error}");
    assert!(error.to_string().contains("flota.html"), "{error}");

    // La hoja, igual.
    let dir = copiar_fijos();
    let hoja = aparte.path().join("fuera.css");
    fs::copy(fijos().join("capturas.css"), &hoja).expect("copia");
    fs::remove_file(dir.path().join("capturas.css")).expect("quita");
    symlink(&hoja, dir.path().join("capturas.css")).expect("enlace");
    let error = Capturas::cargar_con(dir.path(), &fragmento_de_prueba, &css_de_prueba)
        .expect_err("enlace simbólico");
    assert!(error.to_string().contains("enlace simbólico"), "{error}");

    // Y la carpeta entera.
    let enlace_a_la_carpeta = aparte.path().join("capturas");
    symlink(fijos(), &enlace_a_la_carpeta).expect("enlace a la carpeta");
    let error = Capturas::cargar_con(&enlace_a_la_carpeta, &fragmento_de_prueba, &css_de_prueba)
        .expect_err("carpeta enlazada");
    assert!(error.to_string().contains("enlace simbólico"), "{error}");
}

// ───────────────────────── con los validadores de verdad ─────────────────────────

#[test]
fn con_los_validadores_de_verdad_cargan_los_fijos_y_se_pinta_la_portada() {
    let capturas = Capturas::cargar(&fijos()).expect("los fijos son válidos");
    assert_eq!(capturas.fragmentos.len(), IDS.len());
    let m = mundo();
    for idioma in Idioma::TODOS {
        let html =
            portada::renderizar(&m.entorno(), idioma, &eleccion(), &capturas).expect("portada");
        assert_eq!(
            ocurrencias(&html, "<pre class=\"captura\" aria-hidden=\"true\">"),
            7
        );
    }
}

#[test]
fn con_los_validadores_de_verdad_un_fragmento_con_script_no_se_inserta() {
    let dir = copiar_fijos();
    fs::write(
        dir.path().join("archivos.html"),
        "<pre class=\"captura\" aria-hidden=\"true\"><script>alert(1)</script></pre>",
    )
    .expect("fragmento malo");
    let error = Capturas::cargar(dir.path()).expect_err("un script no se carga");
    assert!(error.to_string().contains("archivos.html"), "{error}");

    let m = mundo();
    let mut capturas = Capturas::cargar(&fijos()).expect("los fijos son válidos");
    capturas.fragmentos.insert(
        "archivos".to_string(),
        "<pre class=\"captura\" aria-hidden=\"true\"><script>alert(1)</script></pre>".to_string(),
    );
    portada::renderizar(&m.entorno(), Idioma::Es, &eleccion(), &capturas)
        .expect_err("un script no se pinta");
}
