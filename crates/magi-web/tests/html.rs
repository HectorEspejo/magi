//! Pruebas del tokenizador de HTML (`magi_web::html`) contra su API pública.
//!
//! El tokenizador lo usan la CSP y `comprobar`: si se equivocara de lado (leer
//! como comentario lo que un navegador lee como marcado), esconderíamos un
//! `<script>` a la comprobación. Por eso estas pruebas cuidan sobre todo los
//! casos adversariales: fallar cerrado, no entrar en pánico y no cortar nunca
//! en mitad de un carácter.

use magi_web::html::{
    decodificar_atributo, linea_de, tokenizar, tokenizar_detallado, Etiqueta, Token,
};

/// Generador congruencial lineal propio (sin dependencias): mismas cadenas en
/// cada ejecución.
struct Lcg(u64);

impl Lcg {
    fn siguiente(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn hasta(&mut self, n: usize) -> usize {
        (self.siguiente() % n as u64) as usize
    }
}

fn etiquetas(html: &str) -> Vec<Etiqueta> {
    tokenizar(html)
        .expect("el HTML de la prueba debe tokenizar")
        .into_iter()
        .filter_map(|t| match t {
            Token::Etiqueta(e) => Some(e),
            _ => None,
        })
        .collect()
}

#[test]
fn un_tag_sin_cerrar_falla_cerrado() {
    let malos = [
        // `<` sin cerrar
        "<",
        "texto <",
        "<p>hola <",
        "a < b",
        "<3",
        "<<p>",
        "<>",
        "< p>",
        // etiqueta sin `>`
        "<p",
        "<p class",
        "<p class=",
        "<p class=\"a\"",
        "<a href=x",
        "<br /",
        "</p",
        "</p ",
        // comillas sin cerrar
        "<a href=\"x>y</a>",
        "<a href='x>y</a>",
        "<a title=\"uno\" href=\"dos>texto</a>",
        // comentarios sin cerrar
        "<!--",
        "<!-- sin cierre",
        "<!-- a -- b",
        "<!-- a --",
        "<p><!-- una\nvarias\nlíneas",
        // script y style sin cerrar
        "<script>",
        "<script>alert(1)",
        "<script>alert(1)</scrip>",
        "<script>alert(1)</scriptx>",
        "<script>alert(1)</script",
        "<SCRIPT>alert(1)",
        "<style>p{}",
        "<style>p{}</styl>",
        // doctype sin cerrar
        "<!DOCTYPE html",
        // construcciones que un navegador lee de otra manera
        "<![CDATA[ <script>alert(1)</script> ]]>",
        "<?php echo 1 ?>",
        "<!ELEMENT x>",
        "</ p>",
        "</>",
        "</3>",
        "<script>var a = '<!--'; </script>",
        "<svg><style><img src=x onerror=alert(1)></style></svg>",
        "<math><script><b>x</b></script></math>",
    ];
    for malo in malos {
        assert!(
            tokenizar(malo).is_err(),
            "debía fallar y no falló: {malo:?}"
        );
        let e = tokenizar_detallado(malo).expect_err("el detalle también falla");
        assert!(
            e.posicion <= malo.len(),
            "posición fuera de rango en {malo:?}"
        );
        assert!(!e.que.is_empty());
    }
}

#[test]
fn el_error_dice_la_linea_y_que_pasa() {
    let html = "<p>uno</p>\n<p>dos</p>\n<div class=\"sin-cerrar\n<p>";
    let e = tokenizar(html).expect_err("debía fallar");
    let texto = e.to_string();
    assert!(texto.contains("línea 3"), "{texto}");
    assert!(texto.contains("comillas sin cerrar"), "{texto}");
}

#[test]
fn los_comentarios_se_cierran_como_en_los_navegadores() {
    // `--!>` cierra un comentario: lo que viene después es marcado real.
    let t = tokenizar("<!-- x --!><script>alert(1)</script>").unwrap();
    assert!(matches!(&t[0], Token::Comentario(c) if c == " x "));
    assert!(matches!(&t[1], Token::Etiqueta(e) if e.nombre == "script"));

    // `<!-->` y `<!--->` son comentarios vacíos y no se tragan lo siguiente.
    for vacio in ["<!--><script>x</script>", "<!---><script>x</script>"] {
        let t = tokenizar(vacio).unwrap();
        assert!(
            matches!(&t[0], Token::Comentario(c) if c.is_empty()),
            "{vacio:?}: {t:?}"
        );
        assert!(
            t.iter()
                .any(|t| matches!(t, Token::Etiqueta(e) if e.nombre == "script")),
            "el script de {vacio:?} no debe quedar escondido: {t:?}"
        );
    }

    // Multilínea, con guiones y con marcado dentro.
    let t = tokenizar("<!--\n<script>x</script>\n-- - -->\n<p>").unwrap();
    assert!(matches!(&t[0], Token::Comentario(c) if c.contains("<script>")));
    assert_eq!(
        etiquetas("<!--\n<script>x</script>\n-- - -->\n<p>")
            .iter()
            .map(|e| e.nombre.as_str())
            .collect::<Vec<_>>(),
        ["p"]
    );
}

#[test]
fn los_nombres_van_en_minusculas_y_los_atributos_con_o_sin_comillas() {
    let e = &etiquetas("<DIV ID=\"A\" Data-X='b c' hidden CLASS=uno>")[0];
    assert_eq!(e.nombre, "div");
    assert_eq!(e.atributo("id"), Some("A"), "el valor no cambia de caja");
    assert_eq!(e.atributo("data-x"), Some("b c"));
    assert!(e.tiene_atributo("hidden"));
    assert_eq!(e.atributo("class"), Some("uno"));

    let e = &etiquetas("<a\n  href\n  =\n  \"x>y\"\n  title='a\"b'\n>")[0];
    assert_eq!(e.atributo("href"), Some("x>y"));
    assert_eq!(e.atributo("title"), Some("a\"b"));

    let e = &etiquetas("<input disabled/>")[0];
    assert!(e.autocierre && e.tiene_atributo("disabled"));
    let e = &etiquetas("<br/>")[0];
    assert!(e.autocierre);
    let e = &etiquetas("</P class=x>")[0];
    assert!(e.cierre && e.nombre == "p");
}

#[test]
fn el_script_se_cierra_en_cualquier_caja_y_su_cuerpo_no_se_toca() {
    let html = "<ScRiPt type=\"module\">\n  if (a < b && c > d) { s = \"</p>\"; }\n</SCRIPT\t>";
    let t = tokenizar(html).unwrap();
    assert_eq!(t.len(), 3, "{t:?}");
    match &t[1] {
        Token::Crudo {
            etiqueta,
            cuerpo,
            inicio,
            fin,
        } => {
            assert_eq!(etiqueta, "script");
            assert_eq!(cuerpo, "\n  if (a < b && c > d) { s = \"</p>\"; }\n");
            assert_eq!(&html[*inicio..*fin], cuerpo);
        }
        otro => panic!("esperaba el cuerpo en bruto: {otro:?}"),
    }

    // Un `<script>` dentro de un `<style>` o de un comentario no abre nada.
    let t = tokenizar("<style>a { } /* <script>x</script> */</style>").unwrap();
    assert_eq!(
        t.iter()
            .filter(|t| matches!(t, Token::Etiqueta(e) if e.nombre == "script"))
            .count(),
        0
    );
}

#[test]
fn las_posiciones_son_en_bytes_y_caen_en_limites_de_caracter() {
    let html = "<p título=\"ñandú 🙂\">añ🙂é</p><!-- ü --><script>'é🙂'</script><i>日本語</i>";
    let tokens = tokenizar(html).unwrap();
    let mut siguiente = 0;
    for t in &tokens {
        match t {
            Token::Etiqueta(e) => {
                assert_eq!(e.inicio, siguiente, "las etiquetas no dejan huecos");
                assert!(html.is_char_boundary(e.inicio) && html.is_char_boundary(e.fin));
                assert!(html[e.inicio..e.fin].starts_with('<'));
                assert!(html[e.inicio..e.fin].ends_with('>'));
                siguiente = e.fin;
            }
            Token::Texto { inicio, fin } | Token::Crudo { inicio, fin, .. } => {
                assert_eq!(*inicio, siguiente);
                assert!(html.is_char_boundary(*inicio) && html.is_char_boundary(*fin));
                siguiente = *fin;
            }
            Token::Comentario(c) => {
                assert_eq!(c, " ü ");
                // Un comentario no lleva posiciones: se recorre hasta el cierre.
                siguiente += "<!-- ü -->".len();
            }
            Token::Doctype => {}
        }
    }
    assert_eq!(siguiente, html.len(), "los tokens cubren todo el HTML");
}

#[test]
fn linea_de_cuenta_desde_uno_y_no_corta_caracteres() {
    let html = "añ\nbé\n\ncd";
    assert_eq!(linea_de(html, 0), 1);
    assert_eq!(linea_de(html, 4), 2);
    for posicion in 0..=html.len() + 5 {
        // En mitad de un carácter o más allá del final: nunca pánico.
        let _ = linea_de(html, posicion);
    }
    assert_eq!(linea_de(html, html.len()), 4);
    assert_eq!(linea_de("", 10), 1);
}

#[test]
fn los_atributos_repetidos_se_resuelven_como_el_navegador() {
    // El navegador se queda con el primero: `href` aquí NO es el `javascript:`.
    let e = &etiquetas("<a href=\"bien.html\" href=\"javascript:alert(1)\">")[0];
    assert_eq!(e.atributo("href"), Some("bien.html"));
    assert_eq!(
        e.atributos.len(),
        2,
        "se conservan los dos para poder vigilarlos"
    );
}

#[test]
fn el_valor_sin_comillas_y_el_menor_que_se_leen_como_el_navegador() {
    let e = &etiquetas("<a href=a/b/>")[0];
    assert_eq!(e.atributo("href"), Some("a/b/"));
    assert!(!e.autocierre);
    let e = &etiquetas("<a href=>")[0];
    assert_eq!(e.atributo("href"), Some(""));
    let e = &etiquetas("<a =x>")[0];
    assert!(e.tiene_atributo("=x"));
    // `<` dentro de comillas es solo texto.
    let e = &etiquetas("<a title=\"<b>\">")[0];
    assert_eq!(e.atributo("title"), Some("<b>"));
}

#[test]
fn las_entidades_se_descodifican_solo_si_se_conocen() {
    assert_eq!(decodificar_atributo("a&amp;b").as_deref(), Some("a&b"));
    assert_eq!(
        decodificar_atributo("&#x6A;&#97;va").as_deref(),
        Some("java")
    );
    assert_eq!(
        decodificar_atributo("sin nada").as_deref(),
        Some("sin nada")
    );
    for raro in [
        "javascript&colon;x",
        "&Tab;",
        "&amp",
        "&",
        "&#;",
        "&#x;",
        "&#xZZ;",
        "&#0;",
        "&#+65;",
        "&#x+41;",
        "&#-65;",
        "&#x110000;",
        "&#xD800;",
        "&;",
        "?a=1&b=2",
    ] {
        assert_eq!(decodificar_atributo(raro), None, "{raro:?}");
    }
}

const FRAGMENTOS: &[&str] = &[
    "<",
    ">",
    "</",
    "/>",
    "<!",
    "<!--",
    "-->",
    "--!>",
    "<!-->",
    "<!--->",
    "<?",
    "<![CDATA[",
    "]]>",
    "<script",
    "<SCRIPT",
    "</script",
    "</SCRIPT>",
    "<style",
    "</style>",
    "<title>",
    "</title>",
    "<textarea>",
    "</textarea>",
    "<noscript>",
    "</noscript>",
    "<iframe>",
    "</iframe>",
    "<select>",
    "<svg>",
    "</svg>",
    "<math>",
    "</math>",
    "<a ",
    "href=",
    "src=",
    "style=",
    "onclick=",
    "=",
    "\"",
    "'",
    "`",
    "/",
    " ",
    "\t",
    "\n",
    "\r",
    "\r\n",
    "\u{c}",
    "\0",
    "&",
    "&amp;",
    "&#106;",
    "&colon;",
    ";",
    "%",
    "%2e",
    "..",
    "é",
    "ñ",
    "ü",
    "日本",
    "🙂",
    "\u{fe0f}",
    "\u{200b}",
    "\u{feff}",
    "\u{85}",
    "\u{2028}",
    "a",
    "b",
    "script",
    "meta",
    "head",
    "charset",
    "http-equiv",
    "Content-Security-Policy",
    "x",
    "0",
    "9",
    "-",
    "_",
    ":",
    "javascript:",
    "https://",
    "\\",
];

fn cadena_aleatoria(g: &mut Lcg) -> String {
    let piezas = 1 + g.hasta(40);
    let mut s = String::new();
    for _ in 0..piezas {
        match g.hasta(10) {
            0 => {
                // Un carácter cualquiera (puede ser de cualquier plano).
                let c = char::from_u32((g.siguiente() % 0x11_0000) as u32).unwrap_or('?');
                s.push(c);
            }
            1 => s.push((g.hasta(128)) as u8 as char),
            _ => s.push_str(FRAGMENTOS[g.hasta(FRAGMENTOS.len())]),
        }
    }
    s
}

/// Comprueba lo que debe cumplir cualquier resultado `Ok`.
fn comprobar_tokens(html: &str, tokens: &[Token]) {
    let mut ultimo = 0;
    for t in tokens {
        let (inicio, fin) = match t {
            Token::Etiqueta(e) => {
                assert!(
                    html.get(e.inicio..e.fin)
                        .is_some_and(|s| s.starts_with('<') && s.ends_with('>')),
                    "etiqueta mal delimitada en {html:?}: {e:?}"
                );
                (e.inicio, e.fin)
            }
            Token::Texto { inicio, fin } => {
                assert!(
                    html.get(*inicio..*fin).is_some(),
                    "texto fuera de límites en {html:?}"
                );
                (*inicio, *fin)
            }
            Token::Crudo {
                cuerpo,
                inicio,
                fin,
                ..
            } => {
                assert_eq!(html.get(*inicio..*fin), Some(cuerpo.as_str()));
                (*inicio, *fin)
            }
            Token::Doctype | Token::Comentario(_) => continue,
        };
        assert!(inicio >= ultimo, "tokens solapados en {html:?}");
        assert!(fin >= inicio);
        ultimo = fin;
    }
}

#[test]
fn el_tokenizador_no_hace_panico_con_basura() {
    let mut g = Lcg(0x4D41_4749_2D57_4542);
    let mut bien = 0usize;
    let mut mal = 0usize;
    for _ in 0..30_000 {
        let s = cadena_aleatoria(&mut g);
        match tokenizar_detallado(&s) {
            Ok(tokens) => {
                comprobar_tokens(&s, &tokens);
                bien += 1;
            }
            Err(e) => {
                assert!(e.posicion <= s.len());
                // El mensaje de `tokenizar` nunca entra en pánico al dar la línea.
                let _ = tokenizar(&s).unwrap_err().to_string();
                mal += 1;
            }
        }
        let _ = linea_de(&s, g.hasta(s.len() + 3));
        let _ = decodificar_atributo(&s);
    }
    assert!(
        bien > 500,
        "la basura debe producir también HTML válido ({bien})"
    );
    assert!(mal > 500, "y también HTML inválido ({mal})");

    // Truncar un documento válido en cada límite de carácter.
    let doc = "<!DOCTYPE html>\n<html lang=\"es\"><head><meta charset=\"utf-8\"><!-- ñ --><title>Ñandú 🙂</title>\
               <script>let a = '日本';</script></head><body><a href=\"x?a=1&amp;b=2\" title='é'>🙂</a></body></html>";
    assert!(tokenizar(doc).is_ok());
    for (i, _) in doc.char_indices() {
        let _ = tokenizar_detallado(&doc[..i]);
    }

    // Truncar los bytes en cualquier punto y leerlos como UTF-8 con pérdida.
    let bytes = doc.as_bytes();
    for i in 0..=bytes.len() {
        let s = String::from_utf8_lossy(&bytes[..i]).into_owned();
        let _ = tokenizar_detallado(&s);
    }

    // Bytes sueltos cualesquiera, leídos con pérdida.
    for _ in 0..5_000 {
        let largo = g.hasta(60);
        let bytes: Vec<u8> = (0..largo).map(|_| g.siguiente() as u8).collect();
        let s = String::from_utf8_lossy(&bytes).into_owned();
        if let Ok(tokens) = tokenizar_detallado(&s) {
            comprobar_tokens(&s, &tokens);
        }
    }
}

#[test]
fn una_etiqueta_con_valores_arbitrarios_conserva_su_valor() {
    // Para cualquier valor sin la comilla que lo delimita, el valor vuelve tal cual.
    let mut g = Lcg(7);
    for _ in 0..3_000 {
        let mut valor = cadena_aleatoria(&mut g);
        valor.retain(|c| c != '"');
        let html = format!("<a data-x=\"{valor}\" id=z>");
        let e = &etiquetas(&html)[0];
        assert_eq!(e.atributo("data-x"), Some(valor.as_str()), "{html:?}");
        assert_eq!(e.atributo("id"), Some("z"));
        assert_eq!(&html[e.inicio..e.fin], html);
    }
}

#[test]
fn una_entrada_enorme_no_se_come_la_memoria_ni_el_tiempo() {
    // 2 MB de `<a>` seguidos: lineal.
    let html = "<a>".repeat(700_000);
    let t = tokenizar(&html).unwrap();
    assert_eq!(t.len(), 700_000);
    // Muchos `<script>` sin cerrar: falla en el primero, sin recorrer cuadráticamente.
    let html = "<script>".repeat(200_000);
    assert!(tokenizar(&html).is_err());
    // Una etiqueta con miles de atributos.
    let mut html = String::from("<a");
    for i in 0..20_000 {
        html.push_str(&format!(" a{i}=\"{i}\""));
    }
    html.push('>');
    assert_eq!(etiquetas(&html)[0].atributos.len(), 20_000);
}

// ---------------------------------------------------------------------------
// Elementos que el navegador lee como texto (revisión adversarial)
// ---------------------------------------------------------------------------

#[test]
fn los_elementos_que_el_navegador_lee_como_texto_no_esconden_un_script() {
    // Para el navegador, `<title><!--` no abre ningún comentario: el texto llega
    // hasta `</title>` y el `<script>` que viene detrás se ejecuta. Si se leyera
    // como marcado, el comentario lo escondería a la CSP y a `comprobar`.
    for nombre in ["title", "textarea", "xmp", "iframe", "noembed", "noframes"] {
        for ataque in [
            format!("<{nombre}><!--</{nombre}><script>alert(1)</script>--></{nombre}>"),
            format!("<{nombre}><style></{nombre}><script>alert(1)</script></style></{nombre}>"),
            format!("<{nombre}><script></{nombre}><script>alert(1)</script></script>"),
            format!("<{nombre}><b title=\"</{nombre}><script>alert(1)</script>\"></{nombre}>"),
            format!("<{nombre}>a < b</{nombre}>"),
        ] {
            let error = tokenizar_detallado(&ataque)
                .expect_err(&format!("debía fallar cerrado: {ataque:?}"));
            assert!(error.que.contains(nombre), "{ataque:?}: {}", error.que);
        }
        // Sin cierre, también falla.
        assert!(tokenizar(&format!("<{nombre}>texto")).is_err());
    }
}

#[test]
fn el_texto_de_title_y_compania_va_en_bruto_y_sin_distinguir_mayusculas() {
    let t = tokenizar("<TITLE>Ñandú &lt;3 &amp; más</TiTlE ><p>x</p>").unwrap();
    assert!(matches!(&t[0], Token::Etiqueta(e) if e.nombre == "title" && !e.cierre));
    match &t[1] {
        Token::Crudo {
            etiqueta, cuerpo, ..
        } => {
            assert_eq!(etiqueta, "title");
            assert_eq!(cuerpo, "Ñandú &lt;3 &amp; más");
        }
        otro => panic!("esperaba el texto en bruto, salió {otro:?}"),
    }
    assert!(matches!(&t[2], Token::Etiqueta(e) if e.nombre == "title" && e.cierre));
    // Vacíos, como el `<iframe>` de mdBook, y un `<title>` de SVG, son normales.
    assert!(tokenizar("<iframe class=\"x\" src=\"toc.html\"></iframe>").is_ok());
    assert!(tokenizar("<svg><title>icono</title><path d=\"M0 0\"/></svg>").is_ok());
}

#[test]
fn noscript_solo_se_acepta_si_se_lee_igual_como_texto_y_como_marcado() {
    // Con JavaScript es texto hasta el primer `</noscript>`; sin él, marcado.
    for ataque in [
        "<noscript><!--</noscript><script>alert(1)</script>--></noscript>",
        "<noscript><script></noscript><script>alert(1)</script></noscript>",
        "<noscript><style></noscript><script>alert(1)</script></style></noscript>",
        "<noscript><a title=\"</noscript><script>alert(1)</script>\"></noscript>",
        "<noscript><noscript></noscript></noscript>",
        "<noscript><p",
        "<noscript>",
    ] {
        assert!(
            tokenizar(ataque).is_err(),
            "debía fallar cerrado: {ataque:?}"
        );
    }

    // Lo que mdBook pone dentro (un `<iframe>`) se lee como marcado, con las
    // posiciones del documento entero.
    let html =
        "<p>x</p><noscript>\n  <iframe class=\"a\" src=\"toc.html\"></iframe>\n</noscript><b>";
    let tokens = tokenizar(html).unwrap();
    let marco = etiquetas(html)
        .into_iter()
        .find(|e| e.nombre == "iframe" && !e.cierre)
        .expect("el <iframe> de dentro se ve");
    assert_eq!(
        &html[marco.inicio..marco.fin],
        "<iframe class=\"a\" src=\"toc.html\">"
    );
    assert_eq!(marco.atributo("src"), Some("toc.html"));
    comprobar_tokens(html, &tokens);
    assert!(matches!(
        tokens.last(),
        Some(Token::Etiqueta(e)) if e.nombre == "b"
    ));
}

#[test]
fn select_no_se_admite() {
    // Los navegadores antiguos ignoran un `<style>` dentro de `<select>`: el
    // `<script>` que lleva detrás no estaría en bruto ni escondido, y aquí sí.
    for malo in [
        "<select><style><script>alert(1)</script></style></select>",
        "<SELECT></SELECT>",
        "<select/>",
    ] {
        assert!(tokenizar(malo).is_err(), "{malo:?}");
    }
}

#[test]
fn un_cierre_de_otra_clase_no_saca_del_svg() {
    // `</math>` no cierra un `<svg>`: lo que va dentro se sigue leyendo como marcado.
    assert!(tokenizar("<svg></math><script>a<b</script>").is_err());
    assert!(tokenizar("<math><svg></svg><script>a<b</script></math>").is_err());
    // Cerrados de verdad, vuelve a ser un script normal.
    assert!(tokenizar("<svg></svg><script>a<b</script>").is_ok());
    assert!(tokenizar("<svg><svg></svg></svg><script>a<b</script>").is_ok());
    assert!(tokenizar("<math><svg></svg></math><script>a<b</script>").is_ok());
    // Un `</svg>` que cierra el exterior cierra también el interior.
    assert!(tokenizar("<svg><math></svg><script>a<b</script>").is_ok());
}
