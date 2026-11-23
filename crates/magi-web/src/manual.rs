//! Preparar el manual para mdBook (informe §4.5, §7.5 y §7.6).
//!
//! `magi-web preparar-manual` toma los capítulos de `docs/manual/` de la etiqueta
//! de la última versión publicada del escritorio (o de un árbol de trabajo) y deja
//! una carpeta con todo lo que `mdbook build` necesita: `book.toml` y `theme/` del
//! tema propio (`web/manual/`) y `src/` con los capítulos ya adaptados:
//!
//! 1. `SUMMARY.md` sale del índice `## Capítulos` de `README.md`, más `estado.md`
//!    al final. Un capítulo que no está en el índice es un error («capítulo fuera
//!    del índice: x.md»): la web no se construye.
//! 2. Cada título recibe su `{#id}` con el algoritmo de GitHub (`anclas`), el mismo
//!    que usa `repositorio_publico`: los enlaces del manual funcionan igual en
//!    GitHub y en la web.
//! 3. Los enlaces que salen de `docs/manual/` pasan a GitHub en la etiqueta
//!    (`blob` para ficheros, `tree` para carpetas) y los de los README van a la
//!    portada (`../`) y a `/en/` (`../en/`). Los enlaces entre capítulos se dejan
//!    (mdBook los pasa a `.html`), salvo el del índice del manual: mdBook publica
//!    `README.md` como `index.html` pero deja su enlace en `README.html`, que no
//!    existe, así que el enlace pasa a `index.md`.
//! 4. Todo HTML en bruto (`<host>`, `<uuid>`…) se escapa como texto: mdBook lo
//!    dejaría pasar y se tragaría el resto de la línea. Los comentarios HTML se
//!    quitan.
//! 5. `{{#` (los preprocesadores de mdBook: `{{#include}}`, `{{#playground}}`…) se
//!    rechaza en cualquier sitio, también dentro de código, porque mdBook los
//!    procesa en todo el fichero.
//! 6. Una tabla con alineación (`|:--|:-:|--:|`) se rechaza: mdBook 0.5.4 la
//!    escribe con `style="text-align: …"` y la CSP de la web no admite atributos
//!    `style`. Falla aquí, con su fichero y su línea, y no al construir el sitio.
//! 7. Un título (del capítulo o del índice) con «<» se rechaza: el buscador de mdBook lo
//!    pone en la página como HTML. Un capítulo no puede llamarse como una página de
//!    mdBook (`index`, `toc`…) ni usar saltos de línea solo CR. Y no se aceptan SVG
//!    (se ejecutarían con el origen de la web): solo imágenes de mapa de bits.
//!
//! Lo que se lee del repositorio es entrada no fiable: nombres de fichero, tipos de
//! entrada de git (solo ficheros normales `100644` y `100755`: un enlace simbólico
//! `120000` o un submódulo `160000` se rechazan), tamaños y contenido. Todo error
//! dice qué y dónde (`fichero:línea`) y la salida anterior no se toca si algo falla.
//!
//! # Cómo se localiza lo que se reescribe
//!
//! Los títulos, los enlaces y el HTML se buscan con `pulldown-cmark` (el mismo
//! analizador que usa mdBook, con sus mismas extensiones) y se cambian por posición
//! en el texto original: lo que está dentro de código nunca se toca (los únicos
//! pasos que leen texto sin analizarlo son el índice de `README.md` y la búsqueda de
//! `{{#`, que valen para todo el fichero a propósito). Cada paso vuelve a analizar
//! el resultado y falla si no es lo que prometía (un título sin su id, HTML que
//! sobrevive…): nunca se desalinea en silencio.
//!
//! # Orden de los pasos
//!
//! `rechazar_preprocesadores`, `con_anclas`, `reescribir_enlaces` y, al final,
//! `escapar_html_suelto`. Las anclas se calculan sobre el texto original (así son
//! exactamente las de `repositorio_publico`) y el único paso que cambia el número
//! de líneas (quitar comentarios) va el último: los errores de todos los demás
//! citan la línea del fichero tal como está en el repositorio.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::Read;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use pulldown_cmark::{Alignment, Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::anclas::{encabezados_de, slugs_de};
use crate::enlaces::resolver;
use crate::error::{ErrorWeb, Resultado};
use crate::ordenes::{ejecutar_sin_variables, texto_seguro};

/// Repositorio de MAGI en GitHub (la misma constante, privada, que usa `enlaces`).
const REPOSITORIO: &str = "https://github.com/HectorEspejo/magi";
/// Dónde está el manual dentro del repositorio.
const CARPETA_MANUAL: &str = "docs/manual";
/// Tope de un fichero del manual (2 MiB).
const TOPE_FICHERO: u64 = 2 * 1024 * 1024;
/// Ficheros como mucho en `docs/manual/`.
const TOPE_FICHEROS: usize = 64;
/// Tope de todo lo que se lee del manual (32 MiB).
const TOPE_TOTAL: u64 = 32 * 1024 * 1024;
/// Plazo de cada orden de git.
const PLAZO_GIT: Duration = Duration::from_secs(30);
/// Tope del listado de `git ls-tree` (1 MiB: 64 entradas caben de sobra).
const TOPE_LISTADO: usize = 1024 * 1024;
/// Tope de `book.toml` del tema (1 MiB).
const TOPE_LIBRO_TOML: u64 = 1024 * 1024;
/// Límites al copiar `theme/`.
const TOPE_FICHERO_TEMA: u64 = 4 * 1024 * 1024;
const TOPE_FICHEROS_TEMA: usize = 256;
const TOPE_TOTAL_TEMA: u64 = 32 * 1024 * 1024;
const PROFUNDIDAD_TEMA: usize = 6;
/// Largo máximo de un destino de enlace.
const TOPE_DESTINO: usize = 2048;
/// Extensiones de imagen que se copian junto a los capítulos. Sin `svg`: un SVG suelto
/// se ejecuta con el origen de la web (la CSP del `<meta>` solo vale para HTML) y una
/// lista negra de texto no lo cierra (`<s:script>`, entidades de DTD…). El manual no
/// trae imágenes; si alguna vez las necesita, serán de mapa de bits.
const IMAGENES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// De dónde salen los capítulos.
#[derive(Debug, Clone)]
pub enum OrigenManual {
    /// `docs/manual/` de la etiqueta `OpcionesManual::etiqueta` del repositorio git
    /// de `repositorio`, con `git rev-parse`, `git ls-tree` y `git cat-file` (con
    /// plazo y tope de salida): solo ficheros normales con nombres admitidos, nunca
    /// el árbol de trabajo.
    Etiqueta { repositorio: PathBuf },
    /// La carpeta `docs/manual/` de un árbol de trabajo: la rama de una PR y
    /// `generar-web.sh --sin-red`. Los enlaces se reescriben igual con `etiqueta`.
    Arbol { carpeta: PathBuf },
}

#[derive(Debug, Clone)]
pub struct OpcionesManual {
    pub origen: OrigenManual,
    /// `v1.6.1`: la etiqueta a la que apuntan los enlaces de fuera (y de la que se
    /// lee el manual con `OrigenManual::Etiqueta`). Solo `vX.Y.Z`.
    pub etiqueta: String,
    /// Carpeta del tema propio (`web/manual/`): su `book.toml` y su `theme/` se
    /// copian a la salida.
    pub tema: PathBuf,
    /// Carpeta de salida: raíz del libro (`book.toml`, `theme/`, `src/`). Se
    /// sustituye entera y solo si todo salió bien (como `construir`). Si ya existe,
    /// tiene que estar vacía o ser un libro preparado antes (`book.toml` y
    /// `src/SUMMARY.md`), y no puede solaparse con el tema, el manual ni el
    /// repositorio.
    pub salida: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct InformeManual {
    /// Ficheros de los capítulos, en el orden del índice (sin `README.md` ni
    /// `estado.md`).
    pub capitulos: Vec<String>,
    /// Todo lo escrito en `src/` (con `README.md`, `estado.md` y `SUMMARY.md`),
    /// ordenado por nombre.
    pub ficheros: Vec<String>,
}

// ---------------------------------------------------------------------------
// Utilidades de texto y de errores
// ---------------------------------------------------------------------------

/// Línea (desde 1) de la posición `pos` de `texto`.
fn linea_de(texto: &str, pos: usize) -> usize {
    let limite = pos.min(texto.len());
    texto.as_bytes()[..limite]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

/// Un error en `origen:línea`, con la línea de la posición `pos` de `texto`.
fn error_en(origen: &str, texto: &str, pos: usize, que: impl Into<String>) -> ErrorWeb {
    ErrorWeb::dato_raro(format!("{origen}:{}", linea_de(texto, pos)), que)
}

/// `texto[rango]`, o un error si el analizador devolvió una posición imposible.
fn trozo<'a>(texto: &'a str, rango: &Range<usize>, origen: &str) -> Resultado<&'a str> {
    texto.get(rango.clone()).ok_or_else(|| {
        ErrorWeb::dato_raro(
            origen,
            "posición fuera del texto (error interno del analizador de Markdown)",
        )
    })
}

/// Las extensiones de Markdown que usa mdBook (0.5): lo que el analizador ve aquí
/// es lo que verá él. Sin matemáticas ni superíndices, que no usa (y que cambiarían
/// qué es HTML).
fn opciones() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST
}

/// Aplica ediciones `(rango, texto nuevo)` por posición. Falla si se solapan o si
/// alguna cae fuera del texto o en medio de un carácter.
fn aplicar(
    texto: &str,
    mut ediciones: Vec<(Range<usize>, String)>,
    origen: &str,
) -> Resultado<String> {
    ediciones.sort_by_key(|(r, _)| (r.start, r.end));
    let mut salida = String::with_capacity(texto.len() + 64);
    let mut cursor = 0usize;
    for (rango, nuevo) in ediciones {
        if rango.start < cursor || rango.end < rango.start {
            return Err(error_en(
                origen,
                texto,
                rango.start,
                "dos cambios se solapan (error interno)",
            ));
        }
        salida.push_str(trozo(texto, &(cursor..rango.start), origen)?);
        salida.push_str(&nuevo);
        // Comprueba que el rango entero cae en fronteras de carácter.
        trozo(texto, &rango, origen)?;
        cursor = rango.end;
    }
    salida.push_str(trozo(texto, &(cursor..texto.len()), origen)?);
    Ok(salida)
}

// ---------------------------------------------------------------------------
// SUMMARY.md
// ---------------------------------------------------------------------------

/// `README.md` y `estado.md`: las dos páginas del manual que no son capítulos del
/// índice.
fn es_pagina_fija(nombre: &str) -> bool {
    nombre == "README.md" || nombre == "estado.md"
}

/// Nombres que mdBook usa para sus propias páginas: un capítulo con uno de ellos
/// (`index.md` o `readme.md` pisarían la portada del manual, `toc.md` el relleno de la
/// barra lateral, `print.md` lo rechaza el propio mdBook) cambiaría lo que se publica.
const NOMBRES_RESERVADOS: &[&str] = &["index", "readme", "toc", "print", "404", "searchindex"];

/// `^[a-z0-9-]+\.md$`, sin los nombres reservados de mdBook.
fn nombre_de_capitulo_valido(nombre: &str) -> bool {
    match nombre.strip_suffix(".md") {
        Some(raiz) => {
            !raiz.is_empty()
                && raiz.len() <= 80
                && !NOMBRES_RESERVADOS.contains(&raiz)
                && raiz
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }
        None => false,
    }
}

/// Una línea del índice, `^- \[(.+?)\]\(([a-z0-9-]+\.md)\)`: el título y el fichero.
/// Como la expresión equivalente, el título es lo más corto posible.
fn entrada_del_indice(linea: &str) -> Option<(&str, &str)> {
    let resto = linea.strip_prefix("- [")?;
    let mut desde = 0usize;
    // Dónde está el `)` que cierra el tramo actual, buscado una sola vez por tramo:
    // con una línea de muchos «](» buscarlo en cada vuelta costaría tiempo cuadrático.
    let mut cierre: Option<usize> = None;
    while let Some(i) = resto.get(desde..)?.find("](") {
        let pos = desde + i;
        let tras = pos + 2;
        if cierre.is_none_or(|c| c < tras) {
            cierre = resto.get(tras..)?.find(')').map(|f| tras + f);
        }
        // Sin más `)`, ningún «](» posterior puede cerrarse.
        let fin = cierre?;
        let nombre = &resto[tras..fin];
        if pos > 0 && nombre_de_capitulo_valido(nombre) {
            // Un título que contiene otro enlace (`[A](https://x) y [B](b.md)`)
            // es ambiguo: no se adivina cuál es el título.
            let titulo = &resto[..pos];
            return (!titulo.contains("](")).then_some((titulo, nombre));
        }
        desde = pos + 1;
    }
    None
}

