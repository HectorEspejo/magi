//! Sincronizaciones guardadas (`L`, Fase 8) y la cola con la fase `borrando`.
//! Instantáneas a 40×12, 80×24 y 200×60 (T45).
//!
//! La lista se abre como lo haría el usuario: Archivos con hetzner-01 (el de
//! `vistas_d`) y `L`. Las guardadas se crean en el almacén de la semilla con
//! las de la maqueta §6.5; su última ejecución es «hoy», «ayer» y una de 2025,
//! que se pinta «2025» mientras la prueba corra en otro año.

use crossterm::event::{KeyCode, KeyModifiers};

use magi::almacen::Almacen;
use magi::app::guardadas::{CampoGuardada, DialogoGuardadas, FormularioGuardada, ListaGuardadas};
use magi::app::Dialogo;
use magi::modelo::{DatosSincronizacion, ResultadoSincronizacion, Sincronizacion};
use magi::protocolo::Direccion;
use magi::tema::Tema;
use magi::ui::componentes::CampoTexto;
use magi::ui::disposicion::Lista;

use super::vistas_g::FabricaDialogo;
use crate::arnes::{self, AppPrueba};
use crate::semilla::Sembrado;

/// Los tres tamaños de referencia de la fase.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

/// Diálogos de esta vista para las pruebas comunes de `vistas_g` (enteros a
/// 40×12 y sin glifos Unicode en ASCII). La lista no va: su título empieza
/// por «MAGI ·», que en ASCII es «MAGI -», y esas pruebas buscan el recuadro
/// por el mismo texto en los dos temas; sus equivalentes están en este
/// fichero (`la_lista_se_ve_entera_y_en_ascii_sin_glifos`,
/// `la_lista_no_entra_en_panico_en_areas_diminutas`).
pub(crate) fn dialogos() -> Vec<(&'static str, FabricaDialogo)> {
    vec![(
        "SINCRONIZACIÓN GUARDADA",
        Box::new(|| {
            let mut formulario = FormularioGuardada::nueva(
                1,
                "hetzner-01".to_string(),
                "/home/hector/proyectos/cooperapp".to_string(),
                "/var/www/cooperapp".to_string(),
                Direccion::Subida,
                None,
            );
            formulario.nombre = CampoTexto::nuevo("web prod");
            formulario.exclusiones = CampoTexto::nuevo("*.log tmp/");
            formulario.foco = CampoGuardada::Exclusiones;
            formulario.error = Some(
                "el nombre solo admite letras, dígitos, punto, guion y guion bajo".to_string(),
            );
            Dialogo::Guardadas(DialogoGuardadas::Formulario(formulario))
        }),
    )]
}

// ---------------------------------------------------------------- estado

fn datos(
    host_id: i64,
    nombre: &str,
    local: &str,
    remota: &str,
    direccion: Direccion,
    borrar: bool,
    exclusiones: &[&str],
) -> DatosSincronizacion {
    DatosSincronizacion {
        host_id,
        nombre: nombre.to_string(),
        ruta_local: local.to_string(),
        ruta_remota: remota.to_string(),
        direccion,
        borrar,
        exclusiones: exclusiones
            .iter()
            .map(|patron| patron.to_string())
            .collect(),
    }
}

/// Última ejecución y resultado, como los dejarían el cliente al lanzar y el
/// servidor al terminar (SQL a mano solo para fijar la fecha).
fn fijar_ultima(almacen: &Almacen, id: i64, fecha: &str, resultado: ResultadoSincronizacion) {
    almacen
        .conexion()
        .execute(
            "UPDATE SINCRONIZACIONES_DIR SET ultima_ejecucion_en = ?1, ultimo_resultado = ?2 \
             WHERE id = ?3",
            rusqlite::params![fecha, resultado.como_texto(), id],
        )
        .unwrap();
}

/// Las tres guardadas de la maqueta en hetzner-01 (web-prod, logs y
/// estaticos, por id) y una de hetzner-02 que no debe salir en su lista.
fn sembrar_guardadas(prueba: &AppPrueba, sembrado: &Sembrado) -> Vec<i64> {
    let almacen = &prueba.app.almacen;
    let hetzner = sembrado.host("hetzner-01");
    let ids = vec![
        almacen
            .crear_sincronizacion(&datos(
                hetzner,
                "web-prod",
                "~/proyectos/cooperapp",
                "/var/www/cooperapp",
                Direccion::Subida,
                false,
                &["*.log"],
            ))
            .unwrap(),
        almacen
            .crear_sincronizacion(&datos(
                hetzner,
                "logs",
                "~/logs/hetz01",
                "/var/log/nginx",
                Direccion::Bajada,
                false,
                &[],
            ))
            .unwrap(),
        almacen
            .crear_sincronizacion(&datos(
                hetzner,
                "estaticos",
                "~/web/static",
                "/srv/static",
                Direccion::Subida,
                true,
                &[],
            ))
            .unwrap(),
    ];
    almacen
        .crear_sincronizacion(&datos(
            sembrado.host("hetzner-02"),
            "copias",
            "~/copias",
            "/srv/copias",
            Direccion::Bajada,
            false,
            &[],
        ))
        .unwrap();
    let ahora = chrono::Utc::now();
    fijar_ultima(
        almacen,
        ids[0],
        &(ahora - chrono::Duration::days(1)).to_rfc3339(),
        ResultadoSincronizacion::Ok,
    );
    fijar_ultima(
        almacen,
        ids[1],
        &ahora.to_rfc3339(),
        ResultadoSincronizacion::Ok,
    );
    fijar_ultima(
        almacen,
        ids[2],
        "2025-09-02T10:00:00+00:00",
        ResultadoSincronizacion::Error,
    );
    ids
}

