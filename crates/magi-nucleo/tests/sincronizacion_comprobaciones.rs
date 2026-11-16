//! Pruebas de los datos de la Fase 13 (§3.1, §3.2 y §7.10): comprobaciones
//! propias de servicio o proyecto en el almacén, su viaje con el inventario en
//! el formato 3 de la sincronización y los fallos del sondeo ampliado.
//!
//! Todo va contra un almacén en memoria (o una base temporal para la
//! migración); no se abre ninguna conexión.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::params;
use serde_json::json;

use magi_nucleo::almacen::lapidas;
use magi_nucleo::almacen::versiones::{self, formatear_version, Tabla};
use magi_nucleo::almacen::{conflictos, Almacen};
use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::modelo::{DatosHost, Origen, ResultadoSondeo, Sondeo};
use magi_nucleo::rutas::{Equipo, Rutas};
use magi_nucleo::servicios::{
    FallosContenedores, FallosSondeo, FallosUnidades, MotorNombre, Objetivo, TipoObjetivo,
};
use magi_nucleo::sincronizacion::aplicacion::{aplicar, OpcionesAplicacion, ResumenAplicacion};
use magi_nucleo::sincronizacion::documento_propio;
use magi_nucleo::sincronizacion::elementos::formato_a_escribir;
use magi_nucleo::sincronizacion::formato::{
    self, Documento, ElementoDocumento, EquipoDocumento, LapidaDocumento,
};
use magi_nucleo::sincronizacion::fusion::fusionar;

/// Hogar de pruebas: el JSON canónico compara rutas bajo este hogar.
const HOGAR: &str = "/home/pruebas-magi";

/// uuid del equipo ajeno que firma los documentos de prueba.
const EQUIPO_AJENO: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

const UUID_COMPROBACION: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

// --------------------------------------------------------------- utilidades

fn almacen() -> Almacen {
    Almacen::abrir_en_memoria().expect("almacén en memoria")
}

