use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::Vista;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let linea = if app.servidor_incompatible {
        Line::from(Span::styled(
            format!(
                " {} el servidor es de otra versión de protocolo: «magi servidor parar» y volver a abrir",
                app.tema.glifos.error
            ),
            Style::default().fg(app.tema.paleta.critico),
        ))
    } else if let Some(mensaje) = &app.mensaje {
        let glifo = if mensaje.error {
            format!("{} ", app.tema.glifos.error)
        } else {
            String::new()
        };
        let color = if mensaje.error {
            app.tema.paleta.critico
        } else {
            app.tema.paleta.correcto
        };
        Line::from(Span::styled(
            format!(" {glifo}{}", mensaje.texto),
            Style::default().fg(color),
        ))
    } else {
        atajos(app)
    };
    marco.render_widget(Paragraph::new(linea), area);
}

fn atajos(app: &App) -> Line<'static> {
    let tema = &app.tema;
    let pares: Vec<(&str, &str)> = match app.vista {
        Vista::Registro => vec![
            (super::tecla(tema, "↓↑", "j/k"), "mover"),
            ("↵", "detalle"),
            ("/", "filtrar"),
            ("t", "tipo"),
            ("p", "purgar >90 d"),
            ("x", "exportar"),
            ("?", "ayuda"),
            ("q", "salir"),
        ],
        Vista::Identidades => vec![
            (super::tecla(tema, "↓↑", "j/k"), "mover"),
            ("n", "generar"),
            ("i", "importar"),
            ("c", "copiar pública"),
            ("e", "alias"),
            ("x", "revocar"),
            ("s", "reescanear"),
            ("v", "revocadas"),
            ("?", "ayuda"),
            ("q", "salir"),
        ],
        Vista::Flota | Vista::Hosts => vec![
            (super::tecla(tema, "↵", "enter"), "conectar"),
            (super::tecla(tema, "↓↑", "j/k"), "mover"),
            (super::tecla(tema, "→←", "l/h"), "plegar"),
            ("e", "editar"),
            ("n", "nuevo"),
            ("g", "grupo"),
            ("x", "borrar"),
            (super::tecla(tema, "⇥", "tab"), "etiquetas"),
            ("/", "filtrar"),
            ("^p", "paleta"),
            ("?", "ayuda"),
            ("q", "salir"),
        ],
        Vista::Ficha => vec![
            (super::tecla(tema, "⇥", "tab"), "campo"),
            ("↵", "desplegable"),
            ("espacio", "casilla"),
            ("^s", "guardar"),
            ("^t", "probar conexión"),
            ("esc", "descartar"),
        ],
        Vista::Sesion => vec![],
        Vista::Sesiones => vec![],
    };
    let mut spans = Vec::new();
    for (indice, (tecla, descripcion)) in pares.iter().enumerate() {
        if indice > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            (*tecla).to_string(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {descripcion}"),
            Style::default().fg(tema.paleta.texto),
        ));
    }
    Line::from(spans)
}
