//! Pruebas de voxl por SSH (Fase 12), sin red externa: un servidor SSH de
//! prueba en proceso, un voxl falso en el "host" y un HTTP mínimo de tokio que
//! hace de voxl para /healthz y para el tráfico del túnel.
//!
//! El servidor SSH de prueba acepta una contraseña conocida y ejecuta los
//! comandos con sh -c como un sshd de verdad; el voxl falso es un script que
//! apunta sus argumentos, su entorno y su /proc/self/cmdline y se queda vivo
//! hasta que el script embebido le manda TERM.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use magi_nucleo::conexion::cliente::Cliente;
use magi_nucleo::conexion::EventoConexion;
use magi_nucleo::escritorio::voxl::{
    arrancar, CanalExec, ConfigVoxl, EstadoVoxl, HostSsh, HostSshRussh,
};
use magi_nucleo::escritorio::{ErrorEscritorio, Flujo, OpcionesEscritorio, TipoHost};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;

/// Plazo holgado para las pruebas (los plazos de la implementación son de 10 s).
const PLAZO: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------- servidor SSH

/// Lo que el test ve del host de pruebas.
#[derive(Default)]
struct Observado {
    /// Comandos exec recibidos, en orden.
    comandos: Mutex<Vec<String>>,
    /// Destinos de cada canal direct-tcpip.
    destinos: Mutex<Vec<String>>,
    /// Mangos de las sesiones, para tirar la conexión como una caída de red.
    mangos: Mutex<Vec<russh::server::Handle>>,
}

impl Observado {
    fn comando_ejecutado(&self) -> String {
        self.comandos
            .lock()
            .expect("comandos")
            .first()
            .cloned()
            .unwrap_or_default()
    }

    fn destinos(&self) -> Vec<String> {
        self.destinos.lock().expect("destinos").clone()
    }

    /// Corta todas las conexiones abiertas, como una caída de red.
    async fn tirar_conexiones(&self) {
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

/// Servidor SSH de pruebas servido por russh, en proceso.
struct ServidorSsh {
    observado: Arc<Observado>,
}

impl russh::server::Server for ServidorSsh {
    type Handler = HandlerSsh;

    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> HandlerSsh {
        HandlerSsh {
            entradas: Arc::new(Mutex::new(HashMap::new())),
            procesos: Arc::new(Mutex::new(HashMap::new())),
            observado: self.observado.clone(),
        }
    }
}

struct HandlerSsh {
    /// stdin de cada exec, por canal.
    entradas: Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>>,
    /// pid de cada exec, para matarlo si el canal se cierra antes.
    procesos: Arc<Mutex<HashMap<u32, u32>>>,
    observado: Arc<Observado>,
}

impl Drop for HandlerSsh {
    fn drop(&mut self) {
        for (_, pid) in self.procesos.lock().expect("procesos").drain() {
            matar_grupo(pid);
        }
    }
}

/// Mata el grupo de procesos de un exec (el sh -c y lo que lance).
fn matar_grupo(pid: u32) {
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    );
}

impl russh::server::Handler for HandlerSsh {
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
        Ok(russh::server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _canal: russh::Channel<russh::server::Msg>,
        respuesta: russh::server::ChannelOpenHandle,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.observado
            .mangos
            .lock()
            .expect("mangos")
            .push(sesion.handle());
        respuesta.accept().await;
        Ok(())
    }

    /// exec: el comando corre en local con sh -c, stdout por data, stderr por
    /// extended_data(1) y, al terminar, exit-status, EOF y cierre.
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
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
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
        let procesos = self.procesos.clone();
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
        // EOF del cliente: se cierra el stdin del proceso.
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
        if let Some(pid) = self.procesos.lock().expect("procesos").remove(&id) {
            matar_grupo(pid);
        }
        Ok(())
    }

    /// direct-tcpip: el host abre el destino como un sshd de verdad.
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
        self.observado
            .destinos
            .lock()
            .expect("destinos")
            .push(format!("{host}:{puerto}"));
        let conexion = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::TcpStream::connect((host, puerto as u16)),
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

// ---------------------------------------------------------------- voxl falso

/// Qué hace el voxl falso al arrancar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comportamiento {
    /// Se queda vivo hasta que le llega TERM (el caso normal).
    Normal,
    /// Falla como un voxl sin pantalla X11.
    FallaPantalla,
    /// Falla como un voxl que no puede escuchar en el puerto.
    PuertoOcupado,
    /// Manda un flujo enorme sin saltos de línea (host comprometido).
    LineaSinFin,
}

