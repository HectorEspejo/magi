//! Tubería de tamaño del cliente: `Geometria` agrupa los `Resize`, el bucle se
//! despierta solo con un tamaño pendiente y `aplicar_tamano` repinta y avisa
//! al remoto de la pestaña adjunta (fallos 1 y 2 de R36).

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};

use magi::app::Evento;
use magi::protocolo::MensajeCliente;
use magi::ui::Vista;

use crate::arnes::{sesion, AppPrueba};

const MS: Duration = Duration::from_millis(1);

/// Fallo 1: tras pintar a 120×40 llega `Resize(80, 24)` y ningún evento más;
/// la vista debe quedar pintada a 80×24 sin pulsar ninguna tecla. Se usa el
/// bucle real (`ciclo`). Tras aplicar, el bucle vuelve a dormir hasta el
/// próximo evento: un vigía manda `Tick` (no es una tecla y no repinta) para
/// devolver el control a la prueba.
#[test]
fn fallo1_repinta_sin_tecla() {
    let mut prueba = AppPrueba::nueva(120, 40);
    assert_eq!(prueba.tamano_pintado(), (120, 40));
    let tx = prueba.app.eventos_tx.clone();
    let parar = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let vigia = {
        let (tx, parar) = (tx.clone(), parar.clone());
        std::thread::spawn(move || {
            while !parar.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                let _ = tx.send(Evento::Tick);
            }
        })
    };

    let limpiezas = prueba.terminal.backend().limpiezas;
    prueba.terminal.backend_mut().interior.resize(80, 24);
    let inicio = Instant::now();
    tx.send(Evento::Redimension(80, 24)).unwrap();
    while prueba.app.geometria().aplicado() != (80, 24) && inicio.elapsed() < Duration::from_secs(3)
    {
        prueba.app.ciclo(&mut prueba.terminal).unwrap();
    }
    parar.store(true, std::sync::atomic::Ordering::Relaxed);
    vigia.join().unwrap();

    assert_eq!(prueba.app.geometria().aplicado(), (80, 24));
    assert_eq!(prueba.tamano_pintado(), (80, 24));
    assert!(prueba.texto().contains("MAGI"), "{}", prueba.texto());
    assert_eq!(prueba.terminal.backend().limpiezas, limpiezas + 1);
    // RNF: ≤ 70 ms del último evento al repintado; margen para máquinas lentas.
    let tardanza = prueba.terminal.backend().ultima_limpieza.unwrap() - inicio;
    assert!(tardanza < Duration::from_millis(200), "tardó {tardanza:?}");
}

/// AC: 30 `Resize` en 400 ms; cuando pasan 50 ms sin eventos se aplica un
/// único tamaño (el último), con un solo clear y un solo `Redimensionar`.
#[test]
fn treinta_eventos_un_clear_y_un_redimensionar() {
    let mut prueba = AppPrueba::nueva(120, 40);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.enviados();
    let limpiezas = prueba.terminal.backend().limpiezas;
    let aplicados = prueba.app.geometria().aplicados();

    let t0 = Instant::now();
    let mut ultimo = t0;
    for i in 0..30u16 {
        ultimo = t0 + MS * (u32::from(i) * 13);
        prueba.redimensionar_en(90 + i, 30 + i / 2, ultimo);
        prueba.paso_en(ultimo);
    }
    prueba.paso_en(ultimo + MS * 49);
    assert_eq!(
        prueba.terminal.backend().limpiezas,
        limpiezas,
        "limpió antes de tiempo"
    );
    assert!(prueba.redimensionares().is_empty(), "avisó antes de tiempo");

    prueba.paso_en(ultimo + MS * 50);
    assert_eq!(prueba.terminal.backend().limpiezas, limpiezas + 1);
    assert_eq!(prueba.redimensionares(), vec![(1, 119, 44 - 4)]);
    assert_eq!(prueba.app.geometria().aplicados(), aplicados + 1);
    assert_eq!(prueba.tamano_pintado(), (119, 44));

    // Nada más después.
    prueba.paso_en(ultimo + MS * 500);
    assert_eq!(prueba.terminal.backend().limpiezas, limpiezas + 1);
    assert!(prueba.redimensionares().is_empty());
}

