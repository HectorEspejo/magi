//! Pruebas de la plataforma de Termux (Fase 17, Sprint 1): las órdenes
//! `termux-*` (sin shell, con plazo, con tope de salida y con el apagado de
//! Termux:API), las rutas del entorno, las propiedades de Android, el usuario
//! local y la shell de la deliberación. Todo con los dobles de
//! `tests/fijos/termux/`: ni Termux ni red.
//!
//! Las pruebas que tocan el entorno del proceso (`set_var`) van todas bajo un
//! mismo cerrojo; las que lanzan dobles no lo tocan.

#[path = "fijos/termux/mod.rs"]
mod dobles;

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use magi::termux::ordenes::{Ejecucion, DE_LA_API, TOPE_SALIDA};
use magi::termux::rutas::calcular;
use magi::termux::{ApiProceso, EntornoTermux, EstadoApi, FalloOrden, Ordenes, Propiedades};

use dobles::{sigue_vivo_tras, Dobles, Modo};

// ------------------------------------------------------------ escenarios

struct Escenario {
    temporal: tempfile::TempDir,
    dobles: Dobles,
    entorno: EntornoTermux,
    api: Arc<ApiProceso>,
}

impl Escenario {
    fn nuevo() -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let dobles = Dobles::nuevos(temporal.path());
        let entorno = dobles.entorno(&temporal.path().join("home"));
        Self {
            temporal,
            dobles,
            entorno,
            api: Arc::new(ApiProceso::default()),
        }
    }

    fn ordenes(&self, plazo: Duration) -> Ordenes {
        Ordenes::nuevas(&self.entorno, plazo, Arc::clone(&self.api))
    }

    /// Unas órdenes con un plazo holgado para las pruebas que no miden tiempo.
    fn holgadas(&self) -> Ordenes {
        self.ordenes(Duration::from_secs(20))
    }
}

/// Cualquier orden de la API, para recorrerlas.
fn orden_de_la_api(programa: &str) -> bool {
    DE_LA_API.contains(&programa)
}

/// Las 4 órdenes que no son de la API.
const SIN_API: [&str; 4] = [
    "termux-open-url",
    "termux-wake-lock",
    "termux-wake-unlock",
    "termux-reload-settings",
];

/// Cada entorno del proceso que se toca, restaurado al terminar la prueba.
static ENTORNO: Mutex<()> = Mutex::new(());

struct GuardaEntorno {
    previos: Vec<(&'static str, Option<OsString>)>,
    _cerrojo: MutexGuard<'static, ()>,
}

impl GuardaEntorno {
    /// Pone (`Some`) o quita (`None`) cada variable hasta que se suelte.
    fn nuevo(cambios: &[(&'static str, Option<&str>)]) -> Self {
        let cerrojo = ENTORNO.lock().unwrap_or_else(|e| e.into_inner());
        let mut previos = Vec::new();
        for (nombre, valor) in cambios {
            previos.push((*nombre, std::env::var_os(nombre)));
            match valor {
                Some(valor) => std::env::set_var(nombre, valor),
                None => std::env::remove_var(nombre),
            }
        }
        Self {
            previos,
            _cerrojo: cerrojo,
        }
    }
}

impl Drop for GuardaEntorno {
    fn drop(&mut self) {
        for (nombre, valor) in self.previos.drain(..).rev() {
            match valor {
                Some(valor) => std::env::set_var(nombre, valor),
                None => std::env::remove_var(nombre),
            }
        }
    }
}

// ----------------------------------------------------------- sin shell

#[test]
fn un_argumento_con_metacaracteres_de_shell_no_ejecuta_nada() {
    let escenario = Escenario::nuevo();
    let marca = escenario.temporal.path().join("ejecutado");
    let ordenes = escenario.holgadas();
    let raros = [
        format!("http://ejemplo.org/; touch {}", marca.display()),
        format!("$(touch {})", marca.display()),
        format!("`touch {}`", marca.display()),
        format!("a && touch {} #", marca.display()),
        "con 'comillas' y \"dobles\"\ny un salto de línea".to_string(),
        String::new(),
    ];
    for argumento in &raros {
        ordenes
            .ejecutar(&["termux-open-url", argumento], None)
            .expect("la orden termina bien");
    }
    assert!(!marca.exists(), "un argumento se ejecutó como shell");
    let llamadas = escenario.dobles.llamadas("termux-open-url");
    assert_eq!(llamadas.len(), raros.len());
    for (llamada, esperado) in llamadas.iter().zip(&raros) {
        assert_eq!(llamada.args, vec![esperado.clone()]);
    }
}

#[test]
fn una_orden_con_ruta_o_vacia_no_se_busca_ni_se_ejecuta() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.holgadas();
    for nombre in ["", "/bin/sh", "../usr/bin/termux-open-url", "usr/bin/sh"] {
        let error = ordenes.ejecutar(&[nombre], None).unwrap_err();
        assert!(
            matches!(error, FalloOrden::NoInstalada(_) | FalloOrden::Io(_)),
            "{nombre:?}: {error:?}"
        );
    }
    assert_eq!(
        ordenes.ejecutar(&[], None).unwrap_err(),
        FalloOrden::Io("orden vacía".into())
    );
}

#[test]
fn los_argumentos_llegan_tal_cual_y_sin_entrada() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.holgadas();
    ordenes
        .ejecutar(
            &[
                "termux-notification",
                "--id",
                "magi-1",
                "-t",
                "◆ agente-api pide atención",
                "-c",
                "dev-01 · MAGI",
            ],
            None,
        )
        .unwrap();
    let llamadas = escenario.dobles.llamadas("termux-notification");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(
        llamadas[0].args,
        [
            "--id",
            "magi-1",
            "-t",
            "◆ agente-api pide atención",
            "-c",
            "dev-01 · MAGI"
        ]
    );
    assert!(
        llamadas[0].entrada.is_empty(),
        "sin entrada, la entrada es /dev/null"
    );
    // Sin argumentos no anota uno vacío de más.
    ordenes.ejecutar(&["termux-wake-lock"], None).unwrap();
    assert_eq!(
        escenario.dobles.llamadas("termux-wake-lock")[0].args.len(),
        0
    );
}

// -------------------------------------------------------------- plazo

#[test]
fn el_plazo_da_sin_respuesta_a_tiempo_y_mata_el_grupo_entero() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-clipboard-set", Modo::Colgar);
    let plazo = Duration::from_millis(1500);
    let ordenes = escenario.ordenes(plazo);
    // Medio MiB que el doble no lee: la escritura no puede bloquear la orden.
    let entrada = vec![b'x'; 512 * 1024];

