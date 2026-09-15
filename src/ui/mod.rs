pub mod ayuda;
pub mod barra;
pub mod componentes;
pub mod dialogos;
pub mod ficha;
pub mod flota;
pub mod hosts;
pub mod identidades;
pub mod paleta;
pub mod registro;
pub mod sesion;

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};

use crate::app::App;
use crate::tema::Tema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vista {
    Flota,
    Hosts,
    Ficha,
    Sesion,
    Identidades,
    Registro,
}

pub type TerminalMagi = Terminal<CrosstermBackend<Stdout>>;

pub fn iniciar_terminal() -> Result<TerminalMagi> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    Ok(terminal)
}

pub fn restaurar_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

pub fn dibujar(marco: &mut Frame, app: &App) {
    let area = marco.area();
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    match app.vista {
        Vista::Flota => flota::dibujar(marco, trozos[0], app),
        Vista::Hosts => hosts::dibujar(marco, trozos[0], app),
        Vista::Ficha => ficha::dibujar(marco, trozos[0], app),
        Vista::Sesion => sesion::dibujar(marco, trozos[0], app),
        Vista::Identidades => identidades::dibujar(marco, trozos[0], app),
        Vista::Registro => registro::dibujar(marco, trozos[0], app),
    }
    barra::dibujar(marco, trozos[1], app);
    if let Some(dialogo) = &app.dialogo {
        dialogos::dibujar(marco, area, app, dialogo);
    } else if app.paleta.is_some() {
        paleta::dibujar(marco, area, app);
    } else if app.ayuda {
        ayuda::dibujar(marco, area, app);
    }
}

pub fn centrar(area: Rect, ancho: u16, alto: u16) -> Rect {
    let ancho = ancho.min(area.width.saturating_sub(2)).max(10);
    let alto = alto.min(area.height.saturating_sub(2)).max(3);
    Rect {
        x: area.x + (area.width.saturating_sub(ancho)) / 2,
        y: area.y + (area.height.saturating_sub(alto)) / 2,
        width: ancho,
        height: alto,
    }
}

pub fn bloque(titulo: &str, tema: &Tema) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            format!(" {titulo} "),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ))
}

/// Línea de marco superior de la sesión, con relleno.
pub fn linea_marco(titulo: &str, ancho: u16, tema: &Tema) -> Line<'static> {
    let caracter = if tema.ascii { "-" } else { "─" };
    let encabezado = format!(" {titulo} ");
    let relleno = (ancho as usize).saturating_sub(encabezado.chars().count() + 1);
    Line::from(vec![
        Span::styled(
            encabezado,
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            caracter.repeat(relleno),
            Style::default().fg(tema.paleta.inactivo),
        ),
    ])
}

/// Atajo de teclado legible en modo Unicode o ASCII.
pub fn tecla(tema: &Tema, unicode: &'static str, ascii: &'static str) -> &'static str {
    if tema.ascii {
        ascii
    } else {
        unicode
    }
}

pub fn parrafo_lineas(lineas: Vec<Line<'static>>, tema: &Tema) -> Paragraph<'static> {
    Paragraph::new(lineas).style(Style::default().fg(tema.paleta.texto))
}
