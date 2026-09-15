use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tui_term::widget::{Cursor, PseudoTerminal};

use crate::app::App;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let Some(sesion) = &app.sesion else {
        return;
    };
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
    let titulo = format!("MAGI · SESIÓN · {}", sesion.host_nombre);
    marco.render_widget(
        Paragraph::new(super::linea_marco(&titulo, area.width, &app.tema)),
        trozos[0],
    );
    if trozos[1].height > 0 {
        if let Ok(parser) = sesion.pantalla.lock() {
            let widget = PseudoTerminal::new(parser.screen()).cursor(Cursor::default());
            marco.render_widget(widget, trozos[1]);
        }
    }
    marco.render_widget(Paragraph::new(barra_estado(app, sesion)), trozos[2]);
}

fn barra_estado(app: &App, sesion: &crate::app::SesionUI) -> Line<'static> {
    let tema = &app.tema;
    if app.modo_prefijo {
        return Line::from(vec![
            Span::styled(
                format!(" [MAGI] {} ", app.config.prefijo_escape),
                Style::default()
                    .fg(tema.paleta.fondo)
                    .bg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " q volver · x cerrar · prefijo de nuevo para enviarlo literal",
                Style::default().fg(tema.paleta.acento),
            ),
        ]);
    }
    let tiempo = formatear_duracion(sesion.iniciada.elapsed().as_secs());
    Line::from(vec![
        Span::styled(
            format!(" {} ", tema.glifos.conectado),
            Style::default()
                .fg(tema.paleta.correcto)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("conectado {tiempo} · {} · ", sesion.identidad),
            Style::default().fg(tema.paleta.texto),
        ),
        Span::styled(
            format!(
                "{} q volver  {} x cerrar",
                app.config.prefijo_escape, app.config.prefijo_escape
            ),
            Style::default().fg(tema.paleta.inactivo),
        ),
    ])
}

pub fn formatear_duracion(segundos: u64) -> String {
    if segundos < 60 {
        format!("{segundos} s")
    } else if segundos < 3600 {
        format!("{} m", segundos / 60)
    } else {
        format!("{} h {:02} m", segundos / 3600, (segundos % 3600) / 60)
    }
}
