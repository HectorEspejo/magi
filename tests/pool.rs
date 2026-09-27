//! Pruebas de la corrección 3b (Fase 6): un único punto de apertura por host
//! con cerrojo, pestañas que reutilizan el pool con `multiplexar`, ninguna
//! conexión viva desplazada, diálogos que no retienen el cerrojo, `Ejecutar`
//! con plazo y código real, `Hecho` con la fila escrita y `SIGTERM` = `Parar`.

mod comun;

use std::collections::HashSet;
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use magi::almacen::{tuneles as almacen_tuneles, Almacen};
use magi::modelo::{DatosTunel, IdentidadRef, TipoTunel};
use magi::protocolo::{EstadoSesionRemota, EstadoTunelRemoto, MensajeCliente, MensajeServidor};

use comun::*;

/// Crea un túnel local hacia el servicio de eco (lo que hace el cliente).
fn crear_tunel_local(escenario: &Escenario, host_id: i64, nombre: &str, destino: u16) -> i64 {
    crear_tunel(escenario, host_id, nombre, destino, false)
}

fn crear_tunel(
    escenario: &Escenario,
    host_id: i64,
    nombre: &str,
    destino: u16,
    automatico: bool,
) -> i64 {
    let almacen = Almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let id = almacen_tuneles::crear(
        almacen.conexion(),
        &DatosTunel {
            host_id,
            nombre: nombre.to_string(),
            tipo: TipoTunel::Local,
            escucha: "127.0.0.1:0".to_string(),
            destino: Some(format!("127.0.0.1:{destino}")),
            automatico,
        },
    )
    .expect("creando el túnel");
    almacen.cerrar().unwrap();
    id
}

/// Activa un túnel y espera a verlo activo; devuelve su escucha efectiva.
async fn activar(escenario: &mut Escenario, tunel_id: i64) -> String {
    let peticion_id = 500 + tunel_id as u64;
    escenario
        .enviar(&MensajeCliente::ActivarTunel {
            tunel_id,
            peticion_id,
        })
        .await;
    let error = escenario.esperar_respuesta(peticion_id).await;
    assert!(error.is_none(), "el túnel no se levantó: {error:?}");
    loop {
        if let Some(tunel) = escenario
            .tuneles
            .iter()
            .find(|tunel| tunel.tunel_id == tunel_id && tunel.estado == EstadoTunelRemoto::Activo)
        {
            return tunel.escucha_efectiva.clone();
        }
        escenario.siguiente().await;
    }
}

/// Manda un saludo por el túnel local y devuelve el eco.
async fn hablar_por(escucha: &str, saludo: &str) -> String {
    let (direccion, puerto) = magi::modelo::partir_direccion_puerto(escucha).expect("escucha");
    let mut flujo = tokio::net::TcpStream::connect((direccion.as_str(), puerto))
        .await
        .expect("conectando al túnel");
    flujo.write_all(saludo.as_bytes()).await.expect("saludo");
    let mut bufer = vec![0u8; 256];
    let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
        .await
        .expect("el eco no llegó")
        .expect("leyendo el eco");
    String::from_utf8_lossy(&bufer[..leidos]).to_string()
}

/// AC (checklist l. 6): con `multiplexar` y una pestaña abierta, otra al mismo
/// host no dialoga, no abre otra conexión y `conexion_abierta` se anota una
/// sola vez.
#[tokio::test]
async fn dos_pestanas_con_multiplexar_comparten_una_conexion_y_una_anotacion() {
    let mut escenario = escenario(OpcionesEscenario {
        multiplexar: true,
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    let primera = escenario.abrir_sesion(host).await;
    let segunda = escenario.abrir_sesion(host).await;
    assert_ne!(primera, segunda);
    assert_eq!(
        escenario.observado.conexiones(),
        1,
        "la segunda reautenticó"
    );
    assert!(
        !escenario.vistos.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::HuellaDesconocida { .. }
                | MensajeServidor::PideFrase { .. }
                | MensajeServidor::PideContrasena { .. }
        )),
        "una pestaña que reutiliza no dialoga"
    );
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| {
                escenario.anotaciones("conexion_abierta") == 1
            })
            .await,
        "conexion_abierta se anotó {} veces",
        escenario.anotaciones("conexion_abierta")
    );
}

