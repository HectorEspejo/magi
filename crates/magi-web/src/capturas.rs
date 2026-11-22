//! Exportación de capturas de la TUI a HTML con clases (informe §7.7 y §7.8).
//!
//! La prueba `capturas_web` de `magi-ssh` pinta cada vista con `App::de_prueba` y un
//! `TestBackend` de 100×30, y pasa el `Buffer` a `CapturaCeldas` (una función de
//! unas 40 líneas, en la propia prueba: este crate no depende de ratatui). Aquí
//! vive todo lo demás: resolver colores, agrupar estilos en clases, comprobar que
//! cada carácter tiene glifo y escribir el HTML y la CSS.
//!
//! Reglas:
//! - **Colores por clases**, nunca atributos `style`. Un estilo distinto es una
//!   clase `c<N>` numerada por orden de aparición a través de las capturas, en el
//!   orden de `IDS`. El estilo por defecto no genera `<span>`.
//! - `Reset` se resuelve **antes** de `REVERSED`: `fg Reset` es `TEXTO` y `bg Reset`
//!   es `FONDO`; después se intercambian si el estilo es invertido.
//! - `Indexado(n)`: 0 a 15 con la tabla ANSI fija, 16 a 231 con el cubo 6×6×6 y 232
//!   a 255 en grises (la paleta xterm).
//! - **Glifos**: cada símbolo debe tener glifo en JetBrains Mono o en el
//!   subconjunto de símbolos (`Cobertura`). Si no, se degrada con la función que
//!   se pasa (`disposicion::texto_ascii`, el único degradador) y solo vale si
//!   devuelve **un** carácter distinto de `?` que sí tenga glifo; si no, error.
//! - La captura conserva su fondo de terminal en los dos temas.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::{ErrorWeb, Resultado};
use crate::html::{self, Token};

/// Tamaño de toda captura.
pub const COLUMNAS: usize = 100;
pub const FILAS: usize = 30;

/// Las seis capturas de la portada, en este orden (también el de las clases).
pub const IDS: [&str; 6] = [
    "flota",
    "sesion-paneles",
    "archivos",
    "tuneles",
    "deliberacion",
    "servicios",
];

/// Fondo y texto de la paleta fija de la TUI (`src/tema.rs`).
pub const FONDO: (u8, u8, u8) = (0x0A, 0x0A, 0x0A);
pub const TEXTO: (u8, u8, u8) = (0xED, 0xE8, 0xDC);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorCaptura {
    /// El color por defecto de la terminal.
    Reset,
    Rgb(u8, u8, u8),
    /// Índice de la paleta de 256 colores.
    Indexado(u8),
}

/// Una celda ya sin las de continuación de los caracteres anchos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CeldaCaptura {
    /// El carácter (o los caracteres, si es un grafema) de la celda.
    pub simbolo: String,
    pub fg: ColorCaptura,
    pub bg: ColorCaptura,
    pub negrita: bool,
    pub tenue: bool,
    pub cursiva: bool,
    pub subrayado: bool,
    pub invertido: bool,
    /// `HIDDEN`: el texto se iguala al fondo.
    pub oculta: bool,
}

/// Una captura: sus filas de celdas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturaCeldas {
    pub id: String,
    pub filas: Vec<Vec<CeldaCaptura>>,
}

impl CapturaCeldas {
    /// El texto de la captura, una línea por fila y sin espacios al final: lo que se
    /// ve, para comprobar que no lleva datos que no deben salir (nombres de la
    /// semilla de las pruebas, la versión del paquete…).
    ///
    /// Es el texto **sin degradar** (tal cual lo pinta la TUI): un símbolo vacío o
    /// solo de espacios y controles cuenta como un espacio, cada fila pierde los
    /// espacios finales y las filas en blanco del final no cuentan. Las líneas se
    /// unen con `\n` y no hay salto de línea al final.
    pub fn texto(&self) -> String {
        let mut lineas: Vec<String> = self
            .filas
            .iter()
            .map(|fila| {
                let mut linea = String::new();
                for celda in fila {
                    if es_simbolo_en_blanco(&celda.simbolo) {
                        linea.push(' ');
                    } else {
                        // Un salto de línea o un tabulador dentro de un símbolo no
                        // puede partir la fila: aquí se lee como un espacio (la
                        // exportación, en cambio, lo rechaza).
                        linea.extend(celda.simbolo.chars().map(|c| {
                            if c.is_control() || (c.is_whitespace() && c != ' ') {
                                ' '
                            } else {
                                c
                            }
                        }));
                    }
                }
                linea.truncate(linea.trim_end_matches(' ').len());
                linea
            })
            .collect();
        while lineas.last().is_some_and(String::is_empty) {
            lineas.pop();
        }
        lineas.join("\n")
    }
}

/// Qué símbolos tienen glifo: la unión de JetBrains Mono y del subconjunto.
#[derive(Debug, Clone, Default)]
pub struct Cobertura {
    pub codigos: BTreeSet<u32>,
}

impl Cobertura {
    pub fn desde_codigos(codigos: BTreeSet<u32>) -> Self {
        Self { codigos }
    }

    /// Todos los caracteres del símbolo tienen glifo (los espacios y los controles
    /// no cuentan: un espacio se pinta aunque la fuente no lo anuncie).
    pub fn cubre(&self, simbolo: &str) -> bool {
        simbolo.chars().all(|c| self.cubre_caracter(c))
    }

    fn cubre_caracter(&self, c: char) -> bool {
        c.is_whitespace() || c.is_control() || self.codigos.contains(&u32::from(c))
    }
}

/// El resultado de exportar las seis capturas a la vez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConjuntoExportado {
    /// `id` → `<pre class="captura" aria-hidden="true">…</pre>`.
    pub fragmentos: BTreeMap<String, String>,
    /// `capturas.css`: `.captura{…}` y una regla por clase `.cN{…}`.
    pub css: String,
}

