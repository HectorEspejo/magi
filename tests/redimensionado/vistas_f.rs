//! Instantáneas y pruebas de disposición del grupo F: Resultados, diálogo MAGI, EJECUTAR y formulario de snippet.
//!
//! Las vistas se abren a 80×24 (con el aviso de tamaño las teclas de las
//! vistas no llegan) y se llevan después al tamaño de cada prueba. Las
//! ejecuciones son de 2023 (nunca «de hoy») y sin hosts en marcha: nada
//! depende del reloj.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

use magi::app::lanzar::{ModoLanzamiento, PlanEjecucion};
use magi::app::{DeliberacionAbierta, Dialogo, DialogoSnippets, Evento};
use magi::deliberacion::estado::Fase;
use magi::deliberacion::{ComprobacionesHost, Veredicto};
use magi::protocolo::{
    EstadoEjecucion, EstadoHostEjecucion, InfoEjecucion, InfoEjecucionHost, MensajeCliente,
    MensajeServidor, SalidaRemota,
};
use magi::tema::Tema;
use magi::ui::disposicion::{self, Disposicion, Lista};
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};
use crate::semilla::Sembrado;

/// Los tres tamaños de todas las vistas.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

/// Hora de las ejecuciones: 14 nov 2023, 22:13:20 UTC.
const CREADA: i64 = 1_700_000_000;

// ---------------------------------------------------------------- datos

fn host(
    host_id: i64,
    nombre: &str,
    estado: EstadoHostEjecucion,
    codigo: Option<i32>,
    duracion_ms: Option<u64>,
) -> InfoEjecucionHost {
    InfoEjecucionHost {
        host_id,
        nombre: nombre.to_string(),
        estado,
        codigo,
        inicio_ms: duracion_ms.map(|_| CREADA * 1000),
        duracion_ms,
        bytes_stdout: if duracion_ms.is_some() { 212 } else { 0 },
        bytes_stderr: 0,
        truncada: false,
        error: None,
    }
}

/// Doce ejecuciones de otra ventana (solicitante 99): la más reciente es
/// «reiniciar nginx» con un fallo en hetzner-02; la anterior, en curso con
/// sus hosts en cola; el resto, terminadas.
fn ejecuciones(sembrado: &Sembrado) -> Vec<InfoEjecucion> {
    let id = |nombre: &str| sembrado.host(nombre);
    let ok = |nombre: &str| {
        host(
            id(nombre),
            nombre,
            EstadoHostEjecucion::Ok,
            Some(0),
            Some(1_200),
        )
    };
    let mut lista = Vec::new();
    for numero in 1..=10u32 {
        let nombre = if numero % 2 == 0 {
            "actualizar paquetes"
        } else {
            "espacio en disco"
        };
        lista.push(InfoEjecucion {
            id: numero,
            peticion_id: u64::from(numero),
            solicitante: 99,
            snippet_id: None,
            nombre: nombre.to_string(),
            hosts: vec![ok("hetzner-01"), ok("backup-nas")],
            timeout_seg: 60,
            parar_al_fallo: false,
            deliberacion_id: None,
            forzada: false,
            estado: EstadoEjecucion::Terminada,
            creada_en: CREADA + i64::from(numero) * 60,
            terminada_en: Some(CREADA + i64::from(numero) * 60 + 2),
        });
    }
    lista.push(InfoEjecucion {
        id: 11,
        peticion_id: 11,
        solicitante: 99,
        snippet_id: None,
        nombre: "espacio en disco".to_string(),
        hosts: vec![
            host(
                id("hetzner-01"),
                "hetzner-01",
                EstadoHostEjecucion::EnCola,
                None,
                None,
            ),
            host(
                id("backup-nas"),
                "backup-nas",
                EstadoHostEjecucion::EnCola,
                None,
                None,
            ),
        ],
        timeout_seg: 60,
        parar_al_fallo: false,
        deliberacion_id: None,
        forzada: false,
        estado: EstadoEjecucion::EnCurso,
        creada_en: CREADA + 11 * 60,
        terminada_en: None,
    });
    let mut fallo = host(
        id("hetzner-02"),
        "hetzner-02",
        EstadoHostEjecucion::Fallo,
        Some(1),
        Some(800),
    );
    fallo.bytes_stdout = 0;
    fallo.bytes_stderr = 1_434;
    fallo.error = Some(
        "Job for nginx.service failed because the control process exited with error code."
            .to_string(),
    );
    lista.push(InfoEjecucion {
        id: 12,
        peticion_id: 12,
        solicitante: 99,
        snippet_id: None,
        nombre: "reiniciar nginx".to_string(),
        hosts: vec![ok("hetzner-01"), fallo, ok("vps-openclaw")],
        timeout_seg: 60,
        parar_al_fallo: true,
        deliberacion_id: Some(1),
        forzada: true,
        estado: EstadoEjecucion::Terminada,
        creada_en: CREADA + 12 * 60,
        terminada_en: Some(CREADA + 12 * 60 + 2),
    });
    lista
}

