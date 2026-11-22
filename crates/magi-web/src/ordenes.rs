//! Órdenes externas (`gh`, `curl`, `git`, `mdbook`) con plazo y tope de salida.
//!
//! Sin shell: el programa y sus argumentos van separados. Toda orden lleva plazo
//! (`CLAUDE.md`: «toda orden externa lleva plazo») y la salida tiene un tope en
//! bytes: lo que devuelve GitHub es entrada no fiable. Los secretos (`GH_TOKEN`)
//! viajan por el entorno, nunca en los argumentos ni en los mensajes de error.

use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{ErrorWeb, Resultado};

/// Resultado de una orden que terminó dentro de su plazo.
#[derive(Debug, Clone)]
pub struct SalidaOrden {
    /// Código de salida (`None` si la mató una señal).
    pub codigo: Option<i32>,
    pub salida: Vec<u8>,
    pub error: Vec<u8>,
}

impl SalidaOrden {
    pub fn exito(&self) -> bool {
        self.codigo == Some(0)
    }
}

/// Plazo por omisión de las órdenes de red.
pub const PLAZO_RED: Duration = Duration::from_secs(60);
/// Tope por omisión de la salida de una orden (10 MiB).
pub const TOPE_SALIDA: usize = 10 * 1024 * 1024;

/// Cada cuánto se mira si la orden terminó, pasó su plazo o pasó su tope.
const SONDEO: Duration = Duration::from_millis(10);
/// Lo mínimo que se espera a que los lectores entreguen lo que queda en las
/// tuberías cuando la orden ya terminó.
const GRACIA_LECTORES: Duration = Duration::from_millis(500);

/// Ejecuta `programa` con `args` y el entorno adicional `entorno`.
///
/// - El entorno de `entorno` se **añade** al heredado.
/// - Pasado `plazo`, mata el proceso y devuelve `ErrorWeb::Orden`.
/// - Si `salida` o `error` pasan de `tope_salida` bytes, mata el proceso y falla.
/// - El mensaje de error nunca incluye valores del entorno ni argumentos: solo
///   el nombre del programa.
///
/// Solo se mata el proceso lanzado, no sus descendientes: las órdenes que usa la
/// web (`gh`, `curl`, `git`, `mdbook`) no dejan procesos huérfanos con las
/// tuberías abiertas.
pub fn ejecutar_con_plazo(
    programa: &str,
    args: &[&str],
    entorno: &[(&str, &str)],
    plazo: Duration,
    tope_salida: usize,
) -> Resultado<SalidaOrden> {
    let fallo = |motivo: String| ErrorWeb::Orden {
        orden: programa.to_string(),
        motivo,
    };

    let mut orden = Command::new(programa);
    orden
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (clave, valor) in entorno {
        orden.env(clave, valor);
    }

    let mut hijo = orden
        .spawn()
        .map_err(|e| fallo(format!("no se pudo lanzar la orden: {e}")))?;

    let excedido = Arc::new(AtomicBool::new(false));
    let (Some(stdout), Some(stderr)) = (hijo.stdout.take(), hijo.stderr.take()) else {
        matar(&mut hijo);
        return Err(fallo("no se pudieron abrir sus tuberías".to_string()));
    };
    let lector_salida = leer_con_tope(stdout, tope_salida, Arc::clone(&excedido));
    let lector_error = leer_con_tope(stderr, tope_salida, Arc::clone(&excedido));

    let limite = Instant::now() + plazo;
    let estado = loop {
        if excedido.load(Ordering::SeqCst) {
            matar(&mut hijo);
            return Err(fallo(format!(
                "su salida pasa del tope de {tope_salida} bytes"
            )));
        }
        match hijo.try_wait() {
            Ok(Some(estado)) => break estado,
            Ok(None) => {}
            Err(e) => {
                matar(&mut hijo);
                return Err(fallo(format!("no se pudo esperar a la orden: {e}")));
            }
        }
        if Instant::now() >= limite {
            matar(&mut hijo);
            return Err(fallo(format!(
                "pasó el plazo de {:.1} s y se mató",
                plazo.as_secs_f64()
            )));
        }
        thread::sleep(SONDEO);
    };

    // La orden terminó: se recoge lo que queda en las tuberías, dentro del plazo
    // (un descendiente que las mantenga abiertas no puede colgar la construcción).
    // Las dos tuberías comparten un único límite: esperar a cada una por separado
    // duplicaría el plazo.
    let limite_lectores = Instant::now()
        + limite
            .saturating_duration_since(Instant::now())
            .max(GRACIA_LECTORES);
    let salida = recoger(&lector_salida, limite_lectores);
    let error = recoger(&lector_error, limite_lectores);
    let (Some(salida), Some(error)) = (salida, error) else {
        return Err(fallo(
            "la orden terminó, pero dejó una tubería abierta pasado el plazo".to_string(),
        ));
    };
    if excedido.load(Ordering::SeqCst) {
        return Err(fallo(format!(
            "su salida pasa del tope de {tope_salida} bytes"
        )));
    }

    Ok(SalidaOrden {
        codigo: estado.code(),
        salida,
        error,
    })
}

