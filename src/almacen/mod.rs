use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

pub mod etiquetas;
pub mod grupos;
pub mod hosts;
pub mod migraciones;

/// Fichero SQLite del inventario.
pub struct Almacen {
    conexion: Connection,
    ruta: Option<PathBuf>,
}

impl Almacen {
    pub fn abrir(ruta: &Path) -> Result<Self> {
        if let Some(directorio) = ruta.parent() {
            crear_directorio_privado(directorio)?;
        }
        let conexion =
            Connection::open(ruta).with_context(|| format!("abriendo {}", ruta.display()))?;
        fs::set_permissions(ruta, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("ajustando permisos de {}", ruta.display()))?;
        let mut almacen = Self {
            conexion,
            ruta: Some(ruta.to_path_buf()),
        };
        almacen.ajustes(true)?;
        migraciones::aplicar(&mut almacen.conexion)?;
        Ok(almacen)
    }

    /// Almacén en memoria para tests.
    pub fn abrir_en_memoria() -> Result<Self> {
        let conexion = Connection::open_in_memory().context("abriendo SQLite en memoria")?;
        let mut almacen = Self {
            conexion,
            ruta: None,
        };
        almacen.ajustes(false)?;
        migraciones::aplicar(&mut almacen.conexion)?;
        Ok(almacen)
    }

    fn ajustes(&mut self, en_fichero: bool) -> Result<()> {
        if en_fichero {
            self.conexion
                .pragma_update(None, "journal_mode", "WAL")
                .context("activando WAL")?;
        }
        self.conexion
            .pragma_update(None, "foreign_keys", "ON")
            .context("activando foreign_keys")?;
        self.conexion
            .pragma_update(None, "busy_timeout", 5000)
            .context("configurando busy_timeout")?;
        Ok(())
    }

    pub fn conexion(&self) -> &Connection {
        &self.conexion
    }

    /// Checkpoint de WAL al salir.
    pub fn cerrar(self) -> Result<()> {
        self.conexion
            .pragma_update(None, "wal_checkpoint", "TRUNCATE")
            .context("checkpoint de WAL")?;
        Ok(())
    }

    pub fn ruta(&self) -> Option<&Path> {
        self.ruta.as_deref()
    }

    // Hosts -----------------------------------------------------------------

    pub fn listar_hosts(&self) -> Result<Vec<crate::modelo::Host>> {
        hosts::listar(&self.conexion)
    }

    pub fn obtener_host(&self, id: i64) -> Result<crate::modelo::Host> {
        hosts::obtener(&self.conexion, id)
    }

    pub fn crear_host(
        &self,
        datos: &crate::modelo::DatosHost,
        origen: crate::modelo::Origen,
    ) -> Result<i64> {
        hosts::crear(&self.conexion, datos, origen)
    }

    pub fn actualizar_host(&self, id: i64, datos: &crate::modelo::DatosHost) -> Result<()> {
        hosts::actualizar(&self.conexion, id, datos)
    }

    pub fn borrar_host(&self, id: i64) -> Result<()> {
        hosts::borrar(&self.conexion, id)
    }

    pub fn existe_nombre_host(&self, nombre: &str, excluyendo: Option<i64>) -> Result<bool> {
        hosts::existe_nombre(&self.conexion, nombre, excluyendo)
    }

    pub fn dependientes_de_salto(&self, id: i64) -> Result<i64> {
        hosts::dependientes_de_salto(&self.conexion, id)
    }

    pub fn mover_host_a_grupo(&self, id: i64, grupo_id: Option<i64>) -> Result<()> {
        hosts::mover_a_grupo(&self.conexion, id, grupo_id)
    }

    pub fn marcar_conexion(&self, id: i64) -> Result<()> {
        hosts::marcar_conexion(&self.conexion, id)
    }

    pub fn marcar_estado(
        &self,
        id: i64,
        estado: Option<crate::modelo::UltimoEstado>,
    ) -> Result<()> {
        hosts::marcar_estado(&self.conexion, id, estado)
    }

    // Grupos ----------------------------------------------------------------

    pub fn listar_grupos(&self) -> Result<Vec<crate::modelo::Grupo>> {
        grupos::listar(&self.conexion)
    }

    pub fn crear_grupo(&self, nombre: &str) -> Result<i64> {
        grupos::crear(&self.conexion, nombre)
    }

    pub fn renombrar_grupo(&self, id: i64, nombre: &str) -> Result<()> {
        grupos::renombrar(&self.conexion, id, nombre)
    }

    pub fn borrar_grupo(&self, id: i64) -> Result<()> {
        grupos::borrar(&self.conexion, id)
    }

    pub fn alternar_plegado(&self, id: i64, plegado: bool) -> Result<()> {
        grupos::alternar_plegado(&self.conexion, id, plegado)
    }

    pub fn mover_grupo(&self, id: i64, direccion: i8) -> Result<()> {
        grupos::mover(&self.conexion, id, direccion)
    }

    pub fn hosts_en_grupo(&self, id: i64) -> Result<i64> {
        grupos::hosts_en_grupo(&self.conexion, id)
    }

    // Etiquetas -------------------------------------------------------------

    pub fn listar_etiquetas(&self) -> Result<Vec<crate::modelo::Etiqueta>> {
        etiquetas::listar(&self.conexion)
    }
}

fn crear_directorio_privado(ruta: &Path) -> Result<()> {
    if ruta.exists() {
        return Ok(());
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(ruta)
        .with_context(|| format!("creando {}", ruta.display()))?;
    Ok(())
}

/// Detecta una violación de restricción UNIQUE.
pub fn es_conflicto_unico(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                ..
            },
            _
        )
    )
}
