use std::io::Write;
use std::process::{Command, Stdio};

use zeroize::Zeroizing;

/// Servicio con el que MAGI firma sus entradas en el llavero del sistema.
pub const SERVICIO: &str = "magi";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Herramienta {
    SecretTool,
    Security,
}

fn candidatas() -> &'static [Herramienta] {
    if cfg!(target_os = "macos") {
        &[Herramienta::Security, Herramienta::SecretTool]
    } else {
        &[Herramienta::SecretTool]
    }
}

/// Guarda la contraseña de un host en el llavero del sistema (libsecret en
/// Linux, Keychain en macOS). Nunca toca `magi.db`.
pub fn guardar(host: &str, usuario: &str, contrasena: &str) -> Result<(), String> {
    let mut fallos = Vec::new();
    for herramienta in candidatas() {
        match intentar_guardar(*herramienta, host, usuario, contrasena) {
            Ok(()) => return Ok(()),
            Err(Fallo::NoExiste) => continue,
            Err(Fallo::Fallo(motivo)) => fallos.push(format!("{}: {motivo}", nombre(*herramienta))),
        }
    }
    Err(error_global(&fallos))
}

/// Recupera la contraseña guardada de un host, si existe.
pub fn recuperar(host: &str, usuario: &str) -> Result<Option<Zeroizing<String>>, String> {
    let mut fallos = Vec::new();
    for herramienta in candidatas() {
        match intentar_recuperar(*herramienta, host, usuario) {
            Ok(contrasena) => return Ok(contrasena),
            Err(Fallo::NoExiste) => continue,
            Err(Fallo::Fallo(motivo)) => fallos.push(format!("{}: {motivo}", nombre(*herramienta))),
        }
    }
    Err(error_global(&fallos))
}

/// Borra la contraseña guardada de un host. Devuelve si existía.
pub fn olvidar(host: &str, usuario: &str) -> Result<bool, String> {
    let mut fallos = Vec::new();
    for herramienta in candidatas() {
        match intentar_olvidar(*herramienta, host, usuario) {
            Ok(existia) => return Ok(existia),
            Err(Fallo::NoExiste) => continue,
            Err(Fallo::Fallo(motivo)) => fallos.push(format!("{}: {motivo}", nombre(*herramienta))),
        }
    }
    Err(error_global(&fallos))
}

/// ¿Hay algún llavero disponible en este sistema?
pub fn disponible() -> bool {
    candidatas().iter().any(|herramienta| {
        Command::new(nombre(*herramienta))
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    })
}

fn nombre(herramienta: Herramienta) -> &'static str {
    match herramienta {
        Herramienta::SecretTool => "secret-tool",
        Herramienta::Security => "security",
    }
}

enum Fallo {
    NoExiste,
    Fallo(String),
}

fn error_global(fallos: &[String]) -> String {
    if fallos.is_empty() {
        "no se encontró un llavero del sistema (secret-tool o security)".to_string()
    } else {
        fallos.join(" · ")
    }
}

fn etiqueta(host: &str) -> String {
    format!("MAGI · {host}")
}

fn servicio(host: &str) -> String {
    format!("{SERVICIO}:{host}")
}

fn intentar_guardar(
    herramienta: Herramienta,
    host: &str,
    usuario: &str,
    contrasena: &str,
) -> Result<(), Fallo> {
    let mut proceso = match herramienta {
        Herramienta::SecretTool => {
            let etiqueta = etiqueta(host);
            lanzar(
                "secret-tool",
                &[
                    "store",
                    "--label",
                    &etiqueta,
                    "magi-host",
                    host,
                    "magi-user",
                    usuario,
                ],
                true,
            )?
        }
        Herramienta::Security => lanzar(
            "security",
            &[
                "add-generic-password",
                "-U",
                "-a",
                usuario,
                "-s",
                &servicio(host),
                "-w",
                contrasena,
            ],
            false,
        )?,
    };
    if let Some(mut entrada) = proceso.stdin.take() {
        if let Err(error) = entrada.write_all(contrasena.as_bytes()) {
            return Err(Fallo::Fallo(error.to_string()));
        }
    }
    comprobar(proceso)
}

