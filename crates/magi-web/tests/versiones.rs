//! Pruebas de `versiones` (de los CHANGELOG a HTML saneado) y de la página de
//! versiones, con los CHANGELOG inventados de `tests/fijos/changelog/`, los
//! CHANGELOG reales del repositorio y las plantillas y los textos reales de `web/`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use magi_web::comprobar::ListaPermitida;
use magi_web::html::{self, Etiqueta, Token};
use magi_web::paginas::versiones::{renderizar, DatosVersiones};
use magi_web::paginas::Entorno;
use magi_web::plantillas::Plantillas;
use magi_web::rutas::Idioma;
use magi_web::textos::Textos;
use magi_web::versiones::{
    detalle_a_html, entradas_de_changelog, EntradaChangelog, TOPE_CHANGELOG, TOPE_ENTRADA,
};
use magi_web::{datos::Plataformas, ErrorWeb};

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fijo(nombre: &str) -> String {
    let ruta = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fijos/changelog")
        .join(nombre);
    std::fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("{}: {e}", ruta.display()))
}

fn parsear(nombre: &str) -> Vec<EntradaChangelog> {
    entradas_de_changelog(&fijo(nombre), nombre, "v1.2.0")
        .unwrap_or_else(|e| panic!("{nombre}: {e}"))
}

fn entrada<'a>(entradas: &'a [EntradaChangelog], numero: &str) -> &'a EntradaChangelog {
    entradas
        .iter()
        .find(|e| e.numero == numero)
        .unwrap_or_else(|| panic!("no hay la versión {numero}"))
}

/// El mensaje de un error (`Display`).
fn mensaje<T: std::fmt::Debug>(r: Result<T, ErrorWeb>) -> String {
    match r {
        Ok(v) => panic!("debía fallar y dio {v:?}"),
        Err(e) => e.to_string(),
    }
}

fn etiquetas(html: &str) -> Vec<Etiqueta> {
    html::tokenizar(html)
        .unwrap_or_else(|e| panic!("HTML ilegible: {e}\n{html}"))
        .into_iter()
        .filter_map(|t| match t {
            Token::Etiqueta(e) => Some(e),
            _ => None,
        })
        .collect()
}

/// Los `href` de un HTML, descodificados.
fn hrefs(html: &str) -> Vec<String> {
    etiquetas(html)
        .iter()
        .filter(|e| !e.cierre && e.nombre == "a")
        .map(|e| {
            html::decodificar_atributo(e.atributo("href").expect("un <a> sin href"))
                .expect("href descodificable")
        })
        .collect()
}

