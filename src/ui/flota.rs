use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::flota::estado::{self, EstadoFlota};
use crate::modelo::{ResultadoSondeo, Sondeo};

const ANCHO_BARRAS: usize = 16;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let derecha = cabecera_derecha(app);
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            " MAGI · FLOTA ",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(
            Line::from(Span::styled(
                format!("{derecha} "),
                Style::default().fg(tema.paleta.inactivo),
            ))
            .alignment(Alignment::Right),
        );
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 || interior.width < 20 {
        return;
    }
    let columnas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(interior);
    dibujar_lista(marco, columnas[0], app);
    if columnas.len() > 1 {
        dibujar_detalle(marco, columnas[1], app);
    }
}

fn cabecera_derecha(app: &App) -> String {
    if !app.sondeando.is_empty() {
        return format!("sondeando {}/{} ", app.sondeo_hechos, app.sondeo_total);
    }
    let total = app.hosts.len();
    let nominales = app
        .hosts
        .iter()
        .filter(|host| estado_de(app, host.id).0 == EstadoFlota::Nominal)
        .count();
    let antiguedad = app
        .sondeos
        .values()
        .filter_map(|sondeo| estado::antiguedad_segundos(&sondeo.fecha))
        .fold(f64::INFINITY, f64::min);
    let hace = if antiguedad.is_finite() {
        format!(
            " · sondeo hace {}",
            estado::formatear_antiguedad(antiguedad)
        )
    } else {
        String::new()
    };
    format!("{total} hosts · {nominales} nominal{hace} ")
}

fn estado_de(app: &App, host_id: i64) -> (EstadoFlota, Vec<String>) {
    estado::evaluar(app.sondeos.get(&host_id), &app.config.flota.umbrales)
}

