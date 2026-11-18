//! Fase 17 (Termux), grupo Q: el diálogo FILA DE TECLAS DE TERMUX a 40×12,
//! 80×24, 200×60 y 48×20 (un móvil en vertical, T45).
//!
//! El diálogo se abre ya hecho (`abrir_dialogo`) con una propuesta fabricada a
//! mano, sin leer ningún `termux.properties`: su camino de teclas, la lectura
//! del fichero y la instalación se prueban en `tests/termux_teclas.rs`. La
//! fecha de la copia es fija para que las instantáneas no cambien con el día.

use std::path::PathBuf;

use magi::app::termux_teclas::DialogoTeclasTermux;
use magi::app::Dialogo;
use magi::termux::teclas::{self, ErrorTeclas, EstadoFila, Propuesta};
use magi::ui::disposicion::{ESTRECHO_TECLAS_TERMUX, MINIMO_TECLAS_TERMUX};

use crate::arnes::{self, AppPrueba};
use crate::vistas_g::FabricaDialogo;

/// Los tres tamaños de la fase y el de un móvil en vertical.
const TAMANOS: [(u16, u16); 4] = [(40, 12), (80, 24), (200, 60), (48, 20)];

const COPIA: &str = "termux.properties.magi-2026-11-17";

/// Una propuesta como la que arma `teclas::preparar`, sin tocar el disco.
pub(crate) fn propuesta(estado: EstadoFila, copia: Option<&str>, prefijo: &str) -> Propuesta {
    let (arriba, abajo) = teclas::teclas_de_la_fila(prefijo);
    Propuesta {
        ruta: Some(PathBuf::from("/home/usuario/.termux/termux.properties")),
        ruta_visible: "~/.termux/termux.properties".to_string(),
        estado,
        fila: teclas::fila(prefijo),
        arriba,
        abajo,
        actual: None,
        copia: copia.map(str::to_string),
        aviso_prefijo: teclas::macro_del_prefijo(prefijo).err(),
    }
}

fn fecha() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 11, 17).expect("fecha")
}

/// El diálogo de siempre: otra fila ya puesta y copia de por medio.
pub(crate) fn dialogo_de(propuesta: Propuesta, prefijo: &str) -> DialogoTeclasTermux {
    DialogoTeclasTermux::nuevo(propuesta, prefijo.to_string(), fecha())
}

fn abierto(propuesta: Propuesta, prefijo: &str) -> Dialogo {
    Dialogo::TeclasTermux(Box::new(dialogo_de(propuesta, prefijo)))
}

