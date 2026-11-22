//! Pruebas de `magi-web comprobar` (`magi_web::comprobar`) contra su API pública.
//!
//! Los sitios de prueba son carpetas temporales con unas pocas páginas, cada una
//! con un solo defecto (así un fallo dice qué hallazgo falta), más los sitios de
//! `tests/fijos/`: `sitio-bueno` (no debe dar nada) y `sitio-malo` (un defecto
//! por página). Todo lo que se escribe en ellos son datos inventados.

use std::fs;
use std::path::{Path, PathBuf};

use magi_web::comprobar::{comprobar_sitio, ListaPermitida};
use magi_web::csp::{self, insertar};
use magi_web::error::{ErrorWeb, Hallazgo};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn fijos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fijos")
}

/// Una página propia correcta, con su CSP puesta como lo hace la construcción.
fn pagina_propia(cabeza: &str, cuerpo: &str) -> String {
    let crudo = format!(
        "<!DOCTYPE html>\n<html lang=\"es\">\n<head>\n<meta charset=\"utf-8\">\n<title>Prueba</title>\n{cabeza}</head>\n<body>\n{cuerpo}\n</body>\n</html>\n"
    );
    insertar(&crudo, false).unwrap()
}

/// Lo mismo, con la CSP del manual (con los hashes de sus scripts en línea).
fn pagina_manual(cabeza: &str, cuerpo: &str) -> String {
    let crudo = format!(
        "<!DOCTYPE html>\n<html lang=\"es\">\n<head>\n<meta charset=\"utf-8\">\n<title>Prueba</title>\n{cabeza}</head>\n<body>\n{cuerpo}\n</body>\n</html>\n"
    );
    insertar(&crudo, true).unwrap()
}

/// Escribe `ficheros` (ruta, contenido) en una carpeta temporal.
fn sitio(ficheros: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (ruta, contenido) in ficheros {
        escribir(dir.path(), ruta, contenido.as_bytes());
    }
    dir
}

fn escribir(dir: &Path, ruta: &str, contenido: &[u8]) {
    let destino = dir.join(ruta);
    fs::create_dir_all(destino.parent().unwrap()).unwrap();
    fs::write(destino, contenido).unwrap();
}

fn comprobar(dir: &Path) -> Vec<Hallazgo> {
    comprobar_sitio(dir, &ListaPermitida::por_defecto()).expect("el sitio debe poder recorrerse")
}

/// Hallazgos de un fichero cuyo texto contiene `trozo`.
fn hallazgos_con<'a>(h: &'a [Hallazgo], fichero: &str, trozo: &str) -> Vec<&'a Hallazgo> {
    h.iter()
        .filter(|x| x.fichero == fichero && x.que.contains(trozo))
        .collect()
}

#[track_caller]
fn debe_haber(h: &[Hallazgo], fichero: &str, trozo: &str) {
    assert!(
        !hallazgos_con(h, fichero, trozo).is_empty(),
        "falta un hallazgo en {fichero} con «{trozo}»; salieron:\n{}",
        listar(h)
    );
}

