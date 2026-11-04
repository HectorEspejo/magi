//! Conversiones de claves del móvil (Fase 11): de la firma DER del Keystore al
//! formato SSH ecdsa-sha2-nistp256, y de la clave pública X.509 a una línea
//! OpenSSH.

mod comun;

pub mod der_a_ssh;
pub mod firmante;
pub mod x509_a_openssh;

pub use firmante::Firmante;
