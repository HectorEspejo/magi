//! Edición de ficheros con `E` (Fase 8, §4.1): un fichero remoto se baja a un
//! temporal privado del servidor, se abre con el editor del usuario con la TUI
//! suspendida, se compara por SHA-256 y, si cambió, se sube con los permisos
//! del original tras comprobar que el remoto no cambió entretanto.
//!
//! La lógica pura del ciclo vive en `archivos::edicion`; aquí están los
//! diálogos, las peticiones al servidor y el lanzamiento del editor. Las
//! ediciones viven en la `App` y no en `EstadoArchivos`: una subida en curso
//! sobrevive a salir de Archivos o a cambiar de host (T55).

use crossterm::event::KeyEvent;

use crate::protocolo::InfoTransferencia;

use super::{App, RespuestaArchivos};

/// Ediciones en marcha en esta ventana.
#[derive(Default)]
pub struct Ediciones {
    pub siguiente: u64,
}

/// Diálogos propios de la edición (viven en el hueco `App::dialogo`).
pub enum DialogoEdicion {}

/// Acciones confirmadas de la edición (viajan en `AccionDialogo::Edicion`).
pub enum AccionEdicion {}

/// Peticiones al servidor en vuelo de una edición.
#[derive(Debug)]
pub enum PeticionEdicion {}

/// Fichero que hay que abrir con el editor en cuanto el bucle pueda suspender
/// la TUI.
#[derive(Debug, Clone)]
pub struct PeticionEditor {
    pub ruta: std::path::PathBuf,
    /// Edición remota a la que pertenece el temporal; ninguna si es un
    /// fichero local.
    pub edicion: Option<u64>,
}

impl App {
    /// `E` en Archivos: editar el fichero de la fila.
    pub(super) fn editar_entrada(&mut self) {
        self.mensaje("editar: aún no implementado", true);
    }

    /// Respuesta del servidor a una petición de edición.
    pub(super) fn respuesta_edicion(
        &mut self,
        _peticion_id: u64,
        peticion: PeticionEdicion,
        _respuesta: RespuestaArchivos,
    ) {
        match peticion {}
    }

    /// La cola cambió: las subidas de ediciones que terminan.
    pub(super) fn cola_de_ediciones(&mut self, _lista: &[InfoTransferencia]) {}

    /// El servidor cayó: ninguna edición en subida borra su temporal.
    pub(super) fn ediciones_servidor_caido(&mut self) {}

    pub(super) fn tecla_dialogo_edicion(&mut self, dialogo: DialogoEdicion, _tecla: KeyEvent) {
        match dialogo {}
    }

    pub(super) fn ejecutar_accion_edicion(&mut self, accion: AccionEdicion) {
        match accion {}
    }

    /// Suspende la TUI y abre el editor (desde `ejecutar`, con el terminal a
    /// mano). La vuelta aplica el tamaño real (T46) y sigue el ciclo de la
    /// edición en `editor_terminado`.
    pub(super) fn ver_en_editor(
        &mut self,
        terminal: &mut crate::ui::TerminalMagi,
        peticion: PeticionEditor,
    ) -> anyhow::Result<()> {
        // El hilo de teclas deja el tty antes de que el editor lo tome (T52).
        self.teclado
            .pausar_y_esperar(std::time::Duration::from_millis(200));
        let aviso = crate::ui::pager::suspender_y_editar(terminal, &self.config, &peticion.ruta);
        self.teclado.reanudar();
        let real = terminal
            .size()
            .map(|area| (area.width, area.height))
            .unwrap_or(self.geometria.aplicado());
        self.volver_de_suspension(terminal, real)?;
        self.editor_terminado(peticion, aviso);
        Ok(())
    }

    /// El editor volvió (o no se pudo lanzar, con `aviso`). Separado de la
    /// suspensión para poder probarlo sin terminal.
    #[doc(hidden)]
    pub fn editor_terminado(&mut self, _peticion: PeticionEditor, aviso: Option<String>) {
        if let Some(aviso) = aviso {
            self.mensaje(aviso, true);
        }
    }
}
