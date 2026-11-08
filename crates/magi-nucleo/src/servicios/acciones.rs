//! Acciones permitidas por tipo y estado, deliberación exigida y unidades
//! críticas (§7.2 y §7.9). Lo usan el cliente (para ofrecer teclas y pedir
//! confirmación o deliberación) y el servidor (para recomprobar).

use super::{
    orden, Accion, Contenedor, EstadoArranque, EstadoContenedor, Objetivo, Salud, Timer, Unidad,
};

/// Acciones que se ofrecen para una unidad de servicio, en el orden de la
/// barra (A, P, R, G, H).
pub fn acciones_de_unidad(unidad: &Unidad) -> Vec<Accion> {
    // Una unidad enmascarada se ve, pero no se le ofrece nada (§3.8).
    if unidad.enmascarada() || unidad.prop("LoadState") == "not-found" {
        return Vec::new();
    }
    let activo = unidad.activo();
    let mut acciones = Vec::new();
    if matches!(activo, "inactive" | "failed") {
        acciones.push(Accion::Arrancar);
    }
    if matches!(activo, "active" | "activating" | "reloading") {
        acciones.push(Accion::Parar);
    }
    if matches!(activo, "active" | "inactive" | "failed") {
        acciones.push(Accion::Reiniciar);
    }
    if activo == "active" && unidad.puede_recargar() {
        acciones.push(Accion::Recargar);
    }
    match unidad.arranque() {
        EstadoArranque::Desactivado => acciones.push(Accion::Activar),
        EstadoArranque::Activado => acciones.push(Accion::Desactivar),
        _ => {}
    }
    acciones
}

/// Acciones que se ofrecen para un timer (A = lanzar ahora) y su servicio.
pub fn acciones_de_timer(timer: &Timer, estado_servicio: Option<&str>) -> Vec<Accion> {
    let enmascarado = timer.prop("LoadState") == "masked"
        || EstadoArranque::desde_texto(timer.prop("UnitFileState")) == EstadoArranque::Enmascarado;
    if enmascarado {
        return Vec::new();
    }
    let servicio_activo = estado_servicio.is_some_and(timer_servicio_activo);
    if servicio_activo {
        Vec::new()
    } else {
        vec![Accion::Lanzar]
    }
}

/// Acciones que se ofrecen para un contenedor.
pub fn acciones_de_contenedor(contenedor: &Contenedor) -> Vec<Accion> {
    match contenedor.estado {
        // Un contenedor recién creado nunca se reinicia: solo se arranca.
        EstadoContenedor::Creado => vec![Accion::Arrancar],
        EstadoContenedor::Parado => vec![Accion::Arrancar, Accion::Reiniciar],
        EstadoContenedor::EnMarcha => vec![Accion::Parar, Accion::Reiniciar],
        EstadoContenedor::Reiniciando => vec![Accion::Parar, Accion::Reiniciar],
        // paused y dead: sin acciones (logs y detalle sí).
        EstadoContenedor::Pausado | EstadoContenedor::Muerto | EstadoContenedor::Desconocido => {
            Vec::new()
        }
    }
}

/// Acciones que se ofrecen para un proyecto Compose.
///
/// Sin los ficheros de Compose conocidos no se ofrecen acciones de proyecto
/// (§7.12): el motivo lo pone la vista con `Proyecto::con_ficheros`.
pub fn acciones_de_proyecto(contenedores: &[Contenedor]) -> Vec<Accion> {
    let hay_en_marcha = contenedores
        .iter()
        .any(|contenedor| contenedor.estado.en_marcha());
    let hay_parado = contenedores
        .iter()
        .any(|contenedor| !contenedor.estado.en_marcha());
    let mut acciones = Vec::new();
    if hay_parado {
        acciones.push(Accion::Arrancar);
    }
    if hay_en_marcha {
        acciones.push(Accion::Parar);
    }
    acciones.push(Accion::Reiniciar);
    acciones
}

/// ¿La acción deja algo parado y por tanto exige deliberación? (§7.2)
pub fn exige_deliberacion(objetivo: &Objetivo, accion: Accion) -> bool {
    use super::TipoObjetivo::*;
    matches!(
        (objetivo.tipo, accion),
        (
            Unidad | Timer,
            Accion::Parar | Accion::Reiniciar | Accion::Desactivar
        ) | (Contenedor | Proyecto, Accion::Parar | Accion::Reiniciar)
    )
}

/// ¿La acción exige comprobar la configuración antes? Solo recargar y
/// reiniciar (§7.5).
pub fn comprueba_configuracion(objetivo: &Objetivo, accion: Accion) -> bool {
    use super::TipoObjetivo::*;
    matches!(
        (objetivo.tipo, accion),
        (Unidad | Timer, Accion::Recargar | Accion::Reiniciar) | (Proyecto, Accion::Reiniciar)
    )
}

/// ¿La acción es privilegiada (usa sudo si hace falta)?
pub fn es_privilegiada(accion: Accion) -> bool {
    let _ = accion;
    true
}

/// ¿El nombre coincide con alguno de los patrones de unidades críticas?
pub fn es_critica(patrones: &[String], nombre: &str) -> bool {
    patrones
        .iter()
        .any(|patron| orden::coincide_patron(patron, nombre))
}

