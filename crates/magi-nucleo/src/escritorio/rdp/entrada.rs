//! Entrada del escritorio RDP: teclado por scancodes del conjunto 1 (con
//! respaldo Unicode), ratón absoluto, botones y rueda.
//!
//! El estado de las teclas y de los botones lo lleva ironrdp-input, que además
//! suelta sola una tecla repetida. Al terminar la sesión se sueltan todas para
//! no dejar modificadores pegados en el remoto.

use ironrdp::input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp::pdu::input::fast_path::FastPathInputEvent;

use crate::escritorio::teclas;

/// Una muesca de rueda en RDP: 120 unidades. El visor manda píxeles del
/// escritorio y 120 px es lo que desplaza una muesca en los navegadores.
const UNIDADES_MUESCA: f32 = 120.0;
/// La rueda de RDP son 9 bits en complemento a dos: [-256, 255].
const RUEDA_MIN: i32 = -256;
const RUEDA_MAX: i32 = 255;

/// Estado de entrada de una sesión RDP.
pub(crate) struct Entrada {
    base: Database,
}

impl Entrada {
    pub(crate) fn nueva() -> Self {
        Self {
            base: Database::new(),
        }
    }

    /// Tecla del visor: por scancode si el código físico está en la tabla y,
    /// si no, con el carácter Unicode de reserva (teclados en pantalla y
    /// métodos de entrada). Los modificadores nunca mandan Unicode.
    pub(crate) fn tecla(&mut self, codigo: &str, keysym: u32, abajo: bool) -> Vec<Operation> {
        if let Some((codigo_scancode, extendida)) = teclas::scancode(codigo) {
            let scancode = Scancode::from_u8(extendida, codigo_scancode);
            return vec![if abajo {
                Operation::KeyPressed(scancode)
            } else {
                Operation::KeyReleased(scancode)
            }];
        }

        if !teclas::es_modificador(codigo) {
            if let Some(caracter) = teclas::caracter_de_keysym(keysym) {
                return vec![if abajo {
                    Operation::UnicodeKeyPressed(caracter)
                } else {
                    Operation::UnicodeKeyReleased(caracter)
                }];
            }
        }

        Vec::new()
    }

    /// Movimiento absoluto del ratón, ya acotado al escritorio.
    pub(crate) fn raton(&mut self, x: u16, y: u16) -> Vec<Operation> {
        vec![Operation::MouseMove(MousePosition { x, y })]
    }

    /// Botón del ratón con la numeración de X11 que manda el visor:
    /// 1 izquierdo, 2 central, 3 derecho, 8 atrás y 9 adelante.
    pub(crate) fn boton(&mut self, boton: u8, abajo: bool) -> Vec<Operation> {
        let Some(boton) = boton_de_x11(boton) else {
            return Vec::new();
        };
        vec![if abajo {
            Operation::MouseButtonPressed(boton)
        } else {
            Operation::MouseButtonReleased(boton)
        }]
    }

    /// Rueda: el visor manda el desplazamiento suave en píxeles del escritorio
    /// y RDP espera unidades de 120 por muesca.
    pub(crate) fn rueda(&mut self, dx: f32, dy: f32) -> Vec<Operation> {
        let mut operaciones = Vec::new();

        if let Some(unidades) = unidades_de_rueda(dy) {
            operaciones.push(Operation::WheelRotations(WheelRotations {
                is_vertical: true,
                rotation_units: unidades,
            }));
        }
        if let Some(unidades) = unidades_de_rueda(dx) {
            operaciones.push(Operation::WheelRotations(WheelRotations {
                is_vertical: false,
                rotation_units: unidades,
            }));
        }

        operaciones
    }

    /// Ctrl+Alt+Supr: se pulsan en orden y se sueltan al revés, como el
    /// teclado real.
    pub(crate) fn ctrl_alt_supr(&mut self) -> Vec<Operation> {
        let control = Scancode::from_u8(false, 0x1D);
        let alt = Scancode::from_u8(false, 0x38);
        let suprimir = Scancode::from_u8(true, 0x53);

        vec![
            Operation::KeyPressed(control),
            Operation::KeyPressed(alt),
            Operation::KeyPressed(suprimir),
            Operation::KeyReleased(suprimir),
            Operation::KeyReleased(alt),
            Operation::KeyReleased(control),
        ]
    }

    /// Tecla Windows: una pulsación completa de Meta izquierda.
    pub(crate) fn tecla_windows(&mut self) -> Vec<Operation> {
        let windows = Scancode::from_u8(true, 0x5B);
        vec![
            Operation::KeyPressed(windows),
            Operation::KeyReleased(windows),
        ]
    }

    /// Convierte operaciones en eventos de entrada de RDP.
    pub(crate) fn aplicar(
        &mut self,
        operaciones: impl IntoIterator<Item = Operation>,
    ) -> Vec<FastPathInputEvent> {
        self.base.apply(operaciones).into_iter().collect()
    }

    /// Suelta todas las teclas y botones (al perder el foco o al cerrar).
    pub(crate) fn soltar_todo(&mut self) -> Vec<FastPathInputEvent> {
        self.base.release_all().into_iter().collect()
    }
}

