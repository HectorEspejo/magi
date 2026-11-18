//! Fila de teclas de Termux y fusión de `termux.properties` (§4.8, §7.7).
//!
//! CONTRATO-S3-TECLAS: lo completa la pista de teclas del Sprint 3. Aquí está
//! lo que usa la pista de diagnóstico (`EstadoFila` y `estado_fila`), para que
//! las dos puedan avanzar a la vez; los cuerpos son de relleno.

use std::path::PathBuf;

use super::EntornoTermux;

/// Cómo está la fila de teclas en el `termux.properties` del usuario (línea
/// «teclas» de `magi diagnostico`, §6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoFila {
    /// No hay `termux.properties` (ni `~/.termux/` ni `~/.config/termux/`).
    SinFichero,
    /// Hay fichero pero ninguna línea `extra-keys` efectiva: Termux usa la suya.
    SinExtraKeys,
    /// La línea `extra-keys` efectiva es la de MAGI para el prefijo dado.
    LaDeMagi,
    /// Tiene otra fila (la del usuario): MAGI no la toca sin preguntar.
    Otra,
    /// El fichero no se puede leer (demasiado grande, no UTF-8, permisos): el
    /// motivo, en una línea y sin su contenido.
    Ilegible(String),
}

/// Ruta del `termux.properties` que usa Termux: `~/.config/termux/` si es el
/// que existe y `~/.termux/` en cualquier otro caso (aunque todavía no exista:
/// ahí se crearía). Solo del `EntornoTermux` (`$HOME`), nunca del uid.
#[must_use]
pub fn ruta_properties(entorno: &EntornoTermux) -> Option<PathBuf> {
    let _ = entorno;
    None
}

/// Estado de la fila de teclas para un prefijo de MAGI (el texto de
/// `[prefijo_escape]`, p. ej. «Ctrl+]»). Solo lee: nunca escribe.
#[must_use]
pub fn estado_fila(entorno: &EntornoTermux, prefijo_escape: &str) -> EstadoFila {
    let _ = (entorno, prefijo_escape);
    EstadoFila::SinFichero
}