impl Comportamiento {
    fn marca(self) -> &'static str {
        match self {
            Comportamiento::Normal => "normal",
            Comportamiento::FallaPantalla => "pantalla",
            Comportamiento::PuertoOcupado => "puerto",
            Comportamiento::LineaSinFin => "linea",
        }
    }
}

/// Plantilla del voxl falso: escribe sus marcas en el directorio donde vive
/// (el markador no interfiere con la línea de órdenes, que también se mira).
const PLANTILLA_VOXL_FALSO: &str = r#"#!/bin/sh
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1
if [ "$1" = "--version" ]; then
  echo "voxl __VERSION__"
  exit 0
fi
if [ "__COMPORTAMIENTO__" = "pantalla" ]; then
  echo "X connect: X11 error: no such display" >&2
  exit 1
fi
if [ "__COMPORTAMIENTO__" = "puerto" ]; then
  echo 'bind: Os { code: 98, kind: AddrInUse, message: "Address already in use" }' >&2
  exit 1
fi
if [ "__COMPORTAMIENTO__" = "linea" ]; then
  # Host comprometido: 4 MB sin un solo salto de línea por stdout.
  printf '%s' "$$" > "$dir/pid"
  bloque=$(printf '%1024s' '' | tr ' ' A)
  i=0
  while [ "$i" -lt 4096 ]; do
    printf '%s' "$bloque"
    i=$((i + 1))
  done
  while :; do sleep 0.2; done
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

fn crear_voxl_falso(dir: &Path, version: &str, comportamiento: Comportamiento) -> PathBuf {
    let ruta = dir.join("voxl-falso");
    let contenido = PLANTILLA_VOXL_FALSO
        .replace("__VERSION__", version)
        .replace("__COMPORTAMIENTO__", comportamiento.marca());
    std::fs::write(&ruta, contenido).expect("voxl falso");
    std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755)).expect("permisos");
    ruta
}

// ---------------------------------------------------------------- servicio

/// HTTP mínimo de tokio que hace de voxl: /healthz y una respuesta reconocible
/// para comprobar que el tráfico del túnel llega hasta el "host".
struct ServicioVoxl {
    puerto: u16,
    salud: Arc<AtomicUsize>,
    _tarea: tokio::task::JoinHandle<()>,
}

async fn arrancar_servicio_voxl() -> ServicioVoxl {
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("escucha del servicio");
    let puerto = escucha.local_addr().expect("dirección del servicio").port();
    let salud = Arc::new(AtomicUsize::new(0));
    let contador = salud.clone();
    let tarea = tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            let contador = contador.clone();
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
                let primera = String::from_utf8_lossy(&peticion)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                let respuesta: &[u8] = if primera.starts_with("GET /healthz") {
                    contador.fetch_add(1, Ordering::SeqCst);
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                } else if primera.starts_with("GET /eco") {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nvoxl!"
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                let _ = flujo.write_all(respuesta).await;
                let _ = flujo.shutdown().await;
            });
        }
    });
    ServicioVoxl {
        puerto,
        salud,
        _tarea: tarea,
    }
}

/// Un puerto libre en loopback (para las pruebas de error, que no traen servicio).
async fn puerto_libre() -> u16 {
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("puerto libre");
    escucha.local_addr().expect("dirección").port()
}

// ---------------------------------------------------------------- montaje

/// Todo lo que necesita una prueba: el host SSH de pruebas, el voxl falso, el
/// servicio HTTP y la conexión ya autenticada.
struct Montaje {
    temporal: tempfile::TempDir,
    observado: Arc<Observado>,
    handle: Arc<russh::client::Handle<Cliente>>,
    puerto_voxl: u16,
    servicio: Option<ServicioVoxl>,
    _tarea_ssh: tokio::task::JoinHandle<()>,
}

impl Montaje {
    fn directorio(&self) -> &Path {
        self.temporal.path()
    }
}

