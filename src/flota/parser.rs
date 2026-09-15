use std::collections::BTreeMap;

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
        }
    }
    metricas
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

    #[test]
    fn las_lineas_desconocidas_se_ignoran() {
        let metricas =
            parsear("Welcome to Ubuntu\nMAGI_UPTIME=10.5\nMAGI_LOADAVG=1.0 2.0 3.0 1/1 1\n");
        assert_eq!(metricas.uptime_seg, Some(10));
        assert!(metricas.tiene_metricas());
    }
}
