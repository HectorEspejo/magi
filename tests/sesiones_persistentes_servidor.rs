//! Pruebas del servidor de las sesiones persistentes (Fase 16): la pestaña
//! persistente (crear, enganchar, soltada, reconexión sin quitarla a otro
//! equipo, historial y terminar) y el vigilante (comprobación sin diálogos,
//! `SesionesPersistentes{lista}`, consulta agrupada, vista previa y renombrar).
//!
//! Todo con el servidor SSH de pruebas de `tests/comun` (sin red externa) y el
//! tmux falso de `tests/fijos/terminal`. Cada pista del Sprint 2 tiene su
//! módulo: `pestana` (pista A) aquí, y `vigilante` (pista B) en
//! `sesiones_persistentes_vigilante.rs` mientras se desarrollan por separado
//! (CONTRATO-S2: al integrarlas, `vigilante` pasa a colgar de este fichero).

mod comun;
#[path = "sesiones_persistentes_servidor/pestana.rs"]
mod pestana;
