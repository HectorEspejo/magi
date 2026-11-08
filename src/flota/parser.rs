use std::collections::BTreeMap;

use magi_nucleo::servicios::{FallosContenedores, FallosSondeo, FallosUnidades, MotorNombre};

/// Nombres de fallos que se guardan por el sondeo (§3.2: hasta 20).
const MAX_NOMBRES_FALLO: usize = 20;

/// Métricas extraídas de la salida de `sondeo.sh`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metricas {
    pub nucleos: Option<i64>,
    pub carga_1m: Option<f64>,
    pub carga_5m: Option<f64>,
    pub carga_15m: Option<f64>,
    pub mem_total_kb: Option<i64>,
    pub mem_disponible_kb: Option<i64>,
    pub disco_total_kb: Option<i64>,
    pub disco_usado_kb: Option<i64>,
    pub red_rx_bytes: Option<i64>,
    pub red_tx_bytes: Option<i64>,
    pub uptime_seg: Option<i64>,
    pub servicios: BTreeMap<String, String>,
    /// Fallos del sondeo ampliado de la Fase 13 (unidades fallidas y
    /// contenedores caídos). `None` si el script no trajo el bloque de fallos;
    /// `Some` puede tener total 0.
    pub fallos: Option<FallosSondeo>,
}

impl Metricas {
    /// Un host sin `/proc/loadavg` ni `/proc/uptime` no es un Linux con
    /// métricas: el sondeo queda como `sin_metricas`.
    pub fn tiene_metricas(&self) -> bool {
        self.carga_1m.is_some() && self.uptime_seg.is_some()
    }
}

