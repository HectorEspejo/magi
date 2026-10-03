//! Diálogos de la edición remota (Fase 8, §6.4): «¿subir cambios?», qué
//! hacer si no se suben, conflicto («el fichero cambió en el host»), fallo de
//! la subida con la ruta del temporal y las preguntas de sí o no (parece
//! binario, propietario, sensibles, descartar). Mínimo y modo estrecho en
//! `disposicion::MINIMO_EDICION` / `ESTRECHO_EDICION`.
//!
//! Todos son `Hoja`s de `dialogos.rs`: encogen con desplazamiento, el pie con
//! las teclas queda fijo y el ASCII lo pone `disposicion::texto_ascii`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::edicion::DialogoEdicion;
use crate::app::App;
use crate::archivos::edicion::{bytes_con_miles, fecha_corta};
use crate::tema::Tema;
use crate::ui::dialogos::{teclas, Hoja};
use crate::ui::disposicion::{Disposicion, Minimo, ESTRECHO_EDICION, MINIMO_EDICION};

/// Anchos deseados de las hojas (encogen al área). Las que enseñan la ruta
/// del temporal son más anchas para que quepa entera.
const ANCHO_CONFLICTO: u16 = 64;
const ANCHO_EDICION: u16 = 62;
const ANCHO_CON_RUTA: u16 = 76;

pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoEdicion,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_EDICION;
    let (ancho, hoja) = match dialogo {
        DialogoEdicion::Subir { destino, .. } => (ANCHO_EDICION, subir(destino, estrecho, tema)),
        DialogoEdicion::NoSubir {
            temporal,
            dir_local,
            ..
        } => (
            ANCHO_CON_RUTA,
            no_subir(temporal, dir_local, estrecho, tema),
        ),
        DialogoEdicion::Conflicto {
            destino,
            antes,
            ahora,
            ..
        } => (
            ANCHO_CONFLICTO,
            conflicto(destino, *antes, *ahora, estrecho, tema),
        ),
        DialogoEdicion::Fallida {
            destino,
            error,
            temporal,
            ..
        } => (
            ANCHO_CON_RUTA,
            fallida(destino, error, temporal, estrecho, tema),
        ),
        DialogoEdicion::Pregunta(pregunta) => {
            let cuerpo = pregunta
                .lineas
                .iter()
                .map(|texto| Line::from(Span::raw(texto.clone())))
                .collect();
            let pie = vec![teclas(&[("s", pregunta.si.0), ("n", pregunta.no.0)], tema)];
            (
                ANCHO_CON_RUTA,
                Hoja::nueva(pregunta.titulo.clone(), cuerpo, pie).peligro(pregunta.peligro),
            )
        }
    };
    hoja.ancla(app.desplazamiento_modal)
        .pintar(marco, area, ancho, tema, disp);
}

/// Mínimo que declara el diálogo (T45).
pub fn minimo(_dialogo: &DialogoEdicion) -> Minimo {
    Minimo {
        tamano: MINIMO_EDICION,
        exige: "diálogo de edición",
    }
}

/// `host:ruta` destacado: es lo primero que hay que leer.
fn destino_destacado(destino: &str, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        destino.to_string(),
        Style::default()
            .fg(tema.paleta.texto)
            .add_modifier(Modifier::BOLD),
    ))
}

fn tenue(texto: impl Into<String>, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        texto.into(),
        Style::default().fg(tema.paleta.inactivo),
    ))
}

fn subir(destino: &str, estrecho: bool, tema: &Tema) -> Hoja {
    let mut cuerpo = vec![
        Line::from("¿Subir cambios a"),
        destino_destacado(&format!("{destino}?"), tema),
    ];
    if !estrecho {
        cuerpo.push(Line::from(""));
        cuerpo.push(tenue(
            "Antes se comprueba que no haya cambiado en el host.",
            tema,
        ));
    }
    let pie = vec![teclas(&[("s", "subir"), ("n", "no subir")], tema)];
    Hoja::nueva("SUBIR CAMBIOS", cuerpo, pie)
}

