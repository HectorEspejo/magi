//! Vista Resultados (subvista de F8): ejecuciones y salida por host.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::bloque;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let marco_bloque = bloque("MAGI · RESULTADOS", tema);
    let interior = marco_bloque.inner(area);
    marco.render_widget(marco_bloque, area);
    let centrado = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(interior);
    marco.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Sin ejecuciones todavía",
                Style::default().fg(tema.paleta.inactivo),
            )),
        ])
        .alignment(Alignment::Center),
        centrado[1],
    );
}
