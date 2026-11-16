//! Pruebas de la interfaz de las sesiones persistentes (Fase 16) con
//! `App::de_prueba` (sin hilos, red ni servidor) sobre `TestBackend`, con sus
//! instantáneas a 40×12, 80×24 y 200×60 (T45). Las de pintado corren con
//! `TZ=UTC`.
//!
//! Cada pista del Sprint 3 tiene su módulo (CONTRATO-S3):
//! - `lista`: bloque «Persistentes», vista previa, diálogos y acciones (pista A);
//! - `ficha`: bloque SESIONES de la ficha del host (pista A2);
//! - `pestana`: pestaña persistente y soltada, barra de la pestaña, aviso de la
//!   barra global, historial de tmux y REGISTRO (pista B).

#[path = "sesiones_persistentes_ui/ficha.rs"]
mod ficha;
#[path = "sesiones_persistentes_ui/lista.rs"]
mod lista;
#[path = "sesiones_persistentes_ui/pestana.rs"]
mod pestana;
