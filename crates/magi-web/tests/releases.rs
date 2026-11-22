//! Pruebas de `releases`: el `releases.json`, las fuentes (`gh`/`curl` falsos y
//! datos fijos) y `descargar_releases`. Sin red.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;

use magi_web::datos::Plataformas;
use magi_web::descargas::{elegir, OpcionesDescargas};
use magi_web::error::{ErrorWeb, Resultado};
use magi_web::releases::{
    descargar_releases, DocumentoReleases, Fuente, FuenteFijos, FuenteGh, PAGINAS_MAXIMAS,
    TOPE_PAGINA_API, TOPE_SHA256,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const HUELLA: &str = "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00";
const LINUX: &str = "magi-ssh-x86_64-unknown-linux-gnu.tar.xz";

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn plataformas_con(huella: &str) -> Plataformas {
    let texto = std::fs::read_to_string(raiz().join("../../web/datos/plataformas.toml")).unwrap();
    let texto = texto.replacen(
        "huella_certificado = \"\"",
        &format!("huella_certificado = \"{huella}\""),
        1,
    );
    Plataformas::desde_texto(&texto, "plataformas.toml").unwrap()
}

fn plataformas() -> Plataformas {
    plataformas_con("")
}

fn pagina_real() -> String {
    std::fs::read_to_string(raiz().join("tests/fijos/releases/api_pagina_1.json")).unwrap()
}

fn pagina_valor() -> Vec<Value> {
    serde_json::from_str(&pagina_real()).unwrap()
}

fn hash_falso(etiqueta: &str, fichero: &str) -> String {
    Sha256::digest(format!("fixture-falso:{etiqueta}/{fichero}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Una fuente con la página dada y el `.sha256` de cada activo `*.sha256` de ella.
fn fuente(paginas: &[Value]) -> FuenteFijos {
    let mut sumas = BTreeMap::new();
    for release in paginas {
        let etiqueta = release["tag_name"].as_str().unwrap();
        for activo in release["assets"].as_array().unwrap() {
            let nombre = activo["name"].as_str().unwrap();
            if let Some(fichero) = nombre.strip_suffix(".sha256") {
                sumas.insert(
                    activo["browser_download_url"].as_str().unwrap().to_string(),
                    format!("{}  {}\n", hash_falso(etiqueta, fichero), fichero),
                );
            }
        }
    }
    FuenteFijos {
        paginas: vec![serde_json::to_string(paginas).unwrap()],
        sumas,
    }
}

fn raro<T: std::fmt::Debug>(r: Resultado<T>) -> (String, String) {
    match r {
        Err(ErrorWeb::DatoRaro { donde, que }) => (donde, que),
        Err(otro) => panic!("se esperaba DatoRaro y salió: {otro}"),
        Ok(v) => panic!("se esperaba un error y salió {v:?}"),
    }
}

/// Copia la release `v1.6.1` de la página como otra versión (cambia el número en
/// todos sus nombres y URL).
fn clonar_release(pagina: &[Value], de: &str, a: &str) -> Value {
    let origen = pagina.iter().find(|r| r["tag_name"] == de).unwrap();
    let texto = serde_json::to_string(origen)
        .unwrap()
        .replace(de.trim_start_matches('v'), a.trim_start_matches('v'));
    serde_json::from_str(&texto).unwrap()
}

/// Quita de una release de la página los activos que cumplan `quitar`.
fn quitar_activos(release: &mut Value, quitar: impl Fn(&str) -> bool) {
    let activos = release["assets"].as_array_mut().unwrap();
    activos.retain(|a| !quitar(a["name"].as_str().unwrap()));
}

// ---------------------------------------------------------------------------
// DocumentoReleases
// ---------------------------------------------------------------------------

#[test]
fn el_documento_se_escribe_y_se_lee_igual() {
    let doc = DocumentoReleases::leer(&raiz().join("tests/fijos/releases/completo.json")).unwrap();
    assert!(doc.releases.len() >= 5 && !doc.sumas.is_empty());

    let carpeta = tempfile::tempdir().unwrap();
    // La carpeta de salida puede no existir.
    let ruta = carpeta.path().join("trabajo/nueva/releases.json");
    doc.escribir(&ruta).unwrap();
    let leido = DocumentoReleases::leer(&ruta).unwrap();
    assert_eq!(leido, doc);
    let texto = std::fs::read_to_string(&ruta).unwrap();
    assert!(texto.ends_with('\n'));

    // No deja temporales y sustituye lo que había.
    let otro = DocumentoReleases::default();
    otro.escribir(&ruta).unwrap();
    assert_eq!(DocumentoReleases::leer(&ruta).unwrap(), otro);
    let nombres: Vec<String> = std::fs::read_dir(ruta.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(nombres, ["releases.json"]);
}

#[test]
fn clave_suma_es_etiqueta_barra_artefacto() {
    assert_eq!(
        DocumentoReleases::clave_suma("v1.6.1", LINUX),
        format!("v1.6.1/{LINUX}")
    );
}

#[test]
fn leer_un_fichero_que_no_existe_dice_cual() {
    let ruta = raiz().join("tests/fijos/releases/no-existe.json");
    match DocumentoReleases::leer(&ruta) {
        Err(ErrorWeb::Lectura { ruta: r, .. }) => assert_eq!(r, ruta),
        otro => panic!("se esperaba Lectura: {otro:?}"),
    }
}

#[test]
fn un_json_roto_o_con_tipos_equivocados_falla_diciendo_el_origen() {
    for malo in [
        "",
        "no es json",
        "[]",
        "{\"releases\": 3}",
        "{\"releases\": [{\"tag_name\": 1}]}",
        "{\"releases\": [{\"tag_name\": \"v1.0.0\"}]}",
        "{\"sumas\": {}}",
        "{\"releases\": [], \"sumas\": {\"a\": 3}}",
    ] {
        let (donde, que) = raro(DocumentoReleases::desde_texto(
            malo,
            "releases.json de prueba",
        ));
        assert_eq!(donde, "releases.json de prueba", "«{malo}»");
        assert!(que.contains("JSON"), "{que}");
    }
    // Lo mínimo que se acepta.
    let doc = DocumentoReleases::desde_texto("{\"releases\": []}", "x").unwrap();
    assert!(doc.releases.is_empty() && doc.sumas.is_empty());
}

#[test]
fn los_campos_desconocidos_de_la_api_se_ignoran() {
    let texto = r#"{"releases":[{"tag_name":"v1.0.0","draft":false,"prerelease":false,
        "html_url":"https://github.com/HectorEspejo/magi/releases/tag/v1.0.0",
        "campo_nuevo":{"a":[1,2,3]},"assets":[{"name":"a","size":1,"state":"uploaded",
        "browser_download_url":"https://x","otro":null}]}]}"#;
    let doc = DocumentoReleases::desde_texto(texto, "x").unwrap();
    assert_eq!(doc.releases[0].assets[0].name, "a");
    assert!(doc.releases[0].published_at.is_none() && doc.releases[0].body.is_none());
}

#[test]
fn los_topes_de_lo_que_llega_de_fuera_paran() {
    let con_activos = |n: usize| {
        let activos: Vec<Value> = (0..n)
            .map(|i| json!({"name": format!("a{i}"), "size": 1, "state": "uploaded", "browser_download_url": "https://x"}))
            .collect();
        json!({"releases": [{"tag_name": "v1.0.0", "draft": false, "prerelease": false,
            "html_url": "https://x", "assets": activos}]})
        .to_string()
    };
    DocumentoReleases::desde_texto(&con_activos(200), "x").unwrap();
    let (_, que) = raro(DocumentoReleases::desde_texto(&con_activos(201), "x"));
    assert!(
        que.contains("201") && que.contains("200") && que.contains("v1.0.0"),
        "{que}"
    );

    let largo = "a".repeat(5000);
    let doc = json!({"releases": [{"tag_name": largo, "draft": false, "prerelease": false, "html_url": "https://x"}]});
    assert!(DocumentoReleases::desde_texto(&doc.to_string(), "x").is_err());

    // Una suma enorme.
    let doc = json!({"releases": [], "sumas": {"v1/a": "a".repeat(TOPE_SHA256 + 1)}});
    let (_, que) = raro(DocumentoReleases::desde_texto(&doc.to_string(), "x"));
    assert!(que.contains("topes"), "{que}");
}

#[test]
fn el_numero_de_releases_y_de_sumas_tiene_tope() {
    // `PAGINAS_MAXIMAS` páginas de 100: ni una release más, ni una suma más.
    let maximo = PAGINAS_MAXIMAS as usize * 100;
    let release = |i: usize| {
        json!({"tag_name": format!("v1.0.{i}"), "draft": false, "prerelease": false,
            "html_url": "https://x"})
    };
    let con_releases = |n: usize| {
        let lista: Vec<Value> = (0..n).map(release).collect();
        json!({ "releases": lista }).to_string()
    };
    assert_eq!(
        DocumentoReleases::desde_texto(&con_releases(maximo), "x")
            .unwrap()
            .releases
            .len(),
        maximo
    );
    let (donde, que) = raro(DocumentoReleases::desde_texto(
        &con_releases(maximo + 1),
        "releases.json de prueba",
    ));
    assert_eq!(donde, "releases.json de prueba");
    assert!(
        que.contains(&(maximo + 1).to_string()) && que.contains(&maximo.to_string()),
        "{que}"
    );

    let con_sumas = |n: usize| {
        let sumas: BTreeMap<String, String> = (0..n)
            .map(|i| (format!("v1/a{i}"), "x".to_string()))
            .collect();
        json!({"releases": [], "sumas": sumas}).to_string()
    };
    assert_eq!(
        DocumentoReleases::desde_texto(&con_sumas(maximo), "x")
            .unwrap()
            .sumas
            .len(),
        maximo
    );
    let (_, que) = raro(DocumentoReleases::desde_texto(&con_sumas(maximo + 1), "x"));
    assert!(
        que.contains("sumas") && que.contains(&(maximo + 1).to_string()),
        "{que}"
    );
}

// ---------------------------------------------------------------------------
// Fuentes de datos fijos
// ---------------------------------------------------------------------------

#[test]
fn la_fuente_de_datos_fijos_falla_si_falta_una_pagina_o_una_suma() {
    let f = FuenteFijos {
        paginas: vec!["[]".into()],
        sumas: BTreeMap::from([("https://x/a.sha256".to_string(), "texto".to_string())]),
    };
    assert_eq!(f.pagina_releases(1).unwrap(), "[]");
    assert_eq!(f.suma("https://x/a.sha256").unwrap(), "texto");
    for pagina in [0, 2, 99, u32::MAX] {
        let (_, que) = raro(f.pagina_releases(pagina));
        assert!(que.contains(&format!("página {pagina}")), "{que}");
    }
    let (_, que) = raro(f.suma("https://x/b.sha256"));
    assert!(que.contains("b.sha256"), "{que}");
}

// ---------------------------------------------------------------------------
// descargar_releases
// ---------------------------------------------------------------------------

#[test]
fn una_pagina_real_de_la_api_se_descarga_y_se_elige() {
    let pagina = pagina_valor();
    let f = fuente(&pagina);
    // Los campos de más de la API de verdad están ahí: es una página real.
    assert!(pagina_real().contains("\"node_id\""));
    assert!(pagina_real().contains("\"uploader\""));

    let doc = descargar_releases(&f, &plataformas()).unwrap();
    let etiquetas: Vec<&str> = doc.releases.iter().map(|r| r.tag_name.as_str()).collect();
    assert_eq!(
        etiquetas,
        ["v1.6.1", "v1.6.1-rc.1", "artefactos-prueba-fase13"]
    );
    // Las sumas: los cuatro tarballs y el .deb de la 1.6.1, nada más.
    let claves: Vec<&str> = doc.sumas.keys().map(String::as_str).collect();
    assert_eq!(
        claves,
        [
            "v1.6.1/magi-ssh-aarch64-apple-darwin.tar.xz",
            "v1.6.1/magi-ssh-aarch64-unknown-linux-gnu.tar.xz",
            "v1.6.1/magi-ssh-x86_64-apple-darwin.tar.xz",
            "v1.6.1/magi-ssh-x86_64-unknown-linux-gnu.tar.xz",
            "v1.6.1/magi-ssh_1.6.1_aarch64.deb",
        ]
    );
    assert_eq!(
        doc.sumas[&format!("v1.6.1/{LINUX}")],
        format!("{}  {LINUX}\n", hash_falso("v1.6.1", LINUX))
    );
    // De las notas solo se guarda la de la app: aquí, ninguna.
    assert!(doc.releases.iter().all(|r| r.body.is_none()));

    let e = elegir(&doc, &plataformas(), OpcionesDescargas::default()).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert_eq!(e.escritorio.fecha, "2026-11-22T11:04:48Z");
    assert_eq!(e.termux.as_ref().unwrap().etiqueta, "v1.6.1");
    assert!(e.app.is_none());
    assert_eq!(e.artefactos.len(), 5);
    let linux = &e.artefactos[0];
    assert_eq!(linux.nombre, LINUX);
    assert_eq!(linux.tamano, 11_194_224);
    assert_eq!(linux.sha256, hash_falso("v1.6.1", LINUX));

    // Y lo escrito se vuelve a leer y a elegir igual: lo que hace `construir`.
    let carpeta = tempfile::tempdir().unwrap();
    let ruta = carpeta.path().join("releases.json");
    doc.escribir(&ruta).unwrap();
    let leido = DocumentoReleases::leer(&ruta).unwrap();
    assert_eq!(leido, doc);
    assert_eq!(
        elegir(&leido, &plataformas(), OpcionesDescargas::default()).unwrap(),
        e
    );
}

/// Una fuente que cuenta las páginas y las sumas que le piden.
struct Espia {
    dentro: FuenteFijos,
    paginas_pedidas: RefCell<Vec<u32>>,
    sumas_pedidas: RefCell<Vec<String>>,
}

impl Espia {
    fn nueva(dentro: FuenteFijos) -> Self {
        Self {
            dentro,
            paginas_pedidas: RefCell::default(),
            sumas_pedidas: RefCell::default(),
        }
    }
}

impl Fuente for Espia {
    fn pagina_releases(&self, pagina: u32) -> Resultado<String> {
        self.paginas_pedidas.borrow_mut().push(pagina);
        self.dentro.pagina_releases(pagina)
    }

    fn suma(&self, url: &str) -> Resultado<String> {
        self.sumas_pedidas.borrow_mut().push(url.to_string());
        self.dentro.suma(url)
    }
}

/// `n` releases del escritorio sin activos, de la `v0.0.1` en adelante.
fn rellenar(n: usize, desde: usize) -> Vec<Value> {
    (desde..desde + n)
        .map(|i| {
            json!({"tag_name": format!("v0.0.{i}"), "draft": false, "prerelease": false,
                "published_at": "2026-01-01T00:00:00Z",
                "html_url": format!("https://github.com/HectorEspejo/magi/releases/tag/v0.0.{i}"),
                "assets": []})
        })
        .collect()
}

#[test]
fn descargar_releases_pide_paginas_hasta_una_corta() {
    // Página 1 llena (100), página 2 con las tres reales (<100): se para en la 2.
    let real = pagina_valor();
    let mut f = fuente(&real);
    f.paginas = vec![
        serde_json::to_string(&rellenar(100, 1)).unwrap(),
        serde_json::to_string(&real).unwrap(),
    ];
    let espia = Espia::nueva(f);
    let doc = descargar_releases(&espia, &plataformas()).unwrap();
    assert_eq!(*espia.paginas_pedidas.borrow(), [1, 2]);
    assert_eq!(doc.releases.len(), 103);
    let e = elegir(&doc, &plataformas(), OpcionesDescargas::default()).unwrap();
    assert_eq!(
        e.escritorio.etiqueta, "v1.6.1",
        "la de la página 2 gana por número"
    );

    // Si la primera ya es corta, no se pide la segunda.
    let mut f = fuente(&real);
    f.paginas = vec![
        serde_json::to_string(&real).unwrap(),
        serde_json::to_string(&rellenar(100, 1)).unwrap(),
    ];
    let espia = Espia::nueva(f);
    descargar_releases(&espia, &plataformas()).unwrap();
    assert_eq!(*espia.paginas_pedidas.borrow(), [1]);

    // Una página llena seguida de una vacía: se piden las dos y se termina ahí. La
    // versión mayor de las de relleno (`v0.0.100`) no trae artefactos y falla.
    let mut f = fuente(&real);
    f.paginas = vec![
        serde_json::to_string(&rellenar(100, 1)).unwrap(),
        "[]".into(),
        "no se pide".into(),
    ];
    let espia = Espia::nueva(f);
    let (donde, que) = raro(descargar_releases(&espia, &plataformas()));
    assert_eq!(*espia.paginas_pedidas.borrow(), [1, 2]);
    assert!(donde.contains("v0.0.100"), "{donde}");
    assert!(que.contains("falta el artefacto"), "{que}");
}

#[test]
fn descargar_releases_falla_pasadas_las_paginas_maximas() {
    let mut f = fuente(&pagina_valor());
    let llena = serde_json::to_string(&rellenar(100, 1)).unwrap();
    f.paginas = vec![llena; PAGINAS_MAXIMAS as usize + 5];
    let espia = Espia::nueva(f);
    let (donde, que) = raro(descargar_releases(&espia, &plataformas()));
    assert_eq!(
        espia.paginas_pedidas.borrow().len(),
        PAGINAS_MAXIMAS as usize
    );
    assert_eq!(
        *espia.paginas_pedidas.borrow().last().unwrap(),
        PAGINAS_MAXIMAS
    );
    assert_eq!(donde, "API de GitHub");
    assert!(que.contains(&PAGINAS_MAXIMAS.to_string()), "{que}");
    assert!(
        espia.sumas_pedidas.borrow().is_empty(),
        "no baja nada si falla"
    );
}

#[test]
fn la_api_con_un_objeto_en_vez_de_una_lista_falla_con_su_mensaje() {
    let limite = json!({
        "message": "API rate limit exceeded for usuario. Authenticated requests get a higher rate limit.",
        "documentation_url": "https://docs.github.com/rest/overview/resources-in-the-rest-api#rate-limiting"
    });
    let f = FuenteFijos {
        paginas: vec![limite.to_string()],
        ..FuenteFijos::default()
    };
    let (donde, que) = raro(descargar_releases(&f, &plataformas()));
    assert_eq!(donde, "API de GitHub (página 1)");
    assert!(que.contains("API rate limit exceeded for usuario"), "{que}");
    assert!(que.contains("objeto"), "{que}");

    // También en la segunda página.
    let f = FuenteFijos {
        paginas: vec![
            serde_json::to_string(&rellenar(100, 1)).unwrap(),
            limite.to_string(),
        ],
        ..FuenteFijos::default()
    };
    let (donde, que) = raro(descargar_releases(&f, &plataformas()));
    assert_eq!(donde, "API de GitHub (página 2)");
    assert!(que.contains("API rate limit exceeded"), "{que}");

    // Un mensaje larguísimo y con controles sale recortado y limpio.
    let largo = json!({"message": format!("\u{1b}[31m{}\n", "x".repeat(5000))});
    let f = FuenteFijos {
        paginas: vec![largo.to_string()],
        ..FuenteFijos::default()
    };
    let mensaje = descargar_releases(&f, &plataformas())
        .unwrap_err()
        .to_string();
    assert!(
        !mensaje.contains('\u{1b}') && !mensaje.contains('\n'),
        "{mensaje:?}"
    );
    assert!(mensaje.len() < 600, "{} bytes", mensaje.len());

    // Un objeto sin mensaje, y lo que no es ni lista ni objeto.
    let f = FuenteFijos {
        paginas: vec!["{}".into()],
        ..FuenteFijos::default()
    };
    assert!(raro(descargar_releases(&f, &plataformas()))
        .1
        .contains("sin mensaje"));
    for otra in ["null", "3", "\"texto\"", "true"] {
        let f = FuenteFijos {
            paginas: vec![otra.into()],
            ..FuenteFijos::default()
        };
        let (_, que) = raro(descargar_releases(&f, &plataformas()));
        assert!(que.contains("lista"), "«{otra}»: {que}");
    }
    // Y un JSON roto.
    let f = FuenteFijos {
        paginas: vec!["[{".into()],
        ..FuenteFijos::default()
    };
    assert!(raro(descargar_releases(&f, &plataformas()))
        .1
        .contains("JSON"));
}

#[test]
fn una_release_sin_campos_obligatorios_o_enorme_falla_diciendo_donde() {
    let f = FuenteFijos {
        paginas: vec![r#"[{"tag_name": "v1.6.1"}]"#.into()],
        ..FuenteFijos::default()
    };
    let (donde, que) = raro(descargar_releases(&f, &plataformas()));
    assert_eq!(donde, "API de GitHub (página 1)");
    assert!(
        que.contains("release número 1") && que.contains("draft"),
        "{que}"
    );

    // Una página que pasa del tope.
    let enorme = format!("[{}]", " ".repeat(TOPE_PAGINA_API + 1));
    let f = FuenteFijos {
        paginas: vec![enorme],
        ..FuenteFijos::default()
    };
    assert!(raro(descargar_releases(&f, &plataformas()))
        .1
        .contains("tope"));

    // Una release con 201 activos.
    let mut pagina = pagina_valor();
    let extra: Vec<Value> = (0..190)
        .map(|i| json!({"name": format!("x{i}"), "size": 1, "state": "uploaded", "browser_download_url": "https://x"}))
        .collect();
    pagina[0]["assets"].as_array_mut().unwrap().extend(extra);
    assert!(pagina[0]["assets"].as_array().unwrap().len() > 200);
    let (_, que) = raro(descargar_releases(&fuente(&pagina), &plataformas()));
    assert!(que.contains("v1.6.1") && que.contains("200"), "{que}");
}

#[test]
fn un_artefacto_elegido_sin_su_sha256_en_la_api_falla() {
    let mut pagina = pagina_valor();
    let sha = format!("{LINUX}.sha256");
    quitar_activos(&mut pagina[0], |n| n == sha);
    let (donde, que) = raro(descargar_releases(&fuente(&pagina), &plataformas()));
    assert!(donde.contains("v1.6.1"), "{donde}");
    assert!(que.contains(&sha), "{que}");

    // Y sin el propio artefacto.
    let mut pagina = pagina_valor();
    quitar_activos(&mut pagina[0], |n| n == LINUX);
    let (_, que) = raro(descargar_releases(&fuente(&pagina), &plataformas()));
    assert!(que.contains(LINUX), "{que}");
}

#[test]
fn una_suma_con_otro_nombre_o_demasiado_grande_falla_al_descargar() {
    let pagina = pagina_valor();
    let url =
        format!("https://github.com/HectorEspejo/magi/releases/download/v1.6.1/{LINUX}.sha256");

    let mut f = fuente(&pagina);
    f.sumas.insert(
        url.clone(),
        format!("{}  otro.tar.xz\n", hash_falso("v1.6.1", "otro.tar.xz")),
    );
    let (donde, que) = raro(descargar_releases(&f, &plataformas()));
    assert!(donde.contains(LINUX) && donde.contains("v1.6.1"), "{donde}");
    assert!(que.contains("otro.tar.xz"), "{que}");

    let mut f = fuente(&pagina);
    f.sumas.insert(url.clone(), "a".repeat(TOPE_SHA256 + 1));
    let (_, que) = raro(descargar_releases(&f, &plataformas()));
    assert!(que.contains("tope"), "{que}");

    // Una suma que falta en la fuente.
    let mut f = fuente(&pagina);
    f.sumas.remove(&url);
    let (_, que) = raro(descargar_releases(&f, &plataformas()));
    assert!(que.contains(LINUX), "{que}");
}

#[test]
fn la_url_de_un_sha256_de_otro_sitio_falla_sin_bajar_nada() {
    let mut pagina = pagina_valor();
    let sha = format!("{LINUX}.sha256");
    for a in pagina[0]["assets"].as_array_mut().unwrap() {
        if a["name"] == sha.as_str() {
            a["browser_download_url"] = json!("https://ejemplo.org/malo.sha256");
        }
    }
    let espia = Espia::nueva(fuente(&pagina));
    let (donde, que) = raro(descargar_releases(&espia, &plataformas()));
    assert!(donde.contains("v1.6.1"), "{donde}");
    assert!(
        que.contains("ejemplo.org") && que.contains("esperada"),
        "{que}"
    );
    assert!(
        espia
            .sumas_pedidas
            .borrow()
            .iter()
            .all(|u| u.starts_with("https://github.com/HectorEspejo/magi/releases/download/")),
        "nunca se pide una URL ajena: {:?}",
        espia.sumas_pedidas.borrow()
    );
}

#[test]
fn termux_sin_sha256_en_la_ultima_cae_a_la_anterior_al_descargar() {
    let mut pagina = pagina_valor();
    // Una 1.6.2 publicada sin el .deb todavía; y otra, la 1.6.3, con el .deb pero sin su .sha256.
    let mut v162 = clonar_release(&pagina, "v1.6.1", "v1.6.2");
    quitar_activos(&mut v162, |n| n.contains(".deb"));
    let mut v163 = clonar_release(&pagina, "v1.6.1", "v1.6.3");
    quitar_activos(&mut v163, |n| n == "magi-ssh_1.6.3_aarch64.deb.sha256");
    pagina.insert(0, v162);
    pagina.insert(0, v163);
    let doc = descargar_releases(&fuente(&pagina), &plataformas()).unwrap();
    let e = elegir(&doc, &plataformas(), OpcionesDescargas::default()).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.3");
    assert_eq!(e.termux.unwrap().etiqueta, "v1.6.1");
    assert_eq!(
        e.artefactos
            .iter()
            .find(|a| a.plataforma == "termux-arm64")
            .unwrap()
            .etiqueta,
        "v1.6.1"
    );
    // Se bajaron las sumas de los tarballs de la 1.6.3 y la del .deb de la 1.6.1, no más.
    assert!(doc
        .sumas
        .contains_key("v1.6.3/magi-ssh-aarch64-apple-darwin.tar.xz"));
    assert!(doc.sumas.contains_key("v1.6.1/magi-ssh_1.6.1_aarch64.deb"));
    assert!(!doc.sumas.keys().any(|k| k.starts_with("v1.6.2/")));
    assert_eq!(doc.sumas.len(), 5);
}

#[test]
fn con_app_solo_se_guarda_el_cuerpo_de_la_app_y_se_comprueba_su_huella() {
    let mut pagina = pagina_valor();
    let notas = format!("## Verificación\n\n- Certificado de firma (SHA-256): {HUELLA}\n");
    pagina.insert(
        0,
        json!({
            "tag_name": "android-v0.2.0", "draft": false, "prerelease": false,
            "published_at": "2026-11-30T18:20:11Z",
            "html_url": "https://github.com/HectorEspejo/magi/releases/tag/android-v0.2.0",
            "body": notas, "id": 7, "author": {"login": "github-actions[bot]"},
            "assets": [
                {"name": "magi-android-v0.2.0.apk", "size": 31457280, "state": "uploaded", "label": "",
                 "browser_download_url": "https://github.com/HectorEspejo/magi/releases/download/android-v0.2.0/magi-android-v0.2.0.apk"},
                {"name": "magi-android-v0.2.0.apk.sha256", "size": 90, "state": "uploaded",
                 "browser_download_url": "https://github.com/HectorEspejo/magi/releases/download/android-v0.2.0/magi-android-v0.2.0.apk.sha256"}
            ]
        }),
    );
    // Con las notas de las demás para ver que se descartan.
    pagina[1]["body"] = json!("notas largas del escritorio");
    let f = fuente(&pagina);

    let doc = descargar_releases(&f, &plataformas_con(HUELLA)).unwrap();
    for r in &doc.releases {
        match r.tag_name.as_str() {
            "android-v0.2.0" => assert_eq!(r.body.as_deref(), Some(notas.as_str())),
            otra => assert!(r.body.is_none(), "{otra} no debía conservar sus notas"),
        }
    }
    assert!(doc
        .sumas
        .contains_key("android-v0.2.0/magi-android-v0.2.0.apk"));
    let e = elegir(&doc, &plataformas_con(HUELLA), OpcionesDescargas::default()).unwrap();
    assert_eq!(e.app.unwrap().numero, "0.2.0");

    // Con otra huella en la constante, `magi-web releases` ya falla.
    let otra = HUELLA.replace("11:22", "11:23");
    let (_, que) = raro(descargar_releases(&f, &plataformas_con(&otra)));
    assert!(que.contains(HUELLA) && que.contains(&otra), "{que}");
    // Y sin huella en la constante, también.
    let (_, que) = raro(descargar_releases(&f, &plataformas()));
    assert!(que.contains("falta huella_certificado"), "{que}");
}

#[test]
fn sin_ninguna_release_del_escritorio_falla() {
    let f = FuenteFijos {
        paginas: vec!["[]".into()],
        ..FuenteFijos::default()
    };
    let (_, que) = raro(descargar_releases(&f, &plataformas()));
    assert!(
        que.contains("ninguna versión publicada del escritorio"),
        "{que}"
    );
}

// ---------------------------------------------------------------------------
// FuenteGh con `gh` y `curl` falsos en el PATH
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod con_ordenes_falsas {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::Mutex;

    /// Las pruebas de aquí cambian el PATH del proceso: de una en una.
    static BLOQUEO: Mutex<()> = Mutex::new(());

    /// Pone `carpeta` al principio del PATH y lo deja como estaba al terminar.
    struct PathFalso {
        antes: Option<std::ffi::OsString>,
        _cerrojo: std::sync::MutexGuard<'static, ()>,
    }

    impl PathFalso {
        fn nuevo(carpeta: &Path) -> Self {
            let cerrojo = BLOQUEO.lock().unwrap_or_else(|e| e.into_inner());
            let antes = std::env::var_os("PATH");
            let mut nuevo = std::ffi::OsString::from(carpeta);
            nuevo.push(":");
            nuevo.push(antes.clone().unwrap_or_default());
            std::env::set_var("PATH", nuevo);
            Self {
                antes,
                _cerrojo: cerrojo,
            }
        }
    }

    impl Drop for PathFalso {
        fn drop(&mut self) {
            match &self.antes {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    fn instalar(carpeta: &Path, nombre: &str, cuerpo: &str) {
        let ruta = carpeta.join(nombre);
        std::fs::write(&ruta, format!("#!/bin/sh\n{cuerpo}\n")).unwrap();
        std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn fuente_gh_pide_la_pagina_a_gh_con_el_token_solo_por_el_entorno() {
        let carpeta = tempfile::tempdir().unwrap();
        let d = carpeta.path().display();
        instalar(
            carpeta.path(),
            "gh",
            &format!(
                "printf '%s\\n' \"$@\" > '{d}/gh.args'\nprintf '%s' \"$GH_TOKEN\" > '{d}/gh.token'\nprintf '[]'"
            ),
        );
        let _path = PathFalso::nuevo(carpeta.path());
        std::env::set_var("GH_TOKEN", "token-falso-de-prueba-123");

        let texto = FuenteGh.pagina_releases(2).unwrap();
        assert_eq!(texto, "[]");
        let args = std::fs::read_to_string(carpeta.path().join("gh.args")).unwrap();
        assert_eq!(
            args,
            "api\nrepos/HectorEspejo/magi/releases?per_page=100&page=2\n"
        );
        // El token llegó por el entorno heredado y no por los argumentos.
        let token = std::fs::read_to_string(carpeta.path().join("gh.token")).unwrap();
        assert_eq!(token, "token-falso-de-prueba-123");
        assert!(!args.contains("token-falso"));
        std::env::remove_var("GH_TOKEN");
    }

    #[test]
    fn fuente_gh_falla_con_el_mensaje_de_gh_y_nunca_con_el_token() {
        let carpeta = tempfile::tempdir().unwrap();
        instalar(
            carpeta.path(),
            "gh",
            "echo \"gh: API rate limit exceeded (HTTP 403) $GH_TOKEN\" >&2\necho '{\"message\":\"x\"}'\nexit 1",
        );
        let _path = PathFalso::nuevo(carpeta.path());
        std::env::set_var("GH_TOKEN", "token-falso-de-prueba-123");
        let e = FuenteGh.pagina_releases(1).unwrap_err();
        std::env::remove_var("GH_TOKEN");
        match &e {
            ErrorWeb::Orden { orden, motivo } => {
                assert_eq!(orden, "gh");
                assert!(motivo.contains("código 1"), "{motivo}");
                assert!(motivo.contains("API rate limit exceeded"), "{motivo}");
            }
            otro => panic!("se esperaba Orden: {otro}"),
        }
    }

    #[test]
    fn fuente_gh_rechaza_paginas_fuera_de_rango_sin_lanzar_gh() {
        for pagina in [0, PAGINAS_MAXIMAS + 1] {
            let (_, que) = raro(FuenteGh.pagina_releases(pagina));
            assert!(que.contains("no está entre 1 y"), "{que}");
        }
    }

    #[test]
    fn fuente_gh_baja_el_sha256_con_curl_solo_por_https_y_con_tope() {
        let carpeta = tempfile::tempdir().unwrap();
        let d = carpeta.path().display();
        let hash = hash_falso("v1.6.1", LINUX);
        instalar(
            carpeta.path(),
            "curl",
            &format!(
                "printf '%s\\n' \"$@\" > '{d}/curl.args'\nprintf '%s *%s\\n\\n' '{hash}' '{LINUX}'"
            ),
        );
        let _path = PathFalso::nuevo(carpeta.path());
        let url =
            format!("https://github.com/HectorEspejo/magi/releases/download/v1.6.1/{LINUX}.sha256");
        let texto = FuenteGh.suma(&url).unwrap();
        assert_eq!(texto, format!("{hash} *{LINUX}\n\n"));
        let args: Vec<String> = std::fs::read_to_string(carpeta.path().join("curl.args"))
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(args.first().map(String::as_str), Some("-q"));
        assert_eq!(args.last(), Some(&url));
        let junto = args.join(" ");
        for esperado in [
            "--proto =https",
            "--proto-redir =https",
            "--tlsv1.2",
            "-fsSL",
            "--max-time 30",
            "--max-filesize 4096",
        ] {
            assert!(junto.contains(esperado), "falta «{esperado}» en {junto}");
        }

        // Un `.sha256` más grande que el tope de la web se corta, no se lee entero.
        instalar(
            carpeta.path(),
            "curl",
            "head -c 3000 /dev/zero | tr '\\0' 'a'",
        );
        match FuenteGh.suma(&url) {
            Err(ErrorWeb::Orden { orden, motivo }) => {
                assert_eq!(orden, "curl");
                assert!(motivo.contains("tope"), "{motivo}");
            }
            otro => panic!("se esperaba Orden: {otro:?}"),
        }

        // Un fallo de curl (el 404 con -f) sale como un error de la orden.
        instalar(
            carpeta.path(),
            "curl",
            "echo 'curl: (22) The requested URL returned error: 404' >&2\nexit 22",
        );
        match FuenteGh.suma(&url) {
            Err(ErrorWeb::Orden { orden, motivo }) => {
                assert_eq!(orden, "curl");
                assert!(
                    motivo.contains("código 22") && motivo.contains("404"),
                    "{motivo}"
                );
            }
            otro => panic!("se esperaba Orden: {otro:?}"),
        }
    }

    #[test]
    fn fuente_gh_rechaza_una_url_ajena_antes_de_lanzar_curl() {
        let carpeta = tempfile::tempdir().unwrap();
        let d = carpeta.path().display();
        instalar(carpeta.path(), "curl", &format!("touch '{d}/curl.lanzado'"));
        let _path = PathFalso::nuevo(carpeta.path());
        for mala in [
            "https://ejemplo.org/a.sha256",
            "http://github.com/HectorEspejo/magi/releases/download/v1.6.1/a.sha256",
            "https://github.com/otro/repo/releases/download/v1.6.1/a.sha256",
            "file:///etc/passwd",
            "-o/tmp/x",
        ] {
            assert!(
                matches!(FuenteGh.suma(mala), Err(ErrorWeb::DatoRaro { .. })),
                "«{mala}»"
            );
        }
        assert!(
            !carpeta.path().join("curl.lanzado").exists(),
            "curl no debía lanzarse"
        );
    }
}

#[test]
fn una_url_de_otro_repositorio_no_llega_a_bajarse() {
    // Sin PATH falso: si se llegara a lanzar `curl`, la prueba no dependería de la red
    // porque la URL se rechaza antes.
    for mala in [
        "https://github.com/otro/repo/releases/download/v1.6.1/a.sha256",
        "https://ejemplo.org/HectorEspejo/magi/releases/download/v1.6.1/a.sha256",
    ] {
        let (donde, que) = raro(FuenteGh.suma(mala));
        assert_eq!(donde, "URL de un .sha256");
        assert!(
            que.contains("no empieza por https://github.com/HectorEspejo/magi/releases/download/"),
            "{que}"
        );
    }
}
