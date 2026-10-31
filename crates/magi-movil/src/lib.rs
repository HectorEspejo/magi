//! Puerta de MAGI al móvil: contrato de UniFFI 0.32.2 que carga la app Android.
//!
//! El crate es una cdylib (libmagi_movil.so) más una biblioteca Rust: la app
//! Kotlin llama a las funciones de §5.1 del informe de la Fase 11 a través de
//! los bindings generados y el núcleo le pide a la plataforma lo que necesita
//! (carpeta SAF, llavero del Keystore, firmante de la clave del móvil y oyente
//! de pantallas, sesiones, túneles, diálogos y sincronización) por retrollamadas.
//!
//! En esta entrega el crate es el CONTRATO compilable: solo version() está
//! implementada. La lógica (almacén, runtime de tokio, hilo escritor, gestor de
//! sesiones, túneles, diálogos y sincronización en modo lector) la integra el
//! Lead sustituyendo los unimplemented! con TODO.

uniffi::setup_scaffolding!();

pub mod api;
pub mod clave;
pub mod dialogos;
pub mod escritor;
pub mod estado;
pub mod gestor;
pub mod inventario;
pub mod retrollamadas;
pub mod sincro;
