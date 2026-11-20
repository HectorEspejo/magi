//! Barra inferior: un aviso del servidor o un mensaje si los hay (tienen
//! prioridad) y, si no, los atajos de la vista.
//!
//! Disposición adaptable (Fase 7): cada atajo lleva prioridad (1 = el más
//! importante; los de `[flota.atajos]` los últimos) y se muestran los que
//! caben (`disposicion::atajos_que_caben`), en su orden. Si no caben todos,
//! la barra termina en `? más` (la ayuda los lista todos); si caben y queda
//! sitio, en `? ayuda`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, PanelFlota};
use crate::archivos::panel::Lado;
use crate::tema::Tema;
use crate::ui::disposicion::{self, Atajo, Disposicion};
use crate::ui::Vista;

/// Prioridad de los atajos de `[flota.atajos]`: los primeros en ocultarse.
pub const PRIORIDAD_ATAJOS_FLOTA: u8 = 5;

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    let tema = &app.tema;
    // Los indicadores (llavero bloqueado y sincronización) reservan su ancho
    // antes de los atajos: si no, se saldrían de la línea y ratatui los
    // recortaría.
    let indicadores = indicadores_de_la_barra(app);
    let ancho_indicador = indicadores
        .iter()
        .map(|(texto, _)| texto.chars().count() as u16)
        .sum::<u16>()
        + u16::from(!indicadores.is_empty());
    let disponible = area.width.saturating_sub(ancho_indicador);
    let ancho = usize::from(disponible);
    let linea = if let Some(rechazo) = &app.servidor_incompatible {
        // Mientras el servidor siga siendo de otra versión: uno más nuevo no
        // se puede parar desde aquí; uno anterior sin sustituir hay que
        // pararlo a mano.
        let texto = crate::app::servidor_anterior::aviso_barra(rechazo);
        Line::from(Span::styled(
            ajustar(&format!(" {} {texto}", tema.glifos.error), ancho, tema),
            Style::default().fg(tema.paleta.critico),
        ))
    } else if let Some(mensaje) = &app.mensaje {
        let glifo = if mensaje.error {
            format!("{} ", tema.glifos.error)
        } else {
            String::new()
        };
        let color = if mensaje.error {
            tema.paleta.critico
        } else {
            tema.paleta.correcto
        };
        Line::from(Span::styled(
            ajustar(&format!(" {glifo}{}", mensaje.texto), ancho, tema),
            Style::default().fg(color),
        ))
    } else if let Some(variantes) = aviso_atencion(app) {
        // Una sesión persistente pide atención (Fase 16): se ve desde
        // cualquier vista, con los atajos importantes detrás.
        linea_con_atencion(app, disponible, disp, &variantes)
    } else if let Some(info) = app
        .servicios
        .acciones
        .first()
        .filter(|_| app.vista != Vista::Servicios)
    {
        // Acciones de servicio en curso de cualquier ventana (T67): también se
        // ven desde Sesión.
        let texto = format!(
            "{} {} {} en {}",
            tema.glifos.conectando,
            info.accion.texto(),
            info.objetivo.nombre,
            info.host_nombre
        );
        Line::from(Span::styled(
            ajustar(&format!(" {texto}"), ancho, tema),
            Style::default().fg(tema.paleta.acento),
        ))
    } else {
        linea_atajos(app, disponible, disp)
    };
    marco.render_widget(
        Paragraph::new(con_indicadores(linea, indicadores, area.width)),
        area,
    );
}

/// Marca «llavero bloqueado» (Fase 17, §6 «Marcas»): algo necesitó el llavero
/// de fichero y estaba cerrado; Ctrl+P lo desbloquea. Solo con el de fichero.
/// La barra se pinta muchas veces por segundo y no hace entrada/salida: lee la
/// marca que la `App` calcula en el tick y cuando algo cambia.
fn indicador_llavero(app: &App) -> Option<(String, ratatui::style::Color)> {
    app.marca_llavero_bloqueado()
        .then(|| (" llavero bloqueado ".to_string(), app.tema.paleta.critico))
}

