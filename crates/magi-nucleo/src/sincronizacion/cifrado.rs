//! Cifrado (age X25519) y firma (HMAC-SHA256 con clave derivada por HKDF)
//! de la sincronización del inventario (Fase 10).
//!
//! La identidad es una clave X25519 de age. Todos los equipos comparten la
//! misma identidad, que se guarda en el llavero y se muestra como texto y QR
//! para añadir equipos. La firma impide fabricar un fichero válido con solo la
//! clave pública: age no autentica al emisor (D127).

use age::secrecy::ExposeSecret;
use anyhow::{Context, Result};
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::sincronizacion::formato::{INFO_FIRMA, SAL_HKDF};

type HmacSha256 = Hmac<Sha256>;

/// Por qué no se pudo descifrar un fichero (R85).
///
/// La pasada solo pide una clave nueva cuando ninguna identidad coincide: un
/// fichero cortado, vacío o que no es age se va a reintentar solo en la
/// siguiente pasada, y no tiene nada que ver con la clave. El `Display` de los
/// dos casos es el de siempre y nunca pasa por el de age (ver
/// `texto_error_descifrado`).
///
/// Limitación conocida de age 0.11.5: una entrada X25519 de la cabecera
/// corrupta cuenta como «otra clave» (`NoMatchingKeys`), igual que un fichero
/// cifrado para otra identidad.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorDescifrado {
    /// Ninguna identidad coincide: el fichero está cifrado para otra clave. Es
    /// el único caso que pide la clave nueva.
    #[error("descifrando con age: la clave no abre este fichero (está cifrado para otra)")]
    NingunaIdentidad,
    /// Cualquier otra causa: truncado, vacío, cabecera o MAC alterados, formato
    /// desconocido o contenido dañado. Lleva la explicación en castellano.
    #[error("descifrando con age: {0}")]
    Danado(String),
}

/// Comienzo del texto canónico de una identidad de age.
const PREFIJO_CLAVE: &str = "AGE-SECRET-KEY-1";

/// Normaliza el texto de una clave pegada o escaneada: sin espacios ni saltos
/// de línea y en mayúsculas, su texto canónico. Copiada de una terminal suele
/// llegar partida en dos líneas. Un solo búfer, en `Zeroizing`: es el secreto.
pub fn normalizar_clave(texto: &str) -> Zeroizing<String> {
    let mut normalizado = Zeroizing::new(String::with_capacity(texto.len()));
    for caracter in texto.chars().filter(|caracter| !caracter.is_whitespace()) {
        normalizado.push(caracter);
    }
    normalizado.make_ascii_uppercase();
    normalizado
}

/// Identidad de sincronización: la clave privada X25519 que descifra y firma.
pub struct ClaveSincronizacion {
    identidad: age::x25519::Identity,
}

impl ClaveSincronizacion {
    /// Genera una identidad nueva.
    pub fn generar() -> Self {
        Self {
            identidad: age::x25519::Identity::generate(),
        }
    }

    /// Parsea una identidad del texto que se muestra al usuario
    /// (AGE-SECRET-KEY-1...), pegada o escaneada: se normaliza antes (sin
    /// espacios ni saltos de línea y en mayúsculas) y el error dice qué le pasa
    /// en castellano, sin los textos de age.
    pub fn desde_texto(texto: &str) -> Result<Self> {
        let normalizado = normalizar_clave(texto);
        if normalizado.is_empty() {
            anyhow::bail!("la clave de sincronización está vacía");
        }
        if normalizado.starts_with("AGE1") {
            anyhow::bail!(
                "la clave de sincronización no es válida: es una clave pública (age1…); copia la que empieza por {PREFIJO_CLAVE}"
            );
        }
        if !normalizado.starts_with(PREFIJO_CLAVE) {
            anyhow::bail!(
                "la clave de sincronización no es válida: debe empezar por {PREFIJO_CLAVE}"
            );
        }
        let identidad: age::x25519::Identity = normalizado.parse().map_err(|_| {
            anyhow::anyhow!(
                "la clave de sincronización no es válida: está incompleta o tiene algún carácter cambiado"
            )
        })?;
        Ok(Self { identidad })
    }

    /// Texto de la clave, en Zeroizing para que no quede en memoria sin
    /// borrar.
    pub fn texto(&self) -> Zeroizing<String> {
        let secreto = self.identidad.to_string();
        Zeroizing::new(secreto.expose_secret().to_string())
    }