fn listar(h: &[Hallazgo]) -> String {
    if h.is_empty() {
        return "  (ninguno)".to_string();
    }
    h.iter()
        .map(|x| format!("  - {x}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn de_fichero<'a>(h: &'a [Hallazgo], fichero: &str) -> Vec<&'a Hallazgo> {
    h.iter().filter(|x| x.fichero == fichero).collect()
}

fn copiar_arbol(origen: &Path, destino: &Path) {
    fs::create_dir_all(destino).unwrap();
    for e in fs::read_dir(origen).unwrap() {
        let e = e.unwrap();
        let objetivo = destino.join(e.file_name());
        if e.path().is_dir() {
            copiar_arbol(&e.path(), &objetivo);
        } else {
            fs::copy(e.path(), objetivo).unwrap();
        }
    }
}

// ---------------------------------------------------------------------------
// Sitios de ejemplo
// ---------------------------------------------------------------------------

#[test]
fn un_sitio_bueno_pasa_sin_hallazgos() {
    let h = comprobar(&fijos().join("sitio-bueno"));
    assert!(
        h.is_empty(),
        "el sitio bueno no debe dar hallazgos:\n{}",
        listar(&h)
    );
}

#[test]
fn el_sitio_malo_da_un_hallazgo_por_cada_defecto() {
    let h = comprobar(&fijos().join("sitio-malo"));
    let cuenta = |f: &str| de_fichero(&h, f).len();

    // La página sana y las imágenes no dan nada.
    assert_eq!(cuenta("index.html"), 0, "{}", listar(&h));
    assert_eq!(cuenta("capturas/ok.svg"), 0);

    // enlaces/: roto, dos anclas, imagen rota, ruta absoluta, subida por encima de la raíz y `%2F`.
    debe_haber(&h, "enlaces/index.html", "enlace roto: «no-existe.html»");
    debe_haber(&h, "enlaces/index.html", "ancla rota: «#fantasma»");
    debe_haber(
        &h,
        "enlaces/index.html",
        "ancla rota: «../index.html#no-esta»",
    );
    debe_haber(
        &h,
        "enlaces/index.html",
        "enlace roto: «../capturas/falta.svg»",
    );
    debe_haber(
        &h,
        "enlaces/index.html",
        "las rutas absolutas del sitio solo se usan en 404.html",
    );
    debe_haber(
        &h,
        "enlaces/index.html",
        "enlace no admitido: «../../../etc/passwd»",
    );
    debe_haber(&h, "enlaces/index.html", "enlace no admitido: «a%2Fb.html»");
    assert_eq!(cuenta("enlaces/index.html"), 7, "{}", listar(&h));

    // externos/
    for trozo in [
        "«https://evil.ejemplo.org/x»",
        "«https://github.com.evil.ejemplo.org/HectorEspejo/magi»",
        "«https://github.com/HectorEspejo/magi-evil»",
        "«https://github.com/HectorEspejo/magi/../../evil/x»",
    ] {
        debe_haber(
            &h,
            "externos/index.html",
            &format!("que no es de la lista permitida (o con formato raro): {trozo}"),
        );
    }
    for trozo in [
        "«http://github.com/HectorEspejo/magi»",
        "«javascript:alert(1)»",
        "«data:text/html,x»",
        "«//evil.ejemplo.org/x»",
        "«mailto:alguien@ejemplo.org»",
    ] {
        debe_haber(
            &h,
            "externos/index.html",
            &format!("enlace no admitido: {trozo}"),
        );
    }
    debe_haber(&h, "externos/index.html", "recurso cargado de fuera");
    assert_eq!(cuenta("externos/index.html"), 10, "{}", listar(&h));

    // scripts/
    assert_eq!(
        hallazgos_con(
            &h,
            "scripts/index.html",
            "<script> en una página que no es del manual"
        )
        .len(),
        3,
        "{}",
        listar(&h)
    );
    debe_haber(&h, "scripts/index.html", "atributo `onclick`");
    debe_haber(&h, "scripts/index.html", "atributo `style`");
    debe_haber(&h, "scripts/index.html", "<style> en línea");
    assert_eq!(cuenta("scripts/index.html"), 6, "{}", listar(&h));

    // cabecera/
    debe_haber(&h, "cabecera/index.html", "<base> no permitido");
    debe_haber(
        &h,
        "cabecera/index.html",
        "<meta http-equiv=\"refresh\"> no permitido",
    );
    assert_eq!(cuenta("cabecera/index.html"), 3, "{}", listar(&h));

    // CSP
    debe_haber(&h, "csp-mala/index.html", "'unsafe-inline'");
    debe_haber(&h, "csp-mala/index.html", "'unsafe-eval'");
    debe_haber(&h, "csp-mala/index.html", "`script-src` debe ser `'none'`");
    assert_eq!(cuenta("csp-mala/index.html"), 3, "{}", listar(&h));
    debe_haber(&h, "sin-csp/index.html", "no lleva CSP");
    assert_eq!(cuenta("sin-csp/index.html"), 1);
    debe_haber(
        &h,
        "sin-referrer/index.html",
        "falta <meta name=\"referrer\"",
    );
    assert_eq!(cuenta("sin-referrer/index.html"), 1);
    debe_haber(
        &h,
        "csp-detras/index.html",
        "primer elemento de <head> tras <meta charset>",
    );
    assert_eq!(cuenta("csp-detras/index.html"), 1);

    // HTML ilegible
    debe_haber(&h, "roto/index.html", "HTML que no se puede interpretar");
    assert_eq!(cuenta("roto/index.html"), 1);
    assert_eq!(
        de_fichero(&h, "roto/index.html")[0].linea,
        Some(11),
        "la línea del `<div` sin cerrar"
    );

    // Un manual con un script en línea sin su hash, y con un `style`.
    debe_haber(
        &h,
        "manual/index.html",
        "no lleva el hash de un <script> en línea",
    );
    debe_haber(&h, "manual/index.html", "atributo `style`");
    assert_eq!(cuenta("manual/index.html"), 2, "{}", listar(&h));

    // Y nada más: solo estas páginas tienen hallazgos.
    let con_hallazgos: std::collections::BTreeSet<&str> =
        h.iter().map(|x| x.fichero.as_str()).collect();
    assert_eq!(
        con_hallazgos.into_iter().collect::<Vec<_>>(),
        [
            "cabecera/index.html",
            "csp-detras/index.html",
            "csp-mala/index.html",
            "enlaces/index.html",
            "externos/index.html",
            "manual/index.html",
            "roto/index.html",
            "scripts/index.html",
            "sin-csp/index.html",
            "sin-referrer/index.html",
        ]
    );
}

#[test]
fn los_hallazgos_llevan_fichero_linea_y_texto() {
    let dir = sitio(&[(
        "index.html",
        &pagina_propia(
            "",
            "<p>uno</p>\n<p>dos</p>\n<p style=\"color: red\">tres</p>",
        ),
    )]);
    let h = comprobar(dir.path());
    assert_eq!(h.len(), 1, "{}", listar(&h));
    assert_eq!(h[0].fichero, "index.html");
    // 4 líneas de cabecera de la plantilla + 2 de la CSP y el referrer.
    let linea = h[0].linea.expect("debe llevar línea");
    let html = fs::read_to_string(dir.path().join("index.html")).unwrap();
    assert_eq!(
        html.lines().nth(linea - 1).unwrap().trim(),
        "<p style=\"color: red\">tres</p>"
    );
    let texto = h[0].to_string();
    assert!(
        texto.starts_with(&format!("index.html:{linea}: ")),
        "{texto}"
    );
}

// ---------------------------------------------------------------------------
// style y scripts
// ---------------------------------------------------------------------------

#[test]
fn un_style_en_un_atributo_se_detecta() {
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia(
                "",
                "<p style=\"color: red\">a</p>\n<p STYLE='x'>b</p>\n<div class=a Style=\"\">c</div>\n\
                 <svg><rect style=\"fill: red\"/></svg>\n<p style>d</p>",
            ),
        ),
        // También en el manual, que es donde mdBook lo suele colar.
        (
            "manual/index.html",
            &pagina_manual("", "<div style=\"clear: both\"></div>"),
        ),
        // Y una página limpia, que no da nada.
        ("limpia/index.html", &pagina_propia("", "<p class=\"a\">x</p>")),
    ]);
    let h = comprobar(dir.path());
    assert_eq!(
        hallazgos_con(&h, "index.html", "atributo `style`").len(),
        5,
        "{}",
        listar(&h)
    );
    assert_eq!(
        hallazgos_con(&h, "manual/index.html", "atributo `style`").len(),
        1
    );
    assert!(de_fichero(&h, "limpia/index.html").is_empty());
    assert_eq!(h.len(), 6, "{}", listar(&h));

    // Un `<style>` en línea tampoco lo deja pasar la CSP.
    let dir = sitio(&[(
        "index.html",
        &pagina_propia("<style>p { color: red }</style>\n", "<p>x</p>"),
    )]);
    let h = comprobar(dir.path());
    debe_haber(&h, "index.html", "<style> en línea");
}

#[test]
fn un_script_propio_en_una_pagina_no_del_manual_se_detecta() {
    let dir = sitio(&[
        (
            "a/index.html",
            &pagina_propia("", "<script>alert(1)</script>"),
        ),
        (
            "b/index.html",
            &pagina_propia("", "<script src=\"x.js\"></script>"),
        ),
        ("x.js", "// no se carga\n"),
        ("b/x.js", "// no se carga\n"),
        (
            "c/index.html",
            &pagina_propia("", "<SCRIPT TYPE=\"module\">import './y.js'</SCRIPT>"),
        ),
        (
            "d/index.html",
            &pagina_propia("", "<svg><script>alert(1)</script></svg>"),
        ),
        // Un script en la cabecera.
        (
            "e/index.html",
            &pagina_propia("<script>alert(1)</script>\n", ""),
        ),
        // Una carpeta que solo empieza como la del manual: no es el manual.
        (
            "manual-falso/index.html",
            &pagina_propia("", "<script>alert(1)</script>"),
        ),
        (
            "manualx/index.html",
            &pagina_propia("", "<script>alert(1)</script>"),
        ),
        // Mayúsculas en la carpeta: tampoco.
        (
            "Manual/index.html",
            &pagina_propia("", "<script>alert(1)</script>"),
        ),
    ]);
    let h = comprobar(dir.path());
    for fichero in [
        "a/index.html",
        "b/index.html",
        "c/index.html",
        "d/index.html",
        "e/index.html",
        "manual-falso/index.html",
        "manualx/index.html",
        "Manual/index.html",
    ] {
        debe_haber(&h, fichero, "<script> en una página que no es del manual");
    }

    // En el manual, en cambio, es lo esperado, con su hash en la CSP.
    let dir = sitio(&[
        (
            "manual/index.html",
            &pagina_manual(
                "",
                "<script>const x = 1;</script>\n<script src=\"toc.js\"></script>",
            ),
        ),
        ("manual/toc.js", "// toc\n"),
    ]);
    let h = comprobar(dir.path());
    assert!(h.is_empty(), "{}", listar(&h));
}

#[test]
fn los_manejadores_de_eventos_se_detectan_en_todas_las_paginas() {
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia(
                "",
                "<button onclick=\"x()\">a</button><img src=\"i.svg\" ONERROR=\"x()\">",
            ),
        ),
        ("i.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        (
            "manual/index.html",
            &pagina_manual("", "<body onload=\"x()\"></body>"),
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "index.html", "atributo `onclick`");
    debe_haber(&h, "index.html", "atributo `onerror`");
    debe_haber(&h, "manual/index.html", "atributo `onload`");
}

