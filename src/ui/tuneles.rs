//! Vista Túneles (F6): los reenvíos definidos en `TUNELES` con el estado en
//! vivo que difunde el servidor, y el resumen del seleccionado al pie.
//!
//! Una fila por túnel definido: el estado sale de `App::tuneles_activos` y, si
//! el servidor no lo tiene en memoria, la fila está inactiva.
//!
//! Disposición adaptable (Fase 7): columnas por prioridad (el glifo de estado
//! y «escucha → destino» no se ocultan nunca), host abreviado en modo
//! estrecho y detalle inferior plegado con la vista baja (`↵` lo abre en
//! diálogo). Al final del módulo están las piezas que comparten las cuatro
//! vistas de tabla con detalle inferior: Túneles, Registro, Identidades y
//! Snippets.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::archivos::tamano_legible;
use crate::modelo::{TipoTunel, Tunel};
use crate::protocolo::{EstadoTunelRemoto, InfoTunel, OrigenTunel};
use crate::tema::Tema;
use crate::ui::bloque;
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};

/// Filas del panel de resumen inferior, bordes incluidos: el túnel, su tramo
/// con la actividad, el tráfico y, si está caído, el error.
pub const ALTO_DETALLE: u16 = 6;

/// Ancho de la columna del host en modo estrecho: el nombre va abreviado
/// (`hetzner-01` → `hetz-01`).
pub const ANCHO_HOST_ABREVIADO: u16 = 7;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let punto = tema.glifos.punto_medio;
    let filas = app.tuneles_visibles();
    let activos = filas
        .iter()
        .filter(|tunel| app.estado_de(tunel.id).en_marcha())
        .count();
    let titulo = format!(
        "TÚNELES {punto} {} {punto} {} activos",
        filas.len(),
        activos
    );
    let marco_bloque = bloque(&titulo, tema);
    let interior = marco_bloque.inner(area);

    if filas.is_empty() {
        let mensaje = if app.filtro_tuneles.trim().is_empty() {
            "Sin túneles: n para crear uno, o importa los reenvíos de tus hosts"
        } else {
            "sin resultados para el filtro"
        };
        marco.render_widget(marco_bloque, area);
        // Centrado, con una línea de aire por encima.
        let centrado = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(interior);
        let aviso = vec![
            Line::from(""),
            Line::from(Span::styled(
                disposicion::recortar(mensaje, usize::from(interior.width), ascii),
                Style::default().fg(tema.paleta.inactivo),
            )),
        ];
        marco.render_widget(
            Paragraph::new(aviso).alignment(Alignment::Center),
            centrado[1],
        );
        return;
    }

    let (zona, detalle) = repartir_vista(interior, ALTO_DETALLE, disp);
    let seleccionado = filas.get(app.seleccion_tunel).copied();
    let mut pie = format!(" {} ", filas.len());
    if detalle.is_none() && seleccionado.is_some() {
        pie.push_str(&format!(
            "{punto} {} ",
            texto_plegado(tema, tema.glifos.intro)
        ));
    }
    let marco_bloque =
        marco_bloque.title_bottom(Span::styled(pie, Style::default().fg(tema.paleta.inactivo)));
    marco.render_widget(marco_bloque, area);

    dibujar_tabla(marco, zona, app, &filas, disp);
    if let (Some(detalle), Some(tunel)) = (detalle, seleccionado) {
        dibujar_detalle(marco, detalle, app, tunel);
    }
}

/// Columnas de la tabla: glifo de estado, tipo, «escucha → destino», host y
/// la marca de automático. El glifo y el tramo son el identificador y no se
/// ocultan; el tipo es lo primero que cae.
fn columnas(ancho_host: u16, minimo_tramo: u16) -> [Columna; 5] {
    [
        Columna::fija(1, 1),
        Columna::fija(8, 4),
        Columna::flexible(minimo_tramo, 1),
        Columna::fija(ancho_host, 2),
        Columna::fija(1, 3),
    ]
}

