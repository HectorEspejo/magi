//! Bucle de la sesión RDP activa: gráficos, puntero, tamaño, entrada,
//! portapapeles, reactivación y cierre.
//!
//! Todo lo que llega del servidor es entrada no fiable: los rectángulos se
//! comprueban contra el tamaño de la imagen, el cursor se acota a 256×256 y el
//! texto a 1 MB. Nunca se hace unwrap de datos del servidor.

use std::sync::Arc;
use std::time::Duration;

use ironrdp::cliprdr::pdu::{ClipboardFormat, ClipboardFormatId, FormatDataResponse};
use ironrdp::cliprdr::CliprdrClient;
use ironrdp::connector::connection_activation::{
    ConnectionActivationFactory, ConnectionActivationState,
};
use ironrdp::core::WriteBuf;
use ironrdp::graphics::image_processing::PixelFormat;
use ironrdp::graphics::pointer::DecodedPointer;
use ironrdp::pdu::geometry::{InclusiveRectangle, Rectangle as _};
use ironrdp::pdu::Action;
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{fast_path, ActiveStage, ActiveStageBuilder, ActiveStageOutput};
use ironrdp_tokio::{single_sequence_step, FramedWrite as _};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::cliprdr::{formato_texto, AccionPortapapeles, TEXTO_MAX};
use super::conexion::{SesionConectada, PLAZO_RED};
use super::entrada::Entrada;
use super::transporte::FlujoFramed;
use crate::escritorio::{
    ConfigEscritorio, ErrorEscritorio, EventoEscritorio, ImagenEscritorio, OrdenEscritorio,
};

/// Agrupación de los cambios de tamaño pedidos por el visor.
const AGRUPAR_TAMANO: Duration = Duration::from_millis(500);
/// Plazo para esperar el cierre ordenado del servidor.
const PLAZO_CIERRE: Duration = Duration::from_secs(2);
/// Lado máximo del cursor (entrada no fiable).
const CURSOR_MAX: u16 = 256;
/// Lado máximo del escritorio.
const LADO_MAX: u16 = 8192;
/// Tamaño mínimo que admite un servidor RDP.
const LADO_MIN: u16 = 200;

/// Lo que decide el bucle tras procesar una salida.
#[derive(Debug, PartialEq, Eq)]
enum Control {
    Seguir,
    Fin,
}

/// Una acción que ha completado el select del bucle.
enum Accion {
    Orden(Option<OrdenEscritorio>),
    Leido(std::io::Result<(Action, Vec<u8>)>),
    Tamano,
}

/// Estado del cursor remoto que se ha mandado al visor.
struct EstadoCursor {
    visible: bool,
    x: u16,
    y: u16,
    punto: (u16, u16),
    ancho: u16,
    alto: u16,
    pixeles: Vec<u8>,
}

impl EstadoCursor {
    fn nuevo() -> Self {
        Self {
            visible: false,
            x: 0,
            y: 0,
            punto: (0, 0),
            ancho: 0,
            alto: 0,
            pixeles: Vec::new(),
        }
    }
}

/// Sesión RDP en marcha.
pub(crate) struct Sesion {
    framed: FlujoFramed,
    etapa: ActiveStage,
    imagen: DecodedImage,
    factoria: ConnectionActivationFactory,
    config: ConfigEscritorio,
    eventos: mpsc::Sender<EventoEscritorio>,
    ordenes: mpsc::Receiver<OrdenEscritorio>,
    entrada: Entrada,
    acciones: mpsc::Receiver<AccionPortapapeles>,
    texto_local: Option<String>,
    piden_local: bool,
    /// Hay texto en el portapapeles remoto, pero el canal aún no estaba
    /// listo para pedirlo: se reintenta en la siguiente vuelta del bucle.
    pendiente_remoto: bool,
    tamano: (u16, u16),
    tamano_pendiente: Option<((u16, u16), Instant)>,
    cursor: EstadoCursor,
}

