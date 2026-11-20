//! `magi llavero estado | cambiar | borrar` y el llavero de las demás órdenes
//! de la CLI (Fase 17, Termux).
//!
//! Las subórdenes no van bajo `cfg(target_os)`: fuera de Termux dicen por qué
//! no aplican y salen con 1. En Termux el llavero es un fichero `age` con frase
//! maestra (`LlaveroFichero`), y esta CLI es su única interfaz fuera de la TUI:
//!
//! - `estado` mira el fichero sin abrirlo: nunca pide la frase ni enseña
//!   entradas.
//! - `cambiar` y `borrar` hablan con el usuario en el terminal (la frase se
//!   escribe sin eco y la confirmación de borrar, con la palabra `borrar`); sin
//!   terminal dicen cómo hacerlo y salen con 1 sin tocar nada.
//! - `abrir_para_cli` es el llavero de las demás órdenes (`magi sincronizar`,
//!   `magi sondear`): si está cerrado, pide la frase en el terminal (tres
//!   intentos) y, sin terminal, da un error con instrucción. Nunca abre
//!   diálogos de la TUI. Cada orden lo abre UNA vez y lo pasa a quien lo
//!   necesite: un llavero nuevo estaría cerrado y volvería a preguntar.
//!
//! Todo lo que habla con la persona pasa por el trait [`Consola`], para probar
//! el flujo sin un terminal de verdad; [`ConsolaTerminal`] es la real. La frase
//! se lee con crossterm en modo raw (sin eco), con una guarda de `Drop` que
//! devuelve el terminal a como estaba en todas las ramas de la lectura (error o
//! pánico incluidos), y vive en un `Zeroizing` con la capacidad reservada de
//! antemano, para que el búfer no se realoje al crecer y deje copias en
//! memoria liberada. Se borra lo que está en nuestras manos; no se promete
//! más: crossterm guarda lo tecleado en su búfer interno de lectura (hasta
//! 1 KiB) y en el `Vec` de su analizador, sin `zeroize`, y la cola del
//! terminal es del sistema. Nunca va a un log ni a REGISTRO ni a un mensaje de
//! error.
//!
//! Límites conocidos de la lectura sin eco:
//!
//! - Antes de preguntar se descarta lo tecleado por adelantado (la cola del
//!   terminal y lo que crossterm ya hubiera leído): un pegado de varias
//!   líneas no contesta preguntas que la persona todavía no ha visto.
//! - Si el terminal desaparece (se cierra la ventana con SIGHUP ignorado), la
//!   lectura acaba con un error en vez de girar: crossterm no devuelve error
//!   ante el fin de fichero del terminal y se queda dando vueltas dentro de su
//!   lectura, así que aquí se vigila el descriptor ANTES de llamarle (con
//!   `poll(2)`, cada 500 ms mientras no hay teclas).
//! - No hay manejadores de señales: con SIGTERM o SIGHUP sin ignorar el
//!   proceso muere con el terminal todavía en modo raw (sin eco). `reset` o
//!   `stty sane` lo devuelve a su estado.

use std::collections::HashMap;
use std::io::{BufRead as _, IsTerminal as _, Read as _, Write as _};
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use zeroize::{Zeroize as _, Zeroizing};

use magi_nucleo::llavero_fichero::{
    info_fichero, preparar_frase, ErrorLlavero, EstadoFichero, LlaveroFichero, MIN_FRASE,
};

use crate::config::Rutas;
use crate::llavero::{EstadoLlavero, LlaveroCliente};
use crate::modelo::{Host, ResultadoRegistro};
use crate::registro;

/// Intentos de frase que da `abrir_para_cli` antes de rendirse. La serie de
/// fallos del llavero (5 seguidos, 30 s de espera) vive en la memoria del
/// proceso, así que con tres intentos por orden nunca se llega a ella desde la
/// CLI; si algún día se llegase (un reloj propio en las pruebas), se respeta.
pub const INTENTOS_FRASE: u32 = 3;

/// Bytes que admite la lectura de una frase: más que `MAX_FRASE_BYTES` para
/// que una frase demasiado larga se diga «demasiado larga» y no se corte en
/// silencio, pero acotado para que un pegado enorme no crezca sin medida.
pub const TOPE_LECTURA: usize = 4096;

/// Lo que pide `magi llavero …`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdenLlavero {
    /// Si hay llavero, su ruta, su tamaño y su fecha; nunca pide la frase.
    Estado,
    /// Pide en el TTY la frase actual y la nueva dos veces, y recifra.
    Cambiar,
    /// Explica qué se pierde y pide escribir `borrar`.
    Borrar,
}

