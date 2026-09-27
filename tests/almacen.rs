use magi::almacen::Almacen;
use magi::deliberacion::{
    ComprobacionesHost, DatosVerificaciones, EjecucionResultado, NuevaDeliberacion,
    ResultadoDeliberacion, Veredicto,
};
use magi::modelo::{DatosHost, DatosTunel, IdentidadRef, Origen, TipoTunel, UltimoEstado};
use magi::snippets::{DatosSnippet, Destino};

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
        "TUNELES",
        "SNIPPETS",
        "SNIPPET_DESTINOS",
        "VERIFICACIONES_HOST",
        "DELIBERACIONES",
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
    assert_eq!(version, 5);
    let hosts = almacen.listar_hosts().unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].nombre, "viejo");
    assert_eq!(hosts[0].servicios, "");
    assert_eq!(
        hosts[0].sftp_dir_local, None,
        "la migración 3 deja los directorios nulos"
    );
    assert!(
        almacen.listar_tuneles().unwrap().is_empty(),
        "la migración 4 estrena TUNELES vacía"
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

// ---------------------------------------------------------------- túneles

fn tunel_de(host_id: i64, nombre: &str, tipo: TipoTunel, escucha: &str) -> DatosTunel {
    DatosTunel {
        host_id,
        nombre: nombre.to_string(),
        tipo,
        escucha: escucha.to_string(),
        destino: if tipo.lleva_destino() {
            Some("10.0.0.5:5432".to_string())
        } else {
            None
        },
        automatico: false,
    }
}

/// Los túneles de un host caen con él (ON DELETE CASCADE) y su nombre es único
/// por host.
#[test]
fn los_tuneles_caen_con_su_host_y_el_nombre_no_se_repite() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = almacen
        .crear_host(&datos("con-tuneles"), Origen::Manual)
        .unwrap();
    let otro = almacen.crear_host(&datos("otro"), Origen::Manual).unwrap();
    almacen
        .crear_tunel(&tunel_de(id, "pg-prod", TipoTunel::Local, "127.0.0.1:5432"))
        .unwrap();
    almacen
        .crear_tunel(&tunel_de(
            id,
            "webhook",
            TipoTunel::Remoto,
            "127.0.0.1:9000",
        ))
        .unwrap();
    assert_eq!(almacen.tuneles_de_host(id).unwrap().len(), 2);

    // El mismo nombre en otro host sí vale: se es único por host.
    almacen
        .crear_tunel(&tunel_de(
            otro,
            "pg-prod",
            TipoTunel::Local,
            "127.0.0.1:6543",
        ))
        .unwrap();

    // El mismo nombre en el mismo host, no.
    let repetido =
        almacen.crear_tunel(&tunel_de(id, "pg-prod", TipoTunel::Local, "127.0.0.1:7777"));
    assert!(
        repetido.unwrap_err().to_string().contains("nombre"),
        "el error debe hablar del nombre"
    );

    almacen.borrar_host(id).unwrap();
    assert!(almacen.tuneles_de_host(id).unwrap().is_empty());
    assert_eq!(almacen.tuneles_de_host(otro).unwrap().len(), 1);
}

/// Dos túneles del mismo tipo con la misma escucha se pisan: en local y
/// dinámico en cualquier host (la escucha es de esta máquina) y en remoto solo
/// en el mismo host (la escucha es del host).
#[test]
fn dos_tuneles_no_pueden_escuchar_lo_mismo() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let uno = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let dos = almacen.crear_host(&datos("dos"), Origen::Manual).unwrap();
    almacen
        .crear_tunel(&tunel_de(uno, "pg", TipoTunel::Local, "127.0.0.1:5432"))
        .unwrap();

    // Local: choca aunque sea otro host.
    assert!(almacen
        .crear_tunel(&tunel_de(dos, "pg", TipoTunel::Local, "127.0.0.1:5432"))
        .is_err());
    // Otro tipo y otro puerto, adelante.
    almacen
        .crear_tunel(&tunel_de(
            dos,
            "socks",
            TipoTunel::Dinamico,
            "127.0.0.1:1080",
        ))
        .unwrap();
    assert!(almacen
        .crear_tunel(&tunel_de(
            uno,
            "socks",
            TipoTunel::Dinamico,
            "127.0.0.1:1080"
        ))
        .is_err());

    // Remoto: el mismo puerto en hosts distintos no choca (son máquinas
    // distintas), pero sí en el mismo host.
    almacen
        .crear_tunel(&tunel_de(
            uno,
            "webhook",
            TipoTunel::Remoto,
            "127.0.0.1:9000",
        ))
        .unwrap();
    almacen
        .crear_tunel(&tunel_de(
            dos,
            "webhook",
            TipoTunel::Remoto,
            "127.0.0.1:9000",
        ))
        .unwrap();
    assert!(almacen
        .crear_tunel(&tunel_de(
            uno,
            "webhook-2",
            TipoTunel::Remoto,
            "127.0.0.1:9000"
        ))
        .is_err());

    // Editar sin cambiar la escucha no choca consigo mismo.
    let tunel = almacen
        .tunel_por_nombre(uno, "webhook")
        .unwrap()
        .expect("el túnel existe");
    let mut datos_tunel = tunel_de(uno, "webhook", TipoTunel::Remoto, "127.0.0.1:9000");
    datos_tunel.automatico = true;
    almacen.actualizar_tunel(tunel.id, &datos_tunel).unwrap();
    assert!(almacen.obtener_tunel(tunel.id).unwrap().automatico);
}

