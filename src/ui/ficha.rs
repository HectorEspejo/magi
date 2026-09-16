use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, CampoFicha, Ficha};
use crate::modelo::Tunel;
use crate::protocolo::EstadoTunelRemoto;
use crate::tema::Tema;
use crate::ui::componentes::{casilla, estilo_campo, AreaTexto, Desplegable};

const ANCHO_DESPLEGABLE: u16 = 34;
const ALTO_MAXIMO_DESPLEGABLE: usize = 8;
/// Filas del formulario: los campos y sus cabeceras. No cambian.
const ALTO_CAMPOS: u16 = 17;
/// Tope de filas de túnel que muestra el bloque.
const MAX_FILAS_TUNELES: usize = 4;

/// Línea del formulario en la que vive cada campo. El bloque de túneles no
/// vive en el formulario sino en su propia región, así que no tiene línea.
pub fn linea_de(campo: CampoFicha) -> u16 {
    match campo {
        CampoFicha::Nombre => 2,
        CampoFicha::Direccion => 3,
        CampoFicha::Puerto | CampoFicha::Grupo => 4,
        CampoFicha::Etiquetas => 5,
        CampoFicha::Usuario => 8,
        CampoFicha::Identidad => 9,
        CampoFicha::Salto => 10,
        CampoFicha::Multiplexar => 13,
        CampoFicha::Mantener | CampoFicha::Keepalive => 14,
        CampoFicha::Servicios | CampoFicha::Opciones => 17,
        CampoFicha::Tuneles => 0,
    }
}

fn etiqueta_desplegable(campo: CampoFicha, ficha: &Ficha) -> Option<&Desplegable> {
    match campo {
        CampoFicha::Grupo => Some(&ficha.grupo),
        CampoFicha::Identidad => Some(&ficha.identidad),
        CampoFicha::Salto => Some(&ficha.salto),
        _ => None,
    }
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App) {
    let tema = &app.tema;
    let Some(ficha) = &app.ficha else {
        return;
    };
    let titulo = if let Some(id) = ficha.host_id {
        format!("MAGI · HOST · {} · editar · #{id}", ficha.titulo)
    } else {
        format!("MAGI · HOST · {} · nuevo", ficha.titulo)
    };
    let bloque = super::bloque(&titulo, tema);
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height == 0 {
        return;
    }
    // El bloque de túneles va entre los campos y las áreas de texto; se
    // recorta a lo que quede libre para no dejar sin sitio a los servicios.
    let alto_tuneles =
        alto_bloque_tuneles(app, ficha).min(interior.height.saturating_sub(ALTO_CAMPOS + 3));
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(ALTO_CAMPOS),
            Constraint::Length(alto_tuneles),
            Constraint::Min(3),
        ])
        .split(interior);
    let lineas = construir_lineas(app, ficha);
    marco.render_widget(Paragraph::new(lineas), trozos[0]);
    dibujar_tuneles(marco, trozos[1], app, ficha);

    let columnas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(trozos[2]);
    dibujar_area_texto(
        marco,
        columnas[0],
        " SERVICIOS (systemd, una por línea) ",
        &ficha.servicios,
        ficha.campo == CampoFicha::Servicios,
        tema,
    );
    dibujar_area_texto(
        marco,
        columnas[1],
        " OPCIONES EXTRA (ssh_config, una por línea) ",
        &ficha.opciones,
        ficha.campo == CampoFicha::Opciones,
        tema,
    );

    // Desplegable abierto.
    if let Some(campo) = ficha.desplegable_abierto {
        if let Some(desplegable) = etiqueta_desplegable(campo, ficha) {
            let x = if campo == CampoFicha::Grupo {
                interior.x + 32
            } else {
                interior.x + 17
            };
            let y = interior.y + linea_de(campo) + 1;
            dibujar_desplegable(marco, (x, y), desplegable, ANCHO_DESPLEGABLE, tema);
        }
    }
    // Sugerencias de etiquetas.
    if ficha.campo == CampoFicha::Etiquetas && !ficha.sugerencias.is_empty() {
        let x = interior.x + 17;
        let y = interior.y + linea_de(CampoFicha::Etiquetas) + 1;
        dibujar_sugerencias(marco, (x, y), ficha, tema);
    }
}

