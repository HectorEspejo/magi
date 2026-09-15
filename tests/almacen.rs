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
    for tabla in ["ETIQUETAS", "GRUPOS", "HOSTS", "HOST_ETIQUETAS"] {
        assert!(nombres.contains(&tabla.to_string()), "falta {tabla}");
    }
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert!(version >= 1);
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
