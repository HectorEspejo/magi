//! Tests de la fusión determinista por versiones y vectores (Fase 10, §7.2 y
//! §7.3): dominancia, concurrencia, lápidas, huérfanos, nombres, vector máximo
//! y determinismo con permutaciones.

use std::collections::BTreeMap;

use magi::sincronizacion::formato::{
    Documento, ElementoDocumento, EquipoDocumento, LapidaDocumento,
};
use magi::sincronizacion::fusion::{
    fusionar, nombre_de_contenido, resolver_nombres, Candidato, ConflictoFusion, PlanFusion,
};
use magi::sincronizacion::reloj::formatear_version;
use serde_json::{json, Value};

// Equipos (uuid de equipo de la versión).
const A: &str = "aaaaaaaa-1111-4111-8111-11111111111a";
const B: &str = "bbbbbbbb-2222-4222-8222-22222222222b";
const C: &str = "cccccccc-3333-4333-8333-33333333333c";
const D: &str = "dddddddd-4444-4444-8444-44444444444d";

// uuids de elementos.
const H1: &str = "00000000-0000-4000-8000-000000000001";
const H2: &str = "00000000-0000-4000-8000-000000000002";
const H3: &str = "00000000-0000-4000-8000-000000000003";
const H4: &str = "00000000-0000-4000-8000-000000000004";
const H5: &str = "00000000-0000-4000-8000-000000000005";
const H6: &str = "00000000-0000-4000-8000-000000000006";
const H7: &str = "00000000-0000-4000-8000-000000000007";
const H_MUERTO: &str = "00000000-0000-4000-8000-00000000000d";
const T1: &str = "11111111-1111-4111-8111-111111111111";
const T2: &str = "11111111-1111-4111-8111-222222222222";
const T3: &str = "11111111-1111-4111-8111-333333333333";
const S1: &str = "22222222-2222-4222-8222-222222222222";
const G1: &str = "33333333-3333-4333-8333-333333333333";
const G2: &str = "33333333-3333-4333-8333-444444444444";
const I1: &str = "44444444-4444-4444-8444-444444444444";

fn vector(pares: &[(&str, i64)]) -> BTreeMap<String, i64> {
    pares
        .iter()
        .map(|(equipo, ms)| (equipo.to_string(), *ms))
        .collect()
}

fn candidato_vivo(
    tipo: &str,
    uuid: &str,
    ms: i64,
    equipo: &str,
    pares: &[(&str, i64)],
    datos: Value,
) -> Candidato {
    Candidato {
        tipo: tipo.to_string(),
        uuid: uuid.to_string(),
        nombre: nombre_de_contenido(tipo, &datos),
        version: formatear_version(ms, equipo),
        vector: vector(pares),
        contenido: Some(datos),
        borrado_en: None,
    }
}

fn candidato_lapida(
    tipo: &str,
    uuid: &str,
    nombre: &str,
    ms: i64,
    equipo: &str,
    pares: &[(&str, i64)],
) -> Candidato {
    Candidato {
        tipo: tipo.to_string(),
        uuid: uuid.to_string(),
        nombre: nombre.to_string(),
        version: formatear_version(ms, equipo),
        vector: vector(pares),
        contenido: None,
        borrado_en: Some("2026-10-31T10:00:00+01:00".to_string()),
    }
}

fn elemento_de(candidato: &Candidato) -> ElementoDocumento {
    ElementoDocumento {
        tipo: candidato.tipo.clone(),
        uuid: candidato.uuid.clone(),
        version: candidato.version.clone(),
        vector: candidato.vector.clone(),
        datos: candidato
            .contenido
            .clone()
            .expect("un elemento vivo tiene contenido"),
    }
}

fn lapida_de(candidato: &Candidato) -> LapidaDocumento {
    LapidaDocumento {
        tipo: candidato.tipo.clone(),
        uuid: candidato.uuid.clone(),
        nombre: candidato.nombre.clone(),
        version: candidato.version.clone(),
        vector: candidato.vector.clone(),
        borrado_en: candidato
            .borrado_en
            .clone()
            .expect("una lápida tiene fecha"),
    }
}

