//! De los CHANGELOG a la página de versiones (informe §7.4 y D455).
//!
//! Se leen del CHANGELOG de la etiqueta de la última versión publicada de cada
//! canal. Fuera los comentarios de plantilla y la sección «Sin publicar», que no
//! casa con el encabezado de una versión. Todo lo que llega de un CHANGELOG es
//! entrada no fiable: el HTML en bruto se escapa como texto y los destinos de los
//! enlaces se limitan (ver `detalle_a_html`).
//!
//! Cómo se lee un CHANGELOG:
//!
//! 1. Tope de tamaño (`TOPE_CHANGELOG`) y fuera los comentarios `<!-- … -->`
//!    (también los de varias líneas; uno sin cerrar es un error, no se traga el
//!    resto). Los saltos de línea de un comentario se conservan, para que las
//!    líneas de los errores sean las del fichero.
//! 2. Cada `## [X.Y.Z] - AAAA-MM-DD` abre una versión. Un `## [Sin publicar]` (o
//!    cualquier otro `## […]` que no empiece por una cifra) abre una sección que
//!    se descarta con todo lo que cuelga de ella. Un `## [1…` que no es un
//!    encabezado de versión exacto (ceros a la izquierda, sufijos, fecha
//!    imposible) es un error: callarlo escondería una versión.
//! 3. De cada cuerpo salen `**Resumen.** …` y `**Summary.** …` (el párrafo entero,
//!    hasta la línea en blanco, como texto plano); lo demás es el detalle, en
//!    Markdown, que `detalle_a_html` pasa a HTML saneado.
//!
//! Los encabezados se reconocen línea a línea. Un bloque de código abierto en la
//! columna 0 esconde lo que lleva dentro (también un `## [X.Y.Z] - …`); uno sin
//! cerrar es un error, para que no se trague las versiones siguientes sin avisar.
//!
//! Las definiciones de enlace (`[1.6.1]: https://…`) cuelgan de la última sección
//! del fichero, pero valen para todo él: se leen una vez del texto entero y se
//! usan para resolver las referencias de cualquier versión. No salen como texto.

use std::collections::HashSet;

use pulldown_cmark::{
    BrokenLink, CowStr, Event, HeadingLevel, LinkType, Options, Parser, RefDefs, Tag, TagEnd,
};

use crate::comprobar::analizar_https;
use crate::enlaces;
use crate::error::{ErrorWeb, Resultado};
use crate::ordenes::es_invisible_que_engana;

/// Tope de un CHANGELOG (2 MiB) y de una entrada (512 KiB).
pub const TOPE_CHANGELOG: usize = 2 * 1024 * 1024;
pub const TOPE_ENTRADA: usize = 512 * 1024;

/// Tope del HTML que sale de una entrada: un texto lleno de `&` o de `<` crece al
/// escaparse, pero no tanto.
const TOPE_SALIDA: usize = 8 * TOPE_ENTRADA;

/// Tope de un resumen (una o dos líneas; esto es mucho más de lo que cabe).
const TOPE_RESUMEN: usize = 4000;

/// Versiones como mucho en un CHANGELOG.
const MAX_VERSIONES: usize = 2000;

/// Listas, citas y énfasis abiertos a la vez como mucho: lo que pase de ahí sale
/// sin su marcado (el texto se conserva). Un navegador corta a los 512 niveles.
const NIVELES_MAXIMOS: usize = 24;

/// Tope de un destino de enlace.
const MAX_DESTINO: usize = 2048;

const MARCA_RESUMEN: &str = "**Resumen.**";
const MARCA_SUMMARY: &str = "**Summary.**";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaChangelog {
    /// `1.6.1`
    pub numero: String,
    /// `AAAA-MM-DD`
    pub fecha: String,
    /// El párrafo `**Resumen.**`, sin el marcador (texto plano).
    pub resumen_es: String,
    /// El párrafo `**Summary.**`, sin el marcador (texto plano).
    pub resumen_en: String,
    /// El resto, en castellano, ya como HTML saneado.
    pub detalle_html: String,
}

/// Parte un CHANGELOG en versiones, de la más nueva a la más vieja tal cual están
/// en el fichero. Cada versión tiene que traer `**Resumen.**` y `**Summary.**`
/// (si no, error diciendo cuál y en qué versión). `etiqueta` (`v1.6.1`,
/// `android-v0.2.0`) sirve para reescribir los enlaces relativos del detalle a
/// GitHub; `origen` es lo que sale en los errores.
pub fn entradas_de_changelog(
    texto: &str,
    origen: &str,
    etiqueta: &str,
) -> Resultado<Vec<EntradaChangelog>> {
    validar_etiqueta(etiqueta, origen)?;
    if texto.len() > TOPE_CHANGELOG {
        return Err(ErrorWeb::dato_raro(
            origen,
            format!(
                "el CHANGELOG pesa {} bytes y el tope es {TOPE_CHANGELOG}",
                texto.len()
            ),
        ));
    }

    let normalizado = normalizar_saltos(texto);
    let limpio = quitar_comentarios(&normalizado, origen)?;
    let secciones = trocear(&limpio, origen)?;
    if secciones.is_empty() {
        return Ok(Vec::new());
    }

    // Las definiciones de enlace, de todo el fichero: cuelgan de la última
    // sección pero valen para las demás.
    let global = Parser::new_ext(&limpio, opciones());
    let definiciones = global.reference_definitions();

    let mut entradas = Vec::with_capacity(secciones.len());
    for seccion in &secciones {
        entradas.push(construir_entrada(
            seccion,
            origen,
            etiqueta,
            Some(definiciones),
        )?);
    }
    Ok(entradas)
}

