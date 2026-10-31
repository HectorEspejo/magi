//! Llavero de secretos inyectable: la interfaz que implementan el llavero
//! del sistema (escritorio) y el cifrado con el Keystore (móvil).
//!
//! Un secreto que no es una clave (contraseñas de host y la clave de
//! sincronización) solo vive aquí: nunca en magi.db, en DataStore ni en un
//! log.

use zeroize::Zeroizing;

/// Servicio con el que MAGI firma sus entradas en el llavero del sistema.
pub const SERVICIO: &str = "magi";

/// Cuenta donde vive la clave de sincronización del inventario (Fase 10).
pub const CUENTA_SINCRONIZACION: &str = "sincronizacion";

/// Cuenta de la clave anterior, solo para leer durante una rotación.
pub const CUENTA_SINCRONIZACION_ANTERIOR: &str = "sincronizacion-anterior";

/// Interfaz de los secretos genéricos del llavero (la clave de sincronización
/// y su anterior, las contraseñas de host). La implementación real usa el
/// llavero del sistema en el escritorio y el Keystore en el móvil; las pruebas
/// usan una en memoria.
pub trait LlaveroSecreto: Send + Sync {
    fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String>;
    fn recuperar(&self, cuenta: &str) -> Result<Option<Zeroizing<String>>, String>;
    fn olvidar(&self, cuenta: &str) -> Result<bool, String>;
}

/// Llavero en memoria para las pruebas (nunca se usa en producción).
#[doc(hidden)]
#[derive(Default)]
pub struct LlaveroEnMemoria {
    secretos: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
}

impl std::fmt::Debug for LlaveroEnMemoria {
    /// Nunca imprime los valores: aunque sea solo para pruebas, un Debug no
    /// debe poder volcar secretos en un log ni en un mensaje de error.
    fn fmt(&self, formato: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cuentas = self
            .secretos
            .lock()
            .map(|secretos| secretos.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        formato
            .debug_struct("LlaveroEnMemoria")
            .field("cuentas", &cuentas)
            .finish_non_exhaustive()
    }
}

impl LlaveroEnMemoria {
    pub fn nuevo() -> Self {
        Self::default()
    }
}

impl LlaveroSecreto for LlaveroEnMemoria {
    fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String> {
        let mut secretos = self.secretos.lock().map_err(|_| "llavero envenenado")?;
        secretos.insert(cuenta.to_string(), valor.to_string());
        Ok(())
    }

    fn recuperar(&self, cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        let secretos = self.secretos.lock().map_err(|_| "llavero envenenado")?;
        Ok(secretos
            .get(cuenta)
            .map(|valor| Zeroizing::new(valor.clone())))
    }

    fn olvidar(&self, cuenta: &str) -> Result<bool, String> {
        let mut secretos = self.secretos.lock().map_err(|_| "llavero envenenado")?;
        Ok(secretos.remove(cuenta).is_some())
    }
}

/// Contraseñas de host para la tarea de conexión, con la misma interfaz en el
/// escritorio (llavero del sistema) y en el móvil (Keystore, por uuid).
pub trait LlaveroHost: Send + Sync {
    fn guardar_contrasena(&self, host: &str, usuario: &str, contrasena: &str)
        -> Result<(), String>;
    fn recuperar_contrasena(
        &self,
        host: &str,
        usuario: &str,
    ) -> Result<Option<Zeroizing<String>>, String>;
}

impl LlaveroHost for LlaveroEnMemoria {
    fn guardar_contrasena(
        &self,
        host: &str,
        usuario: &str,
        contrasena: &str,
    ) -> Result<(), String> {
        LlaveroSecreto::guardar(self, &format!("{usuario}@{host}"), contrasena)
    }

    fn recuperar_contrasena(
        &self,
        host: &str,
        usuario: &str,
    ) -> Result<Option<Zeroizing<String>>, String> {
        LlaveroSecreto::recuperar(self, &format!("{usuario}@{host}"))
    }
}
