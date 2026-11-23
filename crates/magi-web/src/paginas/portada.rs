//! Portada (`/` y `/en/`): qué es MAGI, la animación plegable y las seis capturas.
//!
//! Los fragmentos de las capturas (`web/capturas/<id>.html`) son lo único que la
//! plantilla inserta sin escapar. Salen del repositorio, nunca de la API, y pasan
//! por `capturas::validar_fragmento` dos veces: al leerlos (`Capturas::cargar`) y
//! justo antes de marcarlos como seguros al pintar la página, que es donde se
//! decide. La marca de «seguro» se pone aquí, en Rust, y no con `|safe` en la
//! plantilla: así ninguna plantilla puede marcar nada como seguro.
//!
//! Todo lo demás (la versión del botón, los estados, los textos) se escapa.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::Path;

use minijinja::Value;
use serde_json::{json, Value as Json};

use crate::capturas::{validar_css, validar_fragmento, IDS};
use crate::datos::{Canal, Plataforma};
use crate::descargas::EleccionDescargas;
use crate::error::{ErrorWeb, Resultado};
use crate::paginas::{contexto_comun, texto_necesario, Entorno};
use crate::rutas::{self, Idioma, Pagina};

/// Lo que más pesa un fragmento de captura (una de 100×30 con un estilo por celda
/// no llega a 100 KiB).
const TOPE_FRAGMENTO: u64 = 256 * 1024;
/// Lo que más pesa `capturas.css` (una clase por estilo distinto de las seis).
const TOPE_CSS: u64 = 2 * 1024 * 1024;
/// Caracteres como mucho de un número de versión que va en el botón.
const VERSION_MAXIMA: usize = 64;
/// Cómo se regeneran las capturas: sale en el error de una que falta.
const COMO_REGENERAR: &str = "ejecuta scripts/generar-capturas.sh";

/// Comprobación de un trozo de texto (un fragmento o la CSS): las de verdad son
/// `capturas::validar_fragmento` y `capturas::validar_css`.
pub type Validador<'a> = &'a dyn Fn(&str) -> Resultado<()>;

/// Los fragmentos de `web/capturas/<id>.html`, ya validados con
/// `capturas::validar_fragmento` (se insertan sin escapar).
#[derive(Debug, Clone, Default)]
pub struct Capturas {
    pub fragmentos: BTreeMap<String, String>,
}

impl Capturas {
    /// Lee y valida `<id>.html` de cada captura de `capturas::IDS` y `capturas.css`.
    ///
    /// Las seis son obligatorias: si falta una, el error dice cuál. No sigue
    /// enlaces simbólicos y no lee nada que pase de su tope.
    pub fn cargar(dir: &Path) -> Resultado<Self> {
        Self::cargar_con(dir, &validar_fragmento, &validar_css)
    }

    /// Como `cargar`, con los validadores a la vista (las pruebas pasan los suyos).
    #[doc(hidden)]
    pub fn cargar_con(
        dir: &Path,
        fragmento_valido: Validador<'_>,
        css_valida: Validador<'_>,
    ) -> Resultado<Self> {
        comprobar_carpeta(dir)?;
        let mut fragmentos = BTreeMap::new();
        for id in IDS {
            let ruta = dir.join(format!("{id}.html"));
            let texto = leer_acotado(&ruta, TOPE_FRAGMENTO, &format!("la captura «{id}»"))?;
            // El fichero acaba en salto de línea, como todo fichero de texto; el
            // fragmento (un `<pre>` y nada más) no. Se quita uno, no «los que haya».
            let fragmento = texto.strip_suffix('\n').unwrap_or(&texto);
            fragmento_valido(fragmento).map_err(|e| anotar(e, &ruta))?;
            fragmentos.insert(id.to_string(), fragmento.to_string());
        }
        let ruta_css = dir.join("capturas.css");
        let css = leer_acotado(&ruta_css, TOPE_CSS, "capturas.css")?;
        css_valida(&css).map_err(|e| anotar(e, &ruta_css))?;
        Ok(Self { fragmentos })
    }
}

