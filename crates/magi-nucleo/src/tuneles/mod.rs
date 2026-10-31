//! Mecánica de los túneles compartida por el escritorio y el móvil: local,
//! remoto y dinámico (SOCKS5 solo CONNECT), con sus escuchas en loopback.
//!
//! El CRUD de TUNELES lo hace la interfaz; aquí solo vive el motor.

pub mod escucha;
pub mod local;
pub mod socks5;
