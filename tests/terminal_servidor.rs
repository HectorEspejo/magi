//! Pruebas del servidor de terminal de la Fase 15: respuestas a las consultas
//! del remoto (una vez por pestaña), historial por lotes, ED 3, título y
//! avisos, foco agregado, portapapeles del remoto, renombrar y mover,
//! reconexión automática, tmux, escritura en varios y túneles temporales.
//!
//! Todo con el servidor SSH de pruebas de \`tests/comun\` (sin red externa): el
//! host de pruebas ejecuta un programa real de shell cuando la prueba lo pide,
//! y los binarios falsos del \`PATH\` hacen de tmux.

mod comun;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use comun::{escenario, servicio_eco, Escenario, OpcionesEscenario, Ventana};
use magi::protocolo::{
    BytesPestana, DeliberacionLanzada, EstadoSesionRemota, LineaHistorial, MensajeCliente,
    MensajeServidor, ResultadoTmux, RANGO_TERMINAL,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const PLAZO: Duration = Duration::from_secs(10);

/// Un programa para la shell del host de pruebas.
fn programa(script: &str) -> Option<Vec<String>> {
    Some(vec!["sh".to_string(), "-c".to_string(), script.to_string()])
}

/// Los textos de una captura, en orden.
fn textos(lineas: &[LineaHistorial]) -> Vec<String> {
    lineas.iter().map(LineaHistorial::texto).collect()
}

/// Modifica un host de la base de datos mientras el servidor corre.
fn con_almacen(escenario: &Escenario, cambio: impl FnOnce(&rusqlite::Connection)) {
    let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    cambio(almacen.conexion());
    almacen.cerrar().unwrap();
}

fn fijar_tmux(escenario: &Escenario, host_id: i64, valor: bool) {
    con_almacen(escenario, |conexion| {
        magi::almacen::hosts::fijar_tmux_persistente(conexion, host_id, valor).unwrap();
    });
}

/// Adjunta el cliente del escenario a una pestaña y espera el volcado.
async fn adjuntar(escenario: &mut Escenario, sesion_id: u32) {
    escenario
        .enviar(&MensajeCliente::Adjuntar {
            sesion_id,
            cols: 80,
            filas: 24,
            colores: None,
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::PantallaCompleta { sesion_id: id, .. } if *id == sesion_id
            )
        })
        .await;
}

/// Adjunta otra ventana a una pestaña.
async fn adjuntar_ventana(ventana: &mut Ventana, sesion_id: u32) {
    ventana
        .enviar(&MensajeCliente::Adjuntar {
            sesion_id,
            cols: 80,
            filas: 24,
            colores: None,
        })
        .await;
    loop {
        if let MensajeServidor::PantallaCompleta { sesion_id: id, .. } =
            ventana.esperar(|_| true).await
        {
            if id == sesion_id {
                return;
            }
        }
    }
}

/// Pide una captura completa del historial y devuelve (total, líneas).
async fn capturar(escenario: &mut Escenario, sesion_id: u32) -> (u32, Vec<LineaHistorial>) {
    let peticion_id = RANGO_TERMINAL + u64::from(sesion_id);
    escenario
        .enviar(&MensajeCliente::CapturarHistorial {
            peticion_id,
            sesion_id,
        })
        .await;
    let mut lineas = Vec::new();
    loop {
        // El último lote lleva el total; los intermedios lo repiten.
        let mensaje = escenario
            .esperar(|mensaje| {
                matches!(
                    mensaje,
                    MensajeServidor::HistorialLote { peticion_id: id, .. } if *id == peticion_id
                )
            })
            .await;
        let MensajeServidor::HistorialLote {
            lineas: trozo,
            ultimo,
            total,
            ..
        } = mensaje
        else {
            unreachable!("el filtro lo garantiza")
        };
        lineas.extend(trozo);
        if ultimo {
            return (total, lineas);
        }
    }
}

