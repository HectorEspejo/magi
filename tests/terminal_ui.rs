//! Pruebas de la vista Sesión de la Fase 15 sin hilos, red ni servidor:
//! `App::de_prueba`, el bucle real sobre un `TestBackend` y los mensajes del
//! servidor inyectados a mano.
//!
//! Cubren los paneles y sus tamaños, el foco y el ratón, las teclas y el
//! pegado con los modos de cada pestaña, el historial (captura, búsqueda,
//! selección y copia), los enlaces y su diálogo, la escritura en varios con
//! deliberación, el portapapeles del remoto, la reconexión visible, tmux y el
//! bloque TERMINAL de la ficha. Las instantáneas se actualizan con
//! `INSTA_UPDATE=always` y se revisan en `git diff tests/snapshots`.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::escritura_varios::{EstadoEscritura, MotivoFin};
use magi::app::paneles::DisposicionPaneles;
use magi::app::{App, CampoFicha, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::modelo::{DatosHost, Origen};
use magi::protocolo::{
    BytesPestana, EstadoSesionRemota, InfoAviso, InfoReconexion, InfoSesion, InfoSesionTmux,
    InfoTunelTemporal, MensajeCliente, MensajeServidor, ResultadoTmux,
};
use magi::tema::Tema;
use magi::ui::Vista;

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magiterm")
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

    /// App de prueba con el inventario que prepare la prueba (hosts, grupos…).
    fn con_inventario(tocar: impl FnOnce(&magi::almacen::Almacen)) -> Self {
        let entorno = entorno();
        {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
            tocar(&almacen);
        }
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (app, enviados) = App::de_prueba(
            entorno.rutas.clone(),
            entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
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

    fn paso(&mut self) {
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn servidor(&mut self, mensaje: MensajeServidor) {
        self.app.procesar(Evento::Servidor(mensaje));
        self.paso();
    }

    fn evento(&mut self, evento: Evento) {
        self.app.procesar(evento);
        self.paso();
    }

    fn drenar(&mut self) -> Vec<MensajeCliente> {
        let mut mensajes = Vec::new();
        while let Ok(mensaje) = self.enviados.try_recv() {
            mensajes.push(mensaje);
        }
        mensajes
    }

    fn teclas_enviadas(&mut self) -> Vec<(u32, Vec<u8>)> {
        self.drenar()
            .into_iter()
            .filter_map(|mensaje| match mensaje {
                MensajeCliente::Teclas { sesion_id, bytes } => Some((sesion_id, bytes)),
                _ => None,
            })
            .collect()
    }

    fn volcar(&mut self, sesion_id: u32, texto: &str) {
        self.app
            .pantallas
            .volcar(sesion_id, 20, 80, texto.as_bytes());
        self.evento(Evento::Pantallas(vec![sesion_id]));
    }

    fn bienvenida(&mut self, sesiones: Vec<InfoSesion>) {
        self.servidor(bienvenida(sesiones));
    }

    fn entrar_en_sesion(&mut self, sesiones: Vec<InfoSesion>) {
        self.bienvenida(sesiones);
        self.tecla(KeyCode::F(3));
        self.tecla(KeyCode::Enter);
        assert_eq!(self.app.vista, Vista::Sesion);
    }

    fn prefijo(&mut self, codigo: KeyCode) {
        self.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
        self.tecla(codigo);
    }

    fn sedes(&self) -> Vec<u32> {
        self.app
            .pestanas
            .iter()
            .map(|pestana| pestana.sesion_id)
            .collect()
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

    fn instantanea(&self, nombre: &str) {
        let texto = self.texto();
        let mut ajustes = insta::Settings::clone_current();
        ajustes.set_snapshot_path("snapshots");
        ajustes.set_prepend_module_to_snapshot(false);
        ajustes.set_omit_expression(true);
        ajustes.add_filter(r"/tmp/magi[A-Za-z0-9]{6}", "[tmp]");
        ajustes.add_filter(r"hace \d+ s", "hace NN s");
        ajustes.add_filter(r"hace \d+ m", "hace NN m");
        ajustes.add_filter(r"pid \d+", "pid N");
        ajustes.add_filter(r"en \d+ s", "en NN s");
        ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
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
}

fn bienvenida(sesiones: Vec<InfoSesion>) -> MensajeServidor {
    MensajeServidor::Bienvenida {
        version: magi::protocolo::VERSION_PROTOCOLO,
        version_app: "1.4.0".to_string(),
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

fn sesion(id: u32, host: &str) -> InfoSesion {
    InfoSesion {
        id,
        nombre: host.to_string(),
        host_id: i64::from(id),
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

fn sesion_de(id: u32, host_id: i64, host: &str) -> InfoSesion {
    InfoSesion {
        host_id,
        ..sesion(id, host)
    }
}

fn raton(kind: MouseEventKind, columna: u16, fila: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: columna,
        row: fila,
        modifiers: KeyModifiers::NONE,
    }
}

// ------------------------------------------------------------------ paneles

#[test]
fn las_cinco_disposiciones_y_sus_paneles() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02", "db-01", "nas"]);
    let host_ids: Vec<i64> = ["web-01", "web-02", "db-01", "nas"]
        .iter()
        .map(|nombre| prueba.host_id(nombre))
        .collect();
    prueba.entrar_en_sesion(vec![
        sesion_de(1, host_ids[0], "web-01"),
        sesion_de(2, host_ids[1], "web-02"),
        sesion_de(3, host_ids[2], "db-01"),
        sesion_de(4, host_ids[3], "nas"),
    ]);
    assert_eq!(prueba.app.paneles.disposicion, DisposicionPaneles::Completa);
    let mut vistas = Vec::new();
    for _ in 0..5 {
        prueba.prefijo(KeyCode::Char(' '));
        vistas.push((
            prueba.app.paneles.disposicion,
            prueba.app.paneles_visibles().len(),
        ));
    }
    assert_eq!(
        vistas,
        vec![
            (DisposicionPaneles::LadoALado, 2),
            (DisposicionPaneles::Apilados, 2),
            (DisposicionPaneles::Grande2, 3),
            (DisposicionPaneles::Rejilla, 4),
            (DisposicionPaneles::Completa, 1),
        ]
    );
}

#[test]
fn cada_panel_adjunta_su_pestana_y_manda_su_tamano() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("web-02"));
    prueba.entrar_en_sesion(vec![
        sesion_de(1, uno, "web-01"),
        sesion_de(2, dos, "web-02"),
    ]);
    prueba.drenar();
    prueba.prefijo(KeyCode::Char(' '));
    assert_eq!(
        prueba.app.paneles.disposicion,
        DisposicionPaneles::LadoALado
    );
    let mensajes = prueba.drenar();
    let adjuntar: Vec<(u32, u16, u16)> = mensajes
        .iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::Adjuntar {
                sesion_id,
                cols,
                filas,
                colores,
            } => {
                assert!(colores.is_some(), "Adjuntar sin colores del tema");
                Some((*sesion_id, *cols, *filas))
            }
            _ => None,
        })
        .collect();
    // Las dos pestañas quedan adjuntas, cada una con el tamaño de su panel.
    assert_eq!(adjuntar.len(), 2);
    let mut ids: Vec<u32> = adjuntar.iter().map(|(id, _, _)| *id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2]);
    let tamanos: Vec<(u16, u16)> = adjuntar.iter().map(|(_, c, f)| (*c, *f)).collect();
    assert_eq!(
        tamanos[0], tamanos[1],
        "los dos paneles son del mismo tamaño"
    );
    assert!(tamanos[0].0 > 0 && tamanos[0].1 > 0);
    // El tamaño que se manda a cada PTY es el de su panel, no el de la vista.
    assert!(tamanos[0].1 < 20);
}

#[test]
fn las_flechas_cambian_de_panel_y_el_foco_va_al_servidor() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("web-02"));
    prueba.entrar_en_sesion(vec![
        sesion_de(1, uno, "web-01"),
        sesion_de(2, dos, "web-02"),
    ]);
    prueba.prefijo(KeyCode::Char(' '));
    prueba.drenar();
    prueba.prefijo(KeyCode::Right);
    assert_eq!(prueba.app.paneles.enfocado(), 1);
    let foco: Vec<(u32, bool)> = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::Foco {
                sesion_id,
                enfocada,
            } => Some((sesion_id, enfocada)),
            _ => None,
        })
        .collect();
    assert_eq!(foco.last().copied(), Some((2, true)));
    // La pestaña activa es la del panel enfocado.
    assert_eq!(prueba.app.pestana_activa_id(), Some(2));
}

