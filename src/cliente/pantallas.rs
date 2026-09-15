//! Pantallas del cliente: un parser `vt100` por pestaña, compartido con la
//! tarea de lectura (que lo alimenta) y el bucle de UI (que lo pinta).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::conexion::terminal::{nuevo, Pantalla};

/// Registro de pantallas por sesión. La tarea de lectura alimenta el parser y
/// marca la pestaña como sucia; el bucle de la UI la repinta a ~30 fps.
#[derive(Default, Clone)]
pub struct Pantallas {
    interno: Arc<Mutex<HashMap<u32, Pantalla>>>,
}

impl Pantallas {
    /// Crea el parser de una pestaña y devuelve el puntero compartido para la UI.
    pub fn crear(&self, sesion_id: u32, filas: u16, cols: u16) -> Pantalla {
        let pantalla = nuevo(filas, cols);
        self.interno
            .lock()
            .expect("pantallas del cliente")
            .insert(sesion_id, pantalla.clone());
        pantalla
    }

    /// Reemplaza el contenido del parser de una pestaña con el volcado del
    /// servidor (`PantallaCompleta`), manteniendo el mismo puntero que la UI.
    /// Devuelve el puntero vigente (creándolo si aún no existe).
    pub fn volcar(&self, sesion_id: u32, filas: u16, cols: u16, bytes: &[u8]) -> Option<Pantalla> {
        let pantalla = {
            let mut guardia = self.interno.lock().expect("pantallas del cliente");
            match guardia.get(&sesion_id) {
                Some(pantalla) => pantalla.clone(),
                None => {
                    let pantalla = nuevo(filas, cols);
                    guardia.insert(sesion_id, pantalla.clone());
                    pantalla
                }
            }
        };
        if let Ok(mut parser) = pantalla.lock() {
            parser.screen_mut().set_size(filas.max(1), cols.max(1));
            parser.process(bytes);
        }
        Some(pantalla)
    }

    pub fn quitar(&self, sesion_id: u32) {
        self.interno
            .lock()
            .expect("pantallas del cliente")
            .remove(&sesion_id);
    }

    /// Alimenta el parser de una pestaña con datos del remoto.
    pub fn procesar(&self, sesion_id: u32, bytes: &[u8]) {
        let pantalla = {
            let guardia = self.interno.lock().expect("pantallas del cliente");
            guardia.get(&sesion_id).cloned()
        };
        if let Some(pantalla) = pantalla {
            if let Ok(mut parser) = pantalla.lock() {
                parser.process(bytes);
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_pantallas_se_crean_se_vuelcan_y_se_quitan() {
        let pantallas = Pantallas::default();
        let pantalla = pantallas.crear(1, 24, 80);
        assert!(Arc::ptr_eq(
            &pantalla,
            &pantallas.volcar(1, 24, 80, b"hola").unwrap()
        ));
        assert_eq!(
            pantalla.lock().unwrap().screen().contents(),
            "hola".to_string()
        );
        pantallas.procesar(1, b" mundo");
        assert_eq!(
            pantalla.lock().unwrap().screen().contents(),
            "hola mundo".to_string()
        );
        pantallas.quitar(1);
        pantallas.procesar(1, b"sin parser"); // sin pestaña: no hace nada
    }
}
