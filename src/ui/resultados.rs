//! Vista Resultados (subvista de F8, maqueta §6.4): las ejecuciones que
//! difunde el servidor (panel superior), los hosts de la seleccionada con su
//! estado y una vista previa del host seleccionado; con `↵`, el visor de
//! salida del host (stdout y stderr separados y desplazables).
//!
//! Nada que venga del remoto llega a la terminal sin limpiar: la salida se
//! pinta con las líneas limpias que el estado calcula al recibirla (sin
//! secuencias de escape ni caracteres de control) y los nombres y errores
//! pasan por `pantalla`.
//!
//! Disposición (Fase 7): los paneles de ejecuciones y de hosts van siempre
//! apilados; con la terminal de menos de 24 filas la vista previa de la
//! salida desaparece y la salida solo se ve con `↵` (el visor). Las filas de
//! cada lista se derivan del área en cada pintado y se registran en la
//! `Disposicion`, que es lo que usan las teclas de página.

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
use crate::ui::disposicion::{
    self, recortar, Disposicion, Lista, VentanaLista, ALTO_SALIDA_RESULTADOS,
};
use crate::ui::ejecutar::pie_por_prioridad;
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

/// Reparto del interior: filas de ejecuciones, de hosts y de vista previa.
/// Las ejecuciones se quedan como mucho con dos quintos de lo libre. Sin
/// `con_salida` (terminal baja) no hay vista previa: la salida, con `↵`.
fn reparto(interior: usize, ejecuciones: usize, con_salida: bool) -> (usize, usize, usize) {
    let previa = if con_salida && interior >= 12 {
        ALTO_PREVIA
    } else {
        0
    };
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

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    dibujar_resultados(marco, area, &app.tema, &app.resultados, Local::now(), disp);
}

