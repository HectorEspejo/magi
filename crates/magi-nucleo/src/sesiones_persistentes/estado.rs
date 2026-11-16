//! Estado derivado de cada ficha (§7.3 del informe de la Fase 16):
//! `viva_libre`, `viva_enganchada`, `terminada`, `sin_comprobar`,
//! `inalcanzable` o `sin_tmux`, más atención, título y «antigua». Pista B del
//! Sprint 1.
//!
//! Todo se deriva de la última comprobación del host (`estado` del script):
//! el estado no se guarda en ningún sitio y se pierde al apagarse el servidor.
//! La regla que manda: una ficha solo es `terminada` si el host respondió, tiene
//! tmux y la sesión no sale entre las del servidor propio de MAGI. Si el host no
//! responde, la ficha es `inalcanzable`, nunca `terminada`.

use crate::sesiones_persistentes::{
    ComprobacionHost, EstadoFicha, Ficha, ServidorTmux, SesionVista, VersionTmux,
};

/// Segundos de un día, para comparar la actividad con `antigua_dias`.
const SEGUNDOS_POR_DIA: i64 = 86_400;

/// Lo que se sabe de una ficha tras la última comprobación de su host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VistaFicha {
    pub estado: EstadoFicha,
    /// Alguna ventana de la sesión tiene la campana sin mirar (D335). Solo si
    /// la sesión vive.
    pub atencion: bool,
    /// Título del programa que corre dentro (solo si vive).
    pub titulo: Option<String>,
    /// Sin actividad desde hace `antigua_dias` o más, comparada con la hora
    /// del host. Solo si vive.
    pub antigua: bool,
    /// Clientes enganchados ahora (0 si no vive).
    pub clientes: u32,
    /// Etiqueta del último equipo que se enganchó, si consta.
    pub equipo: Option<String>,
    /// Última actividad (segundos Unix, hora del host), si la sesión vive.
    pub actividad: Option<i64>,
}

/// La sesión del servidor propio de MAGI que corresponde a un nombre, por
/// nombre exacto (tmux casa prefijos en `-t`, aquí nunca). Las del tmux
/// habitual del usuario no cuentan: ninguna ficha apunta a ellas.
#[must_use]
pub fn sesion_de<'a>(nombre: &str, host: &'a ComprobacionHost) -> Option<&'a SesionVista> {
    host.sesiones
        .iter()
        .find(|sesion| sesion.servidor == ServidorTmux::Magi && sesion.nombre == nombre)
}

/// Estado de una ficha según la última comprobación de su host (`None` si aún
/// no se ha comprobado). Es la lógica de §7.3, en este orden:
///
/// 1. Sin comprobación (o con la de otro host): `sin_comprobar`.
/// 2. El host no se pudo mirar (sin respuesta, diálogo pendiente o plazo):
///    `inalcanzable`, nunca `terminada`.
/// 3. Sin tmux o anterior a 3.0: `sin_tmux`.
/// 4. Ausente de las sesiones del servidor propio: `terminada`.
/// 5. Con clientes enganchados: `viva_enganchada`; si no, `viva_libre`.
#[must_use]
pub fn estado_visto(ficha: &Ficha, host: Option<&ComprobacionHost>) -> EstadoFicha {
    derivar(ficha, host).0
}

/// Estado, atención, título y antigüedad de una ficha. `antigua_dias` es el
/// de `[persistentes] antigua_dias` ya acotado.
#[must_use]
pub fn vista_de_ficha(
    ficha: &Ficha,
    host: Option<&ComprobacionHost>,
    antigua_dias: u64,
) -> VistaFicha {
    let (estado, sesion) = derivar(ficha, host);
    let ahora_host = host.map_or(0, |host| host.ahora);
    VistaFicha {
        estado,
        atencion: atencion(sesion),
        titulo: sesion.and_then(|sesion| sesion.titulo.clone()),
        antigua: antigua(sesion, ahora_host, antigua_dias),
        clientes: sesion.map_or(0, |sesion| sesion.clientes),
        equipo: sesion.and_then(|sesion| sesion.equipo.clone()),
        actividad: sesion.map(|sesion| sesion.actividad),
    }
}

/// Atención: alguna ventana de la sesión con la campana sin mirar. tmux marca
/// la campana aunque nadie esté enganchado y la quita cuando un cliente la
/// mira, así que no hace falta guardar nada.
#[must_use]
pub fn atencion(sesion: Option<&SesionVista>) -> bool {
    sesion.is_some_and(|sesion| sesion.campana)
}

/// «Antigua»: la última actividad de la sesión está a `dias` días o más de la
/// hora del host (`MAGI_AHORA`, no la del equipo: los relojes no tienen por
/// qué coincidir). Una actividad futura (relojes desajustados) nunca es
/// antigua.
#[must_use]
pub fn antigua(sesion: Option<&SesionVista>, ahora_host: i64, dias: u64) -> bool {
    let Some(sesion) = sesion else {
        return false;
    };
    // Un umbral que no cabe en un i64 de segundos no lo alcanza nadie.
    let Some(umbral) = i64::try_from(dias)
        .ok()
        .and_then(|dias| dias.checked_mul(SEGUNDOS_POR_DIA))
    else {
        return false;
    };
    ahora_host.saturating_sub(sesion.actividad) >= umbral
}

/// Estado de una ficha y la sesión viva que le corresponde, si la hay.
fn derivar<'a>(
    ficha: &Ficha,
    host: Option<&'a ComprobacionHost>,
) -> (EstadoFicha, Option<&'a SesionVista>) {
    let Some(host) = host.filter(|host| host.host_id == ficha.host_id) else {
        return (EstadoFicha::SinComprobar, None);
    };
    if host.error.is_some() {
        return (EstadoFicha::Inalcanzable, None);
    }
    if !host
        .tmux
        .is_some_and(|version| version >= VersionTmux::MINIMA)
    {
        return (EstadoFicha::SinTmux, None);
    }
    match sesion_de(&ficha.nombre, host) {
        None => (EstadoFicha::Terminada, None),
        Some(sesion) if sesion.clientes > 0 => (EstadoFicha::VivaEnganchada, Some(sesion)),
        Some(sesion) => (EstadoFicha::VivaLibre, Some(sesion)),
    }
}
