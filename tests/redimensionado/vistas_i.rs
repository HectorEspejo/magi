//! Diálogo PERMISOS (Fase 8): rejilla rwx, octal, mixto y recursivo.
//! Instantáneas a 40×12, 80×24 y 200×60 (T45).
//!
//! Archivos se abre como en `vistas_d`: el servidor contesta `SftpAbierto` y
//! `DirListado` con un listado de `/srv/web` de modos distintos, y el panel
//! local lista `rutas.hogar`. Lo remoto se comprueba por lo que la App envía
//! (`CambiarPermisos`, `ListarArbol`); lo local, con `chmod` de verdad sobre
//! ficheros del hogar de la prueba.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use filetime::FileTime;

use magi::app::permisos::{
    DialogoPermisos, FocoPermisos, FormularioPermisos, Objetivo, RecuentoEnVuelo,
};
use magi::app::Dialogo;
use magi::archivos::permisos::{Casilla, Recuento};
use magi::archivos::{Entrada, Lado, Marca, Propietario, TipoEntrada};
use magi::protocolo::{AlcancePermisos, EntradaArbol, MensajeCliente, MensajeServidor};
use magi::ui::Vista;

use super::vistas_g::FabricaDialogo;
use crate::arnes::{self, AppPrueba};

/// 12 sep 2025 09:41:07 UTC.
const FECHA: i64 = 1_757_670_067;
const RUTA_REMOTA: &str = "/srv/web";

// ---------------------------------------------------------------- estado

fn deploy() -> Option<Propietario> {
    Some(Propietario {
        uid: Some(1001),
        gid: Some(1001),
        usuario: Some("deploy".to_string()),
        grupo: Some("deploy".to_string()),
    })
}

fn entrada(nombre: &str, tipo: TipoEntrada, modo: u32, tamano: u64) -> Entrada {
    Entrada {
        nombre: nombre.to_string(),
        tipo,
        tamano,
        mtime: FECHA,
        permisos: Some(modo),
        propietario: deploy(),
        enlace: None,
        marca: Marca::Ninguna,
    }
}

/// `/srv/web` con los bits de tipo que manda SFTP: directorios 0755 y 2775,
/// ficheros 0600, 0644 y 0755, un enlace y uno de `www-data` sin nombre.
fn entradas_remotas() -> Vec<Entrada> {
    let mut enlace = entrada("enlace", TipoEntrada::Enlace, 0o120777, 7);
    enlace.enlace = Some("main.py".to_string());
    let mut ajeno = entrada("www.conf", TipoEntrada::Fichero, 0o100644, 300);
    ajeno.propietario = Some(Propietario {
        uid: Some(33),
        gid: Some(33),
        usuario: None,
        grupo: None,
    });
    vec![
        entrada("app", TipoEntrada::Directorio, 0o40755, 4_096),
        entrada("static", TipoEntrada::Directorio, 0o42775, 4_096),
        entrada("config.yaml", TipoEntrada::Fichero, 0o100600, 2_048),
        enlace,
        entrada("main.py", TipoEntrada::Fichero, 0o100644, 14_336),
        entrada("run.sh", TipoEntrada::Fichero, 0o100755, 512),
        ajeno,
    ]
}

/// Un fichero del hogar con modo y fecha fijos (la umask no cuenta).
fn fichero(ruta: &Path, modo: u32) {
    fs::write(ruta, b"contenido").unwrap();
    fs::set_permissions(ruta, fs::Permissions::from_mode(modo)).unwrap();
    filetime::set_file_mtime(ruta, FileTime::from_unix_time(FECHA, 0)).unwrap();
}

fn directorio(ruta: &Path, modo: u32) {
    fs::create_dir_all(ruta).unwrap();
    fs::set_permissions(ruta, fs::Permissions::from_mode(modo)).unwrap();
}

fn modo_de(ruta: &Path) -> u32 {
    fs::symlink_metadata(ruta).unwrap().permissions().mode() & 0o7777
}

