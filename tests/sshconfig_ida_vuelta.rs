use std::collections::HashMap;
use std::fs;

use magi::almacen::Almacen;
use magi::modelo::{DatosHost, DatosTunel, IdentidadRef, Origen, TipoTunel, Tunel};
use magi::sshconfig::{exportar, importar, parser};

/// Bloque `Host <nombre>` del texto generado, sin la cabecera ni el bloque
/// siguiente.
fn bloque_de(texto: &str, nombre: &str) -> String {
    let mut lineas = Vec::new();
    let mut dentro = false;
    for linea in texto.lines() {
        if let Some(resto) = linea.strip_prefix("Host ") {
            if dentro {
                break;
            }
            dentro = resto.trim() == nombre;
            continue;
        }
        if dentro {
            lineas.push(linea);
        }
    }
    lineas.join("\n")
}

/// Comprueba que un túnel está en la lista con esos datos.
fn exigir_tunel<'a>(
    tuneles: &'a [Tunel],
    nombre: &str,
    tipo: TipoTunel,
    escucha: &str,
    destino: Option<&str>,
) -> &'a Tunel {
    let tunel = tuneles
        .iter()
        .find(|tunel| tunel.nombre == nombre)
        .unwrap_or_else(|| panic!("falta el túnel {nombre}"));
    assert_eq!(tunel.tipo, tipo, "tipo de {nombre}");
    assert_eq!(tunel.escucha, escucha, "escucha de {nombre}");
    assert_eq!(tunel.destino.as_deref(), destino, "destino de {nombre}");
    assert!(tunel.automatico, "{nombre} debería nacer automático");
    tunel
}

#[test]
fn sobrescribir_al_importar_conserva_servicios_y_etiquetas() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config"),
        "Host uno\n  HostName 10.0.0.1\n  User hector\n",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&dir.path().join("config"), dir.path()).unwrap();
    importar::aplicar(almacen.conexion(), &analisis, &HashMap::new()).unwrap();
    let host = almacen
        .listar_hosts()
        .unwrap()
        .into_iter()
        .find(|host| host.nombre == "uno")
        .unwrap();
    let datos = DatosHost {
        nombre: "uno".to_string(),
        direccion: "10.0.0.1".to_string(),
        usuario: Some("hector".to_string()),
        servicios: "nginx.service\npostgresql".to_string(),
        etiquetas: vec!["prod".to_string()],
        grupo_id: host.grupo_id,
        ..DatosHost::default()
    };
    almacen.actualizar_host(host.id, &datos).unwrap();

    let mut decisiones = HashMap::new();
    decisiones.insert("uno".to_string(), true);
    importar::aplicar(almacen.conexion(), &analisis, &decisiones).unwrap();
    let host = almacen.obtener_host(host.id).unwrap();
    assert_eq!(host.servicios, "nginx.service\npostgresql");
    assert_eq!(host.etiquetas, vec!["prod"]);
}

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
        opciones_extra: "ForwardAgent yes".to_string(),
        servicios: "nginx\npostgresql".to_string(),
        etiquetas: vec!["backup".to_string()],
        grupo_id: Some(grupo),
    };
    let backup_nas = origen.crear_host(&datos_destino, Origen::Manual).unwrap();
    // Los reenvíos ya no viven en `opciones_extra`: son túneles del host.
    for (nombre, tipo, escucha, destino) in [
        (
            "local-5432",
            TipoTunel::Local,
            "127.0.0.1:5432",
            "127.0.0.1:5432",
        ),
        (
            "remoto-9000",
            TipoTunel::Remoto,
            "127.0.0.1:9000",
            "127.0.0.1:9000",
        ),
        (
            "socks-1080",
            TipoTunel::Dinamico,
            "127.0.0.1:1080",
            "127.0.0.1:1080",
        ),
    ] {
        let destino = (!matches!(tipo, TipoTunel::Dinamico)).then(|| destino.to_string());
        origen
            .crear_tunel(&DatosTunel {
                host_id: backup_nas,
                nombre: nombre.to_string(),
                tipo,
                escucha: escucha.to_string(),
                destino,
                automatico: true,
            })
            .unwrap();
    }

    let exportado = exportar::exportar(origen.conexion(), dir.path()).unwrap();
    assert!(exportado.ruta.exists());
    assert_eq!(exportado.hosts, 3);

    // El bloque del host lleva sus tres reenvíos, con el `bind` por defecto
    // omitido, y no los deja sueltos en las opciones extra.
    let texto = fs::read_to_string(&exportado.ruta).unwrap();
    let bloque = bloque_de(&texto, "backup-nas");
    assert!(
        bloque.contains("    LocalForward 5432 127.0.0.1:5432"),
        "bloque: {bloque}"
    );
    assert!(
        bloque.contains("    RemoteForward 9000 127.0.0.1:9000"),
        "bloque: {bloque}"
    );
    assert!(
        bloque.contains("    DynamicForward 1080"),
        "bloque: {bloque}"
    );
    assert!(bloque.contains("    ForwardAgent yes"), "bloque: {bloque}");

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

    // Los reenvíos del host sobreviven a la ida y vuelta, y ya no aparecen en
    // las opciones extra de nadie.
    let tuneles = destino.listar_tuneles().unwrap();
    assert_eq!(tuneles.len(), 3, "túneles importados: {tuneles:?}");
    for tunel in &tuneles {
        assert_eq!(tunel.host_nombre, "backup-nas");
    }
    exigir_tunel(
        &tuneles,
        "local-5432",
        TipoTunel::Local,
        "127.0.0.1:5432",
        Some("127.0.0.1:5432"),
    );
    exigir_tunel(
        &tuneles,
        "remoto-9000",
        TipoTunel::Remoto,
        "127.0.0.1:9000",
        Some("127.0.0.1:9000"),
    );
    exigir_tunel(
        &tuneles,
        "socks-1080",
        TipoTunel::Dinamico,
        "127.0.0.1:1080",
        None,
    );
    for host in &hosts {
        let extra = host.opciones_extra.to_lowercase();
        for directiva in ["localforward", "remoteforward", "dynamicforward"] {
            assert!(
                !extra.contains(directiva),
                "{} conserva «{directiva}» en opciones extra: {}",
                host.nombre,
                host.opciones_extra
            );
        }
    }
}