/// El texto que se ve (sin etiquetas, con las entidades como están).
fn texto_visible(html: &str) -> String {
    html::tokenizar(html)
        .expect("HTML legible")
        .into_iter()
        .filter_map(|t| match t {
            Token::Texto { inicio, fin } => Some(&html[inicio..fin]),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Lo único que `detalle_a_html` puede escribir.
const PERMITIDAS: &[&str] = &[
    "p",
    "ul",
    "ol",
    "li",
    "em",
    "strong",
    "del",
    "code",
    "pre",
    "a",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "br",
    "hr",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
];

/// Comprueba que el HTML solo lleva etiquetas permitidas, sin más atributo que el
/// `href` `https://` de los enlaces ni más `start` que cifras.
fn comprobar_saneado(html: &str) {
    for e in etiquetas(html) {
        assert!(
            PERMITIDAS.contains(&e.nombre.as_str()),
            "etiqueta <{}> no permitida en:\n{html}",
            e.nombre
        );
        if e.cierre {
            assert!(e.atributos.is_empty(), "</{}> con atributos", e.nombre);
            continue;
        }
        for (nombre, valor) in &e.atributos {
            match (e.nombre.as_str(), nombre.as_str()) {
                ("a", "href") => {
                    let v = html::decodificar_atributo(valor.as_deref().unwrap_or("")).unwrap();
                    assert!(v.starts_with("https://"), "href no https: {v}");
                }
                ("ol", "start") => {
                    assert!(valor
                        .as_deref()
                        .unwrap_or("x")
                        .bytes()
                        .all(|b| b.is_ascii_digit()));
                }
                otro => panic!("atributo no permitido {otro:?} en:\n{html}"),
            }
        }
    }
}

/// Profundidad máxima de anidamiento de elementos de un HTML.
fn profundidad(html: &str) -> usize {
    const VACIAS: &[&str] = &["br", "hr"];
    let mut actual = 0usize;
    let mut maxima = 0usize;
    for e in etiquetas(html) {
        if VACIAS.contains(&e.nombre.as_str()) {
            continue;
        }
        if e.cierre {
            actual = actual.saturating_sub(1);
        } else {
            actual += 1;
            maxima = maxima.max(actual);
        }
    }
    maxima
}

/// Un CHANGELOG de una versión con el cuerpo dado tras el resumen.
fn changelog_con(cuerpo: &str) -> String {
    format!(
        "# Registro\n\n## [1.0.0] - 2026-11-22\n\n**Resumen.** Un resumen.\n\n**Summary.** A summary.\n\n{cuerpo}\n"
    )
}

fn detalle_de(cuerpo: &str) -> String {
    let entradas = entradas_de_changelog(&changelog_con(cuerpo), "prueba", "v1.0.0")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(entradas.len(), 1);
    entradas[0].detalle_html.clone()
}

// ---------------------------------------------------------------------------
// Lectura del fichero
// ---------------------------------------------------------------------------

#[test]
fn la_seccion_sin_publicar_y_los_comentarios_no_salen() {
    let entradas = parsear("dos_versiones.md");
    let numeros: Vec<&str> = entradas.iter().map(|e| e.numero.as_str()).collect();
    assert_eq!(
        numeros,
        ["1.2.0", "1.1.0"],
        "de la más nueva a la más vieja"
    );
    assert_eq!(entradas[0].fecha, "2026-11-22");
    assert_eq!(entradas[1].fecha, "2026-10-01");

    for e in &entradas {
        let todo = format!(
            "{} {} {} {}",
            e.resumen_es, e.resumen_en, e.detalle_html, e.numero
        );
        for fuera in [
            "todavía no ha salido",
            "Sin publicar",
            "Plantilla de una versión",
            "X.Y.Z",
            "comentario de una sola línea",
            "Una o dos líneas",
            "One or two lines",
            "compare",
            "<!--",
        ] {
            assert!(!todo.contains(fuera), "«{fuera}» no debía salir en {todo}");
        }
    }
    // Las definiciones de enlace del final no salen como texto suelto.
    let visible = texto_visible(&entradas[1].detalle_html);
    assert!(!visible.contains("]:"), "{visible}");
    assert!(!visible.contains("releases/tag"), "{visible}");

    // Un comentario puede esconder un encabezado de versión entero.
    let con_comentario = "## [2.0.0] - 2026-11-22\n\n**Resumen.** R.\n\n**Summary.** S.\n\n<!--\n## [1.9.0] - 2026-11-01\n\n**Resumen.** Oculta.\n-->\nTexto.\n";
    let e = entradas_de_changelog(con_comentario, "c", "v1.0.0").unwrap();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].numero, "2.0.0");
    assert!(!e[0].detalle_html.contains("Oculta"));
    assert!(e[0].detalle_html.contains("Texto."));

    // «Sin publicar» en medio no se mete en la versión de encima ni en la de debajo.
    let en_medio = "## [2.0.0] - 2026-11-22\n\n**Resumen.** R2.\n\n**Summary.** S2.\n\nCuerpo dos.\n\n## [Sin publicar]\n\nPendiente de salir.\n\n### Añadido\n\n- Algo.\n\n## [1.0.0] - 2026-10-01\n\n**Resumen.** R1.\n\n**Summary.** S1.\n\nCuerpo uno.\n";
    let e = entradas_de_changelog(en_medio, "c", "v1.0.0").unwrap();
    assert_eq!(e.len(), 2);
    for v in &e {
        assert!(!v.detalle_html.contains("Pendiente"), "{}", v.detalle_html);
        assert!(!v.detalle_html.contains("Algo."), "{}", v.detalle_html);
    }
    assert!(e[0].detalle_html.contains("Cuerpo dos."));
    assert!(e[1].detalle_html.contains("Cuerpo uno."));

    // Un fichero sin versiones da una lista vacía (la página lo trata).
    for texto in [
        "",
        "# Registro\n\n## [Sin publicar]\n\n- Algo.\n",
        "Solo un texto.\n",
    ] {
        assert_eq!(
            entradas_de_changelog(texto, "c", "v1.0.0").unwrap(),
            Vec::new(),
            "{texto:?}"
        );
    }

    // Un comentario sin cerrar es un error, no se traga el resto.
    let sin_cerrar = "## [1.0.0] - 2026-11-22\n\n**Resumen.** R.\n\n**Summary.** S.\n\n<!-- se abre y no se cierra\n\n## [0.9.0] - 2026-01-01\n";
    let m = mensaje(entradas_de_changelog(sin_cerrar, "CHANGELOG.md", "v1.0.0"));
    assert!(m.contains("sin cerrar"), "{m}");
    assert!(m.contains("CHANGELOG.md"), "{m}");
    assert!(m.contains("línea 7"), "debe decir dónde: {m}");
}

#[test]
fn cada_version_trae_resumen_y_summary_o_falla_diciendo_cual() {
    // Resumen y Summary en una o varias líneas, como texto plano.
    let entradas = parsear("dos_versiones.md");
    assert_eq!(
        entrada(&entradas, "1.2.0").resumen_es,
        "Segunda versión de DEMO: añade los túneles y mejora la conexión (un resumen en dos líneas)."
    );
    assert_eq!(
        entrada(&entradas, "1.2.0").resumen_en,
        "Second DEMO release: adds tunnels and improves the connection (a two-line summary)."
    );
    // Y no se repiten en el detalle.
    let detalle = &entrada(&entradas, "1.2.0").detalle_html;
    assert!(!detalle.contains("Segunda versión"), "{detalle}");
    assert!(!detalle.contains("Second DEMO"), "{detalle}");
    assert!(!detalle.contains("Resumen."), "{detalle}");

    // Falta el Summary.
    let m = mensaje(entradas_de_changelog(
        &fijo("sin_summary.md"),
        "sin_summary.md",
        "v1.2.0",
    ));
    assert!(m.contains("1.1.0"), "dice la versión: {m}");
    assert!(m.contains("Summary"), "dice cuál falta: {m}");
    assert!(m.contains("sin_summary.md"), "dice el origen: {m}");
    assert!(
        !m.contains("falta `**Resumen.**`"),
        "el Resumen sí está: {m}"
    );

    // Falta el Resumen.
    let m = mensaje(entradas_de_changelog(
        &fijo("sin_resumen.md"),
        "sin_resumen.md",
        "v1.2.0",
    ));
    assert!(m.contains("1.1.0") && m.contains("Resumen"), "{m}");
    assert!(m.contains("falta `**Resumen.**`"), "{m}");
    assert!(
        !m.contains("falta `**Summary.**`"),
        "el Summary sí está: {m}"
    );

    // Faltan los dos: lo dice.
    let m = mensaje(entradas_de_changelog(
        "## [3.4.5] - 2026-01-02\n\nSolo texto.\n",
        "otro.md",
        "v1.2.0",
    ));
    assert!(m.contains("3.4.5") && m.contains("otro.md"), "{m}");
    assert!(
        m.contains("falta `**Resumen.**`") && m.contains("falta `**Summary.**`"),
        "{m}"
    );

    // Un resumen vacío no vale; ni un marcador a mitad de línea.
    let m = mensaje(entradas_de_changelog(
        "## [1.0.0] - 2026-01-02\n\n**Resumen.**\n\n**Summary.** S.\n",
        "v.md",
        "v1.0.0",
    ));
    assert!(m.contains("vacío"), "{m}");
    let m = mensaje(entradas_de_changelog(
        "## [1.0.0] - 2026-01-02\n\nTexto **Resumen.** a mitad.\n\n**Summary.** S.\n",
        "v.md",
        "v1.0.0",
    ));
    assert!(m.contains("falta `**Resumen.**`"), "{m}");

    // El fallo de una versión de en medio para todo el fichero, aunque las demás estén bien.
    let m = mensaje(entradas_de_changelog(
        "## [2.0.0] - 2026-02-01\n\n**Resumen.** R.\n\n**Summary.** S.\n\n## [1.5.0] - 2026-01-02\n\nNada.\n\n## [1.0.0] - 2026-01-01\n\n**Resumen.** R.\n\n**Summary.** S.\n",
        "v.md",
        "v1.0.0",
    ));
    assert!(m.contains("1.5.0"), "{m}");

    // Los dos párrafos pueden estar en cualquier orden y juntos: cada uno acaba donde empieza el otro.
    let e = entradas_de_changelog(
        "## [1.0.0] - 2026-01-02\n\n**Summary.** S uno\ny dos.\n**Resumen.** R uno\ny dos.\n\nDetalle.\n",
        "v.md",
        "v1.0.0",
    )
    .unwrap();
    assert_eq!(e[0].resumen_en, "S uno y dos.");
    assert_eq!(e[0].resumen_es, "R uno y dos.");
    assert!(e[0].detalle_html.contains("Detalle."));
    assert!(!e[0].detalle_html.contains("uno"));

    // El resumen es texto plano: sin marcas y con el HTML como texto.
    let e = entradas_de_changelog(
        "## [1.0.0] - 2026-01-02\n\n**Resumen.** Con `código`, **negrita**, [un enlace](https://ejemplo.org/) y <b>html</b>.\n\n**Summary.** S.\n",
        "v.md",
        "v1.0.0",
    )
    .unwrap();
    assert_eq!(
        e[0].resumen_es,
        "Con código, negrita, un enlace y <b>html</b>."
    );
    assert_eq!(e[0].detalle_html, "", "sin detalle, sin HTML");

    // Un resumen enorme no vale.
    let enorme = format!(
        "## [1.0.0] - 2026-01-02\n\n**Resumen.** {}\n\n**Summary.** S.\n",
        "palabra ".repeat(1000)
    );
    assert!(mensaje(entradas_de_changelog(&enorme, "v.md", "v1.0.0")).contains("pasa de"));
}

#[test]
fn un_numero_repetido_falla() {
    let m = mensaje(entradas_de_changelog(
        &fijo("numero_repetido.md"),
        "numero_repetido.md",
        "v1.2.0",
    ));
    assert!(m.contains("1.1.0"), "{m}");
    assert!(m.contains("más de una vez"), "{m}");
    assert!(m.contains("numero_repetido.md"), "{m}");
    assert!(m.contains("línea 15"), "dice dónde: {m}");
}

#[test]
fn una_fecha_imposible_falla() {
    let cuerpo = "\n**Resumen.** R.\n\n**Summary.** S.\n";
    // Cuatro números, escrito así para no parecer una dirección IP.
    let cuatro = format!("## [{}] - 2026-11-22", ["1", "0", "0", "0"].join("."));
    for (encabezado, que) in [
        ("## [1.0.0] - 2026-13-01", "fecha"),
        ("## [1.0.0] - 2026-00-10", "fecha"),
        ("## [1.0.0] - 2026-11-32", "fecha"),
        ("## [1.0.0] - 2026-11-00", "fecha"),
        ("## [1.0.0] - 2026-02-30", "fecha"),
        ("## [1.0.0] - 2025-02-29", "fecha"),
        ("## [1.0.0] - 0000-01-01", "fecha"),
        ("## [1.0.0] - 2026-1-5", "fecha"),
        ("## [1.0.0] - ayer", "fecha"),
        ("## [1.0.0] - 2026-11-22 (retirada)", "fecha"),
        ("## [1.0.0]", "AAAA-MM-DD"),
        ("## [1.0.0] 2026-11-22", "AAAA-MM-DD"),
        ("## [01.0.0] - 2026-11-22", "número"),
        ("## [1.00.0] - 2026-11-22", "número"),
        ("## [1.0.0-rc.1] - 2026-11-22", "número"),
        ("## [1.0] - 2026-11-22", "número"),
        (cuatro.as_str(), "número"),
        ("## [10000.0.0] - 2026-11-22", "número"),
        ("## [1.0.x] - 2026-11-22", "número"),
        ("## [1.0.0 - 2026-11-22", "]"),
    ] {
        let texto = format!("# Registro\n\n{encabezado}\n{cuerpo}");
        let m = mensaje(entradas_de_changelog(&texto, "CHANGELOG.md", "v1.0.0"));
        assert!(m.contains(que), "{encabezado}: {m}");
        assert!(m.contains("línea 3"), "{encabezado}: dice dónde: {m}");
        assert!(m.contains("CHANGELOG.md"), "{encabezado}: {m}");
    }
    // Un 29 de febrero que sí existe, y un espacio sobrante al final, valen.
    for bien in [
        "## [1.0.0] - 2024-02-29",
        "## [1.0.0] - 2026-11-22   ",
        "## [0.0.1] - 2026-12-31",
    ] {
        let texto = format!("{bien}\n{cuerpo}");
        assert_eq!(
            entradas_de_changelog(&texto, "c", "v1.0.0").unwrap().len(),
            1,
            "{bien}"
        );
    }
    // Otros `##` no son versiones: se quedan en el cuerpo (como encabezado de detalle).
    let texto = format!("## [1.0.0] - 2026-11-22\n{cuerpo}\n## Notas\n\nUna nota.\n");
    let e = entradas_de_changelog(&texto, "c", "v1.0.0").unwrap();
    assert!(e[0].detalle_html.contains("<h4>Notas</h4>"), "{e:?}");
    // La etiqueta tiene que ser de una versión publicada.
    for etiqueta in ["main", "v1.0", "v1.0.0-rc.1", "", "android-1.0.0"] {
        assert!(
            entradas_de_changelog("", "c", etiqueta).is_err(),
            "{etiqueta:?}"
        );
    }
}

#[test]
fn los_saltos_de_linea_raros_y_el_bom_se_normalizan() {
    let base = fijo("dos_versiones.md");
    let esperado = entradas_de_changelog(&base, "c", "v1.2.0").unwrap();
    let con_crlf = base.replace('\n', "\r\n");
    assert_eq!(
        entradas_de_changelog(&con_crlf, "c", "v1.2.0").unwrap(),
        esperado
    );
    let con_cr = base.replace('\n', "\r");
    assert_eq!(
        entradas_de_changelog(&con_cr, "c", "v1.2.0").unwrap(),
        esperado
    );
    let con_bom = format!("\u{feff}{base}");
    assert_eq!(
        entradas_de_changelog(&con_bom, "c", "v1.2.0").unwrap(),
        esperado
    );
}

#[test]
fn un_bloque_de_codigo_esconde_los_encabezados_y_uno_sin_cerrar_falla() {
    // Un encabezado dentro de un bloque cerrado es código.
    let e = parsear("patologico.md");
    assert_eq!(e.len(), 1, "no hay una versión 9.9.9 dentro del bloque");
    assert!(e[0].detalle_html.contains("## [9.9.9] - 2026-12-31"));
    assert!(e[0].detalle_html.contains("Y texto después del bloque."));

    // Una valla de `~~~` y una más larga que su cierre.
    let texto = "## [1.0.0] - 2026-11-22\n\n**Resumen.** R.\n\n**Summary.** S.\n\n~~~\n## [0.9.0] - 2026-01-01\n~~~\n\n````\n```\n## [0.8.0] - 2026-01-01\n```\n````\n\nFin.\n";
    let e = entradas_de_changelog(texto, "c", "v1.0.0").unwrap();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].detalle_html.contains("Fin."));

    // Sin cerrar: error con su línea, no se traga las versiones siguientes.
    let sin_cerrar = "## [2.0.0] - 2026-11-22\n\n**Resumen.** R.\n\n**Summary.** S.\n\n```\ncódigo\n\n## [1.0.0] - 2026-01-01\n\n**Resumen.** R.\n\n**Summary.** S.\n";
    let m = mensaje(entradas_de_changelog(sin_cerrar, "CHANGELOG.md", "v1.0.0"));
    assert!(m.contains("sin cerrar"), "{m}");
    assert!(m.contains("línea 7"), "{m}");

    // Una valla con sangría (dentro de una lista) no esconde un encabezado en la
    // columna 0: GitHub la cierra.
    let en_lista = "## [2.0.0] - 2026-11-22\n\n**Resumen.** R.\n\n**Summary.** S.\n\n- elemento\n  ```\n  código\n\n## [1.0.0] - 2026-01-01\n\n**Resumen.** R.\n\n**Summary.** S.\n";
    let e = entradas_de_changelog(en_lista, "c", "v1.0.0").unwrap();
    assert_eq!(e.len(), 2, "{e:?}");

    // Una línea de acentos graves con texto detrás en la misma línea no es una valla.
    let en_linea =
        "## [1.0.0] - 2026-11-22\n\n**Resumen.** R.\n\n**Summary.** S.\n\n```código``` en línea.\n";
    assert_eq!(
        entradas_de_changelog(en_linea, "c", "v1.0.0")
            .unwrap()
            .len(),
        1
    );
}

