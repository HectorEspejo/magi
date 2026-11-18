//! Pruebas de `magi diagnostico` con un entorno simulado.
//!
//! La recogida real del sistema se prueba aparte (no crea nada); aquí se
//! construyen los hechos a mano para simular un sistema sin portapapeles, una
//! base de datos futura, un locale ASCII o un `NO_COLOR`, sin depender
//! de la máquina que ejecuta los tests.
//!
//! Las de Termux (Fase 17) construyen un `DatosTermux` a mano y, para
//! `observar`, usan los dobles de `termux-*` de `tests/fijos/termux/`: ni
//! Termux ni red. Las del binario real con `MAGI_PLATAFORMA=termux` solo valen
//! en depuración, que es donde esa variable activa algo.

#[path = "fijos/termux/mod.rs"]
mod dobles;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use magi::config::{Config, Rutas};
use magi::diagnostico::{
    comprobar, ejecutar_en, recoger, AgenteObservado, Datos, Directorios, EquipoObservado, Estado,
    EstadoDirectorio, ResultadoBaseDatos, ResultadoConfig, ResultadoEquipos, ServidorObservado,
    SincronizacionObservada,
};
use magi::termux::diagnostico::{
    observar, observar_interrumpible, segundo_plano, AlmacenamientoObservado, ApiObservada,
    DatosTermux, LlaveroObservado,
};
use magi::termux::teclas::{estado_fila, fila, ruta_properties, EstadoFila};
use magi::termux::Propiedades;

use dobles::{sigue_vivo_tras, Dobles, Modo};

/// Rutas de MAGI en un directorio temporal, sin crear nada.
fn rutas_de_prueba(raiz: &Path) -> Rutas {
    Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    }
}

fn directorio(ruta: PathBuf, existe: bool) -> EstadoDirectorio {
    EstadoDirectorio {
        ruta,
        existe,
        es_directorio: existe,
        permisos: if existe { Some(0o700) } else { None },
        escribible: true,
    }
}

/// Un sistema sin problemas: todas las comprobaciones correctas.
fn datos_sanos(rutas: Rutas) -> Datos {
    let mut binarios = BTreeMap::new();
    for programa in [
        "wl-copy",
        "pbcopy",
        "secret-tool",
        "security",
        "nvim",
        "vi",
        "less",
        "more",
        "alacritty",
        "open",
        "ssh-add",
        // Fase 12: un Chromium para el visor de escritorios.
        "chromium",
        "xdg-open",
    ] {
        binarios.insert(programa.to_string(), true);
    }
    let mut variables = BTreeMap::new();
    variables.insert(
        "SSH_AUTH_SOCK".to_string(),
        "/run/user/1000/ssh-agent.sock".to_string(),
    );
    Datos {
        version_app: magi::VERSION_APP.to_string(),
        protocolo: magi::protocolo::VERSION_PROTOCOLO,
        commit: "abc1234".to_string(),
        directorios: Directorios {
            datos: directorio(rutas.datos.clone(), true),
            configuracion: directorio(rutas.config.clone(), true),
            estado: directorio(rutas.estado.clone(), true),
            runtime: directorio(rutas.dir_runtime(), true),
        },
        config: ResultadoConfig::Valida(Box::default()),
        base_datos: ResultadoBaseDatos::NoExiste,
        sincronizacion: SincronizacionObservada::Desactivada,
        servidor: ServidorObservado::Apagado,
        agente: AgenteObservado::Claves(2),
        binarios,
        variables,
        incluye_magi_config: true,
        ascii: false,
        termux: None,
        rutas,
    }
}

fn texto_de(datos: &Datos, es_terminal: bool) -> (i32, String) {
    let mut salida = Vec::new();
    let codigo = ejecutar_en(datos, es_terminal, &mut salida);
    (
        codigo,
        String::from_utf8(salida).expect("la salida del diagnóstico es UTF-8"),
    )
}

#[test]
fn un_sistema_sano_no_tiene_avisos_ni_errores() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let datos = datos_sanos(rutas_de_prueba(temporal.path()));
    let informe = comprobar(&datos);
    assert_eq!(
        (informe.avisos, informe.errores),
        (0, 0),
        "{:#?}",
        informe.comprobaciones
    );
    let nombres: Vec<&str> = informe
        .comprobaciones
        .iter()
        .map(|comprobacion| comprobacion.nombre)
        .collect();
    assert_eq!(
        nombres,
        [
            "datos",
            "configuración",
            "estado",
            "runtime",
            "servidor",
            "agente SSH",
            "llavero",
            "portapapeles",
            "navegador",
            "editor",
            "paginador",
            "terminal",
            "ssh_config",
            "sincronización",
        ]
    );
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    assert!(texto.contains(&format!(
        "MAGI {} · protocolo {} · commit abc1234",
        magi::VERSION_APP,
        magi::protocolo::VERSION_PROTOCOLO
    )));
    assert!(texto.contains("0 avisos, 0 errores"));
}

#[test]
fn sin_portapapeles_es_un_aviso_y_sale_con_cero() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    datos.binarios.insert("wl-copy".to_string(), false);
    datos.binarios.insert("pbcopy".to_string(), false);
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0, "un aviso no puede cambiar el código de salida");
    assert!(texto.contains("! portapapeles"), "{texto}");
    assert!(texto.contains("instala wl-clipboard"), "{texto}");
    assert!(texto.contains("1 aviso, 0 errores"), "{texto}");
}

#[test]
fn una_base_de_datos_futura_es_un_error_y_sale_con_uno() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    datos.base_datos = ResultadoBaseDatos::Futura(magi::almacen::migraciones::VERSION_BD + 1);
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 1);
    assert!(texto.contains("✕ datos"), "{texto}");
    assert!(texto.contains("actualiza MAGI"), "{texto}");
    assert!(texto.contains("0 avisos, 1 error"), "{texto}");
}

#[test]
fn un_config_toml_roto_es_un_error() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    datos.config = ResultadoConfig::Invalida("TOML parse error at line 2".to_string());
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 1);
    assert!(texto.contains("✕ configuración"), "{texto}");
    assert!(texto.contains("config.toml no válido"), "{texto}");
}

#[test]
fn no_color_y_la_salida_no_terminal_quitan_el_color() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let datos = datos_sanos(rutas_de_prueba(temporal.path()));
    // Terminal y sin NO_COLOR: hay color.
    let (_, texto) = texto_de(&datos, true);
    assert!(
        texto.contains("\x1b[38;2;"),
        "falta el color en una terminal"
    );
    // NO_COLOR lo desactiva aunque la salida sea una terminal.
    let mut con_no_color = datos.clone();
    con_no_color
        .variables
        .insert("NO_COLOR".to_string(), String::new());
    let (_, texto) = texto_de(&con_no_color, true);
    assert!(!texto.contains('\x1b'), "NO_COLOR no desactiva el color");
    // Salida redirigida: tampoco hay color.
    let (_, texto) = texto_de(&datos, false);
    assert!(!texto.contains('\x1b'), "la salida redirigida lleva color");
}

#[test]
fn el_modo_ascii_cambia_los_signos_y_no_deja_glifos() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    datos.ascii = true;
    datos.binarios.insert("wl-copy".to_string(), false);
    datos.binarios.insert("pbcopy".to_string(), false);
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    assert!(texto.contains("ok datos"), "{texto}");
    assert!(texto.contains("! portapapeles"), "{texto}");
    for glifo in ['✓', '✕', '·', '→'] {
        assert!(!texto.contains(glifo), "queda el glifo «{glifo}»:\n{texto}");
    }
    assert!(
        texto.contains(&format!(
            "MAGI {} - protocolo {} - commit abc1234",
            magi::VERSION_APP,
            magi::protocolo::VERSION_PROTOCOLO
        )),
        "{texto}"
    );
    // Un error también degrada su signo.
    datos.base_datos = ResultadoBaseDatos::Futura(magi::almacen::migraciones::VERSION_BD + 1);
    let (_, texto) = texto_de(&datos, false);
    assert!(texto.contains("x datos"), "{texto}");
}

#[test]
fn el_diagnostico_no_crea_ni_escribe_nada() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path();
    let rutas = rutas_de_prueba(raiz);
    assert_eq!(
        std::fs::read_dir(raiz)
            .expect("leyendo el temporal")
            .count(),
        0,
        "el temporal debe empezar vacío"
    );
    // La recogida real y la impresión no pueden crear ni escribir ficheros.
    let datos = recoger(rutas.clone());
    let mut salida = Vec::new();
    ejecutar_en(&datos, false, &mut salida);
    assert!(!salida.is_empty());
    assert!(!rutas.datos.exists(), "se ha creado el directorio de datos");
    assert!(
        !rutas.config.exists(),
        "se ha creado el directorio de configuración"
    );
    assert!(
        !rutas.estado.exists(),
        "se ha creado el directorio de estado"
    );
    assert!(
        !rutas.runtime.exists(),
        "se ha creado el directorio de ejecución"
    );
    assert_eq!(
        std::fs::read_dir(raiz)
            .expect("leyendo el temporal")
            .count(),
        0,
        "el diagnóstico ha creado ficheros"
    );
}

#[test]
fn la_longitud_del_socket_tiene_aviso_y_error() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    // «/tmp/» + 81 = 86 bytes de runtime + «/servidor.sock» = 100 bytes: aviso.
    datos.rutas.runtime = PathBuf::from(format!("/tmp/{}", "a".repeat(81)));
    datos.directorios.runtime = directorio(datos.rutas.dir_runtime(), true);
    let informe = comprobar(&datos);
    let runtime = informe
        .comprobaciones
        .iter()
        .find(|comprobacion| comprobacion.nombre == "runtime")
        .expect("falta la comprobación de runtime");
    assert_eq!(runtime.estado, Estado::Aviso, "{}", runtime.detalle);
    assert!(
        runtime.detalle.contains("socket 100 bytes"),
        "{}",
        runtime.detalle
    );
    assert_eq!(informe.codigo_salida(), 0);
    // «/tmp/» + 89 = 94 bytes de runtime + «/servidor.sock» = 108 bytes: error.
    datos.rutas.runtime = PathBuf::from(format!("/tmp/{}", "a".repeat(89)));
    datos.directorios.runtime = directorio(datos.rutas.dir_runtime(), true);
    let informe = comprobar(&datos);
    let runtime = informe
        .comprobaciones
        .iter()
        .find(|comprobacion| comprobacion.nombre == "runtime")
        .expect("falta la comprobación de runtime");
    assert_eq!(runtime.estado, Estado::Error, "{}", runtime.detalle);
    assert!(
        runtime.detalle.contains("socket 108 bytes"),
        "{}",
        runtime.detalle
    );
    assert_eq!(informe.codigo_salida(), 1);
}