/// Un tamaño igual al aplicado no limpia, no repinta ni avisa al remoto.
#[test]
fn un_tamano_igual_no_hace_nada() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.enviados();
    let limpiezas = prueba.terminal.backend().limpiezas;
    prueba.redimensionar(100, 30);
    assert_eq!(prueba.terminal.backend().limpiezas, limpiezas);
    assert!(prueba.redimensionares().is_empty());
    assert_eq!(prueba.app.geometria().aplicados(), 0);
}

/// Fallo 2 en el cliente: en la vista Sesión cada tamaño aplicado llega al
/// servidor con `alto_pty` del tamaño aplicado.
#[test]
fn cada_tamano_aplicado_llega_a_la_pestana_adjunta() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.enviados();
    prueba.redimensionar(160, 45);
    prueba.redimensionar(150, 42);
    prueba.redimensionar(80, 24);
    assert_eq!(
        prueba.redimensionares(),
        vec![(1, 160, 41), (1, 150, 38), (1, 80, 20)]
    );
}

/// Fuera de la vista Sesión no se envía `Redimensionar` (no hay pestaña
/// adjunta); al volver, `Adjuntar` lleva el tamaño aplicado.
#[test]
fn fuera_de_sesion_no_se_avisa_y_al_volver_se_adjunta_con_el_aplicado() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    // En Sesión las teclas van al remoto: se sale con el prefijo y `q`.
    prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    prueba.tecla(KeyCode::Char('q'));
    assert_ne!(prueba.app.vista, Vista::Sesion);
    prueba.enviados();
    prueba.redimensionar(140, 50);
    assert!(prueba.redimensionares().is_empty());
    prueba.tecla(KeyCode::F(3));
    assert!(prueba.enviados().contains(&MensajeCliente::Adjuntar {
        sesion_id: 1,
        cols: 140,
        filas: 46,
    }));
}

/// Con un tamaño pendiente (aún agrupando), `Adjuntar` lleva el aplicado: el
/// pendiente llegará con su `Redimensionar` al aplicarse.
#[test]
fn adjuntar_usa_el_tamano_aplicado_y_no_el_pendiente() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.bienvenida(vec![sesion(1, "hetzner-01")]);
    let ahora = Instant::now();
    prueba.redimensionar_en(90, 20, ahora);
    prueba.tecla(KeyCode::F(3));
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.enviados().contains(&MensajeCliente::Adjuntar {
        sesion_id: 1,
        cols: 100,
        filas: 26,
    }));
    prueba.paso_en(ahora + MS * 60);
    assert_eq!(prueba.redimensionares(), vec![(1, 90, 16)]);
}

/// `AbrirSesion` lleva el tamaño aplicado.
#[test]
fn abrir_sesion_usa_el_tamano_aplicado() {
    let entorno = crate::comun::entorno();
    {
        let almacen = magi::almacen::Almacen::abrir(&entorno.rutas.base_datos()).unwrap();
        magi::almacen::hosts::crear(
            almacen.conexion(),
            &magi::modelo::DatosHost {
                nombre: "hetzner-01".to_string(),
                direccion: "10.0.0.1".to_string(),
                puerto: 22,
                ..magi::modelo::DatosHost::default()
            },
            magi::modelo::Origen::Manual,
        )
        .unwrap();
        almacen.cerrar().unwrap();
    }
    let mut prueba = AppPrueba::con_entorno(entorno, 90, 28);
    prueba.redimensionar(132, 43);
    prueba.tecla(KeyCode::F(2));
    prueba.enviados();
    // La primera fila es el grupo «sin grupo».
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Enter);
    let abrir = prueba
        .enviados()
        .into_iter()
        .find(|mensaje| matches!(mensaje, MensajeCliente::AbrirSesion { .. }));
    assert!(
        matches!(
            abrir,
            Some(MensajeCliente::AbrirSesion {
                cols: 132,
                filas: 39,
                ..
            })
        ),
        "{abrir:?}"
    );
}

