use std::sync::{Arc, Mutex};

/// Buffer de pantalla compartido entre la tarea de conexión (que lo alimenta)
/// y la UI (que lo pinta con tui-term).
pub type Pantalla = Arc<Mutex<vt100::Parser>>;

pub fn nuevo(filas: u16, cols: u16) -> Pantalla {
    Arc::new(Mutex::new(vt100::Parser::new(filas.max(1), cols.max(1), 0)))
}

pub fn redimensionar(pantalla: &Pantalla, filas: u16, cols: u16) {
    if let Ok(mut parser) = pantalla.lock() {
        parser.screen_mut().set_size(filas.max(1), cols.max(1));
    }
}