fn dibujar_tabla(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    filas: &[&Tunel],
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    if area.height == 0 {
        return;
    }
    let tramos: Vec<String> = filas
        .iter()
        .map(|tunel| disposicion::adaptar(&tramo(tunel, app.tuneles_activos.get(&tunel.id)), ascii))
        .collect();
    let largo_host = filas
        .iter()
        .map(|tunel| tunel.host_nombre.chars().count())
        .max()
        .unwrap_or(4) as u16;
    // El modo es el de la vista (la tabla de puntos de corte sobre el ancho de
    // la terminal), no el del interior del marco: a 100 columnas la vista es
    // Normal y el host va entero.
    let ancho_host = if disp.estrecho() {
        ANCHO_HOST_ABREVIADO
    } else {
        largo_host.clamp(6, 18)
    };
    let largo_tramo = tramos
        .iter()
        .map(|tramo| tramo.chars().count())
        .max()
        .unwrap_or(16) as u16;
    // El tramo pide como mínimo su largo (hasta 32): antes de recortarlo cae
    // el tipo.
    let columnas = columnas(ancho_host, largo_tramo.clamp(16, 32));
    let naturales = [1, 8, largo_tramo.max(16), ancho_host, 1];
    let tabla = Tabla::nueva(
        area.width.saturating_sub(ANCHO_PREFIJO),
        &columnas,
        &naturales,
    );

    let cabecera = tabla.linea(
        prefijo(false, tema),
        vec![
            (String::new(), Style::default()),
            ("TIPO".to_string(), Style::default()),
            ("ESCUCHA → DESTINO".to_string(), Style::default()),
            ("HOST".to_string(), Style::default()),
            ("a".to_string(), Style::default()),
        ],
        Style::default()
            .fg(tema.paleta.inactivo)
            .add_modifier(Modifier::BOLD),
        ascii,
    );
    let mut lineas = vec![cabecera];

    let altura = usize::from(area.height.saturating_sub(1));
    let inicio = disposicion::ventana(
        app.desplazamiento_tuneles,
        app.seleccion_tunel,
        altura,
        filas.len(),
    );
    disp.registrar(
        Lista::Tuneles,
        VentanaLista {
            inicio,
            filas: altura,
            total: filas.len(),
        },
    );
    for (posicion, tunel) in filas.iter().enumerate().skip(inicio).take(altura) {
        let info = app.tuneles_activos.get(&tunel.id);
        let seleccionada = posicion == app.seleccion_tunel;
        lineas.push(linea_de(
            tunel,
            info,
            &tramos[posicion],
            app,
            seleccionada,
            &tabla,
        ));
    }
    marco.render_widget(
        Paragraph::new(lineas).style(Style::default().fg(tema.paleta.texto)),
        area,
    );
}

fn linea_de(
    tunel: &Tunel,
    info: Option<&InfoTunel>,
    tramo: &str,
    app: &App,
    seleccionada: bool,
    tabla: &Tabla,
) -> Line<'static> {
    let tema = &app.tema;
    let estado = info
        .map(|info| info.estado)
        .unwrap_or(EstadoTunelRemoto::Inactivo);
    let estilo = estilo_fila(tema, seleccionada);
    // Sobre la fila seleccionada el color del estado es el de la selección.
    let color_estado = if seleccionada {
        estilo
    } else {
        Style::default().fg(color_de(estado, tema))
    };
    let ancho_host = usize::from(tabla.ancho(3));
    tabla.linea(
        prefijo(seleccionada, tema),
        vec![
            (estado.glifo(tema.ascii).to_string(), color_estado),
            (tunel.tipo.etiqueta().to_string(), estilo),
            (tramo.to_string(), estilo),
            (
                abreviar_host(&tunel.host_nombre, ancho_host, tema.ascii),
                estilo,
            ),
            (if tunel.automatico { "a" } else { "" }.to_string(), estilo),
        ],
        estilo,
        tema.ascii,
    )
}

