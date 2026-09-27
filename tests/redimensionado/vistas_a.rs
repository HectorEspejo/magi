//! Instantáneas y pruebas de disposición del grupo A: Flota, Hosts, Sesiones
//! y la barra inferior.
//!
//! Lo que depende del reloj se filtra en las instantáneas: la antigüedad de
//! los sondeos («hace 31 s») y la del servidor («desde hace 0 s»). Las
//! sesiones se abren con fechas lejos del redondeo («18 m»).

use crossterm::event::{KeyCode, KeyModifiers};

use magi::app::PanelFlota;
use magi::protocolo::{EstadoSesionRemota, InfoSesion};
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};
use crate::semilla::Sembrado;

/// Los tres tamaños de referencia.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];
/// La secuencia de cambio de tamaño del checklist.
const SECUENCIA: [(u16, u16); 4] = [(200, 60), (80, 24), (40, 12), (200, 60)];

/// Instantánea con los filtros de reloj de este grupo.
fn instantanea(prueba: &AppPrueba, nombre: &str) {
    // Mismo ancho que lo sustituido, para no mover las columnas.
    let mut ajustes = insta::Settings::clone_current();
    ajustes.add_filter(r"hace \d\d s", "hace NN s");
    ajustes.add_filter(r"hace \d s", "hace N s");
    ajustes.bind(|| prueba.instantanea(nombre));
}

/// Instantánea de un texto cualquiera, junto a las de las vistas.
fn instantanea_texto(nombre: &str, texto: &str) {
    let mut ajustes = insta::Settings::clone_current();
    ajustes.set_snapshot_path("../snapshots");
    ajustes.set_prepend_module_to_snapshot(false);
    ajustes.set_omit_expression(true);
    ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
}

/// Tres sesiones abiertas a hosts de la semilla: una caída, una con dos
/// ventanas y otra identidad.
fn sesiones(sembrado: &Sembrado) -> Vec<InfoSesion> {
    let ahora = chrono::Utc::now().timestamp();
    let mut sesiones = Vec::new();
    for (id, host, minutos) in [
        (1, "hetzner-01", 18),
        (2, "hetzner-02", 125),
        (3, "mac-mini-m1", 3),
    ] {
        let mut sesion = arnes::sesion(id, host);
        sesion.host_id = sembrado.host(host);
        // Treinta segundos más: lejos del cambio de minuto.
        sesion.abierta_en = ahora - minutos * 60 - 30;
        sesiones.push(sesion);
    }
    sesiones[1].estado = EstadoSesionRemota::Caida;
    sesiones[1].motivo = Some("conexión perdida".to_string());
    sesiones[2].identidad = "yubikey".to_string();
    sesiones[2].ventanas = 2;
    sesiones
}

/// La vista Sesiones con las tres sesiones (sin pestaña activa, F3 lleva a
/// la lista).
fn en_sesiones(prueba: &mut AppPrueba, sembrado: &Sembrado) {
    prueba.bienvenida(sesiones(sembrado));
    prueba.tecla(KeyCode::F(3));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
}

fn lineas(prueba: &AppPrueba) -> Vec<String> {
    prueba.texto().lines().map(str::to_string).collect()
}

/// Última línea: la barra inferior.
fn barra(prueba: &AppPrueba) -> String {
    lineas(prueba).last().cloned().unwrap_or_default()
}

/// Línea con la marca de selección (`▸`, o `>` en ASCII).
fn linea_seleccionada(prueba: &AppPrueba) -> Option<String> {
    let marca = prueba.app.tema.glifos.seleccion;
    lineas(prueba).into_iter().find(|linea| {
        linea
            .trim_start_matches(['│', '|'])
            .trim_start()
            .starts_with(marca)
    })
}

/// Largo de la barra de la línea que empieza por la etiqueta (`CARGA`...).
fn largo_barra(prueba: &AppPrueba, etiqueta: &str) -> usize {
    let linea = lineas(prueba)
        .into_iter()
        .find(|linea| linea.contains(&format!("{etiqueta} ")) && linea.contains('░'))
        .unwrap_or_else(|| panic!("sin barra {etiqueta}:\n{}", prueba.texto()));
    linea.chars().filter(|c| *c == '█' || *c == '░').count()
}

