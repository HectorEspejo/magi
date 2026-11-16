//! Pantalla del terminal en tramos de texto con el mismo estilo (Fase 11).
//!
//! La tarea de conexión guarda el último estado del parser y, como mucho cada
//! 33 ms, compara filas y entrega solo las que cambiaron. La app pinta por
//! celdas para que los caracteres anchos no descuadren las columnas.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Intervalo mínimo entre dos pantallas enviadas: 30 fps como máximo.
pub const INTERVALO_ENTRE_PANTALLAS: Duration = Duration::from_millis(33);

/// Color de una celda de la pantalla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    /// El color por defecto del terminal.
    Predeterminado,
    /// Índice de la paleta de 256 colores.
    Indice(u8),
    /// Color de 24 bits.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Traduce el color del parser vt100.
    fn desde_vt100(color: vt100::Color) -> Self {
        match color {
            vt100::Color::Default => Color::Predeterminado,
            vt100::Color::Idx(indice) => Color::Indice(indice),
            vt100::Color::Rgb(rojo, verde, azul) => Color::Rgb(rojo, verde, azul),
        }
    }
}

/// Un tramo de celdas seguidas con el mismo estilo.
///
/// `texto` cubre todas las celdas del tramo: las celdas vacías son espacios y un
/// carácter ancho ocupa dos columnas de la fila (la segunda no se repite).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tramo {
    /// Texto de las celdas seguidas.
    pub texto: String,
    /// Color del texto.
    pub primer_plano: Color,
    /// Color de fondo.
    pub fondo: Color,
    /// Negrita.
    pub negrita: bool,
    /// Cursiva.
    pub cursiva: bool,
    /// Subrayado.
    pub subrayado: bool,
    /// Inverso (primer plano y fondo intercambiados).
    pub inverso: bool,
}

/// Pantalla de una sesión tal como la recibe la app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PantallaTramos {
    /// Identificador de la sesión.
    pub sesion: u64,
    /// Columnas del terminal.
    pub columnas: u16,
    /// Filas del terminal.
    pub filas: u16,
    /// Solo las filas que cambiaron desde la última pantalla, en orden.
    pub filas_cambiadas: Vec<(u16, Vec<Tramo>)>,
    /// Posición del cursor; `None` si el remoto lo oculta.
    pub cursor: Option<(u16, u16)>,
    /// Filas de historial por encima (0 = al final).
    pub desplazamiento: usize,
    /// `true` al abrir, al redimensionar o al volver a la vista.
    pub completa: bool,
}

/// Generador de pantallas por tramos de una sesión.
///
/// Guarda la última fila enviada de cada fila y el último cursor, y la app
/// consume lo que devuelve: solo las filas que cambian y, como mucho, una
/// pantalla cada 33 ms.
#[derive(Debug)]
pub struct GeneradorTramos {
    sesion: u64,
    ultima: Option<Instant>,
    columnas: u16,
    filas: u16,
    filas_previas: Vec<Vec<Tramo>>,
    cursor: Option<(u16, u16)>,
    desplazamiento: usize,
}

impl GeneradorTramos {
    /// Crea el generador de una sesión.
    #[must_use]
    pub fn nuevo(sesion: u64) -> Self {
        Self {
            sesion,
            ultima: None,
            columnas: 0,
            filas: 0,
            filas_previas: Vec::new(),
            cursor: None,
            desplazamiento: 0,
        }
    }

    /// Identificador de la sesión.
    #[must_use]
    pub fn sesion(&self) -> u64 {
        self.sesion
    }

    /// Devuelve la pantalla pendiente, si toca enviarla.
    ///
    /// Como mucho emite una pantalla cada 33 ms (30 fps); la primera, un
    /// cambio de tamaño y las llamadas con `forzar_completa` no esperan.
    pub fn actualizar(
        &mut self,
        parser: &vt100::Parser,
        forzar_completa: bool,
    ) -> Option<PantallaTramos> {
        self.actualizar_en(parser, forzar_completa, Instant::now())
    }

