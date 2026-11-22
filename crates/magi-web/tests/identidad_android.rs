//! Identidad de la app Android (Fase 9c, D450): el `applicationId` es
//! `io.github.hectorespejo.magi` y, una vez publicada la app, ya no cambia.
//! El namespace y el paquete Kotlin se quedan en `org.cuatrodetres.magi`;
//! `android-release.yml` y `PruebaHumo` esperan el identificador nuevo, y
//! `android.yml` no se lanza por un cambio solo de la web (`crates/magi-web`).
//!
//! Sin dependencias nuevas: se leen los ficheros como texto. Cada comprobación
//! recibe el texto y devuelve `Result<(), String>`: las pruebas de aceptación
//! miran los ficheros reales y las `rechaza_*` demuestran, con copias estropeadas
//! a propósito, que la comprobación de verdad falla.

use std::fs;
use std::path::PathBuf;

const ID_APP: &str = "io.github.hectorespejo.magi";
const NAMESPACE: &str = "org.cuatrodetres.magi";

const GRADLE: &str = "android/app/build.gradle.kts";
const RELEASE_YML: &str = ".github/workflows/android-release.yml";
const ANDROID_YML: &str = ".github/workflows/android.yml";
const PRUEBA_HUMO: &str = "android/app/src/androidTest/kotlin/org/cuatrodetres/magi/PruebaHumo.kt";

fn leer(relativa: &str) -> String {
    let ruta = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relativa);
    fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("no se puede leer {}: {e}", ruta.display()))
}

/// Copia de `texto` con todo `de` sustituido por `a`. Falla si `de` no está:
/// una prueba `rechaza_*` no puede pasar por no haber estropeado nada.
fn mutar(texto: &str, de: &str, a: &str) -> String {
    assert!(
        texto.contains(de),
        "la mutación no encuentra el texto a estropear: {de:?}"
    );
    texto.replace(de, a)
}

// ---------------------------------------------------------------------------
// build.gradle.kts
// ---------------------------------------------------------------------------

/// Valor de la única asignación `clave = "valor"` del fichero (los comentarios
/// no cuentan).
fn asignacion_unica(texto: &str, clave: &str) -> Result<String, String> {
    let mut valores = Vec::new();
    for linea in texto.lines() {
        let t = linea.trim_start();
        if t.starts_with("//") {
            continue;
        }
        let Some(resto) = t.strip_prefix(clave) else {
            continue;
        };
        let Some(resto) = resto.trim_start().strip_prefix('=') else {
            continue;
        };
        let resto = resto.trim();
        if let Some(valor) = resto
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .filter(|v| !v.contains('"'))
        {
            valores.push(valor.to_string());
        }
    }
    match valores.as_slice() {
        [uno] => Ok(uno.clone()),
        otros => Err(format!(
            "se esperaba una sola asignación de `{clave}` y hay {}",
            otros.len()
        )),
    }
}