fn sin_glifos_unicode(prueba: &AppPrueba, contexto: &str) {
    let texto = prueba.texto();
    let malos: String = texto
        .chars()
        .filter(|c| !(c.is_ascii() || c.is_alphabetic()))
        .collect();
    assert!(
        malos.is_empty(),
        "glifos Unicode {malos:?} en {contexto}:\n{texto}"
    );
}

// ------------------------------------------------------------ instantáneas

#[test]
fn flota_a_los_tres_tamanos_y_a_70x20() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    assert_eq!(prueba.app.vista, Vista::Flota);
    for (cols, filas) in TAMANOS.into_iter().chain([(70, 20)]) {
        prueba.pasar_por(cols, filas);
        instantanea(&prueba, &format!("flota_{cols}x{filas}"));
    }
}

#[test]
fn flota_detalle_en_estrecho_tras_tab() {
    let (mut prueba, _) = AppPrueba::con_semilla(70, 20);
    // hetzner-01, como en la maqueta.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
    instantanea(&prueba, "flota_detalle_70x20");
    for (cols, filas) in TAMANOS {
        prueba.pasar_por(cols, filas);
        // A 200×60 vuelven los dos paneles: el panel activo se conserva.
        instantanea(&prueba, &format!("flota_detalle_{cols}x{filas}"));
        assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
    }
}

#[test]
fn hosts_a_los_tres_tamanos_y_en_la_franja_de_60_a_80() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    prueba.tecla(KeyCode::F(2));
    assert_eq!(prueba.app.vista, Vista::Hosts);
    for (cols, filas) in TAMANOS.into_iter().chain([(70, 20)]) {
        prueba.pasar_por(cols, filas);
        instantanea(&prueba, &format!("hosts_{cols}x{filas}"));
    }
    // Con `Tab`, las etiquetas en lugar de usuario·puerto.
    prueba.pasar_por(80, 24);
    prueba.tecla(KeyCode::Tab);
    instantanea(&prueba, "hosts_etiquetas_80x24");
}

#[test]
fn sesiones_a_los_tres_tamanos_y_a_70x20() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 60);
    en_sesiones(&mut prueba, &sembrado);
    for (cols, filas) in TAMANOS.into_iter().chain([(70, 20)]) {
        prueba.pasar_por(cols, filas);
        instantanea(&prueba, &format!("sesiones_{cols}x{filas}"));
    }
}

/// Sin inventario, los avisos de Flota y Hosts se parten en vez de cortarse.
#[test]
fn flota_y_hosts_sin_hosts_a_40x12() {
    let mut prueba = AppPrueba::nueva(40, 12);
    assert_eq!(prueba.app.vista, Vista::Flota);
    instantanea(&prueba, "flota_vacia_40x12");
    prueba.tecla(KeyCode::F(2));
    instantanea(&prueba, "hosts_vacia_40x12");
    prueba.tecla(KeyCode::F(1));
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
    assert!(prueba.texto().contains("DETALLE"), "{}", prueba.texto());
}

#[test]
fn sesiones_sin_sesiones_a_40x12() {
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    prueba.bienvenida(Vec::new());
    prueba.tecla(KeyCode::F(3));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    instantanea(&prueba, "sesiones_vacia_40x12");
}

/// La barra de cada vista a varios anchos (solo los que llegan al mínimo de
/// la vista: por debajo se ve el aviso).
#[test]
fn barra_a_varios_anchos() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 24);
    prueba.bienvenida(sesiones(&sembrado));
    let mut texto = String::new();
    let vistas: [(&str, KeyCode, bool); 8] = [
        ("Flota", KeyCode::F(1), false),
        ("Flota detalle", KeyCode::F(1), true),
        ("Hosts", KeyCode::F(2), false),
        ("Sesiones", KeyCode::F(3), false),
        ("Identidades", KeyCode::F(5), false),
        ("Túneles", KeyCode::F(6), false),
        ("Registro", KeyCode::F(7), false),
        ("Snippets", KeyCode::F(8), false),
    ];
    for (nombre, tecla, detalle) in vistas {
        prueba.redimensionar(200, 24);
        prueba.tecla(tecla);
        prueba.app.panel_flota = PanelFlota::Lista;
        for ancho in [40u16, 50, 60, 70, 80, 99, 100, 120, 200] {
            prueba.redimensionar(ancho, 24);
            if detalle && prueba.app.panel_flota == PanelFlota::Lista && ancho < 100 {
                prueba.tecla(KeyCode::Tab);
            }
            if prueba.app.disposicion().aviso {
                continue;
            }
            texto.push_str(&format!("{nombre} {ancho}:\n{}\n", barra(&prueba)));
        }
    }
    instantanea_texto("barra_anchos", &texto);
}