/// Markdown a HTML con `pulldown-cmark`, sin HTML en bruto (se escapa como texto),
/// sin imágenes (su texto alternativo, como texto) y con los enlaces limitados:
/// `https://` queda; los relativos pasan a `enlaces::destino_en_github(…, etiqueta)`
/// (rutas del repositorio, resueltas desde la raíz); `javascript:`, `data:`,
/// `http:` y todo lo demás se quedan como texto sin enlace. Las referencias de
/// enlace (`[1.6.1]: https://…`) del final se resuelven igual.
///
/// Un `https://` solo queda si tiene la forma que `comprobar` acepta (host en
/// minúsculas, sin usuario, sin puerto raro ni caracteres fuera de ASCII); si no,
/// es texto. Los encabezados bajan de nivel (`###` es `<h4>`, nunca más de `<h6>`)
/// y no llevan `id`. Un destino relativo se lee desde la carpeta del CHANGELOG: la raíz
/// del repositorio para `vX.Y.Z` y `android/` para `android-vX.Y.Z`.
pub fn detalle_a_html(markdown: &str, etiqueta: &str, origen: &str) -> Resultado<String> {
    validar_etiqueta(etiqueta, origen)?;
    markdown_a_html(markdown, etiqueta, origen, None)
}

/// `1.6.1`: tres números sin ceros a la izquierda ni sufijos, de hasta 4 cifras.
pub(crate) fn numero_valido(numero: &str) -> bool {
    let partes: Vec<&str> = numero.split('.').collect();
    partes.len() == 3
        && partes.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 4
                && p.bytes().all(|b| b.is_ascii_digit())
                && (*p == "0" || !p.starts_with('0'))
        })
}

/// `AAAA-MM-DD` con un día que existe (años bisiestos incluidos).
pub(crate) fn fecha_valida(fecha: &str) -> bool {
    let b = fecha.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let son_cifras = |tramo: &[u8]| tramo.iter().all(u8::is_ascii_digit);
    if !son_cifras(&b[0..4]) || !son_cifras(&b[5..7]) || !son_cifras(&b[8..10]) {
        return false;
    }
    let (Ok(anio), Ok(mes), Ok(dia)) = (
        fecha[0..4].parse::<u32>(),
        fecha[5..7].parse::<u32>(),
        fecha[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    if anio == 0 || !(1..=12).contains(&mes) {
        return false;
    }
    let bisiesto = (anio % 4 == 0 && anio % 100 != 0) || anio % 400 == 0;
    let dias = match mes {
        2 if bisiesto => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=dias).contains(&dia)
}

// ---------------------------------------------------------------------------
// Lectura del fichero
// ---------------------------------------------------------------------------

/// Una versión del fichero: su encabezado y las líneas de su cuerpo.
struct Seccion<'a> {
    numero: String,
    fecha: String,
    linea: usize,
    cuerpo: Vec<&'a str>,
}

enum Cabecera {
    Version { numero: String, fecha: String },
    Descartada,
    NoEs,
}

fn opciones() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
}

/// `vX.Y.Z` o `android-vX.Y.Z`, sin candidatas ni sufijos.
fn validar_etiqueta(etiqueta: &str, origen: &str) -> Resultado<()> {
    let numero = etiqueta
        .strip_prefix("android-")
        .unwrap_or(etiqueta)
        .strip_prefix('v');
    if numero.is_some_and(numero_valido) {
        Ok(())
    } else {
        Err(ErrorWeb::dato_raro(
            origen,
            format!(
                "la etiqueta {} no es `vX.Y.Z` ni `android-vX.Y.Z`",
                recorte(etiqueta)
            ),
        ))
    }
}

/// La carpeta del repositorio donde vive el CHANGELOG de una etiqueta, para leer
/// desde ella los enlaces relativos.
fn carpeta_del_changelog(etiqueta: &str) -> &'static str {
    if etiqueta.starts_with("android-") {
        "android"
    } else {
        ""
    }
}