/// `L` en Archivos con hetzner-01.
fn abrir_lista(prueba: &mut AppPrueba, sembrado: &Sembrado) {
    super::vistas_d::abrir_archivos(prueba, sembrado);
    prueba.tecla(KeyCode::Char('L'));
    assert!(
        matches!(
            prueba.app.dialogo,
            Some(Dialogo::Guardadas(DialogoGuardadas::Lista(_)))
        ),
        "L no abrió la lista:\n{}",
        prueba.texto()
    );
}

/// App con las guardadas sembradas y la lista abierta a `cols`×`filas`. Por
/// debajo del mínimo de Archivos (50×14) la vista no recibe teclas: la lista
/// se abre a 80×24 y la terminal encoge después.
fn con_lista_y_tema(cols: u16, filas: u16, tema: Tema) -> (AppPrueba, Sembrado, Vec<i64>) {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(cols.max(80), filas.max(24), tema);
    let ids = sembrar_guardadas(&prueba, &sembrado);
    abrir_lista(&mut prueba, &sembrado);
    if (cols, filas) != prueba.tamano_pintado() {
        prueba.pasar_por(cols, filas);
    }
    (prueba, sembrado, ids)
}

fn con_lista(cols: u16, filas: u16) -> (AppPrueba, Sembrado, Vec<i64>) {
    con_lista_y_tema(cols, filas, Tema::respaldo())
}

fn lista(prueba: &AppPrueba) -> &ListaGuardadas {
    match &prueba.app.dialogo {
        Some(Dialogo::Guardadas(DialogoGuardadas::Lista(lista))) => lista,
        _ => panic!("no está la lista abierta:\n{}", prueba.texto()),
    }
}

fn formulario(prueba: &AppPrueba) -> &FormularioGuardada {
    match &prueba.app.dialogo {
        Some(Dialogo::Guardadas(DialogoGuardadas::Formulario(formulario))) => formulario,
        _ => panic!("no está el formulario abierto:\n{}", prueba.texto()),
    }
}

fn escribir(prueba: &mut AppPrueba, texto: &str) {
    for caracter in texto.chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
}

fn pulsar(prueba: &mut AppPrueba, codigo: KeyCode, veces: usize) {
    for _ in 0..veces {
        prueba.tecla(codigo);
    }
}

fn guardadas_de(prueba: &AppPrueba, sembrado: &Sembrado, host: &str) -> Vec<Sincronizacion> {
    prueba
        .app
        .almacen
        .sincronizaciones_de_host(sembrado.host(host))
        .unwrap()
}

fn mensaje(prueba: &AppPrueba) -> String {
    prueba
        .app
        .mensaje
        .as_ref()
        .map(|mensaje| mensaje.texto.clone())
        .unwrap_or_default()
}

/// Posición del recuadro de la lista: la fila del título (la primera que
/// dice SINCRONIZACIONES), sus esquinas superiores y, si se ve, la fila del
/// borde de abajo. En ASCII las esquinas y la divisoria son `+`: el recuadro
/// sigue mientras las dos columnas de borde sean `|` o `+`.
fn recuadro_lista(texto: &str, ascii: bool) -> (usize, usize, usize, Option<usize>) {
    let lineas: Vec<Vec<char>> = texto.lines().map(|linea| linea.chars().collect()).collect();
    let (y0, titulo) = lineas
        .iter()
        .enumerate()
        .find_map(|(y, linea)| {
            let texto: String = linea.iter().collect();
            texto
                .find("SINCRONIZACIONES")
                .map(|byte| (y, texto[..byte].chars().count()))
        })
        .unwrap_or_else(|| panic!("sin recuadro de la lista:\n{texto}"));
    let (esquina_izquierda, esquina_derecha) = if ascii { ('+', '+') } else { ('┌', '┐') };
    let x0 = (0..titulo)
        .rev()
        .find(|x| lineas[y0][*x] == esquina_izquierda)
        .unwrap_or_else(|| panic!("lista sin esquina izquierda:\n{texto}"));
    let x1 = (titulo..lineas[y0].len())
        .find(|x| lineas[y0][*x] == esquina_derecha)
        .unwrap_or_else(|| panic!("lista sin esquina derecha:\n{texto}"));
    let y1 = if ascii {
        let borde = |y: usize, x: usize| matches!(lineas[y].get(x), Some('|') | Some('+'));
        (y0 + 1..lineas.len())
            .take_while(|y| borde(*y, x0) && borde(*y, x1))
            .last()
            .filter(|y| lineas[*y][x0] == '+')
    } else {
        (y0 + 1..lineas.len())
            .find(|y| lineas[*y].get(x0) == Some(&'└') && lineas[*y].get(x1) == Some(&'┘'))
    };
    (x0, y0, x1, y1)
}

