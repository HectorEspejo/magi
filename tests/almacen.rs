use magi::almacen::Almacen;
use magi::deliberacion::{
    ComprobacionesHost, DatosVerificaciones, EjecucionResultado, NuevaDeliberacion,
    ResultadoDeliberacion, Veredicto,
};
use magi::modelo::{
    DatosHost, DatosSincronizacion, DatosTunel, IdentidadRef, Origen, ResultadoSincronizacion,
    TipoTunel, UltimoEstado,
};
use magi::protocolo::Direccion;
use magi::snippets::{DatosSnippet, Destino};

fn datos(nombre: &str) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: "192.0.2.1".to_string(),
        usuario: Some("usuario".to_string()),
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
        "SINCRONIZACIONES_DIR",
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
                 VALUES ('viejo', '192.0.2.9', 22, '', 'manual',
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
    assert_eq!(version, 7);
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
    assert_eq!(host.direccion, "192.0.2.1");
    assert_eq!(host.usuario.as_deref(), Some("usuario"));
    assert_eq!(host.puerto, 2222);
    assert_eq!(host.keepalive_seg, Some(30));
    assert_eq!(host.etiquetas, vec!["prod", "web"]);
    assert_eq!(host.origen, Origen::Manual);

    let mut editado = datos("uno");
    editado.direccion = "192.0.2.2".to_string();
    editado.identidad_ref = IdentidadRef::Fichero("~/.ssh/id_ed25519".to_string());
    editado.multiplexar = true;
    editado.keepalive_seg = Some(45);
    editado.etiquetas = vec!["solo".to_string()];
    almacen.actualizar_host(id, &editado).unwrap();
    let host = almacen.obtener_host(id).unwrap();
    assert_eq!(host.direccion, "192.0.2.2");
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
            Some("192.0.2.5:5432".to_string())
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
                 VALUES ('fase5', '192.0.2.7', 22, '', 'manual',
                         '2026-06-01T00:00:00+02:00', '2026-06-01T00:00:00+02:00',
                         'nginx', '/srv')",
                [],
            )
            .unwrap();
        conexion
            .execute(
                "INSERT INTO TUNELES (host_id, nombre, tipo, escucha, destino, automatico,
                                      creado_en, actualizado_en)
                 VALUES (1, 'pg', 'local', '127.0.0.1:5432', '192.0.2.5:5432', 1,
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
    assert_eq!(version, 7);

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
            usuario: "usuario".to_string(),
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
        usuario: "usuario".to_string(),
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
    assert_eq!(fila.usuario, "usuario");
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

// ---------------------------------------------------------------- Fase 8

fn sincronizacion_de(host_id: i64, nombre: &str) -> DatosSincronizacion {
    DatosSincronizacion {
        host_id,
        nombre: nombre.to_string(),
        ruta_local: "/home/usuario/proyectos/cooperapp".to_string(),
        ruta_remota: "/var/www/cooperapp".to_string(),
        direccion: Direccion::Subida,
        borrar: false,
        exclusiones: vec!["*.log".to_string(), "tmp/".to_string()],
    }
}

/// Una base de la fase 7 (cinco migraciones) sube a la 6 sin perder hosts,
/// túneles, snippets ni deliberaciones, y estrena `SINCRONIZACIONES_DIR` vacía
/// y usable.
#[test]
fn migracion_desde_fase7_estrena_las_sincronizaciones() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta).unwrap();
        for migracion in &magi::almacen::migraciones::MIGRACIONES[..5] {
            conexion.execute_batch(migracion).unwrap();
        }
        conexion.pragma_update(None, "user_version", 5).unwrap();
        conexion
            .execute(
                "INSERT INTO HOSTS (nombre, direccion, puerto, opciones_extra, origen,
                                    creado_en, actualizado_en, servicios)
                 VALUES ('fase7', '192.0.2.8', 22, '', 'manual',
                         '2026-09-01T00:00:00+02:00', '2026-09-01T00:00:00+02:00', 'nginx')",
                [],
            )
            .unwrap();
        conexion
            .execute(
                "INSERT INTO TUNELES (host_id, nombre, tipo, escucha, destino, automatico,
                                      creado_en, actualizado_en)
                 VALUES (1, 'pg', 'local', '127.0.0.1:5432', '192.0.2.5:5432', 0,
                         '2026-09-01T00:00:00+02:00', '2026-09-01T00:00:00+02:00')",
                [],
            )
            .unwrap();
        conexion
            .execute(
                "INSERT INTO SNIPPETS (nombre, comando, creado_en, actualizado_en)
                 VALUES ('uptime', 'uptime',
                         '2026-09-01T00:00:00+02:00', '2026-09-01T00:00:00+02:00')",
                [],
            )
            .unwrap();
    }
    let almacen = Almacen::abrir(&ruta).unwrap();
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(version, 7);
    let hosts = almacen.listar_hosts().unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].servicios, "nginx");
    assert_eq!(almacen.tuneles_de_host(hosts[0].id).unwrap().len(), 1);
    assert_eq!(almacen.listar_snippets().unwrap().len(), 1);
    assert!(almacen.listar_sincronizaciones().unwrap().is_empty());

    let id = almacen
        .crear_sincronizacion(&sincronizacion_de(hosts[0].id, "web-prod"))
        .unwrap();
    let guardada = almacen.obtener_sincronizacion(id).unwrap();
    assert_eq!(guardada.host_nombre, "fase7");
    assert_eq!(guardada.direccion, Direccion::Subida);
    assert_eq!(guardada.exclusiones, vec!["*.log", "tmp/"]);
    assert_eq!(guardada.ultima_ejecucion_en, None);
    assert_eq!(guardada.ultimo_resultado, None);
}