/// Parsea la salida del script. Las líneas desconocidas se ignoran.
pub fn parsear(salida: &str) -> Metricas {
    let mut metricas = Metricas::default();
    // Acumuladores del bloque de fallos: los totales llegan al final y los
    // nombres (hasta 20) antes.
    let mut fallos_unidades = FallosUnidades::default();
    let mut total_unidades: Option<u32> = None;
    let mut fallos_contenedores = FallosContenedores::default();
    let mut total_contenedores: Option<u32> = None;
    let mut motor: Option<MotorNombre> = None;
    let mut motor_sin_permiso = false;
    for linea in salida.lines() {
        let linea = linea.trim_end();
        if let Some(valor) = linea.strip_prefix("MAGI_NUCLEOS=") {
            metricas.nucleos = valor.trim().parse::<i64>().ok();
        } else if let Some(valor) = linea.strip_prefix("MAGI_LOADAVG=") {
            let campos: Vec<&str> = valor.split_whitespace().collect();
            if campos.len() >= 3 {
                metricas.carga_1m = campos[0].parse::<f64>().ok();
                metricas.carga_5m = campos[1].parse::<f64>().ok();
                metricas.carga_15m = campos[2].parse::<f64>().ok();
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_MEM_MemTotal:") {
            metricas.mem_total_kb = primer_entero(valor);
        } else if let Some(valor) = linea.strip_prefix("MAGI_MEM_MemAvailable:") {
            metricas.mem_disponible_kb = primer_entero(valor);
        } else if let Some(valor) = linea.strip_prefix("MAGI_DISCO=") {
            let campos: Vec<&str> = valor.split_whitespace().collect();
            if campos.len() >= 2 {
                metricas.disco_total_kb = campos[0].parse::<i64>().ok();
                metricas.disco_usado_kb = campos[1].parse::<i64>().ok();
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_RED=") {
            let campos: Vec<&str> = valor.split_whitespace().collect();
            if campos.len() >= 2 {
                metricas.red_rx_bytes = campos[0].parse::<i64>().ok();
                metricas.red_tx_bytes = campos[1].parse::<i64>().ok();
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_UPTIME=") {
            metricas.uptime_seg = valor
                .trim()
                .split('.')
                .next()
                .and_then(|entero| entero.parse::<i64>().ok());
        } else if let Some(valor) = linea.strip_prefix("MAGI_SVC=") {
            if let Some((unidad, estado)) = valor.split_once('=') {
                let estado = if estado.trim().is_empty() {
                    "unknown".to_string()
                } else {
                    estado.trim().to_string()
                };
                metricas.servicios.insert(unidad.trim().to_string(), estado);
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_FALLO_UNIDAD=") {
            if fallos_unidades.nombres.len() < MAX_NOMBRES_FALLO {
                if let Some(nombre) = nombre_de_fallo(valor) {
                    fallos_unidades.nombres.push(nombre);
                }
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_FALLOS_UNIDADES=") {
            total_unidades = valor.trim().parse::<u32>().ok();
        } else if let Some(valor) = linea.strip_prefix("MAGI_MOTOR=") {
            let mut campos = valor.split_whitespace();
            let nombre = campos.next().unwrap_or_default();
            let estado = campos.next().unwrap_or_default();
            if let Some(motor_nombre) = MotorNombre::desde_texto(nombre) {
                motor = Some(motor_nombre);
                motor_sin_permiso = estado == "sin_permiso";
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_FALLO_CONTENEDOR=") {
            if fallos_contenedores.nombres.len() < MAX_NOMBRES_FALLO {
                if let Some(nombre) = nombre_de_fallo(valor) {
                    fallos_contenedores.nombres.push(nombre);
                }
            }
        } else if let Some(valor) = linea.strip_prefix("MAGI_FALLOS_CONTENEDORES=") {
            total_contenedores = valor.trim().parse::<u32>().ok();
        }
    }
    // El resumen de unidades es la señal de que el script trajo el bloque de
    // fallos: un host sin systemd también lo trae, con total 0. Los
    // contenedores solo tienen datos si hubo un motor que respondió a ps -a
    // (sin motor o sin permiso, "contenedores": null).
    if let Some(total) = total_unidades {
        fallos_unidades.total = total.max(fallos_unidades.nombres.len() as u32);
        let contenedores = match (motor, motor_sin_permiso, total_contenedores) {
            (Some(nombre), false, Some(total)) => {
                fallos_contenedores.motor = Some(nombre);
                fallos_contenedores.total = total.max(fallos_contenedores.nombres.len() as u32);
                Some(fallos_contenedores)
            }
            _ => None,
        };
        metricas.fallos = Some(FallosSondeo {
            unidades: fallos_unidades,
            contenedores,
        });
    }
    metricas
}

/// Nombre de una unidad o un contenedor fallido, recortado a los límites de la
/// entrada no fiable (§8) y sin caracteres de control ni de dirección (los
/// nombres llegan del host y se pintan en Flota y «Toda la flota»). `None` si
/// la línea no trae nombre.
fn nombre_de_fallo(valor: &str) -> Option<String> {
    let nombre: String = valor
        .trim()
        .chars()
        .filter(|caracter| !es_caracter_invisible(*caracter))
        .take(magi_nucleo::servicios::limites::MAX_NOMBRE)
        .collect();
    if nombre.is_empty() {
        None
    } else {
        Some(nombre)
    }
}

/// Caracteres de control (C0, C1 y DEL) y de control bidireccional que no
/// deben llegar a la base de datos ni a la pantalla.
fn es_caracter_invisible(caracter: char) -> bool {
    caracter.is_control()
        || matches!(
            caracter,
            '\u{200e}' | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200b}'..='\u{200d}'
        )
}

fn primer_entero(texto: &str) -> Option<i64> {
    texto.split_whitespace().next()?.parse::<i64>().ok()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const SALIDA_LINUX: &str = "MAGI_NUCLEOS=8\n\
        MAGI_LOADAVG=0.52 0.58 0.59 1/234 5678\n\
        MAGI_MEM_MemTotal:       16303408 kB\n\
        MAGI_MEM_MemAvailable:    8123456 kB\n\
        MAGI_DISCO=103080888 30000000\n\
        MAGI_RED=123456789 987654321\n\
        MAGI_UPTIME=123456.78\n\
        MAGI_SVC=nginx=active\n\
        MAGI_SVC=penwork=failed\n\
        MAGI_SVC=router=unknown\n";

    #[test]
    fn parsea_una_salida_real_de_linux() {
        let metricas = parsear(SALIDA_LINUX);
        assert_eq!(metricas.nucleos, Some(8));
        assert_eq!(metricas.carga_1m, Some(0.52));
        assert_eq!(metricas.carga_5m, Some(0.58));
        assert_eq!(metricas.carga_15m, Some(0.59));
        assert_eq!(metricas.mem_total_kb, Some(16_303_408));
        assert_eq!(metricas.mem_disponible_kb, Some(8_123_456));
        assert_eq!(metricas.disco_total_kb, Some(103_080_888));
        assert_eq!(metricas.disco_usado_kb, Some(30_000_000));
        assert_eq!(metricas.red_rx_bytes, Some(123_456_789));
        assert_eq!(metricas.red_tx_bytes, Some(987_654_321));
        assert_eq!(metricas.uptime_seg, Some(123_456));
        assert_eq!(
            metricas.servicios.get("nginx").map(String::as_str),
            Some("active")
        );
        assert_eq!(
            metricas.servicios.get("penwork").map(String::as_str),
            Some("failed")
        );
        assert_eq!(
            metricas.servicios.get("router").map(String::as_str),
            Some("unknown")
        );
        assert!(metricas.tiene_metricas());
    }

    #[test]
    fn sin_proc_no_hay_metricas() {
        let salida = "MAGI_NUCLEOS=1\nMAGI_SVC=nginx=unknown\n";
        let metricas = parsear(salida);
        assert!(!metricas.tiene_metricas());
        assert_eq!(metricas.nucleos, Some(1));
        assert_eq!(
            metricas.servicios.get("nginx").map(String::as_str),
            Some("unknown")
        );
    }

    #[test]
    fn un_estado_vacio_es_unknown() {
        let metricas = parsear("MAGI_SVC=cooperapp=\n");
        assert_eq!(
            metricas.servicios.get("cooperapp").map(String::as_str),
            Some("unknown")
        );
    }

    const SALIDA_CON_FALLOS: &str = "MAGI_NUCLEOS=4\n\
        MAGI_LOADAVG=0.20 0.30 0.40 1/100 2000\n\
        MAGI_UPTIME=100.0\n\
        MAGI_FALLO_UNIDAD=copia.service\n\
        MAGI_FALLOS_UNIDADES=2\n\
        MAGI_MOTOR=docker\n\
        MAGI_FALLO_CONTENEDOR=web-worker-1\n\
        MAGI_FALLO_CONTENEDOR=panel\n\
        MAGI_FALLOS_CONTENEDORES=3\n";

    #[test]
    fn parsea_los_fallos_del_sondeo_ampliado() {
        let metricas = parsear(SALIDA_CON_FALLOS);
        let fallos = metricas.fallos.expect("el sondeo trae fallos");
        assert_eq!(fallos.unidades.total, 2);
        assert_eq!(
            fallos.unidades.nombres.as_slice(),
            ["copia.service".to_string()],
            "total es el recuento real y nombres los que cupieron"
        );
        assert_eq!(fallos.total(), 5);
        let contenedores = fallos.contenedores.as_ref().expect("hay motor y permiso");
        assert_eq!(contenedores.motor, Some(MotorNombre::Docker));
        assert_eq!(contenedores.total, 3);
        assert_eq!(
            contenedores.nombres.as_slice(),
            ["web-worker-1".to_string(), "panel".to_string()]
        );
    }

    #[test]
    fn los_nombres_de_fallo_no_llevan_caracteres_de_control() {
        let salida = "MAGI_FALLOS_UNIDADES=1\n\
            MAGI_FALLO_UNIDAD=\u{1b}[31mrojo\u{7f}.service\n\
            MAGI_MOTOR=docker\n\
            MAGI_FALLO_CONTENEDOR=panel\u{200e}\n\
            MAGI_FALLOS_CONTENEDORES=1\n";
        let fallos = parsear(salida).fallos.expect("trae el bloque de fallos");
        assert_eq!(
            fallos.unidades.nombres,
            vec!["[31mrojo.service".to_string()]
        );
        let contenedores = fallos.contenedores.expect("motor y permiso");
        assert_eq!(contenedores.nombres, vec!["panel".to_string()]);
    }

    #[test]
    fn sin_motor_o_sin_permiso_los_contenedores_van_a_null() {
        let salida = "MAGI_FALLOS_UNIDADES=0\nMAGI_MOTOR=docker sin_permiso\n";
        let fallos = parsear(salida).fallos.expect("trae el bloque de fallos");
        assert_eq!(fallos.unidades.total, 0);
        assert!(fallos.contenedores.is_none());
    }

    #[test]
    fn sin_bloque_de_fallos_no_hay_datos() {
        let metricas = parsear("MAGI_UPTIME=10.5\nMAGI_LOADAVG=1.0 2.0 3.0 1/1 1\n");
        assert!(metricas.fallos.is_none());
    }

    #[test]
    fn los_nombres_de_fallo_se_recortan_a_veinte_y_al_limite() {
        let mut salida = String::new();
        for indice in 0..25 {
            salida.push_str(&format!("MAGI_FALLO_UNIDAD=unidad-{indice}.service\n"));
        }
        salida.push_str("MAGI_FALLOS_UNIDADES=25\n");
        let fallos = parsear(&salida).fallos.expect("trae fallos");
        assert_eq!(fallos.unidades.total, 25);
        assert_eq!(fallos.unidades.nombres.len(), 20);
        assert_eq!(fallos.unidades.nombres[0], "unidad-0.service");
        assert_eq!(fallos.unidades.nombres[19], "unidad-19.service");

        let largo = format!(
            "MAGI_FALLOS_UNIDADES=0\nMAGI_MOTOR=podman\nMAGI_FALLO_CONTENEDOR={}\nMAGI_FALLOS_CONTENEDORES=1\n",
            "a".repeat(5000)
        );
        let fallos = parsear(&largo).fallos.expect("trae fallos");
        let contenedores = fallos.contenedores.expect("hay contenedores");
        assert_eq!(
            contenedores.nombres[0].chars().count(),
            magi_nucleo::servicios::limites::MAX_NOMBRE
        );
    }

    #[test]
    fn las_lineas_desconocidas_se_ignoran() {
        let metricas =
            parsear("Welcome to Ubuntu\nMAGI_UPTIME=10.5\nMAGI_LOADAVG=1.0 2.0 3.0 1/1 1\n");
        assert_eq!(metricas.uptime_seg, Some(10));
        assert!(metricas.tiene_metricas());
    }
}
