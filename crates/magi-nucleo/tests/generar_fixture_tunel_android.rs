//! Generador de la fixture de sincronización para el E2E del túnel local
//! (instrumentada en el emulador Android). Herramienta de mantenimiento:
//!
//!   cargo test -p magi-nucleo --test generar_fixture_tunel_android -- --ignored --nocapture
//!
//! El host apunta a 10.0.2.2:12222 (el anfitrión visto desde el emulador) con el
//! usuario usuario; el túnel es 127.0.0.1:9090 → 127.0.0.1:19091. Los puertos
//! son fijos porque también lo son los del servidor de pruebas
//! (crates/magi-movil/examples/servidor_pruebas.rs).

use std::collections::BTreeMap;

use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::elementos::{DatosHost, DatosTunel};
use magi_nucleo::sincronizacion::formato::{
    self, Documento, ElementoDocumento, EquipoDocumento, FORMATO_1,
};

const CLAVE: &str = "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS";
const UUID_EQUIPO: &str = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";
const UUID_HOST: &str = "11111111-3333-4444-8555-666666666666";
const UUID_TUNEL: &str = "22222222-4444-4555-8666-777777777777";

/// Datos de un host tal como los exporta MAGI 1.1 (formato 1): sin los campos
/// de escritorio, que no existían entonces.
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
#[ignore = "herramienta: regenera la fixture del E2E del túnel"]
fn generar_fixture_tunel_android() {
    let clave = ClaveSincronizacion::desde_texto(CLAVE).expect("clave de prueba válida");
    let elementos = vec![
        elemento(
            "host",
            UUID_HOST,
            datos_host_formato_1(DatosHost {
                nombre: "tunel-prueba".to_string(),
                direccion: "10.0.2.2".to_string(),
                puerto: 12222,
                tipo: TipoHost::Ssh,
                escritorio_dominio: None,
                escritorio_opciones: serde_json::json!({}),
                usuario: Some("usuario".to_string()),
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
            }),
        ),
        elemento(
            "tunel",
            UUID_TUNEL,
            serde_json::to_value(DatosTunel {
                host: UUID_HOST.to_string(),
                nombre: "web".to_string(),
                tipo: "local".to_string(),
                escucha: "127.0.0.1:9090".to_string(),
                destino: Some("127.0.0.1:19091".to_string()),
                automatico: false,
            })
            .unwrap(),
        ),
    ];
    let documento = Documento {
        formato: FORMATO_1,
        app: "1.1.0".to_string(),
        equipo: EquipoDocumento {
            uuid: UUID_EQUIPO.to_string(),
            nombre: "escritorio-tunel".to_string(),
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
    let carpeta = raiz.join("../../android/app/src/androidTest/assets/tunel");
    std::fs::create_dir_all(&carpeta).expect("carpeta de assets");
    let ruta = carpeta.join(format!("magi-{UUID_EQUIPO}.age"));
    std::fs::write(&ruta, &cifrado).expect("escribiendo la fixture del túnel");
    println!("fixture del túnel escrita en {}", ruta.display());
}
