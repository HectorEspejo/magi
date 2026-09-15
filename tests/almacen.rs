use magi::almacen::Almacen;
use magi::modelo::{DatosHost, IdentidadRef, Origen, UltimoEstado};

fn datos(nombre: &str) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: "10.0.0.1".to_string(),
        usuario: Some("hector".to_string()),
        puerto: 2222,
        etiquetas: vec!["web".to_string(), "prod".to_string()],
        ..DatosHost::default()
    }
}

#[test]
fn migracion_desde_vacio_crea_todas_las_tablas() {
    let almacen = Almacen::abrir_en_memoria().expect("almacén en memoria");
    let mut nombres: Vec<String> = almacen
        .conexion()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |fila| fila.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<String>>>()
        .unwrap();
    nombres.sort();
    for tabla in [
        "ETIQUETAS",
        "GRUPOS",
        "HOSTS",
        "HOST_ETIQUETAS",
        "SONDEOS",
        "IDENTIDADES",
        "REGISTRO",
    ] {
        assert!(nombres.contains(&tabla.to_string()), "falta {tabla}");
    }
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert!(version >= 2);
}

#[test]
fn migracion_desde_fase1_conserva_los_datos() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta).unwrap();
        conexion
            .execute_batch(magi::almacen::migraciones::MIGRACIONES[0])
            .unwrap();
        conexion.pragma_update(None, "user_version", 1).unwrap();
        conexion
            .execute(
                "INSERT INTO HOSTS (nombre, direccion, puerto, opciones_extra, origen,
                                    creado_en, actualizado_en)
                 VALUES ('viejo', '10.0.0.9', 22, '', 'manual',
                         '2026-01-01T00:00:00+01:00', '2026-01-01T00:00:00+01:00')",
                [],
            )
            .unwrap();
    }
    let almacen = Almacen::abrir(&ruta).unwrap();
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(version, 3);
    let hosts = almacen.listar_hosts().unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].nombre, "viejo");
    assert_eq!(hosts[0].servicios, "");
    assert_eq!(
        hosts[0].sftp_dir_local, None,
        "la migración 3 deja los directorios nulos"
    );
    assert!(almacen.listar_identidades(true).unwrap().is_empty());
}

#[test]
fn los_sondeos_se_purgan_a_los_veinte_y_caen_con_el_host() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    for indice in 0..25 {
        let mut sondeo = magi::modelo::Sondeo::vacio(host);
        sondeo.duracion_ms = indice;
        sondeo.nucleos = Some(8);
        almacen.guardar_sondeo(&sondeo).unwrap();
    }
    let guardados = almacen.ultimos_sondeos(host, 100).unwrap();
    assert_eq!(guardados.len(), 20);
    assert_eq!(guardados[0].duracion_ms, 24);
    assert_eq!(guardados[19].duracion_ms, 5);
    almacen.borrar_host(host).unwrap();
    assert_eq!(almacen.ultimos_sondeos(host, 100).unwrap().len(), 0);
}

#[test]
fn el_registro_conserva_la_fila_con_host_e_identidad_a_null() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let identidad = almacen
        .crear_identidad(
            "ed25519",
            "SHA256:abc",
            magi::modelo::OrigenIdentidad::Agente,
            None,
            Some("prueba"),
        )
        .unwrap();
    magi::registro::anotar(
        almacen.conexion(),
        magi::registro::CONEXION_ABIERTA,
        Some(host),
        Some(identidad),
        "sesión abierta",
        magi::modelo::ResultadoRegistro::Ok,
    )
    .unwrap();
    almacen.borrar_host(host).unwrap();
    almacen
        .conexion()
        .execute("DELETE FROM IDENTIDADES WHERE id = ?1", [identidad])
        .unwrap();
    let filtro = magi::registro::FiltroRegistro::default();
    let entradas = almacen.listar_registro(&filtro, 10, 0).unwrap();
    assert_eq!(entradas.len(), 1);
    assert_eq!(entradas[0].host_id, None);
    assert_eq!(entradas[0].host_nombre, None);
    assert_eq!(entradas[0].identidad_id, None);
    assert_eq!(entradas[0].identidad_alias, None);
    assert_eq!(entradas[0].detalle, "sesión abierta");
}