/// Fábrica para el barrido y la red de seguridad de `vistas_g` (sin pánico en
/// áreas diminutas, entero a 40×12, ASCII).
pub(crate) fn dialogos() -> Vec<(&'static str, FabricaDialogo)> {
    vec![(
        "FILA DE TECLAS DE TERMUX",
        Box::new(|| abierto(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]")),
    )]
}

fn abrir(prueba: &mut AppPrueba, dialogo: Dialogo) {
    prueba.app.abrir_dialogo(dialogo);
    prueba.paso_en(std::time::Instant::now());
}

// ---------------------------------------------------------------- instantáneas

#[test]
fn instantaneas_del_dialogo() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(
            &mut prueba,
            abierto(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]"),
        );
        prueba.instantanea(&format!("termux_teclas_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_de_los_estados_del_dialogo() {
    for (cols, filas) in [(40, 12), (80, 24), (48, 20)] {
        // Sin fichero: se crea y no hay copia.
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(
            &mut prueba,
            abierto(propuesta(EstadoFila::SinFichero, None, "Ctrl+]"), "Ctrl+]"),
        );
        prueba.instantanea(&format!("termux_teclas_sin_fichero_{cols}x{filas}"));

        // Instalando: sin teclas.
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        let mut dialogo = dialogo_de(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]");
        dialogo.trabajando = Some(1);
        abrir(&mut prueba, Dialogo::TeclasTermux(Box::new(dialogo)));
        prueba.instantanea(&format!("termux_teclas_instalando_{cols}x{filas}"));

        // Un fallo (la copia): el diálogo sigue para reintentar.
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        let mut dialogo = dialogo_de(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]");
        dialogo.error = Some(
            "no se pudo hacer la copia de termux.properties (permiso denegado): no se ha escrito nada"
                .to_string(),
        );
        abrir(&mut prueba, Dialogo::TeclasTermux(Box::new(dialogo)));
        prueba.instantanea(&format!("termux_teclas_error_{cols}x{filas}"));

        // Con otro prefijo configurado y con uno que no cabe como macro.
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(
            &mut prueba,
            abierto(
                propuesta(EstadoFila::SinExtraKeys, Some(COPIA), "Ctrl+a"),
                "Ctrl+a",
            ),
        );
        prueba.instantanea(&format!("termux_teclas_prefijo_a_{cols}x{filas}"));

        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(
            &mut prueba,
            abierto(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+'"), "Ctrl+'"),
        );
        prueba.instantanea(&format!("termux_teclas_sin_macro_{cols}x{filas}"));
    }
}

/// La ayuda (`?`) de Flota en Termux, desplazada hasta el final: recuerda
/// Volumen arriba + 1…9 (F1…F9) y + 0 (F10). Fuera de Termux la ayuda es la
/// de siempre (sus instantáneas no cambian).
#[test]
fn instantanea_de_la_ayuda_en_termux() {
    use crossterm::event::KeyCode;
    for (cols, filas) in [(48, 20), (80, 24)] {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        prueba.app.termux.activo = true;
        prueba.tecla(KeyCode::Char('?'));
        for _ in 0..4 {
            prueba.tecla(KeyCode::PageDown);
        }
        let texto = prueba.texto();
        assert!(texto.contains("Volumen arriba"), "{texto}");
        prueba.instantanea(&format!("termux_ayuda_{cols}x{filas}"));
    }
    // Sin Termux, ni rastro de esa línea (ayuda larga: se mira entera).
    let (mut prueba, _) = AppPrueba::con_semilla(80, 70);
    prueba.tecla(KeyCode::Char('?'));
    assert!(!prueba.texto().contains("Volumen arriba"));
}

// ---------------------------------------------------------------- disposición

/// El diálogo se ve entero (recuadro cerrado) y con sus teclas a los cuatro
/// tamaños, con el tema Unicode y con el ASCII, y en ASCII no pinta ningún
/// glifo fuera de él.
#[test]
fn el_dialogo_se_ve_entero_con_sus_teclas_y_en_ascii() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        abrir(
            &mut prueba,
            abierto(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]"),
        );
        let texto = prueba.texto();
        assert!(
            texto.contains("FILA DE TECLAS DE TERMUX") && texto.contains('└'),
            "cortado a {cols}×{filas}:\n{texto}"
        );
        for esperado in ["↵ instalar", "Esc", COPIA, "^]", "F9"] {
            assert!(
                texto.contains(esperado),
                "falta «{esperado}» a {cols}×{filas}:\n{texto}"
            );
        }

        let (mut prueba, _) = AppPrueba::con_semilla_y_tema(cols, filas, arnes::tema_ascii());
        abrir(
            &mut prueba,
            abierto(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]"),
        );
        let texto = prueba.texto();
        let malos: String = texto
            .chars()
            .filter(|c| !(c.is_ascii() || c.is_alphabetic()))
            .collect();
        assert!(
            malos.is_empty(),
            "glifos {malos:?} en ASCII a {cols}×{filas}:\n{texto}"
        );
        // Y lo que dice sigue ahí, degradado: el título, la fila propuesta (con
        // las flechas en ASCII), el nombre de la copia, las teclas y el
        // recuadro cerrado por abajo. Sin glifos no basta: un diálogo en
        // blanco también estaría libre de ellos.
        for esperado in [
            "FILA DE TECLAS DE TERMUX",
            "MAGI propone esta fila:",
            "ESC TAB CTRL ALT ^] <- v  ^  ->",
            "F1  F2  F3   F4  F5 F6 F7 F8 F9",
            "extra-keys",
            COPIA,
            "enter instalar",
            "Esc",
        ] {
            assert!(
                texto.contains(esperado),
                "falta «{esperado}» en ASCII a {cols}×{filas}:\n{texto}"
            );
        }
        assert!(
            texto
                .lines()
                .skip_while(|linea| !linea.contains("enter instalar"))
                .any(|linea| linea.contains("+-")),
            "el recuadro no se cierra en ASCII a {cols}×{filas}:\n{texto}"
        );
    }
}

/// Las dos filas de la propuesta quedan alineadas por columnas: `F1` bajo
/// `ESC`, `F5` bajo `^]`, `F9` bajo `→`.
#[test]
fn las_dos_filas_de_la_propuesta_quedan_alineadas() {
    let p = propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]");
    let (arriba, abajo) = magi::ui::termux_teclas::filas_alineadas(&p.arriba, &p.abajo, false);
    assert_eq!(arriba, "ESC TAB CTRL ALT ^] ←  ↓  ↑  →");
    assert_eq!(abajo, "F1  F2  F3   F4  F5 F6 F7 F8 F9");
    // En ASCII las flechas ocupan dos columnas y las columnas se ensanchan.
    let (arriba, abajo) = magi::ui::termux_teclas::filas_alineadas(&p.arriba, &p.abajo, true);
    assert_eq!(arriba, "ESC TAB CTRL ALT ^] <- v  ^  ->");
    assert_eq!(abajo, "F1  F2  F3   F4  F5 F6 F7 F8 F9");
    for fila in [&arriba, &abajo] {
        assert!(fila.is_ascii(), "{fila}");
    }
}