#[test]
fn el_servidor_se_resume_sin_arrancarlo() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    datos.servidor = ServidorObservado::EnMarcha {
        version_app: "1.0.0".to_string(),
        protocolo: magi::protocolo::VERSION_PROTOCOLO,
        sesiones: 2,
        transferencias: 0,
        tuneles: 1,
        ejecuciones: 0,
    };
    let informe = comprobar(&datos);
    let servidor = informe
        .comprobaciones
        .iter()
        .find(|comprobacion| comprobacion.nombre == "servidor")
        .expect("falta la comprobación de servidor");
    assert_eq!(servidor.estado, Estado::Correcto, "{}", servidor.detalle);
    assert!(
        servidor.detalle.contains(&format!(
            "en marcha · 1.0.0 · protocolo {} · 2 sesiones, 1 túnel",
            magi::protocolo::VERSION_PROTOCOLO
        )),
        "{}",
        servidor.detalle
    );
    datos.servidor = ServidorObservado::OtraVersion {
        version_app: Some("0.9.0".to_string()),
        protocolo: 5,
    };
    let informe = comprobar(&datos);
    let servidor = informe
        .comprobaciones
        .iter()
        .find(|comprobacion| comprobacion.nombre == "servidor")
        .expect("falta la comprobación de servidor");
    assert_eq!(servidor.estado, Estado::Aviso, "{}", servidor.detalle);
}

#[test]
fn los_directorios_que_faltan_se_crearan_al_arrancar() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path();
    // El padre de los directorios de MAGI tampoco existe: se podrá crear
    // porque el ancestro más cercano (el temporal) es escribible.
    let rutas = Rutas {
        datos: raiz.join("a/b/datos"),
        config: raiz.join("a/b/config"),
        estado: raiz.join("a/b/estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("a/b/runtime"),
    };
    let datos = recoger(rutas.clone());
    assert!(datos.directorios.datos.escribible);
    assert!(datos.directorios.configuracion.escribible);
    let informe = comprobar(&datos);
    for nombre in ["datos", "configuración", "estado", "runtime"] {
        let comprobacion = informe
            .comprobaciones
            .iter()
            .find(|comprobacion| comprobacion.nombre == nombre)
            .expect("falta la comprobación del directorio");
        assert_eq!(
            comprobacion.estado,
            Estado::Correcto,
            "{nombre}: {}",
            comprobacion.detalle
        );
    }
    // Nada de eso se ha creado: el diagnóstico solo lo comprueba.
    assert!(!raiz.join("a").exists());
}

#[test]
fn una_base_de_datos_futura_en_disco_se_detecta_sin_tocarla() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let rutas = rutas_de_prueba(temporal.path());
    std::fs::create_dir_all(&rutas.datos).expect("creando el directorio de datos");
    let ruta_bd = rutas.base_datos();
    {
        let conexion = rusqlite::Connection::open(&ruta_bd).expect("creando magi.db");
        conexion
            .pragma_update(None, "user_version", 999_i64)
            .expect("fijando user_version");
    }
    let antes = std::fs::read(&ruta_bd).expect("leyendo magi.db");
    let datos = recoger(rutas.clone());
    assert_eq!(datos.base_datos, ResultadoBaseDatos::Futura(999));
    let informe = comprobar(&datos);
    assert_eq!(informe.codigo_salida(), 1);
    // Abrirla en solo lectura no la modifica ni crea ficheros de WAL.
    assert_eq!(
        std::fs::read(&ruta_bd).expect("releyendo magi.db"),
        antes,
        "el diagnóstico ha modificado magi.db"
    );
    assert!(!rutas.datos.join("magi.db-wal").exists());
    assert!(!rutas.datos.join("magi.db-shm").exists());
}

/// Busca una comprobación por su nombre.
fn comprobacion_de<'a>(
    informe: &'a magi::diagnostico::Informe,
    nombre: &str,
) -> &'a magi::diagnostico::Comprobacion {
    informe
        .comprobaciones
        .iter()
        .find(|comprobacion| comprobacion.nombre == nombre)
        .unwrap_or_else(|| panic!("falta la comprobación «{nombre}»"))
}

#[test]
fn la_sincronizacion_desactivada_es_correcta() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let datos = datos_sanos(rutas_de_prueba(temporal.path()));
    let informe = comprobar(&datos);
    let sincronizacion = comprobacion_de(&informe, "sincronización");
    assert_eq!(
        sincronizacion.estado,
        Estado::Correcto,
        "{}",
        sincronizacion.detalle
    );
    assert!(
        sincronizacion.detalle.contains("desactivada"),
        "{}",
        sincronizacion.detalle
    );
    assert_eq!(informe.codigo_salida(), 0);
}

#[test]
fn una_carpeta_de_sincronizacion_que_no_existe_es_un_error() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    datos.sincronizacion = SincronizacionObservada::Configurada {
        carpeta: EstadoDirectorio {
            ruta: PathBuf::from("/no/existe/magi-sync"),
            existe: false,
            es_directorio: false,
            permisos: None,
            escribible: false,
        },
        equipos: ResultadoEquipos::SinDatos,
        conflictos_pendientes: None,
        copias_conflicto: 0,
    };
    let informe = comprobar(&datos);
    let sincronizacion = comprobacion_de(&informe, "sincronización");
    assert_eq!(sincronizacion.estado, Estado::Error);
    assert!(
        sincronizacion.detalle.contains("no existe"),
        "{}",
        sincronizacion.detalle
    );
    assert_eq!(informe.codigo_salida(), 1);
}

#[test]
fn la_sincronizacion_avisa_de_equipos_ilegibles_conflictos_y_copias() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let mut datos = datos_sanos(rutas_de_prueba(temporal.path()));
    datos.sincronizacion = SincronizacionObservada::Configurada {
        carpeta: directorio(datos.rutas.hogar.join("magi-sync"), true),
        equipos: ResultadoEquipos::Equipos(vec![
            EquipoObservado {
                nombre: "omarchy-portatil".to_string(),
                plataforma: "linux".to_string(),
                estado: "este".to_string(),
                ultimo_fichero_en: None,
            },
            EquipoObservado {
                nombre: "mac-mini".to_string(),
                plataforma: "macos".to_string(),
                estado: "ilegible".to_string(),
                ultimo_fichero_en: Some("2026-11-05T09:12:44+01:00".to_string()),
            },
        ]),
        conflictos_pendientes: Some(2),
        copias_conflicto: 1,
    };
    let informe = comprobar(&datos);
    let sincronizacion = comprobacion_de(&informe, "sincronización");
    assert_eq!(
        sincronizacion.estado,
        Estado::Aviso,
        "{}",
        sincronizacion.detalle
    );
    for fragmento in [
        "2 equipos",
        "mac-mini",
        "ilegible",
        "2 conflictos pendientes",
        "1 copia en conflicto",
    ] {
        assert!(
            sincronizacion.detalle.contains(fragmento),
            "falta «{fragmento}» en: {}",
            sincronizacion.detalle
        );
    }
    assert!(
        sincronizacion
            .arreglo
            .as_deref()
            .unwrap_or_default()
            .contains("clave nueva"),
        "falta la instrucción de la clave nueva: {:?}",
        sincronizacion.arreglo
    );
    // Un aviso no cambia el código de salida.
    assert_eq!(informe.codigo_salida(), 0);
}

#[test]
fn la_sincronizacion_se_observa_sin_leer_la_clave_ni_descifrar() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let rutas = rutas_de_prueba(temporal.path());
    let carpeta = rutas.hogar.join("magi-sync");
    std::fs::create_dir_all(&carpeta).expect("creando la carpeta de sincronización");
    std::fs::create_dir_all(&rutas.config).expect("creando el directorio de configuración");
    std::fs::write(
        rutas.fichero_config(),
        format!("[sincronizacion]\ncarpeta = \"{}\"\n", carpeta.display()),
    )
    .expect("escribiendo config.toml");

    let otro_uuid = uuid::Uuid::new_v4().to_string();
    {
        let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("creando magi.db");
        let conexion = almacen.conexion();
        let id = magi::almacen::equipos::crear(conexion, &otro_uuid, "mac-mini", "macos")
            .expect("creando el equipo ajeno");
        magi::almacen::equipos::fijar_estado(conexion, id, magi::almacen::equipos::ESTADO_ILEGIBLE)
            .expect("marcando el equipo ilegible");
        let _ = magi::almacen::conflictos::crear_si_nuevo(
            conexion,
            &magi::almacen::conflictos::NuevoConflicto {
                tipo: magi::almacen::conflictos::TIPO_ELEMENTO.to_string(),
                elemento_tipo: "host".to_string(),
                uuid: "0f1e2d3c-2222-4b3a-9c1d-abcdef012345".to_string(),
                nombre: "web".to_string(),
                version_perdedora_json: None,
                version_ganadora: Some("1793873500001.6b1e5c2a".to_string()),
                equipo_ganador: None,
            },
        )
        .expect("creando un conflicto pendiente");
        almacen.cerrar().expect("cerrando magi.db");
    }
    // Una copia en conflicto de la herramienta de sincronización: se ignora
    // como fichero, pero el diagnóstico avisa de que existe.
    std::fs::write(
        carpeta.join(format!(
            "magi-{otro_uuid}.sync-conflict-20261105-101200-ABC123.age"
        )),
        b"copia",
    )
    .expect("creando una copia en conflicto");

    let datos = recoger(rutas.clone());
    match &datos.sincronizacion {
        SincronizacionObservada::Configurada {
            carpeta: observada,
            equipos: ResultadoEquipos::Equipos(equipos),
            conflictos_pendientes,
            copias_conflicto,
        } => {
            assert_eq!(observada.ruta, carpeta);
            assert_eq!(equipos.len(), 2, "este equipo y el ajeno");
            assert!(*conflictos_pendientes >= Some(1));
            assert_eq!(*copias_conflicto, 1);
        }
        otra => panic!("la sincronización no se observó: {otra:?}"),
    }
    let informe = comprobar(&datos);
    let sincronizacion = comprobacion_de(&informe, "sincronización");
    assert_eq!(
        sincronizacion.estado,
        Estado::Aviso,
        "{}",
        sincronizacion.detalle
    );
    assert!(
        sincronizacion.detalle.contains("ilegible"),
        "{}",
        sincronizacion.detalle
    );
    assert!(
        sincronizacion.detalle.contains("conflicto"),
        "{}",
        sincronizacion.detalle
    );
}

// ============================================================ Termux (Fase 17)

const HOGAR_TERMUX: &str = "/data/data/com.termux/files/home";
const TMPDIR_TERMUX: &str = "/data/data/com.termux/files/usr/tmp";

/// Lo que se observa en un Termux sin problemas (el de §6.5).
fn termux_sano() -> DatosTermux {
    DatosTermux {
        version_termux: Some("0.118.3".to_string()),
        android_release: Some("15".to_string()),
        android_sdk: Some(35),
        arquitectura: "aarch64".to_string(),
        api: ApiObservada::Responde { ms: 380 },
        almacenamiento: AlmacenamientoObservado::Presente {
            destino: Some("/storage/emulated/0".to_string()),
            accesible: true,
        },
        teclas: EstadoFila::LaDeMagi,
        fichero_teclas: Some(PathBuf::from(HOGAR_TERMUX).join(".termux/termux.properties")),
        wake_lock_tomado: false,
        api_apagada_en_servidor: false,
        llavero: LlaveroObservado::Fichero {
            modificado: NaiveDate::from_ymd_opt(2026, 11, 17),
        },
        ajustes_android_sin_comprobar: true,
    }
}

