use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::centrar;

/// Primera entrada que se pinta con `visibles` filas: la selección queda
/// siempre dentro de la ventana (abajo del todo si hace falta bajarla).
pub fn inicio_ventana(seleccion: usize, visibles: usize) -> usize {
    seleccion.saturating_sub(visibles.saturating_sub(1))
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let Some(paleta) = &app.paleta else {
        return;
    };
    let tema = &app.tema;
    let recta = centrar(area, 72, 16);
    let bloque = super::bloque("PALETA · ^p", tema);
    let interior = bloque.inner(recta);
    marco.render_widget(Clear, recta);
    marco.render_widget(bloque, recta);
    if interior.height == 0 {
        return;
    }
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(interior);
    let entrada = Line::from(vec![
        Span::styled("  ❯ ", Style::default().fg(tema.paleta.acento)),
        paleta
            .consulta
            .span(true, Style::default().fg(tema.paleta.texto)),
    ]);
    marco.render_widget(Paragraph::new(entrada), trozos[0]);
    // Solo caben `visibles` entradas: la ventana sigue a la selección para
    // que nunca quede por debajo de lo que se ve.
    let visibles = trozos[1].height as usize;
    let inicio = inicio_ventana(paleta.seleccion, visibles);
    let mut lineas = Vec::new();
    for (posicion, indice) in paleta
        .filtradas
        .iter()
        .enumerate()
        .skip(inicio)
        .take(visibles)
    {
        let entrada = &paleta.entradas[*indice];
        let seleccionada = posicion == paleta.seleccion;
        let relleno = 60usize.saturating_sub(entrada.etiqueta.chars().count() + 2);
        let estilo = if seleccionada {
            Style::default()
                .bg(tema.paleta.acento)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        let estilo_categoria = if seleccionada {
            estilo
        } else {
            Style::default().fg(tema.paleta.inactivo)
        };
        lineas.push(Line::from(vec![
            Span::styled(
                format!(
                    " {}{}",
                    if seleccionada { "▸ " } else { "  " },
                    entrada.etiqueta
                ),
                estilo,
            ),
            Span::styled(" ".repeat(relleno.max(2)), estilo),
            Span::styled(entrada.categoria.to_string(), estilo_categoria),
        ]));
    }
    if lineas.is_empty() {
        lineas.push(Line::from(Span::styled(
            "  sin coincidencias",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    marco.render_widget(Paragraph::new(lineas), trozos[1]);
}

#[cfg(test)]
mod pruebas {
    use super::inicio_ventana;

    #[test]
    fn la_ventana_sigue_a_la_seleccion() {
        // Mientras la selección cabe, se pinta desde el principio.
        assert_eq!(inicio_ventana(0, 14), 0);
        assert_eq!(inicio_ventana(13, 14), 0);
        // Por debajo, la ventana baja lo justo para que sea la última fila.
        assert_eq!(inicio_ventana(14, 14), 1);
        assert_eq!(inicio_ventana(40, 14), 27);
        let inicio = inicio_ventana(40, 14);
        assert!((inicio..inicio + 14).contains(&40));
        // Sin filas o con una sola, la ventana empieza en la selección.
        assert_eq!(inicio_ventana(5, 1), 5);
        assert_eq!(inicio_ventana(5, 0), 5);
    }
}
