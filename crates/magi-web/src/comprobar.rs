//! `magi-web comprobar <dir>`: revisa el sitio generado (informe §5.3 y §7.11).
//!
//! Comprueba, en cada página `.html`:
//! - enlaces internos y anclas: todo `href`/`src` relativo resuelve a un fichero
//!   del sitio y toda ancla existe como `id` en su destino;
//! - enlaces externos: solo `https://` y solo de la lista permitida;
//! - ningún atributo `style`, y ningún `<script>` propio en las páginas que no son
//!   del manual;
//! - CSP presente al principio de `<head>`, con el hash de cada script en línea y
//!   sin `'unsafe-inline'`;
//! - ni `<base>` con `href` ni `<meta http-equiv="refresh">`.
//!
//! Falla cerrado ante HTML que no puede interpretar.
//!
//! Además, por la misma razón que la CSP (nada se carga de fuera y nada se
//! ejecuta sin que se sepa): ni `<style>` en línea, ni manejadores `on…` en
//! ninguna página, ni recursos (imágenes, hojas de estilo, scripts, marcos)
//! cargados de otro dominio que el propio, ni enlaces simbólicos en el sitio.
//! Todo lo que se cuela en un mensaje desde el HTML se escapa: un nombre de
//! atributo con un salto de línea no puede inventarse una línea del registro.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::csp;
use crate::enlaces::{self, Destino};
use crate::error::{ErrorWeb, Hallazgo, Resultado};
use crate::html::{self, Etiqueta, Token};
use crate::rutas::RUTA_MANUAL;

/// Dominios (y prefijos de ruta) a los que puede enlazar la web.
#[derive(Debug, Clone)]
pub struct ListaPermitida {
    /// `(host, prefijos de ruta permitidos)`; un prefijo vacío permite todo el host.
    pub dominios: Vec<(String, Vec<String>)>,
}

/// El host de la propia web: sus recursos absolutos (el 404) son suyos.
const HOST_PROPIO: &str = "magi.erratia.com";

/// Tope de lo que se lee de una página (una página de la web pesa kilobytes).
const MAX_PAGINA: u64 = 32 * 1024 * 1024;

/// Tope de anidamiento de carpetas del sitio.
const MAX_PROFUNDIDAD: usize = 32;

impl ListaPermitida {
    /// La del informe §5.3: `github.com` con `/HectorEspejo/magi`,
    /// `/ImranR98/Obtainium` y `/termux-user-repository/`; `cli.github.com`;
    /// `termux.dev`; `keepachangelog.com`; `semver.org`; `magi.erratia.com`; y
    /// `docs.github.com` (enlace de privacidad del pie). Un dominio nuevo entra
    /// aquí en el mismo PR que lo enlaza.
    pub fn por_defecto() -> Self {
        let todo = || vec![String::new()];
        Self {
            dominios: vec![
                (
                    "github.com".to_string(),
                    vec![
                        "/HectorEspejo/magi".to_string(),
                        "/ImranR98/Obtainium".to_string(),
                        "/termux-user-repository/".to_string(),
                    ],
                ),
                ("cli.github.com".to_string(), todo()),
                ("docs.github.com".to_string(), todo()),
                ("termux.dev".to_string(), todo()),
                ("keepachangelog.com".to_string(), todo()),
                ("semver.org".to_string(), todo()),
                (HOST_PROPIO.to_string(), todo()),
            ],
        }
    }

    /// `true` si `url` es `https://`, su host es exactamente uno de la lista (en
    /// minúsculas, sin usuario ni puerto raro) y su ruta cuelga de uno de sus
    /// prefijos, comparados por segmentos: `/HectorEspejo/magi-evil` no cuelga de
    /// `/HectorEspejo/magi`. Una ruta con `.` o `..`, con `%2e`, `%2f` o `%5c`, o
    /// con caracteres de control, blancos o `\` no se permite.
    pub fn permite(&self, url: &str) -> bool {
        let Some(analizada) = analizar_https(url) else {
            return false;
        };
        let segmentos_url: Vec<&str> = analizada
            .ruta
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        self.dominios
            .iter()
            .filter(|(host, _)| *host == analizada.host)
            .any(|(_, prefijos)| {
                prefijos.iter().any(|prefijo| {
                    let segmentos: Vec<&str> =
                        prefijo.split('/').filter(|s| !s.is_empty()).collect();
                    segmentos.len() <= segmentos_url.len()
                        && segmentos.iter().zip(&segmentos_url).all(|(a, b)| a == b)
                })
            })
    }
}

