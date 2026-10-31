//! Tabla ↔ elemento: qué campos viajan de cada tipo y cómo se exportan y se
//! aplican (Fase 10, §3 y §7.3).
//!
//! Los tipos `Datos*` son el contrato del formato (docs/formato-sincronizacion.md
//! y la app Android). Lo que no aparece aquí es propio del equipo y no viaja.

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::almacen::versiones::Tabla;
use crate::sincronizacion::fusion::Candidato;

/// Datos de un grupo que viajan (sin `plegado`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosGrupo {
    pub nombre: String,
    pub orden: i64,
}

/// Datos de un host que viajan. Las referencias son uuids; las etiquetas, por
/// nombre; las verificaciones van dentro del host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosHost {
    pub nombre: String,
    pub direccion: String,
    pub puerto: u16,
    pub usuario: Option<String>,
    /// Referencia de identidad tal cual (`auto`, `contrasena`, `agente:<huella>`,
    /// `fichero:<ruta>`). Las rutas bajo el home se exportan como ~/.
    pub identidad_ref: Option<String>,
    pub salto: Option<String>,
    pub grupo: Option<String>,
    pub multiplexar: bool,
    pub keepalive_seg: Option<u32>,
    pub opciones_extra: String,
    pub servicios: String,
    pub origen: String,
    pub etiquetas: Vec<String>,
    pub verificaciones: Option<DatosVerificaciones>,
    pub snippet_al_conectar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosVerificaciones {
    pub salud: bool,
    pub backup: bool,
    pub backup_ruta: Option<String>,
    pub backup_patron: Option<String>,
    pub tests: bool,
    pub tests_comando: Option<String>,
}

/// Datos de una identidad que viajan (sin origen, ruta, fechas ni último uso).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosIdentidad {
    pub huella: String,
    pub alias: String,
    pub tipo: String,
    pub comentario: Option<String>,
    pub revocada_en: Option<String>,
}

/// Datos de un túnel que viajan (sin fechas de creación).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosTunel {
    pub host: String,
    pub nombre: String,
    pub tipo: String,
    pub escucha: String,
    pub destino: Option<String>,
    pub automatico: bool,
}

/// Un destino de snippet: exactamente uno de `etiqueta` o `host`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestinoSnippet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etiqueta: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// Datos de un snippet que viajan (sin usos ni fechas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosSnippet {
    pub nombre: String,
    pub comando: String,
    pub descripcion: String,
    pub etiquetas: String,
    pub critico: bool,
    pub timeout_seg: u64,
    pub parar_al_fallo: bool,
    pub destinos: Vec<DestinoSnippet>,
}

/// Datos de una sincronización de directorio que viajan (sin último
/// resultado ni fecha de ejecución).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosSincronizacion {
    pub host: String,
    pub nombre: String,
    pub ruta_local: String,
    pub ruta_remota: String,
    pub direccion: String,
    pub borrar: bool,
    pub exclusiones: String,
}

/// JSON canónico de los datos que viajan de una fila, para comparar si una
/// escritura local debe subir la versión.
pub fn datos_de_fila(_conexion: &Connection, _tabla: Tabla, _id: i64) -> Result<Option<String>> {
    anyhow::bail!("datos_de_fila de la fase 10 aún no está implementado")
}

/// Lee el inventario completo como candidatos (elementos vivos y lápidas).
pub fn estado_local(_conexion: &Connection, _hogar: &Path) -> Result<Vec<Candidato>> {
    anyhow::bail!("estado_local de la fase 10 aún no está implementado")
}

/// Normaliza una ruta para exportarla: si cuelga del home, como ~/....
pub fn ruta_a_home(ruta: &str, hogar: &Path) -> String {
    if let Some(home) = hogar.to_str() {
        if let Some(resto) = ruta.strip_prefix(home) {
            let resto = resto.trim_start_matches('/');
            if resto.is_empty() {
                return "~".to_string();
            }
            return format!("~/{resto}");
        }
    }
    ruta.to_string()
}

/// Expande ~/... con el home de este equipo.
pub fn ruta_desde_home(ruta: &str, hogar: &Path) -> String {
    if ruta == "~" {
        return hogar.to_string_lossy().to_string();
    }
    if let Some(resto) = ruta.strip_prefix("~/") {
        return hogar.join(resto).to_string_lossy().to_string();
    }
    ruta.to_string()
}
