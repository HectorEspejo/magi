//! Pintado de las sesiones persistentes (Fase 16): el bloque «Persistentes» de
//! la vista Sesiones (§6.1 del informe), la vista previa (§6.3) y los diálogos
//! de la ficha y de renombrar. La nueva sesión la pinta `persistentes_nueva`.
//!
//! Todo se deriva del área en cada pintado y declara su mínimo y su modo
//! estrecho en `disposicion.rs` (T45):
//!
//! - el bloque, con `ESTRECHO_PERSISTENTES`: por debajo solo enseña marca,
//!   glifo de estado, nombre y host; por encima, también estado, actividad,
//!   título y «antigua»;
//! - los diálogos, con `ESTRECHO_DIALOGO_PERSISTENTES`: pierden el texto de
//!   ayuda y acortan las teclas del pie.
//!
//! El ASCII lo degrada `disposicion::texto_ascii` (el único degradador): `◆`
//! pasa a `P`. Nada de lo que se pinta sale de un sitio distinto de la lista del
//! servidor (`FilaPersistente`).

use std::borrow::Cow;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::persistentes::{
    contar, hace, quien_la_tiene, DialogoPersistentes, FilaPersistente, FormularioFicha, Renombrar,
    TipoFila, VistaPrevia, ETIQUETAS_FICHA,
};
use crate::app::App;
use crate::protocolo::EstadoFicha;
use crate::tema::Tema;
use crate::ui::dialogos::{teclas, Hoja};
use crate::ui::disposicion::{
    self, Atajo, Columna, Disposicion, Lista, Minimo, VentanaLista, ESTRECHO_DIALOGO_PERSISTENTES,
    ESTRECHO_PERSISTENTES, MINIMO_PERSISTENTES,
};
use crate::ui::hosts::{anchos_tabla, celdas};

/// Columnas que ocupa lo que va delante de la tabla: selección, marca `◆`,
/// atención `!` y glifo de estado, con sus espacios.
const PREFIJO: u16 = 7;
/// Cabeceras de las columnas, en orden.
const CABECERAS: [&str; 5] = ["ESTADO", "NOMBRE", "HOST", "ACTIVIDAD", "TÍTULO"];
/// Anchos deseados de las hojas (encogen al área).
const ANCHO_VISTA_PREVIA: u16 = 100;
const ANCHO_FICHA: u16 = 72;
const ANCHO_RENOMBRAR: u16 = 64;
/// Una comprobación más vieja que esto se pinta con otro color: es la de una
/// ventana que se fue o la del vigilante sin refrescar.
const EDAD_VIEJA: i64 = 90;

/// Mínimo del diálogo abierto.
#[must_use]
pub fn minimo(dialogo: &DialogoPersistentes) -> Minimo {
    match dialogo {
        DialogoPersistentes::Nueva(formulario) => crate::ui::persistentes_nueva::minimo(formulario),
        DialogoPersistentes::VistaPrevia(_) => Minimo {
            tamano: MINIMO_PERSISTENTES,
            exige: "vista previa",
        },
        DialogoPersistentes::Ficha(_) => Minimo {
            tamano: MINIMO_PERSISTENTES,
            exige: "ficha de la sesión",
        },
        DialogoPersistentes::Renombrar(_) => Minimo {
            tamano: MINIMO_PERSISTENTES,
            exige: "renombrar sesión",
        },
    }
}

/// Pinta el diálogo abierto como una `Hoja` de `dialogos.rs`.
pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoPersistentes,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_DIALOGO_PERSISTENTES;
    let (ancho, hoja) = match dialogo {
        DialogoPersistentes::Nueva(formulario) => {
            crate::ui::persistentes_nueva::dibujar(marco, area, app, formulario, disp);
            return;
        }
        DialogoPersistentes::VistaPrevia(vista) => {
            (ANCHO_VISTA_PREVIA, hoja_vista_previa(vista, estrecho, tema))
        }
        DialogoPersistentes::Ficha(formulario) => (
            ANCHO_FICHA,
            hoja_ficha(
                formulario,
                estrecho,
                app.config.sincronizacion.activa(),
                tema,
            ),
        ),
        DialogoPersistentes::Renombrar(dialogo) => {
            (ANCHO_RENOMBRAR, hoja_renombrar(dialogo, estrecho, tema))
        }
    };
    hoja.ancla(app.desplazamiento_modal)
        .pintar(marco, area, ancho, tema, disp);
}

