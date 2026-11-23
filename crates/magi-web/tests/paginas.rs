//! Pruebas de las páginas de la pista P1B: contexto común, descargas y 404, con las
//! plantillas y los textos reales de `web/`.
//!
//! `Plataformas` se construye aquí leyendo el `plataformas.toml` real con `toml`,
//! no con `Plataformas::cargar`, para que estas pruebas solo dependan de lo que
//! prueban; y la elección de versiones, a mano.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use magi_web::datos::{Canal, DatosApp, EstadoPlataforma, Plataforma, Plataformas};
use magi_web::descargas::{ArtefactoElegido, EleccionDescargas, VersionElegida};
use magi_web::paginas::{self, contexto_comun, Entorno};
use magi_web::plantillas::Plantillas;
use magi_web::rutas::{self, Idioma, Pagina};
use magi_web::textos::Textos;
use magi_web::ErrorWeb;

fn web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web")
}

const FECHA_ESCRITORIO: &str = "2026-11-22T11:04:48Z";
const FECHA_APP: &str = "2026-12-01T09:30:00Z";

fn huella() -> String {
    (0..32)
        .map(|i| format!("{:02X}", i * 7 + 1))
        .collect::<Vec<_>>()
        .join(":")
}

/// Lee `web/datos/plataformas.toml` con `toml` (sin pasar por `Plataformas::cargar`).
fn plataformas_reales() -> Plataformas {
    let texto = fs::read_to_string(web().join("datos/plataformas.toml")).expect("plataformas.toml");
    let tabla: toml::Table = texto.parse().expect("TOML válido");
    let app = tabla["app"].as_table().expect("[app]");
    let cadena = |t: &toml::Table, clave: &str| t[clave].as_str().expect(clave).to_string();
    let mut plataforma: Vec<Plataforma> = tabla["plataforma"]
        .as_array()
        .expect("[[plataforma]]")
        .iter()
        .map(|p| {
            let p = p.as_table().expect("tabla");
            Plataforma {
                id: cadena(p, "id"),
                canal: Canal::desde_id(&cadena(p, "canal")).expect("canal conocido"),
                estado: EstadoPlataforma::desde_id(&cadena(p, "estado")).expect("estado conocido"),
                artefacto: cadena(p, "artefacto"),
                orden: u32::try_from(p["orden"].as_integer().expect("orden")).expect("u32"),
                instrucciones: cadena(p, "instrucciones"),
            }
        })
        .collect();
    plataforma.sort_by_key(|p| p.orden);
    Plataformas {
        app: DatosApp {
            huella_certificado: cadena(app, "huella_certificado"),
            obtainium_etiquetas: cadena(app, "obtainium_etiquetas"),
            obtainium_apk: cadena(app, "obtainium_apk"),
        },
        plataforma,
    }
}

/// Como las reales, pero con la huella de la app puesta (la real está vacía hasta que haya keystore).
fn plataformas_con_huella() -> Plataformas {
    let mut p = plataformas_reales();
    p.app.huella_certificado = huella();
    p
}

struct Mundo {
    plantillas: Plantillas,
    textos: Textos,
    plataformas: Plataformas,
}

