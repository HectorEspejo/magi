//! tmux del host: nombres, argumentos del script y lectura de su salida
//! (Fase 15, ampliado en la Fase 16).
//!
//! Todo pasa por `scripts/terminal_tmux.sh`, embebido con `include_str!`: el
//! nombre se valida en Rust y en el script y va siempre como argumento, nunca
//! interpolado en una orden de tmux.
//!
//! Desde la Fase 16 las órdenes son las de [`OrdenTmux`] (`comprobar`,
//! `crear`, `enganchar`, `estado`, `vista`, `historial`, `renombrar` y
//! `terminar`, con servidor propio o habitual) y lo que devuelven se lee con
//! `sesiones_persistentes::lectura`, que lo trata como entrada no fiable.
//!
//! LEGADO-S2: [`Suborden`], [`orden`], [`leer_listado`] y [`leer_terminar`]
//! son las subórdenes de la Fase 15 (`abrir`, `listar`, y `comprobar` y
//! `terminar` con un solo argumento, sobre el tmux habitual). El servidor del
//! escritorio las usa hasta el Sprint 2; después se quitan.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::modelo::normalizar_busqueda;
use crate::sesiones_persistentes::{ServidorTmux, VersionTmux};

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

/// Subordenes del script de la Fase 15.
///
/// LEGADO-S2: las usa `src/servidor/tmux.rs` hasta el Sprint 2; las órdenes
/// nuevas son [`OrdenTmux`].
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
///
/// LEGADO-S2: solo la forma de la Fase 15 (sobre el tmux habitual y con un
/// solo argumento). Las nuevas, con `OrdenTmux::argumentos`.
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

// ---------------------------------------------------------------- Fase 16

/// Bytes máximos de la carpeta de una sesión (el mismo tope que el script).
pub const MAXIMO_CARPETA: usize = 1024;
/// Bytes máximos del comando de arranque (el mismo tope que el script).
pub const MAXIMO_COMANDO: usize = 4096;
/// Líneas máximas de `history-limit` que acepta el script.
pub const MAXIMO_LINEAS_HISTORIAL_TMUX: u32 = 1_000_000;
/// Líneas máximas que se piden a `vista` y a `historial`.
pub const MAXIMO_LINEAS_CAPTURA: u32 = 100_000;
/// Columnas de la sesión nueva: mínimo y máximo.
pub const COLUMNAS_TMUX: std::ops::RangeInclusive<u16> = 2..=1000;
/// Filas de la sesión nueva: mínimo y máximo.
pub const FILAS_TMUX: std::ops::RangeInclusive<u16> = 1..=2000;

/// Códigos de salida del script (§5.1). Los de `crear` van además en
/// `MAGI_RESULTADO`; los de `enganchar` son la única señal de cómo acabó.
pub const SALIDA_USO: i32 = 64;
pub const SALIDA_SIN_CARPETA: i32 = 72;
pub const SALIDA_EXISTE: i32 = 73;
pub const SALIDA_ANTIGUO: i32 = 74;
pub const SALIDA_NO_EXISTE: i32 = 75;
pub const SALIDA_SIN_TMUX: i32 = 76;
pub const SALIDA_SOLTADA: i32 = 100;

/// Plazo de las órdenes que hablan con el host (T32).
pub const PLAZO_ORDEN: Duration = Duration::from_secs(10);
/// Plazo de `vista` y de `historial` (T32).
pub const PLAZO_CAPTURA: Duration = Duration::from_secs(5);

