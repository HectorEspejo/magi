//! Vista Resultados (subvista de F8, maqueta §6.4): las ejecuciones que
//! difunde el servidor (panel superior), los hosts de la seleccionada con su
//! estado y una vista previa del host seleccionado; con `↵`, el visor de
//! salida del host (stdout y stderr separados y desplazables).
//!
//! Nada que venga del remoto llega a la terminal sin limpiar: la salida se
//! pinta con las líneas limpias que el estado calcula al recibirla (sin
//! secuencias de escape ni caracteres de control) y los nombres y errores
//! pasan por `pantalla`.

use chrono::{DateTime, Local, TimeZone as _};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, EstadoResultados};
use crate::archivos::tamano_legible;
use crate::protocolo::{EstadoEjecucion, EstadoHostEjecucion, InfoEjecucion, InfoEjecucionHost};
use crate::snippets::salida::duracion_legible;
use crate::tema::Tema;
use crate::ui::archivos::acortar;
use crate::ui::snippets::{limpio, partir};
use crate::ui::{bloque, tecla};

/// Filas de la vista previa del host seleccionado (rótulo y dos de detalle).
const ALTO_PREVIA: usize = 3;

/// Filas fijas del visor: cabecera, pie y los rótulos de los dos flujos.
const FIJAS_VISOR: usize = 4;

/// Columnas fijas de una fila de ejecución (todo menos el nombre).
const FIJAS_EJECUCION: usize = 3 + 5 + 2 + 2 + 5 + 2 + 3 + 1 + 3 + 1 + 3 + 2 + 12;

/// Columnas fijas de una fila de host (todo menos el nombre y el error).
const FIJAS_HOST: usize = 3 + 2 + 2 + 10 + 1 + 4 + 2 + 8 + 2 + 8 + 2;

/// Filas visibles de los paneles de ejecuciones y de hosts con una terminal
/// de `terminal_alto` filas. Lo comparte `app::resultados` para mover la
/// selección con la misma altura con la que se pinta.
pub fn alturas_paneles(terminal_alto: u16, ejecuciones: usize) -> (usize, usize) {
    let (alto_ejecuciones, alto_hosts, _) = reparto(interior_de(terminal_alto), ejecuciones);
    (alto_ejecuciones.max(1), alto_hosts.max(1))
}

/// Filas de contenido de cada flujo del visor (stdout, stderr).
pub fn alturas_visor(terminal_alto: u16) -> [usize; 2] {
    let [stdout, stderr] = reparto_visor(interior_de(terminal_alto));
    [stdout.max(1), stderr.max(1)]
}

/// Alto interior de la vista: la terminal menos la barra y los dos bordes.
fn interior_de(terminal_alto: u16) -> usize {
    (terminal_alto as usize).saturating_sub(1 + 2)
}

/// Reparto del interior: filas de ejecuciones, de hosts y de vista previa.
/// Las ejecuciones se quedan como mucho con dos quintos de lo libre.
fn reparto(interior: usize, ejecuciones: usize) -> (usize, usize, usize) {
    let previa = if interior >= 12 { ALTO_PREVIA } else { 0 };
    let separadores = if previa > 0 { 2 } else { 1 };
    let libres = interior.saturating_sub(1 + separadores + previa);
    let maximo = (libres * 2 / 5).max(1);
    let alto_ejecuciones = ejecuciones.clamp(1, maximo).min(libres);
    (alto_ejecuciones, libres - alto_ejecuciones, previa)
}

/// Reparto del visor: stdout se queda con la fila impar.
fn reparto_visor(interior: usize) -> [usize; 2] {
    let libres = interior.saturating_sub(FIJAS_VISOR);
    [libres - libres / 2, libres / 2]
}

/// Texto de fuera (nombres, errores) listo para pintar en una línea: sin
/// escapes, sin caracteres de control y sin marcas de dirección.
pub fn pantalla(texto: &str) -> String {
    sin_marcas(&limpio(texto))
}

/// Quita las marcas de dirección del texto (bidi), que la terminal podría
/// aplicar al resto de la línea, y cualquier control que quedara.
pub fn sin_marcas(texto: &str) -> String {
    texto
        .chars()
        .filter(|caracter| !caracter.is_control() && !es_marca_de_direccion(*caracter))
        .collect()
}

