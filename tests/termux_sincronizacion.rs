//! La sincronización del inventario en Termux (Fase 17, Sprint 2): la carpeta
//! se guarda resuelta con `realpath`, se sondea con una firma barata en vez de
//! usar `notify`, un `chmod` que el almacenamiento compartido no admite no es
//! un error, y el equipo de Termux (`termux-<modelo>`, plataforma `android`)
//! escribe un fichero que el escritorio y el móvil leen como el de cualquier
//! equipo, sin que el núcleo decida nada por la plataforma.
//!
//! `es_termux()` se decide una vez por proceso: todas las pruebas de este
//! binario empiezan por `termux()`, que pone `MAGI_PLATAFORMA=termux` antes de
//! que nadie pregunte. La lógica que no depende del proceso (el sondeo, las
//! pasadas con permisos explícitos) se prueba con parámetros, no con la
//! variable.
//!
//! Todo el fichero es solo de depuración (`debug_assertions`), que es donde
//! `MAGI_PLATAFORMA=termux` vale (ver `termux_plataforma.rs`).
#![cfg(debug_assertions)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use magi::almacen::{equipos as equipos_bd, sondeos, Almacen};
use magi::app::sincronizacion::{
    debe_pedir_pasada_al_volver, firma_carpeta, lectura_de_carpeta_colgada, sondear, sondear_con,
    vigilar_carpeta, EventoInventario, FirmaCarpeta, LectorFirma, SEGURIDAD_CARPETA_TERMUX,
};
use magi::app::{App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::deliberacion::comprobaciones::{
    lanzar, Futuro, Limites, NoDisponible, Proveedores, ResultadoComprobacion, Trabajo,
};
use magi::deliberacion::{
    Comprobacion, ComprobacionesHost, NuevaDeliberacion, ResultadoDeliberacion, Veredicto,
};
use magi::escritorio::TipoHost;
use magi::llavero::{LlaveroEnMemoria, LlaveroSecreto, CUENTA_SINCRONIZACION};
use magi::modelo::{DatosHost, Origen, Sondeo};
use magi::rutas::Equipo;
use magi::sincronizacion::almacenamiento::{resolver_carpeta, INSTRUCCION_ALMACENAMIENTO};
use magi::sincronizacion::cifrado::ClaveSincronizacion;
use magi::sincronizacion::lector::{self, EstadoLectura};
use magi::sincronizacion::{
    carpeta, equipos, formato, pasada_con_aviso, pasada_con_aviso_y_ajuste,
    permisos_de_esta_plataforma, AvisoUnico, BorradosPendientes, EstadoPasada, OpcionesPasada,
    PermisosCarpeta, PoliticaBorrados, ResultadoPasada, ResumenImportacion,
    AVISO_PERMISOS_IGNORADOS,
};
use magi::tema::Tema;
use magi::termux::{es_termux, Propiedades};
use magi::ui::componentes::CampoTexto;
use magi::ui::Vista;
use magi_nucleo::flota::{aplicar_metricas, evaluar, parsear, Umbrales};
use magi_nucleo::sftp_lectura::{EntradaListado, TipoEntradaListado};
use magi_nucleo::trabajo_vivo::TrabajoVivo;

/// Pone `MAGI_PLATAFORMA=termux` antes de la primera pregunta de este proceso.
fn termux() {
    static UNA: Once = Once::new();
    UNA.call_once(|| std::env::set_var("MAGI_PLATAFORMA", "termux"));
    assert!(
        es_termux(),
        "es_termux() ya se decidió sin MAGI_PLATAFORMA=termux en este proceso"
    );
}

// ------------------------------------------------------------------ entorno

/// Un MAGI de prueba con su magi.db (con la identidad de equipo que se pida),
/// su configuración y la clave de sincronización.
struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    config: Config,
    texto_clave: String,
}

impl Entorno {
    fn nuevo(carpeta: &Path, nombre: &str, plataforma: &str, texto_clave: &str) -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let raiz = temporal.path();
        let rutas = Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        };
        std::fs::create_dir_all(rutas.dir_ssh()).expect("creando ~/.ssh");
        let almacen = Almacen::abrir(
            &rutas.base_datos(),
            &Equipo::nuevo(nombre, plataforma),
            "1.5.0",
        )
        .expect("creando magi.db");
        let mut config = Config::default();
        config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
        config.sincronizacion.copias = 5;
        // Umbrales altos: lo que no va de borrados masivos no abre diálogos.
        config.sincronizacion.umbral_borrado_hosts = 1000;
        config.sincronizacion.umbral_borrado_pct = 1000;
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            config,
            texto_clave: texto_clave.to_string(),
        }
    }

    fn claves(&self) -> Vec<ClaveSincronizacion> {
        vec![ClaveSincronizacion::desde_texto(&self.texto_clave).expect("clave de prueba")]
    }

    fn conexion(&self) -> &rusqlite::Connection {
        self.almacen.conexion()
    }

    /// Una pasada con los permisos que se pidan y un aviso propio.
    fn pasada_con(&self, permisos: PermisosCarpeta) -> ResultadoPasada {
        pasada_con_aviso(
            self.conexion(),
            &self.rutas,
            &self.config,
            &self.claves(),
            &OpcionesPasada::sin_trabajo_vivo(PoliticaBorrados::Preguntar),
            permisos,
            &AvisoUnico::nuevo(),
        )
        .expect("la pasada falla")
    }

    /// Una pasada con un `chmod` que se inyecta y un aviso que se comparte.
    fn pasada_con_ajuste(
        &self,
        permisos: PermisosCarpeta,
        aviso: &AvisoUnico,
        ajustar: &dyn Fn(&Path, u32) -> io::Result<()>,
    ) -> ResultadoPasada {
        pasada_con_aviso_y_ajuste(
            self.conexion(),
            &self.rutas,
            &self.config,
            &self.claves(),
            &OpcionesPasada::sin_trabajo_vivo(PoliticaBorrados::Preguntar),
            permisos,
            aviso,
            ajustar,
        )
        .expect("la pasada falla")
    }

    fn crear_host(&self, nombre: &str, direccion: &str) -> i64 {
        self.almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: direccion.to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("creando el host")
    }

    fn crear_host_rdp(&self, nombre: &str, direccion: &str) -> i64 {
        self.almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: direccion.to_string(),
                    puerto: 3389,
                    tipo: TipoHost::Rdp,
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("creando el host RDP")
    }

    fn host_id(&self, nombre: &str) -> i64 {
        self.almacen
            .listar_hosts()
            .expect("listando hosts")
            .into_iter()
            .find(|host| host.nombre == nombre)
            .unwrap_or_else(|| panic!("no hay host {nombre}"))
            .id
    }

    fn editar_direccion(&self, nombre: &str, direccion: &str) {
        let id = self.host_id(nombre);
        self.almacen
            .actualizar_host(
                id,
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: direccion.to_string(),
                    ..DatosHost::default()
                },
            )
            .expect("editando el host");
    }

    fn borrar_host(&self, nombre: &str) {
        let id = self.host_id(nombre);
        self.almacen.borrar_host(id).expect("borrando el host");
    }

    fn hosts(&self) -> Vec<(String, String)> {
        let mut hosts: Vec<(String, String)> = self
            .almacen
            .listar_hosts()
            .expect("listando hosts")
            .into_iter()
            .map(|host| (host.nombre, host.direccion))
            .collect();
        hosts.sort();
        hosts
    }

    fn este(&self) -> equipos_bd::Equipo {
        equipos_bd::este(self.conexion()).expect("este equipo")
    }

    fn fichero_propio(&self, carpeta: &Path) -> PathBuf {
        carpeta.join(carpeta::nombre_fichero(&self.este().uuid))
    }
}

/// Un dispositivo sin trabajo vivo (el lector del móvil lo pide).
struct SinTrabajo;

impl TrabajoVivo for SinTrabajo {
    fn tiene_trabajo(&self, _tipo: &str, _uuid: &str) -> bool {
        false
    }
}

fn carpeta_compartida() -> (tempfile::TempDir, PathBuf) {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let carpeta = temporal.path().join("sync");
    std::fs::create_dir_all(&carpeta).expect("creando la carpeta compartida");
    (temporal, carpeta)
}

/// El documento descifrado del fichero de un equipo, con el `formato` de su
/// cabecera.
fn documento_de(
    carpeta: &Path,
    uuid: &str,
    clave: &ClaveSincronizacion,
) -> (formato::Documento, u32) {
    let bytes = carpeta::leer(&carpeta.join(carpeta::nombre_fichero(uuid))).expect("el fichero");
    let plano = clave.descifrar(&bytes).expect("descifrando");
    let en_claro = formato::separar(&plano).expect("separando");
    assert!(clave.verificar(en_claro.json.as_bytes(), &en_claro.firma));
    let documento = formato::parsear(&en_claro.json).expect("parseando");
    (documento, en_claro.formato)
}

fn texto_clave() -> String {
    ClaveSincronizacion::generar().texto().to_string()
}

fn es_root() -> bool {
    nix::unistd::Uid::current().is_root()
}

// ------------------------------------------------------------------- captura

/// Escritor que guarda lo que se loguea, para contar líneas.
#[derive(Clone, Default)]
struct Captura(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captura {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Ejecuta `cuerpo` con un suscriptor propio (solo de este hilo) y devuelve
/// las líneas de log que dejó.
fn con_log<T>(cuerpo: impl FnOnce() -> T) -> (T, Vec<String>) {
    let captura = Captura::default();
    let escritor = captura.clone();
    let suscriptor = tracing_subscriber::fmt()
        .with_writer(move || escritor.clone())
        .with_ansi(false)
        .finish();
    let resultado = tracing::subscriber::with_default(suscriptor, cuerpo);
    let texto = String::from_utf8(captura.0.lock().unwrap().clone()).expect("log en UTF-8");
    (resultado, texto.lines().map(str::to_string).collect())
}

fn chmod_que_falla(_ruta: &Path, _modo: u32) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::PermissionDenied))
}

// ====================================================================
// A. La carpeta se guarda resuelta (§4.3, §7.10)
// ====================================================================

/// Un «Termux» de mentira: `home/storage/shared` apunta a `emulado/0`, como
/// `termux-setup-storage` hace con `/storage/emulated/0`.
struct Almacenamiento {
    _temporal: tempfile::TempDir,
    hogar: PathBuf,
    /// `emulado/0/Sync/magi`, ya existente.
    carpeta_real: PathBuf,
}

fn almacenamiento() -> Almacenamiento {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    // El directorio temporal puede colgar de un enlace (/tmp en macOS): lo
    // que se espera es lo que dice `realpath`.
    let raiz = std::fs::canonicalize(temporal.path()).expect("realpath de la raíz");
    let hogar = raiz.join("home");
    let real = raiz.join("emulado").join("0");
    let carpeta_real = real.join("Sync").join("magi");
    std::fs::create_dir_all(&carpeta_real).expect("carpeta real");
    std::fs::create_dir_all(hogar.join("storage")).expect("~/storage");
    std::os::unix::fs::symlink(&real, hogar.join("storage").join("shared")).expect("enlace");
    Almacenamiento {
        _temporal: temporal,
        hogar,
        carpeta_real,
    }
}

#[test]
fn en_termux_la_carpeta_se_resuelve_con_realpath() {
    termux();
    let termux = almacenamiento();
    let resuelta = resolver_carpeta("~/storage/shared/Sync/magi", &termux.hogar, true)
        .expect("la carpeta vale");
    assert_eq!(resuelta, termux.carpeta_real, "se guarda la ruta resuelta");
    assert!(
        !resuelta.to_string_lossy().contains("storage"),
        "sin el enlace de ~/storage: {}",
        resuelta.display()
    );
    // También con la ruta ya absoluta y con espacios alrededor.
    let texto = format!(
        "  {}  ",
        termux.hogar.join("storage/shared/Sync/magi").display()
    );
    assert_eq!(
        resolver_carpeta(&texto, &termux.hogar, true).unwrap(),
        termux.carpeta_real
    );
}

#[test]
fn sin_storage_la_instruccion_es_termux_setup_storage() {
    termux();
    let temporal = tempfile::tempdir().unwrap();
    let hogar = temporal.path().join("home");
    std::fs::create_dir_all(&hogar).unwrap();
    for texto in [
        "~/storage/shared/Sync/magi",
        "~/storage",
        "~/storage/downloads/magi",
    ] {
        let error = resolver_carpeta(texto, &hogar, true).unwrap_err();
        assert!(
            error.contains("ejecuta termux-setup-storage y vuelve"),
            "{texto}: {error}"
        );
        assert!(error.contains(INSTRUCCION_ALMACENAMIENTO));
    }
}

