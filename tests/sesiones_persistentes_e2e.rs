//! Prueba de extremo a extremo de las sesiones persistentes (Fase 16): el
//! servidor de sesiones de MAGI en el mismo proceso, el servidor SSH de pruebas
//! de `tests/comun` y el **tmux de verdad** (el del `PATH`, o el del directorio
//! `MAGI_TMUX_DIR`) con los scripts `terminal_tmux.sh` (por la entrada estándar
//! de los `exec`, como lo manda MAGI) y `terminal_enganche.sh` sin tocar. Sin red
//! externa y sin tocar el tmux ni el `~/.tmux.conf` del usuario: cada prueba
//! lleva su `HOME` y su `TMUX_TMPDIR` y, al acabar, mata sus servidores de tmux.
//!
//! El host de pruebas ejecuta los `exec` con `sh -c` y entubados (sin PTY), así
//! que `attach-session` (que necesita un terminal) pasa por un envoltorio de
//! `tmux` que lo ejecuta dentro de `script -qefc`, el mismo recurso que usa
//! `crates/magi-nucleo/tests/tmux_real.rs`. Las demás órdenes de tmux van tal
//! cual. El «otro equipo» es el enganche mínimo (`terminal_enganche.sh`), lanzado
//! también con `script` contra el mismo servidor `-L magi`.
//!
//! Recorre lo que el informe exige de la fase: crear una sesión y escribir en
//! ella; cerrar la ventana y volver desde otra con la sesión viva y su
//! contenido; soltar (un segundo equipo se la lleva) y volver; la atención por
//! la campana; terminar con el nombre exacto (`magi-a-1` junto a `magi-a-10`);
//! un host que cae (sus fichas quedan `inalcanzable`, nunca `terminada`); la
//! reconexión automática tras un corte de red (vuelve sola a la sesión libre y
//! no se la quita a otro equipo) y Mayús+Intro con `modifyOtherKeys` dentro
//! del tmux de MAGI (`tui_prueba`).
//!
//! Se omiten con un aviso si no hay tmux 3.0 o posterior o si falta el comando
//! `script` de util-linux, salvo con la variable `CI` puesta (`ci.yml` instala
//! tmux): allí fallan, para que no pasen vacías. Las esperas son por
//! condición, con plazo; solo las comprobaciones de «no ocurre nada» miran
//! durante un rato corto.

mod comun;

use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt as _;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio_stream::StreamExt as _;
use tokio_util::codec::FramedRead;

use magi::protocolo::{
    codec, decodificar, CrearTmux, EstadoFicha, EstadoSesionRemota, FichaVista, InfoSesion,
    LinesCodec, ListaPersistentes, MensajeCliente, MensajeServidor, PeticionTmux,
    ResultadoRenombrar, ResultadoTmux, ServidorTmux, RANGO_TERMINAL, VERSION_PROTOCOLO,
};
use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha, VersionTmux};
use magi_nucleo::terminal::emulador::PantallaEmulada;
use magi_nucleo::terminal::teclas::{
    codificar as codificar_tecla, Modificadores, ModoTerminal, Tecla,
};
use magi_nucleo::terminal::tmux::{etiqueta_equipo, version_de_texto, SCRIPT, SCRIPT_ENGANCHE};

use comun::{escenario, Escenario, OpcionesEscenario};

/// Plazo de las esperas por condición (con tmux real, el arranque del servidor
/// y de la shell de la sesión tarda decenas de milisegundos, no segundos).
const PLAZO: Duration = Duration::from_secs(20);

/// Ritmo del vigilante en las pruebas (milisegundos).
const VIGILAR_MS: u64 = 250;

// ------------------------------------------------------------- tmux real

/// El `tmux` que ven los `exec` del host de pruebas: el real, salvo para
/// `attach-session`, que va dentro de `script` para darle un terminal (el host
/// de pruebas no reparte PTY). El tamaño del terminal es el 80×24 que piden
/// todas las pruebas.
const ENVOLTORIO_TMUX: &str = r#"#!/bin/sh
unset TMUX
engancha=no
for argumento in "$@"; do
    if [ "$argumento" = attach-session ]; then
        engancha=si
    fi
done
if [ "$engancha" = no ]; then
    exec "${MAGI_TMUX_REAL:?}" "$@"
fi
orden="'${MAGI_TMUX_REAL:?}'"
for argumento in "$@"; do
    citado=$(printf '%s' "$argumento" | sed "s/'/'\\\\''/g")
    orden="$orden '$citado'"
done
exec script -qefc "stty rows 24 cols 80; exec $orden" /dev/null
"#;

/// Salida de una orden de tmux lanzada por la prueba.
struct Salida {
    stdout: String,
    stderr: String,
    codigo: i32,
}

/// Un entorno aislado con tmux de verdad.
struct Real {
    /// Base de todo: `TMUX_TMPDIR`, el `HOME`, el script y el envoltorio.
    _base: tempfile::TempDir,
    home: PathBuf,
    tmpdir: PathBuf,
    /// Directorio con el envoltorio `tmux` que ven los `exec` del host.
    binarios: PathBuf,
    /// El tmux de verdad.
    tmux_real: PathBuf,
    /// `PATH` de las órdenes que lanza la propia prueba.
    path: String,
    /// El enganche mínimo `terminal_enganche.sh` en disco (para el «otro
    /// equipo»).
    enganche: PathBuf,
    version: VersionTmux,
}

fn buscar_en_path(nombre: &str, path: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|directorio| directorio.join(nombre))
        .find(|ruta| ruta.is_file())
}

impl Real {
    /// `None` (con un aviso en la salida de error) si falta lo necesario.
    fn nuevo(prueba: &str) -> Option<Self> {
        let omitir = |motivo: &str| {
            // En CI, `ci.yml` instala tmux (y util-linux ya viene en el
            // runner): que falte es un fallo del trabajo, no un motivo para
            // que estas pruebas pasen vacías.
            assert!(
                std::env::var_os("CI").is_none(),
                "sesiones_persistentes_e2e::{prueba}: {motivo}; en CI hacen falta tmux 3.0 o \
                 posterior y el `script` de util-linux"
            );
            eprintln!("sesiones_persistentes_e2e::{prueba}: se omite, {motivo}");
            None
        };
        let directorio = std::env::var("MAGI_TMUX_DIR").ok();
        let path = match &directorio {
            Some(directorio) => format!("{directorio}:/usr/bin:/bin:/usr/local/bin"),
            None => std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
        };
        let Some(tmux_real) = buscar_en_path("tmux", &path) else {
            return omitir("no hay tmux");
        };
        let version = Command::new(&tmux_real)
            .arg("-V")
            .env_clear()
            .env("PATH", &path)
            .output()
            .ok()
            .filter(|salida| salida.status.success())
            .and_then(|salida| version_de_texto(&String::from_utf8_lossy(&salida.stdout)));
        let Some(version) = version else {
            return omitir("no se pudo leer la versión de tmux");
        };
        if version < VersionTmux::MINIMA {
            return omitir("tmux es anterior a 3.0");
        }
        // `script` de util-linux (la sintaxis de BSD es otra).
        let util_linux = Command::new("script")
            .arg("--version")
            .output()
            .ok()
            .is_some_and(|salida| String::from_utf8_lossy(&salida.stdout).contains("util-linux"));
        if !util_linux {
            return omitir("falta `script` de util-linux para darle un terminal a tmux");
        }
        // Un directorio corto: el socket de tmux va en TMUX_TMPDIR y no puede
        // pasar de ~108 bytes.
        let base = tempfile::Builder::new()
            .prefix("mpe")
            .tempdir_in("/tmp")
            .or_else(|_| tempfile::Builder::new().prefix("mpe").tempdir())
            .ok()?;
        let home = base.path().join("h");
        let tmpdir = base.path().join("t");
        let binarios = base.path().join("bin");
        for directorio in [&home, &tmpdir, &binarios] {
            std::fs::create_dir_all(directorio).ok()?;
        }
        let envoltorio = binarios.join("tmux");
        std::fs::write(&envoltorio, ENVOLTORIO_TMUX).ok()?;
        std::fs::set_permissions(&envoltorio, std::fs::Permissions::from_mode(0o755)).ok()?;
        let enganche = base.path().join("terminal_enganche.sh");
        std::fs::write(&enganche, SCRIPT_ENGANCHE).ok()?;
        eprintln!(
            "sesiones_persistentes_e2e::{prueba}: con {} (versión {}.{})",
            tmux_real.display(),
            version.mayor,
            version.menor
        );
        Some(Self {
            _base: base,
            home,
            tmpdir,
            binarios,
            tmux_real,
            path,
            enganche,
            version,
        })
    }

