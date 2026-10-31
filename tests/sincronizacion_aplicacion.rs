//! Pruebas de la aplicación del plan de fusión al almacén (Fase 10, §7.3).
//!
//! Todos los planes se construyen a mano (los Candidato que produce la fusión)
//! contra un Almacen en memoria: así se prueba la escritura, las referencias por
//! uuid, las lápidas, los borrados aplazados, las colisiones de nombre, las
//! etiquetas, la recuperación de conflictos y la unión por nombre sin depender
//! del resto de la sincronización.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::params;

use magi::almacen::versiones::{self, Tabla};
use magi::almacen::{conflictos, equipos, etiquetas, lapidas, verificaciones, Almacen};
use magi::modelo::{self, IdentidadRef, Origen, OrigenIdentidad, TipoTunel};
use magi::protocolo::Direccion;
use magi::sincronizacion::aplicacion::{
    aplicar, recuperar_version, unir_por_nombre, OpcionesAplicacion,
};
use magi::sincronizacion::elementos::{
    DatosGrupo, DatosHost, DatosIdentidad, DatosSincronizacion, DatosSnippet, DatosTunel,
    DatosVerificaciones, DestinoSnippet,
};
use magi::sincronizacion::formato::{Documento, ElementoDocumento, EquipoDocumento};
use magi::sincronizacion::fusion::{Candidato, ConflictoFusion, PlanFusion};

/// Hogar de mentira: las rutas bajo él se expanden sin tocar el disco.
const HOGAR: &str = "/home/prueba";

/// Equipo de los planes de prueba.
const EQUIPO_REMOTO: &str = "equipo-remoto";

/// Equipo que se pone a las filas locales de prueba (el uuid real de EQUIPOS es
/// otro; se reescriben a mano para que las versiones sean deterministas).
const EQUIPO_LOCAL: &str = "equipo-local";

fn hogar() -> PathBuf {
    PathBuf::from(HOGAR)
}

fn almacen() -> Almacen {
    Almacen::abrir_en_memoria().expect("almacén en memoria")
}

fn uuid_este(almacen: &Almacen) -> String {
    equipos::este(almacen.conexion())
        .expect("fila del equipo")
        .uuid
}

fn msde(ms: i64, equipo: &str) -> String {
    format!("{ms:013}.{equipo}")
}

fn vector_json(equipo: &str, ms: i64) -> String {
    format!("{{\"{equipo}\":{ms}}}")
}

fn fijar_metadatos(
    almacen: &Almacen,
    tabla: Tabla,
    id: i64,
    uuid: &str,
    version: &str,
    vector: &str,
) {
    almacen
        .conexion()
        .execute(
            &format!(
                "UPDATE {} SET uuid = ?1, version = ?2, vector = ?3 WHERE id = ?4",
                tabla.nombre()
            ),
            params![uuid, version, vector, id],
        )
        .expect("fijando uuid, versión y vector");
}

fn id_bd(almacen: &Almacen, tabla: Tabla, uuid: &str) -> i64 {
    almacen
        .conexion()
        .query_row(
            &format!("SELECT id FROM {} WHERE uuid = ?1", tabla.nombre()),
            params![uuid],
            |fila| fila.get(0),
        )
        .expect("fila por uuid")
}

fn contar(almacen: &Almacen, tabla: Tabla) -> i64 {
    almacen
        .conexion()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", tabla.nombre()),
            [],
            |fila| fila.get(0),
        )
        .expect("contando filas")
}

fn version_de(almacen: &Almacen, tabla: Tabla, uuid: &str) -> (String, String) {
    almacen
        .conexion()
        .query_row(
            &format!(
                "SELECT version, vector FROM {} WHERE uuid = ?1",
                tabla.nombre()
            ),
            params![uuid],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .expect("versión de la fila")
}

// ------------------------------------------------------- filas locales de prueba

fn host_modelo(nombre: &str, direccion: &str) -> modelo::DatosHost {
    modelo::DatosHost {
        nombre: nombre.to_string(),
        direccion: direccion.to_string(),
        puerto: 22,
        usuario: None,
        identidad_ref: IdentidadRef::Auto,
        grupo_id: None,
        salto_host_id: None,
        multiplexar: false,
        keepalive_seg: Some(30),
        opciones_extra: String::new(),
        servicios: String::new(),
        etiquetas: Vec::new(),
    }
}

fn crear_host_local(almacen: &Almacen, nombre: &str, direccion: &str, uuid: &str, ms: i64) -> i64 {
    let id = almacen
        .crear_host(&host_modelo(nombre, direccion), Origen::Manual)
        .expect("host local");
    fijar_metadatos(
        almacen,
        Tabla::Hosts,
        id,
        uuid,
        &msde(ms, EQUIPO_LOCAL),
        &vector_json(EQUIPO_LOCAL, ms),
    );
    id
}

fn crear_grupo_local(almacen: &Almacen, nombre: &str, uuid: &str, ms: i64) -> i64 {
    let id = almacen.crear_grupo(nombre).expect("grupo local");
    fijar_metadatos(
        almacen,
        Tabla::Grupos,
        id,
        uuid,
        &msde(ms, EQUIPO_LOCAL),
        &vector_json(EQUIPO_LOCAL, ms),
    );
    id
}

fn crear_tunel_local(almacen: &Almacen, host_id: i64, nombre: &str, uuid: &str, ms: i64) -> i64 {
    let datos = modelo::DatosTunel {
        host_id,
        nombre: nombre.to_string(),
        tipo: TipoTunel::Local,
        escucha: "127.0.0.1:8080".to_string(),
        destino: Some("192.0.2.10:80".to_string()),
        automatico: false,
    };
    let id = almacen.crear_tunel(&datos).expect("túnel local");
    fijar_metadatos(
        almacen,
        Tabla::Tuneles,
        id,
        uuid,
        &msde(ms, EQUIPO_LOCAL),
        &vector_json(EQUIPO_LOCAL, ms),
    );
    id
}

fn crear_sinc_local(almacen: &Almacen, host_id: i64, nombre: &str, uuid: &str, ms: i64) -> i64 {
    let datos = modelo::DatosSincronizacion {
        host_id,
        nombre: nombre.to_string(),
        ruta_local: format!("{HOGAR}/proyecto"),
        ruta_remota: "/srv/proyecto".to_string(),
        direccion: Direccion::Subida,
        borrar: false,
        exclusiones: Vec::new(),
    };
    let id = almacen
        .crear_sincronizacion(&datos)
        .expect("sincronización local");
    fijar_metadatos(
        almacen,
        Tabla::SincronizacionesDir,
        id,
        uuid,
        &msde(ms, EQUIPO_LOCAL),
        &vector_json(EQUIPO_LOCAL, ms),
    );
    id
}

fn crear_identidad_local(almacen: &Almacen, huella: &str, alias: &str, uuid: &str, ms: i64) -> i64 {
    let id = almacen
        .crear_identidad(
            "ssh-ed25519",
            huella,
            OrigenIdentidad::Fichero,
            Some("/home/prueba/.ssh/id_ed25519"),
            Some(alias),
        )
        .expect("identidad local");
    fijar_metadatos(
        almacen,
        Tabla::Identidades,
        id,
        uuid,
        &msde(ms, EQUIPO_LOCAL),
        &vector_json(EQUIPO_LOCAL, ms),
    );
    id
}

fn poner_etiqueta_local(almacen: &Almacen, host_id: i64, nombre: &str) {
    almacen
        .conexion()
        .execute(
            "INSERT OR IGNORE INTO ETIQUETAS (nombre) VALUES (?1)",
            params![nombre],
        )
        .expect("etiqueta local");
    let etiqueta_id: i64 = almacen
        .conexion()
        .query_row(
            "SELECT id FROM ETIQUETAS WHERE nombre = ?1",
            params![nombre],
            |fila| fila.get(0),
        )
        .expect("id de la etiqueta");
    almacen
        .conexion()
        .execute(
            "INSERT OR IGNORE INTO HOST_ETIQUETAS (host_id, etiqueta_id) VALUES (?1, ?2)",
            params![host_id, etiqueta_id],
        )
        .expect("etiqueta del host");
}

// ------------------------------------------------------- datos del plan (formato)

fn grupo_datos(nombre: &str, orden: i64) -> DatosGrupo {
    DatosGrupo {
        nombre: nombre.to_string(),
        orden,
    }
}

fn host_datos(nombre: &str, direccion: &str) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: direccion.to_string(),
        puerto: 22,
        usuario: None,
        identidad_ref: None,
        salto: None,
        grupo: None,
        multiplexar: false,
        keepalive_seg: Some(30),
        opciones_extra: String::new(),
        servicios: String::new(),
        origen: "manual".to_string(),
        etiquetas: Vec::new(),
        verificaciones: None,
        snippet_al_conectar: None,
    }
}