    /// Clave pública age1..., que no viaja en la carpeta.
    pub fn clave_publica(&self) -> String {
        self.identidad.to_public().to_string()
    }

    /// Huella corta de la clave pública para comparar equipos a simple vista
    /// ("7f3a·91c2"): SHA-256 de la clave pública, primeros 4 bytes en hex.
    pub fn huella_corta(&self) -> String {
        huella_corta_de(self.clave_publica().as_bytes())
    }

    /// Clave de firma derivada: HKDF-SHA256(ikm = secreto canónico de la
    /// identidad, salt = "magi", info = "magi-sincronizacion-firma-v1").
    fn clave_firma(&self) -> Result<Zeroizing<[u8; 32]>> {
        let secreto = self.identidad.to_string();
        let hkdf = Hkdf::<Sha256>::new(Some(SAL_HKDF), secreto.expose_secret().as_bytes());
        let mut clave = Zeroizing::new([0u8; 32]);
        hkdf.expand(INFO_FIRMA, clave.as_mut())
            .map_err(|_| anyhow::anyhow!("derivando la clave de firma"))?;
        Ok(clave)
    }

    /// Firma HMAC-SHA256 del JSON del documento, en hexadecimal minúscula.
    pub fn firmar(&self, json: &[u8]) -> Result<String> {
        let clave = self.clave_firma()?;
        let mut mac = HmacSha256::new_from_slice(clave.as_ref())
            .map_err(|error| anyhow::anyhow!("clave de firma: {error}"))?;
        mac.update(json);
        Ok(a_hex(&mac.finalize().into_bytes()))
    }

    /// Comprueba una firma en tiempo constante.
    pub fn verificar(&self, json: &[u8], firma_hex: &str) -> bool {
        let Ok(firma) = desde_hex(firma_hex) else {
            return false;
        };
        let Ok(clave) = self.clave_firma() else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(clave.as_ref()) else {
            return false;
        };
        mac.update(json);
        mac.verify_slice(&firma).is_ok()
    }

    /// Cifra un contenido en claro con age a la clave pública de esta
    /// identidad (todos los equipos comparten identidad).
    pub fn cifrar(&self, plano: &[u8]) -> Result<Vec<u8>> {
        let recipiente = self.identidad.to_public();
        age::encrypt(&recipiente, plano)
            .map_err(|error| anyhow::anyhow!("cifrando con age: {}", texto_error_cifrado(&error)))
    }

    /// Descifra un fichero age. El contenido en claro se devuelve en Zeroizing.
    ///
    /// Solo «ninguna identidad coincide» es `NingunaIdentidad`: todo lo demás
    /// (R85) es `Danado`, con su explicación en castellano.
    pub fn descifrar(&self, cifrado: &[u8]) -> Result<Zeroizing<Vec<u8>>, ErrorDescifrado> {
        match age::decrypt(&self.identidad, cifrado) {
            Ok(plano) => Ok(Zeroizing::new(plano)),
            Err(age::DecryptError::NoMatchingKeys) => Err(ErrorDescifrado::NingunaIdentidad),
            Err(error) => Err(ErrorDescifrado::Danado(texto_error_descifrado(&error))),
        }
    }
}

impl std::fmt::Debug for ClaveSincronizacion {
    fn fmt(&self, formato: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formato
            .debug_struct("ClaveSincronizacion")
            .field("huella_corta", &self.huella_corta())
            .finish()
    }
}

/// Explica un error de descifrado de age sin pasar por su `Display`: ese carga
/// los textos traducidos de age y, si no los encuentra (una compilación debug
/// en el móvil), entra en pánico con `LanguageNotAvailable`. La pasada prueba
/// cada clave y descarta el error, así que fallar aquí es el camino normal.
fn texto_error_descifrado(error: &age::DecryptError) -> String {
    use age::DecryptError;
    match error {
        DecryptError::NoMatchingKeys => {
            "la clave no abre este fichero (está cifrado para otra)".to_string()
        }
        DecryptError::InvalidHeader => "la cabecera age no es válida".to_string(),
        DecryptError::InvalidMac => "la cabecera age está alterada (MAC no válido)".to_string(),
        DecryptError::UnknownFormat => "formato age desconocido".to_string(),
        DecryptError::DecryptionFailed => "el contenido cifrado está dañado".to_string(),
        DecryptError::KeyDecryptionFailed => "no se pudo descifrar la clave".to_string(),
        DecryptError::ExcessiveWork { required, target } => format!(
            "el fichero pide un factor de trabajo excesivo ({required}; el máximo es {target})"
        ),
        // Al descifrar de memoria, solo el flujo de age da errores de E/S, y su
        // texto («decryption error») no añade nada.
        DecryptError::Io(_) => "el contenido cifrado está dañado o incompleto".to_string(),
    }
}