/// La validación en código: nombre, tipo, escucha y destino obligatorio.
#[test]
fn los_tuneles_se_validan_en_codigo() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("host"), Origen::Manual).unwrap();

    let mut malo = tunel_de(host, "con espacio", TipoTunel::Local, "127.0.0.1:5432");
    assert!(almacen.crear_tunel(&malo).is_err());

    malo = tunel_de(host, "sin-puerto", TipoTunel::Local, "127.0.0.1");
    assert!(almacen.crear_tunel(&malo).is_err());

    malo = tunel_de(host, "sin-destino", TipoTunel::Local, "127.0.0.1:5432");
    malo.destino = None;
    assert!(almacen.crear_tunel(&malo).is_err());

    // El dinámico no lleva destino, y con el 0 se pide un puerto libre.
    let dinamico = tunel_de(host, "socks", TipoTunel::Dinamico, "127.0.0.1:0");
    let id = almacen.crear_tunel(&dinamico).unwrap();
    let guardado = almacen.obtener_tunel(id).unwrap();
    assert_eq!(guardado.destino, None);
    assert_eq!(guardado.escucha, "127.0.0.1:0");
    assert_eq!(guardado.tipo, TipoTunel::Dinamico);

    // La escucha se guarda en su forma canónica, con corchetes para IPv6.
    let v6 = almacen
        .crear_tunel(&tunel_de(host, "v6", TipoTunel::Local, "[::1]:5433"))
        .unwrap();
    assert_eq!(almacen.obtener_tunel(v6).unwrap().escucha, "[::1]:5433");
}

/// El puerto 0 es «el que quede libre»: dos túneles así no se pisan, y el
/// mismo puerto con distinto tipo tampoco es un duplicado.
#[test]
fn el_puerto_cero_y_los_tipos_distintos_no_chocan() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("host"), Origen::Manual).unwrap();
    let otro = almacen.crear_host(&datos("otro"), Origen::Manual).unwrap();

    // Dos locales con el puerto 0: cada uno recibirá uno libre distinto.
    almacen
        .crear_tunel(&tunel_de(
            host,
            "libre-uno",
            TipoTunel::Local,
            "127.0.0.1:0",
        ))
        .unwrap();
    almacen
        .crear_tunel(&tunel_de(
            otro,
            "libre-dos",
            TipoTunel::Local,
            "127.0.0.1:0",
        ))
        .unwrap();

    // Mismo puerto con tipos distintos: uno escucha en local y el otro pide el
    // reenvío al host, así que no se pisan.
    almacen
        .crear_tunel(&tunel_de(
            host,
            "local-8080",
            TipoTunel::Local,
            "127.0.0.1:8080",
        ))
        .unwrap();
    almacen
        .crear_tunel(&tunel_de(
            host,
            "remoto-8080",
            TipoTunel::Remoto,
            "127.0.0.1:8080",
        ))
        .unwrap();
    almacen
        .crear_tunel(&tunel_de(
            otro,
            "socks-8080",
            TipoTunel::Dinamico,
            "127.0.0.1:8080",
        ))
        .unwrap();

    assert_eq!(almacen.tuneles_de_host(host).unwrap().len(), 3);
    assert_eq!(almacen.tuneles_de_host(otro).unwrap().len(), 2);
}

// ---------------------------------------------------------------- snippets

fn snippet_de(nombre: &str, destinos: Vec<Destino>) -> DatosSnippet {
    DatosSnippet {
        nombre: nombre.to_string(),
        comando: "systemctl restart nginx".to_string(),
        destinos,
        ..DatosSnippet::default()
    }
}

fn etiqueta(nombre: &str) -> Destino {
    Destino::Etiqueta(nombre.to_string())
}

fn suelto(id: i64, nombre: &str) -> Destino {
    Destino::Host {
        id,
        nombre: nombre.to_string(),
    }
}

