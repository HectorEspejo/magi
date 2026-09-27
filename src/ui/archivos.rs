//! Vista Archivos (F4): dos paneles —local y remoto— del mismo widget, con la
//! cola de transferencias al pie.
//!
//! El panel activo lleva el nombre en ámbar y el otro el marco tenue; la fila
//! seleccionada lleva `▸` y las marcas (`≠`, `✕`) se pintan al final de la fila.
//!
//! Disposición adaptable (Fase 7), derivada del área en cada pintado:
//! - **Normal** (≥ 100 columnas): los dos paneles lado a lado y, si la vista no
//!   es baja, la cola en sus 3 líneas al pie.
//! - **Estrecho** (< 100 columnas): un solo panel —el activo—; `Tab` alterna y
//!   la cabecera dice cuál se ve (`[local]` / `[remoto]`).
//! - **Bajo** (< 20 filas) o estrecho: la cola se resume en el pie.
//!
//! Los marcados, el filtro, la selección y el panel activo viven en el estado
//! de la App: cambiar de modo no los toca.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::archivos::panel::{EstadoArchivos, Lado, Panel};
use crate::archivos::{fecha_legible, tamano_legible, Marca};
use crate::protocolo::{EstadoTransferencia, InfoTransferencia};
use crate::tema::Tema;
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};
use crate::ui::{centrar, tecla};

/// Filas de la cola al pie de la vista.
pub const ALTO_COLA: u16 = 3;

/// Columnas de una fila de panel: nombre, tamaño, fecha y marca. El nombre y
/// la marca (el «glifo de estado» del panel) nunca se ocultan.
const COLUMNAS_PANEL: [Columna; 4] = [
    Columna::flexible(10, 1),
    Columna::fija(8, 2),
    Columna::fija(6, 3),
    Columna::fija(2, 1),
];
/// Selección, marca y un espacio antes del nombre.
const ANCHO_PREFIJO: usize = 3;
/// Separador entre el pie del panel y la cola resumida en modo estrecho.
const ANCHO_SEPARADOR_PIE: usize = 5;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let Some(archivos) = &app.archivos else {
        return;
    };
    let estrecho = disposicion::es_estrecho(area.width);
    // Sin filas para las 3 líneas de la cola (vista baja) o con un solo panel,
    // la cola se resume en el pie (de más a menos completa: se pinta la que
    // quepa).
    let resumenes = if !archivos.cola.is_empty() && (estrecho || disp.bajo) {
        resumenes_cola(archivos, &app.tema)
    } else {
        Vec::new()
    };

    if estrecho {
        dibujar_estrecho(marco, area, app, archivos, &resumenes, disp);
    } else {
        // Los paneles no crecen sin límite en ventanas muy anchas: cada uno
        // como mucho el ancho máximo de un detalle, y el conjunto centrado.
        let area = disposicion::limitar_ancho(area, 2 * disposicion::ANCHO_MAX_DETALLE);
        let alto_cola = if archivos.cola.is_empty() || !resumenes.is_empty() {
            0
        } else {
            ALTO_COLA
        };
        let trozos = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(alto_cola)])
            .split(area);
        let paneles = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(trozos[0]);

        dibujar_panel(marco, paneles[0], app, archivos, Lado::Local, &[], disp);
        if archivos.solo_local {
            dibujar_sin_remoto(marco, paneles[1], app, archivos, &resumenes);
        } else {
            dibujar_panel(
                marco,
                paneles[1],
                app,
                archivos,
                Lado::Remoto,
                &resumenes,
                disp,
            );
        }
        if alto_cola > 0 {
            dibujar_cola(marco, trozos[1], app, archivos);
        }
    }
    if let Some(aviso) = &archivos.aviso {
        dibujar_aviso(marco, area, app, aviso);
    }
}

fn panel_de(archivos: &EstadoArchivos, lado: Lado) -> &Panel {
    match lado {
        Lado::Local => &archivos.local,
        Lado::Remoto => &archivos.remoto,
    }
}

