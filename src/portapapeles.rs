use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

/// Cuánto se espera a que el programa de copia termine por sí solo.
const ESPERA_COPIA: Duration = Duration::from_millis(750);

fn intentar(programa: &str, texto: &str) -> Result<(), FalloCopia> {
    let mut comando = Command::new(programa);
    intentar_comando(&mut comando, texto)
}

/// Copia con un comando ya preparado (separado para poder probarlo con `sh`).
fn intentar_comando(comando: &mut Command, texto: &str) -> Result<(), FalloCopia> {
    let mut proceso = match comando
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        // La salida de error no se canaliza: `wl-copy` se queda como daemon
        // sirviendo el portapapeles y hereda el tubo, así que esperar a leer
        // su fin colgaría la interfaz para siempre (era el cuelgue al copiar
        // en Wayland).
        .stderr(Stdio::null())
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
    // Se espera un poco a que termine el proceso directo. Si sigue vivo es
    // que está sirviendo el portapapeles en primer plano: se deja hacer su
    // trabajo y se recoge en segundo plano para no bloquear la UI ni dejar
    // un zombi.
    let plazo = Instant::now() + ESPERA_COPIA;
    loop {
        match proceso.try_wait() {
            Ok(Some(estado)) if estado.success() => return Ok(()),
            Ok(Some(estado)) => {
                return Err(FalloCopia::Fallo(format!("salió con {estado}")));
            }
            Ok(None) => {
                if Instant::now() >= plazo {
                    std::thread::spawn(move || {
                        let _ = proceso.wait();
                    });
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(FalloCopia::Fallo(error.to_string())),
        }
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

    /// Regresión: `wl-copy` se queda como daemon sirviendo el portapapeles y
    /// antes se esperaba a que cerrara su salida de error, así que copiar
    /// colgaba la interfaz para siempre.
    #[test]
    fn un_programa_que_se_queda_sirviendo_el_portapapeles_no_bloquea() {
        let mut comando = Command::new("sh");
        comando.args(["-c", "cat >/dev/null; sleep 2"]);
        let inicio = Instant::now();
        let resultado = intentar_comando(&mut comando, "hola");
        let transcurrido = inicio.elapsed();
        assert!(
            resultado.is_ok(),
            "la copia con un daemon debe valer: {resultado:?}"
        );
        assert!(
            transcurrido < Duration::from_secs(3),
            "la copia se quedó esperando al daemon: {transcurrido:?}"
        );
    }

    #[test]
    fn un_fallo_del_programa_de_copia_se_detecta() {
        let mut comando = Command::new("sh");
        comando.args(["-c", "cat >/dev/null; exit 3"]);
        assert!(matches!(
            intentar_comando(&mut comando, "hola"),
            Err(FalloCopia::Fallo(_))
        ));
    }

    /// La prueba de verdad con `wl-copy` del sistema, si hay Wayland.
    #[test]
    fn wl_copy_real_no_se_cuelga() {
        if std::env::var_os("WAYLAND_DISPLAY").is_none_or(|valor| valor.is_empty()) {
            return;
        }
        let hay = Command::new("sh")
            .args(["-c", "command -v wl-copy"])
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|estado| estado.success());
        if !hay {
            return;
        }
        let inicio = Instant::now();
        let resultado = intentar("wl-copy", "prueba de magi");
        assert!(resultado.is_ok(), "wl-copy debe copiar");
        assert!(
            inicio.elapsed() < Duration::from_secs(3),
            "wl-copy no debe bloquear la interfaz"
        );
    }
}
