//! Pruebas del servidor de escritorios remotos (Fase 12): validación de la
//! fila del host, camino, estados, difusión y REGISTRO. Sin red externa: los
//! destinos que no responden son puertos cerrados de 127.0.0.1.

mod comun;

use std::time::Duration;

use magi::almacen::hosts;
use magi::config::Config;
use magi::modelo::{DatosHost, Origen};
use magi::protocolo::{MensajeCliente, MensajeServidor, RANGO_ESCRITORIOS, VERSION_PROTOCOLO};
use magi::servidor;

use comun::*;

const PETICION: u64 = RANGO_ESCRITORIOS + 1;

/// Crea un host de escritorio en la base de datos del entorno.
fn crear_host(rutas: &magi::config::Rutas, tipo: magi::escritorio::TipoHost, puerto: u16) -> i64 {
    let mut modelo = DatosHost {
        nombre: format!("escritorio-{}", tipo.como_texto()),
        direccion: "127.0.0.1".to_string(),
        puerto,
        tipo,
        usuario: Some("usuario".to_string()),
        ..DatosHost::default()
    };
    modelo.escritorio_opciones = magi::escritorio::OpcionesEscritorio::por_defecto(tipo);
    if tipo == magi::escritorio::TipoHost::Vnc {
        modelo.escritorio_opciones.directo = true;
    }
    let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
    let id = hosts::crear(almacen.conexion(), &modelo, Origen::Manual).expect("creando el host");
    almacen.cerrar().expect("cerrando magi.db");
    id
}

/// Arranca el servidor de sesiones y saluda una ventana.
async fn arrancar(
    rutas: &magi::config::Rutas,
    config: &Config,
) -> (
    tokio::task::JoinHandle<anyhow::Result<()>>,
    tokio::net::unix::OwnedWriteHalf,
    tokio_util::codec::FramedRead<tokio::net::unix::OwnedReadHalf, magi::protocolo::LinesCodec>,
    u32,
) {
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), config.clone()));
    esperar_socket(&servidor::ruta_socket(rutas)).await;
    let mut stream = tokio::net::UnixStream::connect(servidor::ruta_socket(rutas))
        .await
        .expect("conectando al servidor");
    let saludo = MensajeCliente::Hola {
        version: VERSION_PROTOCOLO,
        pid: 42,
    };
    enviar(&mut stream, &saludo).await;
    let (lectura, escritura) = stream.into_split();
    let mut lector = tokio_util::codec::FramedRead::new(lectura, magi::protocolo::codec());
    let bienvenida: MensajeServidor = siguiente_framed(&mut lector).await;
    let MensajeServidor::Bienvenida { cliente_id, .. } = bienvenida else {
        panic!("se esperaba Bienvenida y llegó {bienvenida:?}");
    };
    (tarea, escritura, lector, cliente_id)
}

/// Lee mensajes hasta que uno cumpla el predicado (con plazo).
async fn esperar<F>(
    lector: &mut tokio_util::codec::FramedRead<
        tokio::net::unix::OwnedReadHalf,
        magi::protocolo::LinesCodec,
    >,
    mut vale: F,
) -> MensajeServidor
where
    F: FnMut(&MensajeServidor) -> bool,
{
    let fin = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let restante = fin.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(restante, siguiente_framed(lector)).await {
            Ok(mensaje) if vale(&mensaje) => return mensaje,
            Ok(_) => {}
            Err(_) => panic!("el servidor no mandó lo esperado en 20 s"),
        }
    }
}

/// Cuenta las anotaciones de un tipo en REGISTRO, con reintentos: las escribe
/// el hilo escritor de la base de datos.
fn anotaciones(rutas: &magi::config::Rutas, tipo: &str) -> usize {
    let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
    let cuenta = {
        let mut sentencia = almacen
            .conexion()
            .prepare("SELECT COUNT(*) FROM REGISTRO WHERE tipo = ?1")
            .expect("preparando");
        sentencia
            .query_row([tipo], |fila| fila.get::<_, i64>(0))
            .unwrap_or(0) as usize
    };
    almacen.cerrar().expect("cerrando magi.db");
    cuenta
}

