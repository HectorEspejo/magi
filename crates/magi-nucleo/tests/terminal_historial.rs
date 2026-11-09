//! Pruebas de la captura, la búsqueda, la selección y la copia del historial
//! del terminal (Fase 15).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use magi_nucleo::terminal::emulador::PantallaEmulada;
use magi_nucleo::terminal::historial::{
    buscar, capturar, lineas_de_historial, texto_plano, texto_seleccion, ClaseBusqueda,
    Coincidencia, ErrorBusqueda, LineaHistorial, BYTES_POR_CELDA,
};
use magi_nucleo::terminal::tramos::{Color, Tramo};

// --- Medida de memoria -----------------------------------------------------
// El tope de historial por MiB se calcula con BYTES_POR_CELDA (§7.1); la prueba
// mide lo que vt100 0.16 pide de verdad para que el número no se quede corto
// si el parser cambia. Las pruebas se turnan con un cerrojo para que la medida
// no vea asignaciones de otras.

static TURNO: Mutex<()> = Mutex::new(());
static ASIGNADOS: AtomicIsize = AtomicIsize::new(0);
static MIDIENDO: AtomicBool = AtomicBool::new(false);

struct Contador;

unsafe impl GlobalAlloc for Contador {
    unsafe fn alloc(&self, disposicion: Layout) -> *mut u8 {
        if MIDIENDO.load(Ordering::Relaxed) {
            ASIGNADOS.fetch_add(disposicion.size() as isize, Ordering::Relaxed);
        }
        unsafe { System.alloc(disposicion) }
    }

    unsafe fn dealloc(&self, puntero: *mut u8, disposicion: Layout) {
        if MIDIENDO.load(Ordering::Relaxed) {
            ASIGNADOS.fetch_sub(disposicion.size() as isize, Ordering::Relaxed);
        }
        unsafe { System.dealloc(puntero, disposicion) }
    }
}

#[global_allocator]
static ASIGNADOR: Contador = Contador;

fn turno() -> MutexGuard<'static, ()> {
    TURNO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn tramo(texto: &str) -> Tramo {
    Tramo {
        texto: texto.to_string(),
        primer_plano: Color::Predeterminado,
        fondo: Color::Predeterminado,
        negrita: false,
        cursiva: false,
        subrayado: false,
        inverso: false,
    }
}

fn fila(texto: &str, envuelta: bool) -> LineaHistorial {
    LineaHistorial {
        tramos: vec![tramo(texto)],
        envuelta,
    }
}

#[test]
fn el_tope_de_lineas_sale_del_informe() {
    let _turno = turno();
    // 64 MiB y 10 000 líneas por defecto (§7.1).
    assert_eq!(lineas_de_historial(10_000, 64, 167), 10_000);
    assert_eq!(lineas_de_historial(10_000, 64, 168), 9_986);
    assert_eq!(lineas_de_historial(10_000, 64, 200), 8_388);
    assert_eq!(lineas_de_historial(10_000, 64, 300), 5_592);
    // Las columnas por debajo de 2 se tratan como 2.
    assert_eq!(lineas_de_historial(10_000, 64, 0), 10_000);
    assert_eq!(lineas_de_historial(10_000, 64, 1), 10_000);
    // Sin líneas no hay historial, y manda el tope de memoria.
    assert_eq!(lineas_de_historial(0, 64, 200), 0);
    assert_eq!(lineas_de_historial(500, 8, 200), 500);
    assert_eq!(lineas_de_historial(50_000, 8, 200), 1_048);
    // Un tope de MiB disparatado no desborda.
    assert_eq!(lineas_de_historial(100, u64::MAX, 200), 100);
}

