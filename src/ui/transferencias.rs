//! Vista Transferencias (F4): la cola ampliada, con el detalle de la
//! seleccionada al pie (bytes, velocidad y tiempo restante calculados con las
//! dos últimas difusiones del servidor).
//!
//! Disposición adaptable (Fase 7), derivada del área en cada pintado: las
//! columnas se ocultan por prioridad (el glifo de estado y `origen → destino`
//! nunca) y, con la vista baja (< 20 filas), el detalle inferior se pliega y
//! se abre con `↵`.
//!
//! Una sincronización con «borrar» (Fase 8) pasa al terminar la copia por la
//! fase `borrando`, en rojo, con «borrando N/M» en el progreso y «N borrados»
//! en el detalle.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::archivos::tamano_legible;
use crate::protocolo::{EstadoTransferencia, InfoTransferencia};
use crate::tema::Tema;
use crate::ui::archivos::{destino_corto, flecha, nombre_de};
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};
use crate::ui::{bloque, centrar, tecla};

/// Filas del detalle inferior: la línea que lo separa y tres de datos.
const ALTO_DETALLE: u16 = 4;

/// Columnas de la tabla: glifo de estado, estado, dirección, `origen →
/// destino` y progreso. El glifo y la ruta nunca se ocultan; el texto del
/// estado es lo primero que sobra (el glifo ya lo dice). En ASCII no: «en
/// cola» y «en curso» comparten glifo (`o`), así que sobra antes la dirección.
const COLUMNAS: [Columna; 5] = [
    Columna::fija(1, 1),
    Columna::fija(9, 4),
    Columna::fija(9, 3),
    Columna::flexible(16, 1),
    Columna::fija(14, 2),
];
/// Selección y un espacio antes de la primera columna.
const ANCHO_PREFIJO: usize = 2;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    let filas: &[InfoTransferencia] = app
        .archivos
        .as_ref()
        .map(|archivos| archivos.cola.as_slice())
        .unwrap_or(&[]);
    let cuantas = |estado| filas.iter().filter(|fila| fila.estado == estado).count();
    // Borrando sigue en curso: la transferencia aún no ha terminado.
    let en_curso = cuantas(EstadoTransferencia::EnCurso) + cuantas(EstadoTransferencia::Borrando);
    let en_cola = cuantas(EstadoTransferencia::EnCola);
    let titulo = disposicion::recortar(
        &format!("TRANSFERENCIAS {punto} {en_curso} en curso {punto} {en_cola} en cola"),
        usize::from(area.width.saturating_sub(4)),
        tema.ascii,
    );
    let marco_bloque = bloque(&titulo, tema);

    if filas.is_empty() {
        let interior = marco_bloque.inner(area);
        marco.render_widget(marco_bloque, area);
        let aviso = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No hay transferencias: la cola está vacía.",
                Style::default().fg(tema.paleta.inactivo),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Copia algo con c desde Archivos y aparecerá aquí.",
                Style::default().fg(tema.paleta.inactivo),
            )),
        ];
        marco.render_widget(Paragraph::new(aviso), interior);
        return;
    }

    let mut marco_bloque = marco_bloque.title_bottom(Span::styled(
        format!(" {} ", filas.len()),
        Style::default().fg(tema.paleta.inactivo),
    ));
    // Con la vista baja el detalle se pliega; el borde recuerda cómo abrirlo.
    // También se pliega si no deja sitio para la cabecera y una fila.
    let interior_previsto = marco_bloque.inner(area);
    let plegado = disp.bajo || interior_previsto.height < ALTO_DETALLE + 2;
    if plegado {
        marco_bloque = marco_bloque.title_bottom(
            Line::from(Span::styled(
                format!(" {} detalle ", tema.glifos.intro),
                Style::default().fg(tema.paleta.inactivo),
            ))
            .right_aligned(),
        );
    }
    let interior = marco_bloque.inner(area);
    marco.render_widget(marco_bloque, area);
    // En ventanas muy grandes la tabla y el detalle tienen ancho máximo y van
    // centrados, como en las demás vistas de tabla.
    let interior = if disposicion::es_grande(disp.area) {
        disposicion::limitar_ancho(interior, disposicion::ANCHO_MAX_DETALLE)
    } else {
        interior
    };

    let zonas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(if plegado { 0 } else { ALTO_DETALLE }),
        ])
        .split(interior);

    // Columnas por prioridad sobre el ancho que queda tras la selección.
    let ancho = usize::from(interior.width);
    let util = u16::try_from(ancho.saturating_sub(ANCHO_PREFIJO)).unwrap_or(u16::MAX);
    let mut columnas_tabla = COLUMNAS;
    if tema.ascii {
        columnas_tabla[1].prioridad = 2;
    }
    let visibles = disposicion::columnas_visibles(util, &columnas_tabla, 1);
    let mut anchos: Vec<usize> = disposicion::repartir(util, &columnas_tabla, &visibles, 1)
        .into_iter()
        .map(usize::from)
        .collect();
    // La ruta no pasa de la más larga (en la forma que cabe) y lo que sobra
    // va al progreso, para que un error se lea entero: en ventanas anchas la
    // tabla queda compacta en lugar de repartida de borde a borde.
    let mas_larga = filas
        .iter()
        .map(|fila| ruta_que_cabe(fila, anchos[3], tema.ascii).chars().count())
        .max()
        .unwrap_or(0)
        .max(usize::from(columnas_tabla[3].ancho));
    if visibles[4] && anchos[3] > mas_larga {
        let libre = anchos[3] - mas_larga;
        anchos[3] = mas_larga;
        let progreso_mas_largo = filas
            .iter()
            .map(|fila| progreso_de(fila).chars().count())
            .max()
            .unwrap_or(0);
        anchos[4] = (anchos[4] + libre).min(progreso_mas_largo.max(anchos[4]));
    } else {
        anchos[3] = anchos[3].min(mas_larga);
    }
    let columnas = Columnas { visibles, anchos };

    let cabecera = columnas.fila(
        "  ",
        [
            String::new(),
            "ESTADO".to_string(),
            "DIRECCIÓN".to_string(),
            format!("ORIGEN {} DESTINO", flecha(tema.ascii)),
            "PROGRESO".to_string(),
        ],
        tema,
    );
    marco.render_widget(
        Paragraph::new(Line::from(Span::styled(
            cabecera,
            Style::default()
                .fg(tema.paleta.inactivo)
                .add_modifier(Modifier::BOLD),
        ))),
        zonas[0],
    );

    let alto_lista = usize::from(zonas[1].height);
    let inicio = disposicion::ventana(
        app.desplazamiento_cola,
        app.seleccion_cola,
        alto_lista,
        filas.len(),
    );
    disp.registrar(
        Lista::Transferencias,
        VentanaLista {
            inicio,
            filas: alto_lista,
            total: filas.len(),
        },
    );
    let lineas: Vec<Line> = filas
        .iter()
        .enumerate()
        .skip(inicio)
        .take(alto_lista)
        .map(|(posicion, fila)| linea_de(fila, app, posicion == app.seleccion_cola, &columnas))
        .collect();
    marco.render_widget(
        Paragraph::new(lineas).style(Style::default().fg(tema.paleta.texto)),
        zonas[1],
    );

    if !plegado {
        dibujar_detalle(marco, zonas[2], app, filas);
    }
}

