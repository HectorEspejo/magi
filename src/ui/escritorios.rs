//! Escritorios remotos (Fase 12) en la vista Sesión: la tarjeta de la
//! pestaña, su barra de estado y los diálogos del certificado y de las
//! credenciales.
//!
//! Todo se deriva del área en cada pintado (Fase 7) y de lo que difunde el
//! servidor (InfoEscritorio). La tarjeta no pinta píxeles: el escritorio se ve
//! en el visor del navegador, que abre el cliente en 127.0.0.1.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::escritorios::DialogoEscritorio;
use crate::app::{App, PestanaUI};
use crate::escritorio::{
    EstadoEscritorio, FaseEscritorio, ImagenEscritorio, InfoEscritorio, TipoCambioCertificado,
    TipoHost,
};
use crate::tema::Tema;
use crate::ui::componentes::CampoTexto;
use crate::ui::dialogos::Hoja;
use crate::ui::disposicion::{self, Disposicion};
use crate::ui::sesion::formatear_duracion;

/// Marca del cursor de los campos de texto.
const CURSOR: char = '┃';
/// Ancho de la etiqueta de la tarjeta.
const ANCHO_ETIQUETA: usize = 12;

// ------------------------------------------------------------------ tarjeta

/// Tarjeta de la pestaña de un escritorio: estado, destino, opciones, imagen,
/// certificado y tiempo. Sin visor cuenta la gracia; caída enseña el motivo.
pub fn tarjeta(marco: &mut Frame, area: Rect, app: &App, pestaña: &PestanaUI) {
    let tema = &app.tema;
    let Some(info) = pestaña
        .escritorio_id
        .and_then(|id| app.escritorios.get(&id))
    else {
        return;
    };
    let host = app.hosts.iter().find(|host| host.id == info.host_id);
    let estrecho = area.width < 60;
    let (estado, color) = estado_visible(info, app, estrecho);
    let punto = tema.glifos.punto_medio;
    let ancho_texto = usize::from(area.width.saturating_sub(2));
    let mut destino = host
        .map(|host| format!("{}:{}", host.direccion, host.puerto))
        .unwrap_or_else(|| info.nombre.clone());
    if let Some(salto) = info.a_traves_de.as_deref() {
        destino.push_str(&format!(" {} {salto}", tema.glifos.salto));
    } else if info.directo && info.tipo == TipoHost::Vnc {
        destino.push_str(&format!(
            " {} directo, sin cifrar",
            super::tecla(tema, "⚠", "!")
        ));
    }
    let mut lineas: Vec<(u8, Line<'static>)> = vec![
        (1, Line::from("")),
        (
            1,
            Line::from(Span::styled(
                titulo_escritorio(info, tema),
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD),
            )),
        ),
        (1, Line::from("")),
        (
            1,
            campo(
                tema,
                "Estado",
                estado,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
                ancho_texto,
            ),
        ),
        (
            2,
            campo(tema, "Destino", destino, texto_de(tema), ancho_texto),
        ),
    ];
    if let Some(host) = host {
        if let Some(usuario) = &host.usuario {
            let usuario = match &host.escritorio_dominio {
                Some(dominio) if !dominio.is_empty() => format!("{dominio}\\{usuario}"),
                _ => usuario.clone(),
            };
            lineas.push((
                4,
                campo(tema, "Usuario", usuario, texto_de(tema), ancho_texto),
            ));
        }
    }
    let tamano = format!(
        "{}{}{}{}",
        info.ancho,
        tema.glifos.por,
        info.alto,
        if info.redimensionable {
            format!(" {punto} se ajusta a la ventana")
        } else {
            format!(" {punto} fijo")
        }
    );
    lineas.push((
        4,
        campo(tema, "Tamaño", tamano, texto_de(tema), ancho_texto),
    ));
    lineas.push((
        4,
        campo(
            tema,
            "Imagen",
            imagen_texto(info, tema),
            texto_de(tema),
            ancho_texto,
        ),
    ));
    if let Some(huella) = host.and_then(|host| host.huella_escritorio.as_ref()) {
        lineas.push((
            6,
            campo(
                tema,
                "Certificado",
                format!("{} (aceptado)", acorta_huella(huella)),
                texto_de(tema),
                ancho_texto,
            ),
        ));
    }
    lineas.push((
        5,
        campo(
            tema,
            "Desde",
            format!("hace {}", formatear_duracion(segundos_desde(info.desde))),
            texto_de(tema),
            ancho_texto,
        ),
    ));
    // Enlace que no se pudo abrir en el navegador: se copia con y (vale 60 s).
    if let Some(enlace) = app.enlace_reciente(info.id) {
        lineas.push((
            3,
            campo(
                tema,
                "Enlace",
                enlace,
                Style::default().fg(tema.paleta.acento),
                ancho_texto,
            ),
        ));
        let restante = app
            .enlace_visible
            .as_ref()
            .map(|(_, _, momento)| {
                crate::app::escritorios::DURACION_ENLACE
                    .as_secs()
                    .saturating_sub(momento.elapsed().as_secs())
            })
            .unwrap_or(0);
        lineas.push((
            3,
            campo(
                tema,
                "",
                format!("cópialo con y: caduca en {restante} s"),
                Style::default().fg(tema.paleta.inactivo),
                ancho_texto,
            ),
        ));
    }
    lineas.push((2, Line::from("")));
    let prefijo = &app.config.prefijo_escape;
    let acciones = if info.estado == EstadoEscritorio::Caido {
        format!("{prefijo} r reconectar   {prefijo} x cerrar")
    } else if estrecho {
        format!(
            "{} visor   y enlace   {prefijo} x cerrar",
            super::tecla(tema, "↵", "enter")
        )
    } else {
        format!(
            "{} abrir el visor   y copiar enlace   {prefijo} x cerrar",
            super::tecla(tema, "↵", "enter")
        )
    };
    lineas.push((
        1,
        Line::from(Span::styled(
            disposicion::recortar(&acciones, ancho_texto, tema.ascii),
            Style::default().fg(tema.paleta.acento),
        )),
    ));
    lineas.push((
        3,
        Line::from(Span::styled(
            disposicion::recortar(
                &format!(
                    "visor del navegador {punto} 127.0.0.1 {punto} la UI nunca recibe píxeles"
                ),
                ancho_texto,
                tema.ascii,
            ),
            Style::default().fg(tema.paleta.inactivo),
        )),
    ));
    // Prioridad: lo que no cabe se cae (las acciones y el estado nunca).
    let alto = usize::from(area.height);
    let mut orden: Vec<usize> = (0..lineas.len()).collect();
    orden.sort_by_key(|indice| (lineas[*indice].0, *indice));
    let mut conservadas = vec![false; lineas.len()];
    for (cuantas, indice) in orden.into_iter().enumerate() {
        if cuantas >= alto {
            break;
        }
        conservadas[indice] = true;
    }
    let visibles: Vec<Line> = lineas
        .into_iter()
        .zip(conservadas)
        .filter(|(_, conservada)| *conservada)
        .map(|((_, linea), _)| linea)
        .collect();
    let zona = Rect {
        x: area.x + 2.min(area.width),
        width: area.width.saturating_sub(2),
        ..area
    };
    marco.render_widget(Paragraph::new(visibles), zona);
}