    /// El entorno de lo que lanza la propia prueba: limpio, sin `TMUX` y con
    /// un `HOME` y un `TMUX_TMPDIR` propios.
    fn entorno<'a>(&self, comando: &'a mut Command) -> &'a mut Command {
        comando
            .env_clear()
            .env("PATH", &self.path)
            .env("HOME", &self.home)
            .env("TMUX_TMPDIR", &self.tmpdir)
            .env("SHELL", "/bin/sh")
            .env("TERM", "xterm-256color")
            .env("LANG", "C.UTF-8")
    }

    /// Variables que ven los procesos del host de pruebas (los `exec`). Sin
    /// locale, como un canal `exec` de ssh, que es el caso difícil del script.
    fn entorno_del_host(&self) -> Vec<(String, String)> {
        vec![
            ("HOME".into(), self.home.display().to_string()),
            ("TMUX_TMPDIR".into(), self.tmpdir.display().to_string()),
            ("SHELL".into(), "/bin/sh".into()),
            ("TERM".into(), "xterm-256color".into()),
            ("LANG".into(), "C".into()),
            ("LC_ALL".into(), "C".into()),
            (
                "MAGI_TMUX_REAL".into(),
                self.tmux_real.display().to_string(),
            ),
        ]
    }

    /// Ejecuta el tmux real contra un servidor (`Some("magi")` con `-L`).
    fn tmux(&self, socket: Option<&str>, argumentos: &[&str]) -> Salida {
        let mut comando = Command::new(&self.tmux_real);
        if let Some(socket) = socket {
            comando.args(["-L", socket]);
        }
        comando.args(argumentos);
        self.entorno(&mut comando);
        let salida = comando.output().expect("ejecutar tmux");
        Salida {
            stdout: String::from_utf8_lossy(&salida.stdout).to_string(),
            stderr: String::from_utf8_lossy(&salida.stderr).to_string(),
            codigo: salida.status.code().unwrap_or(-1),
        }
    }

    /// Las sesiones del tmux de MAGI: nombre, clientes y equipo.
    fn sesiones(&self) -> Vec<(String, u32, String)> {
        let salida = self.tmux(
            Some("magi"),
            &[
                "list-sessions",
                "-F",
                "#{session_name} #{session_attached} #{@magi_equipo}",
            ],
        );
        if salida.codigo != 0 {
            return Vec::new();
        }
        salida
            .stdout
            .lines()
            .filter_map(|linea| {
                let mut campos = linea.splitn(3, ' ');
                let nombre = campos.next()?.to_string();
                let clientes = campos.next()?.parse().ok()?;
                let equipo = campos.next().unwrap_or("").to_string();
                Some((nombre, clientes, equipo))
            })
            .collect()
    }

    fn sesion(&self, nombre: &str) -> Option<(u32, String)> {
        self.sesiones()
            .into_iter()
            .find(|(sesion, _, _)| sesion == nombre)
            .map(|(_, clientes, equipo)| (clientes, equipo))
    }

    /// Lo que se ve en el panel de la sesión y su historial reciente.
    fn panel(&self, nombre: &str) -> String {
        let destino = format!("={nombre}:");
        self.tmux(
            Some("magi"),
            &["capture-pane", "-p", "-J", "-S", "-300", "-t", &destino],
        )
        .stdout
    }

    /// Escribe en la sesión como si se tecleara (sin pasar por MAGI).
    fn teclear(&self, nombre: &str, texto: &str) {
        let destino = format!("={nombre}:");
        let salida = self.tmux(Some("magi"), &["send-keys", "-t", &destino, texto, "Enter"]);
        assert_eq!(salida.codigo, 0, "send-keys: {}", salida.stderr);
    }

    /// Espera (con plazo) a que la condición se cumpla.
    async fn esperar(&self, motivo: &str, condicion: impl Fn(&Self) -> bool) {
        let limite = Instant::now() + PLAZO;
        while Instant::now() < limite {
            if condicion(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "se agotó el plazo esperando: {motivo}; sesiones {:?}",
            self.sesiones()
        );
    }

    /// Espera a que la sesión tenga `n` clientes enganchados.
    async fn esperar_clientes(&self, nombre: &str, n: u32) {
        self.esperar(&format!("{n} clientes en {nombre}"), |real| {
            real.sesion(nombre)
                .is_some_and(|(clientes, _)| clientes == n)
        })
        .await;
    }

    /// Espera a que el panel de la sesión lleve `texto`; devuelve el panel.
    async fn esperar_en_panel(&self, nombre: &str, texto: &str) -> String {
        let limite = Instant::now() + PLAZO;
        let mut ultimo = String::new();
        while Instant::now() < limite {
            ultimo = self.panel(nombre);
            if ultimo.contains(texto) {
                return ultimo;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("el panel de {nombre} no llegó a llevar {texto:?}: {ultimo:?}");
    }

    /// Lanza el enganche como lo haría otro equipo (con un terminal de verdad)
    /// contra el mismo servidor de tmux, y deja la entrada abierta: queda
    /// enganchado hasta que se lo quiten.
    fn equipo_remoto(&self, nombre: &str, equipo: &str, soltar: bool) -> EquipoRemoto {
        let orden = format!(
            "stty rows 24 cols 80; exec sh {} magi {nombre} {} {equipo} 0",
            self.enganche.display(),
            u8::from(soltar)
        );
        let mut comando = Command::new("script");
        comando.args(["-qefc", &orden, "/dev/null"]);
        self.entorno(&mut comando);
        let mut hijo = comando
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("lanzar script");
        let entrada = hijo.stdin.take().expect("entrada de script");
        let mut salida_hijo = hijo.stdout.take().expect("salida de script");
        std::thread::spawn(move || {
            // Se vacía la salida para que el cliente de tmux no se bloquee.
            let mut bloque = [0u8; 4096];
            while let Ok(leidos) = salida_hijo.read(&mut bloque) {
                if leidos == 0 {
                    break;
                }
            }
        });
        EquipoRemoto {
            hijo,
            _entrada: entrada,
        }
    }
}

impl Drop for Real {
    fn drop(&mut self) {
        // Nada queda vivo: ni el servidor propio ni el habitual de la prueba.
        for socket in [Some("magi"), None] {
            let mut comando = Command::new(&self.tmux_real);
            if let Some(socket) = socket {
                comando.args(["-L", socket]);
            }
            comando.arg("kill-server");
            self.entorno(&mut comando);
            let _ = comando.output();
        }
    }
}

/// Un cliente de tmux de «otro equipo», enganchado por el script.
struct EquipoRemoto {
    hijo: Child,
    _entrada: ChildStdin,
}

impl EquipoRemoto {
    /// El código con el que salió el script (`None` si sigue enganchado).
    fn salida(&mut self) -> Option<i32> {
        self.hijo.try_wait().ok().flatten().and_then(|e| e.code())
    }

    async fn esperar_codigo(&mut self) -> Option<i32> {
        let limite = Instant::now() + PLAZO;
        while Instant::now() < limite {
            if let Some(codigo) = self.salida() {
                return Some(codigo);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        None
    }
}

impl Drop for EquipoRemoto {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

// ------------------------------------------------------------- el banco

/// Escenario con tmux real: el host de pruebas, el servidor de sesiones y el
/// tmux aislado. La ventana que trae el escenario se cierra al empezar: cada
/// prueba abre las suyas con [`Banco::ventana`].
struct Banco {
    escenario: Escenario,
    real: Real,
    host_id: i64,
}

impl Banco {
    async fn nuevo(prueba: &str) -> Option<Self> {
        let real = Real::nuevo(prueba)?;
        let mut config = magi::config::Config::default();
        // El servidor no se apaga mientras no hay ventanas: es lo que se prueba.
        config.servidor.gracia_apagado_seg = 600;
        config.persistentes.vigilar_ms_pruebas = VIGILAR_MS;
        let mut escenario = escenario(OpcionesEscenario {
            hosts: 1,
            sftp: false,
            binarios: Some(real.binarios.clone()),
            entorno_extra: real.entorno_del_host(),
            config: Some(config),
            ..Default::default()
        })
        .await;
        let _ = escenario.escritura.shutdown().await;
        let host_id = escenario.hosts[0];
        Some(Self {
            escenario,
            real,
            host_id,
        })
    }

    /// Otra ventana de MAGI conectada al servidor.
    async fn ventana(&self) -> Ven {
        Ven::conectar(self.escenario.rutas()).await
    }

    /// REGISTRO nunca lleva lo que se escribe en las sesiones, su salida ni
    /// los comandos de arranque: solo los hechos (tipo, host y motivo).
    fn exigir_registro_sin(&self, prohibidos: &[&str]) {
        for (tipo, detalle) in self.escenario.registro() {
            for texto in prohibidos {
                assert!(
                    !detalle.contains(texto),
                    "REGISTRO ({tipo}) lleva contenido de la sesión ({texto}): {detalle}"
                );
            }
        }
    }

    /// Guarda la ficha de una sesión que ya existe en el host, como hace el
    /// cliente (el servidor nunca escribe en esa tabla).
    fn guardar_ficha(&self, nombre: &str) {
        let almacen = magi::almacen::abrir(&self.escenario.rutas().base_datos()).unwrap();
        magi_nucleo::almacen::sesiones_persistentes::crear(
            almacen.conexion(),
            self.host_id,
            &DatosFicha {
                nombre: nombre.to_string(),
                carpeta: None,
                comando: None,
                comando_reanudar: None,
                nota: None,
                origen: OrigenFicha::Manual,
                creada_por: etiqueta_equipo(&magi::config::nombre_actual()),
            },
        )
        .unwrap();
        almacen.cerrar().unwrap();
    }
}

/// La etiqueta de este equipo, como la pone el servidor en `@magi_equipo`.
fn equipo_propio() -> String {
    etiqueta_equipo(&magi::config::nombre_actual())
}

/// La etiqueta del «otro equipo»: distinta de la de esta máquina.
fn equipo_ajeno() -> String {
    let propio = equipo_propio();
    if propio == "otro-equipo" {
        "otro-equipo-2".to_string()
    } else {
        "otro-equipo".to_string()
    }
}

// ------------------------------------------------------------ una ventana

/// Una ventana de MAGI: lo que ve y lo que manda. Lleva el estado como el
/// cliente real: la última lista de pestañas, todas las listas de sesiones
/// persistentes y la pantalla de cada pestaña a la que se adjunta.
struct Ven {
    escritura: OwnedWriteHalf,
    lector: FramedRead<OwnedReadHalf, LinesCodec>,
    sesiones: Vec<InfoSesion>,
    /// Cada `SesionesPersistentes` recibida, en orden.
    listas: Vec<ListaPersistentes>,
    pantallas: HashMap<u32, PantallaEmulada>,
    /// Todo lo demás (menos los datos de las pantallas), en orden.
    vistos: Vec<MensajeServidor>,
}

impl Ven {
    /// Conecta una ventana nueva y la saluda. La `Bienvenida` lleva la lista de
    /// pestañas que había: una ventana que llega después de otra la ve ahí.
    async fn conectar(rutas: &magi::config::Rutas) -> Self {
        let mut flujo = UnixStream::connect(magi::servidor::ruta_socket(rutas))
            .await
            .expect("conectando una ventana al servidor");
        comun::enviar(
            &mut flujo,
            &MensajeCliente::Hola {
                version: VERSION_PROTOCOLO,
                pid: 44,
            },
        )
        .await;
        let (lectura, escritura) = flujo.into_split();
        let mut ven = Self {
            escritura,
            lector: FramedRead::new(lectura, codec()),
            sesiones: Vec::new(),
            listas: Vec::new(),
            pantallas: HashMap::new(),
            vistos: Vec::new(),
        };
        // El servidor registra a la ventana y le manda la `Bienvenida` en dos
        // secciones críticas distintas (`tarea_conexion` y `bienvenida`): la
        // difusión de una lista completa (`Sesiones`, `Escritorios`,
        // `SesionesPersistentes`...) por otra ventana que se cierra
        // (`limpiar_cliente`) o por el vigilante puede colarse antes. Con el
        // cliente real eso da «respuesta inesperada al saludo» (pendiente del
        // servidor, anotado en la revisión). Aquí se anota la difusión y se
        // sigue esperando, para que la prueba no dependa de ese orden; un
        // rechazo sí la hace fallar.
        loop {
            let mensaje = tokio::time::timeout(
                PLAZO,
                comun::siguiente_framed::<MensajeServidor, _>(&mut ven.lector),
            )
            .await
            .expect("el servidor no mandó la Bienvenida a tiempo");
            match mensaje {
                MensajeServidor::Bienvenida { .. } => {
                    ven.anotar(mensaje);
                    break;
                }
                MensajeServidor::Error { .. } | MensajeServidor::VersionIncompatible { .. } => {
                    panic!("el servidor rechazó el saludo: {mensaje:?}")
                }
                difusion => ven.anotar(difusion),
            }
        }
        ven
    }

    async fn enviar(&mut self, mensaje: &MensajeCliente) {
        comun::enviar_a(&mut self.escritura, mensaje).await;
    }

    /// Lee un mensaje (con plazo) y lo anota. `false` si no llegó nada.
    async fn paso(&mut self, plazo: Duration) -> bool {
        match tokio::time::timeout(plazo, self.lector.next()).await {
            Err(_) => false,
            Ok(None) => panic!("el servidor cerró la conexión de la ventana"),
            Ok(Some(Err(error))) => panic!("error leyendo del servidor: {error}"),
            Ok(Some(Ok(linea))) => {
                let mensaje = decodificar::<MensajeServidor>(linea.trim_end())
                    .expect("mensaje del servidor decodificable");
                self.anotar(mensaje);
                true
            }
        }
    }

    fn anotar(&mut self, mensaje: MensajeServidor) {
        match mensaje {
            MensajeServidor::Bienvenida { sesiones, .. } => self.sesiones = sesiones,
            MensajeServidor::Sesiones { lista } => self.sesiones = lista,
            MensajeServidor::SesionesPersistentes { lista } => self.listas.push(lista),
            MensajeServidor::PantallaCompleta {
                sesion_id,
                bytes,
                cols,
                filas,
            } => {
                // Como el cliente real, que reutiliza el emulador de la pestaña.
                let pantalla = self
                    .pantallas
                    .entry(sesion_id)
                    .or_insert_with(|| PantallaEmulada::nueva(filas, cols, 0, false));
                pantalla.redimensionar(filas, cols, 0);
                pantalla.procesar(&bytes);
            }
            MensajeServidor::Datos { sesion_id, bytes } => {
                if let Some(pantalla) = self.pantallas.get_mut(&sesion_id) {
                    pantalla.procesar(&bytes);
                }
            }
            otro => self.vistos.push(otro),
        }
    }

    /// Lee hasta que se cumpla la condición o se acabe el plazo.
    async fn hasta(&mut self, plazo: Duration, mut condicion: impl FnMut(&Self) -> bool) -> bool {
        let fin = Instant::now() + plazo;
        loop {
            if condicion(self) {
                return true;
            }
            let restante = fin.saturating_duration_since(Instant::now());
            if restante.is_zero() {
                return false;
            }
            self.paso(restante.min(Duration::from_millis(100))).await;
        }
    }

    /// Como `hasta`, pero falla la prueba con `motivo` si no se cumple.
    async fn exigir(&mut self, motivo: &str, condicion: impl FnMut(&Self) -> bool) {
        if !self.hasta(PLAZO, condicion).await {
            panic!(
                "se agotó el plazo esperando: {motivo}\n  pestañas: {:?}\n  última lista: {:?}\n  vistos: {:?}",
                self.sesiones
                    .iter()
                    .map(|s| (s.id, s.estado, s.tmux.clone()))
                    .collect::<Vec<_>>(),
                self.listas.last(),
                self.vistos
            );
        }
    }

    /// Pide una pestaña con tmux y espera a que abra; si el servidor dice por
    /// qué no puede, la prueba falla con su mensaje.
    async fn abrir(&mut self, host_id: i64, tmux: PeticionTmux) -> InfoSesion {
        let nombre = tmux.nombre.clone();
        let conocidas: HashSet<u32> = self.sesiones.iter().map(|sesion| sesion.id).collect();
        let antes = self.vistos.len();
        self.enviar(&MensajeCliente::AbrirSesion {
            host_id,
            cols: 80,
            filas: 24,
            comandos_iniciales: Vec::new(),
            orden: None,
            tmux: Some(tmux),
        })
        .await;
        let fin = Instant::now() + PLAZO;
        loop {
            if let Some(sesion) = self.sesiones.iter().find(|sesion| {
                sesion.estado == EstadoSesionRemota::Abierta
                    && !conocidas.contains(&sesion.id)
                    && sesion
                        .tmux
                        .as_ref()
                        .is_some_and(|tmux| tmux.nombre == nombre)
            }) {
                return sesion.clone();
            }
            for mensaje in &self.vistos[antes..] {
                if let MensajeServidor::AperturaTmuxFallida { causa, mensaje, .. } = mensaje {
                    panic!("no se abrió la pestaña de {nombre}: {causa:?} · {mensaje}");
                }
            }
            assert!(
                Instant::now() < fin,
                "la pestaña de {nombre} no llegó a abrirse; vistos {:?}",
                &self.vistos[antes..]
            );
            self.paso(Duration::from_millis(100)).await;
        }
    }

    /// Se adjunta a una pestaña y espera su volcado.
    async fn adjuntar(&mut self, sesion_id: u32) {
        self.pantallas.remove(&sesion_id);
        self.enviar(&MensajeCliente::Adjuntar {
            sesion_id,
            cols: 80,
            filas: 24,
            colores: None,
        })
        .await;
        self.exigir("el volcado de la pestaña", |ven| {
            ven.pantallas.contains_key(&sesion_id)
        })
        .await;
    }

    async fn teclear(&mut self, sesion_id: u32, texto: &str) {
        self.enviar(&MensajeCliente::Teclas {
            sesion_id,
            bytes: texto.as_bytes().to_vec(),
        })
        .await;
    }

    /// El texto de la pantalla de la pestaña, como la ve esta ventana.
    fn pantalla(&self, sesion_id: u32) -> String {
        self.pantallas
            .get(&sesion_id)
            .map(|pantalla| pantalla.screen().contents())
            .unwrap_or_default()
    }

    /// Espera a que la pantalla de la pestaña lleve `texto`.
    async fn exigir_pantalla(&mut self, sesion_id: u32, texto: &str) {
        let fin = Instant::now() + PLAZO;
        while !self.pantalla(sesion_id).contains(texto) {
            assert!(
                Instant::now() < fin,
                "la pantalla de la pestaña {sesion_id} no llegó a llevar {texto:?}: {:?}",
                self.pantalla(sesion_id)
            );
            self.paso(Duration::from_millis(100)).await;
        }
    }

    fn pestana(&self, sesion_id: u32) -> Option<&InfoSesion> {
        self.sesiones.iter().find(|sesion| sesion.id == sesion_id)
    }

    /// La ficha vista de una sesión, en la última lista.
    fn ficha(&self, nombre: &str) -> Option<&FichaVista> {
        self.listas
            .last()?
            .fichas
            .iter()
            .find(|ficha| ficha.nombre == nombre)
    }

    /// Una consulta de las que manda la lista de sesiones al abrirse.
    async fn consultar(&mut self, numero: u64) {
        self.enviar(&MensajeCliente::ConsultarSesionesPersistentes {
            peticion_id: RANGO_TERMINAL + numero,
            host_id: None,
        })
        .await;
    }

    /// Espera a tener la lista de una comprobación hecha del todo DESPUÉS de
    /// esta llamada. Una sola consulta no basta: la lista que llega primero
    /// puede ser la de una comprobación que ya estaba en vuelo (anterior a lo
    /// que acaba de ocurrir en el host). La segunda consulta se manda cuando ha
    /// llegado la lista de la primera, así que su comprobación empieza después.
    async fn comprobacion_fresca(&mut self, numero: u64) {
        for consulta in [numero, numero + 1] {
            let antes = self.listas.len();
            self.consultar(consulta).await;
            self.exigir("la lista de una consulta", |ven| ven.listas.len() > antes)
                .await;
        }
    }

    /// Cierra una pestaña (la sesión de tmux sigue viva) y espera a que se
    /// vaya de la lista.
    async fn cerrar(&mut self, sesion_id: u32) {
        self.enviar(&MensajeCliente::Cerrar { sesion_id }).await;
        self.exigir("la pestaña cerrada se va de la lista", |ven| {
            ven.pestana(sesion_id).is_none()
        })
        .await;
    }
}

fn peticion(nombre: &str, crear: bool, compartir: bool) -> PeticionTmux {
    PeticionTmux {
        servidor: ServidorTmux::Magi,
        nombre: nombre.to_string(),
        crear: crear.then_some(CrearTmux {
            carpeta: None,
            comando: None,
        }),
        compartir,
    }
}

fn soltada_por(pestana: &InfoSesion) -> Option<Option<String>> {
    pestana
        .tmux
        .as_ref()
        .and_then(|tmux| tmux.soltada.as_ref())
        .map(|soltada| soltada.por_equipo.clone())
}

// ----------------------------------------------------------------- pruebas

/// (a) y (b): crear una sesión desde una pestaña y escribir en ella; cerrar la
/// ventana (el servidor y la sesión siguen) y volver desde otra, con la
/// pestaña, la sesión viva y su contenido.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crear_cerrar_la_ventana_y_volver_desde_otra() {
    let Some(banco) = Banco::nuevo("crear_cerrar_la_ventana_y_volver_desde_otra").await else {
        return;
    };
    let host = banco.host_id;
    let mut ventana1 = banco.ventana().await;

    // (a) Crear y enganchar: la pestaña es persistente y la sesión existe.
    let pestana = ventana1
        .abrir(host, peticion("magi-a-1", true, false))
        .await;
    let tmux = pestana.tmux.clone().expect("la pestaña lleva su tmux");
    assert_eq!(tmux.servidor, ServidorTmux::Magi);
    assert_eq!(tmux.nombre, "magi-a-1");
    assert!(tmux.soltada.is_none());
    assert_eq!(
        tmux.version,
        Some(banco.real.version),
        "la pestaña lleva la versión de tmux del host"
    );
    banco.real.esperar_clientes("magi-a-1", 1).await;
    // La ficha la guarda el cliente cuando la sesión ya existe.
    banco.guardar_ficha("magi-a-1");

    // Escribir en la sesión: la shell responde y su salida llega a la ventana.
    banco
        .real
        .esperar("la shell de la sesión pinta algo", |real| {
            !real.panel("magi-a-1").trim().is_empty()
        })
        .await;
    ventana1.adjuntar(pestana.id).await;
    ventana1.teclear(pestana.id, "echo e2e-$((20+22))\r").await;
    ventana1.exigir_pantalla(pestana.id, "e2e-42").await;
    banco.real.esperar_en_panel("magi-a-1", "e2e-42").await;

    // (b) Se cierra la ventana: el servidor sigue y la sesión de tmux también,
    // con el cliente de la pestaña enganchado.
    drop(ventana1);
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            banco.real.sesion("magi-a-1").map(|(clientes, _)| clientes),
            Some(1),
            "la sesión sigue viva y enganchada a la pestaña sin ventanas"
        );
    }

    // Otra ventana: ve la pestaña (la `Bienvenida` la lleva) y, con la lista
    // que había al irse la última ventana, la ficha viva y enganchada.
    let mut ventana2 = banco.ventana().await;
    assert!(
        ventana2
            .pestana(pestana.id)
            .is_some_and(|p| p.estado == EstadoSesionRemota::Abierta),
        "la pestaña persistente sigue abierta para una ventana nueva: {:?}",
        ventana2.sesiones
    );
    let vista = ventana2.pestana(pestana.id).unwrap();
    assert_eq!(
        vista
            .tmux
            .as_ref()
            .map(|tmux| (tmux.servidor, &tmux.nombre)),
        Some((ServidorTmux::Magi, &"magi-a-1".to_string()))
    );
    let equipo = equipo_propio();
    let ficha_viva = |ven: &Ven| {
        ven.ficha("magi-a-1").is_some_and(|ficha| {
            ficha.estado == EstadoFicha::VivaEnganchada
                && ficha.sesion.as_ref().is_some_and(|sesion| {
                    sesion.clientes == 1 && sesion.equipo.as_deref() == Some(equipo.as_str())
                })
        })
    };
    ventana2
        .exigir("la ficha viva y enganchada", &ficha_viva)
        .await;
    // La lista de una ventana nueva es la que había: lo que cuenta es la
    // difusión que llega DESPUÉS de pedir una comprobación. Se pide y se
    // espera una lista más (la respuesta, o un cambio posterior).
    let antes = ventana2.listas.len();
    ventana2.consultar(1).await;
    ventana2
        .exigir("una difusión nueva con la ficha viva", |ven| {
            ven.listas.len() > antes && ficha_viva(ven)
        })
        .await;

    // La sesión conserva su contenido y sigue aceptando teclas desde la otra.
    ventana2.adjuntar(pestana.id).await;
    ventana2.exigir_pantalla(pestana.id, "e2e-42").await;
    ventana2.teclear(pestana.id, "echo e2e-$((20+23))\r").await;
    ventana2.exigir_pantalla(pestana.id, "e2e-43").await;

    // Cerrar la pestaña no termina la sesión: sigue viva, sin clientes.
    ventana2.cerrar(pestana.id).await;
    banco.real.esperar_clientes("magi-a-1", 0).await;
    assert!(banco.real.panel("magi-a-1").contains("e2e-43"));

    // Nada de lo escrito queda en REGISTRO.
    banco.exigir_registro_sin(&["e2e-4", "echo "]);
}

/// (c): un segundo equipo se engancha con `attach -d` y la pestaña pasa a
/// soltada con su pantalla; volver (`Reconectar`) se la quita al otro y
/// compartir no suelta a nadie.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn otro_equipo_suelta_la_pestana_y_volver_la_recupera() {
    let Some(banco) = Banco::nuevo("otro_equipo_suelta_la_pestana_y_volver_la_recupera").await
    else {
        return;
    };
    let host = banco.host_id;
    let ajeno = equipo_ajeno();
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("magi-a-1", true, false)).await;
    banco.real.esperar_clientes("magi-a-1", 1).await;
    banco.guardar_ficha("magi-a-1");
    ventana.adjuntar(pestana.id).await;
    ventana
        .teclear(pestana.id, "echo antes-de-soltar-$((1+1))\r")
        .await;
    ventana
        .exigir_pantalla(pestana.id, "antes-de-soltar-2")
        .await;

    // El otro equipo se engancha soltando a los demás.
    let mut otro = banco.real.equipo_remoto("magi-a-1", &ajeno, true);
    ventana
        .exigir("la pestaña pasa a soltada por el otro equipo", |ven| {
            ven.pestana(pestana.id)
                .and_then(soltada_por)
                .is_some_and(|por| por.as_deref() == Some(&*ajeno))
        })
        .await;
    let soltada = ventana.pestana(pestana.id).unwrap();
    assert_eq!(soltada.estado, EstadoSesionRemota::Caida);
    assert!(
        soltada
            .motivo
            .as_deref()
            .is_some_and(|motivo| motivo.contains(&ajeno)),
        "el motivo dice quién la tiene: {:?}",
        soltada.motivo
    );
    // En el host solo queda el cliente del otro equipo, y la sesión es suya.
    banco
        .real
        .esperar("el cliente del otro equipo es el único", |real| {
            real.sesion("magi-a-1") == Some((1, ajeno.clone()))
        })
        .await;
    // La ficha la ve enganchada por él (con una comprobación posterior).
    ventana
        .exigir("la ficha enganchada por el otro equipo", |ven| {
            ven.ficha("magi-a-1").is_some_and(|ficha| {
                ficha.estado == EstadoFicha::VivaEnganchada
                    && ficha
                        .sesion
                        .as_ref()
                        .is_some_and(|sesion| sesion.equipo.as_deref() == Some(&*ajeno))
            })
        })
        .await;
    // La pestaña soltada se puede adjuntar (la pantalla en sí, en la prueba
    // `la_pestana_soltada_conserva_la_ultima_pantalla_de_tmux`).
    ventana.adjuntar(pestana.id).await;

    // Volver: la pestaña se engancha otra vez soltando al otro equipo.
    ventana
        .enviar(&MensajeCliente::Reconectar {
            sesion_id: pestana.id,
            compartir: false,
        })
        .await;
    ventana
        .exigir("la pestaña vuelve a estar abierta y sin marca", |ven| {
            ven.pestana(pestana.id).is_some_and(|p| {
                p.estado == EstadoSesionRemota::Abierta && soltada_por(p).is_none()
            })
        })
        .await;
    assert_eq!(
        otro.esperar_codigo().await,
        Some(100),
        "al otro equipo le quitaron la sesión sin terminarla (sale con 100)"
    );
    banco
        .real
        .esperar("la sesión vuelve a ser de este equipo", |real| {
            real.sesion("magi-a-1") == Some((1, equipo_propio()))
        })
        .await;
    ventana
        .exigir_pantalla(pestana.id, "antes-de-soltar-2")
        .await;
    ventana.teclear(pestana.id, "echo despues-$((2+3))\r").await;
    ventana.exigir_pantalla(pestana.id, "despues-5").await;

    // Compartir: el otro equipo la vuelve a llevar y esta vez se vuelve sin
    // soltarlo: los dos quedan enganchados.
    let mut otro = banco.real.equipo_remoto("magi-a-1", &ajeno, true);
    ventana
        .exigir("la pestaña se suelta otra vez", |ven| {
            ven.pestana(pestana.id)
                .and_then(soltada_por)
                .is_some_and(|por| por.as_deref() == Some(&*ajeno))
        })
        .await;
    ventana
        .enviar(&MensajeCliente::Reconectar {
            sesion_id: pestana.id,
            compartir: true,
        })
        .await;
    ventana
        .exigir("la pestaña vuelve compartiendo", |ven| {
            ven.pestana(pestana.id).is_some_and(|p| {
                p.estado == EstadoSesionRemota::Abierta && soltada_por(p).is_none()
            })
        })
        .await;
    banco.real.esperar_clientes("magi-a-1", 2).await;
    assert_eq!(
        otro.salida(),
        None,
        "compartir no suelta al otro equipo: su cliente sigue enganchado"
    );
    ventana
        .exigir("la ficha ve dos clientes", |ven| {
            ven.ficha("magi-a-1")
                .and_then(|ficha| ficha.sesion.as_ref())
                .is_some_and(|sesion| sesion.clientes == 2)
        })
        .await;
    banco.exigir_registro_sin(&["antes-de-soltar", "despues-", "echo "]);
}