/// Corre la sesión hasta que el remoto la cierra o se cae.
pub(crate) async fn ejecutar(
    conectada: SesionConectada,
    config: ConfigEscritorio,
    eventos: mpsc::Sender<EventoEscritorio>,
    ordenes: mpsc::Receiver<OrdenEscritorio>,
) -> Result<(), ErrorEscritorio> {
    let SesionConectada {
        framed,
        resultado,
        acciones,
    } = conectada;

    let ancho = resultado.desktop_size.width.max(1);
    let alto = resultado.desktop_size.height.max(1);
    let imagen = DecodedImage::new(PixelFormat::RgbA32, ancho, alto);

    let etapa = ActiveStageBuilder {
        static_channels: resultado.static_channels,
        user_channel_id: resultado.user_channel_id,
        io_channel_id: resultado.io_channel_id,
        message_channel_id: resultado.message_channel_id,
        share_id: resultado.share_id,
        compression_type: resultado.compression_type,
        enable_server_pointer: resultado.enable_server_pointer,
        pointer_software_rendering: resultado.pointer_software_rendering,
    }
    .build();

    let mut sesion = Sesion {
        framed,
        etapa,
        imagen,
        factoria: resultado.activation_factory,
        config,
        eventos,
        ordenes,
        entrada: Entrada::nueva(),
        acciones,
        texto_local: None,
        piden_local: false,
        pendiente_remoto: false,
        tamano: (ancho, alto),
        tamano_pendiente: None,
        cursor: EstadoCursor::nuevo(),
    };

    // La sesión ya está lista: el servidor pasa el escritorio a abierto y
    // emite el enlace del visor. Va antes de los primeros píxeles.
    sesion.emitir(EventoEscritorio::Abierto).await;

    let imagen_inicial = if sesion.config.h264 {
        ImagenEscritorio::H264
    } else {
        ImagenEscritorio::Rectangulos
    };
    sesion
        .emitir(EventoEscritorio::Imagen(imagen_inicial))
        .await;
    sesion
        .emitir(EventoEscritorio::Tamano { ancho, alto })
        .await;

    sesion.bucle().await
}

impl Sesion {
    async fn bucle(&mut self) -> Result<(), ErrorEscritorio> {
        loop {
            self.atender_portapapeles().await;
            if self.pendiente_remoto {
                // El canal CLIPRDR solo está listo tras el intercambio de
                // capacidades: si el remoto copió antes, se reintenta aquí.
                self.pedir_texto_remoto().await;
            }

            let plazo = self.tamano_pendiente.map(|(_, plazo)| plazo);
            let hay_tamano = plazo.is_some();

            let accion = tokio::select! {
                orden = self.ordenes.recv() => Accion::Orden(orden),
                leido = self.framed.read_pdu() => {
                    // Se copia el PDU a un Vec: el bucle lo procesa fuera del
                    // select, donde no hay préstamos vivos de self.
                    Accion::Leido(leido.map(|(accion, carga)| (accion, carga.to_vec())))
                }
                _ = tokio::time::sleep_until(plazo.unwrap_or_else(Instant::now)), if hay_tamano => Accion::Tamano,
            };

            match accion {
                Accion::Orden(Some(orden)) => self.procesar_orden(orden).await?,
                Accion::Orden(None) => {
                    self.cerrar_ordenado().await;
                    self.emitir(EventoEscritorio::Cerrado(
                        "se ha cerrado la sesión".to_string(),
                    ))
                    .await;
                    return Ok(());
                }
                Accion::Leido(Ok((action, carga))) => {
                    if self.procesar_pdu(action, &carga).await? == Control::Fin {
                        return Ok(());
                    }
                }
                Accion::Leido(Err(error)) => {
                    let (motivo, error) = error_de_lectura(error);
                    self.emitir(EventoEscritorio::Caido(motivo)).await;
                    return Err(error);
                }
                Accion::Tamano => self.aplicar_tamano_pendiente().await?,
            }
        }
    }

    async fn procesar_pdu(
        &mut self,
        action: Action,
        carga: &[u8],
    ) -> Result<Control, ErrorEscritorio> {
        let salidas = self
            .etapa
            .process(&mut self.imagen, action, carga)
            .map_err(error_de_sesion)?;

        let mut control = Control::Seguir;
        for salida in salidas {
            if self.procesar_salida(salida).await? == Control::Fin {
                control = Control::Fin;
            }
        }

        self.atender_portapapeles().await;

        Ok(control)
    }