fn verificaciones_datos() -> DatosVerificaciones {
    DatosVerificaciones {
        salud: false,
        backup: true,
        backup_ruta: Some("/srv/copias".to_string()),
        backup_patron: Some("*.sql.gz".to_string()),
        tests: false,
        tests_comando: None,
    }
}

fn snippet_datos(nombre: &str) -> DatosSnippet {
    DatosSnippet {
        nombre: nombre.to_string(),
        comando: "uptime".to_string(),
        descripcion: String::new(),
        etiquetas: "sistema".to_string(),
        critico: false,
        timeout_seg: 60,
        parar_al_fallo: false,
        destinos: vec![DestinoSnippet {
            etiqueta: Some("web".to_string()),
            host: None,
        }],
    }
}

fn tunel_datos(host: &str, nombre: &str) -> DatosTunel {
    DatosTunel {
        host: host.to_string(),
        nombre: nombre.to_string(),
        tipo: "local".to_string(),
        escucha: "127.0.0.1:8080".to_string(),
        destino: Some("192.0.2.10:80".to_string()),
        automatico: false,
    }
}

fn sinc_datos(host: &str, nombre: &str) -> DatosSincronizacion {
    DatosSincronizacion {
        host: host.to_string(),
        nombre: nombre.to_string(),
        ruta_local: "~/proyecto".to_string(),
        ruta_remota: "/srv/proyecto".to_string(),
        direccion: "subida".to_string(),
        borrar: false,
        exclusiones: String::new(),
    }
}

fn identidad_datos(huella: &str, alias: &str) -> DatosIdentidad {
    DatosIdentidad {
        huella: huella.to_string(),
        alias: alias.to_string(),
        tipo: "ssh-ed25519".to_string(),
        comentario: None,
        revocada_en: None,
    }
}

// ------------------------------------------------------- plan de fusión a mano

fn vivo<D: serde::Serialize>(
    tipo: &str,
    uuid: &str,
    ms: i64,
    equipo: &str,
    datos: &D,
) -> Candidato {
    Candidato {
        tipo: tipo.to_string(),
        uuid: uuid.to_string(),
        nombre: String::new(),
        version: msde(ms, equipo),
        vector: BTreeMap::from([(equipo.to_string(), ms)]),
        contenido: Some(serde_json::to_value(datos).expect("datos a JSON")),
        borrado_en: None,
    }
}

fn lapida_plan(tipo: &str, uuid: &str, nombre: &str, ms: i64, equipo: &str) -> Candidato {
    Candidato {
        tipo: tipo.to_string(),
        uuid: uuid.to_string(),
        nombre: nombre.to_string(),
        version: msde(ms, equipo),
        vector: BTreeMap::from([(equipo.to_string(), ms)]),
        contenido: None,
        borrado_en: Some("2026-10-31T10:00:00+01:00".to_string()),
    }
}

fn plan(vivos: Vec<Candidato>, lapidas: Vec<Candidato>) -> PlanFusion {
    PlanFusion {
        vivos,
        lapidas,
        ..PlanFusion::default()
    }
}

// ------------------------------------------------------------------- pruebas

