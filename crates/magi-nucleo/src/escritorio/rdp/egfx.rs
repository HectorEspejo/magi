//! Canal EGFX con AVC420 crudo.
//!
//! ironrdp-egfx 0.3 trae el cliente de alto nivel (GraphicsPipelineClient), pero
//! ese cliente decodifica el H.264 a RGBA y descarta las regiones del
//! Avc420BitmapStream. MAGI manda el vídeo de Windows al visor tal cual (D211),
//! así que aquí se implementa el procesador del canal dinámico a mano sobre los
//! tipos públicos de ironrdp-egfx: se descomprime el payload con ZGFX, se
//! interpretan los PDUs y se emiten los NAL en Annex-B con sus regiones.

use ironrdp::core::{decode_cursor, impl_as_any, ReadCursor};
use ironrdp::dvc::{DvcClientProcessor, DvcMessage, DvcProcessor};
use ironrdp::graphics::zgfx;
use ironrdp::pdu::geometry::{InclusiveRectangle, Rectangle as _};
use ironrdp::pdu::{decode_err, PduResult};
use ironrdp_egfx::pdu::{
    Avc420BitmapStream, CapabilitiesAdvertisePdu, CapabilitiesV81Flags, CapabilitiesV8Flags,
    CapabilitySet, Codec1Type, FrameAcknowledgePdu, GfxPdu, QueueDepth,
};
use tokio::sync::mpsc;

use crate::escritorio::{EventoEscritorio, ImagenEscritorio, Region};

/// Lado máximo de una región o de un escritorio (entrada no fiable).
const LADO_MAX: u32 = 8192;
/// Regiones como mucho por fotograma: acota el vector que manda el servidor.
const REGIONES_MAX: usize = 4096;
/// Tamaño máximo de un fotograma H.264 aceptado (16 MiB).
const H264_MAX: usize = 16 * 1024 * 1024;
/// Tamaño máximo del payload EGFX descomprimido (32 MiB): acota una bomba de
/// descompresión de un servidor hostil.
const ZGFX_MAX: usize = 32 * 1024 * 1024;

/// Procesador del canal dinámico Microsoft::Windows::RDS::Graphics.
pub(crate) struct Egfx {
    eventos: mpsc::Sender<EventoEscritorio>,
    descompresor: zgfx::Decompressor,
    bufer: Vec<u8>,
    total_fotogramas: u32,
    fotograma_actual: Option<u32>,
    avc420: bool,
    imagen_dicha: bool,
    tamano_dicho: Option<(u16, u16)>,
    descartados: u64,
}

impl Egfx {
    pub(crate) fn nuevo(eventos: mpsc::Sender<EventoEscritorio>) -> Self {
        Self {
            eventos,
            descompresor: zgfx::Decompressor::new(),
            bufer: Vec::new(),
            total_fotogramas: 0,
            fotograma_actual: None,
            avc420: false,
            imagen_dicha: false,
            tamano_dicho: None,
            descartados: 0,
        }
    }
}

/// Manda un evento al puente sin bloquear el canal de red: si el visor va
/// lento se descarta el fotograma (el vídeo se recupera en el siguiente
/// fotograma clave) en vez de parar la lectura del servidor. Devuelve 1 si se
/// descartó.
fn enviar(eventos: &mpsc::Sender<EventoEscritorio>, evento: EventoEscritorio) -> u64 {
    if eventos.try_send(evento).is_err() {
        tracing::debug!("el puente del escritorio no da abasto: se descarta un evento de vídeo");
        1
    } else {
        0
    }
}

impl_as_any!(Egfx);

impl DvcProcessor for Egfx {
    fn channel_name(&self) -> &str {
        ironrdp_egfx::CHANNEL_NAME
    }