fn crear_host(almacen: &Almacen, nombre: &str) -> i64 {
    almacen
        .crear_host(
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.10".to_string(),
                usuario: Some("usuario".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host ssh")
}

fn crear_host_escritorio(almacen: &Almacen, nombre: &str) -> i64 {
    almacen
        .crear_host(
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.30".to_string(),
                puerto: 3389,
                tipo: TipoHost::Rdp,
                usuario: Some("operador".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host de escritorio")
}

/// uuid, versión y vector de una fila de una tabla que viaja.
fn fila_versionada(almacen: &Almacen, tabla: Tabla, id: i64) -> (String, String, String) {
    almacen
        .conexion()
        .query_row(
            &format!(
                "SELECT uuid, version, vector FROM {} WHERE id = ?1",
                tabla.nombre()
            ),
            params![id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("leyendo uuid, versión y vector")
}

fn ms_de(version: &str) -> i64 {
    versiones::ms_de_version(version).expect("versión con la forma esperada")
}

fn version_ajena(ms: i64) -> String {
    formatear_version(ms, EQUIPO_AJENO)
}

fn vector_ajeno(ms: i64) -> BTreeMap<String, i64> {
    [(EQUIPO_AJENO.to_string(), ms)].into_iter().collect()
}

/// Datos que viajan de un host SSH de otro equipo (formato 2).
fn datos_host_documento(nombre: &str, direccion: &str) -> serde_json::Value {
    serde_json::to_value(magi_nucleo::sincronizacion::elementos::DatosHost {
        nombre: nombre.to_string(),
        direccion: direccion.to_string(),
        puerto: 22,
        tipo: TipoHost::Ssh,
        escritorio_dominio: None,
        escritorio_opciones: json!({}),
        usuario: None,
        identidad_ref: None,
        salto: None,
        grupo: None,
        multiplexar: false,
        keepalive_seg: None,
        opciones_extra: String::new(),
        servicios: String::new(),
        origen: "manual".to_string(),
        etiquetas: Vec::new(),
        verificaciones: None,
        snippet_al_conectar: None,
    })
    .expect("datos de host a JSON")
}

fn documento_con(
    formato: u32,
    elementos: Vec<ElementoDocumento>,
    lapidas: Vec<LapidaDocumento>,
) -> Documento {
    Documento {
        formato,
        app: "1.3.0".to_string(),
        equipo: EquipoDocumento {
            uuid: EQUIPO_AJENO.to_string(),
            nombre: "equipo-ajeno".to_string(),
            plataforma: "linux".to_string(),
        },
        generado_en: "2026-11-08T10:00:00+01:00".to_string(),
        reloj_ms: 1_000,
        elementos,
        lapidas,
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    }
}

fn elemento_comprobacion(
    uuid: &str,
    host: &str,
    tipo: &str,
    objetivo: &str,
    comando: &str,
    ms: i64,
) -> ElementoDocumento {
    ElementoDocumento {
        tipo: "comprobacion_servicio".to_string(),
        uuid: uuid.to_string(),
        version: version_ajena(ms),
        vector: vector_ajeno(ms),
        datos: json!({
            "host": host,
            "objetivo_tipo": tipo,
            "objetivo": objetivo,
            "comando": comando,
        }),
    }
}

fn lapida_comprobacion(uuid: &str, nombre: &str, ms: i64) -> LapidaDocumento {
    LapidaDocumento {
        tipo: "comprobacion_servicio".to_string(),
        uuid: uuid.to_string(),
        nombre: nombre.to_string(),
        version: version_ajena(ms),
        vector: vector_ajeno(ms),
        borrado_en: "2026-11-08T10:00:00+01:00".to_string(),
    }
}

/// Fusiona los documentos con el estado local y aplica el plan, como una
/// pasada de sincronización (sin cifrado ni carpeta: eso lo prueban las
/// pruebas del escritorio y del lector).
fn fusionar_y_aplicar(
    almacen: &Almacen,
    documentos: &[Documento],
) -> (
    magi_nucleo::sincronizacion::fusion::PlanFusion,
    ResumenAplicacion,
) {
    let local =
        magi_nucleo::sincronizacion::elementos::estado_local(almacen.conexion(), Path::new(HOGAR))
            .expect("estado local");
    let plan = fusionar(&local, documentos).expect("fusión");
    let resumen = aplicar(
        almacen.conexion(),
        &plan,
        &OpcionesAplicacion::default(),
        Path::new(HOGAR),
    )
    .expect("aplicando el plan");
    (plan, resumen)
}

// ------------------------------------------------------------- formato 3

#[test]
fn el_formato_que_se_escribe_sube_a_3_solo_con_comprobaciones() {
    let almacen = almacen();
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato vacío"),
        formato::FORMATO_1
    );
    let host = crear_host(&almacen, "web");
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con solo SSH"),
        formato::FORMATO_1
    );
    crear_host_escritorio(&almacen, "win-01");
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con escritorio"),
        formato::FORMATO_2
    );
    almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación propia");
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con comprobaciones"),
        formato::FORMATO_3
    );
    let id = almacen
        .comprobaciones_de_host(host)
        .expect("comprobaciones")[0]
        .id;
    almacen
        .borrar_comprobacion(id)
        .expect("borrando la comprobación");
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato tras el borrado"),
        formato::FORMATO_2
    );
}

#[test]
fn la_exportacion_del_formato_3_lleva_la_comprobacion() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path();
    let rutas = Rutas::nuevas(
        raiz.join("datos"),
        raiz.join("config"),
        raiz.join("estado"),
        raiz.join("hogar"),
        raiz.join("runtime"),
    );
    std::fs::create_dir_all(rutas.dir_ssh()).expect("creando ~/.ssh");
    std::fs::write(rutas.fichero_known_hosts(), b"").expect("known_hosts vacío");

    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let (host_uuid, _, _) = fila_versionada(&almacen, Tabla::Hosts, host);
    almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación propia");

    let documento =
        documento_propio(almacen.conexion(), &rutas, "1.3.0").expect("documento propio");
    assert_eq!(documento.formato, formato::FORMATO_3);
    let elemento = documento
        .elementos
        .iter()
        .find(|elemento| elemento.tipo == "comprobacion_servicio")
        .expect("el elemento comprobacion_servicio viaja");
    assert_eq!(elemento.datos["host"], host_uuid);
    assert_eq!(elemento.datos["objetivo_tipo"], "unidad");
    assert_eq!(elemento.datos["objetivo"], "nginx.service");
    assert_eq!(elemento.datos["comando"], "nginx -t");
    assert!(!elemento.version.is_empty(), "el elemento lleva versión");
    assert!(!elemento.vector.is_empty(), "el elemento lleva vector");
}

