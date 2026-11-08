//! Protocolo RFB (RFC 6143): constantes, formato de píxel fijo, búfer de
//! entrada y construcción de los mensajes del cliente.
//!
//! El cliente pide siempre un formato de píxel fijo (32 bits, true color,
//! RGBA en little endian: R, G, B y un byte de relleno), así que todo lo que
//! llega del servidor se interpreta con ese formato y se convierte a RGBA
//! opaco para el puente. El formato es una decisión del cliente: no depende de
//! lo que anuncie el servidor.

use std::io::Write as _;

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::escritorio::ErrorEscritorio;

/// Lado máximo del escritorio remoto (entrada no fiable).
pub const LADO_MAX: u16 = crate::escritorio::LADO_MAX;
/// Lado máximo del cursor remoto (entrada no fiable).
pub const CURSOR_MAX: u16 = 256;
/// Tope del texto de portapapeles (entrada no fiable).
pub const TEXTO_MAX: usize = 1024 * 1024;
/// Tope del nombre del escritorio que manda el servidor en ServerInit.
pub const NOMBRE_MAX: usize = 1024 * 1024;
/// Tope de un mensaje de portapapeles ya codificado (texto, cabecera y zlib).
pub const CORTE_MAX: usize = TEXTO_MAX + 128 * 1024;

// ------------------------------------------------------------------ versiones

/// Versión del protocolo que se acaba usando, tras la negociación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// RFB 3.3: el servidor elige la seguridad con una palabra.
    V33,
    /// RFB 3.7: el servidor da la lista de seguridades.
    V37,
    /// RFB 3.8: como 3.7 y los rechazos llevan motivo.
    V38,
}

impl Version {
    /// Texto que se manda en el saludo.
    pub fn saludo(self) -> &'static [u8; 12] {
        match self {
            Version::V33 => b"RFB 003.003\n",
            Version::V37 => b"RFB 003.007\n",
            Version::V38 => b"RFB 003.008\n",
        }
    }

    /// ¿El resultado de seguridad se lee también con la seguridad None?
    pub fn resultado_sin_seguridad(self) -> bool {
        self == Version::V38
    }

    /// ¿Los fallos de seguridad llevan motivo?
    pub fn motivo_en_fallo(self) -> bool {
        self == Version::V38
    }
}

// -------------------------------------------------------------- codificaciones

/// Raw: píxeles sin comprimir.
pub const COD_RAW: i32 = 0;
/// CopyRect: copiar una zona ya pintada.
pub const COD_COPYRECT: i32 = 1;
/// Hextile: teselas de 16×16.
pub const COD_HEXTILE: i32 = 5;
/// ZRLE: zlib, teselas de 64×64 y RLE.
pub const COD_ZRLE: i32 = 16;
/// Seudocodificación DesktopSize.
pub const COD_DESKTOP_SIZE: i32 = -223;
/// Seudocodificación ExtendedDesktopSize.
pub const COD_EXTENDED_DESKTOP_SIZE: i32 = -308;
/// Seudocodificación Cursor.
pub const COD_CURSOR: i32 = -239;
/// Seudocodificación QEMU Extended Key Event.
pub const COD_QEMU_EXTENDED_KEY: i32 = -258;
/// Seudocodificación Extended Clipboard (portapapeles extendido).
pub const COD_PORTAPAPELES_EXTENDIDO: i32 = 0xC0A1_E5CEu32 as i32;

/// Codificaciones que se piden al servidor, en orden de preferencia.
pub const CODIFICACIONES: [i32; 9] = [
    COD_ZRLE,
    COD_HEXTILE,
    COD_COPYRECT,
    COD_RAW,
    COD_DESKTOP_SIZE,
    COD_EXTENDED_DESKTOP_SIZE,
    COD_CURSOR,
    COD_QEMU_EXTENDED_KEY,
    COD_PORTAPAPELES_EXTENDIDO,
];

// ------------------------------------------------------------ mensajes

/// Tipo de mensaje del servidor: actualización de framebuffer.
pub const MSG_ACTUALIZACION: u8 = 0;
/// Tipo de mensaje del servidor: tabla de colores.
pub const MSG_TABLA_COLORES: u8 = 1;
/// Tipo de mensaje del servidor: campana.
pub const MSG_CAMPANA: u8 = 2;
/// Tipo de mensaje del servidor: portapapeles.
pub const MSG_CORTE_SERVIDOR: u8 = 3;
/// Tipo de mensaje del servidor: fin de actualizaciones continuas.
pub const MSG_FIN_CONTINUAS: u8 = 150;
/// Tipo de mensaje del servidor: valla (Fence).
pub const MSG_VALLA_SERVIDOR: u8 = 248;