/// El mínimo del diálogo es el de la fase y a 48 columnas (un móvil) no está
/// en modo estrecho; a 40 sí.
#[test]
fn el_minimo_del_dialogo_es_40x12() {
    assert_eq!(
        (MINIMO_TECLAS_TERMUX.cols, MINIMO_TECLAS_TERMUX.filas),
        (40, 12)
    );
    const { assert!(48 >= ESTRECHO_TECLAS_TERMUX && 40 < ESTRECHO_TECLAS_TERMUX) };
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(
        &mut prueba,
        abierto(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]"),
    );
    let minimo = magi::ui::disposicion::minimo_dialogo(
        prueba.app.dialogo.as_ref().expect("diálogo abierto"),
    )
    .expect("el diálogo declara su mínimo");
    assert_eq!(minimo.tamano, MINIMO_TECLAS_TERMUX);
    assert_eq!(minimo.exige, magi::ui::termux_teclas::TITULO);
}

// ---------------------------------------------------------------- desplazamiento

/// Lo que dice el diálogo cuando falla la copia (la misma frase de las
/// instantáneas de `error`).
const ERROR_COPIA: &str =
    "no se pudo hacer la copia de termux.properties (permiso denegado): no se ha escrito nada";

/// Los casos que a 40×12 no caben y se tienen que poder recorrer: un prefijo
/// que no cabe como macro (la fila sin él y un renglón más), con otra fila y
/// sin `extra-keys`, y un fallo anterior en el pie.
fn caso_largo(nombre: &str) -> Dialogo {
    let mut dialogo = match nombre {
        "sin_macro" => dialogo_de(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+'"), "Ctrl+'"),
        "sin_extra_keys" => dialogo_de(
            propuesta(EstadoFila::SinExtraKeys, Some(COPIA), "Ctrl+'"),
            "Ctrl+'",
        ),
        "error" | "error_tras_copia" => {
            dialogo_de(propuesta(EstadoFila::Otra, Some(COPIA), "Ctrl+]"), "Ctrl+]")
        }
        otro => panic!("caso desconocido: {otro}"),
    };
    if nombre == "error" {
        dialogo.error = Some(ERROR_COPIA.to_string());
    }
    if nombre == "error_tras_copia" {
        // La escritura falla con la copia ya hecha (K4).
        dialogo.error = Some(
            ErrorTeclas::EscrituraTrasCopia {
                motivo: "permiso denegado".to_string(),
                copia: COPIA.to_string(),
            }
            .to_string(),
        );
    }
    Dialogo::TeclasTermux(Box::new(dialogo))
}

/// Pulsa `tecla` hasta que el desplazamiento deje de cambiar y devuelve cada
/// página vista, la de partida incluida.
fn paginas_con(prueba: &mut AppPrueba, tecla: crossterm::event::KeyCode) -> Vec<String> {
    let mut paginas = vec![prueba.texto()];
    for _ in 0..40 {
        let antes = prueba.app.desplazamiento_modal;
        prueba.tecla(tecla);
        if prueba.app.desplazamiento_modal == antes {
            break;
        }
        paginas.push(prueba.texto());
    }
    paginas
}

/// K1: el diálogo es desplazable. A 40×12, con un cuerpo que no cabe, ↑↓ lo
/// recorren y todo lo que va a hacer `↵` (dónde escribe y el nombre de la
/// copia) se puede leer antes de confirmar. Antes, ↑↓ no hacían nada y esas
/// líneas no se veían nunca.
#[test]
fn a_40x12_se_recorre_todo_lo_que_va_a_hacer_antes_de_instalar() {
    use crossterm::event::KeyCode;
    for (nombre, esperado) in [
        (
            "sin_macro",
            vec!["Sin el prefijo", "solo extra-keys. Copia previa:", COPIA],
        ),
        (
            "sin_extra_keys",
            vec!["Sin el prefijo", "solo extra-keys. Copia previa:", COPIA],
        ),
        (
            "error",
            vec![
                "solo extra-keys. Copia previa:",
                COPIA,
                "(permiso",
                "denegado): no se ha escrito nada",
            ],
        ),
        (
            "error_tras_copia",
            vec!["solo extra-keys. Copia previa:", COPIA, "ya está hecha"],
        ),
    ] {
        let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
        abrir(&mut prueba, caso_largo(nombre));
        // Sin mover nada, parte del texto no se ve (la línea que dice dónde se
        // escribe y que hay copia): es lo que había que poder recorrer.
        let primera = prueba.texto();
        assert!(
            primera.contains("↑↓ 1/") && !primera.contains("solo extra-keys. Copia previa:"),
            "{nombre}: debería no caber y no enseñar aún lo que va a hacer:\n{primera}"
        );

        let paginas = paginas_con(&mut prueba, KeyCode::Down);
        assert!(paginas.len() > 1, "{nombre}: ↓ no desplaza:\n{primera}");
        let todo = paginas.join("\n");
        for frase in &esperado {
            assert!(
                todo.contains(frase),
                "{nombre}: no se llega a leer «{frase}»:\n{todo}"
            );
        }
        // El pie con las teclas va fijo en todas las páginas, y la última (el
        // fondo del recorrido) enseña la copia.
        for pagina in &paginas {
            assert!(pagina.contains("↵ instalar"), "{nombre}:\n{pagina}");
        }
        let ultima = paginas.last().expect("páginas");
        assert!(ultima.contains(COPIA), "{nombre}:\n{ultima}");
        let posiciones = paginas.len();
        assert!(
            ultima.contains(&format!("↑↓ {posiciones}/{posiciones}")),
            "{nombre}: el indicador no llega al final:\n{ultima}"
        );

        // Y se vuelve arriba con ↑.
        let de_vuelta = paginas_con(&mut prueba, KeyCode::Up);
        assert_eq!(de_vuelta.last(), Some(&primera), "{nombre}");

        // AvPág y RePág también.
        prueba.tecla(KeyCode::PageDown);
        assert!(prueba.texto().contains(COPIA), "{nombre}");
        prueba.tecla(KeyCode::PageUp);
        assert_eq!(prueba.texto(), primera, "{nombre}");

        // Desplazar no confirma ni cierra nada: el diálogo sigue ahí y sin
        // ninguna instalación en vuelo.
        match &prueba.app.dialogo {
            Some(Dialogo::TeclasTermux(dialogo)) => assert_eq!(dialogo.trabajando, None),
            otro => panic!(
                "{nombre}: el diálogo debería seguir abierto: {}",
                otro.is_some()
            ),
        }
    }
}

/// K1, la mitad ASCII: en ASCII también se recorre y el indicador no usa
/// flechas Unicode.
#[test]
fn a_40x12_en_ascii_tambien_se_recorre_todo() {
    use crossterm::event::KeyCode;
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(40, 12, arnes::tema_ascii());
    abrir(&mut prueba, caso_largo("sin_macro"));
    let primera = prueba.texto();
    assert!(primera.contains("^v 1/"), "{primera}");
    let todo = paginas_con(&mut prueba, KeyCode::Down).join("\n");
    for frase in ["Sin el prefijo", "solo extra-keys. Copia previa:", COPIA] {
        assert!(todo.contains(frase), "falta «{frase}»:\n{todo}");
    }
    assert!(todo.contains("enter instalar"), "{todo}");
    let malos: String = todo
        .chars()
        .filter(|c| !(c.is_ascii() || c.is_alphabetic()))
        .collect();
    assert!(malos.is_empty(), "glifos {malos:?} en ASCII:\n{todo}");
}

/// Cuando el diálogo cabe (80×24), ↑↓ no hacen nada: ni indicador ni cambio.
#[test]
fn cuando_cabe_las_flechas_no_mueven_nada() {
    use crossterm::event::KeyCode;
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    abrir(&mut prueba, caso_largo("sin_macro"));
    let antes = prueba.texto();
    assert!(!antes.contains("↑↓"), "{antes}");
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::PageDown);
    assert_eq!(prueba.texto(), antes);
    assert_eq!(prueba.app.desplazamiento_modal, 0);
}