#[test]
fn un_fichero_del_formato_3_crea_la_comprobacion() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let (host_uuid, _, _) = fila_versionada(&almacen, Tabla::Hosts, host);
    let documento = documento_con(
        3,
        vec![elemento_comprobacion(
            UUID_COMPROBACION,
            &host_uuid,
            "unidad",
            "app.service",
            "/srv/app/bin/comprobar",
            500,
        )],
        Vec::new(),
    );
    // El documento pasa por el mismo camino que en un fichero real:
    // serializar, parsear (formato 3) y fusionar.
    let json = formato::serializar(&documento).expect("serializando el documento");
    let documento = formato::parsear(&json).expect("el formato 3 se lee");
    assert_eq!(documento.formato, formato::FORMATO_3);
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.creados, 1);
    assert_eq!(resumen.conflictos, 0);
    assert!(plan.conflictos.is_empty(), "{:?}", plan.conflictos);

    let comprobacion = almacen
        .obtener_comprobacion(host, TipoObjetivo::Unidad, "app.service")
        .expect("leyendo la comprobación")
        .expect("la comprobación se ha creado");
    assert_eq!(comprobacion.comando, "/srv/app/bin/comprobar");
    assert_eq!(comprobacion.objetivo, Objetivo::unidad("app.service"));
    assert_eq!(comprobacion.uuid.as_deref(), Some(UUID_COMPROBACION));
    let version = version_ajena(500);
    let vector = serde_json::to_string(&vector_ajeno(500)).expect("vector a JSON");
    assert_eq!(comprobacion.version.as_deref(), Some(version.as_str()));
    assert_eq!(comprobacion.vector.as_deref(), Some(vector.as_str()));
}

#[test]
fn una_edicion_del_formato_3_actualiza_la_comprobacion() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let (host_uuid, _, _) = fila_versionada(&almacen, Tabla::Hosts, host);
    let id = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación local");
    let (uuid, version_local, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    let ms = ms_de(&version_local) + 1_000;
    let documento = documento_con(
        3,
        vec![elemento_comprobacion(
            &uuid,
            &host_uuid,
            "unidad",
            "nginx.service",
            "nginx -t -q",
            ms,
        )],
        Vec::new(),
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.actualizados, 1);
    assert_eq!(resumen.creados, 0);

    let comprobacion = almacen
        .obtener_comprobacion(host, TipoObjetivo::Unidad, "nginx.service")
        .expect("leyendo")
        .expect("sigue la comprobación");
    assert_eq!(comprobacion.comando, "nginx -t -q");
    let version = version_ajena(ms);
    assert_eq!(comprobacion.version.as_deref(), Some(version.as_str()));
}

#[test]
fn una_lapida_del_formato_3_borra_la_comprobacion_y_deja_su_lapida() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let id = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación local");
    let (uuid, version_local, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    let ms = ms_de(&version_local) + 1_000;
    let documento = documento_con(
        3,
        Vec::new(),
        vec![lapida_comprobacion(&uuid, "nginx.service", ms)],
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.borrados, 1);
    assert!(almacen
        .comprobaciones_de_host(host)
        .expect("comprobaciones")
        .is_empty());

    let lapida = lapidas::obtener(almacen.conexion(), &uuid)
        .expect("leyendo la lápida")
        .expect("el borrado deja lápida");
    assert_eq!(lapida.tipo, "comprobacion_servicio");
    assert_eq!(lapida.nombre, "nginx.service");
}

#[test]
fn una_lapida_antigua_no_borra_una_edicion_posterior() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let id = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación local");
    let (uuid, version_local, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    // Una lápida de hace más milisegundos que la edición local pierde.
    let ms = ms_de(&version_local) - 1_000;
    let documento = documento_con(
        3,
        Vec::new(),
        vec![lapida_comprobacion(&uuid, "nginx.service", ms)],
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.borrados, 0);
    assert_eq!(
        almacen
            .comprobaciones_de_host(host)
            .expect("comprobaciones")
            .len(),
        1
    );
}