/// Las sincronizaciones de un host caen con él (ON DELETE CASCADE) y su
/// nombre es único por host, pero se repite entre hosts.
#[test]
fn las_sincronizaciones_caen_con_su_host_y_el_nombre_no_se_repite() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = almacen
        .crear_host(&datos("con-sync"), Origen::Manual)
        .unwrap();
    let otro = almacen.crear_host(&datos("otro"), Origen::Manual).unwrap();
    almacen
        .crear_sincronizacion(&sincronizacion_de(id, "web-prod"))
        .unwrap();
    let mut logs = sincronizacion_de(id, "logs");
    logs.direccion = Direccion::Bajada;
    logs.borrar = true;
    almacen.crear_sincronizacion(&logs).unwrap();
    almacen
        .crear_sincronizacion(&sincronizacion_de(otro, "web-prod"))
        .unwrap();

    let repetida = almacen.crear_sincronizacion(&sincronizacion_de(id, "web-prod"));
    assert!(
        repetida.unwrap_err().to_string().contains("web-prod"),
        "el error dice qué nombre choca"
    );
    let del_host = almacen.sincronizaciones_de_host(id).unwrap();
    assert_eq!(
        del_host
            .iter()
            .map(|s| s.nombre.as_str())
            .collect::<Vec<_>>(),
        vec!["logs", "web-prod"],
        "por nombre"
    );
    assert!(del_host[0].borrar);
    assert_eq!(del_host[0].direccion, Direccion::Bajada);

    almacen.borrar_host(id).unwrap();
    assert!(almacen.sincronizaciones_de_host(id).unwrap().is_empty());
    assert_eq!(almacen.sincronizaciones_de_host(otro).unwrap().len(), 1);
}

/// Validación en código (sin `CHECK`): nombre `[A-Za-z0-9._-]+` y rutas no
/// vacías; una dirección desconocida escrita a mano no se interpreta.
#[test]
fn las_sincronizaciones_se_validan_en_codigo() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("h"), Origen::Manual).unwrap();
    let mut mal = sincronizacion_de(host, "con espacios");
    assert!(almacen.crear_sincronizacion(&mal).is_err());
    mal.nombre = "bien".to_string();
    mal.ruta_local = "  ".to_string();
    assert!(almacen.crear_sincronizacion(&mal).is_err());
    mal.ruta_local = "/tmp".to_string();
    mal.ruta_remota = String::new();
    assert!(almacen.crear_sincronizacion(&mal).is_err());

    let id = almacen
        .crear_sincronizacion(&sincronizacion_de(host, "buena"))
        .unwrap();
    almacen
        .conexion()
        .execute(
            "UPDATE SINCRONIZACIONES_DIR SET direccion = 'lateral' WHERE id = ?1",
            [id],
        )
        .unwrap();
    assert!(
        almacen.sincronizaciones_de_host(host).unwrap().is_empty(),
        "una fila con dirección desconocida se salta"
    );
}

/// `ultima_ejecucion_en` la escribe el cliente al lanzar y `ultimo_resultado`
/// el servidor al terminar; editar no los toca.
#[test]
fn la_ultima_ejecucion_y_su_resultado_se_guardan_aparte() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("h"), Origen::Manual).unwrap();
    let id = almacen
        .crear_sincronizacion(&sincronizacion_de(host, "web-prod"))
        .unwrap();
    almacen.marcar_ejecucion_sincronizacion(id).unwrap();
    magi::almacen::sincronizaciones::fijar_resultado(
        almacen.conexion(),
        id,
        ResultadoSincronizacion::Parcial,
    )
    .unwrap();
    let mut datos = sincronizacion_de(host, "web-prod-2");
    datos.borrar = true;
    almacen.actualizar_sincronizacion(id, &datos).unwrap();
    let guardada = almacen.obtener_sincronizacion(id).unwrap();
    assert_eq!(guardada.nombre, "web-prod-2");
    assert!(guardada.borrar);
    assert!(guardada.ultima_ejecucion_en.is_some());
    assert_eq!(
        guardada.ultimo_resultado,
        Some(ResultadoSincronizacion::Parcial)
    );
    almacen.borrar_sincronizacion(id).unwrap();
    assert!(almacen.actualizar_sincronizacion(id, &datos).is_err());
}

/// Una `magi.db` con `user_version` mayor que la que conoce el binario no se
/// abre ni se toca (Fase 9): las migraciones solo van hacia delante.
#[test]
fn una_base_de_datos_de_version_futura_no_se_abre_ni_se_toca() {
    let directorio = tempfile::tempdir().unwrap();
    let ruta = directorio.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta).unwrap();
        conexion
            .execute_batch(
                "CREATE TABLE MARCA (valor TEXT); \
                 INSERT INTO MARCA VALUES ('intacta'); \
                 PRAGMA user_version = 99;",
            )
            .unwrap();
    }
    let antes = std::fs::read(&ruta).unwrap();
    let error = match Almacen::abrir(&ruta) {
        Ok(_) => panic!("una versión futura no se abre"),
        Err(error) => error,
    };
    let mensaje = format!("{error:#}");
    assert!(
        mensaje.contains("esta base de datos es de una versión más nueva de MAGI; actualiza MAGI"),
        "{mensaje}"
    );
    // Ni contenido, ni diario WAL, ni permisos: el fichero queda igual.
    assert_eq!(
        std::fs::read(&ruta).unwrap(),
        antes,
        "el fichero queda intacto"
    );
    assert!(!directorio.path().join("magi.db-wal").exists());
    let conexion = rusqlite::Connection::open(&ruta).unwrap();
    let version: i64 = conexion
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(version, 99);
    let marca: String = conexion
        .query_row("SELECT valor FROM MARCA", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(marca, "intacta");
}