    let inicio = Instant::now();
    let resultado = ordenes.ejecutar(&["termux-clipboard-set"], Some(&entrada));
    let tardado = inicio.elapsed();

    assert_eq!(resultado, Err(FalloOrden::SinRespuesta));
    assert!(tardado >= plazo, "no esperó el plazo: {tardado:?}");
    assert!(
        tardado < plazo + Duration::from_millis(1500),
        "se pasó del plazo: {tardado:?}"
    );

    let llamadas = escenario.dobles.llamadas("termux-clipboard-set");
    assert_eq!(llamadas.len(), 1);
    let pids = &llamadas[0].pids;
    assert_eq!(pids.len(), 2, "el doble y su hijo anotan su pid: {pids:?}");
    for pid in pids {
        assert!(
            !sigue_vivo_tras(*pid, Duration::from_secs(3)),
            "el proceso {pid} sigue vivo"
        );
    }
    // El líder se recogió: no queda ni zombi.
    if Path::new("/proc/self").exists() {
        assert!(
            !Path::new(&format!("/proc/{}", pids[0])).exists(),
            "el hijo quedó como zombi"
        );
    }
    assert_eq!(escenario.api.estado(), EstadoApi::NoResponde);
}

#[test]
fn un_doble_lento_pasa_si_cabe_en_el_plazo_y_no_si_no() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-battery-status", Modo::Lento(3));
    let salida = escenario
        .ordenes(Duration::from_secs(10))
        .ejecutar(&["termux-battery-status"], None)
        .expect("0,3 s caben en 10 s");
    assert!(String::from_utf8_lossy(&salida).contains("percentage"));
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);

    // Otra API, más lenta que su plazo.
    let otra = Escenario::nuevo();
    otra.dobles
        .programar("termux-battery-status", Modo::Lento(40));
    let inicio = Instant::now();
    let resultado = otra
        .ordenes(Duration::from_millis(400))
        .ejecutar(&["termux-battery-status"], None);
    assert_eq!(resultado, Err(FalloOrden::SinRespuesta));
    assert!(inicio.elapsed() < Duration::from_secs(3), "esperó de más");
}

// ------------------------------------------------- orden cancelable

/// Espera a que el doble `programa` haya anotado sus dos pids (el suyo y el de
/// su hijo): ya está corriendo y ya se pueden mirar. Devuelve los pids.
fn esperar_a_que_corra(dobles: &Dobles, programa: &str) -> Vec<i32> {
    let limite = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(llamada) = dobles.llamadas(programa).first() {
            if llamada.pids.len() == 2 {
                return llamada.pids.clone();
            }
        }
        assert!(Instant::now() < limite, "el doble no llegó a arrancar");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn una_orden_cancelable_se_mata_con_todo_su_grupo_al_cancelarse() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-battery-status", Modo::Colgar);
    // Un plazo larguísimo: lo que la para es la cancelación, no el plazo.
    let ordenes = escenario.ordenes(Duration::from_secs(60));
    let cancelar = AtomicBool::new(false);

    let inicio = Instant::now();
    let (resultado, pids) = std::thread::scope(|ambito| {
        let vigilante = ambito.spawn(|| {
            let pids = esperar_a_que_corra(&escenario.dobles, "termux-battery-status");
            cancelar.store(true, Ordering::SeqCst);
            pids
        });
        let resultado = ordenes.ejecutar_cancelable(&["termux-battery-status"], None, &|| {
            cancelar.load(Ordering::SeqCst)
        });
        (resultado, vigilante.join().expect("el vigilante"))
    });
    let tardado = inicio.elapsed();

    assert_eq!(resultado, Ejecucion::Cancelada);
    assert!(
        tardado < Duration::from_secs(10),
        "esperó al plazo en vez de cancelarse: {tardado:?}"
    );
    // El grupo entero (el doble y su hijo) murió y el hijo se recogió.
    for pid in &pids {
        assert!(
            !sigue_vivo_tras(*pid, Duration::from_secs(3)),
            "el proceso {pid} sigue vivo"
        );
    }
    if Path::new("/proc/self").exists() {
        assert!(
            !Path::new(&format!("/proc/{}", pids[0])).exists(),
            "el hijo quedó como zombi"
        );
    }
    // Cancelar no es un fallo de Termux:API: no cambia su estado.
    assert_eq!(escenario.api.estado(), EstadoApi::SinComprobar);
}

#[test]
fn una_orden_cancelable_ya_cancelada_no_lanza_nada() {
    let escenario = Escenario::nuevo();
    let resultado =
        escenario
            .holgadas()
            .ejecutar_cancelable(&["termux-battery-status"], None, &|| true);
    assert_eq!(resultado, Ejecucion::Cancelada);
    assert!(escenario
        .dobles
        .llamadas("termux-battery-status")
        .is_empty());
    assert_eq!(escenario.api.estado(), EstadoApi::SinComprobar);
}

#[test]
fn una_orden_cancelable_que_no_se_cancela_es_como_ejecutar() {
    let nunca = || false;
    // Bien: responde y la API queda disponible.
    let escenario = Escenario::nuevo();
    match escenario
        .holgadas()
        .ejecutar_cancelable(&["termux-battery-status"], None, &nunca)
    {
        Ejecucion::Terminada(Ok(salida)) => {
            assert!(String::from_utf8_lossy(&salida).contains("percentage"));
        }
        otra => panic!("debía responder: {otra:?}"),
    }
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);

    // Sin el programa: lo mismo que `ejecutar`, y apaga la API.
    let otra = Escenario::nuevo();
    otra.dobles.quitar("termux-battery-status");
    assert_eq!(
        otra.holgadas()
            .ejecutar_cancelable(&["termux-battery-status"], None, &nunca),
        Ejecucion::Terminada(Err(FalloOrden::NoInstalada("termux-battery-status".into())))
    );
    assert_eq!(otra.api.estado(), EstadoApi::Ausente);

    // Colgada con plazo corto: vence el plazo, no se cancela.
    let colgada = Escenario::nuevo();
    colgada
        .dobles
        .programar("termux-battery-status", Modo::Colgar);
    assert_eq!(
        colgada
            .ordenes(Duration::from_millis(300))
            .ejecutar_cancelable(&["termux-battery-status"], None, &nunca),
        Ejecucion::Terminada(Err(FalloOrden::SinRespuesta))
    );
    assert_eq!(colgada.api.estado(), EstadoApi::NoResponde);
}

