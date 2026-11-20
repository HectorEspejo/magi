//! tmux del host: nombres, argumentos del script y lectura de su salida
//! (Fase 15, reescrito en la Fase 16).
//!
//! Todo pasa por dos scripts embebidos con `include_str!`: el nombre se valida
//! en Rust y en el script y va siempre como argumento, nunca interpolado en una
//! orden de tmux.
//!
//! - `scripts/terminal_tmux.sh` ([`SCRIPT`]) lo hace casi todo y viaja por la
//!   ENTRADA ESTÁNDAR del `exec`, leído entero antes de ejecutar nada
//!   ([`ARRANQUE`]): en la línea de órdenes solo van el arranque mínimo y los
//!   argumentos validados, porque servidores SSH como Dropbear limitan su
//!   longitud (D357).
//! - `scripts/terminal_enganche.sh` ([`SCRIPT_ENGANCHE`]) es el enganche con
//!   PTY, que no puede recibir nada por la entrada estándar: va en la línea de
//!   órdenes, y por eso es corto.
//!
//! Las órdenes son las de [`OrdenTmux`] (`comprobar`, `crear`, `preparar`,
//! `enganchar`, `estado`, `vista`, `historial`, `renombrar` y `terminar`, con
//! servidor propio o habitual) y lo que devuelven se lee con
//! `sesiones_persistentes::lectura`, que lo trata como entrada no fiable. Las
//! reglas de nombre, carpeta y comando son las de
//! `sesiones_persistentes::validadores`: aquí solo se envuelven.

use std::time::Duration;

use crate::modelo::normalizar_busqueda;
use crate::sesiones_persistentes::validadores;
use crate::sesiones_persistentes::{ServidorTmux, VersionTmux};

/// El script grande, que viaja por la entrada estándar del `exec`.
pub const SCRIPT: &str = include_str!("../../scripts/terminal_tmux.sh");

/// El enganche mínimo con PTY: va en la línea de órdenes del `exec` (con PTY no
/// hay entrada estándar que lo lleve), así que se mantiene de pocos KB.
pub const SCRIPT_ENGANCHE: &str = include_str!("../../scripts/terminal_enganche.sh");

/// Lo que acaba el [`SCRIPT`]: el arranque no lo ejecuta si no termina en esta
/// marca (un script cortado no se ejecuta a medias).
pub const MARCA_FIN: &str = "#MAGI-FIN";

/// El arranque mínimo de las subórdenes sin PTY: lee el script entero de la
/// entrada estándar (`cat` no acaba hasta el fin de la entrada) y solo entonces
/// lo ejecuta, si está completo. Con `sh -c '<arranque>' magi-tmux <suborden>
/// <argumentos…>` los argumentos quedan en `$@` para el script. Sale con 70
/// si no llega el script.
pub const ARRANQUE: &str =
    "S=$(cat) || exit 70; case $S in *'#MAGI-FIN') eval \"$S\" ;; *) exit 70 ;; esac";

/// Nombres admitidos: letras, números, guion y guion bajo, hasta 64 (la regla
/// es la de `sesiones_persistentes::validadores`).
#[must_use]
pub fn nombre_valido(nombre: &str) -> bool {
    validadores::nombre_valido(nombre)
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
pub const MAXIMO_CARPETA: usize = validadores::MAXIMO_CARPETA_BYTES;
/// Bytes máximos del comando de arranque (el mismo tope que el script).
pub const MAXIMO_COMANDO: usize = validadores::MAXIMO_COMANDO_BYTES;
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
/// El arranque no recibió el script entero (nunca lo ejecuta a medias).
pub const SALIDA_SIN_SCRIPT: i32 = 70;
pub const SALIDA_SOLTADA: i32 = 100;
/// Reconexión automática: la sesión la tiene otro equipo y no se tocó (R157).
pub const SALIDA_OTRO_EQUIPO: i32 = 101;

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
/// saltos de línea ni bytes nulos (la regla es la de
/// `validadores::validar_carpeta`). La vacía no es una carpeta (es «sin
/// carpeta»): quien la pasa usa `None`.
#[must_use]
pub fn carpeta_valida(carpeta: &str) -> bool {
    !carpeta.is_empty() && validadores::validar_carpeta(carpeta).is_ok()
}

/// Comando de arranque que acepta `crear`: texto libre de hasta 4096 bytes y
/// sin bytes nulos (la regla es la de `validadores::validar_comando`). Es el
/// comando del usuario, como un snippet: no se escapa porque va como dato
/// (`$1`) del envoltorio del script.
#[must_use]
pub fn comando_valido(comando: &str) -> bool {
    validadores::validar_comando(comando).is_ok()
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
    /// 101: la sesión la tiene otro equipo y la reconexión automática no se
    /// enganchó (R157): la pestaña queda soltada y nadie perdió nada.
    OtroEquipo,
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
        SALIDA_OTRO_EQUIPO => FinEnganchar::OtroEquipo,
        SALIDA_NO_EXISTE => FinEnganchar::NoExiste,
        SALIDA_SIN_TMUX => FinEnganchar::SinTmux,
        SALIDA_ANTIGUO => FinEnganchar::Antiguo,
        otro => FinEnganchar::Error(otro),
    }
}

