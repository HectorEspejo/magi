//! Vistas SINCRONIZACIÓN y CONFLICTOS y sus diálogos (Fase 10): instantáneas a
//! 40×12, 80×24 y 200×60 (T45), mínimo, modo estrecho, ASCII sin glifos
//! Unicode, teclas de las dos vistas y que los diálogos no se cierran solos.
//!
//! La vista solo se abre desde la paleta (no hay tecla de función nueva): el
//! camino de las pruebas es Ctrl+P y teclear la consulta, como el usuario.

use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};
use magi::app::{AccionDialogo, AccionInventario, Dialogo};
use magi::sincronizacion::{BorradosPendientes, EstadoPasada, ResultadoPasada};
use magi::ui::disposicion::{Lista, Tamano, MINIMO_CONFLICTOS, MINIMO_SINCRONIZACION};
use magi::ui::Vista;
use zeroize::Zeroizing;

use crate::arnes::{self, AppPrueba};
use crate::vistas_g::FabricaDialogo;

/// Los tres tamaños obligatorios de la fase.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

/// Clave inventada: las instantáneas nunca llevan una clave real.
const CLAVE_FALSA: &str =
    "AGE-SECRET-KEY-1QQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQ";

/// Abre una vista desde la paleta escribiendo la consulta.
pub(crate) fn abrir_por_paleta(prueba: &mut AppPrueba, consulta: &str) {
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(prueba.app.paleta.is_some(), "la paleta no se abrió");
    for caracter in consulta.chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
}

/// Deja la App con la sincronización configurada y los equipos y conflictos
/// de la semilla.
fn preparar(prueba: &mut AppPrueba, estado: EstadoPasada) {
    prueba.app.probar_sincronizacion("~/Sync/magi", estado);
    prueba.paso_en(Instant::now());
}

/// Abre la vista SINCRONIZACIÓN con datos (para el barrido y las pruebas).
pub(crate) fn abrir_sincronizacion(prueba: &mut AppPrueba) {
    preparar(prueba, EstadoPasada::AlDia);
    abrir_por_paleta(prueba, "sincronización");
    assert_eq!(prueba.app.vista, Vista::Sincronizacion);
}

/// Abre la vista CONFLICTOS con datos.
pub(crate) fn abrir_conflictos(prueba: &mut AppPrueba) {
    preparar(prueba, EstadoPasada::AlDia);
    abrir_por_paleta(prueba, "conflictos");
    assert_eq!(prueba.app.vista, Vista::Conflictos);
}

// ---------------------------------------------------------------- instantáneas

#[test]
fn instantaneas_de_sincronizacion() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir_sincronizacion(&mut prueba);
        prueba.instantanea(&format!("sincronizacion_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_de_conflictos() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir_conflictos(&mut prueba);
        prueba.instantanea(&format!("conflictos_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_de_la_clave_con_qr() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        let clave = Zeroizing::new(CLAVE_FALSA.to_string());
        let qr = magi::sincronizacion::qr::lineas(clave.as_str()).ok();
        prueba.app.abrir_dialogo(Dialogo::ClaveSincronizacion {
            clave,
            qr,
            sincronizar_al_cerrar: false,
        });
        prueba.paso_en(Instant::now());
        prueba.instantanea(&format!("clave_sincronizacion_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_de_unir_este_equipo() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        prueba.app.abrir_dialogo(Dialogo::UnirEquipo {
            carpeta: PathBuf::from("/home/usuario/Sync/magi"),
            equipos: vec!["omarchy-portatil".to_string(), "mac-mini".to_string()],
            campo: magi::ui::componentes::CampoTexto::nuevo("AGE-SECRET-KEY-1QQQQ"),
            visible: false,
            error: None,
        });
        prueba.paso_en(Instant::now());
        prueba.instantanea(&format!("unir_equipo_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_de_importacion_con_borrados() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        prueba.app.abrir_dialogo(Dialogo::ImportacionBorrados {
            pendientes: BorradosPendientes {
                copia: PathBuf::from("/var/datos/magi/copias/magi-20261105-101200.db"),
                hosts: vec![
                    "web-1".to_string(),
                    "web-2".to_string(),
                    "db-1".to_string(),
                    "cache".to_string(),
                    "lb".to_string(),
                ],
                total_hosts: 23,
            },
        });
        prueba.paso_en(Instant::now());
        prueba.instantanea(&format!("importacion_borrados_{cols}x{filas}"));
    }
}

// ---------------------------------------------------------------- disposición

#[test]
fn los_minimos_son_50x12_y_entran_en_la_pantalla() {
    assert_eq!(MINIMO_SINCRONIZACION, Tamano::new(50, 12));
    assert_eq!(MINIMO_CONFLICTOS, Tamano::new(50, 12));
    for (cols, filas) in [(40, 12), (50, 11)] {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        preparar(&mut prueba, EstadoPasada::AlDia);
        abrir_por_paleta(&mut prueba, "sincronización");
        assert!(
            prueba.app.disposicion().aviso,
            "a {cols}×{filas} debe salir el aviso de tamaño"
        );
    }
    let (mut prueba, _) = AppPrueba::con_semilla(50, 12);
    preparar(&mut prueba, EstadoPasada::AlDia);
    abrir_por_paleta(&mut prueba, "sincronización");
    assert!(!prueba.app.disposicion().aviso, "a 50×12 debe verse");
}

#[test]
fn sin_configurar_se_ve_el_aviso_y_la_tecla_n() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir_por_paleta(&mut prueba, "sincronización");
    assert_eq!(prueba.app.vista, Vista::Sincronizacion);
    let texto = prueba.texto();
    assert!(texto.contains("desactivada"), "{texto}");
    assert!(texto.contains("n para elegir la carpeta"), "{texto}");
    // No hay indicador en la barra si la sincronización está desactivada.
    assert!(!texto.contains("sync") && !texto.contains("⇅"), "{texto}");
    prueba.tecla(KeyCode::Char('n'));
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::EntradaTexto { .. })),
        "n debe pedir la carpeta"
    );
}

