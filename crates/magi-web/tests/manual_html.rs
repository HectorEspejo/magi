//! Pruebas de `magi_web::manual_html` (el pie común y lo que choca con la CSP) y del
//! tema de mdBook de `web/manual/`, contra su API pública y los ficheros reales.
//!
//! Sin red ni mdBook: lo que necesita mdBook de verdad está en `manual_mdbook.rs`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use magi_web::comprobar::{comprobar_sitio, ListaPermitida};
use magi_web::construir::{construir, OpcionesConstruir};
use magi_web::contraste::parsear_hex;
use magi_web::csp;
use magi_web::datos::Plataformas;
use magi_web::enlaces::{clasificar, Destino};
use magi_web::error::ErrorWeb;
use magi_web::html::{tokenizar, Token};
use magi_web::manual_html::{adaptar_fragmento, adaptar_pagina, pie_del_manual};
use magi_web::paginas::{contexto_comun, Entorno};
use magi_web::plantillas::Plantillas;
use magi_web::rutas::{Idioma, Pagina};
use magi_web::textos::Textos;

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("raíz del repositorio")
}

fn web() -> PathBuf {
    raiz().join("web")
}

fn fijos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fijos")
}

fn leer(ruta: &Path) -> String {
    fs::read_to_string(ruta).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", ruta.display()))
}

struct Mundo {
    plantillas: Plantillas,
    textos: Textos,
    plataformas: Plataformas,
}

