//! Datos de las releases: lo que devuelve la API de GitHub y los `.sha256` publicados.
//!
//! `magi-web releases` lee la API (paginada) y los `.sha256` de los artefactos
//! elegidos y los deja en un `releases.json` normalizado, sin versionar.
//! `magi-web construir` **vuelve a validar** ese fichero: tanto los tests como
//! `--sin-red` pasan por el mismo código, sin un modo «de confianza».
//!
//! Todo lo que llega de la API es entrada no fiable (informe §8): se valida con
//! los patrones exactos de §7.11 y se escapa al pintarlo.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::datos::{Canal, Plataformas};
use crate::descargas::{
    activo_unico, elegir, elegir_versiones, release_de, url_descarga, OpcionesDescargas,
};
use crate::error::{ErrorWeb, Resultado};
use crate::ordenes::{ejecutar_con_plazo, texto_seguro, SalidaOrden, PLAZO_RED};

/// Repositorio del que salen las releases.
pub const REPOSITORIO: &str = "HectorEspejo/magi";
/// Páginas como mucho que se piden a la API (100 releases por página).
pub const PAGINAS_MAXIMAS: u32 = 20;
/// Tope de una página de la API (10 MiB) y de un `.sha256` (1 KiB).
pub const TOPE_PAGINA_API: usize = 10 * 1024 * 1024;
pub const TOPE_SHA256: usize = 1024;

/// Releases por página que se piden a la API.
const POR_PAGINA: usize = 100;
/// Activos como mucho en una release (la API no admite más de 1000; la nuestra, 200).
const ACTIVOS_MAXIMOS: usize = 200;
/// Tope del texto de un campo de la API (etiqueta, nombre, URL…).
const CAMPO_MAXIMO: usize = 4096;
/// Tope del cuerpo de una release (GitHub admite 125 000 caracteres).
const CUERPO_MAXIMO: usize = 512 * 1024;
/// Tope de `releases.json` al leerlo.
const TOPE_DOCUMENTO: u64 = 32 * 1024 * 1024;
/// Plazo de `curl` para un `.sha256`, un poco por encima de su `--max-time`.
const PLAZO_SUMA: Duration = Duration::from_secs(45);
/// Tope que se le pone a `curl` para un `.sha256` (`--max-filesize`).
const TOPE_CURL_SUMA: usize = 4096;
/// Principio de toda URL de un `.sha256`.
const PREFIJO_SUMAS: &str = "https://github.com/HectorEspejo/magi/releases/download/";

/// Un activo de una release (campos recortados de la API).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivoApi {
    pub name: String,
    pub size: u64,
    /// `uploaded` cuando la subida terminó.
    pub state: String,
    pub browser_download_url: String,
}

/// Una release (campos recortados de la API).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseApi {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    pub html_url: String,
    /// Cuerpo de la release; solo se lee para la huella de la app.
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub assets: Vec<ActivoApi>,
}

/// El `releases.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocumentoReleases {
    pub releases: Vec<ReleaseApi>,
    /// Texto de cada `.sha256` bajado, por `"<etiqueta>/<nombre del artefacto>"`.
    #[serde(default)]
    pub sumas: BTreeMap<String, String>,
}

