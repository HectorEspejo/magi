//! Enlaces http y https en las líneas del terminal (Fase 15).
//!
//! Los enlaces se reconocen sobre líneas lógicas (las que se partieron por el
//! ancho se unen), sin la puntuación final. Solo `http` y `https`: lo demás no
//! es un enlace abrible. Los que apuntan a localhost del host se pueden abrir
//! por un túnel temporal, con la URL reescrita al puerto local.

use std::sync::LazyLock;

use regex::Regex;

/// Patrón de un enlace web (§7.5): `http` o `https`, sin espacios ni
/// controles, comillas, ángulos ni acentos graves.
const PATRON_ENLACE: &str = r#"https?://[^\s<>"'`\x00-\x1f\x7f]+"#;

static ENLACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(PATRON_ENLACE).expect("la expresión regular de enlaces de §7.5 es válida")
});

/// Puntuación que se quita del final: un enlace pegado a una frase no se lleva
/// el punto, la coma ni el paréntesis de cierre.
const FINAL: [char; 11] = ['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"'];

/// Longitudes admitidas de una URL, en caracteres (§7.13).
const LARGO_MINIMO: usize = 10;
const LARGO_MAXIMO: usize = 2048;

/// Hosts que son el propio host de la conexión (§7.5).
const LOCALES: [&str; 4] = ["localhost", "127.0.0.1", "::1", "0.0.0.0"];

/// Un enlace encontrado: línea lógica, columna y URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enlace {
    /// Índice de la línea lógica en la captura.
    pub linea: usize,
    /// Columna (en caracteres) donde empieza dentro de la línea.
    pub columna: usize,
    pub url: String,
}

/// Busca enlaces en las líneas lógicas (índice, texto).
#[must_use]
pub fn enlaces(lineas: &[(usize, String)]) -> Vec<Enlace> {
    let mut encontrados = Vec::new();
    for (indice, texto) in lineas {
        for coincidencia in ENLACE.find_iter(texto) {
            // La columna va en caracteres, no en bytes: el diálogo coloca la
            // letra sobre la URL tal como se ve.
            let columna = texto[..coincidencia.start()].chars().count();
            let url = recortar(coincidencia.as_str());
            let largo = url.chars().count();
            if (LARGO_MINIMO..=LARGO_MAXIMO).contains(&largo) {
                encontrados.push(Enlace {
                    linea: *indice,
                    columna,
                    url: url.to_string(),
                });
            }
        }
    }
    encontrados
}

/// Quita del final la puntuación de la frase, salvo el paréntesis que cierra
/// uno abierto dentro de la propia URL (el de una ruta de Wikipedia, p. ej.).
fn recortar(url: &str) -> &str {
    let mut recortada = url;
    while let Some(ultimo) = recortada.chars().last() {
        if !FINAL.contains(&ultimo) {
            break;
        }
        if ultimo == ')' && cierra_un_parentesis(recortada) {
            break;
        }
        recortada = &recortada[..recortada.len() - ultimo.len_utf8()];
    }
    recortada
}

/// ¿El `)` final cierra un paréntesis abierto dentro de la URL?
fn cierra_un_parentesis(url: &str) -> bool {
    let abiertos = url.chars().filter(|caracter| *caracter == '(').count();
    let cerrados = url.chars().filter(|caracter| *caracter == ')').count();
    abiertos >= cerrados
}

/// Pone letras de la `a` a la `z` a los 26 enlaces más cercanos al final.
///
/// Los elegidos son los últimos de la lista (los más cercanos al final de lo
/// visible) y conservan su orden: el primero de ellos lleva la `a`.
#[must_use]
pub fn con_letras(enlaces: &[Enlace]) -> Vec<(char, Enlace)> {
    let desde = enlaces.len().saturating_sub(26);
    enlaces[desde..]
        .iter()
        .enumerate()
        .map(|(indice, enlace)| {
            let letra = char::from(b'a' + u8::try_from(indice).unwrap_or(0));
            (letra, enlace.clone())
        })
        .collect()
}

