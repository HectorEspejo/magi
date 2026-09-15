//! Pruebas del servidor de sesiones con socket en directorio temporal:
//! arranque, saludo versionado, apagado por inactividad, lock duplicado y
//! mensajes mal formados.

mod comun;

use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixStream;

use magi::config::Config;
use magi::protocolo::{MensajeCliente, MensajeServidor, VERSION_PROTOCOLO};
use magi::servidor;

use comun::*;

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
    // La versión que viaja es la **del servidor**, que es lo que permite al
    // cliente decir si el viejo es él o el servidor.
    match respuesta {
        MensajeServidor::VersionIncompatible { version } => {
            assert_eq!(version, VERSION_PROTOCOLO);
        }
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

/// Un servidor v2 no coopera con un cliente v1: el protocolo cambió y el
/// cliente antiguo no lo entendería.
#[tokio::test]
async fn un_cliente_de_la_version_anterior_tampoco_coopera() {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    let tarea = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let mut lector = cliente(&servidor::ruta_socket(&rutas), 1).await;
    let respuesta: MensajeServidor = siguiente(&mut lector).await;
    match respuesta {
        MensajeServidor::VersionIncompatible { version } => {
            assert_eq!(version, VERSION_PROTOCOLO);
        }
        otro => panic!("se esperaba VersionIncompatible, llegó {otro:?}"),
    }
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

// -------------------------------------------------------------- flujo completo

/// El solicitante recibe el diálogo de contraseña por el protocolo y su
/// respuesta abre la sesión (la pieza que fallaba con el llavero).
#[tokio::test]
async fn el_dialogo_de_contrasena_viaja_y_la_respuesta_abre_la_sesion() {
    use std::sync::Arc;

    use magi::almacen::{hosts, Almacen};
    use magi::modelo::{DatosHost, IdentidadRef, Origen};
    use russh::server::Server as _;

    // 1. Servidor SSH de pruebas con huella conocida de antemano.
    let _dir_ssh = tempfile::tempdir().unwrap();
    let clave = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .unwrap();
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave.clone()],
        ..Default::default()
    });
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let puerto_ssh = escucha.local_addr().unwrap().port();
    let tarea_ssh = tokio::spawn(async move {
        let mut servidor = ServidorSesion::default();
        let _ = servidor.run_on_socket(config, &escucha).await;
    });

    // 2. Entorno MAGI con el host apuntando al servidor de pruebas.
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    std::fs::create_dir_all(rutas.dir_ssh()).unwrap();
    let known_hosts = rutas.fichero_known_hosts();
    std::fs::write(
        &known_hosts,
        format!(
            "[127.0.0.1]:{puerto_ssh} {}\n",
            clave.public_key().to_openssh().unwrap()
        ),
    )
    .unwrap();
    {
        let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
        let host_id = hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: "prueba".to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: puerto_ssh,
                usuario: Some("hector".to_string()),
                identidad_ref: IdentidadRef::Contrasena,
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
        almacen.cerrar().unwrap();
        let _ = host_id;
    }

    // 3. Servidor de sesiones y cliente del protocolo.
    let tarea_magi = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let mut stream = UnixStream::connect(servidor::ruta_socket(&rutas))
        .await
        .expect("conectando al servidor de sesiones");
    enviar(
        &mut stream,
        &MensajeCliente::Hola {
            version: VERSION_PROTOCOLO,
            pid: 42,
        },
    )
    .await;
    let (lectura, mut escritura) = stream.into_split();
    let mut lector = tokio_util::codec::FramedRead::new(lectura, magi::protocolo::codec());
    let bienvenida: MensajeServidor = siguiente_framed(&mut lector).await;
    assert!(matches!(bienvenida, MensajeServidor::Bienvenida { .. }));

    // El id del host lo da la BD: «prueba» es el primero.
    let host_id = {
        let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
        let host = magi::almacen::hosts::por_nombre(almacen.conexion(), "prueba")
            .unwrap()
            .expect("el host de prueba existe");
        host.id
    };
    enviar_a(
        &mut escritura,
        &MensajeCliente::AbrirSesion {
            host_id,
            cols: 80,
            filas: 24,
        },
    )
    .await;

    // 4. El diálogo llega al solicitante y la respuesta abre la sesión.
    let abierta;
    let plazo = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < plazo,
            "el diálogo o la apertura no llegaron"
        );
        let mensaje: MensajeServidor =
            match tokio::time::timeout(Duration::from_secs(3), siguiente_framed(&mut lector)).await
            {
                Ok(mensaje) => {
                    eprintln!("[TRAZA-TEST] recibido: {mensaje:?}");
                    mensaje
                }
                Err(_) => panic!("el servidor no manda nada en 3 s"),
            };
        match mensaje {
            MensajeServidor::PideContrasena { sesion_id, .. } => {
                enviar_a(
                    &mut escritura,
                    &MensajeCliente::Contrasena {
                        sesion_id,
                        contrasena: magi::protocolo::Secreto::nuevo("secreta"),
                        recordar: false,
                    },
                )
                .await;
            }
            MensajeServidor::PideFrase { .. } | MensajeServidor::HuellaDesconocida { .. } => {
                panic!("se pidió lo que no toca: huella o frase")
            }
            MensajeServidor::Sesiones { lista } => {
                if let Some(sesion) = lista.first() {
                    match sesion.estado {
                        magi::protocolo::EstadoSesionRemota::Abierta => {
                            abierta = true;
                            break;
                        }
                        magi::protocolo::EstadoSesionRemota::Caida => {
                            panic!("la sesión quedó caída: {:?}", sesion.motivo);
                        }
                        _ => {}
                    }
                }
            }
            MensajeServidor::Estado {
                estado: magi::protocolo::EstadoSesionRemota::Cerrada,
                motivo,
                ..
            } => {
                panic!("apertura fallida: {:?}", motivo);
            }
            _ => {}
        }
    }
    assert!(abierta);

    // 5. Limpieza: cerrar la sesión y parar el servidor.
    drop(tarea_ssh);
    let _ = escritura.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(8), tarea_magi).await;
}

