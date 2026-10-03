//! Sincronizaciones guardadas (`L`, Fase 8, §5.5 y §6.5): lista del host con
//! su último resultado, alta, edición y borrado con validación, y las entradas
//! de la paleta (`sync · <host> · <nombre>`, `sincronizar directorio`,
//! `editar fichero`). Ejecutar una guardada es pedir su plan a
//! `planificar_sincronizacion` (módulo `sincronizar`).

use crossterm::event::KeyEvent;

use super::{App, EntradaPaleta};

/// Diálogos propios (lista `L` y formulario de alta/edición).
pub enum DialogoGuardadas {}

/// Acciones confirmadas (viajan en `AccionDialogo::Guardadas`).
pub enum AccionGuardadas {}

/// Entradas de la paleta: `sync · <host> · <nombre>`, `sincronizar
/// directorio` y `editar fichero`.
#[derive(Debug, Clone)]
pub enum AccionPaletaArchivos {
    /// `sync · <host> · <nombre>`: planificar esa guardada (id fijado al
    /// construir la paleta) y abrir su vista previa.
    Sincronizar(i64),
    /// `sincronizar directorio`: lo mismo que `S` en Archivos.
    SincronizarDirectorio,
    /// `editar fichero`: lo mismo que `E` en Archivos.
    EditarFichero,
}

/// La solicitud de planificación de una guardada: sus datos tal como están
/// en `SINCRONIZACIONES_DIR` al pedirla.
pub fn solicitud_de_guardada(
    guardada: &crate::modelo::Sincronizacion,
) -> super::sincronizar::SolicitudSincronizacion {
    super::sincronizar::SolicitudSincronizacion {
        host_id: guardada.host_id,
        host_nombre: guardada.host_nombre.clone(),
        guardada: Some((guardada.id, guardada.nombre.clone())),
        direccion: guardada.direccion,
        ruta_local: guardada.ruta_local.clone(),
        ruta_remota: guardada.ruta_remota.clone(),
        borrar: guardada.borrar,
        extras: guardada.exclusiones.clone(),
    }
}

impl App {
    /// `L` en Archivos.
    pub(super) fn abrir_sincronizaciones(&mut self) {
        self.mensaje("sincronizaciones guardadas: aún no implementado", true);
    }

    pub(super) fn tecla_dialogo_guardadas(&mut self, dialogo: DialogoGuardadas, _tecla: KeyEvent) {
        match dialogo {}
    }

    pub(super) fn ejecutar_accion_guardadas(&mut self, accion: AccionGuardadas) {
        match accion {}
    }

    pub(super) fn entradas_paleta_archivos(&self) -> Vec<EntradaPaleta> {
        Vec::new()
    }

    pub(super) fn accion_paleta_archivos(&mut self, accion: AccionPaletaArchivos) {
        match accion {
            AccionPaletaArchivos::Sincronizar(id) => {
                match self.almacen.obtener_sincronizacion(id) {
                    Ok(guardada) => {
                        self.planificar_sincronizacion(solicitud_de_guardada(&guardada))
                    }
                    Err(_) => self.mensaje("esa sincronización ya no existe", true),
                }
            }
            AccionPaletaArchivos::SincronizarDirectorio => self.abrir_sincronizar(),
            AccionPaletaArchivos::EditarFichero => self.editar_entrada(),
        }
    }
}
