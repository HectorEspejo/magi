//! Instantáneas y pruebas de la Fase 12: Hosts con escritorios, formulario
//! RDP, ficha de un escritorio, pestaña ▣ de Sesión y diálogos de certificado
//! (nuevo y cambiado) a 40×12, 80×24 y 200×60 (T45).
//!
//! Todo con App::de_prueba: sin hilos, red ni servidor. Los escritorios se
//! abren con lo que difunde el servidor (Escritorios{lista}) y con las
//! preguntas de certificado que manda al cliente solicitante.

use std::time::Instant;

use crossterm::event::KeyCode;

use magi::app::Dialogo;
use magi::escritorio::{
    DatosCertificado, EstadoEscritorio, ImagenEscritorio, InfoEscritorio, TipoCambioCertificado,
    TipoHost,
};
use magi::protocolo::MensajeServidor;
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};
use crate::semilla::EscritoriosSembrados;

/// Los tres tamaños de referencia de la fase.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];
/// La secuencia de cambios que debe conservar el estado.
const SECUENCIA: [(u16, u16); 4] = [(200, 60), (80, 24), (40, 12), (200, 60)];

/// Huella de documentación para el certificado.
const HUELLA: &str = "SHA256:T2qRAb3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7";
const HUELLA_ANTERIOR: &str = "SHA256:Ab3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9";

/// Sesión de escritorio RDP abierta y sin visor (la gracia ya empezada).
fn info_rdp(escritorios: &EscritoriosSembrados) -> InfoEscritorio {
    let ahora = chrono::Utc::now().timestamp();
    InfoEscritorio {
        id: 1,
        host_id: escritorios.rdp,
        nombre: "win-01".to_string(),
        tipo: TipoHost::Rdp,
        estado: EstadoEscritorio::Abierto,
        fase: None,
        motivo: None,
        visor: false,
        sin_visor_desde: Some(ahora - 15),
        ancho: 1920,
        alto: 1080,
        imagen: Some(ImagenEscritorio::H264),
        fps: 58,
        directo: false,
        a_traves_de: Some("hetzner-01".to_string()),
        desde: ahora - 12 * 60 - 30,
        redimensionable: false,
    }
}

/// Instantánea con los filtros de reloj (el tiempo de apertura y la gracia).
fn instantanea(prueba: &AppPrueba, nombre: &str) {
    let texto = prueba.texto();
    let mut ajustes = insta::Settings::clone_current();
    ajustes.set_snapshot_path("../snapshots");
    ajustes.set_prepend_module_to_snapshot(false);
    ajustes.set_omit_expression(true);
    ajustes.add_filter(r"/tmp/magi[A-Za-z0-9]{6}", "[tmp]");
    ajustes.add_filter(r"se cierra en \d+ s", "se cierra en NN s");
    ajustes.add_filter(r"hace \d+ ([sm])", "hace NN $1");
    ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
}

/// Abre la ficha del escritorio RDP de la semilla.
fn abrir_ficha_rdp(prueba: &mut AppPrueba, escritorios: &EscritoriosSembrados) {
    prueba.tecla(KeyCode::F(2));
    prueba.app.seleccionar_host_id(escritorios.rdp);
    prueba.tecla(KeyCode::Char('e'));
    assert_eq!(prueba.app.vista, Vista::Ficha);
}

/// Abre la ficha nueva y cambia el tipo a RDP con el selector.
fn formulario_rdp(prueba: &mut AppPrueba) {
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Char('n'));
    // El orden de campos de un host nuevo empieza por el tipo.
    prueba.tecla(KeyCode::BackTab);
    // SSH → RDP con un espacio en el selector de tipo.
    prueba.tecla(KeyCode::Char(' '));
    assert!(
        prueba
            .app
            .ficha
            .as_ref()
            .is_some_and(|ficha| ficha.tipo == TipoHost::Rdp),
        "el selector no llegó a RDP"
    );
}

/// Abre la pestaña del escritorio como lo haría el servidor: petición y
/// difusión de la lista.
fn abrir_pestana(prueba: &mut AppPrueba, escritorios: &EscritoriosSembrados) {
    prueba.tecla(KeyCode::F(2));
    prueba.app.seleccionar_host_id(escritorios.rdp);
    prueba.tecla(KeyCode::Enter);
    prueba.servidor(MensajeServidor::Escritorios {
        lista: vec![info_rdp(escritorios)],
    });
    assert_eq!(prueba.app.vista, Vista::Sesion);
}

/// Certificado nuevo o cambiado que pide el servidor al cliente solicitante.
fn pedir_certificado(prueba: &mut AppPrueba, tipo: TipoCambioCertificado) {
    let datos = DatosCertificado {
        tipo,
        sujeto: "CN=WIN-01".to_string(),
        caducidad: "3 may 2027".to_string(),
        huella: HUELLA.to_string(),
        anterior: (tipo == TipoCambioCertificado::Cambiado).then(|| HUELLA_ANTERIOR.to_string()),
    };
    prueba.servidor(MensajeServidor::PideHuellaEscritorio {
        peticion_id: magi::protocolo::RANGO_ESCRITORIOS + 1,
        escritorio_id: 1,
        host_id: 1,
        host_nombre: "win-01".to_string(),
        datos,
    });
    assert!(matches!(prueba.app.dialogo, Some(Dialogo::Escritorio(_))));
}