#[test]
fn importar_un_fichero_del_formato_2_no_toca_las_comprobaciones_locales() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let id = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación local");
    let (uuid, version, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);

    // Un equipo con MAGI 1.2 añade un host en un fichero del formato 2: no
    // conoce las comprobaciones y no las trae. Importarlo no puede tocarlas.
    let documento = documento_con(
        2,
        vec![ElementoDocumento {
            tipo: "host".to_string(),
            uuid: "dddddddd-dddd-4ddd-8ddd-dddddddddddd".to_string(),
            version: version_ajena(500),
            vector: vector_ajeno(500),
            datos: datos_host_documento("db-01", "192.0.2.77"),
        }],
        Vec::new(),
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(
        resumen.creados, 1,
        "el host del fichero formato 2 se importa"
    );
    let db = almacen
        .listar_hosts()
        .expect("hosts")
        .into_iter()
        .find(|host| host.nombre == "db-01")
        .expect("el host del fichero formato 2 se importa");
    assert_eq!(db.direccion, "192.0.2.77");

    let comprobaciones = almacen
        .comprobaciones_de_host(host)
        .expect("comprobaciones locales");
    assert_eq!(comprobaciones.len(), 1, "la comprobación local sigue");
    assert_eq!(comprobaciones[0].uuid.as_deref(), Some(uuid.as_str()));
    assert_eq!(
        comprobaciones[0].version.as_deref(),
        Some(version.as_str()),
        "ni siquiera cambia su versión"
    );
    // Y el fichero de este equipo se seguiría escribiendo en formato 3.
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato del inventario"),
        formato::FORMATO_3
    );
}

// --------------------------------------------------------- huérfanas y formatos

#[test]
fn una_comprobacion_de_un_host_que_ya_no_existe_es_huerfana() {
    let almacen = almacen();
    let _host = crear_host(&almacen, "web");
    let documento = documento_con(
        3,
        vec![elemento_comprobacion(
            UUID_COMPROBACION,
            "99999999-9999-4999-8999-999999999999",
            "unidad",
            "nginx.service",
            "nginx -t",
            500,
        )],
        Vec::new(),
    );
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert!(plan
        .conflictos
        .iter()
        .any(|conflicto| conflicto.tipo == "huerfano"
            && conflicto.elemento_tipo == "comprobacion_servicio"));
    assert_eq!(resumen.creados, 0);
    assert!(almacen
        .comprobaciones_servicio()
        .expect("comprobaciones")
        .is_empty());
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    assert!(pendientes
        .iter()
        .any(|conflicto| conflicto.tipo == "huerfano"
            && conflicto.elemento_tipo == "comprobacion_servicio"));
}

#[test]
fn una_comprobacion_de_un_host_que_no_es_ssh_es_huerfana() {
    let almacen = almacen();
    let escritorio = crear_host_escritorio(&almacen, "win-01");
    let (uuid_escritorio, _, _) = fila_versionada(&almacen, Tabla::Hosts, escritorio);
    let documento = documento_con(
        3,
        vec![elemento_comprobacion(
            UUID_COMPROBACION,
            &uuid_escritorio,
            "unidad",
            "nginx.service",
            "nginx -t",
            500,
        )],
        Vec::new(),
    );
    let (plan, _) = fusionar_y_aplicar(&almacen, &[documento]);
    assert!(plan
        .conflictos
        .iter()
        .any(|conflicto| conflicto.tipo == "huerfano"
            && conflicto.elemento_tipo == "comprobacion_servicio"));
    assert!(almacen
        .comprobaciones_servicio()
        .expect("comprobaciones")
        .is_empty());
}

#[test]
fn el_lector_acepta_los_formatos_1_2_y_3_y_rechaza_el_4_como_formato_nuevo() {
    for numero in [1u32, 2, 3] {
        let documento = documento_con(numero, Vec::new(), Vec::new());
        let json = formato::serializar(&documento).expect("serializando");
        let leido = formato::parsear(&json).expect("se acepta el formato");
        assert_eq!(leido.formato, numero);
    }
    let mut documento = documento_con(3, Vec::new(), Vec::new());
    documento.formato = formato::FORMATO_MAXIMO + 1;
    let json = formato::serializar(&documento).expect("serializando");
    let error = formato::parsear(&json)
        .expect_err("un formato más nuevo no se lee")
        .to_string();
    assert!(error.contains("formato 4"), "{error}");
    assert!(error.contains("actualiza MAGI"), "{error}");
}

