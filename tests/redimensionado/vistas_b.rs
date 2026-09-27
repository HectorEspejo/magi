//! Instantáneas y pruebas de disposición del grupo B: la ficha de host.
//!
//! Estado de prueba: F2, bajar al primer host (hetzner-01) y `e`; o `n` para
//! un host nuevo. Se comprueba que el campo con el foco se ve siempre, que en
//! estrecho las etiquetas van encima, que desplegables y sugerencias se anclan
//! a su campo sin salirse del área y que nada del estado se pierde al cambiar
//! de tamaño.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use magi::app::{CampoFicha, Ficha, ORDEN_CAMPOS};
use magi::modelo::{DatosTunel, TipoTunel};
use magi::tema::Tema;
use magi::ui::disposicion::{self, Disposicion};
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};

/// Cursor de los campos de texto.
const CURSOR: char = '\u{2503}';

/// Tamaños por los que pasan las pruebas de foco y de listas emergentes: el
/// mínimo de la ficha, estrechos, bajos, normal y muy grande.
const TAMANOS_PINTADOS: [(u16, u16); 7] = [
    (50, 14),
    (60, 20),
    (80, 16),
    (80, 24),
    (100, 20),
    (120, 40),
    (200, 60),
];

/// Ficha de hetzner-01 con el tema dado.
fn ficha_de_host_con(cols: u16, filas: u16, tema: Tema) -> AppPrueba {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(cols, filas, tema);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('e'));
    assert_eq!(prueba.app.vista, Vista::Ficha);
    assert_eq!(ficha(&prueba).titulo, "hetzner-01");
    prueba
}

fn ficha_de_host(cols: u16, filas: u16) -> AppPrueba {
    ficha_de_host_con(cols, filas, Tema::respaldo())
}

/// Ficha de un host nuevo (`n` en Hosts).
fn ficha_nueva_con(cols: u16, filas: u16, tema: Tema) -> AppPrueba {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(cols, filas, tema);
    prueba.tecla(KeyCode::F(2));
    prueba.tecla(KeyCode::Char('n'));
    assert_eq!(prueba.app.vista, Vista::Ficha);
    assert_eq!(ficha(&prueba).host_id, None);
    prueba
}

fn ficha(prueba: &AppPrueba) -> &Ficha {
    prueba.app.ficha.as_ref().expect("ficha abierta")
}

/// Lleva el foco a `campo` con `Tab`.
fn ir_a(prueba: &mut AppPrueba, campo: CampoFicha) {
    for _ in 0..ORDEN_CAMPOS.len() {
        if ficha(prueba).campo == campo {
            return;
        }
        prueba.tecla(KeyCode::Tab);
    }
    panic!("no se llega a {campo:?} con Tab");
}

fn escribir(prueba: &mut AppPrueba, texto: &str) {
    for caracter in texto.chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
}

/// Texto que identifica en pantalla a cada campo (su etiqueta).
fn etiqueta(campo: CampoFicha, estrecho: bool) -> &'static str {
    match campo {
        CampoFicha::Nombre => "Nombre",
        CampoFicha::Direccion => "Dirección",
        CampoFicha::Puerto => "Puerto",
        CampoFicha::Grupo => "Grupo",
        CampoFicha::Etiquetas => "Etiquetas",
        CampoFicha::Usuario => "Usuario",
        CampoFicha::Identidad => "Identidad",
        CampoFicha::Salto => "Salto vía",
        CampoFicha::Snippet => "Snippet",
        CampoFicha::Multiplexar => "Multiplexar",
        CampoFicha::Mantener => "Mantener",
        CampoFicha::Keepalive => "keepalive cada",
        CampoFicha::Salud => "salud del host",
        CampoFicha::Backup => "backup reciente",
        CampoFicha::BackupRuta => "ruta",
        CampoFicha::BackupPatron => "patrón",
        CampoFicha::Tests => "tests en verde",
        CampoFicha::TestsComando if estrecho => "comando",
        CampoFicha::TestsComando => "tests en verde",
        CampoFicha::Servicios => "SERVICIOS",
        CampoFicha::Opciones => "OPCIONES EXTRA",
        CampoFicha::Tuneles => "e editar",
    }
}

