//! Vista Archivos (F4): dos paneles —local y remoto— del mismo widget, con la
//! cola de transferencias al pie.
//!
//! El panel activo lleva el nombre en ámbar y el otro el marco tenue; las
//! marcas (`≠`, `✕`) se pintan al final de la fila.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::archivos::panel::{EstadoArchivos, Lado, Panel};
use crate::archivos::{fecha_legible, tamano_legible, Marca};
use crate::protocolo::{EstadoTransferencia, InfoTransferencia};
use crate::ui::{centrar, tecla};

/// Filas de la cola al pie de la vista.
pub const ALTO_COLA: u16 = 3;

/// Filas útiles de un panel: la terminal menos la barra de abajo, la cola (si
/// la hay) y los dos bordes del marco.
pub fn alto_panel(terminal_alto: u16, con_cola: bool) -> usize {
    let cola = if con_cola { ALTO_COLA } else { 0 };
    terminal_alto.saturating_sub(1 + cola + 2).max(1) as usize
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let Some(archivos) = &app.archivos else {
        return;
    };
    let con_cola = !archivos.cola.is_empty();
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(if con_cola { ALTO_COLA } else { 0 }),
        ])
        .split(area);

    let paneles = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(trozos[0]);

    dibujar_panel(marco, paneles[0], app, archivos, Lado::Local);
    if archivos.solo_local {
        dibujar_sin_remoto(marco, paneles[1], app, archivos);
    } else {
        dibujar_panel(marco, paneles[1], app, archivos, Lado::Remoto);
    }

    if con_cola {
        dibujar_cola(marco, trozos[1], app, archivos);
    }
    if let Some(aviso) = &archivos.aviso {
        dibujar_aviso(marco, area, app, aviso);
    }
}

fn dibujar_panel(marco: &mut Frame, area: Rect, app: &App, archivos: &EstadoArchivos, lado: Lado) {
    let tema = &app.tema;
    let activo = archivos.activo == lado;
    let panel = match lado {
        Lado::Local => &archivos.local,
        Lado::Remoto => &archivos.remoto,
    };

    let color_borde = if activo {
        tema.paleta.acento
    } else {
        tema.paleta.inactivo
    };
    let mut titulo = format!(
        " {} ",
        acortar(&panel.ruta, area.width.saturating_sub(6) as usize)
    );
    if panel.filtro_activo || !panel.filtro.is_empty() {
        titulo.push_str(&format!("/ {} ", panel.filtro));
    }
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(color_borde))
        .title(Span::styled(
            titulo,
            Style::default().fg(color_borde).add_modifier(if activo {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ))
        .title_bottom(Span::styled(
            format!(" {} ", pie_de(panel)),
            Style::default().fg(tema.paleta.inactivo),
        ));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);

    let lineas = lineas_de(panel, app, activo, interior.width as usize);
    let visibles = interior.height as usize;
    let inicio = panel.desplazamiento.min(lineas.len().saturating_sub(1));
    let trozo: Vec<Line> = lineas.into_iter().skip(inicio).take(visibles).collect();
    marco.render_widget(
        Paragraph::new(trozo).style(Style::default().fg(tema.paleta.texto)),
        interior,
    );
}

/// Panel remoto cuando el host no ofrece SFTP: solo se dice por qué.
fn dibujar_sin_remoto(marco: &mut Frame, area: Rect, app: &App, archivos: &EstadoArchivos) {
    let tema = &app.tema;
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.inactivo))
        .title(Span::styled(
            " sin remoto ",
            Style::default().fg(tema.paleta.inactivo),
        ));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    let motivo = archivos
        .motivo_solo_local
        .clone()
        .unwrap_or_else(|| "este host no ofrece SFTP".to_string());
    let lineas = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {motivo}"),
            Style::default().fg(tema.paleta.critico),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  la vista sigue con el panel local",
            Style::default().fg(tema.paleta.inactivo),
        )),
    ];
    marco.render_widget(Paragraph::new(lineas), interior);
}