/// Los indicadores de la derecha de la barra, de izquierda a derecha.
fn indicadores_de_la_barra(app: &App) -> Vec<(String, ratatui::style::Color)> {
    indicador_llavero(app)
        .into_iter()
        .chain(indicador_sincronizacion(app))
        .collect()
}

/// Pega los indicadores al final de la línea, dejando un hueco si el contenido
/// no la llena.
fn con_indicadores(
    mut linea: Line<'static>,
    indicadores: Vec<(String, ratatui::style::Color)>,
    ancho: u16,
) -> Line<'static> {
    if indicadores.is_empty() {
        return linea;
    }
    let texto_total: usize = indicadores
        .iter()
        .map(|(texto, _)| texto.chars().count())
        .sum();
    let usado = linea.width();
    let hueco = usize::from(ancho).saturating_sub(usado + texto_total);
    if hueco > 0 {
        linea.spans.push(Span::raw(" ".repeat(hueco)));
        for (texto, color) in indicadores {
            linea
                .spans
                .push(Span::styled(texto, Style::default().fg(color)));
        }
    }
    linea
}

/// Indicador de la sincronización en la barra inferior (informe §6.6): signo,
/// texto y color. No aparece si la sincronización está desactivada.
fn indicador_sincronizacion(app: &App) -> Option<(String, ratatui::style::Color)> {
    if !app.config.sincronizacion.activa() {
        return None;
    }
    let tema = &app.tema;
    let estado = &app.sincronizacion;
    let ascii = tema.ascii;
    let (texto, color) = if estado.sincronizando {
        (
            if ascii {
                "sync ...".to_string()
            } else {
                format!("{} …", tema.glifos.tuneles)
            },
            tema.paleta.acento,
        )
    } else {
        match &estado.estado {
            crate::sincronizacion::EstadoPasada::AlDia => {
                if estado.conflictos_pendientes > 0 {
                    (
                        if ascii {
                            format!("sync {}!", estado.conflictos_pendientes)
                        } else if estado.conflictos_pendientes == 1 {
                            format!("{} 1 conflicto", tema.glifos.tuneles)
                        } else {
                            format!(
                                "{} {} conflictos",
                                tema.glifos.tuneles, estado.conflictos_pendientes
                            )
                        },
                        tema.paleta.acento,
                    )
                } else {
                    (
                        if ascii {
                            "sync ok".to_string()
                        } else {
                            format!("{} al día", tema.glifos.tuneles)
                        },
                        tema.paleta.correcto,
                    )
                }
            }
            crate::sincronizacion::EstadoPasada::DecisionPendiente(_) => (
                if ascii {
                    "sync pausa".to_string()
                } else {
                    format!("{} pausa", tema.glifos.tuneles)
                },
                tema.paleta.acento,
            ),
            crate::sincronizacion::EstadoPasada::EsperandoClave(_)
            | crate::sincronizacion::EstadoPasada::SinClave => (
                if ascii {
                    "sync clave".to_string()
                } else {
                    format!("{} clave nueva", tema.glifos.tuneles)
                },
                tema.paleta.critico,
            ),
            crate::sincronizacion::EstadoPasada::Quitado
            | crate::sincronizacion::EstadoPasada::Error(_) => (
                if ascii {
                    "sync error".to_string()
                } else {
                    format!("{} error", tema.glifos.tuneles)
                },
                tema.paleta.critico,
            ),
            crate::sincronizacion::EstadoPasada::SinConfigurar => return None,
        }
    };
    Some((format!(" {texto} "), color))
}

/// Texto de la barra ajustado al ancho, con la marca de recorte. En ASCII los
/// signos tipográficos de los mensajes se degradan.
fn ajustar(texto: &str, ancho: usize, tema: &Tema) -> String {
    let texto = if tema.ascii {
        disposicion::texto_ascii(texto)
    } else {
        texto.to_string()
    };
    disposicion::recortar(&texto, ancho, tema.ascii)
}