// ------------------------------------------------------- tope y código

#[test]
fn la_salida_se_corta_en_el_tope_y_no_bloquea_al_hijo() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-battery-status", Modo::MuchaSalida);
    let ordenes = escenario.ordenes(Duration::from_secs(20));
    let inicio = Instant::now();
    let salida = ordenes
        .ejecutar(&["termux-battery-status"], None)
        .expect("termina bien aunque escriba 1 MiB por stdout y otro por stderr");
    assert_eq!(salida.len(), TOPE_SALIDA, "se guarda el tope y nada más");
    assert!(salida.iter().all(|byte| *byte == b'x'));
    assert!(
        inicio.elapsed() < Duration::from_secs(10),
        "tardó demasiado"
    );
}

#[test]
fn un_codigo_distinto_de_cero_es_un_fallo_con_el_error_en_una_linea() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.holgadas();

    escenario.dobles.programar("termux-open-url", Modo::Fallar);
    match ordenes.ejecutar(&["termux-open-url", "http://ejemplo.org"], None) {
        Err(FalloOrden::Codigo { codigo, error }) => {
            assert_eq!(codigo, Some(1));
            assert_eq!(error, "fallo simulado de termux-open-url");
        }
        otro => panic!("se esperaba Codigo, llegó {otro:?}"),
    }

    // stderr con colores, tabuladores, un BEL y 700 caracteres.
    escenario
        .dobles
        .programar("termux-reload-settings", Modo::ErrorLargo);
    match ordenes.ejecutar(&["termux-reload-settings"], None) {
        Err(FalloOrden::Codigo { codigo, error }) => {
            assert_eq!(codigo, Some(2));
            assert_eq!(error.chars().count(), 200, "{error:?}");
            assert!(!error.chars().any(char::is_control), "{error:?}");
            assert!(
                error.starts_with("[31mfallo [0m con control eee"),
                "{error:?}"
            );
        }
        otro => panic!("se esperaba Codigo, llegó {otro:?}"),
    }

    // Con 1 MiB por stderr, el error sigue siendo de 200 caracteres.
    escenario
        .dobles
        .programar("termux-wake-lock", Modo::MuchaSalidaFallo);
    match ordenes.ejecutar(&["termux-wake-lock"], None) {
        Err(FalloOrden::Codigo { codigo, error }) => {
            assert_eq!(codigo, Some(1));
            assert_eq!(error, "y".repeat(200));
        }
        otro => panic!("se esperaba Codigo, llegó {otro:?}"),
    }
}

#[test]
fn una_orden_terminada_por_una_senal_no_tiene_codigo() {
    // Un programa que se mata a sí mismo: el estado no trae código.
    let escenario = Escenario::nuevo();
    let programa = escenario.dobles.bin().join("termux-open-url");
    std::fs::write(&programa, "#!/bin/sh\nkill -KILL $$\n").unwrap();
    match escenario.holgadas().ejecutar(&["termux-open-url"], None) {
        Err(FalloOrden::Codigo {
            codigo: None,
            error,
        }) => assert!(error.is_empty()),
        otro => panic!("se esperaba Codigo sin código, llegó {otro:?}"),
    }
}

// ------------------------------------------------------ Termux:API

#[test]
fn un_programa_de_la_api_que_no_esta_instalado_apaga_la_api() {
    let escenario = Escenario::nuevo();
    escenario.dobles.quitar("termux-battery-status");
    let ordenes = escenario.holgadas();

    let (resultado, recien) = ordenes.ejecutar_avisando(&["termux-battery-status"], None);
    assert_eq!(
        resultado,
        Err(FalloOrden::NoInstalada("termux-battery-status".into()))
    );
    assert!(recien, "la primera vez avisa");
    assert_eq!(escenario.api.estado(), EstadoApi::Ausente);
    assert!(escenario.api.apagada());

    // Ya apagada: ni lo intenta, ni avisa otra vez.
    let (resultado, recien) = ordenes.ejecutar_avisando(&["termux-battery-status"], None);
    assert_eq!(resultado, Err(FalloOrden::ApiApagada));
    assert!(!recien);
}

#[test]
fn con_la_api_apagada_ninguna_orden_de_la_api_vuelve_a_intentarse() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-clipboard-set", Modo::Fallar);
    let ordenes = escenario.holgadas();

    assert!(matches!(
        ordenes.ejecutar(&["termux-clipboard-set"], Some(b"hola")),
        Err(FalloOrden::Codigo { .. })
    ));
    assert_eq!(escenario.api.estado(), EstadoApi::NoResponde);
    assert_eq!(escenario.dobles.llamadas("termux-clipboard-set").len(), 1);

    // Cero llamadas nuevas, ni en la misma orden ni en las otras de la API.
    for programa in DE_LA_API {
        for _ in 0..3 {
            assert_eq!(
                ordenes.ejecutar(&[programa, "x"], Some(b"datos")),
                Err(FalloOrden::ApiApagada),
                "{programa}"
            );
        }
    }
    assert_eq!(escenario.dobles.llamadas("termux-clipboard-set").len(), 1);
    for programa in [
        "termux-notification",
        "termux-notification-remove",
        "termux-battery-status",
    ] {
        assert!(escenario.dobles.llamadas(programa).is_empty(), "{programa}");
    }

    // Las de termux-tools siguen funcionando.
    ordenes
        .ejecutar(&["termux-open-url", "http://ejemplo.org"], None)
        .expect("termux-open-url no es de la API");
    assert_eq!(escenario.dobles.llamadas("termux-open-url").len(), 1);
    assert_eq!(
        escenario.api.estado(),
        EstadoApi::NoResponde,
        "no se revierte"
    );
}

#[test]
fn una_respuesta_correcta_marca_la_api_disponible() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.holgadas();
    assert_eq!(escenario.api.estado(), EstadoApi::SinComprobar);
    let (resultado, recien) = ordenes.ejecutar_avisando(&["termux-battery-status"], None);
    assert!(resultado.is_ok());
    assert!(!recien);
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);
    assert!(!escenario.api.apagada());
}