/// Una URL `https://` bien formada, en lo que importa para la lista.
pub(crate) struct UrlHttps<'a> {
    pub host: &'a str,
    /// La ruta, sin `?` ni `#`, siempre con `/` inicial.
    pub ruta: &'a str,
}

/// Analiza `url` o devuelve `None` si no es `https://` con un host claro: sin
/// usuario, solo el puerto 443, host en minúsculas, y ruta sin `.`/`..`,
/// segmentos vacíos intermedios ni escapes que cambien su sentido.
pub(crate) fn analizar_https(url: &str) -> Option<UrlHttps<'_>> {
    let resto = url.strip_prefix("https://")?;
    if resto.chars().any(|c| {
        c.is_control()
            || c.is_whitespace()
            || !c.is_ascii()
            || matches!(c, '\\' | '"' | '<' | '>' | '`' | '^' | '{' | '}' | '|')
    }) {
        return None;
    }
    let fin_autoridad = resto.find(['/', '?', '#']).unwrap_or(resto.len());
    let autoridad = &resto[..fin_autoridad];
    let host = match autoridad.split_once(':') {
        Some((host, "443")) => host,
        Some(_) => return None,
        None => autoridad,
    };
    let host_valido = !host.is_empty()
        && host.split('.').all(|etiqueta| {
            !etiqueta.is_empty()
                && !etiqueta.starts_with('-')
                && !etiqueta.ends_with('-')
                && etiqueta
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    if !host_valido {
        return None;
    }

    let cola = &resto[fin_autoridad..];
    let ruta = cola.split(['?', '#']).next().unwrap_or("");
    let ruta = if ruta.is_empty() { "/" } else { ruta };

    // Escapes: `%2e`, `%2f`, `%5c` y los de control cambian el sentido de la ruta.
    let b = ruta.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let (alto, bajo) = (*b.get(i + 1)?, *b.get(i + 2)?);
            if !alto.is_ascii_hexdigit() || !bajo.is_ascii_hexdigit() {
                return None;
            }
            let valor = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).ok()?, 16).ok()?;
            if matches!(valor, b'.' | b'/' | b'\\') || valor < 0x20 || valor == 0x7f {
                return None;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    let segmentos: Vec<&str> = ruta[1..].split('/').collect();
    let ultimo = segmentos.len() - 1;
    for (i, segmento) in segmentos.iter().enumerate() {
        if *segmento == "." || *segmento == ".." || (segmento.is_empty() && i != ultimo) {
            return None;
        }
    }
    Some(UrlHttps { host, ruta })
}

/// Una página `.html` leída.
struct Pagina {
    rel: String,
    es_manual: bool,
    html: String,
    /// `None` si no se pudo leer o interpretar (ya hay un hallazgo).
    tokens: Option<Vec<Token>>,
    /// Los `id` (y `name` de `<a>`) de la página; `None` si no se interpretó.
    ids: Option<HashSet<String>>,
}

/// Todo el sitio, para resolver enlaces.
struct Sitio {
    ficheros: HashSet<String>,
    paginas: BTreeMap<String, Pagina>,
}

impl Sitio {
    /// El fichero al que apunta una ruta del sitio: ella misma o, si es una
    /// carpeta, su `index.html`.
    fn destino_de(&self, ruta: &str) -> Option<String> {
        if self.ficheros.contains(ruta) {
            return Some(ruta.to_string());
        }
        let indice = format!("{}/index.html", ruta.trim_end_matches('/'));
        self.ficheros.contains(&indice).then_some(indice)
    }
}