#[test]
fn en_termux_una_carpeta_fuera_de_storage_no_pide_termux_setup_storage() {
    termux();
    // §7.11: dentro del home de Termux se puede aunque ninguna otra app la vea.
    let temporal = tempfile::tempdir().unwrap();
    let hogar = std::fs::canonicalize(temporal.path()).unwrap().join("home");
    std::fs::create_dir_all(hogar.join("Sync/magi")).unwrap();
    assert_eq!(
        resolver_carpeta("~/Sync/magi", &hogar, true).unwrap(),
        hogar.join("Sync/magi")
    );
    let error = resolver_carpeta("~/Sync/otra", &hogar, true).unwrap_err();
    assert!(error.contains("no existe"), "{error}");
    assert!(!error.contains("termux-setup-storage"), "{error}");
}

#[test]
fn en_termux_la_carpeta_tiene_que_existir_y_ser_un_directorio() {
    termux();
    let termux = almacenamiento();
    let error =
        resolver_carpeta("~/storage/shared/Sync/no-existe", &termux.hogar, true).unwrap_err();
    assert!(error.contains("no existe"), "{error}");
    assert!(!error.contains("termux-setup-storage"), "storage sí está");
    // Un fichero no es una carpeta, ni siquiera con la ruta resuelta.
    let fichero = termux.carpeta_real.join("fichero.txt");
    std::fs::write(&fichero, b"x").unwrap();
    let error = resolver_carpeta(&fichero.to_string_lossy(), &termux.hogar, true).unwrap_err();
    assert!(error.contains("no es un directorio"), "{error}");
}

#[test]
fn en_termux_una_carpeta_que_no_se_puede_escribir_se_dice() {
    termux();
    if es_root() {
        return; // root escribe donde quiere: no hay nada que probar
    }
    use std::os::unix::fs::PermissionsExt as _;
    let termux = almacenamiento();
    std::fs::set_permissions(&termux.carpeta_real, std::fs::Permissions::from_mode(0o500)).unwrap();
    let resultado = resolver_carpeta("~/storage/shared/Sync/magi", &termux.hogar, true);
    std::fs::set_permissions(&termux.carpeta_real, std::fs::Permissions::from_mode(0o700)).unwrap();
    let error = resultado.unwrap_err();
    assert!(error.contains("no se puede escribir"), "{error}");
    // La prueba de escritura no deja nada en la carpeta.
    assert_eq!(std::fs::read_dir(&termux.carpeta_real).unwrap().count(), 0);
}

#[test]
fn bajo_storage_un_permiso_denegado_remite_a_termux_setup_storage() {
    termux();
    if es_root() {
        return;
    }
    use std::os::unix::fs::PermissionsExt as _;
    let termux = almacenamiento();
    // Android sin el permiso de almacenamiento: el enlace existe pero su
    // destino no se puede atravesar.
    let real = termux
        .carpeta_real
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o000)).unwrap();
    let resultado = resolver_carpeta("~/storage/shared/Sync/magi", &termux.hogar, true);
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
    let error = resultado.unwrap_err();
    assert!(
        error.contains("ejecuta termux-setup-storage y vuelve"),
        "{error}"
    );
}

#[test]
fn fuera_de_termux_la_carpeta_sigue_siendo_la_que_escribio_el_usuario() {
    let temporal = tempfile::tempdir().unwrap();
    let hogar = temporal.path().join("home");
    let real = temporal.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::create_dir_all(&hogar).unwrap();
    std::os::unix::fs::symlink(&real, hogar.join("enlace")).unwrap();
    // Sin resolver: lo que se guarda es lo escrito (con `~` expandido).
    assert_eq!(
        resolver_carpeta("~/enlace", &hogar, false).unwrap(),
        hogar.join("enlace")
    );
    // Sin `~/storage` no hay instrucción de Termux, solo el error de siempre.
    let error = resolver_carpeta("~/storage/shared/x", &hogar, false).unwrap_err();
    assert_eq!(
        error,
        format!(
            "la carpeta {} no existe o no es un directorio",
            hogar.join("storage/shared/x").display()
        )
    );
}

// --------------------------------------------------- por la interfaz (App)

fn entorno_app(hogar: &Path) -> (tempfile::TempDir, Rutas) {
    let temporal = tempfile::Builder::new()
        .prefix("magitsy")
        .tempdir()
        .expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: hogar.to_path_buf(),
        runtime: raiz.join("runtime"),
    };
    std::fs::create_dir_all(&rutas.datos).unwrap();
    (temporal, rutas)
}

fn con_app(rutas: &Rutas, config: Config) -> (App, Terminal<TestBackend>) {
    let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("almacén");
    let (mut app, _enviados) =
        App::de_prueba(rutas.clone(), config, Tema::respaldo(), almacen).expect("app de prueba");
    // La clave va a un llavero en memoria: nunca el del sistema ni un fichero.
    app.llavero = Arc::new(LlaveroEnMemoria::nuevo());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    app.iniciar_pintado(&mut terminal).expect("pintado");
    (app, terminal)
}

