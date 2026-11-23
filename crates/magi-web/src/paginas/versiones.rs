//! Versiones (`/versiones/` y `/en/releases/`): los dos CHANGELOG.
//!
//! El detalle de cada versión es lo único que la plantilla inserta sin escapar:
//! lo produce `versiones::detalle_a_html`, y aquí se vuelve a mirar (solo las
//! etiquetas y atributos que ese módulo puede escribir, y los enlaces solo
//! `https://`) por si alguien arma una `EntradaChangelog` a mano. La marca de
//! «seguro» se pone aquí, en Rust, justo después de esa comprobación, y no con
//! `|safe` en la plantilla (ninguna plantilla marca nada como seguro: lo exige
//! `tests/plantillas.rs`, igual que la portada). Todo lo demás (resúmenes,
//! números, fechas) lo escapa la plantilla.

use std::collections::{BTreeMap, HashSet};

use minijinja::Value;
use serde_json::Value as Json;

use crate::comprobar::analizar_https;
use crate::error::{ErrorWeb, Resultado};
use crate::html::{self, Token};
use crate::paginas::{contexto_comun, fecha_legible, texto_necesario, Entorno};
use crate::rutas::Idioma;
use crate::rutas::Pagina;
use crate::versiones::{fecha_valida, numero_valido, EntradaChangelog};

/// La página de una release de GitHub cuelga de aquí: `…/tag/v1.6.1`.
const PREFIJO_RELEASES: &str = "https://github.com/HectorEspejo/magi/releases/tag/";

/// Etiquetas que `versiones::detalle_a_html` puede escribir.
const ETIQUETAS_DEL_DETALLE: &[&str] = &[
    "p",
    "ul",
    "ol",
    "li",
    "em",
    "strong",
    "del",
    "code",
    "pre",
    "a",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "br",
    "hr",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
];

/// Lo que enseña la página.
#[derive(Debug, Clone, Default)]
pub struct DatosVersiones {
    /// Escritorio: del CHANGELOG de la etiqueta de la última versión publicada.
    pub escritorio: Vec<EntradaChangelog>,
    /// App: `None` mientras no haya ninguna release de la app («aún sin publicar»).
    pub app: Option<Vec<EntradaChangelog>>,
}

#[derive(Clone, Copy)]
enum Canal {
    Escritorio,
    App,
}

impl Canal {
    /// Identificador del canal: el del ancla de su sección y el de sus textos.
    fn id(self) -> &'static str {
        match self {
            Canal::Escritorio => "escritorio",
            Canal::App => "app",
        }
    }

    /// Prefijo de la etiqueta de sus releases: `v1.6.1` o `android-v0.2.0`.
    fn prefijo_etiqueta(self) -> &'static str {
        match self {
            Canal::Escritorio => "v",
            Canal::App => "android-v",
        }
    }
}

/// Cada versión con su resumen en el idioma de la página, el detalle (en
/// castellano) dentro de un `<details>` («Details (in Spanish)» en inglés) y el
/// enlace a su release en GitHub (`v<número>` o `android-v<número>`). Sin la
/// sección «Sin publicar». La app sin releases lo dice con «aún sin publicar».
pub fn renderizar(e: &Entorno<'_>, idioma: Idioma, datos: &DatosVersiones) -> Resultado<String> {
    let secciones = vec![
        seccion(e, idioma, Canal::Escritorio, Some(&datos.escritorio))?,
        seccion(e, idioma, Canal::App, datos.app.as_deref())?,
    ];

    // El contexto común es JSON; lo de la página, valores de minijinja, para poder
    // marcar el detalle como seguro (lo único).
    let mut raiz: BTreeMap<String, Value> = match contexto_comun(e, Pagina::Versiones, idioma) {
        Json::Object(mapa) => mapa
            .iter()
            .map(|(clave, valor)| (clave.clone(), Value::from_serialize(valor)))
            .collect(),
        _ => BTreeMap::new(),
    };
    let mut pagina: BTreeMap<String, Value> = BTreeMap::new();
    pagina.insert("secciones".to_string(), Value::from(secciones));
    raiz.insert("v".to_string(), Value::from(pagina));

    let plantilla = e
        .plantillas
        .entorno
        .get_template("versiones.html")
        .map_err(|error| ErrorWeb::plantilla("versiones.html", detalle_del_error(&error)))?;
    plantilla
        .render(Value::from(raiz))
        .map_err(|error| ErrorWeb::plantilla("versiones.html", detalle_del_error(&error)))
}