#[test]
fn los_reenvios_de_ssh_config_van_a_tuneles() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("config");
    fs::write(
        &ruta,
        "\
Host tunelero
    HostName 10.0.0.9
    User hector
    LocalForward 5432 10.0.0.5:5432
    RemoteForward 9000 127.0.0.1:9000
    DynamicForward 1080

Host sin-reenvios
    HostName 10.0.0.10
",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&ruta, dir.path()).unwrap();
    assert!(
        analisis.omitidos.is_empty(),
        "omitidos: {:?}",
        analisis.omitidos
    );
    let candidato = analisis
        .candidatos
        .iter()
        .find(|candidato| candidato.nombre == "tunelero")
        .unwrap();
    assert_eq!(candidato.tuneles.len(), 3);
    let resumen = importar::aplicar(almacen.conexion(), &analisis, &HashMap::new()).unwrap();
    assert_eq!(resumen.importados, 2);

    let tuneles = almacen.listar_tuneles().unwrap();
    assert_eq!(tuneles.len(), 3);
    for tunel in &tuneles {
        assert_eq!(tunel.host_nombre, "tunelero");
    }
    exigir_tunel(
        &tuneles,
        "local-5432",
        TipoTunel::Local,
        "127.0.0.1:5432",
        Some("10.0.0.5:5432"),
    );
    exigir_tunel(
        &tuneles,
        "remoto-9000",
        TipoTunel::Remoto,
        "127.0.0.1:9000",
        Some("127.0.0.1:9000"),
    );
    exigir_tunel(
        &tuneles,
        "socks-1080",
        TipoTunel::Dinamico,
        "127.0.0.1:1080",
        None,
    );

    let host = almacen
        .listar_hosts()
        .unwrap()
        .into_iter()
        .find(|host| host.nombre == "tunelero")
        .unwrap();
    assert_eq!(host.opciones_extra, "");
}

/// Un reenvío que no se entiende se conserva como opción extra, igual que
/// antes de la Fase 5, en vez de perderse.
#[test]
fn un_reenvio_que_no_se_entiende_se_queda_en_opciones_extra() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("config");
    fs::write(
        &ruta,
        "Host raro\n    HostName 10.0.0.11\n    LocalForward 5432\n",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&ruta, dir.path()).unwrap();
    importar::aplicar(almacen.conexion(), &analisis, &HashMap::new()).unwrap();
    assert!(almacen.listar_tuneles().unwrap().is_empty());
    let host = almacen.listar_hosts().unwrap().remove(0);
    assert_eq!(host.opciones_extra, "LocalForward 5432");
}

