//! Instantáneas y pruebas de disposición del grupo G: diálogos, ayuda y paleta.
//!
//! Los diálogos encogen al área con desplazamiento: los informativos con
//! ↑ ↓ PgUp PgDn (indicador «↑↓ i/n»), los de formulario siguiendo al foco.
//! La ayuda y la paleta hacen lo mismo con sus listas.

use crossterm::event::{KeyCode, KeyModifiers};

use magi::app::{
    AccionDialogo, Dialogo, EntradaTextoAccion, EstadoGeneracion, ExportacionRegistro,
    FormularioTunel,
};
use magi::protocolo::MensajeServidor;
use magi::ui::componentes::{CampoTexto, Desplegable, Opcion, ValorOpcion};
use magi::ui::disposicion::Lista;
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};

/// Los tres tamaños de referencia de la fase.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];
/// La secuencia de cambios que debe conservar el estado.
const SECUENCIA: [(u16, u16); 4] = [(200, 60), (80, 24), (40, 12), (200, 60)];
/// Huella de las maquetas de huella desconocida.
const HUELLA: &str = "SHA256:Ab3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9";

fn nombre(base: &str, cols: u16, filas: u16) -> String {
    format!("{base}_{cols}x{filas}")
}

/// Instantánea como la del arnés, pero sin los segundos de «sondeo hace N s»
/// del pie de Flota: esa vista de fondo cuenta segundos reales y la prueba
/// puede cruzar uno. Se sustituyen por `NN`, del mismo ancho que los dos
/// dígitos de la semilla, para no descuadrar el borde en la instantánea.
fn instantanea(prueba: &AppPrueba, nombre: &str) {
    let texto = prueba.texto();
    let mut ajustes = insta::Settings::clone_current();
    ajustes.set_snapshot_path("../snapshots");
    ajustes.set_prepend_module_to_snapshot(false);
    ajustes.set_omit_expression(true);
    ajustes.add_filter(r"/tmp/magi[A-Za-z0-9]{6}", "[tmp]");
    ajustes.add_filter(r"hace \d+ s ", "hace NN s ");
    ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
}

fn solo_ascii(texto: &str) -> bool {
    texto.chars().all(|c| c.is_ascii() || c.is_alphabetic())
}

/// Los glifos que sobran, para el mensaje de fallo.
fn glifos_unicode(texto: &str) -> String {
    texto
        .chars()
        .filter(|c| !(c.is_ascii() || c.is_alphabetic()))
        .map(|c| format!("{c:?} "))
        .collect()
}

/// Hosts con la selección en hetzner-01 (la primera fila es su grupo).
fn a_hosts(prueba: &mut AppPrueba) {
    prueba.tecla(KeyCode::F(2));
    assert_eq!(prueba.app.vista, Vista::Hosts);
    prueba.tecla(KeyCode::Down);
}

/// «BORRAR HOST» sobre hetzner-01, por el camino de teclas real.
fn abrir_confirmar(prueba: &mut AppPrueba) {
    a_hosts(prueba);
    prueba.tecla(KeyCode::Char('x'));
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Confirmar { .. })),
        "no se abrió la confirmación"
    );
}

/// «NUEVO GRUPO» (menú de grupo → nuevo) con texto a medio escribir.
fn abrir_entrada_texto(prueba: &mut AppPrueba, texto: &str) {
    a_hosts(prueba);
    prueba.tecla(KeyCode::Char('g'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::MenuGrupo { .. })
    ));
    prueba.tecla(KeyCode::Enter);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::EntradaTexto { .. })
    ));
    for caracter in texto.chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
}

/// La huella desconocida que pregunta el servidor al abrir una sesión.
fn abrir_huella(prueba: &mut AppPrueba) {
    prueba.servidor(MensajeServidor::HuellaDesconocida {
        sesion_id: 1,
        host: "hetzner-01".to_string(),
        tipo_clave: "ssh-ed25519".to_string(),
        huella: HUELLA.to_string(),
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::HuellaServidor { .. })
    ));
}

/// Detalle largo: 64 líneas numeradas, más de las que caben a 200×60.
fn detalle_largo() -> Dialogo {
    Dialogo::Detalle {
        titulo: "DETALLE DEL REGISTRO".to_string(),
        lineas: (1..=64)
            .map(|numero| format!("línea {numero:02} · sondeo de hetzner-01 con carga 0.42"))
            .collect(),
        tunel_caido: None,
    }
}

/// Abre un diálogo sin camino de teclas sencillo y lo pinta, como haría el
/// bucle antes de la siguiente tecla.
pub(crate) fn abrir_dialogo(prueba: &mut AppPrueba, dialogo: Dialogo) {
    prueba.app.abrir_dialogo(dialogo);
    prueba.paso_en(std::time::Instant::now());
}

fn abrir_detalle(prueba: &mut AppPrueba) {
    abrir_dialogo(prueba, detalle_largo());
}

fn abrir_paleta(prueba: &mut AppPrueba) {
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(prueba.app.paleta.is_some(), "no se abrió la paleta");
}

