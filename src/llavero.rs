//! Llavero del sistema: el único lugar donde vive una contraseña de host.
//! En macOS el Keychain se usa por API (`security-framework`, sin subproceso
//! y sin argv visible); en Linux con `secret-tool` por stdin.

use std::process::{Command, Stdio};

use zeroize::Zeroizing;

/// Servicio con el que MAGI firma sus entradas en el llavero del sistema.
pub const SERVICIO: &str = "magi";

// ------------------------------------------------------------- macOS: Keychain por API

#[cfg(target_os = "macos")]
mod backend {
    use super::SERVICIO;

    /// Cuenta de la entrada: usuario@host para distinguir hosts y usuarios.
    fn cuenta(host: &str, usuario: &str) -> String {
        format!("{usuario}@{host}")
    }

    use security_framework::passwords as keychain;
    use zeroize::{Zeroize as _, Zeroizing};

    /// `errSecItemNotFound` del Security framework.
    const ITEM_NO_ENCONTRADO: i32 = -25300;

    pub fn guardar(host: &str, usuario: &str, contrasena: &str) -> Result<(), String> {
        keychain::set_generic_password(
            SERVICIO,
            cuenta(host, usuario).as_bytes(),
            contrasena.as_bytes(),
        )
        .map_err(|error| format!("Keychain: {error}"))
    }

    pub fn recuperar(host: &str, usuario: &str) -> Result<Option<Zeroizing<String>>, String> {
        match keychain::get_generic_password(SERVICIO, cuenta(host, usuario).as_bytes()) {
            Ok(bytes) => {
                let texto = String::from_utf8_lossy(&bytes).to_string();
                // La copia devuelta por la API se descarta con zeroize.
                let mut bytes = bytes;
                bytes.zeroize();
                if texto.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(Zeroizing::new(texto)))
                }
            }
            Err(error) if error.code() == ITEM_NO_ENCONTRADO => Ok(None),
            Err(error) => Err(format!("Keychain: {error}")),
        }
    }

    pub fn olvidar(host: &str, usuario: &str) -> Result<bool, String> {
        match keychain::delete_generic_password(SERVICIO, cuenta(host, usuario).as_bytes()) {
            Ok(()) => Ok(true),
            Err(error) if error.code() == ITEM_NO_ENCONTRADO => Ok(false),
            Err(error) => Err(format!("Keychain: {error}")),
        }
    }

    /// El Keychain del usuario está siempre disponible por API en macOS.
    pub fn disponible() -> bool {
        true
    }
}

// ------------------------------------------------------------- Linux: secret-tool por stdin

#[cfg(not(target_os = "macos"))]
mod backend {
    use super::{Command, Stdio, Zeroizing, SERVICIO};
    use std::io::Write as _;

    enum Fallo {
        NoExiste,
        Fallo(String),
    }

    pub fn guardar(host: &str, usuario: &str, contrasena: &str) -> Result<(), String> {
        let mut errores: Vec<String> = Vec::new();
        let resultado = lanzar(
            "secret-tool",
            &[
                "store",
                "--label",
                &format!("{SERVICIO} · {host}"),
                "magi-host",
                host,
                "magi-user",
                usuario,
            ],
            true,
        );
        match resultado {
            Err(Fallo::NoExiste) => {
                return Err("no se encontró secret-tool (llavero del sistema)".to_string())
            }
            Err(Fallo::Fallo(motivo)) => errores.push(motivo),
            Ok(mut proceso) => {
                if let Some(mut entrada) = proceso.stdin.take() {
                    if let Err(error) = entrada.write_all(contrasena.as_bytes()) {
                        errores.push(error.to_string());
                    }
                }
                if let Err(Fallo::Fallo(motivo)) = comprobar(&mut proceso) {
                    errores.push(motivo);
                }
                if errores.is_empty() {
                    return Ok(());
                }
            }
        }
        Err(errores.join(" · "))
    }

    pub fn recuperar(host: &str, usuario: &str) -> Result<Option<Zeroizing<String>>, String> {
        let args = ["lookup", "magi-host", host, "magi-user", usuario];
        let mut comando = Command::new("secret-tool");
        comando.args(args);
        comando.stdout(Stdio::piped()).stderr(Stdio::null());
        let proceso = match comando.spawn() {
            Ok(proceso) => proceso,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err("no se encontró secret-tool (llavero del sistema)".to_string());
            }
            Err(error) => return Err(error.to_string()),
        };
        let salida = proceso
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        if !salida.status.success() {
            return Ok(None);
        }
        let texto = String::from_utf8_lossy(&salida.stdout)
            .trim_end_matches(['\n', '\r'])
            .to_string();
        Ok((!texto.is_empty()).then_some(Zeroizing::new(texto)))
    }

    pub fn olvidar(host: &str, usuario: &str) -> Result<bool, String> {
        let args = ["clear", "magi-host", host, "magi-user", usuario];
        let mut comando = Command::new("secret-tool");
        comando.args(args);
        comando.stdout(Stdio::null()).stderr(Stdio::null());
        match comando.spawn() {
            Ok(mut proceso) => {
                let salida = proceso.wait().map_err(|error| error.to_string())?;
                Ok(salida.success())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err("no se encontró secret-tool (llavero del sistema)".to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    /// ¿Hay algún llavero disponible (secret-tool presente)?
    pub fn disponible() -> bool {
        Command::new("secret-tool")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    fn lanzar(
        programa: &str,
        args: &[&str],
        con_entrada: bool,
    ) -> Result<std::process::Child, Fallo> {
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

    fn comprobar(proceso: &mut std::process::Child) -> Result<(), Fallo> {
        match proceso.wait() {
            Ok(estado) if estado.success() => Ok(()),
            Ok(estado) => Err(Fallo::Fallo(format!("el llavero devolvió {estado}"))),
            Err(error) => Err(Fallo::Fallo(error.to_string())),
        }
    }
}

// ------------------------------------------------------------- API pública

/// Guarda la contraseña de un host en el llavero del sistema. Nunca toca
/// `magi.db`.
pub fn guardar(host: &str, usuario: &str, contrasena: &str) -> Result<(), String> {
    backend::guardar(host, usuario, contrasena)
}

/// Recupera la contraseña guardada de un host, si existe.
pub fn recuperar(host: &str, usuario: &str) -> Result<Option<Zeroizing<String>>, String> {
    backend::recuperar(host, usuario)
}

/// Borra la contraseña guardada de un host. Devuelve si existía.
pub fn olvidar(host: &str, usuario: &str) -> Result<bool, String> {
    backend::olvidar(host, usuario)
}

/// ¿Hay algún llavero disponible en este sistema?
pub fn disponible() -> bool {
    backend::disponible()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Requiere llavero del escritorio (Keychain o secret-tool desbloqueado).
    #[test]
    #[ignore = "requiere llavero del escritorio desbloqueado"]
    fn guarda_recupera_y_olvida_si_hay_llavero() {
        let host = format!("magi-prueba-{}", std::process::id());
        guardar(&host, "tester", "secreta").unwrap();
        let leida = recuperar(&host, "tester").unwrap().unwrap();
        assert_eq!(leida.as_str(), "secreta");
        assert!(olvidar(&host, "tester").unwrap());
        assert!(recuperar(&host, "tester").unwrap().is_none());
    }
}