fn construir_lineas(app: &App, ficha: &Ficha) -> Vec<Line<'static>> {
    let tema = &app.tema;
    let activo = |campo: CampoFicha| ficha.campo == campo;
    let estilo = |campo: CampoFicha| estilo_campo(tema, activo(campo));
    let mut lineas: Vec<Line<'static>> = Vec::with_capacity(17);
    lineas.push(Line::from(""));
    lineas.push(cabecera("IDENTIFICACIÓN", tema));
    lineas.push(linea_campo(
        "Nombre",
        ficha
            .nombre
            .span(activo(CampoFicha::Nombre), estilo(CampoFicha::Nombre)),
        activo(CampoFicha::Nombre),
        tema,
    ));
    lineas.push(linea_campo(
        "Dirección",
        ficha
            .direccion
            .span(activo(CampoFicha::Direccion), estilo(CampoFicha::Direccion)),
        activo(CampoFicha::Direccion),
        tema,
    ));
    lineas.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{:<13}", "Puerto"),
            Style::default().fg(if activo(CampoFicha::Puerto) {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
        Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
        ficha
            .puerto
            .span(activo(CampoFicha::Puerto), estilo(CampoFicha::Puerto)),
        Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
        Span::raw("     "),
        Span::styled(
            "Grupo",
            Style::default().fg(if activo(CampoFicha::Grupo) {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
        Span::raw(" "),
        valor_desplegable(&ficha.grupo, activo(CampoFicha::Grupo), tema, 26),
    ]));
    lineas.push(linea_campo(
        "Etiquetas",
        ficha
            .etiquetas
            .span(activo(CampoFicha::Etiquetas), estilo(CampoFicha::Etiquetas)),
        activo(CampoFicha::Etiquetas),
        tema,
    ));
    lineas.push(Line::from(""));
    lineas.push(cabecera("ACCESO", tema));
    lineas.push(linea_campo(
        "Usuario",
        ficha
            .usuario
            .span(activo(CampoFicha::Usuario), estilo(CampoFicha::Usuario)),
        activo(CampoFicha::Usuario),
        tema,
    ));
    lineas.push(linea_campo(
        "Identidad",
        aviso_o_valor_identidad(app, ficha),
        activo(CampoFicha::Identidad),
        tema,
    ));
    lineas.push(linea_campo(
        "Salto vía",
        valor_desplegable(&ficha.salto, activo(CampoFicha::Salto), tema, 40),
        activo(CampoFicha::Salto),
        tema,
    ));
    lineas.push(Line::from(""));
    lineas.push(cabecera("AL CONECTAR", tema));
    lineas.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Multiplexar",
            Style::default().fg(if activo(CampoFicha::Multiplexar) {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
        Span::raw("   "),
        Span::styled(
            casilla(ficha.multiplexar).to_string(),
            estilo(CampoFicha::Multiplexar),
        ),
        Span::styled(
            " ControlMaster auto al exportar",
            Style::default().fg(tema.paleta.texto),
        ),
    ]));
    lineas.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Mantener",
            Style::default().fg(
                if activo(CampoFicha::Mantener) || activo(CampoFicha::Keepalive) {
                    tema.paleta.acento
                } else {
                    tema.paleta.inactivo
                },
            ),
        ),
        Span::raw("      "),
        Span::styled(
            casilla(ficha.mantener).to_string(),
            estilo(CampoFicha::Mantener),
        ),
        Span::styled(" keepalive cada ", Style::default().fg(tema.paleta.texto)),
        Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
        ficha.keepalive.span(
            activo(CampoFicha::Keepalive) && ficha.mantener,
            estilo(CampoFicha::Keepalive),
        ),
        Span::styled(" ] s", Style::default().fg(tema.paleta.texto)),
    ]));
    lineas.push(Line::from(""));
    lineas
}

// ------------------------------------------------------------------ túneles

/// Filas que necesita el bloque: cabecera, filas de túnel (una aunque no haya,
/// para el aviso de vacío) y el aviso de reenvíos sueltos.
fn alto_bloque_tuneles(app: &App, ficha: &Ficha) -> u16 {
    let filas = match ficha.host_id {
        Some(host_id) => app.tuneles_de_host(host_id).len().min(MAX_FILAS_TUNELES),
        None => 0,
    };
    let aviso = u16::from(reenvios_en_opciones(ficha) > 0);
    1 + filas.max(1) as u16 + aviso
}