#[test]
fn por_debajo_del_minimo_se_ve_el_enfocado_y_se_avisa() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("web-02"));
    prueba.entrar_en_sesion(vec![
        sesion_de(1, uno, "web-01"),
        sesion_de(2, dos, "web-02"),
    ]);
    prueba.prefijo(KeyCode::Char(' '));
    prueba.redimensionar(50, 12);
    let disposicion = prueba.app.disposicion();
    assert_eq!(disposicion.panes.len(), 1, "solo el panel enfocado");
    assert!(prueba.texto().contains("ventana pequeña"));
}

// --------------------------------------------------------- teclado y pegado

#[test]
fn las_teclas_van_con_los_modos_de_la_pestana() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    // El remoto activa el teclado de kitty (bandera 1).
    prueba.volcar(1, "\x1b[>1u");
    prueba.drenar();
    prueba.tecla_con(KeyCode::Enter, KeyModifiers::SHIFT);
    let enviadas = prueba.teclas_enviadas();
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1, b"\x1b[13;2u");
}

#[test]
fn el_pegado_con_2004_es_un_solo_bloque_y_sin_teclas_detras() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.volcar(1, "\x1b[?2004h");
    prueba.drenar();
    prueba.evento(Evento::Pegado("uno\ndos\ntres\n".to_string()));
    let enviadas = prueba.teclas_enviadas();
    assert_eq!(enviadas.len(), 1, "el pegado va en una escritura sola");
    assert_eq!(enviadas[0].1, b"\x1b[200~uno\rdos\rtres\r\x1b[201~");
    assert!(prueba.app.dialogo.is_none(), "con 2004 no se avisa");
}