/// Exporta las capturas (todas a la vez, para numerar las clases en común).
/// `degradar` es el degradador de glifos (`disposicion::texto_ascii`).
///
/// Cada celda se resuelve a un estilo (`fg`, `bg`, negrita, cursiva, subrayado):
/// primero `Reset` y `Indexado`, después `invertido` (se intercambian `fg` y `bg`),
/// luego `tenue` (el texto se mezcla a partes iguales con el fondo, redondeando
/// hacia abajo) y por último `oculta` (el texto se iguala al fondo). Las celdas
/// contiguas de una fila con el mismo estilo resuelto forman un tramo. El estilo
/// por defecto (`TEXTO` sobre `FONDO`, sin modificadores) no genera `<span>`; los
/// demás, una clase `c<N>` numerada por orden de primera aparición a través de todas
/// las capturas, en el orden de la lista: dos estilos resueltos iguales dan siempre
/// la misma clase.
///
/// En el HTML cada fila va separada de la siguiente por `\n` y pierde los espacios
/// del final que estén en el estilo por defecto (los de un tramo con fondo se
/// conservan: se ven). Como un `\n` justo tras `<pre>` lo ignoran los navegadores
/// y uno justo antes de `</pre>` no hace línea, una primera o última fila vacía se
/// escribe con un solo espacio: así la captura mantiene todas sus filas. Quitados
/// los espacios finales de cada línea y las líneas vacías del final, el texto del
/// fragmento es exactamente `CapturaCeldas::texto()` (con los glifos degradados).
///
/// La salida es determinista y cada fragmento pasa por `validar_fragmento`, y el
/// CSS por `validar_css`, antes de devolverse: lo que sale de aquí se puede
/// insertar con `|safe`.
pub fn exportar(
    capturas: &[CapturaCeldas],
    cobertura: &Cobertura,
    degradar: &dyn Fn(&str) -> String,
) -> Resultado<ConjuntoExportado> {
    comprobar_ids(capturas)?;

    let mut clases = Clases::default();
    let mut fragmentos = BTreeMap::new();

    for captura in capturas {
        let mut filas_html = Vec::with_capacity(captura.filas.len());
        let ultima = captura.filas.len().saturating_sub(1);
        for (y, fila) in captura.filas.iter().enumerate() {
            let tramos = tramos_de_fila(captura, y, fila, cobertura, degradar)?;
            let mut html_fila = fila_a_html(tramos, &mut clases);
            if html_fila.is_empty() && (y == 0 || y == ultima) {
                html_fila.push(' ');
            }
            filas_html.push(html_fila);
        }
        let fragmento = format!(
            "<pre class=\"captura\" aria-hidden=\"true\">{}</pre>",
            filas_html.join("\n")
        );
        validar_fragmento(&fragmento).map_err(|e| {
            ErrorWeb::dato_raro(
                format!("captura «{}»", captura.id),
                format!("el fragmento exportado no pasa la validación (fallo interno): {e}"),
            )
        })?;
        fragmentos.insert(captura.id.clone(), fragmento);
    }

    let css = css_de(&clases.orden);
    validar_css(&css).map_err(|e| {
        ErrorWeb::dato_raro(
            "capturas.css",
            format!("el CSS exportado no pasa la validación (fallo interno): {e}"),
        )
    })?;

    Ok(ConjuntoExportado { fragmentos, css })
}

/// Tamaño máximo de un fragmento o de un CSS de capturas.
const MAX_BYTES: usize = 1024 * 1024;

type Rgb = (u8, u8, u8);

/// El estilo ya resuelto de una celda: lo único que llega al navegador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Estilo {
    fg: Rgb,
    bg: Rgb,
    negrita: bool,
    cursiva: bool,
    subrayado: bool,
}

/// El estilo que no necesita clase: lo que ya pone `.captura`.
const ESTILO_POR_DEFECTO: Estilo = Estilo {
    fg: TEXTO,
    bg: FONDO,
    negrita: false,
    cursiva: false,
    subrayado: false,
};

/// Las clases `c<N>` por orden de primera aparición.
#[derive(Default)]
struct Clases {
    orden: Vec<Estilo>,
    indice: HashMap<Estilo, usize>,
}

impl Clases {
    fn numero(&mut self, estilo: Estilo) -> usize {
        if let Some(n) = self.indice.get(&estilo) {
            return *n;
        }
        let n = self.orden.len();
        self.orden.push(estilo);
        self.indice.insert(estilo, n);
        n
    }
}

/// Celdas contiguas con el mismo estilo.
struct Tramo {
    estilo: Estilo,
    texto: String,
}

/// Un id de captura se usa como nombre de fichero y como clave: minúsculas,
/// dígitos y guiones, sin repetirse.
fn comprobar_ids(capturas: &[CapturaCeldas]) -> Resultado<()> {
    let mut vistos = BTreeSet::new();
    for captura in capturas {
        let id = &captura.id;
        let valido = !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !valido {
            return Err(ErrorWeb::dato_raro(
                "capturas",
                format!(
                    "el id {} no es válido: de 1 a 64 caracteres entre «a-z», «0-9» y «-»",
                    recortar(id, 40)
                ),
            ));
        }
        if !vistos.insert(id.as_str()) {
            return Err(ErrorWeb::dato_raro(
                "capturas",
                format!("el id «{id}» está repetido: cada captura necesita el suyo"),
            ));
        }
    }
    Ok(())
}

/// Resuelve las celdas de una fila a tramos con su glifo ya decidido.
fn tramos_de_fila(
    captura: &CapturaCeldas,
    fila_n: usize,
    fila: &[CeldaCaptura],
    cobertura: &Cobertura,
    degradar: &dyn Fn(&str) -> String,
) -> Resultado<Vec<Tramo>> {
    let mut tramos: Vec<Tramo> = Vec::new();
    for (x, celda) in fila.iter().enumerate() {
        let glifo = glifo_de(&celda.simbolo, cobertura, degradar).map_err(|que| {
            ErrorWeb::dato_raro(
                format!("captura «{}»", captura.id),
                format!("fila {}, columna {} (desde 1): {que}", fila_n + 1, x + 1),
            )
        })?;
        let estilo = resolver_estilo(celda);
        match tramos.last_mut() {
            Some(ultimo) if ultimo.estilo == estilo => ultimo.texto.push_str(&glifo),
            _ => tramos.push(Tramo {
                estilo,
                texto: glifo,
            }),
        }
    }
    Ok(tramos)
}

