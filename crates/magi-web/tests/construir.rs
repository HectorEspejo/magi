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
    for pagina in ["descargas/index.html", "en/download/index.html", "404.html"] {
        assert!(salida.join(pagina).is_file(), "falta {pagina}");
        assert!(informe.paginas.iter().any(|p| p == pagina), "{pagina}");
    }
    for estatico in [
        "estilos/web.css",
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
    for pagina in ["descargas/index.html", "en/download/index.html", "404.html"] {
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
    let informe = construir(&opciones(&web, &releases("completo.json"), &salida)).unwrap();
    assert_eq!(informe.version_app.as_deref(), Some("0.2.0"));
    assert!(texto(&salida, "descargas/index.html").contains(HUELLA_DE_PRUEBAS));

    // Con otra: falla enseñando las dos y no escribe nada.
    let otra = "AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA";
    poner_huella(&web, otra);
    let mala = t.path().join("mala");
    let e = construir(&opciones(&web, &releases("completo.json"), &mala)).unwrap_err();
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

// CONTRATO-S3-ENLACES: en el Sprint 1 aún no hay portada, versiones ni
// manual, así que lo único que `comprobar` puede encontrar son enlaces a ellos. Al
// cerrar el Sprint 3 esta prueba se sustituye por `el_sitio_construido_no_tiene_
// hallazgos`.
#[test]
fn el_sitio_de_s1_solo_tiene_enlaces_rotos_a_paginas_pendientes() {
    let t = tempfile::tempdir().unwrap();
    let salida = t.path().join("sitio");
    construir(&opciones(
        &raiz().join("web"),
        &releases("sin-red.json"),
        &salida,
    ))
    .unwrap();
    let hallazgos = comprobar_sitio(&salida, &ListaPermitida::por_defecto()).unwrap();
    assert!(!hallazgos.is_empty(), "debería quejarse de lo que falta");
    let ajenos: Vec<String> = hallazgos
        .iter()
        .filter(|h| {
            !(h.que.contains("enlace roto")
                && (h.que.contains("index.html")
                    || h.que.contains("versiones/")
                    || h.que.contains("releases/")
                    || h.que.contains("manual/")))
        })
        .map(ToString::to_string)
        .collect();
    assert!(
        ajenos.is_empty(),
        "problemas que no son páginas pendientes: {ajenos:#?}"
    );
}
