//! Pruebas de las ejecuciones de snippets en el servidor de sesiones (Fase 6)
//! contra el servidor SSH en proceso de `comun`, que atiende `exec` con
//! `sh -c` (con `MAGI_USUARIO` = el usuario de cada host, para que cada uno
//! pueda portarse distinto).

mod comun;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use magi::almacen::{deliberaciones, Almacen};
use magi::deliberacion::{ComprobacionesHost, NuevaDeliberacion, ResultadoDeliberacion, Veredicto};
use magi::modelo::IdentidadRef;
use magi::protocolo::{
    ComandoInicial, DeliberacionLanzada, Direccion, ElementoTransferencia, EstadoEjecucion,
    EstadoHostEjecucion, EtiquetaTransferencia, InfoEjecucion, MensajeCliente, MensajeServidor,
    Politica, Secreto, SincronizacionLanzada,
};

use comun::*;

/// Parámetros de un lanzamiento de prueba.
struct Lanzamiento<'a> {
    hosts: &'a [i64],
    comando: &'a str,
    timeout_seg: u32,
    parar_al_fallo: bool,
    deliberacion: Option<DeliberacionLanzada>,
}

impl<'a> Lanzamiento<'a> {
    fn nuevo(hosts: &'a [i64], comando: &'a str) -> Self {
        Self {
            hosts,
            comando,
            timeout_seg: 30,
            parar_al_fallo: false,
            deliberacion: None,
        }
    }
}

/// Lanza y espera la respuesta: `Ok(())` si fue `Hecho`, el error si no.
async fn lanzar(
    escenario: &mut Escenario,
    peticion_id: u64,
    lanzamiento: Lanzamiento<'_>,
) -> Result<(), String> {
    escenario
        .enviar(&MensajeCliente::LanzarEjecucion {
            peticion_id,
            snippet_id: None,
            nombre: "prueba".to_string(),
            comando: lanzamiento.comando.to_string(),
            host_ids: lanzamiento.hosts.to_vec(),
            timeout_seg: lanzamiento.timeout_seg,
            parar_al_fallo: lanzamiento.parar_al_fallo,
            deliberacion: lanzamiento.deliberacion,
        })
        .await;
    match escenario.esperar_respuesta(peticion_id).await {
        None => Ok(()),
        Some(error) => Err(error),
    }
}

/// Espera a que la ejecución lanzada con ese `peticion_id` cumpla la
/// condición en alguna difusión y la devuelve.
async fn esperar_ejecucion<F>(
    escenario: &mut Escenario,
    peticion_id: u64,
    mut vale: F,
) -> InfoEjecucion
where
    F: FnMut(&InfoEjecucion) -> bool,
{
    loop {
        if let MensajeServidor::Ejecuciones { lista } = escenario.siguiente().await {
            if let Some(ejecucion) = lista.into_iter().find(|e| e.peticion_id == peticion_id) {
                if vale(&ejecucion) {
                    return ejecucion;
                }
            }
        }
    }
}

async fn esperar_terminada(escenario: &mut Escenario, peticion_id: u64) -> InfoEjecucion {
    esperar_ejecucion(escenario, peticion_id, |e| {
        e.estado != EstadoEjecucion::EnCurso
    })
    .await
}

/// Pide la salida de un host y la espera.
async fn salida(
    escenario: &mut Escenario,
    ejecucion_id: u32,
    host_id: i64,
) -> (String, String, bool) {
    escenario
        .enviar(&MensajeCliente::PedirSalida {
            ejecucion_id,
            host_id,
        })
        .await;
    match escenario
        .esperar(|mensaje| {
            matches!(mensaje, MensajeServidor::Salida { host_id: id, .. } if *id == host_id)
        })
        .await
    {
        MensajeServidor::Salida {
            stdout,
            stderr,
            truncada,
            ..
        } => (
            String::from_utf8_lossy(&stdout.0).to_string(),
            String::from_utf8_lossy(&stderr.0).to_string(),
            truncada,
        ),
        _ => unreachable!(),
    }
}