/// Abre Archivos con hetzner-01 y `/srv/web` en el remoto. El hogar se
/// prepara antes con `preparar` (los directorios se fechan al final).
fn con_archivos(cols: u16, filas: u16, preparar: impl FnOnce(&Path)) -> AppPrueba {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(cols, filas);
    let hogar = prueba.app.rutas.hogar.clone();
    fs::create_dir_all(&hogar).unwrap();
    fichero(&hogar.join("notas.md"), 0o644);
    preparar(&hogar);
    let host_id = sembrado.host("hetzner-01");
    prueba.app.abrir_archivos(Some(host_id));
    assert_eq!(prueba.app.vista, Vista::Archivos);
    prueba.enviados();
    prueba.servidor(MensajeServidor::SftpAbierto {
        host_id,
        dir_inicio: RUTA_REMOTA.to_string(),
        peticion_id: None,
        usuario_conexion: Some("deploy".to_string()),
        uid_conexion: Some(1001),
    });
    let peticion_id = listado_pedido(&mut prueba).expect("la App pide el listado remoto");
    prueba.servidor(MensajeServidor::DirListado {
        host_id,
        ruta: RUTA_REMOTA.to_string(),
        entradas: entradas_remotas(),
        peticion_id,
    });
    assert_eq!(
        prueba.app.archivos.as_ref().unwrap().remoto.ruta,
        RUTA_REMOTA
    );
    prueba
}

fn con_remoto(cols: u16, filas: u16) -> AppPrueba {
    let mut prueba = con_archivos(cols, filas, |_| {});
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.archivos.as_ref().unwrap().activo, Lado::Remoto);
    prueba
}

/// `peticion_id` del `ListarDir` enviado desde la última llamada.
fn listado_pedido(prueba: &mut AppPrueba) -> Option<u64> {
    prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ListarDir { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
}

/// Marca estos nombres en el panel activo (como `Espacio` sobre cada uno).
fn marcar(prueba: &mut AppPrueba, nombres: &[&str]) {
    let archivos = prueba.app.archivos.as_mut().unwrap();
    let panel = match archivos.activo {
        Lado::Local => &mut archivos.local,
        Lado::Remoto => &mut archivos.remoto,
    };
    for nombre in nombres {
        assert!(
            panel
                .entradas
                .iter()
                .any(|entrada| entrada.nombre == *nombre),
            "«{nombre}» no está en el panel"
        );
        panel.marcados.insert(nombre.to_string());
    }
}

/// El cursor a la fila de `nombre` en el panel activo.
fn ir_a(prueba: &mut AppPrueba, nombre: &str) {
    prueba.tecla(KeyCode::Home);
    for _ in 0..20 {
        let archivos = prueba.app.archivos.as_ref().unwrap();
        let panel = match archivos.activo {
            Lado::Local => &archivos.local,
            Lado::Remoto => &archivos.remoto,
        };
        if panel
            .entrada_actual()
            .map(|entrada| entrada.nombre.as_str())
            == Some(nombre)
        {
            return;
        }
        prueba.tecla(KeyCode::Down);
    }
    panic!("«{nombre}» no se alcanza con el cursor");
}

fn formulario(prueba: &AppPrueba) -> &FormularioPermisos {
    match &prueba.app.dialogo {
        Some(Dialogo::Permisos(DialogoPermisos::Formulario(formulario))) => formulario,
        _ => panic!("no está el diálogo PERMISOS:\n{}", prueba.texto()),
    }
}

fn ctrl_s(prueba: &mut AppPrueba) {
    prueba.tecla_con(KeyCode::Char('s'), KeyModifiers::CONTROL);
}

fn escribir(prueba: &mut AppPrueba, texto: &str) {
    for caracter in texto.chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
}

/// Tab hasta el campo `foco`.
fn ir_al_campo(prueba: &mut AppPrueba, foco: FocoPermisos) {
    for _ in 0..5 {
        if formulario(prueba).foco == foco {
            return;
        }
        prueba.tecla(KeyCode::Tab);
    }
    panic!("el foco no llega a {foco:?}");
}

fn mensaje(prueba: &AppPrueba) -> String {
    prueba
        .app
        .mensaje
        .as_ref()
        .map(|mensaje| mensaje.texto.clone())
        .unwrap_or_default()
}

/// Deja correr el bucle (hilos locales) hasta que se cumpla `hasta`.
fn esperar(prueba: &mut AppPrueba, que: &str, hasta: impl Fn(&AppPrueba) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !hasta(prueba) {
        assert!(
            Instant::now() < limite,
            "sin {que}: «{}»\n{}",
            mensaje(prueba),
            prueba.texto()
        );
        prueba
            .app
            .bombear(&mut prueba.terminal, Duration::from_millis(20))
            .unwrap();
    }
}

fn cambiar_permisos(prueba: &mut AppPrueba) -> Vec<MensajeCliente> {
    prueba
        .enviados()
        .into_iter()
        .filter(|mensaje| matches!(mensaje, MensajeCliente::CambiarPermisos { .. }))
        .collect()
}

fn arbol(ruta: &str, tipo: TipoEntrada) -> EntradaArbol {
    EntradaArbol {
        ruta: ruta.to_string(),
        tipo,
        tamano: 10,
        mtime: FECHA,
        permisos: Some(0o644),
        propietario: None,
        enlace_a_dir: false,
    }
}

/// El texto pintado de las celdas con el estilo dado, en orden.
fn celdas_con_color(prueba: &AppPrueba, color: ratatui::style::Color) -> String {
    let buffer = prueba.terminal.backend().interior.buffer();
    let mut texto = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let celda = &buffer[(x, y)];
            if celda.fg == color {
                texto.push_str(celda.symbol());
            }
        }
    }
    texto
}

