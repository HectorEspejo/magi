//! Vista Sesión (Fase 15): paneles con cabecera, barra de pestañas, modo
//! historial, enlaces, escritura en varios y la barra de estado.
//!
//! Todo se deriva del área en cada pintado: la disposición de paneles se
//! reparte aquí y se deja en `Disposicion::panes` para que `Geometria` mande
//! el tamaño de cada PTY. El terminal de cada panel se pinta con tui-term,
//! salvo que su salida sincronizada (2026) pida retener el pintado.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tui_term::widget::{Cursor, PseudoTerminal};

use crate::app::{paneles::DisposicionPaneles, App, PestanaUI};
use crate::protocolo::{EstadoSesionRemota, InfoReconexion};
use crate::tema::Tema;
use crate::ui::disposicion::{self, Disposicion};

/// Largo máximo del nombre de una pestaña en la barra.
const NOMBRE_MAX: usize = 14;
const PESTANAS_COMPLETAS: usize = 9;
const NOMBRE_MIN: usize = 3;
/// Alto de la cabecera de cada panel.
const CABECERA: u16 = 1;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let Some(indice) = app.pestana_activa else {
        return;
    };
    let Some(pestana) = app.pestanas.get(indice) else {
        return;
    };
    let ampliada = app.paneles.ampliado || app.paneles.disposicion == DisposicionPaneles::Completa;
    let banner = app.escritura_varios.activa();
    let trozos = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(banner)),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    marco.render_widget(
        Paragraph::new(linea_titulo(app, pestana, area.width, ampliada)),
        trozos[0],
    );
    marco.render_widget(
        Paragraph::new(barra_pestañas(app, indice, area.width)),
        trozos[1],
    );
    if banner {
        marco.render_widget(Paragraph::new(linea_escritura(app, area.width)), trozos[2]);
    }
    let contenido = trozos[3];
    if contenido.height == 0 {
        return;
    }
    // Modo historial: ocupa el contenido entero y su barra sustituye a la de
    // estado.
    if app.historial.is_some() {
        disp.modo_area = Some(contenido);
        pintar_historial(marco, contenido, app, disp);
        marco.render_widget(
            Paragraph::new(barra_historial(app, contenido.width)),
            trozos[4],
        );
        return;
    }
    if app.enlaces.activo {
        disp.modo_area = Some(contenido);
        pintar_panel_unico(marco, contenido, app, pestana);
        pintar_enlaces(marco, contenido, app);
        marco.render_widget(
            Paragraph::new(barra_enlaces(app, contenido.width)),
            trozos[4],
        );
        return;
    }
    // Paneles.
    let reparto = app.paneles.reparto(contenido);
    let indices = app.paneles.indices(&app.pestanas);
    for (panel, zona) in &reparto {
        let contenido_panel = Rect {
            x: zona.x,
            y: zona.y.saturating_add(CABECERA),
            width: zona.width,
            height: zona.height.saturating_sub(CABECERA),
        };
        disp.panes.push((*panel, contenido_panel));
        pintar_cabecera(
            marco,
            *zona,
            app,
            *panel,
            indices.get(*panel).copied().flatten(),
        );
        let pestaña_panel = indices
            .get(*panel)
            .copied()
            .flatten()
            .and_then(|indice| app.pestanas.get(indice));
        match pestaña_panel {
            Some(pestaña_panel) => {
                pintar_pantalla(marco, contenido_panel, app, pestaña_panel);
            }
            None => {
                marco.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!(" {} 1-9 para elegir pestaña", app.config.prefijo_escape),
                        Style::default().fg(app.tema.paleta.inactivo),
                    ))),
                    contenido_panel,
                );
            }
        }
    }
    // La barra de estado mide el relleno contra el área del panel enfocado,
    // no contra toda la vista.
    let area_enfocada = reparto
        .iter()
        .find(|(panel, _)| *panel == app.paneles.enfocado())
        .map(|(_, zona)| Rect {
            y: zona.y.saturating_add(CABECERA),
            height: zona.height.saturating_sub(CABECERA),
            ..*zona
        })
        .unwrap_or(contenido);
    marco.render_widget(
        Paragraph::new(barra_estado(
            app,
            indice,
            pestana,
            area_enfocada,
            app.paneles.en_espera(contenido),
        )),
        trozos[4],
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

/// Pinta una pestaña en el panel enfocado ocupando todo el contenido (modo
/// enlaces).
fn pintar_panel_unico(marco: &mut Frame, area: Rect, app: &App, pestana: &PestanaUI) {
    let zona = Rect {
        x: area.x,
        y: area.y.saturating_add(CABECERA),
        width: area.width,
        height: area.height.saturating_sub(CABECERA),
    };
    let panel = app.paneles.enfocado();
    let indice = app
        .paneles
        .indices(&app.pestanas)
        .get(panel)
        .copied()
        .flatten();
    pintar_cabecera(marco, area, app, panel, indice);
    pintar_pantalla(marco, zona, app, pestana);
}

/// La cabecera de un panel: `▸` si está enfocado, el glifo y el nombre de su
/// pestaña y `»` si recibe la escritura en varios.
fn pintar_cabecera(marco: &mut Frame, zona: Rect, app: &App, panel: usize, indice: Option<usize>) {
    if zona.height == 0 {
        return;
    }
    let fila = Rect {
        x: zona.x,
        y: zona.y,
        width: zona.width,
        height: 1,
    };
    let tema = &app.tema;
    let enfocado = panel == app.paneles.enfocado();
    let mut spans = Vec::new();
    let marca = if enfocado { tema.glifos.seleccion } else { " " };
    spans.push(Span::styled(
        marca.to_string(),
        Style::default().fg(tema.paleta.acento),
    ));
    if let Some(pestaña) = indice.and_then(|indice| app.pestanas.get(indice)) {
        spans.push(Span::styled(
            glifo_pestaña(enfocado, pestaña, tema),
            estilo_pestaña(enfocado, pestaña, tema),
        ));
        spans.push(Span::styled(
            format!(
                " {}",
                recortar_nombre(&pestaña.nombre, NOMBRE_MAX, tema.ascii)
            ),
            if enfocado {
                Style::default()
                    .fg(tema.paleta.texto)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(tema.paleta.texto)
            },
        ));
        if app.escritura_varios.incluye(pestaña.sesion_id) || pestaña.escritura_varios {
            spans.push(Span::styled(
                format!(" {}", tema.glifos.escribiendo),
                Style::default().fg(tema.paleta.critico),
            ));
        }
        if let Some(titulo) = &pestaña.titulo {
            let hueco = usize::from(zona.width).saturating_sub(
                spans
                    .iter()
                    .map(|span| span.content.chars().count())
                    .sum::<usize>()
                    + 1,
            );
            if hueco > 8 {
                spans.push(Span::styled(
                    format!(" {titulo}"),
                    Style::default().fg(tema.paleta.inactivo),
                ));
            }
        }
    } else {
        spans.push(Span::styled(
            format!(" {}", super::tecla(tema, "vacío", "vacio")),
            Style::default().fg(tema.paleta.inactivo),
        ));
    }
    // Relleno hasta el ancho del panel, con el separador vertical.
    let ancho: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    let relleno = usize::from(zona.width).saturating_sub(ancho);
    spans.push(Span::styled(
        tema.glifos.linea.repeat(relleno),
        Style::default().fg(tema.paleta.inactivo),
    ));
    marco.render_widget(Paragraph::new(Line::from(spans)), fila);
}

/// Pinta la pantalla de una pestaña en su panel (sin cabecera).
fn pintar_pantalla(marco: &mut Frame, area: Rect, app: &App, pestana: &PestanaUI) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if pestana.escritorio_id.is_some() {
        super::escritorios::tarjeta(marco, area, app, pestana);
        return;
    }
    // La salida sincronizada no pinta hasta cerrarse o 200 ms: se deja lo
    // anterior en el búfer.
    if app.pantallas.retener_pintado(pestana.sesion_id) {
        return;
    }
    if let Some(reconexion) = &pestana.reconexion {
        pintar_ultima_pantalla(marco, area, app, pestana);
        aviso_reconexion(marco, area, app, pestana, reconexion);
        return;
    }
    if pestana.estado == EstadoSesionRemota::Caida {
        pintar_ultima_pantalla(marco, area, app, pestana);
        aviso_caida(marco, area, app, pestana);
        return;
    }
    relleno(marco, area, app, pestana);
    if let Ok(parser) = pestana.pantalla.lock() {
        let widget = PseudoTerminal::new(parser.screen()).cursor(cursor(&app.tema));
        marco.render_widget(widget, area_remota(area, pestana));
    }
}