#[test]
fn en_estrecho_caen_plataforma_y_version() {
    let (mut prueba, _) = AppPrueba::con_semilla(60, 20);
    preparar(&mut prueba, EstadoPasada::AlDia);
    abrir_por_paleta(&mut prueba, "sincronización");
    let texto = prueba.texto();
    assert!(!texto.contains("PLATAFORMA"), "{texto}");
    assert!(!texto.contains("MAGI "), "{texto}");
    assert!(
        texto.contains("EQUIPO") && texto.contains("ESTADO"),
        "{texto}"
    );
    assert!(texto.contains("omarchy-portatil"), "{texto}");
    assert!(texto.contains("ÚLTIMO FICH"), "{texto}");
    // A 200 columnas se ven todas.
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    preparar(&mut prueba, EstadoPasada::AlDia);
    abrir_por_paleta(&mut prueba, "sincronización");
    let texto = prueba.texto();
    for parte in ["EQUIPO", "PLATAFORMA", "MAGI", "ÚLTIMO FICH", "ESTADO"] {
        assert!(
            texto.contains(parte),
            "falta «{parte}»:
{texto}"
        );
    }
}

#[test]
fn la_vista_de_conflictos_ensena_el_detalle_del_seleccionado() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir_conflictos(&mut prueba);
    let texto = prueba.texto();
    assert!(texto.contains("CONFLICTOS DE SINCRONIZACIÓN"), "{texto}");
    assert!(texto.contains("huérfano: tunel «socks»"), "{texto}");
    assert!(texto.contains("d lo descarta"), "{texto}");
    // Bajando al conflicto de elemento se ve el campo que difiere.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    let texto = prueba.texto();
    assert!(texto.contains("campo «direccion»"), "{texto}");
    assert!(texto.contains("192.0.2.5"), "{texto}");
    // ↵ abre el detalle entero en un diálogo y esc lo cierra.
    prueba.tecla(KeyCode::Enter);
    assert!(matches!(prueba.app.dialogo, Some(Dialogo::Detalle { .. })));
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(prueba.app.vista, Vista::Conflictos);
}

#[test]
fn el_indicador_de_la_barra_cambia_con_el_estado() {
    let (mut prueba, _) = AppPrueba::con_semilla(120, 24);
    preparar(&mut prueba, EstadoPasada::AlDia);
    let texto = prueba.texto();
    assert!(texto.contains("⇅"), "{texto}");
    // Con conflictos pendientes el indicador los cuenta.
    prueba.app.probar_resultado_pasada(ResultadoPasada {
        estado: EstadoPasada::AlDia,
        resumen: None,
        exportado: false,
        conflictos_pendientes: 3,
        avisos: Vec::new(),
    });
    prueba.paso_en(Instant::now());
    assert!(
        prueba.texto().contains("⇅ 3 conflictos"),
        "{}",
        prueba.texto()
    );
    // Error de pasada.
    prueba
        .app
        .probar_resultado_pasada(ResultadoPasada::nuevo(EstadoPasada::Error(
            "la carpeta no existe".to_string(),
        )));
    prueba.paso_en(Instant::now());
    let texto = prueba.texto();
    assert!(texto.contains("⇅ error"), "{texto}");
    // ASCII: el indicador también se degrada.
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(120, 24, arnes::tema_ascii());
    preparar(&mut prueba, EstadoPasada::AlDia);
    prueba
        .app
        .probar_resultado_pasada(ResultadoPasada::nuevo(EstadoPasada::AlDia));
    prueba.paso_en(Instant::now());
    let texto = prueba.texto();
    assert!(texto.contains("sync ok"), "{texto}");
}

