//! Vista Servicios (F9, Fase 13): servicios y timers de systemd, contenedores
//! de Docker o Podman y sus logs en vivo.
//!
//! Disposición adaptable (T45), derivada del área en cada pintado:
//! - normal (≥ 100 columnas): hosts a la izquierda y lista o detalle a la
//!   derecha;
//! - estrecho: un solo panel con su cabecera y Tab (en el detalle, Tab cambia
//!   de pestaña);
//! - el panel de logs ocupa la pantalla entera, con su propio pie;
//! - con 120 columnas o más, la lista de servicios enseña la descripción.
//!
//! No guarda tamaños entre pintados: todo se deriva del área.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use magi_nucleo::servicios::{acciones, Contenedor, NivelLog, Timer, TipoObjetivo, Unidad};

use crate::app::servicios::{
    ConfirmacionServicio, DialogoServicio, EditorComprobacion, EstadoListado, Fila, FilaFlota,
    Foco, LineaAnillo, PanelLogs, PestanaDetalle, ResultadoDialogo, Seccion, SudoPedido,
};
use crate::app::App;
use crate::tema::Tema;
use crate::ui::disposicion::{self, Columna, Disposicion, Lista, VentanaLista};
use crate::ui::hosts::{anchos_tabla, titulo_derecho, titulo_derecho_cabe};

/// Ancho de la columna de hosts en modo normal.
const ANCHO_HOSTS: u16 = 26;
/// Con este ancho de terminal o más se ve la descripción de las unidades.
const ANCHO_DESCRIPCION: u16 = 120;
/// Con este ancho de lista o más se ven los puertos de los contenedores.
const ANCHO_PUERTOS: u16 = 96;
/// Ancho de la marca de selección.
const MARCA: usize = 2;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let titulo = format!(" MAGI {} SERVICIOS ", tema.glifos.punto_medio);
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            titulo.clone(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    if let Some(derecha) = cabecera_derecha(app, area.width, &titulo) {
        bloque = bloque.title_top(titulo_derecho(derecha, tema));
    }
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 || interior.width < 4 {
        return;
    }
    if disposicion::es_estrecho(area.width) {
        dibujar_estrecho(marco, interior, app, disp);
    } else {
        dibujar_normal(marco, interior, app, disp);
    }
}

/// Título derecho del marco: host, systemd y antigüedad del listado; en
/// «Toda la flota», el número de fallos y las antigüedades de los sondeos.
fn cabecera_derecha(app: &App, ancho: u16, titulo: &str) -> Option<String> {
    let punto = app.tema.glifos.punto_medio;
    let texto = match app.servicios_host_seleccionado() {
        None => {
            let fallos = app.servicios_fallos_flota();
            if fallos.is_empty() {
                "toda la flota".to_string()
            } else {
                let (minima, maxima) = fallos.iter().fold(
                    (f64::INFINITY, f64::NEG_INFINITY),
                    |(minima, maxima), fallo| {
                        (
                            minima.min(fallo.antiguedad_seg),
                            maxima.max(fallo.antiguedad_seg),
                        )
                    },
                );
                format!(
                    "toda la flota {punto} {} fallo(s) {punto} sondeos de hace {} a {}",
                    fallos.len(),
                    crate::flota::estado::formatear_antiguedad(minima),
                    crate::flota::estado::formatear_antiguedad(maxima)
                )
            }
        }
        Some(host_id) => {
            let nombre = app.servicios_host_nombre(host_id);
            let mut partes = vec![nombre];
            if let Some(listado_host) = app.servicios_listado(host_id) {
                match &listado_host.listado.systemd {
                    magi_nucleo::servicios::Systemd::Disponible(version) => {
                        partes.push(format!("systemd {version}"));
                    }
                    magi_nucleo::servicios::Systemd::Antiguo(version) => {
                        partes.push(format!("systemd {version} (antiguo)"));
                    }
                    magi_nucleo::servicios::Systemd::Ausente => {}
                }
                if let magi_nucleo::servicios::MotorEstado::Disponible(motor) =
                    &listado_host.listado.motor
                {
                    partes.push(format!("{} {}", motor.nombre.texto(), motor.version));
                }
                if let Some(segundos) = app.servicios_hora(host_id) {
                    partes.push(format!(
                        "hace {}",
                        crate::flota::estado::formatear_antiguedad(segundos)
                    ));
                }
                if listado_host.estado == EstadoListado::Desconectado {
                    partes.push("desconectado".to_string());
                }
            } else {
                partes.push("cargando".to_string());
            }
            partes.join(&format!(" {punto} "))
        }
    };
    let derecho = format!(" {texto} ");
    titulo_derecho_cabe(ancho, titulo, &derecho).then_some(derecho)
}

fn dibujar_normal(marco: &mut Frame, interior: Rect, app: &App, disp: &mut Disposicion) {
    let ancho_hosts = ANCHO_HOSTS.min(interior.width / 2);
    let hosts = Rect {
        width: ancho_hosts,
        ..interior
    };
    let separador = Rect {
        x: interior.x + ancho_hosts,
        width: 1,
        ..interior
    };
    let derecha = Rect {
        x: separador.x + 1,
        width: interior.width.saturating_sub(ancho_hosts + 1),
        ..interior
    };
    dibujar_hosts(marco, hosts, app, disp);
    let tema = &app.tema;
    let rayas: Vec<Line> = (0..separador.height)
        .map(|_| {
            Line::from(Span::styled(
                tema.glifos.separador,
                Style::default().fg(tema.paleta.inactivo),
            ))
        })
        .collect();
    marco.render_widget(Paragraph::new(rayas), separador);
    let derecha = Rect {
        x: derecha.x + 1,
        width: derecha.width.saturating_sub(1),
        ..derecha
    };
    if app.servicios.detalle.is_some() {
        dibujar_detalle(marco, derecha, app, disp);
    } else if app.servicios_host_seleccionado().is_none() {
        dibujar_toda_la_flota(marco, derecha, app, disp);
    } else {
        dibujar_lista(marco, derecha, app, disp);
    }
}

fn dibujar_estrecho(marco: &mut Frame, interior: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let (etiqueta, destino) = if app.servicios.detalle.is_some() {
        match app
            .servicios
            .detalle
            .as_ref()
            .map(|detalle| detalle.pestaña)
        {
            Some(PestanaDetalle::Estado) => ("DETALLE", "dependencias"),
            Some(PestanaDetalle::Dependencias) => ("DETALLE", "unidad"),
            _ => ("DETALLE", "estado"),
        }
    } else if app.servicios_host_seleccionado().is_none() {
        ("TODA LA FLOTA", "hosts")
    } else {
        match app.servicios.foco {
            Foco::Hosts => ("HOSTS", "lista"),
            Foco::Lista => ("LISTA", "hosts"),
        }
    };
    let ancho = usize::from(interior.width);
    let izquierda = format!("  {etiqueta}");
    let derecha = if app.servicios.filtro_activo {
        String::new()
    } else {
        format!("{} {destino}  ", tema.glifos.tab)
    };
    let hueco = ancho.saturating_sub(izquierda.chars().count() + derecha.chars().count());
    let cabecera = Line::from(vec![
        Span::styled(
            izquierda,
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(hueco)),
        Span::styled(derecha, Style::default().fg(tema.paleta.inactivo)),
    ]);
    let alto_cabecera = if interior.height >= 2 { 2u16 } else { 1 };
    marco.render_widget(
        Paragraph::new(vec![
            cabecera,
            Line::from(Span::styled(
                tema.glifos.linea.repeat(ancho.saturating_sub(2)),
                Style::default().fg(tema.paleta.inactivo),
            )),
        ]),
        Rect {
            height: alto_cabecera,
            ..interior
        },
    );
    let resto = Rect {
        y: interior.y + alto_cabecera,
        height: interior.height.saturating_sub(alto_cabecera),
        ..interior
    };
    if resto.height == 0 {
        return;
    }
    if app.servicios.detalle.is_some() {
        dibujar_detalle(marco, resto, app, disp);
    } else if app.servicios_host_seleccionado().is_none() {
        dibujar_toda_la_flota(marco, resto, app, disp);
    } else {
        match app.servicios.foco {
            Foco::Hosts => dibujar_hosts(marco, resto, app, disp),
            Foco::Lista => dibujar_lista(marco, resto, app, disp),
        }
    }
}

// ------------------------------------------------------- columna de hosts

