//! Barra inferior: un aviso del servidor o un mensaje si los hay (tienen
//! prioridad) y, si no, los atajos de la vista.
//!
//! Disposición adaptable (Fase 7): cada atajo lleva prioridad (1 = el más
//! importante; los de `[flota.atajos]` los últimos) y se muestran los que
//! caben (`disposicion::atajos_que_caben`), en su orden. Si no caben todos,
//! la barra termina en `? más` (la ayuda los lista todos); si caben y queda
//! sitio, en `? ayuda`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, PanelFlota};
use crate::archivos::panel::Lado;
use crate::tema::Tema;
use crate::ui::disposicion::{self, Atajo, Disposicion};
use crate::ui::Vista;

/// Prioridad de los atajos de `[flota.atajos]`: los primeros en ocultarse.
pub const PRIORIDAD_ATAJOS_FLOTA: u8 = 5;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ancho = usize::from(area.width);
    let linea = if let Some(version) = app.servidor_incompatible {
        let pid = app
            .pid_incompatible
            .map(|pid| format!(", pid {pid}"))
            .unwrap_or_default();
        let (abre, cierra) = (super::tecla(tema, "«", "\""), super::tecla(tema, "»", "\""));
        let texto = if version < crate::protocolo::VERSION_PROTOCOLO {
            format!(
                " {} servidor de una versión anterior (protocolo {version}{pid}): {abre}magi servidor parar{cierra} y volver a abrir",
                tema.glifos.error
            )
        } else {
            format!(
                " {} el servidor habla el protocolo {version}{pid}, más nuevo que el {} de esta MAGI",
                tema.glifos.error,
                crate::protocolo::VERSION_PROTOCOLO
            )
        };
        Line::from(Span::styled(
            ajustar(&texto, ancho, tema),
            Style::default().fg(tema.paleta.critico),
        ))
    } else if let Some(mensaje) = &app.mensaje {
        let glifo = if mensaje.error {
            format!("{} ", tema.glifos.error)
        } else {
            String::new()
        };
        let color = if mensaje.error {
            tema.paleta.critico
        } else {
            tema.paleta.correcto
        };
        Line::from(Span::styled(
            ajustar(&format!(" {glifo}{}", mensaje.texto), ancho, tema),
            Style::default().fg(color),
        ))
    } else {
        linea_atajos(app, area.width, disp)
    };
    marco.render_widget(Paragraph::new(linea), area);
}

/// Texto de la barra ajustado al ancho, con la marca de recorte. En ASCII los
/// signos tipográficos de los mensajes se degradan.
fn ajustar(texto: &str, ancho: usize, tema: &Tema) -> String {
    let texto = if tema.ascii {
        disposicion::texto_ascii(texto)
    } else {
        texto.to_string()
    };
    disposicion::recortar(&texto, ancho, tema.ascii)
}

/// Línea de atajos de la vista: los que caben por prioridad y `? más` o
/// `? ayuda` al final. El filtro de Túneles que se está escribiendo va
/// siempre, detrás.
fn linea_atajos(app: &App, ancho: u16, disp: &Disposicion) -> Line<'static> {
    let tema = &app.tema;
    let tecla_estilo = Style::default()
        .fg(tema.paleta.acento)
        .add_modifier(Modifier::BOLD);
    let texto_estilo = Style::default().fg(tema.paleta.texto);
    let mut cola: Vec<Span<'static>> = Vec::new();
    if app.vista == Vista::Tuneles && app.filtro_tuneles_activo {
        // Separador, «/ », el texto y el cursor. Si el texto no cabe, se ve
        // su final, que es donde se escribe.
        let cursor = super::tecla(tema, "▏", "|");
        let cabe = usize::from(ancho).saturating_sub(1 + 3 + 2 + 1);
        cola.push(Span::raw("   "));
        cola.push(Span::styled("/ ".to_string(), tecla_estilo));
        cola.push(Span::styled(
            format!(
                "{}{cursor}",
                final_visible(&app.filtro_tuneles, cabe, tema.ascii)
            ),
            texto_estilo,
        ));
    }
    let ancho_cola: u16 = cola.iter().map(|span| span.width() as u16).sum();
    // Un espacio delante de todo y la cola del filtro (con su separador)
    // reservada.
    let disponible = ancho.saturating_sub(1 + ancho_cola);
    let atajos = atajos(app, disp);
    let (visibles, final_ayuda) = if tiene_ayuda(app.vista) {
        let total: u16 = atajos.iter().map(Atajo::ancho).sum();
        if total.saturating_add(Atajo::new("?", "ayuda", 1).ancho()) <= disponible {
            (vec![true; atajos.len()], Some("ayuda"))
        } else if disponible < disposicion::ANCHO_MAS {
            // Ni «? más» cabe (el filtro de Túneles ocupa la barra).
            (vec![false; atajos.len()], None)
        } else {
            // No caben todos con la ayuda: termina en «? más». Si los atajos
            // solos caben, se pide un ancho menor que su total para que
            // `atajos_que_caben` deje sitio a «? más» quitando el menos
            // importante.
            let limite = disponible.min(total.saturating_sub(1));
            let (visibles, _) = disposicion::atajos_que_caben(&atajos, limite);
            (visibles, Some("más"))
        }
    } else {
        // Sin ayuda en la vista, «? más» no tendría adónde llevar.
        (disposicion::atajos_que_caben(&atajos, disponible).0, None)
    };
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    let mut pares: Vec<(String, String)> = atajos
        .into_iter()
        .zip(visibles)
        .filter(|(_, visible)| *visible)
        .map(|(atajo, _)| (atajo.tecla.into_owned(), atajo.texto.into_owned()))
        .collect();
    if let Some(texto) = final_ayuda {
        pares.push(("?".to_string(), texto.to_string()));
    }
    for (indice, (tecla, descripcion)) in pares.into_iter().enumerate() {
        if indice > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(tecla, tecla_estilo));
        spans.push(Span::styled(format!(" {descripcion}"), texto_estilo));
    }
    spans.extend(cola);
    Line::from(spans)
}

