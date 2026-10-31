//! Pruebas del script embebido que instala la clave del móvil (§7.9): se
//! ejecuta `sh` de verdad con un `HOME` temporal.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use magi_nucleo::instalar_clave::{
    comando_instalar_clave, validar_linea_publica, SCRIPT_INSTALAR_CLAVE,
};

/// Línea pública de prueba con la forma de una clave real y el comentario de §7.9.
const CLAVE: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBOIxQMWCSi2FPVThVIUK6KII13wFT6o6PKXbydITjKglprK7ItUisIEv1eFfuUrDwIWbrsacA7IyJbl6/L6WIp4= magi@prueba";

/// Otra clave que no toca el script.
const OTRA_CLAVE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOtraClaveDeEjemploParaLaPrueba vieja@equipo";

fn ejecutar(home: &Path, comando: &str) -> Output {
    Command::new("sh")
        .arg("-c")
        .arg(comando)
        .env("HOME", home)
        .output()
        .expect("no se pudo ejecutar sh")
}

fn ejecutar_con_path(home: &Path, comando: &str, path: &str) -> Output {
    Command::new("sh")
        .arg("-c")
        .arg(comando)
        .env("HOME", home)
        .env("PATH", path)
        .output()
        .expect("no se pudo ejecutar sh")
}

fn stdout(salida: &Output) -> String {
    String::from_utf8_lossy(&salida.stdout).to_string()
}

fn stderr(salida: &Output) -> String {
    String::from_utf8_lossy(&salida.stderr).to_string()
}

fn assert_permisos(ruta: &Path, esperados: u32) {
    let modo = fs::metadata(ruta)
        .unwrap_or_else(|error| panic!("faltan metadatos de {}: {error}", ruta.display()))
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(modo, esperados, "permisos de {}", ruta.display());
}

fn preparar_authorized_keys(home: &Path, contenido: &[u8]) {
    let dir = home.join(".ssh");
    fs::create_dir_all(&dir).expect("no se pudo crear .ssh");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("no se pudo preparar .ssh");
    let fichero = dir.join("authorized_keys");
    fs::write(&fichero, contenido).expect("no se pudo preparar authorized_keys");
    fs::set_permissions(&fichero, fs::Permissions::from_mode(0o600))
        .expect("no se pudo preparar authorized_keys");
}

fn leer_authorized_keys(home: &Path) -> String {
    fs::read_to_string(home.join(".ssh").join("authorized_keys")).expect("falta authorized_keys")
}

fn buscar_en_path(nombre: &str) -> Option<PathBuf> {
    let camino = std::env::var_os("PATH")?;
    std::env::split_paths(&camino)
        .map(|directorio| directorio.join(nombre))
        .find(|candidato| candidato.is_file())
}

/// Directorio con las herramientas que usa el script, pero sin `restorecon`.
fn bin_sin_restorecon(base: &Path) -> PathBuf {
    let bin = base.join("bin");
    fs::create_dir_all(&bin).expect("no se pudo crear el directorio de herramientas");
    for herramienta in ["sh", "mkdir", "chmod", "grep", "cut", "tail"] {
        let origen = buscar_en_path(herramienta)
            .unwrap_or_else(|| panic!("no se encontró «{herramienta}» en PATH"));
        symlink(&origen, bin.join(herramienta)).expect("no se pudo enlazar la herramienta");
    }
    bin
}

fn directorio_de_prueba() -> tempfile::TempDir {
    tempfile::tempdir().expect("no se pudo crear el directorio temporal")
}

#[test]
fn el_script_es_el_embebido_de_la_especificacion() {
    assert!(SCRIPT_INSTALAR_CLAVE.starts_with("#!/bin/sh\n"));
    assert!(SCRIPT_INSTALAR_CLAVE.contains("chmod 700"));
    assert!(SCRIPT_INSTALAR_CLAVE.contains("chmod 600"));
    assert!(SCRIPT_INSTALAR_CLAVE.contains("YA_ESTABA"));
    assert!(SCRIPT_INSTALAR_CLAVE.contains("INSTALADA"));
    assert!(SCRIPT_INSTALAR_CLAVE.contains("restorecon -F \"$dir\" \"$fichero\""));
}

