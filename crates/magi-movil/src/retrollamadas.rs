//! Retrollamadas que implementa Kotlin (informe §5.2).
//!
//! Son interfaces foreign (las implementa la app, no Rust): la carpeta por SAF,
//! el llavero con AES-GCM sobre el Keystore, el firmante de la clave del móvil
//! y el oyente que pinta pantallas y listas completas en Compose. El núcleo las
//! recibe una sola vez en Nucleo::iniciar.

use crate::api::{ErrorNucleo, EstadoSincronizacion, Pantalla, PeticionDialogo, Sesion, TunelVivo};

/// Fichero de la carpeta compartida tal como lo lista SAF (informe §5.2):
/// nombre, tamaño y fecha de modificación.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FicheroCarpeta {
    pub nombre: String,
    pub tamano: u64,
    /// Fecha de modificación (epoch en segundos).
    pub modificado_en: i64,
}

/// Carpeta compartida por SAF. La app la implementa con DocumentsContract
/// sobre el árbol elegido, con permiso persistente solo de lectura.
#[uniffi::export(foreign)]
pub trait CarpetaMovil: Send + Sync {
    /// Lista los ficheros de la carpeta (la app filtra a magi-<uuid>.age).
    fn listar(&self) -> Result<Vec<FicheroCarpeta>, ErrorNucleo>;

    /// Lee un fichero de la carpeta, como mucho maximo bytes.
    fn leer(&self, nombre: String, maximo: u64) -> Result<Vec<u8>, ErrorNucleo>;
}

/// Llavero del móvil: ficheros cifrados con AES-256-GCM y una clave del
/// Keystore. Guarda la clave de sincronización (y la anterior) y las
/// contraseñas de host, nunca en magi.db ni en DataStore.
#[uniffi::export(foreign)]
pub trait LlaveroMovil: Send + Sync {
    /// Lee un secreto; None si no existe.
    fn leer(&self, clave: String) -> Result<Option<Vec<u8>>, ErrorNucleo>;

    /// Guarda un secreto (los bytes viajan ya cifrados por la app).
    fn guardar(&self, clave: String, secreto: Vec<u8>) -> Result<(), ErrorNucleo>;

    /// Borra un secreto; no falla si no existía.
    fn borrar(&self, clave: String) -> Result<(), ErrorNucleo>;
}

/// Firmante de la clave del móvil: la clave privada vive en el Android Keystore
/// (ECDSA P-256) y nunca sale de él. El núcleo pide la firma DER fuera del hilo
/// de tokio (informe §7.2).
#[uniffi::export(foreign)]
pub trait FirmanteMovil: Send + Sync {
    /// Clave pública X.509 (DER) de la clave del móvil.
    fn clave_publica_x509(&self) -> Result<Vec<u8>, ErrorNucleo>;

    /// Firma con SHA256withECDSA; devuelve la firma DER.
    fn firmar(&self, datos: Vec<u8>) -> Result<Vec<u8>, ErrorNucleo>;

    /// Borra del Keystore la clave del móvil (solo para «borrar todos los datos»).
    fn olvidar_clave(&self) -> Result<(), ErrorNucleo>;
}

/// Oyente del núcleo: pantallas en tramos (como mucho a 30 fps), listas
/// completas de sesiones y túneles, diálogos y estado de la sincronización.
/// Lo implementan los ViewModels de Compose que la app tenga suscritos.
#[uniffi::export(foreign)]
pub trait OyenteNucleo: Send + Sync {
    /// Pantalla por tramos: solo las filas que cambiaron.
    fn pantalla(&self, pantalla: Pantalla);

    /// Lista completa de sesiones (nunca mensajes finos).
    fn sesiones(&self, lista: Vec<Sesion>);

    /// Lista completa de túneles activos y del inventario.
    fn tuneles(&self, lista: Vec<TunelVivo>);

    /// Petición de diálogo (huella nueva, huella cambiada o contraseña).
    fn dialogo(&self, peticion: PeticionDialogo);

    /// Estado completo de la sincronización (lo pinta Ajustes y el indicador ⇅).
    fn sincronizacion(&self, estado: EstadoSincronizacion);
}