/// Los últimos `ancho` caracteres de un texto, con la marca de recorte
/// delante si no cabe entero.
fn final_visible(texto: &str, ancho: usize, ascii: bool) -> String {
    let largo = texto.chars().count();
    if largo <= ancho {
        return texto.to_string();
    }
    if ancho == 0 {
        return String::new();
    }
    let marca = if ascii { "~" } else { "…" };
    let final_: String = texto.chars().skip(largo + 1 - ancho).collect();
    format!("{marca}{final_}")
}

/// ¿La vista abre la ayuda con `?`? En la ficha `?` se escribe y en Sesión va
/// al remoto (allí es `prefijo ?`, anunciado en su barra de estado).
fn tiene_ayuda(vista: Vista) -> bool {
    !matches!(vista, Vista::Ficha | Vista::Sesion)
}

/// Atajos de la vista, en su orden y con su prioridad (sin `?`, que añade la
/// barra al final).
pub fn atajos(app: &App, disp: &Disposicion) -> Vec<Atajo> {
    let tema = &app.tema;
    let intro = super::tecla(tema, "↵", "enter");
    let mover = super::tecla(tema, "↓↑", "j/k");
    let tab = super::tecla(tema, "⇥", "tab");
    let estrecho = disp.estrecho();
    let mut atajos = match app.vista {
        Vista::Archivos => {
            // En estrecho se ve un panel: `Tab` lleva al otro, que se nombra.
            let panel = match app.archivos.as_ref().map(|archivos| archivos.activo) {
                Some(Lado::Remoto) if estrecho => "local",
                Some(Lado::Local) if estrecho => "remoto",
                _ => "panel",
            };
            vec![
                Atajo::new(tab, panel, 1),
                Atajo::new(intro, "abrir", 1),
                Atajo::new("c", "copiar", 1),
                Atajo::new("m", "mover", 2),
                Atajo::new("x", "borrar", 2),
                Atajo::new("E", "editar", 2),
                Atajo::new("S", "sincronizar", 3),
                Atajo::new("p", "permisos", 4),
                Atajo::new("L", "guardadas", 4),
                Atajo::new("r", "renombrar", 3),
                Atajo::new("d", "dir", 3),
                Atajo::new(".", "ocultos", 4),
                Atajo::new("/", "filtrar", 3),
                Atajo::new("g", "ruta", 4),
                Atajo::new("h", "host", 4),
                Atajo::new("t", "cola", 3),
                Atajo::new("q", "volver", 3),
            ]
        }
        Vista::Transferencias => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new("x", "cancelar", 1),
            Atajo::new("C", "limpiar", 2),
            Atajo::new(intro, "detalle", 1),
            Atajo::new("q", "volver", 2),
        ],
        Vista::Registro => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new(intro, "detalle", 1),
            Atajo::new("/", "filtrar", 1),
            Atajo::new("t", "tipo", 2),
            Atajo::new("p", "purgar >90 d", 3),
            Atajo::new("x", "exportar", 3),
            Atajo::new("q", "salir", 3),
        ],
        Vista::Identidades => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new("n", "generar", 1),
            Atajo::new("i", "importar", 2),
            Atajo::new("c", "copiar pública", 1),
            Atajo::new("e", "alias", 3),
            Atajo::new("x", "revocar", 2),
            Atajo::new("s", "reescanear", 3),
            Atajo::new("v", "revocadas", 3),
            Atajo::new("q", "salir", 3),
        ]
        .into_iter()
        // Con la vista baja el detalle está plegado: `↵` lo abre.
        .chain(disp.bajo.then(|| Atajo::new(intro, "detalle", 1)))
        .collect(),
        Vista::Hosts => vec![
            Atajo::new(intro, "conectar", 1),
            Atajo::new(mover, "mover", 4),
            Atajo::new(super::tecla(tema, "→←", "l/h"), "plegar", 3),
            Atajo::new("e", "editar", 2),
            Atajo::new("n", "nuevo", 2),
            Atajo::new("g", "grupo", 3),
            Atajo::new("x", "borrar", 3),
            Atajo::new(tab, "etiquetas", 3),
            Atajo::new("/", "filtrar", 1),
            Atajo::new("!", "snippets", 3),
            Atajo::new("^p", "paleta", 3),
            Atajo::new("q", "salir", 3),
        ],
        Vista::Flota => {
            let mut atajos = vec![
                Atajo::new(intro, "ssh", 1),
                Atajo::new(mover, "mover", 4),
                Atajo::new("r", "sondear", 1),
            ];
            // En estrecho se ve un panel: `Tab` lleva al otro (no mientras
            // se escribe el filtro, que se queda la tecla).
            if estrecho && !app.filtro_activo {
                let destino = match app.panel_flota {
                    PanelFlota::Lista => "detalle",
                    PanelFlota::Detalle => "lista",
                };
                atajos.push(Atajo::new(tab, destino, 1));
            }
            atajos.extend([
                Atajo::new("R", "todos", 3),
                Atajo::new("a", "auto", 3),
                Atajo::new("e", "editar", 2),
                Atajo::new("/", "filtrar", 2),
                Atajo::new("!", "snippets", 3),
                Atajo::new("^p", "paleta", 3),
                Atajo::new("q", "salir", 3),
            ]);
            // Atajos de `[flota.atajos]`: tecla y snippet, los últimos.
            for (tecla, snippet) in app.atajos_flota() {
                atajos.push(Atajo::new(
                    tecla.to_string(),
                    crate::snippets::salida::sanear_linea(snippet, 24),
                    PRIORIDAD_ATAJOS_FLOTA,
                ));
            }
            atajos
        }
        Vista::Ficha => vec![
            Atajo::new(tab, "campo", 2),
            Atajo::new(intro, "desplegable", 3),
            Atajo::new("espacio", "casilla", 3),
            Atajo::new("^s", "guardar", 1),
            Atajo::new("^t", "probar conexión", 3),
            Atajo::new("esc", "descartar", 1),
        ],
        Vista::Tuneles => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new("espacio", "activar/parar", 1),
            Atajo::new("n", "nuevo", 2),
            Atajo::new("e", "editar", 2),
            Atajo::new("x", "borrar", 3),
            Atajo::new("a", "auto", 3),
            Atajo::new("r", "relanzar", 3),
            Atajo::new(intro, "detalle", 1),
            Atajo::new("/", "filtrar", 2),
            Atajo::new("q", "volver", 3),
        ],
        Vista::Snippets => vec![
            Atajo::new(intro, "ejecutar", 1),
            Atajo::new("a", "en todos", 2),
            Atajo::new("p", "en pestaña", 2),
            Atajo::new(
                "n e x",
                format!("nuevo {0} editar {0} borrar", tema.glifos.punto_medio),
                3,
            ),
            Atajo::new("/", "buscar", 1),
            Atajo::new("t", "resultados", 2),
        ]
        .into_iter()
        // Con la vista baja el detalle está plegado: `i` lo abre (`↵` ejecuta).
        .chain(disp.bajo.then(|| Atajo::new("i", "detalle", 2)))
        .collect(),
        Vista::Resultados => vec![
            Atajo::new(tab, "panel", 1),
            Atajo::new(intro, "salida", 1),
            Atajo::new("s", "guardar", 2),
            Atajo::new("r", "repetir", 2),
            Atajo::new("x", "cancelar", 2),
            Atajo::new("C", "limpiar", 3),
            Atajo::new("q", "volver", 2),
        ],
        Vista::Sesiones => vec![
            Atajo::new(intro, "entrar", 1),
            Atajo::new("n", "nueva", 1),
            Atajo::new("r", "reconectar", 2),
            Atajo::new("x", "cerrar", 2),
            Atajo::new("S", "apagar", 3),
            Atajo::new(format!("{} l", app.config.prefijo_escape), "pestañas", 3),
        ],
        // La vista Sesión lleva su propia barra de estado.
        Vista::Sesion => vec![],
    };
    if tema.ascii {
        for atajo in &mut atajos {
            atajo.tecla = disposicion::texto_ascii(&atajo.tecla).into();
            atajo.texto = disposicion::texto_ascii(&atajo.texto).into();
        }
    }
    atajos
}
