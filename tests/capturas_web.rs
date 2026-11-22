//! Capturas de la web (Fase 9c, informe §7.7): cada vista de la TUI pintada con
//! `App::de_prueba` y un `TestBackend` de 100×30, con el tema fijo y el inventario de
//! la demo (`assets/demo/inventario`, nunca la semilla de las pruebas), exportada a
//! HTML con clases en `web/capturas/`.
//!
//! - `generar_capturas` (ignorada): las escribe. La lanza `scripts/generar-capturas.sh`.
//! - `las_capturas_estan_al_dia`: repite el pintado y compara con `web/capturas/`; si
//!   algo cambia, falla y dice cómo regenerarlas.
//!
//! El arnés es propio y no usa el de `tests/redimensionado/` (arrastra los alias de la
//! semilla), salvo para leer de `semilla.rs` los nombres que ninguna captura puede
//! llevar. Todo es síncrono: `App::de_prueba` crea su propio runtime, así que no hay
//! `#[tokio::test]` ni bucle `ciclo` (que bloquea), solo `procesar` + `paso`.
//!
//! Lo que sale en pantalla es fijo e inventado: los tres hosts de la demo
//! (`demo-web`, `demo-datos` y `demo-copias`), el equipo `sobremesa`, la versión del
//! servidor `VERSION_APP`, las direcciones de documentación (`192.0.2.x`) y el usuario
//! `usuario`. Las antigüedades («hace 2 min») se calculan contra la hora real, a mitad
//! de su redondeo (150 s, 2 h 03 m 30 s) para que no cambien con unos segundos de
//! retraso, y las fechas absolutas son de 2024, que la TUI pinta como «2024» mientras
//! el reloj no esté en ese año. El panel local de Archivos lista una carpeta fija de
//! `/tmp` (la ruta sale entera en pantalla) que las pruebas preparan bajo cerrojo.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, Once};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use filetime::FileTime;
use nix::fcntl::{Flock, FlockArg};
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, CellWidth as _};
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

use magi::almacen::{hosts, snippets, sondeos, tuneles, Almacen};
use magi::app::lanzar::{ModoLanzamiento, PlanEjecucion};
use magi::app::{App, DeliberacionAbierta, Evento};
use magi::archivos::{Entrada, Marca, Propietario, TipoEntrada};
use magi::config::{Config, Rutas};
use magi::deliberacion::{ComprobacionesHost, Veredicto};
use magi::modelo::{DatosTunel, ResultadoSondeo, Sondeo, TipoTunel};
use magi::protocolo::{
    EstadoSesionRemota, EstadoTunelRemoto, InfoSesion, InfoTunel, MensajeCliente, MensajeServidor,
    OrigenTunel,
};
use magi::snippets::{DatosSnippet, Destino};
use magi::tema::Tema;
use magi::ui::Vista;
use magi_nucleo::servicios::{
    Contenedor, EstadoContenedor, FallosContenedores, FallosSondeo, FallosUnidades, Listado, Motor,
    MotorEstado, MotorNombre, PermisoDiario, Privilegio, RelojHost, Salud, Systemd, Timer, Unidad,
};
use magi_web::capturas::{
    exportar, validar_css, validar_fragmento, CapturaCeldas, CeldaCaptura, Cobertura, ColorCaptura,
    ConjuntoExportado, COLUMNAS, FILAS, IDS,
};
use magi_web::fuentes::{codigos_de_ttf, leer_unicodes};

// ------------------------------------------------------------------ constantes

/// Raíz fija de lo que las capturas necesitan en `/tmp`: la ruta del panel local de
/// Archivos sale entera en pantalla y no puede ser un directorio temporal aleatorio.
const RAIZ_FIJA: &str = "/tmp/magi-capturas-web";
const HOGAR_FIJO: &str = "/tmp/magi-capturas-web/hogar";
const CERROJO_FIJO: &str = "/tmp/magi-capturas-web/cerrojo";

/// La versión que dice el servidor en la `Bienvenida`: inventada, para que ninguna
/// captura dependa de la del paquete.
const VERSION_APP: &str = "0.0.0-capturas";

/// El equipo, con un nombre inventado (la base guarda el de la máquina real).
const NOMBRE_EQUIPO: &str = "sobremesa";

/// Los tres hosts de `assets/demo/inventario`, en orden alfabético (el de Flota).
const HOSTS_DEMO: [&str; 3] = ["demo-copias", "demo-datos", "demo-web"];

/// Segundos de antigüedad de los sondeos y del listado de servicios: «hace 2 min» a
/// mitad del redondeo (de 120 a 179 s), estable aunque la prueba tarde unos segundos.
const ANTIGUEDAD_SEG: i64 = 150;

/// 2 h 03 m 30 s: «2 h 03 m» a mitad del intervalo de un minuto.
const SESION_ABIERTA_HACE_SEG: i64 = 2 * 3600 + 3 * 60 + 30;

/// Fecha fija (2024) de lo que lista Archivos.
const FECHA_2024: i64 = 1_726_134_067; // 12 sep 2024 09:41:07 UTC
const DIA: i64 = 86_400;

/// Los ficheros del panel local: nombre, tamaño (`None` = directorio) y fecha.
const LOCALES: [(&str, Option<u64>, i64); 7] = [
    ("app", None, FECHA_2024),
    ("static", None, FECHA_2024 - 9 * DIA),
    ("main.py", Some(14_336), FECHA_2024 + 3 * DIA),
    ("config.yaml", Some(2_048), FECHA_2024),
    ("deploy.sh", Some(1_228), FECHA_2024 - 20 * DIA),
    ("README.md", Some(3_000), FECHA_2024),
    ("notas.md", Some(512), FECHA_2024),
];

const RUTA_REMOTA: &str = "/var/www/demo-web";

// ------------------------------------------------------------------ entorno

/// Las fechas absolutas se pintan en hora local: todas las capturas en UTC.
fn fijar_zona_horaria() {
    static UNA_VEZ: Once = Once::new();
    UNA_VEZ.call_once(|| std::env::set_var("TZ", "UTC"));
}

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

/// Rutas aisladas en un directorio temporal, salvo el hogar, que es el fijo de
/// `/tmp` (lo que lista el panel local de Archivos).
fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magicap")
        .tempdir()
        .expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    Entorno {
        rutas: Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: PathBuf::from(HOGAR_FIJO),
            runtime: raiz.join("runtime"),
        },
        config: Config::default(),
        _temporal: temporal,
    }
}

// ------------------------------------------------------------------ carpeta fija

/// Lo que toca `/tmp/magi-capturas-web` va bajo cerrojo: un `Mutex` entre los hilos de
/// la prueba y un `flock` entre procesos (dos `cargo test` a la vez). Se mantiene
/// durante todo el pintado, porque el panel local lista la carpeta al abrirse.
struct CarpetaFija {
    _hilos: MutexGuard<'static, ()>,
    _procesos: Flock<fs::File>,
}

static CERROJO_DE_HILOS: Mutex<()> = Mutex::new(());

/// Crea (si falta) un directorio de la carpeta fija y comprueba que es un directorio
/// de verdad (no un enlace) y de quien corre la prueba: `/tmp` es de todos.
fn directorio_propio(ruta: &Path) {
    match fs::create_dir(ruta) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => panic!("no se pudo crear {}: {error}", ruta.display()),
    }
    let meta = fs::symlink_metadata(ruta)
        .unwrap_or_else(|error| panic!("no se pudo mirar {}: {error}", ruta.display()));
    assert!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "{} debe ser un directorio, no un fichero ni un enlace: bórralo y repite",
        ruta.display()
    );
    assert_eq!(
        meta.uid(),
        nix::unistd::getuid().as_raw(),
        "{} es de otro usuario: las capturas necesitan una carpeta propia (bórrala o \
         pide a su dueño que lo haga)",
        ruta.display()
    );
    fs::set_permissions(ruta, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("permisos de {}: {error}", ruta.display()));
}

/// Toma el cerrojo y deja `/tmp/magi-capturas-web/hogar` con los ficheros de
/// `LOCALES`, modos explícitos y fechas fijas. Es idempotente y nunca borra nada: si
/// sobra algo (de una versión anterior de la prueba), falla y dice qué.
fn preparar_carpeta_fija() -> CarpetaFija {
    let hilos = CERROJO_DE_HILOS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    directorio_propio(Path::new(RAIZ_FIJA));
    let fichero = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(CERROJO_FIJO)
        .unwrap_or_else(|error| panic!("no se pudo abrir {CERROJO_FIJO}: {error}"));
    let procesos = Flock::lock(fichero, FlockArg::LockExclusive)
        .unwrap_or_else(|(_, error)| panic!("no se pudo tomar el cerrojo {CERROJO_FIJO}: {error}"));

    let hogar = Path::new(HOGAR_FIJO);
    directorio_propio(hogar);
    // Los modos son explícitos: la umask es del proceso y otras pruebas la cambian un
    // instante al arrancar su servidor.
    for (nombre, tamano, _) in LOCALES {
        let ruta = hogar.join(nombre);
        match tamano {
            None => directorio_propio(&ruta),
            Some(bytes) => {
                if let Ok(meta) = fs::symlink_metadata(&ruta) {
                    assert!(
                        meta.is_file(),
                        "{} debe ser un fichero normal: borra {HOGAR_FIJO} y repite",
                        ruta.display()
                    );
                }
                fs::write(&ruta, vec![b'x'; bytes as usize]).unwrap_or_else(|error| {
                    panic!("no se pudo escribir {}: {error}", ruta.display())
                });
                fs::set_permissions(&ruta, fs::Permissions::from_mode(0o644))
                    .unwrap_or_else(|error| panic!("permisos de {}: {error}", ruta.display()));
            }
        }
    }
    // Lo que no es de la prueba no se borra: se dice.
    let esperados: BTreeSet<&str> = LOCALES.iter().map(|(nombre, _, _)| *nombre).collect();
    let sobrantes: Vec<String> = fs::read_dir(hogar)
        .unwrap_or_else(|error| panic!("no se pudo leer {HOGAR_FIJO}: {error}"))
        .filter_map(Result::ok)
        .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
        .filter(|nombre| !esperados.contains(nombre.as_str()))
        .collect();
    assert!(
        sobrantes.is_empty(),
        "sobra {sobrantes:?} en {HOGAR_FIJO}: bórralo (la prueba no borra nada) y repite"
    );
    // Las fechas, al final: crear algo dentro de un directorio le cambia la suya.
    for (nombre, _, mtime) in LOCALES {
        filetime::set_file_mtime(hogar.join(nombre), FileTime::from_unix_time(mtime, 0))
            .unwrap_or_else(|error| panic!("fecha de {nombre}: {error}"));
    }
    CarpetaFija {
        _hilos: hilos,
        _procesos: procesos,
    }
}

// ------------------------------------------------------------------ arnés

/// Una `App` de prueba con su terminal de 100×30 y lo que envía al servidor.
struct Prueba {
    app: App,
    terminal: Terminal<TestBackend>,
    enviados: tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
    _entorno: Entorno,
}

/// Lo que ven las pruebas al sembrar: el id de cada host de la demo.
struct Sembrado {
    hosts: Vec<(String, i64)>,
}

impl Sembrado {
    fn host(&self, nombre: &str) -> i64 {
        self.hosts
            .iter()
            .find(|(otro, _)| otro == nombre)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| panic!("host «{nombre}» no está en el inventario de la demo"))
    }
}

fn ruta_demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/demo/inventario")
}

/// Importa `assets/demo/inventario` como lo haría `magi importar`: sin conflictos (la
/// base está vacía) y con las rutas de `rutas.hogar`.
fn importar_demo(almacen: &Almacen, rutas: &Rutas) -> Sembrado {
    let analisis = magi::sshconfig::importar::analizar_fichero(&ruta_demo(), &rutas.hogar)
        .expect("analizar el inventario de la demo");
    let conexion = almacen.conexion();
    assert!(
        magi::sshconfig::importar::conflictos(conexion, &analisis)
            .expect("conflictos")
            .is_empty(),
        "la base de las capturas debe empezar vacía"
    );
    magi::sshconfig::importar::aplicar(conexion, &analisis, &HashMap::new())
        .expect("importar el inventario de la demo");
    let hosts = hosts::listar(conexion)
        .expect("hosts")
        .into_iter()
        .map(|host| (host.nombre, host.id))
        .collect();
    Sembrado { hosts }
}

