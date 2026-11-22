//! Programa de prueba del terminal de la Fase 15: hace lo que hace Claude Code
//! (consultas, pegado entre corchetes, teclado de kitty, salida sincronizada,
//! ratón, OSC 52, campana y título) y deja por su salida estándar lo que ha
//! visto, para que las pruebas de extremo a extremo lo comprueben.
//!
//! Desde la Fase 16 puede pedir además `modifyOtherKeys` de xterm, como hace
//! tmux: con el argumento `--modify-other-keys` (nivel 2) o
//! `--modify-other-keys=1`, o con la variable de entorno
//! `MAGI_TUI_MODIFY_OTHER_KEYS` (`1` o `2`). Con ellos no empuja la bandera de
//! kitty (tmux no se la pide al terminal de fuera). Sin ellos no pide nada de
//! eso y se comporta como en la Fase 15.
//!
//! Se compila en la propia prueba con rustc y se ejecuta en el servidor SSH de
//! pruebas como programa de la sesión. Solo usa la biblioteca estándar: un hilo
//! lee la entrada estándar y el principal la consume con plazo, porque la
//! entrada puede ser un tubo (no un PTY) y una lectura bloqueante no volvería.
//! Por stdout salen líneas de la forma:
//!   RECIBIDO <hex>          bytes que llegaron del terminal, hasta el momento
//!   CHECK DA1=ok|no         respuesta a DA1
//!   CHECK DA2=ok|no         respuesta a DA2
//!   CHECK KITTY=ok|no       respuesta a la consulta del teclado de kitty
//!   CHECK DECRQM2026=ok|no  respuesta a DECRQM del modo 2026
//!   CHECK OSCC=ok|no        respuesta a OSC 11 con ?
//!   LISTO                   ya está configurado y esperando entrada
//!   PEGADO <lineas> <hex>   llegó un pegado entre corchetes
//!   MAYUS_INTRO             llegó CSI 13;2u (Mayús+Intro del teclado de kitty
//!                           o el que tmux reenvía con `extended-keys-format
//!                           csi-u`) o CSI 27;2;13~ (el de modifyOtherKeys de xterm)
//!   TECLA27 <m> <k>         llegó CSI 27;<m>;<k>~ (modifyOtherKeys): modificadores y código
//!   TECLAU <m> <k>          llegó CSI <k>;<m>u (teclado de kitty o csi-u de tmux)
//!   INTRO                   llegó un Intro suelto (`\r`, fuera de un pegado)
//!   ENTRADA <hex>           cada bloque de bytes que llegó tras `LISTO`, tal
//!                           cual (para ver qué reenvía tmux, si algo)
//!   RATON <hex>             llegó un evento de ratón SGR
//!   FIN                     fin de la entrada estándar

use std::io::{Read, Write};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