    async fn procesar_salida(
        &mut self,
        salida: ActiveStageOutput,
    ) -> Result<Control, ErrorEscritorio> {
        match salida {
            ActiveStageOutput::ResponseFrame(bytes) => self.escribir(&bytes).await?,
            ActiveStageOutput::GraphicsUpdate(rect) => self.enviar_rect(&rect).await?,
            ActiveStageOutput::PointerDefault => self.cursor_por_defecto().await,
            ActiveStageOutput::PointerHidden => self.cursor_oculto().await,
            ActiveStageOutput::PointerPosition { x, y } => {
                // El visor pinta el cursor en la posición del ratón local; la
                // posición remota solo se guarda para el próximo cambio de
                // forma, sin reenviar la imagen en cada movimiento.
                self.cursor.x = x;
                self.cursor.y = y;
            }
            ActiveStageOutput::PointerBitmap(pointer) => self.cursor_de_bitmap(&pointer).await,
            ActiveStageOutput::Terminate(motivo) => {
                let texto = match motivo {
                    ironrdp::session::GracefulDisconnectReason::UserInitiated => {
                        "el servidor ha cerrado la sesión".to_string()
                    }
                    ironrdp::session::GracefulDisconnectReason::ServerInitiated => {
                        "el servidor ha cerrado la sesión".to_string()
                    }
                    ironrdp::session::GracefulDisconnectReason::Other(descripcion) => {
                        format!("el servidor ha cerrado la sesión: {descripcion}")
                    }
                };
                self.emitir(EventoEscritorio::Cerrado(texto)).await;
                return Ok(Control::Fin);
            }
            ActiveStageOutput::DeactivateAll => self.reactivar().await?,
            // Sin multitransporte UDP ni telemetría de red.
            ActiveStageOutput::MultitransportRequest(_) | ActiveStageOutput::AutoDetect(_) => {}
        }

        Ok(Control::Seguir)
    }

    // ------------------------------------------------------------ gráficos

    async fn enviar_rect(&mut self, rect: &InclusiveRectangle) -> Result<(), ErrorEscritorio> {
        let ancho = u32::from(rect.width());
        let alto = u32::from(rect.height());
        if ancho == 0 || alto == 0 {
            return Ok(());
        }
        if ancho > u32::from(LADO_MAX) || alto > u32::from(LADO_MAX) {
            return Err(ErrorEscritorio::Datos(
                "el servidor RDP ha mandado una zona de más de 8192 píxeles".to_string(),
            ));
        }

        let ancho_imagen = u32::from(self.imagen.width());
        let alto_imagen = u32::from(self.imagen.height());
        if u32::from(rect.left) + ancho > ancho_imagen || u32::from(rect.top) + alto > alto_imagen {
            return Err(ErrorEscritorio::Datos(
                "el servidor RDP ha mandado una zona fuera del escritorio".to_string(),
            ));
        }

        let bytes_por_pixel = self.imagen.bytes_per_pixel();
        let stride = self.imagen.stride();
        let datos = self.imagen.data();

        let mut pixeles = Vec::with_capacity(ancho as usize * alto as usize * bytes_por_pixel);
        for fila in 0..alto as usize {
            let inicio = (rect.top as usize + fila) * stride + rect.left as usize * bytes_por_pixel;
            let fin = inicio + ancho as usize * bytes_por_pixel;
            let Some(trozo) = datos.get(inicio..fin) else {
                return Err(ErrorEscritorio::Datos(
                    "el búfer del escritorio RDP no cubre la zona mandada".to_string(),
                ));
            };
            pixeles.extend_from_slice(trozo);
        }

        self.emitir(EventoEscritorio::Rect {
            x: rect.left,
            y: rect.top,
            ancho: ancho as u16,
            alto: alto as u16,
            pixeles,
        })
        .await;

        Ok(())
    }

    // ------------------------------------------------------------ cursor

    async fn cursor_de_bitmap(&mut self, pointer: &Arc<DecodedPointer>) {
        // Un puntero de 0×0 es el cursor oculto (IronRDP lo decodifica así).
        if pointer.width == 0 || pointer.height == 0 {
            self.cursor_oculto().await;
            return;
        }
        if pointer.width > CURSOR_MAX || pointer.height > CURSOR_MAX {
            return;
        }
        let esperado = usize::from(pointer.width) * usize::from(pointer.height) * 4;
        if pointer.bitmap_data.len() != esperado {
            return;
        }

        self.cursor = EstadoCursor {
            visible: true,
            x: self.cursor.x,
            y: self.cursor.y,
            punto: (pointer.hotspot_x, pointer.hotspot_y),
            ancho: pointer.width,
            alto: pointer.height,
            pixeles: pointer.bitmap_data.clone(),
        };
        self.emitir_cursor().await;
    }

