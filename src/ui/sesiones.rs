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
/// La marca de «sesión persistente» y, sola, la de una pestaña soltada. En
/// ASCII la degrada `disposicion::texto_ascii` (`P`), como todo glifo.
const MARCA_PERSISTENTE: &str = "◆";
/// Cabeceras de las columnas, en orden.
const CABECERAS: [&str; 8] = [
    "ESTADO",
    "PESTAÑA",
    "TIPO",
    "HOST",
    "TIEMPO",
    "TÍTULO",
    "IDENTIDAD",
    "VENT.",
];

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
    // Sin servidor no hay lista de sesiones persistentes: solo el aviso (o las
    // pestañas que queden).
    if app.servidor_caido || app.servidor_incompatible.is_some() {
        if app.pestanas.is_empty() {
            let tema = &app.tema;
            let aviso = if let Some(motivo) = crate::app::servidor_anterior::motivo_sin_servidor(
                app.servidor_incompatible.as_ref(),
            ) {
                format!("El servidor de sesiones no está disponible: {motivo}.")
            } else {
                let (abre, cierra) = (super::tecla(tema, "«", "\""), super::tecla(tema, "»", "\""));
                format!(
                    "El servidor de sesiones no está disponible.\nRelánzalo desde el diálogo o ejecuta {abre}magi servidor parar{cierra} y vuelve a abrir."
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
            lista(marco, trozos[1], app, disp, true);
        }
    } else {
        bloques(marco, trozos[1], app, disp);
    }
    if alto_panel > 0 {
        panel_inferior(marco, trozos[2], app);
    }
}

/// Reparte el alto entre el bloque «Persistentes» (arriba) y el de «Pestañas».
/// Cada uno pide su cabecera, la de sus columnas y sus filas (las pestañas,
/// hasta seis); mientras quepan los dos, «Persistentes» se queda con lo que
/// necesita y «Pestañas» con el resto. Si no caben, «Pestañas» cede: el bloque
/// de arriba conserva al menos cabecera, columnas y dos filas.
fn repartir_alto(total: u16, filas: usize, pestanas: usize) -> (u16, u16) {
    let minimo_persistentes = 4.min(total);
    let pide = |filas: usize, vacio: u16| {
        if filas == 0 {
            vacio
        } else {
            2 + u16::try_from(filas).unwrap_or(u16::MAX)
        }
    };
    let quiere_persistentes = pide(filas, 2);
    let quiere_pestanas = pide(pestanas.min(6), 2);
    if quiere_persistentes + quiere_pestanas <= total {
        return (quiere_persistentes, total - quiere_persistentes);
    }
    let pestanas = quiere_pestanas.min(total - minimo_persistentes);
    (total - pestanas, pestanas)
}

/// Los dos bloques de la vista: «Persistentes» arriba y «Pestañas» debajo.
fn bloques(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let filas = app.filas_persistentes().len();
    let (alto_persistentes, alto_pestanas) = repartir_alto(area.height, filas, app.pestanas.len());
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(alto_persistentes),
            Constraint::Length(alto_pestanas),
        ])
        .split(area);
    super::persistentes::dibujar_bloque(marco, trozos[0], app, disp);
    if alto_pestanas > 0 {
        bloque_pestanas(marco, trozos[1], app, disp);
    }
}

