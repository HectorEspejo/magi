//! Emulador de terminal de MAGI sobre vt100 0.16 (Fase 15).
//!
//! vt100 lleva la pantalla, el historial, el pegado entre corchetes, el ratón,
//! la pantalla alternativa y los colores. Lo que no lleva —los modos 1004
//! (foco) y 2026 (salida sincronizada), las banderas del teclado de kitty por
//! pantalla, ED 3, el título, los avisos (BEL, OSC 9 y OSC 777), OSC 52 y las
//! respuestas a las consultas del remoto— vive aquí, en los `Callbacks` del
//! parser.
//!
//! El emulador es el mismo en el servidor y en el cliente: el servidor lo crea
//! respondiendo (solo él escribe al remoto) y el cliente sin responder. Las
//! consultas que no están en la tabla de la fase no se contestan.
//!
//! `PantallaEmulada` envuelve el parser y sabe rehacerlo conservando su estado
//! cuando llega ED 3 o cuando el tope del historial baja al redimensionar: es
//! la única forma de vaciar el historial de vt100 0.16, que no expone su
//! `scrollback` ni su longitud.

use std::collections::VecDeque;
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// Tope de la pila de banderas del teclado de kitty por pantalla.
pub const PILA_KITTY_MAX: usize = 16;

/// Lo que el cliente espera como mucho a que se cierre la salida sincronizada
/// (modo 2026): pasado este plazo, se pinta igual.
pub const SALIDA_SINCRONIZADA_MAX: Duration = Duration::from_millis(200);

/// Longitud máxima de un título o un aviso del remoto.
pub const TEXTO_REMOTO_MAX: usize = 120;

/// Tope del texto que el remoto deja en el portapapeles con OSC 52.
pub const PORTAPAPELES_MAX: usize = 1024 * 1024;

/// Secuencia de ED 3 (borrar el historial guardado).
const ED3: &[u8] = b"\x1b[3J";

/// Tope de un OSC o DCS del remoto antes de descartarlo. Es el mismo tope que
/// el del portapapeles (1 MiB): vte acumula el OSC entero en memoria y sin
/// este corte un host podría agotarla con un solo OSC sin terminar.
pub const OSC_MAX: usize = 1024 * 1024;

/// Tamaño máximo de pantalla que se acepta. Un cliente roto o malicioso que
/// pida 65535×65535 haría que vt100 reservara decenas de GB; por encima de
/// esto se recorta.
pub const MAX_FILAS: u16 = 1000;
pub const MAX_COLUMNAS: u16 = 2000;

/// Estado del filtro que acota los OSC/DCS antes de que vte los acumule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum FiltroOsc {
    #[default]
    Normal,
    /// El byte anterior fue ESC.
    Escape,
    /// Dentro de un OSC/DCS, con los bytes vistos hasta ahora.
    Dentro(usize),
    /// Dentro de un OSC/DCS y el byte anterior fue ESC (posible ST).
    DentroEscape {
        bytes: usize,
    },
    /// OSC/DCS pasado de tope: se descarta hasta su terminador.
    Descartando,
    DescartandoEscape,
}

/// Posición del **último** ED 3 del flujo. Varios ED 3 en un lote solo dejan
/// el historial vacío una vez: rehacer el parser por cada uno es un DoS de CPU
/// con entrada no fiable.
fn ultimo_ed3(bytes: &[u8]) -> Option<usize> {
    bytes.windows(ED3.len()).rposition(|ventana| ventana == ED3)
}

/// Colores del tema de la ventana, para responder a OSC 10, 11 y 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColoresTema {
    pub primer_plano: (u8, u8, u8),
    pub fondo: (u8, u8, u8),
    pub cursor: (u8, u8, u8),
}

impl Default for ColoresTema {
    fn default() -> Self {
        Self {
            primer_plano: (0xd0, 0xd0, 0xd0),
            fondo: (0x10, 0x10, 0x10),
            cursor: (0xf0, 0xf0, 0xf0),
        }
    }
}

/// Un aviso del remoto: campana, OSC 9 u OSC 777.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aviso {
    pub texto: String,
    /// Época en segundos en la que llegó.
    pub hora: i64,
}

/// Pila de banderas del teclado de kitty de una pantalla.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PilaKitty {
    pub banderas: VecDeque<u16>,
}

impl PilaKitty {
    /// Bandera vigente (la de arriba de la pila), solo con la 1.
    #[must_use]
    pub fn vigente(&self) -> u16 {
        self.banderas.back().copied().unwrap_or(0) & 1
    }

    fn empujar(&mut self, banderas: u16) {
        if self.banderas.len() >= PILA_KITTY_MAX {
            self.banderas.pop_front();
        }
        self.banderas.push_back(banderas & 1);
    }

    fn sacar(&mut self, cuantas: usize) {
        for _ in 0..cuantas.max(1) {
            self.banderas.pop_back();
        }
    }

    fn fijar(&mut self, banderas: u16) {
        match self.banderas.back_mut() {
            Some(actual) => *actual = banderas & 1,
            None => self.banderas.push_back(banderas & 1),
        }
    }
}

/// Estado del emulador que no es la pantalla de vt100.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Modos {
    /// Modo 1004: el remoto quiere los eventos de foco.
    pub foco: bool,
    /// Modo 2026: salida sincronizada.
    pub salida_sincronizada: bool,
    /// Banderas del teclado de kitty de la pantalla principal.
    pub principal: PilaKitty,
    /// Banderas del teclado de kitty de la pantalla alternativa.
    pub alternativa: PilaKitty,
    /// Título que puso el remoto (OSC 0 u OSC 2), saneado.
    pub titulo: Option<String>,
    /// Último aviso, sin avisar todavía.
    pub aviso: Option<Aviso>,
    /// Última copia del remoto por OSC 52, sin entregar todavía.
    pub portapapeles: Option<String>,
    /// El remoto pidió leer el portapapeles (OSC 52 de lectura): nunca se
    /// responde; se guarda solo para las pruebas.
    pub portapapeles_pedido: bool,
    /// Llegó ED 3 (además de vaciarse el historial al partir el lote).
    pub ed3: bool,
    /// Bytes de respuesta pendientes de escribir al remoto.
    pub respuestas: Vec<u8>,
}

