//! Vista Identidades (F5): las claves de `IDENTIDADES` con el detalle de la
//! seleccionada al pie.
//!
//! Disposición adaptable (Fase 7): columnas por prioridad (el alias no se
//! oculta nunca) y detalle inferior plegado con la vista baja; `↵` abre el
//! detalle completo en diálogo.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::flota::estado::{antiguedad_segundos, formatear_antiguedad};
use crate::modelo::Identidad;
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};
use crate::ui::snippets::{limpio, partir};
use crate::ui::tuneles::{
    estilo_fila, marco_detalle, prefijo, repartir_vista, texto_plegado, Tabla, ANCHO_PREFIJO,
};

/// Filas del detalle inferior, bordes incluidos.
pub const ALTO_DETALLE: u16 = 8;

/// Ancho de la columna de rótulos del detalle.
const ANCHO_ROTULO: usize = 10;

/// Ancho útil de las líneas del diálogo de detalle (74 de ancho menos bordes y
/// márgenes, como el detalle del registro).
const ANCHO_DIALOGO: usize = 68;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    let visibles = app.identidades_visibles();
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            format!(" MAGI {punto} IDENTIDADES "),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    let interior = bloque.inner(area);
    let (zona, detalle) = repartir_vista(interior, ALTO_DETALLE, disp);
    let seleccionada = app.identidad_seleccionada();
    // Pie: «↵ detalle» a la izquierda si el detalle está plegado y el recuento
    // a la derecha. El título de la derecha taparía al de la izquierda si no
    // caben los dos: el primero en caer es el recordatorio de `v`, que la
    // barra ya enseña.
    let plegado = (detalle.is_none() && seleccionada.is_some())
        .then(|| format!(" {} ", texto_plegado(tema, tema.glifos.intro)));
    let mut recuento = format!("{} clave(s)", visibles.len());
    if app.ver_revocadas {
        recuento.push_str(&format!(" {punto} con revocadas"));
    }
    let recordatorio = format!(" {punto} v revocadas");
    // Lo que ocupan el título de la izquierda con su separación y el de la
    // derecha con el recordatorio y su espacio final, frente al ancho del
    // borde sin las esquinas.
    let izquierda = plegado
        .as_deref()
        .map_or(0, |texto| texto.chars().count() + 1);
    let derecha = recuento.chars().count() + recordatorio.chars().count() + 1;
    if app.identidades_bd.iter().any(Identidad::revocada)
        && izquierda + derecha <= usize::from(area.width.saturating_sub(2))
    {
        recuento.push_str(&recordatorio);
    }
    let mut bloque = bloque.title_bottom(
        Line::from(Span::styled(
            format!("{recuento} "),
            Style::default().fg(tema.paleta.inactivo),
        ))
        .alignment(Alignment::Right),
    );
    if let Some(plegado) = plegado {
        bloque = bloque.title_bottom(Span::styled(
            plegado,
            Style::default().fg(tema.paleta.inactivo),
        ));
    }
    marco.render_widget(bloque, area);
    if interior.height == 0 {
        return;
    }

    dibujar_tabla(marco, zona, app, &visibles, disp);
    if let (Some(detalle), Some(identidad)) = (detalle, seleccionada) {
        dibujar_detalle(marco, detalle, app, identidad);
    }
}

/// Columnas: alias, tipo, «usada en» y origen. El alias es el identificador y
/// no se oculta (pide su largo, hasta 24); el origen es lo primero que cae.
/// Lo que sobre va al tipo, que es el que se recorta (`sk-ssh-ed25519@…`).
fn columnas(ancho_alias: u16, ancho_usos: u16) -> [Columna; 4] {
    [
        Columna::fija(ancho_alias, 1),
        Columna::flexible(14, 3),
        Columna::fija(ancho_usos, 2),
        Columna::fija(7, 4),
    ]
}

