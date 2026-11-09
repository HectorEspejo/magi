//! Prueba de extremo a extremo de la deliberación del móvil (informe §4.6,
//! §5.1 y §7.7; checklist de la Fase 14), sin red externa: el servidor SSH de
//! pruebas compartido sirve el sondeo y el SFTP de BALTHASAR-2, y el núcleo se
//! arranca con su API pública como en la app.
//!
//! Cubre: MELCHIOR-1 con el último sondeo del móvil (NOMINAL aprueba y un host
//! sin respuesta rechaza con el dato), BALTHASAR-2 por SFTP con patrón y
//! antigüedad (reciente aprueba, viejo rechaza con «último backup hace 31 h» y
//! sin acceso rechaza), CASPER-3 «solo en el escritorio» (BLOQUEADA y solo se
//! fuerza con motivo de 10 o más), consenso y tabla por host, «sin
//! comprobaciones configuradas» (activas a 0), resolver (aprobar, forzar y
//! cancelar), la fila de DELIBERACIONES con su usuario «móvil (equipo)»,
//! consumir una sola vez, registrar_resultado con ejecucion_resultado y las
//! anotaciones deliberacion_aprobada/forzada/cancelada de REGISTRO.

mod comun;

use std::time::{Duration, SystemTime};

use magi_movil::api::{
    AccionMovil, AjustesMovil, DecisionDeliberacion, ErrorNucleo, EstadoDeliberacion, Nucleo,
    ObjetivoMovil, PlanDeliberadoMovil, TipoObjetivoMovil, VeredictoMovil,
};
use magi_nucleo::deliberacion::{
    DatosVerificaciones, EjecucionResultado, RegistroDeliberacion, ResultadoDeliberacion, Veredicto,
};
use magi_nucleo::modelo::DatosHost;
use magi_nucleo::snippets::{DatosSnippet, Destino};

use comun::servidor_pruebas::{arrancar, arrancar_con, OpcionesPrueba, PuertosPrueba};
use comun::{
    consulta, contar_registro, crear_host, crear_snippet, guardar_verificaciones, PruebaNucleo,
};

// ---------------------------------------------------------------------------
// Ayudantes
// ---------------------------------------------------------------------------

fn datos_host(nombre: &str, puerto: u16) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: "127.0.0.1".to_string(),
        puerto,
        usuario: Some("usuario".to_string()),
        ..DatosHost::default()
    }
}

/// Un host del inventario con la etiqueta web (destino de un snippet).
fn datos_host_web(nombre: &str, puerto: u16) -> DatosHost {
    DatosHost {
        etiquetas: vec!["web".to_string()],
        ..datos_host(nombre, puerto)
    }
}

/// Plan que delibera una acción de servicio sobre app.service en ese host.
fn plan_servicio(host_uuid: &str) -> PlanDeliberadoMovil {
    PlanDeliberadoMovil::Servicio {
        host: host_uuid.to_string(),
        objetivo: ObjetivoMovil {
            tipo: TipoObjetivoMovil::Unidad,
            nombre: "app.service".to_string(),
        },
        accion: AccionMovil::Reiniciar,
        comprobacion: Some("nginx -t".to_string()),
    }
}

/// Verificaciones con BALTHASAR-2 activo sobre una ruta y un patrón.
fn verificaciones_backup(ruta: &str, patron: Option<&str>) -> DatosVerificaciones {
    DatosVerificaciones {
        backup: true,
        backup_ruta: Some(ruta.to_string()),
        backup_patron: patron.map(str::to_string),
        ..DatosVerificaciones::default()
    }
}

/// Ajustes que no convierten un host sano en CARGA por la carga de la máquina:
/// los umbrales del escritorio, al máximo, y sin motores de contenedores.
fn ajustes_sin_ruido() -> AjustesMovil {
    AjustesMovil {
        umbral_carga_por_nucleo: 16.0,
        umbral_memoria_pct: 100.0,
        umbral_disco_pct: 100.0,
        servicios_contenedores: Vec::new(),
        ..AjustesMovil::por_defecto()
    }
}