#[test]
fn en_ascii_no_hay_glifos_unicode_en_las_dos_vistas() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla_y_tema(cols, filas, arnes::tema_ascii());
        abrir_sincronizacion(&mut prueba);
        let texto = prueba.texto();
        assert!(
            texto
                .chars()
                .all(|caracter| caracter.is_ascii() || caracter.is_alphabetic()),
            "sincronización {cols}×{filas}:
{texto}"
        );
        prueba.tecla(KeyCode::Char('c'));
        let texto = prueba.texto();
        assert!(
            texto
                .chars()
                .all(|caracter| caracter.is_ascii() || caracter.is_alphabetic()),
            "conflictos {cols}×{filas}:
{texto}"
        );
    }
}

// ---------------------------------------------------------------- teclas

#[test]
fn q_vuelve_a_la_vista_anterior_y_c_no_pisa_el_destino() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    preparar(&mut prueba, EstadoPasada::AlDia);
    abrir_por_paleta(&mut prueba, "sincronización");
    prueba.tecla(KeyCode::Char('c'));
    assert_eq!(prueba.app.vista, Vista::Conflictos);
    let anterior = prueba.app.sincronizacion.vista_previa;
    prueba.tecla(KeyCode::Char('q'));
    assert_eq!(
        prueba.app.vista,
        anterior.unwrap_or(Vista::Flota),
        "q desde CONFLICTOS vuelve a la vista de la que se entró"
    );
}

#[test]
fn las_confirmaciones_peligrosas_piden_confirmacion_y_n_cancela() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    preparar(&mut prueba, EstadoPasada::AlDia);
    abrir_por_paleta(&mut prueba, "sincronización");
    // x sobre un equipo ajeno.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('x'));
    match &prueba.app.dialogo {
        Some(Dialogo::Confirmar {
            accion, peligro, ..
        }) => {
            assert!(*peligro);
            assert!(matches!(
                accion,
                AccionDialogo::Inventario(AccionInventario::QuitarEquipo(_))
            ));
        }
        _ => panic!("se esperaba la confirmación de quitar equipo"),
    }
    prueba.tecla(KeyCode::Char('n'));
    assert!(prueba.app.dialogo.is_none());
    // r y d también confirman antes de tocar la clave.
    prueba.tecla(KeyCode::Char('r'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Char('d'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    prueba.tecla(KeyCode::Esc);
    // k no muestra la clave sin confirmar.
    prueba.tecla(KeyCode::Char('k'));
    match &prueba.app.dialogo {
        Some(Dialogo::Confirmar { accion, .. }) => assert!(matches!(
            accion,
            AccionDialogo::Inventario(AccionInventario::MostrarClave)
        )),
        _ => panic!("k debe pedir confirmación"),
    }
}

#[test]
fn un_borrado_masivo_abre_el_dialogo_y_lo_deja_en_pausa() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    preparar(&mut prueba, EstadoPasada::AlDia);
    abrir_por_paleta(&mut prueba, "sincronización");
    prueba.app.probar_resultado_pasada(ResultadoPasada {
        estado: EstadoPasada::DecisionPendiente(BorradosPendientes {
            copia: PathBuf::from("/var/datos/magi/copias/magi-20261105-101200.db"),
            hosts: vec!["web-1".to_string(), "web-2".to_string()],
            total_hosts: 23,
        }),
        resumen: None,
        exportado: false,
        conflictos_pendientes: 0,
        avisos: Vec::new(),
    });
    prueba.paso_en(Instant::now());
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ImportacionBorrados { .. })
    ));
    assert!(prueba.app.sincronizacion.pausa.is_some());
    let texto = prueba.texto();
    assert!(texto.contains("IMPORTACIÓN CON BORRADOS"), "{texto}");
    assert!(texto.contains("web-1 · web-2"), "{texto}");
    // Esc deja la pausa y no aplica nada.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.app.sincronizacion.pausa.is_some());
    // s vuelve a abrir el diálogo porque sigue en pausa.
    prueba.tecla(KeyCode::Char('s'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ImportacionBorrados { .. })
    ));
}