#[test]
fn el_pegado_sin_2004_avisa_y_enter_pega() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.drenar();
    prueba.evento(Evento::Pegado("uno\ndos\n".to_string()));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    assert_eq!(
        prueba.teclas_enviadas().len(),
        0,
        "no se pega antes de confirmar"
    );
    prueba.tecla(KeyCode::Enter);
    let enviadas = prueba.teclas_enviadas();
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1, b"uno\rdos\r");
    assert!(prueba.app.dialogo.is_none());
}

#[test]
fn esc_cancela_el_aviso_de_pegado() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.evento(Evento::Pegado("uno\ndos\n".to_string()));
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(prueba.teclas_enviadas().len(), 0);
}

#[test]
fn el_raton_se_reenvia_al_remoto_que_lo_pide() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.volcar(1, "\x1b[?1002;1006h");
    prueba.drenar();
    let (_, rect) = prueba.app.disposicion().panes[0];
    prueba.evento(Evento::Raton(raton(
        MouseEventKind::ScrollUp,
        rect.x + 1,
        rect.y + 1,
    )));
    let enviadas = prueba.teclas_enviadas();
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1, b"\x1b[<64;2;2M");
    // Y el clic también, con sus coordenadas relativas al contenido.
    prueba.evento(Evento::Raton(raton(
        MouseEventKind::Down(MouseButton::Left),
        rect.x,
        rect.y,
    )));
    let enviadas = prueba.teclas_enviadas();
    assert_eq!(enviadas[0].1, b"\x1b[<0;1;1M");
    // El ratón queda capturado por el terminal exterior (se pide al pintar).
    assert!(prueba.app.disposicion().panes.len() == 1);
}

#[test]
fn la_rueda_con_raton_activo_abre_el_historial() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.app.config.terminal.raton = true;
    prueba.drenar();
    let (_, rect) = prueba.app.disposicion().panes[0];
    prueba.evento(Evento::Raton(raton(
        MouseEventKind::ScrollUp,
        rect.x + 2,
        rect.y + 2,
    )));
    let capturas: Vec<u64> = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::CapturarHistorial { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .collect();
    assert_eq!(capturas.len(), 1, "la rueda abre el modo historial");
    assert!(prueba.app.historial.is_some());
}

// --------------------------------------------------------------- historial

fn responder_historial(prueba: &mut Prueba, lineas: &[(&str, bool)]) {
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::CapturarHistorial { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin CapturarHistorial pendiente");
    let lineas = lineas
        .iter()
        .map(
            |(texto, envuelta)| magi_nucleo::terminal::historial::LineaHistorial {
                tramos: vec![magi_nucleo::terminal::tramos::Tramo {
                    texto: (*texto).to_string(),
                    primer_plano: magi_nucleo::terminal::tramos::Color::Predeterminado,
                    fondo: magi_nucleo::terminal::tramos::Color::Predeterminado,
                    negrita: false,
                    cursiva: false,
                    subrayado: false,
                    inverso: false,
                }],
                envuelta: *envuelta,
            },
        )
        .collect::<Vec<_>>();
    let total = lineas.len() as u32;
    prueba.servidor(MensajeServidor::HistorialLote {
        peticion_id,
        sesion_id: 1,
        lote: 0,
        ultimo: true,
        total,
        lineas,
    });
}

#[test]
fn el_historial_se_captura_navega_y_busca() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('['));
    responder_historial(
        &mut prueba,
        &[
            ("linea uno", false),
            ("error de conexion", false),
            ("linea tres", false),
            ("otro error", false),
        ],
    );
    assert!(prueba.app.historial.is_some());
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "error".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let estado = prueba.app.historial.as_ref().unwrap();
    assert_eq!(estado.busqueda.as_ref().unwrap().coincidencias.len(), 2);
    prueba.instantanea("terminal_historial_busqueda_80x24");
    // n va a la coincidencia anterior (más vieja).
    prueba.tecla(KeyCode::Char('n'));
    let estado = prueba.app.historial.as_ref().unwrap();
    assert_eq!(estado.cursor, 1);
    // Esc cierra búsqueda y luego el modo.
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.historial.is_none());
}

#[test]
fn seleccionar_y_copiar_une_las_lineas_envueltas() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.prefijo(KeyCode::Char('['));
    responder_historial(&mut prueba, &[("hola mun", true), ("do entero", false)]);
    prueba.tecla(KeyCode::Char('g'));
    prueba.tecla(KeyCode::Char('v'));
    prueba.tecla(KeyCode::Char('V'));
    prueba.tecla(KeyCode::Char('y'));
    let aviso = prueba.app.mensaje.as_ref().expect("mensaje de copia");
    assert!(aviso.texto.contains("historial"));
    assert!(aviso.texto.contains("copiado"));
}

