//! Fila de teclas de Termux y fusión de `termux.properties` (§4.8, §7.7).
//!
//! Termux lee la fila de teclas extra de la línea `extra-keys` de su
//! `termux.properties` (con `java.util.Properties`). MAGI propone una fila
//! hecha de su tabla de teclas (ESC, TAB, CTRL, ALT, el prefijo configurado
//! como macro, las flechas y F1-F9) y, solo a petición y con confirmación, la
//! escribe tocando únicamente esa línea lógica:
//!
//! - [`fila`] genera la línea; [`fusionar`] la mete en el texto del fichero
//!   sustituyendo la última `extra-keys` efectiva (con sus continuaciones `\`)
//!   o añadiéndola al final, y deja el resto byte a byte.
//! - [`estado_fila`] y [`preparar`] solo leen; [`escribir_fila`] es el único
//!   sitio que escribe (copia con fecha primero, escritura atómica después) y
//!   [`recargar`] pide a Termux que relea sus ajustes.
//!
//! Lo que dice el fichero es entrada no fiable: se lee entero con tope
//! ([`TOPE_PROPERTIES`]), en UTF-8 y sin pánicos; su contenido nunca va a un
//! log ni a REGISTRO (solo el nombre de la copia). Nada de aquí lee
//! `std::env`: la ruta sale del `$HOME` del [`EntornoTermux`] inyectado.
//!
//! El análisis de líneas sigue a `java.util.Properties` (que es lo que usa
//! Termux): terminadores `\n`, `\r\n` y `\r` (no los otros separadores de
//! líneas de Unicode), comentarios con `#` o `!` solo al empezar una línea
//! lógica, continuación con un número impar de `\` (que en la última línea
//! del fichero se descarta) y claves que acaban en espacio, `=` o `:` sin
//! `\` delante. La clave se decodifica como lo hace Java antes de compararla
//! con `extra-keys`: `extra\-keys`, `\extra-keys` o `extra-keys` son esa
//! misma clave. Y antes de escribir, el texto resultante se vuelve a analizar:
//! si la `extra-keys` efectiva no es la de MAGI, no se escribe nada.

use std::ffi::OsStr;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyModifiers};

use super::ordenes::{FalloOrden, Ordenes};
use super::EntornoTermux;

/// Tope de lo que se lee de `termux.properties` (§7.7, §7.10).
pub const TOPE_PROPERTIES: usize = 256 * 1024;

/// Comentario que precede a la línea cuando se añade al final del fichero.
pub const COMENTARIO_FILA: &str = "# fila de teclas de MAGI (magi termux teclas)";

/// Lo que dice el resultado cuando Termux no recarga sus ajustes.
pub const REINICIA_TERMUX: &str = "reinicia Termux para ver la fila";

/// La orden de Termux que relee `termux.properties`. No es de Termux:API (es
/// de termux-tools): no apaga la API si falla.
pub const ORDEN_RECARGAR: &str = "termux-reload-settings";

/// Tope de copias del mismo día (`-2`, `-3`…): más es un fallo, no una racha.
const MAX_COPIAS_POR_DIA: u32 = 1000;

/// Tope de lo que se enseña de la línea `extra-keys` actual del usuario.
const TOPE_FILA_ACTUAL: usize = 400;

/// Espacio en blanco de `java.util.Properties`.
const BLANCOS: [char; 3] = [' ', '\t', '\u{c}'];

/// Cómo está la fila de teclas en el `termux.properties` del usuario (línea
/// «teclas» de `magi diagnostico`, §6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoFila {
    /// No hay `termux.properties` (ni `~/.termux/` ni `~/.config/termux/`).
    SinFichero,
    /// Hay fichero pero ninguna línea `extra-keys` efectiva: Termux usa la suya.
    SinExtraKeys,
    /// La línea `extra-keys` efectiva es la de MAGI para el prefijo dado.
    LaDeMagi,
    /// Tiene otra fila (la del usuario): MAGI no la toca sin preguntar.
    Otra,
    /// El fichero no se puede leer (demasiado grande, no UTF-8, permisos): el
    /// motivo, en una línea y sin su contenido.
    Ilegible(String),
}

// ------------------------------------------------------------ la fila propuesta

/// El prefijo de MAGI como macro de Termux (`{macro: 'CTRL ]', display: '^]'}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroPrefijo {
    /// Las teclas del macro, separadas por espacios (`CTRL ]`).
    pub macro_termux: String,
    /// Lo que se ve en la tecla (`^]`).
    pub texto: String,
}

/// El prefijo configurado (`[prefijo_escape]`, p. ej. «Ctrl+]») como macro de
/// Termux. Con `Ctrl+a` sale `CTRL a` y `^A`.
///
/// No todo prefijo se puede expresar: un carácter que no sea ASCII visible, la
/// comilla simple y la barra invertida (rompen el texto del macro), un
/// carácter con Mayús (Termux no dice qué carácter sale) o un nombre de tecla
/// que Termux no conoce. En esos casos devuelve el motivo y [`fila`] deja la
/// fila sin el macro: el comportamiento seguro es no escribir un macro
/// dudoso, y quien lo enseña dice por qué falta (el prefijo sigue funcionando
/// con el teclado del móvil, no con la fila).
pub fn macro_del_prefijo(prefijo_escape: &str) -> Result<MacroPrefijo, String> {
    let (codigo, modos) = crate::teclas::parsear_prefijo(prefijo_escape)
        .map_err(|motivo| format!("el prefijo no es válido ({motivo})"))?;
    let control = modos.contains(KeyModifiers::CONTROL);
    let alt = modos.contains(KeyModifiers::ALT);
    let mayus = modos.contains(KeyModifiers::SHIFT);

    // (nombre de Termux, texto en la tecla, ¿es un carácter?)
    let (nombre, etiqueta, es_caracter): (String, String, bool) = match codigo {
        KeyCode::Char(' ') => ("SPACE".into(), "Espacio".into(), false),
        KeyCode::Char(c) if c == '\'' || c == '\\' => {
            return Err(format!(
                "Termux no admite «{c}» dentro de un macro de la fila de teclas"
            ));
        }
        KeyCode::Char(c) if c.is_ascii_graphic() => (c.to_string(), c.to_string(), true),
        KeyCode::Char(_) => {
            return Err("solo se pueden poner como macro los caracteres ASCII".to_string());
        }
        KeyCode::Esc => ("ESC".into(), "Esc".into(), false),
        KeyCode::Tab => ("TAB".into(), "Tab".into(), false),
        KeyCode::Enter => ("ENTER".into(), "Intro".into(), false),
        KeyCode::Up => ("UP".into(), "↑".into(), false),
        KeyCode::Down => ("DOWN".into(), "↓".into(), false),
        KeyCode::Left => ("LEFT".into(), "←".into(), false),
        KeyCode::Right => ("RIGHT".into(), "→".into(), false),
        KeyCode::Home => ("HOME".into(), "Inicio".into(), false),
        KeyCode::End => ("END".into(), "Fin".into(), false),
        KeyCode::PageUp => ("PGUP".into(), "RePág".into(), false),
        KeyCode::PageDown => ("PGDN".into(), "AvPág".into(), false),
        KeyCode::Delete => ("DEL".into(), "Supr".into(), false),
        KeyCode::Backspace => ("BKSP".into(), "Retroc".into(), false),
        _ => return Err("Termux no tiene macro para esa tecla".to_string()),
    };
    if mayus && es_caracter {
        return Err(
            "con Mayús no se puede decir a Termux qué carácter sale: usa Ctrl o Alt".to_string(),
        );
    }

    let mut partes = Vec::new();
    if control {
        partes.push("CTRL");
    }
    if alt {
        partes.push("ALT");
    }
    if mayus {
        partes.push("SHIFT");
    }
    partes.push(&nombre);
    let macro_termux = partes.join(" ");

    // Lo que se ve en la tecla: `^]` para Ctrl con un carácter (como se
    // escribe en una terminal) y prefijos `C-`, `M-` y `S-` en el resto.
    let texto = if control && !alt && !mayus && es_caracter {
        format!("^{}", etiqueta.to_ascii_uppercase())
    } else {
        let mut texto = String::new();
        if control {
            texto.push_str("C-");
        }
        if alt {
            texto.push_str("M-");
        }
        if mayus {
            texto.push_str("S-");
        }
        texto.push_str(&etiqueta);
        texto
    };
    Ok(MacroPrefijo {
        macro_termux,
        texto,
    })
}

/// Las teclas de la fila de arriba (el macro del prefijo, si se puede, en el
/// quinto lugar) y de abajo, con el texto que se ve en cada una. Las flechas
/// salen como `← ↓ ↑ →` (el degradado a ASCII lo hace quien pinta).
#[must_use]
pub fn teclas_de_la_fila(prefijo_escape: &str) -> (Vec<String>, Vec<String>) {
    let mut arriba: Vec<String> = ["ESC", "TAB", "CTRL", "ALT"]
        .iter()
        .map(|tecla| (*tecla).to_string())
        .collect();
    if let Ok(macro_prefijo) = macro_del_prefijo(prefijo_escape) {
        arriba.push(macro_prefijo.texto);
    }
    arriba.extend(
        ["←", "↓", "↑", "→"]
            .iter()
            .map(|tecla| (*tecla).to_string()),
    );
    let abajo = (1..=9).map(|n| format!("F{n}")).collect();
    (arriba, abajo)
}

/// La línea `extra-keys` propuesta (§5.4, §7.7) para un prefijo: `ESC TAB CTRL
/// ALT`, el macro del prefijo, `LEFT DOWN UP RIGHT` y, en la segunda fila,
/// `F1`-`F9`. Con el prefijo por defecto:
///
/// ```text
/// extra-keys = [['ESC','TAB','CTRL','ALT',{macro: 'CTRL ]', display: '^]'},'LEFT','DOWN','UP','RIGHT'],['F1','F2','F3','F4','F5','F6','F7','F8','F9']]
/// ```
///
/// Si el prefijo no se puede expresar como macro ([`macro_del_prefijo`]), la
/// fila sale sin él.
#[must_use]
pub fn fila(prefijo_escape: &str) -> String {
    let mut arriba: Vec<String> = ["'ESC'", "'TAB'", "'CTRL'", "'ALT'"]
        .iter()
        .map(|tecla| (*tecla).to_string())
        .collect();
    if let Ok(macro_prefijo) = macro_del_prefijo(prefijo_escape) {
        arriba.push(format!(
            "{{macro: '{}', display: '{}'}}",
            macro_prefijo.macro_termux, macro_prefijo.texto
        ));
    }
    arriba.extend(
        ["'LEFT'", "'DOWN'", "'UP'", "'RIGHT'"]
            .iter()
            .map(|tecla| (*tecla).to_string()),
    );
    let abajo: Vec<String> = (1..=9).map(|n| format!("'F{n}'")).collect();
    format!(
        "extra-keys = [[{}],[{}]]",
        arriba.join(","),
        abajo.join(",")
    )
}

/// El valor de una línea `extra-keys` (lo que va tras `extra-keys = `).
fn valor_de_la_fila(linea: &str) -> &str {
    linea.strip_prefix("extra-keys = ").unwrap_or(linea)
}

/// Los caracteres con los que se arma la estructura de la fila (`[['A'],{macro:
/// 'B'}]`): un espacio que toca uno de ellos solo separa, no dice nada.
const ESTRUCTURA: [char; 6] = ['[', ']', '{', '}', ',', ':'];

/// El valor de una fila listo para compararlo con otro (§7.7, «sin espacios»):
/// quita el espacio ASCII (espacio, tabulador y avance de página) que está
/// **fuera** de las comillas simples y toca la estructura de la fila (o el
/// principio o el final), y deja todo lo demás exacto. Lo que va entre
/// comillas (`'CTRL ]'`) se compara tal cual: dos macros que solo difieren en
/// un espacio son dos teclas distintas. Tampoco se quita ningún espacio de
/// Unicode (U+00A0 y compañía): `org.json`, que lee la fila en Termux, no los
/// admite como separador, así que una fila con ellos no es la de MAGI.
fn normalizar_valor(texto: &str) -> String {
    let caracteres: Vec<char> = texto.chars().collect();
    let mut salida = String::with_capacity(texto.len());
    let mut dentro = false;
    let mut indice = 0;
    while indice < caracteres.len() {
        let caracter = caracteres[indice];
        if caracter == '\'' {
            dentro = !dentro;
        }
        if dentro || caracter == '\'' || !BLANCOS.contains(&caracter) {
            salida.push(caracter);
            indice += 1;
            continue;
        }
        // Una tanda de blancos fuera de comillas: solo se quita si toca la
        // estructura (un espacio dentro de una palabra, `ma cro`, no es de la
        // fila de MAGI).
        let mut fin = indice;
        while fin < caracteres.len() && BLANCOS.contains(&caracteres[fin]) {
            fin += 1;
        }
        let toca = salida
            .chars()
            .next_back()
            .is_none_or(|anterior| ESTRUCTURA.contains(&anterior))
            || caracteres
                .get(fin)
                .is_none_or(|siguiente| ESTRUCTURA.contains(siguiente));
        if !toca {
            salida.extend(&caracteres[indice..fin]);
        }
        indice = fin;
    }
    salida
}

// ------------------------------------------------------- líneas de Properties

/// Una línea física: dónde empieza su texto, dónde acaba (sin terminador) y
/// dónde acaba con él.
#[derive(Debug, Clone, Copy)]
struct Fisica {
    inicio: usize,
    fin_texto: usize,
    fin: usize,
}

