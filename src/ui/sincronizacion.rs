//! Vistas SINCRONIZACIÓN y CONFLICTOS (Fase 10) y sus diálogos: CLAVE con QR,
//! UNIR ESTE EQUIPO e IMPORTACIÓN CON BORRADOS. La carpeta, la huella corta de
//! la clave y el estado de cada equipo salen de la App; aquí solo se pinta.
//!
//! Disposición adaptable (T45): columnas por prioridad (el equipo y su estado
//! no se ocultan nunca), detalle inferior plegado con la vista baja y anchos
//! naturales en ventanas grandes. El QR solo se dibuja si cabe entero y con el
//! tema Unicode: en ASCII y en terminales pequeñas se muestra el aviso de
//! ampliar la terminal.

use std::path::Path;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::almacen::equipos;
use crate::app::sincronizacion::hace;
use crate::app::{App, Dialogo};
use crate::sincronizacion::qr::QrClave;
use crate::tema::Tema;
use crate::ui::dialogos::{medidas, teclas, Hoja};
use crate::ui::disposicion::{self, Disposicion, Lista, VentanaLista, ANCHO_MAX_DETALLE};
use crate::ui::hosts::{titulo_derecho, titulo_derecho_cabe};
use crate::ui::tuneles::{
    estilo_fila, marco_detalle, prefijo, repartir_vista, Tabla, ANCHO_PREFIJO,
};
use crate::ui::{bloque, tecla};

/// Filas del panel de detalle inferior de CONFLICTOS (bordes incluidos).
pub const ALTO_DETALLE: u16 = 4;
/// Ancho máximo del diálogo de borrados.
const ANCHO_BORRADOS: u16 = 74;
/// Ancho del diálogo CLAVE.
const ANCHO_CLAVE: u16 = 60;
/// Ancho del diálogo UNIR.
const ANCHO_UNIR: u16 = 68;

/// Pinta la vista del área de sincronización que toque.
pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    match app.vista {
        crate::ui::Vista::Conflictos => dibujar_conflictos(marco, area, app, disp),
        _ => dibujar_sincronizacion(marco, area, app, disp),
    }
}

// ------------------------------------------------------------- SINCRONIZACIÓN

fn dibujar_sincronizacion(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let activa = app.config.sincronizacion.activa();
    let titulo = " SINCRONIZACIÓN ";
    let mut marco_bloque = bloque("SINCRONIZACIÓN", tema);
    let derecho = derecho_sincronizacion(app, tema, activa);
    if !derecho.is_empty() && titulo_derecho_cabe(area.width, titulo.trim(), &derecho) {
        marco_bloque = marco_bloque.title_top(titulo_derecho(derecho, tema));
    }
    let interior = marco_bloque.inner(area);
    marco.render_widget(marco_bloque, area);
    if interior.height == 0 || interior.width == 0 {
        return;
    }
    let contenido = if disposicion::es_grande(disp.area) {
        disposicion::limitar_ancho(interior, ANCHO_MAX_DETALLE)
    } else {
        interior
    };
    if !activa {
        dibujar_desactivada(marco, contenido, app, ascii);
        return;
    }
    let cabecera = cabecera_sincronizacion(app, contenido.width, ascii);
    let alto_cabecera = (cabecera.len() as u16).min(contenido.height.saturating_sub(2));
    let pie = pie_sincronizacion(app, contenido.width, ascii);
    let alto_pie = if pie.is_some() { 1 } else { 0 };
    let alto_tabla = contenido
        .height
        .saturating_sub(alto_cabecera + alto_pie + u16::from(alto_pie > 0));
    if alto_cabecera > 0 {
        marco.render_widget(
            Paragraph::new(
                cabecera
                    .into_iter()
                    .take(usize::from(alto_cabecera))
                    .collect::<Vec<_>>(),
            ),
            Rect {
                height: alto_cabecera,
                ..contenido
            },
        );
    }
    if alto_tabla > 0 {
        let tabla = Rect {
            y: contenido.y + alto_cabecera + u16::from(alto_pie > 0),
            height: alto_tabla,
            ..contenido
        };
        dibujar_tabla_equipos(marco, tabla, app, disp);
    }
    if alto_pie > 0 {
        if let Some(pie) = pie {
            marco.render_widget(
                Paragraph::new(pie),
                Rect {
                    y: contenido.y + contenido.height - 1,
                    height: 1,
                    ..contenido
                },
            );
        }
    }
}