/// Estilo de un título: ámbar y en negrita si el panel tiene el foco.
fn estilo_titulo(tema: &Tema, activo: bool) -> Style {
    if activo {
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.inactivo)
    }
}

/// Ruta del panel con el filtro, si lo hay: `/var/www / main`.
fn ruta_con_filtro(panel: &Panel) -> String {
    let mut texto = panel.ruta.clone();
    if panel.filtro_activo || !panel.filtro.is_empty() {
        texto.push_str(&format!(" / {}", panel.filtro));
    }
    texto
}

/// Modo normal: un panel con borde, la ruta en el título y el pie en el borde
/// inferior. La cola resumida (`resumenes`, si la hay) va a la derecha del pie.
fn dibujar_panel(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    archivos: &EstadoArchivos,
    lado: Lado,
    resumenes: &[String],
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let activo = archivos.activo == lado;
    let panel = panel_de(archivos, lado);
    let color_borde = if activo {
        tema.paleta.acento
    } else {
        tema.paleta.inactivo
    };
    // Los títulos no pisan las esquinas: dos columnas menos, y un espacio a
    // cada lado del texto.
    let hueco = usize::from(area.width.saturating_sub(4));
    let titulo = disposicion::recortar(&ruta_con_filtro(panel), hueco, tema.ascii);
    // Pie y cola resumida comparten el borde inferior: cada texto con un
    // espacio a cada lado y una columna de borde entre los dos.
    let (pie, resumen) = if resumenes.is_empty() {
        (
            disposicion::recortar(&pie_de(panel, tema), hueco, tema.ascii),
            None,
        )
    } else {
        encajar_pie(
            &pie_de(panel, tema),
            resumenes,
            hueco.saturating_sub(3),
            tema.ascii,
        )
    };
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(color_borde))
        .title(Span::styled(
            format!(" {titulo} "),
            estilo_titulo(tema, activo),
        ))
        .title_bottom(Span::styled(
            format!(" {pie} "),
            Style::default().fg(tema.paleta.inactivo),
        ));
    if let Some(resumen) = resumen {
        bloque = con_resumen(bloque, &resumen, tema);
    }
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    pintar_lista(marco, interior, app, panel, lado, activo, disp);
}

/// Añade la cola resumida (ya encajada) a la derecha del borde inferior.
fn con_resumen<'a>(bloque: Block<'a>, resumen: &str, tema: &Tema) -> Block<'a> {
    if resumen.is_empty() {
        return bloque;
    }
    bloque.title_bottom(
        Line::from(Span::styled(
            format!(" {resumen} "),
            Style::default().fg(tema.paleta.acento),
        ))
        .right_aligned(),
    )
}

/// Reparte `util` columnas entre el pie del panel y la cola resumida: el pie
/// entero con la variante de la cola más completa que quepa; si no cabe ni la
/// más corta, el pie se recorta para dejarle sitio. La cola no desaparece
/// nunca del pie mientras haya sitio para ella.
fn encajar_pie(
    pie: &str,
    resumenes: &[String],
    util: usize,
    ascii: bool,
) -> (String, Option<String>) {
    let largo = |texto: &str| texto.chars().count();
    let Some(corto) = resumenes.last() else {
        return (disposicion::recortar(pie, util, ascii), None);
    };
    if let Some(resumen) = resumenes
        .iter()
        .find(|resumen| largo(pie) + largo(resumen) <= util)
    {
        return (pie.to_string(), Some(resumen.clone()));
    }
    if largo(corto) >= util {
        return (
            String::new(),
            Some(disposicion::recortar(corto, util, ascii)),
        );
    }
    let resto = util - largo(corto);
    (
        disposicion::recortar(pie, resto, ascii),
        Some(corto.clone()),
    )
}