/// Cambiar de pestaña desadjunta la vieja y adjunta la nueva con el tamaño
/// aplicado.
#[test]
fn cambiar_de_pestana_adjunta_la_nueva_con_el_aplicado() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01"), sesion(2, "hetzner-02")]);
    prueba.redimensionar(120, 40);
    prueba.enviados();
    prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    prueba.tecla(KeyCode::Char('n'));
    let enviados = prueba.enviados();
    let desadjuntar = enviados
        .iter()
        .position(|mensaje| *mensaje == MensajeCliente::Desadjuntar { sesion_id: 1 });
    let adjuntar = enviados.iter().position(|mensaje| {
        *mensaje
            == MensajeCliente::Adjuntar {
                sesion_id: 2,
                cols: 120,
                filas: 36,
            }
    });
    assert!(
        matches!((desadjuntar, adjuntar), (Some(d), Some(a)) if d < a),
        "{enviados:?}"
    );
}

/// Al volver del visor (o de cualquier suspensión) se aplica el tamaño real,
/// aunque el hilo de teclas no viera el `Resize`.
#[test]
fn la_vuelta_del_visor_aplica_el_tamano_real() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.enviados();
    // Mientras el paginador tenía el terminal, la ventana cambió.
    prueba.terminal.backend_mut().interior.resize(90, 28);
    prueba
        .app
        .volver_de_suspension(&mut prueba.terminal, (90, 28))
        .unwrap();
    assert_eq!(prueba.app.geometria().aplicado(), (90, 28));
    assert_eq!(prueba.tamano_pintado(), (90, 28));
    assert_eq!(prueba.redimensionares(), vec![(1, 90, 24)]);

    // Sin cambio de tamaño: solo se repinta.
    prueba
        .app
        .volver_de_suspension(&mut prueba.terminal, (90, 28))
        .unwrap();
    assert!(prueba.redimensionares().is_empty());
}

/// Tras relanzar el servidor, la pestaña se readjunta con el tamaño aplicado.
#[test]
fn al_relanzar_el_servidor_se_readjunta_con_el_aplicado() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.redimensionar(150, 48);
    let (cliente, mut enviados) = magi::cliente::Cliente::de_prueba();
    prueba.evento(Evento::ServidorConectado(Ok(cliente)));
    prueba.bienvenida(vec![sesion(7, "hetzner-01")]);
    prueba.tecla(KeyCode::F(3));
    prueba.tecla(KeyCode::Enter);
    let mut mensajes = Vec::new();
    while let Ok(mensaje) = enviados.try_recv() {
        mensajes.push(mensaje);
    }
    assert!(
        mensajes.contains(&MensajeCliente::Adjuntar {
            sesion_id: 7,
            cols: 150,
            filas: 44,
        }),
        "{mensajes:?}"
    );
}

/// Por debajo del mínimo de Sesión el remoto sigue recibiendo su tamaño real
/// saneado y las teclas siguen yendo al remoto.
#[test]
fn por_debajo_del_minimo_de_sesion_el_remoto_sigue_recibiendo_su_tamano() {
    let mut prueba = AppPrueba::nueva(100, 30);
    prueba.entrar_en_sesion(vec![sesion(1, "hetzner-01")]);
    prueba.enviados();
    prueba.redimensionar(30, 6);
    assert_eq!(prueba.redimensionares(), vec![(1, 30, 2)]);
    prueba.redimensionar(1, 1);
    assert_eq!(prueba.redimensionares(), vec![(1, 2, 1)]);
    prueba.tecla(KeyCode::Char('l'));
    assert!(prueba
        .enviados()
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeCliente::Teclas { sesion_id: 1, .. })));
}