fn documento(equipo: &str, elementos: &[&Candidato], lapidas: &[&Candidato]) -> Documento {
    Documento {
        formato: 1,
        app: "1.1.0".to_string(),
        equipo: EquipoDocumento {
            uuid: equipo.to_string(),
            nombre: format!("equipo {equipo}"),
            plataforma: "linux".to_string(),
        },
        generado_en: "2026-10-31T10:00:00+01:00".to_string(),
        reloj_ms: 0,
        elementos: elementos
            .iter()
            .map(|candidato| elemento_de(candidato))
            .collect(),
        lapidas: lapidas
            .iter()
            .map(|candidato| lapida_de(candidato))
            .collect(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    }
}

fn buscar_vivo<'a>(plan: &'a PlanFusion, uuid: &str) -> &'a Candidato {
    plan.vivos
        .iter()
        .find(|candidato| candidato.uuid == uuid)
        .unwrap_or_else(|| panic!("no hay vivo con uuid {uuid}"))
}

fn buscar_lapida<'a>(plan: &'a PlanFusion, uuid: &str) -> &'a Candidato {
    plan.lapidas
        .iter()
        .find(|candidato| candidato.uuid == uuid)
        .unwrap_or_else(|| panic!("no hay lápida con uuid {uuid}"))
}

fn conflictos_de_tipo<'a>(plan: &'a PlanFusion, tipo: &str) -> Vec<&'a ConflictoFusion> {
    plan.conflictos
        .iter()
        .filter(|conflicto| conflicto.tipo == tipo)
        .collect()
}

fn nombres_por_uuid(vivos: &[Candidato]) -> BTreeMap<String, String> {
    vivos
        .iter()
        .map(|candidato| (candidato.uuid.clone(), candidato.nombre.clone()))
        .collect()
}

fn mismo_plan(referencia: &PlanFusion, otro: &PlanFusion) {
    assert_eq!(otro.vivos, referencia.vivos, "difieren los vivos");
    assert_eq!(otro.lapidas, referencia.lapidas, "difieren las lápidas");
    assert_eq!(
        otro.renombrados, referencia.renombrados,
        "difieren los renombrados"
    );
    assert_eq!(
        otro.conflictos, referencia.conflictos,
        "difieren los conflictos"
    );
    assert_eq!(otro.reloj_ms, referencia.reloj_ms, "difiere el reloj");
}

/// Todas las permutaciones de una lista (para los tests de propiedades).
fn permutaciones<T: Clone>(elementos: &[T]) -> Vec<Vec<T>> {
    if elementos.len() <= 1 {
        return vec![elementos.to_vec()];
    }
    let mut resultado = Vec::new();
    for (indice, elemento) in elementos.iter().enumerate() {
        let mut resto = Vec::with_capacity(elementos.len() - 1);
        resto.extend_from_slice(&elementos[..indice]);
        resto.extend_from_slice(&elementos[indice + 1..]);
        for mut permutacion in permutaciones(&resto) {
            permutacion.insert(0, elemento.clone());
            resultado.push(permutacion);
        }
    }
    resultado
}

// ------------------------------------------------------------ fusión básica

#[test]
fn sin_candidatos_el_plan_queda_vacio() {
    let plan = fusionar(&[], &[]).expect("fusión sin candidatos");
    assert!(plan.vivos.is_empty());
    assert!(plan.lapidas.is_empty());
    assert!(plan.renombrados.is_empty());
    assert!(plan.conflictos.is_empty());
    assert_eq!(plan.reloj_ms, 0);
}

#[test]
fn la_dominancia_aplica_la_version_posterior_sin_conflicto() {
    // A edita (t=100) y B importa y vuelve a editar (t=200): al importar A, el
    // vector de B domina al de A y se aplica sin conflicto (AC del checklist).
    let datos_a = json!({"nombre": "web", "direccion": "192.0.2.5"});
    let datos_b = json!({"nombre": "web", "direccion": "192.0.2.7"});
    let a = candidato_vivo("host", H1, 100, A, &[(A, 100)], datos_a);
    let b = candidato_vivo("host", H1, 200, B, &[(A, 100), (B, 200)], datos_b);
    let vector_b = vector(&[(A, 100), (B, 200)]);

    // Local B (estado ya fusionado) + fichero de A: gana B por dominancia.
    let plan = fusionar(std::slice::from_ref(&b), &[documento(A, &[&a], &[])]).expect("fusión");
    assert_eq!(plan.vivos.len(), 1);
    assert_eq!(plan.vivos[0].version, b.version);
    assert_eq!(plan.vivos[0].vector, vector_b);
    assert!(
        plan.conflictos.is_empty(),
        "la dominancia no deja conflicto"
    );
    assert_eq!(plan.reloj_ms, 200);

    // Local A + fichero de B: gana B y el local no deja conflicto (no es
    // concurrente, deriva de su versión).
    let plan = fusionar(&[a], &[documento(B, &[&b], &[])]).expect("fusión");
    assert_eq!(plan.vivos.len(), 1);
    assert_eq!(plan.vivos[0].version, b.version);
    assert_eq!(plan.vivos[0].vector, vector_b);
    assert!(plan.conflictos.is_empty(), "el dominante no deja conflicto");
}