impl Prueba {
    /// App con el inventario de la demo importado y, antes de crearla, lo que siembre
    /// `sembrar` (sondeos, túneles, snippets…: la `App` los carga al arrancar).
    fn nueva(sembrar: impl FnOnce(&Almacen, &Sembrado)) -> Self {
        fijar_zona_horaria();
        let entorno = entorno();
        let sembrado = {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
            // El equipo guarda el nombre de la máquina real: se cambia por uno
            // inventado, o saldría en pantalla.
            let este = magi::almacen::equipos::este(almacen.conexion()).expect("este equipo");
            magi::almacen::equipos::renombrar(almacen.conexion(), este.id, NOMBRE_EQUIPO)
                .expect("renombrar el equipo");
            let sembrado = importar_demo(&almacen, &entorno.rutas);
            sembrar(&almacen, &sembrado);
            sembrado
        };
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (app, enviados) = App::de_prueba(
            entorno.rutas.clone(),
            entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
        .expect("app de prueba");
        let terminal =
            Terminal::new(TestBackend::new(COLUMNAS as u16, FILAS as u16)).expect("terminal");
        let mut prueba = Self {
            app,
            terminal,
            enviados,
            _entorno: entorno,
        };
        prueba
            .app
            .iniciar_pintado(&mut prueba.terminal)
            .expect("pintado");
        let _ = sembrado;
        prueba
    }

    fn host_id(&self, nombre: &str) -> i64 {
        self.app
            .hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .map(|host| host.id)
            .unwrap_or_else(|| panic!("host «{nombre}» no está en la App"))
    }

    fn paso(&mut self) {
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn evento(&mut self, evento: Evento) {
        self.app.procesar(evento);
        self.paso();
    }

    fn tecla(&mut self, codigo: KeyCode) {
        self.tecla_con(codigo, KeyModifiers::NONE);
    }

    fn tecla_con(&mut self, codigo: KeyCode, modificadores: KeyModifiers) {
        self.evento(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
    }

    fn servidor(&mut self, mensaje: MensajeServidor) {
        self.evento(Evento::Servidor(mensaje));
    }

    /// Lo enviado al servidor desde la última llamada.
    fn drenar(&mut self) -> Vec<MensajeCliente> {
        let mut mensajes = Vec::new();
        while let Ok(mensaje) = self.enviados.try_recv() {
            mensajes.push(mensaje);
        }
        mensajes
    }

    fn buffer(&self) -> &Buffer {
        self.terminal.backend().buffer()
    }
}

/// Una fecha RFC 3339 con milisegundos, `segundos` atrás.
fn hace_rfc3339(segundos: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::seconds(segundos))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
}

// ------------------------------------------------------------------ las seis vistas

/// Flota: tres hosts sondeados hace 2 min, uno nominal, uno con fallos de servicios y
/// uno con carga alta. Se selecciona el de los fallos, que enseña más detalle.
fn vista_flota() -> Prueba {
    let mut prueba = Prueba::nueva(|almacen, sembrado| {
        let conexion = almacen.conexion();
        let base = |host: &str, carga: f64, mem_libre_pct: i64, disco_pct: i64| Sondeo {
            host_id: sembrado.host(host),
            fecha: hace_rfc3339(ANTIGUEDAD_SEG),
            resultado: ResultadoSondeo::Ok,
            nucleos: Some(8),
            carga_1m: Some(carga),
            carga_5m: Some(carga * 0.8),
            carga_15m: Some(carga * 0.6),
            mem_total_kb: Some(16_000_000),
            mem_disponible_kb: Some(160_000 * mem_libre_pct),
            disco_total_kb: Some(500_000_000),
            disco_usado_kb: Some(5_000_000 * disco_pct),
            red_rx_bytes: Some(1_000_000),
            red_tx_bytes: Some(500_000),
            uptime_seg: Some(12 * 86_400 + 3_600),
            duracion_ms: 420,
            ..Sondeo::default()
        };
        let mut con_fallos = base("demo-datos", 1.10, 61, 44);
        con_fallos.fallos = Some(FallosSondeo {
            unidades: FallosUnidades {
                total: 1,
                nombres: vec!["copia.service".to_string()],
            },
            contenedores: Some(FallosContenedores {
                motor: Some(MotorNombre::Docker),
                total: 1,
                nombres: vec!["web-worker-1".to_string()],
            }),
        });
        for sondeo in [
            base("demo-web", 0.42, 55, 38),
            con_fallos,
            base("demo-copias", 9.60, 12, 71),
        ] {
            sondeos::guardar(conexion, &sondeo).expect("guardar el sondeo");
        }
    });
    prueba.tecla(KeyCode::F(1));
    let datos = prueba.host_id("demo-datos");
    let indice = prueba
        .app
        .hosts_flota()
        .iter()
        .position(|indice| prueba.app.hosts[*indice].id == datos)
        .expect("demo-datos en Flota");
    prueba.app.seleccion_flota = indice;
    prueba.tecla(KeyCode::F(1));
    prueba
}

/// La `Bienvenida` del servidor con estas sesiones abiertas.
fn bienvenida(sesiones: Vec<InfoSesion>) -> MensajeServidor {
    MensajeServidor::Bienvenida {
        version: magi::protocolo::VERSION_PROTOCOLO,
        version_app: VERSION_APP.to_string(),
        pid: 4242,
        cliente_id: 1,
        clientes: 1,
        sesiones,
        transferencias: Vec::new(),
        tuneles: Vec::new(),
        ejecuciones: Vec::new(),
        acciones_servicio: Vec::new(),
        tuneles_temporales: Vec::new(),
    }
}

fn sesion_abierta(id: u32, host_id: i64, host: &str) -> InfoSesion {
    InfoSesion {
        id,
        nombre: host.to_string(),
        host_id,
        host_nombre: host.to_string(),
        estado: EstadoSesionRemota::Abierta,
        motivo: None,
        identidad: "ed25519 (agente)".to_string(),
        abierta_en: chrono::Utc::now().timestamp() - SESION_ABIERTA_HACE_SEG,
        ventanas: 1,
        actividad_no_vista: false,
        reconexion: None,
        titulo: None,
        aviso: None,
        tmux: None,
        escritura_varios: false,
    }
}

/// El prompt de la demo: `usuario@<host>:~$ ` en verde y azul claro (el azul oscuro del
/// ANSI estándar casi no se lee sobre el fondo de la captura).
macro_rules! prompt {
    ($host:literal) => {
        concat!("\x1b[1;32musuario@", $host, "\x1b[0m:\x1b[1;94m~\x1b[0m$ ")
    };
}

/// Lo que dice el panel de `demo-web`: un `ls --color`, el estado de nginx y el final
/// de su registro de accesos (los clientes son direcciones de documentación).
const SALIDA_WEB: &str = concat!(
    "\x1b[H\x1b[2J",
    prompt!("demo-web"),
    "ls --color\r\n",
    "\x1b[1;94mapp\x1b[0m  \x1b[1;94mstatic\x1b[0m  config.yaml  \x1b[1;32mdeploy.sh\x1b[0m\r\n",
    "main.py  notas.md  README.md\r\n",
    prompt!("demo-web"),
    "systemctl status nginx\r\n",
    "\x1b[1;32m●\x1b[0m nginx.service - nginx\r\n",
    "   Loaded: loaded (enabled)\r\n",
    "   Active: \x1b[1;32mactive (running)\x1b[0m\r\n",
    "     Docs: man:nginx(8)\r\n",
    "  Process: 4121 ExecStart=nginx (code=exited)\r\n",
    " Main PID: 4123 (nginx)\r\n",
    "    Tasks: 5 (limit: 18926)\r\n",
    "   Memory: 12.4M\r\n",
    prompt!("demo-web"),
    "tail -n 6 access.log\r\n",
    "192.0.2.7 \"GET / HTTP/1.1\" 200 612\r\n",
    "192.0.2.7 \"GET /static/app.css\" 200 1840\r\n",
    "192.0.2.19 \"GET /api/estado\" 200 94\r\n",
    "192.0.2.19 \"POST /api/pedidos\" 201 211\r\n",
    "\x1b[33m192.0.2.31 \"GET /favicon.ico\" 404 153\x1b[0m\r\n",
    "192.0.2.7 \"GET /api/estado\" 200 94\r\n",
    prompt!("demo-web")
);

/// Lo que dice el panel de `demo-datos`: el disco, la memoria y el registro de
/// postgres, con un aviso en amarillo y un error en rojo.
const SALIDA_DATOS: &str = concat!(
    "\x1b[H\x1b[2J",
    prompt!("demo-datos"),
    "df -h /srv\r\n",
    "Filesystem  Size  Used Avail Use% Mounted\r\n",
    "/dev/sda1   458G  171G  264G  40% /srv\r\n",
    prompt!("demo-datos"),
    "free -h\r\n",
    "        total  used  free  available\r\n",
    "Mem:      15G  9.6G  1.1G       5.9G\r\n",
    "Swap:    2.0G    0B  2.0G\r\n",
    prompt!("demo-datos"),
    "journalctl -u postgresql\r\n",
    "10:14:03 postgres[912]: checkpoint starting\r\n",
    "10:14:09 postgres[912]: checkpoint complete\r\n",
    "\x1b[33m10:21:40 postgres[912]: slow query 1.2 s\x1b[0m\r\n",
    "10:30:00 postgres[912]: checkpoint starting\r\n",
    "\x1b[1;31m10:30:07 postgres[1377]: connection reset\x1b[0m\r\n",
    "10:30:08 postgres[912]: checkpoint complete\r\n",
    prompt!("demo-datos")
);

/// Sesión con dos paneles lado a lado (`Ctrl+]` y espacio), cada uno con su salida.
fn vista_sesion_paneles() -> Prueba {
    let mut prueba = Prueba::nueva(|_, _| {});
    let (web, datos) = (prueba.host_id("demo-web"), prueba.host_id("demo-datos"));
    prueba.servidor(bienvenida(vec![
        sesion_abierta(1, web, "demo-web"),
        sesion_abierta(2, datos, "demo-datos"),
    ]));
    prueba.tecla(KeyCode::F(3));
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.vista, Vista::Sesion);
    // Prefijo (`Ctrl+]`) y espacio: la siguiente disposición, lado a lado.
    prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    prueba.tecla(KeyCode::Char(' '));
    let paneles = prueba.app.paneles_visibles();
    assert_eq!(paneles.len(), 2, "dos paneles");
    let mut sesiones = Vec::new();
    for ((panel, indice), salida) in paneles.into_iter().zip([SALIDA_WEB, SALIDA_DATOS]) {
        let sesion_id = prueba.app.pestanas[indice].sesion_id;
        let rect = prueba
            .app
            .disposicion()
            .panes
            .iter()
            .find(|(otro, _)| *otro == panel)
            .map(|(_, rect)| *rect)
            .expect("el rectángulo del panel");
        // El servidor aplica el tamaño del panel y el remoto repinta a ese tamaño.
        prueba.servidor(MensajeServidor::Redimensionada {
            sesion_id,
            cols: rect.width,
            filas: rect.height,
            ventana_minima: Some(1),
        });
        prueba
            .app
            .pantallas
            .volcar(sesion_id, rect.height, rect.width, salida.as_bytes());
        sesiones.push(sesion_id);
    }
    // El aviso de «disposición lado a lado» dura unos segundos; la captura enseña la
    // vista en reposo, con la barra de teclas de siempre.
    prueba.app.mensaje = None;
    // Solo avisa la pestaña activa: la App repinta por ella y no marca la otra como
    // «con actividad sin ver» (◐), que en un lado a lado no tiene sentido.
    prueba.evento(Evento::Pantallas(sesiones.into_iter().take(1).collect()));
    prueba
}

fn entrada_remota(nombre: &str, tamano: Option<u64>, mtime: i64) -> Entrada {
    Entrada {
        nombre: nombre.to_string(),
        tipo: if tamano.is_some() {
            TipoEntrada::Fichero
        } else {
            TipoEntrada::Directorio
        },
        tamano: tamano.unwrap_or(4_096),
        mtime,
        permisos: Some(0o644),
        propietario: Some(Propietario {
            uid: Some(1000),
            gid: Some(1000),
            usuario: Some("usuario".to_string()),
            grupo: Some("usuario".to_string()),
        }),
        enlace: None,
        marca: Marca::Ninguna,
    }
}

/// Archivos de `demo-web`: el panel local es la carpeta fija y el remoto, un listado
/// con diferencias (`≠`) y ficheros que solo están en uno de los lados (`✕`).
fn vista_archivos() -> Prueba {
    let mut prueba = Prueba::nueva(|_, _| {});
    let host_id = prueba.host_id("demo-web");
    prueba.app.abrir_archivos(Some(host_id));
    assert_eq!(prueba.app.vista, Vista::Archivos);
    assert!(
        prueba
            .drenar()
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeCliente::AbrirSftp { .. })),
        "Archivos debe pedir el canal SFTP"
    );
    prueba.servidor(MensajeServidor::SftpAbierto {
        host_id,
        dir_inicio: RUTA_REMOTA.to_string(),
        peticion_id: None,
        usuario_conexion: Some("usuario".to_string()),
        uid_conexion: Some(1000),
    });
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ListarDir { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("la App pide el listado remoto");
    prueba.servidor(MensajeServidor::DirListado {
        host_id,
        ruta: RUTA_REMOTA.to_string(),
        entradas: vec![
            entrada_remota("app", None, FECHA_2024),
            entrada_remota("static", None, FECHA_2024 - 9 * DIA),
            entrada_remota("main.py", Some(14_336), FECHA_2024),
            entrada_remota("config.yaml", Some(1_024), FECHA_2024),
            entrada_remota("README.md", Some(3_000), FECHA_2024),
            entrada_remota("requirements.txt", Some(300), FECHA_2024),
            entrada_remota("backup.tar.gz", Some(1_153_434), FECHA_2024 - 30 * DIA),
        ],
        peticion_id,
    });
    prueba
}

