//! Sincronizar directorio (`S`) y sincronizaciones guardadas (`L`) (Fase 8,
//! §4.3): el cliente recorre el árbol local, pide el remoto con `ListarArbol`,
//! aplica las exclusiones, calcula el plan y lo enseña en la vista previa; la
//! ejecución es una `Transferir` plana con `borrar_al_terminar`. Una subida a
//! un host con verificaciones o un plan que borra pasa por la deliberación.
//!
//! El plan y la vista previa viven en la `App`, no en `EstadoArchivos`: una
//! guardada se lanza desde la paleta en cualquier vista.

use crossterm::event::KeyEvent;

use crate::protocolo::InfoTransferencia;

use super::{App, RespuestaArchivos};

/// Diálogos propios (SINCRONIZAR, vista previa, lista de guardadas).
pub enum DialogoSincronizar {}

/// Acciones confirmadas (viajan en `AccionDialogo::Sincronizar`).
pub enum AccionSincronizar {}

/// Peticiones al servidor en vuelo.
#[derive(Debug)]
pub enum PeticionSincronizacion {}

/// Resultados de los hilos locales (recorrido del árbol).
#[derive(Debug)]
pub enum EventoSincronizacion {}

/// Qué sincronizar: lo fijan `S` (desde los paneles), `L` y la paleta (desde
/// una guardada). La planificación no relee nada de esto (T33).
#[derive(Debug, Clone, PartialEq)]
pub struct SolicitudSincronizacion {
    pub host_id: i64,
    pub host_nombre: String,
    /// `(id, nombre)` de la guardada; ninguna si es «ad hoc».
    pub guardada: Option<(i64, String)>,
    pub direccion: crate::protocolo::Direccion,
    pub ruta_local: String,
    pub ruta_remota: String,
    /// Borrar en el destino lo que no está en el origen.
    pub borrar: bool,
    /// Exclusiones extra (además de `[archivos] excluir` y el `.magiignore`).
    pub extras: Vec<String>,
}

/// Planificaciones y sincronizaciones lanzadas por esta ventana.
#[derive(Default)]
pub struct EstadoSincronizar {}

impl App {
    /// `S` en Archivos.
    pub(super) fn abrir_sincronizar(&mut self) {
        self.mensaje("sincronizar: aún no implementado", true);
    }

    /// Planifica una sincronización y abre su vista previa: recorre el árbol
    /// local, pide el remoto, aplica las exclusiones y calcula el plan. Es la
    /// entrada común de `S` (tras su diálogo), `L` y la paleta.
    pub(super) fn planificar_sincronizacion(&mut self, solicitud: SolicitudSincronizacion) {
        let _ = solicitud;
        self.mensaje("planificar: aún no implementado", true);
    }

    pub(super) fn respuesta_sincronizacion(
        &mut self,
        _peticion_id: u64,
        peticion: PeticionSincronizacion,
        _respuesta: RespuestaArchivos,
    ) {
        match peticion {}
    }

    pub(super) fn evento_sincronizacion(&mut self, evento: EventoSincronizacion) {
        match evento {}
    }

    /// La cola cambió: las sincronizaciones lanzadas que terminan.
    pub(super) fn cola_de_sincronizaciones(&mut self, _lista: &[InfoTransferencia]) {}

    /// El servidor cayó: los planes a medias se abandonan.
    pub(super) fn sincronizaciones_servidor_caido(&mut self) {}

    pub(super) fn tecla_dialogo_sincronizar(
        &mut self,
        dialogo: DialogoSincronizar,
        _tecla: KeyEvent,
    ) {
        match dialogo {}
    }

    pub(super) fn ejecutar_accion_sincronizar(&mut self, accion: AccionSincronizar) {
        match accion {}
    }
}