// ------------------------------------------------------------------ la consola

/// Por qué no se pudo leer algo de la persona.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorEntrada {
    /// Canceló (Ctrl+C, Ctrl+D) o se acabó la entrada.
    Cancelada,
    /// El terminal falló, con la causa en una línea (nunca lleva lo escrito).
    Terminal(String),
}

/// Con quién habla la CLI del llavero: la persona del terminal, o un doble en
/// las pruebas.
pub trait Consola {
    /// ¿Se puede preguntar? Hace falta un terminal en la entrada estándar (para
    /// leer sin eco) y en la salida de error (donde van las preguntas).
    fn es_interactiva(&self) -> bool;

    /// Pregunta algo secreto: se escribe sin eco. Se acepta con Intro y se
    /// cancela con Ctrl+C o Ctrl+D.
    fn leer_oculto(&mut self, pregunta: &str) -> Result<Zeroizing<String>, ErrorEntrada>;

    /// Pregunta una línea con eco (la confirmación de `borrar`). Con el final
    /// de la entrada, `Cancelada`.
    fn leer_linea(&mut self, pregunta: &str) -> Result<String, ErrorEntrada>;

    /// Un resultado, por la salida estándar (con salto de línea al final).
    fn decir(&mut self, texto: &str);

    /// Un aviso, un error o una explicación, por la salida de error (con salto
    /// de línea al final).
    fn avisar(&mut self, texto: &str);
}

/// El terminal de verdad: la entrada, la salida y el error estándar.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConsolaTerminal;

impl Consola for ConsolaTerminal {
    fn es_interactiva(&self) -> bool {
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
    }

    fn leer_oculto(&mut self, pregunta: &str) -> Result<Zeroizing<String>, ErrorEntrada> {
        leer_oculto_del_terminal(pregunta)
    }

    fn leer_linea(&mut self, pregunta: &str) -> Result<String, ErrorEntrada> {
        // Lo pegado antes de ver el aviso no es la respuesta.
        vaciar_entrada_pendiente();
        let mut error = std::io::stderr();
        let _ = write!(error, "{pregunta}");
        let _ = error.flush();
        let mut linea = String::new();
        // Una confirmación cabe en poco: la lectura se acota.
        let leidos = std::io::stdin()
            .lock()
            .take(512)
            .read_line(&mut linea)
            .map_err(|causa| {
                ErrorEntrada::Terminal(format!("no se pudo leer la entrada: {causa}"))
            })?;
        if leidos == 0 {
            return Err(ErrorEntrada::Cancelada);
        }
        Ok(linea)
    }

    fn decir(&mut self, texto: &str) {
        // `println!` entra en pánico con una tubería rota; aquí se ignora.
        let mut salida = std::io::stdout().lock();
        let _ = writeln!(salida, "{texto}");
    }

    fn avisar(&mut self, texto: &str) {
        let mut error = std::io::stderr().lock();
        let _ = writeln!(error, "{texto}");
    }
}

/// Lo que hace una tecla o un pegado sobre la frase que se está escribiendo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paso {
    /// Sigue leyendo.
    Seguir,
    /// Intro: la frase está escrita.
    Aceptar,
    /// Ctrl+C o Ctrl+D.
    Cancelar,
}

/// Añade un carácter si cabe y no es de control (así un Tab o un salto pegado
/// no se cuelan en la frase).
fn insertar(buffer: &mut Zeroizing<String>, caracter: char) {
    if caracter.is_control() {
        return;
    }
    if buffer.len() + caracter.len_utf8() <= TOPE_LECTURA {
        buffer.push(caracter);
    }
}

