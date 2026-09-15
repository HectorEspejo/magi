use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Herramienta {
    WlCopy,
    Pbcopy,
}

impl Herramienta {
    pub fn nombre(self) -> &'static str {
        match self {
            Herramienta::WlCopy => "wl-copy",
            Herramienta::Pbcopy => "pbcopy",
        }
    }
}

#[derive(Debug)]
enum FalloCopia {
    NoExiste,
    Fallo(String),
}

/// Copia al portapapeles con `wl-copy` (Wayland) o `pbcopy` (macOS). Si no
/// hay ninguna disponible devuelve un error legible para ofrecer el fallback
/// con la clave pública a la vista.
pub fn copiar(texto: &str) -> Result<Herramienta, String> {
    let mut fallos = Vec::new();
    for herramienta in candidatas() {
        match intentar(herramienta.nombre(), texto) {
            Ok(()) => return Ok(*herramienta),
            Err(FalloCopia::NoExiste) => continue,
            Err(FalloCopia::Fallo(motivo)) => {
                fallos.push(format!("{}: {motivo}", herramienta.nombre()))
            }
        }
    }
    if fallos.is_empty() {
        Err("no se encontró wl-copy ni pbcopy en el sistema".to_string())
    } else {
        Err(fallos.join(" · "))
    }
}

fn candidatas() -> &'static [Herramienta] {
    if cfg!(target_os = "macos") {
        &[Herramienta::Pbcopy, Herramienta::WlCopy]
    } else {
        &[Herramienta::WlCopy, Herramienta::Pbcopy]
    }
}

fn intentar(programa: &str, texto: &str) -> Result<(), FalloCopia> {
    let mut proceso = match Command::new(programa)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(proceso) => proceso,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(FalloCopia::NoExiste)
        }
        Err(error) => return Err(FalloCopia::Fallo(error.to_string())),
    };
    if let Some(mut entrada) = proceso.stdin.take() {
        if let Err(error) = entrada.write_all(texto.as_bytes()) {
            return Err(FalloCopia::Fallo(error.to_string()));
        }
    }
    match proceso.wait_with_output() {
        Ok(salida) if salida.status.success() => Ok(()),
        Ok(salida) => Err(FalloCopia::Fallo(format!(
            "salió con {}: {}",
            salida.status,
            String::from_utf8_lossy(&salida.stderr).trim()
        ))),
        Err(error) => Err(FalloCopia::Fallo(error.to_string())),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn un_binario_inexistente_se_detecta() {
        assert!(matches!(
            intentar("magi-no-existe-clipboard", "hola"),
            Err(FalloCopia::NoExiste)
        ));
    }
}