/// Botón del visor (numeración de X11) al botón de ironrdp-input.
fn boton_de_x11(boton: u8) -> Option<MouseButton> {
    match boton {
        1 => Some(MouseButton::Left),
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::Right),
        8 => Some(MouseButton::X1),
        9 => Some(MouseButton::X2),
        _ => None,
    }
}

/// Píxeles de desplazamiento a unidades de rueda de RDP, acotadas a [-256, 255].
/// El signo se invierte: deltaY positivo (hacia abajo) es rueda negativa en RDP.
fn unidades_de_rueda(delta: f32) -> Option<i16> {
    if !delta.is_finite() || delta.abs() < 1.0 {
        return None;
    }
    let unidades = (-delta / UNIDADES_MUESCA * 120.0).round();
    let acotado = unidades.clamp(RUEDA_MIN as f32, RUEDA_MAX as f32) as i32;
    if acotado == 0 {
        None
    } else {
        Some(acotado as i16)
    }
}

#[cfg(test)]
mod pruebas {
    use ironrdp::pdu::input::fast_path::KeyboardFlags;

    use super::*;

    fn teclas(eventos: &[FastPathInputEvent]) -> Vec<(u8, bool, bool)> {
        eventos
            .iter()
            .filter_map(|evento| match evento {
                FastPathInputEvent::KeyboardEvent(flags, codigo) => Some((
                    *codigo,
                    flags.contains(KeyboardFlags::EXTENDED),
                    flags.contains(KeyboardFlags::RELEASE),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn las_teclas_van_por_scancode() {
        let mut entrada = Entrada::nueva();
        let operaciones = entrada.tecla("KeyA", 0x0061, true);
        let pulsada = entrada.aplicar(operaciones);
        assert_eq!(teclas(&pulsada), vec![(0x1E, false, false)]);

        let operaciones = entrada.tecla("KeyA", 0x0061, false);
        let soltada = entrada.aplicar(operaciones);
        assert_eq!(teclas(&soltada), vec![(0x1E, false, true)]);

        let operaciones = entrada.tecla("ArrowUp", 0xFF52, true);
        let flecha = entrada.aplicar(operaciones);
        assert_eq!(teclas(&flecha), vec![(0x48, true, false)]);
    }

    #[test]
    fn sin_scancode_se_usa_unicode() {
        let mut entrada = Entrada::nueva();
        let operaciones = entrada.tecla("", 0x00F1, true);
        let pulsada = entrada.aplicar(operaciones);
        assert!(matches!(
            pulsada.as_slice(),
            [FastPathInputEvent::UnicodeKeyboardEvent(flags, 0x00F1)]
                if !flags.contains(KeyboardFlags::RELEASE)
        ));

        // Un modificador sin scancode no manda Unicode.
        let operaciones = entrada.tecla("", 0xFFE1, true);
        let nada = entrada.aplicar(operaciones);
        assert!(nada.is_empty());
    }

    #[test]
    fn ctrl_alt_supr_y_windows() {
        let mut entrada = Entrada::nueva();

        let operaciones = entrada.ctrl_alt_supr();
        let secuencia = entrada.aplicar(operaciones);
        assert_eq!(
            teclas(&secuencia),
            vec![
                (0x1D, false, false),
                (0x38, false, false),
                (0x53, true, false),
                (0x53, true, true),
                (0x38, false, true),
                (0x1D, false, true),
            ]
        );

        let operaciones = entrada.tecla_windows();
        let windows = entrada.aplicar(operaciones);
        assert_eq!(
            teclas(&windows),
            vec![(0x5B, true, false), (0x5B, true, true)]
        );
    }

    #[test]
    fn los_botones_van_en_numeracion_de_x11() {
        let mut entrada = Entrada::nueva();
        let operaciones = entrada.boton(1, true);
        assert!(matches!(
            entrada.aplicar(operaciones).as_slice(),
            [FastPathInputEvent::MouseEvent(_)]
        ));
        let operaciones = entrada.boton(8, true);
        assert!(matches!(
            entrada.aplicar(operaciones).as_slice(),
            [FastPathInputEvent::MouseEventEx(_)]
        ));
        assert!(entrada.boton(7, true).is_empty());
    }

    #[test]
    fn la_rueda_se_acota_a_nueve_bits() {
        let mut entrada = Entrada::nueva();
        let operaciones = entrada.rueda(0.0, 120.0);
        match entrada.aplicar(operaciones).as_slice() {
            [FastPathInputEvent::MouseEvent(pdu)] => {
                assert_eq!(pdu.number_of_wheel_rotation_units, -120);
            }
            otro => panic!("evento inesperado: {otro:?}"),
        }

        let operaciones = entrada.rueda(0.0, 100_000.0);
        match entrada.aplicar(operaciones).as_slice() {
            [FastPathInputEvent::MouseEvent(pdu)] => {
                assert_eq!(pdu.number_of_wheel_rotation_units, -256);
            }
            otro => panic!("evento inesperado: {otro:?}"),
        }

        assert!(entrada.rueda(0.0, 0.2).is_empty());
    }

    #[test]
    fn soltar_todo_suelta_las_teclas_pulsadas() {
        let mut entrada = Entrada::nueva();
        let operaciones = entrada.tecla("ShiftLeft", 0xFFE1, true);
        entrada.aplicar(operaciones);
        let sueltas = entrada.soltar_todo();
        assert_eq!(teclas(&sueltas), vec![(0x2A, false, true)]);
    }
}
