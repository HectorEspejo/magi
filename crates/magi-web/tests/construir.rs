//! `magi-web construir` de extremo a extremo, con la `web/` real del repositorio y
//! los datos de ejemplo de `tests/fijos/releases/`.
//!
//! Sin red ni mdBook: el manual es el HTML de ejemplo de `tests/fijos/manual-html/`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use magi_web::comprobar::{comprobar_sitio, ListaPermitida};
use magi_web::construir::{construir, OpcionesConstruir};
use magi_web::error::ErrorWeb;
use magi_web::rutas::{self, Idioma, Pagina};
use magi_web::textos::Textos;

const HUELLA_DE_PRUEBAS: &str = "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00";

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("raíz del repositorio")
}

fn fijos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fijos")
}

fn releases(nombre: &str) -> PathBuf {
    fijos().join("releases").join(nombre)
}

/// Copia un árbol de ficheros (solo para montar entornos de prueba).
fn copiar(origen: &Path, destino: &Path) {
    fs::create_dir_all(destino).unwrap();
    for e in fs::read_dir(origen).unwrap() {
        let e = e.unwrap();
        let d = destino.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copiar(&e.path(), &d);
        } else {
            fs::copy(e.path(), d).unwrap();
        }
    }
}

/// Una copia de `web/` en una carpeta temporal, para poder estropearla.
fn web_copiada(dir: &Path) -> PathBuf {
    let web = dir.join("web");
    copiar(&raiz().join("web"), &web);
    web
}

fn opciones(web: &Path, releases: &Path, salida: &Path) -> OpcionesConstruir {
    OpcionesConstruir {
        web: web.to_path_buf(),
        releases: releases.to_path_buf(),
        manual: None,
        animacion: None,
        changelog_escritorio: raiz().join("CHANGELOG.md"),
        changelog_app: None,
        salida: salida.to_path_buf(),
        homebrew: false,
        aur: false,
    }
}

/// Todos los ficheros de un árbol con su contenido, para comparar dos sitios.
fn contenido(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn rec(base: &Path, dir: &Path, salida: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                rec(base, &e.path(), salida);
            } else {
                let clave = e.path().strip_prefix(base).unwrap().display().to_string();
                salida.insert(clave, fs::read(e.path()).unwrap());
            }
        }
    }
    let mut mapa = BTreeMap::new();
    rec(dir, dir, &mut mapa);
    mapa
}

fn texto(dir: &Path, ruta: &str) -> String {
    fs::read_to_string(dir.join(ruta)).unwrap_or_else(|e| panic!("no se pudo leer {ruta}: {e}"))
}

fn sin_restos_de_trabajo(carpeta_madre: &Path) {
    let restos: Vec<String> = fs::read_dir(carpeta_madre)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".construyendo-") || n.contains(".anterior-"))
        .collect();
    assert!(restos.is_empty(), "quedaron restos de trabajo: {restos:?}");
}

// ---------------------------------------------------------------------------

#[test]
fn construir_genera_las_paginas_y_los_estaticos() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    o.animacion = Some(raiz().join("assets/demo/magi.gif"));

    let informe = construir(&o).expect("construir");

    assert_eq!(informe.version_escritorio, "1.6.1");
    assert_eq!(informe.version_termux.as_deref(), Some("1.6.1"));
    assert_eq!(informe.version_app, None);
    for pagina in [
        "index.html",
        "en/index.html",
        "descargas/index.html",
        "en/download/index.html",
        "versiones/index.html",
        "en/releases/index.html",
        "404.html",
    ] {
        assert!(salida.join(pagina).is_file(), "falta {pagina}");
        assert!(informe.paginas.iter().any(|p| p == pagina), "{pagina}");
    }
    for estatico in [
        "estilos/web.css",
        "capturas/capturas.css",
        "favicon.svg",
        "demo/magi.gif",
        "fuentes/JetBrainsMono-Regular.woff2",
        "fuentes/JetBrainsMono-Bold.woff2",
        "fuentes/magi-simbolos.woff2",
        "fuentes/OFL.txt",
        "fuentes/LICENSE-DejaVu.txt",
    ] {
        assert!(salida.join(estatico).is_file(), "falta {estatico}");
    }
    // Los fragmentos de las capturas van dentro de la portada, no sueltos.
    assert!(!salida.join("capturas/flota.html").exists());
    // Lo que no es de la web no sale: ni `.unicodes` ni el `plataformas.toml`.
    assert!(!salida.join("fuentes/magi-simbolos.unicodes").exists());
    assert!(!salida.join("datos").exists());
    sin_restos_de_trabajo(t.path());
}