/// Un trozo de texto del HTML, apto para un mensaje: sin caracteres de control
/// (un salto de línea inventaría líneas del registro) y acortado.
fn visible(texto: &str) -> String {
    let mut salida = String::new();
    for (i, c) in texto.chars().enumerate() {
        if i >= 120 {
            salida.push('…');
            break;
        }
        if c.is_control() {
            salida.extend(c.escape_default());
        } else {
            salida.push(c);
        }
    }
    salida
}

fn es_pagina(rel: &str) -> bool {
    let minusculas = rel.to_ascii_lowercase();
    [".html", ".htm", ".xhtml"]
        .iter()
        .any(|ext| minusculas.ends_with(ext))
}

fn es_marcado_con_scripts(rel: &str) -> bool {
    let minusculas = rel.to_ascii_lowercase();
    minusculas.ends_with(".svg") || minusculas.ends_with(".xml")
}

fn ruta_en(dir: &Path, rel: &str) -> PathBuf {
    let mut ruta = dir.to_path_buf();
    for parte in rel.split('/') {
        ruta.push(parte);
    }
    ruta
}

fn recorrer(
    dir: &Path,
    prefijo: &str,
    profundidad: usize,
    ficheros: &mut Vec<String>,
    hallazgos: &mut Vec<Hallazgo>,
) -> Resultado<()> {
    if profundidad > MAX_PROFUNDIDAD {
        hallazgos.push(Hallazgo::nuevo(
            prefijo,
            None,
            "carpetas demasiado anidadas: no se sigue",
        ));
        return Ok(());
    }
    let mut entradas = Vec::new();
    for entrada in fs::read_dir(dir).map_err(|e| ErrorWeb::lectura(dir, e))? {
        entradas.push(entrada.map_err(|e| ErrorWeb::lectura(dir, e))?);
    }
    entradas.sort_by_key(|e| e.file_name());
    for entrada in entradas {
        let nombre = match entrada.file_name().into_string() {
            Ok(n) => n,
            Err(n) => {
                hallazgos.push(Hallazgo::nuevo(
                    format!("{prefijo}{}", visible(&n.to_string_lossy())),
                    None,
                    "nombre de fichero que no es UTF-8",
                ));
                continue;
            }
        };
        let rel = format!("{prefijo}{nombre}");
        let tipo = entrada
            .file_type()
            .map_err(|e| ErrorWeb::lectura(&entrada.path(), e))?;
        if tipo.is_symlink() {
            hallazgos.push(Hallazgo::nuevo(
                visible(&rel),
                None,
                "enlace simbólico en el sitio: no se sigue y no debe publicarse",
            ));
        } else if tipo.is_dir() {
            recorrer(
                &entrada.path(),
                &format!("{rel}/"),
                profundidad + 1,
                ficheros,
                hallazgos,
            )?;
        } else if tipo.is_file() {
            ficheros.push(rel);
        } else {
            hallazgos.push(Hallazgo::nuevo(
                visible(&rel),
                None,
                "fichero especial en el sitio (ni fichero ni carpeta)",
            ));
        }
    }
    Ok(())
}

/// Los `id` de una página (y el `name` de sus `<a>`, que también son destino).
fn ids_de(tokens: &[Token]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for token in tokens {
        let Token::Etiqueta(e) = token else { continue };
        if e.cierre {
            continue;
        }
        let candidatos = [Some("id"), (e.nombre == "a").then_some("name")];
        for atributo in candidatos.into_iter().flatten() {
            if let Some(valor) = e.atributo(atributo) {
                if let Some(v) = html::decodificar_atributo(valor) {
                    if !v.is_empty() {
                        ids.insert(v);
                    }
                }
            }
        }
    }
    ids
}

