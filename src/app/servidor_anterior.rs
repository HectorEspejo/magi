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

/// Estado del diálogo SERVIDOR DE SESIONES ANTERIOR (Fase 9, §6.2): lleva
/// fijado el rechazo del servidor para no releer nada al confirmar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogoServidorAnterior {
    /// Hay trabajo, otras ventanas o recuento desconocido: decide el usuario
    /// con `p` (confirmación), `Esc` (seguir sin sesiones) o `q` (salir).
    Aviso(RechazoVersion),
    /// `p` ya se pulsó: `s` confirma la parada y `n` vuelve al aviso.
    Confirmacion(RechazoVersion),
    /// `SIGTERM` enviado: se espera a que libere socket y lock (máx. 5 s).
    /// Mientras tanto se ignora el teclado.
    Sustituyendo(RechazoVersion),
    /// No se pudo sustituir: el servidor anterior sigue como estaba.
    Error {
        rechazo: RechazoVersion,
        motivo: String,
    },
}

impl DialogoServidorAnterior {
    /// El rechazo que fijó el diálogo.
    pub fn rechazo(&self) -> &RechazoVersion {
        match self {
            Self::Aviso(rechazo)
            | Self::Confirmacion(rechazo)
            | Self::Sustituyendo(rechazo)
            | Self::Error { rechazo, .. } => rechazo,
        }
    }

    /// ¿Sigue esperando la respuesta de `cliente::sustituir_servidor`?
    pub fn sustituyendo(&self) -> bool {
        matches!(self, Self::Sustituyendo(_))
    }
}

/// ¿El servidor habla un protocolo mayor que este binario? En ese caso el
/// viejo es este MAGI y nunca se para al servidor desde aquí.
pub fn es_mas_nuevo(rechazo: &RechazoVersion) -> bool {
    rechazo.version > VERSION_PROTOCOLO
}

