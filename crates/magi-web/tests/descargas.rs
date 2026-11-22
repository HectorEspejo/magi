//! Pruebas de `descargas`: qué versión y qué artefactos enseña la web.
//!
//! Los datos son los de `tests/fijos/releases/` (todo inventado) y el
//! `plataformas.toml` real del repositorio.

use std::path::PathBuf;

use magi_web::datos::{Canal, Plataformas};
use magi_web::descargas::{
    comprobar_huella, elegir, elegir_versiones, numero_de_etiqueta, validar_suma,
    EleccionDescargas, OpcionesDescargas,
};
use magi_web::error::ErrorWeb;
use magi_web::releases::{DocumentoReleases, ReleaseApi};
use sha2::{Digest, Sha256};

/// La huella de las pruebas (la de `completo.json`; `tests/fijos/releases/LEEME.md`).
const HUELLA: &str = "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00";
const HUELLA_OTRA: &str = "FE:DC:BA:98:76:54:32:10:01:23:45:67:89:AB:CD:EF:FE:DC:BA:98:76:54:32:10:01:23:45:67:89:AB:CD:EF";

const SIN_OPCIONES: OpcionesDescargas = OpcionesDescargas {
    homebrew: false,
    aur: false,
};

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn texto_plataformas() -> String {
    std::fs::read_to_string(raiz().join("../../web/datos/plataformas.toml")).unwrap()
}

/// El `plataformas.toml` real, con la huella vacía como está hoy.
fn plataformas() -> Plataformas {
    Plataformas::desde_texto(&texto_plataformas(), "plataformas.toml").unwrap()
}

/// El `plataformas.toml` real con una huella puesta (la app ya publicada).
fn plataformas_con_huella(huella: &str) -> Plataformas {
    let texto = texto_plataformas();
    assert!(texto.contains("huella_certificado = \"\""));
    let texto = texto.replacen(
        "huella_certificado = \"\"",
        &format!("huella_certificado = \"{huella}\""),
        1,
    );
    Plataformas::desde_texto(&texto, "plataformas.toml").unwrap()
}

fn fijo(nombre: &str) -> DocumentoReleases {
    DocumentoReleases::leer(&raiz().join("tests/fijos/releases").join(nombre)).unwrap()
}

/// El hash falso de los datos de ejemplo: SHA-256 de `fixture-falso:<etiqueta>/<fichero>`.
fn hash_falso(etiqueta: &str, fichero: &str) -> String {
    Sha256::digest(format!("fixture-falso:{etiqueta}/{fichero}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn release_mut<'a>(doc: &'a mut DocumentoReleases, etiqueta: &str) -> &'a mut ReleaseApi {
    doc.releases
        .iter_mut()
        .find(|r| r.tag_name == etiqueta)
        .unwrap_or_else(|| panic!("el dato de ejemplo no tiene la release {etiqueta}"))
}

/// `(donde, que)` de un error de datos raros, o falla la prueba.
fn raro<T: std::fmt::Debug>(r: Result<T, ErrorWeb>) -> (String, String) {
    match r {
        Err(ErrorWeb::DatoRaro { donde, que }) => (donde, que),
        Err(otro) => panic!("se esperaba DatoRaro y salió: {otro}"),
        Ok(v) => panic!("se esperaba un error y salió {v:?}"),
    }
}

/// Copia la release `origen` con otro número de versión (cambia el número en todos
/// sus nombres y URL) y sus sumas, y las mete en el documento.
fn clonar_version(doc: &mut DocumentoReleases, origen: &str, numero: &str, publicada: &str) {
    // `v1.6.1` → `1.6.1` y `android-v0.2.0` → `0.2.0`.
    let (prefijo, numero_origen) = origen.rsplit_once('v').unwrap();
    let nueva = format!("{prefijo}v{numero}");
    let json = serde_json::to_string(doc.releases.iter().find(|r| r.tag_name == origen).unwrap())
        .unwrap()
        .replace(numero_origen, numero);
    let mut release: ReleaseApi = serde_json::from_str(&json).unwrap();
    release.published_at = Some(publicada.to_string());
    doc.releases.push(release);
    let sumas: Vec<(String, String)> = doc
        .sumas
        .iter()
        .filter(|(k, _)| k.starts_with(&format!("{origen}/")))
        .map(|(k, v)| {
            (
                k.replace(numero_origen, numero),
                v.replace(numero_origen, numero),
            )
        })
        .collect();
    doc.sumas.extend(sumas);
    assert!(doc.releases.iter().any(|r| r.tag_name == nueva));
}

fn artefacto<'a>(
    e: &'a EleccionDescargas,
    plataforma: &str,
) -> &'a magi_web::descargas::ArtefactoElegido {
    e.artefactos
        .iter()
        .find(|a| a.plataforma == plataforma)
        .unwrap_or_else(|| panic!("la elección no trae la plataforma {plataforma}"))
}

// ---------------------------------------------------------------------------
// Aceptación
// ---------------------------------------------------------------------------