impl DocumentoReleases {
    /// Lee y parsea `releases.json` (la validación semántica la hace `descargas::elegir`).
    pub fn leer(ruta: &Path) -> Resultado<Self> {
        let fichero = std::fs::File::open(ruta).map_err(|e| ErrorWeb::lectura(ruta, e))?;
        let mut texto = String::new();
        // Un byte más que el tope, para saber si lo pasa.
        fichero
            .take(TOPE_DOCUMENTO + 1)
            .read_to_string(&mut texto)
            .map_err(|e| ErrorWeb::lectura(ruta, e))?;
        if texto.len() as u64 > TOPE_DOCUMENTO {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                format!("el fichero pasa del tope de {TOPE_DOCUMENTO} bytes"),
            ));
        }
        Self::desde_texto(&texto, &ruta.display().to_string())
    }

    pub fn desde_texto(texto: &str, origen: &str) -> Resultado<Self> {
        let doc: DocumentoReleases = serde_json::from_str(texto).map_err(|e| {
            ErrorWeb::dato_raro(
                origen,
                format!("JSON no válido: {}", texto_seguro(&e.to_string(), 300)),
            )
        })?;
        doc.comprobar_topes(origen)?;
        Ok(doc)
    }

    pub fn escribir(&self, ruta: &Path) -> Resultado<()> {
        let mut texto = serde_json::to_string_pretty(self).map_err(|e| {
            ErrorWeb::dato_raro(
                ruta.display().to_string(),
                format!("no se pudo serializar: {e}"),
            )
        })?;
        texto.push('\n');
        if let Some(padre) = ruta.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(padre).map_err(|e| ErrorWeb::escritura(padre, e))?;
        }
        // Se escribe en un temporal hermano y se renombra: nunca queda a medias.
        let mut temporal = ruta.as_os_str().to_owned();
        temporal.push(".tmp");
        let temporal = std::path::PathBuf::from(temporal);
        std::fs::write(&temporal, texto).map_err(|e| ErrorWeb::escritura(&temporal, e))?;
        std::fs::rename(&temporal, ruta).map_err(|e| {
            let _ = std::fs::remove_file(&temporal);
            ErrorWeb::escritura(ruta, e)
        })
    }

    /// Clave de `sumas` de un artefacto.
    pub fn clave_suma(etiqueta: &str, artefacto: &str) -> String {
        format!("{etiqueta}/{artefacto}")
    }

    /// Topes de lo que llega de fuera: nada pasa de aquí sin que pare.
    fn comprobar_topes(&self, origen: &str) -> Resultado<()> {
        let maximo = PAGINAS_MAXIMAS as usize * POR_PAGINA;
        if self.releases.len() > maximo {
            return Err(ErrorWeb::dato_raro(
                origen,
                format!(
                    "hay {} releases y el máximo son {maximo}",
                    self.releases.len()
                ),
            ));
        }
        for release in &self.releases {
            comprobar_release(release, origen)?;
        }
        if self.sumas.len() > maximo {
            return Err(ErrorWeb::dato_raro(
                origen,
                format!("hay {} sumas y el máximo son {maximo}", self.sumas.len()),
            ));
        }
        for (clave, texto) in &self.sumas {
            if clave.len() > CAMPO_MAXIMO || texto.len() > TOPE_SHA256 {
                return Err(ErrorWeb::dato_raro(
                    origen,
                    format!(
                        "la suma «{}» pasa de los topes (clave {CAMPO_MAXIMO} bytes, texto {TOPE_SHA256})",
                        texto_seguro(clave, 80)
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// Topes de una release de la API: activos y longitud de cada campo.
fn comprobar_release(release: &ReleaseApi, origen: &str) -> Resultado<()> {
    let etiqueta = texto_seguro(&release.tag_name, 60);
    let largo = |campo: &str, valor: &str, tope: usize| {
        if valor.len() > tope {
            Err(ErrorWeb::dato_raro(
                origen,
                format!("release {etiqueta}: {campo} pasa de {tope} bytes"),
            ))
        } else {
            Ok(())
        }
    };
    largo("tag_name", &release.tag_name, CAMPO_MAXIMO)?;
    largo("html_url", &release.html_url, CAMPO_MAXIMO)?;
    if let Some(fecha) = &release.published_at {
        largo("published_at", fecha, CAMPO_MAXIMO)?;
    }
    if let Some(cuerpo) = &release.body {
        largo("body", cuerpo, CUERPO_MAXIMO)?;
    }
    if release.assets.len() > ACTIVOS_MAXIMOS {
        return Err(ErrorWeb::dato_raro(
            origen,
            format!(
                "release {etiqueta}: trae {} activos y el máximo son {ACTIVOS_MAXIMOS}",
                release.assets.len()
            ),
        ));
    }
    for activo in &release.assets {
        largo("el nombre de un activo", &activo.name, CAMPO_MAXIMO)?;
        largo("el estado de un activo", &activo.state, CAMPO_MAXIMO)?;
        largo(
            "la URL de un activo",
            &activo.browser_download_url,
            CAMPO_MAXIMO,
        )?;
    }
    Ok(())
}

/// De dónde salen las páginas de la API y los `.sha256`: `gh` o `curl` en
/// producción, ficheros en las pruebas.
pub trait Fuente {
    /// Cuerpo JSON de la página `pagina` (desde 1) de `releases?per_page=100`.
    fn pagina_releases(&self, pagina: u32) -> Resultado<String>;
    /// Texto del `.sha256` publicado en `url` (siempre `https://github.com/...`).
    fn suma(&self, url: &str) -> Resultado<String>;
}

/// Argumentos de `gh` para la página `pagina` de las releases. El token no va
/// aquí: `gh` lo toma de `GH_TOKEN` o `GITHUB_TOKEN` del entorno heredado.
pub(crate) fn argumentos_gh(pagina: u32) -> Vec<String> {
    vec![
        "api".to_string(),
        format!("repos/{REPOSITORIO}/releases?per_page={POR_PAGINA}&page={pagina}"),
    ]
}

/// Comprueba que `url` es la de un `.sha256` de una release del repositorio:
/// `https://github.com/HectorEspejo/magi/releases/download/<etiqueta>/<nombre>.sha256`
/// con solo letras, cifras, `.`, `_` y `-`.
fn validar_url_suma(url: &str) -> Resultado<()> {
    let raro = |que: String| ErrorWeb::dato_raro("URL de un .sha256", que);
    let Some(resto) = url.strip_prefix(PREFIJO_SUMAS) else {
        return Err(raro(format!(
            "«{}» no empieza por {PREFIJO_SUMAS}",
            texto_seguro(url, 120)
        )));
    };
    let mut partes = resto.split('/');
    let (Some(etiqueta), Some(nombre), None) = (partes.next(), partes.next(), partes.next()) else {
        return Err(raro(format!(
            "«{}» no es <etiqueta>/<fichero>",
            texto_seguro(url, 120)
        )));
    };
    // Sin punto delante: ni `.` ni `..` (que `curl` normalizaría saliendo de
    // `download/`) ni un nombre que empiece por `-`.
    let sencillo = |t: &str| {
        !t.is_empty()
            && !t.starts_with(['.', '-'])
            && !t.contains("..")
            && t.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    if !sencillo(etiqueta) || !sencillo(nombre) || !nombre.ends_with(".sha256") {
        return Err(raro(format!(
            "«{}» no es la URL de un .sha256 con un nombre sencillo",
            texto_seguro(url, 120)
        )));
    }
    Ok(())
}

/// Argumentos de `curl` para bajar un `.sha256`: solo `https`, también en las
/// redirecciones, con plazo y tope de tamaño. `-q` va primero para que `curl` no
/// lea ningún `.curlrc`.
pub(crate) fn argumentos_curl(url: &str) -> Resultado<Vec<String>> {
    validar_url_suma(url)?;
    Ok([
        "-q",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--tlsv1.2",
        "-fsSL",
        "--max-time",
        "30",
        "--max-filesize",
        &TOPE_CURL_SUMA.to_string(),
        url,
    ]
    .iter()
    .map(|a| a.to_string())
    .collect())
}

/// El error de una orden que terminó con un código distinto de 0: el programa, el
/// código y el principio de su salida de error (sin caracteres de control).
fn orden_fallida(programa: &str, salida: &SalidaOrden) -> ErrorWeb {
    let codigo = salida
        .codigo
        .map_or_else(|| "una señal".to_string(), |c| format!("el código {c}"));
    let detalle = texto_seguro(&String::from_utf8_lossy(&salida.error), 300);
    ErrorWeb::Orden {
        orden: programa.to_string(),
        motivo: format!("terminó con {codigo}: {}", detalle.trim()),
    }
}

fn utf8(bytes: Vec<u8>, donde: &str) -> Resultado<String> {
    String::from_utf8(bytes).map_err(|_| ErrorWeb::dato_raro(donde, "la respuesta no es UTF-8"))
}

/// `gh api` para la API (con `GH_TOKEN` por entorno) y `curl` para los `.sha256`.
#[derive(Debug, Default)]
pub struct FuenteGh;

impl Fuente for FuenteGh {
    fn pagina_releases(&self, pagina: u32) -> Resultado<String> {
        if pagina == 0 || pagina > PAGINAS_MAXIMAS {
            return Err(ErrorWeb::dato_raro(
                "API de GitHub",
                format!("la página {pagina} no está entre 1 y {PAGINAS_MAXIMAS}"),
            ));
        }
        let args = argumentos_gh(pagina);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let salida = ejecutar_con_plazo("gh", &args, &[], PLAZO_RED, TOPE_PAGINA_API)?;
        if !salida.exito() {
            return Err(orden_fallida("gh", &salida));
        }
        utf8(salida.salida, &format!("API de GitHub (página {pagina})"))
    }

    fn suma(&self, url: &str) -> Resultado<String> {
        let args = argumentos_curl(url)?;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let salida = ejecutar_con_plazo("curl", &args, &[], PLAZO_SUMA, TOPE_SHA256)?;
        if !salida.exito() {
            return Err(orden_fallida("curl", &salida));
        }
        utf8(
            salida.salida,
            &format!(".sha256 {}", texto_seguro(url, 120)),
        )
    }
}

/// Fuente de pruebas y de `--sin-red`: páginas y sumas ya leídas.
#[derive(Debug, Default, Clone)]
pub struct FuenteFijos {
    pub paginas: Vec<String>,
    /// `url del .sha256` → texto.
    pub sumas: BTreeMap<String, String>,
}

impl Fuente for FuenteFijos {
    fn pagina_releases(&self, pagina: u32) -> Resultado<String> {
        usize::try_from(pagina)
            .ok()
            .and_then(|p| p.checked_sub(1))
            .and_then(|i| self.paginas.get(i))
            .cloned()
            .ok_or_else(|| {
                ErrorWeb::dato_raro(
                    "fuente de datos fijos",
                    format!("falta la página {pagina} de la API"),
                )
            })
    }

    fn suma(&self, url: &str) -> Resultado<String> {
        self.sumas.get(url).cloned().ok_or_else(|| {
            ErrorWeb::dato_raro(
                "fuente de datos fijos",
                format!("falta el .sha256 de {}", texto_seguro(url, 120)),
            )
        })
    }
}

/// Parsea una página de la API: tiene que ser una lista JSON de releases. Un
/// objeto (`{"message": "API rate limit exceeded…"}`) falla diciendo su mensaje.
fn parsear_pagina(texto: &str, pagina: u32) -> Resultado<Vec<ReleaseApi>> {
    let donde = format!("API de GitHub (página {pagina})");
    if texto.len() > TOPE_PAGINA_API {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("la página pasa del tope de {TOPE_PAGINA_API} bytes"),
        ));
    }
    let valor: Value = serde_json::from_str(texto).map_err(|e| {
        ErrorWeb::dato_raro(
            &donde,
            format!("JSON no válido: {}", texto_seguro(&e.to_string(), 300)),
        )
    })?;
    let lista = match valor {
        Value::Array(lista) => lista,
        Value::Object(objeto) => {
            let mensaje = objeto
                .get("message")
                .and_then(Value::as_str)
                .map(|m| texto_seguro(m, 200))
                .unwrap_or_else(|| "sin mensaje".to_string());
            return Err(ErrorWeb::dato_raro(
                donde,
                format!(
                    "se esperaba una lista de releases y la API devolvió un objeto: «{mensaje}»"
                ),
            ));
        }
        _ => {
            return Err(ErrorWeb::dato_raro(
                donde,
                "se esperaba una lista de releases y llegó otra cosa",
            ))
        }
    };
    let mut releases = Vec::with_capacity(lista.len());
    for (i, elemento) in lista.into_iter().enumerate() {
        let release: ReleaseApi = serde_json::from_value(elemento).map_err(|e| {
            ErrorWeb::dato_raro(
                &donde,
                format!(
                    "la release número {} no es válida: {}",
                    i + 1,
                    texto_seguro(&e.to_string(), 300)
                ),
            )
        })?;
        comprobar_release(&release, &donde)?;
        releases.push(release);
    }
    Ok(releases)
}

/// Lee todas las páginas de la API, elige las versiones con
/// `descargas::elegir_versiones` y baja los `.sha256` de los artefactos elegidos.
/// Se detiene en la primera página vacía o corta y falla pasadas `PAGINAS_MAXIMAS`.
pub fn descargar_releases(
    fuente: &dyn Fuente,
    plataformas: &Plataformas,
) -> Resultado<DocumentoReleases> {
    // 1. Las páginas, hasta una vacía o corta.
    let mut releases: Vec<ReleaseApi> = Vec::new();
    let mut terminada = false;
    for pagina in 1..=PAGINAS_MAXIMAS {
        let texto = fuente.pagina_releases(pagina)?;
        let de_la_pagina = parsear_pagina(&texto, pagina)?;
        let llena = de_la_pagina.len() >= POR_PAGINA;
        releases.extend(de_la_pagina);
        if !llena {
            terminada = true;
            break;
        }
    }
    if !terminada {
        return Err(ErrorWeb::dato_raro(
            "API de GitHub",
            format!(
                "hay más de {PAGINAS_MAXIMAS} páginas de releases ({} releases) y no se siguen pidiendo",
                PAGINAS_MAXIMAS as usize * POR_PAGINA
            ),
        ));
    }

    // 2. Qué versiones salen y qué `.sha256` hay que bajar.
    let mut doc = DocumentoReleases {
        releases,
        sumas: BTreeMap::new(),
    };
    let seleccion = elegir_versiones(&doc, plataformas)?;
    let versiones = [
        Some(&seleccion.escritorio),
        seleccion.termux.as_ref(),
        seleccion.app.as_ref(),
    ];
    let mut sumas = BTreeMap::new();
    for version in versiones.into_iter().flatten() {
        let Some(release) = release_de(&doc, &version.etiqueta) else {
            continue;
        };
        for plataforma in plataformas.del_canal(version.canal) {
            let nombre = plataforma.nombre_artefacto(&version.numero);
            let nombre_suma = format!("{nombre}.sha256");
            let donde = format!("releases de GitHub (release {})", version.etiqueta);
            if activo_unico(release, &nombre)?.is_none() {
                return Err(ErrorWeb::dato_raro(
                    donde,
                    format!("falta el artefacto «{}»", texto_seguro(&nombre, 80)),
                ));
            }
            let Some(activo_suma) = activo_unico(release, &nombre_suma)? else {
                // El .deb de Termux sin su .sha256 no se elige (la versión cae a la
                // anterior); en los demás canales, un artefacto sin suma para todo.
                if version.canal == Canal::Termux {
                    continue;
                }
                return Err(ErrorWeb::dato_raro(
                    donde,
                    format!("falta «{}»", texto_seguro(&nombre_suma, 80)),
                ));
            };
            let url = url_descarga(&version.etiqueta, &nombre_suma);
            if activo_suma.browser_download_url != url {
                return Err(ErrorWeb::dato_raro(
                    donde,
                    format!(
                        "la URL de «{}» es «{}» y no la esperada «{url}»",
                        texto_seguro(&nombre_suma, 80),
                        texto_seguro(&activo_suma.browser_download_url, 200)
                    ),
                ));
            }
            let texto = fuente.suma(&url)?;
            if texto.len() > TOPE_SHA256 {
                return Err(ErrorWeb::dato_raro(
                    donde,
                    format!(
                        "«{}» pesa {} bytes y el tope son {TOPE_SHA256}",
                        texto_seguro(&nombre_suma, 80),
                        texto.len()
                    ),
                ));
            }
            sumas.insert(
                DocumentoReleases::clave_suma(&version.etiqueta, &nombre),
                texto,
            );
        }
    }
    doc.sumas = sumas;

    // 3. El cuerpo de una release solo se lee para la huella de la app: de las
    // demás no se guarda.
    let etiqueta_app = seleccion.app.as_ref().map(|a| a.etiqueta.as_str());
    for release in &mut doc.releases {
        if Some(release.tag_name.as_str()) != etiqueta_app {
            release.body = None;
        }
    }

    // 4. Se valida todo, como hará `construir`: la construcción para aquí, no allí.
    elegir(&doc, plataformas, OpcionesDescargas::default())?;
    Ok(doc)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const URL_BUENA: &str = "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/magi-ssh-x86_64-unknown-linux-gnu.tar.xz.sha256";

    #[test]
    fn los_argumentos_de_gh_son_la_api_y_la_pagina_sin_token_ni_shell() {
        let args = argumentos_gh(3);
        assert_eq!(
            args,
            [
                "api",
                "repos/HectorEspejo/magi/releases?per_page=100&page=3"
            ]
        );
        // El token no puede ir aquí: gh lo toma del entorno.
        let junto = args.join(" ").to_lowercase();
        for prohibido in ["token", "bearer", "authorization", "ghp_", "-h", "sh -c"] {
            assert!(!junto.contains(prohibido), "{prohibido} en {junto}");
        }
    }

    #[test]
    fn los_argumentos_de_curl_solo_admiten_https_con_plazo_y_tope() {
        let args = argumentos_curl(URL_BUENA).unwrap();
        assert_eq!(args.first().map(String::as_str), Some("-q"), "sin .curlrc");
        assert_eq!(args.last().map(String::as_str), Some(URL_BUENA));
        let junto = args.join(" ");
        for esperado in [
            "--proto =https",
            "--proto-redir =https",
            "--tlsv1.2",
            "-fsSL",
            "--max-time 30",
            "--max-filesize 4096",
        ] {
            assert!(junto.contains(esperado), "falta «{esperado}» en {junto}");
        }
        // Ni un argumento con espacios, comillas o metacaracteres: no hay shell.
        for a in &args {
            assert!(
                !a.contains(|c: char| c.is_whitespace() || "'\"`$;|&<>()".contains(c)),
                "argumento sospechoso «{a}»"
            );
        }
        // Y nada de secretos.
        assert!(!junto.to_lowercase().contains("token"));
    }

    #[test]
    fn una_url_que_no_es_la_de_un_sha256_del_repositorio_no_llega_a_curl() {
        for mala in [
            "",
            "http://github.com/HectorEspejo/magi/releases/download/v1.6.1/a.sha256",
            "https://github.com/otro/magi/releases/download/v1.6.1/a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a.tar.xz",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a.sha256?x=1",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a.sha256#x",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/../a.sha256",
            // Dos componentes, pero uno que sale de `download/` o que se normaliza.
            "https://github.com/HectorEspejo/magi/releases/download/../a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/./a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/..sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/-v1/a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/-a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1..6/a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/dir/a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download//a.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a b.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a.sha256\n",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/-o.sha256 -o /x",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a;b.sha256",
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a%2e%2e.sha256",
            "https://github.com@ejemplo.org/HectorEspejo/magi/releases/download/v1.6.1/a.sha256",
            "https://ejemplo.org/https://github.com/HectorEspejo/magi/releases/download/v1.6.1/a.sha256",
            "-o/tmp/x",
        ] {
            assert!(
                matches!(argumentos_curl(mala), Err(ErrorWeb::DatoRaro { .. })),
                "«{mala:?}» debía rechazarse"
            );
        }
        assert!(argumentos_curl(URL_BUENA).is_ok());
        // `.sha256` de la app y del .deb.
        assert!(argumentos_curl(
            "https://github.com/HectorEspejo/magi/releases/download/android-v0.2.0/magi-android-v0.2.0.apk.sha256"
        )
        .is_ok());
        assert!(argumentos_curl(
            "https://github.com/HectorEspejo/magi/releases/download/v1.6.1/magi-ssh_1.6.1_aarch64.deb.sha256"
        )
        .is_ok());
    }
}
