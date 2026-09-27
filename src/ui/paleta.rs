//! Paleta de comandos `^p`. Encoge al área (Fase 7): la etiqueta se recorta
//! y la categoría se pega a la derecha o desaparece si no hay sitio; la
//! ventana de entradas sigue a la selección y se registra como
//! `Lista::Paleta`, que usan `PgUp`/`PgDn`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::centrar;
use crate::ui::dialogos::{indicador, texto_de};
use crate::ui::disposicion::{self, Disposicion, Lista, VentanaLista};

/// Tamaño deseado de la paleta.
const ANCHO_PALETA: u16 = 72;
const ALTO_PALETA: u16 = 16;
/// Por debajo de este ancho de etiqueta, la categoría no se pinta: es
/// secundaria (repite el verbo de la etiqueta) y lo que distingue una entrada
/// de otra es el host del final («editar host · vps-openclaw»).
const ETIQUETA_MINIMA: usize = 28;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let Some(paleta) = &app.paleta else {
        return;
    };
    let tema = &app.tema;
    let ascii = tema.ascii;
    let recta = centrar(area, ANCHO_PALETA, ALTO_PALETA);
    // Una fila para la consulta y el resto para las entradas.
    let visibles = usize::from(recta.height.saturating_sub(3));
    let total = paleta.filtradas.len();
    // La ventana sigue a la selección para que nunca quede fuera de la vista.
    let inicio = disposicion::ventana(paleta.desplazamiento, paleta.seleccion, visibles, total);
    disp.registrar(
        Lista::Paleta,
        VentanaLista {
            inicio,
            filas: visibles,
            total,
        },
    );
    let estilo_borde = Style::default().fg(tema.paleta.acento);
    let titulo = disposicion::recortar(
        &format!("PALETA {} ^p", tema.glifos.punto_medio),
        usize::from(recta.width.saturating_sub(4)),
        ascii,
    );
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(estilo_borde)
        .title(Span::styled(
            format!(" {titulo} "),
            estilo_borde.add_modifier(Modifier::BOLD),
        ));
    if total > visibles && visibles > 0 {
        bloque = bloque.title_bottom(
            Line::from(Span::styled(
                indicador(inicio, visibles, total, ascii),
                estilo_borde,
            ))
            .right_aligned(),
        );
    }
    let interior = bloque.inner(recta);
    marco.render_widget(Clear, recta);
    marco.render_widget(bloque, recta);
    if interior.height == 0 || interior.width == 0 {
        return;
    }
    let ancho = usize::from(interior.width);

    // Consulta: si no cabe, se ve el tramo que rodea al cursor.
    let prefijo = format!("  {} ", if ascii { ">" } else { "❯" });
    let prefijo = if ancho >= 12 { prefijo } else { String::new() };
    let hueco = ancho.saturating_sub(prefijo.chars().count()).max(1);
    let consulta: Vec<char> =
        texto_de(&paleta.consulta.span(true, Style::default()).content, ascii)
            .chars()
            .collect();
    let desde = paleta
        .consulta
        .cursor
        .saturating_sub(hueco.saturating_sub(1));
    let consulta: String = consulta.iter().skip(desde).take(hueco).collect();
    marco.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prefijo, Style::default().fg(tema.paleta.acento)),
            Span::styled(consulta, Style::default().fg(tema.paleta.texto)),
        ])),
        Rect {
            height: 1,
            ..interior
        },
    );
    if interior.height < 2 {
        return;
    }

    // La categoría va en una columna a la derecha, del ancho de la más larga
    // de toda la paleta, con dos espacios de separación y uno de margen. Se
    // decide para todas las filas a la vez (no parpadea al desplazarse) y,
    // si no deja sitio a la etiqueta, se quita en todas.
    let cabeza = 3;
    let resto = ancho.saturating_sub(cabeza);
    let ancho_categoria = paleta
        .entradas
        .iter()
        .map(|entrada| texto_de(entrada.categoria, ascii).chars().count())
        .max()
        .unwrap_or(0);
    // Separación, columna de la categoría y margen derecho.
    let columna_categoria = 2 + ancho_categoria + 1;
    let con_categoria = ancho_categoria > 0 && resto >= ETIQUETA_MINIMA + columna_categoria;
    let ancho_etiqueta = if con_categoria {
        resto - columna_categoria
    } else {
        resto
    };
    let mut lineas = Vec::new();
    for (posicion, indice) in paleta
        .filtradas
        .iter()
        .enumerate()
        .skip(inicio)
        .take(visibles)
    {
        let entrada = &paleta.entradas[*indice];
        let seleccionada = posicion == paleta.seleccion;
        let estilo = if seleccionada {
            Style::default()
                .bg(tema.paleta.acento)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        let estilo_categoria = if seleccionada {
            estilo
        } else {
            Style::default().fg(tema.paleta.inactivo)
        };
        let marca = if seleccionada {
            tema.glifos.seleccion
        } else {
            " "
        };
        let etiqueta = texto_de(&entrada.etiqueta, ascii);
        let mut spans = vec![Span::styled(
            format!(
                " {marca} {}",
                disposicion::columna(&etiqueta, ancho_etiqueta, ascii)
            ),
            estilo,
        )];
        if con_categoria {
            let categoria = texto_de(entrada.categoria, ascii);
            spans.push(Span::styled(
                format!("  {categoria:>ancho_categoria$} "),
                estilo_categoria,
            ));
        }
        lineas.push(Line::from(spans));
    }
    if lineas.is_empty() {
        lineas.push(Line::from(Span::styled(
            "  sin coincidencias",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    marco.render_widget(
        Paragraph::new(lineas),
        Rect {
            y: interior.y + 1,
            height: interior.height - 1,
            ..interior
        },
    );
}