// ---------------------------------------------------------------- fábricas

/// app (0755), main.py (0644) y config.yaml (0600): casillas mixtas.
fn formulario_mixto() -> FormularioPermisos {
    let objetivo = |nombre: &str, es_dir: bool| Objetivo {
        ruta: format!("{RUTA_REMOTA}/{nombre}"),
        es_dir,
    };
    FormularioPermisos::nuevo(
        Lado::Remoto,
        1,
        vec![
            (objetivo("app", true), Some(0o40755)),
            (objetivo("config.yaml", false), Some(0o100600)),
            (objetivo("main.py", false), Some(0o100644)),
        ],
    )
}

/// El mixto con 0644 escrito, recursivo y el foco en el alcance: aviso.
fn formulario_recursivo() -> FormularioPermisos {
    let mut formulario = formulario_mixto();
    for digito in "0644".chars() {
        formulario.modo.escribir(digito);
    }
    formulario.recursivo = true;
    formulario.foco = FocoPermisos::Alcance;
    formulario
}

fn contando() -> RecuentoEnVuelo {
    RecuentoEnVuelo {
        id: 1,
        orden: formulario_recursivo().orden().unwrap(),
        recuento: Recuento {
            ficheros: 1_342,
            directorios: 87,
            ..Recuento::default()
        },
        pendientes: 1,
    }
}

/// Diálogos de esta vista para las pruebas comunes de `vistas_g` (enteros a
/// 40×12 y sin glifos Unicode en ASCII).
pub(crate) fn dialogos() -> Vec<(&'static str, FabricaDialogo)> {
    vec![
        (
            "PERMISOS",
            Box::new(|| Dialogo::Permisos(DialogoPermisos::Formulario(formulario_mixto()))),
        ),
        (
            "PERMISOS",
            Box::new(|| Dialogo::Permisos(DialogoPermisos::Formulario(formulario_recursivo()))),
        ),
        (
            "PERMISOS",
            Box::new(|| Dialogo::Permisos(DialogoPermisos::Contando(contando()))),
        ),
    ]
}

// ---------------------------------------------------------------- instantáneas

/// Maqueta §6.1 con tres marcados de modos distintos: `[~]` donde no
/// coinciden y el octal vacío.
#[test]
fn permisos_con_mixto_a_los_tres_tamanos() {
    let mut prueba = con_remoto(200, 60);
    marcar(&mut prueba, &["app", "config.yaml", "main.py"]);
    prueba.tecla(KeyCode::Char('p'));
    prueba.instantanea("permisos_mixto_200x60");
    let texto = prueba.texto();
    assert!(
        texto.contains("PERMISOS · 3 elementos (2 ficheros, 1 directorio)"),
        "{texto}"
    );
    assert!(texto.contains("leer   escribir   ejecutar"), "{texto}");
    assert!(texto.contains("[~]"), "{texto}");
    assert!(texto.contains("octal       [      ]"), "{texto}");
    assert!(texto.contains("[ ] recursivo"), "{texto}");

    prueba.pasar_por(80, 24);
    prueba.instantanea("permisos_mixto_80x24");
    prueba.pasar_por(40, 12);
    prueba.instantanea("permisos_mixto_40x12");
    let texto = prueba.texto();
    assert!(texto.contains("PERMISOS · 3 elementos"), "{texto}");
    assert!(texto.contains(" r   w   x"), "rótulos cortos:\n{texto}");
    assert!(texto.contains("u     [x] [x] [~]"), "{texto}");
}