// ---------------------------------------------------------------------------
// Seguridad del detalle
// ---------------------------------------------------------------------------

#[test]
fn el_html_en_bruto_se_escapa_como_texto() {
    let entradas = parsear("html_en_bruto.md");
    let detalle = &entrada(&entradas, "1.0.0").detalle_html;

    // Nada de HTML vivo: solo las etiquetas que escribe el conversor.
    comprobar_saneado(detalle);
    for vivo in ["<script", "<img", "<div", "<a href=\"javascript"] {
        assert!(!detalle.contains(vivo), "«{vivo}» vivo en:\n{detalle}");
    }
    // Se ve como texto, escapado.
    let visible = texto_visible(detalle);
    assert!(
        visible.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{visible}"
    );
    assert!(
        visible.contains("&lt;img src=x onerror=alert(1)&gt;"),
        "{visible}"
    );
    assert!(
        visible.contains("&lt;div onclick=&quot;alert(3)&quot;&gt;"),
        "{visible}"
    );
    // Una entidad ya escrita en el CHANGELOG no se deshace en HTML vivo.
    assert!(
        visible.contains("&lt;script&gt;alert(4)&lt;/script&gt; y &amp;lt;"),
        "{visible}"
    );

    // El resumen es texto plano con el HTML tal cual: lo escapa la plantilla.
    let e = entrada(&entradas, "1.0.0");
    assert_eq!(
        e.resumen_es,
        "Versión con <b>HTML</b> en el resumen y un código."
    );
    assert_eq!(e.resumen_en, "Release with <i>HTML</i> in the summary.");

    // Las imágenes pasan a su texto alternativo.
    assert!(!detalle.contains("<img"));
    assert!(visible.contains("texto alternativo"));
    assert!(!detalle.contains("imagen.png"));

    // HTML en bruto suelto, de bloque y en línea, en un CHANGELOG nuevo.
    for crudo in [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<iframe src=\"https://ejemplo.org\"></iframe>",
        "<style>body{display:none}</style>",
        "<svg onload=alert(1)>",
        "<details open ontoggle=alert(1)>",
        "<a href=\"javascript:alert(1)\">x</a>",
        "<!DOCTYPE html>",
        "<![CDATA[x]]>",
        "<?php echo 1; ?>",
        "<p style=\"color:red\">x</p>",
        "<form action=\"https://ejemplo.org\"><input></form>",
        "Texto <b>negrita</b> en línea.",
    ] {
        let html = detalle_de(crudo);
        comprobar_saneado(&html);
        assert!(!html.contains("<script"), "{crudo}: {html}");
        assert!(!html.contains("<iframe"), "{crudo}: {html}");
        assert!(!html.contains("<svg"), "{crudo}: {html}");
        assert!(!html.contains("<style"), "{crudo}: {html}");
        assert!(!html.contains("<form"), "{crudo}: {html}");
        assert!(!html.contains("<b>"), "{crudo}: {html}");
        assert!(!html.contains("<input"), "{crudo}: {html}");
        assert!(html.contains("&lt;"), "{crudo}: {html}");
    }

    // Comillas y signos dentro del texto y de los enlaces.
    let html = detalle_de("a \"b\" 'c' & d [e\"f](https://ejemplo.org/a?x=1&y=\"2\")");
    comprobar_saneado(&html);
    assert!(html.contains("&quot;b&quot;"), "{html}");
    assert!(html.contains("&#x27;c&#x27;"), "{html}");
    assert!(html.contains("&amp; d"), "{html}");
}

#[test]
fn javascript_data_y_http_no_son_enlaces() {
    let entradas = parsear("html_en_bruto.md");
    let detalle = &entrada(&entradas, "1.0.0").detalle_html;
    let enlaces = hrefs(detalle);
    assert!(!enlaces.is_empty());
    for h in &enlaces {
        assert!(h.starts_with("https://"), "href que no es https: {h}");
        for malo in [
            "javascript",
            "data:",
            "vbscript",
            "mailto",
            "file:",
            "http://",
        ] {
            assert!(!h.to_lowercase().contains(malo), "{h}");
        }
    }
    // Los que no son enlace dejan su texto.
    let visible = texto_visible(detalle);
    for texto in [
        "click",
        "mayúsculas",
        "entidad",
        "numérica",
        "tabulador",
        "datos",
        "vb",
        "inseguro",
        "correo",
        "fichero",
        "protocolo",
        "solo ancla",
        "con usuario",
        "puerto",
        "sube demasiado",
        "una referencia mala",
    ] {
        assert!(visible.contains(texto), "falta el texto «{texto}»");
    }
    // Y los buenos siguen siendo enlaces.
    assert!(enlaces.contains(&"https://ejemplo.org/ruta".to_string()));
    assert!(enlaces.contains(&"https://ejemplo.org/autoenlace".to_string()));

    // Una tabla de destinos hostiles, por todas las formas de escribir un enlace.
    for destino in [
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "JaVaScRiPt:alert(1)",
        "javascript&colon;alert(1)",
        "&#106;avascript:alert(1)",
        "&#x6A;avascript:alert(1)",
        "java&#x09;script:alert(1)",
        "java&#x0A;script:alert(1)",
        "data:text/html;base64,PHNjcmlwdD4=",
        "DATA:text/html,x",
        "vbscript:msgbox(1)",
        "http://ejemplo.org/",
        "HTTP://ejemplo.org/",
        "ftp://ejemplo.org/",
        "file:///etc/passwd",
        "mailto:usuario@ejemplo.org",
        "tel:123",
        "//ejemplo.org/x",
        "///ejemplo.org/x",
        "/etc/passwd",
        "HTTPS://ejemplo.org/",
        "Https://ejemplo.org/",
        "https:ejemplo.org",
        "https:/ejemplo.org",
        "https://usuario:clave@ejemplo.org/",
        "https://ejemplo.org@malo.ejemplo.org/",
        "https://EJEMPLO.org/",
        "https://ejemplo.org:8080/",
        "https://ejemplo.org/a%2Fb",
        "https://ejemplo.org/../x",
        "https://ejemplo.org/%2e%2e/x",
        "https://ejemplo.org/\u{1}x",
        "https://ejemplo.org/ñ",
        "x:y",
        "#ancla",
        "#",
        "?x=1",
        "../../../etc/passwd",
        "..",
        "docs\\\\x.md",
    ] {
        for forma in [
            format!("[texto]({destino})"),
            format!("[texto](<{destino}>)"),
            format!("[texto][r]\n\n[r]: {destino}"),
            format!("[texto][]\n\n[texto]: {destino}"),
            format!("[texto]\n\n[texto]: {destino}"),
            format!("<{destino}>"),
        ] {
            let html = detalle_de(&forma);
            comprobar_saneado(&html);
            for h in hrefs(&html) {
                // Si hay enlace, es https:// bueno o una ruta del repositorio en GitHub.
                assert!(
                    h.starts_with("https://github.com/HectorEspejo/magi/")
                        || h == "https://ejemplo.org/",
                    "{forma:?} dio el enlace {h:?}:\n{html}"
                );
            }
            assert!(
                !html.to_lowercase().contains("href=\"javascript"),
                "{forma}"
            );
            assert!(!html.to_lowercase().contains("href=\"data"), "{forma}");
        }
    }

    // Los de un solo carácter sospechoso, uno a uno: el destino queda sin enlace.
    for destino in [
        "javascript:alert(1)",
        "http://ejemplo.org/",
        "data:text/html,x",
        "https://usuario:clave@ejemplo.org/",
        "HTTPS://ejemplo.org/",
    ] {
        let html = detalle_de(&format!("[texto]({destino})"));
        assert_eq!(html, "<p>texto</p>\n", "{destino}");
    }
}