/// El dato de un veredicto (para asserts con mensaje).
fn detalle_de(veredicto: &VeredictoMovil) -> &str {
    match veredicto {
        VeredictoMovil::Aprueba { detalle, .. } | VeredictoMovil::Rechaza { detalle, .. } => {
            detalle
        }
        VeredictoMovil::NoActiva | VeredictoMovil::Pendiente => "",
    }
}

/// La fila de DELIBERACIONES, leída por el hilo escritor.
fn leer_deliberacion(nucleo: &Nucleo, id: u64) -> RegistroDeliberacion {
    let id = i64::try_from(id).expect("id de deliberación");
    consulta(nucleo, move |almacen| almacen.obtener_deliberacion(id))
        .expect("hilo escritor")
        .expect("fila de DELIBERACIONES")
}

/// Directorio de backups real y propio de la prueba: db-1.sql.gz de hace 3 h
/// (aprueba con el umbral de 24 h) y db-2.sql.gz de hace 31 h (rechaza si el
/// patrón lo aísla). Hay que mantenerlo vivo mientras dura la prueba.
fn backups_de_prueba() -> tempfile::TempDir {
    let directorio = tempfile::tempdir().expect("directorio de backups de la prueba");
    escribir_backup(&directorio.path().join("db-1.sql.gz"), 3 * 3600);
    escribir_backup(&directorio.path().join("db-2.sql.gz"), 31 * 3600);
    escribir_backup(&directorio.path().join("notas.txt"), 3600);
    directorio
}

fn escribir_backup(ruta: &std::path::Path, edad_segundos: u64) {
    std::fs::write(ruta, b"backup de prueba de MAGI\n").expect("fichero de backup");
    let fichero = std::fs::OpenOptions::new()
        .write(true)
        .open(ruta)
        .expect("abriendo el backup para fecharlo");
    fichero
        .set_modified(SystemTime::now() - Duration::from_secs(edad_segundos))
        .expect("fecha del backup");
}

