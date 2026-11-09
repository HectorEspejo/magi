//! Prueba de extremo a extremo de la vista Servicios (Fase 13) sin red
//! externa: vista -> servidor de sesiones en proceso -> host SSH de prueba con
//! binarios falsos en el PATH.
//!
//! La App se conecta al servidor ya arrancado por el escenario
//! (de_prueba_conectada) y se conduce con bombear hasta que se cumple cada
//! condición, sin dormir plazos fijos. Cubre listar, deliberar y reiniciar un
//! servicio que muere, abrir y cerrar el panel de logs (PararLog corta el
//! journalctl falso) y que «Toda la flota» no abre conexiones.

mod comun;

#[path = "../crates/magi-nucleo/tests/fijos/servicios/binarios.rs"]
mod binarios;

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::servicios::{DialogoServicio, EstadoListado, Foco, Seccion};
use magi::app::{App, Evento};
use magi::tema::Tema;
use magi::ui::Vista;
use magi_nucleo::servicios::{ResultadoAccion, Systemd};

use binarios::BinariosFalsos;
use comun::OpcionesEscenario;

/// Plazo generoso para cada espera (la prueba no debe colgarse nunca).
const PLAZO: Duration = Duration::from_secs(20);

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

/// Bombea eventos (y pintados) hasta que se cumple la condición o vence el
/// plazo. Nada de dormir a ciegas.
fn bombear_hasta(
    app: &mut App,
    terminal: &mut Terminal<TestBackend>,
    plazo: Duration,
    mut vale: impl FnMut(&App) -> bool,
) {
    let fin = Instant::now() + plazo;
    while Instant::now() < fin {
        if vale(app) {
            return;
        }
        app.bombear(terminal, Duration::from_millis(40))
            .expect("bombeando");
    }
    assert!(vale(app), "la condición no se cumplió en {plazo:?}");
}

