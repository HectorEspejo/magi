//! Pruebas de la interfaz de las sesiones persistentes: la pestaña (Fase 16).
//!
//! Con `App::de_prueba` (sin hilos, red ni servidor), el bucle real sobre un
//! `TestBackend` y los mensajes del servidor inyectados a mano. Cubren la marca
//! `◆` y la barra de la pestaña persistente, la pestaña soltada (recuadro, `↵`,
//! `c` y `x`), el aviso de atención de la barra global desde cualquier vista, el
//! historial de tmux, `prefijo N`, un host marcado de punta a punta, los
//! errores con instrucción de la apertura, que una reconexión nunca anota
//! «enganchada» y la ayuda. Sus instantáneas, a 40×12, 80×24 y 200×60 (T45),
//! llevan el prefijo del binario y se actualizan solo con el filtro del nombre
//! de la prueba (`cargo test --test sesiones_persistentes_ui <nombre>` con
//! `INSTA_UPDATE=always`).

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::sesion::DestinoPersistente;
use magi::app::{AccionDialogo, App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::modelo::{DatosHost, Origen};
use magi::protocolo::{
    CausaAperturaTmux, EstadoFicha, EstadoSesionRemota, FichaVista, HostVisto, InfoAviso,
    InfoReconexion, InfoSesion, ListaPersistentes, MensajeCliente, MensajeServidor, ResultadoTmux,
    ServidorTmux, SesionVista, Soltada, TmuxPestana, VersionTmux,
};
use magi::registro::FiltroRegistro;
use magi::tema::Tema;
use magi::ui::Vista;

// ------------------------------------------------------------------ arnés

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magipers")
        .tempdir()
        .expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    Entorno {
        rutas: Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        },
        config: Config::default(),
        _temporal: temporal,
    }
}

struct Prueba {
    app: App,
    terminal: Terminal<TestBackend>,
    enviados: tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
    _entorno: Entorno,
}

impl Prueba {
    fn nueva(hosts: &[&str]) -> Self {
        Self::con_inventario(|almacen| {
            for nombre in hosts {
                almacen
                    .crear_host(
                        &DatosHost {
                            nombre: (*nombre).to_string(),
                            direccion: "192.0.2.10".to_string(),
                            ..DatosHost::default()
                        },
                        Origen::Manual,
                    )
                    .expect("host de prueba");
            }
        })
    }

    /// App de prueba con el inventario que prepare la prueba.
    fn con_inventario(tocar: impl FnOnce(&magi::almacen::Almacen)) -> Self {
        Self::con_tema(Tema::respaldo(), tocar)
    }

    fn con_tema(tema: Tema, tocar: impl FnOnce(&magi::almacen::Almacen)) -> Self {
        let entorno = entorno();
        {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
            tocar(&almacen);
        }
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (app, enviados) =
            App::de_prueba(entorno.rutas.clone(), entorno.config.clone(), tema, almacen)
                .expect("app de prueba");
        let terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
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
        prueba
    }

    fn host_id(&self, nombre: &str) -> i64 {
        self.app
            .hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .map(|host| host.id)
            .unwrap_or_else(|| panic!("host {nombre} no sembrado"))
    }

    fn tecla(&mut self, codigo: KeyCode) {
        self.tecla_con(codigo, KeyModifiers::NONE);
    }

    fn tecla_con(&mut self, codigo: KeyCode, modificadores: KeyModifiers) {
        self.app
            .procesar(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
        self.paso();
    }

    /// El prefijo (`Ctrl+]`) y la tecla siguiente.
    fn prefijo(&mut self, codigo: KeyCode) {
        self.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
        self.tecla(codigo);
    }

    fn paso(&mut self) {
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    /// Obliga a repintar (una tecla que no hace nada marca la pantalla como
    /// sucia): hace falta tras tocar el estado por la puerta de atrás.
    fn repintar(&mut self) {
        self.tecla(KeyCode::Null);
    }

    /// Quita el mensaje de la barra y repinta.
    fn sin_mensaje(&mut self) {
        self.app.mensaje = None;
        self.repintar();
    }

    fn servidor(&mut self, mensaje: MensajeServidor) {
        let es_bienvenida = matches!(mensaje, MensajeServidor::Bienvenida { .. });
        self.app.procesar(Evento::Servidor(mensaje));
        if es_bienvenida {
            // La ventana lleva conectada un buen rato: lo que la lista de
            // pruebas dice haber comprobado hace segundos es posterior a su
            // conexión (D368). Las pruebas de la conexión reciente envían la
            // `Bienvenida` por `Evento::Servidor` directamente.
            self.app.persistentes.conectada_en = Some(chrono::Utc::now().timestamp() - 3600);
        }
        self.paso();
    }

    fn evento(&mut self, evento: Evento) {
        self.app.procesar(evento);
        self.paso();
    }

    fn difundir(&mut self, lista: Vec<InfoSesion>) {
        self.servidor(MensajeServidor::Sesiones { lista });
    }

    fn persistentes(&mut self, lista: ListaPersistentes) {
        self.servidor(MensajeServidor::SesionesPersistentes { lista });
    }

    fn drenar(&mut self) -> Vec<MensajeCliente> {
        let mut mensajes = Vec::new();
        while let Ok(mensaje) = self.enviados.try_recv() {
            mensajes.push(mensaje);
        }
        mensajes
    }

    /// La primera lista del servidor, con las pestañas ya abiertas por el
    /// usuario de esta ventana: la última pasa a ser la visible.
    fn entrar(&mut self, sesiones: Vec<InfoSesion>) {
        for info in &sesiones {
            self.app.aperturas_pendientes.insert(info.host_id);
        }
        self.servidor(bienvenida(Vec::new()));
        self.difundir(sesiones);
        assert_eq!(self.app.vista, Vista::Sesion);
        self.sin_mensaje();
    }

    fn volcar(&mut self, sesion_id: u32, texto: &str) {
        self.app
            .pantallas
            .volcar(sesion_id, 20, 80, texto.as_bytes());
        self.evento(Evento::Pantallas(vec![sesion_id]));
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

    /// La última fila no vacía: la barra global.
    fn barra_global(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let y = buffer.area.height - 1;
        let mut linea = String::new();
        for x in 0..buffer.area.width {
            linea.push_str(buffer[(x, y)].symbol());
        }
        linea.trim_end().to_string()
    }

    fn redimensionar(&mut self, cols: u16, filas: u16) {
        self.terminal.backend_mut().resize(cols, filas);
        self.app
            .procesar_en(Evento::Redimension(cols, filas), Instant::now());
        self.app
            .paso(
                &mut self.terminal,
                Instant::now() + std::time::Duration::from_millis(60),
            )
            .expect("paso");
    }

    fn instantanea(&self, nombre: &str) {
        let texto = self.texto();
        let mut ajustes = insta::Settings::clone_current();
        // Este módulo vive en `tests/sesiones_persistentes_ui/`: las
        // instantáneas se quedan con las demás, en `tests/snapshots/`.
        ajustes.set_snapshot_path("../snapshots");
        ajustes.set_prepend_module_to_snapshot(false);
        ajustes.set_omit_expression(true);
        ajustes.add_filter(r"hace \d+ s", "hace NN s");
        ajustes.add_filter(r"hace \d+ m", "hace NN m");
        ajustes.add_filter(r"en \d+ s", "en NN s");
        ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
    }

    /// Las tres instantáneas obligatorias (T45) con el nombre dado.
    fn instantaneas(&mut self, nombre: &str) {
        for (cols, filas) in [(40, 12), (80, 24), (200, 60)] {
            self.redimensionar(cols, filas);
            self.instantanea(&format!(
                "sesiones_persistentes_ui__{nombre}_{cols}x{filas}"
            ));
        }
        self.redimensionar(80, 24);
    }

    fn registro(&self) -> Vec<magi::modelo::EntradaRegistro> {
        self.app
            .almacen
            .listar_registro(&FiltroRegistro::default(), 200, 0)
            .expect("registro")
    }
}

fn bienvenida(sesiones: Vec<InfoSesion>) -> MensajeServidor {
    MensajeServidor::Bienvenida {
        version: magi::protocolo::VERSION_PROTOCOLO,
        version_app: "1.5.0".to_string(),
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

/// Una pestaña normal de un host.
fn sesion_de(id: u32, host_id: i64, host: &str) -> InfoSesion {
    InfoSesion {
        id,
        nombre: host.to_string(),
        host_id,
        host_nombre: host.to_string(),
        estado: EstadoSesionRemota::Abierta,
        motivo: None,
        identidad: "clave".to_string(),
        abierta_en: chrono::Utc::now().timestamp() - 7380,
        ventanas: 1,
        actividad_no_vista: false,
        reconexion: None,
        titulo: None,
        aviso: None,
        tmux: None,
        escritura_varios: false,
    }
}

fn tmux_propio(nombre: &str) -> TmuxPestana {
    TmuxPestana {
        creada: None,
        servidor: ServidorTmux::Magi,
        nombre: nombre.to_string(),
        version: Some(VersionTmux::nueva(3, 5)),
        soltada: None,
    }
}

fn tmux_habitual(nombre: &str) -> TmuxPestana {
    TmuxPestana {
        creada: None,
        servidor: ServidorTmux::Habitual,
        nombre: nombre.to_string(),
        version: None,
        soltada: None,
    }
}

/// Una pestaña persistente (`tmux -L magi`) de un host.
fn persistente(id: u32, host_id: i64, host: &str, nombre: &str) -> InfoSesion {
    InfoSesion {
        nombre: nombre.to_string(),
        tmux: Some(tmux_propio(nombre)),
        ..sesion_de(id, host_id, host)
    }
}

/// Una pestaña persistente que acaba de crear su sesión en el host (`creada`).
fn persistente_creada(id: u32, host_id: i64, host: &str, nombre: &str) -> InfoSesion {
    let mut info = persistente(id, host_id, host, nombre);
    if let Some(tmux) = info.tmux.as_mut() {
        tmux.creada = Some(true);
    }
    info
}

/// La misma pestaña, soltada por otro equipo (así la difunde el servidor: caída
/// y con la marca en `tmux.soltada`).
fn soltada_por(info: InfoSesion, equipo: Option<&str>, desde: i64) -> InfoSesion {
    let mut tmux = info.tmux.clone().expect("pestaña con tmux");
    tmux.soltada = Some(Soltada {
        por_equipo: equipo.map(str::to_string),
        desde,
    });
    InfoSesion {
        estado: EstadoSesionRemota::Caida,
        motivo: Some(format!(
            "soltada: la usa {}",
            equipo.unwrap_or("otro equipo")
        )),
        tmux: Some(tmux),
        ..info
    }
}

fn pestana(prueba: &Prueba, sesion_id: u32) -> &magi::app::PestanaUI {
    prueba
        .app
        .pestanas
        .iter()
        .find(|pestana| pestana.sesion_id == sesion_id)
        .expect("pestaña")
}

fn sesion_vista(nombre: &str, campana: bool) -> SesionVista {
    SesionVista {
        servidor: ServidorTmux::Magi,
        nombre: nombre.to_string(),
        id: None,
        clientes: 0,
        equipo: None,
        actividad: 1_700_000_000,
        creada: 1_699_990_000,
        campana,
        ventanas: 1,
        titulo: None,
    }
}

/// Una ficha con su sesión viva; `campana` es la atención (D335).
fn ficha_vista(host_id: i64, nombre: &str, campana: bool) -> FichaVista {
    FichaVista {
        uuid: format!("00000000-0000-4000-8000-{host_id:012}"),
        host_id,
        nombre: nombre.to_string(),
        estado: EstadoFicha::VivaLibre,
        sesion: Some(sesion_vista(nombre, campana)),
        comprobada_en: chrono::Utc::now().timestamp(),
    }
}

fn lista_con(fichas: Vec<FichaVista>) -> ListaPersistentes {
    ListaPersistentes {
        fichas,
        hosts: Vec::new(),
    }
}

/// Un host comprobado con sesiones sin ficha y del tmux habitual.
fn host_visto(host_id: i64, sin_ficha: Vec<SesionVista>, habitual: Vec<SesionVista>) -> HostVisto {
    HostVisto {
        host_id,
        comprobada_en: chrono::Utc::now().timestamp(),
        ahora: 1_700_000_000,
        tmux: Some(VersionTmux::nueva(3, 5)),
        error: None,
        sin_ficha,
        habitual,
    }
}

// ------------------------------------------------- marca y barra de pestaña

#[test]
fn la_pestana_persistente_lleva_la_marca_y_su_barra_dice_nombre_y_version() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    prueba.entrar(vec![
        persistente(1, dev, "dev-01", "refactor"),
        sesion_de(2, db, "db-01"),
    ]);
    // La visible es la última abierta (db-01, normal): se pasa a la primera.
    prueba.prefijo(KeyCode::Char('1'));
    assert_eq!(prueba.app.pestana_activa, Some(0));
    let texto = prueba.texto();
    // En la barra de pestañas la marca va delante del nombre (la activa lleva
    // número y nombre en la forma compacta) y la normal lleva el punto de
    // siempre.
    assert!(texto.contains("▸◆ 1 refactor"), "{texto}");
    assert!(texto.contains("● 2"), "{texto}");
    // La barra de estado de la pestaña.
    assert!(
        texto.contains("persistente · refactor · tmux 3.5"),
        "{texto}"
    );
    assert!(!texto.contains("soltada"), "{texto}");
    // La cabecera del panel también lleva la marca.
    assert!(texto.contains("▸◆ refactor"), "{texto}");
}

#[test]
fn una_pestana_del_tmux_habitual_conserva_lo_de_la_fase_15() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![InfoSesion {
        tmux: Some(tmux_habitual("trabajo")),
        ..sesion_de(1, dev, "dev-01")
    }]);
    let texto = prueba.texto();
    assert!(texto.contains("tmux trabajo"), "{texto}");
    assert!(!texto.contains('◆'), "{texto}");
    assert!(!texto.contains("persistente"), "{texto}");
}

#[test]
fn en_ascii_la_marca_es_una_p() {
    let mut tema = Tema::respaldo();
    tema.ascii = true;
    tema.glifos = magi::tema::Glifos::ascii();
    let mut prueba = Prueba::con_tema(tema, |almacen| {
        almacen
            .crear_host(
                &DatosHost {
                    nombre: "dev-01".to_string(),
                    direccion: "192.0.2.10".to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("host");
    });
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    let texto = prueba.texto();
    assert!(texto.contains("P refactor"), "{texto}");
    assert!(!texto.contains('◆'), "{texto}");
    assert!(
        texto.contains("persistente - refactor - tmux 3.5"),
        "{texto}"
    );
}

#[test]
fn la_marca_va_tambien_en_la_rejilla_de_paneles() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    prueba.entrar(vec![
        persistente(1, dev, "dev-01", "refactor"),
        sesion_de(2, db, "db-01"),
    ]);
    prueba.prefijo(KeyCode::Char(' '));
    prueba.redimensionar(120, 30);
    let texto = prueba.texto();
    // Una cabecera de panel con la marca y la del otro panel sin ella.
    let con_marca = texto
        .lines()
        .any(|linea| linea.contains("◆ refactor") && !linea.contains('│'));
    assert!(con_marca, "{texto}");
    assert!(texto.lines().any(|linea| linea.contains("db-01")));
}

#[test]
fn una_pestana_persistente_caida_enseña_el_error_y_la_marca() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![InfoSesion {
        estado: EstadoSesionRemota::Caida,
        motivo: Some("conexión cerrada por el remoto".to_string()),
        ..persistente(1, dev, "dev-01", "refactor")
    }]);
    let texto = prueba.texto();
    // No es una soltada: el aviso es el de siempre y el estado, el de un error.
    assert!(texto.contains("SESIÓN CAÍDA"), "{texto}");
    assert!(!texto.contains("SOLTADA"), "{texto}");
    assert!(texto.contains("✕◆ refactor"), "{texto}");
}

#[test]
fn instantaneas_de_la_pestana_persistente() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    let mut info = persistente(1, dev, "dev-01", "refactor");
    info.titulo = Some("Claude Code".to_string());
    prueba.entrar(vec![info, sesion_de(2, db, "db-01")]);
    prueba.prefijo(KeyCode::Char('1'));
    prueba.volcar(1, "usuario@dev-01:~/proyectos/api$ claude\r\n");
    prueba.instantaneas("pestana_persistente");
}

