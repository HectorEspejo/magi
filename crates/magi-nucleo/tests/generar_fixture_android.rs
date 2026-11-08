//! Generador de la fixture de sincronización que usa la prueba de humo del
//! emulador Android (crates/magi-movil se integra en android/app/src/androidTest).
//!
//! No es una prueba normal: es una herramienta de mantenimiento. Se ejecuta a
//! mano cuando cambie el formato:
//!
//!   cargo test -p magi-nucleo --test generar_fixture_android -- --ignored --nocapture
//!
//! Escribe android/app/src/androidTest/assets/sincronizacion/ con un fichero
//! magi-<uuid>.age de un equipo de pruebas. La clave de sincronización es la
//! constante CLAVE de este fichero y la prueba de humo la usa tal cual.

use std::collections::BTreeMap;

use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::elementos::{DatosComprobacionServicio, DatosGrupo, DatosHost};
use magi_nucleo::sincronizacion::formato::{
    self, Documento, ElementoDocumento, EquipoDocumento, FORMATO_1, FORMATO_3,
};

const CLAVE: &str = "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS";
const UUID_EQUIPO: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const UUID_GRUPO: &str = "11111111-2222-4333-8444-555555555555";
const UUID_WEB: &str = "66666666-7777-4888-8999-aaaaaaaaaaaa";
const UUID_DB: &str = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";

/// Datos de un host tal como los exporta MAGI 1.1: sin los campos de
/// escritorio, que no existían en el formato 1. Así la fixture sirve para
/// comprobar que un MAGI 1.2 lee un fichero antiguo (todos los hosts, SSH).
fn datos_host_formato_1(datos: DatosHost) -> serde_json::Value {
    let mut valor = serde_json::to_value(datos).expect("datos del host a JSON");
    if let Some(objeto) = valor.as_object_mut() {
        objeto.remove("tipo");
        objeto.remove("escritorio_dominio");
        objeto.remove("escritorio_opciones");
    }
    valor
}

fn elemento(tipo: &str, uuid: &str, datos: serde_json::Value) -> ElementoDocumento {
    let mut vector = BTreeMap::new();
    vector.insert(UUID_EQUIPO.to_string(), 1_700_000_000_000i64);
    ElementoDocumento {
        tipo: tipo.to_string(),
        uuid: uuid.to_string(),
        version: format!("000000000000001-{UUID_EQUIPO}"),
        vector,
        datos,
    }
}