#[test]
fn un_svg_con_script_se_detecta_aunque_no_sea_una_pagina() {
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia("", "<img src=\"a.svg\" alt=\"\">"),
        ),
        (
            "a.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><SCRIPT>alert(1)</SCRIPT></svg>",
        ),
        (
            "b.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>",
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "a.svg", "lleva <script>");
    assert!(de_fichero(&h, "b.svg").is_empty());
}

// ---------------------------------------------------------------------------
// Enlaces y anclas
// ---------------------------------------------------------------------------

#[test]
fn un_enlace_roto_y_un_ancla_rota_se_detectan() {
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia(
                "<link rel=\"stylesheet\" href=\"css/falta.css\">\n",
                "<h1 id=\"inicio\">Inicio</h1>\n\
                 <a href=\"otra/\">buena</a> <a href=\"otra/#seccion\">buena con ancla</a>\n\
                 <a href=\"#inicio\">buena en la misma</a> <a href=\"otra/index.html#seccion-1\">rota</a>\n\
                 <a href=\"no-existe/\">rota</a> <a href=\"otra/no.html\">rota</a>\n\
                 <a href=\"#fantasma\">rota</a> <img src=\"img/falta.svg\" alt=\"\">\n\
                 <a href=\"otra/#Seccion\">rota (otra caja)</a>",
            ),
        ),
        (
            "otra/index.html",
            &pagina_propia("", "<h2 id=\"seccion\">S</h2><a href=\"../#inicio\">buena</a><a href=\"../#nada\">rota</a>"),
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "index.html", "enlace roto: «css/falta.css»");
    debe_haber(&h, "index.html", "ancla rota: «otra/index.html#seccion-1»");
    debe_haber(&h, "index.html", "enlace roto: «no-existe/»");
    debe_haber(&h, "index.html", "enlace roto: «otra/no.html»");
    debe_haber(&h, "index.html", "ancla rota: «#fantasma»");
    debe_haber(&h, "index.html", "enlace roto: «img/falta.svg»");
    debe_haber(&h, "index.html", "ancla rota: «otra/#Seccion»");
    assert_eq!(de_fichero(&h, "index.html").len(), 7, "{}", listar(&h));
    debe_haber(&h, "otra/index.html", "ancla rota: «../#nada»");
    assert_eq!(de_fichero(&h, "otra/index.html").len(), 1, "{}", listar(&h));
}

#[test]
fn las_anclas_se_comparan_como_el_navegador() {
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia(
                "",
                "<h1 id=\"túneles\">T</h1><a name=\"antigua\"></a><p id=\"a&amp;b\">x</p>\n\
                 <a href=\"#túneles\">ok</a> <a href=\"#t%C3%BAneles\">ok</a> <a href=\"#antigua\">ok</a>\n\
                 <a href=\"#a&amp;b\">ok</a> <a href=\"#\">arriba</a> <a href=\"#top\">arriba</a>\n\
                 <a href=\"./#túneles\">ok</a> <a href=\"index.html#t%C3%BAneles\">ok</a>\n\
                 <a href=\"#tuneles\">rota</a> <a href=\"#t%C3%BAneles-1\">rota</a>",
            ),
        ),
    ]);
    let h = comprobar(dir.path());
    assert_eq!(h.len(), 2, "{}", listar(&h));
    debe_haber(&h, "index.html", "ancla rota: «#tuneles»");
    debe_haber(&h, "index.html", "ancla rota: «#túneles-1»");
}

#[test]
fn las_carpetas_sin_barra_final_y_los_destinos_que_no_son_paginas_resuelven() {
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia(
                "<link rel=\"stylesheet\" href=\"css/web.css\">\n",
                "<a href=\"carpeta\">sin barra</a> <a href=\"carpeta/\">con barra</a> <a href=\"carpeta/index.html\">fichero</a>\n\
                 <a href=\"css/web.css#ignorada\">ancla en un css</a> <a href=\"sin-index/\">sin índice</a>",
            ),
        ),
        ("carpeta/index.html", &pagina_propia("", "<p>x</p>")),
        ("css/web.css", "body {}\n"),
        ("sin-index/otra.html", &pagina_propia("", "<p>x</p>")),
    ]);
    let h = comprobar(dir.path());
    assert_eq!(h.len(), 1, "{}", listar(&h));
    debe_haber(&h, "index.html", "enlace roto: «sin-index/»");
}

#[test]
fn el_404_puede_usar_rutas_absolutas_y_los_enlaces_a_su_dominio() {
    let cuerpo_404 = "<a href=\"https://magi.erratia.com/\">Portada</a> <a href=\"/descargas/\">Descargas</a>\n\
                      <a href=\"/en/\">English</a> <a href=\"https://magi.erratia.com/en/\">English</a>";
    let dir = sitio(&[
        ("index.html", &pagina_propia("", "<h1>x</h1>")),
        ("descargas/index.html", &pagina_propia("", "<h1>x</h1>")),
        ("en/index.html", &pagina_propia("", "<h1>x</h1>")),
        ("css/web.css", "body {}\n"),
        (
            "404.html",
            &pagina_propia("<link rel=\"stylesheet\" href=\"/css/web.css\">\n<link rel=\"stylesheet\" href=\"https://magi.erratia.com/css/web.css\">\n", cuerpo_404),
        ),
    ]);
    let h = comprobar(dir.path());
    assert!(h.is_empty(), "{}", listar(&h));

    // En el 404 las rutas absolutas también deben existir...
    let dir = sitio(&[
        ("index.html", &pagina_propia("", "<h1>x</h1>")),
        (
            "404.html",
            &pagina_propia("", "<a href=\"/no-existe/\">x</a>"),
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "404.html", "enlace roto: «/no-existe/»");

    // ...y el resto de páginas no las usa (h).
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia(
                "",
                "<a href=\"/descargas/\">x</a><img src=\"/img/a.svg\" alt=\"\">",
            ),
        ),
        ("descargas/index.html", &pagina_propia("", "<h1>x</h1>")),
        ("img/a.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        (
            "404x.html",
            &pagina_propia("", "<a href=\"/descargas/\">x</a>"),
        ),
        (
            "manual/index.html",
            &pagina_manual("", "<a href=\"/descargas/\">x</a>"),
        ),
    ]);
    let h = comprobar(dir.path());
    assert_eq!(
        hallazgos_con(
            &h,
            "index.html",
            "las rutas absolutas del sitio solo se usan en 404.html"
        )
        .len(),
        2,
        "{}",
        listar(&h)
    );
    debe_haber(
        &h,
        "404x.html",
        "las rutas absolutas del sitio solo se usan en 404.html",
    );
    debe_haber(
        &h,
        "manual/index.html",
        "las rutas absolutas del sitio solo se usan en 404.html",
    );
    // Un `404.html` que no está en la raíz tampoco vale.
    let dir = sitio(&[
        ("index.html", &pagina_propia("", "<h1>x</h1>")),
        ("a/404.html", &pagina_propia("", "<a href=\"/\">x</a>")),
    ]);
    debe_haber(
        &comprobar(dir.path()),
        "a/404.html",
        "las rutas absolutas del sitio solo se usan en 404.html",
    );
}