#[test]
fn una_orden_de_la_api_que_falla_o_se_cuelga_la_apaga_una_sola_vez() {
    for modo in [Modo::Fallar, Modo::Colgar] {
        let escenario = Escenario::nuevo();
        escenario.dobles.programar("termux-notification", modo);
        let ordenes = escenario.ordenes(Duration::from_millis(1500));
        let (resultado, recien) =
            ordenes.ejecutar_avisando(&["termux-notification", "-t", "x"], None);
        assert!(resultado.is_err(), "{modo:?}");
        assert!(recien, "{modo:?}: la primera vez avisa");
        assert_eq!(escenario.api.estado(), EstadoApi::NoResponde);
        let (resultado, recien) =
            ordenes.ejecutar_avisando(&["termux-notification-remove", "x"], None);
        assert_eq!(resultado, Err(FalloOrden::ApiApagada));
        assert!(!recien, "{modo:?}: la segunda no");
    }
}

#[test]
fn las_ordenes_que_no_son_de_la_api_no_apagan_nada_aunque_fallen() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.ordenes(Duration::from_millis(1500));

    for programa in SIN_API {
        assert!(!orden_de_la_api(programa));
        // Falla.
        escenario.dobles.programar(programa, Modo::Fallar);
        let (resultado, recien) = ordenes.ejecutar_avisando(&[programa], None);
        assert!(
            matches!(resultado, Err(FalloOrden::Codigo { .. })),
            "{programa}"
        );
        assert!(!recien, "{programa}");
        // Se cuelga.
        escenario.dobles.programar(programa, Modo::Colgar);
        let (resultado, recien) = ordenes.ejecutar_avisando(&[programa], None);
        assert_eq!(resultado, Err(FalloOrden::SinRespuesta), "{programa}");
        assert!(!recien, "{programa}");
        // No está instalada.
        escenario.dobles.quitar(programa);
        let (resultado, recien) = ordenes.ejecutar_avisando(&[programa], None);
        assert_eq!(resultado, Err(FalloOrden::NoInstalada(programa.into())));
        assert!(!recien, "{programa}");
        // Nada de eso toca el estado de la API.
        assert_eq!(
            escenario.api.estado(),
            EstadoApi::SinComprobar,
            "{programa}"
        );
    }
    // Y tampoco la marcan como disponible cuando van bien.
    let bien = Escenario::nuevo();
    bien.ordenes(Duration::from_secs(5))
        .ejecutar(&["termux-open-url", "http://ejemplo.org"], None)
        .unwrap();
    assert_eq!(bien.api.estado(), EstadoApi::SinComprobar);
}

// Un fallo que no es de Termux:API (un argumento que el sistema no admite, no
// poder lanzar el programa) se informa, pero no apaga la función: §7.2 solo
// apaga por «no instalada», plazo agotado y código distinto de 0.

#[test]
fn un_argumento_con_nul_es_un_fallo_de_io_y_no_apaga_la_api() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.holgadas();

    let (resultado, recien) =
        ordenes.ejecutar_avisando(&["termux-notification", "-t", "a\0b"], None);
    assert!(matches!(resultado, Err(FalloOrden::Io(_))), "{resultado:?}");
    assert!(
        !recien,
        "no es un apagado: nadie debe avisar de «no responde»"
    );
    assert_eq!(escenario.api.estado(), EstadoApi::SinComprobar);
    assert!(!escenario.api.apagada());
    // El mensaje no copia el argumento (puede ser un texto del usuario).
    if let Err(FalloOrden::Io(causa)) = resultado {
        assert!(!causa.contains('b'), "{causa:?}");
        assert!(causa.contains('2'), "dice qué argumento es: {causa:?}");
    }
    // Ni siquiera se llegó a lanzar el programa.
    assert!(escenario.dobles.llamadas("termux-notification").is_empty());

    // Sigue disponible: la siguiente orden de la API corre y la marca.
    let (resultado, recien) = ordenes.ejecutar_avisando(&["termux-notification", "-t", "ab"], None);
    assert!(resultado.is_ok(), "{resultado:?}");
    assert!(!recien);
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);
    assert_eq!(escenario.dobles.llamadas("termux-notification").len(), 1);

    // También con las órdenes que no son de la API y con el NUL en el propio
    // nombre: el mismo fallo de E/S, sin lanzar ni tocar nada.
    for argumentos in [
        &["termux-open-url", "http://ejemplo.org/\0"][..],
        &["termux-clipboard-set", "\0"][..],
        &["termux-battery-status\0"][..],
    ] {
        let (resultado, recien) = ordenes.ejecutar_avisando(argumentos, None);
        assert!(
            matches!(resultado, Err(FalloOrden::Io(_))),
            "{argumentos:?}: {resultado:?}"
        );
        assert!(!recien);
    }
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);
    assert!(escenario.dobles.llamadas("termux-open-url").is_empty());
    assert!(escenario.dobles.llamadas("termux-clipboard-set").is_empty());
}

#[tokio::test]
async fn el_nul_tampoco_apaga_la_api_por_el_camino_asincrono() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.holgadas();
    let (resultado, recien) = ordenes
        .ejecutar_async_avisando(
            vec!["termux-notification".into(), "-t".into(), "a\0b".into()],
            None,
        )
        .await;
    assert!(matches!(resultado, Err(FalloOrden::Io(_))), "{resultado:?}");
    assert!(!recien);
    assert_eq!(escenario.api.estado(), EstadoApi::SinComprobar);
    ordenes
        .ejecutar_async(
            vec!["termux-notification".into(), "-t".into(), "ab".into()],
            None,
        )
        .await
        .expect("la API sigue disponible");
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);
}

#[test]
fn un_nul_no_apaga_una_api_sin_comprobar_ni_la_vuelve_a_apagar_si_ya_lo_estaba() {
    // Sin comprobar: sigue sin comprobar tras el fallo.
    let nueva = Escenario::nuevo();
    assert!(matches!(
        nueva
            .holgadas()
            .ejecutar(&["termux-clipboard-set", "\0"], Some(b"x")),
        Err(FalloOrden::Io(_))
    ));
    assert_eq!(nueva.api.estado(), EstadoApi::SinComprobar);

    // Ya apagada: no cambia el motivo ni dice que acabe de apagarse.
    let apagada = Escenario::nuevo();
    apagada.api.apagar(EstadoApi::Ausente);
    let (resultado, recien) = apagada
        .holgadas()
        .ejecutar_avisando(&["termux-notification", "a\0"], None);
    assert!(matches!(
        resultado,
        Err(FalloOrden::Io(_)) | Err(FalloOrden::ApiApagada)
    ));
    assert!(!recien);
    assert_eq!(apagada.api.estado(), EstadoApi::Ausente);
}