/// Filas de `SNIPPET_DESTINOS` que cumplen `condicion` (SQL fijo del test).
fn contar_destinos(almacen: &Almacen, condicion: &str, parametro: i64) -> i64 {
    almacen
        .conexion()
        .query_row(
            &format!("SELECT COUNT(*) FROM SNIPPET_DESTINOS WHERE {condicion}"),
            [parametro],
            |fila| fila.get(0),
        )
        .unwrap()
}

fn contar_filas(almacen: &Almacen, tabla: &str) -> i64 {
    almacen
        .conexion()
        .query_row(&format!("SELECT COUNT(*) FROM {tabla}"), [], |fila| {
            fila.get(0)
        })
        .unwrap()
}

/// Una base de la fase 5 (cuatro migraciones) sube a la 5 sin perder hosts ni
/// túneles, y los hosts estrenan `snippet_al_conectar_id` a NULL.
#[test]
fn migracion_desde_fase5_conserva_los_hosts_sin_snippet_al_conectar() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta).unwrap();
        for migracion in &magi::almacen::migraciones::MIGRACIONES[..4] {
            conexion.execute_batch(migracion).unwrap();
        }
        conexion.pragma_update(None, "user_version", 4).unwrap();
        conexion
            .execute(
                "INSERT INTO HOSTS (nombre, direccion, puerto, opciones_extra, origen,
                                    creado_en, actualizado_en, servicios, sftp_dir_remoto)
                 VALUES ('fase5', '10.0.0.7', 22, '', 'manual',
                         '2026-06-01T00:00:00+02:00', '2026-06-01T00:00:00+02:00',
                         'nginx', '/srv')",
                [],
            )
            .unwrap();
        conexion
            .execute(
                "INSERT INTO TUNELES (host_id, nombre, tipo, escucha, destino, automatico,
                                      creado_en, actualizado_en)
                 VALUES (1, 'pg', 'local', '127.0.0.1:5432', '10.0.0.5:5432', 1,
                         '2026-06-01T00:00:00+02:00', '2026-06-01T00:00:00+02:00')",
                [],
            )
            .unwrap();
    }
    let almacen = Almacen::abrir(&ruta).unwrap();
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(version, 5);

    let hosts = almacen.listar_hosts().unwrap();
    assert_eq!(hosts.len(), 1);
    let host = &hosts[0];
    assert_eq!(host.nombre, "fase5");
    assert_eq!(host.servicios, "nginx");
    assert_eq!(host.sftp_dir_remoto.as_deref(), Some("/srv"));
    assert_eq!(host.snippet_al_conectar_id, None);
    assert_eq!(almacen.tuneles_de_host(host.id).unwrap().len(), 1);
    assert!(almacen.listar_snippets().unwrap().is_empty());
    assert!(almacen.verificaciones_por_host().unwrap().is_empty());
    assert_eq!(contar_filas(&almacen, "DELIBERACIONES"), 0);

    // La columna nueva es una clave ajena que funciona en la base migrada.
    let snippet = almacen
        .crear_snippet(&snippet_de("al-conectar", vec![suelto(host.id, "fase5")]))
        .unwrap();
    almacen
        .fijar_snippet_al_conectar(host.id, Some(snippet))
        .unwrap();
    assert_eq!(
        almacen
            .obtener_host(host.id)
            .unwrap()
            .snippet_al_conectar_id,
        Some(snippet)
    );
}

/// Borrar un snippet se lleva sus destinos (ON DELETE CASCADE) y no toca los
/// hosts a los que apuntaba.
#[test]
fn borrar_un_snippet_borra_sus_destinos() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let uno = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let otro = almacen
        .crear_snippet(&snippet_de("otro", vec![etiqueta("web")]))
        .unwrap();
    let id = almacen
        .crear_snippet(&snippet_de(
            "reiniciar",
            vec![etiqueta("web"), suelto(uno, "uno")],
        ))
        .unwrap();
    assert_eq!(contar_destinos(&almacen, "snippet_id = ?1", id), 2);

    almacen.borrar_snippet(id).unwrap();
    assert_eq!(contar_destinos(&almacen, "snippet_id = ?1", id), 0);
    assert!(almacen.obtener_snippet(id).is_err());
    assert_eq!(almacen.listar_hosts().unwrap().len(), 1);
    assert_eq!(
        almacen.obtener_snippet(otro).unwrap().destinos,
        vec![etiqueta("web")],
        "los destinos de otro snippet no caen"
    );
}