/// Instantáneas del final del recorrido de los casos que a 40×12 no caben
/// (las del principio salen de las de los estados).
#[test]
fn instantaneas_del_dialogo_desplazado_a_40x12() {
    use crossterm::event::KeyCode;
    for nombre in ["sin_macro", "error", "error_tras_copia", "sin_extra_keys"] {
        let (mut prueba, _) = AppPrueba::con_semilla(40, 12);
        abrir(&mut prueba, caso_largo(nombre));
        if nombre == "sin_extra_keys" || nombre == "error_tras_copia" {
            prueba.instantanea(&format!("termux_teclas_{nombre}_40x12"));
        }
        paginas_con(&mut prueba, KeyCode::Down);
        prueba.instantanea(&format!("termux_teclas_{nombre}_40x12_abajo"));
    }
}

// ---------------------------------------------------------------- registro

/// Un tipo de REGISTRO de 25 caracteres, de los que ya existían antes de la
/// Fase 17 (`inventario_equipo_quitado`).
const TIPO_LARGO: &str = "inventario_equipo_quitado";
/// Uno de 22, el tope de la columna del tipo en el escritorio.
const TIPO_DE_22: &str = "llavero_frase_cambiada";

/// La vista Registro (F7) con una sola fila de ese tipo, con fecha fija. Con
/// `termux`, la App se porta como Termux.
fn registro_con(tipo: &str, termux: bool, cols: u16, filas: u16) -> AppPrueba {
    use crossterm::event::KeyCode;
    let mut prueba = AppPrueba::nueva(cols, filas);
    prueba.app.termux.activo = termux;
    let almacen = magi::almacen::abrir(&prueba._entorno.rutas.base_datos()).expect("almacén");
    almacen
        .conexion()
        .execute(
            "INSERT INTO REGISTRO (fecha, tipo, host_id, detalle, resultado)
             VALUES ('2026-09-15T15:41:02+00:00', ?1, NULL, 'detalle de la entrada', 'ok')",
            rusqlite::params![tipo],
        )
        .expect("registro");
    almacen.cerrar().expect("cerrar");
    prueba.tecla(KeyCode::F(7));
    prueba
}