#[test]
fn los_bytes_por_celda_del_historial_no_pasan_del_tope() {
    let _turno = turno();
    const FILAS: u16 = 24;
    const COLUMNAS: u16 = 200;
    const LINEAS: usize = 2_000;
    MIDIENDO.store(true, Ordering::SeqCst);
    let base = ASIGNADOS.load(Ordering::SeqCst);
    let mut parser = vt100::Parser::new(FILAS, COLUMNAS, LINEAS);
    for _ in 0..(LINEAS + usize::from(FILAS) + 16) {
        parser.process(b"0123456789\r\n");
    }
    let asignados = ASIGNADOS.load(Ordering::SeqCst) - base;
    MIDIENDO.store(false, Ordering::SeqCst);
    // La medida solo vale con el historial lleno.
    parser.screen_mut().set_scrollback(usize::MAX);
    assert_eq!(parser.screen().scrollback(), LINEAS);
    let celdas = (LINEAS * usize::from(COLUMNAS)) as f64;
    let por_celda = asignados as f64 / celdas;
    assert!(
        por_celda > 16.0,
        "la medida salió casi vacía: {por_celda:.1} bytes por celda"
    );
    assert!(
        por_celda <= BYTES_POR_CELDA as f64,
        "vt100 0.16 pide {por_celda:.1} bytes por celda; sube BYTES_POR_CELDA y revisa el tope de §7.1"
    );
    std::hint::black_box(&parser);
}

#[test]
fn la_captura_lleva_el_historial_y_la_pantalla() {
    let _turno = turno();
    let mut pantalla = PantallaEmulada::nueva(4, 20, 100, false);
    pantalla.procesar(b"\x1b[31mrojo\x1b[0m\r\n");
    for linea in 0..8 {
        pantalla.procesar(format!("linea{linea}\r\n").as_bytes());
    }
    let historial = pantalla.lineas_historial();
    assert!(historial > 0, "el historial debería tener líneas");
    let captura = capturar(&mut pantalla);
    assert_eq!(captura.historial, historial);
    assert_eq!(captura.lineas.len(), historial + 4);
    // El historial empieza por lo más viejo, con su estilo.
    assert_eq!(captura.lineas[0].texto(), "rojo");
    assert_eq!(captura.lineas[0].tramos[0].primer_plano, Color::Indice(1));
    // Las últimas filas son la pantalla de ahora, con la de abajo vacía.
    assert_eq!(captura.lineas[captura.lineas.len() - 1].texto(), "");
    // Cada fila lleva el ancho completo en tramos; texto() recorta.
    for linea in &captura.lineas {
        let ancho: usize = linea
            .tramos
            .iter()
            .map(|tramo| tramo.texto.chars().count())
            .sum();
        assert_eq!(ancho, 20);
    }
    // La captura deja la vista al final.
    assert_eq!(pantalla.screen().scrollback(), 0);
}

#[test]
fn la_captura_no_lleva_la_pantalla_alternativa() {
    let _turno = turno();
    let mut pantalla = PantallaEmulada::nueva(4, 20, 100, false);
    for linea in 0..10 {
        pantalla.procesar(format!("principal{linea}\r\n").as_bytes());
    }
    let antes = capturar(&mut pantalla);
    assert!(antes.historial > 0);
    // Un programa a pantalla completa no tiene historial en MAGI.
    pantalla.procesar(b"\x1b[?1049h");
    pantalla.procesar(b"programa a pantalla completa");
    let alternativa = capturar(&mut pantalla);
    assert_eq!(alternativa.historial, 0);
    assert_eq!(alternativa.lineas.len(), 4);
    assert_eq!(alternativa.lineas[0].texto(), "programa a pantalla");
    assert_eq!(alternativa.lineas[1].texto(), "completa");
    assert!(
        !alternativa
            .lineas
            .iter()
            .any(|linea| linea.texto().starts_with("principal")),
        "la captura no puede mezclar el historial de la pantalla principal"
    );
    // Al salir vuelve la principal con su historial intacto.
    pantalla.procesar(b"\x1b[?1049l");
    let despues = capturar(&mut pantalla);
    assert_eq!(despues.historial, antes.historial);
    assert_eq!(despues.lineas[0].texto(), antes.lineas[0].texto());
}