fn leer_pagina(dir: &Path, rel: &str, hallazgos: &mut Vec<Hallazgo>) -> Pagina {
    let vacia = |html: String| Pagina {
        rel: rel.to_string(),
        es_manual: rel.starts_with(RUTA_MANUAL),
        html,
        tokens: None,
        ids: None,
    };
    let ruta = ruta_en(dir, rel);
    match fs::metadata(&ruta) {
        Ok(m) if m.len() > MAX_PAGINA => {
            hallazgos.push(Hallazgo::nuevo(
                rel,
                None,
                "página demasiado grande para comprobarla",
            ));
            return vacia(String::new());
        }
        Ok(_) => {}
        Err(e) => {
            hallazgos.push(Hallazgo::nuevo(rel, None, format!("no se pudo leer: {e}")));
            return vacia(String::new());
        }
    }
    let bytes = match fs::read(&ruta) {
        Ok(b) => b,
        Err(e) => {
            hallazgos.push(Hallazgo::nuevo(rel, None, format!("no se pudo leer: {e}")));
            return vacia(String::new());
        }
    };
    let html = match String::from_utf8(bytes) {
        Ok(h) => h,
        Err(e) => {
            hallazgos.push(Hallazgo::nuevo(
                rel,
                None,
                format!(
                    "no es UTF-8 válido (byte {}): no se puede comprobar",
                    e.utf8_error().valid_up_to()
                ),
            ));
            return vacia(String::new());
        }
    };
    match html::tokenizar_detallado(&html) {
        Ok(tokens) => {
            let ids = ids_de(&tokens);
            Pagina {
                rel: rel.to_string(),
                es_manual: rel.starts_with(RUTA_MANUAL),
                html,
                tokens: Some(tokens),
                ids: Some(ids),
            }
        }
        Err(e) => {
            hallazgos.push(Hallazgo::nuevo(
                rel,
                Some(html::linea_de(&html, e.posicion)),
                format!("HTML que no se puede interpretar: {}", e.que),
            ));
            vacia(html)
        }
    }
}

/// Revisa todo `dir`. Devuelve los hallazgos (vacío: todo bien).
pub fn comprobar_sitio(dir: &Path, lista: &ListaPermitida) -> Resultado<Vec<Hallazgo>> {
    let mut hallazgos = Vec::new();
    let mut nombres = Vec::new();
    recorrer(dir, "", 0, &mut nombres, &mut hallazgos)?;
    nombres.sort();

    let mut sitio = Sitio {
        ficheros: nombres.iter().cloned().collect(),
        paginas: BTreeMap::new(),
    };
    for rel in &nombres {
        if es_pagina(rel) {
            let pagina = leer_pagina(dir, rel, &mut hallazgos);
            sitio.paginas.insert(rel.clone(), pagina);
        } else if es_marcado_con_scripts(rel) {
            let ruta = ruta_en(dir, rel);
            if fs::metadata(&ruta).is_ok_and(|m| m.len() > MAX_PAGINA) {
                hallazgos.push(Hallazgo::nuevo(
                    rel.as_str(),
                    None,
                    "fichero SVG o XML demasiado grande para comprobarlo",
                ));
                continue;
            }
            match fs::read(&ruta) {
                Ok(bytes) => {
                    let texto = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
                    if let Some(que) = contenido_activo(&texto) {
                        hallazgos.push(Hallazgo::nuevo(rel.as_str(), None, que));
                    }
                }
                Err(e) => hallazgos.push(Hallazgo::nuevo(
                    rel.as_str(),
                    None,
                    format!("no se pudo leer: {e}"),
                )),
            }
        }
    }
    if sitio.paginas.is_empty() {
        hallazgos.push(Hallazgo::nuevo(
            ".",
            None,
            "el sitio no tiene ninguna página .html",
        ));
    }

    for pagina in sitio.paginas.values() {
        comprobar_pagina(pagina, &sitio, lista, &mut hallazgos);
    }
    // Última barrera: nada que venga del sitio (nombres de fichero incluidos)
    // puede llevar un carácter de control a un mensaje.
    for h in &mut hallazgos {
        h.fichero = sin_controles(&h.fichero);
        h.que = sin_controles(&h.que);
    }
    Ok(hallazgos)
}