#[test]
fn se_comprueban_todos_los_atributos_que_llevan_una_url() {
    let dir = sitio(&[(
        "index.html",
        &pagina_propia(
            "<link rel=\"icon\" href=\"falta1.svg\">\n",
            "<img src=\"falta2.svg\" srcset=\"falta3.svg 1x, falta4.svg 2x\" alt=\"\">\n\
         <iframe src=\"falta5.html\"></iframe>\n\
         <video poster=\"falta6.svg\"><source src=\"falta7.mp4\"></video>\n\
         <svg><use xlink:href=\"falta8.svg#a\"/><use href=\"#nada\"/></svg>\n\
         <form action=\"falta9/\"></form>\n\
         <object data=\"falta10.svg\"></object>\n\
         <a ping=\"falta11/ falta12/\" href=\"#\">x</a>",
        ),
    )]);
    let h = comprobar(dir.path());
    for n in 1..=12 {
        let esperado = if n == 5 {
            "falta5.html".to_string()
        } else {
            format!("falta{n}")
        };
        assert!(
            h.iter().any(|x| x.que.contains(&esperado)),
            "no se vio falta{n}:\n{}",
            listar(&h)
        );
    }
    debe_haber(&h, "index.html", "ancla rota: «#nada»");
}

// ---------------------------------------------------------------------------
// Externos
// ---------------------------------------------------------------------------

#[test]
fn un_enlace_externo_fuera_de_la_lista_se_detecta() {
    let dir = sitio(&[(
        "index.html",
        &pagina_propia(
            "",
            &[
                "<a href=\"https://github.com/HectorEspejo/magi\">ok</a>",
                "<a href=\"https://github.com/HectorEspejo/magi/releases/tag/v1.6.1\">ok</a>",
                "<a href=\"https://github.com/ImranR98/Obtainium\">ok</a>",
                "<a href=\"https://github.com/termux-user-repository/tur\">ok</a>",
                "<a href=\"https://cli.github.com/\">ok</a>",
                "<a href=\"https://termux.dev/\">ok</a>",
                "<a href=\"https://keepachangelog.com/es-ES/1.1.0/\">ok</a>",
                "<a href=\"https://semver.org/lang/es/\">ok</a>",
                "<a href=\"https://magi.erratia.com/\">ok</a>",
                "<a href=\"https://docs.github.com/es/site-policy\">ok</a>",
                "<a href=\"https://ejemplo.org/\">mal</a>",
                "<a href=\"https://google.com/\">mal</a>",
                "<a href=\"https://github.com/otro/repo\">mal</a>",
                "<a href=\"https://github.com/\">mal</a>",
                "<a href=\"https://gist.github.com/HectorEspejo/magi\">mal</a>",
                "<a href=\"https://raw.githubusercontent.com/HectorEspejo/magi/main/x\">mal</a>",
                "<a href=\"https://erratia.com/\">mal</a>",
                "<a href=\"https://blog.erratia.com/\">mal</a>",
                "<a href=\"https://192.0.2.10/\">mal</a>",
            ]
            .join("\n"),
        ),
    )]);
    let h = comprobar(dir.path());
    assert_eq!(h.len(), 9, "{}", listar(&h));
    for mala in [
        "https://ejemplo.org/",
        "https://google.com/",
        "https://github.com/otro/repo",
        "https://github.com/",
        "https://gist.github.com/HectorEspejo/magi",
        "https://raw.githubusercontent.com/HectorEspejo/magi/main/x",
        "https://erratia.com/",
        "https://blog.erratia.com/",
        "https://192.0.2.10/",
    ] {
        assert!(
            h.iter().any(|x| x.que.contains(&format!("«{mala}»"))),
            "no se vio {mala}:\n{}",
            listar(&h)
        );
    }
}

#[test]
fn un_http_o_un_javascript_se_detectan() {
    let dir = sitio(&[(
        "index.html",
        &pagina_propia(
            "",
            &[
                "<a href=\"http://github.com/HectorEspejo/magi\">http</a>",
                "<a href=\"javascript:alert(1)\">js</a>",
                "<a href=\"JaVaScRiPt:alert(1)\">js</a>",
                "<a href=\"java&#x09;script:alert(1)\">js</a>",
                "<a href=\"java\tscript:alert(1)\">js</a>",
                "<a href=\"&#106;avascript:alert(1)\">js</a>",
                "<a href=\"data:text/html;base64,AAAA\">data</a>",
                "<a href=\"vbscript:x\">vb</a>",
                "<a href=\"//ejemplo.org/\">sin esquema</a>",
                "<a href=\"HTTPS://github.com/HectorEspejo/magi\">mayúsculas</a>",
                "<a href=\"mailto:alguien@ejemplo.org\">correo</a>",
                "<a href=\" https://github.com/HectorEspejo/magi\">con blanco</a>",
                "<a href=https://github.com/HectorEspejo/magi\\@evil.ejemplo.org>con barra</a>",
                "<form action=\"javascript:alert(1)\"></form>",
                "<iframe src=\"javascript:alert(1)\"></iframe>",
                "<iframe src=\"data:text/html,x\"></iframe>",
                "<a href=\"https://github.com/HectorEspejo/magi\">esta sí</a>",
            ]
            .join("\n"),
        ),
    )]);
    let h = comprobar(dir.path());
    assert_eq!(
        hallazgos_con(&h, "index.html", "enlace no admitido").len(),
        16,
        "{}",
        listar(&h)
    );
    assert_eq!(h.len(), 16, "{}", listar(&h));
}

#[test]
fn los_recursos_no_se_cargan_de_fuera_ni_de_la_lista() {
    let dir = sitio(&[("index.html", &pagina_propia(
        "<link rel=\"stylesheet\" href=\"https://github.com/HectorEspejo/magi/raw/main/x.css\">\n\
         <link rel=\"alternate\" hreflang=\"en\" href=\"https://magi.erratia.com/en/\">\n\
         <link rel=\"canonical\" href=\"https://magi.erratia.com/\">\n\
         <link rel=\"stylesheet\" href=\"https://magi.erratia.com/css/web.css\">\n\
         <link rel=\"icon\" href=\"https://semver.org/icono.svg\">\n",
        "<img src=\"https://github.com/HectorEspejo/magi/raw/main/a.png\" alt=\"\">\n\
         <iframe src=\"https://semver.org/\"></iframe>\n\
         <a href=\"https://semver.org/\">enlace, no recurso</a>\n\
         <a href=\"https://github.com/HectorEspejo/magi\">enlace</a>",
    )), ("css/web.css", "body {}\n")]);
    let h = comprobar(dir.path());
    assert_eq!(
        hallazgos_con(&h, "index.html", "recurso cargado de fuera").len(),
        4,
        "{}",
        listar(&h)
    );
    assert_eq!(h.len(), 4, "{}", listar(&h));
}