/// Etiqueta del equipo en `@magi_equipo`: `^[a-z0-9-]{1,20}$`.
#[must_use]
pub fn equipo_valido(equipo: &str) -> bool {
    !equipo.is_empty()
        && equipo.len() <= MAXIMO_ETIQUETA
        && equipo
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Carpeta que acepta `crear`: absoluta o `~/…`, hasta 1024 bytes y sin
/// saltos de línea ni bytes nulos. La vacía no es una carpeta (es «sin
/// carpeta»): quien la pasa usa `None`.
#[must_use]
pub fn carpeta_valida(carpeta: &str) -> bool {
    (carpeta.starts_with('/') || carpeta.starts_with("~/"))
        && carpeta.len() <= MAXIMO_CARPETA
        && !carpeta.contains(['\n', '\r', '\0'])
}

/// Comando de arranque que acepta `crear`: texto libre de hasta 4096 bytes y
/// sin bytes nulos. Es el comando del usuario, como un snippet: no se escapa
/// porque va como dato (`$1`) del envoltorio del script.
#[must_use]
pub fn comando_valido(comando: &str) -> bool {
    comando.len() <= MAXIMO_COMANDO && !comando.contains('\0')
}

/// Versión de tmux a partir del texto de `tmux -V` o de `MAGI_TMUX=`:
/// `tmux 3.3a`, `tmux next-3.6` o `tmux master`. La letra no cuenta; un tmux
/// de desarrollo sin número (`master`, `openbsd-…`) se trata como el más
/// nuevo. `None` si no hay número.
#[must_use]
pub fn version_de_texto(texto: &str) -> Option<VersionTmux> {
    let texto = texto.trim();
    let resto = texto
        .strip_prefix("tmux")
        .map_or(texto, |resto| resto.trim_start());
    let resto = resto.strip_prefix("next-").unwrap_or(resto);
    if resto.starts_with("master") || resto.starts_with("openbsd-") {
        return Some(VersionTmux::nueva(99, 0));
    }
    let digitos = |texto: &str| -> String {
        texto
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
    };
    let mayor_texto = digitos(resto);
    if mayor_texto.is_empty() || mayor_texto.len() > 4 {
        return None;
    }
    let mayor = mayor_texto.parse::<u32>().ok()?;
    let menor = match resto[mayor_texto.len()..].strip_prefix('.') {
        Some(despues) => {
            let menor_texto = digitos(despues);
            if menor_texto.is_empty() {
                0
            } else if menor_texto.len() > 4 {
                return None;
            } else {
                menor_texto.parse::<u32>().ok()?
            }
        }
        None => 0,
    };
    Some(VersionTmux::nueva(mayor, menor))
}

/// Cómo acabó `enganchar` (§7.4), según el código de salida del script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinEnganchar {
    /// 0: la sesión terminó.
    Terminada,
    /// 100: la sesión sigue viva; a este cliente lo soltó otro.
    Soltada,
    /// 75: la sesión no existía.
    NoExiste,
    /// 76: el host no tiene tmux.
    SinTmux,
    /// 74: tmux es anterior a 3.0.
    Antiguo,
    /// Cualquier otro código: el cliente de tmux no pudo engancharse.
    Error(i32),
}

/// Interpreta el código de salida de `enganchar`.
#[must_use]
pub fn fin_de_enganchar(codigo: i32) -> FinEnganchar {
    match codigo {
        0 => FinEnganchar::Terminada,
        SALIDA_SOLTADA => FinEnganchar::Soltada,
        SALIDA_NO_EXISTE => FinEnganchar::NoExiste,
        SALIDA_SIN_TMUX => FinEnganchar::SinTmux,
        SALIDA_ANTIGUO => FinEnganchar::Antiguo,
        otro => FinEnganchar::Error(otro),
    }
}

/// Una orden del script de la Fase 16, ya con sus datos. Sus argumentos se
/// validan aquí igual que en el script; los datos del usuario (carpeta,
/// comando) van siempre como argumentos sueltos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrdenTmux<'a> {
    /// `comprobar <servidor> <nombre>`: versión de tmux y si existe.
    Comprobar {
        servidor: ServidorTmux,
        nombre: &'a str,
    },
    /// `crear`: la sesión en el tmux propio, sin engancharse.
    Crear {
        nombre: &'a str,
        carpeta: Option<&'a str>,
        comando: Option<&'a str>,
        /// `history-limit`, igual a `[terminal] historial_lineas`.
        lineas_historial: u32,
        columnas: u16,
        filas: u16,
    },
    /// `enganchar`: va en el PTY de la pestaña.
    Enganchar {
        servidor: ServidorTmux,
        nombre: &'a str,
        /// `attach -d`: soltar a los demás (sin compartir).
        soltar: bool,
        /// Etiqueta de este equipo (`@magi_equipo`).
        equipo: &'a str,
    },
    /// `estado`: las sesiones de los dos servidores y la hora del host.
    Estado,
    /// `vista`: las últimas líneas visibles, en texto plano.
    Vista {
        servidor: ServidorTmux,
        nombre: &'a str,
        lineas: u32,
    },
    /// `historial`: el historial con colores.
    Historial {
        servidor: ServidorTmux,
        nombre: &'a str,
        lineas: u32,
    },
    /// `renombrar`: solo en el tmux propio.
    Renombrar { nombre: &'a str, nuevo: &'a str },
    /// `terminar`.
    Terminar {
        servidor: ServidorTmux,
        nombre: &'a str,
    },
}

