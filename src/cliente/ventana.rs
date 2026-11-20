//! Lanzado de una ventana nueva de terminal con `magi` (`[terminal] comando`).
//!
//! En Termux no hay ventanas que lanzar (es un solo terminal a pantalla
//! completa): `prefijo w` explica cómo abrir otra sesión de Termux (Fase 17).

use std::process::{Command, Stdio};

use crate::config::Config;

/// Lo que dice `prefijo w` en Termux en vez de lanzar un terminal (§5.4).
pub const EXPLICACION_TERMUX: &str =
    "abre otra sesión de Termux (desliza desde el borde izquierdo → NEW SESSION) y ejecuta magi";

/// Cómo salió el intento de abrir una ventana nueva. La explicación de Termux
/// no es un fallo: se distingue del error con el tipo, no comparando textos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AperturaVentana {
    /// Se lanzó el comando de `[terminal] comando`.
    Abierta,
    /// Termux: no hay ventanas que lanzar; es lo que hay que hacer a mano
    /// (información para la barra, no un error).
    Explicacion(String),
    /// No se pudo lanzar, con el motivo.
    Error(String),
}

/// Lanza el comando de `[terminal] comando` (p. ej. «alacritty -e magi») en
/// un proceso independiente. En Termux no lanza nada y devuelve la
/// explicación de [`EXPLICACION_TERMUX`].
pub fn abrir_ventana(config: &Config) -> AperturaVentana {
    abrir_ventana_para(crate::termux::es_termux(), config)
}

/// [`abrir_ventana`] con la plataforma a la vista, para probar las dos ramas
/// sin depender del proceso (y para que la `App` use la suya).
#[doc(hidden)]
pub fn abrir_ventana_para(termux: bool, config: &Config) -> AperturaVentana {
    if termux {
        return AperturaVentana::Explicacion(EXPLICACION_TERMUX.to_string());
    }
    if config.terminal.comando.trim().is_empty() {
        return AperturaVentana::Error("config.toml no define [terminal] comando".to_string());
    }
    match Command::new("sh")
        .arg("-c")
        .arg(&config.terminal.comando)
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(_) => AperturaVentana::Abierta,
        Err(error) => AperturaVentana::Error(format!("no se pudo abrir la ventana: {error}")),
    }
}

/// La forma anterior de [`abrir_ventana`]: el error vuelve como mensaje para la
/// barra y, en Termux, la explicación también va por el error. Quien necesite
/// distinguirlos usa [`abrir_ventana`].
pub fn abrir(config: &Config) -> Result<(), String> {
    abrir_para(crate::termux::es_termux(), config)
}

/// [`abrir`] con la plataforma a la vista.
#[doc(hidden)]
pub fn abrir_para(termux: bool, config: &Config) -> Result<(), String> {
    match abrir_ventana_para(termux, config) {
        AperturaVentana::Abierta => Ok(()),
        AperturaVentana::Explicacion(mensaje) | AperturaVentana::Error(mensaje) => Err(mensaje),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::Config;

    #[test]
    fn un_comando_vacio_da_error_legible() {
        let mut config = Config::default();
        config.terminal.comando = String::new();
        let AperturaVentana::Error(error) = abrir_ventana_para(false, &config) else {
            panic!("un comando vacío debe ser un error");
        };
        assert!(error.contains("[terminal] comando"));
        // La forma anterior sigue dando el mismo error.
        assert!(abrir_para(false, &config)
            .unwrap_err()
            .contains("[terminal] comando"));
    }

    #[test]
    fn en_termux_se_explica_en_vez_de_lanzar_un_terminal() {
        // Aunque haya un comando bueno, no se lanza nada: la explicación sale
        // antes de tocar la configuración.
        let mut config = Config::default();
        config.terminal.comando = "touch /magi-no-debe-existir".to_string();
        assert_eq!(
            abrir_ventana_para(true, &config),
            AperturaVentana::Explicacion(
                "abre otra sesión de Termux (desliza desde el borde izquierdo → NEW SESSION) y ejecuta magi"
                    .to_string()
            ),
            "una explicación, no un error"
        );
        assert!(!std::path::Path::new("/magi-no-debe-existir").exists());
        // Fuera de Termux sigue como siempre: un comando vacío es un error de
        // configuración, no la explicación.
        config.terminal.comando = String::new();
        assert!(matches!(
            abrir_ventana_para(false, &config),
            AperturaVentana::Error(error) if error.contains("[terminal] comando")
        ));
        // La forma anterior lleva la explicación por el error, como siempre.
        assert_eq!(abrir_para(true, &config).unwrap_err(), EXPLICACION_TERMUX);
    }

    #[test]
    fn un_comando_valido_se_lanza_y_se_dice_abierta() {
        let temporal = tempfile::tempdir().unwrap();
        let marca = temporal.path().join("marca");
        let mut config = Config::default();
        config.terminal.comando = format!("touch {}", marca.display());
        assert_eq!(abrir_ventana_para(false, &config), AperturaVentana::Abierta);
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marca.exists() && std::time::Instant::now() < limite {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(marca.exists(), "el comando no llegó a ejecutarse");
    }
}
