//! Acciones permitidas por tipo y estado, deliberación exigida y unidades
//! críticas (§7.2 y §7.9). Lo usan el cliente (para ofrecer teclas y pedir
//! confirmación o deliberación) y el servidor (para recomprobar).

use super::{Accion, Contenedor, EstadoArranque, Objetivo, Timer, Unidad};

/// Acciones que se ofrecen para una unidad de servicio, en el orden de la
/// barra (A, P, R, G, H).
pub fn acciones_de_unidad(unidad: &Unidad) -> Vec<Accion> {
    let _ = unidad;
    Vec::new()
}

/// Acciones que se ofrecen para un timer (A = lanzar ahora) y su servicio.
pub fn acciones_de_timer(timer: &Timer, estado_servicio: Option<&str>) -> Vec<Accion> {
    let _ = (timer, estado_servicio);
    Vec::new()
}

/// Acciones que se ofrecen para un contenedor.
pub fn acciones_de_contenedor(contenedor: &Contenedor) -> Vec<Accion> {
    let _ = contenedor;
    Vec::new()
}

/// Acciones que se ofrecen para un proyecto Compose.
pub fn acciones_de_proyecto(contenedores: &[Contenedor]) -> Vec<Accion> {
    let _ = contenedores;
    Vec::new()
}

/// ¿La acción deja algo parado y por tanto exige deliberación? (§7.2)
pub fn exige_deliberacion(objetivo: &Objetivo, accion: Accion) -> bool {
    let _ = (objetivo, accion);
    false
}

/// ¿La acción exige comprobar la configuración antes? Solo recargar y
/// reiniciar (§7.5).
pub fn comprueba_configuracion(objetivo: &Objetivo, accion: Accion) -> bool {
    let _ = (objetivo, accion);
    false
}

/// ¿La acción es privilegiada (usa sudo si hace falta)?
pub fn es_privilegiada(accion: Accion) -> bool {
    let _ = accion;
    true
}

/// ¿El nombre coincide con alguno de los patrones de unidades críticas?
pub fn es_critica(patrones: &[String], nombre: &str) -> bool {
    let _ = (patrones, nombre);
    false
}

/// Motivo por el que una acción no se ofrece, para el aviso de la tecla
/// («nginx.service ya está parado», «nginx.service no admite recargar»…).
pub fn motivo_no_permitida(
    objetivo: &Objetivo,
    accion: Accion,
    permitidas: &[Accion],
) -> Option<String> {
    let _ = (objetivo, accion, permitidas);
    None
}

/// Estado de la unidad como texto corto («activo», «inactivo», «fallido»,
/// «cambiando»). El glifo lo pone la vista.
pub fn texto_estado_unidad(unidad: &Unidad) -> &'static str {
    let _ = unidad;
    "desconocido"
}

/// Estado de un contenedor como texto corto («en marcha», «parado»…) con el
/// código de salida si lo hay.
pub fn texto_estado_contenedor(contenedor: &Contenedor) -> String {
    let _ = contenedor;
    String::new()
}

/// ¿El estado de la unidad es final (active, inactive o failed)?
pub fn estado_unidad_final(activo: &str) -> bool {
    let _ = activo;
    false
}

/// ¿La acción ha terminado bien según el estado final? (§7.4)
pub fn exito(
    objetivo: &Objetivo,
    accion: Accion,
    unidad: Option<&Unidad>,
    contenedor: Option<&Contenedor>,
    proyecto: &[Contenedor],
) -> bool {
    let _ = (objetivo, accion, unidad, contenedor, proyecto);
    false
}

/// ¿Un timer lanza un servicio que ya está activo? (para no ofrecer `A`).
pub fn timer_servicio_activo(activo: &str) -> bool {
    let _ = activo;
    false
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
    let _ = unidad;
    false
}

/// ¿La unidad existe para systemd (no es un nombre inventado)?
pub fn unidad_cargada(unidad: &Unidad) -> bool {
    unidad.cargada()
}
