//! Pruebas de `magi diagnostico` con un entorno simulado.
//!
//! La recogida real del sistema se prueba aparte (no crea nada); aquí se
//! construyen los hechos a mano para simular un sistema sin portapapeles, una
//! base de datos futura, un locale ASCII o un `NO_COLOR`, sin depender
//! de la máquina que ejecuta los tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use magi::almacen::Almacen;
use magi::config::Rutas;
use magi::diagnostico::{
    comprobar, ejecutar_en, recoger, AgenteObservado, Datos, Directorios, EquipoObservado, Estado,
    EstadoDirectorio, ResultadoBaseDatos, ResultadoConfig, ResultadoEquipos, ServidorObservado,
    SincronizacionObservada,
};

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
        servidor
            .detalle
            .contains("en marcha · 1.0.0 · protocolo 6 · 2 sesiones, 1 túnel"),
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
        let almacen = Almacen::abrir(&rutas.base_datos()).expect("creando magi.db");
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