fn es_marca_de_direccion(caracter: char) -> bool {
    matches!(
        caracter,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    dibujar_resultados(marco, area, &app.tema, &app.resultados, Local::now());
}

/// La vista entera con un estado y un reloj dados (lo usan las pruebas).
pub fn dibujar_resultados(
    marco: &mut Frame,
    area: Rect,
    tema: &Tema,
    estado: &EstadoResultados,
    ahora: DateTime<Local>,
) {
    let titulo = format!(
        "MAGI · RESULTADOS · {} en curso · {} hoy",
        estado.en_curso(),
        de_hoy(&estado.ejecuciones, ahora)
    );
    let marco_bloque = bloque(&titulo, tema);
    let interior = marco_bloque.inner(area);
    marco.render_widget(marco_bloque, area);
    if interior.height == 0 || interior.width == 0 {
        return;
    }
    let ahora_ms = ahora.timestamp_millis();
    if estado.visor.is_some() {
        dibujar_visor(marco, interior, tema, estado, ahora_ms);
    } else if estado.ejecuciones.is_empty() {
        dibujar_vacia(marco, interior, tema);
    } else {
        dibujar_paneles(marco, interior, tema, estado, ahora_ms);
    }
}

fn dibujar_vacia(marco: &mut Frame, interior: Rect, tema: &Tema) {
    let centrado = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(interior);
    marco.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Sin ejecuciones: lanza un snippet desde F8",
                Style::default().fg(tema.paleta.inactivo),
            )),
        ])
        .alignment(Alignment::Center),
        centrado[1],
    );
}

/// Ejecuciones creadas hoy (hora local).
fn de_hoy(ejecuciones: &[InfoEjecucion], ahora: DateTime<Local>) -> usize {
    let hoy = ahora.date_naive();
    ejecuciones
        .iter()
        .filter(|ejecucion| {
            Local
                .timestamp_opt(ejecucion.creada_en, 0)
                .single()
                .is_some_and(|fecha| fecha.date_naive() == hoy)
        })
        .count()
}

/// `HH:MM` local de una época en segundos.
fn hora_corta(epoca: i64) -> String {
    match Local.timestamp_opt(epoca, 0).single() {
        Some(fecha) => fecha.format("%H:%M").to_string(),
        None => "--:--".to_string(),
    }
}

/// Primera fila visible para que la selección quede a la vista, partiendo del
/// desplazamiento guardado (que pudo calcularse con otra altura).
fn ventana(seleccion: usize, desplazamiento: usize, altura: usize, total: usize) -> usize {
    let altura = altura.max(1);
    let mut inicio = desplazamiento;
    if seleccion < inicio {
        inicio = seleccion;
    }
    if seleccion >= inicio + altura {
        inicio = seleccion + 1 - altura;
    }
    inicio.min(total.saturating_sub(altura))
}

fn estilo_seleccion(tema: &Tema) -> Style {
    Style::default()
        .bg(tema.paleta.acento)
        .fg(tema.paleta.fondo)
        .add_modifier(Modifier::BOLD)
}

/// Color del glifo de un host.
fn color_host(estado: EstadoHostEjecucion, tema: &Tema) -> Color {
    match estado {
        EstadoHostEjecucion::Ok => tema.paleta.correcto,
        EstadoHostEjecucion::Fallo | EstadoHostEjecucion::Error => tema.paleta.critico,
        EstadoHostEjecucion::Conectando | EstadoHostEjecucion::Ejecutando => tema.paleta.acento,
        EstadoHostEjecucion::EnCola
        | EstadoHostEjecucion::Cancelado
        | EstadoHostEjecucion::Omitido => tema.paleta.inactivo,
    }
}

/// Hosts ok, con fallo o error, y en marcha o en cola.
fn recuento(ejecucion: &InfoEjecucion) -> (usize, usize, usize) {
    let mut ok = 0;
    let mut fallo = 0;
    let mut pendientes = 0;
    for host in &ejecucion.hosts {
        match host.estado {
            EstadoHostEjecucion::Ok => ok += 1,
            EstadoHostEjecucion::Fallo | EstadoHostEjecucion::Error => fallo += 1,
            EstadoHostEjecucion::EnCola
            | EstadoHostEjecucion::Conectando
            | EstadoHostEjecucion::Ejecutando => pendientes += 1,
            EstadoHostEjecucion::Cancelado | EstadoHostEjecucion::Omitido => {}
        }
    }
    (ok, fallo, pendientes)
}

