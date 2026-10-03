//! Edición remota (Fase 8): «¿subir cambios?», conflicto, aviso de propietario y fallo de la subida.
//! Instantáneas a 40×12, 80×24 y 200×60 (T45).
//!
//! El ciclo se recorre como lo haría el usuario: `E` sobre la fila del panel
//! remoto, el servidor contesta (`RutaTemporal`, `Stat`, `Hecho`, la cola) con
//! los `peticion_id` que la App envió, y la vuelta del editor se simula
//! escribiendo (o no) en el temporal y llamando a `editor_terminado`, que es
//! lo que hace el bucle tras suspender la TUI.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;
use filetime::FileTime;

use magi::app::edicion::{AccionEdicion, DialogoEdicion, Pregunta};
use magi::app::{AccionDialogo, Dialogo};
use magi::archivos::{Entrada, Lado, Marca, Propietario, TipoEntrada};
use magi::protocolo::{
    Direccion, ElementoTransferencia, EstadoTransferencia, EtiquetaTransferencia,
    InfoTransferencia, MensajeCliente, MensajeServidor, Politica,
};
use magi::ui::Vista;

use super::vistas_g::FabricaDialogo;
use crate::arnes::AppPrueba;
use crate::semilla::Sembrado;

/// 12 sep 2025 09:41:07 UTC.
const FECHA: i64 = 1_757_670_067;
const DIA: i64 = 86_400;
const RUTA_REMOTA: &str = "/var/www/cooperapp";
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];
/// Contenido de `main.py` en el host (14 bytes).
const ORIGINAL: &[u8] = b"print('hola')\n";

// ---------------------------------------------------------------- estado

fn dueno(uid: u32, nombre: &str) -> Propietario {
    Propietario {
        uid: Some(uid),
        gid: Some(uid),
        usuario: Some(nombre.to_string()),
        grupo: Some(nombre.to_string()),
    }
}

fn fichero(nombre: &str, tamano: u64, uid: u32, usuario: &str) -> Entrada {
    Entrada {
        nombre: nombre.to_string(),
        tipo: TipoEntrada::Fichero,
        tamano,
        mtime: FECHA,
        permisos: Some(0o100640),
        propietario: Some(dueno(uid, usuario)),
        enlace: None,
        marca: Marca::Ninguna,
    }
}

fn enlace(nombre: &str, destino: &str) -> Entrada {
    Entrada {
        tipo: TipoEntrada::Enlace,
        tamano: destino.len() as u64,
        permisos: Some(0o120777),
        enlace: Some(destino.to_string()),
        ..fichero(nombre, 0, 1001, "deploy")
    }
}

/// Listado de `/var/www/cooperapp`: ficheros del usuario de conexión
/// (`deploy`, 1001) y de `root`, uno sensible, uno grande, un directorio y
/// dos enlaces (a fichero y a directorio).
fn entradas_remotas() -> Vec<Entrada> {
    vec![
        Entrada {
            tipo: TipoEntrada::Directorio,
            ..fichero("static", 4_096, 1001, "deploy")
        },
        fichero("main.py", ORIGINAL.len() as u64, 1001, "deploy"),
        fichero("nginx.conf", 2_041, 0, "root"),
        fichero("servidor.key", 1_675, 1001, "deploy"),
        fichero("volcado.sql", 12 * 1024 * 1024, 1001, "deploy"),
        enlace("actual.py", "./main.py"),
        enlace("compartido", "../comun"),
    ]
}

/// El hogar local: un fichero y un directorio con fecha fija.
fn preparar_hogar(hogar: &Path) {
    fs::create_dir_all(hogar.join("proyectos")).unwrap();
    fs::write(hogar.join("notas.md"), b"# notas\n").unwrap();
    for nombre in ["proyectos", "notas.md"] {
        filetime::set_file_mtime(hogar.join(nombre), FileTime::from_unix_time(FECHA, 0)).unwrap();
    }
}

/// Archivos abierto con hetzner-01, el listado remoto de arriba y la
/// bienvenida dada (la ventana es el cliente 1: así reconoce su fila de la
/// cola).
fn con_archivos(cols: u16, filas: u16) -> (AppPrueba, Sembrado) {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(cols, filas);
    prueba.bienvenida(Vec::new());
    preparar_hogar(&prueba.app.rutas.hogar.clone());
    let host_id = sembrado.host("hetzner-01");
    prueba.app.abrir_archivos(Some(host_id));
    assert_eq!(prueba.app.vista, Vista::Archivos);
    prueba.servidor(MensajeServidor::SftpAbierto {
        host_id,
        dir_inicio: RUTA_REMOTA.to_string(),
        peticion_id: None,
        usuario_conexion: Some("deploy".to_string()),
        uid_conexion: Some(1001),
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
        entradas: entradas_remotas(),
        peticion_id,
    });
    (prueba, sembrado)
}

