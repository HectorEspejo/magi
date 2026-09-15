use std::collections::HashMap;
use std::fs;

use magi::almacen::Almacen;
use magi::modelo::{DatosHost, IdentidadRef, Origen};
use magi::sshconfig::{exportar, importar, parser};

#[test]
fn ida_y_vuelta_reproduce_el_inventario() {
    let dir = tempfile::tempdir().unwrap();
    let origen = Almacen::abrir_en_memoria().unwrap();
    let grupo = origen.crear_grupo("4d3 · producción").unwrap();
    origen
        .crear_host(
            &DatosHost {
                nombre: "hetzner-01".to_string(),
                direccion: "95.217.x.x".to_string(),
                usuario: Some("root".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
    let salto = origen
        .crear_host(
            &DatosHost {
                nombre: "vps-intermedio".to_string(),
                direccion: "49.12.x.x".to_string(),
                usuario: Some("hector".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
    let datos_destino = DatosHost {
        nombre: "backup-nas".to_string(),
        direccion: "10.0.0.12".to_string(),
        puerto: 2200,
        usuario: Some("admin".to_string()),
        identidad_ref: IdentidadRef::Fichero("~/.ssh/4d3-ed25519".to_string()),
        salto_host_id: Some(salto),
        multiplexar: true,
        keepalive_seg: Some(45),
        opciones_extra: "ForwardAgent yes\nLocalForward 5432 127.0.0.1:5432".to_string(),
        etiquetas: vec!["backup".to_string()],
        grupo_id: Some(grupo),
    };
    origen.crear_host(&datos_destino, Origen::Manual).unwrap();

    let exportado = exportar::exportar(origen.conexion(), dir.path()).unwrap();
    assert!(exportado.ruta.exists());
    assert_eq!(exportado.hosts, 3);

    let destino = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&exportado.ruta, dir.path()).unwrap();
    assert!(
        analisis.omitidos.is_empty(),
        "omitidos: {:?}",
        analisis.omitidos
    );
    let resumen = importar::aplicar(destino.conexion(), &analisis, &HashMap::new()).unwrap();
    assert_eq!(resumen.importados, 3);
    assert_eq!(resumen.sobrescritos, 0);

    let hosts = destino.listar_hosts().unwrap();
    let original = origen.listar_hosts().unwrap();
    for host_original in &original {
        let host = hosts
            .iter()
            .find(|host| host.nombre == host_original.nombre)
            .unwrap_or_else(|| panic!("falta {}", host_original.nombre));
        assert_eq!(host.direccion, host_original.direccion);
        assert_eq!(host.puerto, host_original.puerto);
        assert_eq!(host.usuario, host_original.usuario);
        assert_eq!(host.identidad_ref, host_original.identidad_ref);
        assert_eq!(host.multiplexar, host_original.multiplexar);
        assert_eq!(host.keepalive_seg, host_original.keepalive_seg);
        assert_eq!(
            host.opciones_extra, host_original.opciones_extra,
            "opciones extra de {}",
            host.nombre
        );
        assert_eq!(host.salto_nombre, host_original.salto_nombre);
        assert_eq!(host.origen, Origen::SshConfig);
        assert_eq!(host.grupo_nombre.as_deref(), Some("~/.ssh/config"));
    }
}

#[test]
fn conexion_no_se_resuelve_si_el_salto_no_existe() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("config");
    fs::write(
        &ruta,
        "Host destino\n    HostName 10.0.0.1\n    ProxyJump fantasma\n",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&ruta, dir.path()).unwrap();
    let resumen = importar::aplicar(almacen.conexion(), &analisis, &HashMap::new()).unwrap();
    assert_eq!(resumen.importados, 1);
    let host = almacen.listar_hosts().unwrap().remove(0);
    assert_eq!(host.salto_host_id, None);
    assert_eq!(host.opciones_extra, "ProxyJump fantasma");
}

#[test]
fn parser_omite_comodines_match_y_bloques_multipatron() {
    let texto = "\
# comentario
Host *
    User root

Host uno dos
    HostName 1.2.3.4

Match host *.example.com
    User nadie

Host simple
    HostName 5.6.7.8
    User admin
";
    let parseo = parser::parsear(texto);
    assert_eq!(parseo.len(), 3);
    assert_eq!(parseo[2].patrones, vec!["simple"]);
    let analisis = importar::analizar(&parser::Parseo {
        bloques: parseo,
        avisos: Vec::new(),
    });
    assert_eq!(analisis.candidatos.len(), 1);
    assert_eq!(analisis.omitidos.len(), 2);
}

#[test]
fn include_de_un_nivel_ignora_magi_config() {
    let dir = tempfile::tempdir().unwrap();
    let principal = dir.path().join("config");
    let incluido = dir.path().join("extra.conf");
    let magi_config = dir.path().join("magi_config");
    fs::write(
        &principal,
        format!(
            "Include {}\nInclude {}\nHost propio\n    HostName 1.1.1.1\n",
            incluido.display(),
            magi_config.display()
        ),
    )
    .unwrap();
    fs::write(&incluido, "Host incluido\n    HostName 2.2.2.2\n").unwrap();
    fs::write(&magi_config, "Host no-importar\n    HostName 3.3.3.3\n").unwrap();
    let parseo = parser::leer_config(&principal, dir.path()).unwrap();
    let nombres: Vec<String> = parseo
        .bloques
        .iter()
        .map(|bloque| bloque.patrones[0].clone())
        .collect();
    assert!(nombres.contains(&"propio".to_string()));
    assert!(nombres.contains(&"incluido".to_string()));
    assert!(!nombres.contains(&"no-importar".to_string()));
}

#[test]
fn incluir_include_crea_copia_y_lo_pone_primero() {
    let dir = tempfile::tempdir().unwrap();
    let ssh = dir.path().join(".ssh");
    fs::create_dir_all(&ssh).unwrap();
    let config = ssh.join("config");
    fs::write(&config, "Host uno\n    HostName 1.2.3.4\n").unwrap();
    let magi_config = ssh.join("magi_config");
    fs::write(&magi_config, "# generado\n").unwrap();

    assert!(!exportar::comprobar_include(&config, &magi_config));
    let copia = exportar::insertar_include(&config, &magi_config).unwrap();
    assert!(copia.exists());
    assert!(copia
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("config.bak-"));
    let texto = fs::read_to_string(&config).unwrap();
    assert!(texto.starts_with("Include ~/.ssh/magi_config\n"));
    assert!(exportar::comprobar_include(&config, &magi_config));
}

#[test]
fn conflicto_omitido_por_defecto_y_sobrescrito_con_decision() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("config");
    fs::write(
        &ruta,
        "Host existente\n    HostName 9.9.9.9\n    User nuevo\n",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    almacen
        .crear_host(
            &DatosHost {
                nombre: "existente".to_string(),
                direccion: "1.1.1.1".to_string(),
                usuario: Some("viejo".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
    let analisis = importar::analizar_fichero(&ruta, dir.path()).unwrap();
    let conflictos = importar::conflictos(almacen.conexion(), &analisis).unwrap();
    assert_eq!(conflictos, vec!["existente".to_string()]);

    let mut decisiones = HashMap::new();
    decisiones.insert("existente".to_string(), false);
    let resumen = importar::aplicar(almacen.conexion(), &analisis, &decisiones).unwrap();
    assert_eq!(resumen.importados, 0);
    assert_eq!(resumen.omitidos.len(), 1);
    assert_eq!(
        almacen.listar_hosts().unwrap()[0].direccion,
        "1.1.1.1".to_string()
    );

    decisiones.insert("existente".to_string(), true);
    let resumen = importar::aplicar(almacen.conexion(), &analisis, &decisiones).unwrap();
    assert_eq!(resumen.sobrescritos, 1);
    let host = almacen.listar_hosts().unwrap().remove(0);
    assert_eq!(host.direccion, "9.9.9.9");
    assert_eq!(host.usuario.as_deref(), Some("nuevo"));
    assert_eq!(host.origen, Origen::SshConfig);
}