// ------------------------------------------------------------ ASCII

/// En ASCII ninguna de estas vistas pinta glifos Unicode, en ningún modo.
#[test]
fn ascii_sin_glifos_unicode_en_ningun_modo() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(200, 60, arnes::tema_ascii());
    let tamanos = TAMANOS.into_iter().chain([(70, 20), (99, 30)]);
    for (cols, filas) in tamanos.clone() {
        prueba.pasar_por(cols, filas);
        sin_glifos_unicode(&prueba, &format!("Flota {cols}x{filas}"));
    }
    // Detalle en estrecho, filtro a medio escribir y un mensaje en la barra.
    prueba.pasar_por(70, 20);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
    assert!(prueba.texto().contains("tab lista"), "{}", prueba.texto());
    instantanea(&prueba, "flota_detalle_ascii_70x20");
    for (cols, filas) in TAMANOS.into_iter().chain([(70, 20)]) {
        prueba.pasar_por(cols, filas);
        sin_glifos_unicode(&prueba, &format!("Flota detalle {cols}x{filas}"));
    }
    // El detalle de un host con error (indicación de conectarse con ↵).
    for _ in 0..5 {
        prueba.tecla(KeyCode::Down);
    }
    for (cols, filas) in TAMANOS.into_iter().chain([(70, 20)]) {
        prueba.pasar_por(cols, filas);
        assert!(
            prueba.texto().contains("con enter para"),
            "{}",
            prueba.texto()
        );
        sin_glifos_unicode(&prueba, &format!("Flota con error {cols}x{filas}"));
    }
    prueba.tecla(KeyCode::Char('r'));
    assert!(prueba.app.mensaje.is_some());
    sin_glifos_unicode(&prueba, "mensaje de la barra");
    prueba.tecla(KeyCode::Char('/'));
    prueba.tecla(KeyCode::Char('h'));
    sin_glifos_unicode(&prueba, "filtro de Flota");
    prueba.tecla(KeyCode::Esc);

    prueba.tecla(KeyCode::F(2));
    for (cols, filas) in tamanos.clone() {
        prueba.pasar_por(cols, filas);
        sin_glifos_unicode(&prueba, &format!("Hosts {cols}x{filas}"));
    }
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char('/'));
    sin_glifos_unicode(&prueba, "Hosts con etiquetas y filtro");
    prueba.tecla(KeyCode::Esc);

    prueba.bienvenida(Vec::new());
    prueba.tecla(KeyCode::F(3));
    sin_glifos_unicode(&prueba, "Sesiones vacía");
    prueba.bienvenida(sesiones(&sembrado));
    for (cols, filas) in tamanos {
        prueba.pasar_por(cols, filas);
        assert_eq!(prueba.app.vista, Vista::Sesiones);
        sin_glifos_unicode(&prueba, &format!("Sesiones {cols}x{filas}"));
    }
}

// ------------------------------------------------------------ Flota

#[test]
fn tab_alterna_lista_y_detalle_solo_en_estrecho() {
    let (mut prueba, _) = AppPrueba::con_semilla(120, 30);
    // En normal se ven los dos paneles y `Tab` no hace nada.
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Lista);
    let texto = prueba.texto();
    assert!(
        texto.contains("backup-nas") && texto.contains("CARGA ░"),
        "{texto}"
    );
    assert!(!barra(&prueba).contains('⇥'), "{}", barra(&prueba));

    prueba.pasar_por(70, 20);
    let texto = prueba.texto();
    assert!(
        texto.contains("HOSTS") && texto.contains("⇥ detalle"),
        "{texto}"
    );
    assert!(
        !texto.contains("CARGA ░"),
        "en estrecho solo un panel:\n{texto}"
    );
    assert!(barra(&prueba).contains("⇥ detalle"));

    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
    let texto = prueba.texto();
    assert!(
        texto.contains("DETALLE") && texto.contains("⇥ lista"),
        "{texto}"
    );
    assert!(texto.contains("CARGA ░"), "{texto}");
    assert!(!texto.contains("dgx-spark"), "la lista no se ve:\n{texto}");
    assert!(barra(&prueba).contains("⇥ lista"));

    // Mayúsculas+Tab también alterna; `Tab` no hace nada más en Flota.
    prueba.tecla_con(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Lista);
    assert_eq!(prueba.app.vista, Vista::Flota);
    assert!(prueba.app.dialogo.is_none() && prueba.app.mensaje.is_none());
}