/// La carpeta de las capturas existe y no es un enlace simbólico.
fn comprobar_carpeta(dir: &Path) -> Resultado<()> {
    let meta = match fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(ErrorWeb::dato_raro(
                dir.display().to_string(),
                format!("no existe la carpeta de las capturas ({COMO_REGENERAR})"),
            ));
        }
        Err(e) => return Err(ErrorWeb::lectura(dir, e)),
    };
    if meta.file_type().is_symlink() {
        return Err(ErrorWeb::dato_raro(
            dir.display().to_string(),
            "la carpeta de las capturas es un enlace simbólico",
        ));
    }
    if !meta.is_dir() {
        return Err(ErrorWeb::dato_raro(
            dir.display().to_string(),
            "las capturas tienen que estar en una carpeta",
        ));
    }
    Ok(())
}

/// Lee un fichero de texto del repositorio: tiene que existir, ser un fichero
/// normal (ni enlace simbólico ni carpeta), caber en `tope` y estar en UTF-8.
fn leer_acotado(ruta: &Path, tope: u64, que_es: &str) -> Resultado<String> {
    let donde = ruta.display().to_string();
    let meta = match fs::symlink_metadata(ruta) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(ErrorWeb::dato_raro(
                donde,
                format!("falta {que_es} ({COMO_REGENERAR})"),
            ));
        }
        Err(e) => return Err(ErrorWeb::lectura(ruta, e)),
    };
    if meta.file_type().is_symlink() {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("{que_es} es un enlace simbólico: no se sigue"),
        ));
    }
    if !meta.is_file() {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("{que_es} no es un fichero normal"),
        ));
    }
    if meta.len() > tope {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("{que_es} pesa {} bytes y el tope son {tope}", meta.len()),
        ));
    }
    let fichero = fs::File::open(ruta).map_err(|e| ErrorWeb::lectura(ruta, e))?;
    // Un byte de más para notar que el fichero ha crecido desde la comprobación.
    let mut bytes = Vec::new();
    fichero
        .take(tope + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ErrorWeb::lectura(ruta, e))?;
    if bytes.len() as u64 > tope {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("{que_es} pasa del tope de {tope} bytes"),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| ErrorWeb::dato_raro(donde, format!("{que_es} no está en UTF-8")))
}

/// Dice de qué fichero sale un fallo del validador.
fn anotar(error: ErrorWeb, ruta: &Path) -> ErrorWeb {
    match error {
        ErrorWeb::DatoRaro { donde, que } => ErrorWeb::DatoRaro {
            donde: format!("{} ({donde})", ruta.display()),
            que,
        },
        otro => ErrorWeb::dato_raro(ruta.display().to_string(), otro.to_string()),
    }
}

/// La portada: botón «Descargar X.Y.Z» con la versión del escritorio, «Leer el
/// manual», la animación dentro de `<details open>` (con `loading="lazy"`) y, con
/// `prefers-reduced-motion`, la captura de Flota en su lugar; seis figuras con
/// título, descripción (`role="img"` y `aria-label`) y el `<pre aria-hidden>`; sin
/// telemetría, licencia y enlaces a descargas, manual, GitHub, seguridad y contribuir.
pub fn renderizar(
    e: &Entorno<'_>,
    idioma: Idioma,
    eleccion: &EleccionDescargas,
    capturas: &Capturas,
) -> Resultado<String> {
    renderizar_con(e, idioma, eleccion, capturas, &validar_fragmento)
}

