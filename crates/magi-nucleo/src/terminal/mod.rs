//! Terminal compartido: el parser vt100 vive en la conexión; aquí viven la
//! pantalla por tramos que consume la app móvil y la codificación de teclas.
//!
//! La app nunca recibe bytes del remoto: recibe tramos ya interpretados.

pub mod teclas;
pub mod tramos;

pub use crate::conexion::terminal::{nuevo, redimensionar, Pantalla};