/// Resultados abierta a 80×24 (F8 → t) con las doce ejecuciones.
fn abrir_resultados(tema: Tema) -> (AppPrueba, Sembrado) {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(80, 24, tema);
    prueba.tecla(KeyCode::F(8));
    prueba.tecla(KeyCode::Char('t'));
    assert_eq!(prueba.app.vista, Vista::Resultados);
    prueba.servidor(MensajeServidor::Ejecuciones {
        lista: ejecuciones(&sembrado),
    });
    assert_eq!(prueba.app.resultados.ejecuciones.len(), 12);
    (prueba, sembrado)
}

/// Resultados a un tamaño: la ejecución con el fallo seleccionada y, en el
/// panel de hosts, hetzner-02 (el del fallo).
fn resultados_en(cols: u16, filas: u16, tema: Tema) -> AppPrueba {
    let (mut prueba, _) = abrir_resultados(tema);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Down);
    assert_eq!(prueba.app.resultados.host_seleccionado, 1);
    prueba.redimensionar(cols, filas);
    prueba
}

fn plan(hosts: &[(i64, &str)]) -> PlanEjecucion {
    PlanEjecucion {
        snippet_id: 1,
        nombre: "reiniciar nginx".to_string(),
        comando: "sudo systemctl restart nginx".to_string(),
        hosts: hosts.iter().map(|(id, n)| (*id, n.to_string())).collect(),
        timeout_seg: 60,
        parar_al_fallo: false,
        critico: true,
        valores: Default::default(),
        modo: ModoLanzamiento::Servidor,
    }
}

fn aprueba(detalle: &str) -> Veredicto {
    Veredicto::Aprueba {
        detalle: detalle.to_string(),
        ms: 1,
    }
}

fn rechaza(detalle: &str) -> Veredicto {
    Veredicto::Rechaza {
        detalle: detalle.to_string(),
        ms: 1,
    }
}

/// La deliberación de la maqueta §6.5: tres hosts de producción, el backup
/// de hetzner-02 de hace 31 h (bloqueada).
fn deliberacion_maqueta(sembrado: &Sembrado) -> DeliberacionAbierta {
    let id = |nombre: &str| sembrado.host(nombre);
    let filas = vec![
        ComprobacionesHost {
            host_id: id("hetzner-01"),
            host: "hetzner-01".to_string(),
            salud: aprueba("NOMINAL · hace 2 min"),
            backup: aprueba("hace 3 h (pg.sql.gz)"),
            tests: aprueba("verde"),
        },
        ComprobacionesHost {
            host_id: id("hetzner-02"),
            host: "hetzner-02".to_string(),
            salud: aprueba("NOMINAL · hace 2 min"),
            backup: rechaza("último backup hace 31 h (pg.sql.gz)"),
            tests: aprueba("verde"),
        },
        ComprobacionesHost {
            host_id: id("vps-openclaw"),
            host: "vps-openclaw".to_string(),
            salud: aprueba("NOMINAL · hace 2 min"),
            backup: Veredicto::NoActiva,
            tests: Veredicto::NoActiva,
        },
    ];
    let hosts = [
        (id("hetzner-01"), "hetzner-01"),
        (id("hetzner-02"), "hetzner-02"),
        (id("vps-openclaw"), "vps-openclaw"),
    ];
    DeliberacionAbierta::de_prueba(plan(&hosts), filas, 10)
}

/// Una deliberación de `cuantos` hosts, todos aprobados.
fn deliberacion_larga(cuantos: i64) -> DeliberacionAbierta {
    let nombres: Vec<(i64, String)> = (1..=cuantos).map(|i| (i, format!("web-{i:02}"))).collect();
    let referencias: Vec<(i64, &str)> = nombres.iter().map(|(i, n)| (*i, n.as_str())).collect();
    let filas = nombres
        .iter()
        .map(|(i, n)| ComprobacionesHost {
            host_id: *i,
            host: n.clone(),
            salud: aprueba("NOMINAL · hace 1 min"),
            backup: aprueba("hace 2 h (x)"),
            tests: Veredicto::NoActiva,
        })
        .collect();
    DeliberacionAbierta::de_prueba(plan(&referencias), filas, 10)
}

