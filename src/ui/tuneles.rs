//! Vista Túneles (F6): los reenvíos definidos en `TUNELES` con el estado en
//! vivo que difunde el servidor, y el resumen del seleccionado al pie.
//!
//! Una fila por túnel definido: el estado sale de `App::tuneles_activos` y, si
//! el servidor no lo tiene en memoria, la fila está inactiva.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::archivos::tamano_legible;
use crate::modelo::{TipoTunel, Tunel};
use crate::protocolo::{EstadoTunelRemoto, InfoTunel, OrigenTunel};
use crate::tema::Tema;
use crate::ui::{archivos::acortar, bloque};

/// Filas del panel de resumen inferior, igual que el detalle de Transferencias.
pub const ALTO_DETALLE: u16 = 5;

/// Filas útiles de la lista: la terminal menos la barra de abajo, el detalle,
/// los dos bordes y la cabecera de columnas. Lo comparte `app.rs` para mover
/// la selección con la misma altura con la que se pinta.
pub fn alto_lista(terminal_alto: u16) -> usize {
    terminal_alto
        .saturating_sub(1 + ALTO_DETALLE + 2 + 1)
        .max(1) as usize
}

/// Anchos de las columnas flexibles, calculados una vez por dibujo.
struct Anchos {
    ruta: usize,
    host: usize,
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let filas = app.tuneles_visibles();
    let activos = filas
        .iter()
        .filter(|tunel| app.estado_de(tunel.id).en_marcha())
        .count();
    let titulo = format!("TÚNELES · {} · {} activos", filas.len(), activos);
    let marco_bloque = bloque(&titulo, tema);

    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(ALTO_DETALLE)])
        .split(area);

    if filas.is_empty() {
        let mensaje = if app.filtro_tuneles.trim().is_empty() {
            "Sin túneles: n para crear uno, o importa los reenvíos de tus hosts"
        } else {
            "sin resultados para el filtro"
        };
        let interior = marco_bloque.inner(area);
        marco.render_widget(marco_bloque, area);
        // Centrado, con una línea de aire por encima.
        let centrado = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(interior);
        let aviso = vec![
            Line::from(""),
            Line::from(Span::styled(
                mensaje,
                Style::default().fg(tema.paleta.inactivo),
            )),
        ];
        marco.render_widget(
            Paragraph::new(aviso).alignment(Alignment::Center),
            centrado[1],
        );
        return;
    }

    let marco_bloque = marco_bloque.title_bottom(Span::styled(
        format!(" {} ", filas.len()),
        Style::default().fg(tema.paleta.inactivo),
    ));
    let interior = marco_bloque.inner(area);
    marco.render_widget(marco_bloque, area);

    // La columna del host se ajusta al nombre más largo y el resto queda para
    // «escucha → destino», que es la que se recorta.
    let ancho = interior.width as usize;
    let ancho_host = filas
        .iter()
        .map(|tunel| tunel.host_nombre.chars().count())
        .max()
        .unwrap_or(6)
        .clamp(6, 18)
        + 1;
    // Sangrías, columnas fijas (estado, tipo) y la marca final.
    let fijos = 2 + 7 + 1 + 8 + 1 + 1 + 1;
    let anchos = Anchos {
        host: ancho_host,
        ruta: ancho.saturating_sub(fijos + ancho_host).max(8),
    };

    let mut lineas: Vec<Line> = vec![Line::from(Span::styled(
        formato_fila(
            "ESTADO",
            "TIPO",
            &acortar("ESCUCHA → DESTINO", anchos.ruta.saturating_sub(1)),
            "HOST",
            "a",
            &anchos,
        ),
        Style::default()
            .fg(tema.paleta.inactivo)
            .add_modifier(Modifier::BOLD),
    ))];

    let altura = interior.height.saturating_sub(1) as usize;
    let inicio = app.desplazamiento_tuneles;
    for (posicion, tunel) in filas.iter().enumerate().skip(inicio).take(altura) {
        let info = app.tuneles_activos.get(&tunel.id);
        let seleccionada = posicion == app.seleccion_tunel;
        lineas.push(linea_de(tunel, info, app, seleccionada, &anchos));
    }
    marco.render_widget(
        Paragraph::new(lineas).style(Style::default().fg(tema.paleta.texto)),
        interior,
    );

    dibujar_detalle(marco, trozos[1], app, &filas);
}

/// Línea de columnas con el mismo formato para la cabecera y las filas.
fn formato_fila(
    estado: &str,
    tipo: &str,
    ruta: &str,
    host: &str,
    marca: &str,
    anchos: &Anchos,
) -> String {
    format!(
        "  {estado:<7} {tipo:<8} {ruta:<ancho_ruta$} {host:<ancho_host$}{marca}",
        ancho_ruta = anchos.ruta,
        ancho_host = anchos.host,
    )
}