/// Borrar un host se lleva sus destinos sueltos (el snippet se queda con los
/// demás) y su fila de `VERIFICACIONES_HOST`.
#[test]
fn borrar_un_host_borra_sus_destinos_sueltos_y_sus_verificaciones() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let uno = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let dos = almacen.crear_host(&datos("dos"), Origen::Manual).unwrap();
    let id = almacen
        .crear_snippet(&snippet_de(
            "reiniciar",
            vec![etiqueta("web"), suelto(uno, "uno"), suelto(dos, "dos")],
        ))
        .unwrap();
    let salud = DatosVerificaciones {
        salud: true,
        ..DatosVerificaciones::default()
    };
    almacen.guardar_verificaciones(uno, &salud).unwrap();
    almacen.guardar_verificaciones(dos, &salud).unwrap();

    almacen.borrar_host(uno).unwrap();
    assert_eq!(contar_destinos(&almacen, "host_id = ?1", uno), 0);
    assert_eq!(
        almacen.obtener_snippet(id).unwrap().destinos,
        vec![etiqueta("web"), suelto(dos, "dos")]
    );
    assert_eq!(almacen.verificaciones_de_host(uno).unwrap(), None);
    let restantes = almacen.verificaciones_por_host().unwrap();
    assert_eq!(restantes.len(), 1);
    assert!(restantes[&dos].salud);
}

/// Borrar un snippet deja sin «snippet al conectar» a los hosts que lo tenían
/// y conserva la deliberación con `snippet_id` a NULL (ON DELETE SET NULL).
#[test]
fn borrar_un_snippet_deja_a_null_el_snippet_al_conectar_y_la_deliberacion() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let id = almacen
        .crear_snippet(&snippet_de("tmux", vec![suelto(host, "uno")]))
        .unwrap();
    magi::almacen::hosts::fijar_snippet_al_conectar(almacen.conexion(), host, Some(id)).unwrap();
    assert_eq!(
        almacen.hosts_con_snippet_al_conectar(id).unwrap(),
        vec![(host, "uno".to_string())]
    );
    let deliberacion = almacen
        .crear_deliberacion(&NuevaDeliberacion {
            snippet_id: Some(id),
            accion: "tmux → uno".to_string(),
            hosts: vec![(host, "uno".to_string())],
            comprobaciones: Vec::new(),
            resultado: ResultadoDeliberacion::Aprobada,
            bloqueada: false,
            motivo: None,
            usuario: "hector".to_string(),
        })
        .unwrap();

    almacen.borrar_snippet(id).unwrap();
    assert_eq!(
        almacen.obtener_host(host).unwrap().snippet_al_conectar_id,
        None
    );
    assert!(almacen
        .hosts_con_snippet_al_conectar(id)
        .unwrap()
        .is_empty());
    let fila = almacen.obtener_deliberacion(deliberacion).unwrap();
    assert_eq!(fila.snippet_id, None);
    assert_eq!(fila.accion, "tmux → uno");
    assert_eq!(fila.resultado, ResultadoDeliberacion::Aprobada);
    assert_eq!(fila.hosts, vec![(host, "uno".to_string())]);
}

/// Crear con destinos, obtener, listar por nombre, buscar por nombre y
/// actualizar sustituyendo los destinos.
#[test]
fn crud_de_snippets_con_destinos() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let uno = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let dos = almacen.crear_host(&datos("dos"), Origen::Manual).unwrap();

    let id = almacen
        .crear_snippet(&DatosSnippet {
            nombre: "  reiniciar nginx ".to_string(),
            comando: "systemctl restart {{servicio:nginx}}\n".to_string(),
            descripcion: "  reinicia el proxy  ".to_string(),
            etiquetas: vec!["Servicios web".to_string()],
            critico: true,
            timeout_seg: 120,
            parar_al_fallo: true,
            // El nombre del host suelto lo pone `HOSTS` al leer, no el que
            // venga en los datos.
            destinos: vec![etiqueta(" Web "), suelto(dos, "nombre-viejo")],
        })
        .unwrap();
    let snippet = almacen.obtener_snippet(id).unwrap();
    assert_eq!(snippet.nombre, "reiniciar nginx");
    assert_eq!(snippet.comando, "systemctl restart {{servicio:nginx}}");
    assert_eq!(snippet.descripcion, "reinicia el proxy");
    assert_eq!(snippet.etiquetas, vec!["servicios", "web"]);
    assert!(snippet.critico);
    assert_eq!(snippet.timeout_seg, 120);
    assert!(snippet.parar_al_fallo);
    assert_eq!(snippet.usado_veces, 0);
    assert_eq!(snippet.ultimo_uso_en, None);
    assert!(!snippet.creado_en.is_empty());
    assert_eq!(snippet.destinos, vec![etiqueta("web"), suelto(dos, "dos")]);

    // Listar va por nombre, no por orden de creación.
    let otro = almacen
        .crear_snippet(&snippet_de("apt update", vec![etiqueta("prod")]))
        .unwrap();
    let listado = almacen.listar_snippets().unwrap();
    let nombres: Vec<&str> = listado
        .iter()
        .map(|snippet| snippet.nombre.as_str())
        .collect();
    assert_eq!(nombres, vec!["apt update", "reiniciar nginx"]);
    assert_eq!(listado[0].destinos, vec![etiqueta("prod")]);
    assert_eq!(listado[1], snippet);

    // Por nombre exacto, recortando los espacios de la consulta.
    assert_eq!(
        almacen.snippet_por_nombre(" reiniciar nginx ").unwrap(),
        Some(snippet.clone())
    );
    assert_eq!(almacen.snippet_por_nombre("no existe").unwrap(), None);

    // Actualizar sustituye los destinos por completo.
    let mut editado = snippet.datos();
    editado.comando = "systemctl reload nginx".to_string();
    editado.critico = false;
    editado.destinos = vec![suelto(uno, "uno")];
    almacen.actualizar_snippet(id, &editado).unwrap();
    let snippet = almacen.obtener_snippet(id).unwrap();
    assert_eq!(snippet.comando, "systemctl reload nginx");
    assert!(!snippet.critico);
    assert_eq!(snippet.destinos, vec![suelto(uno, "uno")]);
    assert_eq!(contar_destinos(&almacen, "snippet_id = ?1", id), 1);
    assert_eq!(
        almacen.obtener_snippet(otro).unwrap().destinos,
        vec![etiqueta("prod")]
    );

    // Renombrar el host cambia el nombre del destino: va por id.
    let mut renombrado = datos("uno-bis");
    renombrado.etiquetas.clear();
    almacen.actualizar_host(uno, &renombrado).unwrap();
    assert_eq!(
        almacen.obtener_snippet(id).unwrap().destinos,
        vec![suelto(uno, "uno-bis")]
    );

    // Un snippet que ya no existe no se actualiza en silencio.
    assert!(almacen.actualizar_snippet(9999, &editado).is_err());
}

