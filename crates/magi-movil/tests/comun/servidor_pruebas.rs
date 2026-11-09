//! Servidor de pruebas in-process del móvil de guardia (Fase 14): host SSH de
//! pruebas con exec, PTY/shell y subsistema SFTP, eco para los túneles, canal
//! de control «tirar», binarios falsos compartidos y directorios de backups y
//! de trabajo.
//!
//! Lo usan las pruebas de extremo a extremo en Rust de `magi-movil` (T95) y el
//! ejemplo `examples/servidor_pruebas.rs`, que es un envoltorio fino con los
//! puertos fijos que esperan las fixtures del emulador. Todo escucha SOLO en
//! `127.0.0.1` (R107).
//!
//! Uso desde una prueba de integración (el módulo se declara en
//! `tests/comun/mod.rs` y se incluye con `mod comun;`):
//!
//! ```ignore
//! use comun::servidor_pruebas::{arrancar, PuertosPrueba};
//!
//! let servidor = arrancar(PuertosPrueba::efimeros()).await;
//! let puerto = servidor.puertos.ssh;      // real, aunque se pidiera 0
//! servidor.escribir_known_hosts(&ruta);
//! servidor.tirar_conexiones().await;      // como un cambio de red
//! servidor.parar().await;                 // o dejar caer: Drop también para
//! ```
//!
//! Con [`OpcionesPrueba`] se cambian la autenticación, los directorios y los
//! puertos ([`arrancar_con`]). Los `exec` corren con `sh -c`, el `PATH` de los
//! binarios falsos delante y `MAGI_USUARIO`; las shells y los `exec` usan el
//! directorio de trabajo de la prueba.
//!
//! Contrato del host de pruebas (el mismo que espera el emulador):
//!
//! - systemd 252 con `app.service` (activa, `CanReload=no`), `copia.service`
//!   (fallida), `nginx.service` (activa, `CanReload=yes`) y
//!   `postgresql.service` (activa), más el timer `app.timer` (activo);
//! - Docker 27 con los contenedores `web-app-1` (en marcha, sano) y `panel`
//!   (caído con salida 1), los dos del proyecto Compose `web` en
//!   `/srv/web/docker-compose.yml`;
//! - sudo sin contraseña (la contraseña de prueba es «secreta») y `nginx -t`
//!   correcto;
//! - backups en [`DIRECTORIO_BACKUPS`]: `db-1.sql.gz` (hace 3 h),
//!   `db-2.sql.gz` (hace 31 h) y `notas.txt` (hace 1 h);
//! - el subsistema `sftp` se sirve con el `sftp-server` real de OpenSSH
//!   (`MAGI_SFTP_SERVER` manda si apunta a un fichero existente) sobre el
//!   sistema de ficheros del proceso;
//! - `direct-tcpip` solo se concede a `127.0.0.1:<puerto del eco>`.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[path = "../../../magi-nucleo/tests/fijos/servicios/binarios.rs"]
mod binarios;

pub use binarios::BinariosFalsos;

/// Directorio fijo de backups del host de pruebas (BALTHASAR-2 lo lista por
/// SFTP). Las fixtures de la deliberación llevan escrita esta ruta tal cual.
pub const DIRECTORIO_BACKUPS: &str = "/tmp/magi-pruebas/backups";

/// Directorio de trabajo fijo de los `exec` y las shells del host de pruebas.
pub const DIRECTORIO_TRABAJO: &str = "/tmp/magi-pruebas/trabajo";

/// Ficheros del directorio de backups: nombre y edad en segundos. El más
/// reciente que casa con `*.sql.gz` es de hace 3 h (BALTHASAR-2 aprueba con el
/// umbral de 24 h) y `db-2.sql.gz`, de hace 31 h, sirve para el rechazo si el
/// patrón lo aísla (`db-2.*`).
const BACKUPS: [(&str, u64); 3] = [
    ("db-1.sql.gz", 3 * 3600),
    ("db-2.sql.gz", 31 * 3600),
    ("notas.txt", 3600),
];