async fn esperar_anotacion(rutas: &magi::config::Rutas, tipo: &str) -> usize {
    let fin = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let cuenta = anotaciones(rutas, tipo);
        if cuenta > 0 || tokio::time::Instant::now() >= fin {
            return cuenta;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn la_bienvenida_no_manda_escritorios_si_no_hay_ninguno() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let (tarea, escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    // Sin sesiones de escritorio no hay difusión: la ventana arranca vacía.
    let siguiente = tokio::time::timeout(
        Duration::from_millis(300),
        siguiente_framed::<MensajeServidor, _>(&mut lector),
    )
    .await;
    assert!(siguiente.is_err(), "no debería llegar ningún mensaje");
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

#[tokio::test]
async fn una_peticion_fuera_del_rango_se_rechaza_con_el_mismo_peticion_id() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, 5999);
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: 7,
            host_id: host,
        },
    )
    .await;
    let mensaje = esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::Error { .. })
    })
    .await;
    match mensaje {
        MensajeServidor::Error {
            mensaje,
            peticion_id,
        } => {
            assert_eq!(peticion_id, Some(7));
            assert!(mensaje.contains("rango de escritorios"), "{mensaje}");
        }
        otro => panic!("se esperaba Error y llegó {otro:?}"),
    }
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

#[tokio::test]
async fn un_host_ssh_no_se_abre_como_escritorio() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    // Host SSH (el de la base de datos por defecto) contra un puerto cerrado.
    let host = {
        let almacen = magi::almacen::abrir(&rutas.base_datos()).unwrap();
        let id = hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: "web-01".to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: 22,
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
        almacen.cerrar().unwrap();
        id
    };
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    let mensaje = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(PETICION),
                ..
            }
        )
    })
    .await;
    let MensajeServidor::Error { mensaje, .. } = mensaje else {
        unreachable!()
    };
    assert!(mensaje.contains("edita el host"), "{mensaje}");
    assert!(mensaje.contains("host SSH"), "{mensaje}");
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

#[tokio::test]
async fn una_fila_invalida_dice_que_edites_el_host() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let host = {
        let almacen = magi::almacen::abrir(&rutas.base_datos()).unwrap();
        let mut modelo = DatosHost {
            nombre: "vnc-roto".to_string(),
            direccion: "127.0.0.1".to_string(),
            puerto: 5901,
            tipo: magi::escritorio::TipoHost::Vnc,
            ..DatosHost::default()
        };
        // «pantalla» solo vale en un host voxl: la fila es inválida al leerla.
        modelo.escritorio_opciones.pantalla = Some(":1".to_string());
        let id = hosts::crear(almacen.conexion(), &modelo, Origen::Manual).unwrap();
        almacen.cerrar().unwrap();
        id
    };
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    let MensajeServidor::Error { mensaje, .. } = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(PETICION),
                ..
            }
        )
    })
    .await
    else {
        unreachable!()
    };
    assert!(mensaje.contains("edita el host"), "{mensaje}");
    assert!(mensaje.contains("pantalla"), "{mensaje}");
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

#[tokio::test]
async fn un_escritorio_que_no_responde_queda_fallido_y_se_anota() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    // Un puerto cerrado de 127.0.0.1: el TCP no conecta.
    let cerrado = {
        let escucha = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        escucha.local_addr().unwrap().port()
    };
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, cerrado);
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    // Primero se difunde el estado «abriendo» y después el error.
    let mut vio_abriendo = false;
    let mut lista_final = None;
    let error = loop {
        match esperar(&mut lector, |_| true).await {
            MensajeServidor::Escritorios { lista } => {
                if lista
                    .iter()
                    .any(|info| info.estado == magi::escritorio::EstadoEscritorio::Abriendo)
                {
                    vio_abriendo = true;
                }
                lista_final = Some(lista);
            }
            MensajeServidor::Error {
                mensaje,
                peticion_id,
            } => break (mensaje, peticion_id),
            _ => {}
        }
    };
    assert!(vio_abriendo, "no se difundió el estado abriendo");
    assert_eq!(error.1, Some(PETICION));
    assert!(error.0.contains("no responde"), "{}", error.0);
    // La sesión desaparece de la lista (difusión vacía) y queda la anotación
    // de fallo.
    assert!(
        lista_final.as_ref().is_some_and(|lista| lista.is_empty()),
        "la sesión fallida se quita: {lista_final:?}"
    );
    assert_eq!(
        esperar_anotacion(&rutas, magi::registro::ESCRITORIO_FALLIDO).await,
        1
    );
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

