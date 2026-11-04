//! Qué trabajo hay vivo en este dispositivo, para aplazar borrados de la
//! sincronización (F10 §7.4). El escritorio pregunta al servidor de sesiones;
//! el móvil, a su gestor de sesiones y túneles.

/// (tipo de elemento, uuid) -> ¿hay trabajo vivo que lo usa?
pub trait TrabajoVivo: Send + Sync {
    fn tiene_trabajo(&self, tipo: &str, uuid: &str) -> bool;
}

/// Implementación para un dispositivo sin trabajo vivo (pruebas y CLI).
#[derive(Debug, Default, Clone, Copy)]
pub struct SinTrabajoVivo;

impl TrabajoVivo for SinTrabajoVivo {
    fn tiene_trabajo(&self, _tipo: &str, _uuid: &str) -> bool {
        false
    }
}