// -------------------------------------------------------------- soltada

const DESDE_FIJO: i64 = 1_700_000_000;

#[test]
fn una_pestana_soltada_enseña_el_recuadro_la_barra_y_la_ultima_pantalla() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    prueba.entrar(vec![
        persistente(1, dev, "dev-01", "refactor"),
        sesion_de(2, db, "db-01"),
    ]);
    prueba.prefijo(KeyCode::Char('1'));
    prueba.volcar(1, "ultima linea de la sesion\r\n");
    prueba.difundir(vec![
        soltada_por(
            persistente(1, dev, "dev-01", "refactor"),
            Some("portátil"),
            DESDE_FIJO,
        ),
        sesion_de(2, db, "db-01"),
    ]);
    let texto = prueba.texto();
    assert!(texto.contains("SOLTADA"), "{texto}");
    assert!(
        texto.contains("Ahora la usa portátil (desde el 14/11 22:13)."),
        "{texto}"
    );
    assert!(texto.contains("La sesión sigue viva en dev-01."), "{texto}");
    assert!(
        texto.contains("↵ volver (la suelta allí) · c compartir · x cerrar"),
        "{texto}"
    );
    // La barra de estado de la pestaña, la marca (no el aspa de un error) y
    // ninguna alarma de «caída».
    assert!(
        texto.contains("persistente · refactor · tmux 3.5 · soltada"),
        "{texto}"
    );
    assert!(texto.contains("▸◆ refactor"), "{texto}");
    assert!(!texto.contains("SESIÓN CAÍDA"), "{texto}");
    assert!(!texto.contains("soltada: la usa"), "{texto}");
    // La última pantalla sigue ahí, bajo el recuadro.
    assert!(texto.contains("ultima linea de la sesion"), "{texto}");
}

#[test]
fn la_soltada_sin_equipo_conocido_y_de_hoy_lo_dice_con_la_hora() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        None,
        chrono::Utc::now().timestamp(),
    )]);
    let texto = prueba.texto();
    assert!(
        texto.contains("Ahora la usa otro equipo (desde las "),
        "{texto}"
    );
}

#[test]
fn en_la_soltada_enter_c_y_x_mandan_lo_correcto_y_lo_demas_no_va_al_remoto() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        Some("portátil"),
        DESDE_FIJO,
    )]);
    prueba.drenar();

    // `↵` vuelve soltando a los demás.
    prueba.tecla(KeyCode::Enter);
    let enviados = prueba.drenar();
    assert_eq!(
        enviados,
        vec![MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: false
        }],
        "{enviados:?}"
    );
    // `c` vuelve compartiendo.
    prueba.tecla(KeyCode::Char('c'));
    assert_eq!(
        prueba.drenar(),
        vec![MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: true
        }]
    );
    // Cualquier otra tecla no va a ninguna parte: no hay canal.
    for tecla in [KeyCode::Char('a'), KeyCode::Char(' '), KeyCode::Down] {
        prueba.tecla(tecla);
    }
    assert!(prueba.drenar().is_empty());
    // `x` cierra con la confirmación de la Fase 15, que nombra la sesión, avisa
    // de quién la usa y deja terminarla también (con el servidor propio).
    prueba.tecla(KeyCode::Char('x'));
    let Some(Dialogo::Elegir { lineas, .. }) = &prueba.app.dialogo else {
        panic!("sin el diálogo de cerrar con tmux");
    };
    assert!(lineas.iter().any(|linea| linea.contains("«refactor»")));
    assert!(
        lineas.iter().any(|linea| linea.contains("portátil")),
        "{lineas:?}"
    );
    prueba.tecla(KeyCode::Char('T'));
    let enviados = prueba.drenar();
    assert!(
        enviados.iter().any(|mensaje| matches!(
            mensaje,
            MensajeCliente::TerminarTmux { servidor: ServidorTmux::Magi, nombre, host_id, .. }
                if nombre == "refactor" && *host_id == dev
        )),
        "{enviados:?}"
    );
    assert!(enviados
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeCliente::Cerrar { sesion_id: 1 })));
}