#[test]
fn los_concurrentes_ganan_por_version_y_el_local_perdedor_deja_conflicto() {
    let datos_a = json!({"nombre": "web", "direccion": "192.0.2.5"});
    let datos_b = json!({"nombre": "web", "direccion": "192.0.2.7"});
    let local = candidato_vivo("host", H1, 100, A, &[(A, 100)], datos_a.clone());
    let entrante = candidato_vivo("host", H1, 120, B, &[(B, 120)], datos_b);

    let plan = fusionar(
        std::slice::from_ref(&local),
        &[documento(B, &[&entrante], &[])],
    )
    .expect("fusión");
    assert_eq!(plan.vivos.len(), 1);
    assert_eq!(plan.vivos[0].version, entrante.version);
    assert_eq!(plan.vivos[0].vector, vector(&[(A, 100), (B, 120)]));
    assert_eq!(plan.conflictos.len(), 1);
    let conflicto = &plan.conflictos[0];
    assert_eq!(conflicto.tipo, "elemento");
    assert_eq!(conflicto.elemento_tipo, "host");
    assert_eq!(conflicto.uuid, H1);
    assert_eq!(
        conflicto.version_perdedora_json,
        Some(serde_json::to_string(&datos_a).expect("JSON"))
    );
    assert_eq!(conflicto.version_ganadora, Some(entrante.version.clone()));
    assert_eq!(conflicto.equipo_ganador.as_deref(), Some(B));

    // Desde el equipo B, que gana: no hay conflicto y los dos planes convergen.
    let plan_b = fusionar(&[entrante], &[documento(A, &[&local], &[])]).expect("fusión");
    assert!(plan_b.conflictos.is_empty());
    assert_eq!(plan_b.vivos, plan.vivos);
}

#[test]
fn el_ganador_local_actualiza_el_vector_al_maximo_sin_conflicto() {
    let local = candidato_vivo(
        "host",
        H1,
        120,
        B,
        &[(A, 100), (B, 120)],
        json!({"nombre": "web"}),
    );
    let entrante = candidato_vivo("host", H1, 110, C, &[(C, 110)], json!({"nombre": "web"}));
    let plan = fusionar(&[local], &[documento(C, &[&entrante], &[])]).expect("fusión");
    assert_eq!(plan.vivos.len(), 1);
    assert_eq!(plan.vivos[0].version, formatear_version(120, B));
    assert_eq!(
        plan.vivos[0].vector,
        vector(&[(A, 100), (B, 120), (C, 110)])
    );
    assert!(
        plan.conflictos.is_empty(),
        "el local ganó: no hay conflicto"
    );
}

// ------------------------------------------------------------------ lápidas

#[test]
fn un_borrado_posterior_gana_a_la_edicion_anterior() {
    let vivo = candidato_vivo("host", H1, 100, A, &[(A, 100)], json!({"nombre": "web"}));
    let borrado = candidato_lapida("host", H1, "web", 120, B, &[(A, 100), (B, 120)]);
    let plan = fusionar(&[vivo], &[documento(B, &[], &[&borrado])]).expect("fusión");
    assert!(plan.vivos.is_empty(), "el host queda borrado");
    assert_eq!(plan.lapidas.len(), 1);
    assert_eq!(plan.lapidas[0].version, borrado.version);
    assert_eq!(plan.lapidas[0].vector, vector(&[(A, 100), (B, 120)]));
    assert!(
        plan.conflictos.is_empty(),
        "el borrado deriva de la edición"
    );
}

