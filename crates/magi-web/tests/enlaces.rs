//! Pruebas de la clasificación y resolución de enlaces (`magi_web::enlaces`).
//!
//! `clasificar` decide qué se acepta en un `href`: un descuido aquí deja pasar un
//! `javascript:` o un enlace que sale del sitio. Los casos adversariales son la
//! parte principal.

use magi_web::enlaces::{clasificar, destino_en_github, resolver, Destino};

fn interno(ruta: &str, ancla: Option<&str>) -> Destino {
    Destino::Interno {
        ruta: ruta.to_string(),
        ancla: ancla.map(str::to_string),
    }
}

fn es_otro(desde: &str, href: &str) -> bool {
    matches!(clasificar(desde, href), Destino::Otro(_))
}

// ---------------------------------------------------------------------------
// resolver
// ---------------------------------------------------------------------------

#[test]
fn la_ruta_que_sube_por_encima_de_la_raiz_no_resuelve() {
    assert_eq!(resolver("", "../x"), None);
    assert_eq!(resolver("", ".."), None);
    assert_eq!(resolver("a", "../../x"), None);
    assert_eq!(resolver("a/b", "../../../etc/passwd"), None);
    assert_eq!(resolver("", "../../../etc/passwd"), None);
    assert_eq!(resolver("a", "b/../../../x"), None);
    assert_eq!(resolver("a/b/c", "../../../.."), None);
    // Por la vía de `clasificar`: no es un enlace interno, es `Otro`.
    assert!(es_otro("index.html", "../../../etc/passwd"));
    assert!(es_otro("a/index.html", "../../x"));
    assert!(es_otro("a/b/index.html", "../../../x"));
    assert!(es_otro("404.html", "/../x"));
    // Subir hasta la raíz, sin pasarse, sí vale.
    assert_eq!(resolver("a/b", "../../x").as_deref(), Some("x"));
    assert_eq!(resolver("a/b", "../..").as_deref(), Some(""));
    assert_eq!(
        clasificar("a/b/index.html", "../../x.html"),
        interno("x.html", None)
    );
}

#[test]
fn resolver_normaliza_los_segmentos() {
    assert_eq!(
        resolver("en/download", "../x.html").as_deref(),
        Some("en/x.html")
    );
    assert_eq!(
        resolver("en/download/", "../x.html").as_deref(),
        Some("en/x.html")
    );
    assert_eq!(resolver("", "a/./b/../c/").as_deref(), Some("a/c/"));
    assert_eq!(resolver("a", "./b").as_deref(), Some("a/b"));
    assert_eq!(resolver("a", "b/..").as_deref(), Some("a/"));
    assert_eq!(resolver("a/./b", "c").as_deref(), Some("a/b/c"));
    assert_eq!(resolver("a/x/..", "c").as_deref(), Some("a/c"));
    assert_eq!(resolver("", "").as_deref(), Some(""));
    assert_eq!(resolver("a/b", "").as_deref(), Some("a/b/"));
}

#[test]
fn resolver_rechaza_lo_que_no_es_una_ruta_relativa_limpia() {
    for relativa in [
        "/x", "//x", "a//b", "a\\b", "..\\x", "a\0b", "a\nb", "a\tb", "a\u{7f}b",
    ] {
        assert_eq!(resolver("a", relativa), None, "{relativa:?}");
    }
    assert_eq!(resolver("a\\b", "x"), None);
    assert_eq!(resolver("a\0", "x"), None);
    // Una base que ya se sale de la raíz tampoco resuelve.
    assert_eq!(resolver("..", "x"), None);
    assert_eq!(resolver("a/../..", "x"), None);
}

// ---------------------------------------------------------------------------
// clasificar
// ---------------------------------------------------------------------------

