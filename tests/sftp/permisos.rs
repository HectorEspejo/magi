//! Fase 8 · `StatRemoto` y `CambiarPermisos` contra el `sftp-server` real; nombres de propietario.

#[allow(unused_imports)]
use super::*;

use std::os::unix::fs::MetadataExt as _;

use magi::protocolo::AlcancePermisos;

/// Nombre de un id en un fichero con el formato de `/etc/passwd` o
/// `/etc/group` del sistema que hace de host (el de la propia máquina).
fn nombre_en(fichero: &str, id: u32) -> Option<String> {
    let texto = std::fs::read_to_string(fichero).ok()?;
    texto.lines().find_map(|linea| {
        let campos: Vec<&str> = linea.split(':').collect();
        (campos.len() > 2 && campos[2].parse::<u32>().ok() == Some(id))
            .then(|| campos[0].to_string())
    })
}

/// uid de un usuario en el `/etc/passwd` de la máquina.
fn uid_en_passwd(usuario: &str) -> Option<u32> {
    let texto = std::fs::read_to_string("/etc/passwd").ok()?;
    texto.lines().find_map(|linea| {
        let campos: Vec<&str> = linea.split(':').collect();
        (campos.len() > 2 && campos[0] == usuario)
            .then(|| campos[2].parse().ok())
            .flatten()
    })
}

fn modo(ruta: &Path) -> u32 {
    std::fs::symlink_metadata(ruta)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777
}

fn poner_modo(ruta: &Path, modo: u32) {
    std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(modo)).unwrap();
}

async fn stat(
    montaje: &mut Montaje,
    ruta: &Path,
    peticion_id: u64,
) -> Option<magi::archivos::Entrada> {
    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::StatRemoto {
            peticion_id,
            host_id,
            ruta: ruta.display().to_string(),
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Stat { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::Stat {
            peticion_id: id,
            entrada,
        } => {
            assert_eq!(id, peticion_id);
            entrada
        }
        otro => panic!("se esperaba Stat, llegó {otro:?}"),
    }
}

/// Envía `CambiarPermisos` y devuelve el detalle del `Hecho`.
async fn cambiar(
    montaje: &mut Montaje,
    rutas: &[&Path],
    modo: u32,
    mascara: u32,
    alcance: Option<AlcancePermisos>,
    peticion_id: u64,
) -> String {
    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::CambiarPermisos {
            peticion_id,
            host_id,
            rutas: rutas
                .iter()
                .map(|ruta| ruta.display().to_string())
                .collect(),
            modo,
            mascara,
            alcance,
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Hecho { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::Hecho {
            peticion_id: id,
            detalle,
        } => {
            assert_eq!(id, peticion_id);
            detalle.expect("CambiarPermisos contesta con detalle")
        }
        otro => panic!("se esperaba Hecho, llegó {otro:?}"),
    }
}

/// Árbol de prueba: `sitio/` con un fichero, un subdirectorio con otro y un
/// fichero suelto al lado. Todo con modos conocidos.
fn arbol_de_prueba(remoto: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let sitio = remoto.join("sitio");
    let sub = sitio.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let index = sitio.join("index.html");
    let dentro = sub.join("app.js");
    let suelto = remoto.join("suelto.txt");
    for fichero in [&index, &dentro, &suelto] {
        std::fs::write(fichero, b"x").unwrap();
        poner_modo(fichero, 0o644);
    }
    poner_modo(&sub, 0o755);
    poner_modo(&sitio, 0o755);
    (sitio, sub, index, dentro, suelto)
}

#[tokio::test]
async fn stat_remoto_devuelve_la_entrada_con_uid_y_nombre() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let fichero = remoto.join("main.py");
    std::fs::write(&fichero, b"doce bytes!!").unwrap();
    poner_modo(&fichero, 0o640);
    montaje.abrir_sftp().await;

    let entrada = stat(&mut montaje, &fichero, 1)
        .await
        .expect("el fichero existe");
    assert_eq!(entrada.nombre, "main.py");
    assert_eq!(entrada.tipo, TipoEntrada::Fichero);
    assert_eq!(entrada.tamano, 12);
    assert!(entrada.mtime > 1_600_000_000);
    assert_eq!(entrada.permisos.map(|modo| modo & 0o7777), Some(0o640));
    let metadatos = std::fs::metadata(&fichero).unwrap();
    let propietario = entrada.propietario.expect("el remoto da uid y gid");
    assert_eq!(propietario.uid, Some(metadatos.uid()));
    assert_eq!(propietario.gid, Some(metadatos.gid()));
    assert_eq!(
        propietario.usuario,
        nombre_en("/etc/passwd", metadatos.uid()),
        "el nombre sale del /etc/passwd del host"
    );
    assert_eq!(propietario.grupo, nombre_en("/etc/group", metadatos.gid()));

    // Un enlace se describe a sí mismo (lstat), no a su destino.
    std::os::unix::fs::symlink(&fichero, remoto.join("atajo")).unwrap();
    let enlace = stat(&mut montaje, &remoto.join("atajo"), 2)
        .await
        .expect("el enlace existe");
    assert_eq!(enlace.tipo, TipoEntrada::Enlace);
}

#[tokio::test]
async fn stat_remoto_de_una_ruta_inexistente_no_trae_entrada() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    montaje.abrir_sftp().await;
    assert_eq!(stat(&mut montaje, &remoto.join("no-existe"), 3).await, None);
}