/// Una magi.db de una versión futura con un WAL pendiente de recuperar tampoco
/// se abre ni se toca: comprobar la versión abriendo el original aplicaría el
/// WAL y modificaría el fichero antes de poder rechazarlo (Fase 9).
#[test]
fn una_base_de_datos_futura_con_wal_pendiente_no_se_toca() {
    let origen = tempfile::tempdir().unwrap();
    let destino = tempfile::tempdir().unwrap();
    let ruta_origen = origen.path().join("magi.db");
    let ruta = destino.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta_origen).unwrap();
        conexion.pragma_update(None, "journal_mode", "WAL").unwrap();
        conexion
            .pragma_update(None, "wal_autocheckpoint", 0)
            .unwrap();
        conexion
            .execute_batch(
                "CREATE TABLE MARCA (valor TEXT); \
                 INSERT INTO MARCA VALUES ('intacta'); \
                 PRAGMA user_version = 99;",
            )
            .unwrap();
        // Copia del par db+wal con la conexión abierta: el WAL contiene la
        // versión y la cabecera principal todavía puede ser antigua.
        std::fs::copy(&ruta_origen, &ruta).unwrap();
        let mut wal_origen = ruta_origen.as_os_str().to_os_string();
        wal_origen.push("-wal");
        let mut wal = ruta.as_os_str().to_os_string();
        wal.push("-wal");
        std::fs::copy(&wal_origen, &wal).unwrap();
    }
    let mut wal = ruta.as_os_str().to_os_string();
    wal.push("-wal");
    let wal = std::path::PathBuf::from(wal);
    assert!(wal.exists(), "el escenario necesita un WAL pendiente");
    let db_antes = std::fs::read(&ruta).unwrap();
    let wal_antes = std::fs::read(&wal).unwrap();
    let error = match Almacen::abrir(&ruta) {
        Ok(_) => panic!("una versión futura no se abre"),
        Err(error) => error,
    };
    let mensaje = format!("{error:#}");
    assert!(
        mensaje.contains("esta base de datos es de una versión más nueva de MAGI; actualiza MAGI"),
        "{mensaje}"
    );
    // El par db+wal queda byte a byte como estaba y el WAL no se recupera.
    assert_eq!(std::fs::read(&ruta).unwrap(), db_antes, "magi.db intacta");
    assert_eq!(
        std::fs::read(&wal).unwrap(),
        wal_antes,
        "magi.db-wal intacto"
    );
}

/// La versión de esquema que conoce el binario sí se abre (control del test
/// anterior).
#[test]
fn la_version_de_esquema_actual_se_abre() {
    let directorio = tempfile::tempdir().unwrap();
    let ruta = directorio.path().join("magi.db");
    Almacen::abrir(&ruta).unwrap().cerrar().unwrap();
    let almacen = Almacen::abrir(&ruta).expect("la versión actual se abre");
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(version, magi::almacen::migraciones::VERSION_BD);
}

