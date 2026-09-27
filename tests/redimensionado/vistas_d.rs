//! Instantáneas y pruebas de disposición del grupo D: Archivos y Transferencias.
//!
//! Archivos se abre como lo haría el usuario: `F4` con un host seleccionado en
//! Flota, y el servidor contesta `SftpAbierto` y `DirListado` con el
//! `peticion_id` que la App envió. El panel local lista `rutas.hogar`, con
//! ficheros de tamaño y fecha fijos (`filetime`). Las fechas son de 2025, así
//! que el panel las pinta como «2025» mientras la prueba corra en otro año.

use std::fs;
use std::path::Path;

use crossterm::event::KeyCode;
use filetime::FileTime;

use magi::archivos::{Entrada, Lado, Marca, TipoEntrada};
use magi::protocolo::{
    Direccion, EstadoTransferencia, InfoTransferencia, MensajeCliente, MensajeServidor,
};
use magi::tema::Tema;
use magi::ui::disposicion::Lista;
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};
use crate::semilla::Sembrado;

/// 12 sep 2025 09:41:07 UTC.
const FECHA: i64 = 1_757_670_067;
const DIA: i64 = 86_400;
const RUTA_REMOTA: &str = "/var/www/cooperapp";

// ---------------------------------------------------------------- estado

/// Nombre, tamaño (None = directorio) y mtime del panel local.
const LOCALES: [(&str, Option<u64>, i64); 7] = [
    ("app", None, FECHA),
    ("static", None, FECHA - 9 * DIA),
    ("main.py", Some(14_336), FECHA + 3 * DIA),
    ("config.yaml", Some(2_048), FECHA),
    ("README.md", Some(3_000), FECHA),
    ("notas.md", Some(512), FECHA),
    (".env", Some(1_024), FECHA),
];

/// Ficheros del hogar con tamaños y fechas fijos. Los directorios se fechan
/// al final: crear algo dentro les cambiaría la fecha.
fn preparar_hogar(hogar: &Path) {
    fs::create_dir_all(hogar).unwrap();
    for (nombre, tamano, _) in LOCALES {
        let ruta = hogar.join(nombre);
        match tamano {
            None => fs::create_dir_all(&ruta).unwrap(),
            Some(bytes) => fs::write(&ruta, vec![b'x'; bytes as usize]).unwrap(),
        }
    }
    for (nombre, _, mtime) in LOCALES {
        filetime::set_file_mtime(hogar.join(nombre), FileTime::from_unix_time(mtime, 0)).unwrap();
    }
}

fn entrada(nombre: &str, tamano: Option<u64>, mtime: i64) -> Entrada {
    Entrada {
        nombre: nombre.to_string(),
        tipo: if tamano.is_some() {
            TipoEntrada::Fichero
        } else {
            TipoEntrada::Directorio
        },
        tamano: tamano.unwrap_or(4_096),
        mtime,
        permisos: Some(0o644),
        propietario: Some("deploy".to_string()),
        enlace: None,
        marca: Marca::Ninguna,
    }
}

/// Listado de `/var/www/cooperapp`: `main.py` con otra fecha y `config.yaml`
/// con otro tamaño (≠), `README.md` igual y dos que solo están en remoto (✕).
fn entradas_remotas() -> Vec<Entrada> {
    vec![
        entrada("app", None, FECHA),
        entrada("static", None, FECHA - 9 * DIA),
        entrada("main.py", Some(14_336), FECHA),
        entrada("config.yaml", Some(1_024), FECHA),
        entrada("README.md", Some(3_000), FECHA),
        entrada("requirements.txt", Some(300), FECHA),
        entrada("backup.tar.gz", Some(1_153_434), FECHA - 30 * DIA),
    ]
}

/// Listado remoto largo, para que la lista tenga que desplazarse.
fn entradas_remotas_largas() -> Vec<Entrada> {
    let mut entradas = entradas_remotas();
    for indice in 0..30 {
        entradas.push(entrada(
            &format!("registro-{indice:02}.log"),
            Some(2_048 + indice * 100),
            FECHA,
        ));
    }
    entradas
}

/// Abre Archivos con hetzner-01 (lo que hace `F4` con él seleccionado; la
/// selección inicial de Flota no es cosa de estas pruebas) y contesta por el
/// servidor con `entradas` como listado remoto.
fn abrir_archivos_con(prueba: &mut AppPrueba, sembrado: &Sembrado, entradas: Vec<Entrada>) {
    let total = entradas.len();
    preparar_hogar(&prueba.app.rutas.hogar.clone());
    let host_id = sembrado.host("hetzner-01");
    prueba.app.abrir_archivos(Some(host_id));
    assert_eq!(prueba.app.vista, Vista::Archivos);
    assert!(
        prueba
            .enviados()
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeCliente::AbrirSftp { .. })),
        "F4 debe pedir el canal SFTP"
    );
    prueba.servidor(MensajeServidor::SftpAbierto {
        host_id,
        dir_inicio: RUTA_REMOTA.to_string(),
        peticion_id: None,
    });
    let peticion_id = prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ListarDir { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("la App pide el listado remoto");
    prueba.servidor(MensajeServidor::DirListado {
        host_id,
        ruta: RUTA_REMOTA.to_string(),
        entradas,
        peticion_id,
    });
    let archivos = prueba.app.archivos.as_ref().unwrap();
    assert_eq!(archivos.remoto.ruta, RUTA_REMOTA);
    assert_eq!(
        archivos.remoto.total_visibles(),
        total + 1,
        "entradas y «..»"
    );
}

