//! Núcleo compartido de MAGI: lo que el escritorio y el móvil deben hacer
//! igual (almacén, sincronización, conexión SSH, túneles, terminal y registro).
//!
//! El núcleo no lee el entorno ni calcula rutas: recibe las rutas y la
//! identidad del equipo al arrancar, y los servicios que dependen de la
//! plataforma (carpeta compartida, llavero, firmante y trabajo vivo) le llegan
//! inyectados.

pub mod almacen;
pub mod claves;
pub mod conexion;
pub mod configuracion;
pub mod deliberacion;
pub mod escritorio;
pub mod ficheros;
pub mod instalar_clave;
pub mod llavero;
pub mod modelo;
pub mod registro;
pub mod rutas;
pub mod servicios;
pub mod sincronizacion;
pub mod snippets;
pub mod terminal;
pub mod trabajo_vivo;
pub mod tuneles;

/// Versión del núcleo. La app la enseña en «Acerca de» junto a la suya.
pub const VERSION_NUCLEO: &str = env!("CARGO_PKG_VERSION");