/// Tipo de mensaje del cliente: formato de píxel.
pub const MSG_FIJAR_PIXEL: u8 = 0;
/// Tipo de mensaje del cliente: codificaciones.
pub const MSG_FIJAR_CODIFICACIONES: u8 = 2;
/// Tipo de mensaje del cliente: petición de actualización.
pub const MSG_PEDIR_ACTUALIZACION: u8 = 3;
/// Tipo de mensaje del cliente: tecla.
pub const MSG_TECLA: u8 = 4;
/// Tipo de mensaje del cliente: ratón.
pub const MSG_RATON: u8 = 5;
/// Tipo de mensaje del cliente: portapapeles.
pub const MSG_CORTE_CLIENTE: u8 = 6;
/// Tipo de mensaje del cliente: cambiar el tamaño del escritorio.
pub const MSG_CAMBIAR_TAMANO: u8 = 251;
/// Tipo de mensaje del cliente: extensión de QEMU.
pub const MSG_QEMU: u8 = 255;
/// Subtipo de la extensión de QEMU: tecla extendida.
pub const QEMU_TECLA_EXTENDIDA: u8 = 0;

// ------------------------------------------------------------------ píxel

/// Formato de píxel que pide el cliente: 32 bits, true color, canales de 8
/// bits en little endian (bytes R, G, B y relleno).
pub const FORMATO_PIXEL: [u8; 16] = [
    32, // bits-per-pixel
    24, // depth
    0,  // big-endian-flag
    1,  // true-colour-flag
    0, 255, // red-max
    0, 255, // green-max
    0, 255, // blue-max
    0,   // red-shift
    8,   // green-shift
    16,  // blue-shift
    0, 0, 0, // relleno
];

/// Bytes por píxel con el formato fijo.
pub const BYTES_POR_PIXEL: usize = 4;
/// Bytes por CPIXEL de ZRLE con el formato fijo (R, G y B, sin relleno).
pub const BYTES_POR_CPIXEL: usize = 3;

/// Convierte un píxel del formato fijo (R, G, B, relleno) a RGBA opaco. No
/// indexa: quien llama garantiza que el segmento tiene 4 bytes.
pub fn pixel_a_rgba(pixel: &[u8]) -> [u8; 4] {
    [
        pixel.first().copied().unwrap_or(0),
        pixel.get(1).copied().unwrap_or(0),
        pixel.get(2).copied().unwrap_or(0),
        255,
    ]
}

/// Pinta un rectángulo de un solo color en un búfer RGBA de "ancho" píxeles.
/// Devuelve false si el rectángulo no cabe (nunca entra en pánico).
pub fn pintar_rectangulo(
    destino: &mut [u8],
    ancho: u16,
    x: u16,
    y: u16,
    ancho_rect: u16,
    alto_rect: u16,
    rgba: [u8; 4],
) -> bool {
    if ancho_rect == 0 || alto_rect == 0 {
        return true;
    }
    let ancho = u32::from(ancho);
    let x = u32::from(x);
    let y = u32::from(y);
    let ancho_rect = u32::from(ancho_rect);
    let alto_rect = u32::from(alto_rect);
    if x + ancho_rect > ancho || (y + alto_rect) * ancho > (destino.len() / BYTES_POR_PIXEL) as u32
    {
        return false;
    }
    for fila in y..y + alto_rect {
        let inicio = ((fila * ancho + x) as usize) * BYTES_POR_PIXEL;
        let fin = inicio + (ancho_rect as usize) * BYTES_POR_PIXEL;
        let Some(trozo) = destino.get_mut(inicio..fin) else {
            return false;
        };
        for pixel in trozo.chunks_exact_mut(BYTES_POR_PIXEL) {
            pixel.copy_from_slice(&rgba);
        }
    }
    true
}

// ------------------------------------------------------------------ entrada

/// Búfer de entrada con posición de lectura. Permite mirar sin consumir, para
/// que el analizador no pierda estado cuando todavía no han llegado todos los
/// bytes de un mensaje.
#[derive(Debug, Default)]
pub struct Entrada {
    datos: Vec<u8>,
    pos: usize,
}

