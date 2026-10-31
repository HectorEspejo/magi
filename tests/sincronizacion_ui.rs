//! Pruebas de la interfaz de sincronización (Fase 10) sin hilos, red ni
//! servidor: App::de_prueba y el bucle real sobre un TestBackend.
//!
//! Cubren la paleta, la vista SINCRONIZACIÓN sin configurar, las
//! confirmaciones, los diálogos, la acción «recuperar»/«descartar» de
//! CONFLICTOS y el indicador de la barra. Lo que necesita el llavero del
//! sistema o el descifrado real no se ejecuta aquí (las pasadas están
//! apagadas en App::de_prueba).

use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::almacen::{conflictos, Almacen};
use magi::app::{AccionDialogo, AccionInventario, App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::modelo::{DatosHost, Origen};
use magi::sincronizacion::{EstadoPasada, ResultadoPasada};
use magi::tema::Tema;
use magi::ui::Vista;

/// Entorno aislado de la prueba.
struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magiui")
        .tempdir()
        .expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    Entorno {
        rutas: Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        },
        config: Config::default(),
        _temporal: temporal,
    }
}

fn con_app(entorno: &Entorno) -> (App, Terminal<TestBackend>) {
    let almacen = Almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    let (mut app, _enviados) = App::de_prueba(
        entorno.rutas.clone(),
        entorno.config.clone(),
        Tema::respaldo(),
        almacen,
    )
    .expect("app de prueba");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    app.iniciar_pintado(&mut terminal).expect("pintado");
    (app, terminal)
}