fn abrir_archivos(prueba: &mut AppPrueba, sembrado: &Sembrado) {
    abrir_archivos_con(prueba, sembrado, entradas_remotas());
}

fn con_archivos(cols: u16, filas: u16) -> AppPrueba {
    con_archivos_y_tema(cols, filas, Tema::respaldo())
}

fn con_archivos_y_tema(cols: u16, filas: u16, tema: Tema) -> AppPrueba {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(cols, filas, tema);
    abrir_archivos(&mut prueba, &sembrado);
    prueba
}

fn transferencia(
    id: u32,
    estado: EstadoTransferencia,
    direccion: Direccion,
    origen: &str,
    destino: &str,
    bytes: (u64, u64),
) -> InfoTransferencia {
    let (hechos, total) = bytes;
    InfoTransferencia {
        id,
        host_id: 1,
        host_nombre: "hetzner-01".to_string(),
        direccion,
        estado,
        origen: origen.to_string(),
        destino: destino.to_string(),
        es_directorio: false,
        ficheros_total: 1,
        ficheros_hechos: u32::from(estado == EstadoTransferencia::Hecha),
        omitidos: 0,
        bytes_total: total,
        bytes_hechos: hechos,
        fichero_actual: None,
        error: None,
        borrar_origen: false,
        // Ningún cliente de la prueba: la App no refresca paneles por ella.
        solicitante: 99,
        creada_en: FECHA,
        terminada_en: estado.terminada().then_some(FECHA),
    }
}

/// La cola de la maqueta de F4: una en curso (78 %), dos en cola, una hecha y
/// una con error.
fn cola() -> Vec<InfoTransferencia> {
    let mut en_curso = transferencia(
        1,
        EstadoTransferencia::EnCurso,
        Direccion::Subida,
        "/home/hector/cooperapp/main.py",
        "/var/www/cooperapp/main.py",
        (1_153_434, 1_468_006),
    );
    en_curso.fichero_actual = Some("main.py".to_string());
    let mut estaticos = transferencia(
        2,
        EstadoTransferencia::EnCola,
        Direccion::Subida,
        "/home/hector/cooperapp/static",
        "/var/www/cooperapp/static",
        (0, 245_760),
    );
    estaticos.es_directorio = true;
    estaticos.ficheros_total = 14;
    let mut error = transferencia(
        5,
        EstadoTransferencia::Error,
        Direccion::Bajada,
        "/root/backup.tgz",
        "/home/hector/backup.tgz",
        (0, 52_428_800),
    );
    error.host_nombre = "hetzner-02".to_string();
    error.error = Some("permiso denegado".to_string());
    vec![
        en_curso,
        estaticos,
        transferencia(
            3,
            EstadoTransferencia::EnCola,
            Direccion::Bajada,
            "/var/log/nginx/access.log",
            "/home/hector/access.log",
            (0, 3_355_443),
        ),
        transferencia(
            4,
            EstadoTransferencia::Hecha,
            Direccion::Subida,
            "/home/hector/cooperapp/config.yaml",
            "/var/www/cooperapp/config.yaml",
            (2_048, 2_048),
        ),
        error,
    ]
}

/// Cola larga para que la tabla tenga que desplazarse.
fn cola_larga() -> Vec<InfoTransferencia> {
    let mut filas = cola();
    for id in 6..36 {
        filas.push(transferencia(
            id,
            EstadoTransferencia::EnCola,
            Direccion::Bajada,
            &format!("/var/log/app/registro-{id:02}.log"),
            &format!("/home/hector/registros/registro-{id:02}.log"),
            (0, 20_480),
        ));
    }
    filas
}

fn con_cola(prueba: &mut AppPrueba, filas: Vec<InfoTransferencia>) {
    prueba.servidor(MensajeServidor::Transferencias { lista: filas });
}

/// Archivos abierta y la cola ampliada (`t`) con `filas`.
fn con_transferencias_y_tema(
    cols: u16,
    filas: u16,
    tema: Tema,
    cola: Vec<InfoTransferencia>,
) -> AppPrueba {
    let mut prueba = con_archivos_y_tema(cols, filas, tema);
    con_cola(&mut prueba, cola);
    prueba.tecla(KeyCode::Char('t'));
    assert_eq!(prueba.app.vista, Vista::Transferencias);
    prueba
}

