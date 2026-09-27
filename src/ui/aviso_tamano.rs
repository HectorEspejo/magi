//! Aviso «ventana demasiado pequeña»: sustituye a la vista cuando el área no
//! llega a su mínimo. Dice el tamaño actual, el mínimo y qué lo exige. Si ni
//! el aviso cabe (menos de 20×3), se pinta solo `MAGI c×f`.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tema::Tema;
use crate::ui::disposicion::{Minimo, Tamano, MINIMO_AVISO, MINIMO_GLOBAL};

/// Ancho y alto a partir de los que el aviso lleva borde.
const CON_BORDE: Tamano = Tamano::new(34, 7);

pub fn dibujar(marco: &mut Frame, area: Rect, tema: &Tema, minimo: &Minimo) {
    marco.render_widget(Clear, area);
    let actual = Tamano::de(area);
    if !MINIMO_AVISO.cabe_en(area) {
        diminuto(marco, area, tema, actual);
        return;
    }
    let interior = if CON_BORDE.cabe_en(area) {
        let bloque = Block::default()
            .borders(Borders::ALL)
            .border_set(tema.bordes())
            .border_style(Style::default().fg(tema.paleta.inactivo));
        let interior = bloque.inner(area);
        marco.render_widget(bloque, area);
        interior
    } else {
        area
    };
    let ancho = usize::from(interior.width.saturating_sub(2)).max(1);
    let mut lineas: Vec<Line<'static>> = Vec::new();
    for (indice, texto) in textos(tema, actual, minimo).into_iter().enumerate() {
        let estilo = if indice == 0 {
            Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        for trozo in partir(&texto, ancho) {
            lineas.push(Line::from(Span::styled(trozo, estilo)));
        }
    }
    // Centrado vertical; si no cabe todo, manda la primera línea.
    let alto = interior.height as usize;
    lineas.truncate(alto);
    let margen = (alto - lineas.len()) / 2;
    let zona = Rect {
        y: interior.y + margen as u16,
        height: lineas.len() as u16,
        ..interior
    };
    marco.render_widget(Paragraph::new(lineas).alignment(Alignment::Center), zona);
}

/// Las líneas del aviso: el aviso, «actual · mínimo» y, si el mínimo que se
/// muestra es el global, qué vista necesita más.
pub fn textos(tema: &Tema, actual: Tamano, minimo: &Minimo) -> Vec<String> {
    let ascii = tema.ascii;
    let punto = if ascii { "-" } else { "·" };
    let global_aplica =
        minimo.tamano.cols >= MINIMO_GLOBAL.cols && minimo.tamano.filas >= MINIMO_GLOBAL.filas;
    let por_debajo_del_global =
        global_aplica && (actual.cols < MINIMO_GLOBAL.cols || actual.filas < MINIMO_GLOBAL.filas);
    let mostrado = if por_debajo_del_global {
        MINIMO_GLOBAL
    } else {
        minimo.tamano
    };
    let mut textos = vec![
        format!("{} ventana demasiado pequeña", tema.glifos.error),
        format!(
            "{} {punto} mínimo {}",
            actual.texto(ascii),
            mostrado.texto(ascii)
        ),
    ];
    if mostrado != minimo.tamano {
        textos.push(format!(
            "({} necesita {})",
            minimo.exige,
            minimo.tamano.texto(ascii)
        ));
    }
    textos
}

/// Menos de 20×3: solo `MAGI c×f`, recortado a lo que quepa.
pub fn diminuto(marco: &mut Frame, area: Rect, tema: &Tema, actual: Tamano) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let texto: String = format!("MAGI {}", actual.texto(tema.ascii))
        .chars()
        .take(usize::from(area.width))
        .collect();
    marco.render_widget(
        Paragraph::new(Line::from(Span::styled(
            texto,
            Style::default().fg(tema.paleta.acento),
        ))),
        Rect { height: 1, ..area },
    );
}

