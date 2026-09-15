use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tui_term::widget::{Cursor, PseudoTerminal};

use crate::app::{App, PestanaUI};
use crate::protocolo::EstadoSesionRemota;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let Some(indice) = app.pestana_activa else {
        return;
    };
    let Some(pestaña) = app.pestanas.get(indice) else {
        return;
    };
    // El pty pierde una fila por la barra de pestañas: el tamaño que el
    // servidor conoce es filas de ventana − 3.
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
    let titulo = format!("MAGI · SESIÓN · {}", pestaña.nombre);
    marco.render_widget(
        Paragraph::new(super::linea_marco(&titulo, area.width, &app.tema)),
        trozos[0],
    );
    barra_pestañas(marco, trozos[1], app, indice);
    let contenido = trozos[2];
    if contenido.height == 0 {
        return;
    }
    if pestaña.estado == EstadoSesionRemota::Caida {
        // La última pantalla se conserva en gris detrás del aviso.
        if let Ok(parser) = pestaña.pantalla.lock() {
            let widget = PseudoTerminal::new(parser.screen())
                .cursor(Cursor::default())
                .style(Style::default().fg(app.tema.paleta.inactivo));
            marco.render_widget(widget, contenido);
        }
        aviso_caida(marco, contenido, app, pestaña);
    } else {
        relleno(marco, contenido, app, pestaña);
        if let Ok(parser) = pestaña.pantalla.lock() {
            let widget = PseudoTerminal::new(parser.screen()).cursor(Cursor::default());
            marco.render_widget(widget, contenido);
        }
    }
    marco.render_widget(
        Paragraph::new(barra_estado(app, indice, pestaña)),
        trozos[3],
    );
}

/// Relleno tenue `░` para la zona que no cubre el remoto cuando otra ventana
/// más pequeña compartió la pestaña.
fn relleno(marco: &mut Frame, area: Rect, app: &App, pestaña: &PestanaUI) {
    if pestaña.estado != EstadoSesionRemota::Abierta
        || pestaña.cols_remoto == 0
        || (area.width <= pestaña.cols_remoto && area.height <= pestaña.filas_remoto)
    {
        return;
    }
    let ancho = area.width as usize;
    let alto = area.height as usize;
    let mut lineas = Vec::with_capacity(alto);
    for _ in 0..alto {
        lineas.push(Line::from(Span::styled(
            "░".repeat(ancho),
            Style::default().fg(app.tema.paleta.inactivo),
        )));
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

fn aviso_caida(marco: &mut Frame, area: Rect, app: &App, pestaña: &PestanaUI) {
    let tema = &app.tema;
    let motivo = pestaña
        .motivo
        .clone()
        .unwrap_or_else(|| "conexión perdida".to_string());
    let ancho_aviso = (motivo.chars().count() + 12).min(usize::from(area.width)) as u16;
    let hace = crate::ui::sesion::formatear_duracion(pestaña.segundos());
    let lineas = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{} Sesión caída hace {}", tema.glifos.error, hace),
            Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(motivo, Style::default().fg(tema.paleta.texto))),
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "{} {}  reconectar        {} {}  cerrar",
                app.config.prefijo_escape,
                crate::ui::tecla(tema, "r", "r"),
                app.config.prefijo_escape,
                crate::ui::tecla(tema, "x", "x")
            ),
            Style::default().fg(tema.paleta.inactivo),
        )),
    ];
    let recta = super::centrar(area, ancho_aviso, lineas.len() as u16 + 2);
    marco.render_widget(super::bloque("SESIÓN CAÍDA", tema), recta);
    let interior = Rect {
        x: recta.x + 2,
        y: recta.y + 1,
        width: recta.width.saturating_sub(4),
        height: recta.height.saturating_sub(2),
    };
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Barra de pestañas: glifo + nombre truncado por sesión y `+ nueva` al final.
fn barra_pestañas(marco: &mut Frame, area: Rect, app: &App, activa: usize) {
    let tema = &app.tema;
    let mut spans: Vec<Span> = Vec::new();
    let total = app.pestanas.len();
    // Ventana visible de pestañas: con más de 9 se usan ‹ ›.
    let (primera, ultima_visible) = if total <= 9 {
        (0, total)
    } else {
        let centro = activa;
        (centro.saturating_sub(4), (centro + 5).min(total))
    };
    if primera > 0 {
        spans.push(Span::styled(
            " ‹ ",
            Style::default().fg(tema.paleta.inactivo),
        ));
    }
    for (indice, pestaña) in app
        .pestanas
        .iter()
        .enumerate()
        .skip(primera)
        .take(ultima_visible - primera)
    {
        if indice > primera {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(tema.paleta.inactivo),
            ));
        }
        let glifo = glifo_pestaña(indice == activa, pestaña, tema);
        let mut estilo_nombre = Style::default().fg(tema.paleta.texto);
        if indice == activa {
            estilo_nombre = estilo_nombre.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(
            glifo,
            estilo_pestaña(indice == activa, pestaña, tema),
        ));
        spans.push(Span::styled(
            format!(" {}", truncar(&pestaña.nombre, 14)),
            estilo_nombre,
        ));
    }
    if ultima_visible < total {
        spans.push(Span::styled(
            " › ",
            Style::default().fg(tema.paleta.inactivo),
        ));
    }
    spans.push(Span::styled(
        " │ + nueva",
        Style::default().fg(tema.paleta.inactivo),
    ));
    if total >= 10 {
        spans.push(Span::styled(
            format!("  · {total} sesiones: usa ‹ › o la lista"),
            Style::default().fg(tema.paleta.acento),
        ));
    }
    marco.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn glifo_pestaña(activa: bool, pestaña: &PestanaUI, tema: &crate::tema::Tema) -> String {
    let glifo = match pestaña.estado {
        EstadoSesionRemota::Caida | EstadoSesionRemota::Cerrada => tema.glifos.error, // ✕
        EstadoSesionRemota::Abierta => {
            if pestaña.actividad_no_vista && !activa {
                tema.glifos.conectando // ◐
            } else {
                tema.glifos.conectado // ●
            }
        }
        EstadoSesionRemota::Abriendo => tema.glifos.conectado,
    };
    glifo.to_string()
}

