//! Regresiones de la revisión adversarial de cierre de la Fase 7: cada prueba
//! reproduce un hallazgo confirmado (ver magi-fase7-implementacion.md).

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};

use magi::app::Dialogo;
use magi::protocolo::{MensajeCliente, MensajeServidor};
use magi::tema::Tema;
use magi::ui::componentes::CampoTexto;
use magi::ui::Vista;

use crate::arnes::{sesion, tema_ascii, AppPrueba};

fn lanzo_ejecucion(enviados: &[MensajeCliente]) -> bool {
    enviados
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeCliente::LanzarEjecucion { .. }))
}

/// Pérdida de datos: con el aviso de tamaño, una «q» tecleada a ciegas en la
/// ficha con cambios ya no se convierte en Esc (la «s» o el ↵ siguientes
/// confirmaban el descarte sin que el usuario lo viera).
#[test]
fn la_q_a_ciegas_no_descarta_la_ficha_con_cambios() {
    for confirmar in [KeyCode::Char('s'), KeyCode::Enter] {
        let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
        prueba.tecla(KeyCode::F(2));
        prueba.tecla(KeyCode::Down);
        prueba.tecla(KeyCode::Char('e'));
        assert_eq!(prueba.app.vista, Vista::Ficha);
        prueba.tecla(KeyCode::Char('x'));
        assert!(prueba.app.ficha.as_ref().is_some_and(|ficha| ficha.sucio()));
        prueba.redimensionar(48, 20);
        assert!(prueba.app.disposicion().aviso);
        prueba.tecla(KeyCode::Char('q'));
        prueba.tecla(confirmar);
        assert_eq!(prueba.app.vista, Vista::Ficha, "{confirmar:?}");
        assert!(prueba.app.ficha.as_ref().is_some_and(|ficha| ficha.sucio()));
        assert!(prueba.app.dialogo.is_none());
        // Sin cambios, `q` sigue siendo salir.
        prueba.redimensionar(80, 24);
    }
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('e'));
    prueba.redimensionar(48, 20);
    prueba.tecla(KeyCode::Char('q'));
    assert_eq!(prueba.app.vista, Vista::Hosts);
}

/// En Sesión, con la deliberación tapada por el aviso, Ctrl+K no ejecuta: solo
/// vale Esc, que la cancela.
#[test]
fn en_sesion_el_aviso_no_deja_ejecutar_la_deliberacion_tapada() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.app.deliberacion = Some(crate::vistas_f::deliberacion_larga(3));
    prueba.redimensionar(45, 10);
    let texto = prueba.texto();
    assert!(texto.contains("diálogo MAGI necesita 50×12"), "{texto}");
    prueba.enviados();
    prueba.tecla_con(KeyCode::Char('k'), KeyModifiers::CONTROL);
    prueba.tecla(KeyCode::Char('f'));
    assert!(!lanzo_ejecucion(&prueba.enviados()));
    assert!(prueba.app.deliberacion.is_some());
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.deliberacion.is_none());
    assert!(!lanzo_ejecucion(&prueba.enviados()));
}

/// La paleta encogida a la línea de la consulta no ejecuta con ↵ una entrada
/// que no se ve.
#[test]
fn la_paleta_sin_entradas_a_la_vista_no_ejecuta() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.tecla(KeyCode::F(2));
    prueba.redimensionar(60, 5);
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(prueba.app.paleta.is_some());
    for caracter in "disco backup".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.enviados();
    prueba.tecla(KeyCode::Enter);
    assert!(!lanzo_ejecucion(&prueba.enviados()));
    assert!(prueba.app.paleta.is_some(), "la paleta sigue abierta");
    // Con sitio, la misma entrada sí se ejecuta.
    prueba.redimensionar(80, 24);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.paleta.is_none());
}

/// Ida y vuelta al mismo tamaño dentro de la agrupación: el terminal truncó y
/// volvió a crecer, así que se limpia y se repinta una vez, sin avisar al
/// remoto.
#[test]
fn una_ida_y_vuelta_repinta_sin_avisar_al_remoto() {
    let mut prueba = AppPrueba::nueva(120, 40);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.enviados();
    let limpiezas = prueba.terminal.backend().limpiezas;
    let t0 = Instant::now();
    prueba.redimensionar_en(90, 30, t0);
    prueba.redimensionar_en(120, 40, t0 + Duration::from_millis(10));
    prueba.paso_en(t0 + Duration::from_millis(70));
    assert_eq!(prueba.terminal.backend().limpiezas, limpiezas + 1);
    assert!(prueba.redimensionares().is_empty());
    assert_eq!(prueba.app.geometria().aplicado(), (120, 40));
}

