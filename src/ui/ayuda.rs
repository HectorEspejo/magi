use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::{centrar, Vista};

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let (titulo, atajos): (&str, Vec<(&str, &str)>) = match app.vista {
        Vista::Hosts => (
            "AYUDA · HOSTS",
            vec![
                ("↑ ↓ / j k", "mover la selección"),
                ("← → / h l", "plegar o desplegar el grupo"),
                ("↵", "conectar con el host"),
                ("e", "editar la ficha"),
                ("n", "nuevo host"),
                ("g", "menú de grupos"),
                ("x", "borrar host"),
                ("⇥", "alternar usuario·puerto / etiquetas"),
                ("/", "filtro incremental"),
                ("I", "importar ~/.ssh/config"),
                ("E", "exportar magi_config"),
                ("F2 / F3", "ir a Hosts / a la sesión"),
                ("^p", "paleta de comandos"),
                ("q", "volver o salir"),
            ],
        ),
        Vista::Ficha => (
            "AYUDA · FICHA",
            vec![
                ("⇥ / ⇧⇥", "campo siguiente / anterior"),
                ("↵", "abrir desplegable"),
                ("espacio", "marcar o desmarcar casilla"),
                ("^s", "guardar"),
                ("^t", "probar la conexión"),
                ("esc", "descartar (confirma si hay cambios)"),
            ],
        ),
        Vista::Sesion => (
            "AYUDA · SESIÓN",
            vec![
                ("cualquier tecla", "se envía al host remoto"),
                ("^] q / esc", "volver a Hosts (fondo)"),
                ("^] x", "cerrar la sesión"),
                ("^] ^]", "enviar el prefijo literal"),
            ],
        ),
    };
    let alto = atajos.len() as u16 + 4;
    let recta = centrar(area, 62, alto);
    let mut lineas = Vec::new();
    for (tecla, descripcion) in atajos {
        lineas.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{tecla:<16}"),
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                descripcion.to_string(),
                Style::default().fg(tema.paleta.texto),
            ),
        ]));
    }
    lineas.push(Line::from(""));
    lineas.push(Line::from(Span::styled(
        "  esc o ? para cerrar",
        Style::default().fg(tema.paleta.inactivo),
    )));
    marco.render_widget(Clear, recta);
    marco.render_widget(
        Paragraph::new(lineas).block(super::bloque(titulo, tema)),
        recta,
    );
}