#[test]
fn un_fichero_sin_comprobaciones_no_toca_las_locales() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación local");
    for numero in [1u32, 2] {
        let (_, resumen) =
            fusionar_y_aplicar(&almacen, &[documento_con(numero, Vec::new(), Vec::new())]);
        assert_eq!(resumen.borrados, 0);
        assert_eq!(
            almacen
                .comprobaciones_de_host(host)
                .expect("comprobaciones")
                .len(),
            1,
            "un fichero del formato {numero} no puede tocar las comprobaciones locales"
        );
    }
}

#[test]
fn dos_comprobaciones_para_el_mismo_objetivo_dejan_conflicto_sin_romper_la_importacion() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let (host_uuid, _, _) = fila_versionada(&almacen, Tabla::Hosts, host);
    let id = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación local");
    let (uuid_local, _, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    let documento = documento_con(
        3,
        vec![elemento_comprobacion(
            UUID_COMPROBACION,
            &host_uuid,
            "unidad",
            "nginx.service",
            "/srv/comprobar",
            500,
        )],
        Vec::new(),
    );
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    // La fusión deja los dos vivos (cada uno con su uuid); al aplicar, la
    // copia que choca con la clave natural se descarta como conflicto de
    // nombre y la importación termina sin romper la transacción.
    assert_eq!(
        plan.vivos
            .iter()
            .filter(|vivo| vivo.tipo == "comprobacion_servicio")
            .count(),
        2
    );
    assert_eq!(resumen.creados, 0);
    assert_eq!(resumen.conflictos, 1);
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    assert!(pendientes.iter().any(|conflicto| conflicto.tipo == "nombre"
        && conflicto.elemento_tipo == "comprobacion_servicio"
        && conflicto.uuid == UUID_COMPROBACION));
    let comprobaciones = almacen
        .comprobaciones_de_host(host)
        .expect("comprobaciones");
    assert_eq!(comprobaciones.len(), 1);
    assert_eq!(comprobaciones[0].uuid.as_deref(), Some(uuid_local.as_str()));
    assert_eq!(comprobaciones[0].comando, "nginx -t");
}

// ------------------------------------------------------------------ CRUD

