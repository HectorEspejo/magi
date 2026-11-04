//! Cliente RDP con IronRDP (pista RDP): conexión, TLS y huella del
//! certificado, NLA, gráficos (mapas de bits, RemoteFX y AVC420), entrada,
//! portapapeles CLIPRDR y Display Control.
//!
//! La API pública la fija el contrato de escritorio: se llama a abrir con un
//! flujo ya abierto (TCP directo o canal direct-tcpip del pool SSH) y la tarea
//! corre hasta que la sesión se cae o se cierra.

use crate::escritorio::{Conexion, ErrorEscritorio};

/// Abre una sesión RDP y corre hasta que termina. Devuelve el motivo cuando la
/// conexión no llega a abrirse o se cae.
pub async fn abrir(_conexion: Conexion) -> Result<(), ErrorEscritorio> {
    Err(ErrorEscritorio::Protocolo(
        "el cliente RDP todavía no está implementado".to_string(),
    ))
}