    async fn cursor_por_defecto(&mut self) {
        let (ancho, alto, pixeles) = flecha_por_defecto();
        self.cursor = EstadoCursor {
            visible: true,
            x: self.cursor.x,
            y: self.cursor.y,
            punto: (0, 0),
            ancho,
            alto,
            pixeles,
        };
        self.emitir_cursor().await;
    }

    async fn cursor_oculto(&mut self) {
        self.cursor = EstadoCursor {
            visible: false,
            x: self.cursor.x,
            y: self.cursor.y,
            punto: (0, 0),
            ancho: 0,
            alto: 0,
            pixeles: Vec::new(),
        };
        self.emitir_cursor().await;
    }

    async fn emitir_cursor(&self) {
        self.emitir(EventoEscritorio::Cursor {
            visible: self.cursor.visible,
            x: self.cursor.x,
            y: self.cursor.y,
            punto: self.cursor.punto,
            ancho: self.cursor.ancho,
            alto: self.cursor.alto,
            pixeles: self.cursor.pixeles.clone(),
        })
        .await;
    }

    // ------------------------------------------------------------ tamaño

    async fn aplicar_tamano_pendiente(&mut self) -> Result<(), ErrorEscritorio> {
        let Some(((ancho, alto), _)) = self.tamano_pendiente.take() else {
            return Ok(());
        };

        match self
            .etapa
            .encode_resize(u32::from(ancho), u32::from(alto), Some(100), None)
        {
            Some(Ok(bytes)) => self.escribir(&bytes).await?,
            Some(Err(error)) => {
                tracing::debug!(%error, "no se pudo pedir el tamaño del escritorio");
            }
            None => {
                tracing::debug!("Display Control no disponible: el visor escala");
            }
        }

        Ok(())
    }