/// Línea de atajos de la vista: los que caben por prioridad y `? más` o
/// `? ayuda` al final. El filtro de Túneles que se está escribiendo va
/// siempre, detrás.
fn linea_atajos(app: &App, ancho: u16, disp: &Disposicion) -> Line<'static> {
    let tema = &app.tema;
    let tecla_estilo = Style::default()
        .fg(tema.paleta.acento)
        .add_modifier(Modifier::BOLD);
    let texto_estilo = Style::default().fg(tema.paleta.texto);
    let mut cola: Vec<Span<'static>> = Vec::new();
    if app.vista == Vista::Tuneles && app.filtro_tuneles_activo {
        // Separador, «/ », el texto y el cursor. Si el texto no cabe, se ve
        // su final, que es donde se escribe.
        let cursor = super::tecla(tema, "▏", "|");
        let cabe = usize::from(ancho).saturating_sub(1 + 3 + 2 + 1);
        cola.push(Span::raw("   "));
        cola.push(Span::styled("/ ".to_string(), tecla_estilo));
        cola.push(Span::styled(
            format!(
                "{}{cursor}",
                final_visible(&app.filtro_tuneles, cabe, tema.ascii)
            ),
            texto_estilo,
        ));
    }
    let ancho_cola: u16 = cola.iter().map(|span| span.width() as u16).sum();
    // Un espacio delante de todo y la cola del filtro (con su separador)
    // reservada.
    let disponible = ancho.saturating_sub(1 + ancho_cola);
    let atajos = atajos(app, disp);
    let (visibles, final_ayuda) = if tiene_ayuda(app.vista) {
        let total: u16 = atajos.iter().map(Atajo::ancho).sum();
        if total.saturating_add(Atajo::new("?", "ayuda", 1).ancho()) <= disponible {
            (vec![true; atajos.len()], Some("ayuda"))
        } else if disponible < disposicion::ANCHO_MAS {
            // Ni «? más» cabe (el filtro de Túneles ocupa la barra).
            (vec![false; atajos.len()], None)
        } else {
            // No caben todos con la ayuda: termina en «? más». Si los atajos
            // solos caben, se pide un ancho menor que su total para que
            // `atajos_que_caben` deje sitio a «? más» quitando el menos
            // importante.
            let limite = disponible.min(total.saturating_sub(1));
            let (visibles, _) = disposicion::atajos_que_caben(&atajos, limite);
            (visibles, Some("más"))
        }
    } else {
        // Sin ayuda en la vista, «? más» no tendría adónde llevar.
        (disposicion::atajos_que_caben(&atajos, disponible).0, None)
    };
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    let mut pares: Vec<(String, String)> = atajos
        .into_iter()
        .zip(visibles)
        .filter(|(_, visible)| *visible)
        .map(|(atajo, _)| (atajo.tecla.into_owned(), atajo.texto.into_owned()))
        .collect();
    if let Some(texto) = final_ayuda {
        pares.push(("?".to_string(), texto.to_string()));
    }
    for (indice, (tecla, descripcion)) in pares.into_iter().enumerate() {
        if indice > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(tecla, tecla_estilo));
        spans.push(Span::styled(format!(" {descripcion}"), texto_estilo));
    }
    spans.extend(cola);
    Line::from(spans)
}

/// El aviso de atención de las sesiones persistentes (D335), de la frase más
/// completa a la más corta, o `None` si ninguna lo pide. Sale de la última
/// lista del servidor (`persistentes_con_atencion`), así que se quita solo
/// cuando la sesión deja de pedirla. La que se está mirando en la vista Sesión
/// no cuenta: ya se ve.
fn aviso_atencion(app: &App) -> Option<Vec<String>> {
    let mut atencion = app.persistentes_con_atencion();
    if app.vista == Vista::Sesion {
        if let Some(pestana) = app.pestana_enfocada() {
            if let Some(tmux) = pestana.sesion_persistente() {
                atencion.retain(|sesion| {
                    !(sesion.host_id == pestana.host_id && sesion.nombre == tmux.nombre)
                });
            }
        }
    }
    let marca = super::tecla(&app.tema, "◆", "P");
    let punto = app.tema.glifos.punto_medio;
    let aviso = app.tema.glifos.aviso;
    let variantes = match atencion.as_slice() {
        [] => return None,
        [sesion] => vec![
            format!(
                "{marca} {} pide atención {punto} {}",
                sesion.nombre, sesion.host_nombre
            ),
            format!("{marca} {} pide atención", sesion.nombre),
            format!("{marca} {} {aviso}", sesion.nombre),
            format!("{marca} {aviso}"),
        ],
        varias => vec![
            format!("{marca} {} sesiones piden atención", varias.len()),
            format!("{marca} {} piden atención", varias.len()),
            format!("{marca} {} {aviso}", varias.len()),
            format!("{marca} {aviso}"),
        ],
    };
    Some(
        variantes
            .into_iter()
            .map(|texto| disposicion::adaptar(&texto, app.tema.ascii))
            .collect(),
    )
}