/// Pone el foco en `lado` con el cursor sobre `nombre`.
fn seleccionar(prueba: &mut AppPrueba, lado: Lado, nombre: &str) {
    let estado = prueba.app.archivos.as_mut().expect("Archivos abierto");
    estado.activo = lado;
    let panel = match lado {
        Lado::Local => &mut estado.local,
        Lado::Remoto => &mut estado.remoto,
    };
    let posicion = panel
        .visibles()
        .iter()
        .position(|indice| panel.entradas[*indice].nombre == nombre)
        .unwrap_or_else(|| panic!("«{nombre}» no está en el panel {lado:?}"));
    panel.seleccion = posicion;
}

/// `E` sobre `nombre` del panel remoto.
fn pulsar_e(prueba: &mut AppPrueba, nombre: &str) {
    seleccionar(prueba, Lado::Remoto, nombre);
    prueba.tecla(KeyCode::Char('E'));
}

/// El `DescargarTemporal` que se acaba de enviar: (ruta, peticion_id).
fn descarga_pedida(prueba: &mut AppPrueba) -> (String, u64) {
    let pedidas: Vec<(String, u64, bool)> = prueba
        .enviados()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::DescargarTemporal {
                ruta,
                peticion_id,
                edicion,
                ..
            } => Some((ruta, peticion_id, edicion)),
            _ => None,
        })
        .collect();
    assert_eq!(pedidas.len(), 1, "una sola descarga: {pedidas:?}");
    let (ruta, peticion_id, edicion) = pedidas.into_iter().next().unwrap();
    assert!(edicion, "el temporal de una edición va aparte");
    (ruta, peticion_id)
}

/// El servidor deja el temporal (600, con el mtime del remoto) y contesta.
fn llega_temporal(
    prueba: &mut AppPrueba,
    peticion_id: u64,
    nombre: &str,
    contenido: &[u8],
) -> PathBuf {
    let dir = prueba.app.rutas.dir_runtime().join("ediciones");
    fs::create_dir_all(&dir).unwrap();
    let temporal = dir.join(format!("{peticion_id}-{nombre}"));
    fs::write(&temporal, contenido).unwrap();
    fs::set_permissions(&temporal, fs::Permissions::from_mode(0o600)).unwrap();
    filetime::set_file_mtime(&temporal, FileTime::from_unix_time(FECHA, 0)).unwrap();
    prueba.servidor(MensajeServidor::RutaTemporal {
        ruta: temporal.display().to_string(),
        peticion_id,
    });
    temporal
}

/// Lo que haría el bucle: toma la petición de editor y, tras «editar» (o
/// no), llama a `editor_terminado`.
fn vuelve_el_editor(prueba: &mut AppPrueba, nuevo: Option<&[u8]>) {
    let peticion = prueba
        .app
        .peticion_editor
        .take()
        .expect("la App pide el editor");
    if let Some(nuevo) = nuevo {
        fs::write(&peticion.ruta, nuevo).unwrap();
    }
    prueba.app.editor_terminado(peticion, None);
    prueba.paso_en(std::time::Instant::now());
}

/// `main.py` bajado, editado con cambios y con «¿subir?» a la vista.
fn editado_con_cambios(prueba: &mut AppPrueba, nombre: &str) -> PathBuf {
    pulsar_e(prueba, nombre);
    let (_, peticion_id) = descarga_pedida(prueba);
    let temporal = llega_temporal(prueba, peticion_id, nombre, ORIGINAL);
    vuelve_el_editor(prueba, Some(b"print('adios')\n"));
    assert!(
        matches!(
            prueba.app.dialogo,
            Some(Dialogo::Edicion(DialogoEdicion::Subir { .. }))
        ),
        "con cambios se pregunta si subir"
    );
    temporal
}

/// El `StatRemoto` que se acaba de enviar: (ruta, peticion_id).
fn stat_pedido(prueba: &mut AppPrueba) -> (String, u64) {
    prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::StatRemoto {
                ruta, peticion_id, ..
            } => Some((ruta, peticion_id)),
            _ => None,
        })
        .expect("se verifica el remoto con StatRemoto")
}

/// `Stat` del remoto con este tamaño y mtime.
fn stat(prueba: &mut AppPrueba, peticion_id: u64, entrada: Option<Entrada>) {
    prueba.servidor(MensajeServidor::Stat {
        peticion_id,
        entrada,
    });
}

fn transferencias(enviados: &[MensajeCliente]) -> Vec<&MensajeCliente> {
    enviados
        .iter()
        .filter(|mensaje| matches!(mensaje, MensajeCliente::Transferir { .. }))
        .collect()
}

fn borrados(enviados: &[MensajeCliente]) -> Vec<String> {
    enviados
        .iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::BorrarTemporal { ruta } => Some(ruta.clone()),
            _ => None,
        })
        .collect()
}

/// `s` en «¿subir?» y el remoto sin cambios: devuelve el `peticion_id` del
/// `Transferir` enviado.
fn subir_sin_conflicto(prueba: &mut AppPrueba, entrada: Entrada) -> u64 {
    prueba.tecla(KeyCode::Char('s'));
    let (_, peticion_id) = stat_pedido(prueba);
    stat(prueba, peticion_id, Some(entrada));
    transferir_enviado(prueba)
}

fn transferir_enviado(prueba: &mut AppPrueba) -> u64 {
    prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::Transferir { peticion_id, .. } => peticion_id,
            _ => None,
        })
        .expect("se encola la subida")
}

