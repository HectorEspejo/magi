//! Generador de la fixture de sincronización para las pruebas instrumentadas
//! de Flota, Servicios y Snippets de la Fase 14 (emulador Android).
//! Herramienta de mantenimiento:
//!
//!   cargo test -p magi-nucleo --test generar_fixture_flota_android -- --ignored --nocapture
//!
//! El fichero apunta al servidor de pruebas del anfitrión
//! (crates/magi-movil/examples/servidor_pruebas.rs): SSH en 127.0.0.1:12222,
//! que el emulador ve en 10.0.2.2:12222, y backups en
//! /tmp/magi-pruebas/backups. Los puertos y la clave de host son fijos porque
//! la prueba instrumentada los lleva escritos en su known_hosts.
//!
//! Contenido:
//! - grupo «produccion» con web-01 (vigilados nginx.service y copia.service;
//!   verificaciones salud + backup con patrón *.sql.gz; snippet al conectar
//!   «ver-estado») y db-01 (vigilado postgresql.service; verificaciones salud +
//!   tests, para que CASPER-3 salga «solo en el escritorio»).
//! - host «caido» en un puerto cerrado del anfitrión (10.0.2.2:19999) para la
//!   fila CAÍDA.
//! - snippets: ver-estado (no crítico, sin variables, al conectar),
//!   reiniciar nginx (crítico y en dos hosts: deliberación) y limpiar journald
//!   (con variable dias, valor por defecto 7).
//! - comprobación propia de nginx.service en web-01 (nginx -t).

use std::collections::BTreeMap;

use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::elementos::{
    DatosComprobacionServicio, DatosGrupo, DatosHost, DatosSnippet, DatosVerificaciones,
    DestinoSnippet,
};
use magi_nucleo::sincronizacion::formato::{
    self, Documento, ElementoDocumento, EquipoDocumento, FORMATO_3,
};

const CLAVE: &str = "AGE-SECRET-KEY-1TSKT6QRX7APKJPLTTJUKRXVATA8H7DAX3ZZH8D5SEQ94R2G3YRDSZSYMTS";
const UUID_EQUIPO: &str = "cccccccc-dddd-4eee-8fff-000000000000";
const UUID_GRUPO: &str = "11111111-aaaa-4bbb-8ccc-dddddddddddd";
const UUID_WEB: &str = "22222222-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const UUID_DB: &str = "33333333-cccc-4ddd-8eee-ffffffffffff";
const UUID_CAIDO: &str = "44444444-dddd-4eee-8fff-000000000000";
const UUID_SNIPPET_ESTADO: &str = "55555555-eeee-4fff-8aaa-111111111111";
const UUID_SNIPPET_NGINX: &str = "66666666-ffff-4aaa-8bbb-222222222222";
const UUID_SNIPPET_JOURNALD: &str = "77777777-aaaa-4bbb-8ccc-333333333333";
const UUID_COMPROBACION: &str = "88888888-bbbb-4ccc-8ddd-444444444444";

/// Ruta de los backups del servidor de pruebas (el host los ve ahí).
pub const BACKUP_RUTA: &str = "/tmp/magi-pruebas/backups";

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

fn host(
    uuid: &str,
    nombre: &str,
    direccion: &str,
    puerto: u16,
    etiquetas: &[&str],
    servicios: &str,
    verificaciones: Option<DatosVerificaciones>,
    snippet_al_conectar: Option<&str>,
) -> ElementoDocumento {
    elemento(
        "host",
        uuid,
        serde_json::to_value(DatosHost {
            nombre: nombre.to_string(),
            direccion: direccion.to_string(),
            puerto,
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
            servicios: servicios.to_string(),
            origen: "manual".to_string(),
            etiquetas: etiquetas
                .iter()
                .map(|etiqueta| etiqueta.to_string())
                .collect(),
            verificaciones,
            snippet_al_conectar: snippet_al_conectar.map(str::to_string),
        })
        .expect("datos del host a JSON"),
    )
}

fn snippet(
    uuid: &str,
    nombre: &str,
    comando: &str,
    descripcion: &str,
    critico: bool,
    destinos: Vec<DestinoSnippet>,
) -> ElementoDocumento {
    elemento(
        "snippet",
        uuid,
        serde_json::to_value(DatosSnippet {
            nombre: nombre.to_string(),
            comando: comando.to_string(),
            descripcion: descripcion.to_string(),
            etiquetas: String::new(),
            critico,
            timeout_seg: 60,
            parar_al_fallo: false,
            destinos,
        })
        .expect("datos del snippet a JSON"),
    )
}