#[test]
fn shift_repag_con_pantalla_alternativa_va_al_remoto() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.volcar(1, "\x1b[?1049h");
    prueba.drenar();
    prueba.tecla_con(KeyCode::PageUp, KeyModifiers::SHIFT);
    assert!(prueba.app.historial.is_none());
    let enviadas = prueba.teclas_enviadas();
    assert_eq!(enviadas.len(), 1);
    assert_eq!(enviadas[0].1, b"\x1b[5~");
}

// ---------------------------------------------------------------- enlaces

#[test]
fn los_enlaces_se_marcan_y_el_dialogo_ensena_la_url() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.volcar(
        1,
        "Local: http://localhost:5173/\r\nGuia: https://ejemplo.org/guia#arranque\r\n",
    );
    prueba.prefijo(KeyCode::Char('o'));
    assert!(prueba.app.enlaces.activo);
    assert_eq!(prueba.app.enlaces.letras.len(), 2);
    prueba.instantanea("terminal_enlaces_80x24");
    prueba.tecla(KeyCode::Char('a'));
    match &prueba.app.dialogo {
        Some(Dialogo::Enlace(dialogo)) => {
            assert_eq!(dialogo.url, "http://localhost:5173/");
            assert_eq!(dialogo.tunel, Some(("localhost".to_string(), 5173)));
        }
        _ => panic!("se esperaba el diálogo del enlace"),
    }
    prueba.instantanea("terminal_enlace_dialogo_80x24");
    // Con el efecto externo apagado, ↵ abre sin lanzar nada.
    prueba.tecla(KeyCode::Enter);
    assert!(prueba
        .app
        .mensaje
        .as_ref()
        .unwrap()
        .texto
        .contains("abriría"));
}

// ------------------------------------------------- escritura en varios

#[test]
fn prefijo_e_con_dos_hosts_delibera_y_escribe_en_los_paneles() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("web-02"));
    prueba.entrar_en_sesion(vec![
        sesion_de(1, uno, "web-01"),
        sesion_de(2, dos, "web-02"),
    ]);
    prueba.prefijo(KeyCode::Char(' '));
    prueba.drenar();
    prueba.prefijo(KeyCode::Char('E'));
    assert!(
        prueba.app.deliberacion.is_some(),
        "con dos hosts hay deliberación MAGI"
    );
    // Sin comprobaciones activas el consenso es aprobada: Ctrl+K la confirma.
    prueba.tecla_con(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(prueba.app.deliberacion.is_none());
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::IniciarEscrituraVarios { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin IniciarEscrituraVarios");
    prueba.servidor(MensajeServidor::EscrituraVariosIniciada { peticion_id, id: 7 });
    assert!(prueba.app.escritura_varios.activa());
    assert!(prueba.texto().contains("ESCRIBIENDO EN 2 PANELES"));
    // Una tecla va a las dos pestañas con sus modos.
    let sesiones = prueba.sedes();
    prueba.drenar();
    prueba.tecla(KeyCode::Char('x'));
    let enviados: Vec<BytesPestana> = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::EscribirVarios { pestanas, id } => {
                assert_eq!(id, 7);
                Some(pestanas)
            }
            _ => None,
        })
        .expect("sin EscribirVarios");
    assert_eq!(enviados.len(), 2);
    assert_eq!(
        enviados.iter().map(|p| p.sesion_id).collect::<Vec<_>>(),
        sesiones
    );
    assert!(enviados.iter().all(|p| p.bytes == b"x"));
    // prefijo E la para y se anota el motivo.
    prueba.prefijo(KeyCode::Char('E'));
    assert!(!prueba.app.escritura_varios.activa());
    let terminada = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::TerminarEscrituraVarios { id, motivo } => Some((id, motivo)),
            _ => None,
        })
        .expect("sin TerminarEscrituraVarios");
    assert_eq!(terminada.0, 7);
    assert_eq!(terminada.1, MotivoFin::Apagada.texto());
    // Y con la concesión terminada por el servidor se apaga igual.
    prueba.servidor(MensajeServidor::EscrituraVariosTerminada {
        id: 7,
        motivo: "inactividad".to_string(),
    });
    assert!(matches!(
        prueba.app.escritura_varios.estado,
        EstadoEscritura::Apagada
    ));
}

#[test]
fn cambiar_de_disposicion_apaga_la_escritura() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("web-02"));
    prueba.entrar_en_sesion(vec![
        sesion_de(1, uno, "web-01"),
        sesion_de(2, dos, "web-02"),
    ]);
    prueba.prefijo(KeyCode::Char(' '));
    prueba.prefijo(KeyCode::Char('E'));
    prueba.tecla_con(KeyCode::Char('k'), KeyModifiers::CONTROL);
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::IniciarEscrituraVarios { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .unwrap();
    prueba.servidor(MensajeServidor::EscrituraVariosIniciada { peticion_id, id: 3 });
    prueba.prefijo(KeyCode::Char(' '));
    assert!(!prueba.app.escritura_varios.activa());
    assert!(prueba.drenar().iter().any(|mensaje| matches!(
        mensaje,
        MensajeCliente::TerminarEscrituraVarios { id: 3, .. }
    )));
}