/// Columnas visibles y su ancho en este pintado.
struct Columnas {
    visibles: Vec<bool>,
    anchos: Vec<usize>,
}

impl Columnas {
    /// Una fila de texto con las columnas visibles separadas por un espacio.
    fn fila(&self, prefijo: &str, textos: [String; 5], tema: &Tema) -> String {
        self.trozos(textos, tema)
            .into_iter()
            .fold(prefijo.to_string(), |mut fila, (_, texto)| {
                fila.push_str(&texto);
                fila
            })
    }

    /// Cada columna visible ya recortada y rellenada a su ancho (con el
    /// separador delante salvo la primera), con su índice.
    fn trozos(&self, textos: [String; 5], tema: &Tema) -> Vec<(usize, String)> {
        let mut trozos = Vec::new();
        for (indice, texto) in textos.into_iter().enumerate() {
            if !self.visibles[indice] {
                continue;
            }
            let separador = if trozos.is_empty() { "" } else { " " };
            trozos.push((
                indice,
                format!(
                    "{separador}{}",
                    disposicion::columna(&texto, self.anchos[indice], tema.ascii)
                ),
            ));
        }
        trozos
    }
}

fn linea_de(
    fila: &InfoTransferencia,
    app: &App,
    seleccionada: bool,
    columnas: &Columnas,
) -> Line<'static> {
    let tema = &app.tema;
    let mut estilo = Style::default().fg(tema.paleta.texto);
    if seleccionada {
        estilo = estilo
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
            .add_modifier(Modifier::BOLD);
    }
    let color_estado = if seleccionada {
        estilo
    } else {
        match fila.estado {
            EstadoTransferencia::Error | EstadoTransferencia::Borrando => {
                Style::default().fg(tema.paleta.critico)
            }
            EstadoTransferencia::Hecha => Style::default().fg(tema.paleta.correcto),
            EstadoTransferencia::EnCurso => Style::default().fg(tema.paleta.acento),
            _ => Style::default().fg(tema.paleta.inactivo),
        }
    };
    let ruta = ruta_que_cabe(fila, columnas.anchos[3], tema.ascii);
    let progreso = progreso_de(fila);
    let textos = [
        fila.estado.glifo(tema.ascii).to_string(),
        fila.estado.texto().to_string(),
        format!(
            "{} {}",
            fila.direccion.glifo(tema.ascii),
            fila.direccion.texto()
        ),
        ruta,
        progreso,
    ];
    let seleccion = if seleccionada {
        tema.glifos.seleccion
    } else {
        " "
    };
    let mut spans = vec![Span::styled(format!("{seleccion} "), estilo)];
    for (indice, texto) in columnas.trozos(textos, tema) {
        let estilo_columna = match indice {
            0 | 1 | 4 => color_estado,
            _ => estilo,
        };
        spans.push(Span::styled(texto, estilo_columna));
    }
    Line::from(spans)
}