/// Duración que se enseña: la que lleva un host en marcha (con el reloj
/// local: cliente y servidor están en la misma máquina) o la final.
fn duracion_host(host: &InfoEjecucionHost, ahora_ms: i64) -> Option<u64> {
    if host.estado.en_marcha() {
        if let Some(inicio) = host.inicio_ms {
            return Some(ahora_ms.saturating_sub(inicio).max(0) as u64);
        }
    }
    host.duracion_ms
}

// ---------------------------------------------------------------- paneles

fn dibujar_paneles(
    marco: &mut Frame,
    interior: Rect,
    tema: &Tema,
    estado: &EstadoResultados,
    ahora_ms: i64,
) {
    let (alto_ejecuciones, alto_hosts, alto_previa) =
        reparto(interior.height as usize, estado.ejecuciones.len());
    let ancho = interior.width as usize;
    let panel_hosts = estado.panel_hosts();

    // Nombres saneados una vez por dibujo; el ancho de la columna no baila
    // al moverse.
    let nombres: Vec<String> = estado
        .ejecuciones
        .iter()
        .map(|ejecucion| pantalla(&ejecucion.nombre))
        .collect();
    let ancho_nombre = nombres
        .iter()
        .map(|nombre| nombre.chars().count())
        .max()
        .unwrap_or(7)
        .clamp(7, 30)
        .min(ancho.saturating_sub(FIJAS_EJECUCION))
        .max(7);

    let mut lineas: Vec<Line> = Vec::with_capacity(interior.height as usize);
    let estilo_cabecera = if panel_hosts {
        Style::default()
            .fg(tema.paleta.inactivo)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD)
    };
    lineas.push(Line::from(Span::styled(
        format!(
            "   {:<5}  {:<ancho_nombre$}  {:>5}  {:>3} {:>3} {:>3}  {}",
            "HORA",
            "SNIPPET",
            "HOSTS",
            tecla(tema, "✓", "ok"),
            tecla(tema, "✕", "x"),
            tecla(tema, "…", ".."),
            "ESTADO",
        ),
        estilo_cabecera,
    )));

    let seleccion = estado.indice_seleccionada();
    let inicio = ventana(
        seleccion.unwrap_or(0),
        estado.desplazamiento_ejecuciones,
        alto_ejecuciones,
        estado.ejecuciones.len(),
    );
    for (posicion, ejecucion) in estado
        .ejecuciones
        .iter()
        .enumerate()
        .skip(inicio)
        .take(alto_ejecuciones)
    {
        lineas.push(linea_ejecucion(
            ejecucion,
            &nombres[posicion],
            ancho_nombre,
            seleccion == Some(posicion),
            !panel_hosts,
            tema,
        ));
    }
    rellenar(&mut lineas, 1 + alto_ejecuciones);

    let seleccionada = estado.ejecucion_seleccionada();
    let etiqueta = match seleccionada {
        Some(ejecucion) => format!(
            "HOSTS · {}",
            acortar(
                &pantalla(&ejecucion.nombre),
                ancho.saturating_sub(16).max(4)
            )
        ),
        None => "HOSTS".to_string(),
    };
    lineas.push(separador(Some(&etiqueta), panel_hosts, ancho, tema));

    if let Some(ejecucion) = seleccionada {
        let nombres_hosts: Vec<String> = ejecucion
            .hosts
            .iter()
            .map(|host| pantalla(&host.nombre))
            .collect();
        let ancho_host = nombres_hosts
            .iter()
            .map(|nombre| nombre.chars().count())
            .max()
            .unwrap_or(6)
            .clamp(6, 24)
            .min(ancho.saturating_sub(FIJAS_HOST))
            .max(6);
        let inicio = ventana(
            estado.host_seleccionado,
            estado.desplazamiento_hosts,
            alto_hosts,
            ejecucion.hosts.len(),
        );
        for (posicion, host) in ejecucion
            .hosts
            .iter()
            .enumerate()
            .skip(inicio)
            .take(alto_hosts)
        {
            lineas.push(linea_host(
                host,
                &nombres_hosts[posicion],
                ancho_host,
                posicion == estado.host_seleccionado,
                panel_hosts,
                ahora_ms,
                tema,
            ));
        }
    }
    rellenar(&mut lineas, 1 + alto_ejecuciones + 1 + alto_hosts);

    if alto_previa > 0 {
        lineas.push(separador(None, false, ancho, tema));
        lineas.extend(lineas_previa(
            estado.host_actual(),
            ancho,
            alto_previa,
            tema,
        ));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Completa con líneas vacías hasta `filas`.
fn rellenar(lineas: &mut Vec<Line<'static>>, filas: usize) {
    while lineas.len() < filas {
        lineas.push(Line::from(""));
    }
}

/// Raya separadora, con rótulo si lo hay; en color de acento si marca el
/// panel activo.
fn separador(etiqueta: Option<&str>, activo: bool, ancho: usize, tema: &Tema) -> Line<'static> {
    let raya = tecla(tema, "─", "-");
    let estilo = if activo {
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.inactivo)
    };
    let texto = match etiqueta {
        Some(etiqueta) => {
            let rotulo = format!(" {etiqueta} ");
            let resto = ancho.saturating_sub(2 + rotulo.chars().count());
            format!("{}{rotulo}{}", raya.repeat(2), raya.repeat(resto))
        }
        None => raya.repeat(ancho),
    };
    Line::from(Span::styled(texto, estilo))
}