#[test]
fn una_linea_de_trescientos_caracteres_en_el_historial_se_busca_y_se_copia_entera() {
    let _turno = turno();
    let mut pantalla = PantallaEmulada::nueva(4, 200, 100, false);
    let linea = format!("{}PALABRA{}", "a".repeat(196), "b".repeat(97));
    assert_eq!(linea.chars().count(), 300);
    pantalla.procesar(linea.as_bytes());
    pantalla.procesar(b"\r\n");
    for relleno in 0..12 {
        pantalla.procesar(format!("relleno{relleno}\r\n").as_bytes());
    }
    let captura = capturar(&mut pantalla);
    assert!(captura.historial > 0);
    // La palabra cruza el corte de las dos filas y se encuentra igual.
    let coincidencias = buscar(
        &captura.lineas,
        "palabra",
        ClaseBusqueda::Texto,
        captura.lineas.len() - 1,
    )
    .expect("buscar no falla");
    assert_eq!(coincidencias.len(), 1);
    let encontrada = coincidencias[0];
    assert_eq!(encontrada.inicio, 196);
    assert_eq!(encontrada.fin, 203);
    let primera = encontrada.linea;
    assert_eq!(captura.lineas[primera].texto(), "a".repeat(196) + "PALA");
    assert!(captura.lineas[primera].envuelta);
    assert_eq!(
        captura.lineas[primera + 1].texto(),
        format!("BRA{}", "b".repeat(97))
    );
    // Al copiarla sale en una sola línea, sin el salto de la envoltura.
    let copiada = texto_seleccion(&captura.lineas, (primera, 0), (primera + 1, 99), false);
    assert_eq!(copiada, linea);
    assert!(!copiada.contains('\n'));
    // Y el texto plano para guardar también la deja en un renglón.
    let plano = texto_plano(&captura.lineas);
    assert!(plano.lines().any(|renglon| renglon == linea));
}

#[test]
fn la_captura_cuenta_bien_las_columnas_con_caracteres_anchos() {
    let _turno = turno();
    let mut pantalla = PantallaEmulada::nueva(2, 20, 10, false);
    pantalla.procesar("\x1b[31m日\x1b[0m fin".as_bytes());
    let captura = capturar(&mut pantalla);
    assert_eq!(captura.lineas[0].texto(), "日 fin");
    // 日 ocupa dos celdas: el texto de la fila tiene 19 caracteres, no 20.
    let caracteres: usize = captura.lineas[0]
        .tramos
        .iter()
        .map(|tramo| tramo.texto.chars().count())
        .sum();
    assert_eq!(caracteres, 19);
}

#[test]
fn la_busqueda_va_de_la_vista_hacia_arriba() {
    let _turno = turno();
    let lineas = vec![
        fila("Error de conexión", false),
        fila("error de red", false),
        fila("ERROR grave", false),
    ];
    let todas = buscar(&lineas, "error", ClaseBusqueda::Texto, 2).expect("buscar no falla");
    assert_eq!(
        todas,
        vec![
            Coincidencia {
                linea: 2,
                inicio: 0,
                fin: 5
            },
            Coincidencia {
                linea: 1,
                inicio: 0,
                fin: 5
            },
            Coincidencia {
                linea: 0,
                inicio: 0,
                fin: 5
            },
        ]
    );
    // Con mayúsculas en el patrón, la búsqueda distingue.
    let sensible = buscar(&lineas, "Error", ClaseBusqueda::Texto, 2).expect("buscar no falla");
    assert_eq!(
        sensible,
        vec![Coincidencia {
            linea: 0,
            inicio: 0,
            fin: 5
        }]
    );
    // Desde una fila intermedia solo se mira lo que está por encima.
    let desde_una = buscar(&lineas, "ERROR", ClaseBusqueda::Texto, 1).expect("buscar no falla");
    assert!(desde_una.is_empty());
    // Dos coincidencias en una línea, de izquierda a derecha.
    let repetida = vec![fila("error error", false)];
    let dos = buscar(&repetida, "error", ClaseBusqueda::Texto, 0).expect("buscar no falla");
    assert_eq!(
        dos,
        vec![
            Coincidencia {
                linea: 0,
                inicio: 0,
                fin: 5
            },
            Coincidencia {
                linea: 0,
                inicio: 6,
                fin: 11
            },
        ]
    );
    // `desde` fuera de la captura se acota a la última fila.
    let acotada = buscar(&lineas, "ERROR", ClaseBusqueda::Texto, 99).expect("buscar no falla");
    assert_eq!(acotada.len(), 1);
}

