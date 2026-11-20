//! Lectura acotada de lo que devuelve `terminal_tmux.sh` (§5.1 y §7.7 del
//! informe de la Fase 16).
//!
//! Todo lo que llega del host es entrada no fiable y nunca provoca pánico:
//! `estado` trae como mucho 500 sesiones por host, con los campos separados por
//! 0x1F, líneas de hasta 1 KiB y títulos saneados de hasta 120 caracteres; la
//! vista previa llega a 64 KiB y el historial, a 8 MiB. Los 64 tramos por fila
//! del historial los aplica [`crate::terminal::historial::capturar`] cuando el
//! servidor pasa el texto por el emulador: no se duplican aquí.
//!
//! Nada de lo que se lee aquí va al log ni a REGISTRO (títulos, vistas y
//! historial son contenido de las sesiones).

use std::collections::HashSet;

use crate::sesiones_persistentes::{ServidorTmux, SesionVista, VersionTmux};
use crate::terminal::tmux::{
    equipo_valido, nombre_valido, version_de_texto, ResultadoTerminar, SALIDA_ANTIGUO,
    SALIDA_EXISTE, SALIDA_NO_EXISTE, SALIDA_SIN_CARPETA, SALIDA_SIN_TMUX,
};

/// Sesiones que se aceptan por host (los dos servidores juntos).
pub const MAXIMO_SESIONES_POR_HOST: usize = 500;
/// Bytes que puede tener una línea de `estado`; una más larga se descarta.
pub const MAXIMO_LINEA_ESTADO: usize = 1024;
/// Bytes de la salida de `estado` que se miran; el resto se ignora.
pub const MAXIMO_SALIDA_ESTADO: usize = 1024 * 1024;
/// Caracteres del título de una sesión.
pub const MAXIMO_TITULO: usize = 120;
/// Bytes de la vista previa (se quedan los últimos).
pub const MAXIMO_VISTA: usize = 64 * 1024;
/// Bytes del historial de tmux (se quedan los últimos).
pub const MAXIMO_HISTORIAL: usize = 8 * 1024 * 1024;

/// Separador de los campos de `MAGI_SES` (0x1F).
const SEPARADOR: char = '\u{1f}';
/// Campos de una línea `MAGI_SES` sin contar la etiqueta: servidor, nombre,
/// identificador, clientes, equipo, actividad, creada, campana, ventanas y
/// título.
const CAMPOS_SESION: usize = 10;

// ----------------------------------------------------------------- estado

/// Lo que dice `estado` de un host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LecturaEstado {
    /// Versión de tmux; `None` si el host no tiene tmux.
    pub tmux: Option<VersionTmux>,
    /// Hora del host (segundos Unix) al comprobar; 0 si no la dio. Con 0 nada
    /// pasa a «antigua» por error: la diferencia con la actividad sale negativa.
    pub ahora: i64,
    pub sesiones: Vec<SesionVista>,
}

/// Lee la salida de `estado` (stdout y stderr juntos, ya como texto).
///
/// Devuelve `None` si no hay ninguna línea `MAGI_TMUX=`: la salida no es la
/// del script (un aviso de acceso, un error de la conexión…). Sin tmux
/// (`MAGI_TMUX=no`) no hay sesiones. Las líneas que no encajan se descartan una
/// a una: línea larga, etiqueta distinta, campos de menos o de más, servidor
/// desconocido o nombre que MAGI no puede manejar. Un número ilegible vale 0 y
/// un equipo que no case con la etiqueta válida, `None`.
#[must_use]
pub fn leer_estado(salida: &str) -> Option<LecturaEstado> {
    let salida = prefijo_acotado(salida, MAXIMO_SALIDA_ESTADO);
    let mut visto_tmux = false;
    let mut tmux = None;
    let mut ahora = None;
    let mut sesiones: Vec<SesionVista> = Vec::new();
    let mut vistas: HashSet<(ServidorTmux, String)> = HashSet::new();
    for linea in salida.split('\n') {
        let linea = linea.strip_suffix('\r').unwrap_or(linea);
        if linea.len() > MAXIMO_LINEA_ESTADO {
            continue;
        }
        if let Some(valor) = linea.strip_prefix("MAGI_TMUX=") {
            if !visto_tmux {
                visto_tmux = true;
                tmux = version_de_texto(valor);
            }
            continue;
        }
        if let Some(valor) = linea.strip_prefix("MAGI_AHORA=") {
            if ahora.is_none() {
                ahora = Some(numero_i64(valor));
            }
            continue;
        }
        let Some(campos) = linea
            .strip_prefix("MAGI_SES")
            .and_then(|resto| resto.strip_prefix(SEPARADOR))
        else {
            continue;
        };
        if sesiones.len() >= MAXIMO_SESIONES_POR_HOST {
            continue;
        }
        let Some(sesion) = leer_sesion(campos) else {
            continue;
        };
        if vistas.insert((sesion.servidor, sesion.nombre.clone())) {
            sesiones.push(sesion);
        }
    }
    if !visto_tmux {
        return None;
    }
    if tmux.is_none() {
        sesiones.clear();
    }
    Some(LecturaEstado {
        tmux,
        ahora: ahora.unwrap_or(0),
        sesiones,
    })
}

/// Como [`leer_estado`], sobre los bytes tal como llegan: el UTF-8 inválido
/// se sustituye (U+FFFD) y solo se miran los primeros
/// [`MAXIMO_SALIDA_ESTADO`] bytes.
#[must_use]
pub fn leer_estado_bytes(salida: &[u8]) -> Option<LecturaEstado> {
    let tope = salida.len().min(MAXIMO_SALIDA_ESTADO);
    leer_estado(&String::from_utf8_lossy(&salida[..tope]))
}

