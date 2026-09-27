//! Vista Sesión: marco con el título, barra de pestañas, el terminal remoto y
//! la barra de estado de la sesión.
//!
//! Todo se deriva del área en cada pintado (Fase 7): la barra de pestañas se
//! compacta en modo estrecho (solo glifo y número en las pestañas que no son
//! la activa) y la barra de estado quita elementos por prioridad. El terminal
//! se pinta solo en el área que ocupa el remoto; lo que sobra, con relleno.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tui_term::widget::{Cursor, PseudoTerminal};

use crate::app::{App, PestanaUI};
use crate::protocolo::EstadoSesionRemota;
use crate::tema::Tema;
use crate::ui::disposicion::{self, Disposicion};

/// Largo máximo del nombre de una pestaña en la barra.
const NOMBRE_MAX: usize = 14;
/// Pestañas de la forma completa: el prefijo `1`-`9` solo llega a nueve; con
/// más se ve una ventana alrededor de la activa con `‹ ›`.
const PESTANAS_COMPLETAS: usize = 9;
/// Por debajo de este largo el nombre de la pestaña activa no se recorta: se
/// quita (queda solo glifo y número).
const NOMBRE_MIN: usize = 3;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, _disp: &mut Disposicion) {
    let Some(indice) = app.pestana_activa else {
        return;
    };
    let Some(pestaña) = app.pestanas.get(indice) else {
        return;
    };
    // Filas fijas: el marco con el título, la barra de pestañas y la barra de
    // estado de la sesión. Con la barra global que pinta `ui::dibujar` son las
    // cuatro que descuenta `alto_pty`: el contenido mide exactamente
    // `alto_pty(filas de la ventana)`, el alto que el servidor conoce.
    let trozos = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    marco.render_widget(
        Paragraph::new(linea_titulo(pestaña, area.width, &app.tema)),
        trozos[0],
    );
    marco.render_widget(
        Paragraph::new(barra_pestañas(app, indice, area.width)),
        trozos[1],
    );
    let contenido = trozos[2];
    if contenido.height == 0 {
        return;
    }
    if pestaña.estado == EstadoSesionRemota::Caida {
        // La última pantalla se conserva en gris detrás del aviso, sin cursor
        // (no hay nadie al otro lado).
        if let Ok(parser) = pestaña.pantalla.lock() {
            let widget = PseudoTerminal::new(parser.screen())
                .cursor(cursor(&app.tema).visibility(false))
                .style(Style::default().fg(app.tema.paleta.inactivo));
            marco.render_widget(widget, contenido);
        }
        aviso_caida(marco, contenido, app, pestaña);
    } else {
        relleno(marco, contenido, app, pestaña);
        if let Ok(parser) = pestaña.pantalla.lock() {
            let widget = PseudoTerminal::new(parser.screen()).cursor(cursor(&app.tema));
            marco.render_widget(widget, area_remota(contenido, pestaña));
        }
    }
    marco.render_widget(
        Paragraph::new(barra_estado(app, indice, pestaña, contenido)),
        trozos[3],
    );
}

/// Cursor del terminal. En ASCII, un espacio en vídeo inverso en lugar de `█`.
fn cursor(tema: &Tema) -> Cursor {
    if tema.ascii {
        Cursor::default()
            .symbol(" ")
            .style(Style::default().add_modifier(Modifier::REVERSED))
    } else {
        Cursor::default()
    }
}

/// Zona que ocupa el remoto: su tamaño vigente, si es menor que el área (otra
/// ventana más pequeña impone el mínimo). El resto queda con el relleno, que
/// el terminal taparía si se pintase sobre toda el área.
fn area_remota(area: Rect, pestaña: &PestanaUI) -> Rect {
    if pestaña.cols_remoto == 0 || pestaña.filas_remoto == 0 {
        return area;
    }
    Rect {
        width: area.width.min(pestaña.cols_remoto),
        height: area.height.min(pestaña.filas_remoto),
        ..area
    }
}

/// ¿Sobra área porque el remoto es menor (otra ventana impone el mínimo)?
/// Decide a la vez el relleno y el indicador «c×f (mín. ventana N)».
fn hay_relleno(area: Rect, pestaña: &PestanaUI) -> bool {
    pestaña.estado == EstadoSesionRemota::Abierta
        && pestaña.cols_remoto > 0
        && pestaña.filas_remoto > 0
        && (area.width > pestaña.cols_remoto || area.height > pestaña.filas_remoto)
}