/// Las líneas de la sección `## Capítulos` de `README.md`, con su posición en el
/// texto: desde la línea siguiente al título hasta el siguiente título de nivel 1
/// o 2. Lo que está dentro de un bloque de código no cuenta.
fn bloque_de_capitulos(readme: &str) -> Resultado<Vec<(usize, &str)>> {
    let mut en_codigo: Option<char> = None;
    let mut dentro = false;
    let mut vista = false;
    let mut lineas = Vec::new();
    let mut posicion = 0usize;
    for linea in readme.split_inclusive('\n') {
        let inicio = posicion;
        posicion += linea.len();
        let linea = linea.trim_end_matches(['\n', '\r']);
        let sin_sangria = linea.trim_start_matches(' ');
        if linea.len() - sin_sangria.len() <= 3 {
            let valla = if sin_sangria.starts_with("```") {
                Some('`')
            } else if sin_sangria.starts_with("~~~") {
                Some('~')
            } else {
                None
            };
            if let Some(v) = valla {
                en_codigo = match en_codigo {
                    None => Some(v),
                    Some(abierta) if abierta == v => None,
                    otra => otra,
                };
                continue;
            }
        }
        if en_codigo.is_some() {
            continue;
        }
        if !dentro {
            if linea.trim_end() == "## Capítulos" {
                if vista {
                    return Err(error_en(
                        "README.md",
                        readme,
                        inicio,
                        "«## Capítulos» aparece más de una vez",
                    ));
                }
                vista = true;
                dentro = true;
            }
            continue;
        }
        if linea == "#" || linea == "##" || linea.starts_with("# ") || linea.starts_with("## ") {
            dentro = false;
            continue;
        }
        lineas.push((inicio, linea));
    }
    if !vista {
        return Err(ErrorWeb::dato_raro(
            "README.md",
            "falta la sección «## Capítulos» con el índice del manual",
        ));
    }
    Ok(lineas)
}

/// Escapa con barra invertida los caracteres de `escapar` (y la propia barra).
fn escapar_con_barra(texto: &str, escapar: &[char]) -> String {
    let mut salida = String::with_capacity(texto.len() + 4);
    for c in texto.chars() {
        if c == '\\' || escapar.contains(&c) {
            salida.push('\\');
        }
        salida.push(c);
    }
    salida
}

/// El texto que leería mdBook en el enlace `- [escrito](fichero)` de `SUMMARY.md`.
fn texto_leido_del_enlace(escrito: &str, fichero: &str) -> String {
    let linea = format!("- [{escrito}]({fichero})");
    let mut texto = String::new();
    let mut dentro = false;
    for evento in Parser::new_ext(&linea, opciones()) {
        match evento {
            Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == fichero => {
                dentro = true;
            }
            Event::End(TagEnd::Link) => dentro = false,
            Event::Text(t) | Event::Code(t) if dentro => texto.push_str(&t),
            Event::SoftBreak | Event::HardBreak if dentro => texto.push(' '),
            _ => {}
        }
    }
    texto
}

/// El título de una entrada del índice para `SUMMARY.md`: sin comillas invertidas y
/// con escapado solo lo que hace falta. Primero se escapa lo mínimo (la barra y los
/// corchetes, que rompen el enlace); si así mdBook no leería el título tal cual (un
/// `<host>`, un `*énfasis*`…), se escapa también el resto de lo que Markdown
/// interpreta. Si ni así sale el mismo texto, es un error.
fn titulo_para_summary(titulo: &str, fichero: &str) -> Resultado<String> {
    let sin_codigo: String = titulo.chars().filter(|c| *c != '`').collect();
    let limpio = sin_codigo.trim();
    if limpio.is_empty() {
        return Err(ErrorWeb::dato_raro(
            "README.md",
            format!("el capítulo {fichero} tiene un título vacío en el índice"),
        ));
    }
    if limpio.chars().any(char::is_control) {
        return Err(ErrorWeb::dato_raro(
            "README.md",
            format!("el título de {fichero} en el índice lleva caracteres de control"),
        ));
    }
    if limpio.contains('<') {
        return Err(ErrorWeb::dato_raro(
            "README.md",
            format!(
                "el título de {fichero} en el índice lleva «<»: el buscador de mdBook pone los títulos en la página como HTML ({})",
                texto_seguro(limpio, 80)
            ),
        ));
    }
    for escapar in [&['[', ']'][..], &['[', ']', '<', '*', '_', '~', '&'][..]] {
        let escrito = escapar_con_barra(limpio, escapar);
        if texto_leido_del_enlace(&escrito, fichero) == limpio {
            return Ok(escrito);
        }
    }
    Err(ErrorWeb::dato_raro(
        "README.md",
        format!(
            "el título de {fichero} en el índice no se puede escribir en SUMMARY.md tal cual: {}",
            texto_seguro(limpio, 80)
        ),
    ))
}

/// Los capítulos del índice, en orden: `(título sin tratar, fichero)`. Valida que
/// existan, que no se repitan y que no sobre ningún `.md`.
fn capitulos_del_indice<'a>(
    readme: &'a str,
    ficheros: &[String],
) -> Resultado<Vec<(&'a str, &'a str)>> {
    let mut capitulos: Vec<(&str, &str)> = Vec::new();
    for (inicio, linea) in bloque_de_capitulos(readme)? {
        if !linea.starts_with("- [") {
            continue;
        }
        match entrada_del_indice(linea) {
            Some(entrada) => capitulos.push(entrada),
            None => {
                return Err(error_en(
                    "README.md",
                    readme,
                    inicio,
                    format!(
                        "entrada del índice que no es «- [Título](fichero.md)» con un fichero [a-z0-9-]+.md: {}",
                        texto_seguro(linea, 80)
                    ),
                ))
            }
        }
    }
    if capitulos.is_empty() {
        return Err(ErrorWeb::dato_raro(
            "README.md",
            "el índice «## Capítulos» no tiene ningún capítulo",
        ));
    }

    let existentes: BTreeSet<&str> = ficheros.iter().map(String::as_str).collect();
    let mut vistos: BTreeSet<&str> = BTreeSet::new();
    for (_, fichero) in &capitulos {
        if !vistos.insert(fichero) {
            return Err(ErrorWeb::dato_raro(
                "README.md",
                format!("capítulo repetido en el índice: {fichero}"),
            ));
        }
        if es_pagina_fija(fichero) {
            return Err(ErrorWeb::dato_raro(
                "README.md",
                format!(
                    "{fichero} no es un capítulo del índice: va siempre al final de SUMMARY.md"
                ),
            ));
        }
        if !existentes.contains(fichero) {
            return Err(ErrorWeb::dato_raro(
                "README.md",
                format!("el índice nombra {fichero}, que no está en docs/manual/"),
            ));
        }
    }
    if !existentes.contains("estado.md") {
        return Err(ErrorWeb::dato_raro(
            "docs/manual",
            "falta estado.md (SUMMARY.md lo enlaza al final)",
        ));
    }
    let fuera: Vec<&str> = existentes
        .iter()
        .copied()
        .filter(|f| f.ends_with(".md") && !es_pagina_fija(f) && !vistos.contains(f))
        .collect();
    if !fuera.is_empty() {
        // `BTreeSet` ya los da ordenados.
        return Err(ErrorWeb::dato_raro(
            "README.md",
            format!("capítulo fuera del índice: {}", fuera.join(", ")),
        ));
    }
    Ok(capitulos)
}

/// `SUMMARY.md` de mdBook a partir del `README.md` del manual y de los nombres de
/// sus ficheros `.md` (informe §7.5). Falla con «capítulo fuera del índice: a.md,
/// b.md» si un `.md` (que no sea `README.md` ni `estado.md`) no está en `## Capítulos`,
/// y si el índice nombra un fichero que no existe o repite uno.
pub fn summary(readme: &str, ficheros: &[String]) -> Resultado<String> {
    let capitulos = capitulos_del_indice(readme, ficheros)?;
    let mut salida = String::from("[Manual de MAGI](README.md)\n\n");
    for (titulo, fichero) in capitulos {
        salida.push_str(&format!(
            "- [{}]({fichero})\n",
            titulo_para_summary(titulo, fichero)?
        ));
    }
    salida.push_str("- [Qué se ha probado](estado.md)\n");
    Ok(salida)
}

// ---------------------------------------------------------------------------
// Preprocesadores de mdBook
// ---------------------------------------------------------------------------

/// Falla si el texto lleva `{{#` (en cualquier sitio), diciendo fichero y línea.
///
/// mdBook los lee con `\{\{\s*#`: también se rechaza `{{ #include … }}` (y el salto
/// de línea entre `{{` y `#`). `{{servicio}}` sin almohadilla es texto normal.
pub fn rechazar_preprocesadores(markdown: &str, origen: &str) -> Resultado<()> {
    let mut desde = 0usize;
    while let Some(i) = markdown.get(desde..).and_then(|r| r.find("{{")) {
        let pos = desde + i;
        let tras = markdown.get(pos + 2..).unwrap_or("");
        if tras
            .trim_start_matches(char::is_whitespace)
            .starts_with('#')
        {
            return Err(error_en(
                origen,
                markdown,
                pos,
                "«{{#» es un preprocesador de mdBook ({{#include}}, {{#playground}}…): el manual no los usa y mdBook los ejecutaría, también dentro de código",
            ));
        }
        desde = pos + 1;
    }
    Ok(())
}

