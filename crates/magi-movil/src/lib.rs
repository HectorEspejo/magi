//! Puerta de MAGI al móvil: contrato de UniFFI 0.32.2 que carga la app Android.
//!
//! El crate es una cdylib (libmagi_movil.so) más una biblioteca Rust: la app
//! Kotlin llama a las funciones de §5.1 del informe de la Fase 11 a través de
//! los bindings generados y el núcleo le pide a la plataforma lo que necesita
//! (carpeta SAF, llavero del Keystore, firmante de la clave del móvil y oyente
//! de pantallas, sesiones, túneles, diálogos y sincronización) por retrollamadas.
//!
//! El crate implementa la lógica completa del móvil: arranque con las rutas y
//! la identidad del equipo inyectadas, hilo escritor único de SQLite, inventario
//! de solo lectura, sincronización en modo lector, sesiones con terminal por
//! tramos, túneles y diálogos con la plataforma.

uniffi::setup_scaffolding!();

pub mod ajustes;
pub mod api;
pub mod clave;
pub mod conexiones;
pub mod deliberacion;
pub mod dialogos;
pub mod ejecuciones;
pub mod escritor;
pub mod estado;
pub mod flota;
pub mod gestor;
pub mod inventario;
pub mod logs;
pub mod registro;
pub mod retrollamadas;
pub mod servicios;
pub mod sincro;
pub mod snippets;