fn estilo_pestaña(activa: bool, pestaña: &PestanaUI, tema: &crate::tema::Tema) -> Style {
    let base = Style::default();
    match pestaña.estado {
        EstadoSesionRemota::Caida => base.fg(tema.paleta.critico),
        _ => {
            if activa {
                base.fg(tema.paleta.correcto).add_modifier(Modifier::BOLD)
            } else if pestaña.actividad_no_vista {
                base.fg(tema.paleta.acento)
            } else {
                base.fg(tema.paleta.correcto)
            }
        }
    }
}

fn truncar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        texto.to_string()
    } else {
        let recortado: String = texto.chars().take(maximo.saturating_sub(1)).collect();
        format!("{recortado}…")
    }
}

fn barra_estado(app: &App, indice: usize, pestaña: &PestanaUI) -> Line<'static> {
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
                " 1-9 n p pestañas · l lista · c conectar · x cerrar · r reconectar · w ventana · q volver · prefijo de nuevo = literal",
                Style::default().fg(tema.paleta.acento),
            ),
        ]);
    }
    let tiempo = crate::ui::sesion::formatear_duracion(pestaña.segundos());
    let carga = app
        .sondeos
        .get(&pestaña.host_id)
        .and_then(|sondeo| {
            let edad = crate::flota::estado::antiguedad_segundos(&sondeo.fecha)?;
            if edad > 600.0 {
                return None;
            }
            sondeo.carga_1m.map(|carga| format!(" · carga {carga:.1}"))
        })
        .unwrap_or_default();
    let identidad = if pestaña.identidad.is_empty() {
        pestaña.motivo.clone().unwrap_or_default()
    } else {
        pestaña.identidad.clone()
    };
    let tamano = if pestaña.cols_remoto > 0 {
        format!(" · {}×{} remoto", pestaña.cols_remoto, pestaña.filas_remoto)
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(
            format!(" {} ", glifo_pestaña(true, pestaña, tema)),
            estilo_pestaña(true, pestaña, tema),
        ),
        Span::styled(
            format!(
                "{}/{} · {} · conectado {} · {}{}{} · {} ventanas · ",
                indice + 1,
                app.pestanas.len(),
                pestaña.host_nombre,
                tiempo,
                identidad,
                tamano,
                carga,
                pestaña.ventanas
            ),
            Style::default().fg(tema.paleta.texto),
        ),
        Span::styled(
            format!("{} ? ayuda", app.config.prefijo_escape),
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
