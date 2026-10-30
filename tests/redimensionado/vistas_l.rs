//! Diálogo SERVIDOR DE SESIONES ANTERIOR (Fase 9, §6.2): instantáneas a
//! 40×12, 80×24 y 200×60 (T45), modo estrecho, ASCII sin glifos Unicode y el
//! camino de teclas (p → confirmación → n/esc, q, error y sustitución en
//! curso).

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;

use magi::app::Evento;

use magi::app::servidor_anterior::DialogoServidorAnterior as Estado;
use magi::app::Dialogo;
use magi::cliente::RechazoVersion;
use magi::protocolo::TrabajoServidor;
use magi::ui::disposicion::{Tamano, MINIMO_SERVIDOR_ANTERIOR};

use crate::arnes::{self, AppPrueba};

/// Los tres tamaños de referencia de la fase.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

/// Rechazo de un servidor anterior con trabajo y una ventana más.
fn rechazo_con_trabajo() -> RechazoVersion {
    let mut rechazo = RechazoVersion::nueva(5);
    rechazo.version_app = Some("0.9.0".to_string());
    rechazo.pid = Some(48211);
    rechazo.clientes = Some(1);
    rechazo.trabajo = Some(TrabajoServidor {
        sesiones: 2,
        transferencias: 0,
        tuneles: 1,
        ejecuciones: 0,
    });
    rechazo
}

/// Rechazo de un servidor v5: no informa de lo que custodia.
fn rechazo_sin_recuento() -> RechazoVersion {
    let mut rechazo = RechazoVersion::nueva(5);
    rechazo.pid = Some(48211);
    rechazo
}

fn abrir(prueba: &mut AppPrueba, estado: Estado) {
    // Como el flujo real: el rechazo queda fijado en la App (la barra avisa
    // mientras el servidor anterior siga ahí) y el diálogo se abre encima.
    prueba.app.servidor_incompatible = Some(estado.rechazo().clone());
    prueba.app.abrir_dialogo(Dialogo::ServidorAnterior(estado));
    prueba.paso_en(Instant::now());
}

/// Instantánea como la del arnés.
fn instantanea(prueba: &AppPrueba, nombre: &str) {
    let texto = prueba.texto();
    let mut ajustes = insta::Settings::clone_current();
    ajustes.set_snapshot_path("../snapshots");
    ajustes.set_prepend_module_to_snapshot(false);
    ajustes.set_omit_expression(true);
    ajustes.add_filter(r"/tmp/magi[A-Za-z0-9]{6}", "[tmp]");
    // El pie de Flota cuenta segundos reales: mismo ancho, sin el número.
    ajustes.add_filter(r"hace \d\d s", "hace NN s");
    ajustes.add_filter(r"hace \d s", "hace N s");
    ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
}

/// Lo pintado dentro del recuadro del diálogo, para mirar solo su texto.
fn dentro_del_recuadro(texto: &str, ascii: bool) -> String {
    let (izquierda, derecha, abajo) = if ascii {
        ('+', '+', '+')
    } else {
        ('┌', '┐', '└')
    };
    let lineas: Vec<&str> = texto.lines().collect();
    let titulo = format!("{izquierda} SERVIDOR DE SESIONES ANTERIOR");
    let y0 = lineas
        .iter()
        .position(|linea| linea.contains(&titulo))
        .unwrap_or_else(|| panic!("sin recuadro del diálogo:\n{texto}"));
    let x0 = lineas[y0].find(izquierda).expect("esquina izquierda");
    let x1 = lineas[y0].rfind(derecha).expect("esquina derecha");
    let mut salida = Vec::new();
    for linea in &lineas[y0..] {
        let caracteres: Vec<char> = linea.chars().collect();
        if caracteres.len() <= x0 {
            break;
        }
        let fin = (x1 + 1).min(caracteres.len());
        salida.push(caracteres[x0..fin].iter().collect::<String>());
        if caracteres[x0] == abajo {
            break;
        }
    }
    salida.join("\n")
}

// ---------------------------------------------------------------- instantáneas

#[test]
fn instantaneas_del_aviso_con_trabajo() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(&mut prueba, Estado::Aviso(rechazo_con_trabajo()));
        instantanea(&prueba, &format!("servidor_anterior_aviso_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_del_aviso_sin_recuento() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(&mut prueba, Estado::Aviso(rechazo_sin_recuento()));
        instantanea(
            &prueba,
            &format!("servidor_anterior_sin_recuento_{cols}x{filas}"),
        );
    }
}

