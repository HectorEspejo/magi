//! Llavero del sistema: el único lugar donde vive una contraseña de host.
//! En macOS el Keychain se usa por API (`security-framework`, sin subproceso
//! y sin argv visible); en Linux con `secret-tool` por stdin.

pub use magi_nucleo::llavero::{
    es_bloqueado, LlaveroEnMemoria, LlaveroHost, LlaveroSecreto, BLOQUEADO, CUENTA_SINCRONIZACION,
    CUENTA_SINCRONIZACION_ANTERIOR, SERVICIO,
};
pub use magi_nucleo::llavero_fichero::LlaveroFichero;

use std::sync::Arc;

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

// ------------------------------------------------------------- llavero del cliente

/// En qué estado está el llavero de este cliente (Fase 17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoLlavero {
    /// Listo: el del sistema, o el de fichero con la frase en memoria.
    Abierto,
    /// De fichero: existe y la frase no está en memoria (hay que pedirla).
    Bloqueado,
    /// De fichero: todavía no hay `llavero.age` (hay que crear la frase).
    SinCrear,
    /// De fichero: cabecera o contenido ilegibles, con el motivo.
    Danado(String),
}

/// El llavero que usa un cliente (la TUI o la CLI): lo que necesita para
/// guardar, leer y olvidar secretos y contraseñas y para saber si puede.
///
/// Se inyecta (`App.llavero`, `PeticionSondeo.llavero`…): nada de este crate
/// construye un `LlaveroDelSistema` por su cuenta fuera de `del_equipo`, para
/// que Termux (llavero de fichero con frase maestra) y las pruebas (en
/// memoria) pasen por el mismo sitio.
pub trait LlaveroCliente: LlaveroSecreto + LlaveroHost {
    fn estado(&self) -> EstadoLlavero;

    /// ¿Se puede guardar algo (o crear el llavero para poder hacerlo)? El
    /// del sistema pregunta por `secret-tool`; el de fichero siempre puede.
    fn disponible(&self) -> bool;

    /// Borra la contraseña guardada de un host. Devuelve si existía.
    fn olvidar_contrasena(&self, host: &str, usuario: &str) -> Result<bool, String>;

    /// El llavero de fichero, para las operaciones de la frase maestra
    /// (crear, desbloquear, cambiar, borrar, apartar); `None` en el resto.
    fn de_fichero(&self) -> Option<&LlaveroFichero> {
        None
    }

    /// Pone lo que el llavero tiene en memoria al día con lo que hay guardado
    /// ahora. El de fichero guarda las entradas en memoria y otra ventana de
    /// MAGI puede haber reescrito el fichero (rotar la clave de sincronización,
    /// desactivarla): quien va a usar una clave para algo que sale de este
    /// equipo, como la pasada de sincronización, lo llama antes de leerla. El
    /// del sistema y el de memoria siempre leen lo último: no hacen nada.
    ///
    /// Puede bloquear (el cerrojo del fichero y, si cambió, scrypt): desde una
    /// interfaz, en `spawn_blocking`. El error es texto, como en
    /// `LlaveroSecreto`; con la frase que ya no abre el fichero es `BLOQUEADO`.
    fn refrescar(&self) -> Result<(), String> {
        Ok(())
    }
}

impl LlaveroCliente for LlaveroDelSistema {
    fn estado(&self) -> EstadoLlavero {
        EstadoLlavero::Abierto
    }

    fn disponible(&self) -> bool {
        disponible()
    }

    fn olvidar_contrasena(&self, host: &str, usuario: &str) -> Result<bool, String> {
        olvidar(host, usuario)
    }
}

impl LlaveroCliente for LlaveroEnMemoria {
    fn estado(&self) -> EstadoLlavero {
        EstadoLlavero::Abierto
    }

    fn disponible(&self) -> bool {
        true
    }

    fn olvidar_contrasena(&self, host: &str, usuario: &str) -> Result<bool, String> {
        LlaveroSecreto::olvidar(self, &format!("{usuario}@{host}"))
    }
}

impl LlaveroCliente for LlaveroFichero {
    fn estado(&self) -> EstadoLlavero {
        use magi_nucleo::llavero_fichero::EstadoFichero;
        match LlaveroFichero::estado(self) {
            EstadoFichero::Inexistente => EstadoLlavero::SinCrear,
            EstadoFichero::Cerrado => EstadoLlavero::Bloqueado,
            EstadoFichero::Abierto => EstadoLlavero::Abierto,
            EstadoFichero::Danado(motivo) => EstadoLlavero::Danado(motivo),
        }
    }

    fn disponible(&self) -> bool {
        true
    }

    fn olvidar_contrasena(&self, host: &str, usuario: &str) -> Result<bool, String> {
        LlaveroSecreto::olvidar(self, &format!("{usuario}@{host}"))
    }

    fn de_fichero(&self) -> Option<&LlaveroFichero> {
        Some(self)
    }

    fn refrescar(&self) -> Result<(), String> {
        LlaveroFichero::refrescar(self).map_err(|error| error.to_string())
    }
}

/// El llavero de este equipo: el de fichero con frase maestra en Termux y el
/// del sistema (secret-tool o Keychain) en el resto. La decisión de plataforma
/// vive en `termux`; aquí solo se pregunta.
pub fn del_equipo(rutas: &crate::config::Rutas) -> Arc<dyn LlaveroCliente> {
    match crate::termux::llavero_fichero(rutas) {
        Some(llavero) => llavero,
        None => Arc::new(LlaveroDelSistema),
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