/// Pinta las filas del panel en `zona` y registra su ventana visible.
fn pintar_lista(
    marco: &mut Frame,
    zona: Rect,
    app: &App,
    panel: &Panel,
    lado: Lado,
    activo: bool,
    disp: &mut Disposicion,
) {
    let visibles = usize::from(zona.height);
    let total = panel.total_visibles();
    let inicio = disposicion::ventana(panel.desplazamiento, panel.seleccion, visibles, total);
    disp.registrar(
        match lado {
            Lado::Local => Lista::ArchivosLocal,
            Lado::Remoto => Lista::ArchivosRemoto,
        },
        VentanaLista {
            inicio,
            filas: visibles,
            total,
        },
    );
    let lineas = lineas_de(
        panel,
        app,
        activo,
        usize::from(zona.width),
        inicio,
        visibles,
    );
    marco.render_widget(
        Paragraph::new(lineas).style(Style::default().fg(app.tema.paleta.texto)),
        zona,
    );
}

/// Modo estrecho: un solo panel —el activo— con el marco de la vista. La
/// cabecera dice qué panel se ve y que `Tab` lleva al otro; dentro, la ruta,
/// la lista y el pie (con la cola resumida si la hay).
fn dibujar_estrecho(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    archivos: &EstadoArchivos,
    resumenes: &[String],
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let lado = archivos.activo;
    let (izquierda, derecha) = cabecera_estrecha(area.width, tema, archivos);
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.acento));
    if let Some(izquierda) = izquierda {
        bloque = bloque.title(Span::styled(
            format!(" {izquierda} "),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    }
    for (texto, resaltado) in derecha {
        let estilo = if resaltado {
            estilo_titulo(tema, true)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        bloque =
            bloque.title(Line::from(Span::styled(format!(" {texto} "), estilo)).right_aligned());
    }
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    // Una columna de aire a cada lado.
    let contenido = Rect {
        x: interior.x.saturating_add(1),
        width: interior.width.saturating_sub(2),
        ..interior
    };
    if contenido.width == 0 || contenido.height == 0 {
        return;
    }
    let ancho = usize::from(contenido.width);
    let panel = panel_de(archivos, lado);
    let sin_remoto = lado == Lado::Remoto && archivos.solo_local;

    // Ruta, línea, lista, línea y pie; con muy poco alto, solo la lista.
    let con_marco_interior = contenido.height >= 5;
    let (zona_lista, cabecera, pie) = if con_marco_interior {
        let zonas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(contenido);
        (zonas[1], Some(zonas[0]), Some(zonas[2]))
    } else {
        (contenido, None, None)
    };
    let linea = tema.glifos.linea.repeat(ancho);
    let estilo_linea = Style::default().fg(tema.paleta.inactivo);

    if let Some(cabecera) = cabecera {
        let ruta = if sin_remoto {
            "sin remoto".to_string()
        } else {
            ruta_con_filtro(panel)
        };
        marco.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    disposicion::recortar(&ruta, ancho, tema.ascii),
                    estilo_titulo(tema, true),
                )),
                Line::from(Span::styled(linea.clone(), estilo_linea)),
            ]),
            cabecera,
        );
    }
    if sin_remoto {
        marco.render_widget(Paragraph::new(lineas_sin_remoto(app, archivos)), zona_lista);
    } else {
        pintar_lista(marco, zona_lista, app, panel, lado, true, disp);
    }
    if let Some(pie_zona) = pie {
        let pie = if sin_remoto {
            String::new()
        } else {
            pie_de(panel, tema)
        };
        let texto = if resumenes.is_empty() {
            disposicion::recortar(&pie, ancho, tema.ascii)
        } else if pie.is_empty() {
            encajar_pie("", resumenes, ancho, tema.ascii)
                .1
                .unwrap_or_default()
        } else {
            let (pie, resumen) = encajar_pie(
                &pie,
                resumenes,
                ancho.saturating_sub(ANCHO_SEPARADOR_PIE),
                tema.ascii,
            );
            match resumen {
                Some(resumen) if pie.is_empty() => resumen,
                Some(resumen) => format!("{pie}  {}  {resumen}", tema.glifos.punto_medio),
                None => pie,
            }
        };
        marco.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(linea, estilo_linea)),
                Line::from(Span::styled(
                    texto,
                    Style::default().fg(tema.paleta.inactivo),
                )),
            ]),
            pie_zona,
        );
    }
}