fn color_de(estado: EstadoTunelRemoto, tema: &Tema) -> Color {
    match estado {
        EstadoTunelRemoto::Activo => tema.paleta.correcto,
        EstadoTunelRemoto::Activando | EstadoTunelRemoto::Parando => tema.paleta.acento,
        EstadoTunelRemoto::Caido => tema.paleta.critico,
        EstadoTunelRemoto::Inactivo => tema.paleta.inactivo,
    }
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, tunel: &Tunel) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let interior = marco_detalle(marco, area, tema);
    if interior.height == 0 {
        return;
    }
    let ancho = usize::from(interior.width.saturating_sub(2));
    let info = app.tuneles_activos.get(&tunel.id);
    let estado = info
        .map(|info| info.estado)
        .unwrap_or(EstadoTunelRemoto::Inactivo);
    let linea = |texto: String, color: Color| {
        Line::from(Span::styled(
            format!(
                "  {}",
                disposicion::recortar(&disposicion::adaptar(&texto, ascii), ancho, ascii)
            ),
            Style::default().fg(color),
        ))
    };

    let mut lineas = vec![linea(
        format!(
            "{} · {} · {} · {}",
            tunel.host_nombre,
            tunel.nombre,
            tunel.tipo.etiqueta(),
            if tunel.automatico {
                "automático"
            } else {
                "manual"
            },
        ),
        tema.paleta.texto,
    )];
    lineas.push(linea(
        format!(
            "{} · {}",
            tramo(tunel, info),
            resumen_actividad(info, estado)
        ),
        tema.paleta.inactivo,
    ));
    if let Some(info) = info {
        lineas.push(linea(
            format!(
                "tráfico \u{2193} {} \u{2191} {}",
                tamano_legible(info.bytes_bajados),
                tamano_legible(info.bytes_subidos)
            ),
            tema.paleta.inactivo,
        ));
    }
    if estado == EstadoTunelRemoto::Caido {
        let error = info
            .and_then(|info| info.ultimo_error.as_deref())
            .unwrap_or("el túnel se ha caído");
        lineas.push(linea(error.to_string(), tema.paleta.critico));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// «escucha → destino», o la escucha con la marca de SOCKS5 en dinámico.
pub fn tramo(tunel: &Tunel, info: Option<&InfoTunel>) -> String {
    let escucha = info
        .map(|info| info.escucha_mostrada().to_string())
        .unwrap_or_else(|| tunel.escucha.clone());
    match tunel.tipo {
        TipoTunel::Dinamico => format!("{escucha}  (socks5)"),
        _ => format!(
            "{escucha} → {}",
            tunel.destino.as_deref().unwrap_or("\u{2014}")
        ),
    }
}

/// «activo desde 15:24:03 · 18 m · 1 conexión (3 en total)».
fn resumen_actividad(info: Option<&InfoTunel>, estado: EstadoTunelRemoto) -> String {
    let Some(info) = info else {
        return estado.texto().to_string();
    };
    let mut texto = match info.desde {
        Some(desde) => format!("{} desde {}", estado.texto(), hora_de(desde)),
        None => estado.texto().to_string(),
    };
    if let Some(desde) = info.desde {
        let segundos = (chrono::Utc::now().timestamp() - desde).max(0) as u64;
        texto.push_str(&format!(
            " · {}",
            crate::ui::sesion::formatear_duracion(segundos)
        ));
    }
    texto.push_str(&format!(
        " · {} conexión(es) ({} en total)",
        info.conexiones, info.aceptadas
    ));
    texto
}

/// Hora local `HH:MM:SS` de una época en segundos.
pub fn hora_de(epoca: i64) -> String {
    use chrono::TimeZone as _;
    match chrono::Local.timestamp_opt(epoca, 0).single() {
        Some(fecha) => fecha.format("%H:%M:%S").to_string(),
        None => "\u{2014}".to_string(),
    }
}

/// Texto del origen de un túnel: automático o manual con su ventana.
pub fn texto_origen(info: &InfoTunel) -> String {
    match info.origen {
        Some(OrigenTunel::Automatico) => "automático".to_string(),
        Some(OrigenTunel::Manual) => match info.solicitante {
            Some(ventana) => format!("manual (ventana {ventana})"),
            None => "manual".to_string(),
        },
        None => "\u{2014}".to_string(),
    }
}

/// Nombre de host abreviado a `ancho` (mapa §6 de la Fase 7): se conserva el
/// último tramo si es corto (la numeración: `01`, `m1`) y se acortan los
/// demás empezando por el más largo, así `hetzner-01` queda `hetz-01`. Sin
/// guiones, o si ni así cabe, se recorta con la marca de corte.
pub fn abreviar_host(nombre: &str, ancho: usize, ascii: bool) -> String {
    if nombre.chars().count() <= ancho {
        return nombre.to_string();
    }
    let mut tramos: Vec<Vec<char>> = nombre.split('-').map(|t| t.chars().collect()).collect();
    if tramos.len() < 2 || tramos.iter().any(Vec::is_empty) {
        return disposicion::recortar(nombre, ancho, ascii);
    }
    let ultimo = tramos.len() - 1;
    let fijo_ultimo = tramos[ultimo].len() <= 3;
    let largo = |tramos: &[Vec<char>]| -> usize {
        tramos.iter().map(Vec::len).sum::<usize>() + tramos.len() - 1
    };
    while largo(&tramos) > ancho {
        // El tramo más largo que se pueda acortar (a igualdad, el de más a la
        // derecha: el principio del nombre es lo que más lo identifica).
        let Some(indice) = tramos
            .iter()
            .enumerate()
            .filter(|(indice, tramo)| tramo.len() > 1 && !(fijo_ultimo && *indice == ultimo))
            .max_by_key(|(indice, tramo)| (tramo.len(), *indice))
            .map(|(indice, _)| indice)
        else {
            return disposicion::recortar(nombre, ancho, ascii);
        };
        tramos[indice].pop();
    }
    tramos
        .iter()
        .map(|tramo| tramo.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

// ---------------------------------------------------------------- comunes
//
// Piezas que comparten las cuatro vistas de tabla con detalle inferior
// (Túneles, Registro, Identidades y Snippets). Todo se deriva del área en
// cada pintado.

/// Ancho del prefijo de cada fila: la marca de selección y un espacio.
pub const ANCHO_PREFIJO: u16 = 2;

/// Separación entre columnas.
pub const SEPARACION: u16 = 2;

/// Filas que la tabla necesita como poco para que el detalle se pinte abierto.
const MINIMO_TABLA: u16 = 3;

/// Reparte el interior de una vista entre la tabla (arriba) y el detalle
/// inferior de `alto` filas. Con la vista baja (alto < 20) el detalle se
/// pliega: no se pinta y se abre en diálogo. En ventanas muy grandes
/// (≥ 200×60) el contenido no crece sin límite: tabla y detalle se quedan con
/// el ancho máximo de un detalle, centrados.
pub fn repartir_vista(interior: Rect, alto: u16, disp: &Disposicion) -> (Rect, Option<Rect>) {
    let contenido = if disposicion::es_grande(disp.area) {
        disposicion::limitar_ancho(interior, disposicion::ANCHO_MAX_DETALLE)
    } else {
        interior
    };
    if disp.bajo || contenido.height < alto + MINIMO_TABLA {
        return (contenido, None);
    }
    let tabla = Rect {
        height: contenido.height - alto,
        ..contenido
    };
    let detalle = Rect {
        y: contenido.y + contenido.height - alto,
        height: alto,
        ..contenido
    };
    (tabla, Some(detalle))
}

/// «↵ detalle» (o `enter detalle`, o `i detalle`): rótulo del borde inferior
/// cuando el detalle está plegado.
pub fn texto_plegado(tema: &Tema, tecla: &str) -> String {
    disposicion::adaptar(&format!("{tecla} detalle"), tema.ascii)
}

/// Marco del detalle inferior; devuelve su interior.
pub fn marco_detalle(marco: &mut Frame, area: Rect, tema: &Tema) -> Rect {
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.inactivo));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    interior
}

/// Estilo de una fila: el de la selección si lo está.
pub fn estilo_fila(tema: &Tema, seleccionada: bool) -> Style {
    if seleccionada {
        Style::default()
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.texto)
    }
}