fn linea_de(
    tunel: &Tunel,
    info: Option<&InfoTunel>,
    app: &App,
    seleccionada: bool,
    anchos: &Anchos,
) -> Line<'static> {
    let tema = &app.tema;
    let estado = info
        .map(|info| info.estado)
        .unwrap_or(EstadoTunelRemoto::Inactivo);
    let mut estilo = Style::default().fg(tema.paleta.texto);
    if seleccionada {
        estilo = estilo
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
            .add_modifier(Modifier::BOLD);
    }
    // Sobre la fila seleccionada el color del estado es el de la selección.
    let color_estado = if seleccionada {
        estilo
    } else {
        Style::default().fg(color_de(estado, tema))
    };
    Line::from(vec![
        Span::styled(format!("  {:<7} ", estado.glifo(tema.ascii)), color_estado),
        Span::styled(
            format!(
                "{tipo:<8} {ruta:<ancho_ruta$} {host:<ancho_host$}{marca}",
                tipo = tunel.tipo.etiqueta(),
                ruta = acortar(&tramo(tunel, info), anchos.ruta.saturating_sub(1)),
                host = acortar(&tunel.host_nombre, anchos.host.saturating_sub(1)),
                marca = if tunel.automatico { "a" } else { " " },
                ancho_ruta = anchos.ruta,
                ancho_host = anchos.host,
            ),
            estilo,
        ),
    ])
}

fn color_de(estado: EstadoTunelRemoto, tema: &Tema) -> Color {
    match estado {
        EstadoTunelRemoto::Activo => tema.paleta.correcto,
        EstadoTunelRemoto::Activando | EstadoTunelRemoto::Parando => tema.paleta.acento,
        EstadoTunelRemoto::Caido => tema.paleta.critico,
        EstadoTunelRemoto::Inactivo => tema.paleta.inactivo,
    }
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, filas: &[&Tunel]) {
    let tema = &app.tema;
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.inactivo));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);

    let Some(tunel) = filas.get(app.seleccion_tunel) else {
        return;
    };
    let info = app.tuneles_activos.get(&tunel.id);
    let estado = info
        .map(|info| info.estado)
        .unwrap_or(EstadoTunelRemoto::Inactivo);
    let mut lineas = vec![Line::from(Span::styled(
        format!(
            "  {} · {} · {}",
            tunel.host_nombre,
            tunel.nombre,
            tunel.tipo.etiqueta()
        ),
        Style::default().fg(tema.paleta.texto),
    ))];

    let mut detalle = format!(
        "  {} · {} · {}",
        tramo(tunel, info),
        if tunel.automatico {
            "automático"
        } else {
            "manual"
        },
        resumen_actividad(info, estado)
    );
    if let Some(info) = info {
        detalle.push_str(&format!(
            " · \u{2193} {} \u{2191} {}",
            tamano_legible(info.bytes_bajados),
            tamano_legible(info.bytes_subidos)
        ));
    }
    lineas.push(Line::from(Span::styled(
        detalle,
        Style::default().fg(tema.paleta.inactivo),
    )));
    if estado == EstadoTunelRemoto::Caido {
        let error = info
            .and_then(|info| info.ultimo_error.as_deref())
            .unwrap_or("el túnel se ha caído");
        lineas.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// «escucha → destino», o la escucha con la marca de SOCKS5 en dinámico.
pub fn tramo(tunel: &Tunel, info: Option<&InfoTunel>) -> String {
    let escucha = info
        .map(|info| info.escucha_mostrada().to_string())
        .unwrap_or_else(|| tunel.escucha.clone());
    match tunel.tipo {
        TipoTunel::Dinamico => format!("{escucha}  (socks5)"),
        _ => format!(
            "{escucha} → {}",
            tunel.destino.as_deref().unwrap_or("\u{2014}")
        ),
    }
}

/// «activo desde 15:24:03 · 18 m · 1 conexión (3 en total)».
fn resumen_actividad(info: Option<&InfoTunel>, estado: EstadoTunelRemoto) -> String {
    let Some(info) = info else {
        return estado.texto().to_string();
    };
    let mut texto = match info.desde {
        Some(desde) => format!("{} desde {}", estado.texto(), hora_de(desde)),
        None => estado.texto().to_string(),
    };
    if let Some(desde) = info.desde {
        let segundos = (chrono::Utc::now().timestamp() - desde).max(0) as u64;
        texto.push_str(&format!(
            " · {}",
            crate::ui::sesion::formatear_duracion(segundos)
        ));
    }
    texto.push_str(&format!(
        " · {} conexión(es) ({} en total)",
        info.conexiones, info.aceptadas
    ));
    texto
}

/// Hora local `HH:MM:SS` de una época en segundos.
pub fn hora_de(epoca: i64) -> String {
    use chrono::TimeZone as _;
    match chrono::Local.timestamp_opt(epoca, 0).single() {
        Some(fecha) => fecha.format("%H:%M:%S").to_string(),
        None => "\u{2014}".to_string(),
    }
}

/// Texto del origen de un túnel: automático o manual con su ventana.
pub fn texto_origen(info: &InfoTunel) -> String {
    match info.origen {
        Some(OrigenTunel::Automatico) => "automático".to_string(),
        Some(OrigenTunel::Manual) => match info.solicitante {
            Some(ventana) => format!("manual (ventana {ventana})"),
            None => "manual".to_string(),
        },
        None => "\u{2014}".to_string(),
    }
}