fn tecla(app: &mut App, terminal: &mut Terminal<TestBackend>, codigo: KeyCode) {
    app.procesar(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
    app.paso(terminal, Instant::now()).expect("paso");
}

fn tecla_con(
    app: &mut App,
    terminal: &mut Terminal<TestBackend>,
    codigo: KeyCode,
    modificadores: KeyModifiers,
) {
    app.procesar(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
    app.paso(terminal, Instant::now()).expect("paso");
}

/// Abre la paleta, escribe una consulta y ejecuta la primera entrada.
fn abrir_paleta(app: &mut App, terminal: &mut Terminal<TestBackend>, consulta: &str) {
    tecla_con(app, terminal, KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(app.paleta.is_some(), "la paleta no se abrió");
    for caracter in consulta.chars() {
        tecla(app, terminal, KeyCode::Char(caracter));
    }
    tecla(app, terminal, KeyCode::Enter);
}

/// En la vista Sincronización: `n`, el texto en el campo y Enter.
fn elegir_carpeta(app: &mut App, terminal: &mut Terminal<TestBackend>, texto: &str) {
    abrir_paleta(app, terminal, "sincronización");
    assert_eq!(app.vista, Vista::Sincronizacion);
    tecla(app, terminal, KeyCode::Char('n'));
    match &mut app.dialogo {
        Some(Dialogo::EntradaTexto { campo, .. }) => *campo = CampoTexto::nuevo(texto),
        _ => panic!("n debe pedir la carpeta"),
    }
    tecla(app, terminal, KeyCode::Enter);
}

#[test]
fn en_termux_la_interfaz_guarda_la_carpeta_resuelta_y_sondea_en_vez_de_usar_notify() {
    termux();
    let termux = almacenamiento();
    let (_temporal, rutas) = entorno_app(&termux.hogar);
    let (mut app, mut terminal) = con_app(&rutas, Config::default());
    elegir_carpeta(&mut app, &mut terminal, "~/storage/shared/Sync/magi");
    // Se creó la sincronización y la clave se enseña.
    assert!(
        matches!(app.dialogo, Some(Dialogo::ClaveSincronizacion { .. })),
        "{:?}",
        app.mensaje.as_ref().map(|mensaje| &mensaje.texto)
    );
    let esperada = termux.carpeta_real.to_string_lossy().to_string();
    assert_eq!(
        app.config.sincronizacion.carpeta.as_deref(),
        Some(esperada.as_str()),
        "se guarda la ruta resuelta"
    );
    // Y también en config.toml, que es lo que leerá el siguiente arranque.
    let (guardada, aviso) = Config::cargar(&rutas.fichero_config());
    assert_eq!(aviso, None);
    assert_eq!(
        guardada.sincronizacion.carpeta.as_deref(),
        Some(esperada.as_str())
    );
    // La vigilancia es un sondeo cada `[termux] sincronizacion_sondeo_seg`.
    assert_eq!(
        app.sincronizacion.sondeo_vigilancia,
        Some(Duration::from_secs(30))
    );
    assert!(app.sincronizacion.vigilancia.is_some());
}

#[test]
fn el_periodo_del_sondeo_sale_de_la_configuracion_y_se_acota() {
    termux();
    for (configurado, esperado) in [(120, 120), (5, 10), (9_000, 600)] {
        let termux = almacenamiento();
        let (_temporal, rutas) = entorno_app(&termux.hogar);
        let mut config = Config::default();
        config.termux.sincronizacion_sondeo_seg = configurado;
        let (mut app, mut terminal) = con_app(&rutas, config);
        elegir_carpeta(&mut app, &mut terminal, "~/storage/shared/Sync/magi");
        assert_eq!(
            app.sincronizacion.sondeo_vigilancia,
            Some(Duration::from_secs(esperado)),
            "con {configurado} s configurados"
        );
    }
}

#[test]
fn sin_storage_la_interfaz_da_la_instruccion_y_vuelve_a_pedir_la_carpeta() {
    termux();
    let temporal = tempfile::tempdir().unwrap();
    let hogar = temporal.path().join("home");
    std::fs::create_dir_all(&hogar).unwrap();
    let (_otro, rutas) = entorno_app(&hogar);
    let (mut app, mut terminal) = con_app(&rutas, Config::default());
    elegir_carpeta(&mut app, &mut terminal, "~/storage/shared/Sync/magi");
    let mensaje = app.mensaje.as_ref().expect("mensaje");
    assert!(mensaje.error);
    assert!(
        mensaje
            .texto
            .contains("ejecuta termux-setup-storage y vuelve"),
        "{}",
        mensaje.texto
    );
    match &app.dialogo {
        Some(Dialogo::EntradaTexto { campo, .. }) => {
            assert_eq!(
                campo.texto, "~/storage/shared/Sync/magi",
                "conserva lo escrito"
            );
        }
        _ => panic!("se vuelve a pedir la carpeta"),
    }
    assert_eq!(app.config.sincronizacion.carpeta, None, "no se guarda nada");
}

// ====================================================================
// B. La carpeta se sondea con una firma barata (§4.3, §7.12)
// ====================================================================

const UUID_A: &str = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
const UUID_B: &str = "6b1e2f3a-4c5d-4e6f-8a7b-9c0d1e2f3a41";

/// Escribe un fichero como lo hace MAGI: un temporal en la misma carpeta y un
/// `rename`, así que quien lista la carpeta ve el fichero entero o no lo ve.
/// `std::fs::write` trunca y escribe en dos pasos: un sondeo que mira entre
/// los dos vería un fichero vacío y daría dos avisos por un solo cambio.
fn escribir_atomico(ruta: &Path, contenido: &[u8]) {
    let nombre = ruta.file_name().unwrap().to_string_lossy();
    // El punto del principio lo deja fuera de los nombres de equipo.
    let temporal = ruta.with_file_name(format!(".{nombre}.prueba"));
    std::fs::write(&temporal, contenido).unwrap();
    std::fs::rename(&temporal, ruta).unwrap();
}

fn fichero_de_equipo(carpeta: &Path, uuid: &str, contenido: &[u8]) -> PathBuf {
    let ruta = carpeta.join(carpeta::nombre_fichero(uuid));
    escribir_atomico(&ruta, contenido);
    ruta
}

#[test]
fn la_firma_cuenta_nombre_tamano_y_fecha_de_los_ficheros_de_magi_y_nada_mas() {
    let (_temporal, carpeta) = carpeta_compartida();
    let vacia = firma_carpeta(&carpeta);
    assert_eq!(vacia, Some(Vec::new()));
    let ruta = fichero_de_equipo(&carpeta, UUID_A, b"uno");
    let con_uno = firma_carpeta(&carpeta);
    assert_ne!(con_uno, vacia, "un fichero nuevo cambia la firma");
    assert_eq!(con_uno.as_ref().unwrap().len(), 1);
    assert_eq!(con_uno.as_ref().unwrap()[0].tamano, 3);
    // Otro tamaño.
    escribir_atomico(&ruta, b"mas largo");
    let mas_largo = firma_carpeta(&carpeta);
    assert_ne!(mas_largo, con_uno);
    // La misma longitud y otra fecha.
    filetime::set_file_mtime(&ruta, filetime::FileTime::from_unix_time(1_700_000_000, 0)).unwrap();
    let otra_fecha = firma_carpeta(&carpeta);
    assert_ne!(otra_fecha, mas_largo, "la fecha también cuenta");
    // Un fichero más y uno menos.
    let segundo = fichero_de_equipo(&carpeta, UUID_B, b"dos");
    assert_eq!(firma_carpeta(&carpeta).unwrap().len(), 2);
    std::fs::remove_file(&segundo).unwrap();
    assert_eq!(firma_carpeta(&carpeta), otra_fecha);
    // Una carpeta que no existe no es un error: está vacía. Una que no se
    // puede leer sí lo es (`None`).
    assert_eq!(firma_carpeta(&carpeta.join("no-existe")), Some(Vec::new()));
    let fichero_suelto = carpeta.join("suelto.txt");
    std::fs::write(&fichero_suelto, b"x").unwrap();
    assert_eq!(firma_carpeta(&fichero_suelto), None);
}

#[test]
fn la_firma_ignora_lo_que_no_es_de_magi_los_temporales_y_los_enlaces() {
    let (_temporal, carpeta) = carpeta_compartida();
    let base = firma_carpeta(&carpeta);
    std::fs::write(carpeta.join("notas.txt"), b"x").unwrap();
    std::fs::write(carpeta.join(format!(".magi-{UUID_A}.age.tmp")), b"x").unwrap();
    std::fs::write(
        carpeta.join(format!(".syncthing.magi-{UUID_A}.age.tmp")),
        b"x",
    )
    .unwrap();
    std::fs::write(
        carpeta.join(format!("magi-{UUID_A}.sync-conflict-20260101-000000.age")),
        b"x",
    )
    .unwrap();
    std::fs::write(
        carpeta.join(format!("magi-{}.age", UUID_A.to_uppercase())),
        b"x",
    )
    .unwrap();
    std::fs::create_dir(carpeta.join(carpeta::nombre_fichero(UUID_B))).unwrap();
    let fuera = carpeta.parent().unwrap().join("fuera.age");
    std::fs::write(&fuera, b"fuera").unwrap();
    std::os::unix::fs::symlink(&fuera, carpeta.join(carpeta::nombre_fichero(UUID_A))).unwrap();
    // En sistemas que no distinguen mayúsculas, el nombre en mayúsculas es el
    // mismo fichero que el canónico y el enlace de arriba no se habría podido
    // crear: la firma sigue sin contar nada.
    assert_eq!(firma_carpeta(&carpeta), base);
}

/// Los eventos de vigilancia que llegan por el canal de la UI.
async fn siguiente(rx: &mut mpsc::UnboundedReceiver<Evento>, plazo: Duration) -> Option<u64> {
    match tokio::time::timeout(plazo, rx.recv()).await {
        Ok(Some(Evento::Inventario(EventoInventario::CarpetaCambiada { generacion }))) => {
            Some(generacion)
        }
        Ok(Some(_)) => panic!("la vigilancia solo manda CarpetaCambiada"),
        Ok(None) | Err(_) => None,
    }
}

async fn vaciar(rx: &mut mpsc::UnboundedReceiver<Evento>) {
    while siguiente(rx, Duration::from_millis(5)).await.is_some() {}
}

/// El sondeo de pruebas: ritmo rápido y seguridad que no llega.
fn lanzar_sondeo(
    carpeta: &Path,
    seguridad: Duration,
) -> (tokio::task::JoinHandle<()>, mpsc::UnboundedReceiver<Evento>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let tarea = tokio::spawn(sondear(
        carpeta.to_path_buf(),
        7,
        tx,
        Duration::from_millis(30),
        seguridad,
    ));
    (tarea, rx)
}

const NUNCA: Duration = Duration::from_secs(3600);
const ESPERA_CAMBIO: Duration = Duration::from_secs(5);
const ESPERA_NADA: Duration = Duration::from_millis(400);
/// Lo que se deja pasar para que el sondeo tome su línea base antes de tocar
/// nada (con margen: las pruebas corren en paralelo y puede tardar).
const LINEA_BASE: Duration = Duration::from_millis(500);

#[tokio::test]
async fn el_sondeo_sin_cambios_no_emite_nada() {
    let (_temporal, carpeta) = carpeta_compartida();
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    let (tarea, mut rx) = lanzar_sondeo(&carpeta, NUNCA);
    assert_eq!(
        siguiente(&mut rx, ESPERA_NADA).await,
        None,
        "la línea base no avisa"
    );
    assert_eq!(siguiente(&mut rx, ESPERA_NADA).await, None);
    tarea.abort();
}

#[tokio::test]
async fn el_sondeo_avisa_de_un_fichero_nuevo_borrado_o_cambiado() {
    let (_temporal, carpeta) = carpeta_compartida();
    let (tarea, mut rx) = lanzar_sondeo(&carpeta, NUNCA);
    tokio::time::sleep(LINEA_BASE).await; // línea base

    // Nuevo.
    let ruta = fichero_de_equipo(&carpeta, UUID_A, b"uno");
    assert_eq!(
        siguiente(&mut rx, ESPERA_CAMBIO).await,
        Some(7),
        "un fichero nuevo avisa, con la generación de la vigilancia"
    );
    assert_eq!(
        siguiente(&mut rx, ESPERA_NADA).await,
        None,
        "una sola vez por cambio"
    );

    // Otro tamaño.
    escribir_atomico(&ruta, b"mas largo");
    assert_eq!(siguiente(&mut rx, ESPERA_CAMBIO).await, Some(7));
    assert_eq!(siguiente(&mut rx, ESPERA_NADA).await, None);

    // Mismo tamaño, otra fecha.
    filetime::set_file_mtime(&ruta, filetime::FileTime::from_unix_time(1_700_000_000, 0)).unwrap();
    assert_eq!(siguiente(&mut rx, ESPERA_CAMBIO).await, Some(7));
    assert_eq!(siguiente(&mut rx, ESPERA_NADA).await, None);

    // Borrado.
    std::fs::remove_file(&ruta).unwrap();
    assert_eq!(siguiente(&mut rx, ESPERA_CAMBIO).await, Some(7));
    assert_eq!(siguiente(&mut rx, ESPERA_NADA).await, None);
    tarea.abort();
}

#[tokio::test]
async fn el_sondeo_no_avisa_de_ficheros_que_no_son_de_magi_ni_de_temporales() {
    let (_temporal, carpeta) = carpeta_compartida();
    let (tarea, mut rx) = lanzar_sondeo(&carpeta, NUNCA);
    tokio::time::sleep(LINEA_BASE).await;
    std::fs::write(carpeta.join("notas.txt"), b"x").unwrap();
    std::fs::write(
        carpeta.join(format!(".syncthing.magi-{UUID_A}.age.tmp")),
        b"x",
    )
    .unwrap();
    std::fs::write(carpeta.join(format!(".magi-{UUID_A}.age.12345.tmp")), b"x").unwrap();
    std::fs::write(
        carpeta.join(format!("magi-{UUID_A}.sync-conflict-20260101-000000.age")),
        b"x",
    )
    .unwrap();
    std::fs::create_dir(carpeta.join(carpeta::nombre_fichero(UUID_B))).unwrap();
    assert_eq!(siguiente(&mut rx, Duration::from_millis(800)).await, None);
    tarea.abort();
}

#[tokio::test]
async fn el_sondeo_hace_una_pasada_de_seguridad_aunque_la_firma_no_cambie() {
    let (_temporal, carpeta) = carpeta_compartida();
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    let (tarea, mut rx) = lanzar_sondeo(&carpeta, Duration::from_millis(250));
    // Sin ningún cambio, la de seguridad avisa y vuelve a avisar.
    assert_eq!(siguiente(&mut rx, ESPERA_CAMBIO).await, Some(7));
    assert_eq!(siguiente(&mut rx, ESPERA_CAMBIO).await, Some(7));
    tarea.abort();
}

#[tokio::test]
async fn el_sondeo_de_seguridad_se_cuenta_desde_el_ultimo_aviso_y_no_se_acumula() {
    let (_temporal, carpeta) = carpeta_compartida();
    let (tarea, mut rx) = lanzar_sondeo(&carpeta, Duration::from_secs(4));
    tokio::time::sleep(Duration::from_secs(1)).await;
    // Un cambio de verdad avisa en el acto (hacia el segundo 1) y reinicia la
    // cuenta: 3,2 s después, ya pasados los 4 s desde que arrancó el sondeo
    // pero no los 4 s desde ese aviso, todavía no toca la de seguridad.
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    assert_eq!(siguiente(&mut rx, ESPERA_CAMBIO).await, Some(7));
    assert_eq!(siguiente(&mut rx, Duration::from_millis(3200)).await, None);
    tarea.abort();
}

#[tokio::test]
async fn la_seguridad_de_termux_son_diez_minutos() {
    assert_eq!(SEGURIDAD_CARPETA_TERMUX, Duration::from_secs(600));
}

#[tokio::test]
async fn el_sondeo_termina_cuando_la_interfaz_ya_no_escucha() {
    let (_temporal, carpeta) = carpeta_compartida();
    // Con la seguridad corta, el sondeo intenta avisar enseguida y descubre
    // que nadie escucha, haya o no cambiado la carpeta.
    let (tarea, rx) = lanzar_sondeo(&carpeta, Duration::from_millis(150));
    drop(rx);
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    tokio::time::timeout(Duration::from_secs(5), tarea)
        .await
        .expect("el sondeo no termina al cerrarse el canal")
        .expect("el sondeo no debe entrar en pánico");
}

#[tokio::test]
async fn una_carpeta_que_desaparece_y_vuelve_se_ve_como_dos_cambios() {
    let (_temporal, carpeta) = carpeta_compartida();
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    let (tarea, mut rx) = lanzar_sondeo(&carpeta, NUNCA);
    tokio::time::sleep(LINEA_BASE).await;
    let movida = carpeta.with_extension("movida");
    std::fs::rename(&carpeta, &movida).unwrap();
    assert_eq!(
        siguiente(&mut rx, ESPERA_CAMBIO).await,
        Some(7),
        "sin carpeta la firma queda vacía: la pasada dirá qué falta"
    );
    std::fs::rename(&movida, &carpeta).unwrap();
    assert_eq!(siguiente(&mut rx, ESPERA_CAMBIO).await, Some(7));
    tarea.abort();
}

#[tokio::test]
async fn en_termux_no_se_usa_notify() {
    let (_temporal, carpeta) = carpeta_compartida();
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Con sondeo larguísimo: notify avisaría a los ~2 s de escribir (lo
    // comprueba la prueba de al lado, con la misma escritura); el sondeo,
    // nunca en este plazo.
    let tarea = tokio::spawn(vigilar_carpeta(
        carpeta.clone(),
        9,
        tx,
        Some(Duration::from_secs(3600)),
    ));
    tokio::time::sleep(Duration::from_millis(300)).await;
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    assert_eq!(
        siguiente(&mut rx, Duration::from_millis(3500)).await,
        None,
        "con sondeo de Termux no hay avisos del sistema de ficheros"
    );
    tarea.abort();
    vaciar(&mut rx).await;
}

#[tokio::test]
async fn fuera_de_termux_la_vigilancia_es_la_de_siempre_con_notify() {
    // El control de la prueba de arriba: sin sondeo de Termux (`None`), la
    // misma escritura de un fichero de MAGI en la misma carpeta sí despierta a
    // la vigilancia, con la generación que se le dio, a los ~2 s de agrupación
    // de notify. El respaldo de la vigilancia de siempre es de 30 s, así que
    // un aviso dentro de `ESPERA_CAMBIO` (5 s) solo puede ser de notify.
    let (_temporal, carpeta) = carpeta_compartida();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tarea = tokio::spawn(vigilar_carpeta(carpeta.clone(), 9, tx, None));
    tokio::time::sleep(Duration::from_millis(300)).await;
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    assert_eq!(
        siguiente(&mut rx, ESPERA_CAMBIO).await,
        Some(9),
        "sin sondeo de Termux avisa notify"
    );
    tarea.abort();
}

// ---------------------------------- una carpeta que no responde (S1) y sin hilos (S3)

/// Un lector cuya lectura número `colgada` (desde 0) no responde hasta que se
/// suelta el otro extremo del canal: el almacenamiento compartido (FUSE)
/// colgado. Las demás leen la carpeta de verdad.
struct LectorColgado {
    colgada: usize,
    soltar: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    /// Lecturas empezadas, la colgada incluida.
    lecturas: AtomicUsize,
}

impl LectorColgado {
    /// El lector y lo que lo suelta (al soltarlo, o al caer, la lectura sigue).
    fn nuevo(colgada: usize) -> (Arc<Self>, std::sync::mpsc::Sender<()>) {
        let (soltar, esperar) = std::sync::mpsc::channel();
        let lector = Arc::new(Self {
            colgada,
            soltar: Mutex::new(Some(esperar)),
            lecturas: AtomicUsize::new(0),
        });
        (lector, soltar)
    }
}

impl LectorFirma for LectorColgado {
    fn leer(&self, carpeta: &Path) -> FirmaCarpeta {
        if self.lecturas.fetch_add(1, Ordering::SeqCst) == self.colgada {
            let esperar = self.soltar.lock().unwrap().take();
            if let Some(esperar) = esperar {
                let _ = esperar.recv();
            }
        }
        firma_carpeta(carpeta)
    }
}

/// Espera, con plazo, a que se cumpla una condición.
async fn hasta(plazo: Duration, condicion: impl Fn() -> bool) -> bool {
    let limite = Instant::now() + plazo;
    while Instant::now() < limite {
        if condicion() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    condicion()
}

const PLAZO_CORTO: Duration = Duration::from_millis(60);

#[tokio::test]
async fn con_la_carpeta_colgada_el_sondeo_no_pide_ninguna_pasada_ni_la_de_seguridad() {
    let (_temporal, carpeta) = carpeta_compartida();
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    let (lector, soltar) = LectorColgado::nuevo(0);
    let (tx, mut rx) = mpsc::unbounded_channel();
    // La seguridad (100 ms) vence mucho antes que cualquier otra cosa y el
    // plazo de lectura (60 ms) también: sin el arreglo, la seguridad avisaría
    // con la lectura todavía colgada y la pasada que provoca (en el hilo de
    // la UI, sin plazo) congelaría la TUI.
    let tarea = tokio::spawn(sondear_con(
        carpeta.clone(),
        7,
        tx,
        Duration::from_millis(20),
        Duration::from_millis(100),
        PLAZO_CORTO,
        lector.clone(),
    ));
    assert_eq!(
        siguiente(&mut rx, Duration::from_millis(900)).await,
        None,
        "con la lectura sin responder no se pide nada, ni siquiera la de seguridad"
    );
    assert_eq!(
        lector.lecturas.load(Ordering::SeqCst),
        1,
        "no se lanza otra lectura encima de la colgada"
    );
    assert!(
        lectura_de_carpeta_colgada(&carpeta, PLAZO_CORTO),
        "la interfaz ve la carpeta colgada"
    );
    assert!(
        !lectura_de_carpeta_colgada(&carpeta.join("otra"), PLAZO_CORTO),
        "y solo esa carpeta"
    );
    // Responde: la seguridad, que seguía debida, sale ahora.
    drop(soltar);
    assert_eq!(
        siguiente(&mut rx, ESPERA_CAMBIO).await,
        Some(7),
        "al responder, la pasada de seguridad pendiente sale"
    );
    assert!(
        hasta(ESPERA_CAMBIO, || !lectura_de_carpeta_colgada(
            &carpeta,
            PLAZO_CORTO
        ))
        .await,
        "la carpeta deja de verse colgada al responder"
    );
    tarea.abort();
}

#[tokio::test]
async fn con_la_carpeta_colgada_un_cambio_tampoco_pide_pasada_hasta_que_responde() {
    // La primera firma (la línea base) responde: el cambio llega después, con
    // la segunda lectura colgada. Sin pasada mientras no responda; al
    // responder, el cambio se avisa una sola vez. (Esta parte ya se cumplía
    // antes del arreglo, porque un cambio solo se ve con una firma nueva: es
    // la guarda de que no se rompa; la regresión es la prueba de arriba.)
    let (_temporal, carpeta) = carpeta_compartida();
    let (lector, soltar) = LectorColgado::nuevo(1);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tarea = tokio::spawn(sondear_con(
        carpeta.clone(),
        7,
        tx,
        Duration::from_millis(20),
        NUNCA,
        PLAZO_CORTO,
        lector.clone(),
    ));
    // Con la segunda lectura empezada (y colgada), la línea base ya está
    // tomada: el cambio llega después de ella.
    assert!(
        hasta(ESPERA_CAMBIO, || lector.lecturas.load(Ordering::SeqCst)
            >= 2)
        .await
    );
    fichero_de_equipo(&carpeta, UUID_A, b"uno");
    assert!(
        hasta(ESPERA_CAMBIO, || lectura_de_carpeta_colgada(
            &carpeta,
            PLAZO_CORTO
        ))
        .await
    );
    assert_eq!(
        siguiente(&mut rx, Duration::from_millis(500)).await,
        None,
        "colgada la lectura, no se pide pasada por un cambio"
    );
    drop(soltar);
    assert_eq!(
        siguiente(&mut rx, ESPERA_CAMBIO).await,
        Some(7),
        "al responder se ve el cambio de la carpeta"
    );
    assert_eq!(siguiente(&mut rx, ESPERA_NADA).await, None, "una sola vez");
    tarea.abort();
}

/// Un lector con las firmas que se le dan, una por lectura (la última se
/// repite), que espera una orden del test antes de devolver cada una: así el
/// test decide cuándo se toma cada muestra del sondeo.
struct LectorGuionizado {
    firmas: Mutex<std::collections::VecDeque<FirmaCarpeta>>,
    pasos: Mutex<std::sync::mpsc::Receiver<()>>,
    /// Lecturas empezadas.
    lecturas: AtomicUsize,
}

impl LectorGuionizado {
    fn nuevo(firmas: Vec<FirmaCarpeta>) -> (Arc<Self>, std::sync::mpsc::Sender<()>) {
        let (paso, pasos) = std::sync::mpsc::channel();
        let lector = Arc::new(Self {
            firmas: Mutex::new(firmas.into()),
            pasos: Mutex::new(pasos),
            lecturas: AtomicUsize::new(0),
        });
        (lector, paso)
    }
}

impl LectorFirma for LectorGuionizado {
    fn leer(&self, _carpeta: &Path) -> FirmaCarpeta {
        self.lecturas.fetch_add(1, Ordering::SeqCst);
        let _ = self.pasos.lock().unwrap().recv();
        let mut firmas = self.firmas.lock().unwrap();
        if firmas.len() > 1 {
            firmas.pop_front().expect("hay firmas")
        } else {
            firmas.front().cloned().expect("hay una firma")
        }
    }
}

/// La firma de una carpeta con el fichero de un equipo de ese tamaño.
fn firma_de_un_fichero(tamano: u64) -> FirmaCarpeta {
    Some(vec![carpeta::EntradaCarpeta {
        nombre: carpeta::nombre_fichero(UUID_A),
        tamano,
        modificado_ms: 0,
    }])
}

/// Un lector cuya firma cambia en cada lectura: un fichero que otra app no
/// deja de escribir.
struct LectorQueNoParaDeCambiar {
    lecturas: AtomicUsize,
}

impl LectorFirma for LectorQueNoParaDeCambiar {
    fn leer(&self, _carpeta: &Path) -> FirmaCarpeta {
        let numero = self.lecturas.fetch_add(1, Ordering::SeqCst);
        firma_de_un_fichero(numero as u64 + 1)
    }
}

#[tokio::test]
async fn el_sondeo_espera_a_que_la_firma_se_asiente_antes_de_pedir_la_pasada() {
    // Cierre de la Fase 17 (C3): otra app escribe el fichero de un equipo en
    // sitio; una pasada que lo leyera a medias lo tomaría por un fichero que
    // no se abre con la clave y abriría «unir este equipo». Con notify el
    // escritorio espera a que cese la actividad; el sondeo, a que la firma sea
    // la misma en dos muestras seguidas.
    let (_temporal, carpeta) = carpeta_compartida();
    let (lector, paso) = LectorGuionizado::nuevo(vec![
        Some(Vec::new()),        // 0: línea base
        firma_de_un_fichero(10), // 1: aparece (a medias)
        firma_de_un_fichero(20), // 2: sigue creciendo
        firma_de_un_fichero(20), // 3: igual que la anterior: asentada
        firma_de_un_fichero(20), // 4: sin cambios
    ]);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tarea = tokio::spawn(sondear_con(
        carpeta.clone(),
        7,
        tx,
        Duration::from_millis(20),
        NUNCA,
        Duration::from_secs(60),
        lector.clone(),
    ));
    async fn esperar_lectura(lector: &LectorGuionizado, numero: usize) {
        assert!(
            hasta(ESPERA_CAMBIO, || lector.lecturas.load(Ordering::SeqCst)
                >= numero)
            .await,
            "el sondeo no llegó a la lectura {numero}"
        );
    }
    // Cada paso deja tomar una muestra; la lectura siguiente empieza cuando la
    // decisión sobre la anterior ya está tomada (y enviada, si se pidió).
    esperar_lectura(&lector, 1).await;
    for (muestra, esperada) in [(0, None), (1, None), (2, None), (3, Some(7)), (4, None)] {
        paso.send(()).expect("el sondeo sigue vivo");
        esperar_lectura(&lector, muestra + 2).await;
        let recibido = match rx.try_recv() {
            Ok(Evento::Inventario(EventoInventario::CarpetaCambiada { generacion })) => {
                Some(generacion)
            }
            Ok(_) => panic!("la vigilancia solo manda CarpetaCambiada"),
            Err(_) => None,
        };
        assert_eq!(
            recibido, esperada,
            "muestra {muestra}: sin asentarse no se pide la pasada; asentada, una sola vez"
        );
    }
    assert!(rx.try_recv().is_err(), "ni una más");
    tarea.abort();
}

#[tokio::test]
async fn un_fichero_que_no_deja_de_cambiar_no_pide_pasada_pero_la_de_seguridad_si() {
    let (_temporal, carpeta) = carpeta_compartida();
    let lector = Arc::new(LectorQueNoParaDeCambiar {
        lecturas: AtomicUsize::new(0),
    });
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Seguridad de 2 s: durante el primer segundo, con la firma cambiando en
    // cada muestra, no se avisa de nada.
    let tarea = tokio::spawn(sondear_con(
        carpeta.clone(),
        7,
        tx,
        Duration::from_millis(20),
        Duration::from_secs(2),
        Duration::from_secs(60),
        lector.clone(),
    ));
    assert_eq!(
        siguiente(&mut rx, Duration::from_secs(1)).await,
        None,
        "un fichero que sigue cambiando no se lee a medias"
    );
    assert!(
        lector.lecturas.load(Ordering::SeqCst) >= 5,
        "el sondeo siguió tomando muestras"
    );
    // La pasada de seguridad no espera a que nada se asiente.
    assert_eq!(
        siguiente(&mut rx, Duration::from_secs(5)).await,
        Some(7),
        "la seguridad sale aunque la firma no pare de cambiar"
    );
    tarea.abort();
}

/// Una `App` de Termux con la sincronización apuntando a una carpeta cuyo
/// sondeo tiene la lectura colgada, y con los efectos externos encendidos (sin
/// ellos no se pide ninguna pasada).
struct AppConCarpetaColgada {
    app: App,
    terminal: Terminal<TestBackend>,
    carpeta: PathBuf,
    /// Al soltarlo (o al caer), la lectura colgada responde.
    soltar: Option<std::sync::mpsc::Sender<()>>,
    /// El runtime del sondeo: tiene que seguir vivo mientras dure la prueba.
    runtime: tokio::runtime::Runtime,
    _temporales: (tempfile::TempDir, tempfile::TempDir),
}

impl AppConCarpetaColgada {
    fn nueva() -> Self {
        termux();
        let (temporal_carpeta, carpeta) = carpeta_compartida();
        let (temporal_rutas, rutas) = entorno_app(carpeta.parent().unwrap());
        let mut config = Config::default();
        config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
        let (mut app, terminal) = con_app(&rutas, config);
        app.sincronizacion.plazo_lectura_carpeta = PLAZO_CORTO;
        app.activar_efectos_externos_de_prueba();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let (lector, soltar) = LectorColgado::nuevo(0);
        let (tx, rx) = mpsc::unbounded_channel();
        // El sondeo no avisa mientras su lectura no responda; el canal se
        // queda abierto para que no termine.
        std::mem::forget(rx);
        runtime.spawn(sondear_con(
            carpeta.clone(),
            7,
            tx,
            Duration::from_millis(20),
            NUNCA,
            PLAZO_CORTO,
            lector,
        ));
        let colgada = runtime.block_on(hasta(ESPERA_CAMBIO, || {
            lectura_de_carpeta_colgada(&carpeta, PLAZO_CORTO)
        }));
        assert!(colgada, "la lectura debía colgarse");
        Self {
            app,
            terminal,
            carpeta,
            soltar: Some(soltar),
            runtime,
            _temporales: (temporal_carpeta, temporal_rutas),
        }
    }

    /// El aviso de que la carpeta cambió, como lo manda la vigilancia.
    fn carpeta_cambiada(&mut self) {
        let generacion = self.app.sincronizacion.generacion_vigilancia;
        self.app
            .procesar(Evento::Inventario(EventoInventario::CarpetaCambiada {
                generacion,
            }));
    }

    /// La carpeta responde: la lectura colgada termina.
    fn responder(&mut self) {
        drop(self.soltar.take());
        let carpeta = self.carpeta.clone();
        let responde = self.runtime.block_on(hasta(ESPERA_CAMBIO, || {
            !lectura_de_carpeta_colgada(&carpeta, PLAZO_CORTO)
        }));
        assert!(responde, "la carpeta debía responder");
    }
}

#[test]
fn una_pasada_aplazada_por_la_carpeta_colgada_se_reintenta_al_responder() {
    // Cierre de la Fase 17 (C2): la pasada que se aplaza porque leer la carpeta
    // congelaría la TUI no puede perder su disparador (el sondeo solo vuelve a
    // avisar con otro cambio de firma o a los 10 min).
    let mut prueba = AppConCarpetaColgada::nueva();
    assert_eq!(prueba.app.sincronizacion.pasadas_pedidas, 0);

    prueba.carpeta_cambiada();
    assert_eq!(prueba.app.sincronizacion.pasadas_pedidas, 1);
    assert!(
        !prueba.app.sincronizacion.sincronizando,
        "con la carpeta colgada no se lanza la pasada"
    );
    assert_eq!(
        prueba.app.sincronizacion.pasada_aplazada,
        Some(PoliticaBorrados::Preguntar),
        "pero queda aplazada, no perdida"
    );
    assert!(
        matches!(prueba.app.sincronizacion.estado, EstadoPasada::Error(_)),
        "el estado dice que no responde: {:?}",
        prueba.app.sincronizacion.estado
    );

    // Mientras no responda, los ticks no piden nada (ni cuentan pasadas).
    for _ in 0..3 {
        prueba.app.procesar(Evento::Tick);
    }
    assert_eq!(prueba.app.sincronizacion.pasadas_pedidas, 1);
    assert!(prueba.app.sincronizacion.pasada_aplazada.is_some());

    // Responde: el siguiente tick reintenta, una sola vez.
    prueba.responder();
    prueba.app.procesar(Evento::Tick);
    assert_eq!(
        prueba.app.sincronizacion.pasadas_pedidas, 2,
        "al responder se reintenta"
    );
    assert!(prueba.app.sincronizacion.sincronizando, "la pasada arranca");
    assert_eq!(prueba.app.sincronizacion.pasada_aplazada, None);
    prueba.app.procesar(Evento::Tick);
    assert_eq!(prueba.app.sincronizacion.pasadas_pedidas, 2, "una sola vez");
}

#[test]
fn una_pasada_aplazada_no_pisa_lo_que_espera_una_decision() {
    for estado in [
        EstadoPasada::EsperandoClave("otro".to_string()),
        EstadoPasada::Quitado,
    ] {
        let mut prueba = AppConCarpetaColgada::nueva();
        prueba.app.sincronizacion.estado = estado.clone();
        prueba.carpeta_cambiada();
        assert_eq!(
            prueba.app.sincronizacion.estado, estado,
            "el estado que espera una decisión se queda como está"
        );
        let mensaje = prueba.app.mensaje.as_ref().expect("el aviso va a la barra");
        assert!(mensaje.error);
        assert!(mensaje.texto.contains("no responde"), "{}", mensaje.texto);
        assert!(prueba.app.sincronizacion.pasada_aplazada.is_some());
        prueba.responder();
        prueba.app.procesar(Evento::Tick);
        assert!(
            prueba.app.sincronizacion.sincronizando,
            "con {estado:?} también se reintenta al responder"
        );
    }
}

#[test]
fn la_decision_de_borrados_aplazada_por_la_carpeta_colgada_no_se_pierde() {
    let mut prueba = AppConCarpetaColgada::nueva();
    let pendientes = BorradosPendientes {
        copia: PathBuf::from("copia.db"),
        hosts: vec!["web".to_string()],
        total_hosts: 3,
    };
    // Borrados en pausa y el diálogo abierto: el usuario decide aplicarlos
    // con la carpeta colgada.
    prueba.app.sincronizacion.estado = EstadoPasada::DecisionPendiente(pendientes.clone());
    prueba.app.sincronizacion.pausa = Some(pendientes.clone());
    prueba.app.dialogo = Some(Dialogo::ImportacionBorrados { pendientes });
    tecla(&mut prueba.app, &mut prueba.terminal, KeyCode::Char('a'));
    assert!(
        matches!(
            prueba.app.sincronizacion.estado,
            EstadoPasada::DecisionPendiente(_)
        ),
        "la decisión sigue pendiente: {:?}",
        prueba.app.sincronizacion.estado
    );
    assert!(prueba.app.sincronizacion.pausa.is_some());
    assert_eq!(
        prueba.app.sincronizacion.pasada_aplazada,
        Some(PoliticaBorrados::Aplicar)
    );
    // Otra petición sin decisión (la vigilancia) no borra la del usuario.
    prueba.carpeta_cambiada();
    assert_eq!(
        prueba.app.sincronizacion.pasada_aplazada,
        Some(PoliticaBorrados::Aplicar)
    );
    // Responde: la pasada reintentada lleva la decisión del usuario.
    prueba.responder();
    prueba.app.procesar(Evento::Tick);
    assert!(prueba.app.sincronizacion.sincronizando);
    assert_eq!(
        prueba.app.sincronizacion.politica,
        PoliticaBorrados::Aplicar
    );
    assert_eq!(prueba.app.sincronizacion.pasada_aplazada, None);
}

#[test]
fn al_volver_a_la_vista_con_la_carpeta_colgada_no_se_pide_la_pasada() {
    termux();
    let (_temporal_carpeta, carpeta) = carpeta_compartida();
    let (_otro, rutas) = entorno_app(carpeta.parent().unwrap());
    let mut config = Config::default();
    config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
    let (mut app, mut terminal) = con_app(&rutas, config);
    app.sincronizacion.plazo_lectura_carpeta = PLAZO_CORTO;

    // El sondeo de esa carpeta, con su lectura colgada.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let (lector, soltar) = LectorColgado::nuevo(0);
    let (tx, _rx) = mpsc::unbounded_channel();
    let sondeo = runtime.spawn(sondear_con(
        carpeta.clone(),
        7,
        tx,
        Duration::from_millis(20),
        NUNCA,
        PLAZO_CORTO,
        lector,
    ));
    runtime.block_on(async {
        assert!(
            hasta(ESPERA_CAMBIO, || lectura_de_carpeta_colgada(
                &carpeta,
                PLAZO_CORTO
            ))
            .await,
            "la lectura debía colgarse"
        );
    });

    // Con la carpeta colgada, entrar en la vista no pide la pasada.
    assert_eq!(app.sincronizacion.pasadas_pedidas, 0);
    abrir_paleta(&mut app, &mut terminal, "sincronización");
    assert_eq!(app.vista, Vista::Sincronizacion);
    assert_eq!(
        app.sincronizacion.pasadas_pedidas, 0,
        "la pasada leería la carpeta en el hilo de la UI, sin plazo"
    );

    // Responde: volver a la vista la pide como siempre.
    drop(soltar);
    runtime.block_on(async {
        assert!(
            hasta(ESPERA_CAMBIO, || !lectura_de_carpeta_colgada(
                &carpeta,
                PLAZO_CORTO
            ))
            .await
        );
    });
    tecla(&mut app, &mut terminal, KeyCode::Esc);
    assert_ne!(app.vista, Vista::Sincronizacion);
    abrir_paleta(&mut app, &mut terminal, "sincronización");
    assert_eq!(app.vista, Vista::Sincronizacion);
    assert_eq!(app.sincronizacion.pasadas_pedidas, 1, "al volver");
    sondeo.abort();
}

/// Un sistema que no deja crear más hilos (el límite de procesos de Android).
struct SinHilos {
    intentos: AtomicUsize,
}

impl LectorFirma for SinHilos {
    fn lanzar(&self, _trabajo: Box<dyn FnOnce() + Send + 'static>) -> io::Result<()> {
        self.intentos.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::from(io::ErrorKind::WouldBlock))
    }
}

#[test]
fn sin_poder_crear_el_hilo_el_sondeo_avisa_una_vez_en_el_log_y_queda_la_seguridad() {
    let lector = Arc::new(SinHilos {
        intentos: AtomicUsize::new(0),
    });
    let (resultado, lineas) = con_log(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut avisos = Vec::new();
            // Dos sondeos en la misma sesión: el aviso sale una sola vez.
            for _ in 0..2 {
                let (tx, mut rx) = mpsc::unbounded_channel();
                let (_temporal, carpeta) = carpeta_compartida();
                let tarea = tokio::spawn(sondear_con(
                    carpeta,
                    7,
                    tx,
                    Duration::from_millis(20),
                    Duration::from_millis(150),
                    PLAZO_CORTO,
                    lector.clone(),
                ));
                // La seguridad no depende de la lectura: avisa dos veces.
                avisos.push(siguiente(&mut rx, ESPERA_CAMBIO).await);
                avisos.push(siguiente(&mut rx, ESPERA_CAMBIO).await);
                tarea.abort();
            }
            avisos
        })
    });
    assert_eq!(resultado, vec![Some(7); 4], "cae a la pasada de seguridad");
    assert!(
        lector.intentos.load(Ordering::SeqCst) >= 4,
        "se reintenta crear el hilo en cada vuelta"
    );
    let del_hilo: Vec<&String> = lineas
        .iter()
        .filter(|linea| linea.contains("no se pudo crear el hilo"))
        .collect();
    assert_eq!(del_hilo.len(), 1, "una sola vez por sesión: {lineas:#?}");
    assert!(del_hilo[0].contains("WARN"), "{}", del_hilo[0]);
    assert!(
        !del_hilo[0].contains('/'),
        "el aviso no lleva rutas: {}",
        del_hilo[0]
    );
}

#[test]
fn termux_pide_una_pasada_al_volver_a_la_vista_solo_si_no_espera_una_decision() {
    // Solo en Termux.
    assert!(debe_pedir_pasada_al_volver(
        true,
        &EstadoPasada::AlDia,
        false
    ));
    assert!(debe_pedir_pasada_al_volver(
        true,
        &EstadoPasada::Error("la carpeta no existe".to_string()),
        false
    ));
    assert!(debe_pedir_pasada_al_volver(
        true,
        &EstadoPasada::SinConfigurar,
        false
    ));
    // En el escritorio, nunca: no cambia nada.
    for estado in [
        EstadoPasada::AlDia,
        EstadoPasada::SinClave,
        EstadoPasada::Error("x".to_string()),
    ] {
        assert!(!debe_pedir_pasada_al_volver(false, &estado, false));
    }
    // Con una decisión pendiente, otra pasada no la resuelve.
    assert!(!debe_pedir_pasada_al_volver(
        true,
        &EstadoPasada::AlDia,
        true
    ));
    assert!(!debe_pedir_pasada_al_volver(
        true,
        &EstadoPasada::Quitado,
        false
    ));
    assert!(!debe_pedir_pasada_al_volver(
        true,
        &EstadoPasada::EsperandoClave("otro".to_string()),
        false
    ));
}

#[test]
fn al_volver_a_la_vista_sincronizacion_en_termux_se_pide_una_pasada() {
    termux();
    let temporal = tempfile::tempdir().unwrap();
    let hogar = temporal.path().join("home");
    std::fs::create_dir_all(&hogar).unwrap();
    let (_otro, rutas) = entorno_app(&hogar);
    let (mut app, mut terminal) = con_app(&rutas, Config::default());
    assert_eq!(app.sincronizacion.pasadas_pedidas, 0);
    abrir_paleta(&mut app, &mut terminal, "sincronización");
    assert_eq!(app.vista, Vista::Sincronizacion);
    assert_eq!(app.sincronizacion.pasadas_pedidas, 1, "al entrar");
    // Volver a otra vista y regresar la pide otra vez.
    tecla(&mut app, &mut terminal, KeyCode::Esc);
    assert_ne!(app.vista, Vista::Sincronizacion);
    abrir_paleta(&mut app, &mut terminal, "sincronización");
    assert_eq!(app.vista, Vista::Sincronizacion);
    assert_eq!(app.sincronizacion.pasadas_pedidas, 2, "al volver");
}

// ====================================================================
// C. Los permisos que el almacenamiento compartido no admite (§3.2)
// ====================================================================

#[test]
fn la_escritura_tolerante_de_la_carpeta_no_falla_si_el_chmod_falla() {
    let (_temporal, carpeta) = carpeta_compartida();
    let uuid = UUID_A;
    // Estricta: el `chmod` que falla es un error y no queda nada.
    let error = carpeta::escribir_con_ajuste(&carpeta, uuid, b"datos", false, &chmod_que_falla)
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("ajustando permisos"),
        "{error:#}"
    );
    assert_eq!(
        std::fs::read_dir(&carpeta).unwrap().count(),
        0,
        "sin restos"
    );
    // Tolerante: se escribe y dice que ignoró el chmod.
    let (ruta, ignorado) =
        carpeta::escribir_con_ajuste(&carpeta, uuid, b"datos", true, &chmod_que_falla).unwrap();
    assert!(ignorado);
    assert_eq!(std::fs::read(&ruta).unwrap(), b"datos");
    assert_eq!(
        std::fs::read_dir(&carpeta).unwrap().count(),
        1,
        "sin temporales"
    );
    // Con un `chmod` que funciona, ni tolerante lo ignora y queda en 600.
    let (ruta, ignorado) = carpeta::escribir_tolerante(&carpeta, UUID_B, b"otros").unwrap();
    assert!(!ignorado);
    use std::os::unix::fs::PermissionsExt as _;
    assert_eq!(
        std::fs::metadata(&ruta).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // Lo demás (nombre no canónico) sigue siendo un error aunque sea tolerante.
    assert!(carpeta::escribir_tolerante(&carpeta, "../fuera", b"x").is_err());
}

#[test]
fn con_permisos_estrictos_un_chmod_que_falla_es_un_error_de_la_pasada() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = texto_clave();
    let equipo = Entorno::nuevo(&carpeta, "escritorio", "linux", &clave);
    equipo.crear_host("web", "192.0.2.5");
    let resultado = equipo.pasada_con_ajuste(
        PermisosCarpeta::Estrictos,
        &AvisoUnico::nuevo(),
        &chmod_que_falla,
    );
    match &resultado.estado {
        EstadoPasada::Error(motivo) => assert!(
            motivo.contains("no se pudo escribir el fichero de este equipo"),
            "{motivo}"
        ),
        otro => panic!("fuera de Termux un chmod que falla es un error: {otro:?}"),
    }
    assert!(!resultado.exportado);
    assert!(!equipo.fichero_propio(&carpeta).exists());
    assert_eq!(
        std::fs::read_dir(&carpeta).unwrap().count(),
        0,
        "sin restos"
    );
}

