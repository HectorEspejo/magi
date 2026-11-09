//! Sondeo de la flota (F2 §7.2), compartido por el escritorio y el móvil: el
//! script embebido `scripts/sondeo.sh`, su comando y la lectura y el estado de
//! lo que devuelve. Lo que cambia en cada plataforma es cómo se llega al host
//! (conexión del servidor, conexión efímera o la del gestor del móvil): eso se
//! queda fuera del núcleo.

pub mod estado;
pub mod lectura;

pub use estado::{
    antiguedad_segundos, evaluar, formatear_antiguedad, formatear_tasa, formatear_uptime, tasa_red,
    EstadoFlota, Umbrales,
};
pub use lectura::{parsear, Metricas};

use crate::modelo::{ResultadoSondeo, Sondeo};

/// Script ejecutado en el host remoto. Solo depende de `sh` y coreutils; sin
/// `systemctl` los servicios quedan como `unknown`. Desde la Fase 14 recibe
/// `<motores>` como primer argumento (R125).
pub const SCRIPT: &str = include_str!("../../scripts/sondeo.sh");

/// Sondeos simultáneos y tiempo máximo por host (DNS, TCP, autenticación y
/// comando incluidos).
pub const CONCURRENTES: usize = 8;
pub const TIMEOUT_SEG: u64 = 5;

/// Rellena un sondeo con lo leído del host y decide su resultado: `Ok` si el
/// host trae métricas y `SinMetricas` si responde pero no las da (sin
/// `/proc/loadavg` ni `/proc/uptime`). Lo comparten el escritorio y el móvil.
pub fn aplicar_metricas(sondeo: &mut Sondeo, metricas: Metricas) {
    sondeo.nucleos = metricas.nucleos;
    sondeo.carga_1m = metricas.carga_1m;
    sondeo.carga_5m = metricas.carga_5m;
    sondeo.carga_15m = metricas.carga_15m;
    sondeo.mem_total_kb = metricas.mem_total_kb;
    sondeo.mem_disponible_kb = metricas.mem_disponible_kb;
    sondeo.disco_total_kb = metricas.disco_total_kb;
    sondeo.disco_usado_kb = metricas.disco_usado_kb;
    sondeo.red_rx_bytes = metricas.red_rx_bytes;
    sondeo.red_tx_bytes = metricas.red_tx_bytes;
    sondeo.uptime_seg = metricas.uptime_seg;
    let tiene_metricas = metricas.tiene_metricas();
    sondeo.servicios = metricas.servicios;
    sondeo.fallos = metricas.fallos;
    sondeo.resultado = if tiene_metricas {
        ResultadoSondeo::Ok
    } else {
        ResultadoSondeo::SinMetricas
    };
}

/// Motores de contenedores del sondeo como texto para el script: los válidos
/// (docker y podman, sin repetir y en orden) separados por comas, o `-` si no
/// hay ninguno (R125).
pub fn motores_texto(motores: &[String]) -> String {
    let mut limpios: Vec<String> = Vec::new();
    for motor in motores {
        let motor = motor.trim();
        if !matches!(motor, "docker" | "podman") || limpios.iter().any(|otro| otro == motor) {
            continue;
        }
        limpios.push(motor.to_string());
    }
    if limpios.is_empty() {
        "-".to_string()
    } else {
        limpios.join(",")
    }
}

/// Comando `exec`: el script embebido viaja entrecomillado y los motores y las
/// unidades del inventario se pasan como argumentos validados, nunca
/// interpolados en el texto del comando.
pub fn comando_de_sondeo(servicios: &[String], motores: &[String]) -> String {
    let mut comando = format!("sh -c {} magi", entrecomillar(SCRIPT));
    comando.push(' ');
    comando.push_str(&entrecomillar(&motores_texto(motores)));
    for servicio in servicios {
        comando.push(' ');
        comando.push_str(&entrecomillar(servicio));
    }
    comando
}

fn entrecomillar(texto: &str) -> String {
    format!("'{}'", texto.replace('\'', "'\\''"))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_comando_entrecomilla_el_script_los_motores_y_los_servicios() {
        let comando = comando_de_sondeo(
            &["nginx".to_string(), "magi@.service".to_string()],
            &["docker".to_string(), "podman".to_string()],
        );
        assert!(comando.starts_with("sh -c '#!/bin/sh"));
        assert!(comando.contains(" magi 'docker,podman' 'nginx' 'magi@.service'"));
    }

    #[test]
    fn las_comillas_simples_del_script_se_escapan() {
        let comando = comando_de_sondeo(&[], &[]);
        assert!(comando.contains("'\\''"));
        assert!(comando.starts_with("sh -c '"));
        assert!(comando.contains(" magi '-'"));
    }

    #[test]
    fn los_motores_se_validan_sin_repetir() {
        assert_eq!(
            motores_texto(&["podman".to_string(), "docker".to_string()]),
            "podman,docker"
        );
        assert_eq!(
            motores_texto(&[
                "docker".to_string(),
                "docker".to_string(),
                "lxc".to_string(),
                " podman ".to_string()
            ]),
            "docker,podman"
        );
        assert_eq!(motores_texto(&[]), "-");
        assert_eq!(motores_texto(&["lxc".to_string()]), "-");
    }

    #[test]
    fn el_script_lleva_el_bloque_de_motores_y_fallos() {
        assert!(SCRIPT.contains("MAGI_FALLOS_UNIDADES="));
        assert!(SCRIPT.contains("MAGI_FALLOS_CONTENEDORES="));
        assert!(SCRIPT.contains("motores="));
    }
}
