//! Pruebas de la interfaz de las sesiones persistentes: el bloque «Persistentes»
//! de la vista Sesiones, la vista previa, las acciones sobre fichas y sesiones y
//! las consultas al servidor (Fase 16, pista A1 del Sprint 3).
//!
//! Con `App::de_prueba` (sin hilos, red ni servidor), el bucle real sobre un
//! `TestBackend` y los mensajes del servidor inyectados a mano;
//! `Cliente::de_prueba` captura lo que se manda. Cubren la lista con todos los
//! estados (atención, enganchada aquí y en otro equipo, terminada, antigua, sin
//! ficha, de tu tmux y sin comprobar), `Tab`, cada tecla del bloque mandando el
//! mensaje correcto y solo cuando toca, las confirmaciones, que renombrar solo
//! cambia la ficha cuando el host dijo `ok`, las respuestas viejas descartadas,
//! la lista vieja de una ventana nueva y REGISTRO. Las instantáneas, a 40×12,
//! 80×24 y 200×60 (T45), llevan el prefijo del binario y se actualizan solo con
//! el filtro del nombre de la prueba (`INSTA_UPDATE=always cargo test -p magi-ssh
//! --test sesiones_persistentes_ui <nombre>`). Las de pintado corren con
//! `TZ=UTC`.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::persistentes::{DialogoPersistentes, FocoSesiones};
use magi::app::{AperturaInicial, App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::modelo::{DatosHost, Origen};
use magi::protocolo::{
    CausaAperturaTmux, ErrorComprobacion, EstadoFicha, EstadoSesionRemota, FichaVista, HostVisto,
    InfoSesion, ListaPersistentes, MensajeCliente, MensajeServidor, ResultadoRenombrar,
    ResultadoTmux, ServidorTmux, SesionVista, TmuxPestana, VersionTmux,
};
use magi::registro::FiltroRegistro;
use magi::tema::Tema;
use magi::ui::Vista;
use magi_nucleo::sesiones_persistentes::{DatosFicha, Ficha, OrigenFicha};

/// La hora del host en las pruebas: la actividad se cuenta contra ella.
const AHORA_HOST: i64 = 1_700_000_000;
/// Etiqueta de este equipo (`@magi_equipo`) en las pruebas.
const ESTE_EQUIPO: &str = "sobremesa";

// ------------------------------------------------------------------ arnés

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magilista")
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
        Self::con_tema(Tema::respaldo(), hosts)
    }

    fn con_tema(tema: Tema, hosts: &[&str]) -> Self {
        let entorno = entorno();
        {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
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
        }
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (mut app, enviados) =
            App::de_prueba(entorno.rutas.clone(), entorno.config.clone(), tema, almacen)
                .expect("app de prueba");
        // La etiqueta de este equipo no depende de la máquina que corre la prueba.
        app.persistentes.etiqueta_equipo = Some(ESTE_EQUIPO.to_string());
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

    fn letra(&mut self, letra: char) {
        self.tecla(KeyCode::Char(letra));
    }

    fn tecla_con(&mut self, codigo: KeyCode, modificadores: KeyModifiers) {
        self.app
            .procesar(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
        self.paso();
    }

    fn paso(&mut self) {
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn servidor(&mut self, mensaje: MensajeServidor) {
        self.app.procesar(Evento::Servidor(mensaje));
        self.paso();
    }

    fn tick(&mut self) {
        self.app.procesar(Evento::Tick);
        self.paso();
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

    /// La última fila de la pantalla: la barra.
    fn barra(&self) -> String {
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
                Instant::now() + Duration::from_millis(60),
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
        ajustes.add_filter(r"hace \d+ h", "hace NN h");
        ajustes.add_filter(r"en \d+ s", "en NN s");
        ajustes.add_filter(r"pid \d+", "pid N");
        ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
    }

    /// Las tres instantáneas obligatorias (T45) con el nombre dado.
    fn instantaneas(&mut self, nombre: &str) {
        for (cols, filas) in [(40, 12), (80, 24), (200, 60)] {
            self.redimensionar(cols, filas);
            self.instantanea(&format!(
                "sesiones_persistentes_ui__lista_{nombre}_{cols}x{filas}"
            ));
        }
        self.redimensionar(80, 24);
    }

    fn registro(&self, tipo: &str) -> Vec<magi::modelo::EntradaRegistro> {
        self.app
            .almacen
            .listar_registro(&FiltroRegistro::default(), 200, 0)
            .expect("registro")
            .into_iter()
            .filter(|entrada| entrada.tipo == tipo)
            .collect()
    }

    // -------------------------------------------------------- fichas y lista

    fn fichas(&self) -> Vec<Ficha> {
        magi::almacen::sesiones_persistentes::listar(self.app.almacen.conexion()).expect("fichas")
    }

    fn ficha(&self, host: &str, nombre: &str) -> Option<Ficha> {
        magi::almacen::sesiones_persistentes::por_nombre(
            self.app.almacen.conexion(),
            self.host_id(host),
            nombre,
        )
        .expect("lectura de la ficha")
    }

    /// Crea la ficha en la base de datos (la sesión ya existe en el host).
    fn sembrar(&self, host: &str, nombre: &str) -> String {
        magi::almacen::sesiones_persistentes::crear(
            self.app.almacen.conexion(),
            self.host_id(host),
            &DatosFicha {
                nombre: nombre.to_string(),
                carpeta: Some("~/proyectos/api".to_string()),
                comando: Some("claude".to_string()),
                comando_reanudar: Some("claude --continue".to_string()),
                nota: None,
                origen: OrigenFicha::Manual,
                creada_por: ESTE_EQUIPO.to_string(),
            },
        )
        .expect("ficha sembrada");
        self.ficha(host, nombre).expect("ficha creada").uuid
    }

    /// Entra en la vista Sesiones con la bienvenida del servidor y sin lista.
    fn entrar_en_sesiones(&mut self) {
        self.servidor(bienvenida(Vec::new()));
        self.tecla(KeyCode::F(3));
        assert_eq!(self.app.vista, Vista::Sesiones);
    }

    /// Entra en Sesiones y recibe la lista de `mundo`.
    fn con_el_mundo(&mut self) {
        self.entrar_en_sesiones();
        let lista = self.mundo();
        self.servidor(MensajeServidor::SesionesPersistentes { lista });
        self.drenar();
    }

    /// Cinco fichas en dos hosts, una sin ficha y una del tmux habitual: todos
    /// los estados de la lista (§6.1).
    fn mundo(&self) -> ListaPersistentes {
        let ahora = chrono::Utc::now().timestamp();
        let comprobada = ahora - 12;
        let web = self.host_id("web-01");
        let dev = self.host_id("dev-01");
        let mut fichas = Vec::new();
        for (host, host_id, nombre, estado, sesion) in [
            (
                "web-01",
                web,
                "agente-api",
                EstadoFicha::VivaLibre,
                Some(sesion(
                    "agente-api",
                    0,
                    None,
                    AHORA_HOST - 240,
                    true,
                    Some("Claude Code: permiso"),
                )),
            ),
            (
                "dev-01",
                dev,
                "refactor",
                EstadoFicha::VivaEnganchada,
                Some(sesion(
                    "refactor",
                    1,
                    Some(ESTE_EQUIPO),
                    AHORA_HOST - 20,
                    false,
                    Some("Claude Code"),
                )),
            ),
            (
                "dev-01",
                dev,
                "docs",
                EstadoFicha::VivaEnganchada,
                Some(sesion(
                    "docs",
                    1,
                    Some("portatil"),
                    AHORA_HOST - 60,
                    false,
                    Some("Claude Code"),
                )),
            ),
            ("dev-01", dev, "informes", EstadoFicha::Terminada, None),
            (
                "dev-01",
                dev,
                "pruebas",
                EstadoFicha::VivaLibre,
                Some(sesion(
                    "pruebas",
                    0,
                    None,
                    AHORA_HOST - 9 * 86_400,
                    false,
                    Some("usuario@dev-01"),
                )),
            ),
        ] {
            fichas.push(FichaVista {
                uuid: self.sembrar_o_leer(host, nombre),
                host_id,
                nombre: nombre.to_string(),
                estado,
                sesion,
                comprobada_en: comprobada,
            });
        }
        ListaPersistentes {
            fichas,
            hosts: vec![
                HostVisto {
                    host_id: web,
                    comprobada_en: comprobada,
                    ahora: AHORA_HOST,
                    tmux: Some(VersionTmux::nueva(3, 5)),
                    error: None,
                    sin_ficha: vec![sesion(
                        "portatil-3",
                        0,
                        None,
                        AHORA_HOST - 3 * 3600,
                        false,
                        None,
                    )],
                    habitual: vec![SesionVista {
                        servidor: ServidorTmux::Habitual,
                        ..sesion("trabajo", 0, None, AHORA_HOST - 86_400, false, None)
                    }],
                },
                HostVisto {
                    host_id: dev,
                    comprobada_en: comprobada,
                    ahora: AHORA_HOST,
                    tmux: Some(VersionTmux::nueva(3, 5)),
                    error: None,
                    sin_ficha: Vec::new(),
                    habitual: Vec::new(),
                },
            ],
        }
    }

    fn sembrar_o_leer(&self, host: &str, nombre: &str) -> String {
        match self.ficha(host, nombre) {
            Some(ficha) => ficha.uuid,
            None => self.sembrar(host, nombre),
        }
    }

    /// Lleva la selección de la lista a la fila con ese nombre, con teclas.
    fn ir_a(&mut self, nombre: &str) {
        self.tecla(KeyCode::Home);
        for _ in 0..30 {
            if self
                .app
                .fila_persistente_elegida()
                .is_some_and(|fila| fila.nombre == nombre)
            {
                return;
            }
            self.letra('j');
        }
        panic!("no se llegó a la fila «{nombre}»");
    }

    fn mensaje(&self) -> String {
        self.app
            .mensaje
            .as_ref()
            .map(|mensaje| mensaje.texto.clone())
            .unwrap_or_default()
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

fn sesion(
    nombre: &str,
    clientes: u32,
    equipo: Option<&str>,
    actividad: i64,
    campana: bool,
    titulo: Option<&str>,
) -> SesionVista {
    SesionVista {
        servidor: ServidorTmux::Magi,
        nombre: nombre.to_string(),
        clientes,
        equipo: equipo.map(str::to_string),
        actividad,
        creada: actividad - 1000,
        campana,
        ventanas: 1,
        titulo: titulo.map(str::to_string),
    }
}

/// Una pestaña normal de un host.
fn pestana_de(id: u32, host_id: i64, host: &str) -> InfoSesion {
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

/// Una pestaña enganchada a una sesión del tmux propio.
fn pestana_persistente(id: u32, host_id: i64, host: &str, nombre: &str) -> InfoSesion {
    InfoSesion {
        nombre: nombre.to_string(),
        tmux: Some(TmuxPestana {
            servidor: ServidorTmux::Magi,
            nombre: nombre.to_string(),
            version: Some(VersionTmux::nueva(3, 5)),
            soltada: None,
        }),
        ..pestana_de(id, host_id, host)
    }
}

/// La pestaña tal como la difunde el servidor al empezar a abrirla: sin tmux
/// todavía (`marcar_abierta` lo rellena una lista después).
fn pestana_abriendo(id: u32, host_id: i64, host: &str, nombre: &str) -> InfoSesion {
    InfoSesion {
        nombre: nombre.to_string(),
        estado: EstadoSesionRemota::Abriendo,
        ..pestana_de(id, host_id, host)
    }
}

/// Una pestaña enganchada a una sesión del tmux habitual del host.
fn pestana_habitual(id: u32, host_id: i64, host: &str, nombre: &str) -> InfoSesion {
    InfoSesion {
        nombre: nombre.to_string(),
        tmux: Some(TmuxPestana {
            servidor: ServidorTmux::Habitual,
            nombre: nombre.to_string(),
            version: Some(VersionTmux::nueva(3, 5)),
            soltada: None,
        }),
        ..pestana_de(id, host_id, host)
    }
}

/// Las consultas de sesiones persistentes que hay entre lo enviado.
fn consultas(mensajes: &[MensajeCliente]) -> Vec<(u64, Option<i64>)> {
    mensajes
        .iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::ConsultarSesionesPersistentes {
                peticion_id,
                host_id,
            } => Some((*peticion_id, *host_id)),
            _ => None,
        })
        .collect()
}

/// Las aperturas de sesión con tmux que hay entre lo enviado.
fn aperturas(mensajes: &[MensajeCliente]) -> Vec<(i64, magi::protocolo::PeticionTmux)> {
    mensajes
        .iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::AbrirSesion {
                host_id,
                tmux: Some(tmux),
                ..
            } => Some((*host_id, tmux.clone())),
            _ => None,
        })
        .collect()
}

fn dialogo_abierto(prueba: &Prueba) -> bool {
    matches!(prueba.app.dialogo, Some(Dialogo::Persistentes(_)))
}

// ------------------------------------------------------------- la lista

#[test]
fn la_lista_enseña_cada_estado_con_su_marca() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let texto = prueba.texto();
    assert!(
        texto.contains("PERSISTENTES · 5 · 2 sin ficha"),
        "cabecera: {texto}"
    );
    assert!(texto.contains("comprobadas hace "), "{texto}");
    // La atención lleva `!`, la enganchada aquí dice «aquí» y la de otro
    // equipo, su nombre.
    let fila = |nombre: &str| {
        texto
            .lines()
            .find(|linea| linea.contains(nombre))
            .unwrap_or_else(|| panic!("sin fila de {nombre}: {texto}"))
            .to_string()
    };
    let agente = fila("agente-api");
    assert!(
        agente.contains('!') && agente.contains('◆') && agente.contains("libre"),
        "{agente}"
    );
    assert!(agente.contains("Claude Code: permiso"), "{agente}");
    assert!(agente.contains("hace 4 m"), "{agente}");
    let refactor = fila("refactor");
    assert!(
        refactor.contains('●') && refactor.contains("aquí"),
        "{refactor}"
    );
    assert!(refactor.contains("hace 20 s"), "{refactor}");
    let docs = fila("docs");
    assert!(docs.contains("portatil") && docs.contains('●'), "{docs}");
    let informes = fila("informes");
    assert!(
        informes.contains('✕') && informes.contains("terminada"),
        "{informes}"
    );
    assert!(informes.contains("↵ reabrir"), "{informes}");
    // Más de siete días sin actividad: «antigua», con la hora del host.
    let pruebas = fila("pruebas");
    assert!(
        pruebas.contains("antigua") && pruebas.contains("hace 9 d"),
        "{pruebas}"
    );
    // Las que no tienen ficha no llevan la marca ◆.
    let sin_ficha = fila("portatil-3");
    assert!(
        sin_ficha.contains("sin ficha") && !sin_ficha.contains('◆'),
        "{sin_ficha}"
    );
    assert!(sin_ficha.contains("a añadir"), "{sin_ficha}");
    let habitual = fila("trabajo");
    assert!(
        habitual.contains("de tu tmux") && habitual.contains("hace 1 d"),
        "{habitual}"
    );
    assert!(habitual.contains("↵ engancharse"), "{habitual}");
    // Y las pestañas, debajo.
    assert!(texto.contains("PESTAÑAS · 0"), "{texto}");
}

#[test]
fn la_lista_a_tres_tamanos() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.instantaneas("completa");
}

