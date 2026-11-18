//! Portapapeles, enlaces, visor de escritorios y avisos del servidor en la barra
//! de MAGI en Termux (Fase 17, Sprint 2): `App::de_prueba`, el bucle real sobre
//! un `TestBackend` y los dobles de las órdenes `termux-*` de
//! `tests/fijos/termux/` (anotan argumentos y entrada, y saben colgarse o
//! fallar). Sin Termux, sin red y sin tocar la terminal real: el OSC 52 que
//! escribiría la ventana se anota en `App.termux.osc52_de_prueba`.
//!
//! `es_termux()` se decide una vez por proceso, por eso esto vive en su propio
//! binario y todas las pruebas empiezan por `Prueba::con`, que pone
//! `MAGI_PLATAFORMA=termux` antes de que nadie pregunte. Solo vale en
//! compilaciones de depuración: con `cargo test --release` la variable no
//! activa nada y no hay nada que probar.
#![cfg(debug_assertions)]

#[path = "fijos/termux/mod.rs"]
mod dobles;

use std::path::PathBuf;
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::enlaces::DialogoEnlace;
use magi::app::escritorios::{
    navegador_de_termux, resolver_navegador, OrigenNavegador, Plataforma,
};
use magi::app::termux::{
    EventoTermux, COPIAS_A_LA_VEZ, PLAZO_LECTURA_ESTADO, RITMO_ESTADO_SERVIDOR, VISIBILIDAD_AVISO,
};
use magi::app::{App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::escritorio::{
    EstadoEscritorio, InfoEscritorio, OpcionesEscritorio, TamanoEscritorio, TipoHost,
};
use magi::identidades::ClaveGenerada;
use magi::modelo::{DatosHost, IdentidadRef, Origen, OrigenIdentidad};
use magi::protocolo::{
    EstadoSesionRemota, InfoSesion, MensajeCliente, MensajeServidor, RANGO_ESCRITORIOS,
    VERSION_PROTOCOLO,
};
use magi::tema::Tema;
use magi::termux::estado::{self, AvisoServidor, EstadoServidorTermux};
use magi::termux::portapapeles::{
    secuencia_osc52, tamano_kib, AVISO_API_APAGADA, TOPE_API, TOPE_OSC52,
};
use magi::termux::{es_termux, ApiProceso, EstadoApi, Ordenes};
use magi::ui::Vista;

use dobles::{sigue_vivo_tras, Dobles, Modo};

const AVISO_API: &str = "Termux:API no responde: se copia por OSC 52 y no habrá avisos de Android";
const HUELLA: &str = "SHA256:T2qRAb3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7";

/// Pone `MAGI_PLATAFORMA=termux` una sola vez, antes de la primera pregunta.
fn como_termux() {
    static UNA_VEZ: Once = Once::new();
    UNA_VEZ.call_once(|| std::env::set_var("MAGI_PLATAFORMA", "termux"));
    assert!(
        es_termux(),
        "es_termux() ya se decidió sin MAGI_PLATAFORMA=termux en este proceso"
    );
}

/// La configuración por defecto de un MAGI de Termux: sus valores por defecto
/// dependen de `es_termux()`, así que la plataforma se fija antes.
fn config_termux() -> Config {
    como_termux();
    Config::default()
}

// ------------------------------------------------------------------- arnés

struct Prueba {
    app: App,
    terminal: Terminal<TestBackend>,
    enviados: tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
    dobles: Dobles,
    api: Arc<ApiProceso>,
    rutas: Rutas,
    raiz: PathBuf,
    _temporal: tempfile::TempDir,
}

impl Prueba {
    fn nueva() -> Self {
        Self::con(config_termux())
    }

    /// App de Termux con web-01 en el inventario, órdenes de plazo holgado
    /// sobre los dobles y la `Bienvenida` del servidor ya recibida (hasta
    /// entonces el cliente no mira lo que el servidor dejó en el estado).
    fn con(config: Config) -> Self {
        let mut prueba = Self::sin_bienvenida(config);
        prueba.bienvenida();
        prueba
    }

    /// La `Bienvenida` de un servidor sin sesiones.
    fn bienvenida(&mut self) {
        self.servidor(MensajeServidor::Bienvenida {
            version: VERSION_PROTOCOLO,
            version_app: "1.5.0".to_string(),
            pid: 4242,
            cliente_id: 1,
            clientes: 1,
            sesiones: Vec::new(),
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: Vec::new(),
        });
        self.drenar();
    }

    /// Como [`Prueba::con`], pero el servidor aún no ha dado la bienvenida.
    fn sin_bienvenida(config: Config) -> Self {
        como_termux();
        let temporal = tempfile::Builder::new()
            .prefix("magitp")
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
        {
            let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("almacén");
            almacen
                .crear_host(
                    &DatosHost {
                        nombre: "web-01".to_string(),
                        direccion: "192.0.2.11".to_string(),
                        ..DatosHost::default()
                    },
                    Origen::Manual,
                )
                .expect("host de prueba");
        }
        let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("almacén");
        let (app, enviados) =
            App::de_prueba(rutas.clone(), config, Tema::respaldo(), almacen).expect("app");
        let terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let dobles = Dobles::nuevos(&raiz.join("termux"));
        let mut prueba = Self {
            app,
            terminal,
            enviados,
            dobles,
            api: Arc::new(ApiProceso::default()),
            rutas,
            raiz,
            _temporal: temporal,
        };
        prueba
            .app
            .iniciar_pintado(&mut prueba.terminal)
            .expect("pintado");
        prueba.con_plazo(Duration::from_secs(20));
        prueba
    }

    /// Las órdenes de Termux de la ventana, sobre los dobles y con este plazo.
    fn con_plazo(&mut self, plazo: Duration) {
        let entorno = self.dobles.entorno(&self.raiz.join("home"));
        self.app.termux.ordenes = Arc::new(Ordenes::nuevas(&entorno, plazo, Arc::clone(&self.api)));
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
        self.evento(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
    }

    fn tick(&mut self) {
        self.evento(Evento::Tick);
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

    /// Lo que dice ahora la barra (texto y si es un error).
    fn mensaje(&self) -> Option<(String, bool)> {
        self.app
            .mensaje
            .as_ref()
            .map(|mensaje| (mensaje.texto.clone(), mensaje.error))
    }

    /// Bombea los eventos que lleguen de las tareas hasta que se cumpla la
    /// condición (máximo 15 s).
    fn esperar(&mut self, que: &str, condicion: impl Fn(&Prueba) -> bool) {
        let limite = Instant::now() + Duration::from_secs(15);
        while !condicion(self) {
            assert!(
                Instant::now() < limite,
                "no se cumplió «{que}» en 15 s; barra: {:?}\n{}",
                self.mensaje(),
                self.texto()
            );
            self.app
                .bombear(&mut self.terminal, Duration::from_millis(30))
                .expect("bombear");
        }
    }

    /// Espera a que no quede ninguna copia por la orden en vuelo.
    fn esperar_copia(&mut self) {
        self.esperar("copia terminada", |p| {
            p.app.termux.copias_en_curso == 0 && p.mensaje().is_some_and(|(m, _)| m != "copiando…")
        });
    }

    /// Lo que va diciendo la barra, uno a uno, con los turnos de la cola.
    fn vaciar_barra(&mut self) -> Vec<(String, bool)> {
        let mut vistos = Vec::new();
        for _ in 0..30 {
            if let Some(mensaje) = self.app.mensaje.take() {
                vistos.push((mensaje.texto, mensaje.error));
            }
            self.tick();
            if self.app.mensaje.is_none() && self.app.termux.cola_barra.is_empty() {
                break;
            }
        }
        vistos
    }

    /// Copia `texto` con la tecla `c` del diálogo de un enlace (una URL
    /// cualquiera: el diálogo copia lo que lleve).
    fn copiar_url(&mut self, url: &str) {
        self.app.dialogo = Some(Dialogo::Enlace(DialogoEnlace::nuevo(url.to_string())));
        self.tecla(KeyCode::Char('c'));
    }

    /// Abre un enlace con ↵ en su diálogo.
    fn abrir_url(&mut self, url: &str) {
        self.app.dialogo = Some(Dialogo::Enlace(DialogoEnlace::nuevo(url.to_string())));
        self.tecla(KeyCode::Enter);
    }

    fn llamadas(&self, orden: &str) -> Vec<dobles::Llamada> {
        self.dobles.llamadas(orden)
    }

    fn osc52(&self) -> &[(std::thread::ThreadId, Vec<u8>)] {
        &self.app.termux.osc52_de_prueba
    }
}

/// Una URL http de exactamente `bytes` bytes que empieza con `prefijo`.
fn url_de(bytes: usize, prefijo: &str) -> String {
    assert!(prefijo.len() <= bytes);
    format!("{prefijo}{}", "a".repeat(bytes - prefijo.len()))
}

fn sesion(id: u32, host_id: i64, host: &str) -> InfoSesion {
    InfoSesion {
        id,
        nombre: host.to_string(),
        host_id,
        host_nombre: host.to_string(),
        estado: EstadoSesionRemota::Abierta,
        motivo: None,
        identidad: "clave".to_string(),
        abierta_en: chrono::Utc::now().timestamp() - 60,
        ventanas: 1,
        actividad_no_vista: false,
        reconexion: None,
        titulo: None,
        aviso: None,
        tmux: None,
        escritura_varios: false,
    }
}

impl Prueba {
    fn host_id(&self, nombre: &str) -> i64 {
        self.app
            .hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .map(|host| host.id)
            .unwrap_or_else(|| panic!("host {nombre} no sembrado"))
    }

    /// Entra en la vista Sesión con una pestaña de web-01.
    fn entrar_en_sesion(&mut self) {
        let host = self.host_id("web-01");
        self.servidor(MensajeServidor::Bienvenida {
            version: VERSION_PROTOCOLO,
            version_app: "1.5.0".to_string(),
            pid: 4242,
            cliente_id: 1,
            clientes: 1,
            sesiones: vec![sesion(1, host, "web-01")],
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: Vec::new(),
        });
        self.tecla(KeyCode::F(3));
        self.tecla(KeyCode::Enter);
        assert_eq!(self.app.vista, Vista::Sesion);
    }

    fn prefijo(&mut self, codigo: KeyCode) {
        self.evento(Evento::Tecla(KeyEvent::new(
            KeyCode::Char(']'),
            KeyModifiers::CONTROL,
        )));
        self.tecla(codigo);
    }

    /// Entra en el historial de la pestaña con estas líneas (sin envolver).
    fn abrir_historial(&mut self, lineas: &[String]) {
        self.drenar();
        self.prefijo(KeyCode::Char('['));
        let peticion_id = self
            .drenar()
            .into_iter()
            .find_map(|mensaje| match mensaje {
                MensajeCliente::CapturarHistorial { peticion_id, .. } => Some(peticion_id),
                _ => None,
            })
            .expect("sin CapturarHistorial");
        let lineas = lineas
            .iter()
            .map(|texto| magi_nucleo::terminal::historial::LineaHistorial {
                tramos: vec![magi_nucleo::terminal::tramos::Tramo {
                    texto: texto.clone(),
                    primer_plano: magi_nucleo::terminal::tramos::Color::Predeterminado,
                    fondo: magi_nucleo::terminal::tramos::Color::Predeterminado,
                    negrita: false,
                    cursiva: false,
                    subrayado: false,
                    inverso: false,
                }],
                envuelta: false,
            })
            .collect::<Vec<_>>();
        let total = lineas.len() as u32;
        self.servidor(MensajeServidor::HistorialLote {
            peticion_id,
            sesion_id: 1,
            lote: 0,
            ultimo: true,
            total,
            lineas,
        });
        assert!(self.app.historial.is_some(), "no se abrió el historial");
    }
}

// ------------------------------------------------------------ copiar: la API

/// `termux-clipboard-set` recibe el texto por la entrada estándar, exacto y sin
/// pasar por los argumentos, y la barra dice «copiado (3,2 KiB)».
#[test]
fn copiar_por_la_api_manda_el_texto_por_la_entrada_estandar() {
    let mut prueba = Prueba::nueva();
    let url = url_de(3277, "https://ejemplo.org/?a=$(reboot)&b=`id`;\"'ñ\n#");
    assert_eq!(url.len(), 3277);
    prueba.copiar_url(&url);
    assert_eq!(
        prueba.mensaje(),
        Some(("copiando…".to_string(), false)),
        "mientras la orden corre, la barra lo dice"
    );
    prueba.esperar_copia();

    assert_eq!(
        prueba.mensaje(),
        Some(("copiado (3,2 KiB)".to_string(), false))
    );
    let llamadas = prueba.llamadas("termux-clipboard-set");
    assert_eq!(llamadas.len(), 1, "una sola orden");
    assert!(
        llamadas[0].args.is_empty(),
        "el texto nunca va en los argumentos: {:?}",
        llamadas[0].args
    );
    assert_eq!(llamadas[0].entrada, url.as_bytes(), "entrada exacta");
    assert!(prueba.osc52().is_empty(), "con la API no se manda OSC 52");
    assert_eq!(prueba.app.termux.copias_en_curso, 0);
    assert!(prueba.app.termux.cola_barra.is_empty(), "sin avisos de más");
    assert_eq!(prueba.api.estado(), EstadoApi::Disponible);
    assert!(prueba.app.dialogo.is_none(), "la copia cierra el diálogo");
}

/// Hasta 512 KiB va por la orden; con más, ni se intenta.
#[test]
fn el_tope_de_la_api_son_512_kib() {
    let mut prueba = Prueba::nueva();
    let url = url_de(TOPE_API, "https://ejemplo.org/");
    prueba.copiar_url(&url);
    prueba.esperar_copia();
    assert_eq!(
        prueba.mensaje(),
        Some(("copiado (512 KiB)".to_string(), false))
    );
    let llamadas = prueba.llamadas("termux-clipboard-set");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(llamadas[0].entrada.len(), TOPE_API, "entera, sin recortar");
}

/// Más de 512 KiB con la API disponible: aviso con el tamaño y la alternativa,
/// sin llamar a la orden ni mandar OSC 52.
#[test]
fn mas_de_512_kib_con_la_api_pide_guardarlo_en_un_fichero() {
    let mut prueba = Prueba::nueva();
    prueba.copiar_url(&url_de(900 * 1024, "https://ejemplo.org/"));
    assert_eq!(
        prueba.mensaje(),
        Some((
            "demasiado para el portapapeles (900 KiB): guárdalo en un fichero".to_string(),
            true
        )),
        "sin esperar a nada: no hay orden que lanzar"
    );
    prueba.drenar();
    prueba
        .app
        .bombear(&mut prueba.terminal, Duration::from_millis(200))
        .unwrap();
    assert!(prueba.llamadas("termux-clipboard-set").is_empty());
    assert!(prueba.osc52().is_empty(), "ni OSC 52 con la API disponible");
    assert_eq!(prueba.app.termux.copias_en_curso, 0);
    assert!(!prueba.api.apagada(), "no es un fallo de la API");
}

/// El mensaje del tamaño límite: justo un byte más de 512 KiB.
#[test]
fn un_byte_mas_de_512_kib_ya_es_demasiado() {
    let mut prueba = Prueba::nueva();
    prueba.copiar_url(&url_de(TOPE_API + 1, "https://ejemplo.org/"));
    assert_eq!(
        prueba.mensaje(),
        Some((
            "demasiado para el portapapeles (512 KiB): guárdalo en un fichero".to_string(),
            true
        ))
    );
    assert!(prueba.llamadas("termux-clipboard-set").is_empty());
}

// ------------------------------------------------------- copiar: el OSC 52

/// La API no responde: a los `plazo` se copia por OSC 52 escrito por el hilo de
/// la interfaz, la barra dice «copiado» y, una sola vez, que Termux:API se ha
/// apagado. El grupo de procesos de la orden muere.
#[test]
fn con_la_api_colgada_se_copia_por_osc_52_y_la_barra_avisa_una_vez() {
    let mut prueba = Prueba::nueva();
    prueba.con_plazo(Duration::from_millis(400));
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Colgar);
    let url = "https://ejemplo.org/corto?a=1";

    let inicio = Instant::now();
    prueba.copiar_url(url);
    assert!(
        prueba.osc52().is_empty(),
        "el OSC 52 no sale antes de que la orden falle"
    );
    prueba.esperar("OSC 52", |p| !p.osc52().is_empty());
    assert!(
        inicio.elapsed() >= Duration::from_millis(400),
        "esperó el plazo"
    );
    assert!(
        inicio.elapsed() < Duration::from_secs(10),
        "y no se quedó esperando: {:?}",
        inicio.elapsed()
    );

    // Lo escribe el hilo que procesa los eventos (el de la interfaz), no la
    // tarea que lanzó la orden.
    assert_eq!(prueba.osc52().len(), 1);
    assert_eq!(prueba.osc52()[0].0, std::thread::current().id());
    assert_eq!(prueba.osc52()[0].1, secuencia_osc52(url.as_bytes()));
    assert_eq!(prueba.mensaje(), Some(("copiado".to_string(), false)));
    assert!(prueba.api.apagada());
    assert_eq!(prueba.app.termux.copias_en_curso, 0);

    // El aviso espera su turno: primero el resultado y luego, solo una vez, el
    // aviso de que Termux:API se apagó.
    let dicho = prueba.vaciar_barra();
    assert_eq!(
        dicho,
        vec![
            ("copiado".to_string(), false),
            (AVISO_API_APAGADA.to_string(), true)
        ]
    );
    assert_eq!(AVISO_API_APAGADA, AVISO_API);

    // La orden dejó de intentarse: la segunda copia va directa al OSC 52, sin
    // tarea y sin repetir el aviso.
    prueba.copiar_url("https://ejemplo.org/segunda");
    assert_eq!(prueba.mensaje(), Some(("copiado".to_string(), false)));
    assert_eq!(prueba.osc52().len(), 2);
    assert_eq!(
        prueba.llamadas("termux-clipboard-set").len(),
        1,
        "con la API apagada ni se llama"
    );
    assert_eq!(
        prueba.vaciar_barra(),
        vec![("copiado".to_string(), false)],
        "el aviso sale una sola vez"
    );

    // El grupo de procesos del doble (y su hijo) murió con el plazo.
    let llamada = &prueba.llamadas("termux-clipboard-set")[0];
    assert!(!llamada.pids.is_empty(), "el doble anotó sus pids");
    for pid in &llamada.pids {
        assert!(
            !sigue_vivo_tras(*pid, Duration::from_secs(3)),
            "el proceso {pid} sigue vivo tras el plazo"
        );
    }
}

/// Un fallo de la orden (código distinto de 0) también cae al OSC 52 y apaga la
/// API una sola vez, aunque haya varias copias en vuelo.
#[test]
fn un_fallo_de_la_orden_cae_al_osc_52_y_apaga_la_api() {
    let mut prueba = Prueba::nueva();
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Fallar);
    prueba.copiar_url("https://ejemplo.org/uno");
    prueba.esperar_copia();
    assert_eq!(prueba.mensaje(), Some(("copiado".to_string(), false)));
    assert_eq!(prueba.osc52().len(), 1);
    assert!(prueba.api.apagada());
    let dicho = prueba.vaciar_barra();
    assert_eq!(
        dicho.iter().filter(|(m, _)| m == AVISO_API).count(),
        1,
        "{dicho:?}"
    );
}

/// Dos copias seguidas con la API colgada: la primera cae al OSC 52 al vencer
/// el plazo y la segunda, que esperaba su turno, va detrás (la API ya está
/// apagada: directa al OSC 52, sin lanzar otra orden). Y el aviso sale una sola
/// vez.
#[test]
fn dos_copias_a_la_vez_con_la_api_colgada_avisan_una_sola_vez() {
    let mut prueba = Prueba::nueva();
    prueba.con_plazo(Duration::from_millis(400));
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Colgar);
    prueba.copiar_url("https://ejemplo.org/a");
    prueba.copiar_url("https://ejemplo.org/b");
    assert_eq!(prueba.app.termux.copias_en_curso, 1, "una sola en vuelo");
    assert!(
        prueba.app.termux.copia_pendiente.is_some(),
        "la otra espera"
    );
    prueba.esperar("las dos copias", |p| p.osc52().len() == 2);
    assert_eq!(prueba.app.termux.copias_en_curso, 0);
    assert!(prueba.app.termux.copia_pendiente.is_none());
    assert_eq!(
        prueba.llamadas("termux-clipboard-set").len(),
        1,
        "la segunda no volvió a llamar a una API ya apagada"
    );
    assert_eq!(
        prueba.osc52()[0].1,
        secuencia_osc52(b"https://ejemplo.org/a"),
        "primero lo que se pidió primero"
    );
    assert_eq!(
        prueba.osc52()[1].1,
        secuencia_osc52(b"https://ejemplo.org/b")
    );
    let dicho = prueba.vaciar_barra();
    assert_eq!(
        dicho.iter().filter(|(m, _)| m == AVISO_API).count(),
        1,
        "{dicho:?}"
    );
}