/// Aplica un evento de crossterm a la frase en curso. Pura: es el corazón de
/// la lectura sin eco y lo que se prueba sin terminal.
///
/// - Un carácter se añade; Retroceso borra el último; Intro acepta.
/// - Ctrl+C y Ctrl+D cancelan; el resto de las combinaciones con Ctrl y las
///   demás teclas no hacen nada.
/// - Un pegado (con el pegado entre corchetes activado) añade su texto sin los
///   caracteres de control: un salto de línea al final no acepta la frase.
pub fn aplicar_evento(buffer: &mut Zeroizing<String>, evento: Event) -> Paso {
    match evento {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) => {
            if kind == KeyEventKind::Release {
                return Paso::Seguir;
            }
            match code {
                KeyCode::Enter => Paso::Aceptar,
                KeyCode::Backspace => {
                    buffer.pop();
                    Paso::Seguir
                }
                KeyCode::Char(caracter) if modifiers.contains(KeyModifiers::CONTROL) => {
                    if matches!(caracter, 'c' | 'C' | 'd' | 'D') {
                        Paso::Cancelar
                    } else {
                        Paso::Seguir
                    }
                }
                KeyCode::Char(caracter) => {
                    insertar(buffer, caracter);
                    Paso::Seguir
                }
                _ => Paso::Seguir,
            }
        }
        Event::Paste(mut texto) => {
            for caracter in texto.chars() {
                insertar(buffer, caracter);
            }
            // El texto pegado también es la frase: no se deja suelto.
            texto.zeroize();
            Paso::Seguir
        }
        _ => Paso::Seguir,
    }
}

/// Deja el terminal en modo raw y con el pegado entre corchetes mientras vive,
/// y lo devuelve a como estaba en su `Drop`: en todas las ramas, también con un
/// error o un pánico dentro de la lectura.
struct ModoRaw {
    pegado: bool,
}

impl ModoRaw {
    fn activar() -> Result<Self, ErrorEntrada> {
        crossterm::terminal::enable_raw_mode().map_err(|causa| {
            ErrorEntrada::Terminal(format!("no se pudo preparar el terminal: {causa}"))
        })?;
        // Se crea ya la guarda: si algo falla a partir de aquí, su `Drop`
        // devuelve el modo.
        let mut modo = Self { pegado: false };
        modo.pegado =
            crossterm::execute!(std::io::stderr(), crossterm::event::EnableBracketedPaste).is_ok();
        Ok(modo)
    }
}

impl Drop for ModoRaw {
    fn drop(&mut self) {
        if self.pegado {
            let _ = crossterm::execute!(std::io::stderr(), crossterm::event::DisableBracketedPaste);
        }
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Cada cuánto se comprueba que el terminal sigue ahí mientras se espera una
/// tecla (milisegundos).
const PLAZO_VIGILANCIA_MS: i32 = 500;

/// Cuántos eventos ya leídos por crossterm se descartan, como mucho, antes de
/// una pregunta (más de lo que cabe en su búfer de lectura).
const TOPE_VACIADO: usize = 4096;

/// Qué hay ahora mismo en la entrada estándar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entrada {
    /// Hay algo que leer.
    Datos,
    /// Nada por ahora.
    Nada,
    /// El terminal ha desaparecido (colgado, cerrado o ya no es un terminal).
    Perdida,
}

/// Espera hasta `plazo_ms` a que haya algo que leer en la entrada estándar o a
/// que el terminal desaparezca. Con 0, solo mira.
///
/// Es la defensa contra el giro de crossterm: su lectura, ante el fin de
/// fichero del terminal, nunca sale del bucle que lee el descriptor (ni
/// siquiera con el plazo de su `poll`), así que hay que saber que el terminal
/// se ha ido ANTES de llamarle.
fn mirar_entrada(plazo_ms: i32) -> Entrada {
    let mut descriptor = nix::libc::pollfd {
        fd: nix::libc::STDIN_FILENO,
        events: nix::libc::POLLIN,
        revents: 0,
    };
    // SAFETY: `poll` solo lee y escribe este `pollfd`, que vive en la pila y
    // es el único (`nfds` = 1); no toca más memoria.
    let listos = unsafe { nix::libc::poll(&mut descriptor, 1, plazo_ms) };
    if listos < 0 {
        // Una señal (el cambio de tamaño de la ventana) interrumpe la espera:
        // no es que se haya ido nada. Cualquier otro error, sí.
        return if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            Entrada::Nada
        } else {
            Entrada::Perdida
        };
    }
    if !std::io::stdin().is_terminal() {
        // Tras colgar el terminal, `isatty` da que no.
        return Entrada::Perdida;
    }
    if listos == 0 {
        return Entrada::Nada;
    }
    if descriptor.revents & (nix::libc::POLLHUP | nix::libc::POLLERR) != 0 {
        return Entrada::Perdida;
    }
    if descriptor.revents & (nix::libc::POLLIN | nix::libc::POLLNVAL) != 0 {
        // POLLNVAL (algunos sistemas no saben hacer `poll` de un terminal) no
        // prueba que se haya ido: `is_terminal` ya dijo que sigue.
        return Entrada::Datos;
    }
    Entrada::Nada
}