fn abrir_ayuda_hosts(prueba: &mut AppPrueba) {
    a_hosts(prueba);
    prueba.tecla(KeyCode::Char('?'));
    assert!(prueba.app.ayuda);
}

fn abrir_ayuda_flota(prueba: &mut AppPrueba) {
    assert_eq!(prueba.app.vista, Vista::Flota);
    prueba.tecla(KeyCode::Char('?'));
    assert!(prueba.app.ayuda);
}

/// Cada caso de instantánea: nombre, título del recuadro y cómo se abre.
type Abrir = fn(&mut AppPrueba);

fn casos() -> Vec<(&'static str, &'static str, Abrir)> {
    vec![
        ("paleta", "PALETA", abrir_paleta),
        ("ayuda_hosts", "AYUDA", abrir_ayuda_hosts),
        ("ayuda_flota", "AYUDA", abrir_ayuda_flota),
        ("confirmar", "BORRAR HOST", abrir_confirmar),
        ("detalle", "DETALLE DEL REGISTRO", |prueba| {
            abrir_detalle(prueba);
            // Con desplazamiento: una página abajo.
            prueba.tecla(KeyCode::PageDown);
        }),
        ("entrada_texto", "NUEVO GRUPO", |prueba| {
            abrir_entrada_texto(prueba, "servidores de casa")
        }),
        ("huella_desconocida", "HUELLA DESCONOCIDA", abrir_huella),
    ]
}

/// Esquinas superior izquierda, superior derecha e inferior izquierda.
const ESQUINAS_UNICODE: [char; 3] = ['┌', '┐', '└'];
const ESQUINAS_ASCII: [char; 3] = ['+', '+', '+'];

/// Posición de un recuadro en el texto pintado: columnas y filas de sus
/// esquinas (x0, y0, x1, y1), buscado por su título. `None` en y1 si no se
/// ve el borde de abajo.
fn posicion_recuadro(
    texto: &str,
    titulo: &str,
    esquinas: [char; 3],
) -> (usize, usize, usize, Option<usize>) {
    let lineas: Vec<Vec<char>> = texto.lines().map(|linea| linea.chars().collect()).collect();
    let buscado: Vec<char> = format!("{} {titulo}", esquinas[0]).chars().collect();
    let (y0, x0) = lineas
        .iter()
        .enumerate()
        .find_map(|(y, linea)| {
            linea
                .windows(buscado.len())
                .position(|ventana| ventana == buscado.as_slice())
                .map(|x| (y, x))
        })
        .unwrap_or_else(|| panic!("sin recuadro «{titulo}»:\n{texto}"));
    let x1 = (x0 + 1..lineas[y0].len())
        .find(|x| lineas[y0][*x] == esquinas[1])
        .unwrap_or_else(|| panic!("recuadro «{titulo}» sin esquina derecha:\n{texto}"));
    let y1 = (y0 + 1..lineas.len()).find(|y| lineas[*y].get(x0) == Some(&esquinas[2]));
    (x0, y0, x1, y1)
}

/// Lo que se ve dentro del recuadro de un diálogo, línea a línea.
fn recuadro(texto: &str, titulo: &str, esquinas: [char; 3]) -> Vec<String> {
    let (x0, y0, x1, y1) = posicion_recuadro(texto, titulo, esquinas);
    let lineas: Vec<Vec<char>> = texto.lines().map(|linea| linea.chars().collect()).collect();
    let y1 = y1.unwrap_or(lineas.len() - 1);
    lineas[y0..=y1]
        .iter()
        .map(|linea| linea.iter().skip(x0).take(x1 - x0 + 1).collect())
        .collect()
}

// ---------------------------------------------------------------- instantáneas

#[test]
fn instantaneas_a_los_tres_tamanos() {
    for (base, _, abrir) in casos() {
        for (cols, filas) in TAMANOS {
            let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
            abrir(&mut prueba);
            instantanea(&prueba, &nombre(base, cols, filas));
        }
    }
}

/// Un diálogo abierto encima del aviso de tamaño: se recoloca y encoge.
#[test]
fn instantanea_dialogo_sobre_el_aviso() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir_confirmar(&mut prueba);
    prueba.redimensionar(30, 10);
    assert!(prueba.app.disposicion().aviso, "sin aviso a 30×10");
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar { .. })
    ));
    let texto = prueba.texto();
    assert!(texto.contains("BORRAR HOST"), "{texto}");
    assert!(texto.contains("s confirmar"), "{texto}");
    instantanea(&prueba, "dialogo_sobre_aviso_30x10");
}