/// Relleno tenue (`░`, `.` en ASCII) para la zona que no cubre el remoto
/// cuando otra ventana más pequeña comparte la pestaña.
fn relleno(marco: &mut Frame, area: Rect, app: &App, pestaña: &PestanaUI) {
    if !hay_relleno(area, pestaña) {
        return;
    }
    let fila = app.tema.glifos.relleno.repeat(usize::from(area.width));
    let lineas: Vec<Line> = (0..area.height)
        .map(|_| {
            Line::from(Span::styled(
                fila.clone(),
                Style::default().fg(app.tema.paleta.inactivo),
            ))
        })
        .collect();
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Aviso de pestaña caída. En áreas bajas se quitan primero las líneas en
/// blanco y después el motivo; las acciones se abrevian si no caben.
fn aviso_caida(marco: &mut Frame, area: Rect, app: &App, pestaña: &PestanaUI) {
    let tema = &app.tema;
    let motivo = pestaña
        .motivo
        .clone()
        .unwrap_or_else(|| "conexión perdida".to_string());
    let titulo = format!(
        "{} Sesión caída hace {}",
        tema.glifos.error,
        formatear_duracion(pestaña.segundos())
    );
    // Márgenes: el borde y un espacio a cada lado.
    let interior_max = usize::from(area.width.saturating_sub(4));
    let prefijo = &app.config.prefijo_escape;
    let acciones = format!("{prefijo} r  reconectar        {prefijo} x  cerrar");
    let acciones = if acciones.chars().count() <= interior_max {
        acciones
    } else {
        let corto = prefijo_corto(prefijo);
        format!("{corto} r reconectar  {corto} x cerrar")
    };
    let critico = Style::default()
        .fg(tema.paleta.critico)
        .add_modifier(Modifier::BOLD);
    let texto = Style::default().fg(tema.paleta.texto);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    // (texto, prioridad, estilo) en el orden en que se pintan.
    let todas = [
        (String::new(), 4, texto),
        (titulo, 1, critico),
        (motivo, 3, texto),
        (String::new(), 4, texto),
        (acciones, 2, tenue),
    ];
    let alto = (todas.len() as u16 + 2).min(area.height);
    let filas = usize::from(alto.saturating_sub(2));
    let mut orden: Vec<usize> = (0..todas.len()).collect();
    orden.sort_by_key(|indice| (todas[*indice].1, *indice));
    orden.truncate(filas);
    orden.sort_unstable();
    let ancho_texto = orden
        .iter()
        .map(|indice| todas[*indice].0.chars().count())
        .max()
        .unwrap_or(0);
    let ancho = u16::try_from(ancho_texto)
        .unwrap_or(u16::MAX)
        .saturating_add(4);
    let recta = disposicion::centrar_limitado(area, ancho, alto);
    marco.render_widget(super::bloque("SESIÓN CAÍDA", tema), recta);
    let interior = Rect {
        x: recta.x + 2,
        y: recta.y + 1,
        width: recta.width.saturating_sub(4),
        height: recta.height.saturating_sub(2),
    };
    let lineas: Vec<Line> = orden
        .into_iter()
        .map(|indice| {
            let (linea, _, estilo) = &todas[indice];
            Line::from(Span::styled(
                disposicion::recortar(linea, usize::from(interior.width), tema.ascii),
                *estilo,
            ))
        })
        .collect();
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Línea de marco superior: `MAGI · SESIÓN` a la izquierda y el nombre de la
/// pestaña a la derecha, recortado (o quitado) si no cabe.
fn linea_titulo(pestaña: &PestanaUI, ancho: u16, tema: &Tema) -> Line<'static> {
    let ancho = usize::from(ancho);
    let izquierda = format!(" MAGI {} SESIÓN ", tema.glifos.punto_medio);
    let fijo = izquierda.chars().count();
    // Hueco para el nombre: con un espacio a cada lado y al menos un trazo
    // antes y otro después.
    let hueco = ancho.saturating_sub(fijo + 4);
    let derecha = if hueco >= NOMBRE_MIN {
        format!(" {} ", recortar_nombre(&pestaña.nombre, hueco, tema.ascii))
    } else {
        String::new()
    };
    let trazos = ancho.saturating_sub(fijo + derecha.chars().count() + 1);
    let acento = Style::default()
        .fg(tema.paleta.acento)
        .add_modifier(Modifier::BOLD);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    Line::from(vec![
        Span::styled(izquierda, acento),
        Span::styled(tema.glifos.linea.repeat(trazos), tenue),
        Span::styled(derecha, Style::default().fg(tema.paleta.texto)),
        Span::styled(tema.glifos.linea.to_string(), tenue),
    ])
}

/// Nombre recortado a `ancho` sin dejar un espacio colgando antes de la marca
/// de recorte («vps-openclaw …» → «vps-openclaw…»).
fn recortar_nombre(nombre: &str, ancho: usize, ascii: bool) -> String {
    let recortado = disposicion::recortar(nombre, ancho, ascii);
    if recortado == nombre {
        return recortado;
    }
    let marca = if ascii { "~" } else { "…" };
    let cuerpo = recortado
        .strip_suffix(marca)
        .unwrap_or(&recortado)
        .trim_end();
    format!("{cuerpo}{marca}")
}

/// Forma de la barra de pestañas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Forma {
    /// Glifo y nombre en todas las pestañas y `+ nueva` (modo normal).
    Completa,
    /// Glifo y número; el nombre solo en la activa (hasta `nombre_max`, 0 =
    /// sin nombre) y `+` al final.
    Compacta { nombre_max: usize },
}