/// Título de la tarjeta.
fn titulo_escritorio(info: &InfoEscritorio, tema: &Tema) -> String {
    format!(
        "{} ESCRITORIO {} {} {} {}",
        info.tipo.glifo(tema.ascii),
        tema.glifos.punto_medio,
        info.nombre,
        tema.glifos.punto_medio,
        info.tipo.etiqueta()
    )
}

/// Texto de la línea «Estado». En estrecho se abrevia para que quepa el
/// aviso de la gracia.
fn estado_visible(
    info: &InfoEscritorio,
    app: &App,
    estrecho: bool,
) -> (String, ratatui::style::Color) {
    let tema = &app.tema;
    let punto = tema.glifos.punto_medio;
    match info.estado {
        EstadoEscritorio::Caido => (
            format!(
                "{} caído {punto} {}",
                tema.glifos.error,
                info.motivo
                    .clone()
                    .unwrap_or_else(|| "conexión perdida".to_string())
            ),
            tema.paleta.critico,
        ),
        EstadoEscritorio::Cerrando => (
            format!("{} cerrando", tema.glifos.error),
            tema.paleta.critico,
        ),
        EstadoEscritorio::Abriendo => {
            let fase = info
                .fase
                .map(FaseEscritorio::etiqueta)
                .unwrap_or("abriendo");
            (
                format!("{} abriendo {punto} {fase}", tema.glifos.conectando),
                tema.paleta.acento,
            )
        }
        EstadoEscritorio::Abierto => {
            if info.visor {
                (
                    if estrecho {
                        format!("{} abierto {punto} con visor", tema.glifos.conectado)
                    } else {
                        format!("{} abierto {punto} visor abierto", tema.glifos.conectado)
                    },
                    tema.paleta.correcto,
                )
            } else {
                (
                    if estrecho {
                        format!(
                            "{} sin visor {punto} {} s",
                            tema.glifos.conectado,
                            restante_de_gracia(info, app)
                        )
                    } else {
                        format!(
                            "{} abierto {punto} sin visor {punto} se cierra en {} s",
                            tema.glifos.conectado,
                            restante_de_gracia(info, app)
                        )
                    },
                    tema.paleta.acento,
                )
            }
        }
    }
}

