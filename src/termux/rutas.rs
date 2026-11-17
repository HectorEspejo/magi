//! Rutas de Termux: siempre del entorno, nunca del uid (D383). bionic no sabe
//! dónde está el home de Termux.
//!
//! CONTRATO-S1-TERMUX-RUTAS: el cuerpo lo completa la pista P2 del Sprint 1.

use std::ffi::OsString;
use std::path::PathBuf;

/// Lo que de verdad hace falta del entorno del proceso, inyectado.
#[derive(Debug, Clone, Default)]
pub struct EntornoTermux {
    pub home: Option<PathBuf>,
    /// `$PREFIX` (`/data/data/com.termux/files/usr`).
    pub prefix: Option<PathBuf>,
    pub tmpdir: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
    pub xdg_runtime_dir: Option<PathBuf>,
    /// `$PATH`, para buscar las órdenes `termux-*`.
    pub path: Option<OsString>,
    pub uid: u32,
}

impl EntornoTermux {
    /// El entorno de este proceso. Es el único sitio que lee `std::env`.
    /// CONTRATO-S1-TERMUX-RUTAS.
    pub fn del_proceso() -> Self {
        Self::default()
    }
}

/// Rutas calculadas para Termux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RutasTermux {
    pub datos: PathBuf,
    pub config: PathBuf,
    pub estado: PathBuf,
    pub hogar: PathBuf,
    /// Directorio del socket (700, ruta del socket de menos de 100 bytes).
    pub runtime: PathBuf,
    /// `$PREFIX/bin/sh`: en Termux no existe `/bin/sh`.
    pub sh: PathBuf,
}

/// Calcula las rutas (§7.1): datos, configuración y estado de `XDG_*` o del
/// `$HOME`; el socket en `$XDG_RUNTIME_DIR/magi` o, si falta,
/// `$TMPDIR/magi-<uid>` (con `$PREFIX/tmp` si no hay `$TMPDIR`). Error con
/// instrucción si falta `$HOME` o `$PREFIX`. CONTRATO-S1-TERMUX-RUTAS.
pub fn calcular(entorno: &EntornoTermux) -> Result<RutasTermux, String> {
    let _ = entorno;
    Err("CONTRATO-S1-TERMUX-RUTAS".into())
}