/// Todos los casos se recolocan en la secuencia 200×60 → 80×24 → 40×12 →
/// 200×60: siguen abiertos, con el recuadro entero (sin cortar), centrado y
/// sin pasar del ancho máximo de un diálogo.
#[test]
fn todos_se_recolocan_centrados_en_la_secuencia() {
    for (base, titulo, abrir) in casos() {
        let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
        abrir(&mut prueba);
        for (cols, filas) in SECUENCIA {
            prueba.pasar_por(cols, filas);
            let abierto =
                prueba.app.dialogo.is_some() || prueba.app.paleta.is_some() || prueba.app.ayuda;
            assert!(abierto, "{base} se cerró a {cols}×{filas}");
            let texto = prueba.texto();
            let (x0, y0, x1, y1) = posicion_recuadro(&texto, titulo, ESQUINAS_UNICODE);
            let y1 = y1.unwrap_or_else(|| panic!("{base} cortado a {cols}×{filas}:\n{texto}"));
            let (cols, filas) = (usize::from(cols), usize::from(filas));
            assert!(
                x1 - x0 < usize::from(magi::ui::disposicion::ANCHO_MAX_DIALOGO),
                "{base} demasiado ancho a {cols}×{filas}"
            );
            assert!(
                x0.abs_diff(cols - 1 - x1) <= 1 && y0.abs_diff(filas - 1 - y1) <= 1,
                "{base} descentrado a {cols}×{filas}:\n{texto}"
            );
        }
    }
}

// ---------------------------------------------------------------- ASCII

/// Con el tema ASCII, ningún diálogo, ni la paleta ni la ayuda pintan glifos
/// Unicode: a los tres tamaños, en estrecho (70×20) y encima del aviso. Se
/// mira el recuadro del diálogo: la vista de fondo es de otros grupos.
#[test]
fn en_ascii_no_hay_glifos_unicode() {
    let comprobar = |prueba: &AppPrueba, base: &str, titulo: &str| {
        let texto = prueba.texto();
        let dentro = recuadro(&texto, titulo, ESQUINAS_ASCII).join("\n");
        assert!(
            solo_ascii(&dentro),
            "glifo Unicode en {base} a {:?} ({}):\n{dentro}",
            prueba.tamano_pintado(),
            glifos_unicode(&dentro)
        );
    };
    for (base, titulo, abrir) in casos() {
        for (cols, filas) in TAMANOS {
            let (mut prueba, _) = AppPrueba::con_semilla_y_tema(cols, filas, arnes::tema_ascii());
            abrir(&mut prueba);
            comprobar(&prueba, base, titulo);
        }
        // Abierto a 80×24 y recolocado en estrecho y por debajo del mínimo.
        let (mut prueba, _) = AppPrueba::con_semilla_y_tema(80, 24, arnes::tema_ascii());
        abrir(&mut prueba);
        for (cols, filas) in [(70, 20), (30, 10), (40, 12)] {
            prueba.pasar_por(cols, filas);
            comprobar(&prueba, base, titulo);
        }
    }
}

#[test]
fn en_ascii_el_indicador_es_ascii() {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(80, 24, arnes::tema_ascii());
    abrir_detalle(&mut prueba);
    let texto = prueba.texto();
    assert!(texto.contains("^v 1/"), "{texto}");
    assert!(texto.contains("enter / esc cerrar"), "{texto}");
}

// ---------------------------------------------------------------- comportamiento

/// Ventana del diálogo en el último pintado.
fn ventana_modal(prueba: &AppPrueba) -> magi::ui::disposicion::VentanaLista {
    prueba
        .app
        .disposicion()
        .lista(Lista::Modal)
        .expect("el diálogo no registró su ventana")
}

fn pulsar(prueba: &mut AppPrueba, codigo: KeyCode, veces: usize) {
    for _ in 0..veces {
        prueba.tecla(codigo);
    }
}

/// Un diálogo informativo largo se desplaza con ↑ ↓ PgUp PgDn dentro de lo
/// que se ve, sin zona muerta al final, y sus teclas propias siguen valiendo.
#[test]
fn el_detalle_se_desplaza_con_flechas_y_paginas() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir_detalle(&mut prueba);
    let ventana = ventana_modal(&prueba);
    assert_eq!((ventana.inicio, ventana.total), (0, 64));
    let filas = ventana.filas;
    let maximo = 64 - filas;
    let texto = prueba.texto();
    assert!(texto.contains(&format!("↑↓ 1/{}", maximo + 1)), "{texto}");
    assert!(texto.contains("línea 01"), "{texto}");

    prueba.tecla(KeyCode::Down);
    assert_eq!(prueba.app.desplazamiento_modal, 1);
    let texto = prueba.texto();
    assert!(
        !texto.contains("línea 01") && texto.contains("línea 02"),
        "{texto}"
    );

    prueba.tecla(KeyCode::PageDown);
    assert_eq!(prueba.app.desplazamiento_modal, 1 + filas);
    assert_eq!(ventana_modal(&prueba).inicio, 1 + filas);

    // Al final se queda en el máximo y la última línea se ve.
    pulsar(&mut prueba, KeyCode::Down, 100);
    assert_eq!(prueba.app.desplazamiento_modal, maximo);
    let texto = prueba.texto();
    assert!(texto.contains("línea 64"), "{texto}");
    assert!(
        texto.contains(&format!("↑↓ {0}/{0}", maximo + 1)),
        "{texto}"
    );
    // Sin zona muerta: una flecha arriba sube ya.
    prueba.tecla(KeyCode::Up);
    assert_eq!(prueba.app.desplazamiento_modal, maximo - 1);
    pulsar(&mut prueba, KeyCode::PageUp, 10);
    assert_eq!(prueba.app.desplazamiento_modal, 0);
    assert!(matches!(prueba.app.dialogo, Some(Dialogo::Detalle { .. })));

    // Esc sigue cerrando y el siguiente diálogo empieza arriba.
    pulsar(&mut prueba, KeyCode::Down, 3);
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(prueba.app.desplazamiento_modal, 0);
    abrir_detalle(&mut prueba);
    assert_eq!(ventana_modal(&prueba).inicio, 0);
}

