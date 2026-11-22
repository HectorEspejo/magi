//! Errores de `magi-web`.
//!
//! Todo dato raro (de la API de GitHub, de un `.sha256`, de un CHANGELOG, de un
//! fichero de `web/`) para la construcción con un mensaje que dice qué y dónde:
//! la web publicada sigue siendo la anterior.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Un problema encontrado al comprobar un sitio generado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hallazgo {
    /// Fichero del sitio, relativo a su raíz (o descripción del origen).
    pub fichero: String,
    /// Línea (desde 1), si se conoce.
    pub linea: Option<usize>,
    /// Qué pasa.
    pub que: String,
}

impl Hallazgo {
    pub fn nuevo(fichero: impl Into<String>, linea: Option<usize>, que: impl Into<String>) -> Self {
        Self {
            fichero: fichero.into(),
            linea,
            que: que.into(),
        }
    }
}

impl fmt::Display for Hallazgo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.linea {
            Some(linea) => write!(f, "{}:{}: {}", self.fichero, linea, self.que),
            None => write!(f, "{}: {}", self.fichero, self.que),
        }
    }
}

fn lista(hallazgos: &[Hallazgo]) -> String {
    hallazgos
        .iter()
        .map(|h| format!("  - {h}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Error)]
pub enum ErrorWeb {
    /// Un dato que no cumple lo esperado: `donde` dice de dónde sale y `que` qué le pasa.
    #[error("dato raro en {donde}: {que}")]
    DatoRaro { donde: String, que: String },
    #[error("no se pudo leer {}: {fuente}", ruta.display())]
    Lectura { ruta: PathBuf, fuente: io::Error },
    #[error("no se pudo escribir {}: {fuente}", ruta.display())]
    Escritura { ruta: PathBuf, fuente: io::Error },
    /// Una orden externa (`gh`, `curl`, `git`, `mdbook`) falló o pasó su plazo.
    #[error("la orden «{orden}» falló: {motivo}")]
    Orden { orden: String, motivo: String },
    #[error("plantilla «{nombre}»: {motivo}")]
    Plantilla { nombre: String, motivo: String },
    /// `magi-web comprobar` encontró problemas en el sitio generado.
    #[error("la comprobación encontró {} problema(s):\n{}", .0.len(), lista(.0))]
    Comprobacion(Vec<Hallazgo>),
}

pub type Resultado<T> = Result<T, ErrorWeb>;

impl ErrorWeb {
    pub fn dato_raro(donde: impl Into<String>, que: impl Into<String>) -> Self {
        Self::DatoRaro {
            donde: donde.into(),
            que: que.into(),
        }
    }

    pub fn lectura(ruta: &Path, fuente: io::Error) -> Self {
        Self::Lectura {
            ruta: ruta.to_path_buf(),
            fuente,
        }
    }

    pub fn escritura(ruta: &Path, fuente: io::Error) -> Self {
        Self::Escritura {
            ruta: ruta.to_path_buf(),
            fuente,
        }
    }

    pub fn plantilla(nombre: impl Into<String>, motivo: impl ToString) -> Self {
        Self::Plantilla {
            nombre: nombre.into(),
            motivo: motivo.to_string(),
        }
    }
}