#[test]
fn en_estrecho_el_bloque_enseña_marca_nombre_y_host() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.redimensionar(40, 12);
    let texto = prueba.texto();
    let agente = texto
        .lines()
        .find(|linea| linea.contains("agente-api"))
        .expect("fila de agente-api");
    assert!(
        agente.contains('◆') && agente.contains('!') && agente.contains("web-01"),
        "{agente}"
    );
    // Sin columnas secundarias: ni el estado, ni la actividad, ni el título.
    assert!(
        !agente.contains("libre") && !agente.contains("hace"),
        "{agente}"
    );
    assert!(!texto.contains("ACTIVIDAD"), "{texto}");
}

#[test]
fn en_ascii_la_marca_es_una_p_y_no_hay_glifos_unicode() {
    let mut tema = Tema::respaldo();
    tema.ascii = true;
    tema.glifos = magi::tema::Glifos::ascii();
    let mut prueba = Prueba::con_tema(tema, &["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let texto = prueba.texto();
    assert!(!texto.contains('◆'), "{texto}");
    let agente = texto
        .lines()
        .find(|linea| linea.contains("agente-api"))
        .expect("fila de agente-api");
    assert!(agente.contains('P'), "{agente}");
    let raro = texto
        .chars()
        .find(|c| !c.is_ascii() && !c.is_alphabetic() && !"┌┐└┘─│".contains(*c));
    assert!(raro.is_none(), "glifo no ASCII {raro:?} en:\n{texto}");
}

#[test]
fn sin_lista_dice_comprobando_y_con_una_vacia_dice_como_crear_una() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    prueba.entrar_en_sesiones();
    assert!(
        prueba.texto().contains("comprobando…"),
        "{}",
        prueba.texto()
    );
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes::default(),
    });
    let texto = prueba.texto();
    assert!(texto.contains("Sin sesiones persistentes"), "{texto}");
    assert!(!texto.contains("comprobando…"), "{texto}");
}

#[test]
fn un_host_que_no_se_pudo_comprobar_enseña_interrogacion_y_nunca_terminada() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    prueba.entrar_en_sesiones();
    let uuid = prueba.sembrar("web-01", "agente-api");
    let web = prueba.host_id("web-01");
    let ahora = chrono::Utc::now().timestamp();
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes {
            fichas: vec![FichaVista {
                uuid,
                host_id: web,
                nombre: "agente-api".to_string(),
                estado: EstadoFicha::Inalcanzable,
                sesion: None,
                comprobada_en: ahora,
            }],
            hosts: vec![HostVisto {
                host_id: web,
                comprobada_en: ahora,
                ahora: AHORA_HOST,
                tmux: None,
                error: Some(ErrorComprobacion::NecesitaDialogo),
                sin_ficha: Vec::new(),
                habitual: Vec::new(),
            }],
        },
    });
    let texto = prueba.texto();
    let fila = texto
        .lines()
        .find(|linea| linea.contains("agente-api"))
        .expect("fila");
    assert!(
        fila.contains('?') && fila.contains("sin comprobar"),
        "{fila}"
    );
    assert!(fila.contains("conéctate a mano"), "{fila}");
    assert!(!texto.contains("terminada"), "{texto}");
    // El motivo, en el panel de detalle y en la barra al pasar por la fila.
    assert!(
        texto.contains("sin comprobar: conéctate a mano"),
        "sin el motivo: {texto}"
    );
    prueba.letra('k');
    prueba.letra('j');
    assert!(
        prueba
            .barra()
            .contains("web-01: sin comprobar: conéctate a mano"),
        "barra: {}",
        prueba.barra()
    );
    prueba.instantaneas("sin_comprobar");
}

#[test]
fn una_ventana_nueva_con_la_lista_vieja_dice_la_edad_real() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.entrar_en_sesiones();
    let mut lista = prueba.mundo();
    let vieja = chrono::Utc::now().timestamp() - 50 * 60;
    for ficha in &mut lista.fichas {
        ficha.comprobada_en = vieja;
    }
    for host in &mut lista.hosts {
        host.comprobada_en = vieja;
    }
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    let texto = prueba.texto();
    assert!(texto.contains("comprobadas hace 50 m"), "{texto}");
    // Nunca se presenta como actual: la cabecera dice la edad real.
    let cabecera = texto
        .lines()
        .find(|linea| linea.contains("PERSISTENTES"))
        .expect("cabecera");
    assert!(
        cabecera.contains("hace 50 m") && !cabecera.contains("actualizando"),
        "{cabecera}"
    );
    // Y con una lista así no se escribe nada a partir de ella (`C` y `a`).
    prueba.letra('C');
    assert!(!matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
}

// ---------------------------------------------------------- consultas

