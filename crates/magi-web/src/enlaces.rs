//! Clasificación y resolución de enlaces.
//!
//! Todo lo que llega aquí es entrada no fiable (el HTML de un manual, de una
//! plantilla, de un CHANGELOG): ante la duda, `Otro`, y `comprobar` lo rechaza.

use crate::html::decodificar_atributo;

/// Adónde apunta un enlace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destino {
    /// Dentro del sitio: ruta ya resuelta desde la raíz (sin barra inicial) y
    /// ancla. La ruta y el ancla van descodificados (`%C3%BA` → `ú`), tal como
    /// los comparan un servidor de ficheros y los `id` de la página. Una ruta
    /// que acababa en `/` (o era una carpeta) lleva `index.html`.
    Interno { ruta: String, ancla: Option<String> },
    /// Solo un ancla de la misma página (`#algo`), descodificada.
    Ancla(String),
    /// `https://…`.
    Externo(String),
    /// Todo lo demás (`http:`, `mailto:`, `javascript:`, `data:`, rutas raras…): `comprobar` lo rechaza.
    Otro(String),
}

/// Repositorio de MAGI en GitHub.
const REPOSITORIO: &str = "https://github.com/HectorEspejo/magi";

/// Clasifica `href` desde la página `desde` (ruta del fichero dentro del sitio,
/// p. ej. `en/download/index.html`).
///
/// - `https://…` (en minúsculas y con `//`) → `Externo`; `http:`, `mailto:`,
///   `javascript:`, `data:`, `//host`, cualquier otro esquema, mayúsculas en el
///   esquema, blancos o caracteres de control, `\` y entidades que no sean las
///   conocidas (`javascript&colon;`) → `Otro`.
/// - `#ancla` → `Ancla`; un `href` vacío es la propia página (`Ancla("")`).
/// - Una ruta relativa → `Interno`, resuelta desde la carpeta de `desde`. Una
///   ruta absoluta del sitio (`/x`) es `Otro`, salvo en `404.html`, que se sirve
///   desde cualquier profundidad y es la única que las usa.
pub fn clasificar(desde: &str, href: &str) -> Destino {
    match clasificar_con(desde, href) {
        Some(destino) => destino,
        None => Destino::Otro(href.to_string()),
    }
}

fn clasificar_con(desde: &str, href: &str) -> Option<Destino> {
    let valor = decodificar_atributo(href)?;
    // `href=""` es la propia página (como `#`): no sale de ella.
    if valor.is_empty() {
        return Some(Destino::Ancla(String::new()));
    }
    if valor
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\\' || c == '\u{feff}')
    {
        return None;
    }

    if let Some(ancla) = valor.strip_prefix('#') {
        return Some(Destino::Ancla(decodificar_porcentaje(ancla, true)?));
    }
    if valor.starts_with("//") {
        return None;
    }

    // Un `:` antes del primer `/`, `?` o `#` es un esquema (o algo que un
    // navegador podría tomar por uno): solo `https://` en minúsculas pasa.
    let fin_primero = valor.find(['/', '?', '#']).unwrap_or(valor.len());
    if valor[..fin_primero].contains(':') {
        return if valor.starts_with("https://") {
            Some(Destino::Externo(valor))
        } else {
            None
        };
    }

    let (sin_ancla, ancla) = match valor.split_once('#') {
        Some((a, b)) => (a, Some(b)),
        None => (valor.as_str(), None),
    };
    let ruta_bruta = sin_ancla.split('?').next().unwrap_or("");
    if ruta_bruta.is_empty() {
        return None;
    }
    let (absoluta, ruta_bruta) = match ruta_bruta.strip_prefix('/') {
        Some(resto) => (true, resto),
        None => (false, ruta_bruta),
    };
    if absoluta && desde != "404.html" {
        return None;
    }
    let ruta_bruta = decodificar_porcentaje(ruta_bruta, false)?;
    let carpeta = if absoluta {
        ""
    } else {
        desde.rfind('/').map_or("", |i| &desde[..i])
    };
    let mut ruta = resolver(carpeta, &ruta_bruta)?;
    if ruta.is_empty() || ruta.ends_with('/') {
        ruta.push_str("index.html");
    }
    let ancla = match ancla {
        Some(a) if !a.is_empty() => Some(decodificar_porcentaje(a, true)?),
        _ => None,
    };
    Some(Destino::Interno { ruta, ancla })
}

