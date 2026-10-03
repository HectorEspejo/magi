//! Vista Hosts (F2): inventario agrupado.
//!
//! Disposición adaptable (Fase 7), derivada del área en cada pintado: las
//! columnas van por prioridad (`disposicion::columnas_visibles`). El glifo de
//! estado y el nombre nunca se ocultan; la dirección se oculta por debajo de
//! 80 columnas de terminal y usuario·puerto (o las etiquetas, con `Tab`) por
//! debajo de 60.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Fila};
use crate::modelo::UltimoEstado;
use crate::tema::Tema;
use crate::ui::disposicion::{
    self, Columna, Disposicion, Lista, VentanaLista, ANCHO_COLUMNAS_MINIMAS,
};

/// Ancho de la columna del nombre, sin contar el separador.
const ANCHO_NOMBRE: u16 = 26;
/// Sangría de las filas de host: marca de selección y dos espacios más que
/// las de grupo.
const SANGRIA_HOST: u16 = 4;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let titulo = format!(" MAGI {} HOSTS ", tema.glifos.punto_medio);
    let recuento = format!(" {} hosts ", app.hosts.len());
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            titulo.clone(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    // El recuento, arriba a la derecha, solo si cabe junto al título.
    if titulo_derecho_cabe(area.width, &titulo, &recuento) {
        bloque = bloque.title_top(titulo_derecho(recuento, tema));
    }
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 || interior.width == 0 {
        return;
    }
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(u16::from(app.filtro_activo)),
            Constraint::Min(1),
        ])
        .split(interior);
    if app.filtro_activo {
        marco.render_widget(Paragraph::new(linea_filtro(app)), trozos[0]);
    }
    let listado = trozos[1];
    if listado.height == 0 {
        return;
    }
    if app.filas.is_empty() {
        let mensaje = if app.filtro_activo && !app.filtro.trim().is_empty() {
            "sin resultados para el filtro"
        } else {
            "no hay hosts; pulsa n para crear uno o I para importar"
        };
        disp.registrar(
            Lista::Hosts,
            VentanaLista {
                inicio: 0,
                filas: usize::from(listado.height),
                total: 0,
            },
        );
        let parrafo = Paragraph::new(Line::from(Span::styled(
            mensaje,
            Style::default().fg(tema.paleta.inactivo),
        )))
        .wrap(Wrap { trim: true });
        // Sangría de dos columnas también en las líneas partidas.
        let zona = Rect {
            x: listado.x + 2,
            width: listado.width.saturating_sub(2),
            ..listado
        };
        marco.render_widget(parrafo, zona);
        return;
    }
    let tabla = TablaHosts::nueva(app, listado.width, area.width);
    let altura = usize::from(listado.height);
    let total = app.filas.len();
    let inicio = disposicion::ventana(app.desplazamiento, app.seleccion, altura, total);
    disp.registrar(
        Lista::Hosts,
        VentanaLista {
            inicio,
            filas: altura,
            total,
        },
    );
    let visibles: Vec<Line> = (inicio..total.min(inicio + altura))
        .map(|indice| construir_linea(app, &tabla, indice, listado.width))
        .collect();
    marco.render_widget(Paragraph::new(visibles), listado);
}

/// Línea del filtro que se está escribiendo.
pub(crate) fn linea_filtro(app: &App) -> Line<'static> {
    let tema = &app.tema;
    Line::from(vec![
        Span::styled("/ ", Style::default().fg(tema.paleta.acento)),
        Span::styled(
            app.filtro.clone(),
            Style::default()
                .fg(tema.paleta.texto)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            super::tecla(tema, "\u{2503}", "|"),
            Style::default().fg(tema.paleta.acento),
        ),
    ])
}

/// ¿Cabe `derecho` en el borde superior junto a `titulo`, dejando al menos
/// un tramo de línea a cada lado?
pub(crate) fn titulo_derecho_cabe(ancho: u16, titulo: &str, derecho: &str) -> bool {
    // Esquinas, tramo entre los dos títulos y tramo final.
    titulo.chars().count() + derecho.chars().count() + 4 <= usize::from(ancho)
}