fn comprobar_gradle(texto: &str) -> Result<(), String> {
    let id = asignacion_unica(texto, "applicationId")?;
    if id != ID_APP {
        return Err(format!("applicationId es «{id}» y debe ser «{ID_APP}»"));
    }
    let namespace = asignacion_unica(texto, "namespace")?;
    if namespace != NAMESPACE {
        return Err(format!(
            "namespace es «{namespace}» y no debe cambiar de «{NAMESPACE}»"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// android-release.yml y PruebaHumo
// ---------------------------------------------------------------------------

fn comprobar_release(texto: &str) -> Result<(), String> {
    let condicion = format!("if [ \"$paquete\" != \"{ID_APP}\" ]; then");
    if !texto.contains(&condicion) {
        return Err(format!(
            "android-release.yml no comprueba que el APK declare el paquete {ID_APP}"
        ));
    }
    if texto.contains(NAMESPACE) {
        return Err(format!(
            "android-release.yml aún nombra {NAMESPACE}: el APK declara el applicationId"
        ));
    }
    Ok(())
}

fn comprobar_humo(texto: &str) -> Result<(), String> {
    let esperada = format!("assertEquals(\"{ID_APP}\", actividad.packageName)");
    if texto.contains(&esperada) {
        Ok(())
    } else {
        Err(format!("PruebaHumo no espera que packageName sea {ID_APP}"))
    }
}

// ---------------------------------------------------------------------------
// android.yml: filtros de rutas
// ---------------------------------------------------------------------------

/// Listas de `paths:` del workflow, en orden de aparición, con sus patrones
/// sin comillas.
fn listas_de_rutas(texto: &str) -> Vec<Vec<String>> {
    let mut listas = Vec::new();
    let mut actual: Option<(usize, Vec<String>)> = None;
    for linea in texto.lines() {
        let t = linea.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let sangria = linea.len() - linea.trim_start_matches(' ').len();
        if let Some((sangria_clave, patrones)) = actual.as_mut() {
            if let Some(patron) = t.strip_prefix("- ") {
                if sangria > *sangria_clave {
                    let patron = patron.trim().trim_matches(|c| c == '\'' || c == '"');
                    patrones.push(patron.to_string());
                    continue;
                }
            }
            if let Some((_, patrones)) = actual.take() {
                listas.push(patrones);
            }
        }
        if t == "paths:" {
            actual = Some((sangria, Vec::new()));
        }
    }
    if let Some((_, patrones)) = actual {
        listas.push(patrones);
    }
    listas
}

fn coincide(patron: &str, fichero: &str) -> bool {
    match patron.strip_suffix("/**") {
        Some(carpeta) => fichero.starts_with(&format!("{carpeta}/")),
        None => fichero == patron,
    }
}

/// Reglas de GitHub para `paths` con `!`: se evalúan en orden y manda la última
/// que coincide (un negativo después de un positivo excluye; un positivo
/// después de un negativo vuelve a incluir).
fn incluye(patrones: &[String], fichero: &str) -> bool {
    let mut dentro = false;
    for patron in patrones {
        match patron.strip_prefix('!') {
            Some(negativo) if coincide(negativo, fichero) => dentro = false,
            Some(_) => {}
            None if coincide(patron, fichero) => dentro = true,
            None => {}
        }
    }
    dentro
}

fn comprobar_filtros(texto: &str) -> Result<(), String> {
    let listas = listas_de_rutas(texto);
    if listas.len() != 2 {
        return Err(format!(
            "se esperaban dos filtros `paths` (pull_request y push) y hay {}",
            listas.len()
        ));
    }
    let del_nucleo_o_la_app = [
        "crates/magi-nucleo/src/lib.rs",
        "crates/magi-movil/src/lib.rs",
        "android/app/build.gradle.kts",
        "Cargo.toml",
        "Cargo.lock",
        ".github/workflows/android.yml",
    ];
    let ajenos = [
        "crates/magi-web/src/lib.rs",
        "crates/magi-web/tests/workflows.rs",
        "web/plantillas/base.html",
        "docs/manual/README.md",
    ];
    for (n, patrones) in listas.iter().enumerate() {
        for fichero in del_nucleo_o_la_app {
            if !incluye(patrones, fichero) {
                return Err(format!(
                    "el filtro {} no lanza android.yml con {fichero}",
                    n + 1
                ));
            }
        }
        for fichero in ajenos {
            if incluye(patrones, fichero) {
                return Err(format!(
                    "el filtro {} lanza android.yml con {fichero}",
                    n + 1
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pruebas
// ---------------------------------------------------------------------------

#[test]
fn el_gradle_real_fija_el_application_id_y_conserva_el_namespace() {
    comprobar_gradle(&leer(GRADLE)).unwrap();
}

#[test]
fn rechaza_el_application_id_antiguo() {
    let texto = mutar(
        &leer(GRADLE),
        &format!("applicationId = \"{ID_APP}\""),
        &format!("applicationId = \"{NAMESPACE}\""),
    );
    let error = comprobar_gradle(&texto).unwrap_err();
    assert!(error.contains("applicationId"), "{error}");
}

#[test]
fn rechaza_un_namespace_cambiado() {
    let texto = mutar(
        &leer(GRADLE),
        &format!("namespace = \"{NAMESPACE}\""),
        &format!("namespace = \"{ID_APP}\""),
    );
    let error = comprobar_gradle(&texto).unwrap_err();
    assert!(error.contains("namespace"), "{error}");
}

#[test]
fn rechaza_un_application_id_duplicado_o_ausente() {
    let doble = format!("{}\napplicationId = \"{NAMESPACE}\"\n", leer(GRADLE));
    assert!(comprobar_gradle(&doble).is_err());
    let sin = mutar(&leer(GRADLE), "applicationId =", "aplicacionId =");
    assert!(comprobar_gradle(&sin).is_err());
}

#[test]
fn el_workflow_de_release_comprueba_el_paquete_nuevo() {
    comprobar_release(&leer(RELEASE_YML)).unwrap();
}

#[test]
fn rechaza_un_release_que_comprueba_el_paquete_antiguo() {
    let texto = mutar(&leer(RELEASE_YML), ID_APP, NAMESPACE);
    assert!(comprobar_release(&texto).is_err());
}

#[test]
fn rechaza_un_release_sin_la_comprobacion_del_paquete() {
    let texto = mutar(&leer(RELEASE_YML), "\"$paquete\" != ", "\"$paquete\" = ");
    assert!(comprobar_release(&texto).is_err());
}

#[test]
fn la_prueba_de_humo_espera_el_application_id_nuevo() {
    comprobar_humo(&leer(PRUEBA_HUMO)).unwrap();
}

#[test]
fn rechaza_una_prueba_de_humo_con_el_paquete_antiguo() {
    let texto = mutar(
        &leer(PRUEBA_HUMO),
        &format!("assertEquals(\"{ID_APP}\""),
        &format!("assertEquals(\"{NAMESPACE}\""),
    );
    assert!(comprobar_humo(&texto).is_err());
}

#[test]
fn android_yml_no_se_lanza_por_un_cambio_solo_de_la_web() {
    comprobar_filtros(&leer(ANDROID_YML)).unwrap();
}

#[test]
fn rechaza_un_android_yml_sin_la_exclusion_de_la_web() {
    let texto = mutar(&leer(ANDROID_YML), "      - '!crates/magi-web/**'\n", "");
    let error = comprobar_filtros(&texto).unwrap_err();
    assert!(error.contains("crates/magi-web"), "{error}");
}

#[test]
fn rechaza_un_android_yml_con_la_exclusion_antes_de_crates() {
    // Un negativo antes del positivo que lo abarca no excluye nada.
    let texto = mutar(
        &leer(ANDROID_YML),
        "      - 'crates/**'\n      - '!crates/magi-web/**'\n",
        "      - '!crates/magi-web/**'\n      - 'crates/**'\n",
    );
    assert!(comprobar_filtros(&texto).is_err());
}

#[test]
fn rechaza_un_android_yml_que_deja_de_lanzarse_con_el_nucleo_o_la_app() {
    // Quitar el filtro de la app (o de Cargo.lock) dejaría pasar sin CI cambios reales.
    let sin_app = mutar(&leer(ANDROID_YML), "      - 'android/**'\n", "");
    assert!(comprobar_filtros(&sin_app).is_err());
    let sin_lock = mutar(&leer(ANDROID_YML), "      - 'Cargo.lock'\n", "");
    assert!(comprobar_filtros(&sin_lock).is_err());
}

#[test]
fn el_evaluador_de_filtros_sigue_el_orden_de_github() {
    let patrones: Vec<String> = [
        "crates/**",
        "!crates/magi-web/**",
        "crates/magi-web/tests/**",
    ]
    .iter()
    .map(|p| p.to_string())
    .collect();
    assert!(incluye(&patrones, "crates/magi-nucleo/src/lib.rs"));
    assert!(!incluye(&patrones, "crates/magi-web/src/lib.rs"));
    // Un positivo posterior a un negativo vuelve a incluir.
    assert!(incluye(&patrones, "crates/magi-web/tests/workflows.rs"));
    assert!(!incluye(&patrones, "web/plantillas/base.html"));
}
