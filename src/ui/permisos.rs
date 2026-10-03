//! Diálogo PERMISOS (Fase 8, §6.1): rejilla 3×3 de casillas rwx, campo octal,
//! recursivo con alcance y aviso ámbar. Mínimo y modo estrecho en
//! `disposicion::MINIMO_PERMISOS` / `ESTRECHO_PERMISOS`.

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::permisos::DialogoPermisos;
use crate::app::App;
use crate::ui::disposicion::{Disposicion, Minimo, MINIMO_PERMISOS};

pub fn dibujar(
    _marco: &mut Frame,
    _area: Rect,
    _app: &App,
    dialogo: &DialogoPermisos,
    _disp: &mut Disposicion,
) {
    match *dialogo {}
}

/// Mínimo que declara el diálogo (T45).
pub fn minimo(_dialogo: &DialogoPermisos) -> Minimo {
    Minimo {
        tamano: MINIMO_PERMISOS,
        exige: "diálogo PERMISOS",
    }
}
