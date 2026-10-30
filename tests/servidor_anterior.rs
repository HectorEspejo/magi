//! Fase 9: sustitución del servidor de sesiones anterior (§4.4 y §7.2).
//! Comprueba que sin pid no se sustituye y que un servidor que ignora
//! `SIGTERM` y conserva el lock no se sustituye ni se relanza nada.

use std::io::BufRead as _;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;

use magi::cliente::{self, FalloSustitucion, RechazoVersion};
use magi::config::Rutas;

fn rutas_de(raiz: &Path) -> Rutas {
    Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    }
}

/// Sin pid no hay a quién mandar `SIGTERM`: se dice cómo pararlo a mano.
#[tokio::test]
async fn sin_pid_no_se_puede_sustituir() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let rutas = rutas_de(temporal.path());
    let (tx, _rx) = mpsc::unbounded_channel();
    let rechazo = RechazoVersion::nueva(5);
    match cliente::sustituir_servidor(&rutas, &rechazo, tx).await {
        Err(FalloSustitucion::Inaccesible(motivo)) => {
            assert!(motivo.contains("magi servidor parar"), "{motivo}");
        }
        Ok(_) => panic!("se sustituyó un servidor sin saber su pid"),
        Err(otro) => panic!("se esperaba Inaccesible, llegó {otro:?}"),
    }
}

/// Un servidor anterior que no atiende `SIGTERM` y conserva el lock no se
/// sustituye: a los 5 s se devuelve `NoLiberado` con su pid y no se lanza
/// ningún servidor nuevo.
#[tokio::test]
async fn un_servidor_que_no_libera_el_lock_no_se_sustituye() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let rutas = rutas_de(temporal.path());
    std::fs::create_dir_all(rutas.dir_runtime()).expect("runtime");
    // El ayudante es este mismo binario de pruebas: bloquea el lock como el
    // servidor anterior y no atiende SIGTERM (ver el test de apoyo de abajo).
    let mut hijo = std::process::Command::new(std::env::current_exe().expect("executable"))
        .args(["--exact", "ayudante_servidor_que_no_cede", "--nocapture"])
        .env("MAGI_AYUDANTE_LOCK", magi::servidor::ruta_lock(&rutas))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("lanzando el ayudante");
    let salida = hijo.stdout.take().expect("stdout del ayudante");
    let (tx_listo, rx_listo) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for linea in std::io::BufReader::new(salida)
            .lines()
            .map_while(Result::ok)
        {
            if linea.contains("listo") {
                let _ = tx_listo.send(());
                return;
            }
        }
    });
    rx_listo
        .recv_timeout(Duration::from_secs(10))
        .expect("el ayudante no tomó el lock");
    let pid = hijo.id();
    let mut rechazo = RechazoVersion::nueva(5);
    rechazo.pid = Some(pid);
    let (tx, _rx) = mpsc::unbounded_channel();
    let resultado = tokio::time::timeout(
        Duration::from_secs(15),
        cliente::sustituir_servidor(&rutas, &rechazo, tx),
    )
    .await
    .expect("la sustitución no terminó");
    match resultado {
        Err(FalloSustitucion::NoLiberado { pid: Some(al_pid) }) => assert_eq!(al_pid, pid),
        Ok(_) => panic!("se lanzó un servidor con el anterior todavía vivo"),
        Err(otro) => panic!("se esperaba NoLiberado, llegó {otro:?}"),
    }
    // No se lanzó nada: ni socket ni log de un servidor nuevo.
    assert!(!magi::servidor::ruta_socket(&rutas).exists());
    let logs = rutas.dir_logs();
    assert!(
        !logs.exists()
            || std::fs::read_dir(&logs)
                .map(|mut entradas| entradas.next().is_none())
                .unwrap_or(false),
        "se lanzó un servidor que no tocaba"
    );
    // El ayudante sigue vivo: no atendió la señal.
    assert!(hijo.try_wait().expect("estado del ayudante").is_none());
    let _ = hijo.kill();
    let _ = hijo.wait();
}

/// `magi servidor parar --si` no para un servidor más nuevo que este
/// binario: podría custodiar trabajo que esta versión no entiende. Se comprueba
/// que devuelve error, que el socket sigue donde estaba y que un proceso
/// inocente con el pid que anuncia el rechazo no recibe ninguna señal.
#[tokio::test]
async fn parar_no_toca_un_servidor_mas_nuevo() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let rutas = rutas_de(temporal.path());
    std::fs::create_dir_all(rutas.dir_runtime()).expect("runtime");
    // Si el guardia fallara y se mandara SIGTERM al pid anunciado, moriría.
    let mut victima = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("proceso víctima");
    let pid_victima = victima.id();
    let ruta = magi::servidor::ruta_socket(&rutas);
    let escucha = tokio::net::UnixListener::bind(&ruta).expect("socket falso");
    let tarea = tokio::spawn(async move {
        let (stream, _) = escucha.accept().await.expect("conexión del CLI");
        let (lectura, mut escritura) = stream.into_split();
        let mut lector = tokio::io::BufReader::new(lectura);
        let mut saludo = String::new();
        let _ = lector.read_line(&mut saludo).await;
        let respuesta = format!(
            r#"{{"tipo":"VersionIncompatible","version":99,"pid":{pid_victima},"version_app":"9.9.9","clientes":0,"trabajo":{{"sesiones":0,"transferencias":0,"tuneles":0,"ejecuciones":0}}}}"#
        );
        escritura
            .write_all(respuesta.as_bytes())
            .await
            .expect("escribiendo el rechazo");
        escritura
            .write_all(b"\n")
            .await
            .expect("escribiendo el salto");
        let _ = escritura.shutdown().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let codigo = magi::servidor::parar_cli(&rutas, true)
        .await
        .expect("parar_cli no debe fallar");
    assert_eq!(codigo, 1, "un servidor más nuevo no se para");
    let _ = tokio::time::timeout(Duration::from_secs(2), tarea).await;
    assert!(
        victima.try_wait().expect("estado de la víctima").is_none(),
        "se mandó SIGTERM al pid anunciado por un servidor más nuevo"
    );
    assert!(
        ruta.exists(),
        "el socket de un servidor más nuevo no se toca"
    );
    let _ = victima.kill();
    let _ = victima.wait();
}

/// Test de apoyo: cuando la prueba de arriba lo lanza con
/// `MAGI_AYUDANTE_LOCK`, bloquea el lock del servidor, ignora `SIGTERM` y no
/// vuelve nunca. Ejecutado por el resto de la suite, no hace nada.
#[test]
fn ayudante_servidor_que_no_cede() {
    let Ok(ruta) = std::env::var("MAGI_AYUDANTE_LOCK") else {
        return;
    };
    cliente::servidor_que_no_cede(Path::new(&ruta));
}