/// Descarta lo que se haya tecleado o pegado y todavía no se haya leído: la
/// cola del terminal y los eventos que crossterm ya tuviera leídos de más (un
/// pegado de varias líneas llega de golpe). Se llama justo antes de cada
/// pregunta, para que lo escrito por adelantado no conteste lo que la persona
/// aún no ha visto. Lo descartado no se puede borrar de la memoria de
/// crossterm (ver la cabecera del módulo).
fn vaciar_entrada_pendiente() {
    // SAFETY: `tcflush` solo recibe un descriptor y una constante; no toca
    // memoria. Si la entrada no es un terminal, falla y no hace nada.
    let _ = unsafe { nix::libc::tcflush(nix::libc::STDIN_FILENO, nix::libc::TCIFLUSH) };
    for _ in 0..TOPE_VACIADO {
        // Con el terminal perdido no se llama a crossterm (giraría).
        if mirar_entrada(0) == Entrada::Perdida {
            return;
        }
        match crossterm::event::poll(Duration::ZERO) {
            Ok(true) => {
                let _ = crossterm::event::read();
            }
            _ => return,
        }
    }
}

/// El error de cuando el terminal desaparece a media pregunta.
fn terminal_perdido() -> ErrorEntrada {
    ErrorEntrada::Terminal("el terminal se ha cerrado".to_string())
}

/// Lee una frase sin eco del terminal. El modo raw se activa ANTES de escribir
/// la pregunta: quien ve la pregunta ya puede escribir sin que se le haga eco.
fn leer_oculto_del_terminal(pregunta: &str) -> Result<Zeroizing<String>, ErrorEntrada> {
    let modo = ModoRaw::activar()?;
    // Lo tecleado antes de que aparezca la pregunta no la contesta.
    vaciar_entrada_pendiente();
    let mut error = std::io::stderr();
    let _ = write!(error, "{pregunta}");
    let _ = error.flush();
    // La capacidad se reserva entera de una vez: sin reasignaciones no queda
    // ninguna copia de la frase en memoria liberada sin borrar.
    let mut buffer = Zeroizing::new(String::with_capacity(TOPE_LECTURA));
    let resultado = loop {
        // Primero se comprueba que el terminal sigue ahí y solo entonces se
        // llama a crossterm: con el terminal colgado, su lectura no vuelve.
        if mirar_entrada(0) == Entrada::Perdida {
            break Err(terminal_perdido());
        }
        match crossterm::event::poll(Duration::ZERO) {
            // Con un evento listo, `read` no espera.
            Ok(true) => match crossterm::event::read() {
                Ok(evento) => match aplicar_evento(&mut buffer, evento) {
                    Paso::Seguir => {}
                    Paso::Aceptar => break Ok(()),
                    Paso::Cancelar => break Err(ErrorEntrada::Cancelada),
                },
                Err(causa) => {
                    break Err(ErrorEntrada::Terminal(format!(
                        "no se pudo leer del terminal: {causa}"
                    )))
                }
            },
            Ok(false) => {
                // Nada que atender: se espera a que haya teclas o a que el
                // terminal se vaya, sin pasar de `PLAZO_VIGILANCIA_MS`.
                if mirar_entrada(PLAZO_VIGILANCIA_MS) == Entrada::Perdida {
                    break Err(terminal_perdido());
                }
            }
            Err(causa) => {
                break Err(ErrorEntrada::Terminal(format!(
                    "no se pudo leer del terminal: {causa}"
                )))
            }
        }
    };
    // Primero se devuelve el terminal a su modo y luego se baja de línea (en
    // modo raw el salto no devuelve el carro).
    drop(modo);
    let _ = writeln!(error);
    resultado.map(|()| buffer)
}

// ------------------------------------------------------------------ textos

/// Fuera de Termux: el llavero es otro.
const NO_APLICA: &str = "magi llavero: en este equipo el llavero es el del sistema (secret-tool o Keychain) y no tiene frase maestra: esta orden solo existe en Termux";

/// Sin terminal no se puede escribir una frase sin eco ni confirmar nada.
fn sin_terminal(orden: &str, para_que: &str) -> String {
    format!(
        "magi llavero {orden}: hace falta un terminal para {para_que}. Ejecútala en una sesión de Termux, sin redirigir la entrada estándar ni la salida de error, y no desde un guion ni una tarea programada"
    )
}