/// Sin `multiplexar`, cada pestaña abre su conexión y la cierra con ella: no
/// entra en el pool.
#[tokio::test]
async fn una_pestana_sin_multiplexar_abre_su_propia_conexion_y_la_cierra_con_ella() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    let primera = escenario.abrir_sesion(host).await;
    let _segunda = escenario.abrir_sesion(host).await;
    assert_eq!(escenario.observado.conexiones(), 2);
    escenario
        .enviar(&MensajeCliente::Cerrar { sesion_id: primera })
        .await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| {
                escenario.observado.vivas() == 1
            })
            .await,
        "la conexión propia no se cerró con su pestaña: {} vivas",
        escenario.observado.vivas()
    );
}

/// AC (checklist l. 8): un túnel activo sigue activo al abrir una pestaña al
/// mismo host, sin `tunel_cerrado`, con y sin `multiplexar`.
async fn pestana_no_tumba_el_tunel(multiplexar: bool) {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut escenario = escenario(OpcionesEscenario {
        multiplexar,
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    let tunel = crear_tunel_local(&escenario, host, "eco", puerto_eco);
    let escucha = activar(&mut escenario, tunel).await;
    assert_eq!(hablar_por(&escucha, "hola").await, "hola");

    escenario.abrir_sesion(host).await;
    escenario.abrir_sesion(host).await;
    let llegados = escenario.escuchar(Duration::from_millis(1500)).await;
    assert!(
        !llegados.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::Tuneles { lista }
                if lista.iter().any(|t| t.tunel_id == tunel && t.estado == EstadoTunelRemoto::Caido)
        )),
        "el túnel se cayó al abrir la pestaña"
    );
    assert_eq!(hablar_por(&escucha, "sigue").await, "sigue");
    assert_eq!(escenario.anotaciones("tunel_cerrado"), 0);
}

#[tokio::test]
async fn abrir_una_pestana_no_tumba_un_tunel_activo_con_multiplexar() {
    pestana_no_tumba_el_tunel(true).await;
}

#[tokio::test]
async fn abrir_una_pestana_no_tumba_un_tunel_activo_sin_multiplexar() {
    pestana_no_tumba_el_tunel(false).await;
}

/// Pestaña, SFTP y túnel pedidos a la vez comparten una única conexión con
/// `multiplexar`: el cerrojo por host serializa «mirar / conectar / guardar».
#[tokio::test]
async fn pestana_sftp_y_tunel_a_la_vez_comparten_una_conexion() {
    if !hay_sftp_server() {
        eprintln!("sin sftp-server en el sistema: se omite");
        return;
    }
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut escenario = escenario(OpcionesEscenario {
        multiplexar: true,
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    let tunel = crear_tunel_local(&escenario, host, "eco", puerto_eco);
    escenario
        .enviar(&MensajeCliente::AbrirSesion {
            comandos_iniciales: Vec::new(),
            host_id: host,
            cols: 80,
            filas: 24,
        })
        .await;
    escenario
        .enviar(&MensajeCliente::AbrirSftp {
            host_id: host,
            peticion_id: None,
            no_interactivo: false,
        })
        .await;
    escenario
        .enviar(&MensajeCliente::ActivarTunel {
            tunel_id: tunel,
            peticion_id: 900,
        })
        .await;
    let (mut sesion, mut sftp, mut tunel_listo) = (false, false, false);
    while !(sesion && sftp && tunel_listo) {
        match escenario.siguiente().await {
            MensajeServidor::SftpAbierto { .. } => sftp = true,
            MensajeServidor::Hecho { peticion_id: 900 } => tunel_listo = true,
            MensajeServidor::Error { mensaje, .. } => panic!("falló una apertura: {mensaje}"),
            _ => {}
        }
        sesion = escenario
            .sesiones
            .iter()
            .any(|sesion| sesion.estado == EstadoSesionRemota::Abierta);
    }
    assert_eq!(
        escenario.observado.conexiones(),
        1,
        "las tres aperturas debían compartir una conexión"
    );
}

/// Una conexión del pool que se cae no se vuelve a dar: la pestaña siguiente
/// abre otra (y la caída no deja una entrada muerta que nadie recoja).
#[tokio::test]
async fn una_conexion_del_pool_caida_no_se_reutiliza() {
    let mut escenario = escenario(OpcionesEscenario {
        multiplexar: true,
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    let primera = escenario.abrir_sesion(host).await;
    escenario.observado.tirar_conexiones().await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| {
                escenario.sesiones.iter().any(|sesion| {
                    sesion.id == primera && sesion.estado == EstadoSesionRemota::Caida
                })
            })
            .await,
        "la pestaña no se enteró de la caída"
    );
    let segunda = escenario.abrir_sesion(host).await;
    assert_ne!(primera, segunda);
    assert_eq!(escenario.observado.conexiones(), 2);
}

/// La reconexión de una pestaña caída abre de nuevo por la vía única.
#[tokio::test]
async fn reconectar_una_pestana_caida_vuelve_a_abrirla() {
    let mut escenario = escenario(OpcionesEscenario {
        multiplexar: true,
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    let sesion_id = escenario.abrir_sesion(host).await;
    escenario.observado.tirar_conexiones().await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| {
                escenario.sesiones.iter().any(|sesion| {
                    sesion.id == sesion_id && sesion.estado == EstadoSesionRemota::Caida
                })
            })
            .await
    );
    escenario
        .enviar(&MensajeCliente::Reconectar { sesion_id })
        .await;
    assert!(
        escenario
            .hasta(Duration::from_secs(10), |escenario| {
                escenario.sesiones.iter().any(|sesion| {
                    sesion.id == sesion_id && sesion.estado == EstadoSesionRemota::Abierta
                })
            })
            .await,
        "la pestaña no volvió a abrir: {:?}",
        escenario.sesiones
    );
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| {
                escenario.anotaciones("sesion_reconectada") == 1
            })
            .await
    );
}