#[test]
fn con_permisos_tolerantes_un_chmod_que_falla_no_es_un_error_y_el_fichero_se_lee() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = texto_clave();
    let termux = Entorno::nuevo(&carpeta, "termux-abc-x10", "android", &clave);
    termux.crear_host("web", "192.0.2.5");
    let resultado = termux.pasada_con_ajuste(
        PermisosCarpeta::Tolerantes,
        &AvisoUnico::nuevo(),
        &chmod_que_falla,
    );
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(resultado.exportado, "el fichero propio se escribió");
    assert!(termux.fichero_propio(&carpeta).exists());
    assert_eq!(
        std::fs::read_dir(&carpeta).unwrap().count(),
        1,
        "sin restos"
    );
    // Y otro equipo lo lee como cualquier otro.
    let escritorio = Entorno::nuevo(&carpeta, "escritorio", "linux", &clave);
    let leido = escritorio.pasada_con(PermisosCarpeta::Estrictos);
    assert_eq!(leido.estado, EstadoPasada::AlDia, "{:?}", leido.avisos);
    assert_eq!(
        escritorio.hosts(),
        vec![("web".to_string(), "192.0.2.5".to_string())]
    );
}

#[test]
fn con_permisos_tolerantes_y_un_chmod_que_funciona_el_fichero_queda_en_600() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = texto_clave();
    let termux = Entorno::nuevo(&carpeta, "termux-abc-x10", "android", &clave);
    termux.crear_host("web", "192.0.2.5");
    let (resultado, lineas) = con_log(|| termux.pasada_con(PermisosCarpeta::Tolerantes));
    assert!(resultado.exportado);
    use std::os::unix::fs::PermissionsExt as _;
    assert_eq!(
        std::fs::metadata(termux.fichero_propio(&carpeta))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(
        lineas.is_empty(),
        "sin nada que decir en el log: {lineas:?}"
    );
}

