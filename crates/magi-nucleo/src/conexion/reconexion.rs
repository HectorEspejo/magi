//! Reconexión automática: causas y esperas (Fase 15).
//!
//! La hace el servidor, sin diálogos: solo se reintenta una caída de red, con
//! las esperas de 2, 5, 10 y 30 s y luego cada minuto hasta
//! `reconexion_max_min`. La vuelta de una suspensión del equipo adelanta el
//! siguiente intento.

use std::time::Instant;

/// Esperas entre intentos, en segundos; después, 60 s cada vez.
pub const ESPERAS: [u64; 4] = [2, 5, 10, 30];

/// Espera antes del intento `intento` (desde 1).
#[must_use]
pub fn espera(intento: u32) -> u64 {
    let indice = intento.saturating_sub(1) as usize;
    ESPERAS.get(indice).copied().unwrap_or(60)
}

/// Por qué se cayó la pestaña.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Causa {
    ConexionPerdida,
    PlazoAgotado,
    KeepaliveSinRespuesta,
    ConexionRechazada,
    SinRuta,
    DnsSinRespuesta,
    /// El remoto terminó (`exit` o `exit-signal`): no se reintenta.
    Exit,
    Credenciales,
    HuellaNueva,
    HuellaCambiada,
    FrasePendiente,
    CerradaPorElUsuario,
    HostBorrado,
}

impl Causa {
    /// ¿Merece la pena reintentar sin preguntar?
    #[must_use]
    pub fn recuperable(self) -> bool {
        matches!(
            self,
            Causa::ConexionPerdida
                | Causa::PlazoAgotado
                | Causa::KeepaliveSinRespuesta
                | Causa::ConexionRechazada
                | Causa::SinRuta
                | Causa::DnsSinRespuesta
        )
    }

    /// Texto para la barra y para REGISTRO.
    #[must_use]
    pub fn texto(self) -> &'static str {
        match self {
            Causa::ConexionPerdida => "conexión perdida",
            Causa::PlazoAgotado => "el host no responde",
            Causa::KeepaliveSinRespuesta => "el host dejó de responder al keepalive",
            Causa::ConexionRechazada => "conexión rechazada",
            Causa::SinRuta => "sin ruta al host",
            Causa::DnsSinRespuesta => "el DNS no responde",
            Causa::Exit => "el remoto cerró la sesión",
            Causa::Credenciales => "credenciales rechazadas",
            Causa::HuellaNueva => "huella nueva sin aceptar",
            Causa::HuellaCambiada => "la huella del host cambió",
            Causa::FrasePendiente => "la clave necesita frase",
            Causa::CerradaPorElUsuario => "cerrada desde la ventana",
            Causa::HostBorrado => "el host ya no existe",
        }
    }
}

/// Clasifica el motivo que da una conexión o su caída.
#[must_use]
pub fn clasificar(motivo: &str) -> Causa {
    let texto = motivo.to_lowercase();
    if texto.contains("exit") || texto.contains("cerró") || texto.contains("terminó") {
        return Causa::Exit;
    }
    if texto.contains("huella") {
        return if texto.contains("cambi") {
            Causa::HuellaCambiada
        } else {
            Causa::HuellaNueva
        };
    }
    if texto.contains("frase") {
        return Causa::FrasePendiente;
    }
    if texto.contains("autentic") || texto.contains("credencial") || texto.contains("contraseña") {
        return Causa::Credenciales;
    }
    if texto.contains("rechaz") {
        return Causa::ConexionRechazada;
    }
    if texto.contains("ruta") || texto.contains("unreachable") {
        return Causa::SinRuta;
    }
    if texto.contains("dns") || texto.contains("resolv") || texto.contains("nombre") {
        return Causa::DnsSinRespuesta;
    }
    if texto.contains("plazo") || texto.contains("timeout") || texto.contains("tiempo") {
        return Causa::PlazoAgotado;
    }
    if texto.contains("keepalive") {
        return Causa::KeepaliveSinRespuesta;
    }
    Causa::ConexionPerdida
}

/// Segundos de suspensión a partir de los que se adelanta el intento.
pub const SUSPENSION_SEG: u64 = 30;

/// ¿El equipo volvió de una suspensión? Compara el avance del reloj de pared
/// con el del monotónico (que no avanza suspendido).
#[must_use]
pub fn vuelve_de_suspension(antes: (i64, Instant), ahora: (i64, Instant)) -> bool {
    let pared = ahora.0.saturating_sub(antes.0).max(0) as u64;
    let monotono = ahora.1.saturating_duration_since(antes.1).as_secs();
    pared.saturating_sub(monotono) > SUSPENSION_SEG
}