impl Modos {
    /// Bandera vigente de la pantalla activa.
    #[must_use]
    pub fn kitty(&self, alternativa: bool) -> u16 {
        if alternativa {
            self.alternativa.vigente()
        } else {
            self.principal.vigente()
        }
    }
}

/// Los `Callbacks` de vt100 0.16. Responde solo si se creó respondiendo.
#[derive(Debug, Clone)]
pub struct Emulador {
    responder: bool,
    colores: ColoresTema,
    modos: Modos,
    /// Versión del producto que se informa en DA2 y XTVERSION. La fija el
    /// servidor (es el único que responde); el núcleo no conoce la versión de
    /// la app.
    version: String,
    /// El siguiente OSC 52 llega truncado por el tope de memoria y hay que
    /// descartarlo (vte despacha el OSC al cortarlo).
    descartar_portapapeles: bool,
}

impl Default for Emulador {
    fn default() -> Self {
        Self::nuevo(false)
    }
}

impl Emulador {
    /// Crea el emulador. `responder` solo es cierto en el servidor.
    #[must_use]
    pub fn nuevo(responder: bool) -> Self {
        Self {
            responder,
            colores: ColoresTema::default(),
            modos: Modos::default(),
            version: "0.0.0".to_string(),
            descartar_portapapeles: false,
        }
    }

    /// Marca que el siguiente OSC 52 está truncado y no debe guardarse.
    pub fn descartar_siguiente_portapapeles(&mut self) {
        self.descartar_portapapeles = true;
    }

    /// Consume la marca del OSC 52 truncado (la llaman los callbacks que
    /// despachan el OSC cortado).
    fn olvidar_portapapeles_truncado(&mut self) {
        self.descartar_portapapeles = false;
    }

    /// Fija la versión del producto (p. ej. «1.4.0») que se responde en DA2 y
    /// XTVERSION. Solo la usa el servidor.
    pub fn fijar_version(&mut self, version: impl Into<String>) {
        self.version = version.into();
    }

    /// Valor de DA2 a partir de la versión del producto (1.4.0 → 10400).
    fn valor_da2(&self) -> u32 {
        let mut partes = self.version.split('.');
        let mayor: u32 = partes.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let menor: u32 = partes.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let parche: u32 = partes.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        mayor
            .saturating_mul(10_000)
            .saturating_add(menor.saturating_mul(100))
            .saturating_add(parche)
    }

    /// ¿Responde a las consultas del remoto?
    #[must_use]
    pub fn responder(&self) -> bool {
        self.responder
    }

    #[must_use]
    pub fn colores(&self) -> ColoresTema {
        self.colores
    }

    pub fn fijar_colores(&mut self, colores: ColoresTema) {
        self.colores = colores;
    }

    #[must_use]
    pub fn modos(&self) -> &Modos {
        &self.modos
    }

    /// Bandera del teclado de kitty de la pantalla activa.
    #[must_use]
    pub fn banderas_kitty(&self, alternativa: bool) -> u16 {
        self.modos.kitty(alternativa)
    }

    /// Se lleva las respuestas pendientes (una vez por lote).
    pub fn respuestas(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.modos.respuestas)
    }

    /// Vuelve a poner delante respuestas ya generadas antes de rehacer el
    /// parser (si no, un lote con una consulta y un ED 3 perdería la
    /// respuesta de la consulta).
    pub fn anteponer_respuestas(&mut self, previas: Vec<u8>) {
        if previas.is_empty() {
            return;
        }
        let mut todas = previas;
        todas.append(&mut self.modos.respuestas);
        self.modos.respuestas = todas;
    }

    /// Se lleva el título pendiente de avisar a las ventanas.
    pub fn tomar_titulo(&mut self) -> Option<String> {
        self.modos.titulo.take()
    }

    /// Se lleva el último aviso, si lo hay.
    pub fn tomar_aviso(&mut self) -> Option<Aviso> {
        self.modos.aviso.take()
    }

    /// Se lleva la última copia del remoto, si la hay.
    pub fn tomar_portapapeles(&mut self) -> Option<String> {
        self.modos.portapapeles.take()
    }

    /// Se lleva el aviso de que el remoto quiso leer el portapapeles.
    pub fn tomar_portapapeles_pedido(&mut self) -> bool {
        std::mem::take(&mut self.modos.portapapeles_pedido)
    }

    fn escribir(&mut self, bytes: &[u8]) {
        if self.responder {
            self.modos.respuestas.extend_from_slice(bytes);
        }
    }

    fn pila(&mut self, alternativa: bool) -> &mut PilaKitty {
        if alternativa {
            &mut self.modos.alternativa
        } else {
            &mut self.modos.principal
        }
    }

    fn avisar(&mut self, texto: String) {
        self.modos.aviso = Some(Aviso {
            texto,
            hora: chrono::Utc::now().timestamp(),
        });
    }
}

/// Sanea el texto que llega del remoto: sin controles y de hasta 120
/// caracteres (entrada no fiable).
fn sanear(bytes: &[u8]) -> String {
    let texto = String::from_utf8_lossy(bytes);
    texto
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(TEXTO_REMOTO_MAX)
        .collect::<String>()
        .trim()
        .to_string()
}

fn hex4(valor: u8) -> String {
    format!("{valor:02x}{valor:02x}")
}

fn color_a_rgb(color: (u8, u8, u8)) -> String {
    format!("rgb:{}/{}/{}", hex4(color.0), hex4(color.1), hex4(color.2))
}

