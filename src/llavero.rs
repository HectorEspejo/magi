//! Llavero del sistema: el único lugar donde vive una contraseña de host.
//! En macOS el Keychain se usa por API (`security-framework`, sin subproceso
//! y sin argv visible); en Linux con `secret-tool` por stdin.

pub use magi_nucleo::llavero::{
    LlaveroEnMemoria, LlaveroHost, LlaveroSecreto, CUENTA_SINCRONIZACION,
    CUENTA_SINCRONIZACION_ANTERIOR, SERVICIO,
};

use zeroize::Zeroizing;

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
        // security-framework 3.x pide service y account como &str.
        let cuenta = cuenta(host, usuario);
        keychain::set_generic_password(SERVICIO, &cuenta, contrasena.as_bytes())
            .map_err(|error| format!("Keychain: {error}"))
    }

    pub fn recuperar(host: &str, usuario: &str) -> Result<Option<Zeroizing<String>>, String> {
        let cuenta = cuenta(host, usuario);
        match keychain::get_generic_password(SERVICIO, &cuenta) {
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
        let cuenta = cuenta(host, usuario);
        match keychain::delete_generic_password(SERVICIO, &cuenta) {
            Ok(()) => Ok(true),
            Err(error) if error.code() == ITEM_NO_ENCONTRADO => Ok(false),
            Err(error) => Err(format!("Keychain: {error}")),
        }
    }

    pub fn guardar_secreto(cuenta: &str, valor: &str) -> Result<(), String> {
        keychain::set_generic_password(SERVICIO, cuenta, valor.as_bytes())
            .map_err(|error| format!("Keychain: {error}"))
    }

    pub fn recuperar_secreto(cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        match keychain::get_generic_password(SERVICIO, cuenta) {
            Ok(mut bytes) => {
                let texto = String::from_utf8_lossy(&bytes).to_string();
                bytes.zeroize();
                Ok((!texto.is_empty()).then_some(Zeroizing::new(texto)))
            }
            Err(error) if error.code() == ITEM_NO_ENCONTRADO => Ok(None),
            Err(error) => Err(format!("Keychain: {error}")),
        }
    }

    pub fn olvidar_secreto(cuenta: &str) -> Result<bool, String> {
        match keychain::delete_generic_password(SERVICIO, cuenta) {
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
    use super::{Zeroizing, SERVICIO};
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    enum Fallo {
        NoExiste,
        Fallo(String),
    }

    /// Sin `secret-tool` no hay llavero: el aviso dice qué paquete instalar.
    const FALTA_SECRET_TOOL: &str = "no se encontró secret-tool (llavero del sistema): instálalo con «sudo apt install libsecret-tools» en Debian y Ubuntu, «sudo pacman -S libsecret» en Arch o «sudo dnf install libsecret» en Fedora";

    /// Tope de lo que se enseña de la salida de error de `secret-tool`.
    const TOPE_SALIDA_ERROR: usize = 300;

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
            Err(Fallo::NoExiste) => return Err(FALTA_SECRET_TOOL.to_string()),
            Err(Fallo::Fallo(motivo)) => errores.push(motivo),
            Ok(mut proceso) => {
                if let Some(mut entrada) = proceso.stdin.take() {
                    if let Err(error) = entrada.write_all(contrasena.as_bytes()) {
                        errores.push(error.to_string());
                    }
                }
                if let Err(Fallo::Fallo(motivo)) = comprobar(proceso) {
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
                return Err(FALTA_SECRET_TOOL.to_string());
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
                Err(FALTA_SECRET_TOOL.to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn guardar_secreto(cuenta: &str, valor: &str) -> Result<(), String> {
        let mut errores: Vec<String> = Vec::new();
        let etiqueta = format!("{SERVICIO} · {cuenta}");
        let resultado = lanzar(
            "secret-tool",
            &["store", "--label", &etiqueta, "magi-clave", cuenta],
            true,
        );
        match resultado {
            Err(Fallo::NoExiste) => return Err(FALTA_SECRET_TOOL.to_string()),
            Err(Fallo::Fallo(motivo)) => errores.push(motivo),
            Ok(mut proceso) => {
                if let Some(mut entrada) = proceso.stdin.take() {
                    if let Err(error) = entrada.write_all(valor.as_bytes()) {
                        errores.push(error.to_string());
                    }
                }
                if let Err(Fallo::Fallo(motivo)) = comprobar(proceso) {
                    errores.push(motivo);
                }
                if errores.is_empty() {
                    return Ok(());
                }
            }
        }
        Err(errores.join(" · "))
    }

    pub fn recuperar_secreto(cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        let mut comando = Command::new("secret-tool");
        comando.args(["lookup", "magi-clave", cuenta]);
        comando.stdout(Stdio::piped()).stderr(Stdio::null());
        let proceso = match comando.spawn() {
            Ok(proceso) => proceso,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(FALTA_SECRET_TOOL.to_string());
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

    pub fn olvidar_secreto(cuenta: &str) -> Result<bool, String> {
        let mut comando = Command::new("secret-tool");
        comando.args(["clear", "magi-clave", cuenta]);
        comando.stdout(Stdio::null()).stderr(Stdio::null());
        match comando.spawn() {
            Ok(mut proceso) => {
                let salida = proceso.wait().map_err(|error| error.to_string())?;
                Ok(salida.success())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(FALTA_SECRET_TOOL.to_string())
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
        // La salida de error explica el fallo; nunca lleva el secreto, que
        // entra por la entrada estándar.
        comando.stdout(Stdio::piped()).stderr(Stdio::piped());
        match comando.spawn() {
            Ok(proceso) => Ok(proceso),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Fallo::NoExiste),
            Err(error) => Err(Fallo::Fallo(error.to_string())),
        }
    }

    /// Espera a `secret-tool` (con la entrada estándar ya cerrada) y, si falla,
    /// explica por qué con lo que dijo por su salida de error.
    fn comprobar(proceso: std::process::Child) -> Result<(), Fallo> {
        match proceso.wait_with_output() {
            Ok(salida) if salida.status.success() => Ok(()),
            Ok(salida) => Err(Fallo::Fallo(explicar_fallo(
                salida.status.code(),
                &salida.stderr,
            ))),
            Err(error) => Err(Fallo::Fallo(error.to_string())),
        }
    }

    /// Texto del fallo de `secret-tool`: su código, su salida de error en una
    /// línea y acotada y, en los casos conocidos, qué hacer.
    fn explicar_fallo(codigo: Option<i32>, salida_error: &[u8]) -> String {
        let texto = String::from_utf8_lossy(salida_error)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut recortado: String = texto.chars().take(TOPE_SALIDA_ERROR).collect();
        if recortado.len() < texto.len() {
            recortado.push('…');
        }
        let codigo = codigo.map_or_else(
            || "sin código".to_string(),
            |codigo| format!("código {codigo}"),
        );
        let mut motivo = if recortado.is_empty() {
            format!("el llavero falló ({codigo})")
        } else {
            format!("el llavero falló ({codigo}): {recortado}")
        };
        if texto.contains("org.freedesktop.secrets") {
            motivo.push_str(
                " · no hay ningún servicio de llavero en la sesión: instala gnome-keyring (o activa el de KWallet o KeePassXC) y vuelve a iniciar sesión",
            );
        } else if texto.contains("autolaunch") || texto.contains("DBUS_SESSION_BUS_ADDRESS") {
            motivo.push_str(" · no hay sesión de D-Bus: ejecuta MAGI desde la sesión gráfica");
        }
        motivo
    }

    #[cfg(test)]
    mod pruebas {
        use super::{explicar_fallo, FALTA_SECRET_TOOL};

        #[test]
        fn el_fallo_del_llavero_dice_el_motivo_y_que_hacer() {
            let sin_servicio = explicar_fallo(
                Some(1),
                b"The name org.freedesktop.secrets was not provided by any .service files\n",
            );
            assert!(
                sin_servicio.starts_with("el llavero falló (código 1): The name org.freedesktop.secrets was not provided by any .service files"),
                "{sin_servicio}"
            );
            assert!(
                sin_servicio.contains("instala gnome-keyring"),
                "{sin_servicio}"
            );

            let sin_dbus = explicar_fallo(Some(1), b"Cannot autolaunch D-Bus without X11 $DISPLAY");
            assert!(
                sin_dbus.contains("ejecuta MAGI desde la sesión gráfica"),
                "{sin_dbus}"
            );

            assert_eq!(explicar_fallo(Some(1), b""), "el llavero falló (código 1)");
            assert_eq!(
                explicar_fallo(None, b" \n"),
                "el llavero falló (sin código)"
            );

            // Una salida enorme se acota y queda en una línea.
            let larga = "x ".repeat(1_000);
            let acotada = explicar_fallo(Some(2), larga.as_bytes());
            assert!(acotada.ends_with('…'), "{acotada}");
            assert!(!acotada.contains('\n'));
            assert!(acotada.chars().count() < 400);
        }

        #[test]
        fn sin_secret_tool_se_dice_que_paquete_instalar() {
            assert!(FALTA_SECRET_TOOL.contains("libsecret-tools"));
            assert!(FALTA_SECRET_TOOL.contains("pacman -S libsecret"));
        }
    }
}

/// El llavero real del sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct LlaveroDelSistema;

impl LlaveroSecreto for LlaveroDelSistema {
    fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String> {
        guardar_secreto(cuenta, valor)
    }

    fn recuperar(&self, cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        recuperar_secreto(cuenta)
    }

    fn olvidar(&self, cuenta: &str) -> Result<bool, String> {
        olvidar_secreto(cuenta)
    }
}

impl LlaveroHost for LlaveroDelSistema {
    fn guardar_contrasena(
        &self,
        host: &str,
        usuario: &str,
        contrasena: &str,
    ) -> Result<(), String> {
        guardar(host, usuario, contrasena)
    }

    fn recuperar_contrasena(
        &self,
        host: &str,
        usuario: &str,
    ) -> Result<Option<Zeroizing<String>>, String> {
        recuperar(host, usuario)
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

/// Guarda un secreto genérico de MAGI (p. ej. la clave de sincronización) en
/// el llavero, indexado por una cuenta estable. Nunca toca magi.db.
pub fn guardar_secreto(cuenta: &str, valor: &str) -> Result<(), String> {
    backend::guardar_secreto(cuenta, valor)
}

/// Recupera un secreto genérico del llavero.
pub fn recuperar_secreto(cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
    backend::recuperar_secreto(cuenta)
}

/// Borra un secreto genérico del llavero. Devuelve si existía.
pub fn olvidar_secreto(cuenta: &str) -> Result<bool, String> {
    backend::olvidar_secreto(cuenta)
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
