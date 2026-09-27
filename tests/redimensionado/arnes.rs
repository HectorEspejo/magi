//! Arnés de las pruebas de la Fase 7: una App sin hilos, sin red y sin
//! servidor, pintada sobre un `TestBackend` que cuenta las limpiezas.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::Terminal;
use tokio::sync::mpsc;

use magi::almacen::Almacen;
use magi::app::{App, Evento};
use magi::protocolo::{EstadoSesionRemota, InfoSesion, MensajeCliente, MensajeServidor};
use magi::tema::Tema;

use crate::comun;

/// `TestBackend` que cuenta las limpiezas de pantalla. Las limpiezas seguidas
/// (ratatui limpia dos veces al encoger en horizontal) cuentan como una.
pub struct BackendContador {
    pub interior: TestBackend,
    pub limpiezas: usize,
    /// Cuándo se limpió por última vez (para medir la tardanza del repintado).
    pub ultima_limpieza: Option<Instant>,
    ultima_fue_limpieza: bool,
}

impl BackendContador {
    pub fn new(cols: u16, filas: u16) -> Self {
        Self {
            interior: TestBackend::new(cols, filas),
            limpiezas: 0,
            ultima_limpieza: None,
            ultima_fue_limpieza: false,
        }
    }

    fn limpieza(&mut self) {
        if !self.ultima_fue_limpieza {
            self.limpiezas += 1;
            self.ultima_limpieza = Some(Instant::now());
        }
        self.ultima_fue_limpieza = true;
    }
}

impl Backend for BackendContador {
    type Error = <TestBackend as Backend>::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.ultima_fue_limpieza = false;
        self.interior.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.interior.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.interior.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.interior.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.interior.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.limpieza();
        self.interior.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        if clear_type == ClearType::All {
            self.limpieza();
        }
        self.interior.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.interior.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.interior.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.interior.flush()
    }
}

/// Una App de prueba con su terminal y lo que envía al servidor.
pub struct AppPrueba {
    pub app: App,
    pub enviados: mpsc::UnboundedReceiver<MensajeCliente>,
    pub terminal: Terminal<BackendContador>,
    /// Directorio temporal de la prueba: vive tanto como la App.
    pub _entorno: comun::Entorno,
}

impl AppPrueba {
    pub fn nueva(cols: u16, filas: u16) -> Self {
        Self::con_entorno(comun::entorno(), cols, filas)
    }

    pub fn con_entorno(entorno: comun::Entorno, cols: u16, filas: u16) -> Self {
        let almacen = Almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (mut app, enviados) = App::de_prueba(
            entorno.rutas.clone(),
            entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
        .expect("app de prueba");
        let mut terminal = Terminal::new(BackendContador::new(cols, filas)).expect("terminal");
        app.iniciar_pintado(&mut terminal).expect("primer pintado");
        Self {
            app,
            enviados,
            terminal,
            _entorno: entorno,
        }
    }

    /// La terminal cambia de tamaño (como haría Alacritty) y llega el evento.
    pub fn redimensionar_en(&mut self, cols: u16, filas: u16, ahora: Instant) {
        self.terminal.backend_mut().interior.resize(cols, filas);
        self.app
            .procesar_en(Evento::Redimension(cols, filas), ahora);
    }

    /// Redimensiona y deja pasar la agrupación: el tamaño queda aplicado.
    pub fn redimensionar(&mut self, cols: u16, filas: u16) {
        let ahora = Instant::now();
        self.redimensionar_en(cols, filas, ahora);
        self.paso_en(ahora + Duration::from_millis(60));
    }

    pub fn paso_en(&mut self, ahora: Instant) {
        self.app.paso(&mut self.terminal, ahora).expect("paso");
    }

    /// Procesa un evento y pinta lo que haya cambiado.
    pub fn evento(&mut self, evento: Evento) {
        self.app.procesar(evento);
        self.paso_en(Instant::now());
    }

    pub fn tecla(&mut self, codigo: KeyCode) {
        self.evento(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
    }

    pub fn tecla_con(&mut self, codigo: KeyCode, modificadores: KeyModifiers) {
        self.evento(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
    }

    pub fn servidor(&mut self, mensaje: MensajeServidor) {
        self.evento(Evento::Servidor(mensaje));
    }

    /// Lo enviado al servidor desde la última llamada.
    pub fn enviados(&mut self) -> Vec<MensajeCliente> {
        let mut mensajes = Vec::new();
        while let Ok(mensaje) = self.enviados.try_recv() {
            mensajes.push(mensaje);
        }
        mensajes
    }

    /// Solo los `Redimensionar` enviados desde la última llamada.
    pub fn redimensionares(&mut self) -> Vec<(u32, u16, u16)> {
        self.enviados()
            .into_iter()
            .filter_map(|mensaje| match mensaje {
                MensajeCliente::Redimensionar {
                    sesion_id,
                    cols,
                    filas,
                } => Some((sesion_id, cols, filas)),
                _ => None,
            })
            .collect()
    }

    /// Tamaño del búfer pintado.
    pub fn tamano_pintado(&self) -> (u16, u16) {
        let area = self.terminal.backend().interior.buffer().area;
        (area.width, area.height)
    }

    /// Texto pintado, una línea por fila.
    pub fn texto(&self) -> String {
        texto_de(&self.terminal.backend().interior)
    }

    /// Da al cliente la bienvenida del servidor con estas sesiones abiertas.
    pub fn bienvenida(&mut self, sesiones: Vec<InfoSesion>) {
        self.servidor(bienvenida(1, sesiones));
    }

    /// Entra en la vista Sesión con la primera pestaña adjunta.
    pub fn entrar_en_sesion(&mut self, sesiones: Vec<InfoSesion>) {
        self.bienvenida(sesiones);
        self.tecla(KeyCode::F(3));
        self.tecla(KeyCode::Enter);
        assert_eq!(self.app.vista, magi::ui::Vista::Sesion);
    }
}

pub fn texto_de(backend: &TestBackend) -> String {
    let buffer = backend.buffer();
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

pub fn bienvenida(cliente_id: u32, sesiones: Vec<InfoSesion>) -> MensajeServidor {
    MensajeServidor::Bienvenida {
        version: magi::protocolo::VERSION_PROTOCOLO,
        pid: 4242,
        cliente_id,
        clientes: 1,
        sesiones,
        transferencias: Vec::new(),
        tuneles: Vec::new(),
        ejecuciones: Vec::new(),
    }
}

/// Una sesión abierta del servidor, tal como llega en la difusión.
pub fn sesion(id: u32, host: &str) -> InfoSesion {
    InfoSesion {
        id,
        nombre: host.to_string(),
        host_id: i64::from(id),
        host_nombre: host.to_string(),
        estado: EstadoSesionRemota::Abierta,
        motivo: None,
        identidad: "clave".to_string(),
        abierta_en: 0,
        ventanas: 1,
        actividad_no_vista: false,
    }
}
