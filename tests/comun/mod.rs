//! Apoyo común de las pruebas de integración: un entorno MAGI aislado sobre un
//! directorio temporal y un servidor SSH en proceso (russh) que autentica por
//! contraseña o clave y, si se le pide, sirve el subsistema `sftp` lanzando el
//! `sftp-server` real de OpenSSH.
//!
//! El subsistema se sirve con el binario del sistema a propósito: probar el
//! cliente SFTP contra un servidor propio escondería los fallos que solo
//! aparecen con el servidor de verdad.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// Cómo contesta el host de pruebas a los reenvíos remotos.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoReenvio {
    /// Los concede (como un `sshd` con `AllowTcpForwarding`).
    #[default]
    Concede,
    /// Los rechaza (host sin reenvío permitido, puerto ocupado allí...).
    Rechaza,
}

/// Lo que el host de pruebas ve y puede provocar en los reenvíos.
#[derive(Default)]
pub struct Reenvios {
    /// Cada `tcpip_forward` pedido y, si se concedió, el puerto que se dio.
    pub concedidos: Mutex<Vec<(String, u32)>>,
    /// Cada `cancel_tcpip_forward` recibido.
    pub cancelados: Mutex<Vec<(String, u32)>>,
    /// Canales `direct-tcpip` que el host no pudo conectar (destino caído).
    pub destinos_caidos: Mutex<Vec<String>>,
    /// Con qué abrir un canal de vuelta: el mango de la sesión del host.
    mango: Mutex<Option<russh::server::Handle>>,
}

impl Reenvios {
    /// Abre un canal `forwarded-tcpip` como si alguien hubiera conectado al
    /// puerto del host: escribe `saludo` y devuelve lo que conteste el servicio
    /// local. Vacío si no hay sesión con la que abrirlo.
    pub async fn conectar_de_vuelta(&self, direccion: &str, puerto: u32, saludo: &str) -> String {
        let mango = self.mango.lock().expect("mango").clone();
        let Some(mango) = mango else {
            return String::new();
        };
        let Ok(canal) = mango
            .channel_open_forwarded_tcpip(direccion.to_string(), puerto, "127.0.0.1", 40_000)
            .await
        else {
            return String::new();
        };
        let mut flujo = canal.into_stream();
        if flujo.write_all(saludo.as_bytes()).await.is_err() {
            return String::new();
        }
        let mut bufer = vec![0u8; 256];
        match tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer)).await {
            Ok(Ok(leidos)) => String::from_utf8_lossy(&bufer[..leidos]).to_string(),
            _ => String::new(),
        }
    }
}

/// Lo que el host de pruebas ve de las conexiones, las shells y los `exec`
/// (y con qué tirar las conexiones para simular una caída de red).
#[derive(Default)]
pub struct Observado {
    /// Conexiones SSH aceptadas desde que arrancó.
    pub conexiones: AtomicUsize,
    /// Conexiones que siguen abiertas.
    pub vivas: AtomicUsize,
    /// Todo lo que se ha escrito en las shells (pestañas).
    pub shell: Mutex<Vec<u8>>,
    /// `exec` en marcha ahora y el máximo visto a la vez.
    pub exec_activos: AtomicUsize,
    pub exec_max: AtomicUsize,
    /// Comandos `exec` recibidos, en orden.
    pub comandos: Mutex<Vec<String>>,
    /// Tamaño pedido en cada `pty-req` (canal, columnas, filas).
    pub ptys: Mutex<Vec<(u32, u32, u32)>>,
    /// Cada `window-change` recibido (canal, columnas, filas), en orden: es lo
    /// que vería `vim` o `htop` como SIGWINCH en un host real.
    pub tamanos: Mutex<Vec<(u32, u32, u32)>>,
    mangos: Mutex<Vec<russh::server::Handle>>,
}

impl Observado {
    pub fn conexiones(&self) -> usize {
        self.conexiones.load(Ordering::SeqCst)
    }

    pub fn vivas(&self) -> usize {
        self.vivas.load(Ordering::SeqCst)
    }

