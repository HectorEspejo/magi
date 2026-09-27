//! CASPER-3 · tests: un comando local que escribe el propio usuario en la
//! ficha (`gh run list …`, `curl` a un CI, `test -f`) se ejecuta con `sh -c`;
//! código 0 aprueba, otro código rechaza con la última línea de stderr. Con
//! plazo duro: al vencer se mata el grupo entero y se rechaza.
//!
//! El proceso corre en una sesión propia (`setsid`): sin terminal de control
//! no puede leer las teclas de la TUI (que está en modo raw), y todo lo que
//! lance queda en su grupo, así que matarlo mata también a sus hijos.

use std::path::Path;
use std::process::Stdio;

use tokio::io::AsyncReadExt as _;

/// Lo más que se lee de stderr (solo interesa la última línea).
const TOPE_STDERR: usize = 64 * 1024;

/// Mata el grupo del proceso al soltarse (una deliberación cancelada con la
/// comprobación en marcha, un plazo vencido): no queda nada corriendo.
struct GrupoProceso(Option<u32>);

impl GrupoProceso {
    fn matar(&mut self) {
        if let Some(pid) = self.0.take() {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}

impl Drop for GrupoProceso {
    fn drop(&mut self) {
        self.matar();
    }
}

/// Ejecuta el comando de tests en `dir` con plazo hasta `limite`. Devuelve
/// si aprueba y el dato que lo justifica.
pub async fn ejecutar(comando: &str, limite: tokio::time::Instant, dir: &Path) -> (bool, String) {
    let mut orden = tokio::process::Command::new("sh");
    orden
        .arg("-c")
        .arg(comando)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // SAFETY: entre fork y exec solo se llama a `setsid`, que es
    // async-signal-safe y no reserva memoria.
    unsafe {
        orden.pre_exec(|| {
            nix::unistd::setsid()
                .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?;
            Ok(())
        });
    }
    let mut hijo = match orden.spawn() {
        Ok(hijo) => hijo,
        Err(error) => return (false, format!("no se pudo lanzar sh: {error}")),
    };
    let mut grupo = GrupoProceso(hijo.id());
    let Some(mut errores) = hijo.stderr.take() else {
        return (false, "sin stderr".to_string());
    };
    let lector = tokio::spawn(async move {
        let mut leido = Vec::new();
        let mut bufer = vec![0u8; 8 * 1024];
        loop {
            match errores.read(&mut bufer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    leido.extend_from_slice(&bufer[..n]);
                    if leido.len() > TOPE_STDERR {
                        let sobra = leido.len() - TOPE_STDERR;
                        leido.drain(..sobra);
                    }
                }
            }
        }
        leido
    });
    match tokio::time::timeout_at(limite, hijo.wait()).await {
        Ok(Ok(estado)) => {
            // Lo que el comando dejara corriendo en su grupo no sobrevive, ni
            // puede sostener abierto el stderr que se está leyendo.
            grupo.matar();
            let stderr = tokio::time::timeout(std::time::Duration::from_millis(500), lector)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            if estado.success() {
                (true, "verde".to_string())
            } else {
                let linea =
                    crate::snippets::salida::sanear_linea(&String::from_utf8_lossy(&stderr), 60);
                let codigo = estado
                    .code()
                    .map(|codigo| format!("código {codigo}"))
                    .unwrap_or_else(|| "terminado por una señal".to_string());
                if linea.is_empty() {
                    (false, codigo)
                } else {
                    (false, format!("{codigo}: {linea}"))
                }
            }
        }
        Ok(Err(error)) => {
            lector.abort();
            (false, format!("no se pudo esperar al comando: {error}"))
        }
        Err(_) => {
            grupo.matar();
            // Recoger al hijo: un zombi seguiría «existiendo» para `kill`.
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), hijo.wait()).await;
            lector.abort();
            (false, "plazo vencido".to_string())
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::time::Duration;

    fn limite(segundos: u64) -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_secs(segundos)
    }

    #[tokio::test]
    async fn true_aprueba_y_false_rechaza() {
        let dir = std::env::temp_dir();
        assert_eq!(
            ejecutar("true", limite(2), &dir).await,
            (true, "verde".to_string())
        );
        let (aprueba, texto) = ejecutar("false", limite(2), &dir).await;
        assert!(!aprueba);
        assert_eq!(texto, "código 1");
    }

    #[tokio::test]
    async fn el_rechazo_dice_la_ultima_linea_de_stderr() {
        let dir = std::env::temp_dir();
        let (aprueba, texto) = ejecutar(
            "echo primero >&2; printf '\\033[31mtests en rojo\\033[0m\\n' >&2; exit 3",
            limite(2),
            &dir,
        )
        .await;
        assert!(!aprueba);
        assert_eq!(texto, "código 3: tests en rojo");
    }

    /// AC (checklist l. 86): `sleep 3` con plazo de 2 s rechaza «plazo
    /// vencido» a los 2 s y el proceso ya no existe.
    #[tokio::test]
    async fn al_vencer_el_plazo_se_mata_el_proceso() {
        let dir = tempfile::tempdir().unwrap();
        let marca = dir.path().join("pid");
        let comando = format!("echo $$ > {}; exec sleep 3", marca.display());
        let inicio = std::time::Instant::now();
        let (aprueba, texto) = ejecutar(&comando, limite(2), dir.path()).await;
        let tardado = inicio.elapsed();
        assert!(!aprueba);
        assert_eq!(texto, "plazo vencido");
        assert!(
            tardado >= Duration::from_millis(1900) && tardado < Duration::from_millis(2900),
            "tardó {tardado:?}"
        );
        let pid: i32 = std::fs::read_to_string(&marca)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let vivo = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok();
        assert!(!vivo, "el proceso {pid} sigue vivo");
    }

    /// Un nieto que se queda con el stderr abierto no alarga la espera ni
    /// sobrevive.
    #[tokio::test]
    async fn un_nieto_no_sobrevive_ni_bloquea() {
        let dir = tempfile::tempdir().unwrap();
        let marca = dir.path().join("nieto");
        let comando = format!("(sleep 5 & echo $! > {}; wait) ; true", marca.display());
        let (aprueba, texto) = ejecutar(&comando, limite(2), dir.path()).await;
        assert!(!aprueba);
        assert_eq!(texto, "plazo vencido");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let pid: i32 = std::fs::read_to_string(&marca)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err());
    }
}