/// Un túnel automático no dialoga nunca: si la conexión necesita la
/// contraseña del llavero y no hay solicitante, el túnel cae con el motivo en
/// vez de preguntar (antes esperaba 5 minutos con el cerrojo tomado).
#[tokio::test]
async fn un_tunel_automatico_no_dialoga() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut escenario = escenario(OpcionesEscenario {
        identidad: Some(IdentidadRef::ContrasenaLlavero),
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    let tunel = crear_tunel(&escenario, host, "auto", puerto_eco, true);
    // La pestaña sí dialoga (contraseña): `abrir_sesion` la contesta.
    escenario.abrir_sesion(host).await;
    let antes = escenario.vistos.len();
    let caido = escenario
        .hasta(Duration::from_secs(10), |escenario| {
            escenario
                .tuneles
                .iter()
                .any(|t| t.tunel_id == tunel && t.estado == EstadoTunelRemoto::Caido)
        })
        .await;
    assert!(
        caido,
        "el túnel automático no cayó: {:?}",
        escenario.tuneles
    );
    let motivo = escenario
        .tuneles
        .iter()
        .find(|t| t.tunel_id == tunel)
        .and_then(|t| t.ultimo_error.clone())
        .unwrap_or_default();
    assert!(motivo.contains("llavero"), "motivo: {motivo}");
    assert!(
        !escenario.vistos[antes..].iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::PideContrasena { .. }
                | MensajeServidor::PideLlavero { .. }
                | MensajeServidor::HuellaDesconocida { .. }
        )),
        "el túnel automático pidió algo a la ventana"
    );
}

/// `Ejecutar` lee hasta el cierre del canal: el código de salida llega tras el
/// EOF de los datos y no se pierde. Sin pool, usa la conexión de la pestaña.
#[tokio::test]
async fn ejecutar_devuelve_el_codigo_aunque_llegue_tras_el_eof() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    escenario.abrir_sesion(host).await;
    escenario
        .enviar(&MensajeCliente::Ejecutar {
            host_id: host,
            comando: "echo hola; exit 3".to_string(),
            peticion_id: 1 << 48,
        })
        .await;
    match escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Ejecutado { .. } | MensajeServidor::SinSesion { .. }
            )
        })
        .await
    {
        MensajeServidor::Ejecutado {
            salida,
            codigo,
            peticion_id,
            ..
        } => {
            assert_eq!(peticion_id, 1 << 48);
            assert_eq!(codigo, 3);
            assert_eq!(salida, "hola\n");
        }
        otro => panic!("sin ejecución: {otro:?}"),
    }
}

