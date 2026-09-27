//! Deliberación MAGI de extremo a extremo con las fuentes reales: el cliente
//! del protocolo pide un SFTP de comprobación (no interactivo) al servidor de
//! sesiones y lista la ruta de backups con el `sftp-server` de OpenSSH.

mod comun;

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use comun::{escenario, hay_sftp_server, OpcionesEscenario};
use magi::deliberacion::{
    lanzar, Comprobacion, Fuentes, Limites, ResultadoComprobacion, Trabajo, Veredicto,
};

/// Delibera BALTHASAR-2 sobre `ruta` del host con un `Cliente` de verdad.
async fn backup(escenario: &comun::Escenario, ruta: &str, patron: Option<&str>) -> Veredicto {
    let (tx_eventos, _rx_eventos) = tokio::sync::mpsc::unbounded_channel();
    let cliente = magi::cliente::conectar(escenario.rutas(), tx_eventos)
        .await
        .expect("cliente conectado al servidor de sesiones");
    let limites = Limites::desde_config(&magi::config::SeccionDeliberacion::default());
    // Plazo holgado: aquí se prueba el camino, no el plazo de 2 s.
    let limite = tokio::time::Instant::now() + Duration::from_secs(20);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ResultadoComprobacion>();
    let _tareas = lanzar(
        &tokio::runtime::Handle::current(),
        1,
        vec![(
            escenario.hosts[0],
            Comprobacion::Backup,
            Trabajo::Backup {
                ruta: ruta.to_string(),
                patron: patron.map(str::to_string),
            },
        )],
        Fuentes::reales(cliente),
        limites,
        magi::flota::estado::Umbrales::default(),
        limite,
        Arc::new(move |resultado| {
            let _ = tx.send(resultado);
        }),
    );
    rx.recv().await.expect("un resultado").veredicto
}

fn fichero(ruta: &std::path::Path, horas: u64) {
    std::fs::write(ruta, b"respaldo").unwrap();
    let momento = SystemTime::now() - Duration::from_secs(horas * 3600);
    filetime::set_file_mtime(ruta, filetime::FileTime::from_system_time(momento)).unwrap();
}

#[tokio::test]
async fn balthasar_aprueba_un_backup_reciente_y_rechaza_uno_viejo() {
    if !hay_sftp_server() {
        return;
    }
    let escenario = escenario(OpcionesEscenario::default()).await;
    let directorio = tempfile::tempdir().unwrap();
    fichero(&directorio.path().join("pg-antiguo.sql.gz"), 40);
    fichero(&directorio.path().join("pg-nuevo.sql.gz"), 3);
    // Un fichero más reciente que no casa con el patrón no cuenta.
    fichero(&directorio.path().join("notas.txt"), 0);
    let ruta = directorio.path().display().to_string();

    let veredicto = backup(&escenario, &ruta, Some("*.sql.gz")).await;
    assert!(veredicto.aprueba(), "{veredicto:?}");
    assert!(
        veredicto.detalle().unwrap().contains("hace 3 h"),
        "{veredicto:?}"
    );

    std::fs::remove_file(directorio.path().join("pg-nuevo.sql.gz")).unwrap();
    let veredicto = backup(&escenario, &ruta, Some("*.sql.gz")).await;
    assert!(veredicto.rechaza(), "{veredicto:?}");
    assert!(
        veredicto.detalle().unwrap().contains("40 h"),
        "{veredicto:?}"
    );
}

#[tokio::test]
async fn balthasar_rechaza_sin_acceso_con_el_motivo() {
    if !hay_sftp_server() {
        return;
    }
    let escenario = escenario(OpcionesEscenario::default()).await;
    let veredicto = backup(&escenario, "/no/existe/magi-backups", None).await;
    assert!(veredicto.rechaza(), "{veredicto:?}");
    assert!(
        veredicto
            .detalle()
            .unwrap()
            .starts_with("sin acceso SFTP: "),
        "{veredicto:?}"
    );
}

#[tokio::test]
async fn balthasar_no_dialoga_con_una_huella_desconocida() {
    if !hay_sftp_server() {
        return;
    }
    let escenario = escenario(OpcionesEscenario {
        huella_conocida: false,
        ..Default::default()
    })
    .await;
    let veredicto = backup(&escenario, "/tmp", None).await;
    assert!(veredicto.rechaza(), "{veredicto:?}");
    assert!(
        veredicto.detalle().unwrap().contains("sin acceso SFTP"),
        "{veredicto:?}"
    );
}