/// La línea de atajos con el aviso de atención delante. Se elige la frase más
/// completa que deje sitio a los atajos de prioridad 1 y a `? más`: en una
/// barra estrecha el aviso se acorta antes que quitar un atajo importante.
fn linea_con_atencion(
    app: &App,
    ancho: u16,
    disp: &Disposicion,
    variantes: &[String],
) -> Line<'static> {
    // Los dos primeros atajos de prioridad 1 (y `? más`) no se tapan nunca.
    let minimo_atajos: u16 = atajos(app, disp)
        .iter()
        .filter(|atajo| atajo.prioridad <= 1)
        .take(2)
        .map(Atajo::ancho)
        .sum::<u16>()
        .saturating_add(if tiene_ayuda(app.vista) {
            disposicion::ANCHO_MAS
        } else {
            0
        });
    // Un espacio delante, el aviso, un espacio de separación y los atajos. El
    // ancho es el de pantalla, no el de caracteres: el nombre de un host con
    // caracteres anchos (CJK, emoji) ocupa dos columnas por cada uno.
    let ancho_de = |texto: &str| Span::raw(texto).width();
    let cabe =
        |texto: &String| 1 + ancho_de(texto) + 1 + usize::from(minimo_atajos) <= usize::from(ancho);
    let Some(elegida) = variantes
        .iter()
        .find(|texto| cabe(texto))
        .or_else(|| variantes.last())
    else {
        return linea_atajos(app, ancho, disp);
    };
    let usado = 1 + ancho_de(elegida) + 1;
    let resto = ancho.saturating_sub(u16::try_from(usado).unwrap_or(u16::MAX));
    let tema = &app.tema;
    let mut linea = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            elegida.clone(),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ]);
    linea.spans.extend(linea_atajos(app, resto, disp).spans);
    linea
}

/// Los últimos `ancho` caracteres de un texto, con la marca de recorte
/// delante si no cabe entero.
fn final_visible(texto: &str, ancho: usize, ascii: bool) -> String {
    let largo = texto.chars().count();
    if largo <= ancho {
        return texto.to_string();
    }
    if ancho == 0 {
        return String::new();
    }
    let marca = if ascii { "~" } else { "…" };
    let final_: String = texto.chars().skip(largo + 1 - ancho).collect();
    format!("{marca}{final_}")
}

/// ¿La vista abre la ayuda con `?`? En la ficha `?` se escribe y en Sesión va
/// al remoto (allí es `prefijo ?`, anunciado en su barra de estado).
fn tiene_ayuda(vista: Vista) -> bool {
    !matches!(vista, Vista::Ficha | Vista::Sesion)
}