fn mundo() -> Mundo {
    Mundo {
        plantillas: Plantillas::cargar(&web().join("plantillas")).expect("plantillas"),
        textos: Textos::cargar(&web().join("textos")).expect("textos"),
        plataformas: Plataformas::cargar(&web().join("datos/plataformas.toml")).expect("datos"),
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

/// Un pie de prueba, sencillo, para las pruebas que no son del pie real.
const PIE: &str = "<footer class=\"pie\"><p>PIE DE PRUEBA</p></footer>";

/// Una página con la forma de las de mdBook: `<head>` con su `<meta charset>`, la
/// página dentro de `<div class="page">`, `<main>` y, tras él, la navegación con su
/// `clear: both`; los scripts, tras la página.
fn pagina_de_mdbook(dentro_de_main: &str) -> String {
    format!(
        "<!DOCTYPE HTML>\n<html lang=\"es\">\n<head>\n<meta charset=\"UTF-8\">\n\
         <title>Prueba</title>\n</head>\n<body>\n<div class=\"page\">\n<main>\n{dentro_de_main}\n</main>\n\
         <nav class=\"nav-wrapper\">\n<a href=\"x.html\">x</a>\n<div style=\"clear: both\"></div>\n</nav>\n</div>\n\
         <script>window.a = 1;</script>\n</body>\n</html>\n"
    )
}

/// `DatoRaro { donde, que }` de un resultado que tiene que ser un error de dato raro.
fn dato_raro<T: std::fmt::Debug>(r: Result<T, ErrorWeb>) -> (String, String) {
    match r {
        Err(ErrorWeb::DatoRaro { donde, que }) => (donde, que),
        otro => panic!("se esperaba un dato raro y salió {otro:?}"),
    }
}

fn cuenta(texto: &str, trozo: &str) -> usize {
    texto.matches(trozo).count()
}

// ---------------------------------------------------------------------------
// adaptar_pagina
// ---------------------------------------------------------------------------

#[test]
fn adaptar_pagina_sustituye_el_clear_both_y_pone_el_pie_tras_main() {
    let entrada = pagina_de_mdbook("<h1>Título</h1>\n<p>Texto.</p>");
    let salida = adaptar_pagina(&entrada, PIE, "manual/x.html").unwrap();

    // Exactamente lo que cambia: el pie tras `</main>` y la clase en lugar del `style`.
    let esperada = entrada
        .replacen("</main>", &format!("</main>\n{PIE}"), 1)
        .replace(
            "<div style=\"clear: both\"></div>",
            "<div class=\"limpiar\"></div>",
        );
    assert_eq!(salida, esperada);
    assert!(!salida.contains("style="));
    assert_eq!(cuenta(&salida, "<footer class=\"pie\">"), 1);
    // El pie va justo después de `</main>`, antes de la navegación.
    let tras_main = &salida[salida.find("</main>").unwrap() + "</main>".len()..];
    assert!(tras_main.trim_start().starts_with(PIE), "{tras_main}");
    // Y sigue siendo una página que se lee entera.
    tokenizar(&salida).expect("la página adaptada se tiene que poder leer");
}

#[test]
fn el_pie_va_delante_si_el_clear_both_esta_pegado_a_main() {
    let entrada = "<html><body><main>x</main><div style=\"clear: both\"></div></body></html>";
    let salida = adaptar_pagina(entrada, PIE, "manual/x.html").unwrap();
    assert_eq!(
        salida,
        format!("<html><body><main>x</main>\n{PIE}<div class=\"limpiar\"></div></body></html>")
    );
}

#[test]
fn varios_clear_both_se_sustituyen_todos() {
    let entrada =
        "<main>x</main><div style=\"clear: both\"></div><p>a</p><div style=\"clear: both\"></div>";
    let salida = adaptar_pagina(entrada, PIE, "manual/x.html").unwrap();
    assert_eq!(cuenta(&salida, "<div class=\"limpiar\"></div>"), 2);
    assert!(!salida.contains("style"));
}

#[test]
fn el_texto_de_la_pagina_no_se_toca_salvo_lo_que_se_dice() {
    // Acentos, emoji y marcas raras: se cambia solo lo que se dice, sin cortar caracteres.
    let entrada = pagina_de_mdbook("<p>Túneles · ñandú 🚀 &lt;main&gt;</p>");
    let salida = adaptar_pagina(&entrada, PIE, "manual/x.html").unwrap();
    assert!(salida.contains("<p>Túneles · ñandú 🚀 &lt;main&gt;</p>"));
}

#[test]
fn adaptar_pagina_rechaza_otros_style() {
    let casos: [(&str, &str); 11] = [
        ("<p style=\"color: red\">x</p>", "p"),
        // Casi el de mdBook, pero no igual: se rechaza, no se «arregla».
        ("<div style=\"clear:both\"></div>", "div"),
        ("<div style=\"clear: both;\"></div>", "div"),
        ("<div style=\"clear: both\">hola</div>", "div"),
        ("<div style=\"clear: both\"> </div>", "div"),
        ("<div style=\"clear: both\" class=\"x\"></div>", "div"),
        ("<DIV STYLE=\"clear: both\"></DIV>", "div"),
        ("<div style='clear: both'></div>", "div"),
        ("<div style=\"clear: both\"/>", "div"),
        ("<svg style=\"width: 1em\"></svg>", "svg"),
        ("<span style>x</span>", "span"),
    ];
    for (trozo, etiqueta) in casos {
        let entrada = pagina_de_mdbook(trozo);
        let (donde, que) = dato_raro(adaptar_pagina(&entrada, PIE, "manual/tuneles.html"));
        assert!(
            donde.contains("manual/tuneles.html") && donde.contains("línea"),
            "{trozo}: debería decir la página y la línea: {donde}"
        );
        assert!(
            que.contains("style") && que.contains(&format!("<{etiqueta}>")),
            "{trozo}: {que}"
        );
    }
}

#[test]
fn un_elemento_style_tambien_se_rechaza() {
    let entrada = pagina_de_mdbook("<style>p { color: red }</style>");
    let (donde, que) = dato_raro(adaptar_pagina(&entrada, PIE, "manual/tuneles.html"));
    assert!(donde.contains("manual/tuneles.html"), "{donde}");
    assert!(que.contains("<style>"), "{que}");
}

#[test]
fn un_style_ajeno_falla_aunque_haya_un_clear_both_bueno() {
    let entrada = pagina_de_mdbook("<p style=\"x\">x</p>");
    assert!(entrada.contains("<div style=\"clear: both\"></div>"));
    dato_raro(adaptar_pagina(&entrada, PIE, "manual/x.html"));
}

#[test]
fn style_en_texto_comentarios_y_scripts_no_cuenta() {
    // Solo las etiquetas reales cuentan: el texto escapado, un comentario o el cuerpo
    // de un script que dicen `style="…"` no son un atributo.
    let entrada = pagina_de_mdbook(
        "<p>Escribe &lt;p style=&quot;x&quot;&gt; así.</p>\n<!-- <p style=\"a\"> -->\n\
         <pre><code>&lt;div style=\"clear: both\"&gt;&lt;/div&gt;</code></pre>\n\
         <script>var s = '<p style=\"x\">';</script>",
    );
    let salida = adaptar_pagina(&entrada, PIE, "manual/x.html").unwrap();
    assert!(salida.contains("<!-- <p style=\"a\"> -->"));
    assert!(salida.contains("<script>var s = '<p style=\"x\">';</script>"));
}

#[test]
fn adaptar_pagina_es_idempotente() {
    let entrada = pagina_de_mdbook("<h1>Título</h1>");
    let una = adaptar_pagina(&entrada, PIE, "manual/x.html").unwrap();
    let dos = adaptar_pagina(&una, PIE, "manual/x.html").unwrap();
    let tres = adaptar_pagina(&dos, PIE, "manual/x.html").unwrap();
    assert_eq!(una, dos);
    assert_eq!(dos, tres);
    assert_eq!(cuenta(&tres, "PIE DE PRUEBA"), 1);

    // También con el pie de verdad (que acaba en salto de línea) y con las páginas
    // de mdBook de los ejemplos del repositorio.
    let m = mundo();
    let pie = pie_del_manual(&m.entorno()).unwrap();
    for pagina in ["index.html", "instalacion.html", "tuneles.html"] {
        let html = leer(&fijos().join("manual-html").join(pagina));
        let una = adaptar_pagina(&html, &pie, pagina).unwrap();
        let dos = adaptar_pagina(&una, &pie, pagina).unwrap();
        assert_eq!(una, dos, "{pagina}");
        assert_eq!(cuenta(&dos, "<footer class=\"pie\">"), 1, "{pagina}");
    }
}

#[test]
fn otro_pie_de_la_web_no_se_acumula_con_el_nuevo() {
    // La página ya trae un pie y se pide otro distinto: dos pies es un error.
    let entrada = pagina_de_mdbook("<p>x</p>");
    let con_uno = adaptar_pagina(&entrada, PIE, "manual/x.html").unwrap();
    let otro = "<footer class=\"pie\"><p>OTRO PIE</p></footer>";
    let (donde, que) = dato_raro(adaptar_pagina(&con_uno, otro, "manual/x.html"));
    assert!(donde.contains("manual/x.html"), "{donde}");
    assert!(que.contains("pie"), "{que}");

    // Un pie fuera de su sitio (no tras `</main>`) tampoco se acepta.
    let mal_puesto = entrada.replace("<script>", &format!("{PIE}<script>"));
    dato_raro(adaptar_pagina(&mal_puesto, PIE, "manual/x.html"));
}

#[test]
fn adaptar_pagina_falla_sin_main_o_con_dos() {
    let sin_main = "<html><body><p>x</p></body></html>";
    let (donde, que) = dato_raro(adaptar_pagina(sin_main, PIE, "manual/sin.html"));
    assert!(donde.contains("manual/sin.html"), "{donde}");
    assert!(que.contains("</main>"), "{que}");

    let con_dos = "<main>a</main><main>b</main>";
    let (donde, que) = dato_raro(adaptar_pagina(con_dos, PIE, "manual/dos.html"));
    assert!(donde.contains("manual/dos.html"), "{donde}");
    assert!(que.contains("2 `</main>`"), "{que}");

    // Un `</main>` escrito dentro de un script, un comentario o un título no cuenta:
    // sin el de verdad, es «sin main»; con el de verdad, es uno solo.
    let solo_en_otros_sitios = "<html><head><title>a</title></head><body>\
        <script>var s = '</main>';</script><!-- </main> --></body></html>";
    dato_raro(adaptar_pagina(solo_en_otros_sitios, PIE, "manual/x.html"));

    let uno_y_falsos = "<body><script>var s = '</main>';</script><!-- </main> -->\
        <main>x</main></body>";
    let salida = adaptar_pagina(uno_y_falsos, PIE, "manual/x.html").unwrap();
    assert_eq!(cuenta(&salida, "PIE DE PRUEBA"), 1);
    assert!(
        salida.contains(&format!("<main>x</main>\n{PIE}</body>")),
        "{salida}"
    );

    // La caja no importa: `</MAIN>` es el mismo cierre.
    let en_mayusculas = "<MAIN>x</MAIN>";
    let salida = adaptar_pagina(en_mayusculas, PIE, "manual/x.html").unwrap();
    assert_eq!(salida, format!("<MAIN>x</MAIN>\n{PIE}"));
}

#[test]
fn adaptar_pagina_no_toca_los_scripts() {
    // Scripts con justo lo que se busca: el cierre de `main`, el `clear: both` y un
    // `style`. Ni cambian ni se cuentan.
    let script_1 = "window.a = '<div style=\"clear: both\"></div>'; // </main>";
    let script_2 = "const pie = `<footer class=\"pie\">`; let x = \"</main>\";";
    let entrada = format!(
        "<!DOCTYPE HTML>\n<html><head><meta charset=\"UTF-8\"><script>{script_1}</script></head>\n\
         <body><script>{script_2}</script>\n<main>x</main>\n\
         <div style=\"clear: both\"></div>\n<script src=\"a.js\"></script></body></html>\n"
    );
    let antes = csp::scripts_en_linea(&entrada).unwrap();
    assert_eq!(antes, vec![script_1.to_string(), script_2.to_string()]);

    let salida = adaptar_pagina(&entrada, PIE, "manual/x.html").unwrap();
    assert_eq!(csp::scripts_en_linea(&salida).unwrap(), antes);
    assert!(salida.contains(&format!("<script>{script_1}</script>")));
    assert!(salida.contains(&format!("<script>{script_2}</script>")));
    // El pie y la clase, en su sitio y una sola vez, y solo fuera de los scripts.
    assert_eq!(cuenta(&salida, "PIE DE PRUEBA"), 1);
    assert!(salida.contains(&format!("<main>x</main>\n{PIE}")));
    assert_eq!(cuenta(&salida, "<div class=\"limpiar\"></div>"), 1);
    assert_eq!(
        cuenta(&salida, "<div style=\"clear: both\"></div>"),
        1,
        "el del script sigue"
    );
    // Los hashes de la CSP no cambian por adaptar.
    let hashes = |html: &str| -> Vec<String> {
        csp::scripts_en_linea(html)
            .unwrap()
            .iter()
            .map(|s| csp::hash_script(s))
            .collect()
    };
    assert_eq!(hashes(&entrada), hashes(&salida));
}

#[test]
fn una_pagina_que_no_se_puede_leer_es_un_error_con_su_nombre_y_su_linea() {
    let rota = "<html>\n<body>\n<main>x</main>\n<div class=\"a\n";
    let (donde, _) = dato_raro(adaptar_pagina(rota, PIE, "manual/rota.html"));
    assert!(
        donde.contains("manual/rota.html") && donde.contains("línea 4"),
        "{donde}"
    );
}

#[test]
fn un_pie_que_no_vale_se_rechaza() {
    let entrada = pagina_de_mdbook("<p>x</p>");
    for malo in [
        "",
        "   \n",
        "<footer><script>alert(1)</script></footer>",
        "<footer style=\"color: red\">x</footer>",
        "<footer><style>a{}</style></footer>",
        "<footer class=\"pie\"",
    ] {
        let (donde, _) = dato_raro(adaptar_pagina(&entrada, malo, "manual/x.html"));
        assert!(donde.contains("pie"), "{malo:?}: {donde}");
    }
}

#[test]
fn el_origen_de_los_errores_no_cuela_controles() {
    let entrada = pagina_de_mdbook("<p style=\"x\">x</p>");
    let (donde, _) = dato_raro(adaptar_pagina(&entrada, PIE, "manual/a\u{1b}[31m\nb.html"));
    assert!(
        !donde.contains('\u{1b}') && !donde.contains('\n'),
        "{donde:?}"
    );
}

#[test]
fn adaptar_fragmento_no_pide_main_ni_pone_pie() {
    let toc = "<html><body class=\"sidebar-iframe-inner\"><ol><li>a</li></ol></body></html>";
    assert_eq!(adaptar_fragmento(toc, "manual/toc.html").unwrap(), toc);

    let con_clear = "<body><div style=\"clear: both\"></div></body>";
    assert_eq!(
        adaptar_fragmento(con_clear, "manual/toc.html").unwrap(),
        "<body><div class=\"limpiar\"></div></body>"
    );
    let (donde, _) = dato_raro(adaptar_fragmento(
        "<body><p style=\"x\">a</p></body>",
        "manual/toc.html",
    ));
    assert!(donde.contains("manual/toc.html"), "{donde}");
}

// ---------------------------------------------------------------------------
// El pie y la cabecera dicen lo mismo que los de la web
// ---------------------------------------------------------------------------

/// Un enlace de un fragmento de HTML, ya clasificado desde su página.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Enlace {
    texto: String,
    destino: Destino,
    hreflang: Option<String>,
    lang: Option<String>,
    title: Option<String>,
}

fn colapsar(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Los enlaces de `fragmento`, resueltos como si estuvieran en la página `desde`.
fn enlaces_de(fragmento: &str, desde: &str) -> Vec<Enlace> {
    let tokens = tokenizar(fragmento).expect("el fragmento se tiene que poder leer");
    let mut salida: Vec<Enlace> = Vec::new();
    let mut abierto = false;
    for token in &tokens {
        match token {
            Token::Etiqueta(e) if e.nombre == "a" && !e.cierre => {
                salida.push(Enlace {
                    texto: String::new(),
                    destino: clasificar(desde, e.atributo("href").unwrap_or("")),
                    hreflang: e.atributo("hreflang").map(str::to_string),
                    lang: e.atributo("lang").map(str::to_string),
                    title: e.atributo("title").map(str::to_string),
                });
                abierto = true;
            }
            Token::Etiqueta(e) if e.nombre == "a" && e.cierre => abierto = false,
            Token::Texto { inicio, fin } if abierto => {
                if let Some(enlace) = salida.last_mut() {
                    enlace.texto.push_str(&fragmento[*inicio..*fin]);
                }
            }
            _ => {}
        }
    }
    for enlace in &mut salida {
        enlace.texto = colapsar(&enlace.texto);
    }
    salida
}

/// Todo el texto de un fragmento, sin etiquetas y con los blancos colapsados.
fn texto_de(fragmento: &str) -> String {
    let tokens = tokenizar(fragmento).expect("el fragmento se tiene que poder leer");
    let mut texto = String::new();
    for token in &tokens {
        if let Token::Texto { inicio, fin } = token {
            texto.push_str(&fragmento[*inicio..*fin]);
            texto.push(' ');
        }
    }
    colapsar(&texto)
}

/// El `<footer class="pie">…</footer>` de una página.
fn pie_de(html: &str) -> String {
    let inicio = html
        .find("<footer class=\"pie\">")
        .expect("la página no tiene pie");
    let fin = html[inicio..].find("</footer>").expect("pie sin cerrar") + "</footer>".len();
    html[inicio..inicio + fin].to_string()
}

/// El sitio de las páginas propias, construido con la `web/` real y el manual de
/// ejemplo (el pie de las páginas propias no depende del manual).
fn sitio_con_manual_de_ejemplo() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    construir(&opciones(
        fijos().join("manual-html"),
        &t.path().join("sitio"),
    ))
    .expect("construir");
    t
}

fn opciones(manual: PathBuf, salida: &Path) -> OpcionesConstruir {
    OpcionesConstruir {
        web: web(),
        releases: fijos().join("releases/sin-red.json"),
        manual,
        animacion: None,
        changelog_escritorio: raiz().join("CHANGELOG.md"),
        changelog_app: None,
        salida: salida.to_path_buf(),
        homebrew: false,
        aur: false,
    }
}

#[test]
fn el_pie_del_manual_dice_lo_mismo_que_el_de_la_web() {
    let m = mundo();
    let pie_manual = pie_del_manual(&m.entorno()).unwrap();
    let sitio = sitio_con_manual_de_ejemplo();
    let raiz_sitio = sitio.path().join("sitio");

    // Contra el de las páginas de verdad: la portada y las descargas, ya construidas.
    for pagina in ["index.html", "descargas/index.html", "versiones/index.html"] {
        let pie_web = pie_de(&leer(&raiz_sitio.join(pagina)));
        assert_eq!(
            texto_de(&pie_manual),
            texto_de(&pie_web),
            "el texto del pie no es el de {pagina}"
        );
        // Los mismos destinos, cada uno resuelto desde su página: el del manual sale
        // relativo desde `manual/`, el de la web desde la suya, y llegan al mismo sitio.
        let del_manual = enlaces_de(&pie_manual, "manual/tuneles.html");
        let de_la_web = enlaces_de(&pie_web, pagina);
        assert_eq!(
            del_manual, de_la_web,
            "los enlaces del pie no son los de {pagina}"
        );
    }

    // Y se nota que es el de verdad: sus piezas, con su destino.
    let enlaces = enlaces_de(&pie_manual, "manual/tuneles.html");
    let destinos: Vec<(&str, &Destino)> = enlaces
        .iter()
        .map(|e| (e.texto.as_str(), &e.destino))
        .collect();
    assert_eq!(
        destinos
            .iter()
            .find(|(t, _)| t.starts_with("Manual de la"))
            .map(|(t, d)| (*t, (*d).clone())),
        Some((
            "Manual de la 1.6.1",
            Destino::Interno {
                ruta: "manual/index.html".to_string(),
                ancla: None
            }
        ))
    );
    assert!(enlaces.iter().any(|e| e.texto == "MIT"));
    assert!(enlaces.iter().any(|e| e.texto == "GitHub"));
    assert!(enlaces.iter().any(|e| e.texto == "Seguridad"));
    // Lo interno sale relativo desde una carpeta (`../`), nunca absoluto.
    assert!(pie_manual.contains("href=\"../manual/\""), "{pie_manual}");
    assert!(!pie_manual.contains("href=\"manual/\""), "{pie_manual}");
    assert!(!pie_manual.contains("magi.erratia.com"), "{pie_manual}");
    // La nota de privacidad dice lo del navegador y la barra lateral (D453).
    assert!(texto_de(&pie_manual)
        .contains("el manual guarda en tu navegador el estado de su barra lateral"));
}

#[test]
fn el_pie_del_manual_sale_en_castellano_y_ya_escapado() {
    let m = mundo();
    let pie = pie_del_manual(&m.entorno()).unwrap();
    assert!(pie.contains("<footer class=\"pie\">") && pie.contains("hreflang=\"es\""));
    assert!(
        pie.contains("blob/main/CONTRIBUTING.md\""),
        "el de castellano"
    );
    assert!(!pie.contains("CONTRIBUTING.en.md"));
    assert!(!pie.contains("{{") && !pie.contains("{%"));

    // Un texto con HTML no se cuela: sale escapado.
    let es = leer(&web().join("textos/es.toml")).replace(
        "manual_de = \"Manual de la\"",
        "manual_de = \"<script>alert(1)</script> \\\"Manual\\\" de la\"",
    );
    assert!(
        es.contains("<script>alert(1)</script>"),
        "la prueba no cambió el texto"
    );
    let en = leer(&web().join("textos/en.toml"));
    let textos = Textos::desde_textos(&es, "es.toml", &en, "en.toml").unwrap();
    let entorno = Entorno {
        plantillas: &m.plantillas,
        textos: &textos,
        plataformas: &m.plataformas,
        version_manual: "<b>1.6.1</b>",
    };
    let pie = pie_del_manual(&entorno).unwrap();
    assert!(!pie.contains("<script"), "{pie}");
    assert!(!pie.contains("<b>"), "{pie}");
    assert!(
        pie.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{pie}"
    );
    assert!(pie.contains("&lt;b&gt;1.6.1&lt;/b&gt;"), "{pie}");
    tokenizar(&pie).unwrap();
}

/// `{{ …}}` de handlebars en el texto de `header.hbs`: cada expresión, sin llaves.
fn expresiones_de_handlebars(hbs: &str) -> Vec<String> {
    let mut salida = Vec::new();
    let mut resto = hbs;
    while let Some(i) = resto.find("{{") {
        let tras = &resto[i + 2..];
        let fin = tras.find("}}").expect("expresión de handlebars sin cerrar");
        salida.push(tras[..fin].trim().to_string());
        resto = &tras[fin + 2..];
    }
    salida
}

/// `header.hbs` con `{{ path_to_root }}` sustituido por `prefijo`.
fn cabecera_del_manual(prefijo: &str) -> String {
    let hbs = leer(&web().join("manual/theme/header.hbs"));
    // Lo único que lleva handlebars es la ruta a la raíz: nada más puede colarse.
    for expresion in expresiones_de_handlebars(&hbs) {
        assert_eq!(
            expresion, "path_to_root",
            "header.hbs solo puede llevar path_to_root"
        );
    }
    let mut salida = String::new();
    let mut resto = hbs.as_str();
    while let Some(i) = resto.find("{{") {
        salida.push_str(&resto[..i]);
        let fin = resto[i..].find("}}").unwrap() + i + 2;
        salida.push_str(prefijo);
        resto = &resto[fin..];
    }
    salida.push_str(resto);
    salida
}

#[test]
fn la_cabecera_estatica_dice_lo_mismo_que_la_comun() {
    let m = mundo();
    let e = m.entorno();
    // La común, la de la portada en castellano.
    let contexto = contexto_comun(&e, Pagina::Portada, Idioma::Es);
    let comun = m
        .plantillas
        .renderizar("parciales/cabecera.html", &contexto)
        .unwrap();
    let enlaces_comunes = enlaces_de(&comun, "index.html");
    assert_eq!(
        enlaces_comunes.len(),
        6,
        "marca, cuatro del menú e idioma: {enlaces_comunes:?}"
    );

    // La estática, con `path_to_root` de una página directa del manual (`""`) y de una
    // dentro de una subcarpeta (`"../"`): los destinos son los mismos que los de la común.
    for (prefijo, desde) in [
        ("", "manual/tuneles.html"),
        ("../", "manual/sub/tuneles.html"),
    ] {
        let estatica = cabecera_del_manual(prefijo);
        let enlaces_estaticos = enlaces_de(&estatica, desde);
        // El aviso de idioma y el enlace de salto son lo único extra; no son enlaces
        // de la cabecera (el salto va fuera de ella y el aviso no es un enlace).
        let sin_salto: Vec<Enlace> = enlaces_estaticos
            .iter()
            .filter(|e| !matches!(&e.destino, Destino::Ancla(_)))
            .cloned()
            .collect();
        assert_eq!(
            sin_salto, enlaces_comunes,
            "la cabecera estática (path_to_root = {prefijo:?}) no dice lo mismo que la común"
        );
    }

    // El salto al contenido lleva el texto de `comun.saltar` y va a lo que mdBook
    // tiene como contenido.
    let estatica = cabecera_del_manual("");
    let salto: Vec<Enlace> = enlaces_de(&estatica, "manual/tuneles.html")
        .into_iter()
        .filter(|e| matches!(&e.destino, Destino::Ancla(_)))
        .collect();
    assert_eq!(salto.len(), 1);
    assert_eq!(
        salto[0].texto,
        m.textos.texto(Idioma::Es, "comun.saltar").unwrap()
    );
    assert_eq!(
        salto[0].destino,
        Destino::Ancla("mdbook-content".to_string())
    );

    // El aviso: una vez, con el texto de `comun.manual_idioma`, en su clase propia.
    let aviso = m.textos.texto(Idioma::Es, "comun.manual_idioma").unwrap();
    assert_eq!(aviso, "Solo en castellano · Spanish only");
    assert_eq!(cuenta(&estatica, "class=\"aviso-idioma\""), 1);
    // La parte en inglés lleva su `lang` (WCAG 3.1.2, AA): la página es `lang="es"`.
    assert!(
        estatica.contains(&format!(
            "<p class=\"aviso-idioma\">{}</p>",
            aviso.replace("Spanish only", "<span lang=\"en\">Spanish only</span>")
        )),
        "{estatica}"
    );
    assert!(
        !comun.contains("aviso-idioma"),
        "el aviso es lo único extra"
    );

    // El menú: el mismo nombre accesible y los mismos textos que el común.
    for html in [&comun, &estatica] {
        assert!(html.contains("aria-label=\"Navegación principal\""));
    }
    assert_eq!(
        texto_de(&comun).replace(aviso, ""),
        texto_de(&estatica)
            .replace(aviso, "")
            .replace(&texto_del_salto(&m), "")
            .trim()
            .to_string()
    );

    // Solo «Manual» está marcado como actual (en el manual, el enlace va al índice).
    assert_eq!(cuenta(&estatica, "aria-current"), 1);
    assert!(
        estatica.contains("hreflang=\"es\" aria-current=\"true\">Manual</a>"),
        "{estatica}"
    );
}

fn texto_del_salto(m: &Mundo) -> String {
    m.textos
        .texto(Idioma::Es, "comun.saltar")
        .unwrap()
        .to_string()
}

// ---------------------------------------------------------------------------
// Paleta y CSS del tema
// ---------------------------------------------------------------------------

/// Una regla de CSS: su `@media` (si lo tiene), sus selectores y sus declaraciones.
#[derive(Debug)]
struct Regla {
    media: Option<String>,
    selectores: Vec<String>,
    declaraciones: BTreeMap<String, String>,
}

fn sin_comentarios(css: &str) -> String {
    let mut salida = String::new();
    let mut resto = css;
    while let Some(i) = resto.find("/*") {
        salida.push_str(&resto[..i]);
        match resto[i + 2..].find("*/") {
            Some(fin) => resto = &resto[i + 2 + fin + 2..],
            None => return salida,
        }
    }
    salida.push_str(resto);
    salida
}

/// Contenido de las llaves que empiezan tras `css` (ya sin la `{` de apertura) y lo que sigue.
fn bloque(css: &str) -> (&str, &str) {
    let mut profundidad = 1;
    for (i, c) in css.char_indices() {
        match c {
            '{' => profundidad += 1,
            '}' => {
                profundidad -= 1;
                if profundidad == 0 {
                    return (&css[..i], &css[i + 1..]);
                }
            }
            _ => {}
        }
    }
    panic!("llaves sin cerrar");
}

fn reglas(css: &str) -> Vec<Regla> {
    fn analizar(css: &str, media: Option<&str>, salida: &mut Vec<Regla>) {
        let mut resto = css;
        while let Some(i) = resto.find('{') {
            let cabecera = resto[..i].trim();
            let (cuerpo, tras) = bloque(&resto[i + 1..]);
            if cabecera.starts_with("@media") {
                analizar(cuerpo, Some(cabecera), salida);
            } else if !cabecera.starts_with('@') {
                let mut declaraciones = BTreeMap::new();
                for declaracion in cuerpo.split(';') {
                    if let Some((nombre, valor)) = declaracion.split_once(':') {
                        declaraciones.insert(nombre.trim().to_string(), valor.trim().to_string());
                    }
                }
                salida.push(Regla {
                    media: media.map(str::to_string),
                    selectores: cabecera.split(',').map(|s| s.trim().to_string()).collect(),
                    declaraciones,
                });
            }
            resto = tras;
        }
    }
    let mut salida = Vec::new();
    analizar(&sin_comentarios(css), None, &mut salida);
    salida
}

const TOKENS: [&str; 10] = [
    "--fondo",
    "--superficie",
    "--texto",
    "--secundario",
    "--acento",
    "--correcto",
    "--critico",
    "--foco",
    "--borde-control",
    "--borde-decorativo",
];

const MEDIA_OSCURO: &str = "@media (prefers-color-scheme: dark)";

fn regla_con<'a>(reglas: &'a [Regla], media: Option<&str>, selector: &str) -> &'a Regla {
    reglas
        .iter()
        .find(|r| {
            r.media.as_deref() == media
                && r.selectores.iter().any(|s| s == selector)
                && r.declaraciones.contains_key("--fondo")
        })
        .unwrap_or_else(|| panic!("no hay una regla con `{selector}` ({media:?}) y la paleta"))
}