fn dibujar_hosts(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let hosts = app.servicios_hostes();
    let total_filas = hosts.len() + 1;
    let cabecera = if area.height >= 3 { 2 } else { 1 };
    let mut lineas = vec![Line::from(vec![
        Span::styled(
            "  HOSTS",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("({})", hosts.len()),
            Style::default().fg(tema.paleta.inactivo),
        ),
    ])];
    if cabecera == 2 {
        lineas.push(Line::from(Span::styled(
            tema.glifos
                .linea
                .repeat(usize::from(area.width.saturating_sub(1))),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let filas = usize::from(area.height.saturating_sub(cabecera));
    let inicio = disposicion::ventana(
        app.servicios.desplazamiento_hosts,
        app.servicios.seleccion_host,
        filas,
        total_filas,
    );
    disp.registrar(
        Lista::ServiciosHosts,
        VentanaLista {
            inicio,
            filas,
            total: total_filas,
        },
    );
    let fallos_totales = app.servicios_fallos_flota().len();
    for posicion in inicio..(inicio + filas).min(total_filas) {
        let seleccionado = posicion == app.servicios.seleccion_host;
        let marca = if seleccionado {
            tema.glifos.seleccion
        } else {
            " "
        };
        let mut linea = if posicion == 0 {
            let color = if fallos_totales > 0 {
                tema.paleta.acento
            } else {
                tema.paleta.texto
            };
            let mut spans = vec![
                Span::raw(format!("{marca} ")),
                Span::styled(
                    crate::ui::tecla(tema, "▲", "^").to_string(),
                    Style::default().fg(tema.paleta.acento),
                ),
                Span::styled(" Toda la flota", Style::default().fg(color)),
            ];
            if fallos_totales > 0 {
                spans.push(Span::styled(
                    format!(" {}{fallos_totales}", crate::ui::tecla(tema, "⚠", "!")),
                    Style::default().fg(tema.paleta.acento),
                ));
            }
            Line::from(spans)
        } else {
            let host_id = hosts[posicion - 1];
            let nombre = app.servicios_host_nombre(host_id);
            let (glifo, color) = glifo_host(app, host_id);
            let fallos = app.servicios_fallos_total(host_id);
            let ancho_nombre = usize::from(area.width).saturating_sub(MARCA + 3 + 2).max(4);
            let nombre = disposicion::recortar(&nombre, ancho_nombre, tema.ascii);
            let mut spans = vec![
                Span::raw(format!("{marca} ")),
                Span::styled(
                    glifo,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {nombre}"), Style::default().fg(tema.paleta.texto)),
            ];
            if fallos > 0 {
                spans.push(Span::styled(
                    format!(" {}{fallos}", crate::ui::tecla(tema, "⚠", "!")),
                    Style::default().fg(tema.paleta.acento),
                ));
            }
            Line::from(spans)
        };
        if seleccionado {
            linea = con_fondo(linea, area.width, tema);
        }
        lineas.push(linea);
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Glifo y color del estado de un host según su último sondeo.
fn glifo_host(app: &App, host_id: i64) -> (String, Color) {
    let tema = &app.tema;
    if app.sondeando.contains(&host_id) {
        return (tema.glifos.conectando.to_string(), tema.paleta.acento);
    }
    let (estado, _) =
        crate::flota::estado::evaluar(app.sondeos.get(&host_id), &app.config.flota.umbrales);
    let (glifo, color) = match estado {
        crate::flota::estado::EstadoFlota::Nominal => (tema.glifos.conectado, tema.paleta.correcto),
        crate::flota::estado::EstadoFlota::Carga => (tema.glifos.conectando, tema.paleta.acento),
        crate::flota::estado::EstadoFlota::Caida => (tema.glifos.error, tema.paleta.critico),
        crate::flota::estado::EstadoFlota::Alcanzable => {
            (tema.glifos.conectado, tema.paleta.inactivo)
        }
        crate::flota::estado::EstadoFlota::Fria => (tema.glifos.desconectado, tema.paleta.inactivo),
    };
    (glifo.to_string(), color)
}

/// Pinta una línea seleccionable con el fondo de selección.
fn con_fondo(mut linea: Line<'static>, ancho: u16, tema: &Tema) -> Line<'static> {
    let largo = linea.width();
    if largo < usize::from(ancho) {
        linea
            .spans
            .push(Span::raw(" ".repeat(usize::from(ancho) - largo)));
    }
    linea.style(
        Style::default()
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
            .add_modifier(Modifier::BOLD),
    )
}

// -------------------------------------------------------------- lista

fn dibujar_lista(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let Some(host_id) = app.servicios_host_seleccionado() else {
        return;
    };
    let Some(listado_host) = app.servicios_listado(host_id) else {
        let aviso = Paragraph::new(Line::from(Span::styled(
            "  cargando el listado del host…",
            Style::default().fg(tema.paleta.inactivo),
        )));
        marco.render_widget(aviso, area);
        return;
    };
    let listado = &listado_host.listado;
    let mut lineas: Vec<Line> = Vec::new();
    if app.servicios.filtro_activo {
        lineas.push(linea_filtro_servicios(app));
    }
    lineas.push(linea_secciones(app, listado, area.width));
    let (titulos, anchos, seccion) = columnas_de(app, area.width, disp.area.width);
    let mut cabecera = vec![Span::raw("  ")];
    for (indice, titulo) in titulos.iter().enumerate() {
        if anchos[indice] == 0 {
            continue;
        }
        cabecera.push(Span::styled(
            disposicion::columna(titulo, usize::from(anchos[indice]), tema.ascii),
            Style::default().fg(tema.paleta.inactivo),
        ));
        if indice + 1 < titulos.len() {
            cabecera.push(Span::raw(" "));
        }
    }
    lineas.push(Line::from(cabecera));
    lineas.push(Line::from(Span::styled(
        tema.glifos
            .linea
            .repeat(usize::from(area.width.saturating_sub(1))),
        Style::default().fg(tema.paleta.inactivo),
    )));
    let filas = app.servicios_filas(host_id);
    let seleccion = app.servicios_seleccion(host_id);
    let fila_seleccionada = filas
        .iter()
        .position(|fila| matches!(fila, Fila::Elemento(_, indice) if *indice == seleccion))
        .unwrap_or(0);
    let aviso = app.servicios_aviso(host_id);
    let pie = 1 + usize::from(aviso.is_some());
    let alto_filas = usize::from(area.height)
        .saturating_sub(lineas.len() + pie)
        .max(1);
    let inicio = disposicion::ventana(
        app.servicios.desplazamiento_lista,
        fila_seleccionada,
        alto_filas,
        filas.len(),
    );
    disp.registrar(
        Lista::ServiciosLista,
        VentanaLista {
            inicio,
            filas: alto_filas,
            total: filas.len(),
        },
    );
    let mut contador = 0usize;
    for fila in &filas {
        match fila {
            Fila::Elemento(elemento, indice) => {
                let visible = contador >= inicio && contador < inicio + alto_filas;
                contador += 1;
                if !visible {
                    continue;
                }
                let mut linea = linea_elemento(
                    app,
                    listado,
                    seccion,
                    elemento,
                    &anchos,
                    area.width,
                    *indice == seleccion,
                );
                if *indice == seleccion {
                    linea = con_fondo(linea, area.width, tema);
                }
                lineas.push(linea);
            }
            Fila::Separador => {
                let visible = contador >= inicio && contador < inicio + alto_filas;
                contador += 1;
                if visible {
                    lineas.push(Line::from(Span::styled(
                        tema.glifos
                            .linea
                            .repeat(usize::from(area.width.saturating_sub(4))),
                        Style::default().fg(tema.paleta.inactivo),
                    )));
                }
            }
            Fila::Cabecera(texto) => {
                let visible = contador >= inicio && contador < inicio + alto_filas;
                contador += 1;
                if visible {
                    lineas.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("{} {texto}", app.tema.glifos.desplegado),
                            Style::default()
                                .fg(tema.paleta.acento)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
            }
        }
    }
    if seccion == Seccion::Contenedores {
        match &listado.motor {
            magi_nucleo::servicios::MotorEstado::No => {
                lineas.push(Line::from(Span::styled(
                    " no hay Docker ni Podman en el host: la sección Contenedores queda vacía",
                    Style::default().fg(tema.paleta.inactivo),
                )));
            }
            magi_nucleo::servicios::MotorEstado::SinPermiso => {
                lineas.push(Line::from(Span::styled(
                    " sin permiso para Docker: añade el usuario al grupo docker o pulsa ! para usar sudo",
                    Style::default().fg(tema.paleta.acento),
                )));
            }
            magi_nucleo::servicios::MotorEstado::Disponible(_) => {}
        }
    }
    lineas.push(linea_recuento(app, listado, seccion));
    if let Some((texto, error)) = aviso {
        let color = if error {
            tema.paleta.critico
        } else {
            tema.paleta.acento
        };
        lineas.push(Line::from(Span::styled(
            format!(
                " {}",
                disposicion::recortar(
                    &texto,
                    usize::from(area.width.saturating_sub(2)),
                    tema.ascii
                )
            ),
            Style::default().fg(color),
        )));
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

/// Línea del filtro de la sección visible (con el cursor).
fn linea_filtro_servicios(app: &App) -> Line<'static> {
    let tema = &app.tema;
    Line::from(vec![
        Span::styled(
            "  / ",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{}{}",
                app.servicios.filtro,
                crate::ui::tecla(tema, "▏", "|")
            ),
            Style::default().fg(tema.paleta.texto),
        ),
    ])
}

/// Cabecera de secciones con su recuento.
fn linea_secciones(
    app: &App,
    listado: &magi_nucleo::servicios::Listado,
    _ancho: u16,
) -> Line<'static> {
    let tema = &app.tema;
    let servicios = listado
        .unidades
        .iter()
        .filter(|unidad| !unidad.plantilla())
        .count();
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (indice, seccion) in [Seccion::Servicios, Seccion::Timers, Seccion::Contenedores]
        .into_iter()
        .enumerate()
    {
        if indice > 0 {
            spans.push(Span::raw("  "));
        }
        let activa = app.servicios.seccion == seccion;
        let cuantos = match seccion {
            Seccion::Servicios => servicios,
            Seccion::Timers => listado.timers.len(),
            Seccion::Contenedores => listado.contenedores.len(),
        };
        let marca = if activa { tema.glifos.seleccion } else { " " };
        let estilo = if activa {
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.inactivo)
        };
        spans.push(Span::styled(format!("{marca}{}", indice + 1), estilo));
        spans.push(Span::styled(
            format!(" {} ({cuantos})", seccion.texto()),
            estilo,
        ));
    }
    Line::from(spans)
}

/// Columnas de la sección: títulos y anchos ya repartidos.
fn columnas_de(
    app: &App,
    ancho_lista: u16,
    ancho_terminal: u16,
) -> (Vec<&'static str>, Vec<u16>, Seccion) {
    let seccion = app.servicios.seccion;
    let mut columnas = Vec::new();
    let mut titulos = Vec::new();
    match seccion {
        Seccion::Servicios => {
            columnas.push(Columna::flexible(18, 1));
            titulos.push("UNIDAD");
            columnas.push(Columna::fija(12, 2));
            titulos.push("ESTADO");
            columnas.push(Columna::fija(12, 3));
            titulos.push("ARRANQUE");
            columnas.push(Columna::fija(9, 4));
            titulos.push("DESDE");
            if ancho_terminal >= ANCHO_DESCRIPCION {
                columnas.push(Columna::flexible(16, 5));
                titulos.push("DESCRIPCIÓN");
            }
        }
        Seccion::Timers => {
            columnas.push(Columna::flexible(18, 1));
            titulos.push("TIMER");
            columnas.push(Columna::fija(10, 2));
            titulos.push("ESTADO");
            columnas.push(Columna::fija(10, 3));
            titulos.push("PRÓXIMA");
            columnas.push(Columna::fija(10, 4));
            titulos.push("ÚLTIMA");
            columnas.push(Columna::flexible(16, 5));
            titulos.push("SERVICIO");
        }
        Seccion::Contenedores => {
            columnas.push(Columna::flexible(18, 1));
            titulos.push("NOMBRE");
            columnas.push(Columna::fija(16, 2));
            titulos.push("ESTADO");
            columnas.push(Columna::flexible(18, 3));
            titulos.push("IMAGEN");
            if ancho_lista >= ANCHO_PUERTOS {
                columnas.push(Columna::flexible(14, 4));
                titulos.push("PUERTOS");
            }
            columnas.push(Columna::fija(6, 5));
            titulos.push("CPU");
            columnas.push(Columna::fija(9, 6));
            titulos.push("MEM");
        }
    }
    let anchos = anchos_tabla(ancho_lista.saturating_sub(MARCA as u16 + 2), &columnas, 1);
    (titulos, anchos, seccion)
}

/// Una fila de la tabla, según la sección.
fn linea_elemento(
    app: &App,
    listado: &magi_nucleo::servicios::Listado,
    seccion: Seccion,
    elemento: &crate::app::servicios::Elemento,
    anchos: &[u16],
    ancho: u16,
    seleccionada: bool,
) -> Line<'static> {
    let tema = &app.tema;
    let marca = if seleccionada {
        tema.glifos.seleccion
    } else {
        " "
    };
    let mut spans = vec![Span::raw(format!("{marca} ")), Span::raw(" ")];
    let mut celdas: Vec<String> = Vec::new();
    let mut colores: Vec<Color> = Vec::new();
    let mut prefijos: Vec<String> = Vec::new();
    match seccion {
        Seccion::Servicios => {
            let Some(unidad) = listado
                .unidades
                .iter()
                .find(|unidad| unidad.nombre == elemento.nombre())
            else {
                return Line::from(" ");
            };
            let vigilada = unidad.vigilada;
            let critica = acciones::es_critica(&app.config.servicios.criticas, &unidad.nombre);
            let marca = if vigilada {
                crate::ui::tecla(tema, "★", "+")
            } else if critica {
                crate::ui::tecla(tema, "⚑", "^")
            } else {
                " "
            };
            celdas.push(format!("{marca} {}", unidad.nombre));
            colores.push(if vigilada {
                tema.paleta.acento
            } else {
                tema.paleta.texto
            });
            let (glifo, texto, color) = estado_unidad(app, elemento.nombre(), unidad);
            celdas.push(format!("{glifo} {texto}"));
            colores.push(color);
            celdas.push(unidad.arranque().texto().to_string());
            colores.push(if unidad.enmascarada() {
                tema.paleta.critico
            } else {
                tema.paleta.texto
            });
            celdas.push(desde_unidad(listado, unidad));
            colores.push(tema.paleta.texto);
            if anchos.len() > 4 && anchos[4] > 0 {
                celdas.push(unidad.descripcion().to_string());
                colores.push(tema.paleta.inactivo);
            }
            prefijos.push(String::new());
        }
        Seccion::Timers => {
            let Some(timer) = listado
                .timers
                .iter()
                .find(|timer| timer.nombre == elemento.nombre())
            else {
                return Line::from(" ");
            };
            celdas.push(timer.nombre.clone());
            colores.push(tema.paleta.texto);
            celdas.push(timer.activo().to_string());
            colores.push(if timer.activo() == "active" {
                tema.paleta.correcto
            } else {
                tema.paleta.inactivo
            });
            celdas.push(proxima_timer(listado, timer));
            colores.push(tema.paleta.texto);
            celdas.push(ultima_timer(listado, timer));
            colores.push(tema.paleta.texto);
            if anchos.len() > 4 && anchos[4] > 0 {
                celdas.push(timer.servicio_activado().unwrap_or_default());
                colores.push(tema.paleta.texto);
            }
            prefijos.push(String::new());
        }
        Seccion::Contenedores => match elemento {
            crate::app::servicios::Elemento::Proyecto(proyecto) => {
                let cuantos = listado
                    .contenedores
                    .iter()
                    .filter(|contenedor| contenedor.proyecto.as_deref() == Some(proyecto.as_str()))
                    .count();
                let mut linea = Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!(
                            "{} {proyecto} {} compose {} {cuantos}",
                            tema.glifos.desplegado,
                            tema.glifos.punto_medio,
                            tema.glifos.punto_medio
                        ),
                        Style::default()
                            .fg(tema.paleta.acento)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]);
                if let Some(proyecto) = listado
                    .proyectos
                    .iter()
                    .find(|otro| otro.nombre == *proyecto)
                {
                    if !proyecto.con_ficheros() {
                        linea.spans.push(Span::styled(
                            format!(
                                " {} sin ficheros de compose conocidos",
                                tema.glifos.punto_medio
                            ),
                            Style::default().fg(tema.paleta.inactivo),
                        ));
                    }
                }
                return linea;
            }
            crate::app::servicios::Elemento::Contenedor(nombre) => {
                let Some(contenedor) = listado
                    .contenedores
                    .iter()
                    .find(|contenedor| contenedor.nombre == *nombre)
                else {
                    return Line::from(" ");
                };
                celdas.push(contenedor.nombre.clone());
                colores.push(tema.paleta.texto);
                let (glifo, texto, color) = estado_contenedor(app, contenedor);
                celdas.push(format!("{glifo} {texto}"));
                colores.push(color);
                celdas.push(contenedor.imagen.clone());
                colores.push(tema.paleta.texto);
                if ancho >= ANCHO_PUERTOS {
                    celdas.push(contenedor.puertos.clone());
                    colores.push(tema.paleta.inactivo);
                }
                let (cpu, memoria) = consumo_de(app, contenedor);
                celdas.push(cpu);
                colores.push(tema.paleta.texto);
                celdas.push(memoria);
                colores.push(tema.paleta.texto);
                prefijos.push(String::new());
            }
            _ => return Line::from(" "),
        },
    }
    for (indice, celda) in celdas.iter().enumerate() {
        if indice >= anchos.len() || anchos[indice] == 0 {
            continue;
        }
        if indice > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            disposicion::columna(celda, usize::from(anchos[indice]), tema.ascii),
            Style::default().fg(colores.get(indice).copied().unwrap_or(tema.paleta.texto)),
        ));
    }
    let mut linea = Line::from(spans);
    let largo = linea.width();
    if largo < usize::from(ancho) {
        linea
            .spans
            .push(Span::raw(" ".repeat(usize::from(ancho) - largo)));
    }
    linea
}

/// Glifo, texto y color del estado de una unidad (con la acción en curso).
fn estado_unidad(app: &App, nombre: &str, unidad: &Unidad) -> (String, String, Color) {
    let tema = &app.tema;
    if let Some(host_id) = app.servicios_host_seleccionado() {
        let mut objetivo = magi_nucleo::servicios::Objetivo::unidad(nombre);
        objetivo.tipo = TipoObjetivo::Unidad;
        if let Some(info) = app.servicios_accion_en_curso(host_id, &objetivo) {
            return (
                tema.glifos.conectando.to_string(),
                info.accion.texto().to_string(),
                tema.paleta.acento,
            );
        }
    }
    let activo = unidad.activo();
    let (glifo, color) = match activo {
        "active" => (tema.glifos.conectado, tema.paleta.correcto),
        "failed" => (tema.glifos.error, tema.paleta.critico),
        "activating" | "deactivating" | "reloading" => (tema.glifos.conectando, tema.paleta.acento),
        _ => (tema.glifos.desconectado, tema.paleta.inactivo),
    };
    if unidad.enmascarada() {
        return (
            glifo.to_string(),
            "enmascarada".to_string(),
            tema.paleta.critico,
        );
    }
    (
        glifo.to_string(),
        acciones::texto_estado_unidad(unidad).to_string(),
        color,
    )
}

/// Glifo, texto y color del estado de un contenedor.
fn estado_contenedor(app: &App, contenedor: &Contenedor) -> (String, String, Color) {
    let tema = &app.tema;
    if let Some(host_id) = app.servicios_host_seleccionado() {
        let objetivo = magi_nucleo::servicios::Objetivo::contenedor(contenedor.nombre.clone());
        if let Some(info) = app.servicios_accion_en_curso(host_id, &objetivo) {
            return (
                tema.glifos.conectando.to_string(),
                info.accion.texto().to_string(),
                tema.paleta.acento,
            );
        }
    }
    let texto = acciones::texto_estado_contenedor(contenedor);
    let (glifo, color) = if contenedor.caido() {
        (tema.glifos.error, tema.paleta.critico)
    } else {
        match contenedor.estado {
            magi_nucleo::servicios::EstadoContenedor::EnMarcha => {
                (tema.glifos.conectado, tema.paleta.correcto)
            }
            magi_nucleo::servicios::EstadoContenedor::Reiniciando => {
                (tema.glifos.conectando, tema.paleta.acento)
            }
            magi_nucleo::servicios::EstadoContenedor::Creado
            | magi_nucleo::servicios::EstadoContenedor::Pausado
            | magi_nucleo::servicios::EstadoContenedor::Muerto
            | magi_nucleo::servicios::EstadoContenedor::Desconocido => {
                (tema.glifos.desconectado, tema.paleta.inactivo)
            }
            magi_nucleo::servicios::EstadoContenedor::Parado => {
                if contenedor.codigo_salida == Some(0) {
                    (tema.glifos.desconectado, tema.paleta.inactivo)
                } else {
                    (tema.glifos.error, tema.paleta.critico)
                }
            }
        }
    };
    let salud = if contenedor.salud == magi_nucleo::servicios::Salud::Insegura {
        format!(" {}{}", tema.glifos.error, "unhealthy")
    } else {
        String::new()
    };
    (glifo.to_string(), format!("{texto}{salud}"), color)
}

/// «desde hace N» de una unidad, con el reloj del host.
fn desde_unidad(listado: &magi_nucleo::servicios::Listado, unidad: &Unidad) -> String {
    let Some(instante) = unidad.desde_epoch(listado.reloj) else {
        return "—".to_string();
    };
    let segundos = (listado.reloj.epoch - instante).max(0) as f64;
    format!(
        "hace {}",
        crate::flota::estado::formatear_antiguedad(segundos)
    )
}

fn proxima_timer(listado: &magi_nucleo::servicios::Listado, timer: &Timer) -> String {
    let Some(instante) = timer.proxima_epoch() else {
        return "—".to_string();
    };
    let segundos = (instante - listado.reloj.epoch) as f64;
    if segundos >= 0.0 {
        format!(
            "en {}",
            crate::flota::estado::formatear_antiguedad(segundos)
        )
    } else {
        format!(
            "hace {}",
            crate::flota::estado::formatear_antiguedad(-segundos)
        )
    }
}

fn ultima_timer(listado: &magi_nucleo::servicios::Listado, timer: &Timer) -> String {
    let Some(instante) = timer.ultima_epoch(listado.reloj) else {
        return "—".to_string();
    };
    let segundos = (listado.reloj.epoch - instante).max(0) as f64;
    format!(
        "hace {}",
        crate::flota::estado::formatear_antiguedad(segundos)
    )
}

/// CPU y memoria de un contenedor según el último consumo pedido.
fn consumo_de(app: &App, contenedor: &Contenedor) -> (String, String) {
    let Some(host_id) = app.servicios_host_seleccionado() else {
        return ("—".to_string(), "—".to_string());
    };
    let Some(consumo) = app.servicios.consumos.get(&host_id) else {
        return ("—".to_string(), "—".to_string());
    };
    let Some(consumo) = consumo
        .consumos
        .iter()
        .find(|consumo| consumo.id == contenedor.id || consumo.id == contenedor.nombre)
    else {
        return ("—".to_string(), "—".to_string());
    };
    let cpu = consumo
        .cpu_pct
        .map(|pct| format!("{pct:.0} %"))
        .unwrap_or_else(|| "—".to_string());
    let memoria = consumo
        .memoria_bytes
        .map(formatear_bytes)
        .unwrap_or_else(|| "—".to_string());
    (cpu, memoria)
}

/// Bytes legibles («180 MB»).
pub fn formatear_bytes(bytes: u64) -> String {
    const UNIDADES: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut valor = bytes as f64;
    let mut unidad = 0;
    while valor >= 1024.0 && unidad + 1 < UNIDADES.len() {
        valor /= 1024.0;
        unidad += 1;
    }
    if unidad == 0 {
        format!("{bytes} B")
    } else {
        format!("{valor:.0} {}", UNIDADES[unidad])
    }
}

/// Recuento del pie de la lista.
fn linea_recuento(
    app: &App,
    listado: &magi_nucleo::servicios::Listado,
    seccion: Seccion,
) -> Line<'static> {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    let texto = match seccion {
        Seccion::Servicios => {
            let total = listado
                .unidades
                .iter()
                .filter(|unidad| !unidad.plantilla())
                .count();
            let fallidos = listado
                .unidades
                .iter()
                .filter(|unidad| unidad.activo() == "failed")
                .count();
            let vigilados = listado
                .unidades
                .iter()
                .filter(|unidad| unidad.vigilada)
                .count();
            format!("{total} servicios {punto} {fallidos} fallido(s) {punto} {vigilados} vigilados")
        }
        Seccion::Timers => format!(
            "{} timers {punto} {} activos",
            listado.timers.len(),
            listado
                .timers
                .iter()
                .filter(|timer| timer.activo() == "active")
                .count()
        ),
        Seccion::Contenedores => format!(
            "{} contenedores {punto} {} en marcha {punto} {} caídos",
            listado.contenedores.len(),
            listado
                .contenedores
                .iter()
                .filter(|contenedor| contenedor.estado.en_marcha())
                .count(),
            listado
                .contenedores
                .iter()
                .filter(|contenedor| contenedor.caido())
                .count()
        ),
    };
    Line::from(Span::styled(
        format!(" {texto}"),
        Style::default().fg(tema.paleta.inactivo),
    ))
}