/// Columna de progreso: el error si lo hay, lo borrado de lo que hay que
/// borrar, los ficheros de un directorio terminado o el porcentaje y los
/// bytes hechos.
fn progreso_de(fila: &InfoTransferencia) -> String {
    if let Some(error) = &fila.error {
        error.clone()
    } else if fila.estado == EstadoTransferencia::Borrando {
        format!("borrando {}/{}", fila.borrados, fila.borrados_total)
    } else if fila.es_directorio && fila.estado.terminada() {
        format!("{} fichero(s)", fila.ficheros_hechos)
    } else {
        format!(
            "{:>3} %  {}",
            fila.porcentaje(),
            tamano_legible(fila.bytes_hechos)
        )
    }
}

/// `origen → host:destino` completa si cabe en `ancho`; si no, con solo los
/// nombres (`main.py → hetzner-01:main.py`) o, con menos aún, `main.py →
/// hetzner-01`, que la columna recorta. El nombre del fichero se ve siempre;
/// la ruta completa está en el detalle.
fn ruta_que_cabe(fila: &InfoTransferencia, ancho: usize, ascii: bool) -> String {
    let flecha = flecha(ascii);
    let completa = format!(
        "{} {flecha} {}:{}",
        fila.origen, fila.host_nombre, fila.destino
    );
    if completa.chars().count() <= ancho {
        return completa;
    }
    let corta = destino_corto(fila, ascii);
    if corta.chars().count() <= ancho {
        return corta;
    }
    format!("{} {flecha} {}", nombre_de(&fila.origen), fila.host_nombre)
}

