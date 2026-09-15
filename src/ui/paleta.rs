use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::centrar;

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
    let mut lineas = Vec::new();
    for (posicion, indice) in paleta.filtradas.iter().enumerate() {
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