/// Recursivo con «todo» y 0644: el aviso ámbar.
#[test]
fn permisos_recursivo_con_aviso_a_los_tres_tamanos() {
    let mut prueba = con_remoto(200, 60);
    marcar(&mut prueba, &["app", "main.py"]);
    prueba.tecla(KeyCode::Char('p'));
    ir_al_campo(&mut prueba, FocoPermisos::Octal);
    escribir(&mut prueba, "0644");
    ir_al_campo(&mut prueba, FocoPermisos::Recursivo);
    prueba.tecla(KeyCode::Char(' '));
    ir_al_campo(&mut prueba, FocoPermisos::Alcance);
    prueba.instantanea("permisos_recursivo_200x60");
    let texto = prueba.texto();
    assert!(texto.contains("[x] recursivo"), "{texto}");
    assert!(texto.contains("(•) todo"), "{texto}");
    assert!(
        texto.contains("⚠ 0644 en directorios impide entrar en ellos"),
        "{texto}"
    );
    prueba.pasar_por(80, 24);
    prueba.instantanea("permisos_recursivo_80x24");
    prueba.pasar_por(40, 12);
    prueba.instantanea("permisos_recursivo_40x12");
    // El foco (alcance) sigue a la vista aunque no quepa todo.
    let texto = prueba.texto();
    assert!(texto.contains("(•) todo"), "{texto}");
}

// ---------------------------------------------------------------- estado inicial

/// `p` sobre la fila actual: su modo, con el cuarto dígito, y sin recursivo
/// si no es un directorio.
#[test]
fn p_abre_con_el_modo_de_la_fila_actual() {
    let mut prueba = con_remoto(120, 30);
    ir_a(&mut prueba, "main.py");
    prueba.tecla(KeyCode::Char('p'));
    let abierto = formulario(&prueba);
    assert_eq!(abierto.objetivos.len(), 1);
    assert_eq!(abierto.objetivos[0].ruta, "/srv/web/main.py");
    assert_eq!(abierto.modo.octal, "0644");
    assert_eq!(abierto.modo.casillas.simbolico(), "rw-r--r--");
    assert!(!abierto.hay_directorios());
    let texto = prueba.texto();
    assert!(
        texto.contains("PERMISOS · 1 elemento (1 fichero)"),
        "{texto}"
    );
    assert!(texto.contains("[ 0644 ]"), "{texto}");
    assert!(
        !texto.contains("recursivo"),
        "solo con directorios:\n{texto}"
    );

    // Un setgid enseña su cuarto dígito.
    prueba.tecla(KeyCode::Esc);
    ir_a(&mut prueba, "static");
    prueba.tecla(KeyCode::Char('p'));
    assert_eq!(formulario(&prueba).modo.octal, "2775");
    assert!(prueba.texto().contains("[ ] recursivo"));
}

/// Con modos distintos, `[~]` donde no coinciden y el octal vacío.
#[test]
fn p_con_modos_distintos_marca_mixto() {
    let mut prueba = con_remoto(120, 30);
    marcar(&mut prueba, &["app", "config.yaml", "main.py"]);
    prueba.tecla(KeyCode::Char('p'));
    let formulario = formulario(&prueba);
    assert_eq!(formulario.modo.octal, "");
    assert_eq!(formulario.modo.casillas.simbolico(), "rw~~-~~-~");
    assert_eq!(formulario.modo.casillas.casilla(0, 2), Casilla::Mixto);
    assert_eq!(formulario.modo.casillas.casilla(0, 0), Casilla::Si);
    assert_eq!(formulario.modo.casillas.casilla(1, 1), Casilla::No);
    let rutas: Vec<&str> = formulario
        .objetivos
        .iter()
        .map(|objetivo| objetivo.ruta.as_str())
        .collect();
    assert_eq!(
        rutas,
        vec!["/srv/web/app", "/srv/web/config.yaml", "/srv/web/main.py"]
    );
}

/// Los enlaces no se tocan: con solo un enlace no hay diálogo; con más cosas,
/// se queda fuera y lo dice.
#[test]
fn los_enlaces_se_quedan_fuera() {
    let mut prueba = con_remoto(120, 30);
    ir_a(&mut prueba, "enlace");
    prueba.tecla(KeyCode::Char('p'));
    assert!(prueba.app.dialogo.is_none());
    assert!(mensaje(&prueba).contains("los enlaces no se tocan"));

    marcar(&mut prueba, &["enlace", "main.py"]);
    prueba.tecla(KeyCode::Char('p'));
    assert_eq!(formulario(&prueba).objetivos.len(), 1);
    assert!(mensaje(&prueba).contains("1 enlace sin tocar"));

    // «..» tampoco.
    prueba.tecla(KeyCode::Esc);
    prueba
        .app
        .archivos
        .as_mut()
        .unwrap()
        .remoto
        .marcados
        .clear();
    prueba.tecla(KeyCode::Home);
    prueba.tecla(KeyCode::Char('p'));
    assert!(prueba.app.dialogo.is_none());
    assert!(mensaje(&prueba).contains("no hay nada seleccionado"));
}

// ---------------------------------------------------------------- teclado