#[test]
fn clasifica_anclas_externos_e_internos() {
    let desde = "en/download/index.html";
    assert_eq!(clasificar(desde, "#sec"), Destino::Ancla("sec".into()));
    assert_eq!(clasificar(desde, "#"), Destino::Ancla(String::new()));
    // Un `href` vacío es la propia página: no sale de ella.
    assert_eq!(clasificar(desde, ""), Destino::Ancla(String::new()));
    assert_eq!(
        clasificar(desde, "#verificación"),
        Destino::Ancla("verificación".into())
    );
    assert_eq!(
        clasificar(desde, "#verificaci%C3%B3n"),
        Destino::Ancla("verificación".into())
    );
    assert_eq!(
        clasificar(desde, "https://github.com/HectorEspejo/magi/releases"),
        Destino::Externo("https://github.com/HectorEspejo/magi/releases".into())
    );
    assert_eq!(
        clasificar(desde, "https://semver.org/#sec"),
        Destino::Externo("https://semver.org/#sec".into())
    );
    assert_eq!(clasificar(desde, "../"), interno("en/index.html", None));
    assert_eq!(
        clasificar(desde, "./"),
        interno("en/download/index.html", None)
    );
    assert_eq!(
        clasificar(desde, "x.html"),
        interno("en/download/x.html", None)
    );
    assert_eq!(
        clasificar(desde, "../../manual/"),
        interno("manual/index.html", None)
    );
    assert_eq!(
        clasificar(desde, "../../manual/tuneles.html#túnel-local"),
        interno("manual/tuneles.html", Some("túnel-local"))
    );
    assert_eq!(
        clasificar(desde, "../../manual/t%C3%BAneles.html#t%C3%BAnel"),
        interno("manual/túneles.html", Some("túnel"))
    );
    assert_eq!(
        clasificar(desde, "../../css/web.css?v=3"),
        interno("css/web.css", None)
    );
    assert_eq!(
        clasificar("index.html", "descargas/"),
        interno("descargas/index.html", None)
    );
    assert_eq!(clasificar("index.html", "."), interno("index.html", None));
    assert_eq!(clasificar("index.html", "x.html#"), interno("x.html", None));
    assert_eq!(
        clasificar("a/b/index.html", ".."),
        interno("a/index.html", None)
    );
    assert_eq!(
        clasificar("a/b/c.html", "d.html"),
        interno("a/b/d.html", None)
    );
}

#[test]
fn un_http_o_un_javascript_son_otro() {
    for href in [
        "http://github.com/HectorEspejo/magi",
        "http://localhost/",
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "JavaScript:alert(1)",
        "  javascript:alert(1)",
        "javascript:alert(1)  ",
        "data:text/html,<script>alert(1)</script>",
        "data:image/svg+xml;base64,AAAA",
        "vbscript:msgbox(1)",
        "mailto:alguien@ejemplo.org",
        "tel:+34123456789",
        "file:///etc/passwd",
        "ftp://ejemplo.org/x",
        "blob:https://ejemplo.org/uuid",
        "about:blank",
        "ws://ejemplo.org",
        "wss://ejemplo.org",
        "x:y",
        "//ejemplo.org/x",
        "//",
        "///x",
        "\\\\ejemplo.org\\x",
        "https:\\\\ejemplo.org",
        "https:/\\ejemplo.org",
        "https:ejemplo.org",
        "https:/ejemplo.org",
        "HTTPS://ejemplo.org",
        "Https://ejemplo.org",
        "hTTps://ejemplo.org",
    ] {
        assert!(es_otro("index.html", href), "debía ser Otro: {href:?}");
    }
}

#[test]
fn los_esquemas_escondidos_con_blancos_controles_o_entidades_son_otro() {
    for href in [
        "java\tscript:alert(1)",
        "java\nscript:alert(1)",
        "java\rscript:alert(1)",
        "java\u{0}script:alert(1)",
        "\u{1}javascript:alert(1)",
        "\tjavascript:alert(1)",
        "\u{c}javascript:alert(1)",
        "java script:alert(1)",
        "javascript\u{a0}:alert(1)",
        "\u{feff}javascript:alert(1)",
        "javascript&colon;alert(1)",
        "javascript&#58;alert(1)",
        "javascript&#x3a;alert(1)",
        "&#106;avascript:alert(1)",
        "&#x6A;&#x61;vascript:alert(1)",
        "javascript&Tab;:alert(1)",
        "javascript&NewLine;:alert(1)",
        "&#9;javascript:alert(1)",
        "&amp;#106;avascript:alert(1)x&",
        "jav&#x09;ascript:alert(1)",
        "&#0;x",
        "&#x110000;",
        "ht&#x74;p://ejemplo.org",
        "a&b.html",
        "a%2Fb",
        "a%2fb",
        "a%5Cb",
        "a%00b",
        "a%0Ab",
        "a%zz",
        "a%",
        "a%c3",
        " ",
        "?x=1",
        "a b.html",
        "a\tb.html",
        "/descargas/",
        "/",
    ] {
        assert!(es_otro("index.html", href), "debía ser Otro: {href:?}");
    }
}

#[test]
fn una_entidad_numerica_que_forma_https_es_externo_y_no_otra_cosa() {
    // Descodificada, `&#104;ttps://…` es `https://…` y pasa por la lista como cualquier otro.
    assert_eq!(
        clasificar("index.html", "&#104;ttps://github.com/HectorEspejo/magi"),
        Destino::Externo("https://github.com/HectorEspejo/magi".into())
    );
    assert_eq!(
        clasificar("index.html", "https://ejemplo.org/?a=1&amp;b=2"),
        Destino::Externo("https://ejemplo.org/?a=1&b=2".into())
    );
}

