//! Vista Sesiones (F3 sin pestaña activa): sesiones que custodia el servidor.
//!
//! Disposición adaptable (Fase 7), derivada del área en cada pintado:
//! - columnas por prioridad; el glifo de estado y la pestaña nunca se
//!   ocultan. Como Hosts: identidad y ventanas (los datos secundarios, como la
//!   dirección) se ocultan por debajo de 80 columnas y el host (como
//!   usuario·puerto) por debajo de 60;
//! - con la vista baja (< 20 filas) el panel inferior se pliega;
//! - los atajos van en la barra inferior (`ui::barra`).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::protocolo::EstadoSesionRemota;
use crate::ui::disposicion::{
    self, Columna, Disposicion, Lista, VentanaLista, ANCHO_COLUMNAS_MINIMAS,
};
use crate::ui::hosts::{anchos_tabla, celdas};

/// Alto del panel inferior (separador, sesión seleccionada, servidor y aire).
const ALTO_PANEL: u16 = 5;
/// Cabeceras de las columnas, en orden.
const CABECERAS: [&str; 6] = ["ESTADO", "PESTAÑA", "HOST", "TIEMPO", "IDENTIDAD", "VENT."];

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let alto_panel = if disp.bajo { 0 } else { ALTO_PANEL };
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(alto_panel),
        ])
        .split(area);
    marco.render_widget(
        Paragraph::new(super::linea_marco(
            &titulo(app, area.width),
            area.width,
            &app.tema,
        )),
        trozos[0],
    );
    if app.pestanas.is_empty() {
        let tema = &app.tema;
        let aviso = if app.servidor_incompatible.is_some() || app.servidor_caido {
            let (abre, cierra) = (super::tecla(tema, "«", "\""), super::tecla(tema, "»", "\""));
            format!(
                "El servidor de sesiones no está disponible.\nRelánzalo desde el diálogo o ejecuta {abre}magi servidor parar{cierra} y vuelve a abrir."
            )
        } else {
            format!(
                "Sin sesiones abiertas. {} en un host, prefijo c o {}n{} abre una nueva.",
                tema.glifos.intro,
                super::tecla(tema, "«", "\""),
                super::tecla(tema, "»", "\""),
            )
        };
        let lineas: Vec<Line> = aviso
            .lines()
            .map(|texto| {
                Line::from(Span::styled(
                    texto.to_string(),
                    Style::default().fg(tema.paleta.inactivo),
                ))
            })
            .collect();
        // Sangría de dos columnas también en las líneas partidas.
        let zona = Rect {
            x: trozos[1].x + 2,
            width: trozos[1].width.saturating_sub(2),
            ..trozos[1]
        };
        marco.render_widget(Paragraph::new(lineas).wrap(Wrap { trim: true }), zona);
    } else {
        lista(marco, trozos[1], app, disp);
    }
    if alto_panel > 0 {
        panel_inferior(marco, trozos[2], app);
    }
}

/// Título por prioridad: «MAGI · SESIONES», el número de sesiones y el de
/// ventanas, lo que quepa.
fn titulo(app: &App, ancho: u16) -> String {
    let punto = app.tema.glifos.punto_medio;
    let ventanas: u32 = app.pestanas.iter().map(|pestaña| pestaña.ventanas).sum();
    let mut titulo = format!("MAGI {punto} SESIONES");
    for parte in [
        format!("{} sesiones", app.pestanas.len()),
        format!("{ventanas} ventanas"),
    ] {
        let candidato = format!("{titulo} {punto} {parte}");
        // Un espacio a cada lado y al menos un tramo de línea.
        if candidato.chars().count() + 3 > usize::from(ancho) {
            break;
        }
        titulo = candidato;
    }
    titulo
}

/// Columnas de la tabla para su ancho y el de la terminal.
fn columnas(ancho_vista: u16, ancho_tabla: u16) -> [Columna; 6] {
    // Los umbrales son columnas de terminal: se trasladan al ancho de la tabla.
    let margen = ancho_vista.saturating_sub(ancho_tabla);
    let desde_secundarias = ANCHO_COLUMNAS_MINIMAS.saturating_sub(margen);
    let desde_host = disposicion::ANCHO_SIN_USUARIO.saturating_sub(margen);
    [
        Columna::fija(6, 1),
        Columna::fija(18, 1),
        Columna::fija(16, 2).desde(desde_host),
        // «125 h 30 m»: sesiones de más de cuatro días sin recortar.
        Columna::fija(10, 3),
        Columna::fija(16, 4).desde(desde_secundarias),
        Columna::fija(5, 5).desde(desde_secundarias),
    ]
}

