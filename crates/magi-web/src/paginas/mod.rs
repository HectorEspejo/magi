//! Páginas de la web: una función por página e idioma que devuelve su HTML.
//!
//! La CSP, el pie del manual y la comprobación se aplican después, sobre todo el
//! sitio, en `construir`. Aquí solo se genera el HTML de cada página.

pub mod descargas;
pub mod no_encontrada;

use serde_json::{json, Value};

use crate::datos::Plataformas;
use crate::error::{ErrorWeb, Resultado};
use crate::plantillas::Plantillas;
use crate::rutas::{self, Idioma, Pagina};
use crate::textos::Textos;

/// Dirección pública de la web: la usa el 404, que sale de cualquier ruta y por
/// eso no puede llevar enlaces relativos.
pub const DOMINIO: &str = "https://magi.erratia.com/";

/// Todo lo que comparten las páginas al renderizarse.
pub struct Entorno<'a> {
    pub plantillas: &'a Plantillas,
    pub textos: &'a Textos,
    pub plataformas: &'a Plataformas,
    /// Número de la versión del escritorio de la que sale el manual (`1.6.1`): lo
    /// dice el pie («manual de la 1.6.1»).
    pub version_manual: &'a str,
}

/// Nombre de una página en el contexto (`pagina`): el mismo que su `serde`.
pub fn id_pagina(pagina: Pagina) -> &'static str {
    match pagina {
        Pagina::Portada => "portada",
        Pagina::Descargas => "descargas",
        Pagina::Versiones => "versiones",
        Pagina::NoEncontrada => "no_encontrada",
    }
}

/// Contexto común a toda página (objeto JSON), con estas claves:
///
/// - `lang`: `es` o `en`; `idioma`: ídem.
/// - `t`: árbol de textos del idioma (`t.portada.titulo`).
/// - `raiz`: prefijo relativo hasta la raíz del sitio (`""`, `"../"`…).
/// - `pagina`: `portada`, `descargas`, `versiones` o `no_encontrada`.
/// - `alternativa`: `{href, hreflang, texto}` de la página equivalente en el otro idioma.
/// - `nav`: `{inicio, descargas, versiones, manual}`, cada uno con su `href` relativo.
/// - `version_manual`.
///
/// En el 404 los `href` son absolutos (`https://magi.erratia.com/…`) y `raiz` es
/// la dirección de la web; su `alternativa` es la portada del otro idioma.
pub fn contexto_comun(e: &Entorno<'_>, pagina: Pagina, idioma: Idioma) -> serde_json::Value {
    let es_404 = pagina == Pagina::NoEncontrada;
    let ruta_propia = rutas::ruta(pagina, idioma);
    let raiz = if es_404 {
        DOMINIO.to_string()
    } else {
        rutas::hasta_raiz(ruta_propia)
    };
    // Enlace a otra ruta del sitio desde esta página.
    let enlace = |hasta: &str| -> String {
        if es_404 {
            format!("{DOMINIO}{hasta}")
        } else {
            enlace_relativo(ruta_propia, hasta)
        }
    };

    let otro = idioma.otro();
    let ruta_alternativa = if es_404 {
        rutas::ruta(Pagina::Portada, otro)
    } else {
        rutas::ruta(pagina, otro)
    };
    let texto_alternativa = e
        .textos
        .texto(idioma, "comun.idioma_otro")
        .map_or_else(|| otro.codigo().to_uppercase(), str::to_string);

    json!({
        "lang": idioma.codigo(),
        "idioma": idioma.codigo(),
        "t": e.textos.arbol(idioma),
        "raiz": raiz,
        "pagina": id_pagina(pagina),
        "alternativa": {
            "href": enlace(ruta_alternativa),
            "hreflang": otro.codigo(),
            "texto": texto_alternativa,
        },
        "nav": {
            "inicio": {"href": enlace(rutas::ruta(Pagina::Portada, idioma))},
            "descargas": {"href": enlace(rutas::ruta(Pagina::Descargas, idioma))},
            "versiones": {"href": enlace(rutas::ruta(Pagina::Versiones, idioma))},
            "manual": {"href": enlace(rutas::RUTA_MANUAL)},
        },
        "version_manual": e.version_manual,
    })
}

/// Como `rutas::relativa`, pero un enlace a la propia raíz desde la raíz sale
/// `./` y no vacío (un `href=""` es válido, pero `./` no deja dudas).
fn enlace_relativo(desde: &str, hasta: &str) -> String {
    let relativa = rutas::relativa(desde, hasta);
    if relativa.is_empty() {
        "./".to_string()
    } else {
        relativa
    }
}

/// Añade claves al contexto de una página (que es siempre un objeto).
pub(crate) fn ampliar(contexto: &mut Value, clave: &str, valor: Value) {
    if let Value::Object(mapa) = contexto {
        mapa.insert(clave.to_string(), valor);
    }
}