#[test]
fn el_log_dice_una_sola_vez_por_sesion_que_la_carpeta_no_admite_permisos() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = texto_clave();
    let termux = Entorno::nuevo(&carpeta, "termux-abc-x10", "android", &clave);
    let sesion = AvisoUnico::nuevo();
    let (_, lineas) = con_log(|| {
        // Tres pasadas que escriben el fichero propio (cada una con un cambio
        // de inventario) en la misma sesión.
        for (nombre, direccion) in [
            ("web", "192.0.2.5"),
            ("base-secreta", "192.0.2.6"),
            ("cache", "192.0.2.7"),
        ] {
            termux.crear_host(nombre, direccion);
            let resultado =
                termux.pasada_con_ajuste(PermisosCarpeta::Tolerantes, &sesion, &chmod_que_falla);
            assert_eq!(resultado.estado, EstadoPasada::AlDia);
            assert!(resultado.exportado);
        }
    });
    assert_eq!(lineas.len(), 1, "una sola línea por sesión: {lineas:?}");
    let linea = &lineas[0];
    assert!(linea.contains(AVISO_PERMISOS_IGNORADOS), "{linea}");
    // Sin rutas ni datos del inventario.
    for prohibido in ["web", "base-secreta", "cache", "192.0.2", "sync"] {
        assert!(!linea.contains(prohibido), "{prohibido} en «{linea}»");
    }
    assert!(
        !linea.contains(&carpeta.to_string_lossy().to_string()),
        "{linea}"
    );
    // Otra sesión de MAGI (otro aviso) lo dice una vez más, no más.
    let otra_sesion = AvisoUnico::nuevo();
    let (_, lineas) = con_log(|| {
        for direccion in ["192.0.2.8", "192.0.2.9"] {
            termux.editar_direccion("web", direccion);
            let resultado = termux.pasada_con_ajuste(
                PermisosCarpeta::Tolerantes,
                &otra_sesion,
                &chmod_que_falla,
            );
            assert!(resultado.exportado);
        }
    });
    assert_eq!(lineas.len(), 1, "{lineas:?}");
}