#[test]
fn instantaneas_de_la_confirmacion() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(&mut prueba, Estado::Confirmacion(rechazo_con_trabajo()));
        instantanea(
            &prueba,
            &format!("servidor_anterior_confirmacion_{cols}x{filas}"),
        );
    }
}

// ---------------------------------------------------------------- disposición

#[test]
fn el_minimo_es_40x12_y_entra_en_el_de_la_pantalla() {
    assert_eq!(MINIMO_SERVIDOR_ANTERIOR, Tamano::new(40, 12));
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    abrir(&mut prueba, Estado::Aviso(rechazo_con_trabajo()));
    let disposicion = prueba.app.disposicion();
    assert!(!disposicion.aviso, "a 40×12 no debe salir el aviso");
    assert_eq!(
        disposicion.minimo.map(|minimo| minimo.tamano),
        Some(Tamano::new(40, 12))
    );
}

#[test]
fn en_estrecho_la_custodia_va_una_por_renglon() {
    // 55 columnas: por debajo de ESTRECHO_SERVIDOR_ANTERIOR y con sitio para
    // ver las líneas sin que el cuerpo se desplace.
    let (mut prueba, _) = AppPrueba::con_semilla(55, 20);
    abrir(&mut prueba, Estado::Aviso(rechazo_con_trabajo()));
    let texto = prueba.texto();
    assert!(texto.contains("Custodia:  2 sesiones"), "{texto}");
    assert!(texto.contains("1 túnel"), "{texto}");
    assert!(!texto.contains("2 sesiones · 1 túnel"), "{texto}");
}

#[test]
fn en_ancho_la_custodia_va_de_dos_en_dos() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(&mut prueba, Estado::Aviso(rechazo_con_trabajo()));
    let texto = prueba.texto();
    assert!(texto.contains("Custodia:  2 sesiones · 1 túnel"), "{texto}");
    assert!(
        texto.contains("0 transferencias · 0 ejecuciones"),
        "{texto}"
    );
    assert!(
        texto.contains("1 ventana más de la versión anterior"),
        "{texto}"
    );
}

#[test]
fn el_servidor_v5_dice_que_no_informa_de_lo_que_custodia() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(&mut prueba, Estado::Aviso(rechazo_sin_recuento()));
    let texto = prueba.texto();
    assert!(
        texto.contains("la versión anterior no informa de lo que custodia"),
        "{texto}"
    );
}

#[test]
fn en_ascii_no_hay_glifos_unicode() {
    for (cols, filas) in TAMANOS {
        for estado in [
            Estado::Aviso(rechazo_con_trabajo()),
            Estado::Aviso(rechazo_sin_recuento()),
            Estado::Confirmacion(rechazo_con_trabajo()),
            Estado::Error {
                rechazo: rechazo_con_trabajo(),
                motivo: "el servidor anterior (pid 48211) no ha liberado el socket y el lock en 5 s; no se ha arrancado otro servidor".to_string(),
                anterior_parado: false,
            },
        ] {
            let (mut prueba, _) = AppPrueba::con_semilla_y_tema(cols, filas, arnes::tema_ascii());
            abrir(&mut prueba, estado);
            let dentro = dentro_del_recuadro(&prueba.texto(), true);
            assert!(
                dentro.chars().all(|caracter| caracter.is_ascii() || caracter.is_alphabetic()),
                "glifo Unicode a {cols}×{filas}:\n{dentro}"
            );
        }
    }
}

// ---------------------------------------------------------------- teclas

#[test]
fn p_pide_confirmacion_n_vuelve_y_esc_cierra_sin_parar_nada() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(&mut prueba, Estado::Aviso(rechazo_con_trabajo()));
    prueba.tecla(KeyCode::Char('p'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ServidorAnterior(Estado::Confirmacion(_)))
    ));
    prueba.tecla(KeyCode::Char('n'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ServidorAnterior(Estado::Aviso(_)))
    ));
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none(), "esc debe cerrar el diálogo");
    assert!(
        prueba.app.servidor_incompatible.is_some(),
        "el aviso de la barra sigue mientras el servidor anterior siga ahí"
    );
    assert!(!prueba.app.salir);
}

