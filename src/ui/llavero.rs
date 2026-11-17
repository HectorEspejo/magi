//! Pintado de los diálogos del llavero con frase maestra (Fase 17, §6.1 y
//! §6.2 del informe): LLAVERO DE MAGI, CREAR LA FRASE MAESTRA, CAMBIAR LA
//! FRASE MAESTRA, BORRAR EL LLAVERO y LLAVERO DAÑADO.
//!
//! Todos son una `Hoja` de `dialogos.rs`: encogen al área, siguen al campo con
//! foco y llevan en el pie (fijo) lo que se está haciendo («trabajando…» y
//! «espera…» mientras scrypt trabaja, sin teclas porque no se atiende ninguna;
//! el error o la espera) y las teclas. Por debajo de `ESTRECHO_LLAVERO` los
//! textos de ayuda se acortan (T45); el respaldo ASCII lo hace la `Hoja`. Los
//! campos de frase van enmascarados y nunca se pintan en claro.

use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::llavero::{DialogoLlavero, ModoLlavero, PALABRA_BORRAR};
use crate::app::App;
use crate::tema::Tema;
use crate::ui::componentes::CampoTexto;
use crate::ui::dialogos::{teclas, Hoja};
use crate::ui::disposicion::{Disposicion, Minimo, ESTRECHO_LLAVERO, MINIMO_LLAVERO};

/// Ancho que pide el diálogo con sitio.
const ANCHO: u16 = 60;
/// Marca del cursor de los campos (la de `CampoTexto::span`).
const CURSOR: char = '\u{2503}';
/// Tope de lo que se enseña del motivo de un llavero dañado.
const TOPE_MOTIVO: usize = 160;

/// Título del recuadro de cada modo.
#[must_use]
pub fn titulo(modo: ModoLlavero) -> &'static str {
    match modo {
        ModoLlavero::Desbloquear => "LLAVERO DE MAGI",
        ModoLlavero::Crear => "CREAR LA FRASE MAESTRA",
        ModoLlavero::Cambiar => "CAMBIAR LA FRASE MAESTRA",
        ModoLlavero::Borrar => "BORRAR EL LLAVERO",
        ModoLlavero::Danado => "LLAVERO DAÑADO",
    }
}

#[must_use]
pub fn minimo(dialogo: &DialogoLlavero) -> Minimo {
    Minimo {
        tamano: MINIMO_LLAVERO,
        exige: titulo(dialogo.modo),
    }
}

/// Pinta el diálogo del llavero como una `Hoja` de `dialogos.rs`.
pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoLlavero,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_LLAVERO;
    let cuerpo = cuerpo(dialogo, estrecho, tema);
    let pie = pie(dialogo, estrecho, tema);
    let peligro = matches!(dialogo.modo, ModoLlavero::Borrar | ModoLlavero::Danado);
    Hoja::nueva(titulo(dialogo.modo), cuerpo.lineas, pie)
        .foco(cuerpo.foco)
        .peligro(peligro)
        .ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO, tema, disp);
}

/// Las líneas del cuerpo y la del campo con el foco.
struct Cuerpo {
    lineas: Vec<Line<'static>>,
    foco: usize,
}

fn estilo_texto(tema: &Tema) -> Style {
    Style::default().fg(tema.paleta.texto)
}

fn estilo_tenue(tema: &Tema) -> Style {
    Style::default().fg(tema.paleta.inactivo)
}

fn estilo_error(tema: &Tema) -> Style {
    Style::default()
        .fg(tema.paleta.critico)
        .add_modifier(Modifier::BOLD)
}

fn texto(contenido: &str, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(contenido.to_string(), estilo_texto(tema)))
}

fn tenue(contenido: &str, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(contenido.to_string(), estilo_tenue(tema)))
}

/// Frase enmascarada con el cursor donde esté (si el campo tiene el foco).
fn enmascarada(campo: &CampoTexto, activo: bool, tema: &Tema) -> String {
    let mascara = if tema.ascii { '*' } else { '•' };
    let mut visible = String::new();
    let total = campo.texto.chars().count();
    for indice in 0..total {
        if activo && indice == campo.cursor {
            visible.push(CURSOR);
        }
        visible.push(mascara);
    }
    if activo && campo.cursor >= total {
        visible.push(CURSOR);
    }
    visible
}