fn dibujar_lista(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let mut lineas: Vec<Line> = Vec::new();
    if app.hosts.is_empty() {
        lineas.push(Line::from(Span::styled(
            "  Sin hosts: pulsa n en Hosts o I para importar",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    if app.filtro_activo {
        lineas.push(Line::from(vec![
            Span::styled("/ ", Style::default().fg(tema.paleta.acento)),
            Span::styled(
                app.filtro.clone(),
                Style::default()
                    .fg(tema.paleta.texto)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("\u{2503}", Style::default().fg(tema.paleta.acento)),
        ]));
    }
    for (posicion, indice) in app.hosts_flota().iter().enumerate() {
        let host = &app.hosts[*indice];
        let seleccionado = posicion == app.seleccion_flota;
        let (glifo, color, palabra) = if app.sondeando.contains(&host.id) {
            (tema.glifos.conectando.to_string(), tema.paleta.acento, "…")
        } else {
            let (estado, _) = estado_de(app, host.id);
            let (glifo, color, palabra) = glifo_estado(estado, tema);
            // ●N cuando hay más de una sesión viva al host.
            let sesiones = super::hosts::sesiones_del_host(app, host.id);
            if sesiones >= 2 {
                (format!("{glifo}{sesiones}"), color, palabra)
            } else {
                (glifo.to_string(), color, palabra)
            }
        };
        let nombre_max = (area.width as usize).saturating_sub(12);
        let nombre = recortar(&host.nombre, nombre_max.max(4));
        let relleno = (area.width as usize).saturating_sub(10 + nombre.chars().count());
        let mut linea = Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{glifo} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<8}", palabra), Style::default().fg(color)),
            Span::styled(nombre, Style::default().fg(tema.paleta.texto)),
            Span::raw(" ".repeat(relleno.min(20))),
        ]);
        if seleccionado {
            linea = linea.style(
                Style::default()
                    .bg(tema.paleta.acento)
                    .fg(tema.paleta.fondo)
                    .add_modifier(Modifier::BOLD),
            );
        }
        lineas.push(linea);
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

fn glifo_estado(
    estado: EstadoFlota,
    tema: &crate::tema::Tema,
) -> (&'static str, ratatui::style::Color, &'static str) {
    match estado {
        EstadoFlota::Fria => (tema.glifos.desconectado, tema.paleta.inactivo, "FRÍA"),
        EstadoFlota::Caida => (tema.glifos.error, tema.paleta.critico, "CAÍDA"),
        EstadoFlota::Alcanzable => (tema.glifos.conectado, tema.paleta.inactivo, "ALCANZ."),
        EstadoFlota::Carga => (tema.glifos.conectando, tema.paleta.acento, "CARGA"),
        EstadoFlota::Nominal => (tema.glifos.conectado, tema.paleta.correcto, "NOMINAL"),
    }
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let Some(host) = app.host_flota_seleccionado() else {
        return;
    };
    let mut lineas: Vec<Line> = Vec::new();
    lineas.push(Line::from(Span::styled(
        host.nombre.clone(),
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    )));
    lineas.push(Line::from(Span::styled(
        format!("{}:{}", host.direccion, host.puerto),
        Style::default().fg(tema.paleta.inactivo),
    )));
    lineas.push(Line::from(""));
    let Some(sondeo) = app.sondeos.get(&host.id) else {
        lineas.push(Line::from(Span::styled(
            "  sin sondeo todavía",
            Style::default().fg(tema.paleta.inactivo),
        )));
        lineas.push(Line::from(""));
        lineas.push(Line::from(Span::styled(
            "  pulsa r para sondear este host",
            Style::default().fg(tema.paleta.inactivo),
        )));
        marco.render_widget(Paragraph::new(lineas), area);
        return;
    };
    let (estado_actual, culpables) = estado_de(app, host.id);
    match sondeo.resultado {
        ResultadoSondeo::Error => {
            let ambito = area.width.saturating_sub(2) as usize;
            for trozo in envolver(
                &sondeo
                    .error
                    .clone()
                    .unwrap_or_else(|| "error de sondeo".to_string()),
                ambito,
            ) {
                lineas.push(Line::from(Span::styled(
                    format!("  {trozo}"),
                    Style::default().fg(tema.paleta.critico),
                )));
            }
            lineas.push(Line::from(""));
            lineas.push(Line::from(Span::styled(
                "  conéctate una vez con ↵ para aceptar la huella o la frase",
                Style::default().fg(tema.paleta.inactivo),
            )));
        }
        ResultadoSondeo::SinMetricas => {
            lineas.push(Line::from(Span::styled(
                "  ALCANZABLE · responde pero sin métricas",
                Style::default().fg(tema.paleta.inactivo),
            )));
            lineas.push(Line::from(Span::styled(
                "  (no parece Linux con /proc o systemctl)",
                Style::default().fg(tema.paleta.inactivo),
            )));
        }
        ResultadoSondeo::Ok => {
            let culpable = |nombre: &str| culpables.iter().any(|c| c == nombre);
            let nucleos = sondeo.nucleos.filter(|nucleos| *nucleos > 0).unwrap_or(1) as f64;
            lineas.push(linea_barra(
                "CARGA",
                sondeo.carga_1m.map(|carga| carga / nucleos),
                formato_carga(sondeo),
                culpable("carga"),
                app,
            ));
            lineas.push(linea_barra(
                "MEM",
                sondeo.memoria_pct().map(|pct| pct / 100.0),
                sondeo
                    .memoria_pct()
                    .map(|pct| format!("{pct:.0} %"))
                    .unwrap_or_else(|| "—".to_string()),
                culpable("memoria"),
                app,
            ));
            lineas.push(linea_barra(
                "DSK",
                sondeo.disco_pct().map(|pct| pct / 100.0),
                sondeo
                    .disco_pct()
                    .map(|pct| format!("{pct:.0} %"))
                    .unwrap_or_else(|| "—".to_string()),
                culpable("disco"),
                app,
            ));
            let red = match app.tasas_red.get(&host.id) {
                Some((rx, tx)) => format!(
                    "↓ {}  ↑ {}",
                    estado::formatear_tasa(*rx),
                    estado::formatear_tasa(*tx)
                ),
                None => "↓ —  ↑ —".to_string(),
            };
            lineas.push(Line::from(vec![
                Span::styled("  NET   ", Style::default().fg(tema.paleta.inactivo)),
                Span::styled(red, Style::default().fg(tema.paleta.texto)),
            ]));
            let uptime = sondeo
                .uptime_seg
                .map(estado::formatear_uptime)
                .unwrap_or_else(|| "—".to_string());
            lineas.push(Line::from(vec![
                Span::styled("  UP    ", Style::default().fg(tema.paleta.inactivo)),
                Span::styled(uptime, Style::default().fg(tema.paleta.texto)),
            ]));
            if !sondeo.servicios.is_empty() {
                lineas.push(Line::from(""));
                lineas.push(Line::from(Span::styled(
                    "  SERVICIOS",
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD),
                )));
                for (unidad, valor) in &sondeo.servicios {
                    let activo = valor == "active";
                    let desconocida = valor == "unknown";
                    let (glifo, color) = if activo {
                        (tema.glifos.conectado, tema.paleta.correcto)
                    } else {
                        (tema.glifos.error, tema.paleta.acento)
                    };
                    let sufijo = if desconocida {
                        " (unidad no encontrada)"
                    } else {
                        ""
                    };
                    lineas.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("{glifo} "),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{unidad}{sufijo}"),
                            Style::default().fg(if activo {
                                tema.paleta.texto
                            } else {
                                tema.paleta.acento
                            }),
                        ),
                    ]));
                }
            }
        }
    }
    lineas.push(Line::from(""));
    let pie = format!(
        "sondeado hace {} en {} ms{}",
        estado::antiguedad_segundos(&sondeo.fecha)
            .map(estado::formatear_antiguedad)
            .unwrap_or_else(|| "—".to_string()),
        sondeo.duracion_ms,
        if sondeo.resultado == ResultadoSondeo::Ok {
            format!(" · {}", estado_actual.palabra())
        } else {
            String::new()
        }
    );
    lineas.push(Line::from(Span::styled(
        format!("  {pie}"),
        Style::default().fg(tema.paleta.inactivo),
    )));
    let parrafo = Paragraph::new(lineas).wrap(ratatui::widgets::Wrap { trim: false });
    marco.render_widget(parrafo, area);
}