#[test]
fn alta_de_host_con_referencias_por_uuid() {
    let almacen = almacen();
    let conexion = almacen.conexion();

    let grupo = grupo_datos("produccion", 3);
    let mut snippet = snippet_datos("arranque");
    snippet.destinos = vec![DestinoSnippet {
        etiqueta: Some("Produccion".to_string()),
        host: None,
    }];
    let mut web = host_datos("web", "192.0.2.10");
    web.grupo = Some("uuid-grupo".to_string());
    web.salto = Some("uuid-que-no-existe".to_string());
    web.snippet_al_conectar = Some("uuid-snippet".to_string());
    web.etiquetas = vec!["Produccion".to_string(), "web".to_string()];
    web.verificaciones = Some(verificaciones_datos());
    let mut db = host_datos("db", "192.0.2.20");
    db.salto = Some("uuid-host-web".to_string());

    let plan = plan(
        vec![
            vivo("grupo", "uuid-grupo", 200, EQUIPO_REMOTO, &grupo),
            vivo("snippet", "uuid-snippet", 200, EQUIPO_REMOTO, &snippet),
            vivo("host", "uuid-host-web", 200, EQUIPO_REMOTO, &web),
            vivo("host", "uuid-host-db", 200, EQUIPO_REMOTO, &db),
        ],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan");

    assert_eq!(resumen.creados, 4);
    assert_eq!(resumen.actualizados, 0);

    let grupo_id = id_bd(&almacen, Tabla::Grupos, "uuid-grupo");
    let snippet_id = id_bd(&almacen, Tabla::Snippets, "uuid-snippet");
    let web_id = id_bd(&almacen, Tabla::Hosts, "uuid-host-web");
    let db_id = id_bd(&almacen, Tabla::Hosts, "uuid-host-db");

    let (grupo_del_web, salto_del_web, snippet_del_web): (Option<i64>, Option<i64>, Option<i64>) =
        conexion
            .query_row(
                "SELECT grupo_id, salto_host_id, snippet_al_conectar_id FROM HOSTS WHERE id = ?1",
                params![web_id],
                |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
            )
            .expect("referencias del host web");
    assert_eq!(grupo_del_web, Some(grupo_id));
    assert_eq!(salto_del_web, None, "un salto inexistente queda vacío");
    assert_eq!(snippet_del_web, Some(snippet_id));

    let salto_del_db: Option<i64> = conexion
        .query_row(
            "SELECT salto_host_id FROM HOSTS WHERE id = ?1",
            params![db_id],
            |fila| fila.get(0),
        )
        .expect("salto del host db");
    assert_eq!(salto_del_db, Some(web_id));

    // Las versiones y vectores se escriben tal cual vienen del plan.
    assert_eq!(
        version_de(&almacen, Tabla::Hosts, "uuid-host-web"),
        (msde(200, EQUIPO_REMOTO), vector_json(EQUIPO_REMOTO, 200))
    );

    assert_eq!(
        etiquetas::de_host(conexion, web_id).expect("etiquetas del host"),
        vec!["produccion".to_string(), "web".to_string()]
    );
    let nombres_de_etiquetas: Vec<String> = etiquetas::listar(conexion)
        .expect("etiquetas")
        .into_iter()
        .map(|etiqueta| etiqueta.nombre)
        .collect();
    assert_eq!(
        nombres_de_etiquetas,
        vec!["produccion".to_string(), "web".to_string()]
    );

    let guardadas = verificaciones::de_host(conexion, web_id)
        .expect("verificaciones")
        .expect("el host tiene verificaciones");
    assert!(guardadas.backup);
    assert_eq!(guardadas.backup_ruta.as_deref(), Some("/srv/copias"));

    let destino: String = conexion
        .query_row(
            "SELECT etiqueta FROM SNIPPET_DESTINOS WHERE snippet_id = ?1",
            params![snippet_id],
            |fila| fila.get(0),
        )
        .expect("destino del snippet");
    assert_eq!(destino, "produccion");
}

#[test]
fn actualizar_preserva_los_campos_locales() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let id = crear_host_local(&almacen, "web", "192.0.2.1", "uuid-web", 100);
    conexion
        .execute(
            "UPDATE HOSTS SET
                ultimo_estado = 'ok',
                ultima_conexion_en = '2026-10-30T10:00:00+01:00',
                sftp_dir_local = '/home/prueba/x',
                sftp_dir_remoto = '/srv',
                creado_en = '2026-01-01T00:00:00+01:00'
              WHERE id = ?1",
            params![id],
        )
        .expect("campos locales");

    let mut datos = host_datos("web", "192.0.2.99");
    datos.usuario = Some("deploy".to_string());
    let plan = plan(
        vec![vivo("host", "uuid-web", 300, EQUIPO_REMOTO, &datos)],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan");
    assert_eq!(
        (resumen.creados, resumen.actualizados, resumen.borrados),
        (0, 1, 0)
    );

    struct Fila {
        direccion: String,
        usuario: Option<String>,
        version: String,
        ultimo_estado: Option<String>,
        ultima_conexion_en: Option<String>,
        dir_local: Option<String>,
        dir_remoto: Option<String>,
        creado_en: String,
    }
    let fila = conexion
        .query_row(
            "SELECT direccion, usuario, version, ultimo_estado, ultima_conexion_en,
                    sftp_dir_local, sftp_dir_remoto, creado_en
               FROM HOSTS WHERE id = ?1",
            params![id],
            |fila| {
                Ok(Fila {
                    direccion: fila.get(0)?,
                    usuario: fila.get(1)?,
                    version: fila.get(2)?,
                    ultimo_estado: fila.get(3)?,
                    ultima_conexion_en: fila.get(4)?,
                    dir_local: fila.get(5)?,
                    dir_remoto: fila.get(6)?,
                    creado_en: fila.get(7)?,
                })
            },
        )
        .expect("fila del host");

    assert_eq!(fila.direccion, "192.0.2.99", "lo que viaja se actualiza");
    assert_eq!(fila.usuario.as_deref(), Some("deploy"));
    assert_eq!(fila.version, msde(300, EQUIPO_REMOTO));
    assert_eq!(fila.ultimo_estado.as_deref(), Some("ok"));
    assert_eq!(
        fila.ultima_conexion_en.as_deref(),
        Some("2026-10-30T10:00:00+01:00")
    );
    assert_eq!(fila.dir_local.as_deref(), Some("/home/prueba/x"));
    assert_eq!(fila.dir_remoto.as_deref(), Some("/srv"));
    assert_eq!(fila.creado_en, "2026-01-01T00:00:00+01:00");

    // Repetir el mismo plan no reescribe nada (idempotente).
    let otra = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan otra vez");
    assert_eq!((otra.creados, otra.actualizados), (0, 0));
}

#[test]
fn borrar_deja_lapida_y_arrastra_lo_hijo() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let host = crear_host_local(&almacen, "web", "192.0.2.1", "uuid-web", 100);
    crear_tunel_local(&almacen, host, "db-local", "uuid-tunel", 100);
    crear_sinc_local(&almacen, host, "proyecto", "uuid-sinc", 100);

    let plan = plan(
        Vec::new(),
        vec![
            lapida_plan("host", "uuid-web", "web", 400, EQUIPO_REMOTO),
            lapida_plan("tunel", "uuid-tunel", "db-local", 400, EQUIPO_REMOTO),
            lapida_plan(
                "sincronizacion_dir",
                "uuid-sinc",
                "proyecto",
                400,
                EQUIPO_REMOTO,
            ),
        ],
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan");

    assert_eq!(resumen.borrados, 3);
    assert_eq!(contar(&almacen, Tabla::Hosts), 0);
    assert_eq!(contar(&almacen, Tabla::Tuneles), 0);
    assert_eq!(contar(&almacen, Tabla::SincronizacionesDir), 0);

    let guardadas = lapidas::listar(conexion).expect("lápidas");
    assert_eq!(guardadas.len(), 3);
    let de_web = lapidas::obtener(conexion, "uuid-web")
        .expect("lápida del host")
        .expect("el host tiene lápida");
    assert_eq!(de_web.version, msde(400, EQUIPO_REMOTO));
    assert_eq!(de_web.tipo, "host");
    assert_eq!(de_web.nombre, "web");
}

#[test]
fn los_borrados_aplazados_no_se_borran() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_host_local(&almacen, "con-trabajo", "192.0.2.1", "uuid-aplazado", 100);
    crear_host_local(&almacen, "sin-trabajo", "192.0.2.2", "uuid-normal", 100);

    let plan = plan(
        Vec::new(),
        vec![
            lapida_plan("host", "uuid-aplazado", "con-trabajo", 400, EQUIPO_REMOTO),
            lapida_plan("host", "uuid-normal", "sin-trabajo", 400, EQUIPO_REMOTO),
        ],
    );
    let mut opciones = OpcionesAplicacion::default();
    opciones
        .borrados_aplazados
        .insert("uuid-aplazado".to_string());
    let resumen = aplicar(conexion, &plan, &opciones, &hogar()).expect("aplicando el plan");

    assert_eq!(resumen.borrados, 1);
    assert_eq!(contar(&almacen, Tabla::Hosts), 1);
    let (nombre, version): (String, String) = conexion
        .query_row(
            "SELECT nombre, version FROM HOSTS WHERE uuid = 'uuid-aplazado'",
            [],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .expect("el host aplazado sigue");
    assert_eq!(nombre, "con-trabajo");
    assert_eq!(version, msde(100, EQUIPO_LOCAL), "se queda como estaba");
    // La lápida se guarda igual: es parte del universo y el borrado podrá
    // aplicarse cuando acabe el trabajo vivo.
    assert!(lapidas::obtener(conexion, "uuid-aplazado")
        .expect("lápida")
        .is_some());
}

#[test]
fn permutar_nombres_no_choca_con_los_unique() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_host_local(&almacen, "web", "192.0.2.1", "uuid-x", 100);
    crear_host_local(&almacen, "db", "192.0.2.2", "uuid-y", 100);

    let plan = plan(
        vec![
            vivo(
                "host",
                "uuid-x",
                300,
                EQUIPO_REMOTO,
                &host_datos("db", "192.0.2.1"),
            ),
            vivo(
                "host",
                "uuid-y",
                300,
                EQUIPO_REMOTO,
                &host_datos("web", "192.0.2.2"),
            ),
        ],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("permutar nombres no debe chocar");

    assert_eq!(resumen.actualizados, 2);
    let nombre_x: String = conexion
        .query_row(
            "SELECT nombre FROM HOSTS WHERE uuid = 'uuid-x'",
            [],
            |fila| fila.get(0),
        )
        .expect("host x");
    let nombre_y: String = conexion
        .query_row(
            "SELECT nombre FROM HOSTS WHERE uuid = 'uuid-y'",
            [],
            |fila| fila.get(0),
        )
        .expect("host y");
    assert_eq!(nombre_x, "db");
    assert_eq!(nombre_y, "web");
}

#[test]
fn borrar_y_crear_el_mismo_nombre_no_choca() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_host_local(&almacen, "web", "192.0.2.1", "uuid-viejo", 100);

    let plan = plan(
        vec![vivo(
            "host",
            "uuid-nuevo",
            300,
            EQUIPO_REMOTO,
            &host_datos("web", "192.0.2.9"),
        )],
        vec![lapida_plan("host", "uuid-viejo", "web", 400, EQUIPO_REMOTO)],
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("borrar y crear el mismo nombre no debe chocar");

    assert_eq!(resumen.creados, 1);
    assert_eq!(resumen.borrados, 1);
    assert_eq!(contar(&almacen, Tabla::Hosts), 1);
    let uuid: String = conexion
        .query_row("SELECT uuid FROM HOSTS WHERE nombre = 'web'", [], |fila| {
            fila.get(0)
        })
        .expect("el host nuevo");
    assert_eq!(uuid, "uuid-nuevo");
}

#[test]
fn reconstruye_las_etiquetas_y_conserva_las_de_los_aplazados() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let web = crear_host_local(&almacen, "web", "192.0.2.1", "uuid-web", 100);
    poner_etiqueta_local(&almacen, web, "vieja");
    conexion
        .execute("INSERT INTO ETIQUETAS (nombre) VALUES ('huerfana')", [])
        .expect("etiqueta huérfana");
    let aplazado = crear_host_local(&almacen, "otro", "192.0.2.2", "uuid-aplazado", 100);
    poner_etiqueta_local(&almacen, aplazado, "conservada");

    let mut datos = host_datos("web", "192.0.2.1");
    datos.etiquetas = vec!["Nueva".to_string(), "web".to_string()];
    let mut snippet = snippet_datos("tarea");
    snippet.destinos = vec![DestinoSnippet {
        etiqueta: Some("produccion".to_string()),
        host: None,
    }];
    let plan = plan(
        vec![
            vivo("host", "uuid-web", 300, EQUIPO_REMOTO, &datos),
            vivo("snippet", "uuid-snippet", 300, EQUIPO_REMOTO, &snippet),
        ],
        vec![lapida_plan(
            "host",
            "uuid-aplazado",
            "otro",
            400,
            EQUIPO_REMOTO,
        )],
    );
    let mut opciones = OpcionesAplicacion::default();
    opciones
        .borrados_aplazados
        .insert("uuid-aplazado".to_string());
    aplicar(conexion, &plan, &opciones, &hogar()).expect("aplicando el plan");

    let mut nombres: Vec<String> = etiquetas::listar(conexion)
        .expect("etiquetas")
        .into_iter()
        .map(|etiqueta| etiqueta.nombre)
        .collect();
    nombres.sort();
    assert_eq!(
        nombres,
        vec![
            "conservada".to_string(),
            "nueva".to_string(),
            "produccion".to_string(),
            "web".to_string()
        ],
        "la huérfana y la vieja se van; queda lo que usan hosts y destinos"
    );
    assert_eq!(
        etiquetas::de_host(conexion, web).expect("etiquetas del host"),
        vec!["nueva".to_string(), "web".to_string()]
    );
    assert_eq!(
        etiquetas::de_host(conexion, aplazado).expect("etiquetas del aplazado"),
        vec!["conservada".to_string()],
        "un borrado aplazado conserva sus etiquetas"
    );
}