#[tokio::test]
async fn las_respuestas_salen_una_sola_vez_por_pestana() {
    // El «remoto» pregunta DA1 y DA2 y se queda esperando: las respuestas del
    // servidor se escriben al remoto (aquí, a su entrada estándar) una sola
    // vez, aunque haya dos ventanas adjuntas.
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("printf '\\033[c'; printf '\\033[>c'; exec sleep 30"),
        ..Default::default()
    })
    .await;
    let mut ventana = escenario.otra_ventana().await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    adjuntar_ventana(&mut ventana, sesion).await;

    let da1 = "\x1b[?62;22c";
    let da2 = "\x1b[>0;";
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario.observado.shell().contains(da1))
            .await,
        "el servidor no respondió a DA1"
    );
    // Se deja procesar el eco de la respuesta y se cuentan las escrituras al
    // remoto: una por consulta, aunque haya dos ventanas adjuntas.
    escenario.escuchar(Duration::from_millis(400)).await;
    let dicho = escenario.observado.shell();
    assert_eq!(
        dicho.matches(da1).count(),
        1,
        "DA1 contestada más de una vez"
    );
    assert_eq!(
        dicho.matches(da2).count(),
        1,
        "DA2 contestada más de una vez"
    );
}

#[tokio::test]
async fn la_captura_del_historial_trae_historial_y_pantalla() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa(
            "for i in $(seq 1 200); do printf '%s\\r\\n' \"$i\"; done; exec sleep 30",
        ),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    escenario.escuchar(Duration::from_millis(500)).await;
    let (total, lineas) = capturar(&mut escenario, sesion).await;
    assert_eq!(
        total as usize,
        lineas.len(),
        "el total no casa con las líneas"
    );
    assert!(
        total >= 200,
        "la captura no trajo el historial ({total} líneas)"
    );
    let textos = textos(&lineas);
    assert!(
        textos.iter().any(|texto| texto == "200"),
        "falta la última línea"
    );
    assert!(
        textos.iter().any(|texto| texto == "1"),
        "falta la primera línea"
    );
}

#[tokio::test]
async fn csi_3_j_vacia_el_historial_de_la_pestana() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("seq 1 100; printf '\\033[2J\\033[3J'; exec sleep 30"),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    escenario.escuchar(Duration::from_millis(500)).await;
    let (total, lineas) = capturar(&mut escenario, sesion).await;
    // Tras CSI 3 J solo queda la pantalla (24 filas), sin historial.
    assert_eq!(total, 24, "el historial no se vació con CSI 3 J");
    assert!(total as usize == lineas.len());
    let textos = textos(&lineas);
    assert!(
        !textos.iter().any(|texto| texto == "100"),
        "quedó una línea del historial anterior: {textos:?}"
    );
}

#[tokio::test]
async fn el_titulo_y_el_aviso_llegan_en_la_lista_de_sesiones() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("printf '\\033]2;Mi titulo\\007'; printf '\\007'; exec sleep 30"),
        ..Default::default()
    })
    .await;
    let _sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| {
                escenario.sesiones.iter().any(|sesion| {
                    sesion.titulo.as_deref() == Some("Mi titulo")
                        && sesion
                            .aviso
                            .as_ref()
                            .is_some_and(|aviso| aviso.texto == "campana")
                })
            })
            .await,
        "no llegaron el título y el aviso en Sesiones: {:?}",
        escenario.sesiones
    );
}

#[tokio::test]
async fn el_foco_agregado_avisa_al_remoto_con_csi_i_y_csi_o() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("printf '\\033[?1004h'; exec cat"),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    escenario.escuchar(Duration::from_millis(400)).await;
    escenario
        .enviar(&MensajeCliente::Foco {
            sesion_id: sesion,
            enfocada: true,
        })
        .await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| escenario
                .observado
                .shell()
                .contains("\x1b[I"))
            .await,
        "no se avisó el foco con CSI I"
    );
    escenario
        .enviar(&MensajeCliente::Foco {
            sesion_id: sesion,
            enfocada: false,
        })
        .await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| escenario
                .observado
                .shell()
                .contains("\x1b[O"))
            .await,
        "no se avisó la pérdida de foco con CSI O"
    );
}