/// Valor de DA2 a partir de la versión del producto (`1.4.0` → 10400).
/// Estado de un modo numérico para DECRQM: 1 activo, 2 inactivo, 0 no
/// reconocido.
fn estado_de_modo(screen: &vt100::Screen, emulador: &Emulador, modo: u16) -> u8 {
    use vt100::MouseProtocolMode as Modo;
    let activo = match modo {
        1 => Some(screen.application_cursor()),
        25 => Some(!screen.hide_cursor()),
        47 | 1049 => Some(screen.alternate_screen()),
        1000 => Some(screen.mouse_protocol_mode() == Modo::PressRelease),
        1002 => Some(screen.mouse_protocol_mode() == Modo::ButtonMotion),
        1003 => Some(screen.mouse_protocol_mode() == Modo::AnyMotion),
        1004 => Some(emulador.modos.foco),
        1005 => Some(screen.mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Utf8),
        1006 => Some(screen.mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Sgr),
        2004 => Some(screen.bracketed_paste()),
        2026 => Some(emulador.modos.salida_sincronizada),
        _ => None,
    };
    match activo {
        Some(true) => 1,
        Some(false) => 2,
        None => 0,
    }
}

impl vt100::Callbacks for Emulador {
    fn audible_bell(&mut self, _screen: &mut vt100::Screen) {
        self.avisar("campana".to_string());
    }

    fn set_window_icon_name(&mut self, _screen: &mut vt100::Screen, _icon_name: &[u8]) {
        self.olvidar_portapapeles_truncado();
    }

    fn set_window_title(&mut self, _screen: &mut vt100::Screen, titulo: &[u8]) {
        self.olvidar_portapapeles_truncado();
        let texto = sanear(titulo);
        if !texto.is_empty() {
            self.modos.titulo = Some(texto);
        }
    }

    fn copy_to_clipboard(&mut self, _screen: &mut vt100::Screen, _ty: &[u8], datos: &[u8]) {
        if std::mem::take(&mut self.descartar_portapapeles) {
            // OSC 52 cortado por el tope de memoria: no es una copia real.
            return;
        }
        // vt100 ya validó el alfabeto base64; aquí se decodifica, se acota a
        // 1 MiB y se descarta lo que no sea texto.
        let Ok(decodificado) = base64::engine::general_purpose::STANDARD.decode(datos) else {
            return;
        };
        if decodificado.len() > PORTAPAPELES_MAX {
            return;
        }
        let Ok(texto) = String::from_utf8(decodificado) else {
            return;
        };
        self.modos.portapapeles = Some(texto);
    }

    fn paste_from_clipboard(&mut self, _screen: &mut vt100::Screen, _ty: &[u8]) {
        self.olvidar_portapapeles_truncado();
        // Leer el portapapeles del usuario: nunca. Solo se apunta para las
        // pruebas.
        self.modos.portapapeles_pedido = true;
    }

    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        parametros: &[&[u16]],
        c: char,
    ) {
        let alternativa = screen.alternate_screen();
        let primero = parametros.first().and_then(|grupo| grupo.first()).copied();
        match (i1, i2, c) {
            // DECSET / DECRST de los modos que lleva el emulador.
            (Some(b'?'), None, 'h') | (Some(b'?'), None, 'l') => {
                let activo = c == 'h';
                for grupo in parametros {
                    let Some(&modo) = grupo.first() else {
                        continue;
                    };
                    match modo {
                        1004 => self.modos.foco = activo,
                        2026 => self.modos.salida_sincronizada = activo,
                        _ => {}
                    }
                }
            }
            // DECRQM.
            (Some(b'?'), Some(b'$'), 'p') => {
                let Some(modo) = primero else {
                    return;
                };
                let estado = estado_de_modo(screen, self, modo);
                if estado != 0 {
                    self.escribir(format!("\x1b[?{modo};{estado}$y").as_bytes());
                }
            }
            // DA1.
            (None, None, 'c') => {
                self.escribir(b"\x1b[?62;22c");
            }
            // DA2.
            (Some(b'>'), None, 'c') => {
                let version = self.valor_da2();
                self.escribir(format!("\x1b[>0;{version};0c").as_bytes());
            }
            // DSR 5 y 6.
            (None, None, 'n') => match primero {
                Some(5) => self.escribir(b"\x1b[0n"),
                Some(6) => {
                    let (fila, columna) = screen.cursor_position();
                    self.escribir(
                        format!(
                            "\x1b[{};{}R",
                            fila.saturating_add(1),
                            columna.saturating_add(1)
                        )
                        .as_bytes(),
                    );
                }
                _ => {}
            },
            // XTVERSION.
            (Some(b'>'), None, 'q') => {
                if primero.is_none() || primero == Some(0) {
                    self.escribir(format!("\x1bP>|MAGI {}\x1b\\", self.version).as_bytes());
                }
            }
            // XTWINOPS 18: tamaño de la ventana.
            (None, None, 't') => {
                if primero == Some(18) {
                    let (filas, columnas) = screen.size();
                    self.escribir(format!("\x1b[8;{filas};{columnas}t").as_bytes());
                }
            }
            // Teclado de kitty: consulta, empujar, sacar y fijar.
            (Some(b'?'), None, 'u') => {
                let banderas = self.modos.kitty(alternativa);
                self.escribir(format!("\x1b[?{banderas}u").as_bytes());
            }
            (Some(b'>'), None, 'u') => {
                self.pila(alternativa).empujar(primero.unwrap_or(0));
            }
            (Some(b'<'), None, 'u') => {
                let cuantas = usize::from(primero.unwrap_or(1).max(1));
                self.pila(alternativa).sacar(cuantas);
            }
            (Some(b'='), None, 'u') => {
                self.pila(alternativa).fijar(primero.unwrap_or(0));
            }
            // ED 3: borrar el historial guardado.
            (None, None, 'J') if primero == Some(3) => {
                self.modos.ed3 = true;
            }
            _ => {}
        }
    }

    fn unhandled_osc(&mut self, _screen: &mut vt100::Screen, parametros: &[&[u8]]) {
        self.olvidar_portapapeles_truncado();
        let mut partes = parametros
            .iter()
            .map(|parte| String::from_utf8_lossy(parte).to_string());
        let Some(orden) = partes.next() else {
            return;
        };
        match orden.as_str() {
            "9" => {
                let texto = partes.collect::<Vec<_>>().join(" · ");
                let texto = sanear(texto.as_bytes());
                if !texto.is_empty() {
                    self.avisar(texto);
                }
            }
            "777" => {
                // OSC 777;notify;<título>;<cuerpo> (sin especificación
                // pública): se avisa con lo que venga tras el primer campo.
                let texto: Vec<String> = partes.filter(|parte| parte != "notify").collect();
                let texto = sanear(texto.join(" · ").as_bytes());
                if !texto.is_empty() {
                    self.avisar(texto);
                }
            }
            // OSC 10, 11 y 12 con «?»: color de primer plano, fondo y cursor.
            "10" | "11" | "12" => {
                let consulta = parametros.get(1).copied();
                if consulta != Some(b"?") {
                    return;
                }
                let color = match orden.as_str() {
                    "10" => self.colores.primer_plano,
                    "11" => self.colores.fondo,
                    _ => self.colores.cursor,
                };
                let rgb = color_a_rgb(color);
                self.escribir(format!("\x1b]{orden};{rgb}\x1b\\").as_bytes());
            }
            _ => {}
        }
    }
}