/// Explica un error de cifrado de age sin pasar por su `Display` (ver
/// `texto_error_descifrado`).
fn texto_error_cifrado(error: &age::EncryptError) -> String {
    use age::EncryptError;
    match error {
        EncryptError::Io(error) => format!("error de escritura: {error}"),
        EncryptError::EncryptedIdentities(error) => texto_error_descifrado(error),
        EncryptError::IncompatibleRecipients { .. }
        | EncryptError::InvalidRecipientLabels(_)
        | EncryptError::MissingRecipients
        | EncryptError::MixedRecipientAndPassphrase => "age rechazó el destinatario".to_string(),
    }
}

/// Huella corta (8 hex) de unos bytes: se usa con la clave pública.
pub fn huella_corta_de(bytes: &[u8]) -> String {
    let resumen = Sha256::digest(bytes);
    let hex = a_hex(&resumen[..4]);
    format!("{}·{}", &hex[..4], &hex[4..])
}

/// SHA-256 en hexadecimal minúscula (para hashes de fichero).
pub fn sha256_hex(bytes: &[u8]) -> String {
    a_hex(&Sha256::digest(bytes))
}

pub fn a_hex(bytes: &[u8]) -> String {
    let mut texto = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        texto.push_str(&format!("{byte:02x}"));
    }
    texto
}

