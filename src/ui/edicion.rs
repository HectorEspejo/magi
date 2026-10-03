//! Diálogos de la edición remota (Fase 8, §6.4): «¿subir cambios?», conflicto
//! («el fichero cambió en el host»), aviso de propietario y fallo de la
//! subida con la ruta del temporal. Mínimo y modo estrecho en
//! `disposicion::MINIMO_EDICION` / `ESTRECHO_EDICION`.

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::edicion::DialogoEdicion;
use crate::app::App;
use crate::ui::disposicion::{Disposicion, Minimo, MINIMO_EDICION};

pub fn dibujar(
    _marco: &mut Frame,
    _area: Rect,
    _app: &App,
    dialogo: &DialogoEdicion,
    _disp: &mut Disposicion,
) {
    match *dialogo {}
}

/// Mínimo que declara el diálogo (T45).
pub fn minimo(_dialogo: &DialogoEdicion) -> Minimo {
    Minimo {
        tamano: MINIMO_EDICION,
        exige: "diálogo de edición",
    }
}