#[test]
fn descargas_ensena_el_escritorio_1_6_1_y_la_app_0_2_0() {
    // completo.json: v1.6.1, v1.6.0, la candidata v1.6.2-rc.1, el borrador v1.7.0 y android-v0.2.0.
    let doc = fijo("completo.json");
    for etiqueta in ["v1.6.1", "v1.6.2-rc.1", "android-v0.2.0", "v1.7.0"] {
        assert!(
            doc.releases.iter().any(|r| r.tag_name == etiqueta),
            "{etiqueta}"
        );
    }
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap();

    assert_eq!(e.escritorio.canal, Canal::Escritorio);
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert_eq!(e.escritorio.numero, "1.6.1");
    assert_eq!(e.escritorio.fecha, "2026-11-22T11:04:48Z");
    assert_eq!(
        e.escritorio.url,
        "https://github.com/HectorEspejo/magi/releases/tag/v1.6.1"
    );

    let termux = e
        .termux
        .as_ref()
        .expect("Termux sale con el .deb de la 1.6.1");
    assert_eq!(termux.etiqueta, "v1.6.1");
    assert_eq!(termux.canal, Canal::Termux);

    let app = e.app.as_ref().expect("la app 0.2.0");
    assert_eq!(app.etiqueta, "android-v0.2.0");
    assert_eq!(app.numero, "0.2.0");
    assert_eq!(app.canal, Canal::App);
    assert_eq!(app.fecha, "2026-11-30T18:20:11Z");
    assert_eq!(
        app.url,
        "https://github.com/HectorEspejo/magi/releases/tag/android-v0.2.0"
    );

    // Artefactos en el orden de las plataformas, con su tamaño, URL y suma.
    let ids: Vec<&str> = e.artefactos.iter().map(|a| a.plataforma.as_str()).collect();
    assert_eq!(
        ids,
        [
            "linux-x86_64",
            "linux-arm64",
            "macos-arm64",
            "macos-x86_64",
            "termux-arm64",
            "android-arm64"
        ]
    );
    let linux = artefacto(&e, "linux-x86_64");
    assert_eq!(linux.nombre, "magi-ssh-x86_64-unknown-linux-gnu.tar.xz");
    assert_eq!(linux.canal, Canal::Escritorio);
    assert_eq!(linux.tamano, 11_194_224);
    assert_eq!(linux.etiqueta, "v1.6.1");
    assert_eq!(
        linux.url,
        "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/magi-ssh-x86_64-unknown-linux-gnu.tar.xz"
    );
    assert_eq!(
        linux.sha256,
        hash_falso("v1.6.1", "magi-ssh-x86_64-unknown-linux-gnu.tar.xz")
    );
    assert_eq!(linux.sha256.len(), 64);
    assert!(linux
        .sha256
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));

    let deb = artefacto(&e, "termux-arm64");
    assert_eq!(deb.nombre, "magi-ssh_1.6.1_aarch64.deb");
    assert_eq!(
        deb.sha256,
        hash_falso("v1.6.1", "magi-ssh_1.6.1_aarch64.deb")
    );

    let apk = artefacto(&e, "android-arm64");
    assert_eq!(apk.nombre, "magi-android-v0.2.0.apk");
    assert_eq!(apk.etiqueta, "android-v0.2.0");
    assert_eq!(apk.canal, Canal::App);
    assert_eq!(
        apk.sha256,
        hash_falso("android-v0.2.0", "magi-android-v0.2.0.apk")
    );
    assert_eq!(
        apk.url,
        "https://github.com/HectorEspejo/magi/releases/download/android-v0.2.0/magi-android-v0.2.0.apk"
    );
}

#[test]
fn un_sha256_con_otro_nombre_falla() {
    let doc = fijo("sha_otro_nombre.json");
    let (donde, que) = raro(elegir(&doc, &plataformas(), SIN_OPCIONES));
    assert!(
        donde.contains("magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256"),
        "{donde}"
    );
    assert!(donde.contains("v1.6.1"), "{donde}");
    assert!(
        que.contains("magi-ssh-aarch64-unknown-linux-gnu.tar.xz"),
        "{que}"
    );
    assert!(
        que.contains("magi-ssh-x86_64-unknown-linux-gnu.tar.xz"),
        "{que}"
    );

    // El mismo fallo en una suma de Termux o de la app, no solo en la del escritorio.
    let mut doc = fijo("completo.json");
    doc.sumas.insert(
        "v1.6.1/magi-ssh_1.6.1_aarch64.deb".into(),
        format!("{}  otro.deb\n", hash_falso("v1.6.1", "otro.deb")),
    );
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap_err();
    assert!(e.to_string().contains("otro.deb"), "{e}");
    let mut doc = fijo("completo.json");
    doc.sumas.insert(
        "android-v0.2.0/magi-android-v0.2.0.apk".into(),
        format!("{}  otro.apk\n", hash_falso("x", "otro.apk")),
    );
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap_err();
    assert!(e.to_string().contains("otro.apk"), "{e}");
}

#[test]
fn una_huella_distinta_falla_ensenando_las_dos() {
    let doc = fijo("huella_distinta.json");
    let (_, que) = raro(elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES));
    assert!(que.contains(HUELLA_OTRA), "la de la release: {que}");
    assert!(que.contains(HUELLA), "la del repositorio: {que}");
    assert!(que.contains("≠"), "{que}");

    // Con la huella buena, la misma release sale.
    let e = elegir(&doc, &plataformas_con_huella(HUELLA_OTRA), SIN_OPCIONES).unwrap();
    assert_eq!(e.app.unwrap().etiqueta, "android-v0.2.0");

    // La huella de las pruebas no cuela una release que dice otra: la comparación es exacta.
    let mut doc = fijo("completo.json");
    let nueva = HUELLA.replace("11:22", "11:23");
    let release = release_mut(&mut doc, "android-v0.2.0");
    release.body = release.body.as_deref().map(|b| b.replace(HUELLA, &nueva));
    assert!(
        raro(elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES))
            .1
            .contains(&nueva)
    );
}