#[test]
fn vista_servidor_y_host_de_prueba_de_extremo_a_extremo() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime del escenario");

    // Binarios falsos del host: los guiones compartidos de la F13
    // (crates/magi-nucleo/tests/fijos/servicios/). Una unidad viva cuyo
    // reinicio falla, el diario con sus últimas líneas y el seguimiento que
    // deja constancia de que lo mataron.
    let binarios = BinariosFalsos::nuevos();
    binarios.unidad(
        "app.service",
        &[
            ("ActiveState", "active"),
            ("SubState", "running"),
            ("CanReload", "yes"),
            ("UnitFileState", "enabled"),
        ],
    );
    binarios.texto_unidad(
        "app.service",
        "[Unit]\nDescription=app de prueba\n\n[Service]\nExecStart=/bin/true\n",
    );
    binarios.escribir("accion_codigo", "1");
    binarios.escribir(
        "journal_short/app.service",
        "2026-11-08T12:00:00+00:00 aviso: el servicio no arrancó\n",
    );
    binarios.escribir(
        "journal/app.service",
        "{\"__REALTIME_TIMESTAMP\":\"1794139824180943\",\"PRIORITY\":\"6\",\"MESSAGE\":\"hola\"}\n",
    );

    let escenario = runtime.block_on(comun::escenario(OpcionesEscenario {
        binarios: Some(binarios.ruta().to_path_buf()),
        ..Default::default()
    }));
    let host = escenario.hosts[0];

    let almacen =
        magi::almacen::abrir(&escenario.entorno.rutas.base_datos()).expect("almacén de prueba");
    let mut app = App::de_prueba_conectada(
        escenario.entorno.rutas.clone(),
        escenario.entorno.config.clone(),
        Tema::respaldo(),
        almacen,
    )
    .expect("app conectada");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    app.iniciar_pintado(&mut terminal).expect("pintado");

    // F9 desde Flota con el host de prueba elegido.
    let indice = app
        .hosts_flota()
        .iter()
        .position(|indice| app.hosts[*indice].id == host)
        .expect("el host de prueba está en Flota");
    app.seleccion_flota = indice;
    tecla(&mut app, &mut terminal, KeyCode::F(9));
    assert_eq!(app.vista, Vista::Servicios);
    bombear_hasta(&mut app, &mut terminal, PLAZO, |app| {
        app.servicios_listado(host).is_some()
    });
    let listado = app.servicios_listado(host).expect("listado del host");
    assert!(
        matches!(listado.listado.systemd, Systemd::Disponible(_)),
        "systemd debería estar disponible: {:?}",
        listado.listado.systemd
    );
    assert!(
        listado
            .listado
            .unidades
            .iter()
            .any(|unidad| unidad.nombre == "app.service"),
        "falta app.service: {:?}",
        listado
            .listado
            .unidades
            .iter()
            .map(|unidad| unidad.nombre.clone())
            .collect::<Vec<_>>()
    );

    // Reiniciar app.service: deliberación (sin verificaciones activas) y
    // Ctrl+K. El servicio muere al arrancar, así que el resultado es fallido.
    let indice = app
        .servicios_elementos(host)
        .iter()
        .position(|elemento| elemento.nombre() == "app.service")
        .expect("app.service en la lista");
    app.servicios
        .seleccion
        .insert((host, Seccion::Servicios), indice);
    app.servicios.foco = Foco::Lista;
    tecla(&mut app, &mut terminal, KeyCode::Char('R'));
    assert!(
        app.deliberacion.is_some(),
        "reiniciar debe pedir deliberación"
    );
    tecla_con(
        &mut app,
        &mut terminal,
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
    );
    bombear_hasta(&mut app, &mut terminal, PLAZO, |app| {
        matches!(app.servicios.dialogo, Some(DialogoServicio::Resultado(_)))
    });
    match &app.servicios.dialogo {
        Some(DialogoServicio::Resultado(resultado)) => {
            assert_eq!(
                resultado.resultado.resultado,
                ResultadoAccion::Fallido,
                "el reinicio debía fallar: {:?}",
                resultado.resultado
            );
            assert!(
                !resultado.resultado.lineas.is_empty(),
                "el resultado fallido trae las últimas líneas del diario"
            );
        }
        otro => panic!("no hay diálogo de resultado: {otro:?}"),
    }
    tecla(&mut app, &mut terminal, KeyCode::Esc);
    bombear_hasta(&mut app, &mut terminal, PLAZO, |app| {
        app.servicios.dialogo.is_none()
    });

    // Logs en vivo: llegan líneas del journalctl falso y al cerrar se manda
    // PararLog, el servidor cierra el canal y el script mata el seguimiento.
    tecla(&mut app, &mut terminal, KeyCode::Char('l'));
    bombear_hasta(&mut app, &mut terminal, PLAZO, |app| {
        app.servicios
            .logs
            .as_ref()
            .is_some_and(|logs| logs.total > 0)
    });
    let logs = app.servicios.logs.as_ref().expect("panel de logs");
    assert!(!logs.abriendo);
    tecla(&mut app, &mut terminal, KeyCode::Esc);
    assert!(app.servicios.logs.is_none(), "el panel se cierra");
    bombear_hasta(&mut app, &mut terminal, PLAZO, |app| {
        app.servicios.logs.is_none()
            && app
                .servicios
                .listados
                .get(&host)
                .is_some_and(|listado| listado.estado == EstadoListado::Ok)
    });
    // El journalctl falso recibió el TERM: el seguimiento se cortó en el host.
    let marcas = binarios.estado().join("terminado_journalctl");
    bombear_hasta(&mut app, &mut terminal, PLAZO, |_| marcas.exists());
    assert!(
        binarios
            .llamadas()
            .lines()
            .any(|llamada| llamada.starts_with("journalctl") && llamada.contains("-f")),
        "el host debía arrancar journalctl -f: {:?}",
        binarios.llamadas()
    );

    // «Toda la flota» no abre conexiones: pasar a ella no deja más llamadas.
    let antes = binarios.llamadas().lines().count();
    tecla(&mut app, &mut terminal, KeyCode::Tab);
    tecla(&mut app, &mut terminal, KeyCode::Up);
    assert_eq!(app.servicios_host_seleccionado(), None);
    app.bombear(&mut terminal, Duration::from_millis(300))
        .expect("bombeando sin pedir nada");
    assert_eq!(
        binarios.llamadas().lines().count(),
        antes,
        "entrar en «Toda la flota» no debe hablar con el host"
    );
}
