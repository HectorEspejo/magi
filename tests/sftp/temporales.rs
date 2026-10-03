//! Fase 8 · Temporales: los de edición viven aparte y no se vacían; `BorrarTemporal` no sale de su directorio.

#[allow(unused_imports)]
use super::*;

use magi::servidor::sftp::{dir_ediciones, dir_temporales};

/// Pide un temporal y devuelve su ruta.
async fn descargar(montaje: &mut Montaje, ruta: &Path, peticion_id: u64, edicion: bool) -> PathBuf {
    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::DescargarTemporal {
            host_id,
            ruta: ruta.display().to_string(),
            peticion_id,
            edicion,
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::RutaTemporal { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::RutaTemporal {
            ruta,
            peticion_id: id,
        } => {
            assert_eq!(id, peticion_id);
            PathBuf::from(ruta)
        }
        otro => panic!("se esperaba RutaTemporal, llegó {otro:?}"),
    }
}

/// `BorrarTemporal` no contesta si va bien: un listado posterior asegura que
/// el servidor ya lo ha procesado (y que no llegó ningún `Error`).
async fn borrar_y_esperar(montaje: &mut Montaje, temporal: &Path) {
    montaje
        .enviar(&MensajeCliente::BorrarTemporal {
            ruta: temporal.display().to_string(),
        })
        .await;
    let host_id = montaje.host_id;
    let remoto = montaje.remoto();
    montaje
        .enviar(&MensajeCliente::ListarDir {
            host_id,
            ruta: remoto.display().to_string(),
            peticion_id: 999,
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::DirListado { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::DirListado { .. } => {}
        otro => panic!("borrar un temporal propio no debe fallar: {otro:?}"),
    }
}

/// Espera el `Error` sin petición con el que se rechaza un `BorrarTemporal`.
async fn esperar_rechazo(montaje: &mut Montaje, ruta: &str) {
    montaje
        .enviar(&MensajeCliente::BorrarTemporal {
            ruta: ruta.to_string(),
        })
        .await;
    match montaje
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::Error { .. }))
        .await
    {
        MensajeServidor::Error {
            mensaje,
            peticion_id,
        } => {
            assert_eq!(peticion_id, None);
            assert!(
                mensaje.contains("no es un temporal"),
                "motivo explícito: {mensaje}"
            );
        }
        otro => panic!("se esperaba Error, llegó {otro:?}"),
    }
}

/// T55: el temporal de una edición no puede morir porque otra ventana (o la
/// misma) mire otro fichero mientras el editor está abierto.
#[tokio::test]
async fn un_temporal_de_edicion_sobrevive_a_otra_descarga() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::write(remoto.join("nginx.conf"), b"server {}\n").unwrap();
    std::fs::write(remoto.join("error.log"), b"linea\n").unwrap();
    montaje.abrir_sftp().await;

    let edicion = descargar(&mut montaje, &remoto.join("nginx.conf"), 1, true).await;
    // El usuario «edita».
    std::fs::write(&edicion, b"server { listen 80; }\n").unwrap();
    let ver = descargar(&mut montaje, &remoto.join("error.log"), 2, false).await;
    let _otra = descargar(&mut montaje, &remoto.join("error.log"), 3, false).await;

    assert!(ver.exists(), "el temporal de ver sigue mientras se usa");
    assert_eq!(
        leer(&edicion),
        "server { listen 80; }\n",
        "los cambios sin subir siguen ahí"
    );
    assert_eq!(
        edicion.parent().unwrap(),
        dir_ediciones(&montaje.entorno.rutas),
        "las ediciones van aparte de los temporales de ver"
    );
    assert_eq!(
        ver.parent().unwrap(),
        dir_temporales(&montaje.entorno.rutas)
    );
}

#[tokio::test]
async fn el_temporal_de_edicion_va_en_600_dentro_de_un_directorio_700() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::write(remoto.join("app.env"), b"CLAVE=1\n").unwrap();
    montaje.abrir_sftp().await;

    let edicion = descargar(&mut montaje, &remoto.join("app.env"), 4, true).await;
    assert_eq!(leer(&edicion), "CLAVE=1\n");
    assert_eq!(
        std::fs::metadata(&edicion).unwrap().permissions().mode() & 0o777,
        0o600,
        "el temporal de edición va en 600"
    );
    assert_eq!(
        std::fs::metadata(edicion.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "el directorio de ediciones va en 700"
    );

    // Y `BorrarTemporal` también vale para él.
    borrar_y_esperar(&mut montaje, &edicion).await;
    assert!(
        !edicion.exists(),
        "el temporal de edición se borra a petición"
    );
}

/// Dos ventanas tienen cada una su contador de peticiones: el mismo fichero
/// pedido con el mismo `peticion_id` no puede acabar en el mismo temporal.
#[tokio::test]
async fn dos_descargas_del_mismo_fichero_no_comparten_temporal() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::write(remoto.join("config.yaml"), b"a: 1\n").unwrap();
    montaje.abrir_sftp().await;

    for edicion in [false, true] {
        let primera = descargar(&mut montaje, &remoto.join("config.yaml"), 7, edicion).await;
        std::fs::write(&primera, b"cambiado en la primera\n").unwrap();
        let segunda = descargar(&mut montaje, &remoto.join("config.yaml"), 7, edicion).await;
        assert_ne!(primera, segunda, "cada descarga tiene su temporal");
        assert_eq!(
            leer(&primera),
            "cambiado en la primera\n",
            "la segunda descarga no pisa la primera"
        );
        assert_eq!(leer(&segunda), "a: 1\n");
    }
}

/// Un `..` (o un `.`) en la ruta no permite salir del directorio de
/// temporales: `starts_with` no normaliza y `remove_file` sí resuelve.
#[tokio::test]
async fn borrar_temporal_rechaza_rutas_con_puntos() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let rutas = montaje.entorno.rutas.clone();
    let victima = rutas.dir_runtime().join("victima.txt");
    std::fs::write(&victima, b"no me borres").unwrap();
    let otra = dir_temporales(&rutas).join("sub").join("profundo.txt");
    std::fs::create_dir_all(otra.parent().unwrap()).unwrap();
    std::fs::write(&otra, b"tampoco").unwrap();

    let tmp = dir_temporales(&rutas).display().to_string();
    let ediciones = dir_ediciones(&rutas).display().to_string();
    esperar_rechazo(&mut montaje, &format!("{tmp}/../victima.txt")).await;
    esperar_rechazo(&mut montaje, &format!("{ediciones}/../victima.txt")).await;
    esperar_rechazo(&mut montaje, &format!("{tmp}/./../victima.txt")).await;
    esperar_rechazo(&mut montaje, &format!("{tmp}/./profundo.txt")).await;
    // Solo cuelga directamente del directorio: nada de subdirectorios.
    esperar_rechazo(&mut montaje, &otra.display().to_string()).await;
    // Y solo ficheros: el propio subdirectorio tampoco.
    esperar_rechazo(&mut montaje, &format!("{tmp}/sub")).await;
    // Fuera de los dos directorios, nada.
    esperar_rechazo(&mut montaje, &victima.display().to_string()).await;

    assert_eq!(leer(&victima), "no me borres");
    assert_eq!(leer(&otra), "tampoco");
}