#[test]
fn al_abrir_la_lista_consulta_una_vez_y_no_repite_la_que_sigue_en_vuelo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    prueba.servidor(bienvenida(Vec::new()));
    prueba.drenar();
    prueba.tecla(KeyCode::F(3));
    let enviadas = consultas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1, "una consulta al abrir la lista");
    assert!(
        enviadas[0].0 >= 1 << 49,
        "rango del terminal: {}",
        enviadas[0].0
    );
    assert_eq!(enviadas[0].1, None);
    // Más eventos con la lista a la vista no repiten la consulta en vuelo.
    prueba.tick();
    prueba.tick();
    prueba.tecla(KeyCode::Tab);
    prueba.letra('r');
    assert!(consultas(&prueba.drenar()).is_empty());
    assert!(
        prueba.mensaje().contains("ya se están comprobando"),
        "{}",
        prueba.mensaje()
    );
    // Llega la lista: ya no hay nada en vuelo y `r` vuelve a consultar.
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes::default(),
    });
    prueba.drenar();
    prueba.letra('r');
    assert_eq!(consultas(&prueba.drenar()).len(), 1);
}

#[test]
fn con_la_lista_a_la_vista_refresca_cada_refresco_seg_y_fuera_de_ella_no() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    prueba.entrar_en_sesiones();
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes::default(),
    });
    prueba.drenar();
    // Sin que pase el tiempo no hay consulta.
    prueba.tick();
    assert!(consultas(&prueba.drenar()).is_empty());
    // Con la última consulta de hace más del refresco (30 s), sí.
    prueba.app.persistentes.ultima_consulta = Instant::now().checked_sub(Duration::from_secs(31));
    prueba.tick();
    assert_eq!(consultas(&prueba.drenar()).len(), 1, "refresco periódico");
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes::default(),
    });
    prueba.drenar();
    // Fuera de la vista Sesiones no se consulta nunca.
    prueba.letra('q');
    assert_ne!(prueba.app.vista, Vista::Sesiones);
    prueba.app.persistentes.ultima_consulta = Instant::now().checked_sub(Duration::from_secs(300));
    prueba.tick();
    assert!(consultas(&prueba.drenar()).is_empty());
    // Al volver a abrirla, consulta de nuevo.
    prueba.tecla(KeyCode::F(3));
    assert_eq!(consultas(&prueba.drenar()).len(), 1);
}

#[test]
fn t_en_pestanas_consulta_el_host_de_la_pestana_y_lleva_a_sus_sesiones() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let web = prueba.host_id("web-01");
    let dev = prueba.host_id("dev-01");
    prueba.servidor(bienvenida(vec![
        pestana_de(1, web, "web-01"),
        pestana_de(2, dev, "dev-01"),
    ]));
    prueba.tecla(KeyCode::F(3));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    prueba.drenar();
    // Sin lista el foco es «Pestañas»: `j` elige la segunda y `t` pide su host.
    assert_eq!(prueba.app.foco_sesiones(), FocoSesiones::Pestanas);
    prueba.letra('j');
    prueba.letra('t');
    let enviadas = consultas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1, Some(dev), "el host de la pestaña elegida");
    // Cuando llega la comprobación de ese host, la selección va a sus filas.
    let ahora = chrono::Utc::now().timestamp();
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes {
            fichas: Vec::new(),
            hosts: vec![HostVisto {
                host_id: dev,
                comprobada_en: ahora + 1,
                ahora: AHORA_HOST,
                tmux: Some(VersionTmux::nueva(3, 5)),
                error: None,
                sin_ficha: vec![sesion("trabajo", 0, None, AHORA_HOST - 30, false, None)],
                habitual: Vec::new(),
            }],
        },
    });
    assert_eq!(prueba.app.foco_sesiones(), FocoSesiones::Persistentes);
    let elegida = prueba.app.fila_persistente_elegida().expect("fila");
    assert_eq!((elegida.host_id, elegida.nombre.as_str()), (dev, "trabajo"));
    assert!(
        prueba.mensaje().contains("sesiones de tmux en dev-01"),
        "{}",
        prueba.mensaje()
    );
}

#[test]
fn t_con_un_host_sin_tmux_o_que_pide_dialogo_dice_por_que() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    prueba.servidor(bienvenida(vec![pestana_de(1, web, "web-01")]));
    prueba.tecla(KeyCode::F(3));
    prueba.drenar();
    prueba.letra('t');
    let ahora = chrono::Utc::now().timestamp();
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes {
            fichas: Vec::new(),
            hosts: vec![HostVisto {
                host_id: web,
                comprobada_en: ahora + 1,
                ahora: AHORA_HOST,
                tmux: None,
                error: None,
                sin_ficha: Vec::new(),
                habitual: Vec::new(),
            }],
        },
    });
    assert!(
        prueba.mensaje().contains("no tiene tmux"),
        "{}",
        prueba.mensaje()
    );
}

// ------------------------------------------------------------- foco

#[test]
fn tab_pasa_a_pestanas_y_vuelve_y_las_teclas_solo_valen_con_su_bloque() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let web = prueba.host_id("web-01");
    prueba.servidor(bienvenida(vec![pestana_de(1, web, "web-01")]));
    prueba.tecla(KeyCode::F(3));
    let lista = prueba.mundo();
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    prueba.drenar();
    // Con filas, el foco automático es «Persistentes».
    assert_eq!(prueba.app.foco_sesiones(), FocoSesiones::Persistentes);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.foco_sesiones(), FocoSesiones::Pestanas);
    // En «Pestañas» valen las de la F3: `v` no abre ninguna vista previa y `n`
    // abre la paleta de conectar.
    prueba.letra('v');
    assert!(prueba.drenar().is_empty(), "v no manda nada desde Pestañas");
    assert!(!dialogo_abierto(&prueba));
    prueba.letra('n');
    assert!(prueba.app.paleta.is_some(), "n abre la paleta de conectar");
    prueba.tecla(KeyCode::Esc);
    // `↵` entra en la pestaña, no se engancha a ninguna sesión persistente.
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.vista, Vista::Sesion);
    assert!(aperturas(&prueba.drenar()).is_empty());
    // De vuelta a Sesiones, `Tab` lleva a Persistentes.
    prueba.app.vista = Vista::Sesiones;
    prueba.paso();
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.foco_sesiones(), FocoSesiones::Persistentes);
}

#[test]
fn una_lista_que_llega_tarde_no_cambia_lo_que_hace_enter_bajo_los_dedos() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let web = prueba.host_id("web-01");
    prueba.servidor(bienvenida(vec![pestana_de(1, web, "web-01")]));
    prueba.tecla(KeyCode::F(3));
    // El usuario ya ha pulsado una tecla en «Pestañas»…
    prueba.letra('k');
    assert_eq!(prueba.app.foco_sesiones(), FocoSesiones::Pestanas);
    // …y la lista llega después: el foco no se lo roba.
    let lista = prueba.mundo();
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    assert_eq!(prueba.app.foco_sesiones(), FocoSesiones::Pestanas);
    prueba.drenar();
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.vista, Vista::Sesion, "↵ entró en la pestaña");
    assert!(aperturas(&prueba.drenar()).is_empty());
}

#[test]
fn la_seleccion_sigue_a_su_fila_cuando_la_lista_se_refresca() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    // Llega una lista con una ficha nueva por delante: la selección sigue en
    // «agente-api».
    let nueva = prueba.sembrar("dev-01", "aaa-primera");
    let mut lista = prueba.mundo();
    lista.fichas.push(FichaVista {
        uuid: nueva,
        host_id: prueba.host_id("dev-01"),
        nombre: "aaa-primera".to_string(),
        estado: EstadoFicha::VivaLibre,
        sesion: Some(sesion("aaa-primera", 0, None, AHORA_HOST - 5, false, None)),
        comprobada_en: chrono::Utc::now().timestamp(),
    });
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    let elegida = prueba.app.fila_persistente_elegida().expect("fila elegida");
    assert_eq!(elegida.nombre, "agente-api");
}

// ------------------------------------------------------- engancharse

#[test]
fn enter_se_engancha_soltando_y_c_compartiendo_y_solo_se_anota_al_aparecer_la_pestana() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    let dev = prueba.host_id("dev-01");
    // `↵` sobre una libre: AbrirSesion{magi, nombre, sin crear, sin compartir}.
    prueba.ir_a("agente-api");
    prueba.tecla(KeyCode::Enter);
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1, "{enviadas:?}");
    assert_eq!(enviadas[0].0, web);
    assert_eq!(enviadas[0].1.servidor, ServidorTmux::Magi);
    assert_eq!(enviadas[0].1.nombre, "agente-api");
    assert!(enviadas[0].1.crear.is_none());
    assert!(!enviadas[0].1.compartir);
    // Todavía no hay efecto: no se anota nada.
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());
    // Aparece la pestaña: ahora sí, con host, nombre y qué se hizo.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, web, "web-01", "agente-api")],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 1);
    assert!(
        anotadas[0].detalle.contains("web-01") && anotadas[0].detalle.contains("agente-api"),
        "{}",
        anotadas[0].detalle
    );
    assert!(!anotadas[0].detalle.contains("claude"), "nada de comandos");
    // Otra difusión con la misma pestaña (una reconexión, por ejemplo) no anota.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, web, "web-01", "agente-api")],
    });
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);

    // `c` sobre la que tiene otro equipo: compartiendo, y se dice a quién.
    prueba.app.vista = Vista::Sesiones;
    prueba.paso();
    prueba.ir_a("docs");
    prueba.drenar();
    prueba.letra('c');
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1, "{enviadas:?}");
    assert_eq!(enviadas[0].0, dev);
    assert!(enviadas[0].1.compartir);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![
            pestana_persistente(1, web, "web-01", "agente-api"),
            pestana_persistente(2, dev, "dev-01", "docs"),
        ],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 2);
    assert!(
        anotadas
            .iter()
            .any(|entrada| entrada.detalle.contains("compartiendo")),
        "{anotadas:?}"
    );
}

#[test]
fn enter_sobre_una_ocupada_dice_a_quien_suelta() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let dev = prueba.host_id("dev-01");
    prueba.ir_a("docs");
    prueba.tecla(KeyCode::Enter);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, dev, "dev-01", "docs")],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 1);
    assert!(
        anotadas[0].detalle.contains("soltando a portatil"),
        "{}",
        anotadas[0].detalle
    );
}