#[test]
fn el_filtro_en_estrecho_se_escribe_en_la_lista() {
    let (mut prueba, _) = AppPrueba::con_semilla(70, 20);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Detalle);
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "hetz".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    // `Tab` mientras se escribe no alterna ni se anuncia.
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.panel_flota, PanelFlota::Lista);
    let texto = prueba.texto();
    assert!(texto.contains("/ hetz"), "{texto}");
    assert!(!texto.contains("⇥ detalle"), "{texto}");
    assert!(
        texto.contains("hetzner-01") && !texto.contains("dgx-spark"),
        "{texto}"
    );
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.filtro, "hetz");
    assert!(prueba.texto().contains("⇥ detalle"));
}

#[test]
fn barras_de_carga_memoria_y_disco_con_ancho_variable() {
    let (mut prueba, _) = AppPrueba::con_semilla(120, 30);
    let normal = largo_barra(&prueba, "CARGA");
    prueba.pasar_por(70, 20);
    prueba.tecla(KeyCode::Tab);
    let estrecho = largo_barra(&prueba, "MEM");
    prueba.pasar_por(40, 12);
    let minimo = largo_barra(&prueba, "DSK");
    prueba.pasar_por(100, 30);
    let justo = largo_barra(&prueba, "CARGA");
    assert!(minimo < estrecho, "{minimo} {estrecho}");
    assert!(justo < normal || normal == 40, "{justo} {normal}");
    for largo in [normal, estrecho, minimo, justo] {
        assert!((4..=40).contains(&largo), "barra de {largo}");
    }
    // Las tres barras tienen siempre el mismo ancho.
    assert_eq!(largo_barra(&prueba, "MEM"), justo);
    assert_eq!(largo_barra(&prueba, "DSK"), justo);
}

/// A ≥ 200×60 la lista y el detalle tienen ancho máximo y van centrados.
#[test]
fn detalle_con_ancho_maximo_y_centrado_en_ventanas_muy_grandes() {
    let (mut prueba, _) = AppPrueba::con_semilla(199, 60);
    let columna = |prueba: &AppPrueba| {
        let linea = linea_seleccionada(prueba).expect("selección");
        (
            linea.chars().position(|c| c == '▸').unwrap(),
            linea.chars().skip(2).position(|c| c == '│').unwrap() + 2,
        )
    };
    // Por debajo de 200×60: lista pegada al borde.
    assert_eq!(columna(&prueba).0, 1);
    prueba.pasar_por(200, 59);
    assert_eq!(columna(&prueba).0, 1);
    for (cols, filas) in [(200, 60), (260, 70)] {
        prueba.pasar_por(cols, filas);
        let (marca, separador) = columna(&prueba);
        // Bloque de 48 + 1 + 120 columnas centrado en el interior.
        let bloque = 48 + 1 + 120;
        let margen = (usize::from(cols) - 2 - bloque) / 2;
        assert_eq!(marca, 1 + margen, "{cols}x{filas}");
        assert_eq!(separador, 1 + margen + 48, "{cols}x{filas}");
        assert_eq!(largo_barra(&prueba, "CARGA"), 40);
    }
}

