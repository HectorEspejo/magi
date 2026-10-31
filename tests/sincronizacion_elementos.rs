//! Pruebas de la exportación del inventario a candidatos canónicos (Fase 10,
//! §3 y §7.3): qué campos viajan de cada tabla, normalización de las rutas del
//! hogar, serialización estable y estado local con lápidas.
//!
//! El almacén va en memoria y el hogar es una ruta fija de pruebas, así que no
//! hace falta tocar el entorno del proceso.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use magi::almacen::lapidas::{self, Lapida};
use magi::almacen::versiones::{formatear_version, Tabla};
use magi::almacen::Almacen;
use magi::deliberacion::DatosVerificaciones;
use magi::modelo::{
    DatosHost, DatosSincronizacion, DatosTunel, IdentidadRef, Origen, OrigenIdentidad, TipoTunel,
};
use magi::protocolo::Direccion;
use magi::sincronizacion::elementos::{
    datos_de_fila, datos_de_fila_con_hogar, estado_local, ruta_a_home, ruta_desde_home,
};
use magi::sincronizacion::fusion::Candidato;
use magi::snippets::{DatosSnippet, Destino};

const HOGAR: &str = "/home/pruebas-magi";
const EQUIPO: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

const UUID_GRUPO: &str = "11111111-1111-4111-8111-111111111111";
const UUID_SALTO: &str = "22222222-2222-4222-8222-222222222222";
const UUID_HOST: &str = "33333333-3333-4333-8333-333333333333";
const UUID_IDENTIDAD: &str = "44444444-4444-4444-8444-444444444444";
const UUID_TUNEL: &str = "55555555-5555-4555-8555-555555555555";
const UUID_SNIPPET: &str = "66666666-6666-4666-8666-666666666666";
const UUID_SINC: &str = "77777777-7777-4777-8777-777777777777";
const UUID_LAPIDA: &str = "88888888-8888-4888-8888-888888888888";

// Milisegundos fijos para que versiones y vectores sean deterministas.
const MS_GRUPO: i64 = 100;
const MS_SALTO: i64 = 200;
const MS_HOST: i64 = 300;
const MS_IDENTIDAD: i64 = 400;
const MS_TUNEL: i64 = 500;
const MS_SNIPPET: i64 = 600;
const MS_SINC: i64 = 700;
const MS_LAPIDA: i64 = 800;

/// Inventario variado de pruebas con uuids, versiones y vectores fijos.
struct Semilla {
    almacen: Almacen,
    grupo_id: i64,
    salto_id: i64,
    host_id: i64,
    identidad_id: i64,
    tunel_id: i64,
    snippet_id: i64,
    sinc_id: i64,
}

fn version_y_vector(ms: i64) -> (String, String) {
    (
        formatear_version(ms, EQUIPO),
        format!("{{\"{EQUIPO}\":{ms}}}"),
    )
}

fn fijar(almacen: &Almacen, tabla: Tabla, id: i64, uuid: &str, ms: i64) {
    let (version, vector) = version_y_vector(ms);
    almacen
        .conexion()
        .execute(
            &format!(
                "UPDATE {} SET uuid = ?1, version = ?2, vector = ?3 WHERE id = ?4",
                tabla.nombre()
            ),
            rusqlite::params![uuid, version, vector, id],
        )
        .expect("fijando uuid, versión y vector");
}

fn vector_esperado(ms: i64) -> BTreeMap<String, i64> {
    [(EQUIPO.to_string(), ms)].into_iter().collect()
}