/// Una línea `MAGI_SES` sin su etiqueta: servidor, nombre, identificador,
/// clientes, equipo, actividad, creada, campana, ventanas y título.
fn leer_sesion(campos: &str) -> Option<SesionVista> {
    // Un campo de más no se busca más allá del siguiente: no hace falta
    // recorrer una línea entera de separadores.
    let campos: Vec<&str> = campos.split(SEPARADOR).take(CAMPOS_SESION + 1).collect();
    if campos.len() != CAMPOS_SESION {
        return None;
    }
    let servidor = ServidorTmux::desde_texto(campos[0])?;
    let nombre = campos[1];
    if !nombre_valido(nombre) {
        return None;
    }
    Some(SesionVista {
        servidor,
        nombre: nombre.to_string(),
        id: id_de_sesion_valido(campos[2]).then(|| campos[2].to_string()),
        clientes: numero_u32(campos[3]),
        equipo: equipo_valido(campos[4]).then(|| campos[4].to_string()),
        actividad: numero_i64(campos[5]),
        creada: numero_i64(campos[6]),
        campana: campos[7] == "1",
        ventanas: numero_u32(campos[8]),
        titulo: limpiar_titulo(campos[9]),
    })
}

/// Bytes máximos de un identificador de sesión de tmux (`$` y hasta diez
/// dígitos).
const MAXIMO_ID_SESION: usize = 11;

/// ¿Es un identificador interno de sesión de tmux (`$0`, `$12`…)? Lo que no lo
/// es se descarta: nunca llega más lejos que `None`.
#[must_use]
pub fn id_de_sesion_valido(id: &str) -> bool {
    id.len() <= MAXIMO_ID_SESION
        && id.strip_prefix('$').is_some_and(|digitos| {
            !digitos.is_empty() && digitos.bytes().all(|b| b.is_ascii_digit())
        })
}

/// Título saneado: sin caracteres de control ni de formato invisible (los que
/// permiten disfrazar el texto, como el cambio de sentido), recortado y de
/// hasta 120 caracteres. Vacío es «sin título».
#[must_use]
pub fn limpiar_titulo(titulo: &str) -> Option<String> {
    let limpio: String = titulo
        .chars()
        .filter(|caracter| !caracter.is_control() && !es_formato_invisible(*caracter))
        .take(MAXIMO_TITULO * 2)
        .collect();
    let limpio: String = limpio.trim().chars().take(MAXIMO_TITULO).collect();
    let limpio = limpio.trim_end().to_string();
    (!limpio.is_empty()).then_some(limpio)
}

/// Caracteres de formato que no se ven y sirven para disfrazar un texto:
/// marcas de sentido, espacios de ancho cero y el BOM.
fn es_formato_invisible(caracter: char) -> bool {
    matches!(
        caracter,
        '\u{061c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

fn numero_u32(texto: &str) -> u32 {
    texto.parse::<u32>().unwrap_or(0)
}

/// Un entero de segundos Unix: lo ilegible o negativo vale 0.
fn numero_i64(texto: &str) -> i64 {
    texto
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|valor| *valor >= 0)
        .unwrap_or(0)
}

// -------------------------------------------------------------- comprobar

/// Qué terminfo usa el tmux de MAGI en ese host (`MAGI_TERM`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminfoTmux {
    Tmux256,
    Screen256,
}

impl TerminfoTmux {
    #[must_use]
    pub fn texto(self) -> &'static str {
        match self {
            TerminfoTmux::Tmux256 => "tmux-256color",
            TerminfoTmux::Screen256 => "screen-256color",
        }
    }
}

/// Lo que dice `comprobar <servidor> <nombre>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LecturaComprobar {
    /// Versión de tmux; `None` si no hay tmux (o la salida no es del script).
    pub tmux: Option<VersionTmux>,
    /// La sesión existe, por nombre exacto.
    pub existe: bool,
    /// `MAGI_TERM`; `None` si el host no tiene tmux.
    pub terminfo: Option<TerminfoTmux>,
}

/// Lee la salida de `comprobar`: la primera línea de cada clave manda y lo
/// demás se ignora.
#[must_use]
pub fn leer_comprobar(salida: &str) -> LecturaComprobar {
    let salida = prefijo_acotado(salida, MAXIMO_SALIDA_ESTADO);
    let mut lectura = LecturaComprobar::default();
    let (mut visto_tmux, mut visto_sesion, mut visto_term) = (false, false, false);
    for linea in salida.split('\n') {
        let linea = linea.strip_suffix('\r').unwrap_or(linea);
        if linea.len() > MAXIMO_LINEA_ESTADO {
            continue;
        }
        if let Some(valor) = linea.strip_prefix("MAGI_TMUX=") {
            if !visto_tmux {
                visto_tmux = true;
                lectura.tmux = version_de_texto(valor);
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_SESION=") {
            if !visto_sesion {
                visto_sesion = true;
                lectura.existe = valor == "existe";
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_TERM=") {
            if !visto_term {
                visto_term = true;
                lectura.terminfo = match valor {
                    "tmux-256color" => Some(TerminfoTmux::Tmux256),
                    "screen-256color" => Some(TerminfoTmux::Screen256),
                    _ => None,
                };
            }
        }
    }
    if lectura.tmux.is_none() {
        // Sin tmux no hay sesión ni terminfo que valgan.
        lectura.existe = false;
        lectura.terminfo = None;
    }
    lectura
}

// ------------------------------------------------- crear, renombrar, terminar

/// Resultado de `crear` (`MAGI_RESULTADO`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoCrear {
    Creada,
    /// Ya había una sesión con ese nombre (código 73).
    Existe,
    /// La carpeta no existe en el host (código 72).
    SinCarpeta,
    /// tmux es anterior a 3.0 (código 74).
    Antiguo,
    /// El host no tiene tmux (código 76).
    SinTmux,
    /// Cualquier otra cosa: tmux no pudo crearla o la salida no es legible.
    Error,
}

/// Lo que dice `crear`: el resultado y, si la hay, la versión de tmux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LecturaCrear {
    pub resultado: ResultadoCrear,
    pub tmux: Option<VersionTmux>,
}