/// Como `renderizar`, con el validador de los fragmentos a la vista (las pruebas
/// pasan el suyo).
#[doc(hidden)]
pub fn renderizar_con(
    e: &Entorno<'_>,
    idioma: Idioma,
    eleccion: &EleccionDescargas,
    capturas: &Capturas,
    fragmento_valido: Validador<'_>,
) -> Resultado<String> {
    let desde = rutas::ruta(Pagina::Portada, idioma);
    let datos = datos_pagina(e, idioma, eleccion, desde)?;
    let figuras = figuras(e, idioma, capturas, fragmento_valido)?;
    // La captura de Flota (la primera de `IDS`) sustituye a la animación cuando el
    // visitante pide menos movimiento.
    let flota = figuras
        .first()
        .cloned()
        .ok_or_else(|| ErrorWeb::dato_raro("capturas (IDS)", "no hay ninguna captura"))?;

    let contexto = contexto_comun(e, Pagina::Portada, idioma);
    let mut raiz: BTreeMap<String, Value> = objeto_a_mapa(&contexto);
    let mut d = objeto_a_mapa(&datos);
    d.insert("capturas".to_string(), Value::from(figuras));
    d.insert("flota".to_string(), flota);
    raiz.insert("d".to_string(), Value::from(d));

    let plantilla = e
        .plantillas
        .entorno
        .get_template("portada.html")
        .map_err(|error| ErrorWeb::plantilla("portada.html", detalle(&error)))?;
    plantilla
        .render(Value::from(raiz))
        .map_err(|error| ErrorWeb::plantilla("portada.html", detalle(&error)))
}

/// Un objeto JSON como mapa de valores de minijinja (lo que no es un objeto, vacío).
fn objeto_a_mapa(valor: &Json) -> BTreeMap<String, Value> {
    match valor {
        Json::Object(mapa) => mapa
            .iter()
            .map(|(clave, v)| (clave.clone(), Value::from_serialize(v)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// Error de minijinja con su causa y el trozo de plantilla.
fn detalle(error: &minijinja::Error) -> String {
    use std::error::Error as _;
    let mut texto = format!("{error:#}");
    let mut causa = error.source();
    while let Some(c) = causa {
        texto.push_str(&format!("\n  causado por: {c}"));
        causa = c.source();
    }
    texto
}

/// Las seis figuras, en el orden de `IDS`. El fragmento se valida aquí, justo antes
/// de marcarlo como seguro: es lo único de la página que no se escapa.
fn figuras(
    e: &Entorno<'_>,
    idioma: Idioma,
    capturas: &Capturas,
    fragmento_valido: Validador<'_>,
) -> Resultado<Vec<Value>> {
    let mut figuras = Vec::with_capacity(IDS.len());
    for id in IDS {
        let titulo = texto_necesario(e, idioma, &format!("captura.{id}.titulo"))?;
        let descripcion = texto_necesario(e, idioma, &format!("captura.{id}.descripcion"))?;
        let Some(fragmento) = capturas.fragmentos.get(id) else {
            return Err(ErrorWeb::dato_raro(
                "web/capturas",
                format!("falta la captura «{id}» ({COMO_REGENERAR})"),
            ));
        };
        fragmento_valido(fragmento).map_err(|error| match error {
            ErrorWeb::DatoRaro { donde, que } => ErrorWeb::DatoRaro {
                donde: format!("web/capturas/{id}.html ({donde})"),
                que,
            },
            otro => ErrorWeb::dato_raro(format!("web/capturas/{id}.html"), otro.to_string()),
        })?;
        let mut figura: BTreeMap<String, Value> = BTreeMap::new();
        figura.insert("id".to_string(), Value::from(id));
        figura.insert("titulo".to_string(), Value::from(titulo));
        figura.insert("descripcion".to_string(), Value::from(descripcion));
        // La única marca de «seguro» de la página: un fragmento ya validado.
        figura.insert(
            "fragmento".to_string(),
            Value::from_safe_string(fragmento.clone()),
        );
        figuras.push(Value::from(figura));
    }
    Ok(figuras)
}

/// El número de una versión que va en el botón y en la lista: no vacío, corto y
/// sin caracteres de control. Lo demás se escapa en la plantilla.
fn numero_para_la_pagina<'a>(numero: &'a str, donde: &str) -> Resultado<&'a str> {
    if numero.trim().is_empty() {
        return Err(ErrorWeb::dato_raro(
            donde,
            "el número de versión está vacío",
        ));
    }
    if numero.chars().count() > VERSION_MAXIMA || numero.chars().any(char::is_control) {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!(
                "el número de versión no es de una versión (más de {VERSION_MAXIMA} caracteres o con caracteres de control)"
            ),
        ));
    }
    Ok(numero)
}

