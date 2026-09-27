//! Vista Resultados (subvista de `F8`): ejecuciones del servidor y la salida
//! de cada host. En este sprint solo existe la vista vacía; las ejecuciones
//! llegan con el protocolo de ejecución.

use crossterm::event::{KeyCode, KeyEvent};

use crate::ui::Vista;

use super::App;

/// Estado de la vista Resultados.
#[derive(Default)]
pub struct EstadoResultados {}

impl App {
    /// Entra en Resultados (desde `t` en Snippets o la paleta).
    pub(super) fn ir_a_resultados(&mut self) {
        self.salir_de_sesion_si_hace_falta();
        if !matches!(self.vista, Vista::Snippets | Vista::Resultados) {
            self.vista_previa = Some(self.vista);
        }
        self.vista = Vista::Resultados;
        self.ficha = None;
    }

    pub(super) fn tecla_resultados(&mut self, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Char('q') | KeyCode::Esc => self.ir_a_snippets(),
            KeyCode::Char('?') => self.ayuda = true,
            _ => {}
        }
    }
}