/// Barra de pestañas. En modo normal, la forma completa; si no cabe o en modo
/// estrecho, la compacta con todas las pestañas; si tampoco, la compacta con
/// una ventana alrededor de la activa (`‹ ›`), recortando su nombre y, en
/// último caso, quitándolo. La activa siempre se ve, marcada con `▸`.
fn barra_pestañas(app: &App, activa: usize, ancho: u16) -> Line<'static> {
    let total = app.pestanas.len();
    let ancho_util = usize::from(ancho);
    let cabe = |linea: &Line| linea.width() <= ancho_util;
    if !disposicion::es_estrecho(ancho) {
        let cuantas = total.min(PESTANAS_COMPLETAS);
        let primera = activa
            .saturating_sub(cuantas / 2)
            .min(total.saturating_sub(cuantas));
        let linea = linea_pestañas(app, activa, primera, primera + cuantas, Forma::Completa);
        if cabe(&linea) {
            return con_pista(app, linea, ancho_util);
        }
    }
    let compacta = Forma::Compacta {
        nombre_max: NOMBRE_MAX,
    };
    let linea = linea_pestañas(app, activa, 0, total, compacta);
    if cabe(&linea) {
        return con_pista(app, linea, ancho_util);
    }
    let nombres = (NOMBRE_MIN..=NOMBRE_MAX).rev().chain([0]);
    for nombre_max in nombres {
        let forma = Forma::Compacta { nombre_max };
        let cabe_ventana = |primera: usize, fin: usize| {
            linea_pestañas(app, activa, primera, fin, forma).width() <= ancho_util
        };
        if !cabe_ventana(activa, activa + 1) && nombre_max > 0 {
            continue;
        }
        // La ventana crece alrededor de la activa mientras quepa.
        let (mut primera, mut fin) = (activa, activa + 1);
        loop {
            let mut crecio = false;
            if fin < total && cabe_ventana(primera, fin + 1) {
                fin += 1;
                crecio = true;
            }
            if primera > 0 && cabe_ventana(primera - 1, fin) {
                primera -= 1;
                crecio = true;
            }
            if !crecio {
                break;
            }
        }
        let linea = linea_pestañas(app, activa, primera, fin, forma);
        return con_pista(app, linea, ancho_util);
    }
    // No se llega aquí (la forma sin nombre siempre vale), pero sin pánico.
    linea_pestañas(
        app,
        activa,
        activa,
        activa + 1,
        Forma::Compacta { nombre_max: 0 },
    )
}