/// Revisión adversarial: en una soltada, Ctrl+C o Mayús+Intro (el salto de línea
/// de Claude Code) no pueden volver a engancharse, que le quitaría la sesión al
/// otro equipo: solo valen `↵`, `c` y `x` sin modificadores.
#[test]
fn en_la_soltada_las_teclas_con_modificadores_no_vuelven_ni_cierran() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        Some("portátil"),
        DESDE_FIJO,
    )]);
    prueba.drenar();
    for (codigo, modificadores) in [
        (KeyCode::Char('c'), KeyModifiers::CONTROL),
        (KeyCode::Char('x'), KeyModifiers::CONTROL),
        (KeyCode::Char('c'), KeyModifiers::ALT),
        (KeyCode::Enter, KeyModifiers::SHIFT),
        (KeyCode::Enter, KeyModifiers::CONTROL),
        (KeyCode::Enter, KeyModifiers::ALT),
    ] {
        prueba.tecla_con(codigo, modificadores);
        assert!(
            prueba.drenar().is_empty(),
            "{codigo:?} con {modificadores:?} mandó algo"
        );
        assert!(
            prueba.app.dialogo.is_none(),
            "{codigo:?} con {modificadores:?} abrió un diálogo"
        );
    }
    // Sin modificadores siguen valiendo.
    prueba.tecla(KeyCode::Enter);
    assert_eq!(
        prueba.drenar(),
        vec![MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: false
        }]
    );
}

/// Una pestaña persistente que otro equipo se llevó, con la lista de pestañas
/// ya difundida.
fn con_una_soltada(prueba: &mut Prueba, dev: i64) {
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        Some("portátil"),
        DESDE_FIJO,
    )]);
    prueba.drenar();
}

/// El diálogo que pregunta antes de quitarle la sesión al otro equipo: lo que
/// dice y que con `n` no manda nada.
fn confirma_volver_y_cancela_o_acepta(prueba: &mut Prueba, aceptar: bool) -> Vec<MensajeCliente> {
    let Some(Dialogo::Confirmar {
        titulo,
        lineas,
        accion,
        ..
    }) = &prueba.app.dialogo
    else {
        panic!("sin la confirmación de volver: {:?}", prueba.app.mensaje);
    };
    assert_eq!(titulo, "VOLVER A LA SESIÓN");
    let texto = lineas.join(" ");
    assert!(texto.contains("«refactor»"), "{texto}");
    assert!(texto.contains("dev-01"), "{texto}");
    assert!(texto.contains("la usa ahora portátil"), "{texto}");
    assert!(texto.contains("se la quita"), "{texto}");
    assert!(
        matches!(accion, AccionDialogo::VolverASesion { sesion_id: 1 }),
        "la acción no es volver a la sesión 1"
    );
    assert!(prueba.drenar().is_empty(), "preguntar no manda nada");
    prueba.tecla(if aceptar {
        KeyCode::Enter
    } else {
        KeyCode::Char('n')
    });
    assert!(prueba.app.dialogo.is_none());
    prueba.drenar()
}

#[test]
fn el_prefijo_r_en_la_soltada_pide_la_confirmacion_antes_de_volver() {
    // R164: el prefijo r también le quita la sesión al otro equipo.
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    con_una_soltada(&mut prueba, dev);
    prueba.prefijo(KeyCode::Char('r'));
    assert!(
        confirma_volver_y_cancela_o_acepta(&mut prueba, false).is_empty(),
        "con n no se manda nada"
    );
    prueba.prefijo(KeyCode::Char('r'));
    assert_eq!(
        confirma_volver_y_cancela_o_acepta(&mut prueba, true),
        vec![MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: false
        }]
    );
}

#[test]
fn r_en_la_lista_de_pestanas_sobre_una_soltada_pide_la_confirmacion_antes_de_volver() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    con_una_soltada(&mut prueba, dev);
    prueba.prefijo(KeyCode::Char('l'));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    // Entrar en la vista consulta al servidor: eso no es lo que se prueba.
    prueba.drenar();
    // El foco está en «Pestañas» (la lista de «Persistentes» está vacía).
    prueba.tecla(KeyCode::Char('r'));
    assert!(
        confirma_volver_y_cancela_o_acepta(&mut prueba, false).is_empty(),
        "con n no se manda nada"
    );
    prueba.tecla(KeyCode::Char('r'));
    assert_eq!(
        confirma_volver_y_cancela_o_acepta(&mut prueba, true),
        vec![MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: false
        }]
    );
}

#[test]
fn reconectar_de_la_paleta_sobre_una_soltada_pide_la_confirmacion_antes_de_volver() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    con_una_soltada(&mut prueba, dev);
    // La paleta no se abre desde la vista Sesión (allí las teclas van al
    // remoto): se sale a la lista de sesiones.
    prueba.prefijo(KeyCode::Char('l'));
    prueba.drenar();
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(prueba.app.paleta.is_some(), "la paleta no se abrió");
    for letra in "reconectar".chars() {
        prueba.tecla(KeyCode::Char(letra));
    }
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.paleta.is_none());
    assert_eq!(
        confirma_volver_y_cancela_o_acepta(&mut prueba, true),
        vec![MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: false
        }]
    );
}

#[test]
fn instantaneas_de_la_confirmacion_de_volver_a_la_soltada() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    con_una_soltada(&mut prueba, dev);
    prueba.prefijo(KeyCode::Char('r'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    prueba.instantaneas("volver_a_la_soltada");
}

#[test]
fn una_pestana_caida_sin_soltar_se_reconecta_sin_preguntar() {
    // Lo que ya funcionaba no cambia: la confirmación es solo para la soltada.
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![InfoSesion {
        estado: EstadoSesionRemota::Caida,
        ..persistente(1, dev, "dev-01", "refactor")
    }]);
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('r'));
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        prueba.drenar(),
        vec![MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: false
        }]
    );
}

#[test]
fn si_la_soltada_vuelve_mientras_se_pregunta_confirmar_no_manda_nada() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    con_una_soltada(&mut prueba, dev);
    prueba.prefijo(KeyCode::Char('r'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    // Mientras se pregunta, el servidor la vuelve a enganchar (otra ventana).
    prueba.difundir(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.drenar().is_empty());
    assert!(prueba
        .app
        .mensaje
        .as_ref()
        .is_some_and(|mensaje| mensaje.texto.contains("ya no está soltada")));
}

#[test]
fn la_pestana_soltada_vuelve_a_normal_cuando_el_servidor_quita_la_marca() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        Some("portátil"),
        DESDE_FIJO,
    )]);
    assert!(prueba.texto().contains("SOLTADA"));
    assert!(pestana(&prueba, 1).soltada().is_some());
    // El servidor la vuelve a enganchar: abierta y sin marca.
    prueba.difundir(vec![persistente(1, dev, "dev-01", "refactor")]);
    let texto = prueba.texto();
    assert!(!texto.contains("SOLTADA"), "{texto}");
    assert!(pestana(&prueba, 1).soltada().is_none());
    assert!(
        texto.contains("persistente · refactor · tmux 3.5"),
        "{texto}"
    );
    // Con la pestaña abierta, las teclas vuelven a ir al remoto.
    prueba.drenar();
    prueba.tecla(KeyCode::Char('c'));
    assert!(prueba
        .drenar()
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeCliente::Teclas { sesion_id: 1, .. })));
}

#[test]
fn engancharse_desde_la_lista_a_una_soltada_vuelve_con_el_modo_pedido() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        Some("portátil"),
        DESDE_FIJO,
    )]);
    // Se sale a la lista de sesiones y se pide engancharse (lo que hace `↵`
    // en el bloque «Persistentes»): no abre otra pestaña, vuelve a la suya.
    prueba.prefijo(KeyCode::Char('l'));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    prueba.drenar();
    let abierta = prueba.app.abrir_sesion_persistente(DestinoPersistente {
        host_id: dev,
        servidor: ServidorTmux::Magi,
        nombre: "refactor".to_string(),
        crear: None,
        compartir: true,
    });
    assert!(abierta);
    assert_eq!(prueba.app.vista, Vista::Sesion);
    let enviados = prueba.drenar();
    assert!(
        enviados.contains(&MensajeCliente::Reconectar {
            sesion_id: 1,
            compartir: true
        }),
        "{enviados:?}"
    );
    assert!(
        !enviados
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeCliente::AbrirSesion { .. })),
        "abrió una pestaña más: {enviados:?}"
    );
}

#[test]
fn abrir_una_sesion_que_no_tiene_pestana_manda_abrir_sesion_con_su_tmux() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.drenar();
    let hecho = prueba.app.abrir_sesion_persistente(DestinoPersistente {
        host_id: dev,
        servidor: ServidorTmux::Magi,
        nombre: "docs".to_string(),
        crear: None,
        compartir: false,
    });
    assert!(hecho);
    let enviados = prueba.drenar();
    assert!(
        enviados.iter().any(|mensaje| matches!(
            mensaje,
            MensajeCliente::AbrirSesion { host_id, tmux: Some(tmux), .. }
                if *host_id == dev
                    && tmux.nombre == "docs"
                    && tmux.servidor == ServidorTmux::Magi
                    && !tmux.compartir
        )),
        "{enviados:?}"
    );
    assert!(prueba.app.aperturas_pendientes.contains(&dev));
    // Con la apertura en curso, otra al mismo host no se manda ni deja nada.
    let otra = prueba.app.abrir_sesion_persistente(DestinoPersistente {
        host_id: dev,
        servidor: ServidorTmux::Magi,
        nombre: "otra".to_string(),
        crear: None,
        compartir: false,
    });
    assert!(!otra);
    assert!(prueba.drenar().is_empty());
}

#[test]
fn instantaneas_de_la_pestana_soltada() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    prueba.entrar(vec![
        persistente(1, dev, "dev-01", "refactor"),
        sesion_de(2, db, "db-01"),
    ]);
    prueba.prefijo(KeyCode::Char('1'));
    prueba.volcar(1, "usuario@dev-01:~/proyectos/api$ claude\r\n");
    prueba.difundir(vec![
        soltada_por(
            persistente(1, dev, "dev-01", "refactor"),
            Some("portátil"),
            DESDE_FIJO,
        ),
        sesion_de(2, db, "db-01"),
    ]);
    prueba.instantaneas("pestana_soltada");
}