/// Estado de la sincronización a la derecha del borde superior.
fn derecho_sincronizacion(app: &App, tema: &Tema, activa: bool) -> String {
    if !activa {
        return String::new();
    }
    if app.sincronizacion.sincronizando {
        return format!(" {} sincronizando ", tema.glifos.tuneles);
    }
    let estado = app.sincronizacion.texto_estado();
    let mut texto = format!(" {} {estado} ", tema.glifos.tuneles);
    if matches!(
        app.sincronizacion.estado,
        crate::sincronizacion::EstadoPasada::AlDia
    ) {
        if let Some(terminada) = app.sincronizacion.terminada_en {
            let segundos = (chrono::Local::now() - terminada).num_seconds().max(0);
            texto = format!(
                " {} {} {} ",
                tema.glifos.tuneles,
                estado,
                crate::app::sincronizacion::hace_segundos(segundos)
            );
        }
    }
    texto
}

fn dibujar_desactivada(marco: &mut Frame, area: Rect, app: &App, ascii: bool) {
    let tema = &app.tema;
    let ancho = usize::from(area.width.saturating_sub(2));
    let lineas = vec![
        Line::from(""),
        Line::from(Span::styled(
            disposicion::recortar(
                "La sincronización está desactivada en este equipo.",
                ancho,
                ascii,
            ),
            Style::default().fg(tema.paleta.inactivo),
        )),
        Line::from(Span::styled(
            disposicion::recortar(
                "Pulsa n para elegir la carpeta compartida: se crea o se une",
                ancho,
                ascii,
            ),
            Style::default().fg(tema.paleta.inactivo),
        )),
        Line::from(Span::styled(
            disposicion::recortar("según haya o no ficheros de otros equipos.", ancho, ascii),
            Style::default().fg(tema.paleta.inactivo),
        )),
    ];
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Cabecera: carpeta, huella corta de la clave y este equipo.
fn cabecera_sincronizacion(app: &App, ancho: u16, ascii: bool) -> Vec<Line<'static>> {
    let tema = &app.tema;
    let etiqueta = Style::default().fg(tema.paleta.inactivo);
    let valor = Style::default().fg(tema.paleta.texto);
    let clave_estilo = Style::default()
        .fg(tema.paleta.acento)
        .add_modifier(Modifier::BOLD);
    let disponible = usize::from(ancho.saturating_sub(2));
    let carpeta = app
        .config
        .sincronizacion
        .ruta_carpeta(&app.rutas.hogar)
        .map(|ruta| ruta_legible(&ruta, &app.rutas.hogar))
        .unwrap_or_else(|| "sin configurar".to_string());
    let huella = app
        .sincronizacion
        .huella
        .clone()
        .unwrap_or_else(|| "sin clave".to_string());
    let este = app
        .sincronizacion
        .equipos
        .iter()
        .find(|equipo| equipo.es_este);
    let nombre_este = este
        .map(|equipo| equipo.nombre.clone())
        .unwrap_or_else(|| "—".to_string());
    let plataforma = este
        .map(|equipo| equipo.plataforma.clone())
        .unwrap_or_default();
    let huella = disposicion::adaptar(&huella, ascii);
    let clave = format!(" clave {huella}  (en el llavero)");
    let mut primera = vec![
        Span::styled(" carpeta ", etiqueta),
        Span::styled(
            disposicion::recortar(&carpeta, disponible.saturating_sub(10), ascii),
            valor,
        ),
    ];
    let mut segunda = vec![
        Span::styled(" este equipo ", etiqueta),
        Span::styled(
            format!("{nombre_este} {} {plataforma}", tema.glifos.punto_medio),
            valor,
        ),
    ];
    let ancho_primera: usize = primera.iter().map(|span| span.width()).sum();
    let ancho_segunda: usize = segunda.iter().map(|span| span.width()).sum();
    if ancho_primera + clave.chars().count() <= disponible {
        primera.push(Span::styled(clave, clave_estilo));
        vec![Line::from(primera), Line::from(segunda)]
    } else if ancho_segunda + clave.chars().count() <= disponible {
        segunda.push(Span::styled(clave, clave_estilo));
        vec![Line::from(primera), Line::from(segunda)]
    } else {
        vec![
            Line::from(primera),
            Line::from(segunda),
            Line::from(vec![
                Span::styled(" clave ", etiqueta),
                Span::styled(huella, clave_estilo),
                Span::styled("  (en el llavero)", etiqueta),
            ]),
        ]
    }
}

