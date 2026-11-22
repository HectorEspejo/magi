//! Pruebas de la CSP (`magi_web::csp`) contra su API pública.
//!
//! Los hashes se comprueban dos veces: contra vectores calculados fuera de
//! este código (`hashlib` de Python) y contra `sha2` y `base64` llamados aquí.

use std::fs;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use magi_web::csp::{extraer, hash_script, insertar, politica, scripts_en_linea, REFERRER};
use magi_web::html::{tokenizar, Token};
use sha2::{Digest, Sha256};

const POLITICA_PROPIA: &str = "default-src 'none'; img-src 'self'; style-src 'self'; \
     font-src 'self'; script-src 'none'; base-uri 'none'; form-action 'none'";

fn hash_de_referencia(texto: &str) -> String {
    format!(
        "'sha256-{}'",
        STANDARD.encode(Sha256::digest(texto.as_bytes()))
    )
}

/// Una página con `<head>` al estilo de mdBook (un comentario antes del charset).
fn pagina(cabeza: &str, cuerpo: &str) -> String {
    format!(
        "<!DOCTYPE HTML>\n<html lang=\"es\">\n    <head>\n        <!-- Book generated using mdBook -->\n        \
         <meta charset=\"UTF-8\">\n        <title>Prueba</title>\n{cabeza}    </head>\n    <body>\n{cuerpo}\n    </body>\n</html>\n"
    )
}

fn csp_de(html: &str) -> String {
    extraer(html)
        .expect("la página debe poder leerse")
        .expect("la página debe llevar CSP")
}

fn fijos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fijos")
}

// ---------------------------------------------------------------------------
// Hash
// ---------------------------------------------------------------------------

#[test]
fn el_hash_coincide_con_vectores_calculados_fuera_de_este_codigo() {
    // `hashlib.sha256(...)` + `base64.b64encode(...)` de Python.
    assert_eq!(
        hash_script("alert(1)"),
        "'sha256-bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI='"
    );
    assert_eq!(
        hash_script(""),
        "'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='"
    );
    assert_eq!(
        hash_script("a\nb"),
        "'sha256-fhj3NzEbLcOy8mndeDlrA1HxT7Zu+oefdoyyMYGIPHg='"
    );
    for texto in [
        "nuevo()",
        "  con espacios\n",
        "ñandú 🙂 日本語",
        "x".repeat(100_000).as_str(),
    ] {
        assert_eq!(hash_script(texto), hash_de_referencia(texto));
    }
}

#[test]
fn el_hash_es_del_cuerpo_exacto_sin_recortar() {
    let base = hash_script("alert(1)");
    for distinto in [
        " alert(1)",
        "alert(1) ",
        "\nalert(1)",
        "alert(1)\n",
        "alert(1);",
        "ALERT(1)",
    ] {
        assert_ne!(hash_script(distinto), base, "{distinto:?}");
    }
    // El navegador lee \r\n y \r como \n antes de calcular el hash.
    assert_eq!(hash_script("a\r\nb"), hash_script("a\nb"));
    assert_eq!(hash_script("a\rb"), hash_script("a\nb"));
}

// ---------------------------------------------------------------------------
// scripts_en_linea
// ---------------------------------------------------------------------------

#[test]
fn script_en_mayusculas_type_module_y_svg_se_detectan() {
    let html = pagina(
        "",
        "<SCRIPT>mayusculas()</SCRIPT>\n\
         <script type=\"module\">import './x.js'; modulo();</script>\n\
         <Script Type='text/javascript' defer>mixto()</sCrIpT >\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\"><script>en_svg()</script></svg>\n\
         <script type=\"application/json\">{\"a\": 1}</script>\n\
         <script></script>",
    );
    let scripts = scripts_en_linea(&html).unwrap();
    assert_eq!(
        scripts,
        [
            "mayusculas()",
            "import './x.js'; modulo();",
            "mixto()",
            "en_svg()",
            "{\"a\": 1}",
            ""
        ]
    );
}