#[test]
fn el_enganche_se_anota_con_la_secuencia_real_del_servidor_una_sola_vez() {
    // Defecto de la revisión adversarial: el servidor difunde la pestaña
    // `Abriendo`, sin tmux, y rellena el tmux en `marcar_abierta`, UNA LISTA
    // DESPUÉS. El enganche se anota al pasar a «con tmux propio», nunca antes
    // y una sola vez.
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(aperturas(&prueba.drenar()).len(), 1);

    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_abriendo(1, web, "web-01", "agente-api")],
    });
    assert!(
        prueba.registro("sesion_persistente_enganchada").is_empty(),
        "la pestaña aún se abre: sin tmux no hay efecto que anotar"
    );
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, web, "web-01", "agente-api")],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 1, "{anotadas:?}");
    assert!(
        anotadas[0].detalle.contains("web-01") && anotadas[0].detalle.contains("agente-api"),
        "{}",
        anotadas[0].detalle
    );
    assert!(prueba.app.persistentes.creaciones.is_empty());

    // Una difusión igual y una reconexión automática (cae y vuelve con el
    // mismo id) no anotan otra vez.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, web, "web-01", "agente-api")],
    });
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![InfoSesion {
            estado: EstadoSesionRemota::Caida,
            ..pestana_persistente(1, web, "web-01", "agente-api")
        }],
    });
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, web, "web-01", "agente-api")],
    });
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);
}

#[test]
fn c_compartiendo_y_el_tmux_habitual_tambien_se_anotan_con_la_secuencia_real() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    let dev = prueba.host_id("dev-01");
    // `c` sobre la que tiene otro equipo.
    prueba.ir_a("docs");
    prueba.letra('c');
    assert_eq!(aperturas(&prueba.drenar()).len(), 1);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_abriendo(1, dev, "dev-01", "docs")],
    });
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, dev, "dev-01", "docs")],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 1, "{anotadas:?}");
    assert!(anotadas[0].detalle.contains("compartiendo"), "{anotadas:?}");

    // Una del tmux habitual: el servidor la difunde igual (sin tmux y luego
    // con el tmux habitual).
    prueba.app.vista = Vista::Sesiones;
    prueba.paso();
    prueba.ir_a("trabajo");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(aperturas(&prueba.drenar()).len(), 1);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![
            pestana_persistente(1, dev, "dev-01", "docs"),
            pestana_abriendo(2, web, "web-01", "trabajo"),
        ],
    });
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![
            pestana_persistente(1, dev, "dev-01", "docs"),
            pestana_habitual(2, web, "web-01", "trabajo"),
        ],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 2, "{anotadas:?}");
    assert!(
        anotadas
            .iter()
            .any(|entrada| entrada.detalle.contains("trabajo")),
        "{anotadas:?}"
    );
}

#[test]
fn magi_sesion_anota_el_enganche_con_la_secuencia_real_del_servidor() {
    // `magi sesion <nombre>` (informe §3.3): el efecto es la pestaña con su
    // tmux, que llega una lista después de la que la enseña abriéndose.
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    prueba.servidor(bienvenida(Vec::new()));
    prueba.drenar();
    prueba.app.abrir_al_arrancar = Some(AperturaInicial::Sesion {
        host_id: web,
        nombre: "agente-api".to_string(),
    });
    prueba.app.abrir_lo_del_arranque();
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1, "{enviadas:?}");
    assert!(!enviadas[0].1.compartir && enviadas[0].1.crear.is_none());
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());

    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_abriendo(1, web, "web-01", "agente-api")],
    });
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(1, web, "web-01", "agente-api")],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 1, "{anotadas:?}");
    assert!(
        anotadas[0].detalle.contains("web-01") && anotadas[0].detalle.contains("agente-api"),
        "{}",
        anotadas[0].detalle
    );
    assert!(prueba.app.persistentes.creaciones.is_empty());

    // Si el servidor no puede abrirla, no queda nada esperando: otra pestaña
    // con el mismo host y nombre no es el efecto de lo pedido.
    let mut prueba = Prueba::nueva(&["web-01"]);
    let web = prueba.host_id("web-01");
    prueba.servidor(bienvenida(Vec::new()));
    prueba.drenar();
    prueba.app.abrir_al_arrancar = Some(AperturaInicial::Sesion {
        host_id: web,
        nombre: "agente-api".to_string(),
    });
    prueba.app.abrir_lo_del_arranque();
    assert_eq!(aperturas(&prueba.drenar()).len(), 1);
    prueba.servidor(MensajeServidor::AperturaTmuxFallida {
        host_id: web,
        nombre: "agente-api".to_string(),
        causa: CausaAperturaTmux::Conexion,
        mensaje: "web-01 no responde".to_string(),
    });
    assert!(prueba.app.persistentes.creaciones.is_empty());
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(7, web, "web-01", "agente-api")],
    });
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());
}

#[test]
fn engancharse_a_una_sesion_sin_ficha_de_un_host_marcado_no_le_crea_ficha() {
    // Defecto de la revisión adversarial: el enganche desde la lista a una
    // sesión que ya existía (sin ficha) en un host marcado creaba una ficha
    // `origen = host` y una anotación «creada» por algo que no se creó ahora.
    // El `host` es solo para las pestañas nuevas que crea el propio host
    // marcado.
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba
        .app
        .almacen
        .fijar_sesiones_persistentes(web, true)
        .expect("marcar el host");
    prueba.app.recargar_inventario().expect("recargar");
    prueba.ir_a("portatil-3");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(aperturas(&prueba.drenar()).len(), 1);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_abriendo(1, web, "web-01", "portatil-3")],
    });
    // El servidor pone `abierta_en` al abrirla: es una pestaña recién abierta,
    // de las que sí guardarían ficha si el enganche no la excluyera.
    let abierta_ahora = InfoSesion {
        abierta_en: chrono::Utc::now().timestamp(),
        ..pestana_persistente(1, web, "web-01", "portatil-3")
    };
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![abierta_ahora],
    });
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);
    assert!(
        prueba.ficha("web-01", "portatil-3").is_none(),
        "la sesión ya existía: no es de las que crea el host marcado"
    );
    assert!(prueba.registro("sesion_persistente_creada").is_empty());

    // En cambio, la pestaña nueva que abre el propio host marcado (nombre
    // automático, sin enganche pedido) sí guarda su ficha con `origen = host`.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![
            pestana_persistente(1, web, "web-01", "portatil-3"),
            pestana_abriendo(2, web, "web-01", "sobremesa-1"),
        ],
    });
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![
            pestana_persistente(1, web, "web-01", "portatil-3"),
            InfoSesion {
                // Recién abierta por el servidor: de otro modo no cuenta como
                // «creada ahora» (ver `pestana_persistente_reciente`).
                abierta_en: chrono::Utc::now().timestamp(),
                ..pestana_persistente(2, web, "web-01", "sobremesa-1")
            },
        ],
    });
    let nueva = prueba
        .ficha("web-01", "sobremesa-1")
        .expect("ficha del host");
    assert_eq!(nueva.origen, OrigenFicha::Host);
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);
}

#[test]
fn si_la_apertura_ya_tiene_pestana_en_esta_ventana_no_se_anota_otro_enganche() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let web = prueba.host_id("web-01");
    prueba.servidor(bienvenida(vec![pestana_persistente(
        1,
        web,
        "web-01",
        "agente-api",
    )]));
    prueba.tecla(KeyCode::F(3));
    prueba.sembrar("web-01", "agente-api");
    let lista = prueba.mundo();
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    prueba.drenar();
    prueba.ir_a("agente-api");
    prueba.tecla(KeyCode::Enter);
    // Va a la pestaña que ya existe: no manda AbrirSesion ni anota nada.
    assert!(aperturas(&prueba.drenar()).is_empty());
    assert_eq!(prueba.app.vista, Vista::Sesion);
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());
}

#[test]
fn enter_sobre_una_sin_ficha_y_sobre_una_del_tmux_habitual_se_engancha_sin_ficha() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("portatil-3");
    prueba.tecla(KeyCode::Enter);
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1.servidor, ServidorTmux::Magi);
    assert_eq!(enviadas[0].1.nombre, "portatil-3");
    // Se libera el host para la siguiente apertura.
    prueba.servidor(MensajeServidor::AperturaTmuxFallida {
        host_id: web,
        nombre: "portatil-3".to_string(),
        causa: CausaAperturaTmux::Otra,
        mensaje: "la sesión ya no existe".to_string(),
    });
    prueba.ir_a("trabajo");
    prueba.tecla(KeyCode::Enter);
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1.servidor, ServidorTmux::Habitual);
    assert_eq!(enviadas[0].1.nombre, "trabajo");
    assert!(enviadas[0].1.crear.is_none());
    // No crea ninguna ficha.
    assert!(prueba.ficha("web-01", "trabajo").is_none());
}

#[test]
fn enter_sobre_una_terminada_la_reabre_con_su_comando_de_reanudacion() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let dev = prueba.host_id("dev-01");
    prueba.ir_a("informes");
    prueba.tecla(KeyCode::Enter);
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1, "{enviadas:?}");
    assert_eq!(enviadas[0].0, dev);
    let crear = enviadas[0].1.crear.clone().expect("crea la sesión");
    assert_eq!(crear.carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(crear.comando.as_deref(), Some("claude --continue"));
    // `c` sobre una terminada no la reabre: dice qué hacer.
    prueba.servidor(MensajeServidor::AperturaTmuxFallida {
        host_id: dev,
        nombre: "informes".to_string(),
        causa: CausaAperturaTmux::Otra,
        mensaje: "no se pudo".to_string(),
    });
    prueba.drenar();
    prueba.letra('c');
    assert!(aperturas(&prueba.drenar()).is_empty());
    assert!(
        prueba.mensaje().contains("la vuelve a crear"),
        "{}",
        prueba.mensaje()
    );
}

#[test]
fn enter_sobre_un_host_sin_tmux_no_abre_nada_y_dice_que_hacer() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    prueba.entrar_en_sesiones();
    let uuid = prueba.sembrar("web-01", "agente-api");
    let web = prueba.host_id("web-01");
    let ahora = chrono::Utc::now().timestamp();
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes {
            fichas: vec![FichaVista {
                uuid,
                host_id: web,
                nombre: "agente-api".to_string(),
                estado: EstadoFicha::SinTmux,
                sesion: None,
                comprobada_en: ahora,
            }],
            hosts: vec![HostVisto {
                host_id: web,
                comprobada_en: ahora,
                ahora: AHORA_HOST,
                tmux: Some(VersionTmux::nueva(2, 9)),
                error: None,
                sin_ficha: Vec::new(),
                habitual: Vec::new(),
            }],
        },
    });
    prueba.drenar();
    prueba.tecla(KeyCode::Enter);
    assert!(aperturas(&prueba.drenar()).is_empty());
    assert!(
        prueba.mensaje().contains("no tiene tmux 3.0"),
        "{}",
        prueba.mensaje()
    );
    prueba.tecla(KeyCode::Char('v'));
    assert!(!dialogo_abierto(&prueba));
}