/// Túneles: uno activo con tráfico, uno caído con su motivo, un SOCKS activo y uno
/// parado (sin estado en el servidor).
fn vista_tuneles() -> Prueba {
    let mut ids = Vec::new();
    let mut prueba = Prueba::nueva(|almacen, sembrado| {
        let conexion = almacen.conexion();
        for (host, nombre, tipo, escucha, destino, automatico) in [
            (
                "demo-datos",
                "base-de-datos",
                TipoTunel::Local,
                "127.0.0.1:15432",
                Some("127.0.0.1:5432"),
                true,
            ),
            (
                "demo-web",
                "panel",
                TipoTunel::Local,
                "127.0.0.1:3000",
                Some("192.0.2.50:3000"),
                false,
            ),
            (
                "demo-copias",
                "socks",
                TipoTunel::Dinamico,
                "127.0.0.1:1080",
                None,
                false,
            ),
            (
                "demo-web",
                "webhook-demo",
                TipoTunel::Remoto,
                "127.0.0.1:8081",
                Some("127.0.0.1:8080"),
                false,
            ),
        ] {
            let id = tuneles::crear(
                conexion,
                &DatosTunel {
                    host_id: sembrado.host(host),
                    nombre: nombre.to_string(),
                    tipo,
                    escucha: escucha.to_string(),
                    destino: destino.map(str::to_string),
                    automatico,
                },
            )
            .expect("crear el túnel");
            ids.push((id, host, nombre, escucha, destino, automatico));
        }
    });
    prueba.tecla(KeyCode::F(6));
    let info = |indice: usize, tipo: &str, estado: EstadoTunelRemoto| {
        let (tunel_id, host, nombre, escucha, destino, automatico) = ids[indice];
        InfoTunel {
            tunel_id,
            host_id: prueba.host_id(host),
            host_nombre: host.to_string(),
            nombre: nombre.to_string(),
            tipo: tipo.to_string(),
            escucha: escucha.to_string(),
            escucha_efectiva: escucha.to_string(),
            destino: destino.map(str::to_string),
            automatico,
            estado,
            origen: Some(OrigenTunel::Manual),
            solicitante: Some(1),
            conexiones: 0,
            aceptadas: 0,
            bytes_subidos: 0,
            bytes_bajados: 0,
            desde: None,
            ultimo_error: None,
        }
    };
    let mut base_de_datos = info(0, "local", EstadoTunelRemoto::Activo);
    base_de_datos.origen = Some(OrigenTunel::Automatico);
    base_de_datos.conexiones = 1;
    base_de_datos.aceptadas = 3;
    base_de_datos.bytes_bajados = 14_200_000;
    base_de_datos.bytes_subidos = 890_000;
    let mut panel = info(1, "local", EstadoTunelRemoto::Caido);
    panel.ultimo_error = Some("connect: conexión rechazada por 192.0.2.50:3000".to_string());
    let mut socks = info(2, "dinamico", EstadoTunelRemoto::Activo);
    socks.conexiones = 2;
    socks.aceptadas = 11;
    socks.bytes_bajados = 2_400_000;
    socks.bytes_subidos = 310_000;
    prueba.servidor(MensajeServidor::Tuneles {
        lista: vec![base_de_datos, panel, socks],
    });
    // El caído (el tercero de la lista, ordenada por tipo), que enseña su motivo.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba
}

fn aprueba(detalle: &str) -> Veredicto {
    Veredicto::Aprueba {
        detalle: detalle.to_string(),
        ms: 1,
    }
}

fn rechaza(detalle: &str) -> Veredicto {
    Veredicto::Rechaza {
        detalle: detalle.to_string(),
        ms: 1,
    }
}

/// El diálogo MAGI sobre la vista de snippets: reiniciar nginx en los tres hosts de
/// la demo, con el backup de `demo-datos` de hace 31 h (bloqueada).
fn vista_deliberacion() -> Prueba {
    let mut prueba = Prueba::nueva(|almacen, sembrado| {
        let conexion = almacen.conexion();
        let todos = || {
            HOSTS_DEMO
                .iter()
                .map(|nombre| Destino::Host {
                    id: sembrado.host(nombre),
                    nombre: (*nombre).to_string(),
                })
                .collect::<Vec<_>>()
        };
        for (nombre, comando, critico, destinos) in [
            (
                "reiniciar nginx",
                "sudo systemctl restart nginx",
                true,
                todos(),
            ),
            (
                "espacio en disco",
                "df -h /",
                false,
                todos().into_iter().take(2).collect(),
            ),
            (
                "actualizar paquetes",
                "sudo apt update && sudo apt -y upgrade",
                true,
                todos(),
            ),
        ] {
            snippets::crear(
                conexion,
                &DatosSnippet {
                    nombre: nombre.to_string(),
                    comando: comando.to_string(),
                    descripcion: format!("{nombre} en los destinos"),
                    critico,
                    destinos,
                    ..DatosSnippet::default()
                },
            )
            .expect("crear el snippet");
        }
    });
    prueba.tecla(KeyCode::F(8));
    // La lista de snippets con el que se va a ejecutar seleccionado, como la dejaría
    // el usuario antes de pulsar `↵`.
    let posicion = prueba
        .app
        .snippets
        .lista
        .iter()
        .position(|snippet| snippet.nombre == "reiniciar nginx")
        .expect("el snippet «reiniciar nginx»");
    let snippet_id = prueba.app.snippets.lista[posicion].id;
    for _ in 0..posicion {
        prueba.tecla(KeyCode::Down);
    }
    let id = |nombre: &str| prueba.host_id(nombre);
    let hosts_del_plan: Vec<(i64, String)> = ["demo-web", "demo-datos", "demo-copias"]
        .iter()
        .map(|nombre| (id(nombre), (*nombre).to_string()))
        .collect();
    let plan = PlanEjecucion {
        snippet_id,
        nombre: "reiniciar nginx".to_string(),
        comando: "sudo systemctl restart nginx".to_string(),
        hosts: hosts_del_plan,
        timeout_seg: 60,
        parar_al_fallo: false,
        critico: true,
        valores: Default::default(),
        modo: ModoLanzamiento::Servidor,
    };
    let filas = vec![
        ComprobacionesHost {
            host_id: id("demo-web"),
            host: "demo-web".to_string(),
            salud: aprueba("NOMINAL · hace 2 min"),
            backup: aprueba("hace 3 h (pg.sql.gz)"),
            tests: aprueba("verde"),
        },
        ComprobacionesHost {
            host_id: id("demo-datos"),
            host: "demo-datos".to_string(),
            salud: aprueba("NOMINAL · hace 2 min"),
            backup: rechaza("último backup hace 31 h (pg.sql.gz)"),
            tests: aprueba("verde"),
        },
        ComprobacionesHost {
            host_id: id("demo-copias"),
            host: "demo-copias".to_string(),
            salud: aprueba("NOMINAL · hace 2 min"),
            backup: Veredicto::NoActiva,
            tests: Veredicto::NoActiva,
        },
    ];
    prueba.app.deliberacion = Some(DeliberacionAbierta::de_prueba(plan, filas, 10));
    prueba
        .app
        .iniciar_pintado(&mut prueba.terminal)
        .expect("pintado");
    prueba
}

fn unidad(nombre: &str, descripcion: &str, activo: &str, arranque: &str) -> Unidad {
    let mut props = std::collections::BTreeMap::new();
    props.insert("Description".to_string(), descripcion.to_string());
    props.insert("ActiveState".to_string(), activo.to_string());
    props.insert(
        "SubState".to_string(),
        if activo == "failed" {
            "failed".to_string()
        } else {
            "running".to_string()
        },
    );
    props.insert("UnitFileState".to_string(), arranque.to_string());
    props.insert("CanReload".to_string(), "no".to_string());
    props.insert("LoadState".to_string(), "loaded".to_string());
    props.insert("Result".to_string(), "success".to_string());
    props.insert("ExecMainPID".to_string(), "4121".to_string());
    props.insert("MemoryCurrent".to_string(), "188743680".to_string());
    props.insert(
        "FragmentPath".to_string(),
        format!("/etc/systemd/system/{nombre}"),
    );
    props.insert(
        "ActiveEnterTimestampMonotonic".to_string(),
        "92800000000".to_string(),
    );
    Unidad {
        nombre: nombre.to_string(),
        props,
        vigilada: false,
    }
}

fn timer(nombre: &str, servicio: &str) -> Timer {
    let mut props = std::collections::BTreeMap::new();
    props.insert("Description".to_string(), format!("{nombre} de la demo"));
    props.insert("ActiveState".to_string(), "active".to_string());
    props.insert(
        "NextElapseUSecRealtime".to_string(),
        "1760000600000000".to_string(),
    );
    props.insert(
        "LastTriggerUSecMonotonic".to_string(),
        "90000000000".to_string(),
    );
    Timer {
        nombre: nombre.to_string(),
        props,
        servicio: Some(servicio.to_string()),
    }
}

fn contenedor(
    nombre: &str,
    proyecto: Option<&str>,
    estado: EstadoContenedor,
    codigo: Option<i32>,
    salud: Salud,
) -> Contenedor {
    Contenedor {
        id: format!("{nombre}-0123456789ab"),
        nombre: nombre.to_string(),
        imagen: "ghcr.io/ejemplo/app:1.4".to_string(),
        estado,
        codigo_salida: codigo,
        salud,
        puertos: "127.0.0.1:8080->80/tcp".to_string(),
        proyecto: proyecto.map(str::to_string),
        ficheros: proyecto
            .map(|_| vec!["/srv/web/compose.yaml".to_string()])
            .unwrap_or_default(),
        servicio: proyecto.map(|_| nombre.to_string()),
        orden: proyecto.map(|_| "1".to_string()),
        etiquetas: std::collections::BTreeMap::new(),
    }
}

