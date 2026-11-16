//! Nombres de las sesiones persistentes (§7.1 del informe de la Fase 16): el
//! automático `<etiqueta del equipo>-<n>` con el primer `n` libre y el que
//! propone el diálogo (el de la carpeta o `sesion-<n>`). Pista B del Sprint 1.
//!
//! En el tmux propio los nombres ya no llevan el prefijo `magi-` (D340): el
//! servidor entero es de MAGI. Los nombres de las fichas terminadas siguen
//! reservados, porque la ficha sigue ahí hasta que el usuario la borra.

use crate::sesiones_persistentes::validadores::MAXIMO_NOMBRE;
use crate::terminal::tmux::etiqueta_equipo;

/// Etiqueta de los nombres que no salen de una carpeta ni de un equipo.
const ETIQUETA_SIN_CARPETA: &str = "sesion";

/// Nombre automático de una sesión nueva en un host marcado:
/// `<etiqueta>-<n>` con el primer `n` (desde 1) que no esté entre las
/// sesiones del host (`en_host`) ni entre sus fichas (`en_fichas`, que
/// incluye las terminadas). La etiqueta pasa por `etiqueta_equipo` (minúsculas,
/// sin tildes y hasta 20 caracteres), así que el resultado es siempre un
/// nombre válido; una etiqueta que ya lo era no cambia.
#[must_use]
pub fn nombre_automatico(etiqueta: &str, en_host: &[String], en_fichas: &[String]) -> String {
    let etiqueta = etiqueta_equipo(etiqueta);
    let mut numero: u64 = 1;
    loop {
        let candidato = format!("{etiqueta}-{numero}");
        let usado = en_host.contains(&candidato) || en_fichas.contains(&candidato);
        if !usado {
            return candidato;
        }
        numero += 1;
    }
}

/// Nombre que propone el diálogo de una sesión nueva: el de la última
/// carpeta de la ruta (`~/proyectos/api` → `api`) si es utilizable y no lo
/// tiene ya otra ficha del host; si no, `sesion-<n>`. Una carpeta vacía
/// propone `sesion-<n>`.
#[must_use]
pub fn nombre_propuesto(carpeta: &str, en_fichas: &[String]) -> String {
    let base = nombre_de_carpeta(carpeta);
    if !base.is_empty() && !en_fichas.contains(&base) {
        return base;
    }
    nombre_automatico(ETIQUETA_SIN_CARPETA, &[], en_fichas)
}

/// Última componente de la ruta, con todo lo que no sea `[A-Za-z0-9_-]`
/// sustituido por un guion (los grupos seguidos, por uno solo), sin guiones en
/// los extremos y de hasta 64 caracteres. Vacío si no queda nada.
fn nombre_de_carpeta(carpeta: &str) -> String {
    let ultima = carpeta
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let mut base = String::with_capacity(ultima.len());
    let mut ultimo_guion_sustituido = false;
    for caracter in ultima.chars() {
        if caracter.is_ascii_alphanumeric() || caracter == '_' || caracter == '-' {
            base.push(caracter);
            ultimo_guion_sustituido = false;
        } else if !ultimo_guion_sustituido {
            base.push('-');
            ultimo_guion_sustituido = true;
        }
    }
    // Todo ASCII: recortar por caracteres es recortar por bytes.
    base.trim_matches('-').chars().take(MAXIMO_NOMBRE).collect()
}