/// La pestaña soltada y la persistente caben en cualquier tamaño (el mínimo de
/// Sesión es 40×8): sin paniquear, con la barra de estado a la vista y, en el
/// recuadro, lo más importante por delante.
#[test]
fn la_soltada_y_la_persistente_se_pintan_en_todos_los_tamanos_desde_el_minimo() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    prueba.entrar(vec![
        persistente(1, dev, "dev-01", "refactor"),
        sesion_de(2, db, "db-01"),
    ]);
    prueba.prefijo(KeyCode::Char('1'));
    prueba.persistentes(lista_con(vec![ficha_vista(dev, "agente-api", true)]));
    for soltada in [false, true] {
        let base = persistente(1, dev, "dev-01", "refactor");
        let info = if soltada {
            soltada_por(base, Some("portátil"), DESDE_FIJO)
        } else {
            base
        };
        prueba.difundir(vec![info, sesion_de(2, db, "db-01")]);
        for cols in (40..=120).step_by(8) {
            for filas in (8..=30).step_by(4) {
                prueba.redimensionar(cols, filas);
                let texto = prueba.texto();
                assert!(texto.contains('◆'), "{cols}x{filas}: {texto}");
                if soltada {
                    // El recuadro dice quién la usa en cuanto hay una fila.
                    assert!(
                        texto.contains("Ahora la usa portátil")
                            || texto.contains("La usa portátil"),
                        "{cols}x{filas}: {texto}"
                    );
                    // La barra de estado la marca como soltada, con el ancho que sea.
                    assert!(texto.contains("soltada"), "{cols}x{filas}: {texto}");
                }
            }
        }
    }
}

// ----------------------------------------------------- aviso de atención

const AVISO_UNA: &str = "◆ agente-api pide atención · web-01";

/// Las vistas desde las que el aviso tiene que verse.
fn ir_a(prueba: &mut Prueba, vista: Vista) {
    match vista {
        Vista::Flota => prueba.tecla(KeyCode::F(1)),
        Vista::Hosts => prueba.tecla(KeyCode::F(2)),
        Vista::Sesiones => {
            prueba.tecla(KeyCode::F(3));
            if prueba.app.vista == Vista::Sesion {
                prueba.prefijo(KeyCode::Char('l'));
            }
        }
        Vista::Registro => prueba.tecla(KeyCode::F(7)),
        otra => panic!("sin atajo para {otra:?}"),
    }
    assert_eq!(prueba.app.vista, vista);
    prueba.sin_mensaje();
}

#[test]
fn el_aviso_de_atencion_sale_en_la_barra_de_cualquier_vista_y_se_quita() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let web = prueba.host_id("web-01");
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", false)]));
    for vista in [Vista::Flota, Vista::Hosts, Vista::Sesiones, Vista::Registro] {
        ir_a(&mut prueba, vista);
        assert!(
            !prueba.barra_global().contains("pide atención"),
            "{vista:?}: {}",
            prueba.barra_global()
        );
    }
    // La sesión libre toca la campana y el vigilante la comprueba: la lista
    // llega por `SesionesPersistentes` y el aviso sale, sin cambiar de vista.
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", true)]));
    for vista in [Vista::Flota, Vista::Hosts, Vista::Sesiones, Vista::Registro] {
        ir_a(&mut prueba, vista);
        let barra = prueba.barra_global();
        assert!(barra.contains(AVISO_UNA), "{vista:?}: {barra}");
    }
    // Y desde la vista Sesión con otra pestaña a la vista.
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![sesion_de(1, dev, "dev-01")]);
    assert!(prueba.barra_global().contains(AVISO_UNA));
    // Deja de pedir atención (alguien la miró): el aviso se quita.
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", false)]));
    assert!(
        !prueba.barra_global().contains("pide atención"),
        "{}",
        prueba.barra_global()
    );
}

/// `Bienvenida` de una ventana que se acaba de conectar: lo comprobado antes
/// de este momento no vale para avisar de atención (D368).
fn conectar_ahora(prueba: &mut Prueba) {
    prueba
        .app
        .procesar(Evento::Servidor(bienvenida(Vec::new())));
    prueba.paso();
}

#[test]
fn la_barra_no_avisa_con_lo_comprobado_antes_de_conectar_la_ventana() {
    // D368: la lista que llega detrás de la `Bienvenida` es la que el servidor
    // guardó al irse la última ventana, y puede traer una campana de hace
    // horas. No se avisa de ella; la pasada inmediata del servidor trae la
    // lista fresca y entonces sí.
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    conectar_ahora(&mut prueba);
    let hace_horas = chrono::Utc::now().timestamp() - 3 * 3600;
    let guardada = ListaPersistentes {
        fichas: vec![FichaVista {
            comprobada_en: hace_horas,
            ..ficha_vista(web, "agente-api", true)
        }],
        hosts: vec![HostVisto {
            comprobada_en: hace_horas,
            ..host_visto(web, vec![sesion_vista("portatil-3", true)], Vec::new())
        }],
    };
    prueba.persistentes(guardada);
    assert!(prueba.app.persistentes_con_atencion().is_empty());
    assert!(
        !prueba.barra_global().contains("pide atención"),
        "{}",
        prueba.barra_global()
    );

    // La lista fresca (comprobada ahora) sí avisa, también de la sin ficha.
    prueba.persistentes(ListaPersistentes {
        fichas: vec![ficha_vista(web, "agente-api", true)],
        hosts: vec![host_visto(
            web,
            vec![sesion_vista("portatil-3", true)],
            Vec::new(),
        )],
    });
    assert_eq!(prueba.app.persistentes_con_atencion().len(), 2);
    assert!(
        prueba.barra_global().contains("2 sesiones piden atención"),
        "{}",
        prueba.barra_global()
    );
}

#[test]
fn lo_comprobado_hasta_siete_segundos_antes_de_conectar_cuenta_como_fresco() {
    // Como en `magi sesiones`: el servidor no repite un host comprobado hace
    // menos de 5 s, así que esa comprobación es la de ahora; y 2 s más de
    // margen por los segundos enteros y lo que tarda en llegar la `Bienvenida`.
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    conectar_ahora(&mut prueba);
    prueba.app.persistentes.conectada_en = Some(10_000);
    for (comprobada, avisa) in [
        (10_000, true),
        (9_993, true),
        (9_992, false),
        (9_000, false),
        (0, false),
    ] {
        prueba.persistentes(ListaPersistentes {
            fichas: vec![FichaVista {
                comprobada_en: comprobada,
                ..ficha_vista(web, "agente-api", true)
            }],
            hosts: Vec::new(),
        });
        assert_eq!(
            !prueba.app.persistentes_con_atencion().is_empty(),
            avisa,
            "comprobada en {comprobada}"
        );
    }
}

#[test]
fn un_titulo_igual_al_nombre_del_host_no_se_enseña_en_la_lista_de_pestanas() {
    // tmux pone por defecto el nombre de la máquina como título (D370): no
    // cuenta nada del programa de dentro, y la columna dice «—». Uno de verdad
    // sí se ve.
    let mut prueba = Prueba::nueva(&["dev-01", "web-01"]);
    let (dev, web) = (prueba.host_id("dev-01"), prueba.host_id("web-01"));
    prueba.entrar(vec![
        InfoSesion {
            titulo: Some("dev-01".to_string()),
            ..sesion_de(1, dev, "dev-01")
        },
        InfoSesion {
            titulo: Some("Claude Code".to_string()),
            ..sesion_de(2, web, "web-01")
        },
    ]);
    prueba.redimensionar(140, 30);
    prueba.prefijo(KeyCode::Char('l'));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    let texto = prueba.texto();
    let fila = |nombre: &str| {
        texto
            .lines()
            .find(|linea| linea.contains(nombre) && linea.contains("SSH"))
            .unwrap_or_else(|| panic!("sin la fila de {nombre}: {texto}"))
            .to_string()
    };
    let de_dev = fila("dev-01");
    // El nombre de la pestaña es «dev-01» y es lo único con ese texto: el
    // título repetido no sale otra vez.
    assert_eq!(de_dev.matches("dev-01").count(), 2, "{de_dev}");
    assert!(de_dev.contains('—'), "{de_dev}");
    assert!(fila("web-01").contains("Claude Code"), "{}", fila("web-01"));
}

#[test]
fn el_registro_enseña_los_tipos_de_sesiones_persistentes_enteros_con_su_etiqueta_corta() {
    // Los cuatro tipos de la Fase 16 miden de 25 a 29 caracteres y la columna
    // TIPO del REGISTRO corta a 22: llevan una etiqueta corta (D370). El filtro
    // y el detalle siguen con el tipo real.
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    for tipo in [
        magi::registro::SESION_PERSISTENTE_CREADA,
        magi::registro::SESION_PERSISTENTE_BORRADA,
        magi::registro::SESION_PERSISTENTE_REABIERTA,
        magi::registro::SESION_PERSISTENTE_ENGANCHADA,
    ] {
        magi::registro::anotar(
            prueba.app.almacen.conexion(),
            tipo,
            Some(dev),
            None,
            "«dev-01» · refactor",
            magi::modelo::ResultadoRegistro::Ok,
        )
        .expect("anotar");
    }
    prueba.redimensionar(120, 30);
    ir_a(&mut prueba, Vista::Registro);
    let texto = prueba.texto();
    for etiqueta in [
        "persistente_creada",
        "persistente_borrada",
        "persistente_reabierta",
        "persistente_enganchada",
    ] {
        assert!(
            texto
                .lines()
                .any(|linea| linea.contains(etiqueta) && !linea.contains("sesion_persistente")),
            "falta la fila con «{etiqueta}»:\n{texto}"
        );
    }
    // Nada cortado a medias (`sesion_persistente_creada` en 22 caracteres
    // salía `sesion_persistente_cr…`).
    assert!(!texto.contains("sesion_persistente_c"), "{texto}");
    // El detalle de la fila elegida (la última anotada) dice el tipo real.
    assert!(texto.contains("sesion_persistente_enganchada"), "{texto}");
    // Y el filtro de texto sigue buscando por el tipo real.
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "sesion_persistente_borrada".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let filtrado = prueba.texto();
    assert!(filtrado.contains("persistente_borrada"), "{filtrado}");
    assert!(!filtrado.contains("persistente_creada"), "{filtrado}");
}