#[test]
fn el_host_se_compara_exacto_y_el_prefijo_por_segmentos() {
    let lista = ListaPermitida::por_defecto();
    for permitida in [
        "https://github.com/HectorEspejo/magi",
        "https://github.com/HectorEspejo/magi/",
        "https://github.com/HectorEspejo/magi/issues/12",
        "https://github.com/HectorEspejo/magi/releases/tag/v1.6.1?x=1#y",
        "https://github.com/HectorEspejo/magi#readme",
        "https://github.com/HectorEspejo/magi?tab=readme",
        "https://github.com:443/HectorEspejo/magi",
        "https://github.com/ImranR98/Obtainium",
        "https://github.com/ImranR98/Obtainium/releases",
        "https://github.com/termux-user-repository",
        "https://github.com/termux-user-repository/",
        "https://github.com/termux-user-repository/tur/blob/master/packages",
        "https://cli.github.com/",
        "https://cli.github.com",
        "https://cli.github.com/manual/gh_attestation_verify",
        "https://termux.dev/en/",
        "https://keepachangelog.com/es-ES/1.1.0/",
        "https://semver.org/",
        "https://semver.org",
        "https://magi.erratia.com/",
        "https://magi.erratia.com/en/download/",
        "https://docs.github.com/es/site-policy/privacy-policies/github-general-privacy-statement",
    ] {
        assert!(lista.permite(permitida), "debía permitirse: {permitida}");
    }
    for rechazada in [
        // el prefijo se compara por segmentos
        "https://github.com/HectorEspejo/magi-evil",
        "https://github.com/HectorEspejo/magi-evil/x",
        "https://github.com/HectorEspejo/magix",
        "https://github.com/HectorEspejo/mag",
        "https://github.com/HectorEspejo",
        "https://github.com/HectorEspejo/",
        "https://github.com/HectorEspejoX/magi",
        "https://github.com/hectorespejo/magi",
        "https://github.com/ImranR98/Obtainium-evil",
        "https://github.com/ImranR98",
        "https://github.com/termux-user-repository-evil",
        "https://github.com/termux-user-repository-evil/tur",
        "https://github.com/termux",
        "https://github.com/",
        "https://github.com",
        // el host se compara exacto
        "https://github.com.evil.ejemplo.org/HectorEspejo/magi",
        "https://evil.github.com/HectorEspejo/magi",
        "https://xgithub.com/HectorEspejo/magi",
        "https://github.com.ejemplo.org/",
        "https://github.com./HectorEspejo/magi",
        "https://GitHub.com/HectorEspejo/magi",
        "https://GITHUB.COM/HectorEspejo/magi",
        "https://cli.github.com.ejemplo.org/",
        "https://sub.cli.github.com/",
        "https://semver.org.ejemplo.org/",
        "https://fake-semver.org/",
        "https://www.semver.org/",
        "https://magi.erratia.com.ejemplo.org/",
        "https://erratia.com/",
        "https://blog.erratia.com/",
        // el esquema, el usuario, el puerto y las barras
        "http://github.com/HectorEspejo/magi",
        "HTTPS://github.com/HectorEspejo/magi",
        "//github.com/HectorEspejo/magi",
        "github.com/HectorEspejo/magi",
        "ftp://github.com/HectorEspejo/magi",
        "javascript:https://github.com/HectorEspejo/magi",
        "https://usuario@github.com/HectorEspejo/magi",
        "https://github.com@evil.ejemplo.org/HectorEspejo/magi",
        "https://evil.ejemplo.org\\@github.com/HectorEspejo/magi",
        "https://evil.ejemplo.org#@github.com/HectorEspejo/magi",
        "https://github.com:8443/HectorEspejo/magi",
        "https://github.com:/HectorEspejo/magi",
        "https://github.com:80/HectorEspejo/magi",
        "https://github.com\\HectorEspejo\\magi",
        "https://github.com/HectorEspejo\\magi",
        "https:///github.com/HectorEspejo/magi",
        "https://",
        "",
        // rutas que se salen o se esconden
        "https://github.com/HectorEspejo/magi/../../evil/x",
        "https://github.com/HectorEspejo/magi/../magi-evil",
        "https://github.com/HectorEspejo/magi/./x",
        "https://github.com/HectorEspejo/magi/%2e%2e/%2e%2e/evil",
        "https://github.com/HectorEspejo/magi/%2E%2E/evil",
        "https://github.com/HectorEspejo/magi/.%2e/evil",
        "https://github.com/HectorEspejo/magi%2f..%2f..%2fevil",
        "https://github.com/HectorEspejo/magi/a%5c..%5cevil",
        "https://github.com/HectorEspejo/magi//x",
        "https://github.com//HectorEspejo/magi",
        "https://github.com/HectorEspejo/magi/%00",
        "https://github.com/HectorEspejo/magi/%zz",
        // blancos y caracteres de control
        "https://github.com/HectorEspejo/magi evil",
        "https://github.com/HectorEspejo/magi\n",
        "https://github.com/HectorEspejo/magi\t",
        " https://github.com/HectorEspejo/magi",
        "https://github.com/HectorEspejo/magi\u{0}",
        "https://github.com/HectorEspejo/magi\"",
        "https://github.com/HectorEspejo/magi<",
        "https://github.com/HectorEspejo/magí",
        "https://gıthub.com/HectorEspejo/magi",
    ] {
        assert!(!lista.permite(rechazada), "debía rechazarse: {rechazada:?}");
    }

    // Un prefijo vacío permite todo el host; sin prefijos, nada.
    let a_medida = ListaPermitida {
        dominios: vec![
            ("ejemplo.org".into(), vec![String::new()]),
            ("vacio.ejemplo.org".into(), vec![]),
            (
                "a.ejemplo.org".into(),
                vec!["/uno/dos".into(), "/tres".into()],
            ),
            ("b.ejemplo.org".into(), vec!["/".into()]),
        ],
    };
    assert!(a_medida.permite("https://ejemplo.org/cualquier/cosa"));
    assert!(a_medida.permite("https://ejemplo.org"));
    assert!(!a_medida.permite("https://sub.ejemplo.org/"));
    assert!(!a_medida.permite("https://vacio.ejemplo.org/"));
    assert!(a_medida.permite("https://a.ejemplo.org/uno/dos/tres"));
    assert!(a_medida.permite("https://a.ejemplo.org/tres"));
    assert!(!a_medida.permite("https://a.ejemplo.org/uno"));
    assert!(!a_medida.permite("https://a.ejemplo.org/uno/doss"));
    assert!(a_medida.permite("https://b.ejemplo.org/lo/que/sea"));
}

// ---------------------------------------------------------------------------
// Cabecera: base y meta refresh
// ---------------------------------------------------------------------------

#[test]
fn base_y_meta_refresh_se_detectan() {
    let dir = sitio(&[
        (
            "a/index.html",
            &pagina_propia("<base href=\"https://magi.erratia.com/\">\n", "<p>x</p>"),
        ),
        (
            "b/index.html",
            &pagina_propia("<base target=\"_blank\">\n", "<p>x</p>"),
        ),
        (
            "c/index.html",
            &pagina_propia("<BASE HREF=\"/\">\n", "<p>x</p>"),
        ),
        (
            "d/index.html",
            &pagina_propia(
                "<meta http-equiv=\"refresh\" content=\"0; url=https://magi.erratia.com/\">\n",
                "<p>x</p>",
            ),
        ),
        (
            "e/index.html",
            &pagina_propia("<META HTTP-EQUIV=Refresh content=5>\n", "<p>x</p>"),
        ),
        // En el cuerpo también: un navegador los acepta donde estén.
        (
            "f/index.html",
            &pagina_propia(
                "",
                "<meta http-equiv=\"refresh\" content=\"0\"><base href=\"x/\">",
            ),
        ),
        (
            "g/index.html",
            &pagina_manual("<base href=\"x/\">\n", "<p>x</p>"),
        ),
        (
            "manual/index.html",
            &pagina_manual("<meta http-equiv=\"refresh\" content=\"0\">\n", "<p>x</p>"),
        ),
        // Un `<meta http-equiv>` que no es el refresh no cuenta.
        (
            "h/index.html",
            &pagina_propia(
                "<meta http-equiv=\"x-ua-compatible\" content=\"ie=edge\">\n",
                "<p>x</p>",
            ),
        ),
    ]);
    let h = comprobar(dir.path());
    for fichero in ["a", "b", "c", "f", "g"] {
        debe_haber(&h, &format!("{fichero}/index.html"), "<base> no permitido");
    }
    for fichero in ["d", "e", "f", "manual"] {
        debe_haber(
            &h,
            &format!("{fichero}/index.html"),
            "<meta http-equiv=\"refresh\"> no permitido",
        );
    }
    assert!(de_fichero(&h, "h/index.html").is_empty(), "{}", listar(&h));
}