/// Lo que hay dentro del recuadro de la lista, línea a línea.
fn dentro_de_la_lista(texto: &str, ascii: bool) -> Vec<String> {
    let (x0, y0, x1, y1) = recuadro_lista(texto, ascii);
    let lineas: Vec<Vec<char>> = texto.lines().map(|linea| linea.chars().collect()).collect();
    let y1 = y1.unwrap_or(lineas.len() - 1);
    lineas[y0..=y1]
        .iter()
        .map(|linea| linea.iter().skip(x0).take(x1 - x0 + 1).collect())
        .collect()
}

fn solo_ascii(texto: &str) -> bool {
    texto.chars().all(|c| c.is_ascii() || c.is_alphabetic())
}

// ---------------------------------------------------------------- instantáneas

#[test]
fn instantaneas_de_la_lista_a_los_tres_tamanos() {
    for (cols, filas) in TAMANOS {
        let (prueba, _, _) = con_lista(cols, filas);
        prueba.instantanea(&format!("guardadas_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_del_formulario_a_los_tres_tamanos() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _, _) = con_lista(cols, filas);
        // web-prod (la última por nombre), en edición.
        prueba.tecla(KeyCode::End);
        prueba.tecla(KeyCode::Char('e'));
        assert!(formulario(&prueba).id.is_some());
        prueba.instantanea(&format!("guardada_formulario_{cols}x{filas}"));
    }
}

// ---------------------------------------------------------------- lista

/// `L` lista las del host de Archivos con nombre, dirección (`−` si borra),
/// rutas, último resultado con fecha y el detalle de la seleccionada.
#[test]
fn la_lista_trae_las_guardadas_del_host_con_su_formato() {
    let (mut prueba, _, ids) = con_lista(200, 60);
    let abierta = lista(&prueba);
    assert_eq!(abierta.filas.len(), 3, "solo las de hetzner-01");
    let nombres: Vec<&str> = abierta.filas.iter().map(|g| g.nombre.as_str()).collect();
    assert_eq!(nombres, vec!["estaticos", "logs", "web-prod"]);
    let dentro = dentro_de_la_lista(&prueba.texto(), false);
    let todo = dentro.join("\n");
    assert!(
        dentro[0].contains("MAGI · SINCRONIZACIONES · hetzner-01") && dentro[0].contains(" 3 "),
        "{todo}"
    );
    for columna in ["NOMBRE", "DIR.", "LOCAL → REMOTO", "ÚLTIMA"] {
        assert!(dentro[1].contains(columna), "falta {columna}:\n{todo}");
    }
    let fila = |nombre: &str| {
        dentro
            .iter()
            .find(|linea| linea.contains(&format!(" {nombre} ")))
            .unwrap_or_else(|| panic!("sin fila {nombre}:\n{todo}"))
            .clone()
    };
    let estaticos = fila("estaticos");
    assert!(estaticos.contains("▸ estaticos"), "seleccionada:\n{todo}");
    assert!(estaticos.contains("↑ −"), "{estaticos}");
    assert!(
        estaticos.contains("~/web/static → /srv/static"),
        "{estaticos}"
    );
    assert!(estaticos.contains("✕ 2025"), "{estaticos}");
    let logs = fila("logs");
    assert!(logs.contains("↓ "), "{logs}");
    assert!(!logs.contains("−"), "logs no borra: {logs}");
    assert!(logs.contains("~/logs/hetz01 ← /var/log/nginx"), "{logs}");
    assert!(logs.contains("✓ hoy"), "{logs}");
    let web = fila("web-prod");
    assert!(
        web.contains("~/proyectos/cooperapp → /var/www/cooperapp"),
        "{web}"
    );
    assert!(web.contains("✓ ayer"), "{web}");
    assert!(
        !todo.contains("copias"),
        "la de hetzner-02 no sale:\n{todo}"
    );
    assert!(
        todo.contains("estaticos · subida · borra en destino lo que sobra"),
        "{todo}"
    );
    for atajo in ["↵ ejecutar", "n nueva", "e editar", "x borrar", "q volver"] {
        assert!(todo.contains(atajo), "falta {atajo}:\n{todo}");
    }
    // El detalle sigue a la selección.
    pulsar(&mut prueba, KeyCode::Down, 2);
    assert_eq!(lista(&prueba).seleccionada().map(|g| g.id), Some(ids[0]));
    let todo = dentro_de_la_lista(&prueba.texto(), false).join("\n");
    assert!(
        todo.contains("web-prod · subida · sin borrar · excluye *.log"),
        "{todo}"
    );
}

/// En modo estrecho cae la columna de rutas; nombre, dirección y última se
/// quedan, y el título pierde el «MAGI ·» si no cabe.
#[test]
fn en_estrecho_cae_la_columna_de_rutas() {
    for (cols, filas) in [(40, 12), (60, 20)] {
        let (prueba, _, _) = con_lista(cols, filas);
        let dentro = dentro_de_la_lista(&prueba.texto(), false);
        let todo = dentro.join("\n");
        assert!(!todo.contains("LOCAL"), "a {cols}: {todo}");
        assert!(!todo.contains("/srv/static"), "a {cols}: {todo}");
        assert!(todo.contains("estaticos"), "a {cols}: {todo}");
        assert!(todo.contains("↑ −"), "a {cols}: {todo}");
        assert!(todo.contains("✕ 2025"), "a {cols}: {todo}");
        assert!(
            dentro[0].contains("SINCRONIZACIONES · hetzner-01"),
            "a {cols}: {todo}"
        );
    }
    let (prueba, _, _) = con_lista(70, 20);
    let todo = dentro_de_la_lista(&prueba.texto(), false).join("\n");
    assert!(
        todo.contains("LOCAL → REMOTO"),
        "a 70 no es estrecho:\n{todo}"
    );
}

#[test]
fn vacia_dice_como_crear_una() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    abrir_lista(&mut prueba, &sembrado);
    let todo = dentro_de_la_lista(&prueba.texto(), false).join("\n");
    assert!(
        todo.contains("sin sincronizaciones guardadas: n para crear una"),
        "{todo}"
    );
    assert!(todo.contains(" 0 "), "{todo}");
    assert!(!todo.contains("ejecutar"), "{todo}");
    // Sin filas, `↵`, `e` y `x` no hacen nada.
    for tecla in [KeyCode::Enter, KeyCode::Char('e'), KeyCode::Char('x')] {
        prueba.tecla(tecla);
        assert!(lista(&prueba).filas.is_empty());
    }
    prueba.tecla(KeyCode::Char('n'));
    assert!(formulario(&prueba).id.is_none());
}

#[test]
fn esc_y_q_cierran_la_lista() {
    for tecla in [KeyCode::Esc, KeyCode::Char('q')] {
        let (mut prueba, _, _) = con_lista(80, 24);
        prueba.tecla(tecla);
        assert!(prueba.app.dialogo.is_none(), "{tecla:?} no cerró");
        assert_eq!(prueba.app.vista, magi::ui::Vista::Archivos);
    }
}

/// `↵` cierra la lista y pide el plan de la fila tal como se ve (lo que
/// haga el plan es cosa de `sincronizar`).
#[test]
fn intro_cierra_la_lista_y_planifica_la_seleccionada() {
    let (mut prueba, _, _) = con_lista(80, 24);
    prueba.tecla(KeyCode::Enter);
    assert!(
        !matches!(prueba.app.dialogo, Some(Dialogo::Guardadas(_))),
        "la lista sigue abierta:\n{}",
        prueba.texto()
    );
}

/// Una fila con rutas y exclusiones enormes (la escribe otro proceso o una
/// mano en la base) se recorta: la lista no pasa del ancho de un diálogo ni
/// entra en pánico al medir las columnas.
#[test]
fn una_ruta_enorme_se_recorta() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 60);
    let enorme = format!("/{}", "a".repeat(70_000));
    let patrones = ["*.log"; 20_000];
    prueba
        .app
        .almacen
        .crear_sincronizacion(&datos(
            sembrado.host("hetzner-01"),
            "enorme",
            &enorme,
            &enorme,
            Direccion::Subida,
            true,
            &patrones,
        ))
        .unwrap();
    abrir_lista(&mut prueba, &sembrado);
    for (cols, filas) in [(200, 60), (80, 24), (40, 12)] {
        prueba.pasar_por(cols, filas);
        let (x0, _, x1, y1) = recuadro_lista(&prueba.texto(), false);
        assert!(
            y1.is_some(),
            "cortada a {cols}×{filas}:\n{}",
            prueba.texto()
        );
        assert!(
            x1 - x0 < usize::from(magi::ui::disposicion::ANCHO_MAX_DIALOGO),
            "demasiado ancha a {cols}×{filas}"
        );
    }
}