// ------------------------------------------------- portapapeles del remoto

#[test]
fn el_portapapeles_pedido_se_permite_con_prefijo_p() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.drenar();
    prueba.servidor(MensajeServidor::PortapapelesPedido {
        sesion_id: 1,
        bytes: 1229,
    });
    let mensaje = prueba.app.mensaje.as_ref().unwrap();
    assert!(mensaje.texto.contains("web-01"));
    assert!(mensaje.texto.contains("1,2 KB"));
    assert!(mensaje.texto.contains("P"));
    prueba.prefijo(KeyCode::Char('P'));
    let permitido = prueba.drenar().iter().any(|mensaje| {
        matches!(
            mensaje,
            MensajeCliente::PortapapelesPermitido { sesion_id: 1 }
        )
    });
    assert!(permitido);
    let host_bd = prueba.app.almacen.obtener_host(host).unwrap();
    assert!(host_bd.portapapeles_remoto);
}

#[test]
fn la_copia_del_remoto_se_avisa_con_host_y_tamano() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.servidor(MensajeServidor::Portapapeles {
        sesion_id: 1,
        texto: "hola".to_string(),
    });
    let mensaje = prueba.app.mensaje.as_ref().unwrap();
    assert!(mensaje.texto.contains("web-01 copió"));
}

// ------------------------------------------------- reconexión y tmux

#[test]
fn la_pestana_reconectando_se_ve_con_cuenta_atras() {
    let mut prueba = Prueba::nueva(&["web-01", "db-01"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("db-01"));
    let mut reconectando = sesion_de(1, uno, "web-01");
    reconectando.estado = EstadoSesionRemota::Caida;
    reconectando.reconexion = Some(InfoReconexion {
        intento: 3,
        proximo_en: chrono::Utc::now().timestamp() + 8,
        desde: chrono::Utc::now().timestamp() - 41,
        causa: "conexión perdida".to_string(),
    });
    reconectando.tmux = Some("magi-portatil-1".to_string());
    prueba.entrar_en_sesion(vec![reconectando, sesion_de(2, dos, "db-01")]);
    let texto = prueba.texto();
    assert!(texto.contains("Reconectando con web-01"));
    assert!(texto.contains("intento 3"));
    assert!(texto.contains("tmux magi-portatil-1"));
    prueba.instantanea("terminal_reconectando_80x24");
}

#[test]
fn sesiones_muestra_titulo_avisos_y_lista_tmux() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    let mut info = sesion_de(1, host, "web-01");
    info.titulo = Some("Claude Code".to_string());
    info.aviso = Some(InfoAviso {
        texto: "Claude necesita tu permiso".to_string(),
        hora: 0,
    });
    info.tmux = Some("magi-portatil-1".to_string());
    prueba.bienvenida(vec![info]);
    prueba.tecla(KeyCode::F(3));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    prueba.redimensionar(200, 60);
    let texto = prueba.texto();
    assert!(texto.contains("Claude Code"));
    assert!(texto.contains("!"));
    prueba.instantanea("sesiones_con_titulo_tmux_200x60");
    prueba.redimensionar(80, 24);
    prueba.tecla(KeyCode::Char('t'));
    assert!(prueba.app.tmux.is_some());
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ListarTmux {
                peticion_id,
                host_id,
            } => {
                assert_eq!(host_id, host);
                Some(peticion_id)
            }
            _ => None,
        })
        .expect("sin ListarTmux");
    prueba.servidor(MensajeServidor::SesionesTmux {
        peticion_id,
        host_id: host,
        sesiones: vec![
            InfoSesionTmux {
                nombre: "magi-portatil-1".to_string(),
                enganchada: true,
                ventanas: 2,
                creada: "hace 3 h".to_string(),
            },
            InfoSesionTmux {
                nombre: "despliegue".to_string(),
                enganchada: false,
                ventanas: 1,
                creada: "ayer".to_string(),
            },
        ],
        sin_tmux: None,
    });
    let texto = prueba.texto();
    assert!(texto.contains("magi-portatil-1"));
    assert!(texto.contains("despliegue"));
    prueba.instantanea("sesiones_tmux_80x24");
    // T pide confirmación y el servidor la termina.
    prueba.tecla(KeyCode::Char('T'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::TerminarTmux {
                peticion_id,
                nombre,
                ..
            } => Some((peticion_id, nombre)),
            _ => None,
        })
        .expect("sin TerminarTmux");
    assert_eq!(peticion.1, "magi-portatil-1");
    prueba.servidor(MensajeServidor::TmuxTerminada {
        peticion_id: peticion.0,
        resultado: ResultadoTmux::Ok,
    });
    assert!(!prueba
        .app
        .tmux
        .as_ref()
        .unwrap()
        .sesiones
        .iter()
        .any(|s| s.nombre == "magi-portatil-1"));
}

