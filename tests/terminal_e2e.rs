//! Prueba de extremo a extremo del terminal (Fase 15) sin red externa: el
//! servidor de sesiones en proceso abre una pestaña en el servidor SSH de
//! prueba, que ejecuta `tests/fijos/terminal/tui_prueba.rs` (un programa que
//! hace lo que Claude Code: consultas, pegado entre corchetes, teclado de
//! kitty, salida sincronizada, ratón, OSC 52, campana y título).
//!
//! Comprueba lo que el informe exige de la cadena completa: el programa recibe
//! las respuestas a sus consultas (una sola vez), un pegado de cinco líneas
//! llega como un solo bloque y Mayús+Intro llega como `CSI 13;2u`.
//!
//! Desde la Fase 16 también cubre `modifyOtherKeys` (lo que tmux pide a su
//! terminal, sin tmux): el programa lo pide con `CSI > 4 ; 2 m`, el emulador del
//! cliente lo recoge de lo que manda el servidor, MAGI codifica Mayús+Intro
//! como `CSI 27;2;13~` y el programa lo reconoce; y una ventana que se adjunta
//! después recibe el modo en el volcado.

mod comun;

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use magi::protocolo::{MensajeCliente, MensajeServidor};
use magi_nucleo::terminal::emulador::PantallaEmulada;
use magi_nucleo::terminal::pegado::{pegar, sanear};
use magi_nucleo::terminal::raton::{
    codificar as codificar_raton, Accion, Boton, Codificacion, Evento, Modo, ModoRaton,
};
use magi_nucleo::terminal::teclas::{
    codificar as codificar_tecla, Modificadores, ModoTerminal, Tecla,
};

use comun::OpcionesEscenario;

/// Compila `tui_prueba` y devuelve su ruta. Se compila una sola vez por proceso
/// (las pruebas corren en paralelo y comparten el binario) con el `rustc` del
/// toolchain que ejecuta los tests (el programa solo usa std).
fn compilar_tui_prueba() -> PathBuf {
    static COMPILADO: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    COMPILADO
        .get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("magi-tui-prueba-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("directorio del programa de prueba");
            let salida = dir.join(if cfg!(windows) {
                "tui_prueba.exe"
            } else {
                "tui_prueba"
            });
            let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
            let estado = Command::new(rustc)
                .args(["--edition=2021", "-O", "-o"])
                .arg(&salida)
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fijos/terminal/tui_prueba.rs"
                ))
                .status()
                .expect("compilando tui_prueba con rustc");
            assert!(estado.success(), "tui_prueba no compila");
            salida
        })
        .clone()
}

/// El modo de teclas que tendría una ventana cliente: reproduce en un emulador
/// que no responde (el del cliente) lo que el servidor mandó de la pestaña, en
/// orden, y lee su modo como lo hace la vista Sesión.
fn modo_del_cliente(vistos: &[MensajeServidor], sesion_id: u32) -> ModoTerminal {
    let mut cliente = PantallaEmulada::nueva(24, 80, 0, false);
    for mensaje in vistos {
        match mensaje {
            MensajeServidor::PantallaCompleta {
                sesion_id: id,
                bytes,
                cols,
                filas,
            } if *id == sesion_id => {
                cliente.redimensionar(*filas, *cols, 0);
                cliente.procesar(bytes);
            }
            MensajeServidor::Datos {
                sesion_id: id,
                bytes,
            } if *id == sesion_id => {
                cliente.procesar(bytes);
            }
            _ => {}
        }
    }
    ModoTerminal::desde_emulador(&cliente)
}

