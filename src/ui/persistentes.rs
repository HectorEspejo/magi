//! Pintado de las sesiones persistentes (Fase 16): el bloque «Persistentes» de
//! la vista Sesiones, la vista previa y los diálogos (nueva sesión, ficha,
//! renombrar, terminar, borrar y limpiar).
//!
//! **CONTRATO-S3-A1.** Esqueleto compilable del contrato del Sprint 3: la pista
//! A1 lo rellena. Todo se deriva del área en cada pintado y declara su mínimo y
//! su modo estrecho en `disposicion.rs` (T45).

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::persistentes::DialogoPersistentes;
use crate::app::App;
use crate::ui::disposicion::{Disposicion, Minimo};

/// Mínimo del diálogo abierto.
#[must_use]
pub fn minimo(dialogo: &DialogoPersistentes) -> Minimo {
    match dialogo {
        DialogoPersistentes::Nueva(formulario) => crate::ui::persistentes_nueva::minimo(formulario),
    }
}

/// Pinta el diálogo abierto como una `Hoja` de `dialogos.rs`.
pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoPersistentes,
    disp: &mut Disposicion,
) {
    match dialogo {
        DialogoPersistentes::Nueva(formulario) => {
            crate::ui::persistentes_nueva::dibujar(marco, area, app, formulario, disp);
        }
    }
}
