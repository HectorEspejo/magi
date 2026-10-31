//! Aplicación de un plan de fusión al almacén (Fase 10, §7.3).
//!
//! Escribe las versiones del plan tal cual (nunca por el camino que sube
//! versión), resuelve las referencias por uuid, reconstruye ETIQUETAS, deja
//! lápida de lo borrado y registra los conflictos. Todo dentro de una
//! transacción, bajo el cerrojo de la sincronización y con copia previa de
//! magi.db (la copia la hace la pasada, no esto).

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::sincronizacion::fusion::PlanFusion;

/// Opciones para aplicar un plan de fusión al almacén.
#[derive(Debug, Clone, Default)]
pub struct OpcionesAplicacion {
    /// uuids cuyo borrado se aplaza porque tienen trabajo vivo: no se borran.
    pub borrados_aplazados: BTreeSet<String>,
}

/// Resumen de lo aplicado, para REGISTRO y para la vista.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumenAplicacion {
    pub creados: usize,
    pub actualizados: usize,
    pub borrados: usize,
    pub renombrados: usize,
    pub conflictos: usize,
}

/// Aplica un plan de fusión en una transacción, escribiendo las versiones tal
/// cual vienen del fichero. Devuelve el resumen.
///
/// Implementación pendiente en la Fase 10 (equipo de aplicación).
pub fn aplicar(
    _conexion: &Connection,
    _plan: &PlanFusion,
    _opciones: &OpcionesAplicacion,
    _hogar: &Path,
) -> Result<ResumenAplicacion> {
    anyhow::bail!("aplicar de la fase 10 aún no está implementado")
}

/// Recupera la versión local perdedora de un conflicto como edición nueva
/// (acción r de la vista CONFLICTOS).
///
/// Implementación pendiente en la Fase 10.
pub fn recuperar_version(
    _conexion: &Connection,
    _conflicto: &crate::almacen::conflictos::Conflicto,
    _hogar: &Path,
) -> Result<()> {
    anyhow::bail!("recuperar_version de la fase 10 aún no está implementado")
}

/// Unión por nombre al añadir un equipo (§7.4): cada elemento local con el
/// mismo nombre natural que uno del fichero adopta el uuid del fichero. Las
/// identidades no lo necesitan: su uuid sale de la huella.
///
/// Implementación pendiente en la Fase 10.
pub fn unir_por_nombre(
    _conexion: &Connection,
    _documentos: &[crate::sincronizacion::formato::Documento],
    _hogar: &Path,
) -> Result<usize> {
    anyhow::bail!("la unión por nombre de la fase 10 aún no está implementada")
}