/// Un programa de la API que existe y pasa por ejecutable (tiene algún bit
/// `x`) pero el sistema no deja lanzar: con `x` solo para el grupo y los demás,
/// el dueño (quien corre la prueba) no puede ejecutarlo y el `exec` da EACCES.
/// Es un fallo local, no de Termux:API.
#[test]
fn un_programa_que_no_se_puede_lanzar_es_un_fallo_de_io_y_no_apaga_la_api() {
    if nix::unistd::Uid::current().is_root() {
        // root ejecuta cualquier fichero con algún bit `x`: no hay forma de
        // provocar el EACCES.
        eprintln!("aviso: se omite como root (no puede provocar el fallo de lanzamiento)");
        return;
    }
    let escenario = Escenario::nuevo();
    let programa = escenario.dobles.bin().join("termux-notification");
    std::fs::set_permissions(&programa, std::fs::Permissions::from_mode(0o611)).unwrap();
    let ordenes = escenario.holgadas();

    let (resultado, recien) = ordenes.ejecutar_avisando(&["termux-notification", "-t", "x"], None);
    assert!(matches!(resultado, Err(FalloOrden::Io(_))), "{resultado:?}");
    assert!(!recien, "un fallo al lanzar no avisa de «no responde»");
    assert_eq!(escenario.api.estado(), EstadoApi::SinComprobar);

    // Arreglado el permiso, la API sigue disponible: no se apagó para siempre.
    std::fs::set_permissions(&programa, std::fs::Permissions::from_mode(0o755)).unwrap();
    ordenes
        .ejecutar(&["termux-notification", "-t", "x"], None)
        .expect("la API sigue disponible");
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);
}

// ------------------------------------------------------------- entrada

#[test]
fn una_entrada_de_512_kib_llega_entera() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.holgadas();
    // Todos los valores de byte, NUL y saltos de línea incluidos.
    let entrada: Vec<u8> = (0..512 * 1024u32).map(|i| (i % 251) as u8).collect();
    ordenes
        .ejecutar(&["termux-clipboard-set"], Some(&entrada))
        .expect("termux-clipboard-set termina bien");
    let llamadas = escenario.dobles.llamadas("termux-clipboard-set");
    assert_eq!(llamadas.len(), 1);
    assert_eq!(llamadas[0].entrada.len(), entrada.len());
    assert!(llamadas[0].entrada == entrada, "la entrada llegó distinta");
}

#[test]
fn una_entrada_vacia_cierra_la_entrada_estandar() {
    let escenario = Escenario::nuevo();
    escenario
        .holgadas()
        .ejecutar(&["termux-clipboard-set"], Some(b""))
        .expect("el fin de la entrada llega aunque no haya nada que escribir");
    assert_eq!(escenario.dobles.llamadas("termux-clipboard-set").len(), 1);
}

#[tokio::test]
async fn ejecutar_async_usa_el_mismo_camino() {
    let escenario = Escenario::nuevo();
    let ordenes = escenario.ordenes(Duration::from_secs(10));
    ordenes
        .ejecutar_async(
            vec!["termux-clipboard-set".to_string()],
            Some(b"texto copiado".to_vec()),
        )
        .await
        .expect("copiar");
    let llamadas = escenario.dobles.llamadas("termux-clipboard-set");
    assert_eq!(llamadas[0].entrada, b"texto copiado");
    assert_eq!(escenario.api.estado(), EstadoApi::Disponible);

    escenario
        .dobles
        .programar("termux-battery-status", Modo::Fallar);
    let (resultado, recien) = ordenes
        .ejecutar_async_avisando(vec!["termux-battery-status".to_string()], None)
        .await;
    assert!(matches!(resultado, Err(FalloOrden::Codigo { .. })));
    assert!(recien);
    assert_eq!(
        ordenes
            .ejecutar_async(vec!["termux-notification".to_string()], None)
            .await,
        Err(FalloOrden::ApiApagada)
    );
}

#[test]
fn el_path_es_el_inyectado_y_no_el_del_proceso() {
    let escenario = Escenario::nuevo();
    // Un PATH sin los dobles: nada se encuentra, aunque estén en el real.
    let mut entorno = escenario.entorno.clone();
    entorno.path = Some("/usr/bin:/bin".into());
    let ordenes = Ordenes::nuevas(&entorno, Duration::from_secs(5), Arc::clone(&escenario.api));
    assert_eq!(
        ordenes.ejecutar(&["termux-open-url", "x"], None),
        Err(FalloOrden::NoInstalada("termux-open-url".into()))
    );
    // Y sin PATH tampoco.
    entorno.path = None;
    let ordenes = Ordenes::nuevas(&entorno, Duration::from_secs(5), Arc::clone(&escenario.api));
    assert_eq!(
        ordenes.ejecutar(&["termux-open-url", "x"], None),
        Err(FalloOrden::NoInstalada("termux-open-url".into()))
    );
    assert!(escenario.dobles.llamadas("termux-open-url").is_empty());
}

#[test]
fn un_fichero_sin_permiso_de_ejecucion_no_cuenta_como_instalado() {
    let escenario = Escenario::nuevo();
    let programa = escenario.dobles.bin().join("termux-open-url");
    std::fs::set_permissions(&programa, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        escenario
            .holgadas()
            .ejecutar(&["termux-open-url", "x"], None),
        Err(FalloOrden::NoInstalada("termux-open-url".into()))
    );
}

// --------------------------------------------------------------- rutas

const HOME_TERMUX: &str = "/data/data/com.termux/files/home";
const PREFIX_TERMUX: &str = "/data/data/com.termux/files/usr";
const TMPDIR_TERMUX: &str = "/data/data/com.termux/files/usr/tmp";

fn entorno_de_termux(uid: u32) -> EntornoTermux {
    EntornoTermux {
        home: Some(HOME_TERMUX.into()),
        prefix: Some(PREFIX_TERMUX.into()),
        tmpdir: Some(TMPDIR_TERMUX.into()),
        uid,
        ..EntornoTermux::default()
    }
}