#[test]
fn cada_pagina_propia_lleva_su_csp_sin_scripts() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    construir(&opciones(
        &raiz().join("web"),
        &releases("sin-red.json"),
        &salida,
    ))
    .unwrap();
    for pagina in [
        "index.html",
        "en/index.html",
        "descargas/index.html",
        "en/download/index.html",
        "versiones/index.html",
        "en/releases/index.html",
        "404.html",
    ] {
        let html = texto(&salida, pagina);
        let csp = magi_web::csp::extraer(&html)
            .unwrap()
            .unwrap_or_else(|| panic!("{pagina} no lleva CSP"));
        assert!(csp.contains("script-src 'none'"), "{pagina}: {csp}");
        assert!(!csp.contains("unsafe-inline"), "{pagina}: {csp}");
        assert!(!html.contains("<script"), "{pagina} lleva un <script>");
        assert!(
            !html.contains(" style="),
            "{pagina} lleva un atributo style"
        );
    }
}

#[test]
fn construir_dos_veces_da_el_mismo_sitio() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    let o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    construir(&o).unwrap();
    let primero = contenido(&salida);
    construir(&o).unwrap();
    assert_eq!(primero, contenido(&salida));
    sin_restos_de_trabajo(t.path());
}

#[test]
fn un_sha256_con_otro_nombre_falla_y_el_sitio_anterior_no_cambia() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    fs::create_dir_all(&salida).unwrap();
    fs::write(salida.join("anterior.html"), "web publicada").unwrap();

    // Con la huella puesta, `completo.json` es válido; `sha_otro_nombre.json` no.
    let web = web_copiada(t.path());
    poner_huella(&web, HUELLA_DE_PRUEBAS);
    let e = construir(&opciones(&web, &releases("sha_otro_nombre.json"), &salida))
        .expect_err("el .sha256 con otro nombre tiene que fallar");
    assert!(matches!(e, ErrorWeb::DatoRaro { .. }), "{e:?}");

    assert_eq!(texto(&salida, "anterior.html"), "web publicada");
    assert_eq!(fs::read_dir(&salida).unwrap().count(), 1);
    sin_restos_de_trabajo(t.path());
}

#[test]
fn un_fallo_a_mitad_no_deja_un_sitio_a_medias() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    fs::create_dir_all(&salida).unwrap();
    fs::write(salida.join("anterior.html"), "web publicada").unwrap();

    // Una plantilla rota: el fallo llega después de empezar a generar.
    let web = web_copiada(t.path());
    fs::write(
        web.join("plantillas/descargas.html"),
        "{% extends \"base.html\" %}{% block contenido %}{{ no_existe.nada }}{% endblock %}",
    )
    .unwrap();
    let e = construir(&opciones(&web, &releases("sin-red.json"), &salida)).unwrap_err();
    assert!(matches!(e, ErrorWeb::Plantilla { .. }), "{e:?}");

    assert_eq!(texto(&salida, "anterior.html"), "web publicada");
    assert_eq!(contenido(&salida).len(), 1);
    sin_restos_de_trabajo(t.path());
}

#[test]
fn la_salida_no_puede_solaparse_con_las_entradas() {
    let t = tempfile::tempdir().unwrap();
    let web = web_copiada(t.path());
    let releases_json = releases("sin-red.json");

    // Dentro de web/ , igual que web/ y por encima de web/.
    for salida in [web.join("sitio"), web.clone(), t.path().to_path_buf()] {
        let e = construir(&opciones(&web, &releases_json, &salida)).unwrap_err();
        assert!(
            e.to_string().contains("se solapa"),
            "salida {}: {e}",
            salida.display()
        );
    }
    assert!(web.join("datos/plataformas.toml").is_file(), "web/ intacta");
    assert!(!web.join("sitio").exists(), "no creó nada en web/");
}

#[test]
fn una_salida_con_dos_puntos_se_rechaza() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("a/../sitio");
    let e = construir(&opciones(
        &raiz().join("web"),
        &releases("sin-red.json"),
        &salida,
    ))
    .unwrap_err();
    assert!(e.to_string().contains("«..»"), "{e}");
}