/// Clave privada Ed25519 fija del servidor de pruebas (solo pruebas). La
/// prueba instrumentada lleva su línea de `known_hosts` escrita.
const CLAVE_HOST: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\nQyNTUxOQAAACDA1IOIcCWN1rhZ0F5j85aNnNvHOyYGids5Mcn/DhhybAAAAJA+mLoOPpi6\nDgAAAAtzc2gtZWQyNTUxOQAAACDA1IOIcCWN1rhZ0F5j85aNnNvHOyYGids5Mcn/DhhybA\nAAAECR3deuBiE8h56e5dn365ybojzbijkQkd3yic83qo/6McDUg4hwJY3WuFnQXmPzlo2c\n28c7JgaJ2zkxyf8OGHJsAAAADG1hZ2ktcHJ1ZWJhcwE=\n-----END OPENSSH PRIVATE KEY-----\n";

/// Lo que comparten todas las conexiones: con qué tirarlas y los binarios
/// falsos (mientras viva el servidor, vive también su estado).
struct Compartido {
    mangos: Mutex<Vec<russh::server::Handle>>,
    binarios: BinariosFalsos,
    puerto_eco: u16,
    directorio_backups: PathBuf,
    directorio_trabajo: PathBuf,
    acepta_publickey: bool,
    contrasena: Option<String>,
}

struct Servidor {
    compartido: Arc<Compartido>,
}

impl russh::server::Server for Servidor {
    type Handler = Manejador;

    fn new_client(&mut self, origen: Option<std::net::SocketAddr>) -> Manejador {
        eprintln!("ssh: conexión entrante de {origen:?}");
        Manejador {
            compartido: self.compartido.clone(),
            usuario: String::new(),
            entradas: Arc::new(Mutex::new(HashMap::new())),
            procesos: Arc::new(Mutex::new(HashMap::new())),
            shells: HashSet::new(),
            guardado: false,
        }
    }
}

/// Una conexión SSH: sus canales abiertos, los procesos que ha lanzado y el
/// usuario autenticado.
struct Manejador {
    compartido: Arc<Compartido>,
    usuario: String,
    /// stdin del proceso de cada canal (exec, shell o sftp-server).
    entradas: Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>>,
    /// pid (y grupo) del proceso de cada canal, para matarlo si el canal se
    /// cierra antes de que termine.
    procesos: Arc<Mutex<HashMap<u32, u32>>>,
    /// Canales con shell: lo que llega se escribe en la shell.
    shells: HashSet<u32>,
    /// ¿Se guardó ya el mango de esta conexión para poder tirarla?
    guardado: bool,
}

impl Manejador {
    fn guardar_mango(&mut self, sesion: &russh::server::Session) {
        if !self.guardado {
            self.guardado = true;
            self.compartido
                .mangos
                .lock()
                .expect("mangos")
                .push(sesion.handle());
        }
    }

    /// Entorno de los procesos lanzados en el host: el directorio de trabajo
    /// fijo, los binarios falsos compartidos de la F13 delante en el `PATH` y
    /// el usuario autenticado.
    fn aplicar_entorno(&self, orden: &mut tokio::process::Command) {
        orden.current_dir(&self.compartido.directorio_trabajo);
        orden.env("MAGI_USUARIO", &self.usuario);
        let actual = std::env::var_os("PATH").unwrap_or_default();
        let mut path = self.compartido.binarios.ruta().as_os_str().to_os_string();
        if !actual.is_empty() {
            path.push(":");
            path.push(actual);
        }
        orden.env("PATH", path);
    }

