//! Decodificación ZRLE (RFC 6143, codificación 16): zlib, teselas de 64×64 y
//! RLE. El flujo zlib es único para toda la sesión, así que el descompresor se
//! conserva entre rectángulos. La descompresión va acotada al tamaño esperado:
//! cada tesela se decodifica sobre una copia y no se acepta ni un byte de más.

use std::collections::VecDeque;

use super::protocolo::{datos_invalidos, Entrada, BYTES_POR_CPIXEL};
use crate::escritorio::ErrorEscritorio;

/// Lado de una tesela ZRLE.
const TESELA: u16 = 64;
/// Tope de bytes descomprimidos que se acumulan esperando a una tesela; con
/// una lectura del descompresor de 32 KB sobra y acota un servidor malicioso.
const COLA_MAX: usize = 64 * 1024;

/// Estado del flujo zlib de ZRLE, compartido por todos los rectángulos de la
/// sesión (RFC 6143: un solo flujo por conexión).
pub struct DescompresorZrle {
    desc: flate2::Decompress,
}

impl DescompresorZrle {
    pub fn nuevo() -> Self {
        Self {
            desc: flate2::Decompress::new(true),
        }
    }
}

/// Rectángulo ZRLE completo, listo para el puente.
pub struct RectanguloZrle {
    pub x: u16,
    pub y: u16,
    pub ancho: u16,
    pub alto: u16,
    pub pixeles: Vec<u8>,
}

/// Decodificador incremental de un rectángulo ZRLE.
pub struct Zrle {
    x: u16,
    y: u16,
    ancho: u16,
    alto: u16,
    pixeles: Vec<u8>,
    /// Columna y fila de la siguiente tesela.
    columna: u16,
    fila: u16,
    /// Bytes ya descomprimidos pendientes de interpretar.
    cola: VecDeque<u8>,
    /// Bytes comprimidos pendientes de pasar al descompresor.
    comprimidos: Vec<u8>,
    /// Bytes comprimidos que faltan por tomar de la entrada.
    comprimidos_restantes: Option<usize>,
}

impl Zrle {
    /// Prepara un rectángulo de ancho×alto píxeles. El llamador ya ha
    /// comprobado que cabe en el escritorio.
    pub fn nuevo(x: u16, y: u16, ancho: u16, alto: u16) -> Self {
        let total = usize::from(ancho) * usize::from(alto) * 4;
        Self {
            x,
            y,
            ancho,
            alto,
            pixeles: vec![0u8; total],
            columna: 0,
            fila: 0,
            cola: VecDeque::new(),
            comprimidos: Vec::new(),
            comprimidos_restantes: None,
        }
    }

    /// Consume lo que pueda de la entrada. Devuelve true cuando el rectángulo
    /// ya está completo.
    pub fn avanzar(
        &mut self,
        entrada: &mut Entrada,
        descompresor: &mut DescompresorZrle,
    ) -> Result<bool, ErrorEscritorio> {
        if self.comprimidos_restantes.is_none() {
            let Some(bytes) = entrada.mirar(4) else {
                return Ok(false);
            };
            let declarados = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
            let maximo = self.largo_maximo();
            if declarados > maximo {
                return Err(datos_invalidos(format!(
                    "el ZRLE declara {declarados} bytes comprimidos (máximo {maximo})"
                )));
            }
            entrada.tomar(4);
            self.comprimidos_restantes = Some(declarados);
        }
        loop {
            if self.fila >= self.teselas_alto() {
                return self.finalizar(entrada, descompresor);
            }
            let ancho_tesela = self.ancho_tesela_actual();
            let alto_tesela = self.alto_tesela_actual();
            self.cola.make_contiguous();
            match decodificar_tesela(self.cola.as_slices().0, ancho_tesela, alto_tesela)? {
                Some((consumidos, pixeles_tesela)) => {
                    self.cola.drain(..consumidos);
                    copiar_tesela(
                        &mut self.pixeles,
                        self.ancho,
                        self.columna * TESELA,
                        self.fila * TESELA,
                        ancho_tesela,
                        alto_tesela,
                        &pixeles_tesela,
                    )?;
                    self.siguiente_tesela();
                }
                None => {
                    if !self.rellenar(entrada, descompresor)? {
                        return Ok(false);
                    }
                }
            }
        }
    }