#[test]
fn el_aviso_con_varias_sesiones_dice_cuantas() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let (web, dev) = (prueba.host_id("web-01"), prueba.host_id("dev-01"));
    prueba.persistentes(ListaPersistentes {
        fichas: vec![
            ficha_vista(web, "agente-api", true),
            ficha_vista(dev, "docs", true),
            ficha_vista(dev, "pruebas", false),
        ],
        hosts: vec![host_visto(
            web,
            // Una sin ficha del tmux propio también cuenta; las del tmux
            // habitual, no.
            vec![sesion_vista("portatil-3", true)],
            vec![SesionVista {
                servidor: ServidorTmux::Habitual,
                ..sesion_vista("trabajo", true)
            }],
        )],
    });
    ir_a(&mut prueba, Vista::Hosts);
    assert!(
        prueba
            .barra_global()
            .contains("◆ 3 sesiones piden atención"),
        "{}",
        prueba.barra_global()
    );
    // Quedan dos, luego una: cambia el texto con la lista.
    prueba.persistentes(ListaPersistentes {
        fichas: vec![ficha_vista(dev, "docs", true)],
        hosts: Vec::new(),
    });
    assert!(
        prueba
            .barra_global()
            .contains("◆ docs pide atención · dev-01"),
        "{}",
        prueba.barra_global()
    );
}

#[test]
fn las_del_tmux_habitual_no_hacen_saltar_el_aviso() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    prueba.persistentes(ListaPersistentes {
        fichas: Vec::new(),
        hosts: vec![host_visto(
            web,
            Vec::new(),
            vec![SesionVista {
                servidor: ServidorTmux::Habitual,
                ..sesion_vista("trabajo", true)
            }],
        )],
    });
    ir_a(&mut prueba, Vista::Hosts);
    assert!(!prueba.barra_global().contains("atención"));
}

#[test]
fn el_aviso_no_sale_de_la_sesion_que_se_esta_mirando() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    prueba.entrar(vec![persistente(1, web, "web-01", "agente-api")]);
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", true)]));
    // Con la pestaña a la vista, el aviso es redundante.
    assert!(!prueba.barra_global().contains("pide atención"));
    // Al salir de ella, sí.
    prueba.prefijo(KeyCode::Char('l'));
    assert!(prueba.barra_global().contains(AVISO_UNA));
}

#[test]
fn en_estrecho_el_aviso_se_acorta_antes_que_tapar_los_atajos_importantes() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", true)]));
    ir_a(&mut prueba, Vista::Hosts);
    prueba.redimensionar(40, 12);
    let barra = prueba.barra_global();
    assert!(barra.contains('◆'), "{barra}");
    // Los atajos de prioridad 1 de Hosts siguen a la vista.
    assert!(barra.contains("↵ conectar"), "{barra}");
    assert!(barra.contains("/ filtrar"), "{barra}");
    assert!(barra.chars().count() <= 40, "{barra}");
    // A 80 columnas cabe la frase entera.
    prueba.redimensionar(80, 24);
    assert!(prueba.barra_global().contains(AVISO_UNA));
}

/// Revisión adversarial: el aviso se mide por columnas de pantalla, no por
/// caracteres. Con el nombre de un host en caracteres anchos, el aviso largo
/// empujaba «? más» (y parte de los atajos) fuera de la barra.
#[test]
fn el_aviso_con_un_host_de_caracteres_anchos_no_tapa_los_atajos() {
    let mut prueba = Prueba::nueva(&["ホストホストホストホスト", "dev-01"]);
    let ancho = prueba.host_id("ホストホストホストホスト");
    prueba.persistentes(lista_con(vec![ficha_vista(ancho, "agente-api", true)]));
    ir_a(&mut prueba, Vista::Hosts);
    for columnas in 40..=110 {
        prueba.redimensionar(columnas, 12);
        let barra = prueba.barra_global();
        assert!(barra.contains('◆'), "{columnas}: {barra}");
        assert!(
            barra.contains("? más") || barra.contains("? ayuda"),
            "{columnas} columnas: el aviso tapó la ayuda: {barra}"
        );
        assert!(barra.contains("↵ conectar"), "{columnas}: {barra}");
    }
}

#[test]
fn un_mensaje_de_la_barra_tiene_prioridad_sobre_el_aviso() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", true)]));
    ir_a(&mut prueba, Vista::Hosts);
    // Una lista del servidor repinta sin tocar el mensaje (una tecla lo quita).
    prueba.app.mensaje("host guardado", false);
    prueba.difundir(Vec::new());
    assert!(prueba.barra_global().contains("host guardado"));
    assert!(!prueba.barra_global().contains("pide atención"));
}

#[test]
fn en_ascii_el_aviso_lleva_p_en_vez_del_rombo() {
    let mut tema = Tema::respaldo();
    tema.ascii = true;
    tema.glifos = magi::tema::Glifos::ascii();
    let mut prueba = Prueba::con_tema(tema, |almacen| {
        almacen
            .crear_host(
                &DatosHost {
                    nombre: "web-01".to_string(),
                    direccion: "192.0.2.10".to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("host");
    });
    let web = prueba.host_id("web-01");
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", true)]));
    ir_a(&mut prueba, Vista::Hosts);
    let barra = prueba.barra_global();
    assert!(
        barra.contains("P agente-api pide atención - web-01"),
        "{barra}"
    );
    assert!(!barra.contains('◆'));
}

#[test]
fn instantaneas_del_aviso_en_hosts_y_en_sesion() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let (web, dev) = (prueba.host_id("web-01"), prueba.host_id("dev-01"));
    prueba.persistentes(lista_con(vec![ficha_vista(web, "agente-api", true)]));
    ir_a(&mut prueba, Vista::Hosts);
    prueba.instantaneas("aviso_atencion_hosts");
    // En la vista Sesión, con otra pestaña a la vista.
    prueba.entrar(vec![sesion_de(1, dev, "dev-01")]);
    prueba.instantaneas("aviso_atencion_sesion");
    // Varias.
    prueba.persistentes(lista_con(vec![
        ficha_vista(web, "agente-api", true),
        ficha_vista(dev, "docs", true),
    ]));
    prueba.instantaneas("aviso_atencion_varias_sesion");
}

// ----------------------------------------------------- historial de tmux

fn responder_historial(prueba: &mut Prueba, sesion_id: u32, lineas: &[&str]) {
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::CapturarHistorial { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin CapturarHistorial pendiente");
    let lineas: Vec<_> = lineas
        .iter()
        .map(|texto| magi_nucleo::terminal::historial::LineaHistorial {
            tramos: vec![magi_nucleo::terminal::tramos::Tramo {
                texto: (*texto).to_string(),
                primer_plano: magi_nucleo::terminal::tramos::Color::Predeterminado,
                fondo: magi_nucleo::terminal::tramos::Color::Predeterminado,
                negrita: false,
                cursiva: false,
                subrayado: false,
                inverso: false,
            }],
            envuelta: false,
        })
        .collect();
    let total = lineas.len() as u32;
    prueba.servidor(MensajeServidor::HistorialLote {
        peticion_id,
        sesion_id,
        lote: 0,
        ultimo: true,
        total,
        lineas,
    });
}

#[test]
fn el_prefijo_corchete_en_una_persistente_abre_el_historial_de_tmux_aun_con_pantalla_alternativa() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    // tmux trabaja en la pantalla alternativa del terminal de la pestaña.
    prueba.volcar(1, "\x1b[?1049h\x1b[2J\x1b[Hbarra de tmux\r\n");
    assert!(prueba.app.pantallas.pantalla_alternativa(1));
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('['));
    // Pide la captura (el servidor la resuelve con el historial de tmux).
    responder_historial(
        &mut prueba,
        1,
        &[
            "primera linea",
            "error de compilacion",
            "tercera",
            "otro error",
        ],
    );
    assert!(prueba.app.historial.is_some());
    // Buscar, seleccionar y copiar son los de la Fase 15.
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "error".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let estado = prueba.app.historial.as_ref().unwrap();
    assert_eq!(estado.busqueda.as_ref().unwrap().coincidencias.len(), 2);
    prueba.tecla(KeyCode::Char('v'));
    prueba.tecla(KeyCode::Char('y'));
    let mensaje = prueba.app.mensaje.as_ref().expect("sin mensaje de copia");
    assert!(mensaje.texto.contains("historial"), "{}", mensaje.texto);
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.historial.is_none());
}

#[test]
fn el_historial_de_tmux_se_guarda_solo_por_orden_del_usuario_y_sin_ir_a_registro() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.volcar(1, "\x1b[?1049h\x1b[2J\x1b[Hbarra de tmux\r\n");
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('['));
    responder_historial(&mut prueba, 1, &["secreto-de-la-sesion uno", "dos", "tres"]);
    // Hasta aquí no se ha escrito nada.
    let hogar = prueba._entorno.rutas.hogar.clone();
    std::fs::create_dir_all(&hogar).unwrap();
    assert!(std::fs::read_dir(&hogar).unwrap().next().is_none());
    // `w` propone una ruta; `↵` la guarda (create_new y 600).
    prueba.tecla(KeyCode::Char('w'));
    prueba.tecla(KeyCode::Enter);
    let fichero = std::fs::read_dir(&hogar)
        .unwrap()
        .filter_map(|entrada| entrada.ok())
        .map(|entrada| entrada.path())
        .find(|ruta| {
            ruta.file_name()
                .is_some_and(|nombre| nombre.to_string_lossy().starts_with("magi-historial-"))
        })
        .expect("sin fichero de historial");
    let contenido = std::fs::read_to_string(&fichero).unwrap();
    assert!(contenido.contains("secreto-de-la-sesion uno"));
    // Ni el texto del historial ni el nombre de la sesión van a REGISTRO.
    for entrada in prueba.registro() {
        assert!(
            !entrada.detalle.contains("secreto-de-la-sesion"),
            "{entrada:?}"
        );
    }
}

#[test]
fn una_pestana_sin_tmux_con_pantalla_alternativa_sigue_sin_historial() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![sesion_de(1, dev, "dev-01")]);
    prueba.volcar(1, "\x1b[?1049h\x1b[2J\x1b[Hvim\r\n");
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('['));
    assert!(prueba.app.historial.is_none());
    assert!(prueba
        .drenar()
        .iter()
        .all(|mensaje| !matches!(mensaje, MensajeCliente::CapturarHistorial { .. })));
    let mensaje = prueba.app.mensaje.as_ref().expect("sin aviso");
    assert!(
        mensaje.texto.contains("propio historial"),
        "{}",
        mensaje.texto
    );
}