/// Reconexión automática con tmux real (checklist de la reconexión de la F15):
/// la conexión cae (un corte de red, sin que el host se entere) y la pestaña,
/// con la sesión libre o con el cliente viejo todavía enganchado en el host,
/// vuelve sola sin diálogos, a la misma sesión y con su contenido.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_reconexion_automatica_vuelve_a_la_sesion_que_nadie_mas_usa() {
    let Some(banco) =
        Banco::nuevo("la_reconexion_automatica_vuelve_a_la_sesion_que_nadie_mas_usa").await
    else {
        return;
    };
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("magi-a-1", true, false)).await;
    banco.real.esperar_clientes("magi-a-1", 1).await;
    ventana.adjuntar(pestana.id).await;
    ventana
        .teclear(pestana.id, "echo antes-de-caer-$((4+4))\r")
        .await;
    ventana.exigir_pantalla(pestana.id, "antes-de-caer-8").await;

    // Se corta la red: la pestaña pasa a reconectar (el primer intento, a los 2 s)
    // y vuelve sola a Abierta, sin marca de soltada.
    banco.escenario.observado.tirar_conexiones().await;
    ventana
        .exigir("la pestaña cae y reconecta", |ven| {
            ven.pestana(pestana.id)
                .is_some_and(|p| p.estado != EstadoSesionRemota::Abierta)
        })
        .await;
    ventana
        .exigir("la pestaña vuelve sola", |ven| {
            ven.pestana(pestana.id).is_some_and(|p| {
                p.estado == EstadoSesionRemota::Abierta
                    && p.reconexion.is_none()
                    && soltada_por(p).is_none()
            })
        })
        .await;
    // La sesión es la misma, de este equipo y con un solo cliente (el viejo,
    // si seguía enganchado, lo soltó el `attach -d` de la reconexión).
    banco
        .real
        .esperar(
            "la sesión vuelve a tener un solo cliente, el nuestro",
            |real| real.sesion("magi-a-1") == Some((1, equipo_propio())),
        )
        .await;
    assert_eq!(banco.real.sesiones().len(), 1, "no se creó otra sesión");
    ventana.adjuntar(pestana.id).await;
    ventana.exigir_pantalla(pestana.id, "antes-de-caer-8").await;
    ventana
        .teclear(pestana.id, "echo despues-de-volver-$((4+5))\r")
        .await;
    ventana
        .exigir_pantalla(pestana.id, "despues-de-volver-9")
        .await;
    // Una reconexión, una anotación; y nada de lo escrito.
    assert_eq!(banco.escenario.anotaciones("sesion_reconectada"), 1);
    banco.exigir_registro_sin(&["antes-de-caer", "despues-de-volver", "echo "]);
}