fn tokens_iguales(a: &Regla, b: &Regla, que: &str) {
    for token in TOKENS {
        let valor = |r: &Regla| {
            let texto = r
                .declaraciones
                .get(token)
                .unwrap_or_else(|| panic!("{que}: falta {token}"));
            parsear_hex(texto)
                .unwrap_or_else(|| panic!("{que}: {token} = «{texto}» no es un color"))
        };
        assert_eq!(valor(a), valor(b), "{que}: {token} no coincide");
    }
}

#[test]
fn la_paleta_del_manual_es_la_de_la_web() {
    let de_la_web = reglas(&leer(&web().join("estilos/web.css")));
    let del_manual = reglas(&leer(&web().join("manual/theme/css/variables.css")));

    let web_clara = regla_con(&de_la_web, None, ":root");
    let web_oscura = regla_con(&de_la_web, Some(MEDIA_OSCURO), ":root");

    // Las cinco clases de tema de mdBook, a las dos paletas.
    let clara = regla_con(&del_manual, None, ".light");
    let oscura = regla_con(&del_manual, None, ".coal");
    assert_eq!(clara.selectores, [".light", ".rust", "html:not(.js)"]);
    assert_eq!(oscura.selectores, [".coal", ".navy", ".ayu"]);
    tokens_iguales(
        clara,
        web_clara,
        "paleta clara del manual contra la de la web",
    );
    tokens_iguales(
        oscura,
        web_oscura,
        "paleta oscura del manual contra la de la web",
    );

    // Sin JavaScript no hay clase de tema: manda el sistema, con la oscura.
    let sin_js = regla_con(&del_manual, Some(MEDIA_OSCURO), "html:not(.js)");
    assert_eq!(sin_js.selectores, ["html:not(.js)"]);
    tokens_iguales(
        sin_js,
        web_oscura,
        "paleta sin JavaScript del manual contra la de la web",
    );

    // Cada paleta dice qué esquema de color es.
    assert_eq!(clara.declaraciones["--color-scheme"], "light");
    assert_eq!(oscura.declaraciones["--color-scheme"], "dark");
    assert_eq!(sin_js.declaraciones["--color-scheme"], "dark");
}