/// Una orden de los scripts de tmux, ya con sus datos. Sus argumentos se
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
    /// `preparar <servidor> <nombre>`: la primera mitad de engancharse, por la
    /// entrada estándar. Comprueba que la sesión existe, aplica la
    /// configuración de MAGI (solo en el servidor propio) y devuelve su
    /// identificador interno.
    Preparar {
        servidor: ServidorTmux,
        nombre: &'a str,
    },
    /// El enganche mínimo con PTY (`terminal_enganche.sh`): la segunda mitad de
    /// engancharse. Es la única orden que no va por la entrada estándar.
    Enganchar {
        servidor: ServidorTmux,
        nombre: &'a str,
        /// `attach -d`: soltar a los demás (sin compartir).
        soltar: bool,
        /// Etiqueta de este equipo (`@magi_equipo`).
        equipo: &'a str,
        /// Reconexión automática: solo engancharse si la sesión está libre o es
        /// de este equipo; si la tiene otro, sale con 101 sin tocar nada (R157).
        guardar: bool,
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

/// El `exec` de un canal: cada argumento escapado para el shell de acceso del
/// host (el `exec` de ssh pasa la línea por él) y unidos con espacios. Nada se
/// interpola: los datos van como argumentos.
#[must_use]
pub fn linea_de_exec(argumentos: &[String]) -> String {
    argumentos
        .iter()
        .map(|argumento| shell_escape::unix::escape(argumento.as_str().into()).to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

/// Lo que hay que mandar por la entrada estándar del `exec` de estos
/// argumentos (los de [`OrdenTmux::argumentos`]): el [`SCRIPT`] si arrancan con
/// el [`ARRANQUE`] y nada si son el enganche con PTY. Quien ejecuta la orden lo
/// envía entero con el canal ya en marcha y cierra la entrada después.
#[must_use]
pub fn entrada_del_exec(argumentos: &[String]) -> Option<&'static str> {
    (argumentos.get(2).map(String::as_str) == Some(ARRANQUE)).then_some(SCRIPT)
}

impl OrdenTmux<'_> {
    /// Nombre de la suborden en el script.
    #[must_use]
    pub fn suborden(&self) -> &'static str {
        match self {
            OrdenTmux::Comprobar { .. } => "comprobar",
            OrdenTmux::Crear { .. } => "crear",
            OrdenTmux::Preparar { .. } => "preparar",
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

    /// Lo que se manda por la entrada estándar del `exec`: el script grande en
    /// todas las órdenes menos en `enganchar`, que lleva su script mínimo en la
    /// línea de órdenes.
    #[must_use]
    pub fn entrada(&self) -> Option<&'static str> {
        match self {
            OrdenTmux::Enganchar { .. } => None,
            _ => Some(SCRIPT),
        }
    }

    /// Los datos de la orden, ya validados y sin la suborden. `None` si algún
    /// dato no es válido (el nombre, la carpeta, el comando, los números, el
    /// equipo…).
    fn datos(&self) -> Option<Vec<String>> {
        let mut datos: Vec<String> = Vec::new();
        match self {
            OrdenTmux::Comprobar { servidor, nombre }
            | OrdenTmux::Preparar { servidor, nombre }
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
                guardar,
            } => {
                if !nombre_valido(nombre) || !equipo_valido(equipo) {
                    return None;
                }
                datos.push(servidor.texto().to_string());
                datos.push((*nombre).to_string());
                datos.push(if *soltar { "1" } else { "0" }.to_string());
                datos.push((*equipo).to_string());
                datos.push(if *guardar { "1" } else { "0" }.to_string());
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
        Some(datos)
    }

    /// Argumentos para ejecutar la orden en el host. Con la entrada estándar
    /// (todas menos `enganchar`): `sh -c <arranque> magi-tmux <suborden>
    /// <argumentos…>`, y el script va aparte, en [`Self::entrada`]. El
    /// enganche, con su script mínimo: `sh -c <script> magi-enganche
    /// <argumentos…>`. `None` si algún dato no es válido.
    #[must_use]
    pub fn argumentos(&self) -> Option<Vec<String>> {
        let datos = self.datos()?;
        let mut argumentos = match self {
            OrdenTmux::Enganchar { .. } => vec![
                "sh".to_string(),
                "-c".to_string(),
                SCRIPT_ENGANCHE.to_string(),
                "magi-enganche".to_string(),
            ],
            _ => vec![
                "sh".to_string(),
                "-c".to_string(),
                ARRANQUE.to_string(),
                "magi-tmux".to_string(),
                self.suborden().to_string(),
            ],
        };
        argumentos.extend(datos);
        Some(argumentos)
    }

    /// El comando para el `exec` de un canal (`argumentos` escapados para el
    /// shell de acceso del host). Es como se engancha una pestaña
    /// (`enganchar`, con PTY): el script no se escribe en la pantalla ni deja
    /// eco, y el código de salida del canal es el del script. En las demás, la
    /// línea lleva solo el arranque y los datos, y el script va por la entrada
    /// estándar ([`Self::entrada`]).
    #[must_use]
    pub fn comando_exec(&self) -> Option<String> {
        Some(linea_de_exec(&self.argumentos()?))
    }
}