#[test]
fn una_edicion_posterior_resucita_y_la_lapida_se_conserva() {
    let lapida = candidato_lapida("host", H1, "web", 100, A, &[(A, 100)]);
    let vivo = candidato_vivo(
        "host",
        H1,
        150,
        B,
        &[(A, 100), (B, 150)],
        json!({"nombre": "web"}),
    );
    let plan = fusionar(
        std::slice::from_ref(&lapida),
        &[documento(B, &[&vivo], &[])],
    )
    .expect("fusión");
    assert_eq!(plan.vivos.len(), 1);
    assert_eq!(plan.vivos[0].version, vivo.version);
    assert_eq!(plan.vivos[0].vector, vector(&[(A, 100), (B, 150)]));
    // La lápida del universo se conserva, con su propio vector.
    assert_eq!(plan.lapidas.len(), 1);
    assert_eq!(plan.lapidas[0].version, lapida.version);
    assert_eq!(plan.lapidas[0].vector, vector(&[(A, 100)]));
    assert!(
        plan.conflictos.is_empty(),
        "una lápida local que pierde no recupera nada"
    );
}

#[test]
fn un_contenido_vivo_local_que_pierde_ante_una_lapida_deja_conflicto() {
    let datos = json!({"nombre": "web", "direccion": "192.0.2.5"});
    let local = candidato_vivo("host", H1, 100, A, &[(A, 100)], datos.clone());
    // B borra sin haber visto la edición de A: son concurrentes.
    let borrado = candidato_lapida("host", H1, "web", 120, B, &[(B, 120)]);
    let plan = fusionar(&[local], &[documento(B, &[], &[&borrado])]).expect("fusión");
    assert!(plan.vivos.is_empty());
    assert_eq!(plan.lapidas.len(), 1);
    assert_eq!(plan.lapidas[0].vector, vector(&[(A, 100), (B, 120)]));
    assert_eq!(plan.conflictos.len(), 1);
    let conflicto = &plan.conflictos[0];
    assert_eq!(conflicto.tipo, "elemento");
    assert_eq!(
        conflicto.version_perdedora_json,
        Some(serde_json::to_string(&datos).expect("JSON")),
        "la versión viva local se guarda para poder recuperarla"
    );
    assert_eq!(conflicto.version_ganadora, Some(borrado.version.clone()));
    assert_eq!(conflicto.equipo_ganador.as_deref(), Some(B));
}

#[test]
fn una_lapida_local_que_pierde_ante_una_resurreccion_no_deja_conflicto() {
    let lapida = candidato_lapida("host", H1, "web", 100, A, &[(A, 100)]);
    let vivo = candidato_vivo("host", H1, 120, B, &[(B, 120)], json!({"nombre": "web"}));
    let plan = fusionar(&[lapida], &[documento(B, &[&vivo], &[])]).expect("fusión");
    assert_eq!(plan.vivos.len(), 1);
    assert_eq!(plan.lapidas.len(), 1);
    assert!(plan.conflictos.is_empty(), "no hay contenido que recuperar");
}

// ---------------------------------------------------------------- huérfanos

#[test]
fn el_tunel_sin_host_vivo_se_descarta_como_huerfano_y_su_lapida_se_conserva() {
    let host = candidato_vivo("host", H1, 100, A, &[(A, 100)], json!({"nombre": "web"}));
    let tunel = candidato_vivo(
        "tunel",
        T1,
        110,
        A,
        &[(A, 110)],
        json!({"host": H1, "nombre": "salto"}),
    );
    // Lápida antigua del túnel: el elemento resucitó, pero se conserva.
    let lapida_tunel = candidato_lapida("tunel", T1, "salto", 90, A, &[(A, 90)]);
    // Otro equipo borra el host después de la edición de A.
    let borrado_host = candidato_lapida("host", H1, "web", 120, B, &[(A, 100), (B, 120)]);
    let plan = fusionar(
        &[host, tunel.clone(), lapida_tunel.clone()],
        &[documento(B, &[], &[&borrado_host])],
    )
    .expect("fusión");
    assert!(plan.vivos.is_empty(), "el túnel huérfano no queda vivo");
    assert_eq!(
        plan.lapidas.len(),
        2,
        "quedan la lápida del host y la del túnel"
    );
    assert_eq!(buscar_lapida(&plan, H1).version, borrado_host.version);
    assert_eq!(buscar_lapida(&plan, T1).version, lapida_tunel.version);
    assert_eq!(plan.conflictos.len(), 1);
    let conflicto = &plan.conflictos[0];
    assert_eq!(conflicto.tipo, "huerfano");
    assert_eq!(conflicto.elemento_tipo, "tunel");
    assert_eq!(conflicto.uuid, T1);
    assert_eq!(conflicto.nombre, "salto");
    assert_eq!(conflicto.version_ganadora, Some(tunel.version.clone()));
}