fn semilla() -> Semilla {
    let almacen = Almacen::abrir_en_memoria().expect("almacén en memoria");
    let grupo_id = almacen.crear_grupo("flota").expect("grupo");
    let salto_id = almacen
        .crear_host(
            &DatosHost {
                nombre: "salto".to_string(),
                direccion: "192.0.2.1".to_string(),
                usuario: None,
                keepalive_seg: None,
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host de salto");
    let host_id = almacen
        .crear_host(
            &DatosHost {
                nombre: "web-1".to_string(),
                grupo_id: Some(grupo_id),
                direccion: "192.0.2.10".to_string(),
                puerto: 2222,
                usuario: Some("operador".to_string()),
                identidad_ref: IdentidadRef::Fichero(format!("{HOGAR}/.ssh/id_ed25519")),
                salto_host_id: Some(salto_id),
                multiplexar: true,
                keepalive_seg: Some(30),
                opciones_extra: "Compression yes".to_string(),
                servicios: "nginx".to_string(),
                etiquetas: vec!["web".to_string(), "prod".to_string()],
            },
            Origen::Manual,
        )
        .expect("host");
    almacen
        .guardar_verificaciones(
            host_id,
            &DatosVerificaciones {
                salud: true,
                backup: true,
                backup_ruta: Some("/srv/backups".to_string()),
                backup_patron: Some("*.sql.gz".to_string()),
                tests: false,
                tests_comando: None,
            },
        )
        .expect("verificaciones");
    let tunel_id = almacen
        .crear_tunel(&DatosTunel {
            host_id,
            nombre: "db".to_string(),
            tipo: TipoTunel::Local,
            escucha: "127.0.0.1:5432".to_string(),
            destino: Some("db.interno:5432".to_string()),
            automatico: true,
        })
        .expect("túnel");
    let snippet_id = almacen
        .crear_snippet(&DatosSnippet {
            nombre: "reiniciar".to_string(),
            comando: "systemctl restart nginx".to_string(),
            descripcion: "Reinicia nginx".to_string(),
            etiquetas: vec!["prod".to_string(), "ops".to_string()],
            critico: true,
            timeout_seg: 120,
            parar_al_fallo: true,
            destinos: vec![
                Destino::Etiqueta("prod".to_string()),
                Destino::Host {
                    id: host_id,
                    nombre: "web-1".to_string(),
                },
            ],
        })
        .expect("snippet");
    almacen
        .fijar_snippet_al_conectar(host_id, Some(snippet_id))
        .expect("snippet al conectar");
    let sinc_id = almacen
        .crear_sincronizacion(&DatosSincronizacion {
            host_id,
            nombre: "proyecto".to_string(),
            ruta_local: format!("{HOGAR}/proyecto"),
            ruta_remota: "/srv/proyecto".to_string(),
            direccion: Direccion::Subida,
            borrar: true,
            exclusiones: vec!["*.log".to_string(), "tmp/".to_string()],
        })
        .expect("sincronización");
    let identidad_id = almacen
        .crear_identidad(
            "ssh-ed25519",
            "SHA256:huella-de-prueba",
            OrigenIdentidad::Fichero,
            Some(&format!("{HOGAR}/.ssh/id_ed25519.pub")),
            Some("clave de trabajo"),
        )
        .expect("identidad");

    // Campos propios del equipo, marcados para comprobar que no viajan.
    let conexion = almacen.conexion();
    conexion
        .execute(
            "UPDATE HOSTS SET ultimo_estado = 'MARCA_ESTADO',
                    ultima_conexion_en = 'MARCA_CONEXION',
                    sftp_dir_local = 'MARCA_LOCAL', sftp_dir_remoto = 'MARCA_REMOTA',
                    actualizado_en = 'MARCA_ACTUALIZADO', creado_en = 'MARCA_CREADO'
              WHERE id = ?1",
            rusqlite::params![host_id],
        )
        .expect("marcas del host");
    conexion
        .execute(
            "UPDATE IDENTIDADES SET origen = 'MARCA_ORIGEN', ruta = 'MARCA_RUTA',
                    anadida_en = 'MARCA_ANADIDA', ultimo_uso_en = 'MARCA_USO',
                    revocada_en = '2026-10-01T10:00:00+00:00'
              WHERE id = ?1",
            rusqlite::params![identidad_id],
        )
        .expect("marcas de la identidad");
    conexion
        .execute(
            "UPDATE GRUPOS SET creado_en = 'MARCA_CREADO' WHERE id = ?1",
            rusqlite::params![grupo_id],
        )
        .expect("marcas del grupo");
    conexion
        .execute(
            "UPDATE TUNELES SET creado_en = 'MARCA_CREADO',
                    actualizado_en = 'MARCA_ACTUALIZADO' WHERE id = ?1",
            rusqlite::params![tunel_id],
        )
        .expect("marcas del túnel");
    conexion
        .execute(
            "UPDATE SNIPPETS SET usado_veces = 99, ultimo_uso_en = 'MARCA_USO',
                    creado_en = 'MARCA_CREADO', actualizado_en = 'MARCA_ACTUALIZADO'
              WHERE id = ?1",
            rusqlite::params![snippet_id],
        )
        .expect("marcas del snippet");
    conexion
        .execute(
            "UPDATE SINCRONIZACIONES_DIR SET ultima_ejecucion_en = 'MARCA_EJECUCION',
                    ultimo_resultado = 'MARCA_RESULTADO', creado_en = 'MARCA_CREADO',
                    actualizado_en = 'MARCA_ACTUALIZADO'
              WHERE id = ?1",
            rusqlite::params![sinc_id],
        )
        .expect("marcas de la sincronización");
    almacen.alternar_plegado(grupo_id, true).expect("plegado");

    fijar(&almacen, Tabla::Grupos, grupo_id, UUID_GRUPO, MS_GRUPO);
    fijar(&almacen, Tabla::Hosts, salto_id, UUID_SALTO, MS_SALTO);
    fijar(&almacen, Tabla::Hosts, host_id, UUID_HOST, MS_HOST);
    fijar(
        &almacen,
        Tabla::Identidades,
        identidad_id,
        UUID_IDENTIDAD,
        MS_IDENTIDAD,
    );
    fijar(&almacen, Tabla::Tuneles, tunel_id, UUID_TUNEL, MS_TUNEL);
    fijar(
        &almacen,
        Tabla::Snippets,
        snippet_id,
        UUID_SNIPPET,
        MS_SNIPPET,
    );
    fijar(
        &almacen,
        Tabla::SincronizacionesDir,
        sinc_id,
        UUID_SINC,
        MS_SINC,
    );

    Semilla {
        almacen,
        grupo_id,
        salto_id,
        host_id,
        identidad_id,
        tunel_id,
        snippet_id,
        sinc_id,
    }
}

fn exportado(almacen: &Almacen, tabla: Tabla, id: i64) -> (String, Value) {
    let texto = datos_de_fila_con_hogar(almacen.conexion(), tabla, id, Path::new(HOGAR))
        .expect("exportando la fila")
        .expect("la fila existe");
    let valor: Value = serde_json::from_str(&texto).expect("JSON válido");
    (texto, valor)
}

fn candidato<'a>(candidatos: &'a [Candidato], tipo: &str, uuid: &str) -> &'a Candidato {
    candidatos
        .iter()
        .find(|candidato| candidato.tipo == tipo && candidato.uuid == uuid)
        .unwrap_or_else(|| panic!("falta el candidato {tipo} {uuid}"))
}