fn linea_ejecucion(
    ejecucion: &InfoEjecucion,
    nombre: &str,
    ancho_nombre: usize,
    seleccionada: bool,
    panel_activo: bool,
    tema: &Tema,
) -> Line<'static> {
    let (ok, fallo, pendientes) = recuento(ejecucion);
    let marca = if seleccionada {
        tecla(tema, "▸", ">")
    } else {
        " "
    };
    let mut estado = ejecucion.estado.texto().to_string();
    if ejecucion.forzada {
        estado.push(' ');
        estado.push_str(tecla(tema, "⚑", "!"));
    }
    let resaltada = seleccionada && panel_activo;
    let estilo = if resaltada {
        estilo_seleccion(tema)
    } else if seleccionada {
        Style::default()
            .fg(tema.paleta.texto)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.texto)
    };
    let estilo_estado = if resaltada {
        estilo
    } else {
        let color = match ejecucion.estado {
            EstadoEjecucion::EnCurso => tema.paleta.acento,
            EstadoEjecucion::Terminada if fallo > 0 => tema.paleta.critico,
            EstadoEjecucion::Terminada => tema.paleta.correcto,
            EstadoEjecucion::Cancelada => tema.paleta.inactivo,
        };
        Style::default().fg(color)
    };
    Line::from(vec![
        Span::styled(
            format!(
                " {marca} {hora:<5}  {nombre:<ancho_nombre$}  {hosts:>5}  {ok:>3} {fallo:>3} {pendientes:>3}  ",
                hora = hora_corta(ejecucion.creada_en),
                nombre = acortar(nombre, ancho_nombre),
                hosts = ejecucion.hosts.len(),
            ),
            estilo,
        ),
        Span::styled(format!("{estado:<12}"), estilo_estado),
    ])
}

fn linea_host(
    host: &InfoEjecucionHost,
    nombre: &str,
    ancho_nombre: usize,
    seleccionado: bool,
    panel_activo: bool,
    ahora_ms: i64,
    tema: &Tema,
) -> Line<'static> {
    let marca = if seleccionado {
        tecla(tema, "▸", ">")
    } else {
        " "
    };
    let resaltado = seleccionado && panel_activo;
    let estilo = if resaltado {
        estilo_seleccion(tema)
    } else if seleccionado {
        Style::default()
            .fg(tema.paleta.texto)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.texto)
    };
    let estilo_glifo = if resaltado {
        estilo
    } else {
        Style::default().fg(color_host(host.estado, tema))
    };
    let codigo = host
        .codigo
        .map(|codigo| codigo.to_string())
        .unwrap_or_default();
    let duracion = duracion_host(host, ahora_ms)
        .map(duracion_legible)
        .unwrap_or_default();
    let bytes = if host.inicio_ms.is_some() {
        tamano_legible(host.bytes_stdout.saturating_add(host.bytes_stderr))
    } else {
        String::new()
    };
    let mut spans = vec![
        Span::styled(format!(" {marca} "), estilo),
        Span::styled(host.estado.glifo(tema.ascii).to_string(), estilo_glifo),
        Span::styled(
            format!(
                " {nombre:<ancho_nombre$}  {estado:<10} {codigo:>4}  {duracion:>8}  {bytes:>8}  ",
                nombre = acortar(nombre, ancho_nombre),
                estado = host.estado.texto(),
            ),
            estilo,
        ),
    ];
    if let Some(error) = &host.error {
        let estilo_error = if resaltado {
            estilo
        } else {
            Style::default().fg(tema.paleta.critico)
        };
        spans.push(Span::styled(pantalla(error), estilo_error));
    }
    Line::from(spans)
}