    /// Devuelve el rectángulo terminado.
    pub fn terminar(self) -> RectanguloZrle {
        RectanguloZrle {
            x: self.x,
            y: self.y,
            ancho: self.ancho,
            alto: self.alto,
            pixeles: self.pixeles,
        }
    }

    fn teselas_alto(&self) -> u16 {
        self.alto.div_ceil(TESELA)
    }

    fn ancho_tesela_actual(&self) -> u16 {
        TESELA.min(self.ancho - self.columna * TESELA)
    }

    fn alto_tesela_actual(&self) -> u16 {
        TESELA.min(self.alto - self.fila * TESELA)
    }

    fn siguiente_tesela(&mut self) {
        self.columna += 1;
        if self.columna * TESELA >= self.ancho {
            self.columna = 0;
            self.fila += 1;
        }
    }

    /// Tope de bytes comprimidos que se aceptan para este rectángulo: el
    /// máximo de datos ZRLE sin comprimir (4 bytes por píxel y paletas), más
    /// margen para la cabecera zlib y el relleno de sincronización.
    fn largo_maximo(&self) -> usize {
        let pixeles = usize::from(self.ancho) * usize::from(self.alto);
        let teselas =
            usize::from(self.ancho.div_ceil(TESELA)) * usize::from(self.alto.div_ceil(TESELA));
        pixeles * 4 + teselas * 512 + 64 * 1024
    }

    /// Asegura bytes descomprimidos en la cola. Devuelve false si hay que
    /// esperar a que lleguen más bytes del socket; un ZRLE truncado es error.
    fn rellenar(
        &mut self,
        entrada: &mut Entrada,
        descompresor: &mut DescompresorZrle,
    ) -> Result<bool, ErrorEscritorio> {
        loop {
            if !self.comprimidos.is_empty() {
                let entrado_antes = descompresor.desc.total_in();
                let salido_antes = descompresor.desc.total_out();
                let mut salida = [0u8; 32 * 1024];
                let estado = descompresor
                    .desc
                    .decompress(
                        &self.comprimidos,
                        &mut salida,
                        flate2::FlushDecompress::None,
                    )
                    .map_err(|error| datos_invalidos(format!("zlib de ZRLE corrupto ({error})")))?;
                let consumidos = (descompresor.desc.total_in() - entrado_antes) as usize;
                let producidos = (descompresor.desc.total_out() - salido_antes) as usize;
                if consumidos > 0 {
                    self.comprimidos
                        .drain(..consumidos.min(self.comprimidos.len()));
                }
                if producidos > 0 {
                    self.cola.extend(&salida[..producidos]);
                    if self.cola.len() > COLA_MAX {
                        return Err(datos_invalidos(
                            "el ZRLE descomprime más datos de los esperados",
                        ));
                    }
                    return Ok(true);
                }
                if consumidos == 0 {
                    if self.comprimidos_restantes == Some(0) {
                        return Err(datos_invalidos("el ZRLE está truncado"));
                    }
                    // Hacen falta más bytes comprimidos: se piden abajo.
                } else if estado == flate2::Status::StreamEnd && self.comprimidos.is_empty() {
                    // El flujo zlib no debe terminar a mitad de un rectángulo,
                    // pero si el servidor lo cierra no se puede seguir.
                    return Err(datos_invalidos(
                        "el flujo zlib de ZRLE terminó antes de tiempo",
                    ));
                }
                continue;
            }
            let restantes = self.comprimidos_restantes.unwrap_or(0);
            if restantes == 0 {
                return Err(datos_invalidos("el ZRLE está truncado"));
            }
            if entrada.disponible() == 0 {
                return Ok(false);
            }
            let tomar = restantes.min(entrada.disponible()).min(64 * 1024);
            let Some(trozo) = entrada.tomar(tomar) else {
                return Ok(false);
            };
            self.comprimidos.extend_from_slice(trozo);
            self.comprimidos_restantes = Some(restantes - tomar);
        }
    }