/// Casos de instantánea: nombre y cómo se preparan (con los ids sembrados).
type Caso = (&'static str, fn(&mut AppPrueba, &EscritoriosSembrados));

fn casos() -> Vec<Caso> {
    vec![
        ("hosts_con_escritorios", |prueba, _| {
            prueba.tecla(KeyCode::F(2));
        }),
        ("formulario_rdp", |prueba, _| formulario_rdp(prueba)),
        ("ficha_escritorio", |prueba, escritorios| {
            abrir_ficha_rdp(prueba, escritorios);
        }),
        ("pestana_escritorio", |prueba, escritorios| {
            abrir_pestana(prueba, escritorios);
        }),
        ("certificado_nuevo", |prueba, _| {
            pedir_certificado(prueba, TipoCambioCertificado::Nuevo);
        }),
        ("certificado_cambiado", |prueba, _| {
            pedir_certificado(prueba, TipoCambioCertificado::Cambiado);
        }),
    ]
}

// ---------------------------------------------------------------- instantáneas

#[test]
fn instantaneas_a_los_tres_tamanos() {
    for (base, preparar) in casos() {
        for (cols, filas) in TAMANOS {
            // Se prepara al tamaño mínimo que deja usar las teclas (la ficha
            // nueva no se puede manejar por debajo de su mínimo) y luego se
            // lleva al tamaño de la instantánea.
            let (mut prueba, _, escritorios) = AppPrueba::con_semilla_de_escritorios(80, 24);
            preparar(&mut prueba, &escritorios);
            prueba.pasar_por(cols, filas);
            instantanea(&prueba, &format!("{base}_{cols}x{filas}"));
        }
    }
}

/// Todo se recoloca en la secuencia 200×60 → 80×24 → 40×12 → 200×60.
#[test]
fn todo_se_recoloca_en_la_secuencia() {
    for (base, preparar) in casos() {
        let (mut prueba, _, escritorios) = AppPrueba::con_semilla_de_escritorios(200, 60);
        preparar(&mut prueba, &escritorios);
        for (cols, filas) in SECUENCIA {
            prueba.pasar_por(cols, filas);
            assert!(!prueba.texto().trim().is_empty(), "{base} en blanco");
            if base.starts_with("certificado") {
                assert!(
                    prueba.app.dialogo.is_some(),
                    "{base}: el diálogo se cerró solo"
                );
            }
        }
    }
}

// -------------------------------------------------------------------- pruebas

/// Hosts: ▣, tipo, ⤴ y ⚠ directo, con texto en modo ASCII.
#[test]
fn hosts_ensena_el_tipo_el_salto_y_el_vnc_directo() {
    let (mut prueba, _, _) = AppPrueba::con_semilla_de_escritorios(200, 60);
    prueba.tecla(KeyCode::F(2));
    let texto = prueba.texto();
    let linea_rdp = texto
        .lines()
        .find(|linea| linea.contains("win-01"))
        .expect("sin fila de win-01");
    assert!(linea_rdp.contains('▣'), "{texto}");
    assert!(linea_rdp.contains("RDP"), "{texto}");
    assert!(linea_rdp.contains("hetzner-01"), "{texto}");
    let linea_vnc = texto
        .lines()
        .find(|linea| linea.contains("nas"))
        .expect("sin fila de nas");
    assert!(linea_vnc.contains('▣'), "{texto}");
    assert!(linea_vnc.contains("VNC"), "{texto}");
    assert!(linea_vnc.contains("directo"), "{texto}");
    let linea_voxl = texto
        .lines()
        .find(|linea| linea.contains("web-01-voxl"))
        .expect("sin fila de voxl");
    assert!(linea_voxl.contains("voxl"), "{texto}");
    assert!(texto.contains("3 escritorios"), "{texto}");

    // En ASCII, [D] y «! directo» en vez de ▣ y ⚠, sin glifos Unicode.
    let (mut prueba, _, _) =
        AppPrueba::con_semilla_de_escritorios_y_tema(200, 60, arnes::tema_ascii());
    prueba.tecla(KeyCode::F(2));
    let texto = prueba.texto();
    let linea = texto
        .lines()
        .find(|linea| linea.contains("web-01-voxl"))
        .expect("sin fila de voxl en ASCII");
    assert!(linea.contains("[D]"), "{texto}");
    assert!(linea.contains("voxl"), "{texto}");
    let malos: String = texto
        .chars()
        .filter(|c| !(c.is_ascii() || c.is_alphabetic()))
        .collect();
    assert!(malos.is_empty(), "glifos Unicode {malos:?}: {texto}");
}

/// La ficha del escritorio enseña el bloque «Escritorio» completo.
#[test]
fn la_ficha_ensena_el_bloque_escritorio() {
    let (mut prueba, _, escritorios) = AppPrueba::con_semilla_de_escritorios(200, 60);
    abrir_ficha_rdp(&mut prueba, &escritorios);
    let texto = prueba.texto();
    for esperado in [
        "ESCRITORIO",
        "Tipo",
        "RDP",
        "Destino",
        "A través de",
        "hetzner-01",
        "Usuario",
        "OFICINA",
        "Contraseña",
        "Tamaño",
        "1920x1080",
        "Teclado",
        "Portapapeles",
        "Certificado",
        "SHA256:",
        "olvidar la huella",
        "Conexión",
    ] {
        assert!(texto.contains(esperado), "sin «{esperado}»:\n{texto}");
    }
}

/// La pestaña sin visor cuenta la gracia («se cierra en 45 s»).
#[test]
fn la_pestana_sin_visor_cuenta_la_gracia() {
    let (mut prueba, _, escritorios) = AppPrueba::con_semilla_de_escritorios(120, 40);
    abrir_pestana(&mut prueba, &escritorios);
    let texto = prueba.texto();
    assert!(texto.contains("ESCRITORIO"), "{texto}");
    assert!(texto.contains("sin visor"), "{texto}");
    assert!(texto.contains("se cierra en 45 s"), "{texto}");
    assert!(texto.contains("1920"), "{texto}");
    assert!(texto.contains("H.264"), "{texto}");
    assert!(texto.contains("SHA256:"), "{texto}");
    assert!(texto.contains("abrir el visor"), "{texto}");
}

/// El diálogo de certificado nuevo es ámbar y el cambiado pide el nombre.
#[test]
fn certificado_nuevo_y_cambiado() {
    let (mut prueba, _, _) = AppPrueba::con_semilla_de_escritorios(100, 30);
    pedir_certificado(&mut prueba, TipoCambioCertificado::Nuevo);
    let texto = prueba.texto();
    assert!(texto.contains("CERTIFICADO NUEVO"), "{texto}");
    assert!(texto.contains("aceptar y guardar"), "{texto}");
    assert!(texto.contains("CN=WIN-01"), "{texto}");
    assert!(texto.contains("3 may 2027"), "{texto}");
    assert!(texto.contains("SHA256:"), "{texto}");

    // Esc: no se conecta (y el diálogo deja de pintarse).
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none(), "el diálogo no se cerró");

    pedir_certificado(&mut prueba, TipoCambioCertificado::Cambiado);
    let texto = prueba.texto();
    assert!(texto.contains("CERTIFICADO CAMBIADO"), "{texto}");
    assert!(texto.contains("ha cambiado"), "{texto}");
    assert!(texto.contains("Anterior"), "{texto}");
    assert!(texto.contains("sustituir y conectar"), "{texto}");
    // Escribir el nombre del host y sustituir: el diálogo se cierra y la
    // huella queda guardada antes de responder.
    for caracter in "win-01".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none(), "no se sustituyó");
}

