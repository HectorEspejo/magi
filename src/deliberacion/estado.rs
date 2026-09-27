//! Máquina de estados del diálogo MAGI, sin pintar nada: qué hace cada tecla
//! según la fase. `↵` nunca ejecuta; `Ctrl+K` es la única confirmación y solo
//! con consenso o con un forzado de motivo suficiente.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ui::componentes::CampoTexto;

use super::Veredicto;

/// Fase de la deliberación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    /// Alguna comprobación sigue en marcha (`◐`).
    Comprobando,
    /// Todas las activas aprueban (o no hay ninguna activa).
    Aprobada,
    /// Hubo al menos un rechazo: se requiere unanimidad.
    Bloqueada,
    /// Escribiendo el motivo del forzado.
    Motivo,
}

/// Recuento de las celdas activas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Consenso {
    pub aprobadas: usize,
    pub activas: usize,
    pub rechazos: usize,
    pub pendientes: usize,
}

impl Consenso {
    /// Unanimidad de las activas (vacío también: se pide `Ctrl+K` igualmente).
    pub fn unanime(&self) -> bool {
        self.pendientes == 0 && self.rechazos == 0
    }
}

/// Cuenta las celdas: las no activas no cuentan.
pub fn consenso<'a>(celdas: impl Iterator<Item = &'a Veredicto>) -> Consenso {
    let mut consenso = Consenso::default();
    for celda in celdas {
        match celda {
            Veredicto::NoActiva => {}
            Veredicto::Pendiente => {
                consenso.activas += 1;
                consenso.pendientes += 1;
            }
            Veredicto::Aprueba { .. } => {
                consenso.activas += 1;
                consenso.aprobadas += 1;
            }
            Veredicto::Rechaza { .. } => {
                consenso.activas += 1;
                consenso.rechazos += 1;
            }
        }
    }
    consenso
}

/// Lo que pide una tecla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionMaquina {
    Nada,
    /// Ejecutar: con consenso, o forzada con su motivo.
    Ejecutar {
        forzada: bool,
        motivo: Option<String>,
    },
    Cancelar,
    Ayuda,
    /// Recorrer los hosts de la tabla.
    Mover(i32),
}

#[derive(Debug, Clone)]
pub struct MaquinaDeliberacion {
    pub fase: Fase,
    pub motivo: CampoTexto,
    /// `Ctrl+K` con un motivo corto: el campo enseña el mínimo.
    pub aviso_minimo: bool,
    motivo_min: usize,
}

impl MaquinaDeliberacion {
    pub fn nueva(motivo_min: usize) -> Self {
        Self {
            fase: Fase::Comprobando,
            motivo: CampoTexto::default(),
            aviso_minimo: false,
            motivo_min: motivo_min.max(1),
        }
    }

    pub fn motivo_min(&self) -> usize {
        self.motivo_min
    }

    /// Caracteres útiles del motivo (sin los espacios de los extremos).
    pub fn largo_motivo(&self) -> usize {
        self.motivo.texto.trim().chars().count()
    }

    /// ¿Hay un forzado con motivo suficiente (estado FORZADO)?
    pub fn forzado_listo(&self) -> bool {
        self.fase == Fase::Motivo && self.largo_motivo() >= self.motivo_min
    }

    /// Todas las comprobaciones terminaron: aprobada o bloqueada. Solo se
    /// resuelve una vez, desde «comprobando».
    pub fn resolver(&mut self, consenso: &Consenso) {
        if self.fase != Fase::Comprobando || consenso.pendientes > 0 {
            return;
        }
        self.fase = if consenso.unanime() {
            Fase::Aprobada
        } else {
            Fase::Bloqueada
        };
    }

    pub fn pulsar(&mut self, tecla: &KeyEvent) -> AccionMaquina {
        let control = tecla.modifiers.contains(KeyModifiers::CONTROL);
        if control && matches!(tecla.code, KeyCode::Char('k') | KeyCode::Char('K')) {
            return self.confirmar();
        }
        match (self.fase, tecla.code) {
            (Fase::Motivo, KeyCode::Esc) => {
                // Se vuelve a «bloqueado» conservando lo escrito.
                self.fase = Fase::Bloqueada;
                self.aviso_minimo = false;
                AccionMaquina::Nada
            }
            (_, KeyCode::Esc) => AccionMaquina::Cancelar,
            (_, KeyCode::Up) => AccionMaquina::Mover(-1),
            (_, KeyCode::Down) => AccionMaquina::Mover(1),
            // `↵` nunca confirma: evita el «sí» por reflejo.
            (_, KeyCode::Enter) => AccionMaquina::Nada,
            (Fase::Motivo, _) => {
                if !control && self.motivo.manejar_tecla(tecla) {
                    self.aviso_minimo = false;
                }
                AccionMaquina::Nada
            }
            (Fase::Bloqueada, KeyCode::Char('f')) if !control => {
                self.fase = Fase::Motivo;
                AccionMaquina::Nada
            }
            (_, KeyCode::Char('?')) => AccionMaquina::Ayuda,
            _ => AccionMaquina::Nada,
        }
    }