/// `texto` con los caracteres de control escapados (`\n`, `\u{1b}`).
fn sin_controles(texto: &str) -> String {
    if !texto.chars().any(char::is_control) {
        return texto.to_string();
    }
    let mut salida = String::with_capacity(texto.len());
    for c in texto.chars() {
        if c.is_control() {
            salida.extend(c.escape_default());
        } else {
            salida.push(c);
        }
    }
    salida
}

/// Lo que ejecutaría un SVG o un XML abierto directamente (la CSP de una página
/// no los cubre): un `<script>`, un manejador `on…=` o un destino `javascript:`.
/// `texto` ya va en minúsculas.
fn contenido_activo(texto: &str) -> Option<&'static str> {
    if texto.contains("<script") {
        return Some("un fichero SVG o XML lleva <script>");
    }
    if texto.contains("javascript:") {
        return Some("un fichero SVG o XML lleva un destino `javascript:`");
    }
    let b = texto.as_bytes();
    let mut desde = 0;
    while let Some(i) = texto[desde..].find("on") {
        let inicio = desde + i;
        desde = inicio + 2;
        let precede = inicio == 0
            || matches!(
                b[inicio - 1],
                b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b'"' | b'\'' | b'/'
            );
        if !precede {
            continue;
        }
        let mut j = desde;
        while j < b.len() && b[j].is_ascii_lowercase() {
            j += 1;
        }
        if j == desde {
            continue;
        }
        while j < b.len() && matches!(b[j], b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
            j += 1;
        }
        if b.get(j) == Some(&b'=') {
            return Some("un fichero SVG o XML lleva un manejador de eventos `on…`");
        }
    }
    None
}

/// Atributos que llevan una URL (uno solo).
const ATRIBUTOS_URL: &[&str] = &[
    "href",
    "src",
    "xlink:href",
    "action",
    "formaction",
    "poster",
    "data",
    "background",
    "manifest",
    "codebase",
];

/// Las URL de un atributo, o ninguna si no lleva.
fn urls_de<'a>(nombre: &str, valor: &'a str) -> Vec<&'a str> {
    if ATRIBUTOS_URL.contains(&nombre) {
        vec![valor]
    } else if nombre == "srcset" || nombre == "imagesrcset" {
        valor
            .split(',')
            .filter_map(|candidato| candidato.split_whitespace().next())
            .collect()
    } else if nombre == "ping" {
        valor.split_whitespace().collect()
    } else {
        Vec::new()
    }
}

/// `true` si ese atributo de esa etiqueta hace que el navegador cargue la URL
/// (y no solo que el usuario pueda seguirla).
fn carga_recurso(e: &Etiqueta, atributo: &str) -> bool {
    match atributo {
        "href" | "xlink:href" => match e.nombre.as_str() {
            "a" | "area" => false,
            "link" => {
                let rel = e.atributo("rel").unwrap_or("").to_ascii_lowercase();
                let cargan = [
                    "stylesheet",
                    "icon",
                    "preload",
                    "modulepreload",
                    "prefetch",
                    "prerender",
                    "manifest",
                    "mask-icon",
                    "apple-touch-icon",
                ];
                let navega = [
                    "canonical",
                    "alternate",
                    "author",
                    "license",
                    "help",
                    "prev",
                    "next",
                    "me",
                ];
                let rels: Vec<&str> = rel.split_whitespace().collect();
                rels.iter().any(|r| cargan.contains(r)) || !rels.iter().any(|r| navega.contains(r))
            }
            _ => true,
        },
        "action" | "formaction" | "ping" => false,
        _ => true,
    }
}