async fn montar(version: &str, comportamiento: Comportamiento, con_servicio: bool) -> Montaje {
    let temporal = tempfile::tempdir().expect("temporal");
    // El servicio (el voxl de verdad de la prueba) se levanta antes del
    // arranque: /healthz tiene que existir cuando el sondeo llegue.
    let servicio = if con_servicio {
        Some(arrancar_servicio_voxl().await)
    } else {
        None
    };
    let puerto_voxl = match &servicio {
        Some(servicio) => servicio.puerto,
        None => puerto_libre().await,
    };

    // 1. Servidor SSH de prueba con una clave conocida de antemano.
    let clave_host = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .expect("clave del host de pruebas");
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave_host.clone()],
        ..Default::default()
    });
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("escucha SSH");
    let puerto_ssh = escucha.local_addr().expect("dirección SSH").port();
    let observado = Arc::new(Observado::default());
    let mut servidor = ServidorSsh {
        observado: observado.clone(),
    };
    let tarea_ssh = tokio::spawn(async move {
        let _ = russh::server::Server::run_on_socket(&mut servidor, config, &escucha).await;
    });

    // 2. El voxl falso en el "host", y su huella en known_hosts.
    crear_voxl_falso(temporal.path(), version, comportamiento);
    let known_hosts = temporal.path().join("known_hosts");
    std::fs::write(
        &known_hosts,
        format!(
            "[127.0.0.1]:{puerto_ssh} {}\n",
            clave_host
                .public_key()
                .to_openssh()
                .expect("clave pública del host")
        ),
    )
    .expect("known_hosts");

    // 3. La conexión SSH, con el mismo manejador que usa MAGI (known_hosts
    // incluido), ya autenticada.
    let (tx, _eventos) = mpsc::unbounded_channel::<EventoConexion>();
    let manejador = Cliente::nuevo(tx, 1, "127.0.0.1", puerto_ssh, known_hosts, false);
    let mut handle = tokio::time::timeout(
        PLAZO,
        russh::client::connect(
            Arc::new(russh::client::Config::default()),
            ("127.0.0.1", puerto_ssh),
            manejador,
        ),
    )
    .await
    .expect("plazo conectando con el host de pruebas")
    .expect("conexión SSH de pruebas");
    let autenticado =
        tokio::time::timeout(PLAZO, handle.authenticate_password("usuario", "secreta"))
            .await
            .expect("plazo autenticando")
            .expect("autenticación SSH");
    assert!(
        autenticado.success(),
        "el host de pruebas acepta la contraseña"
    );

    Montaje {
        temporal,
        observado,
        handle: Arc::new(handle),
        puerto_voxl,
        servicio,
        _tarea_ssh: tarea_ssh,
    }
}

/// Opciones del host voxl apuntando al voxl falso de la prueba.
fn opciones_de_prueba(montaje: &Montaje, portapapeles: bool) -> OpcionesEscritorio {
    let mut opciones = OpcionesEscritorio::por_defecto(TipoHost::Voxl);
    opciones.pantalla = Some(":1".to_string());
    opciones.programa = Some(
        montaje
            .directorio()
            .join("voxl-falso")
            .display()
            .to_string(),
    );
    opciones.portapapeles = portapapeles;
    opciones
}

fn host_de_prueba(montaje: &Montaje) -> Arc<dyn HostSsh> {
    Arc::new(HostSshRussh::nuevo(montaje.handle.clone()))
}

fn config_de_prueba(montaje: &Montaje) -> ConfigVoxl {
    ConfigVoxl {
        nombre: "web-01".to_string(),
        puerto: montaje.puerto_voxl,
    }
}

/// Arranca y exige éxito (con plazo de prueba).
async fn arrancar_voxl(
    montaje: &Montaje,
    opciones: OpcionesEscritorio,
) -> magi_nucleo::escritorio::voxl::SesionVoxl {
    tokio::time::timeout(
        PLAZO,
        arrancar(config_de_prueba(montaje), host_de_prueba(montaje), opciones),
    )
    .await
    .expect("plazo de arranque")
    .expect("voxl debería arrancar")
}

/// Arranca y devuelve el error legible.
async fn error_de_arranque(montaje: &Montaje, opciones: OpcionesEscritorio) -> ErrorEscritorio {
    tokio::time::timeout(
        PLAZO,
        arrancar(config_de_prueba(montaje), host_de_prueba(montaje), opciones),
    )
    .await
    .expect("plazo de arranque")
    .expect_err("el arranque debía fallar")
}

