use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Convierte el texto de `config.toml` («Ctrl+]») en una tecla.
pub fn parsear_prefijo(texto: &str) -> Result<(KeyCode, KeyModifiers), String> {
    let texto = texto.trim();
    let mut modos = KeyModifiers::NONE;
    let mut resto = texto;
    while let Some((parte, cola)) = resto.split_once('+') {
        match parte.trim().to_lowercase().as_str() {
            "ctrl" | "control" => modos |= KeyModifiers::CONTROL,
            "alt" => modos |= KeyModifiers::ALT,
            "shift" => modos |= KeyModifiers::SHIFT,
            otra => return Err(format!("modificador desconocido en el prefijo: «{otra}»")),
        }
        resto = cola;
    }
    let resto = resto.trim();
    let tecla = match resto.to_lowercase().as_str() {
        "esc" | "escape" => KeyCode::Esc,
        "enter" | "intro" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "espacio" | "space" => KeyCode::Char(' '),
        "flecha_arriba" | "up" => KeyCode::Up,
        "flecha_abajo" | "down" => KeyCode::Down,
        "flecha_izquierda" | "left" => KeyCode::Left,
        "flecha_derecha" | "right" => KeyCode::Right,
        "fin" | "end" => KeyCode::End,
        "inicio" | "home" => KeyCode::Home,
        "borrar" | "delete" => KeyCode::Delete,
        "retroceso" | "backspace" => KeyCode::Backspace,
        "pagina_arriba" | "pageup" => KeyCode::PageUp,
        "pagina_abajo" | "pagedown" => KeyCode::PageDown,
        otro => {
            let mut caracteres = otro.chars();
            match (caracteres.next(), caracteres.next()) {
                (Some(caracter), None) => KeyCode::Char(caracter),
                _ => return Err(format!("prefijo no reconocido: «{texto}»")),
            }
        }
    };
    if modos.is_empty() {
        return Err("el prefijo debe incluir un modificador (p. ej. «Ctrl+]»)".to_string());
    }
    Ok((tecla, modos))
}

/// Equivalencia de las teclas de control que crossterm entrega como dígitos.
/// Los bytes `0x1C`–`0x1F` los mapea a `Ctrl+4`…`Ctrl+7`, de modo que
/// `Ctrl+]` llega como `Ctrl+5`; se normaliza a la tecla «real».
fn normalizar_control(codigo: KeyCode) -> KeyCode {
    match codigo {
        KeyCode::Char('4') => KeyCode::Char('['),
        KeyCode::Char('5') => KeyCode::Char(']'),
        KeyCode::Char('6') => KeyCode::Char('^'),
        KeyCode::Char('7') => KeyCode::Char('_'),
        otro => otro,
    }
}

/// Comprueba si una pulsación es exactamente el prefijo configurado.
pub fn es_prefijo(tecla: &KeyEvent, prefijo: (KeyCode, KeyModifiers)) -> bool {
    let control = tecla.modifiers.contains(KeyModifiers::CONTROL);
    let codigo = if control {
        normalizar_control(tecla.code)
    } else {
        tecla.code
    };
    let (codigo_prefijo, modos_prefijo) = prefijo;
    let codigo_prefijo = if modos_prefijo.contains(KeyModifiers::CONTROL) {
        normalizar_control(codigo_prefijo)
    } else {
        codigo_prefijo
    };
    codigo == codigo_prefijo && tecla.modifiers.contains(modos_prefijo)
}

/// Traduce una tecla de crossterm a los bytes que espera un PTY remoto.
pub fn bytes_de_tecla(tecla: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = tecla.modifiers.contains(KeyModifiers::CONTROL);
    let alt = tecla.modifiers.contains(KeyModifiers::ALT);
    let mut bytes = match tecla.code {
        KeyCode::Char(caracter) => {
            if ctrl {
                let byte = match caracter {
                    'a'..='z' => caracter as u8 - b'a' + 1,
                    'A'..='Z' => caracter as u8 - b'A' + 1,
                    ' ' | '@' => 0,
                    '4' => 0x1c,
                    '5' => 0x1d,
                    '6' => 0x1e,
                    '7' => 0x1f,
                    '[' => 27,
                    '\\' => 28,
                    ']' => 29,
                    '^' => 30,
                    '_' => 31,
                    _ => return None,
                };
                vec![byte]
            } else {
                let mut buffer = [0u8; 4];
                let texto = caracter.encode_utf8(&mut buffer);
                texto.as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => vec![b'\x1b', b'[', b'Z'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        KeyCode::F(1) => vec![0x1b, b'O', b'P'],
        KeyCode::F(2) => vec![0x1b, b'O', b'Q'],
        KeyCode::F(3) => vec![0x1b, b'O', b'R'],
        KeyCode::F(4) => vec![0x1b, b'O', b'S'],
        KeyCode::F(5) => vec![0x1b, b'[', b'1', b'5', b'~'],
        KeyCode::F(6) => vec![0x1b, b'[', b'1', b'7', b'~'],
        KeyCode::F(7) => vec![0x1b, b'[', b'1', b'8', b'~'],
        KeyCode::F(8) => vec![0x1b, b'[', b'1', b'9', b'~'],
        KeyCode::F(9) => vec![0x1b, b'[', b'2', b'0', b'~'],
        KeyCode::F(10) => vec![0x1b, b'[', b'2', b'1', b'~'],
        KeyCode::F(11) => vec![0x1b, b'[', b'2', b'3', b'~'],
        KeyCode::F(12) => vec![0x1b, b'[', b'2', b'4', b'~'],
        _ => return None,
    };
    if alt && !ctrl {
        let mut con_alt = vec![0x1b];
        con_alt.append(&mut bytes);
        return Some(con_alt);
    }
    Some(bytes)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_prefijo_ctrl_corchete_reconoce_ambas_formas() {
        let prefijo = parsear_prefijo("Ctrl+]").unwrap();
        assert_eq!(prefijo, (KeyCode::Char(']'), KeyModifiers::CONTROL));
        let corchete = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL);
        let digito = KeyEvent::new(KeyCode::Char('5'), KeyModifiers::CONTROL);
        assert!(es_prefijo(&corchete, prefijo));
        assert!(es_prefijo(&digito, prefijo));
        assert!(!es_prefijo(
            &KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            prefijo
        ));
        assert!(!es_prefijo(
            &KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE),
            prefijo
        ));
    }

    #[test]
    fn ctrl_corchete_se_traduce_al_byte_de_control() {
        let digito = KeyEvent::new(KeyCode::Char('5'), KeyModifiers::CONTROL);
        assert_eq!(bytes_de_tecla(digito), Some(vec![0x1d]));
        let corchete = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL);
        assert_eq!(bytes_de_tecla(corchete), Some(vec![0x1d]));
    }

    #[test]
    fn parsear_prefijo_valida_el_texto() {
        assert_eq!(
            parsear_prefijo("Ctrl+b").unwrap(),
            (KeyCode::Char('b'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            parsear_prefijo("Alt+x").unwrap(),
            (KeyCode::Char('x'), KeyModifiers::ALT)
        );
        assert!(parsear_prefijo("Ctrl+Alt+7").is_ok());
        assert!(parsear_prefijo("b").is_err());
        assert!(parsear_prefijo("Meta+b").is_err());
    }
}