    /// Igual que `GeneradorTramos::actualizar`, con el reloj inyectado.
    ///
    /// La usan las pruebas para no dormir y los llamadores que ya tienen un
    /// reloj propio.
    pub fn actualizar_en(
        &mut self,
        parser: &vt100::Parser,
        forzar_completa: bool,
        ahora: Instant,
    ) -> Option<PantallaTramos> {
        let pantalla = parser.screen();
        let (filas, columnas) = pantalla.size();
        if filas == 0 || columnas == 0 {
            return None;
        }
        let cambio_de_tamano = columnas != self.columnas || filas != self.filas;
        let completa = forzar_completa || cambio_de_tamano;
        if !completa {
            if let Some(ultima) = self.ultima {
                if ahora.saturating_duration_since(ultima) < INTERVALO_ENTRE_PANTALLAS {
                    return None;
                }
            }
        }
        self.ultima = Some(ahora);
        let cursor = if pantalla.hide_cursor() {
            None
        } else {
            Some(pantalla.cursor_position())
        };
        let desplazamiento = pantalla.scrollback();
        let mut cambiadas: Vec<(u16, Vec<Tramo>)> = Vec::new();
        let mut actuales: Vec<Vec<Tramo>> = Vec::with_capacity(usize::from(filas));
        for fila in 0..filas {
            let tramos = tramos_de_fila(pantalla, fila, columnas);
            let cambio = completa || self.filas_previas.get(usize::from(fila)) != Some(&tramos);
            if cambio {
                cambiadas.push((fila, tramos.clone()));
            }
            actuales.push(tramos);
        }
        let nada_nuevo =
            cambiadas.is_empty() && cursor == self.cursor && desplazamiento == self.desplazamiento;
        if !completa && nada_nuevo {
            return None;
        }
        self.columnas = columnas;
        self.filas = filas;
        self.filas_previas = actuales;
        self.cursor = cursor;
        self.desplazamiento = desplazamiento;
        Some(PantallaTramos {
            sesion: self.sesion,
            columnas,
            filas,
            filas_cambiadas: cambiadas,
            cursor,
            desplazamiento,
            completa,
        })
    }
}

/// Convierte una fila de la vista del parser en tramos del mismo estilo.
///
/// La fila es la visible con el desplazamiento de historial que tenga la
/// pantalla en ese momento. El texto cubre todo el ancho: las celdas vacías son
/// espacios (el llamador que quiera el texto recortado usa
/// [`LineaHistorial::texto`](crate::terminal::historial::LineaHistorial::texto)).
#[must_use]
pub fn tramos_de_fila(pantalla: &vt100::Screen, fila: u16, columnas: u16) -> Vec<Tramo> {
    let mut tramos: Vec<Tramo> = Vec::new();
    let mut anterior_ancho = false;
    for columna in 0..columnas {
        if anterior_ancho {
            // La segunda celda de un carácter ancho ya la cubre el carácter.
            anterior_ancho = false;
            continue;
        }
        let Some(celda) = pantalla.cell(fila, columna) else {
            continue;
        };
        anterior_ancho = celda.is_wide();
        let texto = if celda.has_contents() {
            celda.contents()
        } else {
            " "
        };
        let estilo = Estilo::desde(celda);
        let igual_estilo = tramos.last().is_some_and(|ultimo| estilo.coincide(ultimo));
        if igual_estilo {
            if let Some(ultimo) = tramos.last_mut() {
                ultimo.texto.push_str(texto);
            }
        } else {
            tramos.push(Tramo {
                texto: texto.to_string(),
                primer_plano: estilo.primer_plano,
                fondo: estilo.fondo,
                negrita: estilo.negrita,
                cursiva: estilo.cursiva,
                subrayado: estilo.subrayado,
                inverso: estilo.inverso,
            });
        }
    }
    tramos
}

/// Estilo de una celda, para agrupar celdas seguidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Estilo {
    primer_plano: Color,
    fondo: Color,
    negrita: bool,
    cursiva: bool,
    subrayado: bool,
    inverso: bool,
}

impl Estilo {
    fn desde(celda: &vt100::Cell) -> Self {
        Self {
            primer_plano: Color::desde_vt100(celda.fgcolor()),
            fondo: Color::desde_vt100(celda.bgcolor()),
            negrita: celda.bold(),
            cursiva: celda.italic(),
            subrayado: celda.underline(),
            inverso: celda.inverse(),
        }
    }