/// Espera a que la salida del programa de la shell contenga `texto`.
async fn esperar_programa(escenario: &comun::Escenario, texto: &str) {
    let fin = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if escenario.observado.programa_salida().contains(texto) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < fin,
            "tui_prueba no dijo «{texto}»; salida: {:?}",
            escenario.observado.programa_salida()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[test]
fn tui_prueba_recibe_respuestas_pegado_y_mayus_intro() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime del escenario");
    runtime.block_on(async {
        let programa = compilar_tui_prueba();
        let mut escenario = comun::escenario(OpcionesEscenario {
            hosts: 1,
            programa_shell: Some(vec![programa.to_string_lossy().to_string()]),
            ..OpcionesEscenario::default()
        })
        .await;
        let host_id = escenario.hosts[0];
        let sesion_id = escenario.abrir_sesion(host_id).await;
        // El servidor solo reenvía las teclas de una ventana adjunta.
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

        // 1. Las consultas del programa al arrancar: el servidor responde.
        esperar_programa(&escenario, "LISTO").await;
        let salida = escenario.observado.programa_salida();
        for check in [
            "CHECK DA1=ok",
            "CHECK DA2=ok",
            "CHECK KITTY=ok",
            "CHECK DECRQM2026=ok",
            "CHECK OSCC=ok",
        ] {
            assert!(salida.contains(check), "falta {check} en {salida:?}");
        }
        // Una respuesta por consulta, aunque la pestaña esté adjunta a una
        // ventana: DA1 solo aparece una vez en lo recibido.
        // Las consultas se escriben sin salto, así que la línea de RECIBIDO
        // puede ir pegada a ellas: se busca el texto, no el principio de línea.
        let recibido = salida
            .split("RECIBIDO ")
            .nth(1)
            .and_then(|resto| resto.split('\r').next())
            .unwrap_or_default();
        let da1 = "1b5b3f36323b323263"; // ESC [ ? 6 2 ; 2 2 c
        assert_eq!(
            recibido.matches(da1).count(),
            1,
            "DA1 debe responderse una sola vez: {recibido}"
        );

        // 2. Pegado de cinco líneas: llega como un solo bloque entre marcas
        //    (el programa activó 2004 al arrancar).
        escenario.observado.limpiar_programa();
        let texto = sanear("uno\ndos\ntres\ncuatro\ncinco\n");
        let bytes = pegar(&texto, true).expect("el pegado no está vacío");
        escenario
            .enviar(&MensajeCliente::Teclas { sesion_id, bytes })
            .await;
        esperar_programa(&escenario, "PEGADO 5").await;

        // 3. Mayús+Intro con la bandera 1 del teclado de kitty.
        escenario.observado.limpiar_programa();
        let intro = codificar_tecla(
            Tecla::Intro,
            Modificadores {
                mayusculas: true,
                alt: false,
                ctrl: false,
            },
            ModoTerminal {
                banderas_kitty: 1,
                ..ModoTerminal::default()
            },
        )
        .expect("Mayús+Intro tiene codificación de kitty");
        assert_eq!(intro, b"\x1b[13;2u");
        escenario
            .enviar(&MensajeCliente::Teclas {
                sesion_id,
                bytes: intro,
            })
            .await;
        esperar_programa(&escenario, "MAYUS_INTRO").await;

        // 4. Un evento de ratón SGR llega al programa.
        escenario.observado.limpiar_programa();
        let rueda = codificar_raton(
            &Evento {
                accion: Accion::RuedaArriba,
                boton: Boton::Ninguno,
                columna: 3,
                fila: 2,
                modificadores: Modificadores::default(),
            },
            ModoRaton {
                modo: Modo::TodoMovimiento,
                codificacion: Codificacion::Sgr,
            },
        )
        .expect("el ratón se codifica");
        escenario
            .enviar(&MensajeCliente::Teclas {
                sesion_id,
                bytes: rueda,
            })
            .await;
        esperar_programa(&escenario, "RATON").await;

        // 5. Lo que el cliente escribió en la shell: el pegado con sus marcas,
        //    el Mayús+Intro de kitty y el ratón SGR, sin teclas sueltas.
        let escrito = escenario.observado.shell();
        assert!(escrito.contains("\x1b[200~"), "falta la marca de pegado");
        assert!(escrito.contains("\x1b[201~"), "falta el cierre del pegado");
        assert!(escrito.contains("\x1b[13;2u"), "falta Mayús+Intro");
        assert!(escrito.contains("\x1b[<64;"), "falta el ratón SGR");

        escenario
            .enviar(&MensajeCliente::Cerrar { sesion_id })
            .await;
    });
}