#[test]
fn secuencia_conserva_seleccion_y_panel_en_flota() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    for _ in 0..4 {
        prueba.tecla(KeyCode::Down);
    }
    let elegido = prueba.app.host_flota_seleccionado().unwrap().nombre.clone();
    assert_eq!(elegido, "mac-mini-m1");
    for (indice, (cols, filas)) in SECUENCIA.into_iter().enumerate() {
        prueba.pasar_por(cols, filas);
        if indice == 1 {
            // En estrecho se pasa al detalle; el panel se conserva después.
            prueba.tecla(KeyCode::Tab);
        }
        assert_eq!(prueba.app.seleccion_flota, 4);
        let panel = if indice == 0 {
            PanelFlota::Lista
        } else {
            PanelFlota::Detalle
        };
        assert_eq!(prueba.app.panel_flota, panel);
        let texto = prueba.texto();
        assert!(texto.contains(&elegido), "{cols}x{filas}:\n{texto}");
        if cols >= 100 {
            let linea = linea_seleccionada(&prueba).expect("selección a la vista");
            assert!(linea.contains(&elegido), "{linea}");
        }
    }
    prueba.pasar_por(80, 24);
    assert!(prueba.texto().contains("DETALLE"));
}

/// El filtro a medio escribir se conserva y la lista filtrada con la
/// selección sigue a la vista en toda la secuencia.
#[test]
fn secuencia_conserva_el_filtro_en_flota() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "hetz".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Down);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert!(prueba.app.filtro_activo);
        assert_eq!(prueba.app.filtro, "hetz");
        assert_eq!(prueba.app.seleccion_flota, 1);
        let texto = prueba.texto();
        assert!(texto.contains("/ hetz"), "{cols}x{filas}:\n{texto}");
        assert!(!texto.contains("dgx-spark"), "{cols}x{filas}:\n{texto}");
        let linea = linea_seleccionada(&prueba).expect("selección a la vista");
        assert!(linea.contains("hetzner-02"), "{cols}x{filas}: {linea}");
    }
}

/// Las líneas del detalle que no caben se parten sin perder la sangría
/// (antes la continuación quedaba pegada al borde o al separador).
#[test]
fn el_detalle_partido_conserva_la_sangria() {
    let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
    // rincon-dev: sondeo con error y la indicación larga de conectarse.
    for _ in 0..5 {
        prueba.tecla(KeyCode::Down);
    }
    assert_eq!(
        prueba.app.host_flota_seleccionado().unwrap().nombre,
        "rincon-dev"
    );
    prueba.tecla(KeyCode::Tab);
    let texto = prueba.texto();
    assert!(
        texto.contains("la frase"),
        "la indicación se parte:\n{texto}"
    );
    let interior: Vec<String> = lineas(&prueba)[1..10]
        .iter()
        .map(|linea| linea.trim_start_matches('│').to_string())
        .collect();
    for linea in &interior {
        let contenido = linea.trim_end_matches(['│', ' ']);
        assert!(
            contenido.is_empty() || contenido.starts_with("  "),
            "línea sin sangría «{linea}»:\n{texto}"
        );
    }

    // En normal, el detalle va tras el separador con la misma sangría.
    prueba.pasar_por(100, 30);
    let texto = prueba.texto();
    for linea in lineas(&prueba)[1..28].iter() {
        let detalle: String = linea
            .chars()
            .skip(1)
            .skip_while(|c| *c != '│')
            .skip(1)
            .collect();
        let contenido = detalle.trim_end_matches(['│', ' ']);
        assert!(
            contenido.is_empty() || contenido.starts_with("   "),
            "detalle sin sangría «{detalle}»:\n{texto}"
        );
    }
    // La indicación se parte también aquí: su final va en otra línea.
    assert!(
        lineas(&prueba)
            .iter()
            .any(|linea| linea.contains("│   frase")),
        "{texto}"
    );
}

// ------------------------------------------------------------ Hosts

#[test]
fn hosts_ocultan_direccion_bajo_80_y_usuario_puerto_bajo_60() {
    let (mut prueba, _) = AppPrueba::con_semilla(120, 30);
    prueba.tecla(KeyCode::F(2));
    let fila = |prueba: &AppPrueba| {
        lineas(prueba)
            .into_iter()
            .find(|linea| linea.contains("hetzner-02"))
            .expect("fila de hetzner-02")
    };
    for (ancho, direccion, usuario) in [
        (120, true, true),
        (80, true, true),
        (79, false, true),
        (60, false, true),
        (59, false, false),
        (40, false, false),
    ] {
        prueba.pasar_por(ancho, 24);
        let fila = fila(&prueba);
        assert_eq!(fila.contains("10.0.1.12"), direccion, "{ancho}: {fila}");
        assert_eq!(fila.contains("deploy"), usuario, "{ancho}: {fila}");
        assert_eq!(fila.contains("2222"), usuario, "{ancho}: {fila}");
        // Glifo de estado y nombre nunca se ocultan.
        assert!(fila.contains("○ hetzner-02"), "{ancho}: {fila}");
    }
    // Con etiquetas, el mismo umbral que usuario·puerto.
    prueba.tecla(KeyCode::Tab);
    prueba.pasar_por(60, 24);
    assert!(fila(&prueba).contains("web"), "{}", fila(&prueba));
    prueba.pasar_por(59, 24);
    assert!(!fila(&prueba).contains("web"), "{}", fila(&prueba));
}

