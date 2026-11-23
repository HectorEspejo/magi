//! Pruebas de `textos`: las mismas claves en los dos idiomas y ninguna vacía.

use std::fs;
use std::path::{Path, PathBuf};

use magi_web::rutas::Idioma;
use magi_web::textos::Textos;
use magi_web::ErrorWeb;

fn carpeta_textos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/textos")
}

fn con_ficheros(es: &str, en: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("carpeta temporal");
    fs::write(dir.path().join("es.toml"), es).expect("es.toml");
    fs::write(dir.path().join("en.toml"), en).expect("en.toml");
    dir
}

#[test]
fn una_clave_solo_en_un_idioma_falla_diciendo_cual_y_en_que_fichero() {
    // Solo en castellano.
    let dir = con_ficheros(
        "[portada]\ntitulo = \"Hola\"\nsubtitulo = \"Solo aquí\"\n",
        "[portada]\ntitulo = \"Hello\"\n",
    );
    let error = Textos::cargar(dir.path()).unwrap_err().to_string();
    assert!(
        error.contains("portada.subtitulo"),
        "falta la clave: {error}"
    );
    assert!(
        error.contains(&format!("está en {}", dir.path().join("es.toml").display())),
        "no dice dónde está: {error}"
    );
    assert!(
        error.contains(&format!(
            "falta en {}",
            dir.path().join("en.toml").display()
        )),
        "no dice dónde falta: {error}"
    );
    assert!(
        !error.contains("portada.titulo"),
        "no debe quejarse de la que sí está en los dos: {error}"
    );

    // Solo en inglés: el mismo mensaje con los ficheros al revés.
    let dir = con_ficheros(
        "[portada]\ntitulo = \"Hola\"\n",
        "[portada]\ntitulo = \"Hello\"\n[pie]\nextra = \"Only here\"\n",
    );
    let error = Textos::cargar(dir.path()).unwrap_err().to_string();
    assert!(error.contains("pie.extra"), "{error}");
    assert!(
        error.contains(&format!("está en {}", dir.path().join("en.toml").display())),
        "{error}"
    );
    assert!(
        error.contains(&format!(
            "falta en {}",
            dir.path().join("es.toml").display()
        )),
        "{error}"
    );
}

#[test]
fn todas_las_diferencias_salen_en_un_solo_error() {
    let error = Textos::desde_textos(
        "a = \"1\"\nb = \"2\"\n",
        "es.toml",
        "b = \"2\"\nc = \"3\"\n",
        "en.toml",
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("«a» está en es.toml y falta en en.toml"),
        "{error}"
    );
    assert!(
        error.contains("«c» está en en.toml y falta en es.toml"),
        "{error}"
    );
}

#[test]
fn los_textos_reales_tienen_las_mismas_claves() {
    let textos = Textos::cargar(&carpeta_textos()).expect("los textos reales son válidos");
    let es = textos.claves(Idioma::Es);
    let en = textos.claves(Idioma::En);
    assert!(!es.is_empty());
    assert_eq!(es, en, "es.toml y en.toml no tienen las mismas claves");
    for tabla in ["comun", "cabecera", "pie", "descargas", "no_encontrada"] {
        let prefijo = format!("{tabla}.");
        assert!(
            es.iter().any(|clave| clave.starts_with(&prefijo)),
            "falta la tabla «{tabla}»"
        );
    }
    for idioma in Idioma::TODOS {
        for clave in textos.claves(idioma) {
            let texto = textos
                .texto(idioma, &clave)
                .unwrap_or_else(|| panic!("«{clave}» no se puede leer"));
            assert!(!texto.trim().is_empty(), "«{clave}» está vacía");
        }
    }
}

#[test]
fn los_textos_reales_no_llevan_html() {
    let textos = Textos::cargar(&carpeta_textos()).expect("los textos reales son válidos");
    let etiqueta = regex::Regex::new(r"</?[A-Za-z!]").expect("expresión");
    for idioma in Idioma::TODOS {
        for clave in textos.claves(idioma) {
            let texto = textos.texto(idioma, &clave).unwrap_or_default();
            assert!(
                !etiqueta.is_match(texto),
                "«{clave}» ({}) lleva HTML: {texto}",
                idioma.codigo()
            );
        }
    }
}

#[test]
fn el_arbol_es_el_de_las_plantillas() {
    let textos = Textos::cargar(&carpeta_textos()).expect("los textos reales son válidos");
    let arbol = textos.arbol(Idioma::Es);
    assert_eq!(arbol["cabecera"]["inicio"], "Inicio");
    assert_eq!(arbol["comun"]["mes"]["m11"], "nov");
    assert_eq!(textos.arbol(Idioma::En)["cabecera"]["inicio"], "Home");
    assert_eq!(textos.texto(Idioma::En, "comun.mes.m11"), Some("Nov"));
    assert_eq!(
        textos.texto(Idioma::Es, "descargas.estado.en_pruebas"),
        Some("en pruebas")
    );
    assert_eq!(
        textos.texto(Idioma::En, "descargas.estado.en_pruebas"),
        Some("in testing")
    );
}

#[test]
fn una_clave_vacia_o_un_tipo_raro_paran_la_construccion() {
    let vacia = Textos::desde_textos("a = \"\"", "es.toml", "a = \"x\"", "en.toml");
    let mensaje = vacia.unwrap_err().to_string();
    assert!(
        mensaje.contains("es.toml") && mensaje.contains("«a» está vacía"),
        "{mensaje}"
    );

    let solo_espacios = Textos::desde_textos("a = \"x\"", "es.toml", "a = \"  \\n\"", "en.toml");
    assert!(solo_espacios.unwrap_err().to_string().contains("en.toml"));

    let numero = Textos::desde_textos("a = 1", "es.toml", "a = 1", "en.toml");
    assert!(matches!(numero, Err(ErrorWeb::DatoRaro { .. })));

    let lista = Textos::desde_textos("a = [\"x\"]", "es.toml", "a = [\"x\"]", "en.toml");
    assert!(lista.is_err());

    // Una clave que es texto en un idioma y tabla en el otro tampoco coincide.
    let mezcla = Textos::desde_textos("a = \"x\"", "es.toml", "[a]\nb = \"x\"", "en.toml");
    assert!(mezcla.is_err());

    let roto = Textos::desde_textos("a = \"x\"", "es.toml", "a = [", "en.toml");
    assert!(roto.unwrap_err().to_string().contains("en.toml"));
}

#[test]
fn las_claves_no_pueden_llevar_puntos_ni_caracteres_raros() {
    for clave in ["\"a.b\"", "\"a b\"", "\"ñ\"", "\"\""] {
        let texto = format!("{clave} = \"x\"");
        let resultado = Textos::desde_textos(&texto, "es.toml", &texto, "en.toml");
        assert!(resultado.is_err(), "{clave} debería rechazarse");
    }
}

#[test]
fn un_fichero_que_falta_es_un_error_de_lectura() {
    let dir = tempfile::tempdir().expect("carpeta temporal");
    fs::write(dir.path().join("es.toml"), "a = \"x\"").expect("es.toml");
    let error = Textos::cargar(dir.path()).unwrap_err();
    assert!(matches!(error, ErrorWeb::Lectura { .. }), "{error}");
    assert!(error.to_string().contains("en.toml"), "{error}");
}