#[test]
fn un_huerfano_que_llegara_al_plan_no_se_aplica() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let plan = plan(
        vec![
            vivo(
                "tunel",
                "uuid-tunel",
                300,
                EQUIPO_REMOTO,
                &tunel_datos("uuid-host-inexistente", "db-local"),
            ),
            vivo(
                "sincronizacion_dir",
                "uuid-sinc",
                300,
                EQUIPO_REMOTO,
                &sinc_datos("uuid-host-inexistente", "proyecto"),
            ),
        ],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("un huérfano no debe romper la aplicación");

    assert_eq!(resumen.creados, 0);
    assert_eq!(contar(&almacen, Tabla::Tuneles), 0);
    assert_eq!(contar(&almacen, Tabla::SincronizacionesDir), 0);
}

#[test]
fn recuperar_version_reinserta_el_contenido_y_cierra_el_conflicto() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_host_local(&almacen, "web", "192.0.2.5", "uuid-web", 100);

    let mut perdida = host_datos("web", "192.0.2.5");
    perdida.etiquetas = vec!["produccion".to_string()];
    let perdedora = serde_json::to_string(&perdida).expect("JSON de la versión perdedora");
    let conflicto = ConflictoFusion {
        tipo: "elemento".to_string(),
        elemento_tipo: "host".to_string(),
        uuid: "uuid-web".to_string(),
        nombre: "web".to_string(),
        version_perdedora_json: Some(perdedora),
        version_ganadora: Some(msde(400, EQUIPO_REMOTO)),
        equipo_ganador: Some(EQUIPO_REMOTO.to_string()),
    };
    let plan = PlanFusion {
        vivos: Vec::new(),
        lapidas: vec![lapida_plan("host", "uuid-web", "web", 400, EQUIPO_REMOTO)],
        conflictos: vec![conflicto],
        ..PlanFusion::default()
    };
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan");
    assert_eq!(resumen.borrados, 1);
    assert_eq!(resumen.conflictos, 1);
    assert_eq!(contar(&almacen, Tabla::Hosts), 0);

    let pendientes = conflictos::listar_pendientes(conexion).expect("conflictos pendientes");
    assert_eq!(pendientes.len(), 1);
    recuperar_version(conexion, &pendientes[0], &hogar()).expect("recuperando la versión");

    let id = id_bd(&almacen, Tabla::Hosts, "uuid-web");
    let direccion: String = conexion
        .query_row(
            "SELECT direccion FROM HOSTS WHERE id = ?1",
            params![id],
            |fila| fila.get(0),
        )
        .expect("la fila recuperada");
    assert_eq!(direccion, "192.0.2.5");
    assert_eq!(
        etiquetas::de_host(conexion, id).expect("etiquetas recuperadas"),
        vec!["produccion".to_string()]
    );

    // La versión es nueva de este equipo: supera la de la lápida y su vector
    // lleva la entrada de este equipo.
    let (version, vector) = version_de(&almacen, Tabla::Hosts, "uuid-web");
    assert_ne!(version, msde(400, EQUIPO_REMOTO));
    let mapa = versiones::vector_desde_json(&vector);
    assert!(
        mapa.contains_key(&uuid_este(&almacen)),
        "el vector incluye a este equipo"
    );
    let ms_recuperado = versiones::ms_de_version(&version).expect("versión con forma válida");
    assert!(ms_recuperado > 400, "la recuperación es una edición nueva");

    // El conflicto queda resuelto como recuperado y la lápida permanente sigue.
    let resuelto = conflictos::obtener(conexion, pendientes[0].id)
        .expect("conflicto")
        .expect("el conflicto existe");
    assert_eq!(resuelto.resolucion.as_deref(), Some("recuperado"));
    assert!(lapidas::obtener(conexion, "uuid-web")
        .expect("lápida")
        .is_some());
    assert!(conflictos::listar_pendientes(conexion)
        .expect("pendientes")
        .is_empty());
}

