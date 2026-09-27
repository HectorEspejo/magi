//! Barrido sin pánico: todas las vistas, la paleta, la ayuda, el diálogo MAGI,
//! EJECUTAR, el formulario de snippet y todas las variantes de diálogo pasan
//! por una rejilla de tamaños que incluye los puntos de corte, los mínimos y
//! áreas degeneradas. En cada tamaño: ningún pánico, pintado al tamaño nuevo y
//! la selección de toda lista a la vista y sin huecos al final.

use crossterm::event::{KeyCode, KeyModifiers};

use magi::tema::Tema;
use magi::ui::Vista;

use crate::arnes::{sesion, tema_ascii, AppPrueba};

const ANCHOS: [u16; 13] = [1, 2, 19, 20, 39, 40, 59, 60, 79, 80, 99, 100, 200];
const ALTOS: [u16; 12] = [1, 2, 3, 7, 8, 11, 12, 19, 20, 23, 24, 60];

fn recorrer(prueba: &mut AppPrueba, que: &str) {
    for cols in ANCHOS {
        for filas in ALTOS {
            prueba.pasar_por(cols, filas);
        }
    }
    // Y de vuelta a un tamaño normal: se sigue viendo lo mismo.
    prueba.pasar_por(80, 24);
    assert!(!prueba.texto().trim().is_empty(), "{que} en blanco a 80×24");
}

/// Lleva la App recién creada a una vista.
type Abrir = fn(&mut AppPrueba);

/// Abre una vista a 80×24 con la semilla.
fn vista(tema: Tema, abrir: Abrir) -> AppPrueba {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(80, 24, tema);
    let _ = &sembrado;
    abrir(&mut prueba);
    prueba
}

const VISTAS: [(&str, Abrir); 12] = [
    ("flota", |_| {}),
    ("hosts", |p| p.tecla(KeyCode::F(2))),
    ("ficha", |p| {
        p.tecla(KeyCode::F(2));
        p.tecla(KeyCode::Down);
        p.tecla(KeyCode::Char('e'));
        assert_eq!(p.app.vista, Vista::Ficha);
    }),
    ("sesiones", |p| {
        p.bienvenida(vec![sesion(1, "hetzner-01"), sesion(2, "mac-mini-m1")]);
        p.tecla(KeyCode::F(3));
        assert_eq!(p.app.vista, Vista::Sesiones);
    }),
    ("sesion", |p| {
        p.entrar_en_sesion(vec![sesion(1, "hetzner-01"), sesion(2, "mac-mini-m1")]);
    }),
    ("identidades", |p| p.tecla(KeyCode::F(5))),
    ("tuneles", |p| p.tecla(KeyCode::F(6))),
    ("registro", |p| p.tecla(KeyCode::F(7))),
    ("snippets", |p| p.tecla(KeyCode::F(8))),
    ("resultados", |p| {
        p.tecla(KeyCode::F(8));
        p.tecla(KeyCode::Char('t'));
        assert_eq!(p.app.vista, Vista::Resultados);
    }),
    ("paleta", |p| {
        p.tecla(KeyCode::F(2));
        p.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert!(p.app.paleta.is_some());
    }),
    ("ayuda", |p| {
        p.tecla(KeyCode::F(2));
        p.tecla(KeyCode::Char('?'));
        assert!(p.app.ayuda);
    }),
];

#[test]
fn todas_las_vistas_sin_panico_en_la_rejilla() {
    for (nombre, abrir) in VISTAS {
        let mut prueba = vista(Tema::respaldo(), abrir);
        recorrer(&mut prueba, nombre);
    }
}

#[test]
fn todas_las_vistas_sin_panico_en_ascii() {
    for (nombre, abrir) in VISTAS {
        let mut prueba = vista(tema_ascii(), abrir);
        recorrer(&mut prueba, nombre);
    }
}

#[test]
fn archivos_y_transferencias_sin_panico() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    crate::vistas_d::abrir_archivos(&mut prueba, &sembrado);
    recorrer(&mut prueba, "archivos");
    prueba.tecla(KeyCode::Char('t'));
    assert_eq!(prueba.app.vista, Vista::Transferencias);
    recorrer(&mut prueba, "transferencias");
}

#[test]
fn dialogo_magi_ejecutar_y_formulario_sin_panico() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    prueba.tecla(KeyCode::F(8));
    prueba.app.deliberacion = Some(crate::vistas_f::deliberacion_maqueta(&sembrado));
    recorrer(&mut prueba, "diálogo MAGI");

    let mut prueba = crate::vistas_f::abrir_ejecutar(Tema::respaldo(), "espacio en disco");
    recorrer(&mut prueba, "EJECUTAR");

    let mut prueba = crate::vistas_f::abrir_formulario(Tema::respaldo());
    recorrer(&mut prueba, "formulario de snippet");

    let (mut prueba, _) = crate::vistas_f::abrir_resultados(Tema::respaldo());
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Enter);
    recorrer(&mut prueba, "visor de salida");
}

#[test]
fn todas_las_variantes_de_dialogo_sin_panico() {
    for (titulo, dialogo) in
        crate::vistas_g::todos_los_dialogos(crate::vistas_g::hosts_de_la_semilla())
    {
        let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
        prueba.tecla(KeyCode::F(2));
        crate::vistas_g::abrir_dialogo(&mut prueba, dialogo());
        recorrer(&mut prueba, titulo);
        assert!(
            prueba.app.dialogo.is_some(),
            "{titulo}: el diálogo se cerró solo"
        );
    }
}