/// El usuario de conexión y su uid viajan en `SftpAbierto`, y el listado
/// trae usuario y grupo por nombre.
#[tokio::test]
async fn el_listado_y_la_apertura_traen_los_nombres_de_propietario() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::write(remoto.join("a.txt"), b"a").unwrap();
    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::AbrirSftp {
            host_id,
            peticion_id: Some(5),
            no_interactivo: false,
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::SftpAbierto { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::SftpAbierto {
            dir_inicio,
            usuario_conexion,
            uid_conexion,
            peticion_id,
            ..
        } => {
            assert_eq!(peticion_id, Some(5));
            // El host de las pruebas se conecta como «hector».
            assert_eq!(usuario_conexion.as_deref(), Some("hector"));
            let esperado = uid_en_passwd("hector")
                .or_else(|| std::fs::metadata(&dir_inicio).ok().map(|m| m.uid()));
            assert_eq!(uid_conexion, esperado);
        }
        otro => panic!("se esperaba SftpAbierto, llegó {otro:?}"),
    }

    let entradas = montaje.listar(&remoto.display().to_string(), 6).await;
    let metadatos = std::fs::metadata(remoto.join("a.txt")).unwrap();
    let propietario = entradas[0].propietario.clone().expect("con propietario");
    assert_eq!(
        propietario.usuario,
        nombre_en("/etc/passwd", metadatos.uid())
    );
    assert_eq!(propietario.grupo, nombre_en("/etc/group", metadatos.gid()));
    if propietario.usuario.is_some() {
        assert!(
            propietario
                .usuario_legible()
                .ends_with(&format!("({})", metadatos.uid())),
            "nombre con el número entre paréntesis: {}",
            propietario.usuario_legible()
        );
    }
}

#[tokio::test]
async fn cambiar_permisos_sin_alcance_solo_toca_las_rutas_y_lo_anota() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let (sitio, sub, index, dentro, suelto) = arbol_de_prueba(&montaje.remoto());
    montaje.abrir_sftp().await;

    let detalle = cambiar(&mut montaje, &[&sitio, &suelto], 0o750, 0o777, None, 10).await;
    assert_eq!(detalle, "2 afectados");
    assert_eq!(modo(&sitio), 0o750);
    assert_eq!(modo(&suelto), 0o750);
    assert_eq!(modo(&sub), 0o755, "sin alcance no se recorre");
    assert_eq!(modo(&index), 0o644);
    assert_eq!(modo(&dentro), 0o644);

    // `Hecho` sale tras la barrera: la fila ya está en REGISTRO.
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let entradas = almacen
        .listar_registro(
            &magi::registro::FiltroRegistro {
                texto: "permisos_cambiados".to_string(),
                tipos: vec![],
            },
            10,
            0,
        )
        .unwrap();
    let fila = entradas
        .iter()
        .find(|entrada| entrada.tipo == magi::registro::PERMISOS_CAMBIADOS)
        .expect("el cambio de permisos se anota en REGISTRO");
    assert_eq!(fila.host_id, Some(montaje.host_id));
    assert_eq!(fila.resultado, magi::modelo::ResultadoRegistro::Ok);
    assert!(fila.detalle.contains("modo 0750"), "{}", fila.detalle);
    assert!(fila.detalle.contains("2 afectados"), "{}", fila.detalle);
    assert!(
        fila.detalle.contains(&sitio.display().to_string()),
        "{}",
        fila.detalle
    );
}

