//! Pruebas de la interfaz de las sesiones persistentes: bloque SESIONES de la
//! ficha del host (Fase 16, pista A3).
//!
//! `App::de_prueba` (sin hilos, red ni servidor) y el bucle real sobre un
//! `TestBackend`. Cubren la casilla «Todas las sesiones persistentes» al crear
//! y al editar, que no sale en los hosts de escritorio, que la marca viaja
//! (sube la versión del host) mientras que los ajustes del bloque TERMINAL
//! siguen siendo propios del equipo, que nada escribe ya `tmux_persistente`,
//! el aviso de tmux dentro de tmux (R143), el recorrido de foco y las
//! instantáneas a 40×12, 80×24 y 200×60. Las de pintado corren con `TZ=UTC`.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::{comando_arranca_tmux, App, CampoFicha, Evento, ORDEN_CAMPOS};
use magi::config::{Config, Rutas};
use magi::escritorio::{OpcionesEscritorio, TipoHost};
use magi::modelo::{DatosHost, IdentidadRef, Origen};
use magi::protocolo::MensajeCliente;
use magi::snippets::{DatosSnippet, Destino};
use magi::tema::{Glifos, Tema};
use magi::ui::Vista;

/// Nombre de este equipo en las pruebas: la etiqueta de los nombres
/// automáticos sale de él (`sobremesa`).
const EQUIPO: &str = "Sobremesa";

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magisp")
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
        // Sin exportar a `~/.ssh/config`: la prueba no tiene `.ssh`.
        config: Config {
            exportar_al_guardar: false,
            ..Config::default()
        },
        _temporal: temporal,
    }
}

struct Prueba {
    app: App,
    terminal: Terminal<TestBackend>,
    _enviados: tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
    _entorno: Entorno,
}

/// Un host SSH de documentación.
fn host_ssh(nombre: &str, direccion: &str) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: direccion.to_string(),
        ..DatosHost::default()
    }
}

impl Prueba {
    /// Con un host SSH, `base-01`, y este equipo llamado «Sobremesa».
    fn nueva() -> Self {
        Self::con_inventario(80, 24, Tema::respaldo(), |almacen| {
            almacen
                .crear_host(&host_ssh("base-01", "192.0.2.10"), Origen::Manual)
                .expect("host de prueba");
        })
    }