#[test]
fn el_crud_de_comprobaciones_valida_y_sube_la_version() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");

    // Objetivos y órdenes que no valen.
    assert!(almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx", "nginx -t")
        .is_err());
    assert!(almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "")
        .is_err());
    assert!(almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "una\nlinea")
        .is_err());
    assert!(almacen
        .guardar_comprobacion(
            host,
            TipoObjetivo::Proyecto,
            "Web",
            "docker compose config -q"
        )
        .is_err());
    // Solo los hosts SSH tienen comprobaciones.
    let escritorio = crear_host_escritorio(&almacen, "win-01");
    assert!(almacen
        .guardar_comprobacion(
            escritorio,
            TipoObjetivo::Unidad,
            "nginx.service",
            "nginx -t"
        )
        .is_err());

    // Alta: deja uuid, versión y vector de este equipo.
    let id = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("alta de la comprobación");
    let (uuid, version, vector) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    assert!(!uuid.trim().is_empty());
    assert!(!version.trim().is_empty());
    assert!(!vector.trim().is_empty());

    // Guardar la misma orden no sube la versión: solo cambia lo que viaja.
    almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("guardar la misma orden");
    let (_, version_igual, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    assert_eq!(version_igual, version, "sin cambio no hay versión nueva");

    // Upsert: la misma clave natural no duplica la fila.
    let id_otra_vez = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t -q")
        .expect("edición por upsert");
    assert_eq!(id_otra_vez, id);
    assert_eq!(
        almacen
            .comprobaciones_de_host(host)
            .expect("comprobaciones")
            .len(),
        1
    );
    let (_, version_editada, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    assert!(
        ms_de(&version_editada) > ms_de(&version),
        "la edición sube la versión: {version} → {version_editada}"
    );

    let comprobacion = almacen
        .obtener_comprobacion(host, TipoObjetivo::Unidad, "nginx.service")
        .expect("leyendo")
        .expect("existe");
    assert_eq!(comprobacion.objetivo, Objetivo::unidad("nginx.service"));
    assert_eq!(comprobacion.comando, "nginx -t -q");
    assert_eq!(comprobacion.host_id, host);

    // Borrado con lápida.
    almacen.borrar_comprobacion(id).expect("borrando");
    assert!(almacen
        .comprobaciones_de_host(host)
        .expect("comprobaciones")
        .is_empty());
    let lapida = lapidas::obtener(almacen.conexion(), &uuid)
        .expect("leyendo la lápida")
        .expect("el borrado deja lápida");
    assert_eq!(lapida.tipo, "comprobacion_servicio");
    assert_eq!(lapida.nombre, "nginx.service");
}

#[test]
fn borrar_el_host_deja_lapida_de_sus_comprobaciones() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let id = almacen
        .guardar_comprobacion(
            host,
            TipoObjetivo::Proyecto,
            "web",
            "docker compose config -q",
        )
        .expect("comprobación de proyecto");
    let (uuid, _, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    almacen.borrar_host(host).expect("borrando el host");
    let lapida = lapidas::obtener(almacen.conexion(), &uuid)
        .expect("leyendo la lápida")
        .expect("el host borra sus comprobaciones con lápida");
    assert_eq!(lapida.tipo, "comprobacion_servicio");
    assert_eq!(lapida.nombre, "web");
}

// --------------------------------------------------------------- migración 9

#[test]
fn la_migracion_9_estrena_comprobaciones_y_fallos_json() {
    // Base nueva: la versión actual de la base (10), tabla y columna.
    let almacen = almacen();
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .expect("user_version");
    assert_eq!(version, magi_nucleo::almacen::migraciones::VERSION_BD);
    let tablas: Vec<String> = almacen
        .conexion()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .expect("sqlite_master")
        .query_map([], |fila| fila.get(0))
        .expect("tablas")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("nombres");
    assert!(tablas.contains(&"COMPROBACIONES_SERVICIO".to_string()));
    let columnas: Vec<String> = almacen
        .conexion()
        .prepare("PRAGMA table_info(SONDEOS)")
        .expect("table_info")
        .query_map([], |fila| fila.get(1))
        .expect("columnas")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("nombres");
    assert!(
        columnas.contains(&"fallos_json".to_string()),
        "SONDEOS gana fallos_json: {columnas:?}"
    );

    // Base de la fase 9 (user_version 6) que sube hasta la actual sin perder nada.
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let ruta = temporal.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta).expect("base temporal");
        for migracion in &magi_nucleo::almacen::migraciones::MIGRACIONES[..6] {
            conexion
                .execute_batch(migracion)
                .expect("migración antigua");
        }
        conexion
            .pragma_update(None, "user_version", 6)
            .expect("user_version antigua");
        conexion
            .execute(
                "INSERT INTO HOSTS (nombre, direccion, puerto, opciones_extra, origen,
                                    creado_en, actualizado_en)
                 VALUES ('viejo', '192.0.2.9', 22, '', 'manual',
                         '2026-01-01T00:00:00+01:00', '2026-01-01T00:00:00+01:00')",
                [],
            )
            .expect("host antiguo");
    }
    let almacen = Almacen::abrir(&ruta, &Equipo::nuevo("pruebas", "linux"), "1.3.0")
        .expect("migrando la base antigua");
    let version: i64 = almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .expect("user_version migrada");
    assert_eq!(version, magi_nucleo::almacen::migraciones::VERSION_BD);
    assert_eq!(almacen.listar_hosts().expect("hosts").len(), 1);
    // Las filas antiguas de SONDEOS se leen sin fallos (la columna queda nula).
    let host = almacen.listar_hosts().expect("hosts")[0].id;
    let mut sondeo = Sondeo::vacio(host);
    sondeo.resultado = ResultadoSondeo::Ok;
    sondeo.duracion_ms = 3;
    almacen.guardar_sondeo(&sondeo).expect("sondeo nuevo");
    let leido = almacen
        .ultimo_sondeo(host)
        .expect("leyendo el sondeo")
        .expect("hay sondeo");
    assert!(leido.fallos.is_none(), "un sondeo sin fallos guarda NULL");
}

