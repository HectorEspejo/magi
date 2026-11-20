//! Pruebas de la interfaz del llavero con frase maestra (Fase 17, Termux) sin
//! hilos de teclas, red ni servidor: `App::de_prueba` con un `LlaveroFichero`
//! de factor de trabajo bajo sobre un directorio temporal (nunca el llavero
//! del sistema) y el bucle real sobre un `TestBackend`.
//!
//! Cubren los cinco diálogos (LLAVERO DE MAGI, CREAR, CAMBIAR, BORRAR y
//! DAÑADO), los flujos del §4.2 y §4.3 (conexión interactiva, «recordar»,
//! operaciones automáticas, sincronización), las respuestas de scrypt con su
//! id, la espera tras cinco fallos, REGISTRO, la paleta, la marca de la barra
//! y el pegado. Las instantáneas de pintado están en `tests/redimensionado`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::escritorios::DialogoEscritorio;
use magi::app::llavero::{Continuacion, DialogoLlavero, EventoLlavero, ModoLlavero, UsoLlavero};
use magi::app::{AccionDialogo, AccionInventario, App, Dialogo, EntradaTextoAccion, Evento};
use magi::config::{Config, Rutas};
use magi::escritorio::{EstadoEscritorio, InfoEscritorio, TipoHost};
use magi::llavero::{
    EstadoLlavero, LlaveroCliente, LlaveroHost, LlaveroSecreto, BLOQUEADO, CUENTA_SINCRONIZACION,
    CUENTA_SINCRONIZACION_ANTERIOR,
};
use magi::modelo::{DatosHost, IdentidadRef, Origen};
use magi::protocolo::{
    EstadoSesionRemota, InfoSesion, MensajeCliente, MensajeServidor, VERSION_PROTOCOLO,
};
use magi::tema::Tema;
use magi::ui::componentes::CampoTexto;
use magi_nucleo::llavero_fichero::{
    ErrorLlavero, EstadoFichero, LlaveroFichero, FALLOS_POR_SERIE, NOMBRE_FICHERO, SIN_CREAR,
};

const FRASE: &str = "una frase maestra larga";
const OTRA_FRASE: &str = "otra frase muy distinta";
/// Factor de trabajo de scrypt de las pruebas.
const LOG_N: u8 = 6;
const CONTRASENA: &str = "clave-del-host-01";
const USUARIO: &str = "usuario";

// -------------------------------------------------------- vigilante de memoria
//
// Para comprobar de verdad que una frase se borra de la memoria (y no solo que
// el `String` queda vacío, que pasa con un simple `clear()`), el binario de
// pruebas usa un asignador que, al liberar el búfer que se le indica, mira si
// todos sus bytes son cero. Mirar los bytes justo antes de liberarlos es
// legal (el búfer sigue siendo nuestro) y no depende de qué haga después el
// asignador con la memoria. Solo se vigila un búfer a la vez: las pruebas que
// lo usan toman `CERROJO_VIGILANTE`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

struct Vigilante;

/// Dirección del búfer vigilado (0: ninguno).
static VIGILADO: AtomicUsize = AtomicUsize::new(0);
/// Qué pasó al liberarlo: 0 todavía nada, 1 todo ceros, 2 con bytes sin borrar.
static LIBERADO: AtomicUsize = AtomicUsize::new(0);
static CERROJO_VIGILANTE: Mutex<()> = Mutex::new(());

// SAFETY: delega en `System` sin cambiar ninguna de sus garantías; solo lee,
// antes de liberarla, la memoria del bloque que se le indica con `vigilar`.
unsafe impl GlobalAlloc for Vigilante {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let vigilado = VIGILADO.load(Ordering::SeqCst);
        if vigilado != 0 && ptr as usize == vigilado {
            // SAFETY: el bloque `ptr..ptr+size` sigue siendo del llamante hasta
            // que se libere abajo.
            let limpio = std::slice::from_raw_parts(ptr, layout.size())
                .iter()
                .all(|byte| *byte == 0);
            LIBERADO.store(if limpio { 1 } else { 2 }, Ordering::SeqCst);
            VIGILADO.store(0, Ordering::SeqCst);
        }
        System.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        System.alloc_zeroed(layout)
    }
}

#[global_allocator]
static ASIGNADOR: Vigilante = Vigilante;

/// Empieza a vigilar el búfer de un `String` (el siguiente que se libere con
/// esa dirección). Hay que tener tomado `CERROJO_VIGILANTE`.
fn vigilar(texto: &String) {
    assert!(texto.capacity() > 0, "no hay búfer que vigilar");
    LIBERADO.store(0, Ordering::SeqCst);
    VIGILADO.store(texto.as_ptr() as usize, Ordering::SeqCst);
}

/// Qué pasó con el búfer vigilado: `Some(true)` se liberó con todo a cero,
/// `Some(false)` se liberó con algo sin borrar, `None` todavía no se libera.
fn liberado_a_cero() -> Option<bool> {
    match LIBERADO.load(Ordering::SeqCst) {
        0 => None,
        1 => Some(true),
        _ => Some(false),
    }
}

/// Deja de vigilar (si el búfer nunca se liberó).
fn dejar_de_vigilar() {
    VIGILADO.store(0, Ordering::SeqCst);
}

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magitx")
        .tempdir()
        .expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    };
    std::fs::create_dir_all(&rutas.datos).expect("directorio de datos");
    Entorno {
        _temporal: temporal,
        rutas,
        config: Config::default(),
    }
}

struct Prueba {
    app: App,
    terminal: Terminal<TestBackend>,
    enviados: tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
    /// El llavero de fichero que se ha puesto en la App (el mismo objeto).
    fichero: Arc<LlaveroFichero>,
    entorno: Entorno,
}

impl Prueba {
    /// App con un llavero de fichero que todavía no existe (`SinCrear`) y dos
    /// hosts que guardan la contraseña en el llavero.
    fn nueva() -> Self {
        Self::con_tamano(80, 24)
    }

    fn con_tamano(cols: u16, filas: u16) -> Self {
        let entorno = entorno();
        {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
            for (nombre, direccion, identidad) in [
                ("web-01", "192.0.2.11", IdentidadRef::ContrasenaLlavero),
                ("web-02", "192.0.2.12", IdentidadRef::ContrasenaLlavero),
                ("db-01", "192.0.2.13", IdentidadRef::Contrasena),
            ] {
                almacen
                    .crear_host(
                        &DatosHost {
                            nombre: nombre.to_string(),
                            direccion: direccion.to_string(),
                            usuario: Some(USUARIO.to_string()),
                            identidad_ref: identidad,
                            ..DatosHost::default()
                        },
                        Origen::Manual,
                    )
                    .expect("host de prueba");
            }
        }
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (mut app, enviados) = App::de_prueba(
            entorno.rutas.clone(),
            entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
        .expect("app de prueba");
        let fichero = Arc::new(LlaveroFichero::con_ajustes(
            entorno.rutas.datos.join(NOMBRE_FICHERO),
            LOG_N,
            Arc::new(Instant::now),
        ));
        app.llavero = fichero.clone();
        let mut terminal = Terminal::new(TestBackend::new(cols, filas)).expect("terminal");
        app.iniciar_pintado(&mut terminal).expect("pintado");
        Self {
            app,
            terminal,
            enviados,
            fichero,
            entorno,
        }
    }

    fn ruta_llavero(&self) -> PathBuf {
        self.entorno.rutas.datos.join(NOMBRE_FICHERO)
    }

    /// Crea el fichero con `FRASE` y guarda las contraseñas de los dos hosts
    /// con llavero; lo deja abierto.
    fn crear_llavero_con_contrasenas(&mut self) {
        self.fichero.crear(FRASE, FRASE).expect("crear");
        for host in ["web-01", "web-02"] {
            self.fichero
                .guardar_contrasena(host, USUARIO, &format!("{CONTRASENA}-{host}"))
                .expect("guardar contraseña");
        }
    }

    /// Lo mismo, pero bloqueado, como al abrir MAGI otra vez.
    fn llavero_bloqueado(&mut self) {
        self.crear_llavero_con_contrasenas();
        self.fichero.bloquear();
        assert_eq!(self.fichero.estado(), EstadoFichero::Cerrado);
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

    fn escribir(&mut self, texto: &str) {
        for caracter in texto.chars() {
            self.tecla(KeyCode::Char(caracter));
        }
    }

    fn pegar(&mut self, texto: &str) {
        self.evento(Evento::Pegado(texto.to_string()));
    }

    fn servidor(&mut self, mensaje: MensajeServidor) {
        self.evento(Evento::Servidor(mensaje));
    }

    fn drenar(&mut self) -> Vec<MensajeCliente> {
        let mut mensajes = Vec::new();
        while let Ok(mensaje) = self.enviados.try_recv() {
            mensajes.push(mensaje);
        }
        mensajes
    }

    fn texto(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let mut lineas = Vec::new();
        for y in 0..buffer.area.height {
            let mut linea = String::new();
            for x in 0..buffer.area.width {
                linea.push_str(buffer[(x, y)].symbol());
            }
            lineas.push(linea.trim_end().to_string());
        }
        lineas.join("\n")
    }

    /// Bombea los eventos que lleguen de las tareas hasta que se cumpla la
    /// condición (máximo 10 s).
    fn esperar(&mut self, que: &str, condicion: impl Fn(&Prueba) -> bool) {
        let limite = Instant::now() + Duration::from_secs(10);
        while !condicion(self) {
            assert!(
                Instant::now() < limite,
                "no se cumplió «{que}» en 10 s:\n{}",
                self.texto()
            );
            self.app
                .bombear(&mut self.terminal, Duration::from_millis(40))
                .expect("bombear");
        }
    }

    /// Deja pasar las tareas pendientes sin esperar nada concreto.
    fn bombear(&mut self, milisegundos: u64) {
        self.app
            .bombear(&mut self.terminal, Duration::from_millis(milisegundos))
            .expect("bombear");
    }

    /// El diálogo del llavero que está a la vista, si lo hay.
    fn dialogo_llavero(&self) -> Option<&DialogoLlavero> {
        match &self.app.dialogo {
            Some(Dialogo::Llavero(dialogo)) => Some(dialogo),
            _ => None,
        }
    }

    fn modo(&self) -> Option<ModoLlavero> {
        self.dialogo_llavero().map(|dialogo| dialogo.modo)
    }

    /// Los tipos de REGISTRO anotados y su detalle.
    fn registro(&self) -> Vec<(String, String)> {
        self.app
            .almacen
            .registro_para_exportar(None)
            .expect("registro")
            .into_iter()
            .map(|entrada| (entrada.tipo, entrada.detalle))
            .collect()
    }

    fn anotados(&self, tipo: &str) -> Vec<String> {
        self.registro()
            .into_iter()
            .filter(|(otro, _)| otro == tipo)
            .map(|(_, detalle)| detalle)
            .collect()
    }

    fn host(&self, nombre: &str) -> i64 {
        self.app
            .hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .map(|host| host.id)
            .unwrap_or_else(|| panic!("host {nombre} no sembrado"))
    }

    fn sesion_abierta(&mut self, id: u32, host: &str) {
        let host_id = self.host(host);
        self.servidor(bienvenida(vec![sesion(id, host, host_id)]));
    }
}

fn sesion(id: u32, host: &str, host_id: i64) -> InfoSesion {
    InfoSesion {
        id,
        nombre: host.to_string(),
        host_id,
        host_nombre: host.to_string(),
        estado: EstadoSesionRemota::Abriendo,
        motivo: None,
        identidad: "contraseña".to_string(),
        abierta_en: 0,
        ventanas: 1,
        actividad_no_vista: false,
        reconexion: None,
        titulo: None,
        aviso: None,
        tmux: None,
        escritura_varios: false,
    }
}

fn bienvenida(sesiones: Vec<InfoSesion>) -> MensajeServidor {
    MensajeServidor::Bienvenida {
        version: VERSION_PROTOCOLO,
        version_app: "1.6.0".to_string(),
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

fn contrasenas_enviadas(mensajes: &[MensajeCliente]) -> Vec<(u32, String, bool)> {
    mensajes
        .iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::Contrasena {
                sesion_id,
                contrasena,
                recordar,
            } => Some((*sesion_id, contrasena.como_str().to_string(), *recordar)),
            _ => None,
        })
        .collect()
}

// ------------------------------------------------------------------ al abrir MAGI

#[test]
fn al_abrir_con_el_llavero_cerrado_sale_su_dialogo_con_los_textos_del_informe() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_lo_del_arranque();
    prueba.paso();
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    let texto = prueba.texto();
    for esperado in [
        "LLAVERO DE MAGI",
        "Frase maestra",
        "Desbloquea tus contraseñas y la clave",
        "sincronización de este equipo.",
        "desbloquear",
        "Esc seguir sin él",
    ] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
}

#[test]
fn con_el_llavero_del_sistema_o_sin_crear_no_sale_ningun_dialogo_al_abrir() {
    // Sin fichero: no hay nada que desbloquear.
    let mut prueba = Prueba::nueva();
    prueba.app.abrir_lo_del_arranque();
    prueba.paso();
    assert!(prueba.app.dialogo.is_none());

    // El llavero del sistema (el de siempre): nada de esta interfaz se activa.
    let (mut sistema, _) = {
        let entorno = entorno();
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (app, enviados) = App::de_prueba(
            entorno.rutas.clone(),
            entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
        .expect("app");
        ((app, entorno), enviados)
    };
    assert!(sistema.0.llavero.de_fichero().is_none());
    sistema.0.abrir_lo_del_arranque();
    assert!(sistema.0.dialogo.is_none());
    assert!(sistema.0.necesidad_llavero(UsoLlavero::Escribir).is_none());
    assert!(!sistema.0.llavero_con_frase());
}

#[test]
fn esc_en_el_llavero_de_arranque_sigue_sin_el_y_no_toca_nada() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    let antes = std::fs::read(prueba.ruta_llavero()).unwrap();
    prueba.app.abrir_lo_del_arranque();
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(!prueba.app.salir);
    assert_eq!(std::fs::read(prueba.ruta_llavero()).unwrap(), antes);
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Cerrado);
}

#[test]
fn la_frase_correcta_desbloquea_y_cierra_el_dialogo() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    prueba.paso();
    prueba.escribir(FRASE);
    // Lo escrito se ve enmascarado, nunca en claro.
    assert!(!prueba.texto().contains(FRASE));
    prueba.tecla(KeyCode::Enter);
    // Mientras scrypt trabaja: «trabajando…» y no atiende ninguna tecla.
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_some());
    assert!(prueba.texto().contains("trabajando…"));
    prueba.escribir("x");
    prueba.tecla(KeyCode::Tab);
    assert!(prueba.dialogo_llavero().unwrap().campos[0].texto.is_empty());
    prueba.esperar("llavero desbloqueado", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);
    assert!(!prueba.app.llavero_ui.necesitado);
}

#[test]
fn la_frase_incorrecta_se_dice_y_el_fichero_no_cambia() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    let antes = std::fs::read(prueba.ruta_llavero()).unwrap();
    prueba.app.abrir_llavero_del_arranque();
    prueba.escribir("una frase equivocada");
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("frase incorrecta", |p| {
        p.dialogo_llavero()
            .is_some_and(|d| d.trabajando.is_none() && d.error.is_some())
    });
    assert_eq!(
        prueba.dialogo_llavero().unwrap().error.as_deref(),
        Some("frase incorrecta")
    );
    assert!(prueba.texto().contains("frase incorrecta"));
    // El campo queda vacío y el fichero, tal cual.
    assert!(prueba.dialogo_llavero().unwrap().campos[0].texto.is_empty());
    assert_eq!(std::fs::read(prueba.ruta_llavero()).unwrap(), antes);
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Cerrado);
    assert!(prueba.anotados("llavero_desbloqueo_fallido").is_empty());
}

