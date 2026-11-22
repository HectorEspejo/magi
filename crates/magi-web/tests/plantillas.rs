//! Pruebas de `plantillas`: autoescape, claves que faltan y las plantillas reales.

use std::fs;
use std::path::{Path, PathBuf};

use magi_web::plantillas::Plantillas;
use magi_web::rutas::Idioma;
use magi_web::textos::Textos;
use magi_web::ErrorWeb;
use serde_json::json;

fn web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web")
}

fn con_plantillas(ficheros: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("carpeta temporal");
    for (nombre, contenido) in ficheros {
        let ruta = dir.path().join(nombre);
        fs::create_dir_all(ruta.parent().expect("padre")).expect("carpeta");
        fs::write(ruta, contenido).expect("plantilla");
    }
    dir
}

/// Todos los ficheros de `web/plantillas/` con su ruta y su texto.
fn plantillas_reales() -> Vec<(PathBuf, String)> {
    fn recorrer(dir: &Path, salida: &mut Vec<(PathBuf, String)>) {
        for entrada in fs::read_dir(dir).expect("carpeta de plantillas") {
            let ruta = entrada.expect("entrada").path();
            if ruta.is_dir() {
                recorrer(&ruta, salida);
            } else {
                let texto = fs::read_to_string(&ruta).expect("plantilla en UTF-8");
                salida.push((ruta, texto));
            }
        }
    }
    let mut ficheros = Vec::new();
    recorrer(&web().join("plantillas"), &mut ficheros);
    ficheros.sort();
    assert!(!ficheros.is_empty(), "no hay plantillas en web/plantillas");
    ficheros
}

#[test]
fn carga_recursivamente_y_usa_la_ruta_relativa_como_nombre() {
    let dir = con_plantillas(&[
        ("base.html", "<main>{% block cuerpo %}{% endblock %}</main>"),
        ("parciales/pie.html", "<footer>{{ pie }}</footer>"),
        (
            "pagina.html",
            "{% extends \"base.html\" %}{% block cuerpo %}hola {% include \"parciales/pie.html\" %}{% endblock %}",
        ),
        ("notas.txt", "{% esto no es una plantilla"),
    ]);
    let plantillas = Plantillas::cargar(dir.path()).expect("carga");
    let html = plantillas
        .renderizar("pagina.html", &json!({"pie": "fin"}))
        .expect("render");
    assert_eq!(html, "<main>hola <footer>fin</footer></main>");
    // Los que no son .html no se cargan.
    assert!(plantillas.renderizar("notas.txt", &json!({})).is_err());
}

