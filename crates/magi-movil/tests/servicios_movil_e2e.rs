//! Prueba de extremo a extremo de Servicios y Logs del móvil, sin red
//! externa: el servidor SSH de pruebas compartido ejecuta `servicios.sh` con
//! los binarios falsos de la Fase 13 (systemctl, journalctl, docker, sudo y
//! nginx) y se comprueba todo el flujo del informe §4.2-§4.4 y §5.1.
//!
//! Cubre: Toda la flota sin abrir conexiones, listado y detalle de un host,
//! consumo, preparación de acciones, acción con deliberación, sudo en memoria
//! y ventanas filtradas de logs con su suscripción.

mod comun;

use std::time::Duration;

use magi_movil::api::{
    AccionMovil, AjustesMovil, DecisionDeliberacion, EstadoFlotaMovil, FiltroLogs, NivelLogMovil,
    ObjetivoMovil, PlanDeliberadoMovil, ResultadoSudo, TipoObjetivoMovil,
};
use magi_nucleo::modelo::DatosHost;

use comun::servidor_pruebas::{arrancar, PuertosPrueba};
use comun::{contar_registro, crear_host, esperar_flota, PruebaNucleo};

fn datos_host(nombre: &str, puerto: u16) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: "127.0.0.1".to_string(),
        puerto,
        usuario: Some("usuario".to_string()),
        ..DatosHost::default()
    }
}

/// Unidad de systemd como objetivo de la API.
fn unidad(nombre: &str) -> ObjetivoMovil {
    ObjetivoMovil {
        tipo: TipoObjetivoMovil::Unidad,
        nombre: nombre.to_string(),
    }
}

/// Ajustes rápidos para que las esperas de una acción no alarguen la prueba.
fn ajustes_rapidos() -> AjustesMovil {
    AjustesMovil {
        servicios_espera_seg: 5,
        servicios_contenedores: vec!["docker".to_string()],
        ..AjustesMovil::por_defecto()
    }
}

/// Monta un host con el servidor de pruebas y el núcleo arrancado.
async fn montar() -> (
    PruebaNucleo,
    comun::servidor_pruebas::ServidorPruebas,
    String,
) {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (_id, uuid) = crear_host(&prueba, datos_host("web-01", servidor.puertos.ssh));
    prueba.arrancar();
    prueba
        .nucleo()
        .aplicar_ajustes(ajustes_rapidos())
        .expect("ajustes de servicios");
    (prueba, servidor, uuid)
}

// ===========================================================================
// Toda la flota
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn toda_la_flota_usa_los_sondeos_y_no_abre_conexiones() {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (_id_web, uuid_web) = crear_host(&prueba, datos_host("web-01", servidor.puertos.ssh));
    // Un host sin escucha: saldrá caído en Toda la flota.
    let (_id_db, uuid_db) = crear_host(&prueba, datos_host("db-01", 1));
    prueba.arrancar();
    let nucleo = prueba.nucleo();
    nucleo
        .aplicar_ajustes(AjustesMovil {
            servicios_contenedores: Vec::new(),
            ..AjustesMovil::por_defecto()
        })
        .expect("ajustes");

    // Sembrar los sondeos: web-01 responde y db-01 no.
    for uuid in [&uuid_web, &uuid_db] {
        let _ = magi_movil::flota::sondear_host(prueba.estado(), uuid).await;
    }
    esperar_flota(prueba.oyente(), Duration::from_secs(30), |lista| {
        host(lista, &uuid_web).is_some_and(|host| host.estado != EstadoFlotaMovil::Sondeando)
            && host(lista, &uuid_db).is_some_and(|host| host.estado == EstadoFlotaMovil::Caida)
    })
    .await;
    let conexiones_antes = prueba.estado().conexiones.vivas().len();
    let dialogos_antes = prueba.oyente().dialogos().len();

    let toda = nucleo.servicios_toda_la_flota().expect("toda la flota");
    // Los fallos del último sondeo, con su nombre y su hora.
    let fallo = toda
        .fallos
        .iter()
        .find(|fallo| fallo.host_uuid == uuid_web)
        .expect("fallo de web-01 en Toda la flota");
    assert_eq!(fallo.fallo.nombre, "copia.service", "{fallo:?}");
    assert!(fallo.sondeo_en.is_some());
    // La línea del host caído.
    let caido = toda
        .hosts
        .iter()
        .find(|host| host.host_uuid == uuid_db)
        .expect("db-01 en Toda la flota");
    assert_eq!(caido.estado, EstadoFlotaMovil::Caida);
    assert!(caido.motivo.is_some(), "{caido:?}");
    // Entrar en Toda la flota no abre conexiones ni diálogos.
    assert_eq!(
        prueba.estado().conexiones.vivas().len(),
        conexiones_antes,
        "Toda la flota no debe abrir conexiones"
    );
    assert_eq!(prueba.oyente().dialogos().len(), dialogos_antes);
}

