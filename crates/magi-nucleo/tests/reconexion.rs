//! Pruebas de caja de la reconexión automática (Fase 15): esperas, causas
//! recuperables y no recuperables, clasificación de motivos y detección de la
//! vuelta de una suspensión.

use std::time::{Duration, Instant};

use magi_nucleo::conexion::reconexion::{
    clasificar, espera, vuelve_de_suspension, Causa, ESPERAS, SUSPENSION_SEG,
};

/// Todas las causas, para recorrerlas enteras.
const TODAS: [Causa; 13] = [
    Causa::ConexionPerdida,
    Causa::PlazoAgotado,
    Causa::KeepaliveSinRespuesta,
    Causa::ConexionRechazada,
    Causa::SinRuta,
    Causa::DnsSinRespuesta,
    Causa::Exit,
    Causa::Credenciales,
    Causa::HuellaNueva,
    Causa::HuellaCambiada,
    Causa::FrasePendiente,
    Causa::CerradaPorElUsuario,
    Causa::HostBorrado,
];

#[test]
fn las_esperas_son_2_5_10_30_y_luego_60() {
    assert_eq!(ESPERAS, [2, 5, 10, 30]);
    assert_eq!(espera(1), 2);
    assert_eq!(espera(2), 5);
    assert_eq!(espera(3), 10);
    assert_eq!(espera(4), 30);
    assert_eq!(espera(5), 60);
    assert_eq!(espera(6), 60);
    assert_eq!(espera(100), 60);
    assert_eq!(espera(u32::MAX), 60);
}

#[test]
fn solo_las_caidas_de_red_son_recuperables() {
    let recuperables = [
        Causa::ConexionPerdida,
        Causa::PlazoAgotado,
        Causa::KeepaliveSinRespuesta,
        Causa::ConexionRechazada,
        Causa::SinRuta,
        Causa::DnsSinRespuesta,
    ];
    for causa in TODAS {
        assert_eq!(
            causa.recuperable(),
            recuperables.contains(&causa),
            "causa: {causa:?}"
        );
    }
}

#[test]
fn cada_causa_tiene_su_texto_y_el_texto_vuelve_a_su_causa() {
    for causa in TODAS {
        let texto = causa.texto();
        assert!(!texto.is_empty(), "causa: {causa:?}");
        assert_eq!(clasificar(texto), causa, "texto: {texto}");
    }
    // Los textos son distintos entre sí.
    let mut textos: Vec<&str> = TODAS.iter().map(|causa| causa.texto()).collect();
    textos.sort_unstable();
    textos.dedup();
    assert_eq!(textos.len(), TODAS.len());
}

#[test]
fn clasifica_los_motivos_que_da_la_conexion() {
    // Recuperables.
    assert_eq!(clasificar("conexión perdida"), Causa::ConexionPerdida);
    assert_eq!(clasificar("timeout de red"), Causa::PlazoAgotado);
    assert_eq!(
        clasificar("el host dejó de responder al keepalive"),
        Causa::KeepaliveSinRespuesta
    );
    assert_eq!(clasificar("conexión rechazada"), Causa::ConexionRechazada);
    assert_eq!(clasificar("sin ruta al host"), Causa::SinRuta);
    assert_eq!(clasificar("el DNS no responde"), Causa::DnsSinRespuesta);

    // No recuperables.
    assert_eq!(clasificar("exit-status 0"), Causa::Exit);
    assert_eq!(clasificar("el remoto terminó la sesión"), Causa::Exit);
    assert_eq!(clasificar("credenciales rechazadas"), Causa::Credenciales);
    assert_eq!(clasificar("huella nueva sin aceptar"), Causa::HuellaNueva);
    assert_eq!(
        clasificar("la huella del host cambió"),
        Causa::HuellaCambiada
    );
    assert_eq!(clasificar("la clave necesita frase"), Causa::FrasePendiente);
    assert_eq!(
        clasificar("cerrada desde la ventana"),
        Causa::CerradaPorElUsuario
    );
    assert_eq!(clasificar("el host ya no existe"), Causa::HostBorrado);
}

#[test]
fn todo_lo_desconocido_es_una_conexion_perdida() {
    assert_eq!(clasificar(""), Causa::ConexionPerdida);
    assert_eq!(clasificar("algo raro del canal"), Causa::ConexionPerdida);
    // Y una conexión perdida es recuperable: se reintenta.
    assert!(clasificar("algo raro del canal").recuperable());
}

#[test]
fn una_suspension_de_mas_de_30_s_segundos_adelanta_el_intento() {
    assert_eq!(SUSPENSION_SEG, 30);
    let inicio = Instant::now();

    // 40 s de reloj de pared y 1 s de monotónico: 39 s suspendido.
    assert!(vuelve_de_suspension(
        (1_000, inicio),
        (1_040, inicio + Duration::from_secs(1))
    ));
    // Justo 30 s suspendido no adelanta (se exige más de 30).
    assert!(!vuelve_de_suspension(
        (1_000, inicio),
        (1_032, inicio + Duration::from_secs(2))
    ));
    // 31 s suspendido sí.
    assert!(vuelve_de_suspension(
        (1_000, inicio),
        (1_032, inicio + Duration::from_secs(1))
    ));
    // Sin suspensión: los dos relojes avanzan a la par.
    assert!(!vuelve_de_suspension(
        (1_000, inicio),
        (1_600, inicio + Duration::from_secs(600))
    ));
    // Si el reloj de pared va por detrás del monotónico, no es una suspensión.
    assert!(!vuelve_de_suspension(
        (1_000, inicio),
        (900, inicio + Duration::from_secs(5))
    ));
}