    fn coincide(self, tramo: &Tramo) -> bool {
        self.primer_plano == tramo.primer_plano
            && self.fondo == tramo.fondo
            && self.negrita == tramo.negrita
            && self.cursiva == tramo.cursiva
            && self.subrayado == tramo.subrayado
            && self.inverso == tramo.inverso
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn generador() -> GeneradorTramos {
        GeneradorTramos::nuevo(7)
    }

    fn tramo_de(tramos: &[Tramo], letra: char) -> &Tramo {
        tramos
            .iter()
            .find(|tramo| tramo.texto.starts_with(letra))
            .expect("falta el tramo")
    }

    #[test]
    fn la_primera_pantalla_es_completa() {
        let mut parser = vt100::Parser::new(3, 10, 0);
        parser.process(b"hola");
        let mut generador = generador();
        let pantalla = generador
            .actualizar(&parser, false)
            .expect("la primera pantalla llega siempre");
        assert!(pantalla.completa);
        assert_eq!(pantalla.sesion, 7);
        assert_eq!(pantalla.columnas, 10);
        assert_eq!(pantalla.filas, 3);
        assert_eq!(pantalla.cursor, Some((0, 4)));
        assert_eq!(pantalla.desplazamiento, 0);
        assert_eq!(pantalla.filas_cambiadas.len(), 3);
        assert_eq!(pantalla.filas_cambiadas[0].0, 0);
        assert_eq!(pantalla.filas_cambiadas[1].0, 1);
        assert_eq!(pantalla.filas_cambiadas[2].0, 2);
        assert_eq!(pantalla.filas_cambiadas[0].1.len(), 1);
        assert_eq!(pantalla.filas_cambiadas[0].1[0].texto, "hola      ");
        assert_eq!(
            pantalla.filas_cambiadas[0].1[0].primer_plano,
            Color::Predeterminado
        );
        assert_eq!(pantalla.filas_cambiadas[2].1[0].texto, "          ");
    }

    #[test]
    fn solo_llegan_las_filas_cambiadas() {
        let mut parser = vt100::Parser::new(3, 10, 0);
        parser.process(b"uno");
        let mut generador = generador();
        let base = Instant::now();
        assert!(generador.actualizar_en(&parser, false, base).is_some());
        parser.process(b"\x1b[2;1Hdos");
        let pantalla = generador
            .actualizar_en(&parser, false, base + Duration::from_millis(40))
            .expect("la fila 1 cambia");
        assert!(!pantalla.completa);
        assert_eq!(pantalla.filas_cambiadas.len(), 1);
        assert_eq!(pantalla.filas_cambiadas[0].0, 1);
        assert!(pantalla.filas_cambiadas[0].1[0].texto.starts_with("dos"));
        assert_eq!(pantalla.cursor, Some((1, 3)));
        // Sin nada nuevo no se envía pantalla.
        assert!(generador
            .actualizar_en(&parser, false, base + Duration::from_millis(80))
            .is_none());
    }

    #[test]
    fn el_cursor_oculto_llega_como_none() {
        let mut parser = vt100::Parser::new(2, 10, 0);
        parser.process(b"x");
        let mut generador = generador();
        let base = Instant::now();
        let primera = generador.actualizar_en(&parser, false, base).unwrap();
        assert_eq!(primera.cursor, Some((0, 1)));
        parser.process(b"\x1b[?25l");
        let oculta = generador
            .actualizar_en(&parser, false, base + Duration::from_millis(40))
            .expect("el cursor cambia");
        assert_eq!(oculta.cursor, None);
        assert!(!oculta.completa);
        assert!(oculta.filas_cambiadas.is_empty());
        parser.process(b"\x1b[?25h");
        let visible = generador
            .actualizar_en(&parser, false, base + Duration::from_millis(80))
            .expect("el cursor vuelve");
        assert_eq!(visible.cursor, Some((0, 1)));
    }

    #[test]
    fn los_estilos_y_los_colores_se_traducen() {
        let mut parser = vt100::Parser::new(1, 40, 0);
        parser.process(
            b"\x1b[1mN\x1b[0m\x1b[3mC\x1b[0m\x1b[4mS\x1b[0m\x1b[7mI\x1b[0m\x1b[41mF\x1b[0m\x1b[31mR\x1b[0m\x1b[91mB\x1b[0m\x1b[38;5;123mV\x1b[0m\x1b[38;2;1;2;3mT\x1b[0m",
        );
        let mut generador = generador();
        let pantalla = generador.actualizar(&parser, false).unwrap();
        let tramos = &pantalla.filas_cambiadas[0].1;
        assert!(tramo_de(tramos, 'N').negrita);
        assert!(tramo_de(tramos, 'C').cursiva);
        assert!(tramo_de(tramos, 'S').subrayado);
        assert!(tramo_de(tramos, 'I').inverso);
        assert_eq!(tramo_de(tramos, 'F').fondo, Color::Indice(1));
        assert_eq!(tramo_de(tramos, 'R').primer_plano, Color::Indice(1));
        assert_eq!(tramo_de(tramos, 'B').primer_plano, Color::Indice(9));
        assert_eq!(tramo_de(tramos, 'V').primer_plano, Color::Indice(123));
        assert_eq!(tramo_de(tramos, 'T').primer_plano, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn los_caracteres_anchos_ocupan_dos_columnas_sin_duplicarse() {
        let mut parser = vt100::Parser::new(1, 10, 0);
        parser.process("\x1b[31m日\x1b[0ma".as_bytes());
        let mut generador = generador();
        let pantalla = generador.actualizar(&parser, false).unwrap();
        let tramos = &pantalla.filas_cambiadas[0].1;
        assert_eq!(tramos.len(), 2);
        assert_eq!(tramos[0].texto, "日");
        assert_eq!(tramos[0].primer_plano, Color::Indice(1));
        assert!(tramos[1].texto.starts_with('a'));
        assert_eq!(tramos[1].texto.chars().count(), 8);
    }

    #[test]
    fn el_redimensionado_fuerza_una_pantalla_completa() {
        let mut parser = vt100::Parser::new(2, 5, 0);
        parser.process(b"a");
        let mut generador = generador();
        let base = Instant::now();
        assert!(generador.actualizar_en(&parser, false, base).is_some());
        parser.screen_mut().set_size(3, 8);
        let pantalla = generador
            .actualizar_en(&parser, false, base + Duration::from_millis(10))
            .expect("el tamaño cambia");
        assert!(pantalla.completa);
        assert_eq!(pantalla.filas, 3);
        assert_eq!(pantalla.columnas, 8);
        assert_eq!(pantalla.filas_cambiadas.len(), 3);
    }

    #[test]
    fn forzar_completa_salta_el_limite_de_treinta_fps() {
        let mut parser = vt100::Parser::new(2, 8, 0);
        parser.process(b"a");
        let mut generador = generador();
        let base = Instant::now();
        assert!(generador.actualizar_en(&parser, false, base).is_some());
        let forzada = generador
            .actualizar_en(&parser, true, base + Duration::from_millis(5))
            .expect("forzada");
        assert!(forzada.completa);
        assert_eq!(forzada.filas_cambiadas.len(), 2);
    }

    #[test]
    fn no_llegan_mas_de_treinta_pantallas_por_segundo() {
        let mut parser = vt100::Parser::new(2, 8, 0);
        parser.process(b"inicio");
        let mut generador = generador();
        let base = Instant::now();
        assert!(generador.actualizar_en(&parser, false, base).is_some());
        let mut pantallas = 0;
        for milisegundos in 1..=1000u64 {
            parser.process(format!("\x1b[1;1H{milisegundos:04}").as_bytes());
            if generador
                .actualizar_en(&parser, false, base + Duration::from_millis(milisegundos))
                .is_some()
            {
                pantallas += 1;
            }
        }
        assert!(
            pantallas <= 30,
            "llegaron {pantallas} pantallas en un segundo"
        );
    }

    #[test]
    fn el_desplazamiento_del_historial_llega_a_la_app() {
        let mut parser = vt100::Parser::new(2, 10, 20);
        for linea in 0..6 {
            parser.process(format!("linea{linea}\r\n").as_bytes());
        }
        let mut generador = generador();
        let base = Instant::now();
        assert!(generador.actualizar_en(&parser, false, base).is_some());
        parser.screen_mut().set_scrollback(1);
        let pantalla = generador
            .actualizar_en(&parser, false, base + Duration::from_millis(40))
            .expect("al desplazar cambia la vista");
        assert_eq!(pantalla.desplazamiento, 1);
        assert!(!pantalla.completa);
        assert!(!pantalla.filas_cambiadas.is_empty());
        parser.screen_mut().set_scrollback(0);
        let al_final = generador
            .actualizar_en(&parser, false, base + Duration::from_millis(80))
            .expect("al volver cambia la vista");
        assert_eq!(al_final.desplazamiento, 0);
    }
}
