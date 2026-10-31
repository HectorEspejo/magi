//! Sincronización del inventario en el escritorio (Fase 10): sobre el núcleo
//! compartido añade la pasada en modo escritor (copia previa de magi.db,
//! confirmación de borrados masivos y exportación del fichero propio), el QR
//! de la clave y las operaciones de equipos que tocan config.toml.

pub use magi_nucleo::sincronizacion::*;

pub mod equipos;
pub mod qr;

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::config::{Config, Rutas};

/// Lo que pide la UI del escritorio para hacer una pasada.
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

/// Una pasada completa en modo escritor.
pub fn pasada(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    claves: &[cifrado::ClaveSincronizacion],
    opciones: &OpcionesPasada<'_>,
) -> Result<ResultadoPasada> {
    let exportar = |conn: &Connection| {
        let _ = crate::sshconfig::exportar::exportar(conn, &rutas.dir_ssh());
    };
    let nucleo = magi_nucleo::sincronizacion::OpcionesPasada {
        politica_borrados: opciones.politica_borrados,
        trabajo_vivo: opciones.trabajo_vivo,
        al_aplicar: if config.exportar_al_guardar {
            Some(&exportar as &dyn Fn(&Connection))
        } else {
            None
        },
    };
    magi_nucleo::sincronizacion::pasada(
        conexion,
        rutas,
        &config.sincronizacion,
        crate::VERSION_APP,
        claves,
        &nucleo,
    )
}

/// Texto de estado de la sincronización para la CLI y la vista.
pub fn estado_texto(conexion: &Connection, config: &Config, hogar: &Path) -> Result<String> {
    magi_nucleo::sincronizacion::estado_texto(conexion, &config.sincronizacion, hogar)
}