fn mundo() -> Mundo {
    Mundo {
        plantillas: Plantillas::cargar(&web().join("plantillas")).expect("plantillas reales"),
        textos: Textos::cargar(&web().join("textos")).expect("textos reales"),
        plataformas: plataformas_con_huella(),
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

fn version(canal: Canal, etiqueta: &str, numero: &str, fecha: &str) -> VersionElegida {
    VersionElegida {
        canal,
        etiqueta: etiqueta.to_string(),
        numero: numero.to_string(),
        fecha: fecha.to_string(),
        url: format!("https://github.com/HectorEspejo/magi/releases/tag/{etiqueta}"),
    }
}

fn artefacto(p: &Plataforma, indice: usize, etiqueta: &str, numero: &str) -> ArtefactoElegido {
    let nombre = p.nombre_artefacto(numero);
    ArtefactoElegido {
        plataforma: p.id.clone(),
        canal: p.canal,
        url: format!("https://github.com/HectorEspejo/magi/releases/download/{etiqueta}/{nombre}"),
        nombre,
        tamano: 9_542_041 + (indice as u64) * 1_048_576,
        // Una suma distinta por plataforma, de 64 hexadecimales en minúsculas.
        sha256: format!("{:02x}", 0x31 + indice).repeat(32),
        etiqueta: etiqueta.to_string(),
    }
}

/// Las seis plataformas publicadas: escritorio y Termux en 1.6.1 y la app en 0.2.0.
fn eleccion_completa(plataformas: &Plataformas) -> EleccionDescargas {
    let artefactos = plataformas
        .plataforma
        .iter()
        .enumerate()
        .map(|(i, p)| match p.canal {
            Canal::Escritorio | Canal::Termux => artefacto(p, i, "v1.6.1", "1.6.1"),
            Canal::App => artefacto(p, i, "android-v0.2.0", "0.2.0"),
        })
        .collect();
    EleccionDescargas {
        escritorio: version(Canal::Escritorio, "v1.6.1", "1.6.1", FECHA_ESCRITORIO),
        termux: Some(version(Canal::Termux, "v1.6.1", "1.6.1", FECHA_ESCRITORIO)),
        app: Some(version(Canal::App, "android-v0.2.0", "0.2.0", FECHA_APP)),
        artefactos,
        homebrew: false,
        aur: false,
    }
}

fn descargas(m: &Mundo, idioma: Idioma, eleccion: &EleccionDescargas) -> String {
    paginas::descargas::renderizar(&m.entorno(), idioma, eleccion).expect("descargas")
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

fn hrefs(html: &str) -> Vec<String> {
    let re = regex::Regex::new(r#"(?:href|src)="([^"]*)""#).expect("expresión");
    re.captures_iter(html).map(|c| c[1].to_string()).collect()
}

// ───────────────────────── contexto común ─────────────────────────

#[test]
fn el_contexto_comun_lleva_exactamente_las_claves_del_contrato() {
    let m = mundo();
    let esperadas: BTreeSet<&str> = [
        "lang",
        "idioma",
        "t",
        "raiz",
        "pagina",
        "alternativa",
        "nav",
        "version_manual",
    ]
    .into_iter()
    .collect();
    for pagina in Pagina::TODAS {
        for idioma in Idioma::TODOS {
            let ctx = contexto_comun(&m.entorno(), pagina, idioma);
            let claves: BTreeSet<&str> = ctx
                .as_object()
                .expect("un objeto")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(claves, esperadas, "{pagina:?} {idioma:?}");
            assert_eq!(ctx["lang"], idioma.codigo());
            assert_eq!(ctx["idioma"], idioma.codigo());
            assert_eq!(ctx["version_manual"], "1.6.1");
            let nav = ctx["nav"].as_object().expect("nav");
            assert_eq!(
                nav.keys().map(String::as_str).collect::<BTreeSet<_>>(),
                ["inicio", "descargas", "versiones", "manual"]
                    .into_iter()
                    .collect()
            );
            for (nombre, entrada) in nav {
                assert!(entrada["href"].is_string(), "nav.{nombre}.href");
            }
            let alternativa = ctx["alternativa"].as_object().expect("alternativa");
            assert_eq!(
                alternativa
                    .keys()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>(),
                ["href", "hreflang", "texto"].into_iter().collect()
            );
            assert_eq!(alternativa["hreflang"], idioma.otro().codigo());
            assert!(ctx["t"]["cabecera"]["inicio"].is_string());
        }
    }
}

#[test]
fn el_enlace_a_la_otra_lengua_lleva_a_la_pagina_equivalente() {
    let m = mundo();
    let casos = [
        (Pagina::Portada, Idioma::Es, "en/"),
        (Pagina::Portada, Idioma::En, "../"),
        (Pagina::Descargas, Idioma::Es, "../en/download/"),
        (Pagina::Descargas, Idioma::En, "../../descargas/"),
        (Pagina::Versiones, Idioma::Es, "../en/releases/"),
        (Pagina::Versiones, Idioma::En, "../../versiones/"),
    ];
    for (pagina, idioma, esperado) in casos {
        let ctx = contexto_comun(&m.entorno(), pagina, idioma);
        assert_eq!(
            ctx["alternativa"]["href"], esperado,
            "{pagina:?} {idioma:?}"
        );
    }
    let es = contexto_comun(&m.entorno(), Pagina::Descargas, Idioma::Es);
    assert_eq!(es["alternativa"]["texto"], "English");
    let en = contexto_comun(&m.entorno(), Pagina::Descargas, Idioma::En);
    assert_eq!(en["alternativa"]["texto"], "Español");
}

#[test]
fn la_navegacion_es_relativa_y_el_manual_cuelga_de_la_raiz() {
    let m = mundo();
    let ctx = contexto_comun(&m.entorno(), Pagina::Descargas, Idioma::Es);
    assert_eq!(ctx["raiz"], "../");
    assert_eq!(ctx["pagina"], "descargas");
    assert_eq!(ctx["nav"]["inicio"]["href"], "../");
    assert_eq!(ctx["nav"]["descargas"]["href"], "../descargas/");
    assert_eq!(ctx["nav"]["versiones"]["href"], "../versiones/");
    assert_eq!(ctx["nav"]["manual"]["href"], "../manual/");

    let ctx = contexto_comun(&m.entorno(), Pagina::Versiones, Idioma::En);
    assert_eq!(ctx["raiz"], "../../");
    assert_eq!(ctx["nav"]["inicio"]["href"], "../../en/");
    assert_eq!(ctx["nav"]["descargas"]["href"], "../../en/download/");
    assert_eq!(ctx["nav"]["versiones"]["href"], "../../en/releases/");
    assert_eq!(ctx["nav"]["manual"]["href"], "../../manual/");
    assert_eq!(ctx["t"]["cabecera"]["manual"], "Manual (Spanish)");

    // La portada castellana enlaza a sí misma sin un href vacío.
    let ctx = contexto_comun(&m.entorno(), Pagina::Portada, Idioma::Es);
    assert_eq!(ctx["raiz"], "");
    assert_eq!(ctx["nav"]["inicio"]["href"], "./");
    assert_eq!(ctx["nav"]["manual"]["href"], "manual/");
}

#[test]
fn en_el_404_los_enlaces_son_absolutos() {
    let m = mundo();
    let ctx = contexto_comun(&m.entorno(), Pagina::NoEncontrada, Idioma::Es);
    assert_eq!(ctx["raiz"], "https://magi.erratia.com/");
    assert_eq!(ctx["nav"]["inicio"]["href"], "https://magi.erratia.com/");
    assert_eq!(
        ctx["nav"]["descargas"]["href"],
        "https://magi.erratia.com/descargas/"
    );
    assert_eq!(
        ctx["nav"]["versiones"]["href"],
        "https://magi.erratia.com/versiones/"
    );
    assert_eq!(
        ctx["nav"]["manual"]["href"],
        "https://magi.erratia.com/manual/"
    );
    assert_eq!(ctx["alternativa"]["href"], "https://magi.erratia.com/en/");
    assert_eq!(ctx["pagina"], "no_encontrada");
}

// ───────────────────────── descargas ─────────────────────────

#[test]
fn la_descarga_de_una_plataforma_en_pruebas_lo_dice_en_texto() {
    let m = mundo();
    let eleccion = eleccion_completa(&m.plataformas);

    let es = descargas(&m, Idioma::Es, &eleccion);
    for id in ["macos-arm64", "macos-x86_64", "termux-arm64"] {
        let marca = format!("id=\"plataforma-{id}\"");
        let fila = trozo(&es, &marca, "</li>");
        assert!(
            fila.contains("estado-en_pruebas\">en pruebas</span>"),
            "{id}: {fila}"
        );
        assert!(
            fila.contains(
                "<a href=\"../manual/estado.html\" hreflang=\"es\">Qué se ha probado</a>"
            ),
            "{id} no enlaza a «Qué se ha probado»: {fila}"
        );
    }
    let app = trozo(&es, "id=\"plataforma-android-arm64\"", "</li>");
    assert!(app.contains("estado-beta\">beta</span>"), "{app}");
    assert!(app.contains("../manual/estado.html"), "{app}");
    // Las estables lo dicen y no mandan a «Qué se ha probado».
    for id in ["linux-x86_64", "linux-arm64"] {
        let marca = format!("id=\"plataforma-{id}\"");
        let fila = trozo(&es, &marca, "</li>");
        assert!(
            fila.contains("estado-estable\">estable</span>"),
            "{id}: {fila}"
        );
        assert!(!fila.contains("estado.html"), "{id}: {fila}");
    }

    let en = descargas(&m, Idioma::En, &eleccion);
    let fila = trozo(&en, "id=\"plataforma-macos-arm64\"", "</li>");
    assert!(
        fila.contains("estado-en_pruebas\">in testing</span>"),
        "{fila}"
    );
    assert!(fila.contains("href=\"../../manual/estado.html\""), "{fila}");
    assert!(fila.contains("What has been tested (Spanish)"), "{fila}");
    let fila = trozo(&en, "id=\"plataforma-linux-x86_64\"", "</li>");
    assert!(fila.contains("estado-estable\">stable</span>"), "{fila}");
}

#[test]
fn todas_las_plataformas_salen_a_la_vista_y_en_orden() {
    let m = mundo();
    let ids = [
        "linux-x86_64",
        "linux-arm64",
        "macos-arm64",
        "macos-x86_64",
        "termux-arm64",
        "android-arm64",
    ];
    for idioma in Idioma::TODOS {
        let html = descargas(&m, idioma, &eleccion_completa(&m.plataformas));
        let mut anterior = 0;
        for id in ids {
            let posicion = html
                .find(&format!("id=\"plataforma-{id}\""))
                .unwrap_or_else(|| panic!("falta {id} en {}", idioma.codigo()));
            assert!(posicion > anterior, "{id} sale antes de lo que debe");
            anterior = posicion;
        }
        // Cajas en orden: Linux, macOS, Termux, app y, al final, «Verificar».
        let mut anterior = 0;
        for caja in [
            "grupo-linux",
            "grupo-macos",
            "grupo-termux",
            "grupo-android",
            "verificar",
        ] {
            let posicion = html.find(&format!("id=\"{caja}\"")).expect(caja);
            assert!(posicion > anterior, "{caja} sale antes de lo que debe");
            anterior = posicion;
        }
        // Nada plegado ni oculto ni con JavaScript.
        assert!(!html.contains("<details"), "las descargas no van plegadas");
        assert!(!html.contains("hidden"), "nada oculto");
        assert!(!html.to_lowercase().contains("<script"));
    }

    // Aunque las plataformas lleguen desordenadas, salen por su `orden`.
    let mut al_reves = mundo();
    al_reves.plataformas.plataforma.reverse();
    let eleccion = eleccion_completa(&al_reves.plataformas);
    let html = descargas(&al_reves, Idioma::Es, &eleccion);
    let mut anterior = 0;
    for id in ids {
        let posicion = html.find(&format!("id=\"plataforma-{id}\"")).expect(id);
        assert!(
            posicion > anterior,
            "{id} no sigue el orden de plataformas.toml"
        );
        anterior = posicion;
    }
}

#[test]
fn el_nombre_de_un_artefacto_se_escapa_en_el_html() {
    let m = mundo();
    let mut eleccion = eleccion_completa(&m.plataformas);
    let malo = "<script>alert(1)</script>.tar.xz";
    eleccion.artefactos[0].nombre = malo.to_string();
    eleccion.artefactos[1].nombre = "x\" onmouseover=\"alert(2)\" y='3'.tar.xz".to_string();
    eleccion.escritorio.numero = "<img src=x onerror=alert(4)>".to_string();
    for idioma in Idioma::TODOS {
        let html = descargas(&m, idioma, &eleccion);
        assert!(!html.contains("<script"), "salió un <script> sin escapar");
        assert!(!html.contains("<img"), "salió un <img> sin escapar");
        assert!(
            html.contains("&lt;script&gt;alert(1)&lt;/script&gt;.tar.xz"),
            "el nombre no sale escapado"
        );
        // Las comillas de un atributo no se pueden cerrar desde un dato.
        assert!(!html.contains("onmouseover=\""), "{html}");
        assert!(html.contains("&lt;img src=x onerror=alert(4)&gt;"));
        // También en el bloque «Verificar», que va en <pre><code>.
        let verificar = trozo(&html, "id=\"verificar\"", "</section>");
        assert!(
            verificar.contains("sha256sum -c &lt;script&gt;"),
            "{verificar}"
        );
    }
}

#[test]
fn el_html_de_descargas_lleva_lang_hreflang_y_enlace_para_saltar() {
    let m = mundo();
    let eleccion = eleccion_completa(&m.plataformas);

    let es = descargas(&m, Idioma::Es, &eleccion);
    assert!(
        es.starts_with("<!doctype html>\n<html lang=\"es\">"),
        "{}",
        &es[..60]
    );
    assert!(
        es.contains("<link rel=\"alternate\" hreflang=\"en\" href=\"../en/download/\">"),
        "falta el hreflang a la otra lengua"
    );
    assert!(es.contains("<a class=\"idioma\" href=\"../en/download/\" hreflang=\"en\" lang=\"en\""));
    assert!(es.contains(">English</a>"));
    assert!(es.contains("<title>Descargas · MAGI</title>"));
    // <meta charset> el primero de <head> (la CSP se pone justo después) y viewport.
    assert!(
        es.contains("<head>\n<meta charset=\"utf-8\">\n"),
        "charset no es el primero"
    );
    assert!(es.contains("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"));
    assert!(es.contains("<meta name=\"color-scheme\" content=\"light dark\">"));
    assert!(es.contains("<link rel=\"icon\" href=\"../favicon.svg\" type=\"image/svg+xml\">"));
    assert!(es.contains("<link rel=\"stylesheet\" href=\"../estilos/web.css\">"));
    assert!(
        !es.contains("Content-Security-Policy"),
        "la CSP la pone csp::insertar"
    );
    assert!(!es.contains("referrer"));
    // El enlace para saltar es lo primero del <body>, y su destino es <main>.
    let cuerpo = &es[es.find("<body>").expect("body") + "<body>".len()..];
    assert!(
        cuerpo
            .trim_start()
            .starts_with("<a class=\"saltar\" href=\"#contenido\">Saltar al contenido</a>"),
        "{}",
        &cuerpo[..80]
    );
    assert!(es.contains("<main id=\"contenido\" tabindex=\"-1\">"));
    assert!(es.contains("<nav class=\"menu\" aria-label=\"Navegación principal\">"));
    assert!(es.contains("<h1 class=\"titulo-pagina\">Descargas</h1>"));

    let en = descargas(&m, Idioma::En, &eleccion);
    assert!(en.starts_with("<!doctype html>\n<html lang=\"en\">"));
    assert!(en.contains("<link rel=\"alternate\" hreflang=\"es\" href=\"../../descargas/\">"));
    assert!(
        en.contains("<a class=\"idioma\" href=\"../../descargas/\" hreflang=\"es\" lang=\"es\"")
    );
    assert!(en.contains(">Español</a>"));
    assert!(en.contains("<title>Download · MAGI</title>"));
    assert!(en.contains("<a class=\"saltar\" href=\"#contenido\">Skip to content</a>"));
    assert!(en.contains("<link rel=\"stylesheet\" href=\"../../estilos/web.css\">"));
    assert!(en.contains("aria-label=\"Main navigation\""));
    assert!(en.contains(">Manual (Spanish)</a>"));
    assert!(en.contains("<h1 class=\"titulo-pagina\">Download</h1>"));
}

#[test]
fn la_app_sin_publicar_dice_aun_sin_publicar() {
    let m = mundo();
    let mut eleccion = eleccion_completa(&m.plataformas);
    eleccion.app = None;
    eleccion
        .artefactos
        .retain(|a| a.plataforma != "android-arm64");

    let es = descargas(&m, Idioma::Es, &eleccion);
    let caja = trozo(&es, "id=\"grupo-android\"", "</section>");
    assert!(caja.contains("aún sin publicar"), "{caja}");
    assert!(
        caja.contains("href=\"../manual/app-android.html\""),
        "{caja}"
    );
    // Sin release no hay nada que descargar, verificar ni configurar en Obtainium.
    assert!(!caja.contains("<dl"), "{caja}");
    assert!(!caja.contains("Obtainium"), "{caja}");
    assert!(!es.contains("apksigner"));
    assert!(
        !es.contains("magi-android-"),
        "el filtro de APK solo sale con la app publicada"
    );
    assert!(!es.contains(&huella()));
    let versiones = trozo(&es, "id=\"version-app\"", "</li>");
    assert!(versiones.contains("aún sin publicar"), "{versiones}");
    // El resto sigue ahí.
    assert!(es.contains("id=\"plataforma-linux-x86_64\""));

    let en = descargas(&m, Idioma::En, &eleccion);
    let caja = trozo(&en, "id=\"grupo-android\"", "</section>");
    assert!(caja.contains("not published yet"), "{caja}");
    assert!(
        caja.contains("href=\"../../manual/app-android.html\""),
        "{caja}"
    );
}

#[test]
fn la_app_publicada_ensena_obtainium_y_la_huella() {
    let m = mundo();
    let eleccion = eleccion_completa(&m.plataformas);
    let es = descargas(&m, Idioma::Es, &eleccion);
    let caja = trozo(&es, "id=\"grupo-android\"", "</section>");
    assert!(
        caja.contains("https://github.com/HectorEspejo/magi</code>"),
        "{caja}"
    );
    assert!(caja.contains("<code>android-v</code>"), "{caja}");
    assert!(caja.contains("<code>magi-android-</code>"), "{caja}");
    assert!(
        caja.contains("href=\"https://github.com/ImranR98/Obtainium\""),
        "{caja}"
    );
    assert!(caja.contains(&huella()), "{caja}");
    assert!(caja.contains("magi-android-v0.2.0.apk"), "{caja}");
    assert!(caja.contains("0.2.0"), "{caja}");
    let verificar = trozo(&es, "id=\"verificar-app\"", "</p>");
    assert!(
        verificar.contains("apksigner verify --print-certs magi-android-v0.2.0.apk"),
        "{verificar}"
    );
    assert!(
        verificar.contains("sha256sum -c magi-android-v0.2.0.apk.sha256"),
        "{verificar}"
    );
    assert!(
        verificar
            .contains("gh attestation verify magi-android-v0.2.0.apk --repo HectorEspejo/magi"),
        "{verificar}"
    );
    // El de escritorio no pide apksigner.
    let escritorio = trozo(&es, "id=\"verificar-escritorio\"", "</pre>");
    assert!(!escritorio.contains("apksigner"), "{escritorio}");
}

#[test]
fn termux_ensena_su_version_aunque_sea_anterior() {
    let m = mundo();
    let mut eleccion = eleccion_completa(&m.plataformas);
    eleccion.escritorio = version(Canal::Escritorio, "v1.6.2", "1.6.2", "2026-12-10T10:00:00Z");
    for a in eleccion
        .artefactos
        .iter_mut()
        .filter(|a| a.canal == Canal::Escritorio)
    {
        a.etiqueta = "v1.6.2".to_string();
        a.nombre = a.nombre.replace("1.6.1", "1.6.2");
        a.url = a.url.replace("v1.6.1", "v1.6.2");
    }
    let html = descargas(&m, Idioma::Es, &eleccion);

    let termux = trozo(&html, "id=\"plataforma-termux-arm64\"", "</li>");
    assert!(termux.contains("<dd>1.6.1</dd>"), "{termux}");
    assert!(termux.contains("magi-ssh_1.6.1_aarch64.deb"), "{termux}");
    assert!(
        termux.contains("apt install ./magi-ssh_1.6.1_aarch64.deb"),
        "{termux}"
    );
    let linux = trozo(&html, "id=\"plataforma-linux-x86_64\"", "</li>");
    assert!(linux.contains("<dd>1.6.2</dd>"), "{linux}");
    let versiones = trozo(&html, "id=\"version-termux\"", "</li>");
    assert!(
        versiones.contains("releases/tag/v1.6.1\">1.6.1</a>"),
        "{versiones}"
    );
    let versiones = trozo(&html, "id=\"version-escritorio\"", "</li>");
    assert!(
        versiones.contains("releases/tag/v1.6.2\">1.6.2</a>"),
        "{versiones}"
    );

    // Sin ningún .deb publicado, Termux dice que aún no hay.
    let mut sin_termux = eleccion.clone();
    sin_termux.termux = None;
    sin_termux.artefactos.retain(|a| a.canal != Canal::Termux);
    let html = descargas(&m, Idioma::Es, &sin_termux);
    let caja = trozo(&html, "id=\"grupo-termux\"", "</section>");
    assert!(caja.contains("aún sin publicar"), "{caja}");
    assert!(!html.contains("apt install"), "{html}");
}

#[test]
fn homebrew_y_aur_solo_salen_si_estan_activos() {
    let m = mundo();
    let base = eleccion_completa(&m.plataformas);
    for idioma in Idioma::TODOS {
        let apagados = descargas(&m, idioma, &base);
        assert!(
            !apagados.contains("brew install"),
            "Homebrew sin su variable"
        );
        assert!(!apagados.contains("yay -S"), "AUR sin su variable");
        assert!(!apagados.contains("Homebrew") && !apagados.contains("AUR"));

        let mut solo_brew = base.clone();
        solo_brew.homebrew = true;
        let html = descargas(&m, idioma, &solo_brew);
        assert!(html.contains("brew install HectorEspejo/tap/magi-ssh"));
        assert!(!html.contains("yay -S"));
        // Cada uno en la caja de su sistema.
        let macos = trozo(&html, "id=\"grupo-macos\"", "</section>");
        assert!(
            macos.contains("brew install HectorEspejo/tap/magi-ssh"),
            "{macos}"
        );

        let mut solo_aur = base.clone();
        solo_aur.aur = true;
        let html = descargas(&m, idioma, &solo_aur);
        assert!(html.contains("yay -S magi-ssh-bin"));
        assert!(!html.contains("brew install"));
        let linux = trozo(&html, "id=\"grupo-linux\"", "</section>");
        assert!(linux.contains("yay -S magi-ssh-bin"), "{linux}");

        let mut los_dos = base.clone();
        los_dos.homebrew = true;
        los_dos.aur = true;
        let html = descargas(&m, idioma, &los_dos);
        assert!(html.contains("brew install") && html.contains("yay -S"));
    }
}

#[test]
fn cada_plataforma_enseña_version_tamano_enlace_y_suma_completa() {
    let m = mundo();
    let eleccion = eleccion_completa(&m.plataformas);
    let es = descargas(&m, Idioma::Es, &eleccion);
    let en = descargas(&m, Idioma::En, &eleccion);
    for a in &eleccion.artefactos {
        let marca = format!("id=\"plataforma-{}\"", a.plataforma);
        for html in [&es, &en] {
            let fila = trozo(html, &marca, "</li>");
            assert!(
                fila.contains(&format!("<a href=\"{}\">{}</a>", a.url, a.nombre)),
                "{fila}"
            );
            assert!(
                fila.contains(&format!("<code class=\"suma\">{}</code>", a.sha256)),
                "{fila}"
            );
            assert_eq!(a.sha256.len(), 64);
            assert!(
                fila.contains(&format!("href=\"{}.sha256\"", a.url)),
                "{fila}"
            );
        }
    }
    // 9 542 041 bytes son 9,1 MB; la coma es del castellano y el punto, del inglés.
    let linux_es = trozo(&es, "id=\"plataforma-linux-x86_64\"", "</li>");
    assert!(linux_es.contains("<dd>9,1 MB</dd>"), "{linux_es}");
    assert!(linux_es.contains("<dd>1.6.1</dd>"), "{linux_es}");
    let linux_en = trozo(&en, "id=\"plataforma-linux-x86_64\"", "</li>");
    assert!(linux_en.contains("<dd>9.1 MB</dd>"), "{linux_en}");
    // La fecha de la versión sale en el idioma de la página.
    assert!(es.contains("· 22 nov 2026"), "fecha del escritorio");
    assert!(
        en.contains("· 22 Nov 2026"),
        "fecha del escritorio en inglés"
    );
    assert!(es.contains("· 1 dic 2026"), "fecha de la app");
}

#[test]
fn el_bloque_verificar_lleva_las_ordenes_por_canal() {
    let m = mundo();
    let eleccion = eleccion_completa(&m.plataformas);
    for idioma in Idioma::TODOS {
        let html = descargas(&m, idioma, &eleccion);
        let escritorio = trozo(&html, "id=\"verificar-escritorio\"", "</pre>");
        for fichero in [
            "magi-ssh-x86_64-unknown-linux-gnu.tar.xz",
            "magi-ssh-aarch64-unknown-linux-gnu.tar.xz",
            "magi-ssh-aarch64-apple-darwin.tar.xz",
            "magi-ssh-x86_64-apple-darwin.tar.xz",
        ] {
            assert!(
                escritorio.contains(&format!("sha256sum -c {fichero}.sha256\n")),
                "{escritorio}"
            );
            assert!(
                escritorio.contains(&format!(
                    "gh attestation verify {fichero} --repo HectorEspejo/magi"
                )),
                "{escritorio}"
            );
        }
        let termux = trozo(&html, "id=\"verificar-termux\"", "</pre>");
        assert!(
            termux.contains("sha256sum -c magi-ssh_1.6.1_aarch64.deb.sha256"),
            "{termux}"
        );
        assert!(
            termux.contains(
                "gh attestation verify magi-ssh_1.6.1_aarch64.deb --repo HectorEspejo/magi"
            ),
            "{termux}"
        );
        // Las órdenes van en bloques seleccionables, sin botón de copiar ni JavaScript.
        assert!(html.contains("<pre class=\"orden\"><code>"));
        assert!(!html.to_lowercase().contains("<button"));
        // Si no coincide: no lo instales y avisa (aviso privado).
        let aviso = trozo(&html, "<p class=\"aviso\">", "</p>");
        assert!(
            aviso.contains("href=\"https://github.com/HectorEspejo/magi/security/policy\""),
            "{aviso}"
        );
        // Todas las versiones y la nota de macOS (`shasum`).
        assert!(html.contains("href=\"https://github.com/HectorEspejo/magi/releases\""));
        assert!(html.contains("<code>shasum -a 256 -c</code>"));
    }
}

#[test]
fn cada_canal_enlaza_a_su_capitulo_de_instalacion() {
    let m = mundo();
    let html = descargas(&m, Idioma::Es, &eleccion_completa(&m.plataformas));
    for (caja, capitulo) in [
        ("grupo-linux", "instalacion"),
        ("grupo-macos", "instalacion"),
        ("grupo-termux", "termux"),
        ("grupo-android", "app-android"),
    ] {
        let trozo = trozo(&html, &format!("id=\"{caja}\""), "</section>");
        assert!(
            trozo.contains(&format!(
                "href=\"../manual/{capitulo}.html\" hreflang=\"es\">"
            )),
            "{caja}: {trozo}"
        );
    }
    let en = descargas(&m, Idioma::En, &eleccion_completa(&m.plataformas));
    assert!(en.contains("href=\"../../manual/termux.html\""));
}

#[test]
fn las_instrucciones_de_cada_plataforma_salen_con_su_codigo() {
    let m = mundo();
    let html = descargas(&m, Idioma::Es, &eleccion_completa(&m.plataformas));
    let linux = trozo(&html, "id=\"grupo-linux\"", "</section>");
    assert!(linux.contains("<code>tar -xf</code>"), "{linux}");
    assert!(linux.contains("<code>magi</code>"), "{linux}");
    assert!(
        !linux.contains('`'),
        "los acentos graves tienen que haberse convertido: {linux}"
    );
    // Una sola vez por caja cuando es la misma para todas sus plataformas.
    assert_eq!(
        linux.matches("class=\"instrucciones\"").count(),
        1,
        "{linux}"
    );
    let macos = trozo(&html, "id=\"grupo-macos\"", "</section>");
    assert!(
        macos.contains("xattr -d com.apple.quarantine magi"),
        "{macos}"
    );
    assert!(!html.contains('`'));
}

#[test]
fn el_pie_lleva_licencia_privacidad_seguridad_y_manual() {
    let m = mundo();
    let eleccion = eleccion_completa(&m.plataformas);
    let es = descargas(&m, Idioma::Es, &eleccion);
    let pie = trozo(&es, "<footer class=\"pie\">", "</footer>");
    assert!(
        pie.contains("href=\"https://github.com/HectorEspejo/magi/blob/main/LICENSE-MIT\">MIT</a>"),
        "{pie}"
    );
    assert!(
        pie.contains(
            "href=\"https://github.com/HectorEspejo/magi/blob/main/LICENSE-APACHE\">Apache-2.0</a>"
        ),
        "{pie}"
    );
    assert!(pie.contains("href=\"https://github.com/HectorEspejo/magi\">GitHub</a>"));
    assert!(
        pie.contains("href=\"https://github.com/HectorEspejo/magi/security/policy\">Seguridad</a>")
    );
    assert!(pie.contains("blob/main/CONTRIBUTING.md\">Contribuir</a>"));
    assert!(
        pie.contains("href=\"../manual/\" hreflang=\"es\">Manual de la 1.6.1</a>"),
        "{pie}"
    );
    assert!(
        pie.contains("Sin cookies ni analítica. GitHub, que aloja la web, registra los accesos; el manual guarda en tu navegador el estado de su barra lateral. <a href=\"https://docs.github.com/es/site-policy/privacy-policies/github-general-privacy-statement\">Declaración de privacidad de GitHub</a>."),
        "{pie}"
    );

    let en = descargas(&m, Idioma::En, &eleccion);
    let pie = trozo(&en, "<footer class=\"pie\">", "</footer>");
    assert!(
        pie.contains("blob/main/CONTRIBUTING.en.md\">Contribute</a>"),
        "{pie}"
    );
    assert!(pie.contains("href=\"https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement\">GitHub privacy statement</a>"), "{pie}");
    assert!(pie.contains("Manual (Spanish) for 1.6.1</a>"), "{pie}");
    assert!(!pie.contains("/es/"));
}

#[test]
fn el_menu_marca_la_pagina_actual_con_aria_current() {
    let m = mundo();
    let html = descargas(&m, Idioma::Es, &eleccion_completa(&m.plataformas));
    assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
    assert!(html.contains("<a href=\"../descargas/\" aria-current=\"page\">Descargas</a>"));
}

#[test]
fn el_texto_de_la_version_del_manual_tambien_se_escapa() {
    let m = mundo();
    let mut entorno = m.entorno();
    entorno.version_manual = "<b>1.6.1</b>";
    let html =
        paginas::descargas::renderizar(&entorno, Idioma::Es, &eleccion_completa(&m.plataformas))
            .expect("descargas");
    assert!(!html.contains("<b>1.6.1</b>"));
    assert!(html.contains("&lt;b&gt;1.6.1&lt;/b&gt;"));
}

#[test]
fn un_dato_raro_para_la_pagina_diciendo_que_y_donde() {
    let m = mundo();
    let buena = eleccion_completa(&m.plataformas);
    let renderizar = |eleccion: &EleccionDescargas| {
        paginas::descargas::renderizar(&m.entorno(), Idioma::Es, eleccion)
    };
    assert!(renderizar(&buena).is_ok());

    let con_cambio = |cambiar: &dyn Fn(&mut EleccionDescargas)| {
        let mut e = buena.clone();
        cambiar(&mut e);
        e
    };
    let casos: Vec<(&str, EleccionDescargas, &str)> = vec![
        (
            "URL de otro dominio",
            con_cambio(&|e| e.artefactos[0].url = "https://evil.example/magi.tar.xz".into()),
            "no cuelga de",
        ),
        (
            "URL con javascript:",
            con_cambio(&|e| e.artefactos[0].url = "javascript:alert(1)".into()),
            "no cuelga de",
        ),
        (
            "URL con comillas",
            con_cambio(&|e| {
                e.artefactos[0].url =
                    "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a\"b".into();
            }),
            "no cuelga de",
        ),
        (
            "URL que sale de la release con %2e%2e",
            con_cambio(&|e| {
                e.artefactos[0].url =
                    "https://github.com/HectorEspejo/magi/releases/download/%2e%2e/%2e%2e/otro/x"
                        .into();
            }),
            "no cuelga de",
        ),
        (
            "URL de la release de otro sitio",
            con_cambio(&|e| e.escritorio.url = "https://example.com/release".into()),
            "no cuelga de",
        ),
        (
            "suma en mayúsculas",
            con_cambio(&|e| {
                e.artefactos[0].sha256 = e.artefactos[0].sha256.to_uppercase().replace("31", "AB")
            }),
            "64 hexadecimales",
        ),
        (
            "suma corta",
            con_cambio(&|e| e.artefactos[0].sha256 = "abc".into()),
            "64 hexadecimales",
        ),
        (
            "fecha rara",
            con_cambio(&|e| e.escritorio.fecha = "ayer".into()),
            "no es AAAA-MM-DD",
        ),
        (
            "falta el artefacto de una plataforma",
            con_cambio(&|e| e.artefactos.retain(|a| a.plataforma != "macos-arm64")),
            "falta el artefacto de la plataforma «macos-arm64»",
        ),
        (
            "artefacto de una plataforma desconocida",
            con_cambio(&|e| e.artefactos[0].plataforma = "plan9-mips".into()),
            "no está en plataformas.toml",
        ),
        (
            "artefacto de otra versión",
            con_cambio(&|e| e.artefactos[0].etiqueta = "v9.9.9".into()),
            "v9.9.9",
        ),
        (
            "artefacto sin versión de su canal",
            con_cambio(&|e| e.app = None),
            "no tiene una versión elegida",
        ),
        (
            "versión vacía",
            con_cambio(&|e| e.escritorio.numero = "  ".into()),
            "vacío",
        ),
    ];
    for (nombre, eleccion, mensaje) in casos {
        let error = match renderizar(&eleccion) {
            Ok(_) => panic!("{nombre}: tenía que fallar"),
            Err(e) => e,
        };
        assert!(
            matches!(error, ErrorWeb::DatoRaro { .. }),
            "{nombre}: {error}"
        );
        assert!(error.to_string().contains(mensaje), "{nombre}: {error}");
        assert!(
            error.to_string().contains("descargas"),
            "{nombre} no dice dónde: {error}"
        );
    }
}

#[test]
fn con_la_app_publicada_la_huella_de_plataformas_toml_es_obligatoria() {
    let mut m = mundo();
    let eleccion = eleccion_completa(&m.plataformas);
    for mala in ["", "ab:cd", &huella().to_lowercase()] {
        m.plataformas.app.huella_certificado = mala.to_string();
        let error = paginas::descargas::renderizar(&m.entorno(), Idioma::Es, &eleccion)
            .expect_err("sin huella válida no hay descarga de la app");
        assert!(error.to_string().contains("huella_certificado"), "{error}");
    }
    // Sin app publicada, la huella no hace falta.
    m.plataformas.app.huella_certificado = String::new();
    let mut sin_app = eleccion.clone();
    sin_app.app = None;
    sin_app.artefactos.retain(|a| a.canal != Canal::App);
    assert!(paginas::descargas::renderizar(&m.entorno(), Idioma::Es, &sin_app).is_ok());
}

#[test]
fn si_falta_un_texto_la_pagina_falla_diciendo_cual() {
    let es = fs::read_to_string(web().join("textos/es.toml")).expect("es.toml");
    let en = fs::read_to_string(web().join("textos/en.toml")).expect("en.toml");
    let quitar = |texto: &str| texto.replace("linux = \"Linux\"\n", "");
    let textos = Textos::desde_textos(&quitar(&es), "es.toml", &quitar(&en), "en.toml")
        .expect("las mismas claves");
    let m = mundo();
    let entorno = Entorno {
        plantillas: &m.plantillas,
        textos: &textos,
        plataformas: &m.plataformas,
        version_manual: "1.6.1",
    };
    let error =
        paginas::descargas::renderizar(&entorno, Idioma::Es, &eleccion_completa(&m.plataformas))
            .expect_err("falta descargas.grupo.linux");
    assert!(
        error.to_string().contains("descargas.grupo.linux"),
        "{error}"
    );
}

// ───────────────────────── 404 ─────────────────────────

#[test]
fn el_404_usa_enlaces_absolutos_y_el_resto_relativos() {
    let m = mundo();
    let html = paginas::no_encontrada::renderizar(&m.entorno()).expect("404");
    let enlaces = hrefs(&html);
    assert!(enlaces.len() > 8, "{enlaces:?}");
    for enlace in &enlaces {
        assert!(
            enlace == "#contenido"
                || enlace.starts_with("https://magi.erratia.com/")
                || enlace.starts_with("https://github.com/")
                || enlace.starts_with("https://docs.github.com/"),
            "el 404 lleva un enlace relativo: {enlace}"
        );
    }
    for esperado in [
        "https://magi.erratia.com/estilos/web.css",
        "https://magi.erratia.com/favicon.svg",
        "https://magi.erratia.com/descargas/",
        "https://magi.erratia.com/versiones/",
        "https://magi.erratia.com/manual/",
        "https://magi.erratia.com/en/",
    ] {
        assert!(
            enlaces.iter().any(|e| e == esperado),
            "falta {esperado} en {enlaces:?}"
        );
    }
    // Los dos idiomas, cada uno con su enlace a su portada, y sin hreflang de una página que no existe.
    assert!(html.contains("<html lang=\"es\">"));
    assert!(html.contains("Página no encontrada"));
    assert!(html.contains("<span lang=\"en\">Page not found</span>"));
    assert!(html.contains("<a href=\"https://magi.erratia.com/\">Volver al inicio</a>"));
    assert!(html.contains("<a href=\"https://magi.erratia.com/en/\">Back to the home page</a>"));
    assert!(!html.contains("rel=\"alternate\""));
    assert!(html.contains("<title>Página no encontrada · Page not found · MAGI</title>"));
    assert!(html.contains("<a class=\"saltar\" href=\"#contenido\">"));

    // En cambio, todas las demás páginas solo llevan enlaces relativos o externos de la lista.
    for idioma in Idioma::TODOS {
        let html = descargas(&m, idioma, &eleccion_completa(&m.plataformas));
        for enlace in hrefs(&html) {
            let externo = enlace.contains("://");
            if externo {
                assert!(enlace.starts_with("https://"), "{enlace} no va por https");
                assert!(
                    !enlace.starts_with("https://magi.erratia.com"),
                    "{enlace}: solo el 404 usa la dirección absoluta"
                );
            } else {
                assert!(
                    !enlace.starts_with('/'),
                    "{enlace}: ruta absoluta en el sitio"
                );
            }
        }
    }
}

// ───────────────────────── comprobaciones de todas las páginas ─────────────────────────

fn todas_las_paginas(m: &Mundo) -> Vec<(String, String)> {
    let eleccion = eleccion_completa(&m.plataformas);
    let mut paginas = Vec::new();
    for idioma in Idioma::TODOS {
        paginas.push((
            format!("descargas {}", idioma.codigo()),
            descargas(m, idioma, &eleccion),
        ));
    }
    paginas.push((
        "404".to_string(),
        paginas::no_encontrada::renderizar(&m.entorno()).expect("404"),
    ));
    paginas
}

#[test]
fn las_paginas_generadas_no_llevan_style_ni_script_ni_manejadores() {
    let m = mundo();
    let atributo_style = regex::Regex::new(r"(?i)\sstyle\s*=").expect("expresión");
    let manejador = regex::Regex::new(r"(?i)\son[a-z]+\s*=").expect("expresión");
    for (nombre, html) in todas_las_paginas(&m) {
        let bajo = html.to_lowercase();
        assert!(!bajo.contains("<script"), "{nombre}");
        assert!(!bajo.contains("<style"), "{nombre}");
        assert!(!atributo_style.is_match(&html), "{nombre}");
        assert!(!manejador.is_match(&html), "{nombre}");
        assert!(!bajo.contains("javascript:"), "{nombre}");
    }
}

#[test]
fn los_enlaces_externos_son_https_y_de_la_lista_permitida() {
    let m = mundo();
    let permitidos = [
        "https://github.com/HectorEspejo/magi",
        "https://github.com/ImranR98/Obtainium",
        "https://docs.github.com/",
        "https://cli.github.com/",
        "https://termux.dev",
        "https://keepachangelog.com",
        "https://semver.org",
        "https://magi.erratia.com/",
    ];
    for (nombre, html) in todas_las_paginas(&m) {
        for enlace in hrefs(&html) {
            if enlace.contains(':') {
                assert!(
                    enlace.starts_with("https://"),
                    "{nombre}: {enlace} no va por https"
                );
                assert!(
                    permitidos.iter().any(|p| enlace.starts_with(p)),
                    "{nombre}: {enlace} no es de la lista permitida"
                );
            }
        }
    }
}

#[test]
fn el_html_generado_esta_bien_balanceado() {
    let m = mundo();
    let etiqueta = regex::Regex::new(r"<(/?)([a-zA-Z][a-zA-Z0-9]*)[^>]*>").expect("expresión");
    let vacias = ["meta", "link", "br", "hr", "img", "input"];
    let ids = regex::Regex::new(r#"\sid="([^"]*)""#).expect("expresión");
    for (nombre, html) in todas_las_paginas(&m) {
        assert!(html.starts_with("<!doctype html>"), "{nombre}");
        let mut pila: Vec<String> = Vec::new();
        for captura in etiqueta.captures_iter(&html) {
            let (cierre, tag) = (&captura[1] == "/", captura[2].to_lowercase());
            if vacias.contains(&tag.as_str()) {
                continue;
            }
            if cierre {
                assert_eq!(
                    pila.pop().as_deref(),
                    Some(tag.as_str()),
                    "{nombre}: </{tag}> sin su apertura"
                );
            } else {
                pila.push(tag);
            }
        }
        assert!(pila.is_empty(), "{nombre}: etiquetas sin cerrar {pila:?}");
        // Un solo <h1> y un solo <main>, y todo id es único.
        assert_eq!(html.matches("<h1").count(), 1, "{nombre}");
        assert_eq!(html.matches("<main").count(), 1, "{nombre}");
        let mut vistos = BTreeSet::new();
        for captura in ids.captures_iter(&html) {
            assert!(
                vistos.insert(captura[1].to_string()),
                "{nombre}: id «{}» repetido",
                &captura[1]
            );
        }
    }
}

#[test]
fn cada_pagina_tiene_su_propio_titulo() {
    let m = mundo();
    let titulo = regex::Regex::new(r"<title>([^<]*)</title>").expect("expresión");
    let mut titulos = BTreeSet::new();
    for (nombre, html) in todas_las_paginas(&m) {
        let t = titulo
            .captures(&html)
            .unwrap_or_else(|| panic!("{nombre}: sin <title>"))[1]
            .to_string();
        assert!(t.ends_with("MAGI"), "{nombre}: {t}");
        assert!(titulos.insert(t.clone()), "{nombre}: título repetido «{t}»");
    }
}

#[test]
fn todo_lo_de_plataformas_toml_tiene_su_texto_en_los_dos_idiomas() {
    let m = mundo();
    for idioma in Idioma::TODOS {
        for p in &m.plataformas.plataforma {
            for clave in [
                format!("descargas.plataforma.{}", p.id),
                format!("descargas.estado.{}", p.estado.id()),
                p.instrucciones.clone(),
                format!("descargas.grupo.{}", p.id.split('-').next().unwrap_or("")),
            ] {
                assert!(
                    m.textos.texto(idioma, &clave).is_some(),
                    "falta «{clave}» en {}",
                    idioma.codigo()
                );
            }
            assert!(m
                .textos
                .texto(idioma, &format!("descargas.canal.{}", p.canal.id()))
                .is_some());
            assert!(m
                .textos
                .texto(
                    idioma,
                    &format!("descargas.verificar.canal.{}", p.canal.id())
                )
                .is_some());
        }
    }
    // Y las rutas que usa la página de descargas existen en el mapa del sitio.
    assert_eq!(rutas::ruta(Pagina::Descargas, Idioma::En), "en/download/");
}