/// Si `bytes` acaba en un prefijo propio de ED 3, devuelve su longitud. El
/// lote se retiene hasta el siguiente para no perder una secuencia partida.
fn sufijo_parcial_ed3(bytes: &[u8]) -> Option<usize> {
    for longitud in (1..ED3.len()).rev() {
        if bytes.len() >= longitud && bytes.ends_with(&ED3[..longitud]) {
            return Some(longitud);
        }
    }
    None
}

/// Parser de una pestaña: la pantalla de vt100 más el emulador, con su
/// historial y el troceo de ED 3.
pub struct PantallaEmulada {
    parser: vt100::Parser<Emulador>,
    filas: u16,
    columnas: u16,
    historial: usize,
    /// Resto de una posible secuencia ED 3 partida entre dos lotes.
    pendiente: Vec<u8>,
    /// Estado del filtro que acota los OSC/DCS antes de vt100.
    filtro: FiltroOsc,
    /// Hay que rehacer el parser al volver a la pantalla principal (el tope
    /// del historial bajó mientras estábamos en la alternativa).
    rehacer_pendiente: bool,
    /// Llegó ED 3 en la pantalla alternativa y hay que vaciar el historial de
    /// la principal al volver.
    ed3_pendiente: bool,
}

impl std::fmt::Debug for PantallaEmulada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PantallaEmulada")
            .field("filas", &self.filas)
            .field("columnas", &self.columnas)
            .field("historial", &self.historial)
            .field("modos", self.parser.callbacks().modos())
            .finish()
    }
}

impl PantallaEmulada {
    /// Crea la pantalla de una pestaña. `historial` es el número de líneas que
    /// guarda vt100 (0 = sin historial).
    #[must_use]
    pub fn nueva(filas: u16, columnas: u16, historial: usize, responder: bool) -> Self {
        let filas = filas.clamp(1, MAX_FILAS);
        let columnas = columnas.clamp(1, MAX_COLUMNAS);
        Self {
            parser: vt100::Parser::new_with_callbacks(
                filas,
                columnas,
                historial,
                Emulador::nuevo(responder),
            ),
            filas,
            columnas,
            historial,
            pendiente: Vec::new(),
            filtro: FiltroOsc::Normal,
            rehacer_pendiente: false,
            ed3_pendiente: false,
        }
    }

    /// Crea la pantalla de una pestaña con los colores del tema de la ventana.
    #[must_use]
    pub fn con_colores(
        filas: u16,
        columnas: u16,
        historial: usize,
        responder: bool,
        colores: ColoresTema,
    ) -> Self {
        let mut pantalla = Self::nueva(filas, columnas, historial, responder);
        pantalla.parser.callbacks_mut().fijar_colores(colores);
        pantalla
    }

    #[must_use]
    pub fn filas(&self) -> u16 {
        self.filas
    }

    #[must_use]
    pub fn columnas(&self) -> u16 {
        self.columnas
    }

    /// Líneas de historial que guarda vt100 (no las que hay).
    #[must_use]
    pub fn tope_historial(&self) -> usize {
        self.historial
    }

    #[must_use]
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    #[must_use]
    pub fn screen_mut(&mut self) -> &mut vt100::Screen {
        self.parser.screen_mut()
    }

    #[must_use]
    pub fn emulador(&self) -> &Emulador {
        self.parser.callbacks()
    }

    pub fn emulador_mut(&mut self) -> &mut Emulador {
        self.parser.callbacks_mut()
    }

    /// Procesa un lote del remoto y devuelve las respuestas que haya que
    /// escribirle (vacío en el cliente).
    ///
    /// Antes de llegar a vt100, los OSC/DCS se acotan a `OSC_MAX` para que un
    /// host no pueda agotar la memoria con un OSC sin terminar. Del resto, se
    /// procesa todo hasta el **último** ED 3 y se rehace el parser una sola
    /// vez (el estado final es el mismo; rehacerlo por cada ED 3 sería un DoS
    /// de CPU con entrada no fiable). Las respuestas ya generadas se
    /// conservan. Si la secuencia viene partida entre lotes, se retiene su
    /// prefijo hasta el siguiente.
    pub fn procesar(&mut self, datos: &[u8]) -> Vec<u8> {
        self.aplicar_pendientes();
        let mut flujo = std::mem::take(&mut self.pendiente);
        flujo.extend_from_slice(datos);
        let flujo = self.filtrar_osc(&flujo);
        let mut respuestas_previas = Vec::new();
        let mut inicio = 0usize;
        if let Some(posicion) = ultimo_ed3(&flujo) {
            self.parser.process(&flujo[..posicion]);
            // Las respuestas de lo procesado antes del ED 3 no se pierden.
            respuestas_previas = self.parser.callbacks_mut().respuestas();
            let alternativa = self.parser.screen().alternate_screen();
            if alternativa {
                // vt100 no deja leer la pantalla principal desde la
                // alternativa: el borrado se aplica al volver a ella.
                self.ed3_pendiente = true;
            } else if self.lineas_historial() > 0 {
                self.rehacer_parser(false);
            }
            self.parser.callbacks_mut().modos.ed3 = true;
            inicio = posicion + ED3.len();
        }
        let resto = &flujo[inicio..];
        match sufijo_parcial_ed3(resto) {
            Some(parcial) => {
                let corte = resto.len() - parcial;
                if corte > 0 {
                    self.parser.process(&resto[..corte]);
                }
                self.pendiente = resto[corte..].to_vec();
            }
            None => {
                if !resto.is_empty() {
                    self.parser.process(resto);
                }
            }
        }
        self.aplicar_pendientes();
        let mut respuestas = self.parser.callbacks_mut().respuestas();
        if !respuestas_previas.is_empty() {
            let mut todas = respuestas_previas;
            todas.append(&mut respuestas);
            respuestas = todas;
        }
        respuestas
    }