#[test]
fn cinco_fallos_seguidos_imponen_la_espera_y_anotan_una_sola_vez() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    for intento in 1..=FALLOS_POR_SERIE {
        prueba.escribir(&format!("frase equivocada {intento}"));
        prueba.tecla(KeyCode::Enter);
        prueba.esperar("fallo de desbloqueo", |p| {
            p.dialogo_llavero().is_some_and(|d| d.trabajando.is_none())
        });
        let esperados = usize::from(intento == FALLOS_POR_SERIE);
        assert_eq!(
            prueba.anotados("llavero_desbloqueo_fallido").len(),
            esperados,
            "tras {intento} fallos"
        );
    }
    // Una línea por serie, con el número de intentos y sin la frase.
    assert_eq!(
        prueba.anotados("llavero_desbloqueo_fallido"),
        vec!["5".to_string()]
    );
    let texto = prueba.texto();
    assert!(
        texto.contains("demasiados intentos: espera"),
        "el diálogo dice la espera:\n{texto}"
    );
    // Durante la espera, ↵ no gasta scrypt: ni siquiera arranca una petición,
    // aunque la frase sea la buena.
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    let dialogo = prueba.dialogo_llavero().unwrap();
    assert!(dialogo.trabajando.is_none(), "no debe lanzar scrypt");
    assert!(dialogo.error.as_deref().unwrap().contains("espera"));
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Cerrado);
    assert_eq!(prueba.anotados("llavero_desbloqueo_fallido").len(), 1);
    // Ninguna frase en REGISTRO.
    for (_, detalle) in prueba.registro() {
        assert!(!detalle.contains("equivocada") && !detalle.contains(FRASE));
    }
}

#[test]
fn una_respuesta_de_scrypt_que_ya_no_esta_en_vuelo_se_descarta() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    // Un resultado con un id que nadie pidió no cierra nada ni anota nada.
    prueba.evento(Evento::Llavero(EventoLlavero::Creacion {
        id: 777,
        resultado: Ok(()),
    }));
    assert!(prueba.dialogo_llavero().is_some());
    assert!(prueba.anotados("llavero_creado").is_empty());
    // Si el diálogo desaparece con la tarea en marcha (el servidor cae y su
    // aviso lo sustituye), cuando llega el resultado no hay diálogo al que
    // aplicarlo: el llavero queda como esté y nada se rompe.
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_some());
    prueba.app.dialogo = None;
    prueba.esperar("fin de la tarea", |p| !p.app.llavero_ui.hay_en_vuelo());
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);
}

#[test]
fn con_scrypt_en_marcha_esc_no_cierra_el_dialogo_y_el_pie_no_lo_ofrece() {
    // Crear: Esc no «cancela» una creación que la tarea va a terminar igual.
    let mut prueba = Prueba::nueva();
    prueba.app.pedir_llavero(None);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_some());
    let texto = prueba.texto();
    assert!(texto.contains("trabajando…"), "{texto}");
    assert!(texto.contains("espera…"), "{texto}");
    assert!(
        !texto.contains("Esc"),
        "el pie no ofrece Esc mientras scrypt trabaja:\n{texto}"
    );
    prueba.tecla(KeyCode::Esc);
    assert!(
        prueba
            .dialogo_llavero()
            .is_some_and(|d| d.trabajando.is_some()),
        "Esc se ignora con la tarea en marcha"
    );
    // Lo que se ve es lo que pasa: el llavero se crea, y ahora sí se cierra.
    prueba.esperar("llavero creado", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);
    assert_eq!(prueba.anotados("llavero_creado").len(), 1);

    // Desbloquear y cambiar la frase: lo mismo.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_some());
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.dialogo_llavero().is_some());
    prueba.esperar("desbloqueado", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);
}

// ------------------------------------------------------------------ crear la frase

#[test]
fn crear_pide_dos_veces_la_frase_y_no_acepta_menos_de_doce_caracteres() {
    let mut prueba = Prueba::nueva();
    prueba.app.pedir_llavero(Some(Continuacion::MostrarClave));
    prueba.paso();
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    let texto = prueba.texto();
    for esperado in [
        "CREAR LA FRASE MAESTRA",
        "Frase",
        "Repetir",
        "Al menos 12 caracteres",
        "no se recupera",
        "crear",
        "Esc cancelar",
    ] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
    // Corta.
    prueba.escribir("corta");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("corta");
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_none());
    assert!(prueba
        .dialogo_llavero()
        .unwrap()
        .error
        .as_deref()
        .is_some_and(|error| error.contains("12")));
    // No coinciden.
    let mut prueba = Prueba::nueva();
    prueba.app.pedir_llavero(None);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba
        .dialogo_llavero()
        .unwrap()
        .error
        .as_deref()
        .is_some_and(|error| error.contains("no coinciden")));
    assert!(!prueba.ruta_llavero().exists(), "no se crea nada");
}

#[test]
fn crear_escribe_el_fichero_deja_el_llavero_abierto_y_anota_llavero_creado() {
    let mut prueba = Prueba::nueva();
    prueba.app.pedir_llavero(None);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("llavero creado", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);
    assert!(prueba.ruta_llavero().exists());
    assert_eq!(prueba.anotados("llavero_creado"), vec![String::new()]);
    // La frase no está en ningún sitio visible ni en REGISTRO.
    for (_, detalle) in prueba.registro() {
        assert!(!detalle.contains(FRASE));
    }
    assert!(!prueba.texto().contains(FRASE));
}

// ------------------------------------------------------------------ conexión interactiva

#[test]
fn recordar_sin_llavero_y_esc_usa_la_contrasena_sin_guardarla_ni_crear_ningun_fichero() {
    let mut prueba = Prueba::nueva();
    prueba.sesion_abierta(1, "db-01");
    prueba.drenar();
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "db-01".to_string(),
        intento: 1,
        recordar_por_defecto: true,
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ContrasenaServidor { recordar: true, .. })
    ));
    prueba.escribir(CONTRASENA);
    prueba.tecla(KeyCode::Enter);
    // Antes de mandarla, hay que crear el llavero.
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    assert!(contrasenas_enviadas(&prueba.drenar()).is_empty());
    prueba.tecla(KeyCode::Esc);
    // Esc: la contraseña se usa, sin «recordar» y sin ficheros.
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        contrasenas_enviadas(&prueba.drenar()),
        vec![(1, CONTRASENA.to_string(), false)]
    );
    assert!(!prueba.ruta_llavero().exists());
    // Cuando la sesión abre no hay nada pendiente que guardar.
    let mut abierta = sesion(1, "db-01", prueba.host("db-01"));
    abierta.estado = EstadoSesionRemota::Abierta;
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![abierta],
    });
    prueba.bombear(100);
    assert!(!prueba.ruta_llavero().exists());
    assert!(prueba.anotados("llavero_creado").is_empty());
}

#[test]
fn recordar_sin_llavero_crea_la_frase_y_guarda_la_contrasena_al_abrir_la_sesion() {
    let mut prueba = Prueba::nueva();
    prueba.sesion_abierta(1, "db-01");
    prueba.drenar();
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "db-01".to_string(),
        intento: 1,
        recordar_por_defecto: true,
    });
    prueba.escribir(CONTRASENA);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("contraseña enviada", |p| p.app.dialogo.is_none());
    assert_eq!(
        contrasenas_enviadas(&prueba.drenar()),
        vec![(1, CONTRASENA.to_string(), true)]
    );
    assert_eq!(prueba.anotados("llavero_creado").len(), 1);
    // La sesión abre: la contraseña pasa al llavero en un hilo.
    let mut abierta = sesion(1, "db-01", prueba.host("db-01"));
    abierta.estado = EstadoSesionRemota::Abierta;
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![abierta],
    });
    prueba.esperar("contraseña guardada", |p| {
        matches!(
            p.fichero.recuperar_contrasena("db-01", USUARIO),
            Ok(Some(_))
        )
    });
    prueba.bombear(100);
    let guardada = prueba
        .fichero
        .recuperar_contrasena("db-01", USUARIO)
        .unwrap()
        .unwrap();
    assert_eq!(guardada.as_str(), CONTRASENA);
    // El host ya usa el llavero.
    let host = prueba
        .app
        .hosts
        .iter()
        .find(|host| host.nombre == "db-01")
        .unwrap();
    assert_eq!(host.identidad_ref, IdentidadRef::ContrasenaLlavero);
}

#[test]
fn recordar_con_el_llavero_cerrado_pide_antes_la_frase() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "db-01");
    prueba.drenar();
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "db-01".to_string(),
        intento: 1,
        recordar_por_defecto: true,
    });
    prueba.escribir(CONTRASENA);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    assert!(contrasenas_enviadas(&prueba.drenar()).is_empty());
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("contraseña enviada", |p| p.app.dialogo.is_none());
    assert_eq!(
        contrasenas_enviadas(&prueba.drenar()),
        vec![(1, CONTRASENA.to_string(), true)]
    );
}

#[test]
fn una_conexion_interactiva_con_el_llavero_cerrado_pide_la_frase_y_conecta_sin_pedir_la_contrasena()
{
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "web-01".to_string(),
        intento: 1,
        recordar_por_defecto: true,
    });
    // Antes del diálogo de contraseña, la frase.
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    assert!(prueba.app.llavero_ui.necesitado);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("contraseña enviada", |p| p.app.dialogo.is_none());
    assert_eq!(
        contrasenas_enviadas(&prueba.drenar()),
        vec![(1, format!("{CONTRASENA}-web-01"), false)]
    );
    assert!(!prueba.app.marca_llavero_bloqueado());
}

#[test]
fn con_esc_en_el_llavero_la_conexion_pide_la_contrasena_sin_recordar() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "web-01".to_string(),
        intento: 1,
        recordar_por_defecto: true,
    });
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.tecla(KeyCode::Esc);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ContrasenaServidor {
            recordar: false,
            sesion_id: 1,
            ..
        })
    ));
    assert!(contrasenas_enviadas(&prueba.drenar()).is_empty());
    // El llavero sigue cerrado y la barra lo dice.
    assert!(prueba.app.marca_llavero_bloqueado());
    assert!(prueba.texto().contains("llavero bloqueado"));
}

#[test]
fn dos_peticiones_de_desbloqueo_comparten_un_solo_dialogo() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.servidor(bienvenida(vec![
        sesion(1, "web-01", prueba.host("web-01")),
        sesion(2, "web-02", prueba.host("web-02")),
    ]));
    prueba.drenar();
    for (id, host) in [(1, "web-01"), (2, "web-02")] {
        prueba.servidor(MensajeServidor::PideContrasena {
            sesion_id: id,
            host: host.to_string(),
            intento: 1,
            recordar_por_defecto: true,
        });
    }
    // Un solo diálogo, con las dos continuaciones, y nada apilado debajo.
    assert_eq!(
        prueba.dialogo_llavero().unwrap().continuaciones.len(),
        2,
        "la segunda petición se suma a la primera"
    );
    assert!(prueba.app.dialogos_apilados() == 0);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("las dos contraseñas", |p| p.app.dialogo.is_none());
    let mut enviadas = contrasenas_enviadas(&prueba.drenar());
    enviadas.sort();
    assert_eq!(
        enviadas,
        vec![
            (1, format!("{CONTRASENA}-web-01"), false),
            (2, format!("{CONTRASENA}-web-02"), false),
        ]
    );
}

#[test]
fn la_continuacion_se_mueve_y_corre_una_sola_vez() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "web-01".to_string(),
        intento: 1,
        recordar_por_defecto: false,
    });
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| p.app.dialogo.is_none());
    assert_eq!(contrasenas_enviadas(&prueba.drenar()).len(), 1);
    // Una respuesta repetida (el mismo resultado otra vez) ya no está en
    // vuelo: no vuelve a correr nada ni abre nada.
    for id in 1..=3 {
        prueba.evento(Evento::Llavero(EventoLlavero::Desbloqueo {
            id,
            resultado: Ok(()),
        }));
    }
    assert!(prueba.app.dialogo.is_none());
    assert!(contrasenas_enviadas(&prueba.drenar()).is_empty());
    // Y los textos de depuración no llevan datos.
    let continuacion = Continuacion::EnviarContrasena {
        sesion_id: 9,
        contrasena: zeroize::Zeroizing::new("secreta-9".to_string()),
    };
    assert_eq!(format!("{continuacion:?}"), "enviar_contrasena");
    let mut dialogo = DialogoLlavero::nuevo(ModoLlavero::Crear);
    dialogo.campos[0] = CampoTexto::nuevo("secreta-9");
    dialogo.continuaciones.push(continuacion);
    let depurado = format!("{dialogo:?}");
    assert!(!depurado.contains("secreta"), "{depurado}");
}

// ------------------------------------------------------------------ operaciones automáticas

#[test]
fn la_reconexion_automatica_con_el_llavero_cerrado_no_abre_dialogo_y_la_pestana_lo_dice() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    // El servidor pide la contraseña del llavero para reconectar sola.
    prueba.servidor(MensajeServidor::PideLlavero {
        sesion_id: 1,
        host: "web-01".to_string(),
        usuario: USUARIO.to_string(),
    });
    prueba.bombear(100);
    assert!(
        prueba.app.dialogo.is_none(),
        "una operación automática nunca dialoga"
    );
    assert!(prueba.app.dialogos_apilados() == 0);
    // No se contesta con `Cerrar` (cerraría la pestaña que se reconecta) ni
    // con una contraseña.
    assert!(
        prueba.drenar().is_empty(),
        "no se envía nada: vence el plazo del servidor"
    );
    assert!(prueba
        .app
        .pestanas
        .iter()
        .any(|pestana| pestana.sesion_id == 1));
    // La barra lo dice.
    assert!(prueba.app.llavero_ui.necesitado);
    assert!(prueba.texto().contains("llavero bloqueado"));
    // Pasado el plazo, el servidor deja la pestaña caída con su instrucción de
    // siempre; el cliente la reescribe, en un solo sitio.
    prueba.servidor(MensajeServidor::Estado {
        sesion_id: 1,
        estado: EstadoSesionRemota::Caida,
        motivo: Some(
            "necesita tu respuesta: prefijo r · «web-01» usa contraseña y no hay ninguna \
             guardada en el llavero; conéctate una vez con ↵ y marca «recordar»"
                .to_string(),
        ),
    });
    assert_eq!(
        prueba.app.pestanas[0].motivo.as_deref(),
        Some("llavero bloqueado: desbloquéalo (Ctrl+P)")
    );
    assert_eq!(prueba.app.pestanas[0].motivo.as_deref(), Some(BLOQUEADO));
    assert!(prueba.app.dialogo.is_none());
    // También llega por la lista completa de sesiones.
    let mut caida = sesion(1, "web-01", prueba.host("web-01"));
    caida.estado = EstadoSesionRemota::Caida;
    caida.motivo = Some(
        "necesita tu respuesta: prefijo r · «web-01» usa contraseña y no hay ninguna guardada \
         en el llavero; conéctate una vez con ↵ y marca «recordar»"
            .to_string(),
    );
    prueba.servidor(MensajeServidor::Sesiones { lista: vec![caida] });
    assert_eq!(prueba.app.pestanas[0].motivo.as_deref(), Some(BLOQUEADO));
}

