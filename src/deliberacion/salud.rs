//! MELCHIOR-1 · salud: el host está NOMINAL según su último sondeo (de hace
//! menos de `salud_max_min`) o según uno nuevo, no interactivo. CARGA,
//! ALCANZABLE o CAÍDA rechazan con el dato que lo justifica.

use std::time::Duration;

use crate::flota::estado::{
    antiguedad_segundos, evaluar, formatear_antiguedad, EstadoFlota, Umbrales,
};
use crate::modelo::Sondeo;

/// Qué hacer con el último sondeo conocido del host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionSalud {
    Aprueba(String),
    Rechaza(String),
    /// Viejo o inexistente: hay que sondear ahora.
    Sondear,
}

/// Juicio sobre el último sondeo: reciente y NOMINAL aprueba; reciente y no
/// nominal rechaza con el detalle; viejo o ninguno, a sondear.
pub fn con_ultimo(ultimo: Option<&Sondeo>, maximo: Duration, umbrales: &Umbrales) -> DecisionSalud {
    let Some(sondeo) = ultimo else {
        return DecisionSalud::Sondear;
    };
    let Some(edad) = antiguedad_segundos(&sondeo.fecha) else {
        return DecisionSalud::Sondear;
    };
    if edad > maximo.as_secs_f64() {
        return DecisionSalud::Sondear;
    }
    let hace = formatear_antiguedad(edad);
    match juzgar(sondeo, umbrales) {
        (true, detalle) => DecisionSalud::Aprueba(format!("{detalle} · hace {hace}")),
        (false, _) if evaluar(Some(sondeo), umbrales).0 == EstadoFlota::Fria => {
            DecisionSalud::Sondear
        }
        (false, detalle) => DecisionSalud::Rechaza(format!("{detalle} · hace {hace}")),
    }
}

/// Juicio sobre un sondeo recién hecho.
pub fn con_sondeo_nuevo(sondeo: &Sondeo, umbrales: &Umbrales) -> (bool, String) {
    juzgar(sondeo, umbrales)
}

/// ¿NOMINAL? y el texto que lo dice (con los culpables de una CARGA).
fn juzgar(sondeo: &Sondeo, umbrales: &Umbrales) -> (bool, String) {
    let (estado, culpables) = evaluar(Some(sondeo), umbrales);
    match estado {
        EstadoFlota::Nominal => (true, "NOMINAL".to_string()),
        EstadoFlota::Carga => {
            let detalles: Vec<String> = culpables
                .iter()
                .map(|culpable| detalle_culpable(sondeo, culpable))
                .collect();
            (false, format!("CARGA: {}", detalles.join(", ")))
        }
        EstadoFlota::Alcanzable => (false, "ALCANZABLE: sin métricas".to_string()),
        EstadoFlota::Caida => (
            false,
            format!(
                "CAÍDA: {}",
                sondeo
                    .error
                    .as_deref()
                    .map(|error| crate::snippets::salida::sanear_linea(error, 60))
                    .unwrap_or_else(|| "sin respuesta".to_string())
            ),
        ),
        EstadoFlota::Fria => (false, "sin datos recientes".to_string()),
    }
}

fn detalle_culpable(sondeo: &Sondeo, culpable: &str) -> String {
    match culpable {
        "carga" => format!(
            "carga {:.1}/{}",
            sondeo.carga_1m.unwrap_or_default(),
            sondeo.nucleos.unwrap_or(1).max(1)
        ),
        "memoria" => format!("memoria {:.0} %", sondeo.memoria_pct().unwrap_or_default()),
        "disco" => format!("disco {:.0} %", sondeo.disco_pct().unwrap_or_default()),
        unidad => format!("{unidad} inactivo"),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::modelo::ResultadoSondeo;

    fn sondeo(hace_segundos: i64) -> Sondeo {
        let fecha = chrono::Local::now() - chrono::Duration::seconds(hace_segundos);
        Sondeo {
            host_id: 1,
            fecha: fecha.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            resultado: ResultadoSondeo::Ok,
            nucleos: Some(2),
            carga_1m: Some(0.5),
            mem_total_kb: Some(1000),
            mem_disponible_kb: Some(800),
            disco_total_kb: Some(1000),
            disco_usado_kb: Some(100),
            uptime_seg: Some(100),
            ..Sondeo::default()
        }
    }

    const CINCO_MIN: Duration = Duration::from_secs(300);

    #[test]
    fn nominal_reciente_aprueba() {
        let decision = con_ultimo(Some(&sondeo(60)), CINCO_MIN, &Umbrales::default());
        assert!(
            matches!(decision, DecisionSalud::Aprueba(ref texto) if texto.starts_with("NOMINAL"))
        );
    }

    #[test]
    fn viejo_o_inexistente_hay_que_sondear() {
        assert_eq!(
            con_ultimo(Some(&sondeo(600)), CINCO_MIN, &Umbrales::default()),
            DecisionSalud::Sondear
        );
        assert_eq!(
            con_ultimo(None, CINCO_MIN, &Umbrales::default()),
            DecisionSalud::Sondear
        );
    }

    #[test]
    fn carga_reciente_rechaza_con_los_culpables() {
        let mut cargado = sondeo(30);
        cargado.mem_disponible_kb = Some(60);
        cargado
            .servicios
            .insert("nginx".to_string(), "failed".to_string());
        match con_ultimo(Some(&cargado), CINCO_MIN, &Umbrales::default()) {
            DecisionSalud::Rechaza(texto) => {
                assert!(texto.starts_with("CARGA: memoria 94 %"), "{texto}");
                assert!(texto.contains("nginx inactivo"), "{texto}");
            }
            otro => panic!("{otro:?}"),
        }
    }

    #[test]
    fn alcanzable_y_caida_rechazan() {
        let mut sin_metricas = sondeo(10);
        sin_metricas.resultado = ResultadoSondeo::SinMetricas;
        assert!(matches!(
            con_ultimo(Some(&sin_metricas), CINCO_MIN, &Umbrales::default()),
            DecisionSalud::Rechaza(ref texto) if texto.contains("sin métricas")
        ));
        let mut caido = sondeo(10);
        caido.resultado = ResultadoSondeo::Error;
        caido.error = Some("timeout tras 5 s".to_string());
        let (aprueba, detalle) = con_sondeo_nuevo(&caido, &Umbrales::default());
        assert!(!aprueba);
        assert_eq!(detalle, "CAÍDA: timeout tras 5 s");
    }
}
