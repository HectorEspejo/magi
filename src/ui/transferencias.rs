//! Vista Transferencias (F4): la cola ampliada, con el detalle de la
//! seleccionada al pie (bytes, velocidad y tiempo restante calculados con las
//! dos últimas difusiones del servidor).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::archivos::tamano_legible;
use crate::protocolo::{EstadoTransferencia, InfoTransferencia};
use crate::ui::{bloque, centrar, tecla};

/// Filas del panel de detalle inferior.
const ALTO_DETALLE: u16 = 5;

/// Filas útiles de la lista: la terminal menos la barra de abajo, el detalle,
/// los dos bordes y la cabecera de columnas.
pub fn alto_lista(terminal_alto: u16) -> usize {
    terminal_alto
        .saturating_sub(1 + ALTO_DETALLE + 2 + 1)
        .max(1) as usize
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let en_curso = app
        .archivos
        .as_ref()
        .map(|archivos| {
            archivos
                .cola
                .iter()
                .filter(|fila| fila.estado == EstadoTransferencia::EnCurso)
                .count()
        })
        .unwrap_or(0);
    let en_cola = app
        .archivos
        .as_ref()
        .map(|archivos| {
            archivos
                .cola
                .iter()
                .filter(|fila| fila.estado == EstadoTransferencia::EnCola)
                .count()
        })
        .unwrap_or(0);

    let titulo = format!("TRANSFERENCIAS · {en_curso} en curso · {en_cola} en cola");
    let mut marco_bloque = bloque(&titulo, tema);

    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(ALTO_DETALLE)])
        .split(area);

    let filas = app
        .archivos
        .as_ref()
        .map(|a| a.cola.clone())
        .unwrap_or_default();
    if filas.is_empty() {
        let aviso = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No hay transferencias: la cola está vacía.",
                Style::default().fg(tema.paleta.inactivo),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Copia algo con c desde Archivos y aparecerá aquí.",
                Style::default().fg(tema.paleta.inactivo),
            )),
        ];
        marco.render_widget(marco_bloque.clone(), area);
        marco.render_widget(Paragraph::new(aviso), trozos[0]);
        return;
    }

    marco_bloque = marco_bloque
        .title_bottom(Span::styled(
            format!(" {} ", filas.len()),
            Style::default().fg(tema.paleta.inactivo),
        ))
        .clone();
    let interior = marco_bloque.inner(area);
    marco.render_widget(marco_bloque, area);

    // Cabecera de columnas y filas.
    let ancho = interior.width as usize;
    let mut lineas: Vec<Line> = vec![Line::from(Span::styled(
        format!(
            "  {:<9} {:<8} {:<34} {:>7}",
            "ESTADO", "DIRECCIÓN", "ORIGEN → DESTINO", "PROGRESO"
        ),
        Style::default()
            .fg(tema.paleta.inactivo)
            .add_modifier(Modifier::BOLD),
    ))];
    let alto_lista = interior.height.saturating_sub(2) as usize;
    let inicio = app.desplazamiento_cola;
    for (posicion, fila) in filas.iter().enumerate().skip(inicio).take(alto_lista) {
        let seleccionada = posicion == app.seleccion_cola;
        lineas.push(linea_de(fila, app, seleccionada, ancho));
    }
    marco.render_widget(
        Paragraph::new(lineas).style(Style::default().fg(tema.paleta.texto)),
        interior,
    );

    dibujar_detalle(marco, trozos[1], app, &filas);
}

