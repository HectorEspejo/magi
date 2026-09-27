use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, CampoFicha, Ficha};
use crate::config::SeccionDeliberacion;
use crate::modelo::Tunel;
use crate::protocolo::EstadoTunelRemoto;
use crate::tema::Tema;
use crate::ui::componentes::{casilla, estilo_campo, AreaTexto, CampoTexto, Desplegable};

const ANCHO_DESPLEGABLE: u16 = 34;
const ALTO_MAXIMO_DESPLEGABLE: usize = 8;
/// Filas del formulario: los campos y sus cabeceras. No cambian.
const ALTO_CAMPOS: u16 = 23;
/// Lo que se deja como mínimo debajo del formulario: el bloque de túneles
/// (cabecera y una fila) y las áreas de texto (bordes y una línea).
const ALTO_MINIMO_DEBAJO: u16 = 2 + 3;
/// Tope de filas de túnel que muestra el bloque.
const MAX_FILAS_TUNELES: usize = 4;
/// Columna en la que empiezan los campos de ruta, patrón y comando del bloque
/// «Verificaciones previas»: sangría, casilla y la etiqueta más larga.
const COLUMNA_VERIFICACION: usize = 24;

/// Línea del formulario en la que vive cada campo. El bloque de túneles y las
/// áreas de texto no viven en el formulario sino en su propia región, así que
/// no tienen línea.
pub fn linea_de(campo: CampoFicha) -> Option<u16> {
    Some(match campo {
        CampoFicha::Nombre => 2,
        CampoFicha::Direccion => 3,
        CampoFicha::Puerto | CampoFicha::Grupo => 4,
        CampoFicha::Etiquetas => 5,
        CampoFicha::Usuario => 8,
        CampoFicha::Identidad => 9,
        CampoFicha::Salto => 10,
        CampoFicha::Snippet => 13,
        CampoFicha::Multiplexar => 14,
        CampoFicha::Mantener | CampoFicha::Keepalive => 15,
        CampoFicha::Salud => 18,
        CampoFicha::Backup | CampoFicha::BackupRuta => 19,
        CampoFicha::BackupPatron => 20,
        CampoFicha::Tests | CampoFicha::TestsComando => 21,
        CampoFicha::Servicios | CampoFicha::Opciones | CampoFicha::Tuneles => return None,
    })
}

/// Primera línea visible del formulario cuando solo caben `alto` líneas: la
/// del campo con el foco y la siguiente (el patrón, bajo la ruta del backup)
/// quedan siempre dentro. Con el foco fuera del formulario (túneles, áreas de
/// texto) se ve el final, que es lo que queda junto a ellos.
pub fn desplazamiento_formulario(campo: CampoFicha, alto: u16) -> u16 {
    let maximo = ALTO_CAMPOS.saturating_sub(alto);
    match linea_de(campo) {
        Some(linea) => (linea + 2).saturating_sub(alto).min(maximo),
        None => maximo,
    }
}