#[test]
fn en_el_manual_todo_color_sale_de_un_token_de_la_paleta() {
    let del_manual = reglas(&leer(&web().join("manual/theme/css/variables.css")));
    // La regla de `:root` con lo que lee mdBook: todo es `var(--token)` de la paleta,
    // salvo lo que no es un color (el filtro del botón de copiar, que no hay) y el
    // velo de la ayuda de teclas, que es el fondo oscuro de la paleta con alfa.
    let mapeo = del_manual
        .iter()
        .find(|r| r.media.is_none() && r.declaraciones.contains_key("--bg"))
        .expect("falta el mapeo de las variables de mdBook");
    let mut usados = 0;
    for (nombre, valor) in &mapeo.declaraciones {
        match nombre.as_str() {
            "--copy-button-filter" | "--copy-button-filter-hover" => assert_eq!(valor, "none"),
            "--overlay-bg" => {
                assert_eq!(valor, "rgba(10, 10, 10, 0.6)", "el fondo oscuro con alfa")
            }
            _ => {
                let token = valor
                    .strip_prefix("var(")
                    .and_then(|v| v.strip_suffix(')'))
                    .unwrap_or_else(|| panic!("{nombre} = «{valor}» no sale de un token"));
                assert!(
                    TOKENS.contains(&token),
                    "{nombre} usa {token}, que no es de la paleta"
                );
                usados += 1;
            }
        }
    }
    assert!(
        usados > 30,
        "el mapeo parece incompleto: {usados} variables"
    );
    // Y fuera de las tres reglas de la paleta, ningún valor hexadecimal suelto.
    for regla in &del_manual {
        if regla.declaraciones.contains_key("--fondo") {
            continue;
        }
        for (nombre, valor) in &regla.declaraciones {
            assert!(
                !valor.contains('#'),
                "{nombre} = «{valor}»: un color fuera de la paleta"
            );
        }
    }
    // La paleta del mapeo cubre lo que lee mdBook: la oscura se ve en sus tres temas.
    let paletas = del_manual
        .iter()
        .filter(|r| r.declaraciones.contains_key("--fondo"))
        .count();
    assert_eq!(paletas, 3, "clara, oscura y la de sin JavaScript");
}

