//! Tamaño del PTY remoto: el servidor de sesiones aplica el mínimo de las
//! ventanas adjuntas y el host recibe un `window-change` en cada cambio,
//! también cuando crece (fallo 2 de R36).

use std::time::Duration;

use magi::protocolo::{MensajeCliente, MensajeServidor};

use crate::comun::{escenario, Escenario, OpcionesEscenario};

/// Espera a que el host haya recibido al menos `n` `window-change`.
async fn esperar_tamanos(esc: &mut Escenario, n: usize) -> Vec<(u32, u32)> {
    let observado = esc.observado.clone();
    let llegaron = esc
        .hasta(Duration::from_secs(10), |_| observado.tamanos().len() >= n)
        .await;
    let tamanos = observado.tamanos();
    assert!(
        llegaron,
        "el host esperaba {n} window-change y recibió {tamanos:?}"
    );
    tamanos
}

/// La `Redimensionada` con ese tamaño, ya vista o por llegar (`hasta` va
/// guardando en `vistos` lo que lee mientras espera).
async fn redimensionada(esc: &mut Escenario, cols: u16, filas: u16) -> MensajeServidor {
    let es = |mensaje: &MensajeServidor| {
        matches!(
            mensaje,
            MensajeServidor::Redimensionada { cols: c, filas: f, .. } if *c == cols && *f == filas
        )
    };
    if let Some(mensaje) = esc.vistos.iter().rev().find(|mensaje| es(mensaje)) {
        return mensaje.clone();
    }
    esc.esperar(es).await
}

async fn adjuntar(esc: &mut Escenario, sesion_id: u32, cols: u16, filas: u16) {
    esc.enviar(&MensajeCliente::Adjuntar {
        sesion_id,
        cols,
        filas,
    })
    .await;
}

async fn redimensionar(esc: &mut Escenario, sesion_id: u32, cols: u16, filas: u16) {
    esc.enviar(&MensajeCliente::Redimensionar {
        sesion_id,
        cols,
        filas,
    })
    .await;
}

/// Reproduce el fallo 2 con una sola ventana: encoger llegaba al remoto, pero
/// crecer no (el mínimo partía del tamaño ya aplicado y nunca subía).
#[tokio::test]
async fn fallo2_una_ventana_crece_y_encoge() {
    let mut esc = escenario(OpcionesEscenario::default()).await;
    let host = esc.hosts[0];
    let sesion = esc.abrir_sesion(host).await;
    // La pestaña se da por abierta al pedir la shell: el host puede no haber
    // procesado aún el pty-req.
    let observado = esc.observado.clone();
    assert!(
        esc.hasta(Duration::from_secs(10), |_| !observado.ptys().is_empty())
            .await
    );
    assert_eq!(esc.observado.ptys(), vec![(80, 24)]);

    adjuntar(&mut esc, sesion, 100, 26).await;
    assert_eq!(esperar_tamanos(&mut esc, 1).await, vec![(100, 26)]);

    redimensionar(&mut esc, sesion, 80, 20).await;
    assert_eq!(esperar_tamanos(&mut esc, 2).await[1], (80, 20));

    redimensionar(&mut esc, sesion, 120, 36).await;
    assert_eq!(esperar_tamanos(&mut esc, 3).await[2], (120, 36));

    // El cliente ve el tamaño nuevo en la difusión y en la pantalla del host.
    let aviso = redimensionada(&mut esc, 120, 36).await;
    assert!(matches!(
        aviso,
        MensajeServidor::Redimensionada { ventana_minima: Some(id), .. } if id == esc.cliente_id
    ));
    assert!(
        esc.observado.tamanos().len() == 3,
        "sin window-change de más"
    );
}

/// El mismo tamaño que ya tiene la sesión no produce un `window-change`.
#[tokio::test]
async fn un_tamano_igual_no_llega_al_remoto() {
    let mut esc = escenario(OpcionesEscenario::default()).await;
    let host = esc.hosts[0];
    let sesion = esc.abrir_sesion(host).await;
    adjuntar(&mut esc, sesion, 100, 26).await;
    esperar_tamanos(&mut esc, 1).await;
    redimensionar(&mut esc, sesion, 100, 26).await;
    esc.escuchar(Duration::from_millis(300)).await;
    assert_eq!(esc.observado.tamanos(), vec![(100, 26)]);
}