/// Lo que pide el checklist: los reenvíos de los tres tipos sobreviven a
/// `importar(exportar(hosts))`.
#[test]
fn ida_y_vuelta_de_los_tres_tipos() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("config");
    fs::write(
        &ruta,
        "\
Host tunelero
    HostName 10.0.0.9
    User hector
    LocalForward 5432 10.0.0.5:5432
    RemoteForward 9000 127.0.0.1:9000
    DynamicForward 1080
",
    )
    .unwrap();
    let primero = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&ruta, dir.path()).unwrap();
    importar::aplicar(primero.conexion(), &analisis, &HashMap::new()).unwrap();
    assert_eq!(primero.listar_tuneles().unwrap().len(), 3);

    // Se exporta y se vuelve a importar el resultado en otro inventario.
    let exportado = exportar::exportar(primero.conexion(), dir.path()).unwrap();
    let segundo = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&exportado.ruta, dir.path()).unwrap();
    assert!(
        analisis.omitidos.is_empty(),
        "omitidos: {:?}",
        analisis.omitidos
    );
    let resumen = importar::aplicar(segundo.conexion(), &analisis, &HashMap::new()).unwrap();
    assert_eq!(resumen.importados, 1);

    let tuneles = segundo.listar_tuneles().unwrap();
    assert_eq!(tuneles.len(), 3, "túneles reimportados: {tuneles:?}");
    for tunel in &tuneles {
        assert_eq!(tunel.host_nombre, "tunelero");
    }
    exigir_tunel(
        &tuneles,
        "local-5432",
        TipoTunel::Local,
        "127.0.0.1:5432",
        Some("10.0.0.5:5432"),
    );
    exigir_tunel(
        &tuneles,
        "remoto-9000",
        TipoTunel::Remoto,
        "127.0.0.1:9000",
        Some("127.0.0.1:9000"),
    );
    exigir_tunel(
        &tuneles,
        "socks-1080",
        TipoTunel::Dinamico,
        "127.0.0.1:1080",
        None,
    );

    let host = segundo.listar_hosts().unwrap().remove(0);
    assert_eq!(host.opciones_extra, "");
}

#[test]
fn reimportar_el_mismo_fichero_no_repite_los_tuneles() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("config");
    fs::write(
        &ruta,
        "Host tunelero\n    HostName 10.0.0.9\n    LocalForward 5432 10.0.0.5:5432\n",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&ruta, dir.path()).unwrap();
    importar::aplicar(almacen.conexion(), &analisis, &HashMap::new()).unwrap();

    // El host ya existe: se sobrescribe y el túnel, que ya está, se ignora
    // sin abortar la importación.
    let mut decisiones = HashMap::new();
    decisiones.insert("tunelero".to_string(), true);
    let resumen = importar::aplicar(almacen.conexion(), &analisis, &decisiones).unwrap();
    assert_eq!(resumen.sobrescritos, 1);
    assert_eq!(almacen.listar_tuneles().unwrap().len(), 1);
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

/// Un reenvío que no se puede convertir en túnel (su nombre derivado del tipo y
/// el puerto ya está ocupado por otro del mismo bloque) no se pierde: vuelve a
/// `opciones_extra` tal cual estaba y la importación lo avisa.
#[test]
fn un_reenvio_que_no_cabe_en_tuneles_vuelve_a_opciones_extra() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config"),
        "Host choque\n  HostName 10.0.0.1\n\
         \x20 LocalForward 5432 db1:5432\n\
         \x20 LocalForward 0.0.0.0:5432 db2:5432\n",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let analisis = importar::analizar_fichero(&dir.path().join("config"), dir.path()).unwrap();
    let resumen = importar::aplicar(almacen.conexion(), &analisis, &HashMap::new()).unwrap();

    let host = almacen
        .listar_hosts()
        .unwrap()
        .into_iter()
        .find(|host| host.nombre == "choque")
        .expect("el host se importa");
    // El primero es túnel; el segundo (mismo tipo y puerto, otro bind) no cabe
    // con el nombre derivado, así que se queda como línea y se avisa.
    let tuneles = almacen.tuneles_de_host(host.id).unwrap();
    assert_eq!(tuneles.len(), 1, "{tuneles:?}");
    assert_eq!(tuneles[0].nombre, "local-5432");
    assert_eq!(tuneles[0].escucha, "127.0.0.1:5432");
    assert_eq!(
        host.opciones_extra, "LocalForward 0.0.0.0:5432 db2:5432",
        "la línea que no cabe no puede perderse"
    );
    assert_eq!(resumen.avisos.len(), 1, "{:?}", resumen.avisos);
    assert!(resumen.avisos[0].contains("LocalForward 0.0.0.0:5432"));
}