fn lista(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    // La marca de selección va delante de la tabla.
    let ancho_tabla = area.width.saturating_sub(2);
    let anchos = anchos_tabla(ancho_tabla, &columnas(area.width, ancho_tabla), 1);
    let inactivo = Style::default().fg(tema.paleta.inactivo);
    let mut cabecera = vec![Span::raw("  ")];
    cabecera.extend(celdas(
        CABECERAS
            .iter()
            .map(|texto| (texto.to_string(), inactivo))
            .collect(),
        &anchos,
        ascii,
    ));
    let visibles = usize::from(area.height.saturating_sub(1));
    let inicio = disposicion::ventana(
        app.desplazamiento_sesiones,
        app.seleccion_sesiones,
        visibles,
        app.pestanas.len(),
    );
    disp.registrar(
        Lista::Sesiones,
        VentanaLista {
            inicio,
            filas: visibles,
            total: app.pestanas.len(),
        },
    );
    let mut lineas = vec![Line::from(cabecera)];
    for (indice, pestaña) in app.pestanas.iter().enumerate().skip(inicio).take(visibles) {
        let elegida = indice == app.seleccion_sesiones;
        let marca = if elegida { tema.glifos.seleccion } else { " " };
        let (glifo, color) = match pestaña.estado {
            EstadoSesionRemota::Caida => (tema.glifos.error, tema.paleta.critico),
            EstadoSesionRemota::Abierta => (tema.glifos.conectado, tema.paleta.correcto),
            EstadoSesionRemota::Abriendo => (tema.glifos.conectando, tema.paleta.acento),
            EstadoSesionRemota::Cerrada => (tema.glifos.desconectado, tema.paleta.inactivo),
        };
        let estilo = if elegida {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let texto = estilo.fg(tema.paleta.texto);
        let tiempo = if pestaña.estado == EstadoSesionRemota::Caida {
            "caída".to_string()
        } else {
            crate::ui::sesion::formatear_duracion(pestaña.segundos())
        };
        let identidad = if pestaña.identidad.is_empty() {
            super::tecla(tema, "—", "-").to_string()
        } else {
            pestaña.identidad.clone()
        };
        let mut spans = vec![Span::styled(format!("{marca} "), estilo)];
        spans.extend(celdas(
            vec![
                (glifo.to_string(), estilo.fg(color)),
                (pestaña.nombre.clone(), texto),
                (pestaña.host_nombre.clone(), texto),
                (tiempo, texto),
                (identidad, texto),
                (pestaña.ventanas.to_string(), texto),
            ],
            &anchos,
            ascii,
        ));
        lineas.push(Line::from(spans));
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

fn panel_inferior(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    // Las líneas que no caben se recortan con marca, no a ciegas.
    let ajustar =
        |texto: String| disposicion::recortar(&texto, usize::from(area.width), tema.ascii);
    let separador = Line::from(Span::styled(
        tema.glifos.linea.repeat(usize::from(area.width)),
        Style::default().fg(tema.paleta.inactivo),
    ));
    let mut lineas = vec![separador];
    match app.pestanas.get(app.seleccion_sesiones) {
        Some(pestaña) => {
            let detalle = if pestaña.estado == EstadoSesionRemota::Caida {
                format!(
                    "{} {punto} caída: {}",
                    pestaña.nombre,
                    pestaña
                        .motivo
                        .clone()
                        .unwrap_or_else(|| "motivo desconocido".to_string())
                )
            } else {
                format!("{} {punto} {}", pestaña.nombre, pestaña.identidad)
            };
            lineas.push(Line::from(Span::styled(
                ajustar(format!("  {detalle}")),
                Style::default().fg(tema.paleta.texto),
            )));
        }
        None => lineas.push(Line::from(Span::styled(
            "  Selecciona una sesión de la lista.",
            Style::default().fg(tema.paleta.inactivo),
        ))),
    }
    let servidor = match (app.servidor_pid, app.servidor_desde()) {
        (Some(pid), Some(desde)) => format!(
            "  servidor pid {pid} {punto} protocolo {} {punto} desde hace {}",
            crate::protocolo::VERSION_PROTOCOLO,
            crate::ui::sesion::formatear_duracion(desde.elapsed().as_secs()),
        ),
        (Some(pid), None) => format!(
            "  servidor pid {pid} {punto} protocolo {}",
            crate::protocolo::VERSION_PROTOCOLO
        ),
        _ => "  sin servidor de sesiones".to_string(),
    };
    lineas.push(Line::from(Span::styled(
        ajustar(servidor),
        Style::default().fg(tema.paleta.inactivo),
    )));
    marco.render_widget(Paragraph::new(lineas), area);
}