#[test]
fn termux_cae_a_la_release_anterior_si_la_ultima_no_tiene_deb() {
    let doc = fijo("sin_deb_en_la_ultima.json");
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.2");
    let termux = e.termux.as_ref().expect("Termux cae a la 1.6.1");
    assert_eq!(termux.etiqueta, "v1.6.1");
    assert_eq!(termux.numero, "1.6.1");
    assert_eq!(termux.fecha, "2026-11-22T11:04:48Z");
    assert_eq!(
        termux.url,
        "https://github.com/HectorEspejo/magi/releases/tag/v1.6.1"
    );

    let deb = artefacto(&e, "termux-arm64");
    assert_eq!(deb.nombre, "magi-ssh_1.6.1_aarch64.deb");
    assert_eq!(deb.etiqueta, "v1.6.1");
    assert!(deb.url.contains("/download/v1.6.1/"), "{}", deb.url);
    // Los demás siguen en la 1.6.2.
    let linux = artefacto(&e, "linux-x86_64");
    assert_eq!(linux.etiqueta, "v1.6.2");
    assert!(linux.url.contains("/download/v1.6.2/"), "{}", linux.url);

    // Solo la selección de versiones (sin sumas) dice lo mismo.
    let s = elegir_versiones(&doc, &plataformas()).unwrap();
    assert_eq!(s.escritorio.etiqueta, "v1.6.2");
    assert_eq!(s.termux.unwrap().etiqueta, "v1.6.1");
    assert!(s.app.is_none());
}

#[test]
fn termux_exige_el_deb_y_su_sha256_subidos_y_su_suma() {
    // El .deb de la última está, pero no su .sha256: cae a la anterior.
    let doc = fijo("deb_sin_sha.json");
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert_eq!(e.termux.as_ref().unwrap().etiqueta, "v1.6.0");
    assert_eq!(
        artefacto(&e, "termux-arm64").nombre,
        "magi-ssh_1.6.0_aarch64.deb"
    );

    // El .deb sin subir del todo (estado distinto de uploaded) tampoco vale.
    let mut doc = fijo("completo.json");
    for a in &mut release_mut(&mut doc, "v1.6.1").assets {
        if a.name == "magi-ssh_1.6.1_aarch64.deb" {
            a.state = "starter".into();
        }
    }
    let p = plataformas_con_huella(HUELLA);
    // La 1.6.0 de los datos de ejemplo trae su .deb, pero no su suma: no se enseña.
    assert!(elegir(&doc, &p, SIN_OPCIONES).unwrap().termux.is_none());
    // Con su suma, sí.
    doc.sumas.insert(
        "v1.6.0/magi-ssh_1.6.0_aarch64.deb".into(),
        format!(
            "{}  magi-ssh_1.6.0_aarch64.deb\n",
            hash_falso("v1.6.0", "magi-ssh_1.6.0_aarch64.deb")
        ),
    );
    let e = elegir(&doc, &p, SIN_OPCIONES).unwrap();
    assert_eq!(e.termux.unwrap().etiqueta, "v1.6.0");
}

#[test]
fn termux_sin_suma_bajada_cae_a_la_anterior_y_sin_nada_no_sale() {
    // Con el .deb y su .sha256 en los activos, pero sin la suma en `sumas`: no vale.
    let mut doc = fijo("completo.json");
    doc.sumas.remove("v1.6.1/magi-ssh_1.6.1_aarch64.deb");
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap();
    assert!(e.termux.is_none(), "la 1.6.0 tampoco tiene su suma");
    assert!(e.artefactos.iter().all(|a| a.plataforma != "termux-arm64"));
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert_eq!(e.artefactos.len(), 5, "cuatro de escritorio y la app");

    // Con la suma de la 1.6.0, Termux sale con la 1.6.0.
    doc.sumas.insert(
        "v1.6.0/magi-ssh_1.6.0_aarch64.deb".into(),
        format!(
            "{}  magi-ssh_1.6.0_aarch64.deb\n",
            hash_falso("v1.6.0", "magi-ssh_1.6.0_aarch64.deb")
        ),
    );
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap();
    assert_eq!(e.termux.unwrap().etiqueta, "v1.6.0");

    // Ninguna release con .deb: Termux no sale y no es un error.
    let mut doc = fijo("sin-red.json");
    doc.releases[0].assets.retain(|a| !a.name.contains(".deb"));
    doc.sumas.retain(|k, _| !k.contains(".deb"));
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert!(e.termux.is_none());
    assert_eq!(e.artefactos.len(), 4);
}

#[test]
fn un_suma_presente_pero_mala_de_termux_falla_y_no_cae_a_la_anterior() {
    // Una suma que sí está pero no vale es un dato raro, no una ausencia.
    let mut doc = fijo("completo.json");
    doc.sumas
        .insert("v1.6.1/magi-ssh_1.6.1_aarch64.deb".into(), "basura".into());
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap_err();
    assert!(matches!(e, ErrorWeb::DatoRaro { .. }), "{e}");
}

#[test]
fn homebrew_y_aur_solo_salen_con_su_variable() {
    let doc = fijo("sin-red.json");
    let p = plataformas();
    let e = elegir(&doc, &p, OpcionesDescargas::default()).unwrap();
    assert!(!e.homebrew && !e.aur);
    let e = elegir(
        &doc,
        &p,
        OpcionesDescargas {
            homebrew: true,
            aur: false,
        },
    )
    .unwrap();
    assert!(e.homebrew && !e.aur);
    let e = elegir(
        &doc,
        &p,
        OpcionesDescargas {
            homebrew: false,
            aur: true,
        },
    )
    .unwrap();
    assert!(!e.homebrew && e.aur);
    let e = elegir(
        &doc,
        &p,
        OpcionesDescargas {
            homebrew: true,
            aur: true,
        },
    )
    .unwrap();
    assert!(e.homebrew && e.aur);
}