/// Detalle de la seleccionada: una línea que lo separa de la tabla, la ruta
/// completa, los bytes (con velocidad y restante si se conocen) y el fichero
/// en curso o el error.
fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, filas: &[InfoTransferencia]) {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    let ancho = usize::from(area.width);
    let recortar = |texto: String| disposicion::recortar(&texto, ancho, tema.ascii);
    let mut lineas = vec![Line::from(Span::styled(
        tema.glifos.linea.repeat(ancho),
        Style::default().fg(tema.paleta.inactivo),
    ))];
    if let Some(fila) = filas.get(app.seleccion_cola) {
        lineas.push(Line::from(Span::styled(
            recortar(format!(
                "  {} {} {}:{}",
                fila.origen,
                flecha(tema.ascii),
                fila.host_nombre,
                fila.destino
            )),
            Style::default().fg(tema.paleta.texto),
        )));
        let velocidad = app
            .archivos
            .as_ref()
            .and_then(|archivos| archivos.velocidad(fila));
        let mut detalle = format!(
            "  {} de {} {punto} {} fichero(s)",
            tamano_legible(fila.bytes_hechos),
            tamano_legible(fila.bytes_total),
            fila.ficheros_hechos
        );
        if let Some(velocidad) = velocidad {
            if velocidad > 0.0 {
                detalle.push_str(&format!(
                    " {punto} {}/s {punto} {} restantes",
                    tamano_legible(velocidad as u64),
                    restante(fila, velocidad, tema.ascii)
                ));
            }
        }
        if fila.omitidos > 0 {
            detalle.push_str(&format!(" {punto} {} omitido(s)", fila.omitidos));
        }
        if fila.borrados_total > 0 {
            detalle.push_str(&format!(" {punto} {} borrados", fila.borrados));
        }
        lineas.push(Line::from(Span::styled(
            recortar(detalle),
            Style::default().fg(tema.paleta.inactivo),
        )));
        if let Some(fichero) = &fila.fichero_actual {
            lineas.push(Line::from(Span::styled(
                recortar(format!("  ahora: {fichero}")),
                Style::default().fg(tema.paleta.acento),
            )));
        } else if let Some(error) = &fila.error {
            lineas.push(Line::from(Span::styled(
                recortar(format!("  {error}")),
                Style::default().fg(tema.paleta.critico),
            )));
        }
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Tiempo restante estimado con la velocidad medida.
fn restante(fila: &InfoTransferencia, velocidad: f64, ascii: bool) -> String {
    let pendientes = fila.bytes_total.saturating_sub(fila.bytes_hechos) as f64;
    let segundos = pendientes / velocidad;
    if !segundos.is_finite() || segundos < 0.0 {
        return if ascii { "-" } else { "—" }.to_string();
    }
    if segundos < 60.0 {
        format!("{segundos:.0} s")
    } else if segundos < 3600.0 {
        format!("{:.0} min", segundos / 60.0)
    } else {
        format!("{:.1} h", segundos / 3600.0)
    }
}

/// Aviso modal que se pinta sobre la vista (confirmaciones de cancelar).
pub fn confirmar(marco: &mut Frame, area: Rect, app: &App, lineas: Vec<String>) {
    let tema = &app.tema;
    let alto = (lineas.len() as u16) + 4;
    let recta = centrar(area, 70, alto);
    let contenido: Vec<Line> = lineas
        .into_iter()
        .map(|texto| Line::from(Span::styled(texto, Style::default().fg(tema.paleta.texto))))
        .collect();
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.acento))
        .title(Span::styled(
            " CONFIRMAR ",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    marco.render_widget(ratatui::widgets::Clear, recta);
    marco.render_widget(bloque, recta);
    marco.render_widget(Paragraph::new(contenido), recta);
}

/// Atajos de la vista, para la barra de abajo.
pub fn atajos(app: &App) -> Vec<(&'static str, &'static str)> {
    vec![
        (tecla(&app.tema, "↓↑", "j/k"), "mover"),
        ("x", "cancelar"),
        ("C", "limpiar terminadas"),
        ("↵", "detalle"),
        ("?", "ayuda"),
        ("q", "volver"),
    ]
}