#[test]
fn la_animacion_tiene_que_ser_un_gif() {
    let t = tempfile::tempdir().unwrap();
    let falso = t.path().join("magi.gif");
    fs::write(&falso, b"<html>no soy un gif</html>").unwrap();
    let mut o = opciones(
        &raiz().join("web"),
        &releases("sin-red.json"),
        &t.path().join("sitio"),
    );
    o.animacion = Some(falso);
    let e = construir(&o).unwrap_err();
    assert!(e.to_string().contains("no es un GIF"), "{e}");
    assert!(!t.path().join("sitio").exists());
    sin_restos_de_trabajo(t.path());
}

#[test]
fn con_una_app_publicada_la_huella_tiene_que_coincidir() {
    let t = tempfile::tempdir().unwrap();
    let web = web_copiada(t.path());

    // Con la huella de la release: sale la app 0.2.0.
    poner_huella(&web, HUELLA_DE_PRUEBAS);
    let salida = t.path().join("bien");
    let mut con_app = opciones(&web, &releases("completo.json"), &salida);
    con_app.changelog_app = Some(raiz().join("android/CHANGELOG.md"));
    let informe = construir(&con_app).unwrap();
    assert_eq!(informe.version_app.as_deref(), Some("0.2.0"));
    assert!(texto(&salida, "descargas/index.html").contains(HUELLA_DE_PRUEBAS));

    // Con otra: falla enseñando las dos y no escribe nada.
    let otra = "AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA";
    poner_huella(&web, otra);
    let mala = t.path().join("mala");
    let mut con_app = opciones(&web, &releases("completo.json"), &mala);
    con_app.changelog_app = Some(raiz().join("android/CHANGELOG.md"));
    let e = construir(&con_app).unwrap_err();
    let mensaje = e.to_string();
    assert!(
        mensaje.contains(HUELLA_DE_PRUEBAS) && mensaje.contains(otra),
        "{mensaje}"
    );
    assert!(!mala.exists());
    sin_restos_de_trabajo(t.path());
}

/// Cambia `huella_certificado` en el `plataformas.toml` de una copia de `web/`.
fn poner_huella(web: &Path, huella: &str) {
    let ruta = web.join("datos/plataformas.toml");
    let texto = fs::read_to_string(&ruta).unwrap();
    let nuevo: Vec<String> = texto
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("huella_certificado") {
                format!("huella_certificado = \"{huella}\"")
            } else {
                l.to_string()
            }
        })
        .collect();
    fs::write(ruta, nuevo.join("\n") + "\n").unwrap();
}

#[test]
fn el_manual_pasa_por_la_csp_con_los_hashes_de_sus_scripts() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    o.manual = Some(fijos().join("manual-html"));

    let informe = construir(&o).unwrap();

    let pagina = texto(&salida, "manual/tuneles.html");
    let csp = magi_web::csp::extraer(&pagina).unwrap().expect("CSP");
    assert!(csp.contains("script-src 'self' 'sha256-"), "{csp}");
    assert!(csp.contains("frame-src 'self'"), "{csp}");
    assert!(!csp.contains("unsafe-inline"), "{csp}");
    // Cada script en línea del manual tiene su hash en la política.
    for script in magi_web::csp::scripts_en_linea(&pagina).unwrap() {
        assert!(
            csp.contains(&magi_web::csp::hash_script(&script)),
            "falta el hash de un script en línea"
        );
    }
    assert!(informe.paginas.iter().any(|p| p == "manual/tuneles.html"));
    assert!(salida.join("manual/toc-2128731a.js").is_file());
}

#[cfg(unix)]
#[test]
fn un_enlace_simbolico_en_el_manual_se_rechaza() {
    let t = tempfile::tempdir().unwrap();
    let manual = t.path().join("manual");
    copiar(&fijos().join("manual-html"), &manual);
    std::os::unix::fs::symlink("/etc/passwd", manual.join("secreto.html")).unwrap();

    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    o.manual = Some(manual);
    let e = construir(&o).unwrap_err();
    assert!(e.to_string().contains("enlace simbólico"), "{e}");
    assert!(!salida.exists());
    sin_restos_de_trabajo(t.path());
}