#[test]
fn la_etiqueta_mayor_por_numero_gana_a_la_mas_reciente_por_fecha() {
    // La 1.6.0 se publicó después que la 1.6.1: sale la 1.6.1.
    let mut doc = fijo("sin-red.json");
    clonar_version(&mut doc, "v1.6.1", "1.6.0", "2027-01-31T23:59:59Z");
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert_eq!(e.escritorio.fecha, "2026-11-22T11:04:48Z");

    // Y se compara por número, no como texto: 1.10.0 > 1.9.0 > 1.6.1 y 1.6.10 > 1.6.9.
    let mut doc = fijo("sin-red.json");
    clonar_version(&mut doc, "v1.6.1", "1.9.0", "2027-03-01T00:00:00Z");
    clonar_version(&mut doc, "v1.6.1", "1.10.0", "2026-12-01T00:00:00Z");
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.10.0");
    assert_eq!(e.escritorio.numero, "1.10.0");
    assert_eq!(e.termux.as_ref().unwrap().etiqueta, "v1.10.0");
    assert_eq!(artefacto(&e, "linux-arm64").etiqueta, "v1.10.0");

    let mut doc = fijo("sin-red.json");
    clonar_version(&mut doc, "v1.6.1", "1.6.9", "2027-03-01T00:00:00Z");
    clonar_version(&mut doc, "v1.6.1", "1.6.10", "2026-12-01T00:00:00Z");
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.10");

    // Lo mismo con la app: la mayor por número.
    let mut doc = fijo("completo.json");
    clonar_version(&mut doc, "android-v0.2.0", "0.10.0", "2026-12-01T00:00:00Z");
    clonar_version(&mut doc, "android-v0.2.0", "0.9.0", "2027-02-01T00:00:00Z");
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap();
    assert_eq!(e.app.unwrap().etiqueta, "android-v0.10.0");
}

#[test]
fn un_borrador_y_una_candidata_no_salen() {
    // En completo.json: `v1.7.0` es un borrador y `v1.6.2-rc.1`, una prerelease.
    let doc = fijo("completo.json");
    assert!(doc
        .releases
        .iter()
        .any(|r| r.tag_name == "v1.7.0" && r.draft));
    assert!(doc
        .releases
        .iter()
        .any(|r| r.tag_name == "v1.6.2-rc.1" && r.prerelease));
    let s = elegir_versiones(&doc, &plataformas_con_huella(HUELLA)).unwrap();
    assert_eq!(s.escritorio.etiqueta, "v1.6.1");

    // Un borrador con una etiqueta limpia y mayor tampoco sale.
    let mut doc = fijo("sin-red.json");
    clonar_version(&mut doc, "v1.6.1", "1.6.2", "2026-12-01T00:00:00Z");
    release_mut(&mut doc, "v1.6.2").draft = true;
    release_mut(&mut doc, "v1.6.2").published_at = None;
    assert_eq!(
        elegir_versiones(&doc, &plataformas())
            .unwrap()
            .escritorio
            .etiqueta,
        "v1.6.1"
    );

    // Ni una prerelease con una etiqueta limpia.
    release_mut(&mut doc, "v1.6.2").draft = false;
    release_mut(&mut doc, "v1.6.2").prerelease = true;
    release_mut(&mut doc, "v1.6.2").published_at = Some("2026-12-01T00:00:00Z".into());
    assert_eq!(
        elegir_versiones(&doc, &plataformas())
            .unwrap()
            .escritorio
            .etiqueta,
        "v1.6.1"
    );

    // Ni una candidata marcada como estable por error: su etiqueta no cumple el patrón.
    let mut doc = fijo("sin-red.json");
    clonar_version(&mut doc, "v1.6.1", "1.6.2-rc.1", "2026-12-01T00:00:00Z");
    assert!(!release_mut(&mut doc, "v1.6.2-rc.1").prerelease);
    assert_eq!(
        elegir_versiones(&doc, &plataformas())
            .unwrap()
            .escritorio
            .etiqueta,
        "v1.6.1"
    );

    // Con la app igual: un borrador y una candidata de la app no salen.
    let mut doc = fijo("completo.json");
    clonar_version(&mut doc, "android-v0.2.0", "0.3.0", "2026-12-01T00:00:00Z");
    release_mut(&mut doc, "android-v0.3.0").draft = true;
    clonar_version(
        &mut doc,
        "android-v0.2.0",
        "0.4.0-rc.1",
        "2026-12-02T00:00:00Z",
    );
    clonar_version(&mut doc, "android-v0.2.0", "0.5.0", "2026-12-03T00:00:00Z");
    release_mut(&mut doc, "android-v0.5.0").prerelease = true;
    let s = elegir_versiones(&doc, &plataformas_con_huella(HUELLA)).unwrap();
    assert_eq!(s.app.unwrap().etiqueta, "android-v0.2.0");

    // Si solo hay borradores y candidatas, no hay versión: falla, no inventa.
    let mut doc = fijo("sin-red.json");
    doc.releases[0].draft = true;
    let (donde, que) = raro(elegir_versiones(&doc, &plataformas()));
    assert_eq!(donde, "releases.json");
    assert!(
        que.contains("ninguna versión publicada del escritorio"),
        "{que}"
    );
}

#[test]
fn sin_app_publicada_no_hay_bloque_de_app() {
    let doc = fijo("sin_app.json");
    assert!(doc
        .releases
        .iter()
        .all(|r| !r.tag_name.starts_with("android-")));

    // Con la huella vacía, como el `plataformas.toml` de hoy: no se comprueba nada.
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert!(e.app.is_none());
    assert!(e.artefactos.iter().all(|a| a.canal != Canal::App));
    assert_eq!(e.artefactos.len(), 5);
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert!(e.termux.is_some());

    // Y con una huella puesta, tampoco.
    let e = elegir(&doc, &plataformas_con_huella(HUELLA), SIN_OPCIONES).unwrap();
    assert!(e.app.is_none());
    assert_eq!(e.artefactos.len(), 5);
}