#[test]
fn solo_el_404_usa_rutas_absolutas_del_sitio() {
    assert_eq!(
        clasificar("404.html", "/descargas/"),
        interno("descargas/index.html", None)
    );
    assert_eq!(clasificar("404.html", "/"), interno("index.html", None));
    assert_eq!(
        clasificar("404.html", "/css/web.css"),
        interno("css/web.css", None)
    );
    assert_eq!(
        clasificar("404.html", "/en/download/#linux"),
        interno("en/download/index.html", Some("linux"))
    );
    for desde in [
        "index.html",
        "descargas/index.html",
        "en/index.html",
        "manual/404.html",
    ] {
        assert!(es_otro(desde, "/css/web.css"), "{desde}");
        assert!(es_otro(desde, "/"), "{desde}");
    }
    // Ni en el 404 vale `//host` ni salirse de la raíz.
    assert!(es_otro("404.html", "//ejemplo.org/x"));
    assert!(es_otro("404.html", "/../x"));
    assert!(es_otro("404.html", "/a/../../x"));
}

#[test]
fn clasificar_conserva_el_href_original_en_otro() {
    assert_eq!(
        clasificar("index.html", "JavaScript:alert(1)"),
        Destino::Otro("JavaScript:alert(1)".into())
    );
}

#[test]
fn clasificar_no_hace_panico_con_basura() {
    let mut estado: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut siguiente = move || {
        estado = estado
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (estado >> 33) as usize
    };
    let piezas = [
        "https://",
        "http:",
        "javascript:",
        "//",
        "/",
        "../",
        "./",
        "..",
        ".",
        "#",
        "?",
        "&",
        "&amp;",
        "&#106;",
        "&#x6a;",
        "%",
        "%2f",
        "%C3%BA",
        "%zz",
        ":",
        "\\",
        " ",
        "\t",
        "\n",
        "\0",
        "é",
        "ñ",
        "🙂",
        "日",
        "a",
        "b.html",
        "index.html",
        "github.com",
        "=",
        "@",
        "[",
        "]",
    ];
    let desdes = [
        "index.html",
        "a/index.html",
        "a/b/c.html",
        "404.html",
        "",
        "x/",
        "../x.html",
        "é/ü.html",
    ];
    for _ in 0..40_000 {
        let n = 1 + siguiente() % 12;
        let mut href = String::new();
        for _ in 0..n {
            href.push_str(piezas[siguiente() % piezas.len()]);
        }
        let desde = desdes[siguiente() % desdes.len()];
        match clasificar(desde, &href) {
            Destino::Interno { ruta, ancla } => {
                // Un interno nunca se sale de la raíz ni lleva controles, `\` ni `..`.
                assert!(!ruta.starts_with('/'), "{href:?} → {ruta:?}");
                assert!(
                    !ruta.split('/').any(|s| s == ".." || s == "."),
                    "{href:?} → {ruta:?}"
                );
                assert!(
                    !ruta.chars().any(|c| c.is_control() || c == '\\'),
                    "{href:?} → {ruta:?}"
                );
                assert!(
                    ruta.split('/').count() == 1 || !ruta.contains("//"),
                    "{href:?} → {ruta:?}"
                );
                if let Some(a) = ancla {
                    assert!(!a.is_empty());
                    assert!(!a.chars().any(|c| c.is_control() || c == '\\'));
                }
            }
            Destino::Externo(u) => {
                assert!(u.starts_with("https://"), "{href:?} → {u:?}");
                assert!(!u
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace() || c == '\\'));
            }
            Destino::Ancla(a) => {
                assert!(!a.chars().any(|c| c.is_control() || c == '\\'));
            }
            Destino::Otro(o) => assert_eq!(o, href),
        }
        let _ = resolver(desde, &href);
        let _ = destino_en_github(&href, "v1.6.1");
        let _ = destino_en_github("docs/a.md", &href);
    }
}

// ---------------------------------------------------------------------------
// destino_en_github
// ---------------------------------------------------------------------------

const REPO: &str = "https://github.com/HectorEspejo/magi";