#[test]
fn secuencia_conserva_filtro_y_seleccion_en_hosts() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "hetz".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    let seleccion = prueba.app.seleccion;
    let elegido = prueba.app.host_seleccionado().unwrap().nombre.clone();
    assert_eq!(elegido, "hetzner-02");
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert!(prueba.app.filtro_activo);
        assert_eq!(prueba.app.filtro, "hetz");
        assert_eq!(prueba.app.seleccion, seleccion);
        assert!(prueba.texto().contains("/ hetz"), "{}", prueba.texto());
        let linea = linea_seleccionada(&prueba).expect("selección a la vista");
        assert!(linea.contains(&elegido), "{cols}x{filas}: {linea}");
    }
}

/// La última fila seleccionada sigue a la vista cuando la lista no cabe.
#[test]
fn hosts_la_seleccion_del_final_sigue_a_la_vista_al_encoger() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::End);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        let linea = linea_seleccionada(&prueba).expect("selección a la vista");
        assert!(linea.contains("rincon-dev"), "{cols}x{filas}: {linea}");
    }
    prueba.pasar_por(40, 12);
    let ventana = prueba
        .app
        .disposicion()
        .lista(magi::ui::disposicion::Lista::Hosts)
        .unwrap();
    assert!(ventana.inicio > 0, "{ventana:?}");
    assert_eq!(ventana.inicio + ventana.filas, ventana.total, "sin hueco");
}

// ------------------------------------------------------------ Sesiones

#[test]
fn sesiones_columnas_por_prioridad() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(120, 30);
    en_sesiones(&mut prueba, &sembrado);
    for (ancho, host, secundarias) in [
        (120, true, true),
        (80, true, true),
        (79, true, false),
        (60, true, false),
        (59, false, false),
        (40, false, false),
    ] {
        prueba.pasar_por(ancho, 24);
        let texto = prueba.texto();
        let cabecera = lineas(&prueba)
            .into_iter()
            .find(|linea| linea.contains("ESTADO"))
            .expect("cabecera");
        assert!(cabecera.contains("PESTAÑA"), "{ancho}: {cabecera}");
        assert_eq!(cabecera.contains("HOST"), host, "{ancho}: {cabecera}");
        assert_eq!(cabecera.contains("IDENTIDAD"), secundarias, "{ancho}");
        assert_eq!(cabecera.contains("VENT."), secundarias, "{ancho}");
        assert!(cabecera.contains("TIEMPO"), "{ancho}: {cabecera}");
        // Glifo de estado y pestaña en todas las filas.
        assert!(
            texto.contains("✕ ") && texto.contains("mac-mini-m1"),
            "{texto}"
        );
    }
}

#[test]
fn sesiones_panel_inferior_plegado_si_es_bajo() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    en_sesiones(&mut prueba, &sembrado);
    prueba.tecla(KeyCode::Down);
    prueba.pasar_por(80, 20);
    let texto = prueba.texto();
    assert!(texto.contains("servidor pid 4242"), "{texto}");
    assert!(texto.contains("caída: conexión perdida"), "{texto}");
    prueba.pasar_por(80, 19);
    let texto = prueba.texto();
    assert!(!texto.contains("servidor pid"), "{texto}");
    assert!(texto.contains("hetzner-02"), "{texto}");
}

#[test]
fn secuencia_conserva_la_seleccion_en_sesiones() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 60);
    en_sesiones(&mut prueba, &sembrado);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    for (cols, filas) in SECUENCIA {
        prueba.pasar_por(cols, filas);
        assert_eq!(prueba.app.seleccion_sesiones, 2);
        let linea = linea_seleccionada(&prueba).expect("selección a la vista");
        assert!(linea.contains("mac-mini-m1"), "{cols}x{filas}: {linea}");
    }
}

