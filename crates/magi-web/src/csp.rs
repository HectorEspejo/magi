//! Política de seguridad de contenido (informe §7.10).
//!
//! GitHub Pages no deja poner cabeceras: la CSP va en un
//! `<meta http-equiv="Content-Security-Policy">` justo después de
//! `<meta charset>`, en cada página, también en el manual. La pone
//! `magi-web construir` al final, cuando el HTML ya no cambia:
//!
//! - páginas propias: `script-src 'none'` (sin JavaScript);
//! - páginas del manual: `script-src 'self'` más el hash `'sha256-…'` de cada
//!   `<script>` en línea, calculado al generar. Nunca `'unsafe-inline'`.
//!
//! Resto: `default-src 'none'; img-src 'self'; style-src 'self'; font-src 'self';
//! base-uri 'none'; form-action 'none'`; en el manual, además `frame-src 'self'`
//! (relleno de `toc.html` sin JavaScript). Cada página lleva también
//! `<meta name="referrer" content="strict-origin-when-cross-origin">`.

use std::borrow::Cow;
use std::collections::BTreeSet;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};

use crate::error::{ErrorWeb, Resultado};
use crate::html::{self, Etiqueta, Token};

/// Sangría de las dos líneas que añade `insertar` (la de un `<head>` de mdBook).
const SANGRIA: &str = "        ";

/// Valor del `<meta name="referrer">` de todas las páginas.
pub const REFERRER: &str = "strict-origin-when-cross-origin";

/// `'sha256-<base64>'` del cuerpo exacto de un `<script>` en línea.
///
/// Es el texto tal como lo ve el navegador: el analizador de HTML normaliza los
/// saltos de línea (`\r\n` y `\r` pasan a `\n`) antes de que el script exista,
/// así que el hash se calcula sobre ese texto. No se recorta nada más.
pub fn hash_script(cuerpo: &str) -> String {
    let normal: Cow<str> = if cuerpo.contains('\r') {
        Cow::Owned(cuerpo.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(cuerpo)
    };
    let resumen = Sha256::digest(normal.as_bytes());
    format!("'sha256-{}'", STANDARD.encode(resumen.as_slice()))
}

/// Cuerpos de los `<script>` sin `src` de una página.
pub fn scripts_en_linea(html: &str) -> Resultado<Vec<String>> {
    Ok(scripts_en_linea_de(&html::tokenizar(html)?))
}

/// Los `<script>` sin atributo `src` (ni siquiera vacío: un `<script src>` no
/// ejecuta su cuerpo), en cualquier caja y con cualquier `type`.
pub(crate) fn scripts_en_linea_de(tokens: &[Token]) -> Vec<String> {
    let mut cuerpos = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let Token::Etiqueta(e) = token else { continue };
        if e.nombre != "script" || e.cierre || e.tiene_atributo("src") {
            continue;
        }
        if let Some(Token::Crudo { cuerpo, .. }) = tokens.get(i + 1) {
            cuerpos.push(cuerpo.clone());
        }
    }
    cuerpos
}

/// La política (el valor de `content`) para una página con esos scripts en línea.
pub fn politica(scripts_en_linea: &[String], es_manual: bool) -> String {
    const BASE: &str =
        "default-src 'none'; img-src 'self'; style-src 'self'; font-src 'self'; script-src";
    const COLA: &str = "base-uri 'none'; form-action 'none'";
    if !es_manual {
        return format!("{BASE} 'none'; {COLA}");
    }
    let hashes: BTreeSet<String> = scripts_en_linea.iter().map(|s| hash_script(s)).collect();
    let mut scripts = String::from("'self'");
    for h in &hashes {
        scripts.push(' ');
        scripts.push_str(h);
    }
    format!("{BASE} {scripts}; {COLA}; frame-src 'self'")
}

/// `true` si el atributo vale `esperado` (sin distinguir mayúsculas) una vez
/// descodificadas sus entidades, como lo lee el navegador: `http-equiv="refresh"`
/// escrito con `&#102;` sigue siendo un refresco.
pub(crate) fn atributo_es(e: &Etiqueta, nombre: &str, esperado: &str) -> bool {
    e.atributo(nombre)
        .and_then(html::decodificar_atributo)
        .is_some_and(|v| v.eq_ignore_ascii_case(esperado))
}