#[test]
fn con_permisos_estrictos_no_hay_aviso_en_el_log() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = texto_clave();
    let escritorio = Entorno::nuevo(&carpeta, "escritorio", "linux", &clave);
    escritorio.crear_host("web", "192.0.2.5");
    let (resultado, lineas) = con_log(|| {
        escritorio.pasada_con_ajuste(
            PermisosCarpeta::Estrictos,
            &AvisoUnico::nuevo(),
            &chmod_que_falla,
        )
    });
    assert!(matches!(resultado.estado, EstadoPasada::Error(_)));
    assert!(lineas.is_empty(), "{lineas:?}");
}

#[test]
fn el_aviso_unico_solo_deja_pasar_a_la_primera() {
    let aviso = AvisoUnico::nuevo();
    assert!(aviso.primera_vez());
    assert!(!aviso.primera_vez());
    assert!(!aviso.primera_vez());
}

#[test]
fn en_termux_la_pasada_de_la_interfaz_y_de_la_cli_es_tolerante() {
    termux();
    assert_eq!(permisos_de_esta_plataforma(), PermisosCarpeta::Tolerantes);
    // Y el valor por omisión del núcleo, que es el de los demás, es estricto.
    assert_eq!(PermisosCarpeta::default(), PermisosCarpeta::Estrictos);
}

/// Cierre de la Fase 17 (C1): la carpeta del móvil es almacenamiento compartido
/// donde escribe cualquier app. Con nombre de fichero de equipo caben una
/// carpeta, un FIFO, un fichero enorme (disperso) y uno ilegible: ninguno
/// tumba la pasada, que sigue importando los ficheros buenos.
#[test]
fn una_carpeta_con_basura_con_nombre_de_magi_no_tumba_la_pasada() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = texto_clave();
    let termux = Entorno::nuevo(&carpeta, "termux-abc-x10", "android", &clave);
    termux.crear_host("web", "192.0.2.5");
    assert!(termux.pasada_con(PermisosCarpeta::Tolerantes).exportado);

    // Lo que dejan otras apps, con nombre de fichero de equipo.
    let uuid_directorio = "11111111-1111-4111-8111-111111111111";
    let uuid_fifo = "33333333-3333-4333-8333-333333333333";
    let uuid_grande = "44444444-4444-4444-8444-444444444444";
    let uuid_ilegible = "55555555-5555-4555-8555-555555555555";
    std::fs::create_dir(carpeta.join(carpeta::nombre_fichero(uuid_directorio))).unwrap();
    nix::unistd::mkfifo(
        &carpeta.join(carpeta::nombre_fichero(uuid_fifo)),
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )
    .unwrap();
    std::fs::File::create(carpeta.join(carpeta::nombre_fichero(uuid_grande)))
        .unwrap()
        .set_len(300 * 1024 * 1024)
        .unwrap();
    let ilegible = carpeta.join(carpeta::nombre_fichero(uuid_ilegible));
    std::fs::write(&ilegible, b"no lo puedo leer").unwrap();
    std::fs::set_permissions(&ilegible, std::fs::Permissions::from_mode(0o000)).unwrap();

    let escritorio = Entorno::nuevo(&carpeta, "escritorio", "linux", &clave);
    let resultado = escritorio.pasada_con(PermisosCarpeta::Estrictos);
    assert_eq!(
        resultado.estado,
        EstadoPasada::AlDia,
        "la basura no tumba la pasada: {:?}",
        resultado.avisos
    );
    assert_eq!(
        escritorio.hosts(),
        vec![("web".to_string(), "192.0.2.5".to_string())],
        "los ficheros buenos se importan"
    );
    assert!(
        resultado
            .avisos
            .iter()
            .any(|aviso| aviso.contains(uuid_grande) && aviso.contains("dañado")),
        "el fichero enorme consta como dañado, sin haberlo leído: {:?}",
        resultado.avisos
    );
    if !es_root() {
        assert!(
            resultado
                .avisos
                .iter()
                .any(|aviso| aviso.contains(&carpeta::nombre_fichero(uuid_ilegible))),
            "el ilegible consta como aviso: {:?}",
            resultado.avisos
        );
    }
    // La carpeta y el FIFO no son ficheros de equipo: ni un aviso.
    assert!(
        !resultado
            .avisos
            .iter()
            .any(|aviso| aviso.contains(uuid_directorio) || aviso.contains(uuid_fifo)),
        "{:?}",
        resultado.avisos
    );
    // Para que el directorio temporal se pueda borrar.
    std::fs::set_permissions(&ilegible, std::fs::Permissions::from_mode(0o600)).unwrap();
}

// ====================================================================
// D. Equipo, plataforma y formato (§3.2, §7.8)
// ====================================================================