/// La última pantalla, en gris y sin cursor (no hay nadie al otro lado).
fn pintar_ultima_pantalla(marco: &mut Frame, area: Rect, app: &App, pestana: &PestanaUI) {
    if let Ok(parser) = pestana.pantalla.lock() {
        let widget = PseudoTerminal::new(parser.screen())
            .cursor(cursor(&app.tema).visibility(false))
            .style(Style::default().fg(app.tema.paleta.inactivo));
        marco.render_widget(widget, area_remota(area, pestana));
    }
}

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

fn hay_relleno(area: Rect, pestaña: &PestanaUI) -> bool {
    pestaña.estado == EstadoSesionRemota::Abierta
        && pestaña.cols_remoto > 0
        && pestaña.filas_remoto > 0
        && (area.width > pestaña.cols_remoto || area.height > pestaña.filas_remoto)
}

/// Relleno tenue para la zona que no cubre el remoto cuando otra ventana más
/// pequeña comparte la pestaña.
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

/// Línea de marco superior: `MAGI · SESIÓN` (con la disposición) a la
/// izquierda y el nombre de la pestaña a la derecha.
fn linea_titulo(app: &App, pestaña: &PestanaUI, ancho: u16, ampliada: bool) -> Line<'static> {
    let tema = &app.tema;
    let titulo_vista = if ampliada {
        "SESIÓN".to_string()
    } else {
        format!(
            "SESIÓN {} {}",
            tema.glifos.punto_medio,
            app.paneles.disposicion.etiqueta()
        )
    };
    let ancho = usize::from(ancho);
    let izquierda = format!(" MAGI {} {titulo_vista} ", tema.glifos.punto_medio);
    let fijo = izquierda.chars().count();
    let hueco = ancho.saturating_sub(fijo + 4);
    let nombre = pestaña.nombre.clone();
    let derecha = if hueco >= NOMBRE_MIN {
        format!(" {} ", recortar_nombre(&nombre, hueco, tema.ascii))
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

/// La línea roja fija de la escritura en varios.
fn linea_escritura(app: &App, ancho: u16) -> Line<'static> {
    let tema = &app.tema;
    let paneles = app.escritura_varios.paneles().len();
    let hosts = app.escritura_varios.sesiones().len();
    let texto = format!(
        " {} ESCRIBIENDO EN {paneles} PANELES · prefijo E para parar ",
        tema.glifos.escribiendo
    );
    let estilo = Style::default()
        .fg(tema.paleta.fondo)
        .bg(tema.paleta.critico)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::styled(
        disposicion::recortar(&texto, usize::from(ancho), tema.ascii),
        estilo,
    )];
    let ocupado: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    if ocupado < usize::from(ancho) {
        spans.push(Span::styled(
            format!(" {hosts} sesiones"),
            Style::default()
                .fg(tema.paleta.critico)
                .bg(tema.paleta.fondo),
        ));
    }
    Line::from(spans)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Forma {
    Completa,
    Compacta { nombre_max: usize },
}

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
    linea_pestañas(
        app,
        activa,
        activa,
        activa + 1,
        Forma::Compacta { nombre_max: 0 },
    )
}

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
        // Marca de aviso sin ver (campana/OSC 9/777).
        if pestaña.aviso.is_some() {
            spans.push(Span::styled(
                format!(" {}", tema.glifos.aviso),
                Style::default().fg(tema.paleta.acento),
            ));
        }
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
    if let Some(tipo) = pestaña.tipo {
        return tipo.glifo(tema.ascii).to_string();
    }
    let glifo = if pestaña.reconexion.is_some() {
        tema.glifos.reconectando
    } else {
        match pestaña.estado {
            EstadoSesionRemota::Caida | EstadoSesionRemota::Cerrada => tema.glifos.error,
            EstadoSesionRemota::Abierta => {
                if pestaña.actividad_no_vista && !activa {
                    tema.glifos.conectando
                } else {
                    tema.glifos.conectado
                }
            }
            EstadoSesionRemota::Abriendo => tema.glifos.conectado,
        }
    };
    glifo.to_string()
}

