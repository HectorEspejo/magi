use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use magi_nucleo::terminal::teclas::{codificar, Modificadores, ModoTerminal, Tecla};

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
///
/// La codificación es la del núcleo, la misma que usa la app móvil. El
/// escritorio codifica en modo normal porque el parser de la sesión vive en el
/// servidor; Ctrl, Alt y Mayús se traducen a la tabla de §7.6.
pub fn bytes_de_tecla(tecla: KeyEvent) -> Option<Vec<u8>> {
    let tecla_nucleo = match tecla.code {
        KeyCode::Char(caracter) => Tecla::Caracter(caracter),
        KeyCode::Enter => Tecla::Intro,
        KeyCode::Tab => Tecla::Tabulador,
        KeyCode::BackTab => Tecla::TabuladorInverso,
        KeyCode::Backspace => Tecla::Retroceso,
        KeyCode::Esc => Tecla::Escape,
        KeyCode::Up => Tecla::Arriba,
        KeyCode::Down => Tecla::Abajo,
        KeyCode::Right => Tecla::Derecha,
        KeyCode::Left => Tecla::Izquierda,
        KeyCode::Home => Tecla::Inicio,
        KeyCode::End => Tecla::Fin,
        KeyCode::PageUp => Tecla::RePagina,
        KeyCode::PageDown => Tecla::AvPagina,
        KeyCode::Insert => Tecla::Insertar,
        KeyCode::Delete => Tecla::Suprimir,
        KeyCode::F(numero) => Tecla::F(numero),
        _ => return None,
    };
    let modificadores = Modificadores {
        mayusculas: tecla.modifiers.contains(KeyModifiers::SHIFT),
        alt: tecla.modifiers.contains(KeyModifiers::ALT),
        ctrl: tecla.modifiers.contains(KeyModifiers::CONTROL),
    };
    codificar(tecla_nucleo, modificadores, ModoTerminal::default())
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
