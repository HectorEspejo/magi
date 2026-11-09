//! Consenso de la deliberación: la unanimidad de las comprobaciones activas.
//! Una celda no activa (—) no cuenta; una pendiente impide resolver.

use super::Veredicto;

/// Recuento de las celdas activas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Consenso {
    pub aprobadas: usize,
    pub activas: usize,
    pub rechazos: usize,
    pub pendientes: usize,
}

impl Consenso {
    /// Unanimidad de las activas (vacío también: se pide confirmar igualmente).
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

#[cfg(test)]
mod pruebas {
    use super::*;

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
        let cuenta = consenso(celdas.iter());
        assert_eq!(
            cuenta,
            Consenso {
                aprobadas: 1,
                activas: 3,
                rechazos: 1,
                pendientes: 1
            }
        );
        assert!(!cuenta.unanime());
        assert!(consenso([Veredicto::NoActiva].iter()).unanime());
    }
}
