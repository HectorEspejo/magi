//! Pegado del terminal: saneado y marcas del pegado entre corchetes (Fase 15).
//!
//! Todo lo pegado pasa por aquí, en el cliente y en la escritura en varios:
//! un texto pegado nunca puede salirse del modo de pegado con marcas propias
//! ni llevar controles que el remoto interprete como órdenes.

/// Sanea el texto pegado: fuera las marcas del pegado entre corchetes y los
/// controles salvo tabulador y saltos de línea; los finales de línea, a CR.
#[must_use]
pub fn sanear(texto: &str) -> String {
    let sin_marcas = texto.replace("\x1b[201~", "").replace("\x1b[200~", "");
    let filtrado: String = sin_marcas
        .chars()
        .filter(|caracter| {
            *caracter == '\t' || *caracter == '\n' || *caracter == '\r' || !caracter.is_control()
        })
        .collect();
    filtrado.replace("\r\n", "\r").replace('\n', "\r")
}

/// ¿El texto pegado tiene varias líneas? Los CR finales no cuentan: pegar una
/// sola orden con su salto de línea no dispara el aviso.
#[must_use]
pub fn varias_lineas(texto: &str) -> bool {
    texto.trim_end_matches('\r').contains('\r')
}

/// Bytes que se escriben al remoto para un pegado: con las marcas si activó
/// el pegado entre corchetes (modo 2004), y siempre como una escritura sola.
#[must_use]
pub fn pegar(texto_saneado: &str, pegado_corchetes: bool) -> Option<Vec<u8>> {
    if texto_saneado.is_empty() {
        return None;
    }
    let mut bytes = Vec::with_capacity(texto_saneado.len() + 12);
    if pegado_corchetes {
        bytes.extend_from_slice(b"\x1b[200~");
    }
    bytes.extend_from_slice(texto_saneado.as_bytes());
    if pegado_corchetes {
        bytes.extend_from_slice(b"\x1b[201~");
    }
    Some(bytes)
}

/// Trocea un pegado grande por el tamaño máximo del códec sin partir un
/// carácter UTF-8. Se conserva el orden; el último trozo puede ser menor.
#[must_use]
pub fn trocear(bytes: &[u8], maximo: usize) -> Vec<Vec<u8>> {
    if maximo == 0 || bytes.is_empty() {
        return Vec::new();
    }
    let mut trozos = Vec::new();
    let mut inicio = 0;
    while inicio < bytes.len() {
        let mut fin = (inicio + maximo).min(bytes.len());
        while fin > inicio + 1 && fin < bytes.len() && (bytes[fin] & 0xc0) == 0x80 {
            fin -= 1;
        }
        if fin == inicio + 1 && inicio + 1 < bytes.len() && (bytes[inicio] & 0xc0) == 0xc0 {
            // El carácter UTF-8 no cabe en maximo: sale entero en este trozo
            // (el tamaño del códec real es de MiB, pero así nunca se parte).
            fin = inicio + largo_utf8(bytes[inicio]);
        }
        trozos.push(bytes[inicio..fin].to_vec());
        inicio = fin;
    }
    trozos
}

/// Longitud en bytes de un carácter UTF-8 por su primer byte.
fn largo_utf8(primero: u8) -> usize {
    match primero {
        0xf0..=0xff => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_marcas_interiores_desaparecen() {
        let texto = "antes\x1b[201~rm -rf ~\x1b[200~despues";
        let saneado = sanear(texto);
        assert!(!saneado.contains("\x1b[201~"));
        assert!(!saneado.contains("\x1b[200~"));
        let bytes = pegar(&saneado, true).unwrap();
        assert_eq!(bytes, b"\x1b[200~antesrm -rf ~despues\x1b[201~");
    }

    #[test]
    fn sin_2004_no_hay_marcas_y_los_saltos_van_a_cr() {
        let saneado = sanear("uno\r\ndos\ntres\ttab\u{7}");
        assert_eq!(saneado, "uno\rdos\rtres\ttab");
        assert_eq!(pegar(&saneado, false).unwrap(), b"uno\rdos\rtres\ttab");
        assert!(pegar("", true).is_none());
    }

    #[test]
    fn varias_lineas_ignora_el_salto_final() {
        assert!(!varias_lineas("uptime\r"));
        assert!(!varias_lineas("uptime"));
        assert!(varias_lineas("uno\rdos\r"));
    }

    #[test]
    fn el_troceo_no_parte_caracteres_utf8() {
        let texto = "aé日b".as_bytes();
        let trozos = trocear(texto, 3);
        assert_eq!(trozos.concat(), texto);
        for trozo in &trozos {
            assert!(std::str::from_utf8(trozo).is_ok());
        }
    }
}