/// ¿El campo lleva cursor de texto cuando tiene el foco?
fn lleva_cursor(ficha: &Ficha) -> bool {
    match ficha.campo {
        CampoFicha::Nombre
        | CampoFicha::Direccion
        | CampoFicha::Puerto
        | CampoFicha::Etiquetas
        | CampoFicha::Usuario
        | CampoFicha::BackupRuta
        | CampoFicha::BackupPatron
        | CampoFicha::TestsComando
        | CampoFicha::Servicios
        | CampoFicha::Opciones => true,
        CampoFicha::Keepalive => ficha.mantener,
        _ => false,
    }
}

/// El campo con el foco se ve: su etiqueta y, si es de texto, el cursor.
fn comprobar_foco_a_la_vista(prueba: &AppPrueba) {
    let texto = prueba.texto();
    let ficha = ficha(prueba);
    let (cols, filas) = prueba.tamano_pintado();
    let estrecho = cols < 100;
    assert!(
        texto.contains(etiqueta(ficha.campo, estrecho)),
        "{:?} fuera de la vista a {cols}×{filas}:\n{texto}",
        ficha.campo
    );
    if lleva_cursor(ficha) {
        assert_eq!(
            texto.matches(CURSOR).count(),
            1,
            "sin el cursor de {:?} a {cols}×{filas}:\n{texto}",
            ficha.campo
        );
    }
}

/// El marco de la ficha sigue entero: nada se ha pintado fuera del área.
fn comprobar_marco_entero(prueba: &AppPrueba) {
    let texto = prueba.texto();
    let lineas: Vec<&str> = texto.lines().collect();
    let (_, filas) = prueba.tamano_pintado();
    let ultima = usize::from(filas) - 2;
    assert!(
        lineas[0].starts_with('┌') && lineas[0].ends_with('┐'),
        "{texto}"
    );
    for linea in &lineas[1..ultima] {
        assert!(
            linea.starts_with('│') && linea.ends_with('│'),
            "marco roto:\n{texto}"
        );
    }
    assert!(
        lineas[ultima].starts_with('└') && lineas[ultima].ends_with('┘'),
        "{texto}"
    );
}

/// Da a hetzner-01 seis túneles más (siete en total, más de los que caben en
/// el bloque) y deja un reenvío suelto en sus opciones extra, para que el
/// bloque tenga que desplazarse y enseñar su aviso.
fn con_tuneles_y_reenvio(prueba: &mut AppPrueba) {
    let host_id = ficha(prueba).host_id.expect("host guardado");
    for n in 0..6u16 {
        prueba
            .app
            .almacen
            .crear_tunel(&DatosTunel {
                host_id,
                nombre: format!("extra-{n}"),
                tipo: TipoTunel::Local,
                escucha: format!("127.0.0.1:{}", 6000 + n),
                destino: Some(format!("10.0.0.9:{}", 7000 + n)),
                automatico: false,
            })
            .expect("túnel de prueba");
    }
    prueba.app.recargar_tuneles();
    ir_a(prueba, CampoFicha::Opciones);
    escribir(prueba, "LocalForward 5555 10.0.0.1:5555");
}

/// Con el foco en el bloque de túneles se ven la fila seleccionada y el aviso
/// de reenvíos, y la pista de teclas no se pega a «TÚNELES».
fn comprobar_bloque_tuneles(prueba: &AppPrueba) {
    let texto = prueba.texto();
    let (cols, filas) = prueba.tamano_pintado();
    let ficha = ficha(prueba);
    assert_eq!(ficha.campo, CampoFicha::Tuneles);
    let tuneles = prueba.app.tuneles_de_host(ficha.host_id.unwrap());
    let nombre = &tuneles[ficha.indice_tunel].nombre;
    let seleccionada = texto
        .lines()
        .find(|linea| linea.contains('▸'))
        .unwrap_or_else(|| panic!("sin fila seleccionada a {cols}×{filas}:\n{texto}"));
    assert!(
        seleccionada.contains(nombre.as_str()),
        "la fila seleccionada no es {nombre} a {cols}×{filas}:\n{texto}"
    );
    assert!(
        texto.contains("hay 1 reenvío"),
        "sin el aviso de reenvíos a {cols}×{filas}:\n{texto}"
    );
    assert!(
        texto.contains("TÚNELES n nuevo") || texto.contains("TÚNELES  "),
        "pista pegada a la etiqueta a {cols}×{filas}:\n{texto}"
    );
}