/// Parte el texto en líneas físicas como `java.util.Properties`: `\n`, `\r\n`
/// y `\r` terminan una línea. Los saltos son ASCII, así que los cortes caen
/// siempre en límites de carácter.
fn lineas_fisicas(texto: &str) -> Vec<Fisica> {
    let bytes = texto.as_bytes();
    let mut lineas = Vec::new();
    let mut inicio = 0;
    let mut posicion = 0;
    while posicion < bytes.len() {
        match bytes[posicion] {
            b'\n' => {
                lineas.push(Fisica {
                    inicio,
                    fin_texto: posicion,
                    fin: posicion + 1,
                });
                posicion += 1;
                inicio = posicion;
            }
            b'\r' => {
                let fin = if bytes.get(posicion + 1) == Some(&b'\n') {
                    posicion + 2
                } else {
                    posicion + 1
                };
                lineas.push(Fisica {
                    inicio,
                    fin_texto: posicion,
                    fin,
                });
                posicion = fin;
                inicio = fin;
            }
            _ => posicion += 1,
        }
    }
    if inicio < bytes.len() {
        lineas.push(Fisica {
            inicio,
            fin_texto: bytes.len(),
            fin: bytes.len(),
        });
    }
    lineas
}

/// ¿Termina en un número impar de barras invertidas? Entonces la línea
/// continúa en la siguiente (como en Java).
fn continua(contenido: &str) -> bool {
    let barras = contenido
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count();
    barras % 2 == 1
}

/// Una línea lógica con la clave `extra-keys`.
#[derive(Debug, Clone)]
struct LineaExtraKeys {
    /// Dónde empieza su primera línea física.
    inicio: usize,
    /// Dónde acaba su última línea física, con su terminador.
    fin: usize,
    /// El terminador de la última línea física (`""` si acaba el fichero).
    terminador: String,
    /// El valor ya unido (sin las barras de continuación ni la sangría de las
    /// líneas siguientes).
    valor: String,
}

/// Separa una línea lógica en su clave (sin decodificar) y su valor, como el
/// `load0` de `java.util.Properties`: la clave acaba en el primer espacio,
/// tabulador, avance de página, `=` o `:` que no lleve una `\` delante (una
/// barra escapa al carácter siguiente, y `\\` es una barra), y el valor
/// empieza tras los blancos y, si todavía no había, un solo `=` o `:`.
fn separar_clave_y_valor(logica: &str) -> (&str, &str) {
    let mut fin_clave = logica.len();
    let mut inicio_valor = logica.len();
    let mut con_separador = false;
    let mut barra_previa = false;
    for (indice, caracter) in logica.char_indices() {
        if (caracter == '=' || caracter == ':') && !barra_previa {
            fin_clave = indice;
            inicio_valor = indice + 1;
            con_separador = true;
            break;
        } else if BLANCOS.contains(&caracter) && !barra_previa {
            fin_clave = indice;
            inicio_valor = indice + 1;
            break;
        }
        barra_previa = caracter == '\\' && !barra_previa;
    }
    for (indice, caracter) in logica[inicio_valor..].char_indices() {
        if !BLANCOS.contains(&caracter) {
            if !con_separador && (caracter == '=' || caracter == ':') {
                con_separador = true;
            } else {
                return (&logica[..fin_clave], &logica[inicio_valor + indice..]);
            }
        }
    }
    (&logica[..fin_clave], "")
}

/// La clave tal y como la lee Java (`loadConvert`): `\t`, `\n`, `\r` y `\f` son
/// el carácter que dicen, `\uXXXX` es el carácter con ese código y `\x`, para
/// cualquier otro `x`, es `x`. Una clave con un `\u` mal formado no se puede
/// leer: `None` (en Java sería un error y Termux no cargaría el fichero).
fn decodificar_clave(clave: &str) -> Option<String> {
    let mut salida = String::with_capacity(clave.len());
    let mut caracteres = clave.chars();
    while let Some(caracter) = caracteres.next() {
        if caracter != '\\' {
            salida.push(caracter);
            continue;
        }
        // Una barra al final no escapa nada (no se da: una línea que acaba en
        // barra impar continúa o, en el fin del fichero, se le quita).
        let Some(escapado) = caracteres.next() else {
            break;
        };
        match escapado {
            'u' => {
                let mut codigo = 0_u32;
                for _ in 0..4 {
                    codigo = (codigo << 4) + caracteres.next()?.to_digit(16)?;
                }
                // Un sustituto suelto no es un carácter de Rust: nunca es
                // parte de `extra-keys`.
                salida.push(char::from_u32(codigo).unwrap_or('\u{fffd}'));
            }
            't' => salida.push('\t'),
            'n' => salida.push('\n'),
            'r' => salida.push('\r'),
            'f' => salida.push('\u{c}'),
            otro => salida.push(otro),
        }
    }
    Some(salida)
}

/// El valor de una línea lógica si su clave, ya decodificada como la lee Java,
/// es exactamente `extra-keys`. Con escapes también cuenta (`extra\-keys`,
/// `\extra-keys` o `extra-keys` son `extra-keys` para Termux); una clave
/// que acaba en un espacio escapado (`extra-keys\ `) no lo es.
fn valor_si_extra_keys(logica: &str) -> Option<String> {
    let (clave, valor) = separar_clave_y_valor(logica);
    (decodificar_clave(clave)? == "extra-keys").then(|| valor.to_string())
}

/// La última línea lógica `extra-keys` del texto (la que usa Termux, que se
/// queda con la última clave repetida), sin contar comentarios.
fn ultima_extra_keys(texto: &str) -> Option<LineaExtraKeys> {
    let fisicas = lineas_fisicas(texto);
    let mut ultima = None;
    let mut i = 0;
    while i < fisicas.len() {
        let contenido = &texto[fisicas[i].inicio..fisicas[i].fin_texto];
        let recortada = contenido.trim_start_matches(BLANCOS);
        // Una línea en blanco o un comentario no continúan (en Java, un `\` al
        // final de un comentario no lo alarga).
        if recortada.is_empty() || recortada.starts_with(['#', '!']) {
            i += 1;
            continue;
        }
        let mut unida = String::new();
        let mut j = i;
        loop {
            let contenido = &texto[fisicas[j].inicio..fisicas[j].fin_texto];
            let parte = contenido.trim_start_matches(BLANCOS);
            if continua(contenido) {
                // Se quita la barra de continuación. En la última línea del
                // fichero no hay nada que continuar y Java también la
                // descarta: `extra-keys\` al final es la clave `extra-keys`.
                unida.push_str(&parte[..parte.len() - 1]);
                if j + 1 < fisicas.len() {
                    j += 1;
                } else {
                    break;
                }
            } else {
                unida.push_str(parte);
                break;
            }
        }
        if let Some(valor) = valor_si_extra_keys(&unida) {
            let ultima_fisica = fisicas[j];
            ultima = Some(LineaExtraKeys {
                inicio: fisicas[i].inicio,
                fin: ultima_fisica.fin,
                terminador: texto[ultima_fisica.fin_texto..ultima_fisica.fin].to_string(),
                valor,
            });
        }
        i = j + 1;
    }
    ultima
}

/// El valor de la línea `extra-keys` efectiva del texto, si la hay.
#[must_use]
pub fn valor_efectivo(texto: &str) -> Option<String> {
    ultima_extra_keys(texto).map(|linea| linea.valor)
}

/// ¿La línea `extra-keys` efectiva del texto es la de MAGI para este prefijo?
/// Compara los valores sin el espacio que solo separa (§7.7): el que va entre
/// comillas o es de otra clase (U+00A0) cuenta (véase [`normalizar_valor`]).
#[must_use]
pub fn es_la_de_magi(texto: &str, prefijo_escape: &str) -> bool {
    match valor_efectivo(texto) {
        Some(valor) => {
            normalizar_valor(&valor) == normalizar_valor(valor_de_la_fila(&fila(prefijo_escape)))
        }
        None => false,
    }
}

/// ¿Acaba el texto con una línea lógica abierta? Es decir: su última línea
/// lógica (un comentario o una línea en blanco no cuentan, salvo dentro de una
/// continuación) termina en un número impar de `\` y, en `java.util.Properties`,
/// lo que se escriba después se le pegaría como continuación, aunque empiece
/// por `#` (un comentario solo lo es al empezar una línea lógica).
fn acaba_en_continuacion(texto: &str) -> bool {
    let mut abierta = false;
    for fisica in lineas_fisicas(texto) {
        let contenido = &texto[fisica.inicio..fisica.fin_texto];
        if !abierta {
            let recortada = contenido.trim_start_matches(BLANCOS);
            if recortada.is_empty() || recortada.starts_with(['#', '!']) {
                continue;
            }
        }
        // Una línea en blanco dentro de la continuación la termina: `continua`
        // da `false` y `abierta` se cierra.
        abierta = continua(contenido);
    }
    abierta
}

/// Mete `nueva` (una línea `extra-keys = …` sin salto) en el texto de
/// `termux.properties`: sustituye la última línea lógica `extra-keys`
/// efectiva, con sus continuaciones, conservando su salto de línea (CRLF
/// incluido), o la añade al final con el comentario [`COMENTARIO_FILA`] (con
/// el separador que haga falta si el fichero no acaba en salto de línea). El
/// resto del texto queda byte a byte. Los comentarios y las claves que solo
/// empiezan por `extra-keys` (`extra-keys-style`) no cuentan.
///
/// Si el fichero acaba en una línea lógica abierta (su última línea termina en
/// un número impar de `\`), antes del comentario se pone una línea en blanco,
/// que en Java termina la continuación: sin ella, el comentario se pegaría al
/// valor de la última clave del usuario y MAGI le cambiaría una línea que no
/// es suya.
#[must_use]
pub fn fusionar(texto: &str, nueva: &str) -> String {
    if let Some(linea) = ultima_extra_keys(texto) {
        let salto = if linea.terminador == "\r\n" {
            "\r\n"
        } else {
            "\n"
        };
        let mut salida = String::with_capacity(texto.len() + nueva.len());
        salida.push_str(&texto[..linea.inicio]);
        salida.push_str(nueva);
        salida.push_str(salto);
        salida.push_str(&texto[linea.fin..]);
        return salida;
    }
    let separador = if texto.is_empty() || texto.ends_with(['\n', '\r']) {
        ""
    } else {
        "\n"
    };
    // La línea en blanco que cierra una continuación abierta lleva el mismo
    // salto de línea que la última del fichero.
    let en_blanco = if acaba_en_continuacion(texto) {
        if texto.ends_with("\r\n") {
            "\r\n"
        } else if texto.ends_with('\r') {
            "\r"
        } else {
            "\n"
        }
    } else {
        ""
    };
    format!("{texto}{separador}{en_blanco}{COMENTARIO_FILA}\n{nueva}\n")
}

// ------------------------------------------------------------------ el fichero

/// Qué `termux.properties` usa Termux: `~/.termux/termux.properties` si
/// existe; si no, `~/.config/termux/termux.properties` si es el que existe; y
/// en cualquier otro caso `~/.termux/termux.properties`, aunque todavía no
/// exista (ahí se crearía). Es el orden con el que Termux lo busca. Solo del
/// `EntornoTermux` (`$HOME`), nunca del uid; sin `$HOME` absoluto no hay ruta.
#[must_use]
pub fn ruta_properties(entorno: &EntornoTermux) -> Option<PathBuf> {
    let home = entorno.home.as_ref().filter(|home| home.is_absolute())?;
    let principal = home.join(".termux").join("termux.properties");
    let secundaria = home
        .join(".config")
        .join("termux")
        .join("termux.properties");
    if principal.is_file() || !secundaria.is_file() {
        Some(principal)
    } else {
        Some(secundaria)
    }
}

/// La ruta como se enseña: bajo el home, con `~`.
#[must_use]
pub fn ruta_visible(entorno: &EntornoTermux, ruta: &Path) -> String {
    entorno
        .home
        .as_deref()
        .and_then(|home| ruta.strip_prefix(home).ok())
        .map_or_else(
            || ruta.display().to_string(),
            |relativa| format!("~/{}", relativa.display()),
        )
}

/// Si `ruta` es un enlace simbólico, su destino resuelto (donde se escribe de
/// verdad y junto a donde va la copia); `Ok(None)` si no lo es (o no existe).
/// Falla si el enlace no se puede resolver.
fn destino_si_es_enlace(ruta: &Path) -> std::io::Result<Option<PathBuf>> {
    match std::fs::symlink_metadata(ruta) {
        Ok(datos) if datos.file_type().is_symlink() => std::fs::canonicalize(ruta).map(Some),
        _ => Ok(None),
    }
}

/// Lo que se lee de un `termux.properties`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contenido {
    pub texto: String,
    /// Los permisos del fichero (`0o600`…).
    pub modo: u32,
}

/// El motivo de un error de E/S, en castellano y sin la ruta ni el contenido.
fn motivo_de(error: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::PermissionDenied => "permiso denegado".to_string(),
        ErrorKind::NotFound => "no existe".to_string(),
        ErrorKind::AlreadyExists => "ya existe".to_string(),
        ErrorKind::InvalidData => "contenido no válido".to_string(),
        ErrorKind::TimedOut => "se agotó el tiempo".to_string(),
        ErrorKind::Interrupted => "interrumpido".to_string(),
        _ => "error de lectura o escritura".to_string(),
    }
}

