//! Programa de prueba del terminal de la Fase 15: hace lo que hace Claude Code
//! (consultas, pegado entre corchetes, teclado de kitty, salida sincronizada,
//! ratón, OSC 52, campana y título) y deja por su salida estándar lo que ha
//! visto, para que las pruebas de extremo a extremo lo comprueben.
//!
//! Se compila en la propia prueba con rustc y se ejecuta en el servidor SSH de
//! pruebas como programa de la sesión. Solo usa la biblioteca estándar: deja el
//! terminal en modo no canónico y sin eco con stty (stdin es el PTY) y escribe
//! por stdout. Por stdout salen líneas de la forma:
//!   RECIBIDO <hex>          bytes que llegaron del terminal, hasta el momento
//!   CHECK DA1=ok|no         respuesta a DA1
//!   CHECK DA2=ok|no         respuesta a DA2
//!   CHECK KITTY=ok|no       respuesta a la consulta del teclado de kitty
//!   CHECK DECRQM2026=ok|no  respuesta a DECRQM del modo 2026
//!   CHECK OSCC=ok|no        respuesta a OSC 11 con ?
//!   LISTO                   ya está configurado y esperando entrada
//!   PEGADO <lineas> <hex>   llegó un pegado entre corchetes
//!   MAYUS_INTRO             llegó CSI 13;2u (Mayús+Intro del teclado de kitty)
//!   RATON <hex>             llegó un evento de ratón SGR
//!   FIN                     fin de la entrada estándar
//!
//! Comprobaciones de respaldo: el escenario escribe las secuencias que usa una
//! TUI moderna; si el terminal contesta, se ve en CHECK.

use std::io::{Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

fn poner_modo_crudo() {
    let _ = Command::new("stty")
        .args(["-icanon", "-echo", "min", "0", "time", "5"])
        .status();
}

fn escribir(bytes: &[u8]) {
    let mut salida = std::io::stdout();
    let _ = salida.write_all(bytes);
    let _ = salida.flush();
}

fn informar(texto: &str) {
    escribir(format!("{texto}\r\n").as_bytes());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Lee lo que haya disponible hasta que el terminal deje de mandar (stty con
/// min 0 y time 5 hace que read devuelva 0 al medio segundo sin datos).
fn leer_disponible(espera_maxima: Duration) -> Vec<u8> {
    let mut acumulado = Vec::new();
    let inicio = Instant::now();
    let mut buffer = [0u8; 1024];
    loop {
        match std::io::stdin().read(&mut buffer) {
            Ok(0) => {
                if inicio.elapsed() >= espera_maxima || !acumulado.is_empty() {
                    break;
                }
            }
            Ok(leidos) => acumulado.extend_from_slice(&buffer[..leidos]),
            Err(_) => break,
        }
        if inicio.elapsed() >= espera_maxima {
            break;
        }
    }
    acumulado
}

fn check(nombre: &str, todo: &[u8], esperado: &[u8]) {
    let encontrado = todo.windows(esperado.len()).any(|ventana| ventana == esperado);
    informar(&format!("CHECK {nombre}={}", if encontrado { "ok" } else { "no" }));
}

fn main() {
    poner_modo_crudo();
    // 1. Consultas: lo que Claude Code pregunta al arrancar.
    escribir(b"\x1b[c\x1b[>c\x1b[?u\x1b[?2026$p\x1b[?2004$p\x1b]11;?\x07");
    std::thread::sleep(Duration::from_millis(300));
    let respuestas = leer_disponible(Duration::from_millis(1500));
    informar(&format!("RECIBIDO {}", hex(&respuestas)));
    check("DA1", &respuestas, b"\x1b[?62;22c");
    check("DA2", &respuestas, b"\x1b[>0;10400;0c");
    check("KITTY", &respuestas, b"\x1b[?0u");
    check("DECRQM2026", &respuestas, b"\x1b[?2026;2$y");
    check("OSCC", &respuestas, b"rgb:");

    // 2. Configuración: modos que usa una TUI moderna. La salida sincronizada
    //    se abre y se cierra para probar que no hay estados intermedios.
    escribir(b"\x1b[?2004h\x1b[>1u\x1b[?1000h\x1b[?1006h");
    escribir(b"\x1b]2;Claude Code\x07");
    escribir(b"\x07");
    escribir(b"\x1b[?2026h");
    for fila in 0..40 {
        escribir(format!("\x1b[{};1Hlinea {fila:02}", fila + 1).as_bytes());
    }
    escribir(b"\x1b[?2026l");
    informar("LISTO");

    // 3. Entrada: el pegado, Mayús+Intro y el ratón que manda el terminal.
    let mut todo = Vec::new();
    loop {
        let antes = todo.len();
        let nuevos = leer_disponible(Duration::from_millis(4000));
        if nuevos.is_empty() {
            if antes == 0 {
                continue;
            }
            break;
        }
        todo.extend_from_slice(&nuevos);
        if contiene(&todo, b"\x1b[200~") {
            let pegado = trozo_pegado(&todo);
            let lineas = pegado.iter().filter(|b| **b == b'\r').count();
            informar(&format!("PEGADO {lineas} {}", hex(&pegado)));
            quitar(&mut todo, b"\x1b[200~");
            quitar(&mut todo, b"\x1b[201~");
        }
        if contiene(&todo, b"\x1b[13;2u") {
            informar("MAYUS_INTRO");
            quitar(&mut todo, b"\x1b[13;2u");
        }
        if contiene(&todo, b"\x1b[<") {
            let desde = posicion(&todo, b"\x1b[<").unwrap_or(0);
            let hasta = todo[desde..]
                .iter()
                .position(|b| *b == b'M' || *b == b'm')
                .map(|p| desde + p + 1)
                .unwrap_or(todo.len());
            informar(&format!("RATON {}", hex(&todo[desde..hasta])));
            let sobrante = todo[hasta..].to_vec();
            todo = sobrante;
        }
    }
    informar("FIN");
}

fn contiene(donde: &[u8], que: &[u8]) -> bool {
    posicion(donde, que).is_some()
}

fn posicion(donde: &[u8], que: &[u8]) -> Option<usize> {
    donde.windows(que.len()).position(|ventana| ventana == que)
}

fn quitar(donde: &mut Vec<u8>, que: &[u8]) {
    while let Some(posicion) = posicion(donde, que) {
        donde.drain(posicion..posicion + que.len());
    }
}

/// Texto entre las marcas del pegado entre corchetes.
fn trozo_pegado(todo: &[u8]) -> Vec<u8> {
    let inicio = posicion(todo, b"\x1b[200~").map(|p| p + 6).unwrap_or(0);
    let fin = posicion(&todo[inicio..], b"\x1b[201~")
        .map(|p| inicio + p)
        .unwrap_or(todo.len());
    todo[inicio..fin].to_vec()
}
