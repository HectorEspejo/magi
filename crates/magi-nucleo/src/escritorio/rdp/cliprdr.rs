//! Portapapeles CLIPRDR: canal estático con CF_UNICODETEXT en los dos
//! sentidos.
//!
//! El backend corre DENTRO de ActiveStage::process, así que no puede bloquear
//! ni tocar la sesión: las peticiones se dejan en un canal interno y el bucle
//! de la sesión las convierte en mensajes del SVC, eventos para el puente o
//! texto para el visor. Todo texto que llega del remoto es entrada no fiable:
//! se acota a 1 MB.

use ironrdp::cliprdr::backend::CliprdrBackend;
use ironrdp::cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest,
    FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
};
use ironrdp::core::impl_as_any;
use tokio::sync::mpsc;

/// Tamaño máximo del texto que se acepta o se manda (1 MB).
pub(crate) const TEXTO_MAX: usize = 1024 * 1024;

/// Lo que el backend pide al bucle de la sesión.
#[derive(Debug)]
pub(crate) enum AccionPortapapeles {
    /// El canal está listo.
    Listo,
    /// El remoto pide la lista de formatos local durante la inicialización:
    /// hay que anunciarla para que el canal quede listo.
    SolicitanFormatos,
    /// El remoto ha copiado algo con texto: hay que pedirle el contenido.
    TextoRemotoDisponible,
    /// El remoto pide el texto del portapapeles local: hay que pedirlo al
    /// visor (EventoEscritorio::PedirPortapapeles).
    PidenTextoLocal,
    /// Texto que ha mandado el remoto como respuesta a un pegado.
    TextoRemoto(String),
}

/// Backend de CLIPRDR del cliente RDP.
#[derive(Debug)]
pub(crate) struct Portapapeles {
    acciones: mpsc::Sender<AccionPortapapeles>,
}

impl Portapapeles {
    pub(crate) fn nuevo(acciones: mpsc::Sender<AccionPortapapeles>) -> Self {
        Self { acciones }
    }

    fn avisar(&self, accion: AccionPortapapeles) {
        // Si el canal está lleno se pierde el aviso: no se bloquea la sesión
        // por el portapapeles.
        let _ = self.acciones.try_send(accion);
    }
}

impl_as_any!(Portapapeles);

impl CliprdrBackend for Portapapeles {
    fn temporary_directory(&self) -> &str {
        // Solo se usa para transferencias de ficheros, que MAGI no ofrece.
        "/tmp"
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_ready(&mut self) {
        self.avisar(AccionPortapapeles::Listo);
    }

    fn on_request_format_list(&mut self) {
        // Sin esta respuesta el canal se queda en inicialización y luego no
        // admite initiate_paste: el cliente anuncia lo que tenga (posiblemente
        // una lista vacía) al arrancar.
        self.avisar(AccionPortapapeles::SolicitanFormatos);
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        _capacidades: ClipboardGeneralCapabilityFlags,
    ) {
    }

    fn on_remote_copy(&mut self, formatos: &[ClipboardFormat]) {
        let hay_texto = formatos
            .iter()
            .any(|formato| formato.id == ClipboardFormatId::CF_UNICODETEXT);
        if hay_texto {
            self.avisar(AccionPortapapeles::TextoRemotoDisponible);
        }
    }

    fn on_format_data_request(&mut self, peticion: FormatDataRequest) {
        if peticion.format == ClipboardFormatId::CF_UNICODETEXT {
            self.avisar(AccionPortapapeles::PidenTextoLocal);
        }
    }

    fn on_format_data_response(&mut self, respuesta: FormatDataResponse<'_>) {
        match respuesta.to_unicode_string() {
            Ok(texto) if texto.len() <= TEXTO_MAX => {
                self.avisar(AccionPortapapeles::TextoRemoto(texto));
            }
            Ok(_) => {
                tracing::debug!("el remoto ha mandado un portapapeles de más de 1 MB: se ignora");
            }
            Err(error) => {
                tracing::debug!(%error, "el remoto ha mandado un portapapeles ilegible");
            }
        }
    }

    fn on_file_contents_request(&mut self, _peticion: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _respuesta: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _id: LockDataId) {}

    fn on_unlock(&mut self, _id: LockDataId) {}
}

/// Formato único que habla MAGI.
pub(crate) fn formato_texto() -> ClipboardFormat {
    ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_remoto_con_texto_avisa_y_sin_texto_no() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut backend = Portapapeles::nuevo(tx);

        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_DIB)]);
        assert!(rx.try_recv().is_err());

        backend.on_remote_copy(&[formato_texto()]);
        assert!(matches!(
            rx.try_recv(),
            Ok(AccionPortapapeles::TextoRemotoDisponible)
        ));
    }

    #[test]
    fn el_texto_remoto_y_las_peticiones_se_avisan() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut backend = Portapapeles::nuevo(tx);

        backend.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_UNICODETEXT,
        });
        assert!(matches!(
            rx.try_recv(),
            Ok(AccionPortapapeles::PidenTextoLocal)
        ));

        backend.on_format_data_response(FormatDataResponse::new_unicode_string("hola"));
        match rx.try_recv() {
            Ok(AccionPortapapeles::TextoRemoto(texto)) => assert_eq!(texto, "hola"),
            otro => panic!("acción inesperada: {otro:?}"),
        }
    }
}