#[test]
fn el_css_del_tema_no_trae_colores_ni_recursos_de_fuera() {
    let css = sin_comentarios(&leer(&web().join("manual/theme/magi.css")));
    assert!(!css.contains("@import"));
    assert!(!css.contains("url("), "el tema no carga nada: sin url()");
    assert!(!css.contains("http://") && !css.contains("https://") && !css.contains("//"));
    for color in ["rgb(", "rgba(", "hsl(", "hsla(", "color-mix(", "oklch("] {
        assert!(
            !css.contains(color),
            "magi.css pone un color propio: {color}"
        );
    }
    let mut resto = css.as_str();
    while let Some(i) = resto.find('#') {
        let tras = &resto[i + 1..];
        let hex = tras.bytes().take_while(u8::is_ascii_hexdigit).count();
        // `#mdbook-menu-bar` es un id; `#F4F0E6` sería un color. Un id empieza por una
        // letra que no es de un número hexadecimal de 3, 4, 6 u 8 cifras seguidas.
        let sigue = tras.as_bytes().get(hex).copied().unwrap_or(b' ');
        let es_color = matches!(hex, 3 | 4 | 6 | 8)
            && !(sigue.is_ascii_alphanumeric() || sigue == b'-' || sigue == b'_');
        assert!(
            !es_color,
            "magi.css pone un color propio: #{}",
            &tras[..hex.min(8)]
        );
        resto = tras;
    }

    // Cada `var(--x)` que usa está definida: en la paleta o en el mapeo de mdBook.
    let variables = leer(&web().join("manual/theme/css/variables.css"));
    let definidas: Vec<String> = reglas(&variables)
        .iter()
        .flat_map(|r| r.declaraciones.keys().cloned().collect::<Vec<_>>())
        .collect();
    let web_css = sin_comentarios(&leer(&web().join("estilos/web.css")));
    let mut resto = css.as_str();
    while let Some(i) = resto.find("var(--") {
        let tras = &resto[i + 4..];
        let fin = tras.find(')').expect("var( sin cerrar");
        let nombre = tras[..fin].split(',').next().unwrap().trim();
        assert!(
            definidas.iter().any(|d| d == nombre) || web_css.contains(&format!("{nombre}:")),
            "magi.css usa var({nombre}), que ninguna hoja define"
        );
        resto = &tras[fin..];
    }
}