fn con_transferencias(cols: u16, filas: u16) -> AppPrueba {
    con_transferencias_y_tema(cols, filas, Tema::respaldo(), cola())
}

// ---------------------------------------------------------------- utilidades

fn sin_glifos_unicode(prueba: &AppPrueba, donde: &str) {
    let texto = prueba.texto();
    let intrusos: Vec<char> = texto
        .chars()
        .filter(|c| !(c.is_ascii() || c.is_alphabetic()))
        .collect();
    assert!(
        intrusos.is_empty(),
        "{donde}: glifos Unicode en ASCII {intrusos:?}\n{texto}"
    );
}

/// La selección está dentro de la ventana pintada de la lista.
fn seleccion_a_la_vista(prueba: &AppPrueba, lista: Lista, seleccion: usize) {
    let ventana = prueba
        .app
        .disposicion()
        .lista(lista)
        .unwrap_or_else(|| panic!("{lista:?} no se pintó"));
    assert!(
        ventana.inicio <= seleccion && seleccion < ventana.inicio + ventana.filas,
        "{lista:?}: la selección {seleccion} no se ve en {ventana:?}\n{}",
        prueba.texto()
    );
}

/// Primera línea pintada: el borde superior con la cabecera.
fn primera_linea(prueba: &AppPrueba) -> String {
    prueba
        .texto()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------- Archivos

/// `F4` abre Archivos con el host seleccionado en Flota y pide su SFTP.
#[test]
fn f4_abre_archivos_con_el_host_seleccionado() {
    let (mut prueba, _) = AppPrueba::con_semilla(120, 30);
    let host = prueba.app.host_flota_seleccionado().unwrap().clone();
    prueba.tecla(KeyCode::F(4));
    assert_eq!(prueba.app.vista, Vista::Archivos);
    assert_eq!(prueba.app.archivos.as_ref().unwrap().host_id, host.id);
    assert!(prueba.enviados().iter().any(|mensaje| matches!(
        mensaje,
        MensajeCliente::AbrirSftp { host_id, .. } if *host_id == host.id
    )));
}

#[test]
fn archivos_a_los_tres_tamanos() {
    let mut prueba = con_archivos(200, 60);
    prueba.instantanea("archivos_200x60");
    prueba.pasar_por(80, 24);
    prueba.instantanea("archivos_80x24");
    prueba.pasar_por(40, 12);
    prueba.instantanea("archivos_40x12");
}

/// Maqueta §6.4: estrecho, panel remoto activo y la cola resumida en el pie.
#[test]
fn archivos_estrecho_con_el_remoto_activo() {
    let mut prueba = con_archivos(80, 22);
    con_cola(&mut prueba, cola());
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.archivos.as_ref().unwrap().activo, Lado::Remoto);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.instantanea("archivos_80x22");
    let cabecera = primera_linea(&prueba);
    assert!(cabecera.contains("[remoto] ⇥"), "{cabecera}");
    assert!(cabecera.contains("local ⇄ hetzner-01"), "{cabecera}");
    let texto = prueba.texto();
    assert!(texto.contains(RUTA_REMOTA));
    assert!(texto.contains("↑ 78 % main.py"), "cola resumida en el pie");
    assert!(
        !texto.contains("transferencias"),
        "sin las 3 líneas de la cola"
    );
}

/// AC: Archivos a 45×20 pinta el aviso con «mínimo 50×14» y ningún panel.
#[test]
fn archivos_por_debajo_de_su_minimo() {
    let mut prueba = con_archivos(120, 30);
    prueba.pasar_por(45, 20);
    prueba.instantanea("archivos_45x20");
    let texto = prueba.texto();
    assert!(prueba.app.disposicion().aviso);
    assert!(texto.contains("mínimo 50×14"), "{texto}");
    for recortado in ["main.py", "elementos", RUTA_REMOTA, "ARCHIVOS", "hogar"] {
        assert!(
            !texto.contains(recortado),
            "«{recortado}» pintado:\n{texto}"
        );
    }
    assert!(prueba
        .app
        .disposicion()
        .lista(Lista::ArchivosLocal)
        .is_none());
}

/// Con la vista baja (< 20 filas) la cola no tiene sitio para sus 3 líneas:
/// se resume en el pie del panel remoto.
#[test]
fn archivos_con_cola_y_vista_baja() {
    let mut prueba = con_archivos(120, 18);
    con_cola(&mut prueba, cola());
    prueba.instantanea("archivos_cola_120x18");
    let texto = prueba.texto();
    assert!(texto.contains("↑ 78 % main.py"), "{texto}");
    assert!(!texto.contains("transferencias"), "{texto}");
    // Los dos paneles, lado a lado.
    assert!(texto.contains(RUTA_REMOTA) && texto.contains("hogar"));

    // Con alto de sobra vuelve la cola completa, con su barra.
    prueba.pasar_por(200, 60);
    prueba.instantanea("archivos_cola_200x60");
    let texto = prueba.texto();
    assert!(texto.contains("t transferencias"), "{texto}");
    assert!(texto.contains("en cola 2"), "{texto}");
    assert!(texto.contains("█"), "barra de progreso");
}