/// Flechas y `Espacio` en la rejilla cambian el octal; dígitos válidos en el
/// octal cambian las casillas; `Tab` y `Shift+Tab` recorren los campos.
#[test]
fn la_rejilla_y_el_octal_se_sincronizan() {
    let mut prueba = con_remoto(120, 30);
    marcar(&mut prueba, &["app", "main.py"]);
    prueba.tecla(KeyCode::Char('p'));
    assert_eq!(formulario(&prueba).modo.octal, "", "x mixta");
    // usuario · ejecutar: de mixto a sí; grupo y otros, igual.
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    assert_eq!(formulario(&prueba).celda, (0, 2), "la rejilla no se sale");
    for _ in 0..3 {
        prueba.tecla(KeyCode::Char(' '));
        prueba.tecla(KeyCode::Down);
    }
    assert_eq!(formulario(&prueba).celda, (2, 2));
    assert_eq!(formulario(&prueba).modo.octal, "0755");
    // Y quitar la escritura al usuario.
    prueba.tecla(KeyCode::Up);
    prueba.tecla(KeyCode::Up);
    prueba.tecla(KeyCode::Left);
    prueba.tecla(KeyCode::Char(' '));
    assert_eq!(formulario(&prueba).modo.octal, "0555");
    assert!(prueba.texto().contains("[ 0555 ]"), "{}", prueba.texto());

    // Octal → casillas.
    prueba.tecla(KeyCode::Tab);
    assert_eq!(formulario(&prueba).foco, FocoPermisos::Octal);
    for _ in 0..4 {
        prueba.tecla(KeyCode::Backspace);
    }
    escribir(&mut prueba, "6409");
    assert_eq!(formulario(&prueba).modo.octal, "640", "el 9 no entra");
    assert_eq!(formulario(&prueba).modo.casillas.simbolico(), "rw-r-----");
    // Tab: recursivo (hay un directorio); el alcance solo con él marcado.
    prueba.tecla(KeyCode::Tab);
    assert_eq!(formulario(&prueba).foco, FocoPermisos::Recursivo);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(formulario(&prueba).foco, FocoPermisos::Rejilla);
    prueba.tecla(KeyCode::BackTab);
    assert_eq!(formulario(&prueba).foco, FocoPermisos::Recursivo);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Tab);
    assert_eq!(formulario(&prueba).foco, FocoPermisos::Alcance);
    prueba.tecla(KeyCode::Right);
    assert_eq!(formulario(&prueba).alcance, AlcancePermisos::Directorios);
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    assert_eq!(formulario(&prueba).alcance, AlcancePermisos::Ficheros);
    prueba.tecla(KeyCode::Left);
    assert_eq!(formulario(&prueba).alcance, AlcancePermisos::Directorios);
    // Esc cancela sin enviar nada.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(cambiar_permisos(&mut prueba).is_empty());
}

// ---------------------------------------------------------------- remoto

/// `Ctrl+S` sin recursivo envía `CambiarPermisos` directo: lo mixto queda
/// fuera de la máscara y cada ruta conserva el suyo. `Hecho` refresca.
#[test]
fn ctrl_s_envia_cambiar_permisos_con_la_mascara_del_mixto() {
    let mut prueba = con_remoto(120, 30);
    marcar(&mut prueba, &["app", "config.yaml", "main.py"]);
    prueba.tecla(KeyCode::Char('p'));
    // usuario · ejecutar: de mixto a sí. Grupo y otros siguen mixtos.
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Char(' '));
    prueba.enviados();
    ctrl_s(&mut prueba);
    assert!(
        prueba.app.dialogo.is_none(),
        "sin recursivo no hay confirmación"
    );
    let enviados = cambiar_permisos(&mut prueba);
    let [MensajeCliente::CambiarPermisos {
        peticion_id,
        host_id,
        rutas,
        modo,
        mascara,
        alcance,
    }] = enviados.as_slice()
    else {
        panic!("se esperaba un CambiarPermisos: {enviados:?}");
    };
    assert_eq!(*host_id, prueba.app.archivos.as_ref().unwrap().host_id);
    assert_eq!(
        rutas,
        &vec![
            "/srv/web/app".to_string(),
            "/srv/web/config.yaml".to_string(),
            "/srv/web/main.py".to_string(),
        ]
    );
    assert_eq!(*modo, 0o700);
    assert_eq!(*mascara, 0o777 & !0o055, "r y x de grupo y otros, fuera");
    assert_eq!(*alcance, None);

    prueba.servidor(MensajeServidor::Hecho {
        peticion_id: *peticion_id,
        detalle: Some("3 afectados".to_string()),
    });
    assert!(
        mensaje(&prueba).contains("3 afectados"),
        "{}",
        mensaje(&prueba)
    );
    assert!(listado_pedido(&mut prueba).is_some(), "refresca el remoto");
}