/// `true` si es un `<meta http-equiv="Content-Security-Policy">`.
pub(crate) fn es_meta_csp(e: &Etiqueta) -> bool {
    e.nombre == "meta" && !e.cierre && atributo_es(e, "http-equiv", "content-security-policy")
}

/// `true` si es un `<meta name="referrer">`.
pub(crate) fn es_meta_referrer(e: &Etiqueta) -> bool {
    e.nombre == "meta" && !e.cierre && atributo_es(e, "name", "referrer")
}

/// Los primeros elementos de `<head>`, sin contar blancos ni comentarios.
pub(crate) struct CabeceraInicial<'a> {
    /// `<meta charset>`, que debe ser lo primero.
    pub charset: &'a Etiqueta,
    /// El elemento que viene justo después.
    pub segundo: Option<&'a Etiqueta>,
}

fn es_blanco_solo(texto: &str) -> bool {
    texto
        .bytes()
        .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
}

/// Busca `<head>` y comprueba que su primer elemento es `<meta charset>`. Solo
/// se permiten blancos y comentarios antes (mdBook pone un comentario).
pub(crate) fn cabecera_inicial<'a>(
    html: &str,
    tokens: &'a [Token],
) -> Result<CabeceraInicial<'a>, String> {
    let Some(i_head) = tokens
        .iter()
        .position(|t| matches!(t, Token::Etiqueta(e) if e.nombre == "head" && !e.cierre))
    else {
        return Err("no hay <head>".to_string());
    };
    let mut elementos: Vec<&'a Etiqueta> = Vec::new();
    for token in &tokens[i_head + 1..] {
        match token {
            Token::Comentario(_) => {}
            Token::Texto { inicio, fin } => {
                if !es_blanco_solo(html.get(*inicio..*fin).unwrap_or("x")) {
                    break;
                }
            }
            Token::Etiqueta(e) if !e.cierre => {
                elementos.push(e);
                if elementos.len() == 2 {
                    break;
                }
            }
            _ => break,
        }
    }
    match elementos.first() {
        Some(e) if e.nombre == "meta" && e.tiene_atributo("charset") => Ok(CabeceraInicial {
            charset: e,
            segundo: elementos.get(1).copied(),
        }),
        _ => Err("el primer elemento de <head> no es <meta charset>".to_string()),
    }
}

/// Quita `rangos` (ordenados, sin solaparse) de `html`, junto con el blanco que
/// los precede, para que `insertar` deshaga exactamente lo que insertó.
fn quitar_rangos(html: &str, rangos: &[(usize, usize)]) -> String {
    let mut salida = String::with_capacity(html.len());
    let mut desde = 0;
    for &(inicio, fin) in rangos {
        let mut ini = inicio;
        while ini > desde
            && matches!(
                html.as_bytes()[ini - 1],
                b' ' | b'\t' | b'\n' | b'\r' | 0x0c
            )
        {
            ini -= 1;
        }
        salida.push_str(html.get(desde..ini).unwrap_or(""));
        desde = fin;
    }
    salida.push_str(html.get(desde..).unwrap_or(""));
    salida
}

/// Inserta la CSP y el `referrer` en el HTML de una página. Si ya había una CSP,
/// la sustituye (la construcción es idempotente).
///
/// Van justo después de `<meta charset>`, que debe ser el primer elemento de
/// `<head>`. Los hashes salen del HTML tal como queda: llámala al final, cuando
/// ya nada más cambia los scripts.
pub fn insertar(html: &str, es_manual: bool) -> Resultado<String> {
    let tokens = html::tokenizar(html)?;
    let mut rangos: Vec<(usize, usize)> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Etiqueta(e) if es_meta_csp(e) || es_meta_referrer(e) => Some((e.inicio, e.fin)),
            _ => None,
        })
        .collect();
    rangos.sort_unstable();
    let limpio = quitar_rangos(html, &rangos);

    let tokens = html::tokenizar(&limpio)?;
    let cabecera = cabecera_inicial(&limpio, &tokens)
        .map_err(|que| ErrorWeb::dato_raro("HTML (cabecera)", que))?;
    let punto = cabecera.charset.fin;
    let politica = politica(&scripts_en_linea_de(&tokens), es_manual);
    let nuevo = format!(
        "\n{SANGRIA}<meta http-equiv=\"Content-Security-Policy\" content=\"{politica}\">\
         \n{SANGRIA}<meta name=\"referrer\" content=\"{REFERRER}\">"
    );

    let mut salida = String::with_capacity(limpio.len() + nuevo.len());
    salida.push_str(&limpio[..punto]);
    salida.push_str(&nuevo);
    salida.push_str(&limpio[punto..]);
    Ok(salida)
}