/// Falla si hay una tabla con alineación de columnas, diciendo fichero y línea.
///
/// mdBook 0.5.4 pone a esas celdas `style="text-align: …"`, y la CSP de la web
/// (sin `'unsafe-inline'`) bloquea todo atributo `style`: `adaptar_pagina` también
/// lo rechazaría, pero tarde y sin decir qué línea del Markdown lo causó.
pub fn rechazar_tablas_alineadas(markdown: &str, origen: &str) -> Resultado<()> {
    for (evento, rango) in Parser::new_ext(markdown, opciones()).into_offset_iter() {
        if let Event::Start(Tag::Table(alineaciones)) = evento {
            if alineaciones.iter().any(|a| *a != Alignment::None) {
                return Err(error_en(
                    origen,
                    markdown,
                    rango.start,
                    "una tabla con alineación de columnas (`:--`, `:-:`, `--:`): mdBook la escribe con `style` y la CSP de la web no lo admite; quita los dos puntos de la fila de guiones",
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Anclas
// ---------------------------------------------------------------------------

/// Un título ATX tal como lo ve el analizador de Markdown.
struct Titulo {
    /// Dónde empieza (la primera `#`).
    inicio: usize,
    /// Dónde acaba su primera línea sin contar blancos finales: ahí va el `{#id}`.
    fin: usize,
    nivel: usize,
    /// Su texto, derivado de la línea como lo hace `anclas::encabezados_de`.
    texto: String,
}

/// Los títulos de `markdown` con su posición. Rechaza lo que no se puede anclar:
/// títulos que ya llevan atributos, subrayados (setext) o que no empiezan en la
/// primera columna (dentro de una cita, una lista o con sangría).
fn titulos_de(markdown: &str, origen: &str) -> Resultado<Vec<Titulo>> {
    let mut titulos = Vec::new();
    for (evento, rango) in Parser::new_ext(markdown, opciones()).into_offset_iter() {
        let Event::Start(Tag::Heading {
            id, classes, attrs, ..
        }) = evento
        else {
            continue;
        };
        if id.is_some() || !classes.is_empty() || !attrs.is_empty() {
            return Err(error_en(
                origen,
                markdown,
                rango.start,
                "el título ya trae atributos propios ({#id}, {.clase}…): MAGI pone el id de cada título solo",
            ));
        }
        let fuente = trozo(markdown, &rango, origen)?;
        let inicio_de_linea = markdown
            .get(..rango.start)
            .and_then(|antes| antes.rfind('\n'))
            .map_or(0, |i| i + 1);
        if inicio_de_linea != rango.start {
            return Err(error_en(
                origen,
                markdown,
                rango.start,
                "título con sangría o dentro de una cita o una lista: no se admite (las anclas de GitHub de MAGI solo cuentan los títulos de la primera columna)",
            ));
        }
        if !fuente.starts_with('#') {
            return Err(error_en(
                origen,
                markdown,
                rango.start,
                "título subrayado no admitido: usa «# Título» (el algoritmo de anclas solo lee títulos con almohadillas)",
            ));
        }
        let linea = fuente
            .split('\n')
            .next()
            .unwrap_or("")
            .trim_end_matches('\r');
        let almohadillas = linea.chars().take_while(|c| *c == '#').count();
        if !(1..=6).contains(&almohadillas) || linea.chars().nth(almohadillas) != Some(' ') {
            return Err(error_en(
                origen,
                markdown,
                rango.start,
                "el título no es «#» (de 1 a 6) seguido de un espacio y su texto",
            ));
        }
        let texto = linea[almohadillas..]
            .trim()
            .trim_end_matches('#')
            .trim()
            .to_string();
        titulos.push(Titulo {
            inicio: rango.start,
            fin: rango.start + linea.trim_end().len(),
            nivel: almohadillas,
            texto,
        });
    }
    Ok(titulos)
}

/// Falla si el texto de un título (ya descodificado: también `&lt;` y el código en
/// línea) lleva «<». El buscador de mdBook 0.5.4 pone en la página, con `innerHTML` y
/// sin escapar, el título del capítulo y el del encabezado de cada resultado: un
/// `<meta http-equiv=refresh …>` en un título redirigiría al abrir la búsqueda, y ni
/// `comprobar` (que mira el HTML estático) ni la CSP lo ven. Un `<host>` legítimo
/// tampoco se vería (se tragaría el texto). Los títulos no llevan «<»; en el cuerpo del
/// capítulo, que mdBook sí escapa, no hay límite.
pub fn rechazar_menor_que_en_titulos(markdown: &str, origen: &str) -> Resultado<()> {
    let mut en_titulo = false;
    for (evento, rango) in Parser::new_ext(markdown, opciones()).into_offset_iter() {
        match evento {
            Event::Start(Tag::Heading { .. }) => en_titulo = true,
            Event::End(TagEnd::Heading(_)) => en_titulo = false,
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t)
                if en_titulo && t.contains('<') =>
            {
                return Err(error_en(
                    origen,
                    markdown,
                    rango.start,
                    "un título lleva «<»: el buscador de mdBook pone los títulos en la página como HTML y lo trataría como marcado; cambia el texto del título",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Pone a cada título su `{#id}` con el algoritmo de GitHub (`anclas::slugs_de`),
/// sin tocar los `#` de dentro de bloques de código. `origen` sale en los errores.
///
/// El `{#id}` va al final de la línea del título, después de los `#` de cierre si
/// los hay (con `## Título ## {#id}`, mdBook lo lee y quita también los de cierre;
/// antes de ellos no lo lee). Comprueba que el analizador ve los mismos títulos que
/// `anclas::encabezados_de` y, al acabar, que cada uno quedó con su id.
pub fn con_anclas(markdown: &str, origen: &str) -> Resultado<String> {
    let titulos = titulos_de(markdown, origen)?;
    let referencia = encabezados_de(markdown);

    for (i, (titulo, (nivel, texto))) in titulos.iter().zip(&referencia).enumerate() {
        if titulo.nivel != *nivel || titulo.texto != *texto {
            return Err(error_en(
                origen,
                markdown,
                titulo.inicio,
                format!(
                    "el título {} no coincide con el que cuenta el algoritmo de anclas ({}º: «{}», nivel {nivel}): el Markdown y las anclas no se alinean",
                    texto_seguro(&titulo.texto, 60),
                    i + 1,
                    texto_seguro(texto, 60)
                ),
            ));
        }
    }
    if titulos.len() != referencia.len() {
        let detalle = match titulos.get(referencia.len()) {
            Some(extra) => format!(
                "el analizador ve más títulos que el algoritmo de anclas; el primero de más es «{}»",
                texto_seguro(&extra.texto, 60)
            ),
            None => format!(
                "el algoritmo de anclas cuenta {} título(s) que el analizador no ve como títulos (¿dentro de un bloque de HTML o de código con otra valla?)",
                referencia.len() - titulos.len()
            ),
        };
        let pos = titulos.get(referencia.len()).map_or(0, |t| t.inicio);
        return Err(error_en(
            origen,
            markdown,
            pos,
            format!(
                "{} títulos para el analizador y {} para el algoritmo de anclas: {detalle}",
                titulos.len(),
                referencia.len()
            ),
        ));
    }

    let slugs = slugs_de(markdown);
    if slugs.len() != titulos.len() {
        return Err(ErrorWeb::dato_raro(
            origen,
            "el algoritmo de anclas devolvió otro número de ids (error interno)",
        ));
    }
    let mut vistos: HashSet<&str> = HashSet::new();
    for (titulo, slug) in titulos.iter().zip(&slugs) {
        if slug.is_empty() {
            return Err(error_en(
                origen,
                markdown,
                titulo.inicio,
                format!(
                    "el título «{}» no tiene ancla posible (sin letras ni cifras)",
                    texto_seguro(&titulo.texto, 60)
                ),
            ));
        }
        if !vistos.insert(slug) {
            return Err(error_en(
                origen,
                markdown,
                titulo.inicio,
                format!("dos títulos acabarían con el mismo id «{slug}»"),
            ));
        }
    }

    let ediciones = titulos
        .iter()
        .zip(&slugs)
        .map(|(t, slug)| (t.fin..t.fin, format!(" {{#{slug}}}")))
        .collect();
    let salida = aplicar(markdown, ediciones, origen)?;

    // Comprobación: el analizador ve cada título con su id, sin nada más.
    let mut ids: Vec<Option<String>> = Vec::new();
    let mut sueltos = false;
    for evento in Parser::new_ext(&salida, opciones()) {
        if let Event::Start(Tag::Heading {
            id, classes, attrs, ..
        }) = evento
        {
            sueltos |= !classes.is_empty() || !attrs.is_empty();
            ids.push(id.map(|i| i.to_string()));
        }
    }
    let esperados: Vec<Option<String>> = slugs.iter().map(|s| Some(s.clone())).collect();
    if sueltos || ids != esperados {
        let malo = ids
            .iter()
            .zip(&esperados)
            .position(|(visto, esperado)| visto != esperado)
            .unwrap_or(0);
        let pos = titulos.get(malo).map_or(0, |t| t.inicio);
        return Err(error_en(
            origen,
            markdown,
            pos,
            "mdBook no leería el {#id} de este título tal como se escribió (error interno de las anclas)",
        ));
    }
    Ok(salida)
}

// ---------------------------------------------------------------------------
// Enlaces
// ---------------------------------------------------------------------------

/// `^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$`: la etiqueta de una versión
/// publicada del escritorio, sin candidatas ni sufijos.
fn etiqueta_valida(etiqueta: &str) -> bool {
    let Some(numeros) = etiqueta.strip_prefix('v') else {
        return false;
    };
    let partes: Vec<&str> = numeros.split('.').collect();
    partes.len() == 3
        && partes.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 9
                && p.bytes().all(|b| b.is_ascii_digit())
                && (*p == "0" || !p.starts_with('0'))
        })
}

fn validar_etiqueta(etiqueta: &str) -> Resultado<()> {
    if etiqueta_valida(etiqueta) {
        Ok(())
    } else {
        Err(ErrorWeb::dato_raro(
            "--etiqueta",
            format!(
                "«{}» no es una versión publicada (vX.Y.Z, sin candidatas ni sufijos)",
                texto_seguro(etiqueta, 40)
            ),
        ))
    }
}

fn extension_en(nombre: &str, lista: &[&str]) -> bool {
    nombre
        .rsplit_once('.')
        .is_some_and(|(_, ext)| lista.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

fn es_imagen(nombre: &str) -> bool {
    extension_en(nombre, IMAGENES)
}

fn es_markdown(nombre: &str) -> bool {
    extension_en(nombre, &["md"])
}

fn hexadecimal(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Descodifica `%XX` de una ruta (UTF-8 válido). `None` si hay un `%` mal formado,
/// un carácter de control, `\` o un `/` escondido (`%2F`), que un servidor y un
/// navegador no leen igual.
fn descodificar_ruta(ruta: &str) -> Option<String> {
    if !ruta.contains('%') {
        return Some(ruta.to_string());
    }
    let b = ruta.as_bytes();
    let mut bytes = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'%' {
            let valor = hexadecimal(*b.get(i + 1)?)? * 16 + hexadecimal(*b.get(i + 2)?)?;
            if valor == b'/' || valor == b'\\' || valor < 0x20 || valor == 0x7f {
                return None;
            }
            bytes.push(valor);
            i += 3;
        } else {
            bytes.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(bytes).ok()
}

fn es_no_reservado(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

fn push_porcentaje(salida: &mut String, b: u8) {
    salida.push_str(&format!("%{b:02X}"));
}

/// Una ruta del repositorio lista para una URL: solo letras, cifras y `-._~/`; lo
/// demás en porcentaje (UTF-8).
fn codificar_ruta(ruta: &str) -> String {
    let mut salida = String::with_capacity(ruta.len());
    for b in ruta.bytes() {
        if es_no_reservado(b) || b == b'/' {
            salida.push(b as char);
        } else {
            push_porcentaje(&mut salida, b);
        }
    }
    salida
}

/// Una consulta o un fragmento listos para escribirse en un destino de Markdown sin
/// ángulos: los `%XX` válidos se conservan, `&` va como `&amp;` (Markdown lo
/// descodifica) y lo que no es seguro (blancos, paréntesis, ángulos, comillas, no
/// ASCII…) va en porcentaje.
fn codificar_resto(texto: &str) -> String {
    let b = texto.as_bytes();
    let mut salida = String::with_capacity(texto.len());
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c == b'%'
            && b.get(i + 1).copied().and_then(hexadecimal).is_some()
            && b.get(i + 2).copied().and_then(hexadecimal).is_some()
        {
            salida.push_str(&texto[i..i + 3]);
            i += 3;
            continue;
        }
        if es_no_reservado(c)
            || matches!(
                c,
                b'!' | b'$' | b'\'' | b'*' | b'+' | b',' | b';' | b'=' | b':' | b'@' | b'/' | b'?'
            )
        {
            salida.push(c as char);
        } else if c == b'&' {
            salida.push_str("&amp;");
        } else {
            push_porcentaje(&mut salida, c);
        }
        i += 1;
    }
    salida
}

/// `?consulta#ancla` ya codificados (vacío si no hay nada).
fn sufijo(consulta: Option<&str>, ancla: Option<&str>) -> String {
    let mut salida = String::new();
    if let Some(c) = consulta.filter(|c| !c.is_empty()) {
        salida.push('?');
        salida.push_str(&codificar_resto(c));
    }
    if let Some(a) = ancla.filter(|a| !a.is_empty()) {
        salida.push('#');
        salida.push_str(&codificar_resto(a));
    }
    salida
}

/// URL de GitHub, en la etiqueta, de una ruta del repositorio ya resuelta: `blob`
/// para ficheros y `tree` para carpetas (las que acaban en `/`).
fn url_de_github(ruta: &str, etiqueta: &str) -> String {
    if ruta.is_empty() {
        return format!("{REPOSITORIO}/tree/{etiqueta}");
    }
    let tipo = if ruta.ends_with('/') { "tree" } else { "blob" };
    format!("{REPOSITORIO}/{tipo}/{etiqueta}/{}", codificar_ruta(ruta))
}

/// El esquema de un destino (`https`, `mailto`…) si lo lleva. Un `:` antes de la
/// primera barra que no forma un esquema es ambiguo y se rechaza.
fn esquema_de(destino: &str) -> Result<Option<&str>, String> {
    let fin = destino.find(['/', '?', '#']).unwrap_or(destino.len());
    let Some(dos_puntos) = destino[..fin].find(':') else {
        return Ok(None);
    };
    let esquema = &destino[..dos_puntos];
    let valido = esquema
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && esquema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    if valido {
        Ok(Some(esquema))
    } else {
        Err("lleva «:» antes de la primera barra sin ser un esquema (ambiguo)".to_string())
    }
}

/// Esquemas que ejecutan o incrustan código: ningún destino los lleva, ni en un
/// enlace en línea ni en un autoenlace (`<javascript:…>`).
fn esquema_prohibido(esquema: &str) -> bool {
    ["javascript", "data", "vbscript"]
        .iter()
        .any(|e| esquema.eq_ignore_ascii_case(e))
}

/// Qué hacer con el destino de un enlace.
#[derive(Debug, Default, PartialEq, Eq)]
struct Decision {
    /// El destino nuevo, si cambia.
    nuevo: Option<String>,
    /// Si apunta a un fichero local de `docs/manual/` que tiene que viajar con el
    /// libro (una imagen): su ruta dentro de `docs/manual/`.
    local: Option<String>,
}

/// Lo que hace falta saber del capítulo que se reescribe.
struct Contexto<'a> {
    /// Carpeta del capítulo, desde la raíz del repositorio (`docs/manual`).
    carpeta: String,
    etiqueta: &'a str,
    /// `README.md`, `estado.md` y los capítulos: los `.md` que van al libro.
    paginas: BTreeSet<&'a str>,
}

impl Contexto<'_> {
    /// Decide qué hacer con `logico`, el destino ya sin escapes ni entidades.
    fn decidir(&self, logico: &str, imagen: bool) -> Result<Decision, String> {
        if logico.len() > TOPE_DESTINO {
            return Err(format!("destino de más de {TOPE_DESTINO} caracteres"));
        }
        if logico.chars().any(|c| c.is_control() || c == '\\') {
            return Err("el destino lleva caracteres de control o «\\»".to_string());
        }
        if logico.is_empty() {
            return if imagen {
                Err("imagen sin ruta".to_string())
            } else {
                Ok(Decision::default())
            };
        }
        if let Some(esquema) = esquema_de(logico)? {
            if esquema_prohibido(esquema) {
                return Err(format!("el esquema «{esquema}:» no se admite"));
            }
            if imagen {
                return Err(
                    "imagen de fuera del repositorio: la web no carga nada de otros sitios"
                        .to_string(),
                );
            }
            return Ok(Decision::default());
        }
        if logico.starts_with('#') {
            return if imagen {
                Err("una imagen no puede ser solo un fragmento".to_string())
            } else {
                Ok(Decision::default())
            };
        }
        if logico.starts_with('/') {
            return Err("ruta absoluta: usa una ruta relativa a docs/manual/".to_string());
        }
        if logico.starts_with('?') {
            return Err("destino sin ruta (solo una consulta)".to_string());
        }

        let (sin_ancla, ancla) = match logico.split_once('#') {
            Some((a, b)) => (a, Some(b)),
            None => (logico, None),
        };
        let (ruta_cruda, consulta) = match sin_ancla.split_once('?') {
            Some((a, b)) => (a, Some(b)),
            None => (sin_ancla, None),
        };
        let ruta = descodificar_ruta(ruta_cruda)
            .ok_or_else(|| "ruta con un «%» mal formado o un «/» escondido".to_string())?;
        let resuelta = resolver(&self.carpeta, &ruta).ok_or_else(|| {
            "la ruta sale de la raíz del repositorio o no es una ruta válida".to_string()
        })?;
        let sufijo = sufijo(consulta, ancla);

        // Los README de la raíz son la portada de la web.
        if resuelta == "README.md" || resuelta == "README.en.md" {
            if imagen {
                return Err("una imagen no puede apuntar a un README".to_string());
            }
            let portada = if resuelta == "README.md" {
                "../"
            } else {
                "../en/"
            };
            return Ok(Decision {
                nuevo: Some(format!("{portada}{sufijo}")),
                local: None,
            });
        }

        let prefijo = format!("{CARPETA_MANUAL}/");
        match resuelta.strip_prefix(&prefijo) {
            Some(nombre) => {
                let es_carpeta = nombre.is_empty() || nombre.ends_with('/');
                if !es_carpeta && es_markdown(nombre) {
                    if imagen {
                        return Err("una imagen no puede ser un .md".to_string());
                    }
                    if !self.paginas.contains(nombre) {
                        return Err(format!(
                            "enlace a {nombre}, un .md de docs/manual/ que no es un capítulo del libro"
                        ));
                    }
                    // mdBook pasa `x.md` a `x.html`, pero el capítulo `README.md` lo
                    // publica como `index.html` y deja el enlace en `README.html`, que
                    // no existe: se enlaza `index.md`, que sí llega a `index.html`.
                    let destino = if nombre == "README.md" {
                        "index.md"
                    } else {
                        nombre
                    };
                    let nuevo = (ruta_cruda != destino).then(|| format!("{destino}{sufijo}"));
                    return Ok(Decision { nuevo, local: None });
                }
                if !es_carpeta && es_imagen(nombre) {
                    // El nombre ya va descodificado: se vuelve a codificar para que lo
                    // que se escribe en el Markdown sea siempre un destino entero (un
                    // `%20` o un paréntesis no pueden partirlo).
                    let escrito = codificar_ruta(nombre);
                    let nuevo = (ruta_cruda != escrito).then(|| format!("{escrito}{sufijo}"));
                    return Ok(Decision {
                        nuevo,
                        local: Some(nombre.to_string()),
                    });
                }
                if imagen {
                    return Err(format!(
                        "{nombre} no es una imagen (png, jpg, jpeg, gif o webp)"
                    ));
                }
            }
            None if imagen => {
                return Err(
                    "imagen fuera de docs/manual/: solo se copian las del manual".to_string(),
                )
            }
            None => {}
        }

        Ok(Decision {
            nuevo: Some(format!(
                "{}{sufijo}",
                url_de_github(&resuelta, self.etiqueta)
            )),
            local: None,
        })
    }
}

/// Dónde está el destino de un enlace dentro del Markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Region {
    inicio: usize,
    fin: usize,
    con_angulos: bool,
}

fn saltar_blancos(s: &[u8], mut i: usize) -> usize {
    while matches!(s.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    i
}

fn racha(s: &[u8], i: usize, c: u8) -> usize {
    s.get(i..)
        .map_or(0, |r| r.iter().take_while(|b| **b == c).count())
}

/// Dónde acaba una racha de `n` comillas invertidas que empieza en `desde` o
/// después (la que cierra un código en línea).
fn cierre_de_codigo(s: &[u8], mut desde: usize, n: usize) -> Option<usize> {
    while desde < s.len() {
        if s[desde] == b'`' {
            let m = racha(s, desde, b'`');
            if m == n {
                return Some(desde + m);
            }
            desde += m;
        } else {
            desde += 1;
        }
    }
    None
}

/// La posición del `]` que cierra el texto de un enlace (que empieza en `desde`, ya
/// dentro del primer `[`) si va seguido de `(`.
fn cierre_del_texto(s: &[u8], desde: usize) -> Option<usize> {
    let mut i = desde;
    let mut profundidad = 1usize;
    while i < s.len() {
        match s[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'`' => {
                let n = racha(s, i, b'`');
                i = cierre_de_codigo(s, i + n, n).unwrap_or(i + n);
                continue;
            }
            b'[' => profundidad += 1,
            b']' => {
                profundidad -= 1;
                if profundidad == 0 {
                    return (s.get(i + 1) == Some(&b'(')).then_some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Lee un destino que empieza en `i`: entre ángulos o «crudo» (sin blancos y con
/// paréntesis equilibrados). Devuelve su región y dónde sigue el texto.
fn escanear_destino(s: &[u8], i: usize) -> Option<(Region, usize)> {
    if s.get(i) == Some(&b'<') {
        let mut j = i + 1;
        while j < s.len() {
            match s[j] {
                b'\\' => {
                    j += 2;
                    continue;
                }
                b'>' => {
                    return Some((
                        Region {
                            inicio: i,
                            fin: j + 1,
                            con_angulos: true,
                        },
                        j + 1,
                    ))
                }
                b'<' | b'\n' | b'\r' => return None,
                _ => {}
            }
            j += 1;
        }
        return None;
    }
    let mut j = i;
    let mut profundidad = 0usize;
    while j < s.len() {
        match s[j] {
            b'\\' if s.get(j + 1).is_some_and(u8::is_ascii_punctuation) => {
                j += 2;
                continue;
            }
            b'(' => profundidad += 1,
            b')' => {
                if profundidad == 0 {
                    break;
                }
                profundidad -= 1;
            }
            c if c <= b' ' || c == 0x7f => break,
            _ => {}
        }
        j += 1;
    }
    if profundidad != 0 {
        return None;
    }
    Some((
        Region {
            inicio: i,
            fin: j,
            con_angulos: false,
        },
        j,
    ))
}

/// Salta un título opcional (`"…"`, `'…'` o `(…)`) que empieza en `i`: devuelve
/// dónde acaba.
fn saltar_titulo(s: &[u8], i: usize) -> Option<usize> {
    let cierre = match s.get(i)? {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let mut j = i + 1;
    while j < s.len() {
        match s[j] {
            b'\\' => {
                j += 2;
                continue;
            }
            c if c == cierre => return Some(j + 1),
            _ => {}
        }
        j += 1;
    }
    None
}

/// El destino de un enlace en línea `[texto](destino "título")` o de una imagen
/// `![texto](destino)`, a partir del trozo de origen que el analizador atribuye al
/// enlace entero. La región va relativa al trozo.
fn destino_en_linea(fuente: &str, imagen: bool) -> Option<Region> {
    let s = fuente.as_bytes();
    let inicio_texto = if imagen { 2 } else { 1 };
    if s.get(inicio_texto - 1) != Some(&b'[') {
        return None;
    }
    let cierre = cierre_del_texto(s, inicio_texto)?;
    let i = saltar_blancos(s, cierre + 2);
    let (region, siguiente) = if s.get(i) == Some(&b')') {
        // `[t]()`: destino vacío.
        (
            Region {
                inicio: i,
                fin: i,
                con_angulos: false,
            },
            i,
        )
    } else {
        escanear_destino(s, i)?
    };
    let mut k = saltar_blancos(s, siguiente);
    if k > siguiente && s.get(k) != Some(&b')') {
        k = saltar_blancos(s, saltar_titulo(s, k)?);
    }
    (s.get(k) == Some(&b')') && k + 1 == s.len()).then_some(region)
}

/// El destino de una definición de enlace `[etiqueta]: destino "título"` a partir de
/// su trozo de origen. La región va relativa al trozo.
fn destino_en_definicion(fuente: &str) -> Option<Region> {
    let s = fuente.as_bytes();
    let mut i = 0usize;
    while i < 3 && s.get(i) == Some(&b' ') {
        i += 1;
    }
    if s.get(i) != Some(&b'[') {
        return None;
    }
    i += 1;
    loop {
        match s.get(i)? {
            b'\\' => i += 2,
            b']' => break,
            b'[' => return None,
            _ => i += 1,
        }
    }
    if s.get(i + 1) != Some(&b':') {
        return None;
    }
    let i = saltar_blancos(s, i + 2);
    let (region, siguiente) = escanear_destino(s, i)?;
    // Lo que queda es el título (opcional): nada más.
    let k = saltar_blancos(s, siguiente);
    if k < s.len() && saltar_blancos(s, saltar_titulo(s, k)?) != s.len() {
        return None;
    }
    Some(region)
}

/// El destino que dice una región cruda, descodificada como lo hace Markdown
/// (escapes con barra invertida). `None` si lleva una entidad (`&…;`): no se
/// comprueba.
fn descodificar_region(cruda: &str, con_angulos: bool) -> Option<String> {
    if cruda.contains('&') {
        return None;
    }
    let interior = if con_angulos {
        cruda.strip_prefix('<')?.strip_suffix('>')?
    } else {
        cruda
    };
    let mut salida = String::with_capacity(interior.len());
    let mut chars = interior.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(char::is_ascii_punctuation) {
            if let Some(siguiente) = chars.next() {
                salida.push(siguiente);
            }
        } else {
            salida.push(c);
        }
    }
    Some(salida)
}

/// Reescribe los enlaces y devuelve además las imágenes locales a las que apunta
/// (ruta dentro de `docs/manual/` y línea). Es `reescribir_enlaces` con lo que
/// necesita `preparar_manual` para comprobar que esas imágenes viajan.
fn reescribir(
    markdown: &str,
    fichero: &str,
    etiqueta: &str,
    capitulos: &[String],
    origen: &str,
) -> Resultado<(String, Vec<(String, usize)>)> {
    validar_etiqueta(etiqueta)?;
    if fichero.is_empty()
        || fichero.starts_with('/')
        || fichero.chars().any(|c| c.is_control() || c == '\\')
        || fichero.split('/').any(|s| s == ".." || s == ".")
    {
        return Err(ErrorWeb::dato_raro(
            origen,
            format!(
                "nombre de fichero del manual no válido: {}",
                texto_seguro(fichero, 60)
            ),
        ));
    }
    let carpeta = match fichero.rsplit_once('/') {
        Some((madre, _)) => format!("{CARPETA_MANUAL}/{madre}"),
        None => CARPETA_MANUAL.to_string(),
    };
    let mut paginas: BTreeSet<&str> = capitulos.iter().map(String::as_str).collect();
    paginas.insert("README.md");
    paginas.insert("estado.md");
    let contexto = Contexto {
        carpeta,
        etiqueta,
        paginas,
    };

    let mut acumulado = Acumulado {
        contexto: &contexto,
        markdown,
        origen,
        ediciones: Vec::new(),
        locales: Vec::new(),
    };
    let mut etiquetas_de_imagen: HashSet<String> = HashSet::new();

    let mut analizador = Parser::new_ext(markdown, opciones()).into_offset_iter();
    for (evento, rango) in analizador.by_ref() {
        let (tipo, dest_url, imagen) = match evento {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => (link_type, dest_url, false),
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                id,
                ..
            }) => {
                if matches!(
                    link_type,
                    LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                ) {
                    etiquetas_de_imagen.insert(normalizar_etiqueta(&id));
                }
                (link_type, dest_url, true)
            }
            _ => continue,
        };
        // Los autoenlaces (`<https://…>`) no se reescriben, pero tampoco pueden
        // llevar un esquema que ejecute algo: la web los comprobaría al final, y
        // aquí se rechazan como los demás destinos.
        if matches!(tipo, LinkType::Autolink) {
            if let Ok(Some(esquema)) = esquema_de(&dest_url) {
                if esquema_prohibido(esquema) {
                    return Err(error_en(
                        origen,
                        markdown,
                        rango.start,
                        format!(
                            "autoenlace «{}»: el esquema «{esquema}:» no se admite",
                            texto_seguro(&dest_url, 80)
                        ),
                    ));
                }
            }
            continue;
        }
        // Solo los enlaces en línea llevan el destino en su propio trozo; los de
        // referencia lo toman de su definición.
        if !matches!(tipo, LinkType::Inline) {
            continue;
        }
        let fuente = trozo(markdown, &rango, origen)?;
        let region = destino_en_linea(fuente, imagen).ok_or_else(|| {
            error_en(
                origen,
                markdown,
                rango.start,
                "no se pudo localizar el destino del enlace dentro del texto",
            )
        })?;
        let absoluta = acumulado.verificar(rango.start, &region, &dest_url)?;
        acumulado.anotar(&dest_url, imagen, absoluta)?;
    }

    // Definiciones de enlace de referencia: el destino está en la definición.
    let mut definiciones: Vec<(String, String, Range<usize>)> = analizador
        .reference_definitions()
        .iter()
        .map(|(etiqueta, def)| (etiqueta.to_string(), def.dest.to_string(), def.span.clone()))
        .collect();
    definiciones.sort_by_key(|(_, _, span)| span.start);
    for (etiqueta_def, dest_url, span) in definiciones {
        let fuente = trozo(markdown, &span, origen)?;
        let region = destino_en_definicion(fuente).ok_or_else(|| {
            error_en(
                origen,
                markdown,
                span.start,
                "no se pudo localizar el destino de la definición de enlace dentro del texto",
            )
        })?;
        let absoluta = acumulado.verificar(span.start, &region, &dest_url)?;
        let imagen = etiquetas_de_imagen.contains(&normalizar_etiqueta(&etiqueta_def));
        acumulado.anotar(&dest_url, imagen, absoluta)?;
    }

    let Acumulado {
        ediciones, locales, ..
    } = acumulado;
    let salida = aplicar(markdown, ediciones, origen)?;
    Ok((salida, con_lineas(markdown, locales)))
}

/// Cambia la posición de cada imagen por su línea (desde 1), con un solo recorrido
/// del texto.
fn con_lineas(texto: &str, locales: Vec<(String, usize)>) -> Vec<(String, usize)> {
    if locales.is_empty() {
        return locales;
    }
    let saltos: Vec<usize> = texto
        .bytes()
        .enumerate()
        .filter(|(_, b)| *b == b'\n')
        .map(|(i, _)| i)
        .collect();
    locales
        .into_iter()
        .map(|(nombre, pos)| (nombre, saltos.partition_point(|s| *s < pos) + 1))
        .collect()
}

/// La etiqueta de una definición como la compara Markdown: sin mayúsculas ni
/// blancos repetidos.
fn normalizar_etiqueta(etiqueta: &str) -> String {
    etiqueta
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Lo que va saliendo de recorrer un capítulo: los cambios y las imágenes locales.
struct Acumulado<'a> {
    contexto: &'a Contexto<'a>,
    markdown: &'a str,
    origen: &'a str,
    ediciones: Vec<(Range<usize>, String)>,
    /// Imágenes locales a las que apunta el capítulo, con su posición en el texto
    /// (la línea se calcula al final, de una vez: `linea_de` por imagen sería
    /// cuadrático).
    locales: Vec<(String, usize)>,
}

impl Acumulado<'_> {
    /// La región absoluta de un destino localizado dentro de un trozo que empieza en
    /// `base`, comprobando que dice lo mismo que el analizador (`esperado`).
    fn verificar(
        &self,
        base: usize,
        region: &Region,
        esperado: &str,
    ) -> Result<Range<usize>, ErrorWeb> {
        let absoluta = (base + region.inicio)..(base + region.fin);
        let cruda = trozo(self.markdown, &absoluta, self.origen)?;
        if let Some(leido) = descodificar_region(cruda, region.con_angulos) {
            if leido != esperado {
                return Err(error_en(
                    self.origen,
                    self.markdown,
                    absoluta.start,
                    "el destino leído no coincide con el del analizador de Markdown (error interno)",
                ));
            }
        }
        Ok(absoluta)
    }

    /// Decide qué hacer con el destino `logico`, que está en `region`, y lo anota.
    fn anotar(&mut self, logico: &str, imagen: bool, region: Range<usize>) -> Resultado<()> {
        let decision = self.contexto.decidir(logico, imagen).map_err(|motivo| {
            error_en(
                self.origen,
                self.markdown,
                region.start,
                format!("enlace «{}»: {motivo}", texto_seguro(logico, 80)),
            )
        })?;
        if let Some(local) = decision.local {
            self.locales.push((local, region.start));
        }
        if let Some(nuevo) = decision.nuevo {
            self.ediciones.push((region, nuevo));
        }
        Ok(())
    }
}

/// Reescribe los enlaces que salen de `docs/manual/` (informe §7.5). `fichero` es el
/// nombre del capítulo dentro de `docs/manual/` y `capitulos`, los ficheros `.md`
/// que sí van al libro (los enlaces entre ellos se dejan: mdBook los pasa a `.html`).
///
/// - Los destinos con esquema (`https:`, `mailto:`), los fragmentos y los
///   autoenlaces se dejan; `javascript:`, `data:` y `vbscript:` se rechazan.
/// - Un capítulo (`nombre.md`, con o sin `#ancla`; también `estado.md`) se deja; un
///   `.md` de `docs/manual/` que no es capítulo es un error. El enlace al propio
///   `README.md` del manual pasa a `index.md` (mdBook publica ese capítulo como
///   `index.html` y su enlace a `README.md` saldría como `README.html`, roto).
/// - `../../README.md` pasa a `../` y `../../README.en.md` a `../en/`.
/// - Cualquier otra ruta del repositorio va a GitHub en la etiqueta: `blob` si es un
///   fichero y `tree` si acaba en `/`. Se conservan `?consulta` y `#fragmento`.
/// - Una ruta que sale de la raíz del repositorio, con `\` o con caracteres de
///   control, una ruta absoluta y una imagen que no es de `docs/manual/` (o remota,
///   que la CSP no cargaría) son un error con su línea.
///
/// Nunca se toca nada dentro de código en línea ni de bloques de código. Se
/// reescriben los enlaces en línea, las imágenes y las definiciones de enlace de
/// referencia.
pub fn reescribir_enlaces(
    markdown: &str,
    fichero: &str,
    etiqueta: &str,
    capitulos: &[String],
    origen: &str,
) -> Resultado<String> {
    reescribir(markdown, fichero, etiqueta, capitulos, origen).map(|(texto, _)| texto)
}

// ---------------------------------------------------------------------------
// HTML en bruto
// ---------------------------------------------------------------------------

/// Pone una barra invertida delante de cada `<` que no la lleve ya, y quita los
/// comentarios `<!-- … -->`. `por_bloque` dice que el trozo es un bloque de HTML:
/// un comentario sin cerrar se come el resto del documento, así que es un error.
fn escapar_trozo(trozo: &str, por_bloque: bool) -> Result<String, &'static str> {
    let mut salida = String::with_capacity(trozo.len() + 8);
    let mut resto = trozo;
    loop {
        match resto.find("<!--") {
            Some(i) => {
                escapar_menores(&resto[..i], &mut salida);
                let desde_comentario = &resto[i..];
                // Se busca el cierre desde el segundo guion: `<!-->` y `<!--->` ya
                // son comentarios completos.
                match desde_comentario[2..].find("-->") {
                    Some(j) => resto = &desde_comentario[2 + j + 3..],
                    None => {
                        return Err(if por_bloque {
                            "comentario HTML sin cerrar: se comería el resto del documento"
                        } else {
                            "comentario HTML sin cerrar"
                        })
                    }
                }
            }
            None => {
                escapar_menores(resto, &mut salida);
                return Ok(salida);
            }
        }
    }
}

fn escapar_menores(texto: &str, salida: &mut String) {
    for c in texto.chars() {
        if c == '<' {
            let barras = salida.chars().rev().take_while(|c| *c == '\\').count();
            if barras % 2 == 0 {
                salida.push('\\');
            }
        }
        salida.push(c);
    }
}

/// Cuántos títulos y cuántos trozos de HTML en bruto ve el analizador.
fn contar_titulos_y_html(markdown: &str) -> (usize, usize) {
    let mut titulos = 0usize;
    let mut html = 0usize;
    for evento in Parser::new_ext(markdown, opciones()) {
        match evento {
            Event::Start(Tag::Heading { .. }) => titulos += 1,
            Event::Html(_) | Event::InlineHtml(_) => html += 1,
            _ => {}
        }
    }
    (titulos, html)
}

/// Escapa como texto todo HTML en bruto fuera de código (`<host>` pasa a `\<host>`).
///
/// Los comentarios HTML se quitan (no se escapan). Un bloque de HTML de varias
/// líneas se escapa entero. Comprueba después que no queda HTML en bruto ni cambió
/// el número de títulos.
pub fn escapar_html_suelto(markdown: &str, origen: &str) -> Resultado<String> {
    let mut ediciones: Vec<(Range<usize>, String)> = Vec::new();
    let mut bloque: Option<Range<usize>> = None;
    for (evento, rango) in Parser::new_ext(markdown, opciones()).into_offset_iter() {
        match evento {
            Event::Start(Tag::HtmlBlock) => bloque = Some(rango),
            Event::End(TagEnd::HtmlBlock) => {
                if let Some(rango) = bloque.take() {
                    let original = trozo(markdown, &rango, origen)?;
                    let nuevo = escapar_trozo(original, true)
                        .map_err(|motivo| error_en(origen, markdown, rango.start, motivo))?;
                    ediciones.push((rango, nuevo));
                }
            }
            // Dentro de un bloque ya se trata el bloque entero.
            Event::Html(_) if bloque.is_some() => {}
            Event::Html(_) | Event::InlineHtml(_) => {
                let original = trozo(markdown, &rango, origen)?;
                let nuevo = escapar_trozo(original, false)
                    .map_err(|motivo| error_en(origen, markdown, rango.start, motivo))?;
                ediciones.push((rango, nuevo));
            }
            _ => {}
        }
    }
    let (titulos_antes, _) = contar_titulos_y_html(markdown);
    let salida = aplicar(markdown, ediciones, origen)?;
    let (titulos_despues, html_despues) = contar_titulos_y_html(&salida);
    if html_despues != 0 {
        return Err(ErrorWeb::dato_raro(
            origen,
            "tras escapar el HTML en bruto sigue habiendo HTML en bruto (error interno)",
        ));
    }
    if titulos_antes != titulos_despues {
        return Err(ErrorWeb::dato_raro(
            origen,
            format!(
                "escapar el HTML cambió el número de títulos ({titulos_antes} a {titulos_despues}): un bloque de HTML lleva líneas que Markdown leería como títulos"
            ),
        ));
    }
    Ok(salida)
}

// ---------------------------------------------------------------------------
// Lectura del manual (entrada no fiable)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Fuente {
    markdown: BTreeMap<String, String>,
    imagenes: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TipoDeFichero {
    Markdown,
    Imagen,
}

/// `README.md`, `^[a-z0-9-]+\.md$` o `^[a-z0-9-]+\.(png|jpg|jpeg|gif|webp)$`.
fn tipo_de_nombre(nombre: &str) -> Option<TipoDeFichero> {
    if nombre == "README.md" {
        return Some(TipoDeFichero::Markdown);
    }
    let (raiz, extension) = nombre.rsplit_once('.')?;
    if raiz.is_empty()
        || raiz.len() > 80
        || !raiz
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return None;
    }
    if extension == "md" {
        (!NOMBRES_RESERVADOS.contains(&raiz)).then_some(TipoDeFichero::Markdown)
    } else if IMAGENES.contains(&extension) {
        Some(TipoDeFichero::Imagen)
    } else {
        None
    }
}

/// ¿Parece de verdad una imagen del tipo que dice su extensión?
fn imagen_valida(nombre: &str, datos: &[u8]) -> Result<(), &'static str> {
    let extension = nombre.rsplit_once('.').map_or("", |(_, e)| e);
    let cabecera_ok = match extension {
        "png" => datos.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => datos.starts_with(&[0xFF, 0xD8, 0xFF]),
        "gif" => datos.starts_with(b"GIF87a") || datos.starts_with(b"GIF89a"),
        "webp" => datos.len() >= 12 && &datos[..4] == b"RIFF" && &datos[8..12] == b"WEBP",
        _ => false,
    };
    if cabecera_ok {
        Ok(())
    } else {
        Err("su contenido no es del tipo que dice su extensión")
    }
}

impl Fuente {
    /// Anota un fichero leído, validando su nombre y su contenido.
    fn anotar(&mut self, nombre: &str, datos: Vec<u8>, donde: &str) -> Resultado<()> {
        if nombre.eq_ignore_ascii_case("summary.md") {
            return Err(ErrorWeb::dato_raro(
                donde,
                "SUMMARY.md lo genera preparar-manual: el manual no puede traer uno",
            ));
        }
        match tipo_de_nombre(nombre) {
            Some(TipoDeFichero::Markdown) => {
                let texto = String::from_utf8(datos)
                    .map_err(|_| ErrorWeb::dato_raro(donde, "no es UTF-8 válido"))?;
                let texto = texto.strip_prefix('\u{feff}').unwrap_or(&texto).to_string();
                if texto.contains('\0') {
                    return Err(ErrorWeb::dato_raro(donde, "lleva un carácter nulo"));
                }
                // Un salto de línea que es solo CR (Mac antiguo): el algoritmo de anclas
                // lee por líneas `\n` y `\r\n` y el analizador de Markdown, también por
                // `\r`, así que el capítulo saldría entero como un único título.
                if let Some(pos) = texto
                    .match_indices('\r')
                    .find(|(i, _)| !texto[i + 1..].starts_with('\n'))
                    .map(|(i, _)| i)
                {
                    return Err(error_en(
                        donde,
                        &texto,
                        pos,
                        "salto de línea solo con CR (\\r): usa \\n o \\r\\n",
                    ));
                }
                self.markdown.insert(nombre.to_string(), texto);
            }
            Some(TipoDeFichero::Imagen) => {
                imagen_valida(nombre, &datos).map_err(|m| ErrorWeb::dato_raro(donde, m))?;
                self.imagenes.insert(nombre.to_string(), datos);
            }
            None => {
                return Err(ErrorWeb::dato_raro(
                    donde,
                    "nombre de fichero no admitido (solo README.md, [a-z0-9-]+.md e imágenes [a-z0-9-]+.png|jpg|jpeg|gif|webp)",
                ))
            }
        }
        Ok(())
    }
}

/// Un nombre de fichero listo para ir en un mensaje.
fn nombre_seguro(nombre: &str) -> String {
    texto_seguro(nombre, 80)
}

/// Lee `docs/manual/` de un árbol de trabajo: nada de enlaces simbólicos ni
/// carpetas, solo los nombres admitidos y los topes.
fn leer_arbol(carpeta: &Path) -> Resultado<Fuente> {
    let meta = fs::symlink_metadata(carpeta).map_err(|e| ErrorWeb::lectura(carpeta, e))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(ErrorWeb::dato_raro(
            carpeta.display().to_string(),
            "no es una carpeta normal (¿enlace simbólico?)",
        ));
    }
    let mut entradas: Vec<fs::DirEntry> = fs::read_dir(carpeta)
        .map_err(|e| ErrorWeb::lectura(carpeta, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| ErrorWeb::lectura(carpeta, e))?;
    entradas.sort_by_key(fs::DirEntry::file_name);
    if entradas.len() > TOPE_FICHEROS {
        return Err(ErrorWeb::dato_raro(
            carpeta.display().to_string(),
            format!("más de {TOPE_FICHEROS} ficheros"),
        ));
    }
    let mut fuente = Fuente::default();
    let mut total = 0u64;
    for entrada in entradas {
        let ruta = entrada.path();
        let nombre_os = entrada.file_name();
        let Some(nombre) = nombre_os.to_str() else {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "nombre de fichero que no es UTF-8",
            ));
        };
        let donde = format!("{}/{}", carpeta.display(), nombre_seguro(nombre));
        let meta = fs::symlink_metadata(&ruta).map_err(|e| ErrorWeb::lectura(&ruta, e))?;
        if meta.file_type().is_symlink() {
            return Err(ErrorWeb::dato_raro(
                donde,
                "es un enlace simbólico: no se admite",
            ));
        }
        if !meta.is_file() {
            return Err(ErrorWeb::dato_raro(
                donde,
                "no es un fichero normal (¿una carpeta?): docs/manual/ solo lleva ficheros",
            ));
        }
        if tipo_de_nombre(nombre).is_none() {
            return Err(ErrorWeb::dato_raro(
                donde,
                "nombre de fichero no admitido (solo README.md, [a-z0-9-]+.md e imágenes [a-z0-9-]+.png|jpg|jpeg|gif|webp)",
            ));
        }
        if meta.len() > TOPE_FICHERO {
            return Err(ErrorWeb::dato_raro(
                donde,
                format!("pesa {} bytes y el tope es {TOPE_FICHERO}", meta.len()),
            ));
        }
        total += meta.len();
        if total > TOPE_TOTAL {
            return Err(ErrorWeb::dato_raro(
                carpeta.display().to_string(),
                format!("el manual pasa del tope de {TOPE_TOTAL} bytes"),
            ));
        }
        // `take` acota la lectura aunque el fichero crezca después de mirarlo.
        let mut datos = Vec::new();
        fs::File::open(&ruta)
            .map_err(|e| ErrorWeb::lectura(&ruta, e))?
            .take(TOPE_FICHERO + 1)
            .read_to_end(&mut datos)
            .map_err(|e| ErrorWeb::lectura(&ruta, e))?;
        if datos.len() as u64 > TOPE_FICHERO {
            return Err(ErrorWeb::dato_raro(
                donde,
                format!("pasa del tope de {TOPE_FICHERO} bytes"),
            ));
        }
        fuente.anotar(nombre, datos, &donde)?;
    }
    Ok(fuente)
}

/// `^[0-9a-f]{40}$` o `^[0-9a-f]{64}$`: un identificador de objeto de git.
fn es_sha(texto: &str) -> bool {
    matches!(texto.len(), 40 | 64)
        && texto
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Una orden de git en `repositorio` (sin shell, con plazo y tope de salida).
fn git(
    repositorio: &str,
    argumentos: &[&str],
    tope: usize,
) -> Resultado<crate::ordenes::SalidaOrden> {
    let mut args: Vec<&str> = vec!["-C", repositorio];
    args.extend_from_slice(argumentos);
    ejecutar_sin_variables(
        "git",
        &args,
        &[
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("LC_ALL", "C"),
        ],
        VARIABLES_DE_GIT_QUE_DESVIAN,
        PLAZO_GIT,
        tope,
    )
}

/// Variables de entorno que hacen que `git -C <repositorio>` mire otro repositorio, otro
/// árbol o otros objetos (un gancho, un alias o `git rebase --exec` las dejan puestas).
/// Se quitan: el manual sale del repositorio y de la etiqueta que se piden, no de otros.
const VARIABLES_DE_GIT_QUE_DESVIAN: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_COMMON_DIR",
    "GIT_PREFIX",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
];

/// Lee `docs/manual/` de la etiqueta con git. Solo se aceptan ficheros normales
/// (`100644` y `100755`): un enlace simbólico, un submódulo o una carpeta se
/// rechazan nombrándolos. El contenido se lee por su identificador, el del propio
/// listado, para que el listado y los ficheros salgan siempre del mismo árbol.
fn leer_etiqueta(repositorio: &Path, etiqueta: &str) -> Resultado<Fuente> {
    let meta = fs::metadata(repositorio).map_err(|e| ErrorWeb::lectura(repositorio, e))?;
    if !meta.is_dir() {
        return Err(ErrorWeb::dato_raro(
            repositorio.display().to_string(),
            "el repositorio no es una carpeta",
        ));
    }
    let repo = repositorio.to_str().ok_or_else(|| {
        ErrorWeb::dato_raro(
            repositorio.display().to_string(),
            "la ruta del repositorio no es UTF-8",
        )
    })?;

    // Solo etiquetas (nunca una rama ni un identificador con el mismo nombre).
    let referencia = format!("refs/tags/{etiqueta}^{{commit}}");
    let resuelta = git(
        repo,
        &["rev-parse", "--verify", "--quiet", &referencia],
        4096,
    )?;
    let commit = String::from_utf8_lossy(&resuelta.salida).trim().to_string();
    if !resuelta.exito() || !es_sha(&commit) {
        return Err(ErrorWeb::dato_raro(
            format!("{repo}:{etiqueta}"),
            format!("la etiqueta «{etiqueta}» no existe en el repositorio"),
        ));
    }

    let arbol = format!("{commit}:{CARPETA_MANUAL}");
    let listado = git(repo, &["ls-tree", "-z", "-l", &arbol], TOPE_LISTADO)?;
    if !listado.exito() {
        return Err(ErrorWeb::dato_raro(
            format!("{etiqueta}:{CARPETA_MANUAL}"),
            format!(
                "la etiqueta «{etiqueta}» no trae {CARPETA_MANUAL}/ ({})",
                texto_seguro(String::from_utf8_lossy(&listado.error).trim(), 160)
            ),
        ));
    }

    let mut fuente = Fuente::default();
    let mut total = 0u64;
    let entradas: Vec<&[u8]> = listado
        .salida
        .split(|b| *b == 0)
        .filter(|e| !e.is_empty())
        .collect();
    // Antes de leer nada: demasiadas entradas es un error sin pagar sus lecturas.
    if entradas.len() > TOPE_FICHEROS {
        return Err(ErrorWeb::dato_raro(
            format!("{etiqueta}:{CARPETA_MANUAL}"),
            format!("más de {TOPE_FICHEROS} entradas"),
        ));
    }
    for entrada in entradas {
        let texto = std::str::from_utf8(entrada).map_err(|_| {
            ErrorWeb::dato_raro(
                format!("{etiqueta}:{CARPETA_MANUAL}"),
                "una entrada del listado de git tiene un nombre que no es UTF-8",
            )
        })?;
        let ilegible = || {
            ErrorWeb::dato_raro(
                format!("{etiqueta}:{CARPETA_MANUAL}"),
                format!(
                    "entrada ilegible en el listado de git: {}",
                    texto_seguro(texto, 80)
                ),
            )
        };
        let (izquierda, nombre) = texto.split_once('\t').ok_or_else(ilegible)?;
        let campos: Vec<&str> = izquierda.split_whitespace().collect();
        let [modo, tipo, sha, tamano] = campos[..] else {
            return Err(ilegible());
        };
        let donde = format!("{etiqueta}:{CARPETA_MANUAL}/{}", nombre_seguro(nombre));
        if tipo != "blob" || !matches!(modo, "100644" | "100755") {
            return Err(ErrorWeb::dato_raro(
                donde,
                format!(
                    "es una entrada {modo} {tipo}: solo se admiten ficheros normales (nada de enlaces simbólicos, submódulos ni carpetas)"
                ),
            ));
        }
        if tipo_de_nombre(nombre).is_none() {
            return Err(ErrorWeb::dato_raro(
                donde,
                "nombre de fichero no admitido (solo README.md, [a-z0-9-]+.md e imágenes [a-z0-9-]+.png|jpg|jpeg|gif|webp)",
            ));
        }
        if !es_sha(sha) {
            return Err(ilegible());
        }
        let tamano: u64 = tamano.parse().map_err(|_| ilegible())?;
        if tamano > TOPE_FICHERO {
            return Err(ErrorWeb::dato_raro(
                donde,
                format!("pesa {tamano} bytes y el tope es {TOPE_FICHERO}"),
            ));
        }
        total += tamano;
        if total > TOPE_TOTAL {
            return Err(ErrorWeb::dato_raro(
                format!("{etiqueta}:{CARPETA_MANUAL}"),
                format!("el manual pasa del tope de {TOPE_TOTAL} bytes"),
            ));
        }
        let contenido = git(
            repo,
            &["cat-file", "blob", sha],
            (TOPE_FICHERO as usize) + 4096,
        )?;
        if !contenido.exito() || contenido.salida.len() as u64 != tamano {
            return Err(ErrorWeb::dato_raro(
                donde,
                "git no devolvió el fichero con el tamaño que anunció",
            ));
        }
        fuente.anotar(nombre, contenido.salida, &donde)?;
    }
    Ok(fuente)
}

// ---------------------------------------------------------------------------
// Escritura (en un directorio de trabajo y renombrado al final)
// ---------------------------------------------------------------------------

fn escribir_nuevo(ruta: &Path, datos: &[u8]) -> Resultado<()> {
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(ruta)
        .map_err(|e| ErrorWeb::escritura(ruta, e))?;
    f.write_all(datos).map_err(|e| ErrorWeb::escritura(ruta, e))
}

/// Lee un fichero normal del tema, sin seguir enlaces y con tope.
fn leer_del_tema(ruta: &Path, tope: u64) -> Resultado<Vec<u8>> {
    let meta = fs::symlink_metadata(ruta).map_err(|e| ErrorWeb::lectura(ruta, e))?;
    if meta.file_type().is_symlink() {
        return Err(ErrorWeb::dato_raro(
            ruta.display().to_string(),
            "es un enlace simbólico: no se copia",
        ));
    }
    if !meta.is_file() {
        return Err(ErrorWeb::dato_raro(
            ruta.display().to_string(),
            "no es un fichero normal",
        ));
    }
    if meta.len() > tope {
        return Err(ErrorWeb::dato_raro(
            ruta.display().to_string(),
            format!("pesa {} bytes y el tope es {tope}", meta.len()),
        ));
    }
    let mut datos = Vec::new();
    fs::File::open(ruta)
        .map_err(|e| ErrorWeb::lectura(ruta, e))?
        .take(tope + 1)
        .read_to_end(&mut datos)
        .map_err(|e| ErrorWeb::lectura(ruta, e))?;
    if datos.len() as u64 > tope {
        return Err(ErrorWeb::dato_raro(
            ruta.display().to_string(),
            format!("pasa del tope de {tope} bytes"),
        ));
    }
    Ok(datos)
}

/// Copia `theme/` (y todo lo que cuelga) sin seguir enlaces simbólicos.
fn copiar_tema_carpeta(
    origen: &Path,
    destino: &Path,
    profundidad: usize,
    cuenta: &mut usize,
    total: &mut u64,
) -> Resultado<()> {
    if profundidad > PROFUNDIDAD_TEMA {
        return Err(ErrorWeb::dato_raro(
            origen.display().to_string(),
            "demasiados niveles de carpetas en el tema",
        ));
    }
    let meta = fs::symlink_metadata(origen).map_err(|e| ErrorWeb::lectura(origen, e))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(ErrorWeb::dato_raro(
            origen.display().to_string(),
            "no es una carpeta normal (¿enlace simbólico?)",
        ));
    }
    fs::create_dir_all(destino).map_err(|e| ErrorWeb::escritura(destino, e))?;
    let mut entradas: Vec<fs::DirEntry> = fs::read_dir(origen)
        .map_err(|e| ErrorWeb::lectura(origen, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| ErrorWeb::lectura(origen, e))?;
    entradas.sort_by_key(fs::DirEntry::file_name);
    for entrada in entradas {
        let ruta = entrada.path();
        let nombre = entrada.file_name().to_string_lossy().into_owned();
        if nombre.is_empty()
            || nombre == "."
            || nombre.contains("..")
            || nombre.chars().any(|c| c.is_control() || c == '\\')
        {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "nombre de fichero del tema no admitido",
            ));
        }
        let meta = fs::symlink_metadata(&ruta).map_err(|e| ErrorWeb::lectura(&ruta, e))?;
        if meta.file_type().is_symlink() {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "es un enlace simbólico: no se copia",
            ));
        }
        if meta.is_dir() {
            copiar_tema_carpeta(
                &ruta,
                &destino.join(&nombre),
                profundidad + 1,
                cuenta,
                total,
            )?;
        } else if meta.is_file() {
            *cuenta += 1;
            if *cuenta > TOPE_FICHEROS_TEMA {
                return Err(ErrorWeb::dato_raro(
                    origen.display().to_string(),
                    format!("más de {TOPE_FICHEROS_TEMA} ficheros en el tema"),
                ));
            }
            let datos = leer_del_tema(&ruta, TOPE_FICHERO_TEMA)?;
            *total += datos.len() as u64;
            if *total > TOPE_TOTAL_TEMA {
                return Err(ErrorWeb::dato_raro(
                    origen.display().to_string(),
                    format!("el tema pasa del tope de {TOPE_TOTAL_TEMA} bytes"),
                ));
            }
            escribir_nuevo(&destino.join(&nombre), &datos)?;
        } else {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "ni carpeta ni fichero normal",
            ));
        }
    }
    Ok(())
}

/// Copia `book.toml` (obligatorio) y `theme/` (si existe) del tema.
fn copiar_tema(tema: &Path, destino: &Path) -> Resultado<()> {
    let meta = fs::symlink_metadata(tema).map_err(|e| ErrorWeb::lectura(tema, e))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(ErrorWeb::dato_raro(
            tema.display().to_string(),
            "el tema no es una carpeta normal (¿enlace simbólico?)",
        ));
    }
    let libro = tema.join("book.toml");
    if fs::symlink_metadata(&libro).is_err() {
        return Err(ErrorWeb::dato_raro(
            tema.display().to_string(),
            "el tema no tiene book.toml",
        ));
    }
    let datos = leer_del_tema(&libro, TOPE_LIBRO_TOML)?;
    escribir_nuevo(&destino.join("book.toml"), &datos)?;

    let tema_theme = tema.join("theme");
    if fs::symlink_metadata(&tema_theme).is_ok() {
        let mut cuenta = 0usize;
        let mut total = 0u64;
        copiar_tema_carpeta(
            &tema_theme,
            &destino.join("theme"),
            0,
            &mut cuenta,
            &mut total,
        )?;
    }
    Ok(())
}

/// Las entradas contra las que no puede solaparse la salida.
struct Entradas {
    tema: PathBuf,
    /// La carpeta `docs/manual/` de un árbol de trabajo.
    carpeta: Option<PathBuf>,
    /// El repositorio del que se lee la etiqueta.
    repositorio: Option<PathBuf>,
}

/// Comprueba la carpeta de salida y la deja absoluta y sin solaparse con ninguna
/// entrada: prepararla sustituiría (al renombrar) lo que no es suyo. Una carpeta de
/// salida que ya existe tiene que estar vacía o ser un libro preparado antes
/// (`book.toml` y `src/SUMMARY.md`): `--salida docs` no puede borrar `docs/`.
fn validar_salida(salida: &Path, entradas: &Entradas) -> Resultado<PathBuf> {
    if salida
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(ErrorWeb::dato_raro("--salida", "no puede llevar «..»"));
    }
    let nombre = salida
        .file_name()
        .ok_or_else(|| ErrorWeb::dato_raro("--salida", "no tiene nombre de carpeta"))?
        .to_os_string();
    let madre = match salida.parent() {
        Some(m) if !m.as_os_str().is_empty() => m.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&madre).map_err(|e| ErrorWeb::escritura(&madre, e))?;
    let madre = fs::canonicalize(&madre).map_err(|e| ErrorWeb::lectura(&madre, e))?;
    let salida = madre.join(nombre);

    let mut cruces: Vec<(&str, PathBuf)> = vec![("--tema", entradas.tema.clone())];
    if let Some(c) = &entradas.carpeta {
        cruces.push(("--arbol", c.clone()));
    }
    for (que, ruta) in cruces {
        let ruta = fs::canonicalize(&ruta).map_err(|e| ErrorWeb::lectura(&ruta, e))?;
        if ruta.starts_with(&salida) || salida.starts_with(&ruta) {
            return Err(ErrorWeb::dato_raro(
                "--salida",
                format!(
                    "se solapa con {que} ({}): preparar-manual sustituiría lo que no es suyo",
                    ruta.display()
                ),
            ));
        }
    }
    if let Some(repositorio) = &entradas.repositorio {
        let repositorio =
            fs::canonicalize(repositorio).map_err(|e| ErrorWeb::lectura(repositorio, e))?;
        // Dentro del repositorio sí (`target/web/libro`); él mismo o una carpeta que
        // lo contiene, no. Y nunca dentro de `.git`.
        if repositorio.starts_with(&salida) || salida.starts_with(repositorio.join(".git")) {
            return Err(ErrorWeb::dato_raro(
                "--salida",
                format!(
                    "se solapa con el repositorio ({}): preparar-manual sustituiría lo que no es suyo",
                    repositorio.display()
                ),
            ));
        }
    }
    if let Ok(meta) = fs::symlink_metadata(&salida) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(ErrorWeb::dato_raro(
                salida.display().to_string(),
                "ya existe y no es una carpeta normal",
            ));
        }
        let vacia = fs::read_dir(&salida)
            .map_err(|e| ErrorWeb::lectura(&salida, e))?
            .next()
            .is_none();
        let es_libro = fs::symlink_metadata(salida.join("book.toml")).is_ok_and(|m| m.is_file())
            && fs::symlink_metadata(salida.join("src/SUMMARY.md")).is_ok_and(|m| m.is_file());
        if !vacia && !es_libro {
            return Err(ErrorWeb::dato_raro(
                salida.display().to_string(),
                "ya existe, no está vacía y no es un libro preparado antes (sin book.toml y src/SUMMARY.md): preparar-manual la sustituiría entera",
            ));
        }
    }

    Ok(salida)
}

