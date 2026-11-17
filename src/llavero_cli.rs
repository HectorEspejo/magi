//! `magi llavero estado | cambiar | borrar` y el llavero de las demás órdenes
//! de la CLI (Fase 17, Termux).
//!
//! CONTRATO-S1-LLAVERO-CLI: los cuerpos los completa la pista P4 del Sprint 1.
//! Las subórdenes no van bajo `cfg(target_os)`: fuera de Termux dicen por qué
//! no aplican y salen con 1.

use std::sync::Arc;

use crate::config::Rutas;
use crate::llavero::LlaveroCliente;

/// Lo que pide `magi llavero …`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdenLlavero {
    /// Si hay llavero, su ruta, su tamaño y su fecha; nunca pide la frase.
    Estado,
    /// Pide en el TTY la frase actual y la nueva dos veces, y recifra.
    Cambiar,
    /// Explica qué se pierde y pide escribir `borrar`.
    Borrar,
}

/// Ejecuta la orden y devuelve el código de salida. Fuera de Termux dice «en
/// este equipo el llavero es el del sistema» y sale con 1.
/// CONTRATO-S1-LLAVERO-CLI.
pub fn ejecutar(orden: OrdenLlavero, rutas: &Rutas) -> anyhow::Result<i32> {
    let _ = (orden, rutas);
    if !crate::termux::es_termux() {
        eprintln!("magi llavero: en este equipo el llavero es el del sistema");
        return Ok(1);
    }
    eprintln!("magi llavero: todavía sin implementar en esta versión");
    Ok(1)
}

/// El llavero de una orden de la CLI que lo necesita (`magi sincronizar`,
/// `magi sondear`…): el del equipo y, si es de fichero y está cerrado, pide la
/// frase en el TTY; sin TTY, error con instrucción. Nunca abre diálogos.
/// CONTRATO-S1-LLAVERO-CLI: hoy devuelve el del equipo sin abrirlo.
pub fn abrir_para_cli(rutas: &Rutas) -> anyhow::Result<Arc<dyn LlaveroCliente>> {
    Ok(crate::llavero::del_equipo(rutas))
}