/// Un sistema de Termux sin problemas, con los hechos a mano.
fn datos_termux() -> Datos {
    let hogar = PathBuf::from(HOGAR_TERMUX);
    let rutas = Rutas {
        datos: hogar.join(".local/share/magi"),
        config: hogar.join(".config/magi"),
        estado: hogar.join(".local/state/magi"),
        runtime: PathBuf::from(TMPDIR_TERMUX).join("magi-10234"),
        hogar,
    };
    let mut datos = datos_sanos(rutas);
    datos.termux = Some(termux_sano());
    datos
        .variables
        .insert("TMPDIR".to_string(), TMPDIR_TERMUX.to_string());
    datos
}

fn con_termux(cambio: impl FnOnce(&mut DatosTermux)) -> Datos {
    let mut datos = datos_termux();
    cambio(datos.termux.as_mut().expect("datos de Termux"));
    datos
}

/// Un servidor en marcha (para el wake lock y los avisos de Android).
fn servidor_en_marcha() -> ServidorObservado {
    ServidorObservado::EnMarcha {
        version_app: "1.6.0".to_string(),
        protocolo: magi::protocolo::VERSION_PROTOCOLO,
        sesiones: 1,
        transferencias: 0,
        tuneles: 0,
        ejecuciones: 0,
    }
}

/// Las líneas del bloque TERMUX de la salida, desde su título hasta la línea
/// en blanco que precede al resumen.
fn bloque_de(texto: &str) -> Vec<&str> {
    let mut lineas = texto.lines().skip_while(|linea| *linea != "  TERMUX");
    assert!(lineas.next().is_some(), "falta el bloque TERMUX:\n{texto}");
    lineas.take_while(|linea| !linea.is_empty()).collect()
}

#[test]
fn fuera_de_termux_el_diagnostico_no_tiene_bloque_ni_lineas_de_termux() {
    let datos = datos_sanos(rutas_de_prueba(Path::new("/home/usuario")));
    let informe = comprobar(&datos);
    assert!(informe.bloques.is_empty());
    assert!(
        informe
            .comprobaciones
            .iter()
            .all(|comprobacion| comprobacion.estado != Estado::Info),
        "en Linux no hay líneas informativas: {:#?}",
        informe.comprobaciones
    );
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    for ajeno in ["TERMUX", "Termux", "segundo plano", "batería", "wake lock"] {
        assert!(!texto.contains(ajeno), "«{ajeno}» en Linux:\n{texto}");
    }
}

/// Fija la salida completa del diagnóstico en Linux: la Fase 17 no cambia ni
/// una línea, ni un nombre, ni el orden, ni el ancho. (En macOS el llavero y
/// el portapapeles son otros programas: solo se fija en Linux.)
#[cfg(target_os = "linux")]
#[test]
fn la_salida_completa_en_linux_es_la_de_siempre() {
    let datos = datos_sanos(rutas_de_prueba(Path::new("/home/usuario")));
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    let cabecera = format!(
        "MAGI {} · protocolo {} · commit abc1234\n\n",
        magi::VERSION_APP,
        magi::protocolo::VERSION_PROTOCOLO
    );
    let lineas = "  ✓ datos           /home/usuario/datos (700) · magi.db se creará al arrancar
  ✓ configuración   /home/usuario/config (700) · config.toml correcto
  ✓ estado          /home/usuario/estado (700) · logs
  ✓ runtime         /home/usuario/runtime (700) · socket 35 bytes
  ✓ servidor        no está en marcha (se arrancará solo)
  ✓ agente SSH      2 claves cargadas
  ✓ llavero         secret-tool
  ✓ portapapeles    wl-copy
  ✓ navegador       auto · chromium --app
  ✓ editor          nvim (por defecto)
  ✓ paginador       less (por defecto)
  ✓ terminal        alacritty ([terminal] comando)
  ✓ ssh_config      ~/.ssh/config incluye magi_config
  ✓ sincronización  desactivada (sin carpeta en config.toml)
";
    assert_eq!(texto, format!("{cabecera}{lineas}\n0 avisos, 0 errores\n"));
}

#[test]
fn las_lineas_informativas_no_cuentan_como_avisos_ni_errores() {
    let datos = datos_termux();
    let informe = comprobar(&datos);
    let informativas = informe
        .comprobaciones
        .iter()
        .filter(|comprobacion| comprobacion.estado == Estado::Info)
        .count();
    assert!(
        informativas >= 3,
        "terminal, segundo plano y batería: {informativas}"
    );
    assert_eq!((informe.avisos, informe.errores), (0, 0));
    assert_eq!(informe.codigo_salida(), 0);
    assert_eq!(informe.resumen(), "0 avisos, 0 errores");
    // El resumen y el código solo miran avisos y errores de verdad.
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    assert!(texto.contains("0 avisos, 0 errores"), "{texto}");

    // Con un aviso de verdad, solo ese cuenta; con un error, cambia el código.
    let con_aviso = con_termux(|termux| termux.api = ApiObservada::Ausente);
    let informe = comprobar(&con_aviso);
    assert_eq!((informe.avisos, informe.errores), (1, 0));
    assert_eq!(informe.codigo_salida(), 0);
    let mut con_error = datos_termux();
    con_error.base_datos = ResultadoBaseDatos::Futura(magi::almacen::migraciones::VERSION_BD + 1);
    let informe = comprobar(&con_error);
    assert_eq!((informe.avisos, informe.errores), (0, 1));
    assert_eq!(informe.codigo_salida(), 1);
}

#[test]
fn un_sistema_de_termux_sano_enseña_el_bloque_de_6_5() {
    let datos = datos_termux();
    let informe = comprobar(&datos);
    let nombres: Vec<&str> = informe
        .comprobaciones
        .iter()
        .map(|comprobacion| comprobacion.nombre)
        .collect();
    assert_eq!(
        nombres,
        [
            "datos",
            "configuración",
            "estado",
            "runtime",
            "servidor",
            "agente SSH",
            "llavero",
            "portapapeles",
            "navegador",
            "editor",
            "paginador",
            "terminal",
            "ssh_config",
            "sincronización",
            "Termux",
            "Termux:API",
            "almacenamiento",
            "teclas",
            "wake lock",
            "avisos",
            "segundo plano",
            "batería",
        ]
    );
    assert_eq!(informe.bloques.len(), 1);
    assert_eq!(informe.bloques[0].titulo, "TERMUX");
    assert_eq!(informe.bloques[0].inicio, 14, "tras las generales");

    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    assert_eq!(
        bloque_de(&texto),
        [
            "  ✓ Termux          0.118.3 · Android 15 (API 35) · aarch64",
            "  ✓ Termux:API      responde (0,4 s)",
            "  ✓ almacenamiento  ~/storage/shared → /storage/emulated/0",
            "  ✓ teclas          la fila de MAGI (~/.termux/termux.properties)",
            "  ✓ wake lock       auto · ahora sin trabajo",
            "  ✓ avisos          activados",
            "  · segundo plano   Android 12+ puede cerrar procesos de Termux. En Android 14 o posterior:",
            "                    Opciones de desarrollador → «Desactivar restricciones de procesos secundarios»",
            "  · batería         quita la optimización de batería de Termux",
            "                    (y, según el fabricante, permite su actividad en segundo plano)",
        ]
    );
    // El título va tras una línea en blanco.
    assert!(
        texto.contains("\n\n  TERMUX\n  ✓ Termux "),
        "el bloque no va aparte:\n{texto}"
    );
}

#[test]
fn las_lineas_generales_de_termux_estan_adaptadas() {
    let datos = datos_termux();
    let (_, texto) = texto_de(&datos, false);
    assert!(
        texto.contains("✓ runtime         $TMPDIR/magi-10234 (700) · socket 60 bytes"),
        "{texto}"
    );
    assert!(
        texto.contains("✓ llavero         fichero con frase maestra (modificado el 17 nov 2026)"),
        "{texto}"
    );
    assert!(
        texto.contains("✓ portapapeles    termux-clipboard-set (Termux:API)"),
        "{texto}"
    );
    assert!(
        texto.contains(
            "· terminal        en Termux no aplica: abre otra sesión de Termux para otra ventana"
        ),
        "{texto}"
    );
    // Lo que ya servía sigue igual.
    assert!(texto.contains("✓ editor"), "{texto}");
    assert!(texto.contains("✓ paginador"), "{texto}");
    // Y nada de secret-tool en Termux.
    assert!(!texto.contains("secret-tool"), "{texto}");
}

#[test]
fn el_llavero_de_termux_se_describe_sin_abrirlo() {
    let linea_llavero = |llavero: LlaveroObservado| {
        let informe = comprobar(&con_termux(|termux| termux.llavero = llavero));
        comprobacion_de(&informe, "llavero").clone()
    };
    let sin_crear = linea_llavero(LlaveroObservado::SinCrear);
    assert_eq!(sin_crear.estado, Estado::Correcto);
    assert_eq!(
        sin_crear.detalle,
        "fichero con frase maestra (sin crear aún)"
    );
    let con_fecha = linea_llavero(LlaveroObservado::Fichero {
        modificado: NaiveDate::from_ymd_opt(2027, 1, 3),
    });
    assert_eq!(con_fecha.estado, Estado::Correcto);
    assert_eq!(
        con_fecha.detalle,
        "fichero con frase maestra (modificado el 3 ene 2027)"
    );
    let sin_fecha = linea_llavero(LlaveroObservado::Fichero { modificado: None });
    assert_eq!(sin_fecha.detalle, "fichero con frase maestra");
    let ilegible = linea_llavero(LlaveroObservado::NoLegible(
        "no es un fichero normal".to_string(),
    ));
    assert_eq!(ilegible.estado, Estado::Aviso);
    assert!(
        ilegible.detalle.contains("no es un fichero normal"),
        "{}",
        ilegible.detalle
    );
    assert!(ilegible.arreglo.unwrap().contains("magi llavero estado"));
}

#[test]
fn el_portapapeles_de_termux_cae_a_osc_52_con_su_tope() {
    for api in [
        ApiObservada::Ausente,
        ApiObservada::SinRespuesta { plazo_ms: 5000 },
        ApiObservada::Falla {
            motivo: "fallo".to_string(),
        },
    ] {
        let informe = comprobar(&con_termux(|termux| termux.api = api.clone()));
        let linea = comprobacion_de(&informe, "portapapeles");
        assert_eq!(linea.estado, Estado::Correcto, "{api:?}");
        assert!(
            linea.detalle.starts_with("OSC 52 (hasta unos "),
            "{}",
            linea.detalle
        );
        assert!(linea.detalle.contains("KiB"), "{}", linea.detalle);
    }
    let informe = comprobar(&datos_termux());
    assert!(comprobacion_de(&informe, "portapapeles")
        .detalle
        .starts_with("termux-clipboard-set"));
    // Sin el paquete no «responde mal»: no hay Termux:API.
    let sin_paquete = comprobar(&con_termux(|termux| termux.api = ApiObservada::Ausente));
    assert!(comprobacion_de(&sin_paquete, "portapapeles")
        .detalle
        .ends_with("· sin Termux:API"));
    let colgada = comprobar(&con_termux(|termux| {
        termux.api = ApiObservada::SinRespuesta { plazo_ms: 5000 };
    }));
    assert!(comprobacion_de(&colgada, "portapapeles")
        .detalle
        .ends_with("· Termux:API no responde"));
}