impl Entrada {
    pub fn nueva() -> Self {
        Self::default()
    }

    /// Añade bytes recibidos del socket.
    pub fn anadir(&mut self, trozo: &[u8]) {
        self.datos.extend_from_slice(trozo);
    }

    /// Bytes pendientes de analizar.
    pub fn disponible(&self) -> usize {
        self.datos.len().saturating_sub(self.pos)
    }

    /// Mira los próximos "n" bytes sin consumirlos.
    pub fn mirar(&self, n: usize) -> Option<&[u8]> {
        if n > self.disponible() {
            return None;
        }
        self.datos.get(self.pos..self.pos + n)
    }

    /// Consume "n" bytes.
    pub fn tomar(&mut self, n: usize) -> Option<&[u8]> {
        if n > self.disponible() {
            return None;
        }
        let inicio = self.pos;
        self.pos += n;
        self.datos.get(inicio..inicio + n)
    }

    pub fn u8(&mut self) -> Option<u8> {
        let valor = *self.mirar(1)?.first()?;
        self.pos += 1;
        Some(valor)
    }

    pub fn u16(&mut self) -> Option<u16> {
        let bytes = self.mirar(2)?;
        let valor = u16::from_be_bytes([bytes[0], bytes[1]]);
        self.pos += 2;
        Some(valor)
    }

    /// Descarta los bytes ya consumidos. El analizador no guarda segmentos
    /// prestados, así que es seguro llamarlo entre pasos.
    pub fn compactar(&mut self) {
        if self.pos > 0 {
            self.datos.drain(..self.pos);
            self.pos = 0;
        }
    }
}

// ------------------------------------------------------------------ errores

/// Error de datos inválidos del servidor, con el prefijo que exige el
/// checklist: la sesión cae con «datos inválidos del servidor».
pub fn datos_invalidos(detalle: impl std::fmt::Display) -> ErrorEscritorio {
    ErrorEscritorio::Datos(format!("datos inválidos del servidor: {detalle}"))
}

// ------------------------------------------------------------------ mensajes

/// SetPixelFormat con el formato fijo del cliente.
pub fn mensaje_fijar_pixel() -> [u8; 20] {
    let mut mensaje = [0u8; 20];
    mensaje[0] = MSG_FIJAR_PIXEL;
    mensaje[4..].copy_from_slice(&FORMATO_PIXEL);
    mensaje
}

/// SetEncodings con las codificaciones y seudocodificaciones pedidas.
pub fn mensaje_fijar_codificaciones() -> Vec<u8> {
    let mut mensaje = Vec::with_capacity(4 + CODIFICACIONES.len() * 4);
    mensaje.push(MSG_FIJAR_CODIFICACIONES);
    mensaje.push(0);
    mensaje.extend_from_slice(&(CODIFICACIONES.len() as u16).to_be_bytes());
    for codificacion in CODIFICACIONES {
        mensaje.extend_from_slice(&codificacion.to_be_bytes());
    }
    mensaje
}

/// FramebufferUpdateRequest del escritorio entero.
pub fn mensaje_pedir_actualizacion(incremental: bool, ancho: u16, alto: u16) -> [u8; 10] {
    let mut mensaje = [0u8; 10];
    mensaje[0] = MSG_PEDIR_ACTUALIZACION;
    mensaje[1] = u8::from(incremental);
    mensaje[2..4].copy_from_slice(&0u16.to_be_bytes());
    mensaje[4..6].copy_from_slice(&0u16.to_be_bytes());
    mensaje[6..8].copy_from_slice(&ancho.to_be_bytes());
    mensaje[8..10].copy_from_slice(&alto.to_be_bytes());
    mensaje
}

/// KeyEvent clásico (keysym de X11).
pub fn mensaje_tecla(abajo: bool, keysym: u32) -> [u8; 8] {
    let mut mensaje = [0u8; 8];
    mensaje[0] = MSG_TECLA;
    mensaje[1] = u8::from(abajo);
    mensaje[4..].copy_from_slice(&keysym.to_be_bytes());
    mensaje
}