/// Reconexión automática con tmux real: mientras la conexión de la pestaña
/// está caída, otro equipo se lleva la sesión. La reconexión automática NO se
/// la quita (pérdida de trabajo del otro equipo): la pestaña queda soltada
/// con su nombre y el otro sigue enganchado.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_reconexion_automatica_no_le_quita_la_sesion_a_otro_equipo() {
    let Some(banco) =
        Banco::nuevo("la_reconexion_automatica_no_le_quita_la_sesion_a_otro_equipo").await
    else {
        return;
    };
    let host = banco.host_id;
    let ajeno = equipo_ajeno();
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("magi-a-1", true, false)).await;
    banco.real.esperar_clientes("magi-a-1", 1).await;

    banco.escenario.observado.tirar_conexiones().await;
    ventana
        .exigir("la pestaña cae y espera para reconectar", |ven| {
            ven.pestana(pestana.id)
                .is_some_and(|p| p.estado != EstadoSesionRemota::Abierta)
        })
        .await;
    // Antes del primer intento (a los 2 s), el otro equipo se la lleva.
    let mut otro = banco.real.equipo_remoto("magi-a-1", &ajeno, true);
    banco
        .real
        .esperar("el otro equipo tiene la sesión", |real| {
            real.sesion("magi-a-1") == Some((1, ajeno.clone()))
        })
        .await;
    ventana
        .exigir("la pestaña queda soltada por el otro equipo", |ven| {
            ven.pestana(pestana.id)
                .and_then(soltada_por)
                .is_some_and(|por| por.as_deref() == Some(&*ajeno))
        })
        .await;
    // No se la quitó: sigue siendo suya, con su único cliente.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        otro.salida(),
        None,
        "la reconexión automática no suelta al otro equipo"
    );
    assert_eq!(
        banco.real.sesion("magi-a-1"),
        Some((1, ajeno.clone())),
        "la sesión sigue siendo del otro equipo"
    );
    let soltada = ventana.pestana(pestana.id).unwrap();
    assert_eq!(soltada.estado, EstadoSesionRemota::Caida);
    assert!(soltada.reconexion.is_none(), "no sigue reintentando");
}