/// Vista previa del host seleccionado: rótulo con sus bytes y, debajo, su
/// error o cómo ver la salida.
fn lineas_previa(
    host: Option<&InfoEjecucionHost>,
    ancho: usize,
    filas: usize,
    tema: &Tema,
) -> Vec<Line<'static>> {
    let Some(host) = host else {
        return Vec::new();
    };
    let mut rotulo = format!(
        "  {} · {} · stdout {} · stderr {}",
        pantalla(&host.nombre),
        host.estado.texto(),
        tamano_legible(host.bytes_stdout),
        tamano_legible(host.bytes_stderr)
    );
    if host.truncada {
        rotulo.push_str(" · truncada a 1 MiB");
    }
    let mut lineas = vec![Line::from(Span::styled(
        rotulo,
        Style::default().fg(tema.paleta.texto),
    ))];
    let detalle = filas.saturating_sub(1);
    match &host.error {
        Some(error) => {
            for trozo in partir(&pantalla(error), ancho.saturating_sub(4).max(8))
                .into_iter()
                .take(detalle)
            {
                lineas.push(Line::from(Span::styled(
                    format!("  {trozo}"),
                    Style::default().fg(tema.paleta.critico),
                )));
            }
        }
        None => {
            let pista = if host.estado == EstadoHostEjecucion::EnCola {
                "  en cola: todavía no hay salida"
            } else {
                "  ↵ ver salida (en el panel de hosts)"
            };
            lineas.push(Line::from(Span::styled(
                pista,
                Style::default().fg(tema.paleta.inactivo),
            )));
        }
    }
    lineas.truncate(filas);
    lineas
}

// ---------------------------------------------------------------- visor