// ----------------------------------------------------- nueva y paleta

#[test]
fn n_abre_el_dialogo_de_nueva_sesion_con_el_host_de_la_fila() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    prueba.letra('n');
    assert!(
        matches!(
            prueba.app.dialogo,
            Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(_)))
        ),
        "n no abrió el diálogo"
    );
    assert!(prueba.texto().contains("web-01"), "{}", prueba.texto());
}

#[test]
fn la_paleta_ofrece_nueva_sesion_y_engancharse_a_cada_ficha_viva_o_terminada() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    let etiquetas: Vec<String> = prueba
        .app
        .paleta
        .as_ref()
        .expect("paleta abierta")
        .entradas
        .iter()
        .map(|entrada| entrada.etiqueta.clone())
        .collect();
    assert!(
        etiquetas
            .iter()
            .any(|etiqueta| etiqueta == "nueva sesión persistente"),
        "{etiquetas:?}"
    );
    for esperada in [
        "engancharse a agente-api · web-01",
        "engancharse a refactor · dev-01",
        "engancharse a docs · dev-01",
        "engancharse a informes · dev-01",
        "engancharse a pruebas · dev-01",
    ] {
        assert!(
            etiquetas.iter().any(|etiqueta| etiqueta == esperada),
            "falta «{esperada}» en {etiquetas:?}"
        );
    }
    // Las sin ficha y las del tmux habitual no tienen entrada.
    assert!(!etiquetas
        .iter()
        .any(|etiqueta| etiqueta.contains("portatil-3")));
    assert!(!etiquetas
        .iter()
        .any(|etiqueta| etiqueta.contains("trabajo")));
}

#[test]
fn elegir_engancharse_en_la_paleta_abre_la_sesion_sin_soltar_a_nadie_mas_de_lo_pedido() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    for letra in "engancharse a agente".chars() {
        prueba.letra(letra);
    }
    prueba.tecla(KeyCode::Enter);
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1, "{enviadas:?}");
    assert_eq!(enviadas[0].0, web);
    assert_eq!(enviadas[0].1.nombre, "agente-api");
    assert!(!enviadas[0].1.compartir);
}

// -------------------------------------------------------- vista previa

fn lineas_de_vista(texto: &[&str]) -> Vec<String> {
    texto.iter().map(|linea| (*linea).to_string()).collect()
}

#[test]
fn v_pide_la_vista_previa_y_la_enseña_con_la_hora_y_las_teclas() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra('v');
    let pedidas: Vec<(u64, i64, ServidorTmux, String)> = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::PedirVistaPrevia {
                peticion_id,
                host_id,
                servidor,
                nombre,
            } => Some((peticion_id, host_id, servidor, nombre)),
            _ => None,
        })
        .collect();
    assert_eq!(pedidas.len(), 1);
    let (peticion_id, host_id, servidor, nombre) = pedidas[0].clone();
    assert!(peticion_id >= 1 << 49);
    assert_eq!(
        (host_id, servidor, nombre.as_str()),
        (web, ServidorTmux::Magi, "agente-api")
    );
    assert!(dialogo_abierto(&prueba));
    assert!(prueba.texto().contains("leyendo…"), "{}", prueba.texto());
    // La respuesta de otra petición no la rellena.
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: peticion_id + 7,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: lineas_de_vista(&["no debe verse"]),
        leida_en: AHORA_HOST,
        error: None,
    });
    assert!(!prueba.texto().contains("no debe verse"));
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: lineas_de_vista(&[
            "● Cambios listos en src/pagos/cobro.py",
            "",
            "¿Ejecuto las pruebas?",
            "❯ 1. Sí",
            "  2. Sí, y no vuelvas a preguntar",
            "  3. No, dime qué cambiar",
        ]),
        leida_en: AHORA_HOST,
        error: None,
    });
    let texto = prueba.texto();
    assert!(
        texto.contains("Cambios listos en src/pagos/cobro.py"),
        "{texto}"
    );
    assert!(texto.contains("últimas 20 líneas"), "{texto}");
    assert!(
        texto.contains("engancharse") && texto.contains("refrescar") && texto.contains("volver"),
        "{texto}"
    );
    assert!(
        texto.contains("agente-api") && texto.contains("web-01"),
        "{texto}"
    );
    prueba.instantaneas("vista_previa");
}

#[test]
fn la_vista_previa_descarta_la_respuesta_de_una_peticion_que_ya_no_esta_en_vuelo() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra('v');
    let primera = peticion_de_vista(&mut prueba);
    // Esc cierra: la respuesta tardía no abre ni rellena nada.
    prueba.tecla(KeyCode::Esc);
    assert!(!dialogo_abierto(&prueba));
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: primera,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: lineas_de_vista(&["tarde"]),
        leida_en: AHORA_HOST,
        error: None,
    });
    assert!(!dialogo_abierto(&prueba));
    assert!(!prueba.texto().contains("tarde"));
    // `r` pide otra y solo la nueva vale.
    prueba.letra('v');
    let segunda = peticion_de_vista(&mut prueba);
    prueba.letra('r');
    let tercera = peticion_de_vista(&mut prueba);
    assert_ne!(segunda, tercera);
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: segunda,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: lineas_de_vista(&["vieja"]),
        leida_en: AHORA_HOST,
        error: None,
    });
    assert!(!prueba.texto().contains("vieja"));
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: tercera,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: lineas_de_vista(&["buena"]),
        leida_en: AHORA_HOST,
        error: None,
    });
    assert!(prueba.texto().contains("buena"));
}

fn peticion_de_vista(prueba: &mut Prueba) -> u64 {
    prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirVistaPrevia { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("PedirVistaPrevia")
}

#[test]
fn un_error_de_la_vista_previa_se_enseña_con_instruccion_y_la_lista_sigue() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra('v');
    let peticion = peticion_de_vista(&mut prueba);
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: peticion,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: Vec::new(),
        leida_en: AHORA_HOST,
        error: Some("web-01 no respondió en 10 s".to_string()),
    });
    let texto = prueba.texto();
    assert!(
        texto.contains("no se pudo leer: web-01 no respondió en 10 s"),
        "{texto}"
    );
    assert!(texto.contains("pulsa r"), "{texto}");
    prueba.instantaneas("vista_previa_error");
    prueba.tecla(KeyCode::Esc);
    assert!(!dialogo_abierto(&prueba));
    assert!(prueba.texto().contains("agente-api"));
}

#[test]
fn enter_en_la_vista_previa_se_engancha_a_la_sesion_fijada_al_abrir() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra('v');
    prueba.drenar();
    // Mientras se mira, llega una lista en la que la sesión ya no está (y la
    // selección de la lista cae en otra fila): la vista previa actúa sobre lo
    // que enseñaba, no sobre lo que haya ahora bajo la selección.
    let mut lista = prueba.mundo();
    lista.fichas.retain(|ficha| ficha.nombre != "agente-api");
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    assert!(dialogo_abierto(&prueba));
    assert!(prueba
        .app
        .filas_persistentes()
        .iter()
        .all(|fila| fila.nombre != "agente-api"));
    prueba.tecla(KeyCode::Enter);
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1);
    assert_eq!(
        (enviadas[0].0, enviadas[0].1.nombre.as_str()),
        (web, "agente-api")
    );
    assert!(!dialogo_abierto(&prueba));
}

#[test]
fn la_vista_previa_no_se_guarda_ni_va_a_registro() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra('v');
    let peticion = peticion_de_vista(&mut prueba);
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: peticion,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: lineas_de_vista(&["contraseña-que-no-debe-guardarse"]),
        leida_en: AHORA_HOST,
        error: None,
    });
    prueba.tecla(KeyCode::Esc);
    let registro = prueba
        .app
        .almacen
        .listar_registro(&FiltroRegistro::default(), 500, 0)
        .expect("registro");
    assert!(
        registro
            .iter()
            .all(|entrada| !entrada.detalle.contains("contraseña-que-no-debe-guardarse")),
        "el contenido de una vista previa fue a REGISTRO"
    );
    // Una vez cerrada, no queda en memoria del diálogo ni de la lista.
    assert!(!prueba.texto().contains("contraseña-que-no-debe-guardarse"));
}

// ------------------------------------------------------------ ficha

fn escribir(prueba: &mut Prueba, texto: &str) {
    for letra in texto.chars() {
        prueba.letra(letra);
    }
}

#[test]
fn e_edita_la_ficha_y_la_guarda_con_el_almacen_subiendo_la_version() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    let antes = prueba.ficha("web-01", "agente-api").expect("ficha");
    prueba.letra('e');
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Persistentes(DialogoPersistentes::Ficha(_)))
    ));
    assert!(
        prueba.texto().contains("No pongas secretos en el comando"),
        "{}",
        prueba.texto()
    );
    prueba.instantaneas("ficha");
    // Carpeta: se vacía y se escribe otra; Tab al comando; Tab, Tab a la nota.
    prueba.tecla(KeyCode::Home);
    for _ in 0..40 {
        prueba.tecla(KeyCode::Delete);
    }
    escribir(&mut prueba, "/srv/api");
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    escribir(&mut prueba, "refactor de pagos");
    prueba.tecla(KeyCode::Enter);
    assert!(!dialogo_abierto(&prueba), "{}", prueba.texto());
    let despues = prueba.ficha("web-01", "agente-api").expect("ficha");
    assert_eq!(despues.carpeta.as_deref(), Some("/srv/api"));
    assert_eq!(despues.nota.as_deref(), Some("refactor de pagos"));
    assert_eq!(despues.comando.as_deref(), Some("claude"));
    assert_eq!(despues.uuid, antes.uuid);
    assert!(
        prueba.mensaje().contains("ficha de «agente-api» guardada"),
        "{}",
        prueba.mensaje()
    );
}

