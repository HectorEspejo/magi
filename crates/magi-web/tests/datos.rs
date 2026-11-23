//! Pruebas de `datos::Plataformas`: carga y validación de `plataformas.toml`.

use std::path::PathBuf;

use magi_web::datos::{Canal, EstadoPlataforma, Plataformas};
use magi_web::error::ErrorWeb;

fn ruta_real() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/datos/plataformas.toml")
}

fn texto_real() -> String {
    std::fs::read_to_string(ruta_real()).expect("web/datos/plataformas.toml existe")
}

/// Cambia `de` por `a` en el texto real y comprueba que algo cambió: una prueba
/// que no modifica nada no probaría nada.
fn cambiar(de: &str, a: &str) -> String {
    let texto = texto_real();
    assert!(texto.contains(de), "el TOML real ya no contiene «{de}»");
    texto.replacen(de, a, 1)
}

/// `(donde, que)` del error de datos raros, o falla la prueba.
fn raro(texto: &str) -> (String, String) {
    match Plataformas::desde_texto(texto, "prueba.toml") {
        Err(ErrorWeb::DatoRaro { donde, que }) => (donde, que),
        Err(otro) => panic!("se esperaba DatoRaro y salió: {otro}"),
        Ok(_) => panic!("se esperaba un error y el texto se aceptó"),
    }
}

#[test]
fn un_estado_desconocido_falla_con_el_id_de_la_plataforma() {
    let texto = cambiar(
        "id = \"macos-arm64\"\ncanal = \"escritorio\"\nestado = \"en_pruebas\"",
        "id = \"macos-arm64\"\ncanal = \"escritorio\"\nestado = \"casi_listo\"",
    );
    let (donde, que) = raro(&texto);
    assert_eq!(donde, "prueba.toml (plataforma macos-arm64)");
    assert!(que.contains("estado «casi_listo» desconocido"), "{que}");
    assert!(que.contains("estable, en_pruebas o beta"), "{que}");

    // Y con el texto completo del error, como lo ve quien construye.
    let e = Plataformas::desde_texto(&texto, "web/datos/plataformas.toml").unwrap_err();
    let mensaje = e.to_string();
    assert!(mensaje.contains("macos-arm64"), "{mensaje}");
    assert!(mensaje.contains("casi_listo"), "{mensaje}");
}

#[test]
fn el_plataformas_toml_real_se_carga_y_queda_ordenado() {
    let p = Plataformas::cargar(&ruta_real()).expect("el TOML real es válido");
    let ids: Vec<&str> = p.plataforma.iter().map(|x| x.id.as_str()).collect();
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
    let ordenes: Vec<u32> = p.plataforma.iter().map(|x| x.orden).collect();
    assert!(ordenes.windows(2).all(|w| w[0] < w[1]), "{ordenes:?}");

    assert_eq!(p.app.huella_certificado, "");
    assert_eq!(p.app.obtainium_etiquetas, "android-v");
    assert_eq!(p.app.obtainium_apk, "magi-android-");

    assert_eq!(p.del_canal(Canal::Escritorio).len(), 4);
    assert_eq!(p.del_canal(Canal::Termux).len(), 1);
    assert_eq!(p.del_canal(Canal::App).len(), 1);

    let mac = p.por_id("macos-arm64").unwrap();
    assert_eq!(mac.estado, EstadoPlataforma::EnPruebas);
    assert_eq!(mac.canal, Canal::Escritorio);
    let linux = p.por_id("linux-x86_64").unwrap();
    assert_eq!(linux.estado, EstadoPlataforma::Estable);
    assert_eq!(
        linux.nombre_artefacto("1.6.1"),
        "magi-ssh-x86_64-unknown-linux-gnu.tar.xz"
    );
    assert_eq!(
        p.por_id("termux-arm64").unwrap().nombre_artefacto("1.6.1"),
        "magi-ssh_1.6.1_aarch64.deb"
    );
    assert_eq!(
        p.por_id("android-arm64").unwrap().nombre_artefacto("0.2.0"),
        "magi-android-v0.2.0.apk"
    );
    assert_eq!(
        p.por_id("android-arm64").unwrap().estado,
        EstadoPlataforma::Beta
    );
    assert!(p.por_id("windows-x86_64").is_none());
}