#[test]
fn nombre_de_snippet_repetido_da_error_legible() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();
    let error = almacen
        .crear_snippet(&snippet_de("  reiniciar ", vec![etiqueta("db")]))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ya existe un snippet con el nombre"),
        "{error}"
    );

    let otro = almacen
        .crear_snippet(&snippet_de("otro", vec![etiqueta("web")]))
        .unwrap();
    let error = almacen
        .actualizar_snippet(otro, &snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ya existe un snippet con el nombre"),
        "{error}"
    );
    assert_eq!(almacen.obtener_snippet(otro).unwrap().nombre, "otro");
    assert_eq!(almacen.listar_snippets().unwrap().len(), 2);
}

/// La validación de `snippets::validar` también protege el almacén: nada
/// inválido llega a `SNIPPETS`.
#[test]
fn los_snippets_se_validan_al_guardar() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let bueno = snippet_de("bueno", vec![etiqueta("web")]);

    let sin_destinos = DatosSnippet {
        destinos: Vec::new(),
        ..bueno.clone()
    };
    let error = almacen.crear_snippet(&sin_destinos).unwrap_err();
    assert!(error.to_string().contains("destino"), "{error}");

    // Un destino que se queda vacío al normalizar tampoco cuenta.
    let destino_en_blanco = DatosSnippet {
        destinos: vec![etiqueta("   ")],
        ..bueno.clone()
    };
    assert!(almacen.crear_snippet(&destino_en_blanco).is_err());

    for timeout_seg in [0, 4, 3601] {
        let error = almacen
            .crear_snippet(&DatosSnippet {
                timeout_seg,
                ..bueno.clone()
            })
            .unwrap_err();
        assert!(error.to_string().contains("timeout"), "{error}");
    }
    for timeout_seg in [5, 3600] {
        almacen
            .crear_snippet(&DatosSnippet {
                nombre: format!("limite-{timeout_seg}"),
                timeout_seg,
                ..bueno.clone()
            })
            .unwrap();
    }

    let entre_comillas = DatosSnippet {
        comando: "grep '{{patron}}' /var/log/syslog".to_string(),
        ..bueno.clone()
    };
    let error = almacen.crear_snippet(&entre_comillas).unwrap_err();
    assert!(error.to_string().contains("comillas"), "{error}");

    let sin_nombre = DatosSnippet {
        nombre: "   ".to_string(),
        ..bueno.clone()
    };
    assert!(almacen.crear_snippet(&sin_nombre).is_err());

    // Solo entraron los dos de los límites; editar tampoco se salta la
    // validación.
    assert_eq!(almacen.listar_snippets().unwrap().len(), 2);
    let id = almacen.crear_snippet(&bueno).unwrap();
    assert!(almacen.actualizar_snippet(id, &sin_destinos).is_err());
    assert_eq!(
        almacen.obtener_snippet(id).unwrap().destinos,
        vec![etiqueta("web")]
    );
}