/// Sin el nombre exacto no se sustituye y se avisa.
#[test]
fn el_certificado_cambiado_exige_el_nombre() {
    let (mut prueba, _, _) = AppPrueba::con_semilla_de_escritorios(100, 30);
    pedir_certificado(&mut prueba, TipoCambioCertificado::Cambiado);
    for caracter in "win-1".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Escritorio(_))),
        "se sustituyó sin el nombre exacto"
    );
    let texto = prueba.texto();
    assert!(texto.contains("escribe"), "{texto}");
}

/// Un diálogo abierto encima del aviso de tamaño se recoloca y encoge.
#[test]
fn dialogo_sobre_el_aviso() {
    let (mut prueba, _, _) = AppPrueba::con_semilla_de_escritorios(80, 24);
    pedir_certificado(&mut prueba, TipoCambioCertificado::Nuevo);
    prueba.redimensionar(30, 10);
    assert!(prueba.app.disposicion().aviso, "sin aviso a 30×10");
    let texto = prueba.texto();
    assert!(texto.contains("CERTIFICADO NUEVO"), "{texto}");
    prueba.paso_en(Instant::now());
    instantanea(&prueba, "certificado_nuevo_30x10");
}

/// Los casos con diálogo no se cierran solos al cambiar de tamaño y el
/// formulario no se queda sin pintar.
#[test]
fn las_vistas_no_quedan_en_blanco() {
    for (base, preparar) in casos() {
        let (mut prueba, _, escritorios) = AppPrueba::con_semilla_de_escritorios(80, 24);
        preparar(&mut prueba, &escritorios);
        prueba.pasar_por(80, 24);
        assert!(
            !prueba.texto().trim().is_empty(),
            "{base} en blanco a 80×24"
        );
    }
}