    /// Tras la última tesela comprueba que no sobra nada: ni bytes
    /// descomprimidos (el rectángulo llevaría datos de más) ni comprimidos sin
    /// consumir (que además desincronizarían el flujo zlib de la sesión).
    fn finalizar(
        &mut self,
        entrada: &mut Entrada,
        descompresor: &mut DescompresorZrle,
    ) -> Result<bool, ErrorEscritorio> {
        if !self.cola.is_empty() {
            return Err(datos_invalidos(
                "el ZRLE trae más píxeles de los que declara su rectángulo",
            ));
        }
        loop {
            let restantes = self.comprimidos_restantes.unwrap_or(0);
            if restantes > 0 && self.comprimidos.is_empty() {
                if entrada.disponible() == 0 {
                    return Ok(false);
                }
                let tomar = restantes.min(entrada.disponible()).min(64 * 1024);
                let Some(trozo) = entrada.tomar(tomar) else {
                    return Ok(false);
                };
                self.comprimidos.extend_from_slice(trozo);
                self.comprimidos_restantes = Some(restantes - tomar);
                continue;
            }
            if self.comprimidos.is_empty() {
                break;
            }
            let entrado_antes = descompresor.desc.total_in();
            let salido_antes = descompresor.desc.total_out();
            let mut salida = [0u8; 16 * 1024];
            let _estado = descompresor
                .desc
                .decompress(
                    &self.comprimidos,
                    &mut salida,
                    flate2::FlushDecompress::None,
                )
                .map_err(|error| datos_invalidos(format!("zlib de ZRLE corrupto ({error})")))?;
            let consumidos = (descompresor.desc.total_in() - entrado_antes) as usize;
            let producidos = (descompresor.desc.total_out() - salido_antes) as usize;
            if producidos > 0 {
                return Err(datos_invalidos(
                    "el ZRLE trae más píxeles de los que declara su rectángulo",
                ));
            }
            if consumidos == 0 {
                return Err(datos_invalidos("el ZRLE está truncado"));
            }
            self.comprimidos
                .drain(..consumidos.min(self.comprimidos.len()));
        }
        Ok(true)
    }
}

// ------------------------------------------------------------------ teselas

fn color_de_cpixel(datos: &[u8]) -> [u8; 4] {
    [
        datos.first().copied().unwrap_or(0),
        datos.get(1).copied().unwrap_or(0),
        datos.get(2).copied().unwrap_or(0),
        255,
    ]
}

fn pixeles_solidos(total: usize, color: [u8; 4]) -> Vec<u8> {
    let mut pixeles = Vec::with_capacity(total * 4);
    for _ in 0..total {
        pixeles.extend_from_slice(&color);
    }
    pixeles
}