/// Sustituye `salida` por `trabajo`. Si algo falla, deja la anterior como estaba.
fn sustituir(trabajo: &Path, salida: &Path, padre: &Path, nombre: &str, pid: u32) -> Resultado<()> {
    if fs::symlink_metadata(salida).is_err() {
        return fs::rename(trabajo, salida).map_err(|e| ErrorWeb::escritura(salida, e));
    }
    let anterior = padre.join(format!(".{nombre}.anterior-{pid}"));
    if anterior.exists() {
        fs::remove_dir_all(&anterior).map_err(|e| ErrorWeb::escritura(&anterior, e))?;
    }
    fs::rename(salida, &anterior).map_err(|e| ErrorWeb::escritura(salida, e))?;
    if let Err(e) = fs::rename(trabajo, salida) {
        // Se devuelve la anterior a su sitio.
        let _ = fs::rename(&anterior, salida);
        return Err(ErrorWeb::escritura(salida, e));
    }
    let _ = fs::remove_dir_all(&anterior);
    Ok(())
}

/// Todo el trabajo sobre el manual leído: `SUMMARY.md`, los capítulos adaptados y
/// las comprobaciones. No toca el disco.
struct Preparado {
    capitulos: Vec<String>,
    summary: String,
    paginas: BTreeMap<String, String>,
}