#[test]
fn una_carpeta_no_valida_se_dice_en_el_campo_y_no_cierra_el_dialogo() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    prueba.letra('e');
    prueba.tecla(KeyCode::Home);
    for _ in 0..40 {
        prueba.tecla(KeyCode::Delete);
    }
    escribir(&mut prueba, "relativa/api");
    prueba.tecla(KeyCode::Enter);
    assert!(dialogo_abierto(&prueba));
    assert!(
        prueba
            .texto()
            .contains("la carpeta debe ser una ruta absoluta"),
        "{}",
        prueba.texto()
    );
    let ficha = prueba.ficha("web-01", "agente-api").expect("ficha");
    assert_eq!(
        ficha.carpeta.as_deref(),
        Some("~/proyectos/api"),
        "no se guardó"
    );
    prueba.tecla(KeyCode::Esc);
    assert!(!dialogo_abierto(&prueba));
}

#[test]
fn e_sobre_una_sin_ficha_dice_como_anadirla() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("portatil-3");
    prueba.letra('e');
    assert!(!dialogo_abierto(&prueba));
    assert!(prueba.mensaje().contains("pulsa a"), "{}", prueba.mensaje());
}

// -------------------------------------------------------- renombrar

fn renombrar_a(prueba: &mut Prueba, nuevo: &str) -> Option<u64> {
    prueba.letra(',');
    prueba.tecla(KeyCode::Home);
    for _ in 0..70 {
        prueba.tecla(KeyCode::Delete);
    }
    escribir(prueba, nuevo);
    prueba.tecla(KeyCode::Enter);
    prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::RenombrarTmux { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
}

#[test]
fn renombrar_solo_cambia_la_ficha_cuando_el_host_dijo_ok() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra(',');
    assert!(
        prueba.texto().contains("Nombre nuevo"),
        "{}",
        prueba.texto()
    );
    prueba.instantaneas("renombrar");
    prueba.tecla(KeyCode::Esc);
    let peticion = renombrar_a(&mut prueba, "agente-pagos").expect("RenombrarTmux");
    assert!(peticion >= 1 << 49);
    // Mientras el host no responde, la ficha sigue con su nombre.
    assert!(prueba.ficha("web-01", "agente-api").is_some());
    assert!(prueba.ficha("web-01", "agente-pagos").is_none());
    prueba.servidor(MensajeServidor::TmuxRenombrada {
        peticion_id: peticion,
        resultado: ResultadoRenombrar::Ok,
    });
    assert!(prueba.ficha("web-01", "agente-api").is_none());
    let renombrada = prueba
        .ficha("web-01", "agente-pagos")
        .expect("ficha renombrada");
    assert_eq!(renombrada.host_id, web);
    assert_eq!(
        renombrada.comando.as_deref(),
        Some("claude"),
        "lo demás no cambia"
    );
    // Y se vuelve a consultar para ver el estado nuevo.
    assert_eq!(consultas(&prueba.drenar()).len(), 1);
    // Una respuesta repetida o tardía no hace nada.
    prueba.servidor(MensajeServidor::TmuxRenombrada {
        peticion_id: peticion,
        resultado: ResultadoRenombrar::Ok,
    });
    assert_eq!(prueba.fichas().len(), 5);
}

#[test]
fn renombrar_con_ocupado_error_o_sin_respuesta_no_toca_la_ficha() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    let peticion = renombrar_a(&mut prueba, "otro").expect("RenombrarTmux");
    prueba.servidor(MensajeServidor::TmuxRenombrada {
        peticion_id: peticion,
        resultado: ResultadoRenombrar::Ocupado,
    });
    assert!(
        prueba
            .mensaje()
            .contains("ya hay otra sesión con el nombre «otro»"),
        "{}",
        prueba.mensaje()
    );
    assert!(prueba.ficha("web-01", "agente-api").is_some());
    let peticion = renombrar_a(&mut prueba, "otro2").expect("RenombrarTmux");
    prueba.servidor(MensajeServidor::TmuxRenombrada {
        peticion_id: peticion,
        resultado: ResultadoRenombrar::Error("tmux demasiado antiguo".to_string()),
    });
    assert!(
        prueba.mensaje().contains("tmux demasiado antiguo"),
        "{}",
        prueba.mensaje()
    );
    assert!(prueba.ficha("web-01", "agente-api").is_some());
    // Un `Error` del servidor a la petición también lo dice.
    let peticion = renombrar_a(&mut prueba, "otro3").expect("RenombrarTmux");
    prueba.servidor(MensajeServidor::Error {
        mensaje: "el host ya no existe".to_string(),
        peticion_id: Some(peticion),
    });
    assert!(
        prueba.mensaje().contains("el host ya no existe"),
        "{}",
        prueba.mensaje()
    );
    assert!(prueba.ficha("web-01", "agente-api").is_some());
    assert!(prueba.ficha("web-01", "otro3").is_none());
}

#[test]
fn renombrar_una_sesion_que_ya_no_existe_cambia_solo_la_ficha_y_lo_dice() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("informes");
    let peticion = renombrar_a(&mut prueba, "informes-viejos").expect("RenombrarTmux");
    prueba.servidor(MensajeServidor::TmuxRenombrada {
        peticion_id: peticion,
        resultado: ResultadoRenombrar::NoExiste,
    });
    assert!(prueba.ficha("dev-01", "informes").is_none());
    assert!(prueba.ficha("dev-01", "informes-viejos").is_some());
    assert!(
        prueba.mensaje().contains("solo se ha renombrado la ficha"),
        "{}",
        prueba.mensaje()
    );
}

#[test]
fn el_nombre_nuevo_se_valida_antes_de_mandar_nada() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    // Con un espacio, no vale.
    assert!(renombrar_a(&mut prueba, "dos palabras").is_none());
    assert!(dialogo_abierto(&prueba));
    assert!(prueba.texto().contains("espacios"), "{}", prueba.texto());
    // El mismo nombre, tampoco.
    prueba.tecla(KeyCode::Home);
    for _ in 0..30 {
        prueba.tecla(KeyCode::Delete);
    }
    escribir(&mut prueba, "agente-api");
    prueba.tecla(KeyCode::Enter);
    assert!(
        prueba.texto().contains("es el mismo nombre"),
        "{}",
        prueba.texto()
    );
    // El de otra ficha del mismo host, tampoco (dev-01 tiene «docs»; web-01 no).
    prueba.tecla(KeyCode::Esc);
    prueba.ir_a("refactor");
    assert!(renombrar_a(&mut prueba, "docs").is_none());
    assert!(
        prueba
            .texto()
            .contains("ya hay otra sesión con ese nombre en dev-01"),
        "{}",
        prueba.texto()
    );
    assert!(prueba.drenar().is_empty());
}

#[test]
fn las_del_tmux_habitual_no_se_renombran() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("trabajo");
    prueba.letra(',');
    assert!(!dialogo_abierto(&prueba));
    assert!(
        prueba.mensaje().contains("no se renombran"),
        "{}",
        prueba.mensaje()
    );
}

// ---------------------------------------------------------- terminar

#[test]
fn t_mayuscula_pide_confirmacion_que_nombra_la_sesion_y_el_host() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra('T');
    // Todavía no se manda nada.
    assert!(prueba.drenar().is_empty());
    let texto = prueba.texto();
    assert!(texto.contains("Terminar «agente-api» en web-01"), "{texto}");
    assert!(texto.contains("Se cerrará lo que corre dentro"), "{texto}");
    prueba.instantaneas("confirmar_terminar");
    // Esc cancela.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.drenar().is_empty());
    // `s` confirma: TerminarTmux al servidor de la fila.
    prueba.letra('T');
    prueba.letra('s');
    let terminar: Vec<(u64, i64, ServidorTmux, String)> = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::TerminarTmux {
                peticion_id,
                host_id,
                servidor,
                nombre,
            } => Some((peticion_id, host_id, servidor, nombre)),
            _ => None,
        })
        .collect();
    assert_eq!(terminar.len(), 1);
    assert_eq!(
        (terminar[0].1, terminar[0].2, terminar[0].3.as_str()),
        (web, ServidorTmux::Magi, "agente-api")
    );
    // Cuando el host responde `ok`, la ficha queda terminada y se vuelve a
    // consultar. `tmux_terminada` lo anota el servidor, no el cliente.
    prueba.servidor(MensajeServidor::TmuxTerminada {
        peticion_id: terminar[0].0,
        servidor: ServidorTmux::Magi,
        resultado: ResultadoTmux::Ok,
    });
    let fila = prueba
        .texto()
        .lines()
        .find(|linea| linea.contains("agente-api"))
        .expect("fila")
        .to_string();
    assert!(fila.contains("terminada"), "{fila}");
    assert_eq!(consultas(&prueba.drenar()).len(), 1);
    assert!(prueba.registro("tmux_terminada").is_empty());
    // La ficha sigue en la base de datos: terminar no borra la ficha.
    assert!(prueba.ficha("web-01", "agente-api").is_some());
    // Cuando el host se vuelve a mirar y dice otra cosa, manda el host.
    let mut lista = prueba.mundo();
    for ficha in &mut lista.fichas {
        ficha.comprobada_en += 100;
    }
    for host in &mut lista.hosts {
        host.comprobada_en += 100;
    }
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    let fila = prueba
        .texto()
        .lines()
        .find(|linea| linea.contains("agente-api"))
        .expect("fila")
        .to_string();
    assert!(fila.contains("libre"), "{fila}");
}

#[test]
fn terminar_una_del_tmux_habitual_va_al_servidor_habitual_y_una_ya_terminada_no_pregunta() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("trabajo");
    prueba.letra('T');
    assert!(
        prueba.texto().contains("tu tmux de siempre"),
        "{}",
        prueba.texto()
    );
    prueba.letra('s');
    assert!(prueba.drenar().iter().any(|mensaje| matches!(
        mensaje,
        MensajeCliente::TerminarTmux { servidor: ServidorTmux::Habitual, nombre, .. } if nombre == "trabajo"
    )));
    prueba.ir_a("informes");
    prueba.letra('T');
    assert!(!matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    assert!(
        prueba.mensaje().contains("ya está terminada"),
        "{}",
        prueba.mensaje()
    );
}