/// Si todo cabe no hay indicador y las flechas no mueven nada.
#[test]
fn si_cabe_no_hay_indicador_ni_desplazamiento() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    abrir_confirmar(&mut prueba);
    let ventana = ventana_modal(&prueba);
    assert!(ventana.total <= ventana.filas, "{ventana:?}");
    pulsar(&mut prueba, KeyCode::Down, 3);
    assert_eq!(prueba.app.desplazamiento_modal, 0);
    let texto = prueba.texto();
    assert!(!texto.contains(" ↑↓ "), "{texto}");
    // Y la confirmación sigue ahí con su texto.
    assert!(texto.contains("¿Borrar el host «hetzner-01»?"), "{texto}");
}

/// Desplazar no rompe las teclas propias: `n` cancela y `s` confirma una
/// confirmación larga; `a` acepta la huella aunque esté desplazada.
#[test]
fn desplazar_conserva_las_teclas_de_cada_dialogo() {
    let larga = || Dialogo::Confirmar {
        titulo: "CONFIRMAR".to_string(),
        lineas: (1..=20)
            .map(|numero| format!("consecuencia {numero}"))
            .collect(),
        peligro: true,
        accion: AccionDialogo::Nada,
    };
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    abrir_dialogo(&mut prueba, larga());
    prueba.tecla(KeyCode::Down);
    assert_eq!(prueba.app.desplazamiento_modal, 1);
    prueba.tecla(KeyCode::Char('n'));
    assert!(prueba.app.dialogo.is_none(), "n no canceló");
    abrir_dialogo(&mut prueba, larga());
    prueba.tecla(KeyCode::PageDown);
    prueba.tecla(KeyCode::Char('s'));
    assert!(prueba.app.dialogo.is_none(), "s no confirmó");

    // La huella del servidor no cabe a 40×12: se desplaza y `a` acepta.
    abrir_huella(&mut prueba);
    let ventana = ventana_modal(&prueba);
    assert!(ventana.total > ventana.filas, "{ventana:?}");
    prueba.tecla(KeyCode::Down);
    assert_eq!(ventana_modal(&prueba).inicio, 1);
    prueba.enviados();
    prueba.tecla(KeyCode::Char('a'));
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.enviados().iter().any(|mensaje| matches!(
        mensaje,
        magi::protocolo::MensajeCliente::DecisionHuella {
            sesion_id: 1,
            decision: true
        }
    )));
}

/// En la huella cambiada el campo va en el pie: se ve lo que se escribe
/// aunque el cuerpo esté desplazado, y `r` sustituye con el nombre exacto.
#[test]
fn la_huella_cambiada_deja_el_campo_a_la_vista() {
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    prueba.servidor(MensajeServidor::HuellaCambiada {
        sesion_id: 1,
        host: "hetzner-01".to_string(),
        tipo_clave: "ssh-ed25519".to_string(),
        anterior: HUELLA.to_string(),
        nueva: "SHA256:Zy9xW7vU5tS3rQ1pO9nM7lK5jI3hG1fE9dC7bA5zY3".to_string(),
    });
    for caracter in "hetzner-01".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    pulsar(&mut prueba, KeyCode::PageDown, 3);
    let texto = prueba.texto();
    assert!(texto.contains("[ hetzner-01┃ ]"), "{texto}");
    assert!(prueba.app.desplazamiento_modal > 0);
    prueba.enviados();
    prueba.tecla(KeyCode::Char('r'));
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.enviados().iter().any(|mensaje| matches!(
        mensaje,
        magi::protocolo::MensajeCliente::DecisionHuella { decision: true, .. }
    )));
}

/// Los formularios no se desplazan con las flechas: siguen al campo con foco.
#[test]
fn los_formularios_siguen_al_foco() {
    // GENERAR CLAVE a 40×12: el último campo queda a la vista con ⇥.
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    abrir_dialogo(
        &mut prueba,
        Dialogo::GenerarClave {
            estado: EstadoGeneracion::nuevo(),
        },
    );
    assert!(ventana_modal(&prueba).total > ventana_modal(&prueba).filas);
    pulsar(&mut prueba, KeyCode::Tab, 6);
    let texto = prueba.texto();
    assert!(texto.contains("copiar la pública"), "{texto}");
    assert!(ventana_modal(&prueba).inicio > 0);
    pulsar(&mut prueba, KeyCode::BackTab, 6);
    let texto = prueba.texto();
    assert!(texto.contains("Fichero"), "{texto}");

    // Menú de grupo a 40×10: ↑ ↓ son suyas y la opción elegida se ve.
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    a_hosts(&mut prueba);
    prueba.tecla(KeyCode::Char('g'));
    prueba.redimensionar(40, 10);
    pulsar(&mut prueba, KeyCode::Down, 5);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::MenuGrupo { seleccion: 5 })
    ));
    let texto = prueba.texto();
    assert!(texto.contains("▸  bajar orden"), "{texto}");

    // Túnel a 40×12: el desplegable de hosts abierto sigue a la opción.
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    let formulario = FormularioTunel::nuevo(&prueba.app.hosts, None);
    let ultimo = prueba.app.hosts.last().expect("hosts").nombre.clone();
    abrir_dialogo(&mut prueba, Dialogo::Tunel { estado: formulario });
    prueba.tecla(KeyCode::Enter);
    pulsar(&mut prueba, KeyCode::Down, 10);
    let texto = prueba.texto();
    assert!(texto.contains(&format!("▸ {ultimo}")), "{texto}");
    // Cerrado el desplegable, ⇥ hasta «automático».
    prueba.tecla(KeyCode::Enter);
    pulsar(&mut prueba, KeyCode::Tab, 7);
    let texto = prueba.texto();
    assert!(texto.contains("automático"), "{texto}");
}

