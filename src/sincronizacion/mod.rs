//! Sincronización cifrada del inventario entre equipos (Fase 10).
//!
//! Una pasada lee los ficheros magi-<uuid>.age de la carpeta elegida,
//! descifra y comprueba su firma, fusiona el inventario por versiones y
//! vectores, aplica el resultado en una transacción (con copia previa de
//! magi.db), importa las huellas y exporta el fichero de este equipo si su
//! contenido cambió. Un cerrojo garantiza que solo un proceso sincroniza a la
//! vez.
//!
//! El formato completo está en docs/formato-sincronizacion.md.

pub mod aplicacion;
pub mod carpeta;
pub mod cifrado;
pub mod elementos;
pub mod equipos;
pub mod formato;
pub mod fusion;
pub mod huellas;
pub mod reloj;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use rusqlite::Connection;

use crate::config::{Rutas, SeccionSincronizacion};
use cifrado::ClaveSincronizacion;

/// Qué hacer cuando una importación borra más hosts de los umbrales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoliticaBorrados {
    /// Preguntar (TUI) o devolver código 2 (CLI): no se aplica nada.
    Preguntar,
    /// Aplicar los borrados (el usuario ha aceptado).
    Aplicar,
    /// Mantener los míos: edición nueva de cada host que iba a borrarse.
    Mantener,
}

/// Información de la pasada que necesita saber qué trabajo está vivo: el
/// servidor de sesiones custodia sesiones, túneles, transferencias y
/// ejecuciones. Recibe (tipo de elemento, uuid) y devuelve si hay trabajo vivo.
pub struct OpcionesPasada<'a> {
    pub politica_borrados: PoliticaBorrados,
    pub trabajo_vivo: &'a dyn Fn(&str, &str) -> bool,
}

impl<'a> OpcionesPasada<'a> {
    pub fn sin_trabajo_vivo(politica: PoliticaBorrados) -> Self {
        fn ninguno(_tipo: &str, _uuid: &str) -> bool {
            false
        }
        Self {
            politica_borrados: politica,
            trabajo_vivo: &ninguno,
        }
    }
}

/// Estado final de una pasada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoPasada {
    /// No hay carpeta configurada.
    SinConfigurar,
    /// Hay carpeta pero no hay clave en el llavero (o no está disponible).
    SinClave,
    /// Todo fusionado y exportado; nada pendiente.
    AlDia,
    /// En pausa hasta que el usuario decida sobre un diálogo.
    Pausada,
    /// Un fichero de un equipo conocido no se abre con la clave de este
    /// equipo: hace falta introducir la clave nueva.
    EsperandoClave(String),
    /// Hay que confirmar borrados masivos antes de aplicar nada.
    DecisionPendiente(BorradosPendientes),
    /// Error de carpeta, llavero o fichero.
    Error(String),
}

/// Lo que hace falta para preguntar por los borrados masivos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorradosPendientes {
    /// Copia de magi.db que ya existe antes de aplicar nada.
    pub copia: PathBuf,
    /// Nombres de los hosts que se van a borrar.
    pub hosts: Vec<String>,
    pub total_hosts: usize,
}

/// Resumen de una importación aplicada, para REGISTRO y la vista.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumenImportacion {
    /// Equipos cuyos ficheros se leyeron.
    pub equipos: usize,
    pub creados: usize,
    pub actualizados: usize,
    pub borrados: usize,
    pub conflictos: usize,
    pub huellas: usize,
}

/// Resultado completo de una pasada.
#[derive(Debug, Clone)]
pub struct ResultadoPasada {
    pub estado: EstadoPasada,
    /// Presente si se aplicó una importación.
    pub resumen: Option<ResumenImportacion>,
    /// Si se escribió el fichero propio.
    pub exportado: bool,
    /// Conflictos pendientes después de la pasada.
    pub conflictos_pendientes: i64,
    /// Avisos legibles para la barra o la vista.
    pub avisos: Vec<String>,
}

impl ResultadoPasada {
    pub fn nuevo(estado: EstadoPasada) -> Self {
        Self {
            estado,
            resumen: None,
            exportado: false,
            conflictos_pendientes: 0,
            avisos: Vec::new(),
        }
    }
}

/// Umbral de borrados masivos (§7.5): más de `umbral_n` hosts o al menos
/// `umbral_pct` % de ellos.
pub fn requiere_confirmacion(
    borrados_hosts: usize,
    total_hosts: usize,
    umbral_n: u64,
    umbral_pct: u64,
) -> bool {
    if borrados_hosts == 0 || total_hosts == 0 {
        return false;
    }
    borrados_hosts as u64 > umbral_n
        || (borrados_hosts as u64) * 100 >= umbral_pct.saturating_mul(total_hosts as u64)
}

/// Espera máxima por el cerrojo en la CLI.
pub const ESPERA_CERROJO: Duration = Duration::from_secs(10);

/// Una pasada completa de sincronización. Lee la carpeta, fusiona, aplica y
/// exporta. No toma el cerrojo: lo hace quien llama (la UI sin bloquear, la
/// CLI esperando hasta 10 s).
///
/// Implementación pendiente en la Fase 10 (la hace el equipo de carpeta y
/// ciclo completo).
pub fn pasada(
    _conexion: &Connection,
    _rutas: &Rutas,
    _config: &SeccionSincronizacion,
    _clave: Option<&ClaveSincronizacion>,
    _opciones: &OpcionesPasada<'_>,
) -> Result<ResultadoPasada> {
    anyhow::bail!("la pasada de sincronización de la fase 10 aún no está implementada")
}

/// Texto de estado para la CLI y la barra: una línea por equipo.
///
/// Implementación pendiente en la Fase 10.
pub fn estado_texto(_conexion: &Connection, _config: &SeccionSincronizacion) -> Result<String> {
    anyhow::bail!("el estado de sincronización de la fase 10 aún no está implementado")
}