/// Fila de la cola de la subida de la edición, como la difunde el servidor.
fn fila(
    peticion_id: u64,
    temporal: &Path,
    estado: EstadoTransferencia,
    error: Option<&str>,
) -> InfoTransferencia {
    InfoTransferencia {
        id: 7,
        host_id: 1,
        host_nombre: "hetzner-01".to_string(),
        direccion: Direccion::Subida,
        estado,
        origen: temporal.display().to_string(),
        destino: format!("{RUTA_REMOTA}/main.py"),
        es_directorio: false,
        ficheros_total: 1,
        ficheros_hechos: u32::from(estado == EstadoTransferencia::Hecha),
        omitidos: 0,
        bytes_total: 15,
        bytes_hechos: 15,
        fichero_actual: None,
        error: error.map(str::to_string),
        borrar_origen: false,
        solicitante: 1,
        creada_en: FECHA,
        terminada_en: estado.terminada().then_some(FECHA),
        peticion_id: Some(peticion_id),
        etiqueta: Some(EtiquetaTransferencia::Edicion),
        borrados: 0,
        borrados_total: 0,
    }
}

fn mensaje(prueba: &AppPrueba) -> String {
    prueba
        .app
        .mensaje
        .as_ref()
        .map(|mensaje| mensaje.texto.clone())
        .unwrap_or_default()
}

/// Hasta el conflicto: `main.py` cambió en el host (2 bytes más y un día
/// después) mientras se editaba.
fn hasta_conflicto(cols: u16, filas: u16) -> (AppPrueba, PathBuf) {
    let (mut prueba, _) = con_archivos(cols, filas);
    let temporal = editado_con_cambios(&mut prueba, "main.py");
    prueba.tecla(KeyCode::Char('s'));
    let (ruta, peticion_id) = stat_pedido(&mut prueba);
    assert_eq!(ruta, format!("{RUTA_REMOTA}/main.py"));
    let mut ahora = fichero("main.py", ORIGINAL.len() as u64 + 2, 1001, "deploy");
    ahora.mtime = FECHA + DIA;
    stat(&mut prueba, peticion_id, Some(ahora));
    (prueba, temporal)
}

// ---------------------------------------------------------------- ciclo

#[test]
fn e_sobre_un_fichero_remoto_pide_el_temporal_de_edicion_y_abre_el_editor() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "main.py");
    let (ruta, peticion_id) = descarga_pedida(&mut prueba);
    assert_eq!(ruta, format!("{RUTA_REMOTA}/main.py"));
    assert!(prueba.app.peticion_editor.is_none(), "aún no hay temporal");

    let temporal = llega_temporal(&mut prueba, peticion_id, "main.py", ORIGINAL);
    let peticion = prueba.app.peticion_editor.clone().expect("editor pedido");
    assert_eq!(peticion.ruta, temporal);
    assert!(peticion.edicion.is_some());
    let edicion = prueba.app.ediciones.vivas.values().next().unwrap();
    let bajado = edicion.bajado.clone().expect("datos del temporal fijados");
    assert_eq!((bajado.tamano, bajado.mtime), (14, FECHA));
    assert_eq!(edicion.objetivo.permisos_a_conservar(), Some(0o640));
}

#[test]
fn sin_cambios_borra_el_temporal_y_lo_dice() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "main.py");
    let (_, peticion_id) = descarga_pedida(&mut prueba);
    let temporal = llega_temporal(&mut prueba, peticion_id, "main.py", ORIGINAL);
    vuelve_el_editor(&mut prueba, None);
    let enviados = prueba.enviados();
    assert_eq!(borrados(&enviados), vec![temporal.display().to_string()]);
    assert!(transferencias(&enviados).is_empty());
    assert!(prueba.app.ediciones.vivas.is_empty());
    assert!(prueba.app.dialogo.is_none());
    assert!(
        mensaje(&prueba).contains("sin cambios"),
        "{}",
        mensaje(&prueba)
    );
}

#[test]
fn con_cambios_pregunta_y_subir_verifica_antes_con_stat_remoto() {
    let (mut prueba, _) = con_archivos(80, 24);
    editado_con_cambios(&mut prueba, "main.py");
    assert!(prueba.texto().contains("¿Subir cambios a"));
    assert!(prueba.enviados().is_empty(), "nada sale antes de contestar");
    prueba.tecla(KeyCode::Char('s'));
    let enviados = prueba.enviados();
    assert!(
        transferencias(&enviados).is_empty(),
        "no se sube sin pasar por «verificando»"
    );
    assert!(enviados.iter().any(|mensaje| matches!(
        mensaje,
        MensajeCliente::StatRemoto { ruta, .. } if ruta == "/var/www/cooperapp/main.py"
    )));
}