/// El servidor de sesiones también se niega a abrir una `magi.db` de una
/// versión futura: sale con error, no deja socket huérfano y no toca el
/// fichero (Fase 9, cliente y servidor).
#[test]
fn el_servidor_rechaza_una_base_de_datos_de_version_futura() {
    let directorio = tempfile::tempdir().unwrap();
    let raiz = directorio.path();
    let rutas = magi::config::Rutas {
        datos: raiz.join("datos").join("magi"),
        config: raiz.join("config").join("magi"),
        estado: raiz.join("estado").join("magi"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime").join("magi"),
    };
    std::fs::create_dir_all(rutas.base_datos().parent().unwrap()).unwrap();
    {
        let conexion = rusqlite::Connection::open(rutas.base_datos()).unwrap();
        conexion
            .execute_batch(
                "CREATE TABLE MARCA (valor TEXT); \
                 INSERT INTO MARCA VALUES ('intacta'); \
                 PRAGMA user_version = 99;",
            )
            .unwrap();
    }
    let antes = std::fs::read(rutas.base_datos()).unwrap();
    let salida = std::process::Command::new(env!("CARGO_BIN_EXE_magi"))
        .arg("--servidor")
        .env("HOME", raiz.join("hogar"))
        .env("XDG_DATA_HOME", raiz.join("datos"))
        .env("XDG_CONFIG_HOME", raiz.join("config"))
        .env("XDG_STATE_HOME", raiz.join("estado"))
        .env("XDG_RUNTIME_DIR", raiz.join("runtime"))
        .stdin(std::process::Stdio::null())
        .output()
        .expect("lanzando magi --servidor");
    assert!(!salida.status.success(), "salida: {:?}", salida.status);
    let texto = format!(
        "{}{}",
        String::from_utf8_lossy(&salida.stdout),
        String::from_utf8_lossy(&salida.stderr)
    );
    assert!(
        texto.contains("esta base de datos es de una versión más nueva de MAGI; actualiza MAGI"),
        "{texto}"
    );
    assert_eq!(
        std::fs::read(rutas.base_datos()).unwrap(),
        antes,
        "el fichero queda intacto"
    );
    assert!(
        !magi::servidor::ruta_socket(&rutas).exists(),
        "no puede quedar un socket huérfano"
    );
}

// ------------------------------------------------------- Fase 10: versiones

/// uuid del equipo de este almacén (la única fila de EQUIPOS con `es_este = 1`).
fn uuid_del_equipo(almacen: &Almacen) -> String {
    magi::almacen::equipos::este(almacen.conexion())
        .expect("fila de este equipo")
        .uuid
}

/// (uuid, version, vector) de una fila de una tabla que viaja.
fn version_de_fila(almacen: &Almacen, tabla: &str, id: i64) -> (String, String, String) {
    almacen
        .conexion()
        .query_row(
            &format!("SELECT uuid, version, vector FROM {tabla} WHERE id = ?1"),
            [id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .unwrap()
}

/// Milisegundos de una versión `<ms>.<uuid del equipo>`.
fn ms_de_version(version: &str) -> i64 {
    let (ms, uuid) = version.split_once('.').expect("versión con ms y equipo");
    assert!(
        !uuid.is_empty(),
        "la versión lleva el uuid del equipo: {version}"
    );
    assert_eq!(
        ms.len(),
        13,
        "los ms de la versión van con 13 cifras: {version}"
    );
    ms.parse().unwrap()
}

fn vector_de(texto: &str) -> std::collections::BTreeMap<String, i64> {
    serde_json::from_str(texto).unwrap()
}

/// Comprueba que una fila nació con uuid, versión y vector de este equipo.
fn comprobar_nueva(almacen: &Almacen, tabla: &str, id: i64) {
    let (uuid, version, vector) = version_de_fila(almacen, tabla, id);
    let equipo = uuid_del_equipo(almacen);
    assert!(!uuid.is_empty(), "{tabla} {id} sin uuid");
    let ms = ms_de_version(&version);
    assert_eq!(
        version.split_once('.').unwrap().1,
        equipo,
        "{tabla} {id} no lleva el uuid de este equipo en la versión"
    );
    assert_eq!(
        vector_de(&vector).get(&equipo),
        Some(&ms),
        "{tabla} {id} no tiene la entrada del vector de este equipo"
    );
}

/// Comprueba que una edición subió la versión de la fila: los ms crecen, la
/// versión la firma este equipo y su entrada del vector avanza con ella.
/// Devuelve la versión nueva para encadenar comprobaciones.
fn comprobar_sube(almacen: &Almacen, tabla: &str, id: i64, version_antes: &str) -> String {
    let (_, version, vector) = version_de_fila(almacen, tabla, id);
    let equipo = uuid_del_equipo(almacen);
    let ms = ms_de_version(&version);
    assert!(
        ms > ms_de_version(version_antes),
        "{tabla} {id} no subió la versión: {version_antes} -> {version}"
    );
    assert_eq!(
        version.split_once('.').unwrap().1,
        equipo,
        "{tabla} {id} no lleva el uuid de este equipo en la versión"
    );
    assert_eq!(vector_de(&vector).get(&equipo), Some(&ms));
    version
}

/// Una base de la fase 9 (`user_version = 6`) sube a la 7: todo lo que ya
/// existía recibe uuid, versión y vector de este equipo, las identidades
/// llevan el UUIDv5 de su huella y se estrenan EQUIPOS (una sola fila, la de
/// este equipo), LAPIDAS y CONFLICTOS.
#[test]
fn la_migracion_7_rellena_versiones_y_estrena_equipos_lapidas_y_conflictos() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta).unwrap();
        for migracion in &magi::almacen::migraciones::MIGRACIONES[..6] {
            conexion.execute_batch(migracion).unwrap();
        }
        conexion.pragma_update(None, "user_version", 6).unwrap();
        conexion
            .execute_batch(
                "INSERT INTO GRUPOS (nombre, orden, plegado, creado_en)
                 VALUES ('viejo', 1, 0, '2026-01-01T00:00:00+01:00');
                 INSERT INTO HOSTS (nombre, grupo_id, direccion, puerto, opciones_extra,
                                    origen, creado_en, actualizado_en)
                 VALUES ('host-viejo', 1, '192.0.2.10', 22, '', 'manual',
                         '2026-01-01T00:00:00+01:00', '2026-01-01T00:00:00+01:00');
                 INSERT INTO IDENTIDADES (alias, tipo, huella, origen, anadida_en)
                 VALUES ('clave-vieja', 'ed25519', 'SHA256:vieja', 'agente',
                         '2026-01-01T00:00:00+01:00');
                 INSERT INTO TUNELES (host_id, nombre, tipo, escucha, destino, automatico,
                                      creado_en, actualizado_en)
                 VALUES (1, 'pg', 'local', '127.0.0.1:5432', '192.0.2.5:5432', 0,
                         '2026-01-01T00:00:00+01:00', '2026-01-01T00:00:00+01:00');
                 INSERT INTO SNIPPETS (nombre, comando, creado_en, actualizado_en)
                 VALUES ('uptime', 'uptime',
                         '2026-01-01T00:00:00+01:00', '2026-01-01T00:00:00+01:00');
                 INSERT INTO SINCRONIZACIONES_DIR (host_id, nombre, ruta_local, ruta_remota,
                                                   direccion, borrar, exclusiones,
                                                   creado_en, actualizado_en)
                 VALUES (1, 'web', '/tmp/web', '/var/www', 'subida', 0, '',
                         '2026-01-01T00:00:00+01:00', '2026-01-01T00:00:00+01:00');",
            )
            .unwrap();
    }
    let almacen = Almacen::abrir(&ruta).unwrap();
    let version_esquema: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(version_esquema, 7);

    // Una sola fila de este equipo, con uuid, estado y reloj.
    let equipos = magi::almacen::equipos::listar(almacen.conexion()).unwrap();
    assert_eq!(
        equipos.len(),
        1,
        "EQUIPOS estrena solo la fila de este equipo"
    );
    assert!(equipos[0].es_este);
    assert_eq!(equipos[0].estado, magi::almacen::equipos::ESTADO_ESTE);
    assert!(!equipos[0].uuid.is_empty());
    assert!(equipos[0].reloj_ms.is_some());

    // Todas las filas que ya existían tienen uuid, versión y vector.
    for (tabla, id) in [
        ("GRUPOS", 1),
        ("HOSTS", 1),
        ("IDENTIDADES", 1),
        ("TUNELES", 1),
        ("SNIPPETS", 1),
        ("SINCRONIZACIONES_DIR", 1),
    ] {
        comprobar_nueva(&almacen, tabla, id);
    }
    // La identidad lleva el UUIDv5 de su huella, igual en todos los equipos.
    let (uuid_identidad, _, _) = version_de_fila(&almacen, "IDENTIDADES", 1);
    assert_eq!(
        uuid_identidad,
        magi::almacen::identidades::uuid_de_huella("SHA256:vieja")
    );
    // Las tablas nuevas de la sincronización existen y están vacías.
    assert_eq!(
        magi::almacen::lapidas::contar(almacen.conexion()).unwrap(),
        0
    );
    let conflictos: i64 = almacen
        .conexion()
        .query_row("SELECT COUNT(*) FROM CONFLICTOS", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(conflictos, 0);
}

/// Crear cualquier elemento de las seis tablas que viajan deja uuid, versión y
/// vector de este equipo; los uuid son distintos entre filas y el de la
/// identidad es el UUIDv5 de su huella.
#[test]
fn crear_deja_version_y_vector_de_este_equipo_en_cada_tabla() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let grupo = almacen.crear_grupo("produccion").unwrap();
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
    let tunel = almacen
        .crear_tunel(&tunel_de(host, "pg", TipoTunel::Local, "127.0.0.1:5432"))
        .unwrap();
    let snippet = almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();
    let sincronizacion = almacen
        .crear_sincronizacion(&sincronizacion_de(host, "web"))
        .unwrap();

    let filas = [
        ("GRUPOS", grupo),
        ("HOSTS", host),
        ("IDENTIDADES", identidad),
        ("TUNELES", tunel),
        ("SNIPPETS", snippet),
        ("SINCRONIZACIONES_DIR", sincronizacion),
    ];
    for (tabla, id) in filas {
        comprobar_nueva(&almacen, tabla, id);
    }
    let mut uuids = std::collections::HashSet::new();
    for (tabla, id) in filas {
        uuids.insert(version_de_fila(&almacen, tabla, id).0);
    }
    assert_eq!(uuids.len(), filas.len(), "cada fila tiene su uuid");
    assert_eq!(
        version_de_fila(&almacen, "IDENTIDADES", identidad).0,
        magi::almacen::identidades::uuid_de_huella("SHA256:abc")
    );
}