/// Sin el paquete `termux-api` (la orden no está en el `PATH`) pasa lo mismo.
#[test]
fn sin_la_orden_instalada_se_copia_por_osc_52() {
    let mut prueba = Prueba::nueva();
    prueba.dobles.quitar("termux-clipboard-set");
    prueba.copiar_url("https://ejemplo.org/sin-api");
    prueba.esperar_copia();
    assert_eq!(prueba.mensaje(), Some(("copiado".to_string(), false)));
    assert_eq!(prueba.osc52().len(), 1);
    assert_eq!(prueba.api.estado(), EstadoApi::Ausente);
    let dicho = prueba.vaciar_barra();
    assert_eq!(dicho.iter().filter(|(m, _)| m == AVISO_API).count(), 1);
}

/// Termux:API parada y una selección de 20 KiB: la barra dice que no cabe y no
/// se manda ningún OSC 52.
#[test]
fn veinte_kib_sin_la_api_no_caben_y_no_se_manda_osc_52() {
    let mut prueba = Prueba::nueva();
    prueba.api.apagar(EstadoApi::NoResponde);
    prueba.copiar_url(&url_de(20 * 1024, "https://ejemplo.org/"));
    assert_eq!(
        prueba.mensaje(),
        Some((
            "no cabe en el portapapeles de Termux (20 KiB): instala Termux:API o guárdalo"
                .to_string(),
            true
        ))
    );
    assert!(prueba.osc52().is_empty(), "no se manda OSC 52");
    assert!(prueba.llamadas("termux-clipboard-set").is_empty());
    assert!(prueba.app.termux.cola_barra.is_empty());
}