/// Error de minijinja con su causa y el trozo de plantilla.
fn detalle_del_error(error: &minijinja::Error) -> String {
    use std::error::Error as _;
    let mut texto = format!("{error:#}");
    let mut causa = error.source();
    while let Some(c) = causa {
        texto.push_str(&format!("\n  causado por: {c}"));
        causa = c.source();
    }
    texto
}

fn seccion(
    e: &Entorno<'_>,
    idioma: Idioma,
    canal: Canal,
    entradas: Option<&[EntradaChangelog]>,
) -> Resultado<Value> {
    let titulo = texto_necesario(e, idioma, &format!("versiones.{}", canal.id()))?;
    let vacia = texto_necesario(e, idioma, &format!("versiones.{}_vacia", canal.id()))?;
    let mut vistos = HashSet::new();
    let mut lista = Vec::new();
    for entrada in entradas.unwrap_or(&[]) {
        if !vistos.insert(entrada.numero.as_str()) {
            return Err(ErrorWeb::dato_raro(
                "página de versiones",
                format!(
                    "la versión {} del canal {} está repetida",
                    entrada.numero,
                    canal.id()
                ),
            ));
        }
        lista.push(entrada_json(e, idioma, canal, &titulo, entrada)?);
    }
    let mut mapa: BTreeMap<String, Value> = BTreeMap::new();
    mapa.insert("id".to_string(), Value::from(canal.id()));
    mapa.insert("titulo".to_string(), Value::from(titulo));
    mapa.insert("entradas".to_string(), Value::from(lista));
    mapa.insert("vacia".to_string(), Value::from(vacia));
    Ok(Value::from(mapa))
}

fn entrada_json(
    e: &Entorno<'_>,
    idioma: Idioma,
    canal: Canal,
    titulo_canal: &str,
    entrada: &EntradaChangelog,
) -> Resultado<Value> {
    // El número sale de una entrada ya validada, pero se vuelve a mirar: va en un
    // `id` y en una URL.
    if !numero_valido(&entrada.numero) {
        return Err(ErrorWeb::dato_raro(
            "página de versiones",
            format!(
                "el número de versión {:?} no es `X.Y.Z` (canal {})",
                entrada.numero,
                canal.id()
            ),
        ));
    }
    let donde = format!("página de versiones, {} {}", canal.id(), entrada.numero);

    let resumen = match idioma {
        Idioma::Es => &entrada.resumen_es,
        Idioma::En => &entrada.resumen_en,
    }
    .trim();
    if resumen.is_empty() {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("la versión no tiene resumen en «{}»", idioma.codigo()),
        ));
    }

    // Una fecha que no se puede mostrar se omite: no se inventa.
    let fecha = if fecha_valida(&entrada.fecha) {
        fecha_legible(e.textos, idioma, &entrada.fecha)
    } else {
        None
    };

    verificar_detalle(&entrada.detalle_html, &donde)?;

    let mut mapa: BTreeMap<String, Value> = BTreeMap::new();
    mapa.insert(
        "id".to_string(),
        Value::from(format!(
            "{}-{}",
            canal.id(),
            entrada.numero.replace('.', "-")
        )),
    );
    mapa.insert("titulo_canal".to_string(), Value::from(titulo_canal));
    mapa.insert("numero".to_string(), Value::from(entrada.numero.as_str()));
    mapa.insert(
        "fecha_iso".to_string(),
        Value::from(fecha.as_ref().map(|_| entrada.fecha.as_str())),
    );
    mapa.insert("fecha".to_string(), Value::from(fecha));
    mapa.insert(
        "url".to_string(),
        Value::from(format!(
            "{PREFIJO_RELEASES}{}{}",
            canal.prefijo_etiqueta(),
            entrada.numero
        )),
    );
    mapa.insert("resumen".to_string(), Value::from(resumen));
    mapa.insert(
        "hay_detalle".to_string(),
        Value::from(!entrada.detalle_html.trim().is_empty()),
    );
    // La única marca de «seguro» de la página: un detalle ya comprobado.
    mapa.insert(
        "detalle".to_string(),
        Value::from_safe_string(entrada.detalle_html.clone()),
    );
    Ok(Value::from(mapa))
}