#[test]
fn del_manual_solo_se_copian_las_extensiones_permitidas() {
    let t = tempfile::tempdir().unwrap();
    let manual = t.path().join("manual");
    copiar(&fijos().join("manual-html"), &manual);
    fs::write(manual.join("notas.sh"), "#!/bin/sh\nrm -rf /\n").unwrap();
    fs::write(manual.join("clave.pem"), "no").unwrap();

    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    o.manual = Some(manual);
    let informe = construir(&o).unwrap();

    assert!(!salida.join("manual/notas.sh").exists());
    assert!(!salida.join("manual/clave.pem").exists());
    assert!(informe.omitidos.iter().any(|o| o == "notas.sh"));
    assert!(informe.omitidos.iter().any(|o| o == "clave.pem"));
}

#[test]
fn un_changelog_que_no_trae_la_version_de_la_release_falla() {
    let t = tempfile::tempdir().unwrap();
    let viejo = t.path().join("CHANGELOG.md");
    fs::write(
        &viejo,
        "# Cambios\n\n## [1.5.0] - 2026-01-02\n\n**Resumen.** Algo.\n\n**Summary.** Something.\n",
    )
    .unwrap();
    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    o.changelog_escritorio = viejo;
    let e = construir(&o).expect_err("el CHANGELOG no es el de la 1.6.1");
    let mensaje = e.to_string();
    assert!(
        mensaje.contains("1.6.1") && mensaje.contains("v1.6.1"),
        "{mensaje}"
    );
    assert!(!salida.exists());
    sin_restos_de_trabajo(t.path());
}

#[test]
fn un_changelog_sin_resumen_en_los_dos_idiomas_falla_diciendo_la_version() {
    let t = tempfile::tempdir().unwrap();
    let malo = t.path().join("CHANGELOG.md");
    fs::write(
        &malo,
        "## [1.6.1] - 2026-11-22\n\n**Resumen.** Solo en castellano.\n\nDetalle.\n",
    )
    .unwrap();
    let mut o = opciones(
        &raiz().join("web"),
        &releases("sin-red.json"),
        &t.path().join("sitio"),
    );
    o.changelog_escritorio = malo;
    let e = construir(&o).expect_err("falta el Summary").to_string();
    assert!(e.contains("1.6.1") && e.contains("Summary"), "{e}");
    assert!(!t.path().join("sitio").exists());
}

#[test]
fn con_una_app_publicada_hace_falta_su_changelog() {
    let t = tempfile::tempdir().unwrap();
    let web = web_copiada(t.path());
    poner_huella(&web, HUELLA_DE_PRUEBAS);
    let salida = t.path().join("sitio");
    let e = construir(&opciones(&web, &releases("completo.json"), &salida))
        .expect_err("la app tiene release y no hay CHANGELOG");
    let mensaje = e.to_string();
    assert!(
        mensaje.contains("--changelog-app") && mensaje.contains("android-v0.2.0"),
        "{mensaje}"
    );
    assert!(!salida.exists());
}

#[test]
fn con_el_changelog_de_la_app_equivocado_falla() {
    let t = tempfile::tempdir().unwrap();
    let web = web_copiada(t.path());
    poner_huella(&web, HUELLA_DE_PRUEBAS);
    let mut o = opciones(&web, &releases("completo.json"), &t.path().join("sitio"));
    // El del escritorio no trae la 0.2.0 de la app.
    o.changelog_app = Some(raiz().join("CHANGELOG.md"));
    let e = construir(&o).expect_err("es el CHANGELOG de otro canal");
    assert!(e.to_string().contains("0.2.0"), "{e}");
}