fn dibujar_tuneles(marco: &mut Frame, area: Rect, app: &App, ficha: &Ficha) {
    if area.height == 0 || area.width < 12 {
        return;
    }
    let tema = &app.tema;
    let enfocado = ficha.campo == CampoFicha::Tuneles;
    let reenvios = reenvios_en_opciones(ficha);
    let aviso = u16::from(reenvios > 0);
    let mut lineas = vec![cabecera_tuneles(area.width, enfocado, tema)];

    match ficha.host_id {
        // Un host sin guardar no puede tener túneles colgando.
        None => lineas.push(Line::from(Span::styled(
            "    guarda el host para añadir túneles",
            Style::default().fg(tema.paleta.inactivo),
        ))),
        Some(host_id) => {
            let filas = app.tuneles_de_host(host_id);
            if filas.is_empty() {
                lineas.push(Line::from(Span::styled(
                    "    aún no hay túneles: n para crear uno",
                    Style::default().fg(tema.paleta.inactivo),
                )));
            } else {
                // La ventana sigue a la selección con el alto que quede.
                let altura = (area.height.saturating_sub(1 + aviso) as usize).max(1);
                let seleccionado = ficha.indice_tunel_visible(filas.len());
                let inicio = inicio_ventana(seleccionado, filas.len(), altura);
                let ancho_nombre = filas
                    .iter()
                    .map(|tunel| tunel.nombre.chars().count())
                    .max()
                    .unwrap_or(8)
                    .clamp(8, 16);
                // Sangrías, marca, tipo y separadores: el resto es el tramo.
                let ancho_tramo = (area.width as usize)
                    .saturating_sub(19 + ancho_nombre)
                    .max(10);
                for (posicion, tunel) in filas.iter().enumerate().skip(inicio).take(altura) {
                    lineas.push(linea_tunel(
                        app,
                        tunel,
                        enfocado && posicion == seleccionado,
                        ancho_tramo,
                        ancho_nombre,
                    ));
                }
            }
        }
    }
    if reenvios > 0 {
        lineas.push(linea_reenvios(reenvios, tema));
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

/// «TÚNELES» con las teclas a la derecha: sin el foco solo el alta, con el
/// foco todas las del bloque.
fn cabecera_tuneles(ancho: u16, enfocado: bool, tema: &Tema) -> Line<'static> {
    let etiqueta = "TÚNELES";
    let pista = if enfocado {
        "n nuevo · e editar · x borrar · a automático"
    } else {
        "n nuevo"
    };
    let hueco = (ancho as usize).saturating_sub(2 + etiqueta.chars().count() + 1);
    let pista = crate::ui::archivos::acortar(pista, hueco);
    let relleno = hueco.saturating_sub(pista.chars().count());
    Line::from(vec![
        Span::styled(
            format!("  {etiqueta}"),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(relleno)),
        Span::styled(
            pista,
            Style::default().fg(if enfocado {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
    ])
}

/// Fila: `[a] tipo  escucha → destino  nombre`, con el glifo de estado delante
/// de los que no están inactivos.
fn linea_tunel(
    app: &App,
    tunel: &Tunel,
    seleccionada: bool,
    ancho_tramo: usize,
    ancho_nombre: usize,
) -> Line<'static> {
    let tema = &app.tema;
    let estado = app.estado_de(tunel.id);
    let mut estilo = Style::default().fg(tema.paleta.texto);
    if seleccionada {
        estilo = estilo
            .bg(tema.paleta.acento)
            .fg(tema.paleta.fondo)
            .add_modifier(Modifier::BOLD);
    }
    // Sobre la fila seleccionada el glifo toma el color de la selección.
    let color_estado = if seleccionada {
        estilo
    } else {
        Style::default().fg(color_estado(estado, tema))
    };
    let glifo = if estado == EstadoTunelRemoto::Inactivo {
        " "
    } else {
        estado.glifo(tema.ascii)
    };
    let tramo = crate::ui::tuneles::tramo(tunel, app.tuneles_activos.get(&tunel.id));
    Line::from(vec![
        Span::styled(
            format!("  {} {glifo} ", if seleccionada { "▸" } else { " " }),
            color_estado,
        ),
        Span::styled(
            format!("{} ", if tunel.automatico { "[a]" } else { "[ ]" }),
            estilo,
        ),
        Span::styled(format!("{:<8} ", tunel.tipo.etiqueta()), estilo),
        Span::styled(
            format!(
                "{:<ancho_tramo$}",
                crate::ui::archivos::acortar(&tramo, ancho_tramo.saturating_sub(2))
            ),
            estilo,
        ),
        Span::styled(
            crate::ui::archivos::acortar(&tunel.nombre, ancho_nombre),
            estilo,
        ),
    ])
}

/// Aviso de reenvíos que siguen en `opciones_extra`, en ámbar: se importan a
/// la tabla con `i` desde el bloque.
fn linea_reenvios(reenvios: usize, tema: &Tema) -> Line<'static> {
    let sustantivo = if reenvios == 1 {
        "reenvío"
    } else {
        "reenvíos"
    };
    Line::from(Span::styled(
        format!(
            "    {} hay {reenvios} {sustantivo} en opciones extra · i importar a túneles",
            tema.glifos.error
        ),
        Style::default().fg(tema.paleta.acento),
    ))
}

/// Primera fila visible: la selección siempre dentro de la ventana.
fn inicio_ventana(seleccionado: usize, total: usize, altura: usize) -> usize {
    let altura = altura.max(1);
    seleccionado
        .saturating_sub(altura - 1)
        .min(total.saturating_sub(altura))
}

/// Mismo criterio de color que la vista Túneles.
fn color_estado(estado: EstadoTunelRemoto, tema: &Tema) -> Color {
    match estado {
        EstadoTunelRemoto::Activo => tema.paleta.correcto,
        EstadoTunelRemoto::Activando | EstadoTunelRemoto::Parando => tema.paleta.acento,
        EstadoTunelRemoto::Caido => tema.paleta.critico,
        EstadoTunelRemoto::Inactivo => tema.paleta.inactivo,
    }
}

/// Líneas de `opciones_extra` que son directivas de reenvío: las mismas que
/// cuenta el aviso y las que importa `i`.
fn reenvios_en_opciones(ficha: &Ficha) -> usize {
    ficha
        .opciones
        .lineas
        .iter()
        .filter(|linea| es_reenvio(linea))
        .count()
}

/// Directiva de reenvío, ignorando espacios y comentarios.
fn es_reenvio(linea: &str) -> bool {
    let limpia = linea.trim();
    if limpia.is_empty() || limpia.starts_with('#') {
        return false;
    }
    matches!(
        limpia
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "localforward" | "remoteforward" | "dynamicforward"
    )
}

fn dibujar_area_texto(
    marco: &mut Frame,
    area: Rect,
    titulo: &str,
    area_texto: &AreaTexto,
    activo: bool,
    tema: &Tema,
) {
    let color_borde = if activo {
        tema.paleta.acento
    } else {
        tema.paleta.inactivo
    };
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            titulo.to_string(),
            Style::default().fg(color_borde),
        ))
        .border_style(Style::default().fg(color_borde));
    let interior = bloque.inner(area);
    marco.render_widget(bloque, area);
    if interior.height > 0 {
        let lineas = area_texto.lineas_con_cursor(activo, estilo_campo(tema, activo));
        marco.render_widget(Paragraph::new(lineas), interior);
    }
}