#[test]
fn una_ejecucion_con_el_llavero_cerrado_no_abre_dialogo_y_cierra_su_conexion() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    // El id no es el de ninguna pestaña: lo pide una ejecución de snippet.
    prueba.servidor(MensajeServidor::PideLlavero {
        sesion_id: 9000,
        host: "web-01".to_string(),
        usuario: USUARIO.to_string(),
    });
    assert!(prueba.app.dialogo.is_none());
    assert!(matches!(
        prueba.drenar().as_slice(),
        [MensajeCliente::Cerrar { sesion_id: 9000 }]
    ));
    assert!(prueba.texto().contains("llavero bloqueado"));
    assert!(prueba.app.llavero_ui.necesitado);
}

#[test]
fn con_el_llavero_abierto_la_reconexion_automatica_contesta_como_siempre() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    prueba.servidor(MensajeServidor::PideLlavero {
        sesion_id: 1,
        host: "web-01".to_string(),
        usuario: USUARIO.to_string(),
    });
    // La contestación sale de una tarea: se espera a que llegue.
    let limite = Instant::now() + Duration::from_secs(10);
    let mut enviadas = Vec::new();
    while enviadas.is_empty() {
        assert!(Instant::now() < limite, "no se contestó en 10 s");
        prueba.bombear(40);
        enviadas = contrasenas_enviadas(&prueba.drenar());
    }
    assert_eq!(enviadas, vec![(1, format!("{CONTRASENA}-web-01"), false)]);
    assert!(prueba.app.dialogo.is_none());
}

// ------------------------------------------------------------------ cambiar la frase y borrar

fn abrir_por_la_paleta(prueba: &mut Prueba, consulta: &str) {
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(prueba.app.paleta.is_some());
    prueba.escribir(consulta);
    prueba.tecla(KeyCode::Enter);
}

#[test]
fn cambiar_la_frase_recifra_el_llavero_y_la_vieja_deja_de_abrir() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    abrir_por_la_paleta(&mut prueba, "Cambiar la frase");
    assert_eq!(prueba.modo(), Some(ModoLlavero::Cambiar));
    let texto = prueba.texto();
    for esperado in [
        "CAMBIAR LA FRASE MAESTRA",
        "Actual",
        "Nueva",
        "Repetir",
        "cambiar",
    ] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
    // La actual mal: la frase de ahora no es esa.
    prueba.escribir("no es la frase actual");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("fallo", |p| {
        p.dialogo_llavero()
            .is_some_and(|d| d.trabajando.is_none() && d.error.is_some())
    });
    assert!(prueba.anotados("llavero_frase_cambiada").is_empty());
    // Ahora bien. Las frases nuevas no coinciden: se dice antes de gastar scrypt.
    let dialogo = prueba.dialogo_llavero().unwrap();
    assert!(dialogo.campos[0].texto.is_empty() && dialogo.campos[1].texto.is_empty());
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("la repetición no coincide");
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_none());
    assert!(prueba
        .dialogo_llavero()
        .unwrap()
        .error
        .as_deref()
        .is_some_and(|error| error.contains("no coinciden")));
    // Y por fin las tres bien.
    let dialogo = prueba.app.dialogo.take();
    drop(dialogo);
    abrir_por_la_paleta(&mut prueba, "Cambiar la frase");
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("frase cambiada", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.anotados("llavero_frase_cambiada").len(), 1);
    // La vieja ya no abre; la nueva sí, y las contraseñas siguen dentro.
    let otro = LlaveroFichero::con_ajustes(prueba.ruta_llavero(), LOG_N, Arc::new(Instant::now));
    assert!(matches!(
        otro.desbloquear(FRASE),
        Err(ErrorLlavero::FraseIncorrecta { .. })
    ));
    otro.desbloquear(OTRA_FRASE).expect("la nueva abre");
    assert_eq!(
        otro.recuperar_contrasena("web-01", USUARIO)
            .unwrap()
            .unwrap()
            .as_str(),
        format!("{CONTRASENA}-web-01")
    );
}

#[test]
fn borrar_el_llavero_exige_escribir_borrar_y_explica_que_se_pierde() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    abrir_por_la_paleta(&mut prueba, "Borrar el llavero");
    assert_eq!(prueba.modo(), Some(ModoLlavero::Borrar));
    let texto = prueba.texto();
    for esperado in [
        "BORRAR EL LLAVERO",
        "contraseñas guardadas",
        "clave de sincronización de este equipo",
        "Escribe borrar",
    ] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
    // Sin escribirlo, o escribiendo otra cosa, no se borra.
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.ruta_llavero().exists());
    prueba.escribir("borra");
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.ruta_llavero().exists());
    assert!(prueba.anotados("llavero_borrado").is_empty());
    assert!(prueba.dialogo_llavero().is_some());
    // Escribiéndolo, sí.
    prueba.escribir("r");
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none());
    assert!(!prueba.ruta_llavero().exists());
    assert_eq!(prueba.anotados("llavero_borrado").len(), 1);
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Inexistente);
}

#[test]
fn esc_en_cambiar_o_borrar_no_toca_nada() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    let antes = std::fs::read(prueba.ruta_llavero()).unwrap();
    for consulta in ["Cambiar la frase", "Borrar el llavero"] {
        abrir_por_la_paleta(&mut prueba, consulta);
        prueba.escribir("lo que sea");
        prueba.tecla(KeyCode::Esc);
        assert!(prueba.app.dialogo.is_none());
    }
    assert_eq!(std::fs::read(prueba.ruta_llavero()).unwrap(), antes);
    assert!(prueba
        .registro()
        .iter()
        .all(|(tipo, _)| !tipo.starts_with("llavero_")));
}

// ------------------------------------------------------------------ llavero dañado

fn dejar_danado(prueba: &Prueba) -> Vec<u8> {
    let basura = b"esto no es un fichero age, ni de lejos".to_vec();
    std::fs::write(prueba.ruta_llavero(), &basura).unwrap();
    basura
}

#[test]
fn un_llavero_danado_se_aparta_se_anota_y_se_sigue_sin_llavero() {
    let mut prueba = Prueba::nueva();
    let basura = dejar_danado(&prueba);
    assert!(matches!(prueba.fichero.estado(), EstadoFichero::Danado(_)));
    prueba.app.abrir_lo_del_arranque();
    prueba.paso();
    assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));
    let texto = prueba.texto();
    for esperado in [
        "LLAVERO DAÑADO",
        "No se puede leer llavero.age",
        "no lo borra nunca",
    ] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
    prueba.tecla(KeyCode::Char('a'));
    assert!(prueba.app.dialogo.is_none());
    assert!(!prueba.app.salir);
    // El original ya no está, la copia sí y con su contenido, y el estado es
    // «sin llavero».
    assert!(!prueba.ruta_llavero().exists());
    let copias: Vec<String> = std::fs::read_dir(&prueba.entorno.rutas.datos)
        .unwrap()
        .filter_map(|entrada| entrada.ok())
        .map(|entrada| entrada.file_name().to_string_lossy().to_string())
        .filter(|nombre| nombre.starts_with("llavero.age.danado-"))
        .collect();
    assert_eq!(copias.len(), 1, "{copias:?}");
    assert_eq!(
        std::fs::read(prueba.entorno.rutas.datos.join(&copias[0])).unwrap(),
        basura
    );
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Inexistente);
    // REGISTRO: el nombre de la copia.
    assert_eq!(prueba.anotados("llavero_apartado"), copias);
}

#[test]
fn salir_del_llavero_danado_cierra_magi_sin_tocar_nada() {
    let mut prueba = Prueba::nueva();
    let basura = dejar_danado(&prueba);
    prueba.app.abrir_lo_del_arranque();
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.salir, "salir cierra MAGI");
    assert_eq!(std::fs::read(prueba.ruta_llavero()).unwrap(), basura);
    assert!(prueba.anotados("llavero_apartado").is_empty());

    // La tecla `s` hace lo mismo.
    let mut prueba = Prueba::nueva();
    dejar_danado(&prueba);
    prueba.app.abrir_lo_del_arranque();
    prueba.tecla(KeyCode::Char('s'));
    assert!(prueba.app.salir);
    assert!(prueba.ruta_llavero().exists());
}

#[test]
fn un_fichero_que_se_estropea_al_desbloquear_pasa_al_dialogo_danado() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    // Se estropea después de abrir el diálogo (la cabecera aún valía).
    let mut bytes = std::fs::read(prueba.ruta_llavero()).unwrap();
    let mitad = bytes.len() / 2;
    bytes.truncate(mitad);
    std::fs::write(prueba.ruta_llavero(), &bytes).unwrap();
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("dañado", |p| p.modo() == Some(ModoLlavero::Danado));
    assert!(prueba.texto().contains("LLAVERO DAÑADO"));
    // Nada se borra solo.
    assert!(prueba.ruta_llavero().exists());
}

// ------------------------------------------------------------------ paleta y barra

fn etiquetas_de_la_paleta(prueba: &mut Prueba) -> Vec<String> {
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    let etiquetas = prueba
        .app
        .paleta
        .as_ref()
        .expect("paleta")
        .entradas
        .iter()
        .filter(|entrada| entrada.categoria == "llavero")
        .map(|entrada| entrada.etiqueta.clone())
        .collect();
    prueba.tecla(KeyCode::Esc);
    etiquetas
}

#[test]
fn la_paleta_enseña_las_entradas_que_tocan_segun_el_estado_del_llavero() {
    let mut prueba = Prueba::nueva();
    // Sin fichero: nada que desbloquear, cambiar ni borrar.
    assert!(etiquetas_de_la_paleta(&mut prueba).is_empty());
    prueba.crear_llavero_con_contrasenas();
    assert_eq!(
        etiquetas_de_la_paleta(&mut prueba),
        vec![
            "Bloquear el llavero",
            "Cambiar la frase maestra",
            "Borrar el llavero"
        ]
    );
    prueba.fichero.bloquear();
    assert_eq!(
        etiquetas_de_la_paleta(&mut prueba),
        vec![
            "Desbloquear el llavero",
            "Cambiar la frase maestra",
            "Borrar el llavero"
        ]
    );
    // Un fichero dañado se aparta, no se borra ni se cambia.
    dejar_danado(&prueba);
    assert!(etiquetas_de_la_paleta(&mut prueba).is_empty());
}

#[test]
fn la_paleta_no_enseña_entradas_del_llavero_con_el_llavero_del_sistema() {
    let entorno = entorno();
    let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    let (mut app, _enviados) = App::de_prueba(
        entorno.rutas.clone(),
        entorno.config.clone(),
        Tema::respaldo(),
        almacen,
    )
    .expect("app");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    app.iniciar_pintado(&mut terminal).unwrap();
    assert!(app.llavero.de_fichero().is_none());
    app.procesar(Evento::Tecla(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));
    let paleta = app.paleta.as_ref().expect("paleta abierta");
    for entrada in &paleta.entradas {
        assert_ne!(entrada.categoria, "llavero", "{}", entrada.etiqueta);
        assert!(!entrada.etiqueta.to_lowercase().contains("llavero"));
    }
}

#[test]
fn bloquear_desde_la_paleta_borra_la_frase_de_la_memoria() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    abrir_por_la_paleta(&mut prueba, "Bloquear el llavero");
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Cerrado);
    assert_eq!(
        prueba.fichero.recuperar_contrasena("web-01", USUARIO),
        Err(BLOQUEADO.to_string())
    );
    // Voluntario: la barra no dice nada hasta que algo lo necesite.
    assert!(!prueba.app.marca_llavero_bloqueado());
    // Desbloquearlo desde la paleta.
    abrir_por_la_paleta(&mut prueba, "Desbloquear el llavero");
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("abierto", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);
}

#[test]
fn la_marca_de_la_barra_aparece_cuando_algo_necesita_el_llavero_y_se_va_al_abrirlo() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    assert!(!prueba.texto().contains("llavero bloqueado"));
    prueba.app.pedir_llavero(None);
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.marca_llavero_bloqueado());
    let texto = prueba.texto();
    let barra = texto.lines().last().unwrap();
    assert!(barra.contains("llavero bloqueado"), "{barra}");
    // Al desbloquear, la marca desaparece.
    prueba.app.pedir_llavero(None);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("abierto", |p| p.app.dialogo.is_none());
    prueba.paso();
    assert!(!prueba
        .texto()
        .lines()
        .last()
        .unwrap()
        .contains("llavero bloqueado"));
}

#[test]
fn un_sondeo_que_fallo_por_el_llavero_cerrado_deja_la_marca() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    assert!(!prueba.app.llavero_ui.necesitado);
    prueba.app.nota_llavero_necesitado(Some(&format!(
        "«web-01» usa contraseña y el llavero no está disponible ({BLOQUEADO}); \
         conéctate una vez con ↵ o usa una clave"
    )));
    assert!(prueba.app.marca_llavero_bloqueado());
    prueba.app.llavero_ui.necesitado = false;
    prueba
        .app
        .nota_llavero_necesitado(Some("el host no responde"));
    assert!(!prueba.app.llavero_ui.necesitado);
}

// ------------------------------------------------------------------ pegado