fn etiqueta_desplegable(campo: CampoFicha, ficha: &Ficha) -> Option<&Desplegable> {
    match campo {
        CampoFicha::Grupo => Some(&ficha.grupo),
        CampoFicha::Identidad => Some(&ficha.identidad),
        CampoFicha::Salto => Some(&ficha.salto),
        CampoFicha::Snippet => Some(&ficha.snippet.desplegable),
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
    // Si no cabe entero, el formulario cede lo justo para que debajo sigan
    // viéndose los túneles y las áreas de texto, y se desplaza con el foco.
    let alto_formulario = ALTO_CAMPOS.min(interior.height.saturating_sub(ALTO_MINIMO_DEBAJO));
    // El bloque de túneles va entre los campos y las áreas de texto; se
    // recorta a lo que quede libre para no dejar sin sitio a los servicios.
    let alto_tuneles =
        alto_bloque_tuneles(app, ficha).min(interior.height.saturating_sub(alto_formulario + 3));
    let trozos = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(alto_formulario),
            Constraint::Length(alto_tuneles),
            Constraint::Min(3),
        ])
        .split(interior);
    let desplazamiento = desplazamiento_formulario(ficha.campo, trozos[0].height);
    let lineas = lineas_formulario(
        ficha,
        tema,
        aviso_o_valor_identidad(app, ficha),
        &app.config.deliberacion,
        trozos[0].width,
    );
    marco.render_widget(
        Paragraph::new(lineas).scroll((desplazamiento, 0)),
        trozos[0],
    );
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

    // Fila en pantalla de un campo del formulario, si está a la vista.
    let fila_en_pantalla = |campo: CampoFicha| {
        linea_de(campo)
            .and_then(|linea| linea.checked_sub(desplazamiento))
            .filter(|linea| *linea < trozos[0].height)
            .map(|linea| trozos[0].y + linea)
    };
    // Desplegable abierto.
    if let Some(campo) = ficha.desplegable_abierto {
        if let (Some(desplegable), Some(fila)) =
            (etiqueta_desplegable(campo, ficha), fila_en_pantalla(campo))
        {
            let x = if campo == CampoFicha::Grupo {
                interior.x + 32
            } else {
                interior.x + 17
            };
            dibujar_desplegable(marco, (x, fila + 1), desplegable, ANCHO_DESPLEGABLE, tema);
        }
    }
    // Sugerencias de etiquetas.
    if ficha.campo == CampoFicha::Etiquetas && !ficha.sugerencias.is_empty() {
        if let Some(fila) = fila_en_pantalla(CampoFicha::Etiquetas) {
            dibujar_sugerencias(marco, (interior.x + 17, fila + 1), ficha, tema);
        }
    }
}

/// Las líneas del formulario (sin túneles ni áreas de texto). `identidad` es
/// el valor ya pintado del campo Identidad, que depende del agente.
fn lineas_formulario(
    ficha: &Ficha,
    tema: &Tema,
    identidad: Span<'static>,
    deliberacion: &SeccionDeliberacion,
    ancho: u16,
) -> Vec<Line<'static>> {
    let activo = |campo: CampoFicha| ficha.campo == campo;
    let estilo = |campo: CampoFicha| estilo_campo(tema, activo(campo));
    let mut lineas: Vec<Line<'static>> = Vec::with_capacity(ALTO_CAMPOS as usize);
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
        identidad,
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
    lineas.push(linea_campo(
        "Snippet",
        valor_snippet(ficha, tema),
        activo(CampoFicha::Snippet),
        tema,
    ));
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
    lineas.extend(lineas_verificaciones(ficha, tema, deliberacion, ancho));
    lineas.push(Line::from(""));
    lineas
}

/// Valor del desplegable «Snippet al conectar»; el snippet que el host tenía
/// y ha dejado de ser apto se pinta en rojo, con su «(no apto)».
fn valor_snippet(ficha: &Ficha, tema: &Tema) -> Span<'static> {
    let activo = ficha.campo == CampoFicha::Snippet;
    let valor = valor_desplegable(&ficha.snippet.desplegable, activo, tema, 40);
    if ficha.snippet.en_no_apto() {
        return valor.style(estilo_campo(tema, activo).fg(tema.paleta.critico));
    }
    valor
}