/// El bloque «Pestañas»: su cabecera y la tabla de siempre (o, sin pestañas,
/// cómo abrir una).
fn bloque_pestanas(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let enfocado = !app.persistentes_enfocado();
    let punto = tema.glifos.punto_medio;
    let cabecera = super::persistentes::cabecera_de_bloque(
        &format!("PESTAÑAS {punto} {}", app.pestanas.len()),
        None,
        area.width,
        enfocado,
        tema,
    );
    marco.render_widget(Paragraph::new(cabecera), Rect { height: 1, ..area });
    if area.height <= 1 {
        return;
    }
    let resto = Rect {
        y: area.y + 1,
        height: area.height - 1,
        ..area
    };
    if app.pestanas.is_empty() {
        let aviso = format!(
            "Sin sesiones abiertas. {} en un host, prefijo c o {}n{} abre una nueva.",
            tema.glifos.intro,
            super::tecla(tema, "«", "\""),
            super::tecla(tema, "»", "\""),
        );
        let zona = Rect {
            x: resto.x + 2,
            width: resto.width.saturating_sub(2),
            ..resto
        };
        marco.render_widget(
            Paragraph::new(Line::from(Span::styled(
                aviso,
                Style::default().fg(tema.paleta.inactivo),
            )))
            .wrap(Wrap { trim: true }),
            zona,
        );
    } else {
        lista(marco, resto, app, disp, enfocado);
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
fn columnas(ancho_vista: u16, ancho_tabla: u16) -> [Columna; 8] {
    // Los umbrales son columnas de terminal: se trasladan al ancho de la tabla.
    let margen = ancho_vista.saturating_sub(ancho_tabla);
    let desde_secundarias = ANCHO_COLUMNAS_MINIMAS.saturating_sub(margen);
    let desde_host = disposicion::ANCHO_SIN_USUARIO.saturating_sub(margen);
    [
        Columna::fija(6, 1),
        Columna::fija(18, 1),
        // El tipo se ve siempre (Fase 12: SSH, RDP, VNC o voxl).
        Columna::fija(5, 1),
        Columna::fija(16, 2).desde(desde_host),
        // «125 h 30 m»: sesiones de más de cuatro días sin recortar.
        Columna::fija(10, 3),
        // El título que pone el programa remoto (OSC 0 y 2): solo en ventanas
        // anchas, para no quitarle sitio a tiempo, identidad y ventanas.
        Columna::flexible(20, 4).desde(100u16.saturating_sub(margen)),
        Columna::fija(16, 4).desde(desde_secundarias),
        Columna::fija(5, 5).desde(desde_secundarias),
    ]
}

fn lista(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion, enfocado: bool) {
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
        let marca = if elegida && enfocado {
            tema.glifos.seleccion
        } else {
            " "
        };
        let soltada = pestaña.soltada().is_some();
        let (glifo, color) = if pestaña.reconexion.is_some() {
            (tema.glifos.reconectando, tema.paleta.acento)
        } else if soltada {
            // Soltada (Fase 16): otro equipo se llevó la sesión. El servidor
            // deja la pestaña «caída», pero no es un fallo: como en la vista
            // Sesión, solo la marca de persistente y el acento, sin aspa.
            (MARCA_PERSISTENTE, tema.paleta.acento)
        } else {
            match pestaña.estado {
                EstadoSesionRemota::Caida => (tema.glifos.error, tema.paleta.critico),
                EstadoSesionRemota::Abierta => (tema.glifos.conectado, tema.paleta.correcto),
                EstadoSesionRemota::Abriendo => (tema.glifos.conectando, tema.paleta.acento),
                EstadoSesionRemota::Cerrada => (tema.glifos.desconectado, tema.paleta.inactivo),
            }
        };
        // Los avisos sin ver y las sesiones persistentes (`◆`, `P` en ASCII) se
        // marcan con texto.
        // La soltada no tiene conexión: lleva solo la marca de persistente.
        let mut glifo = disposicion::adaptar(glifo, ascii);
        if !soltada && pestaña.sesion_persistente().is_some() {
            glifo.push(' ');
            glifo.push_str(&disposicion::adaptar(MARCA_PERSISTENTE, ascii));
        }
        if pestaña.aviso.is_some() {
            glifo = format!("{glifo} {}", tema.glifos.aviso);
        }
        let estilo = if elegida {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let texto = estilo.fg(tema.paleta.texto);
        let tiempo = if let Some(reconexion) = &pestaña.reconexion {
            format!("intento {}", reconexion.intento)
        } else if soltada {
            "soltada".to_string()
        } else if pestaña.estado == EstadoSesionRemota::Caida {
            "caída".to_string()
        } else {
            crate::ui::sesion::formatear_duracion(pestaña.segundos())
        };
        let titulo = pestaña
            .titulo
            .clone()
            .unwrap_or_else(|| super::tecla(tema, "—", "-").to_string());
        let nombre = if app.escritura_varios.incluye(pestaña.sesion_id) {
            format!("{} {}", tema.glifos.escribiendo, pestaña.nombre)
        } else {
            pestaña.nombre.clone()
        };
        // El tipo se ve siempre (Fase 12): una terminal es SSH; un escritorio,
        // el suyo.
        let tipo = pestaña
            .tipo
            .map(|tipo| tipo.etiqueta().to_string())
            .unwrap_or_else(|| "SSH".to_string());
        let identidad = if let Some(escritorio_id) = pestaña.escritorio_id {
            app.escritorios
                .get(&escritorio_id)
                .and_then(|info| {
                    app.hosts
                        .iter()
                        .find(|host| host.id == info.host_id)
                        .and_then(|host| host.usuario.clone())
                })
                .unwrap_or_else(|| super::tecla(tema, "—", "-").to_string())
        } else if pestaña.identidad.is_empty() {
            super::tecla(tema, "—", "-").to_string()
        } else {
            pestaña.identidad.clone()
        };
        let ventanas = if pestaña.escritorio_id.is_some() {
            super::tecla(tema, "—", "-").to_string()
        } else {
            pestaña.ventanas.to_string()
        };
        let mut spans = vec![Span::styled(format!("{marca} "), estilo)];
        spans.extend(celdas(
            vec![
                (glifo, estilo.fg(color)),
                (nombre, texto),
                (tipo, texto),
                (pestaña.host_nombre.clone(), texto),
                (tiempo, texto),
                (titulo, texto),
                (identidad, texto),
                (ventanas, texto),
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
    let fila_persistente = if app.persistentes_enfocado() {
        app.fila_persistente_elegida()
    } else {
        None
    };
    match app.pestanas.get(app.seleccion_sesiones) {
        _ if fila_persistente.is_some() => {
            let detalle = fila_persistente
                .as_ref()
                .map(|fila| super::persistentes::detalle_de_fila(app, fila))
                .unwrap_or_default();
            lineas.push(Line::from(Span::styled(
                ajustar(format!("  {detalle}")),
                Style::default().fg(tema.paleta.texto),
            )));
        }
        Some(pestaña) => {
            let es_escritorio = pestaña.escritorio_id.is_some();
            let detalle = if let Some(soltada) = pestaña.soltada() {
                // No es una caída: la sesión sigue viva y la usa otro equipo.
                format!(
                    "{} {punto} soltada: la usa {}",
                    pestaña.nombre,
                    soltada.por_equipo.as_deref().unwrap_or("otro equipo")
                )
            } else if pestaña.estado == EstadoSesionRemota::Caida {
                format!(
                    "{} {punto} caída: {}",
                    pestaña.nombre,
                    pestaña
                        .motivo
                        .clone()
                        .unwrap_or_else(|| "motivo desconocido".to_string())
                )
            } else if es_escritorio {
                let estado = pestaña
                    .escritorio_id
                    .and_then(|id| app.escritorios.get(&id))
                    .map(|info| {
                        let visor = if info.visor {
                            "con visor"
                        } else {
                            "sin visor (se cierra pronto)"
                        };
                        format!("{} {} {visor}", info.tipo.etiqueta(), punto)
                    })
                    .unwrap_or_else(|| "escritorio".to_string());
                format!("{} {punto} {estado}", pestaña.nombre)
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