fn comprobar_pagina(p: &Pagina, sitio: &Sitio, lista: &ListaPermitida, h: &mut Vec<Hallazgo>) {
    let Some(tokens) = &p.tokens else { return };
    let aviso = |posicion: usize, que: String| {
        Hallazgo::nuevo(p.rel.as_str(), Some(html::linea_de(&p.html, posicion)), que)
    };

    for token in tokens {
        let Token::Etiqueta(e) = token else { continue };
        if e.cierre {
            continue;
        }

        for (nombre, _) in &e.atributos {
            if nombre == "style" {
                h.push(aviso(
                    e.inicio,
                    format!(
                        "atributo `style` en <{}>: la CSP (`style-src 'self'`) no lo permite; usa una clase",
                        visible(&e.nombre)
                    ),
                ));
            } else if nombre.starts_with("on") {
                h.push(aviso(
                    e.inicio,
                    format!(
                        "atributo `{}` en <{}>: los manejadores de eventos en línea no se permiten",
                        visible(nombre),
                        visible(&e.nombre)
                    ),
                ));
            }
        }

        match e.nombre.as_str() {
            "script" if !p.es_manual => h.push(aviso(
                e.inicio,
                "<script> en una página que no es del manual: las páginas propias no llevan JavaScript"
                    .to_string(),
            )),
            "style" => h.push(aviso(
                e.inicio,
                "<style> en línea: la CSP (`style-src 'self'`) no lo permite; usa una hoja de estilos"
                    .to_string(),
            )),
            "base" => h.push(aviso(
                e.inicio,
                "<base> no permitido: cambiaría todos los enlaces relativos".to_string(),
            )),
            "meta" => match e.atributo("http-equiv").map(html::decodificar_atributo) {
                // Descodificado como el navegador: `re&#102;resh` también refresca.
                Some(Some(v)) if v.trim().eq_ignore_ascii_case("refresh") => h.push(aviso(
                    e.inicio,
                    "<meta http-equiv=\"refresh\"> no permitido: una redirección que no pasa por la lista de enlaces"
                        .to_string(),
                )),
                Some(None) => h.push(aviso(
                    e.inicio,
                    "<meta http-equiv> con una referencia de caracteres que no se entiende: no se sabe qué hace"
                        .to_string(),
                )),
                _ => {}
            },
            _ => {}
        }

        for (atributo, valor) in &e.atributos {
            let Some(valor) = valor else { continue };
            for url in urls_de(atributo, valor) {
                comprobar_url(p, sitio, lista, e, atributo, url, h);
            }
        }
    }

    comprobar_csp(p, tokens, h);
}

fn comprobar_url(
    p: &Pagina,
    sitio: &Sitio,
    lista: &ListaPermitida,
    e: &Etiqueta,
    atributo: &str,
    url: &str,
    h: &mut Vec<Hallazgo>,
) {
    let aviso =
        |que: String| Hallazgo::nuevo(p.rel.as_str(), Some(html::linea_de(&p.html, e.inicio)), que);
    let etiqueta = visible(&e.nombre);
    match enlaces::clasificar(&p.rel, url) {
        Destino::Ancla(ancla) => {
            let existe = ancla.is_empty()
                || ancla.eq_ignore_ascii_case("top")
                || p.ids.as_ref().is_some_and(|ids| ids.contains(&ancla));
            if !existe {
                h.push(aviso(format!(
                    "ancla rota: «#{}» (en <{etiqueta} {atributo}>) no existe en esta página",
                    visible(&ancla)
                )));
            }
        }
        Destino::Interno { ruta, ancla } => {
            let Some(destino) = sitio.destino_de(&ruta) else {
                h.push(aviso(format!(
                    "enlace roto: «{}» (en <{etiqueta} {atributo}>) apunta a «{}», que no está en el sitio",
                    visible(url),
                    visible(&ruta)
                )));
                return;
            };
            let Some(ancla) = ancla else { return };
            if ancla.eq_ignore_ascii_case("top") {
                return;
            }
            // Solo las páginas HTML tienen anclas que comprobar.
            let Some(pagina) = sitio.paginas.get(&destino) else {
                return;
            };
            match &pagina.ids {
                Some(ids) if ids.contains(&ancla) => {}
                Some(_) => h.push(aviso(format!(
                    "ancla rota: «{}» (en <{etiqueta} {atributo}>) no existe en «{}»",
                    visible(url),
                    visible(&destino)
                ))),
                None => h.push(aviso(format!(
                    "no se puede comprobar el ancla de «{}»: la página «{}» no se puede interpretar",
                    visible(url),
                    visible(&destino)
                ))),
            }
        }
        Destino::Externo(externa) => {
            if !lista.permite(&externa) {
                h.push(aviso(format!(
                    "enlace externo que no es de la lista permitida (o con formato raro): «{}» (en <{etiqueta} {atributo}>)",
                    visible(&externa)
                )));
            } else if carga_recurso(e, atributo)
                && analizar_https(&externa).is_none_or(|u| u.host != HOST_PROPIO)
            {
                h.push(aviso(format!(
                    "recurso cargado de fuera: «{}» (en <{etiqueta} {atributo}>); la web no carga nada de otros dominios",
                    visible(&externa)
                )));
            }
        }
        Destino::Otro(otro) => {
            let nota = if otro.starts_with('/') && !otro.starts_with("//") {
                "; las rutas absolutas del sitio solo se usan en 404.html, el resto va con rutas relativas"
            } else {
                "; solo se admiten rutas relativas, anclas y https://"
            };
            h.push(aviso(format!(
                "enlace no admitido: «{}» (en <{etiqueta} {atributo}>){nota}",
                visible(&otro)
            )));
        }
    }
}