/// Bloque «Verificaciones previas (deliberación MAGI)»: cabecera, salud,
/// backup (ruta y, debajo, patrón) y tests (comando local).
fn lineas_verificaciones(
    ficha: &Ficha,
    tema: &Tema,
    deliberacion: &SeccionDeliberacion,
    ancho: u16,
) -> Vec<Line<'static>> {
    let ancho = ancho as usize;
    let pista = |texto: String| Span::styled(texto, Style::default().fg(tema.paleta.texto));
    let mut lineas = vec![cabecera("VERIFICACIONES PREVIAS (deliberación MAGI)", tema)];

    let mut salud = inicio_verificacion(
        ficha,
        tema,
        CampoFicha::Salud,
        ficha.salud,
        "salud del host",
    );
    salud.push(pista(format!(
        "(último sondeo NOMINAL < {} min)",
        deliberacion.salud_max_min
    )));
    lineas.push(Line::from(salud));

    // Cada campo se queda con lo que deja la línea tras etiquetas y corchetes.
    let mut backup = inicio_verificacion(
        ficha,
        tema,
        CampoFicha::Backup,
        ficha.backup,
        "backup reciente",
    );
    backup.extend(campo_verificacion(
        ficha,
        tema,
        "ruta",
        CampoFicha::BackupRuta,
        ficha.backup,
        ancho.saturating_sub(COLUMNA_VERIFICACION + "ruta [  ]".len()),
    ));
    lineas.push(Line::from(backup));

    let horas = format!("  (< {} h)", deliberacion.backup_horas);
    let mut patron = vec![Span::raw(" ".repeat(COLUMNA_VERIFICACION))];
    patron.extend(campo_verificacion(
        ficha,
        tema,
        "patrón",
        CampoFicha::BackupPatron,
        ficha.backup,
        ancho.saturating_sub(
            COLUMNA_VERIFICACION + "patrón [  ]".chars().count() + horas.chars().count(),
        ),
    ));
    patron.push(pista(horas));
    lineas.push(Line::from(patron));

    let mut tests = inicio_verificacion(
        ficha,
        tema,
        CampoFicha::Tests,
        ficha.tests,
        "tests en verde",
    );
    tests.extend(campo_verificacion(
        ficha,
        tema,
        "",
        CampoFicha::TestsComando,
        ficha.tests,
        ancho.saturating_sub(COLUMNA_VERIFICACION + "[  ]".len()),
    ));
    lineas.push(Line::from(tests));
    lineas
}