fn dibujar_tabla(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    visibles: &[&Identidad],
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let filas = usize::from(area.height).saturating_sub(1);
    let inicio = disposicion::ventana(
        app.desplazamiento_identidades,
        app.seleccion_identidad,
        filas,
        visibles.len(),
    );
    disp.registrar(
        Lista::Identidades,
        VentanaLista {
            inicio,
            filas,
            total: visibles.len(),
        },
    );
    if area.height == 0 {
        return;
    }

    let alias: Vec<String> = visibles
        .iter()
        .map(|identidad| alias_de(identidad))
        .collect();
    let usos: Vec<String> = visibles
        .iter()
        .map(|identidad| texto_usos(app, identidad))
        .collect();
    let largo_alias = alias
        .iter()
        .map(|alias| alias.chars().count())
        .max()
        .unwrap_or(5)
        .max(5) as u16;
    let ancho_usos = usos
        .iter()
        .map(|usos| usos.chars().count())
        .max()
        .unwrap_or(0)
        .max("USADA EN".len()) as u16;
    // El tipo se ve entero si cabe todo; si no, se queda en 14 recortado.
    let largo_tipo = visibles
        .iter()
        .map(|identidad| identidad.tipo.chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 26) as u16;
    let columnas = columnas(largo_alias.min(24), ancho_usos);
    let naturales = [largo_alias, largo_tipo, ancho_usos, 7];
    let tabla = Tabla::nueva(
        area.width.saturating_sub(ANCHO_PREFIJO),
        &columnas,
        &naturales,
    );

    let mut lineas = vec![tabla.linea(
        prefijo(false, tema),
        ["ALIAS", "TIPO", "USADA EN", "ORIGEN"]
            .iter()
            .map(|texto| (texto.to_string(), Style::default()))
            .collect(),
        Style::default().fg(tema.paleta.inactivo),
        ascii,
    )];
    if visibles.is_empty() {
        lineas.push(Line::from(Span::styled(
            disposicion::recortar(
                "  no hay identidades; pulsa s para reescanear o n para generar una",
                usize::from(area.width),
                ascii,
            ),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    for (posicion, identidad) in visibles.iter().enumerate().skip(inicio).take(filas) {
        let seleccionada = posicion == app.seleccion_identidad;
        let base = estilo_fila(tema, seleccionada);
        let color = |color: Color| {
            if seleccionada {
                base
            } else {
                Style::default().fg(color)
            }
        };
        let (color_alias, color_tipo) = if identidad.revocada() {
            (tema.paleta.inactivo, tema.paleta.inactivo)
        } else {
            (tema.paleta.texto, tema.paleta.acento)
        };
        lineas.push(tabla.linea(
            prefijo(seleccionada, tema),
            vec![
                (alias[posicion].clone(), color(color_alias)),
                (limpio(&identidad.tipo), color(color_tipo)),
                (usos[posicion].clone(), color(tema.paleta.texto)),
                (
                    identidad.origen.como_texto().to_string(),
                    color(tema.paleta.inactivo),
                ),
            ],
            base,
            ascii,
        ));
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Alias con la marca de revocada.
fn alias_de(identidad: &Identidad) -> String {
    let alias = limpio(&identidad.alias);
    if identidad.revocada() {
        format!("{alias} (revocada)")
    } else {
        alias
    }
}

/// «2 host(s)» o «—».
fn texto_usos(app: &App, identidad: &Identidad) -> String {
    let usos = app
        .uso_identidades
        .get(&identidad.id)
        .map(Vec::len)
        .unwrap_or(0);
    if usos == 0 {
        "—".to_string()
    } else {
        format!("{usos} host(s)")
    }
}

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, identidad: &Identidad) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let interior = marco_detalle(marco, area, tema);
    if interior.height == 0 {
        return;
    }
    let ancho = usize::from(interior.width.saturating_sub(2));
    let ancho_valor = ancho.saturating_sub(2 + ANCHO_ROTULO);
    let recorte = |texto: &str, ancho: usize| {
        disposicion::recortar(&disposicion::adaptar(texto, ascii), ancho, ascii)
    };

    // «alias · tipo · revocada», recortado por el final.
    let alias = recorte(&limpio(&identidad.alias), ancho);
    let revocada = if identidad.revocada() {
        disposicion::adaptar(" · revocada", ascii)
    } else {
        String::new()
    };
    let resto = ancho.saturating_sub(alias.chars().count() + revocada.chars().count());
    let tipo = recorte(&format!(" · {}", limpio(&identidad.tipo)), resto);
    let mut lineas = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(
            alias,
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(tipo, Style::default().fg(tema.paleta.inactivo)),
        Span::styled(revocada, Style::default().fg(tema.paleta.critico)),
    ])];
    for (rotulo, valor, critico) in campos(app, identidad, false) {
        lineas.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{rotulo:<ANCHO_ROTULO$}"),
                Style::default().fg(Color::Reset),
            ),
            Span::styled(
                recorte(&valor, ancho_valor),
                Style::default().fg(if critico {
                    tema.paleta.critico
                } else {
                    tema.paleta.texto
                }),
            ),
        ]));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Campos del detalle (rótulo, valor, en rojo): los mismos en el panel y en