fn tecla(app: &mut App, terminal: &mut Terminal<TestBackend>, codigo: KeyCode) {
    app.procesar(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
    app.paso(terminal, Instant::now()).expect("paso");
}

fn tecla_con(
    app: &mut App,
    terminal: &mut Terminal<TestBackend>,
    codigo: KeyCode,
    modificadores: KeyModifiers,
) {
    app.procesar(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
    app.paso(terminal, Instant::now()).expect("paso");
}

/// Abre la paleta, escribe una consulta y ejecuta la primera entrada.
fn abrir_paleta(app: &mut App, terminal: &mut Terminal<TestBackend>, consulta: &str) {
    tecla_con(app, terminal, KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(app.paleta.is_some(), "la paleta no se abrió");
    for caracter in consulta.chars() {
        tecla(app, terminal, KeyCode::Char(caracter));
    }
    tecla(app, terminal, KeyCode::Enter);
}

fn texto(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut lineas = Vec::new();
    for y in 0..buffer.area.height {
        let mut linea = String::new();
        for x in 0..buffer.area.width {
            linea.push_str(buffer[(x, y)].symbol());
        }
        lineas.push(linea.trim_end().to_string());
    }
    lineas.join("\n")
}

/// Siembra un host y un conflicto de tipo elemento con una versión perdedora
/// que cambia la dirección.
fn sembrar_conflicto(entorno: &Entorno) {
    let almacen = Almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    let id = almacen
        .crear_host(
            &DatosHost {
                nombre: "web".to_string(),
                direccion: "192.0.2.7".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host");
    let conexion = almacen.conexion();
    let uuid: String = conexion
        .query_row("SELECT uuid FROM HOSTS WHERE id = ?1", [id], |fila| {
            fila.get(0)
        })
        .expect("uuid del host");
    let perdedora = r#"{"nombre":"web","direccion":"192.0.2.5","puerto":22,"usuario":null,"identidad_ref":null,"salto":null,"grupo":null,"multiplexar":false,"keepalive_seg":30,"opciones_extra":"","servicios":"","origen":"manual","etiquetas":[],"verificaciones":null,"snippet_al_conectar":null}"#;
    conexion
        .execute(
            "INSERT INTO CONFLICTOS (
                tipo, elemento_tipo, uuid, nombre, version_perdedora_json,
                version_ganadora, equipo_ganador, detectado_en
             ) VALUES ('elemento', 'host', ?1, 'web', ?2,
                       '1793873500001.6b1e2f3a-4c5d-4e6f-8a7b-9c0d1e2f3a41',
                       '6b1e2f3a-4c5d-4e6f-8a7b-9c0d1e2f3a41',
                       '2026-10-30T10:00:00+00:00')",
            rusqlite::params![uuid, perdedora],
        )
        .expect("conflicto");
    almacen.cerrar().expect("cerrar");
}

fn direccion_de(entorno: &Entorno, nombre: &str) -> Option<String> {
    let almacen = Almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    let direccion = almacen
        .conexion()
        .query_row(
            "SELECT direccion FROM HOSTS WHERE nombre = ?1",
            [nombre],
            |fila| fila.get(0),
        )
        .ok();
    almacen.cerrar().expect("cerrar");
    direccion
}

fn pendientes(entorno: &Entorno) -> Vec<conflictos::Conflicto> {
    let almacen = Almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    let lista = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    almacen.cerrar().expect("cerrar");
    lista
}

// ---------------------------------------------------------------- paleta

#[test]
fn la_paleta_ofrece_las_tres_entradas_de_sincronizacion() {
    let entorno = entorno();
    let (mut app, mut terminal) = con_app(&entorno);
    tecla_con(
        &mut app,
        &mut terminal,
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    );
    for caracter in "sincroniz".chars() {
        tecla(&mut app, &mut terminal, KeyCode::Char(caracter));
    }
    let paleta = app.paleta.as_ref().expect("paleta");
    let etiquetas: Vec<&str> = paleta
        .filtradas
        .iter()
        .map(|indice| paleta.entradas[*indice].etiqueta.as_str())
        .collect();
    for esperada in [
        "sincronización",
        "sincronizar ahora",
        "conflictos de sincronización",
    ] {
        assert!(etiquetas.contains(&esperada), "{etiquetas:?}");
    }
}

#[test]
fn sincronizar_ahora_sin_configurar_avisa_y_no_abre_dialogo() {
    let entorno = entorno();
    let (mut app, mut terminal) = con_app(&entorno);
    abrir_paleta(&mut app, &mut terminal, "sincronizar ahora");
    assert!(app.dialogo.is_none());
    let mensaje = app.mensaje.as_ref().expect("mensaje");
    assert!(mensaje.error);
    assert!(
        mensaje.texto.contains("no está configurada"),
        "{}",
        mensaje.texto
    );
    assert!(!app.sincronizacion.sincronizando);
}

// ---------------------------------------------------------------- vista

#[test]
fn n_pide_la_carpeta_y_el_error_la_vuelve_a_pedir() {
    let entorno = entorno();
    let (mut app, mut terminal) = con_app(&entorno);
    abrir_paleta(&mut app, &mut terminal, "sincronización");
    assert_eq!(app.vista, Vista::Sincronizacion);
    tecla(&mut app, &mut terminal, KeyCode::Char('n'));
    match &app.dialogo {
        Some(Dialogo::EntradaTexto { titulo, accion, .. }) => {
            assert!(titulo.contains("CARPETA"));
            assert!(matches!(
                accion,
                magi::app::EntradaTextoAccion::CarpetaSincronizacion
            ));
        }
        _ => panic!("n debe pedir la carpeta"),
    }
    // Un nombre de carpeta vacío no se acepta y deja el diálogo abierto.
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    assert!(matches!(app.dialogo, Some(Dialogo::EntradaTexto { .. })));
    // Una carpeta que no existe se dice y se vuelve a pedir.
    for caracter in "/ruta/que/no/existe".chars() {
        tecla(&mut app, &mut terminal, KeyCode::Char(caracter));
    }
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    assert!(matches!(app.dialogo, Some(Dialogo::EntradaTexto { .. })));
    let mensaje = app.mensaje.as_ref().expect("mensaje");
    assert!(
        mensaje.error && mensaje.texto.contains("no existe"),
        "{}",
        mensaje.texto
    );
}

#[test]
fn las_confirmaciones_de_peligro_no_ejecutan_sin_s() {
    let entorno = entorno();
    // Un equipo ajeno para que x tenga algo que quitar (este equipo no vale).
    let almacen = Almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    almacen
        .conexion()
        .execute(
            "INSERT INTO EQUIPOS (uuid, nombre, plataforma, es_este, estado)
             VALUES ('6b1e2f3a-4c5d-4e6f-8a7b-9c0d1e2f3a41', 'mac-mini', 'macos', 0, 'al_dia')",
            [],
        )
        .expect("equipo");
    almacen.cerrar().expect("cerrar");
    let (mut app, mut terminal) = con_app(&entorno);
    abrir_paleta(&mut app, &mut terminal, "sincronización");
    tecla(&mut app, &mut terminal, KeyCode::Char('k'));
    match &app.dialogo {
        Some(Dialogo::Confirmar { accion, .. }) => assert!(matches!(
            accion,
            AccionDialogo::Inventario(AccionInventario::MostrarClave)
        )),
        _ => panic!("k debe pedir confirmación"),
    }
    tecla(&mut app, &mut terminal, KeyCode::Esc);
    assert!(app.dialogo.is_none());
    // x solo actúa sobre otro equipo: se baja al de la semilla.
    tecla(&mut app, &mut terminal, KeyCode::Down);
    for pulsada in ['x', 'r', 'd'] {
        if pulsada == 'x' {
            tecla(&mut app, &mut terminal, KeyCode::Down);
        }
        tecla(&mut app, &mut terminal, KeyCode::Char(pulsada));
        let esperada = match &app.dialogo {
            Some(Dialogo::Confirmar { accion, .. }) => matches!(
                (pulsada, accion),
                (
                    'x',
                    AccionDialogo::Inventario(AccionInventario::QuitarEquipo(_))
                ) | ('r', AccionDialogo::Inventario(AccionInventario::RotarClave))
                    | ('d', AccionDialogo::Inventario(AccionInventario::Desactivar))
            ),
            _ => panic!("{pulsada} debe pedir confirmación"),
        };
        assert!(esperada, "{pulsada} pide la confirmación equivocada");
        tecla(&mut app, &mut terminal, KeyCode::Esc);
    }
}

#[test]
fn la_barra_no_ensena_indicador_si_esta_desactivada() {
    let entorno = entorno();
    let (mut app, mut terminal) = con_app(&entorno);
    let pintado = texto(&terminal);
    assert!(
        !pintado.contains("⇅") && !pintado.contains("sync"),
        "{pintado}"
    );
    app.probar_sincronizacion("~/Sync/magi", EstadoPasada::AlDia);
    app.paso(&mut terminal, Instant::now()).expect("paso");
    assert!(
        texto(&terminal).contains("⇅ al día"),
        "{}",
        texto(&terminal)
    );
    app.probar_resultado_pasada(ResultadoPasada::nuevo(EstadoPasada::Error(
        "la carpeta no existe".to_string(),
    )));
    app.paso(&mut terminal, Instant::now()).expect("paso");
    assert!(texto(&terminal).contains("⇅ error"), "{}", texto(&terminal));
}

#[test]
fn el_dialogo_unir_enmascara_la_clave_y_no_se_cierra_solo() {
    let entorno = entorno();
    let (mut app, mut terminal) = con_app(&entorno);
    app.abrir_dialogo(Dialogo::UnirEquipo {
        carpeta: PathBuf::from("/ruta/que/no/existe"),
        equipos: vec!["mac-mini".to_string()],
        campo: magi::ui::componentes::CampoTexto::nuevo(""),
        visible: false,
        error: None,
    });
    app.paso(&mut terminal, Instant::now()).expect("paso");
    for caracter in "AGE-SECRET-KEY-1QQQQ".chars() {
        tecla(&mut app, &mut terminal, KeyCode::Char(caracter));
    }
    assert!(matches!(app.dialogo, Some(Dialogo::UnirEquipo { .. })));
    // Sin enmascarar se ve la clave, no los puntos.
    tecla(&mut app, &mut terminal, KeyCode::Tab);
    match &app.dialogo {
        Some(Dialogo::UnirEquipo { visible, .. }) => assert!(*visible),
        _ => panic!("el diálogo se cerró solo"),
    }
    let pintado = texto(&terminal);
    assert!(pintado.contains("AGE-SECRET-KEY-1QQQQ"), "{pintado}");
    // Al unir con una carpeta que no existe, el motivo queda en el diálogo.
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    match &app.dialogo {
        Some(Dialogo::UnirEquipo { error, .. }) => {
            assert!(
                error
                    .as_deref()
                    .is_some_and(|error| error.contains("no existe")),
                "{error:?}"
            );
        }
        _ => panic!("el diálogo de unir debe seguir abierto con el error"),
    }
}

// ---------------------------------------------------------------- conflictos

#[test]
fn r_recupera_la_version_perdedora_y_resuelve_el_conflicto() {
    let entorno = entorno();
    sembrar_conflicto(&entorno);
    let (mut app, mut terminal) = con_app(&entorno);
    abrir_paleta(&mut app, &mut terminal, "conflictos");
    assert_eq!(app.vista, Vista::Conflictos);
    assert_eq!(app.sincronizacion.conflictos.len(), 1);
    tecla(&mut app, &mut terminal, KeyCode::Char('r'));
    assert!(matches!(app.dialogo, Some(Dialogo::Confirmar { .. })));
    tecla(&mut app, &mut terminal, KeyCode::Char('s'));
    assert!(app.dialogo.is_none());
    // La versión perdedora vuelve a ser la del host y el conflicto se resuelve.
    assert_eq!(direccion_de(&entorno, "web").as_deref(), Some("192.0.2.5"));
    assert!(pendientes(&entorno).is_empty());
}

#[test]
fn d_descarta_el_conflicto_sin_tocar_el_inventario() {
    let entorno = entorno();
    sembrar_conflicto(&entorno);
    let (mut app, mut terminal) = con_app(&entorno);
    abrir_paleta(&mut app, &mut terminal, "conflictos");
    tecla(&mut app, &mut terminal, KeyCode::Char('d'));
    assert!(matches!(app.dialogo, Some(Dialogo::Confirmar { .. })));
    tecla(&mut app, &mut terminal, KeyCode::Char('s'));
    assert!(app.dialogo.is_none());
    assert_eq!(
        direccion_de(&entorno, "web").as_deref(),
        Some("192.0.2.7"),
        "descartar no cambia el inventario"
    );
    assert!(pendientes(&entorno).is_empty());
}

#[test]
fn un_borrado_aplazado_no_se_recupera_y_lo_dice() {
    let entorno = entorno();
    let almacen = Almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    almacen
        .crear_host(
            &DatosHost {
                nombre: "backup-nas".to_string(),
                direccion: "192.0.2.140".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host");
    let conexion = almacen.conexion();
    let uuid: String = conexion
        .query_row(
            "SELECT uuid FROM HOSTS WHERE nombre = 'backup-nas'",
            [],
            |fila| fila.get(0),
        )
        .expect("uuid");
    conexion
        .execute(
            "INSERT INTO CONFLICTOS (tipo, elemento_tipo, uuid, nombre, detectado_en)
             VALUES ('borrado', 'host', ?1, 'backup-nas', '2026-10-30T10:00:00+00:00')",
            rusqlite::params![uuid],
        )
        .expect("conflicto");
    almacen.cerrar().expect("cerrar");
    let (mut app, mut terminal) = con_app(&entorno);
    abrir_paleta(&mut app, &mut terminal, "conflictos");
    assert_eq!(app.sincronizacion.conflictos.len(), 1);
    // r no abre confirmación: un borrado aplazado se acepta o se descarta.
    tecla(&mut app, &mut terminal, KeyCode::Char('r'));
    assert!(app.dialogo.is_none());
    let mensaje = app.mensaje.as_ref().expect("mensaje");
    assert!(
        mensaje.texto.contains("no se recupera"),
        "{}",
        mensaje.texto
    );
    // a acepta el borrado: sin trabajo vivo, se aplica.
    tecla(&mut app, &mut terminal, KeyCode::Char('a'));
    assert!(matches!(app.dialogo, Some(Dialogo::Confirmar { .. })));
    tecla(&mut app, &mut terminal, KeyCode::Char('s'));
    assert!(app.dialogo.is_none());
    assert_eq!(direccion_de(&entorno, "backup-nas"), None);
    assert!(pendientes(&entorno).is_empty());
}
