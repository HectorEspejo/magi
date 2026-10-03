//! Diálogo PERMISOS (`p`, Fase 8, §4.2): casillas rwx y octal sincronizados
//! sobre los marcados (o la fila actual), con estado mixto y recursivo
//! opcional. Lo local va por `std::fs` en un hilo; lo remoto, por
//! `CambiarPermisos`. La lógica pura vive en `archivos::permisos`.

use crossterm::event::KeyEvent;

use super::{App, RespuestaArchivos};

/// Diálogos propios de los permisos (viven en el hueco `App::dialogo`).
pub enum DialogoPermisos {}

/// Acciones confirmadas (viajan en `AccionDialogo::Permisos`).
pub enum AccionPermisos {}

/// Peticiones al servidor en vuelo.
#[derive(Debug)]
pub enum PeticionPermisosCliente {}

/// Resultados de los hilos locales (recuento y aplicación).
#[derive(Debug)]
pub enum EventoPermisos {}

impl App {
    /// `p` en Archivos.
    pub(super) fn abrir_permisos(&mut self) {
        self.mensaje("permisos: aún no implementado", true);
    }

    pub(super) fn respuesta_permisos(
        &mut self,
        _peticion_id: u64,
        peticion: PeticionPermisosCliente,
        _respuesta: RespuestaArchivos,
    ) {
        match peticion {}
    }

    pub(super) fn evento_permisos(&mut self, evento: EventoPermisos) {
        match evento {}
    }

    pub(super) fn tecla_dialogo_permisos(&mut self, dialogo: DialogoPermisos, _tecla: KeyEvent) {
        match dialogo {}
    }

    pub(super) fn ejecutar_accion_permisos(&mut self, accion: AccionPermisos) {
        match accion {}
    }
}