fn estados(ejecucion: &InfoEjecucion) -> Vec<EstadoHostEjecucion> {
    ejecucion.hosts.iter().map(|host| host.estado).collect()
}

#[tokio::test]
async fn una_ejecucion_en_varios_hosts_captura_la_salida_de_cada_uno() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 3,
        ..Default::default()
    })
    .await;
    let hosts = escenario.hosts.clone();
    lanzar(
        &mut escenario,
        1,
        Lanzamiento::nuevo(&hosts, "echo \"hola $MAGI_USUARIO\"; echo aviso >&2"),
    )
    .await
    .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 1).await;
    assert_eq!(ejecucion.estado, EstadoEjecucion::Terminada);
    assert!(ejecucion
        .hosts
        .iter()
        .all(|host| host.estado == EstadoHostEjecucion::Ok && host.codigo == Some(0)));
    for (numero, host_id) in hosts.iter().enumerate() {
        let (stdout, stderr, truncada) = salida(&mut escenario, ejecucion.id, *host_id).await;
        assert_eq!(stdout, format!("hola usuario{}\n", numero + 1));
        assert_eq!(stderr, "aviso\n");
        assert!(!truncada);
    }
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |e| e
                .anotaciones("snippet_ejecutado")
                == 3)
            .await
    );
    // La salida nunca va al registro.
    assert!(escenario
        .registro()
        .iter()
        .all(|(_, detalle)| !detalle.contains("hola") && !detalle.contains("aviso")));
}

#[tokio::test]
async fn un_codigo_distinto_de_cero_deja_el_host_en_fallo() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 2, Lanzamiento::nuevo(&hosts, "exit 3"))
        .await
        .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 2).await;
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Fallo);
    assert_eq!(ejecucion.hosts[0].codigo, Some(3));
}

/// AC (checklist l. 59): `sleep 10` con timeout de 5 s → error «tiempo
/// agotado» a los 5 s y la conexión del pool sigue viva.
#[tokio::test]
async fn el_timeout_deja_el_host_en_error_y_la_conexion_del_pool_viva() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    let inicio = Instant::now();
    lanzar(
        &mut escenario,
        3,
        Lanzamiento {
            timeout_seg: 5,
            ..Lanzamiento::nuevo(&hosts, "sleep 10")
        },
    )
    .await
    .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 3).await;
    let tardado = inicio.elapsed();
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Error);
    assert_eq!(ejecucion.hosts[0].error.as_deref(), Some("tiempo agotado"));
    assert!(
        tardado >= Duration::from_secs(5) && tardado < Duration::from_secs(8),
        "tardó {tardado:?}"
    );
    // La misma conexión sirve a la siguiente ejecución.
    lanzar(&mut escenario, 4, Lanzamiento::nuevo(&hosts, "echo sigue"))
        .await
        .unwrap();
    let segunda = esperar_terminada(&mut escenario, 4).await;
    assert_eq!(segunda.hosts[0].estado, EstadoHostEjecucion::Ok);
    assert_eq!(escenario.observado.conexiones(), 1);
}

/// AC (checklist l. 62): 10 hosts con «parar al primer fallo» y el segundo
/// devuelve 1 → los que no habían empezado quedan omitidos.
#[tokio::test]
async fn parar_al_primer_fallo_omite_los_que_no_han_empezado() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 10,
        ..Default::default()
    })
    .await;
    let hosts = escenario.hosts.clone();
    lanzar(
        &mut escenario,
        5,
        Lanzamiento {
            parar_al_fallo: true,
            ..Lanzamiento::nuevo(
                &hosts,
                "if [ \"$MAGI_USUARIO\" = usuario2 ]; then exit 1; fi; sleep 1",
            )
        },
    )
    .await
    .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 5).await;
    let estados = estados(&ejecucion);
    assert_eq!(estados[1], EstadoHostEjecucion::Fallo);
    assert_eq!(
        &estados[8..],
        &[EstadoHostEjecucion::Omitido; 2],
        "{estados:?}"
    );
    // Los que ya estaban en marcha terminan.
    assert!(estados[..8]
        .iter()
        .enumerate()
        .all(|(indice, estado)| indice == 1 || *estado == EstadoHostEjecucion::Ok));
}

