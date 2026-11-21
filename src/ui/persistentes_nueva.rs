//! Pintado del diálogo «Nueva sesión persistente» (Fase 16, §6.2 del informe):
//! host (desplegable), nombre, carpeta, comando, comando de reanudación y nota,
//! con el error de validación y las teclas en el pie (fijo), y el aviso de
//! nombre ocupado. Todo se deriva del área en cada pintado; el mínimo y el modo
//! estrecho están en `disposicion.rs` (T45): por debajo de
//! `ESTRECHO_PERSISTENTES_NUEVA` la ayuda y las teclas se acortan. Lo demás
//! (partir, desplazar siguiendo el foco, ASCII) lo hace la `Hoja`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::persistentes_nueva::{
    CampoNueva, EstadoOcupado, FormularioNueva, ORDEN_CAMPOS_NUEVA,
};
use crate::app::App;
use crate::tema::Tema;
use crate::ui::componentes::CampoTexto;
use crate::ui::dialogos::{medidas, teclas, Hoja};
use crate::ui::disposicion::{
    self, Disposicion, Minimo, ESTRECHO_PERSISTENTES_NUEVA, MINIMO_PERSISTENTES_NUEVA,
};

/// Ancho que pide el diálogo con sitio.
const ANCHO: u16 = 64;
/// Opciones del desplegable de hosts visibles a la vez.
const OPCIONES_HOST: usize = 5;
/// Ancho de la columna de rótulos (el más largo, «Reanudar», lleva dos de
/// separación).
const ROTULO: usize = 10;
/// Ancho del valor del desplegable con sitio de sobra.
const RELLENO_HOST: usize = 30;

#[must_use]
pub fn minimo(_formulario: &FormularioNueva) -> Minimo {
    Minimo {
        tamano: MINIMO_PERSISTENTES_NUEVA,
        exige: "nueva sesión persistente",
    }
}

/// Pinta el diálogo como una `Hoja` de `dialogos.rs`.
pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    formulario: &FormularioNueva,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_PERSISTENTES_NUEVA;
    let (_, ancho_texto) = medidas(area, ANCHO);
    let (cuerpo, pie, foco) = match formulario.estado_ocupado() {
        Some(estado) => {
            let (cuerpo, pie) = ocupado(formulario, estado, estrecho, tema);
            (cuerpo, pie, 0)
        }
        None => formulario_completo(
            formulario,
            estrecho,
            ancho_texto,
            app.config.sincronizacion.activa(),
            tema,
        ),
    };
    Hoja::nueva("NUEVA SESIÓN PERSISTENTE", cuerpo, pie)
        .foco(foco)
        .ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO, tema, disp);
}

fn estilo_error(tema: &Tema) -> Style {
    Style::default()
        .fg(tema.paleta.critico)
        .add_modifier(Modifier::BOLD)
}