/// Lee la salida de `crear`. El primer `MAGI_RESULTADO=` manda; sin él, el
/// código de salida del script (si se conoce) decide entre los de la tabla y
/// lo demás es `Error`.
#[must_use]
pub fn leer_crear(salida: &str, codigo: Option<i32>) -> LecturaCrear {
    let salida = prefijo_acotado(salida, MAXIMO_SALIDA_ESTADO);
    let mut tmux = None;
    let mut visto_tmux = false;
    let mut resultado = None;
    for linea in salida.split('\n') {
        let linea = linea.strip_suffix('\r').unwrap_or(linea);
        if linea.len() > MAXIMO_LINEA_ESTADO {
            continue;
        }
        if let Some(valor) = linea.strip_prefix("MAGI_TMUX=") {
            if !visto_tmux {
                visto_tmux = true;
                tmux = version_de_texto(valor);
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_RESULTADO=") {
            if resultado.is_none() {
                resultado = Some(match valor {
                    "creada" => ResultadoCrear::Creada,
                    "existe" => ResultadoCrear::Existe,
                    "sin_carpeta" => ResultadoCrear::SinCarpeta,
                    "antiguo" => ResultadoCrear::Antiguo,
                    "sin_tmux" => ResultadoCrear::SinTmux,
                    _ => ResultadoCrear::Error,
                });
            }
        }
    }
    let resultado = resultado.unwrap_or(match codigo {
        Some(SALIDA_EXISTE) => ResultadoCrear::Existe,
        Some(SALIDA_SIN_CARPETA) => ResultadoCrear::SinCarpeta,
        Some(SALIDA_ANTIGUO) => ResultadoCrear::Antiguo,
        Some(SALIDA_SIN_TMUX) => ResultadoCrear::SinTmux,
        _ => ResultadoCrear::Error,
    });
    LecturaCrear { resultado, tmux }
}

/// Resultado de `preparar`, la primera mitad de engancharse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoPreparar {
    /// La sesión existe y está lista para engancharse (código 0).
    Lista,
    /// La sesión ya no existe (código 75).
    NoExiste,
    /// El host no tiene tmux (código 76).
    SinTmux,
    /// tmux es anterior a 3.0 (código 74).
    Antiguo,
    /// Cualquier otra cosa: la salida no es la del script o no se sabe cómo
    /// acabó. No equivale a «no existe».
    Error,
}

/// Lo que dice `preparar`: el resultado y la identidad de la sesión.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LecturaPreparar {
    pub resultado: ResultadoPreparar,
    /// `MAGI_ID`, si lo dio y es un identificador válido.
    pub id: Option<String>,
    /// `MAGI_CREADA`: segundos Unix (hora del host) de la creación de la
    /// sesión, si la dio y es un número.
    pub creada: Option<i64>,
}

/// Lee la salida de `preparar` y su código de salida (que decide el resultado:
/// sin código, `Error`). El primer `MAGI_ID=` y el primer `MAGI_CREADA=` mandan.
#[must_use]
pub fn leer_preparar(salida: &str, codigo: Option<i32>) -> LecturaPreparar {
    let salida = prefijo_acotado(salida, MAXIMO_SALIDA_ESTADO);
    let mut id = None;
    let mut creada = None;
    let (mut visto_id, mut visto_creada) = (false, false);
    for linea in salida.split('\n') {
        let linea = linea.strip_suffix('\r').unwrap_or(linea);
        if linea.len() > MAXIMO_LINEA_ESTADO {
            continue;
        }
        if let Some(valor) = linea.strip_prefix("MAGI_ID=") {
            if !visto_id {
                visto_id = true;
                id = id_de_sesion_valido(valor).then(|| valor.to_string());
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_CREADA=") {
            if !visto_creada {
                visto_creada = true;
                creada = valor.trim().parse::<i64>().ok().filter(|valor| *valor > 0);
            }
        }
    }
    let resultado = match codigo {
        Some(0) => ResultadoPreparar::Lista,
        Some(SALIDA_NO_EXISTE) => ResultadoPreparar::NoExiste,
        Some(SALIDA_SIN_TMUX) => ResultadoPreparar::SinTmux,
        Some(SALIDA_ANTIGUO) => ResultadoPreparar::Antiguo,
        _ => ResultadoPreparar::Error,
    };
    LecturaPreparar {
        resultado,
        id,
        creada,
    }
}

/// La sesión de una lista de `estado` que es «la misma» que sigue una pestaña,
/// aunque otro equipo la haya renombrado (R157).
///
/// 1. La que lleva ese nombre, si no se sabe que sea otra: o no se conoce el
///    identificador de alguna de las dos, o es el mismo.
/// 2. Si no, la que tiene el identificador de la pestaña **y** la misma hora de
///    creación (con otro nombre: la renombraron). Sin la hora conocida no se
///    sigue por identificador: tmux reutiliza `$0`, `$1`… tras reiniciarse, y
///    una sesión ajena con el mismo número no es la de la pestaña.
///
/// Nunca devuelve una sesión del otro servidor de tmux.
#[must_use]
pub fn buscar_sesion<'a>(
    sesiones: &'a [SesionVista],
    servidor: ServidorTmux,
    nombre: &str,
    id: Option<&str>,
    creada: Option<i64>,
) -> Option<&'a SesionVista> {
    let del_servidor = |sesion: &&SesionVista| sesion.servidor == servidor;
    if let Some(sesion) = sesiones
        .iter()
        .filter(del_servidor)
        .find(|sesion| sesion.nombre == nombre)
    {
        // Otra sesión con el mismo nombre: otro identificador o, si tmux se
        // reinició y reutilizó el mismo identificador, otra hora de creación.
        let es_otra = matches!((id, sesion.id.as_deref()), (Some(a), Some(b)) if a != b)
            || matches!(creada, Some(a) if a > 0 && sesion.creada > 0 && a != sesion.creada);
        if !es_otra {
            return Some(sesion);
        }
    }
    let id = id?;
    let creada = creada.filter(|creada| *creada > 0)?;
    sesiones.iter().filter(del_servidor).find(|sesion| {
        sesion.nombre != nombre && sesion.id.as_deref() == Some(id) && sesion.creada == creada
    })
}

/// Resultado de `renombrar` (`MAGI_RESULTADO`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoRenombrar {
    Ok,
    NoExiste,
    /// Ya hay otra sesión con el nombre nuevo.
    Ocupado,
    /// tmux no pudo renombrarla o la salida no es legible.
    Error,
}