#[test]
fn esperando_clave_ofrece_unir_y_quitado_ofrece_reincorporarse() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    preparar(&mut prueba, EstadoPasada::AlDia);
    abrir_por_paleta(&mut prueba, "sincronización");
    prueba
        .app
        .probar_resultado_pasada(ResultadoPasada::nuevo(EstadoPasada::EsperandoClave(
            "mac-mini".to_string(),
        )));
    prueba.paso_en(Instant::now());
    match &prueba.app.dialogo {
        Some(Dialogo::UnirEquipo { equipos, .. }) => {
            assert_eq!(equipos, &vec!["mac-mini".to_string()])
        }
        _ => panic!("EsperandoClave debe ofrecer UNIR"),
    }
    prueba.tecla(KeyCode::Esc);
    prueba
        .app
        .probar_resultado_pasada(ResultadoPasada::nuevo(EstadoPasada::Quitado));
    prueba.paso_en(Instant::now());
    match &prueba.app.dialogo {
        Some(Dialogo::Confirmar { accion, .. }) => assert!(matches!(
            accion,
            AccionDialogo::Inventario(AccionInventario::ReunirEquipo)
        )),
        _ => panic!("Quitado debe ofrecer reincorporarse"),
    }
}

// ---------------------------------------------------------------- diálogos

/// Fábricas de los diálogos nuevos para el barrido y la red de seguridad de
/// vistas_g.
pub(crate) fn dialogos() -> Vec<(&'static str, FabricaDialogo)> {
    vec![
        (
            "CLAVE DE SINCRONIZACIÓN",
            Box::new(|| Dialogo::ClaveSincronizacion {
                clave: Zeroizing::new(CLAVE_FALSA.to_string()),
                qr: magi::sincronizacion::qr::lineas(CLAVE_FALSA).ok(),
                sincronizar_al_cerrar: true,
            }),
        ),
        (
            "UNIR ESTE EQUIPO",
            Box::new(|| Dialogo::UnirEquipo {
                carpeta: PathBuf::from("/home/usuario/Sync/magi"),
                equipos: vec!["omarchy-portatil".to_string(), "mac-mini".to_string()],
                campo: magi::ui::componentes::CampoTexto::nuevo("AGE-SECRET-KEY-1QQQQ"),
                visible: false,
                error: None,
            }),
        ),
        (
            "IMPORTACIÓN CON BORRADOS",
            Box::new(|| Dialogo::ImportacionBorrados {
                pendientes: BorradosPendientes {
                    copia: PathBuf::from("/var/datos/magi/copias/magi-20261105-101200.db"),
                    hosts: vec![
                        "web-1".to_string(),
                        "web-2".to_string(),
                        "db-1".to_string(),
                        "cache".to_string(),
                        "lb".to_string(),
                    ],
                    total_hosts: 23,
                },
            }),
        ),
    ]
}

#[test]
fn los_dialogos_nuevos_no_se_cierran_solos() {
    for (titulo, fabrica) in dialogos() {
        let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
        prueba.app.abrir_dialogo(fabrica());
        prueba.paso_en(Instant::now());
        for codigo in [
            KeyCode::Up,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::Char('z'),
        ] {
            prueba.tecla(codigo);
            assert!(
                prueba.app.dialogo.is_some(),
                "«{titulo}» se cerró con {codigo:?}"
            );
        }
    }
}

#[test]
fn el_minimo_del_dialogo_de_la_clave_entra_en_la_pantalla() {
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    prueba.app.abrir_dialogo(Dialogo::ClaveSincronizacion {
        clave: Zeroizing::new(CLAVE_FALSA.to_string()),
        qr: magi::sincronizacion::qr::lineas(CLAVE_FALSA).ok(),
        sincronizar_al_cerrar: false,
    });
    prueba.paso_en(Instant::now());
    let disposicion = prueba.app.disposicion();
    assert!(!disposicion.aviso);
    assert_eq!(
        disposicion.minimo.map(|minimo| minimo.tamano),
        Some(Tamano::new(40, 12))
    );
    // El QR no cabe a 40×12: se avisa y no se pinta ningún glifo de QR.
    let texto = prueba.texto();
    assert!(!texto.contains("████"), "no debe haber QR a 40×12");
    // A 80×24 tampoco cabe entero, pero el aviso sí se ve.
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.app.abrir_dialogo(Dialogo::ClaveSincronizacion {
        clave: Zeroizing::new(CLAVE_FALSA.to_string()),
        qr: magi::sincronizacion::qr::lineas(CLAVE_FALSA).ok(),
        sincronizar_al_cerrar: false,
    });
    prueba.paso_en(Instant::now());
    let texto = prueba.texto();
    assert!(
        texto.contains("amplía la terminal para ver el QR"),
        "{texto}"
    );
}

#[test]
fn las_listas_nuevas_registran_su_ventana() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir_sincronizacion(&mut prueba);
    let ventana = prueba.app.disposicion().lista(Lista::Sincronizacion);
    assert!(ventana.is_some(), "falta la ventana de la lista de equipos");
    prueba.tecla(KeyCode::Char('c'));
    let ventana = prueba.app.disposicion().lista(Lista::Conflictos);
    assert!(
        ventana.is_some(),
        "falta la ventana de la lista de conflictos"
    );
}