/// Una lista larga se desplaza con la selección (`↑` `↓` `PgUp` `PgDn`
/// `Inicio` `Fin`) y registra su ventana sin huecos al final.
#[test]
fn la_lista_se_desplaza_con_la_seleccion() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    let hetzner = sembrado.host("hetzner-01");
    for numero in 0..20 {
        prueba
            .app
            .almacen
            .crear_sincronizacion(&datos(
                hetzner,
                &format!("sync-{numero:02}"),
                "~/a",
                "/b",
                Direccion::Subida,
                false,
                &[],
            ))
            .unwrap();
    }
    abrir_lista(&mut prueba, &sembrado);
    prueba.pasar_por(40, 12);
    let ventana = || {
        prueba
            .app
            .disposicion()
            .lista(Lista::Modal)
            .expect("la lista registra su ventana")
    };
    let inicial = ventana();
    assert_eq!((inicial.inicio, inicial.total), (0, 20));
    assert!(inicial.filas >= 1 && inicial.filas < 20, "{inicial:?}");
    prueba.tecla(KeyCode::End);
    assert_eq!(lista(&prueba).seleccion, 19);
    let abajo = prueba.app.disposicion().lista(Lista::Modal).unwrap();
    assert_eq!(abajo.inicio, 20 - abajo.filas, "sin hueco al final");
    assert!(prueba.texto().contains("sync-19"), "{}", prueba.texto());
    prueba.comprobar_listas();
    prueba.tecla(KeyCode::PageUp);
    assert_eq!(lista(&prueba).seleccion, 19 - abajo.filas);
    prueba.tecla(KeyCode::Home);
    assert_eq!(lista(&prueba).seleccion, 0);
    assert!(prueba.texto().contains("sync-00"), "{}", prueba.texto());
    prueba.tecla(KeyCode::PageDown);
    assert_eq!(lista(&prueba).seleccion, abajo.filas);
    prueba.tecla(KeyCode::Up);
    assert_eq!(lista(&prueba).seleccion, abajo.filas - 1);
    // El indicador de desplazamiento va en el borde de abajo.
    assert!(prueba.texto().contains("↑↓ "), "{}", prueba.texto());
    prueba.comprobar_listas();
}