#[test]
fn un_fallo_al_terminar_se_dice_y_no_cambia_la_fila() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    prueba.letra('T');
    prueba.letra('s');
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::TerminarTmux { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("TerminarTmux");
    prueba.servidor(MensajeServidor::Error {
        mensaje: "web-01 no respondió".to_string(),
        peticion_id: Some(peticion),
    });
    assert!(
        prueba
            .mensaje()
            .contains("no se pudo terminar «agente-api» en web-01"),
        "{}",
        prueba.mensaje()
    );
    let fila = prueba
        .texto()
        .lines()
        .find(|linea| linea.contains("agente-api"))
        .expect("fila")
        .to_string();
    assert!(fila.contains("libre"), "{fila}");
}

// ------------------------------------------------------------ borrar

#[test]
fn x_borra_la_ficha_con_confirmacion_y_avisa_si_la_sesion_vive() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    prueba.letra('x');
    let texto = prueba.texto();
    assert!(
        texto.contains("¿Borrar la ficha de «agente-api» en web-01?"),
        "{texto}"
    );
    assert!(
        texto.contains("seguirá viva en web-01, sin ficha"),
        "{texto}"
    );
    prueba.instantaneas("confirmar_borrar_viva");
    // Cancelar no toca nada.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.ficha("web-01", "agente-api").is_some());
    assert!(prueba.registro("sesion_persistente_borrada").is_empty());
    // Confirmar borra la ficha y anota el efecto.
    prueba.letra('x');
    prueba.letra('s');
    assert!(prueba.ficha("web-01", "agente-api").is_none());
    let anotadas = prueba.registro("sesion_persistente_borrada");
    assert_eq!(anotadas.len(), 1);
    assert!(
        anotadas[0].detalle.contains("agente-api")
            && anotadas[0].detalle.contains("la sesión sigue en el host"),
        "{}",
        anotadas[0].detalle
    );
    assert!(!anotadas[0].detalle.contains("claude"), "nada de comandos");
    // Borrar la ficha nunca termina la sesión: no se manda TerminarTmux.
    let enviados = prueba.drenar();
    assert!(!enviados
        .iter()
        .any(|m| matches!(m, MensajeCliente::TerminarTmux { .. })));
    assert_eq!(
        consultas(&enviados).len(),
        1,
        "se consulta para ver el estado nuevo"
    );
}

#[test]
fn borrar_la_ficha_de_una_terminada_no_dice_que_siga_viva() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("informes");
    prueba.letra('x');
    let texto = prueba.texto();
    assert!(texto.contains("ya no existe en el host"), "{texto}");
    assert!(!texto.contains("seguirá viva"), "{texto}");
    prueba.letra('s');
    let anotadas = prueba.registro("sesion_persistente_borrada");
    assert_eq!(anotadas.len(), 1);
    assert!(
        !anotadas[0].detalle.contains("sigue en el host"),
        "{}",
        anotadas[0].detalle
    );
}

#[test]
fn x_sobre_una_sin_ficha_no_pregunta_y_la_confirmacion_lleva_los_datos_fijados() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("portatil-3");
    prueba.letra('x');
    assert!(!matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    // La confirmación abierta actúa sobre su ficha aunque la lista cambie: un
    // refresco que reordena filas no cambia lo que se borra.
    prueba.ir_a("docs");
    prueba.letra('x');
    let nueva = prueba.sembrar("dev-01", "aaa-primera");
    let mut lista = prueba.mundo();
    lista.fichas.push(FichaVista {
        uuid: nueva,
        host_id: prueba.host_id("dev-01"),
        nombre: "aaa-primera".to_string(),
        estado: EstadoFicha::VivaLibre,
        sesion: Some(sesion("aaa-primera", 0, None, AHORA_HOST, false, None)),
        comprobada_en: chrono::Utc::now().timestamp(),
    });
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    prueba.letra('s');
    assert!(prueba.ficha("dev-01", "docs").is_none());
    assert!(prueba.ficha("dev-01", "aaa-primera").is_some());
    assert!(prueba.ficha("dev-01", "refactor").is_some());
}

#[test]
fn borrar_una_ficha_que_otro_equipo_ya_borro_lo_dice_y_no_anota() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    prueba.letra('x');
    // Mientras se pregunta, la sincronización borra la ficha.
    let ficha = prueba.ficha("web-01", "agente-api").expect("ficha");
    magi::almacen::sesiones_persistentes::borrar(prueba.app.almacen.conexion(), ficha.id)
        .expect("borrado por otro equipo");
    prueba.letra('s');
    assert!(
        prueba.mensaje().contains("ya no existe"),
        "{}",
        prueba.mensaje()
    );
    assert!(prueba.registro("sesion_persistente_borrada").is_empty());
}

// ---------------------------------------------------------- limpiar

#[test]
fn c_mayuscula_borra_las_fichas_terminadas_con_recuento_y_solo_esas() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.entrar_en_sesiones();
    prueba.sembrar("dev-01", "vieja-1");
    prueba.sembrar("dev-01", "vieja-2");
    prueba.sembrar("dev-01", "viva");
    prueba.sembrar("web-01", "vieja-3");
    let ahora = chrono::Utc::now().timestamp();
    let mut fichas = Vec::new();
    for (host, nombre, estado) in [
        ("dev-01", "vieja-1", EstadoFicha::Terminada),
        ("dev-01", "vieja-2", EstadoFicha::Terminada),
        ("dev-01", "viva", EstadoFicha::VivaLibre),
        ("web-01", "vieja-3", EstadoFicha::Terminada),
    ] {
        let uuid = prueba.ficha(host, nombre).expect("ficha").uuid;
        fichas.push(FichaVista {
            uuid,
            host_id: prueba.host_id(host),
            nombre: nombre.to_string(),
            estado,
            sesion: (estado == EstadoFicha::VivaLibre)
                .then(|| sesion(nombre, 0, None, AHORA_HOST, false, None)),
            comprobada_en: ahora,
        });
    }
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes {
            fichas,
            hosts: Vec::new(),
        },
    });
    prueba.drenar();
    prueba.letra('C');
    let texto = prueba.texto();
    assert!(texto.contains("Borrar 3 fichas terminadas"), "{texto}");
    prueba.instantaneas("confirmar_limpiar");
    prueba.tecla(KeyCode::Esc);
    assert_eq!(prueba.fichas().len(), 4, "cancelar no borra nada");
    prueba.letra('C');
    prueba.letra('s');
    let nombres: Vec<String> = prueba
        .fichas()
        .into_iter()
        .map(|ficha| ficha.nombre)
        .collect();
    assert_eq!(nombres, vec!["viva".to_string()]);
    assert_eq!(prueba.registro("sesion_persistente_borrada").len(), 3);
    assert!(
        prueba.mensaje().contains("3 fichas terminadas borradas"),
        "{}",
        prueba.mensaje()
    );
}

#[test]
fn c_sin_terminadas_no_pregunta_y_con_una_lista_vieja_no_borra() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    // En el mundo hay una terminada («informes»): se pregunta.
    prueba.letra('C');
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    prueba.tecla(KeyCode::Esc);
    // La misma lista, pero de hace 50 minutos (la de una ventana que se fue):
    // no se borra nada y se dice qué hacer.
    let mut lista = prueba.mundo();
    let vieja = chrono::Utc::now().timestamp() - 50 * 60;
    for ficha in &mut lista.fichas {
        ficha.comprobada_en = vieja;
    }
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    prueba.letra('C');
    assert!(!matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    assert!(
        prueba.mensaje().contains("pulsa r y vuelve a pulsar C"),
        "{}",
        prueba.mensaje()
    );
    assert_eq!(prueba.fichas().len(), 5);
}

// ------------------------------------------------------------ añadir

#[test]
fn a_anade_la_sesion_sin_ficha_con_origen_descubierta_y_lo_anota() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("portatil-3");
    prueba.letra('a');
    let ficha = prueba.ficha("web-01", "portatil-3").expect("ficha añadida");
    assert_eq!(ficha.origen, OrigenFicha::Descubierta);
    assert!(ficha.comando.is_none() && ficha.carpeta.is_none());
    let anotadas = prueba.registro("sesion_persistente_creada");
    assert_eq!(anotadas.len(), 1);
    assert!(
        anotadas[0].detalle.contains("portatil-3") && anotadas[0].detalle.contains("descubierta")
    );
    assert_eq!(consultas(&prueba.drenar()).len(), 1);
    // `a` sobre una con ficha o del tmux habitual no hace nada.
    prueba.ir_a("agente-api");
    prueba.letra('a');
    assert_eq!(prueba.fichas().len(), 6);
    prueba.ir_a("trabajo");
    prueba.letra('a');
    assert_eq!(prueba.fichas().len(), 6);
}

// ------------------------------------------------------------ filtrar

#[test]
fn barra_filtra_por_nombre_host_y_titulo_y_esc_lo_quita() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    assert_eq!(prueba.app.filas_persistentes().len(), 7);
    prueba.letra('/');
    escribir(&mut prueba, "claude");
    let filas = prueba.app.filas_persistentes();
    assert_eq!(
        filas.len(),
        3,
        "agente-api, refactor y docs llevan «Claude» en el título"
    );
    assert!(prueba.texto().contains("/ claude"), "{}", prueba.texto());
    // `↵` deja el filtro puesto y devuelve las teclas al bloque.
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.filas_persistentes().len(), 3);
    prueba.letra('j');
    // `Esc` lo quita sin salir de la vista.
    prueba.tecla(KeyCode::Esc);
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    assert_eq!(prueba.app.filas_persistentes().len(), 7);
    // Sin coincidencias lo dice.
    prueba.letra('/');
    escribir(&mut prueba, "zzz");
    assert!(
        prueba.texto().contains("ninguna coincide con «zzz»"),
        "{}",
        prueba.texto()
    );
    prueba.tecla(KeyCode::Esc);
    assert_eq!(prueba.app.filas_persistentes().len(), 7);
}

#[test]
fn las_teclas_con_ctrl_o_alt_no_son_del_bloque() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    for letra in ['x', 'T', 'C', 'a', 'e', 'v', 'n'] {
        prueba.tecla_con(KeyCode::Char(letra), KeyModifiers::CONTROL);
        prueba.tecla_con(KeyCode::Char(letra), KeyModifiers::ALT);
    }
    assert!(prueba.app.dialogo.is_none(), "abrió un diálogo");
    assert!(prueba.drenar().is_empty(), "mandó algo al servidor");
    assert_eq!(prueba.fichas().len(), 5);
}