/// La pestaña soltada conserva la última pantalla (informe §6.4 y notas de UX:
/// «conserva la última pantalla»).
///
/// Con tmux de verdad, el cliente de tmux de la pestaña, al soltarlo otro
/// equipo, sale de la pantalla alternativa y escribe `[detached (from session
/// …)]` en la normal, y el emulador del servidor ya lo ha procesado cuando el
/// script sale con 100. El servidor lo sustituye por lo que muestra la sesión
/// en el host (`vista`) antes de dar la pestaña por soltada.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_pestana_soltada_conserva_la_ultima_pantalla_de_tmux() {
    let Some(banco) = Banco::nuevo("la_pestana_soltada_conserva_la_ultima_pantalla_de_tmux").await
    else {
        return;
    };
    let host = banco.host_id;
    let ajeno = equipo_ajeno();
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("magi-a-1", true, false)).await;
    banco.real.esperar_clientes("magi-a-1", 1).await;
    ventana.adjuntar(pestana.id).await;
    ventana.teclear(pestana.id, "echo visible-$((3+4))\r").await;
    ventana.exigir_pantalla(pestana.id, "visible-7").await;

    let _otro = banco.real.equipo_remoto("magi-a-1", &ajeno, true);
    ventana
        .exigir("la pestaña pasa a soltada", |ven| {
            ven.pestana(pestana.id).and_then(soltada_por).is_some()
        })
        .await;
    // La ventana sigue adjunta: ve el aviso de tmux y, justo después, la
    // última pantalla de la sesión en lugar de él.
    ventana
        .exigir(
            "la pestaña soltada vuelve a enseñar la última pantalla",
            |ven| {
                let pantalla = ven.pantalla(pestana.id);
                pantalla.contains("visible-7") && !pantalla.contains("[detached")
            },
        )
        .await;
    // Una ventana que se adjunta después ve lo mismo: es lo que guarda el
    // servidor.
    ventana.adjuntar(pestana.id).await;
    let pantalla = ventana.pantalla(pestana.id);
    assert!(
        pantalla.contains("visible-7") && !pantalla.contains("[detached"),
        "la pestaña soltada debe conservar la última pantalla de la sesión: {pantalla:?}"
    );
    // Y la sesión sigue siendo del otro equipo: solo se miró.
    assert_eq!(
        banco.real.sesion("magi-a-1"),
        Some((1, ajeno.clone())),
        "mirar la última pantalla no toca la sesión"
    );
    banco.exigir_registro_sin(&["visible-"]);
}

/// (d): la campana de tmux es la atención. Con nadie mirando, un `\a` marca la
/// sesión y el vigilante lo difunde; al engancharse, la campana se limpia.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_campana_marca_la_atencion_hasta_que_alguien_se_engancha() {
    let Some(banco) =
        Banco::nuevo("la_campana_marca_la_atencion_hasta_que_alguien_se_engancha").await
    else {
        return;
    };
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("magi-a-1", true, false)).await;
    banco.real.esperar_clientes("magi-a-1", 1).await;
    banco.guardar_ficha("magi-a-1");
    ventana
        .exigir("la ficha viva y sin atención", |ven| {
            ven.ficha("magi-a-1").is_some_and(|ficha| {
                ficha.estado == EstadoFicha::VivaEnganchada
                    && ficha.sesion.as_ref().is_some_and(|sesion| !sesion.campana)
            })
        })
        .await;
    // Con la pestaña enganchada y mirándola, la campana no pide atención.
    ventana.adjuntar(pestana.id).await;
    // La campana suena de verdad (el eco con la suma solo sale tras ejecutar
    // el `printf`) y, después, una comprobación entera del vigilante no la
    // cuenta como atención.
    ventana
        .teclear(pestana.id, "printf '\\a'; echo campana-sonada-$((2+3))\r")
        .await;
    ventana
        .exigir_pantalla(pestana.id, "campana-sonada-5")
        .await;
    ventana.comprobacion_fresca(10).await;
    assert!(
        ventana
            .ficha("magi-a-1")
            .and_then(|ficha| ficha.sesion.as_ref())
            .is_some_and(|sesion| !sesion.campana),
        "una campana en la ventana que se está viendo no es atención"
    );

    // Se cierra la pestaña: nadie mira. El programa de la sesión (aquí, una
    // orden tecleada en el host) toca la campana y el vigilante lo ve.
    ventana.cerrar(pestana.id).await;
    banco.real.esperar_clientes("magi-a-1", 0).await;
    banco.real.teclear("magi-a-1", "printf '\\a'");
    ventana
        .exigir("la campana llega como atención en la lista", |ven| {
            ven.ficha("magi-a-1").is_some_and(|ficha| {
                ficha.estado == EstadoFicha::VivaLibre
                    && ficha.sesion.as_ref().is_some_and(|sesion| sesion.campana)
            })
        })
        .await;

    // Engancharse la mira: la atención se quita.
    let pestana = ventana
        .abrir(host, peticion("magi-a-1", false, false))
        .await;
    ventana
        .exigir("la atención se quita al engancharse", |ven| {
            ven.ficha("magi-a-1").is_some_and(|ficha| {
                ficha.estado == EstadoFicha::VivaEnganchada
                    && ficha.sesion.as_ref().is_some_and(|sesion| !sesion.campana)
            })
        })
        .await;
    ventana.cerrar(pestana.id).await;
    banco.exigir_registro_sin(&["campana-sonada", "printf"]);
}