/// Mayús+Intro, Ctrl+Intro y Ctrl+A con `modifyOtherKeys`: el programa lo pide
/// (como tmux), el emulador del cliente lo recoge del flujo del servidor y MAGI
/// codifica las teclas con la forma de xterm.
#[test]
fn modify_other_keys_mayus_intro_llega_como_csi_27_2_13() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime del escenario");
    runtime.block_on(async {
        let programa = compilar_tui_prueba();
        let mut escenario = comun::escenario(OpcionesEscenario {
            hosts: 1,
            programa_shell: Some(vec![
                programa.to_string_lossy().to_string(),
                "--modify-other-keys".to_string(),
            ]),
            ..OpcionesEscenario::default()
        })
        .await;
        let host_id = escenario.hosts[0];
        let sesion_id = escenario.abrir_sesion(host_id).await;
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
        esperar_programa(&escenario, "LISTO").await;

        // El modo llega al emulador del cliente por el flujo normal de datos.
        let recogido = escenario
            .hasta(Duration::from_secs(10), |escenario| {
                modo_del_cliente(&escenario.vistos, sesion_id).modify_other_keys == 2
            })
            .await;
        assert!(recogido, "el cliente no recogió modifyOtherKeys nivel 2");
        let modo = modo_del_cliente(&escenario.vistos, sesion_id);

        // Criterio de aceptación: Mayús+Intro es exactamente ESC [ 2 7 ; 2 ; 1 3 ~.
        escenario.observado.limpiar_programa();
        let mayus_intro = codificar_tecla(
            Tecla::Intro,
            Modificadores {
                mayusculas: true,
                alt: false,
                ctrl: false,
            },
            modo,
        )
        .expect("Mayús+Intro tiene codificación con modifyOtherKeys");
        assert_eq!(mayus_intro, b"\x1b[27;2;13~");
        escenario
            .enviar(&MensajeCliente::Teclas {
                sesion_id,
                bytes: mayus_intro,
            })
            .await;
        esperar_programa(&escenario, "MAYUS_INTRO").await;

        // Ctrl+Intro y, con el nivel 2, Ctrl+A.
        escenario.observado.limpiar_programa();
        let ctrl = Modificadores {
            mayusculas: false,
            alt: false,
            ctrl: true,
        };
        let ctrl_intro = codificar_tecla(Tecla::Intro, ctrl, modo).expect("Ctrl+Intro");
        assert_eq!(ctrl_intro, b"\x1b[27;5;13~");
        let ctrl_a = codificar_tecla(Tecla::Caracter('a'), ctrl, modo).expect("Ctrl+A");
        assert_eq!(ctrl_a, b"\x1b[27;5;97~");
        for bytes in [ctrl_intro, ctrl_a] {
            escenario
                .enviar(&MensajeCliente::Teclas { sesion_id, bytes })
                .await;
        }
        esperar_programa(&escenario, "TECLA27 5 13").await;
        esperar_programa(&escenario, "TECLA27 5 97").await;

        // Lo que MAGI escribió en la shell son las secuencias de xterm.
        let escrito = escenario.observado.shell();
        assert!(escrito.contains("\x1b[27;2;13~"), "falta Mayús+Intro");
        assert!(escrito.contains("\x1b[27;5;13~"), "falta Ctrl+Intro");
        assert!(escrito.contains("\x1b[27;5;97~"), "falta Ctrl+A");

        escenario
            .enviar(&MensajeCliente::Cerrar { sesion_id })
            .await;
    });
}

/// Una ventana que se adjunta después de que la pestaña activara
/// `modifyOtherKeys` recibe el modo en el volcado, detrás de `state_formatted`
/// y de la bandera de kitty, y con las dos puestas gana kitty (como en la
/// codificación local).
#[test]
fn modify_other_keys_llega_a_una_ventana_que_se_adjunta_despues() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime del escenario");
    runtime.block_on(async {
        let mut escenario = comun::escenario(OpcionesEscenario {
            hosts: 1,
            programa_shell: Some(vec![
                "sh".to_string(),
                "-c".to_string(),
                "printf '\\033[?2004h\\033[>1u\\033[>4;1mHECHO'; exec sleep 30".to_string(),
            ]),
            ..OpcionesEscenario::default()
        })
        .await;
        let host_id = escenario.hosts[0];
        let sesion_id = escenario.abrir_sesion(host_id).await;
        // Nadie está adjunto: el servidor procesa la salida con su emulador.
        esperar_programa(&escenario, "HECHO").await;
        escenario.escuchar(Duration::from_millis(500)).await;
        escenario
            .enviar(&MensajeCliente::Adjuntar {
                sesion_id,
                cols: 80,
                filas: 24,
                colores: None,
            })
            .await;
        let volcado = escenario
            .esperar(|mensaje| {
                matches!(
                    mensaje,
                    MensajeServidor::PantallaCompleta { sesion_id: id, .. } if *id == sesion_id
                )
            })
            .await;
        let MensajeServidor::PantallaCompleta { bytes, .. } = volcado else {
            unreachable!()
        };
        // Orden del volcado: el contenido y los modos de vt100, la bandera de
        // kitty y, por último, modifyOtherKeys.
        assert!(
            bytes.ends_with(b"\x1b[>1u\x1b[>4;1m"),
            "el volcado no acaba en la bandera de kitty y modifyOtherKeys: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        let modo = modo_del_cliente(&escenario.vistos, sesion_id);
        assert_eq!(modo.modify_other_keys, 1);
        assert_eq!(modo.banderas_kitty, 1);
        let mayus = Modificadores {
            mayusculas: true,
            alt: false,
            ctrl: false,
        };
        assert_eq!(
            codificar_tecla(Tecla::Intro, mayus, modo).unwrap(),
            b"\x1b[13;2u",
            "con kitty y modifyOtherKeys a la vez gana kitty"
        );
        // Sin la bandera de kitty, el nivel 1 codifica Mayús+Intro con la forma 27.
        let solo_mok = ModoTerminal {
            banderas_kitty: 0,
            ..modo
        };
        assert_eq!(
            codificar_tecla(Tecla::Intro, mayus, solo_mok).unwrap(),
            b"\x1b[27;2;13~"
        );

        escenario
            .enviar(&MensajeCliente::Cerrar { sesion_id })
            .await;
    });
}