/// Entera (sin cortar) a 40×12 y, con el tema ASCII, sin glifos Unicode a
/// ningún tamaño: las pruebas comunes de `vistas_g` para la lista.
#[test]
fn la_lista_se_ve_entera_y_en_ascii_sin_glifos() {
    let (prueba, _, _) = con_lista(40, 12);
    let texto = prueba.texto();
    let (_, _, _, y1) = recuadro_lista(&texto, false);
    assert!(y1.is_some(), "lista cortada a 40×12:\n{texto}");

    let (mut prueba, _, _) = con_lista_y_tema(80, 24, arnes::tema_ascii());
    // Con borrado y desplazamiento a la vista.
    for (cols, filas) in [(80, 24), (40, 12), (70, 20), (60, 20), (30, 10), (200, 60)] {
        prueba.pasar_por(cols, filas);
        let texto = prueba.texto();
        let dentro = dentro_de_la_lista(&texto, true).join("\n");
        assert!(
            solo_ascii(&dentro),
            "glifo Unicode en la lista a {cols}×{filas}:\n{dentro}"
        );
        assert!(dentro.contains("^ -"), "a {cols}×{filas}:\n{dentro}");
    }
    prueba.pasar_por(200, 60);
    let dentro = dentro_de_la_lista(&prueba.texto(), true).join("\n");
    assert!(
        dentro.contains("~/logs/hetz01 <- /var/log/nginx"),
        "{dentro}"
    );
    assert!(
        dentro.contains("MAGI - SINCRONIZACIONES - hetzner-01"),
        "{dentro}"
    );
    assert!(dentro.contains("enter ejecutar"), "{dentro}");
}

/// Ni la lista ni su formulario entran en pánico en áreas muy pequeñas, con
/// las teclas de desplazamiento y de foco.
#[test]
fn la_lista_no_entra_en_panico_en_areas_diminutas() {
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
    for diminuto in 1..=3 {
        tamanos.extend([
            (diminuto, 12),
            (diminuto, 60),
            (40, diminuto),
            (200, diminuto),
        ]);
    }
    for formulario in [false, true] {
        let (mut prueba, _, _) = con_lista(80, 24);
        if formulario {
            prueba.tecla(KeyCode::Char('e'));
        }
        for &(cols, filas) in &tamanos {
            prueba.pasar_por(cols, filas);
            assert!(
                matches!(prueba.app.dialogo, Some(Dialogo::Guardadas(_))),
                "diálogo perdido a {cols}×{filas}"
            );
            prueba.tecla(KeyCode::PageDown);
            prueba.tecla(KeyCode::Up);
            prueba.tecla(if formulario {
                KeyCode::Tab
            } else {
                KeyCode::Down
            });
        }
    }
}

// ---------------------------------------------------------------- alta, edición y borrado

