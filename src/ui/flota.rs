//! Vista Flota (F1): lista de hosts con su estado y detalle del seleccionado.
//!
//! Disposición adaptable (Fase 7), derivada del área en cada pintado:
//! - normal (≥ 100 columnas): lista y detalle lado a lado;
//! - estrecho: un solo panel con la cabecera `HOSTS ⇥ detalle` o
//!   `DETALLE ⇥ lista`; `Tab` alterna (`App::panel_flota`), y mientras se
//!   escribe el filtro se ve la lista;
//! - muy grande (≥ 200×60): lista y detalle con ancho máximo, centrados;
//! - bajo (< 20 filas): sin líneas de separación ni huecos en el detalle.
//!
//! Las barras de carga, memoria y disco toman el ancho del detalle.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, PanelFlota};
use crate::flota::estado::{self, EstadoFlota};
use crate::modelo::{ResultadoSondeo, Sondeo};
use crate::tema::Tema;
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};
use crate::ui::hosts::{anchos_tabla, titulo_derecho, titulo_derecho_cabe};

/// Ancho de la lista en ventanas muy grandes; el detalle, como mucho
/// `disposicion::ANCHO_MAX_DETALLE`.
const ANCHO_LISTA_GRANDE: u16 = 48;
/// Límites del ancho de las barras de carga, memoria y disco.
const BARRA_MINIMA: usize = 4;
const BARRA_MAXIMA: usize = 40;
/// Lo que ocupa una línea de barra además de la barra: etiqueta (6), dos
/// espacios, el valor (hasta 9) y un margen.
const FUERA_DE_LA_BARRA: usize = 19;
/// Sangría del texto del detalle, también en las líneas partidas.
const SANGRIA_DETALLE: u16 = 2;
/// Ancho de la palabra de estado (`NOMINAL`, `ALCANZ.`).
const ANCHO_PALABRA: u16 = 7;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let titulo = format!(" MAGI {} FLOTA ", tema.glifos.punto_medio);
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            titulo.clone(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    if let Some(derecha) = cabecera_derecha(app, area.width, &titulo) {
        bloque = bloque.title_top(titulo_derecho(derecha, tema));
    }
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 || interior.width < 4 {
        return;
    }
    if disposicion::es_estrecho(area.width) {
        dibujar_estrecho(marco, interior, app, disp);
    } else {
        dibujar_normal(marco, interior, app, disp);
    }
}

/// Lista y detalle lado a lado; en ventanas muy grandes, con ancho máximo y
/// centrados.
fn dibujar_normal(marco: &mut Frame, interior: Rect, app: &App, disp: &mut Disposicion) {
    let grande = disposicion::es_grande(disp.area);
    let zona = if grande {
        disposicion::limitar_ancho(
            interior,
            ANCHO_LISTA_GRANDE + 1 + disposicion::ANCHO_MAX_DETALLE,
        )
    } else {
        interior
    };
    let ancho_lista = if grande {
        ANCHO_LISTA_GRANDE.min(zona.width / 2)
    } else {
        (u32::from(zona.width) * 42 / 100) as u16
    };
    let lista = Rect {
        width: ancho_lista,
        ..zona
    };
    let separador = Rect {
        x: zona.x + ancho_lista,
        width: 1,
        ..zona
    };
    // En ventanas muy grandes la zona ya limita el detalle a su máximo.
    let detalle = Rect {
        x: separador.x + 1,
        width: zona.width.saturating_sub(ancho_lista + 1),
        ..zona
    };
    dibujar_lista(marco, lista, app, disp);
    let tema = &app.tema;
    let linea: Vec<Line> = (0..separador.height)
        .map(|_| {
            Line::from(Span::styled(
                tema.glifos.separador,
                Style::default().fg(tema.paleta.inactivo),
            ))
        })
        .collect();
    marco.render_widget(Paragraph::new(linea), separador);
    // Un espacio entre el separador y el texto del detalle.
    let detalle = Rect {
        x: detalle.x + 1,
        width: detalle.width.saturating_sub(1),
        ..detalle
    };
    dibujar_detalle(marco, detalle, app, disp.bajo);
}