#[test]
fn grupo_exporta_nombre_y_orden_sin_plegado() {
    let semilla = semilla();
    let (texto, valor) = exportado(&semilla.almacen, Tabla::Grupos, semilla.grupo_id);
    assert_eq!(valor, json!({"nombre": "flota", "orden": 1}));

    // Sin rutas del hogar que normalizar, el hogar del entorno da el mismo JSON.
    let sin_hogar = datos_de_fila(semilla.almacen.conexion(), Tabla::Grupos, semilla.grupo_id)
        .expect("exportando el grupo");
    assert_eq!(sin_hogar.as_deref(), Some(texto.as_str()));
}

#[test]
fn host_exporta_etiquetas_verificaciones_y_referencias() {
    let semilla = semilla();
    let (texto, valor) = exportado(&semilla.almacen, Tabla::Hosts, semilla.host_id);
    assert!(
        texto.starts_with('{'),
        "el JSON canónico es compacto: {texto}"
    );
    assert!(
        !texto.contains("MARCA"),
        "no deben viajar los campos propios del equipo: {texto}"
    );
    assert_eq!(
        valor,
        json!({
            "nombre": "web-1",
            "direccion": "192.0.2.10",
            "puerto": 2222,
            "usuario": "operador",
            "identidad_ref": "fichero:~/.ssh/id_ed25519",
            "salto": UUID_SALTO,
            "grupo": UUID_GRUPO,
            "multiplexar": true,
            "keepalive_seg": 30,
            "opciones_extra": "Compression yes",
            "servicios": "nginx",
            "origen": "manual",
            "etiquetas": ["prod", "web"],
            "verificaciones": {
                "salud": true,
                "backup": true,
                "backup_ruta": "/srv/backups",
                "backup_patron": "*.sql.gz",
                "tests": false,
                "tests_comando": null
            },
            "snippet_al_conectar": UUID_SNIPPET
        })
    );
}

