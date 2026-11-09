//! tmux del host: nombres, argumentos del script y lectura de su salida
//! (Fase 15).
//!
//! Todo pasa por `scripts/terminal_tmux.sh`, embebido con `include_str!`: el
//! nombre se valida en Rust y en el script y va siempre como argumento, nunca
//! interpolado en una orden de tmux. Lo que devuelve `listar` es entrada no
//! fiable: 200 sesiones como mucho y líneas de 1 KiB.

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

/// Etiqueta del equipo para los nombres nuevos: minúsculas, sin tildes, con
/// guiones y 20 caracteres como mucho.
#[must_use]
pub fn etiqueta_equipo(nombre_equipo: &str) -> String {
    unimplemented!("tmux: lo implementa la pista del núcleo")
}

/// Nombre libre `magi-<equipo>-<n>` para una sesión nueva.
#[must_use]
pub fn nombre_nuevo(equipo: &str, existentes: &[String]) -> String {
    unimplemented!("tmux: lo implementa la pista del núcleo")
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
/// es válido o falta.
#[must_use]
pub fn orden(suborden: Suborden, nombre: Option<&str>) -> Option<Vec<String>> {
    let mut argumentos = vec![
        "sh".to_string(),
        "-c".to_string(),
        SCRIPT.to_string(),
        "magi-tmux".to_string(),
        suborden.como_texto().to_string(),
    ];
    if let Some(nombre) = nombre {
        if !nombre_valido(nombre) {
            return None;
        }
        argumentos.push(nombre.to_string());
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

/// Lee la salida de `listar` (entrada no fiable).
#[must_use]
pub fn leer_listado(salida: &str) -> Listado {
    unimplemented!("tmux: lo implementa la pista del núcleo")
}

/// Resultado de `terminar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoTerminar {
    Ok,
    NoExiste,
}

/// Lee la salida de `terminar`.
#[must_use]
pub fn leer_terminar(salida: &str) -> ResultadoTerminar {
    unimplemented!("tmux: lo implementa la pista del núcleo")
}

use serde::{Deserialize, Serialize};