/// (e): terminar con el nombre exacto. Con `magi-a-1` junto a `magi-a-10`, solo
/// cae la pedida; la pestaña enganchada se va; la ficha queda `terminada` y
/// la otra sigue viva.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminar_usa_el_nombre_exacto_y_la_ficha_queda_terminada() {
    let Some(banco) =
        Banco::nuevo("terminar_usa_el_nombre_exacto_y_la_ficha_queda_terminada").await
    else {
        return;
    };
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    // `magi-a-10` primero y sin pestaña; `magi-a-1` con su pestaña enganchada.
    let larga = ventana
        .abrir(host, peticion("magi-a-10", true, false))
        .await;
    banco.real.esperar_clientes("magi-a-10", 1).await;
    banco.guardar_ficha("magi-a-10");
    ventana.cerrar(larga.id).await;
    let corta = ventana.abrir(host, peticion("magi-a-1", true, false)).await;
    banco.real.esperar_clientes("magi-a-1", 1).await;
    banco.guardar_ficha("magi-a-1");
    ventana
        .exigir("las dos fichas vivas", |ven| {
            ["magi-a-1", "magi-a-10"].iter().all(|nombre| {
                ven.ficha(nombre).is_some_and(|ficha| {
                    matches!(
                        ficha.estado,
                        EstadoFicha::VivaLibre | EstadoFicha::VivaEnganchada
                    )
                })
            })
        })
        .await;

    // Un prefijo que no es el nombre exacto no termina nada.
    let mut peticion_id = RANGO_TERMINAL + 100;
    let mut terminar = |nombre: &str| {
        peticion_id += 1;
        (
            peticion_id,
            MensajeCliente::TerminarTmux {
                peticion_id,
                host_id: host,
                servidor: ServidorTmux::Magi,
                nombre: nombre.to_string(),
            },
        )
    };
    let (id, mensaje) = terminar("magi-a");
    ventana.enviar(&mensaje).await;
    ventana
        .exigir("la respuesta a terminar un prefijo", |ven| {
            ven.vistos.iter().any(|m| {
                matches!(m, MensajeServidor::TmuxTerminada { peticion_id, .. } if *peticion_id == id)
            })
        })
        .await;
    assert!(ventana.vistos.iter().any(|m| matches!(
        m,
        MensajeServidor::TmuxTerminada { peticion_id, resultado: ResultadoTmux::NoExiste, .. }
            if *peticion_id == id
    )));
    assert_eq!(banco.real.sesiones().len(), 2, "un prefijo no termina nada");

    // `magi-a-1` exacta: cae esa y solo esa.
    let (id, mensaje) = terminar("magi-a-1");
    ventana.enviar(&mensaje).await;
    ventana
        .exigir("la respuesta a terminar magi-a-1", |ven| {
            ven.vistos.iter().any(|m| {
                matches!(m, MensajeServidor::TmuxTerminada { peticion_id, .. } if *peticion_id == id)
            })
        })
        .await;
    assert!(ventana.vistos.iter().any(|m| matches!(
        m,
        MensajeServidor::TmuxTerminada {
            peticion_id,
            servidor: ServidorTmux::Magi,
            resultado: ResultadoTmux::Ok
        } if *peticion_id == id
    )));
    assert!(banco.real.sesion("magi-a-1").is_none(), "magi-a-1 terminó");
    assert!(
        banco.real.sesion("magi-a-10").is_some(),
        "magi-a-10 sigue viva: tmux casa prefijos y el script lo evita"
    );
    // Su pestaña se va (la sesión terminó: el script sale con 0) y la ficha
    // pasa a terminada; la otra sigue viva.
    ventana
        .exigir("la pestaña de la sesión terminada se va", |ven| {
            ven.pestana(corta.id).is_none()
        })
        .await;
    ventana
        .exigir("la ficha terminada y la otra viva", |ven| {
            ven.ficha("magi-a-1")
                .is_some_and(|ficha| ficha.estado == EstadoFicha::Terminada)
                && ven
                    .ficha("magi-a-10")
                    .is_some_and(|ficha| ficha.estado == EstadoFicha::VivaLibre)
        })
        .await;
    // Terminar otra vez la que ya no existe no toca a la larga.
    let (id, mensaje) = terminar("magi-a-1");
    ventana.enviar(&mensaje).await;
    ventana
        .exigir("la segunda respuesta", |ven| {
            ven.vistos.iter().any(|m| {
                matches!(
                    m,
                    MensajeServidor::TmuxTerminada {
                        peticion_id,
                        resultado: ResultadoTmux::NoExiste,
                        ..
                    } if *peticion_id == id
                )
            })
        })
        .await;
    assert!(banco.real.sesion("magi-a-10").is_some());
    // El efecto se anotó una sola vez, y sin el contenido de nada.
    assert_eq!(banco.escenario.anotaciones("tmux_terminada"), 1);
    banco.exigir_registro_sin(&["printf", "echo "]);
}

/// (f): un host que cae deja sus sesiones `inalcanzable`, nunca `terminada`,
/// aunque las sesiones sigan vivas en él.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn un_host_que_cae_deja_las_fichas_inalcanzables_nunca_terminadas() {
    let Some(banco) =
        Banco::nuevo("un_host_que_cae_deja_las_fichas_inalcanzables_nunca_terminadas").await
    else {
        return;
    };
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("magi-a-1", true, false)).await;
    banco.real.esperar_clientes("magi-a-1", 1).await;
    banco.guardar_ficha("magi-a-1");
    ventana.cerrar(pestana.id).await;
    ventana
        .exigir("la ficha viva", |ven| {
            ven.ficha("magi-a-1")
                .is_some_and(|ficha| ficha.estado == EstadoFicha::VivaLibre)
        })
        .await;

    // El host se cae: ya no escucha y las conexiones abiertas se cortan.
    let desde = ventana.listas.len();
    banco.escenario.apagar_servidor_ssh().await;
    ventana
        .exigir("la ficha inalcanzable", |ven| {
            ven.ficha("magi-a-1")
                .is_some_and(|ficha| ficha.estado == EstadoFicha::Inalcanzable)
        })
        .await;
    let lista = ventana.listas.last().unwrap();
    let visto = lista
        .hosts
        .iter()
        .find(|visto| visto.host_id == host)
        .expect("el host figura en la lista");
    assert!(
        visto.error.is_some(),
        "el host sin respuesta lleva su error: {visto:?}"
    );
    // Se sigue mirando un rato: nunca «terminada», aunque pasen vigilancias.
    ventana
        .hasta(Duration::from_millis(6 * VIGILAR_MS), |_| false)
        .await;
    for lista in &ventana.listas[desde..] {
        assert!(
            lista
                .fichas
                .iter()
                .all(|ficha| ficha.estado != EstadoFicha::Terminada),
            "un host caído nunca deja fichas terminadas: {lista:?}"
        );
    }
    assert_eq!(
        ventana.ficha("magi-a-1").map(|ficha| ficha.estado),
        Some(EstadoFicha::Inalcanzable)
    );
    // La sesión sigue viva en el host (la prueba lo ve por otro camino).
    assert!(banco.real.sesion("magi-a-1").is_some());
}

/// Entrecomilla una ruta con comillas simples para meterla en un comando de shell.
fn entre_comillas(texto: &Path) -> String {
    format!("'{}'", texto.display().to_string().replace('\'', "'\\''"))
}

/// Las líneas de un panel, sin espacios sobrantes.
fn lineas(panel: &str) -> Vec<&str> {
    panel.lines().map(str::trim).collect()
}