/// Una sesión de más de cuatro días cabe entera en TIEMPO y las líneas del
/// panel inferior que no caben se recortan con marca.
#[test]
fn sesiones_tiempo_largo_y_panel_recortado_con_marca() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    let mut lista = sesiones(&sembrado);
    lista[0].abierta_en = chrono::Utc::now().timestamp() - 125 * 3600 - 30;
    prueba.bienvenida(lista);
    prueba.tecla(KeyCode::F(3));
    assert_eq!(prueba.app.vista, Vista::Sesiones);
    let texto = prueba.texto();
    assert!(texto.contains("125 h 00 m"), "{texto}");
    assert!(texto.contains("yubikey"), "{texto}");

    prueba.pasar_por(40, 20);
    let servidor = lineas(&prueba)
        .into_iter()
        .find(|linea| linea.contains("servidor pid"))
        .expect("línea del servidor");
    assert_eq!(servidor.chars().count(), 40, "{servidor}");
    assert!(servidor.ends_with('…'), "{servidor}");
}

// ------------------------------------------------------------ barra

#[test]
fn barra_con_atajos_por_prioridad_y_mas() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 24);
    let ancha = barra(&prueba);
    assert!(ancha.trim_end().ends_with("? ayuda"), "{ancha}");
    assert!(!ancha.contains("más"), "{ancha}");
    assert!(ancha.contains("↓↑ mover"), "{ancha}");

    prueba.redimensionar(70, 20);
    let estrecha = barra(&prueba);
    assert!(estrecha.trim_end().ends_with("? más"), "{estrecha}");
    assert!(estrecha.chars().count() <= 70, "{estrecha}");
    // Los de más prioridad, en su orden; el de menos (mover) no cabe.
    let posicion = |texto: &str| estrecha.find(texto).unwrap_or_else(|| panic!("{texto}"));
    assert!(posicion("↵ ssh") < posicion("r sondear"));
    assert!(posicion("r sondear") < posicion("⇥ detalle"));
    assert!(!estrecha.contains("mover"), "{estrecha}");

    prueba.redimensionar(40, 12);
    let minima = barra(&prueba);
    assert!(
        minima.contains("↵ ssh") && minima.contains("? más"),
        "{minima}"
    );
    assert!(minima.chars().count() <= 40, "{minima}");
}

#[test]
fn barra_los_atajos_de_flota_son_los_primeros_en_ocultarse() {
    let mut entorno = arnes::entorno_fijo();
    entorno
        .config
        .flota
        .atajos
        .insert("d".to_string(), "espacio en disco".to_string());
    crate::semilla::sembrar(&entorno.rutas);
    let mut prueba = AppPrueba::con_entorno(entorno, 200, 24);
    assert_eq!(prueba.app.atajos_flota().len(), 1);
    prueba.tecla(KeyCode::Esc);
    let ancha = barra(&prueba);
    assert!(ancha.contains("d espacio en disco"), "{ancha}");
    prueba.redimensionar(100, 24);
    let media = barra(&prueba);
    assert!(!media.contains("espacio en disco"), "{media}");
    assert!(
        media.contains("q salir") && media.contains("? más"),
        "{media}"
    );
}

#[test]
fn barra_el_mensaje_tiene_prioridad_sobre_los_atajos() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    prueba.tecla(KeyCode::Char('a'));
    let linea = barra(&prueba);
    assert!(linea.contains("auto-refresco"), "{linea}");
    assert!(!linea.contains("sondear"), "{linea}");
    // Un mensaje largo se recorta al ancho con marca.
    prueba.app.mensaje = Some(magi::app::Mensaje {
        texto: "x".repeat(200),
        error: false,
        creado: std::time::Instant::now(),
    });
    prueba.redimensionar(60, 24);
    let linea = barra(&prueba);
    assert_eq!(linea.chars().count(), 60, "{linea}");
    assert!(linea.ends_with('…'), "{linea}");
}

