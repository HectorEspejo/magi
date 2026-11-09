//! Logs del móvil (informe §4.4, §5.1 y §7.4): suscripciones por objetivo
//! (`servicios.sh logs`), anillo en memoria (10 000 líneas y 8 MiB) y ventanas
//! ya filtradas de hasta 500 líneas.
//!
//! Es un esbozo del contrato de la Fase 14: las firmas y los tipos están
//! fijados, pero la lógica la implementa la pista del gestor del móvil.

use std::sync::Arc;

use crate::api::{pendiente, ErrorNucleo, FiltroLogs, ObjetivoMovil, VentanaLogs};
use crate::estado::EstadoNucleo;

/// Abre la suscripción de logs de un objetivo y devuelve su id (informe
/// §5.1); hay una por objetivo y el anillo se queda hasta que se suelta.
pub async fn seguir(
    _estado: &Arc<EstadoNucleo>,
    _host_uuid: &str,
    _objetivo: &ObjetivoMovil,
    _lineas: u32,
) -> Result<u64, ErrorNucleo> {
    Err(pendiente("logs_seguir"))
}

/// Hasta 500 líneas ya filtradas, desde un número de línea o desde el final,
/// con el total, las omitidas y el estado (informe §5.1 y §7.4).
pub fn ventana(
    _estado: &EstadoNucleo,
    _suscripcion: u64,
    _filtro: &FiltroLogs,
) -> Result<VentanaLogs, ErrorNucleo> {
    Err(pendiente("logs_ventana"))
}

/// Cierra el canal (fin de la entrada estándar del script) y deja las líneas
/// en el anillo hasta que se suelte la pantalla.
pub fn parar(_estado: &EstadoNucleo, _suscripcion: u64) -> Result<(), ErrorNucleo> {
    Ok(())
}
