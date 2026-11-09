//! Pruebas de caja del pegado del terminal (Fase 15): saneado del texto,
//! finales de línea, marcas del pegado entre corchetes (modo 2004) y troceo
//! por el tamaño del códec. Sin red ni interfaz.

use magi_nucleo::terminal::pegado::{pegar, sanear, trocear, varias_lineas};

#[test]
fn las_marcas_del_pegado_del_remoto_no_se_cuelan() {
    // AC del checklist: un texto con ESC[201~ dentro sale como un solo bloque
    // entre ESC[200~ y ESC[201~, sin la marca interior.
    let texto = "antes\u{1b}[201~rm -rf ~\u{1b}[200~despues";
    let saneado = sanear(texto);
    assert_eq!(saneado, "antesrm -rf ~despues");
    let bytes = pegar(&saneado, true).expect("pegado no vacío");
    assert_eq!(bytes, b"\x1b[200~antesrm -rf ~despues\x1b[201~");
    // Las marcas solo aparecen al principio y al final, una vez cada una.
    let texto = std::str::from_utf8(&bytes).expect("utf-8");
    assert_eq!(texto.matches("\u{1b}[200~").count(), 1);
    assert_eq!(texto.matches("\u{1b}[201~").count(), 1);
}

#[test]
fn se_quitan_los_controles_salvo_tabulador_y_saltos() {
    // Se va el ESC y el control C1; los bytes imprimibles del CSI se quedan
    // como texto (sin ESC no son una orden).
    let saneado = sanear("uno\u{1b}[31mdos\u{7}tres\u{0}cuatro\u{8}cinco\u{9b}seis");
    assert_eq!(saneado, "uno[31mdostrescuatrocincoseis");
    let saneado = sanear("tab\taquí\nsalto\r\ncrlf");
    assert_eq!(saneado, "tab\taquí\rsalto\rcrlf");
}

#[test]
fn los_finales_de_linea_terminan_en_cr() {
    assert_eq!(sanear("uno\ndos"), "uno\rdos");
    assert_eq!(sanear("uno\r\ndos"), "uno\rdos");
    assert_eq!(sanear("uno\rdos"), "uno\rdos");
    assert_eq!(sanear("uno\n\ndos\n"), "uno\r\rdos\r");
}

#[test]
fn sin_el_modo_2004_no_se_pone_la_marca() {
    assert_eq!(pegar("hola", false).expect("no vacío"), b"hola");
    assert_eq!(pegar("una\rdos", false).expect("no vacío"), b"una\rdos");
}

#[test]
fn un_pegado_vacio_no_se_manda() {
    assert!(pegar("", false).is_none());
    assert!(pegar("", true).is_none());
}

#[test]
fn varias_lineas_ignora_el_salto_final() {
    assert!(!varias_lineas("uptime"));
    assert!(!varias_lineas("uptime\r"));
    assert!(!varias_lineas("uptime\r\r"));
    assert!(varias_lineas("uno\rdos"));
    assert!(varias_lineas("uno\rdos\r"));
}

#[test]
fn el_troceo_conserva_el_orden_y_no_parte_caracteres() {
    let texto = "aé日bçd";
    let bytes = texto.as_bytes();
    // El máximo real del códec es de MiB; cualquier valor mayor que un carácter
    // UTF-8 (4 bytes) tiene que conservar los caracteres enteros.
    for maximo in [4, 5, 6, 7, 8, 100] {
        let trozos = trocear(bytes, maximo);
        assert_eq!(trozos.concat(), bytes, "máximo {maximo}");
        for trozo in &trozos {
            assert!(trozo.len() <= maximo, "máximo {maximo}");
            assert!(std::str::from_utf8(trozo).is_ok(), "máximo {maximo}");
        }
    }
}

#[test]
fn trocear_casos_limite() {
    assert!(trocear(b"", 8).is_empty());
    assert!(trocear(b"hola", 0).is_empty());
    assert_eq!(trocear(b"hola", 4), vec![b"hola".to_vec()]);
    assert_eq!(trocear(b"hola", 2), vec![b"ho".to_vec(), b"la".to_vec()]);
    assert_eq!(trocear(b"hola", 3), vec![b"hol".to_vec(), b"a".to_vec()]);
}

#[test]
fn un_pegado_de_varias_lineas_con_2004_sale_como_un_solo_bloque() {
    let texto = sanear("uno\ndos\ntres\ncuatro\ncinco\n");
    assert!(varias_lineas(&texto));
    assert_eq!(texto, "uno\rdos\rtres\rcuatro\rcinco\r");
    let bytes = pegar(&texto, true).expect("no vacío");
    assert_eq!(bytes, b"\x1b[200~uno\rdos\rtres\rcuatro\rcinco\r\x1b[201~");
}
