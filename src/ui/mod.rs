pub mod archivos;
pub mod aviso_tamano;
pub mod ayuda;
pub mod barra;
pub mod componentes;
pub mod deliberacion;
pub mod dialogos;
pub mod disposicion;
pub mod edicion;
pub mod ejecutar;
pub mod escritorios;
pub mod ficha;
pub mod flota;
pub mod formulario_snippet;
pub mod hosts;
pub mod identidades;
pub mod pager;
pub mod paleta;
pub mod permisos;
pub mod registro;
pub mod resultados;
pub mod servicios;
pub mod servidor_anterior;
pub mod sesion;
pub mod sesiones;
pub mod sincronizacion;
pub mod sincronizaciones;
pub mod sincronizar;
pub mod snippets;
pub mod transferencias;
pub mod tuneles;

use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use crossterm::cursor::Show;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
    LeaveAlternateScreen,
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
    /// Vista SINCRONIZACIÓN (Fase 10): equipos de la carpeta compartida y su
    /// estado. Sin tecla de función: se abre desde la paleta.
    Sincronizacion,
    /// Vista CONFLICTOS de sincronización (Fase 10).
    Conflictos,
    /// Vista SERVICIOS (Fase 13): servicios y timers de systemd, contenedores
    /// y logs en vivo.
    Servicios,
}

pub type TerminalMagi = Terminal<CrosstermBackend<Stdout>>;

// ------------------------------------------------ terminal exterior (F15)
//
// MAGI pide al terminal exterior el pegado entre corchetes, los eventos de
// foco y, si se admite y `[terminal] teclado_mejorado`, la desambiguación del
// teclado de kitty. El ratón se captura solo cuando hace falta (el remoto del
// panel enfocado lo pide o `[terminal] raton`). Todo se deshace al salir, al
// ceder la terminal (visor, paginador) y en el gancho de pánico (D314).

static PEGADO_CORCHETES: AtomicBool = AtomicBool::new(false);
static FOCO: AtomicBool = AtomicBool::new(false);
static TECLADO_MEJORADO: AtomicBool = AtomicBool::new(false);
static RATON: AtomicBool = AtomicBool::new(false);

/// Pide al terminal exterior lo que MAGI necesita (idempotente).
pub fn activar_extras(config: &crate::config::SeccionTerminal) {
    let mut salida = io::stdout();
    if !PEGADO_CORCHETES.swap(true, Ordering::Relaxed) {
        let _ = execute!(salida, EnableBracketedPaste);
    }
    if !FOCO.swap(true, Ordering::Relaxed) {
        let _ = execute!(salida, EnableFocusChange);
    }
    if config.teclado_mejorado
        && !TECLADO_MEJORADO.load(Ordering::Relaxed)
        && supports_keyboard_enhancement().unwrap_or(false)
        && execute!(
            salida,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .is_ok()
    {
        TECLADO_MEJORADO.store(true, Ordering::Relaxed);
    }
}

/// Deshace lo que MAGI pidió al terminal exterior, en orden inverso.
pub fn desactivar_extras() {
    let mut salida = io::stdout();
    if RATON.swap(false, Ordering::Relaxed) {
        let _ = execute!(salida, DisableMouseCapture);
    }
    if TECLADO_MEJORADO.swap(false, Ordering::Relaxed) {
        let _ = execute!(salida, PopKeyboardEnhancementFlags);
    }
    if FOCO.swap(false, Ordering::Relaxed) {
        let _ = execute!(salida, DisableFocusChange);
    }
    if PEGADO_CORCHETES.swap(false, Ordering::Relaxed) {
        let _ = execute!(salida, DisableBracketedPaste);
    }
}

/// Captura o suelta el ratón del terminal exterior (solo cambia si hace falta).
pub fn fijar_raton(capturado: bool) {
    if capturado == RATON.load(Ordering::Relaxed) {
        return;
    }
    let mut salida = io::stdout();
    if capturado {
        if execute!(salida, EnableMouseCapture).is_ok() {
            RATON.store(true, Ordering::Relaxed);
        }
    } else {
        let _ = execute!(salida, DisableMouseCapture);
        RATON.store(false, Ordering::Relaxed);
    }
}

/// ¿Está el ratón capturado por MAGI? (para las pruebas y la ayuda).
#[must_use]
pub fn raton_capturado() -> bool {
    RATON.load(Ordering::Relaxed)
}

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
    let minimo = disposicion::con_extra(
        disposicion::con_dialogo(
            disposicion::minimo_de(app.vista, app.deliberacion.is_some()),
            app.dialogo.as_ref(),
        ),
        app.servicios_minimo_dialogo(),
    );
    let mut disp = Disposicion::nueva(area, minimo);
    if disp.aviso {
        // Por debajo del mínimo: el aviso sustituye a la vista y a la barra.
        aviso_tamano::dibujar(marco, area, &app.tema, &minimo);
    } else if app.vista == Vista::Servicios && app.servicios.logs.is_some() {
        // El panel de logs ocupa la pantalla entera, con su propio pie.
        servicios::dibujar_logs(marco, area, app, &mut disp);
        if let Some(abierta) = &app.deliberacion {
            deliberacion::dibujar(marco, area, app, abierta, &mut disp);
        }
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
            Vista::Sincronizacion => sincronizacion::dibujar(marco, vista, app, &mut disp),
            Vista::Conflictos => sincronizacion::dibujar(marco, vista, app, &mut disp),
            Vista::Servicios => servicios::dibujar(marco, vista, app, &mut disp),
        }
        barra::dibujar(marco, trozos[1], app, &mut disp);
        // La deliberación es modal sobre cualquier vista; su mínimo entra en
        // el de la pantalla, así que con aviso no se pinta.
        if let Some(abierta) = &app.deliberacion {
            deliberacion::dibujar(marco, area, app, abierta, &mut disp);
        }
    }
    // Los diálogos de Servicios se pintan sobre la vista (y bajo una pregunta
    // del servidor, que se apila encima).
    if let Some(dialogo) = &app.servicios.dialogo {
        servicios::dibujar_dialogo(marco, area, app, dialogo, &mut disp);
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