fn host(lista: &[magi_movil::api::HostFlota], uuid: &str) -> Option<magi_movil::api::HostFlota> {
    lista.iter().find(|host| host.uuid == uuid).cloned()
}

// ===========================================================================
// Listado, detalle, consumo y preparación de acciones
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn listado_detalle_consumo_y_preparar_accion() {
    let (prueba, _servidor, uuid) = montar().await;
    let nucleo = prueba.nucleo();

    let listado = nucleo
        .servicios_listar(uuid.clone(), false)
        .await
        .expect("listado de servicios");
    assert!(
        matches!(
            listado.systemd,
            magi_movil::api::SystemdMovil::Disponible(_)
        ),
        "{:?}",
        listado.systemd
    );
    assert_eq!(
        listado.privilegio,
        magi_movil::api::PrivilegioMovil::SudoSinClave
    );
    assert!(!listado.sudo_en_memoria);
    assert!(listado.hora.is_some(), "el listado lleva su hora");
    assert!(
        listado
            .unidades
            .iter()
            .any(|unidad| unidad.nombre == "app.service"),
        "{listado:?}"
    );
    // app.service está activa y no admite recargar: ofrece parar y reiniciar.
    let app = listado
        .unidades
        .iter()
        .find(|unidad| unidad.nombre == "app.service")
        .expect("app.service");
    assert!(app.acciones.contains(&AccionMovil::Parar), "{app:?}");
    assert!(!app.acciones.contains(&AccionMovil::Recargar), "{app:?}");
    // nginx.service está activa y sí admite recargar.
    let nginx = listado
        .unidades
        .iter()
        .find(|unidad| unidad.nombre == "nginx.service")
        .expect("nginx.service");
    assert!(nginx.acciones.contains(&AccionMovil::Recargar), "{nginx:?}");
    // El timer sale en su sección.
    assert!(
        listado
            .timers
            .iter()
            .any(|timer| timer.nombre == "app.timer"),
        "{listado:?}"
    );
    // Docker está disponible y sus contenedores se listan (uno caído).
    assert!(
        listado
            .contenedores
            .iter()
            .any(|contenedor| contenedor.nombre == "panel"),
        "{listado:?}"
    );

    // Detalle de app.service: propiedades y el texto de la unidad.
    let detalle = nucleo
        .servicios_detalle(uuid.clone(), unidad("app.service"))
        .await
        .expect("detalle de app.service");
    assert_eq!(detalle.objetivo.nombre, "app.service");
    assert!(
        detalle
            .props
            .iter()
            .any(|propiedad| propiedad.clave == "ActiveState" && propiedad.valor == "active"),
        "{detalle:?}"
    );
    assert!(!detalle.texto.is_empty(), "el texto de la unidad llega");

    // Consumo: no falla aunque el falso no dé cifras.
    let consumo = nucleo
        .servicios_consumo(uuid.clone())
        .await
        .expect("consumo de contenedores");
    let _ = consumo;

    // Recargar nginx no delibera y se permite; la comprobación de
    // configuración se anuncia en la confirmación.
    let info = nucleo
        .preparar_accion(uuid.clone(), unidad("nginx.service"), AccionMovil::Recargar)
        .expect("preparando la recarga de nginx");
    assert!(info.permitida, "{info:?}");
    assert!(!info.delibera, "{info:?}");
    assert!(
        info.comprobacion.is_some(),
        "nginx tiene comprobación: {info:?}"
    );
    assert!(!info.critica);

    // Parar app.service deja algo parado: exige deliberar.
    let info = nucleo
        .preparar_accion(uuid.clone(), unidad("app.service"), AccionMovil::Parar)
        .expect("preparando parar app.service");
    assert!(info.permitida, "{info:?}");
    assert!(info.delibera, "{info:?}");
}