fn linea_de(
    fila: &InfoTransferencia,
    app: &App,
    seleccionada: bool,
    ancho: usize,
) -> Line<'static> {
    let tema = &app.tema;
    let mut estilo = Style::default().fg(tema.paleta.texto);
    if seleccionada {
        estilo = estilo
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
            .add_modifier(Modifier::BOLD);
    }
    let color_estado = if seleccionada {
        estilo
    } else {
        match fila.estado {
            EstadoTransferencia::Error => Style::default().fg(tema.paleta.critico),
            EstadoTransferencia::Hecha => Style::default().fg(tema.paleta.correcto),
            EstadoTransferencia::EnCurso => Style::default().fg(tema.paleta.acento),
            _ => Style::default().fg(tema.paleta.inactivo),
        }
    };
    let ruta = crate::ui::archivos::acortar(
        &format!("{} → {}:{}", fila.origen, fila.host_nombre, fila.destino),
        ancho.saturating_sub(30),
    );
    let progreso = if let Some(error) = &fila.error {
        crate::ui::archivos::acortar(error, 24)
    } else if fila.es_directorio && fila.estado.terminada() {
        format!("{} fichero(s)", fila.ficheros_hechos)
    } else {
        format!(
            "{:>3} %  {}",
            fila.porcentaje(),
            tamano_legible(fila.bytes_hechos)
        )
    };
    Line::from(vec![
        Span::styled(
            format!(
                " {} {:<7}",
                fila.estado.glifo(tema.ascii),
                fila.estado.texto()
            ),
            color_estado,
        ),
        Span::styled(format!("{:<8}", fila.direccion.texto()), estilo),
        Span::styled(format!("{ruta:<34}"), estilo),
        Span::styled(progreso, color_estado),
    ])
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, filas: &[InfoTransferencia]) {
    let tema = &app.tema;
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.inactivo));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);

    let Some(fila) = filas.get(app.seleccion_cola) else {
        return;
    };
    let mut lineas = vec![Line::from(Span::styled(
        format!("  {} → {}:{}", fila.origen, fila.host_nombre, fila.destino),
        Style::default().fg(tema.paleta.texto),
    ))];
    let velocidad = app
        .archivos
        .as_ref()
        .and_then(|archivos| archivos.velocidad(fila));
    let mut detalle = format!(
        "  {} de {} · {} fichero(s)",
        tamano_legible(fila.bytes_hechos),
        tamano_legible(fila.bytes_total),
        fila.ficheros_hechos
    );
    if let Some(velocidad) = velocidad {
        if velocidad > 0.0 {
            detalle.push_str(&format!(
                " · {}/s · {} restantes",
                tamano_legible(velocidad as u64),
                restante(fila, velocidad)
            ));
        }
    }
    if fila.omitidos > 0 {
        detalle.push_str(&format!(" · {} omitido(s)", fila.omitidos));
    }
    lineas.push(Line::from(Span::styled(
        detalle,
        Style::default().fg(tema.paleta.inactivo),
    )));
    if let Some(fichero) = &fila.fichero_actual {
        lineas.push(Line::from(Span::styled(
            format!("  ahora: {fichero}"),
            Style::default().fg(tema.paleta.acento),
        )));
    } else if let Some(error) = &fila.error {
        lineas.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Tiempo restante estimado con la velocidad medida.
fn restante(fila: &InfoTransferencia, velocidad: f64) -> String {
    let pendientes = fila.bytes_total.saturating_sub(fila.bytes_hechos) as f64;
    let segundos = pendientes / velocidad;
    if !segundos.is_finite() || segundos < 0.0 {
        return "—".to_string();
    }
    if segundos < 60.0 {
        format!("{segundos:.0} s")
    } else if segundos < 3600.0 {
        format!("{:.0} min", segundos / 60.0)
    } else {
        format!("{:.1} h", segundos / 3600.0)
    }
}

/// Aviso modal que se pinta sobre la vista (confirmaciones de cancelar).
pub fn confirmar(marco: &mut Frame, area: Rect, app: &App, lineas: Vec<String>) {
    let tema = &app.tema;
    let alto = (lineas.len() as u16) + 4;
    let recta = centrar(area, 70, alto);
    let contenido: Vec<Line> = lineas
        .into_iter()
        .map(|texto| Line::from(Span::styled(texto, Style::default().fg(tema.paleta.texto))))
        .collect();
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.acento))
        .title(Span::styled(
            " CONFIRMAR ",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    marco.render_widget(ratatui::widgets::Clear, recta);
    marco.render_widget(bloque, recta);
    marco.render_widget(Paragraph::new(contenido), recta);
}

/// Atajos de la vista, para la barra de abajo.
pub fn atajos(app: &App) -> Vec<(&'static str, &'static str)> {
    vec![
        (tecla(&app.tema, "↓↑", "j/k"), "mover"),
        ("x", "cancelar"),
        ("C", "limpiar terminadas"),
        ("↵", "detalle"),
        ("?", "ayuda"),
        ("q", "volver"),
    ]
}