#[tokio::test]
async fn osc_52_sin_permiso_avisa_y_con_permiso_entrega() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("printf '\\033]52;c;aG9sYQ==\\007'; exec sleep 30"),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    // Sin permiso: solo se avisa con el tamaño (no el contenido).
    let pedido = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::PortapapelesPedido { sesion_id: id, .. } if *id == sesion
            )
        })
        .await;
    let MensajeServidor::PortapapelesPedido { bytes, .. } = pedido else {
        unreachable!()
    };
    assert_eq!(bytes, 4, "el aviso lleva el tamaño en bytes");
    // Prefijo P: el cliente permite y el servidor entrega la pendiente.
    escenario
        .enviar(&MensajeCliente::PortapapelesPermitido { sesion_id: sesion })
        .await;
    let entrega = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Portapapeles { sesion_id: id, .. } if *id == sesion
            )
        })
        .await;
    let MensajeServidor::Portapapeles { texto, .. } = entrega else {
        unreachable!()
    };
    assert_eq!(texto, "hola");
    // HOSTS.portapapeles_remoto lo escribe el cliente (es su ficha, `4.10):
    // el servidor no toca esa columna.
}

#[tokio::test]
async fn renombrar_y_mover_pestanas() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        ..Default::default()
    })
    .await;
    let primera = escenario.abrir_sesion(escenario.hosts[0]).await;
    let segunda = escenario.abrir_sesion(escenario.hosts[0]).await;
    // Renombrar la primera.
    escenario
        .enviar(&MensajeCliente::RenombrarSesion {
            sesion_id: primera,
            nombre: "zeta".to_string(),
        })
        .await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario
                .sesiones
                .iter()
                .any(|sesion| sesion.id == primera && sesion.nombre == "zeta"))
            .await
    );
    // El mismo nombre en otra pestaña se hace único con « (n)».
    escenario
        .enviar(&MensajeCliente::RenombrarSesion {
            sesion_id: segunda,
            nombre: "zeta".to_string(),
        })
        .await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario.sesiones.iter().any(
                |sesion| sesion.id == segunda && sesion.nombre == "zeta (2)"
            ))
            .await,
        "no se hizo único el nombre: {:?}",
        escenario.sesiones
    );
    // Mover la primera al final: el orden de la lista cambia.
    escenario
        .enviar(&MensajeCliente::MoverSesion {
            sesion_id: primera,
            posicion: 1,
        })
        .await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| {
                escenario
                    .sesiones
                    .iter()
                    .map(|sesion| sesion.id)
                    .collect::<Vec<u32>>()
                    == vec![segunda, primera]
            })
            .await,
        "la lista no cambió de orden: {:?}",
        escenario.sesiones
    );
}

#[tokio::test]
async fn la_reconexion_automatica_reabre_la_pestana() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("exec sleep 30"),
        ..Default::default()
    })
    .await;
    let sesion_id = escenario.abrir_sesion(escenario.hosts[0]).await;
    escenario.escuchar(Duration::from_millis(300)).await;
    escenario.observado.tirar_conexiones().await;
    // Estado «reconectando» con cuenta atrás en la lista.
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario.sesiones.iter().any(
                |resumen| resumen.id == sesion_id && resumen.reconexion.is_some()
            ))
            .await,
        "la pestaña no pasó a reconectando: {:?}",
        escenario.sesiones
    );
    // Y se reabre sola, sin diálogos.
    assert!(
        escenario
            .hasta(Duration::from_secs(30), |escenario| escenario
                .sesiones
                .iter()
                .any(|resumen| resumen.id == sesion_id
                    && resumen.estado == EstadoSesionRemota::Abierta))
            .await,
        "la pestaña no se reconectó sola: {:?}",
        escenario.sesiones
    );
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario
                .registro()
                .iter()
                .any(|(tipo, detalle)| tipo == "sesion_reconectada"
                    && detalle.contains("automática")))
            .await,
        "no se anotó sesion_reconectada «automática»: {:?}",
        escenario.registro()
    );
}