/// El texto a medio escribir y el cursor de un diálogo se conservan en la
/// secuencia de tamaños, y el campo siempre se ve.
#[test]
fn la_entrada_de_texto_conserva_texto_y_foco_en_la_secuencia() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    abrir_entrada_texto(&mut prueba, "servidores de casa");
    prueba.tecla(KeyCode::Left);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        match &prueba.app.dialogo {
            Some(Dialogo::EntradaTexto { campo, accion, .. }) => {
                assert_eq!(campo.texto, "servidores de casa");
                assert_eq!(campo.cursor, 17);
                assert!(matches!(accion, EntradaTextoAccion::CrearGrupo));
            }
            _ => panic!("se perdió el diálogo a {cols}×{filas}"),
        }
        let texto = prueba.texto();
        assert!(
            texto.contains("servidores de cas┃a"),
            "campo fuera de la vista a {cols}×{filas}:\n{texto}"
        );
    }
    // Un texto más largo que el diálogo estrecho se parte y sigue a la vista.
    prueba.redimensionar(40, 12);
    prueba.tecla(KeyCode::End);
    for caracter in " y del trastero".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    let texto = prueba.texto();
    assert!(texto.contains("trastero┃"), "{texto}");
}

/// Un diálogo desplazado se recoloca en cada tamaño sin huecos al final.
#[test]
fn el_detalle_se_recoloca_en_la_secuencia() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    abrir_detalle(&mut prueba);
    pulsar(&mut prueba, KeyCode::Down, 5);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert!(matches!(prueba.app.dialogo, Some(Dialogo::Detalle { .. })));
        let ventana = ventana_modal(&prueba);
        assert!(
            ventana.inicio <= ventana.total.saturating_sub(ventana.filas),
            "hueco al final a {cols}×{filas}: {ventana:?}"
        );
        assert_eq!(prueba.app.desplazamiento_modal, ventana.inicio);
        assert!(ventana.total > ventana.filas);
        let texto = prueba.texto();
        assert!(texto.contains("↵ / esc cerrar"), "{texto}");
    }
}

/// PgUp/PgDn de la paleta avanzan las entradas que se ven; la consulta y la
/// selección se conservan en la secuencia y la selección siempre se ve.
#[test]
fn la_paleta_pagina_y_conserva_la_consulta() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir_paleta(&mut prueba);
    let filas = prueba.app.disposicion().filas(Lista::Paleta);
    let total = prueba.app.paleta.as_ref().unwrap().filtradas.len();
    assert!(total > 2 * filas, "pocas entradas: {total}");
    prueba.tecla(KeyCode::PageDown);
    assert_eq!(prueba.app.paleta.as_ref().unwrap().seleccion, filas);
    prueba.tecla(KeyCode::PageDown);
    assert_eq!(prueba.app.paleta.as_ref().unwrap().seleccion, 2 * filas);
    prueba.tecla(KeyCode::PageUp);
    assert_eq!(prueba.app.paleta.as_ref().unwrap().seleccion, filas);
    pulsar(&mut prueba, KeyCode::PageDown, 20);
    assert_eq!(prueba.app.paleta.as_ref().unwrap().seleccion, total - 1);
    pulsar(&mut prueba, KeyCode::PageUp, 20);
    assert_eq!(prueba.app.paleta.as_ref().unwrap().seleccion, 0);

    // Consulta a medias y una coincidencia lejos de la primera.
    for caracter in "sftp".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    pulsar(&mut prueba, KeyCode::PageDown, 5);
    let elegida = prueba.app.paleta.as_ref().unwrap().seleccion;
    assert!(elegida > 0);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        let paleta = prueba.app.paleta.as_ref().expect("se cerró la paleta");
        assert_eq!(paleta.consulta.texto, "sftp");
        assert_eq!(paleta.seleccion, elegida);
        let ventana = prueba.app.disposicion().lista(Lista::Paleta).unwrap();
        assert!(
            ventana.inicio <= elegida && elegida < ventana.inicio + ventana.filas,
            "selección fuera de la vista a {cols}×{filas}: {ventana:?}"
        );
        let texto = prueba.texto();
        assert!(texto.contains("sftp┃"), "{texto}");
    }
}