#[test]
fn instala_en_un_home_vacio() {
    let temporal = directorio_de_prueba();
    let home = temporal.path();
    let comando = comando_instalar_clave(CLAVE).expect("la línea de prueba es válida");
    let salida = ejecutar(home, &comando);
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert!(stdout(&salida).contains("INSTALADA"));
    assert_permisos(&home.join(".ssh"), 0o700);
    assert_permisos(&home.join(".ssh").join("authorized_keys"), 0o600);
    assert_eq!(leer_authorized_keys(home), format!("{CLAVE}\n"));
}

#[test]
fn no_duplica_si_la_clave_ya_estaba() {
    let temporal = directorio_de_prueba();
    let home = temporal.path();
    // La misma clave con otro comentario: el script compara tipo y cuerpo.
    let ya = CLAVE.replace("magi@prueba", "otro@equipo");
    preparar_authorized_keys(home, format!("{ya}\n").as_bytes());
    let salida = ejecutar(
        home,
        &comando_instalar_clave(CLAVE).expect("la línea de prueba es válida"),
    );
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert!(stdout(&salida).contains("YA_ESTABA"));
    assert_eq!(leer_authorized_keys(home), format!("{ya}\n"));
}

#[test]
fn arregla_el_salto_de_linea_final() {
    let temporal = directorio_de_prueba();
    let home = temporal.path();
    preparar_authorized_keys(home, OTRA_CLAVE.as_bytes());
    let salida = ejecutar(
        home,
        &comando_instalar_clave(CLAVE).expect("la línea de prueba es válida"),
    );
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert_eq!(
        leer_authorized_keys(home),
        format!("{OTRA_CLAVE}\n{CLAVE}\n")
    );
}

#[test]
fn conserva_las_demas_lineas() {
    let temporal = directorio_de_prueba();
    let home = temporal.path();
    let contenido = format!("# comentario del administrador\n{OTRA_CLAVE}\n");
    preparar_authorized_keys(home, contenido.as_bytes());
    let salida = ejecutar(
        home,
        &comando_instalar_clave(CLAVE).expect("la línea de prueba es válida"),
    );
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert_eq!(leer_authorized_keys(home), format!("{contenido}{CLAVE}\n"));
}

#[test]
fn corrige_los_permisos_de_lo_que_ya_existia() {
    let temporal = directorio_de_prueba();
    let home = temporal.path();
    preparar_authorized_keys(home, format!("{OTRA_CLAVE}\n").as_bytes());
    fs::set_permissions(home.join(".ssh"), fs::Permissions::from_mode(0o755))
        .expect("no se pudo cambiar .ssh");
    fs::set_permissions(
        home.join(".ssh").join("authorized_keys"),
        fs::Permissions::from_mode(0o644),
    )
    .expect("no se pudo cambiar authorized_keys");
    let salida = ejecutar(
        home,
        &comando_instalar_clave(CLAVE).expect("la línea de prueba es válida"),
    );
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert_permisos(&home.join(".ssh"), 0o700);
    assert_permisos(&home.join(".ssh").join("authorized_keys"), 0o600);
}

#[test]
fn funciona_con_espacios_en_el_home() {
    let temporal = directorio_de_prueba();
    let home = temporal.path().join("home con espacios");
    fs::create_dir(&home).expect("no se pudo crear el home con espacios");
    let salida = ejecutar(
        &home,
        &comando_instalar_clave(CLAVE).expect("la línea de prueba es válida"),
    );
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert_eq!(leer_authorized_keys(&home), format!("{CLAVE}\n"));
}

#[test]
fn sin_restorecon_no_falla() {
    let temporal = directorio_de_prueba();
    let home = temporal.path().join("home");
    fs::create_dir(&home).expect("no se pudo crear el home");
    let bin = bin_sin_restorecon(temporal.path());
    let comando = comando_instalar_clave(CLAVE).expect("la línea de prueba es válida");
    let salida = ejecutar_con_path(&home, &comando, &bin.display().to_string());
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert!(stdout(&salida).contains("INSTALADA"));
    assert_eq!(leer_authorized_keys(&home), format!("{CLAVE}\n"));
}