fn cabecera(texto: &str, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {texto}"),
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    ))
}

fn linea_campo(etiqueta: &str, valor: Span<'static>, activo: bool, tema: &Tema) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{etiqueta:<13}"),
            Style::default().fg(if activo {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
        Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
        valor,
        Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
    ])
}

fn valor_desplegable(
    desplegable: &Desplegable,
    activo: bool,
    tema: &Tema,
    ancho: usize,
) -> Span<'static> {
    let flecha = super::tecla(tema, "▾", "v");
    let texto = desplegable.etiqueta_seleccionada();
    let recortado: String = texto.chars().take(ancho).collect();
    Span::styled(
        format!("{recortado:<ancho$} {flecha}"),
        estilo_campo(tema, activo),
    )
}

/// Valor de identidad con el aviso de agente no disponible cuando aplica.
fn aviso_o_valor_identidad(app: &App, ficha: &Ficha) -> Span<'static> {
    let activo = ficha.campo == CampoFicha::Identidad;
    if app.identidades.agente.is_none() && app.identidades.aviso_agente.is_some() {
        return Span::styled(
            format!(
                "{:<40} ▾ · agente no disponible (solo ficheros)",
                ficha.identidad.etiqueta_seleccionada()
            ),
            Style::default().fg(app.tema.paleta.inactivo),
        );
    }
    valor_desplegable(&ficha.identidad, activo, &app.tema, 40)
}