/// Un texto de `web/textos/` que la página necesita: si falta, falla diciendo cuál.
pub(crate) fn texto_necesario(e: &Entorno<'_>, idioma: Idioma, clave: &str) -> Resultado<String> {
    e.textos
        .texto(idioma, clave)
        .map(str::to_string)
        .ok_or_else(|| {
            ErrorWeb::dato_raro(
                "web/textos",
                format!("falta la clave «{clave}» en el texto «{}»", idioma.codigo()),
            )
        })
}

/// Fecha corta de una fecha de la API (`2026-11-22T11:04:48Z`): `22 nov 2026` en
/// castellano y `22 Nov 2026` en inglés, con los meses de `comun.mes.mNN`.
///
/// `None` si no empieza por `AAAA-MM-DD` con un mes y un día posibles: una fecha
/// rara no se enseña a medias.
pub fn fecha_legible(textos: &Textos, idioma: Idioma, fecha: &str) -> Option<String> {
    let dia_completo = fecha.get(..10)?;
    if let Some(resto) = fecha.get(10..) {
        if !resto.is_empty() && !resto.starts_with('T') {
            return None;
        }
    }
    let bytes = dia_completo.as_bytes();
    let son_cifras = |tramo: &[u8]| tramo.iter().all(u8::is_ascii_digit);
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !son_cifras(&bytes[0..4]) || !son_cifras(&bytes[5..7]) || !son_cifras(&bytes[8..10]) {
        return None;
    }
    let anio: u32 = dia_completo[0..4].parse().ok()?;
    let mes: u32 = dia_completo[5..7].parse().ok()?;
    let dia: u32 = dia_completo[8..10].parse().ok()?;
    if !(1..=12).contains(&mes) || !(1..=31).contains(&dia) || anio == 0 {
        return None;
    }
    let nombre_mes = textos.texto(idioma, &format!("comun.mes.m{mes:02}"))?;
    Some(format!("{dia} {nombre_mes} {anio}"))
}

/// Tamaño en MB (÷ 1 048 576, una cifra decimal): coma en castellano y punto en
/// inglés. Un fichero que no está vacío nunca sale como `0,0 MB`.
pub fn tamano_legible(bytes: u64, idioma: Idioma) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    let mut cifra = format!("{mb:.1}");
    if bytes > 0 && cifra == "0.0" {
        cifra = "0.1".to_string();
    }
    if idioma == Idioma::Es {
        cifra = cifra.replace('.', ",");
    }
    format!("{cifra} MB")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn textos() -> Textos {
        let mut es = String::from("[comun.mes]\n");
        let mut en = String::from("[comun.mes]\n");
        let meses_es = [
            "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
        ];
        let meses_en = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        for (i, (a, b)) in meses_es.iter().zip(meses_en).enumerate() {
            es.push_str(&format!("m{:02} = \"{a}\"\n", i + 1));
            en.push_str(&format!("m{:02} = \"{b}\"\n", i + 1));
        }
        Textos::desde_textos(&es, "es.toml", &en, "en.toml").unwrap()
    }

    #[test]
    fn la_fecha_sale_corta_en_cada_idioma() {
        let t = textos();
        let api = "2026-11-22T11:04:48Z";
        assert_eq!(
            fecha_legible(&t, Idioma::Es, api).as_deref(),
            Some("22 nov 2026")
        );
        assert_eq!(
            fecha_legible(&t, Idioma::En, api).as_deref(),
            Some("22 Nov 2026")
        );
        assert_eq!(
            fecha_legible(&t, Idioma::Es, "2026-01-05").as_deref(),
            Some("5 ene 2026")
        );
    }

    #[test]
    fn una_fecha_rara_no_se_enseña() {
        let t = textos();
        for rara in [
            "",
            "ayer",
            "2026-13-01",
            "2026-00-10",
            "2026-11-32",
            "2026-11-00",
            "0000-11-10",
            "2026/11/22",
            "2026-11-22 11:04",
            "2026-11-22x",
            "2026-1é-22",
            "é026-11-22T00:00:00Z",
            "２０２６-11-22",
        ] {
            assert_eq!(fecha_legible(&t, Idioma::Es, rara), None, "{rara:?}");
        }
    }

    #[test]
    fn el_tamano_va_en_mb_con_una_cifra() {
        assert_eq!(tamano_legible(9_542_041, Idioma::Es), "9,1 MB");
        assert_eq!(tamano_legible(9_542_041, Idioma::En), "9.1 MB");
        assert_eq!(tamano_legible(1_048_576, Idioma::En), "1.0 MB");
        assert_eq!(tamano_legible(1, Idioma::Es), "0,1 MB");
        assert_eq!(tamano_legible(0, Idioma::Es), "0,0 MB");
        assert!(tamano_legible(u64::MAX, Idioma::En).ends_with(" MB"));
    }
}