/// Abre la deliberación sobre F8 (a 80×24) y la lleva al tamaño pedido.
fn magi_en(
    cols: u16,
    filas: u16,
    tema: Tema,
    deliberacion: impl FnOnce(&Sembrado) -> DeliberacionAbierta,
) -> AppPrueba {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(80, 24, tema);
    prueba.tecla(KeyCode::F(8));
    prueba.app.deliberacion = Some(deliberacion(&sembrado));
    prueba.app.iniciar_pintado(&mut prueba.terminal).unwrap();
    prueba.redimensionar(cols, filas);
    prueba
}

/// F8 con el snippet `nombre` seleccionado (van por nombre).
fn seleccionar_snippet(prueba: &mut AppPrueba, nombre: &str) {
    prueba.tecla(KeyCode::F(8));
    let posicion = prueba
        .app
        .snippets
        .lista
        .iter()
        .position(|snippet| snippet.nombre == nombre)
        .expect("snippet sembrado");
    for _ in 0..posicion {
        prueba.tecla(KeyCode::Down);
    }
}

/// EJECUTAR abierto a 80×24 con ↵ sobre `snippet`.
fn abrir_ejecutar(tema: Tema, snippet: &str) -> AppPrueba {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(80, 24, tema);
    seleccionar_snippet(&mut prueba, snippet);
    prueba.tecla(KeyCode::Enter);
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Ejecutar(_))),
        "{}",
        prueba.texto()
    );
    prueba
}

/// El formulario de snippet nuevo (F8 → n) abierto a 80×24.
fn abrir_formulario(tema: Tema) -> AppPrueba {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(80, 24, tema);
    prueba.tecla(KeyCode::F(8));
    prueba.tecla(KeyCode::Char('n'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Snippets(DialogoSnippets::Formulario(_)))
    ));
    prueba
}

// ---------------------------------------------------------------- utilidades

/// Pinta algo aparte, sobre una terminal limpia de `cols`×`filas` (para
/// mirar solo lo que pinta este grupo, sin la vista ni la barra de debajo).
fn pintar_aparte(cols: u16, filas: u16, pintar: impl FnOnce(&mut Frame, Rect)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(cols, filas)).unwrap();
    terminal
        .draw(|marco| {
            let area = marco.area();
            pintar(marco, area);
        })
        .unwrap();
    arnes::texto_de(terminal.backend())
}

fn sin_unicode(texto: &str, que: &str) {
    assert!(
        texto.chars().all(|c| c.is_ascii() || c.is_alphabetic()),
        "glifo Unicode en {que}:\n{texto}"
    );
}

/// La lista está pintada, deja `seleccion` a la vista y no tiene huecos al
/// final.
fn a_la_vista(disposicion: &Disposicion, lista: Lista, seleccion: usize) {
    let ventana = disposicion
        .lista(lista)
        .unwrap_or_else(|| panic!("{lista:?} sin pintar"));
    assert!(
        ventana.inicio <= seleccion && seleccion < ventana.inicio + ventana.filas,
        "{lista:?}: la selección {seleccion} no se ve en {ventana:?}"
    );
    assert!(
        ventana.inicio <= ventana.total.saturating_sub(ventana.filas),
        "{lista:?} con hueco al final: {ventana:?}"
    );
}

// ---------------------------------------------------------------- Resultados

#[test]
fn resultados_instantaneas() {
    for (cols, filas) in TAMANOS.into_iter().chain([(100, 20)]) {
        let prueba = resultados_en(cols, filas, Tema::respaldo());
        prueba.instantanea(&format!("resultados_{cols}x{filas}"));
    }
}

/// Con alto < 24 la salida solo se ve con ↵: sin vista previa, y ↵ en el
/// panel de hosts abre el visor y pide la salida.
#[test]
fn resultados_con_alto_menor_de_24_la_salida_solo_con_intro() {
    let mut prueba = resultados_en(100, 24, Tema::respaldo());
    assert!(prueba.texto().contains("hetzner-02 · fallo · stdout 0 B"));
    prueba.redimensionar(100, 23);
    let texto = prueba.texto();
    assert!(!texto.contains("stdout 0 B"), "{texto}");
    assert!(!prueba.app.disposicion().aviso);
    prueba.enviados();
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.resultados.visor.is_some());
    let pedidas: Vec<_> = prueba
        .enviados()
        .into_iter()
        .filter(|mensaje| matches!(mensaje, MensajeCliente::PedirSalida { .. }))
        .collect();
    assert_eq!(pedidas.len(), 1);
    let texto = prueba.texto();
    assert!(texto.contains("stderr · hetzner-02"), "{texto}");
    // Por debajo de 60×14 (mínimo de Resultados), el aviso.
    prueba.redimensionar(59, 20);
    assert!(prueba.app.disposicion().aviso);
    assert!(
        prueba.texto().contains("mínimo 60×14"),
        "{}",
        prueba.texto()
    );
}