/// Cabecera del modo estrecho: a la izquierda el nombre de la vista y a la
/// derecha `local ⇄ host` y el panel que se ve (`[remoto] ⇥`). Lo que no cabe
/// se quita por prioridad; el indicador de panel no se quita nunca. Cada
/// texto de la derecha va con «resaltado».
fn cabecera_estrecha(
    ancho: u16,
    tema: &Tema,
    archivos: &EstadoArchivos,
) -> (Option<String>, Vec<(String, bool)>) {
    let flecha = if tema.ascii { "<->" } else { "⇄" };
    let indicador = format!("[{}] {}", archivos.activo.texto(), tema.glifos.tab);
    let host = archivos.host_nombre.as_str();
    let extremos = format!("local {flecha} {host}");
    let completo = format!("MAGI {} ARCHIVOS", tema.glifos.punto_medio);
    // Lo que ocupa una combinación en el borde: esquinas, un espacio a cada
    // lado de cada título y una columna de borde entre títulos.
    let ocupa = |izquierda: Option<&str>, derecha: &[&str]| -> usize {
        let textos = izquierda.into_iter().chain(derecha.iter().copied());
        let (suma, cuantos) = textos.fold((0usize, 0usize), |(suma, cuantos), texto| {
            (suma + texto.chars().count() + 2, cuantos + 1)
        });
        2 + suma + cuantos.saturating_sub(1)
    };
    let ancho = usize::from(ancho);
    let opciones: [(Option<&str>, Vec<&str>); 4] = [
        (Some(&completo), vec![&extremos, &indicador]),
        (Some("ARCHIVOS"), vec![&extremos, &indicador]),
        (Some("ARCHIVOS"), vec![host, &indicador]),
        (Some("ARCHIVOS"), vec![&indicador]),
    ];
    for (izquierda, derecha) in &opciones {
        if ocupa(*izquierda, derecha) <= ancho {
            let mut textos: Vec<(String, bool)> = derecha
                .iter()
                .map(|texto| (texto.to_string(), false))
                .collect();
            if let Some(ultimo) = textos.last_mut() {
                ultimo.1 = true;
            }
            return (izquierda.map(str::to_string), textos);
        }
    }
    (None, vec![(indicador, true)])
}

/// Panel remoto cuando el host no ofrece SFTP: solo se dice por qué. La cola
/// resumida (`resumenes`, si la hay) va a la derecha del borde inferior.
fn dibujar_sin_remoto(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    archivos: &EstadoArchivos,
    resumenes: &[String],
) {
    let tema = &app.tema;
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.inactivo))
        .title(Span::styled(
            " sin remoto ",
            Style::default().fg(tema.paleta.inactivo),
        ));
    // Sin pie: la cola tiene todo el borde (un espacio a cada lado).
    let util = usize::from(area.width.saturating_sub(4));
    if let (_, Some(resumen)) = encajar_pie("", resumenes, util, tema.ascii) {
        bloque = con_resumen(bloque, &resumen, tema);
    }
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    marco.render_widget(Paragraph::new(lineas_sin_remoto(app, archivos)), interior);
}

fn lineas_sin_remoto(app: &App, archivos: &EstadoArchivos) -> Vec<Line<'static>> {
    let tema = &app.tema;
    let motivo = archivos
        .motivo_solo_local
        .clone()
        .unwrap_or_else(|| "este host no ofrece SFTP".to_string());
    vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {motivo}"),
            Style::default().fg(tema.paleta.critico),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  la vista sigue con el panel local",
            Style::default().fg(tema.paleta.inactivo),
        )),
    ]
}

fn pie_de(panel: &Panel, tema: &Tema) -> String {
    let punto = tema.glifos.punto_medio;
    let elementos = panel.total_visibles();
    let tamano = tamano_legible(panel.tamano_visible());
    let marcados = match panel.total_marcados() {
        0 => String::new(),
        otros => format!(" {punto} {otros} marcado(s)"),
    };
    format!("{elementos} elementos {punto} {tamano}{marcados}")
}

