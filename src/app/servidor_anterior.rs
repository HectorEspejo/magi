//! Decisión ante un servidor de sesiones de otra versión de protocolo
//! (Fase 9, §7.2 del informe): sustituirlo sin preguntar solo si no custodia
//! nada; preguntar siempre que haya trabajo, otras ventanas o recuento
//! desconocido; nunca parar uno más nuevo.
//!
//! Este módulo es lógica pura: no toca el socket, ni la UI, ni el sistema. La
//! ejecución de la sustitución (SIGTERM, espera y autolanzado) vive en
//! `cliente` y el diálogo en `ui::servidor_anterior`.

use crate::cliente::RechazoVersion;
use crate::protocolo::VERSION_PROTOCOLO;

/// Qué hacer con el servidor que rechazó el saludo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// El servidor habla un protocolo mayor: este binario es el viejo y no
    /// puede hacer nada con él (ni pararlo).
    MasNuevo,
    /// No custodia nada ni tiene otras ventanas: se puede sustituir solo.
    Sustituir,
    /// Hay trabajo, otras ventanas o no se sabe: decide el usuario.
    Preguntar,
}

/// Decide con lo que el rechazo dice. Un campo ausente de `trabajo` vale 1
/// (hay trabajo), así que un servidor futuro que añada otro tipo de trabajo
/// nunca se sustituye por error.
pub fn decidir(rechazo: &RechazoVersion) -> Decision {
    if rechazo.version > VERSION_PROTOCOLO {
        return Decision::MasNuevo;
    }
    match (rechazo.trabajo, rechazo.clientes) {
        (Some(trabajo), Some(clientes)) if clientes == 0 && trabajo.vacio() => Decision::Sustituir,
        _ => Decision::Preguntar,
    }
}

/// Versión de la app del servidor, o «desconocida» si no la informa.
pub fn version_app(rechazo: &RechazoVersion) -> String {
    rechazo
        .version_app
        .clone()
        .unwrap_or_else(|| "desconocida".to_string())
}

/// Lo que custodia el servidor, en una línea: «2 sesiones · 1 túnel · 0
/// transferencias · 0 ejecuciones». Un servidor anterior a la v6 no lo
/// informa.
pub fn custodia(rechazo: &RechazoVersion) -> String {
    match rechazo.trabajo {
        Some(trabajo) => format!(
            "{} · {} · {} · {}",
            plural(trabajo.sesiones, "sesión", "sesiones"),
            plural(trabajo.tuneles, "túnel", "túneles"),
            plural(trabajo.transferencias, "transferencia", "transferencias"),
            plural(trabajo.ejecuciones, "ejecución", "ejecuciones"),
        ),
        None => "la versión anterior no informa de lo que custodia".to_string(),
    }
}

/// Lo que se perderá al pararlo, para la confirmación: «cerrará 2 sesiones y
/// 1 túnel». Sin recuento conocido se avisa de que no se sabe.
pub fn confirmacion(rechazo: &RechazoVersion) -> String {
    let Some(trabajo) = rechazo.trabajo else {
        return "la versión anterior no informa de lo que custodia y se cerrará lo que tenga".to_string();
    };
    let mut partes = Vec::new();
    for (cantidad, singular, plural_) in [
        (trabajo.sesiones, "sesión", "sesiones"),
        (trabajo.transferencias, "transferencia", "transferencias"),
        (trabajo.tuneles, "túnel", "túneles"),
        (trabajo.ejecuciones, "ejecución", "ejecuciones"),
    ] {
        if cantidad > 0 {
            partes.push(plural(cantidad, singular, plural_));
        }
    }
    let ventanas = match rechazo.clientes {
        Some(1) => Some("1 ventana más".to_string()),
        Some(clientes) if clientes > 1 => Some(format!("{clientes} ventanas más")),
        _ => None,
    };
    if let Some(ventanas) = ventanas {
        partes.push(ventanas);
    }
    if partes.is_empty() {
        "no custodia nada".to_string()
    } else {
        format!("cerrará {}", unir_con_y(&partes))
    }
}

