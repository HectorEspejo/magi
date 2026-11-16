//! Pruebas de la tabla de teclas del terminal (Fase 16): `modifyOtherKeys`.
//!
//! La más importante protege la codificación clásica: con `modifyOtherKeys`
//! apagado, la tabla de teclas no cambia ni un byte respecto a la que dejó la
//! Fase 15. La tabla de referencia está en
//! `tests/fijos/terminal/teclas_clasicas.txt` (una línea por tecla y modo, con
//! los bytes en hexadecimal de las ocho combinaciones de Mayús, Alt y Ctrl).
//! Se regenera a propósito con `MAGI_ESCRIBIR_TABLA=1`; no la regeneres para
//! hacer pasar una prueba que falla.

use magi_nucleo::terminal::emulador::PantallaEmulada;
use magi_nucleo::terminal::teclas::{codificar, Modificadores, ModoTerminal, Tecla};

fn mods(mayusculas: bool, alt: bool, ctrl: bool) -> Modificadores {
    Modificadores {
        mayusculas,
        alt,
        ctrl,
    }
}

/// Las ocho combinaciones de Mayús (1), Alt (2) y Ctrl (4), en ese orden.
fn combinaciones() -> Vec<Modificadores> {
    (0..8u8)
        .map(|bits| mods(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn teclas_de_la_tabla() -> Vec<Tecla> {
    let mut teclas: Vec<Tecla> = [
        'a', 'A', 'z', 'ñ', ' ', '@', '[', '\\', ']', '^', '_', '?', '0', '1', '2', '3', '4', '5',
        '6', '7', '8', '9', '.', '日',
    ]
    .into_iter()
    .map(Tecla::Caracter)
    .collect();
    teclas.extend([
        Tecla::Intro,
        Tecla::Tabulador,
        Tecla::TabuladorInverso,
        Tecla::Retroceso,
        Tecla::Escape,
        Tecla::Arriba,
        Tecla::Abajo,
        Tecla::Derecha,
        Tecla::Izquierda,
        Tecla::Inicio,
        Tecla::Fin,
        Tecla::RePagina,
        Tecla::AvPagina,
        Tecla::Suprimir,
        Tecla::Insertar,
        Tecla::F(0),
        Tecla::F(1),
        Tecla::F(4),
        Tecla::F(5),
        Tecla::F(12),
        Tecla::F(13),
    ]);
    teclas
}

/// Los modos de la Fase 15 (sin `modifyOtherKeys`): normal, cursor de
/// aplicación, pegado entre corchetes y la bandera 1 de kitty.
fn modos_clasicos() -> Vec<(&'static str, ModoTerminal)> {
    vec![
        ("normal", ModoTerminal::default()),
        (
            "aplicacion",
            ModoTerminal {
                cursor_aplicacion: true,
                ..ModoTerminal::default()
            },
        ),
        (
            "pegado",
            ModoTerminal {
                pegado_corchetes: true,
                ..ModoTerminal::default()
            },
        ),
        (
            "kitty",
            ModoTerminal {
                banderas_kitty: 1,
                ..ModoTerminal::default()
            },
        ),
    ]
}

/// Vuelca la tabla: por cada tecla y modo, los bytes de las ocho combinaciones
/// (`-` si no hay codificación).
fn volcar_tabla() -> String {
    let mut salida = String::new();
    for tecla in teclas_de_la_tabla() {
        for (nombre, modo) in modos_clasicos() {
            salida.push_str(&format!("{tecla:?} {nombre}:"));
            for modificadores in combinaciones() {
                match codificar(tecla, modificadores, modo) {
                    Some(bytes) => salida.push_str(&format!(" {}", hex(&bytes))),
                    None => salida.push_str(" -"),
                }
            }
            salida.push('\n');
        }
    }
    salida
}

const RUTA_TABLA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fijos/terminal/teclas_clasicas.txt"
);

#[test]
fn sin_modify_other_keys_la_tabla_clasica_no_cambia_ni_un_byte() {
    let actual = volcar_tabla();
    if std::env::var_os("MAGI_ESCRIBIR_TABLA").is_some() {
        std::fs::create_dir_all(std::path::Path::new(RUTA_TABLA).parent().unwrap())
            .expect("directorio de la tabla");
        std::fs::write(RUTA_TABLA, &actual).expect("escribir la tabla");
        return;
    }
    let esperada = std::fs::read_to_string(RUTA_TABLA).expect("tabla de referencia");
    // Se compara línea a línea para que el fallo diga cuál cambió.
    for (numero, (esperada, actual)) in esperada.lines().zip(actual.lines()).enumerate() {
        assert_eq!(
            actual,
            esperada,
            "cambió la línea {} de la tabla clásica",
            numero + 1
        );
    }
    assert_eq!(esperada.lines().count(), actual.lines().count());
}

// ---------------------------------------------------------------- modifyOtherKeys

fn mok(nivel: u8) -> ModoTerminal {
    ModoTerminal {
        modify_other_keys: nivel,
        ..ModoTerminal::default()
    }
}

fn con(tecla: Tecla, modificadores: Modificadores, modo: ModoTerminal) -> Vec<u8> {
    codificar(tecla, modificadores, modo).expect("la tecla tiene codificación")
}

#[test]
fn mayus_intro_con_modify_other_keys_es_esc_27_2_13_tilde() {
    // Criterio de aceptación del checklist: exactamente ESC [ 2 7 ; 2 ; 1 3 ~.
    for nivel in [1, 2] {
        assert_eq!(
            con(Tecla::Intro, mods(true, false, false), mok(nivel)),
            [0x1b, b'[', b'2', b'7', b';', b'2', b';', b'1', b'3', b'~']
        );
    }
}

#[test]
fn ctrl_intro_y_las_combinaciones_de_intro_se_codifican_como_xterm() {
    let casos = [
        (mods(false, false, true), "\x1b[27;5;13~"),
        (mods(true, false, true), "\x1b[27;6;13~"),
        (mods(true, true, false), "\x1b[27;4;13~"),
        (mods(false, true, true), "\x1b[27;7;13~"),
        (mods(true, true, true), "\x1b[27;8;13~"),
    ];
    for nivel in [1, 2] {
        for (modificadores, esperada) in casos {
            assert_eq!(
                con(Tecla::Intro, modificadores, mok(nivel)),
                esperada.as_bytes()
            );
        }
    }
}

#[test]
fn alt_intro_solo_se_queda_como_esc_cr_y_sin_modificadores_no_cambia() {
    for nivel in [1, 2] {
        assert_eq!(
            con(Tecla::Intro, mods(false, true, false), mok(nivel)),
            b"\x1b\r"
        );
        assert_eq!(
            con(Tecla::Intro, Modificadores::default(), mok(nivel)),
            b"\r"
        );
    }
}

#[test]
fn tab_retroceso_y_esc_con_mayus_o_ctrl_usan_la_forma_27() {
    for nivel in [1, 2] {
        let modo = mok(nivel);
        assert_eq!(
            con(Tecla::Tabulador, mods(false, false, true), modo),
            b"\x1b[27;5;9~"
        );
        assert_eq!(
            con(Tecla::Tabulador, mods(true, true, false), modo),
            b"\x1b[27;4;9~"
        );
        assert_eq!(
            con(Tecla::Retroceso, mods(true, false, false), modo),
            b"\x1b[27;2;127~"
        );
        assert_eq!(
            con(Tecla::Retroceso, mods(false, false, true), modo),
            b"\x1b[27;5;127~"
        );
        assert_eq!(
            con(Tecla::Escape, mods(true, false, false), modo),
            b"\x1b[27;2;27~"
        );
        assert_eq!(
            con(Tecla::Escape, mods(false, false, true), modo),
            b"\x1b[27;5;27~"
        );
        // Solo Alt se queda en la forma clásica (Esc delante).
        assert_eq!(
            con(Tecla::Tabulador, mods(false, true, false), modo),
            b"\x1b\t"
        );
        assert_eq!(
            con(Tecla::Retroceso, mods(false, true, false), modo),
            b"\x1b\x7f"
        );
        assert_eq!(
            con(Tecla::Escape, mods(false, true, false), modo),
            b"\x1b\x1b"
        );
        // Sin modificadores, nada cambia.
        assert_eq!(con(Tecla::Tabulador, Modificadores::default(), modo), b"\t");
        assert_eq!(
            con(Tecla::Retroceso, Modificadores::default(), modo),
            b"\x7f"
        );
        assert_eq!(con(Tecla::Escape, Modificadores::default(), modo), b"\x1b");
    }
}

#[test]
fn mayus_tab_solo_sigue_siendo_csi_z() {
    for nivel in [1, 2] {
        let modo = mok(nivel);
        assert_eq!(
            con(Tecla::TabuladorInverso, mods(true, false, false), modo),
            b"\x1b[Z"
        );
        assert_eq!(
            con(Tecla::Tabulador, mods(true, false, false), modo),
            b"\x1b[Z"
        );
        // Con Ctrl, ya no es la forma clásica: Mayús va en el valor.
        assert_eq!(
            con(Tecla::TabuladorInverso, mods(true, false, true), modo),
            b"\x1b[27;6;9~"
        );
    }
}

#[test]
fn el_nivel_2_codifica_ctrl_con_los_caracteres_de_forma_clasica() {
    // Nivel 1: Ctrl+letra sigue siendo el byte de control (forma conocida).
    assert_eq!(
        con(Tecla::Caracter('a'), mods(false, false, true), mok(1)),
        [0x01]
    );
    // Nivel 2: como xterm.
    let modo = mok(2);
    assert_eq!(
        con(Tecla::Caracter('a'), mods(false, false, true), modo),
        b"\x1b[27;5;97~"
    );
    assert_eq!(
        con(Tecla::Caracter('A'), mods(true, false, true), modo),
        b"\x1b[27;6;65~"
    );
    assert_eq!(
        con(Tecla::Caracter('a'), mods(false, true, true), modo),
        b"\x1b[27;7;97~"
    );
    assert_eq!(
        con(Tecla::Caracter(' '), mods(false, false, true), modo),
        b"\x1b[27;5;32~"
    );
    // Las formas de crossterm para 0x1c-0x1f se codifican con su tecla real.
    assert_eq!(
        con(Tecla::Caracter('5'), mods(false, false, true), modo),
        b"\x1b[27;5;93~"
    );
    assert_eq!(
        con(Tecla::Caracter(']'), mods(false, false, true), modo),
        b"\x1b[27;5;93~"
    );
    // Sin Ctrl, nada cambia: ni Mayús ni Alt con un carácter.
    assert_eq!(
        con(Tecla::Caracter('a'), Modificadores::default(), modo),
        b"a"
    );
    assert_eq!(
        con(Tecla::Caracter('A'), mods(true, false, false), modo),
        b"A"
    );
    assert_eq!(
        con(Tecla::Caracter('x'), mods(false, true, false), modo),
        b"\x1bx"
    );
}

#[test]
fn ctrl_con_un_caracter_sin_forma_clasica_usa_la_forma_27_en_los_dos_niveles() {
    // Sin el modo, Ctrl+0 y Ctrl+ñ no tienen codificación; con él, sí.
    assert_eq!(
        codificar(
            Tecla::Caracter('0'),
            mods(false, false, true),
            ModoTerminal::default()
        ),
        None
    );
    for nivel in [1, 2] {
        let modo = mok(nivel);
        assert_eq!(
            con(Tecla::Caracter('0'), mods(false, false, true), modo),
            b"\x1b[27;5;48~"
        );
        assert_eq!(
            con(Tecla::Caracter('ñ'), mods(false, false, true), modo),
            "\x1b[27;5;241~".as_bytes()
        );
    }
}

#[test]
fn las_flechas_y_las_teclas_de_funcion_no_cambian_con_modify_other_keys() {
    for nivel in [1, 2] {
        let modo = mok(nivel);
        assert_eq!(
            con(Tecla::Arriba, mods(false, false, true), modo),
            b"\x1b[1;5A"
        );
        assert_eq!(
            con(Tecla::Arriba, Modificadores::default(), modo),
            b"\x1b[A"
        );
        assert_eq!(
            con(Tecla::F(5), Modificadores::default(), modo),
            b"\x1b[15~"
        );
        assert_eq!(
            con(Tecla::Suprimir, Modificadores::default(), modo),
            b"\x1b[3~"
        );
    }
}

#[test]
fn con_la_bandera_1_de_kitty_gana_kitty() {
    for nivel in [1, 2] {
        let modo = ModoTerminal {
            modify_other_keys: nivel,
            banderas_kitty: 1,
            ..ModoTerminal::default()
        };
        assert_eq!(
            con(Tecla::Intro, mods(true, false, false), modo),
            b"\x1b[13;2u"
        );
        assert_eq!(
            con(Tecla::Caracter('a'), mods(false, false, true), modo),
            b"\x1b[97;5u"
        );
    }
}

#[test]
fn apagar_el_modo_devuelve_la_codificacion_clasica() {
    let mut pantalla = PantallaEmulada::nueva(24, 80, 0, false);
    pantalla.procesar(b"\x1b[>4;2m");
    let activo = ModoTerminal::desde_emulador(&pantalla);
    assert_eq!(activo.modify_other_keys, 2);
    assert_eq!(
        con(Tecla::Intro, mods(true, false, false), activo),
        b"\x1b[27;2;13~"
    );
    pantalla.procesar(b"\x1b[>4m");
    let apagado = ModoTerminal::desde_emulador(&pantalla);
    assert_eq!(apagado.modify_other_keys, 0);
    assert_eq!(con(Tecla::Intro, mods(true, false, false), apagado), b"\r");
}

#[test]
fn el_modo_es_por_pestana() {
    let mut una = PantallaEmulada::nueva(24, 80, 0, false);
    let otra = PantallaEmulada::nueva(24, 80, 0, false);
    una.procesar(b"\x1b[>4;1m");
    assert_eq!(ModoTerminal::desde_emulador(&una).modify_other_keys, 1);
    assert_eq!(ModoTerminal::desde_emulador(&otra).modify_other_keys, 0);
}