/// Título a la derecha del borde superior, con un tramo de línea detrás.
pub(crate) fn titulo_derecho(texto: String, tema: &Tema) -> Line<'static> {
    Line::from(vec![
        Span::styled(texto, Style::default().fg(tema.paleta.inactivo)),
        Span::raw(tema.glifos.linea),
    ])
    .right_aligned()
}

/// Columnas de la tabla de hosts para el ancho del listado.
struct TablaHosts {
    /// Glifo, nombre, dirección, usuario, puerto, etiquetas y salto; las
    /// ocultas valen 0.
    anchos: [u16; 7],
    /// Ancho que ocupa una fila de host entera (sangría incluida).
    usado: u16,
}

const GLIFO: usize = 0;
const NOMBRE: usize = 1;
const DIRECCION: usize = 2;
const USUARIO: usize = 3;
const PUERTO: usize = 4;
const ETIQUETAS: usize = 5;

impl TablaHosts {
    fn nueva(app: &App, ancho_listado: u16, ancho_vista: u16) -> Self {
        let tabla = ancho_listado.saturating_sub(SANGRIA_HOST);
        // Los umbrales son columnas de terminal: se trasladan al ancho de la
        // tabla descontando bordes y sangría.
        let margen = ancho_vista.saturating_sub(tabla);
        let desde_direccion = ANCHO_COLUMNAS_MINIMAS.saturating_sub(margen);
        let desde_usuario = disposicion::ANCHO_SIN_USUARIO.saturating_sub(margen);
        let hosts = &app.hosts;
        let ancho_glifo = hosts
            .iter()
            .map(|host| {
                glifo_y_color(app, host.id, host.ultimo_estado)
                    .0
                    .chars()
                    .count()
            })
            .max()
            .unwrap_or(1)
            .max(1) as u16;
        let ancho_etiquetas = hosts
            .iter()
            .map(|host| host.etiquetas.join(" ").chars().count())
            .max()
            .unwrap_or(0)
            .clamp(4, 30) as u16;
        let ancho_salto = hosts
            .iter()
            .filter_map(|host| host.salto_nombre.as_ref())
            .map(|salto| salto.chars().count() + 2)
            .max()
            .unwrap_or(0)
            .min(24) as u16;
        let etiquetas = app.columna_etiquetas;
        // Una columna que no aplica se declara con umbral imposible.
        let nunca = u16::MAX;
        let columnas = [
            Columna::fija(ancho_glifo, 1),
            Columna::fija(ANCHO_NOMBRE, 1),
            Columna::fija(20, 2).desde(desde_direccion),
            Columna::fija(10, 3).desde(if etiquetas { nunca } else { desde_usuario }),
            Columna::fija(5, 3).desde(if etiquetas { nunca } else { desde_usuario }),
            Columna::fija(ancho_etiquetas, 3).desde(if etiquetas { desde_usuario } else { nunca }),
            Columna::fija(ancho_salto, 4).desde(if ancho_salto == 0 { nunca } else { 0 }),
        ];
        let anchos = anchos_tabla(tabla, &columnas, NOMBRE);
        let usado = SANGRIA_HOST + ancho_usado(&anchos);
        Self {
            anchos: anchos.try_into().unwrap_or([0; 7]),
            usado,
        }
    }
}

/// Anchos de las columnas visibles de una tabla de `ancho` (separadas por un
/// espacio): por prioridad y, si ni las imprescindibles caben, encoge la
/// columna `encoge` hasta que quepan. Las ocultas valen 0.
pub(crate) fn anchos_tabla(ancho: u16, columnas: &[Columna], encoge: usize) -> Vec<u16> {
    let visibles = disposicion::columnas_visibles(ancho, columnas, 1);
    let mut anchos = disposicion::repartir(ancho, columnas, &visibles, 1);
    let usado = ancho_usado(&anchos);
    if usado > ancho {
        if let Some(columna) = anchos.get_mut(encoge) {
            *columna = columna.saturating_sub(usado - ancho).max(1);
        }
    }
    anchos
}