/// `  [x] etiqueta` de una verificación, con la etiqueta rellenada hasta la
/// columna de los campos. La etiqueta se ilumina con el foco en la casilla o
/// en cualquiera de sus campos.
fn inicio_verificacion(
    ficha: &Ficha,
    tema: &Tema,
    campo: CampoFicha,
    marcada: bool,
    etiqueta: &str,
) -> Vec<Span<'static>> {
    let enfocada = match campo {
        CampoFicha::Backup => matches!(
            ficha.campo,
            CampoFicha::Backup | CampoFicha::BackupRuta | CampoFicha::BackupPatron
        ),
        CampoFicha::Tests => matches!(ficha.campo, CampoFicha::Tests | CampoFicha::TestsComando),
        _ => ficha.campo == campo,
    };
    // Sangría (2), casilla (3) y un espacio: la etiqueta llena el resto hasta
    // los campos. La salud no tiene campos: solo la separa de su pista.
    let ancho_etiqueta = COLUMNA_VERIFICACION - 6;
    let etiqueta = if campo == CampoFicha::Salud {
        format!("{etiqueta} ")
    } else {
        format!("{etiqueta:<ancho_etiqueta$}")
    };
    vec![
        Span::raw("  "),
        Span::styled(
            casilla(marcada).to_string(),
            estilo_campo(tema, ficha.campo == campo),
        ),
        Span::raw(" "),
        Span::styled(
            etiqueta,
            Style::default().fg(if enfocada {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ),
    ]
}

/// `etiqueta [ valor ]` de un campo de una verificación, con el valor
/// recortado a `hueco` columnas. Con la casilla desmarcada el valor se
/// conserva y se puede editar, pero va apagado: no se exige.
fn campo_verificacion(
    ficha: &Ficha,
    tema: &Tema,
    etiqueta: &str,
    campo: CampoFicha,
    marcada: bool,
    hueco: usize,
) -> Vec<Span<'static>> {
    let texto = match campo {
        CampoFicha::BackupRuta => &ficha.backup_ruta,
        CampoFicha::BackupPatron => &ficha.backup_patron,
        _ => &ficha.tests_comando,
    };
    let activo = ficha.campo == campo;
    let estilo = if activo || marcada {
        estilo_campo(tema, activo)
    } else {
        Style::default().fg(tema.paleta.inactivo)
    };
    let mut spans = Vec::with_capacity(4);
    if !etiqueta.is_empty() {
        spans.push(Span::styled(
            format!("{etiqueta} "),
            Style::default().fg(if activo {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        ));
    }
    spans.push(Span::styled("[ ", Style::default().fg(tema.paleta.acento)));
    spans.push(span_ajustado(texto, activo, estilo, hueco));
    spans.push(Span::styled(" ]", Style::default().fg(tema.paleta.acento)));
    spans
}

/// Valor de un campo de texto recortado a `ancho` columnas alrededor del
/// cursor, con `…` donde se corta: un comando largo sigue viéndose por donde
/// se escribe. Sin el foco se ve el principio.
fn span_ajustado(campo: &CampoTexto, activo: bool, estilo: Style, ancho: usize) -> Span<'static> {
    let completo = campo.span(activo, estilo);
    let caracteres: Vec<char> = completo.content.chars().collect();
    if ancho < 3 || caracteres.len() <= ancho {
        return completo;
    }
    let cursor = if activo {
        campo.cursor.min(caracteres.len() - 1)
    } else {
        0
    };
    // El cursor no cae nunca en la última columna si detrás queda texto: ahí
    // va el `…`.
    let inicio = (cursor + 2)
        .saturating_sub(ancho)
        .min(caracteres.len() - ancho);
    let mut ventana: Vec<char> = caracteres[inicio..inicio + ancho].to_vec();
    if inicio > 0 {
        ventana[0] = '…';
    }
    if inicio + ancho < caracteres.len() {
        ventana[ancho - 1] = '…';
    }
    Span::styled(ventana.into_iter().collect::<String>(), estilo)
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
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::app::pruebas_ficha::{ficha_con, snippets, verificaciones};

    /// Pinta las líneas del formulario en un terminal de pruebas y devuelve
    /// cada fila como texto.
    fn pintar(ficha: &Ficha, ancho: u16) -> Vec<String> {
        let tema = Tema::respaldo();
        let lineas = lineas_formulario(
            ficha,
            &tema,
            Span::raw("auto"),
            &SeccionDeliberacion::default(),
            ancho,
        );
        assert_eq!(lineas.len(), ALTO_CAMPOS as usize);
        let mut terminal = Terminal::new(TestBackend::new(ancho, ALTO_CAMPOS)).unwrap();
        terminal
            .draw(|marco| marco.render_widget(Paragraph::new(lineas), marco.area()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn ficha_de_prueba() -> Ficha {
        let mut host = crate::modelo::host_de_prueba();
        host.snippet_al_conectar_id = Some(4);
        let mut ficha = ficha_con(Some(&host), &snippets(), Some(verificaciones()));
        ficha.tests_comando = CampoTexto::nuevo(
            "gh run list -R 4d3/cooperapp -L 1 --json conclusion -q '.[0].conclusion' \
             | grep -qx success",
        );
        ficha
    }

    #[test]
    fn se_pintan_el_snippet_al_conectar_y_las_verificaciones_previas() {
        let filas = pintar(&ficha_de_prueba(), 140);
        let fila = |campo: CampoFicha| filas[linea_de(campo).unwrap() as usize].clone();
        assert_eq!(filas[12], " AL CONECTAR");
        assert!(
            fila(CampoFicha::Snippet).starts_with("  Snippet      [ tail log"),
            "{}",
            fila(CampoFicha::Snippet)
        );
        assert!(fila(CampoFicha::Multiplexar).starts_with("  Multiplexar"));
        assert_eq!(filas[17], " VERIFICACIONES PREVIAS (deliberación MAGI)");
        assert_eq!(
            fila(CampoFicha::Salud),
            "  [x] salud del host (último sondeo NOMINAL < 5 min)"
        );
        assert_eq!(
            fila(CampoFicha::BackupRuta),
            "  [x] backup reciente   ruta [ /var/backups/pg ]"
        );
        assert_eq!(
            fila(CampoFicha::BackupPatron),
            "                        patrón [ *.sql.gz ]  (< 24 h)"
        );
        assert_eq!(
            fila(CampoFicha::TestsComando),
            "  [x] tests en verde    [ gh run list -R 4d3/cooperapp -L 1 --json conclusion \
             -q '.[0].conclusion' | grep -qx success ]"
        );
    }

    #[test]
    fn un_comando_largo_se_recorta_sin_salirse_de_la_linea() {
        let mut ficha = ficha_de_prueba();
        let filas = pintar(&ficha, 80);
        let tests = &filas[linea_de(CampoFicha::TestsComando).unwrap() as usize];
        assert!(tests.ends_with("… ]"), "{tests}");
        assert_eq!(tests.chars().count(), 80);
        assert!(tests.contains("[ gh run list"));

        // Con el foco, se ve el final, donde está el cursor.
        ficha.campo = CampoFicha::TestsComando;
        let filas = pintar(&ficha, 80);
        let tests = &filas[linea_de(CampoFicha::TestsComando).unwrap() as usize];
        assert!(tests.ends_with("grep -qx success\u{2503} ]"), "{tests}");
        assert!(tests.contains("[ …"), "{tests}");
    }

    #[test]
    fn el_snippet_no_apto_se_pinta_con_su_marca() {
        let mut host = crate::modelo::host_de_prueba();
        host.snippet_al_conectar_id = Some(2);
        let ficha = ficha_con(Some(&host), &snippets(), None);
        let filas = pintar(&ficha, 100);
        assert!(filas[linea_de(CampoFicha::Snippet).unwrap() as usize]
            .contains("[ reiniciar (no apto)"));
        // Sin verificaciones, todas desmarcadas y con los campos vacíos.
        assert_eq!(
            filas[linea_de(CampoFicha::Salud).unwrap() as usize],
            "  [ ] salud del host (último sondeo NOMINAL < 5 min)"
        );
        assert_eq!(
            filas[linea_de(CampoFicha::Tests).unwrap() as usize],
            "  [ ] tests en verde    [  ]"
        );
    }

    #[test]
    fn el_formulario_se_desplaza_para_que_el_foco_se_vea() {
        // Cabe entero: no se mueve.
        for campo in crate::app::ORDEN_CAMPOS {
            assert_eq!(desplazamiento_formulario(campo, ALTO_CAMPOS), 0);
        }
        // Diez líneas: arriba no se mueve; abajo, el foco y la línea siguiente
        // quedan dentro y nunca se pasa del final.
        assert_eq!(desplazamiento_formulario(CampoFicha::Nombre, 10), 0);
        assert_eq!(desplazamiento_formulario(CampoFicha::Snippet, 10), 5);
        assert_eq!(desplazamiento_formulario(CampoFicha::BackupRuta, 10), 11);
        assert_eq!(desplazamiento_formulario(CampoFicha::TestsComando, 10), 13);
        assert_eq!(desplazamiento_formulario(CampoFicha::Tuneles, 10), 13);
        for campo in crate::app::ORDEN_CAMPOS {
            let desplazamiento = desplazamiento_formulario(campo, 10);
            assert!(desplazamiento + 10 <= ALTO_CAMPOS);
            if let Some(linea) = linea_de(campo) {
                assert!(linea >= desplazamiento && linea < desplazamiento + 10);
            }
        }
    }

    #[test]
    fn un_texto_recortado_deja_el_cursor_a_la_vista() {
        let estilo = Style::default();
        let mut campo = CampoTexto::nuevo("abcdefghij");
        // Cabe: tal cual.
        assert_eq!(
            span_ajustado(&campo, false, estilo, 10).content,
            "abcdefghij"
        );
        // Sin foco, el principio.
        assert_eq!(span_ajustado(&campo, false, estilo, 6).content, "abcde…");
        // Con el cursor al final, el final.
        assert_eq!(
            span_ajustado(&campo, true, estilo, 6).content,
            "…ghij\u{2503}"
        );
        // Con el cursor en medio, recortado por los dos lados.
        campo.cursor = 5;
        let visto = span_ajustado(&campo, true, estilo, 6).content.to_string();
        assert!(visto.starts_with('…') && visto.ends_with('…'), "{visto}");
        assert!(visto.contains('\u{2503}'), "{visto}");
    }

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
