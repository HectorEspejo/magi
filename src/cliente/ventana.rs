//! Lanzado de una ventana nueva de terminal con `magi` (`[terminal] comando`).

use std::process::{Command, Stdio};

use crate::config::Config;

/// Lanza el comando de `[terminal] comando` (p. ej. «alacritty -e magi») en
/// un proceso independiente. El error vuelve como mensaje para la barra.
pub fn abrir(config: &Config) -> Result<(), String> {
    if config.terminal.comando.trim().is_empty() {
        return Err("config.toml no define [terminal] comando".to_string());
    }
    Command::new("sh")
        .arg("-c")
        .arg(&config.terminal.comando)
        .stdin(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("no se pudo abrir la ventana: {error}"))
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::Config;

    #[test]
    fn un_comando_vacio_da_error_legible() {
        let mut config = Config::default();
        config.terminal.comando = String::new();
        let error = abrir(&config).unwrap_err();
        assert!(error.contains("[terminal] comando"));
    }
}