    fn con_inventario(
        cols: u16,
        filas: u16,
        tema: Tema,
        tocar: impl FnOnce(&magi::almacen::Almacen),
    ) -> Self {
        let entorno = entorno();
        {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
            let este = magi::almacen::equipos::este(almacen.conexion()).expect("este equipo");
            magi::almacen::equipos::renombrar(almacen.conexion(), este.id, EQUIPO)
                .expect("renombrar el equipo");
            tocar(&almacen);
        }
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (app, enviados) =
            App::de_prueba(entorno.rutas.clone(), entorno.config.clone(), tema, almacen)
                .expect("app de prueba");
        let terminal = Terminal::new(TestBackend::new(cols, filas)).expect("terminal");
        let mut prueba = Self {
            app,
            terminal,
            _enviados: enviados,
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

    fn paso(&mut self) {
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn escribir(&mut self, texto: &str) {
        for caracter in texto.chars() {
            self.tecla(KeyCode::Char(caracter));
        }
    }

    fn guardar(&mut self) {
        self.tecla_con(KeyCode::Char('s'), KeyModifiers::CONTROL);
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

    /// F2 (Hosts) y `e` sobre el host: la ficha para editarlo.
    fn editar(&mut self, nombre: &str) {
        let id = self.host_id(nombre);
        self.tecla(KeyCode::F(2));
        self.app.seleccionar_host_id(id);
        self.tecla(KeyCode::Char('e'));
        assert_eq!(self.app.vista, Vista::Ficha);
        assert_eq!(self.ficha().host_id, Some(id));
    }

    /// F2 (Hosts) y `n`: la ficha de un host nuevo.
    fn nuevo(&mut self) {
        self.tecla(KeyCode::F(2));
        self.tecla(KeyCode::Char('n'));
        assert_eq!(self.app.vista, Vista::Ficha);
        assert_eq!(self.ficha().host_id, None);
    }

    fn ficha(&self) -> &magi::app::Ficha {
        self.app.ficha.as_ref().expect("ficha abierta")
    }

    /// Lleva el foco a `campo` con Tab.
    fn ir_a(&mut self, campo: CampoFicha) {
        for _ in 0..=ORDEN_CAMPOS.len() + 1 {
            if self.ficha().campo == campo {
                return;
            }
            self.tecla(KeyCode::Tab);
        }
        panic!("no se llega a {campo:?} con Tab");
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

    /// Versión de sincronización del host, tal como está en `magi.db`.
    fn version(&self, id: i64) -> String {
        self.app
            .almacen
            .conexion()
            .query_row("SELECT version FROM HOSTS WHERE id = ?1", [id], |fila| {
                fila.get::<_, String>(0)
            })
            .expect("versión del host")
    }

    /// Lo que hay guardado de las tres marcas del host:
    /// (sesiones_persistentes, reconectar, portapapeles_remoto, tmux_persistente).
    fn marcas(&self, id: i64) -> (bool, bool, bool, bool) {
        let host = self.app.almacen.obtener_host(id).expect("host");
        (
            host.sesiones_persistentes,
            host.reconectar,
            host.portapapeles_remoto,
            host.tmux_persistente,
        )
    }

    fn instantanea(&self, nombre: &str) {
        let texto = self.texto();
        let mut ajustes = insta::Settings::clone_current();
        ajustes.set_snapshot_path("../snapshots");
        ajustes.set_omit_expression(true);
        ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
    }

    /// Instantáneas a 40×12, 80×24 y 200×60; vuelve a 80×24.
    fn instantaneas(&mut self, nombre: &str) {
        for (cols, filas) in [(40, 12), (80, 24), (200, 60)] {
            self.redimensionar(cols, filas);
            self.instantanea(&format!("{nombre}_{cols}x{filas}"));
        }
        self.redimensionar(80, 24);
    }
}

/// Siembra un host SSH con un snippet al conectar con ese comando.
fn con_snippet_al_conectar(comando: &'static str) -> impl FnOnce(&magi::almacen::Almacen) {
    move |almacen| {
        let host = almacen
            .crear_host(&host_ssh("base-01", "192.0.2.10"), Origen::Manual)
            .expect("host de prueba");
        let snippet = almacen
            .crear_snippet(&DatosSnippet {
                nombre: "arranque".to_string(),
                comando: comando.to_string(),
                destinos: vec![Destino::Etiqueta("web".to_string())],
                ..DatosSnippet::default()
            })
            .expect("snippet de prueba");
        almacen
            .fijar_snippet_al_conectar(host, Some(snippet))
            .expect("snippet al conectar");
    }
}

// ------------------------------------------------------------ la casilla

#[test]
fn la_casilla_sale_al_editar_un_host_ssh_con_su_bloque() {
    let mut prueba = Prueba::nueva();
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    let texto = prueba.texto();
    assert!(texto.contains("SESIONES (viaja a tus equipos)"), "{texto}");
    assert!(
        texto.contains("[ ] Todas las sesiones persistentes (tmux en el host)"),
        "{texto}"
    );
    assert!(
        texto.contains("nombres automáticos: sobremesa-1, sobremesa-2…"),
        "{texto}"
    );
    assert!(!prueba.ficha().sesiones_persistentes);
    // Espacio la marca y la desmarca.
    prueba.tecla(KeyCode::Char(' '));
    assert!(prueba.ficha().sesiones_persistentes);
    assert!(prueba
        .texto()
        .contains("[x] Todas las sesiones persistentes"));
    prueba.tecla(KeyCode::Char(' '));
    assert!(!prueba.ficha().sesiones_persistentes);
}

#[test]
fn la_casilla_sale_al_crear_un_host_y_se_guarda_con_el() {
    let mut prueba = Prueba::nueva();
    prueba.nuevo();
    // Un host nuevo empieza por el selector de tipo (SSH).
    assert_eq!(prueba.ficha().tipo, TipoHost::Ssh);
    assert!(prueba
        .ficha()
        .orden_campos()
        .contains(&CampoFicha::SesionesPersistentes));
    prueba.ir_a(CampoFicha::Nombre);
    prueba.escribir("web-nuevo");
    prueba.ir_a(CampoFicha::Direccion);
    prueba.escribir("192.0.2.20");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    assert!(prueba.texto().contains("SESIONES (viaja a tus equipos)"));
    prueba.tecla(KeyCode::Char(' '));
    prueba.guardar();
    assert_eq!(prueba.app.vista, Vista::Hosts, "{:?}", prueba.app.mensaje);
    let id = prueba.host_id("web-nuevo");
    let (sesiones, reconectar, portapapeles, tmux) = prueba.marcas(id);
    assert!(sesiones, "la marca se guarda al crear");
    assert!(reconectar && !portapapeles, "lo demás, por defecto");
    assert!(!tmux, "tmux_persistente no se escribe");
}

#[test]
fn un_host_marcado_enseña_la_casilla_marcada_al_editarlo() {
    let mut prueba = Prueba::con_inventario(80, 24, Tema::respaldo(), |almacen| {
        let id = almacen
            .crear_host(&host_ssh("base-01", "192.0.2.10"), Origen::Manual)
            .expect("host");
        magi::almacen::hosts::fijar_sesiones_persistentes(almacen.conexion(), id, true)
            .expect("marca");
    });
    prueba.editar("base-01");
    assert!(prueba.ficha().sesiones_persistentes);
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    assert!(prueba
        .texto()
        .contains("[x] Todas las sesiones persistentes"));
    // Sin tocar nada, la ficha no está sucia (la marca ya venía así).
    assert!(!prueba.ficha().sucio());
}

// -------------------------------------------- solo en hosts SSH (no rdp...)

fn con_escritorios(almacen: &magi::almacen::Almacen) {
    let base = almacen
        .crear_host(&host_ssh("base-01", "192.0.2.10"), Origen::Manual)
        .expect("host ssh");
    let escritorio = |nombre: &str, direccion: &str, tipo: TipoHost, puerto: u16| DatosHost {
        nombre: nombre.to_string(),
        direccion: direccion.to_string(),
        puerto,
        tipo,
        usuario: (tipo == TipoHost::Rdp).then(|| "usuario".to_string()),
        identidad_ref: if tipo == TipoHost::Rdp {
            IdentidadRef::Contrasena
        } else {
            IdentidadRef::Auto
        },
        salto_host_id: (tipo == TipoHost::Voxl).then_some(base),
        escritorio_opciones: OpcionesEscritorio::default(),
        ..DatosHost::default()
    };
    almacen
        .crear_host(
            &escritorio("win-01", "192.0.2.30", TipoHost::Rdp, 3389),
            Origen::Manual,
        )
        .expect("rdp");
    almacen
        .crear_host(
            &escritorio("vnc-01", "192.0.2.31", TipoHost::Vnc, 5900),
            Origen::Manual,
        )
        .expect("vnc");
    almacen
        .crear_host(
            &escritorio("voxl-01", "127.0.0.1", TipoHost::Voxl, 22),
            Origen::Manual,
        )
        .expect("voxl");
}

#[test]
fn la_casilla_no_sale_en_hosts_rdp_vnc_ni_voxl() {
    for (nombre, tipo) in [
        ("win-01", TipoHost::Rdp),
        ("vnc-01", TipoHost::Vnc),
        ("voxl-01", TipoHost::Voxl),
    ] {
        let mut prueba = Prueba::con_inventario(120, 40, Tema::respaldo(), con_escritorios);
        prueba.editar(nombre);
        assert_eq!(prueba.ficha().tipo, tipo);
        for campo in [
            CampoFicha::SesionesPersistentes,
            CampoFicha::Reconectar,
            CampoFicha::PortapapelesRemoto,
        ] {
            assert!(
                !prueba.ficha().orden_campos().contains(&campo),
                "{nombre}: {campo:?} en el recorrido de foco"
            );
        }
        let texto = prueba.texto();
        assert!(!texto.contains("SESIONES"), "{nombre}: {texto}");
        assert!(!texto.contains("Todas las sesiones"), "{nombre}: {texto}");
        // Ningún Tab llega a la casilla.
        for _ in 0..30 {
            prueba.tecla(KeyCode::Tab);
            assert_ne!(prueba.ficha().campo, CampoFicha::SesionesPersistentes);
        }
    }
}

#[test]
fn al_crear_un_escritorio_la_marca_no_sale_ni_se_escribe() {
    let mut prueba = Prueba::nueva();
    prueba.nuevo();
    // Marca la casilla como SSH y cambia después el tipo a RDP: la marca
    // queda en la ficha pero no se ve ni se guarda.
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    prueba.ir_a(CampoFicha::Tipo);
    prueba.tecla(KeyCode::Char(' '));
    assert_eq!(prueba.ficha().tipo, TipoHost::Rdp);
    assert!(!prueba
        .ficha()
        .orden_campos()
        .contains(&CampoFicha::SesionesPersistentes));
    assert!(!prueba.texto().contains("SESIONES"));
    prueba.ir_a(CampoFicha::Nombre);
    prueba.escribir("win-nuevo");
    prueba.ir_a(CampoFicha::Direccion);
    prueba.escribir("192.0.2.40");
    prueba.ir_a(CampoFicha::ContrasenaModo);
    prueba.guardar();
    let mensaje = prueba.app.mensaje.as_ref().map(|m| m.texto.clone());
    assert_eq!(prueba.app.vista, Vista::Hosts, "{mensaje:?}");
    let id = prueba.host_id("win-nuevo");
    assert_eq!(prueba.marcas(id), (false, true, false, false));
    // Sin avisos de error: no se intentó marcar un host que no es SSH.
    assert!(
        !prueba.app.mensaje.as_ref().is_some_and(|m| m.error),
        "{mensaje:?}"
    );
}

// ------------------------------------- la marca viaja; TERMINAL no (T125)

#[test]
fn cambiar_solo_la_marca_sube_la_version_del_host_y_desmarcar_tambien() {
    let mut prueba = Prueba::nueva();
    let id = prueba.host_id("base-01");
    let antes = prueba.version(id);
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    assert!(prueba.ficha().sucio());
    prueba.guardar();
    assert_eq!(prueba.app.vista, Vista::Hosts, "{:?}", prueba.app.mensaje);
    let marcada = prueba.version(id);
    assert_ne!(marcada, antes, "marcar sube la versión: la marca viaja");
    assert_eq!(prueba.marcas(id), (true, true, false, false));
    // Guardar sin cambiar nada no la sube.
    prueba.editar("base-01");
    assert!(prueba.ficha().sesiones_persistentes);
    prueba.guardar();
    assert_eq!(prueba.version(id), marcada, "sin cambios no hay versión");
    // Desmarcar la sube otra vez.
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    prueba.guardar();
    assert_ne!(prueba.version(id), marcada, "desmarcar también viaja");
    assert_eq!(prueba.marcas(id), (false, true, false, false));
}

#[test]
fn cambiar_solo_reconectar_o_el_portapapeles_no_sube_la_version() {
    let mut prueba = Prueba::nueva();
    let id = prueba.host_id("base-01");
    let antes = prueba.version(id);
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::Reconectar);
    prueba.tecla(KeyCode::Char(' '));
    prueba.ir_a(CampoFicha::PortapapelesRemoto);
    prueba.tecla(KeyCode::Char(' '));
    assert!(prueba.ficha().sucio());
    prueba.guardar();
    assert_eq!(prueba.app.vista, Vista::Hosts, "{:?}", prueba.app.mensaje);
    // Propios del equipo: se guardan, pero no viajan ni suben la versión.
    assert_eq!(prueba.marcas(id), (false, false, true, false));
    assert_eq!(
        prueba.version(id),
        antes,
        "reconectar y portapapeles remoto no suben la versión del host"
    );
}

#[test]
fn cada_marca_va_por_su_camino_cuando_se_cambian_las_tres() {
    let mut prueba = Prueba::nueva();
    let id = prueba.host_id("base-01");
    let antes = prueba.version(id);
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    prueba.ir_a(CampoFicha::Reconectar);
    prueba.tecla(KeyCode::Char(' '));
    prueba.ir_a(CampoFicha::PortapapelesRemoto);
    prueba.tecla(KeyCode::Char(' '));
    prueba.guardar();
    assert_eq!(prueba.marcas(id), (true, false, true, false));
    assert_ne!(prueba.version(id), antes);
}

// ----------------------------------------- tmux_persistente ya no se escribe

#[test]
fn nada_escribe_tmux_persistente_desde_la_ficha() {
    // Un host con la marca antigua de la Fase 15 (como lo dejaría una base
    // anterior a la migración 11): la ficha no la lee ni la toca.
    let mut prueba = Prueba::con_inventario(80, 24, Tema::respaldo(), |almacen| {
        let id = almacen
            .crear_host(&host_ssh("base-01", "192.0.2.10"), Origen::Manual)
            .expect("host");
        almacen
            .fijar_tmux_persistente(id, true)
            .expect("marca antigua");
    });
    let id = prueba.host_id("base-01");
    assert_eq!(prueba.marcas(id), (false, true, false, true));
    prueba.editar("base-01");
    assert!(
        !prueba.ficha().sesiones_persistentes,
        "la casilla sale de sesiones_persistentes, no de tmux_persistente"
    );
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    prueba.ir_a(CampoFicha::Reconectar);
    prueba.tecla(KeyCode::Char(' '));
    prueba.guardar();
    assert_eq!(
        prueba.marcas(id),
        (true, false, false, true),
        "tmux_persistente queda como estaba"
    );
    // Y el código del cliente ya no la escribe en ningún sitio.
    for fuente in [
        include_str!("../../src/app.rs"),
        include_str!("../../src/ui/ficha.rs"),
    ] {
        assert!(!fuente.contains("fijar_tmux_persistente"));
    }
}

// ------------------------------------------------- foco y teclado

#[test]
fn el_recorrido_de_foco_pasa_por_la_casilla_antes_del_bloque_terminal() {
    let mut prueba = Prueba::nueva();
    prueba.editar("base-01");
    let orden = prueba.ficha().orden_campos();
    let posicion = |campo| orden.iter().position(|c| *c == campo).unwrap();
    assert_eq!(
        posicion(CampoFicha::Reconectar),
        posicion(CampoFicha::SesionesPersistentes) + 1
    );
    assert_eq!(
        posicion(CampoFicha::PortapapelesRemoto),
        posicion(CampoFicha::Reconectar) + 1
    );
    // Tab hacia delante...
    prueba.ir_a(CampoFicha::Comprobaciones);
    let mut vistos = Vec::new();
    for _ in 0..4 {
        prueba.tecla(KeyCode::Tab);
        vistos.push(prueba.ficha().campo);
    }
    assert_eq!(
        vistos,
        [
            CampoFicha::SesionesPersistentes,
            CampoFicha::Reconectar,
            CampoFicha::PortapapelesRemoto,
            CampoFicha::Nombre,
        ]
    );
    // ...y Mayús+Tab hacia atrás.
    let mut vistos = Vec::new();
    for _ in 0..3 {
        prueba.tecla_con(KeyCode::BackTab, KeyModifiers::SHIFT);
        vistos.push(prueba.ficha().campo);
    }
    assert_eq!(
        vistos,
        [
            CampoFicha::PortapapelesRemoto,
            CampoFicha::Reconectar,
            CampoFicha::SesionesPersistentes,
        ]
    );
}

#[test]
fn el_foco_en_la_casilla_se_ve_a_cualquier_tamano_y_las_letras_no_escriben() {
    for (cols, filas) in [(50, 14), (60, 20), (80, 16), (80, 24), (120, 40), (200, 60)] {
        let mut prueba = Prueba::con_inventario(cols, filas, Tema::respaldo(), |almacen| {
            almacen
                .crear_host(&host_ssh("base-01", "192.0.2.10"), Origen::Manual)
                .expect("host");
        });
        prueba.editar("base-01");
        prueba.ir_a(CampoFicha::SesionesPersistentes);
        let texto = prueba.texto();
        assert!(
            texto.contains("Todas las sesiones persistentes"),
            "{cols}x{filas}: la casilla enfocada no se ve:\n{texto}"
        );
        // La casilla no es un campo de texto: una letra no cambia nada ni
        // abre nada.
        let antes = prueba.ficha().nombre.texto.clone();
        prueba.tecla(KeyCode::Char('z'));
        assert_eq!(prueba.app.vista, Vista::Ficha);
        assert_eq!(prueba.ficha().nombre.texto, antes);
        assert!(!prueba.ficha().sesiones_persistentes);
    }
}

#[test]
fn cerrar_con_la_marca_cambiada_pide_confirmacion() {
    let mut prueba = Prueba::nueva();
    prueba.editar("base-01");
    assert!(!prueba.ficha().sucio());
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    assert!(
        prueba.ficha().sucio(),
        "la marca cuenta como cambio sin guardar"
    );
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_some(), "Esc con cambios pregunta");
    assert!(!prueba.marcas(prueba.host_id("base-01")).0);
}

// ------------------------------------------- aviso de tmux dentro de tmux

#[test]
fn el_aviso_sale_cuando_el_snippet_al_conectar_arranca_tmux() {
    let mut prueba = Prueba::con_inventario(
        120,
        40,
        Tema::respaldo(),
        con_snippet_al_conectar("tmux new -As main"),
    );
    prueba.editar("base-01");
    assert_eq!(prueba.ficha().snippet.arranca_tmux(), Some("arranque"));
    let texto = prueba.texto();
    assert!(
        texto.contains("aviso: el snippet «arranque» arranca tmux"),
        "{texto}"
    );
    assert!(texto.contains("tmux dentro de tmux"), "{texto}");
    // El aviso es del bloque SESIONES: va entre su cabecera y su pie.
    let lineas: Vec<&str> = texto.lines().collect();
    let cabecera = lineas
        .iter()
        .position(|l| l.contains("SESIONES (viaja a tus equipos)"))
        .unwrap();
    let aviso = lineas.iter().position(|l| l.contains("aviso:")).unwrap();
    let terminal = lineas
        .iter()
        .position(|l| l.contains("TERMINAL (solo en este equipo)"))
        .unwrap();
    assert!(cabecera < aviso && aviso < terminal, "{texto}");
}

#[test]
fn sin_snippet_o_con_uno_que_no_arranca_tmux_no_hay_aviso() {
    let mut prueba = Prueba::nueva();
    prueba.editar("base-01");
    assert_eq!(prueba.ficha().snippet.arranca_tmux(), None);
    assert!(!prueba.texto().contains("aviso:"));

    let mut prueba = Prueba::con_inventario(
        120,
        40,
        Tema::respaldo(),
        con_snippet_al_conectar("cd /srv && ls -l"),
    );
    prueba.editar("base-01");
    assert_eq!(prueba.ficha().snippet.arranca_tmux(), None);
    assert!(!prueba.texto().contains("aviso:"));
}

#[test]
fn el_aviso_sigue_al_snippet_elegido_en_la_ficha() {
    // Un host sin snippet al conectar; el desplegable ofrece el que arranca
    // tmux. Al elegirlo sale el aviso, y al quitarlo desaparece.
    let mut prueba = Prueba::con_inventario(120, 40, Tema::respaldo(), |almacen| {
        almacen
            .crear_host(&host_ssh("base-01", "192.0.2.10"), Origen::Manual)
            .expect("host");
        almacen
            .crear_snippet(&DatosSnippet {
                nombre: "arranque".to_string(),
                comando: "exec tmux attach || exec tmux".to_string(),
                destinos: vec![Destino::Etiqueta("web".to_string())],
                ..DatosSnippet::default()
            })
            .expect("snippet");
    });
    prueba.editar("base-01");
    assert!(!prueba.texto().contains("aviso:"));
    prueba.ir_a(CampoFicha::Snippet);
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.ficha().snippet.arranca_tmux(), Some("arranque"));
    assert!(prueba.texto().contains("aviso: el snippet «arranque»"));
    // Elegir «ninguno» lo quita.
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Up);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.ficha().snippet.arranca_tmux(), None);
    assert!(!prueba.texto().contains("aviso:"));
}

