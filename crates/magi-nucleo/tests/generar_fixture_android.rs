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

use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::elementos::{DatosGrupo, DatosHost};
use magi_nucleo::sincronizacion::formato::{self, Documento, ElementoDocumento, EquipoDocumento};

const CLAVE: &str = "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS";
const UUID_EQUIPO: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const UUID_GRUPO: &str = "11111111-2222-4333-8444-555555555555";
const UUID_WEB: &str = "66666666-7777-4888-8999-aaaaaaaaaaaa";
const UUID_DB: &str = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";

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
        elementos.push(elemento("host", uuid, serde_json::to_value(datos).unwrap()));
    }
    let documento = Documento {
        formato: formato::FORMATO,
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
    let plano = formato::componer(&json, &firma);
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