#[test]
fn marcar_uso_cuenta_y_fecha_la_ultima_vez() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = almacen
        .crear_snippet(&snippet_de("uptime", vec![etiqueta("web")]))
        .unwrap();
    almacen.marcar_uso_snippet(id).unwrap();
    almacen.marcar_uso_snippet(id).unwrap();
    let snippet = almacen.obtener_snippet(id).unwrap();
    assert_eq!(snippet.usado_veces, 2);
    let ultimo = snippet.ultimo_uso_en.expect("fecha del último uso");
    assert!(
        chrono::DateTime::parse_from_rfc3339(&ultimo).is_ok(),
        "{ultimo}"
    );
}

#[test]
fn hosts_con_snippet_al_conectar_por_nombre() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let zeta = almacen.crear_host(&datos("zeta"), Origen::Manual).unwrap();
    let alfa = almacen.crear_host(&datos("alfa"), Origen::Manual).unwrap();
    let libre = almacen.crear_host(&datos("libre"), Origen::Manual).unwrap();
    let tmux = almacen
        .crear_snippet(&snippet_de("tmux", vec![etiqueta("web")]))
        .unwrap();
    let otro = almacen
        .crear_snippet(&snippet_de("otro", vec![etiqueta("web")]))
        .unwrap();
    almacen.fijar_snippet_al_conectar(zeta, Some(tmux)).unwrap();
    almacen.fijar_snippet_al_conectar(alfa, Some(tmux)).unwrap();
    almacen
        .fijar_snippet_al_conectar(libre, Some(otro))
        .unwrap();
    assert_eq!(
        almacen.hosts_con_snippet_al_conectar(tmux).unwrap(),
        vec![(alfa, "alfa".to_string()), (zeta, "zeta".to_string())]
    );
    almacen.fijar_snippet_al_conectar(zeta, None).unwrap();
    assert_eq!(
        almacen.hosts_con_snippet_al_conectar(tmux).unwrap(),
        vec![(alfa, "alfa".to_string())]
    );
    assert_eq!(
        almacen.hosts_con_snippet_al_conectar(otro).unwrap(),
        vec![(libre, "libre".to_string())]
    );
}

/// Filas tocadas a mano: un destino con etiqueta y host a la vez (o sin
/// ninguno) se salta, y un snippet con timeout imposible no rompe el listado
/// de los demás.
#[test]
fn las_filas_ilegibles_se_saltan_sin_romper_el_listado() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let uno = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let dos = almacen.crear_host(&datos("dos"), Origen::Manual).unwrap();
    let id = almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();
    let sano = almacen
        .crear_snippet(&snippet_de("sano", vec![suelto(uno, "uno")]))
        .unwrap();
    almacen
        .conexion()
        .execute(
            "INSERT INTO SNIPPET_DESTINOS (snippet_id, etiqueta, host_id) VALUES (?1, 'db', ?2)",
            rusqlite::params![id, dos],
        )
        .unwrap();
    almacen
        .conexion()
        .execute(
            "INSERT INTO SNIPPET_DESTINOS (snippet_id, etiqueta, host_id) VALUES (?1, NULL, NULL)",
            [id],
        )
        .unwrap();
    assert_eq!(contar_destinos(&almacen, "snippet_id = ?1", id), 3);

    let listado = almacen.listar_snippets().unwrap();
    assert_eq!(listado.len(), 2);
    assert_eq!(listado[0].nombre, "reiniciar");
    assert_eq!(listado[0].destinos, vec![etiqueta("web")]);
    assert_eq!(listado[1].destinos, vec![suelto(uno, "uno")]);
    assert_eq!(
        almacen.obtener_snippet(id).unwrap().destinos,
        vec![etiqueta("web")]
    );

    // Un timeout fuera de rango no se ejecuta tal cual: el snippet se salta
    // al listar y los demás siguen ahí.
    almacen
        .conexion()
        .execute("UPDATE SNIPPETS SET timeout_seg = 0 WHERE id = ?1", [id])
        .unwrap();
    let listado = almacen.listar_snippets().unwrap();
    assert_eq!(listado.len(), 1);
    assert_eq!(listado[0].id, sano);
    assert!(almacen.obtener_snippet(id).is_err());
}

// ---------------------------------------------------------- verificaciones