/// Prefijo de la fila: la marca de selección (`▸`, `>` en ASCII) o aire. Así
/// la selección se ve también sin color.
pub fn prefijo(seleccionada: bool, tema: &Tema) -> Span<'static> {
    if seleccionada {
        Span::styled(
            format!("{} ", tema.glifos.seleccion),
            estilo_fila(tema, true),
        )
    } else {
        Span::raw("  ")
    }
}

/// Columnas visibles y anchos de una tabla para un pintado.
pub struct Tabla {
    pub visibles: Vec<bool>,
    pub anchos: Vec<u16>,
}

impl Tabla {
    /// Reparte `disponible` columnas entre `columnas`. Si caben todas a su
    /// ancho natural (`naturales`, uno por columna), cada una se queda con el
    /// suyo y la tabla no crece más: en ventanas anchas las columnas no se
    /// separan sin límite. Si no, prioridades (§7.3) sobre los mínimos y el
    /// sobrante para las flexibles.
    pub fn nueva(disponible: u16, columnas: &[Columna], naturales: &[u16]) -> Self {
        let anchos: Vec<u16> = columnas
            .iter()
            .enumerate()
            .map(|(indice, columna)| {
                naturales
                    .get(indice)
                    .copied()
                    .unwrap_or(0)
                    .max(columna.ancho)
            })
            .collect();
        let natural = u32::from(anchos.iter().sum::<u16>())
            + u32::from(SEPARACION) * (columnas.len() as u32).saturating_sub(1);
        if natural <= u32::from(disponible) {
            return Self {
                visibles: vec![true; columnas.len()],
                anchos,
            };
        }
        let visibles = disposicion::columnas_visibles(disponible, columnas, SEPARACION);
        let anchos = disposicion::repartir(disponible, columnas, &visibles, SEPARACION);
        Self { visibles, anchos }
    }