#[test]
fn el_tema_oculta_el_selector_de_temas_y_define_limpiar() {
    let reglas_del_tema = reglas(&leer(&web().join("manual/theme/magi.css")));
    let oculta = reglas_del_tema
        .iter()
        .find(|r| r.selectores.iter().any(|s| s == "#mdbook-theme-toggle"))
        .expect("falta la regla que oculta el selector de temas");
    assert!(oculta.selectores.iter().any(|s| s == "#mdbook-theme-list"));
    assert_eq!(oculta.declaraciones["display"], "none !important");

    let limpiar = reglas_del_tema
        .iter()
        .find(|r| r.selectores == [".limpiar"])
        .expect("falta .limpiar");
    assert_eq!(limpiar.declaraciones["clear"], "both");

    // El foco del manual no se quita en ninguna regla del tema.
    for regla in &reglas_del_tema {
        let quita = regla
            .declaraciones
            .get("outline")
            .is_some_and(|v| v == "none" || v == "0");
        assert!(!quita, "el tema quita el foco en {:?}", regla.selectores);
    }
}

#[test]
fn el_book_toml_dice_lo_decidido() {
    let texto = leer(&web().join("manual/book.toml"));
    let toml: toml::Table = texto.parse().expect("book.toml es TOML");
    let libro = toml["book"].as_table().unwrap();
    assert_eq!(libro["title"].as_str(), Some("Manual de MAGI"));
    assert_eq!(libro["language"].as_str(), Some("es"));
    assert_eq!(libro["src"].as_str(), Some("src"));
    assert_eq!(toml["build"]["create-missing"].as_bool(), Some(false));

    let html = toml["output"]["html"].as_table().unwrap();
    assert_eq!(html["default-theme"].as_str(), Some("light"));
    assert_eq!(html["preferred-dark-theme"].as_str(), Some("coal"));
    assert_eq!(html["no-section-label"].as_bool(), Some(true));
    assert_eq!(html["input-404"].as_str(), Some(""));
    assert_eq!(
        html["additional-css"].as_array().unwrap(),
        &vec![toml::Value::String("theme/magi.css".to_string())]
    );
    // Nada de enlaces a fuera (GitHub, «editar esta página»).
    for clave in [
        "git-repository-url",
        "edit-url-template",
        "site-url",
        "cname",
        "mathjax-support",
    ] {
        assert!(
            !html.contains_key(clave),
            "book.toml no debe definir {clave}"
        );
    }
    assert_eq!(html["search"]["enable"].as_bool(), Some(true));
    assert_eq!(html["playground"]["copyable"].as_bool(), Some(false));
    assert_eq!(html["playground"]["runnable"].as_bool(), Some(false));
    assert_eq!(html["print"]["enable"].as_bool(), Some(false));
    // Sin preprocesadores propios: se dejan los de serie.
    assert!(toml["build"]
        .as_table()
        .unwrap()
        .keys()
        .all(|k| k == "create-missing"));
    assert!(!toml.contains_key("preprocessor"));
}