#[test]
fn host_sin_datos_opcionales_exporta_nulos() {
    let semilla = semilla();
    let (texto, valor) = exportado(&semilla.almacen, Tabla::Hosts, semilla.salto_id);
    assert_eq!(
        valor,
        json!({
            "nombre": "salto",
            "direccion": "192.0.2.1",
            "puerto": 22,
            "usuario": null,
            "identidad_ref": null,
            "salto": null,
            "grupo": null,
            "multiplexar": false,
            "keepalive_seg": null,
            "opciones_extra": "",
            "servicios": "",
            "origen": "manual",
            "etiquetas": [],
            "verificaciones": null,
            "snippet_al_conectar": null
        })
    );
    // Este host no tiene rutas del hogar: el hogar del entorno da el mismo JSON.
    let sin_hogar = datos_de_fila(semilla.almacen.conexion(), Tabla::Hosts, semilla.salto_id)
        .expect("exportando el host");
    assert_eq!(sin_hogar.as_deref(), Some(texto.as_str()));
}

#[test]
fn identidad_exporta_sin_origen_ruta_ni_fechas() {
    let semilla = semilla();
    let (texto, valor) = exportado(&semilla.almacen, Tabla::Identidades, semilla.identidad_id);
    assert!(
        !texto.contains("MARCA"),
        "no deben viajar los campos propios del equipo: {texto}"
    );
    assert_eq!(
        valor,
        json!({
            "huella": "SHA256:huella-de-prueba",
            "alias": "clave de trabajo",
            "tipo": "ssh-ed25519",
            "comentario": "clave de trabajo",
            "revocada_en": "2026-10-01T10:00:00+00:00"
        })
    );
}

#[test]
fn tunel_exporta_su_host_por_uuid() {
    let semilla = semilla();
    let (texto, valor) = exportado(&semilla.almacen, Tabla::Tuneles, semilla.tunel_id);
    assert!(
        !texto.contains("MARCA"),
        "no deben viajar los campos propios del equipo: {texto}"
    );
    assert_eq!(
        valor,
        json!({
            "host": UUID_HOST,
            "nombre": "db",
            "tipo": "local",
            "escucha": "127.0.0.1:5432",
            "destino": "db.interno:5432",
            "automatico": true
        })
    );
}

#[test]
fn snippet_exporta_destinos_en_orden_y_sin_usos() {
    let semilla = semilla();
    let (texto, valor) = exportado(&semilla.almacen, Tabla::Snippets, semilla.snippet_id);
    assert!(
        !texto.contains("MARCA"),
        "no deben viajar los campos propios del equipo: {texto}"
    );
    assert_eq!(
        valor,
        json!({
            "nombre": "reiniciar",
            "comando": "systemctl restart nginx",
            "descripcion": "Reinicia nginx",
            "etiquetas": "ops prod",
            "critico": true,
            "timeout_seg": 120,
            "parar_al_fallo": true,
            "destinos": [{"etiqueta": "prod"}, {"host": UUID_HOST}]
        })
    );
}

#[test]
fn sincronizacion_exporta_ruta_local_con_home() {
    let semilla = semilla();
    let (texto, valor) = exportado(
        &semilla.almacen,
        Tabla::SincronizacionesDir,
        semilla.sinc_id,
    );
    assert!(
        !texto.contains("MARCA"),
        "no deben viajar los campos propios del equipo: {texto}"
    );
    assert_eq!(
        valor,
        json!({
            "host": UUID_HOST,
            "nombre": "proyecto",
            "ruta_local": "~/proyecto",
            "ruta_remota": "/srv/proyecto",
            "direccion": "subida",
            "borrar": true,
            "exclusiones": "*.log\ntmp/"
        })
    );
}