    pub fn shell(&self) -> String {
        String::from_utf8_lossy(&self.shell.lock().expect("shell")).to_string()
    }

    /// Los `window-change` recibidos, en orden, como (columnas, filas).
    pub fn tamanos(&self) -> Vec<(u32, u32)> {
        self.tamanos
            .lock()
            .expect("tamaños")
            .iter()
            .map(|(_, cols, filas)| (*cols, *filas))
            .collect()
    }

    /// El tamaño de cada `pty-req`, en orden, como (columnas, filas).
    pub fn ptys(&self) -> Vec<(u32, u32)> {
        self.ptys
            .lock()
            .expect("ptys")
            .iter()
            .map(|(_, cols, filas)| (*cols, *filas))
            .collect()
    }

    /// Corta todas las conexiones abiertas, como una caída de red.
    pub async fn tirar_conexiones(&self) {
        let mangos: Vec<russh::server::Handle> =
            self.mangos.lock().expect("mangos").drain(..).collect();
        for mango in mangos {
            let _ = mango
                .disconnect(
                    russh::Disconnect::ByApplication,
                    String::new(),
                    String::new(),
                )
                .await;
        }
    }
}

/// Un servidor SSH de pruebas servido por russh, en proceso.
pub struct ServidorSesion {
    /// ¿Sirve el subsistema `sftp`? Los hosts sin SFTP se prueban con `false`.
    pub sftp: bool,
    /// Qué hace con los reenvíos remotos.
    pub reenvio: ModoReenvio,
    /// Lo que ve el test de los reenvíos (y con qué provocarlos).
    pub estado: Arc<Reenvios>,
    /// Lo que ve el test de conexiones, shells y `exec`.
    pub observado: Arc<Observado>,
}

impl Default for ServidorSesion {
    fn default() -> Self {
        Self {
            sftp: true,
            reenvio: ModoReenvio::Concede,
            estado: Arc::new(Reenvios::default()),
            observado: Arc::new(Observado::default()),
        }
    }
}

impl russh::server::Server for ServidorSesion {
    type Handler = HandlerSesion;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> HandlerSesion {
        self.observado.conexiones.fetch_add(1, Ordering::SeqCst);
        self.observado.vivas.fetch_add(1, Ordering::SeqCst);
        HandlerSesion {
            sftp: self.sftp,
            reenvio: self.reenvio,
            estado: self.estado.clone(),
            observado: self.observado.clone(),
            entradas: Arc::new(Mutex::new(HashMap::new())),
            procesos: Arc::new(Mutex::new(HashMap::new())),
            shells: HashSet::new(),
            usuario: String::new(),
            mango_guardado: false,
        }
    }
}

pub struct HandlerSesion {
    /// stdin de cada `sftp-server` o `exec` lanzado, por canal.
    entradas: Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>>,
    /// pid (y grupo) del proceso de cada `exec`, para matarlo si se cierra el
    /// canal antes de que termine.
    procesos: Arc<Mutex<HashMap<u32, u32>>>,
    /// Canales con shell (pestañas): lo que llega se registra.
    shells: HashSet<u32>,
    sftp: bool,
    reenvio: ModoReenvio,
    estado: Arc<Reenvios>,
    observado: Arc<Observado>,
    /// Usuario autenticado: los `exec` lo reciben en `MAGI_USUARIO`.
    usuario: String,
    mango_guardado: bool,
}

impl Drop for HandlerSesion {
    fn drop(&mut self) {
        self.observado.vivas.fetch_sub(1, Ordering::SeqCst);
        for (_, pid) in self.procesos.lock().expect("procesos").drain() {
            matar_grupo(pid);
        }
    }
}

/// Mata el grupo de procesos de un `exec` (el `sh -c` y lo que lance).
fn matar_grupo(pid: u32) {
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    );
}

impl HandlerSesion {
    fn guardar_mango(&mut self, sesion: &russh::server::Session) {
        if !self.mango_guardado {
            self.mango_guardado = true;
            self.observado
                .mangos
                .lock()
                .expect("mangos")
                .push(sesion.handle());
        }
    }
}