/// En estrecho se ve un panel; `Tab` alterna y la cabecera lo dice. En modo
/// normal se ven los dos.
#[test]
fn tab_alterna_el_panel_en_estrecho() {
    let mut prueba = con_archivos(80, 24);
    let hogar = prueba.app.rutas.hogar.display().to_string();
    assert!(primera_linea(&prueba).contains("[local]"));
    assert!(prueba.texto().contains(&hogar));
    assert!(!prueba.texto().contains(RUTA_REMOTA));
    assert!(prueba
        .app
        .disposicion()
        .lista(Lista::ArchivosRemoto)
        .is_none());

    prueba.tecla(KeyCode::Tab);
    assert!(primera_linea(&prueba).contains("[remoto]"));
    assert!(prueba.texto().contains(RUTA_REMOTA));
    assert!(!prueba.texto().contains(&hogar));
    assert!(prueba
        .app
        .disposicion()
        .lista(Lista::ArchivosLocal)
        .is_none());

    prueba.tecla(KeyCode::Tab);
    assert!(primera_linea(&prueba).contains("[local]"));

    prueba.pasar_por(120, 30);
    let texto = prueba.texto();
    assert!(texto.contains(&hogar) && texto.contains(RUTA_REMOTA));
    assert!(!texto.contains("[local]") && !texto.contains("[remoto]"));
}

/// La cabecera estrecha quita lo que no cabe, nunca el indicador de panel.
#[test]
fn la_cabecera_estrecha_conserva_el_indicador() {
    let mut prueba = con_archivos(50, 14);
    for (cols, filas) in [(50, 14), (56, 16), (64, 20), (99, 30)] {
        prueba.pasar_por(cols, filas);
        let cabecera = primera_linea(&prueba);
        assert!(cabecera.contains("[local] ⇥"), "{cols}×{filas}: {cabecera}");
        assert_eq!(cabecera.chars().count(), usize::from(cols));
        assert!(cabecera.ends_with('┐'), "{cols}×{filas}: {cabecera}");
    }
}

/// Selección, marcados, filtro y panel activo sobreviven a 200×60 → 80×24 →
/// 40×12 → 200×60 (y a una vista estrecha y baja), con la selección siempre
/// a la vista.
#[test]
fn el_estado_de_archivos_se_conserva_al_cambiar_de_modo() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 60);
    abrir_archivos_con(&mut prueba, &sembrado, entradas_remotas_largas());
    prueba.tecla(KeyCode::Tab);
    // Marca dos ficheros y deja el cursor al final de la lista.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::End);
    // Y en el panel local un filtro.
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char('/'));
    for letra in "md".chars() {
        prueba.tecla(KeyCode::Char(letra));
    }
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Tab);

    let foto = |prueba: &AppPrueba| {
        let archivos = prueba.app.archivos.as_ref().unwrap();
        let mut marcados: Vec<String> = archivos.remoto.marcados.iter().cloned().collect();
        marcados.sort();
        (
            archivos.activo,
            archivos.remoto.seleccion,
            marcados,
            archivos.local.filtro.clone(),
            archivos.local.seleccion,
        )
    };
    let antes = foto(&prueba);
    assert_eq!(antes.0, Lado::Remoto);
    assert_eq!(antes.1, 37, "cursor en la última fila (requirements.txt)");
    assert_eq!(antes.2, vec!["backup.tar.gz", "config.yaml"]);
    assert_eq!(antes.3, "md");
    seleccion_a_la_vista(&prueba, Lista::ArchivosRemoto, 37);

    for (cols, filas) in [(80, 24), (40, 12), (60, 16), (200, 60), (120, 18)] {
        prueba.pasar_por(cols, filas);
        assert_eq!(foto(&prueba), antes, "{cols}×{filas}");
        if prueba.app.disposicion().aviso {
            assert!(prueba.texto().contains("mínimo 50×14"));
            continue;
        }
        seleccion_a_la_vista(&prueba, Lista::ArchivosRemoto, 37);
        let texto = prueba.texto();
        assert!(texto.contains("2 marcado(s)"), "{cols}×{filas}\n{texto}");
        assert!(
            texto.contains("requirements.txt"),
            "{cols}×{filas}\n{texto}"
        );
        if cols >= 100 {
            assert!(texto.contains("hogar / md"), "{cols}×{filas}\n{texto}");
        } else {
            assert!(texto.contains("[remoto]"), "{cols}×{filas}\n{texto}");
        }
    }
    // Vuelta al panel local en estrecho: su filtro sigue ahí.
    prueba.pasar_por(80, 24);
    prueba.tecla(KeyCode::Tab);
    assert!(prueba.texto().contains("hogar / md"), "{}", prueba.texto());
    assert!(prueba.texto().contains("notas.md"));
    assert!(!prueba.texto().contains("main.py"));
}