#[tokio::test]
async fn tmux_persistente_abre_la_sesion_y_lista_y_termina() {
    // El fake de tmux guarda su estado en un fichero TSV: una sesión por
    // línea (nombre, adheridos, ventanas, creada).
    let estado = tempfile::tempdir().unwrap();
    let fichero = estado.path().join("tmux_estado");
    let llamadas = estado.path().join("tmux_llamadas");
    let equipo = magi_nucleo::terminal::tmux::etiqueta_equipo(&magi::config::nombre_actual());
    let esperada = format!("magi-{equipo}-2");
    std::fs::write(
        &fichero,
        format!("magi-{equipo}-1\t0\t1\t2026-11-09 19:20:00\n"),
    )
    .unwrap();
    let binarios = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fijos/terminal");
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: Some(vec!["sh".to_string()]),
        binarios: Some(binarios),
        entorno_extra: vec![
            (
                "MAGI_TMUX_ESTADO".to_string(),
                fichero.display().to_string(),
            ),
            (
                "MAGI_TMUX_LLAMADAS".to_string(),
                llamadas.display().to_string(),
            ),
        ],
        ..Default::default()
    })
    .await;
    let host_id = escenario.hosts[0];
    fijar_tmux(&escenario, host_id, true);
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    assert!(
        escenario
            .hasta(Duration::from_secs(15), |escenario| escenario
                .observado
                .programa_salida()
                .contains(&format!("MAGI_TMUX_SESION={esperada}")))
            .await,
        "no se enganchó la sesión de tmux nueva: {:?}",
        escenario.observado.programa_salida()
    );
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario
                .sesiones
                .iter()
                .any(|resumen| resumen.id == sesion
                    && resumen.tmux.as_deref() == Some(esperada.as_str())))
            .await,
        "InfoSesion.tmux no lleva el nombre: {:?}",
        escenario.sesiones
    );
    let listado = std::fs::read_to_string(&fichero).unwrap();
    assert!(
        listado.contains(&esperada),
        "tmux no creó la sesión: {listado}"
    );
    assert!(
        std::fs::read_to_string(&llamadas)
            .unwrap_or_default()
            .contains("new-session"),
        "el script no pidió abrir la sesión al tmux falso"
    );

    // La vista Sesiones lista las sesiones del host.
    let peticion = RANGO_TERMINAL + 100;
    escenario
        .enviar(&MensajeCliente::ListarTmux {
            peticion_id: peticion,
            host_id,
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::SesionesTmux { peticion_id: id, .. } if *id == peticion
            )
        })
        .await;
    let MensajeServidor::SesionesTmux {
        sesiones, sin_tmux, ..
    } = mensaje
    else {
        unreachable!()
    };
    assert!(sin_tmux.is_none());
    assert!(sesiones
        .iter()
        .any(|sesion| sesion.nombre == format!("magi-{equipo}-1")));

    // Terminar la sesión que ya existía, con su anotación.
    escenario
        .enviar(&MensajeCliente::TerminarTmux {
            peticion_id: peticion + 1,
            host_id,
            nombre: format!("magi-{equipo}-1"),
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::TmuxTerminada { peticion_id: id, .. } if *id == peticion + 1
            )
        })
        .await;
    assert!(matches!(
        mensaje,
        MensajeServidor::TmuxTerminada {
            resultado: ResultadoTmux::Ok,
            ..
        }
    ));
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario
                .registro()
                .iter()
                .any(|(tipo, _)| tipo == "tmux_terminada"))
            .await,
        "no se anotó tmux_terminada"
    );
}

#[tokio::test]
async fn la_escritura_en_varios_solo_escribe_en_lo_concedido() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        ..Default::default()
    })
    .await;
    let primera = escenario.abrir_sesion(escenario.hosts[0]).await;
    let segunda = escenario.abrir_sesion(escenario.hosts[0]).await;
    escenario.escuchar(Duration::from_millis(300)).await;
    let peticion = RANGO_TERMINAL + 200;
    escenario
        .enviar(&MensajeCliente::IniciarEscrituraVarios {
            peticion_id: peticion,
            sesiones: vec![primera, segunda],
            deliberacion_id: None,
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::EscrituraVariosIniciada { peticion_id: id, .. } if *id == peticion
            )
        })
        .await;
    let MensajeServidor::EscrituraVariosIniciada { id, .. } = mensaje else {
        unreachable!()
    };
    // Se escribe en una concedida.
    escenario
        .enviar(&MensajeCliente::EscribirVarios {
            id,
            pestanas: vec![BytesPestana {
                sesion_id: primera,
                bytes: b"hola".to_vec(),
            }],
        })
        .await;
    assert!(
        escenario
            .hasta(Duration::from_secs(5), |escenario| escenario
                .observado
                .shell()
                .contains("hola"))
            .await,
        "no llegó lo escrito a la pestaña concedida"
    );
    // Una pestaña no concedida rechaza el envío entero y termina la concesión.
    escenario
        .enviar(&MensajeCliente::EscribirVarios {
            id,
            pestanas: vec![BytesPestana {
                sesion_id: 9_999,
                bytes: b"mal".to_vec(),
            }],
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::EscrituraVariosTerminada { id: otra, .. } if *otra == id
            )
        })
        .await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| {
                escenario.anotaciones("escritura_varios_activada") > 0
                    && escenario.anotaciones("escritura_varios_terminada") > 0
            })
            .await,
        "faltan las anotaciones de la escritura en varios: {:?}",
        escenario.registro()
    );
}