/// Divide una CSP en `(directiva en minúsculas, fuentes)`.
fn directivas(politica: &str) -> Vec<(String, Vec<String>)> {
    politica
        .split(';')
        .filter_map(|d| {
            let mut trozos = d.split_whitespace();
            let nombre = trozos.next()?.to_ascii_lowercase();
            Some((nombre, trozos.map(str::to_string).collect()))
        })
        .collect()
}

fn comprobar_csp(p: &Pagina, tokens: &[Token], h: &mut Vec<Hallazgo>) {
    let aviso = |posicion: usize, que: String| {
        Hallazgo::nuevo(p.rel.as_str(), Some(html::linea_de(&p.html, posicion)), que)
    };

    // `<meta name="referrer">`: uno solo, con el valor de la web.
    let referrers: Vec<&Etiqueta> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Etiqueta(e) if csp::es_meta_referrer(e) => Some(e),
            _ => None,
        })
        .collect();
    match referrers.as_slice() {
        [uno] if uno.atributo("content") == Some(csp::REFERRER) => {}
        [] => h.push(aviso(
            0,
            format!(
                "falta <meta name=\"referrer\" content=\"{}\">",
                csp::REFERRER
            ),
        )),
        [primero, ..] => h.push(aviso(
            primero.inicio,
            format!(
                "el <meta name=\"referrer\"> debe ser uno solo y valer «{}»",
                csp::REFERRER
            ),
        )),
    }

    let actual = match csp::extraer_de(tokens) {
        Ok(Some(c)) => c,
        Ok(None) => {
            h.push(aviso(
                0,
                "la página no lleva CSP (<meta http-equiv=\"Content-Security-Policy\">)"
                    .to_string(),
            ));
            return;
        }
        Err(e) => {
            h.push(aviso(0, format!("CSP no válida: {e}")));
            return;
        }
    };

    // La CSP es el primer elemento tras `<meta charset>`.
    match csp::cabecera_inicial(&p.html, tokens) {
        Ok(cabecera) => {
            if !cabecera.segundo.is_some_and(csp::es_meta_csp) {
                h.push(aviso(
                    cabecera.charset.inicio,
                    "la CSP debe ser el primer elemento de <head> tras <meta charset>".to_string(),
                ));
            }
        }
        Err(que) => h.push(aviso(0, format!("<head> mal formado: {que}"))),
    }

    let antes = h.len();
    for trozo in actual.split(|c: char| c.is_whitespace() || c == ';') {
        let minusculas = trozo.to_ascii_lowercase();
        if minusculas.starts_with("'unsafe-") || minusculas.starts_with("'wasm-unsafe-") {
            h.push(aviso(
                0,
                format!("la CSP lleva {}: nunca se permite", visible(trozo)),
            ));
        }
    }

    let scripts = csp::scripts_en_linea_de(tokens);
    let directivas = directivas(&actual);
    let script_src = directivas
        .iter()
        .find(|(n, _)| n == "script-src")
        .map(|(_, f)| f);
    match script_src {
        None => h.push(aviso(0, "la CSP no lleva `script-src`".to_string())),
        Some(fuentes) if !p.es_manual => {
            if fuentes.as_slice() != ["'none'"] {
                h.push(aviso(
                    0,
                    "en las páginas propias `script-src` debe ser `'none'`".to_string(),
                ));
            }
        }
        Some(fuentes) => {
            if !fuentes.iter().any(|f| f == "'self'") {
                h.push(aviso(
                    0,
                    "en el manual `script-src` debe llevar `'self'`".to_string(),
                ));
            }
            for script in &scripts {
                let hash = csp::hash_script(script);
                if !fuentes.contains(&hash) {
                    h.push(aviso(
                        0,
                        format!(
                            "`script-src` no lleva el hash de un <script> en línea ({hash}): el navegador lo bloquearía"
                        ),
                    ));
                }
            }
        }
    }

    // Si nada concreto falló, la política debe ser exactamente la esperada.
    if h.len() == antes {
        let esperada = csp::politica(&scripts, p.es_manual);
        if actual != esperada {
            h.push(aviso(
                0,
                format!(
                    "la CSP no es la esperada: lleva «{}» y debería llevar «{}»",
                    visible(&actual),
                    esperada
                ),
            ));
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn analiza_https_con_rigor() {
        let ok = analizar_https("https://github.com/HectorEspejo/magi?x=1#y").unwrap();
        assert_eq!(ok.host, "github.com");
        assert_eq!(ok.ruta, "/HectorEspejo/magi");
        assert_eq!(analizar_https("https://semver.org").unwrap().ruta, "/");
        assert_eq!(
            analizar_https("https://semver.org:443/").unwrap().host,
            "semver.org"
        );
        for malo in [
            "http://semver.org/",
            "HTTPS://semver.org/",
            "https://Semver.org/",
            "https://semver.org:8443/",
            "https://semver.org:/",
            "https://usuario@ejemplo.org/",
            "https://semver.org\\@evil.ejemplo.org/",
            "https://semver.org/a/../b",
            "https://semver.org/a/./b",
            "https://semver.org/a//b",
            "https://semver.org/%2e%2e/x",
            "https://semver.org/%2E/x",
            "https://semver.org/a%2Fb",
            "https://semver.org/a%5cb",
            "https://semver.org/a%zz",
            "https://semver.org/a%",
            "https://semver.org/a b",
            "https://semver.org/ñ",
            "https://semver.org/\"x",
            "https://",
            "https:///x",
            "https://.semver.org/",
            "https://semver..org/",
            "https://-semver.org/",
        ] {
            assert!(analizar_https(malo).is_none(), "debía rechazarse: {malo:?}");
        }
    }

    #[test]
    fn la_lista_vacia_no_permite_nada() {
        let lista = ListaPermitida {
            dominios: vec![("semver.org".into(), vec![])],
        };
        assert!(!lista.permite("https://semver.org/"));
    }

    #[test]
    fn visible_escapa_los_controles_y_acorta() {
        assert_eq!(visible("a\nb\u{1b}[31m"), "a\\nb\\u{1b}[31m");
        assert_eq!(visible(&"x".repeat(500)).chars().count(), 121);
    }

    #[test]
    fn directivas_se_parten_por_punto_y_coma() {
        let d = directivas("default-src 'none'; Script-Src 'self'  'sha256-x' ;");
        assert_eq!(
            d[0],
            ("default-src".to_string(), vec!["'none'".to_string()])
        );
        assert_eq!(d[1].0, "script-src");
        assert_eq!(d[1].1.len(), 2);
    }
}