fn preparar_en_memoria(fuente: &Fuente, etiqueta: &str) -> Resultado<Preparado> {
    let readme = fuente
        .markdown
        .get("README.md")
        .ok_or_else(|| ErrorWeb::dato_raro(CARPETA_MANUAL, "falta README.md"))?;
    let nombres: Vec<String> = fuente.markdown.keys().cloned().collect();
    let indice = capitulos_del_indice(readme, &nombres)?;
    let capitulos: Vec<String> = indice.iter().map(|(_, f)| (*f).to_string()).collect();
    let summary = summary(readme, &nombres)?;

    let mut paginas = BTreeMap::new();
    for (nombre, texto) in &fuente.markdown {
        let origen = format!("{CARPETA_MANUAL}/{nombre}");
        rechazar_preprocesadores(texto, &origen)?;
        rechazar_tablas_alineadas(texto, &origen)?;
        rechazar_menor_que_en_titulos(texto, &origen)?;
        let con_ids = con_anclas(texto, &origen)?;
        let (enlazado, locales) = reescribir(&con_ids, nombre, etiqueta, &capitulos, &origen)?;
        for (imagen, linea) in locales {
            if !fuente.imagenes.contains_key(&imagen) {
                return Err(ErrorWeb::dato_raro(
                    format!("{origen}:{linea}"),
                    format!(
                        "la imagen «{}» no está en docs/manual/ (solo se copian las imágenes sueltas de esa carpeta)",
                        texto_seguro(&imagen, 80)
                    ),
                ));
            }
        }
        let limpio = escapar_html_suelto(&enlazado, &origen)?;
        paginas.insert(nombre.clone(), limpio);
    }
    Ok(Preparado {
        capitulos,
        summary,
        paginas,
    })
}

