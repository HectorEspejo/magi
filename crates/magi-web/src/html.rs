//! Tokenizador mínimo de HTML para `csp` y `comprobar`.
//!
//! Falla cerrado: una etiqueta mal cerrada, un `<` sin cerrar o una entidad rara
//! en un destino son un error, no «se ignora». Reconoce `<!DOCTYPE>`, comentarios,
//! etiquetas con atributos (con o sin comillas, con `>` dentro de comillas),
//! `<script>` y `<style>` con su contenido en bruto, y mayúsculas (`<SCRIPT>`).
//!
//! Sigue al analizador de HTML de los navegadores en lo que importa para la
//! seguridad (qué es un comentario, dónde acaba una etiqueta, qué atributo gana
//! si está repetido) y rechaza lo que podría leerse de dos maneras: declaraciones
//! `<!…>` y `<?…>` que no sean `<!DOCTYPE>`, un `<` suelto, un `<!--` dentro de un
//! `<script>` (estado «escapado» de los navegadores) y el contenido con `<` de un
//! `<script>` o `<style>` dentro de `<svg>` o `<math>` (allí se analiza como
//! marcado).
//!
//! El analizador de los navegadores cambia de estado según el elemento que abre
//! (y según el contexto en que lo hace, que aquí no se conoce): `<title>` y
//! `<textarea>` se leen como texto con entidades, y `<xmp>`, `<iframe>`,
//! `<noembed>`, `<noframes>` y, con JavaScript activo, `<noscript>` como texto en
//! bruto. Si se leyeran como marcado, `<title><!--</title><script>x</script>-->`
//! escondería un script bajo un comentario que para el navegador no existe. Por
//! eso esos elementos se leen en bruto hasta su cierre y, si su contenido lleva un
//! `<`, es un error (un contexto lo lee como texto y otro como marcado); el de
//! `<noscript>` se lee además como marcado, aparte, y debe poder leerse entero
//! sin salirse de sus límites. `<select>`, que los navegadores antiguos tratan
//! distinto al ignorar lo que lleva dentro, no se admite.
//!
//! Todas las posiciones son en bytes y caen siempre en el límite de un carácter:
//! solo se corta en bytes ASCII.

use crate::error::{ErrorWeb, Resultado};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Etiqueta {
    /// Nombre en minúsculas.
    pub nombre: String,
    /// `(nombre en minúsculas, valor)`; el valor es `None` si no lleva `=`.
    /// El valor va tal cual está escrito (sin descodificar entidades): para
    /// descodificarlo, `decodificar_atributo`. Si un atributo se repite, se
    /// conservan todos y `atributo` devuelve el primero, como los navegadores.
    pub atributos: Vec<(String, Option<String>)>,
    /// Es `</nombre>`.
    pub cierre: bool,
    /// Termina en `/>`.
    pub autocierre: bool,
    /// Posiciones en bytes de `<` y de la posición siguiente a `>`.
    pub inicio: usize,
    pub fin: usize,
}

impl Etiqueta {
    pub fn atributo(&self, nombre: &str) -> Option<&str> {
        self.atributos
            .iter()
            .find(|(n, _)| n == nombre)
            .and_then(|(_, v)| v.as_deref())
    }

    pub fn tiene_atributo(&self, nombre: &str) -> bool {
        self.atributos.iter().any(|(n, _)| n == nombre)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Doctype,
    /// Contenido del comentario, sin `<!--` ni `-->`.
    Comentario(String),
    Etiqueta(Etiqueta),
    /// Contenido en bruto de un `<script>`, un `<style>` o uno de los elementos
    /// que los navegadores leen como texto (`<title>`, `<textarea>`, `<xmp>`,
    /// `<iframe>`, `<noembed>` y `<noframes>`), entre su apertura y su cierre. Se
    /// emite siempre, aunque esté vacío: va justo después de la etiqueta de
    /// apertura y justo antes de la de cierre.
    Crudo {
        etiqueta: String,
        cuerpo: String,
        inicio: usize,
        fin: usize,
    },
    Texto {
        inicio: usize,
        fin: usize,
    },
}

/// Qué falló al tokenizar y dónde (posición en bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorHtml {
    pub posicion: usize,
    pub que: String,
}

impl ErrorHtml {
    fn nuevo(posicion: usize, que: impl Into<String>) -> Self {
        Self {
            posicion,
            que: que.into(),
        }
    }
}