fn tenue(texto: impl Into<String>, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        texto.into(),
        Style::default().fg(tema.paleta.inactivo),
    ))
}

fn destacado(texto: impl Into<String>, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        texto.into(),
        Style::default()
            .fg(tema.paleta.texto)
            .add_modifier(Modifier::BOLD),
    ))
}

fn en_critico(texto: impl Into<String>, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        texto.into(),
        Style::default().fg(tema.paleta.critico),
    ))
}

/// «14:31:07» en la hora de este equipo.
fn hora_de(epoca: i64) -> String {
    chrono::DateTime::from_timestamp(epoca, 0)
        .map(|momento| {
            momento
                .with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------- vista previa

fn hoja_vista_previa(vista: &VistaPrevia, estrecho: bool, tema: &Tema) -> Hoja {
    let mut cuerpo = vec![tenue(vista.cabecera.clone(), tema), Line::from("")];
    if let Some(error) = &vista.error {
        cuerpo.push(en_critico(format!("no se pudo leer: {error}"), tema));
        cuerpo.push(tenue(
            "Comprueba la conexión con el host y pulsa r para volver a intentarlo.",
            tema,
        ));
    } else if vista.leida_en.is_none() {
        cuerpo.push(tenue("leyendo…", tema));
    } else if vista.lineas.is_empty() {
        cuerpo.push(tenue("(la sesión no tiene nada en pantalla)", tema));
    } else {
        for linea in &vista.lineas {
            cuerpo.push(Line::from(Span::styled(
                format!(" {linea}"),
                Style::default().fg(tema.paleta.texto),
            )));
        }
    }
    // Lo que importa de una terminal es lo último que escribió; de un error, su
    // texto.
    let ultima = if vista.error.is_some() {
        2
    } else {
        cuerpo.len() - 1
    };
    // Con error no se leyó nada: el pie no puede decir «últimas N líneas» (el
    // servidor pone la hora también en las respuestas con error).
    let estado = match (vista.leida_en, vista.error.is_some()) {
        (_, true) => "sin lectura".to_string(),
        (Some(epoca), false) => format!(
            "últimas {} líneas {} {}",
            vista.lineas_pedidas,
            tema.glifos.punto_medio,
            hora_de(epoca)
        ),
        (None, false) => "leyendo…".to_string(),
    };
    let pares: &[(&str, &str)] = if estrecho {
        &[("↵", "engancharse"), ("r", "leer"), ("esc", "volver")]
    } else {
        &[("↵", "engancharse"), ("r", "refrescar"), ("Esc", "volver")]
    };
    let pie = vec![tenue(estado, tema), teclas(pares, tema)];
    Hoja::nueva(
        format!("SESIONES {} VISTA PREVIA", tema.glifos.punto_medio),
        cuerpo,
        pie,
    )
    .foco(ultima)
}

// ------------------------------------------------------------------- ficha

/// `[ texto┃ ]` de un campo de una línea.
fn campo_entre_corchetes(
    campo: &crate::ui::componentes::CampoTexto,
    activo: bool,
    tema: &Tema,
) -> Vec<Span<'static>> {
    let tenue = Style::default().fg(tema.paleta.inactivo);
    vec![
        Span::styled("[ ", tenue),
        campo.span(activo, Style::default().fg(tema.paleta.texto)),
        Span::styled(" ]", tenue),
    ]
}

fn hoja_ficha(formulario: &FormularioFicha, estrecho: bool, sincroniza: bool, tema: &Tema) -> Hoja {
    let objetivo = &formulario.objetivo;
    let mut cuerpo = vec![
        destacado(
            format!(
                "{} {} {}",
                objetivo.nombre, tema.glifos.punto_medio, objetivo.host_nombre
            ),
            tema,
        ),
        Line::from(""),
    ];
    let mut foco = 0;
    for (indice, etiqueta) in ETIQUETAS_FICHA.iter().enumerate() {
        let activo = indice == formulario.foco;
        if activo {
            foco = cuerpo.len();
        }
        let estilo_etiqueta = if activo {
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(tema.paleta.inactivo)
        };
        let mut spans = vec![Span::styled(format!("{etiqueta:<9}"), estilo_etiqueta)];
        spans.extend(campo_entre_corchetes(
            &formulario.campos[indice],
            activo,
            tema,
        ));
        cuerpo.push(Line::from(spans));
    }
    if let Some((_, error)) = &formulario.error {
        cuerpo.push(Line::from(""));
        cuerpo.push(en_critico(error.clone(), tema));
    }
    if !estrecho {
        cuerpo.push(Line::from(""));
        cuerpo.push(tenue(
            "Carpeta absoluta o ~/…; vacía, la carpeta personal del host.",
            tema,
        ));
        // Lo de los otros equipos solo es cierto con la sincronización
        // configurada (D370).
        cuerpo.push(tenue(
            if sincroniza {
                "No pongas secretos en el comando: la ficha viaja cifrada a tus otros equipos."
            } else {
                "No pongas secretos en el comando: queda guardado en la ficha."
            },
            tema,
        ));
    }
    let pares: &[(&str, &str)] = if estrecho {
        &[("↵", "guardar"), ("tab", "sigue"), ("esc", "cancelar")]
    } else {
        &[("↵", "guardar"), ("Tab", "siguiente"), ("Esc", "cancelar")]
    };
    Hoja::nueva("FICHA DE LA SESIÓN", cuerpo, vec![teclas(pares, tema)]).foco(foco)
}

// --------------------------------------------------------------- renombrar

fn hoja_renombrar(dialogo: &Renombrar, estrecho: bool, tema: &Tema) -> Hoja {
    let objetivo = &dialogo.objetivo;
    let mut cuerpo = vec![
        Line::from(format!(
            "Sesión «{}» en {}.",
            objetivo.nombre, objetivo.host_nombre
        )),
        Line::from(""),
    ];
    let foco = cuerpo.len();
    let mut spans = vec![Span::styled(
        "Nombre nuevo  ",
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(campo_entre_corchetes(&dialogo.campo, true, tema));
    cuerpo.push(Line::from(spans));
    if let Some(error) = &dialogo.error {
        cuerpo.push(Line::from(""));
        cuerpo.push(en_critico(error.clone(), tema));
    }
    if !estrecho {
        cuerpo.push(Line::from(""));
        cuerpo.push(tenue(
            "Se renombra en el host y en la ficha; las pestañas enganchadas siguen.",
            tema,
        ));
    }
    let pares: &[(&str, &str)] = if estrecho {
        &[("↵", "renombrar"), ("esc", "cancelar")]
    } else {
        &[("↵", "renombrar"), ("Esc", "cancelar")]
    };
    Hoja::nueva(
        format!(
            "RENOMBRAR {} {}",
            tema.glifos.punto_medio, objetivo.host_nombre
        ),
        cuerpo,
        vec![teclas(pares, tema)],
    )
    .foco(foco)
}

// ------------------------------------------------------------------- bloque

/// Texto con el ASCII degradado si toca.
fn adaptar(texto: &str, tema: &Tema) -> String {
    disposicion::adaptar(texto, tema.ascii)
}

/// Cabecera de un bloque de la vista Sesiones: título, una raya y, si cabe, un
/// texto a la derecha. El bloque con las teclas lleva el título en acento.
pub(crate) fn cabecera_de_bloque(
    titulo: &str,
    derecha: Option<(String, Color)>,
    ancho: u16,
    enfocado: bool,
    tema: &Tema,
) -> Line<'static> {
    let ancho = usize::from(ancho);
    let color_titulo = if enfocado {
        tema.paleta.acento
    } else {
        tema.paleta.texto
    };
    let titulo = disposicion::recortar(&adaptar(titulo, tema), ancho.saturating_sub(2), tema.ascii);
    let usado_izquierda = titulo.chars().count() + 2;
    let derecha = derecha
        .map(|(texto, color)| (adaptar(&texto, tema), color))
        .filter(|(texto, _)| usado_izquierda + texto.chars().count() + 3 <= ancho);
    let usado_derecha = derecha
        .as_ref()
        .map_or(0, |(texto, _)| texto.chars().count() + 2);
    let relleno = ancho.saturating_sub(usado_izquierda + usado_derecha);
    let raya = tema.glifos.linea.repeat(relleno);
    let mut spans = vec![
        Span::styled(
            format!(" {titulo} "),
            Style::default()
                .fg(color_titulo)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(raya, Style::default().fg(tema.paleta.inactivo)),
    ];
    if let Some((texto, color)) = derecha {
        spans.push(Span::styled(
            format!(" {texto} "),
            Style::default().fg(color),
        ));
    }
    Line::from(spans)
}

/// «comprobadas hace 12 s», «comprobando…» o «sin comprobar», con su color y
/// diciendo la verdad: la edad sale de `comprobada_en`, no de cuándo llegó la
/// lista (una ventana nueva recibe la que había al irse la última).
fn texto_de_comprobacion(app: &App) -> Option<(String, Color)> {
    let tema = &app.tema;
    let Some(lista) = &app.persistentes.lista else {
        return Some(("comprobando…".to_string(), tema.paleta.acento));
    };
    let ahora = chrono::Utc::now().timestamp();
    let mas_vieja = lista
        .hosts
        .iter()
        .map(|host| host.comprobada_en)
        .chain(
            lista
                .fichas
                .iter()
                .map(|ficha| ficha.comprobada_en)
                .filter(|comprobada| *comprobada > 0),
        )
        .min();
    let en_vuelo = app.persistentes.consulta.is_some();
    let (mut texto, color) = match mas_vieja {
        Some(epoca) => {
            let edad = ahora.saturating_sub(epoca).max(0);
            let color = if edad > EDAD_VIEJA {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            };
            (format!("comprobadas {}", hace(edad)), color)
        }
        None if lista.fichas.is_empty() => {
            return en_vuelo.then(|| ("comprobando…".to_string(), tema.paleta.acento))
        }
        None => ("sin comprobar".to_string(), tema.paleta.acento),
    };
    if en_vuelo {
        texto.push_str(&format!(" {} actualizando…", tema.glifos.punto_medio));
    }
    Some((texto, color))
}

/// Las columnas de la tabla del bloque. Por debajo de `ESTRECHO_PERSISTENTES`
/// solo quedan nombre y host (las de prioridad 1). «antigua» no es una columna:
/// va al final de la celda del título, solo en las filas que lo son.
fn columnas() -> [Columna; 5] {
    let desde = ESTRECHO_PERSISTENTES.saturating_sub(PREFIJO);
    [
        Columna::fija(13, 3).desde(desde),
        Columna::fija(16, 1),
        Columna::fija(10, 1),
        Columna::fija(10, 3).desde(desde),
        Columna::flexible(8, 3).desde(desde),
    ]
}

/// Ancho que reserva «antigua» al final de la celda del título, con su
/// espacio de separación.
const ANCHO_ANTIGUA: usize = 8;

/// Una celda de texto sencilla, recortada y rellenada a su ancho.
fn celda_simple(texto: &str, estilo: Style, ancho: u16, tema: &Tema) -> Vec<Span<'static>> {
    vec![Span::styled(
        disposicion::columna(&adaptar(texto, tema), usize::from(ancho), tema.ascii),
        estilo,
    )]
}

/// La celda del título; con «antigua» al final si la fila lo es y hay sitio.
fn celda_de_titulo(
    texto: &str,
    estilo: Style,
    antigua: bool,
    ancho: u16,
    tema: &Tema,
) -> Vec<Span<'static>> {
    let ancho = usize::from(ancho);
    if antigua && ancho >= ANCHO_ANTIGUA + 8 {
        vec![
            Span::styled(
                disposicion::columna(&adaptar(texto, tema), ancho - ANCHO_ANTIGUA, tema.ascii),
                estilo,
            ),
            Span::raw(" "),
            Span::styled("antigua", Style::default().fg(tema.paleta.acento)),
        ]
    } else {
        celda_simple(
            texto,
            estilo,
            u16::try_from(ancho).unwrap_or(u16::MAX),
            tema,
        )
    }
}

/// Une las celdas de las columnas visibles con un espacio entre ellas.
fn unir_celdas(celdas: Vec<Vec<Span<'static>>>, anchos: &[u16]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut hay_anterior = false;
    for (celda, ancho) in celdas.into_iter().zip(anchos) {
        if *ancho == 0 {
            continue;
        }
        if hay_anterior {
            spans.push(Span::raw(" "));
        }
        spans.extend(celda);
        hay_anterior = true;
    }
    spans
}

/// Lo que dice la columna ESTADO de una fila.
fn texto_de_estado(fila: &FilaPersistente, este_equipo: &str) -> String {
    match fila.tipo {
        TipoFila::SinFicha => "sin ficha".to_string(),
        TipoFila::Habitual => "de tu tmux".to_string(),
        TipoFila::Ficha => match fila.estado {
            EstadoFicha::VivaLibre => "libre".to_string(),
            EstadoFicha::VivaEnganchada => quien_la_tiene(
                fila.sesion
                    .as_ref()
                    .and_then(|sesion| sesion.equipo.as_deref()),
                este_equipo,
            ),
            EstadoFicha::Terminada => "terminada".to_string(),
            EstadoFicha::SinComprobar | EstadoFicha::Inalcanzable => "sin comprobar".to_string(),
            EstadoFicha::SinTmux => "sin tmux".to_string(),
        },
    }
}

/// Glifo y color del estado de una fila: `●` enganchada, `✕` terminada, `?`
/// sin comprobar (o sin tmux) y nada si está libre.
fn glifo_de_estado(fila: &FilaPersistente, este_equipo: &str, tema: &Tema) -> (String, Color) {
    let paleta = &tema.paleta;
    match (fila.tipo, fila.estado) {
        (TipoFila::Ficha, EstadoFicha::Terminada) => {
            (tema.glifos.error.to_string(), paleta.inactivo)
        }
        (
            TipoFila::Ficha,
            EstadoFicha::SinComprobar | EstadoFicha::Inalcanzable | EstadoFicha::SinTmux,
        ) => ("?".to_string(), paleta.acento),
        _ if fila.enganchada() => {
            let aqui = fila
                .sesion
                .as_ref()
                .and_then(|sesion| sesion.equipo.as_deref())
                == Some(este_equipo);
            (
                tema.glifos.conectado.to_string(),
                if aqui { paleta.correcto } else { paleta.acento },
            )
        }
        _ => (" ".to_string(), paleta.texto),
    }
}

/// Lo que dice la columna TÍTULO: el título del programa o, si no lo hay, lo
/// que hace `↵` o por qué no se ve el estado.
fn texto_de_titulo(fila: &FilaPersistente, tema: &Tema) -> (String, Color) {
    let paleta = &tema.paleta;
    if let Some(titulo) = fila.titulo() {
        return (titulo.to_string(), paleta.texto);
    }
    let intro = tema.glifos.intro;
    match (fila.tipo, fila.estado) {
        (TipoFila::SinFicha, _) => ("a añadir".to_string(), paleta.inactivo),
        (TipoFila::Habitual, _) => (format!("{intro} engancharse"), paleta.inactivo),
        (_, EstadoFicha::Terminada) => (format!("{intro} reabrir"), paleta.inactivo),
        (_, EstadoFicha::Inalcanzable) => (
            match fila.error {
                Some(crate::protocolo::ErrorComprobacion::NecesitaDialogo) => "conéctate a mano",
                Some(crate::protocolo::ErrorComprobacion::SinRespuesta) => "el host no responde",
                Some(crate::protocolo::ErrorComprobacion::Plazo) => "no respondió en 10 s",
                None => "sin respuesta",
            }
            .to_string(),
            paleta.acento,
        ),
        (_, EstadoFicha::SinComprobar) => ("comprobando…".to_string(), paleta.inactivo),
        (_, EstadoFicha::SinTmux) => ("sin tmux 3.0 o posterior".to_string(), paleta.acento),
        _ => ("—".to_string(), paleta.inactivo),
    }
}

/// El bloque «Persistentes»: cabecera, cabeceras de columnas y filas. Registra
/// la ventana de la lista como `Lista::Persistentes`.
pub fn dibujar_bloque(marco: &mut Frame, area: Rect, app: &App, disp: &mut Disposicion) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let tema = &app.tema;
    let ascii = tema.ascii;
    let enfocado = app.persistentes_enfocado();
    let todas = app.filas_persistentes_todas();
    let filas = app.filas_persistentes();
    let (fichas, sin_ficha) = contar(&todas);
    let punto = tema.glifos.punto_medio;
    let mut titulo = format!("PERSISTENTES {punto} {fichas}");
    if sin_ficha > 0 {
        titulo.push_str(&format!(" {punto} {sin_ficha} sin ficha"));
    }
    let filtro = &app.persistentes.filtro.texto;
    if app.persistentes.filtrando || !filtro.is_empty() {
        let cursor = if app.persistentes.filtrando {
            super::tecla(tema, "▏", "|")
        } else {
            ""
        };
        titulo.push_str(&format!("   / {filtro}{cursor}"));
    }
    let derecha = texto_de_comprobacion(app);
    let mut lineas = vec![cabecera_de_bloque(
        &titulo, derecha, area.width, enfocado, tema,
    )];
    let inactivo = Style::default().fg(tema.paleta.inactivo);
    let este_equipo = app.persistentes.etiqueta_equipo.as_deref().unwrap_or("");
    let alto_filas = usize::from(area.height.saturating_sub(2));
    if filas.is_empty() {
        // Sin lista, «comprobando…» ya lo dice la cabecera.
        let texto = if app.persistentes.lista.is_none() {
            String::new()
        } else if !todas.is_empty() {
            format!("ninguna coincide con «{filtro}»")
        } else {
            format!(
                "Sin sesiones persistentes. {} crea una: vive en el host aunque cierres MAGI.",
                super::tecla(tema, "«n»", "\"n\"")
            )
        };
        if area.height >= 2 && !texto.is_empty() {
            lineas.push(Line::from(Span::styled(
                format!(
                    "  {}",
                    disposicion::recortar(
                        &adaptar(&texto, tema),
                        usize::from(area.width).saturating_sub(2),
                        ascii
                    )
                ),
                inactivo,
            )));
        }
        disp.registrar(
            Lista::Persistentes,
            VentanaLista {
                inicio: 0,
                filas: alto_filas,
                total: 0,
            },
        );
        marco.render_widget(Paragraph::new(lineas), area);
        return;
    }

    let ancho_tabla = area.width.saturating_sub(PREFIJO);
    let anchos = anchos_tabla(ancho_tabla, &columnas(), 1);
    if area.height >= 2 {
        let mut cabecera = vec![Span::raw(" ".repeat(usize::from(PREFIJO)))];
        cabecera.extend(celdas(
            CABECERAS
                .iter()
                .map(|texto| ((*texto).to_string(), inactivo))
                .collect(),
            &anchos,
            ascii,
        ));
        lineas.push(Line::from(cabecera));
    }
    let seleccionada = app.indice_persistente(&filas);
    let inicio = disposicion::ventana(
        app.persistentes.desplazamiento,
        seleccionada,
        alto_filas.max(1),
        filas.len(),
    );
    disp.registrar(
        Lista::Persistentes,
        VentanaLista {
            inicio,
            filas: alto_filas,
            total: filas.len(),
        },
    );
    for (indice, fila) in filas.iter().enumerate().skip(inicio).take(alto_filas) {
        let elegida = indice == seleccionada;
        let estilo = if elegida {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let texto = estilo.fg(tema.paleta.texto);
        let marca = if elegida && enfocado {
            tema.glifos.seleccion
        } else {
            " "
        };
        let persistente = if fila.tipo == TipoFila::Ficha {
            super::tecla(tema, "◆", "P")
        } else {
            " "
        };
        let atencion = if fila.atencion() {
            tema.glifos.aviso
        } else {
            " "
        };
        let (glifo, color_glifo) = glifo_de_estado(fila, este_equipo, tema);
        let estado = texto_de_estado(fila, este_equipo);
        let color_estado = match (fila.tipo, fila.estado) {
            (TipoFila::SinFicha | TipoFila::Habitual, _) => tema.paleta.inactivo,
            (_, EstadoFicha::Terminada) => tema.paleta.inactivo,
            (_, EstadoFicha::SinComprobar | EstadoFicha::Inalcanzable | EstadoFicha::SinTmux) => {
                tema.paleta.acento
            }
            (_, EstadoFicha::VivaEnganchada) => {
                if estado == "aquí" {
                    tema.paleta.correcto
                } else {
                    tema.paleta.acento
                }
            }
            _ => tema.paleta.texto,
        };
        let actividad = match fila.actividad_hace() {
            Some(segundos) => hace(segundos),
            None => "—".to_string(),
        };
        let (titulo_texto, color_titulo) = texto_de_titulo(fila, tema);
        let mut spans = vec![
            Span::styled(marca.to_string(), estilo),
            Span::styled(
                persistente.to_string(),
                Style::default().fg(tema.paleta.acento),
            ),
            Span::raw(" "),
            Span::styled(
                atencion.to_string(),
                Style::default()
                    .fg(tema.paleta.critico)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(glifo, estilo.fg(color_glifo)),
            Span::raw(" "),
        ];
        spans.extend(unir_celdas(
            vec![
                celda_simple(&estado, estilo.fg(color_estado), anchos[0], tema),
                celda_simple(&fila.nombre, texto, anchos[1], tema),
                celda_simple(&fila.host_nombre, texto, anchos[2], tema),
                celda_simple(&actividad, texto, anchos[3], tema),
                celda_de_titulo(
                    &titulo_texto,
                    estilo.fg(color_titulo),
                    fila.antigua,
                    anchos[4],
                    tema,
                ),
            ],
            &anchos,
        ));
        lineas.push(Line::from(spans));
    }
    marco.render_widget(Paragraph::new(lineas), area);
}

/// La línea de detalle de la fila elegida, para el panel inferior de la vista
/// Sesiones: nombre, host, estado largo y lo que hace `↵`.
#[must_use]
pub fn detalle_de_fila(app: &App, fila: &FilaPersistente) -> String {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    let intro = tema.glifos.intro;
    let este = app.persistentes.etiqueta_equipo.as_deref().unwrap_or("");
    let mut partes = vec![fila.nombre.clone(), fila.host_nombre.clone()];
    match (fila.tipo, fila.estado) {
        (TipoFila::SinFicha, _) => partes.push("sin ficha: pulsa a para añadirla".to_string()),
        (TipoFila::Habitual, _) => partes.push(format!(
            "de tu tmux de siempre {punto} {intro} engancharse {punto} T terminar"
        )),
        (_, EstadoFicha::Terminada) => {
            partes.push(format!("terminada: {intro} la vuelve a crear"));
        }
        (_, estado) => match fila.motivo() {
            Some(motivo) if estado != EstadoFicha::VivaLibre => partes.push(motivo),
            _ => partes.push(if fila.enganchada() {
                let quien = quien_la_tiene(
                    fila.sesion
                        .as_ref()
                        .and_then(|sesion| sesion.equipo.as_deref()),
                    este,
                );
                format!("enganchada: {quien}")
            } else {
                "libre".to_string()
            }),
        },
    }
    if fila.atencion() {
        partes.push(format!("{} pide atención", tema.glifos.aviso));
    }
    if fila.antigua {
        partes.push("antigua".to_string());
    }
    partes.join(&format!(" {punto} "))
}

/// Atajos de la barra inferior de la vista Sesiones: los del bloque con el
/// foco (Persistentes o Pestañas). La ayuda `?` los lista todos.
#[must_use]
pub fn atajos_sesiones(app: &App) -> Vec<Atajo> {
    let tema = &app.tema;
    let intro = super::tecla(tema, "↵", "enter");
    let tab = super::tecla(tema, "⇥", "tab");
    if app.persistentes.filtrando {
        return vec![
            Atajo::new(intro, "dejar el filtro", 1),
            Atajo::new("esc", "quitarlo", 1),
        ];
    }
    if app.persistentes_enfocado() {
        let mut atajos = vec![
            Atajo::new(intro, "engancharse", 1),
            Atajo::new("c", "compartir", 1),
            Atajo::new("v", "vista previa", 1),
            Atajo::new("n", "nueva", 1),
            Atajo::new("T", "terminar", 2),
            Atajo::new("r", "refrescar", 2),
            Atajo::new(tab, "pestañas", 2),
            Atajo::new("e", "ficha", 3),
            Atajo::new(",", "renombrar", 3),
            Atajo::new("x", "borrar ficha", 3),
            Atajo::new("a", "añadir", 3),
            Atajo::new("C", "limpiar", 4),
            Atajo::new("/", "filtrar", 4),
        ];
        // Una sin ficha se añade; una con ficha se edita.
        if let Some(fila) = app.fila_persistente_elegida() {
            if fila.tipo != TipoFila::SinFicha {
                atajos.retain(|atajo| atajo.tecla != Cow::Borrowed("a"));
            }
        }
        return atajos;
    }
    vec![
        // Los de siempre conservan su prioridad; los nuevos van detrás.
        Atajo::new(intro, "entrar", 1),
        Atajo::new("n", "nueva", 1),
        Atajo::new("r", "reconectar", 2),
        Atajo::new("x", "cerrar", 2),
        Atajo::new("S", "apagar", 3),
        Atajo::new(format!("{} l", app.config.prefijo_escape), "pestañas", 3),
        Atajo::new(tab, "persistentes", 4),
        Atajo::new("t", "tmux", 4),
    ]
}
