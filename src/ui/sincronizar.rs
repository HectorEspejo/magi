//! Diálogo SINCRONIZAR (Fase 8, §6.2) y vista previa del plan (§6.3), con los
//! signos `+ ~ − ·`. La lista de guardadas (§6.5) vive en
//! `ui::sincronizaciones`. Mínimos y modos estrechos en `disposicion`
//! (`MINIMO_SINCRONIZAR`, `MINIMO_VISTA_PREVIA`, `MINIMO_SINCRONIZACIONES`).

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::sincronizar::DialogoSincronizar;
use crate::app::App;
use crate::ui::disposicion::{Disposicion, Minimo, MINIMO_SINCRONIZAR};

pub fn dibujar(
    _marco: &mut Frame,
    _area: Rect,
    _app: &App,
    dialogo: &DialogoSincronizar,
    _disp: &mut Disposicion,
) {
    match *dialogo {}
}

/// Mínimo que declara el diálogo abierto (T45).
pub fn minimo(_dialogo: &DialogoSincronizar) -> Minimo {
    Minimo {
        tamano: MINIMO_SINCRONIZAR,
        exige: "diálogo SINCRONIZAR",
    }
}