/// Ancho que ocupan las columnas visibles con sus separadores.
pub(crate) fn ancho_usado(anchos: &[u16]) -> u16 {
    let visibles: Vec<u16> = anchos.iter().copied().filter(|ancho| *ancho > 0).collect();
    visibles
        .iter()
        .fold(0u16, |suma, ancho| suma.saturating_add(*ancho))
        .saturating_add((visibles.len() as u16).saturating_sub(1))
}

/// Celdas de una fila en las columnas visibles: cada texto recortado y
/// rellenado a su ancho, con un espacio entre columnas.
pub(crate) fn celdas(
    textos: Vec<(String, Style)>,
    anchos: &[u16],
    ascii: bool,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for ((texto, estilo), ancho) in textos.into_iter().zip(anchos) {
        if *ancho == 0 {
            continue;
        }
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            disposicion::columna(&texto, usize::from(*ancho), ascii),
            estilo,
        ));
    }
    spans
}

fn construir_linea(app: &App, tabla: &TablaHosts, indice: usize, ancho: u16) -> Line<'static> {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let seleccionada = indice == app.seleccion;
    let marca = if seleccionada {
        tema.glifos.seleccion
    } else {
        " "
    };
    let linea = match &app.filas[indice] {
        Fila::Grupo {
            grupo_id: _,
            nombre,
            plegado,
            total,
            filtrado,
        } => {
            let glifo = if *plegado && !*filtrado {
                tema.glifos.plegado
            } else {
                tema.glifos.desplegado
            };
            // El recuento se alinea con el final de las columnas de host.
            let recuento = format!("({total})");
            let cabeza = 4usize;
            let hueco = usize::from(ancho)
                .saturating_sub(cabeza + recuento.chars().count() + 1)
                .max(1);
            let nombre = disposicion::recortar(nombre, hueco, ascii);
            let fin = usize::from(tabla.usado)
                .max(cabeza + nombre.chars().count() + 1 + recuento.chars().count())
                .min(usize::from(ancho));
            let relleno =
                fin.saturating_sub(cabeza + nombre.chars().count() + recuento.chars().count());
            Line::from(vec![
                Span::raw(format!("{marca} ")),
                Span::styled(format!("{glifo} "), Style::default().fg(tema.paleta.acento)),
                Span::styled(
                    nombre,
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" ".repeat(relleno)),
                Span::styled(recuento, Style::default().fg(tema.paleta.inactivo)),
            ])
        }
        Fila::Host { indice } => {
            let host = &app.hosts[*indice];
            let (glifo, color) = glifo_y_color(app, host.id, host.ultimo_estado);
            let texto = Style::default().fg(tema.paleta.texto);
            let inactivo = Style::default().fg(tema.paleta.inactivo);
            let salto = host
                .salto_nombre
                .as_ref()
                .map(|salto| format!("{} {salto}", tema.glifos.salto))
                .unwrap_or_default();
            let mut spans = vec![Span::raw(format!("{marca}   "))];
            for (columna, ancho) in tabla.anchos.iter().enumerate() {
                if *ancho == 0 {
                    continue;
                }
                if spans.len() > 1 {
                    spans.push(Span::raw(" "));
                }
                // El nombre lleva pegado el glifo de túneles: va aparte.
                if columna == NOMBRE {
                    spans.extend(columna_nombre(app, host.id, &host.nombre, *ancho));
                    continue;
                }
                let (texto, estilo) = match columna {
                    GLIFO => (
                        glifo.clone(),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    DIRECCION => (host.direccion.clone(), inactivo),
                    USUARIO => (
                        host.usuario.clone().unwrap_or_else(|| "-".to_string()),
                        texto,
                    ),
                    PUERTO => (host.puerto.to_string(), texto),
                    ETIQUETAS => (
                        host.etiquetas.join(" "),
                        Style::default().fg(tema.paleta.correcto),
                    ),
                    _ => (salto.clone(), inactivo),
                };
                spans.push(Span::styled(
                    disposicion::columna(&texto, usize::from(*ancho), ascii),
                    estilo,
                ));
            }
            let destello = app.destello.as_ref().is_some_and(|(id, _)| *id == host.id);
            let linea = Line::from(spans);
            if destello && !seleccionada {
                return rellenar(linea, ancho).style(
                    Style::default()
                        .fg(tema.paleta.fondo)
                        .bg(tema.paleta.acento),
                );
            }
            linea
        }
    };
    estilizar(rellenar(linea, ancho), seleccionada, app)
}