/// La cola en una línea, para el pie, de más a menos completa: lo que está en
/// curso (con su avance) y cuánto espera; si no hay nada en curso, lo que
/// espera. La última variante es lo imprescindible (el avance).
fn resumenes_cola(archivos: &EstadoArchivos, tema: &Tema) -> Vec<String> {
    let punto = tema.glifos.punto_medio;
    let en_cola = archivos
        .cola
        .iter()
        .filter(|fila| fila.estado == EstadoTransferencia::EnCola)
        .count();
    match archivos.en_curso() {
        Some(fila) => {
            let avance = format!(
                "{} {} %",
                fila.direccion.glifo(tema.ascii),
                fila.porcentaje()
            );
            let con_nombre = format!("{avance} {}", nombre_de(&fila.origen));
            let mut variantes = Vec::new();
            if en_cola > 0 {
                variantes.push(format!("{con_nombre} {punto} {en_cola} en cola"));
            }
            variantes.push(con_nombre);
            variantes.push(avance);
            variantes
        }
        None if en_cola > 0 => vec![
            format!(
                "en cola {en_cola} {punto} {}",
                tamano_legible(archivos.bytes_en_cola())
            ),
            format!("en cola {en_cola}"),
        ],
        None => vec!["cola sin pendientes".to_string()],
    }
}

/// Convierte las filas visibles de un panel en líneas de texto, con la fila
/// seleccionada resaltada. Las columnas se reparten por prioridad: con poco
/// ancho se van la fecha y el tamaño; el nombre y la marca se quedan.
fn lineas_de(
    panel: &Panel,
    app: &App,
    activo: bool,
    ancho: usize,
    inicio: usize,
    filas: usize,
) -> Vec<Line<'static>> {
    let tema = &app.tema;
    let indice = panel.visibles();
    if indice.is_empty() {
        let texto = if panel.error.is_some() {
            "  (no se pudo listar)"
        } else if !panel.filtro.is_empty() {
            "  (el filtro no deja nada)"
        } else {
            "  (vacío)"
        };
        return vec![Line::from(Span::styled(
            texto.to_string(),
            Style::default().fg(tema.paleta.inactivo),
        ))];
    }

    // Un tamaño más largo que su columna (`1000.0 MB`) la ensancha: así no
    // empuja la fecha y la marca de su fila.
    let mut columnas = COLUMNAS_PANEL;
    let tamano_mas_largo = indice
        .iter()
        .map(|indice_real| &panel.entradas[*indice_real])
        .filter(|entrada| !entrada.es_dir())
        .map(|entrada| tamano_legible(entrada.tamano).chars().count())
        .max()
        .unwrap_or(0);
    columnas[1].ancho = columnas[1]
        .ancho
        .max(u16::try_from(tamano_mas_largo).unwrap_or(u16::MAX));
    let util = u16::try_from(ancho.saturating_sub(ANCHO_PREFIJO)).unwrap_or(u16::MAX);
    let visibles = disposicion::columnas_visibles(util, &columnas, 1);
    let mut anchos: Vec<usize> = disposicion::repartir(util, &columnas, &visibles, 1)
        .into_iter()
        .map(usize::from)
        .collect();
    // El nombre ocupa lo que el más largo de todo lo visible (no solo de la
    // ventana, para que las columnas no bailen al desplazar): en paneles
    // anchos el tamaño y la fecha quedan junto al nombre.
    let mas_largo = indice
        .iter()
        .map(|indice_real| {
            nombre_visible(&panel.entradas[*indice_real])
                .chars()
                .count()
        })
        .max()
        .unwrap_or(0)
        .max(usize::from(columnas[0].ancho));
    anchos[0] = anchos[0].min(mas_largo);
    let sin_dato = if tema.ascii { "-" } else { "—" };
    let ahora = app.ahora_epoca();

    indice
        .iter()
        .enumerate()
        .skip(inicio)
        .take(filas)
        .map(|(posicion, indice_real)| {
            let entrada = &panel.entradas[*indice_real];
            let seleccionada = posicion == panel.seleccion && activo;
            let marcada = panel.marcados.contains(&entrada.nombre);
            // `..` es sintética: ni tamaño ni fecha.
            let padre = entrada.nombre == "..";
            let nombre = nombre_visible(entrada);
            let tamano = if padre {
                String::new()
            } else if entrada.es_dir() {
                sin_dato.to_string()
            } else {
                tamano_legible(entrada.tamano)
            };
            let fecha = if padre {
                String::new()
            } else {
                let fecha = fecha_legible(entrada.mtime, ahora);
                if tema.ascii && !fecha.is_ascii() {
                    sin_dato.to_string()
                } else {
                    fecha
                }
            };

            let mut estilo = Style::default().fg(tema.paleta.texto);
            if seleccionada {
                estilo = estilo
                    .bg(tema.paleta.acento)
                    .fg(tema.paleta.fondo)
                    .add_modifier(Modifier::BOLD);
            }
            let estilo_marca = if seleccionada {
                estilo
            } else {
                match entrada.marca {
                    Marca::Distinta => Style::default().fg(tema.paleta.acento),
                    Marca::Ausente => Style::default().fg(tema.paleta.critico),
                    Marca::Ninguna => estilo,
                }
            };

            let mut spans = vec![Span::styled(
                format!(
                    "{}{} ",
                    if seleccionada {
                        tema.glifos.seleccion
                    } else {
                        " "
                    },
                    if marcada { tema.glifos.marca } else { " " },
                ),
                estilo,
            )];
            let textos = [
                disposicion::columna(&nombre, anchos[0], tema.ascii),
                format!("{:>ancho$}", tamano, ancho = anchos[1]),
                disposicion::columna(&fecha, anchos[2], tema.ascii),
                disposicion::columna(entrada.marca.glifo(tema.ascii), anchos[3], tema.ascii),
            ];
            let mut primera = true;
            for (indice, texto) in textos.into_iter().enumerate() {
                if !visibles[indice] {
                    continue;
                }
                if !primera {
                    spans.push(Span::styled(" ", estilo));
                }
                primera = false;
                let estilo_columna = if indice == 3 { estilo_marca } else { estilo };
                spans.push(Span::styled(texto, estilo_columna));
            }
            Line::from(spans)
        })
        .collect()
}