/// AC: Dado un fichero que cambia en el host mientras se edita, cuando se
/// elige subir, entonces aparece el conflicto con los dos tamaños y fechas y
/// nada se sobrescribe sin elegir «sobrescribir».
#[test]
fn conflicto_con_los_dos_tamanos_y_fechas_y_nada_se_sube_sin_s() {
    let (mut prueba, temporal) = hasta_conflicto(80, 24);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::Conflicto { .. }))
    ));
    let texto = prueba.texto();
    assert!(texto.contains("EL FICHERO CAMBIÓ EN EL HOST"), "{texto}");
    assert!(
        texto.contains("14 bytes") && texto.contains("16 bytes"),
        "{texto}"
    );
    assert!(
        texto.contains("12 sep 2025 09:41:07") && texto.contains("13 sep 2025 09:41:07"),
        "{texto}"
    );
    // Ni `↵` ni otras teclas sobrescriben.
    for tecla in [
        KeyCode::Enter,
        KeyCode::Char('S'),
        KeyCode::Char('x'),
        KeyCode::Tab,
    ] {
        prueba.tecla(tecla);
        assert!(matches!(
            prueba.app.dialogo,
            Some(Dialogo::Edicion(DialogoEdicion::Conflicto { .. }))
        ));
    }
    assert!(transferencias(&prueba.enviados()).is_empty());
    // `esc` vuelve a preguntar; `s` verifica otra vez y vuelve el conflicto.
    prueba.tecla(KeyCode::Esc);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::Subir { .. }))
    ));
    assert!(transferencias(&prueba.enviados()).is_empty());
    prueba.tecla(KeyCode::Char('s'));
    let (_, peticion_id) = stat_pedido(&mut prueba);
    stat(&mut prueba, peticion_id, None);
    assert!(prueba.texto().contains("ya no existe"));
    assert!(transferencias(&prueba.enviados()).is_empty());

    // Solo `s` sobrescribe: sobre lo que hay, con los permisos del original.
    prueba.tecla(KeyCode::Char('s'));
    let enviados = prueba.enviados();
    let subidas = transferencias(&enviados);
    assert_eq!(subidas.len(), 1);
    let MensajeCliente::Transferir {
        direccion,
        elementos,
        politica,
        borrar_origen,
        etiqueta,
        peticion_id,
        ..
    } = subidas[0]
    else {
        unreachable!()
    };
    assert_eq!(*direccion, Direccion::Subida);
    assert_eq!(*politica, Politica::Sobrescribir);
    assert!(!borrar_origen, "el temporal nunca es un «mover»");
    assert_eq!(*etiqueta, Some(EtiquetaTransferencia::Edicion));
    assert!(peticion_id.is_some());
    assert_eq!(
        elementos,
        &vec![ElementoTransferencia {
            origen: temporal.display().to_string(),
            destino: "/var/www/cooperapp/main.py".to_string(),
            bytes: 15,
            es_directorio: false,
            politica: Some(Politica::Sobrescribir),
            permisos: Some(0o640),
        }]
    );
    assert!(borrados(&enviados).is_empty());
}

#[test]
fn hecha_borra_el_temporal_y_termina_la_edicion() {
    let (mut prueba, _) = con_archivos(80, 24);
    let temporal = editado_con_cambios(&mut prueba, "main.py");
    let peticion_id = subir_sin_conflicto(&mut prueba, fichero("main.py", 14, 1001, "deploy"));
    prueba.servidor(MensajeServidor::Hecho {
        peticion_id,
        detalle: None,
    });
    for estado in [EstadoTransferencia::EnCola, EstadoTransferencia::EnCurso] {
        prueba.servidor(MensajeServidor::Transferencias {
            lista: vec![fila(peticion_id, &temporal, estado, None)],
        });
        assert!(borrados(&prueba.enviados()).is_empty(), "{estado:?}");
    }
    // La fila de otra ventana con el mismo id no es la nuestra.
    let mut ajena = fila(peticion_id, &temporal, EstadoTransferencia::Hecha, None);
    ajena.solicitante = 2;
    prueba.servidor(MensajeServidor::Transferencias { lista: vec![ajena] });
    assert!(borrados(&prueba.enviados()).is_empty());

    prueba.servidor(MensajeServidor::Transferencias {
        lista: vec![fila(
            peticion_id,
            &temporal,
            EstadoTransferencia::Hecha,
            None,
        )],
    });
    assert_eq!(
        borrados(&prueba.enviados()),
        vec![temporal.display().to_string()]
    );
    assert!(prueba.app.ediciones.vivas.is_empty());
    assert!(mensaje(&prueba).contains("cambios subidos"));
}