// ------------------------------------------------------- fallos del sondeo

#[test]
fn el_sondeo_guarda_y_lee_los_fallos() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let fallos = FallosSondeo {
        unidades: FallosUnidades {
            total: 2,
            nombres: vec!["copia.service".to_string()],
        },
        contenedores: Some(FallosContenedores {
            motor: Some(MotorNombre::Docker),
            total: 3,
            nombres: vec!["web-worker-1".to_string(), "panel".to_string()],
        }),
    };
    let mut sondeo = Sondeo::vacio(host);
    sondeo.resultado = ResultadoSondeo::Ok;
    sondeo.nucleos = Some(8);
    sondeo.carga_1m = Some(0.4);
    sondeo.uptime_seg = Some(1_000);
    sondeo.fallos = Some(fallos.clone());
    let id = almacen
        .guardar_sondeo(&sondeo)
        .expect("guardando el sondeo");

    let leido = almacen
        .ultimo_sondeo(host)
        .expect("leyendo el sondeo")
        .expect("hay sondeo");
    assert_eq!(leido.id, id);
    assert_eq!(leido.fallos.as_ref(), Some(&fallos));
    assert_eq!(leido.fallos.as_ref().map(FallosSondeo::total), Some(5));

    // Un host que no respondió no guarda fallos: NULL no es «sin fallos».
    let mut caido = Sondeo::vacio(host);
    caido.resultado = ResultadoSondeo::Error;
    caido.error = Some("timeout".to_string());
    caido.fallos = Some(fallos);
    almacen.guardar_sondeo(&caido).expect("guardando el caído");
    let leido = almacen
        .ultimo_sondeo(host)
        .expect("leyendo el sondeo")
        .expect("hay sondeo");
    assert!(leido.fallos.is_none());

    // Sin motor de contenedores, los contenedores van a null pero las unidades
    // siguen contadas.
    let mut sin_motor = Sondeo::vacio(host);
    sin_motor.resultado = ResultadoSondeo::Ok;
    sin_motor.fallos = Some(FallosSondeo {
        unidades: FallosUnidades {
            total: 1,
            nombres: vec!["exportar.service".to_string()],
        },
        contenedores: None,
    });
    almacen
        .guardar_sondeo(&sin_motor)
        .expect("sondeo sin motor");
    let leido = almacen
        .ultimo_sondeo(host)
        .expect("leyendo el sondeo")
        .expect("hay sondeo");
    let fallos = leido.fallos.expect("fallos del sondeo");
    assert_eq!(fallos.unidades.total, 1);
    assert!(fallos.contenedores.is_none());
}

// -------------------------------------------------- unión de un equipo nuevo

#[test]
fn unir_por_nombre_adopta_el_uuid_del_fichero_para_la_misma_comprobacion() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let (host_uuid, _, _) = fila_versionada(&almacen, Tabla::Hosts, host);
    let id = almacen
        .guardar_comprobacion(host, TipoObjetivo::Unidad, "nginx.service", "nginx -t")
        .expect("comprobación local");
    let (uuid_local, _, _) = fila_versionada(&almacen, Tabla::ComprobacionesServicio, id);
    assert_ne!(uuid_local, UUID_COMPROBACION);

    let documento = documento_con(
        3,
        vec![elemento_comprobacion(
            UUID_COMPROBACION,
            &host_uuid,
            "unidad",
            "nginx.service",
            "nginx -t",
            500,
        )],
        Vec::new(),
    );
    let unidos = magi_nucleo::sincronizacion::aplicacion::unir_por_nombre(
        almacen.conexion(),
        &[documento],
        Path::new(HOGAR),
    )
    .expect("union por nombre");
    assert_eq!(
        unidos, 1,
        "la comprobación local adopta el uuid del fichero"
    );
    let comprobacion = almacen
        .obtener_comprobacion(host, TipoObjetivo::Unidad, "nginx.service")
        .expect("leyendo")
        .expect("sigue la comprobación");
    assert_eq!(comprobacion.uuid.as_deref(), Some(UUID_COMPROBACION));
}
