use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::modelo::{EntradaRegistro, ResultadoRegistro};

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let estado = &app.registro;
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            " MAGI · REGISTRO ",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(
            Line::from(Span::styled(
                format!(
                    "{} entradas · {} ",
                    estado.total,
                    estado.filtro.etiqueta_tipo()
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
    let mut restricciones = Vec::new();
    if estado.texto_activo {
        restricciones.push(Constraint::Length(1));
    }
    restricciones.push(Constraint::Min(3));
    restricciones.push(Constraint::Length(3));
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints(restricciones)
        .split(interior);
    let mut indice = 0;
    if estado.texto_activo {
        let linea = Line::from(vec![
            Span::styled("/ ", Style::default().fg(tema.paleta.acento)),
            estado
                .campo
                .span(true, Style::default().fg(tema.paleta.texto)),
            Span::styled(
                "  ↵ aplicar · esc limpiar",
                Style::default().fg(tema.paleta.inactivo),
            ),
        ]);
        marco.render_widget(Paragraph::new(linea), trozos[indice]);
        indice += 1;
    }
    let listado = trozos[indice];
    let detalle = trozos[indice + 1];

    let altura = listado.height as usize;
    let inicio = inicio_visible(estado.seleccion, altura);
    let lineas: Vec<Line> = estado
        .entradas
        .iter()
        .enumerate()
        .skip(inicio)
        .take(altura)
        .map(|(posicion, entrada)| {
            linea_entrada(
                entrada,
                posicion == estado.seleccion,
                tema,
                listado.width as usize,
            )
        })
        .collect();
    let lineas = if lineas.is_empty() {
        vec![Line::from(Span::styled(
            "  registro vacío",
            Style::default().fg(tema.paleta.inactivo),
        ))]
    } else {
        lineas
    };
    marco.render_widget(Paragraph::new(lineas), listado);

    if detalle.height > 0 {
        let contenido = match estado.entrada_seleccionada() {
            Some(entrada) => vec![
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", entrada.tipo),
                        Style::default()
                            .fg(tema.paleta.acento)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(
                            "{} · {} · {}",
                            entrada.host_nombre.as_deref().unwrap_or("—"),
                            formatear_fecha(&entrada.fecha),
                            entrada.resultado.como_texto()
                        ),
                        Style::default().fg(tema.paleta.texto),
                    ),
                ]),
                Line::from(Span::styled(
                    format!("   {}", entrada.detalle),
                    Style::default().fg(tema.paleta.inactivo),
                )),
            ],
            None => vec![Line::from(Span::styled(
                "  sin selección",
                Style::default().fg(tema.paleta.inactivo),
            ))],
        };
        marco.render_widget(
            Paragraph::new(contenido).wrap(ratatui::widgets::Wrap { trim: true }),
            detalle,
        );
    }
}

fn inicio_visible(seleccion: usize, altura: usize) -> usize {
    if altura == 0 {
        return 0;
    }
    (seleccion + 1).saturating_sub(altura)
}

fn linea_entrada(
    entrada: &EntradaRegistro,
    seleccionada: bool,
    tema: &crate::tema::Tema,
    ancho: usize,
) -> Line<'static> {
    let color_resultado = match entrada.resultado {
        ResultadoRegistro::Ok => tema.paleta.inactivo,
        ResultadoRegistro::Error => tema.paleta.critico,
    };
    let host = entrada
        .host_nombre
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let mut linea = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            formato_columna(&formatear_fecha(&entrada.fecha), 17),
            Style::default().fg(tema.paleta.texto),
        ),
        Span::styled(
            formato_columna(&entrada.tipo, 20),
            Style::default().fg(tema.paleta.acento),
        ),
        Span::styled(
            formato_columna(&host, 18),
            Style::default().fg(tema.paleta.texto),
        ),
        Span::styled(
            entrada.resultado.como_texto().to_string(),
            Style::default().fg(color_resultado),
        ),
    ]);
    if seleccionada {
        linea = linea.style(
            Style::default()
                .bg(tema.paleta.acento)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD),
        );
    } else if ancho > 70 {
        linea.spans.push(Span::styled(
            format!(
                "   {}",
                recortar(&entrada.detalle, ancho.saturating_sub(62))
            ),
            Style::default().fg(tema.paleta.inactivo),
        ));
    }
    linea
}

fn recortar(texto: &str, ancho: usize) -> String {
    let limpia = texto.replace('\n', " ");
    if limpia.chars().count() > ancho {
        let recorte: String = limpia.chars().take(ancho.saturating_sub(1)).collect();
        format!("{recorte}…")
    } else {
        limpia
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

/// Fecha de una entrada en corto (`15/09 15:41:02`), o la original si falla.
pub fn formatear_fecha(fecha: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(fecha) {
        Ok(momento) => momento.format("%d/%m %H:%M:%S").to_string(),
        Err(_) => fecha.to_string(),
    }
}

/// Fecha larga para el detalle completo.
pub fn formatear_fecha_larga(fecha: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(fecha) {
        Ok(momento) => momento.format("%Y-%m-%d %H:%M:%S").to_string(),
        Err(_) => fecha.to_string(),
    }
}