/// Mayús+Intro con `modifyOtherKeys` dentro del tmux de MAGI (D338): el
/// programa de prueba (`tui_prueba`) corre como comando de la sesión, con la
/// configuración que aplica el script (`extended-keys on`) y pide
/// `modifyOtherKeys` como hace Claude Code. MAGI codifica Mayús+Intro con el
/// modo que le pide el cliente de tmux de la pestaña (`CSI 27;2;13~`) y
/// llega al programa como una tecla distinta de Intro: con tmux 3.2 o
/// posterior y `on`, `CSI 13;2u` (el formato `csi-u` que fija el script) o
/// `CSI 27;2;13~`, nunca un `\r`. Con tmux 3.0 y 3.1 (sin `extended-keys`) se
/// omite.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mayus_intro_con_modify_other_keys_llega_dentro_del_tmux_de_magi() {
    let Some(banco) =
        Banco::nuevo("mayus_intro_con_modify_other_keys_llega_dentro_del_tmux_de_magi").await
    else {
        return;
    };
    if banco.real.version < VersionTmux::nueva(3, 2) {
        eprintln!(
            "sesiones_persistentes_e2e::mayus_intro_con_modify_other_keys_llega_dentro_del_tmux_de_magi: \
             se omite, tmux {}.{} no tiene extended-keys (3.2 o posterior)",
            banco.real.version.mayor, banco.real.version.menor
        );
        return;
    }
    // La constante del script decide qué reciben las aplicaciones que no lo
    // piden; a la que sí lo pide (esta) le llega con `on` y con `always`.
    let extended_keys = SCRIPT
        .lines()
        .find_map(|linea| linea.strip_prefix("EXTENDED_KEYS="))
        .expect("EXTENDED_KEYS en el script");
    assert!(
        ["on", "always"].contains(&extended_keys),
        "EXTENDED_KEYS = {extended_keys}: la prueba asume `on` o `always`"
    );

    let programa = comun::compilar_tui_prueba();
    let comando = format!("{} --modify-other-keys", entre_comillas(&programa));
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    let pestana = ventana
        .abrir(
            host,
            PeticionTmux {
                servidor: ServidorTmux::Magi,
                nombre: "claude".to_string(),
                crear: Some(CrearTmux {
                    carpeta: None,
                    comando: Some(comando),
                }),
                compartir: false,
            },
        )
        .await;
    banco.real.esperar_clientes("claude", 1).await;
    ventana.adjuntar(pestana.id).await;
    // El programa está dentro de tmux y ya pidió `CSI > 4 ; 2 m`.
    banco.real.esperar_en_panel("claude", "LISTO").await;

    // tmux le pide `modifyOtherKeys` a su terminal (MAGI, la pestaña): el
    // emulador del cliente lo recoge del flujo del servidor, como en la vista
    // Sesión.
    ventana
        .exigir("modifyOtherKeys pedido por tmux a MAGI", |ven| {
            ven.pantallas.get(&pestana.id).is_some_and(|pantalla| {
                ModoTerminal::desde_emulador(pantalla).modify_other_keys > 0
            })
        })
        .await;
    let modo = ModoTerminal::desde_emulador(ventana.pantallas.get(&pestana.id).unwrap());

    // Mayús+Intro tal como lo codifica MAGI con ese modo.
    let mayus_intro = codificar_tecla(
        Tecla::Intro,
        Modificadores {
            mayusculas: true,
            alt: false,
            ctrl: false,
        },
        modo,
    )
    .expect("Mayús+Intro tiene codificación con modifyOtherKeys");
    assert_eq!(mayus_intro, b"\x1b[27;2;13~");
    ventana
        .enviar(&MensajeCliente::Teclas {
            sesion_id: pestana.id,
            bytes: mayus_intro,
        })
        .await;
    let panel = banco.real.esperar_en_panel("claude", "MAYUS_INTRO").await;
    let recibidas = lineas(&panel);
    assert!(
        recibidas.contains(&"TECLAU 2 13") || recibidas.contains(&"TECLA27 2 13"),
        "Mayús+Intro llega como CSI 13;2u o CSI 27;2;13~, no como otra cosa: {panel}"
    );
    // Y no como un Intro: ningún `\r` suelto llegó al programa.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let panel = banco.real.panel("claude");
    assert!(
        !lineas(&panel).contains(&"INTRO"),
        "Mayús+Intro no debe llegar como Intro: {panel}"
    );

    // Control: un Intro sin modificadores sí llega como Intro.
    let intro = codificar_tecla(Tecla::Intro, Modificadores::default(), modo)
        .expect("Intro tiene codificación");
    assert_eq!(intro, b"\r");
    ventana
        .enviar(&MensajeCliente::Teclas {
            sesion_id: pestana.id,
            bytes: intro,
        })
        .await;
    let limite = Instant::now() + PLAZO;
    loop {
        let panel = banco.real.panel("claude");
        if lineas(&panel).contains(&"INTRO") {
            break;
        }
        assert!(
            Instant::now() < limite,
            "el Intro suelto no llegó al programa: {panel}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Ctrl+Intro, que MAGI codifica con el mismo modo, también llega con su
    // modificador (no como Intro ni como Mayús+Intro).
    let ctrl_intro = codificar_tecla(
        Tecla::Intro,
        Modificadores {
            mayusculas: false,
            alt: false,
            ctrl: true,
        },
        modo,
    )
    .expect("Ctrl+Intro tiene codificación con modifyOtherKeys");
    assert_eq!(ctrl_intro, b"\x1b[27;5;13~");
    ventana
        .enviar(&MensajeCliente::Teclas {
            sesion_id: pestana.id,
            bytes: ctrl_intro,
        })
        .await;
    let panel = banco.real.esperar_en_panel("claude", "5 13").await;
    let recibidas = lineas(&panel);
    assert!(
        recibidas.contains(&"TECLAU 5 13") || recibidas.contains(&"TECLA27 5 13"),
        "Ctrl+Intro llega con su modificador: {panel}"
    );
    // Lo que vio el programa (con `--nocapture`): sirve para ver qué reenvía
    // cada versión de tmux.
    eprintln!(
        "sesiones_persistentes_e2e: tmux {}.{}, EXTENDED_KEYS={extended_keys}; el programa vio {:?}",
        banco.real.version.mayor,
        banco.real.version.menor,
        recibidas
            .iter()
            .filter(|linea| {
                linea.starts_with("TECLA") || linea.starts_with("MAYUS") || **linea == "INTRO"
            })
            .collect::<Vec<_>>()
    );
    // El comando de arranque es texto del usuario: nunca va a REGISTRO.
    banco.exigir_registro_sin(&["tui_prueba", "modify-other-keys"]);
}

// ===========================================================================
// Cierre de la Fase 16 (Fase 9b): la vista previa, renombrar, el historial de
// tmux y el tmux habitual con tmux real, y la pestaña soltada que sigue a su
// sesión tras un renombrado
// ===========================================================================

/// Espera la respuesta a una petición (por su `peticion_id`) entre los
/// mensajes que la ventana ya vio.
async fn esperar_respuesta<F>(
    ventana: &mut Ven,
    motivo: &str,
    mut es_la_respuesta: F,
) -> MensajeServidor
where
    F: FnMut(&MensajeServidor) -> bool,
{
    ventana
        .exigir(motivo, |ven| ven.vistos.iter().any(&mut es_la_respuesta))
        .await;
    ventana
        .vistos
        .iter()
        .rev()
        .find(|mensaje| es_la_respuesta(mensaje))
        .cloned()
        .expect("la respuesta que se acaba de ver")
}

/// La vista previa (`PedirVistaPrevia`) lee de verdad lo que muestra la
/// sesión: texto plano, solo para la ventana que la pide, sin guardarse ni
/// anotarse, con el nombre exacto.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_vista_previa_lee_lo_que_muestra_la_sesion_de_verdad() {
    let Some(banco) = Banco::nuevo("la_vista_previa_lee_lo_que_muestra_la_sesion_de_verdad").await
    else {
        return;
    };
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    let mut otra = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("vista-1", true, false)).await;
    banco.real.esperar_clientes("vista-1", 1).await;
    ventana.cerrar(pestana.id).await;
    banco.real.teclear("vista-1", "echo previa-$((6*7))");
    banco.real.esperar_en_panel("vista-1", "previa-42").await;

    let id = RANGO_TERMINAL + 700;
    ventana
        .enviar(&MensajeCliente::PedirVistaPrevia {
            peticion_id: id,
            host_id: host,
            servidor: ServidorTmux::Magi,
            nombre: "vista-1".to_string(),
        })
        .await;
    let respuesta = esperar_respuesta(&mut ventana, "la vista previa", |mensaje| {
        matches!(mensaje, MensajeServidor::VistaPrevia { peticion_id, .. } if *peticion_id == id)
    })
    .await;
    let MensajeServidor::VistaPrevia { lineas, error, .. } = respuesta else {
        unreachable!()
    };
    assert_eq!(error, None);
    assert!(
        lineas.iter().any(|linea| linea.contains("previa-42")),
        "la vista previa lleva lo que se ve en la sesión: {lineas:?}"
    );
    assert!(
        lineas
            .iter()
            .all(|linea| !linea.contains('\u{1b}') && !linea.ends_with(' ')),
        "texto plano y sin espacios finales: {lineas:?}"
    );

    // Un prefijo que no es el nombre exacto no lee nada ajeno.
    let id_prefijo = RANGO_TERMINAL + 701;
    ventana
        .enviar(&MensajeCliente::PedirVistaPrevia {
            peticion_id: id_prefijo,
            host_id: host,
            servidor: ServidorTmux::Magi,
            nombre: "vista".to_string(),
        })
        .await;
    let respuesta = esperar_respuesta(&mut ventana, "la vista de un prefijo", |mensaje| {
        matches!(
            mensaje,
            MensajeServidor::VistaPrevia { peticion_id, .. } if *peticion_id == id_prefijo
        )
    })
    .await;
    let MensajeServidor::VistaPrevia { lineas, error, .. } = respuesta else {
        unreachable!()
    };
    assert!(lineas.is_empty(), "{lineas:?}");
    assert!(error.is_some(), "un nombre que no existe da su motivo");

    // La otra ventana no ve nada de esto, y no queda rastro: ni REGISTRO ni
    // fichas.
    otra.hasta(Duration::from_millis(800), |_| false).await;
    assert!(
        !otra
            .vistos
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeServidor::VistaPrevia { .. })),
        "la vista previa llegó a otra ventana: {:?}",
        otra.vistos
    );
    banco.exigir_registro_sin(&["previa-42", "echo "]);
}

/// Renombrar (`RenombrarTmux`) con tmux real: las pestañas enganchadas siguen
/// enganchadas con el nombre nuevo, ocupado y fantasma se dicen, y el nombre
/// viejo ya no existe.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn renombrar_con_tmux_real_deja_las_pestanas_enganchadas_y_dice_ocupado_y_fantasma() {
    let Some(banco) = Banco::nuevo(
        "renombrar_con_tmux_real_deja_las_pestanas_enganchadas_y_dice_ocupado_y_fantasma",
    )
    .await
    else {
        return;
    };
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    let uno = ventana.abrir(host, peticion("viejo", true, false)).await;
    banco.real.esperar_clientes("viejo", 1).await;
    let otra = ventana.abrir(host, peticion("ocupado", true, false)).await;
    banco.real.esperar_clientes("ocupado", 1).await;
    ventana.cerrar(otra.id).await;

    let mut numero = 800;
    let mut pedir = |nombre: &str, nuevo: &str| {
        numero += 1;
        (
            numero,
            MensajeCliente::RenombrarTmux {
                peticion_id: RANGO_TERMINAL + numero,
                host_id: host,
                nombre: nombre.to_string(),
                nuevo: nuevo.to_string(),
            },
        )
    };
    for (nombre, nuevo, esperado) in [
        ("viejo", "ocupado", ResultadoRenombrar::Ocupado),
        ("fantasma", "otra", ResultadoRenombrar::NoExiste),
        ("viejo", "nuevo", ResultadoRenombrar::Ok),
    ] {
        let (numero, mensaje) = pedir(nombre, nuevo);
        ventana.enviar(&mensaje).await;
        let respuesta = esperar_respuesta(&mut ventana, "la respuesta a renombrar", |m| {
            matches!(
                m,
                MensajeServidor::TmuxRenombrada { peticion_id, .. }
                    if *peticion_id == RANGO_TERMINAL + numero
            )
        })
        .await;
        let MensajeServidor::TmuxRenombrada { resultado, .. } = respuesta else {
            unreachable!()
        };
        assert_eq!(resultado, esperado, "{nombre} → {nuevo}");
    }
    // El nombre viejo ya no existe, el nuevo sí, y la pestaña sigue
    // enganchada y con el nombre nuevo.
    assert!(banco.real.sesion("viejo").is_none());
    assert_eq!(
        banco.real.sesion("nuevo").map(|(clientes, _)| clientes),
        Some(1)
    );
    ventana
        .exigir("la pestaña sigue con el nombre nuevo", |ven| {
            ven.pestana(uno.id).is_some_and(|pestana| {
                pestana.estado == EstadoSesionRemota::Abierta
                    && pestana
                        .tmux
                        .as_ref()
                        .is_some_and(|tmux| tmux.nombre == "nuevo")
            })
        })
        .await;
    assert!(
        banco.real.sesion("ocupado").is_some(),
        "la ocupada no se toca"
    );
    assert_eq!(banco.escenario.anotaciones("tmux_terminada"), 0);
}