/// Motivo por el que una acción no se ofrece, para el aviso de la tecla
/// («nginx.service ya está parado», «nginx.service no admite recargar»…).
pub fn motivo_no_permitida(
    objetivo: &Objetivo,
    accion: Accion,
    permitidas: &[Accion],
) -> Option<String> {
    if permitidas.contains(&accion) {
        return None;
    }
    let nombre = &objetivo.nombre;
    let motivo = match accion {
        Accion::Recargar => format!("{nombre} no admite recargar"),
        Accion::Parar => format!("{nombre} ya está parado"),
        Accion::Arrancar => format!("{nombre} ya está en marcha"),
        Accion::Reiniciar => format!("{nombre} no se puede reiniciar en su estado actual"),
        Accion::Activar => format!("{nombre} no se puede activar en su estado actual"),
        Accion::Desactivar => format!("{nombre} no se puede desactivar en su estado actual"),
        Accion::Lanzar => format!("{nombre}: su servicio ya está activo"),
    };
    Some(motivo)
}

/// Estado de la unidad como texto corto («activo», «inactivo», «fallido»,
/// «cambiando»). El glifo lo pone la vista.
pub fn texto_estado_unidad(unidad: &Unidad) -> &'static str {
    if unidad.enmascarada() {
        return "enmascarada";
    }
    match unidad.activo() {
        "active" => "activo",
        "inactive" => "inactivo",
        "failed" => "fallido",
        "activating" => "arrancando",
        "deactivating" => "parando",
        "reloading" => "recargando",
        _ => "desconocido",
    }
}

/// Estado de un contenedor como texto corto («en marcha», «parado»…) con el
/// código de salida si lo hay.
pub fn texto_estado_contenedor(contenedor: &Contenedor) -> String {
    if contenedor.salud == Salud::Insegura {
        return "unhealthy".to_string();
    }
    match contenedor.estado {
        EstadoContenedor::Parado => match contenedor.codigo_salida {
            Some(codigo) if codigo != 0 => format!("salió ({codigo})"),
            Some(codigo) => format!("parado ({codigo})"),
            None => "parado".to_string(),
        },
        EstadoContenedor::Creado => "creado".to_string(),
        EstadoContenedor::EnMarcha => {
            if contenedor.salud == Salud::Comprobando {
                "comprobando".to_string()
            } else {
                "en marcha".to_string()
            }
        }
        EstadoContenedor::Reiniciando => "reiniciándose".to_string(),
        EstadoContenedor::Pausado => "en pausa".to_string(),
        EstadoContenedor::Muerto => "muerto".to_string(),
        EstadoContenedor::Desconocido => "desconocido".to_string(),
    }
}

/// ¿El estado de la unidad es final (active, inactive o failed)?
pub fn estado_unidad_final(activo: &str) -> bool {
    matches!(activo, "active" | "inactive" | "failed")
}

/// ¿La acción ha terminado bien según el estado final? (§7.4)
pub fn exito(
    objetivo: &Objetivo,
    accion: Accion,
    unidad: Option<&Unidad>,
    contenedor: Option<&Contenedor>,
    proyecto: &[Contenedor],
) -> bool {
    use super::TipoObjetivo::*;
    match objetivo.tipo {
        Unidad | Timer => {
            let Some(unidad) = unidad else {
                return false;
            };
            match accion {
                Accion::Arrancar | Accion::Reiniciar | Accion::Lanzar => {
                    unidad.activo() == "active"
                        || (unidad.tipo() == "oneshot"
                            && unidad.activo() == "inactive"
                            && unidad.resultado() == "success")
                }
                Accion::Parar => unidad.activo() == "inactive",
                Accion::Recargar => unidad.activo() == "active",
                Accion::Activar => unidad.arranque() == EstadoArranque::Activado,
                Accion::Desactivar => unidad.arranque() == EstadoArranque::Desactivado,
            }
        }
        Contenedor => {
            let Some(contenedor) = contenedor else {
                return false;
            };
            if accion == Accion::Parar {
                contenedor.estado == EstadoContenedor::Parado
            } else {
                contenedor.estado.en_marcha() && contenedor.salud != Salud::Insegura
            }
        }
        Proyecto => {
            let en_marcha = proyecto
                .iter()
                .map(|contenedor| contenedor.estado.en_marcha())
                .collect::<Vec<_>>();
            if accion == Accion::Parar {
                !en_marcha.iter().any(|vivo| *vivo)
            } else {
                en_marcha.iter().all(|vivo| *vivo)
            }
        }
    }
}

/// ¿Un timer lanza un servicio que ya está activo? (para no ofrecer `A`).
pub fn timer_servicio_activo(activo: &str) -> bool {
    matches!(activo, "active" | "activating" | "reloading")
}

/// ¿Se puede ofrecer una acción de arranque según el estado de arranque?
pub fn arranque_conmutable(estado: EstadoArranque) -> bool {
    estado.conmutable()
}

/// Estados en los que un contenedor se considera caído (§4.6).
pub fn contenedor_caido(contenedor: &Contenedor) -> bool {
    contenedor.caido()
}

/// Estados de unidad que se listan como «cargados» a efectos de acciones.
pub fn unidad_utilizable(unidad: &Unidad) -> bool {
    !unidad.enmascarada() && unidad.prop("LoadState") != "not-found"
}

/// ¿La unidad existe para systemd (no es un nombre inventado)?
pub fn unidad_cargada(unidad: &Unidad) -> bool {
    unidad.cargada()
}
