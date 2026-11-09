//! tmux del host: nombres, argumentos del script y lectura de su salida
//! (Fase 15).
//!
//! Todo pasa por `scripts/terminal_tmux.sh`, embebido con `include_str!`: el
//! nombre se valida en Rust y en el script y va siempre como argumento, nunca
//! interpolado en una orden de tmux. Lo que devuelve `listar` es entrada no
//! fiable: 200 sesiones como mucho y líneas de 1 KiB.

use serde::{Deserialize, Serialize};

use crate::modelo::normalizar_busqueda;

/// El script embebido.
pub const SCRIPT: &str = include_str!("../../scripts/terminal_tmux.sh");

/// Nombres admitidos: letras, números, guion y guion bajo, hasta 64.
#[must_use]
pub fn nombre_valido(nombre: &str) -> bool {
    !nombre.is_empty()
        && nombre.len() <= 64
        && nombre
            .chars()
            .all(|caracter| caracter.is_ascii_alphanumeric() || caracter == '-' || caracter == '_')
}

/// Largo máximo de la etiqueta del equipo en los nombres nuevos.
const MAXIMO_ETIQUETA: usize = 20;

/// Etiqueta del equipo para los nombres nuevos: minúsculas, sin tildes, con
/// guiones y 20 caracteres como mucho.
#[must_use]
pub fn etiqueta_equipo(nombre_equipo: &str) -> String {
    // La misma normalización que las búsquedas: sin tildes y en minúsculas.
    let normalizado = normalizar_busqueda(nombre_equipo);
    let mut etiqueta = String::with_capacity(normalizado.len());
    let mut ultimo_guion = false;
    for caracter in normalizado.chars() {
        if caracter.is_ascii_lowercase() || caracter.is_ascii_digit() {
            etiqueta.push(caracter);
            ultimo_guion = false;
        } else if !ultimo_guion {
            etiqueta.push('-');
            ultimo_guion = true;
        }
    }
    let etiqueta: String = etiqueta
        .trim_matches('-')
        .chars()
        .take(MAXIMO_ETIQUETA)
        .collect();
    // El recorte a 20 puede dejar un guion al final.
    let etiqueta = etiqueta.trim_end_matches('-');
    if etiqueta.is_empty() {
        "equipo".to_string()
    } else {
        etiqueta.to_string()
    }
}

/// Nombre libre `magi-<equipo>-<n>` para una sesión nueva.
#[must_use]
pub fn nombre_nuevo(equipo: &str, existentes: &[String]) -> String {
    let prefijo = format!("magi-{}-", etiqueta_equipo(equipo));
    let mut usados = Vec::new();
    for existente in existentes {
        let Some(sufijo) = existente.strip_prefix(&prefijo) else {
            continue;
        };
        if !sufijo.is_empty() && sufijo.chars().all(|caracter| caracter.is_ascii_digit()) {
            if let Ok(numero) = sufijo.parse::<u32>() {
                usados.push(numero);
            }
        }
    }
    let mut numero = 1;
    while usados.contains(&numero) {
        numero += 1;
    }
    format!("{prefijo}{numero}")
}

/// Subordenes del script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suborden {
    Comprobar,
    Abrir,
    Listar,
    Terminar,
}

impl Suborden {
    #[must_use]
    pub fn como_texto(self) -> &'static str {
        match self {
            Suborden::Comprobar => "comprobar",
            Suborden::Abrir => "abrir",
            Suborden::Listar => "listar",
            Suborden::Terminar => "terminar",
        }
    }
}