/// ¿La URL es http o https?
#[must_use]
pub fn es_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Destino del túnel temporal si la URL apunta a localhost del host:
/// `localhost`, `127.0.0.1`, `::1` o `0.0.0.0`, con su puerto (o el de su
/// esquema).
#[must_use]
pub fn destino_tunel(url: &str) -> Option<(String, u16)> {
    let analizada = analizar(url)?;
    if !LOCALES.contains(&analizada.host.as_str()) {
        return None;
    }
    let destino = if analizada.host == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else {
        analizada.host.clone()
    };
    let puerto = analizada.puerto.unwrap_or(analizada.puerto_por_defecto());
    Some((destino, puerto))
}

/// La misma URL con el puerto local del túnel: esquema, ruta, consulta y
/// fragmento no cambian; el host pasa a `127.0.0.1`.
#[must_use]
pub fn url_por_tunel(url: &str, puerto_local: u16) -> Option<String> {
    if puerto_local == 0 {
        return None;
    }
    let analizada = analizar(url)?;
    if !LOCALES.contains(&analizada.host.as_str()) {
        return None;
    }
    Some(format!(
        "{}://127.0.0.1:{}{}",
        analizada.esquema, puerto_local, analizada.resto
    ))
}

/// Una URL ya troceada: el host sin corchetes y en minúsculas, el puerto si lo
/// lleva y la ruta, consulta y fragmento tal cual.
struct Analizada<'url> {
    esquema: &'url str,
    host: String,
    puerto: Option<u16>,
    resto: &'url str,
}

impl Analizada<'_> {
    fn puerto_por_defecto(&self) -> u16 {
        if self.esquema == "https" {
            443
        } else {
            80
        }
    }
}

/// Analiza solo lo que hace falta de la URL: esquema, host, puerto y el resto.
/// `None` si no es `http` o `https` o el puerto no es válido.
fn analizar(url: &str) -> Option<Analizada<'_>> {
    let (esquema, sin_esquema) = url.split_once("://")?;
    if esquema != "http" && esquema != "https" {
        return None;
    }
    let fin_autoridad = sin_esquema
        .find(['/', '?', '#'])
        .unwrap_or(sin_esquema.len());
    let autoridad = &sin_esquema[..fin_autoridad];
    let resto = &sin_esquema[fin_autoridad..];
    // La información de usuario no forma parte del host; el túnel no la lleva.
    let autoridad = match autoridad.rsplit_once('@') {
        Some((_, despues)) => despues,
        None => autoridad,
    };
    let (host, puerto) = separar_host_y_puerto(autoridad)?;
    Some(Analizada {
        esquema,
        host: host.to_lowercase(),
        puerto,
        resto,
    })
}

/// Separa `host[:puerto]`, con el host IPv6 entre corchetes y el puerto
/// validado (1-65535).
fn separar_host_y_puerto(autoridad: &str) -> Option<(&str, Option<u16>)> {
    let (host, texto_puerto) = if let Some(sin_corchete) = autoridad.strip_prefix('[') {
        let cierre = sin_corchete.find(']')?;
        let host = &sin_corchete[..cierre];
        let despues = &sin_corchete[cierre + 1..];
        if despues.is_empty() {
            (host, None)
        } else {
            (host, Some(despues.strip_prefix(':')?))
        }
    } else {
        match autoridad.split_once(':') {
            Some((host, puerto)) => (host, Some(puerto)),
            None => (autoridad, None),
        }
    };
    if host.is_empty() {
        return None;
    }
    let puerto = match texto_puerto {
        None => None,
        Some(texto) => {
            let puerto = texto.parse::<u16>().ok()?;
            if puerto == 0 {
                return None;
            }
            Some(puerto)
        }
    };
    Some((host, puerto))
}