/// Lee el fichero entero: hasta [`TOPE_PROPERTIES`] y en UTF-8. `Ok(None)` si
/// no existe; `Err(motivo)` (una línea, sin contenido) si no se puede leer.
/// Solo abre ficheros normales: un FIFO o un dispositivo no se tocan (abrirlo
/// podría colgar).
pub fn leer_properties(ruta: &Path) -> Result<Option<Contenido>, String> {
    let datos = match std::fs::metadata(ruta) {
        Ok(datos) => datos,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Un enlace roto no es «no existe»: escribir encima lo sustituiría.
            return if std::fs::symlink_metadata(ruta).is_ok() {
                Err("es un enlace roto".to_string())
            } else {
                Ok(None)
            };
        }
        Err(error) => return Err(motivo_de(&error)),
    };
    if !datos.is_file() {
        return Err("no es un fichero normal".to_string());
    }
    if datos.len() > TOPE_PROPERTIES as u64 {
        return Err("tiene más de 256 KiB".to_string());
    }
    let fichero = std::fs::File::open(ruta).map_err(|error| motivo_de(&error))?;
    let mut bytes = Vec::new();
    fichero
        .take(TOPE_PROPERTIES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| motivo_de(&error))?;
    if bytes.len() > TOPE_PROPERTIES {
        return Err("tiene más de 256 KiB".to_string());
    }
    let texto = String::from_utf8(bytes).map_err(|_| "no está en UTF-8".to_string())?;
    Ok(Some(Contenido {
        texto,
        modo: datos.permissions().mode() & 0o777,
    }))
}

/// Estado de la fila de teclas para un prefijo de MAGI (el texto de
/// `[prefijo_escape]`, p. ej. «Ctrl+]»). Solo lee: nunca escribe.
#[must_use]
pub fn estado_fila(entorno: &EntornoTermux, prefijo_escape: &str) -> EstadoFila {
    match ruta_properties(entorno) {
        Some(ruta) => estado_de_ruta(&ruta, prefijo_escape),
        None => EstadoFila::Ilegible("no está puesto $HOME".to_string()),
    }
}

/// Estado de la fila en un fichero concreto.
#[must_use]
pub fn estado_de_ruta(ruta: &Path, prefijo_escape: &str) -> EstadoFila {
    match leer_properties(ruta) {
        Ok(None) => EstadoFila::SinFichero,
        Err(motivo) => EstadoFila::Ilegible(motivo),
        Ok(Some(contenido)) => estado_de_texto(&contenido.texto, prefijo_escape),
    }
}

/// Estado de la fila en un texto ya leído.
#[must_use]
pub fn estado_de_texto(texto: &str, prefijo_escape: &str) -> EstadoFila {
    match valor_efectivo(texto) {
        None => EstadoFila::SinExtraKeys,
        Some(_) if es_la_de_magi(texto, prefijo_escape) => EstadoFila::LaDeMagi,
        Some(_) => EstadoFila::Otra,
    }
}

// ------------------------------------------------------------------ la propuesta

/// Lo que MAGI propone y dónde: todo lo que enseñan el diálogo y la CLI antes
/// de preguntar. Solo se lee para armarla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Propuesta {
    /// El `termux.properties` que se tocaría (`None` sin `$HOME`).
    pub ruta: Option<PathBuf>,
    /// La ruta como se enseña (`~/.termux/termux.properties`).
    pub ruta_visible: String,
    /// Si `ruta` es un enlace (los dotfiles, el almacenamiento compartido), su
    /// destino ya resuelto y como se enseña: ahí es donde se escribe y donde
    /// queda la copia, que puede estar fuera de `~/.termux` e incluso del
    /// home. `None` si no es un enlace (o está roto: entonces el fichero es
    /// ilegible y no se toca).
    pub enlace_a: Option<String>,
    pub estado: EstadoFila,
    /// La línea `extra-keys` propuesta.
    pub fila: String,
    /// Las teclas de la fila de arriba y de abajo, para pintarla.
    pub arriba: Vec<String>,
    pub abajo: Vec<String>,
    /// La línea `extra-keys` efectiva de ahora, si la hay (acotada).
    pub actual: Option<String>,
    /// El nombre de la copia que se haría: `None` si el fichero no existe o no
    /// hay nada que escribir.
    pub copia: Option<String>,
    /// Por qué falta el macro del prefijo, si falta.
    pub aviso_prefijo: Option<String>,
}

/// Arma la propuesta para un prefijo y una fecha (la de la copia). Solo lee.
#[must_use]
pub fn preparar(entorno: &EntornoTermux, prefijo_escape: &str, fecha: NaiveDate) -> Propuesta {
    let fila_propuesta = fila(prefijo_escape);
    let (arriba, abajo) = teclas_de_la_fila(prefijo_escape);
    let aviso_prefijo = macro_del_prefijo(prefijo_escape).err();
    let Some(ruta) = ruta_properties(entorno) else {
        return Propuesta {
            ruta: None,
            ruta_visible: "~/.termux/termux.properties".to_string(),
            enlace_a: None,
            estado: EstadoFila::Ilegible("no está puesto $HOME".to_string()),
            fila: fila_propuesta,
            arriba,
            abajo,
            actual: None,
            copia: None,
            aviso_prefijo,
        };
    };
    let visible = ruta_visible(entorno, &ruta);
    // Un enlace se escribe en su destino y la copia queda junto a él.
    let destino = destino_si_es_enlace(&ruta).ok().flatten();
    let enlace_a = destino
        .as_deref()
        .map(|destino| ruta_visible(entorno, destino));
    let sitio_de_la_copia = destino.as_deref().unwrap_or(&ruta);
    let (estado, actual, copia) = match leer_properties(&ruta) {
        Ok(None) => (EstadoFila::SinFichero, None, None),
        Err(motivo) => (EstadoFila::Ilegible(motivo), None, None),
        Ok(Some(contenido)) => {
            let estado = estado_de_texto(&contenido.texto, prefijo_escape);
            let actual = valor_efectivo(&contenido.texto).map(|valor| acotar(&valor));
            let copia =
                (estado != EstadoFila::LaDeMagi).then(|| nombre_de_copia(sitio_de_la_copia, fecha));
            (estado, actual, copia)
        }
    };
    Propuesta {
        ruta: Some(ruta),
        ruta_visible: visible,
        enlace_a,
        estado,
        fila: fila_propuesta,
        arriba,
        abajo,
        actual,
        copia,
        aviso_prefijo,
    }
}

/// Acorta un texto del usuario a [`TOPE_FILA_ACTUAL`] caracteres, en una línea.
fn acotar(texto: &str) -> String {
    let una_linea: String = texto
        .chars()
        .map(|caracter| if caracter.is_control() { ' ' } else { caracter })
        .collect();
    let una_linea = una_linea.trim();
    if una_linea.chars().count() <= TOPE_FILA_ACTUAL {
        una_linea.to_string()
    } else {
        let mut corto: String = una_linea.chars().take(TOPE_FILA_ACTUAL).collect();
        corto.push('…');
        corto
    }
}

// ------------------------------------------------------------------ escribir

/// Por qué no se pudo instalar la fila. Si falla antes de escribir, no se ha
/// tocado nada; si falla al escribir y ya había una copia, es
/// [`ErrorTeclas::EscrituraTrasCopia`] y la copia queda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorTeclas {
    /// No se sabe dónde está el home de Termux.
    SinHome,
    /// El fichero no se puede leer: no se toca (el motivo no lleva contenido).
    Ilegible(String),
    /// No se pudo hacer la copia: no se ha escrito nada.
    Copia(String),
    /// No se pudo escribir el fichero y no había nada que copiar (no
    /// existía): no queda ninguna copia.
    Escritura(String),
    /// Tras fusionar, la `extra-keys` efectiva del texto nuevo no es la de MAGI
    /// (Termux seguiría leyendo otra fila): no se copia ni se escribe nada.
    NoSeguro,
    /// No se pudo escribir el fichero, pero la copia ya está hecha y queda en
    /// disco: el usuario tiene que saberlo, y un reintento puede reutilizarla
    /// ([`escribir_fila_con_copia_previa`]) en vez de hacer otra.
    EscrituraTrasCopia {
        /// Por qué falló la escritura (sin rutas ni contenido).
        motivo: String,
        /// El nombre de la copia ya hecha (en el mismo directorio).
        copia: String,
    },
}

impl ErrorTeclas {
    /// El nombre de la copia que este fallo deja hecha en disco, si la hay.
    #[must_use]
    pub fn copia_hecha(&self) -> Option<&str> {
        match self {
            Self::EscrituraTrasCopia { copia, .. } => Some(copia),
            _ => None,
        }
    }
}

impl std::fmt::Display for ErrorTeclas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SinHome => f.write_str(
                "no se sabe dónde está el home de Termux ($HOME no está puesto): abre MAGI desde una sesión normal de Termux",
            ),
            Self::Ilegible(motivo) => write!(
                f,
                "termux.properties no se puede leer ({motivo}): edítalo a mano"
            ),
            Self::Copia(motivo) => write!(
                f,
                "no se pudo hacer la copia de termux.properties ({motivo}): no se ha escrito nada"
            ),
            Self::Escritura(motivo) => {
                write!(f, "no se pudo escribir termux.properties ({motivo})")
            }
            Self::NoSeguro => f.write_str(
                "no se puede instalar con seguridad: edita termux.properties a mano (no se ha escrito nada)",
            ),
            Self::EscrituraTrasCopia { motivo, copia } => write!(
                f,
                "no se pudo escribir termux.properties ({motivo}): la copia {copia} ya está hecha"
            ),
        }
    }
}

impl std::error::Error for ErrorTeclas {}

/// Lo que ha pasado al instalar la fila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escritura {
    pub ruta: PathBuf,
    /// El nombre de la copia hecha; `None` si el fichero no existía.
    pub copia: Option<String>,
    /// La fila ya era la de MAGI: no se ha copiado ni escrito nada.
    pub ya_estaba: bool,
    /// La copia no se ha hecho ahora: es la de un intento anterior que falló al
    /// escribir y que sigue intacta e idéntica al fichero (`copia` es su
    /// nombre).
    pub copia_reutilizada: bool,
}

impl Escritura {
    /// El detalle de REGISTRO: el nombre de la copia o «no existía» (§3.3).
    #[must_use]
    pub fn detalle_de_registro(&self) -> String {
        self.copia
            .clone()
            .unwrap_or_else(|| "no existía".to_string())
    }
}

/// El nombre de la copia que se haría hoy: `termux.properties.magi-AAAA-MM-DD`
/// y, si ya existe, con sufijo `-2`, `-3`… (no es el `.bak-` de
/// `ficheros::copia_con_fecha`). Solo mira; la copia de verdad se crea con
/// `create_new`.
#[must_use]
pub fn nombre_de_copia(ruta: &Path, fecha: NaiveDate) -> String {
    let directorio = ruta.parent().unwrap_or_else(|| Path::new("."));
    let base = nombre_base_de_copia(ruta, fecha);
    for n in 1..=MAX_COPIAS_POR_DIA {
        let candidato = candidato_de_copia(&base, n);
        if std::fs::symlink_metadata(directorio.join(&candidato)).is_err() {
            return candidato;
        }
    }
    candidato_de_copia(&base, MAX_COPIAS_POR_DIA)
}

fn nombre_base_de_copia(ruta: &Path, fecha: NaiveDate) -> String {
    let nombre = ruta
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("termux.properties");
    format!("{nombre}.magi-{}", fecha.format("%Y-%m-%d"))
}

fn candidato_de_copia(base: &str, n: u32) -> String {
    if n <= 1 {
        base.to_string()
    } else {
        format!("{base}-{n}")
    }
}

/// Crea la copia con los permisos del original y sin pisar nada (`create_new`).
/// Devuelve su nombre. `ajustar` es el `chmod` que corrige el `umask`; como en
/// la escritura, un sistema de ficheros que no lo admite (el almacenamiento
/// compartido de Android) no impide la copia: el fichero ya se creó con el
/// modo y el `umask` solo pudo quitarle permisos.
fn crear_copia(
    ruta: &Path,
    contenido: &[u8],
    modo: u32,
    fecha: NaiveDate,
    ajustar: &dyn Fn(&Path, u32) -> std::io::Result<()>,
) -> Result<String, String> {
    let directorio = ruta
        .parent()
        .ok_or_else(|| "la ruta no tiene directorio".to_string())?;
    let base = nombre_base_de_copia(ruta, fecha);
    for n in 1..=MAX_COPIAS_POR_DIA {
        let candidato = candidato_de_copia(&base, n);
        let destino = directorio.join(&candidato);
        let abierto = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(modo)
            .open(&destino);
        let mut fichero = match abierto {
            Ok(fichero) => fichero,
            // Otra copia de hoy: la siguiente. `create_new` no sigue enlaces.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(motivo_de(&error)),
        };
        let escrito = fichero
            .write_all(contenido)
            .and_then(|()| fichero.sync_all());
        // El `umask` puede haber quitado permisos al crearla: se los devuelve
        // si el sistema de ficheros deja.
        let _ = ajustar(&destino, modo);
        drop(fichero);
        return match escrito {
            Ok(()) => Ok(candidato),
            Err(error) => {
                // Una copia a medias no sirve: se quita.
                let _ = std::fs::remove_file(&destino);
                Err(motivo_de(&error))
            }
        };
    }
    Err("hay demasiadas copias de hoy".to_string())
}

