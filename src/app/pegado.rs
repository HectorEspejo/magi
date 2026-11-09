//! Pegado del terminal exterior (Fase 15).
//!
//! Todo lo pegado pasa por `magi_nucleo::terminal::pegado`: saneado, sin
//! teclas detrás y con las marcas del pegado entre corchetes cuando el remoto
//! activó el modo 2004. Con varias líneas y un remoto sin 2004, se avisa antes
//! de pegar (`[terminal] avisar_pegado`).

use magi_nucleo::terminal::pegado as nucleo;
use magi_nucleo::terminal::teclas::ModoTerminal;

use crate::protocolo::BytesPestana;

/// Lo que hay que hacer con un pegado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionPegado {
    /// Nada que enviar (texto vacío tras sanear).
    Nada,
    /// Enviar estos bytes, ya saneados y con marcas si tocan.
    Enviar(Vec<u8>),
    /// El remoto no protege el pegado y hay varias líneas: pedir confirmación.
    Avisar(String),
}

/// Prepara un pegado para una pestaña con sus modos.
#[must_use]
pub fn preparar(texto: &str, modo: ModoTerminal, avisar: bool) -> AccionPegado {
    let saneado = nucleo::sanear(texto);
    if saneado.is_empty() {
        return AccionPegado::Nada;
    }
    if !modo.pegado_corchetes && avisar && nucleo::varias_lineas(&saneado) {
        return AccionPegado::Avisar(saneado);
    }
    match nucleo::pegar(&saneado, modo.pegado_corchetes) {
        Some(bytes) => AccionPegado::Enviar(bytes),
        None => AccionPegado::Nada,
    }
}

/// Un pegado pendiente de confirmación: pestañas que necesitan el aviso y
/// bytes ya preparados para las que no.
#[derive(Debug, Clone)]
pub struct PendientePegado {
    pub sesion_id: u32,
    pub texto: String,
    /// Bytes de las pestañas con 2004 que no esperan al aviso.
    pub otros: Vec<BytesPestana>,
    /// Pestañas sin 2004: (sesión, texto saneado).
    pub avisos: Vec<(u32, String)>,
}

/// Las primeras líneas de un pegado, para el aviso.
#[must_use]
pub fn vista_previa(texto: &str, maximo: usize) -> Vec<String> {
    let mut lineas: Vec<String> = texto
        .split('\r')
        .take(maximo)
        .map(|linea| linea.to_string())
        .collect();
    let total = texto.split('\r').count();
    if total > maximo {
        lineas.push(format!("… {} más", total - maximo));
    }
    lineas
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn modo(pegado_corchetes: bool) -> ModoTerminal {
        ModoTerminal {
            cursor_aplicacion: false,
            pegado_corchetes,
            banderas_kitty: 0,
        }
    }

    #[test]
    fn sin_2004_una_sola_orden_se_pega_sin_avisar() {
        assert_eq!(
            preparar("uptime\n", modo(false), true),
            AccionPegado::Enviar(b"uptime\r".to_vec())
        );
    }

    #[test]
    fn sin_2004_varias_lineas_avisan_y_con_2004_no() {
        let texto = "uno\ndos\ntres\n";
        assert_eq!(
            preparar(texto, modo(false), true),
            AccionPegado::Avisar("uno\rdos\rtres\r".to_string())
        );
        assert_eq!(
            preparar(texto, modo(true), true),
            AccionPegado::Enviar(b"\x1b[200~uno\rdos\rtres\r\x1b[201~".to_vec())
        );
        // El aviso se puede apagar.
        assert!(matches!(
            preparar(texto, modo(false), false),
            AccionPegado::Enviar(_)
        ));
    }

    #[test]
    fn un_pegado_vacio_o_solo_controles_no_envia_nada() {
        assert_eq!(preparar("\x1b\x07", modo(false), true), AccionPegado::Nada);
        assert_eq!(preparar("", modo(true), true), AccionPegado::Nada);
    }

    #[test]
    fn la_vista_previa_recorta_y_cuenta_el_resto() {
        let lineas = vista_previa("a\rb\rc\rd", 2);
        assert_eq!(lineas, vec!["a", "b", "… 2 más"]);
    }
}
