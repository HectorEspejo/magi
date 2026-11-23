//! Entorno de plantillas `minijinja` (`web/plantillas/`).
//!
//! Reglas de seguridad (informe §8):
//! - autoescape activo en todo `.html`;
//! - `UndefinedBehavior::Strict`: una clave que falta rompe la construcción en
//!   vez de salir vacía;
//! - `|safe` solo para HTML que genera esta herramienta (capturas, detalle de un
//!   CHANGELOG ya saneado), nunca para un dato de la API;
//! - los nombres de plantilla los fija el código, nunca vienen de fuera;
//! - las plantillas no llevan ni atributos `style` ni `<script>`.
//!
//! Sin cargador del sistema de ficheros: las plantillas se leen una vez, aquí, y
//! `{% include %}` y `{% extends %}` solo ven las que se cargaron (los nombres
//! son rutas relativas a `dir` con `/`: `base.html`, `parciales/pie.html`).
//!
//! El escapado es el de minijinja salvo la barra: minijinja escribe `/` como
//! `&#x2f;`, que no protege de nada dentro de un atributo entrecomillado y llena
//! de entidades todo `href`. Aquí se escapan `& < > " '` y nada más.
//!
//! El filtro `con_codigo` convierte los trozos entre acentos graves de un texto de
//! `web/textos/` en `<code>`: escapa todo lo demás y solo se usa con textos del
//! repositorio, nunca con un dato de la API.

use std::error::Error as _;
use std::fs;
use std::path::Path;

use minijinja::value::{Value, ValueKind};
use minijinja::{AutoEscape, Environment, Output, State, UndefinedBehavior};

use crate::error::{ErrorWeb, Resultado};

/// Niveles de carpetas como mucho bajo `web/plantillas/`.
const PROFUNDIDAD_MAXIMA: usize = 6;

pub struct Plantillas {
    pub(crate) entorno: minijinja::Environment<'static>,
}

impl Plantillas {
    /// Carga todo `*.html` de `dir` (recursivo) con su ruta relativa como nombre:
    /// `base.html`, `descargas.html`, `parciales/cabecera.html`…
    pub fn cargar(dir: &Path) -> Resultado<Self> {
        let mut entorno = Environment::new();
        entorno.set_undefined_behavior(UndefinedBehavior::Strict);
        entorno.set_auto_escape_callback(|_nombre| AutoEscape::Html);
        entorno.set_formatter(formateador);
        // Las etiquetas de bloque no dejan líneas en blanco en el HTML.
        entorno.set_trim_blocks(true);
        entorno.set_lstrip_blocks(true);
        entorno.add_filter("con_codigo", con_codigo);

        let mut ficheros = Vec::new();
        recoger_html(dir, "", 0, &mut ficheros)?;
        if ficheros.is_empty() {
            return Err(ErrorWeb::dato_raro(
                dir.display().to_string(),
                "no hay ninguna plantilla .html",
            ));
        }
        ficheros.sort();
        for (nombre, ruta) in ficheros {
            let contenido = fs::read_to_string(&ruta).map_err(|e| ErrorWeb::lectura(&ruta, e))?;
            entorno
                .add_template_owned(nombre.clone(), contenido)
                .map_err(|e| ErrorWeb::plantilla(nombre, detalle(&e)))?;
        }
        Ok(Self { entorno })
    }

    /// Renderiza `nombre` con `contexto` (un objeto JSON).
    pub fn renderizar(&self, nombre: &str, contexto: &serde_json::Value) -> Resultado<String> {
        if !contexto.is_object() {
            return Err(ErrorWeb::plantilla(
                nombre,
                "el contexto tiene que ser un objeto JSON",
            ));
        }
        let plantilla = self
            .entorno
            .get_template(nombre)
            .map_err(|e| ErrorWeb::plantilla(nombre, detalle(&e)))?;
        plantilla
            .render(contexto)
            .map_err(|e| ErrorWeb::plantilla(nombre, detalle(&e)))
    }
}

/// Error de minijinja con su causa y el trozo de plantilla (con `debug`).
fn detalle(error: &minijinja::Error) -> String {
    let mut texto = format!("{error:#}");
    let mut causa = error.source();
    while let Some(c) = causa {
        texto.push_str(&format!("\n  causado por: {c}"));
        causa = c.source();
    }
    texto
}