/// Las teclas de página usan las filas pintadas de cada panel.
#[test]
fn resultados_las_paginas_son_las_filas_pintadas() {
    let (mut prueba, _) = abrir_resultados(Tema::respaldo());
    let filas = prueba.app.disposicion().filas(Lista::ResultadosEjecuciones);
    assert_eq!(filas, 6);
    prueba.tecla(KeyCode::PageDown);
    assert_eq!(prueba.app.resultados.indice_seleccionada(), Some(6));
    a_la_vista(prueba.app.disposicion(), Lista::ResultadosEjecuciones, 6);
    // A 200×60 caben las doce: la página es más larga.
    prueba.redimensionar(200, 60);
    let filas = prueba.app.disposicion().filas(Lista::ResultadosEjecuciones);
    assert!(filas > 6, "{filas}");
    a_la_vista(prueba.app.disposicion(), Lista::ResultadosEjecuciones, 6);
    prueba.tecla(KeyCode::PageUp);
    assert_eq!(prueba.app.resultados.indice_seleccionada(), Some(0));
}

/// 200×60 → 80×24 → 40×12 → 200×60: selección, panel activo y host se
/// conservan y, donde la vista se pinta, se ven.
#[test]
fn resultados_la_secuencia_conserva_el_estado() {
    let (mut prueba, _) = abrir_resultados(Tema::respaldo());
    for _ in 0..9 {
        prueba.tecla(KeyCode::Down);
    }
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Down);
    let seleccionada = prueba.app.resultados.seleccionada;
    assert_eq!(prueba.app.resultados.indice_seleccionada(), Some(9));
    for (cols, filas) in [
        (200, 60),
        (80, 24),
        (40, 12),
        (200, 60),
        (60, 14),
        (100, 20),
    ] {
        prueba.pasar_por(cols, filas);
        let resultados = &prueba.app.resultados;
        assert_eq!(resultados.seleccionada, seleccionada);
        assert!(resultados.panel_hosts());
        assert_eq!(resultados.host_seleccionado, 1);
        let disposicion = prueba.app.disposicion();
        if (cols, filas) == (40, 12) {
            assert!(disposicion.aviso);
            continue;
        }
        a_la_vista(disposicion, Lista::ResultadosEjecuciones, 9);
        a_la_vista(disposicion, Lista::ResultadosHosts, 1);
    }
}

/// El visor registra sus dos flujos; las páginas son sus filas y, pegado al
/// final, sigue en el final aunque cambie el alto.
#[test]
fn resultados_el_visor_usa_las_filas_pintadas() {
    let mut prueba = resultados_en(80, 24, Tema::respaldo());
    prueba.tecla(KeyCode::Enter);
    let (ejecucion_id, host_id) = {
        let visor = prueba.app.resultados.visor.as_ref().expect("visor");
        (visor.ejecucion_id, visor.host_id)
    };
    let stdout: String = (0..60).map(|n| format!("línea {n}\n")).collect();
    prueba.servidor(MensajeServidor::Salida {
        ejecucion_id,
        host_id,
        stdout: SalidaRemota(stdout.into_bytes()),
        stderr: SalidaRemota(b"aviso\n".to_vec()),
        truncada: false,
    });
    let filas = prueba.app.disposicion().filas(Lista::VisorSalida);
    let ventana = prueba.app.disposicion().lista(Lista::VisorSalida).unwrap();
    assert_eq!((ventana.inicio, ventana.total), (0, 60));
    prueba.tecla(KeyCode::PageDown);
    let ventana = prueba.app.disposicion().lista(Lista::VisorSalida).unwrap();
    assert_eq!(ventana.inicio, filas);
    // Al final y más baja: la última línea sigue en la última fila.
    prueba.tecla(KeyCode::End);
    prueba.redimensionar(80, 16);
    let ventana = prueba.app.disposicion().lista(Lista::VisorSalida).unwrap();
    assert_eq!(ventana.inicio + ventana.filas, 60);
    assert!(prueba.texto().contains("línea 59"), "{}", prueba.texto());
    prueba.redimensionar(200, 60);
    let ventana = prueba.app.disposicion().lista(Lista::VisorSalida).unwrap();
    assert_eq!(ventana.inicio + ventana.filas, 60);
    assert!(ventana.filas > filas);
}