/// Familia de una plataforma: lo que va antes del primer `-` de su id (`linux`,
/// `macos`, `termux`, `android`).
fn familia(plataforma: &Plataforma) -> &str {
    plataforma
        .id
        .split('-')
        .next()
        .unwrap_or(plataforma.id.as_str())
}

/// Cuánto se fía uno del estado: la familia enseña el menos maduro de los suyos.
fn madurez(estado_id: &str) -> u8 {
    match estado_id {
        "en_pruebas" => 0,
        "beta" => 1,
        _ => 2,
    }
}

fn datos_pagina(
    e: &Entorno<'_>,
    idioma: Idioma,
    eleccion: &EleccionDescargas,
    desde: &str,
) -> Resultado<Json> {
    let version = numero_para_la_pagina(
        &eleccion.escritorio.numero,
        "descargas (versión del escritorio)",
    )?;

    // Plataformas por su orden, juntas por familia y canal (Linux, macOS, Termux y app).
    let mut ordenadas: Vec<&Plataforma> = e.plataformas.plataforma.iter().collect();
    ordenadas.sort_by(|a, b| a.orden.cmp(&b.orden).then_with(|| a.id.cmp(&b.id)));
    let mut familias: Vec<(String, Canal, &'static str)> = Vec::new();
    for plataforma in ordenadas {
        let familia_id = familia(plataforma).to_string();
        let estado = plataforma.estado.id();
        match familias
            .iter_mut()
            .find(|(f, canal, _)| *f == familia_id && *canal == plataforma.canal)
        {
            Some((_, _, actual)) => {
                if madurez(estado) < madurez(actual) {
                    *actual = estado;
                }
            }
            None => familias.push((familia_id, plataforma.canal, estado)),
        }
    }

    let sin_publicar = texto_necesario(e, idioma, "descargas.sin_publicar")?;
    let mut plataformas = Vec::new();
    for (familia_id, canal, estado) in familias {
        let numero = match canal {
            Canal::Escritorio => Some(&eleccion.escritorio),
            Canal::Termux => eleccion.termux.as_ref(),
            Canal::App => eleccion.app.as_ref(),
        };
        let version_texto = match numero {
            Some(v) => {
                numero_para_la_pagina(&v.numero, &format!("descargas (versión {})", v.etiqueta))?
                    .to_string()
            }
            None => sin_publicar.clone(),
        };
        plataformas.push(json!({
            "id": familia_id,
            "etiqueta": texto_necesario(e, idioma, &format!("descargas.grupo.{familia_id}"))?,
            "estado": estado,
            "estado_texto": texto_necesario(e, idioma, &format!("descargas.estado.{estado}"))?,
            "probado": estado != "estable",
            "version": version_texto,
        }));
    }

    Ok(json!({
        "version": version,
        "plataformas": plataformas,
        "enlaces": {
            "probado": rutas::relativa(desde, "manual/estado.html"),
        },
    }))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_numero_de_la_pagina_no_puede_estar_vacio_ni_ser_largo() {
        assert_eq!(numero_para_la_pagina("1.6.1", "x").unwrap(), "1.6.1");
        assert!(numero_para_la_pagina("", "x").is_err());
        assert!(numero_para_la_pagina("  ", "x").is_err());
        assert!(numero_para_la_pagina(&"9".repeat(65), "x").is_err());
        assert!(numero_para_la_pagina("1.6\n1", "x").is_err());
        // Lo raro que no es de control no se rechaza: se escapa en la plantilla.
        assert!(numero_para_la_pagina("1.6.1<b>", "x").is_ok());
    }

    #[test]
    fn la_madurez_ordena_los_estados() {
        assert_eq!(madurez("en_pruebas"), 0);
        assert!(madurez("beta") < madurez("estable"));
    }
}