#[tokio::test]
async fn cancelar_en_curso_cierra_los_canales_y_omite_la_cola() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 10,
        ..Default::default()
    })
    .await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 6, Lanzamiento::nuevo(&hosts, "sleep 20"))
        .await
        .unwrap();
    let en_marcha = esperar_ejecucion(&mut escenario, 6, |e| {
        e.hosts
            .iter()
            .filter(|host| host.estado == EstadoHostEjecucion::Ejecutando)
            .count()
            == 8
    })
    .await;
    let inicio = Instant::now();
    escenario
        .enviar(&MensajeCliente::CancelarEjecucion { id: en_marcha.id })
        .await;
    let ejecucion = esperar_terminada(&mut escenario, 6).await;
    assert!(inicio.elapsed() < Duration::from_secs(5));
    assert_eq!(ejecucion.estado, EstadoEjecucion::Cancelada);
    let estados = estados(&ejecucion);
    assert_eq!(&estados[..8], &[EstadoHostEjecucion::Cancelado; 8]);
    assert_eq!(&estados[8..], &[EstadoHostEjecucion::Omitido; 2]);
    // Los `sleep` remotos mueren con su canal.
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |e| {
                e.observado
                    .exec_activos
                    .load(std::sync::atomic::Ordering::SeqCst)
                    == 0
            })
            .await
    );
}

#[tokio::test]
async fn la_salida_se_corta_en_un_mib_y_se_marca_truncada() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    lanzar(
        &mut escenario,
        7,
        Lanzamiento::nuevo(&hosts, "head -c 1100000 /dev/zero | tr '\\0' a"),
    )
    .await
    .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 7).await;
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Ok);
    assert_eq!(ejecucion.hosts[0].bytes_stdout, 1_100_000);
    assert!(ejecucion.hosts[0].truncada);
    let (stdout, _, truncada) = salida(&mut escenario, ejecucion.id, hosts[0]).await;
    assert_eq!(stdout.len(), 1024 * 1024);
    assert!(truncada);
}

#[tokio::test]
async fn ocho_hosts_como_maximo_a_la_vez() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 12,
        ..Default::default()
    })
    .await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 8, Lanzamiento::nuevo(&hosts, "sleep 1"))
        .await
        .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 8).await;
    assert!(ejecucion
        .hosts
        .iter()
        .all(|host| host.estado == EstadoHostEjecucion::Ok));
    assert_eq!(
        escenario
            .observado
            .exec_max
            .load(std::sync::atomic::Ordering::SeqCst),
        8
    );
}

/// Inserta una deliberación como la dejaría el cliente al resolverla.
fn deliberacion(escenario: &Escenario, host_id: i64, forzada: bool) -> i64 {
    let almacen = Almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let id = deliberaciones::crear(
        almacen.conexion(),
        &NuevaDeliberacion {
            snippet_id: None,
            accion: "prueba → prueba-1".to_string(),
            hosts: vec![(host_id, "prueba-1".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id,
                host: "prueba-1".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 10,
                },
                backup: if forzada {
                    Veredicto::Rechaza {
                        detalle: "último backup hace 31 h".to_string(),
                        ms: 10,
                    }
                } else {
                    Veredicto::NoActiva
                },
                tests: Veredicto::NoActiva,
            }],
            resultado: if forzada {
                ResultadoDeliberacion::Forzada
            } else {
                ResultadoDeliberacion::Aprobada
            },
            bloqueada: forzada,
            motivo: forzada.then(|| "backup revisado a mano".to_string()),
            usuario: "hector".to_string(),
        },
    )
    .unwrap();
    almacen.cerrar().unwrap();
    id
}

