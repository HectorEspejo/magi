//! El plan que se delibera: una ejecución de snippet o una acción sobre un
//! servicio o contenedor. Se fija al abrir la deliberación (T33) y no se
//! relee del estado al confirmar. El escritorio añade su variante de
//! sincronización en su propio enum (que envuelve estas dos).

use std::collections::BTreeMap;

use crate::servicios::{Accion, Objetivo};

/// Una ejecución de snippet ya resuelta: comando con las variables
/// sustituidas y hosts elegidos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEjecucion {
    pub snippet_id: i64,
    pub nombre: String,
    /// El comando con las variables ya sustituidas (escapadas).
    pub comando: String,
    pub hosts: Vec<(i64, String)>,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub critico: bool,
    /// Valores de las variables, para repetir la ejecución.
    pub valores: BTreeMap<String, String>,
}

impl PlanEjecucion {
    /// «reiniciar nginx → hetzner-01, hetzner-02»: la acción de la
    /// deliberación y de DELIBERACIONES.accion.
    pub fn accion(&self) -> String {
        let hosts: Vec<&str> = self
            .hosts
            .iter()
            .map(|(_, nombre)| nombre.as_str())
            .collect();
        format!("{} → {}", self.nombre, hosts.join(", "))
    }
}

/// Una acción sobre un servicio, un timer, un contenedor o un proyecto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanServicio {
    pub host_id: i64,
    pub host_nombre: String,
    pub objetivo: Objetivo,
    pub accion: Accion,
    /// Comprobación de configuración que se aplicará antes, para la línea
    /// «Antes de reiniciar: nginx -t» del diálogo.
    pub comprobacion: Option<String>,
}

impl PlanServicio {
    /// El texto que se guarda en DELIBERACIONES.accion: siempre con la flecha
    /// Unicode, aunque la terminal sea ASCII (el texto de la fila no depende
    /// del modo de pintado).
    pub fn texto_accion(&self) -> String {
        format!(
            "{} {} → {}",
            self.accion.texto(),
            self.objetivo.nombre,
            self.host_nombre
        )
    }
}

/// Lo que se delibera en el móvil (y lo que comparten las comprobaciones).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanDeliberado {
    Snippet(PlanEjecucion),
    Servicio(PlanServicio),
}

impl PlanDeliberado {
    /// La acción que queda en DELIBERACIONES.
    pub fn accion(&self) -> String {
        match self {
            PlanDeliberado::Snippet(plan) => plan.accion(),
            PlanDeliberado::Servicio(plan) => plan.texto_accion(),
        }
    }

    /// Hosts que se comprueban, en el orden del plan.
    pub fn hosts(&self) -> Vec<(i64, String)> {
        match self {
            PlanDeliberado::Snippet(plan) => plan.hosts.clone(),
            PlanDeliberado::Servicio(plan) => {
                vec![(plan.host_id, plan.host_nombre.clone())]
            }
        }
    }

    /// El snippet del que sale la ejecución (las acciones de servicio no
    /// tienen snippet: van con NULL).
    pub fn snippet_id(&self) -> Option<i64> {
        match self {
            PlanDeliberado::Snippet(plan) => Some(plan.snippet_id),
            PlanDeliberado::Servicio(_) => None,
        }
    }

    /// En una acción de servicio, el objetivo que MELCHIOR-1 no cuenta.
    pub fn objetivo_de_servicio(&self) -> Option<&Objetivo> {
        match self {
            PlanDeliberado::Snippet(_) => None,
            PlanDeliberado::Servicio(plan) => Some(&plan.objetivo),
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_accion_del_snippet_lleva_los_hosts() {
        let plan = PlanEjecucion {
            snippet_id: 3,
            nombre: "reiniciar nginx".to_string(),
            comando: "systemctl restart nginx".to_string(),
            hosts: vec![(1, "web-01".to_string()), (2, "web-02".to_string())],
            timeout_seg: 60,
            parar_al_fallo: false,
            critico: false,
            valores: BTreeMap::new(),
        };
        let plan = PlanDeliberado::Snippet(plan);
        assert_eq!(plan.accion(), "reiniciar nginx → web-01, web-02");
        assert_eq!(plan.hosts().len(), 2);
        assert_eq!(plan.snippet_id(), Some(3));
        assert!(plan.objetivo_de_servicio().is_none());
    }

    #[test]
    fn la_accion_del_servicio_lleva_el_objetivo_y_el_host() {
        let plan = PlanDeliberado::Servicio(PlanServicio {
            host_id: 7,
            host_nombre: "web-01".to_string(),
            objetivo: Objetivo::unidad("app.service"),
            accion: Accion::Reiniciar,
            comprobacion: Some("nginx -t".to_string()),
        });
        assert_eq!(plan.accion(), "reiniciar app.service → web-01");
        assert_eq!(plan.hosts(), vec![(7, "web-01".to_string())]);
        assert_eq!(plan.snippet_id(), None);
        assert_eq!(
            plan.objetivo_de_servicio()
                .map(|objetivo| objetivo.nombre.as_str()),
            Some("app.service")
        );
    }
}