/// Regresión: los datos que llegan del servidor acaban en el MISMO registro de
/// pantallas que usa la UI (`cliente.pantallas()`), no en uno privado de la
/// tarea de lectura.
#[tokio::test]
async fn el_cliente_vuelca_los_datos_en_su_registro_de_pantallas() {
    use std::sync::Arc;

    use magi::almacen::{hosts, Almacen};
    use magi::modelo::{DatosHost, IdentidadRef, Origen};
    use russh::server::Server as _;

    // 1. Servidor SSH de pruebas con clave pública aceptada.
    let _dir_ssh = tempfile::tempdir().unwrap();
    let clave_servidor = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .unwrap();
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave_servidor.clone()],
        ..Default::default()
    });
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let puerto_ssh = escucha.local_addr().unwrap().port();
    let tarea_ssh = tokio::spawn(async move {
        let mut servidor = ServidorSesion::default();
        let _ = servidor.run_on_socket(config, &escucha).await;
    });

    // 2. Entorno MAGI con clave de cliente y huella conocida.
    let entorno = entorno();
    let rutas = entorno.rutas.clone();
    std::fs::create_dir_all(rutas.dir_ssh()).unwrap();
    let clave_cliente = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .unwrap();
    let ruta_clave = rutas.dir_ssh().join("prueba_cliente");
    std::fs::write(
        &ruta_clave,
        clave_cliente.to_openssh(ssh_key::LineEnding::LF).unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&ruta_clave, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let known_hosts = rutas.fichero_known_hosts();
    std::fs::write(
        &known_hosts,
        format!(
            "[127.0.0.1]:{puerto_ssh} {}\n",
            clave_servidor.public_key().to_openssh().unwrap()
        ),
    )
    .unwrap();
    {
        let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
        hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: "prueba".to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: puerto_ssh,
                usuario: Some("hector".to_string()),
                identidad_ref: IdentidadRef::Fichero(ruta_clave.display().to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
        almacen.cerrar().unwrap();
    }

    // 3. Servidor de sesiones y cliente real.
    let tarea_magi = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;

    let (tx_eventos, mut eventos) = tokio::sync::mpsc::unbounded_channel();
    let cliente = magi::cliente::conectar(&rutas, tx_eventos)
        .await
        .expect("el cliente conecta con el servidor");

    let host_id = {
        let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
        magi::almacen::hosts::por_nombre(almacen.conexion(), "prueba")
            .unwrap()
            .expect("el host de prueba existe")
            .id
    };
    cliente.enviar(magi::protocolo::MensajeCliente::AbrirSesion {
        host_id,
        cols: 80,
        filas: 24,
    });

    // 4. Espera la sesión abierta y adjunta.
    let mut sesion_id = None;
    let mut adjuntado = false;
    let plazo = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut contenido = None;
    while tokio::time::Instant::now() < plazo {
        let Some(evento) = eventos.recv().await else {
            break;
        };
        match evento {
            magi::app::Evento::Servidor(MensajeServidor::Sesiones { lista }) => {
                if let Some(sesion) = lista.iter().find(|s| s.host_id == host_id) {
                    if sesion.estado == magi::protocolo::EstadoSesionRemota::Abierta
                        && sesion_id.is_none()
                    {
                        sesion_id = Some(sesion.id);
                    }
                }
            }
            magi::app::Evento::Pantallas(ids) => {
                if let Some(id) = ids.first() {
                    if let Some(texto) = cliente.pantallas().contenido(*id) {
                        if texto.contains("MARCA_CLIENTE") {
                            contenido = Some(texto);
                        }
                    }
                }
            }
            _ => {}
        }
        if let (Some(id), false) = (sesion_id, adjuntado) {
            cliente.enviar(magi::protocolo::MensajeCliente::Adjuntar {
                sesion_id: id,
                cols: 80,
                filas: 24,
            });
            adjuntado = true;
        }
        if contenido.is_some() {
            break;
        }
    }

    let contenido = contenido.expect("el registro de pantallas del cliente debe recibir los datos");
    assert!(
        contenido.contains("MARCA_CLIENTE"),
        "el volcado debe llegar al registro compartido: {contenido:?}"
    );

    drop(cliente);
    drop(tarea_ssh);
    let _ = tokio::time::timeout(Duration::from_secs(8), tarea_magi).await;
}