/// Regresión (revisión adversarial de la pista D): una ventana que ya tenía
/// `modifyOtherKeys` activo y se vuelve a adjuntar cuando el remoto lo apagó
/// mientras estaba soltada debe quedar con el modo apagado. El cliente reutiliza
/// el emulador de la pestaña entre adjunciones y `state_formatted` no lleva el
/// modo: sin un `CSI > 4 ; 0 m` en el volcado, Ctrl+A en el shell saldría como
/// `CSI 27;5;97~`.
#[test]
fn modify_other_keys_apagado_mientras_estaba_soltada_se_apaga_al_volver_a_adjuntar() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime del escenario");
    runtime.block_on(async {
        // El programa espera a que exista esta marca para apagar el modo: así
        // lo apaga cuando la ventana ya está soltada, sin depender de teclas.
        let marca = std::env::temp_dir().join(format!(
            "magi-mok-apagado-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let _ = std::fs::remove_file(&marca);
        let orden = format!(
            "printf '\\033[>4;2mUNO\\n'; while [ ! -e '{}' ]; do sleep 0.05; done; \
             printf '\\033[>4mDOS\\n'; exec sleep 30",
            marca.display()
        );
        let mut escenario = comun::escenario(OpcionesEscenario {
            hosts: 1,
            programa_shell: Some(vec!["sh".to_string(), "-c".to_string(), orden]),
            ..OpcionesEscenario::default()
        })
        .await;
        let host_id = escenario.hosts[0];
        let sesion_id = escenario.abrir_sesion(host_id).await;
        esperar_programa(&escenario, "UNO").await;
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
        assert_eq!(
            modo_del_cliente(&escenario.vistos, sesion_id).modify_other_keys,
            2,
            "la ventana debería tener el nivel 2 tras el primer volcado"
        );

        // La ventana se suelta; el remoto apaga el modo sin que nadie lo vea.
        escenario
            .enviar(&MensajeCliente::Desadjuntar { sesion_id })
            .await;
        escenario.escuchar(Duration::from_millis(300)).await;
        std::fs::write(&marca, b"").expect("marca del programa de prueba");
        esperar_programa(&escenario, "DOS").await;
        let _ = std::fs::remove_file(&marca);
        escenario.escuchar(Duration::from_millis(500)).await;

        // Vuelve a adjuntarse: el volcado desactiva el nivel que quedó viejo.
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
        let modo = modo_del_cliente(&escenario.vistos, sesion_id);
        assert_eq!(
            modo.modify_other_keys, 0,
            "la ventana conserva un modifyOtherKeys que el remoto ya apagó"
        );
        // Y con el modo apagado, Mayús+Intro vuelve a ser un CR.
        assert_eq!(
            codificar_tecla(
                Tecla::Intro,
                Modificadores {
                    mayusculas: true,
                    alt: false,
                    ctrl: false,
                },
                modo,
            )
            .unwrap(),
            b"\r"
        );

        escenario
            .enviar(&MensajeCliente::Cerrar { sesion_id })
            .await;
    });
}