/// Segundos que le quedan a la gracia sin visor.
fn restante_de_gracia(info: &InfoEscritorio, app: &App) -> i64 {
    let gracia = i64::try_from(app.config.escritorio.gracia_seg).unwrap_or(60);
    match info.sin_visor_desde {
        Some(desde) => (desde + gracia - crate::modelo::fecha_ahora_epoca()).max(0),
        None => gracia,
    }
}

/// Texto de la imagen que llega al visor.
fn imagen_texto(info: &InfoEscritorio, tema: &Tema) -> String {
    let punto = tema.glifos.punto_medio;
    match info.imagen {
        Some(ImagenEscritorio::H264) => format!("H.264 de Windows {punto} {} fps", info.fps),
        Some(ImagenEscritorio::Rectangulos) => format!("rectángulos {punto} {} fps", info.fps),
        Some(ImagenEscritorio::Voxl) => "la página propia de voxl".to_string(),
        None => "sin imagen todavía".to_string(),
    }
}

fn segundos_desde(epoca: i64) -> u64 {
    (crate::modelo::fecha_ahora_epoca() - epoca).max(0) as u64
}

/// Recorta una huella como en el resto de MAGI (el ASCII lo degrada el
/// adaptador al pintar).
fn acorta_huella(huella: &str) -> String {
    let sin_prefijo = huella.trim_start_matches("SHA256:");
    let recorte: String = sin_prefijo.chars().take(12).collect();
    format!("SHA256:{recorte}…")
}

fn texto_de(tema: &Tema) -> Style {
    Style::default().fg(tema.paleta.texto)
}

fn campo(tema: &Tema, etiqueta: &str, valor: String, estilo: Style, ancho: usize) -> Line<'static> {
    let hueco = ancho.saturating_sub(2 + ANCHO_ETIQUETA).max(1);
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{etiqueta:<ANCHO_ETIQUETA$}"),
            Style::default().fg(tema.paleta.inactivo),
        ),
        Span::styled(
            disposicion::recortar(&disposicion::adaptar(&valor, tema.ascii), hueco, tema.ascii),
            estilo,
        ),
    ])
}

// -------------------------------------------------------------------- barra