/// Sin glifos Unicode en toda la pantalla (las letras con tilde sí valen).
fn comprobar_ascii(prueba: &AppPrueba) {
    let texto = prueba.texto();
    let (cols, filas) = prueba.tamano_pintado();
    for linea in texto.lines() {
        assert!(
            linea.chars().all(|c| c.is_ascii() || c.is_alphabetic()),
            "glifo Unicode a {cols}×{filas}: {linea}\n{texto}"
        );
    }
}

// ------------------------------------------------------------ instantáneas

#[test]
fn ficha_a_los_tres_tamanos() {
    let mut prueba = ficha_de_host(200, 60);
    prueba.instantanea("ficha_200x60");

    prueba.pasar_por(80, 24);
    prueba.instantanea("ficha_80x24");

    // Por debajo de 50×14: el aviso en lugar de la ficha.
    prueba.pasar_por(40, 12);
    assert!(prueba.app.disposicion().aviso);
    let texto = prueba.texto();
    assert!(texto.contains("ventana demasiado pequeña"), "{texto}");
    assert!(texto.contains("mínimo 50×14"), "{texto}");
    prueba.instantanea("ficha_40x12");
}

#[test]
fn ficha_estrecha_60x20() {
    let prueba = ficha_de_host(60, 20);
    let texto = prueba.texto();
    let lineas: Vec<&str> = texto.lines().collect();
    // Etiquetas encima de los campos.
    let nombre = lineas
        .iter()
        .position(|linea| linea.trim_matches('│').trim() == "Nombre")
        .expect("etiqueta Nombre en su línea");
    assert!(
        lineas[nombre + 1].starts_with("│  [ hetzner-01┃ ]"),
        "{texto}"
    );
    prueba.instantanea("ficha_estrecha_60x20");
}

/// §7.5: con el foco en el último campo del formulario (el comando de los
/// tests, a medio escribir) la ficha baja a 80×16 y el campo vuelve a la
/// vista, con el texto recortado por donde está el cursor.
#[test]
fn ficha_ultimo_campo_80x16() {
    let mut prueba = ficha_de_host(200, 60);
    ir_a(&mut prueba, CampoFicha::Tests);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Tab);
    assert_eq!(ficha(&prueba).campo, CampoFicha::TestsComando);
    escribir(
        &mut prueba,
        "gh run list -R 4d3/cooperapp -L 1 --json conclusion -q '.[0].conclusion' | grep -qx success",
    );
    comprobar_foco_a_la_vista(&prueba);
    prueba.pasar_por(80, 16);
    comprobar_foco_a_la_vista(&prueba);
    let texto = prueba.texto();
    assert!(texto.contains("grep -qx success┃ ]"), "{texto}");
    assert!(texto.contains("[ …"), "{texto}");
    prueba.instantanea("ficha_ultimo_campo_80x16");
}

