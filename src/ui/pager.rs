//! Suspensión de la TUI para ver un fichero con el paginador y restauración
//! después, como al conectar en la Fase 1.
//!
//! Mientras el visor está en primer plano, el hilo que lee el teclado se
//! pausa: si no, se comería las pulsaciones del paginador y las soltaría todas
//! al volver (incluido un `x` o un `q` que el usuario no quería para MAGI).

use std::io::Write as _;
use std::process::Stdio;

use crate::config::Config;
use crate::tema::Tema;
use crate::ui::TerminalMagi;
use crate::visor::{self, Peticion};

/// Suspende la TUI, ejecuta el paginador sobre el fichero y la restaura.
/// Devuelve el aviso que hay que enseñar en la barra, si algo fue mal.
pub fn suspender_y_ver(
    terminal: &mut TerminalMagi,
    _tema: &Tema,
    config: &Config,
    peticion: &Peticion,
) -> Option<String> {
    let paginador = if config.archivos.pager.trim().is_empty() {
        std::env::var("PAGER").unwrap_or_else(|_| "less".to_string())
    } else {
        config.archivos.pager.clone()
    };
    let (programa, argumentos) = match visor::efectivo(&paginador) {
        Some(par) => par,
        None => {
            return Some(format!(
                "no encuentro el visor «{paginador}»{}: configura [archivos] pager",
                match visor::alternativas() {
                    vacias if vacias.is_empty() => " ni tampoco less o more".to_string(),
                    otras => format!(" (hay {})", otras.join(" o ")),
                }
            ));
        }
    };
    // El configurado no está: se dice con qué se abre, en vez de cambiarlo en
    // silencio.
    let aviso_visor = match visor::comando(&paginador) {
        Some((pedido, _)) if pedido == programa => None,
        _ => Some(format!(
            "«{}» no está instalado: se abre con {programa}",
            paginador.trim()
        )),
    };

    crate::ui::restaurar_terminal();
    // El aviso de la barra del visor: el nombre del fichero, para saber qué se
    // está viendo cuando el paginador no lo pone en el título.
    let _ = writeln!(std::io::stdout(), "\r\n· {} ·", peticion.titulo);
    let _ = std::io::stdout().flush();

    let resultado = std::process::Command::new(&programa)
        .args(&argumentos)
        // La ruta va siempre como argumento suelto: nada de interpolarla.
        .arg(&peticion.ruta)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    let restaurado = crate::ui::iniciar_terminal();
    let mut aviso = match &resultado {
        Err(error) => Some(format!("no se pudo abrir «{programa}»: {error}")),
        // El paginador puede salir con error (un fichero binario, por
        // ejemplo): no es un fallo de MAGI y no se dice nada.
        Ok(_) => None,
    };
    if let Ok(nueva) = restaurado {
        *terminal = nueva;
        let _ = terminal.clear();
        let _ = terminal.autoresize();
    } else {
        aviso = Some("no se pudo restaurar la interfaz tras el visor".to_string());
    }
    aviso.or(aviso_visor)
}

/// Cadena para el paginador de la barra de atajos, según lo configurado.
pub fn etiqueta(config: &Config) -> String {
    if !config.archivos.pager.trim().is_empty() {
        return config.archivos.pager.clone();
    }
    std::env::var("PAGER").unwrap_or_else(|_| "less".to_string())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn config_con(pager: &str) -> Config {
        let mut config = Config::default();
        config.archivos.pager = pager.to_string();
        config
    }

    #[test]
    fn la_etiqueta_ensena_lo_configurado() {
        assert_eq!(etiqueta(&config_con("bat -p")), "bat -p");
    }
}