#[test]
fn las_identidades_sincronizan_por_huella_y_no_resucitan_revocadas() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let claves = vec![
        magi::almacen::identidades::ClaveSincronizada {
            tipo: "ed25519".to_string(),
            huella: "SHA256:uno".to_string(),
            origen: magi::modelo::OrigenIdentidad::Agente,
            ruta: None,
            comentario: Some("4d3".to_string()),
        },
        magi::almacen::identidades::ClaveSincronizada {
            tipo: "rsa".to_string(),
            huella: "SHA256:dos".to_string(),
            origen: magi::modelo::OrigenIdentidad::Fichero,
            ruta: Some("~/.ssh/id_rsa".to_string()),
            comentario: None,
        },
    ];
    almacen.sincronizar_identidades(&claves).unwrap();
    assert_eq!(almacen.listar_identidades(false).unwrap().len(), 2);
    let uno = almacen.identidad_por_huella("SHA256:uno").unwrap().unwrap();
    assert_eq!(uno.alias, "4d3");

    let mut datos_host = datos("uno");
    datos_host.identidad_ref = IdentidadRef::Agente("SHA256:uno".to_string());
    let host = almacen.crear_host(&datos_host, Origen::Manual).unwrap();
    let afectados = almacen
        .revocar_identidad(uno.id, std::path::Path::new("/home/nadie"))
        .unwrap();
    assert_eq!(afectados, 1);
    assert!(almacen.obtener_host(host).unwrap().identidad_ref.es_auto());
    almacen.sincronizar_identidades(&claves).unwrap();
    let uno = almacen.identidad_por_huella("SHA256:uno").unwrap().unwrap();
    assert!(uno.revocada());
    assert_eq!(almacen.listar_identidades(false).unwrap().len(), 1);
    almacen.reactivar_identidad(uno.id).unwrap();
    assert!(!almacen
        .identidad_por_huella("SHA256:uno")
        .unwrap()
        .unwrap()
        .revocada());
}

#[test]
fn el_registro_se_exporta_a_csv_y_json() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    magi::registro::anotar(
        almacen.conexion(),
        magi::registro::SONDEO_FALLIDO,
        Some(host),
        None,
        "timeout tras 5 s",
        magi::modelo::ResultadoRegistro::Error,
    )
    .unwrap();
    let entradas = almacen.registro_para_exportar(None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("registro.csv");
    let json = dir.path().join("registro.json");
    assert_eq!(magi::registro::exportar_csv(&csv, &entradas).unwrap(), 1);
    assert_eq!(magi::registro::exportar_json(&json, &entradas).unwrap(), 1);
    let texto = std::fs::read_to_string(&csv).unwrap();
    assert!(
        texto.starts_with("fecha,tipo,host,identidad,resultado,detalle"),
        "{texto}"
    );
    assert!(texto.contains("uno"));
    let valor: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(valor[0]["tipo"], "sondeo_fallido");
    assert_eq!(valor[0]["host"], "uno");
    let desde = almacen.registro_para_exportar(Some("2999-01-01")).unwrap();
    assert!(desde.is_empty());
}

#[test]
fn crud_de_hosts_conserva_campos_y_etiquetas() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let host = almacen.obtener_host(id).unwrap();
    assert_eq!(host.nombre, "uno");
    assert_eq!(host.direccion, "10.0.0.1");
    assert_eq!(host.usuario.as_deref(), Some("hector"));
    assert_eq!(host.puerto, 2222);
    assert_eq!(host.keepalive_seg, Some(30));
    assert_eq!(host.etiquetas, vec!["prod", "web"]);
    assert_eq!(host.origen, Origen::Manual);

    let mut editado = datos("uno");
    editado.direccion = "10.0.0.2".to_string();
    editado.identidad_ref = IdentidadRef::Fichero("~/.ssh/id_ed25519".to_string());
    editado.multiplexar = true;
    editado.keepalive_seg = Some(45);
    editado.etiquetas = vec!["solo".to_string()];
    almacen.actualizar_host(id, &editado).unwrap();
    let host = almacen.obtener_host(id).unwrap();
    assert_eq!(host.direccion, "10.0.0.2");
    assert_eq!(
        host.identidad_ref,
        IdentidadRef::Fichero("~/.ssh/id_ed25519".to_string())
    );
    assert!(host.multiplexar);
    assert_eq!(host.keepalive_seg, Some(45));
    assert_eq!(host.etiquetas, vec!["solo"]);

    almacen.borrar_host(id).unwrap();
    assert!(almacen.listar_hosts().unwrap().is_empty());
}

#[test]
fn la_identidad_de_contrasena_se_guarda_como_marca_sin_secreto() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let mut con_contrasena = datos("pass");
    con_contrasena.identidad_ref = IdentidadRef::Contrasena;
    let id = almacen.crear_host(&con_contrasena, Origen::Manual).unwrap();
    assert_eq!(
        almacen.obtener_host(id).unwrap().identidad_ref,
        IdentidadRef::Contrasena
    );
    let bruto: String = almacen
        .conexion()
        .query_row(
            "SELECT identidad_ref FROM HOSTS WHERE id = ?1",
            [id],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(bruto, "contrasena");
}