/// El formulario: los seis campos, la ayuda y, en el pie fijo, el error (si lo
/// hay) y las teclas. Devuelve también la línea del campo con el foco.
/// `sincroniza` es la sincronización del inventario configurada en este equipo:
/// solo entonces se dice que la ficha viaja a los demás (D370).
fn formulario_completo(
    formulario: &FormularioNueva,
    estrecho: bool,
    ancho_texto: usize,
    sincroniza: bool,
    tema: &Tema,
) -> (Vec<Line<'static>>, Vec<Line<'static>>, usize) {
    let activo = |campo: CampoNueva| formulario.foco() == campo;
    let estilo = |campo: CampoNueva| {
        let base = Style::default().fg(tema.paleta.texto);
        if activo(campo) {
            base.fg(tema.paleta.acento).add_modifier(Modifier::BOLD)
        } else {
            base
        }
    };
    let rotulo = |campo: CampoNueva| {
        Span::styled(
            format!("{:<ROTULO$}", campo.etiqueta()),
            Style::default().fg(if activo(campo) {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        )
    };
    let corchete =
        |texto: &'static str| Span::styled(texto, Style::default().fg(tema.paleta.acento));

    let mut cuerpo: Vec<Line<'static>> = Vec::new();
    let mut linea_con_foco = 0usize;
    for campo in ORDEN_CAMPOS_NUEVA {
        if activo(campo) {
            linea_con_foco = cuerpo.len();
        }
        if campo == CampoNueva::Host {
            cuerpo.push(linea_host(formulario, ancho_texto, tema));
            if formulario.desplegable().abierto {
                let (lineas, resaltada) = lineas_desplegable(formulario, tema);
                // Con la lista abierta, el foco es la opción resaltada.
                linea_con_foco = cuerpo.len() + resaltada;
                cuerpo.extend(lineas);
            }
            continue;
        }
        let texto: &CampoTexto = formulario
            .campo(campo)
            .expect("todos los campos salvo el host son de texto");
        // Un campo opcional vacío y sin el foco dice qué pasa si se deja así.
        let valor = match sugerencia(campo) {
            Some(sugerencia) if texto.texto.is_empty() && !activo(campo) => Span::styled(
                format!("({sugerencia})"),
                Style::default().fg(tema.paleta.inactivo),
            ),
            _ => texto.span(activo(campo), estilo(campo)),
        };
        cuerpo.push(Line::from(vec![
            rotulo(campo),
            corchete("[ "),
            valor,
            corchete(" ]"),
        ]));
    }

    cuerpo.push(Line::from(""));
    let host = formulario.host_nombre();
    let (vive, secretos) = match (sincroniza, estrecho) {
        (true, true) => (
            format!("Vive en {host} y la ves en todos tus equipos."),
            "Sin secretos en el comando: la ficha viaja.".to_string(),
        ),
        (true, false) => (
            format!(
                "Vive en {host} aunque cierres MAGI o apagues el equipo, y la ves en todos tus equipos."
            ),
            "La ficha viaja cifrada a tus otros equipos: no pongas secretos en el comando."
                .to_string(),
        ),
        (false, true) => (
            format!("Vive en {host}."),
            "Sin secretos en el comando.".to_string(),
        ),
        (false, false) => (
            format!("Vive en {host} aunque cierres MAGI o apagues el equipo."),
            "No pongas secretos en el comando: queda guardado en la ficha.".to_string(),
        ),
    };
    cuerpo.push(Line::from(Span::styled(
        vive,
        Style::default().fg(tema.paleta.inactivo),
    )));
    cuerpo.push(Line::from(Span::styled(
        secretos,
        Style::default().fg(tema.paleta.inactivo),
    )));

    let mut pie: Vec<Line<'static>> = Vec::new();
    if let Some(error) = formulario.error() {
        pie.push(Line::from(Span::styled(
            format!("{} {error}", tema.glifos.error),
            estilo_error(tema),
        )));
    }
    let abierto = formulario.desplegable().abierto;
    let pares: Vec<(&str, &str)> = if abierto {
        vec![("↵", "elegir"), ("esc", "cerrar")]
    } else if estrecho {
        if formulario.foco() == CampoNueva::Host {
            vec![("↵", "crear"), ("espacio", "host"), ("esc", "cancelar")]
        } else {
            vec![("↵", "crear"), ("⇥", "sig."), ("esc", "cancelar")]
        }
    } else if formulario.foco() == CampoNueva::Host {
        vec![
            ("↵", "crear y engancharse"),
            ("espacio", "cambiar de host"),
            ("⇥", "siguiente"),
            ("esc", "cancelar"),
        ]
    } else {
        vec![
            ("↵", "crear y engancharse"),
            ("⇥", "siguiente"),
            ("esc", "cancelar"),
        ]
    };
    pie.push(teclas(&pares, tema));
    if abierto && !formulario.desplegable().filtro.texto.is_empty() {
        pie.push(Line::from(Span::styled(
            format!("filtro: {}", formulario.desplegable().filtro.texto),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    (cuerpo, pie, linea_con_foco)
}

/// Lo que significa dejar vacío un campo opcional.
fn sugerencia(campo: CampoNueva) -> Option<&'static str> {
    match campo {
        CampoNueva::Carpeta => Some("carpeta personal"),
        CampoNueva::Comando => Some("shell de acceso"),
        CampoNueva::Reanudar => Some("el mismo comando"),
        CampoNueva::Nota => Some("opcional"),
        CampoNueva::Host | CampoNueva::Nombre => None,
    }
}

/// `Host      [ dev-01 ▾            ]`, con la lista debajo si está abierta.
fn linea_host(formulario: &FormularioNueva, ancho_texto: usize, tema: &Tema) -> Line<'static> {
    let activo = formulario.foco() == CampoNueva::Host;
    let abierto = formulario.desplegable().abierto;
    let valor = if formulario.desplegable().opciones.is_empty() {
        "(no hay hosts)".to_string()
    } else {
        format!("{} \u{25be}", formulario.host_nombre())
    };
    // Se rellena hasta 30 columnas, o hasta donde quepa con el rótulo y los
    // corchetes para no dejar el `]` en otro renglón.
    let relleno = RELLENO_HOST.min(ancho_texto.saturating_sub(ROTULO + 4));
    let estilo = if abierto {
        Style::default()
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
    } else if activo {
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.texto)
    };
    let corchete =
        |texto: &'static str| Span::styled(texto, Style::default().fg(tema.paleta.acento));
    Line::from(vec![
        Span::styled(
            format!("{:<ROTULO$}", CampoNueva::Host.etiqueta()),
            Style::default().fg(if activo {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
        corchete("[ "),
        Span::styled(format!("{valor:<relleno$}"), estilo),
        corchete(" ]"),
    ])
}

/// Las opciones del desplegable abierto (una ventana que sigue a la
/// resaltada) y el índice, dentro de ellas, de la resaltada.
fn lineas_desplegable(formulario: &FormularioNueva, tema: &Tema) -> (Vec<Line<'static>>, usize) {
    let desplegable = formulario.desplegable();
    let filtradas = desplegable.filtradas();
    let inicio = disposicion::ventana(0, desplegable.resaltado, OPCIONES_HOST, filtradas.len());
    let mut lineas = Vec::new();
    let mut resaltada_en = 0usize;
    for (posicion, indice) in filtradas
        .iter()
        .enumerate()
        .skip(inicio)
        .take(OPCIONES_HOST)
    {
        let opcion = &desplegable.opciones[*indice];
        let resaltada = posicion == desplegable.resaltado;
        if resaltada {
            resaltada_en = lineas.len();
        }
        lineas.push(Line::from(Span::styled(
            format!(
                "  {} {}",
                if resaltada {
                    tema.glifos.seleccion
                } else {
                    " "
                },
                opcion.etiqueta
            ),
            if resaltada {
                Style::default()
                    .bg(tema.paleta.acento)
                    .fg(tema.paleta.fondo)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(tema.paleta.texto)
            },
        )));
    }
    if filtradas.is_empty() {
        lineas.push(Line::from(Span::styled(
            "  (ningún host coincide)",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let debajo = filtradas.len().saturating_sub(inicio + OPCIONES_HOST);
    if debajo > 0 {
        lineas.push(Line::from(Span::styled(
            format!("    … {debajo} más"),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    (lineas, resaltada_en)
}

/// El nombre ya existe: avisa y ofrece engancharse (o reabrir, si es una
/// ficha terminada) o cambiar el nombre. Con la sesión sin comprobar no
/// afirma que exista: ofrece abrirla (se engancha si existe y, si no, se
/// reabre). Las teclas van en el pie fijo.
fn ocupado(
    formulario: &FormularioNueva,
    estado: EstadoOcupado,
    estrecho: bool,
    tema: &Tema,
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let nombre = formulario.texto(CampoNueva::Nombre).trim().to_string();
    let host = formulario.host_nombre();
    let aviso = match estado {
        EstadoOcupado::Terminada => {
            format!("La sesión «{nombre}» de {host} ya terminó, pero su ficha sigue ahí.")
        }
        EstadoOcupado::Viva => format!("Ya hay una sesión «{nombre}» en {host}."),
        EstadoOcupado::SinComprobar => {
            format!("Ya hay una ficha «{nombre}» en {host}: puede que su sesión exista.")
        }
    };
    let consejo = match (estado, estrecho) {
        (EstadoOcupado::Terminada, false) => {
            "Puedes reabrirla con su comando o elegir otro nombre."
        }
        (EstadoOcupado::Terminada, true) => "Reábrela o cambia el nombre.",
        (EstadoOcupado::Viva, false) => "Puedes engancharte a la existente o elegir otro nombre.",
        (EstadoOcupado::Viva, true) => "Engánchate o cambia el nombre.",
        (EstadoOcupado::SinComprobar, false) => {
            "Ábrela: si ya existe podrás engancharte; si no, se reabre con su comando."
        }
        (EstadoOcupado::SinComprobar, true) => "Ábrela o cambia el nombre.",
    };
    let cuerpo = vec![
        Line::from(Span::styled(
            format!("{} {aviso}", tema.glifos.error),
            estilo_error(tema),
        )),
        Line::from(""),
        Line::from(Span::raw(consejo)),
    ];
    let accion = match estado {
        EstadoOcupado::Terminada => "reabrirla",
        EstadoOcupado::Viva => "engancharse",
        EstadoOcupado::SinComprobar => "abrirla",
    };
    let pares: Vec<(&str, &str)> = if estrecho {
        vec![("↵", accion), ("c", "cambiar"), ("esc", "cancelar")]
    } else {
        vec![
            ("↵", accion),
            ("c", "cambiar el nombre"),
            ("esc", "cancelar"),
        ]
    };
    (cuerpo, vec![teclas(&pares, tema)])
}