/// Servicios de `demo-web`: unidades de systemd (una caída) y contenedores de Docker.
fn vista_servicios() -> Prueba {
    let mut prueba = Prueba::nueva(|_, _| {});
    let host_id = prueba.host_id("demo-web");
    prueba.tecla(KeyCode::F(2));
    prueba.app.seleccionar_host_id(host_id);
    prueba.tecla(KeyCode::Char('u'));
    assert_eq!(prueba.app.vista, Vista::Servicios);
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::ListarServicios {
                peticion_id,
                host_id: otro,
                ..
            } if otro == host_id => Some(peticion_id),
            _ => None,
        })
        .next_back()
        .expect("Servicios debe pedir el listado");
    let mut listado = Listado::vacio(Privilegio::Root);
    listado.systemd = Systemd::Disponible("252".to_string());
    listado.reloj = RelojHost {
        epoch: 1_760_000_000,
        uptime_seg: 100_000,
    };
    listado.diario = PermisoDiario::Todo;
    listado.motor = MotorEstado::Disponible(Motor {
        nombre: MotorNombre::Docker,
        version: "27.3".to_string(),
    });
    let mut nginx = unidad("nginx.service", "Servidor web nginx", "active", "enabled");
    nginx.vigilada = true;
    let mut app = unidad("app.service", "Aplicación de la demo", "active", "enabled");
    app.vigilada = true;
    listado.unidades = vec![
        unidad("copia.service", "Copia de seguridad", "failed", "disabled"),
        nginx,
        app,
        unidad("postgresql.service", "Base de datos", "active", "enabled"),
        unidad("cron.service", "Tareas programadas", "active", "enabled"),
        unidad("ssh.service", "Servidor SSH", "active", "enabled"),
    ];
    listado.timers = vec![
        timer("backup.timer", "backup.service"),
        timer("limpieza.timer", "limpieza.service"),
    ];
    listado.contenedores = vec![
        contenedor(
            "web-app-1",
            Some("web"),
            EstadoContenedor::EnMarcha,
            None,
            Salud::Sana,
        ),
        contenedor(
            "web-worker-1",
            Some("web"),
            EstadoContenedor::Parado,
            Some(1),
            Salud::Ninguna,
        ),
        contenedor("panel", None, EstadoContenedor::EnMarcha, None, Salud::Sana),
    ];
    prueba.servidor(MensajeServidor::Servicios {
        peticion_id,
        host_id,
        listado,
    });
    // El listado llegó hace 2 min (con el reloj de la prueba, a mitad del redondeo).
    // Si el reloj monotónico lleva menos que eso (un equipo recién arrancado), la
    // captura diría otra antigüedad: falla en vez de escribir una captura distinta.
    let recibido = Instant::now()
        .checked_sub(std::time::Duration::from_secs(ANTIGUEDAD_SEG as u64))
        .expect("el reloj monotónico lleva menos de 150 s: espera un poco y repite");
    prueba
        .app
        .servicios
        .listados
        .get_mut(&host_id)
        .expect("el listado de servicios de demo-web")
        .recibido = recibido;
    // Bajar y subir repinta con la antigüedad ya fijada y deja seleccionado el fallido.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Up);
    prueba
}

/// Cómo se monta una vista: una `App` ya pintada.
type MontarVista = fn() -> Prueba;

/// Las seis vistas con su id, en el orden de `capturas::IDS`.
fn vistas() -> [(&'static str, MontarVista); 6] {
    [
        ("flota", vista_flota),
        ("sesion-paneles", vista_sesion_paneles),
        ("archivos", vista_archivos),
        ("tuneles", vista_tuneles),
        ("deliberacion", vista_deliberacion),
        ("servicios", vista_servicios),
    ]
}

/// Pinta las seis vistas, en el orden de `IDS`. Toma el cerrojo de la carpeta fija
/// mientras dura.
fn pintar_capturas() -> Vec<CapturaCeldas> {
    let _carpeta = preparar_carpeta_fija();
    let vistas = vistas();
    assert_eq!(
        vistas.map(|(id, _)| id),
        IDS,
        "las vistas van en el orden de capturas::IDS"
    );
    vistas
        .into_iter()
        .map(|(id, vista)| {
            let prueba = vista();
            comprobar_inventario(&prueba);
            celdas_de(id, prueba.buffer()).unwrap_or_else(|error| panic!("captura «{id}»: {error}"))
        })
        .collect()
}

// ------------------------------------------------------------------ inventario

/// Los hosts de la App, ordenados por nombre.
fn nombres_de_hosts(app: &App) -> Vec<String> {
    let mut nombres: Vec<String> = app.hosts.iter().map(|host| host.nombre.clone()).collect();
    nombres.sort();
    nombres
}

/// Los `Host` del inventario de la demo, leídos del fichero (no de lo que importa la
/// prueba: así la comparación es independiente).
fn hosts_del_inventario_de_la_demo() -> Vec<String> {
    let texto = fs::read_to_string(ruta_demo()).expect("assets/demo/inventario");
    let mut nombres: Vec<String> = texto
        .lines()
        .filter_map(|linea| linea.trim().strip_prefix("Host "))
        .map(|nombre| nombre.trim().to_string())
        .collect();
    nombres.sort();
    nombres
}

/// Los nombres son exactamente los del inventario de la demo (`demo-*`).
fn comprobar_nombres_de_hosts(nombres: &[String]) -> Result<(), String> {
    let mut ordenados = nombres.to_vec();
    ordenados.sort();
    let de_la_demo = hosts_del_inventario_de_la_demo();
    if ordenados != de_la_demo {
        return Err(format!(
            "los hosts de la App ({ordenados:?}) deben ser exactamente los de assets/demo/inventario ({de_la_demo:?})"
        ));
    }
    if ordenados != HOSTS_DEMO {
        return Err(format!(
            "los tres hosts de la demo son {HOSTS_DEMO:?}, no {ordenados:?}"
        ));
    }
    if let Some(ajeno) = ordenados.iter().find(|nombre| !nombre.starts_with("demo-")) {
        return Err(format!(
            "todo host de las capturas se llama demo-*: {ajeno}"
        ));
    }
    Ok(())
}

/// La App tiene exactamente los hosts de la demo.
fn comprobar_inventario(prueba: &Prueba) {
    if let Err(error) = comprobar_nombres_de_hosts(&nombres_de_hosts(&prueba.app)) {
        panic!("{error}");
    }
}

// ------------------------------------------------------------------ Buffer → celdas

fn color_de(color: Color) -> Result<ColorCaptura, String> {
    Ok(match color {
        Color::Reset => ColorCaptura::Reset,
        Color::Rgb(rojo, verde, azul) => ColorCaptura::Rgb(rojo, verde, azul),
        Color::Indexed(indice) => ColorCaptura::Indexado(indice),
        Color::Black => ColorCaptura::Indexado(0),
        Color::Red => ColorCaptura::Indexado(1),
        Color::Green => ColorCaptura::Indexado(2),
        Color::Yellow => ColorCaptura::Indexado(3),
        Color::Blue => ColorCaptura::Indexado(4),
        Color::Magenta => ColorCaptura::Indexado(5),
        Color::Cyan => ColorCaptura::Indexado(6),
        Color::Gray => ColorCaptura::Indexado(7),
        Color::DarkGray => ColorCaptura::Indexado(8),
        Color::LightRed => ColorCaptura::Indexado(9),
        Color::LightGreen => ColorCaptura::Indexado(10),
        Color::LightYellow => ColorCaptura::Indexado(11),
        Color::LightBlue => ColorCaptura::Indexado(12),
        Color::LightMagenta => ColorCaptura::Indexado(13),
        Color::LightCyan => ColorCaptura::Indexado(14),
        Color::White => ColorCaptura::Indexado(15),
    })
}

/// Los modificadores que `CeldaCaptura` sabe decir. Cualquier otro (parpadeo,
/// tachado) haría una captura que no es lo que pinta la TUI: es un error.
const MODIFICADORES_SOPORTADOS: Modifier = Modifier::BOLD
    .union(Modifier::DIM)
    .union(Modifier::ITALIC)
    .union(Modifier::UNDERLINED)
    .union(Modifier::REVERSED)
    .union(Modifier::HIDDEN);

/// Pasa un `Buffer` de ratatui a las celdas que exporta `magi-web`: sin las celdas
/// que continúan un carácter ancho (las salta por su ancho) y con los colores con
/// nombre como índices de la paleta ANSI.
fn celdas_de(id: &str, buffer: &Buffer) -> Result<CapturaCeldas, String> {
    let area = buffer.area;
    let mut filas = Vec::with_capacity(usize::from(area.height));
    for y in area.top()..area.bottom() {
        let mut fila = Vec::with_capacity(usize::from(area.width));
        let mut x = area.left();
        while x < area.right() {
            let celda = &buffer[(x, y)];
            let sobrante = celda.modifier.difference(MODIFICADORES_SOPORTADOS);
            if !sobrante.is_empty() {
                return Err(format!(
                    "la celda ({x}, {y}) lleva {sobrante:?}, que la captura no sabe pintar"
                ));
            }
            let modificador = celda.modifier;
            fila.push(CeldaCaptura {
                simbolo: celda.symbol().to_string(),
                fg: color_de(celda.fg).map_err(|e| format!("celda ({x}, {y}): {e}"))?,
                bg: color_de(celda.bg).map_err(|e| format!("celda ({x}, {y}): {e}"))?,
                negrita: modificador.contains(Modifier::BOLD),
                tenue: modificador.contains(Modifier::DIM),
                cursiva: modificador.contains(Modifier::ITALIC),
                subrayado: modificador.contains(Modifier::UNDERLINED),
                invertido: modificador.contains(Modifier::REVERSED),
                oculta: modificador.contains(Modifier::HIDDEN),
            });
            // Un carácter de ancho w ocupa w celdas: las w−1 siguientes no se emiten, y
            // deben estar en blanco (ratatui las deja así): si llevan algo, saltarlas lo
            // perdería en silencio.
            let ancho = celda.cell_width().max(1);
            for desplazamiento in 1..ancho {
                let continuacion = x.saturating_add(desplazamiento);
                if let Some(otra) = buffer.cell((continuacion, y)) {
                    if !otra.symbol().trim().is_empty() {
                        return Err(format!(
                            "la celda ({continuacion}, {y}) debería continuar a ({x}, {y}) y lleva {:?}",
                            otra.symbol()
                        ));
                    }
                }
            }
            x = x.saturating_add(ancho);
        }
        filas.push(fila);
    }
    Ok(CapturaCeldas {
        id: id.to_string(),
        filas,
    })
}

// ------------------------------------------------------------------ glifos

fn ruta_del_repositorio(relativa: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relativa)
}

/// La unión de lo que tienen JetBrains Mono (normal y negrita, los TTF de la app) y
/// el subconjunto de símbolos de la web.
fn cobertura() -> Cobertura {
    let mut codigos = BTreeSet::new();
    for tipografia in ["jetbrains_mono_regular.ttf", "jetbrains_mono_bold.ttf"] {
        let ruta = ruta_del_repositorio(&format!("android/app/src/main/res/font/{tipografia}"));
        let bytes = fs::read(&ruta).unwrap_or_else(|e| panic!("{}: {e}", ruta.display()));
        codigos
            .extend(codigos_de_ttf(&bytes).unwrap_or_else(|e| panic!("{}: {e}", ruta.display())));
    }
    let ruta = ruta_del_repositorio("web/fuentes/magi-simbolos.unicodes");
    let texto = fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("{}: {e}", ruta.display()));
    codigos.extend(leer_unicodes(&texto).unwrap_or_else(|e| panic!("{}: {e}", ruta.display())));
    Cobertura::desde_codigos(codigos)
}

