//! Prueba de extremo a extremo del túnel local del móvil, sin red externa:
//! servidor SSH en proceso (russh) + servicio de eco local como destino.
//!
//! Lo que entra por el agujero del túnel tiene que salir por él y volver. Es la
//! prueba que faltaba en la Fase 11 y que debe fallar si el reenvío no funciona.
//! Desde la Fase 14 el servidor de pruebas compartido vivirá en
//! `comun/servidor_pruebas.rs`; mientras tanto esta prueba conserva el suyo
//! para los canales `direct-tcpip`.

mod comun;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use magi_movil::api::EstadoTunel;
use magi_movil::estado::con_almacen;
use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::tuneles as almacen_tuneles;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::modelo::{DatosHost, DatosTunel, TipoTunel};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use comun::{servir_eco, tunel_por_uuid, Entorno};

// ------------------------------------------------------------ servidor SSH

struct ServidorPruebas {
    mangos: Arc<Mutex<Vec<russh::server::Handle>>>,
}

impl russh::server::Server for ServidorPruebas {
    type Handler = HandlerPruebas;

    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> HandlerPruebas {
        HandlerPruebas {
            mangos: self.mangos.clone(),
        }
    }
}

struct HandlerPruebas {
    mangos: Arc<Mutex<Vec<russh::server::Handle>>>,
}

impl russh::server::Handler for HandlerPruebas {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _usuario: &str,
        contrasena: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        if contrasena == "secreta" {
            Ok(russh::server::Auth::Accept)
        } else {
            Ok(russh::server::Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        _usuario: &str,
        _clave: &russh::keys::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // Pruebas: la clave pública vale, no se verifica la firma.
        Ok(russh::server::Auth::Accept)
    }

    /// Como un sshd con AllowTcpForwarding: abre el destino y bombea bytes.
    async fn channel_open_direct_tcpip(
        &mut self,
        canal: russh::Channel<russh::server::Msg>,
        host: &str,
        puerto: u32,
        _origen: &str,
        _puerto_origen: u32,
        respuesta: russh::server::ChannelOpenHandle,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.mangos.lock().expect("mangos").push(sesion.handle());
        let conexion = tokio::time::timeout(
            Duration::from_secs(5),
            TcpStream::connect((host, puerto as u16)),
        )
        .await;
        let Ok(Ok(mut tcp)) = conexion else {
            respuesta
                .reject(russh::ChannelOpenFailure::ConnectFailed)
                .await;
            return Ok(());
        };
        respuesta.accept().await;
        tokio::spawn(async move {
            let mut flujo = canal.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut flujo).await;
        });
        Ok(())
    }
}

/// Servidor SSH de pruebas; devuelve puerto, clave pública, mangos y tarea.
async fn servir_ssh() -> (
    u16,
    ssh_key::PublicKey,
    Arc<Mutex<Vec<russh::server::Handle>>>,
    tokio::task::JoinHandle<()>,
) {
    let clave = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .expect("clave de host");
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave.clone()],
        ..Default::default()
    });
    let escucha = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("escucha ssh");
    let puerto = escucha.local_addr().expect("ssh").port();
    let mangos = Arc::new(Mutex::new(Vec::new()));
    let mangos_servidor = mangos.clone();
    let tarea = tokio::spawn(async move {
        let mut servidor = ServidorPruebas {
            mangos: mangos_servidor,
        };
        let _ = russh::server::Server::run_on_socket(&mut servidor, config, &escucha).await;
    });
    (puerto, clave.public_key().clone(), mangos, tarea)
}

// ------------------------------------------------------------ escenario

struct Montaje {
    entorno: Entorno,
    host_uuid: String,
    tunel_uuid: String,
    puerto_tunel: u16,
    mangos_ssh: Arc<Mutex<Vec<russh::server::Handle>>>,
    _tarea_ssh: tokio::task::JoinHandle<()>,
}

