//! Tabla de teclas de la Fase 12 (D222): código físico del navegador
//! (KeyboardEvent.code) a scancode del conjunto 1 de PC (con si es extendida) y
//! a keysym de X11. Es la única tabla: la usan el cliente RDP (scancodes y
//! respaldo Unicode) y el cliente RFB (keysyms y extensión de QEMU).
//!
//! El visor manda siempre el keysym y el código físico; aquí no se adivina la
//! distribución del remoto.

/// Scancode del conjunto 1 y si es una tecla extendida (prefijo 0xE0).
pub type Scancode = (u8, bool);

/// Código físico del navegador a scancode del conjunto 1 de PC.
///
/// La tabla cubre el bloque principal, los modificadores, las teclas de
/// función, el teclado numérico, las flechas y los modificadores del sistema.
pub fn scancode(codigo: &str) -> Option<Scancode> {
    let valor = match codigo {
        // Fila de números y símbolos.
        "Digit1" => (0x02, false),
        "Digit2" => (0x03, false),
        "Digit3" => (0x04, false),
        "Digit4" => (0x05, false),
        "Digit5" => (0x06, false),
        "Digit6" => (0x07, false),
        "Digit7" => (0x08, false),
        "Digit8" => (0x09, false),
        "Digit9" => (0x0A, false),
        "Digit0" => (0x0B, false),
        "Minus" => (0x0C, false),
        "Equal" => (0x0D, false),
        "Backspace" => (0x0E, false),
        "Tab" => (0x0F, false),
        // Fila superior.
        "KeyQ" => (0x10, false),
        "KeyW" => (0x11, false),
        "KeyE" => (0x12, false),
        "KeyR" => (0x13, false),
        "KeyT" => (0x14, false),
        "KeyY" => (0x15, false),
        "KeyU" => (0x16, false),
        "KeyI" => (0x17, false),
        "KeyO" => (0x18, false),
        "KeyP" => (0x19, false),
        "BracketLeft" => (0x1A, false),
        "BracketRight" => (0x1B, false),
        "Enter" => (0x1C, false),
        "ControlLeft" => (0x1D, false),
        // Fila central.
        "KeyA" => (0x1E, false),
        "KeyS" => (0x1F, false),
        "KeyD" => (0x20, false),
        "KeyF" => (0x21, false),
        "KeyG" => (0x22, false),
        "KeyH" => (0x23, false),
        "KeyJ" => (0x24, false),
        "KeyK" => (0x25, false),
        "KeyL" => (0x26, false),
        "Semicolon" => (0x27, false),
        "Quote" => (0x28, false),
        "Backquote" => (0x29, false),
        "ShiftLeft" => (0x2A, false),
        "Backslash" => (0x2B, false),
        // Fila inferior.
        "KeyZ" => (0x2C, false),
        "KeyX" => (0x2D, false),
        "KeyC" => (0x2E, false),
        "KeyV" => (0x2F, false),
        "KeyB" => (0x30, false),
        "KeyN" => (0x31, false),
        "KeyM" => (0x32, false),
        "Comma" => (0x33, false),
        "Period" => (0x34, false),
        "Slash" => (0x35, false),
        "ShiftRight" => (0x36, false),
        "NumpadMultiply" => (0x37, false),
        "AltLeft" => (0x38, false),
        "Space" => (0x39, false),
        "CapsLock" => (0x3A, false),
        // Teclas de función y bloque de control.
        "F1" => (0x3B, false),
        "F2" => (0x3C, false),
        "F3" => (0x3D, false),
        "F4" => (0x3E, false),
        "F5" => (0x3F, false),
        "F6" => (0x40, false),
        "F7" => (0x41, false),
        "F8" => (0x42, false),
        "F9" => (0x43, false),
        "F10" => (0x44, false),
        "Pause" => (0x45, false),
        "ScrollLock" => (0x46, false),
        "Numpad7" => (0x47, false),
        "Numpad8" => (0x48, false),
        "Numpad9" => (0x49, false),
        "NumpadSubtract" => (0x4A, false),
        "Numpad4" => (0x4B, false),
        "Numpad5" => (0x4C, false),
        "Numpad6" => (0x4D, false),
        "NumpadAdd" => (0x4E, false),
        "Numpad1" => (0x4F, false),
        "Numpad2" => (0x50, false),
        "Numpad3" => (0x51, false),
        "Numpad0" => (0x52, false),
        "NumpadDecimal" => (0x53, false),
        "IntlBackslash" => (0x56, false),
        "F11" => (0x57, false),
        "F12" => (0x58, false),
        "NumpadEqual" => (0x59, false),
        "F13" => (0x64, false),
        "F14" => (0x65, false),
        "F15" => (0x66, false),
        "F16" => (0x67, false),
        "F17" => (0x68, false),
        "F18" => (0x69, false),
        "F19" => (0x6A, false),
        "F20" => (0x6B, false),
        "F21" => (0x6C, false),
        "F22" => (0x6D, false),
        "F23" => (0x6E, false),
        // Teclas extendidas (como si fueran del bloque de edición).
        "PrintScreen" => (0x37, true),
        "Insert" => (0x52, true),
        "Delete" => (0x53, true),
        "Home" => (0x47, true),
        "End" => (0x4F, true),
        "PageUp" => (0x49, true),
        "PageDown" => (0x51, true),
        "ArrowUp" => (0x48, true),
        "ArrowDown" => (0x50, true),
        "ArrowLeft" => (0x4B, true),
        "ArrowRight" => (0x4D, true),
        "NumLock" => (0x45, true),
        "NumpadDivide" => (0x35, true),
        "NumpadEnter" => (0x1C, true),
        "ControlRight" => (0x1D, true),
        "AltRight" => (0x38, true),
        "MetaLeft" => (0x5B, true),
        "MetaRight" => (0x5C, true),
        "ContextMenu" => (0x5D, true),
        _ => return None,
    };
    Some(valor)
}