/// Un símbolo vacío o solo de espacios, tabuladores y controles se pinta como un
/// espacio.
fn es_simbolo_en_blanco(simbolo: &str) -> bool {
    simbolo.chars().all(|c| c.is_whitespace() || c.is_control())
}

/// El texto que se escribe para un símbolo: él mismo si tiene glifo; si no, su
/// degradación, siempre que sea **un** carácter distinto de `?` con glifo. El
/// error no lleva la ubicación (la pone quien llama).
fn glifo_de(
    simbolo: &str,
    cobertura: &Cobertura,
    degradar: &dyn Fn(&str) -> String,
) -> Result<String, String> {
    if es_simbolo_en_blanco(simbolo) {
        return Ok(" ".to_string());
    }
    // Un símbolo con algo visible no puede llevar además saltos, tabuladores,
    // controles ni invisibles que falsean la lectura: descuadrarían la rejilla o
    // el texto se vería al revés.
    if let Some(c) = simbolo
        .chars()
        .find(|c| c.is_control() || (c.is_whitespace() && *c != ' ') || es_invisible_que_engana(*c))
    {
        return Err(format!(
            "el símbolo {} lleva el carácter {} (U+{:04X}), que no puede ir en una captura",
            recortar(simbolo, 16),
            c.escape_default(),
            u32::from(c)
        ));
    }
    if cobertura.cubre(simbolo) {
        return Ok(simbolo.to_string());
    }

    let degradado = degradar(simbolo);
    let mut letras = degradado.chars();
    let valido = match (letras.next(), letras.next()) {
        (Some(c), None) => {
            c != '?'
                && (c == ' ' || (!c.is_whitespace() && !c.is_control()))
                && cobertura.cubre(&degradado)
        }
        _ => false,
    };
    if valido {
        return Ok(degradado);
    }

    let sin_glifo: Vec<String> = simbolo
        .chars()
        .filter(|c| !cobertura.cubre_caracter(*c))
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect();
    Err(format!(
        "el símbolo {} ({}) no tiene glifo en JetBrains Mono ni en el subconjunto de \
         símbolos, y su degradación {} no vale: debe ser un solo carácter, distinto de \
         «?», que tenga glifo",
        recortar(simbolo, 16),
        sin_glifo.join(", "),
        recortar_debug(&degradado, 24)
    ))
}

/// Resuelve el estilo de una celda (ver `exportar` para el orden).
fn resolver_estilo(celda: &CeldaCaptura) -> Estilo {
    let mut fg = resolver_color(celda.fg, TEXTO);
    let mut bg = resolver_color(celda.bg, FONDO);
    if celda.invertido {
        std::mem::swap(&mut fg, &mut bg);
    }
    if celda.tenue {
        fg = mezclar(fg, bg);
    }
    if celda.oculta {
        fg = bg;
    }
    Estilo {
        fg,
        bg,
        negrita: celda.negrita,
        cursiva: celda.cursiva,
        subrayado: celda.subrayado,
    }
}

fn resolver_color(color: ColorCaptura, por_defecto: Rgb) -> Rgb {
    match color {
        ColorCaptura::Reset => por_defecto,
        ColorCaptura::Rgb(r, g, b) => (r, g, b),
        ColorCaptura::Indexado(n) => color_xterm(n),
    }
}

/// La paleta de 256 colores de xterm: 0 a 15 la tabla ANSI estándar, 16 a 231 el
/// cubo 6×6×6 y 232 a 255 los grises.
fn color_xterm(n: u8) -> Rgb {
    const ANSI: [Rgb; 16] = [
        (0x00, 0x00, 0x00),
        (0xcd, 0x00, 0x00),
        (0x00, 0xcd, 0x00),
        (0xcd, 0xcd, 0x00),
        (0x00, 0x00, 0xee),
        (0xcd, 0x00, 0xcd),
        (0x00, 0xcd, 0xcd),
        (0xe5, 0xe5, 0xe5),
        (0x7f, 0x7f, 0x7f),
        (0xff, 0x00, 0x00),
        (0x00, 0xff, 0x00),
        (0xff, 0xff, 0x00),
        (0x5c, 0x5c, 0xff),
        (0xff, 0x00, 0xff),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0xff),
    ];
    const NIVELES: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match n {
        0..=15 => ANSI[usize::from(n)],
        16..=231 => {
            let k = usize::from(n - 16);
            (NIVELES[k / 36], NIVELES[(k / 6) % 6], NIVELES[k % 6])
        }
        232..=255 => {
            let gris = 8 + 10 * (n - 232);
            (gris, gris, gris)
        }
    }
}

/// La mezcla a partes iguales, redondeando hacia abajo.
fn mezclar(a: Rgb, b: Rgb) -> Rgb {
    let medio = |x: u8, y: u8| ((u16::from(x) + u16::from(y)) / 2) as u8;
    (medio(a.0, b.0), medio(a.1, b.1), medio(a.2, b.2))
}

fn hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2)
}

/// Un tramo en HTML. El texto va escapado y, si el estilo no es el de por defecto,
/// dentro de su `<span>`.
fn fila_a_html(mut tramos: Vec<Tramo>, clases: &mut Clases) -> String {
    // Los espacios finales del estilo por defecto no se ven: fuera.
    if let Some(ultimo) = tramos.last_mut() {
        if ultimo.estilo == ESTILO_POR_DEFECTO {
            ultimo
                .texto
                .truncate(ultimo.texto.trim_end_matches(' ').len());
            if ultimo.texto.is_empty() {
                tramos.pop();
            }
        }
    }
    let mut salida = String::new();
    for tramo in tramos {
        let texto = escapar(&tramo.texto);
        if tramo.estilo == ESTILO_POR_DEFECTO {
            salida.push_str(&texto);
        } else {
            let n = clases.numero(tramo.estilo);
            salida.push_str(&format!("<span class=\"c{n}\">{texto}</span>"));
        }
    }
    salida
}

