pub mod archivos;
pub mod aviso_tamano;
pub mod ayuda;
pub mod barra;
pub mod componentes;
pub mod deliberacion;
pub mod dialogos;
pub mod disposicion;
pub mod ejecutar;
pub mod ficha;
pub mod flota;
pub mod formulario_snippet;
pub mod hosts;
pub mod identidades;
pub mod pager;
pub mod paleta;
pub mod registro;
pub mod resultados;
pub mod sesion;
pub mod sesiones;
pub mod snippets;
pub mod transferencias;
pub mod tuneles;

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
use disposicion::Disposicion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vista {
    Flota,
    Hosts,
    Ficha,
    Sesion,
    /// Lista de sesiones del servidor (F3 sin pestaña activa).
    Sesiones,
    Identidades,
    Registro,
    /// Vista Archivos (F4): panel doble local ⇄ remoto.
    Archivos,
    /// Cola de transferencias ampliada.
    Transferencias,
    /// Vista Túneles (F5): reenvíos definidos y su estado en vivo.
    Tuneles,
    /// Vista Snippets (F8, Fase 6): comandos guardados y sus destinos.
    Snippets,
    /// Resultados de las ejecuciones de snippets (subvista de F8).
    Resultados,
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

/// Pinta la pantalla entera y devuelve su disposición (modo, aviso y ventanas
/// de las listas), que el bucle guarda para las teclas de página.
pub fn dibujar(marco: &mut Frame, app: &App) -> Disposicion {
    let area = marco.area();
    let minimo = disposicion::minimo_de(app.vista, app.deliberacion.is_some());
    let mut disp = Disposicion::nueva(area, minimo);
    if disp.aviso {
        // Por debajo del mínimo: el aviso sustituye a la vista y a la barra.
        aviso_tamano::dibujar(marco, area, &app.tema, &minimo);
    } else {
        let trozos = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);
        let vista = trozos[0];
        match app.vista {
            Vista::Flota => flota::dibujar(marco, vista, app, &mut disp),
            Vista::Hosts => hosts::dibujar(marco, vista, app, &mut disp),
            Vista::Ficha => ficha::dibujar(marco, vista, app, &mut disp),
            Vista::Sesion => sesion::dibujar(marco, vista, app, &mut disp),
            Vista::Sesiones => sesiones::dibujar(marco, vista, app, &mut disp),
            Vista::Identidades => identidades::dibujar(marco, vista, app, &mut disp),
            Vista::Registro => registro::dibujar(marco, vista, app, &mut disp),
            Vista::Archivos => archivos::dibujar(marco, vista, app, &mut disp),
            Vista::Transferencias => transferencias::dibujar(marco, vista, app, &mut disp),
            Vista::Tuneles => tuneles::dibujar(marco, vista, app, &mut disp),
            Vista::Snippets => snippets::dibujar(marco, vista, app, &mut disp),
            Vista::Resultados => resultados::dibujar(marco, vista, app, &mut disp),
        }
        barra::dibujar(marco, trozos[1], app, &mut disp);
        // La deliberación es modal sobre cualquier vista; su mínimo entra en
        // el de la pantalla, así que con aviso no se pinta.
        if let Some(abierta) = &app.deliberacion {
            deliberacion::dibujar(marco, area, app, abierta, &mut disp);
        }
    }
    // Una pregunta del servidor, la paleta o la ayuda van encima incluso del
    // aviso: encogen con desplazamiento y nunca deben quedar bloqueadas.
    if let Some(dialogo) = &app.dialogo {
        dialogos::dibujar(marco, area, app, dialogo, &mut disp);
    } else if app.paleta.is_some() {
        paleta::dibujar(marco, area, app, &mut disp);
    } else if app.ayuda {
        ayuda::dibujar(marco, area, app, &mut disp);
    }
    disp
}

/// Centra un diálogo de `ancho`×`alto` dejando un margen de una celda; en
/// áreas pequeñas encoge sin salirse nunca del área.
pub fn centrar(area: Rect, ancho: u16, alto: u16) -> Rect {
    let ancho = ancho
        .min(disposicion::ANCHO_MAX_DIALOGO)
        .min(area.width.saturating_sub(2))
        .max(area.width.min(10));
    let alto = alto
        .min(area.height.saturating_sub(2))
        .max(area.height.min(3));
    disposicion::centrar_limitado(area, ancho, alto)
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