/// Qué hacer cuando el fichero está dañado: la TUI es quien lo aparta (con una
/// copia con fecha y su entrada en REGISTRO).
fn texto_danado(motivo: &str) -> String {
    format!(
        "el llavero de MAGI está dañado ({motivo}): abre MAGI para apartarlo (queda como llavero.age.danado-<fecha>) y seguir sin llavero"
    )
}

/// Explica en castellano un fallo del llavero. Nunca usa el `Display` de la
/// crate `age`: los textos del núcleo ya son propios.
fn explicar(error: &ErrorLlavero) -> String {
    match error {
        ErrorLlavero::Bloqueado => "el llavero está bloqueado".to_string(),
        ErrorLlavero::Inexistente => {
            "no hay llavero: se crea al guardar el primer secreto desde MAGI".to_string()
        }
        ErrorLlavero::YaExiste => "ya hay un llavero".to_string(),
        ErrorLlavero::FraseIncorrecta { .. } => "frase incorrecta".to_string(),
        ErrorLlavero::FraseInvalida(motivo) | ErrorLlavero::Io(motivo) => motivo.clone(),
        ErrorLlavero::Espera { segundos } => format!(
            "demasiados intentos fallidos: espera {segundos} s antes de volver a intentarlo"
        ),
        ErrorLlavero::Danado(motivo) => texto_danado(motivo),
        ErrorLlavero::NoEstaDanado => "el llavero ya no está dañado".to_string(),
        ErrorLlavero::NoSeBorraDanado => {
            "el llavero está dañado: no se borra, se aparta; abre MAGI para apartarlo (queda como llavero.age.danado-<fecha>)".to_string()
        }
    }
}

/// Anota en REGISTRO un evento del llavero cuando su efecto ya se ha producido.
/// Abre el almacén como las demás órdenes de la CLI. Si no se puede, devuelve
/// el motivo para que la orden lo avise (el efecto ya ocurrió y no se deshace).
fn anotar(rutas: &Rutas, tipo: &str) -> Result<(), String> {
    let almacen =
        crate::almacen::abrir(&rutas.base_datos()).map_err(|causa| format!("{causa:#}"))?;
    let resultado = registro::anotar(
        almacen.conexion(),
        tipo,
        None,
        None,
        "",
        ResultadoRegistro::Ok,
    );
    let cierre = almacen.cerrar();
    resultado.map_err(|causa| format!("{causa:#}"))?;
    cierre.map_err(|causa| format!("{causa:#}"))
}

/// Dice por la consola que el registro no se pudo escribir (sin cambiar el
/// código de salida: el efecto de la orden ya se produjo).
fn avisar_registro_fallido(consola: &mut dyn Consola, motivo: &str) {
    consola.avisar(&format!(
        "aviso: no se pudo anotar en el registro de MAGI ({motivo})"
    ));
}

// ------------------------------------------------------------------ las órdenes

/// Ejecuta la orden y devuelve el código de salida. Fuera de Termux dice «en
/// este equipo el llavero es el del sistema» y sale con 1.
pub fn ejecutar(orden: OrdenLlavero, rutas: &Rutas) -> anyhow::Result<i32> {
    if !crate::termux::es_termux() {
        eprintln!("{NO_APLICA}");
        return Ok(1);
    }
    let llavero = crate::llavero::del_equipo(rutas);
    let Some(fichero) = llavero.de_fichero() else {
        // En Termux el llavero del equipo siempre es de fichero; si no lo
        // fuera, esta CLI no tiene nada que tocar.
        eprintln!("{NO_APLICA}");
        return Ok(1);
    };
    let mut consola = ConsolaTerminal;
    Ok(match orden {
        OrdenLlavero::Estado => orden_estado(fichero, &mut consola),
        OrdenLlavero::Cambiar => orden_cambiar(fichero, rutas, &mut consola),
        OrdenLlavero::Borrar => orden_borrar(fichero, rutas, &mut consola),
    })
}

/// La fecha de modificación en la hora local, legible y con su desfase
/// respecto a UTC (Termux no siempre sabe su zona horaria: así no confunde).
/// Los segundos fuera de rango salen como «desconocida».
fn fecha_local(segundos: i64) -> String {
    chrono::DateTime::from_timestamp(segundos, 0).map_or_else(
        || "desconocida".to_string(),
        |instante| {
            instante
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string()
        },
    )
}

