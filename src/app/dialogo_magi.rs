//! Diálogo de deliberación MAGI (Fase 6): comprobaciones por host en
//! paralelo, consenso, `Ctrl+K` para ejecutar y `f` para forzar con motivo.
//! En el sprint S2 solo existe el enganche: el lanzamiento se detiene con un
//! aviso cuando hace falta deliberar.

use crate::snippets::MotivoDeliberacion;

use super::lanzar::PlanEjecucion;
use super::App;

// Contrato del sprint S2: lo llama `continuar_plan`.
#[allow(dead_code)]
impl App {
    /// El plan necesita deliberación (crítico, más de un host o hosts con
    /// verificaciones): abre el diálogo MAGI.
    pub(super) fn abrir_deliberacion(
        &mut self,
        plan: PlanEjecucion,
        motivos: Vec<MotivoDeliberacion>,
    ) {
        let motivos: Vec<String> = motivos.iter().map(MotivoDeliberacion::texto).collect();
        self.mensaje(
            format!(
                "«{}» requiere deliberación MAGI ({}): llega en el sprint S3",
                plan.nombre,
                motivos.join(" · ")
            ),
            true,
        );
    }
}