/// El historial de tmux de una pestaña persistente (`CapturarHistorial`) sale
/// de `capture-pane` y llega por lotes: lo que se escribió, también lo que ya
/// salió de la pantalla.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_historial_de_una_pestana_persistente_es_el_de_tmux_de_verdad() {
    let Some(banco) =
        Banco::nuevo("el_historial_de_una_pestana_persistente_es_el_de_tmux_de_verdad").await
    else {
        return;
    };
    let host = banco.host_id;
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("historia", true, false)).await;
    banco.real.esperar_clientes("historia", 1).await;
    ventana.adjuntar(pestana.id).await;
    // Más líneas de las que caben en la pantalla (24): las primeras ya solo
    // están en el historial de tmux.
    ventana
        .teclear(
            pestana.id,
            "for i in $(seq 1 60); do echo linea-hist-$i; done\r",
        )
        .await;
    banco
        .real
        .esperar_en_panel("historia", "linea-hist-60")
        .await;

    let id = RANGO_TERMINAL + 900;
    ventana
        .enviar(&MensajeCliente::CapturarHistorial {
            peticion_id: id,
            sesion_id: pestana.id,
        })
        .await;
    let mut textos: Vec<String> = Vec::new();
    let fin = Instant::now() + PLAZO;
    loop {
        let antes = ventana.vistos.len();
        ventana.paso(Duration::from_millis(100)).await;
        let mut terminado = false;
        for mensaje in &ventana.vistos[antes.min(ventana.vistos.len())..] {
            if let MensajeServidor::HistorialLote {
                peticion_id,
                lineas,
                ultimo,
                ..
            } = mensaje
            {
                if *peticion_id == id {
                    textos.extend(lineas.iter().map(|linea| linea.texto()));
                    terminado |= *ultimo;
                }
            }
        }
        if terminado {
            break;
        }
        assert!(Instant::now() < fin, "el historial no llegó entero");
    }
    // Las primeras líneas ya no están en la pantalla, pero sí en el historial.
    assert!(
        textos.iter().any(|texto| texto == "linea-hist-1"),
        "falta el principio del historial de tmux: {} líneas",
        textos.len()
    );
    assert!(textos.iter().any(|texto| texto == "linea-hist-60"));
    // Nada de lo escrito llega a REGISTRO.
    banco.exigir_registro_sin(&["linea-hist-", "seq 1 60"]);
}

/// El tmux habitual del usuario: se ve en la lista, se engancha sin soltar a
/// nadie y sin cambiarle la configuración ni ponerle marca, y se termina por
/// su nombre exacto a petición del usuario.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_tmux_habitual_se_lista_se_engancha_y_se_termina_sin_tocarle_nada() {
    let Some(banco) =
        Banco::nuevo("el_tmux_habitual_se_lista_se_engancha_y_se_termina_sin_tocarle_nada").await
    else {
        return;
    };
    let host = banco.host_id;
    // La sesión del usuario, con su configuración, en el servidor habitual.
    let suya = banco.real.tmux(
        None,
        &["new-session", "-d", "-s", "suya", "-x", "80", "-y", "24"],
    );
    assert_eq!(suya.codigo, 0, "{}", suya.stderr);
    let opciones_antes = banco.real.tmux(None, &["show", "-g"]).stdout;
    let mut ventana = banco.ventana().await;

    // Se lista junto a las del tmux propio.
    let id = RANGO_TERMINAL + 1000;
    ventana
        .enviar(&MensajeCliente::ListarTmux {
            peticion_id: id,
            host_id: host,
        })
        .await;
    let respuesta = esperar_respuesta(&mut ventana, "la lista de tmux", |mensaje| {
        matches!(mensaje, MensajeServidor::SesionesTmux { peticion_id, .. } if *peticion_id == id)
    })
    .await;
    let MensajeServidor::SesionesTmux { sesiones, .. } = respuesta else {
        unreachable!()
    };
    assert!(
        sesiones
            .iter()
            .any(|s| s.servidor == ServidorTmux::Habitual && s.nombre == "suya"),
        "{sesiones:?}"
    );

    // Engancharse (compartiendo): sin `attach -d`, sin marca y sin configuración.
    let pestana = ventana
        .abrir(
            host,
            PeticionTmux {
                servidor: ServidorTmux::Habitual,
                nombre: "suya".to_string(),
                crear: None,
                compartir: true,
            },
        )
        .await;
    let tmux = pestana.tmux.clone().expect("la pestaña lleva su tmux");
    assert_eq!(tmux.servidor, ServidorTmux::Habitual);
    assert_eq!(tmux.creada, Some(false), "no la creó MAGI");
    banco
        .real
        .esperar("la pestaña engancha a la sesión del usuario", |real| {
            let salida = real.tmux(
                None,
                &["list-sessions", "-F", "#{session_name} #{session_attached}"],
            );
            salida.stdout.lines().any(|linea| linea == "suya 1")
        })
        .await;
    assert_eq!(
        banco.real.tmux(None, &["show", "-g"]).stdout,
        opciones_antes,
        "no se le cambia ninguna opción al tmux del usuario"
    );
    let equipo = banco
        .real
        .tmux(None, &["show", "-t", "=suya:", "-v", "@magi_equipo"]);
    assert!(equipo.stdout.trim().is_empty(), "{}", equipo.stdout);
    assert!(
        banco.real.sesion("suya").is_none(),
        "en el tmux propio no hay ninguna «suya»"
    );

    // Escribir en ella llega, y MAGI no deja nada de lo escrito en REGISTRO.
    ventana.adjuntar(pestana.id).await;
    ventana
        .teclear(pestana.id, "echo habitual-$((9*5))\r")
        .await;
    ventana.exigir_pantalla(pestana.id, "habitual-45").await;

    // Terminarla es a petición del usuario y por el nombre exacto: un prefijo no
    // mata nada.
    for (numero, nombre, esperado) in [
        (1001, "sui", ResultadoTmux::NoExiste),
        (1002, "suya", ResultadoTmux::Ok),
    ] {
        let id = RANGO_TERMINAL + numero;
        ventana
            .enviar(&MensajeCliente::TerminarTmux {
                peticion_id: id,
                host_id: host,
                servidor: ServidorTmux::Habitual,
                nombre: nombre.to_string(),
            })
            .await;
        let respuesta = esperar_respuesta(&mut ventana, "la respuesta a terminar", |mensaje| {
            matches!(mensaje, MensajeServidor::TmuxTerminada { peticion_id, .. } if *peticion_id == id)
        })
        .await;
        let MensajeServidor::TmuxTerminada {
            resultado,
            servidor,
            ..
        } = respuesta
        else {
            unreachable!()
        };
        assert_eq!((servidor, resultado), (ServidorTmux::Habitual, esperado));
    }
    let restantes = banco
        .real
        .tmux(None, &["list-sessions", "-F", "#{session_name}"]);
    assert!(
        !restantes.stdout.lines().any(|linea| linea == "suya"),
        "{}",
        restantes.stdout
    );
    // Su pestaña se va: la sesión terminó.
    ventana
        .exigir("la pestaña de la sesión terminada se va", |ven| {
            ven.pestana(pestana.id).is_none()
        })
        .await;
    assert_eq!(banco.escenario.anotaciones("tmux_terminada"), 1);
    banco.exigir_registro_sin(&["habitual-45", "echo "]);
}

/// R157 con tmux real: otro equipo renombra la sesión y se la lleva con
/// `attach -d`; la pestaña soltada sabe quién la tiene y cómo se llama ahora, y
/// volver se engancha a ese nombre.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_pestana_soltada_sigue_a_su_sesion_si_otro_equipo_la_renombra() {
    let Some(banco) =
        Banco::nuevo("la_pestana_soltada_sigue_a_su_sesion_si_otro_equipo_la_renombra").await
    else {
        return;
    };
    let host = banco.host_id;
    let ajeno = equipo_ajeno();
    let mut ventana = banco.ventana().await;
    let pestana = ventana.abrir(host, peticion("antes", true, false)).await;
    banco.real.esperar_clientes("antes", 1).await;
    ventana.adjuntar(pestana.id).await;
    ventana.teclear(pestana.id, "echo sigue-$((5+5))\r").await;
    ventana.exigir_pantalla(pestana.id, "sigue-10").await;

    // El otro equipo la renombra (desde su propio cliente) y se la lleva.
    let renombrada = banco
        .real
        .tmux(Some("magi"), &["rename-session", "-t", "=antes", "despues"]);
    assert_eq!(renombrada.codigo, 0, "{}", renombrada.stderr);
    let _otro = banco.real.equipo_remoto("despues", &ajeno, true);
    ventana
        .exigir(
            "la pestaña soltada sabe quién la tiene y cómo se llama ahora",
            |ven| {
                ven.pestana(pestana.id).is_some_and(|p| {
                    soltada_por(p).is_some_and(|por| por.as_deref() == Some(&*ajeno))
                        && p.tmux.as_ref().is_some_and(|tmux| tmux.nombre == "despues")
                })
            },
        )
        .await;
    assert_eq!(
        ventana.pestana(pestana.id).unwrap().estado,
        EstadoSesionRemota::Caida
    );
    // La sesión sigue siendo del otro equipo y la pestaña no la cerró.
    assert_eq!(banco.real.sesion("despues"), Some((1, ajeno.clone())));

    // Volver: se engancha a «despues» soltando al otro.
    ventana
        .enviar(&MensajeCliente::Reconectar {
            sesion_id: pestana.id,
            compartir: false,
        })
        .await;
    ventana
        .exigir("la pestaña vuelve a estar abierta, sin marca", |ven| {
            ven.pestana(pestana.id).is_some_and(|p| {
                p.estado == EstadoSesionRemota::Abierta && soltada_por(p).is_none()
            })
        })
        .await;
    banco
        .real
        .esperar("la pestaña es la única enganchada", |real| {
            real.sesion("despues") == Some((1, equipo_propio()))
        })
        .await;
}
