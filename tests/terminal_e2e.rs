//! Prueba de extremo a extremo del terminal (Fase 15) sin red externa: el
//! servidor de sesiones en proceso abre una pestaña en el servidor SSH de
//! prueba, que ejecuta `tests/fijos/terminal/tui_prueba.rs` (un programa que
//! hace lo que Claude Code: consultas, pegado entre corchetes, teclado de
//! kitty, salida sincronizada, ratón, OSC 52, campana y título).
//!
//! Comprueba lo que el informe exige de la cadena completa: el programa recibe
//! las respuestas a sus consultas (una sola vez), un pegado de cinco líneas
//! llega como un solo bloque y Mayús+Intro llega como `CSI 13;2u`.

mod comun;

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use magi::protocolo::{MensajeCliente, MensajeServidor};
use magi_nucleo::terminal::pegado::{pegar, sanear};
use magi_nucleo::terminal::raton::{
    codificar as codificar_raton, Accion, Boton, Codificacion, Evento, Modo, ModoRaton,
};
use magi_nucleo::terminal::teclas::{
    codificar as codificar_tecla, Modificadores, ModoTerminal, Tecla,
};

use comun::OpcionesEscenario;

/// Compila `tui_prueba` y devuelve su ruta. Se compila una vez por prueba con
/// el `rustc` del toolchain que ejecuta los tests (el programa solo usa std).
fn compilar_tui_prueba() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("magi-tui-prueba-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("directorio del programa de prueba");
    let salida = dir.join(if cfg!(windows) {
        "tui_prueba.exe"
    } else {
        "tui_prueba"
    });
    if !salida.exists() {
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
    }
    salida
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