#[tokio::test]
async fn cambiar_permisos_con_alcance_todo_recorre_el_arbol() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let (sitio, sub, index, dentro, suelto) = arbol_de_prueba(&montaje.remoto());
    montaje.abrir_sftp().await;

    let detalle = cambiar(
        &mut montaje,
        &[&sitio],
        0o750,
        0o777,
        Some(AlcancePermisos::Todo),
        11,
    )
    .await;
    assert_eq!(detalle, "4 afectados");
    for ruta in [&sitio, &sub, &index, &dentro] {
        assert_eq!(modo(ruta), 0o750, "{}", ruta.display());
    }
    assert_eq!(modo(&suelto), 0o644, "fuera del árbol no se toca");
}

/// Con alcance «todo», un modo sin `x` deja los directorios sin poder entrar
/// en ellos: aun así llega al fondo (primero el contenido, después el
/// directorio).
#[tokio::test]
async fn todo_sin_x_en_los_directorios_llega_igualmente_al_fondo() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let (sitio, sub, index, dentro, _) = arbol_de_prueba(&montaje.remoto());
    montaje.abrir_sftp().await;

    let detalle = cambiar(
        &mut montaje,
        &[&sitio],
        0o600,
        0o777,
        Some(AlcancePermisos::Todo),
        12,
    )
    .await;
    // Se leen los modos devolviendo cada directorio a 755 para poder entrar
    // en él (y para que el directorio temporal se pueda borrar después).
    let modo_sitio = modo(&sitio);
    poner_modo(&sitio, 0o755);
    let modo_sub = modo(&sub);
    poner_modo(&sub, 0o755);
    let modos = vec![modo_sitio, modo_sub];
    let ficheros: Vec<u32> = [&index, &dentro].iter().map(|ruta| modo(ruta)).collect();
    assert_eq!(detalle, "4 afectados");
    assert_eq!(modos, vec![0o600, 0o600]);
    assert_eq!(ficheros, vec![0o600, 0o600]);
}

#[tokio::test]
async fn cambiar_permisos_solo_directorios() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let (sitio, sub, index, dentro, suelto) = arbol_de_prueba(&montaje.remoto());
    montaje.abrir_sftp().await;

    let detalle = cambiar(
        &mut montaje,
        &[&sitio, &suelto],
        0o700,
        0o777,
        Some(AlcancePermisos::Directorios),
        13,
    )
    .await;
    assert_eq!(detalle, "2 afectados");
    assert_eq!(modo(&sitio), 0o700);
    assert_eq!(modo(&sub), 0o700);
    assert_eq!(modo(&index), 0o644);
    assert_eq!(modo(&dentro), 0o644);
    assert_eq!(
        modo(&suelto),
        0o644,
        "un fichero marcado no es un directorio"
    );
}

#[tokio::test]
async fn cambiar_permisos_solo_ficheros() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let (sitio, sub, index, dentro, suelto) = arbol_de_prueba(&montaje.remoto());
    montaje.abrir_sftp().await;

    let detalle = cambiar(
        &mut montaje,
        &[&sitio, &suelto],
        0o600,
        0o777,
        Some(AlcancePermisos::Ficheros),
        14,
    )
    .await;
    assert_eq!(detalle, "3 afectados");
    assert_eq!(modo(&sitio), 0o755, "el directorio marcado no se toca");
    assert_eq!(modo(&sub), 0o755);
    assert_eq!(modo(&index), 0o600);
    assert_eq!(modo(&dentro), 0o600);
    assert_eq!(modo(&suelto), 0o600);
}