/// Escapa el texto para HTML: `&`, `<`, `>`, `"` y `'`.
fn escapar(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '&' => salida.push_str("&amp;"),
            '<' => salida.push_str("&lt;"),
            '>' => salida.push_str("&gt;"),
            '"' => salida.push_str("&quot;"),
            '\'' => salida.push_str("&#39;"),
            otro => salida.push(otro),
        }
    }
    salida
}

/// `capturas.css`: la regla `.captura` y una `.cN` por clase, con solo lo que
/// cambia respecto a `.captura`. Una regla por línea.
fn css_de(estilos: &[Estilo]) -> String {
    let mut css = format!(
        ".captura{{background:{};color:{};font-family:\"JetBrains Mono\",\"MAGI Simbolos\",monospace;\
font-variant-ligatures:none;font-feature-settings:\"liga\" 0,\"calt\" 0;line-height:1.32;\
white-space:pre;overflow-x:auto;margin:0;padding:.75rem 1rem}}\n",
        hex(FONDO),
        hex(TEXTO)
    );
    for (n, estilo) in estilos.iter().enumerate() {
        let mut declaraciones: Vec<String> = Vec::new();
        if estilo.fg != TEXTO {
            declaraciones.push(format!("color:{}", hex(estilo.fg)));
        }
        if estilo.bg != FONDO {
            declaraciones.push(format!("background:{}", hex(estilo.bg)));
        }
        if estilo.negrita {
            declaraciones.push("font-weight:700".to_string());
        }
        if estilo.cursiva {
            declaraciones.push("font-style:italic".to_string());
        }
        if estilo.subrayado {
            declaraciones.push("text-decoration:underline".to_string());
        }
        css.push_str(&format!(".c{n}{{{}}}\n", declaraciones.join(";")));
    }
    css
}

/// Los primeros `max` caracteres de un texto no fiable, entre «», para los mensajes.
fn recortar(texto: &str, max: usize) -> String {
    let mut salida: String = texto.chars().take(max).collect();
    if texto.chars().count() > max {
        salida.push('…');
    }
    format!("«{}»", salida.escape_debug())
}

/// Como `recortar` pero con `Debug` (comillas dobles): para lo que devuelve el
/// degradador, donde importa ver si está vacío o lleva espacios.
fn recortar_debug(texto: &str, max: usize) -> String {
    let corto: String = texto.chars().take(max).collect();
    let puntos = if texto.chars().count() > max {
        "…"
    } else {
        ""
    };
    format!("{corto:?}{puntos}")
}

/// Las únicas entidades que puede llevar un fragmento (las que escribe `escapar`).
const ENTIDADES: [&str; 5] = ["&amp;", "&lt;", "&gt;", "&quot;", "&#39;"];

