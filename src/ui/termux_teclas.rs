//! Pintado del diálogo FILA DE TECLAS DE TERMUX (Fase 17, §6.4 del informe).
//!
//! Es una `Hoja` de `dialogos.rs`: encoge al área, desplaza el cuerpo si no cabe
//! y lleva en el pie (fijo) las teclas o lo que está pasando. Enseña la fila
//! propuesta alineada en dos líneas, dónde se escribe y el nombre de la copia
//! con fecha (o que el fichero no existe). Por debajo de
//! `ESTRECHO_TECLAS_TERMUX` los textos se acortan (T45); el respaldo ASCII lo
//! hace la `Hoja` y las flechas de la fila pasan por `disposicion::adaptar`.
//! Nada de lo que contiene `termux.properties` se pinta salvo, en la CLI, la
//! línea `extra-keys` del propio usuario.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::termux_teclas::DialogoTeclasTermux;
use crate::app::App;
use crate::tema::Tema;
use crate::termux::teclas::EstadoFila;
use crate::ui::dialogos::{teclas, Hoja};
use crate::ui::disposicion::{
    adaptar, Disposicion, Minimo, ESTRECHO_TECLAS_TERMUX, MINIMO_TECLAS_TERMUX,
};

/// Ancho que pide el diálogo con sitio.
const ANCHO: u16 = 56;

/// Título del recuadro.
pub const TITULO: &str = "FILA DE TECLAS DE TERMUX";

#[must_use]
pub fn minimo(_dialogo: &DialogoTeclasTermux) -> Minimo {
    Minimo {
        tamano: MINIMO_TECLAS_TERMUX,
        exige: TITULO,
    }
}

/// Pinta el diálogo como una `Hoja` de `dialogos.rs`.
pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoTeclasTermux,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_TECLAS_TERMUX;
    Hoja::nueva(
        TITULO,
        cuerpo(dialogo, estrecho, tema),
        pie(dialogo, estrecho, tema),
    )
    .ancla(app.desplazamiento_modal)
    .pintar(marco, area, ANCHO, tema, disp);
}

fn texto(contenido: &str, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        contenido.to_string(),
        Style::default().fg(tema.paleta.texto),
    ))
}

fn tenue(contenido: &str, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        contenido.to_string(),
        Style::default().fg(tema.paleta.inactivo),
    ))
}

/// Las dos filas de la propuesta, con cada columna tan ancha como la más
/// ancha de sus dos teclas, para que `F1` quede bajo `ESC`, `F5` bajo `^]`…
/// Se miden los textos ya adaptados (en ASCII, `←` ocupa dos columnas).
#[must_use]
pub fn filas_alineadas(arriba: &[String], abajo: &[String], ascii: bool) -> (String, String) {
    let ancho = |tecla: &str| tecla.chars().count();
    let mut primera = String::new();
    let mut segunda = String::new();
    for indice in 0..arriba.len().max(abajo.len()) {
        let de_arriba = arriba
            .get(indice)
            .map_or_else(String::new, |tecla| adaptar(tecla, ascii));
        let de_abajo = abajo
            .get(indice)
            .map_or_else(String::new, |tecla| adaptar(tecla, ascii));
        let columna = ancho(&de_arriba).max(ancho(&de_abajo));
        primera.push_str(&format!("{de_arriba:<columna$} "));
        segunda.push_str(&format!("{de_abajo:<columna$} "));
    }
    (
        primera.trim_end().to_string(),
        segunda.trim_end().to_string(),
    )
}

fn cuerpo(dialogo: &DialogoTeclasTermux, estrecho: bool, tema: &Tema) -> Vec<Line<'static>> {
    let propuesta = &dialogo.propuesta;
    let teclas_de_la_fila = Style::default()
        .fg(tema.paleta.acento)
        .add_modifier(Modifier::BOLD);
    let (primera, segunda) = filas_alineadas(&propuesta.arriba, &propuesta.abajo, tema.ascii);
    let mut lineas = vec![
        texto("MAGI propone esta fila:", tema),
        Line::from(Span::styled(format!(" {primera}"), teclas_de_la_fila)),
        Line::from(Span::styled(format!(" {segunda}"), teclas_de_la_fila)),
    ];
    if let Some(aviso) = &propuesta.aviso_prefijo {
        // El prefijo no cabe como macro de Termux: se dice, sin esconderlo.
        lineas.push(Line::from(Span::styled(
            if estrecho {
                "Sin el prefijo: no es un macro de Termux.".to_string()
            } else {
                format!("Sin el prefijo ({aviso}).")
            },
            Style::default().fg(tema.paleta.critico),
        )));
    }
    lineas.push(Line::from(""));
    let ruta = &propuesta.ruta_visible;
    match &propuesta.copia {
        Some(copia) => {
            lineas.push(texto(
                &if estrecho {
                    format!("En {ruta}: solo extra-keys. Copia previa:")
                } else {
                    format!("Se escribe en {ruta}, solo la línea extra-keys. Antes, una copia:")
                },
                tema,
            ));
            lineas.push(Line::from(Span::styled(
                copia.clone(),
                Style::default()
                    .fg(tema.paleta.texto)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        None => {
            lineas.push(texto(
                &if estrecho {
                    format!("Crea {ruta}: no existe, sin copia.")
                } else {
                    format!("Se crea {ruta} con la línea extra-keys. No existe todavía: no hay nada que copiar.")
                },
                tema,
            ));
        }
    }
    if !estrecho {
        match &propuesta.estado {
            EstadoFila::Otra => {
                lineas.push(Line::from(""));
                lineas.push(tenue(
                    "Ya tienes otra fila: se sustituye y la tuya queda en la copia.",
                    tema,
                ));
            }
            EstadoFila::SinExtraKeys => {
                lineas.push(Line::from(""));
                lineas.push(tenue(
                    "No tienes extra-keys: se añade al final; lo demás no se toca.",
                    tema,
                ));
            }
            _ => {}
        }
    }
    lineas
}

fn pie(dialogo: &DialogoTeclasTermux, estrecho: bool, tema: &Tema) -> Vec<Line<'static>> {
    let mut pie = Vec::new();
    if dialogo.trabajando.is_some() {
        // La tarea no se puede abortar y el fichero se escribe igualmente: no
        // se ofrece Esc.
        pie.push(Line::from(Span::styled(
            "instalando…".to_string(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        )));
        return pie;
    }
    if let Some(error) = &dialogo.error {
        pie.push(Line::from(Span::styled(
            error.clone(),
            Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD),
        )));
    }
    pie.push(if estrecho {
        teclas(&[("↵", "instalar"), ("Esc", "no")], tema)
    } else {
        teclas(&[("↵", "instalar"), ("Esc", "ahora no")], tema)
    });
    pie
}