/// Dos `Ejecutar` al mismo host a la vez no se cruzan: cada respuesta lleva
/// su `peticion_id`. Sin conexión viva contestan `SinSesion` sin conectar.
#[tokio::test]
async fn dos_ejecutar_al_mismo_host_no_se_cruzan() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    escenario
        .enviar(&MensajeCliente::Ejecutar {
            host_id: host,
            comando: "echo nada".to_string(),
            peticion_id: 7,
        })
        .await;
    match escenario
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::SinSesion { .. }))
        .await
    {
        MensajeServidor::SinSesion { peticion_id, .. } => assert_eq!(peticion_id, 7),
        otro => panic!("{otro:?}"),
    }
    assert_eq!(escenario.observado.conexiones(), 0, "SoloViva no conecta");

    escenario.abrir_sesion(host).await;
    for (peticion_id, texto) in [(10u64, "uno"), (11, "dos")] {
        escenario
            .enviar(&MensajeCliente::Ejecutar {
                host_id: host,
                comando: format!("sleep 0.2; echo {texto}"),
                peticion_id,
            })
            .await;
    }
    let mut vistas = Vec::new();
    while vistas.len() < 2 {
        if let MensajeServidor::Ejecutado {
            peticion_id,
            salida,
            ..
        } = escenario.siguiente().await
        {
            vistas.push((peticion_id, salida));
        }
    }
    vistas.sort();
    assert_eq!(
        vistas,
        vec![(10, "uno\n".to_string()), (11, "dos\n".to_string())]
    );
}

/// `Hecho` de una operación que anota llega con la fila ya en `REGISTRO`.
#[tokio::test]
async fn el_hecho_de_un_tunel_llega_con_la_fila_ya_escrita() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    for vuelta in 0..3 {
        let tunel = crear_tunel_local(&escenario, host, &format!("eco{vuelta}"), puerto_eco);
        escenario
            .enviar(&MensajeCliente::ActivarTunel {
                tunel_id: tunel,
                peticion_id: 40 + vuelta,
            })
            .await;
        assert!(escenario.esperar_respuesta(40 + vuelta).await.is_none());
        // Justo tras el `Hecho`, sin esperar: la fila tiene que estar.
        assert_eq!(
            escenario.anotaciones("tunel_abierto"),
            vuelta as usize + 1,
            "el Hecho llegó antes que la fila"
        );
    }
}

