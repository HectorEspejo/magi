//! Prueba del cliente real (`cliente::conectar`) contra el servidor real:
//! saludo, difusión de sesiones y arranque de la tarea de lectura.

use std::time::Duration;

use magi::config::{Config, Rutas};
use magi::protocolo::MensajeServidor;
use tokio::sync::mpsc;

#[tokio::test]
async fn el_cliente_recibe_la_difusion_de_sesiones_tras_el_saludo() {
    let temporal = tempfile::tempdir().unwrap();
    let raiz = temporal.path().to_path_buf();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    };
    let mut config = Config::default();
    config.servidor.gracia_apagado_seg = 60;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let tarea_servidor = tokio::spawn(magi::servidor::arrancar(rutas.clone(), config));

    // El cliente real: conecta (con autolanzado si hiciera falta) y lanza sus
    // tareas de lectura y escritura.
    let cliente = tokio::time::timeout(Duration::from_secs(5), async {
        match magi::cliente::conectar(&rutas, tx.clone()).await {
            Ok(cliente) => cliente,
            Err(magi::cliente::FalloConexion::Inaccesible(motivo)) => {
                panic!("sin conexión: {motivo}")
            }
            Err(magi::cliente::FalloConexion::VersionIncompatible) => {
                panic!("versión incompatible")
            }
        }
    })
    .await
    .expect("conexión del cliente en 5 s");

    // La tarea de lectura convierte los mensajes en eventos: pedimos `Listar`
    // y esperamos el `Sesiones` de vuelta.
    cliente.enviar(magi::protocolo::MensajeCliente::Listar);
    let mut visto = false;
    let plazo = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < plazo {
        match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Some(magi::app::Evento::Servidor(MensajeServidor::Sesiones { lista }))) => {
                assert!(lista.is_empty());
                visto = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("el canal de eventos se cerró"),
            Err(_) => break,
        }
    }
    assert!(
        visto,
        "la tarea de lectura no entregó el Sesiones de Listar"
    );

    drop(cliente);
    tarea_servidor.abort();
}