/// Parte un texto en líneas de como mucho `ancho` caracteres, por palabras
/// (una palabra más larga que el ancho se corta).
fn partir(texto: &str, ancho: usize) -> Vec<String> {
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        let mut palabra: String = palabra.to_string();
        while palabra.chars().count() > ancho {
            if !actual.is_empty() {
                lineas.push(std::mem::take(&mut actual));
            }
            let cabeza: String = palabra.chars().take(ancho).collect();
            palabra = palabra.chars().skip(ancho).collect();
            lineas.push(cabeza);
        }
        let largo = actual.chars().count();
        if largo > 0 && largo + 1 + palabra.chars().count() > ancho {
            lineas.push(std::mem::take(&mut actual));
        }
        if !actual.is_empty() {
            actual.push(' ');
        }
        actual.push_str(&palabra);
    }
    if !actual.is_empty() {
        lineas.push(actual);
    }
    lineas
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::ui::disposicion::minimo_de;
    use crate::ui::Vista;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn pintar(cols: u16, filas: u16, vista: Vista, tema: &Tema) -> String {
        let mut terminal = Terminal::new(TestBackend::new(cols, filas)).unwrap();
        terminal
            .draw(|marco| {
                let area = marco.area();
                dibujar(marco, area, tema, &minimo_de(vista, false));
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let mut texto = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                texto.push_str(buffer[(x, y)].symbol());
            }
            texto.push('\n');
        }
        texto
    }

    #[test]
    fn archivos_a_36x10_dice_el_global_y_lo_que_necesita_la_vista() {
        let tema = Tema::respaldo();
        let textos = textos(
            &tema,
            Tamano::new(36, 10),
            &minimo_de(Vista::Archivos, false),
        );
        assert_eq!(textos[1], "36×10 · mínimo 40×12");
        assert_eq!(textos[2], "(Archivos necesita 50×14)");
        let pintado = pintar(36, 10, Vista::Archivos, &tema);
        assert!(pintado.contains("necesita 50×14"), "{pintado}");
    }

    #[test]
    fn ac_archivos_a_45x20_dice_minimo_50x14() {
        let tema = Tema::respaldo();
        let textos = textos(
            &tema,
            Tamano::new(45, 20),
            &minimo_de(Vista::Archivos, false),
        );
        assert_eq!(textos[1], "45×20 · mínimo 50×14");
        assert_eq!(textos.len(), 2);
        let pintado = pintar(45, 20, Vista::Archivos, &tema);
        assert!(pintado.contains("mínimo 50×14"), "{pintado}");
    }

    #[test]
    fn sesion_por_debajo_de_40x8() {
        let tema = Tema::respaldo();
        let textos = textos(&tema, Tamano::new(30, 6), &minimo_de(Vista::Sesion, false));
        assert_eq!(textos[1], "30×6 · mínimo 40×8");
    }

    #[test]
    fn por_debajo_de_20x3_solo_magi_y_el_tamano() {
        let tema = Tema::respaldo();
        let pintado = pintar(18, 2, Vista::Hosts, &tema);
        assert_eq!(pintado.lines().next().unwrap().trim_end(), "MAGI 18×2");
        let pintado = pintar(1, 1, Vista::Hosts, &tema);
        assert_eq!(pintado.trim_end(), "M");
    }

    #[test]
    fn en_ascii_no_hay_glifos_unicode() {
        let mut tema = Tema::respaldo();
        tema.ascii = true;
        tema.glifos = crate::tema::Glifos::ascii();
        for (cols, filas) in [(36, 10), (25, 4), (18, 2), (45, 20)] {
            let pintado = pintar(cols, filas, Vista::Archivos, &tema);
            assert!(
                pintado.chars().all(|c| c.is_ascii() || c.is_alphabetic()),
                "glifo Unicode a {cols}×{filas}:\n{pintado}"
            );
        }
    }

    #[test]
    fn partir_por_palabras() {
        assert_eq!(
            partir("ventana demasiado pequeña", 12),
            vec!["ventana", "demasiado", "pequeña"]
        );
        assert_eq!(partir("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }
}