/// Un trozo de texto para un mensaje: corto y con lo raro escapado.
fn recorte(texto: &str) -> String {
    let corto: String = texto.chars().take(60).collect();
    if corto.len() < texto.len() {
        format!("{corto:?}…")
    } else {
        format!("{corto:?}")
    }
}

/// `\r\n` y `\r` pasan a `\n`; un BOM inicial se quita.
fn normalizar_saltos(texto: &str) -> String {
    let sin_bom = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    if !sin_bom.contains('\r') {
        return sin_bom.to_string();
    }
    sin_bom.replace("\r\n", "\n").replace('\r', "\n")
}

/// Quita los `<!-- … -->`, también los de varias líneas, conservando sus saltos de
/// línea. Un comentario sin cerrar es un error.
fn quitar_comentarios(texto: &str, origen: &str) -> Resultado<String> {
    let mut salida = String::with_capacity(texto.len());
    let mut resto = texto;
    let mut consumido = 0usize;
    while let Some(inicio) = resto.find("<!--") {
        salida.push_str(&resto[..inicio]);
        let tras = &resto[inicio + 4..];
        let Some(fin) = tras.find("-->") else {
            let linea = 1 + texto[..consumido + inicio].matches('\n').count();
            return Err(ErrorWeb::dato_raro(
                format!("{origen}, línea {linea}"),
                "un comentario `<!--` sin cerrar",
            ));
        };
        for _ in tras[..fin].matches('\n') {
            salida.push('\n');
        }
        consumido += inicio + 4 + fin + 3;
        resto = &tras[fin + 3..];
    }
    salida.push_str(resto);
    Ok(salida)
}

/// Qué es una línea del fichero. `Err` lleva lo que le pasa a un encabezado que
/// parece de versión y no lo es.
fn leer_cabecera(linea: &str) -> Result<Cabecera, String> {
    let Some(resto) = linea.strip_prefix("## ") else {
        return Ok(Cabecera::NoEs);
    };
    let resto = resto.trim_end();
    let Some(tras) = resto.strip_prefix('[') else {
        return Ok(Cabecera::NoEs);
    };
    let empieza_por_cifra = tras.bytes().next().is_some_and(|b| b.is_ascii_digit());
    let Some(cierre) = tras.find(']') else {
        return if empieza_por_cifra {
            Err("falta el `]` que cierra el número".to_string())
        } else {
            Ok(Cabecera::NoEs)
        };
    };
    let dentro = &tras[..cierre];
    if !empieza_por_cifra {
        // «Sin publicar» y compañía: no es una versión.
        return Ok(Cabecera::Descartada);
    }
    if !numero_valido(dentro) {
        return Err(format!(
            "el número {} no es `X.Y.Z` (sin ceros a la izquierda ni sufijos, de hasta 4 cifras)",
            recorte(dentro)
        ));
    }
    let Some(fecha) = tras[cierre + 1..].strip_prefix(" - ") else {
        return Err("tras el número tiene que ir ` - AAAA-MM-DD`".to_string());
    };
    if !fecha_valida(fecha) {
        return Err(format!(
            "la fecha {} no es una fecha `AAAA-MM-DD` que exista",
            recorte(fecha)
        ));
    }
    Ok(Cabecera::Version {
        numero: dentro.to_string(),
        fecha: fecha.to_string(),
    })
}

/// Una línea de vallado de código (```` ``` ```` o `~~~`): con qué carácter, cuántos
/// y si lo que sigue a la valla está en blanco. Solo cuenta con hasta 3 espacios de
/// sangría, como en CommonMark.
fn valla(linea: &str) -> Option<(char, usize, bool)> {
    let sin_sangria = linea.trim_start_matches(' ');
    if linea.len() - sin_sangria.len() > 3 {
        return None;
    }
    let c = sin_sangria
        .chars()
        .next()
        .filter(|c| matches!(c, '`' | '~'))?;
    let n = sin_sangria.chars().take_while(|x| *x == c).count();
    if n < 3 {
        return None;
    }
    let resto = &sin_sangria[n..];
    // Tras una valla de acentos graves no puede haber otro (sería código en línea).
    if c == '`' && resto.contains('`') {
        return None;
    }
    Some((c, n, resto.trim().is_empty()))
}

