use std::sync::Arc;
use std::time::Duration;

use magi::app::Evento;
use magi::conexion::cliente::{autenticar, Cliente, Contexto};
use magi::conexion::EventoConexion;
use magi::modelo::{Host, IdentidadRef, Origen};
use russh::server::Server as _;
use ssh_key::rand_core::UnwrapErr;
use ssh_key::{getrandom::SysRng, Algorithm, PrivateKey};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

const CONTRASENA_VALIDA: &str = "secreta";

struct Servidor;

struct HandlerAutenticacion;

impl russh::server::Server for Servidor {
    type Handler = HandlerAutenticacion;

    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self::Handler {
        HandlerAutenticacion
    }
}

impl russh::server::Handler for HandlerAutenticacion {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _usuario: &str,
        contrasena: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        if contrasena == CONTRASENA_VALIDA {
            Ok(russh::server::Auth::Accept)
        } else {
            Ok(russh::server::Auth::reject())
        }
    }
}

struct Entorno {
    _dir: tempfile::TempDir,
    _tarea_servidor: tokio::task::JoinHandle<()>,
    puerto: u16,
    known_hosts: std::path::PathBuf,
    hogar: std::path::PathBuf,
}

async fn montar_servidor() -> Entorno {
    let dir = tempfile::tempdir().unwrap();
    let clave = PrivateKey::random(&mut UnwrapErr(SysRng), Algorithm::Ed25519).unwrap();
    let publica = clave.public_key().to_openssh().unwrap();
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![clave],
        ..Default::default()
    });
    let escucha = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let puerto = escucha.local_addr().unwrap().port();
    let tarea_servidor = tokio::spawn(async move {
        let mut servidor = Servidor;
        let _ = servidor.run_on_socket(config, &escucha).await;
    });

    let known_hosts = dir.path().join("known_hosts");
    std::fs::write(&known_hosts, format!("[127.0.0.1]:{puerto} {publica}\n")).unwrap();
    Entorno {
        hogar: dir.path().to_path_buf(),
        _dir: dir,
        _tarea_servidor: tarea_servidor,
        puerto,
        known_hosts,
    }
}

fn host_de_prueba(puerto: u16) -> Host {
    Host {
        id: 1,
        nombre: "servidor-prueba".to_string(),
        grupo_id: None,
        direccion: "127.0.0.1".to_string(),
        puerto,
        usuario: Some("hector".to_string()),
        identidad_ref: IdentidadRef::Contrasena,
        salto_host_id: None,
        multiplexar: false,
        keepalive_seg: None,
        opciones_extra: String::new(),
        servicios: String::new(),
        origen: Origen::Manual,
        ultimo_estado: None,
        ultima_conexion_en: None,
        creado_en: String::new(),
        actualizado_en: String::new(),
        etiquetas: Vec::new(),
        grupo_nombre: None,
        salto_nombre: None,
    }
}

fn contexto_de(
    entorno: &Entorno,
    interactivo: bool,
) -> (Contexto, mpsc::UnboundedReceiver<Evento>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        Contexto {
            known_hosts: entorno.known_hosts.clone(),
            dir_ssh: entorno.hogar.join(".ssh"),
            hogar: entorno.hogar.clone(),
            usuario_local: "hector".to_string(),
            tx,
            interactivo,
        },
        rx,
    )
}

async fn conectar_cliente(
    entorno: &Entorno,
    contexto: &Contexto,
    interactivo: bool,
) -> russh::client::Handle<Cliente> {
    russh::client::connect(
        Arc::new(russh::client::Config::default()),
        ("127.0.0.1", entorno.puerto),
        Cliente::nuevo(
            contexto.tx.clone(),
            1,
            "127.0.0.1",
            entorno.puerto,
            entorno.known_hosts.clone(),
            interactivo,
        ),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn la_contrasena_correcta_autentica_sin_guardarla() {
    let entorno = montar_servidor().await;
    let (contexto, mut eventos) = contexto_de(&entorno, true);
    let mut handle = conectar_cliente(&entorno, &contexto, true).await;
    let host = host_de_prueba(entorno.puerto);
    let tarea = tokio::spawn(async move { autenticar(&mut handle, &host, &contexto).await });

    loop {
        match eventos.recv().await.expect("petición de contraseña") {
            Evento::Conexion(EventoConexion::PideContrasena {
                intento, responder, ..
            }) => {
                assert_eq!(intento, 1);
                responder
                    .send(Some((Zeroizing::new(CONTRASENA_VALIDA.to_string()), false)))
                    .unwrap();
                break;
            }
            Evento::Conexion(EventoConexion::Estado { .. }) => continue,
            _ => panic!("se esperaba PideContrasena"),
        }
    }
    let identidad = tarea.await.unwrap().unwrap();
    assert!(identidad.descripcion.contains("contraseña"));
    assert!(identidad.huella.is_none());
}

#[tokio::test]
async fn la_contrasena_incorrecta_reintenta_tres_veces() {
    let entorno = montar_servidor().await;
    let (contexto, mut eventos) = contexto_de(&entorno, true);
    let mut handle = conectar_cliente(&entorno, &contexto, true).await;
    let host = host_de_prueba(entorno.puerto);
    let tarea = tokio::spawn(async move { autenticar(&mut handle, &host, &contexto).await });

    let mut intentos = 0;
    while let Some(evento) = eventos.recv().await {
        if let Evento::Conexion(EventoConexion::PideContrasena {
            intento, responder, ..
        }) = evento
        {
            intentos += 1;
            assert_eq!(intento, intentos);
            responder
                .send(Some((Zeroizing::new("equivocada".to_string()), false)))
                .unwrap();
        }
    }
    let error = tarea.await.unwrap().unwrap_err().to_string();
    assert_eq!(intentos, 3);
    assert!(error.contains("contraseña incorrecta tras 3 intentos"));
}

#[tokio::test]
async fn el_sondeo_con_llavero_no_dialoga_y_da_instruccion() {
    let entorno = montar_servidor().await;
    let (contexto, mut eventos) = contexto_de(&entorno, false);
    let mut handle = conectar_cliente(&entorno, &contexto, false).await;
    let mut host = host_de_prueba(entorno.puerto);
    host.identidad_ref = IdentidadRef::ContrasenaLlavero;
    let error = autenticar(&mut handle, &host, &contexto)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("llavero"), "{error}");
    assert!(error.contains("conéctate una vez con ↵"), "{error}");
    while let Ok(evento) = eventos.try_recv() {
        assert!(
            !matches!(
                evento,
                Evento::Conexion(EventoConexion::PideContrasena { .. })
            ),
            "el sondeo no debe pedir contraseña"
        );
    }
}

#[tokio::test]
async fn el_sondeo_no_dialoga_con_contrasena() {
    let entorno = montar_servidor().await;
    let (contexto, mut eventos) = contexto_de(&entorno, false);
    let mut handle = conectar_cliente(&entorno, &contexto, false).await;
    let host = host_de_prueba(entorno.puerto);
    let error = autenticar(&mut handle, &host, &contexto)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("no dialoga"), "{error}");
    while let Ok(evento) = eventos.try_recv() {
        assert!(
            !matches!(
                evento,
                Evento::Conexion(EventoConexion::PideContrasena { .. })
            ),
            "el sondeo no debe pedir contraseña"
        );
    }
}