    fn confirmar(&mut self) -> AccionMaquina {
        match self.fase {
            Fase::Aprobada => AccionMaquina::Ejecutar {
                forzada: false,
                motivo: None,
            },
            Fase::Motivo if self.largo_motivo() >= self.motivo_min => AccionMaquina::Ejecutar {
                forzada: true,
                motivo: Some(self.motivo.texto.trim().to_string()),
            },
            Fase::Motivo => {
                self.aviso_minimo = true;
                AccionMaquina::Nada
            }
            Fase::Comprobando | Fase::Bloqueada => AccionMaquina::Nada,
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tecla(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_k() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)
    }

    fn escribir(maquina: &mut MaquinaDeliberacion, texto: &str) {
        for c in texto.chars() {
            maquina.pulsar(&tecla(KeyCode::Char(c)));
        }
    }

    fn bloqueada() -> MaquinaDeliberacion {
        let mut maquina = MaquinaDeliberacion::nueva(10);
        maquina.resolver(&Consenso {
            aprobadas: 5,
            activas: 6,
            rechazos: 1,
            pendientes: 0,
        });
        maquina
    }

    #[test]
    fn el_consenso_cuenta_solo_las_activas() {
        let celdas = [
            Veredicto::NoActiva,
            Veredicto::Aprueba {
                detalle: String::new(),
                ms: 1,
            },
            Veredicto::Rechaza {
                detalle: String::new(),
                ms: 1,
            },
            Veredicto::Pendiente,
        ];
        let consenso = consenso(celdas.iter());
        assert_eq!(
            consenso,
            Consenso {
                aprobadas: 1,
                activas: 3,
                rechazos: 1,
                pendientes: 1
            }
        );
        assert!(!consenso.unanime());
        assert!(super::consenso([Veredicto::NoActiva].iter()).unanime());
    }

    #[test]
    fn no_se_resuelve_mientras_quede_algo_pendiente() {
        let mut maquina = MaquinaDeliberacion::nueva(10);
        maquina.resolver(&Consenso {
            aprobadas: 1,
            activas: 2,
            rechazos: 0,
            pendientes: 1,
        });
        assert_eq!(maquina.fase, Fase::Comprobando);
        assert_eq!(maquina.pulsar(&ctrl_k()), AccionMaquina::Nada);
    }

    #[test]
    fn con_consenso_ctrl_k_ejecuta_y_enter_no() {
        let mut maquina = MaquinaDeliberacion::nueva(10);
        maquina.resolver(&Consenso::default());
        assert_eq!(maquina.fase, Fase::Aprobada);
        assert_eq!(maquina.pulsar(&tecla(KeyCode::Enter)), AccionMaquina::Nada);
        assert_eq!(
            maquina.pulsar(&ctrl_k()),
            AccionMaquina::Ejecutar {
                forzada: false,
                motivo: None
            }
        );
    }

    #[test]
    fn bloqueada_no_ejecuta_sin_forzar() {
        let mut maquina = bloqueada();
        assert_eq!(maquina.pulsar(&ctrl_k()), AccionMaquina::Nada);
        assert_eq!(maquina.pulsar(&tecla(KeyCode::Enter)), AccionMaquina::Nada);
    }

    /// AC (checklist l. 91): «ok» + `Ctrl+K` no ejecuta y el campo indica el
    /// mínimo.
    #[test]
    fn un_motivo_corto_no_se_acepta() {
        let mut maquina = bloqueada();
        maquina.pulsar(&tecla(KeyCode::Char('f')));
        assert_eq!(maquina.fase, Fase::Motivo);
        escribir(&mut maquina, "ok");
        assert_eq!(maquina.pulsar(&ctrl_k()), AccionMaquina::Nada);
        assert!(maquina.aviso_minimo);
        assert!(!maquina.forzado_listo());
    }

    #[test]
    fn con_motivo_suficiente_se_fuerza() {
        let mut maquina = bloqueada();
        maquina.pulsar(&tecla(KeyCode::Char('f')));
        escribir(&mut maquina, "backup revisado a mano");
        assert!(maquina.forzado_listo());
        assert_eq!(maquina.pulsar(&tecla(KeyCode::Enter)), AccionMaquina::Nada);
        assert_eq!(
            maquina.pulsar(&ctrl_k()),
            AccionMaquina::Ejecutar {
                forzada: true,
                motivo: Some("backup revisado a mano".to_string())
            }
        );
    }

    #[test]
    fn esc_en_el_motivo_vuelve_a_bloqueada_y_despues_cancela() {
        let mut maquina = bloqueada();
        maquina.pulsar(&tecla(KeyCode::Char('f')));
        escribir(&mut maquina, "algo");
        assert_eq!(maquina.pulsar(&tecla(KeyCode::Esc)), AccionMaquina::Nada);
        assert_eq!(maquina.fase, Fase::Bloqueada);
        assert_eq!(maquina.motivo.texto, "algo");
        assert_eq!(
            maquina.pulsar(&tecla(KeyCode::Esc)),
            AccionMaquina::Cancelar
        );
    }

    #[test]
    fn f_solo_fuerza_desde_bloqueada() {
        let mut maquina = MaquinaDeliberacion::nueva(10);
        maquina.resolver(&Consenso::default());
        maquina.pulsar(&tecla(KeyCode::Char('f')));
        assert_eq!(maquina.fase, Fase::Aprobada);
        let mut comprobando = MaquinaDeliberacion::nueva(10);
        comprobando.pulsar(&tecla(KeyCode::Char('f')));
        assert_eq!(comprobando.fase, Fase::Comprobando);
    }

    #[test]
    fn los_espacios_no_cuentan_para_el_minimo() {
        let mut maquina = bloqueada();
        maquina.pulsar(&tecla(KeyCode::Char('f')));
        escribir(&mut maquina, "   ok      ");
        assert_eq!(maquina.largo_motivo(), 2);
        assert_eq!(maquina.pulsar(&ctrl_k()), AccionMaquina::Nada);
    }
}