fn resultado_deliberacion(escenario: &Escenario, id: i64) -> Option<String> {
    let almacen = Almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let resultado = almacen
        .conexion()
        .query_row(
            "SELECT ejecucion_resultado FROM DELIBERACIONES WHERE id = ?1",
            [id],
            |fila| fila.get(0),
        )
        .unwrap();
    almacen.cerrar().unwrap();
    resultado
}

#[tokio::test]
async fn una_ejecucion_deliberada_anota_la_deliberacion_y_rellena_el_resultado() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    for (peticion, forzada, tipo) in [
        (10u64, false, "deliberacion_aprobada"),
        (11, true, "deliberacion_forzada"),
    ] {
        let id = deliberacion(&escenario, hosts[0], forzada);
        lanzar(
            &mut escenario,
            peticion,
            Lanzamiento {
                deliberacion: Some(DeliberacionLanzada {
                    id,
                    forzada,
                    motivo: forzada.then(|| "backup revisado a mano".to_string()),
                }),
                ..Lanzamiento::nuevo(&hosts, "true")
            },
        )
        .await
        .unwrap();
        let ejecucion = esperar_terminada(&mut escenario, peticion).await;
        assert_eq!(ejecucion.deliberacion_id, Some(id));
        assert_eq!(ejecucion.forzada, forzada);
        assert!(
            escenario
                .hasta(Duration::from_secs(5), |e| resultado_deliberacion(e, id)
                    .is_some())
                .await
        );
        assert_eq!(
            resultado_deliberacion(&escenario, id).as_deref(),
            Some("ok")
        );
        let anotada = escenario.registro().into_iter().find(|(fila, detalle)| {
            fila == tipo && detalle.starts_with(&format!("deliberación #{id} · "))
        });
        let (_, detalle) = anotada.expect("sin anotación de la deliberación");
        assert!(detalle.contains("resultado ok"), "{detalle}");
        if forzada {
            assert!(detalle.contains("backup revisado a mano"), "{detalle}");
        }
    }
}

#[tokio::test]
async fn una_deliberacion_inexistente_incoherente_o_ya_usada_se_rechaza() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    let inexistente = lanzar(
        &mut escenario,
        20,
        Lanzamiento {
            deliberacion: Some(DeliberacionLanzada {
                id: 999,
                forzada: false,
                motivo: None,
            }),
            ..Lanzamiento::nuevo(&hosts, "true")
        },
    )
    .await;
    assert!(inexistente.is_err());
    let aprobada = deliberacion(&escenario, hosts[0], false);
    // Una aprobada no se puede lanzar como forzada.
    let incoherente = lanzar(
        &mut escenario,
        21,
        Lanzamiento {
            deliberacion: Some(DeliberacionLanzada {
                id: aprobada,
                forzada: true,
                motivo: Some("lo que sea largo".to_string()),
            }),
            ..Lanzamiento::nuevo(&hosts, "true")
        },
    )
    .await;
    assert!(incoherente.is_err());
    let buena = DeliberacionLanzada {
        id: aprobada,
        forzada: false,
        motivo: None,
    };
    lanzar(
        &mut escenario,
        22,
        Lanzamiento {
            deliberacion: Some(buena.clone()),
            ..Lanzamiento::nuevo(&hosts, "sleep 1")
        },
    )
    .await
    .unwrap();
    // Mientras corre, la misma deliberación no autoriza otra.
    let repetida = lanzar(
        &mut escenario,
        23,
        Lanzamiento {
            deliberacion: Some(buena.clone()),
            ..Lanzamiento::nuevo(&hosts, "true")
        },
    )
    .await;
    assert!(repetida.is_err());
    esperar_terminada(&mut escenario, 22).await;
}