#[test]
fn la_sincronizacion_cuyo_host_no_existe_es_huerfana() {
    let sincronizacion = candidato_vivo(
        "sincronizacion_dir",
        S1,
        100,
        A,
        &[(A, 100)],
        json!({"host": H_MUERTO, "nombre": "respaldo"}),
    );
    let plan = fusionar(&[sincronizacion], &[]).expect("fusión");
    assert!(plan.vivos.is_empty());
    assert_eq!(plan.conflictos.len(), 1);
    assert_eq!(plan.conflictos[0].tipo, "huerfano");
    assert_eq!(plan.conflictos[0].elemento_tipo, "sincronizacion_dir");
    assert_eq!(plan.conflictos[0].uuid, S1);
}

// ------------------------------------------------------------------ nombres

#[test]
fn los_nombres_repetidos_se_renombran_por_uuid() {
    let mut vivos = vec![
        candidato_vivo("host", H1, 100, A, &[(A, 100)], json!({"nombre": "web"})),
        candidato_vivo("host", H4, 100, A, &[(A, 100)], json!({"nombre": "web"})),
        candidato_vivo("host", H6, 100, A, &[(A, 100)], json!({"nombre": "web"})),
    ];
    let renombrados = resolver_nombres(&mut vivos);
    assert_eq!(
        renombrados,
        vec![
            ("host".to_string(), H4.to_string()),
            ("host".to_string(), H6.to_string()),
        ]
    );
    assert_eq!(vivos[0].nombre, "web");
    assert_eq!(vivos[1].nombre, "web (2)");
    assert_eq!(vivos[2].nombre, "web (3)");
    assert_eq!(
        vivos[1].contenido.as_ref().expect("contenido")["nombre"],
        json!("web (2)"),
        "el contenido también se renombra"
    );
    assert_eq!(
        vivos[2].contenido.as_ref().expect("contenido")["nombre"],
        json!("web (3)")
    );
}

#[test]
fn el_primer_numero_libre_evita_los_nombres_ocupados() {
    let mut vivos = vec![
        candidato_vivo(
            "host",
            H1,
            100,
            A,
            &[(A, 100)],
            json!({"nombre": "web (2)"}),
        ),
        candidato_vivo("host", H4, 100, A, &[(A, 100)], json!({"nombre": "web"})),
        candidato_vivo("host", H6, 100, A, &[(A, 100)], json!({"nombre": "web"})),
    ];
    let renombrados = resolver_nombres(&mut vivos);
    assert_eq!(renombrados, vec![("host".to_string(), H6.to_string())]);
    assert_eq!(vivos[0].nombre, "web (2)");
    assert_eq!(vivos[1].nombre, "web");
    assert_eq!(vivos[2].nombre, "web (3)", "el (2) ya estaba ocupado");
}

#[test]
fn dos_tuneles_con_el_mismo_nombre_en_hosts_distintos_no_chocan() {
    let mut vivos = vec![
        candidato_vivo(
            "tunel",
            T1,
            100,
            A,
            &[(A, 100)],
            json!({"host": H1, "nombre": "db"}),
        ),
        candidato_vivo(
            "tunel",
            T2,
            100,
            A,
            &[(A, 100)],
            json!({"host": H2, "nombre": "db"}),
        ),
        // En el mismo host sí chocan: el de uuid mayor pasa a "db (2)".
        candidato_vivo(
            "tunel",
            T3,
            100,
            A,
            &[(A, 100)],
            json!({"host": H1, "nombre": "db"}),
        ),
    ];
    let renombrados = resolver_nombres(&mut vivos);
    assert_eq!(renombrados, vec![("tunel".to_string(), T3.to_string())]);
    assert_eq!(vivos[1].nombre, "db", "otro host: no se toca");
    assert_eq!(vivos[2].nombre, "db (2)");
}