/// Rellena la línea con espacios hasta `ancho` para que el resaltado cubra la
/// fila entera.
fn rellenar(mut linea: Line<'static>, ancho: u16) -> Line<'static> {
    let largo = linea.width();
    if largo < usize::from(ancho) {
        linea
            .spans
            .push(Span::raw(" ".repeat(usize::from(ancho) - largo)));
    }
    linea
}

fn estilizar(linea: Line<'static>, seleccionada: bool, app: &App) -> Line<'static> {
    if seleccionada {
        linea.style(
            Style::default()
                .bg(app.tema.paleta.acento)
                .fg(app.tema.paleta.fondo)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        linea
    }
}

/// Columna del nombre: siempre el mismo ancho. Con túneles activos el glifo
/// `⇅` va pegado al nombre y el nombre se acorta lo mismo, de modo que las
/// columnas de la derecha no se desplazan.
fn columna_nombre(app: &App, host_id: i64, nombre: &str, ancho: u16) -> Vec<Span<'static>> {
    let tema = &app.tema;
    let estilo = Style::default().fg(tema.paleta.texto);
    let ancho = usize::from(ancho);
    if tuneles_activos_de_host(app, host_id) == 0 || ancho < 4 {
        return vec![Span::styled(
            disposicion::columna(nombre, ancho, tema.ascii),
            estilo,
        )];
    }
    vec![
        Span::styled(disposicion::columna(nombre, ancho - 2, tema.ascii), estilo),
        Span::styled(
            format!("{} ", tema.glifos.tuneles),
            Style::default().fg(tema.paleta.acento),
        ),
    ]
}

/// Cuántas sesiones vivas hay al host (pestañas en el servidor).
pub fn sesiones_del_host(app: &App, host_id: i64) -> usize {
    app.pestanas
        .iter()
        .filter(|pestaña| pestaña.host_id == host_id && pestaña.viva())
        .count()
}

/// Cuántos túneles del host están en marcha ahora mismo (lo que difunde el
/// servidor: activando, activo o parando).
pub fn tuneles_activos_de_host(app: &App, host_id: i64) -> usize {
    app.tuneles_activos
        .values()
        .filter(|info| info.host_id == host_id && info.estado.en_marcha())
        .count()
}

/// Glifo de estado del host con contador de sesiones: `●`, `●N` (más de una
/// sesión), `◐` (conectando), `✕` (último error) o `○`.
pub fn glifo_y_color(
    app: &App,
    host_id: i64,
    estado: Option<UltimoEstado>,
) -> (String, ratatui::style::Color) {
    let tema = &app.tema;
    let sesiones = sesiones_del_host(app, host_id);
    if sesiones >= 2 {
        return (
            format!("{}{sesiones}", tema.glifos.conectado),
            tema.paleta.correcto,
        );
    }
    if sesiones == 1 {
        return (tema.glifos.conectado.to_string(), tema.paleta.correcto);
    }
    if app.aperturas_pendientes.contains(&host_id) {
        return (tema.glifos.conectando.to_string(), tema.paleta.acento);
    }
    match estado {
        Some(UltimoEstado::Error) => (tema.glifos.error.to_string(), tema.paleta.critico),
        _ => (tema.glifos.desconectado.to_string(), tema.paleta.inactivo),
    }
}