/// Conectar, cambiar el estado, navegar por SFTP, plegar un grupo, contar usos
/// de identidad y snippet o lanzar una sincronización guardada son campos
/// propios del equipo: no tocan la versión ni el vector (Fase 10).
#[test]
fn conectar_navegar_y_usar_no_cambian_la_version() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let grupo = almacen.crear_grupo("web").unwrap();
    let identidad = almacen
        .crear_identidad(
            "ed25519",
            "SHA256:abc",
            magi::modelo::OrigenIdentidad::Agente,
            None,
            Some("prueba"),
        )
        .unwrap();
    let snippet = almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();
    let sincronizacion = almacen
        .crear_sincronizacion(&sincronizacion_de(host, "web"))
        .unwrap();

    let antes_host = version_de_fila(&almacen, "HOSTS", host).1;
    let antes_grupo = version_de_fila(&almacen, "GRUPOS", grupo).1;
    let antes_identidad = version_de_fila(&almacen, "IDENTIDADES", identidad).1;
    let antes_snippet = version_de_fila(&almacen, "SNIPPETS", snippet).1;
    let antes_sincronizacion = version_de_fila(&almacen, "SINCRONIZACIONES_DIR", sincronizacion).1;

    almacen.marcar_conexion(host).unwrap();
    almacen
        .marcar_estado(host, Some(UltimoEstado::Error))
        .unwrap();
    almacen
        .fijar_dirs_sftp(host, Some("/home/usuario"), Some("/srv"))
        .unwrap();
    almacen.alternar_plegado(grupo, true).unwrap();
    almacen.marcar_uso_identidad("SHA256:abc").unwrap();
    almacen.marcar_uso_snippet(snippet).unwrap();
    almacen
        .marcar_ejecucion_sincronizacion(sincronizacion)
        .unwrap();
    magi::almacen::sincronizaciones::fijar_resultado(
        almacen.conexion(),
        sincronizacion,
        ResultadoSincronizacion::Ok,
    )
    .unwrap();

    assert_eq!(version_de_fila(&almacen, "HOSTS", host).1, antes_host);
    assert_eq!(version_de_fila(&almacen, "GRUPOS", grupo).1, antes_grupo);
    assert_eq!(
        version_de_fila(&almacen, "IDENTIDADES", identidad).1,
        antes_identidad
    );
    assert_eq!(
        version_de_fila(&almacen, "SNIPPETS", snippet).1,
        antes_snippet
    );
    assert_eq!(
        version_de_fila(&almacen, "SINCRONIZACIONES_DIR", sincronizacion).1,
        antes_sincronizacion
    );
}

/// Cada escritura del almacén sobre un campo del host que viaja sube su
/// versión; repetir la misma escritura no la sube.
#[test]
fn editar_un_campo_que_viaja_del_host_sube_la_version() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let otro = almacen.crear_host(&datos("dos"), Origen::Manual).unwrap();
    let grupo = almacen.crear_grupo("web").unwrap();
    let snippet = almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();

    // Repetir la ficha tal cual no es una edición.
    let version = version_de_fila(&almacen, "HOSTS", host).1;
    almacen.actualizar_host(host, &datos("uno")).unwrap();
    assert_eq!(version_de_fila(&almacen, "HOSTS", host).1, version);

    let mut ficha = datos("uno");
    ficha.direccion = "192.0.2.99".to_string();
    almacen.actualizar_host(host, &ficha).unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    almacen.mover_host_a_grupo(host, Some(grupo)).unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    almacen.marcar_origen(host, Origen::SshConfig).unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    almacen.fijar_salto(host, Some(otro)).unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    almacen
        .anadir_opciones_extra(host, "ServerAliveInterval 10")
        .unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    almacen
        .marcar_identidad_ref(host, &IdentidadRef::Agente("SHA256:abc".to_string()))
        .unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    almacen
        .fijar_snippet_al_conectar(host, Some(snippet))
        .unwrap();
    comprobar_sube(&almacen, "HOSTS", host, &version);
}