#[test]
fn los_script_con_src_no_son_en_linea_y_su_cuerpo_no_cuenta() {
    let html = pagina(
        "        <script src=\"a.js\"></script>\n",
        "<SCRIPT SRC='b.js' type=module></SCRIPT>\n<script src>no se ejecuta</script>\n<script>propio()</script>",
    );
    assert_eq!(scripts_en_linea(&html).unwrap(), ["propio()"]);
}

#[test]
fn un_script_dentro_de_un_comentario_o_de_un_atributo_no_es_un_script() {
    let html = pagina(
        "",
        "<!-- <script>comentado()</script> -->\n<a title=\"<script>atributo()</script>\">x</a>\n\
         <style>/* <script>estilo()</script> */</style>\n<script>real()</script>",
    );
    assert_eq!(scripts_en_linea(&html).unwrap(), ["real()"]);
}

#[test]
fn un_comentario_vacio_o_cerrado_con_bang_no_esconde_un_script() {
    // Los navegadores cierran `<!-->` y `--!>`: lo que sigue es un script real.
    for apertura in ["<!-->", "<!--->", "<!-- x --!>"] {
        let html = pagina("", &format!("{apertura}<script>escondido()</script>"));
        assert_eq!(
            scripts_en_linea(&html).unwrap(),
            ["escondido()"],
            "{apertura}"
        );
    }
}

#[test]
fn un_html_que_no_se_puede_leer_no_da_una_csp() {
    for malo in [
        "<script>sin cerrar",
        "<p class=\"x",
        "<!-- sin cerrar",
        "<head><meta charset=utf-8></head><body><",
    ] {
        assert!(scripts_en_linea(malo).is_err(), "{malo:?}");
        assert!(insertar(malo, true).is_err(), "{malo:?}");
        assert!(extraer(malo).is_err(), "{malo:?}");
    }
}

// ---------------------------------------------------------------------------
// Política
// ---------------------------------------------------------------------------

#[test]
fn las_paginas_propias_no_permiten_scripts() {
    assert_eq!(politica(&[], false), POLITICA_PROPIA);
    // Aunque la página lleve scripts en línea, la política propia sigue siendo `'none'`.
    let con_scripts = politica(&["a()".into(), "b()".into()], false);
    assert_eq!(con_scripts, POLITICA_PROPIA);
    assert!(!con_scripts.contains("sha256"));
    assert!(!con_scripts.contains("frame-src"));

    let html = pagina(
        "",
        "<script>algo()</script>\n<script src=\"x.js\"></script>",
    );
    let insertada = insertar(&html, false).unwrap();
    let politica = csp_de(&insertada);
    assert_eq!(politica, POLITICA_PROPIA);
    assert!(politica.contains("script-src 'none';"));
    assert!(!politica.contains("'self' 'sha256"));
}

