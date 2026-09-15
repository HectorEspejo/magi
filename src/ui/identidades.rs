use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::flota::estado::{antiguedad_segundos, formatear_antiguedad};
use crate::modelo::Identidad;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let visibles = app.identidades_visibles();
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            " MAGI · IDENTIDADES ",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(
            Line::from(Span::styled(
                format!(
                    "{} clave(s){}{} ",
                    visibles.len(),
                    if app.ver_revocadas {
                        " · con revocadas"
                    } else {
                        ""
                    },
                    if app.identidades_bd.iter().any(Identidad::revocada) {
                        " · v revocadas"
                    } else {
                        ""
                    }
                ),
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
        .constraints([Constraint::Min(4), Constraint::Length(8)])
        .split(interior);

    let mut lineas = vec![cabecera(tema)];
    if visibles.is_empty() {
        lineas.push(Line::from(Span::styled(
            "  no hay identidades; pulsa s para reescanear o n para generar una",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    for (posicion, identidad) in visibles.iter().enumerate() {
        lineas.push(linea_identidad(
            app,
            identidad,
            posicion == app.seleccion_identidad,
        ));
    }
    marco.render_widget(Paragraph::new(lineas), trozos[0]);

    dibujar_detalle(marco, trozos[1], app);
}

fn cabecera(tema: &crate::tema::Tema) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{:<28}", "ALIAS"),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled(
            format!("{:<12}", "TIPO"),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled(
            format!("{:<11}", "USADA EN"),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled("ORIGEN", Style::default().fg(tema.paleta.inactivo)),
    ])
}

fn linea_identidad(app: &App, identidad: &Identidad, seleccionada: bool) -> Line<'static> {
    let tema = &app.tema;
    let usos = app
        .uso_identidades
        .get(&identidad.id)
        .map(Vec::len)
        .unwrap_or(0);
    let usada_en = if usos == 0 {
        "—".to_string()
    } else {
        format!("{usos} host(s)")
    };
    let alias = if identidad.revocada() {
        format!("{} (revocada)", identidad.alias)
    } else {
        identidad.alias.clone()
    };
    let color = if identidad.revocada() {
        tema.paleta.inactivo
    } else {
        tema.paleta.texto
    };
    let mut linea = Line::from(vec![
        Span::raw("  "),
        Span::styled(formato_columna(&alias, 28), Style::default().fg(color)),
        Span::styled(
            formato_columna(&identidad.tipo, 12),
            Style::default().fg(if identidad.revocada() {
                tema.paleta.inactivo
            } else {
                tema.paleta.acento
            }),
        ),
        Span::styled(
            formato_columna(&usada_en, 11),
            Style::default().fg(tema.paleta.texto),
        ),
        Span::styled(
            identidad.origen.como_texto().to_string(),
            Style::default().fg(tema.paleta.inactivo),
        ),
    ]);
    if seleccionada {
        linea = linea.style(
            Style::default()
                .bg(tema.paleta.acento)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD),
        );
    }
    linea
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let Some(identidad) = app.identidad_seleccionada() else {
        return;
    };
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.inactivo));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 {
        return;
    }
    let mut lineas: Vec<Line> = Vec::new();
    lineas.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            identidad.alias.clone(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · {}", identidad.tipo),
            Style::default().fg(tema.paleta.inactivo),
        ),
        if identidad.revocada() {
            Span::styled(" · revocada", Style::default().fg(tema.paleta.critico))
        } else {
            Span::raw("")
        },
    ]));
    lineas.push(campo("Huella", &identidad.huella, tema.paleta.texto));
    lineas.push(campo("Vista", &vista_de(identidad), tema.paleta.texto));
    let fichero = identidad
        .ruta
        .clone()
        .unwrap_or_else(|| "— (solo en el agente)".to_string());
    lineas.push(campo("Fichero", &fichero, tema.paleta.texto));
    let hosts = app
        .uso_identidades
        .get(&identidad.id)
        .map(|nombres| resumir_hosts(nombres))
        .unwrap_or_else(|| "—".to_string());
    lineas.push(campo("Hosts", &hosts, tema.paleta.texto));
    if !app.huellas_escaneadas.contains(&identidad.huella) {
        lineas.push(campo(
            "",
            "no encontrada en el último escaneo",
            tema.paleta.critico,
        ));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

fn campo(etiqueta: &str, valor: &str, color: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{etiqueta:<10}"),
            Style::default().fg(ratatui::style::Color::Reset),
        ),
        Span::styled(valor.to_string(), Style::default().fg(color)),
    ])
}

fn vista_de(identidad: &Identidad) -> String {
    let anadida = fecha_corta(&identidad.anadida_en);
    match &identidad.ultimo_uso_en {
        Some(uso) => {
            let hace = antiguedad_segundos(uso)
                .map(formatear_antiguedad)
                .unwrap_or_else(|| "—".to_string());
            format!("{anadida} · usada hace {hace}")
        }
        None => format!("{anadida} · sin uso"),
    }
}

fn resumir_hosts(nombres: &[String]) -> String {
    const MAXIMO: usize = 4;
    if nombres.len() <= MAXIMO {
        return nombres.join(", ");
    }
    format!(
        "{} +{}",
        nombres[..MAXIMO].join(", "),
        nombres.len() - MAXIMO
    )
}

fn fecha_corta(fecha: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(fecha) {
        Ok(momento) => momento.format("%d/%m/%y").to_string(),
        Err(_) => fecha.to_string(),
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
