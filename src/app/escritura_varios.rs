//! Escritura en varios paneles (Fase 15).
//!
//! Con dos o más paneles a la vista, `prefijo E` escribe lo que se teclea (y
//! lo que se pega) en todos los paneles concedidos. Si hay más de un host o
//! alguna comprobación activa, delibera MAGI (`PlanDeliberado::EscrituraVarios`);
//! si no, pide confirmación. El servidor concede (`IniciarEscrituraVarios`) y
//! solo escribe en lo concedido. Se apaga con `prefijo E`, al cambiar de
//! disposición o de pestaña, si cae un panel, al salir de la vista, a los
//! 10 min sin escribir y al cerrar la ventana.

use std::time::{Duration, Instant};

/// Segundos sin escribir a partir de los que se apaga sola (cliente y
/// servidor).
pub const INACTIVIDAD: Duration = Duration::from_secs(10 * 60);

/// Por qué terminó la escritura en varios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoFin {
    Apagada,
    Disposicion,
    PestanaCambiada,
    PanelCaido,
    SalidaDeLaVista,
    Inactividad,
    VentanaCerrada,
}

impl MotivoFin {
    /// Texto para REGISTRO y para la barra.
    #[must_use]
    pub fn texto(self) -> &'static str {
        match self {
            MotivoFin::Apagada => "apagada",
            MotivoFin::Disposicion => "cambio de disposición",
            MotivoFin::PestanaCambiada => "cambio de pestaña",
            MotivoFin::PanelCaido => "panel caído",
            MotivoFin::SalidaDeLaVista => "salida de la vista",
            MotivoFin::Inactividad => "10 min sin escribir",
            MotivoFin::VentanaCerrada => "ventana cerrada",
        }
    }
}

/// Lo que devuelve la escritura al apagarse: concesión, motivo y resumen
/// (segundos y envíos).
pub type Fin = (u32, MotivoFin, Option<(u64, u64)>);

/// Lo que hace falta para pedir la escritura en varios: las pestañas (por su
/// sesión) y los paneles que las enseñan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peticion {
    pub sesiones: Vec<u32>,
    pub hosts: Vec<(i64, String)>,
    pub paneles: Vec<usize>,
}

/// Estado de la escritura en varios en esta ventana.
#[derive(Debug, Clone)]
pub enum EstadoEscritura {
    Apagada,
    /// Confirmación simple (un host sin comprobaciones activas).
    Confirmando(Peticion),
    /// Pedida al servidor, esperando la concesión.
    Pidiendo {
        peticion_id: u64,
        peticion: Peticion,
    },
    Activa {
        id: u32,
        peticion: Peticion,
        inicio: Instant,
        envios: u64,
        ultimo: Instant,
    },
}

/// La escritura en varios de la ventana.
#[derive(Debug, Clone)]
pub struct EscrituraVarios {
    pub estado: EstadoEscritura,
}

impl Default for EscrituraVarios {
    fn default() -> Self {
        Self {
            estado: EstadoEscritura::Apagada,
        }
    }
}

impl EscrituraVarios {
    /// ¿Está activa?
    #[must_use]
    pub fn activa(&self) -> bool {
        matches!(self.estado, EstadoEscritura::Activa { .. })
    }

    /// Las sesiones concedidas, si está activa.
    #[must_use]
    pub fn sesiones(&self) -> Vec<u32> {
        match &self.estado {
            EstadoEscritura::Activa { peticion, .. } => peticion.sesiones.clone(),
            _ => Vec::new(),
        }
    }

    /// Los paneles que escriben, si está activa.
    #[must_use]
    pub fn paneles(&self) -> Vec<usize> {
        match &self.estado {
            EstadoEscritura::Activa { peticion, .. } => peticion.paneles.clone(),
            _ => Vec::new(),
        }
    }