/// Reparte el fichero en versiones. Lo que va antes de la primera y lo de las
/// secciones descartadas se pierde.
///
/// Un bloque de código abierto en la columna 0 esconde lo que lleva dentro, también
/// un `## [X.Y.Z] - …` (así lo ve GitHub); uno sin cerrar al llegar al final del
/// fichero es un error, no un bloque que se traga las versiones siguientes. Una
/// valla con sangría (dentro de una lista) no se sigue: un encabezado en la columna
/// 0 la cierra.
fn trocear<'a>(texto: &'a str, origen: &str) -> Resultado<Vec<Seccion<'a>>> {
    let mut secciones: Vec<Seccion<'a>> = Vec::new();
    let mut vistos: HashSet<String> = HashSet::new();
    let mut actual: Option<Seccion<'a>> = None;
    let mut tamano = 0usize;
    // Bloque de código abierto: carácter, longitud y línea donde se abrió.
    let mut bloque: Option<(char, usize, usize)> = None;

    for (indice, linea) in texto.split('\n').enumerate() {
        let n = indice + 1;

        let cabecera = match bloque {
            Some((c, largo, _)) => {
                if valla(linea).is_some_and(|(d, m, en_blanco)| d == c && m >= largo && en_blanco) {
                    bloque = None;
                }
                Cabecera::NoEs
            }
            None => {
                if let Some((c, largo, _)) = valla(linea).filter(|_| !linea.starts_with(' ')) {
                    bloque = Some((c, largo, n));
                    Cabecera::NoEs
                } else {
                    leer_cabecera(linea)
                        .map_err(|que| ErrorWeb::dato_raro(format!("{origen}, línea {n}"), que))?
                }
            }
        };

        match cabecera {
            Cabecera::NoEs => {
                if let Some(seccion) = actual.as_mut() {
                    tamano += linea.len() + 1;
                    if tamano > TOPE_ENTRADA {
                        return Err(ErrorWeb::dato_raro(
                            format!("{origen}, versión {}", seccion.numero),
                            format!("la entrada pasa del tope de {TOPE_ENTRADA} bytes"),
                        ));
                    }
                    seccion.cuerpo.push(linea);
                }
            }
            Cabecera::Descartada => {
                secciones.extend(actual.take());
                tamano = 0;
            }
            Cabecera::Version { numero, fecha } => {
                secciones.extend(actual.take());
                if !vistos.insert(numero.clone()) {
                    return Err(ErrorWeb::dato_raro(
                        format!("{origen}, línea {n}"),
                        format!("la versión {numero} aparece más de una vez"),
                    ));
                }
                if vistos.len() > MAX_VERSIONES {
                    return Err(ErrorWeb::dato_raro(
                        origen,
                        format!("más de {MAX_VERSIONES} versiones"),
                    ));
                }
                tamano = 0;
                actual = Some(Seccion {
                    numero,
                    fecha,
                    linea: n,
                    cuerpo: Vec::new(),
                });
            }
        }
    }
    if let Some((_, _, desde)) = bloque {
        return Err(ErrorWeb::dato_raro(
            format!("{origen}, línea {desde}"),
            "un bloque de código sin cerrar: se tragaría las versiones siguientes",
        ));
    }
    secciones.extend(actual);
    Ok(secciones)
}

/// Dónde está el párrafo que empieza por `marca`: de su línea a la que sigue a la
/// última. Acaba en la primera línea en blanco o en la del otro marcador.
fn buscar_parrafo(lineas: &[&str], marca: &str) -> Option<(usize, usize)> {
    let inicio = lineas.iter().position(|l| {
        l.strip_prefix(marca)
            .is_some_and(|resto| resto.is_empty() || resto.starts_with(char::is_whitespace))
    })?;
    let mut fin = inicio + 1;
    while fin < lineas.len()
        && !lineas[fin].trim().is_empty()
        && !lineas[fin].starts_with(MARCA_RESUMEN)
        && !lineas[fin].starts_with(MARCA_SUMMARY)
    {
        fin += 1;
    }
    Some((inicio, fin))
}

/// El texto de un párrafo con su marcador fuera, como texto plano.
fn texto_del_parrafo(lineas: &[&str], marca: &str, rango: (usize, usize)) -> Option<String> {
    let (inicio, fin) = rango;
    let mut crudo = String::new();
    crudo.push_str(&lineas[inicio][marca.len()..]);
    for linea in &lineas[inicio + 1..fin] {
        crudo.push('\n');
        crudo.push_str(linea);
    }
    if crudo.len() > TOPE_RESUMEN {
        return None;
    }
    Some(a_texto_plano(&crudo))
}