/// Una clase de estilo: `c` y de 1 a 9 dígitos ASCII.
fn es_clase_de_estilo(clase: &str) -> bool {
    match clase.strip_prefix('c') {
        Some(digitos) => {
            (1..=9).contains(&digitos.len()) && digitos.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

/// Un fragmento de captura que se va a poner en una página con `|safe`: solo
/// `<pre class="captura" aria-hidden="true">` con `<span class="cN">` y texto
/// escapado. Cualquier otra etiqueta, atributo (`style` incluido) o `<`/`&` sin
/// escapar es un error. Lo usa `construir` antes de insertar el fragmento.
///
/// Es la barrera de seguridad de lo que se inserta sin escapar, así que falla
/// cerrado:
/// - Nada antes ni después del `<pre>` (ni siquiera espacios: quien lea un fichero
///   debe recortarlo) ni dos `<pre>`.
/// - El `<pre>` lleva exactamente `class="captura"` y `aria-hidden="true"`.
/// - Dentro, solo texto y `<span class="c<dígitos>">…</span>` sin más atributos,
///   sin anidar y bien cerrados. Ni comentarios, ni otras etiquetas (en cualquier
///   caja), ni `<` suelto.
/// - En el texto, un `&` solo puede ser `&amp;`, `&lt;`, `&gt;`, `&quot;` o `&#39;`,
///   y no hay controles salvo el salto de línea ni invisibles que falsean la
///   lectura (marcas de dirección, espacios de ancho cero, separadores de línea).
/// - Como mucho 1 MiB.
pub fn validar_fragmento(html: &str) -> Resultado<()> {
    validar_fragmento_interno(html).map_err(|(posicion, que)| {
        ErrorWeb::dato_raro(
            "fragmento de captura",
            format!("línea {}: {que}", html::linea_de(html, posicion)),
        )
    })
}

/// Como `validar_fragmento`, con la posición en bytes del fallo.
fn validar_fragmento_interno(html: &str) -> Result<(), (usize, String)> {
    if html.len() > MAX_BYTES {
        return Err((
            0,
            format!(
                "el fragmento pesa {} bytes y el máximo es {MAX_BYTES} (1 MiB)",
                html.len()
            ),
        ));
    }
    let tokens = html::tokenizar_detallado(html).map_err(|e| (e.posicion, e.que))?;

    #[derive(PartialEq)]
    enum Estado {
        /// Falta el `<pre>` de apertura.
        Inicio,
        /// Dentro del `<pre>`, fuera de un `<span>`.
        EnPre,
        /// Dentro de un `<span>`.
        EnSpan,
        /// Tras `</pre>`.
        Fin,
    }
    let mut estado = Estado::Inicio;

    for token in &tokens {
        match (token, &estado) {
            (Token::Etiqueta(et), Estado::Inicio) => {
                let atributos_ok = et.nombre == "pre"
                    && !et.cierre
                    && !et.autocierre
                    && et.atributos.len() == 2
                    && et.atributo("class") == Some("captura")
                    && et.atributo("aria-hidden") == Some("true");
                if !atributos_ok {
                    return Err((
                        et.inicio,
                        format!(
                            "el fragmento debe empezar por <pre class=\"captura\" \
                             aria-hidden=\"true\"> y nada más, y empieza por {}",
                            describir_etiqueta(et)
                        ),
                    ));
                }
                estado = Estado::EnPre;
            }
            (Token::Etiqueta(et), Estado::EnPre) => {
                if et.nombre == "span" && !et.cierre {
                    let clase_ok = !et.autocierre
                        && et.atributos.len() == 1
                        && et.atributo("class").is_some_and(es_clase_de_estilo);
                    if !clase_ok {
                        return Err((
                            et.inicio,
                            format!(
                                "solo se admite <span class=\"c<N>\"> sin más atributos y {} no lo es",
                                describir_etiqueta(et)
                            ),
                        ));
                    }
                    estado = Estado::EnSpan;
                } else if et.nombre == "pre" && et.cierre {
                    if et.autocierre || !et.atributos.is_empty() {
                        return Err((et.inicio, "</pre> con atributos o con «/>»".to_string()));
                    }
                    estado = Estado::Fin;
                } else {
                    return Err((
                        et.inicio,
                        format!(
                            "etiqueta no admitida dentro de la captura: {}",
                            describir_etiqueta(et)
                        ),
                    ));
                }
            }
            (Token::Etiqueta(et), Estado::EnSpan) => {
                if et.nombre == "span" && et.cierre {
                    if et.autocierre || !et.atributos.is_empty() {
                        return Err((et.inicio, "</span> con atributos o con «/>»".to_string()));
                    }
                    estado = Estado::EnPre;
                } else if et.nombre == "span" {
                    return Err((et.inicio, "<span> dentro de otro <span>".to_string()));
                } else if et.nombre == "pre" && et.cierre {
                    return Err((et.inicio, "</pre> con un <span> sin cerrar".to_string()));
                } else {
                    return Err((
                        et.inicio,
                        format!(
                            "etiqueta no admitida dentro de la captura: {}",
                            describir_etiqueta(et)
                        ),
                    ));
                }
            }
            (Token::Etiqueta(et), Estado::Fin) => {
                return Err((
                    et.inicio,
                    format!("contenido tras </pre>: {}", describir_etiqueta(et)),
                ));
            }
            (Token::Texto { inicio, fin }, Estado::EnPre | Estado::EnSpan) => {
                let texto = html.get(*inicio..*fin).ok_or((
                    *inicio,
                    "texto con límites que no son de carácter".to_string(),
                ))?;
                validar_texto(texto, *inicio)?;
            }
            (Token::Texto { inicio, .. }, Estado::Inicio) => {
                return Err((*inicio, "contenido antes del <pre>".to_string()));
            }
            (Token::Texto { inicio, .. }, Estado::Fin) => {
                return Err((*inicio, "contenido tras </pre>".to_string()));
            }
            (Token::Comentario(_), _) => {
                return Err((0, "comentarios HTML no admitidos".to_string()));
            }
            (Token::Doctype, _) => {
                return Err((0, "<!DOCTYPE> no admitido en un fragmento".to_string()));
            }
            (
                Token::Crudo {
                    etiqueta, inicio, ..
                },
                _,
            ) => {
                return Err((
                    *inicio,
                    format!("contenido en bruto de <{etiqueta}> no admitido"),
                ));
            }
        }
    }

    match estado {
        Estado::Fin => Ok(()),
        Estado::Inicio => Err((0, "fragmento vacío: falta el <pre>".to_string())),
        Estado::EnPre => Err((html.len(), "falta </pre>".to_string())),
        Estado::EnSpan => Err((html.len(), "falta </span> y </pre>".to_string())),
    }
}

/// Una etiqueta para un mensaje: su nombre y los de sus atributos (nunca los
/// valores, que no son de fiar).
fn describir_etiqueta(et: &html::Etiqueta) -> String {
    let nombre = recortar(&et.nombre, 24);
    let atributos: Vec<String> = et
        .atributos
        .iter()
        .take(6)
        .map(|(n, _)| recortar(n, 24))
        .collect();
    let barra = if et.cierre { "cierre de " } else { "" };
    if atributos.is_empty() {
        format!("{barra}<{nombre}>")
    } else {
        format!(
            "{barra}<{nombre}> con los atributos {}",
            atributos.join(", ")
        )
    }
}

/// Caracteres que no se ven pero cambian cómo se lee un texto: marcas de dirección
/// y aislados bidireccionales (el texto se vería al revés), separadores de línea y
/// de párrafo, espacios de ancho cero, marcas de formato y la marca de orden de
/// bytes. Parte del conjunto de `ordenes::es_invisible_que_engana`, copiado para no
/// tocar ese módulo, y añade la marca de letra árabe (U+061C, que ordena el texto
/// como una marca de dirección) y los caracteres de etiqueta (U+E0000 a U+E007F,
/// invisibles y capaces de esconder un texto entero): un fragmento versionado en
/// `web/capturas/` lo puede editar cualquiera con una PR y no debe poder esconder
/// texto al revés ni oculto.
fn es_invisible_que_engana(c: char) -> bool {
    matches!(
        c,
        '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// El texto de una captura: sin `<`, con `&` solo en las cinco entidades, sin
/// controles salvo el salto de línea y sin invisibles que falseen la lectura.
fn validar_texto(texto: &str, base: usize) -> Result<(), (usize, String)> {
    for (i, c) in texto.char_indices() {
        match c {
            c if es_invisible_que_engana(c) => {
                return Err((
                    base + i,
                    format!(
                        "carácter invisible U+{:04X} en el texto (cambia cómo se lee)",
                        u32::from(c)
                    ),
                ));
            }
            '<' => return Err((base + i, "«<» sin escapar en el texto".to_string())),
            '&' => {
                let resto = &texto[i..];
                if !ENTIDADES.iter().any(|e| resto.starts_with(e)) {
                    return Err((
                        base + i,
                        format!(
                            "«&» que no es una de las entidades admitidas ({}): {}",
                            ENTIDADES.join(" "),
                            recortar(resto, 12)
                        ),
                    ));
                }
            }
            '\n' => {}
            c if c.is_control() => {
                return Err((
                    base + i,
                    format!("carácter de control U+{:04X} en el texto", u32::from(c)),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Las propiedades que puede llevar una regla de `capturas.css`.
const PROPIEDADES: [&str; 17] = [
    "color",
    "background",
    "background-color",
    "font-weight",
    "font-style",
    "text-decoration",
    "font-family",
    "font-variant-ligatures",
    "font-feature-settings",
    "line-height",
    "white-space",
    "overflow-x",
    "margin",
    "padding",
    "display",
    "font-size",
    "tab-size",
];

/// `capturas.css`: solo reglas `.captura` y `.cN` con declaraciones de color,
/// fondo y peso/estilo de fuente con valores `#rrggbb` o palabras clave; nada de
/// `url(`, `@import`, `expression(` ni llaves anidadas.
///
/// Admite únicamente reglas `.captura{…}` y `.c<dígitos>{…}` (sin selectores
/// compuestos, sin `@`, sin comentarios, sin llaves anidadas) con declaraciones de
/// la lista blanca (`PROPIEDADES`) y valores que, además de no llevar `url(`,
/// `@import`, `expression(`, `javascript:`, `\`, `<` ni `;` o `}` colados, sean solo
/// colores `#rgb`/`#rrggbb`, números con unidad de longitud, palabras clave y
/// cadenas de nombres de fuente (sin paréntesis: no hay funciones, así que nada
/// carga recursos). Como mucho 1 MiB.
pub fn validar_css(css: &str) -> Resultado<()> {
    validar_css_interno(css).map_err(|(posicion, que)| {
        ErrorWeb::dato_raro(
            "CSS de las capturas",
            format!("línea {}: {que}", html::linea_de(css, posicion)),
        )
    })
}

fn es_blanco_css(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

fn validar_css_interno(css: &str) -> Result<(), (usize, String)> {
    if css.len() > MAX_BYTES {
        return Err((
            0,
            format!(
                "el CSS pesa {} bytes y el máximo es {MAX_BYTES} (1 MiB)",
                css.len()
            ),
        ));
    }
    if let Some(i) = css.find("/*") {
        return Err((i, "comentario CSS no admitido".to_string()));
    }
    if let Some(i) = css.find('@') {
        return Err((
            i,
            "reglas «@» (como @import o @font-face) no admitidas".to_string(),
        ));
    }

    let mut pos = 0;
    loop {
        // Entre reglas solo puede haber blancos.
        let resto = &css[pos..];
        let blancos = resto.len() - resto.trim_start_matches(es_blanco_css).len();
        pos += blancos;
        if pos >= css.len() {
            return Ok(());
        }

        let resto = &css[pos..];
        let Some(llave) = resto.find('{') else {
            return Err((pos, "regla sin «{»".to_string()));
        };
        let selector = resto[..llave].trim_matches(es_blanco_css);
        let selector_ok =
            selector == ".captura" || selector.strip_prefix('.').is_some_and(es_clase_de_estilo);
        if !selector_ok {
            return Err((
                pos,
                format!(
                    "selector {} no admitido: solo «.captura» y «.c<N>»",
                    recortar(selector, 40)
                ),
            ));
        }

        let inicio_cuerpo = pos + llave + 1;
        let cuerpo_y_mas = &css[inicio_cuerpo..];
        let Some(i) = cuerpo_y_mas.find(['{', '}']) else {
            return Err((pos, "regla sin cerrar (falta «}»)".to_string()));
        };
        if cuerpo_y_mas.as_bytes().get(i) == Some(&b'{') {
            return Err((
                inicio_cuerpo + i,
                "llaves anidadas no admitidas".to_string(),
            ));
        }
        validar_declaraciones(&cuerpo_y_mas[..i], inicio_cuerpo)?;
        pos = inicio_cuerpo + i + 1;
    }
}

fn validar_declaraciones(cuerpo: &str, base: usize) -> Result<(), (usize, String)> {
    let mut desplazamiento = 0;
    for declaracion in cuerpo.split(';') {
        let aqui = base + desplazamiento;
        desplazamiento += declaracion.len() + 1;
        let declaracion = declaracion.trim_matches(es_blanco_css);
        if declaracion.is_empty() {
            continue;
        }
        let Some((nombre, valor)) = declaracion.split_once(':') else {
            return Err((
                aqui,
                format!("declaración {} sin «:»", recortar(declaracion, 40)),
            ));
        };
        let nombre = nombre.trim_matches(es_blanco_css).to_ascii_lowercase();
        if !PROPIEDADES.contains(&nombre.as_str()) {
            return Err((
                aqui,
                format!("propiedad {} no admitida", recortar(&nombre, 40)),
            ));
        }
        let valor = valor.trim_matches(es_blanco_css);
        validar_valor(&nombre, valor).map_err(|que| (aqui, format!("en «{nombre}»: {que}")))?;
    }
    Ok(())
}

/// Una pieza de un valor CSS.
#[derive(Debug, PartialEq)]
enum Ficha {
    /// `#rgb`, `#rgba`, `#rrggbb` o `#rrggbbaa`.
    Hex,
    /// Un número con su unidad (vacía si no la lleva).
    Numero { unidad: String },
    /// Una palabra clave, en minúsculas.
    Palabra(String),
    /// Una cadena entre comillas dobles (un nombre de fuente o una etiqueta).
    Cadena(String),
}

const UNIDADES: [&str; 7] = ["", "px", "em", "rem", "ch", "pt", "%"];
const MAX_FICHAS: usize = 16;

fn validar_valor(propiedad: &str, valor: &str) -> Result<(), String> {
    if valor.is_empty() {
        return Err("valor vacío".to_string());
    }
    // Mensajes concretos para lo que un valor no puede llevar nunca; el léxico de
    // más abajo ya lo excluye, esto es el cinturón.
    let minusculas = valor.to_ascii_lowercase();
    for (prohibido, nombre) in [
        ("url(", "url("),
        ("@import", "@import"),
        ("expression(", "expression("),
        ("javascript:", "javascript:"),
        ("\\", "una barra invertida"),
        ("<", "«<»"),
        ("}", "«}»"),
        ("{", "«{»"),
    ] {
        if minusculas.contains(prohibido) {
            return Err(format!("el valor lleva {nombre}, que no se admite"));
        }
    }
    let fichas = fichar_valor(valor)?;
    if fichas.is_empty() {
        return Err("valor vacío".to_string());
    }
    admitir_fichas(propiedad, &fichas)
}

/// Parte un valor en fichas. Solo conoce colores hexadecimales, números con unidad,
/// palabras y cadenas simples: cualquier otro carácter (paréntesis, `!`, `:`, `/`,
/// comillas simples…) es un error.
fn fichar_valor(valor: &str) -> Result<Vec<Ficha>, String> {
    let c: Vec<char> = valor.chars().collect();
    let separador = |ch: char| es_blanco_css(ch) || ch == ',';
    let mut fichas = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if separador(ch) {
            i += 1;
            continue;
        }
        if fichas.len() >= MAX_FICHAS {
            return Err(format!("más de {MAX_FICHAS} piezas en un valor"));
        }
        let siguiente = c.get(i + 1).copied();
        if ch == '"' {
            i += 1;
            let ini = i;
            while i < c.len() && c[i] != '"' {
                if !(c[i].is_ascii_alphanumeric() || matches!(c[i], ' ' | '_' | '.' | '-')) {
                    return Err(format!(
                        "carácter {} no admitido dentro de una cadena",
                        recortar(&c[i].to_string(), 4)
                    ));
                }
                i += 1;
            }
            if i >= c.len() {
                return Err("cadena sin cerrar".to_string());
            }
            if i - ini > 64 {
                return Err("cadena de más de 64 caracteres".to_string());
            }
            fichas.push(Ficha::Cadena(c[ini..i].iter().collect()));
            i += 1;
        } else if ch == '#' {
            i += 1;
            let ini = i;
            while i < c.len() && c[i].is_ascii_hexdigit() {
                i += 1;
            }
            if !matches!(i - ini, 3 | 4 | 6 | 8) {
                return Err("color «#…» que no es #rgb, #rgba, #rrggbb ni #rrggbbaa".to_string());
            }
            fichas.push(Ficha::Hex);
        } else if ch.is_ascii_digit()
            || (matches!(ch, '.' | '-')
                && siguiente.is_some_and(|s| s.is_ascii_digit() || s == '.'))
        {
            if ch == '-' {
                i += 1;
            }
            let ini = i;
            let mut puntos = 0;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                if c[i] == '.' {
                    puntos += 1;
                }
                i += 1;
            }
            if puntos > 1 || !c[ini..i].iter().any(char::is_ascii_digit) {
                return Err("número mal formado".to_string());
            }
            let ini_unidad = i;
            while i < c.len() && (c[i].is_ascii_alphabetic() || c[i] == '%') {
                i += 1;
            }
            let unidad: String = c[ini_unidad..i]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase();
            if !UNIDADES.contains(&unidad.as_str()) {
                return Err(format!("unidad {} no admitida", recortar(&unidad, 12)));
            }
            fichas.push(Ficha::Numero { unidad });
        } else if ch.is_ascii_alphabetic()
            || (ch == '-' && siguiente.is_some_and(|s| s.is_ascii_alphabetic()))
        {
            let ini = i;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || matches!(c[i], '-' | '_')) {
                i += 1;
            }
            let palabra: String = c[ini..i].iter().collect::<String>().to_ascii_lowercase();
            if palabra.len() > 40 {
                return Err("palabra de más de 40 caracteres".to_string());
            }
            fichas.push(Ficha::Palabra(palabra));
        } else {
            return Err(format!(
                "carácter {} (U+{:04X}) no admitido en un valor",
                recortar(&ch.to_string(), 4),
                u32::from(ch)
            ));
        }
        // Cada pieza termina en un separador o en el final del valor.
        if i < c.len() && !separador(c[i]) {
            return Err(format!(
                "carácter {} (U+{:04X}) pegado a la pieza anterior",
                recortar(&c[i].to_string(), 4),
                u32::from(c[i])
            ));
        }
    }
    Ok(fichas)
}

/// Comprueba que las fichas son las que admite la propiedad.
fn admitir_fichas(propiedad: &str, fichas: &[Ficha]) -> Result<(), String> {
    let palabra_de = |f: &Ficha| match f {
        Ficha::Palabra(p) => Some(p.clone()),
        _ => None,
    };
    let todas_palabras = |lista: &[&str], min: usize, max: usize| -> Result<(), String> {
        if fichas.len() < min || fichas.len() > max {
            return Err(format!("se esperaban de {min} a {max} piezas"));
        }
        for f in fichas {
            match palabra_de(f) {
                Some(p) if lista.contains(&p.as_str()) => {}
                _ => {
                    return Err(format!(
                        "pieza {f:?} no admitida (admite {})",
                        lista.join(", ")
                    ))
                }
            }
        }
        Ok(())
    };
    let numero_o_palabra = |lista: &[&str]| -> Result<(), String> {
        match fichas {
            [Ficha::Numero { .. }] => Ok(()),
            [Ficha::Palabra(p)] if lista.contains(&p.as_str()) => Ok(()),
            _ => Err(format!(
                "se esperaba un número o una de: {}",
                lista.join(", ")
            )),
        }
    };
    match propiedad {
        "color" | "background" | "background-color" => match fichas {
            [Ficha::Hex] | [Ficha::Palabra(_)] => Ok(()),
            _ => Err("se esperaba un color «#rrggbb» o una palabra clave".to_string()),
        },
        "font-weight" => numero_o_palabra(&["normal", "bold", "bolder", "lighter"]),
        "font-style" => todas_palabras(&["normal", "italic", "oblique"], 1, 1),
        "text-decoration" => {
            todas_palabras(&["none", "underline", "overline", "line-through"], 1, 4)
        }
        "font-family" => {
            if fichas.is_empty() || fichas.len() > 8 {
                return Err("se esperaban de 1 a 8 familias".to_string());
            }
            match fichas
                .iter()
                .all(|f| matches!(f, Ficha::Cadena(_) | Ficha::Palabra(_)))
            {
                true => Ok(()),
                false => Err("solo nombres de fuente (cadenas o palabras)".to_string()),
            }
        }
        "font-variant-ligatures" => todas_palabras(
            &[
                "normal",
                "none",
                "common-ligatures",
                "no-common-ligatures",
                "discretionary-ligatures",
                "no-discretionary-ligatures",
                "historical-ligatures",
                "no-historical-ligatures",
                "contextual",
                "no-contextual",
            ],
            1,
            5,
        ),
        "font-feature-settings" => {
            if matches!(fichas, [Ficha::Palabra(p)] if p == "normal") {
                return Ok(());
            }
            let mut i = 0;
            while i < fichas.len() {
                match &fichas[i] {
                    Ficha::Cadena(etiqueta)
                        if etiqueta.len() == 4
                            && etiqueta.bytes().all(|b| b.is_ascii_alphanumeric()) => {}
                    _ => {
                        return Err(
                            "se esperaba una etiqueta de cuatro caracteres entre comillas"
                                .to_string(),
                        )
                    }
                }
                i += 1;
                match fichas.get(i) {
                    Some(Ficha::Numero { unidad }) if unidad.is_empty() => i += 1,
                    Some(Ficha::Palabra(p)) if p == "on" || p == "off" => i += 1,
                    _ => {}
                }
            }
            Ok(())
        }
        "line-height" | "font-size" => numero_o_palabra(&[
            "normal", "xx-small", "x-small", "small", "medium", "large", "x-large", "xx-large",
            "smaller", "larger",
        ]),
        "white-space" => todas_palabras(
            &[
                "normal",
                "nowrap",
                "pre",
                "pre-wrap",
                "pre-line",
                "break-spaces",
            ],
            1,
            1,
        ),
        "overflow-x" => todas_palabras(&["visible", "hidden", "clip", "scroll", "auto"], 1, 1),
        "margin" | "padding" => {
            if fichas.is_empty() || fichas.len() > 4 {
                return Err("se esperaban de 1 a 4 medidas".to_string());
            }
            let admite_auto = propiedad == "margin";
            for f in fichas {
                match f {
                    Ficha::Numero { .. } => {}
                    Ficha::Palabra(p) if admite_auto && p == "auto" => {}
                    _ => return Err(format!("pieza {f:?} no admitida (solo medidas)")),
                }
            }
            Ok(())
        }
        "display" => todas_palabras(&["block", "inline", "inline-block", "none"], 1, 1),
        "tab-size" => match fichas {
            [Ficha::Numero { .. }] => Ok(()),
            _ => Err("se esperaba un número".to_string()),
        },
        // Las propiedades de `PROPIEDADES` están todas arriba.
        otra => Err(format!(
            "propiedad {} sin reglas de valor",
            recortar(otra, 40)
        )),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_paleta_xterm_cubre_los_256_colores() {
        // Los 16 primeros, la tabla ANSI estándar de xterm.
        let ansi = [
            "000000", "cd0000", "00cd00", "cdcd00", "0000ee", "cd00cd", "00cdcd", "e5e5e5",
            "7f7f7f", "ff0000", "00ff00", "ffff00", "5c5cff", "ff00ff", "00ffff", "ffffff",
        ];
        for (n, esperado) in ansi.iter().enumerate() {
            assert_eq!(
                hex(color_xterm(n as u8)),
                format!("#{esperado}"),
                "índice {n}"
            );
        }
        // El cubo: los extremos y un par de puntos intermedios.
        assert_eq!(color_xterm(16), (0, 0, 0));
        assert_eq!(color_xterm(21), (0, 0, 255));
        assert_eq!(color_xterm(196), (255, 0, 0));
        assert_eq!(color_xterm(231), (255, 255, 255));
        assert_eq!(color_xterm(67), (95, 135, 175));
        // Los grises.
        assert_eq!(color_xterm(232), (8, 8, 8));
        assert_eq!(color_xterm(255), (238, 238, 238));
        // Ningún índice entra en pánico.
        for n in 0..=255u8 {
            let _ = color_xterm(n);
        }
    }

    #[test]
    fn la_mezcla_tenue_es_a_partes_iguales_y_redondea_hacia_abajo() {
        assert_eq!(mezclar((0, 0, 0), (255, 255, 255)), (127, 127, 127));
        assert_eq!(mezclar((10, 20, 30), (10, 20, 30)), (10, 20, 30));
        assert_eq!(mezclar((255, 255, 255), (255, 255, 255)), (255, 255, 255));
    }

    #[test]
    fn el_lexico_de_valores_distingue_las_piezas() {
        let fichas = fichar_valor("\"liga\" 0,\"calt\" 0").unwrap();
        assert_eq!(
            fichas,
            vec![
                Ficha::Cadena("liga".to_string()),
                Ficha::Numero {
                    unidad: String::new()
                },
                Ficha::Cadena("calt".to_string()),
                Ficha::Numero {
                    unidad: String::new()
                },
            ]
        );
        assert_eq!(fichar_valor("#0A0a0a").unwrap(), vec![Ficha::Hex]);
        assert_eq!(
            fichar_valor(".75rem 1rem").unwrap(),
            vec![
                Ficha::Numero {
                    unidad: "rem".to_string()
                },
                Ficha::Numero {
                    unidad: "rem".to_string()
                }
            ]
        );
        assert_eq!(
            fichar_valor("-0.5em").unwrap(),
            vec![Ficha::Numero {
                unidad: "em".to_string()
            }]
        );
        // Lo que no es de este léxico es un error, nunca se ignora.
        for malo in [
            "url(x)",
            "rgb(1,2,3)",
            "#12",
            "#12345",
            "red!important",
            "1.2.3",
            "12qq",
            "\"sin cerrar",
            "\"a;b\"",
            "'x'",
            "a/b",
            "a:b",
            "é",
            "1e3",
            "#fff#fff",
        ] {
            assert!(fichar_valor(malo).is_err(), "debería fallar: {malo}");
        }
    }

    #[test]
    fn una_clase_de_estilo_son_una_c_y_de_uno_a_nueve_digitos() {
        for bien in ["c0", "c1", "c123456789"] {
            assert!(es_clase_de_estilo(bien), "{bien}");
        }
        for mal in [
            "",
            "c",
            "C1",
            "c1a",
            "c-1",
            "c1234567890",
            "c١",
            "x1",
            " c1",
        ] {
            assert!(!es_clase_de_estilo(mal), "{mal:?}");
        }
    }
}
