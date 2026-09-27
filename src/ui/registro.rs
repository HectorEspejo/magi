//! Vista Registro (F7): la tabla de `REGISTRO` con el detalle de la entrada
//! seleccionada al pie.
//!
//! Disposición adaptable (Fase 7): columnas por prioridad (fecha, tipo y
//! resultado no se ocultan; el host y el detalle caen antes) y detalle
//! inferior plegado con la vista baja (`↵` abre el detalle completo).

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::modelo::{EntradaRegistro, ResultadoRegistro};
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};
use crate::ui::snippets::recortar_en_lineas;
use crate::ui::tuneles::{
    estilo_fila, marco_detalle, prefijo, repartir_vista, texto_plegado, Tabla, ANCHO_PREFIJO,
};

/// Filas del detalle inferior, bordes incluidos: la cabecera de la entrada y
/// hasta dos del detalle.
pub const ALTO_DETALLE: u16 = 5;

/// Ancho de la fecha corta (`15/09 15:41:02`).
const ANCHO_FECHA: u16 = 14;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let punto = tema.glifos.punto_medio;
    let estado = &app.registro;
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            format!(" MAGI {punto} REGISTRO "),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(
            Line::from(Span::styled(
                format!(
                    "{} entradas {punto} {} ",
                    estado.total,
                    estado.filtro.etiqueta_tipo()
                ),
                Style::default().fg(tema.paleta.inactivo),
            ))
            .alignment(Alignment::Right),
        );
    let interior = bloque.inner(area);
    if interior.height == 0 {
        marco.render_widget(bloque, area);
        return;
    }
    let (mut zona, detalle) = repartir_vista(interior, ALTO_DETALLE, disp);
    let bloque = if detalle.is_none() && estado.entrada_seleccionada().is_some() {
        bloque.title_bottom(Span::styled(
            format!(" {} ", texto_plegado(tema, tema.glifos.intro)),
            Style::default().fg(tema.paleta.inactivo),
        ))
    } else {
        bloque
    };
    marco.render_widget(bloque, area);

    if estado.texto_activo && zona.height > 0 {
        let campo = estado
            .campo
            .span(true, Style::default().fg(tema.paleta.texto));
        let linea = Line::from(vec![
            Span::styled("/ ", Style::default().fg(tema.paleta.acento)),
            Span::styled(disposicion::adaptar(&campo.content, ascii), campo.style),
            Span::styled(
                disposicion::adaptar(
                    &format!("  {} aplicar {punto} esc limpiar", tema.glifos.intro),
                    ascii,
                ),
                Style::default().fg(tema.paleta.inactivo),
            ),
        ]);
        marco.render_widget(Paragraph::new(linea), Rect { height: 1, ..zona });
        zona = Rect {
            y: zona.y + 1,
            height: zona.height - 1,
            ..zona
        };
    }

    dibujar_tabla(marco, zona, app, disp);
    if let (Some(detalle), Some(entrada)) = (detalle, estado.entrada_seleccionada()) {
        dibujar_detalle(marco, detalle, app, entrada);
    }
}

/// Columnas: fecha, tipo, host, resultado y detalle. Fecha, tipo y resultado
/// (el estado) no se ocultan; primero cae el detalle, luego el host.
fn columnas(ancho_tipo: u16, ancho_host: u16) -> [Columna; 5] {
    [
        Columna::fija(ANCHO_FECHA, 1),
        Columna::fija(ancho_tipo, 1),
        Columna::fija(ancho_host, 2),
        Columna::fija(5, 1),
        Columna::flexible(12, 3),
    ]
}