/// Extrae la CSP de una página ya generada (para `comprobar`). `None` si no
/// lleva ninguna; error si lleva más de una o si su `content` no se entiende.
pub fn extraer(html: &str) -> Resultado<Option<String>> {
    let tokens = html::tokenizar(html)?;
    extraer_de(&tokens)
}

/// Como `extraer`, sobre unos tokens ya leídos.
pub(crate) fn extraer_de(tokens: &[Token]) -> Resultado<Option<String>> {
    let mut halladas = tokens.iter().filter_map(|t| match t {
        Token::Etiqueta(e) if es_meta_csp(e) => Some(e),
        _ => None,
    });
    let Some(primera) = halladas.next() else {
        return Ok(None);
    };
    if halladas.next().is_some() {
        return Err(ErrorWeb::dato_raro(
            "HTML (CSP)",
            "la página lleva más de una CSP",
        ));
    }
    let Some(contenido) = primera.atributo("content") else {
        return Err(ErrorWeb::dato_raro(
            "HTML (CSP)",
            "el <meta> de la CSP no tiene `content`",
        ));
    };
    html::decodificar_atributo(contenido)
        .map(Some)
        .ok_or_else(|| {
            ErrorWeb::dato_raro(
                "HTML (CSP)",
                "el `content` de la CSP lleva una referencia de caracteres que no se entiende",
            )
        })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_hash_es_el_de_sha256_en_base64_estandar() {
        // sha256("") en base64 estándar.
        assert_eq!(
            hash_script(""),
            "'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='"
        );
    }

    #[test]
    fn el_hash_no_recorta_ni_cambia_los_blancos_salvo_los_saltos_de_linea() {
        assert_ne!(hash_script("a"), hash_script(" a"));
        assert_ne!(hash_script("a"), hash_script("a\n"));
        // El navegador normaliza \r\n y \r a \n antes de calcular el hash.
        assert_eq!(hash_script("a\r\nb\rc"), hash_script("a\nb\nc"));
    }

    #[test]
    fn la_politica_del_manual_ordena_y_no_repite_los_hashes() {
        let p = politica(&["b".into(), "a".into(), "b".into()], true);
        let (ha, hb) = (hash_script("a"), hash_script("b"));
        let (primero, segundo) = if ha < hb { (&ha, &hb) } else { (&hb, &ha) };
        assert!(
            p.contains(&format!("script-src 'self' {primero} {segundo}; ")),
            "{p}"
        );
        assert_eq!(p.matches(&ha).count(), 1);
        assert!(p.ends_with("; frame-src 'self'"));
    }

    #[test]
    fn la_cabecera_inicial_salta_comentarios_y_blancos() {
        let html = "<head>\n <!-- c -->\n <meta charset=\"UTF-8\">\n <title>x</title></head>";
        let tokens = html::tokenizar(html).unwrap();
        let c = cabecera_inicial(html, &tokens).unwrap();
        assert!(c.charset.tiene_atributo("charset"));
        assert_eq!(c.segundo.map(|e| e.nombre.as_str()), Some("title"));
    }

    #[test]
    fn el_texto_antes_del_charset_lo_rechaza() {
        let html = "<head>hola<meta charset=\"UTF-8\"></head>";
        let tokens = html::tokenizar(html).unwrap();
        assert!(cabecera_inicial(html, &tokens).is_err());
    }
}