#[test]
fn si_falla_el_historial_de_tmux_se_dice_y_no_se_abre_el_modo() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.volcar(1, "\x1b[?1049h\x1b[2J\x1b[Hbarra de tmux\r\n");
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('['));
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::CapturarHistorial { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin CapturarHistorial");
    prueba.servidor(MensajeServidor::Error {
        mensaje: "no se pudo leer el historial de tmux: se agotó el plazo".to_string(),
        peticion_id: Some(peticion_id),
    });
    assert!(prueba.app.historial.is_none());
    let mensaje = prueba.app.mensaje.as_ref().expect("sin aviso");
    assert!(mensaje.error);
    assert_eq!(
        mensaje.texto,
        "no se pudo leer el historial de tmux: se agotó el plazo"
    );
}

#[test]
fn una_persistente_soltada_no_abre_el_historial_de_tmux() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.volcar(1, "\x1b[?1049h\x1b[2J\x1b[Hbarra de tmux\r\n");
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        Some("portátil"),
        DESDE_FIJO,
    )]);
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('['));
    // Sin conexión no hay historial de tmux que leer.
    assert!(prueba.app.historial.is_none());
    assert!(prueba
        .drenar()
        .iter()
        .all(|mensaje| !matches!(mensaje, MensajeCliente::CapturarHistorial { .. })));
}

// ------------------------------------------------------------- prefijo N

#[test]
fn el_prefijo_n_mayuscula_abre_el_dialogo_de_nueva_sesion_con_el_host_de_la_pestana() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    prueba.entrar(vec![
        persistente(1, dev, "dev-01", "refactor"),
        sesion_de(2, db, "db-01"),
    ]);
    prueba.prefijo(KeyCode::Char('1'));
    prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    prueba.tecla_con(KeyCode::Char('N'), KeyModifiers::SHIFT);
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Persistentes(_))),
        "no se abrió el diálogo de nueva sesión persistente"
    );
    let texto = prueba.texto();
    assert!(texto.contains("NUEVA SESIÓN PERSISTENTE"), "{texto}");
    assert!(texto.contains("dev-01"), "{texto}");
    // El diálogo propone el host de la pestaña y no el de la otra.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(prueba.app.vista, Vista::Sesion);
}

// ------------------------------------------------------- host marcado

fn con_host_marcado() -> Prueba {
    let mut prueba = Prueba::con_inventario(|almacen| {
        let dev = almacen
            .crear_host(
                &DatosHost {
                    nombre: "dev-01".to_string(),
                    direccion: "192.0.2.10".to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("host de prueba");
        magi::almacen::hosts::fijar_sesiones_persistentes(almacen.conexion(), dev, true)
            .expect("marcar el host");
    });
    prueba.servidor(bienvenida(Vec::new()));
    prueba
}

#[test]
fn host_marcado_toda_pestaña_nueva_es_persistente_con_ficha_y_el_cliente_solo_abre() {
    let mut prueba = con_host_marcado();
    let dev = prueba.host_id("dev-01");
    prueba.drenar();
    // El cliente abre la pestaña como siempre: sin tmux en la petición (el
    // servidor elige el nombre y crea).
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Enter);
    let abrir = prueba
        .drenar()
        .into_iter()
        .find(|mensaje| matches!(mensaje, MensajeCliente::AbrirSesion { .. }))
        .expect("sin AbrirSesion");
    assert!(
        matches!(&abrir, MensajeCliente::AbrirSesion { host_id, tmux: None, .. } if *host_id == dev),
        "{abrir:?}"
    );
    // Primera lista: la pestaña se abre (aún sin tmux).
    let abriendo = InfoSesion {
        estado: EstadoSesionRemota::Abriendo,
        ..sesion_de(1, dev, "dev-01")
    };
    prueba.difundir(vec![abriendo]);
    assert_eq!(prueba.app.vista, Vista::Sesion);
    assert!(prueba.app.almacen.fichas_de_host(dev).unwrap().is_empty());
    // El servidor la abre con nombre automático: llega con su tmux propio y
    // marcada `creada` (la sesión no existía y la creó esta pestaña, D369);
    // una pestaña que se engancha a una que ya existía no crea ficha.
    prueba.difundir(vec![persistente_creada(1, dev, "dev-01", "sobremesa-1")]);
    let fichas = prueba.app.almacen.fichas_de_host(dev).unwrap();
    assert_eq!(fichas.len(), 1, "{fichas:?}");
    assert_eq!(fichas[0].nombre, "sobremesa-1");
    assert_eq!(fichas[0].origen.texto(), "host");
    let creadas = prueba
        .registro()
        .into_iter()
        .filter(|entrada| entrada.tipo == "sesion_persistente_creada")
        .count();
    assert_eq!(creadas, 1);
    // Más listas (título, actividad, reconexión…) no crean otra ficha ni otra
    // anotación: el cliente la resuelve una sola vez por pestaña.
    for _ in 0..3 {
        prueba.difundir(vec![InfoSesion {
            titulo: Some("Claude Code".to_string()),
            ..persistente(1, dev, "dev-01", "sobremesa-1")
        }]);
    }
    assert_eq!(prueba.app.almacen.fichas_de_host(dev).unwrap().len(), 1);
    let creadas = prueba
        .registro()
        .into_iter()
        .filter(|entrada| entrada.tipo == "sesion_persistente_creada")
        .count();
    assert_eq!(creadas, 1);
    // Cerrarla (prefijo x) pregunta como en la Fase 15, con el servidor propio.
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('x'));
    let Some(Dialogo::Elegir {
        lineas, opciones, ..
    }) = &prueba.app.dialogo
    else {
        panic!("sin el diálogo de cerrar con tmux");
    };
    assert!(lineas.iter().any(|linea| linea.contains("«sobremesa-1»")));
    assert!(lineas.iter().any(|linea| linea.contains("dev-01")));
    assert_eq!(opciones.len(), 2);
    prueba.tecla(KeyCode::Char('T'));
    let enviados = prueba.drenar();
    assert!(
        enviados.iter().any(|mensaje| matches!(
            mensaje,
            MensajeCliente::TerminarTmux { servidor: ServidorTmux::Magi, nombre, .. }
                if nombre == "sobremesa-1"
        )),
        "{enviados:?}"
    );
}

#[test]
fn cerrar_terminando_la_sesion_refresca_la_lista_de_persistentes_al_acabar() {
    // D370: la fila de «Persistentes» no se quedaba viva hasta el siguiente
    // refresco del vigilante tras cerrar la pestaña terminando su tmux.
    for resultado in [ResultadoTmux::Ok, ResultadoTmux::NoExiste] {
        let mut prueba = Prueba::nueva(&["dev-01"]);
        let dev = prueba.host_id("dev-01");
        prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
        prueba.drenar();
        prueba.prefijo(KeyCode::Char('x'));
        prueba.tecla(KeyCode::Char('T'));
        let enviados = prueba.drenar();
        let peticion_id = enviados
            .iter()
            .find_map(|mensaje| match mensaje {
                MensajeCliente::TerminarTmux { peticion_id, .. } => Some(*peticion_id),
                _ => None,
            })
            .expect("TerminarTmux");
        let consultas = |mensajes: &[MensajeCliente]| {
            mensajes
                .iter()
                .filter(|mensaje| {
                    matches!(
                        mensaje,
                        MensajeCliente::ConsultarSesionesPersistentes { .. }
                    )
                })
                .count()
        };
        assert_eq!(consultas(&enviados), 0, "aún no hay nada que refrescar");
        prueba.servidor(MensajeServidor::TmuxTerminada {
            peticion_id,
            servidor: ServidorTmux::Magi,
            resultado,
        });
        let enviados = prueba.drenar();
        assert_eq!(
            consultas(&enviados),
            1,
            "{resultado:?}: tras terminar se vuelve a mirar el host: {enviados:?}"
        );
    }
}

#[test]
fn host_marcado_sin_tmux_la_pestana_es_normal_y_se_enseña_el_aviso_del_servidor() {
    let mut prueba = con_host_marcado();
    let dev = prueba.host_id("dev-01");
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba.difundir(vec![InfoSesion {
        estado: EstadoSesionRemota::Abriendo,
        ..sesion_de(1, dev, "dev-01")
    }]);
    assert_eq!(prueba.app.vista, Vista::Sesion);
    // El servidor pone el aviso mientras la pestaña aún se abre: aunque se esté
    // mirando, hay que enseñarlo (en pantalla no hay nada que lo diga).
    let aviso = "dev-01 no tiene tmux: esta pestaña no es persistente";
    prueba.difundir(vec![InfoSesion {
        estado: EstadoSesionRemota::Abriendo,
        aviso: Some(InfoAviso {
            texto: aviso.to_string(),
            hora: chrono::Utc::now().timestamp(),
        }),
        ..sesion_de(1, dev, "dev-01")
    }]);
    let mensaje = prueba.app.mensaje.as_ref().expect("sin el aviso");
    assert_eq!(mensaje.texto, aviso);
    assert!(
        prueba.barra_global().contains(aviso),
        "{}",
        prueba.barra_global()
    );
    // Ya abierta, es una pestaña normal: sin marca y sin ficha.
    prueba.difundir(vec![sesion_de(1, dev, "dev-01")]);
    let texto = prueba.texto();
    assert!(!texto.contains('◆'), "{texto}");
    assert!(prueba.app.almacen.fichas_de_host(dev).unwrap().is_empty());
    // Cerrarla pregunta lo de siempre (no hay sesión de tmux que terminar).
    prueba.prefijo(KeyCode::Char('x'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
}

#[test]
fn un_aviso_de_la_campana_en_una_pestana_abierta_sigue_sin_enseñarse_si_se_mira() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.difundir(vec![InfoSesion {
        aviso: Some(InfoAviso {
            texto: "campana".to_string(),
            hora: chrono::Utc::now().timestamp(),
        }),
        ..persistente(1, dev, "dev-01", "refactor")
    }]);
    // La pestaña está abierta y a la vista: es de la sesión, no del servidor.
    assert!(prueba.app.mensaje.is_none());
    assert!(pestana(&prueba, 1).aviso.is_none());
}

// ----------------------------------------- errores con instrucción

fn fallo_de_apertura(
    prueba: &mut Prueba,
    host_id: i64,
    causa: CausaAperturaTmux,
    mensaje: &str,
) -> String {
    prueba.app.aperturas_pendientes.insert(host_id);
    prueba.servidor(MensajeServidor::AperturaTmuxFallida {
        host_id,
        nombre: "refactor".to_string(),
        causa,
        mensaje: mensaje.to_string(),
    });
    assert!(
        !prueba.app.aperturas_pendientes.contains(&host_id),
        "{causa:?}: el host se quedó con una apertura pendiente"
    );
    let aviso = prueba.app.mensaje.as_ref().expect("sin mensaje");
    assert!(aviso.error, "{causa:?}: no es un error");
    aviso.texto.clone()
}

#[test]
fn los_errores_de_apertura_dicen_que_hacer_segun_la_causa() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");

    let texto = fallo_de_apertura(
        &mut prueba,
        dev,
        CausaAperturaTmux::SinTmux,
        "instala tmux en el host «dev-01»",
    );
    assert!(texto.contains("instala tmux en el host"), "{texto}");

    let texto = fallo_de_apertura(
        &mut prueba,
        dev,
        CausaAperturaTmux::TmuxAntiguo,
        "tmux demasiado antiguo (3.0 o posterior) en «dev-01»",
    );
    assert!(
        texto.contains("tmux demasiado antiguo (3.0 o posterior)"),
        "{texto}"
    );
    assert!(texto.contains("actualiza tmux"), "{texto}");

    let texto = fallo_de_apertura(
        &mut prueba,
        dev,
        CausaAperturaTmux::SinCarpeta,
        "la carpeta ~/proyectos/api no existe en «dev-01»",
    );
    assert!(
        texto.contains("la carpeta ~/proyectos/api no existe en «dev-01»"),
        "{texto}"
    );
    assert!(texto.contains("créala o elige otra"), "{texto}");

    let texto = fallo_de_apertura(
        &mut prueba,
        dev,
        CausaAperturaTmux::Conexion,
        "no se pudo hablar con tmux en «dev-01»: se agotó el plazo",
    );
    assert!(texto.contains("se agotó el plazo"), "{texto}");
    assert!(texto.contains("comprueba la conexión"), "{texto}");
    assert!(texto.contains("conéctate a mano"), "{texto}");

    // D349: la sesión ya no estaba. El texto del servidor se conserva y se
    // dice qué hacer.
    let texto = fallo_de_apertura(
        &mut prueba,
        dev,
        CausaAperturaTmux::NoExiste,
        "la sesión «refactor» ya no existe en «dev-01»",
    );
    assert_eq!(
        texto,
        "la sesión «refactor» ya no existe en «dev-01» · si tiene ficha, reábrela desde Sesiones"
    );

    let texto = fallo_de_apertura(
        &mut prueba,
        dev,
        CausaAperturaTmux::Otra,
        "tmux no pudo abrir la sesión «refactor» en «dev-01»",
    );
    assert_eq!(texto, "tmux no pudo abrir la sesión «refactor» en «dev-01»");
}