#[test]
fn la_politica_del_manual_lleva_self_los_hashes_unicos_ordenados_y_frame_src() {
    let scripts: Vec<String> = ["c()", "a()", "b()", "a()", "c()"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let p = politica(&scripts, true);
    let mut hashes: Vec<String> = ["a()", "b()", "c()"]
        .iter()
        .map(|s| hash_de_referencia(s))
        .collect();
    hashes.sort();
    assert_eq!(
        p,
        format!(
            "default-src 'none'; img-src 'self'; style-src 'self'; font-src 'self'; \
             script-src 'self' {}; base-uri 'none'; form-action 'none'; frame-src 'self'",
            hashes.join(" ")
        )
    );
    // Sin scripts en línea, solo `'self'`.
    assert_eq!(
        politica(&[], true),
        "default-src 'none'; img-src 'self'; style-src 'self'; font-src 'self'; \
         script-src 'self'; base-uri 'none'; form-action 'none'; frame-src 'self'"
    );
}

#[test]
fn la_csp_nunca_lleva_unsafe_inline() {
    let texto_trampa = "'unsafe-inline' 'unsafe-eval' \"; script-src * \"";
    for es_manual in [false, true] {
        for scripts in [
            vec![],
            vec!["a()".to_string()],
            vec![texto_trampa.to_string(), "".to_string()],
        ] {
            let p = politica(&scripts, es_manual);
            assert!(!p.contains("unsafe"), "{p}");
            assert!(!p.contains('*'), "{p}");
            assert!(
                !p.contains("data:") && !p.contains("http:") && !p.contains("https:"),
                "{p}"
            );
            // Nada permite lo de fuera: todo `'self'` o `'none'`.
            assert!(p.starts_with("default-src 'none';"), "{p}");
        }
    }

    // Una página con un script cuyo texto habla de `unsafe-inline`, y con una
    // CSP anterior que lo permitía: la sustituta no lo lleva.
    let html = pagina(
        "        <meta http-equiv=\"Content-Security-Policy\" content=\"default-src * 'unsafe-inline' 'unsafe-eval'\">\n",
        &format!("<script>{texto_trampa}</script>"),
    );
    for es_manual in [false, true] {
        let nueva = insertar(&html, es_manual).unwrap();
        assert!(!csp_de(&nueva).contains("unsafe"), "{nueva}");
        assert!(
            !nueva.contains("default-src * "),
            "se debe quitar la CSP anterior"
        );
    }
}

// ---------------------------------------------------------------------------
// insertar
// ---------------------------------------------------------------------------

#[test]
fn un_script_en_linea_nuevo_entra_en_la_csp_por_su_hash() {
    let sin = pagina("", "<p>sin scripts</p>");
    let con = pagina("", "<p>con</p>\n<script>nuevo()</script>");
    let con_dos = pagina("", "<script>nuevo()</script>\n<script>otro()</script>");

    let p_sin = csp_de(&insertar(&sin, true).unwrap());
    let p_con = csp_de(&insertar(&con, true).unwrap());
    let p_dos = csp_de(&insertar(&con_dos, true).unwrap());

    let hash_nuevo = "'sha256-MJ6QR/MvQljl+WtEIxWGP0L0ukEMS1w/TrHmL/j3BXk='"; // sha256("nuevo()")
    assert_eq!(hash_script("nuevo()"), hash_nuevo);
    assert!(!p_sin.contains("sha256"), "{p_sin}");
    assert!(
        p_con.contains(&format!("script-src 'self' {hash_nuevo};")),
        "{p_con}"
    );
    assert!(p_dos.contains(hash_nuevo) && p_dos.contains(&hash_de_referencia("otro()")));
    // Sin `'unsafe-inline'`.
    for p in [&p_sin, &p_con, &p_dos] {
        assert!(!p.contains("unsafe"));
    }

    // Los hashes se calculan sobre el HTML definitivo: si el script cambia y se
    // vuelve a insertar, el hash viejo desaparece y entra el nuevo.
    let insertada = insertar(&con, true).unwrap();
    let cambiada = insertada.replace("nuevo()", "cambiado()");
    let reinsertada = insertar(&cambiada, true).unwrap();
    let p = csp_de(&reinsertada);
    assert!(!p.contains(hash_nuevo), "queda el hash viejo: {p}");
    assert!(p.contains(&hash_de_referencia("cambiado()")), "{p}");
    assert_eq!(reinsertada.matches("Content-Security-Policy").count(), 1);
}

#[test]
fn el_hash_de_cada_script_en_linea_esta_aunque_se_repitan_entre_paginas() {
    // mdBook pone 5 o 6 scripts idénticos en cada página.
    let cuerpo = "<script>uno()</script>\n<script>dos()</script>\n<script>uno()</script>\n<script src=\"toc.js\"></script>";
    let p = csp_de(&insertar(&pagina("", cuerpo), true).unwrap());
    assert_eq!(p.matches("'sha256-").count(), 2, "{p}");
}

#[test]
fn insertar_la_csp_dos_veces_da_lo_mismo() {
    let paginas = [
        pagina("", "<p>hola</p>"),
        pagina("", "<script>a()</script>\n<script>b()</script>"),
        // Con una CSP y un referrer anteriores, de cualquier forma.
        pagina(
            "        <meta HTTP-EQUIV='content-security-policy' content=\"default-src *\">\n        <meta name=Referrer content=origin>\n",
            "<script>a()</script>",
        ),
        // Sin sangría ni saltos de línea.
        "<html><head><meta charset=utf-8><meta http-equiv=\"Content-Security-Policy\" content=\"x\"><title>t</title></head><body></body></html>"
            .to_string(),
        // Con la CSP donde no debe y repetida.
        pagina(
            "        <title>t</title>\n        <meta http-equiv=\"Content-Security-Policy\" content=\"a\">\n",
            "<meta http-equiv=\"Content-Security-Policy\" content=\"b\">",
        ),
    ];
    for html in &paginas {
        for es_manual in [false, true] {
            let una = insertar(html, es_manual).unwrap();
            let dos = insertar(&una, es_manual).unwrap();
            let tres = insertar(&dos, es_manual).unwrap();
            assert_eq!(una, dos, "no es idempotente (manual: {es_manual}):\n{una}");
            assert_eq!(dos, tres);
            assert_eq!(una.matches("Content-Security-Policy").count(), 1, "{una}");
            assert_eq!(una.matches("name=\"referrer\"").count(), 1, "{una}");
            assert!(!una.contains("Referrer content=origin"));
            // Un solo CSP, y es la de esta política.
            assert_eq!(
                csp_de(&una),
                politica(&scripts_en_linea(&una).unwrap(), es_manual)
            );
        }
    }
}

#[test]
fn la_csp_y_el_referrer_van_justo_despues_del_charset() {
    let html = pagina(
        "        <meta name=\"viewport\" content=\"width=device-width\">\n",
        "<p>x</p>",
    );
    let nueva = insertar(&html, true).unwrap();
    let tokens = tokenizar(&nueva).unwrap();
    let nombres: Vec<(String, Option<String>, Option<String>)> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Etiqueta(e) if !e.cierre => Some((
                e.nombre.clone(),
                e.atributo("http-equiv").map(str::to_string),
                e.atributo("name").map(str::to_string),
            )),
            _ => None,
        })
        .collect();
    let i_head = nombres.iter().position(|(n, _, _)| n == "head").unwrap();
    assert_eq!(nombres[i_head + 1].0, "meta"); // charset
    assert_eq!(
        nombres[i_head + 2].1.as_deref(),
        Some("Content-Security-Policy")
    );
    assert_eq!(nombres[i_head + 3].2.as_deref(), Some("referrer"));
    assert_eq!(nombres[i_head + 4].0, "title");
    assert!(nueva.contains(&format!("content=\"{REFERRER}\"")));
    // Lo demás de la página no cambia: quitando lo insertado, sale lo de antes.
    let sin: String = nueva
        .lines()
        .filter(|l| !l.contains("Content-Security-Policy") && !l.contains("name=\"referrer\""))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(sin, html);
}