/// Las etiquetas y las verificaciones viajan dentro del host: cambiarlas sube
/// su versión (checklist de la Fase 10).
#[test]
fn cambiar_etiquetas_o_verificaciones_sube_la_version_del_host() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let version = version_de_fila(&almacen, "HOSTS", host).1;

    let mut ficha = datos("uno");
    ficha.etiquetas = vec!["web".to_string(), "nueva".to_string()];
    almacen.actualizar_host(host, &ficha).unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    let verificaciones = DatosVerificaciones {
        salud: true,
        ..DatosVerificaciones::default()
    };
    almacen
        .guardar_verificaciones(host, &verificaciones)
        .unwrap();
    let version = comprobar_sube(&almacen, "HOSTS", host, &version);

    // Guardar las mismas verificaciones no es una edición.
    almacen
        .guardar_verificaciones(host, &verificaciones)
        .unwrap();
    assert_eq!(version_de_fila(&almacen, "HOSTS", host).1, version);
}

/// Grupos (renombrar y mover), identidades (renombrar, revocar y reactivar),
/// túneles, snippets y sincronizaciones suben su versión al editar un campo
/// que viaja; repetir la edición no la sube.
#[test]
fn editar_grupo_identidad_tunel_snippet_o_sincronizacion_sube_su_version() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let grupo = almacen.crear_grupo("web").unwrap();
    let otro_grupo = almacen.crear_grupo("datos").unwrap();
    let identidad = almacen
        .crear_identidad(
            "ed25519",
            "SHA256:abc",
            magi::modelo::OrigenIdentidad::Agente,
            None,
            Some("prueba"),
        )
        .unwrap();
    let tunel = almacen
        .crear_tunel(&tunel_de(host, "pg", TipoTunel::Local, "127.0.0.1:5432"))
        .unwrap();
    let snippet = almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();
    let sincronizacion = almacen
        .crear_sincronizacion(&sincronizacion_de(host, "web"))
        .unwrap();

    let version = version_de_fila(&almacen, "GRUPOS", grupo).1;
    almacen.renombrar_grupo(grupo, "web-prod").unwrap();
    let version = comprobar_sube(&almacen, "GRUPOS", grupo, &version);
    almacen.mover_grupo(otro_grupo, -1).unwrap();
    comprobar_sube(&almacen, "GRUPOS", grupo, &version);

    let version = version_de_fila(&almacen, "IDENTIDADES", identidad).1;
    almacen
        .renombrar_identidad(identidad, "clave-nueva")
        .unwrap();
    let version = comprobar_sube(&almacen, "IDENTIDADES", identidad, &version);
    // El host usa la identidad: al revocarla se queda en `auto` y eso viaja.
    almacen
        .marcar_identidad_ref(host, &IdentidadRef::Agente("SHA256:abc".to_string()))
        .unwrap();
    let version_host = version_de_fila(&almacen, "HOSTS", host).1;
    almacen
        .revocar_identidad(identidad, std::path::Path::new("/home/nadie"))
        .unwrap();
    comprobar_sube(&almacen, "IDENTIDADES", identidad, &version);
    comprobar_sube(&almacen, "HOSTS", host, &version_host);
    almacen.reactivar_identidad(identidad).unwrap();

    let version = version_de_fila(&almacen, "TUNELES", tunel).1;
    let mut datos_tunel = tunel_de(host, "pg", TipoTunel::Local, "127.0.0.1:5432");
    datos_tunel.escucha = "127.0.0.1:15432".to_string();
    almacen.actualizar_tunel(tunel, &datos_tunel).unwrap();
    let version = comprobar_sube(&almacen, "TUNELES", tunel, &version);
    almacen.alternar_tunel_automatico(tunel, true).unwrap();
    comprobar_sube(&almacen, "TUNELES", tunel, &version);

    let version = version_de_fila(&almacen, "SNIPPETS", snippet).1;
    let mut datos_snippet = snippet_de("reiniciar", vec![etiqueta("datos")]);
    datos_snippet.comando = "systemctl restart postgresql".to_string();
    almacen.actualizar_snippet(snippet, &datos_snippet).unwrap();
    let version = comprobar_sube(&almacen, "SNIPPETS", snippet, &version);
    // Los destinos van dentro del snippet: cambiarlos también es editar.
    let datos_snippet = snippet_de("reiniciar", vec![suelto(host, "uno")]);
    almacen.actualizar_snippet(snippet, &datos_snippet).unwrap();
    let version = comprobar_sube(&almacen, "SNIPPETS", snippet, &version);
    // Repetir la misma edición no sube la versión.
    almacen.actualizar_snippet(snippet, &datos_snippet).unwrap();
    assert_eq!(version_de_fila(&almacen, "SNIPPETS", snippet).1, version);

    let version = version_de_fila(&almacen, "SINCRONIZACIONES_DIR", sincronizacion).1;
    let mut datos_sincronizacion = sincronizacion_de(host, "web");
    datos_sincronizacion.borrar = true;
    almacen
        .actualizar_sincronizacion(sincronizacion, &datos_sincronizacion)
        .unwrap();
    comprobar_sube(&almacen, "SINCRONIZACIONES_DIR", sincronizacion, &version);
}