#[test]
fn el_aviso_no_impide_marcar_ni_guardar() {
    let mut prueba =
        Prueba::con_inventario(120, 40, Tema::respaldo(), con_snippet_al_conectar("tmux"));
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    prueba.guardar();
    assert_eq!(prueba.app.vista, Vista::Hosts, "{:?}", prueba.app.mensaje);
    assert!(prueba.marcas(prueba.host_id("base-01")).0);
}

#[test]
fn el_criterio_de_tmux_arrancado_distingue_ordenes_de_texto() {
    // Arrancan tmux.
    for comando in [
        "tmux",
        "tmux attach",
        "tmux new -As main",
        "tmux -L trabajo attach",
        "tmux -f /dev/null new",
        "exec tmux",
        "exec tmux attach || exec tmux",
        "/usr/bin/tmux a",
        "TERM=xterm-256color tmux",
        "cd /srv && tmux new -A -s web",
        "sudo tmux attach",
        "env TERM=xterm-256color tmux new",
        "if [ -z \"$TMUX\" ]; then exec tmux; fi",
        "echo hola\ntmux attach",
        "(tmux)",
        "tmux att",
        "tmux new-session -s web",
        // `-d` con `-A` engancha (suelta a los demás); detrás de la orden que
        // corre dentro, `-d` ya no es de tmux.
        "tmux new -A -d -s web",
        "tmux new -dAs web",
        "tmux new -s web top -d",
    ] {
        assert!(comando_arranca_tmux(comando), "debe arrancar: {comando:?}");
    }
    // No lo arrancan.
    for comando in [
        "",
        "ls -l",
        "cd /srv",
        "echo tmux",
        "man tmux",
        "which tmux",
        "# tmux attach",
        "tmuxinator start web",
        "mi-tmux attach",
        "tmux ls",
        "tmux list-sessions",
        "tmux -V",
        "tmux has-session -t web",
        "tmux kill-server",
        "ls | tmux load-buffer -",
        "tmux send-keys ls Enter",
        "tmux new-window",
        "ls /tmux",
        "cat tmux.conf",
        // Una sesión nueva sin engancharse no deja tmux dentro de tmux.
        "tmux new -d -s build",
        "tmux new-session -d",
        "tmux new -ds build make",
        "tmux new -d -c /srv -s build 'make -j4'",
    ] {
        assert!(
            !comando_arranca_tmux(comando),
            "no debe arrancar: {comando:?}"
        );
    }
}