#[test]
fn la_busqueda_une_las_lineas_envueltas() {
    let _turno = turno();
    let lineas = vec![
        fila("hola mundo", false),
        fila("esta es una linea que", true),
        fila("continua y trae aguja", false),
        fila("final", false),
    ];
    let coincidencias = buscar(&lineas, "aguja", ClaseBusqueda::Texto, 3).expect("buscar no falla");
    assert_eq!(
        coincidencias,
        vec![Coincidencia {
            linea: 1,
            inicio: 37,
            fin: 42
        }]
    );
    // Si `desde` cae dentro de la línea envuelta, se busca entera.
    let desde_continuacion =
        buscar(&lineas, "aguja", ClaseBusqueda::Texto, 2).expect("buscar no falla");
    assert_eq!(desde_continuacion, coincidencias);
    // Una coincidencia que empieza en la primera fila y acaba en la segunda.
    let cruzando =
        buscar(&lineas, "quecontinua", ClaseBusqueda::Texto, 3).expect("buscar no falla");
    assert_eq!(
        cruzando,
        vec![Coincidencia {
            linea: 1,
            inicio: 18,
            fin: 29
        }]
    );
}

#[test]
fn la_busqueda_dice_por_que_no_puede_buscar() {
    let _turno = turno();
    let lineas = vec![fila("algo", false)];
    assert_eq!(
        buscar(&lineas, "", ClaseBusqueda::Texto, 0),
        Err(ErrorBusqueda::Vacio)
    );
    let largo = "x".repeat(257);
    assert_eq!(
        buscar(&lineas, &largo, ClaseBusqueda::Texto, 0),
        Err(ErrorBusqueda::DemasiadoLargo)
    );
    let justo = "x".repeat(256);
    assert!(buscar(&lineas, &justo, ClaseBusqueda::Texto, 0).is_ok());
    let invalida = buscar(&lineas, "(", ClaseBusqueda::Expresion, 0).unwrap_err();
    match invalida {
        ErrorBusqueda::Invalida(mensaje) => {
            assert!(!mensaje.is_empty());
            assert!(!mensaje.contains('\n'), "el mensaje va en la barra");
            assert!(mensaje.contains("regex"), "mensaje legible: {mensaje}");
        }
        otro => panic!("se esperaba una expresión inválida, llegó {otro:?}"),
    }
}

#[test]
fn la_expresion_regular_busca_y_respeta_las_mayusculas() {
    let _turno = turno();
    let lineas = vec![
        fila("GET /api/pedidos 200", false),
        fila("POST /api/pedidos 500", false),
        fila("GET /api/clientes 404", false),
    ];
    let numeros =
        buscar(&lineas, r"\b[45]\d\d\b", ClaseBusqueda::Expresion, 2).expect("la expresión vale");
    assert_eq!(
        numeros,
        vec![
            Coincidencia {
                linea: 2,
                inicio: 18,
                fin: 21
            },
            Coincidencia {
                linea: 1,
                inicio: 18,
                fin: 21
            },
        ]
    );
    // El patrón en minúsculas encuentra también las mayúsculas.
    let insensible = buscar(&lineas, "get", ClaseBusqueda::Expresion, 2).expect("vale");
    assert_eq!(insensible.len(), 2);
    let sensible = buscar(&lineas, "GET", ClaseBusqueda::Expresion, 2).expect("vale");
    assert_eq!(sensible.len(), 2);
}

#[test]
fn la_busqueda_se_corta_en_diez_mil_coincidencias() {
    let _turno = turno();
    let lineas: Vec<LineaHistorial> = (0..10_001).map(|_| fila("a", false)).collect();
    let coincidencias =
        buscar(&lineas, "a", ClaseBusqueda::Texto, lineas.len() - 1).expect("buscar no falla");
    assert_eq!(coincidencias.len(), 10_000);
    assert_eq!(
        coincidencias[0],
        Coincidencia {
            linea: 10_000,
            inicio: 0,
            fin: 1
        }
    );
}