fn poner_modo_crudo() {
    // Con un PTY de verdad (un terminal, o el panel de una sesión de tmux):
    // sin eco, sin modo canónico, sin traducir el Intro (`\r`) a `\n` y con
    // lecturas que esperan al menos un byte, como hace una TUI. Una lectura
    // con plazo (`min 0`) devolvería 0 bytes al pasar el plazo y el lector la
    // tomaría por el fin de la entrada. Con un tubo, `stty` falla y no pasa
    // nada: el hilo lector lee igual.
    let _ = Command::new("stty")
        .args(["-icanon", "-echo", "-icrnl", "min", "1", "time", "0"])
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

/// Lanza el hilo que lee la entrada estándar y devuelve su receptor. El
/// receptor se cierra cuando la entrada termina.
fn lector_de_entrada() -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut entrada = std::io::stdin();
        let mut buffer = [0u8; 4096];
        loop {
            match entrada.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(leidos) => {
                    if tx.send(buffer[..leidos].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

/// Acumula lo que llegue hasta `silencio` sin datos nuevos.
fn leer_hasta_silencio(rx: &mpsc::Receiver<Vec<u8>>, silencio: Duration, acumulado: &mut Vec<u8>) {
    loop {
        match rx.recv_timeout(silencio) {
            Ok(nuevos) => acumulado.extend_from_slice(&nuevos),
            Err(_) => break,
        }
    }
}

fn check(nombre: &str, todo: &[u8], esperado: &[u8]) {
    let encontrado = todo
        .windows(esperado.len())
        .any(|ventana| ventana == esperado);
    informar(&format!(
        "CHECK {nombre}={}",
        if encontrado { "ok" } else { "no" }
    ));
}

/// Nivel de `modifyOtherKeys` que debe pedir: el del argumento
/// `--modify-other-keys[=N]` o el de `MAGI_TUI_MODIFY_OTHER_KEYS`. Sin ninguno
/// de los dos, `None` y no pide nada.
fn nivel_modify_other_keys() -> Option<u8> {
    for argumento in std::env::args().skip(1) {
        if argumento == "--modify-other-keys" {
            return Some(2);
        }
        if let Some(nivel) = argumento.strip_prefix("--modify-other-keys=") {
            return nivel
                .parse()
                .ok()
                .filter(|nivel| *nivel == 1 || *nivel == 2);
        }
    }
    std::env::var("MAGI_TUI_MODIFY_OTHER_KEYS")
        .ok()
        .and_then(|nivel| nivel.parse().ok())
        .filter(|nivel| *nivel == 1 || *nivel == 2)
}

fn main() {
    poner_modo_crudo();
    let rx = lector_de_entrada();
    // 1. Consultas: lo que Claude Code pregunta al arrancar.
    escribir(b"\x1b[c\x1b[>c\x1b[?u\x1b[?2026$p\x1b[?2004$p\x1b]11;?\x07");
    let mut respuestas = Vec::new();
    leer_hasta_silencio(&rx, Duration::from_millis(700), &mut respuestas);
    informar(&format!("RECIBIDO {}", hex(&respuestas)));
    check("DA1", &respuestas, b"\x1b[?62;22c");
    check("DA2", &respuestas, b"\x1b[>0;10601;0c");
    check("KITTY", &respuestas, b"\x1b[?0u");
    check("DECRQM2026", &respuestas, b"\x1b[?2026;2$y");
    check("OSCC", &respuestas, b"rgb:");

    // 2. Configuración: modos que usa una TUI moderna. La salida sincronizada
    //    se abre y se cierra para probar que no hay estados intermedios.
    //    Con `--modify-other-keys` pide modifyOtherKeys como tmux, en lugar de
    //    la bandera de kitty (tmux no se la pide al terminal de fuera, y con
    //    las dos puestas gana kitty).
    match nivel_modify_other_keys() {
        Some(nivel) => {
            escribir(format!("\x1b[?2004h\x1b[>4;{nivel}m\x1b[?1000h\x1b[?1006h").as_bytes())
        }
        None => escribir(b"\x1b[?2004h\x1b[>1u\x1b[?1000h\x1b[?1006h"),
    }
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
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(nuevos) => {
                informar(&format!("ENTRADA {}", hex(&nuevos)));
                todo.extend_from_slice(&nuevos);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // Un pegado entre corchetes, entero: el contenido se quita de la
        // entrada para que sus saltos de línea no cuenten como Intro.
        if let (Some(desde), Some(fin)) =
            (posicion(&todo, b"\x1b[200~"), posicion(&todo, b"\x1b[201~"))
        {
            if fin > desde {
                let pegado = trozo_pegado(&todo);
                let lineas = pegado.iter().filter(|b| **b == b'\r').count();
                informar(&format!("PEGADO {lineas} {}", hex(&pegado)));
                todo.drain(desde..fin + 6);
            }
        }
        // El teclado de kitty y el `csi-u` de tmux: CSI <k>;<m>u. Mayús+Intro
        // es 13;2.
        while let Some((modificadores, codigo, desde, hasta)) = tecla_u(&todo) {
            if modificadores == 2 && codigo == 13 {
                informar("MAYUS_INTRO");
            }
            informar(&format!("TECLAU {modificadores} {codigo}"));
            todo.drain(desde..hasta);
        }
        // modifyOtherKeys de xterm: CSI 27;<m>;<k>~. Mayús+Intro es 27;2;13.
        while let Some((modificadores, codigo, desde, hasta)) = tecla27(&todo) {
            if modificadores == 2 && codigo == 13 {
                informar("MAYUS_INTRO");
            }
            informar(&format!("TECLA27 {modificadores} {codigo}"));
            todo.drain(desde..hasta);
        }
        if contiene(&todo, b"\x1b[<") {
            let desde = posicion(&todo, b"\x1b[<").unwrap_or(0);
            let hasta = todo[desde..]
                .iter()
                .position(|b| *b == b'M' || *b == b'm')
                .map(|p| desde + p + 1)
                .unwrap_or(todo.len());
            informar(&format!("RATON {}", hex(&todo[desde..hasta])));
            todo = todo[hasta..].to_vec();
        }
        // Un Intro suelto: lo que queda de la entrada cuando ya se han
        // reconocido las secuencias. Es lo que llega si algo por el camino
        // (tmux, sin teclas extendidas) pierde el modificador de Mayús+Intro.
        // Con un pegado a medias (la marca de apertura sin la de cierre, porque
        // el resto llega en otra lectura) sus `\r` son del pegado y se quedan
        // donde están hasta que llegue entero.
        if posicion(&todo, b"\x1b[200~").is_none() {
            while let Some(posicion_intro) = posicion(&todo, b"\r") {
                informar("INTRO");
                todo.remove(posicion_intro);
            }
        }
    }
    informar("FIN");
}

/// Primera secuencia `CSI <k> ; <m> u` (o `CSI <k> u`, sin modificadores, que
/// vale 1) de `donde`: modificadores, código y el tramo que ocupa. No toma las
/// respuestas del teclado de kitty a la consulta (`CSI ? <banderas> u`).
fn tecla_u(donde: &[u8]) -> Option<(u32, u32, usize, usize)> {
    let mut desde = 0;
    while let Some(relativa) = posicion(&donde[desde..], b"\x1b[") {
        let inicio = desde + relativa;
        let cuerpo = &donde[inicio + 2..];
        let largo = cuerpo
            .iter()
            .position(|b| !(b.is_ascii_digit() || *b == b';'))
            .unwrap_or(cuerpo.len());
        if cuerpo.get(largo) == Some(&b'u') && largo > 0 {
            let texto = std::str::from_utf8(&cuerpo[..largo]).ok()?;
            let (codigo, modificadores) = match texto.split_once(';') {
                Some((codigo, modificadores)) => (codigo, modificadores),
                None => (texto, "1"),
            };
            if let (Ok(codigo), Ok(modificadores)) = (codigo.parse(), modificadores.parse()) {
                return Some((modificadores, codigo, inicio, inicio + 2 + largo + 1));
            }
        }
        desde = inicio + 2;
    }
    None
}

/// Primera secuencia `CSI 27 ; m ; k ~` de `donde`: modificadores, código y
/// el tramo que ocupa.
fn tecla27(donde: &[u8]) -> Option<(u32, u32, usize, usize)> {
    let desde = posicion(donde, b"\x1b[27;")?;
    let resto = &donde[desde + 5..];
    let fin = resto.iter().position(|b| *b == b'~')?;
    let texto = std::str::from_utf8(&resto[..fin]).ok()?;
    let (modificadores, codigo) = texto.split_once(';')?;
    Some((
        modificadores.parse().ok()?,
        codigo.parse().ok()?,
        desde,
        desde + 5 + fin + 1,
    ))
}

fn contiene(donde: &[u8], que: &[u8]) -> bool {
    posicion(donde, que).is_some()
}

fn posicion(donde: &[u8], que: &[u8]) -> Option<usize> {
    donde.windows(que.len()).position(|ventana| ventana == que)
}

/// Texto entre las marcas del pegado entre corchetes.
fn trozo_pegado(todo: &[u8]) -> Vec<u8> {
    let inicio = posicion(todo, b"\x1b[200~").map(|p| p + 6).unwrap_or(0);
    let fin = posicion(&todo[inicio..], b"\x1b[201~")
        .map(|p| inicio + p)
        .unwrap_or(todo.len());
    todo[inicio..fin].to_vec()
}