/// Una fila por host, creada al activar la primera comprobación y
/// actualizada (UPSERT) después.
#[test]
fn las_verificaciones_crean_fila_al_activar_y_despues_actualizan() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();

    // Sin ninguna activa y sin fila previa: nada que guardar.
    almacen
        .guardar_verificaciones(host, &DatosVerificaciones::default())
        .unwrap();
    assert_eq!(almacen.verificaciones_de_host(host).unwrap(), None);
    assert_eq!(contar_filas(&almacen, "VERIFICACIONES_HOST"), 0);

    // Activar la primera crea la fila.
    almacen
        .guardar_verificaciones(
            host,
            &DatosVerificaciones {
                salud: true,
                ..DatosVerificaciones::default()
            },
        )
        .unwrap();
    let guardadas = almacen.verificaciones_de_host(host).unwrap().unwrap();
    assert_eq!(guardadas.host_id, host);
    assert!(guardadas.salud);
    assert!(!guardadas.backup && !guardadas.tests);
    assert!(!guardadas.actualizado_en.is_empty());

    // Volver a guardar actualiza la misma fila y recorta los textos.
    let completas = DatosVerificaciones {
        salud: false,
        backup: true,
        backup_ruta: Some("  /var/backups ".to_string()),
        backup_patron: Some("*.sql.gz".to_string()),
        tests: true,
        tests_comando: Some(" make test ".to_string()),
    };
    almacen.guardar_verificaciones(host, &completas).unwrap();
    assert_eq!(contar_filas(&almacen, "VERIFICACIONES_HOST"), 1);
    let guardadas = almacen.verificaciones_de_host(host).unwrap().unwrap();
    assert!(!guardadas.salud);
    assert!(guardadas.backup);
    assert_eq!(guardadas.backup_ruta.as_deref(), Some("/var/backups"));
    assert_eq!(guardadas.backup_patron.as_deref(), Some("*.sql.gz"));
    assert!(guardadas.tests);
    assert_eq!(guardadas.tests_comando.as_deref(), Some("make test"));

    // Con fila, desactivarlo todo la actualiza (no la borra) y conserva la
    // ruta, el patrón y el comando que vengan en los datos.
    let apagadas = DatosVerificaciones {
        backup: false,
        tests: false,
        ..completas
    };
    almacen.guardar_verificaciones(host, &apagadas).unwrap();
    let guardadas = almacen.verificaciones_de_host(host).unwrap().unwrap();
    assert!(!guardadas.alguna_activa());
    assert_eq!(guardadas.backup_ruta.as_deref(), Some("/var/backups"));
    assert_eq!(guardadas.tests_comando.as_deref(), Some("make test"));
    let por_host = almacen.verificaciones_por_host().unwrap();
    assert_eq!(por_host.len(), 1);
    assert_eq!(por_host[&host], guardadas);
}

#[test]
fn las_verificaciones_incompletas_se_rechazan() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();

    let backup_sin_ruta = DatosVerificaciones {
        backup: true,
        ..DatosVerificaciones::default()
    };
    let error = almacen
        .guardar_verificaciones(host, &backup_sin_ruta)
        .unwrap_err();
    assert!(error.to_string().contains("directorio remoto"), "{error}");

    let backup_ruta_en_blanco = DatosVerificaciones {
        backup: true,
        backup_ruta: Some("   ".to_string()),
        ..DatosVerificaciones::default()
    };
    assert!(almacen
        .guardar_verificaciones(host, &backup_ruta_en_blanco)
        .is_err());

    let patron_invalido = DatosVerificaciones {
        backup: true,
        backup_ruta: Some("/var/backups".to_string()),
        backup_patron: Some("[".to_string()),
        ..DatosVerificaciones::default()
    };
    assert!(almacen
        .guardar_verificaciones(host, &patron_invalido)
        .is_err());

    let tests_sin_comando = DatosVerificaciones {
        tests: true,
        tests_comando: Some("  ".to_string()),
        ..DatosVerificaciones::default()
    };
    let error = almacen
        .guardar_verificaciones(host, &tests_sin_comando)
        .unwrap_err();
    assert!(error.to_string().contains("comando"), "{error}");

    assert_eq!(almacen.verificaciones_de_host(host).unwrap(), None);
    assert_eq!(contar_filas(&almacen, "VERIFICACIONES_HOST"), 0);
}

// ---------------------------------------------------------- deliberaciones

fn deliberacion_de(
    snippet_id: Option<i64>,
    hosts: Vec<(i64, String)>,
    resultado: ResultadoDeliberacion,
    motivo: Option<&str>,
) -> NuevaDeliberacion {
    let comprobaciones = hosts
        .iter()
        .map(|(host_id, host)| ComprobacionesHost {
            host_id: *host_id,
            host: host.clone(),
            salud: Veredicto::Aprueba {
                detalle: "NOMINAL · carga 0,4".to_string(),
                ms: 212,
            },
            backup: Veredicto::Rechaza {
                detalle: "último backup hace 31 h".to_string(),
                ms: 845,
            },
            tests: Veredicto::NoActiva,
        })
        .collect();
    NuevaDeliberacion {
        snippet_id,
        accion: "reiniciar nginx → uno, dos".to_string(),
        hosts,
        comprobaciones,
        resultado,
        bloqueada: true,
        motivo: motivo.map(str::to_string),
        usuario: "hector".to_string(),
    }
}

