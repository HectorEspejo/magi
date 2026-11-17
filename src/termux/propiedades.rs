//! Propiedades de Android (modelo, versión y SDK) con `__system_property_get`
//! de bionic, sin lanzar `getprop`. Fuera de Android no hay propiedades, salvo
//! las que inyectan las pruebas.
//!
//! CONTRATO-S1-TERMUX-PROPIEDADES: el cuerpo lo completa la pista P2.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Propiedades {
    fijas: Option<BTreeMap<String, String>>,
}

impl Propiedades {
    /// Las del sistema (en Android, por bionic; en otro sitio, ninguna).
    pub fn del_sistema() -> Self {
        Self { fijas: None }
    }

    /// Unas fijas, para las pruebas.
    pub fn fijas(pares: &[(&str, &str)]) -> Self {
        Self {
            fijas: Some(
                pares
                    .iter()
                    .map(|(nombre, valor)| ((*nombre).to_string(), (*valor).to_string()))
                    .collect(),
            ),
        }
    }

    /// Una propiedad, si existe y no está vacía. CONTRATO-S1-TERMUX-PROPIEDADES.
    pub fn leer(&self, nombre: &str) -> Option<String> {
        self.fijas.as_ref()?.get(nombre).cloned()
    }

    /// `ro.product.model`.
    pub fn modelo(&self) -> Option<String> {
        self.leer("ro.product.model")
    }

    /// `ro.build.version.release` («15»).
    pub fn version_android(&self) -> Option<String> {
        self.leer("ro.build.version.release")
    }

    /// `ro.build.version.sdk` («35»).
    pub fn sdk(&self) -> Option<u32> {
        self.leer("ro.build.version.sdk")?.trim().parse().ok()
    }
}