/// QEMU Extended Key Event (mensaje 255, subtipo 0). El keycode es el
/// scancode XT con la extensión 0xE0 codificada en el bit alto.
pub fn mensaje_tecla_qemu(abajo: bool, keysym: u32, keycode: u32) -> [u8; 12] {
    let mut mensaje = [0u8; 12];
    mensaje[0] = MSG_QEMU;
    mensaje[1] = QEMU_TECLA_EXTENDIDA;
    mensaje[2..4].copy_from_slice(&u16::from(abajo).to_be_bytes());
    mensaje[4..8].copy_from_slice(&keysym.to_be_bytes());
    mensaje[8..12].copy_from_slice(&keycode.to_be_bytes());
    mensaje
}

/// PointerEvent absoluto con la máscara de botones.
pub fn mensaje_raton(mascara: u8, x: u16, y: u16) -> [u8; 6] {
    let mut mensaje = [0u8; 6];
    mensaje[0] = MSG_RATON;
    mensaje[1] = mascara;
    mensaje[2..4].copy_from_slice(&x.to_be_bytes());
    mensaje[4..6].copy_from_slice(&y.to_be_bytes());
    mensaje
}

/// SetDesktopSize con una sola pantalla que cubre todo el escritorio.
pub fn mensaje_cambiar_tamano(ancho: u16, alto: u16, pantalla_id: u32) -> [u8; 24] {
    let mut mensaje = [0u8; 24];
    mensaje[0] = MSG_CAMBIAR_TAMANO;
    mensaje[1] = 0;
    mensaje[2..4].copy_from_slice(&ancho.to_be_bytes());
    mensaje[4..6].copy_from_slice(&alto.to_be_bytes());
    mensaje[6] = 1; // number-of-screens
    mensaje[7] = 0;
    mensaje[8..12].copy_from_slice(&pantalla_id.to_be_bytes());
    mensaje[12..14].copy_from_slice(&0u16.to_be_bytes());
    mensaje[14..16].copy_from_slice(&0u16.to_be_bytes());
    mensaje[16..18].copy_from_slice(&ancho.to_be_bytes());
    mensaje[18..20].copy_from_slice(&alto.to_be_bytes());
    mensaje[20..24].copy_from_slice(&0u32.to_be_bytes());
    mensaje
}

/// Texto que cabe en Latin-1; lo que no cabe se sustituye por '?'. Los finales
/// de línea se mandan como un solo salto de línea (RFC 6143).
pub fn texto_latin1(texto: &str) -> Vec<u8> {
    let mut salida = Vec::with_capacity(texto.len());
    let mut retorno_pendiente = false;
    for caracter in texto.chars() {
        if caracter == '\r' {
            if retorno_pendiente {
                salida.push(b'\n');
            }
            retorno_pendiente = true;
            continue;
        }
        if retorno_pendiente {
            salida.push(b'\n');
            retorno_pendiente = false;
        }
        if caracter == '\n' {
            salida.push(b'\n');
        } else {
            let punto = caracter as u32;
            if punto <= 0xFF {
                salida.push(punto as u8);
            } else {
                salida.push(b'?');
            }
        }
    }
    if retorno_pendiente {
        salida.push(b'\n');
    }
    salida
}

/// ClientCutText con texto Latin-1.
pub fn mensaje_corte_latin1(texto: &str) -> Vec<u8> {
    let cuerpo = texto_latin1(texto);
    let mut mensaje = Vec::with_capacity(8 + cuerpo.len());
    mensaje.push(MSG_CORTE_CLIENTE);
    mensaje.extend_from_slice(&[0, 0, 0]);
    mensaje.extend_from_slice(&(cuerpo.len() as u32).to_be_bytes());
    mensaje.extend_from_slice(&cuerpo);
    mensaje
}

/// ClientCutText en formato extendido (longitud negativa): cabecera de banderas
/// y cuerpo ya construido.
pub fn mensaje_corte_extendido(banderas: u32, cuerpo: &[u8]) -> Vec<u8> {
    let total = 4 + cuerpo.len();
    let mut mensaje = Vec::with_capacity(8 + total);
    mensaje.push(MSG_CORTE_CLIENTE);
    mensaje.extend_from_slice(&[0, 0, 0]);
    mensaje.extend_from_slice(&(-(total as i32)).to_be_bytes());
    mensaje.extend_from_slice(&banderas.to_be_bytes());
    mensaje.extend_from_slice(cuerpo);
    mensaje
}