/// La fila de la tabla (la primera con la fecha; el panel de detalle de debajo
/// repite el tipo entero).
fn fila_del_registro(prueba: &AppPrueba) -> String {
    let texto = prueba.texto();
    texto
        .lines()
        .find(|linea| linea.contains("15/09 15:41:02"))
        .unwrap_or_else(|| panic!("no está la fila del registro:\n{texto}"))
        .to_string()
}

/// La columna (en caracteres) donde empieza el host de la fila: la del tipo
/// más ancho, más las anteriores.
fn columna_del_host(prueba: &AppPrueba) -> usize {
    let fila = fila_del_registro(prueba);
    fila.chars()
        .position(|caracter| caracter == '—')
        .unwrap_or_else(|| panic!("no está el host de la fila: {fila}"))
}

/// K3: el tope de la columna del tipo (26) es solo de Termux. En el escritorio
/// sigue en 22: una sola fila con un tipo de 23 a 26 caracteres no ensancha la
/// tabla ni le quita ancho al detalle, y su tipo se corta con «…» como siempre.
#[test]
fn en_el_escritorio_la_columna_del_tipo_sigue_acotada_a_22() {
    let de_22 = registro_con(TIPO_DE_22, false, 100, 24);
    let largo = registro_con(TIPO_LARGO, false, 100, 24);
    assert_eq!(
        columna_del_host(&largo),
        columna_del_host(&de_22),
        "un tipo de 25 caracteres movió las columnas del escritorio:\n{}",
        largo.texto()
    );
    let fila = fila_del_registro(&largo);
    assert!(!fila.contains(TIPO_LARGO), "{fila}");
    assert!(
        fila.contains("inventario_equipo_qui…"),
        "el tipo debería cortarse a 22 con «…»:\n{fila}"
    );
}