/// Exporta las capturas con el degradador de la TUI (`texto_ascii`, el único).
fn exportar_capturas(capturas: &[CapturaCeldas]) -> ConjuntoExportado {
    exportar(capturas, &cobertura(), &magi::ui::disposicion::texto_ascii)
        .unwrap_or_else(|error| panic!("no se pudieron exportar las capturas: {error}"))
}

// ------------------------------------------------------------------ ficheros

fn directorio_de_capturas() -> PathBuf {
    ruta_del_repositorio("web/capturas")
}

/// Lo que se escribe en `web/capturas/<id>.html`: el fragmento y un salto de línea
/// (quien lo lea lo recorta, como dice `validar_fragmento`).
fn contenido_de_fragmento(fragmento: &str) -> String {
    format!("{fragmento}\n")
}

fn contenido_de_css(css: &str) -> String {
    if css.ends_with('\n') {
        css.to_string()
    } else {
        format!("{css}\n")
    }
}

/// Escribe un fichero de forma atómica: temporal con nombre único (`create_new`) en
/// el mismo directorio y `rename`.
fn escribir_atomico(ruta: &Path, contenido: &str) {
    let directorio = ruta.parent().expect("directorio del fichero");
    fs::create_dir_all(directorio).expect("crear web/capturas");
    let nombre = ruta
        .file_name()
        .expect("nombre del fichero")
        .to_string_lossy();
    let temporal = directorio.join(format!(".{nombre}.{}.tmp", std::process::id()));
    {
        let mut fichero = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&temporal)
            .unwrap_or_else(|e| panic!("{}: {e}", temporal.display()));
        fichero
            .write_all(contenido.as_bytes())
            .unwrap_or_else(|e| panic!("{}: {e}", temporal.display()));
        fichero
            .sync_all()
            .unwrap_or_else(|e| panic!("{}: {e}", temporal.display()));
    }
    fs::rename(&temporal, ruta).unwrap_or_else(|e| {
        let _ = fs::remove_file(&temporal);
        panic!("{}: {e}", ruta.display())
    });
}

// ------------------------------------------------------------------ las pruebas

/// Todo lo que `generar_capturas` escribe, como pares (ruta, contenido), ya exportado
/// y comprobado: nada se escribe hasta que lo que sale se puede publicar (nombres de la
/// semilla, datos del equipo, glifos, fragmento y CSS válidos). Así una captura con un
/// dato que no debe salir falla antes de tocar `web/capturas/`.
fn ficheros_a_escribir(capturas: &[CapturaCeldas]) -> Vec<(PathBuf, String)> {
    let conjunto = exportar_capturas(capturas);
    comprobar_que_se_puede_publicar(capturas, &conjunto);
    for (id, fragmento) in &conjunto.fragmentos {
        validar_fragmento(fragmento).unwrap_or_else(|e| panic!("captura «{id}»: {e}"));
    }
    validar_css(&conjunto.css).unwrap_or_else(|e| panic!("capturas.css: {e}"));
    let directorio = directorio_de_capturas();
    let mut ficheros = Vec::with_capacity(IDS.len() + 1);
    for id in IDS {
        let fragmento = conjunto
            .fragmentos
            .get(id)
            .unwrap_or_else(|| panic!("falta la captura «{id}»"));
        ficheros.push((
            directorio.join(format!("{id}.html")),
            contenido_de_fragmento(fragmento),
        ));
    }
    ficheros.push((
        directorio.join("capturas.css"),
        contenido_de_css(&conjunto.css),
    ));
    ficheros
}

/// Pinta, exporta, valida y escribe `web/capturas/`. La lanza
/// `scripts/generar-capturas.sh`.
#[test]
#[ignore = "escribe web/capturas/: la lanza scripts/generar-capturas.sh"]
fn generar_capturas() {
    fijar_zona_horaria();
    let ficheros = ficheros_a_escribir(&pintar_capturas());
    for (ruta, contenido) in &ficheros {
        escribir_atomico(ruta, contenido);
        println!("escrita {}", ruta.display());
    }
}

// ------------------------------------------------------------------ lo que no puede salir

/// Un UUID canónico en minúsculas (8-4-4-4-12 hexadecimales).
fn es_uuid(texto: &str) -> bool {
    let partes: Vec<&str> = texto.split('-').collect();
    partes.len() == 5
        && partes
            .iter()
            .zip([8usize, 4, 4, 4, 12])
            .all(|(parte, largo)| {
                parte.len() == largo && parte.bytes().all(|b| b.is_ascii_hexdigit())
            })
}