/// `magi llavero estado`: dice si hay llavero y, si lo hay, su ruta, su tamaño
/// y su fecha. Nunca pide la frase, nunca abre el fichero con scrypt y nunca
/// enseña entradas. Código 0 si todo está bien; 1 si el fichero está dañado o
/// no se puede mirar.
pub fn orden_estado(fichero: &LlaveroFichero, consola: &mut dyn Consola) -> i32 {
    let ruta = fichero.ruta().display().to_string();
    let info = match info_fichero(fichero.ruta()) {
        Ok(Some(info)) => info,
        Ok(None) => {
            // Un enlace simbólico roto en la ruta no es «no hay llavero»: hay
            // algo ahí, que MAGI ofrece apartar (como con cualquier dañado).
            if let EstadoFichero::Danado(motivo) = fichero.estado() {
                consola.decir(&format!(
                    "Hay un fichero de llavero, pero está dañado ({motivo}).\n  Ruta:       {ruta}\nAbre MAGI para apartarlo (queda como llavero.age.danado-<fecha>) y seguir sin llavero."
                ));
                return 1;
            }
            consola.decir(&format!(
                "No hay llavero en este equipo ({ruta}).\nSe crea al guardar el primer secreto desde MAGI (recordar una contraseña o unir este equipo a la sincronización)."
            ));
            return 0;
        }
        Err(error) => {
            consola.avisar(&format!(
                "magi llavero estado: no se pudo mirar el llavero ({})",
                explicar(&error)
            ));
            return 1;
        }
    };
    let datos = format!(
        "  Ruta:       {}\n  Tamaño:     {} bytes\n  Modificado: {}",
        info.ruta.display(),
        info.bytes,
        fecha_local(info.modificado)
    );
    if let EstadoFichero::Danado(motivo) = fichero.estado() {
        consola.decir(&format!(
            "Hay un fichero de llavero, pero está dañado ({motivo}).\n{datos}\nAbre MAGI para apartarlo (queda como llavero.age.danado-<fecha>) y seguir sin llavero."
        ));
        return 1;
    }
    consola.decir(&format!(
        "Hay un llavero cifrado con frase maestra.\n{datos}\nMAGI pide la frase al abrirlo; esta orden no la pide ni enseña su contenido."
    ));
    0
}

/// `magi llavero cambiar`: pide la frase actual y la nueva dos veces (sin eco)
/// y recifra el llavero. Sin terminal, dice cómo hacerlo y sale con 1 sin
/// tocar nada. Anota `llavero_frase_cambiada` cuando el fichero nuevo ya está
/// escrito.
pub fn orden_cambiar(fichero: &LlaveroFichero, rutas: &Rutas, consola: &mut dyn Consola) -> i32 {
    if !consola.es_interactiva() {
        consola.avisar(&sin_terminal(
            "cambiar",
            "escribir las frases sin que se vean",
        ));
        return 1;
    }
    match fichero.estado() {
        EstadoFichero::Inexistente => {
            consola.avisar(&format!(
                "magi llavero cambiar: {}",
                explicar(&ErrorLlavero::Inexistente)
            ));
            return 1;
        }
        EstadoFichero::Danado(motivo) => {
            consola.avisar(&format!("magi llavero cambiar: {}", texto_danado(&motivo)));
            return 1;
        }
        EstadoFichero::Cerrado | EstadoFichero::Abierto => {}
    }
    let Some(actual) = leer_frase(consola, "Frase maestra actual: ") else {
        return 1;
    };
    let Some(nueva) = leer_frase(
        consola,
        &format!(
            "Frase maestra nueva (al menos {MIN_FRASE} caracteres; si la olvidas no se puede recuperar): "
        ),
    ) else {
        return 1;
    };
    let Some(repetida) = leer_frase(consola, "Repite la frase nueva: ") else {
        return 1;
    };
    // La frase nueva se valida antes de gastar scrypt con la actual.
    if let Err(error) = preparar_frase(&nueva, Some(&repetida)) {
        consola.avisar(&format!(
            "magi llavero cambiar: la frase nueva no vale: {}. El llavero no ha cambiado",
            explicar(&error)
        ));
        return 1;
    }
    consola.avisar("Recifrando el llavero… (unos segundos)");
    match fichero.cambiar_frase(&actual, &nueva, &repetida) {
        Ok(()) => {
            if let Err(motivo) = anotar(rutas, registro::LLAVERO_FRASE_CAMBIADA) {
                avisar_registro_fallido(consola, &motivo);
            }
            consola.decir("Frase maestra cambiada. MAGI usará la nueva desde ahora.");
            0
        }
        Err(ErrorLlavero::FraseIncorrecta { .. }) => {
            consola.avisar(
                "magi llavero cambiar: la frase actual es incorrecta. El llavero no ha cambiado",
            );
            1
        }
        Err(error) => {
            consola.avisar(&format!(
                "magi llavero cambiar: {}. El llavero no ha cambiado",
                explicar(&error)
            ));
            1
        }
    }
}