#[test]
fn un_movil_con_modelo_abc_x10_crea_y_une_como_termux_abc_x10() {
    termux();
    let modelo = Propiedades::fijas(&[("ro.product.model", "ABC-X10")])
        .modelo()
        .expect("el modelo");
    let nombre = magi::termux::nombre_equipo(&modelo);
    assert_eq!(nombre, "termux-abc-x10");
    // La identidad que da `almacen::abrir` en Termux: plataforma android.
    let plataforma = magi::config::plataforma_actual();
    assert_eq!(plataforma, "android");

    let (_temporal, carpeta) = carpeta_compartida();
    // Crear: el equipo de Termux crea la sincronización de una carpeta vacía.
    let creador = Entorno::nuevo(&carpeta, &nombre, plataforma, &texto_clave());
    let llavero = LlaveroEnMemoria::nuevo();
    let mut config = creador.config.clone();
    config.sincronizacion.carpeta = None;
    let clave = equipos::crear(
        creador.conexion(),
        &creador.rutas,
        &carpeta,
        &mut config,
        &llavero,
    )
    .expect("crear");
    let creador = Entorno {
        config,
        texto_clave: clave.texto().to_string(),
        ..creador
    };
    creador.crear_host("web", "192.0.2.5");
    assert_eq!(
        creador.pasada_con(PermisosCarpeta::Tolerantes).estado,
        EstadoPasada::AlDia
    );
    let (documento, formato_escrito) = documento_de(&carpeta, &creador.este().uuid, &clave);
    assert_eq!(documento.equipo.nombre, "termux-abc-x10");
    assert_eq!(documento.equipo.plataforma, "android");
    assert_eq!(formato_escrito, formato::FORMATO_1, "sin formato nuevo");

    // Unir: otro móvil del mismo modelo se une con la clave del primero.
    let (_otro, carpeta_dos) = carpeta_compartida();
    let primero = Entorno::nuevo(&carpeta_dos, "escritorio", "linux", &texto_clave());
    let mut config_primero = primero.config.clone();
    config_primero.sincronizacion.carpeta = None;
    let llavero_primero = LlaveroEnMemoria::nuevo();
    let clave_dos = equipos::crear(
        primero.conexion(),
        &primero.rutas,
        &carpeta_dos,
        &mut config_primero,
        &llavero_primero,
    )
    .expect("crear el escritorio");
    let primero = Entorno {
        config: config_primero,
        texto_clave: clave_dos.texto().to_string(),
        ..primero
    };
    primero.crear_host("db", "192.0.2.6");
    primero.pasada_con(PermisosCarpeta::Estrictos);

    let uniendose = Entorno::nuevo(&carpeta_dos, &nombre, plataforma, &clave_dos.texto());
    let mut config_unido = uniendose.config.clone();
    config_unido.sincronizacion.carpeta = None;
    let llavero_unido = LlaveroEnMemoria::nuevo();
    equipos::unir(
        uniendose.conexion(),
        &uniendose.rutas,
        &carpeta_dos,
        clave_dos.texto().as_str(),
        &mut config_unido,
        &llavero_unido,
    )
    .expect("unir");
    assert!(
        llavero_unido
            .recuperar(CUENTA_SINCRONIZACION)
            .unwrap()
            .is_some(),
        "la clave va al llavero"
    );
    let uniendose = Entorno {
        config: config_unido,
        ..uniendose
    };
    assert_eq!(
        uniendose.pasada_con(PermisosCarpeta::Tolerantes).estado,
        EstadoPasada::AlDia
    );
    let (documento, formato_escrito) =
        documento_de(&carpeta_dos, &uniendose.este().uuid, &clave_dos);
    assert_eq!(documento.equipo.nombre, "termux-abc-x10");
    assert_eq!(documento.equipo.plataforma, "android");
    assert_eq!(formato_escrito, formato::FORMATO_1);
    assert_eq!(
        uniendose.hosts(),
        vec![("db".to_string(), "192.0.2.6".to_string())],
        "se unió y trajo el inventario del otro equipo"
    );
}

#[test]
fn el_almacen_de_termux_nace_como_termux_android_y_sin_modelo_se_llama_termux() {
    termux();
    let temporal = tempfile::tempdir().unwrap();
    let ruta = temporal.path().join("magi.db");
    let almacen = magi::almacen::abrir(&ruta).expect("abrir");
    let este = equipos_bd::este(almacen.conexion()).unwrap();
    assert_eq!(este.plataforma, "android");
    // En Linux no hay propiedades de Android: nunca el `localhost` de Termux.
    #[cfg(not(target_os = "android"))]
    assert_eq!(este.nombre, "termux");
    assert_eq!(este.nombre, magi::config::nombre_actual());
}

// ---------------------------------------------- un fichero de Termux se lee

/// Un inventario variado, con un escritorio (formato 2) si se pide.
fn inventario(equipo: &Entorno, con_escritorio: bool) {
    equipo.crear_host("web", "192.0.2.5");
    equipo.crear_host("db", "192.0.2.6");
    if con_escritorio {
        equipo.crear_host_rdp("oficina", "192.0.2.20");
    }
}

#[test]
fn el_fichero_de_termux_lo_leen_el_escritorio_y_el_movil_sin_formato_nuevo() {
    termux();
    for con_escritorio in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let texto = texto_clave();
        let clave = ClaveSincronizacion::desde_texto(&texto).unwrap();
        let termux = Entorno::nuevo(&carpeta, "termux-abc-x10", "android", &texto);
        inventario(&termux, con_escritorio);
        let escrita = termux.pasada_con(PermisosCarpeta::Tolerantes);
        assert_eq!(escrita.estado, EstadoPasada::AlDia);
        assert!(escrita.exportado);

        // El `formato` es el que escribiría un equipo de escritorio con el mismo
        // inventario: la plataforma no lo cambia.
        let (_otra, carpeta_linux) = carpeta_compartida();
        let linux = Entorno::nuevo(&carpeta_linux, "escritorio", "linux", &texto);
        inventario(&linux, con_escritorio);
        linux.pasada_con(PermisosCarpeta::Estrictos);
        let (_, formato_termux) = documento_de(&carpeta, &termux.este().uuid, &clave);
        let (_, formato_linux) = documento_de(&carpeta_linux, &linux.este().uuid, &clave);
        assert_eq!(formato_termux, formato_linux);
        assert!(formato_termux <= formato::FORMATO_MAXIMO);
        assert_eq!(
            formato_termux,
            if con_escritorio {
                formato::FORMATO_2
            } else {
                formato::FORMATO_1
            }
        );

        // Un escritorio (el código actual de la pasada del escritorio) lo
        // importa sin «formato nuevo» ni avisos.
        let escritorio = Entorno::nuevo(&carpeta, "escritorio", "linux", &texto);
        let leida = escritorio.pasada_con(PermisosCarpeta::Estrictos);
        assert_eq!(leida.estado, EstadoPasada::AlDia, "{:?}", leida.avisos);
        assert!(leida.avisos.is_empty(), "{:?}", leida.avisos);
        assert_eq!(escritorio.hosts(), termux.hosts());
        let fila = equipos_bd::por_uuid(escritorio.conexion(), &termux.este().uuid)
            .unwrap()
            .expect("el equipo de Termux se conoce");
        assert_eq!(fila.estado, equipos_bd::ESTADO_AL_DIA);
        assert_eq!(fila.nombre, "termux-abc-x10");
        assert_eq!(fila.plataforma, "android");

        // El móvil lector (el código del núcleo que usa la app Android) también.
        let movil = Entorno::nuevo(&carpeta, "pixel", "android", &texto);
        let lectura = lector::pasada_lector(
            movil.conexion(),
            &movil.rutas,
            &magi::sincronizacion::carpeta::CarpetaDisco::nueva(&carpeta),
            &movil.claves(),
            &SinTrabajo,
        )
        .expect("la pasada del lector");
        assert_eq!(lectura.estado, EstadoLectura::AlDia, "{:?}", lectura.avisos);
        assert!(lectura.avisos.is_empty(), "{:?}", lectura.avisos);
        assert_eq!(movil.hosts(), termux.hosts());
        let fila = equipos_bd::por_uuid(movil.conexion(), &termux.este().uuid)
            .unwrap()
            .expect("el equipo de Termux se conoce");
        assert_eq!(fila.estado, equipos_bd::ESTADO_AL_DIA);
    }
}

// ----------------------- el núcleo no decide nada por EQUIPOS.plataforma
//
// Qué prueba cada cosa de esta sección, sin más:
// - Las pruebas de sincronización meten la plataforma en la capa que de verdad
//   la lee (la fila de EQUIPOS, que la pasada copia al fichero y a la fila de
//   los demás equipos) y comparan lo observable con `linux` y con `android`.
// - Las del sondeo y la deliberación solo prueban que lo que se guarda en un
//   almacén y se relee no depende de lo que diga la fila de EQUIPOS de ese
//   almacén: `aplicar_metricas`, `evaluar` y `lanzar` no reciben ninguna
//   plataforma, así que no hay otra capa que ejercitar con ella.
// - Que el código del núcleo y del móvil no decida por la plataforma lo
//   garantiza la guarda del código fuente, al final de la sección.

/// Lo que se ve de una pasada, sin lo que cambia de una ejecución a otra.
#[derive(Debug, PartialEq)]
struct PasoObservado {
    estado: String,
    exportado: bool,
    resumen: Option<ResumenImportacion>,
    avisos: Vec<String>,
    conflictos: i64,
}

fn observar(resultado: &ResultadoPasada) -> PasoObservado {
    PasoObservado {
        estado: format!("{:?}", resultado.estado),
        exportado: resultado.exportado,
        resumen: resultado.resumen.clone(),
        avisos: resultado.avisos.clone(),
        conflictos: resultado.conflictos_pendientes,
    }
}

#[derive(Debug, PartialEq)]
struct ObservadoSincronizacion {
    pasos: Vec<PasoObservado>,
    hosts_del_primero: Vec<(String, String)>,
    hosts_del_segundo: Vec<(String, String)>,
    formatos: Vec<u32>,
    estados_vistos: Vec<String>,
}

/// Dos equipos, uno con la plataforma que se pide, intercambian altas,
/// ediciones y un borrado por una carpeta; el que escribe primero lleva
/// `plataforma`, el otro es Linux, y se repite con los papeles cambiados.
fn escenario_de_sincronizacion(plataforma: &str) -> ObservadoSincronizacion {
    let (_temporal, carpeta) = carpeta_compartida();
    let texto = texto_clave();
    let clave = ClaveSincronizacion::desde_texto(&texto).unwrap();
    let primero = Entorno::nuevo(&carpeta, "equipo-a", plataforma, &texto);
    let segundo = Entorno::nuevo(&carpeta, "equipo-b", "linux", &texto);
    let estricta = PermisosCarpeta::Estrictos;
    let mut pasos = Vec::new();

    inventario(&primero, true);
    pasos.push(observar(&primero.pasada_con(estricta)));
    pasos.push(observar(&segundo.pasada_con(estricta)));
    primero.editar_direccion("web", "192.0.2.50");
    pasos.push(observar(&primero.pasada_con(estricta)));
    pasos.push(observar(&segundo.pasada_con(estricta)));
    segundo.borrar_host("db");
    segundo.crear_host("monitor", "192.0.2.30");
    pasos.push(observar(&segundo.pasada_con(estricta)));
    pasos.push(observar(&primero.pasada_con(estricta)));
    // Una pasada más sin cambios: no reexporta.
    pasos.push(observar(&primero.pasada_con(estricta)));

    let formatos = vec![
        documento_de(&carpeta, &primero.este().uuid, &clave).1,
        documento_de(&carpeta, &segundo.este().uuid, &clave).1,
    ];
    let estados_vistos = vec![
        equipos_bd::por_uuid(segundo.conexion(), &primero.este().uuid)
            .unwrap()
            .unwrap()
            .estado,
        equipos_bd::por_uuid(primero.conexion(), &segundo.este().uuid)
            .unwrap()
            .unwrap()
            .estado,
    ];
    ObservadoSincronizacion {
        pasos,
        hosts_del_primero: primero.hosts(),
        hosts_del_segundo: segundo.hosts(),
        formatos,
        estados_vistos,
    }
}

#[test]
fn la_sincronizacion_da_lo_mismo_con_android_inyectado_que_con_linux() {
    let con_linux = escenario_de_sincronizacion("linux");
    let con_android = escenario_de_sincronizacion("android");
    assert_eq!(con_android, con_linux);
    // Y el escenario sirve de algo: hubo importaciones y un borrado.
    assert!(con_android.pasos.iter().any(|paso| paso.exportado));
    assert!(con_android.pasos.iter().any(|paso| paso
        .resumen
        .as_ref()
        .is_some_and(|resumen| resumen.creados > 0)));
    assert!(con_android.pasos.iter().any(|paso| paso
        .resumen
        .as_ref()
        .is_some_and(|resumen| resumen.borrados > 0)));
    assert_eq!(
        con_android.hosts_del_primero, con_android.hosts_del_segundo,
        "convergen"
    );
}

