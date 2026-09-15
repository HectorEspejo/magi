//! Apoyo común de las pruebas de integración: un entorno MAGI aislado sobre un
//! directorio temporal y un servidor SSH en proceso (russh) que autentica por
//! contraseña o clave y, si se le pide, sirve el subsistema `sftp` lanzando el
//! `sftp-server` real de OpenSSH.
//!
//! El subsistema se sirve con el binario del sistema a propósito: probar el
//! cliente SFTP contra un servidor propio escondería los fallos que solo
//! aparecen con el servidor de verdad.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_stream::StreamExt as _;

use magi::config::{Config, Rutas};
use magi::protocolo::{decodificar, MensajeCliente};

pub struct Entorno {
    pub _temporal: tempfile::TempDir,
    pub rutas: Rutas,
    pub config: Config,
}

pub fn entorno() -> Entorno {
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

pub async fn esperar_socket(ruta: &Path) {
    for _ in 0..100 {
        if UnixStream::connect(ruta).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("el socket del servidor no ha aparecido en 2 s");
}

/// Envía una línea de mensaje al socket.
pub async fn enviar(stream: &mut UnixStream, mensaje: &MensajeCliente) {
    let linea = magi::protocolo::codificar(mensaje).unwrap();
    stream
        .write_all(linea.as_bytes())
        .await
        .expect("escribiendo");
    stream.write_all(b"\n").await.expect("escribiendo el salto");
    stream.flush().await.expect("vaciando");
}

/// Conecta, saluda y devuelve el lector del socket.
pub async fn cliente(ruta: &Path, version: u32) -> BufReader<UnixStream> {
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
pub async fn siguiente<M: serde::de::DeserializeOwned>(lector: &mut BufReader<UnixStream>) -> M {
    let mut linea = String::new();
    lector
        .read_line(&mut linea)
        .await
        .expect("leyendo la respuesta");
    decodificar::<M>(linea.trim_end()).expect("mensaje decodificable")
}

/// Envía un mensaje por la mitad de escritura del socket.
pub async fn enviar_a(escritura: &mut tokio::net::unix::OwnedWriteHalf, mensaje: &MensajeCliente) {
    let linea = magi::protocolo::codificar(mensaje).unwrap();
    escritura
        .write_all(linea.as_bytes())
        .await
        .expect("escribiendo");
    escritura
        .write_all(b"\n")
        .await
        .expect("escribiendo el salto");
    escritura.flush().await.expect("vaciando");
}

/// Lee el siguiente mensaje del servidor con el codec del protocolo.
pub async fn siguiente_framed<M: serde::de::DeserializeOwned, R: tokio::io::AsyncRead + Unpin>(
    lector: &mut tokio_util::codec::FramedRead<R, magi::protocolo::LinesCodec>,
) -> M {
    match lector.next().await {
        Some(Ok(linea)) => decodificar::<M>(linea.trim_end()).expect("mensaje decodificable"),
        otro => panic!("el servidor cerró la conexión: {otro:?}"),
    }
}

/// Lee mensajes del socket hasta que uno cumpla el predicado (con plazo).
pub async fn esperar<M, F>(lector: &mut BufReader<UnixStream>, mut vale: F) -> M
where
    M: serde::de::DeserializeOwned + std::fmt::Debug,
    F: FnMut(&M) -> bool,
{
    let plazo = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let mensaje: M = siguiente(lector).await;
        if vale(&mensaje) {
            return mensaje;
        }
        assert!(
            tokio::time::Instant::now() < plazo,
            "no llegó el mensaje esperado; el último fue {mensaje:?}"
        );
    }
}

// -------------------------------------------------------------- servidor ssh

/// Un servidor SSH de pruebas servido por russh, en proceso.
pub struct ServidorSesion {
    /// ¿Sirve el subsistema `sftp`? Los hosts sin SFTP se prueban con `false`.
    pub sftp: bool,
}

impl Default for ServidorSesion {
    fn default() -> Self {
        Self { sftp: true }
    }
}

impl russh::server::Server for ServidorSesion {
    type Handler = HandlerSesion;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> HandlerSesion {
        HandlerSesion {
            sftp: self.sftp,
            entradas: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub struct HandlerSesion {
    /// stdin de cada `sftp-server` lanzado, por canal.
    entradas: Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>>,
    sftp: bool,
}

impl russh::server::Handler for HandlerSesion {
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
        _clave: &ssh_key::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // Pruebas: cualquier clave vale.
        Ok(russh::server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _canal: russh::Channel<russh::server::Msg>,
        respuesta: russh::server::ChannelOpenHandle,
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        respuesta.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        canal: russh::ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let _ = sesion.channel_success(canal);
        Ok(())
    }

    async fn shell_request(
        &mut self,
        canal: russh::ChannelId,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let _ = sesion.channel_success(canal);
        // Contenido inicial para que el volcado al adjuntar no esté vacío.
        let _ = sesion.data(canal, &b"MARCA_CLIENTE\r\n"[..]);
        Ok(())
    }

    /// El subsistema `sftp` se sirve con el `sftp-server` real de OpenSSH: se
    /// lanza el proceso y se bombean los bytes entre el canal y sus tuberías,
    /// que es exactamente lo que hace `sshd`.
    async fn subsystem_request(
        &mut self,
        canal: russh::ChannelId,
        nombre: &str,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let Some(programa) = ruta_sftp_server() else {
            let _ = sesion.channel_failure(canal);
            return Ok(());
        };
        if nombre != "sftp" || !self.sftp {
            let _ = sesion.channel_failure(canal);
            return Ok(());
        }
        let mut hijo = match tokio::process::Command::new(programa)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(hijo) => hijo,
            Err(_) => {
                let _ = sesion.channel_failure(canal);
                return Ok(());
            }
        };
        let _ = sesion.channel_success(canal);

        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.entradas
            .lock()
            .expect("entradas")
            .insert(u32::from(canal), tx);

        // Canal → stdin del proceso.
        let mut entrada = hijo.stdin.take().expect("stdin del sftp-server");
        tokio::spawn(async move {
            while let Some(datos) = rx.recv().await {
                if entrada.write_all(&datos).await.is_err() {
                    break;
                }
            }
            let _ = entrada.shutdown().await;
        });

        // stdout del proceso → canal.
        let mut salida = hijo.stdout.take().expect("stdout del sftp-server");
        let manejador = sesion.handle();
        tokio::spawn(async move {
            let mut bufer = vec![0u8; 32 * 1024];
            loop {
                match salida.read(&mut bufer).await {
                    Ok(0) | Err(_) => break,
                    Ok(leidos) => {
                        if manejador
                            .data(canal, bufer[..leidos].to_vec())
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            let _ = hijo.kill().await;
        });
        Ok(())
    }

    async fn data(
        &mut self,
        canal: russh::ChannelId,
        datos: &[u8],
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        if let Some(entrada) = self
            .entradas
            .lock()
            .expect("entradas")
            .get(&u32::from(canal))
        {
            let _ = entrada.send(datos.to_vec());
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        canal: russh::ChannelId,
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.entradas
            .lock()
            .expect("entradas")
            .remove(&u32::from(canal));
        Ok(())
    }
}

/// Ruta del `sftp-server` de OpenSSH, si está instalado.
pub fn ruta_sftp_server() -> Option<PathBuf> {
    const CANDIDATOS: [&str; 4] = [
        "/usr/lib/ssh/sftp-server",
        "/usr/lib/openssh/sftp-server",
        "/usr/libexec/openssh/sftp-server",
        "/usr/libexec/sftp-server",
    ];
    for candidato in CANDIDATOS {
        let ruta = PathBuf::from(candidato);
        if ruta.exists() {
            return Some(ruta);
        }
    }
    // macOS y distribuciones que lo dejan en el PATH.
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directorio| directorio.join("sftp-server"))
        .find(|ruta| ruta.exists())
}

/// Las pruebas de SFTP se saltan si el sistema no trae el `sftp-server`.
pub fn hay_sftp_server() -> bool {
    let hay = ruta_sftp_server().is_some();
    if !hay {
        eprintln!("[AVISO] no hay sftp-server en el sistema: se salta la prueba de SFTP");
    }
    hay
}