// ------------------------------------------------------ instantáneas

#[test]
fn instantaneas_de_la_ficha_con_sesiones_sin_marcar() {
    let mut prueba = Prueba::nueva();
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.instantaneas("ficha_sesiones_sin_marcar");
}

#[test]
fn instantaneas_de_la_ficha_con_sesiones_marcadas() {
    let mut prueba = Prueba::nueva();
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    assert!(prueba.ficha().sesiones_persistentes);
    prueba.instantaneas("ficha_sesiones_marcada");
}

#[test]
fn instantaneas_de_la_ficha_con_el_aviso_de_tmux() {
    let mut prueba = Prueba::con_inventario(
        80,
        24,
        Tema::respaldo(),
        con_snippet_al_conectar("tmux new -As main"),
    );
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    prueba.instantaneas("ficha_sesiones_aviso");
    // En el mínimo de la ficha (50×14) el bloque se parte en filas cortas.
    prueba.redimensionar(50, 14);
    prueba.instantanea("ficha_sesiones_aviso_minimo_50x14");
}

#[test]
fn el_aviso_se_ve_en_el_minimo_de_la_ficha_con_la_casilla_enfocada() {
    // En el mínimo (50×14) con el foco en la casilla solo caben cuatro filas
    // del bloque: el aviso de tmux dentro de tmux (R143) es lo que más
    // importa de ese foco y tiene que estar entre ellas, entero.
    let mut prueba = Prueba::con_inventario(
        50,
        14,
        Tema::respaldo(),
        con_snippet_al_conectar("tmux new -As main"),
    );
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    let texto = prueba.texto();
    assert!(texto.contains("Todas las sesiones persistentes"), "{texto}");
    assert!(texto.contains("aviso: el snippet «arranque»"), "{texto}");
    assert!(texto.contains("tmux dentro de tmux"), "{texto}");
}

