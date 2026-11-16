//! Pruebas del servidor de las sesiones persistentes (Fase 16): la pestaña
//! persistente (crear, enganchar, soltada, reconexión sin quitarla a otro
//! equipo, historial y terminar) y el vigilante (comprobación sin diálogos,
//! `SesionesPersistentes{lista}`, consulta agrupada, vista previa y renombrar).
//!
//! Todo con el servidor SSH de pruebas de `tests/comun` (sin red externa) y el
//! tmux falso de `tests/fijos/terminal`. Cada pista tiene su módulo:
//! `pestana` y `vigilante`.

mod comun;
#[path = "sesiones_persistentes_servidor/pestana.rs"]
mod pestana;
#[path = "sesiones_persistentes_servidor/vigilante.rs"]
mod vigilante;