/// Minúsculas y dígitos en dos o más grupos separados por un guion (`hetzner-01`).
fn tiene_forma_de_nombre(texto: &str) -> bool {
    let grupos: Vec<&str> = texto.split('-').collect();
    grupos.len() >= 2
        && grupos.iter().all(|grupo| {
            !grupo.is_empty()
                && grupo
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Los nombres de host y de equipo de la semilla de las pruebas
/// (`tests/redimensionado/semilla.rs`), sacados de sus literales con forma de nombre
/// (`"[a-z0-9]+(-[a-z0-9]+)+"`), sin `ssh-ed25519` ni los UUID. Se leen del fichero,
/// no de una lista copiada: si la semilla gana un nombre, las capturas lo vigilan.
fn nombres_de_la_semilla() -> Vec<String> {
    nombres_en_literales(include_str!("redimensionado/semilla.rs"))
}

fn nombres_en_literales(codigo: &str) -> Vec<String> {
    let bytes = codigo.as_bytes();
    let mut nombres = BTreeSet::new();
    let mut posicion = 0;
    while posicion < bytes.len() {
        if bytes[posicion] != b'"' {
            posicion += 1;
            continue;
        }
        let inicio = posicion + 1;
        let mut fin = inicio;
        while fin < bytes.len()
            && (bytes[fin].is_ascii_lowercase()
                || bytes[fin].is_ascii_digit()
                || bytes[fin] == b'-')
        {
            fin += 1;
        }
        // Solo bytes ASCII entre dos comillas: los límites son de carácter.
        if fin < bytes.len() && bytes[fin] == b'"' && fin > inicio {
            let candidato = &codigo[inicio..fin];
            if tiene_forma_de_nombre(candidato) && candidato != "ssh-ed25519" && !es_uuid(candidato)
            {
                nombres.insert(candidato.to_string());
            }
            posicion = fin + 1;
        } else {
            posicion = inicio;
        }
    }
    nombres.into_iter().collect()
}

/// Cuáles de estos nombres salen en el texto (cualquier aparición cuenta, aunque sea
/// parte de otra palabra: ante la duda, falla). Ordenados y sin repetir.
fn contiene_nombres_de_la_semilla(texto: &str, nombres: &[String]) -> Vec<String> {
    let mut encontrados: Vec<String> = nombres
        .iter()
        .filter(|nombre| !nombre.is_empty() && texto.contains(nombre.as_str()))
        .cloned()
        .collect();
    encontrados.sort();
    encontrados.dedup();
    encontrados
}

/// `palabra` aparece en el texto como palabra entera (sin letras, dígitos, `-`, `_` ni
/// `.` pegados a los lados).
fn contiene_palabra(texto: &str, palabra: &str) -> bool {
    if palabra.is_empty() {
        return false;
    }
    let es_de_nombre = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.');
    texto.match_indices(palabra).any(|(inicio, _)| {
        let antes = texto[..inicio].chars().next_back();
        let despues = texto[inicio + palabra.len()..].chars().next();
        !antes.is_some_and(es_de_nombre) && !despues.is_some_and(es_de_nombre)
    })
}

/// Lo que identifica a quien corre la prueba y no puede salir en una captura: el
/// nombre real de la máquina, el usuario y su carpeta personal. Se descartan los
/// valores demasiado cortos para buscarlos sin falsos positivos y los inventados que
/// la propia captura usa.
fn datos_del_equipo() -> Vec<(&'static str, String)> {
    let mut datos = Vec::new();
    if let Ok(nombre) = nix::unistd::gethostname() {
        datos.push((
            "el nombre de la máquina",
            nombre.to_string_lossy().into_owned(),
        ));
    }
    for variable in ["USER", "LOGNAME"] {
        if let Ok(valor) = std::env::var(variable) {
            datos.push(("el usuario del sistema", valor));
        }
    }
    datos.retain(|(_, valor)| {
        valor.chars().count() >= 4
            && !["usuario", NOMBRE_EQUIPO, "localhost"].contains(&valor.as_str())
    });
    datos.sort();
    datos.dedup();
    datos
}

/// Todo lo que impide publicar un texto de captura: nombres de la semilla, datos del
/// equipo, la versión del paquete, restos de directorios temporales y rutas de
/// usuario que no sean la de documentación.
fn comprobar_texto_publicable(texto: &str, nombres_semilla: &[String]) -> Result<(), String> {
    let de_la_semilla = contiene_nombres_de_la_semilla(texto, nombres_semilla);
    if !de_la_semilla.is_empty() {
        return Err(format!(
            "lleva nombres de la semilla de las pruebas: {de_la_semilla:?}"
        ));
    }
    for (que, valor) in datos_del_equipo() {
        if contiene_palabra(texto, &valor) {
            return Err(format!("lleva {que}"));
        }
    }
    if let Ok(hogar) = std::env::var("HOME") {
        if hogar.chars().count() >= 4 && texto.contains(&hogar) {
            return Err("lleva la carpeta personal de quien la genera".to_string());
        }
    }
    if texto.contains(env!("CARGO_PKG_VERSION")) {
        return Err(format!(
            "lleva la versión del paquete ({})",
            env!("CARGO_PKG_VERSION")
        ));
    }
    if texto.contains("magicap") {
        return Err("lleva la ruta de un directorio temporal de la prueba".to_string());
    }
    for (inicio, _) in texto.match_indices("/home/") {
        if !texto[inicio + "/home/".len()..].starts_with("usuario") {
            return Err("lleva una ruta de /home que no es la de documentación".to_string());
        }
    }
    Ok(())
}

/// El texto que se lee en un fragmento: sin etiquetas y con las cinco entidades
/// deshechas.
fn texto_de_fragmento(fragmento: &str) -> String {
    let mut texto = String::with_capacity(fragmento.len());
    let mut en_etiqueta = false;
    for c in fragmento.chars() {
        match c {
            '<' => en_etiqueta = true,
            '>' if en_etiqueta => en_etiqueta = false,
            _ if en_etiqueta => {}
            _ => texto.push(c),
        }
    }
    texto
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Los nombres de la semilla con los que se vigila, tras comprobar que el lector no se
/// queda mudo: sin nombres no vigilaría nada.
fn nombres_vigilados() -> Vec<String> {
    let nombres = nombres_de_la_semilla();
    for esperado in [
        "hetzner-01",
        "hetzner-02",
        "backup-nas",
        "mac-mini-m1",
        "rincon-dev",
    ] {
        assert!(
            nombres.iter().any(|nombre| nombre == esperado),
            "no se leyó {esperado} de tests/redimensionado/semilla.rs: {nombres:?}"
        );
    }
    assert!(
        nombres.iter().all(|nombre| tiene_forma_de_nombre(nombre)
            && !es_uuid(nombre)
            && nombre != "ssh-ed25519"),
        "{nombres:?}"
    );
    nombres
}

/// Falla si algo de lo que se va a publicar lleva un dato que no puede salir: en la
/// captura tal cual la pinta la TUI y en lo que acaba en los ficheros, ya degradado.
/// La usan la prueba normal y `generar_capturas`, antes de escribir nada.
fn comprobar_que_se_puede_publicar(capturas: &[CapturaCeldas], conjunto: &ConjuntoExportado) {
    let nombres = nombres_vigilados();
    for captura in capturas {
        assert_eq!(captura.filas.len(), FILAS, "filas de «{}»", captura.id);
        comprobar_texto_publicable(&captura.texto(), &nombres)
            .unwrap_or_else(|error| panic!("la captura «{}» {error}", captura.id));
    }
    for (id, fragmento) in &conjunto.fragmentos {
        comprobar_texto_publicable(&texto_de_fragmento(fragmento), &nombres)
            .unwrap_or_else(|error| panic!("el fichero de «{id}» {error}"));
    }
}

// ------------------------------------------------------------------ comparación con lo versionado

/// La primera diferencia entre lo que pinta la TUI y lo que hay en disco, o `None` si
/// son iguales byte a byte.
fn primera_diferencia(esperado: &str, en_disco: &str) -> Option<String> {
    if esperado == en_disco {
        return None;
    }
    let recortar = |linea: &str| -> String {
        let corta: String = linea.chars().take(110).collect();
        if corta.len() < linea.len() {
            format!("{corta}…")
        } else {
            corta
        }
    };
    let mut esperadas = esperado.split('\n');
    let mut reales = en_disco.split('\n');
    let mut numero = 1;
    loop {
        match (esperadas.next(), reales.next()) {
            (Some(a), Some(b)) if a == b => numero += 1,
            (Some(a), Some(b)) => {
                return Some(format!(
                    "línea {numero}: la TUI pinta «{}» y el fichero dice «{}»",
                    recortar(a),
                    recortar(b)
                ))
            }
            (Some(a), None) => {
                return Some(format!(
                    "línea {numero}: al fichero le falta «{}»",
                    recortar(a)
                ))
            }
            (None, Some(b)) => {
                return Some(format!(
                    "línea {numero}: al fichero le sobra «{}»",
                    recortar(b)
                ))
            }
            (None, None) => return Some("difieren en los saltos de línea del final".to_string()),
        }
    }
}

/// Compara un fichero versionado con lo esperado; añade su descripción a `fallos` si
/// falta o difiere.
fn comparar_con_disco(ruta: &Path, esperado: &str, fallos: &mut Vec<String>) {
    let nombre = ruta
        .strip_prefix(ruta_del_repositorio(""))
        .unwrap_or(ruta)
        .display()
        .to_string();
    match fs::read_to_string(ruta) {
        Ok(en_disco) => {
            if let Some(diferencia) = primera_diferencia(esperado, &en_disco) {
                fallos.push(format!("{nombre}: {diferencia}"));
            }
        }
        Err(error) => fallos.push(format!("{nombre}: no se puede leer ({error})")),
    }
}

// ------------------------------------------------------------------ pruebas

/// Si algo de lo que pinta la TUI cambia, esta prueba falla y dice cómo regenerar las
/// capturas: ejecuta `scripts/generar-capturas.sh` y sube el diff en el mismo PR.
#[test]
fn las_capturas_estan_al_dia() {
    fijar_zona_horaria();
    let conjunto = exportar_capturas(&pintar_capturas());
    let directorio = directorio_de_capturas();
    let mut fallos = Vec::new();
    for id in IDS {
        let fragmento = conjunto
            .fragmentos
            .get(id)
            .unwrap_or_else(|| panic!("falta la captura «{id}»"));
        comparar_con_disco(
            &directorio.join(format!("{id}.html")),
            &contenido_de_fragmento(fragmento),
            &mut fallos,
        );
    }
    comparar_con_disco(
        &directorio.join("capturas.css"),
        &contenido_de_css(&conjunto.css),
        &mut fallos,
    );
    assert!(
        fallos.is_empty(),
        "captura desfasada: ejecuta scripts/generar-capturas.sh y revisa `git diff web/capturas` \
         (lo que pinta la TUI ya no coincide con lo versionado)\n  {}",
        fallos.join("\n  ")
    );
}

#[test]
fn una_captura_distinta_de_lo_versionado_se_nota_y_dice_donde() {
    fijar_zona_horaria();
    let base = "<pre class=\"captura\" aria-hidden=\"true\">uno\ndos\ntres</pre>\n";
    assert_eq!(primera_diferencia(base, base), None);
    // Un carácter cambiado.
    let otra = base.replace("dos", "dós");
    let diferencia = primera_diferencia(base, &otra).expect("una diferencia");
    assert!(
        diferencia.contains("línea 2") && diferencia.contains("dós"),
        "{diferencia}"
    );
    // Una fila de menos y una de más.
    let corta = "<pre class=\"captura\" aria-hidden=\"true\">uno\ndos</pre>\n";
    assert!(primera_diferencia(base, corta).is_some());
    assert!(primera_diferencia(corta, base).is_some());
    // Solo el salto de línea final.
    let sin_salto = base.trim_end();
    assert!(primera_diferencia(base, sin_salto).is_some());
    // Un fichero que falta o no se puede leer es un fallo, no un silencio.
    let mut fallos = Vec::new();
    comparar_con_disco(
        &directorio_de_capturas().join("no-existe.html"),
        "x",
        &mut fallos,
    );
    assert_eq!(fallos.len(), 1, "{fallos:?}");
    assert!(fallos[0].contains("no-existe.html"), "{fallos:?}");
}

/// Dos pintados seguidos dan exactamente lo mismo: nada de lo que sale depende del
/// reloj (más allá de lo redondeado), del azar ni del orden de un `HashMap`.
#[test]
fn el_pintado_es_determinista() {
    fijar_zona_horaria();
    let primero = pintar_capturas();
    let segundo = pintar_capturas();
    assert_eq!(primero.len(), segundo.len());
    for (a, b) in primero.iter().zip(&segundo) {
        assert_eq!(a.id, b.id);
        if a != b {
            let (texto_a, texto_b) = (a.texto(), b.texto());
            let diferencia = primera_diferencia(&texto_a, &texto_b)
                .unwrap_or_else(|| "solo difieren los estilos".to_string());
            panic!("la captura «{}» no es determinista: {diferencia}", a.id);
        }
    }
    // Y lo exportado también.
    assert_eq!(exportar_capturas(&primero), exportar_capturas(&segundo));
}

/// La App de cada captura tiene exactamente los tres hosts del inventario de la demo.
#[test]
fn el_inventario_es_exactamente_el_de_la_demo() {
    fijar_zona_horaria();
    // El fichero de la demo, tal cual.
    assert_eq!(hosts_del_inventario_de_la_demo(), HOSTS_DEMO);
    let _carpeta = preparar_carpeta_fija();
    let vistas = vistas();
    for (id, vista) in vistas {
        let prueba = vista();
        assert_eq!(
            nombres_de_hosts(&prueba.app),
            HOSTS_DEMO,
            "la vista «{id}» no tiene exactamente los hosts de la demo"
        );
        assert_eq!(prueba.app.hosts.len(), 3, "la vista «{id}»");
        // El equipo también es inventado.
        let este = magi::almacen::equipos::este(
            magi::almacen::abrir(&prueba.app.rutas.base_datos())
                .expect("almacén")
                .conexion(),
        )
        .expect("este equipo");
        assert_eq!(este.nombre, NOMBRE_EQUIPO, "la vista «{id}»");
    }
}

/// Un host que no es de la demo (aunque se llame `demo-*`), uno de menos o uno de más
/// hacen fallar la comprobación del inventario.
#[test]
fn un_inventario_distinto_del_de_la_demo_se_rechaza() {
    fijar_zona_horaria();
    let demo = || HOSTS_DEMO.map(str::to_string).to_vec();
    assert_eq!(comprobar_nombres_de_hosts(&demo()), Ok(()));
    let mut de_mas = demo();
    de_mas.push("demo-extra".to_string());
    assert!(comprobar_nombres_de_hosts(&de_mas).is_err());
    let mut de_la_semilla = demo();
    de_la_semilla[0] = "hetzner-01".to_string();
    de_la_semilla.sort();
    assert!(comprobar_nombres_de_hosts(&de_la_semilla).is_err());
    assert!(comprobar_nombres_de_hosts(&demo()[..2]).is_err());
    assert!(comprobar_nombres_de_hosts(&[]).is_err());
}

#[test]
fn ninguna_captura_lleva_nombres_de_la_semilla_ni_datos_del_equipo() {
    fijar_zona_horaria();
    let capturas = pintar_capturas();
    let conjunto = exportar_capturas(&capturas);
    comprobar_que_se_puede_publicar(&capturas, &conjunto);
}

/// Lo que escribe `generar_capturas` pasa antes por la misma comprobación: un nombre de
/// la semilla en una captura impide generar (no queda escrito nada). Es el criterio de
/// aceptación «cuando se generan o se comprueban».
#[test]
#[should_panic(expected = "hetzner-01")]
fn generar_capturas_no_escribe_si_una_captura_lleva_un_nombre_de_la_semilla() {
    fijar_zona_horaria();
    let filas: Vec<&str> = vec!["│ demo-web  NOMINAL │"; FILAS];
    let mut capturas: Vec<CapturaCeldas> =
        IDS.iter().map(|id| captura_de_texto(id, &filas)).collect();
    let mut sucia = filas.clone();
    sucia[7] = "│ hetzner-01  NOMINAL │";
    capturas[3] = captura_de_texto(IDS[3], &sucia);
    let _ = ficheros_a_escribir(&capturas);
}

/// Con capturas limpias, `ficheros_a_escribir` da los seis HTML y `capturas.css`, en
/// `web/capturas/` y sin escribir nada.
#[test]
fn ficheros_a_escribir_son_los_seis_html_y_la_css() {
    fijar_zona_horaria();
    let filas: Vec<&str> = vec!["│ demo-web  NOMINAL │"; FILAS];
    let capturas: Vec<CapturaCeldas> = IDS.iter().map(|id| captura_de_texto(id, &filas)).collect();
    let ficheros = ficheros_a_escribir(&capturas);
    let nombres: Vec<String> = ficheros
        .iter()
        .map(|(ruta, _)| {
            assert_eq!(ruta.parent(), Some(directorio_de_capturas().as_path()));
            ruta.file_name().unwrap().to_string_lossy().into_owned()
        })
        .collect();
    let mut esperados: Vec<String> = IDS.iter().map(|id| format!("{id}.html")).collect();
    esperados.push("capturas.css".to_string());
    assert_eq!(nombres, esperados);
    assert!(ficheros
        .iter()
        .all(|(_, contenido)| contenido.ends_with('\n')));
}

/// El criterio de aceptación: un nombre de la semilla en una captura hace fallar la
/// comprobación (a mano, con una captura sintética; la real no lo lleva).
#[test]
fn una_captura_con_un_nombre_de_la_semilla_falla() {
    fijar_zona_horaria();
    let nombres = nombres_de_la_semilla();
    let limpia = "┌ MAGI · FLOTA ─┐\n│▸ ● demo-web  NOMINAL │\n│  ● demo-datos  NOMINAL │";
    assert_eq!(
        contiene_nombres_de_la_semilla(limpia, &nombres),
        Vec::<String>::new()
    );
    assert_eq!(comprobar_texto_publicable(limpia, &nombres), Ok(()));

    let sucia = "│▸ ● hetzner-01  NOMINAL │\n│  ● backup-nas  CARGA │\n│  ● hetzner-01 ⚠2 │";
    assert_eq!(
        contiene_nombres_de_la_semilla(sucia, &nombres),
        vec!["backup-nas".to_string(), "hetzner-01".to_string()],
        "ordenados y sin repetir"
    );
    let error = comprobar_texto_publicable(sucia, &nombres).expect_err("debe fallar");
    assert!(
        error.contains("hetzner-01") && error.contains("backup-nas"),
        "{error}"
    );

    // Un solo nombre, pegado a otras letras, en cualquier fila, también falla.
    let escondida = format!("{}\n│ x{}y │", "│ demo-web │", "rincon-dev");
    assert!(comprobar_texto_publicable(&escondida, &nombres).is_err());

    // Y a través de una captura de celdas, como la que llega de la TUI.
    let captura = captura_de_texto("flota", &["│ mac-mini-m1  CARGA │"]);
    assert!(comprobar_texto_publicable(&captura.texto(), &nombres).is_err());
    let captura = captura_de_texto("flota", &["│ demo-copias  CARGA │"]);
    assert_eq!(
        comprobar_texto_publicable(&captura.texto(), &nombres),
        Ok(())
    );
}

/// El lector de literales de la semilla: lo que cuenta, lo que no y los bordes.
#[test]
fn el_lector_de_la_semilla_solo_toma_literales_con_forma_de_nombre() {
    fijar_zona_horaria();
    let codigo = r#"
        let a = "hetzner-01";
        let b = "ssh-ed25519";
        let c = "6b1e2f3a-4c5d-4e6f-8a7b-9c0d1e2f3a41";
        let d = "sin_guion";
        let e = "Mayusculas-Dentro";
        let f = "doble--guion";
        let g = "-empieza";
        let h = "termina-";
        let i = "uno solo";
        let j = ("mac-mini", "otro-nombre-largo-2");
        let k = "sin cerrar-comilla
        let l = "ñ-acento";
    "#;
    assert_eq!(
        nombres_en_literales(codigo),
        vec![
            "hetzner-01".to_string(),
            "mac-mini".to_string(),
            "otro-nombre-largo-2".to_string()
        ]
    );
    assert!(nombres_en_literales("").is_empty());
    assert!(nombres_en_literales("\"").is_empty());
    assert!(nombres_en_literales("\"a-b").is_empty());
    assert!(es_uuid("6b1e2f3a-4c5d-4e6f-8a7b-9c0d1e2f3a41"));
    assert!(!es_uuid("6b1e2f3a-4c5d-4e6f-8a7b-9c0d1e2f3a4"));
    assert!(!es_uuid("hetzner-01"));
}

/// Los datos del equipo se buscan como palabra: «sobremesa» no es «mesa», y un nombre
/// corto no es motivo para fallar.
#[test]
fn la_busqueda_de_datos_del_equipo_es_por_palabras() {
    fijar_zona_horaria();
    assert!(contiene_palabra("equipo: sobremesa ✓", "sobremesa"));
    assert!(!contiene_palabra("sobremesa-2", "sobremesa"));
    assert!(!contiene_palabra("mi.sobremesa", "sobremesa"));
    assert!(!contiene_palabra("sobremesas", "sobremesa"));
    assert!(contiene_palabra("sobremesa", "sobremesa"));
    assert!(contiene_palabra("a sobremesa", "sobremesa"));
    assert!(!contiene_palabra("nada", ""));
    // Una ruta de /home que no es la de documentación no sale.
    assert!(comprobar_texto_publicable("/home/usuario/web", &[]).is_ok());
    // (La ruta ajena se compone aquí: la prueba del repositorio público vigila
    // el árbol entero, también este fichero, y no distingue un caso negativo.)
    let ajena = ["/home/", "maria", "/web"].concat();
    assert!(comprobar_texto_publicable(&ajena, &[]).is_err());
    assert!(
        comprobar_texto_publicable("v1.6.1", &[]).is_err()
            == "v1.6.1".contains(env!("CARGO_PKG_VERSION"))
    );
    assert!(comprobar_texto_publicable("/tmp/magicapXYZ/datos", &[]).is_err());
}

#[test]
fn cada_vista_tiene_su_fichero_y_hay_capturas_css() {
    fijar_zona_horaria();
    let directorio = directorio_de_capturas();
    // Lo que debe salir en cada captura: la vista de verdad, no un resto del contrato.
    let marcas: [(&str, &[&str]); 6] = [
        (
            "flota",
            &["MAGI · FLOTA", "demo-web", "demo-datos", "demo-copias"],
        ),
        (
            "sesion-paneles",
            &["SESIÓN", "usuario@demo-web:~$", "usuario@demo-datos:~$"],
        ),
        (
            "archivos",
            &[
                "/tmp/magi-capturas-web/hogar",
                "/var/www/demo-web",
                "main.py",
            ],
        ),
        (
            "tuneles",
            &["TÚNELES", "caído", "connect: conexión rechazada"],
        ),
        (
            "deliberacion",
            &[
                "DELIBERACIÓN MAGI",
                "MELCHIOR-1",
                "BALTHASAR-2",
                "CASPER-3",
                "BLOQUEADO",
            ],
        ),
        (
            "servicios",
            &["MAGI · SERVICIOS", "nginx.service", "docker 27.3"],
        ),
    ];
    assert_eq!(marcas.map(|(id, _)| id), IDS);
    for (id, marcas) in marcas {
        let ruta = directorio.join(format!("{id}.html"));
        let contenido = fs::read_to_string(&ruta).unwrap_or_else(|e| {
            panic!(
                "falta {}: {e} (scripts/generar-capturas.sh)",
                ruta.display()
            )
        });
        assert!(
            contenido.ends_with("</pre>\n") && !contenido.ends_with("\n\n"),
            "{id}.html acaba en el </pre> y un solo salto de línea"
        );
        validar_fragmento(contenido.trim_end())
            .unwrap_or_else(|e| panic!("{id}.html no es un fragmento válido: {e}"));
        let texto = texto_de_fragmento(&contenido);
        assert_eq!(
            texto.trim_end_matches('\n').split('\n').count(),
            FILAS,
            "{id}.html tiene {FILAS} filas"
        );
        for marca in marcas {
            assert!(texto.contains(marca), "{id}.html no lleva «{marca}»");
        }
        assert!(
            !texto.contains("contrato S2"),
            "{id}.html es un resto del contrato, no una captura"
        );
    }
    let ruta = directorio.join("capturas.css");
    let css = fs::read_to_string(&ruta).unwrap_or_else(|e| {
        panic!(
            "falta {}: {e} (scripts/generar-capturas.sh)",
            ruta.display()
        )
    });
    validar_css(&css).unwrap_or_else(|e| panic!("capturas.css no es válido: {e}"));
    assert!(
        css.starts_with(".captura{"),
        "capturas.css empieza por la regla .captura"
    );
    // Cada clase que usa un fragmento está definida, y ninguna sobra.
    let mut usadas = BTreeSet::new();
    for id in IDS {
        let contenido = fs::read_to_string(directorio.join(format!("{id}.html"))).expect("captura");
        for trozo in contenido.split("class=\"c").skip(1) {
            let numero: String = trozo.chars().take_while(char::is_ascii_digit).collect();
            // `class="captura"` también empieza por «c»: no es una clase de estilo.
            if !numero.is_empty() {
                usadas.insert(format!(".c{numero}{{"));
            }
        }
    }
    let definidas: BTreeSet<String> = css
        .lines()
        .filter(|linea| linea.starts_with(".c") && !linea.starts_with(".captura"))
        .filter_map(|linea| {
            linea
                .split_once('{')
                .map(|(selector, _)| format!("{selector}{{"))
        })
        .collect();
    assert_eq!(
        usadas, definidas,
        "las clases de las capturas y las de capturas.css"
    );
    // Ni capturas de más ni restos: solo lo que usa la portada.
    let mut ficheros: Vec<String> = fs::read_dir(&directorio)
        .expect("web/capturas")
        .filter_map(Result::ok)
        .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
        .collect();
    ficheros.sort();
    let mut esperados: Vec<String> = IDS.iter().map(|id| format!("{id}.html")).collect();
    esperados.push("capturas.css".to_string());
    esperados.sort();
    assert_eq!(ficheros, esperados, "los ficheros de web/capturas");
}

/// Todo carácter de lo que pinta la TUI tiene glifo en JetBrains Mono o en el
/// subconjunto de símbolos, o se degrada con `texto_ascii` a uno que sí lo tiene.
#[test]
fn todo_caracter_de_las_capturas_tiene_glifo_o_se_degrada() {
    fijar_zona_horaria();
    let cobertura = cobertura();
    let capturas = pintar_capturas();
    let mut degradados = BTreeSet::new();
    for captura in &capturas {
        for (y, fila) in captura.filas.iter().enumerate() {
            for (x, celda) in fila.iter().enumerate() {
                if cobertura.cubre(&celda.simbolo) {
                    continue;
                }
                let degradado = magi::ui::disposicion::texto_ascii(&celda.simbolo);
                assert!(
                    degradado.chars().count() == 1
                        && degradado != "?"
                        && cobertura.cubre(&degradado),
                    "«{}» (fila {}, columna {}) de la captura «{}» no tiene glifo y su degradación «{degradado}» no vale",
                    celda.simbolo,
                    y + 1,
                    x + 1,
                    captura.id
                );
                degradados.insert((celda.simbolo.clone(), degradado));
            }
        }
    }
    // Lo exportado también: cada carácter del texto de los ficheros tiene glifo.
    let conjunto = exportar_capturas(&capturas);
    for (id, fragmento) in &conjunto.fragmentos {
        for caracter in texto_de_fragmento(fragmento).chars() {
            assert!(
                caracter.is_whitespace() || cobertura.cubre(&caracter.to_string()),
                "U+{:04X} de la captura «{id}» no tiene glifo",
                u32::from(caracter)
            );
        }
    }
    // Lo versionado tampoco puede traer uno sin glifo.
    for id in IDS {
        let contenido = fs::read_to_string(directorio_de_capturas().join(format!("{id}.html")))
            .unwrap_or_else(|e| panic!("falta {id}.html: {e}"));
        for caracter in texto_de_fragmento(&contenido).chars() {
            assert!(
                caracter.is_whitespace() || cobertura.cubre(&caracter.to_string()),
                "U+{:04X} de {id}.html no tiene glifo",
                u32::from(caracter)
            );
        }
    }
    eprintln!("símbolos degradados en las capturas: {degradados:?}");
}

/// Un carácter sin glifo y sin degradación posible hace fallar la exportación (no se
/// cuela un cuadrado vacío en la web).
#[test]
fn un_caracter_sin_glifo_ni_degradacion_hace_fallar_la_exportacion() {
    fijar_zona_horaria();
    let cobertura = cobertura();
    // Un muñeco de nieve: ni está en las fuentes ni `texto_ascii` lo traduce.
    assert!(!cobertura.cubre("☃"));
    assert_eq!(magi::ui::disposicion::texto_ascii("☃"), "?");
    let captura = captura_de_texto("flota", &["abc ☃ def"]);
    let error = exportar(&[captura], &cobertura, &magi::ui::disposicion::texto_ascii)
        .expect_err("el muñeco de nieve no tiene glifo");
    assert!(error.to_string().contains("U+2603"), "{error}");
    // Uno que sí tiene degradación, y uno de la TUI que solo está en el subconjunto.
    assert!(cobertura.cubre("↵"), "↵ viene del subconjunto de símbolos");
    assert!(cobertura.cubre("▸") && cobertura.cubre("●") && cobertura.cubre("█"));
    let captura = captura_de_texto("flota", &["▸ ● ↵ ⇄ ◐ ✕ ✓ ⚠ ★"]);
    exportar(&[captura], &cobertura, &magi::ui::disposicion::texto_ascii)
        .expect("los glifos de la TUI tienen fuente");
}

/// Una captura de una sola fila por línea de texto, con el estilo por defecto.
fn captura_de_texto(id: &str, lineas: &[&str]) -> CapturaCeldas {
    let celda = |simbolo: &str| CeldaCaptura {
        simbolo: simbolo.to_string(),
        fg: ColorCaptura::Reset,
        bg: ColorCaptura::Reset,
        negrita: false,
        tenue: false,
        cursiva: false,
        subrayado: false,
        invertido: false,
        oculta: false,
    };
    CapturaCeldas {
        id: id.to_string(),
        filas: lineas
            .iter()
            .map(|linea| linea.chars().map(|c| celda(&c.to_string())).collect())
            .collect(),
    }
}

/// El adaptador no emite la celda que continúa un carácter ancho: el navegador pinta
/// el carácter en dos columnas y la fila saldría desplazada.
#[test]
fn el_adaptador_salta_las_celdas_de_continuacion() {
    fijar_zona_horaria();
    use ratatui::layout::Rect;
    use ratatui::style::Style;
    let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 2));
    buffer.set_string(0, 0, "a世b", Style::default());
    buffer.set_string(0, 1, "世界", Style::default().fg(Color::Red));
    // La celda de continuación de «世» es la 2: ratatui la deja con un espacio.
    assert_eq!(buffer[(1, 0)].symbol(), "世");
    assert_eq!(buffer[(2, 0)].symbol(), " ");
    let captura = celdas_de("flota", &buffer).expect("celdas");
    let simbolos = |fila: usize| -> Vec<&str> {
        captura.filas[fila]
            .iter()
            .map(|celda| celda.simbolo.as_str())
            .collect()
    };
    // «a», «世» (2 columnas), «b» y los blancos: 8 columnas, 7 celdas.
    assert_eq!(simbolos(0), ["a", "世", "b", " ", " ", " ", " "]);
    // Dos anchos seguidos: 4 columnas, 2 celdas, y el resto en blanco.
    assert_eq!(simbolos(1), ["世", "界", " ", " ", " ", " "]);
    assert_eq!(captura.texto(), "a世b\n世界");
    // La que sí se emite conserva su color.
    assert_eq!(captura.filas[1][0].fg, ColorCaptura::Indexado(1));
    // Y exportada, la fila no lleva espacios de continuación en medio. JetBrains Mono
    // no tiene ideogramas: la prueba pone los dos en una cobertura propia.
    let cobertura = Cobertura::desde_codigos(BTreeSet::from([0x4E16, 0x754C, 0x61, 0x62]));
    let conjunto = exportar(&[captura], &cobertura, &magi::ui::disposicion::texto_ascii)
        .expect("exportar con ideogramas");
    let texto = texto_de_fragmento(&conjunto.fragmentos["flota"]);
    assert_eq!(texto, "a世b\n世界");
    // Sin esa cobertura, el ideograma es un error y no un cuadrado vacío.
    let captura = celdas_de("flota", &buffer).expect("celdas");
    assert!(exportar(
        &[captura],
        &self::cobertura(),
        &magi::ui::disposicion::texto_ascii
    )
    .is_err());
    // Si lo que debería ser la continuación lleva algo, no se salta en silencio: la
    // generación falla con la celda.
    let mut roto = Buffer::empty(Rect::new(0, 0, 4, 1));
    roto.set_string(0, 0, "世", Style::default());
    roto[(1, 0)].set_symbol("z");
    let error = celdas_de("flota", &roto).expect_err("continuación con contenido");
    assert!(
        error.contains("(1, 0)") && error.contains("\"z\""),
        "{error}"
    );
}

/// Colores y modificadores del adaptador: el mapeo de ratatui a `ColorCaptura`, y los
/// modificadores que no sabe decir son un error (no una captura que miente).
#[test]
fn el_adaptador_traduce_colores_y_modificadores_y_rechaza_lo_que_no_sabe() {
    fijar_zona_horaria();
    use ratatui::layout::Rect;
    use ratatui::style::Style;
    let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 1));
    let estilos = [
        Style::default()
            .fg(Color::LightCyan)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
        Style::default()
            .fg(Color::Rgb(1, 2, 3))
            .bg(Color::Indexed(200))
            .add_modifier(Modifier::DIM | Modifier::UNDERLINED),
        Style::default().add_modifier(Modifier::REVERSED | Modifier::HIDDEN),
    ];
    for (columna, estilo) in estilos.into_iter().enumerate() {
        buffer.set_string(columna as u16, 0, "x", estilo);
    }
    let captura = celdas_de("flota", &buffer).expect("celdas");
    let fila = &captura.filas[0];
    assert_eq!(fila[0].fg, ColorCaptura::Indexado(14));
    assert_eq!(fila[0].bg, ColorCaptura::Indexado(8));
    assert!(fila[0].negrita && fila[0].cursiva && !fila[0].tenue);
    assert_eq!(fila[1].fg, ColorCaptura::Rgb(1, 2, 3));
    assert_eq!(fila[1].bg, ColorCaptura::Indexado(200));
    assert!(fila[1].tenue && fila[1].subrayado && !fila[1].negrita);
    assert!(fila[2].invertido && fila[2].oculta);
    assert_eq!(
        (fila[3].fg, fila[3].bg),
        (ColorCaptura::Reset, ColorCaptura::Reset)
    );
    // Los 16 colores con nombre, en el orden de la paleta ANSI.
    for (indice, color) in [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(color_de(color), Ok(ColorCaptura::Indexado(indice as u8)));
    }
    // Un parpadeo o un tachado no se pueden pintar: la generación falla con la celda.
    for modificador in [
        Modifier::SLOW_BLINK,
        Modifier::RAPID_BLINK,
        Modifier::CROSSED_OUT,
    ] {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 1));
        buffer.set_string(
            1,
            0,
            "y",
            Style::default().add_modifier(Modifier::BOLD | modificador),
        );
        let error = celdas_de("flota", &buffer).expect_err("modificador sin soporte");
        assert!(error.contains("(1, 0)"), "{error}");
    }
}

