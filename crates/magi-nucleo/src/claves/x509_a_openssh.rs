//! Clave pública X.509 (SPKI) del Keystore a línea OpenSSH.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

use super::comun::{escribir_cadena, Lector, ALGORITMO, CURVA};

/// OID 1.2.840.10045.2.1 (clave pública de curva elíptica).
const OID_CLAVE_ELIPTICA: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];

/// OID 1.2.840.10045.3.1.7 (prime256v1).
const OID_PRIME256V1: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];

/// Longitud de un punto sin comprimir de P-256: 0x04 y las dos coordenadas.
const LONGITUD_PUNTO: usize = 65;

/// Devuelve la línea OpenSSH de una clave pública SPKI de P-256.
///
/// El comentario debe ser seguro para authorized_keys: 1-64 caracteres de
/// `A-Za-z0-9._@-`.
pub fn x509_a_openssh(spki: &[u8], comentario: &str) -> Result<String, String> {
    validar_comentario(comentario)?;
    let punto = punto_de_spki(spki)?;
    let mut blob = Vec::with_capacity(4 + ALGORITMO.len() + 4 + CURVA.len() + 4 + punto.len());
    escribir_cadena(&mut blob, ALGORITMO.as_bytes());
    escribir_cadena(&mut blob, CURVA.as_bytes());
    escribir_cadena(&mut blob, &punto);
    Ok(format!(
        "{ALGORITMO} {} {comentario}",
        STANDARD.encode(blob)
    ))
}

/// Extrae el punto sin comprimir de una SPKI de P-256.
fn punto_de_spki(spki: &[u8]) -> Result<Vec<u8>, String> {
    let mut lector = Lector::nuevo(spki);
    let secuencia = lector.leer(0x30, "clave X.509")?;
    lector.terminar("clave X.509")?;
    let mut interior = Lector::nuevo(secuencia);
    let algoritmo = interior.leer(0x30, "clave X.509: algoritmo")?;
    let punto = interior.leer(0x03, "clave X.509: punto")?;
    interior.terminar("clave X.509")?;
    let mut algoritmo_lector = Lector::nuevo(algoritmo);
    let oid_clave = algoritmo_lector.leer(0x06, "clave X.509: OID de clave")?;
    let oid_curva = algoritmo_lector.leer(0x06, "clave X.509: OID de curva")?;
    algoritmo_lector.terminar("clave X.509")?;
    if oid_clave != OID_CLAVE_ELIPTICA || oid_curva != OID_PRIME256V1 {
        return Err("clave X.509: no es una clave P-256".to_string());
    }
    let Some((&sin_usar, coordenadas)) = punto.split_first() else {
        return Err("clave X.509: falta el punto de la clave".to_string());
    };
    if sin_usar != 0 {
        return Err("clave X.509: el punto de la clave no está alineado".to_string());
    }
    if coordenadas.len() != LONGITUD_PUNTO || coordenadas[0] != 0x04 {
        return Err("clave X.509: el punto de la clave no es de P-256 sin comprimir".to_string());
    }
    Ok(coordenadas.to_vec())
}

/// Valida el comentario que acompañará a la línea.
fn validar_comentario(comentario: &str) -> Result<(), String> {
    let valido = !comentario.is_empty()
        && comentario.len() <= 64
        && comentario.chars().all(|caracter| {
            caracter.is_ascii_alphanumeric() || matches!(caracter, '.' | '_' | '@' | '-')
        });
    if valido {
        Ok(())
    } else {
        Err("el comentario de la clave debe tener 1-64 caracteres [A-Za-z0-9._@-]".to_string())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::claves::comun::vectores;

    #[test]
    fn convierte_el_vector_fijo() {
        let spki = vectores::de_hex(vectores::SPKI_HEX);
        let linea = x509_a_openssh(&spki, "magi@prueba").expect("el SPKI del vector es válido");
        assert_eq!(linea, vectores::LINEA);
        let publica = ssh_key::PublicKey::from_openssh(&linea).expect("la línea es OpenSSH");
        assert_eq!(publica.to_openssh().unwrap(), linea);
        assert!(matches!(
            publica.key_data(),
            ssh_key::public::KeyData::Ecdsa(_)
        ));
        assert_eq!(publica.comment().as_str().unwrap(), "magi@prueba");
    }

    #[test]
    fn rechaza_lo_que_no_es_una_p256_sin_comprimir() {
        let spki = vectores::de_hex(vectores::SPKI_HEX);
        // OID de otra curva (último byte del OID de prime256v1).
        let mut otra_curva = spki.clone();
        otra_curva[22] = 0x08;
        assert!(x509_a_openssh(&otra_curva, "magi@prueba").is_err());
        // Punto comprimido.
        let mut comprimido = spki.clone();
        comprimido[26] = 0x02;
        assert!(x509_a_openssh(&comprimido, "magi@prueba").is_err());
        // Bits sin usar del BIT STRING distintos de cero.
        let mut desalineado = spki.clone();
        desalineado[25] = 0x01;
        assert!(x509_a_openssh(&desalineado, "magi@prueba").is_err());
        // Truncada y con datos sobrantes.
        assert!(x509_a_openssh(&spki[..30], "magi@prueba").is_err());
        let mut sobrante = spki.clone();
        sobrante.push(0x00);
        assert!(x509_a_openssh(&sobrante, "magi@prueba").is_err());
        assert!(x509_a_openssh(&[], "magi@prueba").is_err());
    }

    #[test]
    fn valida_el_comentario() {
        let spki = vectores::de_hex(vectores::SPKI_HEX);
        assert!(x509_a_openssh(&spki, "magi@Pixel-7_20261031").is_ok());
        assert!(x509_a_openssh(&spki, "").is_err());
        assert!(x509_a_openssh(&spki, "magi@Pixel 7").is_err());
        assert!(x509_a_openssh(&spki, "magi@ñandú").is_err());
        let largo = "a".repeat(65);
        assert!(x509_a_openssh(&spki, &largo).is_err());
        let justo = "a".repeat(64);
        assert!(x509_a_openssh(&spki, &justo).is_ok());
    }
}