#[test]
fn los_enlaces_relativos_pasan_a_github_en_la_etiqueta() {
    // El CHANGELOG 1.6.1 trae `](docs/formato-sincronizacion.md)`.
    let h = detalle_a_html(
        "Mira [el formato](docs/formato-sincronizacion.md) y el [manual](docs/manual/).",
        "v1.6.1",
        "prueba",
    )
    .unwrap();
    assert_eq!(
        hrefs(&h),
        [
            "https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md",
            "https://github.com/HectorEspejo/magi/tree/v1.6.1/docs/manual/",
        ]
    );

    // Con ancla, `./`, consulta y rutas con letras no ASCII.
    let h = detalle_a_html(
        "[a](./docs/x.md#seccion-uno) [b](docs/x.md?plain=1) [c](docs/túnel.md) [d](src/lib.rs#L10-L20)",
        "v1.6.1",
        "prueba",
    )
    .unwrap();
    assert_eq!(
        hrefs(&h),
        [
            "https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/x.md#seccion-uno",
            "https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/x.md",
            "https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/t%C3%BAnel.md",
            "https://github.com/HectorEspejo/magi/blob/v1.6.1/src/lib.rs#L10-L20",
        ]
    );

    // La app lee los relativos desde `android/`.
    let h = detalle_a_html(
        "[publicar](../docs/publicar-android.md) [propio](CHANGELOG.md)",
        "android-v0.2.0",
        "prueba",
    )
    .unwrap();
    assert_eq!(
        hrefs(&h),
        [
            "https://github.com/HectorEspejo/magi/blob/android-v0.2.0/docs/publicar-android.md",
            "https://github.com/HectorEspejo/magi/blob/android-v0.2.0/android/CHANGELOG.md",
        ]
    );

    // Lo que se sale del repositorio o es raro no es un enlace.
    for malo in [
        "../../etc/passwd",
        "../fuera.md",
        "docs/../../fuera.md",
        "docs//x.md",
        "docs/a b.md",
        "docs/a%20b.md",
        "docs/a%2Fb.md",
        "docs/x.md#a..b",
        "/docs/x.md",
        "docs\\x.md",
        "docs/x;y.md",
        "docs/x*.md",
        "docs/a'b.md",
        "docs/<x>.md",
    ] {
        let h = detalle_a_html(&format!("[x](<{malo}>)"), "v1.6.1", "prueba").unwrap();
        let enlaces = hrefs(&h);
        // Una ancla rara se descarta pero la ruta sigue; todo lo demás se queda sin enlace.
        if malo == "docs/x.md#a..b" {
            assert_eq!(
                enlaces,
                ["https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/x.md"]
            );
        } else {
            assert!(enlaces.is_empty(), "{malo:?} dio {enlaces:?}");
        }
    }

    // Todos los que salen los acepta `comprobar`.
    let lista = ListaPermitida::por_defecto();
    for entrada in parsear("html_en_bruto.md") {
        for h in hrefs(&entrada.detalle_html) {
            if h.starts_with("https://github.com") {
                assert!(lista.permite(&h), "{h}");
            }
        }
    }
    for entrada in parsear("dos_versiones.md") {
        for h in hrefs(&entrada.detalle_html) {
            assert!(lista.permite(&h), "{h}");
        }
    }

    // Sin una etiqueta de versión no hay a dónde apuntar: falla en vez de adivinar.
    assert!(detalle_a_html("[x](docs/x.md)", "main", "prueba").is_err());
    assert!(detalle_a_html("[x](docs/x.md)", "v1.6.1-rc.1", "prueba").is_err());
}

#[test]
fn las_referencias_de_enlace_se_resuelven_con_el_mismo_criterio() {
    // Definiciones al final del fichero (en la última versión) valen para todas.
    let entradas = parsear("dos_versiones.md");
    let h = hrefs(&entrada(&entradas, "1.2.0").detalle_html);
    assert!(
        h.contains(&"https://github.com/HectorEspejo/magi/releases/tag/v1.1.0".to_string()),
        "{h:?}"
    );
    // La definición de la propia versión de debajo no sale como texto.
    let visible = texto_visible(&entrada(&entradas, "1.1.0").detalle_html);
    assert!(!visible.contains("https://"), "{visible}");

    // Las de html_en_bruto: la mala no es enlace, las buenas sí (la relativa, en GitHub).
    let entradas = parsear("html_en_bruto.md");
    let detalle = &entrada(&entradas, "1.0.0").detalle_html;
    let visible = texto_visible(detalle);
    assert!(visible.contains("una referencia mala"), "{visible}");
    assert!(!visible.contains("javascript:alert(12)"), "{visible}");
    assert!(!detalle.contains("alert(12)"), "{detalle}");
    let guia = "https://github.com/HectorEspejo/magi/blob/v1.2.0/docs/guia.md";
    let n_guia = hrefs(detalle).iter().filter(|h| *h == guia).count();
    // [buena], [BUENA] y [  buena  ] (más los dos «relativo» de arriba, uno con ancla).
    assert!(n_guia >= 4, "{n_guia} enlaces a la guía:\n{detalle}");
    assert!(hrefs(detalle).contains(&"https://ejemplo.org/abreviada".to_string()));
    // Lo que no está definido se queda como texto, sin que desaparezca.
    assert!(visible.contains("[sin definir][no-existe]"), "{visible}");
    assert!(visible.contains("[tampoco]"), "{visible}");
    // Las definiciones no aparecen en el texto.
    assert!(!visible.contains("[mala]:"), "{visible}");
    assert!(!visible.contains("[buena]:"), "{visible}");

    // Una referencia hacia una definición de otra versión: la definición manda
    // aunque esté más abajo en el fichero, y con el mismo criterio de destinos.
    let texto = "## [2.0.0] - 2026-11-22\n\n**Resumen.** R.\n\n**Summary.** S.\n\nVer [la guía][guia], [lo malo][malo] y [lo http][web].\n\n## [1.0.0] - 2026-10-01\n\n**Resumen.** R.\n\n**Summary.** S.\n\n[guia]: docs/guia.md\n[malo]: javascript:alert(1)\n[web]: http://ejemplo.org/\n";
    let e = entradas_de_changelog(texto, "c", "v2.0.0").unwrap();
    assert_eq!(
        hrefs(&e[0].detalle_html),
        ["https://github.com/HectorEspejo/magi/blob/v2.0.0/docs/guia.md"]
    );
    let visible = texto_visible(&e[0].detalle_html);
    assert!(visible.contains("lo malo") && visible.contains("lo http"));

    // Una definición dentro de un bloque de código es código, no una definición.
    let h = detalle_de("Ver [x][r].\n\n```\n[r]: https://ejemplo.org/\n```");
    assert!(hrefs(&h).is_empty(), "{h}");
}

#[test]
fn un_enlace_sin_texto_lleva_su_destino_como_nombre() {
    // Un enlace vacío no tiene nombre para un lector de pantalla: sale su destino.
    for (md, destino) in [
        ("[](https://ejemplo.org/a)", "https://ejemplo.org/a"),
        ("[ ](https://ejemplo.org/a)", "https://ejemplo.org/a"),
        (
            "[![](https://ejemplo.org/i.png)](https://ejemplo.org/a)",
            "https://ejemplo.org/a",
        ),
        (
            "[](https://ejemplo.org/a?x=1&y=2)",
            "https://ejemplo.org/a?x=1&amp;y=2",
        ),
    ] {
        let h = detalle_de(md);
        comprobar_saneado(&h);
        assert!(h.contains(&format!("{destino}</a>")), "{md:?} dio {h}");
        assert_eq!(hrefs(&h).len(), 1, "{h}");
    }
    // Con texto, con código o con el alt de una imagen, no se añade nada.
    for (md, esperado) in [
        (
            "[texto](https://ejemplo.org/a)",
            "<a href=\"https://ejemplo.org/a\">texto</a>",
        ),
        (
            "[`c`](https://ejemplo.org/a)",
            "<a href=\"https://ejemplo.org/a\"><code>c</code></a>",
        ),
        (
            "[![alt](https://ejemplo.org/i.png)](https://ejemplo.org/a)",
            "<a href=\"https://ejemplo.org/a\">alt</a>",
        ),
    ] {
        let h = detalle_de(md);
        assert!(h.contains(esperado), "{md:?} dio {h}");
    }
    // Un enlace que no es enlace (queda el texto) tampoco añade su destino.
    let h = detalle_de("[](javascript:alert(1)) y [a](http://ejemplo.org/)");
    assert!(!h.contains("javascript"), "{h}");
    assert_eq!(h, "<p> y a</p>\n");
    // Y cada enlace lleva su propia cuenta: el vacío no contagia al siguiente.
    let h = detalle_de("[](https://ejemplo.org/a) [b](https://ejemplo.org/b)");
    assert!(h.contains(">https://ejemplo.org/a</a>"), "{h}");
    assert!(h.contains(">b</a>"), "{h}");
    assert!(!h.contains(">https://ejemplo.org/b</a>"), "{h}");
}

#[test]
fn un_uuid_suelto_sale_como_texto() {
    let h = detalle_de(
        "- Sale `magi-<uuid>.age` y también «magi-<uuid>.age: a medio copiar» y <uuid> suelto.\n- Otra `<fecha>` y <https://ejemplo.org/a> y <a@b>.",
    );
    comprobar_saneado(&h);
    let visible = texto_visible(&h);
    assert!(visible.contains("magi-&lt;uuid&gt;.age"), "{visible}");
    assert!(
        visible.contains("«magi-&lt;uuid&gt;.age: a medio copiar»"),
        "{visible}"
    );
    assert!(visible.contains("&lt;uuid&gt; suelto"), "{visible}");
    assert!(visible.contains("&lt;fecha&gt;"), "{visible}");
    assert!(!h.contains("<uuid"), "{h}");
    assert!(h.contains("<code>magi-&lt;uuid&gt;.age</code>"), "{h}");
    // El autoenlace de verdad sigue siendo enlace; el correo, no.
    assert_eq!(hrefs(&h), ["https://ejemplo.org/a"]);
    assert!(visible.contains("a@b"), "{visible}");

    // También lo que sale en una entrada completa, con el CHANGELOG real si lo trae.
    let real = std::fs::read_to_string(raiz().join("CHANGELOG.md")).unwrap();
    let entradas = entradas_de_changelog(&real, "CHANGELOG.md", "v1.6.1").unwrap();
    for e in &entradas {
        assert!(!e.detalle_html.contains("<uuid"), "{}", e.numero);
        assert!(!e.detalle_html.contains("<fecha"), "{}", e.numero);
    }
}

