//! Fase 7: redimensionado adaptable. Reproducciones de los dos fallos (R36),
//! tubería de tamaño del cliente y tamaño del PTY remoto de extremo a extremo.

#![allow(dead_code)]

mod comun;

#[path = "redimensionado/arnes.rs"]
mod arnes;
#[path = "redimensionado/barrido.rs"]
mod barrido;
#[path = "redimensionado/extremo.rs"]
mod extremo;
#[path = "redimensionado/tuberia.rs"]
mod tuberia;
#[path = "redimensionado/vistas_a.rs"]
mod vistas_a;
#[path = "redimensionado/vistas_b.rs"]
mod vistas_b;
#[path = "redimensionado/vistas_c.rs"]
mod vistas_c;
#[path = "redimensionado/vistas_d.rs"]
mod vistas_d;
#[path = "redimensionado/vistas_e.rs"]
mod vistas_e;
#[path = "redimensionado/vistas_f.rs"]
mod vistas_f;
#[path = "redimensionado/vistas_g.rs"]
mod vistas_g;
// Fase 8: edición, permisos, sincronizar y guardadas.
#[path = "redimensionado/vistas_h.rs"]
mod vistas_h;
#[path = "redimensionado/vistas_i.rs"]
mod vistas_i;
#[path = "redimensionado/vistas_j.rs"]
mod vistas_j;
#[path = "redimensionado/vistas_k.rs"]
mod vistas_k;

#[path = "redimensionado/remoto.rs"]
mod remoto;
#[path = "redimensionado/revision.rs"]
mod revision;
#[path = "redimensionado/secuencias.rs"]
mod secuencias;
#[path = "redimensionado/semilla.rs"]
mod semilla;