/// La pregunta de contraseña deja el host a la vista aunque el cuerpo se
/// desplace hasta el campo (Sesión a 40×8, su mínimo).
#[test]
fn la_contrasena_dice_de_que_host_es() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.servidor(MensajeServidor::PideContrasena {
        sesion_id: 1,
        host: "hetzner-02".to_string(),
        intento: 1,
        recordar_por_defecto: false,
    });
    for (cols, filas) in [(40, 8), (50, 8), (80, 24)] {
        prueba.redimensionar(cols, filas);
        let texto = prueba.texto();
        assert!(texto.contains("hetzner-02"), "a {cols}×{filas}:\n{texto}");
    }
}

/// El aviso de un túnel que escucha fuera de 127.0.0.1 se ve siempre en su
/// mínimo, sea cual sea el campo con foco.
#[test]
fn el_aviso_del_tunel_expuesto_se_ve_en_su_minimo() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.tecla(KeyCode::F(6));
    prueba.tecla(KeyCode::Char('n'));
    let Some(Dialogo::Tunel { estado }) = prueba.app.dialogo.as_mut() else {
        panic!("se esperaba el diálogo de túnel");
    };
    estado.escucha_direccion = CampoTexto::nuevo("0.0.0.0");
    estado.escucha_puerto = CampoTexto::nuevo("8080");
    prueba.redimensionar(50, 12);
    for _ in 0..8 {
        let texto = prueba.texto();
        assert!(texto.contains("red podrá"), "{texto}");
        prueba.tecla(KeyCode::Tab);
    }
}

/// Generar clave: el aviso de «sin frase» y las casillas que `^s` aplica se
/// ven en el mínimo de Identidades con el foco en cualquier campo.
#[test]
fn generar_clave_ensena_lo_que_aplica_ctrl_s() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.tecla(KeyCode::F(5));
    prueba.tecla(KeyCode::Char('n'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::GenerarClave { .. })
    ));
    for (cols, filas) in [(50, 12), (60, 12)] {
        prueba.redimensionar(cols, filas);
        for _ in 0..6 {
            let texto = prueba.texto();
            assert!(texto.contains("sin cifrar"), "a {cols}×{filas}:\n{texto}");
            assert!(texto.contains("agente"), "a {cols}×{filas}:\n{texto}");
            prueba.tecla(KeyCode::Tab);
        }
    }
}

/// Formulario de snippet en terminales bajas: la opción resaltada del
/// desplegable de hosts se ve, y la pregunta de descartar no desaparece.
#[test]
fn formulario_bajo_ensena_la_resaltada_y_la_pregunta() {
    let mut prueba = crate::vistas_f::abrir_formulario(Tema::respaldo());
    // Nombre, comando, descripción, etiquetas, destinos por etiqueta y hosts.
    for _ in 0..5 {
        prueba.tecla(KeyCode::Tab);
    }
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    for filas in [10, 11] {
        prueba.redimensionar(60, filas);
        let texto = prueba.texto();
        assert!(texto.contains('▸'), "a 60×{filas}:\n{texto}");
    }

    // Con cambios, Esc pregunta; con una sola fila de texto la pregunta se ve.
    let mut prueba = crate::vistas_f::abrir_formulario(Tema::respaldo());
    prueba.tecla(KeyCode::Char('x'));
    prueba.tecla(KeyCode::Esc);
    prueba.redimensionar(60, 7);
    let texto = prueba.texto();
    assert!(texto.contains("Descartar"), "{texto}");
}

/// Modo ASCII: el error de EJECUTAR (con «») y los diálogos de `modal()` se
/// pintan sin glifos Unicode.
#[test]
fn ejecutar_en_ascii_degrada_el_error() {
    let mut prueba = crate::vistas_f::abrir_ejecutar(tema_ascii(), "espacio en disco");
    prueba.tecla(KeyCode::Enter);
    let texto = prueba.texto();
    assert!(
        texto.chars().all(|c| c.is_ascii() || c.is_alphabetic()),
        "{texto}"
    );
}