#[test]
fn todo_lo_que_viene_de_fuera_se_escapa() {
    let dir = con_plantillas(&[("a.html", "<p title=\"{{ x }}\">{{ x }}</p>")]);
    let plantillas = Plantillas::cargar(dir.path()).expect("carga");
    let html = plantillas
        .renderizar(
            "a.html",
            &json!({"x": "<script>alert(\"1\")</script> & 'y'"}),
        )
        .expect("render");
    assert!(!html.contains("<script>"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
    assert!(html.contains("&amp;"), "{html}");
    assert!(
        !html.contains("\"1\""),
        "las comillas de un atributo tienen que escaparse: {html}"
    );
}

#[test]
fn una_clave_que_falta_rompe_en_vez_de_salir_vacia() {
    let dir = con_plantillas(&[
        ("a.html", "{{ falta }}"),
        ("b.html", "{{ a.b.c }}"),
        ("c.html", "{% if falta %}x{% endif %}"),
    ]);
    let plantillas = Plantillas::cargar(dir.path()).expect("carga");
    for nombre in ["a.html", "b.html", "c.html"] {
        let error = plantillas
            .renderizar(nombre, &json!({"a": {}}))
            .unwrap_err();
        assert!(
            matches!(error, ErrorWeb::Plantilla { .. }),
            "{nombre}: {error}"
        );
        assert!(error.to_string().contains(nombre), "{error}");
    }
}

#[test]
fn un_error_de_sintaxis_dice_en_que_plantilla() {
    let dir = con_plantillas(&[("buena.html", "ok"), ("rota.html", "{% if %}\n{{ x ")]);
    let error = match Plantillas::cargar(dir.path()) {
        Err(e) => e,
        Ok(_) => panic!("una plantilla rota tiene que parar la carga"),
    };
    assert!(
        matches!(&error, ErrorWeb::Plantilla { nombre, .. } if nombre == "rota.html"),
        "{error}"
    );
}

#[test]
fn una_plantilla_que_no_existe_o_un_contexto_raro_son_errores() {
    let dir = con_plantillas(&[("a.html", "x")]);
    let plantillas = Plantillas::cargar(dir.path()).expect("carga");
    let sin_plantilla = plantillas
        .renderizar("no_existe.html", &json!({}))
        .unwrap_err();
    assert!(matches!(sin_plantilla, ErrorWeb::Plantilla { .. }));
    for contexto in [json!(null), json!([1, 2]), json!("texto"), json!(3)] {
        let error = plantillas.renderizar("a.html", &contexto).unwrap_err();
        assert!(error.to_string().contains("objeto"), "{error}");
    }
}

#[test]
fn no_hay_cargador_del_sistema_de_ficheros() {
    let fuera = tempfile::tempdir().expect("carpeta temporal");
    fs::write(fuera.path().join("secreto.html"), "SECRETO").expect("secreto");
    let dir = con_plantillas(&[
        (
            "sube.html",
            &format!(
                "{{% include \"../{}/secreto.html\" %}}",
                fuera
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("x")
            ),
        ),
        (
            "absoluta.html",
            &format!(
                "{{% include \"{}\" %}}",
                fuera.path().join("secreto.html").display()
            ),
        ),
        ("sin_cargar.html", "{% include \"secreto.html\" %}"),
    ]);
    let plantillas = Plantillas::cargar(dir.path()).expect("carga");
    for nombre in ["sube.html", "absoluta.html", "sin_cargar.html"] {
        let resultado = plantillas.renderizar(nombre, &json!({}));
        assert!(resultado.is_err(), "{nombre} no debería leer nada de fuera");
    }
}

#[test]
fn una_carpeta_sin_plantillas_o_inexistente_es_un_error() {
    let vacia = tempfile::tempdir().expect("carpeta temporal");
    assert!(Plantillas::cargar(vacia.path()).is_err());
    let error = match Plantillas::cargar(&vacia.path().join("no_existe")) {
        Err(e) => e,
        Ok(_) => panic!("una carpeta inexistente tiene que fallar"),
    };
    assert!(matches!(error, ErrorWeb::Lectura { .. }), "{error}");
}

#[cfg(unix)]
#[test]
fn una_plantilla_que_es_un_enlace_simbolico_se_rechaza() {
    let dir = con_plantillas(&[("a.html", "x")]);
    let fuera = tempfile::tempdir().expect("carpeta temporal");
    fs::write(fuera.path().join("b.html"), "fuera").expect("fichero");
    std::os::unix::fs::symlink(fuera.path().join("b.html"), dir.path().join("b.html"))
        .expect("enlace");
    let error = match Plantillas::cargar(dir.path()) {
        Err(e) => e,
        Ok(_) => panic!("un enlace simbólico tiene que rechazarse"),
    };
    assert!(error.to_string().contains("enlaces simbólicos"), "{error}");
}

#[test]
fn el_filtro_con_codigo_pone_code_y_escapa_todo() {
    let dir = con_plantillas(&[("a.html", "<p>{{ texto|con_codigo }}</p>")]);
    let plantillas = Plantillas::cargar(dir.path()).expect("carga");
    let html = plantillas
        .renderizar(
            "a.html",
            &json!({"texto": "copia `magi` a <tu> PATH y `<b>x</b>`"}),
        )
        .expect("render");
    assert_eq!(
        html,
        "<p>copia <code>magi</code> a &lt;tu&gt; PATH y <code>&lt;b&gt;x&lt;/b&gt;</code></p>"
    );
}

#[test]
fn las_plantillas_reales_cargan_con_las_que_hay() {
    let plantillas = Plantillas::cargar(&web().join("plantillas")).expect("carga");
    // Sin contexto no se puede pintar, pero existen y compilan: el error es de una clave, no de «no existe».
    for nombre in [
        "base.html",
        "descargas.html",
        "404.html",
        "parciales/cabecera.html",
        "parciales/pie.html",
    ] {
        let error = plantillas
            .renderizar(nombre, &json!({}))
            .unwrap_err()
            .to_string();
        assert!(
            !error.contains("template not found") && !error.contains("not found"),
            "{nombre}: {error}"
        );
    }
}

#[test]
fn las_plantillas_no_llevan_style_ni_script() {
    let atributo_style = regex::Regex::new(r"(?i)[\s'\x22]style\s*=").expect("expresión");
    let manejador = regex::Regex::new(r"(?i)[\s'\x22]on[a-z]+\s*=").expect("expresión");
    for (ruta, texto) in plantillas_reales() {
        let donde = ruta.display();
        let minusculas = texto.to_lowercase();
        assert!(!minusculas.contains("<script"), "{donde}: lleva <script>");
        assert!(!minusculas.contains("<style"), "{donde}: lleva <style>");
        assert!(
            !atributo_style.is_match(&texto),
            "{donde}: lleva un atributo style"
        );
        assert!(
            !manejador.is_match(&texto),
            "{donde}: lleva un manejador de eventos (onclick…)"
        );
        assert!(
            !minusculas.contains("javascript:"),
            "{donde}: lleva un enlace javascript:"
        );
        assert!(
            !minusculas.contains("data:"),
            "{donde}: lleva una URL data:"
        );
        assert!(
            !minusculas.contains("<iframe")
                && !minusculas.contains("<object")
                && !minusculas.contains("<embed")
        );
        assert!(
            !minusculas.contains("<form"),
            "{donde}: la web no tiene formularios"
        );
    }
}

#[test]
fn las_plantillas_nunca_marcan_nada_como_seguro() {
    for (ruta, texto) in plantillas_reales() {
        let compacto = texto.replace(' ', "");
        assert!(!compacto.contains("|safe"), "{}: usa |safe", ruta.display());
        assert!(
            !texto.contains("autoescape"),
            "{}: toca el autoescape",
            ruta.display()
        );
    }
}

#[test]
fn las_plantillas_solo_enlazan_a_https_y_a_la_lista_permitida() {
    let direccion = regex::Regex::new(r#"(?:https?:)?//[^\s"'<>{}]+"#).expect("expresión");
    let permitidas = [
        "https://github.com/HectorEspejo/magi",
        "https://github.com/ImranR98/Obtainium",
        "https://docs.github.com/",
        "https://cli.github.com/",
        "https://termux.dev",
        "https://keepachangelog.com",
        "https://semver.org",
        "https://magi.erratia.com/",
    ];
    for (ruta, texto) in plantillas_reales() {
        for coincidencia in direccion.find_iter(&texto) {
            let url = coincidencia.as_str();
            assert!(
                url.starts_with("https://"),
                "{}: «{url}» no va por https://",
                ruta.display()
            );
            assert!(
                permitidas.iter().any(|p| url.starts_with(p)),
                "{}: «{url}» no es de la lista permitida",
                ruta.display()
            );
        }
    }
}

#[test]
fn toda_clave_de_texto_que_usan_las_plantillas_existe_en_los_dos_idiomas() {
    let textos = Textos::cargar(&web().join("textos")).expect("textos");
    let t = regex::Regex::new(r"(?:^|[^A-Za-z0-9_.])t\.([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*)")
        .expect("expresión");
    let t_en =
        regex::Regex::new(r"d\.t_en\.([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*)").expect("expresión");
    let mut comprobadas = 0;
    for (ruta, texto) in plantillas_reales() {
        for captura in t.captures_iter(&texto) {
            let clave = &captura[1];
            for idioma in Idioma::TODOS {
                assert!(
                    textos.texto(idioma, clave).is_some(),
                    "{}: usa t.{clave} y no es un texto de {}",
                    ruta.display(),
                    idioma.codigo()
                );
            }
            comprobadas += 1;
        }
        for captura in t_en.captures_iter(&texto) {
            let clave = &captura[1];
            assert!(
                textos.texto(Idioma::En, clave).is_some(),
                "{}: usa d.t_en.{clave} y no es un texto de en.toml",
                ruta.display()
            );
            comprobadas += 1;
        }
    }
    assert!(
        comprobadas > 30,
        "la expresión no encuentra las claves ({comprobadas})"
    );
}
