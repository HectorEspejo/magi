//! Decodificación Hextile (RFC 6143, codificación 5): teselas de 16×16 con
//! fondo, frente y subrectángulos. Todo se valida antes de pintar: la entrada
//! del servidor es no fiable y nunca se entra en pánico.

use super::protocolo::{datos_invalidos, pintar_rectangulo, pixel_a_rgba, Entrada};
use crate::escritorio::ErrorEscritorio;

/// Máscaras del byte de subcodificación de una tesela.
const RAW: u8 = 1;
const FONDO_ESPECIFICADO: u8 = 2;
const FRENTE_ESPECIFICADO: u8 = 4;
const HAY_SUBRECTS: u8 = 8;
const SUBRECTS_CON_COLOR: u8 = 16;

/// Rectángulo Hextile completo, listo para el puente.
pub struct RectanguloHextile {
    pub x: u16,
    pub y: u16,
    pub ancho: u16,
    pub alto: u16,
    pub pixeles: Vec<u8>,
}

/// Estado de una tesela que se está decodificando.
struct Tesela {
    x: u16,
    y: u16,
    ancho: u16,
    alto: u16,
    /// Ya se leyó el byte de subcodificación (el valor 0 es válido).
    leida: bool,
    mascara: u8,
    fondo: Option<[u8; 4]>,
    frente: Option<[u8; 4]>,
    fondo_pintado: bool,
    subrects: Option<u8>,
    subrects_leidos: u8,
}

/// Decodificador incremental de un rectángulo Hextile.
pub struct Hextile {
    x: u16,
    y: u16,
    ancho: u16,
    alto: u16,
    pixeles: Vec<u8>,
    /// Origen de la siguiente tesela.
    tesela_x: u16,
    tesela_y: u16,
    /// Último fondo y frente válidos (se arrastran entre teselas).
    fondo: Option<[u8; 4]>,
    frente: Option<[u8; 4]>,
    tesela: Option<Tesela>,
}