// ---------------------------------------------------------------------------
// CSP
// ---------------------------------------------------------------------------

#[test]
fn la_csp_de_cada_pagina_se_recalcula_y_se_comprueba() {
    let manual_bueno = pagina_manual(
        "",
        "<script>const a = 1;</script>\n<script>const b = 2;</script>",
    );
    let propia = pagina_propia("", "<p>x</p>");

    // Un script en línea nuevo en el manual, sin volver a pasar por `insertar`:
    // su hash no está en la CSP.
    let manual_con_script_nuevo =
        manual_bueno.replace("</body>", "<script>const c = 3;</script>\n</body>");
    // Un hash que ya no corresponde a ningún script.
    let manual_con_hash_viejo = manual_bueno.replace("const a = 1;", "const a = 11;");
    // Una CSP a mano más permisiva en una página propia.
    let propia_con_self = propia.replace("script-src 'none'", "script-src 'self'");
    // Una CSP con otra directiva de más.
    let propia_con_connect = propia.replace("base-uri 'none'", "base-uri 'none'; connect-src *");
    // Dos CSP.
    let propia_con_dos = propia.replace(
        "</body>",
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'\"></body>",
    );
    // `content` con una entidad que no se entiende.
    let propia_con_entidad = propia.replace("default-src 'none'", "default-src &bogus;");

    let dir = sitio(&[
        ("manual/bueno.html", &manual_bueno),
        ("manual/script-nuevo.html", &manual_con_script_nuevo),
        ("manual/hash-viejo.html", &manual_con_hash_viejo),
        ("propia.html", &propia),
        ("propia-self.html", &propia_con_self),
        ("propia-connect.html", &propia_con_connect),
        ("propia-dos.html", &propia_con_dos),
        ("propia-entidad.html", &propia_con_entidad),
    ]);
    let h = comprobar(dir.path());
    assert!(
        de_fichero(&h, "manual/bueno.html").is_empty(),
        "{}",
        listar(&h)
    );
    assert!(de_fichero(&h, "propia.html").is_empty(), "{}", listar(&h));
    debe_haber(
        &h,
        "manual/script-nuevo.html",
        "`script-src` no lleva el hash de un <script> en línea",
    );
    debe_haber(
        &h,
        "manual/hash-viejo.html",
        "`script-src` no lleva el hash de un <script> en línea",
    );
    debe_haber(
        &h,
        "propia-self.html",
        "en las páginas propias `script-src` debe ser `'none'`",
    );
    debe_haber(&h, "propia-connect.html", "la CSP no es la esperada");
    debe_haber(&h, "propia-dos.html", "más de una CSP");
    debe_haber(&h, "propia-entidad.html", "CSP no válida");
}

#[test]
fn un_script_en_linea_nuevo_se_ve_y_al_reinsertar_la_csp_se_arregla() {
    let dir = tempfile::tempdir().unwrap();
    copiar_arbol(&fijos().join("sitio-bueno"), dir.path());
    assert!(comprobar(dir.path()).is_empty());

    // Alguien añade un script al tema del manual y no recalcula la CSP.
    let ruta = dir.path().join("manual/tuneles.html");
    let original = fs::read_to_string(&ruta).unwrap();
    let con_script = original.replace("</body>", "<script>nuevo();</script>\n</body>");
    fs::write(&ruta, &con_script).unwrap();
    let h = comprobar(dir.path());
    debe_haber(
        &h,
        "manual/tuneles.html",
        "`script-src` no lleva el hash de un <script> en línea",
    );
    assert_eq!(h.len(), 1, "{}", listar(&h));

    // Pasándola de nuevo por `insertar`, el hash entra y todo vuelve a estar bien.
    fs::write(&ruta, insertar(&con_script, true).unwrap()).unwrap();
    assert!(comprobar(dir.path()).is_empty());
    assert!(csp::extraer(&fs::read_to_string(&ruta).unwrap())
        .unwrap()
        .unwrap()
        .contains(&csp::hash_script("nuevo();")));
}