#[tokio::test]
async fn el_tunel_temporal_lleva_al_localhost_del_host() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("exec sleep 30"),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    let (puerto_eco, tarea_eco) = servicio_eco().await;
    let peticion = RANGO_TERMINAL + 300;
    escenario
        .enviar(&MensajeCliente::AbrirTunelTemporal {
            peticion_id: peticion,
            sesion_id: sesion,
            destino: "127.0.0.1".to_string(),
            puerto: puerto_eco,
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::TunelTemporal { peticion_id: id, .. } if *id == peticion
            )
        })
        .await;
    let MensajeServidor::TunelTemporal {
        id, puerto_local, ..
    } = mensaje
    else {
        unreachable!()
    };
    // La página del «host» llega por el túnel.
    let mut flujo = tokio::net::TcpStream::connect(("127.0.0.1", puerto_local))
        .await
        .expect("conectando al túnel temporal");
    flujo.write_all(b"hola").await.unwrap();
    let mut bufer = vec![0u8; 16];
    let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
        .await
        .expect("el eco no contestó")
        .unwrap();
    assert_eq!(&bufer[..leidos], b"hola");
    // La lista completa de túneles temporales lo tiene.
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario
                .tuneles_temporales
                .iter()
                .any(|tunel| tunel.id == id))
            .await,
        "no se difundió la lista de túneles temporales: {:?}",
        escenario.tuneles_temporales
    );
    // Cerrar el túnel lo quita de la lista y deja de escuchar.
    escenario
        .enviar(&MensajeCliente::CerrarTunelTemporal { id })
        .await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario
                .tuneles_temporales
                .iter()
                .all(|tunel| tunel.id != id))
            .await,
        "el túnel temporal no se cerró: {:?}",
        escenario.tuneles_temporales
    );
    assert!(tokio::net::TcpStream::connect(("127.0.0.1", puerto_local))
        .await
        .is_err());
    tarea_eco.abort();
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario.registro().iter().any(
                |(tipo, detalle)| tipo == "tunel_abierto" && detalle.contains("temporal")
            ))
            .await,
        "no se anotó el túnel como temporal"
    );
}