// ------------------------------------------------------------------ ficha

#[test]
fn la_ficha_tiene_el_bloque_terminal_y_lo_guarda() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('e'));
    assert_eq!(prueba.app.vista, Vista::Ficha);
    let _ = host;
    let ficha = prueba.app.ficha.as_ref().unwrap();
    assert!(ficha.reconectar, "por defecto se reconecta");
    assert!(!ficha.tmux_persistente);
    assert!(!ficha.portapapeles_remoto);
    // Tab hasta el bloque TERMINAL: el formulario se desplaza hasta el campo.
    for _ in 0..40 {
        if prueba.app.ficha.as_ref().unwrap().campo == CampoFicha::Reconectar {
            break;
        }
        prueba.tecla(KeyCode::Tab);
    }
    assert_eq!(
        prueba.app.ficha.as_ref().unwrap().campo,
        CampoFicha::Reconectar
    );
    let texto = prueba.texto();
    assert!(texto.contains("TERMINAL (solo en este equipo)"), "{texto}");
    assert!(texto.contains("Reconectar solo si se cae la red"));
    prueba.instantanea("ficha_terminal_80x24");
    prueba.tecla(KeyCode::Char(' '));
    assert!(!prueba.app.ficha.as_ref().unwrap().reconectar);
    prueba.tecla_con(KeyCode::Char('s'), KeyModifiers::CONTROL);
    let guardado = prueba.app.almacen.obtener_host(host).unwrap();
    assert!(!guardado.reconectar);
    assert!(!guardado.tmux_persistente);
    assert!(!guardado.portapapeles_remoto);
}

// ----------------------------------------------------------- instantáneas

#[test]
fn instantaneas_de_las_cinco_disposiciones() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02", "db-01", "nas"]);
    let host_ids: Vec<i64> = ["web-01", "web-02", "db-01", "nas"]
        .iter()
        .map(|nombre| prueba.host_id(nombre))
        .collect();
    prueba.entrar_en_sesion(vec![
        sesion_de(1, host_ids[0], "web-01"),
        sesion_de(2, host_ids[1], "web-02"),
        sesion_de(3, host_ids[2], "db-01"),
        sesion_de(4, host_ids[3], "nas"),
    ]);
    for indice in 0..5 {
        if indice > 0 {
            prueba.prefijo(KeyCode::Char(' '));
        }
        let nombre = match prueba.app.paneles.disposicion {
            DisposicionPaneles::Completa => "completa",
            DisposicionPaneles::LadoALado => "lado_a_lado",
            DisposicionPaneles::Apilados => "apilados",
            DisposicionPaneles::Grande2 => "grande2",
            DisposicionPaneles::Rejilla => "rejilla",
        };
        for (cols, filas) in [(40, 12), (80, 24), (200, 60)] {
            prueba.redimensionar(cols, filas);
            prueba.instantanea(&format!("terminal_{nombre}_{cols}x{filas}"));
        }
    }
}

// ----------------------------------------- instantáneas de los modos (F15)

/// Tres tamaños obligatorios de la vista Sesión para las instantáneas.
const TAMANOS_F15: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

fn instantaneas(prueba: &mut Prueba, nombre: &str) {
    for (cols, filas) in TAMANOS_F15 {
        prueba.redimensionar(cols, filas);
        prueba.instantanea(&format!("{nombre}_{cols}x{filas}"));
    }
    prueba.redimensionar(80, 24);
}

#[test]
fn instantaneas_del_historial_con_busqueda_y_seleccion() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.prefijo(KeyCode::Char('['));
    responder_historial(
        &mut prueba,
        &[
            ("linea uno", false),
            ("error de conexion", false),
            ("linea tres", false),
            ("otro error", false),
        ],
    );
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "error".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    instantaneas(&mut prueba, "terminal_historial_busqueda");
    // Selección por líneas del rango visible.
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Char('g'));
    prueba.tecla(KeyCode::Char('v'));
    prueba.tecla(KeyCode::Char('V'));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    instantaneas(&mut prueba, "terminal_historial_seleccion");
}

#[test]
fn instantaneas_de_enlaces_y_del_dialogo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.volcar(
        1,
        "Local: http://localhost:5173/\r\nGuia: https://ejemplo.org/guia#arranque\r\n",
    );
    prueba.prefijo(KeyCode::Char('o'));
    instantaneas(&mut prueba, "terminal_enlaces");
    prueba.tecla(KeyCode::Char('a'));
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Enlace(_))),
        "sin diálogo de enlace"
    );
    instantaneas(&mut prueba, "terminal_enlace_dialogo");
}

#[test]
fn instantaneas_de_la_escritura_en_varios() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("web-02"));
    prueba.entrar_en_sesion(vec![
        sesion_de(1, uno, "web-01"),
        sesion_de(2, dos, "web-02"),
    ]);
    prueba.prefijo(KeyCode::Char(' '));
    prueba.prefijo(KeyCode::Char('E'));
    prueba.tecla_con(KeyCode::Char('k'), KeyModifiers::CONTROL);
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::IniciarEscrituraVarios { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .unwrap();
    prueba.servidor(MensajeServidor::EscrituraVariosIniciada { peticion_id, id: 5 });
    instantaneas(&mut prueba, "terminal_escritura_varios");
}

