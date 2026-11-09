//! Prueba de extremo a extremo de la Flota del móvil, sin red externa: el
//! servidor SSH de pruebas compartido, una `magi.db` preparada con sus hosts y
//! el `Nucleo` de verdad.
//!
//! Comprueba lo que piden el informe §4.1, §5.1 y §7.2 y el checklist de la
//! Fase 14: el sondeo guarda en `SONDEOS` con `fallos_json`, las transiciones a
//! CAÍDA y su vuelta van a `REGISTRO`, la Flota se difunde completa por
//! `OyenteNucleo::flota`, la ficha lleva su bloque FLOTA y lo automático no
//! abre diálogos.
//!
//! El servidor de pruebas ejecuta los `exec` de verdad con `sh -c` y los
//! binarios falsos de la Fase 13 en el `PATH` (así el sondeo lee /proc y `df`
//! reales pero `systemctl` falso, que es lo determinista de estas pruebas).

mod comun;

use std::sync::Arc;
use std::time::Duration;

use magi_movil::api::{AjustesMovil, ErrorNucleo, EstadoFlotaMovil};
use magi_movil::estado::con_almacen;
use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::modelo::DatosHost;

use comun::servidor_pruebas::{arrancar, arrancar_con, OpcionesPrueba, PuertosPrueba};
use comun::{contar_registro, crear_host, esperar_flota, host_flota, PruebaNucleo};

/// Estados que puede dar un host sano en una máquina real: las métricas de
/// /proc y df pueden salir NOMINAL o CARGA según la carga del momento (el
/// sondeo no se falsea); lo que sí es determinista es que responde.
fn responde(estado: EstadoFlotaMovil) -> bool {
    matches!(
        estado,
        EstadoFlotaMovil::Nominal | EstadoFlotaMovil::Carga | EstadoFlotaMovil::Alcanzable
    )
}

/// El `systemctl` falso del servidor de pruebas devuelve una unidad fallida
/// conocida: el bloque de fallos del sondeo es determinista.
fn preparar_sondeo(servidor: &comun::servidor_pruebas::ServidorPruebas) {
    servidor.binarios().escribir(
        "list_units_service_failed",
        "copia.service loaded failed failed Copia nocturna\n",
    );
}

fn datos_host(nombre: &str, puerto: u16, servicios: &str) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: "127.0.0.1".to_string(),
        puerto,
        usuario: Some("usuario".to_string()),
        servicios: servicios.to_string(),
        ..DatosHost::default()
    }
}

/// Los ajustes por defecto con los motores vacíos: el sondeo no mira
/// contenedores, así su bloque de fallos es solo el de unidades.
fn ajustes_sin_motores() -> AjustesMovil {
    AjustesMovil {
        servicios_contenedores: Vec::new(),
        ..AjustesMovil::por_defecto()
    }
}

