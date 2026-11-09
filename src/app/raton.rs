//! Ratón del terminal exterior (Fase 15).
//!
//! El ratón se le pasa al programa remoto que lo pide (modo 1000, 1002 o
//! 1003), con su modo y su codificación, también con `[terminal] raton =
//! false`. La opción solo enciende los usos propios de MAGI: la rueda en el
//! historial, el clic para elegir panel y el arrastre en el modo historial.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use magi_nucleo::terminal::raton::{self, Accion, Boton, Evento, Modificadores, ModoRaton};

/// ¿Hay que capturar el ratón del terminal exterior?
///
/// Con `raton = true` siempre (los usos propios de MAGI); sin él, solo cuando
/// el remoto del panel enfocado lo pide.
#[must_use]
pub fn capturar(raton_config: bool, modo: ModoRaton) -> bool {
    raton_config || modo.pedido()
}

/// Botón de crossterm para el núcleo.
fn traducir_boton(boton: MouseButton) -> Boton {
    match boton {
        MouseButton::Left => Boton::Izquierda,
        MouseButton::Middle => Boton::Media,
        MouseButton::Right => Boton::Derecha,
    }
}

/// Traduce un evento de crossterm a las coordenadas del contenido de un panel
/// (desde 1, sin su cabecera). `None` si el ratón cae fuera del contenido o el
/// evento no se reenvía.
#[must_use]
pub fn a_nucleo(evento: &MouseEvent, contenido: Rect) -> Option<Evento> {
    let dentro = contenido.contains(ratatui::layout::Position::new(evento.column, evento.row));
    if !dentro {
        return None;
    }
    let columna = evento.column.saturating_sub(contenido.x).saturating_add(1);
    let fila = evento.row.saturating_sub(contenido.y).saturating_add(1);
    let (accion, boton) = match evento.kind {
        MouseEventKind::Down(boton) => (Accion::Pulsar, traducir_boton(boton)),
        MouseEventKind::Up(boton) => (Accion::Soltar, traducir_boton(boton)),
        MouseEventKind::Drag(boton) => (Accion::Arrastrar, traducir_boton(boton)),
        MouseEventKind::ScrollUp => (Accion::RuedaArriba, Boton::Ninguno),
        MouseEventKind::ScrollDown => (Accion::RuedaAbajo, Boton::Ninguno),
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight | MouseEventKind::Moved => {
            return None;
        }
    };
    Some(Evento {
        accion,
        boton,
        columna,
        fila,
        modificadores: Modificadores {
            mayusculas: evento
                .modifiers
                .contains(crossterm::event::KeyModifiers::SHIFT),
            alt: evento
                .modifiers
                .contains(crossterm::event::KeyModifiers::ALT),
            ctrl: evento
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL),
        },
    })
}

/// Codifica el evento para el remoto si su modo lo admite.
#[must_use]
pub fn codificar(evento: &Evento, modo: ModoRaton) -> Option<Vec<u8>> {
    raton::codificar(evento, modo)
}

/// ¿El evento es una rueda? (para el historial y las flechas de la pantalla
/// alternativa; las de arrastre no cuentan).
#[must_use]
pub fn rueda(evento: &MouseEvent) -> Option<i32> {
    match evento.kind {
        MouseEventKind::ScrollUp => Some(1),
        MouseEventKind::ScrollDown => Some(-1),
        _ => None,
    }
}

/// ¿El evento es un clic (pulsar)?
#[must_use]
pub fn clic(evento: &MouseEvent) -> bool {
    matches!(evento.kind, MouseEventKind::Down(MouseButton::Left))
}

/// ¿El evento es un arrastre (selección en el modo historial)?
#[must_use]
pub fn arrastre(evento: &MouseEvent) -> bool {
    matches!(evento.kind, MouseEventKind::Drag(_))
}

/// Posición del ratón relativa a un área (sin exigir que esté dentro).
#[must_use]
pub fn posicion(evento: &MouseEvent, area: Rect) -> (usize, usize) {
    (
        usize::from(evento.column.saturating_sub(area.x)),
        usize::from(evento.row.saturating_sub(area.y)),
    )
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn evento(kind: MouseEventKind, columna: u16, fila: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: columna,
            row: fila,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn las_coordenadas_van_desde_uno_sin_la_cabecera() {
        let contenido = Rect::new(10, 5, 20, 10);
        let traducido = a_nucleo(
            &evento(MouseEventKind::Down(MouseButton::Left), 10, 6),
            contenido,
        )
        .unwrap();
        assert_eq!((traducido.columna, traducido.fila), (1, 2));
        // Fuera del contenido no se reenvía.
        assert!(a_nucleo(
            &evento(MouseEventKind::Down(MouseButton::Left), 0, 0),
            contenido
        )
        .is_none());
    }

    #[test]
    fn sin_raton_configurado_se_captura_solo_si_el_remoto_lo_pide() {
        let ninguno = ModoRaton::default();
        assert!(!capturar(false, ninguno));
        assert!(capturar(true, ninguno));
        let pedido = ModoRaton {
            modo: magi_nucleo::terminal::raton::Modo::PulsarSoltar,
            codificacion: magi_nucleo::terminal::raton::Codificacion::Sgr,
        };
        assert!(capturar(false, pedido));
    }

    #[test]
    fn la_rueda_y_el_clic_se_reconocen() {
        assert_eq!(rueda(&evento(MouseEventKind::ScrollUp, 1, 1)), Some(1));
        assert_eq!(rueda(&evento(MouseEventKind::ScrollDown, 1, 1)), Some(-1));
        assert_eq!(
            rueda(&evento(MouseEventKind::Down(MouseButton::Left), 1, 1)),
            None
        );
        assert!(clic(&evento(MouseEventKind::Down(MouseButton::Left), 1, 1)));
        assert!(arrastre(&evento(
            MouseEventKind::Drag(MouseButton::Left),
            1,
            1
        )));
    }
}
