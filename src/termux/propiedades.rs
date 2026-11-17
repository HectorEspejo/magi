//! Propiedades de Android (modelo, versión y SDK) con `__system_property_get`
//! de bionic, sin lanzar `getprop`. Fuera de Android no hay propiedades, salvo
//! las que inyectan las pruebas.

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

    /// Una propiedad, si existe y no está vacía. Con propiedades fijas, solo
    /// las fijas; si no, en Android la del sistema y en otro sitio, ninguna.
    pub fn leer(&self, nombre: &str) -> Option<String> {
        let valor = match &self.fijas {
            Some(fijas) => fijas.get(nombre).cloned(),
            None => leer_del_sistema(nombre),
        }?;
        let valor = valor.trim();
        (!valor.is_empty()).then(|| valor.to_string())
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

/// Una propiedad del sistema con `__system_property_get` de bionic. Un valor
/// vacío (o una propiedad que no existe) es `None`. Sin lanzar ningún
/// programa.
#[cfg(target_os = "android")]
fn leer_del_sistema(nombre: &str) -> Option<String> {
    use nix::libc::{__system_property_get, c_char, PROP_VALUE_MAX};

    let nombre = std::ffi::CString::new(nombre).ok()?;
    // El valor, con su NUL final, cabe siempre en PROP_VALUE_MAX bytes.
    let mut valor = [0u8; PROP_VALUE_MAX as usize];
    // SAFETY: `nombre` es una cadena C válida y `valor` tiene PROP_VALUE_MAX
    // bytes, que es lo que la función promete no sobrepasar (incluido el NUL).
    let largo =
        unsafe { __system_property_get(nombre.as_ptr(), valor.as_mut_ptr().cast::<c_char>()) };
    // Devuelve la longitud del valor; 0 si no existe o está vacío.
    let largo = usize::try_from(largo).ok().filter(|largo| *largo > 0)?;
    let valor = &valor[..largo.min(valor.len() - 1)];
    let texto = String::from_utf8_lossy(valor);
    let texto = texto.trim();
    (!texto.is_empty()).then(|| texto.to_string())
}

/// Fuera de Android no hay propiedades del sistema.
#[cfg(not(target_os = "android"))]
fn leer_del_sistema(nombre: &str) -> Option<String> {
    let _ = nombre;
    None
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_fijas_se_leen_y_las_vacias_no_existen() {
        let propiedades = Propiedades::fijas(&[
            ("ro.product.model", "ABC-X10"),
            ("ro.build.version.release", "15"),
            ("ro.build.version.sdk", " 35 "),
            ("ro.vacia", "  "),
        ]);
        assert_eq!(propiedades.modelo().as_deref(), Some("ABC-X10"));
        assert_eq!(propiedades.version_android().as_deref(), Some("15"));
        assert_eq!(propiedades.sdk(), Some(35));
        assert_eq!(propiedades.leer("ro.vacia"), None);
        assert_eq!(propiedades.leer("ro.no.existe"), None);
    }

    #[test]
    fn un_sdk_que_no_es_numero_no_es_nada() {
        let propiedades = Propiedades::fijas(&[("ro.build.version.sdk", "treinta")]);
        assert_eq!(propiedades.sdk(), None);
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn fuera_de_android_no_hay_propiedades_del_sistema() {
        let propiedades = Propiedades::del_sistema();
        assert_eq!(propiedades.modelo(), None);
        assert_eq!(propiedades.sdk(), None);
    }
}
