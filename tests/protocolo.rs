//! Fase 9: contrato estable del saludo. Fija la forma JSON de `Hola` y
//! `VersionIncompatible` (nunca se quitan ni renombran campos, solo se añaden
//! opcionales) y comprueba que el cliente lee el rechazo de un servidor v5 y
//! de uno v6 con el parser genérico de la primera respuesta.

use std::path::Path;
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use magi::cliente::{self, FalloConexion, RechazoVersion};
use magi::config::Rutas;
use magi::protocolo::{MensajeCliente, MensajeServidor, TrabajoServidor, VERSION_PROTOCOLO};

/// Entorno aislado con una ruta de socket corta.
fn rutas_de(raiz: &Path) -> Rutas {
    Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    }
}

#[test]
fn hola_tiene_una_forma_json_estable() {
    let mensaje = MensajeCliente::Hola {
        version: VERSION_PROTOCOLO,
        pid: 123,
    };
    assert_eq!(
        serde_json::to_value(&mensaje).unwrap(),
        json!({"tipo": "Hola", "version": VERSION_PROTOCOLO, "pid": 123})
    );
}

#[test]
fn version_incompatible_tiene_una_forma_json_estable() {
    let mensaje = MensajeServidor::VersionIncompatible {
        version: 6,
        pid: Some(48211),
        version_app: Some("1.0.0".to_string()),
        clientes: Some(2),
        trabajo: Some(TrabajoServidor {
            sesiones: 1,
            transferencias: 2,
            tuneles: 3,
            ejecuciones: 4,
        }),
    };
    assert_eq!(
        serde_json::to_value(&mensaje).unwrap(),
        json!({
            "tipo": "VersionIncompatible",
            "version": 6,
            "pid": 48211,
            "version_app": "1.0.0",
            "clientes": 2,
            "trabajo": {
                "sesiones": 1,
                "transferencias": 2,
                "tuneles": 3,
                "ejecuciones": 4
            }
        })
    );
}

/// El rechazo de un servidor v5 no trae `version_app`, `clientes` ni
/// `trabajo`: se lee como «recuento desconocido», nunca como cero.
#[tokio::test]
async fn el_rechazo_de_un_servidor_v5_se_lee_como_recuento_desconocido() {
    let rechazo = leer_rechazo(r#"{"tipo":"VersionIncompatible","version":5,"pid":4242}"#).await;
    assert_eq!(rechazo.version, 5);
    assert_eq!(rechazo.pid, Some(4242));
    assert_eq!(rechazo.version_app, None);
    assert_eq!(rechazo.clientes, None);
    assert_eq!(rechazo.trabajo, None);
}

/// Un rechazo v6 trae la versión de la app, las otras ventanas y lo que
/// custodia, y el parser genérico lo entiende.
#[tokio::test]
async fn el_rechazo_de_un_servidor_v6_se_lee_entero() {
    let rechazo = leer_rechazo(
        r#"{"tipo":"VersionIncompatible","version":6,"pid":4242,"version_app":"1.0.0","clientes":1,"trabajo":{"sesiones":2,"transferencias":0,"tuneles":1,"ejecuciones":0}}"#,
    )
    .await;
    assert_eq!(rechazo.version, 6);
    assert_eq!(rechazo.pid, Some(4242));
    assert_eq!(rechazo.version_app.as_deref(), Some("1.0.0"));
    assert_eq!(rechazo.clientes, Some(1));
    assert_eq!(
        rechazo.trabajo,
        Some(TrabajoServidor {
            sesiones: 2,
            transferencias: 0,
            tuneles: 1,
            ejecuciones: 0,
        })
    );
}

/// Un campo de `trabajo` que falte vale 1 («hay trabajo»): un servidor futuro
/// que añada otro tipo nunca se sustituye por error.
#[tokio::test]
async fn un_trabajo_incompleto_vale_como_lleno() {
    let rechazo =
        leer_rechazo(r#"{"tipo":"VersionIncompatible","version":6,"trabajo":{"sesiones":0}}"#)
            .await;
    assert_eq!(
        rechazo.trabajo,
        Some(TrabajoServidor {
            sesiones: 0,
            transferencias: 1,
            tuneles: 1,
            ejecuciones: 1,
        })
    );
}

/// Levanta un socket falso que contesta al `Hola` con `linea` y devuelve el
/// rechazo tal como lo lee `cliente::conectar`.
async fn leer_rechazo(linea: &'static str) -> RechazoVersion {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let rutas = rutas_de(temporal.path());
    std::fs::create_dir_all(rutas.dir_runtime()).expect("runtime");
    let ruta = magi::servidor::ruta_socket(&rutas);
    let escucha = UnixListener::bind(&ruta).expect("socket de prueba");
    let tarea = tokio::spawn(async move {
        let (stream, _) = escucha.accept().await.expect("conexión del cliente");
        let (lectura, mut escritura) = stream.into_split();
        let mut lector = BufReader::new(lectura);
        let mut saludo = String::new();
        let _ = lector.read_line(&mut saludo).await;
        escritura
            .write_all(linea.as_bytes())
            .await
            .expect("escribiendo el rechazo");
        escritura
            .write_all(b"\n")
            .await
            .expect("escribiendo el salto");
        let _ = escritura.shutdown().await;
    });
    let (tx, _rx) = mpsc::unbounded_channel();
    let resultado = tokio::time::timeout(Duration::from_secs(5), cliente::conectar(&rutas, tx))
        .await
        .expect("la conexión no terminó");
    let _ = tokio::time::timeout(Duration::from_secs(2), tarea).await;
    match resultado {
        Err(FalloConexion::VersionIncompatible(rechazo)) => rechazo,
        Ok(_) => panic!("se esperaba un rechazo de versión y llegó una bienvenida"),
        Err(otro) => panic!("se esperaba un rechazo de versión, llegó {otro:?}"),
    }
}