/// Barra de estado de la pestaña de un escritorio (la de Sesión): glifo,
/// posición, estado, visor y fps, por prioridad.
pub fn barra(area: Rect, app: &App, pestaña: &PestanaUI, indice: usize) -> Line<'static> {
    let tema = &app.tema;
    let Some(info) = pestaña
        .escritorio_id
        .and_then(|id| app.escritorios.get(&id))
    else {
        return Line::default();
    };
    let (estado, color) = estado_visible(info, app, false);
    let estado = estado.split(' ').nth(1).unwrap_or("abierto").to_string();
    let visor = if info.visor {
        "visor abierto".to_string()
    } else if info.estado == EstadoEscritorio::Abierto {
        "sin visor".to_string()
    } else {
        String::new()
    };
    let cabeza = Elemento::fijo(
        format!(" {} ", info.tipo.glifo(tema.ascii)),
        1,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    );
    let texto = texto_de(tema);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let mut elementos = vec![
        Elemento::fijo(format!("{}/{}", indice + 1, app.pestanas.len()), 2, texto),
        Elemento::fijo(estado, 3, texto),
    ];
    if !visor.is_empty() {
        elementos.push(Elemento::fijo(visor, 4, texto));
    }
    if let Some(ImagenEscritorio::H264 | ImagenEscritorio::Rectangulos) = info.imagen {
        elementos.push(Elemento::fijo(format!("{} fps", info.fps), 6, texto));
    }
    let prefijo = &app.config.prefijo_escape;
    elementos.push(Elemento::new(
        format!("{prefijo} ? ayuda"),
        format!("{prefijo} ?"),
        1,
        tenue,
    ));
    componer(&cabeza, &elementos, area.width, tema)
}

struct Elemento {
    completo: String,
    compacto: String,
    prioridad: u8,
    estilo: Style,
}

impl Elemento {
    fn fijo(texto: String, prioridad: u8, estilo: Style) -> Self {
        Self::new(texto.clone(), texto, prioridad, estilo)
    }

    fn new(completo: String, compacto: String, prioridad: u8, estilo: Style) -> Self {
        Self {
            completo,
            compacto,
            prioridad,
            estilo,
        }
    }
}

fn componer(cabeza: &Elemento, elementos: &[Elemento], ancho: u16, tema: &Tema) -> Line<'static> {
    let ancho_util = usize::from(ancho);
    let separador = format!(" {} ", tema.glifos.punto_medio);
    let mut quitables: Vec<usize> = (0..elementos.len())
        .filter(|indice| elementos[*indice].prioridad > 1)
        .collect();
    quitables.sort_by_key(|indice| std::cmp::Reverse((elementos[*indice].prioridad, *indice)));
    let mut ultima = Line::default();
    for compacto in [false, true] {
        for quitados in 0..=quitables.len() {
            let visibles = elementos
                .iter()
                .enumerate()
                .filter(|(indice, _)| !quitables[..quitados].contains(indice))
                .map(|(_, elemento)| elemento);
            let mut spans = vec![Span::styled(
                if compacto {
                    cabeza.compacto.clone()
                } else {
                    cabeza.completo.clone()
                },
                cabeza.estilo,
            )];
            for (posicion, elemento) in visibles.enumerate() {
                if posicion > 0 {
                    spans.push(Span::styled(
                        separador.clone(),
                        Style::default().fg(tema.paleta.inactivo),
                    ));
                }
                let texto = if compacto {
                    elemento.compacto.clone()
                } else {
                    elemento.completo.clone()
                };
                spans.push(Span::styled(
                    disposicion::adaptar(&texto, tema.ascii),
                    elemento.estilo,
                ));
            }
            ultima = Line::from(spans);
            if ultima.width() <= ancho_util {
                return ultima;
            }
        }
    }
    ultima
}

// ----------------------------------------------------------------- diálogos