/// Banderas del portapapeles extendido.
pub const CORTE_TEXTO: u32 = 0x0000_0001;
pub const CORTE_CAPS: u32 = 0x0100_0000;
pub const CORTE_PETICION: u32 = 0x0200_0000;
pub const CORTE_AVISO: u32 = 0x0400_0000;
pub const CORTE_PROVISION: u32 = 0x1000_0000;

/// Mensaje de capacidades del portapapeles extendido: solo texto, con el tope
/// pedido (el servidor no mandará textos mayores sin avisar).
pub fn mensaje_corte_caps(tope_texto: u32) -> Vec<u8> {
    mensaje_corte_extendido(CORTE_CAPS | CORTE_TEXTO, &tope_texto.to_be_bytes())
}

/// Mensaje de petición del portapapeles extendido (solo texto).
pub fn mensaje_corte_peticion() -> Vec<u8> {
    mensaje_corte_extendido(CORTE_PETICION | CORTE_TEXTO, &[])
}

/// Zlib de un flujo y sus datos, con el tamaño por delante (formato de
/// "provide" del portapapeles extendido).
fn comprimir_provision(texto: &str) -> Vec<u8> {
    let mut cuerpo = Vec::with_capacity(texto.len() + 5);
    cuerpo.extend_from_slice(&((texto.len() + 1) as u32).to_be_bytes());
    cuerpo.extend_from_slice(texto.as_bytes());
    cuerpo.push(0);
    let mut codificador = ZlibEncoder::new(Vec::new(), Compression::default());
    // Escribir en un Vec no falla; si fallara, el mensaje saldría vacío y el
    // servidor lo ignoraría (nunca se entra en pánico por el portapapeles).
    if codificador.write_all(&cuerpo).is_err() {
        return Vec::new();
    }
    codificador.finish().unwrap_or_default()
}

/// Provide del portapapeles extendido con texto UTF-8.
pub fn mensaje_corte_provision_texto(texto: &str) -> Vec<u8> {
    let comprimido = comprimir_provision(texto);
    mensaje_corte_extendido(CORTE_PROVISION | CORTE_TEXTO, &comprimido)
}

/// Descomprime zlib sin pasar del tope indicado (entrada no fiable).
///
/// Se usa un ZlibDecoder sobre un Take de tope+1: flate2 avanza la entrada por
/// sí solo. El bucle anterior reutilizaba la misma rebanada en cada llamada y
/// fallaba con textos de más de 16 KB («incorrect data check», M1).
pub fn descomprimir_acotado(datos: &[u8], tope: usize) -> Result<Vec<u8>, ErrorEscritorio> {
    use std::io::Read as _;

    let mut descompresor = flate2::read::ZlibDecoder::new(datos);
    let mut acotado = (&mut descompresor).take(tope.saturating_add(1) as u64);
    let mut salida = Vec::new();
    acotado
        .read_to_end(&mut salida)
        .map_err(|error| datos_invalidos(format!("zlib corrupto ({error})")))?;
    if salida.len() > tope {
        return Err(datos_invalidos("el portapapeles del servidor pasa de 1 MB"));
    }
    Ok(salida)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn comprimir(datos: &[u8]) -> Vec<u8> {
        let mut codificador = ZlibEncoder::new(Vec::new(), Compression::default());
        codificador.write_all(datos).expect("comprimir");
        codificador.finish().expect("terminar zlib")
    }

    #[test]
    fn descomprime_textos_de_mas_de_16_kb() {
        // M1: con el bucle anterior, la segunda llamada a flate2 reutilizaba la
        // rebanada y fallaba con «incorrect data check» a partir de 16 KB.
        let texto = "ñ€ y texto largo ".repeat(12_000);
        assert!(texto.len() > 16 * 1024);
        let comprimido = comprimir(texto.as_bytes());
        let descomprimido =
            descomprimir_acotado(&comprimido, TEXTO_MAX).expect("descomprimir texto largo");
        assert_eq!(descomprimido, texto.as_bytes());
    }

    #[test]
    fn rechaza_lo_que_pasa_del_tope_sin_panico() {
        let datos = vec![7u8; 4096];
        let comprimido = comprimir(&datos);
        assert!(descomprimir_acotado(&comprimido, 1024).is_err());
        // Un flujo truncado también da error de datos, nunca un pánico.
        assert!(descomprimir_acotado(&comprimido[..comprimido.len() / 2], TEXTO_MAX).is_err());
    }
}
