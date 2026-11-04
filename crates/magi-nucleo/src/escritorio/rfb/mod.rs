//! Cliente RFB propio (pista VNC, RFC 6143): versiones 3.3, 3.7 y 3.8,
//! seguridad None y VNC Authentication, codificaciones Raw, CopyRect, Hextile y
//! ZRLE, seudocodificaciones DesktopSize, ExtendedDesktopSize y Cursor, entrada
//! por keysyms (y teclas de QEMU) y portapapeles de texto.
//!
//! Toda entrada del servidor es no fiable: escritorio de hasta 8192×8192,
//! rectángulos dentro del escritorio, ZRLE acotado, cursor de hasta 256×256 y
//! texto de hasta 1 MB; nunca un pánico.

use crate::escritorio::{Conexion, ErrorEscritorio};

/// Abre una sesión VNC y corre hasta que termina.
pub async fn abrir(_conexion: Conexion) -> Result<(), ErrorEscritorio> {
    Err(ErrorEscritorio::Protocolo(
        "el cliente VNC todavía no está implementado".to_string(),
    ))
}