/// `(nombre con «/», ruta en disco)` de cada `.html` bajo `dir`. No sigue enlaces
/// simbólicos: una plantilla es del repositorio, no de otro sitio.
fn recoger_html(
    dir: &Path,
    prefijo: &str,
    profundidad: usize,
    salida: &mut Vec<(String, std::path::PathBuf)>,
) -> Resultado<()> {
    if profundidad > PROFUNDIDAD_MAXIMA {
        return Err(ErrorWeb::dato_raro(
            dir.display().to_string(),
            "demasiados niveles de carpetas de plantillas",
        ));
    }
    let entradas = fs::read_dir(dir).map_err(|e| ErrorWeb::lectura(dir, e))?;
    for entrada in entradas {
        let entrada = entrada.map_err(|e| ErrorWeb::lectura(dir, e))?;
        let ruta = entrada.path();
        let nombre = entrada.file_name().into_string().map_err(|_| {
            ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "el nombre del fichero no es UTF-8",
            )
        })?;
        let tipo = entrada
            .file_type()
            .map_err(|e| ErrorWeb::lectura(&ruta, e))?;
        if tipo.is_symlink() {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "las plantillas no pueden ser enlaces simbólicos",
            ));
        }
        if tipo.is_dir() {
            recoger_html(
                &ruta,
                &format!("{prefijo}{nombre}/"),
                profundidad + 1,
                salida,
            )?;
        } else if tipo.is_file() && nombre.ends_with(".html") {
            salida.push((format!("{prefijo}{nombre}"), ruta));
        }
    }
    Ok(())
}

/// Escapa lo que tiene significado en HTML, también dentro de un atributo.
pub fn escapar_html(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '&' => salida.push_str("&amp;"),
            '<' => salida.push_str("&lt;"),
            '>' => salida.push_str("&gt;"),
            '"' => salida.push_str("&quot;"),
            '\'' => salida.push_str("&#x27;"),
            otro => salida.push(otro),
        }
    }
    salida
}

/// HTML de un texto con trozos `` `así` ``: lo de fuera, escapado; lo de dentro,
/// escapado y en `<code>`. Un acento grave sin pareja es texto normal.
pub fn texto_con_codigo(texto: &str) -> String {
    let trozos: Vec<&str> = texto.split('`').collect();
    // Con un número par de trozos sobra un acento grave al final: sin pareja.
    let con_pareja = if trozos.len() % 2 == 1 {
        trozos.len()
    } else {
        trozos.len() - 1
    };
    let mut salida = String::with_capacity(texto.len() + 16);
    for (indice, trozo) in trozos.iter().enumerate() {
        if indice < con_pareja && indice % 2 == 1 {
            salida.push_str("<code>");
            salida.push_str(&escapar_html(trozo));
            salida.push_str("</code>");
        } else {
            if indice >= con_pareja && indice > 0 {
                salida.push('`');
            }
            salida.push_str(&escapar_html(trozo));
        }
    }
    salida
}

/// Lo que se escribe por cada `{{ … }}`: igual que el formateador de minijinja
/// (lo seguro, los números, `none` y lo indefinido pasan tal cual y lo indefinido
/// sigue fallando con `Strict`), pero con `escapar_html` para todo lo demás.
fn formateador(salida: &mut Output, estado: &State, valor: &Value) -> Result<(), minijinja::Error> {
    let sin_escapar = estado.auto_escape() != AutoEscape::Html
        || valor.is_safe()
        || matches!(
            valor.kind(),
            ValueKind::Undefined | ValueKind::None | ValueKind::Bool | ValueKind::Number
        );
    if sin_escapar {
        return minijinja::escape_formatter(salida, estado, valor);
    }
    let texto = match valor.as_str() {
        Some(texto) => escapar_html(texto),
        None => escapar_html(&valor.to_string()),
    };
    salida.write_str(&texto).map_err(minijinja::Error::from)
}

fn con_codigo(texto: &str) -> Value {
    Value::from_safe_string(texto_con_codigo(texto))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_trozos_entre_acentos_graves_van_en_code() {
        assert_eq!(
            texto_con_codigo("copia `magi` a tu PATH"),
            "copia <code>magi</code> a tu PATH"
        );
        assert_eq!(
            texto_con_codigo("`a` y `b`"),
            "<code>a</code> y <code>b</code>"
        );
    }

    #[test]
    fn todo_se_escapa_dentro_y_fuera_del_code() {
        assert_eq!(
            texto_con_codigo("<b>`<i>`&\"'"),
            "&lt;b&gt;<code>&lt;i&gt;</code>&amp;&quot;&#x27;"
        );
    }

    #[test]
    fn un_acento_grave_sin_pareja_es_texto() {
        assert_eq!(texto_con_codigo("a `b"), "a `b");
        assert_eq!(texto_con_codigo("`a` y `b"), "<code>a</code> y `b");
        assert_eq!(texto_con_codigo("`"), "`");
        assert_eq!(texto_con_codigo(""), "");
        assert_eq!(texto_con_codigo("``"), "<code></code>");
    }
}