#[test]
fn las_rutas_de_termux_salen_del_entorno_y_no_del_uid() {
    let rutas = calcular(&entorno_de_termux(10123)).unwrap();
    assert_eq!(rutas.hogar, Path::new(HOME_TERMUX));
    assert_eq!(
        rutas.datos,
        Path::new(HOME_TERMUX).join(".local/share/magi")
    );
    assert_eq!(rutas.config, Path::new(HOME_TERMUX).join(".config/magi"));
    assert_eq!(
        rutas.estado,
        Path::new(HOME_TERMUX).join(".local/state/magi")
    );
    assert_eq!(rutas.runtime, Path::new(TMPDIR_TERMUX).join("magi-10123"));
    assert_eq!(rutas.sh, Path::new(PREFIX_TERMUX).join("bin/sh"));

    // Otro uid solo cambia el directorio del socket.
    let otro = calcular(&entorno_de_termux(10456)).unwrap();
    assert_eq!(otro.runtime, Path::new(TMPDIR_TERMUX).join("magi-10456"));
    assert_eq!(otro.datos, rutas.datos);
    assert_eq!(otro.hogar, rutas.hogar);
}

#[test]
fn xdg_runtime_dir_puesta_se_respeta_y_vacia_cuenta_como_sin_poner() {
    let mut entorno = entorno_de_termux(10123);
    entorno.xdg_runtime_dir = Some("/data/data/com.termux/files/usr/var/run".into());
    entorno.xdg_data_home = Some("/storage/datos".into());
    entorno.xdg_config_home = Some("/storage/config".into());
    entorno.xdg_state_home = Some("/storage/estado".into());
    let rutas = calcular(&entorno).unwrap();
    assert_eq!(
        rutas.runtime,
        Path::new("/data/data/com.termux/files/usr/var/run/magi")
    );
    assert_eq!(rutas.datos, Path::new("/storage/datos/magi"));
    assert_eq!(rutas.config, Path::new("/storage/config/magi"));
    assert_eq!(rutas.estado, Path::new("/storage/estado/magi"));
    assert_eq!(rutas.hogar, Path::new(HOME_TERMUX), "el home no cambia");

    // Una variable vacía del entorno real es como no ponerla. (TMPDIR y PATH
    // no se tocan: las usan las demás pruebas a la vez; se comparan tal cual.)
    let _guarda = GuardaEntorno::nuevo(&[
        ("HOME", Some(HOME_TERMUX)),
        ("PREFIX", Some(PREFIX_TERMUX)),
        ("XDG_RUNTIME_DIR", Some("")),
        ("XDG_DATA_HOME", Some("")),
        ("XDG_CONFIG_HOME", None),
        ("XDG_STATE_HOME", Some("/storage/estado")),
    ]);
    let del_proceso = EntornoTermux::del_proceso();
    assert_eq!(del_proceso.home.as_deref(), Some(Path::new(HOME_TERMUX)));
    assert_eq!(
        del_proceso.prefix.as_deref(),
        Some(Path::new(PREFIX_TERMUX))
    );
    assert_eq!(del_proceso.xdg_runtime_dir, None);
    assert_eq!(del_proceso.xdg_data_home, None);
    assert_eq!(del_proceso.xdg_config_home, None);
    assert_eq!(
        del_proceso.xdg_state_home.as_deref(),
        Some(Path::new("/storage/estado"))
    );
    assert_eq!(
        del_proceso.tmpdir,
        std::env::var_os("TMPDIR")
            .filter(|valor| !valor.is_empty())
            .map(PathBuf::from)
    );
    assert_eq!(
        del_proceso.path,
        std::env::var_os("PATH").filter(|valor| !valor.is_empty())
    );
    assert_eq!(del_proceso.uid, nix::unistd::Uid::current().as_raw());
    // Sin XDG_RUNTIME_DIR el socket va en el directorio temporal, con el uid.
    let rutas = calcular(&del_proceso).unwrap();
    let tmp = del_proceso
        .tmpdir
        .clone()
        .unwrap_or_else(|| Path::new(PREFIX_TERMUX).join("tmp"));
    assert_eq!(rutas.runtime, tmp.join(format!("magi-{}", del_proceso.uid)));
    assert_eq!(rutas.estado, Path::new("/storage/estado/magi"));
}

#[test]
fn sin_home_o_sin_prefix_el_error_dice_que_hacer() {
    let mut sin_home = entorno_de_termux(10123);
    sin_home.home = None;
    let error = calcular(&sin_home).unwrap_err();
    assert!(error.contains("HOME"), "{error}");
    assert!(error.contains("exporta"), "dice cómo arreglarlo: {error}");

    let mut sin_prefix = entorno_de_termux(10123);
    sin_prefix.prefix = None;
    let error = calcular(&sin_prefix).unwrap_err();
    assert!(error.contains("PREFIX"), "{error}");

    // `descubrir_rutas` lo envuelve con contexto, sin perder la instrucción.
    let error = magi::config::rutas_de_termux(&sin_prefix).unwrap_err();
    let texto = format!("{error:#}");
    assert!(
        texto.contains("Termux") && texto.contains("PREFIX"),
        "{texto}"
    );
}

#[test]
fn sin_tmpdir_el_socket_cae_en_el_tmp_del_prefijo() {
    let mut entorno = entorno_de_termux(10123);
    entorno.tmpdir = None;
    let rutas = calcular(&entorno).unwrap();
    assert_eq!(
        rutas.runtime,
        Path::new(PREFIX_TERMUX).join("tmp/magi-10123")
    );
}

#[test]
fn el_socket_con_el_tmpdir_de_termux_queda_muy_por_debajo_de_100_bytes() {
    // Los uids de Android van de 10000 (apps) hasta más de un millón (otros
    // perfiles de usuario).
    for uid in [0, 10123, 99999, 1_010_123] {
        let rutas = magi::config::rutas_de_termux(&entorno_de_termux(uid)).unwrap();
        let socket = magi::servidor::ruta_socket(&rutas);
        let largo = socket.as_os_str().len();
        assert!(
            largo < 100,
            "uid {uid}: {largo} bytes: {}",
            socket.display()
        );
        assert_eq!(
            socket,
            Path::new(TMPDIR_TERMUX).join(format!("magi-{uid}/servidor.sock"))
        );
    }
}