/// Descodifica `%XX` (UTF-8). `None` si hay un `%` mal formado, un UTF-8 inválido,
/// un carácter de control, `\` o, en una ruta, un `/` escondido (`%2F`), que un
/// servidor y un navegador no leen igual.
fn decodificar_porcentaje(texto: &str, permitir_barra: bool) -> Option<String> {
    if !texto.contains('%') {
        return Some(texto.to_string());
    }
    let b = texto.as_bytes();
    let mut bytes = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let alto = hexadecimal(*b.get(i + 1)?)?;
            let bajo = hexadecimal(*b.get(i + 2)?)?;
            let valor = alto * 16 + bajo;
            if valor == b'/' && !permitir_barra {
                return None;
            }
            bytes.push(valor);
            i += 3;
        } else {
            bytes.push(b[i]);
            i += 1;
        }
    }
    let decodificado = String::from_utf8(bytes).ok()?;
    if decodificado.chars().any(|c| c.is_control() || c == '\\') {
        return None;
    }
    Some(decodificado)
}

fn hexadecimal(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Normaliza `base/relativa` sin salirse de la raíz: `None` si sube por encima de ella.
///
/// `desde_carpeta` es la carpeta de la página (con o sin barra final; vacía es la
/// raíz). El resultado no lleva barra inicial y acaba en `/` si apunta a una
/// carpeta (`relativa` acaba en `/`, `.` o `..`, o está vacía). Una `relativa`
/// absoluta, con `\`, con caracteres de control o con segmentos vacíos
/// intermedios (`a//b`) no se resuelve.
pub fn resolver(desde_carpeta: &str, relativa: &str) -> Option<String> {
    if relativa.starts_with('/') {
        return None;
    }
    let raro = |c: char| c.is_control() || c == '\\';
    if desde_carpeta.chars().any(raro) || relativa.chars().any(raro) {
        return None;
    }

    let mut partes: Vec<&str> = Vec::new();
    for segmento in desde_carpeta.split('/') {
        match segmento {
            "" | "." => {}
            ".." => {
                partes.pop()?;
            }
            otro => partes.push(otro),
        }
    }

    let segmentos: Vec<&str> = relativa.split('/').collect();
    let ultimo = segmentos.len() - 1;
    let mut es_carpeta = false;
    for (i, segmento) in segmentos.iter().enumerate() {
        let final_ = i == ultimo;
        match *segmento {
            "" => {
                if !final_ {
                    return None;
                }
                es_carpeta = true;
            }
            "." => es_carpeta = final_,
            ".." => {
                partes.pop()?;
                es_carpeta = final_;
            }
            otro => {
                partes.push(otro);
                es_carpeta = false;
            }
        }
    }

    let mut ruta = partes.join("/");
    if es_carpeta && !ruta.is_empty() {
        ruta.push('/');
    }
    Some(ruta)
}

/// `^(android-)?v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$`: la etiqueta de una
/// versión publicada, sin candidatas ni sufijos.
fn etiqueta_valida(etiqueta: &str) -> bool {
    let resto = etiqueta.strip_prefix("android-").unwrap_or(etiqueta);
    let Some(numeros) = resto.strip_prefix('v') else {
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

/// La ruta de un repositorio lista para una URL, o `None` si no es segura:
/// segmentos sin `.`, sin `..` en ningún punto ni vacíos, solo letras, cifras y `._-+@~` (lo demás de
/// ASCII se rechaza; las letras no ASCII se codifican en porcentaje).
fn ruta_segura(ruta: &str) -> Option<String> {
    if ruta.is_empty() {
        return Some(String::new());
    }
    if ruta.starts_with('/') {
        return None;
    }
    let sin_barra_final = ruta.strip_suffix('/').unwrap_or(ruta);
    let mut salida = String::with_capacity(ruta.len());
    for (i, segmento) in sin_barra_final.split('/').enumerate() {
        if segmento.is_empty() || segmento == "." || segmento.contains("..") {
            return None;
        }
        if i > 0 {
            salida.push('/');
        }
        for c in segmento.chars() {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '@' | '~') {
                salida.push(c);
            } else if !c.is_ascii() && c.is_alphanumeric() {
                let mut buffer = [0u8; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    salida.push_str(&format!("%{byte:02X}"));
                }
            } else {
                return None;
            }
        }
    }
    if sin_barra_final.len() != ruta.len() {
        salida.push('/');
    }
    Some(salida)
}

/// Un ancla de GitHub (`L10`, `L10-L20`, `seccion-2`): empieza por letra o cifra
/// y sigue con letras, cifras y `_-.` (sin `..`).
fn ancla_segura(ancla: &str) -> bool {
    ancla
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric())
        && ancla.len() <= 200
        && !ancla.contains("..")
        && ancla
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// Destino en GitHub, en la etiqueta publicada, de un fichero o carpeta del
/// repositorio: `blob` para ficheros y `tree` para carpetas (las que acaban en `/`).
/// `https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md`.
///
/// Nunca se deja manipular: la etiqueta debe ser `vX.Y.Z` o `android-vX.Y.Z` (si
/// no, el resultado es la raíz del repositorio) y la ruta no puede tener `..`,
/// `.`, `\`, controles, caracteres raros ni empezar por `/` (si no, el resultado
/// es la raíz del repositorio en esa etiqueta). Admite un `#ancla` final con
/// letras, cifras y `_-.`; si no es así, se descarta el ancla.
pub fn destino_en_github(ruta_repositorio: &str, etiqueta: &str) -> String {
    if !etiqueta_valida(etiqueta) {
        return REPOSITORIO.to_string();
    }
    let raiz = format!("{REPOSITORIO}/tree/{etiqueta}");
    let (ruta, ancla) = match ruta_repositorio.split_once('#') {
        Some((r, a)) => (r, Some(a)),
        None => (ruta_repositorio, None),
    };
    let Some(codificada) = ruta_segura(ruta) else {
        return raiz;
    };
    if codificada.is_empty() {
        return raiz;
    }
    let tipo = if codificada.ends_with('/') {
        "tree"
    } else {
        "blob"
    };
    let mut destino = format!("{REPOSITORIO}/{tipo}/{etiqueta}/{codificada}");
    if let Some(a) = ancla.filter(|a| ancla_segura(a)) {
        destino.push('#');
        destino.push_str(a);
    }
    destino
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn interno(ruta: &str, ancla: Option<&str>) -> Destino {
        Destino::Interno {
            ruta: ruta.into(),
            ancla: ancla.map(Into::into),
        }
    }

    #[test]
    fn clasifica_lo_basico() {
        let d = "en/download/index.html";
        assert_eq!(clasificar(d, "#sec"), Destino::Ancla("sec".into()));
        assert_eq!(
            clasificar(d, "https://github.com/HectorEspejo/magi"),
            Destino::Externo("https://github.com/HectorEspejo/magi".into())
        );
        assert_eq!(clasificar(d, "../"), interno("en/index.html", None));
        assert_eq!(
            clasificar(d, "../../manual/tuneles.html#t%C3%BAnel"),
            interno("manual/tuneles.html", Some("túnel"))
        );
        assert_eq!(
            clasificar(d, "x.html?q=1#a"),
            interno("en/download/x.html", Some("a"))
        );
        assert_eq!(
            clasificar("index.html", "descargas/"),
            interno("descargas/index.html", None)
        );
        assert_eq!(clasificar("index.html", "."), interno("index.html", None));
        assert_eq!(
            clasificar("a/b/index.html", ".."),
            interno("a/index.html", None)
        );
        assert_eq!(
            clasificar("a/index.html", "../.."),
            Destino::Otro("../..".into())
        );
    }

    #[test]
    fn lo_raro_es_otro() {
        for href in [
            "http://github.com/HectorEspejo/magi",
            "mailto:alguien@ejemplo.org",
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "JavaScript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            " javascript:alert(1)",
            "javascript&colon;alert(1)",
            "&#106;avascript:alert(1)",
            "&#x6A;avascript:alert(1)",
            "data:text/html;base64,AAAA",
            "vbscript:x",
            "//evil.ejemplo.org/x",
            "HTTPS://github.com/HectorEspejo/magi",
            "Https://github.com/HectorEspejo/magi",
            "https:\\\\evil.ejemplo.org",
            "https:github.com",
            "/descargas/",
            "?x=1",
            "a b.html",
            "a%2Fb.html",
            "a%00b.html",
            "a%zz.html",
            "a%c3.html",
            "a\\b.html",
            "../../../etc/passwd",
            "x:y",
            "tel:123",
            "a&b",
            " ",
        ] {
            assert!(
                matches!(clasificar("index.html", href), Destino::Otro(_)),
                "debía ser Otro: {href:?} → {:?}",
                clasificar("index.html", href)
            );
        }
    }

    #[test]
    fn un_href_vacio_es_la_propia_pagina() {
        assert_eq!(
            clasificar("a/index.html", ""),
            Destino::Ancla(String::new())
        );
        assert_eq!(
            clasificar("a/index.html", "#"),
            Destino::Ancla(String::new())
        );
        assert!(matches!(clasificar("a/index.html", " "), Destino::Otro(_)));
    }

    #[test]
    fn las_entidades_conocidas_se_descodifican() {
        assert_eq!(
            clasificar("index.html", "https://ejemplo.org/?a=1&amp;b=2"),
            Destino::Externo("https://ejemplo.org/?a=1&b=2".into())
        );
        assert_eq!(
            clasificar("index.html", "&#104;ttps://github.com/"),
            Destino::Externo("https://github.com/".into())
        );
    }

    #[test]
    fn solo_el_404_usa_rutas_absolutas_del_sitio() {
        assert_eq!(
            clasificar("404.html", "/descargas/"),
            interno("descargas/index.html", None)
        );
        assert_eq!(clasificar("404.html", "/"), interno("index.html", None));
        assert_eq!(
            clasificar("404.html", "/css/web.css"),
            interno("css/web.css", None)
        );
        assert!(matches!(
            clasificar("descargas/index.html", "/css/web.css"),
            Destino::Otro(_)
        ));
        assert!(matches!(clasificar("404.html", "/../x"), Destino::Otro(_)));
    }

    #[test]
    fn resolver_normaliza_y_no_sale_de_la_raiz() {
        assert_eq!(resolver("a/b", "../c.html").as_deref(), Some("a/c.html"));
        assert_eq!(resolver("a/b/", "./c").as_deref(), Some("a/b/c"));
        assert_eq!(resolver("", "a/./b/../c").as_deref(), Some("a/c"));
        assert_eq!(resolver("a", "..").as_deref(), Some(""));
        assert_eq!(resolver("a/b", "../").as_deref(), Some("a/"));
        assert_eq!(resolver("a/b", "").as_deref(), Some("a/b/"));
        assert_eq!(resolver("", "").as_deref(), Some(""));
        assert_eq!(resolver("", "../x"), None);
        assert_eq!(resolver("a", "../../x"), None);
        assert_eq!(resolver("a", "/x"), None);
        assert_eq!(resolver("a", "b//c"), None);
        assert_eq!(resolver("a", "b\\c"), None);
        assert_eq!(resolver("a", "b\0c"), None);
        assert_eq!(resolver("..", "x"), None);
    }

    #[test]
    fn la_etiqueta_de_github_se_valida() {
        for buena in ["v1.6.1", "v0.0.0", "v10.20.30", "android-v0.2.0"] {
            assert!(etiqueta_valida(buena), "{buena}");
        }
        for mala in [
            "",
            "1.6.1",
            "v1.6",
            "v1.6.1.2",
            "v1.6.1-rc.1",
            "v1.6.1+x",
            "v01.6.1",
            "v1.06.1",
            "V1.6.1",
            "android-1.6.1",
            "android-android-v1.6.1",
            "v1.6.1/../x",
            "v1.6.1\n",
            "v1..1",
            "v-1.0.0",
            "main",
        ] {
            assert!(!etiqueta_valida(mala), "{mala:?}");
        }
    }

    #[test]
    fn la_ruta_segura_codifica_o_rechaza() {
        assert_eq!(ruta_segura("docs/a.md").as_deref(), Some("docs/a.md"));
        assert_eq!(ruta_segura("docs/").as_deref(), Some("docs/"));
        assert_eq!(
            ruta_segura(".github/workflows/x.yml").as_deref(),
            Some(".github/workflows/x.yml")
        );
        assert_eq!(
            ruta_segura("docs/túneles.md").as_deref(),
            Some("docs/t%C3%BAneles.md")
        );
        for mala in [
            "/docs",
            "docs//a",
            "docs/../a",
            "..",
            ".",
            "./a",
            "a\\b",
            "a b",
            "a?b",
            "a%2e",
            "a\"b",
            "a<b",
            "a\nb",
            "a:b",
            "//",
        ] {
            assert_eq!(ruta_segura(mala), None, "{mala:?}");
        }
    }
}
