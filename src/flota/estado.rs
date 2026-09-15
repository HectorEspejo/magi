use serde::{Deserialize, Serialize};

use crate::modelo::{ResultadoSondeo, Sondeo};

/// Umbrales globales de la flota, en `config.toml` `[flota.umbrales]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Umbrales {
    pub carga_por_nucleo: f64,
    pub memoria_pct: f64,
    pub disco_pct: f64,
}

impl Default for Umbrales {
    fn default() -> Self {
        Self {
            carga_por_nucleo: 1.0,
            memoria_pct: 90.0,
            disco_pct: 90.0,
        }
    }
}

/// Estado derivado del último sondeo de un host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoFlota {
    Fria,
    Caida,
    Alcanzable,
    Carga,
    Nominal,
}

impl EstadoFlota {
    pub fn palabra(self) -> &'static str {
        match self {
            EstadoFlota::Fria => "FRÍA",
            EstadoFlota::Caida => "CAÍDA",
            EstadoFlota::Alcanzable => "ALCANZ.",
            EstadoFlota::Carga => "CARGA",
            EstadoFlota::Nominal => "NOMINAL",
        }
    }
}

/// Evalúa un sondeo contra los umbrales y devuelve el estado con la lista de
/// culpables (carga, memoria, disco o unidades que no están `active`).
pub fn evaluar(sondeo: Option<&Sondeo>, umbrales: &Umbrales) -> (EstadoFlota, Vec<String>) {
    let Some(sondeo) = sondeo else {
        return (EstadoFlota::Fria, Vec::new());
    };
    match sondeo.resultado {
        ResultadoSondeo::Error => return (EstadoFlota::Caida, Vec::new()),
        ResultadoSondeo::SinMetricas => return (EstadoFlota::Alcanzable, Vec::new()),
        ResultadoSondeo::Ok => {}
    }
    let mut culpables = Vec::new();
    let nucleos = sondeo.nucleos.unwrap_or(1).max(1) as f64;
    if let Some(carga) = sondeo.carga_1m {
        if carga > umbrales.carga_por_nucleo * nucleos {
            culpables.push("carga".to_string());
        }
    }
    if let Some(porcentaje) = sondeo.memoria_pct() {
        if porcentaje > umbrales.memoria_pct {
            culpables.push("memoria".to_string());
        }
    }
    if let Some(porcentaje) = sondeo.disco_pct() {
        if porcentaje > umbrales.disco_pct {
            culpables.push("disco".to_string());
        }
    }
    for (unidad, estado) in &sondeo.servicios {
        if estado != "active" {
            culpables.push(unidad.clone());
        }
    }
    if culpables.is_empty() {
        (EstadoFlota::Nominal, culpables)
    } else {
        (EstadoFlota::Carga, culpables)
    }
}

/// Tasa de red por diferencia con el sondeo anterior, solo si es de hace menos
/// de una hora y los contadores no retrocedieron.
pub fn tasa_red(actual: &Sondeo, anterior: &Sondeo) -> Option<(f64, f64)> {
    let segundos = segundos_entre(&anterior.fecha, &actual.fecha)?;
    if segundos <= 0.0 || segundos > 3600.0 {
        return None;
    }
    let rx = actual.red_rx_bytes? - anterior.red_rx_bytes?;
    let tx = actual.red_tx_bytes? - anterior.red_tx_bytes?;
    if rx < 0 || tx < 0 {
        return None;
    }
    Some((rx as f64 / segundos, tx as f64 / segundos))
}

fn segundos_entre(anterior: &str, actual: &str) -> Option<f64> {
    let anterior = chrono::DateTime::parse_from_rfc3339(anterior).ok()?;
    let actual = chrono::DateTime::parse_from_rfc3339(actual).ok()?;
    Some((actual - anterior).num_milliseconds() as f64 / 1000.0)
}

/// Segundos transcurridos desde una fecha ISO de MAGI.
pub fn antiguedad_segundos(fecha: &str) -> Option<f64> {
    let momento = chrono::DateTime::parse_from_rfc3339(fecha).ok()?;
    let ahora = chrono::Local::now();
    Some((ahora - momento.with_timezone(&chrono::Local)).num_milliseconds() as f64 / 1000.0)
}

/// Antigüedad legible: «12 s», «3 min», «2 h», «4 d».
pub fn formatear_antiguedad(segundos: f64) -> String {
    let segundos = segundos.max(0.0).round() as i64;
    if segundos < 60 {
        format!("{segundos} s")
    } else if segundos < 3600 {
        format!("{} min", segundos / 60)
    } else if segundos < 86_400 {
        format!("{} h", segundos / 3600)
    } else {
        format!("{} d", segundos / 86_400)
    }
}