#[test]
fn con_el_termux_de_google_play_la_linea_avisa_y_recomienda_fdroid_o_github() {
    let datos = con_termux(|termux| {
        termux.version_termux = Some("googleplay.2026.11.1".to_string());
    });
    let informe = comprobar(&datos);
    let linea = comprobacion_de(&informe, "Termux");
    assert_eq!(linea.estado, Estado::Aviso, "{}", linea.detalle);
    assert!(
        linea
            .detalle
            .contains("googleplay.2026.11.1 · Android 15 (API 35) · aarch64"),
        "{}",
        linea.detalle
    );
    assert!(
        linea
            .detalle
            .contains("no soportado: usa el Termux de F-Droid o GitHub"),
        "{}",
        linea.detalle
    );
    // Es un aviso, no un error: el código de salida no cambia.
    assert_eq!((informe.avisos, informe.errores), (1, 0));
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    assert!(
        texto.contains("  ! Termux          googleplay.2026.11.1"),
        "{texto}"
    );
    assert!(texto.contains("1 aviso, 0 errores"), "{texto}");
    // Sin versión (variable ausente) no es el de Google Play.
    let sin_version = con_termux(|termux| termux.version_termux = None);
    let informe = comprobar(&sin_version);
    let linea = comprobacion_de(&informe, "Termux");
    assert_eq!(linea.estado, Estado::Correcto);
    assert!(
        linea.detalle.starts_with("? · Android 15"),
        "{}",
        linea.detalle
    );
}

#[test]
fn termux_api_se_describe_en_sus_cuatro_estados() {
    let linea = |api: ApiObservada| {
        let informe = comprobar(&con_termux(|termux| termux.api = api));
        comprobacion_de(&informe, "Termux:API").clone()
    };
    let responde = linea(ApiObservada::Responde { ms: 380 });
    assert_eq!(responde.estado, Estado::Correcto);
    assert_eq!(responde.detalle, "responde (0,4 s)");
    assert_eq!(
        linea(ApiObservada::Responde { ms: 1000 }).detalle,
        "responde (1 s)"
    );
    assert_eq!(
        linea(ApiObservada::Responde { ms: 3 }).detalle,
        "responde (menos de 0,1 s)"
    );

    for (api, texto) in [
        (ApiObservada::Ausente, "falta el paquete termux-api"),
        (
            ApiObservada::SinRespuesta { plazo_ms: 5000 },
            "no responde en 5 s",
        ),
        (
            ApiObservada::Falla {
                motivo: "se rompió".to_string(),
            },
            "falla: se rompió",
        ),
    ] {
        let linea = linea(api.clone());
        assert_eq!(linea.estado, Estado::Aviso, "{api:?}");
        assert!(linea.detalle.contains(texto), "{}", linea.detalle);
        let arreglo = linea.arreglo.expect("con el arreglo");
        assert!(
            arreglo.contains("termux-api") && arreglo.contains("app Termux:API"),
            "{arreglo}"
        );
    }
}

#[test]
fn el_almacenamiento_dice_adonde_apunta_o_que_falta() {
    let linea = |almacenamiento: AlmacenamientoObservado| {
        let informe = comprobar(&con_termux(|termux| termux.almacenamiento = almacenamiento));
        comprobacion_de(&informe, "almacenamiento").clone()
    };
    let bien = linea(AlmacenamientoObservado::Presente {
        destino: Some("/storage/emulated/0".to_string()),
        accesible: true,
    });
    assert_eq!(bien.estado, Estado::Correcto);
    assert_eq!(bien.detalle, "~/storage/shared → /storage/emulated/0");

    let sin_enlace = linea(AlmacenamientoObservado::Presente {
        destino: None,
        accesible: true,
    });
    assert_eq!(sin_enlace.estado, Estado::Correcto);
    assert_eq!(sin_enlace.detalle, "~/storage/shared");

    let falta = linea(AlmacenamientoObservado::Ausente);
    assert_eq!(falta.estado, Estado::Aviso);
    assert_eq!(falta.detalle, "~/storage/shared no existe");
    assert_eq!(
        falta.arreglo.as_deref(),
        Some("ejecuta termux-setup-storage")
    );

    let sin_permiso = linea(AlmacenamientoObservado::Presente {
        destino: Some("/storage/emulated/0".to_string()),
        accesible: false,
    });
    assert_eq!(sin_permiso.estado, Estado::Aviso);
    assert!(
        sin_permiso.detalle.contains("sin acceso"),
        "{}",
        sin_permiso.detalle
    );
    assert!(sin_permiso
        .arreglo
        .unwrap()
        .contains("termux-setup-storage"));
}

#[test]
fn las_teclas_son_correctas_si_es_la_fila_de_magi_y_si_no_solo_informan() {
    let linea = |teclas: EstadoFila| {
        let informe = comprobar(&con_termux(|termux| termux.teclas = teclas));
        comprobacion_de(&informe, "teclas").clone()
    };
    let de_magi = linea(EstadoFila::LaDeMagi);
    assert_eq!(de_magi.estado, Estado::Correcto);
    assert_eq!(
        de_magi.detalle,
        "la fila de MAGI (~/.termux/termux.properties)"
    );
    assert_eq!(de_magi.arreglo, None);

    for teclas in [
        EstadoFila::SinFichero,
        EstadoFila::SinExtraKeys,
        EstadoFila::Otra,
    ] {
        let linea = linea(teclas.clone());
        assert_eq!(
            linea.estado,
            Estado::Info,
            "{teclas:?}: nunca un aviso ni un error"
        );
        assert!(
            linea.detalle.contains("~/.termux/termux.properties"),
            "{}",
            linea.detalle
        );
        assert_eq!(
            linea.arreglo.as_deref(),
            Some("magi termux teclas --instalar"),
            "{teclas:?}"
        );
    }
    let ilegible = linea(EstadoFila::Ilegible("demasiado grande".to_string()));
    assert_eq!(ilegible.estado, Estado::Info);
    assert!(
        ilegible.detalle.contains("demasiado grande"),
        "{}",
        ilegible.detalle
    );
}

#[test]
fn el_wake_lock_dice_el_modo_y_si_magi_lo_tiene_tomado() {
    let linea = |tomado: bool, servidor: ServidorObservado, modo: &str| {
        let mut datos = con_termux(|termux| termux.wake_lock_tomado = tomado);
        datos.servidor = servidor;
        let mut config = Config::default();
        config.termux.wake_lock = modo.to_string();
        datos.config = ResultadoConfig::Valida(Box::new(config));
        let informe = comprobar(&datos);
        comprobacion_de(&informe, "wake lock").clone()
    };
    let sin_trabajo = linea(false, ServidorObservado::Apagado, "auto");
    assert_eq!(sin_trabajo.estado, Estado::Correcto);
    assert_eq!(sin_trabajo.detalle, "auto · ahora sin trabajo");

    let tomado = linea(true, servidor_en_marcha(), "auto");
    assert_eq!(tomado.estado, Estado::Correcto);
    assert!(
        tomado.detalle.starts_with("auto · tomado ahora"),
        "{}",
        tomado.detalle
    );

    // Una marca sin servidor es de uno que murió sin soltarlo.
    let huerfano = linea(true, ServidorObservado::Apagado, "auto");
    assert_eq!(huerfano.estado, Estado::Aviso);
    assert!(
        huerfano.detalle.contains("ya no está"),
        "{}",
        huerfano.detalle
    );
    assert!(huerfano.arreglo.unwrap().contains("termux-wake-unlock"));

    let nunca = linea(false, ServidorObservado::Apagado, "nunca");
    assert_eq!(nunca.estado, Estado::Correcto);
    assert!(nunca.detalle.starts_with("nunca"), "{}", nunca.detalle);
    // Con «nunca» no importa la marca de un servidor anterior.
    assert!(linea(true, servidor_en_marcha(), "nunca")
        .detalle
        .starts_with("nunca"));

    // Un valor que no existe se toma por «auto» y se dice.
    let raro = linea(false, ServidorObservado::Apagado, "siempre");
    assert_eq!(raro.estado, Estado::Aviso);
    assert!(
        raro.detalle.contains("auto · ahora sin trabajo"),
        "{}",
        raro.detalle
    );
    assert!(
        raro.detalle.contains("[termux] wake_lock no vale"),
        "{}",
        raro.detalle
    );
}

#[test]
fn los_avisos_dicen_si_estan_activados_desactivados_o_sin_termux_api() {
    let linea = |activados: bool, apagada: bool, servidor: ServidorObservado| {
        let mut datos = con_termux(|termux| termux.api_apagada_en_servidor = apagada);
        datos.servidor = servidor;
        let mut config = Config::default();
        config.termux.avisos = activados;
        datos.config = ResultadoConfig::Valida(Box::new(config));
        let informe = comprobar(&datos);
        comprobacion_de(&informe, "avisos").clone()
    };
    let activados = linea(true, false, servidor_en_marcha());
    assert_eq!(activados.estado, Estado::Correcto);
    assert_eq!(activados.detalle, "activados");
    let desactivados = linea(false, false, servidor_en_marcha());
    assert_eq!(desactivados.estado, Estado::Correcto);
    assert!(
        desactivados.detalle.starts_with("desactivados"),
        "{}",
        desactivados.detalle
    );
    // Con los avisos desactivados da igual lo que diga el servidor.
    assert!(linea(false, true, servidor_en_marcha())
        .detalle
        .starts_with("desactivados"));

    let apagada = linea(true, true, servidor_en_marcha());
    assert_eq!(apagada.estado, Estado::Aviso);
    assert!(
        apagada
            .detalle
            .contains("Termux:API está apagada en el servidor"),
        "{}",
        apagada.detalle
    );
    assert!(apagada.arreglo.unwrap().contains("magi servidor parar"));
    // El fichero del servidor de antes no vale si ya no hay servidor.
    let sin_servidor = linea(true, true, ServidorObservado::Apagado);
    assert_eq!(sin_servidor.estado, Estado::Correcto);
    // Ni tampoco se afirma nada de uno que no se pudo comprobar.
    for servidor in [servidor_otra_version(), ServidorObservado::SinRespuesta] {
        let incierto = linea(true, true, servidor);
        assert_eq!(incierto.estado, Estado::Correcto);
        assert_eq!(incierto.detalle, "activados");
    }
}

/// Un servidor de otra versión: vivo, pero no habla este protocolo.
fn servidor_otra_version() -> ServidorObservado {
    ServidorObservado::OtraVersion {
        version_app: Some("1.4.0".to_string()),
        protocolo: magi::protocolo::VERSION_PROTOCOLO - 1,
    }
}

/// La línea «wake lock» con la marca, el servidor y el modo indicados.
fn linea_wake_lock(
    tomado: bool,
    servidor: ServidorObservado,
    modo: &str,
) -> magi::diagnostico::Comprobacion {
    let mut datos = con_termux(|termux| termux.wake_lock_tomado = tomado);
    datos.servidor = servidor;
    let mut config = Config::default();
    config.termux.wake_lock = modo.to_string();
    datos.config = ResultadoConfig::Valida(Box::new(config));
    comprobacion_de(&comprobar(&datos), "wake lock").clone()
}