/// Borrar un host deja lápida suya y de sus túneles y sincronizaciones, que
/// caen en cascada; cada lápida parte del vector del elemento borrado.
#[test]
fn borrar_un_host_deja_lapida_suya_y_de_sus_tuneles_y_sincronizaciones() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let tunel = almacen
        .crear_tunel(&tunel_de(host, "pg", TipoTunel::Local, "127.0.0.1:5432"))
        .unwrap();
    let sincronizacion = almacen
        .crear_sincronizacion(&sincronizacion_de(host, "web"))
        .unwrap();
    let uuid_host = version_de_fila(&almacen, "HOSTS", host).0;
    let uuid_tunel = version_de_fila(&almacen, "TUNELES", tunel).0;
    let uuid_sincronizacion = version_de_fila(&almacen, "SINCRONIZACIONES_DIR", sincronizacion).0;
    let version_host = version_de_fila(&almacen, "HOSTS", host).1;

    almacen.borrar_host(host).unwrap();

    let equipo = uuid_del_equipo(&almacen);
    let lapida_host = magi::almacen::lapidas::obtener(almacen.conexion(), &uuid_host)
        .unwrap()
        .expect("lápida del host");
    assert_eq!(lapida_host.tipo, "host");
    assert_eq!(lapida_host.nombre, "uno");
    assert!(
        ms_de_version(&lapida_host.version) > ms_de_version(&version_host),
        "la lápida es posterior a la última edición"
    );
    assert!(vector_de(&lapida_host.vector).contains_key(&equipo));
    let lapida_tunel = magi::almacen::lapidas::obtener(almacen.conexion(), &uuid_tunel)
        .unwrap()
        .expect("lápida del túnel");
    assert_eq!(lapida_tunel.tipo, "tunel");
    assert_eq!(lapida_tunel.nombre, "pg");
    let lapida_sincronizacion =
        magi::almacen::lapidas::obtener(almacen.conexion(), &uuid_sincronizacion)
            .unwrap()
            .expect("lápida de la sincronización");
    assert_eq!(lapida_sincronizacion.tipo, "sincronizacion_dir");
    assert_eq!(lapida_sincronizacion.nombre, "web");
    assert_eq!(
        magi::almacen::lapidas::contar(almacen.conexion()).unwrap(),
        3
    );
    assert!(almacen.listar_tuneles().unwrap().is_empty());
    assert!(almacen.listar_sincronizaciones().unwrap().is_empty());
}

/// Borrar un grupo sube la versión de los hosts que se quedan sin él (el
/// `ON DELETE SET NULL` cuenta como edición suya) y deja la lápida del grupo.
#[test]
fn borrar_un_grupo_sube_la_version_de_los_hosts_que_se_quedan_sin_el() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let grupo = almacen.crear_grupo("web").unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    almacen.mover_host_a_grupo(host, Some(grupo)).unwrap();
    let uuid_grupo = version_de_fila(&almacen, "GRUPOS", grupo).0;
    let version_host = version_de_fila(&almacen, "HOSTS", host).1;

    almacen.borrar_grupo(grupo).unwrap();

    comprobar_sube(&almacen, "HOSTS", host, &version_host);
    assert_eq!(almacen.obtener_host(host).unwrap().grupo_id, None);
    let lapida = magi::almacen::lapidas::obtener(almacen.conexion(), &uuid_grupo)
        .unwrap()
        .expect("lápida del grupo");
    assert_eq!(lapida.tipo, "grupo");
    assert_eq!(lapida.nombre, "web");
}

/// Borrar el host por el que saltaban otros sube la versión de esos hosts: el
/// `ON DELETE SET NULL` de `salto_host_id` cuenta como edición.
#[test]
fn borrar_un_host_sube_la_version_de_los_que_saltaban_por_el() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let salto = almacen.crear_host(&datos("salto"), Origen::Manual).unwrap();
    let mut ficha = datos("detras");
    ficha.salto_host_id = Some(salto);
    let detras = almacen.crear_host(&ficha, Origen::Manual).unwrap();
    let version_detras = version_de_fila(&almacen, "HOSTS", detras).1;

    almacen.borrar_host(salto).unwrap();

    comprobar_sube(&almacen, "HOSTS", detras, &version_detras);
    assert_eq!(almacen.obtener_host(detras).unwrap().salto_host_id, None);
}

/// Borrar un snippet sube la versión de los hosts que lo tenían como «snippet
/// al conectar» (el `ON DELETE SET NULL` cuenta como edición) y deja su
/// lápida.
#[test]
fn borrar_un_snippet_sube_la_version_de_los_hosts_que_lo_usaban_al_conectar() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let snippet = almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();
    almacen
        .fijar_snippet_al_conectar(host, Some(snippet))
        .unwrap();
    let uuid_snippet = version_de_fila(&almacen, "SNIPPETS", snippet).0;
    let version_host = version_de_fila(&almacen, "HOSTS", host).1;

    almacen.borrar_snippet(snippet).unwrap();

    comprobar_sube(&almacen, "HOSTS", host, &version_host);
    assert_eq!(
        almacen.obtener_host(host).unwrap().snippet_al_conectar_id,
        None
    );
    let lapida = magi::almacen::lapidas::obtener(almacen.conexion(), &uuid_snippet)
        .unwrap()
        .expect("lápida del snippet");
    assert_eq!(lapida.tipo, "snippet");
    assert_eq!(lapida.nombre, "reiniciar");
}