/// Pestañas `primera..fin` en la forma dada, con `‹ ›` si hay más a los lados
/// y `+ nueva` (o `+`) al final.
fn linea_pestañas(
    app: &App,
    activa: usize,
    primera: usize,
    fin: usize,
    forma: Forma,
) -> Line<'static> {
    let tema = &app.tema;
    let total = app.pestanas.len();
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let separador = format!(" {} ", tema.glifos.separador);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if primera > 0 {
        spans.push(Span::styled(
            format!("{} ", super::tecla(tema, "‹", "<")),
            tenue,
        ));
    }
    for (indice, pestaña) in app.pestanas.iter().enumerate().take(fin).skip(primera) {
        let es_activa = indice == activa;
        if indice > primera {
            spans.push(Span::styled(separador.clone(), tenue));
        }
        // La primera de todas lleva un espacio de margen; la activa, `▸`.
        let marca = if es_activa {
            tema.glifos.seleccion
        } else if indice == 0 {
            " "
        } else {
            ""
        };
        spans.push(Span::styled(
            format!("{marca}{}", glifo_pestaña(es_activa, pestaña, tema)),
            estilo_pestaña(es_activa, pestaña, tema),
        ));
        let texto = match forma {
            Forma::Completa => format!(
                " {}",
                recortar_nombre(&pestaña.nombre, NOMBRE_MAX, tema.ascii)
            ),
            Forma::Compacta { nombre_max } if es_activa && nombre_max > 0 => format!(
                " {} {}",
                indice + 1,
                recortar_nombre(&pestaña.nombre, nombre_max, tema.ascii)
            ),
            Forma::Compacta { .. } => format!(" {}", indice + 1),
        };
        let mut estilo_nombre = Style::default().fg(tema.paleta.texto);
        if es_activa {
            estilo_nombre = estilo_nombre.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(texto, estilo_nombre));
    }
    if fin < total {
        spans.push(Span::styled(
            format!(" {}", super::tecla(tema, "›", ">")),
            tenue,
        ));
    }
    let nueva = match forma {
        Forma::Completa => "+ nueva",
        Forma::Compacta { .. } => "+",
    };
    spans.push(Span::styled(format!("{separador}{nueva}"), tenue));
    Line::from(spans)
}

/// Con diez pestañas o más el prefijo `1`-`9` no llega a todas: se añade la
/// pista (entera o abreviada) si cabe tras las pestañas.
fn con_pista(app: &App, mut linea: Line<'static>, ancho: usize) -> Line<'static> {
    let total = app.pestanas.len();
    if total < 10 {
        return linea;
    }
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    let pistas = [
        format!(
            "  {punto} {total} sesiones: usa {} {} o la lista",
            super::tecla(tema, "‹", "<"),
            super::tecla(tema, "›", ">")
        ),
        format!("  {punto} {total} sesiones"),
    ];
    if let Some(pista) = pistas
        .into_iter()
        .find(|pista| linea.width() + Span::raw(pista.as_str()).width() <= ancho)
    {
        linea
            .spans
            .push(Span::styled(pista, Style::default().fg(tema.paleta.acento)));
    }
    linea
}

fn glifo_pestaña(activa: bool, pestaña: &PestanaUI, tema: &Tema) -> String {
    let glifo = match pestaña.estado {
        EstadoSesionRemota::Caida | EstadoSesionRemota::Cerrada => tema.glifos.error, // ✕
        EstadoSesionRemota::Abierta => {
            if pestaña.actividad_no_vista && !activa {
                tema.glifos.conectando // ◐
            } else {
                tema.glifos.conectado // ●
            }
        }
        EstadoSesionRemota::Abriendo => tema.glifos.conectado,
    };
    glifo.to_string()
}

fn estilo_pestaña(activa: bool, pestaña: &PestanaUI, tema: &Tema) -> Style {
    let base = Style::default();
    match pestaña.estado {
        EstadoSesionRemota::Caida => base.fg(tema.paleta.critico),
        _ => {
            if activa {
                base.fg(tema.paleta.correcto).add_modifier(Modifier::BOLD)
            } else if pestaña.actividad_no_vista {
                base.fg(tema.paleta.acento)
            } else {
                base.fg(tema.paleta.correcto)
            }
        }
    }
}

/// El prefijo en su forma corta de terminal: `Ctrl+]` → `^]`. Los demás
/// (con `Alt`, varias teclas o teclas con nombre) se dejan como están.
fn prefijo_corto(prefijo: &str) -> String {
    let texto = prefijo.trim();
    if let Some((modificador, tecla)) = texto.split_once('+') {
        let modificador = modificador.trim();
        let tecla = tecla.trim();
        if (modificador.eq_ignore_ascii_case("ctrl") || modificador.eq_ignore_ascii_case("control"))
            && tecla.chars().count() == 1
        {
            return format!("^{tecla}");
        }
    }
    texto.to_string()
}

/// Un elemento de una barra con su prioridad (1 = no se quita nunca) y su
/// texto completo y compacto.
struct Elemento {
    completo: String,
    compacto: String,
    prioridad: u8,
    estilo: Style,
}

impl Elemento {
    fn new(completo: String, compacto: String, prioridad: u8, estilo: Style) -> Self {
        Self {
            completo,
            compacto,
            prioridad,
            estilo,
        }
    }

    fn fijo(texto: String, prioridad: u8, estilo: Style) -> Self {
        Self::new(texto.clone(), texto, prioridad, estilo)
    }
}

/// Compone una barra: `cabeza` y los elementos separados por ` · `. En modo
/// normal prueba primero los textos completos; después, los compactos, y si
/// aún no caben quita elementos de menor a mayor prioridad (a igualdad, el
/// último). Los de prioridad 1 no se quitan nunca.
fn componer(cabeza: &Elemento, elementos: &[Elemento], ancho: u16, tema: &Tema) -> Line<'static> {
    let ancho_util = usize::from(ancho);
    let mut quitables: Vec<usize> = (0..elementos.len())
        .filter(|indice| elementos[*indice].prioridad > 1)
        .collect();
    quitables.sort_by_key(|indice| std::cmp::Reverse((elementos[*indice].prioridad, *indice)));
    let mut intentos: Vec<(bool, usize)> = Vec::new();
    if !disposicion::es_estrecho(ancho) {
        intentos.push((false, 0));
    }
    intentos.extend((0..=quitables.len()).map(|quitados| (true, quitados)));
    let separador = format!(" {} ", tema.glifos.punto_medio);
    let construir = |compacto: bool, quitados: usize| -> Line<'static> {
        let texto = |elemento: &Elemento| {
            if compacto {
                elemento.compacto.clone()
            } else {
                elemento.completo.clone()
            }
        };
        let mut spans = vec![Span::styled(texto(cabeza), cabeza.estilo)];
        let visibles = elementos
            .iter()
            .enumerate()
            .filter(|(indice, _)| !quitables[..quitados].contains(indice))
            .map(|(_, elemento)| elemento);
        for (posicion, elemento) in visibles.enumerate() {
            if posicion > 0 {
                spans.push(Span::styled(
                    separador.clone(),
                    Style::default().fg(tema.paleta.inactivo),
                ));
            }
            spans.push(Span::styled(texto(elemento), elemento.estilo));
        }
        Line::from(spans)
    };
    let mut ultima = Line::default();
    for (compacto, quitados) in intentos {
        ultima = construir(compacto, quitados);
        if ultima.width() <= ancho_util {
            break;
        }
    }
    ultima
}