#[test]
fn las_versiones_salen_del_changelog_en_los_dos_idiomas() {
    let t = tempfile::tempdir().unwrap();
    let web = web_copiada(t.path());
    poner_huella(&web, HUELLA_DE_PRUEBAS);
    let salida = t.path().join("sitio");
    let mut o = opciones(&web, &releases("completo.json"), &salida);
    o.changelog_app = Some(raiz().join("android/CHANGELOG.md"));
    o.animacion = Some(raiz().join("assets/demo/magi.gif"));
    construir(&o).unwrap();

    let es = texto(&salida, "versiones/index.html");
    let en = texto(&salida, "en/releases/index.html");
    for (idioma, pagina) in [("es", &es), ("en", &en)] {
        assert!(
            pagina.contains("1.6.1"),
            "{idioma}: la versión del escritorio"
        );
        assert!(pagina.contains("0.2.0"), "{idioma}: la versión de la app");
        // Nunca la sección «Sin publicar» ni los comentarios de plantilla.
        assert!(!pagina.contains("Sin publicar"), "{idioma}");
        assert!(!pagina.contains("<!--"), "{idioma}");
    }
    // El resumen sale en el idioma de la página, y el detalle, siempre en castellano.
    assert!(es.contains("<p class=\"version-resumen\">Primera versión publicada de MAGI"));
    assert!(!es.contains("First published release"));
    assert!(en.contains("<p class=\"version-resumen\">First published release of MAGI"));
    assert!(!en.contains("<p class=\"version-resumen\">Primera versión"));
    assert!(en.contains("<summary>Details (in Spanish)</summary>"));
    // Cada versión enlaza a su release, la del escritorio y la de la app.
    for pagina in [&es, &en] {
        assert!(
            pagina.contains("href=\"https://github.com/HectorEspejo/magi/releases/tag/v1.6.1\"")
        );
        assert!(pagina
            .contains("href=\"https://github.com/HectorEspejo/magi/releases/tag/android-v0.2.0\""));
    }

    // Con los dos CHANGELOG reales dentro, `comprobar` solo echa en falta el manual:
    // ningún enlace del CHANGELOG sale de la lista permitida ni apunta a un ancla rota.
    let hallazgos = comprobar_sitio(&salida, &ListaPermitida::por_defecto()).unwrap();
    let ajenos: Vec<String> = hallazgos
        .iter()
        .filter(|h| !(h.que.contains("enlace roto") && h.que.contains("manual/")))
        .map(ToString::to_string)
        .collect();
    assert!(ajenos.is_empty(), "{ajenos:#?}");
}

#[test]
fn sin_release_de_la_app_su_changelog_no_se_usa_y_la_pagina_lo_dice() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    o.changelog_app = Some(raiz().join("android/CHANGELOG.md"));
    let informe = construir(&o).unwrap();
    assert_eq!(informe.version_app, None);
    let es = texto(&salida, "versiones/index.html");
    // La 0.2.0 del CHANGELOG de la app no se publica mientras no haya release.
    assert!(
        !es.contains("0.2.0"),
        "no debe salir una versión sin release"
    );
    let textos = Textos::cargar(&raiz().join("web/textos")).unwrap();
    for idioma in Idioma::TODOS {
        let vacia = textos.texto(idioma, "versiones.app_vacia").unwrap();
        let pagina = texto(&salida, &rutas::fichero(Pagina::Versiones, idioma));
        // La plantilla escapa el texto (el apóstrofo inglés sale como `&#x27;`).
        let escrito = vacia
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;");
        assert!(
            pagina.contains(&escrito),
            "{idioma:?}: debería decir «{vacia}»"
        );
    }
}

#[test]
fn faltan_las_capturas_y_la_construccion_se_para_diciendo_cual() {
    let t = tempfile::tempdir().unwrap();
    let web = web_copiada(t.path());
    fs::remove_file(web.join("capturas/archivos.html")).unwrap();
    let salida = t.path().join("sitio");
    fs::create_dir_all(&salida).unwrap();
    fs::write(salida.join("anterior.html"), "web publicada").unwrap();
    let e = construir(&opciones(&web, &releases("sin-red.json"), &salida))
        .expect_err("falta una captura");
    let mensaje = e.to_string();
    assert!(
        mensaje.contains("archivos") && mensaje.contains("generar-capturas.sh"),
        "{mensaje}"
    );
    assert_eq!(texto(&salida, "anterior.html"), "web publicada");
    sin_restos_de_trabajo(t.path());
}

#[test]
fn una_captura_con_un_script_se_rechaza_y_no_se_publica_nada() {
    let t = tempfile::tempdir().unwrap();
    let web = web_copiada(t.path());
    fs::write(
        web.join("capturas/flota.html"),
        "<pre class=\"captura\" aria-hidden=\"true\"><script>alert(1)</script></pre>\n",
    )
    .unwrap();
    let salida = t.path().join("sitio");
    let e = construir(&opciones(&web, &releases("sin-red.json"), &salida))
        .expect_err("un script en una captura");
    assert!(e.to_string().contains("flota.html"), "{e}");
    assert!(!salida.exists());
    sin_restos_de_trabajo(t.path());
}

/// Lo que más pesa la portada: su HTML y todo lo que carga (hojas, imágenes, la
/// animación y las fuentes de las hojas), en bytes (checklist: menos de 1 MB).
const PESO_MAXIMO_PORTADA: u64 = 1_000_000;