/// La misma selección de 20 KiB que sale del modo historial (la tecla `y`).
#[test]
fn copiar_del_historial_sigue_el_mismo_camino() {
    let mut prueba = Prueba::nueva();
    prueba.entrar_en_sesion();
    // 3 líneas de 6 826 caracteres y 2 saltos: 20 480 bytes.
    let lineas: Vec<String> = (0..3).map(|_| "x".repeat(6826)).collect();
    prueba.abrir_historial(&lineas);
    prueba.tecla(KeyCode::Char('g'));
    prueba.tecla(KeyCode::Char('V'));
    prueba.tecla(KeyCode::Char('G'));
    let texto = prueba
        .app
        .historial
        .as_ref()
        .expect("historial")
        .texto_a_copiar();
    assert_eq!(texto.len(), 20 * 1024, "{}", texto.len());

    // Con la API disponible: va por la orden.
    prueba.tecla(KeyCode::Char('y'));
    prueba.esperar_copia();
    assert_eq!(
        prueba.mensaje(),
        Some(("copiado (20 KiB)".to_string(), false))
    );
    assert_eq!(
        prueba.llamadas("termux-clipboard-set")[0].entrada,
        texto.as_bytes()
    );

    // Con la API apagada: no cabe en el OSC 52.
    prueba.api.apagar(EstadoApi::NoResponde);
    prueba.tecla(KeyCode::Char('y'));
    assert_eq!(
        prueba.mensaje(),
        Some((
            "no cabe en el portapapeles de Termux (20 KiB): instala Termux:API o guárdalo"
                .to_string(),
            true
        ))
    );
    assert!(prueba.osc52().is_empty());
}

/// Las copias se serializan: una orden en vuelo y, detrás, solo la última que
/// se pidió. Una orden vieja y lenta no puede escribir después de una nueva, y
/// el portapapeles queda con lo último que se pidió.
#[test]
fn las_copias_se_serializan_y_el_portapapeles_queda_con_la_ultima() {
    assert_eq!(COPIAS_A_LA_VEZ, 1);
    let mut prueba = Prueba::nueva();
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Lento(6));
    prueba.copiar_url("https://ejemplo.org/vieja");
    prueba.copiar_url("https://ejemplo.org/descartada");
    prueba.copiar_url("https://ejemplo.org/ultima");
    assert_eq!(prueba.app.termux.copias_en_curso, 1);
    assert!(prueba.app.termux.copia_pendiente.is_some());
    // Mientras la primera sigue en vuelo no sale ninguna otra orden.
    prueba
        .app
        .bombear(&mut prueba.terminal, Duration::from_millis(250))
        .unwrap();
    assert_eq!(
        prueba.llamadas("termux-clipboard-set").len(),
        1,
        "solo la que está en vuelo"
    );
    assert_eq!(prueba.app.termux.copias_en_curso, 1);

    prueba.esperar_copia();
    let entradas: Vec<String> = prueba
        .llamadas("termux-clipboard-set")
        .into_iter()
        .map(|llamada| String::from_utf8(llamada.entrada).unwrap())
        .collect();
    assert_eq!(
        entradas,
        vec![
            "https://ejemplo.org/vieja".to_string(),
            "https://ejemplo.org/ultima".to_string()
        ],
        "la descartada no se copia y la última va después de la vieja"
    );
    assert_eq!(
        prueba.mensaje(),
        Some((
            format!(
                "copiado ({})",
                tamano_kib("https://ejemplo.org/ultima".len())
            ),
            false
        ))
    );
    assert!(prueba.app.termux.copia_pendiente.is_none());
    assert_eq!(prueba.app.termux.copias_en_curso, 0);
}

/// Con una orden en vuelo, ni el OSC 52 se cuela por delante: una copia que ya
/// iría directa (la API se apagó mientras tanto) espera su turno y sale después.
#[test]
fn con_una_orden_en_vuelo_nada_pasa_por_delante() {
    let mut prueba = Prueba::nueva();
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Lento(6));
    prueba.copiar_url("https://ejemplo.org/primera");
    // Algo apaga la API mientras la orden corre (otra orden que no respondió).
    prueba.api.apagar(EstadoApi::NoResponde);
    prueba.copiar_url("https://ejemplo.org/segunda");
    assert!(
        prueba.osc52().is_empty(),
        "el OSC 52 de la segunda no sale antes de que acabe la primera"
    );
    prueba.esperar("la segunda copia", |p| !p.osc52().is_empty());
    assert_eq!(prueba.llamadas("termux-clipboard-set").len(), 1);
    assert_eq!(
        prueba.osc52()[0].1,
        secuencia_osc52(b"https://ejemplo.org/segunda")
    );
    assert_eq!(prueba.app.termux.copias_en_curso, 0);
}

/// Una copia descartada porque llegó otra después lo dice por su `ParaQue`:
/// la clave pública enseña la línea para copiarla a mano.
#[test]
fn una_copia_descartada_dice_por_que_con_su_respaldo() {
    let mut prueba = Prueba::nueva();
    let publica = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEjemploEjemploEjemplo usuario@ejemplo.org";
    prueba.con_identidad("id_prueba", "ssh-ed25519", publica);
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Lento(6));
    prueba.tecla(KeyCode::F(5));
    assert_eq!(prueba.app.vista, Vista::Identidades);
    prueba.tecla(KeyCode::Char('c')); // en vuelo
    prueba.tecla(KeyCode::Char('c')); // espera
    assert!(prueba.app.dialogo.is_none());
    prueba.tecla(KeyCode::Char('c')); // sustituye a la que esperaba
    match &prueba.app.dialogo {
        Some(Dialogo::Detalle { titulo, lineas, .. }) => {
            assert_eq!(titulo, "CLAVE PÚBLICA");
            assert_eq!(
                lineas[0],
                "No se pudo copiar al portapapeles (sustituida por una copia más reciente)."
            );
            assert_eq!(lineas.last().map(String::as_str), Some(publica));
        }
        otro => panic!("sin el diálogo de la descartada: {}", otro.is_some()),
    }
    prueba.esperar_copia();
    assert_eq!(
        prueba.llamadas("termux-clipboard-set").len(),
        2,
        "la que estaba en vuelo y la última"
    );
}

/// El límite del OSC 52: 6 000 bytes ya en base64 (4 500 de texto).
#[test]
fn el_osc_52_cabe_hasta_6000_bytes_codificados() {
    assert_eq!(TOPE_OSC52, 6000, "PROVISIONAL-POC");
    let mut prueba = Prueba::nueva();
    prueba.api.apagar(EstadoApi::Ausente);
    let cabe = url_de(4500, "https://ejemplo.org/");
    prueba.copiar_url(&cabe);
    assert_eq!(prueba.mensaje(), Some(("copiado".to_string(), false)));
    assert_eq!(prueba.osc52().len(), 1);
    assert_eq!(prueba.osc52()[0].1, secuencia_osc52(cabe.as_bytes()));

    prueba.copiar_url(&url_de(4501, "https://ejemplo.org/"));
    assert_eq!(
        prueba.mensaje(),
        Some((
            format!(
                "no cabe en el portapapeles de Termux ({}): instala Termux:API o guárdalo",
                tamano_kib(4501)
            ),
            true
        ))
    );
    assert_eq!(prueba.osc52().len(), 1, "el que no cabe no se manda");
}

/// Dentro de SSH (`SSH_CONNECTION`) la F15 mandaba siempre OSC 52: en Termux
/// manda este camino.
#[test]
fn en_termux_ssh_connection_no_cambia_el_camino() {
    let mut prueba = Prueba::nueva();
    let previo = std::env::var_os("SSH_CONNECTION");
    std::env::set_var("SSH_CONNECTION", "192.0.2.1 50000 192.0.2.2 22");
    prueba.copiar_url("https://ejemplo.org/por-ssh");
    prueba.esperar_copia();
    match previo {
        Some(valor) => std::env::set_var("SSH_CONNECTION", valor),
        None => std::env::remove_var("SSH_CONNECTION"),
    }
    assert_eq!(prueba.llamadas("termux-clipboard-set").len(), 1);
    assert!(prueba.osc52().is_empty());
}

// ---------------------------------------------- copiar: la copia de un host