#[test]
#[ignore = "herramienta: regenera la fixture que usa la prueba de humo de Android"]
fn generar_fixture_android() {
    let clave = ClaveSincronizacion::desde_texto(CLAVE).expect("clave de prueba válida");
    let mut elementos = Vec::new();
    elementos.push(elemento(
        "grupo",
        UUID_GRUPO,
        serde_json::to_value(DatosGrupo {
            nombre: "produccion".to_string(),
            orden: 1,
        })
        .unwrap(),
    ));
    for (uuid, nombre, direccion) in [
        (UUID_WEB, "web-01", "192.0.2.10"),
        (UUID_DB, "db-01", "192.0.2.11"),
    ] {
        let datos = DatosHost {
            nombre: nombre.to_string(),
            direccion: direccion.to_string(),
            puerto: 22,
            tipo: TipoHost::Ssh,
            escritorio_dominio: None,
            escritorio_opciones: serde_json::json!({}),
            usuario: Some("usuario".to_string()),
            identidad_ref: None,
            salto: None,
            grupo: Some(UUID_GRUPO.to_string()),
            multiplexar: false,
            keepalive_seg: Some(30),
            opciones_extra: String::new(),
            servicios: String::new(),
            origen: "manual".to_string(),
            etiquetas: vec!["web".to_string()],
            verificaciones: None,
            snippet_al_conectar: None,
        };
        elementos.push(elemento("host", uuid, datos_host_formato_1(datos)));
    }
    let documento = Documento {
        formato: FORMATO_1,
        app: "1.1.0".to_string(),
        equipo: EquipoDocumento {
            uuid: UUID_EQUIPO.to_string(),
            nombre: "escritorio-pruebas".to_string(),
            plataforma: "linux".to_string(),
        },
        generado_en: magi_nucleo::modelo::fecha_ahora(),
        reloj_ms: 1_700_000_000_000,
        elementos,
        lapidas: Vec::new(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    };
    let json = formato::serializar(&documento).expect("JSON del documento");
    let firma = clave.firmar(json.as_bytes()).expect("firma");
    let plano = formato::componer(&json, &firma, FORMATO_1);
    let cifrado = clave.cifrar(&plano).expect("cifrado");
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let carpeta = raiz.join("../../android/app/src/androidTest/assets/sincronizacion");
    std::fs::create_dir_all(&carpeta).expect("carpeta de assets");
    let ruta = carpeta.join(format!("magi-{UUID_EQUIPO}.age"));
    std::fs::write(&ruta, &cifrado).expect("escribiendo el fichero de pruebas");
    println!(
        "fixture escrita en {} ({} bytes)",
        ruta.display(),
        cifrado.len()
    );
    println!("clave de la fixture: {CLAVE}");
}

// ---------------------------------------------------------------------------
// Fixture del formato 3 (Fase 13).
//
// La prueba JVM de la app Android (android/app/src/test/.../nucleo/
// PruebaFormatoSincronizacion.kt) la importa con el núcleo real compilado para
// el anfitrión y comprueba que lee el formato 3 y que el equipo queda al día.
// Escribe en android/app/src/test/resources/sincronizacion/ y NO toca la
// fixture del formato 1 que usa la prueba de humo del emulador.
//
//   cargo test -p magi-nucleo --test generar_fixture_android \
//       generar_fixture_android_formato3 -- --ignored --nocapture
// ---------------------------------------------------------------------------

const UUID_EQUIPO_F3: &str = "dddddddd-eeee-4fff-8aaa-bbbbbbbbbbbb";
const UUID_GRUPO_F3: &str = "22222222-3333-4555-8666-777777777777";
const UUID_WEB_F3: &str = "88888888-9999-4aaa-8bbb-cccccccccccc";
const UUID_COMPROBACION_F3: &str = "12121212-3434-4565-8787-909090909090";

/// Elemento del documento del formato 3, con el vector del equipo de pruebas.
fn elemento_formato3(tipo: &str, uuid: &str, datos: serde_json::Value) -> ElementoDocumento {
    let mut vector = BTreeMap::new();
    vector.insert(UUID_EQUIPO_F3.to_string(), 1_700_000_000_000i64);
    ElementoDocumento {
        tipo: tipo.to_string(),
        uuid: uuid.to_string(),
        version: format!("000000000000001-{UUID_EQUIPO_F3}"),
        vector,
        datos,
    }
}

#[test]
#[ignore = "herramienta: regenera la fixture del formato 3 que usa la prueba JVM de Android"]
fn generar_fixture_android_formato3() {
    let clave = ClaveSincronizacion::desde_texto(CLAVE).expect("clave de prueba válida");
    let mut elementos = vec![elemento_formato3(
        "grupo",
        UUID_GRUPO_F3,
        serde_json::to_value(DatosGrupo {
            nombre: "produccion".to_string(),
            orden: 1,
        })
        .unwrap(),
    )];
    elementos.push(elemento_formato3(
        "host",
        UUID_WEB_F3,
        serde_json::to_value(DatosHost {
            nombre: "web-01".to_string(),
            direccion: "192.0.2.10".to_string(),
            puerto: 22,
            tipo: TipoHost::Ssh,
            escritorio_dominio: None,
            escritorio_opciones: serde_json::json!({}),
            usuario: Some("usuario".to_string()),
            identidad_ref: None,
            salto: None,
            grupo: Some(UUID_GRUPO_F3.to_string()),
            multiplexar: false,
            keepalive_seg: Some(30),
            opciones_extra: String::new(),
            servicios: String::new(),
            origen: "manual".to_string(),
            etiquetas: vec!["web".to_string()],
            verificaciones: None,
            snippet_al_conectar: None,
        })
        .unwrap(),
    ));
    // La comprobación propia es lo que obliga a escribir el formato 3 (Fase 13,
    // D249); viaja con host, objetivo_tipo, objetivo y comando.
    elementos.push(elemento_formato3(
        "comprobacion_servicio",
        UUID_COMPROBACION_F3,
        serde_json::to_value(DatosComprobacionServicio {
            host: UUID_WEB_F3.to_string(),
            objetivo_tipo: "unidad".to_string(),
            objetivo: "nginx.service".to_string(),
            comando: "nginx -t".to_string(),
        })
        .unwrap(),
    ));
    let documento = Documento {
        formato: FORMATO_3,
        app: "1.3.0".to_string(),
        equipo: EquipoDocumento {
            uuid: UUID_EQUIPO_F3.to_string(),
            nombre: "escritorio-pruebas".to_string(),
            plataforma: "linux".to_string(),
        },
        generado_en: magi_nucleo::modelo::fecha_ahora(),
        reloj_ms: 1_700_000_000_000,
        elementos,
        lapidas: Vec::new(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    };
    let json = formato::serializar(&documento).expect("JSON del documento");
    let firma = clave.firmar(json.as_bytes()).expect("firma");
    let plano = formato::componer(&json, &firma, FORMATO_3);
    let cifrado = clave.cifrar(&plano).expect("cifrado");
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let carpeta = raiz.join("../../android/app/src/androidTest/assets/sincronizacion");
    std::fs::create_dir_all(&carpeta).expect("carpeta de assets de androidTest");
    let ruta = carpeta.join(format!("magi-{UUID_EQUIPO_F3}.age"));
    std::fs::write(&ruta, &cifrado).expect("escribiendo el fichero de pruebas");
    println!(
        "fixture del formato 3 escrita en {} ({} bytes)",
        ruta.display(),
        cifrado.len()
    );
    println!("clave de la fixture: {CLAVE}");
}