#[test]
fn cargar_lee_un_fichero_y_dice_cual_cuando_falla() {
    let carpeta = tempfile::tempdir().unwrap();
    let ruta = carpeta.path().join("plataformas.toml");

    // Un fichero que no existe: error de lectura con su ruta.
    match Plataformas::cargar(&ruta) {
        Err(ErrorWeb::Lectura { ruta: r, .. }) => assert_eq!(r, ruta),
        otro => panic!("se esperaba Lectura: {otro:?}"),
    }

    // Un fichero con un dato raro: el error nombra el fichero y la plataforma.
    std::fs::write(&ruta, cambiar("estado = \"beta\"", "estado = \"rara\"")).unwrap();
    let mensaje = Plataformas::cargar(&ruta).unwrap_err().to_string();
    assert!(mensaje.contains("plataformas.toml"), "{mensaje}");
    assert!(mensaje.contains("plataforma android-arm64"), "{mensaje}");

    // Y uno bueno.
    std::fs::write(&ruta, texto_real()).unwrap();
    assert_eq!(Plataformas::cargar(&ruta).unwrap().plataforma.len(), 6);
}

#[test]
fn se_ordena_por_orden_aunque_el_fichero_no_lo_este() {
    // La primera pasa a ser la última, y la app, la primera.
    let texto = cambiar("orden = 10", "orden = 70").replacen("orden = 60", "orden = 5", 1);
    let p = Plataformas::desde_texto(&texto, "prueba.toml").unwrap();
    let ids: Vec<&str> = p.plataforma.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids.first(), Some(&"android-arm64"));
    assert_eq!(ids.last(), Some(&"linux-x86_64"));
}

#[test]
fn un_id_desconocido_o_repetido_falla() {
    let (donde, que) = raro(&cambiar("id = \"linux-arm64\"", "id = \"windows-x86_64\""));
    assert_eq!(donde, "prueba.toml (plataforma windows-x86_64)");
    assert!(que.contains("id desconocido"), "{que}");
    assert!(que.contains("linux-x86_64"), "la lista de admitidos: {que}");

    // Repetido: dos con id `linux-x86_64`, con artefacto y orden distintos.
    let texto = cambiar("id = \"linux-arm64\"", "id = \"linux-x86_64\"");
    let (donde, que) = raro(&texto);
    assert_eq!(donde, "prueba.toml (plataforma linux-x86_64)");
    assert!(que.contains("id repetido"), "{que}");
}

#[test]
fn un_canal_desconocido_o_que_no_es_el_de_la_plataforma_falla() {
    let (donde, que) = raro(&cambiar(
        "id = \"termux-arm64\"\ncanal = \"termux\"",
        "id = \"termux-arm64\"\ncanal = \"windows\"",
    ));
    assert_eq!(donde, "prueba.toml (plataforma termux-arm64)");
    assert!(que.contains("canal «windows» desconocido"), "{que}");

    // Un canal que existe, pero no es el de esa plataforma.
    let (donde, que) = raro(&cambiar(
        "id = \"termux-arm64\"\ncanal = \"termux\"",
        "id = \"termux-arm64\"\ncanal = \"escritorio\"",
    ));
    assert_eq!(donde, "prueba.toml (plataforma termux-arm64)");
    assert!(que.contains("canal"), "{que}");
}

#[test]
fn un_artefacto_o_un_orden_repetidos_fallan() {
    let (donde, que) = raro(&cambiar(
        "artefacto = \"magi-ssh-aarch64-unknown-linux-gnu.tar.xz\"",
        "artefacto = \"magi-ssh-x86_64-unknown-linux-gnu.tar.xz\"",
    ));
    assert_eq!(donde, "prueba.toml (plataforma linux-arm64)");
    assert!(
        que.contains("ya es el de la plataforma linux-x86_64"),
        "{que}"
    );

    let (donde, que) = raro(&cambiar("orden = 20", "orden = 10"));
    assert_eq!(donde, "prueba.toml (plataforma linux-arm64)");
    assert!(que.contains("orden 10"), "{que}");
    assert!(que.contains("linux-x86_64"), "{que}");
}

#[test]
fn termux_y_la_app_exigen_la_version_en_el_artefacto() {
    let (donde, que) = raro(&cambiar(
        "artefacto = \"magi-ssh_{version}_aarch64.deb\"",
        "artefacto = \"magi-ssh_aarch64.deb\"",
    ));
    assert_eq!(donde, "prueba.toml (plataforma termux-arm64)");
    assert!(que.contains("{version}"), "{que}");

    let (donde, _) = raro(&cambiar(
        "artefacto = \"magi-android-v{version}.apk\"",
        "artefacto = \"magi-android.apk\"",
    ));
    assert_eq!(donde, "prueba.toml (plataforma android-arm64)");
}