/// La ayuda encoge con desplazamiento y vuelve a verse entera al crecer.
#[test]
fn la_ayuda_se_desplaza_y_encoge() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    abrir_ayuda_hosts(&mut prueba);
    let ventana = prueba.app.disposicion().lista(Lista::Ayuda).unwrap();
    assert!(ventana.total <= ventana.filas, "{ventana:?}");

    prueba.pasar_por(40, 12);
    let ventana = prueba.app.disposicion().lista(Lista::Ayuda).unwrap();
    assert!(ventana.total > ventana.filas, "{ventana:?}");
    let maximo = ventana.total - ventana.filas;
    let texto = prueba.texto();
    assert!(texto.contains(&format!("↑↓ 1/{}", maximo + 1)), "{texto}");
    prueba.tecla(KeyCode::Down);
    assert_eq!(prueba.app.desplazamiento_ayuda, 1);
    prueba.tecla(KeyCode::PageDown);
    assert_eq!(prueba.app.desplazamiento_ayuda, 1 + ventana.filas);
    pulsar(&mut prueba, KeyCode::Down, 50);
    let texto = prueba.texto();
    assert!(texto.contains("desplazar diálogos"), "{texto}");
    assert_eq!(prueba.app.desplazamiento_ayuda, maximo);
    assert!(texto.contains("esc o ? para cerrar"), "{texto}");

    // Al crecer se ve entera otra vez y sigue abierta.
    prueba.pasar_por(80, 24);
    prueba.pasar_por(200, 60);
    assert!(prueba.app.ayuda);
    let ventana = prueba.app.disposicion().lista(Lista::Ayuda).unwrap();
    assert_eq!(ventana.inicio, 0);
    let texto = prueba.texto();
    assert!(texto.contains("↑ ↓ / j k") && texto.contains("desplazar diálogos"));
    prueba.tecla(KeyCode::Esc);
    assert!(!prueba.app.ayuda);
}

/// La ayuda añade las teclas nuevas de la fase en cada vista.
#[test]
fn la_ayuda_trae_las_teclas_nuevas() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    abrir_ayuda_flota(&mut prueba);
    let texto = prueba.texto();
    assert!(
        texto.contains("alternar lista y detalle (estrecho)"),
        "{texto}"
    );
    assert!(texto.contains("↑↓ PgUp PgDn  desplazar diálogos y esta ayuda"));
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::F(5));
    prueba.tecla(KeyCode::Char('?'));
    let texto = prueba.texto();
    assert!(
        texto.contains("AYUDA · IDENTIDADES") && texto.contains("↵             detalle"),
        "{texto}"
    );
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::F(8));
    prueba.tecla(KeyCode::Char('?'));
    let texto = prueba.texto();
    assert!(texto.contains("detalle (vista baja)"), "{texto}");
}

/// Crea un diálogo nuevo (los diálogos no son `Clone`).
pub(crate) type FabricaDialogo = Box<dyn Fn() -> Dialogo>;

/// Todas las variantes de diálogo que pinta `dialogos.rs`, con el principio
/// del título de su recuadro.
pub(crate) fn todos_los_dialogos(
    hosts: Vec<magi::modelo::Host>,
) -> Vec<(&'static str, FabricaDialogo)> {
    let mut dialogos = todos_los_dialogos_hasta_fase_7(hosts);
    // Los de la Fase 8, cada uno en el fichero de su vista.
    dialogos.extend(super::vistas_h::dialogos());
    dialogos.extend(super::vistas_i::dialogos());
    dialogos.extend(super::vistas_j::dialogos());
    dialogos.extend(super::vistas_k::dialogos());
    dialogos
}