/// `n` abre el formulario con las rutas de los paneles y la dirección del
/// activo; el almacén valida (nombre con espacio, repetido) y el error se ve
/// en el formulario; al guardar se vuelve a la lista con la nueva elegida.
#[test]
fn n_crea_con_validacion() {
    let (mut prueba, sembrado, _) = con_lista(80, 24);
    prueba.tecla(KeyCode::Char('n'));
    let hogar = prueba.app.rutas.hogar.display().to_string();
    {
        let formulario = formulario(&prueba);
        assert_eq!(formulario.id, None);
        assert_eq!(formulario.ruta_local.texto, hogar);
        assert_eq!(formulario.ruta_remota.texto, "/var/www/cooperapp");
        assert_eq!(formulario.direccion, Direccion::Subida);
        assert!(!formulario.borrar);
    }
    assert!(prueba.texto().contains("SINCRONIZACIÓN GUARDADA · nueva"));

    escribir(&mut prueba, "web prod");
    prueba.tecla_con(KeyCode::Char('s'), KeyModifiers::CONTROL);
    let error = formulario(&prueba)
        .error
        .clone()
        .expect("error de validación");
    assert!(error.contains("el nombre solo admite"), "{error}");
    assert!(
        prueba.texto().contains("el nombre solo admite"),
        "el error se ve:\n{}",
        prueba.texto()
    );
    assert_eq!(guardadas_de(&prueba, &sembrado, "hetzner-01").len(), 3);

    // Repetido en el mismo host: lo rechaza la unicidad.
    pulsar(&mut prueba, KeyCode::Backspace, "web prod".len());
    escribir(&mut prueba, "web-prod");
    prueba.tecla(KeyCode::Enter);
    let error = formulario(&prueba).error.clone().expect("nombre repetido");
    assert!(error.contains("ya existe"), "{error}");

    pulsar(&mut prueba, KeyCode::Backspace, "web-prod".len());
    escribir(&mut prueba, "despliegue");
    // Remoto en bajada, con borrado y exclusiones.
    pulsar(&mut prueba, KeyCode::Tab, 3);
    assert_eq!(formulario(&prueba).foco, CampoGuardada::Direccion);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Tab);
    escribir(&mut prueba, "*.log tmp/");
    prueba.tecla(KeyCode::Enter);

    let abierta = lista(&prueba);
    assert_eq!(abierta.filas.len(), 4);
    let elegida = abierta.seleccionada().expect("selección").clone();
    assert_eq!(elegida.nombre, "despliegue");
    assert!(mensaje(&prueba).contains("«despliegue» guardada"));
    let guardada = guardadas_de(&prueba, &sembrado, "hetzner-01")
        .into_iter()
        .find(|guardada| guardada.nombre == "despliegue")
        .expect("en la base");
    assert_eq!(guardada.id, elegida.id);
    assert_eq!(guardada.ruta_local, hogar);
    assert_eq!(guardada.ruta_remota, "/var/www/cooperapp");
    assert_eq!(guardada.direccion, Direccion::Bajada);
    assert!(guardada.borrar);
    assert_eq!(guardada.exclusiones, vec!["*.log", "tmp/"]);
    assert_eq!(guardada.ultima_ejecucion_en, None);
}

/// `Esc` en el formulario vuelve a la lista sin guardar, con la fila que
/// estaba elegida.
#[test]
fn esc_en_el_formulario_vuelve_a_la_lista_sin_guardar() {
    let (mut prueba, sembrado, ids) = con_lista(80, 24);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('n'));
    escribir(&mut prueba, "otra");
    prueba.tecla(KeyCode::Esc);
    assert_eq!(lista(&prueba).seleccionada().map(|g| g.id), Some(ids[1]));
    assert_eq!(guardadas_de(&prueba, &sembrado, "hetzner-01").len(), 3);
}