#[test]
fn un_artefacto_que_no_es_un_nombre_de_fichero_falla() {
    for malo in [
        "../magi.tar.xz",
        "carpeta/magi.tar.xz",
        "magi ssh.tar.xz",
        "magi-{otra}.tar.xz",
        "magi-{version.deb",
        "..",
        "-magi.tar.xz",
        ".magi.tar.xz",
        "magi\\\\x.tar.xz",
        "magi;rm.tar.xz",
        "magi$(x).tar.xz",
        "",
    ] {
        let texto = cambiar(
            "artefacto = \"magi-ssh-x86_64-unknown-linux-gnu.tar.xz\"",
            &format!("artefacto = \"{malo}\""),
        );
        let (donde, _) = raro(&texto);
        assert_eq!(
            donde, "prueba.toml (plataforma linux-x86_64)",
            "«{malo}» debía fallar con la plataforma"
        );
    }
}

#[test]
fn un_fichero_enorme_o_un_nombre_larguisimo_fallan_sin_leerlos_enteros() {
    let carpeta = tempfile::tempdir().unwrap();
    let ruta = carpeta.path().join("plataformas.toml");

    // Hasta 1 MiB de comentarios se aceptan; un byte más, no.
    let tope = 1024 * 1024;
    let relleno = |bytes: usize| {
        let mut texto = texto_real();
        let linea = format!("# {}\n", "x".repeat(78));
        while texto.len() + linea.len() <= bytes {
            texto.push_str(&linea);
        }
        while texto.len() < bytes {
            texto.push('#');
            texto.push('\n');
        }
        texto.truncate(bytes);
        texto
    };
    std::fs::write(&ruta, relleno(tope)).unwrap();
    Plataformas::cargar(&ruta).expect("justo en el tope se acepta");
    std::fs::write(&ruta, relleno(tope + 1)).unwrap();
    match Plataformas::cargar(&ruta) {
        Err(ErrorWeb::DatoRaro { donde, que }) => {
            assert!(donde.contains("plataformas.toml"), "{donde}");
            assert!(que.contains("tope"), "{que}");
        }
        otro => panic!("se esperaba DatoRaro: {otro:?}"),
    }

    // Un nombre de artefacto de más de 200 caracteres (con 200 pasa).
    let con_nombre = |largo: usize| {
        let nombre = format!("{}.tar.xz", "a".repeat(largo - ".tar.xz".len()));
        cambiar(
            "artefacto = \"magi-ssh-x86_64-unknown-linux-gnu.tar.xz\"",
            &format!("artefacto = \"{nombre}\""),
        )
    };
    Plataformas::desde_texto(&con_nombre(200), "prueba.toml").expect("200 caracteres se aceptan");
    let (donde, que) = raro(&con_nombre(201));
    assert_eq!(donde, "prueba.toml (plataforma linux-x86_64)");
    assert!(que.contains("200"), "{que}");
}

#[test]
fn las_instrucciones_no_pueden_estar_vacias() {
    let (donde, que) = raro(&cambiar(
        "instrucciones = \"descargas.instalar.app\"",
        "instrucciones = \"  \"",
    ));
    assert_eq!(donde, "prueba.toml (plataforma android-arm64)");
    assert!(que.contains("instrucciones"), "{que}");
}

#[test]
fn un_orden_negativo_falla_con_el_id() {
    let (donde, que) = raro(&cambiar("orden = 60", "orden = -1"));
    assert_eq!(donde, "prueba.toml (plataforma android-arm64)");
    assert!(que.contains("-1"), "{que}");
}

