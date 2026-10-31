//! Recursos de instalación de `assets/`: autocompletado, página man,
//! entrada de escritorio e iconos.
//!
//! El test de frescura compara los ficheros versionados con la salida del
//! propio binario; si falla, ejecuta `scripts/generar-recursos.sh`.

use std::path::PathBuf;
use std::process::Command;

/// Raíz del repositorio.
fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Ejecuta el binario con los argumentos y devuelve la salida completa.
fn ejecutar(argumentos: &[&str]) -> std::process::Output {
    let salida = Command::new(env!("CARGO_BIN_EXE_magi"))
        .args(argumentos)
        .output()
        .expect("ejecutando el binario magi");
    assert!(
        salida.status.success(),
        "«magi {}» falló: {}",
        argumentos.join(" "),
        String::from_utf8_lossy(&salida.stderr)
    );
    salida
}

/// Compara un recurso versionado con lo que imprime el binario.
fn comparar(ruta: &str, argumentos: &[&str]) {
    let esperado = std::fs::read(raiz().join(ruta))
        .unwrap_or_else(|error| panic!("no se puede leer {ruta}: {error}"));
    let salida = ejecutar(argumentos);
    assert_eq!(
        salida.stdout,
        esperado,
        "«{ruta}» no coincide con «magi {}»; ejecuta scripts/generar-recursos.sh",
        argumentos.join(" ")
    );
}

#[test]
fn los_recursos_estan_al_dia() {
    comparar("assets/completions/magi.bash", &["autocompletado", "bash"]);
    comparar("assets/completions/_magi", &["autocompletado", "zsh"]);
    comparar("assets/completions/magi.fish", &["autocompletado", "fish"]);
    comparar("assets/man/magi.1", &["man"]);
}

#[test]
fn los_recursos_son_deterministas() {
    for argumentos in [
        vec!["autocompletado", "bash"],
        vec!["autocompletado", "zsh"],
        vec!["autocompletado", "fish"],
        vec!["man"],
    ] {
        let primera = ejecutar(&argumentos);
        let segunda = ejecutar(&argumentos);
        assert_eq!(
            primera.stdout,
            segunda.stdout,
            "«{}» no es determinista",
            argumentos.join(" ")
        );
    }
}

#[test]
fn la_pagina_man_lleva_fecha_fija_y_esta_en_castellano() {
    let salida = ejecutar(&["man"]);
    let texto = String::from_utf8(salida.stdout).expect("la página man debe ser UTF-8");
    assert!(
        texto.contains(&format!(".TH magi 1 {} ", magi::FECHA_DE_VERSION)),
        "falta la fecha fija de la versión en la cabecera de la página man"
    );
    for seccion in [
        ".SH NOMBRE",
        ".SH SINOPSIS",
        ".SH DESCRIPCIÓN",
        ".SH OPCIONES",
        ".SH SUBCOMANDOS",
        ".SH VERSIÓN",
    ] {
        assert!(texto.contains(seccion), "falta la sección {seccion}");
    }
    assert!(
        !texto.contains("subcommands"),
        "la sinopsis sigue en inglés"
    );
    assert!(!texto.contains("Print help"), "la ayuda sigue en inglés");
}

#[test]
fn version_muestra_version_protocolo_y_commit() {
    let salida = ejecutar(&["--version"]);
    let texto = String::from_utf8(salida.stdout).expect("UTF-8");
    let comienzo = format!(
        "magi {} (protocolo {}, ",
        magi::VERSION_APP,
        magi::protocolo::VERSION_PROTOCOLO
    );
    assert!(
        texto.starts_with(&comienzo),
        "«{texto}» no empieza por «{comienzo}»"
    );
    let texto = texto.trim_end();
    assert!(
        texto.ends_with(')'),
        "falta el cierre del commit en «{texto}»"
    );
    let commit = texto
        .trim_end_matches(')')
        .rsplit(", ")
        .next()
        .unwrap_or_default();
    assert!(!commit.is_empty(), "falta el commit en «{texto}»");
    // `-V` sigue aceptándose: clap imprime la versión corta.
    let corta = ejecutar(&["-V"]);
    assert!(String::from_utf8_lossy(&corta.stdout).contains(magi::VERSION_APP));
    // `-h` sigue aceptándose.
    let ayuda = ejecutar(&["-h"]);
    assert!(String::from_utf8_lossy(&ayuda.stdout).contains("Usage: magi"));
}

#[test]
fn la_entrada_de_escritorio_lleva_las_claves_del_paquete() {
    let texto = std::fs::read_to_string(raiz().join("assets/magi.desktop"))
        .expect("leyendo assets/magi.desktop");
    let mut claves = std::collections::BTreeMap::new();
    for linea in texto.lines() {
        let linea = linea.trim();
        if linea.is_empty() || linea.starts_with('#') || linea.starts_with('[') {
            continue;
        }
        let (clave, valor) = linea
            .split_once('=')
            .unwrap_or_else(|| panic!("línea sin «=» en el .desktop: {linea}"));
        claves.insert(clave.trim().to_string(), valor.trim().to_string());
    }
    assert_eq!(claves.get("Type").map(String::as_str), Some("Application"));
    assert_eq!(claves.get("Name").map(String::as_str), Some("MAGI"));
    assert_eq!(claves.get("Exec").map(String::as_str), Some("magi"));
    assert_eq!(claves.get("Terminal").map(String::as_str), Some("true"));
    assert_eq!(claves.get("Icon").map(String::as_str), Some("magi"));
    assert_eq!(
        claves.get("Categories").map(String::as_str),
        Some("Network;RemoteAccess;ConsoleOnly;")
    );
    assert!(
        texto.lines().any(|linea| linea.trim() == "[Desktop Entry]"),
        "falta el grupo [Desktop Entry]"
    );
}

#[test]
fn los_iconos_son_originales_y_del_tamano_esperado() {
    let svg =
        std::fs::read_to_string(raiz().join("assets/iconos/magi.svg")).expect("leyendo el SVG");
    assert!(svg.contains("<svg"), "el icono no es un SVG");
    assert!(
        svg.contains("#FFA000") && svg.contains("#0A0A0A"),
        "el icono no usa la paleta de MAGI"
    );
    let png = std::fs::read(raiz().join("assets/iconos/magi-256.png")).expect("leyendo el PNG");
    assert!(
        png.starts_with(b"\x89PNG\r\n\x1a\n"),
        "el icono no es un PNG"
    );
    // El IHDR de un PNG lleva ancho y alto en big-endian a partir del byte 16.
    let ancho = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let alto = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    assert_eq!((ancho, alto), (256, 256), "el PNG debe ser de 256×256");
}