/// Barra de estado de la sesión, compactada por prioridad: glifo · posición ·
/// host · tiempo · identidad · carga · ventanas (y túneles y el tamaño
/// remoto, los últimos). Con relleno, el indicador `c×f (mín. ventana N)` no
/// se quita nunca, igual que el glifo y la ayuda.
fn barra_estado(app: &App, indice: usize, pestaña: &PestanaUI, area: Rect) -> Line<'static> {
    let tema = &app.tema;
    if app.modo_prefijo {
        return barra_prefijo(app, area.width);
    }
    let texto = Style::default().fg(tema.paleta.texto);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let cabeza = Elemento::fijo(
        format!(" {} ", glifo_pestaña(true, pestaña, tema)),
        1,
        estilo_pestaña(true, pestaña, tema),
    );
    let tiempo = formatear_duracion(pestaña.segundos());
    let mut elementos = vec![
        Elemento::fijo(format!("{}/{}", indice + 1, app.pestanas.len()), 2, texto),
        Elemento::fijo(pestaña.host_nombre.clone(), 3, texto),
        Elemento::new(format!("conectado {tiempo}"), tiempo, 4, texto),
    ];
    let identidad = if pestaña.identidad.is_empty() {
        pestaña.motivo.clone().unwrap_or_default()
    } else {
        pestaña.identidad.clone()
    };
    if !identidad.is_empty() {
        elementos.push(Elemento::fijo(identidad, 5, texto));
    }
    if pestaña.cols_remoto > 0 {
        let mut tamano = format!(
            "{}{}{}",
            pestaña.cols_remoto, tema.glifos.por, pestaña.filas_remoto
        );
        let prioridad = if hay_relleno(area, pestaña) {
            // Otra ventana más pequeña comparte la pestaña y fija el tamaño:
            // explica el relleno, así que se queda siempre.
            match pestaña.ventana_minima {
                Some(minima) if Some(minima) == app.cliente_id => {
                    tamano.push_str(" (mín. esta ventana)");
                }
                Some(minima) => tamano.push_str(&format!(" (mín. ventana {minima})")),
                None => tamano.push_str(" (mín. otra ventana)"),
            }
            1
        } else {
            9
        };
        elementos.push(Elemento::fijo(tamano, prioridad, texto));
    }
    let carga = app.sondeos.get(&pestaña.host_id).and_then(|sondeo| {
        let edad = crate::flota::estado::antiguedad_segundos(&sondeo.fecha)?;
        if edad > 600.0 {
            return None;
        }
        sondeo.carga_1m.map(|carga| format!("carga {carga:.1}"))
    });
    if let Some(carga) = carga {
        elementos.push(Elemento::fijo(carga, 6, texto));
    }
    let activos = super::hosts::tuneles_activos_de_host(app, pestaña.host_id);
    if activos > 0 {
        elementos.push(Elemento::fijo(format!("túneles {activos}"), 8, texto));
    }
    let ventanas = if pestaña.ventanas == 1 {
        "1 ventana".to_string()
    } else {
        format!("{} ventanas", pestaña.ventanas)
    };
    elementos.push(Elemento::fijo(ventanas, 7, texto));
    let prefijo = &app.config.prefijo_escape;
    elementos.push(Elemento::new(
        format!("{prefijo} ? ayuda"),
        format!("{} ?", prefijo_corto(prefijo)),
        1,
        tenue,
    ));
    componer(&cabeza, &elementos, area.width, tema)
}