    /// Ancho de la columna `indice` (0 si está oculta).
    pub fn ancho(&self, indice: usize) -> u16 {
        self.anchos.get(indice).copied().unwrap_or(0)
    }

    pub fn visible(&self, indice: usize) -> bool {
        self.visibles.get(indice).copied().unwrap_or(false)
    }

    /// Una fila: el prefijo y las celdas visibles recortadas y rellenadas a
    /// su ancho. `base` es el estilo de los separadores.
    pub fn linea(
        &self,
        prefijo: Span<'static>,
        celdas: Vec<(String, Style)>,
        base: Style,
        ascii: bool,
    ) -> Line<'static> {
        let mut spans = vec![prefijo];
        let mut primera = true;
        for (indice, (texto, estilo)) in celdas.into_iter().enumerate() {
            if !self.visible(indice) {
                continue;
            }
            if !primera {
                spans.push(Span::styled(" ".repeat(usize::from(SEPARACION)), base));
            }
            primera = false;
            spans.push(Span::styled(
                disposicion::columna(
                    &disposicion::adaptar(&texto, ascii),
                    usize::from(self.ancho(indice)),
                    ascii,
                ),
                estilo,
            ));
        }
        Line::from(spans)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_host_se_abrevia_conservando_la_numeracion() {
        assert_eq!(abreviar_host("hetzner-01", 7, false), "hetz-01");
        assert_eq!(abreviar_host("hetzner-02", 7, false), "hetz-02");
        assert_eq!(abreviar_host("mac-mini-m1", 7, false), "ma-m-m1");
        assert_eq!(abreviar_host("vps-openclaw", 7, false), "vps-ope");
        assert_eq!(abreviar_host("backup-nas", 7, false), "bac-nas");
        assert_eq!(abreviar_host("db", 7, false), "db");
        // Sin guiones se recorta con la marca de corte.
        assert_eq!(abreviar_host("servidorprincipal", 7, false), "servid…");
        assert_eq!(abreviar_host("servidorprincipal", 7, true), "servid~");
        for nombre in ["hetzner-01", "mac-mini-m1", "a-b-c-d-e-f-g-h", "x--y", "-z"] {
            for ancho in 1..12 {
                let abreviado = abreviar_host(nombre, ancho, false);
                assert!(
                    abreviado.chars().count() <= ancho,
                    "{nombre} a {ancho}: {abreviado}"
                );
            }
        }
    }

    #[test]
    fn a_ascii_no_deja_glifos_y_respeta_las_tildes() {
        let texto = disposicion::adaptar(
            "reiniciar nginx → 3 hosts · «web» — ↓ 1 kB ↑ 2 kB… CRÍTICO ┃",
            true,
        );
        assert_eq!(
            texto,
            "reiniciar nginx -> 3 hosts - \"web\" - v 1 kB ^ 2 kB~ CRÍTICO |"
        );
        assert!(texto.chars().all(|c| c.is_ascii() || c.is_alphabetic()));
        assert_eq!(disposicion::adaptar("a → b", false), "a → b");
    }

    #[test]
    fn la_tabla_no_pasa_de_su_ancho_natural() {
        let columnas = [
            Columna::fija(1, 1),
            Columna::flexible(10, 1),
            Columna::fija(8, 3),
        ];
        let tabla = Tabla::nueva(200, &columnas, &[1, 30, 8]);
        assert_eq!(tabla.anchos, vec![1, 30, 8]);
        // Una fija con ancho natural mayor lo usa si cabe todo.
        let tabla = Tabla::nueva(200, &columnas, &[1, 30, 20]);
        assert_eq!(tabla.anchos, vec![1, 30, 20]);
        let tabla = Tabla::nueva(30, &columnas, &[1, 30, 8]);
        assert_eq!(tabla.visibles, vec![true, true, true]);
        assert_eq!(tabla.anchos, vec![1, 17, 8]);
        let tabla = Tabla::nueva(20, &columnas, &[1, 30, 8]);
        assert_eq!(tabla.visibles, vec![true, true, false]);
        assert_eq!(tabla.anchos, vec![1, 17, 0]);
    }
}