fn todos_los_dialogos_hasta_fase_7(
    hosts: Vec<magi::modelo::Host>,
) -> Vec<(&'static str, FabricaDialogo)> {
    vec![
        ("DETALLE DEL REGISTRO", Box::new(detalle_largo)),
        (
            "CONFIRMAR CON",
            Box::new(|| Dialogo::Confirmar {
                titulo: "CONFIRMAR CON UN TÍTULO MUY LARGO QUE NO CABE".to_string(),
                lineas: vec!["¿Seguro?".to_string()],
                peligro: true,
                accion: AccionDialogo::Nada,
            }),
        ),
        (
            "SERVIDOR CA",
            Box::new(|| Dialogo::ServidorCaido { perdidas: 2 }),
        ),
        (
            "HUELLA DESCONOCIDA",
            Box::new(|| Dialogo::HuellaServidor {
                sesion_id: 1,
                host: "hetzner-01".to_string(),
                tipo: "ssh-ed25519".to_string(),
                huella: HUELLA.to_string(),
            }),
        ),
        (
            "HUELLA CAMBIADA",
            Box::new(|| Dialogo::HuellaCambiadaServidor {
                sesion_id: 1,
                host: "hetzner-01".to_string(),
                tipo: "ssh-ed25519".to_string(),
                anterior: HUELLA.to_string(),
                nueva: HUELLA.to_string(),
                campo: CampoTexto::nuevo("hetz"),
            }),
        ),
        (
            "FRASE DE LA CLAVE",
            Box::new(|| Dialogo::FraseServidor {
                sesion_id: 1,
                host: "hetzner-01".to_string(),
                intento: 2,
                campo: CampoTexto::nuevo("secreto"),
            }),
        ),
        (
            "CONTRASE",
            Box::new(|| Dialogo::ContrasenaServidor {
                sesion_id: 1,
                host: "hetzner-01".to_string(),
                intento: 1,
                campo: CampoTexto::nuevo("secreto"),
                recordar: true,
                foco_casilla: true,
            }),
        ),
        ("MEN", Box::new(|| Dialogo::MenuGrupo { seleccion: 5 })),
        (
            "NUEVO GRUPO",
            Box::new(|| Dialogo::EntradaTexto {
                titulo: "NUEVO GRUPO".to_string(),
                etiqueta: "Nombre".to_string(),
                campo: CampoTexto::nuevo("un nombre de grupo bastante largo"),
                accion: EntradaTextoAccion::CrearGrupo,
            }),
        ),
        (
            "MOVER HOST",
            Box::new(|| {
                let mut desplegable = Desplegable::nuevo(
                    ["produccion", "casa", "sin grupo"]
                        .iter()
                        .enumerate()
                        .map(|(indice, etiqueta)| Opcion {
                            etiqueta: etiqueta.to_string(),
                            valor: ValorOpcion::Grupo(indice as i64),
                        })
                        .collect(),
                );
                desplegable.resaltado = 2;
                desplegable.filtro = CampoTexto::nuevo("s");
                Dialogo::MoverHost {
                    host_id: 1,
                    host_nombre: "hetzner-01".to_string(),
                    desplegable,
                }
            }),
        ),
        (
            "IMPORTACI",
            Box::new(|| Dialogo::ResumenImportacion {
                titulo: "IMPORTACIÓN".to_string(),
                lineas: vec![
                    "7 hosts nuevos".to_string(),
                    "· aviso: 2 omitidos → sin dirección".to_string(),
                ],
            }),
        ),
        (
            "CONFLICTO DE",
            Box::new(|| Dialogo::ConflictoImportacion {
                nombre: "hetzner-01".to_string(),
                restantes: 3,
            }),
        ),
        (
            "YA EXISTE",
            Box::new(|| Dialogo::Conflicto {
                nombre: "main.py".to_string(),
                es_dir: false,
                lado_origen: magi::archivos::Lado::Local,
                tamano_origen: 14_000,
                fecha_origen: 1_757_671_200,
                tamano_destino: 12_000,
                fecha_destino: 1_757_584_800,
            }),
        ),
        (
            "EXPORTAR REGISTRO",
            Box::new(|| Dialogo::ExportarRegistro {
                estado: ExportacionRegistro {
                    json: true,
                    campo: CampoTexto::nuevo("~/registro.json"),
                    foco: 1,
                },
            }),
        ),
        (
            "GENERAR CLAVE",
            Box::new(|| Dialogo::GenerarClave {
                estado: EstadoGeneracion::nuevo(),
            }),
        ),
        (
            "FRASE DE LA CLAVE",
            Box::new(|| Dialogo::FraseImportacion {
                ruta: std::path::PathBuf::from("/tmp/clave"),
                campo: CampoTexto::nuevo("x"),
            }),
        ),
        (
            "T",
            Box::new(move || {
                let mut estado = FormularioTunel::nuevo(&hosts, None);
                estado.host.abrir();
                estado.host.resaltado = 5;
                Dialogo::Tunel { estado }
            }),
        ),
    ]
}