impl russh::server::Handler for HandlerSesion {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        usuario: &str,
        contrasena: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        if contrasena == "secreta" {
            self.usuario = usuario.to_string();
            Ok(russh::server::Auth::Accept)
        } else {
            Ok(russh::server::Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        usuario: &str,
        _clave: &ssh_key::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // Pruebas: cualquier clave vale.
        self.usuario = usuario.to_string();
        Ok(russh::server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _canal: russh::Channel<russh::server::Msg>,
        respuesta: russh::server::ChannelOpenHandle,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.guardar_mango(sesion);
        respuesta.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        canal: russh::ChannelId,
        _: &str,
        cols: u32,
        filas: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.observado
            .ptys
            .lock()
            .expect("ptys")
            .push((u32::from(canal), cols, filas));
        let _ = sesion.channel_success(canal);
        Ok(())
    }

    /// Hace de `vim` o `stty size`: registra el tamaño nuevo y lo escribe en la
    /// pestaña, para que la prueba lo vea también en la pantalla del cliente.
    async fn window_change_request(
        &mut self,
        canal: russh::ChannelId,
        cols: u32,
        filas: u32,
        _: u32,
        _: u32,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.observado
            .tamanos
            .lock()
            .expect("tamaños")
            .push((u32::from(canal), cols, filas));
        let _ = sesion.data(canal, format!("TAM {cols}x{filas}\r\n").into_bytes());
        Ok(())
    }

    async fn shell_request(
        &mut self,
        canal: russh::ChannelId,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.shells.insert(u32::from(canal));
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

    /// `exec`: el comando corre en local con `sh -c` (con `MAGI_USUARIO` para
    /// que cada host de prueba pueda portarse distinto), stdout por `data`,
    /// stderr por `extended_data(1)`, y al terminar `exit-status`, EOF y
    /// cierre, como un `sshd` de verdad (el código llega tras el EOF de los
    /// datos). Si el canal se cierra antes, el proceso se mata.
    async fn exec_request(
        &mut self,
        canal: russh::ChannelId,
        datos: &[u8],
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let comando = String::from_utf8_lossy(datos).to_string();
        self.observado
            .comandos
            .lock()
            .expect("comandos")
            .push(comando.clone());
        let mut hijo = match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&comando)
            .env("MAGI_USUARIO", &self.usuario)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
        {
            Ok(hijo) => hijo,
            Err(_) => {
                let _ = sesion.channel_failure(canal);
                return Ok(());
            }
        };
        let _ = sesion.channel_success(canal);
        let id = u32::from(canal);
        if let Some(pid) = hijo.id() {
            self.procesos.lock().expect("procesos").insert(id, pid);
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.entradas.lock().expect("entradas").insert(id, tx);
        let mut entrada = hijo.stdin.take().expect("stdin del exec");
        tokio::spawn(async move {
            while let Some(datos) = rx.recv().await {
                if entrada.write_all(&datos).await.is_err() {
                    break;
                }
            }
            let _ = entrada.shutdown().await;
        });

        let mut salida = hijo.stdout.take().expect("stdout del exec");
        let mut errores = hijo.stderr.take().expect("stderr del exec");
        let mango = sesion.handle();
        let observado = self.observado.clone();
        let procesos = self.procesos.clone();
        let activos = observado.exec_activos.fetch_add(1, Ordering::SeqCst) + 1;
        observado.exec_max.fetch_max(activos, Ordering::SeqCst);
        tokio::spawn(async move {
            let mango_salida = mango.clone();
            let copia_salida = async move {
                let mut bufer = vec![0u8; 32 * 1024];
                loop {
                    match salida.read(&mut bufer).await {
                        Ok(0) | Err(_) => break,
                        Ok(leidos) => {
                            if mango_salida
                                .data(canal, bufer[..leidos].to_vec())
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            };
            let mango_errores = mango.clone();
            let copia_errores = async move {
                let mut bufer = vec![0u8; 32 * 1024];
                loop {
                    match errores.read(&mut bufer).await {
                        Ok(0) | Err(_) => break,
                        Ok(leidos) => {
                            if mango_errores
                                .extended_data(canal, 1, bufer[..leidos].to_vec())
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            };
            tokio::join!(copia_salida, copia_errores);
            let terminado = hijo.wait().await;
            observado.exec_activos.fetch_sub(1, Ordering::SeqCst);
            procesos.lock().expect("procesos").remove(&id);
            let codigo = terminado
                .ok()
                .and_then(|estado| estado.code())
                .unwrap_or(255) as u32;
            let _ = mango.eof(canal).await;
            let _ = mango.exit_status_request(canal, codigo).await;
            let _ = mango.close(canal).await;
        });
        Ok(())
    }

    async fn data(
        &mut self,
        canal: russh::ChannelId,
        datos: &[u8],
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        if self.shells.contains(&u32::from(canal)) {
            self.observado
                .shell
                .lock()
                .expect("shell")
                .extend_from_slice(datos);
            return Ok(());
        }
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

    async fn channel_eof(
        &mut self,
        canal: russh::ChannelId,
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        // EOF del cliente: se cierra el stdin del proceso (sftp-server o exec).
        self.entradas
            .lock()
            .expect("entradas")
            .remove(&u32::from(canal));
        Ok(())
    }

    async fn channel_close(
        &mut self,
        canal: russh::ChannelId,
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let id = u32::from(canal);
        self.entradas.lock().expect("entradas").remove(&id);
        self.shells.remove(&id);
        // Un `exec` cortado (tiempo agotado, cancelación) no sigue corriendo.
        if let Some(pid) = self.procesos.lock().expect("procesos").remove(&id) {
            matar_grupo(pid);
        }
        Ok(())
    }

    /// Reenvío remoto: el host concede el puerto (eligiéndolo si se pide 0) o
    /// lo rechaza, según cómo se haya montado la prueba.
    async fn tcpip_forward(
        &mut self,
        direccion: &str,
        puerto: &mut u32,
        sesion: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        if self.reenvio == ModoReenvio::Rechaza {
            return Ok(false);
        }
        if *puerto == 0 {
            // Un puerto libre de verdad, como haría el host al elegirlo.
            *puerto = puerto_libre().await;
        }
        self.estado
            .concedidos
            .lock()
            .expect("concedidos")
            .push((direccion.to_string(), *puerto));
        // Se guarda el mango para que la prueba pueda abrir el canal de vuelta
        // cuando le convenga (en la vida real lo abre alguien al conectar).
        *self.estado.mango.lock().expect("mango") = Some(sesion.handle());
        Ok(true)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        direccion: &str,
        puerto: u32,
        _sesion: &mut russh::server::Session,
    ) -> Result<bool, Self::Error> {
        self.estado
            .cancelados
            .lock()
            .expect("cancelados")
            .push((direccion.to_string(), puerto));
        Ok(true)
    }

    /// Túnel local y dinámico: el host abre el destino, como un `sshd` de verdad
    /// (si no puede conectar, rechaza el canal).
    async fn channel_open_direct_tcpip(
        &mut self,
        canal: russh::Channel<russh::server::Msg>,
        host: &str,
        puerto: u32,
        _origen: &str,
        _puerto_origen: u32,
        respuesta: russh::server::ChannelOpenHandle,
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let destino = format!("{host}:{puerto}");
        let conexion = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::TcpStream::connect((host, puerto as u16)),
        )
        .await;
        let Ok(Ok(mut tcp)) = conexion else {
            self.estado
                .destinos_caidos
                .lock()
                .expect("destinos")
                .push(destino);
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

/// Un puerto libre de la máquina, para conceder un reenvío pedido con 0.
async fn puerto_libre() -> u32 {
    tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .and_then(|escucha| escucha.local_addr())
        .map(|direccion| u32::from(direccion.port()))
        .unwrap_or(0)
}

/// Un servicio local de eco: devuelve lo que reciba. Es el «servicio» al que
/// apuntan los túneles de las pruebas.
pub async fn servicio_eco() -> (u16, tokio::task::JoinHandle<()>) {
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("escuchando el eco");
    let puerto = escucha.local_addr().expect("dirección del eco").port();
    let tarea = tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut bufer = vec![0u8; 1024];
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
    (puerto, tarea)
}

/// Habla SOCKS5 con un túnel dinámico y devuelve lo que conteste el destino.
/// Devuelve `None` si el host rechazó el canal.
pub async fn socks5_conectar(
    puerto_socks: u16,
    destino: &str,
    puerto_destino: u16,
    saludo: &str,
) -> Option<String> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let mut flujo = tokio::net::TcpStream::connect(("127.0.0.1", puerto_socks))
        .await
        .expect("conectando al proxy SOCKS");
    // Saludo: versión 5, un método, «sin autenticación».
    flujo.write_all(&[0x05, 0x01, 0x00]).await.expect("saludo");
    let mut respuesta = [0u8; 2];
    flujo.read_exact(&mut respuesta).await.expect("respuesta");
    assert_eq!(respuesta, [0x05, 0x00], "el proxy no acepta sin auth");

    // Petición CONNECT con el dominio tal cual, sin resolver en local.
    let mut peticion = vec![0x05, 0x01, 0x00, 0x03, destino.len() as u8];
    peticion.extend_from_slice(destino.as_bytes());
    peticion.extend_from_slice(&puerto_destino.to_be_bytes());
    flujo.write_all(&peticion).await.expect("petición");
    let mut cabecera = [0u8; 10];
    flujo.read_exact(&mut cabecera).await.expect("respuesta");
    if cabecera[0] != 0x05 {
        return None;
    }
    if cabecera[1] != 0x00 {
        return None;
    }
    flujo.write_all(saludo.as_bytes()).await.expect("saludo");
    let mut bufer = vec![0u8; 256];
    let leidos = flujo.read(&mut bufer).await.expect("eco");
    Some(String::from_utf8_lossy(&bufer[..leidos]).to_string())
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

// -------------------------------------------------------------- escenario

/// Opciones del escenario completo de `escenario`.
pub struct OpcionesEscenario {
    /// Cuántos hosts crear (`prueba-1`, `prueba-2`…), todos contra el mismo
    /// servidor SSH de pruebas.
    pub hosts: usize,
    pub multiplexar: bool,
    pub sftp: bool,
    pub reenvio: ModoReenvio,
    /// Identidad de los hosts: `None` = la clave de fichero de la prueba.
    pub identidad: Option<magi::modelo::IdentidadRef>,
    /// ¿Se escribe la huella del host en known_hosts?
    pub huella_conocida: bool,
}

impl Default for OpcionesEscenario {
    fn default() -> Self {
        Self {
            hosts: 1,
            multiplexar: false,
            sftp: true,
            reenvio: ModoReenvio::Concede,
            identidad: None,
            huella_conocida: true,
        }
    }
}

type LectorProtocolo =
    tokio_util::codec::FramedRead<tokio::net::unix::OwnedReadHalf, magi::protocolo::LinesCodec>;

/// Escenario completo: servidor SSH de pruebas, hosts en la base de datos,
/// servidor de sesiones en proceso y un cliente del protocolo ya saludado.
pub struct Escenario {
    pub entorno: Entorno,
    /// Id que el servidor dio al cliente del escenario.
    pub cliente_id: u32,
    pub hosts: Vec<i64>,
    pub observado: Arc<Observado>,
    pub reenvios: Arc<Reenvios>,
    pub escritura: tokio::net::unix::OwnedWriteHalf,
    pub lector: LectorProtocolo,
    /// Últimas difusiones vistas.
    pub sesiones: Vec<magi::protocolo::InfoSesion>,
    pub tuneles: Vec<magi::protocolo::InfoTunel>,
    /// Todos los mensajes recibidos, en orden (para comprobar lo que no llegó).
    pub vistos: Vec<magi::protocolo::MensajeServidor>,
    tarea_ssh: tokio::task::JoinHandle<()>,
    pub tarea_magi: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Drop for Escenario {
    fn drop(&mut self) {
        self.tarea_ssh.abort();
        self.tarea_magi.abort();
    }
}

/// Una ventana extra conectada al servidor de sesiones del escenario.
pub struct Ventana {
    pub cliente_id: u32,
    pub escritura: tokio::net::unix::OwnedWriteHalf,
    pub lector: LectorProtocolo,
}

impl Ventana {
    pub async fn enviar(&mut self, mensaje: &MensajeCliente) {
        enviar_a(&mut self.escritura, mensaje).await;
    }

    /// Lee hasta un mensaje que cumpla el predicado, con plazo de 10 s.
    pub async fn esperar<F>(&mut self, mut vale: F) -> magi::protocolo::MensajeServidor
    where
        F: FnMut(&magi::protocolo::MensajeServidor) -> bool,
    {
        let fin = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let restante = fin.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(restante, siguiente_framed(&mut self.lector)).await {
                Ok(mensaje) if vale(&mensaje) => return mensaje,
                Ok(_) => {}
                Err(_) => panic!(
                    "la ventana {} no recibió lo esperado en 10 s",
                    self.cliente_id
                ),
            }
        }
    }
}

/// Monta el escenario.
pub async fn escenario(opciones: OpcionesEscenario) -> Escenario {
    use magi::almacen::{hosts, Almacen};
    use magi::modelo::{DatosHost, IdentidadRef, Origen};
    use magi::protocolo::{MensajeServidor, VERSION_PROTOCOLO};
    use std::os::unix::fs::PermissionsExt as _;

    let entorno = entorno();
    let rutas = entorno.rutas.clone();

    // 1. Servidor SSH de pruebas, con clave conocida de antemano.
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
    let reenvios = Arc::new(Reenvios::default());
    let observado = Arc::new(Observado::default());
    let mut servidor_ssh = ServidorSesion {
        sftp: opciones.sftp,
        reenvio: opciones.reenvio,
        estado: reenvios.clone(),
        observado: observado.clone(),
    };
    let tarea_ssh = tokio::spawn(async move {
        let _ = russh::server::Server::run_on_socket(&mut servidor_ssh, config, &escucha).await;
    });

    // 2. Identidad del cliente y huella del host.
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
    std::fs::set_permissions(&ruta_clave, std::fs::Permissions::from_mode(0o600)).unwrap();
    if opciones.huella_conocida {
        std::fs::write(
            rutas.fichero_known_hosts(),
            format!(
                "[127.0.0.1]:{puerto_ssh} {}\n",
                clave.public_key().to_openssh().unwrap()
            ),
        )
        .unwrap();
    }

    // 3. Hosts en la base de datos.
    let identidad = opciones
        .identidad
        .clone()
        .unwrap_or_else(|| IdentidadRef::Fichero(ruta_clave.display().to_string()));
    let ids = {
        let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
        let mut ids = Vec::new();
        for numero in 1..=opciones.hosts {
            let id = hosts::crear(
                almacen.conexion(),
                &DatosHost {
                    nombre: format!("prueba-{numero}"),
                    direccion: "127.0.0.1".to_string(),
                    puerto: puerto_ssh,
                    usuario: Some(format!("usuario{numero}")),
                    identidad_ref: identidad.clone(),
                    multiplexar: opciones.multiplexar,
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .unwrap();
            ids.push(id);
        }
        almacen.cerrar().unwrap();
        ids
    };

    // 4. Servidor de sesiones y cliente del protocolo.
    let tarea_magi = tokio::spawn(magi::servidor::arrancar(
        rutas.clone(),
        entorno.config.clone(),
    ));
    esperar_socket(&magi::servidor::ruta_socket(&rutas)).await;
    let mut stream = UnixStream::connect(magi::servidor::ruta_socket(&rutas))
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
    let (lectura, escritura) = stream.into_split();
    let mut lector = tokio_util::codec::FramedRead::new(lectura, magi::protocolo::codec());
    let bienvenida: MensajeServidor = siguiente_framed(&mut lector).await;
    let MensajeServidor::Bienvenida { cliente_id, .. } = bienvenida else {
        panic!("se esperaba Bienvenida y llegó {bienvenida:?}");
    };

    Escenario {
        entorno,
        cliente_id,
        hosts: ids,
        observado,
        reenvios,
        escritura,
        lector,
        sesiones: Vec::new(),
        tuneles: Vec::new(),
        vistos: Vec::new(),
        tarea_ssh,
        tarea_magi,
    }
}

impl Escenario {
    pub fn rutas(&self) -> &Rutas {
        &self.entorno.rutas
    }

    /// Otra ventana de MAGI conectada al mismo servidor, ya saludada.
    pub async fn otra_ventana(&self) -> Ventana {
        use magi::protocolo::{MensajeServidor, VERSION_PROTOCOLO};
        let mut stream = UnixStream::connect(magi::servidor::ruta_socket(self.rutas()))
            .await
            .expect("conectando la segunda ventana");
        enviar(
            &mut stream,
            &MensajeCliente::Hola {
                version: VERSION_PROTOCOLO,
                pid: 43,
            },
        )
        .await;
        let (lectura, escritura) = stream.into_split();
        let mut lector = tokio_util::codec::FramedRead::new(lectura, magi::protocolo::codec());
        let bienvenida: MensajeServidor = siguiente_framed(&mut lector).await;
        let MensajeServidor::Bienvenida { cliente_id, .. } = bienvenida else {
            panic!("se esperaba Bienvenida y llegó {bienvenida:?}");
        };
        Ventana {
            cliente_id,
            escritura,
            lector,
        }
    }

    pub async fn enviar(&mut self, mensaje: &MensajeCliente) {
        enviar_a(&mut self.escritura, mensaje).await;
    }

    /// Siguiente mensaje del servidor, con plazo: si no llega, la prueba falla
    /// en vez de quedarse colgada. Las difusiones se guardan al pasar.
    pub async fn siguiente(&mut self) -> magi::protocolo::MensajeServidor {
        use magi::protocolo::MensajeServidor;
        match tokio::time::timeout(Duration::from_secs(45), siguiente_framed(&mut self.lector))
            .await
        {
            Ok(mensaje) => {
                match &mensaje {
                    MensajeServidor::Tuneles { lista } => self.tuneles = lista.clone(),
                    MensajeServidor::Sesiones { lista } => self.sesiones = lista.clone(),
                    _ => {}
                }
                self.vistos.push(mensaje.clone());
                mensaje
            }
            Err(_) => panic!(
                "el servidor no mandó nada en 45 s; sesiones {:?}",
                self.sesiones
            ),
        }
    }

    /// Lee hasta un mensaje que cumpla el predicado.
    pub async fn esperar<F>(&mut self, mut vale: F) -> magi::protocolo::MensajeServidor
    where
        F: FnMut(&magi::protocolo::MensajeServidor) -> bool,
    {
        loop {
            let mensaje = self.siguiente().await;
            if vale(&mensaje) {
                return mensaje;
            }
        }
    }

    /// Lee lo que llegue durante un rato (para comprobar lo que no llega).
    pub async fn escuchar(&mut self, plazo: Duration) -> Vec<magi::protocolo::MensajeServidor> {
        use magi::protocolo::MensajeServidor;
        let fin = tokio::time::Instant::now() + plazo;
        let mut llegados = Vec::new();
        loop {
            let restante = fin.saturating_duration_since(tokio::time::Instant::now());
            if restante.is_zero() {
                return llegados;
            }
            match tokio::time::timeout(restante, siguiente_framed(&mut self.lector)).await {
                Ok(mensaje) => {
                    match &mensaje {
                        MensajeServidor::Tuneles { lista } => self.tuneles = lista.clone(),
                        MensajeServidor::Sesiones { lista } => self.sesiones = lista.clone(),
                        _ => {}
                    }
                    self.vistos.push(mensaje.clone());
                    llegados.push(mensaje);
                }
                Err(_) => return llegados,
            }
        }
    }

    /// Espera la respuesta de una petición: `None` si fue `Hecho`, el mensaje
    /// si fue `Error`.
    pub async fn esperar_respuesta(&mut self, peticion_id: u64) -> Option<String> {
        use magi::protocolo::MensajeServidor;
        loop {
            match self.siguiente().await {
                MensajeServidor::Hecho { peticion_id: id } if id == peticion_id => return None,
                MensajeServidor::Error {
                    mensaje,
                    peticion_id: Some(id),
                } if id == peticion_id => return Some(mensaje),
                _ => {}
            }
        }
    }

    /// Pide una pestaña al host y espera a que abra; devuelve su id. Contesta
    /// «sí» a la huella y la contraseña «secreta» si se las piden.
    pub async fn abrir_sesion(&mut self, host_id: i64) -> u32 {
        let conocidas: HashSet<u32> = self.sesiones.iter().map(|sesion| sesion.id).collect();
        self.enviar(&MensajeCliente::AbrirSesion {
            comandos_iniciales: Vec::new(),
            host_id,
            cols: 80,
            filas: 24,
        })
        .await;
        self.esperar_sesion_nueva(&conocidas).await
    }

    /// Espera a que aparezca abierta una sesión que no esté en `conocidas`.
    pub async fn esperar_sesion_nueva(&mut self, conocidas: &HashSet<u32>) -> u32 {
        use magi::protocolo::{EstadoSesionRemota, MensajeServidor, Secreto};
        loop {
            if let Some(sesion) = self.sesiones.iter().find(|sesion| {
                sesion.estado == EstadoSesionRemota::Abierta && !conocidas.contains(&sesion.id)
            }) {
                return sesion.id;
            }
            match self.siguiente().await {
                MensajeServidor::HuellaDesconocida { sesion_id, .. } => {
                    self.enviar(&MensajeCliente::DecisionHuella {
                        sesion_id,
                        decision: true,
                    })
                    .await;
                }
                MensajeServidor::PideContrasena { sesion_id, .. } => {
                    self.enviar(&MensajeCliente::Contrasena {
                        sesion_id,
                        contrasena: Secreto::nuevo("secreta"),
                        recordar: false,
                    })
                    .await;
                }
                MensajeServidor::Estado {
                    estado: EstadoSesionRemota::Cerrada,
                    motivo,
                    ..
                } => panic!("la apertura falló: {motivo:?}"),
                _ => {}
            }
        }
    }

    /// Filas del registro, de la más reciente a la más antigua.
    pub fn registro(&self) -> Vec<(String, String)> {
        let almacen = magi::almacen::Almacen::abrir(&self.rutas().base_datos()).unwrap();
        let filas = {
            let mut sentencia = almacen
                .conexion()
                .prepare("SELECT tipo, detalle FROM REGISTRO ORDER BY id DESC LIMIT 200")
                .unwrap();
            sentencia
                .query_map([], |fila| Ok((fila.get(0)?, fila.get(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<(String, String)>>>()
                .unwrap()
        };
        almacen.cerrar().unwrap();
        filas
    }

    /// Cuántas filas de un tipo hay en el registro.
    pub fn anotaciones(&self, tipo: &str) -> usize {
        self.registro()
            .iter()
            .filter(|(fila, _)| fila == tipo)
            .count()
    }

    /// Espera (con plazo) a que la condición se cumpla, leyendo mientras tanto.
    pub async fn hasta<F>(&mut self, plazo: Duration, mut condicion: F) -> bool
    where
        F: FnMut(&Escenario) -> bool,
    {
        let fin = tokio::time::Instant::now() + plazo;
        loop {
            if condicion(self) {
                return true;
            }
            if tokio::time::Instant::now() >= fin {
                return false;
            }
            self.escuchar(Duration::from_millis(100)).await;
        }
    }
}
