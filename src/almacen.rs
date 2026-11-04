//! Almacén del escritorio: abre magi.db con la identidad de este equipo
//! (hostname y sistema) inyectada en el núcleo.

use std::path::Path;

use anyhow::Result;

pub use magi_nucleo::almacen::*;

use crate::config::{nombre_actual, plataforma_actual};

/// Abre (o crea) la base de datos de este equipo.
pub fn abrir(ruta: &Path) -> Result<Almacen> {
    let equipo = magi_nucleo::rutas::Equipo::nuevo(nombre_actual(), plataforma_actual());
    magi_nucleo::almacen::Almacen::abrir(ruta, &equipo, crate::VERSION_APP)
}

/// Almacén en memoria para las pruebas.
pub fn abrir_en_memoria() -> Result<Almacen> {
    magi_nucleo::almacen::Almacen::abrir_en_memoria()
}