    /// Redimensiona la pestaña. Si el tope de historial baja, se rehace el
    /// parser con el tope nuevo conservando las líneas más nuevas; si sube o
    /// no cambia, basta con redimensionar la pantalla. En la pantalla
    /// alternativa el rehacer se aplaza para no perder la principal.
    pub fn redimensionar(&mut self, filas: u16, columnas: u16, historial: usize) {
        let filas = filas.clamp(1, MAX_FILAS);
        let columnas = columnas.clamp(1, MAX_COLUMNAS);
        self.filas = filas;
        self.columnas = columnas;
        self.parser.screen_mut().set_size(filas, columnas);
        if historial < self.historial {
            self.historial = historial;
            if self.parser.screen().alternate_screen() {
                self.rehacer_pendiente = true;
            } else {
                self.rehacer_parser(true);
            }
        }
    }

    /// Aplica lo que quedó pendiente mientras estábamos en la pantalla
    /// alternativa, ya de vuelta en la principal.
    fn aplicar_pendientes(&mut self) {
        if self.parser.screen().alternate_screen() {
            return;
        }
        if self.ed3_pendiente {
            self.ed3_pendiente = false;
            if self.lineas_historial() > 0 {
                self.rehacer_parser(false);
            }
        }
        if self.rehacer_pendiente {
            self.rehacer_pendiente = false;
            self.rehacer_parser(true);
        }
    }

    /// Número de líneas que hay ahora mismo en el historial de vt100.
    ///
    /// vt100 no lo expone: se pide el desplazamiento máximo (que se acota al
    /// tamaño real) y se vuelve al final.
    pub fn lineas_historial(&mut self) -> usize {
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let lineas = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(0);
        lineas
    }

    /// Desplaza la vista del historial (0 = al final).
    pub fn desplazar_historial(&mut self, lineas: usize) {
        self.parser.screen_mut().set_scrollback(lineas);
    }

    /// Vacía el historial conservando la pantalla y los modos (ED 3).
    pub fn vaciar_historial(&mut self) {
        self.aplicar_pendientes();
        if self.parser.screen().alternate_screen() {
            self.ed3_pendiente = true;
        } else if self.lineas_historial() > 0 {
            self.rehacer_parser(false);
        }
    }

    /// Rehace el parser con su estado. vt100 no permite vaciar su historial ni
    /// cambiar su capacidad, así que la pantalla, los modos de entrada y el
    /// emulador se trasladan a un parser nuevo. Con `conservar`, se reinyectan
    /// las líneas más nuevas del historial (sin estilos) para no perderlas al
    /// bajar su tope; sin él, el historial queda vacío (ED 3).
    fn rehacer_parser(&mut self, conservar: bool) {
        let alternativa = self.parser.screen().alternate_screen();
        let estado = self.parser.screen().state_formatted();
        let responder = self.parser.callbacks().responder();
        // Las respuestas ya generadas en este lote se conservan.
        let previas = self.parser.callbacks_mut().respuestas();
        let nuevas = if conservar && !alternativa {
            let tope = self.historial;
            self.leer_ultimas_historial(tope)
        } else {
            Vec::new()
        };
        let emulador = std::mem::replace(self.parser.callbacks_mut(), Emulador::nuevo(responder));
        let mut nuevo =
            vt100::Parser::new_with_callbacks(self.filas, self.columnas, self.historial, emulador);
        if !nuevas.is_empty() {
            // Se reinyectan como texto (los estilos de las líneas más viejas
            // no llegan a vt100 por su API) y se empujan fuera de la pantalla
            // antes de restaurar la principal.
            for linea in &nuevas {
                nuevo.process(linea.as_bytes());
                nuevo.process(b"\r\n");
            }
            for _ in 0..self.filas {
                nuevo.process(b"\r\n");
            }
        }
        // `state_formatted` no lleva el modo de pantalla alternativa: se
        // restaura antes del volcado.
        if alternativa {
            nuevo.process(b"\x1b[?1049h");
        }
        nuevo.process(&estado);
        let _ = nuevo.callbacks_mut().respuestas();
        nuevo.callbacks_mut().anteponer_respuestas(previas);
        self.parser = nuevo;
    }

    /// Texto de las `tope` filas más nuevas del historial, de la más vieja a la
    /// más nueva. Deja la vista al final.
    fn leer_ultimas_historial(&mut self, tope: usize) -> Vec<String> {
        let total = self.lineas_historial();
        let desde = total.saturating_sub(tope);
        let mut lineas = Vec::with_capacity(total - desde);
        for indice in desde..total {
            self.parser.screen_mut().set_scrollback(total - indice);
            let texto = self.parser.screen().contents_between(0, 0, 1, 0);
            let texto = texto.strip_suffix('\n').unwrap_or(&texto).trim_end();
            lineas.push(texto.to_string());
        }
        self.parser.screen_mut().set_scrollback(0);
        lineas
    }