/// Código físico del navegador a keysym de X11 (teclas que no son caracteres).
/// Para los caracteres, el visor manda el keysym Unicode; ver
/// keysym_de_caracter.
pub fn keysym_de_codigo(codigo: &str) -> Option<u32> {
    let valor = match codigo {
        "Backspace" => 0xFF08,
        "Tab" => 0xFF09,
        "Enter" | "NumpadEnter" => 0xFF0D,
        "Escape" => 0xFF1B,
        "Delete" => 0xFFFF,
        "Insert" => 0xFF63,
        "Home" => 0xFF50,
        "End" => 0xFF57,
        "PageUp" => 0xFF55,
        "PageDown" => 0xFF56,
        "ArrowLeft" => 0xFF51,
        "ArrowUp" => 0xFF52,
        "ArrowRight" => 0xFF53,
        "ArrowDown" => 0xFF54,
        "ShiftLeft" => 0xFFE1,
        "ShiftRight" => 0xFFE2,
        "ControlLeft" => 0xFFE3,
        "ControlRight" => 0xFFE4,
        "AltLeft" => 0xFFE9,
        "AltRight" => 0xFFEA,
        "MetaLeft" => 0xFFEB,
        "MetaRight" => 0xFFEC,
        "CapsLock" => 0xFFE5,
        "NumLock" => 0xFF7F,
        "ScrollLock" => 0xFF14,
        "PrintScreen" => 0xFF61,
        "Pause" => 0xFF13,
        "ContextMenu" => 0xFF67,
        "Space" => 0x20,
        "F1" => 0xFFBE,
        "F2" => 0xFFBF,
        "F3" => 0xFFC0,
        "F4" => 0xFFC1,
        "F5" => 0xFFC2,
        "F6" => 0xFFC3,
        "F7" => 0xFFC4,
        "F8" => 0xFFC5,
        "F9" => 0xFFC6,
        "F10" => 0xFFC7,
        "F11" => 0xFFC8,
        "F12" => 0xFFC9,
        "Numpad0" => 0xFFB0,
        "Numpad1" => 0xFFB1,
        "Numpad2" => 0xFFB2,
        "Numpad3" => 0xFFB3,
        "Numpad4" => 0xFFB4,
        "Numpad5" => 0xFFB5,
        "Numpad6" => 0xFFB6,
        "Numpad7" => 0xFFB7,
        "Numpad8" => 0xFFB8,
        "Numpad9" => 0xFFB9,
        "NumpadDecimal" => 0xFFAE,
        "NumpadAdd" => 0xFFAB,
        "NumpadSubtract" => 0xFFAD,
        "NumpadMultiply" => 0xFFAA,
        "NumpadDivide" => 0xFFAF,
        _ => return None,
    };
    Some(valor)
}

