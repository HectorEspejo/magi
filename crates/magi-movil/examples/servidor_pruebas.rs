//! Servidor SSH y servicio de eco de pruebas para el E2E del túnel del móvil
//! en el emulador Android:
//!
//!   cargo run -p magi-movil --example servidor_pruebas_tunel
//!
//! - SSH en 0.0.0.0:12222 (el emulador lo ve en 10.0.2.2:12222). Acepta
//!   cualquier clave pública y no verifica la firma: solo prueba el reenvío.
//! - Eco en 127.0.0.1:19091, al que el servidor SSH conecta al recibir un canal
//!   direct-tcpip. La fixture del túnel apunta a 127.0.0.1:19091.
//!
//! Los puertos son fijos porque la fixture de sincronización del E2E también
//! los lleva escritos.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

const PUERTO_SSH: u16 = 12222;

/// Clave privada Ed25519 fija del servidor de pruebas (solo pruebas).
const CLAVE_HOST: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\nQyNTUxOQAAACDA1IOIcCWN1rhZ0F5j85aNnNvHOyYGids5Mcn/DhhybAAAAJA+mLoOPpi6\nDgAAAAtzc2gtZWQyNTUxOQAAACDA1IOIcCWN1rhZ0F5j85aNnNvHOyYGids5Mcn/DhhybA\nAAAECR3deuBiE8h56e5dn365ybojzbijkQkd3yic83qo/6McDUg4hwJY3WuFnQXmPzlo2c\n28c7JgaJ2zkxyf8OGHJsAAAADG1hZ2ktcHJ1ZWJhcwE=\n-----END OPENSSH PRIVATE KEY-----\n";
const PUERTO_ECO: u16 = 19091;
/// Canal de control para tirar las conexiones SSH desde la prueba.
const PUERTO_CONTROL: u16 = 12223;

struct Servidor {
    mangos: Arc<Mutex<Vec<russh::server::Handle>>>,
}

impl russh::server::Server for Servidor {
    type Handler = Manejador;

    fn new_client(&mut self, origen: Option<std::net::SocketAddr>) -> Manejador {
        eprintln!("ssh: conexión entrante de {origen:?}");
        Manejador {
            mangos: self.mangos.clone(),
        }
    }
}

struct Manejador {
    mangos: Arc<Mutex<Vec<russh::server::Handle>>>,
}

impl russh::server::Handler for Manejador {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        _usuario: &str,
        _clave: &russh::keys::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // Pruebas: cualquier clave vale y la firma no se verifica.
        Ok(russh::server::Auth::Accept)
    }

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
        eprintln!("direct-tcpip pedido: {host}:{puerto}");
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

/// Canal de control: al recibir «tirar» corta todas las conexiones SSH.
async fn servir_control(mangos: Arc<Mutex<Vec<russh::server::Handle>>>) {
    let escucha = TcpListener::bind(("0.0.0.0", PUERTO_CONTROL))
        .await
        .expect("control en 0.0.0.0:12223");
    tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            let mangos = mangos.clone();
            tokio::spawn(async move {
                let mut orden = String::new();
                let _ = flujo.read_to_string(&mut orden).await;
                if orden.trim() == "tirar" {
                    let cortados: Vec<russh::server::Handle> = {
                        let mut guardia = mangos.lock().expect("mangos");
                        std::mem::take(&mut *guardia)
                    };
                    for mango in cortados {
                        let _ = mango
                            .disconnect(
                                russh::Disconnect::ByApplication,
                                "prueba: se cae la red".to_string(),
                                "".to_string(),
                            )
                            .await;
                    }
                    eprintln!("control: conexiones SSH tiradas");
                }
            });
        }
    });
}

/// Eco: devuelve tal cual lo recibido.
async fn servir_eco() {
    let escucha = TcpListener::bind(("127.0.0.1", PUERTO_ECO))
        .await
        .expect("eco en 127.0.0.1:19091");
    tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut bufer = vec![0u8; 4096];
                loop {
                    match flujo.read(&mut bufer).await {
                        Ok(0) | Err(_) => return,
                        Ok(leidos) => {
                            if flujo.write_all(&bufer[..leidos]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    eprintln!("eco escuchando en 127.0.0.1:{PUERTO_ECO}");
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    servir_eco().await;

    // Clave de host fija: la prueba instrumentada lleva su línea de known_hosts.
    let clave = ssh_key::PrivateKey::from_openssh(CLAVE_HOST).expect("clave de host de pruebas");
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave],
        ..Default::default()
    });
    let escucha = TcpListener::bind(("0.0.0.0", PUERTO_SSH))
        .await
        .expect("ssh en 0.0.0.0:12222");
    eprintln!("ssh escuchando en 0.0.0.0:{PUERTO_SSH}");
    eprintln!("listo: SSH {PUERTO_SSH} · ECO {PUERTO_ECO} · CONTROL {PUERTO_CONTROL}");
    let mangos = Arc::new(Mutex::new(Vec::new()));
    servir_control(mangos.clone()).await;
    let mut servidor = Servidor { mangos };
    let _ = russh::server::Server::run_on_socket(&mut servidor, config, &escucha).await;
}