/// Peso de una página y de los recursos locales que carga, con el detalle.
fn peso_de_la_pagina(sitio: &Path, pagina: &str) -> (u64, Vec<(String, u64)>) {
    use regex::Regex;
    let referencia =
        Regex::new(r##"<(?:link|img|source)\b[^>]*?\b(?:href|src)="([^"#?]+)""##).unwrap();
    let url_css = Regex::new(r#"url\(\s*["']?([^"')]+)["']?\s*\)"#).unwrap();
    let mut recursos: Vec<(String, u64)> = vec![(pagina.to_string(), 0)];
    let mut vistos = vec![pagina.to_string()];
    let carpeta_de = |ruta: &str| -> PathBuf {
        let mut c = PathBuf::from(ruta);
        c.pop();
        c
    };
    let mut pendientes = vec![pagina.to_string()];
    while let Some(ruta) = pendientes.pop() {
        let completa = sitio.join(&ruta);
        if !ruta.ends_with(".html") && !ruta.ends_with(".css") {
            continue;
        }
        let texto = fs::read_to_string(&completa).unwrap();
        let patron = if ruta.ends_with(".css") {
            &url_css
        } else {
            &referencia
        };
        for c in patron.captures_iter(&texto) {
            let destino = c[1].to_string();
            if destino.contains("://") || destino.ends_with('/') || destino.ends_with(".html") {
                continue; // enlaces a otras páginas o a otros sitios: no se cargan
            }
            let normal = normalizar(&carpeta_de(&ruta).join(&destino));
            if !vistos.contains(&normal) {
                vistos.push(normal.clone());
                recursos.push((normal.clone(), 0));
                pendientes.push(normal);
            }
        }
    }
    for (ruta, peso) in &mut recursos {
        *peso = fs::metadata(sitio.join(&*ruta))
            .unwrap_or_else(|e| panic!("la portada carga {ruta} y no está en el sitio: {e}"))
            .len();
    }
    (recursos.iter().map(|(_, p)| p).sum(), recursos)
}

/// Quita `.` y `..` de una ruta relativa al sitio.
fn normalizar(ruta: &Path) -> String {
    let mut partes: Vec<String> = Vec::new();
    for c in ruta.components() {
        match c {
            std::path::Component::ParentDir => {
                partes.pop();
            }
            std::path::Component::Normal(n) => partes.push(n.to_string_lossy().into_owned()),
            _ => {}
        }
    }
    partes.join("/")
}

#[test]
fn la_portada_pesa_menos_de_un_megabyte_en_los_dos_idiomas() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    // La animación de verdad: es lo que más pesa.
    o.animacion = Some(raiz().join("assets/demo/magi.gif"));
    construir(&o).unwrap();
    for pagina in ["index.html", "en/index.html"] {
        let (total, detalle) = peso_de_la_pagina(&salida, pagina);
        assert!(
            detalle.iter().any(|(r, _)| r == "demo/magi.gif"),
            "{pagina} debería cargar la animación: {detalle:#?}"
        );
        assert!(
            detalle.iter().any(|(r, _)| r.ends_with(".woff2")),
            "{pagina} debería cargar las fuentes: {detalle:#?}"
        );
        assert!(
            total < PESO_MAXIMO_PORTADA,
            "{pagina} pesa {total} bytes (máximo {PESO_MAXIMO_PORTADA}): {detalle:#?}"
        );
    }
}

// CONTRATO-S3-ENLACES: con el manual de ejemplo de `tests/fijos/manual-html/`
// (que no es el real) quedan enlaces rotos a sus capítulos. Al cerrar el Sprint 3
// esta prueba pasa a construir con el manual de verdad y a exigir cero hallazgos.
#[test]
fn el_sitio_solo_tiene_enlaces_rotos_al_manual_que_aun_no_esta() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    let mut o = opciones(&raiz().join("web"), &releases("sin-red.json"), &salida);
    o.animacion = Some(raiz().join("assets/demo/magi.gif"));
    construir(&o).unwrap();
    let hallazgos = comprobar_sitio(&salida, &ListaPermitida::por_defecto()).unwrap();
    let ajenos: Vec<String> = hallazgos
        .iter()
        .filter(|h| !(h.que.contains("enlace roto") && h.que.contains("manual/")))
        .map(ToString::to_string)
        .collect();
    assert!(
        ajenos.is_empty(),
        "problemas que no son el manual pendiente: {ajenos:#?}"
    );
}