/// El sondeo de la flota: una salida grabada del script, leída, guardada y
/// evaluada en un almacén cuya fila de EQUIPOS lleva la plataforma que se pide
/// (la fila no la lee nada de esto: es el control de que guardar y releer no
/// depende de ella).
fn escenario_de_sondeo(plataforma: &str) -> String {
    const SALIDA: &str = "MAGI_NUCLEOS=8\n\
        MAGI_LOADAVG=0.52 0.58 0.59 1/234 5678\n\
        MAGI_MEM_MemTotal:       16303408 kB\n\
        MAGI_MEM_MemAvailable:    8123456 kB\n\
        MAGI_DISCO=103080888 30000000\n\
        MAGI_RED=123456789 987654321\n\
        MAGI_UPTIME=123456.78\n\
        MAGI_SVC=nginx=active\n\
        MAGI_SVC=penwork=failed\n";
    let almacen = Almacen::abrir_en_memoria_con(&Equipo::nuevo("equipo", plataforma), "1.5.0")
        .expect("almacén");
    let host_id = almacen
        .crear_host(
            &DatosHost {
                nombre: "web".to_string(),
                direccion: "192.0.2.5".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
    let mut sondeo = Sondeo::vacio(host_id);
    aplicar_metricas(&mut sondeo, parsear(SALIDA));
    sondeo.fecha = "2026-11-18T10:00:00+00:00".to_string();
    sondeos::guardar(almacen.conexion(), &sondeo).unwrap();
    let guardado = sondeos::ultimo(almacen.conexion(), host_id)
        .unwrap()
        .expect("el sondeo guardado");
    let (estado, motivos) = evaluar(Some(&guardado), &Umbrales::default());
    format!(
        "{:?} · {:?} · {motivos:?} · {:?}",
        guardado.resultado, estado, guardado.servicios
    )
}

#[test]
fn el_sondeo_guardado_y_evaluado_no_depende_de_la_fila_de_equipos_del_almacen() {
    let con_linux = escenario_de_sondeo("linux");
    let con_android = escenario_de_sondeo("android");
    assert_eq!(con_android, con_linux);
    assert!(con_android.contains("penwork"), "{con_android}");
}

/// Proveedores de la deliberación con datos fijos.
struct ProveedoresFijos {
    sondeo: Sondeo,
    listado: Vec<EntradaListado>,
}

impl Proveedores for ProveedoresFijos {
    fn sondear(&self, _host_id: i64, _excluir: Vec<String>) -> Futuro<Sondeo> {
        let sondeo = self.sondeo.clone();
        Box::pin(async move { sondeo })
    }

    fn listar(&self, _host_id: i64, _ruta: String) -> Futuro<Result<Vec<EntradaListado>, String>> {
        let listado = self.listado.clone();
        Box::pin(async move { Ok(listado) })
    }

    fn tests(
        &self,
        _host_id: i64,
        comando: String,
    ) -> Futuro<Result<(bool, String), NoDisponible>> {
        // CASPER-3 con el `sh` de siempre y el plazo de la deliberación.
        Box::pin(async move {
            let limite = tokio::time::Instant::now() + Duration::from_secs(5);
            Ok(magi::deliberacion::tests::ejecutar(&comando, limite, Path::new("/")).await)
        })
    }
}

/// La deliberación de una acción con las tres comprobaciones, guardada y
/// releída de un almacén cuya fila de EQUIPOS lleva la plataforma que se pide
/// (como en el sondeo, la fila no la lee nada de esto).
fn escenario_de_deliberacion(plataforma: &str) -> String {
    let almacen = Almacen::abrir_en_memoria_con(&Equipo::nuevo("equipo", plataforma), "1.5.0")
        .expect("almacén");
    let host_id = almacen
        .crear_host(
            &DatosHost {
                nombre: "web".to_string(),
                direccion: "192.0.2.5".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
    let mut sondeo = Sondeo::vacio(host_id);
    aplicar_metricas(
        &mut sondeo,
        parsear(
            "MAGI_NUCLEOS=4\nMAGI_LOADAVG=0.10 0.10 0.10 1/1 1\n\
             MAGI_MEM_MemTotal: 1000000 kB\nMAGI_MEM_MemAvailable: 900000 kB\n\
             MAGI_DISCO=1000000 100000\nMAGI_UPTIME=1000.0\n",
        ),
    );
    let ahora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let proveedores = Arc::new(ProveedoresFijos {
        sondeo,
        listado: vec![EntradaListado {
            nombre: "pg.sql.gz".to_string(),
            tipo: TipoEntradaListado::Fichero,
            tamano: 10,
            mtime: ahora - 3600,
        }],
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<ResultadoComprobacion>();
    let tx = Mutex::new(tx);
    let limite = tokio::time::Instant::now() + Duration::from_secs(10);
    let _tareas = lanzar(
        runtime.handle(),
        1,
        vec![
            (
                host_id,
                Comprobacion::Salud,
                Trabajo::Salud {
                    excluir: Vec::new(),
                },
            ),
            (
                host_id,
                Comprobacion::Backup,
                Trabajo::Backup {
                    ruta: "/backups".to_string(),
                    patron: Some("*.sql.gz".to_string()),
                },
            ),
            (
                host_id,
                Comprobacion::Tests,
                Trabajo::Tests {
                    comando: "true".to_string(),
                },
            ),
        ],
        proveedores,
        Limites::default(),
        Umbrales::default(),
        limite,
        Arc::new(move |resultado| {
            let _ = tx.lock().unwrap().send(resultado);
        }),
    );
    let mut celdas = Vec::new();
    for _ in 0..3 {
        celdas.push(
            rx.recv_timeout(Duration::from_secs(10))
                .expect("un resultado"),
        );
    }
    let celda = |comprobacion: Comprobacion| {
        let veredicto = celdas
            .iter()
            .find(|celda| celda.comprobacion == comprobacion)
            .expect("la celda")
            .veredicto
            .clone();
        // Lo que cambia entre ejecuciones: los milisegundos y la duración que
        // cierra el detalle de la salud.
        match veredicto {
            Veredicto::Aprueba { detalle, .. } => Veredicto::Aprueba {
                detalle: detalle.split(" · ").next().unwrap_or("").to_string(),
                ms: 0,
            },
            Veredicto::Rechaza { detalle, .. } => Veredicto::Rechaza { detalle, ms: 0 },
            otro => otro,
        }
    };
    let comprobaciones = vec![ComprobacionesHost {
        host_id,
        host: "web".to_string(),
        salud: celda(Comprobacion::Salud),
        backup: celda(Comprobacion::Backup),
        tests: celda(Comprobacion::Tests),
    }];
    let id = magi::almacen::deliberaciones::crear(
        almacen.conexion(),
        &NuevaDeliberacion {
            snippet_id: None,
            accion: "reiniciar nginx → web".to_string(),
            hosts: vec![(host_id, "web".to_string())],
            comprobaciones,
            resultado: ResultadoDeliberacion::Aprobada,
            bloqueada: false,
            motivo: None,
            usuario: "usuario".to_string(),
        },
    )
    .unwrap();
    let registro = magi::almacen::deliberaciones::obtener(almacen.conexion(), id).unwrap();
    format!(
        "{:?} · {:?} · {:?} · {:?} · {:?}",
        registro.comprobaciones,
        registro.resultado,
        registro.consenso(),
        registro.bloqueada,
        registro.usuario
    )
}

#[test]
fn la_deliberacion_guardada_no_depende_de_la_fila_de_equipos_del_almacen() {
    let con_linux = escenario_de_deliberacion("linux");
    let con_android = escenario_de_deliberacion("android");
    assert_eq!(con_android, con_linux);
    assert!(con_android.contains("(3, 3)"), "unanimidad: {con_android}");
}

#[test]
fn con_android_de_lector_las_pasadas_tambien_dan_lo_mismo_que_con_linux() {
    // El otro papel: el equipo de Termux importa lo que escribe un escritorio.
    let resultados: Vec<Vec<PasoObservado>> = ["linux", "android"]
        .into_iter()
        .map(|plataforma| {
            let (_temporal, carpeta) = carpeta_compartida();
            let texto = texto_clave();
            let escritor = Entorno::nuevo(&carpeta, "escritorio", "linux", &texto);
            let lector = Entorno::nuevo(&carpeta, "movil", plataforma, &texto);
            inventario(&escritor, true);
            let estricta = PermisosCarpeta::Estrictos;
            vec![
                observar(&escritor.pasada_con(estricta)),
                observar(&lector.pasada_con(estricta)),
                observar(&lector.pasada_con(estricta)),
            ]
        })
        .collect();
    assert_eq!(resultados[0], resultados[1]);
}

/// Todos los `.rs` de un directorio, de forma recursiva.
fn ficheros_rs(directorio: &Path, salida: &mut Vec<PathBuf>) {
    for entrada in std::fs::read_dir(directorio).expect("leyendo el código") {
        let ruta = entrada.expect("entrada").path();
        if ruta.is_dir() {
            ficheros_rs(&ruta, salida);
        } else if ruta.extension().is_some_and(|extension| extension == "rs") {
            salida.push(ruta);
        }
    }
}

/// ¿La línea de código decide algo con la plataforma (la compara, la
/// pregunta en un `if` o un `match`, la trata como texto)?
fn decide_por_la_plataforma(linea: &str) -> bool {
    if !linea.contains("plataforma") {
        return false;
    }
    let comparacion = [
        "plataforma ==",
        "plataforma !=",
        "== plataforma",
        "!= plataforma",
    ];
    let como_texto = [
        "plataforma.as_str",
        "plataforma.starts_with",
        "plataforma.ends_with",
        "plataforma.contains",
        "plataforma.eq",
        "plataforma.to_lowercase",
    ];
    let rama = ["if ", "match ", "while ", "matches!("];
    comparacion.iter().any(|patron| linea.contains(patron))
        || como_texto.iter().any(|patron| linea.contains(patron))
        || rama.iter().any(|patron| linea.contains(patron))
}

#[test]
fn el_codigo_del_nucleo_y_del_movil_no_decide_nada_por_la_plataforma_del_equipo() {
    // La garantía es del código, no solo de lo que prueban los escenarios: la
    // plataforma de un equipo se guarda, se copia al fichero y se enseña, pero
    // ninguna rama del núcleo ni del móvil depende de ella. Si esta prueba
    // falla porque alguien añadió una comparación, el requisito (la
    // plataforma `android` de Termux no cambia nada) ya no se cumple.
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates");
    let mut ficheros = Vec::new();
    ficheros_rs(&raiz.join("magi-nucleo/src"), &mut ficheros);
    ficheros_rs(&raiz.join("magi-movil/src"), &mut ficheros);
    assert!(ficheros.len() > 50, "no se encontró el código del núcleo");
    let mut decisiones = Vec::new();
    let mut literales = Vec::new();
    for fichero in &ficheros {
        let texto = std::fs::read_to_string(fichero).expect("leyendo un fichero");
        for (numero, linea) in texto.lines().enumerate() {
            let linea = linea.trim();
            if linea.starts_with("//") {
                continue;
            }
            let sitio = format!("{}:{}: {linea}", fichero.display(), numero + 1);
            if decide_por_la_plataforma(linea) {
                decisiones.push(sitio.clone());
            }
            if ["\"linux\"", "\"macos\"", "\"android\""]
                .iter()
                .any(|literal| linea.contains(literal))
            {
                literales.push(sitio);
            }
        }
    }
    assert!(
        decisiones.is_empty(),
        "algo del núcleo o del móvil decide por la plataforma:\n{}",
        decisiones.join("\n")
    );
    // Los únicos literales de plataforma: cómo se llama este sistema
    // (`plataforma_actual`, por el sistema operativo con el que se compiló) y
    // la identidad de pruebas o de un equipo recién visto sin nombre.
    let permitidos = [
        "almacen/equipos.rs",
        "almacen/mod.rs",
        "sincronizacion/mod.rs",
        "escritor.rs",
    ];
    let intrusos: Vec<&String> = literales
        .iter()
        .filter(|sitio| !permitidos.iter().any(|ok| sitio.contains(ok)))
        .collect();
    assert!(
        intrusos.is_empty(),
        "literales de plataforma fuera de los sitios conocidos:\n{intrusos:#?}"
    );
}

#[test]
fn la_guarda_de_codigo_se_da_cuenta_de_una_decision_por_la_plataforma() {
    // Que la guarda sirva: estas líneas sí son decisiones.
    for linea in [
        "if equipo.plataforma == \"android\" {",
        "match este.plataforma.as_str() {",
        "let movil = documento.equipo.plataforma != \"linux\";",
        "if plataforma.starts_with(\"and\") {",
    ] {
        assert!(decide_por_la_plataforma(linea), "{linea}");
    }
    // Y estas no: guardar, copiar y enseñar.
    for linea in [
        "equipo.plataforma.clone(),",
        "plataforma: este.plataforma,",
        "este.nombre, este.uuid, este.plataforma",
        "pub fn nuevo(nombre: impl Into<String>, plataforma: impl Into<String>) -> Self {",
    ] {
        assert!(!decide_por_la_plataforma(linea), "{linea}");
    }
}