/// Con una marca de wake lock y un servidor que no se pudo comprobar (de otra
/// versión, o ocupado), el diagnóstico no dice que el servidor «ya no está» ni
/// recomienda soltarlo: podría soltar el wake lock de un servidor vivo y con
/// trabajo. Solo se afirma lo que se sabe: sin socket, no hay servidor.
#[test]
fn el_wake_lock_no_afirma_que_no_hay_servidor_si_solo_no_responde() {
    for (servidor, motivo) in [
        (servidor_otra_version(), "es de otra versión"),
        (ServidorObservado::SinRespuesta, "no responde"),
    ] {
        let linea = linea_wake_lock(true, servidor.clone(), "auto");
        assert_eq!(linea.estado, Estado::Info, "{servidor:?}");
        assert!(linea.detalle.contains(motivo), "{}", linea.detalle);
        assert!(
            !linea.detalle.contains("ya no está"),
            "afirma que no hay servidor: {}",
            linea.detalle
        );
        assert!(
            linea.detalle.contains("no se puede saber"),
            "{}",
            linea.detalle
        );
        // Ni arreglo ni recomendación de soltar el wake lock de un servidor
        // que quizá lo necesita.
        assert_eq!(linea.arreglo, None, "{servidor:?}");
        // Y no suma ningún aviso al de la línea del servidor (que ya lo da por
        // su cuenta): con y sin marca salen los mismos.
        let avisos = |tomado: bool| {
            let mut datos = con_termux(|termux| termux.wake_lock_tomado = tomado);
            datos.servidor = servidor.clone();
            comprobar(&datos).avisos
        };
        assert_eq!(avisos(true), avisos(false), "{servidor:?}");
    }
    // Sin socket sí se sabe: es de un servidor que murió, y se dice cómo soltarlo.
    let huerfano = linea_wake_lock(true, ServidorObservado::Apagado, "auto");
    assert_eq!(huerfano.estado, Estado::Aviso);
    assert!(
        huerfano.detalle.contains("ya no está"),
        "{}",
        huerfano.detalle
    );
    assert!(huerfano.arreglo.unwrap().contains("termux-wake-unlock"));
    // Con el servidor en marcha, es lo normal.
    assert_eq!(
        linea_wake_lock(true, servidor_en_marcha(), "auto").estado,
        Estado::Correcto
    );
    // Sin marca no hay nada que decir de ningún servidor.
    for servidor in [
        servidor_otra_version(),
        ServidorObservado::SinRespuesta,
        ServidorObservado::Apagado,
    ] {
        let linea = linea_wake_lock(false, servidor, "auto");
        assert_eq!(linea.estado, Estado::Correcto);
        assert_eq!(linea.detalle, "auto · ahora sin trabajo");
    }
}

/// Con `wake_lock = "nunca"` y una marca de antes, MAGI no toma ninguno, pero
/// el wake lock de antes puede seguir tomado (lo suelta el servidor al
/// arrancar): decir «MAGI no toca el wake lock» sin más era dar por bueno lo
/// que puede estar gastando batería.
#[test]
fn con_nunca_una_marca_residual_se_avisa() {
    let limpia = linea_wake_lock(false, ServidorObservado::Apagado, "nunca");
    assert_eq!(limpia.estado, Estado::Correcto);
    assert_eq!(
        limpia.detalle,
        "nunca · MAGI no toca el wake lock ([termux] wake_lock)"
    );

    // Sin servidor: nadie lo va a soltar hasta que se abra MAGI. Aviso.
    let sin_servidor = linea_wake_lock(true, ServidorObservado::Apagado, "nunca");
    assert_eq!(sin_servidor.estado, Estado::Aviso);
    assert!(
        sin_servidor.detalle.starts_with("nunca · MAGI no lo toma"),
        "{}",
        sin_servidor.detalle
    );
    assert!(
        sin_servidor.detalle.contains("dejó uno tomado"),
        "{}",
        sin_servidor.detalle
    );
    assert!(
        !sin_servidor.detalle.contains("no toca el wake lock"),
        "da por bueno lo que no lo está: {}",
        sin_servidor.detalle
    );
    assert!(sin_servidor.arreglo.unwrap().contains("termux-wake-unlock"));

    // Con un servidor en marcha o sin comprobar, se sabe menos: informa y dice
    // cómo se suelta, pero no cuenta como aviso.
    for servidor in [
        servidor_en_marcha(),
        servidor_otra_version(),
        ServidorObservado::SinRespuesta,
    ] {
        let linea = linea_wake_lock(true, servidor.clone(), "nunca");
        assert_eq!(linea.estado, Estado::Info, "{servidor:?}");
        assert!(
            linea.detalle.contains("queda la marca"),
            "{}",
            linea.detalle
        );
        assert!(
            linea
                .arreglo
                .as_deref()
                .unwrap_or("")
                .contains("magi servidor parar"),
            "{:?}",
            linea.arreglo
        );
    }
    // El código de salida no cambia, pero el aviso sí cuenta en el resumen.
    let mut datos = con_termux(|termux| termux.wake_lock_tomado = true);
    datos.servidor = ServidorObservado::Apagado;
    let mut config = Config::default();
    config.termux.wake_lock = "nunca".to_string();
    datos.config = ResultadoConfig::Valida(Box::new(config));
    let informe = comprobar(&datos);
    assert_eq!((informe.avisos, informe.errores), (1, 0));
    assert_eq!(informe.codigo_salida(), 0);
}

/// Sin Termux:API (falta el paquete, no responde o falla), la línea de
/// Termux:API ya avisa: la de avisos no puede decir «activados» sin más, pero
/// tampoco contar un segundo aviso por lo mismo.
#[test]
fn sin_termux_api_los_avisos_dicen_que_solo_salen_en_la_barra() {
    let datos_con = |api: ApiObservada, activados: bool| {
        let mut datos = con_termux(|termux| termux.api = api);
        datos.servidor = servidor_en_marcha();
        let mut config = Config::default();
        config.termux.avisos = activados;
        datos.config = ResultadoConfig::Valida(Box::new(config));
        datos
    };
    for api in [
        ApiObservada::Ausente,
        ApiObservada::SinRespuesta { plazo_ms: 5000 },
        ApiObservada::Falla {
            motivo: "se rompió".to_string(),
        },
    ] {
        let datos = datos_con(api.clone(), true);
        let informe = comprobar(&datos);
        let avisos = comprobacion_de(&informe, "avisos");
        assert_eq!(avisos.estado, Estado::Info, "{api:?}");
        assert_eq!(
            avisos.detalle,
            "activados, pero sin Termux:API solo salen en la barra de MAGI"
        );
        assert_eq!(avisos.arreglo, None);
        // El aviso es el de la línea de Termux:API y solo ese.
        assert_eq!(
            comprobacion_de(&informe, "Termux:API").estado,
            Estado::Aviso
        );
        assert_eq!((informe.avisos, informe.errores), (1, 0), "{api:?}");

        // En pantalla: ya no sale «✓ avisos activados» junto a «! Termux:API».
        let (_, texto) = texto_de(&datos, false);
        assert!(
            texto.contains("  · avisos          activados, pero sin Termux:API"),
            "{texto}"
        );
        assert!(!texto.contains("✓ avisos"), "{texto}");

        // Con los avisos desactivados no hay nada que matizar.
        let informe = comprobar(&datos_con(api, false));
        assert!(comprobacion_de(&informe, "avisos")
            .detalle
            .starts_with("desactivados"));
    }
    // Con Termux:API respondiendo, lo de siempre.
    let informe = comprobar(&datos_con(ApiObservada::Responde { ms: 100 }, true));
    assert_eq!(comprobacion_de(&informe, "avisos").detalle, "activados");

    // Sin Termux:API ahora y apagada en el servidor, no se avisa dos veces: la
    // línea de Termux:API ya dice cómo arreglarlo.
    let mut apagada = datos_con(ApiObservada::Ausente, true);
    apagada.termux.as_mut().unwrap().api_apagada_en_servidor = true;
    let informe = comprobar(&apagada);
    assert_eq!(comprobacion_de(&informe, "avisos").estado, Estado::Info);
    assert_eq!((informe.avisos, informe.errores), (1, 0));
}

#[test]
fn segundo_plano_depende_de_la_version_de_android() {
    let linea = |sdk: Option<u32>| {
        let informe = comprobar(&con_termux(|termux| termux.android_sdk = sdk));
        informe
            .comprobaciones
            .iter()
            .find(|comprobacion| comprobacion.nombre == "segundo plano")
            .cloned()
    };
    // Antes de Android 12 no hay límite de procesos: ni línea.
    for sdk in [24, 29, 30] {
        assert!(linea(Some(sdk)).is_none(), "SDK {sdk}");
    }
    let doce = linea(Some(31)).expect("Android 12");
    assert_eq!(doce.estado, Estado::Info);
    assert_eq!(
        doce.detalle,
        "Android 12+ puede cerrar procesos de Termux. En Android 12:"
    );
    // Las dos órdenes de adb, cada una en su línea.
    assert_eq!(
        doce.nota.expect("las órdenes de adb"),
        "adb shell device_config set_sync_disabled_for_tests persistent\n\
         adb shell device_config put activity_manager max_phantom_processes 2147483647"
    );
    for sdk in [32, 33] {
        let linea = linea(Some(sdk)).expect("Android 12L o 13");
        assert_eq!(linea.estado, Estado::Info);
        assert_eq!(
            linea.detalle,
            "Android 12+ puede cerrar procesos de Termux. En Android 12L y 13:"
        );
        assert_eq!(
            linea.nota.as_deref(),
            Some("adb shell settings put global settings_enable_monitor_phantom_procs false"),
            "SDK {sdk}"
        );
    }
    for sdk in [34, 35] {
        let linea = linea(Some(sdk)).expect("Android 14 o posterior");
        assert_eq!(linea.estado, Estado::Info);
        assert_eq!(
            linea.detalle,
            "Android 12+ puede cerrar procesos de Termux. En Android 14 o posterior:"
        );
        assert_eq!(
            linea.nota.as_deref(),
            Some("Opciones de desarrollador → «Desactivar restricciones de procesos secundarios»"),
            "SDK {sdk}"
        );
    }
    // Sin saber la versión se dice, y se da el ajuste de las nuevas.
    let desconocida = linea(None).expect("sin versión");
    assert_eq!(desconocida.estado, Estado::Info);
    assert!(
        desconocida
            .detalle
            .contains("No se pudo leer tu versión de Android"),
        "{}",
        desconocida.detalle
    );

    // La función pura es la de §7.9.
    assert_eq!(segundo_plano(30), None);
    assert!(segundo_plano(31).unwrap().contains(" · "), "dos órdenes");
}

#[test]
fn la_bateria_siempre_informa_y_sin_la_bandera_no_salen_las_dos_lineas() {
    let informe = comprobar(&datos_termux());
    let bateria = comprobacion_de(&informe, "batería");
    assert_eq!(bateria.estado, Estado::Info);
    assert_eq!(
        bateria.detalle,
        "quita la optimización de batería de Termux"
    );
    assert_eq!(
        bateria.nota.as_deref(),
        Some("(y, según el fabricante, permite su actividad en segundo plano)")
    );
    // Antes de Android 12, la de batería sigue.
    let vieja = con_termux(|termux| termux.android_sdk = Some(29));
    let informe = comprobar(&vieja);
    assert!(informe
        .comprobaciones
        .iter()
        .all(|comprobacion| comprobacion.nombre != "segundo plano"));
    assert_eq!(comprobacion_de(&informe, "batería").estado, Estado::Info);

    // Sin la bandera, ninguna de las dos.
    let informe = comprobar(&con_termux(|termux| {
        termux.ajustes_android_sin_comprobar = false;
    }));
    for nombre in ["segundo plano", "batería"] {
        assert!(
            informe
                .comprobaciones
                .iter()
                .all(|comprobacion| comprobacion.nombre != nombre),
            "{nombre}"
        );
    }
}