#[test]
fn se_puede_pegar_en_los_dialogos_con_campos() {
    // LLAVERO DE MAGI: el pegado va al campo y se ve enmascarado.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    prueba.pegar(FRASE);
    assert_eq!(prueba.dialogo_llavero().unwrap().campos[0].texto, FRASE);
    assert!(!prueba.texto().contains(FRASE));
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("abierto", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);

    // CREAR: en el campo con foco.
    let mut prueba = Prueba::nueva();
    prueba.app.pedir_llavero(None);
    prueba.pegar(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.pegar(FRASE);
    let dialogo = prueba.dialogo_llavero().unwrap();
    assert_eq!(dialogo.campos[0].texto, FRASE);
    assert_eq!(dialogo.campos[1].texto, FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("creado", |p| p.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Abierto);

    // BORRAR y CAMBIAR.
    abrir_por_la_paleta(&mut prueba, "Borrar el llavero");
    prueba.pegar("borrar");
    assert_eq!(prueba.dialogo_llavero().unwrap().campos[0].texto, "borrar");
    prueba.tecla(KeyCode::Esc);
    abrir_por_la_paleta(&mut prueba, "Cambiar la frase");
    prueba.pegar(FRASE);
    assert_eq!(prueba.dialogo_llavero().unwrap().campos[0].texto, FRASE);
    prueba.tecla(KeyCode::Esc);

    // El dañado no tiene campos: no consume el pegado.
    let mut dialogo = Dialogo::Llavero(Box::new(DialogoLlavero::danado("roto".to_string())));
    assert!(!dialogo.pegar_texto("hola"));
    // Y con scrypt en marcha tampoco.
    let mut trabajando = DialogoLlavero::nuevo(ModoLlavero::Desbloquear);
    trabajando.trabajando = Some(1);
    let mut dialogo = Dialogo::Llavero(Box::new(trabajando));
    assert!(!dialogo.pegar_texto("hola"));
}

/// Los bytes de todo el búfer de un `String` (la capacidad entera, no solo lo
/// que tiene dentro): lo que alguien podría leer de la memoria.
fn bytes_del_bufer(texto: &String) -> Vec<u8> {
    // SAFETY: el búfer es del `String` y mide `capacity()` bytes; leerlos como
    // `u8` es válido porque `zeroize` (o `with_capacity` + escribir) los inicia
    // todos antes de que esta función los mire (si no, la prueba falla antes).
    unsafe { std::slice::from_raw_parts(texto.as_ptr(), texto.capacity()).to_vec() }
}

#[test]
fn los_campos_de_frase_se_borran_de_la_memoria_al_cerrar() {
    // `borrar_campos`: el mismo búfer, entero a ceros. Un `clear()` también
    // deja el `String` vacío pero deja la frase en el búfer.
    let mut dialogo = DialogoLlavero::nuevo(ModoLlavero::Cambiar);
    for campo in &mut dialogo.campos {
        campo.texto.push_str(FRASE);
        campo.cursor = FRASE.chars().count();
    }
    let antes: Vec<(usize, usize)> = dialogo
        .campos
        .iter()
        .map(|campo| (campo.texto.as_ptr() as usize, campo.texto.capacity()))
        .collect();
    for campo in &dialogo.campos {
        assert!(
            bytes_del_bufer(&campo.texto)
                .windows(FRASE.len())
                .any(|ventana| ventana == FRASE.as_bytes()),
            "la frase debería estar en el búfer antes de borrar"
        );
    }
    dialogo.borrar_campos();
    for (campo, (puntero, capacidad)) in dialogo.campos.iter().zip(antes) {
        assert_eq!(campo.texto.as_ptr() as usize, puntero, "cambió de búfer");
        assert_eq!(campo.texto.capacity(), capacidad, "cambió de búfer");
        assert!(campo.texto.is_empty());
        assert_eq!(campo.cursor, 0);
        assert!(
            bytes_del_bufer(&campo.texto).iter().all(|byte| *byte == 0),
            "quedan bytes de la frase en el búfer"
        );
    }
}

#[test]
fn el_buffer_de_un_campo_de_frase_se_libera_a_ceros_al_caer_cerrar_y_confirmar() {
    let _cerrojo = CERROJO_VIGILANTE.lock().unwrap_or_else(|e| e.into_inner());

    // Al caer el diálogo (Drop).
    let mut dialogo = DialogoLlavero::nuevo(ModoLlavero::Desbloquear);
    dialogo.campos[0].texto.push_str(FRASE);
    vigilar(&dialogo.campos[0].texto);
    drop(dialogo);
    assert_eq!(
        liberado_a_cero(),
        Some(true),
        "Drop deja la frase en memoria"
    );

    // Al cerrarlo con Esc: el diálogo cae y su búfer se libera a ceros.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    prueba.escribir(FRASE);
    vigilar(&prueba.dialogo_llavero().unwrap().campos[0].texto);
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        liberado_a_cero(),
        Some(true),
        "Esc deja la frase en memoria"
    );

    // Al confirmar: la frase sale del campo hacia la tarea (sin copias en el
    // campo) y se libera a ceros cuando la tarea termina.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    prueba.escribir(FRASE);
    vigilar(&prueba.dialogo_llavero().unwrap().campos[0].texto);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| p.app.dialogo.is_none());
    prueba.bombear(50);
    let resultado = liberado_a_cero();
    dejar_de_vigilar();
    assert_eq!(resultado, Some(true), "la tarea dejó la frase en memoria");
}

// ------------------------------------------------------------------ sincronización

fn confirmar(prueba: &mut Prueba, accion: AccionInventario) {
    prueba.app.dialogo = Some(Dialogo::Confirmar {
        titulo: "PRUEBA".to_string(),
        lineas: vec!["¿Seguro?".to_string()],
        peligro: false,
        accion: AccionDialogo::Inventario(accion),
    });
    prueba.tecla(KeyCode::Char('s'));
}

/// Una carpeta de sincronización vacía y la entrada de texto de «n».
fn elegir_carpeta(prueba: &mut Prueba) -> PathBuf {
    let carpeta = prueba.entorno.rutas.hogar.join("Sync").join("magi");
    std::fs::create_dir_all(&carpeta).unwrap();
    prueba.app.dialogo = Some(Dialogo::EntradaTexto {
        titulo: "CARPETA DE SINCRONIZACIÓN".to_string(),
        etiqueta: "Carpeta".to_string(),
        campo: CampoTexto::nuevo(carpeta.to_string_lossy().to_string()),
        accion: EntradaTextoAccion::CarpetaSincronizacion,
    });
    prueba.tecla(KeyCode::Enter);
    carpeta
}

#[test]
fn crear_la_sincronizacion_sin_llavero_pide_la_frase_y_despues_crea_la_clave() {
    let mut prueba = Prueba::nueva();
    let carpeta = elegir_carpeta(&mut prueba);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    assert!(prueba.app.config.sincronizacion.carpeta.is_none());
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("clave de sincronización", |p| {
        matches!(p.app.dialogo, Some(Dialogo::ClaveSincronizacion { .. }))
    });
    // La clave quedó en el llavero y la carpeta en la configuración.
    assert!(prueba
        .fichero
        .recuperar("sincronizacion")
        .unwrap()
        .is_some());
    assert_eq!(
        prueba.app.config.sincronizacion.carpeta.as_deref(),
        Some(carpeta.to_string_lossy().as_ref())
    );
    assert_eq!(prueba.anotados("llavero_creado").len(), 1);
}

#[test]
fn crear_la_sincronizacion_y_esc_dice_que_sin_llavero_no_se_guarda_la_clave() {
    let mut prueba = Prueba::nueva();
    elegir_carpeta(&mut prueba);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    prueba.tecla(KeyCode::Esc);
    // Cancelado: no se crea ningún fichero ni se configura la carpeta, y se
    // vuelve a pedir la carpeta con lo escrito.
    assert!(!prueba.ruta_llavero().exists());
    assert!(prueba.app.config.sincronizacion.carpeta.is_none());
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::EntradaTexto {
            accion: EntradaTextoAccion::CarpetaSincronizacion,
            ..
        })
    ));
    let mensaje = prueba.app.mensaje.as_ref().expect("mensaje").texto.clone();
    assert!(
        mensaje.contains("sin llavero no se puede guardar la clave"),
        "{mensaje}"
    );
}

#[test]
fn mostrar_la_clave_con_el_llavero_cerrado_pide_la_frase_y_la_muestra() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    // Una clave de sincronización guardada, como tras crear o unir.
    let clave = magi::sincronizacion::cifrado::ClaveSincronizacion::generar();
    magi::sincronizacion::equipos::guardar_en_llavero(&clave, &*prueba.fichero).unwrap();
    prueba.fichero.bloquear();
    confirmar(&mut prueba, AccionInventario::MostrarClave);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("clave a la vista", |p| {
        matches!(p.app.dialogo, Some(Dialogo::ClaveSincronizacion { .. }))
    });
    // Con Esc, en cambio, se dice por qué no.
    prueba.tecla(KeyCode::Esc);
    prueba.fichero.bloquear();
    confirmar(&mut prueba, AccionInventario::MostrarClave);
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        prueba
            .app
            .mensaje
            .as_ref()
            .map(|mensaje| mensaje.texto.as_str()),
        Some(BLOQUEADO)
    );
}

#[test]
fn rotar_quitar_y_desactivar_con_el_llavero_cerrado_piden_la_frase_primero() {
    for accion in [
        AccionInventario::RotarClave,
        AccionInventario::Desactivar,
        AccionInventario::QuitarEquipo("00000000-0000-4000-8000-000000000000".to_string()),
    ] {
        let mut prueba = Prueba::nueva();
        prueba.llavero_bloqueado();
        let carpeta = prueba.entorno.rutas.hogar.join("Sync");
        std::fs::create_dir_all(&carpeta).unwrap();
        prueba.app.config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
        confirmar(&mut prueba, accion);
        assert_eq!(
            prueba.modo(),
            Some(ModoLlavero::Desbloquear),
            "antes de tocar nada"
        );
        // Con Esc la operación se cancela diciéndolo y no cambia nada.
        prueba.tecla(KeyCode::Esc);
        assert!(prueba.app.dialogo.is_none());
        let mensaje = prueba.app.mensaje.as_ref().expect("mensaje").texto.clone();
        assert!(mensaje.starts_with(BLOQUEADO), "{mensaje}");
        assert!(prueba.app.config.sincronizacion.carpeta.is_some());
    }
}

#[test]
fn unir_con_el_llavero_sin_crear_y_esc_vuelve_a_unir_con_el_error_en_el_campo() {
    let mut prueba = Prueba::nueva();
    let carpeta = prueba.entorno.rutas.hogar.join("Sync").join("magi");
    std::fs::create_dir_all(&carpeta).unwrap();
    prueba.app.pedir_llavero(Some(Continuacion::UnirEquipo {
        carpeta: carpeta.clone(),
        clave: zeroize::Zeroizing::new("AGE-SECRET-KEY-1QQQQ".to_string()),
    }));
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    prueba.tecla(KeyCode::Esc);
    match &prueba.app.dialogo {
        Some(Dialogo::UnirEquipo { campo, error, .. }) => {
            assert_eq!(campo.texto, "AGE-SECRET-KEY-1QQQQ");
            assert!(error
                .as_deref()
                .is_some_and(|error| error.contains("sin llavero no se puede guardar la clave")));
        }
        _ => panic!("debía volver UNIR ESTE EQUIPO"),
    }
}

#[test]
fn la_pasada_con_el_llavero_cerrado_dice_llavero_bloqueado_y_no_abre_dialogo() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    let mut config = prueba.app.config.clone();
    config.sincronizacion.carpeta = Some(
        prueba
            .entorno
            .rutas
            .hogar
            .join("Sync")
            .to_string_lossy()
            .to_string(),
    );
    // La espera de claves de la pasada, como la lanza `solicitar_pasada`.
    magi::app::sincronizacion::lanzar_espera(
        &prueba.app.runtime,
        prueba.app.eventos_tx.clone(),
        prueba.entorno.rutas.clone(),
        config,
        prueba.app.llavero.clone(),
    );
    prueba.esperar("error de la pasada", |p| {
        matches!(
            &p.app.sincronizacion.estado,
            magi::sincronizacion::EstadoPasada::Error(_)
        )
    });
    match &prueba.app.sincronizacion.estado {
        magi::sincronizacion::EstadoPasada::Error(motivo) => {
            assert_eq!(motivo, BLOQUEADO, "no «falta la clave»");
        }
        otro => panic!("estado inesperado: {otro:?}"),
    }
    assert!(prueba.app.dialogo.is_none());
    // Y la barra lo dice: algo necesitó el llavero y estaba cerrado.
    assert!(prueba.app.marca_llavero_bloqueado());
}

#[test]
fn sin_fichero_la_pasada_sigue_diciendo_que_falta_la_clave() {
    let mut prueba = Prueba::nueva();
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Inexistente);
    // La sincronización configurada (la pasada lee la configuración de la App).
    prueba.app.config.sincronizacion.carpeta = Some(
        prueba
            .entorno
            .rutas
            .hogar
            .join("Sync")
            .to_string_lossy()
            .to_string(),
    );
    let config = prueba.app.config.clone();
    // Por el camino real: la espera de claves de la App (sin fichero no hay
    // ninguna, y eso no es un error) y la pasada que corre el bucle.
    magi::app::sincronizacion::lanzar_espera(
        &prueba.app.runtime,
        prueba.app.eventos_tx.clone(),
        prueba.entorno.rutas.clone(),
        config,
        prueba.app.llavero.clone(),
    );
    prueba.esperar("fin de la pasada", |p| {
        !matches!(
            &p.app.sincronizacion.estado,
            magi::sincronizacion::EstadoPasada::SinConfigurar
        )
    });
    assert!(
        matches!(
            &prueba.app.sincronizacion.estado,
            magi::sincronizacion::EstadoPasada::SinClave
        ),
        "estado: {:?}",
        prueba.app.sincronizacion.estado
    );
    let mensaje = prueba.app.mensaje.as_ref().expect("sin mensaje");
    assert_eq!(
        mensaje.texto,
        "falta la clave de sincronización en el llavero"
    );
    assert!(mensaje.error);
    // No es «llavero bloqueado»: sin fichero no hay nada que desbloquear.
    assert!(prueba.app.dialogo.is_none());
    assert!(!prueba.app.llavero_ui.necesitado);
    assert!(!prueba.app.marca_llavero_bloqueado());
    // Y el llavero no se ha creado por el camino.
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Inexistente);
    assert_eq!(prueba.fichero.guardar("x", "y"), Err(SIN_CREAR.to_string()));
}

// ------------------------------------------------------------------ varios

#[test]
fn guardar_una_contrasena_con_el_llavero_cerrado_se_dice_y_deja_la_marca() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    prueba.sesion_abierta(1, "db-01");
    prueba.drenar();
    // El usuario teclea la contraseña con «recordar» (el llavero está
    // abierto: sale hacia el servidor y queda pendiente de guardar).
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "db-01".to_string(),
        intento: 1,
        recordar_por_defecto: true,
    });
    prueba.escribir(CONTRASENA);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(
        contrasenas_enviadas(&prueba.drenar()),
        vec![(1, CONTRASENA.to_string(), true)]
    );
    // El llavero se cierra en medio (la paleta, o el tiempo) y la sesión abre.
    prueba.fichero.bloquear();
    assert!(!prueba.app.marca_llavero_bloqueado());
    let mut abierta = sesion(1, "db-01", prueba.host("db-01"));
    abierta.estado = EstadoSesionRemota::Abierta;
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![abierta],
    });
    // El guardado va en un hilo: el resultado llega por `Evento::Llavero`, con
    // el id de su petición, y la App lo dice y deja la marca.
    prueba.esperar("aviso del guardado", |p| {
        p.app.mensaje.as_ref().is_some_and(|mensaje| mensaje.error)
    });
    let mensaje = prueba.app.mensaje.as_ref().unwrap();
    assert!(
        mensaje
            .texto
            .starts_with("no se pudo guardar la contraseña")
            && mensaje.texto.contains(BLOQUEADO),
        "{}",
        mensaje.texto
    );
    assert!(prueba.app.llavero_ui.necesitado);
    assert!(prueba.app.marca_llavero_bloqueado());
    prueba.paso();
    let texto = prueba.texto();
    let barra = texto.lines().last().unwrap();
    assert!(barra.contains("llavero bloqueado"), "{barra}");
    // La identidad del host no cambió: la contraseña no se guardó.
    let host = prueba
        .app
        .hosts
        .iter()
        .find(|host| host.nombre == "db-01")
        .unwrap();
    assert_eq!(host.identidad_ref, IdentidadRef::Contrasena);
    assert!(matches!(
        prueba.fichero.recuperar_contrasena("db-01", USUARIO),
        Err(error) if error == BLOQUEADO
    ));

    // El mismo resultado, entregado como evento a una petición que no está en
    // vuelo, se descarta sin tocar nada.
    prueba.app.llavero_ui.necesitado = false;
    prueba.evento(Evento::Llavero(EventoLlavero::ContrasenaGuardada {
        id: 9999,
        host_id: prueba.host("db-01"),
        host: "db-01".to_string(),
        resultado: Err(BLOQUEADO.to_string()),
    }));
    assert!(!prueba.app.llavero_ui.necesitado);
}