/// Pie de la vista: conflictos pendientes, pausa, aviso o error.
fn pie_sincronizacion(app: &App, ancho: u16, ascii: bool) -> Option<Line<'static>> {
    let tema = &app.tema;
    let estado = &app.sincronizacion;
    let (texto, color) = if estado.pausa.is_some() {
        (
            " en pausa: pulsa s y decide los borrados masivos".to_string(),
            tema.paleta.acento,
        )
    } else if estado.conflictos_pendientes > 0 {
        (
            format!(
                " {} conflicto(s) pendiente(s) {} c para verlos",
                estado.conflictos_pendientes, tema.glifos.punto_medio
            ),
            tema.paleta.acento,
        )
    } else {
        let aviso = estado.avisos.first()?;
        (format!(" {aviso}"), tema.paleta.inactivo)
    };
    let texto = disposicion::recortar(&texto, usize::from(ancho), ascii);
    Some(Line::from(Span::styled(texto, Style::default().fg(color))))
}

/// Tabla de equipos: EQUIPO, PLATAFORMA, MAGI, ÚLTIMO FICHERO y ESTADO.
fn dibujar_tabla_equipos(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    if area.height == 0 || area.width == 0 {
        return;
    }
    let lista = &app.sincronizacion.equipos;
    let natural_nombre = lista
        .iter()
        .map(|equipo| equipo.nombre.chars().count())
        .max()
        .unwrap_or(7)
        .clamp(7, 22) as u16;
    let ultimos: Vec<String> = lista
        .iter()
        .map(|equipo| {
            equipo
                .ultimo_fichero_en
                .as_deref()
                .and_then(hace)
                .unwrap_or_else(|| "nunca".to_string())
        })
        .collect();
    let natural_ultimo = ultimos
        .iter()
        .map(|texto| texto.chars().count())
        .max()
        .unwrap_or(7)
        .clamp(7, 14) as u16;
    let estados: Vec<String> = lista.iter().map(estado_equipo_texto).collect();
    let natural_estado = estados
        .iter()
        .map(|texto| texto.chars().count())
        .max()
        .unwrap_or(10)
        .clamp(10, 24) as u16;
    // En estrecho caen primero MAGI y después la plataforma: el estado y el
    // último fichero son lo que se mira para saber si un equipo está al día.
    let columnas = [
        disposicion::Columna::flexible(natural_nombre, 1),
        disposicion::Columna::fija(10, 3),
        disposicion::Columna::fija(8, 4),
        disposicion::Columna::fija(natural_ultimo, 2),
        disposicion::Columna::flexible(natural_estado, 1),
    ];
    let naturales = [natural_nombre, 10, 8, natural_ultimo, natural_estado];
    let tabla = Tabla::nueva(
        area.width.saturating_sub(ANCHO_PREFIJO),
        &columnas,
        &naturales,
    );
    let cabecera = tabla.linea(
        prefijo(false, tema),
        vec![
            ("EQUIPO".to_string(), Style::default()),
            ("PLATAFORMA".to_string(), Style::default()),
            ("MAGI".to_string(), Style::default()),
            ("ÚLTIMO FICHERO".to_string(), Style::default()),
            ("ESTADO".to_string(), Style::default()),
        ],
        Style::default()
            .fg(tema.paleta.inactivo)
            .add_modifier(Modifier::BOLD),
        ascii,
    );
    let mut lineas = vec![cabecera];
    let altura = usize::from(area.height.saturating_sub(1));
    let total = lista.len();
    let inicio = disposicion::ventana(
        app.sincronizacion.desplazamiento,
        app.sincronizacion.seleccion,
        altura,
        total,
    );
    disp.registrar(
        Lista::Sincronizacion,
        VentanaLista {
            inicio,
            filas: altura,
            total,
        },
    );
    for (posicion, equipo) in lista.iter().enumerate().skip(inicio).take(altura) {
        let seleccionada = posicion == app.sincronizacion.seleccion;
        let estilo = estilo_fila(tema, seleccionada);
        let color_estado = if seleccionada {
            estilo
        } else {
            Style::default().fg(color_estado_equipo(&equipo.estado, tema))
        };
        lineas.push(tabla.linea(
            prefijo(seleccionada, tema),
            vec![
                (equipo.nombre.clone(), estilo),
                (equipo.plataforma.clone(), estilo),
                (
                    equipo
                        .app_version
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                    estilo,
                ),
                (ultimos[posicion].clone(), estilo),
                (estados[posicion].clone(), color_estado),
            ],
            estilo,
            ascii,
        ));
    }
    marco.render_widget(
        Paragraph::new(lineas).style(Style::default().fg(tema.paleta.texto)),
        area,
    );
}