/// Keysym de X11 de un carácter: el valor Unicode en Latin-1 y, por encima,
/// el rango 0x01000000 + punto de código (X11 recomienda esa forma).
pub fn keysym_de_caracter(caracter: char) -> u32 {
    let punto = caracter as u32;
    if punto <= 0xFF {
        punto
    } else {
        0x0100_0000 + punto
    }
}

/// Vuelta de keysym a carácter Unicode (respaldo Unicode de RDP y
/// portapapeles). None si el keysym no representa un carácter.
pub fn caracter_de_keysym(keysym: u32) -> Option<char> {
    if keysym & 0xFF00_0000 == 0x0100_0000 {
        char::from_u32(keysym & 0x00FF_FFFF)
    } else if (0x20..=0x7E).contains(&keysym) || (0xA0..=0xFF).contains(&keysym) {
        char::from_u32(keysym)
    } else {
        None
    }
}

/// ¿Es una tecla modificadora? Sirve para no mandar un carácter Unicode por un
/// modificador en el respaldo de RDP.
pub fn es_modificador(codigo: &str) -> bool {
    matches!(
        codigo,
        "ShiftLeft"
            | "ShiftRight"
            | "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
            | "CapsLock"
            | "NumLock"
            | "ScrollLock"
    )
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_letras_y_los_numeros_tienen_scancode() {
        assert_eq!(scancode("KeyA"), Some((0x1E, false)));
        assert_eq!(scancode("KeyQ"), Some((0x10, false)));
        assert_eq!(scancode("Digit1"), Some((0x02, false)));
        assert_eq!(scancode("Enter"), Some((0x1C, false)));
        assert_eq!(scancode("Space"), Some((0x39, false)));
        assert_eq!(scancode("F12"), Some((0x58, false)));
    }

    #[test]
    fn las_teclas_extendidas_se_marcan() {
        assert_eq!(scancode("ArrowUp"), Some((0x48, true)));
        assert_eq!(scancode("Delete"), Some((0x53, true)));
        assert_eq!(scancode("ControlRight"), Some((0x1D, true)));
        assert_eq!(scancode("NumpadEnter"), Some((0x1C, true)));
        assert_eq!(scancode("MetaLeft"), Some((0x5B, true)));
    }

    #[test]
    fn los_keysyms_de_las_teclas_especiales_son_los_de_x11() {
        assert_eq!(keysym_de_codigo("Tab"), Some(0xFF09));
        assert_eq!(keysym_de_codigo("Return"), None);
        assert_eq!(keysym_de_codigo("Enter"), Some(0xFF0D));
        assert_eq!(keysym_de_codigo("F1"), Some(0xFFBE));
        assert_eq!(keysym_de_codigo("ArrowLeft"), Some(0xFF51));
        assert_eq!(keysym_de_codigo("ShiftLeft"), Some(0xFFE1));
    }

    #[test]
    fn los_caracteres_van_al_rango_unicode_cuando_hace_falta() {
        assert_eq!(keysym_de_caracter('a'), 0x61);
        assert_eq!(keysym_de_caracter('ñ'), 0xF1);
        assert_eq!(keysym_de_caracter('€'), 0x0100_20AC);
        assert_eq!(caracter_de_keysym(0x61), Some('a'));
        assert_eq!(caracter_de_keysym(0xF1), Some('ñ'));
        assert_eq!(caracter_de_keysym(0x0100_20AC), Some('€'));
        assert_eq!(caracter_de_keysym(0xFF51), None);
    }

    #[test]
    fn los_modificadores_se_reconocen() {
        assert!(es_modificador("ShiftLeft"));
        assert!(es_modificador("AltRight"));
        assert!(!es_modificador("KeyA"));
    }
}