/// AC: Dado un host sin permiso de escritura en el fichero, cuando falla la
/// subida, entonces el temporal sigue existiendo y el mensaje dice dónde
/// está.
#[test]
fn un_error_de_subida_conserva_el_temporal_y_dice_su_ruta() {
    let (mut prueba, _) = con_archivos(200, 60);
    let temporal = editado_con_cambios(&mut prueba, "main.py");
    let peticion_id = subir_sin_conflicto(&mut prueba, fichero("main.py", 14, 1001, "deploy"));
    prueba.servidor(MensajeServidor::Transferencias {
        lista: vec![fila(
            peticion_id,
            &temporal,
            EstadoTransferencia::Error,
            Some("permiso denegado"),
        )],
    });
    assert!(temporal.exists(), "el temporal sigue en el disco");
    assert!(borrados(&prueba.enviados()).is_empty());
    let ruta = temporal.display().to_string();
    assert!(mensaje(&prueba).contains(&ruta), "{}", mensaje(&prueba));
    let texto = prueba.texto();
    assert!(texto.contains("LA SUBIDA FALLÓ"), "{texto}");
    assert!(texto.contains("permiso denegado"), "{texto}");
    assert!(texto.contains(&ruta), "{texto}");

    // `r` vuelve a verificar antes de subir otra vez.
    prueba.tecla(KeyCode::Char('r'));
    let (_, stat_id) = stat_pedido(&mut prueba);
    stat(
        &mut prueba,
        stat_id,
        Some(fichero("main.py", 14, 1001, "deploy")),
    );
    let otra = transferir_enviado(&mut prueba);
    assert_ne!(otra, peticion_id);

    // Cancelada: también se conserva; `esc` cierra sin borrar nada.
    prueba.servidor(MensajeServidor::Transferencias {
        lista: vec![fila(otra, &temporal, EstadoTransferencia::Cancelada, None)],
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::Fallida { .. }))
    ));
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(borrados(&prueba.enviados()).is_empty());
    assert!(temporal.exists());
    assert!(mensaje(&prueba).contains(&ruta));
    assert!(prueba.app.ediciones.vivas.is_empty());
}

#[test]
fn un_rechazo_al_encolar_tambien_conserva_el_temporal() {
    let (mut prueba, _) = con_archivos(80, 24);
    let temporal = editado_con_cambios(&mut prueba, "main.py");
    let peticion_id = subir_sin_conflicto(&mut prueba, fichero("main.py", 14, 1001, "deploy"));
    prueba.servidor(MensajeServidor::Error {
        mensaje: "ese host no tiene canal SFTP".to_string(),
        peticion_id: Some(peticion_id),
    });
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::Fallida { .. }))
    ));
    assert!(borrados(&prueba.enviados()).is_empty());
    assert!(temporal.exists());
}

#[test]
fn el_servidor_caido_no_borra_el_temporal_de_una_subida() {
    let (mut prueba, _) = con_archivos(80, 24);
    let temporal = editado_con_cambios(&mut prueba, "main.py");
    subir_sin_conflicto(&mut prueba, fichero("main.py", 14, 1001, "deploy"));
    prueba.evento(magi::app::Evento::ServidorCaido);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::ServidorCaido { .. })
    ));
    assert!(borrados(&prueba.enviados()).is_empty());
    assert!(temporal.exists());
    // Tras contestar al servidor caído vuelve el fallo, con la ruta.
    prueba.tecla(KeyCode::Char('n'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::Fallida { .. }))
    ));
    let texto = prueba.texto();
    assert!(texto.contains(&temporal.display().to_string()), "{texto}");
    // `c` guarda la copia local y solo entonces se borra el temporal.
    prueba.tecla(KeyCode::Char('c'));
    let copia = copia_en(&prueba.app.rutas.hogar, "main.py");
    assert_eq!(fs::read(&copia).unwrap(), b"print('adios')\n");
}

#[test]
fn no_subir_deja_elegir_y_descartar_pide_confirmacion() {
    let (mut prueba, _) = con_archivos(80, 24);
    let temporal = editado_con_cambios(&mut prueba, "main.py");
    prueba.tecla(KeyCode::Char('n'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::NoSubir { .. }))
    ));
    // `esc` vuelve a preguntar; `n` otra vez a elegir.
    prueba.tecla(KeyCode::Esc);
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::Subir { .. }))
    ));
    prueba.tecla(KeyCode::Char('n'));
    prueba.tecla(KeyCode::Char('d'));
    assert!(prueba.texto().contains("DESCARTAR CAMBIOS"));
    // «No» vuelve a elegir sin borrar nada.
    prueba.tecla(KeyCode::Char('n'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::NoSubir { .. }))
    ));
    assert!(borrados(&prueba.enviados()).is_empty());
    prueba.tecla(KeyCode::Char('d'));
    prueba.tecla(KeyCode::Char('s'));
    assert_eq!(
        borrados(&prueba.enviados()),
        vec![temporal.display().to_string()]
    );
    assert!(prueba.app.ediciones.vivas.is_empty());
}

/// La copia local `<nombre>.magi-<AAAAMMDD-HHMMSS>` que se guardó en `dir`.
fn copia_en(dir: &Path, nombre: &str) -> PathBuf {
    let prefijo = format!("{nombre}.magi-");
    let copias: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|elemento| elemento.path())
        .filter(|ruta| {
            ruta.file_name()
                .and_then(|nombre| nombre.to_str())
                .is_some_and(|nombre| nombre.starts_with(&prefijo))
        })
        .collect();
    assert_eq!(
        copias.len(),
        1,
        "una copia en {}: {copias:?}",
        dir.display()
    );
    copias.into_iter().next().unwrap()
}