#[test]
fn los_encabezados_bajan_de_nivel_y_no_llevan_id() {
    let h = detalle_de("# Uno\n\n## Dos\n\n### Tres\n\n#### Cuatro\n\n##### Cinco\n\n###### Seis");
    for (esperado, no_esperado) in [
        ("<h4>Uno</h4>", "<h1"),
        ("<h4>Dos</h4>", "<h2"),
        ("<h4>Tres</h4>", "<h3"),
        ("<h5>Cuatro</h5>", "<h4>Cuatro"),
        ("<h6>Cinco</h6>", "<h5>Cinco"),
        ("<h6>Seis</h6>", "<h6>Seis</h6>x"),
    ] {
        assert!(h.contains(esperado), "{esperado} en {h}");
        assert!(!h.contains(no_esperado), "{no_esperado} en {h}");
    }
    assert!(!h.contains("id="), "{h}");
    // Ni con atributos de encabezado escritos a mano.
    let h = detalle_de("### Titulo {#mi-id .clase}");
    assert!(!h.contains("id=") && !h.contains("class="), "{h}");
    // Dentro de una cita o una lista también bajan.
    let h = detalle_de("> # En una cita\n\n- ## En una lista");
    assert!(!h.contains("<h1") && !h.contains("<h2"), "{h}");
}

#[test]
fn las_listas_los_codigos_y_las_tablas_salen_bien() {
    let h = detalle_de(
        "1. uno\n2. dos\n\n3) tres\n\n7. siete\n\n- a\n- b\n\n```rust\nfn main() { println!(\"<hola>\"); }\n```\n\n    sangrado <b>\n\n| A | B |\n|---|---|\n| 1 | `2` |\n\n---\n\n~~tachado~~ y *énfasis* y **fuerte** y salto  \nduro",
    );
    comprobar_saneado(&h);
    assert!(h.contains("<ol>"), "{h}");
    assert!(h.contains("<ol start=\"7\">"), "{h}");
    assert!(h.contains("<ul>"), "{h}");
    assert!(
        h.contains("<pre><code>fn main() { println!(&quot;&lt;hola&gt;&quot;); }\n</code></pre>"),
        "{h}"
    );
    assert!(h.contains("sangrado &lt;b&gt;"), "{h}");
    assert!(h.contains("<table>") && h.contains("<th>A</th>") && h.contains("<td>1</td>"));
    assert!(h.contains("<td><code>2</code></td>"), "{h}");
    assert!(h.contains("<hr>"), "{h}");
    assert!(h.contains("<del>tachado</del>") && h.contains("<em>énfasis</em>"));
    assert!(
        h.contains("<strong>fuerte</strong>") && h.contains("<br>"),
        "{h}"
    );
    // El código no lleva `class`: nada del texto de una valla llega a un atributo.
    let h = detalle_de("```\"><script>alert(1)</script>\ncódigo\n```");
    comprobar_saneado(&h);
    assert!(!h.contains("class="), "{h}");
}

// ---------------------------------------------------------------------------
// Los CHANGELOG reales
// ---------------------------------------------------------------------------

#[test]
fn los_changelog_del_repositorio_se_parsean() {
    let lista = ListaPermitida::por_defecto();
    for (ruta, etiqueta, esperada) in [
        ("CHANGELOG.md", "v1.6.1", "1.6.1"),
        ("android/CHANGELOG.md", "android-v0.2.0", "0.2.0"),
    ] {
        let texto = std::fs::read_to_string(raiz().join(ruta)).expect(ruta);
        assert!(texto.len() <= TOPE_CHANGELOG, "{ruta} pasa del tope");
        let entradas =
            entradas_de_changelog(&texto, ruta, etiqueta).unwrap_or_else(|e| panic!("{ruta}: {e}"));
        assert!(!entradas.is_empty(), "{ruta} sin versiones");
        assert!(
            entradas.iter().any(|e| e.numero == esperada),
            "{ruta} sin la {esperada}"
        );
        let mut vistos = std::collections::HashSet::new();
        for e in &entradas {
            assert!(
                vistos.insert(e.numero.clone()),
                "{ruta}: {} repetida",
                e.numero
            );
            assert!(
                !e.resumen_es.is_empty() && !e.resumen_en.is_empty(),
                "{ruta} {}",
                e.numero
            );
            assert!(!e.detalle_html.is_empty(), "{ruta} {}", e.numero);
            assert!(e.detalle_html.len() <= TOPE_ENTRADA * 8);
            comprobar_saneado(&e.detalle_html);
            // Sin la sección «Sin publicar», ni las definiciones de enlace del final.
            let visible = texto_visible(&e.detalle_html);
            assert!(!visible.contains("Sin publicar"), "{ruta} {}", e.numero);
            assert!(!visible.contains("]: "), "{ruta} {}", e.numero);
            assert!(!e.detalle_html.contains("compare/"), "{ruta} {}", e.numero);
            // Todos los enlaces son https:// y de los que `comprobar` deja pasar. Si
            // falla por un dominio nuevo, entra en la lista en el mismo PR.
            for h in hrefs(&e.detalle_html) {
                assert!(
                    lista.permite(&h),
                    "{ruta} {}: el enlace {h} no está en la lista permitida de `magi-web comprobar`",
                    e.numero
                );
            }
        }
    }

    // El de la 1.6.1 trae un enlace relativo que tiene que ir a GitHub en su etiqueta.
    let texto = std::fs::read_to_string(raiz().join("CHANGELOG.md")).unwrap();
    let entradas = entradas_de_changelog(&texto, "CHANGELOG.md", "v1.6.1").unwrap();
    let e = entrada(&entradas, "1.6.1");
    assert!(
        hrefs(&e.detalle_html).contains(
            &"https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md"
                .to_string()
        ),
        "falta el enlace al formato de la sincronización"
    );
    assert_eq!(e.fecha, "2026-11-22");
    assert!(e
        .resumen_es
        .starts_with("Primera versión publicada de MAGI"));
    assert!(e.resumen_en.starts_with("First published release of MAGI"));
}

// ---------------------------------------------------------------------------
// Entradas patológicas
// ---------------------------------------------------------------------------

/// Una entrada con el cuerpo dado, medida: no tiene que dar pánico ni tardar.
fn medir(nombre: &str, cuerpo: &str) -> (Result<String, ErrorWeb>, Duration) {
    let t = Instant::now();
    let r = detalle_a_html(cuerpo, "v1.0.0", nombre);
    let tardo = t.elapsed();
    assert!(
        tardo < Duration::from_secs(20),
        "{nombre} tardó {tardo:?} con {} bytes",
        cuerpo.len()
    );
    (r, tardo)
}

#[test]
fn un_changelog_patologico_no_hace_panico_ni_tarda() {
    let n = 100_000;
    let casos: Vec<(&str, String)> = vec![
        ("corchetes abiertos", "[".repeat(n)),
        ("corchetes cerrados", "]".repeat(n)),
        ("paréntesis", "(".repeat(n)),
        ("corchetes y paréntesis", "[a](".repeat(n / 4)),
        (
            "enlaces anidados",
            format!("{}x{}", "[".repeat(n / 2), "](u)".repeat(n / 8)),
        ),
        ("enlaces con referencias", "[a][b][c][d]".repeat(n / 12)),
        (
            "referencias que se repiten",
            format!("[x]: https://ejemplo.org/\n\n{}", "[x] ".repeat(n / 4)),
        ),
        ("imágenes", "![a](".repeat(n / 5)),
        ("citas anidadas", "> ".repeat(n)),
        ("citas en líneas", "> hola\n".repeat(n / 7)),
        (
            "listas anidadas",
            (0..2000)
                .map(|i| format!("{}- x\n", " ".repeat(i % 200)))
                .collect(),
        ),
        (
            "listas con muchas sangrías",
            format!("{}x", "- ".repeat(n / 2)),
        ),
        ("lista numerada", format!("{}x", "1. ".repeat(n / 3))),
        (
            "énfasis anidado",
            format!("{}x{}", "*a ".repeat(n / 6), "a* ".repeat(n / 6)),
        ),
        ("negritas", "**a ".repeat(n / 4)),
        ("guiones bajos", "_".repeat(n)),
        ("asteriscos", "*".repeat(n)),
        ("tachados", "~~a ".repeat(n / 4)),
        ("acentos graves", "`".repeat(n)),
        ("código en línea sin cerrar", "`a ".repeat(n / 3)),
        ("vallas", "```\n".repeat(n / 4)),
        ("html en bruto", "<a ".repeat(n / 3)),
        ("html anidado", "<div>".repeat(n / 5)),
        ("autoenlaces", "<https://ejemplo.org/a>".repeat(n / 23)),
        ("menores", "<".repeat(n)),
        ("entidades", "&amp;&lt;&#x26;&#38;&bogus;".repeat(n / 26)),
        ("encabezados", "# a\n".repeat(n / 4)),
        ("tablas", "| a | b |\n|---|---|\n".repeat(n / 20)),
        (
            "tabla ancha",
            format!("{}\n{}\n", "|a".repeat(n / 2), "|-".repeat(n / 2)),
        ),
        ("destino enorme", format!("[x]({})", "a".repeat(400_000))),
        (
            "destino enorme de referencia",
            format!("[x]\n\n[x]: {}", "a".repeat(400_000)),
        ),
        (
            "destino https enorme",
            format!("[x](https://ejemplo.org/{})", "a/".repeat(100_000)),
        ),
        (
            "texto de enlace enorme",
            format!("[{}](https://ejemplo.org/)", "a".repeat(400_000)),
        ),
        (
            "UTF-8 raro",
            "c\u{0302}\u{0338}\u{0322}o\u{0337}m\u{0335}b\u{0337}i\u{0335}n\u{0334}a\u{0338}\u{1f3f3}\u{fe0f}\u{200d}\u{1f308}\u{202e}\u{202a}\u{200b}\u{feff}日本語".repeat(n / 60),
        ),
        ("combinantes", format!("a{}", "\u{0301}".repeat(n))),
        (
            "controles",
            "\u{0}\u{1}\u{7}\u{1b}[31m\u{7f}\u{85}".repeat(n / 8),
        ),
        ("saltos", "\n".repeat(n)),
        ("espacios", " ".repeat(n)),
        ("tabuladores", "\t".repeat(n)),
        ("barras", "\\".repeat(n)),
        ("comillas", "\"'".repeat(n / 2)),
        ("línea enorme sin espacios", "a".repeat(400_000)),
        ("notas al pie", "[^1] [^2]: x\n".repeat(n / 13)),
    ];
    let mut lentos = Vec::new();
    for (nombre, cuerpo) in &casos {
        assert!(
            cuerpo.len() <= TOPE_ENTRADA,
            "{nombre}: {} bytes",
            cuerpo.len()
        );
        let (resultado, tardo) = medir(nombre, cuerpo);
        if tardo > Duration::from_secs(3) {
            lentos.push(format!("{nombre}: {tardo:?}"));
        }
        // Cualquier resultado vale (HTML saneado o un error claro), pero nunca algo vivo.
        match resultado {
            Ok(html) => {
                comprobar_saneado(&html);
                assert!(
                    profundidad(&html) <= 100,
                    "{nombre}: anidamiento de {}",
                    profundidad(&html)
                );
            }
            Err(e) => {
                let m = e.to_string();
                assert!(m.contains(nombre), "{nombre}: el error no dice dónde: {m}");
            }
        }
    }
    assert!(lentos.is_empty(), "casos que tardan más de 3 s: {lentos:?}");

    // Los mismos, dentro de un CHANGELOG entero (lo que pasa por `entradas_de_changelog`).
    for (nombre, cuerpo) in casos.iter().take(12) {
        let texto = changelog_con(cuerpo);
        let t = Instant::now();
        let r = entradas_de_changelog(&texto, nombre, "v1.0.0");
        assert!(t.elapsed() < Duration::from_secs(20), "{nombre}");
        if let Ok(e) = r {
            for v in e {
                comprobar_saneado(&v.detalle_html);
            }
        }
    }

    // Las listas profundas de verdad se aplanan: la profundidad no crece con la entrada.
    let (profundo, _) = medir("citas y listas", &format!("{}x", "> - ".repeat(5000)));
    let profundo = profundo.expect("se aplana, no falla");
    assert!(profundidad(&profundo) <= 100, "{}", profundidad(&profundo));
    assert!(profundo.contains('x'), "el texto se conserva");

    // Un CHANGELOG con muchísimas versiones: el tope las corta con un error claro.
    let mut muchas = String::new();
    for i in 0..3000 {
        muchas.push_str(&format!(
            "## [1.{i}.0] - 2026-01-01\n\n**Resumen.** R.\n\n**Summary.** S.\n\n"
        ));
    }
    let t = Instant::now();
    let m = mensaje(entradas_de_changelog(&muchas, "muchas.md", "v1.0.0"));
    assert!(m.contains("más de"), "{m}");
    assert!(t.elapsed() < Duration::from_secs(10));

    // Muchas definiciones de enlace y muchas versiones que las usan, sin coste cuadrático.
    let mut texto = String::new();
    for i in 0..1500 {
        texto.push_str(&format!(
            "## [2.{i}.0] - 2026-01-01\n\n**Resumen.** R.\n\n**Summary.** S.\n\nVer [a{i}] y [b{i}][r{i}].\n\n"
        ));
    }
    for i in 0..1500 {
        texto.push_str(&format!(
            "[r{i}]: docs/guia-{i}.md\n[a{i}]: https://ejemplo.org/{i}\n"
        ));
    }
    let t = Instant::now();
    let e = entradas_de_changelog(&texto, "defs.md", "v1.0.0").unwrap();
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert_eq!(e.len(), 1500);
    assert_eq!(
        hrefs(&e[7].detalle_html),
        [
            "https://ejemplo.org/7",
            "https://github.com/HectorEspejo/magi/blob/v1.0.0/docs/guia-7.md"
        ]
    );
    assert_eq!(
        hrefs(&e[1499].detalle_html),
        [
            "https://ejemplo.org/1499",
            "https://github.com/HectorEspejo/magi/blob/v1.0.0/docs/guia-1499.md"
        ]
    );
}