#[test]
fn el_mismo_uuid_no_se_renombra_contra_si_mismo() {
    let mut vivos = vec![
        candidato_vivo("host", H1, 100, A, &[(A, 100)], json!({"nombre": "web"})),
        candidato_vivo("host", H1, 100, A, &[(A, 100)], json!({"nombre": "web"})),
    ];
    let renombrados = resolver_nombres(&mut vivos);
    assert!(renombrados.is_empty());
    assert_eq!(vivos[0].nombre, "web");
    assert_eq!(vivos[1].nombre, "web");
}

#[test]
fn la_colision_de_nombres_en_la_fusion_genera_conflicto_nombre() {
    let local = candidato_vivo("host", H1, 100, A, &[(A, 100)], json!({"nombre": "web"}));
    let entrante = candidato_vivo("host", H4, 120, B, &[(B, 120)], json!({"nombre": "web"}));
    let plan = fusionar(&[local], &[documento(B, &[&entrante], &[])]).expect("fusión");
    assert_eq!(plan.vivos.len(), 2);
    assert_eq!(buscar_vivo(&plan, H1).nombre, "web");
    assert_eq!(buscar_vivo(&plan, H4).nombre, "web (2)");
    assert_eq!(plan.renombrados, vec![("host".to_string(), H4.to_string())]);
    let nombres = conflictos_de_tipo(&plan, "nombre");
    assert_eq!(nombres.len(), 1);
    assert_eq!(nombres[0].elemento_tipo, "host");
    assert_eq!(nombres[0].uuid, H4);
    assert_eq!(nombres[0].nombre, "web (2)");
    assert_eq!(nombres[0].version_ganadora, Some(entrante.version.clone()));
    assert_eq!(nombres[0].equipo_ganador.as_deref(), Some(B));
}

#[test]
fn resolver_nombres_es_determinista_con_cualquier_orden() {
    let base = vec![
        candidato_vivo("host", H1, 100, A, &[(A, 100)], json!({"nombre": "web"})),
        candidato_vivo("host", H4, 100, A, &[(A, 100)], json!({"nombre": "web"})),
        candidato_vivo("host", H6, 100, A, &[(A, 100)], json!({"nombre": "web"})),
        candidato_vivo(
            "tunel",
            T1,
            100,
            A,
            &[(A, 100)],
            json!({"host": H1, "nombre": "salto"}),
        ),
        candidato_vivo(
            "tunel",
            T2,
            100,
            A,
            &[(A, 100)],
            json!({"host": H1, "nombre": "salto"}),
        ),
    ];
    let mut referencia = base.clone();
    let renombrados = resolver_nombres(&mut referencia);
    let nombres = nombres_por_uuid(&referencia);
    // Dos hosts "web" y un túnel "salto" (el de uuid mayor de cada colisión).
    assert_eq!(renombrados.len(), 3);
    for mut permutacion in permutaciones(&base) {
        let otros_renombrados = resolver_nombres(&mut permutacion);
        assert_eq!(otros_renombrados, renombrados);
        assert_eq!(nombres_por_uuid(&permutacion), nombres);
    }
}

// ----------------------------------------------------- vector y reloj finales

#[test]
fn el_vector_resultante_es_el_maximo_de_todos_y_el_reloj_el_mayor_visto() {
    let local = candidato_vivo(
        "host",
        H1,
        100,
        A,
        &[(A, 100)],
        json!({"nombre": "web", "direccion": "192.0.2.5"}),
    );
    let b = candidato_vivo(
        "host",
        H1,
        120,
        B,
        &[(B, 120)],
        json!({"nombre": "web", "direccion": "192.0.2.7"}),
    );
    let c = candidato_vivo(
        "host",
        H1,
        110,
        C,
        &[(C, 110)],
        json!({"nombre": "web", "direccion": "192.0.2.6"}),
    );
    let plan = fusionar(
        &[local],
        &[documento(B, &[&b], &[]), documento(C, &[&c], &[])],
    )
    .expect("fusión");
    assert_eq!(plan.vivos.len(), 1);
    assert_eq!(plan.vivos[0].version, formatear_version(120, B));
    assert_eq!(
        plan.vivos[0].vector,
        vector(&[(A, 100), (B, 120), (C, 110)])
    );
    assert_eq!(plan.reloj_ms, 120);
    assert_eq!(plan.conflictos.len(), 1, "solo pierde el local");
}

