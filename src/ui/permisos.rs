//! Diálogo PERMISOS (Fase 8, §6.1): rejilla 3×3 de casillas rwx, campo octal,
//! recursivo con alcance y aviso ámbar. Mínimo y modo estrecho en
//! `disposicion::MINIMO_PERMISOS` / `ESTRECHO_PERMISOS`: por debajo de ese
//! ancho los rótulos se acortan («u g o», «r w x») y el alcance baja a su
//! propia línea. Lo demás (partir, desplazar, ASCII) lo hace la `Hoja`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::permisos::{DialogoPermisos, FocoPermisos, FormularioPermisos, RecuentoEnVuelo};
use crate::app::App;
use crate::archivos::permisos::{cantidad, Casilla};
use crate::protocolo::AlcancePermisos;
use crate::tema::Tema;
use crate::ui::dialogos::{atajo, Hoja};
use crate::ui::disposicion::{Disposicion, Minimo, ESTRECHO_PERMISOS, MINIMO_PERMISOS};

/// Ancho que pide el diálogo con sitio: la línea del recursivo con sus tres
/// alcances.
const ANCHO: u16 = 66;
/// Marca del cursor en el campo octal (la de los campos de texto).
const CURSOR: char = '\u{2503}';

pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoPermisos,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_PERMISOS;
    let hoja = match dialogo {
        DialogoPermisos::Formulario(formulario) => {
            let (cuerpo, foco) = cuerpo(formulario, estrecho, tema);
            let pares: &[(&str, &str)] = if estrecho {
                &[("^s", "aplicar"), ("esc", "cancelar")]
            } else {
                &[
                    ("^s", "aplicar"),
                    ("⇥", "campo"),
                    ("espacio", "marcar"),
                    ("esc", "cancelar"),
                ]
            };
            Hoja::nueva(
                titulo(formulario, estrecho),
                cuerpo,
                vec![teclas(pares, tema)],
            )
            .foco(foco)
        }
        DialogoPermisos::Contando(en_vuelo) => Hoja::nueva(
            "PERMISOS · CONTANDO",
            cuerpo_contando(en_vuelo, tema),
            vec![teclas(&[("esc", "cancelar")], tema)],
        ),
    };
    hoja.ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO, tema, disp);
}

/// Mínimo que declara el diálogo (T45).
pub fn minimo(_dialogo: &DialogoPermisos) -> Minimo {
    Minimo {
        tamano: MINIMO_PERMISOS,
        exige: "diálogo PERMISOS",
    }
}

/// `PERMISOS · 3 elementos (2 ficheros, 1 directorio)`; en estrecho, sin el
/// desglose.
fn titulo(formulario: &FormularioPermisos, estrecho: bool) -> String {
    let elementos = cantidad(formulario.objetivos.len() as u64, "elemento", "elementos");
    if estrecho {
        return format!("PERMISOS · {elementos}");
    }
    let mut desglose = Vec::new();
    if formulario.ficheros() > 0 {
        desglose.push(cantidad(
            formulario.ficheros() as u64,
            "fichero",
            "ficheros",
        ));
    }
    if formulario.directorios() > 0 {
        desglose.push(cantidad(
            formulario.directorios() as u64,
            "directorio",
            "directorios",
        ));
    }
    format!("PERMISOS · {elementos} ({})", desglose.join(", "))
}

/// Línea de teclas del pie, como la de los demás diálogos.
fn teclas(pares: &[(&str, &str)], tema: &Tema) -> Line<'static> {
    let mut spans = Vec::new();
    for (indice, (tecla, texto)) in pares.iter().enumerate() {
        if indice > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(atajo(tecla, tema));
        spans.push(Span::raw(format!(" {texto}")));
    }
    Line::from(spans)
}

fn estilo_foco(foco: bool, tema: &Tema) -> Style {
    if foco {
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.texto)
    }
}