// ------------------------------------------------------ toda la flota

fn dibujar_toda_la_flota(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let filas = app.servicios_filas_flota();
    let mut lineas: Vec<Line> = Vec::new();
    lineas.push(Line::from(Span::styled(
        " FALLOS DE LA FLOTA (del último sondeo de cada host, sin abrir conexiones)",
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    )));
    let columnas = [
        Columna::fija(12, 1),
        Columna::fija(11, 2),
        Columna::flexible(22, 1),
        Columna::fija(14, 3),
        Columna::fija(10, 4),
    ];
    let anchos = anchos_tabla(area.width.saturating_sub(2), &columnas, 1);
    let titulos = ["HOST", "TIPO", "NOMBRE", "ESTADO", "SONDEO"];
    let mut cabecera = vec![Span::raw("  ")];
    for (indice, titulo) in titulos.iter().enumerate() {
        if anchos[indice] == 0 {
            continue;
        }
        cabecera.push(Span::styled(
            disposicion::columna(titulo, usize::from(anchos[indice]), tema.ascii),
            Style::default().fg(tema.paleta.inactivo),
        ));
        if indice + 1 < titulos.len() {
            cabecera.push(Span::raw(" "));
        }
    }
    lineas.push(Line::from(cabecera));
    lineas.push(Line::from(Span::styled(
        tema.glifos
            .linea
            .repeat(usize::from(area.width.saturating_sub(1))),
        Style::default().fg(tema.paleta.inactivo),
    )));
    let alto_filas = usize::from(area.height.saturating_sub(4)).max(1);
    let fila_seleccionada = filas
        .iter()
        .position(
            |fila| matches!(fila, FilaFlota::Fallo(_, indice) if *indice == app.servicios.seleccion_flota),
        )
        .unwrap_or(0);
    let inicio = disposicion::ventana(
        app.servicios.desplazamiento_flota,
        fila_seleccionada,
        alto_filas,
        filas.len(),
    );
    disp.registrar(
        Lista::ServiciosTodaLaFlota,
        VentanaLista {
            inicio,
            filas: alto_filas,
            total: filas.len(),
        },
    );
    for fila in filas.iter().skip(inicio).take(alto_filas) {
        match fila {
            FilaFlota::Fallo(fallo, indice) => {
                let (glifo, color) = (tema.glifos.error, tema.paleta.critico);
                let celdas = [
                    fallo.host_nombre.clone(),
                    fallo.fallo.tipo.texto().to_string(),
                    fallo.fallo.nombre.clone(),
                    format!("{glifo} {}", fallo.fallo.estado),
                    format!(
                        "hace {}",
                        crate::flota::estado::formatear_antiguedad(fallo.antiguedad_seg)
                    ),
                ];
                let mut spans = vec![Span::raw("  ")];
                for (indice_celda, celda) in celdas.iter().enumerate() {
                    if anchos[indice_celda] == 0 {
                        continue;
                    }
                    if indice_celda > 0 {
                        spans.push(Span::raw(" "));
                    }
                    let estilo = if indice_celda == 3 {
                        Style::default().fg(color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(tema.paleta.texto)
                    };
                    spans.push(Span::styled(
                        disposicion::columna(celda, usize::from(anchos[indice_celda]), tema.ascii),
                        estilo,
                    ));
                }
                let mut linea = Line::from(spans);
                if *indice == app.servicios.seleccion_flota {
                    linea = con_fondo(linea, area.width, tema);
                }
                lineas.push(linea);
            }
            FilaFlota::Nota(texto) => {
                if texto.is_empty() {
                    lineas.push(Line::from(""));
                } else {
                    lineas.push(Line::from(Span::styled(
                        format!(
                            "  {}",
                            disposicion::recortar(
                                texto,
                                usize::from(area.width.saturating_sub(3)),
                                tema.ascii
                            )
                        ),
                        Style::default().fg(tema.paleta.inactivo),
                    )));
                }
            }
        }
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

// --------------------------------------------------------------- detalle

fn dibujar_detalle(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let Some(detalle) = app.servicios.detalle.as_ref() else {
        return;
    };
    let mut lineas: Vec<Line> = Vec::new();
    lineas.push(Line::from(vec![
        Span::styled(
            format!(" {}", detalle.objetivo.nombre),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} {}", tema.glifos.punto_medio, detalle.host_nombre),
            Style::default().fg(tema.paleta.inactivo),
        ),
    ]));
    let mut tabs = vec![Span::raw(" ")];
    for (indice, pestana) in [
        PestanaDetalle::Estado,
        PestanaDetalle::Dependencias,
        PestanaDetalle::Unidad,
    ]
    .into_iter()
    .enumerate()
    {
        if indice > 0 {
            tabs.push(Span::styled(
                format!("  {}  ", tema.glifos.punto_medio),
                Style::default().fg(tema.paleta.inactivo),
            ));
        }
        let activa = detalle.pestaña == pestana;
        let estilo = if activa {
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.inactivo)
        };
        tabs.push(Span::styled(
            format!("{} {}", indice + 1, pestana.texto()),
            estilo,
        ));
    }
    lineas.push(Line::from(tabs));
    lineas.push(Line::from(Span::styled(
        tema.glifos
            .linea
            .repeat(usize::from(area.width.saturating_sub(1))),
        Style::default().fg(tema.paleta.inactivo),
    )));
    let cuerpo = lineas_detalle(app, ancho_contenido(area.width));
    let alto_cuerpo = usize::from(area.height).saturating_sub(lineas.len()).max(1);
    let inicio = disposicion::ventana(detalle.desplazamiento, 0, alto_cuerpo, cuerpo.len());
    let inicio = inicio.min(cuerpo.len().saturating_sub(alto_cuerpo));
    disp.registrar(
        Lista::ServiciosDetalle,
        VentanaLista {
            inicio,
            filas: alto_cuerpo,
            total: cuerpo.len(),
        },
    );
    for linea in cuerpo.into_iter().skip(inicio).take(alto_cuerpo) {
        lineas.push(linea);
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

fn ancho_contenido(ancho: u16) -> usize {
    usize::from(ancho.saturating_sub(2)).max(10)
}

/// Líneas del detalle según la pestaña.
fn lineas_detalle(app: &App, ancho: usize) -> Vec<Line<'static>> {
    let tema = &app.tema;
    let Some(detalle) = app.servicios.detalle.as_ref() else {
        return Vec::new();
    };
    if let Some(error) = &detalle.error {
        return vec![Line::from(Span::styled(
            format!(" ✕ {error}"),
            Style::default().fg(tema.paleta.critico),
        ))];
    }
    let Some(contenido) = &detalle.detalle else {
        return vec![Line::from(Span::styled(
            " cargando el detalle…",
            Style::default().fg(tema.paleta.inactivo),
        ))];
    };
    let punto = tema.glifos.punto_medio;
    let mut lineas: Vec<Line> = Vec::new();
    match detalle.pestaña {
        PestanaDetalle::Estado => {
            if let Some(contenedor) = &contenido.contenedor {
                let campo = |etiqueta: &str, valor: String| {
                    Line::from(vec![
                        Span::styled(
                            format!(" {etiqueta:<12}"),
                            Style::default().fg(tema.paleta.inactivo),
                        ),
                        Span::styled(valor, Style::default().fg(tema.paleta.texto)),
                    ])
                };
                lineas.push(campo("id", contenedor.id.clone()));
                lineas.push(campo("nombre", contenedor.nombre.clone()));
                lineas.push(campo("imagen", contenedor.imagen.clone()));
                lineas.push(campo("creado", contenedor.creado.clone()));
                lineas.push(campo(
                    "estado",
                    format!(
                        "{} ({})",
                        contenedor.estado.texto(),
                        contenedor
                            .codigo_salida
                            .map(|codigo| codigo.to_string())
                            .unwrap_or_else(|| "—".to_string())
                    ),
                ));
                lineas.push(campo("reinicios", contenedor.reinicios.to_string()));
                lineas.push(campo(
                    "salud",
                    if contenedor.salud.texto().is_empty() {
                        "—".to_string()
                    } else {
                        contenedor.salud.texto().to_string()
                    },
                ));
                lineas.push(campo(
                    "puertos",
                    if contenedor.puertos.is_empty() {
                        "—".to_string()
                    } else {
                        contenedor.puertos.join(", ")
                    },
                ));
                lineas.push(campo(
                    "montajes",
                    if contenedor.montajes.is_empty() {
                        "—".to_string()
                    } else {
                        contenedor.montajes.join(", ")
                    },
                ));
                lineas.push(campo(
                    "redes",
                    if contenedor.redes.is_empty() {
                        "—".to_string()
                    } else {
                        contenedor.redes.join(", ")
                    },
                ));
                lineas.push(campo(
                    "proyecto",
                    formato_proyecto(contenedor.proyecto.as_deref(), contenedor.orden.as_deref()),
                ));
            } else {
                let props = &contenido.props;
                let prop = |clave: &str| props.get(clave).cloned().unwrap_or_default();
                lineas.push(Line::from(Span::styled(
                    format!(" {}", prop("Description")),
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD),
                )));
                let estado = format!("{} ({})", prop("ActiveState"), prop("SubState"));
                lineas.push(campo_detalle(tema, "estado", estado));
                if let Some(listado_host) = app.servicios_listado(detalle.host_id) {
                    let reloj = listado_host.listado.reloj;
                    if let Ok(monotonico) = prop("ActiveEnterTimestampMonotonic").parse::<u64>() {
                        if let Some(instante) = reloj.instante_desde_monotonico(monotonico) {
                            let segundos = (reloj.epoch - instante).max(0) as f64;
                            lineas.push(campo_detalle(
                                tema,
                                "desde",
                                format!(
                                    "hace {}",
                                    crate::flota::estado::formatear_antiguedad(segundos)
                                ),
                            ));
                        }
                    }
                }
                let principal = prop("ExecMainPID");
                if !principal.is_empty() {
                    lineas.push(campo_detalle(tema, "PID", principal));
                }
                if let Ok(bytes) = prop("MemoryCurrent").parse::<u64>() {
                    if bytes != u64::MAX {
                        lineas.push(campo_detalle(tema, "memoria", formatear_bytes(bytes)));
                    }
                }
                if let Some(fichero) = contenido.props.get("FragmentPath") {
                    if !fichero.is_empty() {
                        lineas.push(campo_detalle(tema, "unidad", fichero.clone()));
                    }
                }
                let arranque = prop("UnitFileState");
                if !arranque.is_empty() {
                    lineas.push(campo_detalle(
                        tema,
                        "arranque",
                        magi_nucleo::servicios::EstadoArranque::desde_texto(&arranque)
                            .texto()
                            .to_string(),
                    ));
                }
                lineas.push(campo_detalle(
                    tema,
                    "recargar",
                    if prop("CanReload") == "yes" {
                        "sí".to_string()
                    } else {
                        "no".to_string()
                    },
                ));
                if acciones::es_critica(&app.config.servicios.criticas, &detalle.objetivo.nombre) {
                    lineas.push(campo_detalle(
                        tema,
                        "crítica",
                        "sí: puede cortar la conexión".to_string(),
                    ));
                }
                if detalle.objetivo.tipo == TipoObjetivo::Timer {
                    if let Some(listado_host) = app.servicios_listado(detalle.host_id) {
                        if let Some(timer) = listado_host
                            .listado
                            .timers
                            .iter()
                            .find(|timer| timer.nombre == detalle.objetivo.nombre)
                        {
                            lineas.push(campo_detalle(
                                tema,
                                "próxima",
                                proxima_timer(&listado_host.listado, timer),
                            ));
                            lineas.push(campo_detalle(
                                tema,
                                "última",
                                ultima_timer(&listado_host.listado, timer),
                            ));
                            if let Some(servicio) = timer.servicio_activado() {
                                let estado = listado_host
                                    .listado
                                    .unidades
                                    .iter()
                                    .find(|unidad| unidad.nombre == servicio)
                                    .map(|unidad| {
                                        format!(
                                            "{servicio} ({})",
                                            acciones::texto_estado_unidad(unidad)
                                        )
                                    })
                                    .unwrap_or(servicio);
                                lineas.push(campo_detalle(tema, "servicio", estado));
                            }
                        }
                    }
                } else if let Some(listado_host) = app.servicios_listado(detalle.host_id) {
                    if let Some(unidad) = listado_host
                        .listado
                        .unidades
                        .iter()
                        .find(|unidad| unidad.nombre == detalle.objetivo.nombre)
                    {
                        if unidad.vigilada {
                            lineas.push(campo_detalle(tema, "vigilada", "★".to_string()));
                        }
                    }
                }
                if let Some(texto) =
                    app.servicios_comprobacion_texto(detalle.host_id, &detalle.objetivo)
                {
                    lineas.push(campo_detalle(tema, "comprueba", texto));
                }
            }
        }
        PestanaDetalle::Dependencias => {
            lineas.push(Line::from(Span::styled(
                " NECESITA",
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            )));
            if contenido.necesita.is_empty() {
                lineas.push(Line::from(Span::styled(
                    "   (ninguna)",
                    Style::default().fg(tema.paleta.inactivo),
                )));
            }
            for dependencia in &contenido.necesita {
                lineas.push(Line::from(Span::styled(
                    format!("   {dependencia}"),
                    Style::default().fg(tema.paleta.texto),
                )));
            }
            lineas.push(Line::from(""));
            lineas.push(Line::from(Span::styled(
                " LA NECESITAN",
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            )));
            if contenido.necesitada.is_empty() {
                lineas.push(Line::from(Span::styled(
                    "   (ninguna)",
                    Style::default().fg(tema.paleta.inactivo),
                )));
            }
            for dependencia in &contenido.necesitada {
                lineas.push(Line::from(Span::styled(
                    format!("   {dependencia}"),
                    Style::default().fg(tema.paleta.texto),
                )));
            }
            lineas.push(Line::from(""));
            lineas.push(Line::from(Span::styled(
                format!(
                    " {} pestaña Tab cambia entre Estado, Dependencias y Unidad",
                    tema.glifos.tab
                ),
                Style::default().fg(tema.paleta.inactivo),
            )));
        }
        PestanaDetalle::Unidad => {
            let etiqueta = if contenido.contenedor.is_some() {
                "inspect"
            } else {
                "systemctl cat"
            };
            lineas.push(Line::from(Span::styled(
                format!(" {etiqueta} {punto} solo lectura"),
                Style::default().fg(tema.paleta.inactivo),
            )));
            for linea in contenido.texto.lines() {
                lineas.push(Line::from(Span::styled(
                    disposicion::recortar(linea, ancho, tema.ascii),
                    Style::default().fg(tema.paleta.texto),
                )));
            }
        }
    }
    lineas
}

fn campo_detalle(tema: &Tema, etiqueta: &str, valor: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {etiqueta:<12}"),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled(valor, Style::default().fg(tema.paleta.texto)),
    ])
}