#[tokio::test]
async fn el_servidor_arranca_con_las_rutas_de_termux_y_crea_el_directorio_700() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path();
    // Un Termux de mentira: `$HOME`, `$PREFIX` y `$TMPDIR` bajo el temporal,
    // sin XDG_RUNTIME_DIR ni /tmp.
    let entorno = EntornoTermux {
        home: Some(raiz.join("home")),
        prefix: Some(raiz.join("usr")),
        tmpdir: Some(raiz.join("usr/tmp")),
        uid: nix::unistd::Uid::current().as_raw(),
        ..EntornoTermux::default()
    };
    let rutas = magi::config::rutas_de_termux(&entorno).expect("rutas de Termux");
    assert_eq!(
        rutas.runtime,
        raiz.join(format!("usr/tmp/magi-{}", entorno.uid))
    );
    assert!(!rutas.runtime.exists(), "el servidor lo crea");

    let mut config = magi::config::Config::default();
    config.servidor.gracia_apagado_seg = 1;
    let tarea = tokio::spawn(magi::servidor::arrancar(rutas.clone(), config));
    let socket = magi::servidor::ruta_socket(&rutas);
    let mut atendiendo = false;
    for _ in 0..150 {
        if tokio::net::UnixStream::connect(&socket).await.is_ok() {
            atendiendo = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(atendiendo, "el socket no apareció en {}", socket.display());
    let modo = std::fs::metadata(&rutas.runtime)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(modo & 0o777, 0o700, "el directorio del socket es 700");
    assert_eq!(socket.parent(), Some(rutas.runtime.as_path()));
    tarea.abort();
}

// ------------------------------------------------- propiedades de Android

#[test]
fn el_nombre_del_equipo_sale_del_modelo_de_las_propiedades() {
    let propiedades = Propiedades::fijas(&[
        ("ro.product.model", "ABC-X10"),
        ("ro.build.version.release", "15"),
        ("ro.build.version.sdk", "35"),
    ]);
    let modelo = propiedades.modelo().expect("hay modelo");
    assert_eq!(magi::termux::nombre_equipo(&modelo), "termux-abc-x10");
    assert_eq!(propiedades.version_android().as_deref(), Some("15"));
    assert_eq!(propiedades.sdk(), Some(35));

    // Sin modelo, «termux» a secas.
    let sin_modelo = Propiedades::fijas(&[]);
    assert_eq!(sin_modelo.modelo(), None);
    assert_eq!(
        magi::termux::nombre_equipo(&sin_modelo.modelo().unwrap_or_default()),
        "termux"
    );
    // Y 20 caracteres como mucho.
    assert!(magi::termux::nombre_equipo("Un Modelo Larguisimo Con Muchas Palabras").len() <= 20);
}

#[cfg(not(target_os = "android"))]
#[test]
fn fuera_de_android_las_propiedades_del_sistema_estan_vacias_y_no_se_lanza_nada() {
    let propiedades = Propiedades::del_sistema();
    assert_eq!(propiedades.modelo(), None);
    assert_eq!(propiedades.version_android(), None);
    assert_eq!(propiedades.sdk(), None);
    assert_eq!(propiedades.leer("ro.product.model"), None);
}

// ------------------------------------------------------- usuario local

fn usuario_del_uid() -> String {
    nix::unistd::User::from_uid(nix::unistd::Uid::current())
        .ok()
        .flatten()
        .map(|usuario| usuario.name)
        .filter(|nombre| !nombre.is_empty())
        .unwrap_or_else(|| "root".to_string())
}

// Lo que pasa depende de la plataforma del proceso (`es_termux()` se decide una
// vez, con `MAGI_PLATAFORMA=termux` en las compilaciones de depuración): cada
// prueba comprueba la regla de la que le toca, así que valen en los dos modos.
// La lógica de las dos ramas, con valores dados, se prueba aparte en
// `conexion::pruebas`.

#[test]
fn usuario_local_prefiere_user_y_luego_username() {
    let _guarda = GuardaEntorno::nuevo(&[("USER", Some("usuario")), ("USERNAME", Some("otro"))]);
    assert_eq!(magi::conexion::usuario_local(), "usuario");

    std::env::remove_var("USER");
    assert_eq!(magi::conexion::usuario_local(), "otro");
}

#[test]
fn usuario_local_con_las_variables_vacias_sigue_la_regla_de_su_plataforma() {
    let _guarda = GuardaEntorno::nuevo(&[("USER", Some("")), ("USERNAME", Some(""))]);
    if magi::termux::es_termux() {
        // Termux: una variable vacía es como no ponerla.
        assert_eq!(magi::conexion::usuario_local(), usuario_del_uid());
    } else {
        // Escritorio de siempre: la primera variable puesta, aunque esté vacía.
        assert_eq!(magi::conexion::usuario_local(), "");
    }
}

#[test]
fn usuario_local_sin_user_ni_username_sigue_la_regla_de_su_plataforma() {
    let _guarda = GuardaEntorno::nuevo(&[("USER", None), ("USERNAME", None)]);
    let usuario = magi::conexion::usuario_local();
    if magi::termux::es_termux() {
        // Termux no define USER: el nombre sale del uid (u0_a123 en Android).
        assert!(!usuario.is_empty());
        assert_eq!(usuario, usuario_del_uid());
    } else {
        // Escritorio de siempre: «root», sin preguntar al sistema.
        assert_eq!(usuario, "root");
    }
}

// -------------------------------------- el escritorio de Linux, sin cambios

#[cfg(target_os = "linux")]
#[test]
fn descubrir_rutas_en_linux_sigue_siendo_la_de_siempre() {
    // Con `MAGI_PLATAFORMA=termux` (como se repiten estas pruebas) el proceso
    // se porta como Termux y esta rama de Linux no existe: se omite con un
    // aviso, no falla. Las rutas de Termux las prueba `termux_plataforma`.
    if magi::termux::es_termux() {
        eprintln!("aviso: se omite: este proceso se porta como Termux (MAGI_PLATAFORMA=termux)");
        return;
    }
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path();
    let datos = raiz.join("datos");
    let config = raiz.join("config");
    let estado = raiz.join("estado");
    let runtime = raiz.join("run");
    let home = raiz.join("home");
    let texto = |ruta: &PathBuf| ruta.to_string_lossy().into_owned();
    {
        let _guarda = GuardaEntorno::nuevo(&[
            ("HOME", Some(&texto(&home))),
            ("XDG_DATA_HOME", Some(&texto(&datos))),
            ("XDG_CONFIG_HOME", Some(&texto(&config))),
            ("XDG_STATE_HOME", Some(&texto(&estado))),
            ("XDG_RUNTIME_DIR", Some(&texto(&runtime))),
            ("PREFIX", Some("/no/debe/usarse")),
        ]);
        let rutas = magi::config::descubrir_rutas().expect("rutas de Linux");
        assert_eq!(rutas.datos, datos.join("magi"));
        assert_eq!(rutas.config, config.join("magi"));
        assert_eq!(rutas.estado, estado.join("magi"));
        assert_eq!(rutas.hogar, home);
        assert_eq!(rutas.runtime, runtime.join("magi"));
    }
    {
        // Sin XDG_RUNTIME_DIR, /tmp/magi-<uid>, como siempre.
        let _guarda = GuardaEntorno::nuevo(&[
            ("HOME", Some(&texto(&home))),
            ("XDG_DATA_HOME", Some(&texto(&datos))),
            ("XDG_CONFIG_HOME", Some(&texto(&config))),
            ("XDG_STATE_HOME", Some(&texto(&estado))),
            ("XDG_RUNTIME_DIR", None),
        ]);
        let rutas = magi::config::descubrir_rutas().expect("rutas de Linux");
        let uid = nix::unistd::Uid::current().as_raw();
        assert_eq!(rutas.runtime, Path::new("/tmp").join(format!("magi-{uid}")));
    }
    assert_eq!(magi::config::plataforma_actual(), "linux");
    assert_ne!(magi::config::nombre_actual(), "termux");
    assert!(!magi::config::nombre_actual().is_empty());
}

// ------------------------------------------- la shell de la deliberación

#[test]
fn la_shell_local_sale_de_termux_y_solo_existe_en_termux() {
    // La shell del comando local es la del prefijo (en Termux no hay /bin/sh).
    assert_eq!(
        magi::termux::shell_de(&entorno_de_termux(10123)),
        Ok(Path::new(PREFIX_TERMUX).join("bin/sh"))
    );
    // Sin `$PREFIX` no se inventa una: el error dice qué hacer.
    let mut sin_prefix = entorno_de_termux(10123);
    sin_prefix.prefix = None;
    let error = magi::termux::shell_de(&sin_prefix).unwrap_err();
    assert!(error.contains("PREFIX"), "{error}");
    // Y fuera de Termux el comando local sigue siendo el `sh` del núcleo.
    assert_eq!(
        magi::termux::shell_local().is_some(),
        magi::termux::es_termux()
    );
}

mod shell {
    use super::*;
    use magi::deliberacion::tests::ejecutar_con as ejecutar_con_shell;

    fn limite(segundos: u64) -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_secs(segundos)
    }

    /// Un `sh` que es un enlace al del sistema, con un nombre propio: dentro de
    /// `sh -c`, `$0` es el programa tal como se lanzó, así que el comando puede
    /// decir qué shell le ha tocado sin que haya que escribir un ejecutable
    /// nuevo (que un fork de otra prueba podría dejar «ocupado»).
    fn sh_propio(raiz: &Path) -> PathBuf {
        let sh = raiz.join("sh-de-termux");
        std::os::unix::fs::symlink("/bin/sh", &sh).unwrap();
        sh
    }

    #[tokio::test]
    async fn el_comando_local_corre_con_la_shell_que_se_le_da() {
        let temporal = tempfile::tempdir().unwrap();
        let sh = sh_propio(temporal.path());
        let marca = temporal.path().join("shell-usada");
        let comando = format!("printf '%s' \"$0\" > '{}'", marca.display());

        let (aprueba, texto) = ejecutar_con_shell(&sh, &comando, limite(5), temporal.path()).await;
        assert!(aprueba, "{texto}");
        assert_eq!(texto, "verde");
        assert_eq!(
            std::fs::read_to_string(&marca).unwrap(),
            sh.to_str().unwrap()
        );

        let (aprueba, texto) = ejecutar_con_shell(
            &sh,
            "echo '\x1b[31mtests en rojo\x1b[0m' >&2; exit 3",
            limite(5),
            temporal.path(),
        )
        .await;
        assert!(!aprueba);
        assert_eq!(texto, "código 3: tests en rojo");
    }

    #[tokio::test]
    async fn una_shell_que_no_existe_rechaza_con_un_motivo() {
        let temporal = tempfile::tempdir().unwrap();
        let sh = temporal.path().join("usr/bin/sh");
        let (aprueba, texto) = ejecutar_con_shell(&sh, "true", limite(5), temporal.path()).await;
        assert!(!aprueba);
        assert!(texto.starts_with("no se pudo lanzar sh"), "{texto}");
    }

    #[tokio::test]
    async fn al_vencer_el_plazo_se_mata_el_grupo_entero() {
        let temporal = tempfile::tempdir().unwrap();
        let sh = sh_propio(temporal.path());
        let marca = temporal.path().join("pids");
        let comando = format!("sleep 30 & echo $$ $! > '{}'; wait", marca.display());
        let inicio = std::time::Instant::now();
        let (aprueba, texto) = ejecutar_con_shell(
            &sh,
            &comando,
            tokio::time::Instant::now() + Duration::from_millis(1500),
            temporal.path(),
        )
        .await;
        assert!(!aprueba);
        assert_eq!(texto, "plazo vencido");
        assert!(inicio.elapsed() < Duration::from_secs(4));
        let anotados = std::fs::read_to_string(&marca).unwrap();
        for pid in anotados.split_whitespace() {
            let pid: i32 = pid.parse().unwrap();
            assert!(
                !sigue_vivo_tras(pid, Duration::from_secs(3)),
                "el proceso {pid} sigue vivo"
            );
        }
    }

    #[tokio::test]
    async fn el_sh_del_prefijo_de_los_dobles_sirve_como_shell() {
        let temporal = tempfile::tempdir().unwrap();
        let dobles = Dobles::nuevos(temporal.path());
        let sh = dobles.bin().join("sh");
        let (aprueba, texto) =
            ejecutar_con_shell(&sh, "test -d .", limite(5), temporal.path()).await;
        assert!(aprueba, "{texto}");
    }
}