/// Fase 8: una deliberación vale para una sola cosa. La que autorizó la
/// transferencia de una sincronización no autoriza después una ejecución, y
/// es la transferencia la que la cierra.
#[tokio::test]
async fn una_deliberacion_usada_por_una_transferencia_no_vale_para_una_ejecucion() {
    if !hay_sftp_server() {
        return;
    }
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    let id = deliberacion(&escenario, hosts[0], false);
    let local = tempfile::tempdir().unwrap();
    let remoto = tempfile::tempdir().unwrap();
    let origen = local.path().join("a.txt");
    std::fs::write(&origen, b"a").unwrap();
    let lanzada = DeliberacionLanzada {
        id,
        forzada: false,
        motivo: None,
    };
    escenario
        .enviar(&MensajeCliente::Transferir {
            host_id: hosts[0],
            direccion: Direccion::Subida,
            elementos: vec![ElementoTransferencia {
                origen: origen.display().to_string(),
                destino: remoto.path().join("a.txt").display().to_string(),
                bytes: 1,
                es_directorio: false,
                politica: None,
                permisos: Some(0o644),
            }],
            politica: Politica::Sobrescribir,
            borrar_origen: false,
            peticion_id: Some(100),
            borrar_al_terminar: Vec::new(),
            deliberacion: Some(lanzada.clone()),
            sincronizacion: Some(SincronizacionLanzada {
                id: None,
                nombre: None,
                creados: 1,
                actualizados: 0,
                omitidos: 0,
                raiz_origen: local.path().display().to_string(),
                raiz_destino: remoto.path().display().to_string(),
            }),
            etiqueta: Some(EtiquetaTransferencia::Sincronizacion),
        })
        .await;
    assert_eq!(escenario.esperar_respuesta(100).await, None, "se encola");
    let reutilizada = lanzar(
        &mut escenario,
        101,
        Lanzamiento {
            deliberacion: Some(lanzada),
            ..Lanzamiento::nuevo(&hosts, "true")
        },
    )
    .await;
    assert!(reutilizada.is_err(), "{reutilizada:?}");
    assert!(
        escenario
            .hasta(Duration::from_secs(10), |e| resultado_deliberacion(e, id)
                .is_some())
            .await
    );
    assert_eq!(
        resultado_deliberacion(&escenario, id).as_deref(),
        Some("ok")
    );
    assert_eq!(escenario.anotaciones("snippet_ejecutado"), 0);
}

#[tokio::test]
async fn una_huella_desconocida_da_error_con_instruccion_sin_dialogo() {
    let mut escenario = escenario(OpcionesEscenario {
        huella_conocida: false,
        ..Default::default()
    })
    .await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 30, Lanzamiento::nuevo(&hosts, "true"))
        .await
        .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 30).await;
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Error);
    let error = ejecucion.hosts[0].error.clone().unwrap_or_default();
    assert!(error.contains("conéctate una vez"), "{error}");
    assert!(!escenario
        .vistos
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeServidor::HuellaDesconocida { .. })));
}