/// Lee la salida de `renombrar`: solo cuenta el primer `MAGI_RESULTADO=`.
#[must_use]
pub fn leer_renombrar(salida: &str) -> ResultadoRenombrar {
    let salida = prefijo_acotado(salida, MAXIMO_SALIDA_ESTADO);
    for linea in salida.split('\n') {
        let linea = linea.strip_suffix('\r').unwrap_or(linea);
        if linea.len() > MAXIMO_LINEA_ESTADO {
            continue;
        }
        if let Some(valor) = linea.strip_prefix("MAGI_RESULTADO=") {
            return match valor {
                "ok" => ResultadoRenombrar::Ok,
                "no_existe" => ResultadoRenombrar::NoExiste,
                "ocupado" => ResultadoRenombrar::Ocupado,
                _ => ResultadoRenombrar::Error,
            };
        }
    }
    ResultadoRenombrar::Error
}

/// Lee la salida de `terminar`: solo `MAGI_RESULTADO=ok` cuenta como
/// terminada (es la misma lectura que la de la Fase 15).
#[must_use]
pub fn leer_terminar(salida: &str) -> ResultadoTerminar {
    crate::terminal::tmux::leer_terminar(prefijo_acotado(salida, MAXIMO_SALIDA_ESTADO))
}

// ---------------------------------------------------- vista e historial

/// Vista previa lista para enseñar: texto plano de hasta 64 KiB (se quedan
/// las últimas líneas) y sin caracteres de control salvo el salto de línea.
#[must_use]
pub fn acotar_vista(texto: &str) -> String {
    cola_por_lineas(texto, MAXIMO_VISTA)
        .chars()
        .filter(|caracter| *caracter == '\n' || !caracter.is_control())
        .collect()
}

/// Historial de tmux de hasta 8 MiB (se queda lo más reciente, empezando en
/// una línea entera). Conserva las secuencias de color: el servidor lo pasa
/// por el emulador, que acota los tramos por fila.
#[must_use]
pub fn acotar_historial(texto: &str) -> &str {
    cola_por_lineas(texto, MAXIMO_HISTORIAL)
}

/// Los primeros `maximo` bytes del texto, sin partir un carácter.
fn prefijo_acotado(texto: &str, maximo: usize) -> &str {
    if texto.len() <= maximo {
        return texto;
    }
    let mut fin = maximo;
    while !texto.is_char_boundary(fin) {
        fin -= 1;
    }
    &texto[..fin]
}