#[test]
fn instantaneas_de_una_pestana_reconectando() {
    let mut prueba = Prueba::nueva(&["web-01", "db-01"]);
    let (uno, dos) = (prueba.host_id("web-01"), prueba.host_id("db-01"));
    let mut reconectando = sesion_de(1, uno, "web-01");
    reconectando.estado = EstadoSesionRemota::Caida;
    reconectando.reconexion = Some(InfoReconexion {
        intento: 3,
        proximo_en: chrono::Utc::now().timestamp() + 8,
        desde: chrono::Utc::now().timestamp() - 41,
        causa: "conexión perdida".to_string(),
    });
    reconectando.tmux = Some("magi-portatil-1".to_string());
    prueba.entrar_en_sesion(vec![reconectando, sesion_de(2, dos, "db-01")]);
    instantaneas(&mut prueba, "terminal_reconectando");
}

#[test]
fn instantaneas_de_sesiones_con_tmux() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    let mut info = sesion_de(1, host, "web-01");
    info.titulo = Some("Claude Code".to_string());
    info.aviso = Some(InfoAviso {
        texto: "Claude necesita tu permiso".to_string(),
        hora: 0,
    });
    info.tmux = Some("magi-portatil-1".to_string());
    prueba.bienvenida(vec![info]);
    prueba.tecla(KeyCode::F(3));
    instantaneas(&mut prueba, "sesiones_con_titulo_tmux");
    prueba.tecla(KeyCode::Char('t'));
    let peticion_id = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ListarTmux { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .unwrap();
    prueba.servidor(MensajeServidor::SesionesTmux {
        peticion_id,
        host_id: host,
        sesiones: vec![
            InfoSesionTmux {
                nombre: "magi-portatil-1".to_string(),
                enganchada: true,
                ventanas: 2,
                creada: "hace 3 h".to_string(),
            },
            InfoSesionTmux {
                nombre: "despliegue".to_string(),
                enganchada: false,
                ventanas: 1,
                creada: "ayer".to_string(),
            },
        ],
        sin_tmux: None,
    });
    instantaneas(&mut prueba, "sesiones_tmux");
}

#[test]
fn instantaneas_del_aviso_de_pegado() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.evento(Evento::Pegado("uno\ndos\ntres\n".to_string()));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    instantaneas(&mut prueba, "terminal_pegado_aviso");
}

#[test]
fn instantaneas_de_la_ficha_con_terminal() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('e'));
    assert_eq!(prueba.app.vista, Vista::Ficha);
    for _ in 0..40 {
        if prueba.app.ficha.as_ref().unwrap().campo == CampoFicha::Reconectar {
            break;
        }
        prueba.tecla(KeyCode::Tab);
    }
    assert_eq!(
        prueba.app.ficha.as_ref().unwrap().campo,
        CampoFicha::Reconectar
    );
    instantaneas(&mut prueba, "ficha_terminal");
}

#[test]
fn w_guarda_la_captura_en_un_fichero_nuevo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.entrar_en_sesion(vec![sesion_de(1, host, "web-01")]);
    prueba.prefijo(KeyCode::Char('['));
    responder_historial(
        &mut prueba,
        &[("uno", false), ("dos", true), ("tres", false)],
    );
    // El hogar de la prueba no existe hasta que algo lo crea.
    std::fs::create_dir_all(&prueba._entorno.rutas.hogar).unwrap();
    prueba.tecla(KeyCode::Char('w'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::EntradaTexto { .. })
    ));
    // La ruta propuesta cuelga del hogar de la prueba: ↵ la guarda.
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none());
    let aviso = prueba.app.mensaje.as_ref().expect("sin mensaje");
    assert!(
        aviso.texto.contains("historial guardado"),
        "{}",
        aviso.texto
    );
    let hogar = prueba._entorno.rutas.hogar.clone();
    let fichero = std::fs::read_dir(&hogar)
        .unwrap()
        .filter_map(|entrada| entrada.ok())
        .map(|entrada| entrada.path())
        .find(|ruta| {
            ruta.file_name()
                .is_some_and(|nombre| nombre.to_string_lossy().starts_with("magi-historial-"))
        })
        .expect("sin fichero de historial");
    let contenido = std::fs::read_to_string(fichero).unwrap();
    assert!(contenido.contains("uno"));
    assert!(contenido.contains("tres"));
}

