//! Codificación del ratón para el remoto (Fase 15).
//!
//! El ratón se le pasa al programa remoto que lo pide, con su modo (1000,
//! 1002 o 1003) y su codificación (por defecto, UTF-8 1005 o SGR 1006). Las
//! coordenadas van desde 1 y son relativas al contenido del panel (sin su
//! cabecera).

use crate::terminal::teclas::Modificadores;

/// Botón del ratón.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boton {
    Izquierda,
    Media,
    Derecha,
    /// Sin botón (movimiento o rueda).
    Ninguno,
}

/// Lo que hizo el ratón.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accion {
    Pulsar,
    Soltar,
    Arrastrar,
    RuedaArriba,
    RuedaAbajo,
}

/// Un evento de ratón ya relativo al contenido de un panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Evento {
    pub accion: Accion,
    pub boton: Boton,
    /// Columna desde 1.
    pub columna: u16,
    /// Fila desde 1.
    pub fila: u16,
    pub modificadores: Modificadores,
}

/// Modo del ratón que pidió el remoto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modo {
    #[default]
    Ninguno,
    /// 9: solo pulsación.
    Pulsar,
    /// 1000: pulsar y soltar.
    PulsarSoltar,
    /// 1002: además, arrastrar con un botón pulsado.
    BotonMovimiento,
    /// 1003: todo el movimiento, con o sin botón.
    TodoMovimiento,
}

/// Codificación de las coordenadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Codificacion {
    #[default]
    PorDefecto,
    Utf8,
    Sgr,
}

/// Lo que el parser dice del ratón de una pestaña.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModoRaton {
    pub modo: Modo,
    pub codificacion: Codificacion,
}

impl ModoRaton {
    /// Lee el modo del ratón del parser vt100.
    #[must_use]
    pub fn desde_pantalla(pantalla: &vt100::Screen) -> Self {
        use vt100::MouseProtocolMode as ModoVt;
        let modo = match pantalla.mouse_protocol_mode() {
            ModoVt::None => Modo::Ninguno,
            ModoVt::Press => Modo::Pulsar,
            ModoVt::PressRelease => Modo::PulsarSoltar,
            ModoVt::ButtonMotion => Modo::BotonMovimiento,
            ModoVt::AnyMotion => Modo::TodoMovimiento,
        };
        let codificacion = match pantalla.mouse_protocol_encoding() {
            vt100::MouseProtocolEncoding::Default => Codificacion::PorDefecto,
            vt100::MouseProtocolEncoding::Utf8 => Codificacion::Utf8,
            vt100::MouseProtocolEncoding::Sgr => Codificacion::Sgr,
        };
        Self { modo, codificacion }
    }

    /// ¿El remoto pide el ratón?
    #[must_use]
    pub fn pedido(&self) -> bool {
        self.modo != Modo::Ninguno
    }
}

/// Código de botón base para un botón (0, 1, 2; 3 = ninguno).
fn boton_base(boton: Boton) -> u8 {
    match boton {
        Boton::Izquierda => 0,
        Boton::Media => 1,
        Boton::Derecha => 2,
        Boton::Ninguno => 3,
    }
}

fn modificadores_xterm(modificadores: Modificadores) -> u8 {
    let mut valor = 0;
    if modificadores.mayusculas {
        valor += 4;
    }
    if modificadores.alt {
        valor += 8;
    }
    if modificadores.ctrl {
        valor += 16;
    }
    valor
}

/// ¿El modo deja pasar este evento?
fn permitido(modo: Modo, accion: Accion, boton: Boton) -> bool {
    match modo {
        Modo::Ninguno => false,
        Modo::Pulsar => {
            matches!(
                accion,
                Accion::Pulsar | Accion::RuedaArriba | Accion::RuedaAbajo
            )
        }
        Modo::PulsarSoltar => !matches!(accion, Accion::Arrastrar),
        Modo::BotonMovimiento => !matches!(accion, Accion::Arrastrar) || boton != Boton::Ninguno,
        Modo::TodoMovimiento => true,
    }
}