/// `hosts_json` y `comprobaciones_json` vuelven tal cual se guardaron, con
/// los cuatro tipos de veredicto.
#[test]
fn las_deliberaciones_guardan_hosts_y_comprobaciones_de_ida_y_vuelta() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let uno = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let dos = almacen.crear_host(&datos("dos"), Origen::Manual).unwrap();
    let snippet = almacen
        .crear_snippet(&snippet_de("reiniciar nginx", vec![etiqueta("web")]))
        .unwrap();
    let mut nueva = deliberacion_de(
        Some(snippet),
        vec![(uno, "uno".to_string()), (dos, "dos «bis»".to_string())],
        ResultadoDeliberacion::Forzada,
        Some("  revisado a mano  "),
    );
    nueva.comprobaciones[1].salud = Veredicto::Pendiente;
    let id = almacen.crear_deliberacion(&nueva).unwrap();

    let fila = almacen.obtener_deliberacion(id).unwrap();
    assert_eq!(fila.id, id);
    assert!(!fila.fecha.is_empty());
    assert_eq!(fila.snippet_id, Some(snippet));
    assert_eq!(fila.accion, nueva.accion);
    assert_eq!(fila.hosts, nueva.hosts);
    assert_eq!(fila.comprobaciones, nueva.comprobaciones);
    assert_eq!(fila.resultado, ResultadoDeliberacion::Forzada);
    assert!(fila.bloqueada);
    assert_eq!(fila.motivo.as_deref(), Some("revisado a mano"));
    assert_eq!(fila.usuario, "hector");
    assert_eq!(fila.ejecucion_resultado, None);
    assert_eq!(fila.consenso(), (1, 4));

    // En la base es JSON legible, no un volcado opaco.
    let (hosts_json, comprobaciones_json): (String, String) = almacen
        .conexion()
        .query_row(
            "SELECT hosts_json, comprobaciones_json FROM DELIBERACIONES WHERE id = ?1",
            [id],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .unwrap();
    let hosts: serde_json::Value = serde_json::from_str(&hosts_json).unwrap();
    assert_eq!(hosts[1][1], "dos «bis»");
    let comprobaciones: serde_json::Value = serde_json::from_str(&comprobaciones_json).unwrap();
    assert_eq!(comprobaciones[0]["backup"]["estado"], "rechaza");
    assert_eq!(comprobaciones[0]["tests"]["estado"], "n/a");
    assert_eq!(comprobaciones[1]["salud"]["estado"], "pendiente");

    // Aprobada y cancelada no necesitan motivo; un motivo en blanco es None.
    for resultado in [
        ResultadoDeliberacion::Aprobada,
        ResultadoDeliberacion::Cancelada,
    ] {
        let id = almacen
            .crear_deliberacion(&deliberacion_de(
                None,
                vec![(uno, "uno".to_string())],
                resultado,
                Some("   "),
            ))
            .unwrap();
        let fila = almacen.obtener_deliberacion(id).unwrap();
        assert_eq!(fila.resultado, resultado);
        assert_eq!(fila.snippet_id, None);
        assert_eq!(fila.motivo, None);
    }
}

#[test]
fn una_deliberacion_forzada_sin_motivo_no_se_guarda() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let hosts = vec![(1, "uno".to_string())];
    for motivo in [None, Some(""), Some("   ")] {
        let error = almacen
            .crear_deliberacion(&deliberacion_de(
                None,
                hosts.clone(),
                ResultadoDeliberacion::Forzada,
                motivo,
            ))
            .unwrap_err();
        assert!(error.to_string().contains("motivo"), "{error}");
    }
    assert_eq!(contar_filas(&almacen, "DELIBERACIONES"), 0);
}

/// El resultado de la ejecución se escribe una sola vez: la segunda escritura
/// no pisa la primera.
#[test]
fn el_resultado_de_la_ejecucion_solo_se_fija_una_vez() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = almacen
        .crear_deliberacion(&deliberacion_de(
            None,
            vec![(1, "uno".to_string())],
            ResultadoDeliberacion::Aprobada,
            None,
        ))
        .unwrap();
    assert!(almacen
        .fijar_resultado_deliberacion(id, EjecucionResultado::Parcial)
        .unwrap());
    assert!(!almacen
        .fijar_resultado_deliberacion(id, EjecucionResultado::Ok)
        .unwrap());
    assert_eq!(
        almacen
            .obtener_deliberacion(id)
            .unwrap()
            .ejecucion_resultado,
        Some(EjecucionResultado::Parcial)
    );
    // Una fila que no existe no se escribe ni da error.
    assert!(!almacen
        .fijar_resultado_deliberacion(id + 100, EjecucionResultado::Error)
        .unwrap());
}