/// Nombre tal como se pinta: los enlaces llevan `@`.
fn nombre_visible(entrada: &crate::archivos::Entrada) -> String {
    let mut nombre = entrada.nombre.clone();
    if entrada.tipo == crate::archivos::TipoEntrada::Enlace {
        nombre.push('@');
    }
    nombre
}

/// Cola al pie: la transferencia en curso con su barra y lo que espera turno.
fn dibujar_cola(marco: &mut Frame, area: Rect, app: &App, archivos: &EstadoArchivos) {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    let mut lineas = Vec::new();
    if let Some(fila) = archivos.en_curso() {
        lineas.push(linea_en_curso(fila, app));
    } else {
        lineas.push(Line::from(Span::styled(
            "  nada en curso",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let en_cola = archivos
        .cola
        .iter()
        .filter(|fila| fila.estado == EstadoTransferencia::EnCola)
        .count();
    let tamano = tamano_legible(archivos.bytes_en_cola());
    let texto = if en_cola == 0 {
        "  sin nada en cola".to_string()
    } else {
        format!("  en cola {en_cola} {punto} {tamano}")
    };
    lineas.push(Line::from(Span::styled(
        texto,
        Style::default().fg(tema.paleta.texto),
    )));
    // `x` y `C` son teclas de la vista Transferencias (aquí `x` borra).
    lineas.push(Line::from(Span::styled(
        format!(
            "  {} transferencias: {} cancelar {punto} {} limpiar",
            tecla(tema, "t", "t"),
            tecla(tema, "x", "x"),
            tecla(tema, "C", "C"),
        ),
        Style::default().fg(tema.paleta.inactivo),
    )));
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Barra de progreso de `ancho` celdas.
fn barra(fraccion: f64, ancho: usize, ascii: bool) -> String {
    let llenos = (fraccion.clamp(0.0, 1.0) * ancho as f64).round() as usize;
    let (lleno, vacio) = if ascii { ('#', '.') } else { ('█', '░') };
    (0..ancho)
        .map(|indice| if indice < llenos { lleno } else { vacio })
        .collect()
}

/// Una línea con la barra de progreso y los datos de la transferencia.
pub fn linea_en_curso(fila: &InfoTransferencia, app: &App) -> Line<'static> {
    let tema = &app.tema;
    let tanto = fila.porcentaje();
    let barra = barra(f64::from(tanto) / 100.0, 20, tema.ascii);
    Line::from(vec![
        Span::styled(
            format!(" {} ", fila.direccion.glifo(tema.ascii)),
            Style::default().fg(tema.paleta.acento),
        ),
        Span::styled(
            disposicion::recortar(&destino_corto(fila, tema.ascii), 28, tema.ascii),
            Style::default().fg(tema.paleta.texto),
        ),
        Span::styled(
            format!(" {barra} "),
            Style::default().fg(if tanto == 100 {
                tema.paleta.correcto
            } else {
                tema.paleta.acento
            }),
        ),
        Span::styled(
            format!(
                "{tanto:>3} % {} {} de {}",
                tema.glifos.punto_medio,
                tamano_legible(fila.bytes_hechos),
                tamano_legible(fila.bytes_total)
            ),
            Style::default().fg(tema.paleta.texto),
        ),
    ])
}

/// `origen → host:destino` recortado, que es lo que cabe en la cola.
pub fn destino_corto(fila: &InfoTransferencia, ascii: bool) -> String {
    format!(
        "{} {} {}:{}",
        nombre_de(&fila.origen),
        flecha(ascii),
        fila.host_nombre,
        nombre_de(&fila.destino)
    )
}

/// `→`, o `->` en ASCII.
pub fn flecha(ascii: bool) -> &'static str {
    if ascii {
        "->"
    } else {
        "→"
    }
}

/// Último tramo de una ruta: el nombre del fichero o directorio.
pub fn nombre_de(ruta: &str) -> &str {
    ruta.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(ruta)
}

/// Aviso de subida de ficheros sensibles, centrado y en rojo. Los textos se
/// parten por palabras al ancho del diálogo; si no cabe entero, se quitan
/// coincidencias (con «y N más»): la pregunta y las teclas no se pierden
/// nunca.
fn dibujar_aviso(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    aviso: &crate::archivos::panel::AvisoPendiente,
) {
    let tema = &app.tema;
    // El ancho no depende del alto: se calcula antes para partir los textos.
    let ancho = usize::from(centrar(area, 74, 3).width.saturating_sub(2));
    const TEXTO: &str = "Vas a subir ficheros que coinciden con los patrones de aviso:";
    // En ASCII no cabe el «¿» (no es ASCII ni letra).
    const PREGUNTA: &str = "Suelen contener secretos. ¿Subirlos igualmente?";
    const PREGUNTA_ASCII: &str = "Suelen contener secretos. Subirlos igualmente?";
    let pregunta_completa = if tema.ascii { PREGUNTA_ASCII } else { PREGUNTA };
    // Márgenes (arriba, tras el texto y antes de la pregunta), los textos y
    // la línea de teclas; más dos de borde y los dos de margen de `centrar`.
    let sitio = |texto: &[String], pregunta: &[String]| {
        let fijas = 3 + texto.len() + pregunta.len() + 1;
        (
            fijas,
            usize::from(area.height).saturating_sub(2 + 2 + fijas),
        )
    };
    let mut texto = partir(TEXTO, ancho.saturating_sub(2));
    let mut pregunta = partir(pregunta_completa, ancho.saturating_sub(2));
    let (mut fijas, mut cabe) = sitio(&texto, &pregunta);
    // Si partidos no dejan ver ni una coincidencia, van en una línea cada uno
    // (recortados): qué ficheros son importa más que la explicación.
    if cabe < 2 && (texto.len() > 1 || pregunta.len() > 1) {
        texto = vec![disposicion::recortar(
            TEXTO,
            ancho.saturating_sub(2),
            tema.ascii,
        )];
        pregunta = vec![disposicion::recortar(
            pregunta_completa,
            ancho.saturating_sub(2),
            tema.ascii,
        )];
        (fijas, cabe) = sitio(&texto, &pregunta);
    }
    let total = aviso.coincidencias.len() + aviso.restantes;
    let mostradas = if total <= cabe {
        aviso.coincidencias.len()
    } else {
        // Una línea se la lleva «y N más».
        cabe.saturating_sub(1).min(aviso.coincidencias.len())
    };
    let restantes = total - mostradas;
    let alto = fijas + mostradas + usize::from(restantes > 0) + 2;
    let recta = centrar(area, 74, u16::try_from(alto).unwrap_or(u16::MAX));
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(tema.paleta.critico))
        .title(Span::styled(
            " AVISO ",
            Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD),
        ));
    let interior = bloque.inner(recta);
    marco.render_widget(ratatui::widgets::Clear, recta);
    marco.render_widget(bloque, recta);

    let estilo_texto = Style::default().fg(tema.paleta.texto);
    let mut cuerpo = vec![Line::from("")];
    for linea in texto {
        cuerpo.push(Line::from(Span::styled(format!("  {linea}"), estilo_texto)));
    }
    cuerpo.push(Line::from(""));
    for (nombre, bytes) in aviso.coincidencias.iter().take(mostradas) {
        cuerpo.push(Line::from(Span::styled(
            disposicion::recortar(
                &format!(
                    "    {nombre}  ({}),  {}  {}",
                    tamano_legible(*bytes),
                    flecha(tema.ascii),
                    aviso.destino
                ),
                ancho,
                tema.ascii,
            ),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    if restantes > 0 {
        cuerpo.push(Line::from(Span::styled(
            format!("    y {restantes} más"),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    cuerpo.push(Line::from(""));
    let mut final_: Vec<Line> = pregunta
        .into_iter()
        .map(|linea| Line::from(Span::styled(format!("  {linea}"), estilo_texto)))
        .collect();
    final_.push(Line::from(vec![
        Span::styled(
            format!("  {} subir", tecla(tema, "s", "s")),
            Style::default().fg(tema.paleta.correcto),
        ),
        Span::styled(
            format!("      {} cancelar (recomendado)", tecla(tema, "esc", "esc")),
            Style::default().fg(tema.paleta.acento),
        ),
    ]));
    // Si el diálogo encogió más de lo previsto, se sacrifica el cuerpo.
    let sitio_cuerpo = usize::from(interior.height).saturating_sub(final_.len());
    cuerpo.truncate(sitio_cuerpo);
    cuerpo.extend(final_);
    marco.render_widget(Paragraph::new(cuerpo), interior);
}

/// Parte un texto en líneas de como mucho `ancho` caracteres, por palabras
/// (una palabra más larga que el ancho se corta).
fn partir(texto: &str, ancho: usize) -> Vec<String> {
    let ancho = ancho.max(1);
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        let mut palabra = palabra.to_string();
        while palabra.chars().count() > ancho {
            if !actual.is_empty() {
                lineas.push(std::mem::take(&mut actual));
            }
            lineas.push(palabra.chars().take(ancho).collect());
            palabra = palabra.chars().skip(ancho).collect();
        }
        let largo = actual.chars().count();
        if largo > 0 && largo + 1 + palabra.chars().count() > ancho {
            lineas.push(std::mem::take(&mut actual));
        }
        if !actual.is_empty() {
            actual.push(' ');
        }
        actual.push_str(&palabra);
    }
    if !actual.is_empty() {
        lineas.push(actual);
    }
    lineas
}

/// Recorta un texto al ancho dado, con puntos suspensivos si sobra.
pub fn acortar(texto: &str, ancho: usize) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    let recortado: String = texto.chars().take(ancho.saturating_sub(1)).collect();
    format!("{recortado}…")
}

/// Glifo del estado de una transferencia, para la vista ampliada.
pub fn glifo_estado(estado: EstadoTransferencia, ascii: bool) -> &'static str {
    estado.glifo(ascii)
}