/// Escribe el libro en `trabajo` y devuelve los ficheros de `src/`.
fn escribir_libro(
    trabajo: &Path,
    tema: &Path,
    preparado: &Preparado,
    imagenes: &BTreeMap<String, Vec<u8>>,
) -> Resultado<Vec<String>> {
    let src = trabajo.join("src");
    fs::create_dir_all(&src).map_err(|e| ErrorWeb::escritura(&src, e))?;
    let mut escritos = vec!["SUMMARY.md".to_string()];
    escribir_nuevo(&src.join("SUMMARY.md"), preparado.summary.as_bytes())?;
    for (nombre, texto) in &preparado.paginas {
        escribir_nuevo(&src.join(nombre), texto.as_bytes())?;
        escritos.push(nombre.clone());
    }
    for (nombre, datos) in imagenes {
        escribir_nuevo(&src.join(nombre), datos)?;
        escritos.push(nombre.clone());
    }
    copiar_tema(tema, trabajo)?;
    escritos.sort();
    Ok(escritos)
}

/// El flujo entero: lee, adapta y escribe en `o.salida`.
///
/// Todo se lee y se adapta en memoria antes de escribir nada; se escribe en una
/// carpeta hermana de la salida (`.<salida>.preparando-<pid>`) y solo al final se
/// renombra. Si algo falla, la salida anterior queda como estaba.
pub fn preparar_manual(o: &OpcionesManual) -> Resultado<InformeManual> {
    validar_etiqueta(&o.etiqueta)?;
    let (carpeta, repositorio) = match &o.origen {
        OrigenManual::Arbol { carpeta } => (Some(carpeta.clone()), None),
        OrigenManual::Etiqueta { repositorio } => (None, Some(repositorio.clone())),
    };
    let salida = validar_salida(
        &o.salida,
        &Entradas {
            tema: o.tema.clone(),
            carpeta,
            repositorio,
        },
    )?;

    let fuente = match &o.origen {
        OrigenManual::Arbol { carpeta } => leer_arbol(carpeta)?,
        OrigenManual::Etiqueta { repositorio } => leer_etiqueta(repositorio, &o.etiqueta)?,
    };
    let preparado = preparar_en_memoria(&fuente, &o.etiqueta)?;

    let nombre = salida
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| ErrorWeb::dato_raro("--salida", "no tiene nombre de carpeta"))?;
    let padre = salida
        .parent()
        .ok_or_else(|| ErrorWeb::dato_raro("--salida", "no tiene carpeta madre"))?
        .to_path_buf();
    let pid = std::process::id();
    let trabajo = padre.join(format!(".{nombre}.preparando-{pid}"));
    if fs::symlink_metadata(&trabajo).is_ok() {
        fs::remove_dir_all(&trabajo).map_err(|e| ErrorWeb::escritura(&trabajo, e))?;
    }
    fs::create_dir(&trabajo).map_err(|e| ErrorWeb::escritura(&trabajo, e))?;

    let ficheros = match escribir_libro(&trabajo, &o.tema, &preparado, &fuente.imagenes) {
        Ok(f) => f,
        Err(e) => {
            let _ = fs::remove_dir_all(&trabajo);
            return Err(e);
        }
    };
    if let Err(e) = sustituir(&trabajo, &salida, &padre, &nombre, pid) {
        let _ = fs::remove_dir_all(&trabajo);
        return Err(e);
    }
    Ok(InformeManual {
        capitulos: preparado.capitulos,
        ficheros,
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// El texto de la región que localiza `destino_en_linea`.
    fn region_en_linea(fuente: &str, imagen: bool) -> Option<String> {
        destino_en_linea(fuente, imagen).map(|r| fuente[r.inicio..r.fin].to_string())
    }

    #[test]
    fn el_indice_toma_el_titulo_mas_corto_y_solo_ficheros_de_capitulo() {
        assert_eq!(
            entrada_del_indice("- [Uno](uno.md) — texto con [otro](otro.md)"),
            Some(("Uno", "uno.md"))
        );
        assert_eq!(
            entrada_del_indice("- [El `Include` de `~/.ssh/config`](ssh-config.md)"),
            Some(("El `Include` de `~/.ssh/config`", "ssh-config.md"))
        );
        // Con corchetes y paréntesis dentro del título.
        assert_eq!(
            entrada_del_indice("- [A [b] (c)](a-1.md)"),
            Some(("A [b] (c)", "a-1.md"))
        );
        // Con otro enlace dentro del título es ambiguo.
        assert_eq!(
            entrada_del_indice("- [A](https://ejemplo.org) y [B](b.md)"),
            None
        );
        for no in [
            "- [Uno](https://ejemplo.org/uno.md)",
            "- [Uno](Uno.md)",
            "- [Uno](uno_dos.md)",
            "- [Uno](.md)",
            "- [Uno](uno.md",
            "- [](uno.md)",
            "- Uno](uno.md)",
            "* [Uno](uno.md)",
            "  - [Uno](uno.md)",
            "-[Uno](uno.md)",
            "- [Uno](uno.md.bak)",
            "- [Uno](a/uno.md)",
        ] {
            assert_eq!(entrada_del_indice(no), None, "{no}");
        }
    }

    #[test]
    fn localiza_el_destino_de_un_enlace_en_linea() {
        let casos: &[(&str, bool, Option<&str>)] = &[
            ("[a](b.md)", false, Some("b.md")),
            ("[a]( b.md )", false, Some("b.md")),
            ("[a](b.md \"título (x)\")", false, Some("b.md")),
            ("[a](b.md 'título')", false, Some("b.md")),
            ("[a](b.md (título))", false, Some("b.md")),
            ("[a](<b c.md> \"t\")", false, Some("<b c.md>")),
            ("[a](<b)c.md>)", false, Some("<b)c.md>")),
            ("[a](b(c)d.md)", false, Some("b(c)d.md")),
            ("[a](b\\(c.md)", false, Some("b\\(c.md")),
            ("[a](\nb.md\n\"t\"\n)", false, Some("b.md")),
            ("[a [b] c](d.md)", false, Some("d.md")),
            ("[a `]` c](d.md)", false, Some("d.md")),
            ("[a ``x`]`` c](d.md)", false, Some("d.md")),
            ("[a\\]](d.md)", false, Some("d.md")),
            ("[![i](p.png)](q.md)", false, Some("q.md")),
            ("![alt [x]](p.png)", true, Some("p.png")),
            ("![](p.png)", true, Some("p.png")),
            ("[](b.md)", false, Some("b.md")),
            ("[a]()", false, Some("")),
            ("[a](<>)", false, Some("<>")),
            ("[a]( )", false, Some("")),
            // Lo que no es un enlace en línea bien formado.
            ("[a](b.md", false, None),
            ("[a](b.md x)", false, None),
            ("[a](b.md \"t\" x)", false, None),
            ("[a](<b.md)", false, None),
            ("[a]b.md)", false, None),
            ("a](b.md)", false, None),
            ("[a](b(.md)", false, None),
        ];
        for (fuente, imagen, esperado) in casos {
            assert_eq!(
                region_en_linea(fuente, *imagen).as_deref(),
                *esperado,
                "{fuente:?}"
            );
        }
    }

    #[test]
    fn localiza_el_destino_de_una_definicion() {
        let region = |fuente: &str| {
            destino_en_definicion(fuente).map(|r| fuente[r.inicio..r.fin].to_string())
        };
        assert_eq!(region("[r]: b.md").as_deref(), Some("b.md"));
        assert_eq!(region("   [r]:   b.md  \"t\"").as_deref(), Some("b.md"));
        assert_eq!(
            region("[r]: <b c.md> 'título'").as_deref(),
            Some("<b c.md>")
        );
        assert_eq!(region("[r]:\n   b.md\n   \"t\"").as_deref(), Some("b.md"));
        assert_eq!(region("[a\\]b]: c.md").as_deref(), Some("c.md"));
        assert_eq!(region("[r]: <>").as_deref(), Some("<>"));
        assert_eq!(region("[r] b.md"), None);
        assert_eq!(region("[a[b]]: c.md"), None);
        assert_eq!(region("[r]: b.md x"), None);
        assert_eq!(region("    [r]: b.md"), None);
    }

    #[test]
    fn el_destino_descodificado_sigue_las_reglas_de_markdown() {
        assert_eq!(descodificar_region("b.md", false).as_deref(), Some("b.md"));
        assert_eq!(
            descodificar_region("<a b.md>", true).as_deref(),
            Some("a b.md")
        );
        assert_eq!(
            descodificar_region("a\\(b\\).md", false).as_deref(),
            Some("a(b).md")
        );
        // Una barra seguida de una letra es una barra.
        assert_eq!(descodificar_region("a\\b", false).as_deref(), Some("a\\b"));
        // Con entidades no se comprueba.
        assert_eq!(descodificar_region("a&amp;b", false), None);
    }

    #[test]
    fn escapa_menores_y_quita_comentarios() {
        assert_eq!(escapar_trozo("<host>", false).unwrap(), "\\<host>");
        assert_eq!(escapar_trozo("<a><b>", false).unwrap(), "\\<a>\\<b>");
        assert_eq!(escapar_trozo("<!-- x -->", false).unwrap(), "");
        assert_eq!(escapar_trozo("<!-->", false).unwrap(), "");
        assert_eq!(escapar_trozo("<!--->", false).unwrap(), "");
        assert_eq!(escapar_trozo("<!---->", false).unwrap(), "");
        assert_eq!(
            escapar_trozo("a <!-- <b> --> c <d>", false).unwrap(),
            "a  c \\<d>"
        );
        assert_eq!(
            escapar_trozo("<!-- a\nb --> resto <i>\n", true).unwrap(),
            " resto \\<i>\n"
        );
        // Ya escapado: se deja; con la barra escapada, no.
        assert_eq!(escapar_trozo("x \\<b> y", true).unwrap(), "x \\<b> y");
        assert_eq!(escapar_trozo("x \\\\<b> y", true).unwrap(), "x \\\\\\<b> y");
        assert!(escapar_trozo("<!-- sin cerrar", true).is_err());
        assert!(escapar_trozo("<!-- sin cerrar", false).is_err());
    }

    #[test]
    fn codifica_rutas_consultas_y_fragmentos() {
        assert_eq!(
            codificar_ruta("docs/túnel (1).md"),
            "docs/t%C3%BAnel%20%281%29.md"
        );
        assert_eq!(codificar_ruta("a+b@c~d_e-f.g"), "a%2Bb%40c~d_e-f.g");
        assert_eq!(codificar_resto("a=1&b=2"), "a=1&amp;b=2");
        assert_eq!(codificar_resto("a b"), "a%20b");
        assert_eq!(codificar_resto("t%C3%BAnel"), "t%C3%BAnel");
        assert_eq!(codificar_resto("100%"), "100%25");
        assert_eq!(codificar_resto("100%z"), "100%25z");
        assert_eq!(codificar_resto("túnel"), "t%C3%BAnel");
        assert_eq!(
            codificar_resto("a(b)<c>\"d\"`e`"),
            "a%28b%29%3Cc%3E%22d%22%60e%60"
        );
        assert_eq!(sufijo(Some("q=1"), Some("a")), "?q=1#a");
        assert_eq!(sufijo(Some(""), Some("")), "");
        assert_eq!(sufijo(None, Some("x y")), "#x%20y");
    }

    #[test]
    fn descodifica_solo_rutas_sin_trampas() {
        assert_eq!(descodificar_ruta("a%20b").as_deref(), Some("a b"));
        assert_eq!(descodificar_ruta("t%C3%BAnel").as_deref(), Some("túnel"));
        assert_eq!(descodificar_ruta("100%25").as_deref(), Some("100%"));
        for mala in [
            "a%2Fb", "a%2fb", "a%5Cb", "a%00b", "a%0Ab", "a%zz", "a%2", "a%", "a%C3",
        ] {
            assert_eq!(descodificar_ruta(mala), None, "{mala}");
        }
    }

    #[test]
    fn la_etiqueta_es_una_version_publicada() {
        for buena in ["v1.6.1", "v0.0.0", "v10.20.30"] {
            assert!(etiqueta_valida(buena), "{buena}");
        }
        for mala in [
            "",
            "1.6.1",
            "v1.6",
            "v1.6.1.2",
            "v1.6.1-rc.1",
            "v01.6.1",
            "V1.6.1",
            "android-v0.2.0",
            "v1.6.1 ",
            "v1.6.1\n",
            "v1.6.1/../x",
            "v-1.0.0",
            "v1..1",
            "v1.6.1000000000",
        ] {
            assert!(!etiqueta_valida(mala), "{mala:?}");
        }
    }

    #[test]
    fn los_nombres_del_manual_son_solo_los_admitidos() {
        use TipoDeFichero::{Imagen, Markdown};
        for (nombre, tipo) in [
            ("README.md", Markdown),
            ("estado.md", Markdown),
            ("flota-identidades-y-registro.md", Markdown),
            ("a1.md", Markdown),
            ("captura-1.png", Imagen),
            ("foto.jpg", Imagen),
            ("foto.jpeg", Imagen),
            ("anim.gif", Imagen),
            ("foto.webp", Imagen),
        ] {
            assert_eq!(tipo_de_nombre(nombre), Some(tipo), "{nombre}");
        }
        for malo in [
            "",
            "logo.svg",
            "index.md",
            "readme.md",
            "toc.md",
            "print.md",
            ".md",
            "Notas.md",
            "a_b.md",
            "a b.md",
            "ñ.md",
            "x.sh",
            "x.md.bak",
            "x.MD",
            "README.MD",
            "readme.md.orig",
            ".oculto.md",
            "a.PNG",
            "a.bmp",
            "a/b.md",
            "a\nb.md",
            "x.md~",
        ] {
            assert_eq!(tipo_de_nombre(malo), None, "{malo:?}");
        }
    }

    #[test]
    fn una_imagen_tiene_que_ser_lo_que_dice() {
        assert!(imagen_valida("a.png", b"\x89PNG\r\n\x1a\n...").is_ok());
        assert!(imagen_valida("a.png", b"GIF89a").is_err());
        assert!(imagen_valida("a.jpg", &[0xFF, 0xD8, 0xFF, 0xE0]).is_ok());
        assert!(imagen_valida("a.gif", b"GIF87a..").is_ok());
        assert!(imagen_valida("a.webp", b"RIFF\0\0\0\0WEBPVP8 ").is_ok());
        assert!(imagen_valida("a.webp", b"RIFF\0\0\0\0WAVE").is_err());
        assert!(imagen_valida("a.png", b"").is_err());
        // Un SVG no es una imagen admitida: ni siquiera pasa por aquí (ver `tipo_de_nombre`).
        assert!(imagen_valida("a.svg", b"<svg xmlns='x'><rect/></svg>").is_err());
    }

    #[test]
    fn el_esquema_solo_cuenta_antes_de_la_primera_barra() {
        assert_eq!(esquema_de("https://x"), Ok(Some("https")));
        assert_eq!(esquema_de("mailto:a@b"), Ok(Some("mailto")));
        assert_eq!(esquema_de("a/b:c"), Ok(None));
        assert_eq!(esquema_de("a.md#x:y"), Ok(None));
        assert_eq!(esquema_de("a.md?x=y:z"), Ok(None));
        assert!(esquema_de("1a:b").is_err());
        assert!(esquema_de(":x").is_err());
        assert!(esquema_de("a b:c").is_err());
    }

    #[test]
    fn las_ediciones_se_aplican_por_posicion_y_no_se_solapan() {
        let r = aplicar("abcdef", vec![(4..5, "X".into()), (1..2, "YY".into())], "t").unwrap();
        assert_eq!(r, "aYYcdXf");
        assert!(aplicar("abc", vec![(0..2, "x".into()), (1..3, "y".into())], "t").is_err());
        assert!(aplicar("abc", vec![(2..9, "x".into())], "t").is_err());
        // En medio de un carácter de varios bytes.
        assert!(aplicar("ñ", vec![(1..2, "x".into())], "t").is_err());
        assert_eq!(aplicar("abc", Vec::new(), "t").unwrap(), "abc");
    }
}