fn no_subir(temporal: &str, dir_local: &str, estrecho: bool, tema: &Tema) -> Hoja {
    let mut cuerpo = vec![
        Line::from("Tus cambios siguen en el temporal:"),
        tenue(format!("  {temporal}"), tema),
    ];
    if !estrecho {
        cuerpo.push(Line::from(""));
        cuerpo.push(Line::from(format!("La copia local iría a {dir_local}")));
    }
    let pie = if estrecho {
        vec![
            teclas(&[("d", "descartar"), ("c", "copia local")], tema),
            teclas(&[("esc", "volver")], tema),
        ]
    } else {
        vec![teclas(
            &[
                ("d", "descartar"),
                ("c", "guardar copia local"),
                ("esc", "volver"),
            ],
            tema,
        )]
    };
    Hoja::nueva("TUS CAMBIOS SIN SUBIR", cuerpo, pie)
}

/// Tamaño y fecha de una de las dos filas del conflicto.
fn fila_conflicto(
    etiqueta: &str,
    valor: Option<(u64, i64)>,
    estrecho: bool,
    estilo: Style,
) -> Line<'static> {
    let texto = match (valor, estrecho) {
        (Some((tamano, mtime)), true) => format!(
            "{etiqueta:<6}{} B  {}",
            bytes_con_miles(tamano),
            fecha_corta(mtime)
        ),
        (Some((tamano, mtime)), false) => format!(
            "  {etiqueta:<11}  {} bytes   {}",
            bytes_con_miles(tamano),
            crate::archivos::fecha_completa(mtime)
        ),
        (None, true) => format!("{etiqueta:<6}ya no existe"),
        (None, false) => format!("  {etiqueta:<11}  ya no existe en el host"),
    };
    Line::from(Span::styled(texto, estilo))
}

fn conflicto(
    destino: &str,
    antes: (u64, i64),
    ahora: Option<(u64, i64)>,
    estrecho: bool,
    tema: &Tema,
) -> Hoja {
    let (etiqueta_antes, etiqueta_ahora) = match estrecho {
        true => ("antes", "ahora"),
        false => ("al abrirlo", "ahora"),
    };
    let mut cuerpo = vec![
        destino_destacado(destino, tema),
        fila_conflicto(
            etiqueta_antes,
            Some(antes),
            estrecho,
            Style::default().fg(tema.paleta.acento),
        ),
        fila_conflicto(
            etiqueta_ahora,
            ahora,
            estrecho,
            Style::default().fg(tema.paleta.critico),
        ),
    ];
    if !estrecho {
        cuerpo.push(Line::from(""));
        cuerpo.push(tenue(
            "Nada se sobrescribe si no eliges «s».".to_string(),
            tema,
        ));
    }
    let pie = if estrecho {
        vec![
            teclas(&[("s", "sobrescribir"), ("c", "copia local")], tema),
            teclas(&[("d", "descartar"), ("esc", "volver")], tema),
        ]
    } else {
        vec![
            teclas(
                &[
                    ("s", "sobrescribir"),
                    ("c", "guardar mi versión como copia local"),
                ],
                tema,
            ),
            teclas(&[("d", "descartar mis cambios"), ("esc", "volver")], tema),
        ]
    };
    Hoja::nueva("EL FICHERO CAMBIÓ EN EL HOST", cuerpo, pie).peligro(true)
}

fn fallida(destino: &str, error: &str, temporal: &str, estrecho: bool, tema: &Tema) -> Hoja {
    let mut cuerpo = vec![
        destino_destacado(destino, tema),
        Line::from(Span::styled(
            error.to_string(),
            Style::default().fg(tema.paleta.critico),
        )),
    ];
    if !estrecho {
        cuerpo.push(Line::from(""));
    }
    cuerpo.push(Line::from("Tu versión sigue a salvo en el temporal:"));
    cuerpo.push(tenue(format!("  {temporal}"), tema));
    let pie = if estrecho {
        vec![
            teclas(&[("r", "reintentar"), ("c", "copia local")], tema),
            teclas(&[("esc", "cerrar (se queda)")], tema),
        ]
    } else {
        vec![
            teclas(&[("r", "reintentar"), ("c", "guardar copia local")], tema),
            teclas(&[("esc", "cerrar (el temporal se queda)")], tema),
        ]
    };
    Hoja::nueva("LA SUBIDA FALLÓ", cuerpo, pie).peligro(true)
}
