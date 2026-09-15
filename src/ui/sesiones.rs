use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::protocolo::EstadoSesionRemota;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .split(area);
    let titulo = format!(
        "MAGI · SESIONES · {} sesiones · {} ventanas",
        app.pestanas.len(),
        app.pestanas
            .iter()
            .map(|pestaña| pestaña.ventanas)
            .sum::<u32>()
    );
    marco.render_widget(
        Paragraph::new(super::linea_marco(&titulo, area.width, &app.tema)),
        trozos[0],
    );
    if app.pestanas.is_empty() {
        let aviso = if app.servidor_incompatible || app.servidor_caido {
            "El servidor de sesiones no está disponible.\nRelánzalo desde el diálogo o ejecuta «magi servidor parar» y vuelve a abrir."
        } else {
            "Sin sesiones abiertas. ↵ en un host, prefijo c o «n» abre una nueva."
        };
        let lineas: Vec<Line> = aviso
            .lines()
            .map(|texto| {
                Line::from(Span::styled(
                    format!("  {texto}"),
                    Style::default().fg(app.tema.paleta.inactivo),
                ))
            })
            .collect();
        marco.render_widget(Paragraph::new(lineas), trozos[1]);
    } else {
        lista(marco, trozos[1], app);
    }
    panel_inferior(marco, trozos[2], app);
    marco.render_widget(Paragraph::new(barra_atajos(app)), trozos[3]);
}

fn lista(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let cabecera = Line::from(Span::styled(
        format!(
            "  {:<9} {:<22} {:<20} {:<8} {:<22} {}",
            "ESTADO", "PESTAÑA", "HOST", "TIEMPO", "IDENTIDAD", "VENT."
        ),
        Style::default().fg(tema.paleta.inactivo),
    ));
    let visibles = area.height.saturating_sub(1) as usize;
    let inicio = app
        .seleccion_sesiones
        .saturating_sub(visibles.saturating_sub(1))
        .max(
            app.seleccion_sesiones
                .saturating_sub(visibles.saturating_sub(1)),
        );
    let mut lineas = vec![cabecera];
    for (indice, pestaña) in app.pestanas.iter().enumerate().skip(inicio).take(visibles) {
        let elegida = indice == app.seleccion_sesiones;
        let flecha = if elegida { "▸" } else { " " };
        let (glifo, color) = match pestaña.estado {
            EstadoSesionRemota::Caida => (tema.glifos.error, tema.paleta.critico),
            EstadoSesionRemota::Abierta => (tema.glifos.conectado, tema.paleta.correcto),
            EstadoSesionRemota::Abriendo => (tema.glifos.conectando, tema.paleta.acento),
            EstadoSesionRemota::Cerrada => (tema.glifos.desconectado, tema.paleta.inactivo),
        };
        let estilo = if elegida {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lineas.push(Line::from(vec![
            Span::styled(flecha, estilo),
            Span::styled(
                format!("{} ", glifo),
                Style::default().fg(color).add_modifier(if elegida {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ),
            Span::styled(
                format!(
                    "{:<21} {:<19} {:<7} {:<21} {}",
                    pestaña.nombre,
                    pestaña.host_nombre,
                    if pestaña.estado == EstadoSesionRemota::Caida {
                        "caída".to_string()
                    } else {
                        crate::ui::sesion::formatear_duracion(pestaña.segundos())
                    },
                    if pestaña.identidad.is_empty() {
                        "—".to_string()
                    } else {
                        pestaña.identidad.clone()
                    },
                    pestaña.ventanas
                ),
                estilo.fg(tema.paleta.texto),
            ),
        ]));
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

fn panel_inferior(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let separador = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(tema.paleta.inactivo),
    ));
    let mut lineas = vec![separador];
    match app.pestanas.get(app.seleccion_sesiones) {
        Some(pestaña) => {
            let detalle = if pestaña.estado == EstadoSesionRemota::Caida {
                format!(
                    "{} · caída: {}",
                    pestaña.nombre,
                    pestaña
                        .motivo
                        .clone()
                        .unwrap_or_else(|| "motivo desconocido".to_string())
                )
            } else {
                format!("{} · {}", pestaña.nombre, pestaña.identidad)
            };
            lineas.push(Line::from(Span::styled(
                format!("  {detalle}"),
                Style::default().fg(tema.paleta.texto),
            )));
        }
        None => lineas.push(Line::from(Span::styled(
            "  Selecciona una sesión de la lista.",
            Style::default().fg(tema.paleta.inactivo),
        ))),
    }
    let servidor = match (app.servidor_pid, app.servidor_desde()) {
        (Some(pid), Some(desde)) => format!(
            "  servidor pid {pid} · protocolo {} · desde {:?}",
            crate::protocolo::VERSION_PROTOCOLO,
            desde.elapsed(),
        ),
        (Some(pid), None) => format!(
            "  servidor pid {pid} · protocolo {}",
            crate::protocolo::VERSION_PROTOCOLO
        ),
        _ => "  sin servidor de sesiones".to_string(),
    };
    lineas.push(Line::from(Span::styled(
        servidor,
        Style::default().fg(tema.paleta.inactivo),
    )));
    marco.render_widget(Paragraph::new(lineas), area);
}

fn barra_atajos(app: &App) -> Line<'static> {
    let tema = &app.tema;
    Line::from(vec![
        Span::styled(
            " ↵ entrar   n nueva   r reconectar   x cerrar   S apagar",
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled(
            format!("   {} l pestañas", app.config.prefijo_escape),
            Style::default().fg(tema.paleta.inactivo),
        ),
    ])
}