#[tokio::test]
async fn pedir_enlace_de_un_escritorio_que_no_existe_da_error() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::PedirEnlace {
            peticion_id: PETICION,
            escritorio_id: 99,
        },
    )
    .await;
    let MensajeServidor::Error {
        mensaje,
        peticion_id,
    } = esperar(&mut lector, |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::Error {
                peticion_id: Some(PETICION),
                ..
            }
        )
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(peticion_id, Some(PETICION));
    assert!(mensaje.contains("ya no existe"), "{mensaje}");
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}
// ------------------------------------------------------- VNC de prueba

/// Servidor RFB mínimo en proceso: saludo 3.8, seguridad None y ServerInit
/// 1920×1080. Después ignora lo que mande el cliente hasta que cierre. Basta
/// para que el cliente VNC de MAGI dé la sesión por abierta.
/// Si «cortar» es verdadero, cada conexión se cierra poco después de quedar
/// abierta, para probar la caída y la reconexión de la sesión.
async fn vnc_falso(cortar: bool) -> (u16, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("escuchando el VNC de prueba");
    let puerto = escucha.local_addr().expect("dirección local").port();
    let tarea = tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            // Saludo 3.8, seguridad None, SecurityResult, ClientInit y
            // ServerInit 1920×1080.
            let listo = async {
                flujo.write_all(b"RFB 003.008\n").await.ok()?;
                let mut saludo = [0u8; 12];
                flujo.read_exact(&mut saludo).await.ok()?;
                // Un solo tipo de seguridad: None (1).
                flujo.write_all(&[1, 1]).await.ok()?;
                let mut elegida = [0u8; 1];
                flujo.read_exact(&mut elegida).await.ok()?;
                flujo.write_all(&0u32.to_be_bytes()).await.ok()?;
                let mut client_init = [0u8; 1];
                flujo.read_exact(&mut client_init).await.ok()?;
                let mut init = Vec::new();
                init.extend_from_slice(&1920u16.to_be_bytes());
                init.extend_from_slice(&1080u16.to_be_bytes());
                // Formato de píxel: 32 bpp, 24 de profundidad, true-colour.
                init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
                init.extend_from_slice(&4u32.to_be_bytes());
                init.extend_from_slice(b"magi");
                flujo.write_all(&init).await.ok()?;
                Some(())
            }
            .await;
            if listo.is_none() {
                continue;
            }
            if cortar {
                let mut bufer = [0u8; 256];
                let _ =
                    tokio::time::timeout(Duration::from_millis(500), flujo.read(&mut bufer)).await;
                tokio::time::sleep(Duration::from_millis(300)).await;
                let _ = flujo.shutdown().await;
                continue;
            }
            let mut bufer = [0u8; 4096];
            loop {
                match flujo.read(&mut bufer).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        }
    });
    (puerto, tarea)
}

/// Detalles de las anotaciones de un tipo, de la más reciente a la más
/// antigua.
fn detalles(rutas: &magi::config::Rutas, tipo: &str) -> Vec<String> {
    let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
    let filas = {
        let mut sentencia = almacen
            .conexion()
            .prepare("SELECT detalle FROM REGISTRO WHERE tipo = ?1 ORDER BY id DESC")
            .expect("preparando");
        sentencia
            .query_map([tipo], |fila| fila.get::<_, String>(0))
            .expect("consultando")
            .collect::<rusqlite::Result<Vec<String>>>()
            .expect("leyendo")
    };
    almacen.cerrar().expect("cerrando magi.db");
    filas
}