/// Sin SFTP, el panel remoto dice por qué, también en estrecho (con `Tab`).
#[test]
fn archivos_sin_remoto_en_estrecho() {
    let mut prueba = con_archivos(80, 24);
    {
        let archivos = prueba.app.archivos.as_mut().unwrap();
        archivos.solo_local = true;
        archivos.motivo_solo_local = Some("el host no ofrece SFTP".to_string());
    }
    con_cola(&mut prueba, cola());
    prueba.tecla(KeyCode::Tab);
    let texto = prueba.texto();
    assert!(primera_linea(&prueba).contains("[remoto]"), "{texto}");
    assert!(texto.contains("sin remoto"), "{texto}");
    assert!(texto.contains("el host no ofrece SFTP"), "{texto}");
    assert!(texto.contains("↑ 78 % main.py"), "{texto}");
    prueba.pasar_por(120, 18);
    let texto = prueba.texto();
    assert!(
        texto.contains("sin remoto") && texto.contains("hogar"),
        "{texto}"
    );
    assert!(texto.contains("↑ 78 % main.py"), "{texto}");
}

/// El aviso de ficheros sensibles cabe a 50×14: si no hay sitio para todas
/// las coincidencias se resumen con «y N más», pero la pregunta y las teclas
/// se ven siempre.
#[test]
fn aviso_de_sensibles_en_el_minimo() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(50, 14);
    let claves = prueba.app.rutas.hogar.join("claves");
    fs::create_dir_all(&claves).unwrap();
    for letra in 'a'..='g' {
        fs::write(claves.join(format!("id_{letra}")), b"secreto").unwrap();
    }
    filetime::set_file_mtime(&claves, FileTime::from_unix_time(FECHA, 0)).unwrap();
    abrir_archivos(&mut prueba, &sembrado);
    // «..», app, claves: se copia el directorio con sus siete claves.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('c'));
    assert!(prueba.app.archivos.as_ref().unwrap().aviso.is_some());
    prueba.instantanea("archivos_aviso_sensibles_50x14");
    let texto = prueba.texto();
    assert!(texto.contains("id_a"), "{texto}");
    assert!(texto.contains("y 5 más"), "{texto}");
    assert!(texto.contains("s subir"), "{texto}");
    assert!(texto.contains("esc cancelar (recomendado)"), "{texto}");

    // Con sitio, las cinco que enseña el aviso y el resto.
    prueba.pasar_por(120, 30);
    let texto = prueba.texto();
    assert!(
        texto.contains("id_e") && texto.contains("y 2 más"),
        "{texto}"
    );
    assert!(texto.contains("esc cancelar (recomendado)"), "{texto}");

    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.archivos.as_ref().unwrap().aviso.is_none());
}

// ---------------------------------------------------------------- Transferencias

#[test]
fn transferencias_a_los_tres_tamanos() {
    let mut prueba = con_transferencias(200, 60);
    prueba.instantanea("transferencias_200x60");
    prueba.pasar_por(80, 24);
    prueba.instantanea("transferencias_80x24");
    prueba.pasar_por(40, 12);
    prueba.instantanea("transferencias_40x12");
    assert!(
        prueba.texto().contains("mínimo 50×12"),
        "{}",
        prueba.texto()
    );
}

/// Columnas por prioridad: al estrechar se va primero el texto del estado; el
/// glifo de estado, la ruta y el progreso se quedan.
#[test]
fn transferencias_columnas_por_prioridad() {
    let mut prueba = con_transferencias(80, 24);
    let texto = prueba.texto();
    for columna in ["ESTADO", "DIRECCIÓN", "ORIGEN → DESTINO", "PROGRESO"] {
        assert!(texto.contains(columna), "80×24 sin {columna}\n{texto}");
    }
    assert!(texto.contains("◐ en curso"), "{texto}");

    prueba.pasar_por(50, 12);
    prueba.instantanea("transferencias_50x12");
    let texto = prueba.texto();
    assert!(!texto.contains("ESTADO"), "{texto}");
    assert!(!texto.contains("en curso  "), "{texto}");
    for columna in ["DIRECCIÓN", "ORIGEN", "PROGRESO"] {
        assert!(texto.contains(columna), "50×12 sin {columna}\n{texto}");
    }
    // El glifo de estado y el progreso de cada fila siguen ahí.
    assert!(texto.contains("◐ ↑ subida"), "{texto}");
    assert!(texto.contains("✕ ↓ bajada"), "{texto}");
    assert!(texto.contains("78 %"), "{texto}");
}