#[test]
fn el_pintado_a_48x20_y_40x12_no_se_corta_con_el_dialogo_en_marcha() {
    for (cols, filas) in [(48, 20), (40, 12)] {
        let mut prueba = Prueba::con_tamano(cols, filas);
        prueba.llavero_bloqueado();
        prueba.app.abrir_llavero_del_arranque();
        prueba.paso();
        let texto = prueba.texto();
        assert!(texto.contains("LLAVERO DE MAGI"), "{texto}");
        assert!(texto.contains("desbloquear"), "{texto}");
        prueba.escribir(FRASE);
        prueba.tecla(KeyCode::Enter);
        let texto = prueba.texto();
        assert!(texto.contains("trabajando"), "{texto}");
        prueba.esperar("abierto", |p| p.app.dialogo.is_none());
    }
}

// ------------------------------------------------------------------ más flujos

/// Escribe a mano un fichero de sincronización válido de otro equipo, cifrado
/// con la clave indicada (como `sincronizacion_equipos`).
fn escribir_fichero_ajeno(
    carpeta: &std::path::Path,
    clave: &magi::sincronizacion::cifrado::ClaveSincronizacion,
) {
    use magi::sincronizacion::formato;
    let uuid_equipo = uuid::Uuid::new_v4().to_string();
    let documento = formato::Documento {
        formato: formato::FORMATO_1,
        app: "1.5.0".to_string(),
        equipo: formato::EquipoDocumento {
            uuid: uuid_equipo.clone(),
            nombre: "otro-equipo".to_string(),
            plataforma: "linux".to_string(),
        },
        generado_en: "2026-11-05T09:12:44.120+01:00".to_string(),
        reloj_ms: 1,
        elementos: Vec::new(),
        lapidas: Vec::new(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    };
    let json = formato::serializar(&documento).unwrap();
    let firma = clave.firmar(json.as_bytes()).unwrap();
    let plano = formato::componer(&json, &firma, documento.formato);
    let cifrado = clave.cifrar(&plano).unwrap();
    magi::sincronizacion::carpeta::escribir(carpeta, &uuid_equipo, &cifrado).unwrap();
}

fn abrir_unir(prueba: &mut Prueba, carpeta: &std::path::Path, texto: &str) {
    prueba.app.dialogo = Some(Dialogo::UnirEquipo {
        carpeta: carpeta.to_path_buf(),
        equipos: vec!["otro-equipo".to_string()],
        campo: CampoTexto::nuevo(texto),
        visible: false,
        error: None,
    });
    prueba.tecla(KeyCode::Enter);
}

#[test]
fn unir_valida_la_clave_antes_de_pedir_el_llavero_y_despues_lo_crea() {
    use magi::sincronizacion::cifrado::ClaveSincronizacion;
    let mut prueba = Prueba::nueva();
    let carpeta = prueba.entorno.rutas.hogar.join("Sync").join("magi");
    std::fs::create_dir_all(&carpeta).unwrap();
    let clave = ClaveSincronizacion::generar();
    escribir_fichero_ajeno(&carpeta, &clave);

    // Una clave equivocada se dice en el campo, sin pedir ninguna frase.
    let otra = ClaveSincronizacion::generar();
    abrir_unir(&mut prueba, &carpeta, otra.texto().as_str());
    match &prueba.app.dialogo {
        Some(Dialogo::UnirEquipo { error, .. }) => {
            assert!(error.is_some(), "la clave no abre los ficheros");
        }
        _ => panic!("debía seguir UNIR ESTE EQUIPO con el error"),
    }
    assert!(!prueba.ruta_llavero().exists());

    // La buena: ahora sí falta el llavero y se crea la frase.
    prueba.app.dialogo = None;
    abrir_unir(&mut prueba, &carpeta, clave.texto().as_str());
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    assert!(prueba.app.config.sincronizacion.carpeta.is_none());
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("equipo unido", |p| {
        p.app.config.sincronizacion.carpeta.is_some()
    });
    assert!(prueba.app.dialogo.is_none());
    // La clave pegada es la del llavero y no se ve en ninguna parte.
    let guardada = prueba.fichero.recuperar("sincronizacion").unwrap().unwrap();
    assert_eq!(guardada.as_str(), clave.texto().as_str());
    assert!(!prueba.texto().contains(clave.texto().as_str()));
    for (_, detalle) in prueba.registro() {
        assert!(!detalle.contains(clave.texto().as_str()));
    }
}

#[test]
fn unir_con_el_llavero_cerrado_pide_la_frase_y_esc_deja_la_clave_en_el_campo() {
    use magi::sincronizacion::cifrado::ClaveSincronizacion;
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    let carpeta = prueba.entorno.rutas.hogar.join("Sync").join("magi");
    std::fs::create_dir_all(&carpeta).unwrap();
    let clave = ClaveSincronizacion::generar();
    escribir_fichero_ajeno(&carpeta, &clave);
    abrir_unir(&mut prueba, &carpeta, clave.texto().as_str());
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.tecla(KeyCode::Esc);
    match &prueba.app.dialogo {
        Some(Dialogo::UnirEquipo { campo, error, .. }) => {
            assert_eq!(campo.texto, clave.texto().as_str());
            assert!(error
                .as_deref()
                .is_some_and(|error| error.contains("sin llavero no se puede guardar la clave")));
        }
        _ => panic!("debía volver UNIR ESTE EQUIPO"),
    }
    assert!(prueba.app.config.sincronizacion.carpeta.is_none());
}

#[test]
fn desactivar_sin_fichero_de_llavero_sigue_valiendo() {
    let mut prueba = Prueba::nueva();
    let carpeta = prueba.entorno.rutas.hogar.join("Sync");
    std::fs::create_dir_all(&carpeta).unwrap();
    prueba.app.config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
    // Alguien borró llavero.age a mano: no hay clave que olvidar y la
    // sincronización se desactiva sin pedir ninguna frase.
    confirmar(&mut prueba, AccionInventario::Desactivar);
    assert!(
        prueba.app.dialogo.is_none(),
        "{:?}",
        prueba.dialogo_llavero()
    );
    assert!(prueba.app.config.sincronizacion.carpeta.is_none());
    assert!(!prueba.ruta_llavero().exists());
}

#[test]
fn un_resultado_de_scrypt_llega_al_dialogo_aunque_este_apilado_debajo_de_una_pregunta() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.app.abrir_llavero_del_arranque();
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_some());
    // Mientras scrypt trabaja, el servidor pregunta algo: no pisa el diálogo.
    prueba.servidor(MensajeServidor::PideFrase {
        sesion_id: 1,
        host: "web-01".to_string(),
        intento: 1,
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::FraseServidor { .. })
    ));
    assert_eq!(prueba.app.dialogos_apilados(), 1);
    // El resultado cierra el del llavero, que está debajo, y deja la pregunta.
    prueba.esperar("desbloqueado", |p| {
        p.fichero.estado() == EstadoFichero::Abierto && p.app.dialogos_apilados() == 0
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::FraseServidor { .. })
    ));
}

// ------------------------------------------------------------------ escritorios

fn con_escritorio(prueba: &mut Prueba) -> (i64, u64) {
    {
        let almacen = magi::almacen::abrir(&prueba.entorno.rutas.base_datos()).expect("almacén");
        almacen
            .crear_host(
                &DatosHost {
                    nombre: "win-01".to_string(),
                    direccion: "192.0.2.50".to_string(),
                    puerto: 3389,
                    tipo: magi::escritorio::TipoHost::Rdp,
                    usuario: Some("administrador".to_string()),
                    identidad_ref: IdentidadRef::ContrasenaLlavero,
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("host rdp");
    }
    prueba.app.recargar_inventario().expect("recargar");
    let host_id = prueba.host("win-01");
    // Como en el servidor: la sesión entra en la lista (abriéndose) antes de
    // que pida las credenciales.
    prueba.servidor(MensajeServidor::Escritorios {
        lista: vec![info_escritorio(5, host_id, EstadoEscritorio::Abriendo)],
    });
    (host_id, 1 << 46)
}

/// La sesión de escritorio que difunde el servidor.
fn info_escritorio(id: u32, host_id: i64, estado: EstadoEscritorio) -> InfoEscritorio {
    InfoEscritorio {
        id,
        host_id,
        nombre: "win-01".to_string(),
        tipo: TipoHost::Rdp,
        estado,
        fase: None,
        motivo: None,
        visor: false,
        sin_visor_desde: None,
        ancho: 1280,
        alto: 720,
        imagen: None,
        fps: 0,
        directo: false,
        a_traves_de: None,
        desde: 0,
        redimensionable: false,
    }
}

fn pide_credenciales(prueba: &mut Prueba, peticion_id: u64) {
    prueba.servidor(MensajeServidor::PideCredenciales {
        peticion_id,
        escritorio_id: 5,
        host_nombre: "win-01".to_string(),
        peticion: magi::escritorio::PeticionCredenciales {
            tipo: TipoHost::Rdp,
            usuario: None,
            dominio: None,
            intento: 1,
        },
    });
}

fn credenciales_enviadas(mensajes: &[MensajeCliente]) -> Vec<(u64, String, String, bool)> {
    mensajes
        .iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::Credenciales {
                peticion_id,
                usuario,
                contrasena,
                recordar,
                ..
            } => Some((
                *peticion_id,
                usuario.clone(),
                contrasena.como_str().to_string(),
                *recordar,
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn un_escritorio_con_contrasena_del_llavero_cerrado_pide_antes_la_frase() {
    let mut prueba = Prueba::nueva();
    let (_, peticion_id) = con_escritorio(&mut prueba);
    prueba.crear_llavero_con_contrasenas();
    prueba
        .fichero
        .guardar_contrasena("win-01", "administrador", "clave-de-windows")
        .unwrap();
    prueba.fichero.bloquear();
    pide_credenciales(&mut prueba, peticion_id);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("credenciales enviadas", |p| p.app.dialogo.is_none());
    assert_eq!(
        credenciales_enviadas(&prueba.drenar()),
        vec![(
            peticion_id,
            "administrador".to_string(),
            "clave-de-windows".to_string(),
            false
        )]
    );
}

#[test]
fn un_escritorio_con_esc_en_el_llavero_pregunta_las_credenciales_sin_recordar() {
    let mut prueba = Prueba::nueva();
    let (_, peticion_id) = con_escritorio(&mut prueba);
    prueba.llavero_bloqueado();
    pide_credenciales(&mut prueba, peticion_id);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.tecla(KeyCode::Esc);
    match &prueba.app.dialogo {
        Some(Dialogo::Escritorio(DialogoEscritorio::Credenciales { recordar, .. })) => {
            assert!(!recordar, "sin «recordar»");
        }
        _ => panic!("debía salir el diálogo de credenciales"),
    }
    assert!(credenciales_enviadas(&prueba.drenar()).is_empty());
}

#[test]
fn un_escritorio_recordado_sin_llavero_y_esc_usa_las_credenciales_sin_guardarlas() {
    let mut prueba = Prueba::nueva();
    let (_, peticion_id) = con_escritorio(&mut prueba);
    pide_credenciales(&mut prueba, peticion_id);
    // Sin fichero no hay nada guardado: el diálogo sale con «recordar».
    match &prueba.app.dialogo {
        Some(Dialogo::Escritorio(DialogoEscritorio::Credenciales { recordar, .. })) => {
            assert!(recordar);
        }
        _ => panic!("debía salir el diálogo de credenciales"),
    }
    prueba.escribir("clave-de-windows");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    assert!(credenciales_enviadas(&prueba.drenar()).is_empty());
    prueba.tecla(KeyCode::Esc);
    assert_eq!(
        credenciales_enviadas(&prueba.drenar()),
        vec![(
            peticion_id,
            "administrador".to_string(),
            "clave-de-windows".to_string(),
            false
        )]
    );
    assert!(!prueba.ruta_llavero().exists());
}

#[test]
fn un_escritorio_recordado_con_el_llavero_nuevo_envia_y_guarda_al_abrir() {
    let mut prueba = Prueba::nueva();
    let (host_id, peticion_id) = con_escritorio(&mut prueba);
    pide_credenciales(&mut prueba, peticion_id);
    prueba.escribir("clave-de-windows");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("credenciales enviadas", |p| p.app.dialogo.is_none());
    assert_eq!(
        credenciales_enviadas(&prueba.drenar()),
        vec![(
            peticion_id,
            "administrador".to_string(),
            "clave-de-windows".to_string(),
            true
        )]
    );
    // El servidor da la sesión por abierta: la contraseña pasa al llavero.
    prueba.servidor(MensajeServidor::Escritorios {
        lista: vec![info_escritorio(5, host_id, EstadoEscritorio::Abierto)],
    });
    prueba.esperar("contraseña guardada", |p| {
        matches!(
            p.fichero.recuperar_contrasena("win-01", "administrador"),
            Ok(Some(_))
        )
    });
}

// ------------------------------------------------------------------ robustez

#[test]
fn una_continuacion_que_abre_un_dialogo_no_pierde_la_pregunta_del_servidor() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    let clave = magi::sincronizacion::cifrado::ClaveSincronizacion::generar();
    magi::sincronizacion::equipos::guardar_en_llavero(&clave, &*prueba.fichero).unwrap();
    prueba.fichero.bloquear();
    prueba.sesion_abierta(1, "web-01");
    // Mostrar la clave con el llavero cerrado: LLAVERO DE MAGI con su
    // continuación, y scrypt en marcha.
    confirmar(&mut prueba, AccionInventario::MostrarClave);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.dialogo_llavero().unwrap().trabajando.is_some());
    // El servidor pregunta algo encima del diálogo del llavero.
    prueba.servidor(MensajeServidor::PideFrase {
        sesion_id: 1,
        host: "web-01".to_string(),
        intento: 1,
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::FraseServidor { .. })
    ));
    // Al terminar scrypt, la clave se enseña y la pregunta no se pierde: queda
    // apilada y vuelve al cerrar la clave.
    prueba.esperar("clave a la vista", |p| {
        matches!(p.app.dialogo, Some(Dialogo::ClaveSincronizacion { .. }))
    });
    assert_eq!(prueba.app.dialogos_apilados(), 1);
    prueba.tecla(KeyCode::Esc);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::FraseServidor { .. })
    ));
}

#[test]
fn si_otra_ventana_crea_el_llavero_mientras_se_pide_la_frase_se_pasa_a_desbloquear() {
    let mut prueba = Prueba::nueva();
    prueba.app.pedir_llavero(Some(Continuacion::MostrarClave));
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    // Otra ventana crea el fichero antes de que esta pulse ↵.
    let otra = LlaveroFichero::con_ajustes(prueba.ruta_llavero(), LOG_N, Arc::new(Instant::now));
    otra.crear(OTRA_FRASE, OTRA_FRASE).unwrap();
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("cambio de diálogo", |p| {
        p.modo() == Some(ModoLlavero::Desbloquear)
    });
    let dialogo = prueba.dialogo_llavero().unwrap();
    assert_eq!(dialogo.continuaciones.len(), 1, "las continuaciones siguen");
    assert!(dialogo
        .error
        .as_deref()
        .is_some_and(|error| error.contains("otra ventana")));
    // Se desbloquea con la frase de la otra ventana y nada se anotó como creado.
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| {
        p.fichero.estado() == EstadoFichero::Abierto
    });
    assert!(prueba.anotados("llavero_creado").is_empty());
}

#[test]
fn si_el_fichero_desaparece_al_desbloquear_se_pasa_a_crear() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    prueba.escribir(FRASE);
    std::fs::remove_file(prueba.ruta_llavero()).unwrap();
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("cambio de diálogo", |p| {
        p.modo() == Some(ModoLlavero::Crear)
    });
    assert!(prueba
        .dialogo_llavero()
        .unwrap()
        .error
        .as_deref()
        .is_some_and(|error| error.contains("ya no existe")));
}

