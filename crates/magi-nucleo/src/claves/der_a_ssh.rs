//! Firma DER (SEQUENCE de dos INTEGER) del Keystore a la firma que espera
//! SSH para ecdsa-sha2-nistp256: r y s como mpint.

use super::comun::{escribir_cadena, Lector, ALGORITMO};

/// Longitud máxima de r o s en P-256 (32 bytes).
const LONGITUD_MAXIMA_ENTERO: usize = 32;

/// Convierte una firma DER en el blob de firma SSH que espera russh: el
/// algoritmo y, como una cadena, los dos mpint de la firma.
pub fn der_a_ssh(der: &[u8]) -> Result<Vec<u8>, String> {
    let mut lector = Lector::nuevo(der);
    let secuencia = lector.leer(0x30, "firma DER")?;
    lector.terminar("firma DER")?;
    let mut interior = Lector::nuevo(secuencia);
    let r = interior.leer(0x02, "firma DER: r")?;
    let s = interior.leer(0x02, "firma DER: s")?;
    interior.terminar("firma DER")?;
    let mut firma =
        Vec::with_capacity(4 + ALGORITMO.len() + 4 + 2 * (4 + LONGITUD_MAXIMA_ENTERO + 1));
    escribir_cadena(&mut firma, ALGORITMO.as_bytes());
    let mut enteros = Vec::with_capacity(2 * (4 + LONGITUD_MAXIMA_ENTERO + 1));
    escribir_mpint(&mut enteros, r)?;
    escribir_mpint(&mut enteros, s)?;
    escribir_cadena(&mut firma, &enteros);
    Ok(firma)
}

/// Escribe un INTEGER DER como mpint SSH.
///
/// Quita los ceros a la izquierda del DER y vuelve a poner el 0x00 de positivo
/// si el bit alto del primer byte significativo está a 1.
fn escribir_mpint(salida: &mut Vec<u8>, entero_der: &[u8]) -> Result<(), String> {
    let Some((&primero, _)) = entero_der.split_first() else {
        return Err("firma DER: INTEGER vacío".to_string());
    };
    if primero & 0x80 != 0 {
        return Err("firma DER: r o s es negativo".to_string());
    }
    let mut bytes = entero_der;
    while bytes.len() > 1 && bytes[0] == 0 {
        bytes = &bytes[1..];
    }
    if bytes.iter().all(|byte| *byte == 0) {
        return Err("firma DER: r o s vale cero".to_string());
    }
    if bytes.len() > LONGITUD_MAXIMA_ENTERO {
        return Err("firma DER: r o s no cabe en P-256".to_string());
    }
    let mut valor = Vec::with_capacity(bytes.len() + 1);
    if bytes[0] & 0x80 != 0 {
        valor.push(0);
    }
    valor.extend_from_slice(bytes);
    escribir_cadena(salida, &valor);
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::claves::comun::vectores;
    use crate::claves::x509_a_openssh::x509_a_openssh;
    use ssh_key::encoding::Decode as _;

    /// Construye una firma DER con dos enteros, para las pruebas de error.
    fn der_con_enteros(r: &[u8], s: &[u8]) -> Vec<u8> {
        let mut contenido = Vec::new();
        for entero in [r, s] {
            contenido.push(0x02);
            contenido.push(u8::try_from(entero.len()).unwrap());
            contenido.extend_from_slice(entero);
        }
        let mut der = vec![0x30, u8::try_from(contenido.len()).unwrap()];
        der.extend_from_slice(&contenido);
        der
    }

    #[test]
    fn convierte_el_vector_fijo() {
        let firma = der_a_ssh(&vectores::de_hex(vectores::FIRMA_DER_HEX))
            .expect("la firma del vector es válida");
        assert_eq!(firma, vectores::de_hex(vectores::FIRMA_SSH_HEX));
    }

    #[test]
    fn la_firma_verifica_con_la_linea_openssh() {
        let firma = der_a_ssh(&vectores::de_hex(vectores::FIRMA_DER_HEX)).unwrap();
        let linea = x509_a_openssh(&vectores::de_hex(vectores::SPKI_HEX), "magi@prueba")
            .expect("el SPKI del vector es válido");
        assert_eq!(linea, vectores::LINEA);
        let publica = ssh_key::PublicKey::from_openssh(vectores::LINEA)
            .expect("la línea OpenSSH del vector es válida");
        let mut lector = firma.as_slice();
        let firma_ssh = ssh_key::Signature::decode(&mut lector).expect("la firma SSH es válida");
        assert!(lector.is_empty());
        signature::Verifier::verify(&publica, vectores::MENSAJE, &firma_ssh)
            .expect("la firma tiene que verificar con la clave pública");
    }

    #[test]
    fn los_enteros_pequenos_se_rellenan_sin_ceros_de_mas() {
        let firma = der_a_ssh(&der_con_enteros(&[0x01], &[0x01])).unwrap();
        let esperada = vectores::de_hex(
            "0000001365636473612d736861322d6e697374703235360000000a00000001010000000101",
        );
        assert_eq!(firma, esperada);
    }

    #[test]
    fn rechaza_lo_que_no_es_una_firma_der_valida() {
        let valida = vectores::de_hex(vectores::FIRMA_DER_HEX);
        assert!(der_a_ssh(&[]).is_err());
        assert!(der_a_ssh(&[0x31, 0x00]).is_err());
        // Datos sobrantes tras la SEQUENCE.
        let mut con_sobrante = valida.clone();
        con_sobrante.push(0x00);
        assert!(der_a_ssh(&con_sobrante).is_err());
        // SEQUENCE truncada.
        assert!(der_a_ssh(&valida[..10]).is_err());
        // Etiqueta que no es INTEGER.
        assert!(der_a_ssh(&[0x30, 0x03, 0x04, 0x01, 0x01]).is_err());
        // Un solo entero.
        assert!(der_a_ssh(&[0x30, 0x03, 0x02, 0x01, 0x01]).is_err());
        // r negativo, r cero y r que no cabe en P-256.
        assert!(der_a_ssh(&der_con_enteros(&[0x80], &[0x01])).is_err());
        assert!(der_a_ssh(&der_con_enteros(&[0x00], &[0x01])).is_err());
        assert!(der_a_ssh(&der_con_enteros(&[0x01; 33], &[0x01])).is_err());
        // INTEGER vacío.
        assert!(der_a_ssh(&[0x30, 0x05, 0x02, 0x00, 0x02, 0x01, 0x01]).is_err());
    }
}
