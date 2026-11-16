//! Pruebas de caja del ratón del terminal (Fase 15): modos 9, 1000, 1002 y
//! 1003; codificaciones por defecto, UTF-8 (1005) y SGR (1006); botones,
//! arrastre, rueda y modificadores. Sin red ni interfaz.

use magi_nucleo::terminal::raton::{
    codificar, Accion, Boton, Codificacion, Evento, Modo, ModoRaton,
};

use magi_nucleo::terminal::teclas::Modificadores;

fn evento(accion: Accion, boton: Boton, columna: u16, fila: u16) -> Evento {
    Evento {
        accion,
        boton,
        columna,
        fila,
        modificadores: Modificadores::default(),
    }
}

fn modo(modo: Modo, codificacion: Codificacion) -> ModoRaton {
    ModoRaton { modo, codificacion }
}

#[test]
fn sgr_codifica_pulsar_soltar_arrastrar_y_rueda() {
    let todo = modo(Modo::TodoMovimiento, Codificacion::Sgr);
    assert_eq!(
        codificar(&evento(Accion::Pulsar, Boton::Izquierda, 3, 4), todo).unwrap(),
        b"\x1b[<0;3;4M"
    );
    assert_eq!(
        codificar(&evento(Accion::Soltar, Boton::Izquierda, 3, 4), todo).unwrap(),
        b"\x1b[<0;3;4m"
    );
    assert_eq!(
        codificar(&evento(Accion::Arrastrar, Boton::Derecha, 5, 6), todo).unwrap(),
        b"\x1b[<34;5;6M"
    );
    assert_eq!(
        codificar(&evento(Accion::RuedaArriba, Boton::Ninguno, 1, 1), todo).unwrap(),
        b"\x1b[<64;1;1M"
    );
    assert_eq!(
        codificar(&evento(Accion::RuedaAbajo, Boton::Ninguno, 1, 1), todo).unwrap(),
        b"\x1b[<65;1;1M"
    );
    // El botón central también se codifica.
    assert_eq!(
        codificar(&evento(Accion::Pulsar, Boton::Media, 2, 2), todo).unwrap(),
        b"\x1b[<1;2;2M"
    );
}

#[test]
fn los_modificadores_se_suman_al_boton() {
    let modo = modo(Modo::PulsarSoltar, Codificacion::Sgr);
    let mut con_modificadores = evento(Accion::Pulsar, Boton::Izquierda, 1, 1);
    con_modificadores.modificadores = Modificadores {
        mayusculas: true,
        alt: true,
        ctrl: true,
    };
    // 0 (izquierda) + 4 (mayúsculas) + 8 (alt) + 16 (ctrl) = 28.
    assert_eq!(
        codificar(&con_modificadores, modo).unwrap(),
        b"\x1b[<28;1;1M"
    );
    let mut solo_ctrl = evento(Accion::Pulsar, Boton::Izquierda, 1, 1);
    solo_ctrl.modificadores = Modificadores {
        mayusculas: false,
        alt: false,
        ctrl: true,
    };
    assert_eq!(codificar(&solo_ctrl, modo).unwrap(), b"\x1b[<16;1;1M");
}

#[test]
fn cada_modo_filtra_lo_que_no_admite() {
    let ninguno = modo(Modo::Ninguno, Codificacion::Sgr);
    assert!(codificar(&evento(Accion::Pulsar, Boton::Izquierda, 1, 1), ninguno).is_none());

    // 9: solo pulsación (y rueda); ni soltar ni arrastrar.
    let solo_pulsar = modo(Modo::Pulsar, Codificacion::Sgr);
    assert!(codificar(&evento_pulsar(), solo_pulsar).is_some());
    assert!(codificar(&evento(Accion::Soltar, Boton::Izquierda, 1, 1), solo_pulsar).is_none());
    assert!(codificar(
        &evento(Accion::Arrastrar, Boton::Izquierda, 1, 1),
        solo_pulsar
    )
    .is_none());
    assert!(codificar(
        &evento(Accion::RuedaArriba, Boton::Ninguno, 1, 1),
        solo_pulsar
    )
    .is_some());

    // 1000: pulsar y soltar, sin arrastre.
    let pulsar_soltar = modo(Modo::PulsarSoltar, Codificacion::Sgr);
    assert!(codificar(
        &evento(Accion::Pulsar, Boton::Izquierda, 1, 1),
        pulsar_soltar
    )
    .is_some());
    assert!(codificar(
        &evento(Accion::Soltar, Boton::Izquierda, 1, 1),
        pulsar_soltar
    )
    .is_some());
    assert!(codificar(
        &evento(Accion::Arrastrar, Boton::Izquierda, 1, 1),
        pulsar_soltar
    )
    .is_none());

    // 1002: el arrastre solo con un botón pulsado.
    let con_boton = modo(Modo::BotonMovimiento, Codificacion::Sgr);
    assert!(codificar(
        &evento(Accion::Arrastrar, Boton::Izquierda, 1, 1),
        con_boton
    )
    .is_some());
    assert!(codificar(&evento(Accion::Arrastrar, Boton::Ninguno, 1, 1), con_boton).is_none());

    // 1003: todo el movimiento, con o sin botón.
    let todo = modo(Modo::TodoMovimiento, Codificacion::Sgr);
    assert!(codificar(&evento(Accion::Arrastrar, Boton::Izquierda, 1, 1), todo).is_some());
    assert!(codificar(&evento(Accion::Arrastrar, Boton::Ninguno, 1, 1), todo).is_some());
}