#[test]
fn la_paleta_abre_el_dialogo_de_rejilla_y_confirma_con_dos() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02", "db-01"]);
    // El inventario se lista por nombre: db-01 va antes que web-01.
    let (uno, tres) = (prueba.host_id("web-01"), prueba.host_id("db-01"));
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(prueba.app.paleta.is_some());
    for caracter in "abrir en rejilla".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Rejilla(_))),
        "la paleta debe abrir el diálogo de casillas"
    );
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char(' '));
    prueba.instantanea("terminal_rejilla_dialogo_80x24");
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none());
    let abiertas: Vec<i64> = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::AbrirSesion { host_id, .. } => Some(host_id),
            _ => None,
        })
        .collect();
    assert_eq!(abiertas, vec![tres, uno]);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![sesion_de(1, tres, "db-01"), sesion_de(2, uno, "web-01")],
    });
    assert_eq!(prueba.app.vista, Vista::Sesion);
    assert_eq!(
        prueba.app.paneles.disposicion,
        DisposicionPaneles::LadoALado
    );
}

#[test]
fn marcar_un_grupo_marca_todos_sus_hosts() {
    let mut prueba = Prueba::con_inventario(|almacen| {
        let grupo = almacen.crear_grupo("producción").expect("grupo");
        for nombre in ["web-01", "web-02"] {
            almacen
                .crear_host(
                    &DatosHost {
                        nombre: nombre.to_string(),
                        direccion: "192.0.2.10".to_string(),
                        grupo_id: Some(grupo),
                        ..DatosHost::default()
                    },
                    Origen::Manual,
                )
                .expect("host del grupo");
        }
        almacen
            .crear_host(
                &DatosHost {
                    nombre: "db-01".to_string(),
                    direccion: "192.0.2.11".to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("host suelto");
    });
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    for caracter in "rejilla".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let Some(Dialogo::Rejilla(estado)) = &prueba.app.dialogo else {
        panic!("sin diálogo de rejilla");
    };
    assert!(matches!(
        estado.filas.first(),
        Some(magi::app::rejilla::FilaRejilla::Grupo { .. })
    ));
    prueba.tecla(KeyCode::Char(' '));
    let Some(Dialogo::Rejilla(estado)) = &prueba.app.dialogo else {
        panic!("sin diálogo de rejilla");
    };
    assert_eq!(estado.marcados(), 2);
    assert!(estado.hosts_marcados().contains(&prueba.host_id("web-01")));
    assert!(estado.hosts_marcados().contains(&prueba.host_id("web-02")));
    prueba.tecla(KeyCode::Char(' '));
    let Some(Dialogo::Rejilla(estado)) = &prueba.app.dialogo else {
        panic!("sin diálogo de rejilla");
    };
    assert_eq!(estado.marcados(), 0);
}

#[test]
fn el_dialogo_de_rejilla_avisa_del_limite_de_cuatro() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02", "db-01", "nas", "extra"]);
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    for caracter in "rejilla".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    for _ in 0..4 {
        prueba.tecla(KeyCode::Char(' '));
        prueba.tecla(KeyCode::Down);
    }
    let Some(Dialogo::Rejilla(estado)) = &prueba.app.dialogo else {
        panic!("sin diálogo de rejilla");
    };
    assert_eq!(estado.marcados(), 4);
    prueba.tecla(KeyCode::Char(' '));
    let Some(Dialogo::Rejilla(estado)) = &prueba.app.dialogo else {
        panic!("sin diálogo de rejilla");
    };
    assert_eq!(estado.marcados(), 4);
    assert!(estado.aviso.is_some());
    assert!(
        prueba.texto().contains("como mucho 4 hosts"),
        "{}",
        prueba.texto()
    );
}

#[test]
fn esc_cancela_el_dialogo_de_rejilla_sin_abrir_nada() {
    let mut prueba = Prueba::nueva(&["web-01", "web-02"]);
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    for caracter in "rejilla".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        prueba
            .drenar()
            .into_iter()
            .filter(|mensaje| matches!(mensaje, MensajeCliente::AbrirSesion { .. }))
            .count(),
        0
    );
}

#[test]
fn la_vista_tuneles_muestra_los_temporales_de_los_enlaces() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host = prueba.host_id("web-01");
    prueba.servidor(MensajeServidor::TunelesTemporales {
        lista: vec![InfoTunelTemporal {
            id: 1,
            sesion_id: 7,
            host_id: host,
            host_nombre: "web-01".to_string(),
            puerto_local: 54321,
            destino: "localhost".to_string(),
            puerto: 8000,
            conexiones: 2,
        }],
    });
    prueba.tecla(KeyCode::F(6));
    assert_eq!(prueba.app.vista, Vista::Tuneles);
    let texto = prueba.texto();
    assert!(texto.contains("TEMPORALES"), "{texto}");
    assert!(texto.contains("temporal"), "{texto}");
    assert!(texto.contains("web-01"), "{texto}");
    assert!(texto.contains("127.0.0.1:54321"), "{texto}");
    assert!(texto.contains("localhost:8000"), "{texto}");
    assert!(texto.contains("2 conex"), "{texto}");
    assert!(texto.contains("se cierran con su pestaña"), "{texto}");
    // No hay acción de gestión: la barra no ofrece activar/parar para ellos.
    prueba.instantanea("terminal_tuneles_temporales_80x24");
}