fn estilo_pestaña(activa: bool, pestaña: &PestanaUI, tema: &Tema) -> Style {
    let base = Style::default();
    if pestaña.reconexion.is_some() {
        return base.fg(tema.paleta.acento);
    }
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

/// Cuenta atrás hasta el próximo intento de reconexión.
fn cuenta_atras(reconexion: &InfoReconexion) -> u64 {
    let ahora = chrono::Utc::now().timestamp();
    u64::try_from(reconexion.proximo_en.saturating_sub(ahora).max(0)).unwrap_or(0)
}

/// Segundos desde la caída de la reconexión.
fn caida_hace(reconexion: &InfoReconexion) -> u64 {
    let ahora = chrono::Utc::now().timestamp();
    u64::try_from(ahora.saturating_sub(reconexion.desde).max(0)).unwrap_or(0)
}

fn barra_estado(
    app: &App,
    indice: usize,
    pestaña: &PestanaUI,
    area: Rect,
    en_espera: bool,
) -> Line<'static> {
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
        // La posición se ve siempre: es la única forma de saber dónde estás.
        Elemento::fijo(format!("{}/{}", indice + 1, app.pestanas.len()), 1, texto),
        Elemento::fijo(pestaña.host_nombre.clone(), 3, texto),
        Elemento::new(format!("conectado {tiempo}"), tiempo, 4, texto),
    ];
    // Escritura en varios: la barra lo dice con texto, no solo con color.
    if app.escritura_varios.activa() {
        elementos.push(Elemento::fijo(
            format!(
                "{} escribiendo en {} paneles · {} E parar",
                tema.glifos.escribiendo,
                app.escritura_varios.paneles().len(),
                prefijo_corto(&app.config.prefijo_escape)
            ),
            1,
            Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD),
        ));
    }
    // Disposición en espera (ventana pequeña).
    if en_espera {
        elementos.push(Elemento::new(
            "disposición en espera: ventana pequeña".to_string(),
            "ventana pequeña".to_string(),
            1,
            Style::default().fg(tema.paleta.acento),
        ));
    }
    // Reconexión automática: cuenta atrás y causa.
    if let Some(reconexion) = &pestaña.reconexion {
        elementos.push(Elemento::fijo(
            format!(
                "{} intento {} en {} s · {}",
                tema.glifos.reconectando,
                reconexion.intento,
                cuenta_atras(reconexion),
                reconexion.causa
            ),
            1,
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(aviso) = &pestaña.aviso {
        elementos.push(Elemento::fijo(
            format!("{} {}", tema.glifos.aviso, aviso.texto),
            2,
            Style::default().fg(tema.paleta.acento),
        ));
    }
    let identidad = if pestaña.identidad.is_empty() {
        pestaña.motivo.clone().unwrap_or_default()
    } else {
        pestaña.identidad.clone()
    };
    if !identidad.is_empty() {
        elementos.push(Elemento::fijo(identidad, 5, texto));
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
    if pestaña.cols_remoto > 0 {
        let mut tamano = format!(
            "{}{}{}",
            pestaña.cols_remoto, tema.glifos.por, pestaña.filas_remoto
        );
        let prioridad = if hay_relleno(area, pestaña) {
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
    let ventanas = if pestaña.ventanas == 1 {
        "1 ventana".to_string()
    } else {
        format!("{} ventanas", pestaña.ventanas)
    };
    elementos.push(Elemento::fijo(ventanas, 7, texto));
    if let Some(nombre) = &pestaña.tmux {
        elementos.push(Elemento::fijo(
            format!("tmux {nombre}"),
            6,
            Style::default().fg(tema.paleta.acento),
        ));
    }
    let prefijo = &app.config.prefijo_escape;
    elementos.push(Elemento::new(
        format!("{prefijo} ? ayuda"),
        format!("{} ?", prefijo_corto(prefijo)),
        1,
        tenue,
    ));
    componer(&cabeza, &elementos, area.width, tema)
}

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
    let panel = if tema.ascii {
        "<>^v panel"
    } else {
        "←↑→↓ panel"
    };
    let mut elementos: Vec<Elemento> = [
        ("1-9 n p pestañas", 1),
        (panel, 2),
        ("Espacio disposición", 2),
        ("z ampliar", 3),
        ("[ historial", 2),
        ("/ buscar", 3),
        ("o enlaces", 3),
        ("E escribir en varios", 3),
        ("l lista", 4),
        ("c conectar", 5),
        ("x cerrar", 4),
        ("r reconectar", 6),
        (", renombrar", 6),
        ("q volver", 2),
    ]
    .into_iter()
    .map(|(texto, prioridad)| Elemento::fijo(texto.to_string(), prioridad, estilo))
    .collect();
    elementos[0].completo.insert(0, ' ');
    elementos[0].compacto.insert(0, ' ');
    componer(&cabeza, &elementos, ancho, tema)
}

// ------------------------------------------------------------- historial

/// Pinta el modo historial: las filas de la captura y la búsqueda resaltada.
fn pintar_historial(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let Some(estado) = &app.historial else {
        return;
    };
    let tema = &app.tema;
    let alto = usize::from(area.height);
    disp.historial_alto = Some(alto);
    let mut lineas: Vec<Line> = Vec::new();
    if estado.captura.is_empty() {
        lineas.push(Line::from(Span::styled(
            if estado.esperando {
                " capturando el historial…"
            } else {
                " el historial está vacío"
            },
            Style::default().fg(tema.paleta.inactivo),
        )));
    } else {
        let primera = estado.primera();
        for fila in primera..(primera + alto).min(estado.captura.len()) {
            let texto = estado.captura[fila].texto();
            let (hay, actual) = estado.coincidencias_en(fila);
            let seleccionada = estado.seleccion.is_some_and(|seleccion| {
                let (inicio, fin) = if seleccion.inicio.0 <= seleccion.fin.0 {
                    (seleccion.inicio.0, seleccion.fin.0)
                } else {
                    (seleccion.fin.0, seleccion.inicio.0)
                };
                (inicio..=fin).contains(&fila)
            });
            let mut estilo = Style::default().fg(tema.paleta.texto);
            let mut marca = " ";
            if actual {
                marca = tema.glifos.seleccion;
                estilo = estilo.fg(tema.paleta.acento).add_modifier(Modifier::BOLD);
            } else if hay {
                estilo = estilo.fg(tema.paleta.acento);
            }
            if seleccionada {
                estilo = estilo.add_modifier(Modifier::REVERSED);
            }
            if fila == estado.cursor {
                marca = tema.glifos.seleccion;
            }
            let texto = disposicion::recortar(
                &format!("{marca}{texto}"),
                usize::from(area.width),
                tema.ascii,
            );
            lineas.push(Line::from(Span::styled(texto, estilo)));
        }
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

fn barra_historial(app: &App, ancho: u16) -> Line<'static> {
    let tema = &app.tema;
    let Some(estado) = &app.historial else {
        return Line::default();
    };
    if estado.editando {
        let Some(busqueda) = &estado.busqueda else {
            return Line::default();
        };
        let clase = match busqueda.clase {
            magi_nucleo::terminal::historial::ClaseBusqueda::Texto => "texto",
            magi_nucleo::terminal::historial::ClaseBusqueda::Expresion => "regex",
        };
        let mut spans = vec![
            Span::styled(
                format!(" /{}", busqueda.patron),
                Style::default().fg(tema.paleta.texto),
            ),
            Span::styled(
                format!(" · {clase} · "),
                Style::default().fg(tema.paleta.inactivo),
            ),
            Span::styled("Tab", Style::default().fg(tema.paleta.acento)),
            Span::styled(" cambiar · ", Style::default().fg(tema.paleta.texto)),
            Span::styled("↵", Style::default().fg(tema.paleta.acento)),
            Span::styled(" buscar · ", Style::default().fg(tema.paleta.texto)),
            Span::styled("Esc", Style::default().fg(tema.paleta.acento)),
            Span::styled(" cancelar", Style::default().fg(tema.paleta.texto)),
        ];
        if let Some(error) = &busqueda.error {
            spans.push(Span::styled(
                format!(" · {error}"),
                Style::default().fg(tema.paleta.critico),
            ));
        }
        return Line::from(spans).style(Style::default().bg(tema.paleta.fondo));
    }
    let cabeza = Elemento::fijo(
        format!(" {} ", tema.glifos.reconectando),
        1,
        Style::default().fg(tema.paleta.acento),
    );
    let mut elementos = vec![Elemento::fijo(
        format!("HISTORIAL {} líneas", estado.captura.len()),
        2,
        Style::default().fg(tema.paleta.texto),
    )];
    if let Some(busqueda) = &estado.busqueda {
        let total = busqueda.coincidencias.len();
        let actual = busqueda.actual.map_or(0, |indice| indice + 1);
        elementos.push(Elemento::fijo(
            format!(
                "/{} · {} · {actual} de {total}",
                busqueda.patron,
                match busqueda.clase {
                    magi_nucleo::terminal::historial::ClaseBusqueda::Texto => "texto",
                    magi_nucleo::terminal::historial::ClaseBusqueda::Expresion => "regex",
                }
            ),
            1,
            Style::default().fg(tema.paleta.acento),
        ));
        if let Some(error) = &busqueda.error {
            elementos.push(Elemento::fijo(
                error.clone(),
                1,
                Style::default().fg(tema.paleta.critico),
            ));
        }
    }
    elementos.push(Elemento::fijo(
        format!(
            "líneas {} {} {} de {}",
            estado.primera() + 1,
            tema.glifos.por,
            (estado.primera() + estado.alto.max(1)).min(estado.captura.len()),
            estado.captura.len()
        ),
        4,
        Style::default().fg(tema.paleta.texto),
    ));
    if estado.nueva_salida {
        elementos.push(Elemento::fijo(
            format!("{} salida nueva", tema.glifos.abajo),
            2,
            Style::default().fg(tema.paleta.acento),
        ));
    }
    let ayuda = [
        ("v selec. · y copiar · w guardar · o enlaces", 5),
        ("v · y · w · o · n N · Tab regex · q", 5),
        ("v y w q", 1),
    ];
    elementos.push(Elemento::fijo(
        ayuda[0].0.to_string(),
        5,
        Style::default().fg(tema.paleta.inactivo),
    ));
    componer(&cabeza, &elementos, ancho, tema)
}

// ----------------------------------------------------------------- enlaces

/// Pinta la lista de enlaces marcados sobre el panel enfocado.
fn pintar_enlaces(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    if app.enlaces.letras.is_empty() {
        return;
    }
    let lineas: Vec<Line> = app
        .enlaces
        .letras
        .iter()
        .map(|(letra, enlace)| {
            Line::from(vec![
                Span::styled(
                    format!("[{letra}] "),
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    disposicion::recortar(
                        &enlace.url,
                        usize::from(area.width.saturating_sub(6)),
                        tema.ascii,
                    ),
                    Style::default().fg(tema.paleta.texto),
                ),
            ])
        })
        .collect();
    let alto = u16::try_from(lineas.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let ancho = area.width.clamp(20, 66);
    let caja = disposicion::centrar_limitado(
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        },
        ancho,
        alto.min(area.height),
    );
    marco.render_widget(super::bloque("ENLACES", tema), caja);
    let interior = Rect {
        x: caja.x + 1,
        y: caja.y + 1,
        width: caja.width.saturating_sub(2),
        height: caja.height.saturating_sub(2),
    };
    marco.render_widget(Paragraph::new(lineas), interior);
}

fn barra_enlaces(app: &App, ancho: u16) -> Line<'static> {
    let tema = &app.tema;
    let cabeza = Elemento::fijo(
        format!(" ENLACES {} ", app.enlaces.letras.len()),
        1,
        Style::default().fg(tema.paleta.acento),
    );
    let elementos = vec![Elemento::fijo(
        "pulsa la letra · Esc salir".to_string(),
        2,
        Style::default().fg(tema.paleta.texto),
    )];
    componer(&cabeza, &elementos, ancho, tema)
}

// ----------------------------------------------------------------- avisos

fn aviso_reconexion(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    pestaña: &PestanaUI,
    reconexion: &InfoReconexion,
) {
    let tema = &app.tema;
    let prefijo = &app.config.prefijo_escape;
    let corto = prefijo_corto(prefijo);
    let acciones = if area.width > 50 {
        format!("{prefijo} r  reintentar ya        {prefijo} x  cerrar")
    } else {
        format!("{corto} r reintentar   {corto} x cerrar")
    };
    let tmux = pestaña
        .tmux
        .as_ref()
        .map(|nombre| format!("tmux {nombre}: lo que corría sigue en el host"))
        .unwrap_or_default();
    let critico = Style::default()
        .fg(tema.paleta.acento)
        .add_modifier(Modifier::BOLD);
    let texto = Style::default().fg(tema.paleta.texto);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let todas = [
        (String::new(), 1, critico),
        (
            format!(
                "{} Reconectando con {} · intento {} en {} s",
                tema.glifos.reconectando,
                pestaña.host_nombre,
                reconexion.intento,
                cuenta_atras(reconexion)
            ),
            1,
            critico,
        ),
        (
            format!(
                "caída hace {} s: {}",
                caida_hace(reconexion),
                reconexion.causa
            ),
            2,
            texto,
        ),
        (tmux, 3, texto),
        (String::new(), 1, texto),
        (acciones, 1, tenue),
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
    marco.render_widget(super::bloque("RECONECTANDO", tema), recta);
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
    fn la_cuenta_atras_no_es_negativa() {
        let reconexion = InfoReconexion {
            intento: 2,
            proximo_en: 0,
            desde: 0,
            causa: "conexión perdida".to_string(),
        };
        assert_eq!(cuenta_atras(&reconexion), 0);
        assert!(caida_hace(&reconexion) > 0);
    }
}