/// Uptime en formato «14 d 02 h» (o «3 h 05 m»).
pub fn formatear_uptime(segundos: i64) -> String {
    let dias = segundos / 86_400;
    let horas = (segundos % 86_400) / 3600;
    let minutos = (segundos % 3600) / 60;
    if dias > 0 {
        format!("{dias} d {horas:02} h")
    } else {
        format!("{horas} h {minutos:02} m")
    }
}

/// Formatea una tasa de bytes por segundo de forma legible.
pub fn formatear_tasa(bytes_por_segundo: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes_por_segundo >= MB {
        format!("{:.1} MB/s", bytes_por_segundo / MB)
    } else if bytes_por_segundo >= KB {
        format!("{:.0} kB/s", bytes_por_segundo / KB)
    } else {
        format!("{:.0} B/s", bytes_por_segundo)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::modelo::Sondeo;

    fn sondeo_ok() -> Sondeo {
        let mut sondeo = Sondeo::vacio(1);
        sondeo.nucleos = Some(4);
        sondeo.carga_1m = Some(1.0);
        sondeo.carga_5m = Some(1.0);
        sondeo.carga_15m = Some(1.0);
        sondeo.mem_total_kb = Some(1000);
        sondeo.mem_disponible_kb = Some(500);
        sondeo.disco_total_kb = Some(1000);
        sondeo.disco_usado_kb = Some(100);
        sondeo.uptime_seg = Some(10);
        sondeo
    }

    #[test]
    fn sin_sondeo_esta_fria() {
        let (estado, culpables) = evaluar(None, &Umbrales::default());
        assert_eq!(estado, EstadoFlota::Fria);
        assert!(culpables.is_empty());
    }

    #[test]
    fn error_es_caida_y_sin_metricas_es_alcanzable() {
        let mut sondeo = sondeo_ok();
        sondeo.resultado = ResultadoSondeo::Error;
        assert_eq!(
            evaluar(Some(&sondeo), &Umbrales::default()).0,
            EstadoFlota::Caida
        );
        sondeo.resultado = ResultadoSondeo::SinMetricas;
        assert_eq!(
            evaluar(Some(&sondeo), &Umbrales::default()).0,
            EstadoFlota::Alcanzable
        );
    }

    #[test]
    fn los_umbrales_y_los_servicios_derivan_carga() {
        let umbrales = Umbrales::default();
        assert_eq!(
            evaluar(Some(&sondeo_ok()), &umbrales).0,
            EstadoFlota::Nominal
        );

        let mut con_carga = sondeo_ok();
        con_carga.carga_1m = Some(4.1);
        let (estado, culpables) = evaluar(Some(&con_carga), &umbrales);
        assert_eq!(estado, EstadoFlota::Carga);
        assert_eq!(culpables, vec!["carga"]);

        let mut con_memoria = sondeo_ok();
        con_memoria.mem_disponible_kb = Some(50);
        assert_eq!(evaluar(Some(&con_memoria), &umbrales).0, EstadoFlota::Carga);

        let mut con_servicio = sondeo_ok();
        con_servicio
            .servicios
            .insert("penwork".to_string(), "failed".to_string());
        let (estado, culpables) = evaluar(Some(&con_servicio), &umbrales);
        assert_eq!(estado, EstadoFlota::Carga);
        assert_eq!(culpables, vec!["penwork"]);
    }

    #[test]
    fn la_tasa_solo_sale_con_contadores_validos_y_menos_de_una_hora() {
        let mut anterior = sondeo_ok();
        anterior.fecha = "2026-09-15T10:00:00+02:00".to_string();
        anterior.red_rx_bytes = Some(1000);
        anterior.red_tx_bytes = Some(2000);
        let mut actual = sondeo_ok();
        actual.fecha = "2026-09-15T10:00:10+02:00".to_string();
        actual.red_rx_bytes = Some(3000);
        actual.red_tx_bytes = Some(4000);
        let (rx, tx) = tasa_red(&actual, &anterior).unwrap();
        assert!((rx - 200.0).abs() < 0.01);
        assert!((tx - 200.0).abs() < 0.01);

        actual.fecha = "2026-09-15T12:00:00+02:00".to_string();
        assert!(tasa_red(&actual, &anterior).is_none());

        actual.fecha = "2026-09-15T10:00:10+02:00".to_string();
        actual.red_rx_bytes = Some(500);
        assert!(tasa_red(&actual, &anterior).is_none());
    }

    #[test]
    fn las_tasas_se_formatean_legibles() {
        assert_eq!(formatear_tasa(512.0), "512 B/s");
        assert_eq!(formatear_tasa(2048.0), "2 kB/s");
        assert_eq!(formatear_tasa(2.5 * 1024.0 * 1024.0), "2.5 MB/s");
    }
}