async fn esperar_detalle(rutas: &magi::config::Rutas, tipo: &str, aguja: &str) -> bool {
    let fin = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if detalles(rutas, tipo)
            .iter()
            .any(|detalle| detalle.contains(aguja))
        {
            return true;
        }
        if tokio::time::Instant::now() >= fin {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn abrir_un_vnc_y_cerrarlo_anota_abierto_y_cerrado() {
    let entorno = entorno();
    let (puerto, _vnc) = vnc_falso(false).await;
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, puerto);
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;

    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    let mut vio_abierto = false;
    let (escritorio_id, enlace) = loop {
        match esperar(&mut lector, |_| true).await {
            MensajeServidor::Escritorios { lista } => {
                if lista
                    .iter()
                    .any(|info| info.estado == magi::escritorio::EstadoEscritorio::Abierto)
                {
                    vio_abierto = true;
                }
            }
            MensajeServidor::EscritorioAbierto {
                peticion_id,
                escritorio_id,
                enlace,
            } => {
                assert_eq!(peticion_id, PETICION);
                break (escritorio_id, enlace);
            }
            MensajeServidor::Error {
                mensaje,
                peticion_id,
            } => panic!("la apertura falló ({peticion_id:?}): {mensaje}"),
            _ => {}
        }
    };
    assert!(vio_abierto, "no se difundió el estado abierto");
    assert!(
        enlace.starts_with("http://127.0.0.1:") && enlace.contains("/entrar?t="),
        "enlace inesperado: {enlace}"
    );

    // PedirEnlace responde con otro enlace de un solo uso para la misma
    // sesión.
    enviar_a(
        &mut escritura,
        &MensajeCliente::PedirEnlace {
            peticion_id: PETICION + 1,
            escritorio_id,
        },
    )
    .await;
    let MensajeServidor::Enlace {
        peticion_id,
        escritorio_id: id_enlace,
        enlace: enlace_nuevo,
    } = esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::Enlace { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(peticion_id, PETICION + 1);
    assert_eq!(id_enlace, escritorio_id);
    assert_ne!(enlace_nuevo, enlace);
    assert!(
        esperar_detalle(&rutas, magi::registro::ESCRITORIO_ABIERTO, "VNC").await,
        "el registro del abierto lleva el tipo"
    );
    assert!(esperar_detalle(&rutas, magi::registro::ESCRITORIO_ABIERTO, "directo").await);

    // Cerrar desde la ventana: la lista queda vacía y se anota el cierre con
    // su motivo.
    enviar_a(
        &mut escritura,
        &MensajeCliente::CerrarEscritorio { escritorio_id },
    )
    .await;
    esperar(
        &mut lector,
        |mensaje| matches!(mensaje, MensajeServidor::Escritorios { lista } if lista.is_empty()),
    )
    .await;
    assert!(
        esperar_detalle(
            &rutas,
            magi::registro::ESCRITORIO_CERRADO,
            "cerrado por el usuario"
        )
        .await
    );
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

#[tokio::test]
async fn la_gracia_cierra_un_escritorio_sin_visor() {
    let mut entorno = entorno();
    // La gracia mínima permitida, para que la prueba no tarde un minuto.
    entorno.config.escritorio.gracia_seg = 10;
    let (puerto, _vnc) = vnc_falso(false).await;
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, puerto);
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::EscritorioAbierto { .. })
    })
    .await;
    // Sin visor, la gracia cierra la sesión: la última difusión es la lista
    // vacía.
    let cierre = tokio::time::timeout(
        Duration::from_secs(30),
        esperar(
            &mut lector,
            |mensaje| matches!(mensaje, MensajeServidor::Escritorios { lista } if lista.is_empty()),
        ),
    )
    .await;
    assert!(cierre.is_ok(), "la gracia no cerró el escritorio en 30 s");
    assert!(
        esperar_detalle(
            &rutas,
            magi::registro::ESCRITORIO_CERRADO,
            "sin visor durante 10 s"
        )
        .await
    );
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}
#[tokio::test]
async fn un_servidor_con_escritorios_no_se_deja_sustituir_sin_preguntar() {
    let entorno = entorno();
    let (puerto, _vnc) = vnc_falso(false).await;
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, puerto);
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::EscritorioAbierto { .. })
    })
    .await;

    // Una ventana de una versión anterior pregunta: el servidor le cuenta que
    // custodia un escritorio abierto (T67).
    let mut viejo = tokio::net::UnixStream::connect(servidor::ruta_socket(&rutas))
        .await
        .expect("conectando la ventana antigua");
    enviar(
        &mut viejo,
        &MensajeCliente::Hola {
            version: 5,
            pid: 43,
        },
    )
    .await;
    let (lectura, _escritura_vieja) = viejo.into_split();
    let mut lector_viejo = tokio_util::codec::FramedRead::new(lectura, magi::protocolo::codec());
    let rechazo: MensajeServidor = siguiente_framed(&mut lector_viejo).await;
    match rechazo {
        MensajeServidor::VersionIncompatible {
            version, trabajo, ..
        } => {
            assert_eq!(version, VERSION_PROTOCOLO);
            let trabajo = trabajo.expect("el rechazo informa del trabajo");
            assert_eq!(trabajo.escritorios, 1, "cuenta el escritorio abierto");
        }
        otro => panic!("se esperaba VersionIncompatible y llegó {otro:?}"),
    }
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}

