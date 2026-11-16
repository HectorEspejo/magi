//! Pintado de las sesiones persistentes (Fase 16): el bloque «Persistentes» de
//! la vista Sesiones, la vista previa y los diálogos (nueva sesión, ficha,
//! renombrar, terminar, borrar y limpiar).
//!
//! **CONTRATO-S3-A.** Esqueleto compilable del contrato del Sprint 3: la pista
//! A lo rellena. Todo se deriva del área en cada pintado y declara su mínimo y
//! su modo estrecho en `disposicion.rs` (T45).

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::persistentes::DialogoPersistentes;
use crate::app::App;
use crate::ui::disposicion::{Disposicion, Minimo, Tamano};

/// Mínimo de los diálogos de las sesiones persistentes (CONTRATO-S3-A).
pub const MINIMO_PERSISTENTES: Tamano = Tamano::new(40, 12);

/// Mínimo del diálogo abierto.
#[must_use]
pub fn minimo(_dialogo: &DialogoPersistentes) -> Minimo {
    Minimo {
        tamano: MINIMO_PERSISTENTES,
        exige: "sesión persistente",
    }
}

/// Pinta el diálogo abierto como una `Hoja` de `dialogos.rs`.
pub fn dibujar(
    _marco: &mut Frame,
    _area: Rect,
    _app: &App,
    _dialogo: &DialogoPersistentes,
    _disp: &mut Disposicion,
) {
}