/// `setstat` sigue los enlaces: tocarlos cambiaría su destino, que puede
/// estar fuera de lo marcado. Se saltan y se cuentan.
#[tokio::test]
async fn cambiar_permisos_no_toca_los_enlaces_ni_su_destino() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let (sitio, _, index, _, suelto) = arbol_de_prueba(&remoto);
    let fuera = remoto.join("fuera");
    std::fs::create_dir(&fuera).unwrap();
    poner_modo(&fuera, 0o755);
    std::os::unix::fs::symlink(&suelto, sitio.join("a-fichero")).unwrap();
    std::os::unix::fs::symlink(&fuera, sitio.join("a-directorio")).unwrap();
    std::os::unix::fs::symlink(&suelto, remoto.join("marcado")).unwrap();
    montaje.abrir_sftp().await;

    let detalle = cambiar(
        &mut montaje,
        &[&sitio, &remoto.join("marcado")],
        0o700,
        0o777,
        Some(AlcancePermisos::Todo),
        15,
    )
    .await;
    assert_eq!(detalle, "4 afectados · 3 enlaces sin tocar");
    assert_eq!(modo(&index), 0o700);
    assert_eq!(modo(&suelto), 0o644, "el destino del enlace no se toca");
    assert_eq!(modo(&fuera), 0o755, "ni el directorio al que apunta");
    assert!(std::fs::symlink_metadata(sitio.join("a-fichero"))
        .unwrap()
        .file_type()
        .is_symlink());
}

/// Un error en una ruta no detiene el resto: `Hecho` lleva los afectados y
/// los errores, y la anotación queda como error.
#[tokio::test]
async fn una_ruta_inexistente_no_para_las_demas() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let (_, _, _, _, suelto) = arbol_de_prueba(&remoto);
    let fantasma = remoto.join("fantasma.txt");
    montaje.abrir_sftp().await;

    let detalle = cambiar(&mut montaje, &[&fantasma, &suelto], 0o600, 0o777, None, 16).await;
    assert_eq!(modo(&suelto), 0o600, "la ruta buena se cambia igualmente");
    assert!(detalle.starts_with("1 afectado · 1 error: "), "{detalle}");
    assert!(
        detalle.contains(&format!("{}: la ruta no existe", fantasma.display())),
        "{detalle}"
    );

    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let entradas = almacen
        .listar_registro(
            &magi::registro::FiltroRegistro {
                texto: "permisos_cambiados".to_string(),
                tipos: vec![],
            },
            10,
            0,
        )
        .unwrap();
    let fila = entradas
        .iter()
        .find(|entrada| entrada.tipo == magi::registro::PERMISOS_CAMBIADOS)
        .expect("se anota aunque haya errores");
    assert_eq!(fila.resultado, magi::modelo::ResultadoRegistro::Error);
    assert!(fila.detalle.contains("1 error"), "{}", fila.detalle);
}

/// La máscara dice qué bits se escriben: el resto (incluidos setuid, setgid y
/// sticky) se conserva ruta a ruta.
#[tokio::test]
async fn la_mascara_solo_toca_sus_bits() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let uno = remoto.join("uno.txt");
    let dos = remoto.join("dos.txt");
    let especial = remoto.join("especial");
    for fichero in [&uno, &dos, &especial] {
        std::fs::write(fichero, b"x").unwrap();
    }
    poner_modo(&uno, 0o640);
    poner_modo(&dos, 0o600);
    poner_modo(&especial, 0o4755);
    montaje.abrir_sftp().await;

    // Solo «otros»: r para todos, sin tocar usuario ni grupo.
    cambiar(&mut montaje, &[&uno, &dos], 0o004, 0o007, None, 17).await;
    assert_eq!(modo(&uno), 0o644);
    assert_eq!(modo(&dos), 0o604);

    // Las casillas (9 bits) conservan el setuid.
    cambiar(&mut montaje, &[&especial], 0o700, 0o777, None, 18).await;
    assert_eq!(modo(&especial), 0o4700);
    // Un octal de 4 dígitos lo quita.
    cambiar(&mut montaje, &[&especial], 0o0700, 0o7777, None, 19).await;
    assert_eq!(modo(&especial), 0o700);
}