#[test]
#[ignore = "herramienta: regenera la fixture de Flota/Servicios/Snippets del emulador"]
fn generar_fixture_flota_android() {
    let clave = ClaveSincronizacion::desde_texto(CLAVE).expect("clave de prueba válida");
    let mut elementos = vec![elemento(
        "grupo",
        UUID_GRUPO,
        serde_json::to_value(DatosGrupo {
            nombre: "produccion".to_string(),
            orden: 1,
        })
        .expect("grupo a JSON"),
    )];

    // web-01: vigilados nginx.service (activo) y copia.service (fallido, que la
    // fila enseña como culpable y en FALLOS); backup reciente y snippet al
    // conectar no crítico.
    elementos.push(host(
        UUID_WEB,
        "web-01",
        "10.0.2.2",
        12222,
        &["web"],
        "nginx.service\ncopia.service",
        Some(DatosVerificaciones {
            salud: true,
            backup: true,
            backup_ruta: Some(BACKUP_RUTA.to_string()),
            backup_patron: Some("*.sql.gz".to_string()),
            tests: false,
            tests_comando: None,
        }),
        Some(UUID_SNIPPET_ESTADO),
    ));

    // db-01: salud y tests activos. CASPER-3 no se puede hacer en el móvil, así
    // que una deliberación con este host queda BLOQUEADA (AC de §6.6).
    elementos.push(host(
        UUID_DB,
        "db-01",
        "10.0.2.2",
        12222,
        &["db"],
        "postgresql.service",
        Some(DatosVerificaciones {
            salud: true,
            backup: false,
            backup_ruta: None,
            backup_patron: None,
            tests: true,
            tests_comando: Some("true".to_string()),
        }),
        None,
    ));

    // Host caído: un puerto cerrado del anfitrión, para la fila CAÍDA.
    elementos.push(host(
        UUID_CAIDO,
        "caido",
        "10.0.2.2",
        19999,
        &[],
        "",
        None,
        None,
    ));

    // Snippets sincronizados (solo lectura en el móvil).
    elementos.push(snippet(
        UUID_SNIPPET_ESTADO,
        "ver-estado",
        "uptime",
        "Estado del host",
        false,
        vec![DestinoSnippet {
            etiqueta: None,
            host: Some(UUID_WEB.to_string()),
        }],
    ));
    elementos.push(snippet(
        UUID_SNIPPET_NGINX,
        "reiniciar nginx",
        "systemctl restart nginx",
        "Reinicia nginx en los hosts web",
        true,
        vec![
            DestinoSnippet {
                etiqueta: Some("web".to_string()),
                host: None,
            },
            DestinoSnippet {
                etiqueta: None,
                host: Some(UUID_DB.to_string()),
            },
        ],
    ));
    elementos.push(snippet(
        UUID_SNIPPET_JOURNALD,
        "limpiar journald",
        "journalctl --vacuum-time={{dias}}d",
        "Recorta el diario",
        false,
        vec![DestinoSnippet {
            etiqueta: None,
            host: Some(UUID_WEB.to_string()),
        }],
    ));

    // Comprobación propia de nginx.service en web-01: la acción la aplicará
    // antes de recargar o reiniciar.
    elementos.push(elemento(
        "comprobacion_servicio",
        UUID_COMPROBACION,
        serde_json::to_value(DatosComprobacionServicio {
            host: UUID_WEB.to_string(),
            objetivo_tipo: "unidad".to_string(),
            objetivo: "nginx.service".to_string(),
            comando: "nginx -t".to_string(),
        })
        .expect("comprobación a JSON"),
    ));

    let documento = Documento {
        formato: FORMATO_3,
        app: "1.3.0".to_string(),
        equipo: EquipoDocumento {
            uuid: UUID_EQUIPO.to_string(),
            nombre: "escritorio-flota".to_string(),
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
    let carpeta = raiz.join("../../android/app/src/androidTest/assets/flota");
    std::fs::create_dir_all(&carpeta).expect("carpeta de assets de androidTest");
    let ruta = carpeta.join(format!("magi-{UUID_EQUIPO}.age"));
    std::fs::write(&ruta, &cifrado).expect("escribiendo la fixture de Flota");
    println!(
        "fixture de Flota escrita en {} ({} bytes)",
        ruta.display(),
        cifrado.len()
    );
    println!("clave de la fixture: {CLAVE}");
}