/// Una ventana que se va con un diálogo pendiente lo suelta: la siguiente
/// apertura del host no espera los 5 minutos del plazo con el cerrojo tomado.
#[tokio::test]
async fn la_ventana_que_se_va_suelta_sus_dialogos() {
    if !hay_sftp_server() {
        eprintln!("sin sftp-server en el sistema: se omite");
        return;
    }
    let mut escenario = escenario(OpcionesEscenario {
        huella_conocida: false,
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    let ruta = magi::servidor::ruta_socket(escenario.rutas());

    // Una segunda ventana pide el canal SFTP y se va sin contestar la huella.
    let mut otra = cliente(&ruta, magi::protocolo::VERSION_PROTOCOLO).await;
    let _: MensajeServidor = siguiente(&mut otra).await; // Bienvenida
    enviar(
        otra.get_mut(),
        &MensajeCliente::AbrirSftp {
            host_id: host,
            peticion_id: None,
            no_interactivo: false,
        },
    )
    .await;
    let _ = esperar::<MensajeServidor, _>(&mut otra, |mensaje| {
        matches!(mensaje, MensajeServidor::HuellaDesconocida { .. })
    })
    .await;
    drop(otra);

    // La ventana que queda pide lo mismo: su diálogo llega enseguida.
    escenario
        .enviar(&MensajeCliente::AbrirSftp {
            host_id: host,
            peticion_id: None,
            no_interactivo: false,
        })
        .await;
    let llegado = tokio::time::timeout(
        Duration::from_secs(8),
        escenario.esperar(|mensaje| matches!(mensaje, MensajeServidor::HuellaDesconocida { .. })),
    )
    .await;
    assert!(
        llegado.is_ok(),
        "el cerrojo del host siguió tomado por el diálogo de la ventana que se fue"
    );
}

/// Una pestaña cerrada con su diálogo de huella pendiente se anota una sola
/// vez («ventana cerrada»): su apertura se abandona sin volver a anotar ni
/// marcar el host en error.
#[tokio::test]
async fn cerrar_una_pestana_que_espera_la_huella_se_anota_una_vez() {
    let mut escenario = escenario(OpcionesEscenario {
        huella_conocida: false,
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    escenario
        .enviar(&MensajeCliente::AbrirSesion {
            comandos_iniciales: Vec::new(),
            host_id: host,
            cols: 80,
            filas: 24,
        })
        .await;
    let sesion_id = match escenario
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::HuellaDesconocida { .. }))
        .await
    {
        MensajeServidor::HuellaDesconocida { sesion_id, .. } => sesion_id,
        _ => unreachable!(),
    };
    escenario
        .enviar(&MensajeCliente::Cerrar { sesion_id })
        .await;
    escenario.escuchar(Duration::from_millis(800)).await;
    assert_eq!(escenario.anotaciones("conexion_fallida"), 1);
    // El host no queda marcado en error por una apertura que el usuario cerró.
    let almacen = Almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let estado: Option<String> = almacen
        .conexion()
        .query_row(
            "SELECT ultimo_estado FROM HOSTS WHERE id = ?1",
            [host],
            |fila| fila.get(0),
        )
        .unwrap();
    almacen.cerrar().unwrap();
    assert_ne!(estado.as_deref(), Some("error"));
}

/// Un host que acepta TCP y no habla SSH no retiene el cerrojo para siempre:
/// la apertura interactiva falla con su plazo de red y la siguiente del mismo
/// host no se queda esperando.
#[tokio::test]
async fn un_host_mudo_no_retiene_el_cerrojo() {
    let mudo = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let puerto = mudo.local_addr().unwrap().port();
    let _aceptador = tokio::spawn(async move {
        let mut abiertas = Vec::new();
        while let Ok((flujo, _)) = mudo.accept().await {
            abiertas.push(flujo);
        }
    });
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    {
        let almacen = Almacen::abrir(&escenario.rutas().base_datos()).unwrap();
        almacen
            .conexion()
            .execute(
                "UPDATE HOSTS SET puerto = ?1 WHERE id = ?2",
                [i64::from(puerto), host],
            )
            .unwrap();
        almacen.cerrar().unwrap();
    }
    escenario
        .enviar(&MensajeCliente::AbrirSftp {
            host_id: host,
            peticion_id: None,
            no_interactivo: false,
        })
        .await;
    let inicio = std::time::Instant::now();
    let error = tokio::time::timeout(
        Duration::from_secs(40),
        escenario.esperar(|mensaje| matches!(mensaje, MensajeServidor::Error { .. })),
    )
    .await
    .expect("la apertura no falló nunca");
    assert!(
        inicio.elapsed() < Duration::from_secs(30),
        "tardó {:?}",
        inicio.elapsed()
    );
    match error {
        MensajeServidor::Error { mensaje, .. } => {
            assert!(mensaje.contains("no respondió"), "{mensaje}")
        }
        _ => unreachable!(),
    }
    // La siguiente apertura del host no espera al cerrojo de la anterior.
    escenario
        .enviar(&MensajeCliente::AbrirSftp {
            host_id: host,
            peticion_id: None,
            no_interactivo: false,
        })
        .await;
    let segundo = tokio::time::timeout(
        Duration::from_secs(40),
        escenario.esperar(|mensaje| matches!(mensaje, MensajeServidor::Error { .. })),
    )
    .await;
    assert!(segundo.is_ok(), "la segunda apertura se quedó esperando");
}

/// `Parar` (lo mismo que hace `SIGTERM`) anota el cierre de las pestañas.
#[tokio::test]
async fn parar_anota_el_cierre_de_las_sesiones() {
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    escenario.abrir_sesion(host).await;
    escenario.abrir_sesion(host).await;
    escenario.enviar(&MensajeCliente::Parar).await;
    let tarea = std::mem::replace(&mut escenario.tarea_magi, tokio::spawn(async { Ok(()) }));
    let salida = tokio::time::timeout(Duration::from_secs(15), tarea).await;
    assert!(salida.is_ok(), "el servidor no se paró");
    let filas = escenario.registro();
    let cerradas = filas
        .iter()
        .filter(|(tipo, detalle)| tipo == "sesion_cerrada" && detalle.contains("apaga"))
        .count();
    assert_eq!(cerradas, 2, "{filas:?}");
    assert!(filas.iter().any(|(tipo, _)| tipo == "servidor_detenido"));
    assert!(!magi::servidor::ruta_socket(escenario.rutas()).exists());
}

/// `SIGTERM` al binario real: se para limpio (código 0), borra socket y lock y
/// anota la parada. `parar_por_senal` y el pid del par, contra ese proceso.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn sigterm_para_el_servidor_limpio() {
    let temporal = tempfile::tempdir().unwrap();
    let raiz = temporal.path();
    for sub in ["datos", "config", "estado", "hogar", "runtime"] {
        std::fs::create_dir_all(raiz.join(sub)).unwrap();
    }
    let rutas = magi::config::Rutas {
        datos: raiz.join("datos").join("magi"),
        config: raiz.join("config").join("magi"),
        estado: raiz.join("estado").join("magi"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime").join("magi"),
    };
    let mut hijo = tokio::process::Command::new(env!("CARGO_BIN_EXE_magi"))
        .arg("--servidor")
        .env("HOME", raiz.join("hogar"))
        .env("XDG_DATA_HOME", raiz.join("datos"))
        .env("XDG_CONFIG_HOME", raiz.join("config"))
        .env("XDG_STATE_HOME", raiz.join("estado"))
        .env("XDG_RUNTIME_DIR", raiz.join("runtime"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("lanzando magi --servidor");
    let pid = hijo.id().expect("pid del servidor");
    let socket = magi::servidor::ruta_socket(&rutas);
    let mut listo = false;
    for _ in 0..100 {
        if socket.exists() && tokio::net::UnixStream::connect(&socket).await.is_ok() {
            listo = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(listo, "el servidor no abrió su socket");

    // El pid del otro lado del socket es el del servidor.
    let stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    assert_eq!(magi::servidor::pid_del_par(&stream), Some(pid));
    drop(stream);

    // El hijo se recoge a la vez: si no, seguiría como zombi y `parar_por_senal`
    // (que espera a que el proceso no exista) no lo daría por parado.
    let espera = tokio::spawn(async move { hijo.wait().await });
    let parado = magi::servidor::parar_por_senal(&rutas, pid).await.unwrap();
    assert!(parado, "el servidor no se detuvo");
    let estado = tokio::time::timeout(Duration::from_secs(10), espera)
        .await
        .expect("el servidor no terminó")
        .unwrap()
        .unwrap();
    assert!(estado.success(), "salida: {estado:?}");
    assert!(!socket.exists());
    assert!(!magi::servidor::ruta_lock(&rutas).exists());

    let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
    let tipos: HashSet<String> = {
        let mut sentencia = almacen
            .conexion()
            .prepare("SELECT tipo FROM REGISTRO")
            .unwrap();
        sentencia
            .query_map([], |fila| fila.get(0))
            .unwrap()
            .collect::<rusqlite::Result<HashSet<String>>>()
            .unwrap()
    };
    almacen.cerrar().unwrap();
    assert!(tipos.contains("servidor_arrancado"), "{tipos:?}");
    assert!(tipos.contains("servidor_detenido"), "{tipos:?}");
}

/// El canal SFTP que abre una comprobación de backup (BALTHASAR-2) no dialoga
/// ni levanta los túneles automáticos del host; cuando Archivos lo usa
/// después, sí cuenta.
#[tokio::test]
async fn una_comprobacion_sftp_no_levanta_los_tuneles_automaticos() {
    if !hay_sftp_server() {
        eprintln!("sin sftp-server en el sistema: se omite");
        return;
    }
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut escenario = escenario(OpcionesEscenario::default()).await;
    let host = escenario.hosts[0];
    let tunel = crear_tunel(&escenario, host, "auto", puerto_eco, true);
    escenario
        .enviar(&MensajeCliente::AbrirSftp {
            host_id: host,
            peticion_id: Some(1 << 48),
            no_interactivo: true,
        })
        .await;
    match escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::SftpAbierto { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::SftpAbierto { peticion_id, .. } => {
            assert_eq!(peticion_id, Some(1 << 48))
        }
        otro => panic!("{otro:?}"),
    }
    let llegados = escenario.escuchar(Duration::from_millis(1200)).await;
    assert!(
        !llegados.iter().any(|mensaje| matches!(
            mensaje,
            MensajeServidor::Tuneles { lista } if lista.iter().any(|t| t.tunel_id == tunel)
        )),
        "la comprobación levantó el túnel automático"
    );
    // El listado del directorio va por ese canal.
    escenario
        .enviar(&MensajeCliente::ListarDir {
            host_id: host,
            ruta: "/".to_string(),
            peticion_id: (1 << 48) + 1,
        })
        .await;
    assert!(matches!(
        escenario
            .esperar(|mensaje| matches!(
                mensaje,
                MensajeServidor::DirListado { .. } | MensajeServidor::Error { .. }
            ))
            .await,
        MensajeServidor::DirListado { .. }
    ));
    // Archivos lo usa: ahora sí es un canal del host y el túnel se levanta.
    escenario
        .enviar(&MensajeCliente::AbrirSftp {
            host_id: host,
            peticion_id: None,
            no_interactivo: false,
        })
        .await;
    assert!(
        escenario
            .hasta(Duration::from_secs(10), |e| {
                e.tuneles
                    .iter()
                    .any(|t| t.tunel_id == tunel && t.estado == EstadoTunelRemoto::Activo)
            })
            .await,
        "el túnel automático no se levantó con Archivos"
    );
}