#[test]
fn destino_en_github_distingue_blob_y_tree_y_no_se_deja_manipular() {
    // Ficheros: blob. Carpetas (acaban en `/`): tree.
    assert_eq!(
        destino_en_github("docs/formato-sincronizacion.md", "v1.6.1"),
        format!("{REPO}/blob/v1.6.1/docs/formato-sincronizacion.md")
    );
    assert_eq!(
        destino_en_github("docs/", "v1.6.1"),
        format!("{REPO}/tree/v1.6.1/docs/")
    );
    assert_eq!(
        destino_en_github("crates/magi-nucleo/src/almacen/", "v1.6.1"),
        format!("{REPO}/tree/v1.6.1/crates/magi-nucleo/src/almacen/")
    );
    assert_eq!(
        destino_en_github("CONTRIBUTING.md", "android-v0.2.0"),
        format!("{REPO}/blob/android-v0.2.0/CONTRIBUTING.md")
    );
    assert_eq!(
        destino_en_github(".github/workflows/ci.yml", "v0.0.1"),
        format!("{REPO}/blob/v0.0.1/.github/workflows/ci.yml")
    );
    assert_eq!(
        destino_en_github("src/lib.rs#L10-L20", "v1.6.1"),
        format!("{REPO}/blob/v1.6.1/src/lib.rs#L10-L20")
    );
    assert_eq!(
        destino_en_github("docs/túneles.md", "v1.6.1"),
        format!("{REPO}/blob/v1.6.1/docs/t%C3%BAneles.md")
    );
    // La raíz del repositorio en esa etiqueta.
    assert_eq!(
        destino_en_github("", "v1.6.1"),
        format!("{REPO}/tree/v1.6.1")
    );

    // Rutas que no se dejan manipular: raíz del repositorio en esa etiqueta.
    let raiz = format!("{REPO}/tree/v1.6.1");
    for ruta in [
        "../x",
        "docs/../../x",
        "docs/..",
        "..",
        "/etc/passwd",
        "/docs/a.md",
        "docs\\a.md",
        "..\\x",
        "docs/a\u{0}.md",
        "docs/a\n.md",
        "docs/a\r\n.md",
        "docs/a\u{1b}[31m.md",
        "docs//a.md",
        "./docs/a.md",
        "docs/./a.md",
        "docs/a b.md",
        "docs/a?x=1",
        "docs/a%2e%2e/x",
        "docs/a\"onmouseover=\"x",
        "docs/<script>",
        "docs/a'b",
        "javascript:alert(1)",
        "https://evil.ejemplo.org/",
        "docs/a:b",
        "//evil.ejemplo.org/x",
        "//",
        "/",
    ] {
        assert_eq!(
            destino_en_github(ruta, "v1.6.1"),
            raiz,
            "la ruta {ruta:?} no debía dejarse pasar"
        );
    }

    // Etiquetas que no son una versión publicada: la raíz del repositorio, sin etiqueta.
    for etiqueta in [
        "",
        "main",
        "v1.6.1-rc.1",
        "v1.6",
        "v1.6.1.1",
        "1.6.1",
        "v01.6.1",
        "V1.6.1",
        "android-1.6.1",
        "v1.6.1/../../x",
        "v1.6.1/../../../evil/repo/blob/main",
        "v1.6.1\n",
        "v1.6.1 ",
        "v1.6.1#x",
        "v1.6.1?x",
        "refs/heads/main",
        "%76%31.6.1",
        "v1.6.1+build",
        "v-1.6.1",
    ] {
        assert_eq!(
            destino_en_github("docs/a.md", etiqueta),
            REPO,
            "la etiqueta {etiqueta:?} no debía dejarse pasar"
        );
    }
}

#[test]
fn destino_en_github_solo_produce_urls_del_repositorio() {
    let mut estado: u64 = 42;
    let mut siguiente = move || {
        estado = estado
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (estado >> 33) as usize
    };
    let piezas = [
        "docs", "/", "..", ".", "a.md", "#", "L10", "?", "%", "\\", " ", "é", "\n", "\"", ":",
        "x/", "https://",
    ];
    for _ in 0..20_000 {
        let n = 1 + siguiente() % 8;
        let mut ruta = String::new();
        for _ in 0..n {
            ruta.push_str(piezas[siguiente() % piezas.len()]);
        }
        let url = destino_en_github(&ruta, "v1.6.1");
        assert!(
            url.starts_with(&format!("{REPO}/")) || url == REPO,
            "{ruta:?} → {url}"
        );
        let cola = &url[REPO.len()..];
        assert!(
            cola.starts_with("/tree/v1.6.1") || cola.starts_with("/blob/v1.6.1/"),
            "{ruta:?} → {url}"
        );
        assert!(!url.contains(".."), "{ruta:?} → {url}");
        assert!(
            !url.contains('\\') && !url.contains(' ') && !url.contains('"') && !url.contains('\n')
        );
        assert!(url.matches('#').count() <= 1);
        assert!(!url.contains('?'));
        // Todo menos la parte del repositorio y la etiqueta es ASCII.
        assert!(url.is_ascii(), "{ruta:?} → {url}");
    }
}