/// Codifica un evento de ratón para el remoto. `None` si no hay que enviarlo
/// (modo que no lo admite o coordenada fuera del rango de la codificación).
#[must_use]
pub fn codificar(evento: &Evento, modo: ModoRaton) -> Option<Vec<u8>> {
    if !permitido(modo.modo, evento.accion, evento.boton) {
        return None;
    }
    if evento.columna == 0 || evento.fila == 0 {
        return None;
    }
    let mods = modificadores_xterm(evento.modificadores);
    let (codigo, final_sgr) = match evento.accion {
        Accion::Pulsar => (boton_base(evento.boton), 'M'),
        Accion::Soltar => (boton_base(evento.boton), 'm'),
        Accion::Arrastrar => (32 + boton_base(evento.boton), 'M'),
        Accion::RuedaArriba => (64, 'M'),
        Accion::RuedaAbajo => (65, 'M'),
    };
    match modo.codificacion {
        Codificacion::Sgr => {
            let boton = codigo + mods;
            Some(
                format!(
                    "\x1b[<{boton};{};{}{}",
                    evento.columna, evento.fila, final_sgr
                )
                .into_bytes(),
            )
        }
        Codificacion::PorDefecto | Codificacion::Utf8 => {
            // La codificación por defecto llega a la columna o fila 223.
            if modo.codificacion == Codificacion::PorDefecto
                && (evento.columna > 223 || evento.fila > 223)
            {
                return None;
            }
            // En la codificación por defecto, soltar es siempre el código 3.
            let base = if evento.accion == Accion::Soltar {
                3
            } else {
                codigo
            };
            let mut bytes = vec![0x1b, b'[', b'M', 32 + base + mods];
            if modo.codificacion == Codificacion::Utf8 {
                let mut buffer = [0u8; 4];
                for valor in [33 + evento.columna, 33 + evento.fila] {
                    let caracter = char::from_u32(u32::from(valor)).unwrap_or(' ');
                    bytes.extend_from_slice(caracter.encode_utf8(&mut buffer).as_bytes());
                }
            } else {
                bytes.push(33 + evento.columna as u8);
                bytes.push(33 + evento.fila as u8);
            }
            Some(bytes)
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn evento(accion: Accion, boton: Boton, columna: u16, fila: u16) -> Evento {
        Evento {
            accion,
            boton,
            columna,
            fila,
            modificadores: Modificadores::default(),
        }
    }

    #[test]
    fn sgr_codifica_pulsar_soltar_arrastrar_y_rueda() {
        let modo = ModoRaton {
            modo: Modo::TodoMovimiento,
            codificacion: Codificacion::Sgr,
        };
        assert_eq!(
            codificar(&evento(Accion::Pulsar, Boton::Izquierda, 3, 4), modo).unwrap(),
            b"\x1b[<0;3;4M"
        );
        assert_eq!(
            codificar(&evento(Accion::Soltar, Boton::Izquierda, 3, 4), modo).unwrap(),
            b"\x1b[<0;3;4m"
        );
        assert_eq!(
            codificar(&evento(Accion::Arrastrar, Boton::Derecha, 5, 6), modo).unwrap(),
            b"\x1b[<34;5;6M"
        );
        assert_eq!(
            codificar(&evento(Accion::RuedaArriba, Boton::Ninguno, 1, 1), modo).unwrap(),
            b"\x1b[<64;1;1M"
        );
        assert_eq!(
            codificar(&evento(Accion::RuedaAbajo, Boton::Ninguno, 1, 1), modo).unwrap(),
            b"\x1b[<65;1;1M"
        );
    }

    #[test]
    fn los_modificadores_van_sumarizados() {
        let modo = ModoRaton {
            modo: Modo::PulsarSoltar,
            codificacion: Codificacion::Sgr,
        };
        let mut pulsar = evento(Accion::Pulsar, Boton::Izquierda, 1, 1);
        pulsar.modificadores = Modificadores {
            mayusculas: true,
            alt: true,
            ctrl: true,
        };
        assert_eq!(codificar(&pulsar, modo).unwrap(), b"\x1b[<28;1;1M");
    }

    #[test]
    fn por_defecto_no_pasa_de_223_y_utf8_usa_codigos_utf8() {
        let por_defecto = ModoRaton {
            modo: Modo::PulsarSoltar,
            codificacion: Codificacion::PorDefecto,
        };
        assert!(codificar(
            &evento(Accion::Pulsar, Boton::Izquierda, 224, 3),
            por_defecto
        )
        .is_none());
        assert_eq!(
            codificar(&evento(Accion::Pulsar, Boton::Izquierda, 3, 4), por_defecto).unwrap(),
            b"\x1b[M $%"
        );
        let utf8 = ModoRaton {
            modo: Modo::PulsarSoltar,
            codificacion: Codificacion::Utf8,
        };
        let bytes = codificar(&evento(Accion::Pulsar, Boton::Izquierda, 300, 4), utf8).unwrap();
        assert_eq!(bytes[..4], [0x1b, b'[', b'M', 32]);
        let texto = std::str::from_utf8(&bytes[4..]).unwrap();
        assert_eq!(texto.chars().count(), 2);
    }

    #[test]
    fn los_modos_filtran_lo_que_no_admiten() {
        let pulsar_soltar = ModoRaton {
            modo: Modo::PulsarSoltar,
            codificacion: Codificacion::Sgr,
        };
        assert!(codificar(
            &evento(Accion::Arrastrar, Boton::Izquierda, 1, 1),
            pulsar_soltar
        )
        .is_none());
        assert!(codificar(
            &evento(Accion::Soltar, Boton::Izquierda, 1, 1),
            ModoRaton {
                modo: Modo::Ninguno,
                codificacion: Codificacion::Sgr
            }
        )
        .is_none());
        let solo_pulsar = ModoRaton {
            modo: Modo::Pulsar,
            codificacion: Codificacion::Sgr,
        };
        assert!(codificar(&evento(Accion::Soltar, Boton::Izquierda, 1, 1), solo_pulsar).is_none());
        assert!(codificar(
            &evento(Accion::RuedaArriba, Boton::Ninguno, 1, 1),
            solo_pulsar
        )
        .is_some());
        let con_boton = ModoRaton {
            modo: Modo::BotonMovimiento,
            codificacion: Codificacion::Sgr,
        };
        assert!(codificar(
            &evento(Accion::Arrastrar, Boton::Izquierda, 1, 1),
            con_boton
        )
        .is_some());
        assert!(codificar(&evento(Accion::Arrastrar, Boton::Ninguno, 1, 1), con_boton).is_none());
    }

    #[test]
    fn lee_el_modo_de_la_pantalla() {
        let mut parser = vt100::Parser::new(2, 10, 0);
        assert!(!ModoRaton::desde_pantalla(parser.screen()).pedido());
        parser.process(b"\x1b[?1002;1006h");
        let modo = ModoRaton::desde_pantalla(parser.screen());
        assert_eq!(modo.modo, Modo::BotonMovimiento);
        assert_eq!(modo.codificacion, Codificacion::Sgr);
    }
}