/// Borrar un grupo, un túnel, un snippet o una sincronización deja su lápida
/// con el nombre y el tipo que le toca.
#[test]
fn borrar_deja_lapida_con_su_tipo_y_nombre() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let grupo = almacen.crear_grupo("web").unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let tunel = almacen
        .crear_tunel(&tunel_de(host, "pg", TipoTunel::Local, "127.0.0.1:5432"))
        .unwrap();
    let snippet = almacen
        .crear_snippet(&snippet_de("reiniciar", vec![etiqueta("web")]))
        .unwrap();
    let sincronizacion = almacen
        .crear_sincronizacion(&sincronizacion_de(host, "web"))
        .unwrap();
    let uuid_tunel = version_de_fila(&almacen, "TUNELES", tunel).0;
    let uuid_snippet = version_de_fila(&almacen, "SNIPPETS", snippet).0;
    let uuid_sincronizacion = version_de_fila(&almacen, "SINCRONIZACIONES_DIR", sincronizacion).0;
    let uuid_grupo = version_de_fila(&almacen, "GRUPOS", grupo).0;

    almacen.borrar_tunel(tunel).unwrap();
    almacen.borrar_snippet(snippet).unwrap();
    almacen.borrar_sincronizacion(sincronizacion).unwrap();
    almacen.borrar_grupo(grupo).unwrap();

    for (uuid, tipo, nombre) in [
        (uuid_tunel.as_str(), "tunel", "pg"),
        (uuid_snippet.as_str(), "snippet", "reiniciar"),
        (uuid_sincronizacion.as_str(), "sincronizacion_dir", "web"),
        (uuid_grupo.as_str(), "grupo", "web"),
    ] {
        let lapida = magi::almacen::lapidas::obtener(almacen.conexion(), uuid)
            .unwrap()
            .unwrap_or_else(|| panic!("falta la lápida de {tipo}"));
        assert_eq!(lapida.tipo, tipo);
        assert_eq!(lapida.nombre, nombre);
        assert!(!lapida.version.is_empty());
        assert!(vector_de(&lapida.vector).contains_key(&uuid_del_equipo(&almacen)));
        assert!(!lapida.borrado_en.is_empty());
    }
    assert_eq!(
        magi::almacen::lapidas::contar(almacen.conexion()).unwrap(),
        4
    );
}

/// Borrar algo que ya no existe no deja lápida de nada.
#[test]
fn borrar_lo_que_no_existe_no_deja_lapida() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let id = 99_999;
    almacen.borrar_host(id).unwrap();
    almacen.borrar_grupo(id).unwrap();
    almacen.borrar_tunel(id).unwrap();
    almacen.borrar_snippet(id).unwrap();
    almacen.borrar_sincronizacion(id).unwrap();
    assert_eq!(
        magi::almacen::lapidas::contar(almacen.conexion()).unwrap(),
        0
    );
}

/// El uuid de una identidad sale de la huella (UUIDv5): la misma clave lo
/// tiene igual en todos los equipos, se cree con `crear` o con el escaneo.
#[test]
fn el_uuid_de_las_identidades_es_el_mismo_en_todos_los_equipos() {
    let huella = "SHA256:identidad-compartida";
    let esperado = uuid::Uuid::new_v5(
        &magi::sincronizacion::formato::NAMESPACE_IDENTIDADES,
        huella.as_bytes(),
    )
    .to_string();
    assert_eq!(magi::almacen::identidades::uuid_de_huella(huella), esperado);

    let primero = Almacen::abrir_en_memoria().unwrap();
    let uno = primero
        .crear_identidad(
            "ed25519",
            huella,
            magi::modelo::OrigenIdentidad::Agente,
            None,
            Some("4d3"),
        )
        .unwrap();
    let segundo = Almacen::abrir_en_memoria().unwrap();
    let dos = segundo
        .crear_identidad(
            "ed25519",
            huella,
            magi::modelo::OrigenIdentidad::Fichero,
            Some("~/.ssh/id_ed25519"),
            None,
        )
        .unwrap();
    assert_eq!(
        version_de_fila(&primero, "IDENTIDADES", uno).0,
        version_de_fila(&segundo, "IDENTIDADES", dos).0
    );
    // El escaneo del agente llega a la misma identidad (upsert por huella).
    let claves = vec![magi::almacen::identidades::ClaveSincronizada {
        tipo: "ed25519".to_string(),
        huella: huella.to_string(),
        origen: magi::modelo::OrigenIdentidad::Agente,
        ruta: None,
        comentario: Some("4d3".to_string()),
    }];
    primero.sincronizar_identidades(&claves).unwrap();
    let por_huella = primero.identidad_por_huella(huella).unwrap().unwrap();
    assert_eq!(
        version_de_fila(&primero, "IDENTIDADES", por_huella.id).0,
        esperado
    );
    assert_eq!(
        version_de_fila(&primero, "IDENTIDADES", uno).0,
        version_de_fila(&primero, "IDENTIDADES", por_huella.id).0
    );
}

/// Las ediciones de este equipo hacen crecer la versión de forma monótona, el
/// reloj híbrido de EQUIPOS la acompaña y el vector solo ve a este equipo
/// mientras no se importe nada.
#[test]
fn la_version_crece_de_forma_monotona_y_el_reloj_la_acompana() {
    let almacen = Almacen::abrir_en_memoria().unwrap();
    let host = almacen.crear_host(&datos("uno"), Origen::Manual).unwrap();
    let equipo = uuid_del_equipo(&almacen);
    let mut version = version_de_fila(&almacen, "HOSTS", host).1;
    let mut ms = ms_de_version(&version);
    for indice in 0..5 {
        let mut ficha = datos("uno");
        ficha.direccion = format!("192.0.2.{}", indice + 20);
        almacen.actualizar_host(host, &ficha).unwrap();
        let nueva = version_de_fila(&almacen, "HOSTS", host).1;
        let ms_nueva = ms_de_version(&nueva);
        assert!(ms_nueva > ms, "la versión no crece: {version} -> {nueva}");
        assert_eq!(nueva.split_once('.').unwrap().1, equipo);
        let vector = vector_de(&version_de_fila(&almacen, "HOSTS", host).2);
        assert_eq!(vector.len(), 1, "sin importar nada solo ve a este equipo");
        assert_eq!(vector.get(&equipo), Some(&ms_nueva));
        version = nueva;
        ms = ms_nueva;
    }
    let reloj = magi::almacen::versiones::reloj_de_este(almacen.conexion()).unwrap();
    assert!(
        reloj >= ms,
        "el reloj híbrido no se queda por detrás de la última versión"
    );
    // El uuid del elemento no cambia al editar.
    assert_eq!(version_de_fila(&almacen, "HOSTS", host).0.len(), 36);
}