fn dibujar_visor(
    marco: &mut Frame,
    interior: Rect,
    tema: &Tema,
    estado: &EstadoResultados,
    ahora_ms: i64,
) {
    let Some(visor) = estado.visor.as_ref() else {
        return;
    };
    let host = estado.estado_host(visor.ejecucion_id, visor.host_id);
    let ancho = interior.width as usize;
    let altos = reparto_visor(interior.height as usize);
    let foco = visor.foco.indice();

    // Cabecera: snippet, host, estado y cómo va la salida (lo que más
    // importa, delante: con poco ancho se recorta por la derecha).
    let mut cabecera = vec![Span::styled(
        format!(" «{}» · {}", visor.snippet, visor.host),
        Style::default()
            .fg(tema.paleta.texto)
            .add_modifier(Modifier::BOLD),
    )];
    match host {
        Some(host) => {
            cabecera.push(Span::raw(" · "));
            cabecera.push(Span::styled(
                format!("{} {}", host.estado.glifo(tema.ascii), host.estado.texto()),
                Style::default().fg(color_host(host.estado, tema)),
            ));
            if host.estado == EstadoHostEjecucion::EnCola {
                cabecera.push(Span::styled(
                    " · en cola",
                    Style::default().fg(tema.paleta.inactivo),
                ));
            } else if !visor.completa {
                cabecera.push(Span::styled(
                    " · actualizando…",
                    Style::default().fg(tema.paleta.acento),
                ));
            }
            let mut datos = String::new();
            if let Some(codigo) = host.codigo {
                datos.push_str(&format!(" · código {codigo}"));
            }
            if let Some(duracion) = duracion_host(host, ahora_ms) {
                datos.push_str(&format!(" · {}", duracion_legible(duracion)));
            }
            cabecera.push(Span::styled(datos, Style::default().fg(tema.paleta.texto)));
        }
        None => cabecera.push(Span::styled(
            " · ya no está en el servidor",
            Style::default().fg(tema.paleta.inactivo),
        )),
    }
    let mut lineas: Vec<Line> = vec![Line::from(cabecera)];

    for (indice, rotulo) in ["stdout", "stderr"].into_iter().enumerate() {
        let alto = altos[indice];
        let total = visor.lineas[indice].len();
        let inicio = visor.desplazamiento[indice].min(total.saturating_sub(alto.max(1)));
        let horizontal = visor.horizontal[indice];
        let enfocado = foco == indice;
        let marca = if enfocado {
            tecla(tema, "▸", ">")
        } else {
            " "
        };
        let mut titulo = format!(" {marca} {rotulo} · {}", visor.host);
        if total > 0 {
            titulo.push_str(&format!(
                "  {}-{} de {}",
                inicio + 1,
                (inicio + alto).min(total),
                total
            ));
        }
        if horizontal > 0 {
            titulo.push_str(&format!(" · columna {}", horizontal + 1));
        }
        let estilo_titulo = if enfocado {
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.inactivo)
        };
        let mut rotulos = vec![Span::styled(titulo, estilo_titulo)];
        if visor.truncada {
            rotulos.push(Span::styled(
                " · salida truncada a 1 MiB",
                Style::default()
                    .fg(tema.paleta.critico)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        lineas.push(Line::from(rotulos));

        let desde = lineas.len();
        if !visor.recibida {
            lineas.push(Line::from(Span::styled(
                "   pidiendo la salida…",
                Style::default().fg(tema.paleta.inactivo),
            )));
        } else if total == 0 {
            lineas.push(Line::from(Span::styled(
                "   (sin salida)",
                Style::default().fg(tema.paleta.inactivo),
            )));
        } else {
            for linea in visor.lineas[indice].iter().skip(inicio).take(alto) {
                let visible: String = linea
                    .chars()
                    .skip(horizontal)
                    .take(ancho.saturating_sub(3))
                    .collect();
                lineas.push(Line::from(Span::styled(
                    format!("   {visible}"),
                    Style::default().fg(tema.paleta.texto),
                )));
            }
        }
        lineas.truncate(desde + alto);
        rellenar(&mut lineas, desde + alto);
    }

    lineas.push(Line::from(Span::styled(
        format!(
            " {} stdout/stderr  {} RePág AvPág Inicio Fin  {} columnas  s guardar  esc cerrar",
            tecla(tema, "⇥", "tab"),
            tecla(tema, "↑↓", "arriba/abajo"),
            tecla(tema, "← →", "izq/der"),
        ),
        Style::default().fg(tema.paleta.inactivo),
    )));
    marco.render_widget(Paragraph::new(lineas), interior);
}

#[cfg(test)]
mod pruebas {
    use std::time::Instant;

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;

    const CREADA: i64 = 1_790_000_000;

    fn host(host_id: i64, nombre: &str, estado: EstadoHostEjecucion) -> InfoEjecucionHost {
        InfoEjecucionHost {
            host_id,
            nombre: nombre.to_string(),
            estado,
            codigo: None,
            inicio_ms: None,
            duracion_ms: None,
            bytes_stdout: 0,
            bytes_stderr: 0,
            truncada: false,
            error: None,
        }
    }

    fn ejecuciones() -> Vec<InfoEjecucion> {
        let mut ok = host(1, "hetzner-01", EstadoHostEjecucion::Ok);
        ok.codigo = Some(0);
        ok.inicio_ms = Some(CREADA * 1000);
        ok.duracion_ms = Some(1_200);
        ok.bytes_stdout = 212;
        let mut fallo = host(2, "hetzner-02\u{1b}[2J\u{202E}", EstadoHostEjecucion::Fallo);
        fallo.codigo = Some(1);
        fallo.inicio_ms = Some(CREADA * 1000);
        fallo.duracion_ms = Some(800);
        fallo.bytes_stderr = 1_434;
        fallo.error = Some("Job for nginx.service failed\u{7}\n\u{1b}]0;titulo\u{7}".to_string());
        let terminada = InfoEjecucion {
            id: 2,
            peticion_id: 1,
            solicitante: 1,
            snippet_id: Some(1),
            nombre: "reiniciar nginx\u{1b}[31m".to_string(),
            hosts: vec![
                ok,
                fallo,
                host(3, "vps-openclaw", EstadoHostEjecucion::Omitido),
            ],
            timeout_seg: 60,
            parar_al_fallo: true,
            deliberacion_id: Some(4),
            forzada: true,
            estado: EstadoEjecucion::Terminada,
            creada_en: CREADA,
            terminada_en: Some(CREADA + 2),
        };
        let mut corriendo = host(4, "web-01", EstadoHostEjecucion::Ejecutando);
        corriendo.inicio_ms = Some((CREADA - 60) * 1000);
        let en_curso = InfoEjecucion {
            id: 1,
            peticion_id: 2,
            solicitante: 2,
            snippet_id: Some(2),
            nombre: "limpiar journald".to_string(),
            hosts: vec![corriendo, host(5, "web-02", EstadoHostEjecucion::EnCola)],
            timeout_seg: 60,
            parar_al_fallo: false,
            deliberacion_id: None,
            forzada: false,
            estado: EstadoEjecucion::EnCurso,
            creada_en: CREADA - 120,
            terminada_en: None,
        };
        vec![en_curso, terminada]
    }

    fn pintar(ancho: u16, alto: u16, tema: &Tema, estado: &EstadoResultados) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(ancho, alto)).unwrap();
        let ahora = Local.timestamp_opt(CREADA + 5, 0).single().unwrap();
        terminal
            .draw(|marco| dibujar_resultados(marco, marco.area(), tema, estado, ahora))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn sin_controles(filas: &[String]) {
        for fila in filas {
            assert!(
                !fila
                    .chars()
                    .any(|c| c.is_control() || es_marca_de_direccion(c)),
                "carácter de control pintado en {fila:?}"
            );
        }
    }

    #[test]
    fn vacia_invita_a_lanzar_desde_f8() {
        let estado = EstadoResultados::default();
        let todo = pintar(80, 20, &Tema::respaldo(), &estado).join("\n");
        assert!(
            todo.contains("MAGI · RESULTADOS · 0 en curso · 0 hoy"),
            "{todo}"
        );
        assert!(
            todo.contains("Sin ejecuciones: lanza un snippet desde F8"),
            "{todo}"
        );
    }

    #[test]
    fn pinta_la_maqueta_sin_caracteres_de_control() {
        let mut estado = EstadoResultados::default();
        estado.actualizar(ejecuciones(), None);
        // La más reciente primero y seleccionada.
        assert_eq!(estado.seleccionada, Some(2));
        estado.cambiar_panel();
        estado.mover(1, (5, 5));
        let tema = Tema::respaldo();
        let filas = pintar(100, 24, &tema, &estado);
        sin_controles(&filas);
        let todo = filas.join("\n");
        assert!(
            todo.contains("MAGI · RESULTADOS · 1 en curso · 2 hoy"),
            "{todo}"
        );
        assert!(todo.contains("HORA"), "{todo}");
        assert!(todo.contains("reiniciar nginx"), "{todo}");
        assert!(todo.contains("terminada ⚑"), "{todo}");
        assert!(todo.contains("limpiar journald"), "{todo}");
        assert!(todo.contains("en curso"), "{todo}");
        assert!(todo.contains(&hora_corta(CREADA)), "{todo}");
        assert!(todo.contains("● hetzner-01"), "{todo}");
        assert!(todo.contains("✕ hetzner-02"), "{todo}");
        assert!(todo.contains("1,2 s"), "{todo}");
        assert!(todo.contains("212 B"), "{todo}");
        assert!(todo.contains("– vps-openclaw"), "{todo}");
        assert!(todo.contains("Job for nginx.service failed"), "{todo}");
        // Vista previa del host seleccionado (hetzner-02).
        assert!(
            todo.contains("hetzner-02 · fallo · stdout 0 B · stderr 1 kB"),
            "{todo}"
        );

        // En ASCII, los glifos tienen su equivalente.
        let mut ascii = tema;
        ascii.ascii = true;
        let filas = pintar(100, 24, &ascii, &estado);
        sin_controles(&filas);
        let todo = filas.join("\n");
        assert!(todo.contains("terminada !"), "{todo}");
        assert!(todo.contains("x hetzner-02"), "{todo}");
        assert!(!todo.contains('✕'), "{todo}");

        // Pequeña: no revienta ni pinta controles.
        for (ancho, alto) in [(30, 8), (12, 4), (100, 3)] {
            sin_controles(&pintar(ancho, alto, &tema, &estado));
        }
    }

    #[test]
    fn el_visor_pinta_los_dos_flujos_limpios() {
        let mut estado = EstadoResultados::default();
        estado.actualizar(ejecuciones(), None);
        // La ejecución en curso, host «web-01» (en marcha).
        estado.mover(1, (5, 5));
        estado.cambiar_panel();
        assert_eq!(estado.abrir_visor(Instant::now()), Some((1, 4)));
        let tema = Tema::respaldo();

        let filas = pintar(90, 24, &tema, &estado);
        sin_controles(&filas);
        let todo = filas.join("\n");
        assert!(todo.contains("stdout · web-01"), "{todo}");
        assert!(todo.contains("stderr · web-01"), "{todo}");
        assert!(todo.contains("pidiendo la salida…"), "{todo}");

        let recepcion = estado.recibir_salida(
            1,
            4,
            b"\x1b[1;32mlimpiando\x1b[0m\t12 MB\r\n\x1b]0;x\x07hecho \xe2\x80\xae al reves\n"
                .to_vec(),
            b"aviso: \xc2\x9b2Jdisco \x9b\n".to_vec(),
            true,
            alturas_visor(24),
        );
        assert!(recepcion.aceptada);
        let filas = pintar(90, 24, &tema, &estado);
        sin_controles(&filas);
        let todo = filas.join("\n");
        assert!(todo.contains("limpiando   12 MB"), "{todo}");
        assert!(todo.contains("hecho  al reves"), "{todo}");
        // El CSI de 8 bits se quita; un byte suelto que no es UTF-8 queda como
        // «�», que no es un control.
        assert!(todo.contains("aviso: disco �"), "{todo}");
        assert!(todo.contains("salida truncada a 1 MiB"), "{todo}");
        assert!(todo.contains("actualizando…"), "{todo}");
        assert!(todo.contains("1-2 de 2"), "{todo}");

        for (ancho, alto) in [(20, 6), (8, 3)] {
            sin_controles(&pintar(ancho, alto, &tema, &estado));
        }
    }

    #[test]
    fn los_repartos_dan_filas_a_todos_los_paneles() {
        // 24 filas: 21 de interior; 15 libres para las listas.
        assert_eq!(reparto(21, 2), (2, 13, 3));
        assert_eq!(reparto(21, 40), (6, 9, 3));
        assert_eq!(alturas_paneles(24, 40), (6, 9));
        // Muy baja: sin vista previa y sin desbordar.
        let (ejecuciones, hosts, previa) = reparto(5, 10);
        assert_eq!(previa, 0);
        assert!(1 + 1 + ejecuciones + hosts <= 5);
        assert_eq!(reparto(0, 3), (0, 0, 0));
        assert_eq!(alturas_paneles(0, 3), (1, 1));
        // Visor: 21 de interior, 17 para los flujos.
        assert_eq!(reparto_visor(21), [9, 8]);
        assert_eq!(alturas_visor(24), [9, 8]);
        assert_eq!(alturas_visor(2), [1, 1]);
    }

    #[test]
    fn la_ventana_deja_la_seleccion_a_la_vista() {
        assert_eq!(ventana(0, 0, 5, 20), 0);
        assert_eq!(ventana(7, 0, 5, 20), 3);
        assert_eq!(ventana(2, 10, 5, 20), 2);
        assert_eq!(ventana(19, 3, 5, 20), 15);
        assert_eq!(ventana(1, 9, 5, 3), 0);
    }

    #[test]
    fn pantalla_quita_escapes_controles_y_marcas_de_direccion() {
        assert_eq!(pantalla("a\u{1b}[31mb\u{7}\u{202E}c\nd"), "abc d");
        assert_eq!(sin_marcas("x\u{2066}y\u{200F}"), "xy");
        assert_eq!(pantalla("ñandú"), "ñandú");
    }
}