#[tokio::test]
async fn el_apagado_cierra_los_escritorios_y_los_anota() {
    let entorno = entorno();
    let (puerto, _vnc) = vnc_falso(false).await;
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, puerto);
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::EscritorioAbierto { .. })
    })
    .await;

    // Parar el servidor con el escritorio abierto: se cierra y se anota.
    enviar_a(&mut escritura, &MensajeCliente::Parar).await;
    let _ = tokio::time::timeout(Duration::from_secs(15), tarea).await;
    assert!(
        esperar_detalle(
            &rutas,
            magi::registro::ESCRITORIO_CERRADO,
            "el servidor se apaga"
        )
        .await,
        "el apagado anota el cierre del escritorio"
    );
    assert!(
        !servidor::ruta_socket(&rutas).exists(),
        "el socket queda borrado"
    );
}
// ------------------------------------------------------- voxl de prueba

const PLANTILLA_VOXL_FALSO: &str = r#"#!/bin/sh
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1
if [ "$1" = "--version" ]; then
  echo "voxl 0.3.3"
  exit 0
fi
printf '%s\n' "$@" > "$dir/argumentos"
tr '\0' '\n' < "/proc/$$/cmdline" > "$dir/cmdline"
printf '%s' "$VOXL_PASSWORD" > "$dir/clave"
printf '%s' "$DISPLAY" > "$dir/display"
printf '%s' "$$" > "$dir/pid"
trap 'printf "parada\n" > "$dir/parada"; exit 0' TERM INT
printf 'vivo\n' > "$dir/arrancado"
while :; do sleep 0.2; done
"#;

fn crear_voxl_falso(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let ruta = dir.join("voxl-falso");
    std::fs::write(&ruta, PLANTILLA_VOXL_FALSO).expect("escribiendo el voxl falso");
    std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755))
        .expect("permisos del voxl falso");
    ruta
}

/// HTTP mínimo que hace de voxl en el host: 200 en /healthz y en el resto de
/// GET (para comprobar que el túnel de la sesión llega hasta él).
async fn servicio_voxl(retraso_salud: Duration) -> (u16, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("escuchando el voxl de prueba");
    let puerto = escucha.local_addr().expect("dirección").port();
    let tarea = tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut peticion = Vec::new();
                let mut bufer = [0u8; 1024];
                loop {
                    match flujo.read(&mut bufer).await {
                        Ok(0) | Err(_) => return,
                        Ok(leidos) => {
                            peticion.extend_from_slice(&bufer[..leidos]);
                            if peticion.windows(4).any(|trozo| trozo == b"\r\n\r\n")
                                || peticion.len() > 8192
                            {
                                break;
                            }
                        }
                    }
                }
                let primera = String::from_utf8_lossy(&peticion);
                if primera.starts_with("GET /healthz") {
                    tokio::time::sleep(retraso_salud).await;
                }
                let respuesta =
                    b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nvoxl";
                let _ = flujo.write_all(respuesta).await;
                let _ = flujo.shutdown().await;
            });
        }
    });
    (puerto, tarea)
}

async fn esperar_fichero(ruta: &std::path::Path, plazo: Duration) -> String {
    let fin = tokio::time::Instant::now() + plazo;
    loop {
        if let Ok(texto) = std::fs::read_to_string(ruta) {
            if !texto.is_empty() {
                return texto;
            }
        }
        assert!(
            tokio::time::Instant::now() < fin,
            "no apareció el fichero {}",
            ruta.display()
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Petición HTTP simple al servidor del visor (Host y Origin propios).
async fn peticion_http(puerto: u16, peticion: &str) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut flujo = tokio::net::TcpStream::connect(("127.0.0.1", puerto))
        .await
        .expect("conectando al visor");
    flujo
        .write_all(peticion.as_bytes())
        .await
        .expect("escribiendo la petición");
    let mut respuesta = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), flujo.read_to_end(&mut respuesta)).await;
    String::from_utf8_lossy(&respuesta).to_string()
}