/// Un solo panel con su cabecera: la lista o el detalle (`Tab` alterna).
fn dibujar_estrecho(marco: &mut Frame, interior: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    // Mientras se escribe el filtro se ve la lista, que es donde se escribe.
    let panel = if app.filtro_activo {
        PanelFlota::Lista
    } else {
        app.panel_flota
    };
    let (etiqueta, destino) = match panel {
        PanelFlota::Lista => ("HOSTS", "detalle"),
        PanelFlota::Detalle => ("DETALLE", "lista"),
    };
    let ancho = usize::from(interior.width);
    let izquierda = format!("  {etiqueta}");
    // Mientras se escribe el filtro, `Tab` no alterna: no se anuncia.
    let derecha = if app.filtro_activo {
        String::new()
    } else {
        format!("{} {destino}  ", tema.glifos.tab)
    };
    let hueco = ancho.saturating_sub(izquierda.chars().count() + derecha.chars().count());
    let mut cabecera = vec![Line::from(vec![
        Span::styled(
            izquierda,
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(hueco)),
        Span::styled(derecha, Style::default().fg(tema.paleta.inactivo)),
    ])];
    if !disp.bajo {
        cabecera.push(Line::from(Span::styled(
            format!("  {}", tema.glifos.linea.repeat(ancho.saturating_sub(4))),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let alto_cabecera = (cabecera.len() as u16).min(interior.height);
    marco.render_widget(
        Paragraph::new(cabecera),
        Rect {
            height: alto_cabecera,
            ..interior
        },
    );
    let resto = Rect {
        y: interior.y + alto_cabecera,
        height: interior.height - alto_cabecera,
        ..interior
    };
    if resto.height == 0 {
        return;
    }
    match panel {
        PanelFlota::Lista => dibujar_lista(marco, resto, app, disp),
        PanelFlota::Detalle => dibujar_detalle(marco, resto, app, disp.bajo),
    }
}

/// Resumen del borde superior por prioridad: número de hosts, nominales y
/// antigüedad del último sondeo (o el progreso del sondeo en curso), lo que
/// quepa junto al título.
fn cabecera_derecha(app: &App, ancho: u16, titulo: &str) -> Option<String> {
    let punto = app.tema.glifos.punto_medio;
    let partes: Vec<String> = if !app.sondeando.is_empty() {
        vec![format!(
            "sondeando {}/{}",
            app.sondeo_hechos, app.sondeo_total
        )]
    } else {
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
        let mut partes = vec![format!("{total} hosts"), format!("{nominales} nominal")];
        if antiguedad.is_finite() {
            partes.push(format!(
                "sondeo hace {}",
                estado::formatear_antiguedad(antiguedad)
            ));
        }
        partes
    };
    let mut texto: Option<String> = None;
    for parte in partes {
        let candidato = match &texto {
            Some(previo) => format!("{previo} {punto} {parte}"),
            None => parte,
        };
        if !titulo_derecho_cabe(ancho, titulo, &format!(" {candidato} ")) {
            break;
        }
        texto = Some(candidato);
    }
    texto.map(|texto| format!(" {texto} "))
}

fn estado_de(app: &App, host_id: i64) -> (EstadoFlota, Vec<String>) {
    estado::evaluar(app.sondeos.get(&host_id), &app.config.flota.umbrales)
}

/// Glifo, color y palabra de estado de un host en Flota.
fn estado_visible(app: &App, host_id: i64) -> (String, ratatui::style::Color, &'static str) {
    let tema = &app.tema;
    if app.sondeando.contains(&host_id) {
        return (
            tema.glifos.conectando.to_string(),
            tema.paleta.acento,
            tema.glifos.puntos,
        );
    }
    let (estado, _) = estado_de(app, host_id);
    let (glifo, color, palabra) = glifo_estado(estado, tema);
    // ●N cuando hay más de una sesión viva al host.
    let sesiones = super::hosts::sesiones_del_host(app, host_id);
    if sesiones >= 2 {
        (format!("{glifo}{sesiones}"), color, palabra)
    } else {
        (glifo.to_string(), color, palabra)
    }
}

/// Lista de hosts: marca de selección, glifo, nombre (con `⇅` si tiene
/// túneles) y palabra de estado. Glifo y nombre nunca se ocultan; la palabra
/// se oculta si no cabe.
fn dibujar_lista(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    if app.hosts.is_empty() {
        let aviso = Paragraph::new(Line::from(Span::styled(
            "Sin hosts: pulsa n en Hosts o I para importar",
            Style::default().fg(tema.paleta.inactivo),
        )))
        .wrap(Wrap { trim: true });
        let zona = Rect {
            x: area.x + 2,
            width: area.width.saturating_sub(2),
            ..area
        };
        marco.render_widget(aviso, zona);
        return;
    }
    let mut lineas: Vec<Line> = Vec::new();
    if app.filtro_activo {
        lineas.push(super::hosts::linea_filtro(app));
    }
    let hosts = app.hosts_flota();
    let filas = usize::from(area.height).saturating_sub(lineas.len());
    let inicio = disposicion::ventana(
        app.desplazamiento_flota,
        app.seleccion_flota,
        filas,
        hosts.len(),
    );
    disp.registrar(
        Lista::Flota,
        VentanaLista {
            inicio,
            filas,
            total: hosts.len(),
        },
    );
    // Columnas: glifo y nombre imprescindibles, palabra de estado después.
    let estados: Vec<_> = hosts
        .iter()
        .map(|indice| estado_visible(app, app.hosts[*indice].id))
        .collect();
    let ancho_glifo = estados
        .iter()
        .map(|(glifo, _, _)| glifo.chars().count())
        .max()
        .unwrap_or(1)
        .max(1) as u16;
    let ancho_nombre = hosts
        .iter()
        .map(|indice| {
            let host = &app.hosts[*indice];
            let tuneles = super::hosts::tuneles_activos_de_host(app, host.id) > 0;
            host.nombre.chars().count() + if tuneles { 2 } else { 0 }
        })
        .max()
        .unwrap_or(4)
        .max(4) as u16;
    let columnas = [
        Columna::fija(ancho_glifo, 1),
        Columna::fija(ancho_nombre, 1),
        Columna::fija(ANCHO_PALABRA, 2),
    ];
    // La marca de selección y un espacio van delante de la tabla.
    let anchos = anchos_tabla(area.width.saturating_sub(2), &columnas, 1);
    for (posicion, indice) in hosts.iter().enumerate().skip(inicio).take(filas) {
        let host = &app.hosts[*indice];
        let seleccionado = posicion == app.seleccion_flota;
        let (glifo, color, palabra) = &estados[posicion];
        let marca = if seleccionado {
            tema.glifos.seleccion
        } else {
            " "
        };
        let mut spans = vec![
            Span::raw(format!("{marca} ")),
            Span::styled(
                disposicion::columna(glifo, usize::from(anchos[0]), tema.ascii),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];
        // Con túneles activos, `⇅` va pegado al nombre dentro de su columna.
        let ancho_nombre = usize::from(anchos[1]);
        let tuneles = super::hosts::tuneles_activos_de_host(app, host.id) > 0 && ancho_nombre >= 4;
        let reserva = if tuneles { 2 } else { 0 };
        let nombre = disposicion::recortar(&host.nombre, ancho_nombre - reserva, tema.ascii);
        let largo = nombre.chars().count() + reserva;
        spans.push(Span::styled(nombre, Style::default().fg(tema.paleta.texto)));
        if tuneles {
            spans.push(Span::styled(
                format!(" {}", tema.glifos.tuneles),
                Style::default().fg(tema.paleta.acento),
            ));
        }
        spans.push(Span::raw(" ".repeat(ancho_nombre.saturating_sub(largo))));
        if anchos[2] > 0 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                disposicion::columna(palabra, usize::from(anchos[2]), tema.ascii),
                Style::default().fg(*color),
            ));
        }
        let mut linea = Line::from(spans);
        let largo = linea.width();
        if largo < usize::from(area.width) {
            linea
                .spans
                .push(Span::raw(" ".repeat(usize::from(area.width) - largo)));
        }
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
    tema: &Tema,
) -> (&'static str, ratatui::style::Color, &'static str) {
    match estado {
        EstadoFlota::Fria => (tema.glifos.desconectado, tema.paleta.inactivo, "FRÍA"),
        EstadoFlota::Caida => (tema.glifos.error, tema.paleta.critico, "CAÍDA"),
        EstadoFlota::Alcanzable => (tema.glifos.conectado, tema.paleta.inactivo, "ALCANZ."),
        EstadoFlota::Carga => (tema.glifos.conectando, tema.paleta.acento, "CARGA"),
        EstadoFlota::Nominal => (tema.glifos.conectado, tema.paleta.correcto, "NOMINAL"),
    }
}

/// Raya de «sin dato».
fn raya(tema: &Tema) -> &'static str {
    super::tecla(tema, "—", "-")
}

/// Detalle del host seleccionado. Con la vista baja no hay líneas en blanco.
/// El texto va sangrado en su zona, de modo que las líneas que no caben se
/// parten sin perder la sangría.
fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, bajo: bool) {
    let tema = &app.tema;
    let Some(host) = app.host_flota_seleccionado() else {
        return;
    };
    let area = Rect {
        x: area.x + SANGRIA_DETALLE.min(area.width),
        width: area.width.saturating_sub(SANGRIA_DETALLE),
        ..area
    };
    let punto = tema.glifos.punto_medio;
    let hueco = |lineas: &mut Vec<Line>| {
        if !bajo {
            lineas.push(Line::from(""));
        }
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
    hueco(&mut lineas);
    let Some(sondeo) = app.sondeos.get(&host.id) else {
        lineas.push(Line::from(Span::styled(
            "sin sondeo todavía",
            Style::default().fg(tema.paleta.inactivo),
        )));
        hueco(&mut lineas);
        lineas.push(Line::from(Span::styled(
            "pulsa r para sondear este host",
            Style::default().fg(tema.paleta.inactivo),
        )));
        marco.render_widget(Paragraph::new(lineas).wrap(Wrap { trim: false }), area);
        return;
    };
    let (estado_actual, culpables) = estado_de(app, host.id);
    let ancho_barra = usize::from(area.width)
        .saturating_sub(FUERA_DE_LA_BARRA)
        .clamp(BARRA_MINIMA, BARRA_MAXIMA);
    match sondeo.resultado {
        ResultadoSondeo::Error => {
            lineas.push(Line::from(Span::styled(
                sondeo
                    .error
                    .clone()
                    .unwrap_or_else(|| "error de sondeo".to_string()),
                Style::default().fg(tema.paleta.critico),
            )));
            hueco(&mut lineas);
            lineas.push(Line::from(Span::styled(
                format!(
                    "conéctate una vez con {} para aceptar la huella o la frase",
                    tema.glifos.intro
                ),
                Style::default().fg(tema.paleta.inactivo),
            )));
        }
        ResultadoSondeo::SinMetricas => {
            lineas.push(Line::from(Span::styled(
                format!("ALCANZABLE {punto} responde pero sin métricas"),
                Style::default().fg(tema.paleta.inactivo),
            )));
            lineas.push(Line::from(Span::styled(
                "(no parece Linux con /proc o systemctl)",
                Style::default().fg(tema.paleta.inactivo),
            )));
        }
        ResultadoSondeo::Ok => {
            let culpable = |nombre: &str| culpables.iter().any(|c| c == nombre);
            let nucleos = sondeo.nucleos.filter(|nucleos| *nucleos > 0).unwrap_or(1) as f64;
            let porcentaje = |valor: Option<f64>| {
                valor
                    .map(|pct| format!("{pct:.0} %"))
                    .unwrap_or_else(|| raya(tema).to_string())
            };
            lineas.push(linea_barra(
                "CARGA",
                sondeo.carga_1m.map(|carga| carga / nucleos),
                formato_carga(sondeo, tema),
                culpable("carga"),
                ancho_barra,
                tema,
            ));
            lineas.push(linea_barra(
                "MEM",
                sondeo.memoria_pct().map(|pct| pct / 100.0),
                porcentaje(sondeo.memoria_pct()),
                culpable("memoria"),
                ancho_barra,
                tema,
            ));
            lineas.push(linea_barra(
                "DSK",
                sondeo.disco_pct().map(|pct| pct / 100.0),
                porcentaje(sondeo.disco_pct()),
                culpable("disco"),
                ancho_barra,
                tema,
            ));
            let (abajo, arriba) = (tema.glifos.abajo, tema.glifos.arriba);
            let red = match app.tasas_red.get(&host.id) {
                Some((rx, tx)) => format!(
                    "{abajo} {}  {arriba} {}",
                    estado::formatear_tasa(*rx),
                    estado::formatear_tasa(*tx)
                ),
                None => format!("{abajo} {0}  {arriba} {0}", raya(tema)),
            };
            lineas.push(Line::from(vec![
                Span::styled("NET   ", Style::default().fg(tema.paleta.inactivo)),
                Span::styled(red, Style::default().fg(tema.paleta.texto)),
            ]));
            let uptime = sondeo
                .uptime_seg
                .map(estado::formatear_uptime)
                .unwrap_or_else(|| raya(tema).to_string());
            lineas.push(Line::from(vec![
                Span::styled("UP    ", Style::default().fg(tema.paleta.inactivo)),
                Span::styled(uptime, Style::default().fg(tema.paleta.texto)),
            ]));
            if !sondeo.servicios.is_empty() {
                hueco(&mut lineas);
                lineas.push(Line::from(Span::styled(
                    "SERVICIOS",
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
    hueco(&mut lineas);
    // Pie por prioridad: antigüedad, estado y duración, lo que quepa.
    let hace = format!(
        "sondeado hace {}",
        estado::antiguedad_segundos(&sondeo.fecha)
            .map(estado::formatear_antiguedad)
            .unwrap_or_else(|| raya(tema).to_string())
    );
    let duracion = format!(" en {} ms", sondeo.duracion_ms);
    let palabra = if sondeo.resultado == ResultadoSondeo::Ok {
        format!(" {punto} {}", estado_actual.palabra())
    } else {
        String::new()
    };
    let cabe = |texto: &str| texto.chars().count() <= usize::from(area.width);
    let pie = [
        format!("{hace}{duracion}{palabra}"),
        format!("{hace}{palabra}"),
    ]
    .into_iter()
    .find(|pie| cabe(pie))
    .unwrap_or(hace);
    lineas.push(Line::from(Span::styled(
        pie,
        Style::default().fg(tema.paleta.inactivo),
    )));
    let parrafo = Paragraph::new(lineas).wrap(Wrap { trim: false });
    marco.render_widget(parrafo, area);
}

fn linea_barra(
    etiqueta: &str,
    fraccion: Option<f64>,
    valor: String,
    culpable: bool,
    ancho: usize,
    tema: &Tema,
) -> Line<'static> {
    let color = if culpable {
        tema.paleta.acento
    } else {
        tema.paleta.correcto
    };
    let barra = barra(fraccion.unwrap_or(0.0), ancho, tema.ascii);
    Line::from(vec![
        Span::styled(
            format!("{etiqueta:<6}"),
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

fn formato_carga(sondeo: &Sondeo, tema: &Tema) -> String {
    match (sondeo.carga_1m, sondeo.nucleos) {
        (Some(carga), Some(nucleos)) => format!("{carga:.1}/{nucleos}"),
        (Some(carga), None) => format!("{carga:.1}"),
        _ => raya(tema).to_string(),
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
