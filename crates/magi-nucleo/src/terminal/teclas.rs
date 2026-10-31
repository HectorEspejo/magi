//! Codificación de teclas a los bytes de un PTY remoto (Fase 11).
//!
//! La misma tabla vale para la fila de teclas de la app, el teclado del sistema
//! y el teclado físico: todo pasa por aquí. El modo de la sesión (cursor de
//! aplicación y pegado entre corchetes) lo da el parser vt100.

/// Una tecla física, sin depender de crossterm ni de Android.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tecla {
    /// Un carácter, ya con las mayúsculas que haya puesto el teclado.
    Caracter(char),
    /// Intro.
    Intro,
    /// Tabulador.
    Tabulador,
    /// Mayús+Tabulador.
    TabuladorInverso,
    /// Retroceso.
    Retroceso,
    /// Escape.
    Escape,
    /// Flecha arriba.
    Arriba,
    /// Flecha abajo.
    Abajo,
    /// Flecha derecha.
    Derecha,
    /// Flecha izquierda.
    Izquierda,
    /// Inicio.
    Inicio,
    /// Fin.
    Fin,
    /// RePág.
    RePagina,
    /// AvPág.
    AvPagina,
    /// Supr.
    Suprimir,
    /// Insert.
    Insertar,
    /// Una tecla de función de F1 a F12.
    F(u8),
}

/// Modificadores de una pulsación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modificadores {
    /// Mayúsculas.
    pub mayusculas: bool,
    /// Alt.
    pub alt: bool,
    /// Ctrl.
    pub ctrl: bool,
}

impl Modificadores {
    /// ¿Hay algún modificador activo?
    #[must_use]
    pub fn hay(self) -> bool {
        self.mayusculas || self.alt || self.ctrl
    }

    /// Valor `m` de la forma `1 + Mayús·1 + Alt·2 + Ctrl·4` que usan los
    /// terminales para las flechas modificadas (`ESC [ 1 ; m A`).
    #[must_use]
    pub fn valor_xterm(self) -> u8 {
        let mut valor = 1;
        if self.mayusculas {
            valor += 1;
        }
        if self.alt {
            valor += 2;
        }
        if self.ctrl {
            valor += 4;
        }
        valor
    }
}

/// Modo del terminal que cambia la codificación (lo da el parser).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModoTerminal {
    /// Cursor de aplicación activo (flechas e Inicio/Fin con `ESC O`).
    pub cursor_aplicacion: bool,
    /// Pegado entre corchetes activo (modo 2004).
    pub pegado_corchetes: bool,
}

impl ModoTerminal {
    /// Lee el modo de la pantalla del parser vt100.
    #[must_use]
    pub fn desde_pantalla(pantalla: &vt100::Screen) -> Self {
        Self {
            cursor_aplicacion: pantalla.application_cursor(),
            pegado_corchetes: pantalla.bracketed_paste(),
        }
    }

    /// Lee el modo de la sesión del parser vt100.
    #[must_use]
    pub fn desde_parser(parser: &vt100::Parser) -> Self {
        Self::desde_pantalla(parser.screen())
    }
}

const ESC: u8 = 0x1b;

/// Codifica una tecla con sus modificadores y el modo del terminal.
///
/// Devuelve `None` si la combinación no tiene una codificación estándar (por
/// ejemplo, Ctrl con un carácter que no es ASCII).
#[must_use]
pub fn codificar(
    tecla: Tecla,
    modificadores: Modificadores,
    modo: ModoTerminal,
) -> Option<Vec<u8>> {
    if modificadores.hay() {
        if let Some(final_flecha) = final_de_flecha(tecla) {
            let mut bytes = vec![ESC, b'[', b'1', b';'];
            bytes.push(b'0' + modificadores.valor_xterm());
            bytes.push(final_flecha);
            return Some(bytes);
        }
    }
    let mut bytes = match tecla {
        Tecla::Caracter(caracter) => {
            if modificadores.ctrl {
                vec![byte_de_control(caracter)?]
            } else {
                let mut buffer = [0u8; 4];
                caracter.encode_utf8(&mut buffer).as_bytes().to_vec()
            }
        }
        Tecla::Intro => vec![b'\r'],
        Tecla::Tabulador => vec![b'\t'],
        Tecla::TabuladorInverso => vec![ESC, b'[', b'Z'],
        Tecla::Retroceso => vec![0x7f],
        Tecla::Escape => vec![ESC],
        Tecla::Arriba => bytes_de_cursor(b'A', modo),
        Tecla::Abajo => bytes_de_cursor(b'B', modo),
        Tecla::Derecha => bytes_de_cursor(b'C', modo),
        Tecla::Izquierda => bytes_de_cursor(b'D', modo),
        Tecla::Inicio => bytes_de_cursor(b'H', modo),
        Tecla::Fin => bytes_de_cursor(b'F', modo),
        Tecla::RePagina => b"\x1b[5~".to_vec(),
        Tecla::AvPagina => b"\x1b[6~".to_vec(),
        Tecla::Suprimir => b"\x1b[3~".to_vec(),
        Tecla::Insertar => b"\x1b[2~".to_vec(),
        Tecla::F(numero) => secuencia_de_funcion(numero)?,
    };
    if modificadores.alt {
        let mut con_alt = Vec::with_capacity(bytes.len() + 1);
        con_alt.push(ESC);
        con_alt.append(&mut bytes);
        return Some(con_alt);
    }
    Some(bytes)
}