    /// Lanza un proceso con la entrada y las salidas del canal, como hace
    /// `sshd` con un `exec`, una shell o el `sftp-server`.
    fn lanzar_proceso(
        &mut self,
        canal: russh::ChannelId,
        sesion: &mut russh::server::Session,
        mut orden: tokio::process::Command,
        con_estado_de_salida: bool,
    ) -> Result<(), russh::Error> {
        orden
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        let mut hijo = match orden.spawn() {
            Ok(hijo) => hijo,
            Err(error) => {
                eprintln!("ssh: no se pudo lanzar el proceso: {error}");
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
        let mut entrada = hijo.stdin.take().expect("stdin del proceso");
        tokio::spawn(async move {
            while let Some(datos) = rx.recv().await {
                if entrada.write_all(&datos).await.is_err() {
                    break;
                }
            }
            let _ = entrada.shutdown().await;
        });

        let mut salida = hijo.stdout.take().expect("stdout del proceso");
        let mut errores = hijo.stderr.take().expect("stderr del proceso");
        let mango = sesion.handle();
        let procesos = self.procesos.clone();
        tokio::spawn(async move {
            let mango_salida = mango.clone();
            let leer_salida = async move {
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
            let leer_errores = async move {
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
            tokio::join!(leer_salida, leer_errores);
            let terminado = hijo.wait().await;
            procesos.lock().expect("procesos").remove(&id);
            let codigo = terminado
                .ok()
                .and_then(|estado| estado.code())
                .unwrap_or(255) as u32;
            // Como un `sshd` de verdad: primero el EOF de los datos, luego el
            // código de salida (si este canal lo tiene) y el cierre.
            let _ = mango.eof(canal).await;
            if con_estado_de_salida {
                let _ = mango.exit_status_request(canal, codigo).await;
            }
            let _ = mango.close(canal).await;
        });
        Ok(())
    }

    /// Mata el grupo de procesos de un canal (el `sh -c`, la shell o el
    /// `sftp-server` y lo que hayan lanzado).
    fn matar_canal(&self, id: u32) {
        if let Some(pid) = self.procesos.lock().expect("procesos").remove(&id) {
            matar_grupo(pid);
        }
    }
}

impl Drop for Manejador {
    fn drop(&mut self) {
        let pids: Vec<u32> = self
            .procesos
            .lock()
            .expect("procesos")
            .drain()
            .map(|(_, pid)| pid)
            .collect();
        for pid in pids {
            matar_grupo(pid);
        }
    }
}

fn matar_grupo(pid: u32) {
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    );
}

/// Resumen del comando `exec` para el registro del servidor: el subcomando de
/// `magi-servicios` o los argumentos del sondeo, nunca el script entero (ni,
/// por supuesto, la contraseña de sudo, que viaja por la entrada estándar).
fn resumen_de_comando(comando: &str) -> String {
    for marca in [" magi-servicios", " magi "] {
        if let Some(posicion) = comando.rfind(marca) {
            return comando[posicion..].chars().take(160).collect();
        }
    }
    comando.chars().take(160).collect()
}

/// En una shell, el «enter» de un terminal llega como `\r`: `sh` lee líneas
/// separadas por `\n`, así que se traduce para que las sesiones con orden y
/// las teclas del usuario ejecuten.
fn retornos_a_saltos(datos: &[u8]) -> Vec<u8> {
    datos
        .iter()
        .map(|byte| if *byte == b'\r' { b'\n' } else { *byte })
        .collect()
}

impl russh::server::Handler for Manejador {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        usuario: &str,
        contrasena: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        if self.compartido.contrasena.as_deref() == Some(contrasena) {
            self.usuario = usuario.to_string();
            Ok(russh::server::Auth::Accept)
        } else {
            Ok(russh::server::Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        usuario: &str,
        _clave: &russh::keys::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        if !self.compartido.acepta_publickey {
            return Ok(russh::server::Auth::reject());
        }
        // Pruebas: cualquier clave vale y la firma no se verifica.
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

    /// Hay PTY: las sesiones con orden piden uno. La shell de pruebas no
    /// necesita un PTY de verdad (no hay `vim` que lo use); se acepta para que
    /// el cliente siga con la shell.
    async fn pty_request(
        &mut self,
        canal: russh::ChannelId,
        _terminal: &str,
        _columnas: u32,
        _filas: u32,
        _ancho: u32,
        _alto: u32,
        _modos: &[(russh::Pty, u32)],
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let _ = sesion.channel_success(canal);
        Ok(())
    }

    /// Como `sshd`, no contesta a un `window-change` que no pidió respuesta:
    /// el cliente solo quiere que el proceso lo sepa, y la shell de pruebas no
    /// depende del tamaño.
    async fn window_change_request(
        &mut self,
        _canal: russh::ChannelId,
        _columnas: u32,
        _filas: u32,
        _ancho: u32,
        _alto: u32,
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn shell_request(
        &mut self,
        canal: russh::ChannelId,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.shells.insert(u32::from(canal));
        let mut orden = tokio::process::Command::new("sh");
        self.aplicar_entorno(&mut orden);
        self.lanzar_proceso(canal, sesion, orden, true)
    }

    async fn exec_request(
        &mut self,
        canal: russh::ChannelId,
        datos: &[u8],
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let comando = String::from_utf8_lossy(datos).to_string();
        eprintln!(
            "ssh: exec «{}» ({} bytes)",
            resumen_de_comando(&comando),
            datos.len()
        );
        let mut orden = tokio::process::Command::new("sh");
        orden.arg("-c").arg(&comando);
        self.aplicar_entorno(&mut orden);
        self.lanzar_proceso(canal, sesion, orden, true)
    }

    /// El subsistema `sftp` se sirve con el `sftp-server` real de OpenSSH,
    /// como hace `sshd`: probar contra un servidor propio escondería los
    /// fallos que solo aparecen con el de verdad.
    async fn subsystem_request(
        &mut self,
        canal: russh::ChannelId,
        nombre: &str,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        if nombre != "sftp" {
            let _ = sesion.channel_failure(canal);
            return Ok(());
        }
        let Some(programa) = ruta_sftp_server() else {
            eprintln!("ssh: piden sftp pero no hay sftp-server; define MAGI_SFTP_SERVER");
            let _ = sesion.channel_failure(canal);
            return Ok(());
        };
        let orden = tokio::process::Command::new(programa);
        self.lanzar_proceso(canal, sesion, orden, false)
    }

    async fn data(
        &mut self,
        canal: russh::ChannelId,
        datos: &[u8],
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        let id = u32::from(canal);
        let datos = if self.shells.contains(&id) {
            retornos_a_saltos(datos)
        } else {
            datos.to_vec()
        };
        if let Some(entrada) = self.entradas.lock().expect("entradas").get(&id) {
            let _ = entrada.send(datos);
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        canal: russh::ChannelId,
        _sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        // EOF del cliente: se cierra el stdin del proceso. En `servicios.sh`
        // los logs usan ese fin de la entrada estándar para mandar TERM al
        // proceso real del host.
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
        // Un `exec` cortado (cancelado, plazo agotado) no sigue corriendo.
        self.matar_canal(id);
        Ok(())
    }

    /// Túnel local o dinámico: solo se reenvía al eco de la prueba. Cualquier
    /// otro destino se rechaza para que el SSH de pruebas no sirva de salto.
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
        self.guardar_mango(sesion);
        let es_loopback = matches!(host, "127.0.0.1" | "localhost" | "::1");
        if !es_loopback || puerto != u32::from(self.compartido.puerto_eco) {
            eprintln!("ssh: direct-tcpip rechazado a {host}:{puerto}");
            respuesta
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        let conexion = tokio::time::timeout(
            Duration::from_secs(5),
            TcpStream::connect(("127.0.0.1", self.compartido.puerto_eco)),
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

/// Canal de control: al recibir «tirar» corta todas las conexiones SSH, como
/// un cambio de red. Lo usan las pruebas del vigilante del móvil.
async fn servir_control(
    compartido: Arc<Compartido>,
    puerto: u16,
) -> (u16, tokio::task::JoinHandle<()>) {
    let escucha = TcpListener::bind(("127.0.0.1", puerto))
        .await
        .expect("control de pruebas en 127.0.0.1");
    let puerto = escucha
        .local_addr()
        .expect("dirección del control de pruebas")
        .port();
    eprintln!("control escuchando en 127.0.0.1:{puerto}");
    let tarea = tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            let compartido = compartido.clone();
            tokio::spawn(async move {
                let mut orden = String::new();
                let _ = flujo.read_to_string(&mut orden).await;
                match orden.trim() {
                    "tirar" => tirar_todas(&compartido).await,
                    "" => {}
                    otra => eprintln!("control: orden desconocida «{otra}»"),
                }
            });
        }
    });
    (puerto, tarea)
}

/// Corta todas las conexiones SSH abiertas, como un cambio de red.
async fn tirar_todas(compartido: &Compartido) {
    let cortados: Vec<russh::server::Handle> = {
        let mut guardia = compartido.mangos.lock().expect("mangos");
        std::mem::take(&mut *guardia)
    };
    let cuantos = cortados.len();
    for mango in cortados {
        let _ = mango
            .disconnect(
                russh::Disconnect::ByApplication,
                "prueba: se cae la red".to_string(),
                String::new(),
            )
            .await;
    }
    eprintln!("control: {cuantos} conexiones SSH tiradas");
}

/// Eco: devuelve tal cual lo recibido. Es el destino de los túneles de la
/// prueba.
async fn servir_eco(puerto: u16) -> (u16, tokio::task::JoinHandle<()>) {
    let escucha = TcpListener::bind(("127.0.0.1", puerto))
        .await
        .expect("eco de pruebas en 127.0.0.1");
    let puerto = escucha
        .local_addr()
        .expect("dirección del eco de pruebas")
        .port();
    eprintln!("eco escuchando en 127.0.0.1:{puerto}");
    let tarea = tokio::spawn(async move {
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
    (puerto, tarea)
}

/// Prepara los directorios por defecto de la prueba una sola vez por proceso
/// (las pruebas de un mismo binario pueden arrancar varios servidores en
/// paralelo y comparten los directorios fijos): el de backups de BALTHASAR-2
/// con ficheros recientes y viejos y el de trabajo de los `exec` y las shells.
fn preparar_directorios_por_defecto() {
    PREPARACION_POR_DEFECTO.call_once(|| {
        preparar_backups(Path::new(DIRECTORIO_BACKUPS));
        preparar_trabajo(Path::new(DIRECTORIO_TRABAJO));
        eprintln!(
            "backups: {DIRECTORIO_BACKUPS} con {} ficheros · trabajo: {DIRECTORIO_TRABAJO}",
            BACKUPS.len()
        );
    });
}

/// Rellena el directorio de backups de la prueba con los ficheros de
/// [`BACKUPS`] y sus edades (se rehace: es de la prueba y de nadie más).
fn preparar_backups(directorio: &Path) {
    let _ = std::fs::remove_dir_all(directorio);
    std::fs::create_dir_all(directorio).expect("directorio de backups de prueba");
    for (nombre, edad_segundos) in BACKUPS {
        let ruta = directorio.join(nombre);
        std::fs::write(&ruta, b"backup de prueba de MAGI\n").expect("fichero de backup");
        let fichero = std::fs::OpenOptions::new()
            .write(true)
            .open(&ruta)
            .expect("abrir el backup para fecharlo");
        fichero
            .set_modified(SystemTime::now() - Duration::from_secs(edad_segundos))
            .expect("fecha del backup");
    }
}

fn preparar_trabajo(directorio: &Path) {
    std::fs::create_dir_all(directorio).expect("directorio de trabajo de prueba");
}

static PREPARACION_POR_DEFECTO: std::sync::Once = std::sync::Once::new();

/// Estado del host de pruebas: el de la F13 (unidades systemd, Docker con su
/// contenedor y su proyecto Compose, sudo sin contraseña y nginx) más lo que
/// espera la fixture del móvil (`postgresql.service`, el timer `app.timer` y
/// el contenedor caído `panel`) y los diarios y logs que leen las ventanas.
fn preparar_binarios() -> BinariosFalsos {
    let binarios = BinariosFalsos::nuevos();
    binarios.preparar_estado();
    // El sondeo del móvil pide `list-units --state=failed`: solo copia.service
    // está fallida; el resto de unidades están bien.
    binarios.escribir(
        "list_units_service_failed",
        "copia.service loaded failed failed\n",
    );
    binarios.escribir("list_units_timer_failed", "");
    // preparar_estado ya deja app (activa, sin recarga), copia (fallida) y
    // nginx (activa, con recarga); se añaden postgresql y el timer.
    binarios.texto_unidad(
        "app.service",
        "[Unit]\nDescription=Aplicación de prueba\n\n[Service]\nExecStart=/usr/bin/app\n",
    );
    binarios.unidad("postgresql.service", &[]);
    binarios.unidad(
        "app.timer",
        &[
            ("ActiveState", "active"),
            ("SubState", "waiting"),
            ("Type", "oneshot"),
            ("CanReload", "no"),
        ],
    );
    // Contenedores del proyecto Compose «web»: web-app-1 en marcha y sano y
    // panel caído con salida 1. `docker_estado` es lo que lee
    // `inspect --format` (el contenedor que se consulta), así que lleva el del
    // contenedor vivo; la caída de panel se ve en el `ps` y en el estado del
    // proyecto (1 de 2 en marcha).
    binarios.escribir(
        "docker_ps",
        "{\"ID\":\"a1b2c3d4e5f6\",\"Names\":\"web-app-1\",\"Image\":\"nginx:1.27\",\"State\":\"running\",\"Status\":\"Up 7 days (healthy)\",\"Labels\":\"com.docker.compose.project=web\",\"Ports\":\"0.0.0.0:8080->80/tcp\"}\n\
         {\"ID\":\"f6e5d4c3b2a1\",\"Names\":\"panel\",\"Image\":\"panel:1.0\",\"State\":\"exited\",\"Status\":\"Exited (1) 2 hours ago\",\"Labels\":\"com.docker.compose.project=web\",\"Ports\":\"\"}\n",
    );
    binarios.escribir("docker_contenedores", "web-app-1\npanel\n");
    binarios.escribir("docker_estado", "running|healthy\n");
    binarios.escribir("docker_estado_proyecto", "running\nexited\n");
    binarios.escribir(
        "journal_short/app.service",
        "2026-11-08T12:00:00+00:00 aviso: el servicio no arrancó\n",
    );
    binarios.escribir(
        "journal/app.service",
        "{\"__REALTIME_TIMESTAMP\":\"1794139824180943\",\"PRIORITY\":\"6\",\"MESSAGE\":\"arrancando app\"}\n\
         {\"__REALTIME_TIMESTAMP\":\"1794139825180943\",\"PRIORITY\":\"6\",\"MESSAGE\":\"app en marcha\"}\n",
    );
    binarios.escribir(
        "docker_logs_out",
        "2026-11-08T12:00:00Z arrancando web\n2026-11-08T12:00:01Z web en marcha\n",
    );
    binarios.escribir("docker_logs_corto", "2026-11-08T12:00:01Z web en marcha\n");
    binarios
}

/// Ruta del `sftp-server` de OpenSSH, si está instalado.
///
/// `MAGI_SFTP_SERVER` manda si apunta a un fichero existente: los runners de
/// CI y las instalaciones de macOS/Arch no siempre usan las rutas habituales.
fn ruta_sftp_server() -> Option<PathBuf> {
    if let Some(ruta) = std::env::var_os("MAGI_SFTP_SERVER") {
        let ruta = PathBuf::from(ruta);
        if ruta.exists() {
            return Some(ruta);
        }
    }
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

/// Puertos del servidor de pruebas. Un 0 pide uno efímero al sistema; los
/// puertos reales se leen en [`ServidorPruebas::puertos`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PuertosPrueba {
    pub ssh: u16,
    pub eco: u16,
    pub control: u16,
}

impl PuertosPrueba {
    /// Los puertos fijos que esperan las fixtures del emulador y el ejemplo
    /// `servidor_pruebas`.
    pub const fn fijos() -> Self {
        Self {
            ssh: 12222,
            eco: 19091,
            control: 12223,
        }
    }

    /// Todos efímeros: el sistema elige y se leen en `ServidorPruebas::puertos`.
    pub const fn efimeros() -> Self {
        Self {
            ssh: 0,
            eco: 0,
            control: 0,
        }
    }
}

impl Default for PuertosPrueba {
    fn default() -> Self {
        Self::efimeros()
    }
}

/// Cómo montar el host de pruebas. Lo normal es [`OpcionesPrueba::default()`]:
/// puertos efímeros, cualquier clave pública y contraseña «secreta».
#[derive(Debug, Clone)]
pub struct OpcionesPrueba {
    pub puertos: PuertosPrueba,
    /// ¿Se acepta cualquier clave pública? En `false` se rechaza el método
    /// `publickey` (para probar el error de credenciales del móvil).
    pub acepta_publickey: bool,
    /// Contraseña que se acepta, si la hay.
    pub contrasena: Option<String>,
    /// Directorio de backups para BALTHASAR-2. Si es `None` se prepara el
    /// fijo ([`DIRECTORIO_BACKUPS`]) con los ficheros de la prueba; si se da,
    /// se respeta tal cual y lo rellena la prueba.
    pub directorio_backups: Option<PathBuf>,
    /// Directorio de trabajo de los `exec` y las shells. Si es `None` es
    /// [`DIRECTORIO_TRABAJO`] (se crea); si se da, se usa ese.
    pub directorio_trabajo: Option<PathBuf>,
}

impl Default for OpcionesPrueba {
    fn default() -> Self {
        Self {
            puertos: PuertosPrueba::efimeros(),
            acepta_publickey: true,
            contrasena: Some("secreta".to_string()),
            directorio_backups: None,
            directorio_trabajo: None,
        }
    }
}

/// Servidor de pruebas en marcha. Al soltarlo (o al llamar a [`Self::parar`])
/// deja de escuchar y pide a las conexiones que se desconecten.
pub struct ServidorPruebas {
    /// Puertos reales en los que escucha, aunque se hayan pedido efímeros.
    pub puertos: PuertosPrueba,
    compartido: Arc<Compartido>,
    tareas: Vec<tokio::task::JoinHandle<()>>,
    apagado: Option<russh::server::RunningServerHandle>,
    clave_host: ssh_key::PublicKey,
}

impl ServidorPruebas {
    /// Clave pública del host de pruebas (la que llevan los `known_hosts`).
    pub fn clave_host(&self) -> &ssh_key::PublicKey {
        &self.clave_host
    }

    /// Línea de `known_hosts` de este servidor: `[127.0.0.1]:<puerto> …`.
    pub fn linea_known_hosts(&self) -> String {
        format!(
            "[127.0.0.1]:{} {}",
            self.puertos.ssh,
            self.clave_host
                .to_openssh()
                .expect("clave pública del servidor de pruebas")
        )
    }

    /// Escribe [`Self::linea_known_hosts`] en `ruta` (con permisos 600).
    pub fn escribir_known_hosts(&self, ruta: &Path) {
        let linea = format!("{}\n", self.linea_known_hosts());
        std::fs::write(ruta, linea).expect("escribiendo el known_hosts de prueba");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(0o600))
                .expect("permisos del known_hosts de prueba");
        }
    }

    /// Los binarios falsos instalados (estado y guiones del host de prueba).
    pub fn binarios(&self) -> &BinariosFalsos {
        &self.compartido.binarios
    }

    /// Directorio de backups de BALTHASAR-2.
    pub fn directorio_backups(&self) -> &Path {
        &self.compartido.directorio_backups
    }

    /// Directorio de trabajo de los `exec` y las shells.
    pub fn directorio_trabajo(&self) -> &Path {
        &self.compartido.directorio_trabajo
    }

    /// Corta todas las conexiones SSH, como un cambio de red (el vigilante).
    pub async fn tirar_conexiones(&self) {
        tirar_todas(&self.compartido).await;
    }

    /// Para el servidor: deja de escuchar y corta las conexiones.
    pub async fn parar(mut self) {
        self.parar_interno().await;
    }

    async fn parar_interno(&mut self) {
        if let Some(apagado) = self.apagado.take() {
            apagado.shutdown("prueba: se para el servidor".to_string());
        }
        // Un respiro para que las conexiones reciban el DISCONNECT.
        tokio::time::sleep(Duration::from_millis(50)).await;
        for tarea in self.tareas.drain(..) {
            tarea.abort();
        }
    }
}

impl Drop for ServidorPruebas {
    fn drop(&mut self) {
        if let Some(apagado) = self.apagado.take() {
            apagado.shutdown("prueba: se para el servidor".to_string());
        }
        for tarea in self.tareas.drain(..) {
            tarea.abort();
        }
    }
}

/// Arranca el servidor de pruebas en `127.0.0.1` con los puertos dados (0 =
/// efímero) y el resto de opciones por defecto.
pub async fn arrancar(puertos: PuertosPrueba) -> ServidorPruebas {
    arrancar_con(OpcionesPrueba {
        puertos,
        ..OpcionesPrueba::default()
    })
    .await
}

/// Arranca el servidor de pruebas con todas las opciones.
pub async fn arrancar_con(opciones: OpcionesPrueba) -> ServidorPruebas {
    let directorio_backups = match opciones.directorio_backups.clone() {
        Some(ruta) => ruta,
        None => {
            preparar_directorios_por_defecto();
            PathBuf::from(DIRECTORIO_BACKUPS)
        }
    };
    let directorio_trabajo = match opciones.directorio_trabajo.clone() {
        Some(ruta) => {
            preparar_trabajo(&ruta);
            ruta
        }
        None => {
            preparar_directorios_por_defecto();
            PathBuf::from(DIRECTORIO_TRABAJO)
        }
    };

    let clave = ssh_key::PrivateKey::from_openssh(CLAVE_HOST).expect("clave de host de pruebas");
    let clave_host = clave.public_key().clone();

    // El eco primero: el SSH de pruebas reenvía los túneles a su puerto real.
    let (puerto_eco, tarea_eco) = servir_eco(opciones.puertos.eco).await;
    let binarios = preparar_binarios();
    eprintln!(
        "binarios falsos: {} (estado en {})",
        binarios.ruta().display(),
        binarios.estado().display()
    );
    let compartido = Arc::new(Compartido {
        mangos: Mutex::new(Vec::new()),
        binarios,
        puerto_eco,
        directorio_backups: directorio_backups.clone(),
        directorio_trabajo: directorio_trabajo.clone(),
        acepta_publickey: opciones.acepta_publickey,
        contrasena: opciones.contrasena,
    });
    let (puerto_control, tarea_control) =
        servir_control(compartido.clone(), opciones.puertos.control).await;

    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave],
        ..Default::default()
    });
    let escucha = TcpListener::bind(("127.0.0.1", opciones.puertos.ssh))
        .await
        .expect("ssh de pruebas en 127.0.0.1");
    let puerto_ssh = escucha
        .local_addr()
        .expect("dirección del ssh de pruebas")
        .port();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let compartido_ssh = compartido.clone();
    let tarea_ssh = tokio::spawn(async move {
        let mut servidor = Servidor {
            compartido: compartido_ssh,
        };
        let en_marcha = russh::server::Server::run_on_socket(&mut servidor, config, &escucha);
        let _ = tx.send(en_marcha.handle());
        let _ = en_marcha.await;
    });
    let apagado = rx.await.expect("el servidor SSH de pruebas arrancó");
    eprintln!("ssh escuchando en 127.0.0.1:{puerto_ssh}");
    ServidorPruebas {
        puertos: PuertosPrueba {
            ssh: puerto_ssh,
            eco: puerto_eco,
            control: puerto_control,
        },
        compartido,
        tareas: vec![tarea_eco, tarea_control, tarea_ssh],
        apagado: Some(apagado),
        clave_host,
    }
}
