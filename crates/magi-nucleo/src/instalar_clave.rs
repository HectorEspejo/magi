//! Instalación de la clave del móvil en un host (Fase 11): script embebido que
//! añade la clave a authorized_keys, con la clave como argumento validado.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

/// Script embebido que instala la clave (§7.9).
///
/// Recibe la línea pública como `$1`; la clave nunca se interpola en el
/// texto del comando.
pub const SCRIPT_INSTALAR_CLAVE: &str = include_str!("../scripts/instalar_clave.sh");

/// Línea pública que §7.9 permite instalar.
const PATRON_LINEA_PUBLICA: &str =
    r"^ecdsa-sha2-nistp256 [A-Za-z0-9+/]+={0,2} [A-Za-z0-9._@-]{1,64}$";

/// Expresión regular compilada una sola vez.
static LINEA_PUBLICA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(PATRON_LINEA_PUBLICA).expect("la expresión regular de §7.9 es válida")
});

/// Valida la línea pública antes de instalarla.
pub fn validar_linea_publica(linea: &str) -> Result<(), String> {
    if LINEA_PUBLICA.is_match(linea) {
        Ok(())
    } else {
        Err("la línea pública debe ser «ecdsa-sha2-nistp256 <base64> <comentario>»".to_string())
    }
}

/// Compone el comando `sh -c '<script>' magi '<clave>'` que instala la clave.
///
/// El script viaja como argumento de `sh -c` (su `$0` es «magi») y la
/// clave como `$1`, así que ninguna de las dos es texto del comando. Las dos se
/// escapan entre comillas simples.
pub fn comando_instalar_clave(linea: &str) -> Result<String, String> {
    validar_linea_publica(linea)?;
    let script = shell_escape::escape(Cow::Borrowed(SCRIPT_INSTALAR_CLAVE));
    let clave = shell_escape::escape(Cow::Borrowed(linea));
    Ok(format!("sh -c {script} magi {clave}"))
}