/// Con la vista baja el detalle inferior se pliega (el borde dice `↵
/// detalle`) y `↵` lo abre en un diálogo; con alto de sobra vuelve.
#[test]
fn transferencias_detalle_plegado_en_vista_baja() {
    let mut prueba = con_transferencias(80, 24);
    assert!(prueba.texto().contains("fichero(s)"), "{}", prueba.texto());
    assert!(prueba.texto().contains("ahora: main.py"));

    prueba.pasar_por(80, 16);
    prueba.instantanea("transferencias_80x16");
    let texto = prueba.texto();
    assert!(!texto.contains("fichero(s)"), "{texto}");
    assert!(!texto.contains("ahora: main.py"), "{texto}");
    assert!(texto.contains("↵ detalle ┘"), "{texto}");

    prueba.tecla(KeyCode::Enter);
    assert!(
        matches!(
            &prueba.app.dialogo,
            Some(magi::app::Dialogo::Detalle { titulo, .. }) if titulo == "TRANSFERENCIA 1"
        ),
        "↵ abre el detalle"
    );
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());

    prueba.pasar_por(80, 24);
    assert!(prueba.texto().contains("fichero(s)"));
}

/// La selección de la cola se conserva y queda a la vista en cualquier
/// tamaño, sin huecos al final.
#[test]
fn transferencias_seleccion_a_la_vista_al_cambiar_de_modo() {
    let mut prueba = con_transferencias_y_tema(200, 60, Tema::respaldo(), cola_larga());
    prueba.tecla(KeyCode::End);
    let ultima = cola_larga().len() - 1;
    assert_eq!(prueba.app.seleccion_cola, ultima);
    seleccion_a_la_vista(&prueba, Lista::Transferencias, ultima);
    for (cols, filas) in [(80, 24), (40, 12), (50, 12), (200, 60), (80, 16)] {
        prueba.pasar_por(cols, filas);
        assert_eq!(prueba.app.seleccion_cola, ultima, "{cols}×{filas}");
        if prueba.app.disposicion().aviso {
            continue;
        }
        seleccion_a_la_vista(&prueba, Lista::Transferencias, ultima);
        assert!(
            prueba.texto().contains("registro-35.log"),
            "{cols}×{filas}\n{}",
            prueba.texto()
        );
    }
    // Arriba del todo: la ventana sigue a la selección.
    prueba.tecla(KeyCode::Home);
    seleccion_a_la_vista(&prueba, Lista::Transferencias, 0);
}

// ---------------------------------------------------------------- ASCII

/// Con el tema ASCII ninguna de las dos vistas pinta glifos Unicode, en
/// ningún modo (normal, estrecho, bajo, aviso).
#[test]
fn modo_ascii_sin_glifos_unicode() {
    let mut prueba = con_archivos_y_tema(200, 60, arnes::tema_ascii());
    con_cola(&mut prueba, cola());
    // Una fila marcada, para que salga su marca.
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char(' '));
    sin_glifos_unicode(&prueba, "archivos 200×60");
    for (cols, filas) in [(80, 24), (40, 12), (120, 18), (50, 14), (80, 22)] {
        prueba.pasar_por(cols, filas);
        sin_glifos_unicode(&prueba, &format!("archivos {cols}×{filas}"));
    }
    prueba.tecla(KeyCode::Tab);
    sin_glifos_unicode(&prueba, "archivos remoto 80×22");
    prueba.instantanea("archivos_ascii_80x22");
    assert!(primera_linea(&prueba).contains("[remoto] tab"));

    let mut prueba = con_transferencias_y_tema(200, 60, arnes::tema_ascii(), cola());
    sin_glifos_unicode(&prueba, "transferencias 200×60");
    for (cols, filas) in [(80, 24), (40, 12), (50, 12), (80, 16)] {
        prueba.pasar_por(cols, filas);
        sin_glifos_unicode(&prueba, &format!("transferencias {cols}×{filas}"));
    }
    prueba.pasar_por(80, 24);
    prueba.instantanea("transferencias_ascii_80x24");
}

// ---------------------------------------------------------------- revisión

/// Listado remoto de más de mil entradas: el pie del panel pasa de 40
/// caracteres cuando se marcan todas.
fn entradas_remotas_enormes() -> Vec<Entrada> {
    let mut entradas = entradas_remotas();
    for indice in 0..1_022 {
        entradas.push(entrada(&format!("f{indice:04}"), Some(2_048), FECHA));
    }
    entradas
}

/// Regresión: con un pie largo (mil entradas, todas marcadas) la cola resumida
/// no desaparece del pie ni queda en «↑ 7…»: cede el pie y se ve al menos el
/// avance, en modo normal-bajo (borde del panel remoto) y en estrecho.
#[test]
fn la_cola_resumida_no_desaparece_con_un_pie_largo() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(100, 18);
    abrir_archivos_con(&mut prueba, &sembrado, entradas_remotas_enormes());
    con_cola(&mut prueba, cola());
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char('a'));
    assert!(
        prueba
            .app
            .archivos
            .as_ref()
            .unwrap()
            .remoto
            .total_marcados()
            >= 1_029
    );
    for (cols, filas) in [(100, 18), (104, 18), (50, 14), (60, 16)] {
        prueba.pasar_por(cols, filas);
        let texto = prueba.texto();
        assert!(texto.contains("↑ 78 %"), "{cols}×{filas}\n{texto}");
        assert!(texto.contains("1030 elementos"), "{cols}×{filas}\n{texto}");
    }
    // Con sitio, el pie entero y la cola con el nombre.
    prueba.pasar_por(130, 18);
    let texto = prueba.texto();
    assert!(texto.contains("marcado(s)"), "{texto}");
    assert!(texto.contains("↑ 78 % main.py"), "{texto}");
    prueba.pasar_por(80, 22);
    let texto = prueba.texto();
    assert!(texto.contains("marcado(s)"), "{texto}");
    assert!(texto.contains("↑ 78 % main.py"), "{texto}");
}