/// Final `A`-`D` de una flecha, para la forma modificada.
fn final_de_flecha(tecla: Tecla) -> Option<u8> {
    match tecla {
        Tecla::Arriba => Some(b'A'),
        Tecla::Abajo => Some(b'B'),
        Tecla::Derecha => Some(b'C'),
        Tecla::Izquierda => Some(b'D'),
        _ => None,
    }
}

/// Flecha o Inicio/Fin en modo normal (`ESC [ X`) o de aplicación (`ESC O X`).
fn bytes_de_cursor(final_cursor: u8, modo: ModoTerminal) -> Vec<u8> {
    if modo.cursor_aplicacion {
        vec![ESC, b'O', final_cursor]
    } else {
        vec![ESC, b'[', final_cursor]
    }
}

/// Secuencia de una tecla de función; `None` fuera de F1-F12.
fn secuencia_de_funcion(numero: u8) -> Option<Vec<u8>> {
    Some(match numero {
        1 => b"\x1bOP".to_vec(),
        2 => b"\x1bOQ".to_vec(),
        3 => b"\x1bOR".to_vec(),
        4 => b"\x1bOS".to_vec(),
        5 => b"\x1b[15~".to_vec(),
        6 => b"\x1b[17~".to_vec(),
        7 => b"\x1b[18~".to_vec(),
        8 => b"\x1b[19~".to_vec(),
        9 => b"\x1b[20~".to_vec(),
        10 => b"\x1b[21~".to_vec(),
        11 => b"\x1b[23~".to_vec(),
        12 => b"\x1b[24~".to_vec(),
        _ => return None,
    })
}