// ===========================================================================
// Pruebas
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_sondeo_guarda_la_flota_y_la_ficha_lleva_su_bloque() {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    preparar_sondeo(&servidor);
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (id_web, uuid_web) = crear_host(
        &prueba,
        datos_host("web-01", servidor.puertos.ssh, "app.service"),
    );
    // Un host de escritorio: no se sondea y sale como sin sondeo.
    let (_id_rdp, uuid_rdp) = crear_host(
        &prueba,
        DatosHost {
            nombre: "win-01".to_string(),
            direccion: "192.0.2.50".to_string(),
            tipo: TipoHost::Rdp,
            ..DatosHost::default()
        },
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();
    nucleo
        .aplicar_ajustes(ajustes_sin_motores())
        .expect("ajustes del sondeo");

    nucleo
        .sondear(Vec::new())
        .await
        .expect("lanzando el sondeo");
    let lista = esperar_flota(prueba.oyente(), Duration::from_secs(30), |lista| {
        host_flota(lista, &uuid_web)
            .is_some_and(|host| responde(host.estado) && host.ultimo_sondeo_en.is_some())
    })
    .await;

    let web = host_flota(&lista, &uuid_web).expect("web-01 en Flota");
    assert!(responde(web.estado), "{web:?}");
    assert_eq!(
        web.fallos, 1,
        "la unidad fallida del systemctl falso: {web:?}"
    );
    assert_eq!(web.sesiones, 0);
    let resumen = web.resumen.as_ref().expect("resumen de métricas");
    assert!(resumen.nucleos.is_some(), "{resumen:?}");
    assert!(resumen.carga_1m.is_some(), "{resumen:?}");
    assert!(resumen.uptime_seg.is_some(), "{resumen:?}");
    assert!(web.duracion_ms.is_some(), "la duración se mide: {web:?}");
    assert!(web.motivo.is_none(), "{web:?}");

    let escritorio = host_flota(&lista, &uuid_rdp).expect("win-01 en Flota");
    assert_eq!(escritorio.estado, EstadoFlotaMovil::SinSondeo);
    assert_eq!(
        escritorio.motivo.as_deref(),
        Some("se abre en el ordenador")
    );

    // El sondeo se guardó con su bloque de fallos y no tocó HOSTS.ultimo_estado.
    let sondeo = con_almacen(prueba.estado(), move |almacen| {
        almacen.ultimo_sondeo(id_web)
    })
    .expect("escritor")
    .expect("bd")
    .expect("sondeo guardado");
    assert_eq!(sondeo.resultado, magi_nucleo::modelo::ResultadoSondeo::Ok);
    assert_eq!(sondeo.fallos.as_ref().map(|fallos| fallos.total()), Some(1));
    assert!(sondeo.duracion_ms > 0, "{sondeo:?}");
    let ultimo_estado: Option<String> = con_almacen(prueba.estado(), move |almacen| {
        almacen
            .conexion()
            .query_row(
                "SELECT ultimo_estado FROM HOSTS WHERE id = ?1",
                [id_web],
                |fila| fila.get::<_, Option<String>>(0),
            )
            .unwrap_or(None)
    })
    .expect("escritor");
    assert!(
        ultimo_estado.is_none(),
        "el sondeo no marca el host: {ultimo_estado:?}"
    );
    assert_eq!(contar_registro(nucleo, "sondeo_fallido"), 0);
    assert!(prueba.oyente().dialogos().is_empty());

    // La API devuelve la misma lista y aplica el filtro de la Fase 11.
    let desde_api = nucleo.flota(None).expect("flota por la API");
    assert_eq!(desde_api.len(), 2);
    let filtrada = nucleo.flota(Some("web".to_string())).expect("filtro");
    assert_eq!(filtrada.len(), 1);
    assert_eq!(filtrada[0].uuid, uuid_web);
    assert!(nucleo
        .flota(Some("no-existe".to_string()))
        .expect("filtro vacío")
        .is_empty());

    // La ficha lleva el bloque FLOTA con métricas, vigilados y fallos.
    let ficha = nucleo.ficha_host(uuid_web.clone()).expect("ficha");
    let detalle = ficha.flota.expect("bloque FLOTA en la ficha");
    assert_eq!(detalle.estado, web.estado);
    let metricas = detalle.metricas.as_ref().expect("métricas completas");
    assert!(metricas.memoria_total_kb.is_some(), "{metricas:?}");
    assert!(metricas.disco_total_kb.is_some(), "{metricas:?}");
    assert_eq!(detalle.vigilados.len(), 1, "{detalle:?}");
    assert_eq!(detalle.vigilados[0].nombre, "app.service");
    assert!(detalle.vigilados[0].activo, "{detalle:?}");
    assert_eq!(detalle.fallos.len(), 1, "{detalle:?}");
    assert_eq!(detalle.fallos[0].nombre, "copia.service");
    assert!(detalle.ultimo_sondeo_en.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_paso_a_caida_anota_sondeo_fallido_y_la_vuelta_se_recupera() {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    preparar_sondeo(&servidor);
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (id_web, uuid_web) = crear_host(
        &prueba,
        datos_host("web-01", servidor.puertos.ssh, "app.service"),
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();
    nucleo
        .aplicar_ajustes(ajustes_sin_motores())
        .expect("ajustes del sondeo");

    let primero = magi_movil::flota::sondear_host(prueba.estado(), &uuid_web)
        .await
        .expect("primer sondeo");
    assert_eq!(primero.resultado, magi_nucleo::modelo::ResultadoSondeo::Ok);
    esperar_flota(prueba.oyente(), Duration::from_secs(30), |lista| {
        host_flota(lista, &uuid_web).is_some_and(|host| responde(host.estado))
    })
    .await;

    // El host deja de escuchar: se le cambia el puerto a uno cerrado y se tira
    // la conexión que quedó en gracia, como un cambio de red.
    let datos_caido = DatosHost {
        puerto: 1,
        ..datos_host("web-01", servidor.puertos.ssh, "app.service")
    };
    con_almacen(prueba.estado(), move |almacen| {
        almacen.actualizar_host(id_web, &datos_caido)
    })
    .expect("escritor")
    .expect("actualizando el host");
    prueba.estado().conexiones.forzar_cierre(&uuid_web);

    let segundo = magi_movil::flota::sondear_host(prueba.estado(), &uuid_web)
        .await
        .expect("segundo sondeo");
    assert_eq!(
        segundo.resultado,
        magi_nucleo::modelo::ResultadoSondeo::Error,
        "{segundo:?}"
    );
    let lista = esperar_flota(prueba.oyente(), Duration::from_secs(30), |lista| {
        host_flota(lista, &uuid_web).is_some_and(|host| host.estado == EstadoFlotaMovil::Caida)
    })
    .await;
    let caido = host_flota(&lista, &uuid_web).expect("web-01 caído");
    assert!(
        caido
            .motivo
            .as_deref()
            .unwrap_or_default()
            .contains("no responde"),
        "{caido:?}"
    );
    assert_eq!(contar_registro(nucleo, "sondeo_fallido"), 1);
    assert_eq!(contar_registro(nucleo, "sondeo_recuperado"), 0);

    // Vuelve a escuchar: se anota la recuperación una vez y el host responde.
    let datos_sano = datos_host("web-01", servidor.puertos.ssh, "app.service");
    con_almacen(prueba.estado(), move |almacen| {
        almacen.actualizar_host(id_web, &datos_sano)
    })
    .expect("escritor")
    .expect("actualizando el host");
    prueba.estado().conexiones.forzar_cierre(&uuid_web);
    let tercero = magi_movil::flota::sondear_host(prueba.estado(), &uuid_web)
        .await
        .expect("tercer sondeo");
    assert_eq!(tercero.resultado, magi_nucleo::modelo::ResultadoSondeo::Ok);
    esperar_flota(prueba.oyente(), Duration::from_secs(30), |lista| {
        host_flota(lista, &uuid_web).is_some_and(|host| responde(host.estado))
    })
    .await;
    assert_eq!(contar_registro(nucleo, "sondeo_fallido"), 1);
    assert_eq!(contar_registro(nucleo, "sondeo_recuperado"), 1);
    // Los tres sondeos quedan en SONDEOS (el tope es 20 por host).
    let guardados = con_almacen(prueba.estado(), move |almacen| {
        almacen
            .ultimos_sondeos(id_web, 10)
            .map(|sondeos| sondeos.len())
    })
    .expect("escritor")
    .expect("bd");
    assert_eq!(guardados, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lo_automatico_no_dialoga_y_falla_con_instruccion() {
    // Un servidor sano sin known_hosts (huella sin aceptar) y uno que rechaza
    // cualquier credencial (sin credenciales), con su huella aceptada.
    let sin_huella = arrancar(PuertosPrueba::efimeros()).await;
    let rechaza = arrancar_con(OpcionesPrueba {
        acepta_publickey: false,
        contrasena: None,
        ..OpcionesPrueba::default()
    })
    .await;
    let mut prueba = PruebaNucleo::nueva(&[rechaza.linea_known_hosts()]);
    let (_id_cache, uuid_cache) = crear_host(
        &prueba,
        datos_host("cache", sin_huella.puertos.ssh, "app.service"),
    );
    let (_id_db, uuid_db) = crear_host(
        &prueba,
        datos_host("db-01", rechaza.puertos.ssh, "app.service"),
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();

    nucleo
        .sondear(vec![uuid_cache.clone(), uuid_db.clone()])
        .await
        .expect("lanzando el sondeo");
    let lista = esperar_flota(prueba.oyente(), Duration::from_secs(30), |lista| {
        [&uuid_cache, &uuid_db].iter().all(|uuid| {
            host_flota(lista, uuid).is_some_and(|host| host.estado == EstadoFlotaMovil::Caida)
        })
    })
    .await;

    let cache = host_flota(&lista, &uuid_cache).expect("cache");
    assert_eq!(
        cache.motivo.as_deref(),
        Some("huella sin aceptar: conéctate una vez desde la ficha"),
        "{cache:?}"
    );
    let db = host_flota(&lista, &uuid_db).expect("db-01");
    assert_eq!(
        db.motivo.as_deref(),
        Some("sin credenciales: instala tu clave o guarda la contraseña"),
        "{db:?}"
    );
    // Lo automático no abre ningún diálogo (T15, T49).
    assert!(
        prueba.oyente().dialogos().is_empty(),
        "{:?}",
        prueba.oyente().dialogos()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_sondeo_reutiliza_la_conexion_en_gracia_y_no_se_solapa() {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    preparar_sondeo(&servidor);
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (_id, uuid_web) = crear_host(
        &prueba,
        datos_host("web-01", servidor.puertos.ssh, "app.service"),
    );
    prueba.arrancar();

    magi_movil::flota::sondear_host(prueba.estado(), &uuid_web)
        .await
        .expect("primer sondeo");
    let primera = prueba
        .estado()
        .conexiones
        .viva(&uuid_web)
        .expect("conexión viva tras el sondeo");
    // En gracia (60 s), el segundo sondeo reutiliza la conexión: no abre otra.
    magi_movil::flota::sondear_host(prueba.estado(), &uuid_web)
        .await
        .expect("segundo sondeo");
    let segunda = prueba
        .estado()
        .conexiones
        .viva(&uuid_web)
        .expect("la conexión sigue viva en gracia");
    assert!(
        Arc::ptr_eq(&primera, &segunda),
        "el sondeo en gracia debe reutilizar la misma conexión"
    );

    // Dos sondeos a la vez del mismo host: uno se salta con Ocupado.
    let (uno, otro) = tokio::join!(
        magi_movil::flota::sondear_host(prueba.estado(), &uuid_web),
        magi_movil::flota::sondear_host(prueba.estado(), &uuid_web),
    );
    let ocupados = [&uno, &otro]
        .iter()
        .filter(|resultado| matches!(resultado, Err(ErrorNucleo::Ocupado { .. })))
        .count();
    assert_eq!(ocupados, 1, "uno debe saltarse: {uno:?} / {otro:?}");
}