    /// El id de la concesión.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        match &self.estado {
            EstadoEscritura::Activa { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// ¿Escribe en esta pestaña?
    #[must_use]
    pub fn incluye(&self, sesion_id: u32) -> bool {
        self.sesiones().contains(&sesion_id)
    }

    /// Marca la concesión como activa.
    pub fn activar(&mut self, id: u32, peticion: Peticion) {
        let ahora = Instant::now();
        self.estado = EstadoEscritura::Activa {
            id,
            peticion,
            inicio: ahora,
            envios: 0,
            ultimo: ahora,
        };
    }

    /// Apunta un envío: cuenta para la inactividad y para el REGISTRO.
    pub fn enviado(&mut self) {
        if let EstadoEscritura::Activa { envios, ultimo, .. } = &mut self.estado {
            *envios += 1;
            *ultimo = Instant::now();
        }
    }

    /// ¿Se ha pasado de inactividad?
    #[must_use]
    pub fn inactiva(&self, ahora: Instant) -> bool {
        match &self.estado {
            EstadoEscritura::Activa { ultimo, .. } => ahora.duration_since(*ultimo) >= INACTIVIDAD,
            _ => false,
        }
    }

    /// Duración y envíos, para el REGISTRO de terminación.
    #[must_use]
    pub fn resumen(&self) -> Option<(u64, u64)> {
        match &self.estado {
            EstadoEscritura::Activa {
                inicio,
                envios,
                ultimo,
                ..
            } => Some((ultimo.duration_since(*inicio).as_secs(), *envios)),
            _ => None,
        }
    }

    /// Apaga la escritura y devuelve la concesión que hay que terminar en el
    /// servidor, si estaba activa.
    pub fn apagar(&mut self) -> Option<Fin> {
        let resumen = self.resumen();
        let anterior = std::mem::replace(&mut self.estado, EstadoEscritura::Apagada);
        match anterior {
            EstadoEscritura::Activa { id, .. } => Some((id, MotivoFin::Apagada, resumen)),
            _ => None,
        }
    }

    /// Termina por un motivo concreto (disposición, panel caído…).
    pub fn terminar(&mut self, motivo: MotivoFin) -> Option<Fin> {
        let resumen = self.resumen();
        let anterior = std::mem::replace(&mut self.estado, EstadoEscritura::Apagada);
        match anterior {
            EstadoEscritura::Activa { id, .. } => Some((id, motivo, resumen)),
            _ => None,
        }
    }

    /// deja el estado en apagada sin nada que enviar (p. ej. un rechazo).
    pub fn cancelar(&mut self) {
        self.estado = EstadoEscritura::Apagada;
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn peticion() -> Peticion {
        Peticion {
            sesiones: vec![1, 2],
            hosts: vec![(10, "web-01".to_string()), (11, "db-01".to_string())],
            paneles: vec![0, 1],
        }
    }

    #[test]
    fn se_activa_cuenta_envios_y_se_apaga_con_su_motivo() {
        let mut escritura = EscrituraVarios::default();
        assert!(!escritura.activa());
        escritura.activar(7, peticion());
        assert!(escritura.activa());
        assert_eq!(escritura.id(), Some(7));
        assert!(escritura.incluye(2));
        assert!(!escritura.incluye(3));
        escritura.enviado();
        escritura.enviado();
        assert_eq!(escritura.resumen().unwrap().1, 2);
        let (id, motivo, resumen) = escritura.terminar(MotivoFin::Disposicion).unwrap();
        assert_eq!(id, 7);
        assert_eq!(motivo, MotivoFin::Disposicion);
        assert_eq!(resumen.unwrap().1, 2);
        assert!(!escritura.activa());
    }

    #[test]
    fn la_inactividad_es_de_diez_minutos() {
        let mut escritura = EscrituraVarios::default();
        escritura.activar(1, peticion());
        let ahora = Instant::now();
        assert!(!escritura.inactiva(ahora));
        assert!(escritura.inactiva(ahora + INACTIVIDAD + Duration::from_secs(1)));
    }

    #[test]
    fn apagar_una_escritura_ya_apagada_no_hace_nada() {
        let mut escritura = EscrituraVarios::default();
        assert!(escritura.apagar().is_none());
        escritura.activar(3, peticion());
        assert!(escritura.apagar().is_some());
        assert!(escritura.apagar().is_none());
    }
}