    fn start(&mut self, _canal: u32) -> PduResult<Vec<DvcMessage>> {
        // V8.1 con AVC420 es lo que anuncia un cliente que quiere H.264. V8 se
        // anuncia además por si el servidor no entiende V8.1.
        let capacidades = vec![
            CapabilitySet::V8_1 {
                flags: CapabilitiesV81Flags::AVC420_ENABLED | CapabilitiesV81Flags::SMALL_CACHE,
            },
            CapabilitySet::V8 {
                flags: CapabilitiesV8Flags::SMALL_CACHE,
            },
        ];

        let pdu = GfxPdu::CapabilitiesAdvertise(CapabilitiesAdvertisePdu::from_typed(&capacidades));
        Ok(vec![Box::new(pdu) as DvcMessage])
    }

    fn process(&mut self, _canal: u32, carga: &[u8]) -> PduResult<Vec<DvcMessage>> {
        // Todo lo que viaja por EGFX viene comprimido con ZGFX.
        self.bufer.clear();
        self.descompresor
            .decompress(carga, &mut self.bufer)
            .map_err(|error| decode_err!(error))?;

        if self.bufer.len() > ZGFX_MAX {
            return Err(ironrdp::pdu::other_err!(
                "EGFX",
                "payload descomprimido de más de 32 MiB"
            ));
        }

        let mut respuestas: Vec<DvcMessage> = Vec::new();
        let mut total = self.total_fotogramas;
        let mut fotograma = self.fotograma_actual;
        let mut avc420 = self.avc420;
        let mut imagen_dicha = self.imagen_dicha;
        let mut tamano_dicho = self.tamano_dicho;
        let mut descartados = self.descartados;

        {
            let mut cursor = ReadCursor::new(self.bufer.as_slice());

            while !cursor.is_empty() {
                let pdu: GfxPdu = decode_cursor(&mut cursor).map_err(|error| decode_err!(error))?;

                match pdu {
                    GfxPdu::CapabilitiesConfirm(confirmacion) => {
                        avc420 = confirmacion
                            .0
                            .parsed()
                            .ok()
                            .flatten()
                            .is_some_and(|capacidad| match capacidad {
                                CapabilitySet::V8_1 { flags } => {
                                    flags.contains(CapabilitiesV81Flags::AVC420_ENABLED)
                                }
                                _ => false,
                            });

                        if !imagen_dicha {
                            imagen_dicha = true;
                            let imagen = if avc420 {
                                ImagenEscritorio::H264
                            } else {
                                ImagenEscritorio::Rectangulos
                            };
                            descartados = descartados.saturating_add(enviar(
                                &self.eventos,
                                EventoEscritorio::Imagen(imagen),
                            ));
                        }
                    }
                    GfxPdu::ResetGraphics(pdu) => {
                        if pdu.width > 0
                            && pdu.height > 0
                            && pdu.width <= LADO_MAX
                            && pdu.height <= LADO_MAX
                        {
                            let nuevo = (pdu.width as u16, pdu.height as u16);
                            if tamano_dicho != Some(nuevo) {
                                tamano_dicho = Some(nuevo);
                                descartados = descartados.saturating_add(enviar(
                                    &self.eventos,
                                    EventoEscritorio::Tamano {
                                        ancho: nuevo.0,
                                        alto: nuevo.1,
                                    },
                                ));
                            }
                        }
                    }
                    GfxPdu::StartFrame(pdu) => {
                        fotograma = Some(pdu.frame_id);
                    }
                    GfxPdu::EndFrame(pdu) => {
                        total = total.wrapping_add(1);

                        // MS-RDPEGFX 3.3.5.12: sin FrameAcknowledge el servidor
                        // deja de mandar fotogramas.
                        respuestas.push(Box::new(GfxPdu::FrameAcknowledge(FrameAcknowledgePdu {
                            queue_depth: QueueDepth::Unavailable,
                            frame_id: pdu.frame_id,
                            total_frames_decoded: total,
                        })) as DvcMessage);
                    }
                    GfxPdu::WireToSurface1(pdu) if pdu.codec_id == Codec1Type::Avc420 => {
                        let mut cursor_bitmap = ReadCursor::new(pdu.bitmap_data.as_slice());

                        match decode_cursor::<Avc420BitmapStream<'_>>(&mut cursor_bitmap) {
                            Ok(stream) => {
                                let h264 = a_annex_b(stream.data);
                                if h264.is_empty() || h264.len() > H264_MAX {
                                    // Un fotograma vacío o desmedido no se
                                    // reenvía, pero se confirma igual.
                                    continue;
                                }

                                let regiones: Vec<Region> = stream
                                    .rectangles
                                    .iter()
                                    .take(REGIONES_MAX)
                                    .filter_map(region_de_rectangulo)
                                    .collect();

                                if !imagen_dicha {
                                    imagen_dicha = true;
                                    descartados = descartados.saturating_add(enviar(
                                        &self.eventos,
                                        EventoEscritorio::Imagen(ImagenEscritorio::H264),
                                    ));
                                }

                                descartados = descartados.saturating_add(enviar(
                                    &self.eventos,
                                    EventoEscritorio::Video {
                                        marco: fotograma.unwrap_or(0),
                                        regiones,
                                        h264,
                                    },
                                ));
                            }
                            Err(error) => {
                                tracing::debug!(%error, "AVC420 con cabecera ilegible");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        self.total_fotogramas = total;
        self.fotograma_actual = fotograma;
        self.avc420 = avc420;
        self.imagen_dicha = imagen_dicha;
        self.tamano_dicho = tamano_dicho;
        self.descartados = descartados;

        Ok(respuestas)
    }
}

impl DvcClientProcessor for Egfx {}

/// Convierte una región inclusiva de EGFX en la región del contrato, con los
/// límites del escritorio. None si no es representable.
fn region_de_rectangulo(rectangulo: &InclusiveRectangle) -> Option<Region> {
    let ancho = u32::from(rectangulo.width());
    let alto = u32::from(rectangulo.height());
    if ancho == 0 || alto == 0 || ancho > LADO_MAX || alto > LADO_MAX {
        return None;
    }
    Some(Region::nueva(
        rectangulo.left,
        rectangulo.top,
        ancho as u16,
        alto as u16,
    ))
}

/// Convierte NAL con prefijo de longitud BE de 4 bytes (formato AVC de RDPGFX)
/// a Annex-B (00 00 00 01 delante de cada NAL). Un NAL con longitud imposible
/// se descarta, sin pánico.
pub(crate) fn a_annex_b(avc: &[u8]) -> Vec<u8> {
    let mut salida = Vec::with_capacity(avc.len() + 16);
    let mut desplazamiento = 0usize;

    while desplazamiento + 4 <= avc.len() {
        let longitud = u32::from_be_bytes([
            avc[desplazamiento],
            avc[desplazamiento + 1],
            avc[desplazamiento + 2],
            avc[desplazamiento + 3],
        ]) as usize;
        desplazamiento += 4;

        let Some(fin) = desplazamiento.checked_add(longitud) else {
            break;
        };
        if fin > avc.len() || longitud == 0 {
            break;
        }

        salida.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        salida.extend_from_slice(&avc[desplazamiento..fin]);
        desplazamiento = fin;
    }

    salida
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn convierte_avc_a_annex_b() {
        let mut avc = Vec::new();
        avc.extend_from_slice(&2u32.to_be_bytes());
        avc.extend_from_slice(&[0x67, 0x42]);
        avc.extend_from_slice(&4u32.to_be_bytes());
        avc.extend_from_slice(&[0x68, 0xCE, 0x3C, 0x80]);

        assert_eq!(
            a_annex_b(&avc),
            vec![
                0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x00, 0x00, 0x01, 0x68, 0xCE, 0x3C, 0x80
            ]
        );
    }

    #[test]
    fn annex_b_descarta_longitudes_invalidas() {
        let mut avc = Vec::new();
        avc.extend_from_slice(&99u32.to_be_bytes());
        avc.extend_from_slice(&[0x67, 0x42]);
        assert!(a_annex_b(&avc).is_empty());

        // Longitud cero: no se cuela un NAL vacío.
        let mut cero = Vec::new();
        cero.extend_from_slice(&0u32.to_be_bytes());
        assert!(a_annex_b(&cero).is_empty());
    }

    #[test]
    fn avc420_crudo_emite_video_con_regiones() {
        use ironrdp::core::encode_vec;
        use ironrdp::graphics::zgfx::{compress_and_wrap_egfx, CompressionMode, Compressor};
        use ironrdp_egfx::pdu::{PixelFormat as PixelGfx, QuantQuality, WireToSurface1Pdu};

        let mut avc = Vec::new();
        avc.extend_from_slice(&2u32.to_be_bytes());
        avc.extend_from_slice(&[0x67, 0x42]);
        avc.extend_from_slice(&3u32.to_be_bytes());
        avc.extend_from_slice(&[0x68, 0xCE, 0x3C]);

        let stream = Avc420BitmapStream {
            rectangles: vec![InclusiveRectangle {
                left: 0,
                top: 0,
                right: 63,
                bottom: 47,
            }],
            quant_qual_vals: vec![QuantQuality {
                quantization_parameter: 22,
                progressive: false,
                quality: 100,
            }],
            data: &avc,
        };

        let bitmap_data = encode_vec(&stream).expect("codificar Avc420BitmapStream");

        let pdu = GfxPdu::WireToSurface1(WireToSurface1Pdu {
            surface_id: 7,
            codec_id: Codec1Type::Avc420,
            pixel_format: PixelGfx::ARgb,
            destination_rectangle: ironrdp::pdu::geometry::ExclusiveRectangle {
                left: 0,
                top: 0,
                right: 64,
                bottom: 48,
            },
            bitmap_data,
        });

        let bytes = encode_vec(&pdu).expect("codificar GfxPdu");
        let carga = compress_and_wrap_egfx(&bytes, &mut Compressor::new(), CompressionMode::Never)
            .expect("envolver en ZGFX");

        let (tx, mut rx) = mpsc::channel(8);
        let mut procesador = Egfx::nuevo(tx);

        let respuestas = procesador.process(0, &carga).expect("procesar EGFX");
        assert!(
            respuestas.is_empty(),
            "un WireToSurface1 no genera respuesta"
        );

        // El primer evento es Imagen(H264) y el segundo el vídeo.
        match rx.try_recv().expect("debe anunciar la imagen") {
            EventoEscritorio::Imagen(ImagenEscritorio::H264) => {}
            otro => panic!("evento inesperado: {otro:?}"),
        }

        match rx.try_recv().expect("debe emitir un fotograma") {
            EventoEscritorio::Video { regiones, h264, .. } => {
                assert_eq!(regiones.len(), 1);
                assert_eq!(regiones[0].ancho, 64);
                assert_eq!(regiones[0].alto, 48);
                assert_eq!(h264.len(), 13);
                assert_eq!(&h264[..4], &[0x00, 0x00, 0x00, 0x01]);
                assert_eq!(&h264[4..6], &[0x67, 0x42]);
            }
            otro => panic!("evento inesperado: {otro:?}"),
        }
    }

    #[test]
    fn end_frame_responde_con_frame_acknowledge() {
        use ironrdp::core::encode_vec;
        use ironrdp::graphics::zgfx::{compress_and_wrap_egfx, CompressionMode, Compressor};
        use ironrdp_egfx::pdu::EndFramePdu;

        let pdu = GfxPdu::EndFrame(EndFramePdu { frame_id: 9 });
        let bytes = encode_vec(&pdu).expect("codificar GfxPdu");
        let carga = compress_and_wrap_egfx(&bytes, &mut Compressor::new(), CompressionMode::Never)
            .expect("envolver en ZGFX");

        let (tx, _rx) = mpsc::channel(8);
        let mut procesador = Egfx::nuevo(tx);
        let respuestas = procesador.process(0, &carga).expect("procesar EGFX");
        assert_eq!(respuestas.len(), 1);
    }
}