    /// Acota los OSC/DCS antes de que vte los acumule. Un OSC/DCS que pase de
    /// `OSC_MAX` se aborta con CAN y se descarta hasta su terminador.
    fn filtrar_osc(&mut self, datos: &[u8]) -> Vec<u8> {
        if self.filtro == FiltroOsc::Normal && !datos.contains(&0x1b) {
            return datos.to_vec();
        }
        let mut salida = Vec::with_capacity(datos.len());
        for &byte in datos {
            match self.filtro {
                FiltroOsc::Normal => {
                    salida.push(byte);
                    if byte == 0x1b {
                        self.filtro = FiltroOsc::Escape;
                    }
                }
                FiltroOsc::Escape => {
                    salida.push(byte);
                    match byte {
                        b']' | b'P' => self.filtro = FiltroOsc::Dentro(0),
                        0x1b => {}
                        _ => self.filtro = FiltroOsc::Normal,
                    }
                }
                FiltroOsc::Dentro(vistos) => {
                    if byte == 0x07 {
                        salida.push(byte);
                        self.filtro = FiltroOsc::Normal;
                    } else if byte == 0x1b {
                        salida.push(byte);
                        self.filtro = FiltroOsc::DentroEscape { bytes: vistos };
                    } else {
                        salida.push(byte);
                        self.un_byte_de_osc(vistos + 1, &mut salida);
                    }
                }
                FiltroOsc::DentroEscape { bytes } => {
                    salida.push(byte);
                    if byte == b'\\' {
                        self.filtro = FiltroOsc::Normal;
                    } else if byte == 0x1b {
                        self.filtro = FiltroOsc::DentroEscape { bytes };
                    } else if byte == 0x07 {
                        self.filtro = FiltroOsc::Normal;
                    } else {
                        self.un_byte_de_osc(bytes + 1, &mut salida);
                    }
                }
                FiltroOsc::Descartando => {
                    if byte == 0x1b {
                        self.filtro = FiltroOsc::DescartandoEscape;
                    } else if byte == 0x07 {
                        self.filtro = FiltroOsc::Normal;
                    }
                }
                FiltroOsc::DescartandoEscape => {
                    if byte == b'\\' {
                        self.filtro = FiltroOsc::Normal;
                    } else if byte != 0x1b {
                        self.filtro = FiltroOsc::Descartando;
                    }
                }
            }
        }
        salida
    }

