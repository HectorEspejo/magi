//! Certificado X.509 del servidor RDP: huella, sujeto, caducidad y clave
//! pública.
//!
//! La huella se calcula ANTES de mandar ninguna credencial y es la que el
//! cliente compara con HOSTS.huella_escritorio. La clave pública son los bits
//! del SubjectPublicKeyInfo (no el DER), que es lo que espera CredSSP.

use sha2::{Digest as _, Sha256};
use x509_parser::prelude::{FromDer as _, X509Certificate};

use crate::escritorio::ErrorEscritorio;

/// Prefijo de la huella, el mismo estilo que las huellas SSH de MAGI.
pub(crate) const PREFIJO_HUELLA: &str = "SHA256:";

/// Datos del certificado del servidor que hacen falta para decidir y conectar.
#[derive(Debug, Clone)]
pub(crate) struct CertificadoServidor {
    /// SHA-256 del DER en base64 sin relleno, con el prefijo SHA256:.
    pub(crate) huella: String,
    /// Sujeto legible.
    pub(crate) sujeto: String,
    /// Caducidad legible (notAfter).
    pub(crate) caducidad: String,
    /// Bits de la clave pública del SubjectPublicKeyInfo.
    pub(crate) clave_publica: Vec<u8>,
}

impl CertificadoServidor {
    /// Disecciona el DER. Un certificado ilegible es un error del servidor, no
    /// un pánico: se devuelve Protocolo con el motivo.
    pub(crate) fn desde_der(der: Vec<u8>) -> Result<Self, ErrorEscritorio> {
        let huella = huella_sha256(&der);

        let (resto, certificado) = X509Certificate::from_der(&der).map_err(|error| {
            ErrorEscritorio::Protocolo(format!("el certificado del servidor no es X.509: {error}"))
        })?;

        if !resto.is_empty() {
            return Err(ErrorEscritorio::Protocolo(
                "el certificado del servidor lleva datos de más".to_string(),
            ));
        }

        let sujeto = certificado.subject().to_string();
        let caducidad = certificado.validity().not_after.to_string();

        let clave = certificado.public_key();
        if clave.subject_public_key.unused_bits != 0 {
            return Err(ErrorEscritorio::Protocolo(
                "la clave pública del certificado no está alineada a bytes".to_string(),
            ));
        }
        let clave_publica = clave.subject_public_key.data.to_vec();
        if clave_publica.is_empty() {
            return Err(ErrorEscritorio::Protocolo(
                "el certificado del servidor no trae clave pública".to_string(),
            ));
        }

        Ok(Self {
            huella,
            sujeto,
            caducidad,
            clave_publica,
        })
    }
}

/// Huella SHA-256 del DER en base64 sin relleno, con el prefijo SHA256:.
pub(crate) fn huella_sha256(der: &[u8]) -> String {
    use base64::Engine as _;

    let resumen = Sha256::digest(der);
    let texto = base64::engine::general_purpose::STANDARD_NO_PAD.encode(resumen);
    format!("{PREFIJO_HUELLA}{texto}")
}