#[test]
fn la_copia_local_va_al_panel_local_con_600_y_entonces_se_borra_el_temporal() {
    let (mut prueba, _) = con_archivos(80, 24);
    let temporal = editado_con_cambios(&mut prueba, "main.py");
    prueba.tecla(KeyCode::Char('n'));
    prueba.tecla(KeyCode::Char('c'));
    let hogar = prueba.app.rutas.hogar.clone();
    let copia = copia_en(&hogar, "main.py");
    let nombre = copia.file_name().unwrap().to_string_lossy().to_string();
    let sello = nombre.trim_start_matches("main.py.magi-");
    assert_eq!(sello.len(), 15, "AAAAMMDD-HHMMSS: {nombre}");
    assert_eq!(sello.as_bytes()[8], b'-');
    assert_eq!(fs::read(&copia).unwrap(), b"print('adios')\n");
    let modo = fs::metadata(&copia).unwrap().permissions().mode();
    assert_eq!(modo & 0o777, 0o600);
    assert_eq!(
        borrados(&prueba.enviados()),
        vec![temporal.display().to_string()]
    );
    // El panel local ya la enseña.
    let local = &prueba.app.archivos.as_ref().unwrap().local;
    assert!(local
        .entradas
        .iter()
        .any(|entrada| entrada.nombre == nombre));
}

#[test]
fn el_aviso_de_propietario_va_antes_de_subir() {
    let (mut prueba, _) = con_archivos(80, 24);
    editado_con_cambios(&mut prueba, "nginx.conf");
    prueba.tecla(KeyCode::Char('s'));
    let (_, peticion_id) = stat_pedido(&mut prueba);
    let mut igual = fichero("nginx.conf", 14, 0, "root");
    igual.mtime = FECHA;
    stat(&mut prueba, peticion_id, Some(igual));
    assert!(transferencias(&prueba.enviados()).is_empty());
    let texto = prueba.texto();
    assert!(texto.contains("CAMBIA EL PROPIETARIO"), "{texto}");
    assert!(texto.contains("pertenecerá a deploy (1001)"), "{texto}");
    // «No» vuelve a elegir; «sí» sube.
    prueba.tecla(KeyCode::Char('n'));
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Edicion(DialogoEdicion::NoSubir { .. }))
    ));
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Char('s'));
    let (_, peticion_id) = stat_pedido(&mut prueba);
    let mut igual = fichero("nginx.conf", 14, 0, "root");
    igual.mtime = FECHA;
    stat(&mut prueba, peticion_id, Some(igual));
    prueba.tecla(KeyCode::Char('s'));
    assert_eq!(transferencias(&prueba.enviados()).len(), 1);
}

#[test]
fn permisos_y_propietario_son_los_del_remoto_justo_antes_de_subir() {
    let (mut prueba, _) = con_archivos(80, 24);
    editado_con_cambios(&mut prueba, "main.py");
    prueba.tecla(KeyCode::Char('s'));
    let (_, peticion_id) = stat_pedido(&mut prueba);
    // Mientras se editaba alguien hizo `chmod 600` y `chown root` (no cambia
    // el mtime): se avisa del dueño de ahora y se conservan sus permisos.
    let mut ahora = fichero("main.py", 14, 0, "root");
    ahora.permisos = Some(0o100600);
    stat(&mut prueba, peticion_id, Some(ahora));
    assert!(prueba.texto().contains("CAMBIA EL PROPIETARIO"));
    prueba.tecla(KeyCode::Char('s'));
    let enviados = prueba.enviados();
    let MensajeCliente::Transferir { elementos, .. } = transferencias(&enviados)[0] else {
        unreachable!()
    };
    assert_eq!(elementos[0].permisos, Some(0o600));
}

#[test]
fn el_aviso_de_sensibles_se_aplica_al_subir_una_edicion() {
    let (mut prueba, _) = con_archivos(80, 24);
    editado_con_cambios(&mut prueba, "servidor.key");
    prueba.tecla(KeyCode::Char('s'));
    let (_, peticion_id) = stat_pedido(&mut prueba);
    stat(
        &mut prueba,
        peticion_id,
        Some(fichero("servidor.key", 14, 1001, "deploy")),
    );
    let texto = prueba.texto();
    assert!(texto.contains("FICHERO SENSIBLE"), "{texto}");
    assert!(texto.contains("*.key"), "{texto}");
    assert!(transferencias(&prueba.enviados()).is_empty());
    prueba.tecla(KeyCode::Char('s'));
    assert_eq!(transferencias(&prueba.enviados()).len(), 1);
}

#[test]
fn editar_dos_veces_el_mismo_fichero_avisa() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "main.py");
    descarga_pedida(&mut prueba);
    pulsar_e(&mut prueba, "main.py");
    assert!(mensaje(&prueba).contains("ya lo estás editando"));
    assert!(prueba.enviados().is_empty(), "ni otra descarga ni nada");
    // Por el enlace se llega al mismo fichero: también avisa.
    pulsar_e(&mut prueba, "actual.py");
    let (ruta, peticion_id) = stat_pedido(&mut prueba);
    assert_eq!(ruta, "/var/www/cooperapp/main.py");
    stat(
        &mut prueba,
        peticion_id,
        Some(fichero("main.py", 14, 1001, "deploy")),
    );
    assert!(mensaje(&prueba).contains("ya lo estás editando"));
    assert!(prueba.enviados().is_empty());
}