/// Texto del estado de un equipo de la carpeta.
fn estado_equipo_texto(equipo: &equipos::Equipo) -> String {
    match equipo.estado.as_str() {
        equipos::ESTADO_ESTE => "este equipo".to_string(),
        equipos::ESTADO_AL_DIA => "al día".to_string(),
        equipos::ESTADO_ILEGIBLE => "no abre con la clave".to_string(),
        equipos::ESTADO_DANADO => "dañado".to_string(),
        equipos::ESTADO_FORMATO_NUEVO => "versión más nueva".to_string(),
        equipos::ESTADO_QUITADO => "quitado".to_string(),
        otro => otro.to_string(),
    }
}

fn color_estado_equipo(estado: &str, tema: &Tema) -> Color {
    match estado {
        equipos::ESTADO_ESTE => tema.paleta.acento,
        equipos::ESTADO_AL_DIA => tema.paleta.correcto,
        equipos::ESTADO_ILEGIBLE
        | equipos::ESTADO_DANADO
        | equipos::ESTADO_FORMATO_NUEVO
        | equipos::ESTADO_QUITADO => tema.paleta.critico,
        _ => tema.paleta.inactivo,
    }
}

// ------------------------------------------------------------------ CONFLICTOS

fn dibujar_conflictos(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let titulo = " CONFLICTOS DE SINCRONIZACIÓN ";
    let derecho = format!(" {} ", app.sincronizacion.conflictos.len());
    let mut marco_bloque = bloque("CONFLICTOS DE SINCRONIZACIÓN", tema);
    if titulo_derecho_cabe(area.width, titulo.trim(), &derecho) {
        marco_bloque = marco_bloque.title_top(titulo_derecho(derecho, tema));
    }
    let interior = marco_bloque.inner(area);
    marco.render_widget(marco_bloque, area);
    if interior.height == 0 || interior.width == 0 {
        return;
    }
    let contenido = if disposicion::es_grande(disp.area) {
        disposicion::limitar_ancho(interior, ANCHO_MAX_DETALLE)
    } else {
        interior
    };
    if app.sincronizacion.conflictos.is_empty() {
        disp.registrar(
            Lista::Conflictos,
            VentanaLista {
                inicio: 0,
                filas: usize::from(contenido.height),
                total: 0,
            },
        );
        let aviso = Line::from(Span::styled(
            disposicion::adaptar("  sin conflictos pendientes", ascii),
            Style::default().fg(tema.paleta.inactivo),
        ));
        marco.render_widget(Paragraph::new(vec![Line::from(""), aviso]), contenido);
        return;
    }
    let (zona, detalle) = repartir_vista(contenido, ALTO_DETALLE, disp);
    dibujar_tabla_conflictos(marco, zona, app, disp);
    if let Some(detalle) = detalle {
        dibujar_detalle_conflicto(marco, detalle, app);
    }
}