/// Decodifica una tesela entera. Devuelve None si todavía no han llegado todos
/// sus bytes (no se consume nada hasta que está completa).
fn decodificar_tesela(
    datos: &[u8],
    ancho: u16,
    alto: u16,
) -> Result<Option<(usize, Vec<u8>)>, ErrorEscritorio> {
    let total = usize::from(ancho) * usize::from(alto);
    let Some(subcodificacion) = datos.first().copied() else {
        return Ok(None);
    };
    let mut pos = 1usize;
    match subcodificacion {
        0 => {
            // Tesela cruda: un CPIXEL de 3 bytes por píxel.
            let necesarios = total * BYTES_POR_CPIXEL;
            if datos.len() < pos + necesarios {
                return Ok(None);
            }
            let mut pixeles = Vec::with_capacity(total * 4);
            for crudo in datos[pos..pos + necesarios].chunks_exact(BYTES_POR_CPIXEL) {
                pixeles.extend_from_slice(&color_de_cpixel(crudo));
            }
            Ok(Some((pos + necesarios, pixeles)))
        }
        1 => {
            // Tesela de un solo color.
            if datos.len() < pos + BYTES_POR_CPIXEL {
                return Ok(None);
            }
            let color = color_de_cpixel(&datos[pos..pos + BYTES_POR_CPIXEL]);
            pos += BYTES_POR_CPIXEL;
            Ok(Some((pos, pixeles_solidos(total, color))))
        }
        2..=16 => {
            let tamano_paleta = usize::from(subcodificacion);
            let bytes_paleta = tamano_paleta * BYTES_POR_CPIXEL;
            if datos.len() < pos + bytes_paleta {
                return Ok(None);
            }
            let mut paleta = Vec::with_capacity(tamano_paleta);
            for crudo in datos[pos..pos + bytes_paleta].chunks_exact(BYTES_POR_CPIXEL) {
                paleta.push(color_de_cpixel(crudo));
            }
            pos += bytes_paleta;
            let fila_bytes = match tamano_paleta {
                2 => usize::from(ancho).div_ceil(8),
                3 | 4 => usize::from(ancho).div_ceil(4),
                _ => usize::from(ancho).div_ceil(2),
            };
            let necesarios = fila_bytes * usize::from(alto);
            if datos.len() < pos + necesarios {
                return Ok(None);
            }
            let mut pixeles = Vec::with_capacity(total * 4);
            for fila in 0..usize::from(alto) {
                let fila_datos = &datos[pos + fila * fila_bytes..pos + (fila + 1) * fila_bytes];
                for columna in 0..usize::from(ancho) {
                    let indice = match tamano_paleta {
                        2 => {
                            let byte = fila_datos[columna / 8];
                            usize::from((byte >> (7 - (columna % 8))) & 1)
                        }
                        3 | 4 => {
                            let byte = fila_datos[columna / 4];
                            usize::from((byte >> (6 - 2 * (columna % 4))) & 3)
                        }
                        _ => {
                            let byte = fila_datos[columna / 2];
                            usize::from((byte >> (4 - 4 * (columna % 2))) & 0x0F)
                        }
                    };
                    let Some(color) = paleta.get(indice) else {
                        return Err(datos_invalidos(
                            "paleta de ZRLE con un índice fuera de rango",
                        ));
                    };
                    pixeles.extend_from_slice(color);
                }
            }
            Ok(Some((pos + necesarios, pixeles)))
        }
        128 => decodificar_rle(datos, pos, total),
        130..=255 => decodificar_paleta_rle(datos, pos, total, usize::from(subcodificacion - 128)),
        17..=127 | 129 => Err(datos_invalidos(format!(
            "subcodificación ZRLE no admitida: {subcodificacion}"
        ))),
    }
}

fn leer_longitud(datos: &[u8], pos: &mut usize) -> Option<usize> {
    let mut longitud = 1usize;
    loop {
        let byte = *datos.get(*pos)?;
        *pos += 1;
        longitud += usize::from(byte);
        if byte != 255 {
            return Some(longitud);
        }
    }
}

fn decodificar_rle(
    datos: &[u8],
    mut pos: usize,
    total: usize,
) -> Result<Option<(usize, Vec<u8>)>, ErrorEscritorio> {
    let mut pixeles = Vec::with_capacity(total * 4);
    while pixeles.len() < total * 4 {
        if datos.len() < pos + BYTES_POR_CPIXEL {
            return Ok(None);
        }
        let color = color_de_cpixel(&datos[pos..pos + BYTES_POR_CPIXEL]);
        pos += BYTES_POR_CPIXEL;
        let Some(longitud) = leer_longitud(datos, &mut pos) else {
            return Ok(None);
        };
        if longitud > total - pixeles.len() / 4 {
            return Err(datos_invalidos(
                "corrida RLE de ZRLE más larga que su tesela",
            ));
        }
        for _ in 0..longitud {
            pixeles.extend_from_slice(&color);
        }
    }
    Ok(Some((pos, pixeles)))
}