/// Entre un pintado y la tecla siguiente puede cerrarse un visor y abrirse
/// otro (con un tamaño pendiente no se pinta): el nuevo no hereda el
/// desplazamiento que se pintó para el anterior.
#[test]
fn resultados_un_visor_nuevo_no_hereda_el_desplazamiento_pintado() {
    let mut prueba = resultados_en(80, 24, Tema::respaldo());
    let salida = |prueba: &AppPrueba, lineas: usize| {
        let visor = prueba.app.resultados.visor.as_ref().expect("visor");
        let stdout: String = (0..lineas).map(|n| format!("línea {n}\n")).collect();
        Evento::Servidor(MensajeServidor::Salida {
            ejecucion_id: visor.ejecucion_id,
            host_id: visor.host_id,
            stdout: SalidaRemota(stdout.into_bytes()),
            stderr: SalidaRemota(Vec::new()),
            truncada: false,
        })
    };
    let sin_pintar = |prueba: &mut AppPrueba, codigo: KeyCode| {
        prueba
            .app
            .procesar(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
    };
    // Visor de hetzner-02 con 60 líneas, una página abajo (pintado).
    prueba.tecla(KeyCode::Enter);
    let evento = salida(&prueba, 60);
    prueba.evento(evento);
    prueba.tecla(KeyCode::PageDown);
    let filas = prueba.app.disposicion().filas(Lista::VisorSalida);
    let pintada = prueba.app.disposicion().lista(Lista::VisorSalida).unwrap();
    assert_eq!(pintada.inicio, filas);
    // Sin pintar: cierra, sube a hetzner-01, abre su visor, le llegan 50
    // líneas y una página abajo: empieza en la primera página, no en la
    // segunda del visor anterior.
    for codigo in [KeyCode::Esc, KeyCode::Up, KeyCode::Enter] {
        sin_pintar(&mut prueba, codigo);
    }
    let evento = salida(&prueba, 50);
    prueba.app.procesar(evento);
    sin_pintar(&mut prueba, KeyCode::PageDown);
    let visor = prueba.app.resultados.visor.as_ref().expect("visor");
    assert_eq!(visor.host, "hetzner-01");
    assert_eq!(visor.desplazamiento[0], filas);
}

#[test]
fn resultados_en_ascii_sin_glifos_unicode() {
    for (cols, filas) in TAMANOS.into_iter().chain([(60, 14), (100, 20), (99, 30)]) {
        let mut prueba = resultados_en(cols, filas, arnes::tema_ascii());
        let texto = prueba.texto();
        if prueba.app.disposicion().aviso {
            sin_unicode(&texto, &format!("resultados a {cols}×{filas}"));
            continue;
        }
        sin_unicode(&texto, &format!("resultados a {cols}×{filas}"));
        // Y el visor.
        prueba.tecla(KeyCode::Enter);
        sin_unicode(&prueba.texto(), &format!("visor a {cols}×{filas}"));
    }
}

// ---------------------------------------------------------------- diálogo MAGI

#[test]
fn magi_instantaneas() {
    for (cols, filas) in TAMANOS.into_iter().chain([(70, 18)]) {
        let prueba = magi_en(cols, filas, Tema::respaldo(), deliberacion_maqueta);
        prueba.instantanea(&format!("magi_{cols}x{filas}"));
    }
}

/// Compacto por debajo de 100 columnas o de 20 filas: nombres abreviados y
/// datos recortados; completo en la ventana grande.
#[test]
fn magi_compacto_en_estrecho_y_completo_en_grande() {
    let prueba = magi_en(70, 18, Tema::respaldo(), deliberacion_maqueta);
    let texto = prueba.texto();
    for esperado in [
        "DELIBERACIÓN MAGI ─ reiniciar nginx → 3",
        "M-1 salud",
        "B-2 backup",
        "C-3 tests",
        "✕ 31 h",
        "CONSENSO 6/7",
        "▸ BLOQUEADO",
        "↑↓ 1/3",
    ] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
    assert!(!texto.contains("MELCHIOR"), "{texto}");
    let prueba = magi_en(200, 60, Tema::respaldo(), deliberacion_maqueta);
    let texto = prueba.texto();
    assert!(texto.contains("MELCHIOR-1"), "{texto}");
    assert!(
        texto.contains("ACCIÓN: reiniciar nginx → 3 hosts"),
        "{texto}"
    );
    // Por debajo de 50×12, el aviso con lo que exige el diálogo.
    let prueba = magi_en(45, 20, Tema::respaldo(), deliberacion_maqueta);
    assert!(prueba.app.disposicion().aviso);
    assert!(
        prueba.texto().contains("mínimo 50×12"),
        "{}",
        prueba.texto()
    );
}

/// Los hosts se desplazan con ↑ ↓ y PgUp PgDn (las filas pintadas) y la
/// selección queda siempre a la vista.
#[test]
fn magi_los_hosts_se_desplazan() {
    let mut prueba = magi_en(50, 12, Tema::respaldo(), |_| deliberacion_larga(12));
    let filas = prueba.app.disposicion().filas(Lista::DeliberacionHosts);
    assert!(filas < 12, "{filas}");
    for _ in 0..10 {
        prueba.tecla(KeyCode::Down);
    }
    assert_eq!(prueba.app.deliberacion.as_ref().unwrap().seleccion, 10);
    a_la_vista(prueba.app.disposicion(), Lista::DeliberacionHosts, 10);
    assert!(prueba.texto().contains("web-11"), "{}", prueba.texto());
    assert!(prueba.texto().contains("11/12"), "{}", prueba.texto());
    prueba.tecla(KeyCode::PageUp);
    let seleccion = prueba.app.deliberacion.as_ref().unwrap().seleccion;
    assert_eq!(seleccion, 10 - filas);
    a_la_vista(
        prueba.app.disposicion(),
        Lista::DeliberacionHosts,
        seleccion,
    );
    prueba.tecla(KeyCode::PageDown);
    prueba.tecla(KeyCode::PageDown);
    assert_eq!(prueba.app.deliberacion.as_ref().unwrap().seleccion, 11);
    a_la_vista(prueba.app.disposicion(), Lista::DeliberacionHosts, 11);
}

/// 200×60 → 80×24 → 40×12 → 200×60 con el motivo a medio escribir: el
/// texto, la fase y la selección se conservan; el host seleccionado se ve.
#[test]
fn magi_la_secuencia_conserva_el_motivo_y_la_seleccion() {
    let mut prueba = magi_en(80, 24, Tema::respaldo(), |_| deliberacion_larga(12));
    for _ in 0..7 {
        prueba.tecla(KeyCode::Down);
    }
    // Bloqueada no está (todo aprueba): forzar solo desde bloqueada, así
    // que se usa la de la maqueta para el motivo.
    let mut prueba_motivo = magi_en(80, 24, Tema::respaldo(), deliberacion_maqueta);
    prueba_motivo.tecla(KeyCode::Down);
    prueba_motivo.tecla(KeyCode::Char('f'));
    for caracter in "backup revisado".chars() {
        prueba_motivo.tecla(KeyCode::Char(caracter));
    }
    for (cols, filas) in [(200, 60), (80, 24), (40, 12), (200, 60), (70, 18)] {
        prueba.pasar_por(cols, filas);
        prueba_motivo.pasar_por(cols, filas);
        let abierta = prueba.app.deliberacion.as_ref().unwrap();
        assert_eq!(abierta.seleccion, 7);
        let motivo = prueba_motivo.app.deliberacion.as_ref().unwrap();
        assert_eq!(motivo.maquina.fase, Fase::Motivo);
        assert_eq!(motivo.maquina.motivo.texto, "backup revisado");
        assert_eq!(motivo.seleccion, 1);
        if (cols, filas) == (40, 12) {
            assert!(prueba.app.disposicion().aviso);
            continue;
        }
        a_la_vista(prueba.app.disposicion(), Lista::DeliberacionHosts, 7);
        assert!(prueba.texto().contains("web-08"), "{}", prueba.texto());
        a_la_vista(prueba_motivo.app.disposicion(), Lista::DeliberacionHosts, 1);
        assert!(
            prueba_motivo.texto().contains("backup revisado"),
            "{}",
            prueba_motivo.texto()
        );
    }
}

#[test]
fn magi_en_ascii_sin_glifos_unicode() {
    let (prueba, sembrado) = AppPrueba::con_semilla_y_tema(80, 24, arnes::tema_ascii());
    let mut deliberaciones = vec![deliberacion_maqueta(&sembrado), deliberacion_larga(12)];
    let mut motivo = deliberacion_maqueta(&sembrado);
    motivo
        .maquina
        .pulsar(&crossterm::event::KeyEvent::from(KeyCode::Char('f')));
    for caracter in "ok".chars() {
        motivo
            .maquina
            .pulsar(&crossterm::event::KeyEvent::from(KeyCode::Char(caracter)));
    }
    deliberaciones.push(motivo);
    for deliberacion in &deliberaciones {
        for (cols, filas) in TAMANOS.into_iter().chain([(70, 18), (50, 12), (99, 30)]) {
            let texto = pintar_aparte(cols, filas, |marco, area| {
                let mut disp =
                    Disposicion::nueva(area, disposicion::minimo_de(Vista::Snippets, true));
                magi::ui::deliberacion::dibujar(marco, area, &prueba.app, deliberacion, &mut disp);
            });
            sin_unicode(&texto, &format!("el diálogo MAGI a {cols}×{filas}"));
        }
    }
    // Y la pantalla entera a 40×12: el aviso.
    let prueba = magi_en(40, 12, arnes::tema_ascii(), deliberacion_maqueta);
    sin_unicode(&prueba.texto(), "el aviso a 40×12");
}

// ---------------------------------------------------------------- EJECUTAR

#[test]
fn ejecutar_instantaneas() {
    for (cols, filas) in TAMANOS {
        let mut prueba = abrir_ejecutar(Tema::respaldo(), "espacio en disco");
        prueba.redimensionar(cols, filas);
        prueba.instantanea(&format!("ejecutar_{cols}x{filas}"));
    }
}

/// La rejilla tiene las columnas que caben (tres a 80, una a 40) y las
/// flechas recorren la que se ve.
#[test]
fn ejecutar_la_rejilla_se_adapta_al_ancho() {
    let mut prueba = abrir_ejecutar(Tema::respaldo(), "reiniciar nginx");
    let area = |prueba: &AppPrueba| prueba.app.disposicion().area;
    assert_eq!(prueba.app.columnas_rejilla(), 3);
    assert_eq!(
        magi::ui::ejecutar::columnas_rejilla(area(&prueba)),
        prueba.app.columnas_rejilla()
    );
    // Tres hosts en una fila: ↓ sale de la rejilla hacia «parar».
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    let cursor = |prueba: &AppPrueba| match &prueba.app.dialogo {
        Some(Dialogo::Ejecutar(dialogo)) => (dialogo.cursor_host(), dialogo.foco()),
        _ => panic!("sin diálogo EJECUTAR"),
    };
    assert_eq!(cursor(&prueba).0, 2);
    prueba.redimensionar(50, 16);
    assert_eq!(prueba.app.columnas_rejilla(), 2);
    // Dos columnas: vps-openclaw está en la segunda fila; ↑ sube a la
    // primera.
    prueba.tecla(KeyCode::Up);
    assert_eq!(cursor(&prueba).0, 0);
    prueba.redimensionar(40, 12);
    assert_eq!(prueba.app.columnas_rejilla(), 1);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    assert_eq!(cursor(&prueba).0, 2);
    let texto = prueba.texto();
    assert!(texto.contains("[x] vps-openclaw"), "{texto}");
    prueba.redimensionar(80, 24);
    prueba.tecla(KeyCode::Down);
    assert_eq!(
        cursor(&prueba).1,
        magi::app::lanzar::FocoEjecutar::Parar,
        "{}",
        prueba.texto()
    );
}

/// 200×60 → 80×24 → 40×12 → 200×60: marcados, cursor y foco se conservan
/// y el host con el cursor se ve siempre.
#[test]
fn ejecutar_la_secuencia_conserva_marcados_y_cursor() {
    let mut prueba = abrir_ejecutar(Tema::respaldo(), "reiniciar nginx");
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    for (cols, filas) in [(200, 60), (80, 24), (40, 12), (200, 60), (50, 12)] {
        prueba.pasar_por(cols, filas);
        let Some(Dialogo::Ejecutar(dialogo)) = &prueba.app.dialogo else {
            panic!("se cerró el diálogo");
        };
        assert_eq!(dialogo.cursor_host(), 2);
        assert_eq!(dialogo.marcados(), 2);
        assert!(!dialogo.marcado(0));
        let texto = prueba.texto();
        assert!(texto.contains("vps-openclaw"), "{cols}×{filas}:\n{texto}");
        assert!(texto.contains("continuar"), "{cols}×{filas}:\n{texto}");
    }
}

#[test]
fn ejecutar_en_ascii_sin_glifos_unicode() {
    for snippet in ["espacio en disco", "reiniciar nginx"] {
        let prueba = abrir_ejecutar(arnes::tema_ascii(), snippet);
        let Some(Dialogo::Ejecutar(dialogo)) = &prueba.app.dialogo else {
            panic!("sin diálogo");
        };
        for (cols, filas) in TAMANOS.into_iter().chain([(50, 12), (60, 14), (99, 30)]) {
            let texto = pintar_aparte(cols, filas, |marco, area| {
                magi::ui::ejecutar::dibujar(marco, area, &prueba.app, dialogo);
            });
            sin_unicode(&texto, &format!("EJECUTAR a {cols}×{filas}"));
        }
    }
}

// ---------------------------------------------------------------- formulario

#[test]
fn formulario_snippet_instantaneas() {
    for (cols, filas) in TAMANOS {
        let mut prueba = abrir_formulario(Tema::respaldo());
        prueba.redimensionar(cols, filas);
        prueba.instantanea(&format!("formulario_snippet_{cols}x{filas}"));
    }
}

/// El formulario nunca se sale de la terminal, el campo con el foco se ve
/// siempre y el texto escrito se conserva en la secuencia de tamaños.
#[test]
fn formulario_snippet_no_desborda_y_conserva_el_texto() {
    let mut prueba = abrir_formulario(Tema::respaldo());
    for caracter in "limpiar tmp".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    // Al campo del timeout (siete campos más abajo).
    for _ in 0..7 {
        prueba.tecla(KeyCode::Tab);
    }
    for (cols, filas) in [(200, 60), (80, 24), (40, 12), (200, 60), (60, 14)] {
        prueba.pasar_por(cols, filas);
        let Some(Dialogo::Snippets(DialogoSnippets::Formulario(formulario))) = &prueba.app.dialogo
        else {
            panic!("se cerró el formulario");
        };
        assert_eq!(formulario.nombre().texto, "limpiar tmp");
        let texto = prueba.texto();
        // El título, el campo con el foco y el pie: nada se ha quedado fuera.
        assert!(texto.contains("NUEVO SNIPPET"), "{cols}×{filas}:\n{texto}");
        assert!(texto.contains("Timeout"), "{cols}×{filas}:\n{texto}");
        assert!(texto.contains("guardar"), "{cols}×{filas}:\n{texto}");
    }
}

#[test]
fn formulario_snippet_en_ascii_sin_glifos_unicode() {
    let mut prueba = abrir_formulario(arnes::tema_ascii());
    for caracter in "limpiar tmp".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    // Recorre los campos: cada uno pinta su pista y su cursor.
    for campo in 0..9 {
        if campo > 0 {
            prueba.tecla(KeyCode::Tab);
        }
        let Some(Dialogo::Snippets(DialogoSnippets::Formulario(formulario))) = &prueba.app.dialogo
        else {
            panic!("se cerró el formulario");
        };
        for (cols, filas) in TAMANOS.into_iter().chain([(60, 14), (99, 30)]) {
            let texto = pintar_aparte(cols, filas, |marco, area| {
                magi::ui::formulario_snippet::dibujar(marco, area, &prueba.app, formulario);
            });
            sin_unicode(
                &texto,
                &format!("el formulario (campo {campo}) a {cols}×{filas}"),
            );
        }
    }
}

/// El desplegable de hosts abierto con un filtro escrito: en la secuencia de
/// tamaños se conservan el filtro y la opción resaltada, y los dos se ven
/// siempre (también a 50×12, el mínimo de Snippets).
#[test]
fn formulario_snippet_el_desplegable_conserva_filtro_y_resaltada() {
    let mut prueba = abrir_formulario(Tema::respaldo());
    // Al campo de hosts sueltos (cinco campos más abajo) y ↵ abre el
    // desplegable; «e» filtra y ↓ resalta la segunda que queda.
    for _ in 0..5 {
        prueba.tecla(KeyCode::Tab);
    }
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Char('e'));
    prueba.tecla(KeyCode::Down);
    let formulario = |prueba: &AppPrueba| match &prueba.app.dialogo {
        Some(Dialogo::Snippets(DialogoSnippets::Formulario(formulario))) => {
            let desplegable = formulario.desplegable_hosts();
            let resaltada = desplegable.filtradas()[desplegable.resaltado];
            (
                desplegable.abierto,
                desplegable.filtro.texto.clone(),
                desplegable.resaltado,
                desplegable.opciones[resaltada].etiqueta.clone(),
            )
        }
        _ => panic!("se cerró el formulario"),
    };
    let (abierto, filtro, resaltado, etiqueta) = formulario(&prueba);
    assert!(abierto);
    assert_eq!((filtro.as_str(), resaltado), ("e", 1));
    for (cols, filas) in [(200, 60), (80, 24), (40, 12), (200, 60), (50, 12), (60, 14)] {
        prueba.pasar_por(cols, filas);
        assert_eq!(
            formulario(&prueba),
            (true, "e".to_string(), 1, etiqueta.clone()),
            "{cols}×{filas}"
        );
        let texto = prueba.texto();
        assert!(
            texto.contains(&format!("▸ {etiqueta}")),
            "la resaltada no se ve a {cols}×{filas}:\n{texto}"
        );
        assert!(texto.contains("filtro: e"), "{cols}×{filas}:\n{texto}");
        assert!(texto.contains("guardar"), "{cols}×{filas}:\n{texto}");
    }
}