/// K3: en Termux el tipo de 25 caracteres se ve entero (la columna llega a 26).
#[test]
fn en_termux_la_columna_del_tipo_llega_a_26() {
    let de_22 = registro_con(TIPO_DE_22, true, 100, 24);
    let largo = registro_con(TIPO_LARGO, true, 100, 24);
    // Con 22 la columna es de 22 también en Termux; con 25, tres más.
    assert_eq!(columna_del_host(&largo), columna_del_host(&de_22) + 3);
    let fila = fila_del_registro(&largo);
    assert!(fila.contains(TIPO_LARGO), "{fila}");
    // Y a 26 se queda: uno de 29 caracteres se corta ahí.
    let enorme = registro_con("sesion_persistente_enganchada", true, 100, 24);
    let fila = fila_del_registro(&enorme);
    assert!(
        fila.contains("sesion_persistente_enganc…"),
        "el tipo debería cortarse a 26 con «…»:\n{fila}"
    );
    assert_eq!(columna_del_host(&enorme), columna_del_host(&largo) + 1);
}

/// K3: instantánea del escritorio con un tipo largo (la tabla es la de
/// siempre) y de Termux (el tipo entero).
#[test]
fn instantaneas_del_registro_con_un_tipo_largo() {
    let prueba = registro_con(TIPO_LARGO, false, 80, 24);
    prueba.instantanea("registro_tipo_largo_escritorio_80x24");
    let prueba = registro_con(TIPO_LARGO, true, 80, 24);
    prueba.instantanea("registro_tipo_largo_termux_80x24");
}
