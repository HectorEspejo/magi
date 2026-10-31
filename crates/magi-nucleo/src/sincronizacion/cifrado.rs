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
    /// (AGE-SECRET-KEY-1...).
    pub fn desde_texto(texto: &str) -> Result<Self> {
        let identidad: age::x25519::Identity = texto
            .trim()
            .parse()
            .map_err(|error| anyhow::anyhow!("clave de sincronización no válida: {error}"))?;
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
            .map_err(|error| anyhow::anyhow!("cifrando con age: {error}"))
    }

    /// Descifra un fichero age. El contenido en claro se devuelve en Zeroizing.
    pub fn descifrar(&self, cifrado: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        let plano = age::decrypt(&self.identidad, cifrado)
            .map_err(|error| anyhow::anyhow!("descifrando con age: {error}"))?;
        Ok(Zeroizing::new(plano))
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
    fn cifra_y_descifra_ida_y_vuelta() {
        let clave = ClaveSincronizacion::generar();
        let claro = b"MAGI-SINCRONIZACION 1\nfirma: x\n{}";
        let cifrado = clave.cifrar(claro).unwrap();
        assert_ne!(cifrado, claro);
        assert_eq!(clave.descifrar(&cifrado).unwrap().as_slice(), claro);
        assert!(clave.descifrar(b"no es age").is_err());
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