#[test]
fn la_marca_de_los_motivos_solo_reescribe_lo_que_toca() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    // Un motivo cualquiera no se toca aunque el llavero esté cerrado.
    prueba.servidor(MensajeServidor::Estado {
        sesion_id: 1,
        estado: EstadoSesionRemota::Caida,
        motivo: Some("conexión perdida".to_string()),
    });
    assert_eq!(
        prueba.app.pestanas[0].motivo.as_deref(),
        Some("conexión perdida")
    );
    // Con el llavero abierto, el motivo del servidor se deja como viene.
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    prueba.sesion_abierta(1, "web-01");
    let texto = "«web-01» usa contraseña y no hay ninguna guardada en el llavero; conéctate una \
                 vez con ↵ y marca «recordar»";
    prueba.servidor(MensajeServidor::Estado {
        sesion_id: 1,
        estado: EstadoSesionRemota::Caida,
        motivo: Some(texto.to_string()),
    });
    assert_eq!(prueba.app.pestanas[0].motivo.as_deref(), Some(texto));
}

// ------------------------------------------------------------------ otra ventana (U1)
//
// Dos ventanas de MAGI comparten `llavero.age`: lo que una hace con el fichero
// mientras la otra tiene un diálogo abierto no puede hacer que la otra renombre
// o borre lo que ya no es lo que el diálogo creía.

/// Otra ventana de MAGI: otro `LlaveroFichero` sobre el mismo fichero.
fn otra_ventana(prueba: &Prueba) -> LlaveroFichero {
    LlaveroFichero::con_ajustes(prueba.ruta_llavero(), LOG_N, Arc::new(Instant::now))
}

/// Las copias `llavero.age.danado-*` que hay en la carpeta de datos.
fn copias_danadas(prueba: &Prueba) -> Vec<String> {
    let mut copias: Vec<String> = std::fs::read_dir(&prueba.entorno.rutas.datos)
        .unwrap()
        .filter_map(|entrada| entrada.ok())
        .map(|entrada| entrada.file_name().to_string_lossy().to_string())
        .filter(|nombre| nombre.starts_with("llavero.age.danado-"))
        .collect();
    copias.sort();
    copias
}

/// Una conexión interactiva espera la frase (LLAVERO DAÑADO con su
/// continuación): lo que `pedir_contrasena_interactiva` deja al encontrar un
/// llavero que no se abre.
fn esperar_con_una_continuacion(prueba: &mut Prueba, sesion_id: u32) {
    prueba
        .app
        .pedir_llavero(Some(Continuacion::ReintentarContrasena {
            sesion_id,
            host: "web-01".to_string(),
            intento: 1,
            recordar_por_defecto: false,
        }));
}

#[test]
fn si_otra_ventana_arreglo_el_llavero_danado_no_se_aparta_el_bueno() {
    let mut prueba = Prueba::nueva();
    dejar_danado(&prueba);
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    esperar_con_una_continuacion(&mut prueba, 1);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));

    // Otra ventana aparta el roto y crea un llavero bueno, con una contraseña.
    let otra = otra_ventana(&prueba);
    otra.apartar_danado()
        .expect("la otra ventana aparta lo roto");
    otra.crear(OTRA_FRASE, OTRA_FRASE).expect("crea uno bueno");
    otra.guardar_contrasena("web-01", USUARIO, "clave-de-la-otra")
        .expect("guarda");
    let bueno = std::fs::read(prueba.ruta_llavero()).unwrap();
    assert_eq!(copias_danadas(&prueba).len(), 1, "solo la copia del roto");

    // Esta ventana, que seguía con el aviso de «dañado», pulsa apartar.
    prueba.tecla(KeyCode::Char('a'));

    // El fichero bueno sigue donde estaba, igual, y no hay copia nueva.
    assert_eq!(std::fs::read(prueba.ruta_llavero()).unwrap(), bueno);
    assert_eq!(
        copias_danadas(&prueba).len(),
        1,
        "se apartó el fichero bueno"
    );
    assert!(prueba.anotados("llavero_apartado").is_empty());
    // Se avisa y se vuelve a mirar el estado, como al arrancar: el llavero
    // existe y está cerrado, así que sale su frase con la misma continuación.
    assert_eq!(
        prueba.app.mensaje.as_ref().map(|m| m.texto.as_str()),
        Some("el llavero ya no está dañado")
    );
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    let continuaciones: Vec<&str> = prueba
        .dialogo_llavero()
        .unwrap()
        .continuaciones
        .iter()
        .map(Continuacion::nombre)
        .collect();
    assert_eq!(continuaciones, vec!["reintentar_contrasena"]);
    // Con la frase de la otra ventana, la continuación sigue su camino.
    prueba.escribir(OTRA_FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("contraseña enviada", |p| p.app.dialogo.is_none());
    prueba.bombear(100);
    assert_eq!(
        contrasenas_enviadas(&prueba.drenar()),
        vec![(1, "clave-de-la-otra".to_string(), false)]
    );
}

#[test]
fn si_otra_ventana_arreglo_el_llavero_danado_al_arrancar_sin_continuacion_sale_su_frase() {
    let mut prueba = Prueba::nueva();
    dejar_danado(&prueba);
    prueba.app.abrir_llavero_del_arranque();
    assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));
    let otra = otra_ventana(&prueba);
    otra.apartar_danado().unwrap();
    otra.crear(OTRA_FRASE, OTRA_FRASE).unwrap();
    otra.bloquear();
    prueba.tecla(KeyCode::Char('a'));
    assert!(prueba.ruta_llavero().exists());
    assert!(!prueba.app.salir);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    assert!(prueba.anotados("llavero_apartado").is_empty());
}

#[test]
fn si_otra_ventana_aparto_el_llavero_danado_una_continuacion_de_lectura_sigue_sin_llavero() {
    // «Mostrar la clave» solo lee: sin fichero no hay nada que pedir.
    let mut prueba = Prueba::nueva();
    dejar_danado(&prueba);
    prueba.app.pedir_llavero(Some(Continuacion::MostrarClave));
    assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));
    // Otra ventana lo aparta (y no crea otro): ya no existe.
    otra_ventana(&prueba).apartar_danado().unwrap();
    prueba.tecla(KeyCode::Char('a'));
    // Sin diálogo nuevo (no se pide crear un llavero para leer una clave que
    // no puede estar) y sin anotar un apartado que no hizo este equipo. El
    // mensaje que queda es el de la propia continuación al cancelarse.
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.anotados("llavero_apartado").is_empty());
    assert!(!prueba.ruta_llavero().exists());
}

#[test]
fn apartar_un_llavero_que_otra_ventana_ya_aparto_sigue_sin_llavero_sin_anotar() {
    let mut prueba = Prueba::nueva();
    dejar_danado(&prueba);
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    esperar_con_una_continuacion(&mut prueba, 1);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));
    // La otra ventana lo apartó y no hay fichero.
    otra_ventana(&prueba).apartar_danado().unwrap();
    assert!(!prueba.ruta_llavero().exists());

    prueba.tecla(KeyCode::Char('a'));

    // Sin llavero, igual que tras apartarlo: la continuación cae a su camino
    // sin llavero (el diálogo normal de contraseña), pero no se anota un
    // `llavero_apartado` que no hizo este diálogo, y el mensaje lo explica.
    assert_eq!(
        prueba.app.mensaje.as_ref().map(|m| m.texto.as_str()),
        Some("el llavero ya no existe: sigues sin llavero")
    );
    assert!(prueba.anotados("llavero_apartado").is_empty());
    assert_eq!(
        copias_danadas(&prueba).len(),
        1,
        "solo la que apartó la otra"
    );
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ContrasenaServidor { sesion_id: 1, .. })
    ));
    assert!(!prueba.ruta_llavero().exists());
}

#[test]
fn borrar_un_llavero_que_se_estropeo_pasa_a_llavero_danado_y_no_lo_borra() {
    let mut prueba = Prueba::nueva();
    prueba.crear_llavero_con_contrasenas();
    abrir_por_la_paleta(&mut prueba, "Borrar el llavero");
    assert_eq!(prueba.modo(), Some(ModoLlavero::Borrar));
    prueba.escribir("borrar");
    // Entre abrir el diálogo y confirmar, el fichero se estropea.
    let basura = dejar_danado(&prueba);

    prueba.tecla(KeyCode::Enter);

    // El fichero está donde estaba, sin tocar; no se anotó ningún borrado; y
    // el diálogo ofrece lo que toca: apartarlo.
    assert_eq!(std::fs::read(prueba.ruta_llavero()).unwrap(), basura);
    assert!(prueba.anotados("llavero_borrado").is_empty());
    assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));
    let texto = prueba.texto();
    for esperado in ["LLAVERO DAÑADO", "No se puede leer llavero.age", "apartar"] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
    let dialogo = prueba.dialogo_llavero().unwrap();
    assert!(dialogo.campos.is_empty());
    assert!(dialogo
        .motivo_danado
        .as_deref()
        .is_some_and(|motivo| !motivo.is_empty()));
    // Desde ahí se aparta (nunca se borra).
    prueba.tecla(KeyCode::Char('a'));
    assert!(prueba.app.dialogo.is_none());
    assert!(!prueba.ruta_llavero().exists());
    assert_eq!(copias_danadas(&prueba).len(), 1);
    assert_eq!(prueba.anotados("llavero_apartado").len(), 1);
}

// ------------------------------------------------------------------ peticiones ignoradas (U2)

/// El servidor pide la contraseña de una pestaña que se reconecta sola.
fn pide_llavero(prueba: &mut Prueba, sesion_id: u32, host: &str) {
    prueba.servidor(MensajeServidor::PideLlavero {
        sesion_id,
        host: host.to_string(),
        usuario: USUARIO.to_string(),
    });
}

/// Desbloquea el llavero con la frase por la paleta (como haría el usuario).
fn desbloquear_por_la_paleta(prueba: &mut Prueba) {
    abrir_por_la_paleta(prueba, "Desbloquear el llavero");
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| p.app.dialogo.is_none());
}

/// Bombea hasta recoger `cuantas` contraseñas enviadas (salen de una tarea).
fn contrasenas_hasta(prueba: &mut Prueba, cuantas: usize) -> Vec<(u32, String, bool)> {
    let limite = Instant::now() + Duration::from_secs(10);
    let mut enviadas = Vec::new();
    while enviadas.len() < cuantas {
        assert!(
            Instant::now() < limite,
            "se enviaron {} contraseñas de {cuantas} en 10 s",
            enviadas.len()
        );
        prueba.bombear(40);
        enviadas.extend(contrasenas_enviadas(&prueba.drenar()));
    }
    enviadas
}

#[test]
fn una_peticion_de_pestana_ignorada_se_contesta_si_se_desbloquea_dentro_del_plazo() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    pide_llavero(&mut prueba, 1, "web-01");
    prueba.bombear(50);
    // Sin diálogo y sin contestar (un `Cerrar` cerraría la pestaña)…
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.drenar().is_empty());
    // …pero se recuerda.
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 1);

    desbloquear_por_la_paleta(&mut prueba);

    // La contestación sale de una tarea con la contraseña del llavero.
    assert_eq!(
        contrasenas_hasta(&mut prueba, 1),
        vec![(1, format!("{CONTRASENA}-web-01"), false)]
    );
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 0);
    // Una sola vez: no se repite.
    prueba.bombear(100);
    assert!(contrasenas_enviadas(&prueba.drenar()).is_empty());
}

#[test]
fn una_peticion_vencida_no_se_contesta_y_el_tope_acota_la_memoria() {
    // Vencida: el servidor ya no espera.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    pide_llavero(&mut prueba, 1, "web-01");
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 1);
    prueba
        .app
        .envejecer_peticiones_llavero(magi_nucleo::conexion::cliente::PLAZO_LLAVERO);
    desbloquear_por_la_paleta(&mut prueba);
    prueba.bombear(300);
    assert!(contrasenas_enviadas(&prueba.drenar()).is_empty());
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 0);

    // Una sesión repetida sustituye a su petición anterior, no la duplica.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    pide_llavero(&mut prueba, 1, "web-01");
    pide_llavero(&mut prueba, 1, "web-01");
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 1);

    // Tope: 64 como mucho, quedándose con las más recientes.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    let host_id = prueba.host("web-01");
    let sesiones = (1..=100u32)
        .map(|id| sesion(id, "web-01", host_id))
        .collect();
    prueba.servidor(bienvenida(sesiones));
    prueba.drenar();
    for id in 1..=100u32 {
        pide_llavero(&mut prueba, id, "web-01");
    }
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 64);
    desbloquear_por_la_paleta(&mut prueba);
    let mut ids: Vec<u32> = contrasenas_hasta(&mut prueba, 64)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (37..=100).collect::<Vec<u32>>());
}

#[test]
fn una_peticion_de_una_pestana_que_ya_no_existe_no_se_contesta_al_desbloquear() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    pide_llavero(&mut prueba, 1, "web-01");
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 1);
    // La pestaña se cierra mientras se pide la frase.
    prueba.servidor(MensajeServidor::Sesiones { lista: Vec::new() });
    assert!(prueba.app.pestanas.is_empty());
    desbloquear_por_la_paleta(&mut prueba);
    prueba.bombear(300);
    assert!(prueba.drenar().is_empty());
}

#[test]
fn una_ejecucion_con_el_llavero_cerrado_no_deja_peticion_recordada() {
    // Lo que no es una pestaña sigue cerrándose en el acto, sin recordarse.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    pide_llavero(&mut prueba, 9000, "web-01");
    assert!(matches!(
        prueba.drenar().as_slice(),
        [MensajeCliente::Cerrar { sesion_id: 9000 }]
    ));
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 0);
}

#[test]
fn al_desbloquear_los_motivos_de_llavero_bloqueado_dejan_de_decirlo() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    let host_id = prueba.host("web-01");
    let otro_id = prueba.host("web-02");
    prueba.servidor(bienvenida(vec![
        sesion(1, "web-01", host_id),
        sesion(2, "web-02", otro_id),
    ]));
    let sin_contrasena = "necesita tu respuesta: prefijo r · «web-01» usa contraseña y no hay \
                          ninguna guardada en el llavero; conéctate una vez con ↵ y marca \
                          «recordar»";
    // Una pestaña caída y otra que se está reabriendo dicen «llavero bloqueado».
    prueba.servidor(MensajeServidor::Estado {
        sesion_id: 1,
        estado: EstadoSesionRemota::Caida,
        motivo: Some(sin_contrasena.to_string()),
    });
    prueba.servidor(MensajeServidor::Estado {
        sesion_id: 2,
        estado: EstadoSesionRemota::Abriendo,
        motivo: Some(sin_contrasena.to_string()),
    });
    assert_eq!(prueba.app.pestanas[0].motivo.as_deref(), Some(BLOQUEADO));
    assert_eq!(prueba.app.pestanas[1].motivo.as_deref(), Some(BLOQUEADO));

    desbloquear_por_la_paleta(&mut prueba);

    // La caída lo dice con lo que toca ahora; la que abre, sin motivo.
    assert_eq!(
        prueba.app.pestanas[0].motivo.as_deref(),
        Some("llavero desbloqueado: reconecta la pestaña")
    );
    assert_eq!(prueba.app.pestanas[1].motivo, None);
    // Un motivo cualquiera no se toca.
    prueba.servidor(MensajeServidor::Estado {
        sesion_id: 1,
        estado: EstadoSesionRemota::Caida,
        motivo: Some("conexión perdida".to_string()),
    });
    assert_eq!(
        prueba.app.pestanas[0].motivo.as_deref(),
        Some("conexión perdida")
    );
}