#[test]
fn sin_head_o_sin_charset_al_principio_falla() {
    assert!(insertar("<html><body></body></html>", false).is_err());
    assert!(insertar("<html><head><title>t</title></head></html>", false).is_err());
    // El charset no es lo primero.
    assert!(insertar(
        "<html><head><title>t</title><meta charset=\"utf-8\"></head></html>",
        false
    )
    .is_err());
    // Texto suelto antes del charset: el navegador abriría el `<body>`.
    assert!(insertar(
        "<html><head>hola<meta charset=\"utf-8\"></head></html>",
        false
    )
    .is_err());
    // `<meta http-equiv=content-type>` no es `<meta charset>`.
    assert!(
        insertar(
            "<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\"></head></html>",
            false
        )
        .is_err()
    );
    let e = insertar("<html><head></head></html>", true)
        .unwrap_err()
        .to_string();
    assert!(e.contains("charset"), "{e}");
}

#[test]
fn una_csp_sin_head_en_un_script_o_comentario_no_cuenta() {
    // `<head>` y `<meta charset>` solo dentro de un comentario o de un script: no son de la página.
    let html = "<!-- <head><meta charset=\"utf-8\"> --><html><body><script>'<head>'</script></body></html>";
    assert!(insertar(html, false).is_err());
}