/// Reimportar el mismo fichero no repite túneles ni ensucia `opciones_extra`.
#[test]
fn reimportar_el_mismo_fichero_no_cambia_nada() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("config");
    fs::write(
        &ruta,
        "Host ida\n  HostName 10.0.0.1\n\
         \x20 LocalForward 5432 db:5432\n\
         \x20 DynamicForward 1080\n",
    )
    .unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    for _ in 0..2 {
        let analisis = importar::analizar_fichero(&ruta, dir.path()).unwrap();
        let resumen = importar::aplicar(almacen.conexion(), &analisis, &HashMap::new()).unwrap();
        assert!(resumen.avisos.is_empty(), "{:?}", resumen.avisos);
    }
    let host = almacen
        .listar_hosts()
        .unwrap()
        .into_iter()
        .find(|host| host.nombre == "ida")
        .expect("el host está");
    assert_eq!(almacen.tuneles_de_host(host.id).unwrap().len(), 2);
    assert!(host.opciones_extra.is_empty(), "{:?}", host.opciones_extra);
}

/// ¿Hay un `ssh` de verdad en el sistema? Sin él, la prueba de abajo se salta.
fn hay_ssh() -> bool {
    std::process::Command::new("ssh").arg("-V").output().is_ok()
}

/// El `magi_config` tiene que ser un `ssh_config` que `ssh` acepte: se incluye
/// desde `~/.ssh/config`, así que una sola línea mala dejaría al usuario sin
/// `ssh` para ningún host. Se comprueba con el `ssh` real.
#[test]
fn el_magi_config_generado_lo_acepta_ssh() {
    if !hay_ssh() {
        eprintln!("[AVISO] no hay ssh en el sistema: se salta la validación del fichero");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen
        .crear_host(
            &DatosHost {
                nombre: "prueba".to_string(),
                direccion: "10.0.0.1".to_string(),
                usuario: Some("hector".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
    // Un túnel de cada tipo, incluido el local con puerto 0 (que `ssh` no sabe
    // escribir) y el remoto con 0 (que sí: lo elige el host).
    for (nombre, tipo, escucha, destino) in [
        (
            "normal",
            TipoTunel::Local,
            "127.0.0.1:5432",
            Some("10.0.0.5:5432"),
        ),
        (
            "efimero",
            TipoTunel::Local,
            "127.0.0.1:0",
            Some("10.0.0.5:5432"),
        ),
        ("socks", TipoTunel::Dinamico, "127.0.0.1:1080", None),
        (
            "webhook",
            TipoTunel::Remoto,
            "127.0.0.1:0",
            Some("127.0.0.1:9000"),
        ),
    ] {
        almacen
            .crear_tunel(&DatosTunel {
                host_id: host,
                nombre: nombre.to_string(),
                tipo,
                escucha: escucha.to_string(),
                destino: destino.map(str::to_string),
                automatico: false,
            })
            .unwrap();
    }
    let ruta = dir.path().join("magi_config");
    let hosts = almacen.listar_hosts().unwrap();
    let tuneles = almacen.tuneles_por_host().unwrap();
    let grupos = almacen.listar_grupos().unwrap();
    fs::write(&ruta, exportar::generar_texto(&hosts, &grupos, &tuneles)).unwrap();

    let salida = std::process::Command::new("ssh")
        .arg("-F")
        .arg(&ruta)
        .args(["-G", "prueba"])
        .output()
        .expect("ejecutando ssh");
    let errores = String::from_utf8_lossy(&salida.stderr);
    assert!(
        salida.status.success(),
        "ssh no acepta el fichero generado: {errores}"
    );
    assert!(!errores.contains("bad configuration"), "{errores}");

    // Y lo que sí se puede escribir, está.
    let texto = fs::read_to_string(&ruta).unwrap();
    assert!(texto.contains("LocalForward 5432 10.0.0.5:5432"), "{texto}");
    assert!(texto.contains("DynamicForward 1080"), "{texto}");
    assert!(texto.contains("RemoteForward 0 127.0.0.1:9000"), "{texto}");
    // El del puerto 0 en local va comentado, no como línea inválida.
    assert!(!texto.contains("\n    LocalForward 0 "), "{texto}");
}