/// Sesiones abre la ayuda con `?`: la barra termina en `? ayuda` si caben
/// todos los atajos y en `? más` si no.
#[test]
fn barra_de_sesiones_anuncia_la_ayuda() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 24);
    en_sesiones(&mut prueba, &sembrado);
    prueba.redimensionar(200, 24);
    let linea = barra(&prueba);
    assert!(
        linea.contains("↵ entrar") && linea.ends_with("? ayuda"),
        "{linea}"
    );
    prueba.redimensionar(50, 24);
    let linea = barra(&prueba);
    assert!(
        linea.contains("↵ entrar") && linea.ends_with("? más"),
        "{linea}"
    );
    prueba.tecla(KeyCode::Char('?'));
    assert!(prueba.app.ayuda);
}

/// El filtro de Túneles que se está escribiendo va siempre al final de la
/// barra y entero a la vista (con su cursor), a cualquier ancho: los atajos
/// le dejan sitio y, si el texto no cabe, se ve su final.
#[test]
fn barra_el_filtro_de_tuneles_cabe_siempre() {
    let (mut prueba, _) = AppPrueba::con_semilla(120, 24);
    prueba.tecla(KeyCode::F(6));
    assert_eq!(prueba.app.vista, Vista::Tuneles);
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "post".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    for ancho in 50..=120u16 {
        prueba.redimensionar(ancho, 24);
        let linea = barra(&prueba);
        assert!(
            linea.chars().count() <= usize::from(ancho),
            "{ancho}: {linea}"
        );
        assert!(linea.ends_with("/ post▏"), "{ancho}: {linea}");
    }
    for caracter in "gres-de-produccion-con-un-nombre-muy-largo-de-verdad".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.redimensionar(50, 24);
    let linea = barra(&prueba);
    assert_eq!(linea.chars().count(), 50, "{linea}");
    assert!(
        linea.contains("/ …") && linea.ends_with("de-verdad▏"),
        "se ve el final: {linea}"
    );
    assert!(!linea.contains('?'), "{linea}");
}

// ------------------------------------------------------------ áreas diminutas

/// Las vistas del grupo y la barra se pintan sin pánico en cualquier área,
/// también en las que `ui::dibujar` nunca les daría (anchos y altos de 0 a 3,
/// franjas de una fila o de una columna).
#[test]
fn sin_panico_en_areas_diminutas() {
    use magi::ui::disposicion::{self, Disposicion};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    prueba.bienvenida(sesiones(&sembrado));
    let mut tamanos: Vec<(u16, u16)> = Vec::new();
    for ancho in 0..=3 {
        for alto in 0..=3 {
            tamanos.push((ancho, alto));
        }
    }
    tamanos.extend([
        (120, 1),
        (120, 2),
        (120, 3),
        (1, 40),
        (3, 40),
        (99, 2),
        (200, 3),
        (41, 4),
    ]);
    let estados: [(Vista, bool, PanelFlota); 6] = [
        (Vista::Flota, false, PanelFlota::Lista),
        (Vista::Flota, true, PanelFlota::Lista),
        (Vista::Flota, false, PanelFlota::Detalle),
        (Vista::Hosts, false, PanelFlota::Lista),
        (Vista::Hosts, true, PanelFlota::Lista),
        (Vista::Sesiones, false, PanelFlota::Lista),
    ];
    for (vista, filtro, panel) in estados {
        prueba.app.vista = vista;
        prueba.app.filtro_activo = filtro;
        prueba.app.panel_flota = panel;
        for (ancho, alto) in &tamanos {
            let area = Rect::new(0, 0, *ancho, *alto);
            let mut terminal =
                Terminal::new(TestBackend::new((*ancho).max(1), (*alto).max(1))).unwrap();
            terminal
                .draw(|marco| {
                    let mut disp = Disposicion::nueva(area, disposicion::minimo_de(vista, false));
                    match vista {
                        Vista::Flota => {
                            magi::ui::flota::dibujar(marco, area, &prueba.app, &mut disp)
                        }
                        Vista::Hosts => {
                            magi::ui::hosts::dibujar(marco, area, &prueba.app, &mut disp)
                        }
                        _ => magi::ui::sesiones::dibujar(marco, area, &prueba.app, &mut disp),
                    }
                    magi::ui::barra::dibujar(marco, area, &prueba.app, &mut disp);
                })
                .unwrap_or_else(|error| panic!("{vista:?} a {ancho}x{alto}: {error}"));
        }
    }
}