    /// Reactivación (MS-RDPBCGR 1.3.1.3): en IronRDP 0.17 la conduce el
    /// consumidor al recibir DeactivateAll. Es el único sitio donde el
    /// escritorio puede cambiar de tamaño por su cuenta.
    async fn reactivar(&mut self) -> Result<(), ErrorEscritorio> {
        let mut secuencia = self.factoria.create();
        let mut buffer = WriteBuf::new();

        loop {
            match tokio::time::timeout(
                PLAZO_RED,
                single_sequence_step(&mut self.framed, &mut secuencia, &mut buffer),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let motivo = format!("ha fallado la reactivación del escritorio: {error}");
                    self.emitir(EventoEscritorio::Caido(motivo.clone())).await;
                    return Err(ErrorEscritorio::Conexion(motivo));
                }
                Err(_) => {
                    let motivo =
                        "el servidor RDP no ha terminado la reactivación en 10 s".to_string();
                    self.emitir(EventoEscritorio::Caido(motivo.clone())).await;
                    return Err(ErrorEscritorio::Conexion(motivo));
                }
            }

            if let ConnectionActivationState::Finalized {
                desktop_size,
                share_id,
                enable_server_pointer,
                pointer_software_rendering,
            } = secuencia.connection_activation_state()
            {
                let ancho = desktop_size.width.max(1);
                let alto = desktop_size.height.max(1);

                self.imagen = DecodedImage::new(PixelFormat::RgbA32, ancho, alto);
                self.etapa.set_fastpath_processor(
                    fast_path::ProcessorBuilder {
                        io_channel_id: secuencia.io_channel_id(),
                        user_channel_id: secuencia.user_channel_id(),
                        share_id,
                        enable_server_pointer,
                        pointer_software_rendering,
                        bulk_decompressor: None,
                    }
                    .build(),
                );
                self.etapa.set_share_id(share_id);
                self.etapa.set_enable_server_pointer(enable_server_pointer);

                if (ancho, alto) != self.tamano {
                    self.tamano = (ancho, alto);
                    self.emitir(EventoEscritorio::Tamano { ancho, alto }).await;
                }

                return Ok(());
            }
        }
    }

    // ------------------------------------------------------------ entrada

    async fn procesar_orden(&mut self, orden: OrdenEscritorio) -> Result<(), ErrorEscritorio> {
        match orden {
            OrdenEscritorio::Tecla {
                codigo,
                keysym,
                abajo,
            } => {
                let operaciones = self.entrada.tecla(&codigo, keysym, abajo);
                self.enviar_operaciones(operaciones).await?;
            }
            OrdenEscritorio::Raton { x, y } => {
                let (x, y) = self.acotar_raton(x, y);
                let operaciones = self.entrada.raton(x, y);
                self.enviar_operaciones(operaciones).await?;
            }
            OrdenEscritorio::Boton { boton, abajo } => {
                let operaciones = self.entrada.boton(boton, abajo);
                self.enviar_operaciones(operaciones).await?;
            }
            OrdenEscritorio::Rueda { dx, dy } => {
                let operaciones = self.entrada.rueda(dx, dy);
                self.enviar_operaciones(operaciones).await?;
            }
            OrdenEscritorio::SoltarTodo => {
                let eventos = self.entrada.soltar_todo();
                self.enviar_eventos(eventos).await?;
            }
            OrdenEscritorio::Tamano { ancho, alto } => {
                let tamano = tamano_valido(ancho, alto);
                let pendiente = self.tamano_pendiente.map(|(tamano, _)| tamano);
                if tamano != self.tamano && Some(tamano) != pendiente {
                    self.tamano_pendiente = Some((tamano, Instant::now() + AGRUPAR_TAMANO));
                }
            }
            OrdenEscritorio::CtrlAltSupr => {
                let operaciones = self.entrada.ctrl_alt_supr();
                self.enviar_operaciones(operaciones).await?;
            }
            OrdenEscritorio::TeclaWindows => {
                let operaciones = self.entrada.tecla_windows();
                self.enviar_operaciones(operaciones).await?;
            }
            OrdenEscritorio::Portapapeles(texto) => self.recibir_texto_local(texto).await?,
            OrdenEscritorio::PedirPortapapeles => self.pedir_texto_remoto().await,
            // El puente decide pausar la imagen; la sesión sigue viva. La
            // reconexión la lleva la capa de sesión, no este cliente.
            OrdenEscritorio::Visible(_) | OrdenEscritorio::Reconectar => {}
        }

        Ok(())
    }

    fn acotar_raton(&self, x: u16, y: u16) -> (u16, u16) {
        let ancho = self.tamano.0.max(1);
        let alto = self.tamano.1.max(1);
        (x.min(ancho - 1), y.min(alto - 1))
    }

    async fn enviar_operaciones(
        &mut self,
        operaciones: Vec<ironrdp::input::Operation>,
    ) -> Result<(), ErrorEscritorio> {
        let eventos = self.entrada.aplicar(operaciones);
        self.enviar_eventos(eventos).await
    }

    async fn enviar_eventos(
        &mut self,
        eventos: Vec<ironrdp::pdu::input::fast_path::FastPathInputEvent>,
    ) -> Result<(), ErrorEscritorio> {
        if eventos.is_empty() {
            return Ok(());
        }

        let salidas = self
            .etapa
            .process_fastpath_input(&mut self.imagen, &eventos)
            .map_err(error_de_sesion)?;

        for salida in salidas {
            match salida {
                ActiveStageOutput::ResponseFrame(bytes) => self.escribir(&bytes).await?,
                ActiveStageOutput::GraphicsUpdate(rect) => self.enviar_rect(&rect).await?,
                _ => {}
            }
        }

        self.atender_portapapeles().await;

        Ok(())
    }

    // ------------------------------------------------------------ portapapeles

    async fn atender_portapapeles(&mut self) {
        loop {
            match self.acciones.try_recv() {
                Ok(AccionPortapapeles::Listo) => {}
                Ok(AccionPortapapeles::SolicitanFormatos) => {
                    let formatos = if self.texto_local.is_some() {
                        vec![formato_texto()]
                    } else {
                        Vec::new()
                    };
                    self.anunciar_formatos(&formatos).await;
                }
                Ok(AccionPortapapeles::TextoRemotoDisponible) => {
                    self.pedir_texto_remoto().await;
                }
                Ok(AccionPortapapeles::PidenTextoLocal) => {
                    if let Some(texto) = self.texto_local.clone() {
                        self.enviar_texto_portapapeles(texto).await;
                    } else {
                        self.piden_local = true;
                        self.emitir(EventoEscritorio::PedirPortapapeles).await;
                    }
                }
                Ok(AccionPortapapeles::TextoRemoto(texto)) => {
                    self.emitir(EventoEscritorio::Portapapeles(texto)).await;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
    }

    async fn recibir_texto_local(&mut self, texto: String) -> Result<(), ErrorEscritorio> {
        if texto.len() > TEXTO_MAX {
            tracing::debug!("el visor ha mandado un portapapeles de más de 1 MB: se ignora");
            return Ok(());
        }

        self.texto_local = Some(texto.clone());
        self.anunciar_texto_local().await;

        if self.piden_local {
            self.piden_local = false;
            self.enviar_texto_portapapeles(texto).await;
        }

        Ok(())
    }

    async fn anunciar_texto_local(&mut self) {
        self.anunciar_formatos(&[formato_texto()]).await;
    }

    /// Anuncia al remoto la lista de formatos del portapapeles local.
    async fn anunciar_formatos(&mut self, formatos: &[ClipboardFormat]) {
        let Some(cliprdr) = self.etapa.get_svc_processor_mut::<CliprdrClient>() else {
            return;
        };
        let mensajes = match cliprdr.initiate_copy(formatos) {
            Ok(mensajes) => mensajes,
            Err(error) => {
                tracing::debug!(%error, "no se pudo anunciar el portapapeles");
                return;
            }
        };
        if let Ok(bytes) = self.etapa.process_svc_processor_messages(mensajes) {
            let _ = self.escribir(&bytes).await;
        }
    }

    async fn pedir_texto_remoto(&mut self) {
        self.pendiente_remoto = false;
        let Some(cliprdr) = self.etapa.get_svc_processor_mut::<CliprdrClient>() else {
            return;
        };
        let mensajes = match cliprdr.initiate_paste(ClipboardFormatId::CF_UNICODETEXT) {
            Ok(mensajes) => mensajes,
            Err(error) => {
                tracing::debug!(%error, "el portapapeles remoto aún no está listo: se reintenta");
                self.pendiente_remoto = true;
                return;
            }
        };
        if let Ok(bytes) = self.etapa.process_svc_processor_messages(mensajes) {
            let _ = self.escribir(&bytes).await;
        }
    }

    async fn enviar_texto_portapapeles(&mut self, texto: String) {
        let Some(cliprdr) = self.etapa.get_svc_processor_mut::<CliprdrClient>() else {
            return;
        };
        let respuesta = FormatDataResponse::new_unicode_string(&texto);
        let mensajes = match cliprdr.submit_format_data(respuesta) {
            Ok(mensajes) => mensajes,
            Err(error) => {
                tracing::debug!(%error, "no se pudo mandar el portapapeles");
                return;
            }
        };
        if let Ok(bytes) = self.etapa.process_svc_processor_messages(mensajes) {
            let _ = self.escribir(&bytes).await;
        }
    }

    // ------------------------------------------------------------ salida

    async fn escribir(&mut self, bytes: &[u8]) -> Result<(), ErrorEscritorio> {
        if let Err(error) = self.framed.write_all(bytes).await {
            let motivo = "se ha perdido la conexión con el escritorio".to_string();
            self.emitir(EventoEscritorio::Caido(motivo.clone())).await;
            return Err(ErrorEscritorio::Conexion(format!("{motivo}: {error}")));
        }
        Ok(())
    }

    async fn emitir(&self, evento: EventoEscritorio) {
        // Si el puente ya no escucha, la sesión sigue hasta que se cierre el
        // canal de órdenes: no se convierte en un pánico ni en una espera.
        let _ = self.eventos.send(evento).await;
    }

    /// Suelta teclas y botones y manda el cierre ordenado, sin quedarse colgado
    /// si el servidor no contesta.
    async fn cerrar_ordenado(&mut self) {
        let eventos = self.entrada.soltar_todo();
        let _ = tokio::time::timeout(Duration::from_secs(1), self.enviar_eventos(eventos)).await;

        let salidas = match self.etapa.graceful_shutdown() {
            Ok(salidas) => salidas,
            Err(_) => return,
        };
        for salida in salidas {
            if let ActiveStageOutput::ResponseFrame(bytes) = salida {
                if self.escribir(&bytes).await.is_err() {
                    return;
                }
            }
        }

        let Sesion {
            framed,
            etapa,
            imagen,
            ..
        } = self;
        let _ = tokio::time::timeout(PLAZO_CIERRE, async {
            loop {
                let Ok((action, carga)) = framed.read_pdu().await else {
                    break;
                };
                match etapa.process(imagen, action, &carga) {
                    Ok(salidas) => {
                        if salidas
                            .iter()
                            .any(|salida| matches!(salida, ActiveStageOutput::Terminate(_)))
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .await;
    }
}

/// Traduce un error de la sesión de IronRDP a datos inválidos.
fn error_de_sesion(error: ironrdp::session::SessionError) -> ErrorEscritorio {
    ErrorEscritorio::Datos(format!("datos inválidos del servidor RDP: {error}"))
}

/// Clasifica un error de lectura: caída de red o cierre del remoto.
fn error_de_lectura(error: std::io::Error) -> (String, ErrorEscritorio) {
    let motivo = match error.kind() {
        std::io::ErrorKind::UnexpectedEof
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::BrokenPipe => {
            "la conexión con el escritorio se ha cortado".to_string()
        }
        _ => format!("fallo de red en la sesión RDP: {error}"),
    };
    let error = ErrorEscritorio::Conexion(motivo.clone());
    (motivo, error)
}

/// Acota un tamaño pedido por el visor a lo que admite Display Control: entre
/// 200 y 8192 y con el ancho par.
fn tamano_valido(ancho: u16, alto: u16) -> (u16, u16) {
    let ancho = ancho.clamp(LADO_MIN, LADO_MAX);
    let ancho = if ancho % 2 == 1 { ancho - 1 } else { ancho };
    (ancho, alto.clamp(LADO_MIN, LADO_MAX))
}

/// Cursor por defecto de Windows: una flecha blanca con contorno negro.
/// El contrato del escritorio no tiene un evento "cursor del sistema", así que
/// se manda una flecha de 18×26 con el punto caliente en la punta.
fn flecha_por_defecto() -> (u16, u16, Vec<u8>) {
    const FORMA: [&str; 24] = [
        "#               ",
        "##              ",
        "###             ",
        "####            ",
        "#####           ",
        "######          ",
        "#######         ",
        "########        ",
        "#########       ",
        "##########      ",
        "###########     ",
        "############    ",
        "#############   ",
        "##############  ",
        "############### ",
        "##############  ",
        "#####  ####     ",
        "####   ####     ",
        "###    ####     ",
        "##     ####     ",
        "       ####     ",
        "        ####    ",
        "        ###     ",
        "         #      ",
    ];

    const ANCHO: u16 = 18;
    const ALTO: u16 = 26;
    let ancho = usize::from(ANCHO);
    let alto = usize::from(ALTO);

    let dentro = |x: usize, y: usize| -> bool {
        if x == 0 || y == 0 {
            return false;
        }
        let (x, y) = (x - 1, y - 1);
        FORMA
            .get(y)
            .and_then(|fila| fila.as_bytes().get(x))
            .is_some_and(|byte| *byte == b'#')
    };

    let mut pixeles = vec![0u8; ancho * alto * 4];
    for y in 0..alto {
        for x in 0..ancho {
            let color = if dentro(x, y) {
                // Blanco opaco.
                [0xFF, 0xFF, 0xFF, 0xFF]
            } else if dentro(x.wrapping_sub(1), y)
                || dentro(x + 1, y)
                || dentro(x, y.wrapping_sub(1))
                || dentro(x, y + 1)
            {
                // Contorno negro alrededor de la flecha.
                [0x00, 0x00, 0x00, 0xFF]
            } else {
                // Transparente.
                [0x00, 0x00, 0x00, 0x00]
            };

            let base = (y * ancho + x) * 4;
            pixeles[base..base + 4].copy_from_slice(&color);
        }
    }

    (ANCHO, ALTO, pixeles)
}
