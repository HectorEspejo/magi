//! Clave del móvil y llavero: adaptadores entre las retrollamadas de Kotlin
//! (Android Keystore) y los servicios que espera el núcleo compartido.

use std::sync::Arc;

use magi_nucleo::claves::x509_a_openssh::x509_a_openssh;
use magi_nucleo::claves::Firmante;
use magi_nucleo::conexion::huellas::huella;
use magi_nucleo::llavero::LlaveroHost;
use magi_nucleo::rutas::Equipo;
use zeroize::Zeroizing;

use crate::api::{ClavePublicaMovil, ErrorNucleo};
use crate::escritor::Escritor;
use crate::retrollamadas::{FirmanteMovil, LlaveroMovil};

/// Cuenta de la clave de sincronización en LlaveroMovil.
pub const CUENTA_CLAVE: &str = "sincronizacion";
/// Cuenta de la clave anterior durante una rotación.
pub const CUENTA_CLAVE_ANTERIOR: &str = "sincronizacion-anterior";

/// Cuenta de la contraseña guardada de un host: por uuid, que no cambia al
/// rehacer el inventario.
pub fn cuenta_contrasena(host_uuid: &str) -> String {
    format!("contrasena:{host_uuid}")
}

/// Llavero de secretos del móvil: adapta LlaveroMovil al trait del núcleo y
/// resuelve las contraseñas de host por uuid.
pub struct LlaveroNucleo {
    movil: Arc<dyn LlaveroMovil>,
    escritor: Escritor,
}

impl LlaveroNucleo {
    pub fn nuevo(movil: Arc<dyn LlaveroMovil>, escritor: Escritor) -> Self {
        Self { movil, escritor }
    }

    /// Lee un secreto. None si no existe.
    pub fn leer(&self, cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        let bytes = self
            .movil
            .leer(cuenta.to_string())
            .map_err(|error| error.to_string())?;
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let texto = String::from_utf8(bytes)
            .map_err(|_| "el secreto guardado no es texto válido".to_string())?;
        Ok(Some(Zeroizing::new(texto)))
    }

    /// Guarda un secreto (los bytes los cifra la app con el Keystore).
    pub fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String> {
        self.movil
            .guardar(cuenta.to_string(), valor.as_bytes().to_vec())
            .map_err(|error| error.to_string())
    }

    /// Borra un secreto; no falla si no existía.
    pub fn borrar(&self, cuenta: &str) -> Result<(), String> {
        self.movil
            .borrar(cuenta.to_string())
            .map_err(|error| error.to_string())
    }

    /// uuid del host por su nombre, para la cuenta de la contraseña.
    fn uuid_de_host(&self, nombre: &str) -> Option<String> {
        let nombre = nombre.to_string();
        self.escritor
            .con_almacen(move |almacen| {
                magi_nucleo::almacen::movil::uuid_por_nombre(almacen.conexion(), &nombre)
                    .ok()
                    .flatten()
            })
            .ok()
            .flatten()
    }

    fn cuenta(&self, host: &str, usuario: &str) -> String {
        match self.uuid_de_host(host) {
            Some(uuid) => cuenta_contrasena(&uuid),
            None => format!("contrasena:{usuario}@{host}"),
        }
    }

    /// ¿Hay contraseña guardada para el host?
    pub fn tiene_contrasena(&self, host_nombre: &str) -> Result<bool, ErrorNucleo> {
        let cuenta = self.cuenta(host_nombre, "");
        self.leer(&cuenta)
            .map(|secreto| secreto.is_some())
            .map_err(|detalle| ErrorNucleo::Llavero { detalle })
    }

    /// Olvida la contraseña de un host; devuelve si había una.
    pub fn olvidar_contrasena(&self, host_uuid: &str) -> Result<bool, ErrorNucleo> {
        let cuenta = cuenta_contrasena(host_uuid);
        let habia = self
            .leer(&cuenta)
            .map_err(|detalle| ErrorNucleo::Llavero { detalle })?
            .is_some();
        self.borrar(&cuenta)
            .map_err(|detalle| ErrorNucleo::Llavero { detalle })?;
        Ok(habia)
    }
}

impl LlaveroHost for LlaveroNucleo {
    fn guardar_contrasena(
        &self,
        host: &str,
        usuario: &str,
        contrasena: &str,
    ) -> Result<(), String> {
        let cuenta = self.cuenta(host, usuario);
        self.guardar(&cuenta, contrasena)
    }

    fn recuperar_contrasena(
        &self,
        host: &str,
        usuario: &str,
    ) -> Result<Option<Zeroizing<String>>, String> {
        let cuenta = self.cuenta(host, usuario);
        self.leer(&cuenta)
    }
}

/// Firmante del móvil: firma con el Keystore a través de FirmanteMovil.
pub struct FirmanteNucleo {
    movil: Arc<dyn FirmanteMovil>,
}

impl FirmanteNucleo {
    pub fn nuevo(movil: Arc<dyn FirmanteMovil>) -> Self {
        Self { movil }
    }
}

impl Firmante for FirmanteNucleo {
    fn clave_publica_x509(&self) -> Result<Vec<u8>, String> {
        self.movil
            .clave_publica_x509()
            .map_err(|error| error.to_string())
    }

    fn firmar(&self, datos: &[u8]) -> Result<Vec<u8>, String> {
        self.movil
            .firmar(datos.to_vec())
            .map_err(|error| error.to_string())
    }
}

/// Línea OpenSSH y huella SHA256 de la clave del móvil (informe §7.2).
pub fn clave_publica_movil(
    firmante: &dyn Firmante,
    equipo: &Equipo,
) -> Result<ClavePublicaMovil, ErrorNucleo> {
    let x509 = firmante
        .clave_publica_x509()
        .map_err(|detalle| ErrorNucleo::Firmante { detalle })?;
    let modelo: String = equipo
        .nombre
        .chars()
        .map(|caracter| {
            if caracter.is_alphanumeric() || caracter == '.' || caracter == '_' || caracter == '-' {
                caracter
            } else {
                '-'
            }
        })
        .collect();
    let fecha = chrono::Local::now().format("%Y%m%d");
    let comentario = format!("magi@{modelo}-{fecha}");
    let linea_openssh =
        x509_a_openssh(&x509, &comentario).map_err(|detalle| ErrorNucleo::Firmante { detalle })?;
    let publica = russh::keys::PublicKey::from_openssh(&linea_openssh).map_err(|error| {
        ErrorNucleo::Firmante {
            detalle: format!("la clave del móvil no es una clave OpenSSH válida: {error}"),
        }
    })?;
    Ok(ClavePublicaMovil {
        huella_sha256: huella(&publica),
        linea_openssh,
    })
}