/// Lee una frase y, si se cancela o falla el terminal, lo dice y devuelve
/// `None` (la orden sale con 1).
fn leer_frase(consola: &mut dyn Consola, pregunta: &str) -> Option<Zeroizing<String>> {
    match consola.leer_oculto(pregunta) {
        Ok(frase) => Some(frase),
        Err(ErrorEntrada::Cancelada) => {
            consola.avisar("Cancelado: no se ha cambiado nada.");
            None
        }
        Err(ErrorEntrada::Terminal(motivo)) => {
            consola.avisar(&format!("magi llavero: {motivo}"));
            None
        }
    }
}

/// `magi llavero borrar`: explica qué se pierde y pide escribir `borrar`
/// (sin la frase: borrar no la necesita). Cualquier otra respuesta no hace
/// nada y sale con 1. Anota `llavero_borrado` cuando el fichero ya no existe.
///
/// Un llavero dañado no se borra nunca (informe §3.6): se aparta con fecha, y
/// eso lo hace la TUI. Con el fichero dañado, la orden se niega con 1 sin
/// preguntar, sin tocar nada y sin anotar, y remite a abrir MAGI; lo mismo si
/// se daña mientras se espera la confirmación (el núcleo lo mira bajo el
/// cerrojo).
pub fn orden_borrar(fichero: &LlaveroFichero, rutas: &Rutas, consola: &mut dyn Consola) -> i32 {
    if !consola.es_interactiva() {
        consola.avisar(&sin_terminal("borrar", "confirmar el borrado"));
        return 1;
    }
    match fichero.estado() {
        EstadoFichero::Inexistente => {
            consola.decir("No hay llavero: no hay nada que borrar.");
            return 0;
        }
        EstadoFichero::Danado(motivo) => {
            consola.avisar(&format!(
                "magi llavero borrar: {}. Un llavero dañado no se borra, se aparta; no se ha tocado nada",
                texto_danado(&motivo)
            ));
            return 1;
        }
        EstadoFichero::Cerrado | EstadoFichero::Abierto => {}
    }
    consola.avisar(&format!(
        "Se va a borrar el llavero de este equipo ({}).\nSe perderán las contraseñas de hosts guardadas en él y la clave de sincronización de este equipo. No se puede deshacer.\nDespués, los hosts con contraseña guardada volverán a pedirla y habrá que unir este equipo a la sincronización de nuevo.",
        fichero.ruta().display()
    ));
    let respuesta = match consola
        .leer_linea("Para confirmar, escribe borrar (en minúsculas) y pulsa Intro: ")
    {
        Ok(respuesta) => respuesta,
        Err(ErrorEntrada::Cancelada) => {
            consola.avisar("Cancelado: no se ha borrado nada.");
            return 1;
        }
        Err(ErrorEntrada::Terminal(motivo)) => {
            consola.avisar(&format!("magi llavero: {motivo}"));
            return 1;
        }
    };
    if respuesta.trim() != "borrar" {
        consola.avisar("No se ha escrito «borrar»: no se ha borrado nada.");
        return 1;
    }
    match fichero.borrar() {
        Ok(()) => {
            if let Err(motivo) = anotar(rutas, registro::LLAVERO_BORRADO) {
                avisar_registro_fallido(consola, &motivo);
            }
            consola.decir("Llavero borrado.");
            0
        }
        Err(ErrorLlavero::NoSeBorraDanado) => {
            // El fichero se dañó mientras se esperaba la confirmación.
            consola.avisar(
                "magi llavero borrar: el llavero se ha dañado mientras tanto y un llavero dañado no se borra, se aparta: abre MAGI para apartarlo (queda como llavero.age.danado-<fecha>). No se ha borrado nada",
            );
            1
        }
        Err(error) => {
            consola.avisar(&format!(
                "magi llavero borrar: no se pudo borrar el llavero ({})",
                explicar(&error)
            ));
            1
        }
    }
}

// ------------------------------------------------------------------ el llavero de las demás órdenes