// -------------------------------------------------------------- determinismo

/// Universo con todos los casos: dominancia, concurrencia, lápidas,
/// resurrección, huérfanos y colisiones de nombre.
fn universo_determinista() -> (Vec<Candidato>, Vec<Documento>) {
    let local = vec![
        candidato_vivo(
            "host",
            H1,
            100,
            A,
            &[(A, 100)],
            json!({"nombre": "web", "direccion": "192.0.2.1"}),
        ),
        candidato_vivo(
            "host",
            H2,
            150,
            A,
            &[(A, 150), (B, 120)],
            json!({"nombre": "db", "direccion": "192.0.2.2"}),
        ),
        candidato_lapida("host", H3, "viejo", 90, A, &[(A, 90)]),
        candidato_vivo(
            "tunel",
            T1,
            110,
            A,
            &[(A, 110)],
            json!({"host": H1, "nombre": "salto"}),
        ),
        candidato_vivo(
            "host",
            H4,
            100,
            A,
            &[(A, 100)],
            json!({"nombre": "web", "direccion": "192.0.2.4"}),
        ),
        candidato_vivo(
            "grupo",
            G1,
            100,
            A,
            &[(A, 100)],
            json!({"nombre": "prod", "orden": 0}),
        ),
        candidato_vivo(
            "identidad",
            I1,
            100,
            A,
            &[(A, 100)],
            json!({"alias": "id1", "huella": "SHA256:x", "tipo": "ed25519",
                   "comentario": null, "revocada_en": null}),
        ),
        candidato_vivo(
            "sincronizacion_dir",
            S1,
            100,
            A,
            &[(A, 100)],
            json!({"host": H_MUERTO, "nombre": "respaldo"}),
        ),
    ];

    let b1 = candidato_vivo(
        "host",
        H1,
        120,
        B,
        &[(B, 120)],
        json!({"nombre": "web", "direccion": "192.0.2.9"}),
    );
    let b2 = candidato_vivo(
        "host",
        H2,
        140,
        B,
        &[(B, 140)],
        json!({"nombre": "db", "direccion": "192.0.2.2"}),
    );
    let b3 = candidato_lapida("host", H5, "nas", 100, B, &[(B, 100)]);
    let b4 = candidato_vivo(
        "tunel",
        T2,
        125,
        B,
        &[(B, 125)],
        json!({"host": H1, "nombre": "salto"}),
    );
    let b5 = candidato_vivo(
        "host",
        H4,
        110,
        B,
        &[(B, 110)],
        json!({"nombre": "web", "direccion": "192.0.2.4b"}),
    );
    let doc_b = documento(B, &[&b1, &b2, &b4, &b5], &[&b3]);

    let c1 = candidato_vivo(
        "host",
        H5,
        160,
        C,
        &[(B, 100), (C, 160)],
        json!({"nombre": "nas", "direccion": "192.0.2.5"}),
    );
    let c2 = candidato_lapida("host", H_MUERTO, "muerto", 120, C, &[(C, 120)]);
    let c3 = candidato_vivo(
        "tunel",
        T3,
        130,
        C,
        &[(C, 130)],
        json!({"host": H_MUERTO, "nombre": "cola"}),
    );
    let c4 = candidato_vivo(
        "host",
        H6,
        150,
        C,
        &[(C, 150)],
        json!({"nombre": "web", "direccion": "192.0.2.6"}),
    );
    let c5 = candidato_vivo(
        "identidad",
        I1,
        130,
        C,
        &[(C, 130)],
        json!({"alias": "id1", "huella": "SHA256:x", "tipo": "ed25519",
               "comentario": "nueva", "revocada_en": null}),
    );
    let c6 = candidato_vivo(
        "grupo",
        G2,
        120,
        C,
        &[(C, 120)],
        json!({"nombre": "ops", "orden": 1}),
    );
    let doc_c = documento(C, &[&c1, &c3, &c4, &c5, &c6], &[&c2]);

    let d1 = candidato_vivo(
        "host",
        H7,
        155,
        D,
        &[(D, 155)],
        json!({"nombre": "web", "direccion": "192.0.2.7"}),
    );
    let d2 = candidato_lapida("grupo", G1, "prod", 140, D, &[(D, 140)]);
    let doc_d = documento(D, &[&d1], &[&d2]);

    (local, vec![doc_b, doc_c, doc_d])
}