fn pie_de(panel: &Panel) -> String {
    let elementos = panel.total_visibles();
    let tamano = tamano_legible(panel.tamano_visible());
    let marcados = match panel.total_marcados() {
        0 => String::new(),
        otros => format!(" · {otros} marcado(s)"),
    };
    format!("{elementos} elementos · {tamano}{marcados}")
}

/// Convierte un panel en líneas de texto, con la fila seleccionada resaltada.
fn lineas_de(panel: &Panel, app: &App, activo: bool, ancho: usize) -> Vec<Line<'static>> {
    let tema = &app.tema;
    let indice = panel.visibles();
    if indice.is_empty() {
        let texto = if panel.error.is_some() {
            "  (no se pudo listar)"
        } else if !panel.filtro.is_empty() {
            "  (el filtro no deja nada)"
        } else {
            "  (vacío)"
        };
        return vec![Line::from(Span::styled(
            texto.to_string(),
            Style::default().fg(tema.paleta.inactivo),
        ))];
    }

    // Columnas: nombre flexible, tamaño 9, fecha 7, marca 2.
    let fijo = 9 + 1 + 7 + 2;
    let ancho_nombre = ancho.saturating_sub(fijo + 2).max(6);

    indice
        .iter()
        .enumerate()
        .map(|(posicion, indice_real)| {
            let entrada = &panel.entradas[*indice_real];
            let seleccionada = posicion == panel.seleccion && activo;
            let marcada = panel.marcados.contains(&entrada.nombre);
            let mut nombre = acortar(&entrada.nombre, ancho_nombre);
            if entrada.tipo == crate::archivos::TipoEntrada::Enlace {
                nombre.push('@');
            }
            let tamano = if entrada.es_dir() {
                "—".to_string()
            } else {
                tamano_legible(entrada.tamano)
            };
            let fecha = fecha_legible(entrada.mtime, app.ahora_epoca());

            let mut estilo = Style::default().fg(tema.paleta.texto);
            if seleccionada {
                estilo = estilo
                    .bg(tema.paleta.acento)
                    .fg(tema.paleta.fondo)
                    .add_modifier(Modifier::BOLD);
            }
            let estilo_marca = if seleccionada {
                estilo
            } else {
                match entrada.marca {
                    Marca::Distinta => Style::default().fg(tema.paleta.acento),
                    Marca::Ausente => Style::default().fg(tema.paleta.critico),
                    Marca::Ninguna => estilo,
                }
            };

            let mut spans = vec![
                Span::styled(
                    format!("{} ", if marcada { tema.glifos.marca } else { " " }),
                    estilo,
                ),
                Span::styled(
                    format!("{nombre:<ancho_nombre$}", ancho_nombre = ancho_nombre),
                    estilo,
                ),
                Span::styled(format!("{tamano:>9} "), estilo),
                Span::styled(format!("{fecha:>7} "), estilo),
            ];
            let glifo = entrada.marca.glifo(tema.ascii);
            spans.push(Span::styled(format!("{glifo:<2}"), estilo_marca));
            Line::from(spans)
        })
        .collect()
}