fn decodificar_paleta_rle(
    datos: &[u8],
    mut pos: usize,
    total: usize,
    tamano_paleta: usize,
) -> Result<Option<(usize, Vec<u8>)>, ErrorEscritorio> {
    let bytes_paleta = tamano_paleta * BYTES_POR_CPIXEL;
    if datos.len() < pos + bytes_paleta {
        return Ok(None);
    }
    let mut paleta = Vec::with_capacity(tamano_paleta);
    for crudo in datos[pos..pos + bytes_paleta].chunks_exact(BYTES_POR_CPIXEL) {
        paleta.push(color_de_cpixel(crudo));
    }
    pos += bytes_paleta;
    let mut pixeles = Vec::with_capacity(total * 4);
    while pixeles.len() < total * 4 {
        let Some(byte) = datos.get(pos).copied() else {
            return Ok(None);
        };
        pos += 1;
        let con_longitud = byte & 0x80 != 0;
        let indice = usize::from(byte & 0x7F);
        let Some(color) = paleta.get(indice) else {
            return Err(datos_invalidos(
                "paleta RLE de ZRLE con un índice fuera de rango",
            ));
        };
        let longitud = if con_longitud {
            let Some(longitud) = leer_longitud(datos, &mut pos) else {
                return Ok(None);
            };
            longitud
        } else {
            1
        };
        if longitud > total - pixeles.len() / 4 {
            return Err(datos_invalidos(
                "corrida RLE de ZRLE más larga que su tesela",
            ));
        }
        for _ in 0..longitud {
            pixeles.extend_from_slice(color);
        }
    }
    Ok(Some((pos, pixeles)))
}

// ------------------------------------------------------------------ pintado

/// Copia la tesela ya decodificada al búfer del rectángulo, comprobando los
/// límites (nunca entra en pánico).
fn copiar_tesela(
    destino: &mut [u8],
    ancho_rect: u16,
    x: u16,
    y: u16,
    ancho: u16,
    alto: u16,
    tesela: &[u8],
) -> Result<(), ErrorEscritorio> {
    let ancho_rect = u32::from(ancho_rect);
    let x = u32::from(x);
    let y = u32::from(y);
    let ancho = u32::from(ancho);
    let alto = u32::from(alto);
    if x + ancho > ancho_rect
        || (y + alto) * ancho_rect > (destino.len() / 4) as u32
        || tesela.len() < (ancho * alto) as usize * 4
    {
        return Err(datos_invalidos(
            "tesela ZRLE fuera del rectángulo de su actualización",
        ));
    }
    for fila in 0..alto {
        let inicio = (((y + fila) * ancho_rect + x) as usize) * 4;
        let fin = inicio + (ancho as usize) * 4;
        let origen = (fila as usize) * (ancho as usize) * 4;
        let Some(destino) = destino.get_mut(inicio..fin) else {
            return Err(datos_invalidos("tesela ZRLE fuera del búfer"));
        };
        destino.copy_from_slice(&tesela[origen..origen + (ancho as usize) * 4]);
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn una_tesela_solida_se_decodifica() {
        let datos = [1u8, 10, 20, 30];
        let (consumidos, pixeles) = decodificar_tesela(&datos, 2, 2).unwrap().unwrap();
        assert_eq!(consumidos, 4);
        assert_eq!(
            pixeles,
            vec![10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255]
        );
    }

    #[test]
    fn una_tesela_cruda_se_decodifica() {
        let datos = [0u8, 1, 2, 3, 4, 5, 6];
        let (consumidos, pixeles) = decodificar_tesela(&datos, 2, 1).unwrap().unwrap();
        assert_eq!(consumidos, 7);
        assert_eq!(pixeles, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn una_tesela_incompleta_no_consume() {
        assert!(decodificar_tesela(&[0u8, 1, 2], 2, 2).unwrap().is_none());
        assert!(decodificar_tesela(&[], 2, 2).unwrap().is_none());
    }

    #[test]
    fn una_corrida_demasiado_larga_es_error() {
        // Tesela de 2×1 (2 píxeles) con una corrida de 3.
        let datos = [128u8, 1, 2, 3, 2];
        assert!(decodificar_tesela(&datos, 2, 1).is_err());
    }
}