/// Mata el proceso y lo recoge para que no quede zombi. Ignora los errores: ya
/// puede haber terminado.
fn matar(hijo: &mut Child) {
    let _ = hijo.kill();
    let _ = hijo.wait();
}

/// Lee `flujo` en un hilo hasta el final o hasta `tope` bytes. Si pasa del tope,
/// marca `excedido` y deja de leer (el bucle principal mata la orden).
fn leer_con_tope<R: Read + Send + 'static>(
    mut flujo: R,
    tope: usize,
    excedido: Arc<AtomicBool>,
) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut datos: Vec<u8> = Vec::new();
        let mut bloque = [0u8; 8192];
        loop {
            match flujo.read(&mut bloque) {
                Ok(0) => break,
                Ok(n) => {
                    // `datos.len() <= tope` siempre: nunca se guarda más.
                    let sitio = tope - datos.len();
                    if n > sitio {
                        datos.extend_from_slice(&bloque[..sitio]);
                        excedido.store(true, Ordering::SeqCst);
                        break;
                    }
                    datos.extend_from_slice(&bloque[..n]);
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let _ = tx.send(datos);
    });
    rx
}

/// Lo que entregó el lector antes de `limite` (`None` si no llegó a tiempo).
fn recoger(lector: &Receiver<Vec<u8>>, limite: Instant) -> Option<Vec<u8>> {
    lector
        .recv_timeout(limite.saturating_duration_since(Instant::now()))
        .ok()
}

/// Caracteres que no se ven pero cambian cómo se lee una línea: marcas de
/// dirección y aislados bidireccionales (el texto se vería al revés), separadores
/// de línea y de párrafo, espacios de ancho cero, marcas de formato (`U+2060` a
/// `U+206F`) y la marca de orden de bytes.
fn es_invisible_que_engana(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

/// Un texto que viene de fuera, listo para ir en un mensaje de error: sin
/// caracteres de control (que podrían manipular la terminal), sin los invisibles
/// que falsean cómo se lee (dirección del texto, separadores de línea) y acotado a
/// `max` caracteres.
pub(crate) fn texto_seguro(texto: &str, max: usize) -> String {
    let mut salida = String::new();
    for (cuenta, c) in texto.chars().enumerate() {
        if cuenta == max {
            salida.push('…');
            break;
        }
        salida.push(if c.is_control() || es_invisible_que_engana(c) {
            ' '
        } else {
            c
        });
    }
    salida
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn texto_seguro_quita_controles_y_acota() {
        assert_eq!(texto_seguro("a\u{1b}[31mb\nc", 100), "a [31mb c");
        assert_eq!(texto_seguro("abcdef", 3), "abc…");
        assert_eq!(texto_seguro("abc", 3), "abc");
        assert_eq!(texto_seguro("", 3), "");
        // Los caracteres multibyte cuentan como uno y no se parten.
        assert_eq!(texto_seguro("ñññññ", 2), "ññ…");
    }

    #[test]
    fn texto_seguro_quita_los_invisibles_que_falsean_la_lectura() {
        // Dirección del texto (U+202E la invierte), aislados, separadores de línea y
        // de párrafo, espacios de ancho cero y marca de orden de bytes.
        for raro in [
            '\u{202E}', '\u{202D}', '\u{202A}', '\u{2066}', '\u{2069}', '\u{200B}', '\u{200F}',
            '\u{2028}', '\u{2029}', '\u{2060}', '\u{FEFF}',
        ] {
            let texto = format!("a{raro}b");
            assert_eq!(texto_seguro(&texto, 100), "a b", "U+{:04X}", raro as u32);
        }
        // Los acentos y los símbolos de siempre se quedan.
        assert_eq!(texto_seguro("«≠» ñ é — €", 100), "«≠» ñ é — €");
    }

    #[test]
    fn el_lector_nunca_guarda_mas_del_tope() {
        let excedido = Arc::new(AtomicBool::new(false));
        let datos = vec![7u8; 20_000];
        let rx = leer_con_tope(std::io::Cursor::new(datos), 100, Arc::clone(&excedido));
        let leido = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("el lector responde");
        assert_eq!(leido.len(), 100);
        assert!(excedido.load(Ordering::SeqCst));
    }

    #[test]
    fn el_lector_admite_exactamente_el_tope() {
        let excedido = Arc::new(AtomicBool::new(false));
        let datos = vec![7u8; 9000];
        let rx = leer_con_tope(std::io::Cursor::new(datos), 9000, Arc::clone(&excedido));
        let leido = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("el lector responde");
        assert_eq!(leido.len(), 9000);
        assert!(!excedido.load(Ordering::SeqCst));
    }
}