/// Byte de control de una letra o símbolo con Ctrl (`letra AND 0x1F`).
///
/// Crossterm entrega algunos de estos bytes como dígitos (`Ctrl+]` llega como
/// `Ctrl+5`), así que se admiten las dos formas.
fn byte_de_control(caracter: char) -> Option<u8> {
    Some(match caracter {
        'a'..='z' => caracter as u8 - b'a' + 1,
        'A'..='Z' => caracter as u8 - b'A' + 1,
        ' ' | '@' | '2' => 0,
        '[' | '3' => ESC,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

/// Bytes de un pegado: el texto tal cual o entre corchetes si el remoto activó
/// el modo 2004.
#[must_use]
pub fn pegar(texto: &str, modo: ModoTerminal) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(texto.len() + 12);
    if modo.pegado_corchetes {
        bytes.extend_from_slice(b"\x1b[200~");
    }
    bytes.extend_from_slice(texto.as_bytes());
    if modo.pegado_corchetes {
        bytes.extend_from_slice(b"\x1b[201~");
    }
    bytes
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn normal() -> ModoTerminal {
        ModoTerminal::default()
    }

    fn aplicacion() -> ModoTerminal {
        ModoTerminal {
            cursor_aplicacion: true,
            pegado_corchetes: false,
        }
    }

    fn sin_modificadores() -> Modificadores {
        Modificadores::default()
    }

    fn mods(mayusculas: bool, alt: bool, ctrl: bool) -> Modificadores {
        Modificadores {
            mayusculas,
            alt,
            ctrl,
        }
    }

    fn codifica(tecla: Tecla) -> Vec<u8> {
        codificar(tecla, sin_modificadores(), normal()).expect("la tecla tiene codificación")
    }

    fn codifica_con(tecla: Tecla, modificadores: Modificadores) -> Vec<u8> {
        codificar(tecla, modificadores, normal()).expect("la tecla tiene codificación")
    }

    #[test]
    fn las_flechas_cambian_con_el_modo_de_cursor() {
        // AC del checklist: en modo aplicación, ↑ manda ESC O A; en normal, ESC [ A.
        assert_eq!(codifica(Tecla::Arriba), b"\x1b[A".to_vec());
        assert_eq!(
            codificar(Tecla::Arriba, sin_modificadores(), aplicacion()).unwrap(),
            b"\x1bOA".to_vec()
        );
        assert_eq!(codifica(Tecla::Abajo), b"\x1b[B".to_vec());
        assert_eq!(
            codificar(Tecla::Abajo, sin_modificadores(), aplicacion()).unwrap(),
            b"\x1bOB".to_vec()
        );
        assert_eq!(codifica(Tecla::Derecha), b"\x1b[C".to_vec());
        assert_eq!(
            codificar(Tecla::Derecha, sin_modificadores(), aplicacion()).unwrap(),
            b"\x1bOC".to_vec()
        );
        assert_eq!(codifica(Tecla::Izquierda), b"\x1b[D".to_vec());
        assert_eq!(
            codificar(Tecla::Izquierda, sin_modificadores(), aplicacion()).unwrap(),
            b"\x1bOD".to_vec()
        );
    }

    #[test]
    fn inicio_y_fin_cambian_con_el_modo_de_cursor() {
        assert_eq!(codifica(Tecla::Inicio), b"\x1b[H".to_vec());
        assert_eq!(
            codificar(Tecla::Inicio, sin_modificadores(), aplicacion()).unwrap(),
            b"\x1bOH".to_vec()
        );
        assert_eq!(codifica(Tecla::Fin), b"\x1b[F".to_vec());
        assert_eq!(
            codificar(Tecla::Fin, sin_modificadores(), aplicacion()).unwrap(),
            b"\x1bOF".to_vec()
        );
    }

    #[test]
    fn las_teclas_de_edicion_no_cambian_con_el_modo() {
        for modo in [normal(), aplicacion()] {
            assert_eq!(
                codificar(Tecla::RePagina, sin_modificadores(), modo).unwrap(),
                b"\x1b[5~".to_vec()
            );
            assert_eq!(
                codificar(Tecla::AvPagina, sin_modificadores(), modo).unwrap(),
                b"\x1b[6~".to_vec()
            );
            assert_eq!(
                codificar(Tecla::Suprimir, sin_modificadores(), modo).unwrap(),
                b"\x1b[3~".to_vec()
            );
            assert_eq!(
                codificar(Tecla::Insertar, sin_modificadores(), modo).unwrap(),
                b"\x1b[2~".to_vec()
            );
        }
    }

    #[test]
    fn las_teclas_de_funcion_van_de_f1_a_f12() {
        let esperadas: [&[u8]; 12] = [
            b"\x1bOP",
            b"\x1bOQ",
            b"\x1bOR",
            b"\x1bOS",
            b"\x1b[15~",
            b"\x1b[17~",
            b"\x1b[18~",
            b"\x1b[19~",
            b"\x1b[20~",
            b"\x1b[21~",
            b"\x1b[23~",
            b"\x1b[24~",
        ];
        for (indice, esperada) in esperadas.iter().enumerate() {
            let numero = u8::try_from(indice + 1).unwrap();
            assert_eq!(codifica(Tecla::F(numero)), esperada.to_vec());
        }
        assert_eq!(codificar(Tecla::F(0), sin_modificadores(), normal()), None);
        assert_eq!(codificar(Tecla::F(13), sin_modificadores(), normal()), None);
    }

    #[test]
    fn las_flechas_con_modificadores_usan_la_forma_csi() {
        // m = 1 + Mayús·1 + Alt·2 + Ctrl·4.
        let casos = [
            (mods(true, false, false), "\x1b[1;2A"),
            (mods(false, true, false), "\x1b[1;3A"),
            (mods(false, false, true), "\x1b[1;5A"),
            (mods(true, true, false), "\x1b[1;4A"),
            (mods(true, false, true), "\x1b[1;6A"),
            (mods(false, true, true), "\x1b[1;7A"),
            (mods(true, true, true), "\x1b[1;8A"),
        ];
        for (modificadores, esperada) in casos {
            assert_eq!(
                codifica_con(Tecla::Arriba, modificadores),
                esperada.as_bytes()
            );
        }
        // El modo de cursor no cambia la forma modificada.
        assert_eq!(
            codificar(Tecla::Arriba, mods(false, false, true), aplicacion()).unwrap(),
            b"\x1b[1;5A".to_vec()
        );
    }

    #[test]
    fn ctrl_letra_manda_el_byte_de_control() {
        assert_eq!(
            codifica_con(Tecla::Caracter('a'), mods(false, false, true)),
            vec![0x01]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter('z'), mods(false, false, true)),
            vec![0x1a]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter('A'), mods(false, false, true)),
            vec![0x01]
        );
        // Ctrl+espacio es NUL y Ctrl+[ es ESC.
        assert_eq!(
            codifica_con(Tecla::Caracter(' '), mods(false, false, true)),
            vec![0x00]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter('['), mods(false, false, true)),
            vec![0x1b]
        );
        // Las formas que entrega crossterm para los bytes 0x1c-0x1f.
        assert_eq!(
            codifica_con(Tecla::Caracter('4'), mods(false, false, true)),
            vec![0x1c]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter('5'), mods(false, false, true)),
            vec![0x1d]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter(']'), mods(false, false, true)),
            vec![0x1d]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter('^'), mods(false, false, true)),
            vec![0x1e]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter('_'), mods(false, false, true)),
            vec![0x1f]
        );
        assert_eq!(
            codifica_con(Tecla::Caracter('?'), mods(false, false, true)),
            vec![0x7f]
        );
        // Lo que no tiene byte de control no manda nada.
        assert_eq!(
            codificar(Tecla::Caracter('ñ'), mods(false, false, true), normal()),
            None
        );
        assert_eq!(
            codificar(Tecla::Caracter('0'), mods(false, false, true), normal()),
            None
        );
    }

    #[test]
    fn alt_prefija_escape() {
        assert_eq!(
            codifica_con(Tecla::Caracter('x'), mods(false, true, false)),
            b"\x1bx".to_vec()
        );
        assert_eq!(
            codifica_con(Tecla::Intro, mods(false, true, false)),
            b"\x1b\r".to_vec()
        );
        // Ctrl+Alt manda Escape y el byte de control.
        assert_eq!(
            codifica_con(Tecla::Caracter('a'), mods(false, true, true)),
            vec![0x1b, 0x01]
        );
        // Alt+flecha es la forma modificada, no Escape + flecha.
        assert_eq!(
            codifica_con(Tecla::Arriba, mods(false, true, false)),
            b"\x1b[1;3A".to_vec()
        );
    }

    #[test]
    fn las_teclas_simples_tienen_sus_bytes() {
        assert_eq!(codifica(Tecla::Intro), b"\r".to_vec());
        assert_eq!(codifica(Tecla::Tabulador), b"\t".to_vec());
        assert_eq!(codifica(Tecla::TabuladorInverso), b"\x1b[Z".to_vec());
        assert_eq!(codifica(Tecla::Retroceso), vec![0x7f]);
        assert_eq!(codifica(Tecla::Escape), vec![0x1b]);
        assert_eq!(codifica(Tecla::Caracter('á')), "á".as_bytes().to_vec());
        assert_eq!(codifica(Tecla::Caracter('日')), "日".as_bytes().to_vec());
    }

    #[test]
    fn el_pegado_usa_los_corchetes_solo_si_el_remoto_lo_activo() {
        let texto = "una orden\ndos";
        assert_eq!(pegar(texto, normal()), texto.as_bytes().to_vec());
        let entre_corchetes = pegar(
            texto,
            ModoTerminal {
                cursor_aplicacion: false,
                pegado_corchetes: true,
            },
        );
        assert_eq!(
            entre_corchetes,
            format!("\x1b[200~{texto}\x1b[201~").as_bytes().to_vec()
        );
    }

    #[test]
    fn el_modo_se_lee_del_parser() {
        let mut parser = vt100::Parser::new(2, 10, 0);
        assert_eq!(ModoTerminal::desde_parser(&parser), normal());
        parser.process(b"\x1b[?1h\x1b[?2004h");
        let modo = ModoTerminal::desde_parser(&parser);
        assert!(modo.cursor_aplicacion);
        assert!(modo.pegado_corchetes);
        assert_eq!(ModoTerminal::desde_pantalla(parser.screen()), modo);
    }
}