fn dibujar_tabla(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let estado = &app.registro;
    let altura = usize::from(area.height);
    let total = estado.entradas.len();
    let inicio = disposicion::ventana(estado.desplazamiento, estado.seleccion, altura, total);
    disp.registrar(
        Lista::Registro,
        VentanaLista {
            inicio,
            filas: altura,
            total,
        },
    );
    if altura == 0 {
        return;
    }
    if total == 0 {
        marco.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  registro vacío",
                Style::default().fg(tema.paleta.inactivo),
            ))),
            area,
        );
        return;
    }

    let largo = |texto: fn(&EntradaRegistro) -> usize| -> u16 {
        estado.entradas.iter().map(texto).max().unwrap_or(0) as u16
    };
    let ancho_tipo = largo(|entrada| entrada.tipo.chars().count()).clamp(8, 22);
    let ancho_host = largo(|entrada| {
        entrada
            .host_nombre
            .as_deref()
            .map(|host| host.chars().count())
            .unwrap_or(1)
    })
    .clamp(4, 18);
    let ancho_detalle = largo(|entrada| entrada.detalle.chars().count()).max(12);
    let columnas = columnas(ancho_tipo, ancho_host);
    let naturales = [ANCHO_FECHA, ancho_tipo, ancho_host, 5, ancho_detalle];
    let tabla = Tabla::nueva(
        area.width.saturating_sub(ANCHO_PREFIJO),
        &columnas,
        &naturales,
    );

    let lineas: Vec<Line> = estado
        .entradas
        .iter()
        .enumerate()
        .skip(inicio)
        .take(altura)
        .map(|(posicion, entrada)| {
            let seleccionada = posicion == estado.seleccion;
            let base = estilo_fila(tema, seleccionada);
            let color = |color| {
                if seleccionada {
                    base
                } else {
                    Style::default().fg(color)
                }
            };
            let color_resultado = match entrada.resultado {
                ResultadoRegistro::Ok => tema.paleta.inactivo,
                ResultadoRegistro::Error => tema.paleta.critico,
            };
            tabla.linea(
                prefijo(seleccionada, tema),
                vec![
                    (formatear_fecha(&entrada.fecha), color(tema.paleta.texto)),
                    (una_linea(&entrada.tipo), color(tema.paleta.acento)),
                    (host_de(entrada), color(tema.paleta.texto)),
                    (
                        entrada.resultado.como_texto().to_string(),
                        color(color_resultado),
                    ),
                    (una_linea(&entrada.detalle), color(tema.paleta.inactivo)),
                ],
                base,
                ascii,
            )
        })
        .collect();
    marco.render_widget(Paragraph::new(lineas), area);
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, entrada: &EntradaRegistro) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let interior = marco_detalle(marco, area, tema);
    if interior.height == 0 {
        return;
    }
    let ancho = usize::from(interior.width.saturating_sub(2));
    let tipo = disposicion::recortar(
        &disposicion::adaptar(&una_linea(&entrada.tipo), ascii),
        ancho,
        ascii,
    );
    let resto = disposicion::adaptar(
        &format!(
            "  {} · {} · {}",
            host_de(entrada),
            formatear_fecha(&entrada.fecha),
            entrada.resultado.como_texto()
        ),
        ascii,
    );
    let resto = disposicion::recortar(&resto, ancho.saturating_sub(tipo.chars().count()), ascii);
    let mut lineas = vec![Line::from(vec![
        Span::raw(" "),
        Span::styled(
            tipo,
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(resto, Style::default().fg(tema.paleta.texto)),
    ])];
    let filas_detalle = usize::from(interior.height).saturating_sub(1).max(1);
    // La marca de corte de `recortar_en_lineas` también pasa a ASCII.
    for trozo in recortar_en_lineas(
        &disposicion::adaptar(&una_linea(&entrada.detalle), ascii),
        ancho,
        filas_detalle,
    ) {
        lineas.push(Line::from(Span::styled(
            format!(" {}", disposicion::adaptar(&trozo, ascii)),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// El detalle en una sola línea, sin secuencias de escape ni caracteres de
/// control (los saltos pasan a espacios).
fn una_linea(texto: &str) -> String {
    crate::ui::snippets::limpio(texto)
}

/// Host de la entrada saneado, o «—» si no tiene.
fn host_de(entrada: &EntradaRegistro) -> String {
    entrada
        .host_nombre
        .as_deref()
        .map(una_linea)
        .unwrap_or_else(|| "—".to_string())
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
