//! Fase 7: redimensionado adaptable. Reproducciones de los dos fallos (R36),
//! tubería de tamaño del cliente y tamaño del PTY remoto de extremo a extremo.

mod comun;

#[path = "redimensionado/arnes.rs"]
mod arnes;
#[path = "redimensionado/extremo.rs"]
mod extremo;
#[path = "redimensionado/tuberia.rs"]
mod tuberia;

#[path = "redimensionado/remoto.rs"]
mod remoto;