/// Un `Error` del servidor se dice y no refresca.
#[test]
fn un_error_del_servidor_se_dice() {
    let mut prueba = con_remoto(120, 30);
    ir_a(&mut prueba, "main.py");
    prueba.tecla(KeyCode::Char('p'));
    prueba.tecla(KeyCode::Char(' '));
    prueba.enviados();
    ctrl_s(&mut prueba);
    let peticion = match cambiar_permisos(&mut prueba).as_slice() {
        [MensajeCliente::CambiarPermisos {
            peticion_id, modo, ..
        }] => {
            assert_eq!(*modo, 0o244, "usuario · leer desmarcado");
            *peticion_id
        }
        otros => panic!("{otros:?}"),
    };
    prueba.servidor(MensajeServidor::Error {
        mensaje: "permiso denegado".to_string(),
        peticion_id: Some(peticion),
    });
    let mensaje_error = prueba.app.mensaje.as_ref().unwrap();
    assert!(mensaje_error.error);
    assert!(mensaje_error.texto.contains("permiso denegado"));
    assert!(listado_pedido(&mut prueba).is_none());
}

/// Recursivo remoto: un `ListarArbol` sin exclusiones por directorio, los
/// bloques se acumulan hasta `fin` y sale la confirmación con el recuento;
/// al confirmar, `CambiarPermisos` con el alcance.
#[test]
fn el_recursivo_remoto_cuenta_con_listar_arbol_y_confirma() {
    let mut prueba = con_remoto(120, 30);
    marcar(&mut prueba, &["app", "main.py"]);
    prueba.tecla(KeyCode::Char('p'));
    ir_al_campo(&mut prueba, FocoPermisos::Octal);
    escribir(&mut prueba, "0644");
    ir_al_campo(&mut prueba, FocoPermisos::Recursivo);
    prueba.tecla(KeyCode::Char(' '));
    prueba.enviados();
    ctrl_s(&mut prueba);
    let enviados = prueba.enviados();
    let [MensajeCliente::ListarArbol {
        peticion_id,
        ruta,
        exclusiones,
        usar_magiignore,
        exclusiones_extra,
        ..
    }] = enviados.as_slice()
    else {
        panic!("se esperaba un ListarArbol: {enviados:?}");
    };
    assert_eq!(ruta, "/srv/web/app", "solo los directorios");
    assert!(exclusiones.is_empty() && exclusiones_extra.is_empty());
    assert!(!usar_magiignore);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Permisos(DialogoPermisos::Contando(_)))
    ));
    assert!(prueba.texto().contains("PERMISOS · CONTANDO"));

    // Dos bloques: el primero sin `fin`.
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id: *peticion_id,
        entradas: vec![
            arbol("a.py", TipoEntrada::Fichero),
            arbol("lib", TipoEntrada::Directorio),
            arbol("lib/b.py", TipoEntrada::Fichero),
            arbol("enlace", TipoEntrada::Enlace),
        ],
        magiignore: None,
        excluidos: 0,
        fin: false,
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Permisos(DialogoPermisos::Contando(_)))
    ));
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id: *peticion_id,
        entradas: vec![arbol("lib/c.py", TipoEntrada::Fichero)],
        magiignore: None,
        excluidos: 0,
        fin: true,
    });
    let Some(Dialogo::Confirmar {
        titulo,
        lineas,
        peligro,
        ..
    }) = &prueba.app.dialogo
    else {
        panic!("sin confirmación:\n{}", prueba.texto());
    };
    assert_eq!(titulo, "PERMISOS RECURSIVOS");
    assert!(peligro);
    let lineas = lineas.join("\n");
    // main.py + 3 de dentro; app + lib.
    assert!(
        lineas.contains("Aplicará 0644 a 4 ficheros y 2 directorios."),
        "{lineas}"
    );
    assert!(lineas.contains("1 enlace sin tocar"), "{lineas}");
    assert!(
        lineas.contains("⚠ 0644 en directorios impide entrar en ellos"),
        "{lineas}"
    );

    prueba.tecla(KeyCode::Char('s'));
    let enviados = cambiar_permisos(&mut prueba);
    let [MensajeCliente::CambiarPermisos {
        rutas,
        modo,
        mascara,
        alcance,
        ..
    }] = enviados.as_slice()
    else {
        panic!("{enviados:?}");
    };
    assert_eq!(
        rutas,
        &vec!["/srv/web/app".to_string(), "/srv/web/main.py".to_string()]
    );
    assert_eq!((*modo, *mascara), (0o644, 0o7777), "4 dígitos escritos");
    assert_eq!(*alcance, Some(AlcancePermisos::Todo));
}