async fn montar() -> Montaje {
    let puerto_eco = servir_eco().await;
    let (puerto_ssh, clave_host, mangos_ssh, tarea_ssh) = servir_ssh().await;
    let entorno = Entorno::nuevo(&[(puerto_ssh, clave_host)]);
    let (host_id, uuid_host) = entorno.crear_host(DatosHost {
        nombre: "prueba".to_string(),
        direccion: "127.0.0.1".to_string(),
        puerto: puerto_ssh,
        usuario: Some("usuario".to_string()),
        ..DatosHost::default()
    });
    entorno.guardar_contrasena(&uuid_host, "secreta");

    // Puerto libre para la escucha del túnel (se libera justo antes de usarlo).
    let escucha = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("puerto libre");
    let puerto_tunel = escucha.local_addr().expect("puerto").port();
    drop(escucha);

    let tunel_id = con_almacen(entorno.estado(), move |almacen| {
        almacen_tuneles::crear(
            almacen.conexion(),
            &DatosTunel {
                host_id,
                nombre: "web".to_string(),
                tipo: TipoTunel::Local,
                escucha: format!("127.0.0.1:{puerto_tunel}"),
                destino: Some(format!("127.0.0.1:{puerto_eco}")),
                automatico: false,
            },
        )
    })
    .expect("escritor")
    .expect("túnel");
    let tunel_uuid = con_almacen(entorno.estado(), move |almacen| {
        movil_bd::uuid_por_id(almacen.conexion(), Tabla::Tuneles)
            .map(|mapa| mapa.get(&tunel_id).cloned())
    })
    .expect("escritor")
    .expect("mapa")
    .expect("uuid del túnel");

    Montaje {
        entorno,
        host_uuid: uuid_host,
        tunel_uuid,
        puerto_tunel,
        mangos_ssh,
        _tarea_ssh: tarea_ssh,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_tunel_local_del_movil_reenvia_de_verdad() {
    let montaje = montar().await;
    let estado = montaje.entorno.estado();
    tokio::time::timeout(
        Duration::from_secs(15),
        magi_movil::gestor::conexion_para_host(estado, &montaje.host_uuid),
    )
    .await
    .expect("la conexión del host se colgó")
    .expect("conectando al host");
    tokio::time::timeout(
        Duration::from_secs(20),
        magi_movil::gestor::activar_tunel(estado, &montaje.tunel_uuid, false),
    )
    .await
    .expect("la activación del túnel se colgó")
    .expect("activando el túnel");

    let tunel =
        tunel_por_uuid(montaje.entorno.oyente(), &montaje.tunel_uuid).expect("fila del túnel");
    assert_eq!(tunel.estado, EstadoTunel::Activo, "{tunel:?}");
    assert_eq!(
        tunel.escucha,
        format!("127.0.0.1:{}", montaje.puerto_tunel),
        "{tunel:?}"
    );

    let mut flujo = TcpStream::connect(("127.0.0.1", montaje.puerto_tunel))
        .await
        .expect("conectando al agujero del túnel");
    flujo
        .write_all(
            b"hola por el tunel
",
        )
        .await
        .expect("escribiendo");
    let mut bufer = vec![0u8; 256];
    let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
        .await
        .expect("el eco no llegó por el túnel")
        .expect("leyendo el eco");
    assert_eq!(
        &bufer[..leidos],
        b"hola por el tunel
"
    );

    // Los contadores tienen que reflejarlo.
    let mut contadores = (0u32, 0u32, 0u64, 0u64);
    for _ in 0..100 {
        let lista = magi_movil::gestor::tuneles(estado, None).expect("túneles");
        if let Some(tunel) = lista.into_iter().find(|t| t.uuid == montaje.tunel_uuid) {
            contadores = (
                tunel.conexiones,
                tunel.aceptadas,
                tunel.bytes_subida,
                tunel.bytes_bajada,
            );
            if contadores.1 >= 1 && contadores.2 > 0 && contadores.3 > 0 {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert!(
        contadores.1 >= 1 && contadores.2 > 0 && contadores.3 > 0,
        "contadores sin tráfico: {contadores:?}"
    );
    // Con el interruptor normal no se marca «puerto libre».
    let fila = magi_movil::gestor::tuneles(estado, None)
        .expect("túneles")
        .into_iter()
        .find(|t| t.uuid == montaje.tunel_uuid)
        .expect("fila");
    assert_eq!(fila.puerto_efectivo, None, "{fila:?}");

    // «Abrir en un puerto libre» sí lo marca y la escucha es la de verdad.
    magi_movil::gestor::parar_tunel(estado, &montaje.tunel_uuid)
        .await
        .expect("parando el túnel");
    magi_movil::gestor::activar_tunel(estado, &montaje.tunel_uuid, true)
        .await
        .expect("activando en puerto libre");
    let libre = magi_movil::gestor::tuneles(estado, None)
        .expect("túneles")
        .into_iter()
        .find(|t| t.uuid == montaje.tunel_uuid)
        .expect("fila libre");
    let puerto = libre.puerto_efectivo.expect("puerto libre marcado");
    assert_eq!(libre.escucha, format!("127.0.0.1:{puerto}"), "{libre:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn si_cae_la_conexion_del_host_el_tunel_queda_caido() {
    let montaje = montar().await;
    let estado = montaje.entorno.estado();
    magi_movil::gestor::activar_tunel(estado, &montaje.tunel_uuid, false)
        .await
        .expect("activando el túnel");

    // Alguien conecta al agujero: eso abre el canal y guarda el mango.
    let mut flujo = TcpStream::connect(("127.0.0.1", montaje.puerto_tunel))
        .await
        .expect("conectando");
    flujo.write_all(b"x").await.expect("escribiendo");
    let mut bufer = [0u8; 1];
    flujo.read_exact(&mut bufer).await.expect("eco");
    drop(flujo);

    // Se tira la conexión SSH del host, como un cambio de red.
    let mangos: Vec<russh::server::Handle> = {
        let mut guardia = montaje.mangos_ssh.lock().expect("mangos");
        std::mem::take(&mut *guardia)
    };
    for mango in mangos {
        let _ = mango
            .disconnect(
                russh::Disconnect::ByApplication,
                "".to_string(),
                "".to_string(),
            )
            .await;
    }

    let mut caido = None;
    for _ in 0..200 {
        let lista = magi_movil::gestor::tuneles(estado, None).expect("túneles");
        if let Some(tunel) = lista.into_iter().find(|t| t.uuid == montaje.tunel_uuid) {
            if tunel.estado == EstadoTunel::Caido {
                caido = Some(tunel);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let caido = caido.expect("el túnel no pasó a caído tras perder la conexión");
    let motivo = caido.error.unwrap_or_default();
    assert!(motivo.contains("conexión"), "motivo poco claro: {motivo:?}");
}