/// La contraseña de una ejecución solo sale del llavero del solicitante: el
/// servidor pide `PideLlavero` (nunca un diálogo) y, si no la hay, el host
/// falla enseguida con instrucción.
#[tokio::test]
async fn la_contrasena_de_una_ejecucion_solo_sale_del_llavero() {
    let mut escenario = escenario(OpcionesEscenario {
        identidad: Some(IdentidadRef::ContrasenaLlavero),
        ..Default::default()
    })
    .await;
    let hosts = escenario.hosts.clone();
    lanzar(
        &mut escenario,
        40,
        Lanzamiento::nuevo(&hosts, "echo dentro"),
    )
    .await
    .unwrap();
    let (sesion_id, usuario) = match escenario
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::PideLlavero { .. }))
        .await
    {
        MensajeServidor::PideLlavero {
            sesion_id, usuario, ..
        } => (sesion_id, usuario),
        _ => unreachable!(),
    };
    assert_eq!(usuario, "usuario1");
    escenario
        .enviar(&MensajeCliente::Contrasena {
            sesion_id,
            contrasena: Secreto::nuevo("secreta"),
            recordar: false,
        })
        .await;
    let ejecucion = esperar_terminada(&mut escenario, 40).await;
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Ok);

    // Sin contraseña en el llavero: `Cerrar` y el host falla sin esperar.
    let mut otro = escenario_con_llavero().await;
    let hosts = otro.hosts.clone();
    lanzar(&mut otro, 41, Lanzamiento::nuevo(&hosts, "true"))
        .await
        .unwrap();
    let sesion_id = match otro
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::PideLlavero { .. }))
        .await
    {
        MensajeServidor::PideLlavero { sesion_id, .. } => sesion_id,
        _ => unreachable!(),
    };
    let inicio = Instant::now();
    otro.enviar(&MensajeCliente::Cerrar { sesion_id }).await;
    let ejecucion = esperar_terminada(&mut otro, 41).await;
    assert!(inicio.elapsed() < Duration::from_secs(5));
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Error);
    let error = ejecucion.hosts[0].error.clone().unwrap_or_default();
    assert!(error.contains("llavero"), "{error}");
    assert!(!otro
        .vistos
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeServidor::PideContrasena { .. })));
}

async fn escenario_con_llavero() -> Escenario {
    escenario(OpcionesEscenario {
        identidad: Some(IdentidadRef::ContrasenaLlavero),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn un_host_con_contrasena_sin_llavero_falla_sin_preguntar() {
    let mut escenario = escenario(OpcionesEscenario {
        identidad: Some(IdentidadRef::Contrasena),
        ..Default::default()
    })
    .await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 50, Lanzamiento::nuevo(&hosts, "true"))
        .await
        .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 50).await;
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Error);
    assert!(!escenario.vistos.iter().any(|mensaje| matches!(
        mensaje,
        MensajeServidor::PideContrasena { .. } | MensajeServidor::PideLlavero { .. }
    )));
}

#[tokio::test]
async fn una_ventana_nueva_ve_las_ejecuciones_en_la_bienvenida() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 60, Lanzamiento::nuevo(&hosts, "true"))
        .await
        .unwrap();
    esperar_terminada(&mut escenario, 60).await;
    let ruta = magi::servidor::ruta_socket(escenario.rutas());
    let mut otra = cliente(&ruta, magi::protocolo::VERSION_PROTOCOLO).await;
    match siguiente::<MensajeServidor>(&mut otra).await {
        MensajeServidor::Bienvenida { ejecuciones, .. } => {
            assert!(ejecuciones.iter().any(|e| e.peticion_id == 60));
        }
        otro => panic!("{otro:?}"),
    }
}

#[tokio::test]
async fn limpiar_quita_las_terminadas_y_no_las_en_curso() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 70, Lanzamiento::nuevo(&hosts, "true"))
        .await
        .unwrap();
    esperar_terminada(&mut escenario, 70).await;
    lanzar(&mut escenario, 71, Lanzamiento::nuevo(&hosts, "sleep 2"))
        .await
        .unwrap();
    escenario.enviar(&MensajeCliente::LimpiarEjecuciones).await;
    let lista = loop {
        if let MensajeServidor::Ejecuciones { lista } = escenario.siguiente().await {
            if !lista.iter().any(|e| e.peticion_id == 70) {
                break lista;
            }
        }
    };
    assert!(lista.iter().any(|e| e.peticion_id == 71));
}

