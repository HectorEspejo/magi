//! Pruebas de la migración 7 y de los asistentes de sincronización (Fase 10):
//! crear, unir con clave equivocada, rotar, quitar y desactivar.

use std::path::PathBuf;

use magi::almacen::Almacen;
use magi::config::{Config, Rutas};
use magi::llavero::{
    LlaveroEnMemoria, LlaveroSecreto, CUENTA_SINCRONIZACION, CUENTA_SINCRONIZACION_ANTERIOR,
};
use magi::modelo::{DatosHost, Origen};
use magi::sincronizacion::carpeta;
use magi::sincronizacion::cifrado::ClaveSincronizacion;
use magi::sincronizacion::equipos;
use magi::sincronizacion::formato;

struct Entorno {
    _temporal: tempfile::TempDir,
    pub rutas: Rutas,
    pub almacen: Almacen,
    pub config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::tempdir().unwrap();
    let raiz = temporal.path();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    };
    std::fs::create_dir_all(&rutas.hogar).unwrap();
    let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
    Entorno {
        _temporal: temporal,
        rutas,
        almacen,
        config: Config::default(),
    }
}

fn carpeta_de_prueba(entorno: &Entorno, nombre: &str) -> PathBuf {
    let carpeta = entorno.rutas.hogar.join(nombre);
    std::fs::create_dir_all(&carpeta).unwrap();
    carpeta
}

