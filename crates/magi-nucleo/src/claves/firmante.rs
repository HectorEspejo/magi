//! Firmante inyectable: la clave del móvil nunca está en la memoria del
//! proceso. El núcleo pide la clave pública y las firmas a la plataforma
//! (Android Keystore en el móvil; en el escritorio no se usa).

/// Servicio de firma con una clave que no está en memoria.
pub trait Firmante: Send + Sync {
    /// Clave pública en formato X.509 SubjectPublicKeyInfo (DER).
    fn clave_publica_x509(&self) -> Result<Vec<u8>, String>;

    /// Firma SHA256withECDSA de los datos; devuelve la firma en DER.
    fn firmar(&self, datos: &[u8]) -> Result<Vec<u8>, String>;
}