#[test]
fn en_ascii_las_marcas_de_termux_se_degradan_y_no_queda_ningun_glifo() {
    let mut datos = con_termux(|termux| {
        termux.api = ApiObservada::Ausente;
        termux.teclas = EstadoFila::SinFichero;
    });
    datos.ascii = true;
    let (codigo, texto) = texto_de(&datos, false);
    assert_eq!(codigo, 0);
    for glifo in ['✓', '✕', '·', '→', '«', '»'] {
        assert!(!texto.contains(glifo), "queda el glifo «{glifo}»:\n{texto}");
    }
    // El título del bloque y las marcas: ok, !, -.
    let bloque = bloque_de(&texto);
    for inicio in [
        "  ok Termux ",
        "  ! Termux:API",
        "  - teclas",
        "  - segundo plano",
        "  - batería",
    ] {
        assert!(
            bloque.iter().any(|linea| linea.starts_with(inicio)),
            "falta «{inicio}»: {bloque:?}"
        );
    }
    assert!(
        texto.contains("  - terminal"),
        "la línea general de terminal también:\n{texto}"
    );
    // Las flechas pasan a «->».
    assert!(
        texto.contains("~/storage/shared -> /storage/emulated/0"),
        "{texto}"
    );
    assert!(
        texto.contains("-> instala el paquete termux-api"),
        "{texto}"
    );
    assert!(texto.contains("1 aviso, 0 errores"), "{texto}");
}

#[test]
fn con_color_la_marca_informativa_va_en_gris_y_sin_color_no_hay_escapes() {
    let datos = datos_termux();
    let (_, con_color) = texto_de(&datos, true);
    // El gris de lo inactivo (0x5A) en las líneas informativas.
    assert!(
        con_color.contains("\x1b[38;2;90;90;90m·\x1b[0m terminal"),
        "{con_color}"
    );
    let mut sin = datos.clone();
    sin.variables.insert("NO_COLOR".to_string(), String::new());
    let (_, texto) = texto_de(&sin, true);
    assert!(!texto.contains('\x1b'), "{texto}");
    let (_, texto) = texto_de(&datos, false);
    assert!(!texto.contains('\x1b'), "{texto}");
}

// ------------------------------------------------------------ la recogida

/// Un Termux de mentira: dobles de `termux-*` en el `PATH`, un `$HOME` y las
/// rutas de MAGI bajo él.
struct Escenario {
    temporal: tempfile::TempDir,
    dobles: Dobles,
    entorno: magi::termux::EntornoTermux,
    rutas: Rutas,
}

impl Escenario {
    fn nuevo() -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let dobles = Dobles::nuevos(temporal.path());
        let hogar = temporal.path().join("home");
        let entorno = dobles.entorno(&hogar);
        let rutas = Rutas {
            datos: hogar.join(".local/share/magi"),
            config: hogar.join(".config/magi"),
            estado: hogar.join(".local/state/magi"),
            runtime: dobles.prefix().join("tmp/magi-1"),
            hogar,
        };
        Self {
            temporal,
            dobles,
            entorno,
            rutas,
        }
    }

    fn observar(&self, plazo: Duration) -> DatosTermux {
        self.observar_con(plazo, &[])
    }

    fn observar_con(&self, plazo: Duration, variables: &[(&str, &str)]) -> DatosTermux {
        self.observar_con_config(plazo, variables, &Config::default())
    }

    fn observar_con_config(
        &self,
        plazo: Duration,
        variables: &[(&str, &str)],
        config: &Config,
    ) -> DatosTermux {
        let variables: BTreeMap<String, String> = variables
            .iter()
            .map(|(clave, valor)| ((*clave).to_string(), (*valor).to_string()))
            .collect();
        observar(
            &self.rutas,
            config,
            &self.entorno,
            &Propiedades::fijas(&[
                ("ro.product.model", "ABC-X10"),
                ("ro.build.version.release", "15"),
                ("ro.build.version.sdk", "35"),
            ]),
            &variables,
            plazo,
        )
    }

    /// Los ficheros y directorios bajo el temporal (sin los de los dobles, que
    /// anotan sus llamadas), para ver que el diagnóstico no crea nada.
    fn arbol(&self) -> Vec<PathBuf> {
        fn recorrer(ruta: &Path, salida: &mut Vec<PathBuf>) {
            let Ok(entradas) = std::fs::read_dir(ruta) else {
                return;
            };
            for entrada in entradas.flatten() {
                let ruta = entrada.path();
                salida.push(ruta.clone());
                if entrada.file_type().is_ok_and(|tipo| tipo.is_dir()) {
                    recorrer(&ruta, salida);
                }
            }
        }
        let mut salida = Vec::new();
        recorrer(self.temporal.path(), &mut salida);
        salida.retain(|ruta| !ruta.starts_with(self.temporal.path().join("estado")));
        salida.sort();
        salida
    }
}

#[test]
fn observar_lee_la_version_android_y_la_arquitectura() {
    let escenario = Escenario::nuevo();
    let datos = escenario.observar_con(
        Duration::from_secs(20),
        &[("TERMUX_VERSION", "0.118.3"), ("HOME", "/otra/cosa")],
    );
    assert_eq!(datos.version_termux.as_deref(), Some("0.118.3"));
    assert_eq!(datos.android_release.as_deref(), Some("15"));
    assert_eq!(datos.android_sdk, Some(35));
    assert_eq!(datos.arquitectura, std::env::consts::ARCH);
    assert!(datos.ajustes_android_sin_comprobar);

    // Sin la variable no hay versión; una vacía o con basura, tampoco.
    assert_eq!(
        escenario.observar(Duration::from_secs(20)).version_termux,
        None
    );
    let con_basura = escenario.observar_con(
        Duration::from_secs(20),
        &[("TERMUX_VERSION", "\x1b[31m0.1\n")],
    );
    assert_eq!(con_basura.version_termux.as_deref(), Some("[31m0.1"));
    let vacia = escenario.observar_con(Duration::from_secs(20), &[("TERMUX_VERSION", "  ")]);
    assert_eq!(vacia.version_termux, None);
}

#[test]
fn termux_api_responde_con_su_doble_y_sin_efectos() {
    let escenario = Escenario::nuevo();
    let antes = escenario.arbol();
    let datos = escenario.observar(Duration::from_secs(20));
    match datos.api {
        ApiObservada::Responde { ms } => assert!(ms < 20_000, "{ms} ms"),
        otra => panic!("debía responder: {otra:?}"),
    }
    // Una sola llamada a `termux-battery-status`, sin argumentos ni entrada,
    // y ninguna otra orden de Termux (nada de wake lock, avisos ni portapapeles).
    let llamadas = escenario.dobles.llamadas("termux-battery-status");
    assert_eq!(llamadas.len(), 1, "{llamadas:?}");
    assert!(llamadas[0].args.is_empty() && llamadas[0].entrada.is_empty());
    for orden in dobles::ORDENES {
        if orden != "termux-battery-status" {
            assert!(
                escenario.dobles.llamadas(orden).is_empty(),
                "el diagnóstico no debe llamar a {orden}"
            );
        }
    }
    assert_eq!(escenario.arbol(), antes, "el diagnóstico creó ficheros");
}

/// La variante de la línea de comandos observa lo mismo que `observar`: solo
/// añade que se puede interrumpir.
#[test]
fn observar_interrumpible_sin_senal_observa_lo_mismo_que_observar() {
    let escenario = Escenario::nuevo();
    let plazo = Duration::from_secs(20);
    let propiedades = Propiedades::fijas(&[
        ("ro.product.model", "ABC-X10"),
        ("ro.build.version.release", "15"),
        ("ro.build.version.sdk", "35"),
    ]);
    let variables = BTreeMap::new();
    let normal = escenario.observar(plazo);
    let interrumpible = observar_interrumpible(
        &escenario.rutas,
        &Config::default(),
        &escenario.entorno,
        &propiedades,
        &variables,
        plazo,
    )
    .expect("sin señal no se interrumpe");
    assert!(
        matches!(interrumpible.api, ApiObservada::Responde { .. }),
        "{:?}",
        interrumpible.api
    );
    // Lo demás es idéntico (los milisegundos de la respuesta no).
    let mismo_tiempo = DatosTermux {
        api: normal.api.clone(),
        ..interrumpible
    };
    assert_eq!(mismo_tiempo, normal);
    assert_eq!(
        escenario.dobles.llamadas("termux-battery-status").len(),
        2,
        "una llamada de cada"
    );
}

#[test]
fn termux_api_ausente_si_falta_el_paquete() {
    let escenario = Escenario::nuevo();
    escenario.dobles.quitar("termux-battery-status");
    let inicio = Instant::now();
    let datos = escenario.observar(Duration::from_secs(20));
    assert_eq!(datos.api, ApiObservada::Ausente);
    assert!(inicio.elapsed() < Duration::from_secs(5), "no debe esperar");
}

#[test]
fn termux_api_colgada_vence_el_plazo_y_se_mata_la_orden() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-battery-status", Modo::Colgar);
    let inicio = Instant::now();
    let datos = escenario.observar(Duration::from_millis(300));
    let tardo = inicio.elapsed();
    assert_eq!(datos.api, ApiObservada::SinRespuesta { plazo_ms: 300 });
    assert!(
        tardo >= Duration::from_millis(250) && tardo < Duration::from_secs(10),
        "tardó {tardo:?}"
    );
    // El grupo entero (el doble y su hijo) muere: no queda nada colgando.
    let llamadas = escenario.dobles.llamadas("termux-battery-status");
    assert_eq!(llamadas.len(), 1);
    assert!(!llamadas[0].pids.is_empty(), "el doble anota sus pids");
    for pid in &llamadas[0].pids {
        assert!(
            !sigue_vivo_tras(*pid, Duration::from_secs(5)),
            "el proceso {pid} sigue vivo"
        );
    }
    // Y el diagnóstico la cuenta como aviso, no como error.
    let mut a_mano = datos_termux();
    a_mano.termux = Some(DatosTermux {
        api: datos.api,
        ..termux_sano()
    });
    let informe = comprobar(&a_mano);
    assert_eq!((informe.avisos, informe.errores), (1, 0));
}

#[test]
fn termux_api_que_falla_se_cuenta_con_su_motivo_saneado() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-battery-status", Modo::ErrorLargo);
    match escenario.observar(Duration::from_secs(20)).api {
        ApiObservada::Falla { motivo } => {
            assert!(!motivo.chars().any(char::is_control), "{motivo:?}");
            assert!(motivo.chars().count() <= 200, "{}", motivo.chars().count());
            assert!(motivo.contains("fallo"), "{motivo}");
        }
        otra => panic!("debía fallar: {otra:?}"),
    }
    escenario
        .dobles
        .programar("termux-battery-status", Modo::Fallar);
    assert!(matches!(
        escenario.observar(Duration::from_secs(20)).api,
        ApiObservada::Falla { .. }
    ));
}