/// Escribe a mano un fichero de sincronización válido de otro equipo, cifrado
/// con la clave indicada.
fn escribir_fichero_ajeno(carpeta: &std::path::Path, clave: &ClaveSincronizacion) -> String {
    let uuid_equipo = uuid::Uuid::new_v4().to_string();
    let documento = formato::Documento {
        formato: formato::FORMATO,
        app: "1.1.0".to_string(),
        equipo: formato::EquipoDocumento {
            uuid: uuid_equipo.clone(),
            nombre: "otro-equipo".to_string(),
            plataforma: "linux".to_string(),
        },
        generado_en: "2026-11-05T09:12:44.120+01:00".to_string(),
        reloj_ms: 1,
        elementos: Vec::new(),
        lapidas: Vec::new(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    };
    let json = formato::serializar(&documento).unwrap();
    let firma = clave.firmar(json.as_bytes()).unwrap();
    let plano = formato::componer(&json, &firma);
    let cifrado = clave.cifrar(&plano).unwrap();
    carpeta::escribir(carpeta, &uuid_equipo, &cifrado).unwrap();
    uuid_equipo
}

#[test]
fn la_migracion_7_rellena_uuid_version_y_vector_y_crea_el_equipo() {
    let temporal = tempfile::tempdir().unwrap();
    let ruta = temporal.path().join("magi.db");
    {
        let conexion = rusqlite::Connection::open(&ruta).unwrap();
        for migracion in magi::almacen::migraciones::MIGRACIONES {
            conexion.execute_batch(migracion).unwrap();
        }
        conexion.pragma_update(None, "user_version", 6).unwrap();
        conexion
            .execute(
                "INSERT INTO GRUPOS (nombre, orden, plegado, creado_en)
                 VALUES ('produccion', 0, 0, '2026-01-01T00:00:00+01:00')",
                [],
            )
            .unwrap();
        conexion
            .execute(
                "INSERT INTO HOSTS (nombre, direccion, puerto, opciones_extra, origen,
                                    creado_en, actualizado_en)
                 VALUES ('web', '192.0.2.5', 22, '', 'manual',
                         '2026-01-01T00:00:00+01:00', '2026-01-01T00:00:00+01:00')",
                [],
            )
            .unwrap();
        conexion
            .execute(
                "INSERT INTO IDENTIDADES (alias, tipo, huella, origen, anadida_en)
                 VALUES ('mi-clave', 'ed25519', 'SHA256:abcdef', 'fichero',
                         '2026-01-01T00:00:00+01:00')",
                [],
            )
            .unwrap();
    }
    let almacen = Almacen::abrir(&ruta).unwrap();
    let conexion = almacen.conexion();
    let version: i64 = conexion
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(version, 7);

    let (uuid_equipo, reloj_ms): (String, i64) = conexion
        .query_row(
            "SELECT uuid, reloj_ms FROM EQUIPOS WHERE es_este = 1",
            [],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .unwrap();
    assert!(!uuid_equipo.is_empty());
    assert!(reloj_ms > 0);
    let equipos_totales: i64 = conexion
        .query_row("SELECT COUNT(*) FROM EQUIPOS", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(equipos_totales, 1, "solo la fila de este equipo");

    for tabla in [
        "GRUPOS",
        "HOSTS",
        "IDENTIDADES",
        "TUNELES",
        "SNIPPETS",
        "SINCRONIZACIONES_DIR",
    ] {
        let filas: i64 = conexion
            .query_row(&format!("SELECT COUNT(*) FROM {tabla}"), [], |fila| {
                fila.get(0)
            })
            .unwrap();
        if filas == 0 {
            continue;
        }
        let (uuid, version, vector): (String, String, String) = conexion
            .query_row(
                &format!("SELECT uuid, version, vector FROM {tabla} LIMIT 1"),
                [],
                |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
            )
            .unwrap();
        assert!(!uuid.is_empty(), "{tabla} sin uuid");
        let ms = magi::sincronizacion::reloj::ms_de_version(&version)
            .unwrap_or_else(|| panic!("{tabla} con versión rara: {version}"));
        assert_eq!(ms, reloj_ms);
        let mapa: std::collections::BTreeMap<String, i64> = serde_json::from_str(&vector).unwrap();
        assert_eq!(mapa.get(&uuid_equipo), Some(&ms), "{tabla} con vector raro");
    }

    let huella: String = conexion
        .query_row("SELECT huella FROM IDENTIDADES LIMIT 1", [], |fila| {
            fila.get(0)
        })
        .unwrap();
    let uuid_identidad: String = conexion
        .query_row("SELECT uuid FROM IDENTIDADES LIMIT 1", [], |fila| {
            fila.get(0)
        })
        .unwrap();
    let esperado =
        uuid::Uuid::new_v5(&formato::NAMESPACE_IDENTIDADES, huella.as_bytes()).to_string();
    assert_eq!(uuid_identidad, esperado, "uuid v5 de la identidad");

    // Abrir otra vez no vuelve a migrar ni duplica la fila del equipo.
    drop(almacen);
    let almacen = Almacen::abrir(&ruta).unwrap();
    let equipos_totales: i64 = almacen
        .conexion()
        .query_row("SELECT COUNT(*) FROM EQUIPOS", [], |fila| fila.get(0))
        .unwrap();
    assert_eq!(equipos_totales, 1);
}

#[test]
fn crear_valida_la_carpeta_guarda_la_clave_y_la_configuracion() {
    let mut entorno = entorno();
    let carpeta = carpeta_de_prueba(&entorno, "magi-sync");
    let llavero = LlaveroEnMemoria::nuevo();

    let clave = equipos::crear(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &carpeta,
        &mut entorno.config,
        &llavero,
    )
    .unwrap();

    let guardada = llavero.recuperar(CUENTA_SINCRONIZACION).unwrap().unwrap();
    assert_eq!(guardada.as_str(), clave.texto().as_str());
    assert_eq!(
        entorno.config.sincronizacion.carpeta.as_deref(),
        Some(carpeta.to_string_lossy().as_ref())
    );
    let (leida, _) = Config::cargar(&entorno.rutas.fichero_config());
    assert_eq!(
        leida.sincronizacion.carpeta,
        entorno.config.sincronizacion.carpeta
    );
    assert_eq!(leida.sincronizacion.retraso_seg, 5);
    assert_eq!(leida.sincronizacion.copias, 10);
    let activaciones: i64 = entorno
        .almacen
        .conexion()
        .query_row(
            "SELECT COUNT(*) FROM REGISTRO WHERE tipo = 'inventario_sinc_activada'",
            [],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(activaciones, 1);

    // Una carpeta que no existe o que no es carpeta se rechaza.
    let inexistente = entorno.rutas.hogar.join("no-existe");
    assert!(equipos::crear(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &inexistente,
        &mut entorno.config,
        &llavero
    )
    .is_err());
}

#[test]
fn crear_rechaza_una_carpeta_que_ya_tiene_ficheros_de_magi() {
    let mut entorno = entorno();
    let carpeta = carpeta_de_prueba(&entorno, "magi-sync");
    let clave = ClaveSincronizacion::generar();
    escribir_fichero_ajeno(&carpeta, &clave);
    let llavero = LlaveroEnMemoria::nuevo();
    let error = equipos::crear(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &carpeta,
        &mut entorno.config,
        &llavero,
    )
    .unwrap_err();
    assert!(error.to_string().contains("une este equipo"), "{error}");
}

#[test]
fn unir_con_una_clave_equivocada_no_guarda_nada() {
    let mut entorno = entorno();
    let carpeta = carpeta_de_prueba(&entorno, "magi-sync");
    let clave_buena = ClaveSincronizacion::generar();
    escribir_fichero_ajeno(&carpeta, &clave_buena);
    let llavero = LlaveroEnMemoria::nuevo();

    let equivocada = ClaveSincronizacion::generar().texto().to_string();
    let error = equipos::unir(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &carpeta,
        &equivocada,
        &mut entorno.config,
        &llavero,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("la clave no abre los ficheros de esta carpeta"),
        "{error}"
    );
    assert!(llavero.recuperar(CUENTA_SINCRONIZACION).unwrap().is_none());
    assert!(entorno.config.sincronizacion.carpeta.is_none());
}

#[test]
fn rotar_guarda_la_anterior_y_las_dos_claves_sirven_para_leer() {
    let mut entorno = entorno();
    let carpeta = carpeta_de_prueba(&entorno, "magi-sync");
    let llavero = LlaveroEnMemoria::nuevo();
    let original = equipos::crear(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &carpeta,
        &mut entorno.config,
        &llavero,
    )
    .unwrap();

    let nueva = equipos::rotar(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &mut entorno.config,
        &llavero,
    )
    .unwrap();
    assert_ne!(nueva.clave_publica(), original.clave_publica());
    let actual = llavero.recuperar(CUENTA_SINCRONIZACION).unwrap().unwrap();
    assert_eq!(actual.as_str(), nueva.texto().as_str());
    let anterior = llavero
        .recuperar(CUENTA_SINCRONIZACION_ANTERIOR)
        .unwrap()
        .unwrap();
    assert_eq!(anterior.as_str(), original.texto().as_str());
    assert!(equipos::clave_anterior_vigente(&entorno.config));
    let claves = equipos::claves_para_leer(&entorno.config, &llavero).unwrap();
    assert_eq!(claves.len(), 2);
    let cambiadas: i64 = entorno
        .almacen
        .conexion()
        .query_row(
            "SELECT COUNT(*) FROM REGISTRO WHERE tipo = 'inventario_clave_cambiada'",
            [],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(cambiadas, 1);
}

#[test]
fn quitar_marca_el_equipo_borra_su_fichero_y_rota_la_clave() {
    let mut entorno = entorno();
    let carpeta = carpeta_de_prueba(&entorno, "magi-sync");
    let llavero = LlaveroEnMemoria::nuevo();
    let original = equipos::crear(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &carpeta,
        &mut entorno.config,
        &llavero,
    )
    .unwrap();
    let otro_uuid = uuid::Uuid::new_v4().to_string();
    magi::almacen::equipos::crear(entorno.almacen.conexion(), &otro_uuid, "mac-mini", "macos")
        .unwrap();
    std::fs::write(
        carpeta.join(carpeta::nombre_fichero(&otro_uuid)),
        b"fichero de otro equipo",
    )
    .unwrap();

    let nueva = equipos::quitar(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &mut entorno.config,
        &otro_uuid,
        &llavero,
    )
    .unwrap();
    assert_ne!(nueva.clave_publica(), original.clave_publica());
    assert!(!carpeta.join(carpeta::nombre_fichero(&otro_uuid)).exists());
    let equipo = magi::almacen::equipos::por_uuid(entorno.almacen.conexion(), &otro_uuid)
        .unwrap()
        .unwrap();
    assert_eq!(equipo.estado, magi::almacen::equipos::ESTADO_QUITADO);
    assert!(equipo.quitado_en.is_some());
    let registros: i64 = entorno
        .almacen
        .conexion()
        .query_row(
            "SELECT COUNT(*) FROM REGISTRO WHERE tipo IN
                ('inventario_equipo_quitado', 'inventario_clave_cambiada')",
            [],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(registros, 2);
}

#[test]
fn desactivar_borra_su_fichero_y_la_clave_pero_deja_magi_db_intacta() {
    let mut entorno = entorno();
    let carpeta = carpeta_de_prueba(&entorno, "magi-sync");
    let llavero = LlaveroEnMemoria::nuevo();
    equipos::crear(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &carpeta,
        &mut entorno.config,
        &llavero,
    )
    .unwrap();
    let host = entorno
        .almacen
        .crear_host(
            &DatosHost {
                nombre: "web".to_string(),
                direccion: "192.0.2.5".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
    let este_uuid = magi::almacen::equipos::este(entorno.almacen.conexion())
        .unwrap()
        .uuid;
    std::fs::write(
        carpeta.join(carpeta::nombre_fichero(&este_uuid)),
        b"mi fichero",
    )
    .unwrap();

    equipos::desactivar(
        entorno.almacen.conexion(),
        &entorno.rutas,
        &mut entorno.config,
        &llavero,
    )
    .unwrap();
    assert!(!carpeta.join(carpeta::nombre_fichero(&este_uuid)).exists());
    assert!(llavero.recuperar(CUENTA_SINCRONIZACION).unwrap().is_none());
    assert!(entorno.config.sincronizacion.carpeta.is_none());
    assert_eq!(entorno.almacen.listar_hosts().unwrap().len(), 1);
    assert_eq!(
        entorno.almacen.obtener_host(host).unwrap().nombre,
        "web",
        "magi.db queda intacta"
    );
    let desactivaciones: i64 = entorno
        .almacen
        .conexion()
        .query_row(
            "SELECT COUNT(*) FROM REGISTRO WHERE tipo = 'inventario_sinc_desactivada'",
            [],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(desactivaciones, 1);
}
