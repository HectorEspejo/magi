//! `magi-web`: el generador de la web de MAGI (`https://magi.erratia.com`).
//!
//! Biblioteca y binario (`magi-web`), con `publish = false` y `dist = false`: no
//! se distribuye. Hace la portada, las descargas, las versiones y el 404 con
//! plantillas `minijinja`, prepara el manual para mdBook y comprueba el sitio
//! generado. Lo que llega de la API de GitHub, de los `.sha256` y de los CHANGELOG
//! es entrada no fiable: se valida y se escapa.

pub mod anclas;
pub mod capturas;
pub mod comprobar;
pub mod construir;
pub mod contraste;
pub mod csp;
pub mod datos;
pub mod descargas;
pub mod enlaces;
pub mod error;
pub mod fuentes;
pub mod html;
pub mod manual;
pub mod manual_html;
pub mod ordenes;
pub mod paginas;
pub mod plantillas;
pub mod releases;
pub mod rutas;
pub mod textos;
pub mod versiones;

pub use error::{ErrorWeb, Hallazgo, Resultado};