#[test]
fn los_topes_de_tamano_se_respetan() {
    // El fichero entero.
    let enorme = "a".repeat(TOPE_CHANGELOG + 1);
    let m = mensaje(entradas_de_changelog(&enorme, "enorme.md", "v1.0.0"));
    assert!(m.contains("enorme.md") && m.contains("tope"), "{m}");
    let justo = format!("{}\n", "a".repeat(TOPE_CHANGELOG - 1));
    assert!(entradas_de_changelog(&justo, "justo.md", "v1.0.0").is_ok());

    // Una entrada.
    let relleno = "x".repeat(79);
    let lineas = TOPE_ENTRADA / 80 + 10;
    let cuerpo: String = (0..lineas).map(|_| format!("{relleno}\n")).collect();
    let texto =
        format!("## [1.0.0] - 2026-01-01\n\n**Resumen.** R.\n\n**Summary.** S.\n\n{cuerpo}");
    let m = mensaje(entradas_de_changelog(&texto, "grande.md", "v1.0.0"));
    assert!(m.contains("1.0.0") && m.contains("tope"), "{m}");

    // Directamente, el detalle.
    let m = mensaje(detalle_a_html(
        &"a".repeat(TOPE_ENTRADA + 1),
        "v1.0.0",
        "directo",
    ));
    assert!(m.contains("directo") && m.contains("tope"), "{m}");

    // El HTML que sale también tiene tope (un texto de `&` crece al escaparse 5×).
    let amperes = "&".repeat(TOPE_ENTRADA);
    let r = detalle_a_html(&amperes, "v1.0.0", "amperes");
    match r {
        Ok(h) => assert!(h.len() <= 8 * TOPE_ENTRADA + 64, "{}", h.len()),
        Err(e) => assert!(e.to_string().contains("amperes")),
    }
}

// ---------------------------------------------------------------------------
// La página
// ---------------------------------------------------------------------------

struct Mundo {
    plantillas: Plantillas,
    textos: Textos,
    plataformas: Plataformas,
}