#[test]
fn head_hbs_enlaza_la_hoja_comun_y_nada_de_fuera() {
    let hbs = leer(&web().join("manual/theme/head.hbs"));
    for expresion in expresiones_de_handlebars(&hbs) {
        assert_eq!(expresion, "path_to_root");
    }
    let sin_expresiones = hbs.replace("{{ path_to_root }}", "");
    let tokens = tokenizar(&sin_expresiones).expect("head.hbs se tiene que poder leer");
    let mut destinos = Vec::new();
    for token in &tokens {
        if let Token::Etiqueta(e) = token {
            assert!(
                e.nombre == "link",
                "head.hbs solo lleva <link>, no <{}>",
                e.nombre
            );
            assert!(!e.tiene_atributo("style"));
            destinos.push(e.atributo("href").unwrap().to_string());
        }
    }
    // Desde `manual/` la web está un nivel más arriba; sin barra inicial ni esquema.
    assert_eq!(destinos, ["../favicon.svg", "../estilos/web.css"]);
}

// ---------------------------------------------------------------------------
// El manual construido
// ---------------------------------------------------------------------------

fn paginas_html(dir: &Path) -> Vec<PathBuf> {
    let mut salida = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let ruta = e.path();
        if ruta.is_dir() {
            salida.extend(paginas_html(&ruta));
        } else if ruta.extension().is_some_and(|x| x == "html") {
            salida.push(ruta);
        }
    }
    salida.sort();
    salida
}