// ---------------------------------------------------------------------------
// extraer
// ---------------------------------------------------------------------------

#[test]
fn extraer_devuelve_el_content_o_nada() {
    let nueva = insertar(&pagina("", "<script>a()</script>"), true).unwrap();
    let p = extraer(&nueva).unwrap().unwrap();
    assert_eq!(p, politica(&["a()".into()], true));

    assert_eq!(extraer(&pagina("", "")).unwrap(), None);
    // `http-equiv` sin distinguir mayúsculas, comillas simples y entidades.
    let a_mano = pagina(
        "        <meta HTTP-EQUIV='content-security-policy' content=\"default-src &#39;none&#39;\">\n",
        "",
    );
    assert_eq!(
        extraer(&a_mano).unwrap().as_deref(),
        Some("default-src 'none'")
    );
    // Dos CSP o un `content` ausente o incomprensible: error, no «la primera».
    let dos = pagina(
        "        <meta http-equiv=\"Content-Security-Policy\" content=\"a\">\n        <meta http-equiv=\"Content-Security-Policy\" content=\"b\">\n",
        "",
    );
    assert!(extraer(&dos).is_err());
    let sin_content = pagina(
        "        <meta http-equiv=\"Content-Security-Policy\">\n",
        "",
    );
    assert!(extraer(&sin_content).is_err());
    let rara = pagina(
        "        <meta http-equiv=\"Content-Security-Policy\" content=\"default-src &bogus;\">\n",
        "",
    );
    assert!(extraer(&rara).is_err());
}

// ---------------------------------------------------------------------------
// Fixtures: un manual con la forma de la salida de mdBook
// ---------------------------------------------------------------------------

fn copiar_arbol(origen: &Path, destino: &Path, tratar: &dyn Fn(&Path, Vec<u8>) -> Vec<u8>) {
    fs::create_dir_all(destino).unwrap();
    let mut entradas: Vec<_> = fs::read_dir(origen).unwrap().map(|e| e.unwrap()).collect();
    entradas.sort_by_key(|e| e.file_name());
    for e in entradas {
        let ruta = e.path();
        let objetivo = destino.join(e.file_name());
        if ruta.is_dir() {
            copiar_arbol(&ruta, &objetivo, tratar);
        } else {
            let bytes = fs::read(&ruta).unwrap();
            fs::write(&objetivo, tratar(&ruta, bytes)).unwrap();
        }
    }
}

fn con_csp_del_manual(ruta: &Path, bytes: Vec<u8>) -> Vec<u8> {
    if ruta.extension().is_some_and(|e| e == "html") {
        let html = String::from_utf8(bytes).unwrap();
        insertar(&html, true).unwrap().into_bytes()
    } else {
        bytes
    }
}

#[test]
fn las_paginas_de_un_manual_de_mdbook_llevan_cada_una_los_hashes_de_sus_scripts() {
    let dir = fijos().join("manual-html");
    let mut hashes_por_pagina = Vec::new();
    for nombre in ["index.html", "instalacion.html", "tuneles.html", "toc.html"] {
        let html = fs::read_to_string(dir.join(nombre)).unwrap();
        assert!(
            extraer(&html).unwrap().is_none(),
            "el original no lleva CSP"
        );
        let scripts = scripts_en_linea(&html).unwrap();
        let nueva = insertar(&html, true).unwrap();
        let p = csp_de(&nueva);
        for s in &scripts {
            assert!(
                p.contains(&hash_de_referencia(s)),
                "{nombre}: falta el hash de {s:?}"
            );
        }
        assert!(p.contains("script-src 'self'"));
        assert!(p.ends_with("frame-src 'self'"));
        assert!(!p.contains("unsafe"));
        hashes_por_pagina.push((nombre, scripts.len()));
    }
    // Las páginas de capítulo llevan sus 5 scripts en línea; el `toc.html`, ninguno.
    assert_eq!(
        hashes_por_pagina,
        [
            ("index.html", 5),
            ("instalacion.html", 5),
            ("tuneles.html", 5),
            ("toc.html", 0)
        ]
    );
    // El marco del `toc.html` está permitido por `frame-src 'self'` y los scripts
    // externos (`toc-….js`, `book-….js`) por `'self'`, sin hash.
    let html = fs::read_to_string(dir.join("index.html")).unwrap();
    assert!(html.contains("<iframe class=\"sidebar-iframe-outer\" src=\"toc.html\">"));
    assert!(html.contains("<script src=\"toc-2128731a.js\">"));
}