fn construir_entrada(
    seccion: &Seccion<'_>,
    origen: &str,
    etiqueta: &str,
    definiciones: Option<&RefDefs<'_>>,
) -> Resultado<EntradaChangelog> {
    let donde = format!(
        "{origen}, versión {} (línea {})",
        seccion.numero, seccion.linea
    );
    let lineas = &seccion.cuerpo;

    let rango_es = buscar_parrafo(lineas, MARCA_RESUMEN);
    let rango_en = buscar_parrafo(lineas, MARCA_SUMMARY);
    let mut faltan = Vec::new();
    let mut leer = |marca: &str, nombre: &str, rango: Option<(usize, usize)>| -> Option<String> {
        let Some(rango) = rango else {
            faltan.push(format!("falta `{marca}`"));
            return None;
        };
        match texto_del_parrafo(lineas, marca, rango) {
            Some(texto) if !texto.is_empty() => Some(texto),
            Some(_) => {
                faltan.push(format!("el {nombre} está vacío"));
                None
            }
            None => {
                faltan.push(format!("el {nombre} pasa de {TOPE_RESUMEN} bytes"));
                None
            }
        }
    };
    let resumen_es = leer(MARCA_RESUMEN, "Resumen", rango_es);
    let resumen_en = leer(MARCA_SUMMARY, "Summary", rango_en);
    let (Some(resumen_es), Some(resumen_en), Some(rango_es), Some(rango_en)) =
        (resumen_es, resumen_en, rango_es, rango_en)
    else {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!(
                "la versión {} no tiene Resumen y Summary: {}",
                seccion.numero,
                faltan.join("; ")
            ),
        ));
    };

    // El detalle es el resto del cuerpo, sin esos dos párrafos.
    let mut detalle = String::new();
    for (i, linea) in lineas.iter().enumerate() {
        let en_parrafo = |r: (usize, usize)| (r.0..r.1).contains(&i);
        if en_parrafo(rango_es) || en_parrafo(rango_en) {
            continue;
        }
        detalle.push_str(linea);
        detalle.push('\n');
    }
    let detalle = detalle.trim_matches(|c: char| c == '\n' || c == ' ');
    let detalle_html = if detalle.trim().is_empty() {
        String::new()
    } else {
        markdown_a_html(detalle, etiqueta, &donde, definiciones)?
    };

    Ok(EntradaChangelog {
        numero: seccion.numero.clone(),
        fecha: seccion.fecha.clone(),
        resumen_es,
        resumen_en,
        detalle_html,
    })
}

/// Un párrafo de Markdown como texto plano: sin marcas, con los espacios
/// colapsados y el HTML en bruto como texto.
fn a_texto_plano(markdown: &str) -> String {
    let mut texto = String::with_capacity(markdown.len());
    for evento in Parser::new(markdown) {
        match evento {
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => {
                texto.push_str(&t);
            }
            Event::SoftBreak | Event::HardBreak => texto.push(' '),
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_)
                | TagEnd::HtmlBlock,
            ) => texto.push(' '),
            _ => {}
        }
    }
    texto
        .split_whitespace()
        .map(|palabra| {
            palabra
                .chars()
                .filter(|c| !es_invisible(*c))
                .collect::<String>()
        })
        .filter(|palabra| !palabra.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Markdown a HTML
// ---------------------------------------------------------------------------

fn markdown_a_html(
    markdown: &str,
    etiqueta: &str,
    origen: &str,
    definiciones: Option<&RefDefs<'_>>,
) -> Resultado<String> {
    if markdown.len() > TOPE_ENTRADA {
        return Err(ErrorWeb::dato_raro(
            origen,
            format!(
                "el detalle pesa {} bytes y el tope es {TOPE_ENTRADA}",
                markdown.len()
            ),
        ));
    }
    // Una referencia que esta entrada no define se busca en las del fichero.
    let resolver = |enlace: BrokenLink<'_>| -> Option<(CowStr<'_>, CowStr<'_>)> {
        let definicion = definiciones?.get(&enlace.reference)?;
        Some((
            CowStr::from(definicion.dest.to_string()),
            CowStr::from(String::new()),
        ))
    };
    let analizador = Parser::new_with_broken_link_callback(markdown, opciones(), Some(resolver));

    let mut escritor = Escritor {
        salida: String::with_capacity(markdown.len() + markdown.len() / 2),
        pila: Vec::new(),
        abiertos: 0,
        en_cabecera_tabla: false,
        enlace: None,
        etiqueta,
        carpeta: carpeta_del_changelog(etiqueta),
    };
    for evento in analizador {
        escritor.evento(evento);
        if escritor.salida.len() > TOPE_SALIDA {
            return Err(ErrorWeb::dato_raro(
                origen,
                format!("el detalle genera más de {TOPE_SALIDA} bytes de HTML"),
            ));
        }
    }
    Ok(escritor.salida)
}

/// Un elemento que se abrió (o se omitió) y lo que lleva su cierre.
struct Abierto {
    cierre: &'static str,
    /// Cuenta para `NIVELES_MAXIMOS`.
    cuenta: bool,
    /// Es una lista que no se emitió: sus elementos tampoco.
    lista_omitida: bool,
    /// Es un enlace que sí se escribió (`<a>`).
    es_enlace: bool,
}

/// Un enlace abierto y si ya ha salido algún texto dentro: un enlace sin texto no
/// tiene nombre para un lector de pantalla (WCAG 2.4.4), así que al cerrarse
/// vacío se escribe su destino.
struct EnlaceAbierto {
    destino: String,
    con_texto: bool,
}

struct Escritor<'a> {
    salida: String,
    pila: Vec<Abierto>,
    abiertos: usize,
    en_cabecera_tabla: bool,
    enlace: Option<EnlaceAbierto>,
    etiqueta: &'a str,
    carpeta: &'a str,
}

