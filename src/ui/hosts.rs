use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Fila};
use crate::modelo::UltimoEstado;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let titulo = "MAGI · HOSTS".to_string();
    let recuento = format!("{} hosts ", app.hosts.len());
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            format!(" {titulo} "),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(
            Line::from(Span::styled(
                recuento,
                Style::default().fg(tema.paleta.inactivo),
            ))
            .alignment(Alignment::Right),
        );
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 {
        return;
    }
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(u16::from(app.filtro_activo)),
            Constraint::Min(1),
        ])
        .split(interior);
    if app.filtro_activo {
        let linea = Line::from(vec![
            Span::styled("/ ", Style::default().fg(tema.paleta.acento)),
            Span::styled(
                app.filtro.clone(),
                Style::default()
                    .fg(tema.paleta.texto)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "\u{2503}".to_string(),
                Style::default().fg(tema.paleta.acento),
            ),
        ]);
        marco.render_widget(Paragraph::new(linea), trozos[0]);
    }
    let listado = trozos[1];
    if listado.height == 0 {
        return;
    }
    let lineas = construir_lineas(app);
    let altura = listado.height as usize;
    let mut inicio = app.desplazamiento;
    if app.seleccion < inicio {
        inicio = app.seleccion;
    }
    if app.seleccion >= inicio + altura {
        inicio = app.seleccion + 1 - altura;
    }
    if inicio > lineas.len().saturating_sub(altura) {
        inicio = lineas.len().saturating_sub(altura);
    }
    let visibles: Vec<Line> = lineas.into_iter().skip(inicio).take(altura).collect();
    marco.render_widget(Paragraph::new(visibles), listado);
}

fn construir_lineas(app: &App) -> Vec<Line<'static>> {
    let tema = &app.tema;
    let mut lineas = Vec::with_capacity(app.filas.len());
    for (indice, fila) in app.filas.iter().enumerate() {
        let seleccionada = indice == app.seleccion;
        match fila {
            Fila::Grupo {
                grupo_id: _,
                nombre,
                plegado,
                total,
                filtrado,
            } => {
                let glifo = if *plegado && !*filtrado {
                    tema.glifos.plegado
                } else {
                    tema.glifos.desplegado
                };
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(format!("{glifo} "), Style::default().fg(tema.paleta.acento)),
                    Span::styled(
                        nombre.clone(),
                        Style::default()
                            .fg(tema.paleta.acento)
                            .add_modifier(Modifier::BOLD),
                    ),
                ];
                let relleno = 60usize.saturating_sub(nombre.chars().count() + 4);
                spans.push(Span::raw(" ".repeat(relleno)));
                spans.push(Span::styled(
                    format!("({total})"),
                    Style::default().fg(tema.paleta.inactivo),
                ));
                lineas.push(estilizar(Line::from(spans), seleccionada, app));
            }
            Fila::Host { indice } => {
                let host = &app.hosts[*indice];
                let (glifo, color) = glifo_y_color(app, host.id, host.ultimo_estado);
                let destello = app.destello.as_ref().is_some_and(|(id, _)| *id == host.id);
                let mut spans = vec![
                    Span::raw("    "),
                    Span::styled(
                        format!("{glifo} "),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        formato_columna(&host.nombre, 26),
                        Style::default().fg(tema.paleta.texto),
                    ),
                    Span::styled(
                        formato_columna(&host.direccion, 20),
                        Style::default().fg(tema.paleta.inactivo),
                    ),
                ];
                if app.columna_etiquetas {
                    spans.push(Span::styled(
                        host.etiquetas.join(" "),
                        Style::default().fg(tema.paleta.correcto),
                    ));
                } else {
                    spans.push(Span::styled(
                        formato_columna(host.usuario.as_deref().unwrap_or("-"), 10),
                        Style::default().fg(tema.paleta.texto),
                    ));
                    spans.push(Span::styled(
                        host.puerto.to_string(),
                        Style::default().fg(tema.paleta.texto),
                    ));
                }
                if let Some(salto) = &host.salto_nombre {
                    spans.push(Span::styled(
                        format!("  {} {salto}", tema.glifos.salto),
                        Style::default().fg(tema.paleta.inactivo),
                    ));
                }
                let mut linea = estilizar(Line::from(spans), seleccionada, app);
                if destello && !seleccionada {
                    linea = linea.style(
                        Style::default()
                            .fg(tema.paleta.fondo)
                            .bg(tema.paleta.acento),
                    );
                }
                lineas.push(linea);
            }
        }
    }
    if lineas.is_empty() {
        let mensaje = if app.filtro_activo && !app.filtro.trim().is_empty() {
            "sin resultados para el filtro"
        } else {
            "no hay hosts; pulsa n para crear uno o I para importar"
        };
        lineas.push(Line::from(Span::styled(
            format!("  {mensaje}"),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    lineas
}

fn estilizar(linea: Line<'static>, seleccionada: bool, app: &App) -> Line<'static> {
    if seleccionada {
        linea.style(
            Style::default()
                .bg(app.tema.paleta.acento)
                .fg(app.tema.paleta.fondo)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        linea
    }
}

fn formato_columna(texto: &str, ancho: usize) -> String {
    let texto = if texto.chars().count() > ancho {
        let recortado: String = texto.chars().take(ancho.saturating_sub(1)).collect();
        format!("{recortado}…")
    } else {
        texto.to_string()
    };
    format!("{texto:<ancho$} ")
}

/// Cuántas sesiones vivas hay al host (pestañas en el servidor).
pub fn sesiones_del_host(app: &App, host_id: i64) -> usize {
    app.pestanas
        .iter()
        .filter(|pestaña| pestaña.host_id == host_id && pestaña.viva())
        .count()
}

/// Glifo de estado del host con contador de sesiones: `●`, `●N` (más de una
/// sesión), `◐` (conectando), `✕` (último error) o `○`.
pub fn glifo_y_color(
    app: &App,
    host_id: i64,
    estado: Option<UltimoEstado>,
) -> (String, ratatui::style::Color) {
    let tema = &app.tema;
    let sesiones = sesiones_del_host(app, host_id);
    if sesiones >= 2 {
        return (
            format!("{}{sesiones}", tema.glifos.conectado),
            tema.paleta.correcto,
        );
    }
    if sesiones == 1 {
        return (tema.glifos.conectado.to_string(), tema.paleta.correcto);
    }
    if app.aperturas_pendientes.contains(&host_id) {
        return (tema.glifos.conectando.to_string(), tema.paleta.acento);
    }
    match estado {
        Some(UltimoEstado::Error) => (tema.glifos.error.to_string(), tema.paleta.critico),
        _ => (tema.glifos.desconectado.to_string(), tema.paleta.inactivo),
    }
}