#[test]
fn con_app_publicada_la_huella_del_repositorio_es_obligatoria() {
    // Con la huella vacía y una release de la app, la construcción falla.
    let doc = fijo("completo.json");
    let (_, que) = raro(elegir(&doc, &plataformas(), SIN_OPCIONES));
    assert!(
        que.contains("falta huella_certificado en web/datos/plataformas.toml"),
        "{que}"
    );
}

#[test]
fn la_app_exige_su_apk_y_su_sha256() {
    let p = plataformas_con_huella(HUELLA);
    // Sin el APK.
    let mut doc = fijo("completo.json");
    release_mut(&mut doc, "android-v0.2.0")
        .assets
        .retain(|a| a.name != "magi-android-v0.2.0.apk");
    let (donde, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
    assert!(donde.contains("android-v0.2.0"), "{donde}");
    assert!(que.contains("magi-android-v0.2.0.apk"), "{que}");
    // Sin su .sha256.
    let mut doc = fijo("completo.json");
    release_mut(&mut doc, "android-v0.2.0")
        .assets
        .retain(|a| a.name != "magi-android-v0.2.0.apk.sha256");
    let (_, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
    assert!(que.contains("magi-android-v0.2.0.apk.sha256"), "{que}");
    // Sin su suma bajada.
    let mut doc = fijo("completo.json");
    doc.sumas.remove("android-v0.2.0/magi-android-v0.2.0.apk");
    let (_, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
    assert!(que.contains("magi-android-v0.2.0.apk"), "{que}");
    // Sin las notas.
    let mut doc = fijo("completo.json");
    release_mut(&mut doc, "android-v0.2.0").body = None;
    let (_, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
    assert!(que.contains("no traen la huella"), "{que}");
}

#[test]
fn una_url_de_otro_repositorio_falla() {
    let p = plataformas();
    let nombre = "magi-ssh-x86_64-unknown-linux-gnu.tar.xz";
    let buenas = format!("https://github.com/HectorEspejo/magi/releases/download/v1.6.1/{nombre}");
    for mala in [
        format!("https://github.com/otro/repo/releases/download/v1.6.1/{nombre}"),
        format!("https://github.com/HectorEspejo/otro/releases/download/v1.6.1/{nombre}"),
        format!("http://github.com/HectorEspejo/magi/releases/download/v1.6.1/{nombre}"),
        format!("https://ejemplo.org/HectorEspejo/magi/releases/download/v1.6.1/{nombre}"),
        format!(
            "https://github.com.ejemplo.org/HectorEspejo/magi/releases/download/v1.6.1/{nombre}"
        ),
        // La de otra versión del mismo repositorio.
        format!("https://github.com/HectorEspejo/magi/releases/download/v1.6.0/{nombre}"),
        // Con una cola o un camino que sale de la carpeta.
        format!("{buenas}?x=1"),
        format!("{buenas}/../{nombre}"),
        format!("{buenas} "),
        format!("https://github.com/HectorEspejo/magi/releases/download/v1.6.1/../v1.6.0/{nombre}"),
        String::new(),
    ] {
        let mut doc = fijo("sin-red.json");
        for a in &mut release_mut(&mut doc, "v1.6.1").assets {
            if a.name == nombre {
                a.browser_download_url = mala.clone();
            }
        }
        let (donde, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
        assert!(donde.contains("v1.6.1"), "{donde}");
        assert!(que.contains("URL"), "«{mala}»: {que}");
        assert!(
            que.contains(&buenas),
            "dice la esperada para «{mala}»: {que}"
        );
    }

    // También la URL del .sha256 y la página de la release.
    let mut doc = fijo("sin-red.json");
    for a in &mut release_mut(&mut doc, "v1.6.1").assets {
        if a.name == format!("{nombre}.sha256") {
            a.browser_download_url =
                "https://github.com/otro/repo/releases/download/v1.6.1/x.sha256".into();
        }
    }
    assert!(raro(elegir(&doc, &p, SIN_OPCIONES)).1.contains("URL"));
    let mut doc = fijo("sin-red.json");
    release_mut(&mut doc, "v1.6.1").html_url =
        "https://github.com/otro/repo/releases/tag/v1.6.1".into();
    let (_, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
    assert!(que.contains("html_url"), "{que}");
}

#[test]
fn los_dos_formatos_de_sha256_son_validos() {
    let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let nombre = "magi-ssh-x86_64-unknown-linux-gnu.tar.xz";

    // dist: `<hash> *<nombre>` seguido de una línea en blanco.
    assert_eq!(
        validar_suma(&format!("{hash} *{nombre}\n\n"), nombre).unwrap(),
        hash
    );
    // sha256sum: `<hash>  <nombre>` y un salto de línea.
    assert_eq!(
        validar_suma(&format!("{hash}  {nombre}\n"), nombre).unwrap(),
        hash
    );
    // Sin salto final, con CRLF, con espacios alrededor.
    assert_eq!(
        validar_suma(&format!("{hash}  {nombre}"), nombre).unwrap(),
        hash
    );
    assert_eq!(
        validar_suma(&format!("{hash}  {nombre}\r\n"), nombre).unwrap(),
        hash
    );
    assert_eq!(
        validar_suma(&format!("  \n{hash} *{nombre}\n \n"), nombre).unwrap(),
        hash
    );
}

#[test]
fn un_sha256_mal_formado_falla() {
    let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let nombre = "magi-ssh_1.6.1_aarch64.deb";
    let malos = [
        // Mayúsculas en el hash.
        format!("{}  {nombre}\n", hash.to_uppercase()),
        // Dos líneas (también con la misma suma repetida).
        format!("{hash}  {nombre}\n{hash}  {nombre}\n"),
        format!("{hash}  {nombre}\n{hash}  otro\n"),
        // Otro nombre.
        format!("{hash}  otro.deb\n"),
        format!("{hash} *{nombre}.sha256\n"),
        format!("{hash}  {}\n", nombre.to_uppercase()),
        // Con ruta o «..».
        format!("{hash}  ./{nombre}\n"),
        format!("{hash}  ../{nombre}\n"),
        format!("{hash}  carpeta/{nombre}\n"),
        format!("{hash}  /{nombre}\n"),
        // Hash demasiado corto o largo, o no hexadecimal.
        format!("{}  {nombre}\n", &hash[..63]),
        format!("{hash}0  {nombre}\n"),
        format!("{}g  {nombre}\n", &hash[..63]),
        // Un solo espacio, tres espacios, tabulador o sin nombre.
        format!("{hash} {nombre}\n"),
        format!("{hash}   {nombre}\n"),
        format!("{hash}\t{nombre}\n"),
        format!("{hash}  \n"),
        hash.to_string(),
        // Vacío, con basura delante o detrás.
        String::new(),
        "   \n\n".to_string(),
        format!("basura {hash}  {nombre}\n"),
        format!("{hash}  {nombre} basura\n"),
        format!("{hash}  {nombre}\0\n"),
        // Una página HTML de error en lugar del .sha256.
        "<html><body>404 Not Found</body></html>".to_string(),
    ];
    for malo in malos {
        let (donde, que) = raro(validar_suma(&malo, nombre));
        assert_eq!(donde, format!("{nombre}.sha256"), "«{malo:?}»");
        assert!(!que.is_empty());
    }

    // El nombre que se espera tampoco puede llevar «/» ni «..», aunque el fichero lo diga igual.
    for nombre_malo in ["../magi.tar.xz", "a/b.tar.xz", "..", "", "a..b"] {
        let texto = format!("{hash}  {nombre_malo}\n");
        assert!(
            validar_suma(&texto, nombre_malo).is_err(),
            "«{nombre_malo}»"
        );
    }
}

#[test]
fn validar_suma_no_entra_en_panico_con_texto_raro() {
    // Caracteres multibyte justo donde el código corta (en 64 y 66) y en todas partes.
    let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let mut casos: Vec<String> = Vec::new();
    for posicion in [0usize, 1, 31, 62, 63, 64, 65, 66, 67] {
        let mut t = format!("{hash}  nombre.tar.xz");
        t.replace_range(posicion..posicion + 1, "é");
        casos.push(t);
        let mut t = format!("{hash}  nombre.tar.xz");
        t.insert(posicion, '😀');
        casos.push(t);
    }
    casos.push("é".repeat(100));
    casos.push("\u{200b}".repeat(70));
    casos.push(format!("{}\u{85}{}", &hash[..30], &hash[30..]));
    for caso in casos {
        let _ = validar_suma(&caso, "nombre.tar.xz");
        let _ = validar_suma(&caso, &caso);
    }
}

#[test]
fn comprobar_huella_compara_la_linea_de_las_notas_con_la_constante() {
    let notas = format!("## Verificación\n\n- APK: x.apk\n- Certificado de firma (SHA-256): {HUELLA}\n\nMás texto\n");
    comprobar_huella(&notas, HUELLA).unwrap();
    // Con CRLF, como las notas editadas en la web de GitHub.
    comprobar_huella(&notas.replace('\n', "\r\n"), HUELLA).unwrap();
    // Sin salto de línea final.
    comprobar_huella(notas.trim_end(), HUELLA).unwrap();

    // Constante vacía.
    let (_, que) = raro(comprobar_huella(&notas, ""));
    assert_eq!(
        que,
        "falta huella_certificado en web/datos/plataformas.toml"
    );
    // Notas sin huella.
    let (_, que) = raro(comprobar_huella("## Notas\n\nnada\n", HUELLA));
    assert!(
        que.starts_with("las notas de la release de la app no traen la huella"),
        "{que}"
    );
    let (_, que) = raro(comprobar_huella("", HUELLA));
    assert!(que.contains("no traen la huella"), "{que}");
    // Distinta: enseña las dos.
    let (_, que) = raro(comprobar_huella(&notas, HUELLA_OTRA));
    assert_eq!(
        que,
        format!("huella de la release {HUELLA} ≠ la del repositorio {HUELLA_OTRA}")
    );
}

#[test]
fn comprobar_huella_solo_acepta_la_linea_exacta() {
    let con = |linea: &str| format!("intro\n{linea}\nfin\n");
    for mala in [
        // Minúsculas, otra marca, sangrado, texto detrás, 31 pares, sin guion inicial.
        format!(
            "- Certificado de firma (SHA-256): {}",
            HUELLA.to_lowercase()
        ),
        format!("- Certificado de firma (SHA-1): {HUELLA}"),
        format!("  - Certificado de firma (SHA-256): {HUELLA}"),
        format!("- Certificado de firma (SHA-256): {HUELLA} "),
        format!("- Certificado de firma (SHA-256): {HUELLA}:00"),
        format!("- Certificado de firma (SHA-256): {}", &HUELLA[..92]),
        format!("Certificado de firma (SHA-256): {HUELLA}"),
        format!("* Certificado de firma (SHA-256): {HUELLA}"),
        // La huella en medio de una línea no cuenta.
        format!("el certificado - Certificado de firma (SHA-256): {HUELLA}"),
    ] {
        let (_, que) = raro(comprobar_huella(&con(&mala), HUELLA));
        assert!(que.contains("no traen la huella"), "«{mala}»: {que}");
    }

    // Dos líneas con huellas distintas: se rechaza (cambió el keystore), aunque una coincida.
    let dos = format!(
        "- Certificado de firma (SHA-256): {HUELLA}\n- Certificado de firma (SHA-256): {HUELLA_OTRA}\n"
    );
    let (_, que) = raro(comprobar_huella(&dos, HUELLA));
    assert!(que.contains(HUELLA_OTRA) && que.contains('≠'), "{que}");
    // Dos líneas iguales no son un problema.
    let igual = format!(
        "- Certificado de firma (SHA-256): {HUELLA}\n- Certificado de firma (SHA-256): {HUELLA}\n"
    );
    comprobar_huella(&igual, HUELLA).unwrap();
}

// ---------------------------------------------------------------------------
// Etiquetas y selección
// ---------------------------------------------------------------------------

#[test]
fn las_etiquetas_se_leen_con_su_patron_exacto() {
    use Canal::{App, Escritorio, Termux};
    assert_eq!(numero_de_etiqueta("v1.6.1", Escritorio), Some((1, 6, 1)));
    assert_eq!(numero_de_etiqueta("v1.6.1", Termux), Some((1, 6, 1)));
    assert_eq!(numero_de_etiqueta("v0.0.0", Escritorio), Some((0, 0, 0)));
    assert_eq!(
        numero_de_etiqueta("v10.20.3000", Escritorio),
        Some((10, 20, 3000))
    );
    assert_eq!(
        numero_de_etiqueta("v9999.9999.9999", Escritorio),
        Some((9999, 9999, 9999))
    );
    assert_eq!(numero_de_etiqueta("android-v0.2.0", App), Some((0, 2, 0)));

    // Cada canal, con su prefijo.
    assert_eq!(numero_de_etiqueta("android-v0.2.0", Escritorio), None);
    assert_eq!(numero_de_etiqueta("android-v0.2.0", Termux), None);
    assert_eq!(numero_de_etiqueta("v1.6.1", App), None);

    for mala in [
        "",
        "v",
        "v1",
        "v1.6",
        "v1.6.",
        "v1.6.1.",
        "v1.6.1.0",
        "v.6.1",
        "v1..1",
        "1.6.1",
        "V1.6.1",
        "v01.6.1",
        "v1.06.1",
        "v1.6.01",
        "v1.6.00",
        "v00.0.0",
        "v1.6.1-rc.1",
        "v1.6.1+build",
        "v1.6.1 ",
        " v1.6.1",
        "v1.6.1\n",
        "v1.6.1\0",
        "v10000.0.0",
        "v0.10000.0",
        "v0.0.10000",
        "v-1.6.1",
        "v+1.6.1",
        "v1.6.x",
        "v１.６.１",
        "vv1.6.1",
        "v1_6_1",
        "v1,6,1",
        "ver1.6.1",
        "release-v1.6.1",
        "v99999999999999999999.0.0",
        "v1.6.1/../x",
    ] {
        assert_eq!(numero_de_etiqueta(mala, Escritorio), None, "«{mala:?}»");
    }
    for mala in [
        "android-v0.2",
        "android-v0.2.0-rc.1",
        "android-v00.2.0",
        "Android-v0.2.0",
        "android-0.2.0",
        "android-v",
    ] {
        assert_eq!(numero_de_etiqueta(mala, App), None, "«{mala:?}»");
    }
}

#[test]
fn las_etiquetas_raras_no_salen_ni_rompen_la_eleccion() {
    // etiqueta_rara.json: v1.6.3-rc.1 (sin marcar como candidata), v01.6.2, v1.7, v2.0.0.0,
    // v10000.0.0, V1.6.8, 1.6.9 y android-v0.3, además de la v1.6.1.
    let doc = fijo("etiqueta_rara.json");
    assert!(doc.releases.len() >= 9);
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert!(e.app.is_none(), "android-v0.3 no cumple el patrón");

    // Si solo hay etiquetas raras, no hay versión.
    let mut doc = fijo("etiqueta_rara.json");
    doc.releases.retain(|r| r.tag_name != "v1.6.1");
    let (_, que) = raro(elegir_versiones(&doc, &plataformas()));
    assert!(
        que.contains("ninguna versión publicada del escritorio"),
        "{que}"
    );

    // Un documento vacío tampoco.
    let (_, que) = raro(elegir_versiones(
        &DocumentoReleases::default(),
        &plataformas(),
    ));
    assert!(
        que.contains("ninguna versión publicada del escritorio"),
        "{que}"
    );
}

#[test]
fn una_etiqueta_repetida_entre_las_publicadas_falla() {
    let mut doc = fijo("sin-red.json");
    let copia = doc.releases[0].clone();
    doc.releases.push(copia);
    let (_, que) = raro(elegir_versiones(&doc, &plataformas()));
    assert!(
        que.contains("v1.6.1") && que.contains("más de una vez"),
        "{que}"
    );
}

// ---------------------------------------------------------------------------
// Datos raros de una release elegida
// ---------------------------------------------------------------------------

#[test]
fn un_artefacto_raro_en_la_release_elegida_para_la_construccion() {
    let p = plataformas();
    let nombre = "magi-ssh-aarch64-apple-darwin.tar.xz";
    let tocar = |f: &dyn Fn(&mut DocumentoReleases)| {
        let mut doc = fijo("sin-red.json");
        f(&mut doc);
        doc
    };

    // Falta el artefacto, o su .sha256, o su suma.
    let doc = tocar(&|d| release_mut(d, "v1.6.1").assets.retain(|a| a.name != nombre));
    let (donde, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
    assert!(
        donde.contains("v1.6.1") && que.contains(nombre),
        "{donde}: {que}"
    );
    let doc = tocar(&|d| {
        release_mut(d, "v1.6.1")
            .assets
            .retain(|a| a.name != format!("{nombre}.sha256"))
    });
    assert!(raro(elegir(&doc, &p, SIN_OPCIONES))
        .1
        .contains(&format!("{nombre}.sha256")));
    let doc = tocar(&|d| {
        d.sumas.remove(&format!("v1.6.1/{nombre}"));
    });
    assert!(raro(elegir(&doc, &p, SIN_OPCIONES))
        .1
        .contains("falta la suma"));

    // Tamaño 0, sin subir del todo, nombre repetido.
    let doc = tocar(&|d| {
        for a in &mut release_mut(d, "v1.6.1").assets {
            if a.name == nombre {
                a.size = 0;
            }
        }
    });
    assert!(raro(elegir(&doc, &p, SIN_OPCIONES)).1.contains("0 bytes"));
    let doc = tocar(&|d| {
        for a in &mut release_mut(d, "v1.6.1").assets {
            if a.name == nombre {
                a.state = "starter".into();
            }
        }
    });
    let (_, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
    assert!(que.contains("starter") && que.contains("uploaded"), "{que}");
    let doc = tocar(&|d| {
        let repetido = release_mut(d, "v1.6.1")
            .assets
            .iter()
            .find(|a| a.name == nombre)
            .unwrap()
            .clone();
        release_mut(d, "v1.6.1").assets.push(repetido);
    });
    assert!(raro(elegir(&doc, &p, SIN_OPCIONES))
        .1
        .contains("más de una vez"));

    // El .sha256 pesa demasiado, o su suma es enorme.
    let doc = tocar(&|d| {
        for a in &mut release_mut(d, "v1.6.1").assets {
            if a.name == format!("{nombre}.sha256") {
                a.size = 5_000_000;
            }
        }
    });
    assert!(raro(elegir(&doc, &p, SIN_OPCIONES)).1.contains("tope"));

    // Sin fecha o con una fecha que no es la de la API.
    let doc = tocar(&|d| release_mut(d, "v1.6.1").published_at = None);
    assert!(raro(elegir(&doc, &p, SIN_OPCIONES))
        .1
        .contains("published_at"));
    for mala in [
        "ayer",
        "2026-11-22",
        "2026-11-22 11:04:48",
        "2026-11-22T11:04:48+01:00",
        "2026-11-22T11:04:48Z\n",
        "",
    ] {
        let doc = tocar(&|d| release_mut(d, "v1.6.1").published_at = Some(mala.to_string()));
        let (_, que) = raro(elegir(&doc, &p, SIN_OPCIONES));
        assert!(que.contains("published_at"), "«{mala}»: {que}");
    }
}

#[test]
fn lo_que_llega_de_fuera_no_se_cuela_en_los_mensajes_con_caracteres_de_control() {
    let mut doc = fijo("sin-red.json");
    let nombre = "magi-ssh-aarch64-apple-darwin.tar.xz";
    for a in &mut release_mut(&mut doc, "v1.6.1").assets {
        if a.name == nombre {
            a.browser_download_url =
                "https://ejemplo.org/\u{1b}[2J\u{1b}[31mpeligro\n\r\u{7}".into();
        }
    }
    let mensaje = elegir(&doc, &plataformas(), SIN_OPCIONES)
        .unwrap_err()
        .to_string();
    assert!(
        !mensaje.contains('\u{1b}') && !mensaje.contains('\u{7}') && !mensaje.contains('\r'),
        "{mensaje:?}"
    );
    assert!(!mensaje.contains("peligro\n"), "{mensaje:?}");

    // Una suma con una URL enorme y con controles queda recortada.
    let mut doc = fijo("sin-red.json");
    doc.sumas.insert(
        format!("v1.6.1/{nombre}"),
        format!("{}  {}\u{1b}[0m\n", "a".repeat(64), "x".repeat(900)),
    );
    let mensaje = elegir(&doc, &plataformas(), SIN_OPCIONES)
        .unwrap_err()
        .to_string();
    assert!(!mensaje.contains('\u{1b}'), "{mensaje:?}");
    assert!(
        mensaje.len() < 1000,
        "el mensaje debe ir acotado: {} bytes",
        mensaje.len()
    );
}

#[test]
fn el_documento_de_sin_red_pasa_con_el_plataformas_toml_real() {
    // Lo usa `scripts/generar-web.sh --sin-red`: solo el escritorio, con Termux y sin app,
    // y con la huella vacía del `plataformas.toml` de hoy.
    let doc = fijo("sin-red.json");
    let e = elegir(&doc, &plataformas(), SIN_OPCIONES).unwrap();
    assert_eq!(e.escritorio.etiqueta, "v1.6.1");
    assert_eq!(e.termux.as_ref().unwrap().etiqueta, "v1.6.1");
    assert!(e.app.is_none());
    assert_eq!(e.artefactos.len(), 5);
}

#[test]
fn todos_los_datos_de_ejemplo_se_leen() {
    let carpeta = raiz().join("tests/fijos/releases");
    let mut vistos = 0;
    for entrada in std::fs::read_dir(carpeta).unwrap() {
        let ruta = entrada.unwrap().path();
        if ruta.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        // La página de la API tiene otro formato: es una lista, no un documento.
        if ruta.file_name().unwrap() == "api_pagina_1.json" {
            continue;
        }
        DocumentoReleases::leer(&ruta).unwrap_or_else(|e| panic!("{}: {e}", ruta.display()));
        vistos += 1;
    }
    assert!(
        vistos >= 8,
        "se esperaban al menos 8 datos de ejemplo y hay {vistos}"
    );
}