/// Comprueba que el detalle es solo lo que `detalle_a_html` puede escribir: nada de
/// `<script>`, ni manejadores, ni `style`, ni comentarios, y los enlaces solo
/// `https://` bien formados, y que cada etiqueta que se abre se cierra (un `<a>` o
/// una tabla sin cerrar se tragaría el resto de la página). Falla cerrado ante HTML
/// que no puede interpretar.
fn verificar_detalle(detalle: &str, donde: &str) -> Resultado<()> {
    let error = |que: String| ErrorWeb::dato_raro(donde, format!("el detalle: {que}"));
    let tokens = html::tokenizar(detalle).map_err(|e| error(format!("no se puede leer: {e}")))?;
    // Etiquetas abiertas, en orden. `br` y `hr` no se cierran; en cualquier otra,
    // `<x/>` no es un cierre en HTML y cuenta como una apertura.
    let mut abiertas: Vec<String> = Vec::new();
    for token in tokens {
        let etiqueta = match token {
            Token::Texto { .. } => continue,
            Token::Etiqueta(etiqueta) => etiqueta,
            otro => {
                return Err(error(format!(
                    "lleva algo que no es texto ni etiqueta: {otro:?}"
                )))
            }
        };
        if !ETIQUETAS_DEL_DETALLE.contains(&etiqueta.nombre.as_str()) {
            return Err(error(format!(
                "la etiqueta <{}> no se admite",
                etiqueta.nombre
            )));
        }
        let vacia = matches!(etiqueta.nombre.as_str(), "br" | "hr");
        if etiqueta.cierre {
            if !etiqueta.atributos.is_empty() {
                return Err(error(format!("</{}> lleva atributos", etiqueta.nombre)));
            }
            if vacia || abiertas.pop().as_deref() != Some(etiqueta.nombre.as_str()) {
                return Err(error(format!(
                    "</{}> no cierra la etiqueta que está abierta",
                    etiqueta.nombre
                )));
            }
            continue;
        }
        if !vacia {
            abiertas.push(etiqueta.nombre.clone());
        }
        for (nombre, valor) in &etiqueta.atributos {
            let valor = valor.as_deref().unwrap_or("");
            match (etiqueta.nombre.as_str(), nombre.as_str()) {
                ("a", "href") => {
                    let destino = html::decodificar_atributo(valor)
                        .ok_or_else(|| error("un href con una entidad rara".to_string()))?;
                    if analizar_https(&destino).is_none() {
                        return Err(error(format!("un enlace que no es https:// {destino:?}")));
                    }
                }
                ("ol", "start") => {
                    if valor.is_empty()
                        || valor.len() > 18
                        || !valor.bytes().all(|b| b.is_ascii_digit())
                    {
                        return Err(error(format!("un start raro {valor:?}")));
                    }
                }
                (etiqueta, atributo) => {
                    return Err(error(format!(
                        "<{etiqueta}> lleva el atributo «{atributo}»"
                    )));
                }
            }
        }
    }
    if let Some(sin_cerrar) = abiertas.last() {
        return Err(error(format!("<{sin_cerrar}> se queda sin cerrar")));
    }
    Ok(())
}
