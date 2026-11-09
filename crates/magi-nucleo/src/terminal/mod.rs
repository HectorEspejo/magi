//! Terminal del núcleo: lo que comparten el escritorio y el móvil.
//!
//! vt100 lleva la pantalla y el historial; aquí viven lo que no lleva (Fase
//! 15), la codificación de teclas y del ratón, el pegado, la búsqueda y la
//! copia del historial, los enlaces y tmux. El móvil solo usa de momento
//! `teclas` y `tramos`.

pub mod emulador;
pub mod enlaces;
pub mod historial;
pub mod pegado;
pub mod raton;
pub mod teclas;
pub mod tmux;
pub mod tramos;

pub use crate::conexion::terminal::{nuevo, redimensionar, Pantalla};
pub use emulador::PantallaEmulada;