/// ¿Sirve como copia la que dejó un intento anterior que falló al escribir?
/// Solo si `nombre` es un nombre de copia de MAGI de este fichero (sin
/// directorios), está en el mismo directorio, es un fichero normal (nunca un
/// enlace) y su contenido es byte a byte el del fichero que se va a
/// sobrescribir. Cualquier duda es «no»: se hace una copia nueva.
fn copia_previa_vale(ruta: &Path, nombre: &str, contenido: &Contenido) -> bool {
    let Some(directorio) = ruta.parent() else {
        return false;
    };
    let Some(propio) = ruta.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let nombre_de_copia = Path::new(nombre).file_name().and_then(OsStr::to_str) == Some(nombre);
    if !nombre_de_copia || !nombre.starts_with(&format!("{propio}.magi-")) {
        return false;
    }
    let destino = directorio.join(nombre);
    match std::fs::symlink_metadata(&destino) {
        Ok(datos) if datos.is_file() && datos.len() == contenido.texto.len() as u64 => {}
        _ => return false,
    }
    std::fs::read(&destino).is_ok_and(|bytes| bytes == contenido.texto.as_bytes())
}

/// Instala la fila de MAGI en `ruta` (el `termux.properties` que enseñó el
/// diálogo o la CLI: no se vuelve a calcular). Con efectos:
///
/// 1. Lee el fichero (hasta 256 KiB y UTF-8; si no, [`ErrorTeclas::Ilegible`] y
///    no se toca).
/// 2. Si su `extra-keys` ya es la de MAGI, no hace nada (`ya_estaba`).
/// 3. Si existe, copia primero `termux.properties.magi-<fecha>` con los
///    permisos del original; si la copia falla, error y no se escribe nada.
///    Si `ruta` es un enlace, la copia queda junto a su destino (con el
///    nombre de este), que es el fichero que se sobrescribe.
/// 4. Escribe de forma atómica (por `ficheros.rs`, tolerando que el destino no
///    admita `chmod`) solo el texto fusionado, con los permisos que tenía (600
///    si es nuevo) y creando `~/.termux/` (700) si hace falta. Si el fichero es
///    un enlace, escribe en su destino y el enlace sigue siéndolo. Si falla
///    después de la copia, el error ([`ErrorTeclas::EscrituraTrasCopia`]) lleva
///    el nombre de la copia.
///
/// Antes de la copia, el texto fusionado se vuelve a analizar: si la `extra-keys`
/// efectiva no es la de MAGI, [`ErrorTeclas::NoSeguro`] y no se toca nada.
///
/// Quien llama anota `termux_teclas_instaladas` cuando esto devuelve `Ok` sin
/// `ya_estaba`, y llama a [`recargar`] después. Bloquea el hilo (disco):
/// llámala desde una tarea, nunca desde el bucle de la interfaz.
pub fn escribir_fila(
    ruta: &Path,
    prefijo_escape: &str,
    fecha: NaiveDate,
) -> Result<Escritura, ErrorTeclas> {
    escribir_fila_con_copia_previa(ruta, prefijo_escape, fecha, None)
}

/// Como [`escribir_fila`], para un reintento: `copia_previa` es el nombre de la
/// copia que dejó el intento anterior al fallar la escritura
/// ([`ErrorTeclas::copia_hecha`]). Si esa copia sigue intacta y es idéntica al
/// fichero actual, se reutiliza (`copia_reutilizada`) en vez de hacer otra
/// (`-2`, `-3`…); si no, se hace una nueva como siempre.
pub fn escribir_fila_con_copia_previa(
    ruta: &Path,
    prefijo_escape: &str,
    fecha: NaiveDate,
    copia_previa: Option<&str>,
) -> Result<Escritura, ErrorTeclas> {
    let existente = leer_properties(ruta).map_err(ErrorTeclas::Ilegible)?;
    if let Some(contenido) = &existente {
        if es_la_de_magi(&contenido.texto, prefijo_escape) {
            return Ok(Escritura {
                ruta: ruta.to_path_buf(),
                copia: None,
                ya_estaba: true,
                copia_reutilizada: false,
            });
        }
    }
    let texto_actual = existente
        .as_ref()
        .map_or("", |contenido| contenido.texto.as_str());
    // Antes de tocar nada: lo que se va a escribir tiene que dejar a Termux
    // leyendo la fila de MAGI.
    let nuevo = fusion_verificada(texto_actual, prefijo_escape)?;

    // Un enlace (el `termux.properties` de unos dotfiles) se escribe en su
    // destino —renombrar encima lo sustituiría por un fichero normal— y la
    // copia queda junto a él: es el fichero que se va a sobrescribir.
    let destino = match destino_si_es_enlace(ruta) {
        Ok(Some(destino)) => destino,
        Ok(None) => ruta.to_path_buf(),
        Err(error) => return Err(ErrorTeclas::Escritura(motivo_de(&error))),
    };
    let ajuste = ajuste_de_permisos(&destino);

    let (copia, copia_reutilizada) = match &existente {
        Some(contenido) => {
            match copia_previa.filter(|previa| copia_previa_vale(&destino, previa, contenido)) {
                Some(previa) => (Some(previa.to_string()), true),
                None => (
                    Some(
                        crear_copia(
                            &destino,
                            contenido.texto.as_bytes(),
                            contenido.modo,
                            fecha,
                            &ajuste,
                        )
                        .map_err(ErrorTeclas::Copia)?,
                    ),
                    false,
                ),
            }
        }
        None => (None, false),
    };
    // Un fallo al escribir cuenta la copia que ya está hecha.
    let fallo_de_escritura = |motivo: String| match &copia {
        Some(copia) => ErrorTeclas::EscrituraTrasCopia {
            motivo,
            copia: copia.clone(),
        },
        None => ErrorTeclas::Escritura(motivo),
    };

    #[cfg(debug_assertions)]
    ganchos_de_pruebas::antes_de_escribir(&destino);
    if existente.is_none() {
        if let Some(directorio) = destino.parent() {
            // `~/.termux/` se crea con 700 (como Termux).
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directorio)
                .map_err(|error| fallo_de_escritura(motivo_de(&error)))?;
        }
    }
    let modo = existente.as_ref().map_or(0o600, |contenido| contenido.modo);
    // Tolerante con el `chmod`: un destino que no lo admite (el almacenamiento
    // compartido de Android, al que suelen apuntar esos enlaces) no impide
    // instalar la fila.
    crate::ficheros::escribir_atomico_con(&destino, nuevo.as_bytes(), modo, true, ajuste).map_err(
        |error| {
            // Solo el motivo, en castellano y sin rutas: la cadena de `anyhow`
            // lleva el texto del sistema operativo en inglés.
            fallo_de_escritura(
                error
                    .root_cause()
                    .downcast_ref::<std::io::Error>()
                    .map_or_else(|| "error de escritura".to_string(), motivo_de),
            )
        },
    )?;
    Ok(Escritura {
        ruta: ruta.to_path_buf(),
        copia,
        ya_estaba: false,
        copia_reutilizada,
    })
}

/// Fusiona la fila de MAGI en `texto` y comprueba el resultado con el mismo
/// analizador con el que se lee: si la `extra-keys` efectiva del texto nuevo no
/// es la de MAGI (una forma de escribir la clave que MAGI no ha previsto,
/// por ejemplo), Termux seguiría leyendo la fila del usuario y MAGI no puede
/// decir «instalada»: [`ErrorTeclas::NoSeguro`], sin copiar ni escribir nada.
fn fusion_verificada(texto: &str, prefijo_escape: &str) -> Result<String, ErrorTeclas> {
    let nuevo = fusionar(texto, &fila(prefijo_escape));
    verificar_fusion(&nuevo, prefijo_escape)?;
    Ok(nuevo)
}

fn verificar_fusion(nuevo: &str, prefijo_escape: &str) -> Result<(), ErrorTeclas> {
    if es_la_de_magi(nuevo, prefijo_escape) {
        Ok(())
    } else {
        Err(ErrorTeclas::NoSeguro)
    }
}

/// El `chmod` de las escrituras de aquí: el de siempre y, solo en las
/// pruebas, uno que falla (un sistema de ficheros que no lo admite).
fn ajuste_de_permisos(destino: &Path) -> impl Fn(&Path, u32) -> std::io::Result<()> {
    #[cfg(debug_assertions)]
    let sin_chmod = ganchos_de_pruebas::sin_chmod(destino);
    #[cfg(not(debug_assertions))]
    let sin_chmod = {
        let _ = destino;
        false
    };
    move |ruta, modo| {
        if sin_chmod {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        } else {
            crate::ficheros::ajustar_permisos(ruta, modo)
        }
    }
}

/// Superficie de pruebas (solo en compilaciones de depuración): hace fallar o
/// esperar la escritura de un fichero concreto, para probar lo que pasa
/// justo después de la copia. Se indexa por la ruta del fichero que se
/// escribe (el destino de un enlace, ya resuelto), de modo que las pruebas que
/// corren a la vez, cada una con su directorio, no se ven entre sí.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub mod ganchos_de_pruebas {
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, MutexGuard};

    type Gancho = Arc<dyn Fn() + Send + Sync>;

    static ANTES_DE_ESCRIBIR: Mutex<Option<HashMap<PathBuf, Gancho>>> = Mutex::new(None);
    static SIN_CHMOD: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

    fn cerrojo<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// La ruta con su directorio resuelto: la misma para quien instala el
    /// gancho y para quien lo busca, exista ya el fichero o no.
    fn clave(ruta: &Path) -> PathBuf {
        match (ruta.parent(), ruta.file_name()) {
            (Some(padre), Some(nombre)) => std::fs::canonicalize(padre)
                .unwrap_or_else(|_| padre.to_path_buf())
                .join(nombre),
            _ => ruta.to_path_buf(),
        }
    }

    /// Quita el gancho al soltarse.
    pub struct Guarda {
        clave: PathBuf,
        de_espera: bool,
    }

    impl Drop for Guarda {
        fn drop(&mut self) {
            if self.de_espera {
                if let Some(mapa) = cerrojo(&ANTES_DE_ESCRIBIR).as_mut() {
                    mapa.remove(&self.clave);
                }
            } else if let Some(conjunto) = cerrojo(&SIN_CHMOD).as_mut() {
                conjunto.remove(&self.clave);
            }
        }
    }

    /// Cada escritura de `fichero` llama a `gancho` justo después de la copia y
    /// antes de escribir (puede esperar, o dejar el directorio sin permiso).
    pub fn antes_de_escribir_en(
        fichero: &Path,
        gancho: impl Fn() + Send + Sync + 'static,
    ) -> Guarda {
        let clave = clave(fichero);
        cerrojo(&ANTES_DE_ESCRIBIR)
            .get_or_insert_with(HashMap::new)
            .insert(clave.clone(), Arc::new(gancho));
        Guarda {
            clave,
            de_espera: true,
        }
    }

    /// El sistema de ficheros de `fichero` no admite `chmod`.
    pub fn sin_chmod_en(fichero: &Path) -> Guarda {
        let clave = clave(fichero);
        cerrojo(&SIN_CHMOD)
            .get_or_insert_with(HashSet::new)
            .insert(clave.clone());
        Guarda {
            clave,
            de_espera: false,
        }
    }

    pub(super) fn antes_de_escribir(destino: &Path) {
        let gancho = cerrojo(&ANTES_DE_ESCRIBIR)
            .as_ref()
            .and_then(|mapa| mapa.get(&clave(destino)).cloned());
        if let Some(gancho) = gancho {
            gancho();
        }
    }

    pub(super) fn sin_chmod(destino: &Path) -> bool {
        cerrojo(&SIN_CHMOD)
            .as_ref()
            .is_some_and(|conjunto| conjunto.contains(&clave(destino)))
    }
}

/// Pide a Termux que relea `termux.properties` (`termux-reload-settings`),
/// por las órdenes con plazo y sin shell. No es de Termux:API: si falla, no
/// apaga la API. Bloquea el hilo hasta el plazo: desde una tarea.
pub fn recargar(ordenes: &Ordenes) -> Result<(), FalloOrden> {
    ordenes.ejecutar(&[ORDEN_RECARGAR], None).map(|_| ())
}

/// Por qué no se pudo recargar, en pocas palabras y sin lo que haya dicho la
/// orden (es texto de otro programa: no se repite).
#[must_use]
pub fn motivo_de_recarga(fallo: &FalloOrden) -> &'static str {
    match fallo {
        FalloOrden::NoInstalada(_) => "falta termux-reload-settings",
        FalloOrden::SinRespuesta | FalloOrden::ApiApagada => "Termux no responde",
        FalloOrden::Codigo { .. } => "la orden falló",
        FalloOrden::Io(_) => "no se pudo lanzar la orden",
    }
}