#[test]
fn e_sobre_un_enlace_a_fichero_edita_su_destino() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "actual.py");
    let (ruta, peticion_id) = stat_pedido(&mut prueba);
    assert_eq!(ruta, "/var/www/cooperapp/main.py");
    stat(
        &mut prueba,
        peticion_id,
        Some(fichero("main.py", 14, 0, "root")),
    );
    let (ruta, _) = descarga_pedida(&mut prueba);
    assert_eq!(ruta, "/var/www/cooperapp/main.py", "se baja el destino");
    // Permisos y dueño son los del destino, no los del enlace.
    let edicion = prueba.app.ediciones.vivas.values().next().unwrap();
    assert_eq!(edicion.objetivo.permisos_a_conservar(), Some(0o640));
    assert_eq!(
        edicion.objetivo.nuevo_propietario().as_deref(),
        Some("deploy (1001)")
    );
}

#[test]
fn e_sobre_un_directorio_o_enlace_a_directorio_no_hace_nada_y_lo_dice() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "static");
    assert!(
        mensaje(&prueba).contains("directorio"),
        "{}",
        mensaje(&prueba)
    );
    assert!(prueba.enviados().is_empty());
    pulsar_e(&mut prueba, "..");
    assert!(mensaje(&prueba).contains("directorio"));
    assert!(prueba.enviados().is_empty());

    pulsar_e(&mut prueba, "compartido");
    let (ruta, peticion_id) = stat_pedido(&mut prueba);
    assert_eq!(ruta, "/var/www/comun");
    stat(
        &mut prueba,
        peticion_id,
        Some(Entrada {
            tipo: TipoEntrada::Directorio,
            ..fichero("comun", 4_096, 1001, "deploy")
        }),
    );
    assert!(
        mensaje(&prueba).contains("directorio"),
        "{}",
        mensaje(&prueba)
    );
    assert!(prueba.enviados().is_empty());
    assert!(prueba.app.ediciones.vivas.is_empty());

    // En el panel local, igual.
    seleccionar(&mut prueba, Lado::Local, "proyectos");
    prueba.tecla(KeyCode::Char('E'));
    assert!(mensaje(&prueba).contains("directorio"));
    assert!(prueba.app.peticion_editor.is_none());
}

#[test]
fn e_sobre_un_fichero_local_abre_el_editor_y_refresca_el_panel() {
    let (mut prueba, _) = con_archivos(80, 24);
    seleccionar(&mut prueba, Lado::Local, "notas.md");
    prueba.tecla(KeyCode::Char('E'));
    let peticion = prueba.app.peticion_editor.clone().expect("editor pedido");
    assert_eq!(peticion.ruta, prueba.app.rutas.hogar.join("notas.md"));
    assert!(peticion.edicion.is_none());
    assert!(
        prueba.enviados().is_empty(),
        "lo local no pasa por el servidor"
    );
    vuelve_el_editor(&mut prueba, Some(b"# notas\n\nmucho mas largo\n"));
    let local = &prueba.app.archivos.as_ref().unwrap().local;
    let notas = local
        .entradas
        .iter()
        .find(|entrada| entrada.nombre == "notas.md")
        .unwrap();
    assert_eq!(notas.tamano, 25, "el panel enseña el tamaño nuevo");
}

#[test]
fn un_fichero_grande_pide_confirmacion_antes_de_bajar() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "volcado.sql");
    assert!(matches!(
        prueba.app.dialogo,
        Some(Dialogo::Confirmar {
            accion: AccionDialogo::Edicion(AccionEdicion::Bajar(_)),
            ..
        })
    ));
    assert!(prueba.enviados().is_empty());
    prueba.tecla(KeyCode::Char('n'));
    assert!(prueba.enviados().is_empty());
    assert!(prueba.app.ediciones.vivas.is_empty());
    pulsar_e(&mut prueba, "volcado.sql");
    prueba.tecla(KeyCode::Char('s'));
    let (ruta, _) = descarga_pedida(&mut prueba);
    assert_eq!(ruta, "/var/www/cooperapp/volcado.sql");
}

#[test]
fn lo_que_parece_binario_se_confirma_y_con_no_se_borra_el_temporal() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "main.py");
    let (_, peticion_id) = descarga_pedida(&mut prueba);
    let temporal = llega_temporal(&mut prueba, peticion_id, "main.py", b"\x7fELF\0\0\x01");
    assert!(prueba.app.peticion_editor.is_none());
    assert!(prueba.texto().contains("PARECE BINARIO"));
    prueba.tecla(KeyCode::Char('n'));
    assert_eq!(
        borrados(&prueba.enviados()),
        vec![temporal.display().to_string()]
    );
    assert!(prueba.app.ediciones.vivas.is_empty());

    // Con «sí», el editor.
    pulsar_e(&mut prueba, "main.py");
    let (_, peticion_id) = descarga_pedida(&mut prueba);
    llega_temporal(&mut prueba, peticion_id, "main.py", b"\0");
    prueba.tecla(KeyCode::Char('s'));
    assert!(prueba.app.peticion_editor.is_some());
}