/// el diálogo; el diálogo lleva además las columnas de la tabla y todos los
/// hosts.
fn campos(app: &App, identidad: &Identidad, completo: bool) -> Vec<(&'static str, String, bool)> {
    let mut campos = Vec::new();
    if completo {
        campos.push(("Tipo", limpio(&identidad.tipo), false));
        campos.push(("Origen", identidad.origen.como_texto().to_string(), false));
    }
    campos.push(("Huella", limpio(&identidad.huella), false));
    campos.push(("Vista", vista_de(identidad), false));
    let fichero = identidad
        .ruta
        .as_deref()
        .map(limpio)
        .unwrap_or_else(|| "— (solo en el agente)".to_string());
    campos.push(("Fichero", fichero, false));
    let nombres = app.uso_identidades.get(&identidad.id);
    let hosts = match nombres {
        Some(nombres) if !nombres.is_empty() => {
            if completo {
                nombres
                    .iter()
                    .map(|nombre| limpio(nombre))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                resumir_hosts(nombres)
            }
        }
        _ => "—".to_string(),
    };
    campos.push(("Hosts", hosts, false));
    if !app.huellas_escaneadas.contains(&identidad.huella) {
        campos.push(("", "no encontrada en el último escaneo".to_string(), true));
    }
    campos
}

/// Título del diálogo de detalle (`↵`).
pub fn titulo_detalle(identidad: &Identidad, ascii: bool) -> String {
    disposicion::adaptar(&format!("IDENTIDAD · {}", limpio(&identidad.alias)), ascii)
}

/// Líneas del diálogo de detalle (`↵`): todo lo del panel inferior más las
/// columnas de la tabla, con los hosts completos partidos a lo ancho del
/// diálogo (que no parte líneas).
pub fn lineas_detalle(app: &App, identidad: &Identidad, ascii: bool) -> Vec<String> {
    let mut lineas = Vec::new();
    let mut cabecera = limpio(&identidad.alias);
    if identidad.revocada() {
        cabecera.push_str(" · revocada");
    }
    lineas.push(cabecera);
    lineas.push(String::new());
    let mut campos = campos(app, identidad, true);
    // «Usada en» tras el origen, como en la tabla.
    campos.insert(2, ("Usada en", texto_usos(app, identidad), false));
    for (rotulo, valor, _) in campos {
        for (indice, trozo) in partir(&valor, ANCHO_DIALOGO - ANCHO_ROTULO)
            .into_iter()
            .enumerate()
        {
            let rotulo = if indice == 0 { rotulo } else { "" };
            lineas.push(format!("{rotulo:<ANCHO_ROTULO$}{trozo}"));
        }
    }
    lineas
        .into_iter()
        .map(|linea| disposicion::adaptar(&linea, ascii))
        .collect()
}

fn vista_de(identidad: &Identidad) -> String {
    let anadida = fecha_corta(&identidad.anadida_en);
    match &identidad.ultimo_uso_en {
        Some(uso) => {
            let hace = antiguedad_segundos(uso)
                .map(formatear_antiguedad)
                .unwrap_or_else(|| "—".to_string());
            format!("{anadida} · usada hace {hace}")
        }
        None => format!("{anadida} · sin uso"),
    }
}

fn resumir_hosts(nombres: &[String]) -> String {
    const MAXIMO: usize = 4;
    let nombres: Vec<String> = nombres.iter().map(|nombre| limpio(nombre)).collect();
    if nombres.len() <= MAXIMO {
        return nombres.join(", ");
    }
    format!(
        "{} +{}",
        nombres[..MAXIMO].join(", "),
        nombres.len() - MAXIMO
    )
}

fn fecha_corta(fecha: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(fecha) {
        Ok(momento) => momento.format("%d/%m/%y").to_string(),
        Err(_) => fecha.to_string(),
    }
}
