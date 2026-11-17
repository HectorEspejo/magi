//! Dobles de las órdenes de Termux (`termux-*`) para las pruebas (Fase 17).
//!
//! Cada doble es un script de `sh` que anota sus argumentos y su entrada en un
//! fichero y que sabe colgarse o fallar según lo que se programe. Se incluye
//! con `#[path]` desde los tests de `magi-ssh`, como los dobles de servicios.
//!
//! CONTRATO-S1-TERMUX-DOBLES: el cuerpo lo completa la pista P2 del Sprint 1
//! (los guiones, `Dobles::nuevos`, `programar` y `llamadas`).