#[test]
fn instantaneas_de_la_ficha_nueva_con_sesiones() {
    let mut prueba = Prueba::nueva();
    prueba.nuevo();
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.instantaneas("ficha_sesiones_nueva");
}

#[test]
fn la_ficha_con_sesiones_en_ascii_no_lleva_glifos_unicode() {
    let mut tema = Tema::respaldo();
    tema.ascii = true;
    tema.glifos = Glifos::ascii();
    let mut prueba =
        Prueba::con_inventario(120, 40, tema, con_snippet_al_conectar("tmux new -As main"));
    prueba.editar("base-01");
    prueba.ir_a(CampoFicha::SesionesPersistentes);
    prueba.tecla(KeyCode::Char(' '));
    for (cols, filas) in [(120, 40), (80, 24), (50, 14)] {
        prueba.redimensionar(cols, filas);
        let texto = prueba.texto();
        assert!(texto.contains("[x] Todas las sesiones persistentes"));
        // En el mínimo de la ficha (50×14) solo cabe parte del bloque a la
        // vez: el aviso se ve con alto suficiente.
        if filas >= 24 {
            assert!(texto.contains("aviso: el snippet \"arranque\""), "{texto}");
        }
        // Dentro del marco no queda ningún glifo que no sea ASCII o letra.
        let intruso = texto
            .chars()
            .find(|c| !c.is_ascii() && !c.is_alphabetic() && !"┌┐└┘─│".contains(*c));
        assert_eq!(intruso, None, "{cols}x{filas}:\n{texto}");
    }
}