#[test]
fn la_fusion_es_determinista_con_cualquier_orden_y_numero_de_documentos() {
    let (local, documentos) = universo_determinista();
    let base = fusionar(&local, &documentos).expect("fusión de referencia");

    // El universo ejercita todos los casos.
    assert_eq!(base.vivos.len(), 10);
    assert_eq!(base.lapidas.len(), 4);
    assert_eq!(base.conflictos.len(), 10);
    assert_eq!(base.renombrados.len(), 4);
    assert_eq!(base.reloj_ms, 160);
    assert_eq!(conflictos_de_tipo(&base, "elemento").len(), 4);
    assert_eq!(conflictos_de_tipo(&base, "huerfano").len(), 2);
    assert_eq!(conflictos_de_tipo(&base, "nombre").len(), 4);
    assert_eq!(buscar_vivo(&base, H1).nombre, "web");
    assert_eq!(buscar_vivo(&base, H4).nombre, "web (2)");
    assert_eq!(buscar_vivo(&base, H6).nombre, "web (3)");
    assert_eq!(buscar_vivo(&base, H7).nombre, "web (4)");
    assert_eq!(buscar_vivo(&base, T1).nombre, "salto");
    assert_eq!(buscar_vivo(&base, T2).nombre, "salto (2)");
    assert_eq!(buscar_vivo(&base, H2).vector, vector(&[(A, 150), (B, 140)]));
    assert_eq!(
        buscar_vivo(&base, H5).version,
        formatear_version(160, C),
        "la edición posterior resucita al host"
    );
    assert_eq!(
        buscar_lapida(&base, H5).version,
        formatear_version(100, B),
        "la lápida del resucitado se conserva"
    );
    assert_eq!(
        buscar_lapida(&base, G1).vector,
        vector(&[(A, 100), (D, 140)])
    );
    assert!(base
        .conflictos
        .iter()
        .any(|conflicto| { conflicto.tipo == "huerfano" && conflicto.uuid == T3 }));
    assert!(base
        .conflictos
        .iter()
        .any(|conflicto| { conflicto.tipo == "huerfano" && conflicto.uuid == S1 }));

    // Cualquier orden de los documentos y de los elementos internos da lo mismo.
    let mut variantes = 0;
    for permutacion in permutaciones(&documentos) {
        for mascara in 0..8 {
            let mut docs = permutacion.clone();
            for (indice, doc) in docs.iter_mut().enumerate() {
                if mascara & (1 << indice) != 0 {
                    doc.elementos.reverse();
                    doc.lapidas.reverse();
                }
            }
            mismo_plan(&base, &fusionar(&local, &docs).expect("fusión permutada"));
            variantes += 1;
        }
    }
    assert!(variantes >= 24, "se han probado {variantes} variantes");

    // El orden del estado local tampoco importa.
    let mut local_invertido = local.clone();
    local_invertido.reverse();
    mismo_plan(
        &base,
        &fusionar(&local_invertido, &documentos).expect("fusión con local invertido"),
    );

    // Repetir documentos no cambia el resultado (idempotencia).
    let mut repetidos = documentos.clone();
    repetidos.extend(documentos.iter().cloned());
    mismo_plan(
        &base,
        &fusionar(&local, &repetidos).expect("fusión con documentos repetidos"),
    );
}

#[test]
fn los_conflictos_no_se_duplican_al_repetir_un_documento() {
    let local = candidato_vivo(
        "host",
        H1,
        100,
        A,
        &[(A, 100)],
        json!({"nombre": "web", "direccion": "192.0.2.5"}),
    );
    let entrante = candidato_vivo(
        "host",
        H1,
        120,
        B,
        &[(B, 120)],
        json!({"nombre": "web", "direccion": "192.0.2.7"}),
    );
    let doc = documento(B, &[&entrante], &[]);
    let plan = fusionar(std::slice::from_ref(&local), &[doc.clone(), doc]).expect("fusión");
    assert_eq!(plan.conflictos.len(), 1, "no se duplica el conflicto");
    let simple = fusionar(&[local], &[documento(B, &[&entrante], &[])]).expect("fusión");
    mismo_plan(&simple, &plan);
}