/// `e` edita la fila elegida (id fijado al abrir): dirección y borrado
/// cambian y el resultado de la última ejecución se conserva.
#[test]
fn e_edita_la_fila_elegida() {
    let (mut prueba, sembrado, ids) = con_lista(80, 24);
    prueba.tecla(KeyCode::End);
    prueba.tecla(KeyCode::Char('e'));
    {
        let formulario = formulario(&prueba);
        assert_eq!(formulario.id, Some(ids[0]));
        assert_eq!(formulario.nombre.texto, "web-prod");
        assert_eq!(formulario.ruta_local.texto, "~/proyectos/cooperapp");
        assert_eq!(formulario.ruta_remota.texto, "/var/www/cooperapp");
        assert_eq!(formulario.exclusiones.texto, "*.log");
        assert_eq!(formulario.direccion, Direccion::Subida);
    }
    assert!(prueba.texto().contains("SINCRONIZACIÓN GUARDADA · editar"));
    pulsar(&mut prueba, KeyCode::Tab, 3);
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla_con(KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert_eq!(lista(&prueba).seleccionada().map(|g| g.id), Some(ids[0]));
    let guardada = prueba.app.almacen.obtener_sincronizacion(ids[0]).unwrap();
    assert_eq!(guardada.nombre, "web-prod");
    assert_eq!(guardada.direccion, Direccion::Bajada);
    assert!(guardada.borrar);
    assert_eq!(guardada.exclusiones, vec!["*.log"]);
    assert_eq!(guardada.ultimo_resultado, Some(ResultadoSincronizacion::Ok));
    assert_eq!(guardadas_de(&prueba, &sembrado, "hetzner-01").len(), 3);
    let todo = dentro_de_la_lista(&prueba.texto(), false).join("\n");
    assert!(
        todo.contains("web-prod · bajada · borra en destino lo que sobra"),
        "{todo}"
    );
}

/// `x` pide confirmación con peligro; cancelar vuelve a la lista intacta y
/// confirmar borra y vuelve a la lista releída (sin dejar la vieja apartada).
#[test]
fn x_pide_confirmacion_y_borra() {
    let (mut prueba, sembrado, ids) = con_lista(80, 24);
    prueba.tecla(KeyCode::Char('x'));
    match &prueba.app.dialogo {
        Some(Dialogo::Confirmar {
            titulo,
            lineas,
            peligro,
            ..
        }) => {
            assert_eq!(titulo, "BORRAR SINCRONIZACIÓN");
            assert!(*peligro);
            assert!(lineas[0].contains("«estaticos»"), "{lineas:?}");
        }
        _ => panic!("sin confirmación:\n{}", prueba.texto()),
    }
    // Cancelar: la lista vuelve tal cual.
    prueba.tecla(KeyCode::Esc);
    assert_eq!(lista(&prueba).filas.len(), 3);
    assert_eq!(lista(&prueba).seleccionada().map(|g| g.id), Some(ids[2]));
    assert_eq!(guardadas_de(&prueba, &sembrado, "hetzner-01").len(), 3);

    prueba.tecla(KeyCode::Char('x'));
    prueba.tecla(KeyCode::Char('s'));
    let lista_nueva = lista(&prueba);
    let nombres: Vec<&str> = lista_nueva
        .filas
        .iter()
        .map(|g| g.nombre.as_str())
        .collect();
    assert_eq!(nombres, vec!["logs", "web-prod"]);
    assert_eq!(lista_nueva.seleccion, 0);
    assert!(mensaje(&prueba).contains("«estaticos» borrada"));
    assert!(prueba.app.almacen.obtener_sincronizacion(ids[2]).is_err());
    // La de otro host sigue.
    assert_eq!(guardadas_de(&prueba, &sembrado, "hetzner-02").len(), 1);
    // Al cerrar no vuelve ninguna lista vieja de la pila.
    prueba.tecla(KeyCode::Char('q'));
    assert!(prueba.app.dialogo.is_none(), "{}", prueba.texto());
}

// ---------------------------------------------------------------- CLI

/// `magi sincronizaciones` con el binario real y un entorno aislado (sin
/// servidor): vacía dice cómo crearlas y, con guardadas, las lista con host,
/// dirección, borrado, último resultado y rutas.
#[test]
fn magi_sincronizaciones_lista_las_guardadas() {
    let temporal = tempfile::tempdir().unwrap();
    let raiz = temporal.path();
    let rutas = magi::config::Rutas {
        datos: raiz.join("datos").join("magi"),
        config: raiz.join("config").join("magi"),
        estado: raiz.join("estado").join("magi"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime").join("magi"),
    };
    std::fs::create_dir_all(&rutas.hogar).unwrap();
    let ejecutar = || {
        let salida = std::process::Command::new(env!("CARGO_BIN_EXE_magi"))
            .arg("sincronizaciones")
            .env("HOME", raiz.join("hogar"))
            .env("XDG_DATA_HOME", raiz.join("datos"))
            .env("XDG_CONFIG_HOME", raiz.join("config"))
            .env("XDG_STATE_HOME", raiz.join("estado"))
            .env("XDG_RUNTIME_DIR", raiz.join("runtime"))
            .stdin(std::process::Stdio::null())
            .output()
            .expect("lanzando magi sincronizaciones");
        assert!(salida.status.success(), "salida: {salida:?}");
        String::from_utf8(salida.stdout).unwrap()
    };
    assert_eq!(
        ejecutar(),
        "Sin sincronizaciones: créalas en la TUI con S o L.\n"
    );

    let sembrado = crate::semilla::sembrar(&rutas);
    let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
    let id = almacen
        .crear_sincronizacion(&datos(
            sembrado.host("hetzner-01"),
            "web-prod",
            "/home/hector/proyectos/cooperapp",
            "/var/www/cooperapp",
            Direccion::Subida,
            true,
            &["*.log"],
        ))
        .unwrap();
    fijar_ultima(
        &almacen,
        id,
        "2026-10-02T14:31:05+02:00",
        ResultadoSincronizacion::Parcial,
    );
    almacen.cerrar().unwrap();
    let texto = ejecutar();
    let lineas: Vec<&str> = texto.lines().collect();
    assert_eq!(lineas.len(), 2, "{texto}");
    assert!(lineas[0].starts_with("HOST"), "{texto}");
    for parte in [
        "hetzner-01",
        " web-prod ",
        " subida ",
        " sí ",
        " parcial · 2026-10-02 14:31 ",
        "/home/hector/proyectos/cooperapp → /var/www/cooperapp",
    ] {
        assert!(lineas[1].contains(parte), "falta «{parte}»: {texto}");
    }
}

// ---------------------------------------------------------------- paleta

fn abrir_paleta(prueba: &mut AppPrueba) {
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(prueba.app.paleta.is_some(), "no se abrió la paleta");
}

/// Las filas de entradas de la paleta pintada (sin título, consulta ni
/// borde de abajo).
fn entradas_paleta(texto: &str) -> Vec<String> {
    let lineas: Vec<&str> = texto.lines().collect();
    let y0 = lineas
        .iter()
        .position(|linea| linea.contains("┌ PALETA"))
        .unwrap_or_else(|| panic!("sin paleta:\n{texto}"));
    let x0 = lineas[y0].chars().position(|c| c == '┌').unwrap();
    let x1 = lineas[y0]
        .chars()
        .enumerate()
        .skip(x0)
        .find(|(_, c)| *c == '┐')
        .map(|(x, _)| x)
        .unwrap();
    let y1 = (y0 + 1..lineas.len())
        .find(|y| lineas[*y].chars().nth(x0) == Some('└'))
        .unwrap_or(lineas.len());
    lineas[y0 + 2..y1]
        .iter()
        .map(|linea| {
            linea
                .chars()
                .skip(x0)
                .take(x1 - x0 + 1)
                .collect::<String>()
                .trim_matches(['│', ' ', '▸'])
                .to_string()
        })
        .filter(|fila| !fila.is_empty())
        .collect()
}

/// `sync · <host> · <nombre>` por guardada (de todos los hosts, releídas al
/// abrir), `sincronizar directorio` y `editar fichero`, en «archivos» y con
/// la categoría en su columna.
#[test]
fn la_paleta_trae_las_guardadas_con_su_categoria() {
    for cols in [30, 40, 60, 80, 200] {
        let (mut prueba, sembrado) = AppPrueba::con_semilla(cols, 24);
        sembrar_guardadas(&prueba, &sembrado);
        abrir_paleta(&mut prueba);
        escribir(&mut prueba, "sync ·");
        let texto = prueba.texto();
        let entradas = entradas_paleta(&texto);
        assert!(entradas.len() >= 4, "a {cols}: pocas entradas:\n{texto}");
        if cols >= 60 {
            for etiqueta in [
                "sync · hetzner-01 · estaticos",
                "sync · hetzner-01 · logs",
                "sync · hetzner-01 · web-prod",
                "sync · hetzner-02 · copias",
            ] {
                assert!(
                    entradas.iter().any(|fila| fila.starts_with(etiqueta)),
                    "a {cols}: falta «{etiqueta}»:\n{texto}"
                );
            }
        }
        let con_categoria = entradas
            .iter()
            .filter(|fila| fila.ends_with("archivos"))
            .count();
        assert!(
            con_categoria == 0 || con_categoria == entradas.len(),
            "a {cols}: categoría solo en algunas filas:\n{texto}"
        );
        if cols >= 60 {
            assert_eq!(con_categoria, entradas.len(), "a {cols}:\n{texto}");
        }
        for fila in entradas.iter().filter(|fila| fila.ends_with("archivos")) {
            let antes = &fila[..fila.len() - "archivos".len()];
            assert!(
                antes.ends_with("  "),
                "a {cols}: categoría pegada a la etiqueta en «{fila}»:\n{texto}"
            );
        }
    }
}

/// Una guardada nueva sale en la paleta la próxima vez que se abre.
#[test]
fn la_paleta_relee_las_guardadas_al_abrir() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    abrir_paleta(&mut prueba);
    escribir(&mut prueba, "sync ·");
    assert!(!prueba.texto().contains("sync · hetzner-01"));
    prueba.tecla(KeyCode::Esc);
    prueba
        .app
        .almacen
        .crear_sincronizacion(&datos(
            sembrado.host("hetzner-01"),
            "nueva",
            "~/a",
            "/b",
            Direccion::Subida,
            false,
            &[],
        ))
        .unwrap();
    abrir_paleta(&mut prueba);
    escribir(&mut prueba, "sync ·");
    assert!(
        prueba.texto().contains("sync · hetzner-01 · nueva"),
        "{}",
        prueba.texto()
    );
}

/// `sincronizar directorio` y `editar fichero` trabajan sobre los paneles:
/// fuera de Archivos solo lo dicen.
#[test]
fn sincronizar_y_editar_desde_la_paleta_piden_archivos() {
    for consulta in ["sincronizar directorio", "editar fichero"] {
        let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
        abrir_paleta(&mut prueba);
        escribir(&mut prueba, consulta);
        let entradas = entradas_paleta(&prueba.texto());
        assert!(
            entradas[0].starts_with(consulta) && entradas[0].ends_with("archivos"),
            "{entradas:?}"
        );
        prueba.tecla(KeyCode::Enter);
        assert!(prueba.app.paleta.is_none());
        assert_eq!(mensaje(&prueba), "abre Archivos (F4) primero");
        assert!(prueba.app.dialogo.is_none());
    }
}