/// Cola al pie: la transferencia en curso con su barra y lo que espera turno.
fn dibujar_cola(marco: &mut Frame, area: Rect, app: &App, archivos: &EstadoArchivos) {
    let tema = &app.tema;
    let mut lineas = Vec::new();
    if let Some(fila) = archivos.en_curso() {
        lineas.push(linea_en_curso(fila, app));
    } else {
        lineas.push(Line::from(Span::styled(
            "  nada en curso",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let en_cola = archivos
        .cola
        .iter()
        .filter(|fila| fila.estado == EstadoTransferencia::EnCola)
        .count();
    let tamano = tamano_legible(archivos.bytes_en_cola());
    let texto = if en_cola == 0 {
        "  sin nada en cola".to_string()
    } else {
        format!("  en cola {en_cola} · {tamano}")
    };
    lineas.push(Line::from(Span::styled(
        texto,
        Style::default().fg(tema.paleta.texto),
    )));
    lineas.push(Line::from(Span::styled(
        format!(
            "  {} transferencias · {} cancelar {}",
            tecla(tema, "t", "t"),
            tecla(tema, "x", "x"),
            tecla(tema, "C", "C"),
        ),
        Style::default().fg(tema.paleta.inactivo),
    )));
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Una línea con la barra de progreso y los datos de la transferencia.
pub fn linea_en_curso(fila: &InfoTransferencia, app: &App) -> Line<'static> {
    let tema = &app.tema;
    let tanto = fila.porcentaje();
    let ancho_barra = 20usize;
    let rellenos = (usize::from(tanto) * ancho_barra) / 100;
    let barra = crate::ui::flota::barra(f64::from(tanto) / 100.0, ancho_barra, tema.ascii);
    let _ = rellenos;
    Line::from(vec![
        Span::styled(
            format!(" {} ", fila.direccion.glifo(tema.ascii)),
            Style::default().fg(tema.paleta.acento),
        ),
        Span::styled(
            acortar(&destino_corto(fila), 28),
            Style::default().fg(tema.paleta.texto),
        ),
        Span::styled(
            format!(" {barra} "),
            Style::default().fg(if tanto == 100 {
                tema.paleta.correcto
            } else {
                tema.paleta.acento
            }),
        ),
        Span::styled(
            format!(
                "{tanto:>3} % · {} de {}",
                tamano_legible(fila.bytes_hechos),
                tamano_legible(fila.bytes_total)
            ),
            Style::default().fg(tema.paleta.texto),
        ),
    ])
}

/// `origen → host:destino` recortado, que es lo que cabe en la cola.
pub fn destino_corto(fila: &InfoTransferencia) -> String {
    format!(
        "{} → {}:{}",
        nombre_de(&fila.origen),
        fila.host_nombre,
        nombre_de(&fila.destino)
    )
}

fn nombre_de(ruta: &str) -> &str {
    ruta.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(ruta)
}

/// Aviso de subida de ficheros sensibles, centrado y en rojo.
fn dibujar_aviso(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    aviso: &crate::archivos::panel::AvisoPendiente,
) {
    let tema = &app.tema;
    let alto = (aviso.coincidencias.len() as u16) + 7;
    let recta = centrar(area, 74, alto);
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.critico))
        .title(Span::styled(
            " AVISO ",
            Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD),
        ));
    let interior = bloque.inner(recta);
    marco.render_widget(ratatui::widgets::Clear, recta);
    marco.render_widget(bloque, recta);

    let mut lineas = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Vas a subir ficheros que coinciden con los patrones de aviso:",
            Style::default().fg(tema.paleta.texto),
        )),
        Line::from(""),
    ];
    for (nombre, bytes) in &aviso.coincidencias {
        lineas.push(Line::from(Span::styled(
            format!(
                "    {nombre}  ({}),  →  {}",
                tamano_legible(*bytes),
                aviso.destino
            ),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    if aviso.restantes > 0 {
        lineas.push(Line::from(Span::styled(
            format!("    y {} más", aviso.restantes),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    lineas.push(Line::from(""));
    lineas.push(Line::from(Span::styled(
        "  Suelen contener secretos. ¿Subirlos igualmente?",
        Style::default().fg(tema.paleta.texto),
    )));
    lineas.push(Line::from(vec![
        Span::styled(
            format!("  {} subir", tecla(tema, "s", "s")),
            Style::default().fg(tema.paleta.correcto),
        ),
        Span::styled(
            format!("      {} cancelar (recomendado)", tecla(tema, "esc", "esc")),
            Style::default().fg(tema.paleta.acento),
        ),
    ]));
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Recorta un texto al ancho dado, con puntos suspensivos si sobra.
pub fn acortar(texto: &str, ancho: usize) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    let recortado: String = texto.chars().take(ancho.saturating_sub(1)).collect();
    format!("{recortado}…")
}

/// Glifo del estado de una transferencia, para la vista ampliada.
pub fn glifo_estado(estado: EstadoTransferencia, ascii: bool) -> &'static str {
    estado.glifo(ascii)
}
