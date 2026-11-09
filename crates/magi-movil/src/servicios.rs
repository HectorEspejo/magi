//! Servicios, contenedores y logs del móvil (informe §4.2, §4.3 y §5.1): el
//! listado de un host, su consumo y detalle, las acciones con sudo y la
//! apertura de sesiones con orden. Todo lo remoto va con
//! `crates/magi-nucleo/scripts/servicios.sh` sobre la conexión del host.
//!
//! Es un esbozo del contrato de la Fase 14: las firmas y los tipos están
//! fijados, pero la lógica la implementa la pista del gestor del móvil.

use std::sync::Arc;

use crate::api::{
    pendiente, AccionMovil, Consumo, DetalleServicios, ErrorNucleo, InfoAccionMovil,
    ListadoServicios, ObjetivoMovil, OrdenPestanaMovil, ResultadoServicioMovil, ResultadoSudo,
    TodaLaFlota,
};
use crate::estado::EstadoNucleo;

/// Fallos de todos los hosts según el último sondeo del móvil, con su hora, y
/// los hosts caídos o sin sondeo. No abre conexiones (informe §5.1 y §6.4).
pub fn toda_la_flota(_estado: &EstadoNucleo) -> Result<TodaLaFlota, ErrorNucleo> {
    Err(pendiente("servicios_toda_la_flota"))
}

/// `servicios.sh listar` (10 s) con los motores de los ajustes. Con
/// `refresco = false` (lo pide el usuario) puede abrir la conexión con
/// diálogos; con `true` es automática y no dialoga (informe §5.1).
pub async fn listar(
    _estado: &Arc<EstadoNucleo>,
    _host_uuid: &str,
    _refresco: bool,
) -> Result<ListadoServicios, ErrorNucleo> {
    Err(pendiente("servicios_listar"))
}

/// CPU y memoria de los contenedores en marcha (suborden `consumo`).
pub async fn consumo(
    _estado: &Arc<EstadoNucleo>,
    _host_uuid: &str,
) -> Result<Vec<Consumo>, ErrorNucleo> {
    Err(pendiente("servicios_consumo"))
}

/// Propiedades, dependencias y texto de la unidad o de `inspect` (informe
/// §5.1 y §6.4).
pub async fn detalle(
    _estado: &Arc<EstadoNucleo>,
    _host_uuid: &str,
    _objetivo: &ObjetivoMovil,
) -> Result<DetalleServicios, ErrorNucleo> {
    Err(pendiente("servicios_detalle"))
}

/// Lo que la app necesita saber antes de confirmar o deliberar una acción
/// (informe §4.3 y §5.1).
pub fn preparar_accion(
    _estado: &EstadoNucleo,
    _host_uuid: &str,
    _objetivo: &ObjetivoMovil,
    _accion: AccionMovil,
) -> Result<InfoAccionMovil, ErrorNucleo> {
    Err(pendiente("preparar_accion"))
}

/// Ejecuta la acción, la anota y la difunde por
/// `OyenteNucleo.accion_servicio_hecha` (informe §4.3 y §5.1).
pub async fn accion(
    _estado: &Arc<EstadoNucleo>,
    _host_uuid: &str,
    _objetivo: &ObjetivoMovil,
    _accion: AccionMovil,
    _deliberacion: Option<u64>,
) -> Result<ResultadoServicioMovil, ErrorNucleo> {
    Err(pendiente("servicio_accion"))
}

/// Comprueba la contraseña de sudo (`servicios.sh sudo`): `ok`,
/// `rechazada` o `sin_sudo` (informe §5.1 y §7.5).
pub async fn sudo_comprobar(
    _estado: &Arc<EstadoNucleo>,
    _host_uuid: &str,
    _contrasena: Vec<u8>,
) -> Result<ResultadoSudo, ErrorNucleo> {
    Err(pendiente("sudo_comprobar"))
}

/// Olvida la contraseña de sudo en memoria de un host. De momento no hay
/// ninguna guardada: la guardará la pista del gestor del móvil.
pub fn sudo_olvidar(_estado: &EstadoNucleo, _host_uuid: &str) -> Result<(), ErrorNucleo> {
    Ok(())
}

/// Abre una sesión con PTY para una `OrdenPestana` del núcleo (informe §5.1).
pub async fn abrir_sesion_con_orden(
    _estado: &Arc<EstadoNucleo>,
    _host_uuid: &str,
    _orden: OrdenPestanaMovil,
) -> Result<u64, ErrorNucleo> {
    Err(pendiente("abrir_sesion_con_orden"))
}