/// Regresión: un tamaño más ancho que su columna (`1000.0 MB`) no empuja la
/// fecha ni la marca de su fila.
#[test]
fn un_tamano_largo_no_desalinea_las_filas() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(120, 30);
    let mut entradas = entradas_remotas();
    entradas.push(entrada("imagen.iso", Some(1_048_576_000), FECHA));
    abrir_archivos_con(&mut prueba, &sembrado, entradas);
    prueba.tecla(KeyCode::Tab);
    for (cols, filas) in [(120, 30), (80, 24), (50, 14)] {
        prueba.pasar_por(cols, filas);
        let texto = prueba.texto();
        assert!(texto.contains("1000.0 MB"), "{cols}×{filas}\n{texto}");
        // Columna (en caracteres) de la marca `✕` del panel remoto (el de la
        // derecha) en las dos filas que la llevan.
        let columnas: Vec<usize> = texto
            .lines()
            .filter(|linea| linea.contains("imagen.iso") || linea.contains("backup.tar.gz"))
            .map(|linea| {
                let caracteres: Vec<char> = linea.chars().collect();
                caracteres.iter().rposition(|c| *c == '✕').expect("marca")
            })
            .collect();
        assert_eq!(columnas.len(), 2, "{cols}×{filas}\n{texto}");
        assert_eq!(columnas[0], columnas[1], "{cols}×{filas}\n{texto}");
    }
}

/// En ventanas más anchas que dos paneles de detalle los paneles no crecen:
/// se quedan en 2 × 120 columnas, centrados.
#[test]
fn los_paneles_no_crecen_sin_limite() {
    let mut prueba = con_archivos(300, 80);
    con_cola(&mut prueba, cola());
    let texto = prueba.texto();
    let primera = texto.lines().next().unwrap_or_default();
    let margen = primera.chars().take_while(|c| *c == ' ').count();
    assert_eq!(margen, 30, "{primera}");
    assert_eq!(primera.chars().count(), 270, "{primera}");
    // La cola completa va dentro del mismo ancho.
    let cola = texto
        .lines()
        .find(|linea| linea.contains("en cola 2"))
        .expect("cola completa");
    assert!(cola.starts_with(&" ".repeat(30)), "{cola}");
    // A 200×60 no hay límite que aplicar: los paneles llenan el ancho.
    prueba.pasar_por(200, 60);
    assert!(primera_linea(&prueba).starts_with('┌'));
}

/// En ASCII «en cola» y «en curso» comparten glifo (`o`): al estrechar se va
/// antes la dirección que el texto del estado.
#[test]
fn transferencias_en_ascii_conservan_el_texto_del_estado() {
    let mut prueba = con_transferencias_y_tema(80, 24, arnes::tema_ascii(), cola());
    prueba.pasar_por(50, 12);
    prueba.instantanea("transferencias_ascii_50x12");
    let texto = prueba.texto();
    assert!(texto.contains("o en curso"), "{texto}");
    assert!(texto.contains("o en cola"), "{texto}");
    assert!(!texto.contains("DIRECCI"), "{texto}");
    sin_glifos_unicode(&prueba, "transferencias ASCII 50×12");
}

/// El aviso de sensibles en ASCII: sin glifos Unicode (tampoco el «¿») y con
/// la pregunta y las teclas siempre a la vista.
#[test]
fn aviso_de_sensibles_en_ascii() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(50, 14, arnes::tema_ascii());
    let claves = prueba.app.rutas.hogar.join("claves");
    fs::create_dir_all(&claves).unwrap();
    for letra in 'a'..='g' {
        fs::write(claves.join(format!("id_{letra}")), b"secreto").unwrap();
    }
    filetime::set_file_mtime(&claves, FileTime::from_unix_time(FECHA, 0)).unwrap();
    abrir_archivos(&mut prueba, &sembrado);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('c'));
    assert!(prueba.app.archivos.as_ref().unwrap().aviso.is_some());
    for (cols, filas) in [(50, 14), (64, 16), (80, 24), (120, 30), (200, 60)] {
        prueba.pasar_por(cols, filas);
        sin_glifos_unicode(&prueba, &format!("aviso ASCII {cols}×{filas}"));
        let texto = prueba.texto();
        assert!(texto.contains("s subir"), "{cols}×{filas}\n{texto}");
        assert!(texto.contains("esc cancelar"), "{cols}×{filas}\n{texto}");
    }
    assert!(prueba.texto().contains("Subirlos igualmente?"));
}