fn mundo() -> Mundo {
    let web = raiz().join("web");
    let texto_plataformas = std::fs::read_to_string(web.join("datos/plataformas.toml")).unwrap();
    Mundo {
        plantillas: Plantillas::cargar(&web.join("plantillas")).expect("plantillas reales"),
        textos: Textos::cargar(&web.join("textos")).expect("textos reales"),
        plataformas: Plataformas::desde_texto(&texto_plataformas, "plataformas.toml").unwrap(),
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

    fn pagina(&self, idioma: Idioma, datos: &DatosVersiones) -> String {
        renderizar(&self.entorno(), idioma, datos).unwrap_or_else(|e| panic!("{e}"))
    }
}

fn changelog_de_la_app() -> Vec<EntradaChangelog> {
    entradas_de_changelog(
        "## [0.2.0] - 2026-12-01\n\n**Resumen.** La app llega: terminal y túneles.\n\n**Summary.** The app arrives: terminal and tunnels.\n\n### Añadido\n\n- Lo primero, con [su guía](../docs/publicar-android.md).\n\n## [0.1.0] - 2026-11-01\n\n**Resumen.** Una prueba.\n\n**Summary.** A test.\n",
        "android/CHANGELOG.md",
        "android-v0.2.0",
    )
    .unwrap()
}

fn datos(app: Option<Vec<EntradaChangelog>>) -> DatosVersiones {
    DatosVersiones {
        escritorio: parsear("dos_versiones.md"),
        app,
    }
}

#[test]
fn la_pagina_en_castellano_lleva_el_resumen_y_el_detalle() {
    let m = mundo();
    let pagina = m.pagina(Idioma::Es, &datos(None));

    assert!(pagina.contains("<html lang=\"es\">"), "{pagina}");
    assert!(pagina.contains("<title>Versiones · MAGI</title>"));
    assert!(pagina.contains("<h1 class=\"titulo-pagina\">Versiones</h1>"));
    // El resumen en castellano y no el inglés.
    assert!(pagina.contains(
        "Segunda versión de DEMO: añade los túneles y mejora la conexión (un resumen en dos líneas)."
    ));
    assert!(!pagina.contains("Second DEMO release"), "{pagina}");
    assert!(pagina.contains("Primera versión de DEMO."));
    // El título de cada versión: canal, número y fecha en castellano.
    assert!(
        pagina.contains("Escritorio 1.2.0 · <time datetime=\"2026-11-22\">22 nov 2026</time></h3>"),
        "{pagina}"
    );
    assert!(pagina.contains("Escritorio 1.1.0 · <time datetime=\"2026-10-01\">1 oct 2026</time>"));
    // El detalle (en castellano) con sus encabezados y listas.
    assert!(pagina.contains("<h4>Añadido</h4>"));
    assert!(pagina.contains("<summary>Detalle</summary>"));
    assert!(pagina.contains("<div class=\"detalle\" lang=\"es\">"));
    assert!(pagina.contains("<code>magi tunel</code>"));
    assert!(pagina.contains(
        "href=\"https://github.com/HectorEspejo/magi/blob/v1.2.0/docs/formato-sincronizacion.md\""
    ));
    // La más nueva primero.
    let a = pagina.find("Escritorio 1.2.0").unwrap();
    let b = pagina.find("Escritorio 1.1.0").unwrap();
    assert!(a < b, "orden de las versiones");
    // Sin «Sin publicar» del CHANGELOG ni sus comentarios.
    assert!(!pagina.contains("todavía no ha salido"));
    assert!(!pagina.contains("Plantilla de una versión"));
    assert!(!pagina.contains("[Sin publicar]"));
    // El anclaje de salto «Escritorio · App» y las dos secciones.
    let salto_a = pagina
        .find("<a href=\"#escritorio\">Escritorio</a>")
        .unwrap();
    let salto_b = pagina.find("<a href=\"#app\">App</a>").unwrap();
    assert!(salto_a < salto_b);
    assert!(pagina.contains("<section class=\"seccion-versiones\" id=\"escritorio\""));
    assert!(pagina.contains("<section class=\"seccion-versiones\" id=\"app\""));
    assert!(pagina.contains("aria-label=\"Ir a una sección de esta página\""));
    // La página es hija de la base: cabecera, pie y su página actual.
    assert!(pagina.contains("aria-current=\"page\">Versiones</a>"));
    assert!(pagina.contains("href=\"../en/releases/\""));
    assert!(pagina.contains("class=\"pie\""));
}

#[test]
fn la_pagina_en_ingles_lleva_el_summary_y_details_in_spanish() {
    let m = mundo();
    let pagina = m.pagina(Idioma::En, &datos(Some(changelog_de_la_app())));

    assert!(pagina.contains("<html lang=\"en\">"));
    assert!(pagina.contains("<title>Releases · MAGI</title>"));
    assert!(pagina.contains("<h1 class=\"titulo-pagina\">Releases</h1>"));
    // El Summary y no el Resumen.
    assert!(pagina.contains(
        "Second DEMO release: adds tunnels and improves the connection (a two-line summary)."
    ));
    assert!(!pagina.contains("Segunda versión de DEMO"), "{pagina}");
    assert!(pagina.contains("The app arrives: terminal and tunnels."));
    assert!(!pagina.contains("La app llega"));
    // El detalle sigue en castellano, dentro de «Details (in Spanish)».
    assert!(pagina.contains("<summary>Details (in Spanish)</summary>"));
    assert!(!pagina.contains("<summary>Detalle</summary>"));
    assert!(pagina.contains("<div class=\"detalle\" lang=\"es\">"));
    assert!(pagina.contains("<h4>Añadido</h4>"));
    // Fechas y canales en inglés.
    assert!(
        pagina.contains("Desktop 1.2.0 · <time datetime=\"2026-11-22\">22 Nov 2026</time></h3>"),
        "{pagina}"
    );
    assert!(pagina.contains("App 0.2.0 · <time datetime=\"2026-12-01\">1 Dec 2026</time>"));
    assert!(pagina.contains("<a href=\"#escritorio\">Desktop</a>"));
    assert!(pagina.contains("on GitHub"));
    // Los enlaces internos de una página de `en/releases/` suben dos niveles.
    assert!(pagina.contains("href=\"../../versiones/\""));
}

#[test]
fn la_app_sin_releases_dice_aun_sin_publicar() {
    let m = mundo();
    for (idioma, titulo, texto) in [
        (
            Idioma::Es,
            "App · aún sin publicar",
            "La primera release de la app sale después de esta fase.",
        ),
        (
            Idioma::En,
            "App · not published yet",
            "The app&#x27;s first release comes out after this phase.",
        ),
    ] {
        for app in [None, Some(Vec::new())] {
            let pagina = m.pagina(idioma, &datos(app.clone()));
            assert!(pagina.contains(titulo), "{idioma:?} {app:?}:\n{pagina}");
            assert!(pagina.contains(texto), "{idioma:?}:\n{pagina}");
            // Sin versiones de la app ni su enlace a una release.
            assert!(!pagina.contains("android-v"), "{pagina}");
            // El escritorio sí las tiene y no dice «sin publicar».
            assert!(pagina.contains("1.2.0"));
            assert_eq!(
                pagina.matches("sin publicar").count()
                    + pagina.matches("not published yet").count(),
                1
            );
        }
    }

    // Con releases de la app, la caja desaparece y salen sus versiones.
    let pagina = m.pagina(Idioma::Es, &datos(Some(changelog_de_la_app())));
    assert!(!pagina.contains("aún sin publicar"), "{pagina}");
    assert!(!pagina.contains("sale después de esta fase"));
    assert!(pagina.contains("App 0.2.0 · <time datetime=\"2026-12-01\">1 dic 2026</time>"));
    assert!(pagina.contains("App 0.1.0"));
    assert!(pagina.contains("La app llega: terminal y túneles."));
    // El enlace relativo de su detalle va a GitHub en la etiqueta de la app.
    assert!(pagina.contains(
        "href=\"https://github.com/HectorEspejo/magi/blob/android-v0.2.0/docs/publicar-android.md\""
    ));

    // Y sin versiones del escritorio, lo dice en su sección.
    let vacia = DatosVersiones {
        escritorio: Vec::new(),
        app: None,
    };
    let pagina = m.pagina(Idioma::Es, &vacia);
    assert!(pagina.contains("Escritorio · aún sin publicar"), "{pagina}");
    assert!(pagina.contains("Todavía no hay ninguna versión del escritorio publicada."));
}

#[test]
fn cada_version_enlaza_a_su_release() {
    let m = mundo();
    let lista = ListaPermitida::por_defecto();
    for idioma in Idioma::TODOS {
        let pagina = m.pagina(idioma, &datos(Some(changelog_de_la_app())));
        let enlaces: Vec<String> = etiquetas(&pagina)
            .iter()
            .filter(|e| !e.cierre && e.nombre == "a")
            .filter_map(|e| {
                e.atributo("href")
                    .map(|h| html::decodificar_atributo(h).unwrap())
            })
            .collect();
        for esperado in [
            "https://github.com/HectorEspejo/magi/releases/tag/v1.2.0",
            "https://github.com/HectorEspejo/magi/releases/tag/v1.1.0",
            "https://github.com/HectorEspejo/magi/releases/tag/android-v0.2.0",
            "https://github.com/HectorEspejo/magi/releases/tag/android-v0.1.0",
        ] {
            assert!(enlaces.iter().any(|h| h == esperado), "{esperado}");
            assert_eq!(
                pagina
                    .matches(&format!(
                        "<p class=\"version-enlace\"><a href=\"{esperado}\">"
                    ))
                    .count(),
                1,
                "{idioma:?}: un enlace «en GitHub» a {esperado}"
            );
            assert!(lista.permite(esperado), "{esperado}");
        }
        // Cada enlace dice de qué versión es (para los lectores de pantalla).
        let texto = if idioma == Idioma::Es {
            "en GitHub"
        } else {
            "on GitHub"
        };
        assert_eq!(pagina.matches(texto).count(), 4, "{idioma:?}");
        assert!(
            pagina.contains("<span class=\"solo-lectores\"> (App 0.2.0)</span>")
                || idioma == Idioma::En
        );
        // Todos los enlaces externos son https.
        for h in enlaces.iter().filter(|h| h.contains("://")) {
            assert!(h.starts_with("https://"), "{h}");
        }
    }
}

#[test]
fn el_detalle_va_dentro_de_un_details() {
    let m = mundo();
    for idioma in Idioma::TODOS {
        let pagina = m.pagina(idioma, &datos(Some(changelog_de_la_app())));
        let mut dentro = 0usize;
        let mut detalles = 0usize;
        let mut encabezados_dentro = 0usize;
        let mut resumenes_fuera = 0usize;
        for e in etiquetas(&pagina) {
            match (e.nombre.as_str(), e.cierre) {
                ("details", false) => {
                    dentro += 1;
                    detalles += 1;
                    assert!(!e.tiene_atributo("open"), "el detalle va plegado");
                }
                ("details", true) => dentro -= 1,
                ("h4", false) | ("h5", false) => {
                    assert!(dentro > 0, "{} del detalle fuera de un <details>", e.nombre);
                    encabezados_dentro += 1;
                }
                ("p", false) if e.atributo("class") == Some("version-resumen") => {
                    assert_eq!(dentro, 0, "el resumen va fuera del <details>");
                    resumenes_fuera += 1;
                }
                _ => {}
            }
        }
        // 1.2.0 (con detalle), 1.1.0 (con detalle) y 0.2.0 (con detalle); 0.1.0 no tiene.
        assert_eq!(detalles, 3, "{idioma:?}");
        assert_eq!(resumenes_fuera, 4, "{idioma:?}");
        assert_eq!(encabezados_dentro, 4, "{idioma:?}");
        assert_eq!(dentro, 0);
        // Cada `<details>` empieza por su `<summary>`.
        assert_eq!(pagina.matches("<summary>").count(), 3);
    }
}

#[test]
fn la_pagina_de_versiones_no_lleva_style_ni_script() {
    let m = mundo();
    // Con el CHANGELOG hostil dentro, como lo dejaría `entradas_de_changelog`.
    let hostil = DatosVersiones {
        escritorio: parsear("html_en_bruto.md"),
        app: Some(changelog_de_la_app()),
    };
    for datos in [&datos(None), &datos(Some(changelog_de_la_app())), &hostil] {
        for idioma in Idioma::TODOS {
            let pagina = m.pagina(idioma, datos);
            let minusculas = pagina.to_lowercase();
            for malo in ["<script", "<style", "<iframe", "<object", "<embed", "<svg"] {
                assert!(!minusculas.contains(malo), "«{malo}» en la página");
            }
            for e in etiquetas(&pagina) {
                assert!(!e.tiene_atributo("style"), "<{}> con style", e.nombre);
                assert_ne!(e.nombre, "script");
                assert_ne!(e.nombre, "style");
                for (nombre, _) in &e.atributos {
                    assert!(!nombre.starts_with("on"), "<{}> con {nombre}", e.nombre);
                }
                if let Some(h) = e.atributo("href") {
                    let h = html::decodificar_atributo(h).unwrap();
                    assert!(
                        h.starts_with('#') || h.starts_with("https://") || !h.contains(':'),
                        "href raro {h}"
                    );
                }
            }
        }
    }

    // El resumen, el número y todo lo que no es el detalle se escapan, sin `|safe`.
    let mut malo = parsear("dos_versiones.md");
    malo[0].resumen_es = "<script>alert(1)</script> \"x\" & 'y'".to_string();
    malo[0].resumen_en = "<img src=x onerror=alert(1)>".to_string();
    let pagina = m.pagina(
        Idioma::Es,
        &DatosVersiones {
            escritorio: malo.clone(),
            app: None,
        },
    );
    assert!(!pagina.contains("<script>alert(1)"), "{pagina}");
    assert!(
        pagina.contains("&lt;script&gt;alert(1)&lt;/script&gt; &quot;x&quot; &amp; &#x27;y&#x27;")
    );
    let pagina = m.pagina(
        Idioma::En,
        &DatosVersiones {
            escritorio: malo,
            app: None,
        },
    );
    assert!(!pagina.contains("<img src=x"), "{pagina}");
    assert!(pagina.contains("&lt;img src=x onerror=alert(1)&gt;"));
}

#[test]
fn la_pagina_no_se_fia_de_una_entrada_armada_a_mano() {
    let m = mundo();
    let buena = || parsear("dos_versiones.md");
    let con = |cambio: &dyn Fn(&mut EntradaChangelog)| {
        let mut v = buena();
        cambio(&mut v[0]);
        renderizar(
            &m.entorno(),
            Idioma::Es,
            &DatosVersiones {
                escritorio: v,
                app: None,
            },
        )
    };

    // Un detalle que no es lo que escribe `detalle_a_html`: falla cerrado.
    for malo in [
        "<script>alert(1)</script>",
        "<p onclick=\"x\">a</p>",
        "<p style=\"color:red\">a</p>",
        "<a href=\"javascript:alert(1)\">x</a>",
        "<a href=\"http://ejemplo.org/\">x</a>",
        "<a href=\"https://ejemplo.org/\" target=\"_blank\">x</a>",
        "<a href=\"&#106;avascript:alert(1)\">x</a>",
        "<img src=\"https://ejemplo.org/x.png\">",
        "<div>a</div>",
        "<!-- comentario -->",
        "<!DOCTYPE html>",
        "<iframe src=\"https://ejemplo.org/\"></iframe>",
        "<p>sin cerrar <",
        "<ol start=\"x\"><li>a</li></ol>",
        "<h4 id=\"a\">x</h4>",
        "</p onclick=\"x\">",
        "<style>a{}</style>",
        "<svg onload=\"x\"></svg>",
        // Etiquetas sin cerrar o cerradas en desorden: se tragarían el resto de la página.
        "<a href=\"https://ejemplo.org/\">sin cerrar",
        "<p>sin cerrar",
        "<table><tr><td>a</td></tr>",
        "<ul><li>a</ul></li>",
        "</p>",
        "<p>a</p></p>",
        "<p/>",
        "<a href=\"https://ejemplo.org/\"/>texto",
        "<br></br>",
    ] {
        let r = con(&|e| e.detalle_html = malo.to_string());
        assert!(r.is_err(), "debía rechazar {malo:?}");
        let m = r.unwrap_err().to_string();
        assert!(m.contains("1.2.0"), "dice la versión: {m}");
    }
    // Lo que sí escribe `detalle_a_html` pasa.
    let r = con(&|e| {
        e.detalle_html = "<ol start=\"3\"><li><a href=\"https://ejemplo.org/\">x</a></li></ol>\n<pre><code>a</code></pre>\n<br><hr>".to_string()
    });
    assert!(r.is_ok(), "{r:?}");

    // Un número que no es `X.Y.Z` iría a un id y a una URL.
    for numero in [
        "1.2.0\"><script>alert(1)</script>",
        "1.2",
        "01.2.0",
        "1.2.0-rc.1",
        "",
        "v1.2.0",
        "1.2.0/../../x",
    ] {
        let r = con(&|e| e.numero = numero.to_string());
        assert!(r.is_err(), "debía rechazar el número {numero:?}");
    }
    // Una versión repetida en un canal repetiría ids.
    let mut repetidas = buena();
    repetidas.push(repetidas[0].clone());
    assert!(renderizar(
        &m.entorno(),
        Idioma::Es,
        &DatosVersiones {
            escritorio: repetidas,
            app: None
        }
    )
    .is_err());
    // Sin resumen en el idioma de la página: falla (la lectura ya lo exige).
    assert!(con(&|e| e.resumen_es = "  ".to_string()).is_err());
    // Una fecha que no se puede mostrar se omite, no se inventa.
    for rara in ["ayer", "", "2026-13-01", "2026-02-30", "22/11/2026"] {
        let pagina = con(&|e| e.fecha = rara.to_string()).expect("la fecha rara no es fatal");
        assert!(
            pagina.contains(
                "<h3 class=\"caja-titulo\" id=\"escritorio-1-2-0-titulo\">Escritorio 1.2.0</h3>"
            ),
            "{rara:?}:\n{pagina}"
        );
        assert!(!pagina.contains("datetime=\"") || !pagina.contains(rara) || rara.is_empty());
    }
}

#[test]
fn la_pagina_funciona_con_las_versiones_de_los_changelog_reales() {
    let m = mundo();
    let leer = |ruta: &str, etiqueta: &str| {
        let texto = std::fs::read_to_string(raiz().join(ruta)).unwrap();
        entradas_de_changelog(&texto, ruta, etiqueta).unwrap()
    };
    let datos = DatosVersiones {
        escritorio: leer("CHANGELOG.md", "v1.6.1"),
        app: Some(leer("android/CHANGELOG.md", "android-v0.2.0")),
    };
    for idioma in Idioma::TODOS {
        let pagina = m.pagina(idioma, &datos);
        assert!(pagina.contains("releases/tag/v1.6.1"));
        assert!(pagina.contains("releases/tag/android-v0.2.0"));
        assert!(!pagina.contains("<script"));
        // Todos los enlaces y anclas internos de la página existen en ella.
        let ids: Vec<String> = etiquetas(&pagina)
            .iter()
            .filter_map(|e| e.atributo("id").map(str::to_string))
            .collect();
        let mut unicos = ids.clone();
        unicos.sort();
        unicos.dedup();
        assert_eq!(ids.len(), unicos.len(), "ids repetidos: {ids:?}");
        for e in etiquetas(&pagina) {
            if let Some(h) = e.atributo("href").and_then(|h| h.strip_prefix('#')) {
                if h != "contenido" {
                    assert!(ids.iter().any(|i| i == h), "ancla rota #{h}");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Entradas aleatorias
// ---------------------------------------------------------------------------

/// Un generador pseudoaleatorio sencillo y determinista (xorshift64*).
struct Azar(u64);

impl Azar {
    fn siguiente(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn hasta(&mut self, n: usize) -> usize {
        (self.siguiente() % n as u64) as usize
    }
}

/// Trozos de Markdown, de HTML y de CHANGELOG para mezclar sin ton ni son.
const TROZOS: &[&str] = &[
    "[",
    "]",
    "(",
    ")",
    "![",
    "](",
    "](https://ejemplo.org/",
    "](javascript:alert(1))",
    "](docs/x.md)",
    "](#a)",
    "](<",
    ">)",
    "<",
    ">",
    "</",
    "<script>",
    "</script>",
    "<img src=x onerror=1>",
    "<!--",
    "-->",
    "&",
    "&amp;",
    "&#106;",
    "&#x6A;",
    "&colon;",
    "&lt;",
    "\"",
    "'",
    "`",
    "``",
    "```",
    "~~~",
    "*",
    "**",
    "_",
    "__",
    "~~",
    "> ",
    "- ",
    "* ",
    "1. ",
    "7) ",
    "    ",
    "\t",
    "# ",
    "## ",
    "### ",
    "## [1.0.0] - 2026-11-22",
    "## [Sin publicar]",
    "## [2.0.0] - 2026-02-30",
    "\n",
    "\n\n",
    "\r\n",
    "**Resumen.** ",
    "**Summary.** ",
    "[x]: https://ejemplo.org/",
    "[x]: javascript:1",
    "[x]",
    "[x][y]",
    "<https://ejemplo.org/a>",
    "<javascript:alert(1)>",
    "<a@b>",
    "|",
    "|---|",
    "| a | b |\n|---|---|\n",
    "\\",
    "\u{0}",
    "\u{202e}",
    "ñ",
    "日本語",
    "🏳️‍🌈",
    "\u{0301}",
    "http://ejemplo.org",
    "https://ejemplo.org/",
    "docs/",
    "../",
    "..",
    "a",
    "b",
    " ",
    "{#id}",
    "<uuid>",
    "---",
    "***",
    "<div>",
    "</div>",
    "<a href=\"x\">",
];

fn texto_azar(azar: &mut Azar, trozos: usize) -> String {
    (0..trozos)
        .map(|_| TROZOS[azar.hasta(TROZOS.len())])
        .collect()
}

#[test]
fn entradas_aleatorias_nunca_hacen_panico_ni_dejan_nada_vivo() {
    let m = mundo();
    let mut azar = Azar(0x9e37_79b9_7f4a_7c15);
    let mut buenas = 0usize;
    for vuelta in 0..1500 {
        let largo = 1 + azar.hasta(60);
        let cuerpo = texto_azar(&mut azar, largo);

        // El detalle suelto: o HTML saneado o un error, nunca pánico.
        match detalle_a_html(&cuerpo, "v1.0.0", "azar") {
            Ok(html) => {
                comprobar_saneado(&html);
                buenas += 1;
                // Y lo que sale lo acepta la página (si no, un CHANGELOG legítimo
                // pararía la construcción).
                let entrada = EntradaChangelog {
                    numero: "1.0.0".to_string(),
                    fecha: "2026-11-22".to_string(),
                    resumen_es: "R".to_string(),
                    resumen_en: "S".to_string(),
                    detalle_html: html.clone(),
                };
                let pagina = renderizar(
                    &m.entorno(),
                    Idioma::En,
                    &DatosVersiones {
                        escritorio: vec![entrada],
                        app: None,
                    },
                )
                .unwrap_or_else(|e| panic!("vuelta {vuelta}: la página rechaza {html:?}: {e}"));
                assert!(!pagina.contains("<script"), "vuelta {vuelta}");
            }
            Err(e) => assert!(e.to_string().contains("azar"), "{e}"),
        }

        // Un CHANGELOG entero hecho de trozos: lo que salga, bien formado.
        let texto = format!(
            "{}\n## [3.0.0] - 2026-11-22\n\n**Resumen.** R{}\n\n**Summary.** S\n\n{}\n{}",
            texto_azar(&mut azar, 5),
            texto_azar(&mut azar, 3),
            cuerpo,
            texto_azar(&mut azar, 8),
        );
        if let Ok(entradas) = entradas_de_changelog(&texto, "azar", "android-v1.0.0") {
            for e in entradas {
                comprobar_saneado(&e.detalle_html);
                assert!(!e.resumen_es.is_empty() && !e.resumen_en.is_empty());
                assert!(magi_web::versiones::TOPE_ENTRADA > e.detalle_html.len());
            }
        }
    }
    assert!(buenas > 500, "demasiados fallos de lectura: {buenas}/1500");
}