/// Lo versionado no lleva `style=` ni más etiquetas que `<pre>` y `<span>` con sus
/// atributos de siempre, y la CSS solo trae reglas de color: nada que ejecute o cargue.
/// Es una comprobación aparte de `validar_fragmento` y `validar_css`.
#[test]
fn los_ficheros_generados_no_llevan_style_ni_etiquetas_ajenas() {
    fijar_zona_horaria();
    let directorio = directorio_de_capturas();
    for id in IDS {
        let ruta = directorio.join(format!("{id}.html"));
        let contenido =
            fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("falta {}: {e}", ruta.display()));
        comprobar_solo_pre_y_span(id, &contenido)
            .unwrap_or_else(|error| panic!("{id}.html: {error}"));
    }
    let ruta = directorio.join("capturas.css");
    let css = fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("falta {}: {e}", ruta.display()));
    comprobar_css_solo_de_color(&css).unwrap_or_else(|error| panic!("capturas.css: {error}"));

    // Lo que esta comprobación detecta (para que no se quede ciega).
    let bueno = "<pre class=\"captura\" aria-hidden=\"true\">a<span class=\"c1\">b</span></pre>";
    assert_eq!(comprobar_solo_pre_y_span("x", bueno), Ok(()));
    for malo in [
        "<pre class=\"captura\" aria-hidden=\"true\"><span style=\"color:red\">b</span></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><span class=\"c1\" style=\"x\">b</span></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><b>b</b></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><script>x</script></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><img src=x></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\" onclick=\"x\">b</pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><span class=\"c1\" onmouseover=\"x\">b</span></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><SPAN STYLE=\"x\">b</SPAN></pre>",
        "<pre class=\"captura\" aria-hidden=\"true\">a < b</pre>",
        "<pre class=\"captura\" aria-hidden=\"true\">STYLE=x</pre>",
        "<pre class=\"captura\" aria-hidden=\"true\"><!-- x --></pre>",
        "<pre class=\"captura\">sin aria</pre>",
    ] {
        assert!(
            comprobar_solo_pre_y_span("x", malo).is_err(),
            "debía rechazar {malo}"
        );
    }
    assert_eq!(
        comprobar_css_solo_de_color(".captura{color:#fff}\n.c1{color:#000}\n"),
        Ok(())
    );
    for malo in [
        ".c1{background:url(x)}\n",
        "@import 'x';\n",
        ".c1{color:red}\nbody{color:red}\n",
        ".c1{color:expression(x)}\n",
        ".c1{color:red}</style>\n",
        ".c1{color:\\72 ed}\n",
        ".c1{color:red}\n/* x */\n",
        ".c1{color:red;{x}}\n",
    ] {
        assert!(
            comprobar_css_solo_de_color(malo).is_err(),
            "debía rechazar {malo}"
        );
    }
}

