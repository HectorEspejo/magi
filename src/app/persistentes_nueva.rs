//! Sesiones persistentes (Fase 16): el diálogo «Nueva sesión persistente»
//! (host, nombre, carpeta, comando, comando de reanudación y nota), la ficha que
//! solo se guarda cuando la sesión ya existe en el host, reabrir una terminada,
//! el nombre ocupado y la ficha de un host marcado.
//!
//! **CONTRATO-S3-A2.** Esqueleto compilable al fijar el contrato del Sprint 3:
//! la pista A2 lo rellena. Las firmas de abajo las usan otras pistas.

use crossterm::event::KeyEvent;

use crate::app::App;
use crate::protocolo;

/// Una creación pedida y aún sin pestaña: lo que hace falta para guardar la
/// ficha cuando la sesión exista (CONTRATO-S3-A2: la pista A2 define los
/// campos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreacionPendiente {
    pub host_id: i64,
    pub nombre: String,
}

/// Formulario «Nueva sesión persistente» (CONTRATO-S3-A2).
pub struct FormularioNueva {}

impl FormularioNueva {
    /// Pega texto en el campo con el foco; `false` si no admite texto.
    pub fn pegar_texto(&mut self, _texto: &str) -> bool {
        false
    }
}

impl App {
    /// Abre el diálogo «Nueva sesión persistente», con el host propuesto si lo
    /// hay (el de la pestaña activa, el de la fila…). Lo llaman la tecla `n` del
    /// bloque, la paleta, `prefijo N` y las vistas Hosts y Flota
    /// (CONTRATO-S3-A2).
    pub(crate) fn abrir_dialogo_nueva_sesion(&mut self, _host_id: Option<i64>) {}

    /// Tecla para el diálogo «Nueva sesión persistente» (CONTRATO-S3-A2).
    pub(crate) fn tecla_dialogo_nueva(&mut self, formulario: FormularioNueva, _tecla: KeyEvent) {
        self.dialogo = Some(crate::app::Dialogo::Persistentes(
            crate::app::persistentes::DialogoPersistentes::Nueva(formulario),
        ));
    }

    /// Reabre una ficha terminada: la crea con su `comando_reanudar` (o su
    /// `comando`) en su carpeta y anota `sesion_persistente_reabierta`. La
    /// llama la tecla `↵` del bloque sobre una terminada (CONTRATO-S3-A2).
    #[allow(dead_code)] // CONTRATO-S3-A2: la llama la pista A1.
    pub(crate) fn reabrir_ficha(&mut self, _host_id: i64, _nombre: &str) {}

    /// Una pestaña persistente acaba de aparecer en `Sesiones{lista}`: si hay
    /// una creación pendiente de ese host y nombre, se guarda la ficha y se
    /// anota `sesion_persistente_creada`; en un host marcado sin ficha, se crea
    /// con `origen = host`. La llama la pista B desde `reconciliar_sesiones`
    /// (CONTRATO-S3-A2).
    #[allow(dead_code)] // CONTRATO-S3-B: la llama la pista B.
    pub(crate) fn pestana_persistente_abierta(&mut self, _host_id: i64, _nombre: &str) {}

    /// El servidor no pudo abrir la pestaña de tmux. Devuelve `true` si esta
    /// pista enseñó su diálogo (nombre ocupado: engancharse a la existente o
    /// cambiar el nombre) y la pista B no debe mostrar el error genérico
    /// (CONTRATO-S3-A2).
    pub(crate) fn persistentes_apertura_fallida(
        &mut self,
        _host_id: i64,
        _nombre: &str,
        _causa: protocolo::CausaAperturaTmux,
    ) -> bool {
        false
    }
}