fn intentar_recuperar(
    herramienta: Herramienta,
    host: &str,
    usuario: &str,
) -> Result<Option<Zeroizing<String>>, Fallo> {
    let args: Vec<String> = match herramienta {
        Herramienta::SecretTool => vec![
            "lookup".to_string(),
            "magi-host".to_string(),
            host.to_string(),
            "magi-user".to_string(),
            usuario.to_string(),
        ],
        Herramienta::Security => vec![
            "find-generic-password".to_string(),
            "-a".to_string(),
            usuario.to_string(),
            "-s".to_string(),
            servicio(host),
            "-w".to_string(),
        ],
    };
    let referencias: Vec<&str> = args.iter().map(String::as_str).collect();
    let proceso = lanzar(nombre(herramienta), &referencias, false)?;
    let salida = match proceso.wait_with_output() {
        Ok(salida) => salida,
        Err(error) => return Err(Fallo::Fallo(error.to_string())),
    };
    if !salida.status.success() {
        return Ok(None);
    }
    let texto = String::from_utf8_lossy(&salida.stdout)
        .trim_end_matches(['\n', '\r'])
        .to_string();
    if texto.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Zeroizing::new(texto)))
    }
}

fn intentar_olvidar(herramienta: Herramienta, host: &str, usuario: &str) -> Result<bool, Fallo> {
    let args: Vec<String> = match herramienta {
        Herramienta::SecretTool => vec![
            "clear".to_string(),
            "magi-host".to_string(),
            host.to_string(),
            "magi-user".to_string(),
            usuario.to_string(),
        ],
        Herramienta::Security => vec![
            "delete-generic-password".to_string(),
            "-a".to_string(),
            usuario.to_string(),
            "-s".to_string(),
            servicio(host),
        ],
    };
    let referencias: Vec<&str> = args.iter().map(String::as_str).collect();
    let proceso = lanzar(nombre(herramienta), &referencias, false)?;
    let salida = match proceso.wait_with_output() {
        Ok(salida) => salida,
        Err(error) => return Err(Fallo::Fallo(error.to_string())),
    };
    Ok(salida.status.success())
}

fn lanzar(programa: &str, args: &[&str], con_entrada: bool) -> Result<std::process::Child, Fallo> {
    let mut comando = Command::new(programa);
    comando.args(args);
    if con_entrada {
        comando.stdin(Stdio::piped());
    }
    comando.stdout(Stdio::piped()).stderr(Stdio::null());
    match comando.spawn() {
        Ok(proceso) => Ok(proceso),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Fallo::NoExiste),
        Err(error) => Err(Fallo::Fallo(error.to_string())),
    }
}

fn comprobar(mut proceso: std::process::Child) -> Result<(), Fallo> {
    match proceso.wait() {
        Ok(estado) if estado.success() => Ok(()),
        Ok(estado) => Err(Fallo::Fallo(format!("el llavero devolvió {estado}"))),
        Err(error) => Err(Fallo::Fallo(error.to_string())),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_claves_del_llavero_no_contienen_la_contrasena() {
        let etiqueta = etiqueta("hetzner-01");
        assert!(etiqueta.contains("hetzner-01"));
        assert_eq!(servicio("hetzner-01"), "magi:hetzner-01");
    }

    /// Requiere sesión de escritorio con llavero desbloqueado.
    #[test]
    #[ignore = "requiere llavero del escritorio (secret-tool/gnome-keyring)"]
    fn guarda_recupera_y_olvida_si_hay_llavero() {
        let host = format!("magi-prueba-{}", std::process::id());
        guardar(&host, "tester", "secreta").unwrap();
        let leida = recuperar(&host, "tester").unwrap().unwrap();
        assert_eq!(leida.as_str(), "secreta");
        assert!(olvidar(&host, "tester").unwrap());
        assert!(recuperar(&host, "tester").unwrap().is_none());
    }
}
