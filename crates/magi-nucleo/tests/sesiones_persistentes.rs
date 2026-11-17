//! Pruebas del núcleo de las sesiones persistentes (Fase 16, Sprint 1, pista
//! B): validadores, nombres automáticos, estado derivado, el almacén de fichas
//! (CRUD, unicidad por host, cascada con lápida, versiones), el paso en Rust
//! de la migración 11 con su idempotencia y los tipos nuevos del registro.
//!
//! Todo va contra un almacén en memoria (o una base temporal para la
//! migración); no se abre ninguna conexión.

use magi_nucleo::almacen::versiones::{self, Tabla};
use magi_nucleo::almacen::{aviso_marca_migrada, lapidas, Almacen};
use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::modelo::{DatosHost, Origen};
use magi_nucleo::registro::{self, ResultadoRegistro};
use magi_nucleo::rutas::Equipo;
use magi_nucleo::sesiones_persistentes::estado::{
    antigua, atencion, estado_visto, sesion_de, vista_de_ficha,
};
use magi_nucleo::sesiones_persistentes::nombres::{nombre_automatico, nombre_propuesto};
use magi_nucleo::sesiones_persistentes::validadores::{
    nombre_valido, normalizar_opcional, validar_carpeta, validar_comando, validar_comando_reanudar,
    validar_datos, validar_host, validar_nombre, validar_nota, CampoFicha, ErrorFicha,
    MAXIMO_CARPETA_BYTES, MAXIMO_COMANDO_BYTES, MAXIMO_NOMBRE, MAXIMO_NOTA_CARACTERES,
};
use magi_nucleo::sesiones_persistentes::{
    ComprobacionHost, DatosFicha, ErrorComprobacion, EstadoFicha, Ficha, OrigenFicha, ServidorTmux,
    SesionVista, VersionTmux,
};

// ------------------------------------------------------------------ utilidades

fn almacen() -> Almacen {
    Almacen::abrir_en_memoria().expect("almacén en memoria")
}

