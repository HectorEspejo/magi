//! Cliente RDP con IronRDP (Fase 12): conexión, TLS y huella del
//! certificado, NLA, gráficos (mapas de bits, RemoteFX y AVC420), entrada,
//! portapapeles CLIPRDR y Display Control.
//!
//! La API pública la fija el contrato de escritorio: se llama a abrir con un
//! flujo ya abierto (TCP directo o canal direct-tcpip del pool SSH) y la tarea
//! corre hasta que la sesión se cae o se cierra.
//!
//! IronRDP está fijado con = (0.17) y el TLS va con rustls y aws-lc-rs, sin
//! OpenSSL. La huella del certificado se comprueba antes de mandar ninguna
//! credencial. Nada de lo que se ve o se teclea va al log ni a REGISTRO.

mod certificado;
mod cliprdr;
mod conexion;
mod egfx;
mod entrada;
mod sesion;
mod tls;
mod transporte;

use crate::escritorio::{Conexion, ErrorEscritorio, EventoEscritorio};

/// Abre una sesión RDP y corre hasta que termina. Devuelve el motivo cuando la
/// conexión no llega a abrirse o se cae.
///
/// Si la autenticación falla devuelve ErrorEscritorio::Autenticacion: la capa
/// de sesión reabre el camino y vuelve a llamar con intento + 1. Aquí no hay
/// bucle de reconexión.
pub async fn abrir(conexion: Conexion) -> Result<(), ErrorEscritorio> {
    let Conexion {
        flujo,
        config,
        dialogo,
        eventos,
        ordenes,
        intento,
    } = conexion;

    let resultado =
        match conexion::conectar(flujo, &config, &dialogo, eventos.clone(), intento).await {
            Ok(conectada) => sesion::ejecutar(conectada, config, eventos.clone(), ordenes).await,
            Err(error) => Err(error),
        };

    // El puente deja de esperar en cuanto ve Fin, también cuando la apertura
    // falla: la capa de sesión ya habrá contado el motivo.
    let _ = eventos.send(EventoEscritorio::Fin).await;

    resultado
}
