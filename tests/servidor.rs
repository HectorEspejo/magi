//! Pruebas del servidor de sesiones con socket en directorio temporal:
//! arranque, saludo versionado, apagado por inactividad, lock duplicado y
//! mensajes mal formados.

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

use magi::config::{Config, Rutas};
use magi::protocolo::{decodificar, MensajeCliente, MensajeServidor, VERSION_PROTOCOLO};
use magi::servidor;

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    };
    // Gracia corta para que las pruebas de apagado no tarden 10 s.
    let mut config = Config::default();
    config.servidor.gracia_apagado_seg = 1;
    Entorno {
        _temporal: temporal,
        rutas,
        config,
    }
}

async fn esperar_socket(ruta: &Path) {
    for _ in 0..100 {
        if UnixStream::connect(ruta).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("el socket del servidor no ha aparecido en 2 s");
}

/// Envía una línea de mensaje al socket.
async fn enviar(stream: &mut UnixStream, mensaje: &MensajeCliente) {
    let linea = magi::protocolo::codificar(mensaje).unwrap();
    stream
        .write_all(linea.as_bytes())
        .await
        .expect("escribiendo");
    stream.write_all(b"\n").await.expect("escribiendo el salto");
    stream.flush().await.expect("vaciando");
}

/// Conecta, saluda y devuelve el lector del socket.
async fn cliente(ruta: &Path, version: u32) -> BufReader<UnixStream> {
    let mut stream = UnixStream::connect(ruta)
        .await
        .expect("conectando al socket");
    let saludo = MensajeCliente::Hola {
        version,
        pid: std::process::id(),
    };
    enviar(&mut stream, &saludo).await;
    BufReader::new(stream)
}

/// Lee la siguiente línea del socket y la decodifica.
async fn siguiente<M: serde::de::DeserializeOwned>(lector: &mut BufReader<UnixStream>) -> M {
    let mut linea = String::new();
    lector
        .read_line(&mut linea)
        .await
        .expect("leyendo la respuesta");
    decodificar::<M>(linea.trim_end()).expect("mensaje decodificable")
}

#[tokio::test]
async fn el_saludo_versionado_da_la_bienvenida() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let mut lector = cliente(&servidor::ruta_socket(&rutas), VERSION_PROTOCOLO).await;
    let bienvenida: MensajeServidor = siguiente(&mut lector).await;
    match bienvenida {
        MensajeServidor::Bienvenida {
            version, sesiones, ..
        } => {
            assert_eq!(version, VERSION_PROTOCOLO);
            assert!(sesiones.is_empty());
        }
        otro => panic!("se esperaba Bienvenida, llegó {otro:?}"),
    }

    // Desconexión limpia y apagado por inactividad con la gracia mínima.
    let _ = lector.get_mut().shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(8), tarea).await;
    assert!(
        !servidor::ruta_socket(&rutas).exists(),
        "el socket queda borrado"
    );
    assert!(
        !servidor::ruta_lock(&rutas).exists(),
        "el lock queda borrado"
    );
}

#[tokio::test]
async fn una_version_distinta_no_coopera() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let mut lector = cliente(&servidor::ruta_socket(&rutas), 99).await;
    let respuesta: MensajeServidor = siguiente(&mut lector).await;
    match respuesta {
        MensajeServidor::VersionIncompatible { version } => assert_eq!(version, 99),
        otro => panic!("se esperaba VersionIncompatible, llegó {otro:?}"),
    }
    // El servidor cierra la conexión al cliente incompatible.
    let mut resto = Vec::new();
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        lector.get_mut().read_to_end(&mut resto),
    )
    .await;
    let _ = tokio::time::timeout(Duration::from_secs(8), tarea).await;
}

#[tokio::test]
async fn un_segundo_servidor_no_arranca_por_el_lock() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let segundo = tokio::time::timeout(
        Duration::from_secs(3),
        servidor::arrancar(rutas.clone(), entorno.config.clone()),
    )
    .await
    .expect("el segundo servidor debe fallar sin bloquearse");
    assert!(segundo.is_err(), "el lock lo impide con un mensaje");

    let _ = tokio::time::timeout(Duration::from_secs(8), tarea).await;
}

#[tokio::test]
async fn un_mensaje_mal_formado_cierra_solo_ese_cliente() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let mut lector = cliente(&servidor::ruta_socket(&rutas), VERSION_PROTOCOLO).await;
    let _bienvenida: MensajeServidor = siguiente(&mut lector).await;

    lector
        .get_mut()
        .write_all(b"{esto no es un mensaje\n")
        .await
        .expect("escribiendo basura");
    let respuesta: MensajeServidor = siguiente(&mut lector).await;
    assert!(matches!(respuesta, MensajeServidor::Error { .. }));
    let _ = lector.get_mut().shutdown().await;

    // El servidor sigue vivo: un cliente nuevo saluda bien.
    let mut lector_2 = cliente(&servidor::ruta_socket(&rutas), VERSION_PROTOCOLO).await;
    let bienvenida: MensajeServidor = siguiente(&mut lector_2).await;
    assert!(matches!(bienvenida, MensajeServidor::Bienvenida { .. }));
    let _ = lector_2.get_mut().shutdown().await;

    let _ = tokio::time::timeout(Duration::from_secs(8), tarea).await;
}

#[tokio::test]
async fn el_apagado_por_orden_borra_socket_y_lock() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let mut stream = UnixStream::connect(servidor::ruta_socket(&rutas))
        .await
        .expect("conectando");
    enviar(
        &mut stream,
        &MensajeCliente::Hola {
            version: VERSION_PROTOCOLO,
            pid: std::process::id(),
        },
    )
    .await;
    enviar(&mut stream, &MensajeCliente::Parar).await;

    let _ = tokio::time::timeout(Duration::from_secs(8), tarea).await;
    assert!(!servidor::ruta_socket(&rutas).exists(), "socket borrado");
    assert!(!servidor::ruta_lock(&rutas).exists(), "lock borrado");
}

#[tokio::test]
async fn la_gracia_de_apagado_es_de_diez_segundos_por_defecto() {
    assert_eq!(Config::default().servidor.gracia_apagado_seg, 10);
}