/// Decide con lo que el rechazo dice. Un campo ausente de `trabajo` vale 1
/// (hay trabajo), así que un servidor futuro que añada otro tipo de trabajo
/// nunca se sustituye por error.
pub fn decidir(rechazo: &RechazoVersion) -> Decision {
    if es_mas_nuevo(rechazo) {
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

/// Los cuatro recuentos de lo que custodia, en el orden del diálogo
/// (sesiones, túneles, transferencias y ejecuciones); `None` si su versión no
/// lo informa.
pub fn recuentos_custodia(rechazo: &RechazoVersion) -> Option<[String; 4]> {
    rechazo.trabajo.map(|trabajo| {
        [
            plural(trabajo.sesiones, "sesión", "sesiones"),
            plural(trabajo.tuneles, "túnel", "túneles"),
            plural(trabajo.transferencias, "transferencia", "transferencias"),
            plural(trabajo.ejecuciones, "ejecución", "ejecuciones"),
        ]
    })
}

/// Lo que custodia el servidor, repartido en dos líneas para el diálogo
/// («2 sesiones · 1 túnel» / «0 transferencias · 0 ejecuciones»). Un servidor
/// anterior a la v6 no lo informa.
pub fn lineas_custodia(rechazo: &RechazoVersion) -> Vec<String> {
    match recuentos_custodia(rechazo) {
        Some([sesiones, tuneles, transferencias, ejecuciones]) => vec![
            format!("{sesiones} · {tuneles}"),
            format!("{transferencias} · {ejecuciones}"),
        ],
        None => vec!["la versión anterior no informa de lo que custodia".to_string()],
    }
}

/// Otras ventanas de la versión anterior, si el rechazo las informa: «1
/// ventana más de la versión anterior».
pub fn ventanas(rechazo: &RechazoVersion) -> Option<String> {
    match rechazo.clientes {
        Some(1) => Some("1 ventana más de la versión anterior".to_string()),
        Some(clientes) if clientes > 1 => {
            Some(format!("{clientes} ventanas más de la versión anterior"))
        }
        _ => None,
    }
}

/// Aviso de la barra mientras el servidor siga siendo de otra versión (Fase
/// 9, §4.4): uno más nuevo no se puede parar desde este binario; uno anterior
/// sin sustituir se queda y hay que pararlo a mano.
pub fn aviso_barra(rechazo: &RechazoVersion) -> String {
    if es_mas_nuevo(rechazo) {
        return "este magi es más antiguo que su servidor; actualiza magi".to_string();
    }
    let pid = rechazo
        .pid
        .map(|pid| format!(", pid {pid}"))
        .unwrap_or_default();
    format!(
        "servidor de una versión anterior ({}, protocolo {}{pid}): sin sustituir; «magi servidor parar» y volver a abrir",
        version_app(rechazo),
        rechazo.version
    )
}

/// Por qué no se puede usar el servidor para una operación que lo necesita
/// (abrir sesión, túnel, edición remota…). `None` si no hay incompatibilidad
/// de versión. Un servidor más nuevo nunca se para desde este binario: se
/// actualiza MAGI; un servidor anterior sin sustituir se para a mano.
pub fn motivo_sin_servidor(rechazo: Option<&RechazoVersion>) -> Option<String> {
    let rechazo = rechazo?;
    Some(if es_mas_nuevo(rechazo) {
        "este magi es más antiguo que su servidor; actualiza magi".to_string()
    } else {
        "servidor de otra versión de protocolo: «magi servidor parar» y volver a abrir".to_string()
    })
}

/// Lo que se perderá al pararlo, para la confirmación: «cerrará 2 sesiones y
/// 1 túnel». Sin recuento conocido se avisa de que no se sabe.
pub fn confirmacion(rechazo: &RechazoVersion) -> String {
    let Some(trabajo) = rechazo.trabajo else {
        return "la versión anterior no informa de lo que custodia y se cerrará lo que tenga"
            .to_string();
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

    fn rechazo(
        version: u32,
        trabajo: Option<TrabajoServidor>,
        clientes: Option<u32>,
    ) -> RechazoVersion {
        let mut rechazo = RechazoVersion::nueva(version);
        rechazo.trabajo = trabajo;
        rechazo.clientes = clientes;
        rechazo
    }

    fn trabajo(
        sesiones: u32,
        transferencias: u32,
        tuneles: u32,
        ejecuciones: u32,
    ) -> TrabajoServidor {
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
        assert_eq!(
            decidir(&rechazo(5, Some(trabajo(0, 0, 0, 0)), None)),
            Decision::Preguntar
        );
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
        assert_eq!(
            confirmacion(&rechazo),
            "cerrará 2 sesiones, 1 túnel y 1 ventana más"
        );
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
        assert_eq!(confirmacion(&rechazo), "cerrará 1 sesión y 1 transferencia");
    }

    #[test]
    fn la_custodia_del_dialogo_va_en_dos_lineas() {
        let con_trabajo = rechazo(5, Some(trabajo(2, 0, 1, 3)), Some(0));
        assert_eq!(
            lineas_custodia(&con_trabajo),
            vec!["2 sesiones · 1 túnel", "0 transferencias · 3 ejecuciones"]
        );
        let sin_datos = rechazo(5, None, None);
        assert_eq!(
            lineas_custodia(&sin_datos),
            vec!["la versión anterior no informa de lo que custodia"]
        );
    }

    #[test]
    fn las_ventanas_mas_se_cuentan_solo_si_se_saben() {
        assert_eq!(
            ventanas(&rechazo(5, Some(trabajo(0, 0, 0, 0)), Some(1))).as_deref(),
            Some("1 ventana más de la versión anterior")
        );
        assert_eq!(
            ventanas(&rechazo(5, Some(trabajo(0, 0, 0, 0)), Some(3))).as_deref(),
            Some("3 ventanas más de la versión anterior")
        );
        assert_eq!(
            ventanas(&rechazo(5, Some(trabajo(0, 0, 0, 0)), Some(0))),
            None
        );
        assert_eq!(ventanas(&rechazo(5, None, None)), None);
    }

    #[test]
    fn la_barra_avisa_de_forma_distinta_al_mas_nuevo_y_al_anterior() {
        let mas_nuevo = rechazo(7, Some(trabajo(0, 0, 0, 0)), Some(0));
        assert_eq!(
            aviso_barra(&mas_nuevo),
            "este magi es más antiguo que su servidor; actualiza magi"
        );
        let mut anterior = rechazo(5, Some(trabajo(2, 0, 1, 0)), Some(0));
        anterior.version_app = Some("1.0.0".to_string());
        anterior.pid = Some(48211);
        assert_eq!(
            aviso_barra(&anterior),
            "servidor de una versión anterior (1.0.0, protocolo 5, pid 48211): sin sustituir; «magi servidor parar» y volver a abrir"
        );
    }

    #[test]
    fn el_estado_del_dialogo_lleva_fijado_el_rechazo() {
        let original = rechazo(5, Some(trabajo(2, 0, 0, 0)), Some(0));
        for dialogo in [
            DialogoServidorAnterior::Aviso(original.clone()),
            DialogoServidorAnterior::Confirmacion(original.clone()),
            DialogoServidorAnterior::Sustituyendo(original.clone()),
            DialogoServidorAnterior::Error {
                rechazo: original.clone(),
                motivo: "no liberó el lock".to_string(),
            },
        ] {
            assert_eq!(dialogo.rechazo(), &original);
        }
        assert!(DialogoServidorAnterior::Sustituyendo(original.clone()).sustituyendo());
        assert!(!DialogoServidorAnterior::Aviso(original).sustituyendo());
    }

    #[test]
    fn las_operaciones_que_necesitan_servidor_distinguen_mas_nuevo_de_anterior() {
        assert_eq!(motivo_sin_servidor(None), None);
        let mas_nuevo = rechazo(7, Some(trabajo(0, 0, 0, 0)), Some(0));
        assert_eq!(
            motivo_sin_servidor(Some(&mas_nuevo)).as_deref(),
            Some("este magi es más antiguo que su servidor; actualiza magi")
        );
        let anterior = rechazo(5, Some(trabajo(0, 0, 0, 0)), Some(0));
        let motivo = motivo_sin_servidor(Some(&anterior)).unwrap();
        assert!(motivo.contains("magi servidor parar"), "{motivo}");
        assert!(
            !motivo_sin_servidor(Some(&mas_nuevo))
                .unwrap()
                .contains("servidor parar"),
            "un servidor más nuevo nunca se para desde aquí"
        );
    }
}