/// Un lector aparte de `html::tokenizar_detallado`: el HTML de una captura solo trae
/// `<pre class="captura" aria-hidden="true">` y `<span class="c<N>">`, y ningún texto
/// lleva `<`, `style` ni un manejador de eventos.
fn comprobar_solo_pre_y_span(id: &str, html: &str) -> Result<(), String> {
    let minusculas = html.to_ascii_lowercase();
    for prohibido in [
        "style",
        "<script",
        "<!--",
        "javascript:",
        "onclick",
        "onmouse",
        "onload",
        "onerror",
    ] {
        if minusculas.contains(prohibido) {
            return Err(format!("«{id}» lleva «{prohibido}»"));
        }
    }
    let mut resto = html.trim_end();
    let mut abiertos = 0i32;
    let mut pre_abierto = false;
    while let Some(inicio) = resto.find('<') {
        // Entre etiquetas, solo texto: ni un `>` suelto cuenta como etiqueta.
        let desde = &resto[inicio + 1..];
        let Some(fin) = desde.find('>') else {
            return Err("una etiqueta sin cerrar".to_string());
        };
        let etiqueta = &desde[..fin];
        if etiqueta.contains('<') {
            return Err(format!("un `<` suelto dentro de «<{etiqueta}»"));
        }
        match etiqueta {
            "pre class=\"captura\" aria-hidden=\"true\"" if !pre_abierto => pre_abierto = true,
            "/pre" if pre_abierto && abiertos == 0 => pre_abierto = false,
            "/span" if abiertos == 1 => abiertos -= 1,
            _ => {
                let es_span = etiqueta
                    .strip_prefix("span class=\"c")
                    .and_then(|r| r.strip_suffix('"'))
                    .is_some_and(|n| {
                        (1..=9).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit())
                    });
                if es_span && abiertos == 0 && pre_abierto {
                    abiertos += 1;
                } else {
                    return Err(format!("una etiqueta ajena o mal colocada: «<{etiqueta}>»"));
                }
            }
        }
        resto = &desde[fin + 1..];
    }
    if pre_abierto || abiertos != 0 {
        return Err("etiquetas sin cerrar".to_string());
    }
    if resto.contains('>') {
        return Err("un `>` suelto fuera de una etiqueta".to_string());
    }
    Ok(())
}

/// Un lector aparte de `validar_css`: solo reglas `.captura{…}` y `.c<N>{…}`, una por
/// línea, con propiedades de la lista y sin nada que cargue (`url`, `@`, `\`, comentarios).
fn comprobar_css_solo_de_color(css: &str) -> Result<(), String> {
    const PROPIEDADES: [&str; 17] = [
        "background",
        "color",
        "font-weight",
        "font-style",
        "text-decoration",
        "opacity",
        "font-family",
        "font-variant-ligatures",
        "font-feature-settings",
        "line-height",
        "white-space",
        "overflow-x",
        "margin",
        "padding",
        "border",
        "border-radius",
        "text-decoration-line",
    ];
    for prohibido in [
        "url(",
        "@",
        "\\",
        "/*",
        "expression",
        "javascript",
        "<",
        "import",
        "{{",
    ] {
        if css.to_ascii_lowercase().contains(prohibido) {
            return Err(format!("lleva «{prohibido}»"));
        }
    }
    if css.is_empty() || !css.ends_with('\n') {
        return Err("no acaba en salto de línea".to_string());
    }
    for (numero, linea) in css.lines().enumerate() {
        let numero = numero + 1;
        let Some((selector, cuerpo)) = linea.split_once('{') else {
            return Err(format!("línea {numero}: no es una regla"));
        };
        let Some(cuerpo) = cuerpo.strip_suffix('}') else {
            return Err(format!(
                "línea {numero}: la regla no cierra al final de la línea"
            ));
        };
        if cuerpo.contains('{') || cuerpo.contains('}') {
            return Err(format!("línea {numero}: llaves anidadas"));
        }
        let clase = selector.strip_prefix('.').unwrap_or("");
        let es_clase_de_estilo = clase
            .strip_prefix('c')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if clase != "captura" && !es_clase_de_estilo {
            return Err(format!(
                "línea {numero}: selector «{selector}» no permitido"
            ));
        }
        for declaracion in cuerpo.split(';').filter(|d| !d.is_empty()) {
            let Some((propiedad, _)) = declaracion.split_once(':') else {
                return Err(format!("línea {numero}: declaración sin «:»"));
            };
            if !PROPIEDADES.contains(&propiedad) {
                return Err(format!(
                    "línea {numero}: propiedad «{propiedad}» no permitida"
                ));
            }
        }
    }
    Ok(())
}