// ------------------------------------------------------------------ escritorios y servidor caído (U3)

#[test]
fn las_credenciales_recordadas_de_un_escritorio_que_ya_no_se_abre_se_descartan() {
    let mut prueba = Prueba::nueva();
    let (host_id, peticion_id) = con_escritorio(&mut prueba);
    pide_credenciales(&mut prueba, peticion_id);
    prueba.escribir("clave-de-windows");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
    // Mientras se piensa la frase, el escritorio desaparece de la lista (el
    // servidor lo dio por perdido).
    prueba.servidor(MensajeServidor::Escritorios { lista: Vec::new() });
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("llavero creado", |p| p.app.dialogo.is_none());
    // Nada se envía, y se dice por qué.
    assert!(credenciales_enviadas(&prueba.drenar()).is_empty());
    assert_eq!(
        prueba.app.mensaje.as_ref().map(|m| m.texto.as_str()),
        Some("el escritorio ya no se está abriendo: credenciales descartadas")
    );
    // Y no queda una contraseña esperando a un escritorio que ya no existe:
    // si más tarde una sesión con ese id abre, no se guarda nada.
    prueba.servidor(MensajeServidor::Escritorios {
        lista: vec![info_escritorio(5, host_id, EstadoEscritorio::Abierto)],
    });
    prueba.bombear(300);
    assert!(matches!(
        prueba
            .fichero
            .recuperar_contrasena("win-01", "administrador"),
        Ok(None)
    ));
}

#[test]
fn si_el_escritorio_ya_no_se_abre_la_frase_no_reabre_su_pregunta_ni_con_esc() {
    // Desbloquear: la pregunta de credenciales ya caducó, no hay diálogo.
    let mut prueba = Prueba::nueva();
    let (host_id, peticion_id) = con_escritorio(&mut prueba);
    prueba.crear_llavero_con_contrasenas();
    prueba
        .fichero
        .guardar_contrasena("win-01", "administrador", "clave-de-windows")
        .unwrap();
    prueba.fichero.bloquear();
    pide_credenciales(&mut prueba, peticion_id);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.servidor(MensajeServidor::Escritorios {
        lista: vec![info_escritorio(5, host_id, EstadoEscritorio::Caido)],
    });
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| p.app.dialogo.is_none());
    prueba.bombear(100);
    assert!(credenciales_enviadas(&prueba.drenar()).is_empty());
    assert!(prueba.app.dialogo.is_none());

    // Esc: tampoco se abre el diálogo de credenciales de una apertura muerta.
    let mut prueba = Prueba::nueva();
    let (_, peticion_id) = con_escritorio(&mut prueba);
    prueba.llavero_bloqueado();
    pide_credenciales(&mut prueba, peticion_id);
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.servidor(MensajeServidor::Escritorios { lista: Vec::new() });
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(credenciales_enviadas(&prueba.drenar()).is_empty());
}

#[test]
fn con_el_escritorio_abriendose_las_continuaciones_siguen_como_siempre() {
    // La comprobación solo descarta lo caducado: Esc con el escritorio vivo
    // sigue abriendo el diálogo de credenciales sin «recordar».
    let mut prueba = Prueba::nueva();
    let (_, peticion_id) = con_escritorio(&mut prueba);
    prueba.llavero_bloqueado();
    pide_credenciales(&mut prueba, peticion_id);
    prueba.tecla(KeyCode::Esc);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Escritorio(DialogoEscritorio::Credenciales {
            recordar: false,
            ..
        }))
    ));
}

#[test]
fn si_cae_el_servidor_se_cancelan_las_continuaciones_de_sesion_y_escritorio_apiladas() {
    let mut prueba = Prueba::nueva();
    let (_, peticion_id) = con_escritorio(&mut prueba);
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    // Un diálogo del llavero con una continuación de cada tipo: la de sesión,
    // la de escritorio y una que no es del servidor (mostrar la clave).
    prueba
        .app
        .pedir_llavero(Some(Continuacion::ReintentarContrasena {
            sesion_id: 1,
            host: "web-01".to_string(),
            intento: 1,
            recordar_por_defecto: false,
        }));
    prueba
        .app
        .pedir_llavero(Some(Continuacion::EnviarCredenciales {
            peticion_id,
            escritorio_id: 5,
            credenciales: magi::escritorio::CredencialesEscritorio {
                usuario: "administrador".to_string(),
                dominio: None,
                contrasena: zeroize::Zeroizing::new("clave-de-windows".to_string()),
                recordar: true,
            },
        }));
    prueba.app.pedir_llavero(Some(Continuacion::MostrarClave));
    assert_eq!(prueba.dialogo_llavero().unwrap().continuaciones.len(), 3);
    // El servidor pregunta algo encima: el del llavero queda apilado debajo.
    prueba.servidor(MensajeServidor::PideFrase {
        sesion_id: 1,
        host: "web-01".to_string(),
        intento: 1,
    });
    assert_eq!(prueba.app.dialogos_apilados(), 1);

    prueba.evento(Evento::ServidorCaido);

    // Nada de lo cancelado abrió un diálogo ni contestó a nadie: a la vista,
    // solo SERVIDOR CAÍDO, y el del llavero sigue debajo con lo que no era del
    // servidor.
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ServidorCaido { .. })
    ));
    assert!(prueba.drenar().is_empty());
    assert_eq!(prueba.app.dialogos_apilados(), 1);
    prueba.tecla(KeyCode::Char('n'));
    let continuaciones: Vec<&str> = prueba
        .dialogo_llavero()
        .expect("el diálogo del llavero vuelve")
        .continuaciones
        .iter()
        .map(Continuacion::nombre)
        .collect();
    assert_eq!(continuaciones, vec!["mostrar_clave"]);
    assert!(prueba.drenar().is_empty());
    // Al desbloquearlo después no se contesta nada a un servidor que no está.
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| {
        p.fichero.estado() == EstadoFichero::Abierto
    });
    prueba.bombear(100);
    assert!(prueba.drenar().is_empty());
}

#[test]
fn si_cae_el_servidor_con_el_dialogo_del_llavero_a_la_vista_tambien_se_cancelan() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.sesion_abierta(1, "web-01");
    prueba.drenar();
    prueba
        .app
        .pedir_llavero(Some(Continuacion::EnviarContrasena {
            sesion_id: 1,
            contrasena: zeroize::Zeroizing::new(CONTRASENA.to_string()),
        }));
    pide_llavero(&mut prueba, 1, "web-01");
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 1);
    prueba.evento(Evento::ServidorCaido);
    // SERVIDOR CAÍDO sustituye al diálogo; la contraseña no se envía (ni se
    // deja a punto de guardarse) y no queda nada recordado.
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ServidorCaido { .. })
    ));
    assert!(prueba.drenar().is_empty());
    assert_eq!(prueba.app.peticiones_llavero_recordadas(), 0);
    assert!(
        prueba
            .app
            .mensaje
            .as_ref()
            .is_none_or(|mensaje| !mensaje.texto.contains("contraseña usada")),
        "no dice «contraseña usada» de una sesión que ya no existe"
    );
}

// ------------------------------------------------------------------ la barra no hace E/S (U4)

/// Un llavero de fichero que cuenta cuántas veces le preguntan su estado.
struct LlaveroContador {
    dentro: Arc<LlaveroFichero>,
    consultas: Arc<std::sync::atomic::AtomicUsize>,
}

impl LlaveroSecreto for LlaveroContador {
    fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String> {
        self.dentro.guardar(cuenta, valor)
    }

    fn recuperar(&self, cuenta: &str) -> Result<Option<zeroize::Zeroizing<String>>, String> {
        self.dentro.recuperar(cuenta)
    }

    fn olvidar(&self, cuenta: &str) -> Result<bool, String> {
        self.dentro.olvidar(cuenta)
    }
}

impl LlaveroHost for LlaveroContador {
    fn guardar_contrasena(
        &self,
        host: &str,
        usuario: &str,
        contrasena: &str,
    ) -> Result<(), String> {
        self.dentro.guardar_contrasena(host, usuario, contrasena)
    }

    fn recuperar_contrasena(
        &self,
        host: &str,
        usuario: &str,
    ) -> Result<Option<zeroize::Zeroizing<String>>, String> {
        self.dentro.recuperar_contrasena(host, usuario)
    }
}

impl LlaveroCliente for LlaveroContador {
    fn estado(&self) -> EstadoLlavero {
        self.consultas
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        LlaveroCliente::estado(&*self.dentro)
    }

    fn disponible(&self) -> bool {
        true
    }

    fn olvidar_contrasena(&self, host: &str, usuario: &str) -> Result<bool, String> {
        self.dentro.olvidar_contrasena(host, usuario)
    }

    fn de_fichero(&self) -> Option<&LlaveroFichero> {
        Some(&self.dentro)
    }
}

#[test]
fn pintar_la_barra_no_consulta_el_llavero_y_la_marca_se_pone_al_dia_en_el_tick() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    let consultas = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    prueba.app.llavero = Arc::new(LlaveroContador {
        dentro: prueba.fichero.clone(),
        consultas: consultas.clone(),
    });
    let consultado = || consultas.load(std::sync::atomic::Ordering::SeqCst);

    // Algo necesita el llavero y está cerrado: la marca sale (esta consulta
    // es del evento, no del pintado).
    prueba.app.nota_llavero_necesitado(Some(BLOQUEADO));
    assert!(prueba.app.marca_llavero_bloqueado());
    let tras_el_evento = consultado();
    assert!(tras_el_evento >= 1);

    // Muchos pintados seguidos, con la marca puesta: ninguna consulta.
    for _ in 0..50 {
        // Un pegado vacío solo marca la pantalla como sucia: se repinta todo.
        prueba.evento(Evento::Pegado(String::new()));
    }
    assert!(prueba
        .texto()
        .lines()
        .last()
        .unwrap()
        .contains("llavero bloqueado"));
    assert_eq!(
        consultado(),
        tras_el_evento,
        "el pintado de la barra consultó el estado del llavero"
    );

    // El tick sí la pone al día: la frase entró en memoria sin pasar por la
    // interfaz, y la marca se va sin que nada más lo toque.
    prueba.fichero.desbloquear(FRASE).unwrap();
    assert!(prueba.app.marca_llavero_bloqueado(), "no cambia sin tick");
    prueba.evento(Evento::Tick);
    assert!(!prueba.app.marca_llavero_bloqueado());
    assert!(!prueba
        .texto()
        .lines()
        .last()
        .unwrap()
        .contains("llavero bloqueado"));
}

// ------------------------------------------------------------------ pegar una frase (U6)

#[test]
fn pegar_una_frase_borra_el_texto_del_evento_de_la_memoria() {
    let _cerrojo = CERROJO_VIGILANTE.lock().unwrap_or_else(|e| e.into_inner());

    // La función que reparte el pegado deja a ceros el `String` del evento
    // (el mismo búfer, entero) y la frase llega al campo.
    for modo in [
        ModoLlavero::Desbloquear,
        ModoLlavero::Crear,
        ModoLlavero::Cambiar,
    ] {
        let mut dialogo = Dialogo::Llavero(Box::new(DialogoLlavero::nuevo(modo)));
        let mut texto = String::with_capacity(256);
        texto.push_str(FRASE);
        texto.push('\n');
        let (puntero, capacidad) = (texto.as_ptr() as usize, texto.capacity());
        assert!(magi::app::sesion::pegar_en_dialogo(
            &mut dialogo,
            &mut texto
        ));
        assert!(texto.is_empty());
        assert_eq!(
            (texto.as_ptr() as usize, texto.capacity()),
            (puntero, capacidad)
        );
        assert!(
            bytes_del_bufer(&texto).iter().all(|byte| *byte == 0),
            "{modo:?}: la frase sigue en el búfer del evento"
        );
        let Dialogo::Llavero(dialogo) = &dialogo else {
            unreachable!()
        };
        assert_eq!(dialogo.campos[0].texto, FRASE, "sin el salto de línea");
    }

    // BORRAR no pide una frase: el texto del evento no es un secreto.
    let mut dialogo = Dialogo::Llavero(Box::new(DialogoLlavero::nuevo(ModoLlavero::Borrar)));
    let mut texto = "borrar".to_string();
    assert!(magi::app::sesion::pegar_en_dialogo(
        &mut dialogo,
        &mut texto
    ));
    assert_eq!(texto, "borrar");

    // Por el camino real (`Evento::Pegado`): el búfer que viajó en el evento
    // se libera a ceros.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    let mut texto = String::with_capacity(256);
    texto.push_str(FRASE);
    vigilar(&texto);
    prueba.evento(Evento::Pegado(texto));
    assert_eq!(prueba.dialogo_llavero().unwrap().campos[0].texto, FRASE);
    let resultado = liberado_a_cero();
    dejar_de_vigilar();
    assert_eq!(
        resultado,
        Some(true),
        "el texto pegado quedó en memoria al soltar el evento"
    );
}

#[test]
fn una_frase_larga_pegada_no_deja_copias_a_medias_al_crecer_el_campo() {
    let _cerrojo = CERROJO_VIGILANTE.lock().unwrap_or_else(|e| e.into_inner());
    // Más de lo que cabe en el búfer reservado del campo: el campo pasa a un
    // búfer mayor, y el anterior (con lo que ya había) se borra antes de soltarlo.
    let mut dialogo = DialogoLlavero::nuevo(ModoLlavero::Desbloquear);
    dialogo.campos[0].texto.push_str(FRASE);
    dialogo.campos[0].cursor = FRASE.chars().count();
    let capacidad = dialogo.campos[0].texto.capacity();
    vigilar(&dialogo.campos[0].texto);
    let larga = "x".repeat(capacidad + 500);
    assert!(dialogo.pegar_texto(&larga));
    assert_eq!(dialogo.campos[0].texto.len(), FRASE.len() + larga.len());
    assert!(dialogo.campos[0].texto.capacity() > capacidad);
    let resultado = liberado_a_cero();
    dejar_de_vigilar();
    assert_eq!(
        resultado,
        Some(true),
        "el búfer anterior se soltó con la frase dentro"
    );

    // Lo mismo escribiendo tecla a tecla hasta pasarse del búfer.
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    prueba.app.abrir_llavero_del_arranque();
    {
        let Some(Dialogo::Llavero(dialogo)) = &mut prueba.app.dialogo else {
            panic!("debía estar el diálogo del llavero");
        };
        let justo = "y".repeat(dialogo.campos[0].texto.capacity());
        dialogo.campos[0].texto.push_str(&justo);
        dialogo.campos[0].cursor = justo.chars().count();
        assert_eq!(
            dialogo.campos[0].texto.len(),
            dialogo.campos[0].texto.capacity()
        );
        vigilar(&dialogo.campos[0].texto);
    }
    prueba.tecla(KeyCode::Char('z'));
    let resultado = liberado_a_cero();
    dejar_de_vigilar();
    assert_eq!(
        resultado,
        Some(true),
        "al teclear más de lo reservado el búfer anterior se soltó con la frase"
    );
}

// ------------------------------------------------------------------ cierre de la Fase 17 (pista B)

/// Lo que la tarea de la pasada devolvió al bucle: las claves con las que
/// leería y exportaría, o el error que la paró.
enum EsperaDeLaPasada {
    Claves(Vec<String>),
    Error(String),
}

