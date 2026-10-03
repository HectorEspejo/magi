//! Lista de sincronizaciones guardadas del host (`L`, Fase 8, §6.5): nombre,
//! dirección con `−` si borra, rutas y último resultado, con detalle
//! inferior. Mínimo y modo estrecho en `disposicion::MINIMO_SINCRONIZACIONES`
//! / `ESTRECHO_SINCRONIZACIONES`.

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::guardadas::DialogoGuardadas;
use crate::app::App;
use crate::ui::disposicion::{Disposicion, Minimo, MINIMO_SINCRONIZACIONES};

pub fn dibujar(
    _marco: &mut Frame,
    _area: Rect,
    _app: &App,
    dialogo: &DialogoGuardadas,
    _disp: &mut Disposicion,
) {
    match *dialogo {}
}

/// Mínimo que declara el diálogo abierto (T45).
pub fn minimo(_dialogo: &DialogoGuardadas) -> Minimo {
    Minimo {
        tamano: MINIMO_SINCRONIZACIONES,
        exige: "SINCRONIZACIONES",
    }
}