/// Lo que copia un host (F15, con permiso) sigue el mismo camino y su aviso con
/// el nombre del host sigue ahí cuando termina.
#[test]
fn la_copia_de_un_host_sigue_el_mismo_camino_y_se_avisa() {
    let mut prueba = Prueba::nueva();
    prueba.entrar_en_sesion();
    let texto = "x".repeat(1229);
    prueba.servidor(MensajeServidor::Portapapeles {
        sesion_id: 1,
        texto: texto.clone(),
    });
    prueba.esperar_copia();
    assert_eq!(
        prueba.mensaje(),
        Some(("web-01 copió 1,2 KB al portapapeles".to_string(), false))
    );
    assert_eq!(
        prueba.llamadas("termux-clipboard-set")[0].entrada,
        texto.as_bytes()
    );

    // Con la API apagada y un texto que no cabe: no se dice que se copió.
    prueba.api.apagar(EstadoApi::NoResponde);
    prueba.servidor(MensajeServidor::Portapapeles {
        sesion_id: 1,
        texto: "y".repeat(8000),
    });
    let (mensaje, error) = prueba.mensaje().expect("mensaje");
    assert!(error, "{mensaje}");
    assert!(
        mensaje.starts_with("no cabe en el portapapeles de Termux"),
        "{mensaje}"
    );
    assert!(prueba.osc52().is_empty());

    // Y si cabe, por OSC 52, con su aviso.
    prueba.servidor(MensajeServidor::Portapapeles {
        sesion_id: 1,
        texto: "z".repeat(100),
    });
    assert_eq!(
        prueba.mensaje(),
        Some(("web-01 copió 100 B al portapapeles".to_string(), false))
    );
    assert_eq!(prueba.osc52().len(), 1);
}

// ------------------------------------------------- copiar: las tres vías

impl Prueba {
    /// Una identidad de fichero con su `.pub` en la raíz de la prueba.
    fn con_identidad(&mut self, nombre: &str, tipo: &str, publica: &str) {
        let ruta = self.raiz.join(nombre);
        std::fs::write(
            self.raiz.join(format!("{nombre}.pub")),
            format!("{publica}\n"),
        )
        .unwrap();
        self.app
            .almacen
            .crear_identidad(
                tipo,
                HUELLA,
                OrigenIdentidad::Fichero,
                Some(&ruta.display().to_string()),
                Some("usuario@ejemplo.org"),
            )
            .expect("identidad");
    }
}

/// La clave pública de una identidad (F5, `c`) pasa por la misma vía.
#[test]
fn copiar_la_publica_de_una_identidad_va_por_la_orden() {
    let mut prueba = Prueba::nueva();
    let ruta = prueba.raiz.join("id_prueba");
    let publica = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEjemploEjemploEjemplo usuario@ejemplo.org";
    std::fs::write(prueba.raiz.join("id_prueba.pub"), format!("{publica}\n")).unwrap();
    prueba
        .app
        .almacen
        .crear_identidad(
            "ssh-ed25519",
            HUELLA,
            OrigenIdentidad::Fichero,
            Some(&ruta.display().to_string()),
            Some("usuario@ejemplo.org"),
        )
        .expect("identidad");
    prueba.tecla(KeyCode::F(5));
    assert_eq!(prueba.app.vista, Vista::Identidades);
    prueba.tecla(KeyCode::Char('c'));
    prueba.esperar_copia();
    assert_eq!(
        prueba.mensaje(),
        Some((format!("copiado ({})", tamano_kib(publica.len())), false))
    );
    let llamadas = prueba.llamadas("termux-clipboard-set");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(llamadas[0].entrada, publica.as_bytes());
    assert!(llamadas[0].args.is_empty());
    assert!(prueba.app.dialogo.is_none());

    // Si no se puede copiar (no cabe), el diálogo de siempre enseña la línea.
    prueba.api.apagar(EstadoApi::NoResponde);
    prueba.tecla(KeyCode::Char('c'));
    assert_eq!(
        prueba.mensaje().map(|(m, _)| m),
        Some("copiado".to_string())
    );
    assert_eq!(prueba.osc52().len(), 1, "la pública cabe en el OSC 52");
}

/// Una clave pública que no cabe en ningún camino abre el diálogo para copiarla
/// a mano (lo de siempre cuando no hay portapapeles).
#[test]
fn una_publica_que_no_se_puede_copiar_abre_el_dialogo_de_siempre() {
    let mut prueba = Prueba::nueva();
    let ruta = prueba.raiz.join("id_grande");
    // Una «clave» enorme: no cabe en 6 000 bytes de OSC 52.
    let publica = format!("ssh-rsa {} usuario@ejemplo.org", "A".repeat(9000));
    std::fs::write(prueba.raiz.join("id_grande.pub"), format!("{publica}\n")).unwrap();
    prueba
        .app
        .almacen
        .crear_identidad(
            "ssh-rsa",
            HUELLA,
            OrigenIdentidad::Fichero,
            Some(&ruta.display().to_string()),
            None,
        )
        .expect("identidad");
    prueba.api.apagar(EstadoApi::NoResponde);
    prueba.tecla(KeyCode::F(5));
    prueba.tecla(KeyCode::Char('c'));
    match &prueba.app.dialogo {
        Some(Dialogo::Detalle { titulo, lineas, .. }) => {
            assert_eq!(titulo, "CLAVE PÚBLICA");
            assert!(lineas[0].starts_with("No se pudo copiar al portapapeles (no cabe"));
            assert_eq!(lineas.last().map(String::as_str), Some(publica.as_str()));
        }
        otro => panic!("sin el diálogo de la clave pública: {}", otro.is_some()),
    }
    assert!(prueba.osc52().is_empty());
}

fn clave_generada(publica: &str, solicito_copia: bool) -> ClaveGenerada {
    ClaveGenerada {
        tipo: "ssh-ed25519".to_string(),
        huella: HUELLA.to_string(),
        ruta: PathBuf::from("/home/usuario/.ssh/id_nueva"),
        publica: publica.to_string(),
        comentario: Some("usuario@ejemplo.org".to_string()),
        en_agente: true,
        aviso_agente: None,
        copiada: false,
        solicito_copia,
        fallo_copia: None,
    }
}

/// La clave recién generada se copia desde la `App`, no desde la tarea.
#[test]
fn la_clave_generada_se_copia_por_la_misma_via() {
    let mut prueba = Prueba::nueva();
    let publica = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGeneradaGeneradaGen usuario@ejemplo.org\n";
    prueba.evento(Evento::ClaveGenerada(Ok(clave_generada(publica, true))));
    prueba.esperar_copia();
    assert_eq!(
        prueba.mensaje(),
        Some((
            "clave ssh-ed25519 generada en /home/usuario/.ssh/id_nueva · añadida al agente · pública copiada al portapapeles"
                .to_string(),
            false
        ))
    );
    let llamadas = prueba.llamadas("termux-clipboard-set");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(llamadas[0].entrada, publica.as_bytes());
    assert!(prueba.app.dialogo.is_none());

    // Sin pedir la copia, no se copia nada.
    prueba.evento(Evento::ClaveGenerada(Ok(clave_generada(publica, false))));
    prueba
        .app
        .bombear(&mut prueba.terminal, Duration::from_millis(150))
        .unwrap();
    assert_eq!(prueba.llamadas("termux-clipboard-set").len(), 1);
    let (mensaje, _) = prueba.mensaje().expect("mensaje");
    assert!(!mensaje.contains("copiada"), "{mensaje}");

    // Y si no se pudo copiar, el diálogo con la línea a mano.
    prueba.api.apagar(EstadoApi::NoResponde);
    let grande = format!("ssh-rsa {} usuario@ejemplo.org\n", "A".repeat(9000));
    prueba.evento(Evento::ClaveGenerada(Ok(clave_generada(&grande, true))));
    match &prueba.app.dialogo {
        Some(Dialogo::Detalle { titulo, lineas, .. }) => {
            assert_eq!(titulo, "CLAVE PÚBLICA");
            assert_eq!(
                lineas[0],
                "No se pudo copiar al portapapeles; copia la línea a mano:"
            );
            assert_eq!(lineas[2], grande.trim());
        }
        otro => panic!("sin el diálogo de la clave pública: {}", otro.is_some()),
    }
    let (mensaje, _) = prueba.mensaje().expect("mensaje");
    assert!(!mensaje.contains("pública copiada"), "{mensaje}");
}

fn info_escritorio(id: u32, host_id: i64, estado: EstadoEscritorio) -> InfoEscritorio {
    let ahora = chrono::Utc::now().timestamp();
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
        ancho: 1920,
        alto: 1080,
        imagen: None,
        fps: 0,
        directo: false,
        a_traves_de: None,
        desde: ahora - 60,
        redimensionable: false,
    }
}