/// Barra del modo prefijo: qué hace cada tecla tras el prefijo, compactada
/// por prioridad como la de estado.
fn barra_prefijo(app: &App, ancho: u16) -> Line<'static> {
    let tema = &app.tema;
    let prefijo = &app.config.prefijo_escape;
    let cabeza = Elemento::new(
        format!(" [MAGI] {prefijo} "),
        format!(" [MAGI] {} ", prefijo_corto(prefijo)),
        1,
        Style::default()
            .fg(tema.paleta.fondo)
            .bg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    );
    let estilo = Style::default().fg(tema.paleta.acento);
    let mut elementos: Vec<Elemento> = [
        ("1-9 n p pestañas", 1),
        ("l lista", 3),
        ("c conectar", 5),
        ("x cerrar", 4),
        ("r reconectar", 6),
        ("w ventana", 7),
        ("q volver", 2),
        ("prefijo de nuevo = literal", 8),
    ]
    .into_iter()
    .map(|(texto, prioridad)| Elemento::fijo(texto.to_string(), prioridad, estilo))
    .collect();
    // La primera va pegada a la marca, con un espacio.
    elementos[0].completo.insert(0, ' ');
    elementos[0].compacto.insert(0, ' ');
    componer(&cabeza, &elementos, ancho, tema)
}

pub fn formatear_duracion(segundos: u64) -> String {
    if segundos < 60 {
        format!("{segundos} s")
    } else if segundos < 3600 {
        format!("{} m", segundos / 60)
    } else {
        format!("{} h {:02} m", segundos / 3600, (segundos % 3600) / 60)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_prefijo_corto_abrevia_solo_ctrl_con_una_tecla() {
        assert_eq!(prefijo_corto("Ctrl+]"), "^]");
        assert_eq!(prefijo_corto(" ctrl + a "), "^a");
        assert_eq!(prefijo_corto("Alt+x"), "Alt+x");
        assert_eq!(prefijo_corto("Ctrl+Shift+x"), "Ctrl+Shift+x");
        assert_eq!(prefijo_corto("Ctrl+esc"), "Ctrl+esc");
    }

    #[test]
    fn componer_quita_por_prioridad_y_conserva_las_imprescindibles() {
        let tema = Tema::respaldo();
        let estilo = Style::default();
        let cabeza = Elemento::fijo(" * ".to_string(), 1, estilo);
        let elementos = [
            Elemento::fijo("1/3".to_string(), 2, estilo),
            Elemento::new("largo largo".to_string(), "corto".to_string(), 3, estilo),
            Elemento::fijo("xxxxxxxx".to_string(), 4, estilo),
            Elemento::fijo("ayuda".to_string(), 1, estilo),
        ];
        let texto = |linea: Line| -> String {
            linea
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        };
        // Normal y con sitio: todo completo.
        assert_eq!(
            texto(componer(&cabeza, &elementos, 120, &tema)),
            " * 1/3 · largo largo · xxxxxxxx · ayuda"
        );
        // Estrecho: compacto y quitando la de menor prioridad.
        assert_eq!(
            texto(componer(&cabeza, &elementos, 22, &tema)),
            " * 1/3 · corto · ayuda"
        );
        // Sin sitio para nada: quedan las de prioridad 1.
        assert_eq!(texto(componer(&cabeza, &elementos, 5, &tema)), " * ayuda");
    }
}
