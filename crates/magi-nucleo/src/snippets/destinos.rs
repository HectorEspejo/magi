//! Resolución de destinos de un snippet (F6 §7.1) y cuándo exige
//! deliberación: los hosts que salen de sus etiquetas y hosts sueltos, sin
//! repetir, y los motivos por los que hay que deliberar (crítico, más de un
//! host o hosts con verificaciones activas).
//!
//! Las funciones viven en `snippets::mod` y se reexportan desde aquí para
//! que el móvil y el escritorio compartan el mismo punto de entrada.

pub use super::{
    apunta_a, escritorios_omitidos, motivos_deliberacion, resolver, resumen_destino,
    MotivoDeliberacion,
};