#[tokio::test]
async fn la_escritura_en_varios_se_ve_en_las_otras_ventanas() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        ..Default::default()
    })
    .await;
    let primera = escenario.abrir_sesion(escenario.hosts[0]).await;
    let segunda = escenario.abrir_sesion(escenario.hosts[0]).await;
    let tercera = escenario.abrir_sesion(escenario.hosts[0]).await;
    // Una segunda ventana: la marca «»» la tienen que ver todas, no solo la
    // que activa la escritura en varios (§7.14).
    let mut ventana = escenario.otra_ventana().await;
    let peticion = RANGO_TERMINAL + 500;
    escenario
        .enviar(&MensajeCliente::IniciarEscrituraVarios {
            peticion_id: peticion,
            sesiones: vec![primera, segunda],
            deliberacion_id: None,
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::EscrituraVariosIniciada { peticion_id: id, .. } if *id == peticion
            )
        })
        .await;
    let MensajeServidor::EscrituraVariosIniciada { id, .. } = mensaje else {
        unreachable!()
    };
    // La ventana que no la pidió ve la lista con las dos concedidas marcadas y
    // la tercera sin marcar.
    let mensaje = ventana
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Sesiones { lista }
                    if lista.iter().any(|sesion| sesion.id == primera
                        && sesion.escritura_varios)
                        && lista.iter().any(|sesion| sesion.id == segunda
                            && sesion.escritura_varios)
            )
        })
        .await;
    let MensajeServidor::Sesiones { lista } = mensaje else {
        unreachable!()
    };
    assert!(
        !lista
            .iter()
            .find(|sesion| sesion.id == tercera)
            .expect("la tercera pestaña está en la lista")
            .escritura_varios,
        "una pestaña no concedida no lleva la marca: {lista:?}"
    );
    // El dueño también la ve en su propia lista.
    assert!(
        escenario
            .sesiones
            .iter()
            .any(|sesion| sesion.id == primera && sesion.escritura_varios),
        "{:?}",
        escenario.sesiones
    );
    // Al terminar (motivo del cliente) la marca desaparece para todas.
    escenario
        .enviar(&MensajeCliente::TerminarEscrituraVarios {
            id,
            motivo: "apagada".to_string(),
        })
        .await;
    let mensaje = ventana
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Sesiones { lista }
                    if lista.iter().all(|sesion| !sesion.escritura_varios)
            )
        })
        .await;
    let MensajeServidor::Sesiones { lista } = mensaje else {
        unreachable!()
    };
    assert!(
        lista
            .iter()
            .any(|sesion| sesion.id == primera || sesion.id == segunda),
        "la lista de después del cierre perdió pestañas: {lista:?}"
    );
}

/// Una deliberación de escritura en varios ya aprobada, como la escribe el
/// cliente: snippet NULL, acción con el prefijo y hosts ordenados por id.
fn crear_deliberacion_escritura(escenario: &Escenario, hosts: &[(i64, String)]) -> i64 {
    let mut ordenados = hosts.to_vec();
    ordenados.sort_by_key(|(host_id, _)| *host_id);
    let accion = format!(
        "escritura en varios: {}",
        ordenados
            .iter()
            .map(|(_, nombre)| nombre.as_str())
            .collect::<Vec<&str>>()
            .join(" · ")
    );
    let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let id = almacen
        .crear_deliberacion(&magi::deliberacion::NuevaDeliberacion {
            snippet_id: None,
            accion,
            hosts: ordenados,
            comprobaciones: Vec::new(),
            resultado: magi::deliberacion::ResultadoDeliberacion::Aprobada,
            bloqueada: false,
            motivo: None,
            usuario: "usuario".to_string(),
        })
        .unwrap();
    almacen.cerrar().unwrap();
    id
}

#[tokio::test]
async fn un_tamano_de_terminal_desmedido_se_recorta() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("exec sleep 30"),
        ..Default::default()
    })
    .await;
    let host = escenario.hosts[0];
    escenario
        .enviar(&MensajeCliente::AbrirSesion {
            host_id: host,
            cols: 65_535,
            filas: 65_535,
            comandos_iniciales: Vec::new(),
            orden: None,
            tmux: None,
        })
        .await;
    let sesion = escenario.esperar_sesion_nueva(&HashSet::new()).await;
    // El PTY se pide con el tamaño recortado al tope del servidor.
    assert!(
        escenario
            .hasta(PLAZO, |escenario| !escenario.observado.ptys().is_empty())
            .await,
        "no llegó ningún pty-req: {:?} sesiones {:?}",
        escenario.observado.ptys(),
        escenario.sesiones
    );
    assert_eq!(
        escenario.observado.ptys().last().copied(),
        Some((1000, 2000)),
        "el tamaño desmedido llegó al PTY: {:?}",
        escenario.observado.ptys()
    );
    // El servidor sigue vivo: otra pestaña normal se abre después.
    let otra = escenario.abrir_sesion(host).await;
    assert_ne!(otra, sesion);
    // Y un Redimensionar desmedido también se recorta sin tumbar nada. Hay
    // que estar adjunto para que el tamaño cuente.
    adjuntar(&mut escenario, sesion).await;
    escenario
        .enviar(&MensajeCliente::Redimensionar {
            sesion_id: sesion,
            cols: 65_535,
            filas: 65_535,
        })
        .await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario
                .observado
                .tamanos()
                .last()
                .copied()
                == Some((1000, 2000)))
            .await,
        "el redimensionado desmedido no se recortó: {:?}",
        escenario.observado.tamanos()
    );
}