#[test]
fn q_sale_de_magi() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(&mut prueba, Estado::Aviso(rechazo_con_trabajo()));
    prueba.tecla(KeyCode::Char('q'));
    assert!(prueba.app.salir);
}

#[test]
fn mientras_sustituye_se_ignora_el_teclado() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(&mut prueba, Estado::Sustituyendo(rechazo_con_trabajo()));
    for codigo in [
        KeyCode::Char('q'),
        KeyCode::Esc,
        KeyCode::Char('p'),
        KeyCode::Enter,
    ] {
        prueba.tecla(codigo);
        assert!(matches!(
            prueba.app.dialogo,
            Some(Dialogo::ServidorAnterior(Estado::Sustituyendo(_)))
        ));
    }
    assert!(!prueba.app.salir);
}

/// Deja correr el bucle hasta que se cumpla la condición (el resultado de la
/// sustitución llega por el canal de eventos).
fn esperar(prueba: &mut AppPrueba, que: &str, hasta: impl Fn(&AppPrueba) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !hasta(prueba) {
        assert!(Instant::now() < limite, "sin {que}:\n{}", prueba.texto());
        prueba
            .app
            .bombear(&mut prueba.terminal, Duration::from_millis(20))
            .expect("bombeando");
    }
}

/// `s` confirma la parada y lanza la sustitución: el diálogo pasa a
/// «sustituyendo» y el resultado llega por el canal de eventos (aquí falla al
/// momento porque el rechazo no trae pid).
#[test]
fn s_confirma_y_el_resultado_llega_al_dialogo() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    let mut rechazo = rechazo_con_trabajo();
    rechazo.pid = None;
    abrir(&mut prueba, Estado::Aviso(rechazo));
    prueba.tecla(KeyCode::Char('p'));
    prueba.tecla(KeyCode::Char('s'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ServidorAnterior(Estado::Sustituyendo(_)))
    ));
    esperar(&mut prueba, "el error de la sustitución", |prueba| {
        matches!(
            prueba.app.dialogo,
            Some(Dialogo::ServidorAnterior(Estado::Error { .. }))
        )
    });
    let texto = prueba.texto();
    assert!(
        texto.contains("No se pudo sustituir el servidor anterior"),
        "{texto}"
    );
    assert!(texto.contains("magi servidor parar"), "{texto}");
}

/// El resultado bueno actualiza la conexión, cierra el diálogo y lo dice en la
/// barra («servidor de sesiones actualizado a …»).
#[test]
fn el_resultado_bueno_cierra_el_dialogo_y_lo_dice_en_la_barra() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(&mut prueba, Estado::Sustituyendo(rechazo_con_trabajo()));
    let (cliente, _enviados) = magi::cliente::Cliente::de_prueba();
    prueba.evento(Evento::Sustitucion(Ok(cliente)));
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.app.servidor_incompatible.is_none());
    let texto = prueba.texto();
    // La versión nueva es la de este binario, no la del servidor rechazado.
    let esperado = format!("servidor de sesiones actualizado a {}", magi::VERSION_APP);
    assert!(texto.contains(&esperado), "{texto}");
}

#[test]
fn el_error_ensena_el_motivo_y_esc_sigue_sin_sesiones() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(
        &mut prueba,
        Estado::Error {
            rechazo: rechazo_con_trabajo(),
            motivo: "el servidor anterior (pid 48211) no ha liberado el socket y el lock en 5 s"
                .to_string(),
            anterior_parado: false,
        },
    );
    let texto = prueba.texto();
    assert!(
        texto.contains("No se pudo sustituir el servidor anterior"),
        "{texto}"
    );
    assert!(texto.contains("pid 48211"), "{texto}");
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.app.servidor_incompatible.is_some());
}

/// Si el servidor anterior ya se paró y el nuevo no arrancó, el diálogo no
/// puede decir que el anterior sigue en marcha.
#[test]
fn el_error_de_servidor_parado_sin_nuevo_lo_dice_asi() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(
        &mut prueba,
        Estado::Error {
            rechazo: rechazo_con_trabajo(),
            motivo: "el servidor de sesiones nuevo no ha abierto el socket en 10 s".to_string(),
            anterior_parado: true,
        },
    );
    let texto = prueba.texto();
    assert!(
        // El texto se parte al ancho; se comprueba un fragmento que cabe.
        texto.contains("se paró, pero el nuevo"),
        "{texto}"
    );
    assert!(!texto.contains("sigue en marcha"), "{texto}");
}