/// Diálogos del escritorio: certificado nuevo o cambiado, credenciales y
/// aviso de VNC directo sin cifrar.
pub fn dibujar_dialogo(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoEscritorio,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let mut mostrar = |ancho: u16, hoja: Hoja| {
        hoja.ancla(app.desplazamiento_modal)
            .pintar(marco, area, ancho, tema, disp);
    };
    match dialogo {
        DialogoEscritorio::Certificado {
            host_nombre,
            datos,
            campo,
            error,
            ..
        } => {
            let cambiado = datos.tipo == TipoCambioCertificado::Cambiado;
            let critico = Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD);
            let acento = Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD);
            let texto = texto_de(tema);
            let tenue = Style::default().fg(tema.paleta.inactivo);
            let mut cuerpo = Vec::new();
            if cambiado {
                cuerpo.push(Line::from(Span::styled(
                    format!(
                        "{} El certificado de {host_nombre} ha cambiado.",
                        tema.glifos.error
                    ),
                    critico,
                )));
                cuerpo.push(Line::from(Span::styled(
                    "Puede ser una reinstalación o alguien en medio.",
                    texto,
                )));
            } else {
                cuerpo.push(Line::from(Span::styled(
                    format!("El certificado de {host_nombre} es nuevo."),
                    acento,
                )));
                cuerpo.push(Line::from(Span::styled(
                    "Acéptalo solo si es el equipo al que te vas a conectar.",
                    texto,
                )));
            }
            cuerpo.push(Line::from(""));
            if let Some(anterior) = &datos.anterior {
                cuerpo.push(Line::from(vec![
                    Span::styled("Anterior  ", tenue),
                    Span::styled(anterior.clone(), Style::default().fg(tema.paleta.inactivo)),
                ]));
            }
            cuerpo.push(Line::from(vec![
                Span::styled("Nuevo     ", tenue),
                Span::styled(
                    format!("{} {}", datos.huella, datos.sujeto),
                    Style::default().fg(tema.paleta.acento),
                ),
            ]));
            cuerpo.push(Line::from(vec![
                Span::styled("Caduca    ", tenue),
                Span::styled(datos.caducidad.clone(), texto),
            ]));
            if let Some(error) = error {
                cuerpo.push(Line::from(""));
                cuerpo.push(Line::from(Span::styled(error.clone(), critico)));
            }
            let pie = if cambiado {
                cuerpo.push(Line::from(""));
                cuerpo.push(Line::from(Span::styled(
                    format!("Para sustituirlo, escribe el nombre del host ({host_nombre}):"),
                    texto,
                )));
                vec![
                    Line::from(vec![
                        Span::styled("[ ", Style::default().fg(tema.paleta.critico)),
                        campo.span(true, Style::default().fg(tema.paleta.critico)),
                        Span::styled(" ]", Style::default().fg(tema.paleta.critico)),
                    ]),
                    super::dialogos::teclas(
                        &[("r", "sustituir y conectar"), ("esc", "no conectar")],
                        tema,
                    ),
                ]
            } else {
                vec![super::dialogos::teclas(
                    &[("a", "aceptar y guardar"), ("esc", "no conectar")],
                    tema,
                )]
            };
            let titulo = match datos.tipo {
                TipoCambioCertificado::Nuevo => "CERTIFICADO NUEVO",
                TipoCambioCertificado::Cambiado => "CERTIFICADO CAMBIADO",
            };
            mostrar(74, Hoja::nueva(titulo, cuerpo, pie).peligro(cambiado));
        }
        DialogoEscritorio::Credenciales {
            host_nombre,
            peticion,
            usuario,
            dominio,
            contrasena,
            recordar,
            foco,
            error,
            ..
        } => {
            let foco = *foco;
            let recordar = *recordar;
            let es_vnc = peticion.tipo == TipoHost::Vnc;
            let con_llavero = crate::llavero::disponible();
            let texto = texto_de(tema);
            let tenue = Style::default().fg(tema.paleta.inactivo);
            let acento = Style::default().fg(tema.paleta.acento);
            let mut cuerpo = vec![
                Line::from(Span::styled(
                    format!(
                        "Credenciales para «{host_nombre}» {} {} {} intento {} de 3",
                        tema.glifos.punto_medio,
                        peticion.tipo.etiqueta(),
                        tema.glifos.punto_medio,
                        peticion.intento
                    ),
                    texto,
                )),
                Line::from(""),
            ];
            let mut enfocada = 0usize;
            let campo_linea = |cuerpo: &mut Vec<Line<'static>>,
                               etiqueta: &str,
                               valor: Vec<Span<'static>>,
                               activo: bool,
                               enfocada: &mut usize| {
                if activo {
                    *enfocada = cuerpo.len();
                }
                let estilo_etiqueta = if activo {
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD)
                } else {
                    tenue
                };
                let mut spans = vec![Span::styled(format!("  {etiqueta:<10}"), estilo_etiqueta)];
                spans.extend(valor);
                cuerpo.push(Line::from(spans));
            };
            if !es_vnc {
                campo_linea(
                    &mut cuerpo,
                    "Usuario",
                    vec![
                        Span::styled("[ ", acento),
                        usuario.span(foco == 0, texto),
                        Span::styled(" ]", acento),
                    ],
                    foco == 0,
                    &mut enfocada,
                );
                campo_linea(
                    &mut cuerpo,
                    "Dominio",
                    vec![
                        Span::styled("[ ", acento),
                        dominio.span(foco == 1, texto),
                        Span::styled(" ]", acento),
                    ],
                    foco == 1,
                    &mut enfocada,
                );
            }
            campo_linea(
                &mut cuerpo,
                "Contraseña",
                vec![
                    Span::styled("[ ", acento),
                    Span::styled(
                        enmascarado(contrasena, foco == 2, tema),
                        if foco == 2 { texto } else { tenue },
                    ),
                    Span::styled(" ]", acento),
                ],
                foco == 2,
                &mut enfocada,
            );
            cuerpo.push(Line::from(""));
            if con_llavero {
                let estilo = if foco == 3 {
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD)
                } else if recordar {
                    Style::default().fg(tema.paleta.correcto)
                } else {
                    texto
                };
                cuerpo.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!(
                            "{} recordar en el llavero del sistema",
                            crate::ui::componentes::casilla(recordar)
                        ),
                        estilo,
                    ),
                ]));
                cuerpo.push(Line::from(Span::styled(
                    if recordar {
                        "  la contraseña queda cifrada en el llavero, nunca en magi.db"
                    } else {
                        "  sin marcar: se pedirá en cada conexión y no se guarda"
                    },
                    tenue,
                )));
            } else {
                cuerpo.push(Line::from(Span::styled(
                    "  el llavero del sistema no está disponible: no se puede recordar",
                    tenue,
                )));
            }
            if let Some(error) = error {
                cuerpo.push(Line::from(""));
                cuerpo.push(Line::from(Span::styled(
                    error.clone(),
                    Style::default()
                        .fg(tema.paleta.critico)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            let mut pares: Vec<(&str, &str)> = vec![("↵", "aceptar"), ("⇥", "campo")];
            if con_llavero {
                pares.push(("espacio", "marcar"));
            }
            pares.push(("esc", "cancelar"));
            let pie = vec![super::dialogos::teclas(&pares, tema)];
            mostrar(
                66,
                Hoja::nueva(
                    format!("CREDENCIALES {} {host_nombre}", tema.glifos.punto_medio),
                    cuerpo,
                    pie,
                )
                .foco(enfocada),
            );
        }
        DialogoEscritorio::VncDirecto { aviso, .. } => {
            let tema = &app.tema;
            let cuerpo = vec![
                Line::from(Span::styled(
                    format!("{} VNC directo, sin cifrar", super::tecla(tema, "⚠", "!")),
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(aviso.clone(), texto_de(tema))),
                Line::from(""),
                Line::from(Span::styled(
                    "Un VNC directo no cifra la imagen ni el teclado. Si el host está en otra red, mejor a través de un host SSH.",
                    Style::default().fg(tema.paleta.inactivo),
                )),
            ];
            let pie = vec![super::dialogos::teclas(
                &[("s", "abrir sin cifrar"), ("esc", "cancelar")],
                tema,
            )];
            mostrar(
                72,
                Hoja::nueva("VNC DIRECTO SIN CIFRAR", cuerpo, pie).peligro(true),
            );
        }
    }
}

/// Campo enmascarado de contraseña, con el cursor donde esté.
fn enmascarado(campo: &CampoTexto, activo: bool, tema: &Tema) -> String {
    let mascara = |_: char| if tema.ascii { '*' } else { '•' };
    if campo.texto.is_empty() {
        return if activo {
            CURSOR.to_string()
        } else {
            String::new()
        };
    }
    let mut visible = String::new();
    if activo {
        let mut puesto = false;
        for (indice, caracter) in campo.texto.chars().enumerate() {
            if indice == campo.cursor {
                visible.push(CURSOR);
                puesto = true;
            }
            visible.push(mascara(caracter));
        }
        if !puesto {
            visible.push(CURSOR);
        }
    } else {
        visible = campo.texto.chars().map(mascara).collect();
    }
    visible
}