#[test]
fn la_seleccion_por_caracteres_y_por_lineas() {
    let _turno = turno();
    let lineas = vec![
        fila("hola mundo", false),
        fila("segunda linea", false),
        fila("tercera", false),
    ];
    assert_eq!(texto_seleccion(&lineas, (0, 0), (0, 3), false), "hola");
    assert_eq!(
        texto_seleccion(&lineas, (0, 5), (1, 2), false),
        "mundo\nseg"
    );
    // Del revés se ordena.
    assert_eq!(
        texto_seleccion(&lineas, (1, 3), (0, 5), false),
        "mundo\nsegu"
    );
    // Por líneas coge las filas enteras, también la última.
    assert_eq!(
        texto_seleccion(&lineas, (0, 3), (2, 1), true),
        "hola mundo\nsegunda linea\ntercera"
    );
    // Las columnas fuera del texto se recortan.
    assert_eq!(texto_seleccion(&lineas, (0, 100), (0, 200), false), "");
    assert_eq!(
        texto_seleccion(&lineas, (0, 0), (0, 100), false),
        "hola mundo"
    );
    assert_eq!(
        texto_seleccion(&lineas, (0, 0), (9, 9), false),
        "hola mundo\nsegunda linea\ntercera"
    );
    // Una selección vacía de una captura vacía.
    assert_eq!(texto_seleccion(&[], (0, 0), (0, 0), false), "");
}

#[test]
fn la_seleccion_une_las_lineas_envueltas() {
    let _turno = turno();
    let lineas = vec![
        fila("primera", false),
        fila("abcdefghij", true),
        fila("klmnop", false),
        fila("otra", false),
    ];
    assert_eq!(
        texto_seleccion(&lineas, (1, 0), (2, 5), false),
        "abcdefghijklmnop"
    );
    assert_eq!(texto_seleccion(&lineas, (1, 3), (1, 6), false), "defg");
    // Por líneas, las envueltas van juntas y la última fila entera.
    assert_eq!(
        texto_seleccion(&lineas, (1, 4), (2, 0), true),
        "abcdefghijklmnop"
    );
}

#[test]
fn el_texto_plano_pone_una_linea_logica_por_renglon() {
    let _turno = turno();
    let lineas = vec![
        fila("primera   ", false),
        fila("envuelta", true),
        fila("y sigue", false),
        fila("", false),
    ];
    assert_eq!(texto_plano(&lineas), "primera\nenvueltay sigue\n\n");
    assert_eq!(texto_plano(&[]), "");
}

#[test]
fn los_tramos_de_una_fila_son_publicos_para_la_captura() {
    let _turno = turno();
    // La función pública que usa el historial rellena todo el ancho.
    let mut parser = vt100::Parser::new(1, 10, 0);
    parser.process(b"hola");
    let tramos = magi_nucleo::terminal::tramos::tramos_de_fila(parser.screen(), 0, 10);
    let texto: String = tramos.iter().map(|tramo| tramo.texto.as_str()).collect();
    assert_eq!(texto, "hola      ");
}

#[test]
fn la_envoltura_cruza_del_historial_a_la_pantalla() {
    let _turno = turno();
    let mut pantalla = PantallaEmulada::nueva(2, 10, 100, false);
    pantalla.procesar(b"0123456789ABCDE");
    // Un salto justo después manda a la fila envuelta al historial y deja su
    // continuación al principio de la pantalla.
    pantalla.procesar(b"\r\n");
    let captura = capturar(&mut pantalla);
    assert_eq!(captura.historial, 1);
    assert_eq!(captura.lineas[0].texto(), "0123456789");
    assert!(captura.lineas[0].envuelta);
    assert_eq!(captura.lineas[1].texto(), "ABCDE");
    assert!(!captura.lineas[1].envuelta);
    assert_eq!(texto_plano(&captura.lineas), "0123456789ABCDE\n\n");
}