#[test]
fn unir_por_nombre_adopta_el_uuid_y_la_actualizacion_posterior_gana() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let host_local = crear_host_local(&almacen, "web", "192.0.2.1", "uuid-local-host", 100);
    crear_tunel_local(&almacen, host_local, "db-local", "uuid-local-tunel", 100);

    let documento = documento_con(
        vec![
            elemento(
                "host",
                "uuid-fichero-host",
                300,
                &host_datos("web", "192.0.2.2"),
            ),
            elemento(
                "tunel",
                "uuid-fichero-tunel",
                300,
                &tunel_datos("uuid-fichero-host", "db-local"),
            ),
        ],
        Vec::new(),
    );
    let unidos = unir_por_nombre(conexion, std::slice::from_ref(&documento), &hogar())
        .expect("uniendo por nombre");
    assert_eq!(
        unidos, 2,
        "se adopta el host y, en la segunda pasada, su túnel"
    );
    assert_eq!(
        id_bd(&almacen, Tabla::Hosts, "uuid-fichero-host"),
        host_local
    );
    let uuid_tunel: String = conexion
        .query_row(
            "SELECT uuid FROM TUNELES WHERE nombre = 'db-local'",
            [],
            |fila| fila.get(0),
        )
        .expect("el túnel adoptado");
    assert_eq!(uuid_tunel, "uuid-fichero-tunel");

    // Repetir la unión no adopta nada más.
    assert_eq!(
        unir_por_nombre(conexion, std::slice::from_ref(&documento), &hogar())
            .expect("uniendo otra vez"),
        0
    );

    // La fusión posterior trae el contenido del fichero y gana.
    let plan = plan(
        vec![vivo(
            "host",
            "uuid-fichero-host",
            300,
            EQUIPO_REMOTO,
            &host_datos("web", "192.0.2.2"),
        )],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando la fusión");
    assert_eq!(resumen.actualizados, 1);
    let direccion: String = conexion
        .query_row(
            "SELECT direccion FROM HOSTS WHERE uuid = 'uuid-fichero-host'",
            [],
            |fila| fila.get(0),
        )
        .expect("el host actualizado");
    assert_eq!(direccion, "192.0.2.2");
}