/// Lo que dice el resultado de la recarga: la fila ya se ve, o hay que
/// reiniciar Termux.
#[must_use]
pub fn texto_de_recarga(resultado: &Result<(), FalloOrden>) -> String {
    match resultado {
        Ok(()) => "Termux ha recargado sus ajustes: la fila ya se ve bajo el terminal".to_string(),
        Err(_) => REINICIA_TERMUX.to_string(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const DEFECTO: &str = "extra-keys = [['ESC','TAB','CTRL','ALT',{macro: 'CTRL ]', display: '^]'},'LEFT','DOWN','UP','RIGHT'],['F1','F2','F3','F4','F5','F6','F7','F8','F9']]";

    fn fecha() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 17).expect("fecha")
    }

    fn entorno_en(home: &Path) -> EntornoTermux {
        EntornoTermux {
            home: Some(home.to_path_buf()),
            ..EntornoTermux::default()
        }
    }

    #[test]
    fn la_fila_por_defecto_es_la_del_informe() {
        assert_eq!(fila("Ctrl+]"), DEFECTO);
    }

    #[test]
    fn el_prefijo_configurado_cambia_el_macro_y_su_texto() {
        let con_a = fila("Ctrl+a");
        assert!(
            con_a.contains("{macro: 'CTRL a', display: '^A'}"),
            "{con_a}"
        );
        assert!(fila("Ctrl+A").contains("{macro: 'CTRL a', display: '^A'}"));
        let alt = fila("Alt+x");
        assert!(alt.contains("{macro: 'ALT x', display: 'M-x'}"), "{alt}");
        let los_dos = macro_del_prefijo("Ctrl+Alt+b").unwrap();
        assert_eq!(los_dos.macro_termux, "CTRL ALT b");
        assert_eq!(los_dos.texto, "C-M-b");
        let espacio = macro_del_prefijo("Ctrl+espacio").unwrap();
        assert_eq!(espacio.macro_termux, "CTRL SPACE");
        let flecha = macro_del_prefijo("Alt+flecha_izquierda").unwrap();
        assert_eq!(flecha.macro_termux, "ALT LEFT");
        assert_eq!(flecha.texto, "M-←");
    }

    #[test]
    fn un_prefijo_que_no_se_puede_expresar_deja_la_fila_sin_macro() {
        for prefijo in [
            "Ctrl+'",
            "Ctrl+\\",
            "Ctrl+Shift+a",
            "Ctrl+ñ",
            "no es un prefijo",
            "Ctrl+",
        ] {
            assert!(
                macro_del_prefijo(prefijo).is_err(),
                "«{prefijo}» no debería poder ser macro"
            );
            let linea = fila(prefijo);
            assert!(!linea.contains("macro"), "{prefijo}: {linea}");
            // La fila sigue siendo válida: las dos filas completas.
            assert!(linea.starts_with("extra-keys = [['ESC','TAB','CTRL','ALT','LEFT'"));
            assert!(linea.ends_with("['F1','F2','F3','F4','F5','F6','F7','F8','F9']]"));
        }
    }

    #[test]
    fn nada_de_la_fila_puede_romper_el_texto_de_la_linea() {
        // Cualquier carácter ASCII visible que se acepte no lleva comillas
        // simples ni barras: el macro nunca cierra su propio texto.
        for codigo in 0x21_u8..=0x7e {
            let caracter = char::from(codigo);
            let prefijo = format!("Ctrl+{caracter}");
            let linea = fila(&prefijo);
            assert_eq!(linea.matches('\'').count() % 2, 0, "{prefijo}: {linea}");
            assert!(!linea.contains('\\'), "{prefijo}: {linea}");
            assert!(!linea.contains('\n'), "{prefijo}");
        }
    }

    #[test]
    fn las_teclas_para_pintar_siguen_a_la_fila() {
        let (arriba, abajo) = teclas_de_la_fila("Ctrl+]");
        assert_eq!(
            arriba,
            ["ESC", "TAB", "CTRL", "ALT", "^]", "←", "↓", "↑", "→"]
        );
        assert_eq!(abajo.len(), 9);
        assert_eq!(abajo[0], "F1");
        assert_eq!(abajo[8], "F9");
        let (sin_macro, _) = teclas_de_la_fila("Ctrl+'");
        assert_eq!(sin_macro.len(), 8);
    }

    // ------------------------------------------------------------ fusionar

    #[test]
    fn sin_extra_keys_se_anade_al_final_con_su_comentario() {
        let nueva = "extra-keys = X";
        assert_eq!(
            fusionar("", nueva),
            format!("{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        assert_eq!(
            fusionar("a=1\n", nueva),
            format!("a=1\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        // Sin salto al final: el separador correcto.
        assert_eq!(
            fusionar("a=1", nueva),
            format!("a=1\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        // Un fichero que acaba en una línea en blanco no suma otra.
        assert_eq!(
            fusionar("a=1\n\n", nueva),
            format!("a=1\n\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
    }

    #[test]
    fn un_fichero_que_acaba_en_continuacion_no_se_traga_el_comentario_de_magi() {
        let nueva = "extra-keys = X";
        // Con LF: una línea en blanco termina la continuación (en Java una
        // línea tras `\` no es comentario aunque empiece por `#`).
        assert_eq!(
            fusionar("a=1\nbell-character = vibrate \\\n", nueva),
            format!("a=1\nbell-character = vibrate \\\n\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        // Con CRLF, la línea en blanco lleva el mismo salto.
        assert_eq!(
            fusionar("bell-character = vibrate \\\r\n", nueva),
            format!("bell-character = vibrate \\\r\n\r\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        // Con `\r` suelto, igual.
        assert_eq!(
            fusionar("bell-character = vibrate \\\r", nueva),
            format!("bell-character = vibrate \\\r\r{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        // Sin salto final: el de la línea y el de la línea en blanco.
        assert_eq!(
            fusionar("bell-character = vibrate \\", nueva),
            format!("bell-character = vibrate \\\n\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        // Tres barras: la última continúa.
        assert_eq!(
            fusionar("x = A\\\\\\\n", nueva),
            format!("x = A\\\\\\\n\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
    }

    #[test]
    fn solo_una_continuacion_abierta_pide_la_linea_en_blanco() {
        let nueva = "extra-keys = X";
        let sin_blanco = |texto: &str| {
            assert_eq!(
                fusionar(texto, nueva),
                format!("{texto}{COMENTARIO_FILA}\nextra-keys = X\n"),
                "{texto:?}"
            );
        };
        // Un número par de barras no continúa.
        sin_blanco("a = 1\\\\\n");
        // Un comentario no continúa (en Java un `\` al final no lo alarga).
        sin_blanco("# comentario \\\n");
        sin_blanco("! otro \\\n");
        // Una línea en blanco tras la continuación la cierra.
        sin_blanco("a = 1 \\\n\n");
        sin_blanco("a = 1 \\\n   \n");
        // Un fichero que acaba en blanco o sin nada.
        sin_blanco("a = 1\n\n");
        sin_blanco("");
        // Dentro de una continuación, un «comentario» sigue siendo del valor:
        // si acaba en `\` el valor sigue abierto.
        assert_eq!(
            fusionar("a = 1 \\\n# sigue \\\n", nueva),
            format!("a = 1 \\\n# sigue \\\n\n{COMENTARIO_FILA}\nextra-keys = X\n")
        );
        // …y si no, se cierra con su propio salto.
        sin_blanco("a = 1 \\\n# acaba\n");
        // Una extra-keys que se sustituye no pide línea en blanco.
        assert_eq!(
            fusionar("a=1\nextra-keys = V \\\n  W\n", nueva),
            "a=1\nextra-keys = X\n"
        );
    }

    #[test]
    fn sustituye_una_linea_y_deja_el_resto_byte_a_byte() {
        let texto = "# mi config\nbell-character=ignore\nextra-keys = [['A']]\nfullscreen=true\n";
        assert_eq!(
            fusionar(texto, "extra-keys = NUEVA"),
            "# mi config\nbell-character=ignore\nextra-keys = NUEVA\nfullscreen=true\n"
        );
    }

    #[test]
    fn sustituye_la_linea_lógica_entera_con_sus_continuaciones() {
        let texto = "a=1\r\n# comentario\r\nextra-keys = [['ESC','TAB'], \\\r\n    ['F1','F2'], \\\r\n    ['F3']]\r\nz=2\r\n";
        assert_eq!(
            fusionar(texto, "extra-keys = NUEVA"),
            "a=1\r\n# comentario\r\nextra-keys = NUEVA\r\nz=2\r\n"
        );
        // Con LF.
        let texto = "a=1\nextra-keys = [[\\\n'A'\\\n]]\nz=2\n";
        assert_eq!(
            fusionar(texto, "extra-keys = NUEVA"),
            "a=1\nextra-keys = NUEVA\nz=2\n"
        );
    }

    #[test]
    fn la_ultima_extra_keys_es_la_que_manda() {
        let texto = "extra-keys = PRIMERA\nx=1\nextra-keys = SEGUNDA\ny=2\n";
        assert_eq!(
            fusionar(texto, "extra-keys = NUEVA"),
            "extra-keys = PRIMERA\nx=1\nextra-keys = NUEVA\ny=2\n"
        );
        assert_eq!(valor_efectivo(texto).as_deref(), Some("SEGUNDA"));
    }

    #[test]
    fn un_comentario_no_cuenta_como_extra_keys() {
        for comentario in [
            "# extra-keys = [['X']]",
            "#extra-keys=1",
            "   # extra-keys = 1",
            "! extra-keys = 1",
            "\t!extra-keys: 1",
        ] {
            let texto = format!("{comentario}\na=1\n");
            assert_eq!(valor_efectivo(&texto), None, "{comentario}");
            assert_eq!(
                fusionar(&texto, "extra-keys = N"),
                format!("{texto}{COMENTARIO_FILA}\nextra-keys = N\n"),
                "{comentario}"
            );
        }
    }

    #[test]
    fn la_continuacion_de_un_comentario_no_es_una_extra_keys() {
        // En Java un comentario no continúa con `\`: la línea siguiente es
        // una línea nueva.
        let texto = "# comentario \\\nextra-keys = REAL\n";
        assert_eq!(valor_efectivo(texto).as_deref(), Some("REAL"));
        // Una línea de comentario dentro de una continuación sí es del valor.
        let texto = "x=1 \\\nextra-keys = DENTRO\ny=2\n";
        assert_eq!(valor_efectivo(texto), None);
    }

    #[test]
    fn una_clave_que_solo_empieza_por_extra_keys_no_cuenta() {
        let texto = "extra-keys-style = default\nextra-keys-x=1\nextra-keysy = 3\n";
        assert_eq!(valor_efectivo(texto), None);
        assert_eq!(
            fusionar(texto, "extra-keys = N"),
            format!("{texto}{COMENTARIO_FILA}\nextra-keys = N\n")
        );
        // Con la de verdad en medio, solo se toca ella.
        let texto = "extra-keys-style = default\nextra-keys = [['A']]\nextra-keys-x=1\n";
        assert_eq!(
            fusionar(texto, "extra-keys = N"),
            "extra-keys-style = default\nextra-keys = N\nextra-keys-x=1\n"
        );
    }

    #[test]
    fn la_clave_acepta_los_separadores_de_properties() {
        for linea in [
            "extra-keys=V",
            "extra-keys = V",
            "extra-keys : V",
            "extra-keys:V",
            "  extra-keys\tV",
            "extra-keys V",
            "extra-keys  =  V",
        ] {
            assert_eq!(
                valor_efectivo(&format!("{linea}\n")).as_deref(),
                Some("V"),
                "{linea}"
            );
        }
        // Una clave que acaba en un espacio escapado es otra clave
        // (`extra-keys `): Java no la lee como `extra-keys`.
        assert_eq!(valor_efectivo("extra-keys\\ = V\n"), None);
        assert_eq!(valor_efectivo("extra-keys\\:x = V\n"), None);
    }

    /// E3: Java decodifica los escapes de la clave antes de usarla: todas estas
    /// son la clave `extra-keys` para Termux.
    #[test]
    fn la_clave_se_decodifica_como_lo_hace_java() {
        for linea in [
            "extra\\-keys = V",
            "\\extra-keys = V",
            "e\\xtr\\a-keys = V",
            "extra\\u002dkeys = V",
            "extra\\u002Dkeys=V",
            "\\u0065xtra-keys: V",
        ] {
            assert_eq!(
                valor_efectivo(&format!("{linea}\n")).as_deref(),
                Some("V"),
                "{linea}"
            );
        }
        // Una barra escapada en la clave es otra clave.
        assert_eq!(valor_efectivo("extra-keys\\\\ = W\n"), None);
        // Un escape de tabulador o de salto no es el nombre de la clave.
        assert_eq!(valor_efectivo("extra-\\tkeys = V\n"), None);
        assert_eq!(valor_efectivo("extra\\nkeys = V\n"), None);
        // Un `\u` mal formado no se puede leer: no es `extra-keys`.
        assert_eq!(valor_efectivo("extra\\u00zdkeys = V\n"), None);
        assert_eq!(valor_efectivo("extra-keys\\u00 = V\n"), None);
    }

    /// E3: una `extra-keys` escrita con escapes cuenta como la de verdad, para
    /// leerla, para decir si es la de MAGI y para sustituirla (y, si va detrás
    /// de la normal, es la que manda).
    #[test]
    fn una_clave_con_escapes_manda_como_la_normal() {
        let nueva = "extra-keys = N";
        for clave in ["extra\\-keys", "\\extra-keys", "extra\\u002dkeys"] {
            let texto = format!("extra-keys = [['A']]\nx = 1\n{clave} = [['B']]\ny = 2\n");
            assert_eq!(
                valor_efectivo(&texto).as_deref(),
                Some("[['B']]"),
                "{clave}"
            );
            // Se sustituye la que manda; la anterior se queda como estaba.
            assert_eq!(
                fusionar(&texto, nueva),
                "extra-keys = [['A']]\nx = 1\nextra-keys = N\ny = 2\n",
                "{clave}"
            );
            // Y si la de MAGI está antes y la del usuario (con escapes) después,
            // la de MAGI no es la que lee Termux.
            let con_la_de_magi = format!("{DEFECTO}\n{clave} = [['B']]\n");
            assert!(!es_la_de_magi(&con_la_de_magi, "Ctrl+]"), "{clave}");
            assert_eq!(
                estado_de_texto(&con_la_de_magi, "Ctrl+]"),
                EstadoFila::Otra,
                "{clave}"
            );
        }
    }

    /// E3: la barra de una última línea sin nada detrás se descarta, como en
    /// Java: `extra-keys\` al final del fichero es la clave `extra-keys`.
    #[test]
    fn una_barra_al_final_del_fichero_se_descarta_como_en_java() {
        assert_eq!(valor_efectivo("extra-keys\\").as_deref(), Some(""));
        assert_eq!(valor_efectivo("extra-keys\\\n").as_deref(), Some(""));
        assert_eq!(valor_efectivo("extra-keys = A\\").as_deref(), Some("A"));
        assert_eq!(valor_efectivo("extra-keys = A\\\r\n").as_deref(), Some("A"));
        // Dos barras no son una continuación: la clave es otra.
        assert_eq!(valor_efectivo("extra-keys\\\\"), None);
        // Con la de MAGI delante, la del final es la que lee Termux.
        let texto = format!("{DEFECTO}\nextra-keys\\");
        assert_eq!(valor_efectivo(&texto).as_deref(), Some(""));
        assert!(!es_la_de_magi(&texto, "Ctrl+]"));
        // Se sustituye la línea que manda; la de antes se queda como estaba.
        assert_eq!(
            fusionar(&texto, "extra-keys = N"),
            format!("{DEFECTO}\nextra-keys = N\n")
        );
    }

    /// E3, el cinturón: lo que se va a escribir se vuelve a analizar y, si la
    /// fila efectiva no es la de MAGI, no se escribe.
    #[test]
    fn un_texto_fusionado_que_no_deja_la_fila_de_magi_no_se_escribe() {
        // Lo normal: la fusión deja la fila de MAGI.
        for texto in ["", "a=1\n", "extra-keys = [['A']]\n", "x = 1 \\"] {
            let nuevo = fusion_verificada(texto, "Ctrl+]").unwrap();
            assert!(es_la_de_magi(&nuevo, "Ctrl+]"), "{texto:?}");
        }
        // Un texto en el que la efectiva es otra: se niega, y dice por qué.
        let malo = format!("{DEFECTO}\nextra-keys = [['B']]\n");
        let error = verificar_fusion(&malo, "Ctrl+]").unwrap_err();
        assert_eq!(error, ErrorTeclas::NoSeguro);
        assert!(error
            .to_string()
            .contains("no se puede instalar con seguridad"));
        assert!(error.to_string().contains("edita termux.properties a mano"));
        assert_eq!(error.copia_hecha(), None);
        assert_eq!(verificar_fusion(&format!("{DEFECTO}\n"), "Ctrl+]"), Ok(()));
    }

    #[test]
    fn un_numero_par_de_barras_no_continua() {
        // `\\` es una barra escapada: la línea acaba ahí.
        let texto = "extra-keys = A\\\\\nb=2\n";
        assert_eq!(fusionar(texto, "extra-keys = N"), "extra-keys = N\nb=2\n");
        // Tres barras: la última continúa.
        let texto = "extra-keys = A\\\\\\\nB\nc=3\n";
        assert_eq!(fusionar(texto, "extra-keys = N"), "extra-keys = N\nc=3\n");
    }

    #[test]
    fn el_salto_del_ultimo_renglon_decide_el_de_la_linea() {
        // Sin salto al final del fichero: la línea nueva lleva `\n`.
        assert_eq!(
            fusionar("a=1\nextra-keys = V", "extra-keys = N"),
            "a=1\nextra-keys = N\n"
        );
        // Un CRLF solo cambia lo que toca: las demás líneas siguen con LF.
        assert_eq!(
            fusionar("a=1\nextra-keys = V\r\nb=2\n", "extra-keys = N"),
            "a=1\nextra-keys = N\r\nb=2\n"
        );
    }

    #[test]
    fn solo_separan_lineas_los_saltos_de_java() {
        // U+2028 y el avance de página no parten líneas en Java: no hay
        // `extra-keys` aquí, solo una línea con un carácter raro.
        let texto = "a=1\u{2028}extra-keys = V\n";
        assert_eq!(valor_efectivo(texto), None);
        let texto = "a=1\u{85}extra-keys = V\n";
        assert_eq!(valor_efectivo(texto), None);
        // Un `\r` suelto sí es un salto.
        assert_eq!(
            valor_efectivo("a=1\rextra-keys = V\r").as_deref(),
            Some("V")
        );
    }

    #[test]
    fn fusionar_dos_veces_es_estable() {
        let texto = "a=1\n";
        let una = fusionar(texto, &fila("Ctrl+]"));
        let dos = fusionar(&una, &fila("Ctrl+]"));
        assert_eq!(una, dos);
        assert!(es_la_de_magi(&una, "Ctrl+]"));
        assert!(!es_la_de_magi(&una, "Ctrl+a"));
    }

    #[test]
    fn ya_es_la_de_magi_compara_sin_espacios() {
        let linea = fila("Ctrl+]");
        assert!(es_la_de_magi(&format!("{linea}\n"), "Ctrl+]"));
        // Con otro espaciado y repartida en líneas.
        let partida = "extra-keys=[['ESC','TAB','CTRL','ALT', \\\n  {macro: 'CTRL ]', display: '^]'},\\\n  'LEFT','DOWN','UP','RIGHT'],['F1','F2','F3','F4','F5','F6','F7','F8','F9']]\n";
        assert!(es_la_de_magi(partida, "Ctrl+]"), "{partida}");
        // Otra fila no lo es, aunque se parezca.
        assert!(!es_la_de_magi(
            "extra-keys = [['ESC','TAB'],['F1']]\n",
            "Ctrl+]"
        ));
        // Y si la de MAGI está comentada, no cuenta.
        assert!(!es_la_de_magi(&format!("# {linea}\n"), "Ctrl+]"));
    }

    /// E5: solo el espacio que separa (ASCII, fuera de las comillas) es
    /// indiferente; lo que va dentro de una comilla o es de otra clase cuenta.
    #[test]
    fn ya_es_la_de_magi_distingue_lo_que_va_dentro_de_las_comillas() {
        let de_magi = |texto: &str| es_la_de_magi(&format!("extra-keys = {texto}\n"), "Ctrl+]");
        let valor = valor_de_la_fila(DEFECTO);
        assert!(de_magi(valor));
        // Un macro que difiere solo en espacios dentro de la comilla es otra tecla.
        assert!(!de_magi(&valor.replace("'CTRL ]'", "'CTRL]'")));
        assert!(!de_magi(&valor.replace("'CTRL ]'", "'CTRL  ]'")));
        assert!(!de_magi(&valor.replace("'CTRL ]'", "' CTRL ]'")));
        assert!(!de_magi(&valor.replace("'^]'", "'^ ]'")));
        // Un espacio de Unicode (U+00A0) entre elementos no es separador para
        // `org.json`: no es la fila de MAGI. Tampoco el de ancho cero.
        for raro in ['\u{a0}', '\u{2003}', '\u{3000}', '\u{200b}'] {
            assert!(!de_magi(
                &valor.replace("','TAB'", &format!("',{raro}'TAB'"))
            ));
            assert!(!de_magi(
                &valor.replace("{macro:", &format!("{{macro:{raro}"))
            ));
            assert!(!de_magi(&format!("{valor}{raro}")));
        }
        // Un espacio dentro de una palabra sin comillas tampoco se descarta.
        assert!(!de_magi(&valor.replace("macro:", "ma cro:")));
        assert!(!de_magi(&valor.replace("display:", "dis play:")));
        // Sí valen espacios, tabuladores y avances de página que separan, y una
        // fila partida en líneas (la sangría de la continuación se pierde).
        assert!(de_magi(&valor.replace(',', " ,\t").replace('[', "[ \u{c}")));
        assert!(de_magi(&format!("  {valor}  ")));
        assert!(de_magi(&valor.replace("macro: '", "macro:\t'")));
        assert!(de_magi(&valor.replace("{macro", "{ macro")));
        // Y el estado lo refleja: con el espacio de dentro cambiado, hay que
        // ofrecer la fila (y la instalación la cambia).
        let con_macro_distinta = format!("{}\n", DEFECTO.replace("'CTRL ]'", "'CTRL]'"));
        assert_eq!(
            estado_de_texto(&con_macro_distinta, "Ctrl+]"),
            EstadoFila::Otra
        );
    }

    #[test]
    fn el_estado_distingue_los_cuatro_casos_del_texto() {
        assert_eq!(estado_de_texto("a=1\n", "Ctrl+]"), EstadoFila::SinExtraKeys);
        assert_eq!(estado_de_texto("", "Ctrl+]"), EstadoFila::SinExtraKeys);
        assert_eq!(
            estado_de_texto("extra-keys = [['A']]\n", "Ctrl+]"),
            EstadoFila::Otra
        );
        assert_eq!(
            estado_de_texto(&format!("{DEFECTO}\n"), "Ctrl+]"),
            EstadoFila::LaDeMagi
        );
    }

    // ------------------------------------------------------------ el fichero

    #[test]
    fn la_ruta_es_la_de_termux_en_el_orden_de_termux() {
        let temporal = tempfile::tempdir().unwrap();
        let home = temporal.path();
        let entorno = entorno_en(home);
        let principal = home.join(".termux/termux.properties");
        let secundaria = home.join(".config/termux/termux.properties");
        // Ninguno: se crearía el principal.
        assert_eq!(ruta_properties(&entorno), Some(principal.clone()));
        // Solo el de ~/.config: ese.
        std::fs::create_dir_all(secundaria.parent().unwrap()).unwrap();
        std::fs::write(&secundaria, "a=1\n").unwrap();
        assert_eq!(ruta_properties(&entorno), Some(secundaria.clone()));
        // Con los dos manda ~/.termux (es el que Termux lee).
        std::fs::create_dir_all(principal.parent().unwrap()).unwrap();
        std::fs::write(&principal, "b=2\n").unwrap();
        assert_eq!(ruta_properties(&entorno), Some(principal));
    }

    #[test]
    fn sin_home_absoluto_no_hay_ruta() {
        assert_eq!(ruta_properties(&EntornoTermux::default()), None);
        let relativo = EntornoTermux {
            home: Some(PathBuf::from("relativo")),
            ..EntornoTermux::default()
        };
        assert_eq!(ruta_properties(&relativo), None);
        assert!(matches!(
            estado_fila(&EntornoTermux::default(), "Ctrl+]"),
            EstadoFila::Ilegible(_)
        ));
    }

    #[test]
    fn leer_distingue_lo_que_no_se_puede_leer_de_lo_que_no_hay() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path();
        // No existe.
        assert_eq!(leer_properties(&carpeta.join("no")), Ok(None));
        // Demasiado grande.
        let grande = carpeta.join("grande");
        std::fs::write(&grande, vec![b'a'; TOPE_PROPERTIES + 1]).unwrap();
        assert!(leer_properties(&grande).unwrap_err().contains("256 KiB"));
        // Justo en el tope, se lee.
        let justo = carpeta.join("justo");
        std::fs::write(&justo, vec![b'a'; TOPE_PROPERTIES]).unwrap();
        assert!(leer_properties(&justo).unwrap().is_some());
        // No UTF-8.
        let binario = carpeta.join("binario");
        std::fs::write(&binario, [0xff, 0xfe, b'a']).unwrap();
        assert!(leer_properties(&binario).unwrap_err().contains("UTF-8"));
        // Un directorio.
        assert!(leer_properties(carpeta).unwrap_err().contains("normal"));
        // Un enlace roto no es «no existe».
        let roto = carpeta.join("roto");
        std::os::unix::fs::symlink(carpeta.join("nada"), &roto).unwrap();
        assert!(leer_properties(&roto).unwrap_err().contains("enlace roto"));
        // Ningún motivo lleva el contenido ni la ruta.
        for motivo in [
            leer_properties(&grande).unwrap_err(),
            leer_properties(&binario).unwrap_err(),
        ] {
            assert!(!motivo.contains(carpeta.to_str().unwrap()), "{motivo}");
        }
    }

    #[test]
    fn un_fifo_no_se_abre_ni_se_cuelga() {
        let temporal = tempfile::tempdir().unwrap();
        let fifo = temporal.path().join("fifo");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
        assert!(leer_properties(&fifo).unwrap_err().contains("normal"));
    }

    #[test]
    fn el_estado_del_fichero_sale_de_su_contenido() {
        let temporal = tempfile::tempdir().unwrap();
        let entorno = entorno_en(temporal.path());
        let ruta = temporal.path().join(".termux/termux.properties");
        assert_eq!(estado_fila(&entorno, "Ctrl+]"), EstadoFila::SinFichero);
        std::fs::create_dir_all(ruta.parent().unwrap()).unwrap();
        std::fs::write(&ruta, "a=1\n").unwrap();
        assert_eq!(estado_fila(&entorno, "Ctrl+]"), EstadoFila::SinExtraKeys);
        std::fs::write(&ruta, "extra-keys = [['X']]\n").unwrap();
        assert_eq!(estado_fila(&entorno, "Ctrl+]"), EstadoFila::Otra);
        std::fs::write(&ruta, format!("{DEFECTO}\n")).unwrap();
        assert_eq!(estado_fila(&entorno, "Ctrl+]"), EstadoFila::LaDeMagi);
        std::fs::write(&ruta, [0xff, 0xfe]).unwrap();
        assert!(matches!(
            estado_fila(&entorno, "Ctrl+]"),
            EstadoFila::Ilegible(_)
        ));
    }

    // ------------------------------------------------------------ las copias

    #[test]
    fn la_copia_lleva_la_fecha_y_un_sufijo_si_ya_existe() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join("termux.properties");
        assert_eq!(
            nombre_de_copia(&ruta, fecha()),
            "termux.properties.magi-2026-11-17"
        );
        std::fs::write(
            temporal.path().join("termux.properties.magi-2026-11-17"),
            "",
        )
        .unwrap();
        assert_eq!(
            nombre_de_copia(&ruta, fecha()),
            "termux.properties.magi-2026-11-17-2"
        );
        std::fs::write(
            temporal.path().join("termux.properties.magi-2026-11-17-2"),
            "",
        )
        .unwrap();
        assert_eq!(
            nombre_de_copia(&ruta, fecha()),
            "termux.properties.magi-2026-11-17-3"
        );
    }

    #[test]
    fn instalar_en_un_fichero_nuevo_crea_el_directorio_y_el_fichero_con_modos_estrechos() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join(".termux/termux.properties");
        let escritura = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap();
        assert!(!escritura.ya_estaba);
        assert_eq!(escritura.copia, None);
        assert_eq!(escritura.detalle_de_registro(), "no existía");
        assert_eq!(
            std::fs::read_to_string(&ruta).unwrap(),
            format!("{COMENTARIO_FILA}\n{DEFECTO}\n")
        );
        let modo = |ruta: &Path| std::fs::metadata(ruta).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo(&ruta), 0o600);
        assert_eq!(modo(ruta.parent().unwrap()), 0o700);
    }

    #[test]
    fn instalar_hace_la_copia_antes_y_conserva_los_permisos() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join("termux.properties");
        let original = "a=1\nextra-keys = [['A']]\nb=2\n";
        std::fs::write(&ruta, original).unwrap();
        std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o640)).unwrap();
        let escritura = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap();
        assert_eq!(
            escritura.copia.as_deref(),
            Some("termux.properties.magi-2026-11-17")
        );
        assert_eq!(
            escritura.detalle_de_registro(),
            "termux.properties.magi-2026-11-17"
        );
        let copia = temporal.path().join("termux.properties.magi-2026-11-17");
        assert_eq!(std::fs::read_to_string(&copia).unwrap(), original);
        assert_eq!(
            std::fs::metadata(&copia).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(
            std::fs::read_to_string(&ruta).unwrap(),
            format!("a=1\n{DEFECTO}\nb=2\n")
        );
        assert_eq!(
            std::fs::metadata(&ruta).unwrap().permissions().mode() & 0o777,
            0o640
        );
        // Una segunda copia el mismo día lleva sufijo y la primera no se pisa.
        std::fs::write(&ruta, "x=1\n").unwrap();
        let otra = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap();
        assert_eq!(
            otra.copia.as_deref(),
            Some("termux.properties.magi-2026-11-17-2")
        );
        assert_eq!(std::fs::read_to_string(&copia).unwrap(), original);
    }

    #[test]
    fn si_ya_es_la_de_magi_no_hay_copia_ni_escritura() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join("termux.properties");
        // Con otro espaciado y un resto del usuario: ni un byte cambia.
        let texto = format!("# mío\nx = 1\r\n{DEFECTO}   \r\n");
        std::fs::write(&ruta, &texto).unwrap();
        let antes = std::fs::metadata(&ruta).unwrap().modified().unwrap();
        let escritura = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap();
        assert!(escritura.ya_estaba);
        assert_eq!(escritura.copia, None);
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), texto);
        assert_eq!(std::fs::metadata(&ruta).unwrap().modified().unwrap(), antes);
        let nombres: Vec<_> = std::fs::read_dir(temporal.path())
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name())
            .collect();
        assert_eq!(nombres.len(), 1, "{nombres:?}");
    }

    #[test]
    fn un_fichero_ilegible_no_se_toca_ni_se_copia() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join("termux.properties");
        std::fs::write(&ruta, [0xff_u8, 0xfe, 0x00]).unwrap();
        let error = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap_err();
        assert!(matches!(error, ErrorTeclas::Ilegible(_)), "{error}");
        assert!(error.to_string().contains("edítalo a mano"));
        assert_eq!(std::fs::read(&ruta).unwrap(), [0xff, 0xfe, 0x00]);
        assert_eq!(std::fs::read_dir(temporal.path()).unwrap().count(), 1);
        // Demasiado grande, igual.
        std::fs::write(&ruta, vec![b'a'; TOPE_PROPERTIES + 1]).unwrap();
        assert!(matches!(
            escribir_fila(&ruta, "Ctrl+]", fecha()),
            Err(ErrorTeclas::Ilegible(_))
        ));
        assert_eq!(std::fs::read_dir(temporal.path()).unwrap().count(), 1);
    }

    #[test]
    fn si_la_copia_falla_no_se_escribe_nada() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path().join("ajustes");
        std::fs::create_dir(&carpeta).unwrap();
        let ruta = carpeta.join("termux.properties");
        std::fs::write(&ruta, "a=1\n").unwrap();
        // El directorio sin permiso de escritura: no cabe ni la copia ni la
        // escritura. (Con root la prueba no demuestra nada y se omite.)
        std::fs::set_permissions(&carpeta, std::fs::Permissions::from_mode(0o500)).unwrap();
        let resultado = escribir_fila(&ruta, "Ctrl+]", fecha());
        std::fs::set_permissions(&carpeta, std::fs::Permissions::from_mode(0o700)).unwrap();
        if nix::unistd::Uid::effective().is_root() {
            return;
        }
        assert!(
            matches!(resultado, Err(ErrorTeclas::Copia(_))),
            "{resultado:?}"
        );
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a=1\n");
        assert_eq!(std::fs::read_dir(&carpeta).unwrap().count(), 1);
    }

    #[test]
    fn un_enlace_se_escribe_en_su_destino_y_sigue_siendo_enlace() {
        let temporal = tempfile::tempdir().unwrap();
        let destino = temporal.path().join("dotfiles-termux.properties");
        std::fs::write(&destino, "a=1\n").unwrap();
        let carpeta = temporal.path().join(".termux");
        std::fs::create_dir(&carpeta).unwrap();
        let ruta = carpeta.join("termux.properties");
        std::os::unix::fs::symlink(&destino, &ruta).unwrap();
        escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap();
        assert!(std::fs::symlink_metadata(&ruta)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::read_to_string(&destino).unwrap().contains(DEFECTO));
        // E2: la copia queda junto al destino (con su nombre), no junto al
        // enlace: es el fichero que se ha sobrescrito.
        assert_eq!(
            std::fs::read_to_string(
                temporal
                    .path()
                    .join("dotfiles-termux.properties.magi-2026-11-17")
            )
            .unwrap(),
            "a=1\n"
        );
        assert_eq!(entradas_de(&carpeta), ["termux.properties"]);
    }

    fn entradas_de(carpeta: &Path) -> Vec<String> {
        let mut nombres: Vec<String> = std::fs::read_dir(carpeta)
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        nombres.sort();
        nombres
    }

    /// E2: la propuesta de un enlace dice a dónde apunta y pone la copia junto
    /// al destino; sin enlace no dice nada de eso.
    #[test]
    fn la_propuesta_de_un_enlace_dice_el_destino_y_la_copia_va_junto_a_el() {
        let temporal = tempfile::tempdir().unwrap();
        let home = temporal.path().join("home");
        let fuera = temporal.path().join("almacenamiento");
        std::fs::create_dir_all(home.join(".termux")).unwrap();
        std::fs::create_dir_all(&fuera).unwrap();
        // Los dos ya resueltos (el directorio temporal puede colgar de un
        // enlace en algunos sistemas).
        let home = std::fs::canonicalize(&home).unwrap();
        let fuera = std::fs::canonicalize(&fuera).unwrap();
        let entorno = entorno_en(&home);
        let ruta = home.join(".termux/termux.properties");

        // Un fichero normal: sin enlace.
        std::fs::write(&ruta, "extra-keys = [['A']]\n").unwrap();
        let normal = preparar(&entorno, "Ctrl+]", fecha());
        assert_eq!(normal.enlace_a, None);
        assert_eq!(
            normal.copia.as_deref(),
            Some("termux.properties.magi-2026-11-17")
        );

        // Un enlace a algo fuera del home: lo dice, entero, y la copia lleva el
        // nombre del destino.
        std::fs::remove_file(&ruta).unwrap();
        std::fs::write(fuera.join("props.txt"), "extra-keys = [['A']]\n").unwrap();
        std::os::unix::fs::symlink(fuera.join("props.txt"), &ruta).unwrap();
        let enlazada = preparar(&entorno, "Ctrl+]", fecha());
        assert_eq!(enlazada.ruta_visible, "~/.termux/termux.properties");
        assert_eq!(
            enlazada.enlace_a.as_deref(),
            Some(fuera.join("props.txt").to_str().unwrap())
        );
        assert_eq!(enlazada.copia.as_deref(), Some("props.txt.magi-2026-11-17"));
        assert_eq!(enlazada.estado, EstadoFila::Otra);
        // Mirar no escribe nada.
        assert_eq!(entradas_de(&fuera), ["props.txt"]);

        // Un enlace dentro del home: se enseña con `~`.
        let dentro = home.join("dotfiles");
        std::fs::create_dir(&dentro).unwrap();
        std::fs::rename(fuera.join("props.txt"), dentro.join("tp")).unwrap();
        std::fs::remove_file(&ruta).unwrap();
        std::os::unix::fs::symlink(dentro.join("tp"), &ruta).unwrap();
        let en_home = preparar(&entorno, "Ctrl+]", fecha());
        assert_eq!(en_home.enlace_a.as_deref(), Some("~/dotfiles/tp"));

        // Un enlace roto es ilegible y no dice a dónde apunta.
        std::fs::remove_file(dentro.join("tp")).unwrap();
        let rota = preparar(&entorno, "Ctrl+]", fecha());
        assert_eq!(rota.enlace_a, None);
        assert!(matches!(rota.estado, EstadoFila::Ilegible(_)));
    }

    /// E2: lo que se ve en la propuesta es lo que pasa: el destino de un enlace
    /// fuera de `~/.termux` recibe la fila y la copia, y el enlace no se toca.
    #[test]
    fn un_enlace_fuera_del_home_recibe_la_copia_y_la_fila_y_sigue_siendo_enlace() {
        let temporal = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(temporal.path()).unwrap().join("home");
        let fuera = std::fs::canonicalize(temporal.path())
            .unwrap()
            .join("almacenamiento");
        std::fs::create_dir_all(home.join(".termux")).unwrap();
        std::fs::create_dir_all(&fuera).unwrap();
        let destino = fuera.join("props.txt");
        std::fs::write(&destino, "a=1\nextra-keys = [['A']]\n").unwrap();
        let ruta = home.join(".termux/termux.properties");
        std::os::unix::fs::symlink(&destino, &ruta).unwrap();
        let entorno = entorno_en(&home);
        let propuesta = preparar(&entorno, "Ctrl+]", fecha());
        let copia = propuesta.copia.clone().expect("copia prevista");

        let escritura = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap();
        // La copia es la que se prometió, en la carpeta del destino.
        assert_eq!(escritura.copia.as_deref(), Some(copia.as_str()));
        assert_eq!(
            std::fs::read_to_string(fuera.join(&copia)).unwrap(),
            "a=1\nextra-keys = [['A']]\n"
        );
        assert_eq!(entradas_de(&home.join(".termux")), ["termux.properties"]);
        assert_eq!(
            std::fs::read_to_string(&destino).unwrap(),
            format!("a=1\n{DEFECTO}\n")
        );
        assert!(std::fs::symlink_metadata(&ruta)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(estado_fila(&entorno, "Ctrl+]"), EstadoFila::LaDeMagi);
    }

    /// E2: un destino que no admite `chmod` (el almacenamiento compartido de
    /// Android) no impide instalar la fila ni hacer la copia.
    #[test]
    fn un_destino_que_no_admite_chmod_no_impide_instalar_la_fila() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = std::fs::canonicalize(temporal.path()).unwrap();
        let destino = carpeta.join("termux.properties");
        std::fs::write(&destino, "a=1\n").unwrap();
        let _sin_chmod = ganchos_de_pruebas::sin_chmod_en(&destino);
        let escritura = escribir_fila(&destino, "Ctrl+]", fecha()).unwrap();
        assert_eq!(
            escritura.copia.as_deref(),
            Some("termux.properties.magi-2026-11-17")
        );
        assert_eq!(
            std::fs::read_to_string(carpeta.join("termux.properties.magi-2026-11-17")).unwrap(),
            "a=1\n"
        );
        assert!(std::fs::read_to_string(&destino).unwrap().contains(DEFECTO));
        // No queda ningún temporal.
        assert_eq!(
            entradas_de(&carpeta),
            ["termux.properties", "termux.properties.magi-2026-11-17"]
        );

        // Un fichero nuevo, igual.
        let nuevo = carpeta.join("nueva/termux.properties");
        let _sin_chmod = ganchos_de_pruebas::sin_chmod_en(&nuevo);
        let escritura = escribir_fila(&nuevo, "Ctrl+]", fecha()).unwrap();
        assert_eq!(escritura.copia, None);
        assert!(std::fs::read_to_string(&nuevo).unwrap().contains(DEFECTO));
    }

    /// Un directorio sin permiso de escritura mientras vive (con root no
    /// demuestra nada: `None`).
    struct SinEscritura(PathBuf);

    impl SinEscritura {
        fn de(directorio: &Path) -> Option<Self> {
            if nix::unistd::Uid::effective().is_root() {
                return None;
            }
            std::fs::set_permissions(directorio, std::fs::Permissions::from_mode(0o500)).unwrap();
            Some(Self(directorio.to_path_buf()))
        }
    }

    impl Drop for SinEscritura {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        }
    }

    /// Un `termux.properties` que es un enlace a unos dotfiles: la copia y la
    /// escritura van las dos al destino (a su directorio). Devuelve el enlace
    /// y el directorio de los dotfiles, ya resuelto.
    fn con_destino_en_otro_directorio(raiz: &Path, contenido: &str) -> (PathBuf, PathBuf) {
        let raiz = std::fs::canonicalize(raiz).unwrap();
        let dotfiles = raiz.join("dotfiles");
        std::fs::create_dir(&dotfiles).unwrap();
        std::fs::write(dotfiles.join("termux.properties"), contenido).unwrap();
        let carpeta = raiz.join(".termux");
        std::fs::create_dir(&carpeta).unwrap();
        let ruta = carpeta.join("termux.properties");
        std::os::unix::fs::symlink(dotfiles.join("termux.properties"), &ruta).unwrap();
        (ruta, dotfiles)
    }

    /// Hace que la escritura de `fichero` falle justo después de la copia: el
    /// gancho deja su directorio sin permiso de escritura en ese momento (la
    /// copia y la escritura van al mismo directorio, así que no se puede
    /// quitar antes). Al soltarse, devuelve el permiso y quita el gancho.
    struct FalloTrasLaCopia {
        directorio: PathBuf,
        _gancho: ganchos_de_pruebas::Guarda,
    }

    impl FalloTrasLaCopia {
        fn en(fichero: &Path) -> Option<Self> {
            if nix::unistd::Uid::effective().is_root() {
                return None;
            }
            let directorio = fichero.parent().unwrap().to_path_buf();
            let a_quitar = directorio.clone();
            let gancho = ganchos_de_pruebas::antes_de_escribir_en(fichero, move || {
                std::fs::set_permissions(&a_quitar, std::fs::Permissions::from_mode(0o500))
                    .unwrap();
            });
            Some(Self {
                directorio,
                _gancho: gancho,
            })
        }
    }

    impl Drop for FalloTrasLaCopia {
        fn drop(&mut self) {
            let _ =
                std::fs::set_permissions(&self.directorio, std::fs::Permissions::from_mode(0o700));
        }
    }

    fn copias_de(carpeta: &Path) -> Vec<String> {
        let mut copias: Vec<String> = std::fs::read_dir(carpeta)
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|nombre| nombre.contains(".magi-"))
            .collect();
        copias.sort();
        copias
    }

    #[test]
    fn si_la_escritura_falla_tras_la_copia_el_error_dice_que_la_copia_ya_esta_hecha() {
        let temporal = tempfile::tempdir().unwrap();
        let (ruta, dotfiles) = con_destino_en_otro_directorio(temporal.path(), "a=1\n");
        let Some(_guarda) = FalloTrasLaCopia::en(&dotfiles.join("termux.properties")) else {
            return;
        };
        let error = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap_err();
        assert_eq!(
            error,
            ErrorTeclas::EscrituraTrasCopia {
                motivo: "permiso denegado".to_string(),
                copia: "termux.properties.magi-2026-11-17".to_string(),
            }
        );
        assert_eq!(
            error.copia_hecha(),
            Some("termux.properties.magi-2026-11-17")
        );
        assert_eq!(
            error.to_string(),
            "no se pudo escribir termux.properties (permiso denegado): la copia termux.properties.magi-2026-11-17 ya está hecha"
        );
        // E2: la copia existe, junto al destino (no junto al enlace), y es el
        // original; el fichero no se ha tocado.
        assert_eq!(
            std::fs::read_to_string(dotfiles.join("termux.properties.magi-2026-11-17")).unwrap(),
            "a=1\n"
        );
        assert_eq!(copias_de(ruta.parent().unwrap()), Vec::<String>::new());
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a=1\n");
    }

    #[test]
    fn si_la_escritura_falla_sin_copia_no_se_habla_de_ninguna() {
        // El fichero no existe y no se puede crear su directorio: no hay nada
        // que copiar, así que no queda copia ni el error la nombra.
        let temporal = tempfile::tempdir().unwrap();
        let Some(_guarda) = SinEscritura::de(temporal.path()) else {
            return;
        };
        let ruta = temporal.path().join(".termux/termux.properties");
        let error = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap_err();
        assert!(matches!(error, ErrorTeclas::Escritura(_)), "{error:?}");
        assert_eq!(error.copia_hecha(), None);
        assert!(!error.to_string().contains("copia"), "{error}");
    }

    #[test]
    fn un_reintento_con_la_copia_previa_intacta_no_hace_otra_y_la_reutiliza() {
        let temporal = tempfile::tempdir().unwrap();
        let (ruta, dotfiles) = con_destino_en_otro_directorio(temporal.path(), "a=1\n");
        // E2: la copia queda junto al destino, en los dotfiles.
        let carpeta = dotfiles.clone();
        let Some(guarda) = FalloTrasLaCopia::en(&dotfiles.join("termux.properties")) else {
            return;
        };
        let error = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap_err();
        let copia = error.copia_hecha().expect("copia hecha").to_string();
        assert_eq!(copias_de(&carpeta), std::slice::from_ref(&copia));

        // Otro intento sin saber de la copia anterior (la CLI, otra ventana):
        // hace la suya y se apila (`-2`). (El fallo anterior dejó el
        // directorio sin permiso: quien lo arregla es el usuario.)
        std::fs::set_permissions(&dotfiles, std::fs::Permissions::from_mode(0o700)).unwrap();
        let otro = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap_err();
        assert_eq!(
            otro.copia_hecha(),
            Some("termux.properties.magi-2026-11-17-2")
        );
        std::fs::set_permissions(&dotfiles, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_file(carpeta.join("termux.properties.magi-2026-11-17-2")).unwrap();

        // Con la copia previa, el mismo fallo no duplica nada y repite el nombre.
        let reintento =
            escribir_fila_con_copia_previa(&ruta, "Ctrl+]", fecha(), Some(&copia)).unwrap_err();
        assert_eq!(reintento.copia_hecha(), Some(copia.as_str()));
        assert_eq!(copias_de(&carpeta), std::slice::from_ref(&copia));

        // Arreglado el permiso, el reintento escribe con la copia de antes.
        drop(guarda);
        let escritura =
            escribir_fila_con_copia_previa(&ruta, "Ctrl+]", fecha(), Some(&copia)).unwrap();
        assert!(escritura.copia_reutilizada);
        assert_eq!(escritura.copia.as_deref(), Some(copia.as_str()));
        assert_eq!(escritura.detalle_de_registro(), copia);
        assert_eq!(copias_de(&carpeta), std::slice::from_ref(&copia));
        assert_eq!(
            std::fs::read_to_string(carpeta.join(&copia)).unwrap(),
            "a=1\n"
        );
        assert_eq!(
            std::fs::read_to_string(&ruta).unwrap(),
            format!("a=1\n{COMENTARIO_FILA}\n{}\n", fila("Ctrl+]"))
        );
        // Una instalación normal (sin copia previa) no reutiliza nada.
        std::fs::write(&ruta, "b=2\n").unwrap();
        let normal = escribir_fila(&ruta, "Ctrl+]", fecha()).unwrap();
        assert!(!normal.copia_reutilizada);
        assert_eq!(
            normal.copia.as_deref(),
            Some("termux.properties.magi-2026-11-17-2")
        );
    }

    #[test]
    fn una_copia_previa_que_no_es_identica_o_no_es_de_fiar_no_se_reutiliza() {
        let temporal = tempfile::tempdir().unwrap();
        let carpeta = temporal.path();
        let ruta = carpeta.join("termux.properties");
        let previa = "termux.properties.magi-2026-11-16";
        let intentar = |previa: &str| {
            escribir_fila_con_copia_previa(&ruta, "Ctrl+]", fecha(), Some(previa)).unwrap()
        };

        // La copia ya no es igual que el fichero (alguien la tocó).
        std::fs::write(&ruta, "a=1\n").unwrap();
        std::fs::write(carpeta.join(previa), "otra cosa\n").unwrap();
        let escritura = intentar(previa);
        assert!(!escritura.copia_reutilizada);
        assert_eq!(
            escritura.copia.as_deref(),
            Some("termux.properties.magi-2026-11-17")
        );

        // El fichero cambió desde el intento que dejó la copia.
        std::fs::write(&ruta, "a=2\n").unwrap();
        std::fs::write(carpeta.join(previa), "a=1\n").unwrap();
        assert!(!intentar(previa).copia_reutilizada);

        // Idéntica, pero con un nombre que no es el de una copia de MAGI de
        // este fichero, con directorios o que no existe.
        for nombre in [
            "../fuera",
            "/etc/passwd",
            "sub/termux.properties.magi-2026-11-16",
            "otro.magi-2026-11-16",
            "termux.properties",
            "termux.properties.magi-2026-11-01",
            "",
            "..",
        ] {
            std::fs::write(&ruta, "a=3\n").unwrap();
            std::fs::write(carpeta.join(previa), "a=3\n").unwrap();
            std::fs::write(carpeta.join("otro.magi-2026-11-16"), "a=3\n").unwrap();
            let escritura = intentar(nombre);
            assert!(!escritura.copia_reutilizada, "«{nombre}»");
        }

        // Un enlace con nombre de copia no vale, aunque apunte a algo idéntico.
        std::fs::write(&ruta, "a=4\n").unwrap();
        std::fs::write(carpeta.join("identico"), "a=4\n").unwrap();
        let enlace = "termux.properties.magi-2026-11-15";
        std::os::unix::fs::symlink(carpeta.join("identico"), carpeta.join(enlace)).unwrap();
        assert!(!intentar(enlace).copia_reutilizada);

        // Y una copia previa de un fichero que no existía (nada que copiar).
        let nuevo = tempfile::tempdir().unwrap();
        let ruta_nueva = nuevo.path().join(".termux/termux.properties");
        let escritura =
            escribir_fila_con_copia_previa(&ruta_nueva, "Ctrl+]", fecha(), Some(previa)).unwrap();
        assert!(!escritura.copia_reutilizada);
        assert_eq!(escritura.copia, None);
    }

    #[test]
    fn la_propuesta_junta_ruta_estado_y_copia() {
        let temporal = tempfile::tempdir().unwrap();
        let entorno = entorno_en(temporal.path());
        let sin_fichero = preparar(&entorno, "Ctrl+]", fecha());
        assert_eq!(sin_fichero.estado, EstadoFila::SinFichero);
        assert_eq!(sin_fichero.ruta_visible, "~/.termux/termux.properties");
        assert_eq!(sin_fichero.copia, None);
        assert_eq!(sin_fichero.actual, None);
        assert_eq!(sin_fichero.fila, DEFECTO);
        assert_eq!(sin_fichero.arriba.len(), 9);
        assert_eq!(sin_fichero.aviso_prefijo, None);

        let ruta = temporal.path().join(".termux/termux.properties");
        std::fs::create_dir_all(ruta.parent().unwrap()).unwrap();
        std::fs::write(&ruta, "extra-keys = [['A']]\n").unwrap();
        let con_otra = preparar(&entorno, "Ctrl+]", fecha());
        assert_eq!(con_otra.estado, EstadoFila::Otra);
        assert_eq!(
            con_otra.copia.as_deref(),
            Some("termux.properties.magi-2026-11-17")
        );
        assert_eq!(con_otra.actual.as_deref(), Some("[['A']]"));

        std::fs::write(&ruta, format!("{DEFECTO}\n")).unwrap();
        let ya = preparar(&entorno, "Ctrl+]", fecha());
        assert_eq!(ya.estado, EstadoFila::LaDeMagi);
        assert_eq!(ya.copia, None, "no hay copia que hacer");

        let sin_macro = preparar(&entorno, "Ctrl+'", fecha());
        assert!(sin_macro.aviso_prefijo.is_some());
        assert_eq!(sin_macro.arriba.len(), 8);
    }

    #[test]
    fn la_fila_actual_se_acota_y_va_en_una_linea() {
        let largo = format!("[{}]", "'X',".repeat(500));
        let acotada = acotar(&largo);
        assert!(acotada.chars().count() <= TOPE_FILA_ACTUAL + 1);
        assert!(acotada.ends_with('…'));
        assert_eq!(acotar("a\tb\u{1b}c"), "a b c");
    }

    #[test]
    fn el_texto_de_la_recarga_dice_que_hacer() {
        assert!(texto_de_recarga(&Ok(())).contains("ya se ve"));
        assert_eq!(
            texto_de_recarga(&Err(FalloOrden::SinRespuesta)),
            REINICIA_TERMUX
        );
        assert_eq!(
            texto_de_recarga(&Err(FalloOrden::NoInstalada(ORDEN_RECARGAR.into()))),
            REINICIA_TERMUX
        );
    }
}