impl Escritor<'_> {
    fn abrir(&mut self, abre: &str, cierra: &'static str, cuenta: bool) {
        self.salida.push_str(abre);
        if cuenta {
            self.abiertos += 1;
        }
        self.pila.push(Abierto {
            cierre: cierra,
            cuenta,
            lista_omitida: false,
            es_enlace: false,
        });
    }

    fn omitir(&mut self, lista_omitida: bool) {
        self.pila.push(Abierto {
            cierre: "",
            cuenta: false,
            lista_omitida,
            es_enlace: false,
        });
    }

    /// Anota que ha salido texto que se ve, para el enlace abierto.
    fn texto_visto(&mut self, texto: &str) {
        if let Some(enlace) = self.enlace.as_mut() {
            if !enlace.con_texto
                && texto
                    .chars()
                    .any(|c| !c.is_whitespace() && !es_invisible(c))
            {
                enlace.con_texto = true;
            }
        }
    }

    fn cerrar(&mut self) {
        if let Some(abierto) = self.pila.pop() {
            self.salida.push_str(abierto.cierre);
            if abierto.cuenta {
                self.abiertos = self.abiertos.saturating_sub(1);
            }
        }
    }

    /// Abre un elemento que cuenta para el tope de profundidad, o lo omite.
    fn abrir_con_tope(&mut self, abre: &str, cierra: &'static str) {
        if self.abiertos < NIVELES_MAXIMOS {
            self.abrir(abre, cierra, true);
        } else {
            self.omitir(false);
        }
    }

    fn evento(&mut self, evento: Event<'_>) {
        match evento {
            Event::Start(tag) => self.empezar(tag),
            Event::End(fin) => self.terminar(fin),
            Event::Text(t) => {
                self.texto_visto(&t);
                escapar(&mut self.salida, &t);
            }
            Event::Code(t) => {
                self.texto_visto(&t);
                self.salida.push_str("<code>");
                escapar(&mut self.salida, &t);
                self.salida.push_str("</code>");
            }
            // HTML en bruto: texto, nunca marcado.
            Event::Html(t) | Event::InlineHtml(t) => {
                self.texto_visto(&t);
                escapar(&mut self.salida, &t);
            }
            Event::SoftBreak => self.salida.push('\n'),
            Event::HardBreak => self.salida.push_str("<br>\n"),
            Event::Rule => self.salida.push_str("<hr>\n"),
            // Notas, tareas y fórmulas: no están activadas.
            _ => {}
        }
    }

    fn empezar(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.abrir("<p>", "</p>\n", false),
            Tag::Heading { level, .. } => match nivel_de_encabezado(level) {
                4 => self.abrir("<h4>", "</h4>\n", false),
                5 => self.abrir("<h5>", "</h5>\n", false),
                _ => self.abrir("<h6>", "</h6>\n", false),
            },
            Tag::BlockQuote(_) => self.abrir_con_tope("<blockquote>\n", "</blockquote>\n"),
            Tag::CodeBlock(_) => self.abrir("<pre><code>", "</code></pre>\n", false),
            Tag::HtmlBlock => self.abrir("<p>", "</p>\n", false),
            Tag::List(inicio) => {
                if self.abiertos >= NIVELES_MAXIMOS {
                    self.omitir(true);
                } else if let Some(n) = inicio.filter(|n| *n != 1) {
                    self.abrir(&format!("<ol start=\"{n}\">\n"), "</ol>\n", true);
                } else if inicio.is_some() {
                    self.abrir("<ol>\n", "</ol>\n", true);
                } else {
                    self.abrir("<ul>\n", "</ul>\n", true);
                }
            }
            Tag::Item => {
                if self.pila.last().is_some_and(|a| a.lista_omitida) {
                    self.omitir(false);
                } else {
                    self.abrir("<li>", "</li>\n", false);
                }
            }
            Tag::Emphasis => self.abrir_con_tope("<em>", "</em>"),
            Tag::Strong => self.abrir_con_tope("<strong>", "</strong>"),
            Tag::Strikethrough => self.abrir_con_tope("<del>", "</del>"),
            // Un correo (`<a@b>`) llega sin esquema: nunca es una ruta del repositorio.
            Tag::Link {
                link_type: LinkType::Email,
                ..
            } => self.omitir(false),
            Tag::Link { dest_url, .. } => {
                match destino_seguro(&dest_url, self.etiqueta, self.carpeta) {
                    Some(url) => {
                        let mut abre = String::with_capacity(url.len() + 16);
                        abre.push_str("<a href=\"");
                        escapar(&mut abre, &url);
                        abre.push_str("\">");
                        self.abrir(&abre, "</a>", false);
                        if let Some(abierto) = self.pila.last_mut() {
                            abierto.es_enlace = true;
                        }
                        self.enlace = Some(EnlaceAbierto {
                            destino: url,
                            con_texto: false,
                        });
                    }
                    // Sin enlace: queda el texto.
                    None => self.omitir(false),
                }
            }
            // Una imagen es su texto alternativo, que llega como texto.
            Tag::Image { .. } => self.omitir(false),
            Tag::Table(_) => self.abrir("<table>\n", "</tbody>\n</table>\n", false),
            Tag::TableHead => {
                self.en_cabecera_tabla = true;
                self.abrir("<thead>\n<tr>", "</tr>\n</thead>\n<tbody>\n", false);
            }
            Tag::TableRow => self.abrir("<tr>", "</tr>\n", false),
            Tag::TableCell => {
                if self.en_cabecera_tabla {
                    self.abrir("<th>", "</th>", false);
                } else {
                    self.abrir("<td>", "</td>", false);
                }
            }
            // Lo que no está activado no llega; si llegara, solo su contenido.
            _ => self.omitir(false),
        }
    }

    fn terminar(&mut self, fin: TagEnd) {
        if matches!(fin, TagEnd::TableHead) {
            self.en_cabecera_tabla = false;
        }
        if self.pila.last().is_some_and(|abierto| abierto.es_enlace) {
            if let Some(enlace) = self.enlace.take() {
                if !enlace.con_texto {
                    escapar(&mut self.salida, &enlace.destino);
                }
            }
        }
        self.cerrar();
    }
}