// ===========================================================================
// MELCHIOR-1
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn melchior_aprueba_el_host_nominal_y_rechaza_el_caido() {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (id_web, uuid_web) = crear_host(&prueba, datos_host("web-01", servidor.puertos.ssh));
    let (id_caido, uuid_caido) = crear_host(&prueba, datos_host("caido-01", 1));
    guardar_verificaciones(
        &prueba,
        id_web,
        DatosVerificaciones {
            salud: true,
            ..Default::default()
        },
    );
    guardar_verificaciones(
        &prueba,
        id_caido,
        DatosVerificaciones {
            salud: true,
            ..Default::default()
        },
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();
    nucleo
        .aplicar_ajustes(ajustes_sin_ruido())
        .expect("ajustes del móvil");

    // Siembra el último sondeo del móvil del host sano: con estos umbrales es
    // NOMINAL y MELCHIOR-1 lo usa sin sondear otra vez.
    let sondeo = magi_movil::flota::sondear_host(prueba.estado(), &uuid_web)
        .await
        .expect("sembrando el sondeo del host sano");
    assert_eq!(
        sondeo.resultado,
        magi_nucleo::modelo::ResultadoSondeo::Ok,
        "{sondeo:?}"
    );

    let tabla = nucleo
        .deliberar(plan_servicio(&uuid_web))
        .await
        .expect("deliberando el host sano");
    assert_eq!(tabla.filas.len(), 1, "{tabla:?}");
    assert_eq!(tabla.filas[0].host_uuid.as_deref(), Some(uuid_web.as_str()));
    assert_eq!(tabla.filas[0].host_nombre, "web-01");
    assert!(
        matches!(tabla.filas[0].salud, VeredictoMovil::Aprueba { .. }),
        "{tabla:?}"
    );
    assert_eq!(tabla.filas[0].backup, VeredictoMovil::NoActiva);
    assert_eq!(tabla.filas[0].tests, VeredictoMovil::NoActiva);
    assert_eq!(
        (
            tabla.aprobadas,
            tabla.activas,
            tabla.rechazos,
            tabla.pendientes
        ),
        (1, 1, 0, 0),
        "{tabla:?}"
    );
    assert!(tabla.unanime, "{tabla:?}");
    assert_eq!(tabla.estado, EstadoDeliberacion::Aprobado);

    // Sin sondeo reciente, MELCHIOR-1 sondea dentro de la comprobación: un host
    // que no responde rechaza con el dato, no se queda en «comprobando».
    let tabla = nucleo
        .deliberar(plan_servicio(&uuid_caido))
        .await
        .expect("deliberando el host caído");
    assert_eq!(tabla.estado, EstadoDeliberacion::Bloqueado, "{tabla:?}");
    assert!(!tabla.unanime, "{tabla:?}");
    assert_eq!((tabla.aprobadas, tabla.activas, tabla.rechazos), (0, 1, 1));
    assert!(
        matches!(tabla.filas[0].salud, VeredictoMovil::Rechaza { .. }),
        "{tabla:?}"
    );
    let detalle = detalle_de(&tabla.filas[0].salud);
    assert!(detalle.starts_with("CAÍDA"), "{detalle}");
    assert!(detalle.contains("no responde"), "{detalle}");
}

// ===========================================================================
// BALTHASAR-2
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn balthasar_aprueba_el_reciente_y_rechaza_el_viejo_o_sin_acceso() {
    let backups = backups_de_prueba();
    let ruta = backups.path().display().to_string();
    let servidor = arrancar_con(OpcionesPrueba {
        directorio_backups: Some(backups.path().to_path_buf()),
        ..OpcionesPrueba::default()
    })
    .await;
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (id_reciente, uuid_reciente) =
        crear_host(&prueba, datos_host("web-01", servidor.puertos.ssh));
    let (id_viejo, uuid_viejo) = crear_host(&prueba, datos_host("web-02", servidor.puertos.ssh));
    let (id_sin_acceso, uuid_sin_acceso) =
        crear_host(&prueba, datos_host("web-03", servidor.puertos.ssh));
    guardar_verificaciones(
        &prueba,
        id_reciente,
        verificaciones_backup(&ruta, Some("*.sql.gz")),
    );
    guardar_verificaciones(
        &prueba,
        id_viejo,
        verificaciones_backup(&ruta, Some("db-2.*")),
    );
    let ruta_rota = format!("{ruta}/no-existe-magi-backups");
    guardar_verificaciones(
        &prueba,
        id_sin_acceso,
        verificaciones_backup(&ruta_rota, None),
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();

    // El fichero más reciente que casa con el patrón es de hace 3 h: aprueba.
    let plan = plan_servicio(&uuid_reciente);
    let tabla = nucleo
        .deliberar(plan.clone())
        .await
        .expect("deliberando el backup reciente");
    assert_eq!(tabla.estado, EstadoDeliberacion::Aprobado, "{tabla:?}");
    assert!(
        matches!(tabla.filas[0].backup, VeredictoMovil::Aprueba { .. }),
        "{tabla:?}"
    );
    assert!(
        detalle_de(&tabla.filas[0].backup).contains("hace 3 h"),
        "{:?}",
        tabla.filas[0].backup
    );

    // Aprobar: la fila de DELIBERACIONES lleva el usuario «móvil (equipo)».
    let id = nucleo
        .resolver_deliberacion(tabla.id, DecisionDeliberacion::Aprobar)
        .expect("aprobando la deliberación");
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(registro.resultado, ResultadoDeliberacion::Aprobada);
    assert!(!registro.bloqueada);
    assert_eq!(registro.motivo, None);
    assert_eq!(registro.usuario, "móvil (pruebas)");
    assert_eq!(registro.accion, "reiniciar app.service → web-01");
    assert_eq!(registro.hosts, vec![(id_reciente, "web-01".to_string())]);
    assert_eq!(registro.comprobaciones.len(), 1);
    assert!(matches!(
        registro.comprobaciones[0].backup,
        Veredicto::Aprueba { .. }
    ));
    assert_eq!(registro.ejecucion_resultado, None);

    // Consumir una deliberación: una sola vez (T62).
    let estado = prueba.estado();
    magi_movil::deliberacion::consumir(estado, id, &plan).expect("consumiendo la deliberación");
    let segunda = magi_movil::deliberacion::consumir(estado, id, &plan);
    assert!(
        matches!(segunda, Err(ErrorNucleo::EstadoInvalido { .. })),
        "{segunda:?}"
    );

    // Registrar el resultado: fija ejecucion_resultado y anota una vez.
    magi_movil::deliberacion::registrar_resultado(estado, id, EjecucionResultado::Ok)
        .expect("registrando el resultado");
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(registro.ejecucion_resultado, Some(EjecucionResultado::Ok));
    assert_eq!(contar_registro(nucleo, "deliberacion_aprobada"), 1);
    // Idempotente: un segundo registro no pisa ni duplica la anotación.
    magi_movil::deliberacion::registrar_resultado(estado, id, EjecucionResultado::Error)
        .expect("segundo registro");
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(registro.ejecucion_resultado, Some(EjecucionResultado::Ok));
    assert_eq!(contar_registro(nucleo, "deliberacion_aprobada"), 1);
    assert_eq!(contar_registro(nucleo, "deliberacion_forzada"), 0);

    // El patrón aísla db-2.sql.gz, de hace 31 h: rechaza con el dato.
    let tabla = nucleo
        .deliberar(plan_servicio(&uuid_viejo))
        .await
        .expect("deliberando el backup viejo");
    assert_eq!(tabla.estado, EstadoDeliberacion::Bloqueado, "{tabla:?}");
    let detalle = detalle_de(&tabla.filas[0].backup);
    assert!(detalle.contains("último backup hace 31 h"), "{detalle}");

    // Un directorio que no existe: rechaza con el motivo, sin pánico.
    let tabla = nucleo
        .deliberar(plan_servicio(&uuid_sin_acceso))
        .await
        .expect("deliberando sin acceso");
    assert_eq!(tabla.estado, EstadoDeliberacion::Bloqueado, "{tabla:?}");
    let detalle = detalle_de(&tabla.filas[0].backup);
    assert!(detalle.starts_with("sin acceso SFTP: "), "{detalle}");
}

// ===========================================================================
// CASPER-3: «solo en el escritorio» → BLOQUEADA y forzado
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn casper_bloquea_y_solo_se_fuerza_con_motivo_largo() {
    // CASPER-3 no abre conexiones: el host no necesita servidor.
    let mut prueba = PruebaNucleo::nueva(&[]);
    let (id_web, uuid_web) = crear_host(&prueba, datos_host("web-01", 1));
    guardar_verificaciones(
        &prueba,
        id_web,
        DatosVerificaciones {
            tests: true,
            tests_comando: Some("true".to_string()),
            ..Default::default()
        },
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();
    let plan = plan_servicio(&uuid_web);

    let tabla = nucleo
        .deliberar(plan.clone())
        .await
        .expect("deliberando con CASPER-3 activo");
    assert_eq!(tabla.estado, EstadoDeliberacion::Bloqueado, "{tabla:?}");
    assert!(!tabla.unanime, "{tabla:?}");
    assert_eq!((tabla.aprobadas, tabla.activas, tabla.rechazos), (0, 1, 1));
    assert_eq!(tabla.filas[0].salud, VeredictoMovil::NoActiva);
    assert_eq!(tabla.filas[0].backup, VeredictoMovil::NoActiva);
    assert!(
        matches!(tabla.filas[0].tests, VeredictoMovil::Rechaza { .. }),
        "{tabla:?}"
    );
    assert!(
        detalle_de(&tabla.filas[0].tests).contains("solo en el escritorio"),
        "{:?}",
        tabla.filas[0].tests
    );

    // BLOQUEADA no se aprueba, y el forzado exige un motivo de 10 o más.
    let aprobar = nucleo.resolver_deliberacion(tabla.id, DecisionDeliberacion::Aprobar);
    assert!(
        matches!(aprobar, Err(ErrorNucleo::EstadoInvalido { .. })),
        "{aprobar:?}"
    );
    let corto = nucleo.resolver_deliberacion(
        tabla.id,
        DecisionDeliberacion::Forzar {
            motivo: "   ok   ".to_string(),
        },
    );
    match corto {
        Err(ErrorNucleo::EstadoInvalido { detalle }) => {
            assert!(detalle.contains("10"), "{detalle}");
        }
        otro => panic!("un motivo corto no puede forzar: {otro:?}"),
    }

    // Con motivo suficiente: FORZADA, con el motivo recortado y el usuario.
    let id = nucleo
        .resolver_deliberacion(
            tabla.id,
            DecisionDeliberacion::Forzar {
                motivo: "  tests en verde en CI esta mañana  ".to_string(),
            },
        )
        .expect("forzando con motivo");
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(registro.resultado, ResultadoDeliberacion::Forzada);
    assert!(registro.bloqueada);
    assert_eq!(
        registro.motivo.as_deref(),
        Some("tests en verde en CI esta mañana")
    );
    assert_eq!(registro.usuario, "móvil (pruebas)");
    assert_eq!(registro.accion, "reiniciar app.service → web-01");
    assert!(matches!(
        registro.comprobaciones[0].tests,
        Veredicto::Rechaza { .. }
    ));
    // Una deliberación solo se resuelve una vez.
    let segunda = nucleo.resolver_deliberacion(tabla.id, DecisionDeliberacion::Cancelar);
    assert!(segunda.is_err(), "{segunda:?}");
    // La anotación de forzada llega al registrar el resultado, no al resolver.
    assert_eq!(contar_registro(nucleo, "deliberacion_forzada"), 0);

    let estado = prueba.estado();
    magi_movil::deliberacion::consumir(estado, id, &plan).expect("consumiendo la forzada");
    magi_movil::deliberacion::registrar_resultado(estado, id, EjecucionResultado::Parcial)
        .expect("registrando el resultado de la forzada");
    assert_eq!(contar_registro(nucleo, "deliberacion_forzada"), 1);
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(
        registro.ejecucion_resultado,
        Some(EjecucionResultado::Parcial)
    );
}

// ===========================================================================
// Sin comprobaciones activas y cancelar
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sin_comprobaciones_activas_aprueba_y_cancelar_anota() {
    let mut prueba = PruebaNucleo::nueva(&[]);
    let (_id_web, uuid_web) = crear_host(&prueba, datos_host("web-01", 1));
    prueba.arrancar();
    let nucleo = prueba.nucleo();
    let plan = plan_servicio(&uuid_web);

    // Sin verificaciones activas: activas 0, APROBADO y se puede cancelar.
    let tabla = nucleo
        .deliberar(plan.clone())
        .await
        .expect("deliberando sin comprobaciones");
    assert_eq!(tabla.filas.len(), 1, "{tabla:?}");
    assert_eq!(tabla.filas[0].salud, VeredictoMovil::NoActiva);
    assert_eq!(tabla.filas[0].backup, VeredictoMovil::NoActiva);
    assert_eq!(tabla.filas[0].tests, VeredictoMovil::NoActiva);
    assert_eq!(
        (
            tabla.aprobadas,
            tabla.activas,
            tabla.rechazos,
            tabla.pendientes
        ),
        (0, 0, 0, 0),
        "{tabla:?}"
    );
    assert!(tabla.unanime, "{tabla:?}");
    assert_eq!(tabla.estado, EstadoDeliberacion::Aprobado);

    let id = nucleo
        .resolver_deliberacion(tabla.id, DecisionDeliberacion::Cancelar)
        .expect("cancelando la deliberación");
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(registro.resultado, ResultadoDeliberacion::Cancelada);
    assert!(!registro.bloqueada);
    assert_eq!(registro.motivo, None);
    assert_eq!(registro.usuario, "móvil (pruebas)");
    assert_eq!(registro.ejecucion_resultado, None);
    assert_eq!(contar_registro(nucleo, "deliberacion_cancelada"), 1);

    // Una cancelada no autoriza nada y registrar su resultado no toca la fila.
    let consumir = magi_movil::deliberacion::consumir(prueba.estado(), id, &plan);
    assert!(
        matches!(consumir, Err(ErrorNucleo::EstadoInvalido { .. })),
        "{consumir:?}"
    );
    magi_movil::deliberacion::registrar_resultado(prueba.estado(), id, EjecucionResultado::Ok)
        .expect("registrando una cancelada");
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(registro.ejecucion_resultado, None, "{registro:?}");
    assert_eq!(contar_registro(nucleo, "deliberacion_aprobada"), 0);
    assert_eq!(contar_registro(nucleo, "deliberacion_forzada"), 0);
}

// ===========================================================================
// Tabla de un snippet en varios hosts
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_tabla_de_un_snippet_lleva_una_fila_por_host() {
    let backups = backups_de_prueba();
    let ruta = backups.path().display().to_string();
    let servidor = arrancar_con(OpcionesPrueba {
        directorio_backups: Some(backups.path().to_path_buf()),
        ..OpcionesPrueba::default()
    })
    .await;
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (id_01, uuid_01) = crear_host(&prueba, datos_host_web("web-01", servidor.puertos.ssh));
    let (id_02, uuid_02) = crear_host(&prueba, datos_host_web("web-02", servidor.puertos.ssh));
    // Solo web-01 tiene una comprobación activa: la tabla lleva una fila por
    // host y las celdas del otro host salen «—».
    guardar_verificaciones(
        &prueba,
        id_01,
        verificaciones_backup(&ruta, Some("*.sql.gz")),
    );
    let uuid_snippet = crear_snippet(
        &prueba,
        DatosSnippet {
            nombre: "reiniciar nginx".to_string(),
            comando: "true".to_string(),
            destinos: vec![Destino::Etiqueta("web".to_string())],
            critico: false,
            ..DatosSnippet::default()
        },
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();

    let preparacion = nucleo
        .preparar_ejecucion(
            uuid_snippet,
            vec![uuid_01.clone(), uuid_02.clone()],
            Vec::new(),
        )
        .expect("preparando la ejecución");
    assert!(preparacion.errores.is_empty(), "{preparacion:?}");
    assert!(preparacion.delibera, "{preparacion:?}");
    let plan = PlanDeliberadoMovil::Snippet {
        preparacion: preparacion.clone(),
    };

    let tabla = nucleo
        .deliberar(plan.clone())
        .await
        .expect("deliberando el snippet");
    assert_eq!(tabla.filas.len(), 2, "{tabla:?}");
    let fila_01 = tabla
        .filas
        .iter()
        .find(|fila| fila.host_uuid.as_deref() == Some(uuid_01.as_str()))
        .expect("web-01 en la tabla");
    assert!(
        matches!(fila_01.backup, VeredictoMovil::Aprueba { .. }),
        "{fila_01:?}"
    );
    let fila_02 = tabla
        .filas
        .iter()
        .find(|fila| fila.host_uuid.as_deref() == Some(uuid_02.as_str()))
        .expect("web-02 en la tabla");
    assert_eq!(fila_02.salud, VeredictoMovil::NoActiva);
    assert_eq!(fila_02.backup, VeredictoMovil::NoActiva);
    assert_eq!(fila_02.tests, VeredictoMovil::NoActiva);
    assert_eq!(
        (tabla.aprobadas, tabla.activas, tabla.pendientes),
        (1, 1, 0),
        "{tabla:?}"
    );
    assert!(tabla.unanime, "{tabla:?}");
    assert_eq!(tabla.estado, EstadoDeliberacion::Aprobado);

    let id = nucleo
        .resolver_deliberacion(tabla.id, DecisionDeliberacion::Aprobar)
        .expect("aprobando el snippet");
    let registro = leer_deliberacion(nucleo, id);
    assert_eq!(registro.resultado, ResultadoDeliberacion::Aprobada);
    assert_eq!(registro.accion, "reiniciar nginx → web-01, web-02");
    assert_eq!(
        registro.hosts,
        vec![(id_01, "web-01".to_string()), (id_02, "web-02".to_string())]
    );
    assert_eq!(registro.comprobaciones.len(), 2);

    let estado = prueba.estado();
    magi_movil::deliberacion::consumir(estado, id, &plan).expect("consumiendo el snippet");
    magi_movil::deliberacion::registrar_resultado(estado, id, EjecucionResultado::Ok)
        .expect("registrando el resultado del snippet");
    assert_eq!(contar_registro(nucleo, "deliberacion_aprobada"), 1);
    assert_eq!(
        leer_deliberacion(nucleo, id).ejecucion_resultado,
        Some(EjecucionResultado::Ok)
    );
}