#[tokio::test]
async fn la_deliberacion_de_escritura_en_varios_no_se_reutiliza() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 2,
        ..Default::default()
    })
    .await;
    let host_uno = escenario.hosts[0];
    let host_dos = escenario.hosts[1];
    let primera = escenario.abrir_sesion(host_uno).await;
    let segunda = escenario.abrir_sesion(host_dos).await;
    let deliberacion = crear_deliberacion_escritura(
        &escenario,
        &[
            (host_uno, "prueba-1".to_string()),
            (host_dos, "prueba-2".to_string()),
        ],
    );
    let peticion = RANGO_TERMINAL + 600;
    escenario
        .enviar(&MensajeCliente::IniciarEscrituraVarios {
            peticion_id: peticion,
            sesiones: vec![primera, segunda],
            deliberacion_id: Some(deliberacion),
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::EscrituraVariosIniciada { peticion_id: id, .. } if *id == peticion
            )
        })
        .await;
    // La misma deliberación no autoriza una segunda concesión.
    let peticion_repetida = peticion + 1;
    escenario
        .enviar(&MensajeCliente::IniciarEscrituraVarios {
            peticion_id: peticion_repetida,
            sesiones: vec![primera, segunda],
            deliberacion_id: Some(deliberacion),
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Error { peticion_id: Some(id), .. } if *id == peticion_repetida
            )
        })
        .await;
    let MensajeServidor::Error { mensaje, .. } = mensaje else {
        unreachable!()
    };
    assert!(mensaje.contains("ya autorizó"), "{mensaje}");
    // Y una ejecución con la misma deliberación también se rechaza: «ya usada»
    // vale en los dos sentidos.
    let peticion_ejecucion = 1 << 44;
    escenario
        .enviar(&MensajeCliente::LanzarEjecucion {
            peticion_id: peticion_ejecucion,
            snippet_id: None,
            nombre: "prueba".to_string(),
            comando: "true".to_string(),
            host_ids: vec![host_uno],
            timeout_seg: 60,
            parar_al_fallo: false,
            deliberacion: Some(DeliberacionLanzada {
                id: deliberacion,
                forzada: false,
                motivo: None,
            }),
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Error { peticion_id: Some(id), .. } if *id == peticion_ejecucion
            )
        })
        .await;
    let MensajeServidor::Error { mensaje, .. } = mensaje else {
        unreachable!()
    };
    assert!(mensaje.contains("ya autorizó"), "{mensaje}");
}

#[tokio::test]
async fn al_adjuntar_tarde_el_parser_ve_modos_y_pantalla_alternativa() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa(
            "printf '\\033[?1049h'; printf '\\033[?2004h\\033[?1002h\\033[?1006h'; \\
             printf 'PANTALLA ALT'; exec sleep 30",
        ),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    escenario.escuchar(Duration::from_millis(500)).await;
    escenario
        .enviar(&MensajeCliente::Adjuntar {
            sesion_id: sesion,
            cols: 80,
            filas: 24,
            colores: None,
        })
        .await;
    let mensaje = escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::PantallaCompleta { sesion_id: id, .. } if *id == sesion
            )
        })
        .await;
    let MensajeServidor::PantallaCompleta {
        bytes, cols, filas, ..
    } = mensaje
    else {
        unreachable!()
    };
    // El parser nuevo del cliente, alimentado con el volcado, tiene que quedar
    // como el del servidor: alternativa, pegado entre corchetes y ratón.
    let mut parser = vt100::Parser::new(filas, cols, 0);
    parser.process(&bytes);
    assert!(
        parser.screen().alternate_screen(),
        "el volcado no lleva la pantalla alternativa: {:?}",
        String::from_utf8_lossy(&bytes)
    );
    assert!(
        parser.screen().bracketed_paste(),
        "el volcado no lleva el pegado entre corchetes (2004)"
    );
    assert_eq!(
        parser.screen().mouse_protocol_mode(),
        vt100::MouseProtocolMode::ButtonMotion,
        "el volcado no lleva el modo de ratón 1002"
    );
    assert_eq!(
        parser.screen().mouse_protocol_encoding(),
        vt100::MouseProtocolEncoding::Sgr,
        "el volcado no lleva la codificación SGR (1006)"
    );
    assert!(
        parser.screen().contents().contains("PANTALLA ALT"),
        "el volcado no llevó lo que se veía en la alternativa: {:?}",
        parser.screen().contents()
    );
}