fn dibujar_desplegable(
    marco: &mut Frame,
    ancla: (u16, u16),
    desplegable: &Desplegable,
    ancho: u16,
    tema: &Tema,
) {
    let filtradas = desplegable.filtradas();
    let visibles: Vec<usize> = filtradas
        .iter()
        .skip(
            desplegable
                .resaltado
                .saturating_sub(ALTO_MAXIMO_DESPLEGABLE - 1),
        )
        .take(ALTO_MAXIMO_DESPLEGABLE)
        .copied()
        .collect();
    let alto = visibles.len().max(1) as u16 + 2;
    let area_marco = marco.area();
    let x = ancla.0.min(area_marco.width.saturating_sub(ancho));
    let y = ancla.1.min(area_marco.height.saturating_sub(alto));
    let recta = Rect {
        x,
        y,
        width: ancho.min(area_marco.width),
        height: alto.min(area_marco.height),
    };
    let mut lineas = Vec::new();
    for indice in &visibles {
        let opcion = &desplegable.opciones[*indice];
        let resaltada = *indice == *filtradas.get(desplegable.resaltado).unwrap_or(indice);
        let estilo = if resaltada {
            Style::default()
                .bg(tema.paleta.acento)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        lineas.push(Line::from(Span::styled(
            format!(
                " {:<ancho$}",
                opcion.etiqueta,
                ancho = (ancho as usize).saturating_sub(2)
            ),
            estilo,
        )));
    }
    if lineas.is_empty() {
        lineas.push(Line::from(Span::styled(
            " (sin coincidencias)",
            Style::default().fg(tema.paleta.inactivo),
        )));
    }
    let titulo = if desplegable.filtro.texto.is_empty() {
        String::new()
    } else {
        format!(" {} ", desplegable.filtro.texto)
    };
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            titulo,
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(tema.paleta.acento));
    marco.render_widget(Clear, recta);
    marco.render_widget(Paragraph::new(lineas).block(bloque), recta);
}

fn dibujar_sugerencias(marco: &mut Frame, ancla: (u16, u16), ficha: &Ficha, tema: &Tema) {
    let alto = ficha.sugerencias.len().min(6) as u16 + 2;
    let ancho: u16 = 30;
    let area_marco = marco.area();
    let x = ancla.0.min(area_marco.width.saturating_sub(ancho));
    let y = ancla.1.min(area_marco.height.saturating_sub(alto));
    let recta = Rect {
        x,
        y,
        width: ancho.min(area_marco.width),
        height: alto.min(area_marco.height),
    };
    let mut lineas = Vec::new();
    for (indice, sugerencia) in ficha.sugerencias.iter().enumerate() {
        let estilo = if indice == ficha.indice_sugerencia {
            Style::default()
                .bg(tema.paleta.correcto)
                .fg(tema.paleta.fondo)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.texto)
        };
        lineas.push(Line::from(Span::styled(format!(" {sugerencia}"), estilo)));
    }
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .title(Span::styled(
            " etiquetas ",
            Style::default().fg(tema.paleta.correcto),
        ))
        .border_style(Style::default().fg(tema.paleta.correcto));
    marco.render_widget(Clear, recta);
    marco.render_widget(Paragraph::new(lineas).block(bloque), recta);
}

#[cfg(test)]
mod pruebas {
    use super::{es_reenvio, inicio_ventana};

    #[test]
    fn se_reconocen_las_tres_directivas_con_espacios_y_comentarios() {
        assert!(es_reenvio("LocalForward 127.0.0.1:5432 10.0.0.5:5432"));
        assert!(es_reenvio("   remoteforward 9000 127.0.0.1:9000"));
        assert!(es_reenvio("DYNAMICFORWARD [::1]:1080"));
        assert!(es_reenvio("\tdynamicforward 1080"));
        assert!(!es_reenvio(""));
        assert!(!es_reenvio("   "));
        assert!(!es_reenvio("# LocalForward 5432 10.0.0.5:5432"));
        assert!(!es_reenvio("ForwardAgent yes"));
        assert!(!es_reenvio("LocalForwardX 5432"));
    }

    #[test]
    fn la_ventana_sigue_a_la_seleccion_y_no_se_sale_de_la_lista() {
        // Con sitio para todo, no hay desplazamiento.
        assert_eq!(inicio_ventana(0, 3, 4), 0);
        // Selección al final: la última fila visible es la elegida.
        assert_eq!(inicio_ventana(9, 10, 3), 7);
        // Índice fuera de la lista (tras borrar) y lista vacía.
        assert_eq!(inicio_ventana(5, 3, 2), 1);
        assert_eq!(inicio_ventana(0, 0, 0), 0);
    }
}