pub fn desde_hex(texto: &str) -> Result<Vec<u8>> {
    let texto = texto.trim();
    if texto.len() % 2 != 0 {
        anyhow::bail!("hexadecimal con longitud impar");
    }
    let mut bytes = Vec::with_capacity(texto.len() / 2);
    for par in texto.as_bytes().chunks(2) {
        let alto = (par[0] as char)
            .to_digit(16)
            .context("hexadecimal no válido")?;
        let bajo = (par[1] as char)
            .to_digit(16)
            .context("hexadecimal no válido")?;
        bytes.push((alto * 16 + bajo) as u8);
    }
    Ok(bytes)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn una_identidad_va_y_vuelve_del_texto() {
        let clave = ClaveSincronizacion::generar();
        let texto = clave.texto();
        assert!(texto.starts_with("AGE-SECRET-KEY-1"));
        let otra = ClaveSincronizacion::desde_texto(&texto).unwrap();
        assert_eq!(clave.clave_publica(), otra.clave_publica());
        assert_eq!(clave.huella_corta(), otra.huella_corta());
        assert!(ClaveSincronizacion::desde_texto("no es una clave").is_err());
    }

    #[test]
    fn la_clave_pegada_se_normaliza_y_sus_errores_dicen_que_pasa() {
        let clave = ClaveSincronizacion::generar();
        let texto = clave.texto();
        // Partida en dos líneas, con espacios y en minúsculas, como al copiarla
        // de una terminal.
        let (principio, fin) = texto.split_at(40);
        let pegada = format!("  {}\n  {} \t", principio.to_lowercase(), fin);
        let leida = ClaveSincronizacion::desde_texto(&pegada).unwrap();
        assert_eq!(leida.clave_publica(), clave.clave_publica());
        assert_eq!(
            normalizar_clave("  age-secret-key-1 abc \n  def\t").as_str(),
            "AGE-SECRET-KEY-1ABCDEF"
        );

        let error = |texto: &str| {
            ClaveSincronizacion::desde_texto(texto)
                .unwrap_err()
                .to_string()
        };
        assert_eq!(error(" \n "), "la clave de sincronización está vacía");
        assert!(
            error(&clave.clave_publica()).contains("es una clave pública"),
            "{}",
            error(&clave.clave_publica())
        );
        assert!(error("no es una clave").contains("debe empezar por AGE-SECRET-KEY-1"));
        let cortada = &texto[..texto.len() - 3];
        assert!(
            error(cortada).contains("está incompleta o tiene algún carácter cambiado"),
            "{}",
            error(cortada)
        );
        // Ningún error lleva el texto en inglés de age.
        assert!(!error(cortada).contains("Bech32"));
    }

    #[test]
    fn cifra_y_descifra_ida_y_vuelta() {
        let clave = ClaveSincronizacion::generar();
        let claro = b"MAGI-SINCRONIZACION 1\nfirma: x\n{}";
        let cifrado = clave.cifrar(claro).unwrap();
        assert_ne!(cifrado, claro);
        assert_eq!(clave.descifrar(&cifrado).unwrap().as_slice(), claro);
        assert!(clave.descifrar(b"no es age").is_err());
    }

    #[test]
    fn los_errores_de_age_se_explican_sin_sus_textos_traducidos() {
        // El Display de age carga sus .ftl: en la APK debug no los encontraba y
        // la pasada entraba en pánico (LanguageNotAvailable). El texto es el
        // propio, en castellano, para cualquier fichero que no abre.
        let clave = ClaveSincronizacion::generar();
        let ajena = ClaveSincronizacion::generar();
        let cifrado = ajena.cifrar(b"de otra clave").unwrap();
        assert_eq!(
            clave.descifrar(&cifrado).unwrap_err().to_string(),
            "descifrando con age: la clave no abre este fichero (está cifrado para otra)"
        );
        assert_eq!(
            clave.descifrar(b"no es age").unwrap_err().to_string(),
            "descifrando con age: la cabecera age no es válida"
        );
        let propio = clave.cifrar(b"contenido que se corta").unwrap();
        let cortado = &propio[..propio.len() - 8];
        assert_eq!(
            clave.descifrar(cortado).unwrap_err().to_string(),
            "descifrando con age: el contenido cifrado está dañado o incompleto"
        );
    }

    #[test]
    fn solo_ninguna_identidad_coincide_pide_otra_clave_y_lo_demas_es_un_fichero_danado() {
        // R85: otra clave, y nada más que otra clave, es `NingunaIdentidad`.
        let clave = ClaveSincronizacion::generar();
        let ajena = ClaveSincronizacion::generar();
        let de_otra = ajena.cifrar(b"de otra clave").unwrap();
        assert_eq!(
            clave.descifrar(&de_otra).unwrap_err(),
            ErrorDescifrado::NingunaIdentidad
        );
        // Vacío, que no es age, cortado en la cabecera o en el contenido, o con
        // un byte del contenido cambiado: ficheros dañados o a medio copiar.
        let propio = clave.cifrar(b"contenido de un fichero de equipo").unwrap();
        let mut alterado = propio.clone();
        let ultimo = alterado.len() - 1;
        alterado[ultimo] ^= 0xff;
        for (nombre, bytes) in [
            ("vacío", Vec::new()),
            ("que no es age", b"esto no es un fichero age".to_vec()),
            ("cortado en la cabecera", propio[..30].to_vec()),
            ("cortado por la mitad", propio[..propio.len() / 2].to_vec()),
            ("con el final alterado", alterado),
        ] {
            match clave.descifrar(&bytes) {
                Err(ErrorDescifrado::Danado(motivo)) => {
                    assert!(!motivo.is_empty(), "{nombre}");
                }
                otro => panic!("{nombre}: se esperaba un fichero dañado y salió {otro:?}"),
            }
        }
        // El texto de siempre, sin el de age.
        assert_eq!(
            ErrorDescifrado::NingunaIdentidad.to_string(),
            "descifrando con age: la clave no abre este fichero (está cifrado para otra)"
        );
    }

    #[test]
    fn la_firma_detecta_alteraciones_y_la_comprueba_cualquier_copia_de_la_clave() {
        let clave = ClaveSincronizacion::generar();
        let texto = clave.texto();
        let otra = ClaveSincronizacion::desde_texto(&texto).unwrap();
        let json = br#"{"formato":1}"#;
        let firma = clave.firmar(json).unwrap();
        assert_eq!(firma.len(), 64);
        assert!(otra.verificar(json, &firma));
        assert!(!otra.verificar(b"{\"formato\":2}", &firma));
        assert!(!otra.verificar(json, "zz"));
        // Otra identidad distinta no valida la firma.
        let ajena = ClaveSincronizacion::generar();
        assert!(!ajena.verificar(json, &firma));
    }

    #[test]
    fn el_hexadecimal_va_y_vuelve_y_rechaza_lo_invalido() {
        assert_eq!(a_hex(&[0, 15, 16, 255]), "000f10ff");
        assert_eq!(desde_hex("000f10ff").unwrap(), vec![0, 15, 16, 255]);
        assert!(desde_hex("abc").is_err());
        assert!(desde_hex("zz").is_err());
        assert_eq!(huella_corta_de(b"").chars().count(), 9);
    }
}