fn formato_proyecto(proyecto: Option<&str>, orden: Option<&str>) -> String {
    match (proyecto, orden) {
        (None, _) => "—".to_string(),
        (Some(proyecto), Some(orden)) => format!("{proyecto} (orden {orden})"),
        (Some(proyecto), None) => proyecto.to_string(),
    }
}

// ------------------------------------------------------------------ logs

/// Una fila del panel de logs: una línea o la marca de las omitidas.
enum EntradaLog<'a> {
    Linea(&'a LineaAnillo),
    Omitidas(u32),
}

/// Panel de logs en vivo: ocupa la pantalla entera, con su cabecera y su pie.
pub fn dibujar_logs(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    let Some(logs) = app.servicios.logs.as_ref() else {
        return;
    };
    let titulo = format!(
        " MAGI {} SERVICIOS {} LOGS ",
        tema.glifos.punto_medio, tema.glifos.punto_medio
    );
    let estado = if logs.abriendo {
        "abriendo…".to_string()
    } else if logs.pausado {
        format!(
            "en pausa {} {} nuevas",
            tema.glifos.punto_medio, logs.nuevas
        )
    } else if let Some(motivo) = logs.motivo_fin {
        format!("terminada: {}", motivo.texto())
    } else {
        format!("{} siguiendo", tema.glifos.conectado)
    };
    let derecha = format!(
        " {} {} {} {} {} ",
        logs.objetivo.nombre,
        tema.glifos.punto_medio,
        logs.host_nombre,
        tema.glifos.punto_medio,
        estado
    );
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            titulo.clone(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    if titulo_derecho_cabe(area.width, &titulo, &derecha) {
        bloque = bloque.title_top(titulo_derecho(derecha, tema));
    }
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height < 3 || interior.width < 6 {
        return;
    }
    let mut entradas: Vec<EntradaLog> = Vec::new();
    for anillo in logs.visibles() {
        if anillo.omitidas_antes > 0 {
            entradas.push(EntradaLog::Omitidas(anillo.omitidas_antes));
        }
        entradas.push(EntradaLog::Linea(anillo));
    }
    let omitidas_totales: u32 = logs
        .anillo
        .iter()
        .map(|anillo| anillo.omitidas_antes)
        .sum::<u32>();
    let mut cabecera: Vec<Line> = Vec::new();
    let filtro = if logs.filtro.is_empty() {
        "sin filtro".to_string()
    } else {
        format!("{}{}", logs.filtro, crate::ui::tecla(tema, "▏", "|"))
    };
    let nivel = nivel_texto(logs);
    let recuento = format!(
        "{} líneas {} {} omitidas",
        logs.total, tema.glifos.punto_medio, omitidas_totales
    );
    cabecera.push(Line::from(vec![
        Span::styled(
            " / ",
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(filtro, Style::default().fg(tema.paleta.texto)),
        Span::raw("   "),
        Span::styled(
            format!("n {nivel}"),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::raw("   "),
        Span::styled(recuento, Style::default().fg(tema.paleta.inactivo)),
    ]));
    cabecera.push(Line::from(Span::styled(
        tema.glifos
            .linea
            .repeat(usize::from(interior.width.saturating_sub(1))),
        Style::default().fg(tema.paleta.inactivo),
    )));
    let mut pie: Vec<Line> = vec![Line::from(Span::styled(
        format!(
            " / texto  n nivel  espacio pausa  fin al final  {} desplazar  t pestaña  esc cerrar",
            tema.glifos.arriba
        ),
        Style::default().fg(tema.paleta.inactivo),
    ))];
    if logs.abriendo {
        pie.clear();
        pie.push(Line::from(Span::styled(
            " abriendo la suscripción… (esc cerrar)",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let alto_extra = if logs.abriendo || logs.motivo_fin.is_some() {
        1u16
    } else {
        0
    };
    let alto_cuerpo = interior
        .height
        .saturating_sub(cabecera.len() as u16 + 1 + alto_extra)
        .max(1);
    let total = entradas.len();
    let inicio = total
        .saturating_sub(usize::from(alto_cuerpo) + logs.desde_final)
        .min(total.saturating_sub(usize::from(alto_cuerpo)));
    disp.registrar(
        Lista::ServiciosLogs,
        VentanaLista {
            inicio,
            filas: usize::from(alto_cuerpo),
            total,
        },
    );
    let mut cuerpo: Vec<Line> = Vec::new();
    for entrada in entradas
        .into_iter()
        .skip(inicio)
        .take(usize::from(alto_cuerpo))
    {
        match entrada {
            EntradaLog::Linea(anillo) => {
                cuerpo.push(linea_log(tema, &anillo.linea));
            }
            EntradaLog::Omitidas(cuantas) => {
                cuerpo.push(Line::from(Span::styled(
                    format!(
                        " {} {} {} líneas omitidas (el cliente iba lento)",
                        " ".repeat(11),
                        tema.glifos.punto_medio,
                        cuantas
                    ),
                    Style::default().fg(tema.paleta.acento),
                )));
            }
        }
    }
    if let Some(motivo) = logs.motivo_fin {
        cuerpo.push(Line::from(Span::styled(
            format!(
                " {} {} {} (las líneas se quedan; esc para cerrar)",
                " ".repeat(11),
                tema.glifos.error,
                motivo.texto()
            ),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    let mut lineas = cabecera;
    lineas.extend(cuerpo);
    lineas.extend(pie);
    marco.render_widget(Paragraph::new(lineas), interior);
}

/// Texto del nivel del filtro.
fn nivel_texto(logs: &PanelLogs) -> String {
    if logs.es_contenedor() {
        match logs.nivel {
            NivelLog::Todo => "todo".to_string(),
            _ => "solo stderr".to_string(),
        }
    } else {
        match logs.nivel {
            NivelLog::Todo => "todo".to_string(),
            otro => format!("{} o más", otro.texto()),
        }
    }
}

/// Una línea del diario o de un contenedor, con hora, glifo y color.
fn linea_log(tema: &Tema, linea: &magi_nucleo::servicios::LineaLog) -> Line<'static> {
    let hora = linea
        .hora
        .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0))
        .map(|fecha| {
            fecha
                .with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "--:--:--".to_string());
    let (glifo, color) = if linea.stderr || linea.es_error() {
        (tema.glifos.error, tema.paleta.critico)
    } else if linea.es_aviso() {
        ("!", tema.paleta.acento)
    } else {
        (tema.glifos.marca, tema.paleta.texto)
    };
    Line::from(vec![
        Span::styled(
            format!(" {hora} "),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled(
            format!("{glifo} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(linea.texto.clone(), Style::default().fg(color)),
    ])
}

// --------------------------------------------------------------- diálogos

/// Pinta el diálogo propio de Servicios abierto, si lo hay.
pub fn dibujar_dialogo(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoServicio,
    disp: &mut Disposicion,
) {
    match dialogo {
        DialogoServicio::Confirmar(confirmacion) => {
            dibujar_confirmacion(marco, area, app, confirmacion, disp)
        }
        DialogoServicio::Resultado(resultado) => {
            dibujar_resultado(marco, area, app, resultado, disp)
        }
        DialogoServicio::Sudo(pedido) => dibujar_sudo(marco, area, app, pedido, disp),
        DialogoServicio::Comprobacion(editor) => {
            dibujar_editor_comprobacion(marco, area, app, editor, disp)
        }
        DialogoServicio::BorrarComprobacion {
            host_nombre,
            objetivo,
            ..
        } => {
            let lineas = vec![
                Line::from(Span::styled(
                    format!(
                        "¿Borrar la comprobación propia de {} en {host_nombre}?",
                        objetivo.nombre
                    ),
                    Style::default().fg(tema_de(app).paleta.texto),
                )),
                Line::from(Span::styled(
                    "Deja lápida para la sincronización; no se pierde en otros equipos.",
                    Style::default().fg(tema_de(app).paleta.inactivo),
                )),
            ];
            let pie = vec![Line::from(Span::styled(
                " ↵ borrar · esc cancelar",
                Style::default().fg(tema_de(app).paleta.inactivo),
            ))];
            let _ = hoja(
                marco,
                area,
                app,
                "BORRAR COMPROBACIÓN",
                lineas,
                pie,
                72,
                disp,
            );
        }
    }
}

fn tema_de(app: &App) -> &Tema {
    &app.tema
}

/// Una hoja centrada con título, cuerpo y pie; devuelve su zona.
#[allow(clippy::too_many_arguments)]
fn hoja(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    titulo: &str,
    lineas: Vec<Line<'static>>,
    pie: Vec<Line<'static>>,
    ancho_deseado: u16,
    disp: &mut Disposicion,
) -> Rect {
    let tema = &app.tema;
    let ancho = ancho_deseado
        .min(disposicion::ANCHO_MAX_DIALOGO)
        .min(area.width.saturating_sub(2))
        .max(area.width.min(10));
    let alto = (lineas.len() + pie.len() + 2) as u16;
    let zona = crate::ui::centrar(area, ancho, alto);
    marco.render_widget(ratatui::widgets::Clear, zona);
    let bloque = crate::ui::bloque(titulo, tema);
    let interior = bloque.inner(zona);
    marco.render_widget(bloque, zona);
    let mut todas = lineas;
    todas.extend(pie);
    marco.render_widget(Paragraph::new(todas).wrap(Wrap { trim: false }), interior);
    disp.registrar(
        Lista::Modal,
        VentanaLista {
            inicio: 0,
            filas: usize::from(interior.height),
            total: 0,
        },
    );
    zona
}

fn dibujar_confirmacion(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    confirmacion: &ConfirmacionServicio,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let pendiente = &confirmacion.pendiente;
    let mut lineas = vec![Line::from(Span::styled(
        format!(
            "¿{} {} en {}?",
            pendiente.accion.texto(),
            pendiente.objetivo.nombre,
            pendiente.host_nombre
        ),
        Style::default()
            .fg(tema.paleta.texto)
            .add_modifier(Modifier::BOLD),
    ))];
    if let Some(comprobacion) = &confirmacion.comprobacion {
        lineas.push(Line::from(Span::styled(
            format!("Antes: {comprobacion}"),
            Style::default().fg(tema.paleta.acento),
        )));
    }
    if let Some(critica) = &confirmacion.critica {
        lineas.push(Line::from(Span::styled(
            format!("⚑ {critica}"),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    let pie = vec![Line::from(Span::styled(
        format!(
            " {} {} · esc cancelar",
            tema.glifos.intro,
            pendiente.accion.texto()
        ),
        Style::default().fg(tema.paleta.inactivo),
    ))];
    let _ = hoja(marco, area, app, "CONFIRMACIÓN", lineas, pie, 72, disp);
}

fn dibujar_resultado(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    resultado: &ResultadoDialogo,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let datos = &resultado.resultado;
    let (titulo, texto, color) = match datos.resultado {
        magi_nucleo::servicios::ResultadoAccion::Hecho => (
            "✓ HECHO",
            format!("hecho en {:.1} s", datos.segundos),
            tema.paleta.correcto,
        ),
        magi_nucleo::servicios::ResultadoAccion::Fallido => (
            "✕ FALLIDO",
            if datos.mensaje.is_empty() {
                "no volvió al estado esperado".to_string()
            } else {
                datos.mensaje.clone()
            },
            tema.paleta.critico,
        ),
        magi_nucleo::servicios::ResultadoAccion::SinConfirmar => (
            "◐ SIN CONFIRMAR",
            "la orden sigue en el host: r para ver el estado".to_string(),
            tema.paleta.acento,
        ),
        magi_nucleo::servicios::ResultadoAccion::RechazadaConfiguracion => (
            "✕ RECHAZADA POR LA CONFIGURACIÓN",
            "no se ha ejecutado la acción".to_string(),
            tema.paleta.critico,
        ),
        magi_nucleo::servicios::ResultadoAccion::Error => (
            "✕ ERROR",
            if datos.mensaje.is_empty() {
                "no se pudo completar la acción".to_string()
            } else {
                datos.mensaje.clone()
            },
            tema.paleta.critico,
        ),
    };
    let mut lineas = vec![Line::from(Span::styled(
        format!("{titulo} {}{} {texto}", tema.glifos.punto_medio, ""),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))];
    if let Some(comprobacion) = &datos.comprobacion {
        lineas.push(Line::from(Span::styled(
            format!("Comprobación: {comprobacion}"),
            Style::default().fg(tema.paleta.acento),
        )));
    }
    if !datos.lineas.is_empty() {
        let etiqueta =
            if datos.resultado == magi_nucleo::servicios::ResultadoAccion::RechazadaConfiguracion {
                "Salida de la comprobación:"
            } else {
                "Últimas líneas del diario:"
            };
        lineas.push(Line::from(Span::styled(
            etiqueta,
            Style::default().fg(tema.paleta.inactivo),
        )));
        for linea in &datos.lineas {
            lineas.push(Line::from(Span::styled(
                disposicion::recortar(
                    &format!("  {linea}"),
                    usize::from(area.width.saturating_sub(6)),
                    tema.ascii,
                ),
                Style::default().fg(tema.paleta.texto),
            )));
        }
    }
    let mut pie = format!(" l logs · {} detalle · esc cerrar", tema.glifos.intro);
    if datos.resultado == magi_nucleo::servicios::ResultadoAccion::RechazadaConfiguracion {
        pie = format!(" c editar comprobación · {pie}");
    }
    let pie = vec![Line::from(Span::styled(
        pie,
        Style::default().fg(tema.paleta.inactivo),
    ))];
    let titulo = format!(
        "RESULTADO · {} {} · {}",
        resultado.pendiente.accion.texto(),
        resultado.pendiente.objetivo.nombre,
        resultado.host_nombre
    );
    let _ = hoja(marco, area, app, &titulo, lineas, pie, 84, disp);
}

fn dibujar_sudo(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    pedido: &SudoPedido,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let usuario = pedido
        .usuario
        .clone()
        .unwrap_or_else(|| "usuario".to_string());
    let mut lineas = vec![
        Line::from(Span::styled(
            format!("Contraseña de sudo de {usuario}@{}", pedido.host_nombre),
            Style::default()
                .fg(tema.paleta.texto)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                asteriscos(pedido.campo.texto.chars().count()),
                Style::default().fg(tema.paleta.acento),
            ),
            Span::styled(
                crate::ui::tecla(tema, "▏", "|"),
                Style::default().fg(tema.paleta.acento),
            ),
        ]),
    ];
    if let Some(error) = &pedido.error {
        lineas.push(Line::from(Span::styled(
            format!("✕ {error}"),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    lineas.push(Line::from(Span::styled(
        format!(
            "se recuerda {} min sin uso; nunca se guarda",
            app.config.servicios.sudo_memoria_min
        ),
        Style::default().fg(tema.paleta.inactivo),
    )));
    let pie = vec![Line::from(Span::styled(
        format!(
            " {} comprobar · esc cancelar · intento {} de 3",
            tema.glifos.intro,
            pedido.intento + 1
        ),
        Style::default().fg(tema.paleta.inactivo),
    ))];
    let _ = hoja(
        marco,
        area,
        app,
        "CONTRASEÑA DE SUDO",
        lineas,
        pie,
        64,
        disp,
    );
}

fn asteriscos(cuantos: usize) -> String {
    "*".repeat(cuantos)
}

fn dibujar_editor_comprobacion(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    editor: &EditorComprobacion,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let cursor = crate::ui::tecla(tema, "▏", "|");
    let mut lineas: Vec<Line> = Vec::new();
    let foco_objetivo = editor.objetivo.is_none() && editor.foco == 0;
    if let Some(objetivo) = &editor.objetivo {
        lineas.push(Line::from(Span::styled(
            format!("Objetivo: {} ({})", objetivo.nombre, objetivo.tipo.texto()),
            Style::default().fg(tema.paleta.texto),
        )));
    } else {
        lineas.push(Line::from(vec![
            Span::styled("Objetivo: ", Style::default().fg(tema.paleta.acento)),
            Span::styled(
                format!(
                    "{}{}",
                    editor.objetivo_campo.texto,
                    if foco_objetivo { cursor } else { "" }
                ),
                Style::default().fg(tema.paleta.texto),
            ),
        ]));
    }
    let foco_comando = editor.objetivo.is_some() || editor.foco == 1;
    lineas.push(Line::from(vec![
        Span::styled("Comando:  ", Style::default().fg(tema.paleta.acento)),
        Span::styled(
            format!(
                "{}{}",
                editor.comando.texto,
                if foco_comando { cursor } else { "" }
            ),
            Style::default().fg(tema.paleta.texto),
        ),
    ]));
    if let Some(error) = &editor.error {
        lineas.push(Line::from(Span::styled(
            format!("✕ {error}"),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    lineas.push(Line::from(Span::styled(
        "Se ejecuta con sh -c y los privilegios de la acción, justo antes.",
        Style::default().fg(tema.paleta.inactivo),
    )));
    let mut pie = format!(" {} guardar · esc cancelar", tema.glifos.intro);
    if editor.existente.is_some() {
        pie = format!(" {pie} · x borrar");
    }
    let pie = vec![Line::from(Span::styled(
        pie,
        Style::default().fg(tema.paleta.inactivo),
    ))];
    let _ = hoja(
        marco,
        area,
        app,
        "COMPROBACIÓN PROPIA",
        lineas,
        pie,
        72,
        disp,
    );
}