/// Atajos de la vista, en su orden y con su prioridad (sin `?`, que añade la
/// barra al final).
pub fn atajos(app: &App, disp: &Disposicion) -> Vec<Atajo> {
    let tema = &app.tema;
    let intro = super::tecla(tema, "↵", "enter");
    let mover = super::tecla(tema, "↓↑", "j/k");
    let tab = super::tecla(tema, "⇥", "tab");
    let estrecho = disp.estrecho();
    let mut atajos = match app.vista {
        Vista::Archivos => {
            // En estrecho se ve un panel: `Tab` lleva al otro, que se nombra.
            let panel = match app.archivos.as_ref().map(|archivos| archivos.activo) {
                Some(Lado::Remoto) if estrecho => "local",
                Some(Lado::Local) if estrecho => "remoto",
                _ => "panel",
            };
            vec![
                Atajo::new(tab, panel, 1),
                Atajo::new(intro, "abrir", 1),
                Atajo::new("c", "copiar", 1),
                Atajo::new("m", "mover", 2),
                Atajo::new("x", "borrar", 2),
                Atajo::new("E", "editar", 2),
                Atajo::new("S", "sincronizar", 3),
                Atajo::new("p", "permisos", 4),
                Atajo::new("L", "guardadas", 4),
                Atajo::new("r", "renombrar", 3),
                Atajo::new("d", "dir", 3),
                Atajo::new(".", "ocultos", 4),
                Atajo::new("/", "filtrar", 3),
                Atajo::new("g", "ruta", 4),
                Atajo::new("h", "host", 4),
                Atajo::new("t", "cola", 3),
                Atajo::new("q", "volver", 3),
            ]
        }
        Vista::Transferencias => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new("x", "cancelar", 1),
            Atajo::new("C", "limpiar", 2),
            Atajo::new(intro, "detalle", 1),
            Atajo::new("q", "volver", 2),
        ],
        Vista::Registro => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new(intro, "detalle", 1),
            Atajo::new("/", "filtrar", 1),
            Atajo::new("t", "tipo", 2),
            Atajo::new("p", "purgar >90 d", 3),
            Atajo::new("x", "exportar", 3),
            Atajo::new("q", "salir", 3),
        ],
        Vista::Identidades => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new("n", "generar", 1),
            Atajo::new("i", "importar", 2),
            Atajo::new("c", "copiar pública", 1),
            Atajo::new("e", "alias", 3),
            Atajo::new("x", "revocar", 2),
            Atajo::new("s", "reescanear", 3),
            Atajo::new("v", "revocadas", 3),
            Atajo::new("q", "salir", 3),
        ]
        .into_iter()
        // Con la vista baja el detalle está plegado: `↵` lo abre.
        .chain(disp.bajo.then(|| Atajo::new(intro, "detalle", 1)))
        .collect(),
        Vista::Hosts => vec![
            Atajo::new(intro, "conectar", 1),
            Atajo::new(mover, "mover", 4),
            Atajo::new(super::tecla(tema, "→←", "l/h"), "plegar", 3),
            Atajo::new("e", "editar", 2),
            Atajo::new("n", "nuevo", 2),
            Atajo::new("g", "grupo", 3),
            Atajo::new("x", "borrar", 3),
            Atajo::new(tab, "etiquetas", 3),
            Atajo::new("/", "filtrar", 1),
            Atajo::new("!", "snippets", 3),
            Atajo::new("u", "servicios", 4),
            Atajo::new("^p", "paleta", 3),
            Atajo::new("q", "salir", 3),
        ],
        Vista::Flota => {
            let mut atajos = vec![
                Atajo::new(intro, "ssh", 1),
                Atajo::new(mover, "mover", 4),
                Atajo::new("r", "sondear", 1),
            ];
            // En estrecho se ve un panel: `Tab` lleva al otro (no mientras
            // se escribe el filtro, que se queda la tecla).
            if estrecho && !app.filtro_activo {
                let destino = match app.panel_flota {
                    PanelFlota::Lista => "detalle",
                    PanelFlota::Detalle => "lista",
                };
                atajos.push(Atajo::new(tab, destino, 1));
            }
            atajos.extend([
                Atajo::new("R", "todos", 3),
                Atajo::new("a", "auto", 3),
                Atajo::new("e", "editar", 2),
                Atajo::new("/", "filtrar", 2),
                Atajo::new("!", "snippets", 3),
                Atajo::new("u", "servicios", 4),
                Atajo::new("^p", "paleta", 3),
                Atajo::new("q", "salir", 3),
            ]);
            // Atajos de `[flota.atajos]`: tecla y snippet, los últimos.
            for (tecla, snippet) in app.atajos_flota() {
                atajos.push(Atajo::new(
                    tecla.to_string(),
                    crate::snippets::salida::sanear_linea(snippet, 24),
                    PRIORIDAD_ATAJOS_FLOTA,
                ));
            }
            atajos
        }
        Vista::Ficha => {
            let mut atajos = vec![
                Atajo::new(tab, "campo", 2),
                Atajo::new(intro, "desplegable", 3),
                Atajo::new("espacio", "casilla", 3),
                Atajo::new("^s", "guardar", 1),
            ];
            // Probar la conexión es solo para un host SSH (Fase 12).
            if app.ficha.as_ref().is_some_and(|ficha| ficha.tipo.es_ssh()) {
                atajos.push(Atajo::new("^t", "probar conexión", 3));
            }
            atajos.push(Atajo::new("u", "servicios", 3));
            atajos.push(Atajo::new("esc", "descartar", 1));
            atajos
        }
        Vista::Tuneles => {
            let mut atajos = vec![
                Atajo::new(mover, "mover", 4),
                Atajo::new("espacio", "activar/parar", 1),
                Atajo::new("n", "nuevo", 2),
                Atajo::new("e", "editar", 2),
                Atajo::new("x", "borrar", 3),
                Atajo::new("a", "auto", 3),
                Atajo::new("r", "relanzar", 3),
                Atajo::new(intro, "detalle", 1),
                Atajo::new("/", "filtrar", 2),
                Atajo::new("q", "volver", 3),
            ];
            if !app.tuneles_temporales.is_empty() {
                atajos.push(Atajo::new("temporal", "se cierra con su pestaña", 1));
            }
            atajos
        }
        Vista::Snippets => vec![
            Atajo::new(intro, "ejecutar", 1),
            Atajo::new("a", "en todos", 2),
            Atajo::new("p", "en pestaña", 2),
            Atajo::new(
                "n e x",
                format!("nuevo {0} editar {0} borrar", tema.glifos.punto_medio),
                3,
            ),
            Atajo::new("/", "buscar", 1),
            Atajo::new("t", "resultados", 2),
        ]
        .into_iter()
        // Con la vista baja el detalle está plegado: `i` lo abre (`↵` ejecuta).
        .chain(disp.bajo.then(|| Atajo::new("i", "detalle", 2)))
        .collect(),
        Vista::Resultados => vec![
            Atajo::new(tab, "panel", 1),
            Atajo::new(intro, "salida", 1),
            Atajo::new("s", "guardar", 2),
            Atajo::new("r", "repetir", 2),
            Atajo::new("x", "cancelar", 2),
            Atajo::new("C", "limpiar", 3),
            Atajo::new("q", "volver", 2),
        ],
        // Los atajos son los del bloque con el foco (Fase 16).
        Vista::Sesiones => crate::ui::persistentes::atajos_sesiones(app),
        Vista::Sincronizacion => {
            let mut atajos = vec![
                Atajo::new("s", "sincronizar", 1),
                Atajo::new("c", "conflictos", 1),
                Atajo::new("k", "clave", 2),
            ];
            if app.config.sincronizacion.activa() {
                atajos.extend([
                    Atajo::new("x", "quitar equipo", 3),
                    Atajo::new("r", "cambiar clave", 3),
                    Atajo::new("d", "desactivar", 3),
                ]);
            } else {
                atajos.push(Atajo::new("n", "configurar carpeta", 1));
            }
            atajos.push(Atajo::new("q", "volver", 2));
            atajos
        }
        Vista::Conflictos => vec![
            Atajo::new(mover, "mover", 4),
            Atajo::new(intro, "detalle", 1),
            Atajo::new("r", "recuperar", 1),
            Atajo::new("a", "aceptar", 2),
            Atajo::new("d", "descartar", 2),
            Atajo::new("q", "volver", 2),
        ],
        Vista::Servicios => {
            // Solo se ofrecen las acciones que valen en el estado del elemento
            // seleccionado; su tecla, si se pulsa igualmente, explica el motivo.
            let mut atajos = vec![Atajo::new(intro, "detalle", 1), Atajo::new("l", "logs", 1)];
            if let Some(host_id) = app.servicios_host_seleccionado() {
                if let Some(objetivo) = app.servicios_objetivo_seleccionado(host_id) {
                    let permitidas = app.servicios_acciones_ofrecidas(host_id, &objetivo);
                    for accion in [
                        magi_nucleo::servicios::Accion::Arrancar,
                        magi_nucleo::servicios::Accion::Lanzar,
                        magi_nucleo::servicios::Accion::Parar,
                        magi_nucleo::servicios::Accion::Reiniciar,
                        magi_nucleo::servicios::Accion::Recargar,
                    ] {
                        if !permitidas.contains(&accion) {
                            continue;
                        }
                        let (tecla_accion, texto) = match accion {
                            magi_nucleo::servicios::Accion::Arrancar => ("A", "arrancar"),
                            magi_nucleo::servicios::Accion::Lanzar => ("A", "lanzar"),
                            magi_nucleo::servicios::Accion::Parar => ("P", "parar"),
                            magi_nucleo::servicios::Accion::Reiniciar => ("R", "reiniciar"),
                            magi_nucleo::servicios::Accion::Recargar => ("G", "recargar"),
                            _ => continue,
                        };
                        atajos.push(Atajo::new(tecla_accion, texto, 2));
                    }
                    if permitidas.contains(&magi_nucleo::servicios::Accion::Activar) {
                        atajos.push(Atajo::new("H", "activar", 3));
                    } else if permitidas.contains(&magi_nucleo::servicios::Accion::Desactivar) {
                        atajos.push(Atajo::new("H", "desactivar", 3));
                    }
                    if objetivo.tipo == magi_nucleo::servicios::TipoObjetivo::Contenedor {
                        let corre = app
                            .servicios_listado(host_id)
                            .and_then(|listado| {
                                listado
                                    .listado
                                    .contenedores
                                    .iter()
                                    .find(|contenedor| contenedor.nombre == objetivo.nombre)
                            })
                            .is_some_and(|contenedor| contenedor.admite_shell());
                        if corre {
                            atajos.push(Atajo::new("S", "shell", 3));
                        }
                    }
                    if matches!(
                        objetivo.tipo,
                        magi_nucleo::servicios::TipoObjetivo::Unidad
                            | magi_nucleo::servicios::TipoObjetivo::Proyecto
                    ) {
                        atajos.push(Atajo::new("c", "comprobación", 3));
                    }
                }
            }
            atajos.extend([
                Atajo::new(mover, "mover", 4),
                Atajo::new("/", "filtrar", 2),
                Atajo::new("1", "servicios", 3),
                Atajo::new("2", "timers", 3),
                Atajo::new("3", "contenedores", 3),
                Atajo::new("!", "sudo", 3),
                Atajo::new("r", "refrescar", 1),
                Atajo::new("esc", "volver", 2),
            ]);
            if app.servicios_host_seleccionado().is_none() {
                atajos = vec![
                    Atajo::new(intro, "ir al host", 1),
                    Atajo::new("l", "logs", 2),
                    Atajo::new("A", "arrancar", 3),
                    Atajo::new("R", "reiniciar", 3),
                    Atajo::new("r", "sondear todos", 1),
                    Atajo::new(mover, "mover", 4),
                    Atajo::new("esc", "volver", 2),
                ];
            }
            if estrecho {
                atajos.insert(
                    0,
                    Atajo::new(
                        tab,
                        match app.servicios.foco {
                            crate::app::servicios::Foco::Hosts => "lista",
                            crate::app::servicios::Foco::Lista => "hosts",
                        },
                        1,
                    ),
                );
            }
            atajos
        }
        // La vista Sesión lleva su propia barra de estado.
        Vista::Sesion => vec![],
    };
    if tema.ascii {
        for atajo in &mut atajos {
            atajo.tecla = disposicion::texto_ascii(&atajo.tecla).into();
            atajo.texto = disposicion::texto_ascii(&atajo.texto).into();
        }
    }
    atajos
}