// --------------------------------------------- contrato con la pista B

#[test]
fn las_sesiones_con_atencion_salen_de_la_lista_del_servidor() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let atencion = prueba.app.persistentes_con_atencion();
    assert_eq!(atencion.len(), 1);
    assert_eq!(
        (
            atencion[0].host_nombre.as_str(),
            atencion[0].nombre.as_str()
        ),
        ("web-01", "agente-api")
    );
}

#[test]
fn el_texto_de_la_lista_no_va_a_registro_ni_se_anota_nada_al_solo_mirarla() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    prueba.letra('j');
    prueba.letra('k');
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    let registro = prueba
        .app
        .almacen
        .listar_registro(&FiltroRegistro::default(), 500, 0)
        .expect("registro");
    assert!(
        registro
            .iter()
            .all(|entrada| !entrada.tipo.starts_with("sesion_persistente")),
        "{registro:?}"
    );
    assert!(registro
        .iter()
        .all(|entrada| !entrada.detalle.contains("Claude Code")));
}

// ------------------------------- revisión adversarial de la pista A1 (S3)

#[test]
fn sin_tocar_nada_la_seleccion_sigue_a_su_fila_cuando_la_lista_se_refresca() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    // Nadie ha movido la selección: es la primera fila, «docs».
    let elegida = prueba.app.fila_persistente_elegida().expect("fila elegida");
    assert_eq!(elegida.nombre, "docs");
    // Llega un refresco con una ficha nueva que se ordena por delante: lo que
    // hay bajo `↵` sigue siendo «docs» (si no, un `↵` se engancharía a otra
    // sesión, soltando a quien la tenga).
    let nueva = prueba.sembrar("dev-01", "aaa-primera");
    let mut lista = prueba.mundo();
    lista.fichas.push(FichaVista {
        uuid: nueva,
        host_id: prueba.host_id("dev-01"),
        nombre: "aaa-primera".to_string(),
        estado: EstadoFicha::VivaLibre,
        sesion: Some(sesion("aaa-primera", 0, None, AHORA_HOST - 5, false, None)),
        comprobada_en: chrono::Utc::now().timestamp(),
    });
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    assert_eq!(
        prueba.app.filas_persistentes()[0].nombre,
        "aaa-primera",
        "la fila nueva va la primera"
    );
    let elegida = prueba.app.fila_persistente_elegida().expect("fila elegida");
    assert_eq!(elegida.nombre, "docs", "la selección se quedó en el índice");
    prueba.drenar();
    prueba.tecla(KeyCode::Enter);
    let enviadas = aperturas(&prueba.drenar());
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1.nombre, "docs");
}

#[test]
fn a_con_una_lista_vieja_no_guarda_ninguna_ficha() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.entrar_en_sesiones();
    // La lista que había al irse la última ventana: de hace 50 minutos. La
    // sesión «sin ficha» pudo terminar desde entonces.
    let mut lista = prueba.mundo();
    let vieja = chrono::Utc::now().timestamp() - 50 * 60;
    for ficha in &mut lista.fichas {
        ficha.comprobada_en = vieja;
    }
    for host in &mut lista.hosts {
        host.comprobada_en = vieja;
    }
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    prueba.drenar();
    prueba.ir_a("portatil-3");
    let antes = prueba.fichas().len();
    prueba.letra('a');
    assert_eq!(prueba.fichas().len(), antes, "no se guardó ninguna ficha");
    assert!(prueba.ficha("web-01", "portatil-3").is_none());
    assert!(
        prueba.mensaje().contains("pulsa r y vuelve a pulsar a"),
        "{}",
        prueba.mensaje()
    );
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    assert!(consultas(&prueba.drenar()).is_empty());
    // Con la comprobación fresca, sí.
    let mut lista = prueba.mundo();
    for host in &mut lista.hosts {
        host.comprobada_en = chrono::Utc::now().timestamp();
    }
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    prueba.ir_a("portatil-3");
    prueba.letra('a');
    assert!(prueba.ficha("web-01", "portatil-3").is_some());
}

#[test]
fn el_pie_de_la_vista_previa_con_error_no_dice_que_leyo_las_ultimas_lineas() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.letra('v');
    let peticion = peticion_de_vista(&mut prueba);
    // El servidor pone la hora también en las respuestas con error.
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: peticion,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: Vec::new(),
        leida_en: AHORA_HOST,
        error: Some("web-01 no respondió en 5 s".to_string()),
    });
    let texto = prueba.texto();
    assert!(texto.contains("no se pudo leer"), "{texto}");
    assert!(texto.contains("sin lectura"), "{texto}");
    assert!(!texto.contains("últimas 20 líneas"), "{texto}");
}

#[test]
fn una_apertura_que_el_servidor_no_pudo_hacer_no_deja_un_enganche_esperando() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    let web = prueba.host_id("web-01");
    prueba.ir_a("agente-api");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(aperturas(&prueba.drenar()).len(), 1);
    prueba.servidor(MensajeServidor::AperturaTmuxFallida {
        host_id: web,
        nombre: "agente-api".to_string(),
        causa: CausaAperturaTmux::Conexion,
        mensaje: "web-01 no responde".to_string(),
    });
    assert!(prueba.app.persistentes.creaciones.is_empty());
    // Otra pestaña con el mismo host y nombre (por ejemplo, de otra ventana)
    // no es el efecto de lo que se pidió: no se anota nada.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![pestana_persistente(7, web, "web-01", "agente-api")],
    });
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());
}

#[test]
fn c_dice_cuantas_fichas_no_se_pudieron_borrar() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.entrar_en_sesiones();
    for nombre in ["vieja-1", "vieja-2", "vieja-3"] {
        prueba.sembrar("dev-01", nombre);
    }
    let ahora = chrono::Utc::now().timestamp();
    let dev = prueba.host_id("dev-01");
    let fichas = ["vieja-1", "vieja-2", "vieja-3"]
        .into_iter()
        .map(|nombre| FichaVista {
            uuid: prueba.ficha("dev-01", nombre).expect("ficha").uuid,
            host_id: dev,
            nombre: nombre.to_string(),
            estado: EstadoFicha::Terminada,
            sesion: None,
            comprobada_en: ahora,
        })
        .collect();
    prueba.servidor(MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes {
            fichas,
            hosts: Vec::new(),
        },
    });
    prueba.drenar();
    prueba.letra('C');
    assert!(prueba.texto().contains("Borrar 3 fichas terminadas"));
    // Mientras se pregunta, otro equipo borra una de las tres.
    let ficha = prueba.ficha("dev-01", "vieja-2").expect("ficha");
    magi::almacen::sesiones_persistentes::borrar(prueba.app.almacen.conexion(), ficha.id)
        .expect("borrada por otro equipo");
    prueba.letra('s');
    assert!(prueba.fichas().is_empty());
    let mensaje = prueba.mensaje();
    assert!(
        mensaje.contains("2 fichas terminadas borradas"),
        "{mensaje}"
    );
    assert!(mensaje.contains("1 no se pudo borrar"), "{mensaje}");
    assert_eq!(prueba.registro("sesion_persistente_borrada").len(), 2);
}

#[test]
fn con_el_servidor_caido_la_lista_y_los_avisos_no_se_quedan_como_si_fueran_actuales() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    let web = prueba.host_id("web-01");
    prueba.servidor(bienvenida(vec![pestana_de(1, web, "web-01")]));
    prueba.tecla(KeyCode::F(3));
    let lista = prueba.mundo();
    prueba.servidor(MensajeServidor::SesionesPersistentes { lista });
    prueba.drenar();
    assert_eq!(prueba.app.persistentes_con_atencion().len(), 1);
    // Una terminación en vuelo y una consulta sin respuesta: no van a llegar.
    prueba.ir_a("agente-api");
    prueba.letra('T');
    prueba.letra('s');
    assert!(!prueba.app.persistentes.peticiones.is_empty());
    prueba.app.procesar(Evento::ServidorCaido);
    prueba.paso();
    assert!(prueba.app.persistentes.peticiones.is_empty());
    assert!(prueba.app.persistentes.consulta.is_none());
    // «Seguir sin sesiones»: la vista no enseña filas ni avisos del servidor
    // que se fue, y `↵` y compañía no hacen como si hubiera alguien al otro
    // lado.
    prueba.letra('n');
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.app.filas_persistentes().is_empty());
    assert!(prueba.app.persistentes_con_atencion().is_empty());
    assert!(
        !prueba.texto().contains("pide atención"),
        "{}",
        prueba.texto()
    );
    prueba.drenar();
    prueba.tecla(KeyCode::Enter);
    assert!(aperturas(&prueba.drenar()).is_empty());
    assert!(
        !prueba.mensaje().contains("enganchándose"),
        "{}",
        prueba.mensaje()
    );
    assert!(prueba.app.aperturas_pendientes.is_empty());
}

#[test]
fn al_reconectar_una_vista_previa_sin_respuesta_no_se_queda_leyendo_para_siempre() {
    let mut prueba = Prueba::nueva(&["web-01", "dev-01"]);
    prueba.con_el_mundo();
    prueba.ir_a("agente-api");
    prueba.letra('v');
    assert!(prueba.texto().contains("leyendo…"));
    // Otra `Bienvenida` (el cliente volvió a conectar): la petición se perdió.
    prueba.servidor(bienvenida(Vec::new()));
    let texto = prueba.texto();
    assert!(!texto.contains("leyendo…"), "{texto}");
    assert!(texto.contains("se perdió la conexión"), "{texto}");
    // `r` la vuelve a pedir y esa sí se atiende.
    prueba.drenar();
    prueba.letra('r');
    let peticion = peticion_de_vista(&mut prueba);
    let web = prueba.host_id("web-01");
    prueba.servidor(MensajeServidor::VistaPrevia {
        peticion_id: peticion,
        host_id: web,
        nombre: "agente-api".to_string(),
        lineas: lineas_de_vista(&["listo"]),
        leida_en: AHORA_HOST,
        error: None,
    });
    assert!(prueba.texto().contains("listo"));
}