/// `rótulo  [ ••••┃ ]` de un campo de frase. Con `rotulo` vacío, solo el campo.
fn linea_campo(
    rotulo: &str,
    ancho_rotulo: usize,
    campo: &CampoTexto,
    activo: bool,
    secreto: bool,
    tema: &Tema,
) -> Line<'static> {
    let mut spans = Vec::new();
    if !rotulo.is_empty() {
        spans.push(Span::styled(
            format!("{rotulo:<ancho_rotulo$}"),
            Style::default().fg(if activo {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ));
    }
    let corchete = Style::default().fg(tema.paleta.acento);
    spans.push(Span::styled("[ ", corchete));
    if secreto {
        spans.push(Span::styled(
            enmascarada(campo, activo, tema),
            estilo_texto(tema),
        ));
    } else {
        spans.push(campo.span(activo, estilo_texto(tema)));
    }
    spans.push(Span::styled(" ]", corchete));
    Line::from(spans)
}

fn cuerpo(dialogo: &DialogoLlavero, estrecho: bool, tema: &Tema) -> Cuerpo {
    match dialogo.modo {
        ModoLlavero::Desbloquear => cuerpo_desbloquear(dialogo, estrecho, tema),
        ModoLlavero::Crear => cuerpo_crear(dialogo, estrecho, tema),
        ModoLlavero::Cambiar => cuerpo_cambiar(dialogo, estrecho, tema),
        ModoLlavero::Borrar => cuerpo_borrar(dialogo, estrecho, tema),
        ModoLlavero::Danado => cuerpo_danado(dialogo, estrecho, tema),
    }
}

/// §6.1: «Frase maestra», el campo y lo que desbloquea.
fn cuerpo_desbloquear(dialogo: &DialogoLlavero, estrecho: bool, tema: &Tema) -> Cuerpo {
    let mut lineas = vec![
        Line::from(Span::styled(
            "Frase maestra".to_string(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        )),
        linea_campo("", 0, &dialogo.campos[0], true, true, tema),
        Line::from(""),
    ];
    lineas.push(texto(
        if estrecho {
            "Desbloquea tus contraseñas y la clave de sincronización."
        } else {
            "Desbloquea tus contraseñas y la clave de sincronización de este equipo."
        },
        tema,
    ));
    Cuerpo { lineas, foco: 1 }
}

/// §6.2: dos campos y el aviso de que la frase no se recupera.
fn cuerpo_crear(dialogo: &DialogoLlavero, estrecho: bool, tema: &Tema) -> Cuerpo {
    let mut lineas = vec![
        linea_campo(
            "Frase    ",
            9,
            &dialogo.campos[0],
            dialogo.foco == 0,
            true,
            tema,
        ),
        linea_campo(
            "Repetir  ",
            9,
            &dialogo.campos[1],
            dialogo.foco == 1,
            true,
            tema,
        ),
        Line::from(""),
    ];
    if estrecho {
        lineas.push(texto(
            "Al menos 12 caracteres. Si la olvidas no se recupera.",
            tema,
        ));
    } else {
        lineas.push(texto(
            "Guarda tus contraseñas y la clave de sincronización. Al menos 12 caracteres; mejor, varias palabras.",
            tema,
        ));
        lineas.push(texto(
            "Si la olvidas no se recupera: habría que borrar el llavero y volver a unir este equipo.",
            tema,
        ));
    }
    Cuerpo {
        lineas,
        foco: dialogo.foco,
    }
}

/// La actual y la nueva dos veces.
fn cuerpo_cambiar(dialogo: &DialogoLlavero, estrecho: bool, tema: &Tema) -> Cuerpo {
    let mut lineas = Vec::new();
    for (indice, rotulo) in ["Actual   ", "Nueva    ", "Repetir  "].iter().enumerate() {
        lineas.push(linea_campo(
            rotulo,
            9,
            &dialogo.campos[indice],
            dialogo.foco == indice,
            true,
            tema,
        ));
    }
    lineas.push(Line::from(""));
    if estrecho {
        lineas.push(texto(
            "La nueva, de 12 caracteres o más. Si la olvidas no se recupera.",
            tema,
        ));
    } else {
        lineas.push(texto(
            "La frase nueva tiene al menos 12 caracteres; mejor, varias palabras. Se recifra el llavero con ella.",
            tema,
        ));
        lineas.push(texto(
            "Si la olvidas no se recupera: habría que borrar el llavero y volver a unir este equipo.",
            tema,
        ));
    }
    Cuerpo {
        lineas,
        foco: dialogo.foco,
    }
}

/// Qué se pierde y la palabra que hay que escribir.
fn cuerpo_borrar(dialogo: &DialogoLlavero, estrecho: bool, tema: &Tema) -> Cuerpo {
    let mut lineas = Vec::new();
    if estrecho {
        lineas.push(texto(
            "Se pierden las contraseñas guardadas y la clave de sincronización de este equipo.",
            tema,
        ));
    } else {
        lineas.push(texto("Se borrará llavero.age de este equipo.", tema));
        lineas.push(Line::from(""));
        lineas.push(texto(
            "Se pierden las contraseñas guardadas de tus hosts y la clave de sincronización de este equipo: habrá que volver a escribir las contraseñas y a unir el equipo con la clave.",
            tema,
        ));
        lineas.push(tenue("El inventario y los demás equipos no cambian.", tema));
    }
    lineas.push(Line::from(""));
    lineas.push(texto(
        &format!("Escribe {PALABRA_BORRAR} para confirmar:"),
        tema,
    ));
    let foco = lineas.len();
    lineas.push(linea_campo("", 0, &dialogo.campos[0], true, false, tema));
    Cuerpo { lineas, foco }
}

/// Por qué no se puede leer y las dos salidas.
fn cuerpo_danado(dialogo: &DialogoLlavero, estrecho: bool, tema: &Tema) -> Cuerpo {
    let motivo: String = dialogo
        .motivo_danado
        .as_deref()
        .unwrap_or("el fichero no se puede leer")
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(TOPE_MOTIVO)
        .collect();
    let mut lineas = vec![
        Line::from(Span::styled(
            "No se puede leer llavero.age.".to_string(),
            estilo_error(tema),
        )),
        tenue(&motivo, tema),
        Line::from(""),
    ];
    if estrecho {
        lineas.push(texto(
            "MAGI no lo borra nunca. Apartarlo lo renombra a llavero.age.danado-<fecha> y sigues sin llavero.",
            tema,
        ));
    } else {
        lineas.push(texto(
            "MAGI no lo borra nunca. Apartarlo lo renombra a llavero.age.danado-<fecha> y sigues sin llavero: lo que guardaba no se recupera, pero la copia queda por si alguien puede arreglarla.",
            tema,
        ));
        lineas.push(texto("Si prefieres no tocar nada, sal de MAGI.", tema));
    }
    Cuerpo { lineas, foco: 0 }
}

/// El pie: lo que pasa (trabajando, error, espera) y las teclas.
fn pie(dialogo: &DialogoLlavero, estrecho: bool, tema: &Tema) -> Vec<Line<'static>> {
    let mut pie = Vec::new();
    if dialogo.trabajando.is_some() {
        pie.push(Line::from(Span::styled(
            "trabajando…".to_string(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        )));
        // La tarea de scrypt no se puede abortar y el llavero se crea, se
        // desbloquea o cambia igualmente: no se ofrece Esc.
        pie.push(tenue("espera…", tema));
        return pie;
    }
    if let Some(segundos) = dialogo.espera_restante(Instant::now()) {
        pie.push(Line::from(Span::styled(
            format!("demasiados intentos: espera {segundos} s"),
            estilo_error(tema),
        )));
    } else if let Some(error) = &dialogo.error {
        pie.push(Line::from(Span::styled(error.clone(), estilo_error(tema))));
    }
    pie.push(match dialogo.modo {
        ModoLlavero::Desbloquear => {
            if estrecho {
                teclas(&[("↵", "desbloquear"), ("Esc", "sin él")], tema)
            } else {
                teclas(&[("↵", "desbloquear"), ("Esc", "seguir sin él")], tema)
            }
        }
        ModoLlavero::Crear => {
            if estrecho {
                teclas(&[("↵", "crear"), ("Tab", "campo"), ("Esc", "no")], tema)
            } else {
                teclas(
                    &[("↵", "crear"), ("Tab", "siguiente"), ("Esc", "cancelar")],
                    tema,
                )
            }
        }
        ModoLlavero::Cambiar => {
            if estrecho {
                teclas(&[("↵", "cambiar"), ("Tab", "campo"), ("Esc", "no")], tema)
            } else {
                teclas(
                    &[("↵", "cambiar"), ("Tab", "siguiente"), ("Esc", "cancelar")],
                    tema,
                )
            }
        }
        ModoLlavero::Borrar => teclas(&[("↵", "borrar"), ("Esc", "cancelar")], tema),
        ModoLlavero::Danado => {
            if dialogo.al_arrancar {
                if estrecho {
                    teclas(&[("a", "apartar"), ("s / Esc", "salir")], tema)
                } else {
                    teclas(
                        &[("a", "apartar y seguir sin llavero"), ("s / Esc", "salir")],
                        tema,
                    )
                }
            } else if estrecho {
                teclas(&[("a", "apartar"), ("Esc", "cerrar")], tema)
            } else {
                teclas(
                    &[("a", "apartar y seguir sin llavero"), ("Esc", "cerrar")],
                    tema,
                )
            }
        }
    });
    pie
}