/// `Esc` durante el recuento: los bloques que lleguen después se descartan
/// y no aparece ninguna confirmación.
#[test]
fn cancelar_el_recuento_descarta_los_bloques() {
    let mut prueba = con_remoto(120, 30);
    ir_a(&mut prueba, "app");
    prueba.tecla(KeyCode::Char('p'));
    ir_al_campo(&mut prueba, FocoPermisos::Recursivo);
    prueba.tecla(KeyCode::Char(' '));
    prueba.enviados();
    ctrl_s(&mut prueba);
    let peticion_id = prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ListarArbol { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("ListarArbol");
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id,
        entradas: vec![arbol("a.py", TipoEntrada::Fichero)],
        magiignore: None,
        excluidos: 0,
        fin: true,
    });
    assert!(prueba.app.dialogo.is_none(), "{}", prueba.texto());
    assert!(prueba.app.peticiones_archivos.is_empty());
    assert!(cambiar_permisos(&mut prueba).is_empty());
}

/// El aviso ámbar sale con «todo» y algún trío con r sin x, y se va con
/// otro alcance o al devolver la x.
#[test]
fn el_aviso_ambar_sale_solo_con_todo_y_sin_x() {
    let mut prueba = con_remoto(120, 30);
    ir_a(&mut prueba, "app");
    prueba.tecla(KeyCode::Char('p'));
    // usuario, grupo y otros sin x: 0644.
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    for _ in 0..3 {
        prueba.tecla(KeyCode::Char(' '));
        prueba.tecla(KeyCode::Down);
    }
    assert_eq!(formulario(&prueba).modo.octal, "0644");
    const AVISO: &str = "⚠ 0644 en directorios impide entrar en ellos";
    assert!(
        !prueba.texto().contains(AVISO),
        "sin recursivo no hay aviso"
    );
    ir_al_campo(&mut prueba, FocoPermisos::Recursivo);
    prueba.tecla(KeyCode::Char(' '));
    assert!(prueba.texto().contains(AVISO), "{}", prueba.texto());
    let ambar = celdas_con_color(&prueba, prueba.app.tema.paleta.acento);
    assert!(ambar.contains(AVISO), "en ámbar: {ambar}");

    ir_al_campo(&mut prueba, FocoPermisos::Alcance);
    prueba.tecla(KeyCode::Right);
    assert!(
        !prueba.texto().contains(AVISO),
        "solo directorios: sin aviso"
    );
    prueba.tecla(KeyCode::Left);
    assert!(prueba.texto().contains(AVISO));
    // Devolver la x al usuario basta para entrar.
    ir_al_campo(&mut prueba, FocoPermisos::Rejilla);
    prueba.tecla(KeyCode::Up);
    prueba.tecla(KeyCode::Up);
    prueba.tecla(KeyCode::Char(' '));
    assert!(
        prueba.texto().contains("⚠ 0744"),
        "grupo y otros siguen sin x"
    );
}

// ---------------------------------------------------------------- local

/// En local, `Ctrl+S` aplica de verdad en un hilo, conserva los bits mixtos
/// de cada fichero y refresca el panel.
#[test]
fn el_chmod_local_se_aplica_de_verdad() {
    let mut prueba = con_archivos(120, 30, |hogar| {
        fichero(&hogar.join("a.txt"), 0o644);
        fichero(&hogar.join("b.txt"), 0o640);
    });
    let hogar = prueba.app.rutas.hogar.clone();
    marcar(&mut prueba, &["a.txt", "b.txt"]);
    prueba.tecla(KeyCode::Char('p'));
    assert_eq!(formulario(&prueba).modo.casillas.simbolico(), "rw-r--~--");
    // usuario · ejecutar.
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Char(' '));
    ctrl_s(&mut prueba);
    assert!(prueba.app.dialogo.is_none());
    esperar(&mut prueba, "el chmod local", |prueba| {
        mensaje(prueba).contains("cambiados")
    });
    assert_eq!(mensaje(&prueba), "permisos rwxr--~--: 2 cambiados");
    assert_eq!(modo_de(&hogar.join("a.txt")), 0o744);
    assert_eq!(
        modo_de(&hogar.join("b.txt")),
        0o740,
        "otros · leer, el suyo"
    );
    // El panel ya enseña el modo nuevo.
    let local = &prueba.app.archivos.as_ref().unwrap().local;
    let a = local
        .entradas
        .iter()
        .find(|entrada| entrada.nombre == "a.txt")
        .unwrap();
    assert_eq!(a.permisos.unwrap() & 0o7777, 0o744);
    assert!(cambiar_permisos(&mut prueba).is_empty(), "nada al servidor");
}