#[test]
fn sin_texto_del_servidor_cada_causa_tiene_el_suyo() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    for (causa, esperado) in [
        (
            CausaAperturaTmux::SinTmux,
            "instala tmux en el host «dev-01»",
        ),
        (CausaAperturaTmux::TmuxAntiguo, "3.0 o posterior"),
        (CausaAperturaTmux::SinCarpeta, "no existe en «dev-01»"),
        (CausaAperturaTmux::Conexion, "comprueba la conexión"),
        (
            CausaAperturaTmux::NoExiste,
            "la sesión «refactor» ya no existe en «dev-01»",
        ),
        (
            CausaAperturaTmux::Otra,
            "no se pudo abrir la sesión de tmux «refactor»",
        ),
    ] {
        let texto = fallo_de_apertura(&mut prueba, dev, causa, "");
        assert!(texto.contains(esperado), "{causa:?}: {texto}");
    }
}

// ---------------------------------------------------------- reconexión

#[test]
fn una_reconexion_nunca_anota_sesion_persistente_enganchada() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    let tipos = |prueba: &Prueba| -> Vec<String> {
        prueba
            .registro()
            .into_iter()
            .map(|entrada| entrada.tipo)
            .filter(|tipo| tipo.starts_with("sesion_persistente_"))
            .collect()
    };
    let antes = tipos(&prueba);
    // Reconexión automática: la pestaña cae, reintenta y vuelve (la misma
    // sesión del servidor, con el mismo id).
    prueba.difundir(vec![InfoSesion {
        estado: EstadoSesionRemota::Caida,
        reconexion: Some(InfoReconexion {
            intento: 1,
            proximo_en: chrono::Utc::now().timestamp() + 2,
            desde: chrono::Utc::now().timestamp() - 3,
            causa: "conexión perdida".to_string(),
        }),
        ..persistente(1, dev, "dev-01", "refactor")
    }]);
    prueba.difundir(vec![persistente(1, dev, "dev-01", "refactor")]);
    // Reconexión a mano (prefijo r) y soltada que vuelve (↵ y c).
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('r'));
    prueba.difundir(vec![soltada_por(
        persistente(1, dev, "dev-01", "refactor"),
        Some("portátil"),
        DESDE_FIJO,
    )]);
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Char('c'));
    prueba.difundir(vec![persistente(1, dev, "dev-01", "refactor")]);
    assert_eq!(tipos(&prueba), antes);
    assert!(
        !tipos(&prueba)
            .iter()
            .any(|tipo| tipo == "sesion_persistente_enganchada"),
        "{:?}",
        tipos(&prueba)
    );
    // El cliente tampoco repite ficha ni anotación de creación.
    assert!(!tipos(&prueba)
        .iter()
        .any(|tipo| tipo == "sesion_persistente_creada"));
    // Las reconexiones mandan `Reconectar` y nada más hacia el servidor.
    let enviados = prueba.drenar();
    assert!(
        enviados.iter().all(
            |mensaje| matches!(mensaje, MensajeCliente::Reconectar { .. })
                || matches!(mensaje, MensajeCliente::Teclas { .. })
                || matches!(mensaje, MensajeCliente::Foco { .. })
                || matches!(mensaje, MensajeCliente::Adjuntar { .. })
                || matches!(mensaje, MensajeCliente::Desadjuntar { .. })
                || matches!(mensaje, MensajeCliente::Redimensionar { .. })
        ),
        "{enviados:?}"
    );
}

// ------------------------------------------------------------------ ayuda

#[test]
fn la_ayuda_de_sesion_y_sesiones_recoge_las_teclas_y_la_recomendacion_de_la_campana() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.redimensionar(200, 60);
    // En la vista Sesión: prefijo ?.
    prueba.prefijo(KeyCode::Char('?'));
    let texto = prueba.texto();
    assert!(texto.contains("AYUDA · SESIÓN"), "{texto}");
    assert!(texto.contains("^] N"), "{texto}");
    assert!(texto.contains("nueva sesión persistente"), "{texto}");
    assert!(texto.contains("volver (la suelta allí)"), "{texto}");
    assert!(texto.contains("c compartir"), "{texto}");
    assert!(texto.contains("x cerrar"), "{texto}");
    assert!(texto.contains("persistente, el de tmux"), "{texto}");
    prueba.tecla(KeyCode::Esc);
    // En Sesiones: las teclas del bloque, Pestañas y las recomendaciones.
    ir_a(&mut prueba, Vista::Sesiones);
    prueba.tecla_con(KeyCode::Char('?'), KeyModifiers::SHIFT);
    let texto = prueba.texto();
    assert!(texto.contains("AYUDA · SESIONES"), "{texto}");
    for esperado in [
        "engancharse, soltando a los demás",
        "engancharse compartiendo",
        "vista previa",
        "nueva sesión persistente",
        "editar la ficha",
        "renombrar",
        "terminar la sesión en el host",
        "borrar la ficha",
        "añadir a la lista",
        "borrar las fichas terminadas",
        "refrescar",
        "filtrar",
        "comprobar el host de la pestaña",
        "^] N",
        "preferredNotifChannel",
        "campana del terminal",
        "no pongas secretos",
        // D367: el shell del host que arranca tmux por su cuenta.
        "tmux en tmux",
        "$TMUX",
    ] {
        assert!(
            texto.contains(esperado),
            "falta «{esperado}» en la ayuda:\n{texto}"
        );
    }
}

#[test]
fn la_ayuda_de_sesiones_solo_dice_que_la_ficha_viaja_con_la_sincronizacion_configurada() {
    // D370: «viaja, cifrada, a tus equipos» solo es cierto con la
    // sincronización del inventario configurada.
    let mut prueba = Prueba::nueva(&["dev-01"]);
    prueba.redimensionar(200, 60);
    ir_a(&mut prueba, Vista::Sesiones);
    prueba.tecla_con(KeyCode::Char('?'), KeyModifiers::SHIFT);
    let sin = prueba.texto();
    assert!(sin.contains("no pongas secretos"), "{sin}");
    assert!(!sin.contains("viaja"), "{sin}");
    prueba.tecla(KeyCode::Esc);
    prueba.app.config.sincronizacion.carpeta = Some("~/Sync/magi".to_string());
    prueba.tecla_con(KeyCode::Char('?'), KeyModifiers::SHIFT);
    let con = prueba.texto();
    assert!(con.contains("viaja, cifrada"), "{con}");
}

/// Revisión adversarial: Hosts y Flota abren el diálogo con `N`, así que su
/// ayuda tiene que decirlo (ítem «Ayuda (?) …con las teclas nuevas»).
#[test]
fn la_ayuda_de_hosts_y_flota_dice_que_n_abre_una_sesion_persistente() {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    prueba.redimensionar(200, 60);
    for vista in [Vista::Hosts, Vista::Flota] {
        ir_a(&mut prueba, vista);
        prueba.tecla_con(KeyCode::Char('?'), KeyModifiers::SHIFT);
        let texto = prueba.texto();
        assert!(
            texto.contains("nueva sesión persistente en este host"),
            "{vista:?}: falta la tecla N en la ayuda:\n{texto}"
        );
        prueba.tecla(KeyCode::Esc);
    }
}

// ---------------------------------------- la soltada en la lista de «Pestañas»

/// La primera línea del pintado que lleva `nombre` (la fila de la lista, que va
/// antes que el panel inferior con el detalle).
fn fila_con(texto: &str, nombre: &str) -> String {
    texto
        .lines()
        .find(|linea| linea.contains(nombre))
        .unwrap_or_else(|| panic!("sin fila con «{nombre}»:\n{texto}"))
        .to_string()
}