#[test]
fn el_manual_del_sitio_bueno_es_el_de_mdbook_con_su_csp() {
    let origen = fijos().join("manual-html");
    let esperado = tempfile::tempdir().unwrap();
    copiar_arbol(&origen, esperado.path(), &con_csp_del_manual);
    comparar_arboles(esperado.path(), &fijos().join("sitio-bueno/manual"));
}

fn lista_de_ficheros(dir: &Path, prefijo: &str, salida: &mut Vec<String>) {
    for e in fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let nombre = format!("{prefijo}{}", e.file_name().to_string_lossy());
        if e.path().is_dir() {
            lista_de_ficheros(&e.path(), &format!("{nombre}/"), salida);
        } else {
            salida.push(nombre);
        }
    }
}

fn comparar_arboles(esperado: &Path, real: &Path) {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    lista_de_ficheros(esperado, "", &mut a);
    lista_de_ficheros(real, "", &mut b);
    a.sort();
    b.sort();
    let ayuda = "regenera los fijos con: cargo test -p magi-web --test csp -- --ignored regenerar_el_manual_del_sitio_bueno";
    assert_eq!(a, b, "{ayuda}");
    for nombre in a {
        assert_eq!(
            fs::read(esperado.join(&nombre)).unwrap(),
            fs::read(real.join(&nombre)).unwrap(),
            "{nombre} no es el esperado; {ayuda}"
        );
    }
}

/// Regenera `tests/fijos/sitio-bueno/manual` desde `manual-html`: el manual
/// «generado» (mdBook) con la CSP puesta.
#[test]
#[ignore = "regenera ficheros: solo a mano"]
fn regenerar_el_manual_del_sitio_bueno() {
    let destino = fijos().join("sitio-bueno/manual");
    let _ = fs::remove_dir_all(&destino);
    copiar_arbol(&fijos().join("manual-html"), &destino, &con_csp_del_manual);
}

// ---------------------------------------------------------------------------
// Basura
// ---------------------------------------------------------------------------

#[test]
fn insertar_y_extraer_no_hacen_panico_con_basura() {
    let mut estado: u64 = 0x2545_F491_4F6C_DD1D;
    let mut siguiente = move || {
        estado = estado
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (estado >> 33) as usize
    };
    let piezas = [
        "<head>",
        "</head>",
        "<meta charset=\"utf-8\">",
        "<meta http-equiv=\"Content-Security-Policy\" content=\"x\">",
        "<meta name=\"referrer\" content=\"a\">",
        "<script>",
        "</script>",
        "<!--",
        "-->",
        "<",
        ">",
        "\"",
        "é",
        "🙂",
        " ",
        "\n",
        "\r\n",
        "<title>",
        "</title>",
        "<p>",
        "&",
        "'",
        "=",
        "/",
        "<html>",
        "<body>",
    ];
    let mut con_exito = 0;
    for _ in 0..20_000 {
        let n = 1 + siguiente() % 20;
        // La mitad de las veces, con el comienzo que sí admite la inserción (antes de
        // `<head>` solo caben un doctype, comentarios y `<html>`): así la prueba alcanza
        // `insertar` con basura detrás.
        let mut s = if siguiente() % 2 == 0 {
            String::from("<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\">")
        } else {
            String::new()
        };
        for _ in 0..n {
            s.push_str(piezas[siguiente() % piezas.len()]);
        }
        let _ = extraer(&s);
        let _ = scripts_en_linea(&s);
        for es_manual in [false, true] {
            if let Ok(nueva) = insertar(&s, es_manual) {
                con_exito += 1;
                // Lo insertado es estable.
                assert_eq!(insertar(&nueva, es_manual).unwrap(), nueva, "{s:?}");
                assert!(extraer(&nueva).unwrap().is_some());
            }
        }
    }
    assert!(
        con_exito > 100,
        "la prueba debe alcanzar la inserción ({con_exito})"
    );
}