/// ¿Se autentica con la contraseña del llavero alguno de estos hosts o alguno
/// de su cadena de saltos? Solo entonces un sondeo necesita el llavero abierto
/// (informe §4.2: «CLI que lo necesita»); si no, abrirlo pediría la frase, o
/// abortaría el sondeo entero sin terminal, sin que nadie lo vaya a usar.
///
/// `todos` es el inventario completo, para seguir los saltos. Una cadena
/// imposible (un salto que ya no existe, o más de tres niveles) falla antes de
/// autenticar nada: no lee el llavero, así que no cuenta.
pub fn algun_host_usa_llavero(hosts: &[Host], todos: &HashMap<i64, Host>) -> bool {
    hosts.iter().any(|host| {
        crate::conexion::salto::construir_cadena(host, todos)
            .map(|cadena| {
                cadena
                    .iter()
                    .any(|eslabon| eslabon.identidad_ref.usa_llavero())
            })
            .unwrap_or(false)
    })
}

/// El llavero de una orden de la CLI que lo necesita (`magi sincronizar`,
/// `magi sondear`…): el del equipo y, si es de fichero y está cerrado, pide la
/// frase en el TTY; sin TTY, error con instrucción. Nunca abre diálogos.
pub fn abrir_para_cli(rutas: &Rutas) -> anyhow::Result<Arc<dyn LlaveroCliente>> {
    let llavero = crate::llavero::del_equipo(rutas);
    desbloquear_para_cli(&*llavero, &mut ConsolaTerminal)?;
    Ok(llavero)
}

/// Deja abierto, si hace falta y se puede, el llavero de una orden de la CLI.
///
/// - Abierto, o el del sistema: no pregunta nada.
/// - Sin crear: lo deja tal cual (las lecturas dan `None`; lo que guarde algo
///   dirá que hay que crear la frase en MAGI).
/// - Dañado: error que remite a abrir MAGI para apartarlo.
/// - Cerrado: con terminal, pide la frase (hasta [`INTENTOS_FRASE`] intentos;
///   si el llavero impone una espera, la dice y sale); sin terminal, error con
///   instrucción. Una frase vacía o demasiado larga cuenta como un intento
///   más (se avisa y se vuelve a preguntar): no llega a gastar scrypt ni a la
///   serie de fallos del llavero.
pub fn desbloquear_para_cli(
    llavero: &dyn LlaveroCliente,
    consola: &mut dyn Consola,
) -> anyhow::Result<()> {
    match llavero.estado() {
        EstadoLlavero::Abierto | EstadoLlavero::SinCrear => return Ok(()),
        EstadoLlavero::Danado(motivo) => return Err(anyhow!(texto_danado(&motivo))),
        EstadoLlavero::Bloqueado => {}
    }
    let Some(fichero) = llavero.de_fichero() else {
        return Err(anyhow!(
            "el llavero está bloqueado y no se puede abrir desde la línea de órdenes"
        ));
    };
    if !consola.es_interactiva() {
        return Err(anyhow!(
            "el llavero de MAGI está bloqueado y esta orden necesita la frase maestra: ejecútala en un terminal (una sesión de Termux, sin redirigir la entrada estándar ni la salida de error) para poder escribirla"
        ));
    }
    for intento in 1..=INTENTOS_FRASE {
        let frase = match consola.leer_oculto("Frase maestra del llavero de MAGI: ") {
            Ok(frase) => frase,
            Err(ErrorEntrada::Cancelada) => {
                return Err(anyhow!("cancelado: no se ha abierto el llavero de MAGI"));
            }
            Err(ErrorEntrada::Terminal(motivo)) => return Err(anyhow!(motivo)),
        };
        consola.avisar("Abriendo el llavero… (unos segundos)");
        match fichero.desbloquear(&frase) {
            Ok(()) => return Ok(()),
            Err(ErrorLlavero::FraseIncorrecta { .. }) => {
                if intento < INTENTOS_FRASE {
                    consola.avisar(&format!(
                        "Frase incorrecta (intento {intento} de {INTENTOS_FRASE}). Prueba otra vez."
                    ));
                }
            }
            // Vacía o demasiado larga: ni se prueba (sin scrypt), pero es un
            // intento más y no tumba la orden.
            Err(ErrorLlavero::FraseInvalida(motivo)) => {
                if intento < INTENTOS_FRASE {
                    consola.avisar(&format!(
                        "Frase no válida: {motivo} (intento {intento} de {INTENTOS_FRASE}). Prueba otra vez."
                    ));
                }
            }
            Err(error) => return Err(anyhow!(explicar(&error))),
        }
    }
    Err(anyhow!(
        "frase incorrecta {INTENTOS_FRASE} veces: el llavero de MAGI sigue cerrado"
    ))
}