fn plural(cantidad: u32, singular: &str, plural_: &str) -> String {
    if cantidad == 1 {
        format!("1 {singular}")
    } else {
        format!("{cantidad} {plural_}")
    }
}

fn unir_con_y(partes: &[String]) -> String {
    match partes {
        [] => String::new(),
        [una] => una.clone(),
        [primera, segunda] => format!("{primera} y {segunda}"),
        _ => {
            let (ultima, resto) = partes.split_last().expect("no está vacía");
            format!("{} y {ultima}", resto.join(", "))
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::protocolo::TrabajoServidor;

    fn rechazo(version: u32, trabajo: Option<TrabajoServidor>, clientes: Option<u32>) -> RechazoVersion {
        let mut rechazo = RechazoVersion::nueva(version);
        rechazo.trabajo = trabajo;
        rechazo.clientes = clientes;
        rechazo
    }

    fn trabajo(sesiones: u32, transferencias: u32, tuneles: u32, ejecuciones: u32) -> TrabajoServidor {
        TrabajoServidor {
            sesiones,
            transferencias,
            tuneles,
            ejecuciones,
        }
    }

    #[test]
    fn sin_trabajo_ni_ventanas_se_sustituye_solo() {
        assert_eq!(
            decidir(&rechazo(5, Some(trabajo(0, 0, 0, 0)), Some(0))),
            Decision::Sustituir
        );
    }

    #[test]
    fn con_trabajo_o_ventanas_se_pregunta() {
        for (trabajo, clientes) in [
            (Some(trabajo(2, 0, 0, 0)), Some(0)),
            (Some(trabajo(0, 1, 0, 0)), Some(0)),
            (Some(trabajo(0, 0, 1, 0)), Some(0)),
            (Some(trabajo(0, 0, 0, 1)), Some(0)),
            (Some(trabajo(0, 0, 0, 0)), Some(1)),
        ] {
            assert_eq!(decidir(&rechazo(5, trabajo, clientes)), Decision::Preguntar);
        }
    }

    #[test]
    fn un_servidor_anterior_a_la_v6_no_informa_y_se_pregunta() {
        assert_eq!(decidir(&rechazo(5, None, None)), Decision::Preguntar);
        assert_eq!(decidir(&rechazo(5, None, Some(0))), Decision::Preguntar);
        assert_eq!(decidir(&rechazo(5, Some(trabajo(0, 0, 0, 0)), None)), Decision::Preguntar);
    }

    #[test]
    fn un_servidor_mas_nuevo_nunca_se_para() {
        assert_eq!(
            decidir(&rechazo(7, Some(trabajo(0, 0, 0, 0)), Some(0))),
            Decision::MasNuevo
        );
    }

    #[test]
    fn el_resumen_cuenta_lo_que_custodia() {
        let rechazo = rechazo(5, Some(trabajo(2, 0, 1, 0)), Some(1));
        assert_eq!(
            custodia(&rechazo),
            "2 sesiones · 1 túnel · 0 transferencias · 0 ejecuciones"
        );
        assert_eq!(confirmacion(&rechazo), "cerrará 2 sesiones, 1 túnel y 1 ventana más");
        assert_eq!(version_app(&rechazo), "desconocida");
    }

    #[test]
    fn sin_datos_de_custodia_se_avisa_de_que_no_se_sabe() {
        let rechazo = rechazo(5, None, None);
        assert_eq!(
            custodia(&rechazo),
            "la versión anterior no informa de lo que custodia"
        );
        assert!(confirmacion(&rechazo).contains("no informa"));
    }

    #[test]
    fn la_confirmacion_en_singular_no_dice_uno_sesions() {
        let rechazo = rechazo(5, Some(trabajo(1, 1, 0, 0)), Some(0));
        assert_eq!(
            confirmacion(&rechazo),
            "cerrará 1 sesión y 1 transferencia"
        );
    }
}