/// Los encabezados del detalle cuelgan de un `<h3>` (la versión): `###` es `<h4>`
/// y nunca pasan de `<h6>`.
fn nivel_de_encabezado(nivel: HeadingLevel) -> u8 {
    match nivel {
        HeadingLevel::H1 | HeadingLevel::H2 | HeadingLevel::H3 => 4,
        HeadingLevel::H4 => 5,
        HeadingLevel::H5 | HeadingLevel::H6 => 6,
    }
}

/// Un carácter que no es texto: de control (salvo el salto de línea y el
/// tabulador), o de los que cambian el orden de lectura o no se ven (el mismo
/// conjunto que en las capturas y en los mensajes: `ordenes::es_invisible_que_engana`).
fn es_invisible(c: char) -> bool {
    (c.is_control() && !matches!(c, '\n' | '\t')) || es_invisible_que_engana(c)
}

/// Escapa `& < > " '` (vale también dentro de un atributo entrecomillado) y quita
/// lo que no es texto (`es_invisible`).
fn escapar(salida: &mut String, texto: &str) {
    for c in texto.chars() {
        match c {
            '&' => salida.push_str("&amp;"),
            '<' => salida.push_str("&lt;"),
            '>' => salida.push_str("&gt;"),
            '"' => salida.push_str("&quot;"),
            '\'' => salida.push_str("&#x27;"),
            c if es_invisible(c) => {}
            c => salida.push(c),
        }
    }
}