    fn un_byte_de_osc(&mut self, vistos: usize, salida: &mut Vec<u8>) {
        if vistos > OSC_MAX {
            // vte despacha el OSC al terminarlo, aunque sea a trozos: se cierra
            // aquí con BEL para que suelte el búfer y se descarta lo que
            // venga hasta el terminador real. Si era un OSC 52, el callback
            // lo ignora por la marca.
            self.parser
                .callbacks_mut()
                .descartar_siguiente_portapapeles();
            salida.push(0x07);
            self.filtro = FiltroOsc::Descartando;
        } else {
            self.filtro = FiltroOsc::Dentro(vistos);
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn pantalla(filas: u16, columnas: u16, historial: usize, responder: bool) -> PantallaEmulada {
        PantallaEmulada::nueva(filas, columnas, historial, responder)
    }

    #[test]
    fn el_modo_2026_partido_entre_dos_lotes_queda_activo() {
        let mut pantalla = pantalla(24, 80, 0, false);
        pantalla.procesar(b"\x1b[?20");
        assert!(!pantalla.emulador().modos().salida_sincronizada);
        pantalla.procesar(b"26h");
        assert!(pantalla.emulador().modos().salida_sincronizada);
        pantalla.procesar(b"\x1b[?2026l");
        assert!(!pantalla.emulador().modos().salida_sincronizada);
    }

    #[test]
    fn los_modos_1004_y_2026_se_leen_declarados_de_una_vez() {
        let mut pantalla = pantalla(24, 80, 0, false);
        pantalla.procesar(b"\x1b[?1004;2026h");
        assert!(pantalla.emulador().modos().foco);
        assert!(pantalla.emulador().modos().salida_sincronizada);
        pantalla.procesar(b"\x1b[?1004;2026l");
        assert!(!pantalla.emulador().modos().foco);
        assert!(!pantalla.emulador().modos().salida_sincronizada);
    }

    #[test]
    fn la_pila_de_kitty_es_por_pantalla_y_solo_guarda_la_bandera_1() {
        let mut pantalla = pantalla(24, 80, 0, true);
        pantalla.procesar(b"\x1b[>1u");
        assert_eq!(pantalla.emulador().banderas_kitty(false), 1);
        pantalla.procesar(b"\x1b[?1049h");
        assert_eq!(pantalla.emulador().banderas_kitty(true), 0);
        pantalla.procesar(b"\x1b[>1u");
        assert_eq!(pantalla.emulador().banderas_kitty(true), 1);
        assert_eq!(pantalla.emulador().banderas_kitty(false), 1);
        pantalla.procesar(b"\x1b[<u");
        assert_eq!(pantalla.emulador().banderas_kitty(true), 0);
        pantalla.procesar(b"\x1b[?1049l");
        assert_eq!(pantalla.emulador().banderas_kitty(false), 1);
    }

    #[test]
    fn la_pila_de_kitty_se_llena_a_dieciseis() {
        let mut pantalla = pantalla(24, 80, 0, true);
        for _ in 0..20 {
            pantalla.procesar(b"\x1b[>1u");
        }
        assert_eq!(
            pantalla.emulador().modos().principal.banderas.len(),
            PILA_KITTY_MAX
        );
    }

    #[test]
    fn da1_da2_dsr_xtversion_y_xtwinops_se_responden() {
        let mut pantalla = pantalla(24, 80, 0, true);
        pantalla.emulador_mut().fijar_version("1.4.0");
        let respuestas = pantalla.procesar(b"\x1b[c");
        assert_eq!(respuestas, b"\x1b[?62;22c");
        let respuestas = pantalla.procesar(b"\x1b[>c");
        assert_eq!(respuestas, b"\x1b[>0;10400;0c");
        let respuestas = pantalla.procesar(b"\x1b[5n");
        assert_eq!(respuestas, b"\x1b[0n");
        let respuestas = pantalla.procesar(b"\x1b[>0q");
        assert!(String::from_utf8(respuestas)
            .unwrap()
            .starts_with("\x1bP>|MAGI "));
        let respuestas = pantalla.procesar(b"\x1b[18t");
        assert_eq!(respuestas, b"\x1b[8;24;80t");
    }

    #[test]
    fn el_dsr_lleva_la_posicion_tras_lo_ya_procesado() {
        let mut pantalla = pantalla(24, 80, 0, true);
        let respuestas = pantalla.procesar(b"\x1b[3;1Hhola\x1b[6n");
        assert_eq!(respuestas, b"\x1b[3;5R");
    }

    #[test]
    fn el_cliente_no_responde_nada() {
        let mut pantalla = pantalla(24, 80, 0, false);
        assert!(pantalla
            .procesar(b"\x1b[c\x1b[>c\x1b[5n\x1b[6n\x1b[18t")
            .is_empty());
    }

    #[test]
    fn decrqm_contesta_los_modos_de_la_tabla() {
        let mut pantalla = pantalla(24, 80, 0, true);
        pantalla.procesar(b"\x1b[?2004h");
        let respuestas = pantalla.procesar(b"\x1b[?2004$p");
        assert_eq!(respuestas, b"\x1b[?2004;1$y");
        let respuestas = pantalla.procesar(b"\x1b[?2026$p");
        assert_eq!(respuestas, b"\x1b[?2026;2$y");
        let respuestas = pantalla.procesar(b"\x1b[?9999$p");
        assert!(respuestas.is_empty());
    }

    #[test]
    fn osc_10_11_y_12_contestan_los_colores_del_tema() {
        let mut pantalla = PantallaEmulada::con_colores(
            24,
            80,
            0,
            true,
            ColoresTema {
                primer_plano: (1, 2, 3),
                fondo: (4, 5, 6),
                cursor: (7, 8, 9),
            },
        );
        assert_eq!(
            pantalla.procesar(b"\x1b]10;?\x07"),
            b"\x1b]10;rgb:0101/0202/0303\x1b\\"
        );
        assert_eq!(
            pantalla.procesar(b"\x1b]11;?\x07"),
            b"\x1b]11;rgb:0404/0505/0606\x1b\\"
        );
        assert_eq!(
            pantalla.procesar(b"\x1b]12;?\x07"),
            b"\x1b]12;rgb:0707/0808/0909\x1b\\"
        );
    }

    #[test]
    fn osc_52_de_lectura_nunca_se_responde_y_la_escritura_se_guarda() {
        let mut pantalla = pantalla(24, 80, 0, true);
        assert!(pantalla.procesar(b"\x1b]52;c;?\x07").is_empty());
        assert!(pantalla.emulador_mut().tomar_portapapeles_pedido());
        pantalla.procesar(b"\x1b]52;c;aG9sYQ==\x07");
        assert_eq!(
            pantalla.emulador_mut().tomar_portapapeles(),
            Some("hola".to_string())
        );
        let grande =
            base64::engine::general_purpose::STANDARD.encode(vec![b'a'; PORTAPAPELES_MAX + 1]);
        assert!(pantalla
            .procesar(format!("\x1b]52;c;{grande}\x07").as_bytes())
            .is_empty());
        assert!(pantalla.emulador_mut().tomar_portapapeles().is_none());
    }

    #[test]
    fn la_campana_osc_9_y_osc_777_dejan_aviso() {
        let mut pantalla = pantalla(24, 80, 0, false);
        pantalla.procesar(b"\x07");
        let aviso = pantalla.emulador_mut().tomar_aviso().unwrap();
        assert_eq!(aviso.texto, "campana");
        pantalla.procesar(b"\x1b]9;te necesito\x07");
        assert_eq!(
            pantalla.emulador_mut().tomar_aviso().unwrap().texto,
            "te necesito"
        );
        pantalla.procesar(b"\x1b]777;notify;Listo;compilacion\x07");
        assert_eq!(
            pantalla.emulador_mut().tomar_aviso().unwrap().texto,
            "Listo · compilacion"
        );
    }

    #[test]
    fn el_titulo_se_sanea_y_se_acota() {
        let mut pantalla = pantalla(24, 80, 0, false);
        let largo = "x".repeat(200);
        pantalla.procesar(format!("\x1b]2;{largo}\x07").as_bytes());
        let titulo = pantalla.emulador_mut().tomar_titulo().unwrap();
        assert_eq!(titulo.chars().count(), TEXTO_REMOTO_MAX);
        pantalla.procesar(b"\x1b]0;Claude Code\x07");
        assert_eq!(
            pantalla.emulador_mut().tomar_titulo().unwrap(),
            "Claude Code"
        );
    }

    #[test]
    fn ed_3_vacia_el_historial_y_conserva_la_pantalla() {
        let mut pantalla = pantalla(2, 10, 100, false);
        for linea in 0..5 {
            pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
        }
        assert!(pantalla.lineas_historial() > 0);
        pantalla.procesar(b"\x1b[2J\x1b[3J");
        assert_eq!(pantalla.lineas_historial(), 0);
        assert!(pantalla.screen().contents().trim().is_empty());
        pantalla.procesar(b"despues");
        assert!(pantalla.screen().contents().contains("despues"));
    }

    #[test]
    fn ed_3_partido_entre_lotes_tambien_vacia_el_historial() {
        let mut pantalla = pantalla(2, 10, 100, false);
        for linea in 0..5 {
            pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
        }
        let antes = pantalla.lineas_historial();
        assert!(antes > 0);
        pantalla.procesar(b"\x1b[3");
        assert_eq!(pantalla.lineas_historial(), antes);
        pantalla.procesar(b"J");
        assert_eq!(pantalla.lineas_historial(), 0);
    }

    #[test]
    fn el_historial_sobrevive_a_un_lote_de_datos_normal() {
        let mut pantalla = pantalla(2, 10, 100, false);
        for linea in 0..5 {
            pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
        }
        let antes = pantalla.lineas_historial();
        pantalla.procesar(b"\x1b[33mhola");
        assert_eq!(pantalla.lineas_historial(), antes);
    }

    #[test]
    fn la_pantalla_alternativa_se_conserva_al_vaciar_el_historial() {
        let mut pantalla = pantalla(4, 10, 100, false);
        pantalla.procesar(b"principal");
        pantalla.procesar(b"\x1b[?1049h");
        pantalla.procesar(b"alternativa");
        assert!(pantalla.screen().alternate_screen());
        pantalla.procesar(b"\x1b[3J");
        assert!(pantalla.screen().alternate_screen());
        assert!(pantalla.screen().contents().contains("alternativa"));
        pantalla.procesar(b"\x1b[?1049l");
        assert!(pantalla.screen().contents().contains("principal"));
    }

    #[test]
    fn redimensionar_por_debajo_del_tope_conserva_las_lineas_nuevas() {
        let mut pantalla = pantalla(2, 10, 100, false);
        for linea in 0..8 {
            pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
        }
        let antes = pantalla.lineas_historial();
        assert!(antes > 2);
        pantalla.redimensionar(2, 10, 2);
        assert_eq!(pantalla.tope_historial(), 2);
        let despues = pantalla.lineas_historial();
        assert!(despues > 0, "al bajar el tope no se pierde todo");
        assert!(despues <= 2, "no se pasa del tope nuevo: {despues}");
        assert!(
            pantalla.screen().contents().contains("linea7"),
            "la pantalla se conserva"
        );
        // Y el texto más nuevo reinyectado sigue estando en el historial.
        let captura = crate::terminal::historial::capturar(&mut pantalla);
        let textos: Vec<String> = captura.lineas.iter().map(|linea| linea.texto()).collect();
        assert!(textos.iter().any(|texto| texto == "linea7"));
    }

    #[test]
    fn las_respuestas_salen_dentro_del_mismo_lote_y_en_orden() {
        let mut pantalla = pantalla(4, 10, 0, true);
        let respuestas = pantalla.procesar(b"hola\x1b[3;5H\x1b[6n\x1b[c");
        assert_eq!(respuestas, b"\x1b[3;5R\x1b[?62;22c");
    }

    #[test]
    fn miles_de_ed_3_en_un_lote_no_bloquean() {
        // Antes se rehacía el parser por cada ED 3: 5.000 ED3 tardaban segundos
        // y bloqueaban el bucle de lectura (DoS de entrada no fiable).
        let mut pantalla = pantalla(24, 80, 100, false);
        for linea in 0..50 {
            pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
        }
        let ed3 = b"\x1b[3J";
        let mut lote = Vec::with_capacity(ed3.len() * 5000);
        for _ in 0..5000 {
            lote.extend_from_slice(ed3);
        }
        let inicio = std::time::Instant::now();
        pantalla.procesar(&lote);
        let transcurrido = inicio.elapsed();
        assert!(
            transcurrido < std::time::Duration::from_millis(500),
            "5.000 ED 3 tardaron {transcurrido:?}"
        );
        assert_eq!(pantalla.lineas_historial(), 0);
    }

    #[test]
    fn un_ed_3_con_una_consulta_en_el_mismo_lote_conserva_la_respuesta() {
        let mut pantalla = pantalla(24, 80, 100, true);
        for linea in 0..10 {
            pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
        }
        let respuestas = pantalla.procesar(b"\x1b[c\x1b[3J");
        assert_eq!(respuestas, b"\x1b[?62;22c");
        assert_eq!(pantalla.lineas_historial(), 0);
    }

    #[test]
    fn un_osc_gigante_se_descarta_sin_llegar_al_parser() {
        let mut pantalla = pantalla(4, 20, 100, false);
        pantalla.procesar(b"antes");
        // Un OSC de 2 MiB que nunca termina: se aborta a 1 MiB y el resto se
        // descarta hasta el terminador; la pantalla no cambia.
        pantalla.procesar(b"\x1b]52;c;");
        let trozo = vec![b'A'; 64 * 1024];
        for _ in 0..32 {
            pantalla.procesar(&trozo);
        }
        pantalla.procesar(b"\x07");
        assert!(pantalla.screen().contents().contains("antes"));
        pantalla.procesar(b"\r\ndespues");
        assert!(pantalla.screen().contents().contains("despues"));
    }

    #[test]
    fn ed_3_en_la_alternativa_vacia_el_historial_al_volver() {
        let mut pantalla = pantalla(2, 10, 100, false);
        for linea in 0..8 {
            pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
        }
        assert!(pantalla.lineas_historial() > 0);
        pantalla.procesar(b"\x1b[?1049h");
        pantalla.procesar(b"\x1b[3J");
        pantalla.procesar(b"\x1b[?1049l");
        pantalla.procesar(b"x");
        assert_eq!(
            pantalla.lineas_historial(),
            0,
            "el ED 3 se aplica al volver"
        );
        assert!(pantalla.screen().contents().contains("linea7"));
    }

    #[test]
    fn el_resize_en_la_alternativa_conserva_la_principal() {
        let mut pantalla = pantalla(4, 10, 100, false);
        pantalla.procesar(b"principal");
        pantalla.procesar(b"\x1b[?1049h");
        // Texto corto: vt100 borra las marcas de envoltura en cualquier
        // set_size y una línea partida se vería en dos filas.
        pantalla.procesar(b"alt");
        pantalla.redimensionar(4, 10, 2);
        assert!(pantalla.screen().alternate_screen());
        assert!(pantalla.screen().contents().contains("alt"));
        pantalla.procesar(b"\x1b[?1049l");
        pantalla.procesar(b"x");
        assert!(
            pantalla.screen().contents().contains("principal"),
            "la pantalla principal no se pierde al redimensionar en la alternativa"
        );
    }

    #[test]
    fn las_pantallas_gigantes_se_recortan() {
        let mut pantalla = pantalla(u16::MAX, u16::MAX, 0, false);
        assert_eq!(pantalla.filas(), MAX_FILAS);
        assert_eq!(pantalla.columnas(), MAX_COLUMNAS);
        pantalla.procesar(b"hola");
        assert!(pantalla.screen().contents().contains("hola"));
    }

    #[test]
    fn la_version_de_da2_no_desborda() {
        let mut pantalla = pantalla(4, 10, 0, true);
        pantalla
            .emulador_mut()
            .fijar_version("4294967295.4294967295.4294967295");
        let respuestas = pantalla.procesar(b"\x1b[>c");
        assert!(!respuestas.is_empty());
    }
}