#[tokio::test]
async fn pedir_salida_solo_responde_al_que_la_pide() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    lanzar(
        &mut escenario,
        80,
        Lanzamiento::nuevo(&hosts, "echo privado"),
    )
    .await
    .unwrap();
    let ejecucion = esperar_terminada(&mut escenario, 80).await;
    let ruta = magi::servidor::ruta_socket(escenario.rutas());
    let mut otra = cliente(&ruta, magi::protocolo::VERSION_PROTOCOLO).await;
    let _: MensajeServidor = siguiente(&mut otra).await;
    let (stdout, _, _) = salida(&mut escenario, ejecucion.id, hosts[0]).await;
    assert_eq!(stdout, "privado\n");
    // La otra ventana no recibe nada con la salida.
    let llegado = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            let mensaje: MensajeServidor = siguiente(&mut otra).await;
            if matches!(mensaje, MensajeServidor::Salida { .. }) {
                return mensaje;
            }
        }
    })
    .await;
    assert!(llegado.is_err(), "la otra ventana recibió la salida");
}

/// Un host borrado durante la ejecución: su fila sigue hasta terminar y
/// `snippet_ejecutado` queda con host nulo y el nombre en el detalle.
#[tokio::test]
async fn un_host_borrado_durante_la_ejecucion_anota_con_host_nulo() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let hosts = escenario.hosts.clone();
    lanzar(&mut escenario, 90, Lanzamiento::nuevo(&hosts, "sleep 1"))
        .await
        .unwrap();
    esperar_ejecucion(&mut escenario, 90, |e| {
        e.hosts[0].estado == EstadoHostEjecucion::Ejecutando
    })
    .await;
    {
        let almacen = Almacen::abrir(&escenario.rutas().base_datos()).unwrap();
        almacen.borrar_host(hosts[0]).unwrap();
        almacen.cerrar().unwrap();
    }
    let ejecucion = esperar_terminada(&mut escenario, 90).await;
    assert_eq!(ejecucion.hosts[0].estado, EstadoHostEjecucion::Ok);
    let fila = escenario.hasta(Duration::from_secs(5), |e| {
        e.anotaciones("snippet_ejecutado") == 1
    });
    assert!(fila.await);
    let almacen = Almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let (host_id, detalle): (Option<i64>, String) = almacen
        .conexion()
        .query_row(
            "SELECT host_id, detalle FROM REGISTRO WHERE tipo = 'snippet_ejecutado'",
            [],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .unwrap();
    almacen.cerrar().unwrap();
    assert_eq!(host_id, None);
    assert!(detalle.contains("prueba-1"), "{detalle}");
}

/// `comando_inicial`: se escribe tras abrir la shell; al reconectar solo el
/// que repite (el snippet al conectar), no el de «abrir en pestaña».
#[tokio::test]
async fn el_comando_inicial_se_escribe_y_solo_el_que_repite_al_reconectar() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    let conocidas: HashSet<u32> = HashSet::new();
    escenario
        .enviar(&MensajeCliente::AbrirSesion {
            host_id: host,
            cols: 80,
            filas: 24,
            comandos_iniciales: vec![
                ComandoInicial {
                    texto: "tmux attach\n".to_string(),
                    repetir: true,
                },
                ComandoInicial {
                    texto: "systemctl restart nginx\n".to_string(),
                    repetir: false,
                },
            ],
        })
        .await;
    let sesion_id = escenario.esperar_sesion_nueva(&conocidas).await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |e| {
                e.observado.shell() == "tmux attach\nsystemctl restart nginx\n"
            })
            .await,
        "shell: {:?}",
        escenario.observado.shell()
    );
    escenario.observado.tirar_conexiones().await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |e| {
                e.sesiones.iter().any(|s| {
                    s.id == sesion_id && s.estado == magi::protocolo::EstadoSesionRemota::Caida
                })
            })
            .await
    );
    escenario
        .enviar(&MensajeCliente::Reconectar { sesion_id })
        .await;
    assert!(
        escenario
            .hasta(Duration::from_secs(10), |e| {
                e.observado.shell() == "tmux attach\nsystemctl restart nginx\ntmux attach\n"
            })
            .await,
        "shell: {:?}",
        escenario.observado.shell()
    );
}