/// Orden para ejecutar el script por un canal `exec`:
/// `sh -c <script> magi-tmux <suborden> [<nombre>]`. `None` si el nombre no
/// es válido, falta o sobra.
#[must_use]
pub fn orden(suborden: Suborden, nombre: Option<&str>) -> Option<Vec<String>> {
    let nombre = match suborden {
        Suborden::Listar => {
            // Listar no lleva nombre: el script rechaza los argumentos de más.
            if nombre.is_some() {
                return None;
            }
            None
        }
        Suborden::Comprobar | Suborden::Abrir | Suborden::Terminar => {
            let nombre = nombre?;
            if !nombre_valido(nombre) {
                return None;
            }
            Some(nombre.to_string())
        }
    };
    let mut argumentos = vec![
        "sh".to_string(),
        "-c".to_string(),
        SCRIPT.to_string(),
        "magi-tmux".to_string(),
        suborden.como_texto().to_string(),
    ];
    if let Some(nombre) = nombre {
        argumentos.push(nombre);
    }
    Some(argumentos)
}

/// Una sesión de tmux del host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SesionTmux {
    pub nombre: String,
    pub enganchada: bool,
    pub ventanas: u16,
    /// Descripción de cuándo se creó (la da tmux).
    pub creada: String,
}

/// Lo que devuelve `listar`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listado {
    /// Versión de tmux, si la hay.
    pub tmux: Option<String>,
    pub sesiones: Vec<SesionTmux>,
}

/// Límites de la entrada no fiable de `listar` (§5.1).
const MAXIMO_SESIONES: usize = 200;
const MAXIMO_NOMBRE: usize = 64;
const MAXIMO_LINEA: usize = 1024;
const MAXIMO_VERSION: usize = 64;
const MAXIMO_CREADA: usize = 128;

/// Lee la salida de `listar` (entrada no fiable).
#[must_use]
pub fn leer_listado(salida: &str) -> Listado {
    let mut listado = Listado::default();
    for linea in salida.lines() {
        let linea = linea.strip_suffix('\r').unwrap_or(linea);
        if linea.len() > MAXIMO_LINEA {
            continue;
        }
        if let Some(valor) = linea.strip_prefix("MAGI_TMUX=") {
            let version = limpiar(valor, MAXIMO_VERSION);
            listado.tmux = if version.is_empty() || version == "no" {
                None
            } else {
                Some(version)
            };
            continue;
        }
        let Some(resto) = linea.strip_prefix("MAGI_SESION") else {
            continue;
        };
        let Some(resto) = resto.strip_prefix('\t').or_else(|| resto.strip_prefix(' ')) else {
            continue;
        };
        if listado.sesiones.len() >= MAXIMO_SESIONES {
            continue;
        }
        let campos: Vec<&str> = resto.split('\t').collect();
        if campos.len() != 4 {
            continue;
        }
        let nombre = campos[0];
        if nombre.is_empty()
            || nombre.chars().count() > MAXIMO_NOMBRE
            || nombre.chars().any(char::is_control)
        {
            continue;
        }
        let enganchada = campos[1]
            .trim()
            .parse::<u32>()
            .map(|clientes| clientes > 0)
            .unwrap_or(false);
        let ventanas = campos[2].trim().parse::<u16>().unwrap_or(0);
        listado.sesiones.push(SesionTmux {
            nombre: nombre.to_string(),
            enganchada,
            ventanas,
            creada: limpiar(campos[3], MAXIMO_CREADA),
        });
    }
    listado
}

/// Quita los controles y recorta: lo que llega del host se pinta en la lista.
fn limpiar(texto: &str, maximo: usize) -> String {
    texto
        .trim()
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(maximo)
        .collect()
}

/// Resultado de `terminar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoTerminar {
    Ok,
    NoExiste,
}

/// Lee la salida de `terminar`. Solo `MAGI_RESULTADO=ok` cuenta como
/// terminada; cualquier otra cosa (o una salida ilegible) es `NoExiste`.
#[must_use]
pub fn leer_terminar(salida: &str) -> ResultadoTerminar {
    for linea in salida.lines() {
        let linea = linea.strip_suffix('\r').unwrap_or(linea).trim();
        if linea == "MAGI_RESULTADO=ok" {
            return ResultadoTerminar::Ok;
        }
    }
    ResultadoTerminar::NoExiste
}