/// El último campo en el orden de `Tab` es el bloque de túneles: el
/// formulario enseña su final, junto a él.
#[test]
fn ficha_tuneles_80x16() {
    let mut prueba = ficha_de_host(80, 16);
    prueba.tecla_con(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(ficha(&prueba).campo, CampoFicha::Tuneles);
    comprobar_foco_a_la_vista(&prueba);
    prueba.instantanea("ficha_tuneles_80x16");
}

#[test]
fn ficha_desplegable_estrecha_60x20() {
    let mut prueba = ficha_de_host(60, 20);
    ir_a(&mut prueba, CampoFicha::Identidad);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(
        ficha(&prueba).desplegable_abierto,
        Some(CampoFicha::Identidad)
    );
    let texto = prueba.texto();
    // La lista cuelga del valor, que sigue a la vista, y no se sale.
    assert!(texto.contains("│  [ auto"), "{texto}");
    assert!(
        texto.contains("│ contraseña · se pide al conectar"),
        "{texto}"
    );
    comprobar_marco_entero(&prueba);
    prueba.instantanea("ficha_desplegable_estrecha_60x20");
}

#[test]
fn ficha_sugerencias_estrecha_60x20() {
    let mut prueba = ficha_nueva_con(60, 20, Tema::respaldo());
    ir_a(&mut prueba, CampoFicha::Etiquetas);
    prueba.tecla(KeyCode::Char('w'));
    assert_eq!(ficha(&prueba).sugerencias, vec!["web".to_string()]);
    let texto = prueba.texto();
    assert!(texto.contains("│ web"), "{texto}");
    assert!(texto.contains("[ w┃ ]"), "{texto}");
    comprobar_marco_entero(&prueba);
    prueba.instantanea("ficha_sugerencias_estrecha_60x20");
}

#[test]
fn ficha_ascii_estrecha_60x20() {
    let mut prueba = ficha_de_host_con(60, 20, arnes::tema_ascii());
    ir_a(&mut prueba, CampoFicha::Identidad);
    prueba.tecla(KeyCode::Enter);
    comprobar_ascii(&prueba);
    prueba.instantanea("ficha_ascii_estrecha_60x20");
}

// ------------------------------------------------------------------ modos

#[test]
fn etiquetas_al_lado_desde_100_columnas_y_encima_por_debajo() {
    let mut prueba = ficha_de_host(100, 30);
    assert!(
        prueba.texto().contains("│  Nombre       [ hetzner-01┃ ]"),
        "{}",
        prueba.texto()
    );
    prueba.pasar_por(99, 30);
    let texto = prueba.texto();
    assert!(texto.contains("│  Nombre"), "{texto}");
    assert!(texto.contains("│  [ hetzner-01┃ ]"), "{texto}");
    assert!(!texto.contains("Nombre       ["), "{texto}");
    prueba.pasar_por(100, 30);
    assert!(prueba.texto().contains("│  Nombre       [ hetzner-01┃ ]"));
}

/// En ventanas muy grandes el formulario no crece sin límite: se centra.
#[test]
fn a_200x60_el_contenido_tiene_ancho_maximo_y_se_centra() {
    let prueba = ficha_de_host(200, 60);
    let texto = prueba.texto();
    let columna_de = |aguja: &str| {
        texto
            .lines()
            .find_map(|linea| linea.find(aguja).map(|byte| linea[..byte].chars().count()))
            .unwrap_or_else(|| panic!("«{aguja}» no está:\n{texto}"))
    };
    // 198 de interior, 120 de contenido: 39 de margen a cada lado.
    assert_eq!(columna_de("IDENTIFICACIÓN"), 1 + 39 + 1);
    let servicios = texto
        .lines()
        .find(|linea| linea.contains("SERVICIOS"))
        .unwrap();
    assert_eq!(servicios.chars().position(|c| c == '┌'), Some(1 + 39));
    let fin = servicios.chars().rev().position(|c| c == '┐').unwrap();
    assert_eq!(fin, 1 + 39, "{servicios}");
}

// ------------------------------------------------------------------ foco

/// El campo con el foco se ve siempre, tabulando hacia delante y hacia atrás,
/// a todos los tamaños en los que se pinta la ficha.
#[test]
fn el_foco_siempre_a_la_vista_al_tabular() {
    for (cols, filas) in TAMANOS_PINTADOS {
        let mut prueba = ficha_de_host(cols, filas);
        for _ in 0..ORDEN_CAMPOS.len() {
            comprobar_foco_a_la_vista(&prueba);
            prueba.tecla(KeyCode::Tab);
        }
        assert_eq!(ficha(&prueba).campo, CampoFicha::Nombre);
        for _ in 0..ORDEN_CAMPOS.len() {
            prueba.tecla_con(KeyCode::BackTab, KeyModifiers::SHIFT);
            comprobar_foco_a_la_vista(&prueba);
        }
    }
}

/// §7.5: el campo con el foco que queda fuera al encoger vuelve a la vista,
/// sea cual sea, y al volver al tamaño anterior se pinta igual (nada se
/// guarda entre pintados).
#[test]
fn el_campo_que_queda_fuera_vuelve_a_la_vista() {
    for campo in ORDEN_CAMPOS {
        let mut prueba = ficha_de_host(200, 60);
        ir_a(&mut prueba, campo);
        let mut antes = None;
        for (cols, filas) in [(80, 16), (50, 14), (60, 20), (200, 60), (80, 16)] {
            prueba.pasar_por(cols, filas);
            comprobar_foco_a_la_vista(&prueba);
            if (cols, filas) == (80, 16) {
                let texto = prueba.texto();
                if let Some(antes) = &antes {
                    assert_eq!(antes, &texto, "{campo:?}: 80×16 no se repite");
                }
                antes = Some(texto);
            }
        }
    }
}

/// Un área de texto con más líneas que filas deja a la vista la del cursor.
#[test]
fn el_area_de_texto_con_foco_deja_ver_la_linea_del_cursor() {
    let mut prueba = ficha_de_host(60, 20);
    ir_a(&mut prueba, CampoFicha::Servicios);
    for (indice, servicio) in ["nginx", "postgresql", "redis"].iter().enumerate() {
        if indice > 0 {
            prueba.tecla(KeyCode::Enter);
        }
        escribir(&mut prueba, servicio);
    }
    let texto = prueba.texto();
    assert!(texto.contains("redis┃"), "{texto}");
    assert!(!texto.contains("nginx"), "{texto}");
    prueba.pasar_por(200, 60);
    let texto = prueba.texto();
    for servicio in ["nginx", "postgresql", "redis┃"] {
        assert!(texto.contains(servicio), "{texto}");
    }
}

// ------------------------------------------------------- listas emergentes

/// Los desplegables se anclan a su campo según el modo, sin taparlo y sin
/// salirse nunca del marco de la ficha.
#[test]
fn los_desplegables_nunca_se_salen_del_area() {
    let casos = [
        (CampoFicha::Grupo, "│ sin grupo", "produccion"),
        (CampoFicha::Identidad, "│ auto", "[ auto"),
        (CampoFicha::Salto, "│ ninguno", "Salto vía"),
        (CampoFicha::Snippet, "│ ninguno", "Snippet"),
    ];
    for (cols, filas) in TAMANOS_PINTADOS {
        for (campo, primera_opcion, campo_visible) in casos {
            let mut prueba = ficha_de_host(cols, filas);
            ir_a(&mut prueba, campo);
            prueba.tecla(KeyCode::Enter);
            assert_eq!(ficha(&prueba).desplegable_abierto, Some(campo));
            let texto = prueba.texto();
            assert!(
                texto.contains(primera_opcion),
                "{campo:?} sin lista a {cols}×{filas}:\n{texto}"
            );
            assert!(
                texto.contains(campo_visible),
                "{campo:?} tapado a {cols}×{filas}:\n{texto}"
            );
            comprobar_marco_entero(&prueba);
            prueba.tecla(KeyCode::Esc);
            assert_eq!(ficha(&prueba).desplegable_abierto, None);
        }
    }
}

#[test]
fn las_sugerencias_nunca_se_salen_del_area() {
    for (cols, filas) in TAMANOS_PINTADOS {
        let mut prueba = ficha_nueva_con(cols, filas, Tema::respaldo());
        ir_a(&mut prueba, CampoFicha::Etiquetas);
        prueba.tecla(KeyCode::Char('w'));
        let texto = prueba.texto();
        assert!(
            texto.contains("│ web"),
            "sin sugerencias a {cols}×{filas}:\n{texto}"
        );
        assert!(
            texto.contains("[ w┃ ]"),
            "campo tapado a {cols}×{filas}:\n{texto}"
        );
        comprobar_marco_entero(&prueba);
    }
}

// ------------------------------------------------------------------ estado

/// Secuencia 200×60 → 80×24 → 40×12 → 200×60: el texto a medio escribir, el
/// cursor y el foco se conservan; con el aviso las teclas no lo tocan.
#[test]
fn el_texto_a_medio_escribir_y_el_foco_se_conservan() {
    let mut prueba = ficha_de_host(200, 60);
    ir_a(&mut prueba, CampoFicha::Direccion);
    escribir(&mut prueba, "abc");
    prueba.tecla(KeyCode::Left);
    prueba.tecla(KeyCode::Left);
    let comprobar = |prueba: &AppPrueba| {
        let ficha = ficha(prueba);
        assert_eq!(ficha.campo, CampoFicha::Direccion);
        assert_eq!(ficha.direccion.texto, "10.0.1.11abc");
        assert_eq!(ficha.direccion.cursor, 10);
    };
    comprobar(&prueba);
    for (cols, filas) in [(80, 24), (40, 12), (200, 60)] {
        prueba.pasar_por(cols, filas);
        comprobar(&prueba);
        if prueba.app.disposicion().aviso {
            // Con el aviso, una letra no escribe en la ficha.
            prueba.tecla(KeyCode::Char('z'));
            comprobar(&prueba);
        } else {
            let texto = prueba.texto();
            assert!(texto.contains("10.0.1.11a┃bc"), "{cols}×{filas}:\n{texto}");
        }
    }
}

/// La misma secuencia con un desplegable abierto y filtrado: sigue abierto,
/// con su filtro y su opción a la vista.
#[test]
fn el_desplegable_abierto_y_su_filtro_se_conservan() {
    let mut prueba = ficha_de_host(200, 60);
    ir_a(&mut prueba, CampoFicha::Salto);
    prueba.tecla(KeyCode::Enter);
    escribir(&mut prueba, "vps");
    for (cols, filas) in [(80, 24), (40, 12), (200, 60), (60, 20), (50, 14)] {
        prueba.pasar_por(cols, filas);
        let ficha = ficha(&prueba);
        assert_eq!(ficha.campo, CampoFicha::Salto);
        assert_eq!(ficha.desplegable_abierto, Some(CampoFicha::Salto));
        assert_eq!(ficha.salto.filtro.texto, "vps");
        if !prueba.app.disposicion().aviso {
            let texto = prueba.texto();
            assert!(texto.contains("│ vps-openclaw"), "{cols}×{filas}:\n{texto}");
            assert!(
                texto.contains(" vps "),
                "sin el filtro a {cols}×{filas}:\n{texto}"
            );
            comprobar_marco_entero(&prueba);
        }
    }
    // Elegir sigue funcionando tras los cambios.
    prueba.tecla(KeyCode::Enter);
    assert_eq!(ficha(&prueba).salto.etiqueta_seleccionada(), "vps-openclaw");
}

/// Las sugerencias de etiquetas y su selección sobreviven a la secuencia.
#[test]
fn las_sugerencias_se_conservan() {
    let mut prueba = ficha_nueva_con(200, 60, Tema::respaldo());
    ir_a(&mut prueba, CampoFicha::Etiquetas);
    prueba.tecla(KeyCode::Char('w'));
    for (cols, filas) in [(80, 24), (40, 12), (200, 60)] {
        prueba.pasar_por(cols, filas);
        let ficha = ficha(&prueba);
        assert_eq!(ficha.campo, CampoFicha::Etiquetas);
        assert_eq!(ficha.etiquetas.texto, "w");
        assert_eq!(ficha.sugerencias, vec!["web".to_string()]);
        if !prueba.app.disposicion().aviso {
            assert!(prueba.texto().contains("│ web"), "{}", prueba.texto());
        }
    }
    prueba.tecla(KeyCode::Enter);
    assert_eq!(ficha(&prueba).etiquetas.texto, "web ");
}

// ------------------------------------------------------------------ ASCII

/// Con `MAGI_ASCII` la ficha no pinta ningún glifo Unicode a ningún tamaño ni
/// modo, tampoco con cursor, desplegable o sugerencias.
#[test]
fn en_ascii_la_ficha_no_pinta_glifos_unicode() {
    // Por debajo del mínimo, el aviso (sin barra) en lugar de la ficha.
    let prueba = ficha_de_host_con(40, 12, arnes::tema_ascii());
    assert!(prueba.app.disposicion().aviso);
    comprobar_ascii(&prueba);

    for (cols, filas) in [(80, 24), (200, 60), (60, 20), (50, 14), (80, 16)] {
        let mut prueba = ficha_de_host_con(cols, filas, arnes::tema_ascii());
        let aviso = prueba.app.disposicion().aviso;
        assert!(!aviso);
        comprobar_ascii(&prueba);
        // Con un texto recortado por los dos lados y el cursor en medio.
        ir_a(&mut prueba, CampoFicha::Tests);
        prueba.tecla(KeyCode::Char(' '));
        prueba.tecla(KeyCode::Tab);
        escribir(
            &mut prueba,
            "gh run list -R 4d3/cooperapp -L 1 --json conclusion -q '.[0].conclusion' | grep -qx success · → …",
        );
        for _ in 0..40 {
            prueba.tecla(KeyCode::Left);
        }
        comprobar_ascii(&prueba);
        // Desplegable con opciones que llevan `·` y el bloque de túneles.
        ir_a(&mut prueba, CampoFicha::Identidad);
        prueba.tecla(KeyCode::Enter);
        comprobar_ascii(&prueba);
        prueba.tecla(KeyCode::Esc);
        ir_a(&mut prueba, CampoFicha::Tuneles);
        comprobar_ascii(&prueba);

        let mut nueva = ficha_nueva_con(cols, filas, arnes::tema_ascii());
        ir_a(&mut nueva, CampoFicha::Etiquetas);
        nueva.tecla(KeyCode::Char('w'));
        comprobar_ascii(&nueva);
    }
}

// ------------------------------------------------------- bloque de túneles

/// Mínimo de la ficha (50×14) con el foco en el bloque de túneles: la fila
/// seleccionada, el aviso de reenvíos y la pista de teclas separada de su
/// etiqueta caben a la vez.
#[test]
fn ficha_tuneles_aviso_50x14() {
    let mut prueba = ficha_de_host(50, 14);
    con_tuneles_y_reenvio(&mut prueba);
    ir_a(&mut prueba, CampoFicha::Tuneles);
    for _ in 0..3 {
        prueba.tecla(KeyCode::Down);
    }
    comprobar_bloque_tuneles(&prueba);
    comprobar_marco_entero(&prueba);
    prueba.instantanea("ficha_tuneles_aviso_50x14");
}

/// El aviso de reenvíos tiene su fila reservada: no lo tapa la selección del
/// bloque en ventanas bajas, y la selección sigue a la vista al moverla.
#[test]
fn el_aviso_de_reenvios_y_la_seleccion_de_tuneles_se_ven_a_todos_los_tamanos() {
    for (cols, filas) in TAMANOS_PINTADOS.into_iter().chain([(100, 14)]) {
        let mut prueba = ficha_de_host(cols, filas);
        con_tuneles_y_reenvio(&mut prueba);
        ir_a(&mut prueba, CampoFicha::Tuneles);
        comprobar_bloque_tuneles(&prueba);
        for _ in 0..10 {
            prueba.tecla(KeyCode::Down);
            comprobar_bloque_tuneles(&prueba);
        }
        assert_eq!(ficha(&prueba).indice_tunel, 6);
        for _ in 0..3 {
            prueba.tecla(KeyCode::Up);
            comprobar_bloque_tuneles(&prueba);
        }
        // Con el foco en otro campo, el aviso sigue a la vista.
        prueba.tecla(KeyCode::Tab);
        assert!(
            prueba.texto().contains("hay 1 reenvío"),
            "{cols}×{filas}:\n{}",
            prueba.texto()
        );
    }
    // Y en ASCII, sin glifos Unicode, al mínimo de la ficha.
    let mut prueba = ficha_de_host_con(50, 14, arnes::tema_ascii());
    con_tuneles_y_reenvio(&mut prueba);
    ir_a(&mut prueba, CampoFicha::Tuneles);
    let texto = prueba.texto();
    assert!(texto.contains("hay 1 reenvío"), "{texto}");
    assert!(texto.contains("TÚNELES n nuevo - e editar"), "{texto}");
    comprobar_ascii(&prueba);
}

/// Secuencia 200×60 → 80×24 → 40×12 → 200×60 → 50×14 con el foco en el
/// bloque de túneles: la selección se conserva y sigue a la vista.
#[test]
fn la_seleccion_de_tuneles_se_conserva_al_cambiar_de_tamano() {
    let mut prueba = ficha_de_host(200, 60);
    con_tuneles_y_reenvio(&mut prueba);
    ir_a(&mut prueba, CampoFicha::Tuneles);
    for _ in 0..5 {
        prueba.tecla(KeyCode::Down);
    }
    assert_eq!(ficha(&prueba).indice_tunel, 5);
    for (cols, filas) in [(80, 24), (40, 12), (200, 60), (50, 14)] {
        prueba.pasar_por(cols, filas);
        assert_eq!(ficha(&prueba).campo, CampoFicha::Tuneles);
        assert_eq!(ficha(&prueba).indice_tunel, 5);
        if !prueba.app.disposicion().aviso {
            comprobar_bloque_tuneles(&prueba);
        }
    }
}

// ------------------------------------------------------------ marcas

/// Las marcas ↑/↓ solo salen si lo escondido tiene contenido: el separador en
/// blanco del principio o del final del formulario no cuenta.
#[test]
fn las_marcas_no_senalan_solo_separadores() {
    // 130×30: caben 22 de las 23 líneas del formulario en modo normal.
    let sin_barra = |prueba: &AppPrueba| {
        let texto = prueba.texto();
        let lineas: Vec<&str> = texto.lines().collect();
        lineas[..lineas.len() - 1].join("\n")
    };
    let mut prueba = ficha_de_host(130, 30);
    let texto = sin_barra(&prueba);
    assert!(!texto.contains('↓') && !texto.contains('↑'), "{texto}");
    // Con el foco en los tests se esconde solo la línea en blanco de arriba.
    ir_a(&mut prueba, CampoFicha::Tests);
    let texto = sin_barra(&prueba);
    assert!(texto.contains("VERIFICACIONES PREVIAS"), "{texto}");
    assert!(!texto.contains('↓') && !texto.contains('↑'), "{texto}");
    // Donde sí queda contenido fuera, las marcas siguen saliendo.
    prueba.pasar_por(100, 20);
    let texto = sin_barra(&prueba);
    assert!(texto.contains('↑'), "{texto}");
    ir_a(&mut prueba, CampoFicha::Nombre);
    let texto = sin_barra(&prueba);
    assert!(texto.contains('↓') && !texto.contains('↑'), "{texto}");
}

// ------------------------------------------------------------ extremos

/// Pinta la ficha directamente en `area` (sin el aviso de la base, que la
/// protege por debajo de 50×14), en modo normal o estrecho.
fn pintar_ficha_en(prueba: &AppPrueba, area: Rect, estrecho: bool) {
    let mut terminal =
        Terminal::new(TestBackend::new(area.right() + 1, area.bottom() + 1)).expect("terminal");
    terminal
        .draw(|marco| {
            let pantalla = if estrecho {
                Rect::new(0, 0, 60, 20)
            } else {
                Rect::new(0, 0, 120, 40)
            };
            let mut disp =
                Disposicion::nueva(pantalla, disposicion::minimo_de(Vista::Ficha, false));
            magi::ui::ficha::dibujar(marco, area, &prueba.app, &mut disp);
        })
        .expect("pintado");
}

/// Ninguna resta ni índice de la ficha entra en pánico con áreas diminutas
/// (anchos y altos de 0 a 3 y algo más), con el foco en cualquier campo, con
/// un desplegable abierto o con sugerencias.
#[test]
fn la_ficha_no_entra_en_panico_en_areas_diminutas() {
    const LADOS: [u16; 6] = [0, 1, 2, 3, 5, 13];
    let pintar_todo = |prueba: &AppPrueba| {
        for ancho in LADOS {
            for alto in LADOS {
                for estrecho in [false, true] {
                    pintar_ficha_en(prueba, Rect::new(1, 1, ancho, alto), estrecho);
                }
            }
        }
    };
    let mut prueba = ficha_de_host(80, 24);
    con_tuneles_y_reenvio(&mut prueba);
    prueba.app.identidades.agente = None;
    prueba.app.identidades.aviso_agente = Some("sin agente".to_string());
    for campo in ORDEN_CAMPOS {
        ir_a(&mut prueba, campo);
        pintar_todo(&prueba);
        if matches!(
            campo,
            CampoFicha::Grupo | CampoFicha::Identidad | CampoFicha::Salto | CampoFicha::Snippet
        ) {
            prueba.tecla(KeyCode::Enter);
            assert_eq!(ficha(&prueba).desplegable_abierto, Some(campo));
            pintar_todo(&prueba);
            prueba.tecla(KeyCode::Esc);
        }
    }
    let mut nueva = ficha_nueva_con(80, 24, Tema::respaldo());
    ir_a(&mut nueva, CampoFicha::Etiquetas);
    nueva.tecla(KeyCode::Char('w'));
    assert!(!ficha(&nueva).sugerencias.is_empty());
    pintar_todo(&nueva);
}

/// Un desplegable encogido (al mínimo de la ficha no caben sus siete
/// opciones) sigue a la opción resaltada al bajar y al subir.
#[test]
fn el_desplegable_encogido_sigue_a_la_opcion_resaltada() {
    let mut prueba = ficha_de_host(50, 14);
    ir_a(&mut prueba, CampoFicha::Salto);
    prueba.tecla(KeyCode::Enter);
    let opciones: Vec<String> = ficha(&prueba)
        .salto
        .opciones
        .iter()
        .map(|opcion| opcion.etiqueta.clone())
        .collect();
    let visibles = |prueba: &AppPrueba| {
        let texto = prueba.texto();
        opciones
            .iter()
            .filter(|opcion| texto.contains(&format!("│ {opcion}")))
            .count()
    };
    assert!(
        visibles(&prueba) < opciones.len(),
        "el desplegable debería ir encogido:\n{}",
        prueba.texto()
    );
    for paso in 0..opciones.len() * 2 {
        let resaltado = ficha(&prueba).salto.resaltado;
        let texto = prueba.texto();
        assert!(
            texto.contains(&format!("│ {}", opciones[resaltado])),
            "paso {paso}: {} fuera de la vista:\n{texto}",
            opciones[resaltado]
        );
        comprobar_marco_entero(&prueba);
        let tecla = if paso < opciones.len() {
            KeyCode::Down
        } else {
            KeyCode::Up
        };
        prueba.tecla(tecla);
    }
}