#[test]
fn un_renombrado_es_edicion_nueva() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_host_local(&almacen, "web", "192.0.2.1", "uuid-r", 100);

    let plan = PlanFusion {
        vivos: vec![vivo(
            "host",
            "uuid-r",
            300,
            EQUIPO_REMOTO,
            &host_datos("web (2)", "192.0.2.1"),
        )],
        renombrados: vec![("host".to_string(), "uuid-r".to_string())],
        ..PlanFusion::default()
    };
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el renombrado");
    assert_eq!(resumen.actualizados, 1);
    assert_eq!(resumen.renombrados, 1);

    let nombre: String = conexion
        .query_row(
            "SELECT nombre FROM HOSTS WHERE uuid = 'uuid-r'",
            [],
            |fila| fila.get(0),
        )
        .expect("el host renombrado");
    assert_eq!(nombre, "web (2)");

    let (version, vector) = version_de(&almacen, Tabla::Hosts, "uuid-r");
    assert_ne!(version, msde(300, EQUIPO_REMOTO), "es edición nueva");
    let mapa = versiones::vector_desde_json(&vector);
    assert_eq!(mapa.get(EQUIPO_REMOTO), Some(&300));
    assert!(mapa.contains_key(&uuid_este(&almacen)));
}

#[test]
fn los_conflictos_no_se_duplican() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let conflicto = ConflictoFusion {
        tipo: "elemento".to_string(),
        elemento_tipo: "grupo".to_string(),
        uuid: "uuid-conflicto".to_string(),
        nombre: "produccion".to_string(),
        version_perdedora_json: Some("{\"nombre\":\"produccion\",\"orden\":1}".to_string()),
        version_ganadora: Some(msde(300, EQUIPO_REMOTO)),
        equipo_ganador: Some(EQUIPO_REMOTO.to_string()),
    };
    let plan = PlanFusion {
        conflictos: vec![conflicto],
        ..PlanFusion::default()
    };
    let primera = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("primera aplicación");
    assert_eq!(primera.conflictos, 1);
    let segunda = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("segunda aplicación");
    assert_eq!(segunda.conflictos, 0, "el mismo conflicto no se duplica");
    assert_eq!(
        conflictos::contar_pendientes(conexion).expect("contar conflictos"),
        1
    );
}

#[test]
fn la_identidad_conserva_origen_y_ruta_locales() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_identidad_local(
        &almacen,
        "SHA256:huella-de-prueba",
        "clave-uno",
        "uuid-identidad",
        100,
    );
    let mut datos = identidad_datos("SHA256:huella-de-prueba", "clave-renombrada");
    datos.comentario = Some("comentario nuevo".to_string());
    datos.revocada_en = Some("2026-10-31T09:00:00+01:00".to_string());
    let plan = plan(
        vec![vivo(
            "identidad",
            "uuid-identidad",
            300,
            EQUIPO_REMOTO,
            &datos,
        )],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando la identidad");
    assert_eq!(resumen.actualizados, 1);

    let (alias, origen, ruta, comentario, revocada, version): (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    ) = conexion
        .query_row(
            "SELECT alias, origen, ruta, comentario, revocada_en, version
               FROM IDENTIDADES WHERE uuid = 'uuid-identidad'",
            [],
            |fila| {
                Ok((
                    fila.get(0)?,
                    fila.get(1)?,
                    fila.get(2)?,
                    fila.get(3)?,
                    fila.get(4)?,
                    fila.get(5)?,
                ))
            },
        )
        .expect("identidad actualizada");
    assert_eq!(alias, "clave-renombrada");
    assert_eq!(origen, "fichero", "el origen es propio del equipo");
    assert_eq!(ruta.as_deref(), Some("/home/prueba/.ssh/id_ed25519"));
    assert_eq!(comentario.as_deref(), Some("comentario nuevo"));
    assert_eq!(revocada.as_deref(), Some("2026-10-31T09:00:00+01:00"));
    assert_eq!(version, msde(300, EQUIPO_REMOTO));
}

#[test]
fn una_referencia_con_lapida_queda_vacia() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let grupo = crear_grupo_local(&almacen, "produccion", "uuid-grupo", 100);
    let host = crear_host_local(&almacen, "web", "192.0.2.1", "uuid-web", 100);
    conexion
        .execute(
            "UPDATE HOSTS SET grupo_id = ?1 WHERE id = ?2",
            params![grupo, host],
        )
        .expect("grupo del host");

    let mut datos = host_datos("web", "192.0.2.1");
    datos.grupo = Some("uuid-grupo".to_string());
    let plan = plan(
        vec![vivo("host", "uuid-web", 300, EQUIPO_REMOTO, &datos)],
        vec![lapida_plan(
            "grupo",
            "uuid-grupo",
            "produccion",
            400,
            EQUIPO_REMOTO,
        )],
    );
    aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar()).expect("aplicando el plan");

    assert_eq!(contar(&almacen, Tabla::Grupos), 0);
    let grupo_id: Option<i64> = conexion
        .query_row(
            "SELECT grupo_id FROM HOSTS WHERE uuid = 'uuid-web'",
            [],
            |fila| fila.get(0),
        )
        .expect("el host sigue");
    assert_eq!(
        grupo_id, None,
        "la referencia a un grupo borrado queda vacía"
    );
}