/// A dónde puede apuntar un enlace del detalle, o `None` si no puede (queda el
/// texto): `https://` bien formado, o una ruta relativa del repositorio, que pasa a
/// GitHub en la etiqueta de la versión. Todo lo demás (`#ancla`, `http:`,
/// `javascript:`, `data:`, `mailto:`, `//host`, esquemas con mayúsculas, blancos,
/// controles o `\`) no es un enlace.
fn destino_seguro(destino: &str, etiqueta: &str, carpeta: &str) -> Option<String> {
    if destino.is_empty() || destino.len() > MAX_DESTINO {
        return None;
    }
    if destino
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\\' || es_invisible(c))
    {
        return None;
    }

    if destino.starts_with("https://") {
        return analizar_https(destino).map(|_| destino.to_string());
    }
    if destino.starts_with('#') || destino.starts_with('/') {
        return None;
    }
    // Un `:` antes del primer `/`, `?` o `#` es un esquema (o algo que un
    // navegador podría tomar por uno).
    let fin_primero = destino.find(['/', '?', '#']).unwrap_or(destino.len());
    if destino[..fin_primero].contains(':') {
        return None;
    }

    // Una ruta relativa: del repositorio, resuelta desde la carpeta del CHANGELOG.
    let (sin_ancla, ancla) = match destino.split_once('#') {
        Some((ruta, ancla)) => (ruta, Some(ancla)),
        None => (destino, None),
    };
    let ruta = sin_ancla.split('?').next().unwrap_or("");
    if ruta.is_empty() {
        return None;
    }
    let resuelta = enlaces::resolver(carpeta, ruta)?;
    let mut en_repositorio = resuelta.clone();
    if let Some(ancla) = ancla.filter(|a| !a.is_empty()) {
        en_repositorio.push('#');
        en_repositorio.push_str(ancla);
    }
    let url = enlaces::destino_en_github(&en_repositorio, etiqueta);
    // Una ruta que `destino_en_github` no admite sale como la raíz de la etiqueta:
    // eso no es lo que decía el enlace.
    if !resuelta.is_empty() && url == enlaces::destino_en_github("", etiqueta) {
        return None;
    }
    analizar_https(&url)?;
    Some(url)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_numeros_validos_y_los_que_no() {
        for bien in ["0.0.0", "1.6.1", "10.20.30", "0.2.0", "9999.9999.9999"] {
            assert!(numero_valido(bien), "{bien}");
        }
        // Cuatro números, escrito así para no parecer una dirección IP.
        let cuatro = ["1", "6", "1", "2"].join(".");
        for mal in [
            "",
            "1",
            "1.6",
            cuatro.as_str(),
            "01.6.1",
            "1.06.1",
            "1.6.01",
            "1.6.1-rc.1",
            "1.6.x",
            "1..1",
            ".1.1",
            "v1.6.1",
            "1.6.1 ",
            "10000.0.0",
            "1.6.+1",
            "１.6.1",
        ] {
            assert!(!numero_valido(mal), "{mal:?}");
        }
    }

    #[test]
    fn las_fechas_validas_y_las_que_no() {
        for bien in [
            "2026-11-22",
            "2024-02-29",
            "2000-02-29",
            "2026-12-31",
            "0001-01-01",
        ] {
            assert!(fecha_valida(bien), "{bien}");
        }
        for mal in [
            "",
            "2026-13-01",
            "2026-00-10",
            "2026-11-32",
            "2026-11-00",
            "2026-02-29",
            "2100-02-29",
            "2026-04-31",
            "0000-01-01",
            "2026/11/22",
            "2026-11-2",
            "2026-11-22 ",
            "2026-11-22T00:00:00Z",
            "２０２６-11-22",
        ] {
            assert!(!fecha_valida(mal), "{mal:?}");
        }
    }

    #[test]
    fn escapar_cambia_lo_que_importa_y_quita_lo_invisible() {
        let mut s = String::new();
        escapar(
            &mut s,
            "<a href=\"x\">'&'</a>\u{202e}\u{0}\u{7}ok\u{feff}\n\t",
        );
        assert_eq!(
            s,
            "&lt;a href=&quot;x&quot;&gt;&#x27;&amp;&#x27;&lt;/a&gt;ok\n\t"
        );
    }

    #[test]
    fn la_etiqueta_se_valida() {
        for bien in ["v1.6.1", "android-v0.2.0", "v0.0.0"] {
            assert!(validar_etiqueta(bien, "x").is_ok(), "{bien}");
        }
        for mal in [
            "",
            "main",
            "v1.6",
            "v1.6.1-rc.1",
            "android-1.6.1",
            "V1.6.1",
            "v01.6.1",
            "vv1.6.1",
        ] {
            assert!(validar_etiqueta(mal, "x").is_err(), "{mal:?}");
        }
    }

    #[test]
    fn los_destinos_seguros() {
        let d = |destino: &str| destino_seguro(destino, "v1.6.1", "");
        assert_eq!(
            d("https://github.com/HectorEspejo/magi/releases/tag/v1.6.1").as_deref(),
            Some("https://github.com/HectorEspejo/magi/releases/tag/v1.6.1")
        );
        assert_eq!(
            d("docs/formato-sincronizacion.md").as_deref(),
            Some("https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md")
        );
        assert_eq!(
            d("docs/manual/").as_deref(),
            Some("https://github.com/HectorEspejo/magi/tree/v1.6.1/docs/manual/")
        );
        assert_eq!(
            d("docs/formato-sincronizacion.md#seccion-2").as_deref(),
            Some(
                "https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md#seccion-2"
            )
        );
        for malo in [
            "",
            "#ancla",
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "data:text/html,x",
            "vbscript:x",
            "http://ejemplo.org/",
            "mailto:usuario@ejemplo.org",
            "//ejemplo.org/x",
            "/etc/passwd",
            "HTTPS://ejemplo.org/",
            "https://usuario@ejemplo.org/",
            "https://ejemplo.org:8443/",
            "https://ejemplo.org/a b",
            "../../etc/passwd",
            "..",
            "?x=1",
            "docs/a b.md",
            "docs\\x.md",
            "docs/a%2Fb.md",
            "java\tscript:x",
        ] {
            assert_eq!(d(malo), None, "{malo:?}");
        }
    }

    #[test]
    fn los_relativos_de_la_app_se_leen_desde_su_carpeta() {
        let d = |destino: &str| destino_seguro(destino, "android-v0.2.0", "android");
        assert_eq!(
            d("../docs/publicar-android.md").as_deref(),
            Some(
                "https://github.com/HectorEspejo/magi/blob/android-v0.2.0/docs/publicar-android.md"
            )
        );
        assert_eq!(
            d("CHANGELOG.md").as_deref(),
            Some("https://github.com/HectorEspejo/magi/blob/android-v0.2.0/android/CHANGELOG.md")
        );
        assert_eq!(d("../../fuera.md"), None);
    }
}