/// Lanza la espera de la pasada con `llavero` (como `solicitar_pasada`) y
/// recoge lo que manda al bucle, sin pasar por él: así se ven las claves.
fn esperar_de_la_pasada(
    prueba: &Prueba,
    llavero: Arc<dyn LlaveroCliente>,
    config: Config,
) -> EsperaDeLaPasada {
    use magi::app::sincronizacion::EventoInventario;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    magi::app::sincronizacion::lanzar_espera(
        &prueba.app.runtime,
        tx,
        prueba.entorno.rutas.clone(),
        config,
        llavero,
    );
    let evento = prueba
        .app
        .runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(20), rx.recv()).await })
        .expect("la espera de la pasada no contestó en 20 s")
        .expect("el canal se cerró");
    match evento {
        Evento::Inventario(EventoInventario::Listo { claves, .. }) => EsperaDeLaPasada::Claves(
            claves
                .iter()
                .map(|clave| clave.texto().to_string())
                .collect(),
        ),
        Evento::Inventario(EventoInventario::Error(motivo)) => EsperaDeLaPasada::Error(motivo),
        _ => panic!("evento inesperado de la espera de la pasada"),
    }
}

/// Config con la sincronización configurada y la clave anterior vigente.
fn config_con_sincronizacion(prueba: &Prueba) -> Config {
    let mut config = prueba.app.config.clone();
    config.sincronizacion.carpeta = Some(
        prueba
            .entorno
            .rutas
            .hogar
            .join("Sync")
            .to_string_lossy()
            .to_string(),
    );
    config.sincronizacion.clave_anterior_hasta = Some(
        (chrono::Local::now() + chrono::Duration::days(10))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string(),
    );
    config
}

/// Dos ventanas con la clave K1 de sincronización guardada: la que está en la
/// App (la W1) y otra con su propia memoria (la W2, la que hace la pasada).
fn dos_ventanas_con_clave(
    prueba: &mut Prueba,
) -> (
    magi::sincronizacion::cifrado::ClaveSincronizacion,
    Arc<LlaveroFichero>,
) {
    prueba.fichero.crear(FRASE, FRASE).expect("crear");
    let k1 = magi::sincronizacion::cifrado::ClaveSincronizacion::generar();
    magi::sincronizacion::equipos::guardar_en_llavero(&k1, &*prueba.fichero).unwrap();
    let w2 = Arc::new(otra_ventana(prueba));
    w2.desbloquear(FRASE).expect("desbloquear la otra ventana");
    (k1, w2)
}

#[test]
fn la_pasada_con_la_clave_vieja_en_memoria_lee_la_nueva_de_otra_ventana() {
    let mut prueba = Prueba::nueva();
    let (k1, w2) = dos_ventanas_con_clave(&mut prueba);
    let config = config_con_sincronizacion(&prueba);
    // La otra ventana rota la clave: la actual pasa a ser la anterior.
    let k2 = magi::sincronizacion::cifrado::ClaveSincronizacion::generar();
    prueba
        .fichero
        .guardar(CUENTA_SINCRONIZACION_ANTERIOR, k1.texto().as_str())
        .unwrap();
    magi::sincronizacion::equipos::guardar_en_llavero(&k2, &*prueba.fichero).unwrap();
    // W2 todavía tiene la K1 en la memoria…
    assert_eq!(
        w2.recuperar(CUENTA_SINCRONIZACION)
            .unwrap()
            .as_deref()
            .map(String::as_str),
        Some(k1.texto().as_str())
    );
    // …y la pasada, con su llavero, lee la nueva (y la anterior para leer).
    match esperar_de_la_pasada(&prueba, w2.clone(), config) {
        EsperaDeLaPasada::Claves(claves) => {
            assert_eq!(
                claves,
                vec![k2.texto().to_string(), k1.texto().to_string()],
                "primero la actual (con la que se exporta), después la anterior"
            );
        }
        EsperaDeLaPasada::Error(error) => panic!("no debía fallar: {error}"),
    }
}

#[test]
fn si_otra_ventana_cambia_la_frase_la_pasada_se_bloquea_y_no_exporta() {
    let mut prueba = Prueba::nueva();
    let (_k1, w2) = dos_ventanas_con_clave(&mut prueba);
    let config = config_con_sincronizacion(&prueba);
    prueba
        .fichero
        .cambiar_frase(FRASE, OTRA_FRASE, OTRA_FRASE)
        .unwrap();
    match esperar_de_la_pasada(&prueba, w2.clone(), config) {
        EsperaDeLaPasada::Error(error) => assert_eq!(error, BLOQUEADO),
        EsperaDeLaPasada::Claves(_) => panic!("exportó con una clave que ya no es la del fichero"),
    }
    // La W2 quedó cerrada: pedirá la frase nueva.
    assert_eq!(w2.estado(), EstadoFichero::Cerrado);
}

#[test]
fn desactivar_en_otra_ventana_deja_a_la_pasada_sin_clave_y_no_resucita_el_fichero() {
    let mut prueba = Prueba::nueva();
    let (_k1, w2) = dos_ventanas_con_clave(&mut prueba);
    let config = config_con_sincronizacion(&prueba);
    // La otra ventana desactiva la sincronización: olvida la clave.
    assert!(prueba.fichero.olvidar(CUENTA_SINCRONIZACION).unwrap());
    match esperar_de_la_pasada(&prueba, w2.clone(), config) {
        EsperaDeLaPasada::Claves(claves) => {
            assert!(
                claves.is_empty(),
                "sin clave la pasada dice «falta la clave»"
            );
        }
        EsperaDeLaPasada::Error(error) => panic!("no debía fallar: {error}"),
    }
}

#[test]
fn la_espera_de_la_pasada_con_otro_llavero_no_cambia() {
    // El llavero del sistema y el de memoria no tienen nada que refrescar.
    let prueba = Prueba::nueva();
    let en_memoria = Arc::new(magi::llavero::LlaveroEnMemoria::nuevo());
    let clave = magi::sincronizacion::cifrado::ClaveSincronizacion::generar();
    magi::sincronizacion::equipos::guardar_en_llavero(&clave, &*en_memoria).unwrap();
    let config = config_con_sincronizacion(&prueba);
    match esperar_de_la_pasada(&prueba, en_memoria, config) {
        EsperaDeLaPasada::Claves(claves) => {
            assert_eq!(claves, vec![clave.texto().to_string()]);
        }
        EsperaDeLaPasada::Error(error) => panic!("no debía fallar: {error}"),
    }
}

// -------------------------------------------------- la clave anterior vencida

/// Una clave anterior guardada y vencida (ayer) con el llavero abierto; deja
/// el llavero cerrado, como al abrir MAGI.
fn clave_anterior_vencida(prueba: &mut Prueba) {
    prueba.fichero.crear(FRASE, FRASE).expect("crear");
    prueba
        .fichero
        .guardar(CUENTA_SINCRONIZACION, "clave-actual")
        .unwrap();
    prueba
        .fichero
        .guardar(CUENTA_SINCRONIZACION_ANTERIOR, "clave-vieja")
        .unwrap();
    prueba.app.config.sincronizacion.clave_anterior_hasta = Some(
        (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string(),
    );
    prueba.fichero.bloquear();
}

#[test]
fn la_clave_anterior_vencida_se_olvida_al_desbloquear_el_llavero() {
    let mut prueba = Prueba::nueva();
    clave_anterior_vencida(&mut prueba);
    // Al arrancar el llavero está cerrado: no se puede olvidar y no se toca
    // nada (ni la clave ni la fecha).
    prueba.app.olvidar_clave_anterior_vencida();
    assert!(prueba
        .app
        .config
        .sincronizacion
        .clave_anterior_hasta
        .is_some());
    // Se desbloquea como siempre (diálogo del arranque, frase, ↵).
    prueba.app.abrir_lo_del_arranque();
    assert_eq!(prueba.modo(), Some(ModoLlavero::Desbloquear));
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| {
        p.fichero.estado() == EstadoFichero::Abierto
    });
    // Ahora sí: la clave vieja sale del llavero y la fecha de la
    // configuración (en memoria y en config.toml); la actual sigue.
    assert!(prueba
        .fichero
        .recuperar(CUENTA_SINCRONIZACION_ANTERIOR)
        .unwrap()
        .is_none());
    assert!(prueba
        .fichero
        .recuperar(CUENTA_SINCRONIZACION)
        .unwrap()
        .is_some());
    assert!(prueba
        .app
        .config
        .sincronizacion
        .clave_anterior_hasta
        .is_none());
    let guardado = std::fs::read_to_string(prueba.entorno.rutas.fichero_config())
        .expect("config.toml guardado");
    assert!(!guardado.contains("clave_anterior_hasta"), "{guardado}");
}

#[test]
fn la_clave_anterior_vigente_no_se_olvida_al_desbloquear() {
    let mut prueba = Prueba::nueva();
    clave_anterior_vencida(&mut prueba);
    let futura = (chrono::Local::now() + chrono::Duration::days(5))
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    prueba.app.config.sincronizacion.clave_anterior_hasta = Some(futura.clone());
    prueba.app.abrir_lo_del_arranque();
    prueba.escribir(FRASE);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("desbloqueado", |p| {
        p.fichero.estado() == EstadoFichero::Abierto
    });
    assert!(prueba
        .fichero
        .recuperar(CUENTA_SINCRONIZACION_ANTERIOR)
        .unwrap()
        .is_some());
    assert_eq!(
        prueba
            .app
            .config
            .sincronizacion
            .clave_anterior_hasta
            .as_deref(),
        Some(futura.as_str())
    );
}

#[test]
fn la_clave_anterior_vencida_sin_fichero_de_llavero_no_deja_la_fecha() {
    let mut prueba = Prueba::nueva();
    prueba.app.config.sincronizacion.clave_anterior_hasta = Some(
        (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string(),
    );
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Inexistente);
    prueba.app.olvidar_clave_anterior_vencida();
    assert!(prueba
        .app
        .config
        .sincronizacion
        .clave_anterior_hasta
        .is_none());
    assert!(!prueba.ruta_llavero().exists(), "no crea el llavero");
}

// -------------------------------------------------- servidor caído con el llavero a la vista

#[test]
fn si_cae_el_servidor_el_llavero_a_la_vista_no_pierde_sus_continuaciones_ajenas() {
    let mut prueba = Prueba::nueva();
    prueba.llavero_bloqueado();
    // El llavero espera la frase para rotar la clave y para mostrarla: nada de
    // eso contesta al servidor.
    prueba.app.pedir_llavero(Some(Continuacion::MostrarClave));
    prueba.app.pedir_llavero(Some(Continuacion::RotarClave));
    assert_eq!(prueba.dialogo_llavero().unwrap().continuaciones.len(), 2);
    prueba.evento(Evento::ServidorCaido);
    // SERVIDOR CAÍDO a la vista y el llavero debajo, con todo lo suyo.
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ServidorCaido { .. })
    ));
    assert_eq!(prueba.app.dialogos_apilados(), 1);
    prueba.tecla(KeyCode::Char('n'));
    let continuaciones: Vec<&str> = prueba
        .dialogo_llavero()
        .expect("el diálogo del llavero vuelve")
        .continuaciones
        .iter()
        .map(Continuacion::nombre)
        .collect();
    assert_eq!(continuaciones, vec!["mostrar_clave", "rotar_clave"]);
    // Y con Esc se cancelan diciéndolo, como siempre.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    let mensaje = prueba.app.mensaje.as_ref().expect("mensaje").texto.clone();
    assert!(mensaje.contains("la clave no se ha rotado"), "{mensaje}");
}

// -------------------------------------------------- Esc sin llavero que desbloquear

#[test]
fn esc_con_el_llavero_sin_crear_no_dice_que_esta_bloqueado() {
    use magi::app::llavero::SIN_LLAVERO_NO_GUARDA;
    for (continuacion, texto) in [
        (Continuacion::RotarClave, "la clave no se ha rotado"),
        (
            Continuacion::QuitarEquipo("00000000-0000-4000-8000-000000000000".to_string()),
            "no se ha quitado el equipo",
        ),
        (Continuacion::Desactivar, "la sincronización sigue activa"),
        (Continuacion::MostrarClave, ""),
    ] {
        let mut prueba = Prueba::nueva();
        assert_eq!(prueba.fichero.estado(), EstadoFichero::Inexistente);
        let nombre = continuacion.nombre();
        prueba.app.pedir_llavero(Some(continuacion));
        assert_eq!(prueba.modo(), Some(ModoLlavero::Crear), "{nombre}");
        prueba.tecla(KeyCode::Esc);
        assert!(prueba.app.dialogo.is_none());
        let mensaje = prueba.app.mensaje.as_ref().expect("mensaje").texto.clone();
        assert!(
            mensaje.starts_with(SIN_LLAVERO_NO_GUARDA) && mensaje.contains(texto),
            "{nombre}: {mensaje}"
        );
        assert!(!mensaje.contains("bloqueado"), "{nombre}: {mensaje}");
        assert!(!prueba.ruta_llavero().exists(), "{nombre}: no se crea nada");
    }
}

#[test]
fn con_el_llavero_danado_esc_o_apartarlo_tampoco_dicen_que_esta_bloqueado() {
    use magi::app::llavero::SIN_LLAVERO_NO_GUARDA;
    for tecla in [KeyCode::Esc, KeyCode::Char('a')] {
        let mut prueba = Prueba::nueva();
        dejar_danado(&prueba);
        prueba.app.pedir_llavero(Some(Continuacion::RotarClave));
        assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));
        prueba.tecla(tecla);
        let mensaje = prueba.app.mensaje.as_ref().expect("mensaje").texto.clone();
        assert!(
            mensaje.starts_with(SIN_LLAVERO_NO_GUARDA),
            "{tecla:?}: {mensaje}"
        );
        assert!(
            mensaje.contains("la clave no se ha rotado"),
            "{tecla:?}: {mensaje}"
        );
    }
}

// -------------------------------------------------- un enlace roto no es «no existe»

#[test]
fn con_un_enlace_roto_en_lugar_del_llavero_se_ofrece_apartarlo_y_no_crear_en_bucle() {
    let mut prueba = Prueba::nueva();
    std::os::unix::fs::symlink(
        prueba.entorno.rutas.datos.join("no-existe-nada"),
        prueba.ruta_llavero(),
    )
    .unwrap();
    assert!(matches!(prueba.fichero.estado(), EstadoFichero::Danado(_)));
    assert!(matches!(
        LlaveroCliente::estado(&*prueba.fichero),
        EstadoLlavero::Danado(_)
    ));
    // Algo que necesita escribir: antes salía CREAR, que daba «otra ventana
    // ha creado el llavero», luego «ya no existe» y vuelta a CREAR.
    prueba.app.pedir_llavero(Some(Continuacion::RotarClave));
    prueba.paso();
    assert_eq!(prueba.modo(), Some(ModoLlavero::Danado));
    assert!(prueba.texto().contains("enlace roto"), "{}", prueba.texto());
    prueba.tecla(KeyCode::Char('a'));
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(prueba.fichero.estado(), EstadoFichero::Inexistente);
    assert_eq!(copias_danadas(&prueba).len(), 1);
    // Y ya se puede crear.
    prueba.app.pedir_llavero(Some(Continuacion::MostrarClave));
    assert_eq!(prueba.modo(), Some(ModoLlavero::Crear));
}