fn evento_pulsar() -> Evento {
    evento(Accion::Pulsar, Boton::Izquierda, 1, 1)
}

#[test]
fn la_codificacion_por_defecto_no_pasa_de_223() {
    let por_defecto = modo(Modo::PulsarSoltar, Codificacion::PorDefecto);
    assert_eq!(
        codificar(&evento(Accion::Pulsar, Boton::Izquierda, 3, 4), por_defecto).unwrap(),
        b"\x1b[M #$"
    );
    assert!(codificar(
        &evento(Accion::Pulsar, Boton::Izquierda, 224, 3),
        por_defecto
    )
    .is_none());
    assert!(codificar(
        &evento(Accion::Pulsar, Boton::Izquierda, 3, 224),
        por_defecto
    )
    .is_none());
    assert!(codificar(
        &evento(Accion::Pulsar, Boton::Izquierda, 223, 223),
        por_defecto
    )
    .is_some());

    // Soltar en la codificación por defecto es siempre el código 3.
    let mut soltar = evento(Accion::Soltar, Boton::Izquierda, 1, 1);
    soltar.modificadores = Modificadores::default();
    let bytes = codificar(&soltar, por_defecto).unwrap();
    assert_eq!(bytes[3], 32 + 3);
    // Las coordenadas llevan 32 + valor (1-based): 33 es '!'.
    assert_eq!(&bytes[4..], b"!!");
}

#[test]
fn utf8_codifica_las_coordenadas_como_caracteres() {
    let utf8 = modo(Modo::PulsarSoltar, Codificacion::Utf8);
    let bytes = codificar(&evento(Accion::Pulsar, Boton::Izquierda, 300, 4), utf8).unwrap();
    assert_eq!(&bytes[..4], b"\x1b[M ");
    // 32 + 300 = 332 = U+014C, y 32 + 4 = 36 = '$'.
    assert_eq!(&bytes[4..], "\u{14c}$".as_bytes());
}

#[test]
fn utf8_admite_hasta_la_columna_2015() {
    let utf8 = modo(Modo::PulsarSoltar, Codificacion::Utf8);
    assert!(codificar(&evento(Accion::Pulsar, Boton::Izquierda, 2015, 1), utf8).is_some());
    assert!(codificar(&evento(Accion::Pulsar, Boton::Izquierda, 2016, 1), utf8).is_none());
    assert!(codificar(&evento(Accion::Pulsar, Boton::Izquierda, 1, 2015), utf8).is_some());
    assert!(codificar(&evento(Accion::Pulsar, Boton::Izquierda, 1, 2016), utf8).is_none());
}

#[test]
fn las_coordenadas_desde_cero_no_se_envian() {
    let todo = modo(Modo::TodoMovimiento, Codificacion::Sgr);
    assert!(codificar(&evento(Accion::Pulsar, Boton::Izquierda, 0, 1), todo).is_none());
    assert!(codificar(&evento(Accion::Pulsar, Boton::Izquierda, 1, 0), todo).is_none());
}

#[test]
fn lee_el_modo_y_la_codificacion_de_la_pantalla() {
    let mut parser = vt100::Parser::new(2, 10, 0);
    let sin_raton = ModoRaton::desde_pantalla(parser.screen());
    assert_eq!(sin_raton.modo, Modo::Ninguno);
    assert!(!sin_raton.pedido());

    parser.process(b"\x1b[?1000h");
    let mil = ModoRaton::desde_pantalla(parser.screen());
    assert_eq!(mil.modo, Modo::PulsarSoltar);
    assert!(mil.pedido());

    parser.process(b"\x1b[?1002;1006h");
    let dos_mil = ModoRaton::desde_pantalla(parser.screen());
    assert_eq!(dos_mil.modo, Modo::BotonMovimiento);
    assert_eq!(dos_mil.codificacion, Codificacion::Sgr);

    parser.process(b"\x1b[?1003;1005h");
    let todo = ModoRaton::desde_pantalla(parser.screen());
    assert_eq!(todo.modo, Modo::TodoMovimiento);
    assert_eq!(todo.codificacion, Codificacion::Utf8);

    parser.process(b"\x1b[?1003l\x1b[?1006l\x1b[?1005l");
    let apagado = ModoRaton::desde_pantalla(parser.screen());
    assert_eq!(apagado.modo, Modo::Ninguno);
    assert_eq!(apagado.codificacion, Codificacion::PorDefecto);
}
