//! Ajustes de la sincronización que el núcleo necesita, sin config.toml.
//!
//! El escritorio los lee de su config.toml; el móvil no tiene fichero de
//! configuración y usa los valores por defecto de una pasada en modo lector.

use serde::{Deserialize, Serialize};

/// Sección [sincronizacion] de config.toml (Fase 10). Sin carpeta, la
/// sincronización está desactivada.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SeccionSincronizacion {
    /// Carpeta compartida (Syncthing, Nextcloud, iCloud…). Puede ser absoluta o
    /// empezar por ~/.
    pub carpeta: Option<String>,
    /// Segundos entre el último cambio local y la exportación.
    pub retraso_seg: u64,
    /// Copias de magi.db que se conservan antes de importar.
    pub copias: u64,
    /// Más de estos hosts borrados disparan la confirmación de borrados.
    pub umbral_borrado_hosts: u64,
    /// Porcentaje de hosts borrados que dispara la confirmación.
    pub umbral_borrado_pct: u64,
    /// Fecha límite para seguir leyendo con la clave anterior tras una
    /// rotación (30 días como máximo). Fuera de plazo se olvida.
    pub clave_anterior_hasta: Option<String>,
}

impl Default for SeccionSincronizacion {
    fn default() -> Self {
        Self {
            carpeta: None,
            retraso_seg: 5,
            copias: 10,
            umbral_borrado_hosts: 5,
            umbral_borrado_pct: 20,
            clave_anterior_hasta: None,
        }
    }
}

impl SeccionSincronizacion {
    /// ¿Está configurada la sincronización en este equipo?
    pub fn activa(&self) -> bool {
        self.carpeta
            .as_deref()
            .map(|carpeta| !carpeta.trim().is_empty())
            .unwrap_or(false)
    }

    /// Carpeta resuelta contra el home de este equipo.
    pub fn ruta_carpeta(&self, hogar: &std::path::Path) -> Option<std::path::PathBuf> {
        let texto = self.carpeta.as_deref()?.trim();
        if texto.is_empty() {
            return None;
        }
        if texto == "~" {
            return Some(hogar.to_path_buf());
        }
        if let Some(resto) = texto.strip_prefix("~/") {
            return Some(hogar.join(resto));
        }
        Some(std::path::PathBuf::from(texto))
    }
}