#[test]
fn llama_a_restorecon_si_existe() {
    let temporal = directorio_de_prueba();
    let home = temporal.path().join("home");
    fs::create_dir(&home).expect("no se pudo crear el home");
    let bin = temporal.path().join("bin");
    fs::create_dir(&bin).expect("no se pudo crear el directorio de herramientas");
    let marca = temporal.path().join("restorecon.txt");
    let restorecon = bin.join("restorecon");
    fs::write(
        &restorecon,
        format!("#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\n", marca.display()),
    )
    .expect("no se pudo escribir el restorecon de prueba");
    fs::set_permissions(&restorecon, fs::Permissions::from_mode(0o755))
        .expect("no se pudo hacer ejecutable el restorecon de prueba");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let comando = comando_instalar_clave(CLAVE).expect("la línea de prueba es válida");
    let salida = ejecutar_con_path(&home, &comando, &path);
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    let marca = fs::read_to_string(&marca).expect("no se llamó a restorecon");
    assert!(marca.contains("-F"), "{marca}");
    assert!(marca.contains(".ssh"), "{marca}");
    assert!(marca.contains("authorized_keys"), "{marca}");
}

#[test]
fn un_restorecon_que_falla_no_rompe_la_instalacion() {
    let temporal = directorio_de_prueba();
    let home = temporal.path().join("home");
    fs::create_dir(&home).expect("no se pudo crear el home");
    let bin = temporal.path().join("bin");
    fs::create_dir(&bin).expect("no se pudo crear el directorio de herramientas");
    let restorecon = bin.join("restorecon");
    fs::write(&restorecon, "#!/bin/sh\nexit 42\n").expect("no se pudo escribir restorecon");
    fs::set_permissions(&restorecon, fs::Permissions::from_mode(0o755))
        .expect("no se pudo hacer ejecutable restorecon");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let comando = comando_instalar_clave(CLAVE).expect("la línea de prueba es válida");
    let salida = ejecutar_con_path(&home, &comando, &path);
    assert!(
        salida.status.success(),
        "el script falló: {}",
        stderr(&salida)
    );
    assert_eq!(leer_authorized_keys(&home), format!("{CLAVE}\n"));
}

#[test]
fn valida_la_linea_publica_antes_de_construir_el_comando() {
    assert!(validar_linea_publica(CLAVE).is_ok());
    assert!(comando_instalar_clave(CLAVE).is_ok());
    let comentario_largo = format!("ecdsa-sha2-nistp256 AAAA {}", "a".repeat(65));
    let casos = [
        String::new(),
        "ecdsa-sha2-nistp256".to_string(),
        "ssh-ed25519 AAAA magi@prueba".to_string(),
        "ecdsa-sha2-nistp256 AAAA".to_string(),
        format!("{CLAVE}\n"),
        format!(" {CLAVE}"),
        "ecdsa-sha2-nistp256 AA-A magi@prueba".to_string(),
        "ecdsa-sha2-nistp256 AAAA=== magi@prueba".to_string(),
        "ecdsa-sha2-nistp256  magi@prueba".to_string(),
        "ecdsa-sha2-nistp256 AAAA magi prueba".to_string(),
        "ecdsa-sha2-nistp256 AAAA magi@ñandú".to_string(),
        comentario_largo,
    ];
    for caso in casos {
        assert!(
            validar_linea_publica(&caso).is_err(),
            "debería rechazar «{caso}»"
        );
        assert!(comando_instalar_clave(&caso).is_err());
    }
    let comentario_justo = format!("ecdsa-sha2-nistp256 AAAA {}", "a".repeat(64));
    assert!(validar_linea_publica(&comentario_justo).is_ok());
    let comando = comando_instalar_clave(CLAVE).expect("la línea de prueba es válida");
    assert!(comando.starts_with("sh -c '"));
    assert!(comando.contains(CLAVE));
}
