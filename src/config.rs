use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::ficheros::escribir_atomico;

/// Rutas XDG de MAGI (equivalentes en macOS vía `directories`).
#[derive(Debug, Clone)]
pub struct Rutas {
    pub datos: PathBuf,
    pub config: PathBuf,
    pub estado: PathBuf,
    pub hogar: PathBuf,
}

impl Rutas {
    pub fn descubrir() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("org", "4d3", "magi")
            .context("no se pudo determinar el directorio de datos del usuario")?;
        let hogar = directories::BaseDirs::new()
            .context("no se pudo determinar el directorio personal")?
            .home_dir()
            .to_path_buf();
        Ok(Self {
            datos: dirs.data_dir().to_path_buf(),
            config: dirs.config_dir().to_path_buf(),
            estado: dirs
                .state_dir()
                .unwrap_or_else(|| dirs.data_dir())
                .to_path_buf(),
            hogar,
        })
    }

    pub fn base_datos(&self) -> PathBuf {
        self.datos.join("magi.db")
    }

    pub fn fichero_config(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn dir_logs(&self) -> PathBuf {
        self.estado.join("logs")
    }

    pub fn dir_ssh(&self) -> PathBuf {
        self.hogar.join(".ssh")
    }

    pub fn fichero_ssh_config(&self) -> PathBuf {
        self.dir_ssh().join("config")
    }

    pub fn fichero_magi_config(&self) -> PathBuf {
        self.dir_ssh().join("magi_config")
    }

    pub fn fichero_known_hosts(&self) -> PathBuf {
        self.dir_ssh().join("known_hosts")
    }
}

pub const PREFIJO_POR_DEFECTO: &str = "Ctrl+]";

/// Sección `[flota]` de `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SeccionFlota {
    /// Segundos entre auto-refrescos; 0 = desactivado y mínimo 15.
    pub auto_refresco_seg: u64,
    pub umbrales: crate::flota::estado::Umbrales,
}

/// Contenido de `~/.config/magi/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Prefijo de escape de la vista Sesión, p. ej. «Ctrl+]».
    pub prefijo_escape: String,
    /// Regenerar `magi_config` al guardar una ficha.
    pub exportar_al_guardar: bool,
    /// `auto`, `fijo` o la ruta a un tema de Alacritty.
    pub tema: String,
    /// Forzar el modo degradado ASCII.
    pub terminal_ascii: bool,
    /// Sondeo de flota: auto-refresco y umbrales.
    pub flota: SeccionFlota,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefijo_escape: PREFIJO_POR_DEFECTO.to_string(),
            exportar_al_guardar: true,
            tema: "auto".to_string(),
            terminal_ascii: false,
            flota: SeccionFlota::default(),
        }
    }
}

impl Config {
    /// Carga la configuración; si no existe la crea con los valores por
    /// defecto. Devuelve un aviso legible si el fichero no se pudo leer.
    pub fn cargar(ruta: &Path) -> (Self, Option<String>) {
        match fs::read_to_string(ruta) {
            Ok(texto) => match toml::from_str::<Config>(&texto) {
                Ok(config) => (config, None),
                Err(error) => (
                    Config::default(),
                    Some(format!("config.toml no válido: {error}")),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let config = Config::default();
                let aviso = match config.guardar(ruta) {
                    Ok(()) => None,
                    Err(error) => Some(format!("no se pudo crear config.toml: {error}")),
                };
                (config, aviso)
            }
            Err(error) => (
                Config::default(),
                Some(format!("no se pudo leer config.toml: {error}")),
            ),
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<()> {
        let texto = toml::to_string_pretty(self).context("serializando config.toml")?;
        escribir_atomico(ruta, texto.as_bytes(), 0o600)
    }
}