impl Hextile {
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
            tesela_x: 0,
            tesela_y: 0,
            fondo: None,
            frente: None,
            tesela: None,
        }
    }

    /// Consume lo que pueda de la entrada. Devuelve true cuando el rectángulo
    /// ya está completo.
    pub fn avanzar(&mut self, entrada: &mut Entrada) -> Result<bool, ErrorEscritorio> {
        loop {
            if let Some(mut tesela) = self.tesela.take() {
                if !self.avanzar_tesela(&mut tesela, entrada)? {
                    self.tesela = Some(tesela);
                    return Ok(false);
                }
                self.siguiente_tesela(&tesela);
                if self.tesela_y >= self.alto {
                    return Ok(true);
                }
                continue;
            }
            if self.tesela_y >= self.alto {
                return Ok(true);
            }
            let ancho = 16.min(self.ancho - self.tesela_x);
            let alto = 16.min(self.alto - self.tesela_y);
            self.tesela = Some(Tesela {
                x: self.tesela_x,
                y: self.tesela_y,
                ancho,
                alto,
                leida: false,
                mascara: 0,
                fondo: None,
                frente: None,
                fondo_pintado: false,
                subrects: None,
                subrects_leidos: 0,
            });
        }
    }

    /// Devuelve el rectángulo terminado.
    pub fn terminar(self) -> RectanguloHextile {
        RectanguloHextile {
            x: self.x,
            y: self.y,
            ancho: self.ancho,
            alto: self.alto,
            pixeles: self.pixeles,
        }
    }

    fn siguiente_tesela(&mut self, tesela: &Tesela) {
        self.tesela_x += tesela.ancho;
        if self.tesela_x >= self.ancho {
            self.tesela_x = 0;
            self.tesela_y += tesela.alto;
        }
    }

    fn avanzar_tesela(
        &mut self,
        tesela: &mut Tesela,
        entrada: &mut Entrada,
    ) -> Result<bool, ErrorEscritorio> {
        if !tesela.leida {
            // El byte de subcodificación puede valer 0 (tesela del color de
            // fondo heredado), así que hace falta una bandera aparte.
            let Some(mascara) = entrada.u8() else {
                return Ok(false);
            };
            tesela.leida = true;
            tesela.mascara = mascara;
            if mascara & RAW != 0 {
                let necesarios = usize::from(tesela.ancho) * usize::from(tesela.alto) * 4;
                if entrada.disponible() < necesarios {
                    return Ok(false);
                }
                let datos = entrada
                    .tomar(necesarios)
                    .ok_or_else(|| datos_invalidos("Hextile con tesela incompleta"))?;
                self.pintar_tesela_cruda(tesela, datos)?;
                // Tras una tesela cruda no se arrastran fondo ni frente.
                self.fondo = None;
                self.frente = None;
                return Ok(true);
            }
        }
        if !tesela.fondo_pintado {
            // Fondo de la tesela.
            if tesela.mascara & FONDO_ESPECIFICADO != 0 {
                let Some(datos) = entrada.mirar(4) else {
                    return Ok(false);
                };
                let fondo = pixel_a_rgba(datos);
                entrada.tomar(4);
                tesela.fondo = Some(fondo);
                self.fondo = Some(fondo);
            } else {
                let Some(fondo) = self.fondo else {
                    return Err(datos_invalidos(
                        "Hextile sin color de fondo en la primera tesela",
                    ));
                };
                tesela.fondo = Some(fondo);
            }
            // Frente de la tesela (opcional).
            if tesela.mascara & FRENTE_ESPECIFICADO != 0 {
                if tesela.mascara & SUBRECTS_CON_COLOR != 0 {
                    return Err(datos_invalidos(
                        "Hextile con frente especificado y subrectángulos con color",
                    ));
                }
                let Some(datos) = entrada.mirar(4) else {
                    return Ok(false);
                };
                let frente = pixel_a_rgba(datos);
                entrada.tomar(4);
                tesela.frente = Some(frente);
                self.frente = Some(frente);
            } else {
                tesela.frente = self.frente;
            }
            let Some(fondo) = tesela.fondo else {
                return Err(datos_invalidos("Hextile sin color de fondo"));
            };
            if !pintar_rectangulo(
                &mut self.pixeles,
                self.ancho,
                tesela.x,
                tesela.y,
                tesela.ancho,
                tesela.alto,
                fondo,
            ) {
                return Err(datos_invalidos("tesela Hextile fuera del rectángulo"));
            }
            tesela.fondo_pintado = true;
        }
        if tesela.mascara & HAY_SUBRECTS == 0 {
            self.terminar_tesela(tesela);
            return Ok(true);
        }
        let total = match tesela.subrects {
            Some(total) => total,
            None => {
                let Some(total) = entrada.u8() else {
                    return Ok(false);
                };
                tesela.subrects = Some(total);
                total
            }
        };
        while tesela.subrects_leidos < total {
            let con_color = tesela.mascara & SUBRECTS_CON_COLOR != 0;
            let necesarios = if con_color { 6 } else { 2 };
            if entrada.disponible() < necesarios {
                return Ok(false);
            }
            let color = if con_color {
                let datos = entrada
                    .tomar(4)
                    .ok_or_else(|| datos_invalidos("Hextile con subrectángulo incompleto"))?;
                pixel_a_rgba(datos)
            } else {
                let Some(frente) = tesela.frente else {
                    return Err(datos_invalidos(
                        "Hextile con subrectángulos sin color de frente",
                    ));
                };
                frente
            };
            let Some(posiciones) = entrada.tomar(2) else {
                return Ok(false);
            };
            let posicion = posiciones[0];
            let tamano = posiciones[1];
            let x = u16::from(posicion >> 4);
            let y = u16::from(posicion & 0x0F);
            let ancho = u16::from(tamano >> 4) + 1;
            let alto = u16::from(tamano & 0x0F) + 1;
            if x + ancho > tesela.ancho || y + alto > tesela.alto {
                return Err(datos_invalidos(
                    "subrectángulo de Hextile fuera de su tesela",
                ));
            }
            if !pintar_rectangulo(
                &mut self.pixeles,
                self.ancho,
                tesela.x + x,
                tesela.y + y,
                ancho,
                alto,
                color,
            ) {
                return Err(datos_invalidos(
                    "subrectángulo Hextile fuera del rectángulo",
                ));
            }
            tesela.subrects_leidos += 1;
        }
        self.terminar_tesela(tesela);
        Ok(true)
    }

    fn terminar_tesela(&mut self, tesela: &Tesela) {
        self.fondo = tesela.fondo;
        // El frente solo se arrastra si no es una tesela con subrectángulos de
        // colores (RFC 6143).
        self.frente = if tesela.mascara & SUBRECTS_CON_COLOR != 0 {
            None
        } else {
            tesela.frente
        };
    }

    fn pintar_tesela_cruda(
        &mut self,
        tesela: &Tesela,
        datos: &[u8],
    ) -> Result<(), ErrorEscritorio> {
        let ancho_tesela = usize::from(tesela.ancho);
        let alto_tesela = usize::from(tesela.alto);
        if datos.len() < ancho_tesela * alto_tesela * 4 {
            return Err(datos_invalidos("Hextile con tesela cruda incompleta"));
        }
        let ancho_total = usize::from(self.ancho);
        for fila in 0..alto_tesela {
            let destino_inicio =
                ((usize::from(tesela.y) + fila) * ancho_total + usize::from(tesela.x)) * 4;
            let destino_fin = destino_inicio + ancho_tesela * 4;
            let Some(destino) = self.pixeles.get_mut(destino_inicio..destino_fin) else {
                return Err(datos_invalidos("tesela Hextile fuera del rectángulo"));
            };
            let origen_inicio = fila * ancho_tesela * 4;
            let origen_fin = origen_inicio + ancho_tesela * 4;
            let Some(origen) = datos.get(origen_inicio..origen_fin) else {
                return Err(datos_invalidos("tesela Hextile con píxeles de menos"));
            };
            for (pixel, crudo) in destino.chunks_exact_mut(4).zip(origen.chunks_exact(4)) {
                pixel.copy_from_slice(&pixel_a_rgba(crudo));
            }
        }
        Ok(())
    }
}