fn crear_host(almacen: &Almacen, nombre: &str) -> i64 {
    almacen
        .crear_host(
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.10".to_string(),
                usuario: Some("usuario".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host ssh")
}

fn crear_host_rdp(almacen: &Almacen, nombre: &str) -> i64 {
    almacen
        .crear_host(
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.30".to_string(),
                puerto: 3389,
                tipo: TipoHost::Rdp,
                usuario: Some("operador".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host rdp")
}

fn datos_ficha(nombre: &str) -> DatosFicha {
    DatosFicha {
        nombre: nombre.to_string(),
        carpeta: Some("~/proyectos/api".to_string()),
        comando: Some("claude".to_string()),
        comando_reanudar: Some("claude --continue".to_string()),
        nota: Some("refactor del módulo de pagos".to_string()),
        origen: OrigenFicha::Manual,
        creada_por: "portatil".to_string(),
    }
}

fn ficha_minima(nombre: &str) -> DatosFicha {
    DatosFicha {
        nombre: nombre.to_string(),
        carpeta: None,
        comando: None,
        comando_reanudar: None,
        nota: None,
        origen: OrigenFicha::Host,
        creada_por: "portatil".to_string(),
    }
}

/// El `ErrorFicha` que lleva un error del almacén, si lo lleva.
fn error_de_ficha(error: &anyhow::Error) -> ErrorFicha {
    error
        .downcast_ref::<ErrorFicha>()
        .unwrap_or_else(|| panic!("no es un ErrorFicha: {error:#}"))
        .clone()
}

/// uuid, versión y vector de una fila de una tabla que viaja.
fn fila_versionada(almacen: &Almacen, tabla: Tabla, id: i64) -> (String, String, String) {
    almacen
        .conexion()
        .query_row(
            &format!(
                "SELECT uuid, version, vector FROM {} WHERE id = ?1",
                tabla.nombre()
            ),
            [id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("uuid, versión y vector")
}

fn ms_de(version: &str) -> i64 {
    versiones::ms_de_version(version).expect("versión con la forma esperada")
}

fn contar(almacen: &Almacen, tabla: &str) -> i64 {
    almacen
        .conexion()
        .query_row(&format!("SELECT COUNT(*) FROM {tabla}"), [], |fila| {
            fila.get(0)
        })
        .unwrap()
}

/// Inserta una fila saltándose el almacén, como la dejaría otro proceso o una
/// sincronización defectuosa.
#[allow(clippy::too_many_arguments)]
fn insertar_cruda(
    almacen: &Almacen,
    host_id: i64,
    nombre: &str,
    uuid: Option<&str>,
    origen: &str,
    carpeta: Option<&str>,
    comando: Option<&str>,
    nota: Option<&str>,
) {
    almacen
        .conexion()
        .execute(
            "INSERT INTO SESIONES_PERSISTENTES
                (host_id, nombre, carpeta, comando, nota, origen, creada_por,
                 creada_en, actualizado_en, uuid, version, vector)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'portatil',
                     '2026-11-16T10:00:00+01:00', '2026-11-16T10:00:00+01:00',
                     ?7, '0000000000001.aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '{}')",
            rusqlite::params![host_id, nombre, carpeta, comando, nota, origen, uuid],
        )
        .expect("fila cruda");
}

// ------------------------------------------------------------------ validadores

#[test]
fn el_nombre_admite_letras_numeros_guion_y_guion_bajo_de_1_a_64() {
    for valido in [
        "a",
        "Z",
        "9",
        "agente-api",
        "agente_api",
        "A-z_9",
        "portatil-10",
        &"a".repeat(MAXIMO_NOMBRE),
    ] {
        assert_eq!(validar_nombre(valido), Ok(()), "«{valido}» debe valer");
        assert!(nombre_valido(valido));
    }
    assert_eq!(validar_nombre(""), Err(ErrorFicha::NombreVacio));
    assert_eq!(
        validar_nombre(&"a".repeat(MAXIMO_NOMBRE + 1)),
        Err(ErrorFicha::NombreLargo { largo: 65 })
    );
    for (nombre, caracter) in [
        ("con espacio", ' '),
        ("tab\taqui", '\t'),
        ("linea\nnueva", '\n'),
        ("ñandú", 'ñ'),
        ("a/b", '/'),
        ("a.b", '.'),
        ("a:b", ':'),
        ("=nombre", '='),
        ("a\0b", '\0'),
        ("emoji-🙂", '🙂'),
        ("a;rm", ';'),
        ("$(x)", '$'),
    ] {
        assert_eq!(
            validar_nombre(nombre),
            Err(ErrorFicha::NombreCaracter { caracter }),
            "«{nombre:?}»"
        );
        assert!(!nombre_valido(nombre));
    }
}

#[test]
fn la_carpeta_es_absoluta_o_empieza_por_virgula_barra_y_tiene_limites() {
    for valida in [
        "",
        "/",
        "/home/usuario",
        "/home/usuario/con espacios/ñandú",
        "~/",
        "~/proyectos/api",
    ] {
        assert_eq!(validar_carpeta(valida), Ok(()), "«{valida}» debe valer");
    }
    for relativa in ["proyectos", "./api", "../api", "~", "~usuario/api", " /x"] {
        assert_eq!(
            validar_carpeta(relativa),
            Err(ErrorFicha::CarpetaNoAbsoluta),
            "«{relativa}»"
        );
    }
    assert_eq!(
        validar_carpeta("/a\nb"),
        Err(ErrorFicha::CarpetaConSaltoDeLinea)
    );
    assert_eq!(
        validar_carpeta("/a\rb"),
        Err(ErrorFicha::CarpetaConSaltoDeLinea)
    );
    assert_eq!(validar_carpeta("/a\0b"), Err(ErrorFicha::CarpetaConNulo));
    // El límite es de 1024 bytes, no de caracteres.
    let justa = format!("/{}", "a".repeat(MAXIMO_CARPETA_BYTES - 1));
    assert_eq!(justa.len(), MAXIMO_CARPETA_BYTES);
    assert_eq!(validar_carpeta(&justa), Ok(()));
    let larga = format!("/{}", "a".repeat(MAXIMO_CARPETA_BYTES));
    assert_eq!(
        validar_carpeta(&larga),
        Err(ErrorFicha::CarpetaLarga { bytes: 1025 })
    );
    let multibyte = format!("/{}", "é".repeat(512));
    assert_eq!(multibyte.chars().count(), 513);
    assert_eq!(
        validar_carpeta(&multibyte),
        Err(ErrorFicha::CarpetaLarga { bytes: 1025 }),
        "cuentan los bytes"
    );
}

#[test]
fn el_comando_y_el_de_reanudacion_admiten_hasta_4096_bytes_sin_nulos() {
    for valido in [
        "",
        "claude",
        "claude --continue",
        "cd ~/api && claude -p 'hola; adiós' | tee /tmp/x",
        "echo \"$HOME\"\nls",
        &"a".repeat(MAXIMO_COMANDO_BYTES),
    ] {
        assert_eq!(validar_comando(valido), Ok(()));
        assert_eq!(validar_comando_reanudar(valido), Ok(()));
    }
    let largo = "a".repeat(MAXIMO_COMANDO_BYTES + 1);
    assert_eq!(
        validar_comando(&largo),
        Err(ErrorFicha::ComandoLargo {
            campo: CampoFicha::Comando,
            bytes: 4097
        })
    );
    assert_eq!(
        validar_comando_reanudar(&largo),
        Err(ErrorFicha::ComandoLargo {
            campo: CampoFicha::ComandoReanudar,
            bytes: 4097
        })
    );
    // Bytes, no caracteres: 2049 «é» son 4098 bytes.
    assert!(validar_comando(&"é".repeat(2049)).is_err());
    assert_eq!(validar_comando(&"é".repeat(2048)), Ok(()));
    assert_eq!(
        validar_comando("claude\0"),
        Err(ErrorFicha::ComandoConNulo {
            campo: CampoFicha::Comando
        })
    );
    assert_eq!(
        validar_comando_reanudar("a\0b"),
        Err(ErrorFicha::ComandoConNulo {
            campo: CampoFicha::ComandoReanudar
        })
    );
}

#[test]
fn la_nota_admite_hasta_500_caracteres() {
    assert_eq!(validar_nota(""), Ok(()));
    assert_eq!(validar_nota(&"a".repeat(MAXIMO_NOTA_CARACTERES)), Ok(()));
    // Cuentan caracteres, no bytes: 500 «ñ» son 1000 bytes y valen.
    assert_eq!(validar_nota(&"ñ".repeat(MAXIMO_NOTA_CARACTERES)), Ok(()));
    assert_eq!(
        validar_nota(&"a".repeat(MAXIMO_NOTA_CARACTERES + 1)),
        Err(ErrorFicha::NotaLarga { caracteres: 501 })
    );
}

#[test]
fn solo_los_hosts_ssh_tienen_sesiones_persistentes() {
    assert_eq!(validar_host(TipoHost::Ssh), Ok(()));
    for tipo in [TipoHost::Rdp, TipoHost::Vnc, TipoHost::Voxl] {
        assert_eq!(validar_host(tipo), Err(ErrorFicha::HostNoSsh { tipo }));
    }
}

#[test]
fn los_errores_dicen_el_campo_y_el_motivo_en_castellano() {
    let casos = [
        (validar_nombre("").unwrap_err(), CampoFicha::Nombre, "vacío"),
        (
            validar_nombre("a b").unwrap_err(),
            CampoFicha::Nombre,
            "espacios",
        ),
        (validar_nombre("ñ").unwrap_err(), CampoFicha::Nombre, "«ñ»"),
        (
            validar_nombre(&"a".repeat(70)).unwrap_err(),
            CampoFicha::Nombre,
            "64",
        ),
        (
            validar_carpeta("api").unwrap_err(),
            CampoFicha::Carpeta,
            "absoluta",
        ),
        (
            validar_carpeta("/a\nb").unwrap_err(),
            CampoFicha::Carpeta,
            "saltos de línea",
        ),
        (
            validar_comando(&"a".repeat(5000)).unwrap_err(),
            CampoFicha::Comando,
            "4096",
        ),
        (
            validar_comando_reanudar("\0").unwrap_err(),
            CampoFicha::ComandoReanudar,
            "reanudación",
        ),
        (
            validar_nota(&"a".repeat(501)).unwrap_err(),
            CampoFicha::Nota,
            "500",
        ),
        (
            validar_host(TipoHost::Vnc).unwrap_err(),
            CampoFicha::Host,
            "VNC",
        ),
        (
            ErrorFicha::NombreEnUso {
                nombre: "x".to_string(),
            },
            CampoFicha::Nombre,
            "ya hay",
        ),
    ];
    for (error, campo, fragmento) in casos {
        assert_eq!(error.campo(), campo, "{error}");
        let mensaje = error.to_string();
        assert!(mensaje.contains(fragmento), "«{mensaje}» sin «{fragmento}»");
        assert!(!mensaje.is_empty());
    }
    assert_eq!(CampoFicha::ComandoReanudar.etiqueta(), "Reanudar");
    // Un nombre con un carácter de control no se pinta tal cual.
    let mensaje = validar_nombre("a\u{1b}b").unwrap_err().to_string();
    assert!(!mensaje.contains('\u{1b}'), "{mensaje:?}");
}

#[test]
fn validar_datos_normaliza_y_devuelve_el_primer_error() {
    let normalizados = validar_datos(&DatosFicha {
        nombre: "  agente-api  ".to_string(),
        carpeta: Some("  ~/api  ".to_string()),
        comando: Some("   ".to_string()),
        comando_reanudar: Some("".to_string()),
        nota: Some(" una nota ".to_string()),
        origen: OrigenFicha::Descubierta,
        creada_por: " portatil ".to_string(),
    })
    .expect("datos válidos");
    assert_eq!(normalizados.nombre, "agente-api");
    assert_eq!(normalizados.carpeta.as_deref(), Some("~/api"));
    assert_eq!(normalizados.comando, None, "el vacío es sin valor");
    assert_eq!(normalizados.comando_reanudar, None);
    assert_eq!(normalizados.nota.as_deref(), Some("una nota"));
    assert_eq!(normalizados.origen, OrigenFicha::Descubierta);
    assert_eq!(normalizados.creada_por, "portatil");

    // El primer error es el del orden del diálogo: nombre, carpeta, comando...
    let mut datos = datos_ficha("bien");
    datos.nombre = "mal nombre".to_string();
    datos.carpeta = Some("relativa".to_string());
    assert_eq!(
        validar_datos(&datos).unwrap_err().campo(),
        CampoFicha::Nombre
    );
    datos.nombre = "bien".to_string();
    assert_eq!(
        validar_datos(&datos).unwrap_err().campo(),
        CampoFicha::Carpeta
    );
    datos.carpeta = None;
    datos.creada_por = "  ".to_string();
    assert_eq!(validar_datos(&datos), Err(ErrorFicha::EquipoVacio));

    assert_eq!(normalizar_opcional(None), None);
    assert_eq!(normalizar_opcional(Some("  ")), None);
    assert_eq!(normalizar_opcional(Some(" a ")).as_deref(), Some("a"));
}

// ------------------------------------------------------------------ nombres

fn lista(nombres: &[&str]) -> Vec<String> {
    nombres.iter().map(|nombre| nombre.to_string()).collect()
}

#[test]
fn el_nombre_automatico_es_etiqueta_guion_primer_numero_libre() {
    assert_eq!(nombre_automatico("portatil", &[], &[]), "portatil-1");
    // Los huecos se aprovechan: el primer libre, no el siguiente al mayor.
    assert_eq!(
        nombre_automatico("portatil", &lista(&["portatil-1", "portatil-3"]), &[]),
        "portatil-2"
    );
    // Las fichas (también las terminadas) reservan su nombre.
    assert_eq!(
        nombre_automatico("portatil", &[], &lista(&["portatil-1", "portatil-2"])),
        "portatil-3"
    );
    // Entre las sesiones del host y sus fichas a la vez.
    assert_eq!(
        nombre_automatico(
            "portatil",
            &lista(&["portatil-1"]),
            &lista(&["portatil-2", "otra"])
        ),
        "portatil-3"
    );
    // Los nombres de otro equipo no cuentan.
    assert_eq!(
        nombre_automatico("portatil", &lista(&["sobremesa-1", "portatil"]), &[]),
        "portatil-1"
    );
}

#[test]
fn el_nombre_automatico_compara_exacto_y_no_por_prefijo() {
    // magi-a-1 y magi-a-10 conviven: un prefijo no reserva el otro nombre.
    let diez: Vec<String> = (1..=9).map(|n| format!("magi-a-{n}")).collect();
    assert_eq!(nombre_automatico("magi-a", &diez, &[]), "magi-a-10");
    let once: Vec<String> = (1..=10).map(|n| format!("magi-a-{n}")).collect();
    assert_eq!(nombre_automatico("magi-a", &once, &[]), "magi-a-11");
    assert_eq!(
        nombre_automatico("magi-a", &lista(&["magi-a-10"]), &[]),
        "magi-a-1",
        "magi-a-10 no ocupa magi-a-1"
    );
    // Las mayúsculas son otro nombre en tmux.
    assert_eq!(
        nombre_automatico("portatil", &lista(&["PORTATIL-1"]), &[]),
        "portatil-1"
    );
}

#[test]
fn la_etiqueta_del_equipo_se_normaliza_y_el_nombre_siempre_es_valido() {
    assert_eq!(
        nombre_automatico("Portátil de Ángel", &[], &[]),
        "portatil-de-angel-1"
    );
    assert_eq!(nombre_automatico("", &[], &[]), "equipo-1");
    let larga = nombre_automatico(&"x".repeat(100), &[], &[]);
    assert_eq!(larga, format!("{}-1", "x".repeat(20)));
    for etiqueta in ["portatil", "Sobremesa 2", "ñ", "a/b", "....", "web-01"] {
        let nombre = nombre_automatico(etiqueta, &[], &[]);
        assert!(nombre_valido(&nombre), "«{nombre}» de «{etiqueta}»");
    }
}

#[test]
fn el_nombre_propuesto_sale_de_la_carpeta_o_es_sesion_n() {
    assert_eq!(nombre_propuesto("~/proyectos/api", &[]), "api");
    assert_eq!(
        nombre_propuesto("/srv/web/", &[]),
        "web",
        "sin la barra final"
    );
    assert_eq!(nombre_propuesto("~/proyectos/mi api!", &[]), "mi-api");
    assert_eq!(nombre_propuesto("/home/usuario/ñandú", &[]), "and");
    assert_eq!(nombre_propuesto("/a/b__c-d", &[]), "b__c-d");
    // Sin carpeta o sin nada utilizable: sesion-<n>.
    for sin_nombre in ["", "/", "~", "/x/--", "/x/...", "/x/ñ"] {
        assert_eq!(
            nombre_propuesto(sin_nombre, &[]),
            "sesion-1",
            "«{sin_nombre}»"
        );
    }
    // Si ya hay una ficha con ese nombre, sesion-<n>, con el primer n libre.
    assert_eq!(
        nombre_propuesto("~/proyectos/api", &lista(&["api"])),
        "sesion-1"
    );
    assert_eq!(
        nombre_propuesto("~/proyectos/api", &lista(&["api", "sesion-1", "sesion-2"])),
        "sesion-3"
    );
    assert_eq!(
        nombre_propuesto("", &lista(&["sesion-1"])),
        "sesion-2",
        "las fichas terminadas reservan su nombre"
    );
    // Recorta a 64 caracteres y sigue siendo válido.
    let propuesto = nombre_propuesto(&format!("/x/{}", "a".repeat(200)), &[]);
    assert_eq!(propuesto.len(), 64);
    assert!(nombre_valido(&propuesto));
    for carpeta in ["~/a b/c d", "/x/\u{1b}[31m", "/tmp/.hidden", "/x/日本語"] {
        let propuesto = nombre_propuesto(carpeta, &[]);
        assert!(nombre_valido(&propuesto), "«{propuesto}» de «{carpeta}»");
    }
}

// ------------------------------------------------------------------ estado

const AHORA: i64 = 1_800_000_000;
const DIA: i64 = 86_400;

fn ficha(host_id: i64, nombre: &str) -> Ficha {
    Ficha {
        id: 1,
        uuid: "11111111-1111-4111-8111-111111111111".to_string(),
        host_id,
        nombre: nombre.to_string(),
        carpeta: None,
        comando: None,
        comando_reanudar: None,
        nota: None,
        origen: OrigenFicha::Manual,
        creada_por: "portatil".to_string(),
        creada_en: "2026-11-16T10:00:00+01:00".to_string(),
        actualizado_en: "2026-11-16T10:00:00+01:00".to_string(),
    }
}

fn sesion(nombre: &str, clientes: u32) -> SesionVista {
    SesionVista {
        servidor: ServidorTmux::Magi,
        nombre: nombre.to_string(),
        clientes,
        equipo: None,
        actividad: AHORA - 60,
        creada: AHORA - 3600,
        campana: false,
        ventanas: 1,
        titulo: None,
    }
}

fn comprobacion(host_id: i64, sesiones: Vec<SesionVista>) -> ComprobacionHost {
    ComprobacionHost {
        host_id,
        tmux: Some(VersionTmux::nueva(3, 4)),
        ahora: AHORA,
        sesiones,
        error: None,
    }
}

#[test]
fn sin_comprobacion_la_ficha_esta_sin_comprobar() {
    let f = ficha(1, "agente-api");
    assert_eq!(estado_visto(&f, None), EstadoFicha::SinComprobar);
    // La comprobación de otro host no vale, ni aunque salga el nombre.
    let ajena = comprobacion(2, vec![sesion("agente-api", 1)]);
    assert_eq!(estado_visto(&f, Some(&ajena)), EstadoFicha::SinComprobar);
    let vista = vista_de_ficha(&f, None, 7);
    assert_eq!(vista.estado, EstadoFicha::SinComprobar);
    assert!(!vista.atencion && !vista.antigua);
    assert_eq!(vista.titulo, None);
}

#[test]
fn una_sesion_que_no_sale_en_un_host_que_responde_esta_terminada() {
    let f = ficha(1, "agente-api");
    let host = comprobacion(1, vec![sesion("otra", 0), sesion("agente-api-2", 1)]);
    assert_eq!(estado_visto(&f, Some(&host)), EstadoFicha::Terminada);
    // Sin ninguna sesión también.
    let vacio = comprobacion(1, Vec::new());
    assert_eq!(estado_visto(&f, Some(&vacio)), EstadoFicha::Terminada);
}

#[test]
fn si_el_host_no_responde_la_ficha_es_inalcanzable_nunca_terminada() {
    let f = ficha(1, "agente-api");
    for error in [
        ErrorComprobacion::SinRespuesta,
        ErrorComprobacion::NecesitaDialogo,
        ErrorComprobacion::Plazo,
    ] {
        // Con la lista vacía (lo que parece «terminada») y con la sesión
        // a la vista: el error manda siempre.
        for sesiones in [Vec::new(), vec![sesion("agente-api", 1)]] {
            let mut host = comprobacion(1, sesiones);
            host.error = Some(error);
            assert_eq!(
                estado_visto(&f, Some(&host)),
                EstadoFicha::Inalcanzable,
                "{error:?}"
            );
        }
    }
    let mut host = comprobacion(1, vec![sesion("agente-api", 1)]);
    host.error = Some(ErrorComprobacion::Plazo);
    let vista = vista_de_ficha(&f, Some(&host), 7);
    assert_eq!(vista.estado, EstadoFicha::Inalcanzable);
    assert!(!vista.atencion, "sin comprobar no hay atención que avisar");
    assert_eq!(vista.clientes, 0);
}

#[test]
fn sin_tmux_o_anterior_a_3_0_la_ficha_esta_sin_tmux() {
    let f = ficha(1, "agente-api");
    let mut host = comprobacion(1, vec![sesion("agente-api", 1)]);
    host.tmux = None;
    assert_eq!(estado_visto(&f, Some(&host)), EstadoFicha::SinTmux);
    for (mayor, menor) in [(1, 8), (2, 0), (2, 9)] {
        host.tmux = Some(VersionTmux::nueva(mayor, menor));
        assert_eq!(
            estado_visto(&f, Some(&host)),
            EstadoFicha::SinTmux,
            "tmux {mayor}.{menor}"
        );
    }
    // La 3.0 ya vale, y las posteriores.
    for (mayor, menor) in [(3, 0), (3, 4), (4, 0)] {
        host.tmux = Some(VersionTmux::nueva(mayor, menor));
        assert_eq!(estado_visto(&f, Some(&host)), EstadoFicha::VivaEnganchada);
    }
}

#[test]
fn con_clientes_esta_enganchada_y_sin_ellos_libre() {
    let f = ficha(1, "agente-api");
    let libre = comprobacion(1, vec![sesion("agente-api", 0)]);
    assert_eq!(estado_visto(&f, Some(&libre)), EstadoFicha::VivaLibre);
    let enganchada = comprobacion(1, vec![sesion("agente-api", 2)]);
    assert_eq!(
        estado_visto(&f, Some(&enganchada)),
        EstadoFicha::VivaEnganchada
    );
}

#[test]
fn la_ficha_se_busca_por_nombre_exacto_en_el_servidor_de_magi() {
    let f = ficha(1, "agente-api");
    // Un prefijo no es la sesión (tmux casa prefijos en -t; aquí nunca).
    let prefijo = comprobacion(1, vec![sesion("agente-api-2", 1), sesion("agente", 1)]);
    assert_eq!(estado_visto(&f, Some(&prefijo)), EstadoFicha::Terminada);
    assert!(sesion_de("agente-api", &prefijo).is_none());
    // Las mayúsculas son otro nombre.
    let mayusculas = comprobacion(1, vec![sesion("AGENTE-API", 1)]);
    assert_eq!(estado_visto(&f, Some(&mayusculas)), EstadoFicha::Terminada);
    // Una sesión del tmux habitual con el mismo nombre no es la ficha.
    let mut habitual = sesion("agente-api", 1);
    habitual.servidor = ServidorTmux::Habitual;
    let solo_habitual = comprobacion(1, vec![habitual.clone()]);
    assert_eq!(
        estado_visto(&f, Some(&solo_habitual)),
        EstadoFicha::Terminada
    );
    // Con las dos, la de MAGI.
    let mut propia = sesion("agente-api", 0);
    propia.titulo = Some("propia".to_string());
    let ambas = comprobacion(1, vec![habitual, propia]);
    assert_eq!(estado_visto(&f, Some(&ambas)), EstadoFicha::VivaLibre);
    assert_eq!(
        sesion_de("agente-api", &ambas).and_then(|s| s.titulo.as_deref()),
        Some("propia")
    );
}

#[test]
fn la_vista_lleva_atencion_titulo_equipo_y_actividad() {
    let f = ficha(1, "agente-api");
    let mut viva = sesion("agente-api", 1);
    viva.campana = true;
    viva.titulo = Some("claude: refactor".to_string());
    viva.equipo = Some("sobremesa".to_string());
    viva.actividad = AHORA - 120;
    let host = comprobacion(1, vec![viva.clone()]);
    let vista = vista_de_ficha(&f, Some(&host), 7);
    assert_eq!(vista.estado, EstadoFicha::VivaEnganchada);
    assert!(vista.atencion);
    assert_eq!(vista.titulo.as_deref(), Some("claude: refactor"));
    assert_eq!(vista.equipo.as_deref(), Some("sobremesa"));
    assert_eq!(vista.clientes, 1);
    assert_eq!(vista.actividad, Some(AHORA - 120));
    assert!(!vista.antigua);

    // La campana sin nadie enganchado también es atención.
    viva.clientes = 0;
    assert!(atencion(Some(&viva)));
    assert!(!atencion(None));
    viva.campana = false;
    assert!(!atencion(Some(&viva)));

    // Una terminada no tiene atención, título ni actividad.
    let terminada = vista_de_ficha(&ficha(1, "otra"), Some(&host), 7);
    assert_eq!(terminada.estado, EstadoFicha::Terminada);
    assert!(!terminada.atencion && !terminada.antigua);
    assert_eq!(terminada.titulo, None);
    assert_eq!(terminada.actividad, None);
}

#[test]
fn antigua_compara_la_actividad_con_la_hora_del_host_y_antigua_dias() {
    let mut viva = sesion("agente-api", 0);
    // Justo en el límite: «a 7 días o más».
    viva.actividad = AHORA - 7 * DIA;
    assert!(antigua(Some(&viva), AHORA, 7));
    viva.actividad = AHORA - 7 * DIA + 1;
    assert!(!antigua(Some(&viva), AHORA, 7));
    assert!(antigua(Some(&viva), AHORA, 1));
    viva.actividad = AHORA - 400 * DIA;
    assert!(antigua(Some(&viva), AHORA, 365));
    assert!(!antigua(None, AHORA, 7), "sin sesión no hay antigüedad");
    // Una actividad en el futuro (relojes desajustados) nunca es antigua.
    viva.actividad = AHORA + 10 * DIA;
    assert!(!antigua(Some(&viva), AHORA, 1));
    // Valores extremos: ni desbordan ni entran en pánico.
    assert!(!antigua(Some(&viva), i64::MIN, u64::MAX));
    viva.actividad = i64::MIN;
    assert!(!antigua(Some(&viva), i64::MAX, u64::MAX));
    assert!(antigua(Some(&viva), i64::MAX, 1));

    // La hora que cuenta es la del host de la comprobación, no la del equipo.
    let f = ficha(1, "agente-api");
    let mut vieja = sesion("agente-api", 0);
    vieja.actividad = 1_000;
    let mut host = comprobacion(1, vec![vieja]);
    host.ahora = 1_000 + 8 * DIA;
    assert!(vista_de_ficha(&f, Some(&host), 7).antigua);
    host.ahora = 1_000 + 6 * DIA;
    assert!(!vista_de_ficha(&f, Some(&host), 7).antigua);
}

// ------------------------------------------------------------------ almacén

#[test]
fn crear_una_ficha_rellena_uuid_version_vector_y_fechas() {
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    let id = almacen
        .crear_ficha(host, &datos_ficha("agente-api"))
        .expect("crear la ficha");

    let (uuid, version, vector) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    assert!(
        uuid::Uuid::parse_str(&uuid).is_ok(),
        "uuid de verdad: {uuid}"
    );
    let equipo = magi_nucleo::almacen::equipos::este(almacen.conexion()).unwrap();
    assert_eq!(
        versiones::uuid_de_version(&version),
        Some(equipo.uuid.as_str())
    );
    let vector = versiones::vector_desde_json(&vector);
    assert_eq!(vector.get(&equipo.uuid), Some(&ms_de(&version)));

    let ficha = almacen.obtener_ficha(id).unwrap().expect("la ficha");
    assert_eq!(ficha.id, id);
    assert_eq!(ficha.uuid, uuid);
    assert_eq!(ficha.host_id, host);
    assert_eq!(ficha.nombre, "agente-api");
    assert_eq!(ficha.carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(ficha.comando.as_deref(), Some("claude"));
    assert_eq!(ficha.comando_reanudar.as_deref(), Some("claude --continue"));
    assert_eq!(ficha.nota.as_deref(), Some("refactor del módulo de pagos"));
    assert_eq!(ficha.origen, OrigenFicha::Manual);
    assert_eq!(ficha.creada_por, "portatil");
    for fecha in [&ficha.creada_en, &ficha.actualizado_en] {
        chrono::DateTime::parse_from_rfc3339(fecha)
            .unwrap_or_else(|error| panic!("«{fecha}» no es ISO 8601: {error}"));
    }
    assert_eq!(ficha.creada_en, ficha.actualizado_en);

    // Una ficha sin campos opcionales los guarda como NULL, no como vacío.
    let minima = almacen.crear_ficha(host, &ficha_minima("minima")).unwrap();
    let minima = almacen.obtener_ficha(minima).unwrap().unwrap();
    assert_eq!(
        (
            &minima.carpeta,
            &minima.comando,
            &minima.comando_reanudar,
            &minima.nota
        ),
        (&None, &None, &None, &None)
    );
    assert_eq!(minima.origen, OrigenFicha::Host);
    let nulos: i64 = almacen
        .conexion()
        .query_row(
            "SELECT COUNT(*) FROM SESIONES_PERSISTENTES
              WHERE nombre = 'minima' AND carpeta IS NULL AND comando IS NULL
                AND comando_reanudar IS NULL AND nota IS NULL",
            [],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(nulos, 1);
    // Cada ficha, su uuid.
    let (otro_uuid, ..) = fila_versionada(&almacen, Tabla::SesionesPersistentes, minima.id);
    assert_ne!(otro_uuid, uuid);
}

#[test]
fn el_origen_y_los_datos_se_validan_en_codigo_al_crear() {
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    for origen in [
        OrigenFicha::Manual,
        OrigenFicha::Host,
        OrigenFicha::Descubierta,
    ] {
        let mut datos = ficha_minima(&format!("de-{}", origen.texto()));
        datos.origen = origen;
        let id = almacen.crear_ficha(host, &datos).unwrap();
        assert_eq!(almacen.obtener_ficha(id).unwrap().unwrap().origen, origen);
    }
    for (datos, campo) in [
        (ficha_minima("nombre malo"), CampoFicha::Nombre),
        (ficha_minima(""), CampoFicha::Nombre),
        (
            DatosFicha {
                carpeta: Some("relativa".to_string()),
                ..ficha_minima("a")
            },
            CampoFicha::Carpeta,
        ),
        (
            DatosFicha {
                comando: Some("x".repeat(4097)),
                ..ficha_minima("a")
            },
            CampoFicha::Comando,
        ),
        (
            DatosFicha {
                comando_reanudar: Some("a\0".to_string()),
                ..ficha_minima("a")
            },
            CampoFicha::ComandoReanudar,
        ),
        (
            DatosFicha {
                nota: Some("n".repeat(501)),
                ..ficha_minima("a")
            },
            CampoFicha::Nota,
        ),
        (
            DatosFicha {
                creada_por: " ".to_string(),
                ..ficha_minima("a")
            },
            CampoFicha::Equipo,
        ),
    ] {
        let error = almacen.crear_ficha(host, &datos).unwrap_err();
        assert_eq!(error_de_ficha(&error).campo(), campo, "{error:#}");
    }
    assert_eq!(
        contar(&almacen, "SESIONES_PERSISTENTES"),
        3,
        "solo las válidas"
    );
}

#[test]
fn solo_los_hosts_ssh_tienen_fichas() {
    let almacen = almacen();
    let rdp = crear_host_rdp(&almacen, "escritorio");
    let error = almacen
        .crear_ficha(rdp, &ficha_minima("a"))
        .expect_err("un host rdp no tiene fichas");
    assert_eq!(
        error_de_ficha(&error),
        ErrorFicha::HostNoSsh {
            tipo: TipoHost::Rdp
        }
    );
    let error = almacen
        .crear_ficha(9999, &ficha_minima("a"))
        .expect_err("un host que no existe tampoco");
    assert_eq!(
        error_de_ficha(&error),
        ErrorFicha::HostNoExiste { host_id: 9999 }
    );
    assert_eq!(contar(&almacen, "SESIONES_PERSISTENTES"), 0);
}

#[test]
fn el_nombre_es_unico_por_host_pero_no_entre_hosts() {
    let almacen = almacen();
    let uno = crear_host(&almacen, "dev-01");
    let dos = crear_host(&almacen, "dev-02");
    almacen
        .crear_ficha(uno, &datos_ficha("agente-api"))
        .unwrap();
    let error = almacen
        .crear_ficha(uno, &datos_ficha("agente-api"))
        .expect_err("el mismo nombre en el mismo host");
    assert_eq!(
        error_de_ficha(&error),
        ErrorFicha::NombreEnUso {
            nombre: "agente-api".to_string()
        }
    );
    assert_eq!(error_de_ficha(&error).campo(), CampoFicha::Nombre);
    // En otro host sí, y las mayúsculas son otro nombre.
    almacen
        .crear_ficha(dos, &datos_ficha("agente-api"))
        .unwrap();
    almacen
        .crear_ficha(uno, &datos_ficha("Agente-Api"))
        .unwrap();
    assert_eq!(contar(&almacen, "SESIONES_PERSISTENTES"), 3);
    // El intento fallido no dejó rastro: ni fila ni versión gastada de más.
    assert_eq!(almacen.fichas_de_host(uno).unwrap().len(), 2);
}

#[test]
fn listar_y_buscar_por_host_y_nombre_exacto() {
    let almacen = almacen();
    let uno = crear_host(&almacen, "dev-01");
    let dos = crear_host(&almacen, "dev-02");
    for nombre in ["agente-api", "agente-api-2", "base"] {
        almacen.crear_ficha(uno, &ficha_minima(nombre)).unwrap();
    }
    almacen.crear_ficha(dos, &ficha_minima("otra")).unwrap();

    let todas: Vec<(i64, String)> = almacen
        .listar_fichas()
        .unwrap()
        .into_iter()
        .map(|f| (f.host_id, f.nombre))
        .collect();
    assert_eq!(
        todas,
        vec![
            (uno, "agente-api".to_string()),
            (uno, "agente-api-2".to_string()),
            (uno, "base".to_string()),
            (dos, "otra".to_string()),
        ],
        "por host y nombre"
    );
    let del_uno: Vec<String> = almacen
        .fichas_de_host(uno)
        .unwrap()
        .into_iter()
        .map(|f| f.nombre)
        .collect();
    assert_eq!(del_uno, ["agente-api", "agente-api-2", "base"]);
    assert_eq!(almacen.fichas_de_host(dos).unwrap().len(), 1);
    assert!(almacen.fichas_de_host(9999).unwrap().is_empty());

    let exacta = almacen
        .ficha_por_nombre(uno, "agente-api")
        .unwrap()
        .unwrap();
    assert_eq!(exacta.nombre, "agente-api", "no la de nombre mayor");
    assert!(almacen.ficha_por_nombre(uno, "agente").unwrap().is_none());
    assert!(almacen
        .ficha_por_nombre(uno, "AGENTE-API")
        .unwrap()
        .is_none());
    assert!(almacen
        .ficha_por_nombre(dos, "agente-api")
        .unwrap()
        .is_none());
    assert!(almacen.obtener_ficha(9999).unwrap().is_none());

    assert_eq!(
        almacen.nombres_de_fichas_de_host(uno).unwrap(),
        ["agente-api", "agente-api-2", "base"]
    );
}

#[test]
fn actualizar_una_ficha_sube_la_version_solo_si_algo_cambia() {
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    let id = almacen
        .crear_ficha(host, &datos_ficha("agente-api"))
        .unwrap();
    let (uuid0, version0, vector0) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    let ficha0 = almacen.obtener_ficha(id).unwrap().unwrap();

    // Sin cambios: ni versión ni fecha.
    almacen
        .actualizar_ficha(id, &datos_ficha("agente-api"))
        .unwrap();
    assert_eq!(
        fila_versionada(&almacen, Tabla::SesionesPersistentes, id),
        (uuid0.clone(), version0.clone(), vector0.clone())
    );
    assert_eq!(almacen.obtener_ficha(id).unwrap().unwrap(), ficha0);

    // Con un cambio, versión nueva (mayor), mismo uuid y fecha de edición.
    let mut datos = datos_ficha("agente-api");
    datos.comando_reanudar = Some("claude --resume".to_string());
    datos.nota = None;
    // El origen y la creadora no se editan: se ignoran.
    datos.origen = OrigenFicha::Descubierta;
    datos.creada_por = "otro-equipo".to_string();
    std::thread::sleep(std::time::Duration::from_millis(5));
    almacen.actualizar_ficha(id, &datos).unwrap();
    let (uuid1, version1, vector1) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    assert_eq!(uuid1, uuid0);
    assert_ne!(version1, version0);
    assert!(
        ms_de(&version1) > ms_de(&version0),
        "{version1} > {version0}"
    );
    let equipo = magi_nucleo::almacen::equipos::este(almacen.conexion()).unwrap();
    assert_eq!(
        versiones::vector_desde_json(&vector1).get(&equipo.uuid),
        Some(&ms_de(&version1))
    );
    assert_ne!(vector1, vector0);
    let ficha1 = almacen.obtener_ficha(id).unwrap().unwrap();
    assert_eq!(ficha1.comando_reanudar.as_deref(), Some("claude --resume"));
    assert_eq!(ficha1.nota, None);
    assert_eq!(ficha1.origen, OrigenFicha::Manual, "el origen no se edita");
    assert_eq!(ficha1.creada_por, "portatil", "la creadora no se edita");
    assert_eq!(ficha1.creada_en, ficha0.creada_en);
    assert_eq!(ficha1.id, id);

    // Una ficha inexistente o unos datos inválidos no escriben nada.
    let error = almacen.actualizar_ficha(9999, &datos).unwrap_err();
    assert_eq!(
        error_de_ficha(&error),
        ErrorFicha::FichaNoExiste { id: 9999 }
    );
    let mut malos = datos.clone();
    malos.carpeta = Some("relativa".to_string());
    assert!(almacen.actualizar_ficha(id, &malos).is_err());
    assert_eq!(
        fila_versionada(&almacen, Tabla::SesionesPersistentes, id).1,
        version1
    );
}

#[test]
fn renombrar_la_ficha_cambia_solo_el_nombre_y_sube_la_version() {
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    let id = almacen.crear_ficha(host, &datos_ficha("viejo")).unwrap();
    let otra = almacen.crear_ficha(host, &datos_ficha("ocupado")).unwrap();
    let (uuid0, version0, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    let antes = almacen.obtener_ficha(id).unwrap().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(5));
    almacen.renombrar_ficha(id, "nuevo").unwrap();
    let despues = almacen.obtener_ficha(id).unwrap().unwrap();
    assert_eq!(despues.nombre, "nuevo");
    assert_eq!(despues.carpeta, antes.carpeta);
    assert_eq!(despues.comando, antes.comando);
    assert_eq!(despues.comando_reanudar, antes.comando_reanudar);
    assert_eq!(despues.nota, antes.nota);
    assert_eq!(despues.uuid, uuid0, "es la misma ficha");
    let (_, version1, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    assert!(ms_de(&version1) > ms_de(&version0));
    assert!(almacen.ficha_por_nombre(host, "viejo").unwrap().is_none());

    // El mismo nombre no hace nada.
    almacen.renombrar_ficha(id, "nuevo").unwrap();
    assert_eq!(
        fila_versionada(&almacen, Tabla::SesionesPersistentes, id).1,
        version1
    );

    // Un nombre ocupado por otra ficha del host o inválido se rechaza sin
    // tocar nada; el de otro host no estorba.
    let error = almacen.renombrar_ficha(id, "ocupado").unwrap_err();
    assert_eq!(
        error_de_ficha(&error),
        ErrorFicha::NombreEnUso {
            nombre: "ocupado".to_string()
        }
    );
    assert_eq!(
        error_de_ficha(&almacen.renombrar_ficha(id, "con espacio").unwrap_err()).campo(),
        CampoFicha::Nombre
    );
    assert!(almacen.renombrar_ficha(9999, "x").is_err());
    assert_eq!(almacen.obtener_ficha(id).unwrap().unwrap().nombre, "nuevo");
    assert_eq!(
        almacen.obtener_ficha(otra).unwrap().unwrap().nombre,
        "ocupado"
    );
    assert_eq!(
        fila_versionada(&almacen, Tabla::SesionesPersistentes, id).1,
        version1
    );
}

#[test]
fn borrar_la_ficha_deja_lapida_y_la_fila_desaparece() {
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    let id = almacen
        .crear_ficha(host, &datos_ficha("agente-api"))
        .unwrap();
    let otra = almacen.crear_ficha(host, &datos_ficha("otra")).unwrap();
    let (uuid, version, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);

    almacen.borrar_ficha(id).unwrap();
    assert!(almacen.obtener_ficha(id).unwrap().is_none());
    assert!(almacen.obtener_ficha(otra).unwrap().is_some(), "solo esa");
    let lapida = lapidas::obtener(almacen.conexion(), &uuid)
        .unwrap()
        .expect("lápida de la ficha");
    assert_eq!(lapida.tipo, "sesion_persistente");
    assert_eq!(lapida.nombre, "agente-api");
    assert!(
        ms_de(&lapida.version) > ms_de(&version),
        "el borrado domina las ediciones que ya había visto"
    );
    // El nombre queda libre para otra ficha.
    almacen
        .crear_ficha(host, &datos_ficha("agente-api"))
        .unwrap();

    // Borrar lo que no existe no hace nada ni deja lápida.
    let lapidas_antes = lapidas::contar(almacen.conexion()).unwrap();
    almacen.borrar_ficha(id).unwrap();
    almacen.borrar_ficha(9999).unwrap();
    assert_eq!(lapidas::contar(almacen.conexion()).unwrap(), lapidas_antes);
}

#[test]
fn borrar_un_host_borra_sus_fichas_con_lapida() {
    let almacen = almacen();
    let uno = crear_host(&almacen, "dev-01");
    let dos = crear_host(&almacen, "dev-02");
    let a = almacen.crear_ficha(uno, &datos_ficha("a")).unwrap();
    let b = almacen.crear_ficha(uno, &datos_ficha("b")).unwrap();
    let c = almacen.crear_ficha(dos, &datos_ficha("c")).unwrap();
    let uuid_a = fila_versionada(&almacen, Tabla::SesionesPersistentes, a).0;
    let uuid_b = fila_versionada(&almacen, Tabla::SesionesPersistentes, b).0;
    let uuid_c = fila_versionada(&almacen, Tabla::SesionesPersistentes, c).0;

    almacen.borrar_host(uno).unwrap();
    assert!(almacen.fichas_de_host(uno).unwrap().is_empty());
    assert_eq!(
        contar(&almacen, "SESIONES_PERSISTENTES"),
        1,
        "cae en cascada"
    );
    let de_tipo = lapidas::listar_de_tipo(almacen.conexion(), "sesion_persistente").unwrap();
    let mut uuids: Vec<&str> = de_tipo.iter().map(|l| l.uuid.as_str()).collect();
    uuids.sort_unstable();
    let mut esperadas = vec![uuid_a.as_str(), uuid_b.as_str()];
    esperadas.sort_unstable();
    assert_eq!(
        uuids, esperadas,
        "una lápida por ficha caída, y no la del otro host"
    );
    assert!(lapidas::obtener(almacen.conexion(), &uuid_c)
        .unwrap()
        .is_none());
    assert_eq!(
        lapidas::listar_de_tipo(almacen.conexion(), "host")
            .unwrap()
            .len(),
        1
    );
    assert!(almacen.obtener_ficha(c).unwrap().is_some());
}

#[test]
fn la_lectura_salta_las_filas_ilegibles_y_sigue() {
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    let rdp = crear_host_rdp(&almacen, "escritorio");
    let buena = almacen.crear_ficha(host, &datos_ficha("buena")).unwrap();
    let uuid_ok = "22222222-2222-4222-8222-222222222222";
    insertar_cruda(&almacen, host, "sin-uuid", None, "manual", None, None, None);
    insertar_cruda(
        &almacen,
        host,
        "uuid-vacio",
        Some("  "),
        "manual",
        None,
        None,
        None,
    );
    insertar_cruda(
        &almacen,
        host,
        "origen-raro",
        Some("33333333-3333-4333-8333-333333333333"),
        "inventado",
        None,
        None,
        None,
    );
    insertar_cruda(
        &almacen,
        host,
        "con espacio",
        Some("44444444-4444-4444-8444-444444444444"),
        "manual",
        None,
        None,
        None,
    );
    insertar_cruda(
        &almacen,
        host,
        "carpeta-mala",
        Some("55555555-5555-4555-8555-555555555555"),
        "manual",
        Some("relativa"),
        None,
        None,
    );
    insertar_cruda(
        &almacen,
        host,
        "comando-largo",
        Some("66666666-6666-4666-8666-666666666666"),
        "manual",
        None,
        Some(&"x".repeat(5000)),
        None,
    );
    insertar_cruda(
        &almacen,
        host,
        "nota-larga",
        Some("77777777-7777-4777-8777-777777777777"),
        "manual",
        None,
        None,
        Some(&"n".repeat(600)),
    );
    insertar_cruda(
        &almacen,
        rdp,
        "en-un-rdp",
        Some("88888888-8888-4888-8888-888888888888"),
        "manual",
        None,
        None,
        None,
    );
    insertar_cruda(
        &almacen,
        host,
        "valida-cruda",
        Some(uuid_ok),
        "descubierta",
        Some("  ~/api  "),
        Some("claude"),
        None,
    );

    let nombres: Vec<String> = almacen
        .listar_fichas()
        .unwrap()
        .into_iter()
        .map(|f| f.nombre)
        .collect();
    assert_eq!(
        nombres,
        ["buena", "valida-cruda"],
        "solo las legibles, sin cortar la lectura"
    );
    let cruda = almacen
        .ficha_por_nombre(host, "valida-cruda")
        .unwrap()
        .unwrap();
    assert_eq!(cruda.origen, OrigenFicha::Descubierta);
    assert_eq!(
        cruda.carpeta.as_deref(),
        Some("~/api"),
        "se lee normalizada"
    );
    assert_eq!(cruda.uuid, uuid_ok);
    for ilegible in [
        "sin-uuid",
        "origen-raro",
        "carpeta-mala",
        "comando-largo",
        "nota-larga",
    ] {
        assert!(
            almacen.ficha_por_nombre(host, ilegible).unwrap().is_none(),
            "{ilegible}"
        );
    }
    assert!(almacen.fichas_de_host(rdp).unwrap().is_empty());
    assert!(almacen.obtener_ficha(buena).unwrap().is_some());
    // Una ilegible sigue ocupando su nombre: no se puede crear otra igual.
    let reservados = almacen.nombres_de_fichas_de_host(host).unwrap();
    for nombre in ["sin-uuid", "origen-raro", "con espacio", "nota-larga"] {
        assert!(reservados.iter().any(|r| r == nombre), "{nombre}");
    }
    assert!(almacen
        .crear_ficha(host, &ficha_minima("sin-uuid"))
        .is_err());
    // Y borrar una sin uuid la quita (sin lápida: no puede viajar).
    let lapidas_antes = lapidas::contar(almacen.conexion()).unwrap();
    let id_sin_uuid: i64 = almacen
        .conexion()
        .query_row(
            "SELECT id FROM SESIONES_PERSISTENTES WHERE nombre = 'sin-uuid'",
            [],
            |fila| fila.get(0),
        )
        .unwrap();
    almacen.borrar_ficha(id_sin_uuid).unwrap();
    assert_eq!(lapidas::contar(almacen.conexion()).unwrap(), lapidas_antes);
    assert!(!almacen
        .nombres_de_fichas_de_host(host)
        .unwrap()
        .contains(&"sin-uuid".to_string()));
}

#[test]
fn marcar_el_host_sube_su_version_y_solo_si_cambia() {
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    let (uuid0, version0, vector0) = fila_versionada(&almacen, Tabla::Hosts, host);
    assert!(!almacen.obtener_host(host).unwrap().sesiones_persistentes);

    std::thread::sleep(std::time::Duration::from_millis(5));
    almacen.fijar_sesiones_persistentes(host, true).unwrap();
    assert!(almacen.obtener_host(host).unwrap().sesiones_persistentes);
    let (uuid1, version1, vector1) = fila_versionada(&almacen, Tabla::Hosts, host);
    assert_eq!(uuid1, uuid0);
    assert!(ms_de(&version1) > ms_de(&version0), "viaja: versión nueva");
    assert_ne!(vector1, vector0);
    // La marca forma parte de lo que viaja.
    let datos = versiones::datos_de(almacen.conexion(), Tabla::Hosts, host)
        .unwrap()
        .unwrap();
    assert!(datos.contains("\"sesiones_persistentes\":true"), "{datos}");

    // Marcar otra vez no cambia nada: ni la versión ni la fecha de edición.
    almacen
        .conexion()
        .execute(
            "UPDATE HOSTS SET actualizado_en = 'centinela' WHERE id = ?1",
            [host],
        )
        .unwrap();
    almacen.fijar_sesiones_persistentes(host, true).unwrap();
    assert_eq!(fila_versionada(&almacen, Tabla::Hosts, host).1, version1);
    let fecha: String = almacen
        .conexion()
        .query_row(
            "SELECT actualizado_en FROM HOSTS WHERE id = ?1",
            [host],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(fecha, "centinela");

    // Desmarcar sube otra versión.
    std::thread::sleep(std::time::Duration::from_millis(5));
    almacen.fijar_sesiones_persistentes(host, false).unwrap();
    assert!(!almacen.obtener_host(host).unwrap().sesiones_persistentes);
    let (_, version2, _) = fila_versionada(&almacen, Tabla::Hosts, host);
    assert!(ms_de(&version2) > ms_de(&version1));
    let datos = versiones::datos_de(almacen.conexion(), Tabla::Hosts, host)
        .unwrap()
        .unwrap();
    assert!(!datos.contains("sesiones_persistentes"), "{datos}");
}

#[test]
fn la_marca_solo_se_enciende_en_hosts_ssh_que_existen() {
    let almacen = almacen();
    let rdp = crear_host_rdp(&almacen, "escritorio");
    let version0 = fila_versionada(&almacen, Tabla::Hosts, rdp).1;
    let error = almacen.fijar_sesiones_persistentes(rdp, true).unwrap_err();
    assert_eq!(
        error_de_ficha(&error),
        ErrorFicha::HostNoSsh {
            tipo: TipoHost::Rdp
        }
    );
    assert!(!almacen.obtener_host(rdp).unwrap().sesiones_persistentes);
    assert_eq!(fila_versionada(&almacen, Tabla::Hosts, rdp).1, version0);
    let error = almacen.fijar_sesiones_persistentes(9999, true).unwrap_err();
    assert_eq!(
        error_de_ficha(&error),
        ErrorFicha::HostNoExiste { host_id: 9999 }
    );
    // Apagarla donde ya está apagada no es un error ni sube nada.
    almacen.fijar_sesiones_persistentes(rdp, false).unwrap();
    assert_eq!(fila_versionada(&almacen, Tabla::Hosts, rdp).1, version0);
}

#[test]
fn editar_la_ficha_del_host_no_pisa_la_marca() {
    // La importación de ~/.ssh/config reescribe la ficha con DatosHost, que no
    // lleva la marca: no debe apagarla.
    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    almacen.fijar_sesiones_persistentes(host, true).unwrap();
    let mut datos = DatosHost {
        nombre: "dev-01".to_string(),
        direccion: "192.0.2.99".to_string(),
        usuario: Some("usuario".to_string()),
        ..DatosHost::default()
    };
    datos.puerto = 2200;
    almacen.actualizar_host(host, &datos).unwrap();
    assert!(almacen.obtener_host(host).unwrap().sesiones_persistentes);
}

// ------------------------------------------------------------------ migración

fn equipo_de_pruebas() -> Equipo {
    Equipo::nuevo("pruebas", "linux")
}

/// Deja la base como la de la 1.4: sin la tabla ni la columna de la
/// migración 11 y con `user_version` 10.
fn dejar_como_la_version_diez(almacen: &Almacen) {
    let conexion = almacen.conexion();
    conexion
        .execute_batch(
            "DROP TABLE SESIONES_PERSISTENTES;
             ALTER TABLE HOSTS DROP COLUMN sesiones_persistentes;",
        )
        .unwrap();
    conexion.pragma_update(None, "user_version", 10).unwrap();
}

fn user_version(almacen: &Almacen) -> i64 {
    almacen
        .conexion()
        .query_row("PRAGMA user_version", [], |fila| fila.get(0))
        .unwrap()
}

/// Marca (tmux_persistente, sesiones_persistentes) tal como están en la fila.
fn marcas(almacen: &Almacen, id: i64) -> (i64, i64) {
    almacen
        .conexion()
        .query_row(
            "SELECT tmux_persistente, sesiones_persistentes FROM HOSTS WHERE id = ?1",
            [id],
            |fila| Ok((fila.get(0)?, fila.get(1)?)),
        )
        .unwrap()
}

#[test]
fn la_migracion_pasa_la_marca_de_tmux_por_el_camino_que_sube_la_version() {
    let directorio = tempfile::tempdir().unwrap();
    let ruta = directorio.path().join("magi.db");
    let equipo = equipo_de_pruebas();
    let (marcado, sin_marca, rdp_marcado);
    let (uuid_antes, version_antes, vector_antes);
    let (version_sin_marca, version_rdp);
    {
        let almacen = Almacen::abrir(&ruta, &equipo, "1.4.0").unwrap();
        marcado = crear_host(&almacen, "marcado");
        sin_marca = crear_host(&almacen, "sin-marca");
        rdp_marcado = crear_host_rdp(&almacen, "escritorio");
        // Lo que dejaba la Fase 15 con `fijar_tmux_persistente`: propia del
        // equipo, sin subir la versión.
        almacen
            .fijar_tmux_persistente(marcado, true)
            .expect("marca de la Fase 15");
        almacen
            .conexion()
            .execute(
                "UPDATE HOSTS SET tmux_persistente = 1 WHERE id = ?1",
                [rdp_marcado],
            )
            .unwrap();
        (uuid_antes, version_antes, vector_antes) =
            fila_versionada(&almacen, Tabla::Hosts, marcado);
        version_sin_marca = fila_versionada(&almacen, Tabla::Hosts, sin_marca).1;
        version_rdp = fila_versionada(&almacen, Tabla::Hosts, rdp_marcado).1;
        dejar_como_la_version_diez(&almacen);
        almacen.cerrar().unwrap();
    }

    std::thread::sleep(std::time::Duration::from_millis(5));
    let almacen = Almacen::abrir(&ruta, &equipo, "1.5.0").unwrap();
    assert_eq!(user_version(&almacen), 11);
    // El host marcado: marca nueva, versión nueva y la antigua a 0.
    assert_eq!(marcas(&almacen, marcado), (0, 1));
    assert!(almacen.obtener_host(marcado).unwrap().sesiones_persistentes);
    assert!(!almacen.obtener_host(marcado).unwrap().tmux_persistente);
    let (uuid1, version1, vector1) = fila_versionada(&almacen, Tabla::Hosts, marcado);
    assert_eq!(uuid1, uuid_antes);
    assert_ne!(version1, version_antes);
    assert!(
        ms_de(&version1) > ms_de(&version_antes),
        "{version1} > {version_antes}"
    );
    assert_ne!(vector1, vector_antes);
    let este = magi_nucleo::almacen::equipos::este(almacen.conexion()).unwrap();
    assert_eq!(
        versiones::vector_desde_json(&vector1).get(&este.uuid),
        Some(&ms_de(&version1)),
        "la versión nueva es de este equipo"
    );
    // La marca viaja: forma parte de los datos de sincronización del host.
    let datos = versiones::datos_de(almacen.conexion(), Tabla::Hosts, marcado)
        .unwrap()
        .unwrap();
    assert!(datos.contains("\"sesiones_persistentes\":true"), "{datos}");

    // Los demás no se tocan; uno que no es ssh solo pierde la marca antigua.
    assert_eq!(marcas(&almacen, sin_marca), (0, 0));
    assert_eq!(
        fila_versionada(&almacen, Tabla::Hosts, sin_marca).1,
        version_sin_marca
    );
    assert_eq!(marcas(&almacen, rdp_marcado), (0, 0), "un rdp no se marca");
    assert_eq!(
        fila_versionada(&almacen, Tabla::Hosts, rdp_marcado).1,
        version_rdp
    );

    // Se avisa de lo que migró, una vez.
    assert_eq!(almacen.hosts_con_marca_migrada(), 1);
    let aviso = almacen.aviso_de_migracion().expect("aviso de la migración");
    assert!(aviso.contains("1 host"), "{aviso}");
    assert!(aviso.contains("viaja"), "{aviso}");
    almacen.cerrar().unwrap();

    // Idempotente: abrir otra vez no cambia nada ni vuelve a avisar.
    let otra_vez = Almacen::abrir(&ruta, &equipo, "1.5.0").unwrap();
    assert_eq!(user_version(&otra_vez), 11);
    assert_eq!(marcas(&otra_vez, marcado), (0, 1));
    assert_eq!(
        fila_versionada(&otra_vez, Tabla::Hosts, marcado),
        (uuid1, version1, vector1),
        "ni versión ni vector"
    );
    assert_eq!(marcas(&otra_vez, sin_marca), (0, 0));
    assert_eq!(otra_vez.hosts_con_marca_migrada(), 0);
    assert_eq!(otra_vez.aviso_de_migracion(), None);
}

#[test]
fn la_migracion_cuenta_cada_host_marcado_y_el_aviso_va_en_plural() {
    let directorio = tempfile::tempdir().unwrap();
    let ruta = directorio.path().join("magi.db");
    let equipo = equipo_de_pruebas();
    let ids;
    {
        let almacen = Almacen::abrir(&ruta, &equipo, "1.4.0").unwrap();
        ids = [
            crear_host(&almacen, "uno"),
            crear_host(&almacen, "dos"),
            crear_host(&almacen, "tres"),
        ];
        for id in &ids[..2] {
            almacen.fijar_tmux_persistente(*id, true).unwrap();
        }
        dejar_como_la_version_diez(&almacen);
    }
    let almacen = Almacen::abrir(&ruta, &equipo, "1.5.0").unwrap();
    assert_eq!(almacen.hosts_con_marca_migrada(), 2);
    let aviso = almacen.aviso_de_migracion().unwrap();
    assert!(aviso.contains("2 hosts"), "{aviso}");
    assert_eq!(marcas(&almacen, ids[0]), (0, 1));
    assert_eq!(marcas(&almacen, ids[1]), (0, 1));
    assert_eq!(marcas(&almacen, ids[2]), (0, 0));
}

/// Regresión de la revisión adversarial: el paso de la migración 11 no puede
/// depender de que el resto de la fila del host sea válido. Si leyera los datos
/// que viajan del host (puerto, keepalive, opciones del escritorio), una fila
/// corrupta haría fallar la migración y MAGI no volvería a abrir la base.
#[test]
fn la_migracion_no_falla_por_un_host_marcado_con_datos_corruptos() {
    let directorio = tempfile::tempdir().unwrap();
    let ruta = directorio.path().join("magi.db");
    let equipo = equipo_de_pruebas();
    let (roto, sano);
    let version_antes;
    {
        let almacen = Almacen::abrir(&ruta, &equipo, "1.4.0").unwrap();
        roto = crear_host(&almacen, "roto");
        sano = crear_host(&almacen, "sano");
        for id in [roto, sano] {
            almacen.fijar_tmux_persistente(id, true).unwrap();
        }
        version_antes = fila_versionada(&almacen, Tabla::Hosts, roto).1;
        dejar_como_la_version_diez(&almacen);
        almacen
            .conexion()
            .execute_batch(
                "UPDATE HOSTS SET puerto = 70000, keepalive_seg = -5,
                        escritorio_opciones = 'no es json' WHERE nombre = 'roto';",
            )
            .unwrap();
    }
    let almacen = Almacen::abrir(&ruta, &equipo, "1.5.0")
        .expect("una fila corrupta no impide migrar ni abrir la base");
    assert_eq!(user_version(&almacen), 11);
    assert_eq!(almacen.hosts_con_marca_migrada(), 2);
    for id in [roto, sano] {
        assert_eq!(marcas(&almacen, id), (0, 1));
    }
    let version_despues = fila_versionada(&almacen, Tabla::Hosts, roto).1;
    assert!(
        ms_de(&version_despues) > ms_de(&version_antes),
        "la marca viaja aunque el resto de la fila no se pueda leer"
    );
    // La segunda apertura no repite nada.
    almacen.cerrar().unwrap();
    let otra_vez = Almacen::abrir(&ruta, &equipo, "1.5.0").unwrap();
    assert_eq!(otra_vez.hosts_con_marca_migrada(), 0);
    assert_eq!(
        fila_versionada(&otra_vez, Tabla::Hosts, roto).1,
        version_despues
    );
}

#[test]
fn sin_hosts_marcados_no_hay_aviso_ni_en_memoria_ni_en_disco() {
    assert_eq!(almacen().hosts_con_marca_migrada(), 0);
    assert_eq!(almacen().aviso_de_migracion(), None);
    assert_eq!(aviso_marca_migrada(0), None);
    assert!(aviso_marca_migrada(1).unwrap().contains("1 host "));
    assert!(aviso_marca_migrada(7).unwrap().contains("7 hosts"));

    let directorio = tempfile::tempdir().unwrap();
    let ruta = directorio.path().join("magi.db");
    let equipo = equipo_de_pruebas();
    {
        let almacen = Almacen::abrir(&ruta, &equipo, "1.4.0").unwrap();
        crear_host(&almacen, "sin-marca");
        dejar_como_la_version_diez(&almacen);
    }
    let almacen = Almacen::abrir(&ruta, &equipo, "1.5.0").unwrap();
    assert_eq!(user_version(&almacen), 11);
    assert_eq!(almacen.hosts_con_marca_migrada(), 0);
    assert_eq!(almacen.aviso_de_migracion(), None);
}

#[test]
fn una_base_nueva_se_abre_en_la_version_11_y_las_fichas_sobreviven_a_reabrirla() {
    let directorio = tempfile::tempdir().unwrap();
    let ruta = directorio.path().join("magi.db");
    let equipo = equipo_de_pruebas();
    let (host, ficha_id, version_ficha);
    {
        let almacen = Almacen::abrir(&ruta, &equipo, "1.5.0").unwrap();
        assert_eq!(user_version(&almacen), 11);
        host = crear_host(&almacen, "dev-01");
        ficha_id = almacen
            .crear_ficha(host, &datos_ficha("agente-api"))
            .unwrap();
        version_ficha = fila_versionada(&almacen, Tabla::SesionesPersistentes, ficha_id);
        almacen.cerrar().unwrap();
    }
    let almacen = Almacen::abrir(&ruta, &equipo, "1.5.0").unwrap();
    assert_eq!(almacen.hosts_con_marca_migrada(), 0);
    assert_eq!(
        fila_versionada(&almacen, Tabla::SesionesPersistentes, ficha_id),
        version_ficha,
        "reabrir no toca las fichas"
    );
    assert_eq!(
        almacen
            .ficha_por_nombre(host, "agente-api")
            .unwrap()
            .unwrap()
            .id,
        ficha_id
    );
}

// ------------------------------------------------------------------ registro

#[test]
fn el_registro_admite_los_cuatro_tipos_de_sesiones_persistentes() {
    assert_eq!(
        registro::TIPOS.len(),
        56,
        "46 de antes, los cuatro de la F16 y los seis de la F17"
    );
    let nuevos = [
        (
            registro::SESION_PERSISTENTE_CREADA,
            "sesion_persistente_creada",
        ),
        (
            registro::SESION_PERSISTENTE_REABIERTA,
            "sesion_persistente_reabierta",
        ),
        (
            registro::SESION_PERSISTENTE_ENGANCHADA,
            "sesion_persistente_enganchada",
        ),
        (
            registro::SESION_PERSISTENTE_BORRADA,
            "sesion_persistente_borrada",
        ),
    ];
    // Sin repetidos.
    let mut todos: Vec<&str> = registro::TIPOS.to_vec();
    todos.sort_unstable();
    todos.dedup();
    assert_eq!(todos.len(), 56);

    let almacen = almacen();
    let host = crear_host(&almacen, "dev-01");
    for (constante, texto) in nuevos {
        assert_eq!(constante, texto);
        assert!(registro::TIPOS.contains(&constante), "{texto}");
        registro::anotar(
            almacen.conexion(),
            constante,
            Some(host),
            None,
            "dev-01 · agente-api",
            ResultadoRegistro::Ok,
        )
        .unwrap_or_else(|error| panic!("{texto}: {error:#}"));
    }
    assert_eq!(contar(&almacen, "REGISTRO"), 4);
    let tipos: Vec<String> = almacen
        .conexion()
        .prepare("SELECT tipo FROM REGISTRO ORDER BY id")
        .unwrap()
        .query_map([], |fila| fila.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        tipos,
        nuevos
            .iter()
            .map(|(_, texto)| texto.to_string())
            .collect::<Vec<_>>()
    );
    // Un tipo que no está en la lista sigue rechazándose.
    assert!(registro::anotar(
        almacen.conexion(),
        "sesion_persistente_inventada",
        Some(host),
        None,
        "",
        ResultadoRegistro::Ok
    )
    .is_err());
    assert_eq!(contar(&almacen, "REGISTRO"), 4);
}