/// Recursivo local: recuento en un hilo, confirmación con el número de
/// ficheros y `chmod` solo de los ficheros, sin seguir enlaces.
#[test]
fn el_recursivo_local_cuenta_confirma_y_aplica() {
    let mut prueba = con_archivos(120, 30, |hogar| {
        let sitio = hogar.join("sitio");
        directorio(&sitio.join("css"), 0o755);
        fichero(&sitio.join("index.html"), 0o600);
        fichero(&sitio.join("css/app.css"), 0o600);
        fichero(&hogar.join("fuera.txt"), 0o600);
        std::os::unix::fs::symlink(hogar.join("fuera.txt"), sitio.join("enlace")).unwrap();
        directorio(&sitio, 0o755);
    });
    let hogar = prueba.app.rutas.hogar.clone();
    ir_a(&mut prueba, "sitio");
    prueba.tecla(KeyCode::Char('p'));
    ir_al_campo(&mut prueba, FocoPermisos::Octal);
    for _ in 0..4 {
        prueba.tecla(KeyCode::Backspace);
    }
    escribir(&mut prueba, "644");
    ir_al_campo(&mut prueba, FocoPermisos::Recursivo);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Right);
    prueba.tecla(KeyCode::Right);
    assert_eq!(formulario(&prueba).alcance, AlcancePermisos::Ficheros);
    ctrl_s(&mut prueba);
    esperar(&mut prueba, "la confirmación", |prueba| {
        matches!(prueba.app.dialogo, Some(Dialogo::Confirmar { .. }))
    });
    let texto = prueba.texto();
    assert!(texto.contains("PERMISOS RECURSIVOS"), "{texto}");
    assert!(texto.contains("Aplicará 644 a 2 ficheros."), "{texto}");
    assert!(texto.contains("Alcance: solo ficheros"), "{texto}");
    assert!(texto.contains("1 enlace sin tocar"), "{texto}");
    assert!(!texto.contains("⚠"), "{texto}");

    prueba.tecla(KeyCode::Char('s'));
    esperar(&mut prueba, "el chmod local", |prueba| {
        mensaje(prueba).contains("cambiados")
    });
    assert_eq!(
        mensaje(&prueba),
        "permisos 644: 2 cambiados · 1 enlace sin tocar"
    );
    assert_eq!(modo_de(&hogar.join("sitio/index.html")), 0o644);
    assert_eq!(modo_de(&hogar.join("sitio/css/app.css")), 0o644);
    assert_eq!(modo_de(&hogar.join("sitio/css")), 0o755, "solo ficheros");
    assert_eq!(
        modo_de(&hogar.join("fuera.txt")),
        0o600,
        "el enlace no se sigue"
    );
}

// ---------------------------------------------------------------- detalle

/// El detalle `i` dice propietario y grupo por nombre con el número, o solo
/// el número si el host no lo resuelve.
#[test]
fn el_detalle_dice_el_propietario_por_nombre() {
    let mut prueba = con_remoto(120, 30);
    ir_a(&mut prueba, "main.py");
    prueba.tecla(KeyCode::Char('i'));
    let texto = prueba.texto();
    assert!(texto.contains("Propietario deploy (1001)"), "{texto}");
    assert!(texto.contains("Grupo       deploy (1001)"), "{texto}");
    assert!(texto.contains("Permisos    -rw-r--r--"), "{texto}");

    prueba.tecla(KeyCode::Esc);
    ir_a(&mut prueba, "www.conf");
    prueba.tecla(KeyCode::Char('i'));
    let texto = prueba.texto();
    assert!(texto.contains("Propietario 33"), "{texto}");
    assert!(texto.contains("Grupo       33"), "{texto}");
    assert!(!texto.contains("33 (33)"), "{texto}");
}

/// En ASCII el diálogo y su aviso no pintan glifos Unicode.
#[test]
fn en_ascii_el_aviso_se_degrada() {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(80, 24, arnes::tema_ascii());
    super::vistas_g::abrir_dialogo(
        &mut prueba,
        Dialogo::Permisos(DialogoPermisos::Formulario(formulario_recursivo())),
    );
    let texto = prueba.texto();
    assert!(
        texto.contains("! 0644 en directorios impide entrar en ellos"),
        "{texto}"
    );
    assert!(texto.contains("(*) todo"), "{texto}");
}