// ===========================================================================
// Acción con deliberación y sudo en memoria
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accion_con_deliberacion_registro_y_sudo() {
    let (prueba, servidor, uuid) = montar().await;
    let nucleo = prueba.nucleo();
    // El action del systemctl falso deja app.service inactiva al pararla.
    servidor.binarios().escribir("accion_estado", "inactive");
    let objetivo = unidad("app.service");
    // preparar_accion recompone sus datos del último listado del host: la app
    // abre los servicios antes de actuar, así que se lista primero.
    nucleo
        .servicios_listar(uuid.clone(), false)
        .await
        .expect("listando servicios antes de actuar");

    let preparacion = nucleo
        .preparar_accion(uuid.clone(), objetivo.clone(), AccionMovil::Parar)
        .expect("preparando la acción");
    assert!(preparacion.permitida, "{preparacion:?}");
    assert!(preparacion.delibera, "{preparacion:?}");

    // Sin deliberación, la acción que la exige no se ejecuta.
    let sin_deliberar = nucleo
        .servicio_accion(uuid.clone(), objetivo.clone(), AccionMovil::Parar, None)
        .await;
    assert!(sin_deliberar.is_err(), "{sin_deliberar:?}");

    // Deliberar (sin verificaciones activas sale APROBADO) y ejecutar.
    let tabla = nucleo
        .deliberar(PlanDeliberadoMovil::Servicio {
            host: uuid.clone(),
            objetivo: objetivo.clone(),
            accion: AccionMovil::Parar,
            comprobacion: preparacion.comprobacion.clone(),
        })
        .await
        .expect("deliberando la acción");
    assert_eq!(tabla.filas.len(), 1, "{tabla:?}");
    let deliberacion = nucleo
        .resolver_deliberacion(tabla.id, DecisionDeliberacion::Aprobar)
        .expect("aprobando la deliberación");
    let resultado = nucleo
        .servicio_accion(
            uuid.clone(),
            objetivo.clone(),
            AccionMovil::Parar,
            Some(deliberacion),
        )
        .await
        .expect("ejecutando la acción");
    assert_eq!(resultado.objetivo.nombre, "app.service");
    assert_eq!(resultado.host_uuid, uuid);
    assert_eq!(
        resultado.resultado,
        magi_movil::api::ResultadoAccionMovil::Hecho,
        "{resultado:?}"
    );
    assert_eq!(contar_registro(nucleo, "servicio_accion"), 1);

    // La deliberación es de un solo uso: repetir la misma acción con ella falla.
    let repetida = nucleo
        .servicio_accion(
            uuid.clone(),
            objetivo.clone(),
            AccionMovil::Arrancar,
            Some(deliberacion),
        )
        .await;
    assert!(repetida.is_err(), "{repetida:?}");

    // El resultado llegó también por la retrollamada aunque la pantalla no esté.
    let mut hecha = None;
    for _ in 0..100 {
        if let Some(ultima) = prueba.oyente().acciones_hechas().last() {
            hecha = Some(ultima.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let hecha = hecha.expect("accion_servicio_hecha no llegó");
    assert_eq!(hecha.objetivo.nombre, "app.service");

    // sudo: con contraseña configurada en el host falso, primero rechaza la
    // mala y luego acepta la buena y la deja en memoria hasta olvidarla.
    servidor.binarios().escribir("sudo_modo", "con_clave");
    // El privilegio está cacheado por host: se olvida para que el listado lo
    // vuelva a detectar, como cuando el host cambia su política de sudo.
    prueba
        .estado()
        .privilegios_servicios
        .lock()
        .expect("privilegios")
        .remove(&uuid);
    let listado = nucleo
        .servicios_listar(uuid.clone(), true)
        .await
        .expect("listado con sudo con contraseña");
    assert_eq!(
        listado.privilegio,
        magi_movil::api::PrivilegioMovil::SudoConClave
    );
    assert!(!listado.sudo_en_memoria);
    let rechazada = nucleo
        .sudo_comprobar(uuid.clone(), b"mala".to_vec())
        .await
        .expect("comprobando la contraseña mala");
    assert_eq!(rechazada, ResultadoSudo::Rechazada);
    let aceptada = nucleo
        .sudo_comprobar(uuid.clone(), b"secreta".to_vec())
        .await
        .expect("comprobando la contraseña buena");
    assert_eq!(aceptada, ResultadoSudo::Ok);
    let listado = nucleo
        .servicios_listar(uuid.clone(), true)
        .await
        .expect("listado con la contraseña en memoria");
    assert!(listado.sudo_en_memoria, "{listado:?}");
    nucleo
        .sudo_olvidar(uuid.clone())
        .expect("olvidando el sudo");
    let listado = nucleo
        .servicios_listar(uuid.clone(), true)
        .await
        .expect("listado tras olvidar");
    assert!(!listado.sudo_en_memoria);
}

// ===========================================================================
// Logs: suscripción y ventanas filtradas
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logs_por_ventanas_filtradas_y_suscripcion() {
    let (prueba, _servidor, uuid) = montar().await;
    let nucleo = prueba.nucleo();
    let objetivo = unidad("app.service");

    let suscripcion = nucleo
        .logs_seguir(uuid.clone(), objetivo.clone(), 200)
        .await
        .expect("abriendo el seguimiento de logs");
    // El falso de journalctl imprime el diario y se queda siguiendo: la
    // ventana tiene que ver sus líneas.
    let mut ventana = None;
    let mut ultima = None;
    for _ in 0..100 {
        let filtro = FiltroLogs {
            texto: String::new(),
            nivel: NivelLogMovil::Todo,
            solo_stderr: false,
            desde: None,
            cuantas: 500,
        };
        match nucleo.logs_ventana(suscripcion, filtro) {
            Ok(actual) => {
                if actual.total >= 2 {
                    ventana = Some(actual);
                    break;
                }
                ultima = Some(actual);
            }
            Err(error) => eprintln!("ventana con error: {error:?}"),
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let ventana = match ventana {
        Some(ventana) => ventana,
        None => {
            panic!("la ventana de logs no llegó a tener líneas: {ultima:?}");
        }
    };
    assert!(ventana.total >= 2, "{ventana:?}");
    assert!(
        ventana
            .lineas
            .iter()
            .any(|linea| linea.texto.contains("arrancando app")),
        "{ventana:?}"
    );
    // Filtro por texto: solo la línea que lo contiene.
    let filtro = FiltroLogs {
        texto: "en marcha".to_string(),
        nivel: NivelLogMovil::Todo,
        solo_stderr: false,
        desde: None,
        cuantas: 500,
    };
    let filtrada = nucleo
        .logs_ventana(suscripcion, filtro)
        .expect("ventana filtrada");
    assert_eq!(filtrada.total, 1, "{filtrada:?}");
    assert!(filtrada.lineas[0].texto.contains("app en marcha"));
    // Filtro por nivel: las dos líneas del falso son info, ninguna es error.
    let filtro = FiltroLogs {
        texto: String::new(),
        nivel: NivelLogMovil::Error,
        solo_stderr: false,
        desde: None,
        cuantas: 500,
    };
    let errores = nucleo
        .logs_ventana(suscripcion, filtro)
        .expect("ventana de errores");
    assert!(errores.lineas.is_empty(), "{errores:?}");
    // Ventana desde un número de línea: no repite las anteriores.
    let primera = ventana.lineas[0].numero;
    let filtro = FiltroLogs {
        texto: String::new(),
        nivel: NivelLogMovil::Todo,
        solo_stderr: false,
        desde: Some(primera + 1),
        cuantas: 500,
    };
    let desde = nucleo
        .logs_ventana(suscripcion, filtro)
        .expect("ventana desde una línea");
    assert!(
        desde.lineas.iter().all(|linea| linea.numero > primera),
        "{desde:?}"
    );
    // La suscripción avisó por la retrollamada (como mucho cada 100 ms).
    let mut avisos = Vec::new();
    for _ in 0..100 {
        avisos = prueba.oyente().logs();
        if !avisos.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!avisos.is_empty(), "OyenteNucleo.logs no llegó");

    // Parar el seguimiento: fin de la entrada estándar y el estado lo dice.
    nucleo.logs_parar(suscripcion).expect("parando los logs");
    let mut parada = None;
    for _ in 0..100 {
        let filtro = FiltroLogs {
            texto: String::new(),
            nivel: NivelLogMovil::Todo,
            solo_stderr: false,
            desde: None,
            cuantas: 500,
        };
        let ventana = nucleo
            .logs_ventana(suscripcion, filtro)
            .expect("ventana tras parar");
        if ventana.estado != magi_movil::api::EstadoSuscripcionLog::Siguiendo {
            parada = Some(ventana);
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let parada = parada.expect("la suscripción no pasó a parada");
    assert!(parada.total >= 2, "{parada:?}");
}
