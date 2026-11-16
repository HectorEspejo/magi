//! Pintado del diálogo «Nueva sesión persistente» (Fase 16, §6.2 del informe).
//!
//! **CONTRATO-S3-A2.** Esqueleto compilable: la pista A2 lo rellena. Todo se
//! deriva del área en cada pintado y declara su mínimo y su modo estrecho en
//! `disposicion.rs` (T45).

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::persistentes_nueva::FormularioNueva;
use crate::app::App;
use crate::ui::disposicion::{Disposicion, Minimo, Tamano};

/// Mínimo del diálogo «Nueva sesión persistente» (CONTRATO-S3-A2).
pub const MINIMO_NUEVA: Tamano = Tamano::new(40, 12);

#[must_use]
pub fn minimo(_formulario: &FormularioNueva) -> Minimo {
    Minimo {
        tamano: MINIMO_NUEVA,
        exige: "nueva sesión persistente",
    }
}

/// Pinta el diálogo como una `Hoja` de `dialogos.rs`.
pub fn dibujar(
    _marco: &mut Frame,
    _area: Rect,
    _app: &App,
    _formulario: &FormularioNueva,
    _disp: &mut Disposicion,
) {
}