#[test]
fn una_csp_que_nunca_debe_permitir_unsafe_se_detecta_en_cualquier_pagina() {
    let base = pagina_propia("", "<p>x</p>");
    let manual = pagina_manual("", "<p>x</p>");
    let dir = sitio(&[
        (
            "a.html",
            &base.replace("style-src 'self'", "style-src 'self' 'unsafe-inline'"),
        ),
        (
            "b.html",
            &base.replace("script-src 'none'", "script-src 'none' 'unsafe-eval'"),
        ),
        (
            "c.html",
            &base.replace("script-src 'none'", "script-src 'none' 'UNSAFE-INLINE'"),
        ),
        (
            "manual/d.html",
            &manual.replace("script-src 'self'", "script-src 'self' 'unsafe-inline'"),
        ),
        (
            "manual/e.html",
            &manual.replace(
                "script-src 'self'",
                "script-src 'self' 'wasm-unsafe-eval' 'unsafe-hashes'",
            ),
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "a.html", "'unsafe-inline': nunca se permite");
    debe_haber(&h, "b.html", "'unsafe-eval': nunca se permite");
    debe_haber(&h, "c.html", "'UNSAFE-INLINE': nunca se permite");
    debe_haber(&h, "manual/d.html", "'unsafe-inline': nunca se permite");
    debe_haber(&h, "manual/e.html", "'wasm-unsafe-eval': nunca se permite");
    debe_haber(&h, "manual/e.html", "'unsafe-hashes': nunca se permite");
}

#[test]
fn la_csp_debe_ser_el_primer_elemento_tras_el_charset() {
    let buena = pagina_propia("", "<p>x</p>");
    // La CSP detrás del `<title>`.
    let linea_csp = buena
        .lines()
        .find(|l| l.contains("Content-Security-Policy"))
        .unwrap()
        .to_string();
    let detras = buena
        .replace(&format!("{linea_csp}\n"), "")
        .replace("</head>", &format!("{linea_csp}\n</head>"));
    // Delante del charset.
    let delante = buena
        .replace(
            "<meta charset=\"utf-8\">",
            &format!("{}\n<meta charset=\"utf-8\">", linea_csp.trim()),
        )
        .replace(&format!("{linea_csp}\n"), "");
    // Un comentario entre medias no cuenta como elemento (mdBook los pone).
    let con_comentario = buena.replace(&linea_csp, &format!("<!-- c -->\n{linea_csp}"));
    let dir = sitio(&[
        ("buena.html", &buena),
        ("detras.html", &detras),
        ("delante.html", &delante),
        ("con-comentario.html", &con_comentario),
    ]);
    let h = comprobar(dir.path());
    assert!(de_fichero(&h, "buena.html").is_empty(), "{}", listar(&h));
    assert!(
        de_fichero(&h, "con-comentario.html").is_empty(),
        "{}",
        listar(&h)
    );
    debe_haber(
        &h,
        "detras.html",
        "primer elemento de <head> tras <meta charset>",
    );
    debe_haber(&h, "delante.html", "primer elemento de <head>");
}

#[test]
fn sin_csp_sin_head_o_sin_referrer_es_un_hallazgo() {
    let dir = sitio(&[
        ("a.html", "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>x</title></head><body></body></html>"),
        ("b.html", "<html><body><p>sin head</p></body></html>"),
        ("c.html", &pagina_propia("", "<p>x</p>").replace("<meta name=\"referrer\" content=\"strict-origin-when-cross-origin\">", "<meta name=\"referrer\" content=\"unsafe-url\">")),
        ("d.html", &pagina_propia("", "<p>x</p>").replace("<meta charset=\"utf-8\">", "<meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">")),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "a.html", "no lleva CSP");
    debe_haber(&h, "a.html", "falta <meta name=\"referrer\"");
    debe_haber(&h, "b.html", "no lleva CSP");
    debe_haber(
        &h,
        "c.html",
        "el <meta name=\"referrer\"> debe ser uno solo",
    );
    debe_haber(&h, "d.html", "<head> mal formado");
}

// ---------------------------------------------------------------------------
// Falla cerrado
// ---------------------------------------------------------------------------

#[test]
fn un_tag_sin_cerrar_es_un_hallazgo_y_no_un_panico() {
    let dir = sitio(&[
        ("index.html", &pagina_propia("", "<p>bien</p>")),
        (
            "a.html",
            "<html><head><meta charset=\"utf-8\"></head><body><div class=\"x",
        ),
        ("b.html", "<html><body><a href=\"x\""),
        ("c.html", "<html><body><!-- sin cerrar"),
        ("d.html", "<html><body><script>sin cerrar"),
        ("e.html", "<html><body>a < b</body></html>"),
        ("f.html", "<?xml version=\"1.0\"?><html></html>"),
        (
            "manual/g.html",
            "<html><body><svg><style><a>x</a></style></svg>",
        ),
    ]);
    let h = comprobar(dir.path());
    for fichero in [
        "a.html",
        "b.html",
        "c.html",
        "d.html",
        "e.html",
        "f.html",
        "manual/g.html",
    ] {
        debe_haber(&h, fichero, "HTML que no se puede interpretar");
        assert_eq!(
            de_fichero(&h, fichero).len(),
            1,
            "solo ese hallazgo en {fichero}"
        );
        assert!(de_fichero(&h, fichero)[0].linea.is_some());
    }
    assert!(de_fichero(&h, "index.html").is_empty());
}

#[test]
fn un_ancla_hacia_una_pagina_ilegible_no_se_da_por_buena() {
    let dir = sitio(&[
        (
            "index.html",
            &pagina_propia(
                "",
                "<a href=\"rota.html#algo\">x</a><a href=\"rota.html\">y</a>",
            ),
        ),
        ("rota.html", "<html><body><div class=\"x"),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "rota.html", "HTML que no se puede interpretar");
    debe_haber(
        &h,
        "index.html",
        "no se puede comprobar el ancla de «rota.html#algo»",
    );
    // El enlace sin ancla sí llega a un fichero que existe.
    assert_eq!(de_fichero(&h, "index.html").len(), 1, "{}", listar(&h));
}

#[test]
fn un_fichero_que_no_es_utf8_es_un_hallazgo() {
    let dir = sitio(&[("index.html", &pagina_propia("", "<p>bien</p>"))]);
    escribir(
        dir.path(),
        "mal.html",
        b"<html><body>\xff\xfe\xfd</body></html>",
    );
    escribir(dir.path(), "en/mal2.html", b"\xc3\x28");
    let h = comprobar(dir.path());
    debe_haber(&h, "mal.html", "no es UTF-8 válido");
    debe_haber(&h, "en/mal2.html", "no es UTF-8 válido");
    assert_eq!(h.len(), 2, "{}", listar(&h));
}

#[test]
fn la_extension_en_mayusculas_tambien_es_una_pagina() {
    let dir = sitio(&[
        ("index.html", &pagina_propia("", "<p>bien</p>")),
        (
            "x/Pagina.HTML",
            "<html><body><script>alert(1)</script></body></html>",
        ),
        (
            "y/otra.htm",
            "<html><body><p style=\"a\">x</p></body></html>",
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(
        &h,
        "x/Pagina.HTML",
        "<script> en una página que no es del manual",
    );
    debe_haber(&h, "y/otra.htm", "atributo `style`");
}

#[test]
fn un_sitio_sin_paginas_o_inexistente_no_pasa() {
    let dir = sitio(&[("css/web.css", "body {}\n")]);
    let h = comprobar(dir.path());
    debe_haber(&h, ".", "ninguna página");

    let vacia = tempfile::tempdir().unwrap();
    assert_eq!(comprobar(vacia.path()).len(), 1);

    let inexistente = vacia.path().join("no-existe");
    let e = comprobar_sitio(&inexistente, &ListaPermitida::por_defecto()).unwrap_err();
    assert!(matches!(e, ErrorWeb::Lectura { .. }), "{e}");
}

#[cfg(unix)]
#[test]
fn los_enlaces_simbolicos_no_se_siguen_y_se_avisan() {
    use std::os::unix::fs::symlink;
    let dir = sitio(&[(
        "index.html",
        &pagina_propia("", "<a href=\"enlace/\">x</a><a href=\"fuera.html\">y</a>"),
    )]);
    // Una carpeta y un fichero de fuera del sitio, que no deben leerse.
    let fuera = tempfile::tempdir().unwrap();
    escribir(
        fuera.path(),
        "index.html",
        b"<html><body><script>alert(1)</script></body></html>",
    );
    escribir(fuera.path(), "fuera.html", b"<html><body></body></html>");
    symlink(fuera.path(), dir.path().join("enlace")).unwrap();
    symlink(
        fuera.path().join("fuera.html"),
        dir.path().join("fuera.html"),
    )
    .unwrap();
    symlink("/etc/hostname", dir.path().join("host.html")).unwrap();
    let h = comprobar(dir.path());
    debe_haber(&h, "enlace", "enlace simbólico");
    debe_haber(&h, "fuera.html", "enlace simbólico");
    debe_haber(&h, "host.html", "enlace simbólico");
    // No se leyó nada de lo que apuntan, y los enlaces que pasan por ellos están rotos.
    assert!(
        !h.iter().any(|x| x.que.contains("<script>")),
        "{}",
        listar(&h)
    );
    debe_haber(&h, "index.html", "enlace roto: «enlace/»");
    debe_haber(&h, "index.html", "enlace roto: «fuera.html»");
}

#[test]
fn los_mensajes_no_dejan_pasar_caracteres_de_control() {
    // Un `href` con saltos de línea y secuencias de escape no puede inventarse
    // líneas del registro (p. ej. órdenes `::error::` de GitHub Actions).
    let dir = sitio(&[(
        "index.html",
        &pagina_propia(
            "",
            "<a href=\"x\n::error::inyectado\u{1b}[31m\">uno</a>\n\
             <a href=\"https://evil.ejemplo.org/\n::warning::otro\">dos</a>\n\
             <a href=\"nada\n::error::roto.html\">tres</a>\n\
             <img src=\"largo.svg\" alt=\"\"><a href=\"#anc\nla\">cuatro</a>",
        ),
    )]);
    let h = comprobar(dir.path());
    assert!(h.len() >= 4, "{}", listar(&h));
    for x in &h {
        let texto = x.to_string();
        assert!(!texto.contains('\n'), "salto de línea en {texto:?}");
        assert!(!texto.contains('\u{1b}'), "escape en {texto:?}");
        assert!(
            !texto.chars().any(|c| c.is_control()),
            "control en {texto:?}"
        );
    }
    // Y los largos se acortan.
    let largo = "a".repeat(5_000);
    let dir = sitio(&[(
        "index.html",
        &pagina_propia("", &format!("<a href=\"{largo}.html\">x</a>")),
    )]);
    let h = comprobar(dir.path());
    assert_eq!(h.len(), 1);
    assert!(
        h[0].que.chars().count() < 600,
        "{}",
        h[0].que.chars().count()
    );
}

#[test]
fn un_ruido_de_paginas_no_hace_panico() {
    let piezas = [
        "<html>",
        "<head>",
        "</head>",
        "<body>",
        "</body>",
        "<meta charset=\"utf-8\">",
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'\">",
        "<meta name=\"referrer\" content=\"x\">",
        "<base href=\"x\">",
        "<script>",
        "</script>",
        "<script src=x>",
        "<a href=\"",
        "<a href=\"#",
        "<a href=\"https://github.com/",
        "<a href=\"../",
        "<a href=\"javascript:",
        "\">",
        "<img src=\"",
        "<p style=\"",
        "onclick=\"",
        "<svg>",
        "</svg>",
        "<!--",
        "-->",
        "<",
        ">",
        "&",
        "&amp;",
        "'",
        "\"",
        "é",
        "🙂",
        " ",
        "\n",
        "\r\n",
        "\0",
        "index.html",
        "x/",
        "../../",
        "id=\"a\"",
        "<h1 id=\"a\">",
    ];
    let mut estado: u64 = 0x1234_5678_9ABC_DEF1;
    let mut siguiente = move || {
        estado = estado
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (estado >> 33) as usize
    };
    for _ in 0..40 {
        let dir = tempfile::tempdir().unwrap();
        escribir(
            dir.path(),
            "index.html",
            pagina_propia("", "<h1 id=\"a\">x</h1>").as_bytes(),
        );
        for i in 0..12 {
            let n = 1 + siguiente() % 25;
            let mut s = String::new();
            for _ in 0..n {
                s.push_str(piezas[siguiente() % piezas.len()]);
            }
            let ruta = if i % 3 == 0 {
                format!("manual/p{i}.html")
            } else {
                format!("p{i}/index.html")
            };
            escribir(dir.path(), &ruta, s.as_bytes());
        }
        let h =
            comprobar_sitio(dir.path(), &ListaPermitida::por_defecto()).expect("no debe fallar");
        for x in &h {
            assert!(!x.to_string().chars().any(|c| c.is_control()), "{x:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Revisión adversarial
// ---------------------------------------------------------------------------

#[test]
fn un_refresh_escrito_con_entidades_se_detecta_igual() {
    // El navegador descodifica el valor: `re&#102;resh` es `refresh`.
    let dir = sitio(&[
        (
            "a/index.html",
            &pagina_propia(
                "<meta http-equiv=\"re&#102;resh\" content=\"0; url=https://magi.erratia.com/\">\n",
                "<p>x</p>",
            ),
        ),
        (
            "b/index.html",
            &pagina_propia(
                "<meta http-equiv=\"&#x72;&#101;fresh\" content=\"0\">\n",
                "<p>x</p>",
            ),
        ),
        (
            "c/index.html",
            &pagina_propia(
                "<meta http-equiv=\"re&desconocida;\" content=\"0\">\n",
                "<p>x</p>",
            ),
        ),
        (
            "d/index.html",
            &pagina_propia(
                "<meta http-equiv=\"x-ua-comp&#97;tible\" content=\"ie=edge\">\n",
                "<p>x</p>",
            ),
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(
        &h,
        "a/index.html",
        "<meta http-equiv=\"refresh\"> no permitido",
    );
    debe_haber(
        &h,
        "b/index.html",
        "<meta http-equiv=\"refresh\"> no permitido",
    );
    debe_haber(
        &h,
        "c/index.html",
        "referencia de caracteres que no se entiende",
    );
    assert!(de_fichero(&h, "d/index.html").is_empty(), "{}", listar(&h));
}

#[test]
fn un_script_escondido_bajo_un_elemento_de_texto_no_pasa_inadvertido() {
    // Para el navegador el `<script>` de después de `</title>` se ejecuta; si se
    // leyera `<!--` como comentario, no se vería. Falla cerrado.
    for (i, cabeza) in [
        "<title><!--</title><script>alert(1)</script>--></title>",
        "<textarea><!--</textarea><script>alert(1)</script>--></textarea>",
        "<noscript><!--</noscript><script>alert(1)</script>--></noscript>",
    ]
    .iter()
    .enumerate()
    {
        let crudo = format!(
            "<!DOCTYPE html>\n<html lang=\"es\">\n<head>\n<meta charset=\"utf-8\">\n{cabeza}\n</head>\n<body><p>x</p></body>\n</html>\n"
        );
        assert!(insertar(&crudo, false).is_err(), "{cabeza}");
        let dir = sitio(&[("index.html", &crudo)]);
        let h = comprobar(dir.path());
        debe_haber(&h, "index.html", "HTML que no se puede interpretar");
        assert_eq!(h.len(), 1, "caso {i}: {}", listar(&h));
    }
}

#[test]
fn un_nombre_de_fichero_con_controles_no_inventa_lineas_del_registro() {
    let dir = sitio(&[
        ("a\n::error::falso\u{1b}[31m/index.html", "<html></html>"),
        ("b\r::warning::otro.html", "<html></html>"),
    ]);
    let h = comprobar(dir.path());
    assert!(h.len() >= 2, "{}", listar(&h));
    for x in &h {
        let texto = x.to_string();
        assert!(
            !texto.chars().any(|c| c.is_control()),
            "control en {texto:?}"
        );
    }
    assert!(h
        .iter()
        .any(|x| x.fichero.starts_with("a\\n::error::falso")));
}

#[test]
fn un_svg_enorme_no_se_lee_entero() {
    let dir = tempfile::tempdir().unwrap();
    escribir(
        dir.path(),
        "index.html",
        pagina_propia("", "<p>x</p>").as_bytes(),
    );
    // Disperso: no ocupa disco ni se llega a leer.
    let f = fs::File::create(dir.path().join("grande.svg")).unwrap();
    f.set_len(33 * 1024 * 1024).unwrap();
    drop(f);
    let h = comprobar(dir.path());
    debe_haber(&h, "grande.svg", "demasiado grande");
}

#[test]
fn un_svg_con_manejadores_o_javascript_se_detecta_aunque_no_lleve_script() {
    // Abierto directamente, `<svg onload>` ejecuta código sin ningún `<script>`.
    let dir = sitio(&[
        ("a.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\" onload=\"alert(1)\"/>"),
        ("b.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect OnClick = 'x()'/></svg>"),
        (
            "c.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><a href=\"JavaScript:alert(1)\"><rect/></a></svg>",
        ),
        ("d.xml", "<x><y\nonmouseover\n=\"1\"/></x>"),
        // Palabras que contienen «on» y letras seguidas de `=` que no son manejadores.
        (
            "e.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" fill=\"none\" \
             stroke-width=\"2\" font-family=\"Mono\"><title>una conexion</title><path d=\"M0 0\"/></svg>",
        ),
    ]);
    let h = comprobar(dir.path());
    debe_haber(&h, "a.svg", "manejador de eventos");
    debe_haber(&h, "b.svg", "manejador de eventos");
    debe_haber(&h, "c.svg", "javascript:");
    debe_haber(&h, "d.xml", "manejador de eventos");
    assert!(de_fichero(&h, "e.svg").is_empty(), "{}", listar(&h));
}
