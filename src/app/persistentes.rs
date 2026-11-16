//! Sesiones persistentes (Fase 16): el estado del cliente, el bloque
//! «Persistentes» de la vista Sesiones, la vista previa y los diálogos de las
//! sesiones (nueva, ficha, renombrar, terminar, borrar y limpiar).
//!
//! **CONTRATO-S3-A.** Este módulo nace como esqueleto compilable al fijar el
//! contrato del Sprint 3: la pista A lo rellena. Lo que ya está fijado y no
//! debe cambiar de firma sin avisar a las demás pistas:
//!
//! - `EstadoPersistentes.lista`: la última `ListaPersistentes` que difundió el
//!   servidor (la guarda `evento_persistentes`; la pista B la lee para el aviso
//!   de la barra global y la pestaña soltada).
//! - `App::persistentes_con_atencion`: las sesiones que piden atención ahora.
//! - `App::pestana_persistente_abierta`: la pista B la llama al ver una pestaña
//!   persistente nueva (la pista A guarda la ficha pendiente o, en un host
//!   marcado, la ficha con `origen = host`).
//! - `App::persistentes_apertura_fallida`: la pista B la llama desde
//!   `apertura_tmux_fallida`; si devuelve `true`, la pista A ya enseñó su
//!   diálogo (nombre ocupado: engancharse a la existente o cambiar el nombre).
//!
//! El estado de una sesión (viva, clientes, equipo, actividad, atención,
//! título) y su contenido (vista previa) viven solo en memoria: nunca van a
//! disco, al log ni a REGISTRO.

use std::time::Instant;

use crossterm::event::KeyEvent;

use crate::app::App;
use crate::protocolo::{self, ListaPersistentes, MensajeServidor};

/// Estado del cliente de las sesiones persistentes.
#[derive(Default)]
pub struct EstadoPersistentes {
    /// Última lista que difundió el servidor (lista completa, T27).
    pub lista: Option<ListaPersistentes>,
    /// Cuándo llegó esa lista (reloj del cliente).
    pub recibida_en: Option<Instant>,
}

/// Formulario «Nueva sesión persistente» (host, nombre, carpeta, comando,
/// comando de reanudación y nota). CONTRATO-S3-A: lo define la pista A.
pub struct FormularioNueva {}

/// Diálogos de las sesiones persistentes (viven en `App.dialogo`).
pub enum DialogoPersistentes {
    /// «Nueva sesión persistente» (CONTRATO-S3-A: la pista A añade el resto).
    Nueva(FormularioNueva),
}

impl DialogoPersistentes {
    /// Pega texto en el campo con el foco; `false` si no admite texto.
    pub fn pegar_texto(&mut self, _texto: &str) -> bool {
        false
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

    /// Tecla para un diálogo de las sesiones persistentes (CONTRATO-S3-A).
    pub(crate) fn tecla_dialogo_persistentes(
        &mut self,
        dialogo: DialogoPersistentes,
        _tecla: KeyEvent,
    ) {
        self.dialogo = Some(crate::app::Dialogo::Persistentes(dialogo));
    }

    /// Ejecuta una acción confirmada (CONTRATO-S3-A).
    pub(crate) fn ejecutar_accion_persistentes(&mut self, _accion: AccionPersistentes) {}

    /// Una entrada de la paleta (CONTRATO-S3-A).
    pub(crate) fn accion_paleta_persistentes(&mut self, _accion: AccionPaletaPersistentes) {}

    /// Una pestaña persistente acaba de aparecer en `Sesiones{lista}`: si hay
    /// una creación pendiente de ese host y nombre, se guarda la ficha y se
    /// anota `sesion_persistente_creada`; en un host marcado sin ficha, se crea
    /// con `origen = host` (CONTRATO-S3-A; la llama la pista B).
    #[allow(dead_code)] // CONTRATO-S3-B: la llama la pista B desde `reconciliar_sesiones`.
    pub(crate) fn pestana_persistente_abierta(&mut self, _host_id: i64, _nombre: &str) {}

    /// El servidor no pudo abrir la pestaña de tmux. Devuelve `true` si esta
    /// pista enseñó su diálogo (nombre ocupado) y la pista B no debe mostrar
    /// el error genérico (CONTRATO-S3-A).
    pub(crate) fn persistentes_apertura_fallida(
        &mut self,
        _host_id: i64,
        _nombre: &str,
        _causa: protocolo::CausaAperturaTmux,
    ) -> bool {
        false
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
