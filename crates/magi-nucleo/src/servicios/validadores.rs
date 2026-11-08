//! Comprobaciones de configuración de la Fase 13 (§7.5): cuáles conoce MAGI,
//! cuál se aplica a un objetivo y cómo se describe en la confirmación.
//!
//! La orden concreta la compone `servicios.sh`, que tiene el `ExecStart` y los
//! ficheros del proyecto. Aquí se decide qué comprobación toca y qué se enseña.

use super::{Objetivo, TipoObjetivo};

/// Comprobación de configuración que se aplicará antes de recargar o
/// reiniciar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comprobacion {
    /// El objetivo no tiene comprobación: se ejecuta tal cual.
    Ninguna,
    /// La conoce MAGI y la hace `servicios.sh` (Nginx, Caddy, Apache, HAProxy
    /// o Compose).
    Conocida(Conocida),
    /// La escribió el usuario en COMPROBACIONES_SERVICIO.
    Propia(String),
}

/// Comprobaciones que conoce MAGI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conocida {
    Nginx,
    Caddy,
    Apache,
    Haproxy,
    Compose,
}

impl Conocida {
    /// Texto corto de la orden, para la confirmación («nginx -t»).
    pub fn texto(self) -> &'static str {
        match self {
            Conocida::Nginx => "nginx -t",
            Conocida::Caddy => "caddy validate",
            Conocida::Apache => "apachectl configtest",
            Conocida::Haproxy => "haproxy -c",
            Conocida::Compose => "config -q",
        }
    }

    /// Nombre del validador para los mensajes («Nginx», «Compose»…).
    pub fn nombre(self) -> &'static str {
        match self {
            Conocida::Nginx => "Nginx",
            Conocida::Caddy => "Caddy",
            Conocida::Apache => "Apache",
            Conocida::Haproxy => "HAProxy",
            Conocida::Compose => "Compose",
        }
    }
}

/// Comprobación conocida que corresponde a un objetivo, si la hay.
pub fn conocida(objetivo: &Objetivo) -> Option<Conocida> {
    match objetivo.tipo {
        TipoObjetivo::Unidad | TipoObjetivo::Timer => servicios_conocidos()
            .iter()
            .find(|(nombre, _)| *nombre == objetivo.nombre)
            .map(|(_, comprobacion)| *comprobacion),
        TipoObjetivo::Proyecto => Some(Conocida::Compose),
        TipoObjetivo::Contenedor => None,
    }
}

/// Comprobación efectiva de un objetivo: la propia sustituye a la conocida.
pub fn para_objetivo(objetivo: &Objetivo, propia: Option<&str>) -> Comprobacion {
    if !aplica(objetivo.tipo) {
        return Comprobacion::Ninguna;
    }
    if let Some(orden) = propia {
        if !orden.trim().is_empty() {
            return Comprobacion::Propia(orden.to_string());
        }
    }
    match conocida(objetivo) {
        Some(conocida) => Comprobacion::Conocida(conocida),
        None => Comprobacion::Ninguna,
    }
}

/// Texto legible de la comprobación, para la línea de confirmación:
/// «Antes: nginx -t», «Antes: la comprobación propia».
pub fn descripcion(comprobacion: &Comprobacion) -> Option<String> {
    match comprobacion {
        Comprobacion::Ninguna => None,
        Comprobacion::Conocida(conocida) => Some(conocida.texto().to_string()),
        Comprobacion::Propia(_) => Some("la comprobación propia".to_string()),
    }
}

/// ¿La comprobación se aplica a este tipo de objetivo?
pub fn aplica(tipo: TipoObjetivo) -> bool {
    matches!(
        tipo,
        TipoObjetivo::Unidad | TipoObjetivo::Timer | TipoObjetivo::Proyecto
    )
}

/// Comandos conocidos que se usan con privilegios (§7.3).
pub fn requiere_privilegios() -> bool {
    true
}

/// Nombres de servicio que reconoce la comprobación conocida.
pub fn servicios_conocidos() -> &'static [(&'static str, Conocida)] {
    &[
        ("nginx.service", Conocida::Nginx),
        ("caddy.service", Conocida::Caddy),
        ("apache2.service", Conocida::Apache),
        ("httpd.service", Conocida::Apache),
        ("haproxy.service", Conocida::Haproxy),
    ]
}