fn puerto_del_enlace(enlace: &str) -> u16 {
    let resto = enlace
        .strip_prefix("http://127.0.0.1:")
        .expect("el enlace es del visor");
    let (puerto, _) = resto.split_once('/').expect("enlace con ruta");
    puerto.parse().expect("puerto del visor")
}

#[tokio::test]
async fn abrir_un_voxl_por_ssh_y_cerrarlo_para_el_script() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let (puerto_voxl, _servicio) = servicio_voxl(Duration::ZERO).await;
    let temporal = tempfile::tempdir().expect("directorio del voxl falso");
    let voxl_falso = crear_voxl_falso(temporal.path());

    // Host voxl que se abre a través del host SSH del escenario.
    let host = {
        use magi::escritorio::{OpcionesEscritorio, TipoHost};
        let mut opciones = OpcionesEscritorio::por_defecto(TipoHost::Voxl);
        opciones.pantalla = Some(":1".to_string());
        opciones.programa = Some(voxl_falso.display().to_string());
        opciones.portapapeles = true;
        let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).expect("magi.db");
        let id = hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: "voxl-1".to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: puerto_voxl,
                tipo: TipoHost::Voxl,
                salto_host_id: Some(escenario.hosts[0]),
                escritorio_opciones: opciones,
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("creando el host voxl");
        almacen.cerrar().expect("cerrando magi.db");
        id
    };

    escenario
        .enviar(&MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        })
        .await;
    let mut abierto = None;
    while abierto.is_none() {
        match escenario.siguiente().await {
            MensajeServidor::EscritorioAbierto {
                escritorio_id,
                enlace,
                ..
            } => abierto = Some((escritorio_id, enlace)),
            MensajeServidor::Error { mensaje, .. } => {
                panic!("la apertura de voxl falló: {mensaje}")
            }
            _ => {}
        }
    }
    let (escritorio_id, enlace) = abierto.expect("abierto");
    assert!(enlace.contains("/entrar?t="), "enlace: {enlace}");

    // El script recibió los argumentos validados, la pantalla y la contraseña
    // por la entrada estándar (nunca en la línea de órdenes).
    let argumentos =
        esperar_fichero(&temporal.path().join("argumentos"), Duration::from_secs(10)).await;
    for esperado in [
        "--addr=127.0.0.1".to_string(),
        format!("--port={puerto_voxl}"),
        "--enable-resize=true".to_string(),
        "--file-transfers=none".to_string(),
        "--audio-enabled=false".to_string(),
        "--microphone-enabled=false".to_string(),
        "--enable-clipboard=true".to_string(),
    ] {
        assert!(
            argumentos.contains(&esperado),
            "falta {esperado}: {argumentos}"
        );
    }
    let display = esperar_fichero(&temporal.path().join("display"), Duration::from_secs(5)).await;
    assert_eq!(display, ":1");
    let clave = esperar_fichero(&temporal.path().join("clave"), Duration::from_secs(5)).await;
    assert_eq!(
        clave.len(),
        32,
        "la contraseña de voxl es aleatoria: {clave}"
    );
    assert!(clave.chars().all(|caracter| caracter.is_ascii_hexdigit()));
    let cmdline = esperar_fichero(&temporal.path().join("cmdline"), Duration::from_secs(5)).await;
    assert!(
        !cmdline.contains(&clave),
        "la contraseña no puede ir en la línea de órdenes"
    );

    // El enlace del visor se canjea por la URL de voxl a través del túnel de la
    // sesión, y el túnel llega al servicio del host.
    let puerto_visor = puerto_del_enlace(&enlace);
    let token = enlace
        .split_once("t=")
        .map(|(_, token)| token)
        .unwrap_or_default();
    let cuerpo = format!("{{\"enlace\":\"{token}\"}}");
    let peticion = format!(
        "POST /api/canjear HTTP/1.1\r\nHost: 127.0.0.1:{puerto_visor}\r\nOrigin: http://127.0.0.1:{puerto_visor}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{cuerpo}",
        cuerpo.len()
    );
    let respuesta = peticion_http(puerto_visor, &peticion).await;
    assert!(
        respuesta.contains("\"voxl\":\"http://127.0.0.1:"),
        "el canje debe devolver la URL de voxl: {respuesta}"
    );
    let cuerpo_respuesta = respuesta.split("\r\n\r\n").nth(1).unwrap_or_default();
    let inicio = cuerpo_respuesta
        .find("http://127.0.0.1:")
        .expect("URL de voxl en la respuesta")
        + "http://127.0.0.1:".len();
    let resto = &cuerpo_respuesta[inicio..];
    let fin_puerto = resto
        .find(['/', '?'])
        .expect("puerto del túnel en la URL de voxl");
    let puerto_tunel: u16 = resto[..fin_puerto].parse().expect("puerto del túnel");
    let pagina = peticion_http(
        puerto_tunel,
        &format!(
            "GET /?token={clave} HTTP/1.1\r\nHost: 127.0.0.1:{puerto_tunel}\r\nConnection: close\r\n\r\n"
        ),
    )
    .await;
    assert!(
        pagina.starts_with("HTTP/1.1 200"),
        "el túnel de la sesión debe llegar a voxl: {pagina}"
    );

    // Cerrar la sesión para el script (fin de stdin -> TERM) y anota el cierre.
    escenario
        .enviar(&MensajeCliente::CerrarEscritorio { escritorio_id })
        .await;
    escenario
        .esperar(
            |mensaje| matches!(mensaje, MensajeServidor::Escritorios { lista } if lista.is_empty()),
        )
        .await;
    esperar_fichero(&temporal.path().join("parada"), Duration::from_secs(10)).await;
    assert!(
        esperar_detalle(
            &escenario.rutas().clone(),
            magi::registro::ESCRITORIO_CERRADO,
            "cerrado por el usuario"
        )
        .await
    );
}
async fn esperar_cuenta(rutas: &magi::config::Rutas, tipo: &str, minimo: usize) -> usize {
    let fin = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let cuenta = anotaciones(rutas, tipo);
        if cuenta >= minimo || tokio::time::Instant::now() >= fin {
            return cuenta;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn reconectar_un_vnc_caido_reabre_la_sesion() {
    let entorno = entorno();
    // El VNC de prueba cierra cada conexión poco después de abrirla: la
    // sesión pasa a caída y se puede reconectar.
    let (puerto, _vnc) = vnc_falso(true).await;
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, puerto);
    let (tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    let escritorio_id = loop {
        match esperar(&mut lector, |_| true).await {
            MensajeServidor::EscritorioAbierto { escritorio_id, .. } => break escritorio_id,
            MensajeServidor::Error { mensaje, .. } => panic!("la apertura falló: {mensaje}"),
            _ => {}
        }
    };

    // La conexión se cae sola: la sesión queda caída con su motivo.
    esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::Escritorios { lista }
        if lista.iter().any(|info| {
            info.estado == magi::escritorio::EstadoEscritorio::Caido
        }))
    })
    .await;

    // Reconectar conserva el id de la sesión y contesta con un enlace nuevo.
    enviar_a(
        &mut escritura,
        &MensajeCliente::ReconectarEscritorio {
            peticion_id: PETICION + 2,
            escritorio_id,
        },
    )
    .await;
    let mensaje = esperar(&mut lector, |mensaje| {
        matches!(mensaje, MensajeServidor::EscritorioAbierto { peticion_id, .. } if *peticion_id == PETICION + 2)
    })
    .await;
    let MensajeServidor::EscritorioAbierto {
        escritorio_id: id,
        enlace,
        ..
    } = mensaje
    else {
        unreachable!()
    };
    assert_eq!(id, escritorio_id, "la reconexión conserva la sesión");
    assert!(enlace.contains("/entrar?t="), "enlace: {enlace}");
    assert!(
        esperar_cuenta(&rutas, magi::registro::ESCRITORIO_ABIERTO, 2).await >= 2,
        "se anota cada apertura"
    );

    enviar_a(
        &mut escritura,
        &MensajeCliente::CerrarEscritorio { escritorio_id },
    )
    .await;
    esperar(
        &mut lector,
        |mensaje| matches!(mensaje, MensajeServidor::Escritorios { lista } if lista.is_empty()),
    )
    .await;
    drop(escritura);
    let _ = tokio::time::timeout(Duration::from_secs(10), tarea).await;
}
#[tokio::test]
async fn voxl_lento_como_unica_sesion_abre_y_da_enlace() {
    // Regresión C1: si voxl tarda más de un segundo en arrancar, la revisión
    // de la gracia no puede parar el servidor del visor antes de que la sesión
    // quede registrada; al acabar, el cliente recibe su enlace.
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let (puerto_voxl, _servicio) = servicio_voxl(Duration::from_millis(1500)).await;
    let temporal = tempfile::tempdir().expect("directorio del voxl falso");
    let voxl_falso = crear_voxl_falso(temporal.path());
    let host = {
        use magi::escritorio::{OpcionesEscritorio, TipoHost};
        let mut opciones = OpcionesEscritorio::por_defecto(TipoHost::Voxl);
        opciones.pantalla = Some(":0".to_string());
        opciones.programa = Some(voxl_falso.display().to_string());
        let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).expect("magi.db");
        let id = hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: "voxl-lento".to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: puerto_voxl,
                tipo: TipoHost::Voxl,
                salto_host_id: Some(escenario.hosts[0]),
                escritorio_opciones: opciones,
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("creando el host voxl");
        almacen.cerrar().expect("cerrando magi.db");
        id
    };
    escenario
        .enviar(&MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::EscritorioAbierto { .. } | MensajeServidor::Error { .. }
            )
        })
        .await;
    let escritorio_id = match mensaje {
        MensajeServidor::EscritorioAbierto {
            escritorio_id,
            enlace,
            ..
        } => {
            assert!(
                enlace.starts_with("http://127.0.0.1:") && enlace.contains("/entrar?t="),
                "enlace inesperado: {enlace}"
            );
            escritorio_id
        }
        MensajeServidor::Error { mensaje, .. } => {
            panic!("la apertura de voxl falló: {mensaje}")
        }
        otro => panic!("se esperaba EscritorioAbierto y llegó {otro:?}"),
    };
    escenario
        .enviar(&MensajeCliente::CerrarEscritorio { escritorio_id })
        .await;
    escenario
        .esperar(
            |mensaje| matches!(mensaje, MensajeServidor::Escritorios { lista } if lista.is_empty()),
        )
        .await;
}
#[tokio::test]
async fn dos_aperturas_del_mismo_host_contestan_una_abierta_y_una_error() {
    // Regresión M8: la apertura que pierde la carrera (dos ↵ seguidos o dos
    // ventanas) ya no se queda «abriendo»: recibe Error con su peticion_id.
    let entorno = entorno();
    let (puerto, _vnc) = vnc_falso(false).await;
    let rutas = entorno.rutas.clone();
    let host = crear_host(&rutas, magi::escritorio::TipoHost::Vnc, puerto);
    let (_tarea, mut escritura, mut lector, _) = arrancar(&rutas, &entorno.config).await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION,
            host_id: host,
        },
    )
    .await;
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirEscritorio {
            peticion_id: PETICION + 1,
            host_id: host,
        },
    )
    .await;

    let mut abierta = None;
    let mut errores = Vec::new();
    let fin = tokio::time::Instant::now() + Duration::from_secs(20);
    while (abierta.is_none() || errores.is_empty()) && tokio::time::Instant::now() < fin {
        match esperar(&mut lector, |_| true).await {
            MensajeServidor::EscritorioAbierto { peticion_id, .. } => abierta = Some(peticion_id),
            MensajeServidor::Error {
                peticion_id: Some(peticion_id),
                ..
            } if peticion_id == PETICION || peticion_id == PETICION + 1 => {
                errores.push(peticion_id)
            }
            _ => {}
        }
    }
    let abierta = abierta.expect("una de las dos aperturas abre");
    assert_eq!(
        errores.len(),
        1,
        "la otra recibe Error: abierta {abierta}, errores {errores:?}"
    );
    assert_ne!(
        errores[0], abierta,
        "no puede haber dos respuestas a la vez"
    );
}