fn dibujar_tabla_conflictos(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    if area.height == 0 || area.width == 0 {
        return;
    }
    let conflictos = &app.sincronizacion.conflictos;
    let tipos: Vec<String> = conflictos.iter().map(tipo_conflicto_texto).collect();
    let elementos: Vec<String> = conflictos
        .iter()
        .map(|conflicto| conflicto.nombre.clone())
        .collect();
    let ganadores: Vec<String> = conflictos
        .iter()
        .map(|conflicto| ganador_texto(app, conflicto))
        .collect();
    let cuando: Vec<String> = conflictos
        .iter()
        .map(|conflicto| hace(&conflicto.detectado_en).unwrap_or_else(|| "—".to_string()))
        .collect();
    let natural_tipo = tipos
        .iter()
        .map(|texto| texto.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(8, 12) as u16;
    let natural_elemento = elementos
        .iter()
        .map(|texto| texto.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(8, 20) as u16;
    let natural_ganador = ganadores
        .iter()
        .map(|texto| texto.chars().count())
        .max()
        .unwrap_or(10)
        .clamp(10, 28) as u16;
    let natural_cuando = cuando
        .iter()
        .map(|texto| texto.chars().count())
        .max()
        .unwrap_or(7)
        .clamp(7, 12) as u16;
    let columnas = [
        disposicion::Columna::fija(natural_tipo, 2),
        disposicion::Columna::flexible(natural_elemento, 1),
        disposicion::Columna::flexible(natural_ganador, 2),
        disposicion::Columna::fija(natural_cuando, 3),
    ];
    let tabla = Tabla::nueva(
        area.width.saturating_sub(ANCHO_PREFIJO),
        &columnas,
        &[
            natural_tipo,
            natural_elemento,
            natural_ganador,
            natural_cuando,
        ],
    );
    let cabecera = tabla.linea(
        prefijo(false, tema),
        vec![
            ("TIPO".to_string(), Style::default()),
            ("ELEMENTO".to_string(), Style::default()),
            ("GANÓ".to_string(), Style::default()),
            ("CUÁNDO".to_string(), Style::default()),
        ],
        Style::default()
            .fg(tema.paleta.inactivo)
            .add_modifier(Modifier::BOLD),
        ascii,
    );
    let mut lineas = vec![cabecera];
    let altura = usize::from(area.height.saturating_sub(1));
    let total = conflictos.len();
    let inicio = disposicion::ventana(
        app.sincronizacion.desplazamiento_conflicto,
        app.sincronizacion.seleccion_conflicto,
        altura,
        total,
    );
    disp.registrar(
        Lista::Conflictos,
        VentanaLista {
            inicio,
            filas: altura,
            total,
        },
    );
    for (posicion, _conflicto) in conflictos.iter().enumerate().skip(inicio).take(altura) {
        let seleccionada = posicion == app.sincronizacion.seleccion_conflicto;
        let estilo = estilo_fila(tema, seleccionada);
        lineas.push(tabla.linea(
            prefijo(seleccionada, tema),
            vec![
                (tipos[posicion].clone(), estilo),
                (elementos[posicion].clone(), estilo),
                (ganadores[posicion].clone(), estilo),
                (cuando[posicion].clone(), estilo),
            ],
            estilo,
            ascii,
        ));
    }
    marco.render_widget(
        Paragraph::new(lineas).style(Style::default().fg(tema.paleta.texto)),
        area,
    );
}

/// El detalle del conflicto seleccionado lo prepara la App al moverse por la
/// lista (necesita leer el almacén); aquí solo se pinta.
fn dibujar_detalle_conflicto(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let interior = marco_detalle(marco, area, tema);
    if interior.height == 0 {
        return;
    }
    let ancho = usize::from(interior.width.saturating_sub(2));
    let mut lineas = Vec::new();
    for texto in app
        .sincronizacion
        .detalle
        .iter()
        .take(usize::from(interior.height))
    {
        lineas.push(Line::from(Span::styled(
            format!(
                "  {}",
                disposicion::recortar(&disposicion::adaptar(texto, ascii), ancho, ascii)
            ),
            Style::default().fg(tema.paleta.texto),
        )));
    }
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Tipo de conflicto legible.
fn tipo_conflicto_texto(conflicto: &crate::almacen::conflictos::Conflicto) -> String {
    conflicto.tipo.clone()
}

/// Quién ganó, para la columna GANÓ: equipo, «aplazado» o «aceptó otra».
fn ganador_texto(app: &App, conflicto: &crate::almacen::conflictos::Conflicto) -> String {
    match conflicto.tipo.as_str() {
        crate::almacen::conflictos::TIPO_BORRADO => "aplazado".to_string(),
        crate::almacen::conflictos::TIPO_HUERFANO => "huérfano".to_string(),
        crate::almacen::conflictos::TIPO_HUELLA => {
            match nombre_equipo(app, conflicto.equipo_ganador.as_deref()) {
                Some(nombre) => format!("{nombre} aceptó otra"),
                None => "otro equipo aceptó otra".to_string(),
            }
        }
        _ => nombre_equipo(app, conflicto.equipo_ganador.as_deref())
            .unwrap_or_else(|| "—".to_string()),
    }
}

/// Nombre de un equipo por su uuid.
fn nombre_equipo(app: &App, uuid: Option<&str>) -> Option<String> {
    let uuid = uuid?;
    if uuid.is_empty() {
        return None;
    }
    Some(
        app.sincronizacion
            .equipos
            .iter()
            .find(|equipo| equipo.uuid == uuid)
            .map(|equipo| equipo.nombre.clone())
            .unwrap_or_else(|| uuid.chars().take(8).collect()),
    )
}

// ------------------------------------------------------------------ diálogos

/// Diálogo CLAVE DE SINCRONIZACIÓN: texto completo y QR si cabe.
pub fn dibujar_clave(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &Dialogo,
    disp: &mut Disposicion,
) {
    let Dialogo::ClaveSincronizacion { clave, qr, .. } = dialogo else {
        return;
    };
    let tema = &app.tema;
    let ascii = tema.ascii;
    let (_, ancho_texto) = medidas(area, ANCHO_CLAVE);
    let mostrar_qr = qr
        .as_ref()
        .is_some_and(|qr| cabe_qr(qr, ancho_texto, area, ascii));
    let mut cuerpo = Vec::new();
    if mostrar_qr {
        cuerpo.push(Line::from(
            "Guárdala en tu gestor de contraseñas. La necesitas",
        ));
        cuerpo.push(Line::from("para añadir equipos. Si la pierdes, tus datos"));
        cuerpo.push(Line::from(
            "siguen en cada equipo: crearías una clave nueva.",
        ));
    } else {
        // Sin sitio para el QR: solo el texto, con el aviso de ampliar.
        cuerpo.push(Line::from(
            "Guárdala en tu gestor de contraseñas: la necesitas para añadir equipos.",
        ));
    }
    cuerpo.push(Line::from(""));
    let foco = cuerpo.len();
    cuerpo.push(Line::from(Span::styled(
        clave.to_string(),
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    )));
    cuerpo.push(Line::from(""));
    if let (true, Some(qr)) = (mostrar_qr, qr.as_ref()) {
        for linea in &qr.lineas {
            cuerpo.push(Line::from(Span::styled(
                linea.clone(),
                Style::default().bg(Color::White).fg(Color::Black),
            )));
        }
    } else {
        let aviso = if ascii {
            "el QR no se dibuja en modo ASCII: copia la clave"
        } else {
            "amplía la terminal para ver el QR"
        };
        cuerpo.push(Line::from(Span::styled(
            aviso.to_string(),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let pie = vec![teclas(
        &[("s", "la he guardado"), ("esc", "cancelar")],
        tema,
    )];
    Hoja::nueva("CLAVE DE SINCRONIZACIÓN", cuerpo, pie)
        .foco(foco)
        .peligro(true)
        .ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO_CLAVE, tema, disp);
}

/// ¿Cabe el QR entero en el diálogo, sin desplazar?
fn cabe_qr(qr: &QrClave, ancho_texto: usize, area: Rect, ascii: bool) -> bool {
    let alto_necesario = qr.lineas.len() as u16 + 12;
    !ascii && qr.columnas <= ancho_texto && area.height >= alto_necesario
}

/// Diálogo UNIR ESTE EQUIPO: carpeta, equipos conocidos y clave pegada.
pub fn dibujar_unir(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &Dialogo,
    disp: &mut Disposicion,
) {
    let Dialogo::UnirEquipo {
        carpeta,
        equipos,
        campo,
        visible,
        error,
    } = dialogo
    else {
        return;
    };
    let tema = &app.tema;
    let nombre = ruta_legible(carpeta, &app.rutas.hogar);
    let cuantos = equipos.len();
    let mut cuerpo = Vec::new();
    cuerpo.push(Line::from(Span::styled(
        if cuantos == 0 {
            format!("{nombre} tiene ficheros de otros equipos de MAGI.")
        } else if cuantos == 1 {
            format!("{nombre} tiene ficheros de 1 equipo:")
        } else {
            format!("{nombre} tiene ficheros de {cuantos} equipos:")
        },
        Style::default().fg(tema.paleta.texto),
    )));
    if !equipos.is_empty() {
        cuerpo.push(Line::from(Span::styled(
            format!("  {}", equipos.join(" · ")),
            Style::default().fg(tema.paleta.acento),
        )));
    }
    cuerpo.push(Line::from(""));
    let mut spans = vec![
        Span::styled("clave  ", Style::default().fg(tema.paleta.inactivo)),
        Span::raw("[ "),
    ];
    if *visible {
        spans.push(campo.span(true, Style::default().fg(tema.paleta.texto)));
    } else {
        spans.push(Span::styled(
            enmascarado(campo, tema),
            Style::default().fg(tema.paleta.texto),
        ));
    }
    spans.push(Span::raw(" ]"));
    let foco = cuerpo.len();
    cuerpo.push(Line::from(spans));
    cuerpo.push(Line::from(Span::styled(
        format!(
            "       {} {}",
            tecla(tema, "⇥", "tab"),
            if *visible { "ocultar" } else { "mostrar" }
        ),
        Style::default().fg(tema.paleta.inactivo),
    )));
    cuerpo.push(Line::from(""));
    cuerpo.push(Line::from(
        "Los hosts con el mismo nombre se tomarán como el",
    ));
    cuerpo.push(Line::from("mismo (gana la versión más reciente)."));
    if let Some(error) = error {
        cuerpo.push(Line::from(""));
        cuerpo.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    let pie = vec![teclas(&[("↵", "unir"), ("esc", "cancelar")], tema)];
    Hoja::nueva("UNIR ESTE EQUIPO", cuerpo, pie)
        .foco(foco)
        .ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO_UNIR, tema, disp);
}

/// Texto enmascarado de la clave pegada, con el cursor donde esté.
fn enmascarado(campo: &crate::ui::componentes::CampoTexto, tema: &Tema) -> String {
    let mascara = |_: char| if tema.ascii { '*' } else { '•' };
    if campo.texto.is_empty() {
        return tecla(tema, "|", "|").to_string();
    }
    let mut visible = String::new();
    for (indice, caracter) in campo.texto.chars().enumerate() {
        if indice == campo.cursor {
            visible.push('|');
        }
        visible.push(mascara(caracter));
    }
    if campo.cursor >= campo.texto.chars().count() {
        visible.push('|');
    }
    visible
}

/// Diálogo IMPORTACIÓN CON BORRADOS: la copia ya existe y no se ha aplicado
/// nada.
pub fn dibujar_importacion(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &Dialogo,
    disp: &mut Disposicion,
) {
    let Dialogo::ImportacionBorrados { pendientes } = dialogo else {
        return;
    };
    let tema = &app.tema;
    let mut cuerpo = vec![Line::from(Span::styled(
        format!(
            "Se han borrado {} de tus {} hosts en otro equipo:",
            pendientes.hosts.len(),
            pendientes.total_hosts
        ),
        Style::default().fg(tema.paleta.critico),
    ))];
    cuerpo.push(Line::from(""));
    cuerpo.push(Line::from(Span::styled(
        pendientes.hosts.join(" · "),
        Style::default().fg(tema.paleta.texto),
    )));
    cuerpo.push(Line::from(""));
    cuerpo.push(Line::from(
        "Copia de magi.db guardada antes de aplicar nada:",
    ));
    cuerpo.push(Line::from(Span::styled(
        nombre_copia(&pendientes.copia),
        Style::default().fg(tema.paleta.inactivo),
    )));
    cuerpo.push(Line::from(""));
    for (tecla_accion, texto) in [
        ("a", "aplicar los borrados"),
        ("m", "mantener los míos (vuelven a todos los equipos)"),
        ("esc", "decidir luego (sincronización en pausa)"),
    ] {
        cuerpo.push(Line::from(vec![
            Span::styled(
                format!("{tecla_accion}  "),
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(texto.to_string(), Style::default().fg(tema.paleta.texto)),
        ]));
    }
    Hoja::nueva("IMPORTACIÓN CON BORRADOS", cuerpo, Vec::new())
        .peligro(true)
        .ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO_BORRADOS, tema, disp);
}

/// copias/magi-AAAAMMDD-HHMMSS.db sin la ruta absoluta.
fn nombre_copia(ruta: &Path) -> String {
    let fichero = ruta
        .file_name()
        .map(|nombre| nombre.to_string_lossy().to_string())
        .unwrap_or_else(|| ruta.display().to_string());
    format!("copias/{fichero}")
}

/// Ruta con ~ si cuelga del home, para la cabecera y los diálogos.
pub fn ruta_legible(ruta: &Path, hogar: &Path) -> String {
    match ruta.strip_prefix(hogar) {
        Ok(resto) if resto.as_os_str().is_empty() => "~".to_string(),
        Ok(resto) => format!("~/{}", resto.display()),
        Err(_) => ruta.display().to_string(),
    }
}

/// Pinta el diálogo de sincronización que toque (los tres de la Fase 10).
pub fn dibujar_dialogo(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &Dialogo,
    disp: &mut Disposicion,
) {
    match dialogo {
        Dialogo::ClaveSincronizacion { .. } => dibujar_clave(marco, area, app, dialogo, disp),
        Dialogo::UnirEquipo { .. } => dibujar_unir(marco, area, app, dialogo, disp),
        Dialogo::ImportacionBorrados { .. } => dibujar_importacion(marco, area, app, dialogo, disp),
        _ => {}
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_rutas_del_home_se_abrevian() {
        let hogar = Path::new("/home/ana");
        assert_eq!(ruta_legible(Path::new("/home/ana"), hogar), "~");
        assert_eq!(
            ruta_legible(Path::new("/home/ana/Sync/magi"), hogar),
            "~/Sync/magi"
        );
        assert_eq!(ruta_legible(Path::new("/mnt/datos"), hogar), "/mnt/datos");
    }

    #[test]
    fn la_copia_se_nombra_sin_la_ruta_absoluta() {
        assert_eq!(
            nombre_copia(Path::new(
                "/home/ana/.local/share/magi/copias/magi-20261105-101200.db"
            )),
            "copias/magi-20261105-101200.db"
        );
    }
}