/// Una soltada y una caída de verdad en la lista de Pestañas, con la soltada
/// elegida.
fn sesiones_con_soltada_y_caida(prueba: &mut Prueba) -> (i64, i64) {
    let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
    prueba.entrar(vec![
        persistente(1, dev, "dev-01", "refactor"),
        sesion_de(2, db, "db-01"),
    ]);
    prueba.difundir(vec![
        soltada_por(
            persistente(1, dev, "dev-01", "refactor"),
            Some("portátil"),
            DESDE_FIJO,
        ),
        InfoSesion {
            estado: EstadoSesionRemota::Caida,
            motivo: Some("conexión cerrada por el remoto".to_string()),
            ..sesion_de(2, db, "db-01")
        },
    ]);
    ir_a(prueba, Vista::Sesiones);
    prueba.app.seleccion_sesiones = 0;
    prueba.repintar();
    (dev, db)
}

/// Una pestaña soltada no es una caída (en la vista Sesión ya se trata aparte):
/// en la lista de «Pestañas» lleva su marca y el acento, la palabra «soltada»
/// y el equipo que la usa, nunca el aspa, «caída» ni el color de error.
#[test]
fn una_soltada_en_la_lista_de_pestanas_no_se_pinta_como_caida() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    sesiones_con_soltada_y_caida(&mut prueba);
    prueba.redimensionar(120, 30);
    let texto = prueba.texto();
    let soltada = fila_con(&texto, "refactor");
    assert!(soltada.contains("soltada"), "{texto}");
    assert!(soltada.contains('◆'), "{texto}");
    assert!(!soltada.contains("caída"), "{texto}");
    assert!(!soltada.contains('✕'), "{texto}");
    // El detalle de la fila elegida dice quién la usa, no «caída: …».
    assert!(
        texto.contains("refactor · soltada: la usa portátil"),
        "{texto}"
    );
    assert!(!texto.contains("caída: soltada"), "{texto}");
    // Una caída de verdad sigue con su aspa y su «caída».
    let caida = fila_con(&texto, "db-01");
    assert!(caida.contains('✕') && caida.contains("caída"), "{texto}");
    prueba.tecla(KeyCode::Down);
    assert!(
        prueba
            .texto()
            .contains("caída: conexión cerrada por el remoto"),
        "{}",
        prueba.texto()
    );
    // El color: la soltada va en acento (como en la vista Sesión) y la caída
    // en el color de error.
    prueba.tecla(KeyCode::Up);
    let tema = Tema::respaldo();
    let buffer = prueba.terminal.backend().buffer();
    let color_de = |texto_fila: &str, simbolo: &str| {
        (0..buffer.area.height)
            .find_map(|y| {
                let linea: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect();
                if !linea.contains(texto_fila) {
                    return None;
                }
                (0..buffer.area.width)
                    .find(|x| buffer[(*x, y)].symbol() == simbolo)
                    .map(|x| buffer[(x, y)].fg)
            })
            .unwrap_or_else(|| panic!("sin «{simbolo}» en la fila de «{texto_fila}»"))
    };
    assert_eq!(color_de("refactor", "◆"), tema.paleta.acento);
    assert_eq!(color_de("db-01", "✕"), tema.paleta.critico);
}

/// En ASCII la marca de la soltada la degrada `texto_ascii` (`P`), sin aspa.
#[test]
fn en_ascii_la_soltada_de_la_lista_de_pestanas_lleva_p_y_no_x() {
    let mut tema = Tema::respaldo();
    tema.ascii = true;
    tema.glifos = magi::tema::Glifos::ascii();
    let mut prueba = Prueba::con_tema(tema, |almacen| {
        for nombre in ["dev-01", "db-01"] {
            almacen
                .crear_host(
                    &DatosHost {
                        nombre: nombre.to_string(),
                        direccion: "192.0.2.10".to_string(),
                        ..DatosHost::default()
                    },
                    Origen::Manual,
                )
                .expect("host");
        }
    });
    sesiones_con_soltada_y_caida(&mut prueba);
    prueba.redimensionar(120, 30);
    let texto = prueba.texto();
    let soltada = fila_con(&texto, "refactor");
    assert!(soltada.contains("soltada"), "{texto}");
    assert!(soltada.contains("P "), "{texto}");
    assert!(
        !soltada.contains("caída") && !soltada.contains('x'),
        "{texto}"
    );
    assert!(!texto.contains('◆') && !texto.contains('✕'), "{texto}");
    // La caída de verdad lleva la `x` de siempre.
    let caida = fila_con(&texto, "db-01");
    assert!(caida.contains('x') && caida.contains("caída"), "{texto}");
}

#[test]
fn instantaneas_de_sesiones_con_una_soltada() {
    let mut prueba = Prueba::nueva(&["dev-01", "db-01"]);
    sesiones_con_soltada_y_caida(&mut prueba);
    prueba.instantaneas("sesiones_soltada");
}

/// Una persistente que no está soltada lleva en la lista de «Pestañas» el
/// glifo de su estado y, detrás, la marca (`P` en ASCII), con o sin aviso sin
/// ver; la normal, solo el glifo. La soltada es la excepción: solo la marca.
#[test]
fn una_persistente_lleva_la_marca_en_la_lista_de_pestanas_y_la_normal_no() {
    for ascii in [false, true] {
        let mut tema = Tema::respaldo();
        if ascii {
            tema.ascii = true;
            tema.glifos = magi::tema::Glifos::ascii();
        }
        let mut prueba = Prueba::con_tema(tema, |almacen| {
            for nombre in ["dev-01", "db-01"] {
                almacen
                    .crear_host(
                        &DatosHost {
                            nombre: nombre.to_string(),
                            direccion: "192.0.2.10".to_string(),
                            ..DatosHost::default()
                        },
                        Origen::Manual,
                    )
                    .expect("host");
            }
        });
        let (dev, db) = (prueba.host_id("dev-01"), prueba.host_id("db-01"));
        prueba.entrar(vec![
            persistente(1, dev, "dev-01", "refactor"),
            sesion_de(2, db, "db-01"),
        ]);
        ir_a(&mut prueba, Vista::Sesiones);
        prueba.redimensionar(120, 30);
        let texto = prueba.texto();
        let (marca, conectado) = if ascii { ("P", "*") } else { ("◆", "●") };
        let persistente_fila = fila_con(&texto, "refactor");
        assert!(
            persistente_fila.contains(&format!("{conectado} {marca}")),
            "{texto}"
        );
        let normal = fila_con(&texto, "db-01");
        assert!(normal.contains(conectado), "{texto}");
        assert!(!normal.contains(marca) && !normal.contains('◆'), "{texto}");
        // Con un aviso sin ver, el aviso va detrás de la marca y cabe en la
        // columna.
        let mut con_aviso = persistente(1, dev, "dev-01", "refactor");
        con_aviso.aviso = Some(InfoAviso {
            texto: "listo".to_string(),
            hora: chrono::Utc::now().timestamp(),
        });
        prueba.difundir(vec![con_aviso, sesion_de(2, db, "db-01")]);
        let texto = prueba.texto();
        let con_aviso = fila_con(&texto, "refactor");
        assert!(
            con_aviso.contains(&format!("{conectado} {marca} !")),
            "{texto}"
        );
    }
}

// ------------------------- historial de una persistente sin conexión (Fase 16)

/// Una persistente con tmux en la pantalla alternativa, ya difundida con
/// `info` (soltada, caída o reconectando) y con el modo historial pedido.
fn pedir_historial_sin_conexion(info: impl FnOnce(InfoSesion) -> InfoSesion) -> Prueba {
    let mut prueba = Prueba::nueva(&["dev-01"]);
    let dev = prueba.host_id("dev-01");
    prueba.entrar(vec![persistente(1, dev, "dev-01", "refactor")]);
    prueba.volcar(1, "\x1b[?1049h\x1b[2J\x1b[Hbarra de tmux\r\n");
    prueba.difundir(vec![info(persistente(1, dev, "dev-01", "refactor"))]);
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('['));
    assert!(prueba.app.historial.is_none());
    assert!(prueba
        .drenar()
        .iter()
        .all(|mensaje| !matches!(mensaje, MensajeCliente::CapturarHistorial { .. })));
    prueba
}

/// Sin conexión no hay historial de tmux que leer, y eso es lo que se dice (con
/// la instrucción): no que «este programa tiene su propio historial».
#[test]
fn el_historial_de_una_soltada_dice_que_hay_que_volver_a_engancharla() {
    let prueba =
        pedir_historial_sin_conexion(|info| soltada_por(info, Some("portátil"), DESDE_FIJO));
    let mensaje = prueba.app.mensaje.as_ref().expect("sin aviso");
    assert!(mensaje.error);
    assert!(mensaje.texto.contains("soltada"), "{}", mensaje.texto);
    assert!(
        mensaje.texto.contains("vuelve a engancharla (↵)"),
        "{}",
        mensaje.texto
    );
    assert!(
        !mensaje.texto.contains("propio historial"),
        "{}",
        mensaje.texto
    );
}

#[test]
fn el_historial_de_una_persistente_caida_dice_que_hay_que_reconectarla() {
    let prueba = pedir_historial_sin_conexion(|info| InfoSesion {
        estado: EstadoSesionRemota::Caida,
        motivo: Some("conexión cerrada por el remoto".to_string()),
        ..info
    });
    let mensaje = prueba.app.mensaje.as_ref().expect("sin aviso");
    assert!(mensaje.error);
    assert!(mensaje.texto.contains("caída"), "{}", mensaje.texto);
    assert!(mensaje.texto.contains("prefijo r"), "{}", mensaje.texto);
    assert!(
        !mensaje.texto.contains("propio historial"),
        "{}",
        mensaje.texto
    );
}

#[test]
fn el_historial_de_una_persistente_reconectando_dice_que_hay_que_esperar() {
    let prueba = pedir_historial_sin_conexion(|info| InfoSesion {
        estado: EstadoSesionRemota::Caida,
        reconexion: Some(InfoReconexion {
            intento: 2,
            proximo_en: chrono::Utc::now().timestamp() + 8,
            desde: chrono::Utc::now().timestamp() - 20,
            causa: "conexión perdida".to_string(),
        }),
        ..info
    });
    let mensaje = prueba.app.mensaje.as_ref().expect("sin aviso");
    assert!(mensaje.error);
    assert!(
        mensaje.texto.contains("aún no está conectada"),
        "{}",
        mensaje.texto
    );
    assert!(
        !mensaje.texto.contains("propio historial"),
        "{}",
        mensaje.texto
    );
}