/// La vista entera con un estado y un reloj dados (lo usan las pruebas).
/// Registra en `disp` las ventanas de las listas que pinta.
pub fn dibujar_resultados(
    marco: &mut Frame,
    area: Rect,
    tema: &Tema,
    estado: &EstadoResultados,
    ahora: DateTime<Local>,
    disp: &mut Disposicion,
) {
    let punto = tema.glifos.punto_medio;
    let titulo = format!(
        "MAGI {punto} RESULTADOS {punto} {} en curso {punto} {} hoy",
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
    // La vista previa de la salida solo con la terminal alta (§7.2).
    let con_salida = disp.area.height >= ALTO_SALIDA_RESULTADOS;
    if estado.visor.is_some() {
        dibujar_visor(marco, interior, tema, estado, ahora_ms, disp);
    } else if estado.ejecuciones.is_empty() {
        dibujar_vacia(marco, interior, tema);
    } else {
        dibujar_paneles(marco, interior, tema, estado, ahora_ms, con_salida, disp);
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
    con_salida: bool,
    disp: &mut Disposicion,
) {
    let (alto_ejecuciones, alto_hosts, alto_previa) = reparto(
        interior.height as usize,
        estado.ejecuciones.len(),
        con_salida,
    );
    let ancho = interior.width as usize;
    let ascii = tema.ascii;
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
    let inicio = disposicion::ventana(
        estado.desplazamiento_ejecuciones,
        seleccion.unwrap_or(0),
        alto_ejecuciones,
        estado.ejecuciones.len(),
    );
    disp.registrar(
        Lista::ResultadosEjecuciones,
        VentanaLista {
            inicio,
            filas: alto_ejecuciones.max(1),
            total: estado.ejecuciones.len(),
        },
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
            "HOSTS {} {}",
            tema.glifos.punto_medio,
            recortar(
                &pantalla(&ejecucion.nombre),
                ancho.saturating_sub(16).max(4),
                ascii
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
        let inicio = disposicion::ventana(
            estado.desplazamiento_hosts,
            estado.host_seleccionado,
            alto_hosts,
            ejecucion.hosts.len(),
        );
        disp.registrar(
            Lista::ResultadosHosts,
            VentanaLista {
                inicio,
                filas: alto_hosts.max(1),
                total: ejecucion.hosts.len(),
            },
        );
        for (posicion, host) in ejecucion
            .hosts
            .iter()
            .enumerate()
            .skip(inicio)
            .take(alto_hosts)
        {
            let linea = linea_host(
                host,
                &nombres_hosts[posicion],
                ancho_host,
                posicion == estado.host_seleccionado,
                panel_hosts,
                ahora_ms,
                tema,
            );
            // El error va al final: si no cabe, se recorta con su marca.
            lineas.push(recortar_linea(linea, ancho, ascii));
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

/// Recorta una línea a `ancho` columnas; si sobra, el último carácter que
/// cabe pasa a ser la marca de recorte (`…`, `~` en ASCII).
fn recortar_linea(linea: Line<'static>, ancho: usize, ascii: bool) -> Line<'static> {
    let total: usize = linea
        .spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum();
    if total <= ancho {
        return linea;
    }
    let mut quedan = ancho;
    let mut spans = Vec::with_capacity(linea.spans.len());
    for span in linea.spans {
        if quedan == 0 {
            break;
        }
        let largo = span.content.chars().count();
        if largo < quedan {
            quedan -= largo;
            spans.push(span);
            continue;
        }
        // Aquí se pasa del ancho (lo que sigue no cabe): la marca va en la
        // última columna.
        let marca = if ascii { '~' } else { '…' };
        let mut texto: String = span.content.chars().take(quedan - 1).collect();
        texto.push(marca);
        spans.push(Span::styled(texto, span.style));
        quedan = 0;
    }
    Line::from(spans)
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
                nombre = recortar(nombre, ancho_nombre, tema.ascii),
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
                nombre = recortar(nombre, ancho_nombre, tema.ascii),
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
    let punto = tema.glifos.punto_medio;
    let mut rotulo = format!(
        "  {} {punto} {} {punto} stdout {} {punto} stderr {}",
        pantalla(&host.nombre),
        host.estado.texto(),
        tamano_legible(host.bytes_stdout),
        tamano_legible(host.bytes_stderr)
    );
    if host.truncada {
        rotulo.push_str(&format!(" {punto} truncada a 1 MiB"));
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
                "  en cola: todavía no hay salida".to_string()
            } else {
                format!("  {} ver salida (en el panel de hosts)", tema.glifos.intro)
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
    disp: &mut Disposicion,
) {
    let Some(visor) = estado.visor.as_ref() else {
        return;
    };
    let host = estado.estado_host(visor.ejecucion_id, visor.host_id);
    let ancho = interior.width as usize;
    let altos = reparto_visor(interior.height as usize);
    let foco = visor.foco.indice();
    let punto = tema.glifos.punto_medio;
    let puntos = tema.glifos.puntos;
    let (abre, cierra) = if tema.ascii {
        ("\"", "\"")
    } else {
        ("«", "»")
    };

    // Cabecera: snippet, host, estado y cómo va la salida (lo que más
    // importa, delante: con poco ancho se recorta por la derecha).
    let mut cabecera = vec![Span::styled(
        format!(" {abre}{}{cierra} {punto} {}", visor.snippet, visor.host),
        Style::default()
            .fg(tema.paleta.texto)
            .add_modifier(Modifier::BOLD),
    )];
    match host {
        Some(host) => {
            cabecera.push(Span::raw(format!(" {punto} ")));
            cabecera.push(Span::styled(
                format!("{} {}", host.estado.glifo(tema.ascii), host.estado.texto()),
                Style::default().fg(color_host(host.estado, tema)),
            ));
            if host.estado == EstadoHostEjecucion::EnCola {
                cabecera.push(Span::styled(
                    format!(" {punto} en cola"),
                    Style::default().fg(tema.paleta.inactivo),
                ));
            } else if !visor.completa {
                cabecera.push(Span::styled(
                    format!(" {punto} actualizando{puntos}"),
                    Style::default().fg(tema.paleta.acento),
                ));
            }
            let mut datos = String::new();
            if let Some(codigo) = host.codigo {
                datos.push_str(&format!(" {punto} código {codigo}"));
            }
            if let Some(duracion) = duracion_host(host, ahora_ms) {
                datos.push_str(&format!(" {punto} {}", duracion_legible(duracion)));
            }
            cabecera.push(Span::styled(datos, Style::default().fg(tema.paleta.texto)));
        }
        None => cabecera.push(Span::styled(
            format!(" {punto} ya no está en el servidor"),
            Style::default().fg(tema.paleta.inactivo),
        )),
    }
    let mut lineas: Vec<Line> = vec![Line::from(cabecera)];

    let listas = [Lista::VisorSalida, Lista::VisorErrores];
    for (indice, rotulo) in ["stdout", "stderr"].into_iter().enumerate() {
        let alto = altos[indice];
        let total = visor.lineas[indice].len();
        // Siguiendo el final (`tail -f`), la última línea queda en la última
        // fila aunque el alto haya cambiado desde que se desplazó.
        let maximo = total.saturating_sub(alto.max(1));
        let inicio = if visor.siguiendo[indice] {
            maximo
        } else {
            visor.desplazamiento[indice].min(maximo)
        };
        disp.registrar(
            listas[indice],
            VentanaLista {
                inicio,
                filas: alto.max(1),
                total,
            },
        );
        let horizontal = visor.horizontal[indice];
        let enfocado = foco == indice;
        let marca = if enfocado { tema.glifos.seleccion } else { " " };
        let mut titulo = format!(" {marca} {rotulo} {punto} {}", visor.host);
        if total > 0 {
            titulo.push_str(&format!(
                "  {}-{} de {}",
                inicio + 1,
                (inicio + alto).min(total),
                total
            ));
        }
        if horizontal > 0 {
            titulo.push_str(&format!(" {punto} columna {}", horizontal + 1));
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
                format!(" {punto} salida truncada a 1 MiB"),
                Style::default()
                    .fg(tema.paleta.critico)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        lineas.push(Line::from(rotulos));

        let desde = lineas.len();
        if !visor.recibida {
            lineas.push(Line::from(Span::styled(
                format!("   pidiendo la salida{puntos}"),
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

    // Pie por prioridad: con poco ancho quedan las teclas de salir y de
    // cambiar de flujo.
    let atajos = [
        disposicion::Atajo::new(tema.glifos.tab, "stdout/stderr", 1),
        disposicion::Atajo::new(
            tecla(tema, "↑↓", "arriba/abajo"),
            "RePág AvPág Inicio Fin",
            3,
        ),
        disposicion::Atajo::new(tecla(tema, "← →", "izq/der"), "columnas", 4),
        disposicion::Atajo::new("s", "guardar", 2),
        disposicion::Atajo::new("esc", "cerrar", 1),
    ];
    let mut pie = pie_por_prioridad(&atajos, ancho.saturating_sub(1), tema);
    pie.spans.insert(0, Span::raw(" "));
    lineas.push(pie);
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

    /// Pinta la vista en una terminal de `ancho`×`alto` y devuelve las filas
    /// y la disposición que registró.
    fn pintar_con(
        ancho: u16,
        alto: u16,
        tema: &Tema,
        estado: &EstadoResultados,
    ) -> (Vec<String>, Disposicion) {
        let mut terminal = Terminal::new(TestBackend::new(ancho, alto)).unwrap();
        let ahora = Local.timestamp_opt(CREADA + 5, 0).single().unwrap();
        let mut disp = Disposicion::default();
        terminal
            .draw(|marco| {
                let area = marco.area();
                disp = Disposicion::nueva(
                    area,
                    disposicion::minimo_de(crate::ui::Vista::Resultados, false),
                );
                dibujar_resultados(marco, area, tema, estado, ahora, &mut disp);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let filas = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect();
        (filas, disp)
    }

    fn pintar(ancho: u16, alto: u16, tema: &Tema, estado: &EstadoResultados) -> Vec<String> {
        pintar_con(ancho, alto, tema, estado).0
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

        let (filas, disp) = pintar_con(90, 24, &tema, &estado);
        sin_controles(&filas);
        let todo = filas.join("\n");
        assert!(todo.contains("stdout · web-01"), "{todo}");
        assert!(todo.contains("stderr · web-01"), "{todo}");
        assert!(todo.contains("pidiendo la salida…"), "{todo}");
        // 24 filas: 22 de interior (sin barra), 18 para los flujos.
        let altos = [
            disp.filas(Lista::VisorSalida),
            disp.filas(Lista::VisorErrores),
        ];
        assert_eq!(altos, [9, 9]);

        let recepcion = estado.recibir_salida(
            1,
            4,
            b"\x1b[1;32mlimpiando\x1b[0m\t12 MB\r\n\x1b]0;x\x07hecho \xe2\x80\xae al reves\n"
                .to_vec(),
            b"aviso: \xc2\x9b2Jdisco \x9b\n".to_vec(),
            true,
            altos,
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
        // 21 de interior: 15 libres para las listas con vista previa.
        assert_eq!(reparto(21, 2, true), (2, 13, 3));
        assert_eq!(reparto(21, 40, true), (6, 9, 3));
        // Sin salida (terminal baja): las filas de la vista previa y su
        // separador van a las listas.
        assert_eq!(reparto(21, 40, false), (7, 12, 0));
        // Muy baja: sin vista previa y sin desbordar.
        let (ejecuciones, hosts, previa) = reparto(5, 10, true);
        assert_eq!(previa, 0);
        assert!(1 + 1 + ejecuciones + hosts <= 5);
        assert_eq!(reparto(0, 3, true), (0, 0, 0));
        // Visor: 21 de interior, 17 para los flujos.
        assert_eq!(reparto_visor(21), [9, 8]);
        assert_eq!(reparto_visor(2), [0, 0]);
    }

    /// Con alto < 24 la salida solo se ve con `↵`: sin vista previa, y las
    /// listas registran las filas con las que se pintaron.
    #[test]
    fn con_la_terminal_baja_la_salida_solo_con_intro() {
        let mut estado = EstadoResultados::default();
        estado.actualizar(ejecuciones(), None);
        estado.cambiar_panel();
        estado.mover(1, (5, 5));
        let tema = Tema::respaldo();
        let (filas, disp) = pintar_con(100, 23, &tema, &estado);
        let todo = filas.join("\n");
        assert!(!todo.contains("stdout 0 B"), "{todo}");
        // 21 de interior: cabecera y separador; 19 para las listas.
        let ejecuciones = disp.lista(Lista::ResultadosEjecuciones).unwrap();
        let hosts = disp.lista(Lista::ResultadosHosts).unwrap();
        assert_eq!((ejecuciones.filas, ejecuciones.total), (2, 2));
        assert_eq!((hosts.filas, hosts.total), (17, 3));
        let (filas, _) = pintar_con(100, 24, &tema, &estado);
        assert!(filas.join("\n").contains("stdout 0 B"));
    }

    /// La ventana registrada deja la selección a la vista aunque el
    /// desplazamiento guardado venga de otro alto.
    #[test]
    fn la_ventana_registrada_deja_la_seleccion_a_la_vista() {
        let mut lista = Vec::new();
        for id in 1..=30u32 {
            let mut ejecucion = ejecuciones().remove(1);
            ejecucion.id = id;
            lista.push(ejecucion);
        }
        let mut estado = EstadoResultados::default();
        estado.actualizar(lista, None);
        // La más reciente primero: la 30. Baja a la posición 25 con una
        // altura de 40 filas (desplazamiento 0).
        estado.mover(25, (40, 40));
        assert_eq!(estado.desplazamiento_ejecuciones, 0);
        let (_, disp) = pintar_con(80, 24, &Tema::respaldo(), &estado);
        let ventana = disp.lista(Lista::ResultadosEjecuciones).unwrap();
        assert!(ventana.inicio <= 25 && 25 < ventana.inicio + ventana.filas);
        assert!(ventana.inicio <= ventana.total - ventana.filas);
    }

    #[test]
    fn pantalla_quita_escapes_controles_y_marcas_de_direccion() {
        assert_eq!(pantalla("a\u{1b}[31mb\u{7}\u{202E}c\nd"), "abc d");
        assert_eq!(sin_marcas("x\u{2066}y\u{200F}"), "xy");
        assert_eq!(pantalla("ñandú"), "ñandú");
    }
}