#[test]
fn observar_mira_el_almacenamiento_y_las_marcas_del_estado() {
    let escenario = Escenario::nuevo();
    let vacio = escenario.observar(Duration::from_secs(20));
    assert_eq!(vacio.almacenamiento, AlmacenamientoObservado::Ausente);
    assert!(!vacio.wake_lock_tomado);
    assert!(!vacio.api_apagada_en_servidor);
    assert_eq!(vacio.llavero, LlaveroObservado::SinCrear);

    // `termux-setup-storage` deja `~/storage/shared`.
    let destino = escenario.temporal.path().join("emulated");
    std::fs::create_dir_all(&destino).unwrap();
    std::fs::create_dir_all(escenario.rutas.hogar.join("storage")).unwrap();
    std::os::unix::fs::symlink(&destino, escenario.rutas.hogar.join("storage/shared")).unwrap();
    // El servidor deja su marca de wake lock y dice que Termux:API está apagada.
    magi::termux::estado::poner_marca(&escenario.rutas, magi::termux::estado::Marca::WakeLock)
        .unwrap();
    magi::termux::estado::escribir_servidor(
        &escenario.rutas,
        &magi::termux::estado::EstadoServidorTermux {
            arranque: 1,
            avisos: vec![magi::termux::estado::AvisoServidor::ApiApagada],
        },
    )
    .unwrap();
    let datos = escenario.observar(Duration::from_secs(20));
    assert_eq!(
        datos.almacenamiento,
        AlmacenamientoObservado::Presente {
            destino: Some(destino.to_string_lossy().into_owned()),
            accesible: true,
        }
    );
    assert!(datos.wake_lock_tomado);
    assert!(datos.api_apagada_en_servidor);
}

/// Una configuración con otro prefijo de escape que el de por defecto.
fn config_con_prefijo(prefijo: &str) -> Config {
    Config {
        prefijo_escape: prefijo.to_string(),
        ..Config::default()
    }
}

/// Pone en el `termux.properties` del escenario la fila de MAGI para `prefijo`.
fn instalar_fila_de_magi(escenario: &Escenario, prefijo: &str) {
    let ruta = ruta_properties(&escenario.entorno).expect("ruta de termux.properties");
    std::fs::create_dir_all(ruta.parent().unwrap()).unwrap();
    std::fs::write(&ruta, format!("{}\n", fila(prefijo))).unwrap();
}

/// La línea «teclas» del diagnóstico para lo observado.
fn linea_de_teclas(datos: DatosTermux) -> magi::diagnostico::Comprobacion {
    let mut con_datos = datos_termux();
    con_datos.termux = Some(datos);
    comprobacion_de(&comprobar(&con_datos), "teclas").clone()
}

/// `observar` le pasa al módulo de teclas el prefijo de la configuración: con
/// un `termux.properties` que lleva la fila de MAGI para un prefijo que NO es
/// el de por defecto, solo es «la de MAGI» para ese prefijo. (Sin fichero, el
/// estado es `SinFichero` con cualquier prefijo y la prueba no distinguiría un
/// prefijo vacío o equivocado.)
#[test]
fn las_teclas_las_da_el_modulo_de_teclas() {
    let escenario = Escenario::nuevo();
    let plazo = Duration::from_secs(20);

    // Sin fichero: no hay fila, con cualquier prefijo.
    let sin_fichero = escenario.observar(plazo);
    assert_eq!(sin_fichero.teclas, EstadoFila::SinFichero);
    assert_eq!(
        sin_fichero.fichero_teclas,
        ruta_properties(&escenario.entorno)
    );

    // La fila de MAGI para «Ctrl+a», que no es el prefijo por defecto.
    instalar_fila_de_magi(&escenario, "Ctrl+a");
    let por_defecto = Config::default();
    assert_ne!(por_defecto.prefijo_escape, "Ctrl+a");
    let con_el_suyo = escenario.observar_con_config(plazo, &[], &config_con_prefijo("Ctrl+a"));
    assert_eq!(con_el_suyo.teclas, EstadoFila::LaDeMagi);
    assert_eq!(
        con_el_suyo.teclas,
        estado_fila(&escenario.entorno, "Ctrl+a")
    );
    // Con el prefijo por defecto, esa fila es otra distinta.
    let con_el_defecto = escenario.observar(plazo);
    assert_eq!(con_el_defecto.teclas, EstadoFila::Otra);
    assert_eq!(
        con_el_defecto.teclas,
        estado_fila(&escenario.entorno, &por_defecto.prefijo_escape)
    );
    assert_eq!(
        con_el_defecto.fichero_teclas,
        ruta_properties(&escenario.entorno)
    );

    // Y la línea del diagnóstico lo refleja.
    let linea = linea_de_teclas(con_el_suyo);
    assert_eq!(linea.estado, Estado::Correcto);
    assert!(
        linea.detalle.starts_with("la fila de MAGI"),
        "{}",
        linea.detalle
    );
    let linea = linea_de_teclas(con_el_defecto);
    assert_eq!(linea.estado, Estado::Info);
    assert!(
        linea.detalle.starts_with("tu propia fila"),
        "{}",
        linea.detalle
    );

    // A la inversa: la fila del prefijo por defecto no es la de «Ctrl+a».
    instalar_fila_de_magi(&escenario, &por_defecto.prefijo_escape);
    assert_eq!(escenario.observar(plazo).teclas, EstadoFila::LaDeMagi);
    assert_eq!(
        escenario
            .observar_con_config(plazo, &[], &config_con_prefijo("Ctrl+a"))
            .teclas,
        EstadoFila::Otra
    );
}

/// El diagnóstico nunca abre el llavero ni pide la frase: con basura, con uno
/// de verdad (cerrado) o sin nada, ni cambia el fichero ni toca otra cosa.
#[test]
fn el_llavero_se_mira_sin_abrirlo_ni_pedir_nada() {
    let escenario = Escenario::nuevo();
    let ruta = escenario.rutas.datos.join("llavero.age");
    std::fs::create_dir_all(&escenario.rutas.datos).unwrap();

    // Un fichero con basura.
    std::fs::write(&ruta, b"esto no es un fichero age").unwrap();
    let antes = std::fs::read(&ruta).unwrap();
    let modificado = std::fs::metadata(&ruta).unwrap().modified().unwrap();
    let arbol = escenario.arbol();
    let datos = escenario.observar(Duration::from_secs(20));
    assert!(
        matches!(
            datos.llavero,
            LlaveroObservado::Fichero {
                modificado: Some(_)
            }
        ),
        "{:?}",
        datos.llavero
    );
    assert_eq!(std::fs::read(&ruta).unwrap(), antes);
    assert_eq!(
        std::fs::metadata(&ruta).unwrap().modified().unwrap(),
        modificado
    );
    assert_eq!(
        escenario.arbol(),
        arbol,
        "no se crea ni un fichero (ni un cerrojo)"
    );

    // Uno de verdad, cerrado (la frase solo vive en la memoria del cliente).
    std::fs::remove_file(&ruta).unwrap();
    let llavero = magi_nucleo::llavero_fichero::LlaveroFichero::con_ajustes(
        ruta.clone(),
        4,
        Arc::new(Instant::now),
    );
    llavero
        .crear("una frase maestra larga", "una frase maestra larga")
        .expect("creando el llavero de prueba");
    drop(llavero);
    let antes = std::fs::read(&ruta).unwrap();
    let arbol = escenario.arbol();
    let datos = escenario.observar(Duration::from_secs(20));
    assert!(matches!(datos.llavero, LlaveroObservado::Fichero { .. }));
    assert_eq!(std::fs::read(&ruta).unwrap(), antes);
    assert_eq!(escenario.arbol(), arbol);
    // Y su línea sale sin pedir nada, con la fecha de hoy.
    let mut a_mano = datos_termux();
    a_mano.termux = Some(DatosTermux {
        llavero: datos.llavero,
        ..termux_sano()
    });
    let (_, texto) = texto_de(&a_mano, false);
    assert!(
        texto.contains("fichero con frase maestra (modificado el "),
        "{texto}"
    );

    // Un directorio en su lugar: no legible, un aviso y nunca un error.
    std::fs::remove_file(&ruta).unwrap();
    std::fs::create_dir(&ruta).unwrap();
    let datos = escenario.observar(Duration::from_secs(20));
    assert!(matches!(datos.llavero, LlaveroObservado::NoLegible(_)));
    let mut a_mano = datos_termux();
    a_mano.termux = Some(DatosTermux {
        llavero: datos.llavero,
        ..termux_sano()
    });
    let informe = comprobar(&a_mano);
    assert_eq!((informe.avisos, informe.errores), (1, 0));
}

// -------------------------------------------------------- el binario real

/// `magi diagnostico` como un proceso aparte, con el entorno vacío y el de
/// `escenario` (o el de un Linux aislado si `termux` es falso), sin lanzar.
#[cfg(debug_assertions)]
fn orden_del_binario(
    escenario: &Escenario,
    termux: bool,
    extra: &[(&str, &str)],
) -> std::process::Command {
    let mut orden = std::process::Command::new(env!("CARGO_BIN_EXE_magi"));
    orden
        .arg("diagnostico")
        .env_clear()
        .stdin(std::process::Stdio::null());
    orden.env("HOME", &escenario.rutas.hogar);
    orden.env("PATH", escenario.dobles.path());
    orden.env("LANG", "C.UTF-8");
    if termux {
        orden
            .env("MAGI_PLATAFORMA", "termux")
            .env("PREFIX", escenario.dobles.prefix())
            .env("TMPDIR", escenario.dobles.prefix().join("tmp"))
            .env("TERMUX_VERSION", "0.118.3");
    } else {
        let corto = escenario.dobles.prefix().join("run");
        orden
            .env("XDG_DATA_HOME", &escenario.rutas.datos)
            .env("XDG_CONFIG_HOME", &escenario.rutas.config)
            .env("XDG_STATE_HOME", &escenario.rutas.estado)
            .env("XDG_RUNTIME_DIR", corto);
    }
    for (clave, valor) in extra {
        orden.env(clave, valor);
    }
    orden
}

/// Ejecuta `magi diagnostico` y espera a que termine.
#[cfg(debug_assertions)]
fn ejecutar_binario(
    escenario: &Escenario,
    termux: bool,
    extra: &[(&str, &str)],
) -> std::process::Output {
    orden_del_binario(escenario, termux, extra)
        .output()
        .expect("ejecutando magi diagnostico")
}