#[test]
fn la_huella_son_32_pares_hexadecimales_en_mayusculas() {
    let buena = "AB:".repeat(31) + "AB";
    let con = |huella: &str| {
        cambiar(
            "huella_certificado = \"\"",
            &format!("huella_certificado = \"{huella}\""),
        )
    };

    let p = Plataformas::desde_texto(&con(&buena), "prueba.toml").unwrap();
    assert_eq!(p.app.huella_certificado, buena);
    // Todos los dígitos hexadecimales en mayúsculas.
    let todos = "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:0F:1E:2D:3C:4B:5A:69:78:87:96:A5:B4:C3:D2:E1:F0";
    Plataformas::desde_texto(&con(todos), "prueba.toml").unwrap();

    for mala in [
        "ab:".repeat(31) + "ab",        // minúsculas
        "AB:".repeat(30) + "AB",        // 31 pares
        "AB:".repeat(32) + "AB",        // 33 pares
        "AB".repeat(32),                // sin «:»
        "AB:".repeat(32),               // «:» de más al final
        format!(" {buena}"),            // espacio delante
        format!("{buena} "),            // espacio detrás
        "AB-".repeat(31) + "AB",        // otro separador
        "GG:".repeat(31) + "GG",        // no hexadecimal
        "A:".repeat(31) + "A",          // pares de una cifra
        "SHA256:".to_string() + &buena, // con prefijo
        "ÁB:".repeat(31) + "ÁB",        // no ASCII
    ] {
        let (donde, que) = raro(&con(&mala));
        assert_eq!(donde, "prueba.toml ([app])", "huella «{mala}»");
        assert!(que.contains("huella_certificado"), "{que}");
    }
}

#[test]
fn un_toml_roto_o_incompleto_o_con_campos_de_mas_falla_sin_panico() {
    // No es TOML.
    let (donde, que) = raro("esto no es toml [[[");
    assert_eq!(donde, "prueba.toml");
    assert!(que.contains("TOML"), "{que}");
    // Sin [app].
    let sin_app = texto_real()
        .split("[[plataforma]]")
        .skip(1)
        .fold(String::new(), |a, p| a + "[[plataforma]]" + p);
    let (_, que) = raro(&sin_app);
    assert!(que.contains("app"), "{que}");
    // Sin plataformas.
    let solo_app = texto_real()
        .split("[[plataforma]]")
        .next()
        .unwrap()
        .to_string();
    let (_, que) = raro(&solo_app);
    assert!(que.contains("plataforma"), "{que}");
    // Un campo desconocido en una plataforma y en [app].
    let (_, que) = raro(&cambiar("orden = 10", "orden = 10\nnota = \"x\""));
    assert!(que.contains("nota"), "{que}");
    let (_, que) = raro(&cambiar("obtainium_apk = ", "otro = 1\nobtainium_apk = "));
    assert!(que.contains("otro"), "{que}");
    // Un tipo equivocado.
    let (_, que) = raro(&cambiar("estado = \"beta\"", "estado = 3"));
    assert!(que.contains("estado") || que.contains("string"), "{que}");
    // Un campo obligatorio que falta.
    let (_, que) = raro(&cambiar("orden = 60\n", ""));
    assert!(que.contains("orden"), "{que}");
    // Vacío.
    let (donde, _) = raro("");
    assert_eq!(donde, "prueba.toml");
}

#[test]
fn los_filtros_de_obtainium_no_pueden_estar_vacios() {
    let (donde, que) = raro(&cambiar(
        "obtainium_etiquetas = \"android-v\"",
        "obtainium_etiquetas = \"\"",
    ));
    assert_eq!(donde, "prueba.toml ([app])");
    assert!(que.contains("obtainium_etiquetas"), "{que}");
    let (_, que) = raro(&cambiar(
        "obtainium_apk = \"magi-android-\"",
        "obtainium_apk = \" \"",
    ));
    assert!(que.contains("obtainium_apk"), "{que}");
}

#[test]
fn los_canales_y_estados_tienen_sus_ids_y_capitulos() {
    assert_eq!(Canal::desde_id("escritorio"), Some(Canal::Escritorio));
    assert_eq!(Canal::desde_id("termux"), Some(Canal::Termux));
    assert_eq!(Canal::desde_id("app"), Some(Canal::App));
    assert_eq!(Canal::desde_id("App"), None);
    assert_eq!(Canal::App.prefijo_etiqueta(), "android-v");
    assert_eq!(Canal::Termux.prefijo_etiqueta(), "v");
    assert_eq!(
        EstadoPlataforma::desde_id("en_pruebas"),
        Some(EstadoPlataforma::EnPruebas)
    );
    assert_eq!(EstadoPlataforma::desde_id("en pruebas"), None);
    for estado in [
        EstadoPlataforma::Estable,
        EstadoPlataforma::EnPruebas,
        EstadoPlataforma::Beta,
    ] {
        assert_eq!(EstadoPlataforma::desde_id(estado.id()), Some(estado));
    }
}