#[test]
fn un_plan_invalido_no_deja_nada_a_medias() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let bueno = host_datos("bueno", "192.0.2.1");
    let malo = host_datos("malo", "");
    let plan = plan(
        vec![
            vivo("host", "uuid-bueno", 300, EQUIPO_REMOTO, &bueno),
            vivo("host", "uuid-malo", 300, EQUIPO_REMOTO, &malo),
        ],
        Vec::new(),
    );
    let resultado = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar());
    assert!(resultado.is_err(), "un host sin dirección no es válido");
    assert_eq!(
        contar(&almacen, Tabla::Hosts),
        0,
        "la transacción no deja filas a medias"
    );
}

// ------------------------------------------------- utilidades de los documentos

fn elemento<D: serde::Serialize>(tipo: &str, uuid: &str, ms: i64, datos: &D) -> ElementoDocumento {
    ElementoDocumento {
        tipo: tipo.to_string(),
        uuid: uuid.to_string(),
        version: msde(ms, EQUIPO_REMOTO),
        vector: BTreeMap::from([(EQUIPO_REMOTO.to_string(), ms)]),
        datos: serde_json::to_value(datos).expect("datos a JSON"),
    }
}

fn documento_con(
    elementos: Vec<ElementoDocumento>,
    lapidas: Vec<magi::sincronizacion::formato::LapidaDocumento>,
) -> Documento {
    Documento {
        formato: magi::sincronizacion::formato::FORMATO,
        app: "1.1.0".to_string(),
        equipo: EquipoDocumento {
            uuid: "equipo-otro".to_string(),
            nombre: "otro".to_string(),
            plataforma: "linux".to_string(),
        },
        generado_en: "2026-10-31T10:00:00+00:00".to_string(),
        reloj_ms: 300,
        elementos,
        lapidas,
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    }
}

#[test]
fn expande_las_rutas_del_hogar_y_sube_el_reloj() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let mut host = host_datos("web", "192.0.2.1");
    host.identidad_ref = Some("fichero:~/.ssh/id_ed25519".to_string());
    let mut sinc = sinc_datos("uuid-host", "proyecto");
    sinc.ruta_local = "~/proyecto".to_string();

    let mut plan = plan(
        vec![
            vivo("host", "uuid-host", 300, EQUIPO_REMOTO, &host),
            vivo("sincronizacion_dir", "uuid-sinc", 300, EQUIPO_REMOTO, &sinc),
        ],
        Vec::new(),
    );
    plan.reloj_ms = 777;
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan");
    assert_eq!(resumen.creados, 2);

    let identidad_ref: Option<String> = conexion
        .query_row(
            "SELECT identidad_ref FROM HOSTS WHERE uuid = 'uuid-host'",
            [],
            |fila| fila.get(0),
        )
        .expect("identidad_ref del host");
    assert_eq!(
        identidad_ref.as_deref(),
        Some("fichero:/home/prueba/.ssh/id_ed25519")
    );
    let ruta_local: String = conexion
        .query_row(
            "SELECT ruta_local FROM SINCRONIZACIONES_DIR WHERE uuid = 'uuid-sinc'",
            [],
            |fila| fila.get(0),
        )
        .expect("ruta local de la sincronización");
    assert_eq!(ruta_local, "/home/prueba/proyecto");

    // El reloj híbrido local sube a la mayor versión vista al importar.
    let reloj = equipos::este(conexion)
        .expect("equipo")
        .reloj_ms
        .unwrap_or(0);
    assert!(reloj >= 777, "el reloj sube a la mayor versión vista");
}

#[test]
fn mover_un_tunel_a_otro_host_actualiza_la_referencia() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let viejo = crear_host_local(&almacen, "viejo", "192.0.2.1", "uuid-viejo", 100);
    crear_tunel_local(&almacen, viejo, "db-local", "uuid-tunel", 100);

    let mut tunel = tunel_datos("uuid-nuevo", "db-local");
    tunel.escucha = "127.0.0.1:9090".to_string();
    let plan = plan(
        vec![
            vivo(
                "host",
                "uuid-nuevo",
                300,
                EQUIPO_REMOTO,
                &host_datos("nuevo", "192.0.2.2"),
            ),
            vivo("tunel", "uuid-tunel", 300, EQUIPO_REMOTO, &tunel),
        ],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan");
    assert_eq!(resumen.creados, 1, "se crea el host nuevo");
    assert_eq!(resumen.actualizados, 1, "el túnel cambia de host");

    let nuevo_id = id_bd(&almacen, Tabla::Hosts, "uuid-nuevo");
    let (host_id, escucha): (i64, String) = conexion
        .query_row(
            "SELECT host_id, escucha FROM TUNELES WHERE uuid = 'uuid-tunel'",
            [],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .expect("el túnel movido");
    assert_eq!(host_id, nuevo_id);
    assert_eq!(escucha, "127.0.0.1:9090");
}

#[test]
fn una_lapida_de_un_elemento_que_ya_no_esta_se_guarda_igual() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    let plan = plan(
        Vec::new(),
        vec![lapida_plan(
            "snippet",
            "uuid-ausente",
            "viejo",
            500,
            EQUIPO_REMOTO,
        )],
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("una lápida de algo que no está no rompe");
    assert_eq!(resumen.borrados, 0);
    let guardada = lapidas::obtener(conexion, "uuid-ausente")
        .expect("lápida")
        .expect("la lápida se guarda");
    assert_eq!(guardada.version, msde(500, EQUIPO_REMOTO));
}

#[test]
fn no_borra_filas_que_no_participan_en_la_fusion() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    // Una fila sin versión o vector (anterior a la instrumentación) no la
    // exporta estado_local: no puede considerarse borrada por el plan.
    let sin_metadatos = crear_host_local(&almacen, "sin-metadatos", "192.0.2.1", "uuid-rara", 100);
    conexion
        .execute(
            "UPDATE HOSTS SET version = NULL, vector = NULL WHERE id = ?1",
            params![sin_metadatos],
        )
        .expect("dejando la fila sin metadatos");

    // Una fila ilegible (timeout fuera de rango) tampoco se exporta.
    let datos = magi::snippets::DatosSnippet {
        nombre: "roto".to_string(),
        comando: "uptime".to_string(),
        descripcion: String::new(),
        etiquetas: Vec::new(),
        critico: false,
        timeout_seg: 60,
        parar_al_fallo: false,
        destinos: vec![magi::snippets::Destino::Etiqueta("web".to_string())],
    };
    let snippet = almacen.crear_snippet(&datos).expect("snippet local");
    fijar_metadatos(
        &almacen,
        Tabla::Snippets,
        snippet,
        "uuid-roto",
        &msde(100, EQUIPO_LOCAL),
        &vector_json(EQUIPO_LOCAL, 100),
    );
    conexion
        .execute(
            "UPDATE SNIPPETS SET timeout_seg = -1 WHERE id = ?1",
            params![snippet],
        )
        .expect("corrompiendo el timeout");

    let plan = plan(Vec::new(), Vec::new());
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando un plan vacío");
    assert_eq!(resumen.borrados, 0);
    assert_eq!(
        contar(&almacen, Tabla::Hosts),
        1,
        "la fila rara se conserva"
    );
    assert_eq!(
        contar(&almacen, Tabla::Snippets),
        1,
        "la fila rota se conserva"
    );
    assert_eq!(lapidas::contar(conexion).expect("lápidas"), 0);
}