#[test]
fn identidad_ref_fuera_del_hogar_se_conserva() {
    let almacen = Almacen::abrir_en_memoria().expect("almacén en memoria");
    let host_id = almacen
        .crear_host(
            &DatosHost {
                nombre: "clave-fuera".to_string(),
                direccion: "192.0.2.20".to_string(),
                identidad_ref: IdentidadRef::Fichero("/opt/claves/id_ed25519".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host");
    fijar(
        &almacen,
        Tabla::Hosts,
        host_id,
        "99999999-9999-4999-8999-999999999999",
        900,
    );
    let (_, valor) = exportado(&almacen, Tabla::Hosts, host_id);
    assert_eq!(
        valor["identidad_ref"],
        json!("fichero:/opt/claves/id_ed25519")
    );
}

#[test]
fn identidad_ref_que_no_es_fichero_se_conserva() {
    let almacen = Almacen::abrir_en_memoria().expect("almacén en memoria");
    let host_id = almacen
        .crear_host(
            &DatosHost {
                nombre: "agente".to_string(),
                direccion: "192.0.2.21".to_string(),
                identidad_ref: IdentidadRef::Agente("SHA256:huella".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host");
    fijar(
        &almacen,
        Tabla::Hosts,
        host_id,
        "99999999-9999-4999-8999-999999999998",
        901,
    );
    let (_, valor) = exportado(&almacen, Tabla::Hosts, host_id);
    assert_eq!(valor["identidad_ref"], json!("agente:SHA256:huella"));
}

#[test]
fn referencia_a_grupo_borrado_queda_nula() {
    let semilla = semilla();
    semilla
        .almacen
        .borrar_grupo(semilla.grupo_id)
        .expect("borrando el grupo");
    let (_, valor) = exportado(&semilla.almacen, Tabla::Hosts, semilla.host_id);
    assert_eq!(valor["grupo"], Value::Null);
}

#[test]
fn datos_de_fila_devuelve_none_si_la_fila_no_existe() {
    let almacen = Almacen::abrir_en_memoria().expect("almacén en memoria");
    for tabla in Tabla::todas() {
        assert_eq!(
            datos_de_fila(almacen.conexion(), tabla, 12345).expect("consultando"),
            None,
            "{}",
            tabla.nombre()
        );
        assert_eq!(
            datos_de_fila_con_hogar(almacen.conexion(), tabla, 0, Path::new(HOGAR))
                .expect("consultando"),
            None,
            "{}",
            tabla.nombre()
        );
    }
}

#[test]
fn estado_local_devuelve_vivos_y_lapidas() {
    let semilla = semilla();
    lapidas::guardar(
        semilla.almacen.conexion(),
        &Lapida {
            uuid: UUID_LAPIDA.to_string(),
            tipo: "host".to_string(),
            nombre: "viejo".to_string(),
            version: formatear_version(MS_LAPIDA, EQUIPO),
            vector: version_y_vector(MS_LAPIDA).1,
            borrado_en: "2026-10-02T09:00:00+00:00".to_string(),
        },
    )
    .expect("lápida");

    let candidatos =
        estado_local(semilla.almacen.conexion(), Path::new(HOGAR)).expect("estado local");

    // Seis tablas: grupo, dos hosts, identidad, túnel, snippet y sincronización,
    // más una lápida.
    assert_eq!(candidatos.len(), 8, "candidatos: {candidatos:?}");

    // Todos los vivos están, con su versión y su vector parseados.
    for (tabla, id, uuid, ms) in [
        (Tabla::Grupos, semilla.grupo_id, UUID_GRUPO, MS_GRUPO),
        (Tabla::Hosts, semilla.salto_id, UUID_SALTO, MS_SALTO),
        (Tabla::Hosts, semilla.host_id, UUID_HOST, MS_HOST),
        (
            Tabla::Identidades,
            semilla.identidad_id,
            UUID_IDENTIDAD,
            MS_IDENTIDAD,
        ),
        (Tabla::Tuneles, semilla.tunel_id, UUID_TUNEL, MS_TUNEL),
        (
            Tabla::Snippets,
            semilla.snippet_id,
            UUID_SNIPPET,
            MS_SNIPPET,
        ),
        (
            Tabla::SincronizacionesDir,
            semilla.sinc_id,
            UUID_SINC,
            MS_SINC,
        ),
    ] {
        let candidato = candidato(&candidatos, tabla.tipo_elemento(), uuid);
        assert!(!candidato.es_lapida(), "{} no es lápida", tabla.nombre());
        assert_eq!(candidato.version, formatear_version(ms, EQUIPO));
        assert_eq!(candidato.vector, vector_esperado(ms));
        assert_eq!(candidato.borrado_en, None);
        assert!(
            candidato.contenido.is_some(),
            "{} sin contenido",
            tabla.nombre()
        );

        // El contenido es byte a byte el mismo JSON canónico de datos_de_fila.
        let directo =
            datos_de_fila_con_hogar(semilla.almacen.conexion(), tabla, id, Path::new(HOGAR))
                .expect("exportando la fila")
                .expect("la fila existe");
        assert_eq!(
            candidato.contenido_json().as_deref(),
            Some(directo.as_str()),
            "contenido distinto para {}",
            tabla.nombre()
        );
    }

    let host = candidato(&candidatos, "host", UUID_HOST);
    assert_eq!(host.nombre, "web-1");

    let lapida = candidato(&candidatos, "host", UUID_LAPIDA);
    assert!(lapida.es_lapida());
    assert_eq!(lapida.nombre, "viejo");
    assert_eq!(lapida.contenido, None);
    assert_eq!(
        lapida.borrado_en.as_deref(),
        Some("2026-10-02T09:00:00+00:00")
    );
    assert_eq!(lapida.version, formatear_version(MS_LAPIDA, EQUIPO));
    assert_eq!(lapida.vector, vector_esperado(MS_LAPIDA));
}

#[test]
fn estado_local_omite_filas_sin_uuid_version_o_vector() {
    let almacen = Almacen::abrir_en_memoria().expect("almacén en memoria");
    almacen
        .conexion()
        .execute_batch(
            "INSERT INTO GRUPOS (nombre, orden, plegado, creado_en)
                 VALUES ('sin-uuid', 1, 0, '2026-01-01T00:00:00+00:00');
             INSERT INTO GRUPOS (nombre, orden, plegado, creado_en, uuid)
                 VALUES ('sin-version', 2, 0, '2026-01-01T00:00:00+00:00',
                         '11111111-1111-4111-8111-111111111111');
             INSERT INTO GRUPOS (nombre, orden, plegado, creado_en, uuid, version)
                 VALUES ('sin-vector', 3, 0, '2026-01-01T00:00:00+00:00',
                         '22222222-2222-4222-8222-222222222222',
                         '0000000000001.equipo');
             INSERT INTO GRUPOS (nombre, orden, plegado, creado_en, uuid, version, vector)
                 VALUES ('vector-vacio', 4, 0, '2026-01-01T00:00:00+00:00',
                         '33333333-3333-4333-8333-333333333333',
                         '0000000000002.equipo', '{}');",
        )
        .expect("filas incompletas");
    let candidatos = estado_local(almacen.conexion(), Path::new(HOGAR)).expect("estado local");
    assert!(
        candidatos.is_empty(),
        "no deben exportarse filas incompletas: {candidatos:?}"
    );
}

#[test]
fn rutas_del_hogar_se_exportan_y_expanden() {
    let hogar = Path::new(HOGAR);
    assert_eq!(
        ruta_a_home("/home/pruebas-magi/.ssh/id_ed25519", hogar),
        "~/.ssh/id_ed25519"
    );
    assert_eq!(ruta_a_home("/home/pruebas-magi", hogar), "~");
    assert_eq!(ruta_a_home("/home/pruebas-magi/", hogar), "~");
    // El límite es por componentes: un hermano no cuelga del hogar.
    assert_eq!(
        ruta_a_home("/home/pruebas-magix/otro", hogar),
        "/home/pruebas-magix/otro"
    );
    assert_eq!(ruta_a_home("~/.ssh/id_ed25519", hogar), "~/.ssh/id_ed25519");
    assert_eq!(ruta_a_home("/srv/proyecto", hogar), "/srv/proyecto");
    assert_eq!(
        ruta_desde_home("~/.ssh/id_ed25519", hogar),
        "/home/pruebas-magi/.ssh/id_ed25519"
    );
    assert_eq!(ruta_desde_home("~", hogar), HOGAR);
    assert_eq!(ruta_desde_home("/srv/proyecto", hogar), "/srv/proyecto");

    let original = "/home/pruebas-magi/proyecto";
    assert_eq!(
        ruta_desde_home(&ruta_a_home(original, hogar), hogar),
        original
    );
}