// ---------------------------------------------------------------------------
// Revisión adversarial
// ---------------------------------------------------------------------------

/// Revisión CSP-03: con algo delante de `<head>` que no es un doctype, un comentario ni
/// `<html>`, el navegador ignora el `<head>` (o lo deja en otro sitio), el `<meta>` de la
/// CSP queda fuera y no se aplica: `insertar` no puede decir que la puso.
#[test]
fn con_algo_antes_de_head_no_se_inserta_la_csp() {
    let cuerpo = "<head><meta charset=\"utf-8\"><title>t</title></head><p>hola</p>";
    for antes in [
        "<html><body>",
        "x",
        "<template>",
        "<svg>",
        "<html><html>",
        "<p>",
        "<div>",
    ] {
        let html = format!("<!doctype html>{antes}{cuerpo}");
        let e = insertar(&html, false).expect_err(&html);
        assert!(e.to_string().contains("antes de <head>"), "{antes}: {e}");
    }
    // Un `<noscript>` sin cerrar ni siquiera se tokeniza: también falla cerrado.
    insertar(&format!("<!doctype html><noscript>{cuerpo}"), false).unwrap_err();
    // Lo que sí: doctype, comentarios, blancos y un solo `<html>` con atributos.
    for bien in [
        "<!doctype html><html lang=\"es\">",
        "<!DOCTYPE HTML>\n<!-- hecho con mdBook -->\n<html lang=\"es\" class=\"x\" dir=\"ltr\">\n  ",
        "<html>",
        "",
    ] {
        let html = format!("{bien}{cuerpo}");
        let nueva = insertar(&html, false).unwrap_or_else(|e| panic!("{bien:?}: {e}"));
        assert!(extraer(&nueva).unwrap().is_some(), "{bien:?}");
    }
}

#[test]
fn una_csp_con_el_nombre_escrito_con_entidades_se_reconoce_y_se_sustituye() {
    // El navegador descodifica `http-equiv`: es una CSP, y debe quedar una sola.
    let html = pagina(
        "        <meta http-equiv=\"Content-Security-Polic&#121;\" content=\"default-src *\">\n\
                 <meta name=\"refer&#114;er\" content=\"unsafe-url\">\n",
        "<p>x</p>",
    );
    assert_eq!(extraer(&html).unwrap().as_deref(), Some("default-src *"));
    let insertada = insertar(&html, false).unwrap();
    assert_eq!(csp_de(&insertada), POLITICA_PROPIA);
    assert_eq!(insertada.matches("default-src").count(), 1);
    assert!(!insertada.contains("unsafe-url"), "{insertada}");
    assert_eq!(insertar(&insertada, false).unwrap(), insertada);
}

#[test]
fn un_script_escondido_bajo_un_elemento_de_texto_no_se_pierde_en_silencio() {
    for html in [
        "<title><!--</title><script>alert(1)</script>--></title>",
        "<textarea><!--</textarea><script>alert(1)</script>--></textarea>",
        "<xmp><!--</xmp><script>alert(1)</script>--></xmp>",
        "<noscript><!--</noscript><script>alert(1)</script>--></noscript>",
    ] {
        // Nunca `Ok(vec![])`: el script no puede quedar fuera de la CSP sin avisar.
        assert!(scripts_en_linea(html).is_err(), "{html}");
        assert!(insertar(&pagina("", html), true).is_err(), "{html}");
    }
}