/// AC de la pestaña compartida: ventanas de 200×50 y 120×40; la segunda pasa
/// a 100×30 y luego se cierra. El remoto recibe primero las 100 columnas y
/// después las 200 (filas ya descontadas con `alto_pty`).
#[tokio::test]
async fn ac_dos_ventanas_la_pequena_se_cierra() {
    let mut esc = escenario(OpcionesEscenario::default()).await;
    let host = esc.hosts[0];
    let sesion = esc.abrir_sesion(host).await;
    let mut segunda = esc.otra_ventana().await;

    adjuntar(&mut esc, sesion, 200, 46).await;
    esperar_tamanos(&mut esc, 1).await;
    segunda
        .enviar(&MensajeCliente::Adjuntar {
            sesion_id: sesion,
            cols: 120,
            filas: 36,
        })
        .await;
    assert_eq!(esperar_tamanos(&mut esc, 2).await[1], (120, 36));

    segunda
        .enviar(&MensajeCliente::Redimensionar {
            sesion_id: sesion,
            cols: 100,
            filas: 26,
        })
        .await;
    assert_eq!(esperar_tamanos(&mut esc, 3).await[2], (100, 26));
    // La ventana grande sabe qué ventana impone el mínimo.
    let id_segunda = segunda.cliente_id;
    let aviso = redimensionada(&mut esc, 100, 26).await;
    assert!(matches!(
        aviso,
        MensajeServidor::Redimensionada { ventana_minima: Some(id), .. } if id == id_segunda
    ));

    // Se cierra la segunda ventana: el remoto vuelve a las 200 columnas.
    drop(segunda);
    let tamanos = esperar_tamanos(&mut esc, 4).await;
    assert_eq!(&tamanos[2..], &[(100, 26), (200, 46)]);
    let aviso = redimensionada(&mut esc, 200, 46).await;
    assert!(matches!(
        aviso,
        MensajeServidor::Redimensionada { ventana_minima: Some(id), .. } if id == esc.cliente_id
    ));
}

/// Igual, pero la ventana pequeña sale de la vista Sesión (`Desadjuntar`) en
/// vez de cerrarse.
#[tokio::test]
async fn ac_dos_ventanas_la_pequena_se_desadjunta() {
    let mut esc = escenario(OpcionesEscenario::default()).await;
    let host = esc.hosts[0];
    let sesion = esc.abrir_sesion(host).await;
    let mut segunda = esc.otra_ventana().await;

    adjuntar(&mut esc, sesion, 200, 46).await;
    esperar_tamanos(&mut esc, 1).await;
    segunda
        .enviar(&MensajeCliente::Adjuntar {
            sesion_id: sesion,
            cols: 100,
            filas: 26,
        })
        .await;
    assert_eq!(esperar_tamanos(&mut esc, 2).await[1], (100, 26));
    segunda
        .enviar(&MensajeCliente::Desadjuntar { sesion_id: sesion })
        .await;
    assert_eq!(esperar_tamanos(&mut esc, 3).await[2], (200, 46));
}

/// Una ventana grande que se adjunta a una pestaña ya limitada por otra más
/// pequeña recibe `Redimensionada` aunque el tamaño no cambie: si no, no sabe
/// que debe rellenar con `░` ni quién impone el mínimo.
#[tokio::test]
async fn la_ventana_que_se_adjunta_sabe_el_tamano_vigente() {
    let mut esc = escenario(OpcionesEscenario::default()).await;
    let host = esc.hosts[0];
    let sesion = esc.abrir_sesion(host).await;
    let mut segunda = esc.otra_ventana().await;
    segunda
        .enviar(&MensajeCliente::Adjuntar {
            sesion_id: sesion,
            cols: 100,
            filas: 26,
        })
        .await;
    esperar_tamanos(&mut esc, 1).await;

    adjuntar(&mut esc, sesion, 200, 46).await;
    let id_segunda = segunda.cliente_id;
    let aviso = esc
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::Redimensionada { .. }))
        .await;
    assert!(
        matches!(
            aviso,
            MensajeServidor::Redimensionada { cols: 100, filas: 26, ventana_minima: Some(id), .. }
                if id == id_segunda
        ),
        "llegó {aviso:?}"
    );
    assert_eq!(esc.observado.tamanos(), vec![(100, 26)]);
}
