//! `magi termux teclas [--instalar]` y `magi termux aviso` (§5.5).
//!
//! CONTRATO-S2-AVISO y CONTRATO-S3-TECLAS: los cuerpos llegan en los Sprints 2
//! y 3; hasta entonces las dos órdenes lo dicen y salen con 1.

use crate::config::Rutas;

/// Lo que pide `magi termux …`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdenTermux {
    /// Enseña la fila propuesta y la actual; con `instalar`, pregunta y escribe.
    Teclas { instalar: bool },
    /// Publica un aviso de prueba y lo retira a los 10 s.
    Aviso,
}

/// Ejecuta la orden y devuelve el código de salida. Fuera de Termux dice
/// «solo en Termux» y sale con 1.
pub fn ejecutar(orden: OrdenTermux, rutas: &Rutas) -> anyhow::Result<i32> {
    let _ = (orden, rutas);
    if !super::es_termux() {
        eprintln!("magi termux: solo en Termux");
        return Ok(1);
    }
    eprintln!("magi termux: todavía sin implementar en esta versión");
    Ok(1)
}