#[test]
fn nombre_de_host_duplicado_da_error_legible() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    almacen.crear_host(&datos("dup"), Origen::Manual).unwrap();
    let error = almacen
        .crear_host(&datos("dup"), Origen::Manual)
        .unwrap_err();
    assert!(error.to_string().contains("ya existe un host"));
    assert!(almacen.existe_nombre_host("dup", None).unwrap());
    let id = almacen.listar_hosts().unwrap()[0].id;
    assert!(!almacen.existe_nombre_host("dup", Some(id)).unwrap());
}

#[test]
fn borrar_un_grupo_mueve_sus_hosts_a_sin_grupo() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let grupo = almacen.crear_grupo("producción").unwrap();
    let mut datos = datos("uno");
    datos.grupo_id = Some(grupo);
    let id = almacen.crear_host(&datos, Origen::Manual).unwrap();
    almacen.borrar_grupo(grupo).unwrap();
    let host = almacen.obtener_host(id).unwrap();
    assert_eq!(host.grupo_id, None);
    assert_eq!(almacen.listar_hosts().unwrap().len(), 1);
}

#[test]
fn borrar_un_host_de_salto_limpia_los_dependientes() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let salto = almacen.crear_host(&datos("salto"), Origen::Manual).unwrap();
    let mut dependiente = datos("destino");
    dependiente.salto_host_id = Some(salto);
    let id = almacen.crear_host(&dependiente, Origen::Manual).unwrap();
    assert_eq!(almacen.dependientes_de_salto(salto).unwrap(), 1);
    almacen.borrar_host(salto).unwrap();
    assert_eq!(almacen.obtener_host(id).unwrap().salto_host_id, None);
}

#[test]
fn una_etiqueta_sin_hosts_se_elimina() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    assert_eq!(almacen.listar_etiquetas().unwrap().len(), 2);
    let mut sin_etiquetas = datos("uno");
    sin_etiquetas.etiquetas.clear();
    almacen.actualizar_host(id, &sin_etiquetas).unwrap();
    assert!(almacen.listar_etiquetas().unwrap().is_empty());
    assert_eq!(almacen.listar_hosts().unwrap()[0].etiquetas.len(), 0);
}

#[test]
fn grupos_se_reordenan_y_persiste_el_plegado() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let a = almacen.crear_grupo("a").unwrap();
    let b = almacen.crear_grupo("b").unwrap();
    let grupos = almacen.listar_grupos().unwrap();
    assert_eq!(grupos[0].id, a);
    assert_eq!(grupos[1].id, b);
    almacen.mover_grupo(b, -1).unwrap();
    let grupos = almacen.listar_grupos().unwrap();
    assert_eq!(grupos[0].id, b);
    almacen.alternar_plegado(b, true).unwrap();
    let grupos = almacen.listar_grupos().unwrap();
    assert!(grupos[0].plegado);
}

#[test]
fn ultimo_estado_y_ultima_conexion_se_marcan() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    almacen
        .marcar_estado(id, Some(UltimoEstado::Error))
        .unwrap();
    let host = almacen.obtener_host(id).unwrap();
    assert_eq!(host.ultimo_estado, Some(UltimoEstado::Error));
    assert_eq!(host.ultima_conexion_en, None);
    almacen.marcar_conexion(id).unwrap();
    let host = almacen.obtener_host(id).unwrap();
    assert_eq!(host.ultimo_estado, Some(UltimoEstado::Ok));
    assert!(host.ultima_conexion_en.is_some());
}

#[test]
fn fijar_salto_y_opciones_extra_acumulan() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let salto = almacen.crear_host(&datos("salto"), Origen::Manual).unwrap();
    let id = almacen
        .crear_host(&datos("destino"), Origen::Manual)
        .unwrap();
    almacen
        .conexion()
        .execute(
            "UPDATE HOSTS SET salto_host_id = ?1 WHERE id = ?2",
            rusqlite::params![salto, id],
        )
        .unwrap();
    assert_eq!(
        almacen.obtener_host(id).unwrap().salto_nombre.as_deref(),
        Some("salto")
    );
    magi::almacen::hosts::anadir_opciones_extra(almacen.conexion(), id, "ProxyJump antiguo")
        .unwrap();
    magi::almacen::hosts::anadir_opciones_extra(almacen.conexion(), id, "ForwardAgent yes")
        .unwrap();
    assert_eq!(
        almacen.obtener_host(id).unwrap().opciones_extra,
        "ProxyJump antiguo\nForwardAgent yes"
    );
}