#[test]
fn actualiza_grupos_y_snippets_y_crea_identidades() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_grupo_local(&almacen, "prod", "uuid-grupo", 100);
    let datos = magi::snippets::DatosSnippet {
        nombre: "arranque".to_string(),
        comando: "uptime".to_string(),
        descripcion: String::new(),
        etiquetas: Vec::new(),
        critico: false,
        timeout_seg: 60,
        parar_al_fallo: false,
        destinos: vec![magi::snippets::Destino::Etiqueta("web".to_string())],
    };
    let snippet = almacen.crear_snippet(&datos).expect("snippet local");
    fijar_metadatos(
        &almacen,
        Tabla::Snippets,
        snippet,
        "uuid-snippet",
        &msde(100, EQUIPO_LOCAL),
        &vector_json(EQUIPO_LOCAL, 100),
    );

    let grupo = grupo_datos("produccion", 7);
    let mut snip = snippet_datos("arranque");
    snip.comando = "df -h".to_string();
    snip.etiquetas = "web, sistema".to_string();
    snip.destinos = vec![DestinoSnippet {
        etiqueta: Some("produccion".to_string()),
        host: None,
    }];
    let identidad = identidad_datos("SHA256:nueva", "clave-nueva");
    let plan = plan(
        vec![
            vivo("grupo", "uuid-grupo", 300, EQUIPO_REMOTO, &grupo),
            vivo("snippet", "uuid-snippet", 300, EQUIPO_REMOTO, &snip),
            vivo(
                "identidad",
                "uuid-identidad-nueva",
                300,
                EQUIPO_REMOTO,
                &identidad,
            ),
        ],
        Vec::new(),
    );
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando el plan");
    assert_eq!(resumen.actualizados, 2);
    assert_eq!(resumen.creados, 1);

    let (nombre_grupo, orden, version_grupo): (String, i64, String) = conexion
        .query_row(
            "SELECT nombre, orden, version FROM GRUPOS WHERE uuid = 'uuid-grupo'",
            [],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("grupo actualizado");
    assert_eq!(nombre_grupo, "produccion");
    assert_eq!(orden, 7);
    assert_eq!(version_grupo, msde(300, EQUIPO_REMOTO));

    let (comando, etiquetas, version_snippet): (String, String, String) = conexion
        .query_row(
            "SELECT comando, etiquetas, version FROM SNIPPETS WHERE uuid = 'uuid-snippet'",
            [],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("snippet actualizado");
    assert_eq!(comando, "df -h");
    assert_eq!(etiquetas, "web sistema");
    assert_eq!(version_snippet, msde(300, EQUIPO_REMOTO));
    let destino: String = conexion
        .query_row(
            "SELECT etiqueta FROM SNIPPET_DESTINOS WHERE snippet_id = ?1",
            params![snippet],
            |fila| fila.get(0),
        )
        .expect("destino sustituido");
    assert_eq!(destino, "produccion");

    let (alias, origen, ruta): (String, String, Option<String>) = conexion
        .query_row(
            "SELECT alias, origen, ruta FROM IDENTIDADES WHERE uuid = 'uuid-identidad-nueva'",
            [],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("identidad importada");
    assert_eq!(alias, "clave-nueva");
    assert_eq!(
        origen, "agente",
        "el origen es local y se rellena por defecto"
    );
    assert_eq!(ruta, None, "la ruta es local y no viaja");
}

#[test]
fn una_fila_exportable_que_el_plan_omite_se_borra_con_lapida_minima() {
    let almacen = almacen();
    let conexion = almacen.conexion();
    crear_host_local(
        &almacen,
        "huerfano-del-plan",
        "192.0.2.1",
        "uuid-omitido",
        100,
    );

    // Un plan vacío no debería omitir una fila exportable; si lo hace, la
    // aplicación deja una lápida mínima con lo que había en vez de perder la
    // fila sin rastro.
    let plan = plan(Vec::new(), Vec::new());
    let resumen = aplicar(conexion, &plan, &OpcionesAplicacion::default(), &hogar())
        .expect("aplicando un plan vacío");
    assert_eq!(resumen.borrados, 1);
    assert_eq!(contar(&almacen, Tabla::Hosts), 0);
    let guardada = lapidas::obtener(conexion, "uuid-omitido")
        .expect("lápida")
        .expect("lápida mínima");
    assert_eq!(guardada.nombre, "huerfano-del-plan");
    assert_eq!(guardada.version, msde(100, EQUIPO_LOCAL));
    assert_eq!(guardada.tipo, "host");
}
