//! Sesiones persistentes (Fase 16): el estado del cliente, el bloque
//! «Persistentes» de la vista Sesiones, la vista previa y los diálogos de las
//! sesiones (nueva, ficha, renombrar, terminar, borrar y limpiar).
//!
//! **CONTRATO-S3-A1.** Este módulo nace como esqueleto compilable al fijar el
//! contrato del Sprint 3: la pista A1 (lista, vista previa y acciones) lo rellena. Lo que ya está fijado y no
//! debe cambiar de firma sin avisar a las demás pistas:
//!
//! - `EstadoPersistentes.lista`: la última `ListaPersistentes` que difundió el
//!   servidor (la guarda `evento_persistentes`; la pista B la lee para el aviso
//!   de la barra global y la pestaña soltada).
//! - `App::persistentes_con_atencion`: las sesiones que piden atención ahora.
//! - `App::pestana_persistente_abierta` y `App::persistentes_apertura_fallida`
//!   (en `persistentes_nueva.rs`, pista A2): la pista B las llama al ver una
//!   pestaña persistente nueva y al fallar una apertura de tmux.
//! - `App::abrir_dialogo_nueva_sesion` y `App::reabrir_ficha` (pista A2): las
//!   llaman la pista A1 (teclas `n` y `↵` sobre una terminada), la paleta, la
//!   pista B (`prefijo N`) y las vistas Hosts y Flota.
//!
//! El estado de una sesión (viva, clientes, equipo, actividad, atención,
//! título) y su contenido (vista previa) viven solo en memoria: nunca van a
//! disco, al log ni a REGISTRO.

use std::time::Instant;

use crossterm::event::KeyEvent;

use crate::app::persistentes_nueva::{CreacionPendiente, FormularioNueva};
use crate::app::App;
use crate::protocolo::{self, ListaPersistentes, MensajeServidor};

/// Estado del cliente de las sesiones persistentes.
#[derive(Default)]
pub struct EstadoPersistentes {
    /// Última lista que difundió el servidor (lista completa, T27).
    pub lista: Option<ListaPersistentes>,
    /// Cuándo llegó esa lista (reloj del cliente).
    pub recibida_en: Option<Instant>,
    /// Sesiones que se pidieron crear y aún no tienen pestaña: la ficha se
    /// guarda solo cuando la sesión ya existe en el host (pista A2).
    pub creaciones: Vec<CreacionPendiente>,
}

/// Diálogos de las sesiones persistentes (viven en `App.dialogo`).
pub enum DialogoPersistentes {
    /// «Nueva sesión persistente» (lo define la pista A2, `persistentes_nueva`).
    Nueva(FormularioNueva),
    // CONTRATO-S3-A1: la pista A1 añade aquí la vista previa, la ficha de la
    // sesión, renombrar y las confirmaciones de terminar, borrar y limpiar.
}

impl DialogoPersistentes {
    /// Pega texto en el campo con el foco; `false` si no admite texto.
    pub fn pegar_texto(&mut self, texto: &str) -> bool {
        match self {
            DialogoPersistentes::Nueva(formulario) => formulario.pegar_texto(texto),
        }
    }
}

/// Acciones confirmadas de las sesiones persistentes (viven en
/// `AccionDialogo::Persistentes`). CONTRATO-S3-A: la pista A las define
/// (terminar, borrar una ficha, borrar las terminadas…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionPersistentes {
    /// Borrar las fichas terminadas de todos los hosts.
    LimpiarTerminadas,
}

/// Entradas de la paleta de las sesiones persistentes («nueva sesión
/// persistente», «engancharse a <nombre>»).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionPaletaPersistentes {
    /// «nueva sesión persistente».
    Nueva,
}

/// Una sesión que pide atención ahora (la campana de tmux, D335).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtencionPersistente {
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
}

impl App {
    /// Mensajes de las sesiones persistentes que llegan del servidor
    /// (`SesionesPersistentes`, `VistaPrevia` y `TmuxRenombrada`). Devuelve si
    /// lo atendió.
    pub(crate) fn evento_persistentes(&mut self, mensaje: &MensajeServidor) -> bool {
        match mensaje {
            MensajeServidor::SesionesPersistentes { lista } => {
                self.persistentes.lista = Some(lista.clone());
                self.persistentes.recibida_en = Some(Instant::now());
                self.sucio = true;
                true
            }
            // CONTRATO-S3-A: la vista previa y el renombrado son de la pista A.
            MensajeServidor::VistaPrevia { .. } | MensajeServidor::TmuxRenombrada { .. } => true,
            _ => false,
        }
    }

    /// Teclas de la vista Sesiones para el bloque «Persistentes». Devuelve si
    /// la tecla era suya (CONTRATO-S3-A).
    pub(crate) fn tecla_persistentes_sesiones(&mut self, _tecla: KeyEvent) -> bool {
        false
    }

    /// Tecla para un diálogo de las sesiones persistentes.
    pub(crate) fn tecla_dialogo_persistentes(
        &mut self,
        dialogo: DialogoPersistentes,
        tecla: KeyEvent,
    ) {
        match dialogo {
            DialogoPersistentes::Nueva(formulario) => {
                self.tecla_dialogo_nueva(formulario, tecla);
            }
        }
    }

    /// Ejecuta una acción confirmada (CONTRATO-S3-A).
    pub(crate) fn ejecutar_accion_persistentes(&mut self, _accion: AccionPersistentes) {}

    /// Una entrada de la paleta (CONTRATO-S3-A1).
    pub(crate) fn accion_paleta_persistentes(&mut self, accion: AccionPaletaPersistentes) {
        match accion {
            AccionPaletaPersistentes::Nueva => self.abrir_dialogo_nueva_sesion(None),
        }
    }

    /// Las sesiones que piden atención según la última lista del servidor, en
    /// el orden de la lista: las fichas vivas con la campana y las sesiones sin
    /// ficha del tmux propio de los hosts comprobados. Las del tmux habitual no
    /// cuentan (CONTRATO-S3: lo usa el aviso de la barra global de la pista B).
    #[must_use]
    pub fn persistentes_con_atencion(&self) -> Vec<AtencionPersistente> {
        let Some(lista) = &self.persistentes.lista else {
            return Vec::new();
        };
        let host_nombre = |host_id: i64| {
            self.hosts
                .iter()
                .find(|host| host.id == host_id)
                .map(|host| host.nombre.clone())
                .unwrap_or_else(|| format!("host {host_id}"))
        };
        let mut atencion = Vec::new();
        for ficha in &lista.fichas {
            if ficha.sesion.as_ref().is_some_and(|sesion| sesion.campana) {
                atencion.push(AtencionPersistente {
                    host_id: ficha.host_id,
                    host_nombre: host_nombre(ficha.host_id),
                    nombre: ficha.nombre.clone(),
                });
            }
        }
        for host in &lista.hosts {
            for sesion in &host.sin_ficha {
                if sesion.campana && sesion.servidor == protocolo::ServidorTmux::Magi {
                    atencion.push(AtencionPersistente {
                        host_id: host.host_id,
                        host_nombre: host_nombre(host.host_id),
                        nombre: sesion.nombre.clone(),
                    });
                }
            }
        }
        atencion
    }
}