/// Los últimos `maximo` bytes del texto, sin partir un carácter y, si se puede,
/// empezando en una línea entera.
fn cola_por_lineas(texto: &str, maximo: usize) -> &str {
    if texto.len() <= maximo {
        return texto;
    }
    let mut inicio = texto.len() - maximo;
    while !texto.is_char_boundary(inicio) {
        inicio += 1;
    }
    if inicio > 0 && texto.as_bytes()[inicio - 1] != b'\n' {
        if let Some(salto) = texto[inicio..].find('\n') {
            if inicio + salto + 1 < texto.len() {
                inicio += salto + 1;
            }
        }
    }
    &texto[inicio..]
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const US: char = '\u{1f}';

    fn linea(campos: &[&str]) -> String {
        format!("MAGI_SES{US}{}", campos.join(&US.to_string()))
    }

    fn sesion_ok() -> String {
        linea(&[
            "magi",
            "agente-api",
            "$7",
            "1",
            "portatil",
            "1794845343",
            "1794845000",
            "1",
            "3",
            "claude · api",
        ])
    }

    fn con_cabecera(lineas: &str) -> String {
        format!("MAGI_TMUX=tmux 3.4\nMAGI_AHORA=1794845400\n{lineas}\n")
    }

    #[test]
    fn lee_una_sesion_completa() {
        let lectura = leer_estado(&con_cabecera(&sesion_ok())).expect("salida legible");
        assert_eq!(lectura.tmux, Some(VersionTmux::nueva(3, 4)));
        assert_eq!(lectura.ahora, 1794845400);
        assert_eq!(
            lectura.sesiones,
            vec![SesionVista {
                servidor: ServidorTmux::Magi,
                nombre: "agente-api".to_string(),
                id: Some("$7".to_string()),
                clientes: 1,
                equipo: Some("portatil".to_string()),
                actividad: 1794845343,
                creada: 1794845000,
                campana: true,
                ventanas: 3,
                titulo: Some("claude · api".to_string()),
            }]
        );
    }

    #[test]
    fn la_version_con_letra_o_prefijo_no_falla() {
        for (texto, esperada) in [
            ("tmux 3.3a", Some(VersionTmux::nueva(3, 3))),
            ("tmux next-3.6", Some(VersionTmux::nueva(3, 6))),
            ("tmux master", Some(VersionTmux::nueva(99, 0))),
            ("tmux 3.5-rc1", Some(VersionTmux::nueva(3, 5))),
            ("tmux 3", Some(VersionTmux::nueva(3, 0))),
            ("tmux 3.0a", Some(VersionTmux::nueva(3, 0))),
            ("tmux", None),
            ("", None),
            ("tmux x.y", None),
        ] {
            let salida = format!("MAGI_TMUX={texto}\n");
            let lectura = leer_estado(&salida).expect("hay línea MAGI_TMUX");
            assert_eq!(lectura.tmux, esperada, "versión {texto:?}");
        }
    }

    #[test]
    fn sin_tmux_no_hay_sesiones_aunque_vengan() {
        let salida = format!("MAGI_TMUX=no\n{}\n", sesion_ok());
        let lectura = leer_estado(&salida).unwrap();
        assert_eq!(lectura.tmux, None);
        assert!(lectura.sesiones.is_empty());
    }

    #[test]
    fn una_salida_que_no_es_del_script_es_ilegible() {
        assert!(leer_estado("").is_none());
        assert!(leer_estado("Bienvenido a web-01\nÚltimo acceso: ayer\n").is_none());
        assert!(leer_estado(&sesion_ok()).is_none());
    }

    #[test]
    fn el_servidor_desconocido_descarta_la_linea() {
        let desconocido = linea(&["otro", "x", "", "0", "", "1", "1", "0", "1", ""]);
        let vacio = linea(&["", "x", "", "0", "", "1", "1", "0", "1", ""]);
        let lectura = leer_estado(&con_cabecera(&format!(
            "{desconocido}\n{vacio}\n{}",
            sesion_ok()
        )))
        .unwrap();
        assert_eq!(lectura.sesiones.len(), 1);
        assert_eq!(lectura.sesiones[0].nombre, "agente-api");
    }

    #[test]
    fn campos_de_menos_o_de_mas_descartan_la_linea() {
        let menos = linea(&["magi", "a", "", "0", "", "1", "1", "0", "1"]);
        let mas = linea(&["magi", "b", "", "0", "", "1", "1", "0", "1", "t", "extra"]);
        let muchos = format!("MAGI_SES{}", US.to_string().repeat(500));
        let solo_etiqueta = format!("MAGI_SES{US}");
        let sin_separador = "MAGI_SES".to_string();
        let lectura = leer_estado(&con_cabecera(&format!(
            "{menos}\n{mas}\n{muchos}\n{solo_etiqueta}\n{sin_separador}\n{}",
            sesion_ok()
        )))
        .unwrap();
        assert_eq!(lectura.sesiones.len(), 1);
        assert_eq!(lectura.sesiones[0].nombre, "agente-api");
    }

    #[test]
    fn el_equipo_que_no_case_con_la_etiqueta_es_none() {
        for equipo in [
            "Portatil",
            "con espacio",
            "con_guion_bajo",
            &"x".repeat(21),
            "",
        ] {
            let texto = linea(&["magi", "a", "", "0", equipo, "1", "1", "0", "1", ""]);
            let lectura = leer_estado(&con_cabecera(&texto)).unwrap();
            assert_eq!(lectura.sesiones.len(), 1, "equipo {equipo:?}");
            assert_eq!(lectura.sesiones[0].equipo, None, "equipo {equipo:?}");
        }
        let texto = linea(&["magi", "a", "", "0", "web-01", "1", "1", "0", "1", ""]);
        let lectura = leer_estado(&con_cabecera(&texto)).unwrap();
        assert_eq!(lectura.sesiones[0].equipo.as_deref(), Some("web-01"));
    }

    #[test]
    fn los_nombres_que_magi_no_puede_manejar_se_descartan() {
        let largo = "a".repeat(65);
        let mut lineas = String::new();
        for nombre in ["con espacio", "a:b", "ñ", largo.as_str(), "a.b", "-a b"] {
            lineas.push_str(&linea(&[
                "habitual", nombre, "", "0", "", "1", "1", "0", "1", "",
            ]));
            lineas.push('\n');
        }
        // Los de la tabla de nombres válidos, sí (incluido uno de 64 y el «0»
        // de tmux).
        let valido64 = "b".repeat(64);
        for nombre in ["0", "A_z-09", valido64.as_str()] {
            lineas.push_str(&linea(&[
                "habitual", nombre, "", "0", "", "1", "1", "0", "1", "",
            ]));
            lineas.push('\n');
        }
        let lectura = leer_estado(&con_cabecera(&lineas)).unwrap();
        let nombres: Vec<&str> = lectura
            .sesiones
            .iter()
            .map(|sesion| sesion.nombre.as_str())
            .collect();
        assert_eq!(nombres, vec!["0", "A_z-09", valido64.as_str()]);
    }

    #[test]
    fn el_titulo_se_sanea_y_se_acota_a_120_caracteres() {
        let con_controles = "\u{1b}[31mrojo\u{7}\u{0} y \u{202e}vuelta\u{200b}\u{feff}\u{85}";
        let texto = linea(&["magi", "a", "", "0", "", "1", "1", "0", "1", con_controles]);
        let lectura = leer_estado(&con_cabecera(&texto)).unwrap();
        assert_eq!(
            lectura.sesiones[0].titulo.as_deref(),
            Some("[31mrojo y vuelta")
        );

        let largo = "ñ".repeat(300);
        let texto = linea(&["magi", "a", "", "0", "", "1", "1", "0", "1", &largo]);
        let lectura = leer_estado(&con_cabecera(&texto)).unwrap();
        assert_eq!(
            lectura.sesiones[0]
                .titulo
                .as_deref()
                .map(|t| t.chars().count()),
            Some(MAXIMO_TITULO)
        );

        for vacio in ["", "   ", "\u{1b}\u{7}", "\u{200b}"] {
            let texto = linea(&["magi", "a", "", "0", "", "1", "1", "0", "1", vacio]);
            let lectura = leer_estado(&con_cabecera(&texto)).unwrap();
            assert_eq!(lectura.sesiones[0].titulo, None, "título {vacio:?}");
        }
    }

    #[test]
    fn los_numeros_ilegibles_valen_cero() {
        let texto = linea(&[
            "magi",
            "a",
            "",
            "muchos",
            "",
            "ayer",
            "-5",
            "si",
            "99999999999999999999",
            "",
        ]);
        let lectura = leer_estado(&con_cabecera(&texto)).unwrap();
        let sesion = &lectura.sesiones[0];
        assert_eq!(sesion.clientes, 0);
        assert_eq!(sesion.actividad, 0);
        assert_eq!(sesion.creada, 0);
        assert!(!sesion.campana);
        assert_eq!(sesion.ventanas, 0);
        let lectura = leer_estado("MAGI_TMUX=tmux 3.4\nMAGI_AHORA=pronto\n").unwrap();
        assert_eq!(lectura.ahora, 0);
        let lectura = leer_estado("MAGI_TMUX=tmux 3.4\n").unwrap();
        assert_eq!(lectura.ahora, 0);
    }

    #[test]
    fn solo_caben_500_sesiones_y_las_repetidas_no_cuentan() {
        let mut lineas = String::new();
        for numero in 0..700 {
            lineas.push_str(&linea(&[
                "magi",
                &format!("s-{numero}"),
                "",
                "0",
                "",
                "1",
                "1",
                "0",
                "1",
                "",
            ]));
            lineas.push('\n');
        }
        let lectura = leer_estado(&con_cabecera(&lineas)).unwrap();
        assert_eq!(lectura.sesiones.len(), MAXIMO_SESIONES_POR_HOST);
        assert_eq!(lectura.sesiones[0].nombre, "s-0");
        assert_eq!(lectura.sesiones[499].nombre, "s-499");

        let repetidas = format!("{}\n{}\n{}", sesion_ok(), sesion_ok(), sesion_ok());
        let lectura = leer_estado(&con_cabecera(&repetidas)).unwrap();
        assert_eq!(lectura.sesiones.len(), 1);
        // El mismo nombre en los dos servidores son dos sesiones distintas.
        let dos = format!(
            "{}\n{}",
            linea(&["magi", "x", "", "0", "", "1", "1", "0", "1", ""]),
            linea(&["habitual", "x", "", "0", "", "1", "1", "0", "1", ""])
        );
        assert_eq!(leer_estado(&con_cabecera(&dos)).unwrap().sesiones.len(), 2);
    }

    #[test]
    fn una_linea_gigante_se_descarta_sin_leerla() {
        let gigante = format!(
            "MAGI_SES{US}magi{US}a{US}{US}0{US}{US}1{US}1{US}0{US}1{US}{}",
            "x".repeat(5000)
        );
        let lectura = leer_estado(&con_cabecera(&format!("{gigante}\n{}", sesion_ok()))).unwrap();
        assert_eq!(lectura.sesiones.len(), 1);

        // Mil líneas de 1 MiB sin salto: solo se miran los primeros bytes.
        let enorme = "z".repeat(3 * 1024 * 1024);
        let lectura = leer_estado(&format!("MAGI_TMUX=tmux 3.4\n{enorme}")).unwrap();
        assert!(lectura.sesiones.is_empty());
        // Lo que pasa del tope no se lee: ni la versión.
        assert!(leer_estado(&format!("{enorme}\nMAGI_TMUX=tmux 3.4\n")).is_none());
    }

    #[test]
    fn el_utf8_invalido_no_provoca_panico() {
        let mut bytes = b"MAGI_TMUX=tmux 3.4\nMAGI_AHORA=1\nMAGI_SES\x1fmagi\x1fa\x1f\x1f0\x1f\x1f1\x1f1\x1f0\x1f1\x1ft\xff\xfe\xc3(x\n"
            .to_vec();
        bytes.extend_from_slice(
            b"MAGI_SES\x1fmagi\x1f\xff\xfe\x1f\x1f0\x1f\x1f1\x1f1\x1f0\x1f1\x1f\n",
        );
        let lectura = leer_estado_bytes(&bytes).expect("legible");
        assert_eq!(lectura.sesiones.len(), 1);
        let titulo = lectura.sesiones[0].titulo.as_deref().unwrap();
        assert!(titulo.starts_with('t'));
        assert!(titulo.contains('\u{fffd}'));
        // Basura pura.
        assert!(leer_estado_bytes(&[0xff, 0xfe, 0x00, 0x1f, 0x1f]).is_none());
        assert!(leer_estado_bytes(&[]).is_none());
    }

    #[test]
    fn tolera_crlf_y_lineas_sueltas() {
        let salida = format!(
            "\r\nMAGI_TMUX=tmux 3.4\r\nMAGI_AHORA=7\r\n{}\r\n\r\nbasura\r\n",
            sesion_ok()
        );
        let lectura = leer_estado(&salida).unwrap();
        assert_eq!(lectura.ahora, 7);
        assert_eq!(lectura.sesiones.len(), 1);
    }

    #[test]
    fn la_primera_cabecera_manda() {
        let lectura =
            leer_estado("MAGI_TMUX=tmux 3.4\nMAGI_TMUX=no\nMAGI_AHORA=5\nMAGI_AHORA=9\n").unwrap();
        assert_eq!(lectura.tmux, Some(VersionTmux::nueva(3, 4)));
        assert_eq!(lectura.ahora, 5);
    }

    #[test]
    fn comprobar_lee_version_existencia_y_terminfo() {
        let lectura =
            leer_comprobar("MAGI_TMUX=tmux 3.3a\nMAGI_SESION=existe\nMAGI_TERM=screen-256color\n");
        assert_eq!(lectura.tmux, Some(VersionTmux::nueva(3, 3)));
        assert!(lectura.existe);
        assert_eq!(lectura.terminfo, Some(TerminfoTmux::Screen256));
        assert_eq!(TerminfoTmux::Screen256.texto(), "screen-256color");

        let nueva =
            leer_comprobar("MAGI_TMUX=tmux 3.5\nMAGI_SESION=nueva\nMAGI_TERM=tmux-256color\n");
        assert!(!nueva.existe);
        assert_eq!(nueva.terminfo, Some(TerminfoTmux::Tmux256));

        // Sin tmux no hay nada que valga, aunque la salida diga otra cosa.
        let sin = leer_comprobar("MAGI_TMUX=no\nMAGI_SESION=existe\nMAGI_TERM=tmux-256color\n");
        assert_eq!(sin, LecturaComprobar::default());
        assert_eq!(leer_comprobar(""), LecturaComprobar::default());
        assert_eq!(
            leer_comprobar("\u{0}\u{1f}basura"),
            LecturaComprobar::default()
        );
        // Un terminfo desconocido no es ninguno de los dos.
        assert_eq!(
            leer_comprobar("MAGI_TMUX=tmux 3.4\nMAGI_TERM=vt100\n").terminfo,
            None
        );
    }

    #[test]
    fn crear_lee_el_resultado_y_el_codigo() {
        let creada = leer_crear("MAGI_TMUX=tmux 3.4\nMAGI_RESULTADO=creada\n", Some(0));
        assert_eq!(creada.resultado, ResultadoCrear::Creada);
        assert_eq!(creada.tmux, Some(VersionTmux::nueva(3, 4)));
        for (texto, esperado) in [
            ("existe", ResultadoCrear::Existe),
            ("sin_carpeta", ResultadoCrear::SinCarpeta),
            ("antiguo", ResultadoCrear::Antiguo),
            ("sin_tmux", ResultadoCrear::SinTmux),
            ("error", ResultadoCrear::Error),
            ("inventado", ResultadoCrear::Error),
            ("CREADA", ResultadoCrear::Error),
        ] {
            let salida = format!("MAGI_RESULTADO={texto}\n");
            assert_eq!(leer_crear(&salida, None).resultado, esperado, "{texto}");
        }
        // Sin línea de resultado, manda el código de salida.
        assert_eq!(leer_crear("", Some(73)).resultado, ResultadoCrear::Existe);
        assert_eq!(
            leer_crear("x", Some(72)).resultado,
            ResultadoCrear::SinCarpeta
        );
        assert_eq!(leer_crear("x", Some(74)).resultado, ResultadoCrear::Antiguo);
        assert_eq!(leer_crear("x", Some(76)).resultado, ResultadoCrear::SinTmux);
        assert_eq!(leer_crear("x", Some(1)).resultado, ResultadoCrear::Error);
        assert_eq!(leer_crear("x", Some(0)).resultado, ResultadoCrear::Error);
        assert_eq!(leer_crear("", None).resultado, ResultadoCrear::Error);
        // El primero manda.
        let doble = leer_crear("MAGI_RESULTADO=existe\nMAGI_RESULTADO=creada\n", None);
        assert_eq!(doble.resultado, ResultadoCrear::Existe);
    }

    #[test]
    fn renombrar_y_terminar_solo_creen_la_marca_exacta() {
        assert_eq!(
            leer_renombrar("MAGI_RESULTADO=ok\n"),
            ResultadoRenombrar::Ok
        );
        assert_eq!(
            leer_renombrar("MAGI_RESULTADO=ok\r\n"),
            ResultadoRenombrar::Ok
        );
        assert_eq!(
            leer_renombrar("MAGI_RESULTADO=no_existe\n"),
            ResultadoRenombrar::NoExiste
        );
        assert_eq!(
            leer_renombrar("MAGI_RESULTADO=ocupado\n"),
            ResultadoRenombrar::Ocupado
        );
        assert_eq!(
            leer_renombrar("MAGI_RESULTADO=OK\n"),
            ResultadoRenombrar::Error
        );
        assert_eq!(
            leer_renombrar("MAGI_RESULTADO=ok de mas\n"),
            ResultadoRenombrar::Error
        );
        assert_eq!(leer_renombrar(""), ResultadoRenombrar::Error);
        assert_eq!(leer_renombrar("\u{1f}\u{0}"), ResultadoRenombrar::Error);

        assert_eq!(leer_terminar("MAGI_RESULTADO=ok\n"), ResultadoTerminar::Ok);
        assert_eq!(
            leer_terminar("MAGI_RESULTADO=OK\n"),
            ResultadoTerminar::NoExiste
        );
        assert_eq!(leer_terminar(""), ResultadoTerminar::NoExiste);
    }

    #[test]
    fn la_vista_se_acota_y_pierde_los_controles() {
        let texto = "uno\u{1b}[31m\ttab\u{7}\r\ndos\n";
        assert_eq!(acotar_vista(texto), "uno[31mtab\ndos\n");
        assert_eq!(acotar_vista(""), "");

        // 64 KiB como mucho: se queda lo último, empezando en una línea entera.
        let mut grande = String::new();
        for numero in 0..20_000 {
            grande.push_str(&format!("linea {numero}\n"));
        }
        let vista = acotar_vista(&grande);
        assert!(vista.len() <= MAXIMO_VISTA);
        assert!(vista.len() > MAXIMO_VISTA - 64);
        assert!(vista.ends_with("linea 19999\n"));
        assert!(vista.starts_with("linea "));
        assert!(!vista.starts_with("linea linea"));

        // Una sola línea gigante y multibyte: se corta sin partir un carácter.
        let gigante = "ñ".repeat(100_000);
        let vista = acotar_vista(&gigante);
        assert!(vista.len() <= MAXIMO_VISTA);
        assert!(vista.chars().all(|caracter| caracter == 'ñ'));
    }

    #[test]
    fn el_historial_se_acota_a_8_mib_por_lo_mas_reciente() {
        let corto = "hola\n";
        assert_eq!(acotar_historial(corto), corto);
        let mut grande = String::new();
        let mut numero = 0;
        while grande.len() < MAXIMO_HISTORIAL + 100_000 {
            grande.push_str(&format!("\u{1b}[32mlinea {numero}\u{1b}[0m\n"));
            numero += 1;
        }
        let historial = acotar_historial(&grande);
        assert!(historial.len() <= MAXIMO_HISTORIAL);
        assert!(historial.len() > MAXIMO_HISTORIAL - 64);
        assert!(historial.starts_with("\u{1b}[32mlinea "));
        assert!(historial.ends_with(&format!("linea {}\u{1b}[0m\n", numero - 1)));
        // Conserva las secuencias de color: las pasa el emulador.
        assert!(historial.contains('\u{1b}'));
    }

    #[test]
    fn la_cola_por_lineas_no_entra_en_panico_con_casos_limite() {
        assert_eq!(cola_por_lineas("", 0), "");
        assert_eq!(cola_por_lineas("a\n", 1), "\n");
        assert_eq!(cola_por_lineas("abc", 0), "");
        assert_eq!(cola_por_lineas("añb", 1), "b");
        assert_eq!(cola_por_lineas("ñ", 1), "");
        assert_eq!(prefijo_acotado("añb", 2), "a");
        assert_eq!(prefijo_acotado("añb", 0), "");
        assert_eq!(prefijo_acotado("ab", 5), "ab");
    }

    #[test]
    fn el_identificador_de_sesion_solo_admite_dolar_y_digitos() {
        for valido in ["$0", "$7", "$12", "$1234567890"] {
            assert!(id_de_sesion_valido(valido), "{valido:?}");
        }
        for invalido in [
            "",
            "$",
            "7",
            "$a",
            "$-1",
            "$1 2",
            "$12345678901",
            "$1\n",
            "@1",
            "$１",
        ] {
            assert!(!id_de_sesion_valido(invalido), "{invalido:?}");
        }
    }

    #[test]
    fn una_sesion_sin_identificador_valido_se_lista_sin_el() {
        for id in ["", "7", "$x", "$99999999999999"] {
            let texto = linea(&["magi", "a", id, "0", "", "1", "1", "0", "1", ""]);
            let lectura = leer_estado(&con_cabecera(&texto)).unwrap();
            assert_eq!(lectura.sesiones.len(), 1, "id {id:?}");
            assert_eq!(lectura.sesiones[0].id, None, "id {id:?}");
        }
    }

    #[test]
    fn preparar_lee_el_codigo_de_salida_y_el_identificador() {
        let lista = leer_preparar("MAGI_ID=$4\n", Some(0));
        assert_eq!(lista.resultado, ResultadoPreparar::Lista);
        assert_eq!(lista.id.as_deref(), Some("$4"));
        // Sin identificador (o con uno inválido), preparada igualmente: solo
        // se pierde el seguimiento por identificador.
        assert_eq!(leer_preparar("", Some(0)).id, None);
        assert_eq!(leer_preparar("MAGI_ID=zz\n", Some(0)).id, None);
        // El primero manda.
        assert_eq!(
            leer_preparar("MAGI_ID=$1\nMAGI_ID=$2\n", Some(0))
                .id
                .as_deref(),
            Some("$1")
        );
        for (codigo, esperado) in [
            (Some(SALIDA_NO_EXISTE), ResultadoPreparar::NoExiste),
            (Some(SALIDA_SIN_TMUX), ResultadoPreparar::SinTmux),
            (Some(SALIDA_ANTIGUO), ResultadoPreparar::Antiguo),
            (Some(1), ResultadoPreparar::Error),
            (Some(70), ResultadoPreparar::Error),
            // Sin código, o cortado a mitad, nunca vale por «no existe»: la
            // salida de arriba podría ser media.
            (None, ResultadoPreparar::Error),
        ] {
            assert_eq!(
                leer_preparar("MAGI_ID=$4\n", codigo).resultado,
                esperado,
                "{codigo:?}"
            );
        }
    }

    #[test]
    fn preparar_lee_la_hora_de_creacion_solo_si_es_un_numero_positivo() {
        let lectura = leer_preparar("MAGI_ID=$4\nMAGI_CREADA=1794845000\n", Some(0));
        assert_eq!(lectura.creada, Some(1794845000));
        for ilegible in ["", "ayer", "-5", "0", "12 34", "99999999999999999999"] {
            let salida = format!("MAGI_ID=$4\nMAGI_CREADA={ilegible}\n");
            assert_eq!(leer_preparar(&salida, Some(0)).creada, None, "{ilegible:?}");
        }
        assert_eq!(leer_preparar("MAGI_ID=$4\n", Some(0)).creada, None);
    }

    fn vista(servidor: ServidorTmux, nombre: &str, id: Option<&str>, creada: i64) -> SesionVista {
        SesionVista {
            servidor,
            nombre: nombre.to_string(),
            id: id.map(str::to_string),
            clientes: 0,
            equipo: None,
            actividad: 0,
            creada,
            campana: false,
            ventanas: 1,
            titulo: None,
        }
    }

    #[test]
    fn la_sesion_se_busca_por_nombre_y_sigue_por_identificador_tras_un_renombrado() {
        let lista = [
            vista(ServidorTmux::Magi, "otra", Some("$1"), 500),
            vista(
                ServidorTmux::Magi,
                "nueva-del-otro-equipo",
                Some("$2"),
                1000,
            ),
        ];
        // Por nombre, aunque no se conozca el identificador de la pestaña.
        let hallada = buscar_sesion(&lista, ServidorTmux::Magi, "otra", None, None);
        assert_eq!(hallada.map(|s| s.nombre.as_str()), Some("otra"));
        // Tras un renombrado: el nombre ya no está y el identificador, con su
        // hora de creación, la encuentra.
        let hallada = buscar_sesion(&lista, ServidorTmux::Magi, "agente", Some("$2"), Some(1000));
        assert_eq!(
            hallada.map(|s| s.nombre.as_str()),
            Some("nueva-del-otro-equipo")
        );
        // Sin la hora de creación no se sigue por identificador (tmux reutiliza
        // los números al reiniciarse), ni con otra hora.
        assert!(buscar_sesion(&lista, ServidorTmux::Magi, "agente", Some("$2"), None).is_none());
        assert!(
            buscar_sesion(&lista, ServidorTmux::Magi, "agente", Some("$2"), Some(999)).is_none()
        );
        // Sin identificador, una sesión que ya no tiene el nombre no existe.
        assert!(buscar_sesion(&lista, ServidorTmux::Magi, "agente", None, Some(1000)).is_none());
    }

    #[test]
    fn un_nombre_reutilizado_por_otra_sesion_no_es_la_de_la_pestana() {
        // La pestaña seguía a «agente» ($2, creada a las 1000); otro equipo la
        // renombró y creó otra «agente» ($5).
        let lista = [
            vista(ServidorTmux::Magi, "agente", Some("$5"), 2000),
            vista(ServidorTmux::Magi, "agente-viejo", Some("$2"), 1000),
        ];
        let hallada = buscar_sesion(&lista, ServidorTmux::Magi, "agente", Some("$2"), Some(1000));
        assert_eq!(hallada.map(|s| s.nombre.as_str()), Some("agente-viejo"));
        // Con tmux reiniciado (otro «agente» con otra hora) y sin nadie con el
        // identificador viejo, no hay sesión: no se engancha a la ajena.
        let reiniciado = [vista(ServidorTmux::Magi, "agente", Some("$0"), 3000)];
        assert!(buscar_sesion(
            &reiniciado,
            ServidorTmux::Magi,
            "agente",
            Some("$2"),
            Some(1000)
        )
        .is_none());
        // Con tmux reiniciado, el mismo nombre **y el mismo identificador**
        // (los números se reutilizan): solo la hora de creación los distingue.
        let mismo_id = [vista(ServidorTmux::Magi, "agente", Some("$2"), 3000)];
        assert!(buscar_sesion(
            &mismo_id,
            ServidorTmux::Magi,
            "agente",
            Some("$2"),
            Some(1000)
        )
        .is_none());
        // Con la misma hora, sí es la suya.
        assert!(buscar_sesion(
            &[vista(ServidorTmux::Magi, "agente", Some("$2"), 1000)],
            ServidorTmux::Magi,
            "agente",
            Some("$2"),
            Some(1000)
        )
        .is_some());
    }

    #[test]
    fn la_busqueda_nunca_cruza_de_servidor() {
        let lista = [
            vista(ServidorTmux::Habitual, "agente", Some("$2"), 1000),
            vista(ServidorTmux::Habitual, "otro-nombre", Some("$9"), 1000),
        ];
        assert!(
            buscar_sesion(&lista, ServidorTmux::Magi, "agente", Some("$2"), Some(1000)).is_none()
        );
        assert!(buscar_sesion(
            &lista,
            ServidorTmux::Magi,
            "otro-nombre",
            Some("$9"),
            Some(1000)
        )
        .is_none());
        let habitual = buscar_sesion(&lista, ServidorTmux::Habitual, "agente", None, None);
        assert_eq!(habitual.map(|s| s.nombre.as_str()), Some("agente"));
    }
}