impl Prueba {
    /// Siembra un RDP y abre su pestaña ▣ (lo que difunde el servidor); devuelve
    /// la petición de apertura y el id del escritorio.
    fn abrir_escritorio(&mut self) -> (u64, u32) {
        let host_id = self
            .app
            .almacen
            .crear_host(
                &DatosHost {
                    nombre: "win-01".to_string(),
                    direccion: "192.0.2.30".to_string(),
                    puerto: 3389,
                    tipo: TipoHost::Rdp,
                    escritorio_opciones: OpcionesEscritorio {
                        tamano: TamanoEscritorio::Fijo {
                            ancho: 1920,
                            alto: 1080,
                        },
                        ..OpcionesEscritorio::default()
                    },
                    usuario: Some("usuario".to_string()),
                    identidad_ref: IdentidadRef::Contrasena,
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("host de escritorio");
        self.app
            .almacen
            .fijar_huella_escritorio(host_id, Some(HUELLA))
            .expect("huella");
        self.app.recargar_inventario().expect("inventario");
        self.tecla(KeyCode::F(2));
        self.app.seleccionar_host_id(host_id);
        self.drenar();
        self.tecla(KeyCode::Enter);
        let peticion = self
            .drenar()
            .into_iter()
            .find_map(|mensaje| match mensaje {
                MensajeCliente::AbrirEscritorio { peticion_id, .. } => Some(peticion_id),
                _ => None,
            })
            .expect("sin AbrirEscritorio");
        assert!(peticion >= RANGO_ESCRITORIOS);
        let escritorio_id = 7;
        self.servidor(MensajeServidor::Escritorios {
            lista: vec![info_escritorio(
                escritorio_id,
                host_id,
                EstadoEscritorio::Abriendo,
            )],
        });
        assert_eq!(self.app.vista, Vista::Sesion, "no se abrió la pestaña");
        (peticion, escritorio_id)
    }
}

/// El enlace del visor (`y` en la pestaña de un escritorio) pasa por la misma
/// vía y queda a la vista si no se puede copiar.
#[test]
fn copiar_el_enlace_del_visor_va_por_la_misma_via() {
    let mut prueba = Prueba::nueva();
    let (_, escritorio_id) = prueba.abrir_escritorio();
    let enlace = "http://127.0.0.1:8080/entrar?t=enlace-de-prueba";

    prueba.tecla(KeyCode::Char('y'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin PedirEnlace");
    prueba.servidor(MensajeServidor::Enlace {
        peticion_id: peticion,
        escritorio_id,
        enlace: enlace.to_string(),
    });
    prueba.esperar_copia();
    assert_eq!(
        prueba.mensaje(),
        Some((format!("copiado ({})", tamano_kib(enlace.len())), false))
    );
    let llamadas = prueba.llamadas("termux-clipboard-set");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(llamadas[0].entrada, enlace.as_bytes());
    assert!(
        prueba.app.enlace_visible.is_none(),
        "copiado: nada a la vista"
    );

    // Si el enlace no se puede copiar, se queda a la vista en la pestaña.
    prueba.api.apagar(EstadoApi::NoResponde);
    let enorme = format!("http://127.0.0.1:8080/entrar?t={}", "z".repeat(7000));
    prueba.tecla(KeyCode::Char('y'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin PedirEnlace");
    prueba.servidor(MensajeServidor::Enlace {
        peticion_id: peticion,
        escritorio_id,
        enlace: enorme.clone(),
    });
    let visible = prueba
        .app
        .enlace_visible
        .clone()
        .expect("enlace a la vista");
    assert_eq!(
        (visible.0, visible.1.as_str()),
        (escritorio_id, enorme.as_str())
    );
    let (mensaje, error) = prueba.mensaje().expect("mensaje");
    assert!(error);
    assert!(
        mensaje.starts_with("no se pudo copiar el enlace: no cabe en el portapapeles de Termux"),
        "{mensaje}"
    );
    assert!(mensaje.ends_with("está en la pestaña"), "{mensaje}");
}

/// El enlace es del escritorio que se pidió, no de la pestaña que esté activa
/// cuando vuelve la respuesta del servidor.
#[test]
fn el_enlace_copiado_es_del_escritorio_de_la_peticion() {
    let mut prueba = Prueba::nueva();
    let (_, escritorio_id) = prueba.abrir_escritorio();
    prueba.api.apagar(EstadoApi::NoResponde);
    prueba.tecla(KeyCode::Char('y'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin PedirEnlace");
    // Mientras el servidor contesta, el usuario deja la pestaña del escritorio.
    prueba.app.pestana_activa = None;
    assert_eq!(prueba.app.escritorio_activo_id(), None);
    let enorme = format!("http://127.0.0.1:8080/entrar?t={}", "z".repeat(7000));
    prueba.servidor(MensajeServidor::Enlace {
        peticion_id: peticion,
        escritorio_id,
        enlace: enorme.clone(),
    });
    let visible = prueba
        .app
        .enlace_visible
        .clone()
        .expect("el enlace quedó a la vista");
    assert_eq!(
        (visible.0, visible.1.as_str()),
        (escritorio_id, enorme.as_str()),
        "del escritorio pedido, no del activo (ninguno)"
    );
}

/// El resultado de una copia por la orden llega hasta `api_plazo_seg` después:
/// si el usuario tiene entonces otro diálogo abierto, el respaldo de la clave
/// pública no lo pisa, queda debajo y sale al cerrarlo.
#[test]
fn el_respaldo_de_una_copia_tardia_no_pisa_el_dialogo_abierto() {
    let mut prueba = Prueba::nueva();
    prueba.con_plazo(Duration::from_millis(400));
    // Una «clave» enorme: no cabe en 6 000 bytes de OSC 52 cuando la orden
    // (colgada) falla.
    let publica = format!("ssh-rsa {} usuario@ejemplo.org", "A".repeat(9000));
    prueba.con_identidad("id_grande", "ssh-rsa", &publica);
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Colgar);
    prueba.tecla(KeyCode::F(5));
    prueba.tecla(KeyCode::Char('c'));
    assert_eq!(prueba.app.termux.copias_en_curso, 1);
    // Mientras la orden corre, el usuario abre otra cosa.
    prueba.app.dialogo = Some(Dialogo::Enlace(DialogoEnlace::nuevo(
        "https://ejemplo.org/otro".to_string(),
    )));
    prueba.esperar("el resultado de la copia", |p| {
        p.app.termux.copias_en_curso == 0
    });
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Enlace(_))),
        "el diálogo del usuario sigue encima"
    );
    assert_eq!(prueba.app.dialogos_apilados(), 1, "el respaldo, debajo");
    let (mensaje, error) = prueba.mensaje().expect("la barra lo dice");
    assert!(error);
    assert!(
        mensaje.starts_with("no se pudo copiar la clave pública: no cabe en el portapapeles"),
        "{mensaje}"
    );
    // Al cerrar el suyo, sale el respaldo con la línea.
    prueba.tecla(KeyCode::Esc);
    match &prueba.app.dialogo {
        Some(Dialogo::Detalle { titulo, lineas, .. }) => {
            assert_eq!(titulo, "CLAVE PÚBLICA");
            assert_eq!(lineas.last().map(String::as_str), Some(publica.as_str()));
        }
        otro => panic!("sin el respaldo al cerrar: {}", otro.is_some()),
    }
    assert_eq!(prueba.app.dialogos_apilados(), 0);
}

// --------------------------------------------------------- enlaces y visor

/// `[terminal] abrir_enlaces` vale `termux-open-url` por defecto en Termux.
#[test]
fn abrir_enlaces_vale_termux_open_url_por_defecto() {
    como_termux();
    assert_eq!(config_termux().terminal.abrir_enlaces, "termux-open-url");
    assert_eq!(
        magi::config::abrir_enlaces_por_defecto_para(true),
        "termux-open-url"
    );
    assert_ne!(
        magi::config::abrir_enlaces_por_defecto_para(false),
        "termux-open-url",
        "en el escritorio, igual que hoy"
    );
    // El plazo por defecto de las órdenes de Termux son 5 s.
    assert_eq!(config_termux().termux.api_plazo(), Duration::from_secs(5));
}

/// Un enlace se abre con la orden, con la URL como único argumento y sin shell
/// (una URL con metacaracteres llega tal cual).
#[test]
fn un_enlace_se_abre_con_termux_open_url_sin_shell() {
    let mut prueba = Prueba::nueva();
    let url = "https://ejemplo.org/a?b=1&c=$(reboot);`id`'\"|x#frag";
    prueba.abrir_url(url);
    assert_eq!(prueba.mensaje(), Some((format!("abriendo {url}"), false)));
    prueba.esperar("la orden", |p| !p.llamadas("termux-open-url").is_empty());
    prueba
        .app
        .bombear(&mut prueba.terminal, Duration::from_millis(200))
        .unwrap();
    let llamadas = prueba.llamadas("termux-open-url");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(llamadas[0].args, vec![url.to_string()], "la URL, sola");
    assert!(llamadas[0].entrada.is_empty());
    assert_eq!(
        prueba.mensaje(),
        Some((format!("abriendo {url}"), false)),
        "sin error: sigue diciendo que abre"
    );
    assert!(!prueba.api.apagada(), "termux-open-url no es de la API");
}

/// Si la orden falla, la barra lo dice con la salida de la orden: una línea, sin
/// controles.
#[test]
fn si_la_orden_de_abrir_falla_la_barra_dice_por_que() {
    let mut prueba = Prueba::nueva();
    prueba.dobles.programar("termux-open-url", Modo::Fallar);
    prueba.abrir_url("https://ejemplo.org/");
    prueba.esperar("el fallo", |p| {
        p.mensaje()
            .is_some_and(|(m, _)| m.starts_with("no se pudo abrir"))
    });
    assert_eq!(
        prueba.mensaje(),
        Some((
            "no se pudo abrir el enlace: fallo simulado de termux-open-url".to_string(),
            true
        ))
    );

    // Una salida larga y con controles: una sola línea limpia de 200 como mucho.
    prueba.dobles.programar("termux-open-url", Modo::ErrorLargo);
    prueba.abrir_url("https://ejemplo.org/");
    prueba.esperar("el fallo largo", |p| {
        p.mensaje()
            .is_some_and(|(m, _)| m.starts_with("no se pudo abrir") && m.contains("fallo"))
    });
    let (mensaje, error) = prueba.mensaje().unwrap();
    assert!(error);
    assert!(!mensaje.chars().any(char::is_control), "{mensaje:?}");
    let motivo = mensaje
        .strip_prefix("no se pudo abrir el enlace: ")
        .expect("prefijo");
    assert!(motivo.chars().count() <= 200, "{}", motivo.chars().count());

    // Sin la orden instalada.
    prueba.dobles.quitar("termux-open-url");
    prueba.abrir_url("https://ejemplo.org/");
    prueba.esperar("sin orden", |p| {
        p.mensaje()
            .is_some_and(|(m, _)| m.contains("no está instalado"))
    });
    assert_eq!(
        prueba.mensaje(),
        Some((
            "no se pudo abrir el enlace: termux-open-url no está instalado".to_string(),
            true
        ))
    );
}

/// La orden que no responde no deja a MAGI esperando: a los `plazo` se dice.
#[test]
fn si_la_orden_de_abrir_no_responde_no_se_queda_esperando() {
    let mut prueba = Prueba::nueva();
    prueba.con_plazo(Duration::from_millis(400));
    prueba.dobles.programar("termux-open-url", Modo::Colgar);
    let inicio = Instant::now();
    prueba.abrir_url("https://ejemplo.org/");
    // La interfaz no se queda esperando a la orden.
    assert!(
        inicio.elapsed() < Duration::from_millis(300),
        "{:?}",
        inicio.elapsed()
    );
    prueba.esperar("sin respuesta", |p| {
        p.mensaje().is_some_and(|(m, _)| m.contains("no responde"))
    });
    assert_eq!(
        prueba.mensaje(),
        Some(("no se pudo abrir el enlace: no responde".to_string(), true))
    );
    assert!(!prueba.api.apagada(), "no apaga Termux:API: no es suya");
    for pid in &prueba.llamadas("termux-open-url")[0].pids {
        assert!(!sigue_vivo_tras(*pid, Duration::from_secs(3)));
    }
}

/// Lo que no es http o https no se abre; un programa con ruta se lanza como en
/// el escritorio (sin las órdenes de Termux).
#[test]
fn solo_http_y_https_y_un_programa_con_ruta_va_como_en_el_escritorio() {
    let mut prueba = Prueba::nueva();
    prueba.abrir_url("ftp://ejemplo.org/x");
    assert_eq!(
        prueba.mensaje(),
        Some(("solo se abren enlaces http y https".to_string(), true))
    );
    assert!(prueba.llamadas("termux-open-url").is_empty());

    let mut config = config_termux();
    config.terminal.abrir_enlaces = "/data/data/com.termux/files/usr/bin/termux-open-url".into();
    let mut prueba = Prueba::con(config);
    prueba.abrir_url("https://ejemplo.org/");
    assert_eq!(
        prueba.mensaje(),
        Some(("abriría https://ejemplo.org/".to_string(), false)),
        "sin efectos externos, como en el escritorio"
    );
    assert!(prueba.llamadas("termux-open-url").is_empty());
}

/// `navegador = auto` en Termux es `termux-open-url {url}`.
#[test]
fn el_navegador_auto_es_termux_open_url() {
    let url = "http://127.0.0.1:8080/entrar?t=uno";
    let auto = config_termux().escritorio;
    let nunca = |_: &str| false;
    let navegador = resolver_navegador(&auto, Plataforma::Termux, url, &nunca, &nunca).unwrap();
    assert_eq!(navegador.args, vec!["termux-open-url", url]);
    assert_eq!(navegador.origen, OrigenNavegador::Termux);
    // Aunque haya un Chromium en el PATH: en Termux manda el del móvil.
    let siempre = |_: &str| true;
    let navegador = resolver_navegador(&auto, Plataforma::Termux, url, &siempre, &siempre).unwrap();
    assert_eq!(navegador.args, vec!["termux-open-url", url]);
    // Linux y macOS no cambian.
    let linux = resolver_navegador(&auto, Plataforma::Linux, url, &nunca, &nunca).unwrap();
    assert_eq!(linux.args, vec!["xdg-open", url]);
    let mac = resolver_navegador(&auto, Plataforma::Macos, url, &nunca, &nunca).unwrap();
    assert_eq!(mac.args, vec!["open", url]);

    assert_eq!(
        navegador_de_termux(&auto, url),
        Some(vec!["termux-open-url".to_string(), url.to_string()])
    );
    let mut propio = auto.clone();
    propio.navegador = "brave --app={url}".to_string();
    assert_eq!(navegador_de_termux(&propio, url), None, "una orden propia");
    assert_eq!(
        resolver_navegador(&propio, Plataforma::Termux, url, &nunca, &nunca)
            .unwrap()
            .args,
        vec!["brave", &format!("--app={url}")],
        "la orden propia manda también en Termux"
    );
}

/// El visor de un escritorio se abre con `termux-open-url` y, si falla, el
/// enlace se queda a la vista para copiarlo.
#[test]
fn el_visor_de_un_escritorio_se_abre_con_termux_open_url() {
    let mut prueba = Prueba::nueva();
    let (peticion, escritorio_id) = prueba.abrir_escritorio();
    let enlace = "http://127.0.0.1:8080/entrar?t=enlace-de-prueba";
    prueba.servidor(MensajeServidor::EscritorioAbierto {
        peticion_id: peticion,
        escritorio_id,
        enlace: enlace.to_string(),
    });
    assert_eq!(
        prueba.mensaje(),
        Some(("abriendo el visor…".to_string(), false))
    );
    prueba.esperar("visor abierto", |p| {
        p.mensaje()
            .is_some_and(|(m, _)| m == "visor abierto en el navegador")
    });
    let llamadas = prueba.llamadas("termux-open-url");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(llamadas[0].args, vec![enlace.to_string()]);
    assert!(prueba.app.enlace_visible.is_none());

    // Si falla, el enlace a la vista (60 s) y cómo copiarlo.
    prueba.dobles.programar("termux-open-url", Modo::Fallar);
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin PedirEnlace");
    prueba.servidor(MensajeServidor::Enlace {
        peticion_id: peticion,
        escritorio_id,
        enlace: enlace.to_string(),
    });
    prueba.esperar("visor caído", |p| p.app.enlace_visible.is_some());
    assert_eq!(
        prueba.app.enlace_reciente(escritorio_id).as_deref(),
        Some(enlace)
    );
    assert_eq!(
        prueba.mensaje(),
        Some((
            "no se pudo abrir el navegador: fallo simulado de termux-open-url; copia el enlace con y"
                .to_string(),
            true
        ))
    );
    assert!(
        prueba.texto().contains("cópialo con y"),
        "{}",
        prueba.texto()
    );
}

/// Una orden propia de `[escritorio] navegador` se lanza como en el escritorio.
#[test]
fn un_navegador_propio_en_termux_no_pasa_por_termux_open_url() {
    let mut config = config_termux();
    config.escritorio.navegador = "magi-navegador-que-no-existe --app={url}".to_string();
    let mut prueba = Prueba::con(config);
    let (peticion, escritorio_id) = prueba.abrir_escritorio();
    prueba.servidor(MensajeServidor::EscritorioAbierto {
        peticion_id: peticion,
        escritorio_id,
        enlace: "http://127.0.0.1:8080/entrar?t=otro".to_string(),
    });
    assert!(
        prueba.app.enlace_visible.is_some(),
        "el navegador propio falló"
    );
    assert!(prueba.llamadas("termux-open-url").is_empty());
}

/// `prefijo w` en Termux explica cómo abrir otra sesión en vez de lanzar un
/// terminal.
#[test]
fn prefijo_w_explica_como_abrir_otra_sesion_de_termux() {
    let mut config = config_termux();
    let marca = std::env::temp_dir().join(format!("magi-prefijo-w-{}", std::process::id()));
    config.terminal.comando = format!("touch {}", marca.display());
    let mut prueba = Prueba::con(config);
    prueba.entrar_en_sesion();
    prueba.prefijo(KeyCode::Char('w'));
    assert_eq!(
        prueba.mensaje(),
        Some((
            "abre otra sesión de Termux (desliza desde el borde izquierdo → NEW SESSION) y ejecuta magi"
                .to_string(),
            false
        )),
        "una explicación, no un error"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marca.exists(), "no se lanzó ningún terminal");
}

const EXPLICACION_W: &str =
    "abre otra sesión de Termux (desliza desde el borde izquierdo → NEW SESSION) y ejecuta magi";

/// Lo mismo desde la paleta («nueva ventana»): la explicación es información,
/// no un error en rojo.
#[test]
fn nueva_ventana_de_la_paleta_explica_igual_que_prefijo_w() {
    let mut config = config_termux();
    let marca = std::env::temp_dir().join(format!("magi-paleta-w-{}", std::process::id()));
    config.terminal.comando = format!("touch {}", marca.display());
    let mut prueba = Prueba::con(config);
    prueba.evento(Evento::Tecla(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));
    assert!(prueba.app.paleta.is_some());
    for letra in "nueva ventana".chars() {
        prueba.tecla(KeyCode::Char(letra));
    }
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.paleta.is_none(), "la paleta se cerró");
    assert_eq!(
        prueba.mensaje(),
        Some((EXPLICACION_W.to_string(), false)),
        "una explicación, no un error"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marca.exists(), "no se lanzó ningún terminal");
}

/// Fuera de Termux, un fallo al abrir la ventana sigue siendo un error por los
/// dos caminos (prefijo w y paleta).
#[test]
fn fuera_de_termux_un_fallo_al_abrir_la_ventana_es_un_error() {
    let mut config = config_termux();
    config.terminal.comando = String::new();
    let mut prueba = Prueba::con(config);
    prueba.app.termux.activo = false;
    let error = Some(("config.toml no define [terminal] comando".to_string(), true));
    // La paleta, desde Flota.
    prueba.evento(Evento::Tecla(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));
    for letra in "nueva ventana".chars() {
        prueba.tecla(KeyCode::Char(letra));
    }
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.mensaje(), error);
    // Y el prefijo, desde una sesión.
    prueba.app.mensaje = None;
    prueba.entrar_en_sesion();
    prueba.prefijo(KeyCode::Char('w'));
    assert_eq!(prueba.mensaje(), error);
}

// ------------------------------------------------- avisos del servidor

fn escribir_estado(prueba: &Prueba, arranque: i64, avisos: &[AvisoServidor]) {
    estado::escribir_servidor(
        &prueba.rutas,
        &EstadoServidorTermux {
            arranque,
            avisos: avisos.to_vec(),
        },
    )
    .expect("escribir el estado del servidor");
}

impl Prueba {
    /// Hace que el siguiente tick lea el estado del servidor y espera a que la
    /// lectura vuelva.
    fn leer_estado_del_servidor(&mut self) {
        self.app.termux.ultima_lectura = Some(Instant::now() - RITMO_ESTADO_SERVIDOR * 2);
        self.tick();
        assert!(self.app.termux.leyendo_servidor, "no se lanzó la lectura");
        self.esperar("la lectura", |p| !p.app.termux.leyendo_servidor);
    }
}

/// Cada aviso del servidor sale una vez por servidor, de uno en uno y sin
/// pisar lo que se está viendo; un servidor nuevo los repite.
#[test]
fn los_avisos_del_servidor_salen_una_vez_por_arranque() {
    let mut prueba = Prueba::nueva();
    escribir_estado(
        &prueba,
        1000,
        &[AvisoServidor::ApiApagada, AvisoServidor::SinWakeLock],
    );
    // El primer tick lee el estado (no hay nada dicho ni leído todavía).
    prueba.tick();
    assert!(prueba.app.termux.ultima_lectura.is_some());
    prueba.esperar("la lectura", |p| !p.app.termux.leyendo_servidor);
    assert_eq!(prueba.app.termux.cola_barra.len(), 2);
    assert!(prueba.mensaje().is_none(), "espera su turno hasta el tick");

    // Un error del usuario no se pisa: el aviso espera.
    prueba.app.mensaje("no se pudo hacer algo", true);
    prueba.tick();
    assert_eq!(
        prueba.mensaje(),
        Some(("no se pudo hacer algo".to_string(), true))
    );
    // Cuando caduca (el tick de 4 s), salen los avisos de uno en uno.
    prueba.app.mensaje = None;
    prueba.tick();
    assert_eq!(
        prueba.mensaje(),
        Some((AvisoServidor::ApiApagada.texto().to_string(), true))
    );
    prueba.tick();
    assert_eq!(
        prueba.mensaje().map(|(m, _)| m),
        Some(AvisoServidor::ApiApagada.texto().to_string()),
        "el segundo espera a que el primero se vaya"
    );
    prueba.app.mensaje = None;
    prueba.tick();
    assert_eq!(
        prueba.mensaje(),
        Some((AvisoServidor::SinWakeLock.texto().to_string(), true))
    );
    prueba.app.mensaje = None;
    prueba.tick();
    assert!(prueba.mensaje().is_none(), "ya los dijo todos");

    // El mismo servidor, leído otra vez: nada nuevo.
    prueba.leer_estado_del_servidor();
    prueba.app.mensaje = None;
    prueba.tick();
    assert!(prueba.mensaje().is_none());
    assert!(prueba.app.termux.cola_barra.is_empty());

    // El mismo servidor suma un aviso: solo ese.
    escribir_estado(
        &prueba,
        1000,
        &[
            AvisoServidor::ApiApagada,
            AvisoServidor::SinWakeLock,
            AvisoServidor::LimiteProcesos,
        ],
    );
    prueba.leer_estado_del_servidor();
    assert_eq!(
        prueba.vaciar_barra(),
        vec![(AvisoServidor::LimiteProcesos.texto().to_string(), true)]
    );

    // Otro arranque es otro servidor: vuelve a decir los suyos.
    escribir_estado(&prueba, 2000, &[AvisoServidor::ApiApagada]);
    prueba.leer_estado_del_servidor();
    assert_eq!(
        prueba.vaciar_barra(),
        vec![(AvisoServidor::ApiApagada.texto().to_string(), true)]
    );
}

/// Se mira cada `RITMO_ESTADO_SERVIDOR` (5 s), no en cada tick.
#[test]
fn el_estado_del_servidor_se_mira_cada_cinco_segundos() {
    assert_eq!(RITMO_ESTADO_SERVIDOR, Duration::from_secs(5));
    let mut prueba = Prueba::nueva();
    escribir_estado(&prueba, 1, &[]);
    prueba.tick();
    prueba.esperar("la lectura", |p| !p.app.termux.leyendo_servidor);
    let primera = prueba.app.termux.ultima_lectura.expect("primera lectura");

    // Un servidor nuevo, pero dentro de los 5 s: aún no se mira.
    escribir_estado(&prueba, 2, &[AvisoServidor::SinWakeLock]);
    prueba.tick();
    prueba.tick();
    assert_eq!(prueba.app.termux.ultima_lectura, Some(primera));
    assert!(!prueba.app.termux.leyendo_servidor);
    assert!(prueba.app.termux.cola_barra.is_empty());

    // Pasados los 5 s, sí.
    prueba.leer_estado_del_servidor();
    assert_eq!(prueba.app.termux.cola_barra.len(), 1);
}

/// Fuera de Termux la ventana no mira nada del estado del servidor.
#[test]
fn fuera_de_termux_no_se_mira_el_estado_del_servidor() {
    let mut prueba = Prueba::nueva();
    prueba.app.termux.activo = false;
    escribir_estado(&prueba, 1, &[AvisoServidor::SinWakeLock]);
    prueba.tick();
    prueba.tick();
    assert!(prueba.app.termux.ultima_lectura.is_none());
    assert!(!prueba.app.termux.leyendo_servidor);
    assert!(prueba.mensaje().is_none());
}

/// El fichero lo escribe otro proceso: basura, enormes o raros, sin pánico y
/// sin avisos.
#[test]
fn un_estado_del_servidor_ilegible_no_dice_nada_ni_entra_en_panico() {
    let mut prueba = Prueba::nueva();
    let directorio = estado::directorio(&prueba.rutas);
    std::fs::create_dir_all(&directorio).unwrap();
    let fichero = directorio.join("servidor");

    let casos: Vec<(&str, Vec<u8>)> = vec![
        ("no es json", b"no es json".to_vec()),
        ("vacío", Vec::new()),
        ("binario", (0..=255u8).cycle().take(5000).collect()),
        (
            "aviso desconocido",
            br#"{"arranque":5,"avisos":["algo_nuevo"]}"#.to_vec(),
        ),
        (
            "arranque que no es número",
            br#"{"arranque":"x","avisos":[]}"#.to_vec(),
        ),
        (
            "avisos que no es lista",
            br#"{"arranque":5,"avisos":7}"#.to_vec(),
        ),
        ("sin arranque", br#"{"avisos":["sin_wake_lock"]}"#.to_vec()),
        ("más de 64 KiB", vec![b'['; 200 * 1024]),
        ("anidado sin fin", "[".repeat(100_000).into_bytes()),
    ];
    for (nombre, contenido) in casos {
        std::fs::write(&fichero, &contenido).unwrap();
        prueba.leer_estado_del_servidor();
        assert!(
            prueba.app.termux.cola_barra.is_empty(),
            "{nombre}: no debe decir nada"
        );
        assert!(prueba.app.termux.avisos_dichos.is_none(), "{nombre}");
    }

    // Un directorio, o un fichero infinito, en vez del fichero.
    std::fs::remove_file(&fichero).unwrap();
    std::fs::create_dir(&fichero).unwrap();
    prueba.leer_estado_del_servidor();
    std::fs::remove_dir(&fichero).unwrap();
    std::os::unix::fs::symlink("/dev/zero", &fichero).unwrap();
    prueba.leer_estado_del_servidor();
    assert!(prueba.app.termux.cola_barra.is_empty());

    // Y tras todo eso, uno bueno sigue funcionando.
    std::fs::remove_file(&fichero).unwrap();
    escribir_estado(&prueba, 9, &[AvisoServidor::LimiteProcesos]);
    prueba.leer_estado_del_servidor();
    assert_eq!(
        prueba.vaciar_barra(),
        vec![(AvisoServidor::LimiteProcesos.texto().to_string(), true)]
    );
}

/// El evento de una lectura vieja que vuelve tarde no deja la lectura colgada.
#[test]
fn una_lectura_que_no_vuelve_se_da_por_perdida() {
    let mut prueba = Prueba::nueva();
    prueba.app.termux.leyendo_servidor = true;
    prueba.app.termux.ultima_lectura = Some(Instant::now() - Duration::from_secs(10));
    prueba.tick();
    assert!(
        prueba.app.termux.ultima_lectura.unwrap().elapsed() > Duration::from_secs(9),
        "dentro de la espera no se lanza otra"
    );
    prueba.app.termux.ultima_lectura = Some(Instant::now() - Duration::from_secs(60));
    prueba.tick();
    assert!(
        prueba.app.termux.ultima_lectura.unwrap().elapsed() < Duration::from_secs(5),
        "pasada la espera se lanza otra"
    );
    prueba.esperar("la lectura", |p| !p.app.termux.leyendo_servidor);
}

/// La cola de la barra tiene tope: lo más viejo se descarta y nunca crece sin
/// medida aunque el servidor cambie de arranque una y otra vez.
#[test]
fn la_cola_de_la_barra_tiene_tope() {
    let mut prueba = Prueba::nueva();
    for arranque in 1..=5 {
        prueba.evento(Evento::Termux(EventoTermux::EstadoServidor(Some(
            EstadoServidorTermux {
                arranque,
                avisos: vec![
                    AvisoServidor::ApiApagada,
                    AvisoServidor::SinWakeLock,
                    AvisoServidor::LimiteProcesos,
                ],
            },
        ))));
    }
    assert_eq!(
        prueba.app.termux.cola_barra.len(),
        8,
        "15 avisos, 8 de tope"
    );
    let ultimo = prueba.app.termux.cola_barra.back().cloned().unwrap();
    assert_eq!(
        ultimo,
        (AvisoServidor::LimiteProcesos.texto().to_string(), true),
        "se queda lo más nuevo"
    );
}

/// Los eventos de Termux se pueden inyectar directamente (los usa el bucle).
#[test]
fn el_evento_del_estado_del_servidor_llega_por_el_bucle() {
    let mut prueba = Prueba::nueva();
    prueba.evento(Evento::Termux(EventoTermux::EstadoServidor(Some(
        EstadoServidorTermux {
            arranque: 77,
            avisos: vec![AvisoServidor::SinWakeLock, AvisoServidor::SinWakeLock],
        },
    ))));
    assert_eq!(
        prueba.vaciar_barra(),
        vec![(AvisoServidor::SinWakeLock.texto().to_string(), true)],
        "repetidos en el mismo fichero: uno"
    );
    prueba.evento(Evento::Termux(EventoTermux::EstadoServidor(None)));
    assert!(prueba.vaciar_barra().is_empty());
}

// ------------------------------------------- visibilidad mínima de los avisos

/// Los avisos que salen de la cola de la barra se ven al menos
/// `VISIBILIDAD_AVISO`: tecleando no se quitan ni vencen a los 4 s de cualquier
/// mensaje; solo otro mensaje los pisa.
#[test]
fn un_aviso_de_la_cola_no_se_borra_con_las_teclas_antes_de_su_visibilidad_minima() {
    assert_eq!(VISIBILIDAD_AVISO, Duration::from_secs(6));
    let mut prueba = Prueba::nueva();
    prueba.evento(Evento::Termux(EventoTermux::EstadoServidor(Some(
        EstadoServidorTermux {
            arranque: 5,
            avisos: vec![AvisoServidor::SinWakeLock],
        },
    ))));
    prueba.tick();
    let aviso = (AvisoServidor::SinWakeLock.texto().to_string(), true);
    assert_eq!(prueba.mensaje(), Some(aviso.clone()));

    // El usuario sigue tecleando: el aviso sigue ahí.
    for _ in 0..5 {
        prueba.tecla(KeyCode::Down);
        prueba.tick();
    }
    assert_eq!(
        prueba.mensaje(),
        Some(aviso.clone()),
        "las teclas no lo quitan"
    );

    // Ni el vencimiento de 4 s de cualquier mensaje.
    prueba.app.mensaje.as_mut().expect("mensaje").creado = Instant::now() - Duration::from_secs(5);
    prueba.tick();
    assert_eq!(
        prueba.mensaje(),
        Some(aviso.clone()),
        "ni los 4 s de siempre"
    );

    // Pasada su visibilidad, se va como cualquier mensaje: con la tecla...
    prueba
        .app
        .termux
        .aviso_fijado
        .as_mut()
        .expect("aviso fijado")
        .hasta = Instant::now() - Duration::from_millis(1);
    prueba.tecla(KeyCode::Down);
    assert!(prueba.mensaje().is_none(), "vencido, la tecla lo quita");

    // ...o con el tick.
    prueba.app.mensaje(aviso.0.clone(), true);
    prueba.app.mensaje.as_mut().expect("mensaje").creado = Instant::now() - Duration::from_secs(7);
    prueba.tick();
    assert!(prueba.mensaje().is_none(), "vencido, el tick lo quita");
}

/// Otro mensaje sí pisa a un aviso protegido, y a partir de ahí las teclas
/// borran el nuevo como siempre.
#[test]
fn otro_mensaje_pisa_a_un_aviso_protegido() {
    let mut prueba = Prueba::nueva();
    prueba.evento(Evento::Termux(EventoTermux::EstadoServidor(Some(
        EstadoServidorTermux {
            arranque: 5,
            avisos: vec![AvisoServidor::LimiteProcesos],
        },
    ))));
    prueba.tick();
    assert!(prueba.mensaje().is_some());
    prueba.app.mensaje("no se pudo hacer algo", true);
    prueba.tecla(KeyCode::Down);
    assert!(
        prueba.mensaje().is_none(),
        "el mensaje que lo pisó no está protegido"
    );
}

/// El aviso de Termux:API apagada, encolado tras «copiado», también se ve entero
/// aunque el usuario siga tecleando.
#[test]
fn el_aviso_de_la_api_apagada_sobrevive_a_quien_teclea() {
    let mut prueba = Prueba::nueva();
    prueba.con_plazo(Duration::from_millis(400));
    prueba
        .dobles
        .programar("termux-clipboard-set", Modo::Colgar);
    prueba.copiar_url("https://ejemplo.org/corto?a=1");
    prueba.esperar("OSC 52", |p| !p.osc52().is_empty());
    assert_eq!(prueba.mensaje(), Some(("copiado".to_string(), false)));
    // La primera tecla quita «copiado» y sale el aviso; las siguientes no.
    prueba.tecla(KeyCode::Down);
    prueba.tick();
    let aviso = (AVISO_API_APAGADA.to_string(), true);
    assert_eq!(prueba.mensaje(), Some(aviso.clone()));
    for _ in 0..5 {
        prueba.tecla(KeyCode::Down);
    }
    assert_eq!(prueba.mensaje(), Some(aviso));
}

// ------------------------------------------------ lectura del estado: plazo

/// Suelta la lectura que no termina aunque la prueba falle (el runtime espera a
/// los hilos bloqueantes al caer).
struct SoltarAlSalir(Arc<std::sync::atomic::AtomicBool>);

impl Drop for SoltarAlSalir {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Una lectura del estado que no termina se da por vacía a los 2 s (libera la
/// bandera) y, mientras su hilo siga vivo, no se lanza otra detrás.
#[test]
fn una_lectura_del_estado_que_no_termina_no_se_relanza() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    assert_eq!(PLAZO_LECTURA_ESTADO, Duration::from_secs(2));
    let mut prueba = Prueba::nueva();
    let llamadas = Arc::new(AtomicUsize::new(0));
    let soltar = Arc::new(AtomicBool::new(false));
    let _guarda = SoltarAlSalir(Arc::clone(&soltar));
    {
        let (llamadas, soltar) = (Arc::clone(&llamadas), Arc::clone(&soltar));
        prueba.app.termux.lector_estado = Arc::new(move |_: &Rutas| {
            llamadas.fetch_add(1, Ordering::SeqCst);
            while !soltar.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            None
        });
    }
    let inicio = Instant::now();
    prueba.tick();
    assert!(prueba.app.termux.leyendo_servidor, "no se lanzó la lectura");
    prueba.esperar("el plazo de la lectura", |p| !p.app.termux.leyendo_servidor);
    assert!(
        inicio.elapsed() >= PLAZO_LECTURA_ESTADO - Duration::from_millis(100),
        "no esperó el plazo: {:?}",
        inicio.elapsed()
    );
    assert!(
        inicio.elapsed() < Duration::from_secs(10),
        "{:?}",
        inicio.elapsed()
    );
    assert_eq!(llamadas.load(Ordering::SeqCst), 1);
    assert!(
        prueba.app.termux.lectura_viva.load(Ordering::SeqCst),
        "el hilo sigue vivo aunque su espera venció"
    );

    // Pasan el ritmo y hasta la espera de pérdida: con el hilo vivo, no se
    // lanza otra lectura.
    prueba.app.termux.ultima_lectura = Some(Instant::now() - Duration::from_secs(60));
    prueba.tick();
    prueba.tick();
    assert!(!prueba.app.termux.leyendo_servidor);
    assert_eq!(llamadas.load(Ordering::SeqCst), 1, "no se acumulan hilos");

    // Cuando el hilo acaba, la siguiente vuelve a tocar.
    soltar.store(true, Ordering::SeqCst);
    prueba.esperar("el hilo acabó", |p| {
        !p.app.termux.lectura_viva.load(Ordering::SeqCst)
    });
    prueba.app.termux.ultima_lectura = Some(Instant::now() - Duration::from_secs(60));
    prueba.tick();
    assert!(
        prueba.app.termux.leyendo_servidor,
        "no se lanzó otra lectura"
    );
    prueba.esperar("la nueva lectura", |p| !p.app.termux.leyendo_servidor);
    assert_eq!(llamadas.load(Ordering::SeqCst), 2);
}

// ------------------------------------------- el servidor al que se habla

/// El cliente no lee el estado del servidor hasta haber recibido su
/// `Bienvenida`: antes, el fichero puede ser de otro servidor. Tampoco mientras
/// el servidor esté caído.
#[test]
fn el_estado_del_servidor_no_se_lee_antes_de_su_bienvenida() {
    let mut prueba = Prueba::sin_bienvenida(config_termux());
    // Lo que dejó otro servidor (anterior, ya muerto o a punto de serlo).
    escribir_estado(&prueba, 1000, &[AvisoServidor::SinWakeLock]);
    prueba.tick();
    prueba.tick();
    assert!(
        prueba.app.termux.ultima_lectura.is_none(),
        "no se leyó sin bienvenida"
    );
    assert!(!prueba.app.termux.leyendo_servidor);
    assert!(prueba.app.termux.cola_barra.is_empty());

    // Con la bienvenida, ya.
    prueba.bienvenida();
    prueba.leer_estado_del_servidor();
    assert_eq!(
        prueba.vaciar_barra(),
        vec![(AvisoServidor::SinWakeLock.texto().to_string(), true)]
    );

    // Con el servidor caído no se mira lo que dejó.
    prueba.app.servidor_caido = true;
    escribir_estado(&prueba, 2000, &[AvisoServidor::LimiteProcesos]);
    prueba.app.termux.ultima_lectura = Some(Instant::now() - RITMO_ESTADO_SERVIDOR * 2);
    prueba.tick();
    assert!(!prueba.app.termux.leyendo_servidor, "servidor caído");
    assert!(prueba.app.termux.cola_barra.is_empty());
}

// ------------------------------------------------------ el cierre de la TUI

/// Al salir, una orden de Termux colgada no retiene el cierre: las órdenes
/// van en `spawn_blocking` y soltar el runtime espera a que acaben (hasta
/// `api_plazo_seg`, 30 s como mucho). `cerrar_al_salir` lo apaga con un plazo
/// corto; la orden sigue su camino y muere en su propio plazo.
#[test]
fn una_orden_colgada_en_vuelo_no_retiene_la_salida() {
    let mut prueba = Prueba::nueva();
    // El plazo de la orden es mucho mayor que lo que puede tardar el cierre:
    // soltar el runtime esperaría casi todo él.
    prueba.con_plazo(Duration::from_secs(4));
    prueba.dobles.programar("termux-open-url", Modo::Colgar);
    prueba.abrir_url("https://ejemplo.org/");
    // La orden ya está en marcha (el doble anotó su pid) y sigue colgada.
    let limite = Instant::now() + Duration::from_secs(5);
    let pids = loop {
        let llamadas = prueba.llamadas("termux-open-url");
        if let Some(llamada) = llamadas.first().filter(|llamada| !llamada.pids.is_empty()) {
            break llamada.pids.clone();
        }
        assert!(
            Instant::now() < limite,
            "la orden no llegó a lanzarse en 5 s"
        );
        std::thread::sleep(Duration::from_millis(10));
    };

    let Prueba {
        app,
        dobles,
        _temporal,
        ..
    } = prueba;
    let inicio = Instant::now();
    app.cerrar_al_salir().expect("cierre");
    let tardo = inicio.elapsed();
    assert!(
        tardo < Duration::from_millis(1500),
        "la salida esperó a la orden colgada: {tardo:?}"
    );

    // Dejada atrás, la orden no queda huérfana para siempre: su plazo mata el
    // grupo (el hilo bloqueante sigue vivo hasta entonces dentro de este
    // proceso).
    for pid in pids {
        assert!(!sigue_vivo_tras(pid, Duration::from_secs(8)), "{pid}");
    }
    drop(dobles);
}

/// Lo mismo con la lectura del estado del servidor, que va en su propio hilo
/// bloqueante y no se puede cancelar.
#[test]
fn una_lectura_del_estado_en_vuelo_no_retiene_la_salida() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut prueba = Prueba::nueva();
    let soltar = Arc::new(AtomicBool::new(false));
    let _guarda = SoltarAlSalir(Arc::clone(&soltar));
    {
        let soltar = Arc::clone(&soltar);
        prueba.app.termux.lector_estado = Arc::new(move |_: &Rutas| {
            // Con tope propio: sin el arreglo, la salida esperaría hasta aquí
            // y la prueba fallaría por el tiempo, no se quedaría colgada.
            let tope = Instant::now() + Duration::from_secs(6);
            while !soltar.load(Ordering::SeqCst) && Instant::now() < tope {
                std::thread::sleep(Duration::from_millis(10));
            }
            None
        });
    }
    prueba.tick();
    assert!(prueba.app.termux.leyendo_servidor, "no se lanzó la lectura");
    assert!(
        prueba.app.termux.lectura_viva.load(Ordering::SeqCst),
        "el hilo está leyendo"
    );
    let Prueba { app, _temporal, .. } = prueba;
    let inicio = Instant::now();
    app.cerrar_al_salir().expect("cierre");
    assert!(
        inicio.elapsed() < Duration::from_millis(1500),
        "la salida esperó a la lectura: {:?}",
        inicio.elapsed()
    );
}

/// Sin nada en vuelo, el cierre sigue siendo lo de siempre: el adiós al
/// servidor se envía antes de apagar el runtime y el almacén se cierra.
#[test]
fn el_cierre_sin_nada_en_vuelo_dice_adios_y_cierra_el_almacen() {
    let mut prueba = Prueba::nueva();
    prueba.drenar();
    let Prueba {
        app,
        mut enviados,
        _temporal,
        ..
    } = prueba;
    app.cerrar_al_salir().expect("cierre");
    let mut mensajes = Vec::new();
    while let Ok(mensaje) = enviados.try_recv() {
        mensajes.push(mensaje);
    }
    assert!(
        mensajes
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeCliente::Adios)),
        "no se envió el adiós: {mensajes:?}"
    );
}