/// Pintadas directamente en áreas diminutas (y vacías), las dos vistas no
/// entran en pánico en ningún modo: normal, estrecho, sin remoto y con el
/// aviso de sensibles.
#[test]
fn areas_diminutas_sin_panico() {
    use magi::ui::disposicion::{minimo_de, Disposicion};
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    let (mut prueba, sembrado) = AppPrueba::con_semilla(120, 30);
    let claves = prueba.app.rutas.hogar.join("claves");
    fs::create_dir_all(&claves).unwrap();
    fs::write(claves.join("id_a"), b"secreto").unwrap();
    abrir_archivos(&mut prueba, &sembrado);
    con_cola(&mut prueba, cola());
    prueba.tecla(KeyCode::Char(' '));
    let pintar = |prueba: &AppPrueba| {
        for ancho in 0..=12u16 {
            for alto in 0..=8u16 {
                let mut terminal = Terminal::new(TestBackend::new(12, 8)).unwrap();
                terminal
                    .draw(|marco| {
                        let area = Rect::new(0, 0, ancho, alto);
                        let mut disp = Disposicion::nueva(area, minimo_de(Vista::Archivos, false));
                        magi::ui::archivos::dibujar(marco, area, &prueba.app, &mut disp);
                        magi::ui::transferencias::dibujar(marco, area, &prueba.app, &mut disp);
                    })
                    .unwrap();
            }
        }
    };
    pintar(&prueba);
    prueba.tecla(KeyCode::Tab);
    pintar(&prueba);
    prueba.app.archivos.as_mut().unwrap().solo_local = true;
    pintar(&prueba);
    prueba.app.archivos.as_mut().unwrap().solo_local = false;
    prueba.tecla(KeyCode::Tab);
    // Sin marcas, `c` copia la seleccionada: el directorio con la clave.
    prueba.tecla(KeyCode::Char('A'));
    prueba.tecla(KeyCode::Home);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char('c'));
    assert!(prueba.app.archivos.as_ref().unwrap().aviso.is_some());
    pintar(&prueba);
}

/// Barrido de tamaños por la App real (con los dos paneles activos): sin
/// pánico, la cabecera estrecha ocupa el ancho exacto y conserva el
/// indicador, y la cola resumida está en el pie siempre que la vista es
/// estrecha o baja.
#[test]
fn barrido_de_tamanos() {
    let mut prueba = con_archivos(120, 30);
    con_cola(&mut prueba, cola());
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Char(' '));
    for _ in 0..2 {
        prueba.tecla(KeyCode::Tab);
        for cols in [
            1u16, 2, 3, 20, 39, 40, 49, 50, 51, 64, 79, 80, 99, 100, 101, 199, 200, 240, 241, 300,
        ] {
            for filas in [1u16, 2, 3, 8, 12, 13, 14, 19, 20, 60] {
                prueba.pasar_por(cols, filas);
                if prueba.app.disposicion().aviso {
                    continue;
                }
                let texto = prueba.texto();
                if cols < 100 {
                    let cabecera = primera_linea(&prueba);
                    assert!(cabecera.contains("] ⇥"), "{cols}×{filas}: {cabecera}");
                    assert_eq!(cabecera.chars().count(), usize::from(cols), "{cabecera}");
                }
                if cols < 100 || filas < 20 {
                    assert!(texto.contains("↑ 78 %"), "{cols}×{filas}\n{texto}");
                    assert!(
                        !texto.contains("t transferencias"),
                        "{cols}×{filas}\n{texto}"
                    );
                } else {
                    assert!(
                        texto.contains("t transferencias"),
                        "{cols}×{filas}\n{texto}"
                    );
                }
            }
        }
    }
}

/// El filtro a medio escribir (campo abierto) sobrevive a los cambios de modo
/// y sigue recibiendo teclas después.
#[test]
fn el_filtro_a_medio_escribir_sobrevive_al_redimensionado() {
    let mut prueba = con_archivos(200, 60);
    prueba.tecla(KeyCode::Char('/'));
    prueba.tecla(KeyCode::Char('m'));
    for (cols, filas) in [(80, 24), (40, 12), (50, 14), (200, 60), (120, 18)] {
        prueba.pasar_por(cols, filas);
        let panel = &prueba.app.archivos.as_ref().unwrap().local;
        assert!(panel.filtro_activo, "{cols}×{filas}");
        assert_eq!(panel.filtro, "m", "{cols}×{filas}");
        if !prueba.app.disposicion().aviso {
            assert!(prueba.texto().contains("hogar / m"), "{}", prueba.texto());
        }
    }
    prueba.pasar_por(80, 24);
    prueba.tecla(KeyCode::Char('d'));
    let texto = prueba.texto();
    assert!(texto.contains("hogar / md"), "{texto}");
    assert!(
        texto.contains("README.md") && !texto.contains("main.py"),
        "{texto}"
    );
}