impl OrdenTmux<'_> {
    /// Nombre de la suborden en el script.
    #[must_use]
    pub fn suborden(&self) -> &'static str {
        match self {
            OrdenTmux::Comprobar { .. } => "comprobar",
            OrdenTmux::Crear { .. } => "crear",
            OrdenTmux::Enganchar { .. } => "enganchar",
            OrdenTmux::Estado => "estado",
            OrdenTmux::Vista { .. } => "vista",
            OrdenTmux::Historial { .. } => "historial",
            OrdenTmux::Renombrar { .. } => "renombrar",
            OrdenTmux::Terminar { .. } => "terminar",
        }
    }

    /// Plazo de la orden (T32). `enganchar` no tiene: dura lo que la pestaña.
    #[must_use]
    pub fn plazo(&self) -> Option<Duration> {
        match self {
            OrdenTmux::Enganchar { .. } => None,
            OrdenTmux::Vista { .. } | OrdenTmux::Historial { .. } => Some(PLAZO_CAPTURA),
            _ => Some(PLAZO_ORDEN),
        }
    }

    /// Argumentos para ejecutar el script: `sh -c <script> magi-tmux
    /// <suborden> <argumentos…>`. `None` si algún dato no es válido (el
    /// nombre, la carpeta, el comando, los números, el equipo…).
    #[must_use]
    pub fn argumentos(&self) -> Option<Vec<String>> {
        let mut datos: Vec<String> = Vec::new();
        match self {
            OrdenTmux::Comprobar { servidor, nombre }
            | OrdenTmux::Terminar { servidor, nombre } => {
                if !nombre_valido(nombre) {
                    return None;
                }
                datos.push(servidor.texto().to_string());
                datos.push((*nombre).to_string());
            }
            OrdenTmux::Crear {
                nombre,
                carpeta,
                comando,
                lineas_historial,
                columnas,
                filas,
            } => {
                let carpeta = carpeta.unwrap_or("");
                let comando = comando.unwrap_or("");
                if !nombre_valido(nombre)
                    || (!carpeta.is_empty() && !carpeta_valida(carpeta))
                    || !comando_valido(comando)
                    || *lineas_historial > MAXIMO_LINEAS_HISTORIAL_TMUX
                    || !COLUMNAS_TMUX.contains(columnas)
                    || !FILAS_TMUX.contains(filas)
                {
                    return None;
                }
                datos.push((*nombre).to_string());
                datos.push(carpeta.to_string());
                datos.push(comando.to_string());
                datos.push(lineas_historial.to_string());
                datos.push(columnas.to_string());
                datos.push(filas.to_string());
            }
            OrdenTmux::Enganchar {
                servidor,
                nombre,
                soltar,
                equipo,
            } => {
                if !nombre_valido(nombre) || !equipo_valido(equipo) {
                    return None;
                }
                datos.push(servidor.texto().to_string());
                datos.push((*nombre).to_string());
                datos.push(if *soltar { "1" } else { "0" }.to_string());
                datos.push((*equipo).to_string());
            }
            OrdenTmux::Estado => {}
            OrdenTmux::Vista {
                servidor,
                nombre,
                lineas,
            }
            | OrdenTmux::Historial {
                servidor,
                nombre,
                lineas,
            } => {
                if !nombre_valido(nombre) || !(1..=MAXIMO_LINEAS_CAPTURA).contains(lineas) {
                    return None;
                }
                datos.push(servidor.texto().to_string());
                datos.push((*nombre).to_string());
                datos.push(lineas.to_string());
            }
            OrdenTmux::Renombrar { nombre, nuevo } => {
                if !nombre_valido(nombre) || !nombre_valido(nuevo) {
                    return None;
                }
                datos.push((*nombre).to_string());
                datos.push((*nuevo).to_string());
            }
        }
        let mut argumentos = vec![
            "sh".to_string(),
            "-c".to_string(),
            SCRIPT.to_string(),
            "magi-tmux".to_string(),
            self.suborden().to_string(),
        ];
        argumentos.extend(datos);
        Some(argumentos)
    }

    /// La línea que se escribe en el PTY de una pestaña para ejecutar la orden
    /// con `exec`, de modo que el código de salida de la pestaña sea el del
    /// script (`enganchar`: 0, 100 o 75). Cada argumento va escapado para el
    /// shell de acceso; nada se interpola.
    #[must_use]
    pub fn linea_para_pty(&self) -> Option<String> {
        let argumentos = self.argumentos()?;
        let linea = argumentos
            .iter()
            .map(|argumento| shell_escape::unix::escape(argumento.as_str().into()).to_string())
            .collect::<Vec<String>>()
            .join(" ");
        Some(format!("exec {linea}\n"))
    }
}