#[test]
fn si_ya_no_esta_en_archivos_no_se_abre_el_editor_y_el_temporal_se_borra() {
    let (mut prueba, _) = con_archivos(80, 24);
    pulsar_e(&mut prueba, "main.py");
    let (_, peticion_id) = descarga_pedida(&mut prueba);
    prueba.tecla(KeyCode::Char('q'));
    assert_ne!(prueba.app.vista, Vista::Archivos);
    let temporal = llega_temporal(&mut prueba, peticion_id, "main.py", ORIGINAL);
    assert!(prueba.app.peticion_editor.is_none());
    assert_eq!(
        borrados(&prueba.enviados()),
        vec![temporal.display().to_string()],
        "recién bajado y sin tocar: se puede borrar"
    );
    assert!(prueba.app.ediciones.vivas.is_empty());
}

// ---------------------------------------------------------------- pintado

/// El ciclo se recorre a 80×24 (por debajo del mínimo de Archivos la vista
/// no atiende `E`) y el diálogo se pinta después a cada tamaño.
#[test]
fn instantaneas_del_conflicto() {
    let (mut prueba, _) = hasta_conflicto(80, 24);
    for (cols, filas) in TAMANOS {
        prueba.pasar_por(cols, filas);
        prueba.instantanea(&format!("edicion_conflicto_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_de_subir_cambios() {
    let (mut prueba, _) = con_archivos(80, 24);
    editado_con_cambios(&mut prueba, "main.py");
    for (cols, filas) in TAMANOS {
        prueba.pasar_por(cols, filas);
        prueba.instantanea(&format!("edicion_subir_{cols}x{filas}"));
    }
}

#[test]
fn el_conflicto_se_recoloca_en_la_secuencia_y_conserva_la_decision() {
    let (mut prueba, _) = hasta_conflicto(200, 60);
    for (cols, filas) in [(80, 24), (40, 12), (59, 20), (200, 60)] {
        prueba.pasar_por(cols, filas);
        let texto = prueba.texto();
        assert!(
            texto.contains("EL FICHERO CAMBIÓ EN EL HOST"),
            "{cols}×{filas}:\n{texto}"
        );
        assert!(matches!(
            prueba.app.dialogo,
            Some(Dialogo::Edicion(DialogoEdicion::Conflicto { .. }))
        ));
    }
    assert!(transferencias(&prueba.enviados()).is_empty());
}

// ---------------------------------------------------------------- fábricas

fn subir() -> Dialogo {
    Dialogo::Edicion(DialogoEdicion::Subir {
        edicion: 1,
        destino: "hetzner-01:/etc/nginx/sites-available/cooperapp".to_string(),
    })
}

fn no_subir() -> Dialogo {
    Dialogo::Edicion(DialogoEdicion::NoSubir {
        edicion: 1,
        temporal: "/run/user/1000/magi/ediciones/12-cooperapp".to_string(),
        dir_local: "/home/hector/proyectos/cooperapp".to_string(),
    })
}

fn conflicto(ahora: Option<(u64, i64)>) -> Dialogo {
    Dialogo::Edicion(DialogoEdicion::Conflicto {
        edicion: 1,
        destino: "hetzner-01:/etc/nginx/sites-available/cooperapp".to_string(),
        antes: (2_041, FECHA),
        ahora,
    })
}

fn fallida() -> Dialogo {
    Dialogo::Edicion(DialogoEdicion::Fallida {
        edicion: 1,
        destino: "hetzner-01:/etc/nginx/sites-available/cooperapp".to_string(),
        error: "permiso denegado".to_string(),
        temporal: "/run/user/1000/magi/ediciones/12-cooperapp".to_string(),
    })
}

fn descartar() -> Dialogo {
    Dialogo::Edicion(DialogoEdicion::Pregunta(Box::new(Pregunta {
        edicion: 1,
        titulo: "DESCARTAR CAMBIOS".to_string(),
        lineas: vec![
            "Se borrará el temporal con tus cambios a cooperapp:".to_string(),
            "  /run/user/1000/magi/ediciones/12-cooperapp".to_string(),
            "No se puede deshacer.".to_string(),
        ],
        peligro: true,
        si: ("descartar", AccionEdicion::Descartar(1)),
        no: (
            "volver",
            AccionEdicion::Volver(Box::new(DialogoEdicion::Subir {
                edicion: 1,
                destino: String::new(),
            })),
        ),
    })))
}

/// Diálogos de esta vista para las pruebas comunes de `vistas_g` (enteros a
/// 40×12 y sin glifos Unicode en ASCII).
pub(crate) fn dialogos() -> Vec<(&'static str, FabricaDialogo)> {
    vec![
        ("SUBIR CAMBIOS", Box::new(subir)),
        ("TUS CAMBIOS SIN SUBIR", Box::new(no_subir)),
        (
            "EL FICHERO CAMBI",
            Box::new(|| conflicto(Some((2_112, FECHA + 300)))),
        ),
        ("EL FICHERO CAMBI", Box::new(|| conflicto(None))),
        ("LA SUBIDA FALL", Box::new(fallida)),
        ("DESCARTAR CAMBIOS", Box::new(descartar)),
    ]
}