#[cfg(debug_assertions)]
#[test]
fn el_binario_en_termux_enseña_el_bloque_con_los_dobles() {
    let escenario = Escenario::nuevo();
    // El almacenamiento compartido, ya concedido.
    let emulado = escenario.temporal.path().join("emulated");
    std::fs::create_dir_all(&emulado).unwrap();
    std::fs::create_dir_all(escenario.rutas.hogar.join("storage")).unwrap();
    std::os::unix::fs::symlink(&emulado, escenario.rutas.hogar.join("storage/shared")).unwrap();
    // Un llavero con basura: el diagnóstico ni lo abre ni lo toca.
    let llavero = escenario.rutas.datos.join("llavero.age");
    std::fs::create_dir_all(&escenario.rutas.datos).unwrap();
    std::fs::write(&llavero, b"basura").unwrap();

    let salida = ejecutar_binario(&escenario, true, &[]);
    let texto = String::from_utf8(salida.stdout).expect("UTF-8");
    let errores = String::from_utf8_lossy(&salida.stderr);
    assert!(salida.status.success(), "{texto}\n{errores}");
    assert!(texto.contains("\n  TERMUX\n"), "{texto}");
    // Sin propiedades de Android (esto es Linux): Termux sale con interrogantes.
    assert!(
        texto.contains(&format!(
            "✓ Termux          0.118.3 · Android ? (API ?) · {}",
            std::env::consts::ARCH
        )),
        "{texto}"
    );
    assert!(texto.contains("✓ Termux:API      responde ("), "{texto}");
    // La espera a Termux:API avisa por la salida de errores, no por la del informe.
    assert!(errores.contains("comprobando Termux:API…"), "{errores}");
    assert!(!texto.contains("comprobando"), "{texto}");
    assert!(
        texto.contains(&format!(
            "✓ almacenamiento  ~/storage/shared → {}",
            emulado.display()
        )),
        "{texto}"
    );
    assert!(texto.contains("· teclas"), "{texto}");
    assert!(
        texto.contains("✓ wake lock       auto · ahora sin trabajo"),
        "{texto}"
    );
    assert!(texto.contains("✓ avisos          activados"), "{texto}");
    assert!(texto.contains("  · batería"), "{texto}");
    assert!(
        texto.contains("✓ runtime         $TMPDIR/magi-"),
        "el socket va en $TMPDIR/magi-<uid>:\n{texto}"
    );
    assert!(
        texto.contains("✓ llavero         fichero con frase maestra (modificado el "),
        "{texto}"
    );
    assert!(
        texto.contains("✓ portapapeles    termux-clipboard-set (Termux:API)"),
        "{texto}"
    );
    assert!(
        texto.contains("· terminal        en Termux no aplica"),
        "{texto}"
    );
    // Sin Android real no hay SDK: la línea de segundo plano lo dice.
    assert!(
        texto.contains("No se pudo leer tu versión de Android"),
        "{texto}"
    );
    // No toca el llavero, no crea nada de MAGI y solo llama a la batería.
    assert_eq!(std::fs::read(&llavero).unwrap(), b"basura");
    assert!(!escenario.rutas.config.exists(), "creó la configuración");
    assert!(!escenario.rutas.estado.exists(), "creó el estado");
    assert!(!escenario.rutas.runtime.exists(), "creó el runtime");
    assert_eq!(escenario.dobles.llamadas("termux-battery-status").len(), 1);
    for orden in dobles::ORDENES {
        if orden != "termux-battery-status" {
            assert!(escenario.dobles.llamadas(orden).is_empty(), "{orden}");
        }
    }
}

#[cfg(debug_assertions)]
#[test]
fn el_binario_en_termux_con_termux_api_colgada_avisa_y_sale_con_cero() {
    let escenario = Escenario::nuevo();
    escenario
        .dobles
        .programar("termux-battery-status", Modo::Colgar);
    // El plazo más corto que admite `[termux] api_plazo_seg` (2 s).
    std::fs::create_dir_all(&escenario.rutas.config).unwrap();
    std::fs::write(
        escenario.rutas.fichero_config(),
        "[termux]\napi_plazo_seg = 2\n",
    )
    .unwrap();
    let inicio = Instant::now();
    let salida = ejecutar_binario(&escenario, true, &[]);
    let tardo = inicio.elapsed();
    let texto = String::from_utf8(salida.stdout).expect("UTF-8");
    assert!(
        salida.status.success(),
        "un aviso no cambia el código:\n{texto}"
    );
    assert!(tardo < Duration::from_secs(15), "tardó {tardo:?}");
    assert!(
        texto.contains("! Termux:API      no responde en 2 s"),
        "{texto}"
    );
    assert!(texto.contains("pkg install termux-api"), "{texto}");
    // Sin Termux:API, el portapapeles cae a OSC 52.
    assert!(
        texto.contains("✓ portapapeles    OSC 52 (hasta unos "),
        "{texto}"
    );
    for pid in &escenario.dobles.llamadas("termux-battery-status")[0].pids {
        assert!(!sigue_vivo_tras(*pid, Duration::from_secs(5)), "{pid}");
    }
}

#[cfg(debug_assertions)]
#[test]
fn el_binario_en_termux_sin_termux_api_ni_almacenamiento_avisa_con_su_arreglo() {
    let escenario = Escenario::nuevo();
    escenario.dobles.quitar("termux-battery-status");
    let salida = ejecutar_binario(
        &escenario,
        true,
        &[("TERMUX_VERSION", "googleplay.2026.11.1")],
    );
    let texto = String::from_utf8(salida.stdout).expect("UTF-8");
    assert!(salida.status.success(), "{texto}");
    assert!(
        texto.contains("! Termux          googleplay.2026.11.1")
            && texto.contains("no soportado: usa el Termux de F-Droid o GitHub"),
        "{texto}"
    );
    assert!(
        texto.contains("! Termux:API      falta el paquete termux-api"),
        "{texto}"
    );
    assert!(
        texto.contains(
            "! almacenamiento  ~/storage/shared no existe → ejecuta termux-setup-storage"
        ),
        "{texto}"
    );
    assert!(
        texto.contains("✓ llavero         fichero con frase maestra (sin crear aún)"),
        "{texto}"
    );
}

#[cfg(debug_assertions)]
#[test]
fn el_binario_fuera_de_termux_no_enseña_nada_de_termux() {
    let escenario = Escenario::nuevo();
    let salida = ejecutar_binario(&escenario, false, &[]);
    let texto = String::from_utf8(salida.stdout).expect("UTF-8");
    assert!(texto.contains("MAGI "), "{texto}");
    for ajeno in ["TERMUX", "Termux", "segundo plano", "batería", "wake lock"] {
        assert!(!texto.contains(ajeno), "«{ajeno}» en Linux:\n{texto}");
    }
    assert!(escenario
        .dobles
        .llamadas("termux-battery-status")
        .is_empty());
    // Ni una palabra de Termux por la salida de errores.
    let errores = String::from_utf8_lossy(&salida.stderr);
    assert!(!errores.contains("Termux"), "{errores}");
}

/// La fila de teclas que el binario compara es la del prefijo de `config.toml`
/// (no uno fijo ni vacío): `recoger` le pasa a `observar` la configuración.
#[cfg(debug_assertions)]
#[test]
fn el_binario_en_termux_compara_la_fila_con_el_prefijo_de_config_toml() {
    let escenario = Escenario::nuevo();
    instalar_fila_de_magi(&escenario, "Ctrl+a");
    let texto = |salida: std::process::Output| {
        assert!(salida.status.success(), "{:?}", salida.status);
        String::from_utf8(salida.stdout).expect("UTF-8")
    };

    // Sin configuración, el prefijo es el de por defecto: esa fila es otra.
    let sin_config = texto(ejecutar_binario(&escenario, true, &[]));
    assert!(
        sin_config.contains("· teclas          tu propia fila en ~/.termux/termux.properties"),
        "{sin_config}"
    );
    // Con `prefijo_escape = "Ctrl+a"`, es la de MAGI.
    std::fs::create_dir_all(&escenario.rutas.config).unwrap();
    std::fs::write(
        escenario.rutas.fichero_config(),
        "prefijo_escape = \"Ctrl+a\"\n",
    )
    .unwrap();
    let con_config = texto(ejecutar_binario(&escenario, true, &[]));
    assert!(
        con_config.contains("✓ teclas          la fila de MAGI (~/.termux/termux.properties)"),
        "{con_config}"
    );
}

/// Espera a que el doble `termux-battery-status` haya anotado sus dos pids (el
/// suyo y el de su hijo): ya está corriendo, y `magi` ya tiene puestas las
/// señales (las pone antes de lanzarlo).
#[cfg(debug_assertions)]
fn esperar_al_doble_colgado(dobles: &Dobles) -> Vec<i32> {
    let limite = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(llamada) = dobles.llamadas("termux-battery-status").first() {
            if llamada.pids.len() == 2 {
                return llamada.pids.clone();
            }
        }
        assert!(Instant::now() < limite, "el doble no llegó a arrancar");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Ctrl+C (y `SIGTERM`, y el cierre de la sesión) mientras `magi diagnostico`
/// espera a Termux:API no deja la orden ni sus hijos huérfanos: cada orden corre
/// en su propio grupo de procesos y la señal de la terminal solo llega al de
/// `magi`, que antes moría sin pasar por el `Drop` que mata al grupo. Ahora sale
/// al instante, sin imprimir informe, con el código de una shell para la señal.
#[cfg(debug_assertions)]
#[test]
fn una_senal_durante_la_espera_a_termux_api_mata_la_orden_y_sale_sin_informe() {
    use nix::sys::signal::{kill, Signal};
    use std::process::Stdio;

    for (senal, codigo) in [
        (Signal::SIGINT, 130),
        (Signal::SIGTERM, 143),
        (Signal::SIGHUP, 129),
    ] {
        let escenario = Escenario::nuevo();
        escenario
            .dobles
            .programar("termux-battery-status", Modo::Colgar);
        // El plazo máximo de `[termux] api_plazo_seg` (30 s): lo que para la
        // espera es la señal, no el plazo.
        std::fs::create_dir_all(&escenario.rutas.config).unwrap();
        std::fs::write(
            escenario.rutas.fichero_config(),
            "[termux]\napi_plazo_seg = 30\n",
        )
        .unwrap();
        let hijo = orden_del_binario(&escenario, true, &[])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("lanzando magi diagnostico");
        let pids = esperar_al_doble_colgado(&escenario.dobles);
        for pid in &pids {
            assert!(
                dobles::proceso_vivo(*pid),
                "{senal}: el doble {pid} no está vivo antes de la señal"
            );
        }

        let pid_de_magi = i32::try_from(hijo.id()).expect("pid de magi");
        kill(nix::unistd::Pid::from_raw(pid_de_magi), senal).expect("mandando la señal");
        let inicio = Instant::now();
        let salida = hijo.wait_with_output().expect("esperando a magi");
        let tardo = inicio.elapsed();

        let errores = String::from_utf8_lossy(&salida.stderr);
        // Salió por su cuenta con el código de la señal (no murió por ella: eso
        // sería `code() == None`) y rápido, sin esperar al plazo.
        assert_eq!(
            salida.status.code(),
            Some(codigo),
            "{senal}: {:?}\n{errores}",
            salida.status
        );
        assert!(
            tardo < Duration::from_secs(10),
            "{senal}: esperó al plazo: {tardo:?}"
        );
        // Sin informe, y diciendo qué pasó.
        assert!(
            salida.stdout.is_empty(),
            "{senal}: imprimió algo: {}",
            String::from_utf8_lossy(&salida.stdout)
        );
        assert!(
            errores.contains("comprobando Termux:API…")
                && errores.contains("diagnóstico interrumpido"),
            "{senal}: {errores}"
        );
        // Lo importante: ni la orden ni su hijo quedan vivos.
        for pid in &pids {
            assert!(
                !sigue_vivo_tras(*pid, Duration::from_secs(5)),
                "{senal}: el proceso {pid} quedó huérfano"
            );
        }
        assert_eq!(escenario.dobles.llamadas("termux-battery-status").len(), 1);
    }
}