/// Cuerpo del formulario y la línea con el foco (para que la `Hoja` la deje
/// a la vista al desplazar).
fn cuerpo(
    formulario: &FormularioPermisos,
    estrecho: bool,
    tema: &Tema,
) -> (Vec<Line<'static>>, usize) {
    let tenue = Style::default().fg(tema.paleta.inactivo);
    // Rótulos y separación de columnas: «usuario» y «leer   escribir…» con
    // sitio; «u» y «r w x» en estrecho.
    let (filas, columnas, rotulo, hueco): ([&str; 3], [&str; 3], usize, [usize; 3]) = if estrecho {
        (["u", "g", "o"], ["r", "w", "x"], 6, [1, 1, 0])
    } else {
        (
            ["usuario", "grupo", "otros"],
            ["leer", "escribir", "ejecutar"],
            12,
            [4, 8, 0],
        )
    };
    let mut cuerpo = Vec::new();
    // Cabecera: en estrecho cada letra va sobre el centro de su casilla.
    let cabecera = if estrecho {
        format!(
            "{} {}   {}   {}",
            " ".repeat(rotulo),
            columnas[0],
            columnas[1],
            columnas[2]
        )
    } else {
        format!(
            "{}{}   {}   {}",
            " ".repeat(rotulo),
            columnas[0],
            columnas[1],
            columnas[2]
        )
    };
    cuerpo.push(Line::from(Span::styled(cabecera, tenue)));
    let en_rejilla = formulario.foco == FocoPermisos::Rejilla;
    for (quien, nombre) in filas.iter().enumerate() {
        let mut spans = vec![Span::styled(
            format!("{nombre:<rotulo$}"),
            Style::default().fg(tema.paleta.texto),
        )];
        for (que, hueco) in hueco.iter().enumerate() {
            let casilla = formulario.modo.casillas.casilla(quien, que);
            let mut estilo = Style::default().fg(match casilla {
                Casilla::Si => tema.paleta.correcto,
                Casilla::No => tema.paleta.texto,
                Casilla::Mixto => tema.paleta.acento,
            });
            if en_rejilla && formulario.celda == (quien, que) {
                estilo = estilo
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED);
            }
            spans.push(Span::styled(casilla.texto(), estilo));
            spans.push(Span::raw(" ".repeat(*hueco)));
        }
        cuerpo.push(Line::from(spans));
    }

    let en_octal = formulario.foco == FocoPermisos::Octal;
    let mut octal = formulario.modo.octal.clone();
    if en_octal {
        octal.push(CURSOR);
    }
    let ancho_octal = if en_octal { 5 } else { 4 };
    cuerpo.push(Line::from(vec![
        Span::styled(format!("{:<rotulo$}", "octal"), estilo_foco(en_octal, tema)),
        Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
        Span::styled(
            format!("{octal:<ancho_octal$}"),
            estilo_foco(en_octal, tema),
        ),
        Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
    ]));
    let mut foco = match formulario.foco {
        FocoPermisos::Rejilla => 1 + formulario.celda.0,
        _ => 4,
    };

    if formulario.hay_directorios() {
        let casilla = Span::styled(
            format!(
                "{} recursivo",
                crate::ui::componentes::casilla(formulario.recursivo)
            ),
            if formulario.foco == FocoPermisos::Recursivo {
                estilo_foco(true, tema)
            } else if formulario.recursivo {
                Style::default().fg(tema.paleta.correcto)
            } else {
                Style::default().fg(tema.paleta.texto)
            },
        );
        let marca = |alcance: AlcancePermisos| {
            if formulario.alcance == alcance {
                "(•)"
            } else {
                "( )"
            }
        };
        let (dirs, ficheros) = if estrecho {
            ("dirs", "ficheros")
        } else {
            ("solo dirs", "solo ficheros")
        };
        let alcances = Span::styled(
            format!(
                "{} todo   {} {dirs}   {} {ficheros}",
                marca(AlcancePermisos::Todo),
                marca(AlcancePermisos::Directorios),
                marca(AlcancePermisos::Ficheros),
            ),
            if formulario.foco == FocoPermisos::Alcance {
                estilo_foco(true, tema)
            } else if formulario.recursivo {
                Style::default().fg(tema.paleta.texto)
            } else {
                tenue
            },
        );
        if formulario.foco == FocoPermisos::Recursivo {
            foco = cuerpo.len();
        }
        if estrecho {
            cuerpo.push(Line::from(casilla));
            if formulario.foco == FocoPermisos::Alcance {
                foco = cuerpo.len();
            }
            cuerpo.push(Line::from(alcances));
        } else {
            if formulario.foco == FocoPermisos::Alcance {
                foco = cuerpo.len();
            }
            cuerpo.push(Line::from(vec![casilla, Span::raw("   "), alcances]));
        }
    }

    if formulario.aviso() {
        cuerpo.push(Line::from(Span::styled(
            format!(
                "⚠ {} en directorios impide entrar en ellos",
                formulario.modo.texto()
            ),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        )));
    }
    (cuerpo, foco)
}

/// Cuerpo del recuento en marcha.
fn cuerpo_contando(en_vuelo: &RecuentoEnVuelo, tema: &Tema) -> Vec<Line<'static>> {
    let directorios = en_vuelo
        .orden
        .objetivos
        .iter()
        .filter(|objetivo| objetivo.es_dir)
        .count() as u64;
    vec![
        Line::from(Span::raw(format!(
            "Contando lo que hay dentro de {}…",
            cantidad(directorios, "directorio", "directorios")
        ))),
        Line::from(Span::styled(
            format!("{} por ahora", en_vuelo.recuento.descripcion()),
            Style::default().fg(tema.paleta.inactivo),
        )),
    ]
}