pub(crate) fn hosts_de_la_semilla() -> Vec<magi::modelo::Host> {
    let (prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.app.hosts.clone()
}

/// Ningún diálogo, ni la paleta ni la ayuda entran en pánico en áreas muy
/// pequeñas (el aviso de tamaño debajo y el diálogo encima), tampoco con uno
/// solo de los lados diminuto.
#[test]
fn sin_panico_en_areas_diminutas() {
    let mut tamanos = vec![
        (1, 1),
        (2, 2),
        (3, 3),
        (5, 3),
        (10, 4),
        (12, 5),
        (20, 3),
        (20, 6),
        (30, 10),
        (39, 11),
    ];
    // Un lado diminuto y el otro normal o enorme.
    for diminuto in 1..=3 {
        for normal in [12, 60] {
            tamanos.push((diminuto, normal));
        }
        for normal in [40, 200] {
            tamanos.push((normal, diminuto));
        }
    }
    for (indice, (_, dialogo)) in todos_los_dialogos(hosts_de_la_semilla()).iter().enumerate() {
        let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
        abrir_dialogo(&mut prueba, dialogo());
        for &(cols, filas) in &tamanos {
            prueba.pasar_por(cols, filas);
            assert!(prueba.app.dialogo.is_some(), "diálogo {indice} perdido");
            // Las flechas y el foco tampoco rompen nada con el área mínima.
            prueba.tecla(KeyCode::PageDown);
            prueba.tecla(KeyCode::Up);
            prueba.tecla(KeyCode::Tab);
        }
    }
    // La paleta y la ayuda, abiertas a 80×24 y encogidas.
    for abrir in [abrir_paleta as Abrir, abrir_ayuda_hosts] {
        let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
        abrir(&mut prueba);
        for &(cols, filas) in &tamanos {
            prueba.pasar_por(cols, filas);
            prueba.tecla(KeyCode::PageDown);
            prueba.tecla(KeyCode::Up);
        }
    }
}

/// Todas las variantes de diálogo se ven enteras (sin cortar) a 40×12 y,
/// con el tema ASCII, no pintan glifos Unicode a ningún tamaño: también las
/// que no tienen instantánea (túnel, generar clave, contraseña, conflictos…).
#[test]
fn todas_las_variantes_enteras_y_en_ascii() {
    for (titulo, dialogo) in todos_los_dialogos(hosts_de_la_semilla()) {
        let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
        abrir_dialogo(&mut prueba, dialogo());
        let texto = prueba.texto();
        let (_, _, _, y1) = posicion_recuadro(&texto, titulo, ESQUINAS_UNICODE);
        assert!(y1.is_some(), "«{titulo}» cortado a 40×12:\n{texto}");

        let (mut prueba, _) = AppPrueba::con_semilla_y_tema(80, 24, arnes::tema_ascii());
        abrir_dialogo(&mut prueba, dialogo());
        for (cols, filas) in [(80, 24), (40, 12), (70, 20), (30, 10), (200, 60)] {
            prueba.pasar_por(cols, filas);
            let texto = prueba.texto();
            let dentro = recuadro(&texto, titulo, ESQUINAS_ASCII).join("\n");
            assert!(
                solo_ascii(&dentro),
                "glifo Unicode en «{titulo}» a {cols}×{filas} ({}):\n{dentro}",
                glifos_unicode(&dentro)
            );
        }
    }
}

/// La categoría de la paleta nunca se pega a la etiqueta: va en su columna,
/// con dos espacios de separación, en todas las filas o en ninguna.
#[test]
fn la_categoria_de_la_paleta_no_se_pega_a_la_etiqueta() {
    for cols in [26, 30, 40, 44, 60, 80, 200] {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, 24);
        abrir_paleta(&mut prueba);
        // Etiquetas largas: «editar host · vps-openclaw», snippets…
        for caracter in "openclaw".chars() {
            prueba.tecla(KeyCode::Char(caracter));
        }
        let texto = prueba.texto();
        let filas = recuadro(&texto, "PALETA", ESQUINAS_UNICODE);
        // Sin el título, la consulta ni el borde de abajo.
        let entradas: Vec<String> = filas[2..filas.len() - 1]
            .iter()
            .map(|fila| fila.trim_matches(['│', ' ']).to_string())
            .filter(|fila| !fila.is_empty())
            .collect();
        assert!(entradas.len() >= 5, "a {cols}: pocas entradas:\n{texto}");
        let categorias = ["host", "acción", "archivos", "túneles", "snippets", "flota"];
        let con_categoria = entradas
            .iter()
            .filter(|fila| categorias.iter().any(|categoria| fila.ends_with(categoria)))
            .count();
        assert!(
            con_categoria == 0 || con_categoria == entradas.len(),
            "a {cols}: categoría solo en algunas filas:\n{texto}"
        );
        for fila in &entradas {
            if let Some(categoria) = categorias.iter().find(|c| fila.ends_with(**c)) {
                let antes = &fila[..fila.len() - categoria.len()];
                assert!(
                    antes.ends_with("  "),
                    "a {cols}: categoría pegada a la etiqueta en «{fila}»:\n{texto}"
                );
            }
        }
    }
}

/// Un conflicto de importación desplazado no deja desplazado el siguiente:
/// es otro diálogo de la misma variante y debe empezar arriba, con el nombre
/// del host a la vista (se decide sobre él).
#[test]
fn el_conflicto_siguiente_empieza_arriba() {
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    let config = prueba._entorno.rutas.fichero_ssh_config();
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        "Host hetzner-01\n  HostName 10.9.9.1\n\nHost hetzner-02\n  HostName 10.9.9.2\n",
    )
    .unwrap();
    a_hosts(&mut prueba);
    prueba.tecla(KeyCode::Char('I'));
    assert!(
        matches!(
            &prueba.app.dialogo,
            Some(Dialogo::ConflictoImportacion { nombre, .. }) if nombre == "hetzner-01"
        ),
        "{}",
        prueba.texto()
    );
    let ventana = ventana_modal(&prueba);
    assert!(ventana.total > ventana.filas, "cabe entero: {ventana:?}");
    prueba.tecla(KeyCode::PageDown);
    assert!(prueba.app.desplazamiento_modal > 0);
    // Omitir: aparece el conflicto de hetzner-02.
    prueba.tecla(KeyCode::Char('o'));
    assert!(matches!(
        &prueba.app.dialogo,
        Some(Dialogo::ConflictoImportacion { nombre, .. }) if nombre == "hetzner-02"
    ));
    assert_eq!(ventana_modal(&prueba).inicio, 0);
    let texto = prueba.texto();
    assert!(texto.contains("Ya existe un host"), "{texto}");
    assert!(texto.contains("hetzner-02"), "{texto}");
}