/// Parte `html` en tokens.
pub fn tokenizar(html: &str) -> Resultado<Vec<Token>> {
    tokenizar_detallado(html).map_err(|e| {
        ErrorWeb::dato_raro(
            format!("HTML (línea {})", linea_de(html, e.posicion)),
            e.que,
        )
    })
}

fn es_blanco(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

/// Elementos que los navegadores leen como texto (RCDATA o RAWTEXT) además de
/// `<script>` y `<style>`: su contenido va en bruto hasta su cierre.
const COMO_TEXTO: [&str; 6] = ["title", "textarea", "xmp", "iframe", "noembed", "noframes"];

/// Desplaza las posiciones de un token leído de un trozo del documento.
fn desplazar(token: Token, delta: usize) -> Token {
    match token {
        Token::Etiqueta(mut e) => {
            e.inicio += delta;
            e.fin += delta;
            Token::Etiqueta(e)
        }
        Token::Crudo {
            etiqueta,
            cuerpo,
            inicio,
            fin,
        } => Token::Crudo {
            etiqueta,
            cuerpo,
            inicio: inicio + delta,
            fin: fin + delta,
        },
        Token::Texto { inicio, fin } => Token::Texto {
            inicio: inicio + delta,
            fin: fin + delta,
        },
        otro => otro,
    }
}

/// Como `tokenizar`, pero con la posición del fallo (la usa `comprobar` para dar
/// la línea).
pub fn tokenizar_detallado(html: &str) -> Result<Vec<Token>, ErrorHtml> {
    let b = html.as_bytes();
    let n = b.len();
    let mut tokens = Vec::new();
    let mut pos = 0;
    // `<svg>` y `<math>` abiertos, del más viejo al más nuevo. Es una aproximación
    // (solo cuenta aperturas y cierres), pero un `</math>` suelto no cierra un `<svg>`.
    let mut extranjero: Vec<&'static str> = Vec::new();

    while pos < n {
        if b[pos] != b'<' {
            let fin = b[pos..]
                .iter()
                .position(|c| *c == b'<')
                .map_or(n, |i| pos + i);
            tokens.push(Token::Texto { inicio: pos, fin });
            pos = fin;
            continue;
        }

        let siguiente = b.get(pos + 1).copied();
        match siguiente {
            Some(b'!') => {
                if b[pos..].starts_with(b"<!--") {
                    let (cuerpo, fin) = leer_comentario(html, pos)?;
                    tokens.push(Token::Comentario(cuerpo));
                    pos = fin;
                } else if empieza_por_ci(&b[pos..], b"<!doctype") {
                    let tras = pos + b"<!doctype".len();
                    match b.get(tras) {
                        Some(c) if es_blanco(*c) || *c == b'>' => {}
                        _ => {
                            return Err(ErrorHtml::nuevo(
                                pos,
                                "declaración «<!…» que no es un DOCTYPE válido",
                            ))
                        }
                    }
                    let Some(i) = b[tras..].iter().position(|c| *c == b'>') else {
                        return Err(ErrorHtml::nuevo(pos, "DOCTYPE sin cerrar (falta «>»)"));
                    };
                    tokens.push(Token::Doctype);
                    pos = tras + i + 1;
                } else {
                    return Err(ErrorHtml::nuevo(
                        pos,
                        "declaración «<!…» no admitida (solo DOCTYPE y comentarios)",
                    ));
                }
            }
            Some(b'?') => {
                return Err(ErrorHtml::nuevo(
                    pos,
                    "instrucción «<?…» no admitida (los navegadores la leen como comentario)",
                ));
            }
            Some(b'/') => {
                if !b.get(pos + 2).is_some_and(u8::is_ascii_alphabetic) {
                    return Err(ErrorHtml::nuevo(
                        pos,
                        "«</» que no abre una etiqueta de cierre válida",
                    ));
                }
                let et = leer_etiqueta(html, pos)?;
                if matches!(et.nombre.as_str(), "svg" | "math") {
                    // Como los navegadores: cierra el más reciente de su clase y
                    // todo lo que tiene dentro; si no hay ninguno, no hace nada.
                    if let Some(i) = extranjero.iter().rposition(|n| *n == et.nombre) {
                        extranjero.truncate(i);
                    }
                }
                pos = et.fin;
                tokens.push(Token::Etiqueta(et));
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let et = leer_etiqueta(html, pos)?;
                let nombre = et.nombre.clone();
                if nombre == "select" {
                    return Err(ErrorHtml::nuevo(
                        pos,
                        "<select> no admitido: según el navegador ignora parte de lo que lleva \
                         dentro y no se sabe cómo se leería",
                    ));
                }
                if !et.autocierre {
                    match nombre.as_str() {
                        "svg" => extranjero.push("svg"),
                        "math" => extranjero.push("math"),
                        _ => {}
                    }
                }
                let crudo =
                    matches!(nombre.as_str(), "script" | "style") || COMO_TEXTO.contains(&&*nombre);
                let es_noscript = nombre == "noscript";
                let apertura = et.inicio;
                pos = et.fin;
                tokens.push(Token::Etiqueta(et));
                if crudo || es_noscript {
                    let Some(fin_cuerpo) = buscar_cierre(b, pos, nombre.as_bytes()) else {
                        return Err(ErrorHtml::nuevo(
                            apertura,
                            format!("<{nombre}> sin cerrar (falta «</{nombre}>»)"),
                        ));
                    };
                    let cuerpo = trozo(html, pos, fin_cuerpo);
                    if es_noscript {
                        // Con JavaScript activo es texto hasta el primer cierre; sin
                        // él, marcado. Se acepta solo si como marcado se lee entero
                        // dentro de sus límites: así las dos lecturas coinciden.
                        let internos = tokenizar_detallado(cuerpo).map_err(|e| {
                            ErrorHtml::nuevo(
                                pos + e.posicion,
                                format!("dentro de <noscript>: {}", e.que),
                            )
                        })?;
                        tokens.extend(internos.into_iter().map(|t| desplazar(t, pos)));
                        pos = fin_cuerpo;
                        continue;
                    }
                    if nombre == "script" && cuerpo.contains("<!--") {
                        return Err(ErrorHtml::nuevo(
                            pos,
                            "«<!--» dentro de un <script>: los navegadores lo leen de otra \
                             manera y su hash no sería fiable",
                        ));
                    }
                    let es_codigo = matches!(nombre.as_str(), "script" | "style");
                    if cuerpo.contains('<') && (!es_codigo || !extranjero.is_empty()) {
                        let motivo = if es_codigo {
                            "dentro de <svg> o <math>: allí se analiza como marcado y no como texto"
                        } else {
                            "dentro: según el contexto un navegador lo lee como texto o como \
                             marcado (escríbelo como «&lt;»)"
                        };
                        return Err(ErrorHtml::nuevo(
                            pos,
                            format!("<{nombre}> con «<» {motivo}"),
                        ));
                    }
                    tokens.push(Token::Crudo {
                        etiqueta: nombre,
                        cuerpo: cuerpo.to_string(),
                        inicio: pos,
                        fin: fin_cuerpo,
                    });
                    pos = fin_cuerpo;
                }
            }
            _ => {
                return Err(ErrorHtml::nuevo(
                    pos,
                    "«<» suelto: o no cierra una etiqueta o debería ir como «&lt;»",
                ));
            }
        }
    }
    Ok(tokens)
}

/// `html[desde..hasta]` sin pánico: vacío si los límites no son válidos.
fn trozo(html: &str, desde: usize, hasta: usize) -> &str {
    html.get(desde..hasta).unwrap_or("")
}

fn empieza_por_ci(b: &[u8], prefijo: &[u8]) -> bool {
    b.len() >= prefijo.len() && b[..prefijo.len()].eq_ignore_ascii_case(prefijo)
}

/// Lee el comentario que empieza en `inicio` (`<!--`). Devuelve su contenido y
/// la posición siguiente. Termina donde lo hacen los navegadores: en `-->` o en
/// `--!>`, y `<!-->` y `<!--->` son comentarios vacíos.
fn leer_comentario(html: &str, inicio: usize) -> Result<(String, usize), ErrorHtml> {
    let b = html.as_bytes();
    let cuerpo_ini = inicio + 4;
    let resto = &b[cuerpo_ini.min(b.len())..];
    if resto.starts_with(b">") {
        return Ok((String::new(), cuerpo_ini + 1));
    }
    if resto.starts_with(b"->") {
        return Ok((String::new(), cuerpo_ini + 2));
    }
    let mut i = cuerpo_ini;
    while i + 1 < b.len() {
        if b[i] == b'-' && b[i + 1] == b'-' {
            match b.get(i + 2) {
                Some(b'>') => return Ok((trozo(html, cuerpo_ini, i).to_string(), i + 3)),
                Some(b'!') if b.get(i + 3) == Some(&b'>') => {
                    return Ok((trozo(html, cuerpo_ini, i).to_string(), i + 4))
                }
                _ => {}
            }
        }
        i += 1;
    }
    Err(ErrorHtml::nuevo(
        inicio,
        "comentario sin cerrar (falta «-->»)",
    ))
}

/// Posición del `<` de `</nombre` (sin distinguir mayúsculas) seguido de blanco,
/// `/` o `>`, a partir de `desde`.
fn buscar_cierre(b: &[u8], desde: usize, nombre: &[u8]) -> Option<usize> {
    let largo = 2 + nombre.len();
    let mut i = desde;
    while i + largo < b.len() {
        if b[i] == b'<' && b[i + 1] == b'/' && b[i + 2..i + largo].eq_ignore_ascii_case(nombre) {
            let c = b[i + largo];
            if es_blanco(c) || c == b'/' || c == b'>' {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Lee la etiqueta (de apertura o de cierre) que empieza en `inicio` (`<`).
fn leer_etiqueta(html: &str, inicio: usize) -> Result<Etiqueta, ErrorHtml> {
    let b = html.as_bytes();
    let n = b.len();
    let mut i = inicio + 1;
    let cierre = b.get(i) == Some(&b'/');
    if cierre {
        i += 1;
    }

    let ini_nombre = i;
    while i < n && !es_blanco(b[i]) && b[i] != b'/' && b[i] != b'>' {
        i += 1;
    }
    let nombre = trozo(html, ini_nombre, i).to_ascii_lowercase();

    let mut atributos: Vec<(String, Option<String>)> = Vec::new();
    let mut autocierre = false;
    loop {
        while i < n && es_blanco(b[i]) {
            i += 1;
        }
        if i >= n {
            return Err(ErrorHtml::nuevo(
                inicio,
                format!("etiqueta <{nombre}> sin cerrar (falta «>»)"),
            ));
        }
        match b[i] {
            b'>' => {
                i += 1;
                break;
            }
            b'/' => {
                if b.get(i + 1) == Some(&b'>') {
                    autocierre = true;
                    i += 2;
                    break;
                }
                i += 1;
                continue;
            }
            _ => {}
        }

        // Nombre del atributo: un `=` inicial forma parte del nombre.
        let ini = i;
        if b[i] == b'=' {
            i += 1;
        }
        while i < n && !es_blanco(b[i]) && b[i] != b'/' && b[i] != b'>' && b[i] != b'=' {
            i += 1;
        }
        let nombre_attr = trozo(html, ini, i).to_ascii_lowercase();

        let mut j = i;
        while j < n && es_blanco(b[j]) {
            j += 1;
        }
        if j < n && b[j] == b'=' {
            j += 1;
            while j < n && es_blanco(b[j]) {
                j += 1;
            }
            if j >= n {
                return Err(ErrorHtml::nuevo(
                    inicio,
                    format!("etiqueta <{nombre}> sin cerrar (falta el valor de un atributo)"),
                ));
            }
            match b[j] {
                comilla @ (b'"' | b'\'') => {
                    let Some(k) = b[j + 1..].iter().position(|c| *c == comilla) else {
                        return Err(ErrorHtml::nuevo(
                            inicio,
                            format!(
                                "comillas sin cerrar en el atributo «{nombre_attr}» de <{nombre}>"
                            ),
                        ));
                    };
                    let fin = j + 1 + k;
                    atributos.push((nombre_attr, Some(trozo(html, j + 1, fin).to_string())));
                    i = fin + 1;
                }
                b'>' => {
                    atributos.push((nombre_attr, Some(String::new())));
                    i = j;
                }
                _ => {
                    let ini_valor = j;
                    while j < n && !es_blanco(b[j]) && b[j] != b'>' {
                        j += 1;
                    }
                    atributos.push((nombre_attr, Some(trozo(html, ini_valor, j).to_string())));
                    i = j;
                }
            }
        } else {
            atributos.push((nombre_attr, None));
            i = j;
        }
    }

    Ok(Etiqueta {
        nombre,
        atributos,
        cierre,
        autocierre,
        inicio,
        fin: i,
    })
}

/// Línea (desde 1) de una posición en bytes.
pub fn linea_de(html: &str, posicion: usize) -> usize {
    let hasta = posicion.min(html.len());
    html.as_bytes()[..hasta]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

/// Descodifica las referencias de caracteres de un valor de atributo: solo las
/// numéricas (`&#58;`, `&#x3a;`) y `&amp;`, `&lt;`, `&gt;`, `&quot;` y `&apos;`,
/// siempre con su `;`. Devuelve `None` ante cualquier otro `&` (una entidad que
/// no conocemos puede esconder un esquema, p. ej. `javascript&colon;`): quien
/// llama debe tratarlo como un valor no fiable.
pub fn decodificar_atributo(valor: &str) -> Option<String> {
    if !valor.contains('&') {
        return Some(valor.to_string());
    }
    let mut salida = String::with_capacity(valor.len());
    let mut resto = valor;
    while let Some(i) = resto.find('&') {
        salida.push_str(&resto[..i]);
        let tras = &resto[i + 1..];
        let fin = tras.find(';')?;
        if fin > 10 {
            return None;
        }
        let nombre = &tras[..fin];
        let c = match nombre {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let (digitos, base) = if let Some(h) = nombre
                    .strip_prefix("#x")
                    .or_else(|| nombre.strip_prefix("#X"))
                {
                    (h, 16)
                } else {
                    (nombre.strip_prefix('#')?, 10)
                };
                // `from_str_radix` admite un `+` inicial; una referencia de caracteres no.
                if digitos.is_empty()
                    || digitos.len() > 7
                    || !digitos.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return None;
                }
                let valor = u32::from_str_radix(digitos, base).ok()?;
                let c = char::from_u32(valor)?;
                if c == '\0' {
                    return None;
                }
                c
            }
        };
        salida.push(c);
        resto = &tras[fin + 1..];
    }
    salida.push_str(resto);
    Some(salida)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn etiquetas(html: &str) -> Vec<Etiqueta> {
        tokenizar(html)
            .unwrap()
            .into_iter()
            .filter_map(|t| match t {
                Token::Etiqueta(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn reconoce_doctype_comentarios_y_texto() {
        let html = "<!DOCTYPE HTML><!-- hola\nmundo --><p>x</p>";
        let t = tokenizar(html).unwrap();
        assert_eq!(t[0], Token::Doctype);
        assert_eq!(t[1], Token::Comentario(" hola\nmundo ".into()));
        assert!(matches!(&t[2], Token::Etiqueta(e) if e.nombre == "p" && !e.cierre));
        let Token::Texto { inicio, fin } = t[3] else {
            panic!("esperaba texto")
        };
        assert_eq!(&html[inicio..fin], "x");
        assert!(matches!(&t[4], Token::Etiqueta(e) if e.nombre == "p" && e.cierre));
        assert_eq!(t.len(), 5);
    }

    #[test]
    fn los_atributos_admiten_comillas_dobles_simples_y_ninguna() {
        let e = &etiquetas("<a HREF=\"x>y\" Class='b c' id=z data-x hidden=>")[0];
        assert_eq!(e.nombre, "a");
        assert_eq!(e.atributo("href"), Some("x>y"));
        assert_eq!(e.atributo("class"), Some("b c"));
        assert_eq!(e.atributo("id"), Some("z"));
        assert!(e.tiene_atributo("data-x"));
        assert_eq!(e.atributo("data-x"), None);
        assert_eq!(e.atributo("hidden"), Some(""));
    }

    #[test]
    fn el_primer_atributo_repetido_gana_como_en_los_navegadores() {
        let e = &etiquetas("<a href=\"uno\" href=\"dos\">")[0];
        assert_eq!(e.atributo("href"), Some("uno"));
        assert_eq!(e.atributos.len(), 2);
    }

    #[test]
    fn el_valor_sin_comillas_llega_hasta_el_blanco_o_el_cierre() {
        let e = &etiquetas("<a href=foo/>")[0];
        assert_eq!(e.atributo("href"), Some("foo/"));
        assert!(!e.autocierre);
        let e = &etiquetas("<br/>")[0];
        assert!(e.autocierre);
        let e = &etiquetas("<img src=\"a\" />")[0];
        assert!(e.autocierre);
    }

    #[test]
    fn el_contenido_de_script_y_style_va_en_bruto_y_sin_distinguir_mayusculas() {
        let t = tokenizar("<SCRIPT>if (a<b) { x = '</div>'; }</ScRiPt ><b>").unwrap();
        assert!(matches!(&t[0], Token::Etiqueta(e) if e.nombre == "script"));
        match &t[1] {
            Token::Crudo {
                etiqueta, cuerpo, ..
            } => {
                assert_eq!(etiqueta, "script");
                assert_eq!(cuerpo, "if (a<b) { x = '</div>'; }");
            }
            otro => panic!("esperaba el cuerpo en bruto, salió {otro:?}"),
        }
        assert!(matches!(&t[2], Token::Etiqueta(e) if e.nombre == "script" && e.cierre));
    }

    #[test]
    fn un_cierre_que_solo_empieza_igual_no_cierra_el_script() {
        let t = tokenizar("<script>a</scripty>b</script>").unwrap();
        assert!(matches!(&t[1], Token::Crudo { cuerpo, .. } if cuerpo == "a</scripty>b"));
    }

    #[test]
    fn un_script_vacio_emite_su_cuerpo_vacio() {
        let t = tokenizar("<script></script>").unwrap();
        assert!(matches!(&t[1], Token::Crudo { cuerpo, .. } if cuerpo.is_empty()));
    }

    #[test]
    fn un_comentario_se_cierra_como_en_los_navegadores() {
        let t = tokenizar("<!--a--!><p><!--><!---><i>").unwrap();
        assert_eq!(t[0], Token::Comentario("a".into()));
        assert!(matches!(&t[1], Token::Etiqueta(e) if e.nombre == "p"));
        assert_eq!(t[2], Token::Comentario(String::new()));
        assert_eq!(t[3], Token::Comentario(String::new()));
        assert!(matches!(&t[4], Token::Etiqueta(e) if e.nombre == "i"));
    }

    #[test]
    fn las_posiciones_son_en_bytes_con_acentos_y_emojis() {
        let html = "<p>añ🙂</p><a href=\"ñ\">";
        let t = tokenizar(html).unwrap();
        let Token::Texto { inicio, fin } = t[1] else {
            panic!("esperaba texto")
        };
        assert_eq!(&html[inicio..fin], "añ🙂");
        let Token::Etiqueta(e) = &t[3] else {
            panic!("esperaba etiqueta")
        };
        assert_eq!(&html[e.inicio..e.fin], "<a href=\"ñ\">");
    }

    #[test]
    fn lo_que_no_se_puede_leer_falla() {
        for malo in [
            "<",
            "a < b",
            "<p",
            "<p class=\"x",
            "<p class=",
            "<!-- sin cerrar",
            "<script>sin cerrar",
            "<style>a{}",
            "<!DOCTYPE html",
            "<!DOCTYPEhtml>",
            "<![CDATA[x]]>",
            "<?xml version=\"1.0\"?>",
            "</>",
            "</ p>",
            "<script>x = '<!-- y'</script>",
            "<svg><style><a>x</a></style></svg>",
        ] {
            assert!(tokenizar(malo).is_err(), "debía fallar: {malo:?}");
        }
    }

    #[test]
    fn un_script_en_svg_sin_marcado_dentro_se_lee_igual() {
        let t = tokenizar("<svg><script>alert(1)</script></svg>").unwrap();
        assert!(t
            .iter()
            .any(|t| matches!(t, Token::Crudo { cuerpo, .. } if cuerpo == "alert(1)")));
    }

    #[test]
    fn linea_de_no_entra_en_panico_en_mitad_de_un_caracter() {
        let html = "a\nñ\nb";
        assert_eq!(linea_de(html, 0), 1);
        assert_eq!(linea_de(html, 3), 2);
        assert_eq!(linea_de(html, 4), 2); // dentro de «ñ»
        assert_eq!(linea_de(html, 5), 3);
        assert_eq!(linea_de(html, 999), 3);
    }

    #[test]
    fn decodifica_solo_lo_que_conoce() {
        assert_eq!(decodificar_atributo("a&amp;b").as_deref(), Some("a&b"));
        assert_eq!(decodificar_atributo("&#58;&#x3A;").as_deref(), Some("::"));
        assert_eq!(
            decodificar_atributo("sin entidades").as_deref(),
            Some("sin entidades")
        );
        assert_eq!(decodificar_atributo("javascript&colon;x"), None);
        assert_eq!(decodificar_atributo("a&b"), None);
        assert_eq!(decodificar_atributo("&#0;"), None);
        assert_eq!(decodificar_atributo("&#xD800;"), None);
        assert_eq!(decodificar_atributo("&#99999999999;"), None);
        assert_eq!(decodificar_atributo("&;"), None);
    }
}