// ---------------------------------------------------------------- ayudas

/// Espera a que un fichero del host de pruebas exista y tenga contenido.
async fn esperar_fichero(ruta: &Path, plazo: Duration) -> String {
    let fin = tokio::time::Instant::now() + plazo;
    loop {
        if let Ok(texto) = std::fs::read_to_string(ruta) {
            if !texto.is_empty() {
                return texto;
            }
        }
        assert!(
            tokio::time::Instant::now() < fin,
            "no apareció {}",
            ruta.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Espera a que se cumpla una condición booleana.
async fn esperar<F>(mut condicion: F, plazo: Duration, mensaje: &str)
where
    F: FnMut() -> bool,
{
    let fin = tokio::time::Instant::now() + plazo;
    while tokio::time::Instant::now() < fin {
        if condicion() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{mensaje}");
}

/// Lee de un flujo hasta que el otro extremo cierra.
async fn leer_respuesta(flujo: &mut tokio::net::TcpStream) -> String {
    let mut respuesta = Vec::new();
    let mut bufer = [0u8; 1024];
    loop {
        match tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer)).await {
            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
            Ok(Ok(leidos)) => {
                respuesta.extend_from_slice(&bufer[..leidos]);
                if respuesta.len() > 8192 {
                    break;
                }
            }
        }
    }
    String::from_utf8_lossy(&respuesta).to_string()
}

// ---------------------------------------------------------------- pruebas

/// Un canal que no se debe tocar: si el arranque lo usa, la prueba falla.
struct HostNoUsado;

#[async_trait::async_trait]
impl HostSsh for HostNoUsado {
    async fn exec(
        &self,
        _programa: &str,
        _argumentos: &[String],
    ) -> Result<CanalExec, ErrorEscritorio> {
        panic!("no se debe abrir un canal exec con argumentos inválidos");
    }

    async fn direct_tcpip(&self, _direccion: &str, _puerto: u16) -> Result<Flujo, ErrorEscritorio> {
        panic!("no se debe abrir un direct-tcpip con argumentos inválidos");
    }

    fn conexion_viva(&self) -> bool {
        panic!("no se debe consultar la conexión con argumentos inválidos");
    }
}

#[tokio::test]
async fn arranca_espera_salud_abre_tunel_y_para_voxl() {
    let montaje = montar("0.3.3", Comportamiento::Normal, true).await;
    let mut sesion = arrancar_voxl(&montaje, opciones_de_prueba(&montaje, true)).await;

    assert_eq!(sesion.version, "0.3.3");
    assert_eq!(sesion.puerto_remoto(), montaje.puerto_voxl);
    assert_ne!(sesion.puerto_tunel, montaje.puerto_voxl);
    assert!(
        sesion.estado().esta_abierto(),
        "estado: {:?}",
        sesion.estado()
    );
    assert_eq!(
        *sesion.url(),
        format!(
            "http://127.0.0.1:{}/?token={}",
            sesion.puerto_tunel,
            sesion.clave.as_str()
        )
    );
    assert_eq!(sesion.clave.len(), 32);
    assert!(sesion
        .clave
        .chars()
        .all(|caracter| caracter.is_ascii_hexdigit()));

    // 1. Argumentos que recibió el voxl falso: la orden los llevó tal cual.
    let argumentos = esperar_fichero(&montaje.directorio().join("argumentos"), PLAZO).await;
    let esperados = vec![
        "--addr=127.0.0.1".to_string(),
        format!("--port={}", montaje.puerto_voxl),
        "--enable-resize=true".to_string(),
        "--file-transfers=none".to_string(),
        "--audio-enabled=false".to_string(),
        "--microphone-enabled=false".to_string(),
        "--enable-clipboard=true".to_string(),
    ];
    let recibidos: Vec<String> = argumentos.lines().map(|linea| linea.to_string()).collect();
    assert_eq!(recibidos, esperados);

    // 2. La contraseña llegó por la entrada estándar, no por la orden.
    let clave = esperar_fichero(&montaje.directorio().join("clave"), PLAZO).await;
    assert_eq!(clave, *sesion.clave);
    let cmdline = esperar_fichero(&montaje.directorio().join("cmdline"), PLAZO).await;
    assert!(
        !cmdline.contains(sesion.clave.as_str()),
        "la contraseña no puede estar en la línea de órdenes: {cmdline}"
    );
    let display = esperar_fichero(&montaje.directorio().join("display"), PLAZO).await;
    assert_eq!(display, ":1");

    // 3. La orden exec lleva el script como argumento de sh, sin interpolar.
    let comando = montaje.observado.comando_ejecutado();
    assert!(comando.starts_with("sh -c '"), "{comando}");
    assert!(comando.contains("magi-voxl"), "{comando}");
    assert!(comando.contains("--addr=127.0.0.1"), "{comando}");
    assert!(!comando.contains(sesion.clave.as_str()), "{comando}");

    // 4. /healthz se sondeó por direct-tcpip al puerto del host.
    let salud = montaje
        .servicio
        .as_ref()
        .expect("servicio")
        .salud
        .load(Ordering::SeqCst);
    assert!(salud > 0, "el arranque debe haber sondeado /healthz");
    let destino = format!("127.0.0.1:{}", montaje.puerto_voxl);
    assert!(
        montaje.observado.destinos().contains(&destino),
        "destinos: {:?}",
        montaje.observado.destinos()
    );

    // 5. El túnel lleva el tráfico hasta el voxl del host y cuenta conexiones.
    let mut cliente = tokio::net::TcpStream::connect(("127.0.0.1", sesion.puerto_tunel))
        .await
        .expect("conectar al túnel");
    cliente
        .write_all(b"GET /eco HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .await
        .expect("petición por el túnel");
    esperar(
        || sesion.conexiones_abiertas() >= 1,
        Duration::from_secs(5),
        "el túnel debe contar la conexión abierta",
    )
    .await;
    let respuesta = leer_respuesta(&mut cliente).await;
    assert!(
        respuesta.contains("voxl!"),
        "el túnel debe llegar al voxl de la prueba: {respuesta}"
    );
    drop(cliente);
    esperar(
        || sesion.conexiones_abiertas() == 0,
        Duration::from_secs(5),
        "la conexión cerrada debe descontarse",
    )
    .await;

    // 6. La parada cierra la entrada: el script manda TERM, voxl termina y el
    // túnel deja de escuchar.
    tokio::time::timeout(PLAZO, sesion.parar())
        .await
        .expect("plazo de parada")
        .expect("parar la sesión");
    assert!(
        matches!(sesion.estado(), EstadoVoxl::Cerrado { .. }),
        "estado tras parar: {:?}",
        sesion.estado()
    );
    esperar_fichero(&montaje.directorio().join("parada"), PLAZO).await;
    let pid: i32 = esperar_fichero(&montaje.directorio().join("pid"), PLAZO)
        .await
        .trim()
        .parse()
        .expect("pid del voxl falso");
    esperar(
        || nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err(),
        Duration::from_secs(5),
        "el voxl falso debe haber terminado",
    )
    .await;
    let conexion = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(("127.0.0.1", sesion.puerto_tunel)),
    )
    .await;
    assert!(
        !matches!(conexion, Ok(Ok(_))),
        "el túnel debe estar cerrado tras parar"
    );
}

#[tokio::test]
async fn sin_voxl_el_error_dice_que_no_esta_instalado() {
    let montaje = montar("0.3.3", Comportamiento::Normal, false).await;
    let mut opciones = opciones_de_prueba(&montaje, true);
    opciones.programa = Some(
        montaje
            .directorio()
            .join("no-existe")
            .join("voxl")
            .display()
            .to_string(),
    );
    let error = error_de_arranque(&montaje, opciones).await;
    assert_eq!(
        error.motivo(),
        "voxl no está instalado en web-01: instala voxl-server"
    );
}

#[tokio::test]
async fn una_version_antigua_pide_actualizar() {
    let montaje = montar("0.2.9", Comportamiento::Normal, false).await;
    let error = error_de_arranque(&montaje, opciones_de_prueba(&montaje, false)).await;
    assert_eq!(error.motivo(), "actualiza voxl a la 0.3 o posterior");
}

#[tokio::test]
async fn una_pantalla_inexistente_se_traduce() {
    let montaje = montar("0.3.3", Comportamiento::FallaPantalla, false).await;
    let error = error_de_arranque(&montaje, opciones_de_prueba(&montaje, false)).await;
    assert_eq!(error.motivo(), "la pantalla :1 no existe");
}

#[tokio::test]
async fn un_puerto_ocupado_se_traduce() {
    let montaje = montar("0.3.3", Comportamiento::PuertoOcupado, false).await;
    let error = error_de_arranque(&montaje, opciones_de_prueba(&montaje, false)).await;
    assert_eq!(
        error.motivo(),
        format!(
            "el puerto {} del host está ocupado: cambia el puerto de voxl en la ficha",
            montaje.puerto_voxl
        )
    );
}

#[tokio::test]
async fn healthz_que_no_responde_falla_a_los_diez_segundos() {
    let montaje = montar("0.3.3", Comportamiento::Normal, false).await;
    let inicio = tokio::time::Instant::now();
    let error = error_de_arranque(&montaje, opciones_de_prueba(&montaje, false)).await;
    assert_eq!(error.motivo(), "voxl no respondió en 10 s");
    assert!(
        inicio.elapsed() >= Duration::from_secs(9),
        "el plazo debe respetarse: {:?}",
        inicio.elapsed()
    );
}

#[tokio::test]
async fn una_linea_sin_fin_del_host_se_corta_y_falla() {
    let montaje = montar("0.3.3", Comportamiento::LineaSinFin, false).await;
    let inicio = tokio::time::Instant::now();
    let error = error_de_arranque(&montaje, opciones_de_prueba(&montaje, false)).await;
    assert!(
        error.motivo().contains("sin fin"),
        "el error debe diagnosticar el flujo sin fin: {}",
        error.motivo()
    );
    assert!(
        inicio.elapsed() < Duration::from_secs(9),
        "debe cortar al pasar del máximo, sin esperar al plazo: {:?}",
        inicio.elapsed()
    );
    // El canal se cierra y el voxl falso no queda vivo.
    let pid: i32 = esperar_fichero(&montaje.directorio().join("pid"), PLAZO)
        .await
        .trim()
        .parse()
        .expect("pid del voxl falso");
    esperar(
        || nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err(),
        Duration::from_secs(8),
        "el voxl falso debe terminar al cortarse el canal",
    )
    .await;
}

#[tokio::test]
async fn los_argumentos_invalidos_se_rechazan_antes_de_tocar_el_host() {
    let host: Arc<dyn HostSsh> = Arc::new(HostNoUsado);
    let mut opciones = OpcionesEscritorio::por_defecto(TipoHost::Voxl);
    opciones.pantalla = Some(":1; rm -rf".to_string());
    opciones.programa = Some("/usr/bin/voxl".to_string());
    let error = arrancar(
        ConfigVoxl {
            nombre: "web-01".to_string(),
            puerto: 6901,
        },
        host.clone(),
        opciones,
    )
    .await
    .expect_err("pantalla inválida");
    assert_eq!(error.motivo(), "pantalla o programa inválidos");

    let mut opciones = OpcionesEscritorio::por_defecto(TipoHost::Voxl);
    opciones.programa = Some("/usr/bin/voxl".to_string());
    let error = arrancar(
        ConfigVoxl {
            nombre: "web-01".to_string(),
            puerto: 80,
        },
        host,
        opciones,
    )
    .await
    .expect_err("puerto reservado");
    assert!(error.motivo().contains("1024"), "{}", error.motivo());
}

#[tokio::test]
async fn si_cae_la_conexion_ssh_la_sesion_queda_caida() {
    let montaje = montar("0.3.3", Comportamiento::Normal, true).await;
    let mut sesion = arrancar_voxl(&montaje, opciones_de_prueba(&montaje, true)).await;
    assert!(sesion.estado().esta_abierto());

    montaje.observado.tirar_conexiones().await;
    esperar(
        || sesion.estado().caido(),
        PLAZO,
        "la sesión debe quedar caída al perderse la conexión",
    )
    .await;
    let motivo = sesion
        .estado()
        .motivo()
        .expect("motivo de la caída")
        .to_string();
    assert!(
        motivo.contains("conexión") || motivo.contains("voxl"),
        "motivo: {motivo}"
    );

    // Con la conexión caída, parar no puede quedarse colgado.
    tokio::time::timeout(PLAZO, sesion.parar())
        .await
        .expect("plazo de parada con la conexión caída")
        .expect("parar la sesión caída");
    assert!(!sesion.estado().esta_abierto());
}