fn linea_barra(
    etiqueta: &str,
    fraccion: Option<f64>,
    valor: String,
    culpable: bool,
    app: &App,
) -> Line<'static> {
    let tema = &app.tema;
    let color = if culpable {
        tema.paleta.acento
    } else {
        tema.paleta.correcto
    };
    let barra = barra(fraccion.unwrap_or(0.0), ANCHO_BARRAS, tema.ascii);
    Line::from(vec![
        Span::styled(
            format!("  {etiqueta:<6}"),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled(barra, Style::default().fg(color)),
        Span::styled(
            format!("  {valor}"),
            Style::default().fg(if culpable {
                tema.paleta.acento
            } else {
                tema.paleta.texto
            }),
        ),
    ])
}

fn formato_carga(sondeo: &Sondeo) -> String {
    match (sondeo.carga_1m, sondeo.nucleos) {
        (Some(carga), Some(nucleos)) => format!("{carga:.1}/{nucleos}"),
        (Some(carga), None) => format!("{carga:.1}"),
        _ => "—".to_string(),
    }
}

/// Barra de progreso con `█`/`░` (ASCII: `#`/`.`).
pub fn barra(fraccion: f64, ancho: usize, ascii: bool) -> String {
    let llenos = (fraccion.clamp(0.0, 1.0) * ancho as f64).round() as usize;
    let (lleno, vacio) = if ascii { ('#', '.') } else { ('█', '░') };
    let mut texto = String::new();
    for indice in 0..ancho {
        texto.push(if indice < llenos { lleno } else { vacio });
    }
    texto
}

fn recortar(texto: &str, ancho: usize) -> String {
    if texto.chars().count() > ancho {
        let recorte: String = texto.chars().take(ancho.saturating_sub(1)).collect();
        format!("{recorte}…")
    } else {
        texto.to_string()
    }
}

fn envolver(texto: &str, ancho: usize) -> Vec<String> {
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        if !actual.is_empty() && actual.chars().count() + 1 + palabra.chars().count() > ancho {
            lineas.push(actual.clone());
            actual.clear();
        }
        if !actual.is_empty() {
            actual.push(' ');
        }
        actual.push_str(palabra);
    }
    if !actual.is_empty() {
        lineas.push(actual);
    }
    if lineas.is_empty() {
        lineas.push(String::new());
    }
    lineas
}