#[test]
fn el_manual_construido_lleva_pie_y_csp() {
    let m = mundo();
    let pie = pie_del_manual(&m.entorno()).unwrap();
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    let informe = construir(&opciones(fijos().join("manual-html"), &salida)).unwrap();

    let manual = salida.join("manual");
    let paginas = paginas_html(&manual);
    assert!(paginas.len() >= 4, "{paginas:?}");
    for ruta in paginas {
        let nombre = ruta.strip_prefix(&manual).unwrap().display().to_string();
        let html = leer(&ruta);
        assert!(
            informe
                .paginas
                .iter()
                .any(|p| *p == format!("manual/{nombre}")),
            "{nombre} no está en el informe"
        );

        // La CSP: con el hash de cada script en línea y sin `unsafe-inline`.
        let politica = csp::extraer(&html)
            .unwrap()
            .unwrap_or_else(|| panic!("{nombre} sin CSP"));
        assert!(
            politica.contains("script-src 'self'"),
            "{nombre}: {politica}"
        );
        assert!(!politica.contains("unsafe-inline"), "{nombre}: {politica}");
        let scripts = csp::scripts_en_linea(&html).unwrap();
        for script in &scripts {
            assert!(
                politica.contains(&csp::hash_script(script)),
                "{nombre}: falta el hash de un script en línea"
            );
        }
        // Un script sin hash en la política no puede haberse colado: la política lleva
        // exactamente los hashes de los scripts en línea de la página.
        assert_eq!(
            politica.matches("'sha256-").count(),
            scripts
                .iter()
                .map(|s| csp::hash_script(s))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            "{nombre}: {politica}"
        );
        // Nada de `style` ni del `clear: both` de mdBook.
        assert!(
            !html.contains(" style="),
            "{nombre} lleva un atributo style"
        );
        assert!(
            !html.contains("clear: both"),
            "{nombre} conserva el clear: both"
        );

        if nombre == "toc.html" {
            // El relleno de la barra lateral sin JavaScript no es una página.
            assert!(!html.contains("<footer"), "toc.html no lleva pie");
        } else {
            assert_eq!(cuenta(&html, "<footer class=\"pie\">"), 1, "{nombre}");
            let tras_main = &html[html.find("</main>").unwrap() + "</main>".len()..];
            assert!(
                tras_main.trim_start().starts_with(pie.trim()),
                "{nombre}: el pie no va justo tras </main>"
            );
        }
    }

    // El pie de las páginas del manual es el de `pie_del_manual`, byte a byte.
    let tuneles = leer(&manual.join("tuneles.html"));
    assert_eq!(pie_de(&tuneles), pie.trim());

    // Y la comprobación del sitio no encuentra nada que ver con el estilo, los scripts
    // o la CSP del manual (los enlaces del ejemplo son cosa de otras pruebas).
    let hallazgos = comprobar_sitio(&salida, &ListaPermitida::por_defecto()).unwrap();
    let del_manual: Vec<String> = hallazgos
        .iter()
        .map(ToString::to_string)
        .filter(|h| {
            let h = h.to_lowercase();
            h.contains("style") || h.contains("csp") || h.contains("script") || h.contains("pie")
        })
        .collect();
    assert!(del_manual.is_empty(), "{del_manual:?}");
}
