//! Rutas del equipo y datos que lo identifican, inyectados al arrancar.
//!
//! El núcleo no lee el entorno ni calcula rutas por su cuenta: el escritorio
//! las descubre con la crate directories y las pasa; el móvil pasa las suyas
//! dentro de su almacenamiento privado.

use std::path::{Path, PathBuf};

/// Rutas XDG de MAGI (equivalentes en macOS vía directories), o las que la
/// app móvil decida dentro de su almacenamiento privado.
#[derive(Debug, Clone)]
pub struct Rutas {
    pub datos: PathBuf,
    pub config: PathBuf,
    pub estado: PathBuf,
    pub hogar: PathBuf,
    /// Directorio de ejecución del servidor; inyectable para pruebas y vacío
    /// en el móvil, que no tiene servidor de sesiones.
    pub runtime: PathBuf,
}

impl Rutas {
    /// Construye las rutas con los valores ya calculados por la plataforma.
    pub fn nuevas(
        datos: PathBuf,
        config: PathBuf,
        estado: PathBuf,
        hogar: PathBuf,
        runtime: PathBuf,
    ) -> Self {
        Self {
            datos,
            config,
            estado,
            hogar,
            runtime,
        }
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

    /// Directorio donde vive el known_hosts (para leer sus entradas).
    pub fn dir_known_hosts(&self) -> &Path {
        self.hogar.as_path()
    }

    /// Directorio de ejecución del servidor. Debe crearse con permisos 700.
    pub fn dir_runtime(&self) -> PathBuf {
        self.runtime.clone()
    }
}

/// Identidad del equipo en la sincronización: lo que se guarda en la fila de
/// EQUIPOS con es_este = 1. El escritorio pasa el hostname y linux o macos;
/// el móvil, el modelo del teléfono y android.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Equipo {
    pub nombre: String,
    pub plataforma: String,
}

impl Equipo {
    pub fn nuevo(nombre: impl Into<String>, plataforma: impl Into<String>) -> Self {
        Self {
            nombre: nombre.into(),
            plataforma: plataforma.into(),
        }
    }
}