#[tokio::test]
async fn un_fallo_de_tunel_temporal_se_anota() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa("exec sleep 30"),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    // Destino fuera del host: se rechaza y se anota.
    let peticion = RANGO_TERMINAL + 700;
    escenario
        .enviar(&MensajeCliente::AbrirTunelTemporal {
            peticion_id: peticion,
            sesion_id: sesion,
            destino: "ejemplo.org".to_string(),
            puerto: 80,
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Error { peticion_id: Some(id), .. } if *id == peticion
            )
        })
        .await;
    // Pestaña inexistente: también.
    let peticion_dos = peticion + 1;
    escenario
        .enviar(&MensajeCliente::AbrirTunelTemporal {
            peticion_id: peticion_dos,
            sesion_id: 9_999,
            destino: "127.0.0.1".to_string(),
            puerto: 80,
        })
        .await;
    escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Error { peticion_id: Some(id), .. } if *id == peticion_dos
            )
        })
        .await;
    assert!(
        escenario
            .hasta(PLAZO, |escenario| escenario.anotaciones("tunel_fallido")
                >= 2)
            .await,
        "faltan los tunel_fallido: {:?}",
        escenario.registro()
    );
    let filas = escenario.registro();
    let fallos: Vec<&(String, String)> = filas
        .iter()
        .filter(|(tipo, _)| tipo == "tunel_fallido")
        .collect();
    assert!(
        fallos
            .iter()
            .all(|(_, detalle)| detalle.contains("temporal")),
        "algún fallo no se anotó como temporal: {fallos:?}"
    );
}

#[tokio::test]
async fn la_configuracion_fuera_de_rango_se_acota() {
    let mut config = magi::config::Config::default();
    config.terminal.historial_lineas = 200_000;
    config.terminal.historial_mib = 1024;
    config.terminal.reconexion_max_min = 0;
    assert_eq!(config.terminal.historial_lineas_efectivo(), 100_000);
    assert_eq!(config.terminal.historial_mib_efectivo(), 512);
    assert_eq!(config.terminal.reconexion_max_min_efectivo(), 1);
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        config: Some(config),
        programa_shell: programa(
            "for i in $(seq 1 300); do printf '%s\\r\\n' \"$i\"; done; exec sleep 30",
        ),
        ..Default::default()
    })
    .await;
    let sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    adjuntar(&mut escenario, sesion).await;
    escenario.escuchar(Duration::from_millis(500)).await;
    // El servidor arrancó con la configuración acotada y el historial sigue
    // funcionando (y acotado por el tope efectivo).
    let (total, lineas) = capturar(&mut escenario, sesion).await;
    assert!(total >= 300, "la captura no trajo el historial: {total}");
    assert_eq!(total as usize, lineas.len());
}

#[tokio::test]
async fn el_titulo_no_hace_una_tormenta_de_listas() {
    let mut escenario = escenario(OpcionesEscenario {
        hosts: 1,
        programa_shell: programa(
            "for i in $(seq 1 50); do printf '\\033]2;titulo %s\\007' \"$i\"; done; \
             exec sleep 30",
        ),
        ..Default::default()
    })
    .await;
    let _sesion = escenario.abrir_sesion(escenario.hosts[0]).await;
    // Cincuenta cambios de título seguidos no pueden producir cincuenta
    // difusiones: la lista sale coalescida.
    let vistos = escenario.escuchar(Duration::from_secs(2)).await;
    let listas = vistos
        .iter()
        .filter(|mensaje| matches!(mensaje, MensajeServidor::Sesiones { .. }))
        .count();
    assert!(
        listas <= 4,
        "demasiadas difusiones de Sesiones por los títulos: {listas}"
    );
}
