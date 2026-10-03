//! Diálogos modales. Todos encogen al área disponible (Fase 7): el texto se
//! parte en renglones del ancho que haya, el pie con las teclas queda fijo
//! abajo y, si el cuerpo no cabe, se desplaza. Los informativos (confirmar,
//! detalle, huellas, conflictos…) se desplazan con ↑ ↓ PgUp PgDn; los de
//! formulario siguen al campo con foco. Nada se guarda entre pintados: la
//! ventana visible se registra como `Lista::Modal` y el bucle la devuelve a
//! `App::desplazamiento_modal`.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, Dialogo};
use crate::tema::Tema;
use crate::ui::centrar;
use crate::ui::componentes::CampoTexto;
use crate::ui::disposicion::{self, Disposicion, Lista, VentanaLista};

/// Marca del cursor de los campos de texto (la de `CampoTexto::span`).
const CURSOR: char = '\u{2503}';
/// Al desplazar, la línea en blanco antes del pie se conserva si el cuerpo
/// sigue teniendo al menos estas filas.
const FILAS_CON_SEPARADOR: usize = 4;
/// Una etiqueta de formulario más larga que esto se trata como texto (sin
/// sangría colgante al partir la línea).
const ETIQUETA_MAXIMA: usize = 16;

fn linea(contenido: &str) -> Line<'static> {
    Line::from(Span::raw(contenido.to_string()))
}

fn span_texto(contenido: &str) -> Span<'static> {
    Span::raw(contenido.to_string())
}

pub(crate) fn atajo(tecla: &str, tema: &Tema) -> Span<'static> {
    Span::styled(
        tecla.to_string(),
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    )
}

/// Línea de teclas del pie: `tecla texto`, separadas por tres espacios (el
/// reparto en renglones prefiere cortar ahí).
fn teclas(pares: &[(&str, &str)], tema: &Tema) -> Line<'static> {
    let mut spans = Vec::new();
    for (indice, (tecla, texto)) in pares.iter().enumerate() {
        if indice > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(atajo(tecla, tema));
        spans.push(span_texto(&format!(" {texto}")));
    }
    Line::from(spans)
}

/// `[ texto┃ ]` de un campo de texto.
fn campo_entre_corchetes(
    campo: &CampoTexto,
    activo: bool,
    estilo: Style,
    color_corchetes: Color,
) -> Vec<Span<'static>> {
    vec![
        Span::styled("[ ", Style::default().fg(color_corchetes)),
        campo.span(activo, estilo),
        Span::styled(" ]", Style::default().fg(color_corchetes)),
    ]
}

/// Campo enmascarado de frase: el cursor si está vacío, puntos si no.
fn enmascarado(campo: &CampoTexto, tema: &Tema) -> String {
    if campo.texto.is_empty() {
        CURSOR.to_string()
    } else {
        campo
            .texto
            .chars()
            .map(|_| if tema.ascii { '*' } else { '•' })
            .collect()
    }
}

/// Campo enmascarado de contraseña, con el cursor donde esté (o al final si
/// el foco está en la casilla).
fn enmascarado_con_cursor(campo: &CampoTexto, foco_casilla: bool, tema: &Tema) -> String {
    let mascara = |_: char| if tema.ascii { '*' } else { '•' };
    let mut visible = String::new();
    if !foco_casilla {
        let mut cursor_puesto = false;
        for (indice, caracter) in campo.texto.chars().enumerate() {
            if indice == campo.cursor {
                visible.push(CURSOR);
                cursor_puesto = true;
            }
            visible.push(mascara(caracter));
        }
        if !cursor_puesto {
            visible.push(CURSOR);
        }
    } else if campo.texto.is_empty() {
        visible.push(CURSOR);
    } else {
        visible = campo.texto.chars().map(mascara).collect();
    }
    visible
}

/// Cuerpo de los diálogos de contraseña (local y del servidor). Devuelve las
/// líneas y la del foco.
fn cuerpo_contrasena(
    host: &str,
    intento: u8,
    campo: &CampoTexto,
    recordar: bool,
    foco_casilla: bool,
    tema: &Tema,
) -> (Vec<Line<'static>>, usize) {
    let estilo_casilla = if foco_casilla {
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD)
    } else if recordar {
        Style::default().fg(tema.paleta.correcto)
    } else {
        Style::default().fg(tema.paleta.texto)
    };
    let cuerpo = vec![
        linea(&format!(
            "Contraseña para «{host}» · intento {intento} de 3"
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
            Span::styled(
                enmascarado_con_cursor(campo, foco_casilla, tema),
                if foco_casilla {
                    Style::default().fg(tema.paleta.inactivo)
                } else {
                    Style::default().fg(tema.paleta.texto)
                },
            ),
            Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!(
                    "{} recordar en el llavero del sistema",
                    crate::ui::componentes::casilla(recordar)
                ),
                estilo_casilla,
            ),
        ]),
        Line::from(Span::styled(
            if recordar {
                "  la contraseña queda cifrada en el llavero, nunca en magi.db"
            } else {
                "  sin marcar: se pedirá en cada conexión y no se guarda"
            },
            Style::default().fg(tema.paleta.inactivo),
        )),
    ];
    (cuerpo, if foco_casilla { 4 } else { 2 })
}

/// Cuerpo de los diálogos de frase (local y del servidor).
fn cuerpo_frase(host: &str, intento: u8, campo: &CampoTexto, tema: &Tema) -> Vec<Line<'static>> {
    vec![
        linea(&format!(
            "Clave con frase para «{host}» · intento {intento} de 3"
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
            Span::styled(
                enmascarado(campo, tema),
                Style::default().fg(tema.paleta.texto),
            ),
            Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
        ]),
    ]
}

/// Cuerpo de las huellas desconocidas (local y del servidor).
fn cuerpo_huella(host: &str, tipo: &str, huella: &str, tema: &Tema) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            span_texto("Host: "),
            Span::styled(host.to_string(), Style::default().fg(tema.paleta.texto)),
        ]),
        Line::from(vec![
            span_texto("Tipo:  "),
            Span::styled(tipo.to_string(), Style::default().fg(tema.paleta.acento)),
        ]),
        Line::from(vec![
            span_texto("Huella: "),
            Span::styled(huella.to_string(), Style::default().fg(tema.paleta.acento)),
        ]),
        Line::from(""),
        linea("La huella no está en known_hosts. Comprueba que es la correcta."),
    ]
}

/// Cuerpo y pie de las huellas cambiadas: el campo va en el pie para que se
/// vea lo que se escribe aunque el cuerpo esté desplazado.
fn huella_cambiada(
    host: &str,
    anterior: &str,
    nueva: &str,
    campo: &CampoTexto,
    tema: &Tema,
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let cuerpo = vec![
        Line::from(Span::styled(
            format!(
                "{} La clave del servidor NO coincide con known_hosts",
                tema.glifos.error
            ),
            Style::default()
                .fg(tema.paleta.critico)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            span_texto("Anterior  "),
            Span::styled(
                anterior.to_string(),
                Style::default().fg(tema.paleta.inactivo),
            ),
        ]),
        Line::from(vec![
            span_texto("Nueva     "),
            Span::styled(nueva.to_string(), Style::default().fg(tema.paleta.critico)),
        ]),
        Line::from(""),
        linea("Puede ser una reinstalación… o un intermediario."),
        linea(&format!(
            "Para sustituir la huella escribe el nombre del host ({host}):"
        )),
    ];
    let pie = vec![
        Line::from(campo_entre_corchetes(
            campo,
            true,
            Style::default().fg(tema.paleta.critico),
            tema.paleta.critico,
        )),
        teclas(
            &[("r", "sustituir"), ("esc", "cancelar (recomendado)")],
            tema,
        ),
    ];
    (cuerpo, pie)
}

pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &Dialogo,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    // Todos los diálogos de este fichero se registran como `Lista::Modal`
    // anclados al desplazamiento que dejó el último pintado.
    let mut mostrar = |ancho: u16, hoja: Hoja| {
        hoja.ancla(app.desplazamiento_modal)
            .pintar(marco, area, ancho, tema, disp);
    };
    match dialogo {
        Dialogo::Snippets(crate::app::DialogoSnippets::Formulario(formulario)) => {
            crate::ui::formulario_snippet::dibujar(marco, area, app, formulario);
        }
        Dialogo::Ejecutar(dialogo) => {
            crate::ui::ejecutar::dibujar_con_disposicion(marco, area, app, dialogo, disp);
        }
        Dialogo::Edicion(dialogo) => crate::ui::edicion::dibujar(marco, area, app, dialogo, disp),
        Dialogo::Permisos(dialogo) => {
            crate::ui::permisos::dibujar(marco, area, app, dialogo, disp);
        }
        Dialogo::Sincronizar(dialogo) => {
            crate::ui::sincronizar::dibujar(marco, area, app, dialogo, disp);
        }
        Dialogo::Guardadas(dialogo) => {
            crate::ui::sincronizaciones::dibujar(marco, area, app, dialogo, disp);
        }
        Dialogo::Confirmar {
            titulo,
            lineas,
            peligro,
            ..
        } => {
            let cuerpo = lineas.iter().map(|texto| linea(texto)).collect();
            let pie = vec![teclas(&[("s", "confirmar"), ("n / esc", "cancelar")], tema)];
            mostrar(66, Hoja::nueva(titulo, cuerpo, pie).peligro(*peligro));
        }
        Dialogo::ServidorCaido { perdidas } => {
            let mut cuerpo = vec![Line::from(Span::styled(
                format!(
                    "{} El servidor de sesiones ha dejado de responder",
                    tema.glifos.error
                ),
                Style::default()
                    .fg(tema.paleta.critico)
                    .add_modifier(Modifier::BOLD),
            ))];
            if *perdidas > 0 {
                cuerpo.push(linea(&format!("  {perdidas} sesión(es) se han perdido.")));
            }
            cuerpo.push(Line::from(""));
            cuerpo.push(linea("  Detalle en el registro y en logs/servidor.log."));
            let pie = vec![teclas(
                &[("s", "relanzar servidor"), ("n", "seguir sin sesiones")],
                tema,
            )];
            mostrar(70, Hoja::nueva("SERVIDOR CAÍDO", cuerpo, pie).peligro(true));
        }
        Dialogo::HuellaServidor {
            host, tipo, huella, ..
        } => {
            let pie = vec![teclas(
                &[("a", "aceptar y añadir"), ("x", "cancelar la apertura")],
                tema,
            )];
            mostrar(
                66,
                Hoja::nueva(
                    "HUELLA DESCONOCIDA",
                    cuerpo_huella(host, tipo, huella, tema),
                    pie,
                ),
            );
        }
        Dialogo::HuellaDesconocida {
            host, tipo, huella, ..
        } => {
            let pie = vec![teclas(
                &[("a", "aceptar y añadir"), ("esc", "cancelar")],
                tema,
            )];
            mostrar(
                66,
                Hoja::nueva(
                    "HUELLA DESCONOCIDA",
                    cuerpo_huella(host, tipo, huella, tema),
                    pie,
                ),
            );
        }
        Dialogo::HuellaCambiadaServidor {
            host,
            anterior,
            nueva,
            campo,
            ..
        }
        | Dialogo::HuellaCambiada {
            host,
            anterior,
            nueva,
            campo,
            ..
        } => {
            let (cuerpo, pie) = huella_cambiada(host, anterior, nueva, campo, tema);
            mostrar(
                74,
                Hoja::nueva("HUELLA CAMBIADA", cuerpo, pie).peligro(true),
            );
        }
        Dialogo::FraseServidor {
            host,
            intento,
            campo,
            ..
        } => {
            let pie = vec![teclas(
                &[("↵", "aceptar"), ("esc", "cancelar la apertura")],
                tema,
            )];
            // El host va también en el título: con poco alto el cuerpo se
            // desplaza hasta el campo y el título es lo único que queda.
            mostrar(
                60,
                Hoja::nueva(
                    format!("FRASE DE LA CLAVE · {host}"),
                    cuerpo_frase(host, *intento, campo, tema),
                    pie,
                )
                .foco(2),
            );
        }
        Dialogo::Frase {
            host,
            intento,
            campo,
            ..
        } => {
            let pie = vec![teclas(&[("↵", "aceptar"), ("esc", "cancelar")], tema)];
            mostrar(
                60,
                Hoja::nueva(
                    format!("FRASE DE LA CLAVE · {host}"),
                    cuerpo_frase(host, *intento, campo, tema),
                    pie,
                )
                .foco(2),
            );
        }
        Dialogo::ContrasenaServidor {
            host,
            intento,
            campo,
            recordar,
            foco_casilla,
            ..
        }
        | Dialogo::Contrasena {
            host,
            intento,
            campo,
            recordar,
            foco_casilla,
            ..
        } => {
            let (cuerpo, foco) =
                cuerpo_contrasena(host, *intento, campo, *recordar, *foco_casilla, tema);
            let pie = vec![teclas(
                &[
                    ("↵", "aceptar"),
                    ("⇥", "campo/casilla"),
                    ("espacio", "marcar"),
                    ("esc", "cancelar"),
                ],
                tema,
            )];
            mostrar(
                64,
                Hoja::nueva(format!("CONTRASEÑA · {host}"), cuerpo, pie).foco(foco),
            );
        }
        Dialogo::MenuGrupo { seleccion } => {
            let opciones = [
                "nuevo grupo",
                "renombrar grupo…",
                "mover host a grupo…",
                "borrar grupo",
                "subir orden",
                "bajar orden",
            ];
            let cuerpo = opciones
                .iter()
                .enumerate()
                .map(|(indice, opcion)| {
                    let elegida = indice == *seleccion;
                    let estilo = if elegida {
                        Style::default()
                            .bg(tema.paleta.acento)
                            .fg(tema.paleta.fondo)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(tema.paleta.texto)
                    };
                    Line::from(Span::styled(
                        format!(
                            " {}  {opcion}",
                            if elegida { tema.glifos.seleccion } else { " " }
                        ),
                        estilo,
                    ))
                })
                .collect();
            let pie = vec![teclas(&[("↵", "elegir"), ("esc", "cerrar")], tema)];
            mostrar(
                46,
                Hoja::nueva("MENÚ DE GRUPO", cuerpo, pie).foco(*seleccion),
            );
        }
        Dialogo::EntradaTexto {
            titulo,
            etiqueta,
            campo,
            ..
        } => {
            let mut spans = vec![span_texto(&format!("{etiqueta}: "))];
            spans.extend(campo_entre_corchetes(
                campo,
                true,
                Style::default().fg(tema.paleta.texto),
                tema.paleta.acento,
            ));
            let pie = vec![teclas(&[("↵", "aceptar"), ("esc", "cancelar")], tema)];
            mostrar(
                56,
                Hoja::nueva(titulo, vec![Line::from(spans)], pie).foco(0),
            );
        }
        Dialogo::MoverHost {
            host_nombre,
            desplegable,
            ..
        } => {
            let mut cuerpo = vec![Line::from(vec![
                span_texto("Host: "),
                Span::styled(host_nombre.clone(), Style::default().fg(tema.paleta.acento)),
            ])];
            for (posicion, indice) in desplegable.filtradas().iter().enumerate() {
                let opcion = &desplegable.opciones[*indice];
                let elegida = posicion == desplegable.resaltado;
                let estilo = if elegida {
                    Style::default()
                        .bg(tema.paleta.acento)
                        .fg(tema.paleta.fondo)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(tema.paleta.texto)
                };
                cuerpo.push(Line::from(Span::styled(
                    format!(" {}", opcion.etiqueta),
                    estilo,
                )));
            }
            let mut pie = vec![teclas(&[("↵", "mover"), ("esc", "cancelar")], tema)];
            if !desplegable.filtro.texto.is_empty() {
                pie.push(Line::from(Span::styled(
                    format!("filtro: {}", desplegable.filtro.texto),
                    Style::default().fg(tema.paleta.inactivo),
                )));
            }
            mostrar(
                52,
                Hoja::nueva("MOVER HOST A GRUPO", cuerpo, pie)
                    .foco(1 + desplegable.resaltado)
                    .alto_minimo(14),
            );
        }
        Dialogo::ResumenImportacion { titulo, lineas } => {
            let cuerpo = lineas.iter().map(|texto| linea(texto)).collect();
            let pie = vec![teclas(&[("↵ / esc", "cerrar")], tema)];
            mostrar(74, Hoja::nueva(titulo, cuerpo, pie));
        }
        Dialogo::Conflicto {
            nombre,
            es_dir,
            lado_origen,
            tamano_origen,
            fecha_origen,
            tamano_destino,
            fecha_destino,
        } => {
            let (etiqueta_origen, etiqueta_destino) = match lado_origen {
                crate::archivos::Lado::Local => ("local", "remoto"),
                crate::archivos::Lado::Remoto => ("remoto", "local"),
            };
            let cuerpo = vec![
                Line::from(Span::styled(
                    format!("  {nombre}"),
                    Style::default()
                        .fg(tema.paleta.texto)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!(
                        "    {:<8} {:>10}   {}   (origen)",
                        etiqueta_origen,
                        crate::archivos::tamano_legible(*tamano_origen),
                        crate::archivos::fecha_completa(*fecha_origen)
                    ),
                    Style::default().fg(tema.paleta.acento),
                )),
                Line::from(Span::styled(
                    format!(
                        "    {:<8} {:>10}   {}   (destino)",
                        etiqueta_destino,
                        crate::archivos::tamano_legible(*tamano_destino),
                        crate::archivos::fecha_completa(*fecha_destino)
                    ),
                    Style::default().fg(tema.paleta.critico),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    match es_dir {
                        true => "  Es un directorio: la decisión vale para todo su contenido.",
                        false => "  Ya existe algo con ese nombre en el destino.",
                    },
                    Style::default().fg(tema.paleta.inactivo),
                )),
            ];
            let pie = vec![
                teclas(
                    &[
                        ("s", "sobrescribir"),
                        ("o", "omitir"),
                        ("S", "todos"),
                        ("O", "omitir todos"),
                    ],
                    tema,
                ),
                teclas(&[("esc", "cancelar la operación entera")], tema),
            ];
            mostrar(
                74,
                Hoja::nueva("YA EXISTE EN EL DESTINO", cuerpo, pie).peligro(true),
            );
        }
        Dialogo::Detalle { titulo, lineas, .. } => {
            let cuerpo = lineas.iter().map(|texto| linea(texto)).collect();
            let pie = vec![teclas(&[("↵ / esc", "cerrar")], tema)];
            mostrar(74, Hoja::nueva(titulo, cuerpo, pie));
        }
        Dialogo::ExportarRegistro { estado } => {
            let marca = |activo: bool| if activo { "(•)" } else { "( )" };
            let estilo_foco = |foco: bool| {
                if foco {
                    Style::default()
                        .fg(tema.paleta.acento)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(tema.paleta.texto)
                }
            };
            let mut ruta = vec![Span::styled("Ruta     ", estilo_foco(estado.foco == 1))];
            ruta.extend(campo_entre_corchetes(
                &estado.campo,
                estado.foco == 1,
                Style::default().fg(tema.paleta.texto),
                tema.paleta.acento,
            ));
            let cuerpo = vec![
                Line::from(vec![
                    Span::styled("Formato  ", estilo_foco(estado.foco == 0)),
                    Span::raw(format!(
                        "{} CSV   {} JSON",
                        marca(!estado.json),
                        marca(estado.json)
                    )),
                ]),
                Line::from(""),
                Line::from(ruta),
                Line::from(""),
                Line::from(Span::styled(
                    "  el fichero se escribe con permisos 600",
                    Style::default().fg(tema.paleta.inactivo),
                )),
            ];
            let pie = vec![teclas(
                &[
                    ("⇥", "cambiar campo"),
                    ("espacio", "alternar formato"),
                    ("↵", "exportar"),
                    ("esc", "cancelar"),
                ],
                tema,
            )];
            let foco = if estado.foco == 0 { 0 } else { 2 };
            mostrar(70, Hoja::nueva("EXPORTAR REGISTRO", cuerpo, pie).foco(foco));
        }
        Dialogo::GenerarClave { estado } => {
            // Las casillas y el aviso de la frase van en el pie fijo: `^s` los
            // aplica, así que se ven siempre aunque el cuerpo se desplace.
            let (cuerpo, mut pie, foco) = cuerpo_generacion(estado, tema);
            pie.push(teclas(
                &[
                    ("^s", "generar"),
                    ("⇥", "campo"),
                    ("espacio", "marcar"),
                    ("esc", "cancelar"),
                ],
                tema,
            ));
            mostrar(74, Hoja::nueva("GENERAR CLAVE", cuerpo, pie).foco(foco));
        }
        Dialogo::Tunel { estado } => {
            let (_, ancho_texto) = medidas(area, ANCHO_TUNEL);
            let (cuerpo, foco) = cuerpo_tunel(estado, tema, ancho_texto);
            // El aviso (escucha fuera de 127.0.0.1…) va en el pie fijo: `^s`
            // guarda con él, así que se ve siempre.
            let mut pie: Vec<Line<'static>> = estado
                .aviso()
                .map(|aviso| {
                    Line::from(Span::styled(
                        aviso.to_string(),
                        Style::default().fg(tema.paleta.acento),
                    ))
                })
                .into_iter()
                .collect();
            pie.push(teclas(
                &[
                    ("^s", "guardar"),
                    ("⇥", "campo"),
                    ("↵", "desplegable"),
                    ("esc", "cancelar"),
                ],
                tema,
            ));
            let titulo = if estado.id.is_some() {
                "EDITAR TÚNEL"
            } else {
                "TÚNEL NUEVO"
            };
            mostrar(ANCHO_TUNEL, Hoja::nueva(titulo, cuerpo, pie).foco(foco));
        }
        Dialogo::FraseImportacion { campo, .. } => {
            let cuerpo = vec![
                linea("La clave importada está cifrada."),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(
                        enmascarado(campo, tema),
                        Style::default().fg(tema.paleta.texto),
                    ),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
            ];
            let pie = vec![teclas(&[("↵", "aceptar"), ("esc", "cancelar")], tema)];
            mostrar(60, Hoja::nueva("FRASE DE LA CLAVE", cuerpo, pie).foco(2));
        }
        Dialogo::ConflictoImportacion { nombre, restantes } => {
            let cuerpo = vec![
                Line::from(vec![
                    span_texto("Ya existe un host con el nombre "),
                    Span::styled(
                        format!("«{nombre}»"),
                        Style::default()
                            .fg(tema.paleta.acento)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                linea("¿Sobrescribirlo con los datos importados?"),
                Line::from(Span::styled(
                    format!("Quedan {} conflicto(s) por decidir.", restantes),
                    Style::default().fg(tema.paleta.inactivo),
                )),
            ];
            let pie = vec![
                teclas(&[("s", "sobrescribir"), ("o", "omitir")], tema),
                teclas(
                    &[
                        ("S", "sobrescribir todos"),
                        ("O", "omitir todos"),
                        ("esc", "cancelar"),
                    ],
                    tema,
                ),
            ];
            mostrar(
                74,
                Hoja::nueva("CONFLICTO DE IMPORTACIÓN", cuerpo, pie).peligro(true),
            );
        }
    }
}

/// Cuerpo del diálogo GENERAR CLAVE y la línea del campo con foco.
/// Cuerpo del diálogo de generar clave (los campos), las líneas del pie que
/// `^s` aplica (casillas y aviso de la frase) y la línea con foco.
fn cuerpo_generacion(
    estado: &crate::app::EstadoGeneracion,
    tema: &Tema,
) -> (Vec<Line<'static>>, Vec<Line<'static>>, usize) {
    use crate::app::CampoGeneracion;
    let activo = |campo: CampoGeneracion| estado.campo == campo;
    let estilo = |campo: CampoGeneracion| {
        let base = Style::default().fg(tema.paleta.texto);
        if activo(campo) {
            base.fg(tema.paleta.acento).add_modifier(Modifier::BOLD)
        } else {
            base
        }
    };
    let etiqueta = |campo: CampoGeneracion, texto: &str| {
        Span::styled(
            format!("{texto:<12}"),
            Style::default().fg(if activo(campo) {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        )
    };
    let mut frase_visible = enmascarado(&estado.frase, tema);
    let mut repetir_visible = enmascarado(&estado.repetir, tema);
    if activo(CampoGeneracion::Frase) && !estado.frase.texto.is_empty() {
        frase_visible.push(CURSOR);
    }
    if activo(CampoGeneracion::Repetir) && !estado.repetir.texto.is_empty() {
        repetir_visible.push(CURSOR);
    }
    let corchete =
        |texto: &'static str| Span::styled(texto, Style::default().fg(tema.paleta.acento));
    let cuerpo = vec![
        Line::from(vec![
            etiqueta(CampoGeneracion::Fichero, "Fichero"),
            corchete("[ "),
            estado.fichero.span(
                activo(CampoGeneracion::Fichero),
                estilo(CampoGeneracion::Fichero),
            ),
            corchete(" ]"),
        ]),
        Line::from(vec![
            etiqueta(CampoGeneracion::Tipo, "Tipo"),
            Span::styled(
                format!(
                    "{} ed25519   {} rsa 4096",
                    if estado.rsa { "( )" } else { "(•)" },
                    if estado.rsa { "(•)" } else { "( )" }
                ),
                estilo(CampoGeneracion::Tipo),
            ),
        ]),
        Line::from(vec![
            etiqueta(CampoGeneracion::Comentario, "Comentario"),
            corchete("[ "),
            estado.comentario.span(
                activo(CampoGeneracion::Comentario),
                estilo(CampoGeneracion::Comentario),
            ),
            corchete(" ]"),
        ]),
        Line::from(vec![
            etiqueta(CampoGeneracion::Frase, "Frase"),
            corchete("[ "),
            Span::styled(frase_visible, estilo(CampoGeneracion::Frase)),
            corchete(" ]"),
        ]),
        Line::from(vec![
            etiqueta(CampoGeneracion::Repetir, "Repetir"),
            corchete("[ "),
            Span::styled(repetir_visible, estilo(CampoGeneracion::Repetir)),
            corchete(" ]"),
        ]),
    ];
    let pie = vec![
        Line::from(Span::styled(
            if estado.frase.texto.is_empty() {
                "sin frase: la clave quedará sin cifrar en ~/.ssh"
            } else {
                "la frase cifra la clave en formato OpenSSH"
            },
            Style::default().fg(if estado.frase.texto.is_empty() {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        )),
        Line::from(vec![
            Span::styled(
                format!(
                    "{} añadir al agente",
                    crate::ui::componentes::casilla(estado.agente)
                ),
                estilo(CampoGeneracion::Agente),
            ),
            Span::raw("      "),
            Span::styled(
                format!(
                    "{} copiar la pública",
                    crate::ui::componentes::casilla(estado.copiar)
                ),
                estilo(CampoGeneracion::Copiar),
            ),
        ]),
    ];
    // Las casillas están en el pie, siempre a la vista: con el foco en ellas
    // el cuerpo se queda en su último campo.
    let foco = match estado.campo {
        CampoGeneracion::Fichero => 0,
        CampoGeneracion::Tipo => 1,
        CampoGeneracion::Comentario => 2,
        CampoGeneracion::Frase => 3,
        CampoGeneracion::Repetir | CampoGeneracion::Agente | CampoGeneracion::Copiar => 4,
    };
    (cuerpo, pie, foco)
}

/// Ancho deseado del diálogo de túnel.
const ANCHO_TUNEL: u16 = 78;
/// Opciones del desplegable de hosts visibles a la vez en el diálogo de túnel.
const OPCIONES_TUNEL: usize = 6;
/// Ancho del valor del desplegable de host con sitio de sobra.
const RELLENO_HOST_TUNEL: usize = 34;

/// Cuerpo del diálogo de túnel y la línea con foco (la opción resaltada si
/// el desplegable está abierto).
fn cuerpo_tunel(
    estado: &crate::app::FormularioTunel,
    tema: &Tema,
    ancho_texto: usize,
) -> (Vec<Line<'static>>, usize) {
    use crate::app::CampoTunel;
    use crate::modelo::TipoTunel;
    let activo = |campo: CampoTunel| estado.foco == campo;
    let estilo = |campo: CampoTunel| {
        let base = Style::default().fg(tema.paleta.texto);
        if activo(campo) {
            base.fg(tema.paleta.acento).add_modifier(Modifier::BOLD)
        } else {
            base
        }
    };
    let etiqueta = |campo: CampoTunel, texto: &str| {
        Span::styled(
            format!("{texto:<11}"),
            Style::default().fg(if activo(campo) {
                tema.paleta.acento
            } else {
                tema.paleta.inactivo
            }),
        )
    };
    let corchete =
        |texto: &'static str| Span::styled(texto, Style::default().fg(tema.paleta.acento));
    let deshabilitado = !estado.tipo.lleva_destino();
    let estilo_destino = |campo: CampoTunel| {
        if deshabilitado {
            Style::default().fg(tema.paleta.inactivo)
        } else {
            estilo(campo)
        }
    };
    let mut cuerpo = Vec::new();
    let mut lineas_campo: Vec<(CampoTunel, usize)> = Vec::new();
    let mut foco_desplegable = None;

    // Host: desplegable, con su lista desplegada debajo si está abierto.
    let valor_host = if estado.host.opciones.is_empty() {
        "(no hay hosts)".to_string()
    } else {
        format!("{} \u{25be}", estado.host.etiqueta_seleccionada())
    };
    // El desplegable se rellena hasta 34 columnas, o hasta donde quepa con
    // la etiqueta y los corchetes para no dejar el `]` en otro renglón.
    let relleno = RELLENO_HOST_TUNEL.min(ancho_texto.saturating_sub(11 + 4));
    lineas_campo.push((CampoTunel::Host, cuerpo.len()));
    cuerpo.push(Line::from(vec![
        etiqueta(CampoTunel::Host, "Host"),
        corchete("[ "),
        Span::styled(
            format!("{valor_host:<relleno$}"),
            if estado.host.abierto {
                Style::default()
                    .bg(tema.paleta.acento)
                    .fg(tema.paleta.fondo)
            } else {
                estilo(CampoTunel::Host)
            },
        ),
        corchete(" ]"),
    ]));
    if estado.host.abierto {
        let filtradas = estado.host.filtradas();
        // La ventana de opciones sigue a la resaltada.
        let inicio =
            disposicion::ventana(0, estado.host.resaltado, OPCIONES_TUNEL, filtradas.len());
        for (posicion, indice) in filtradas
            .iter()
            .enumerate()
            .skip(inicio)
            .take(OPCIONES_TUNEL)
        {
            let opcion = &estado.host.opciones[*indice];
            let resaltada = posicion == estado.host.resaltado;
            if resaltada {
                foco_desplegable = Some(cuerpo.len());
            }
            cuerpo.push(Line::from(Span::styled(
                format!(
                    "  {} {}",
                    if resaltada {
                        tema.glifos.seleccion
                    } else {
                        " "
                    },
                    opcion.etiqueta
                ),
                if resaltada {
                    Style::default()
                        .bg(tema.paleta.acento)
                        .fg(tema.paleta.fondo)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(tema.paleta.texto)
                },
            )));
        }
        let debajo = filtradas.len().saturating_sub(inicio + OPCIONES_TUNEL);
        if debajo > 0 {
            cuerpo.push(Line::from(Span::styled(
                format!("    … {debajo} más"),
                Style::default().fg(tema.paleta.inactivo),
            )));
        }
        cuerpo.push(Line::from(Span::styled(
            format!("  filtro: {}", estado.host.filtro.texto),
            Style::default().fg(tema.paleta.inactivo),
        )));
    }

    lineas_campo.push((CampoTunel::Nombre, cuerpo.len()));
    cuerpo.push(Line::from(vec![
        etiqueta(CampoTunel::Nombre, "Nombre"),
        corchete("[ "),
        estado
            .nombre
            .span(activo(CampoTunel::Nombre), estilo(CampoTunel::Nombre)),
        corchete(" ]"),
    ]));
    let marca = |elegido: bool| if elegido { "(•)" } else { "( )" };
    lineas_campo.push((CampoTunel::Tipo, cuerpo.len()));
    cuerpo.push(Line::from(vec![
        etiqueta(CampoTunel::Tipo, "Tipo"),
        Span::styled(
            format!(
                "{} local   {} remoto   {} dinámico",
                marca(estado.tipo == TipoTunel::Local),
                marca(estado.tipo == TipoTunel::Remoto),
                marca(estado.tipo == TipoTunel::Dinamico),
            ),
            estilo(CampoTunel::Tipo),
        ),
    ]));
    lineas_campo.push((CampoTunel::EscuchaDireccion, cuerpo.len()));
    lineas_campo.push((CampoTunel::EscuchaPuerto, cuerpo.len()));
    cuerpo.push(Line::from(vec![
        etiqueta(CampoTunel::EscuchaDireccion, "Escucha"),
        corchete("[ "),
        estado.escucha_direccion.span(
            activo(CampoTunel::EscuchaDireccion),
            estilo(CampoTunel::EscuchaDireccion),
        ),
        corchete(" ] : [ "),
        estado.escucha_puerto.span(
            activo(CampoTunel::EscuchaPuerto),
            estilo(CampoTunel::EscuchaPuerto),
        ),
        corchete(" ]"),
        Span::styled(
            "   puerto 0: el que asigne el sistema",
            Style::default().fg(tema.paleta.inactivo),
        ),
    ]));
    let mut spans_destino = vec![
        etiqueta(CampoTunel::DestinoDireccion, "Destino"),
        corchete("[ "),
        estado.destino_direccion.span(
            activo(CampoTunel::DestinoDireccion) && !deshabilitado,
            estilo_destino(CampoTunel::DestinoDireccion),
        ),
        corchete(" ] : [ "),
        estado.destino_puerto.span(
            activo(CampoTunel::DestinoPuerto) && !deshabilitado,
            estilo_destino(CampoTunel::DestinoPuerto),
        ),
        corchete(" ]"),
    ];
    if deshabilitado {
        spans_destino.push(Span::styled(
            "   no se usa en dinámico",
            Style::default().fg(tema.paleta.inactivo),
        ));
    }
    lineas_campo.push((CampoTunel::DestinoDireccion, cuerpo.len()));
    lineas_campo.push((CampoTunel::DestinoPuerto, cuerpo.len()));
    cuerpo.push(Line::from(spans_destino));
    lineas_campo.push((CampoTunel::Automatico, cuerpo.len()));
    cuerpo.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!(
                "{} automático: se levanta con la primera sesión",
                crate::ui::componentes::casilla(estado.automatico)
            ),
            estilo(CampoTunel::Automatico),
        ),
    ]));
    let foco = foco_desplegable.unwrap_or_else(|| {
        lineas_campo
            .iter()
            .find(|(campo, _)| *campo == estado.foco)
            .map(|(_, linea)| *linea)
            .unwrap_or(0)
    });
    (cuerpo, foco)
}

// ---------------------------------------------------------------- hoja

/// Diálogo que encoge al área (Fase 7). El cuerpo se parte en renglones del
/// ancho disponible y se desplaza si no cabe; el pie (teclas, y el campo en
/// las huellas cambiadas) queda fijo abajo. Con sitio lleva márgenes de 2×1
/// y una línea en blanco antes del pie; sin sitio los pierde antes de
/// desplazar. La ventana visible se registra en la `Disposicion`.
pub(crate) struct Hoja {
    titulo: String,
    cuerpo: Vec<Line<'static>>,
    pie: Vec<Line<'static>>,
    peligro: bool,
    /// Línea del cuerpo que debe quedar a la vista (formularios). Sin foco,
    /// el cuerpo se desplaza con las teclas desde `ancla`.
    foco: Option<usize>,
    /// Alto mínimo del recuadro (evita que el diálogo salte al filtrar).
    alto_minimo: u16,
    /// Lista con la que se registra la ventana y su desplazamiento anterior.
    lista: Lista,
    ancla: usize,
}

impl Hoja {
    pub(crate) fn nueva(
        titulo: impl Into<String>,
        cuerpo: Vec<Line<'static>>,
        pie: Vec<Line<'static>>,
    ) -> Self {
        Self {
            titulo: titulo.into(),
            cuerpo,
            pie,
            peligro: false,
            foco: None,
            alto_minimo: 0,
            lista: Lista::Modal,
            ancla: 0,
        }
    }

    pub(crate) fn peligro(mut self, peligro: bool) -> Self {
        self.peligro = peligro;
        self
    }

    pub(crate) fn foco(mut self, linea: usize) -> Self {
        self.foco = Some(linea);
        self
    }

    pub(crate) fn alto_minimo(mut self, alto: u16) -> Self {
        self.alto_minimo = alto;
        self
    }

    /// Registra la ventana como `lista` (por defecto `Lista::Modal`).
    pub(crate) fn lista(mut self, lista: Lista) -> Self {
        self.lista = lista;
        self
    }

    /// Primera línea visible del pintado anterior.
    pub(crate) fn ancla(mut self, ancla: usize) -> Self {
        self.ancla = ancla;
        self
    }

    /// Pinta la hoja centrada en `area` con `ancho` como ancho deseado.
    pub(crate) fn pintar(
        self,
        marco: &mut Frame,
        area: Rect,
        ancho: u16,
        tema: &Tema,
        disp: &mut Disposicion,
    ) {
        let (margen, ancho_texto) = medidas(area, ancho);
        let (mut cuerpo, mut rangos) = partir(self.cuerpo, ancho_texto, tema.ascii);
        let (pie, _) = partir(self.pie, ancho_texto, tema.ascii);
        let separador = usize::from(!cuerpo.is_empty() && !pie.is_empty());
        let mut necesario = cuerpo.len() + separador + pie.len();
        // Bordes (2) y márgenes verticales (2).
        let ideal = u16::try_from(necesario + 4).unwrap_or(u16::MAX);
        let recta = centrar(area, ancho, ideal.max(self.alto_minimo));
        let alto_interior = usize::from(recta.height.saturating_sub(2));
        if necesario > alto_interior {
            // Sin sitio, las líneas en blanco del cuerpo se van antes de
            // desplazar.
            (cuerpo, rangos) = sin_blancos(cuerpo, rangos);
            necesario = cuerpo.len() + separador + pie.len();
        }
        // Primero se pierden los márgenes (el de abajo antes que el de
        // arriba) y, si hay que desplazar y el cuerpo se queda con menos de
        // cuatro filas, la línea en blanco antes del pie.
        let (arriba, abajo, separador) = if alto_interior >= necesario + 2 {
            (1, 1, separador)
        } else if alto_interior >= necesario {
            (alto_interior - necesario, 0, separador)
        } else if alto_interior >= pie.len() + separador + FILAS_CON_SEPARADOR {
            (0, 0, separador)
        } else {
            (0, 0, 0)
        };
        let util = alto_interior.saturating_sub(arriba + abajo + separador);
        let pie_visible = pie.len().min(util.saturating_sub(1));
        let filas = util - pie_visible;
        let total = cuerpo.len();
        let inicio = match self.foco.and_then(|linea| rangos.get(linea)) {
            Some(&(primera, ultima)) => disposicion::ventana(
                disposicion::ventana(self.ancla, ultima, filas, total),
                primera,
                filas,
                total,
            ),
            None => disposicion::ventana(self.ancla, self.ancla, filas, total),
        };
        disp.registrar(
            self.lista,
            VentanaLista {
                inicio,
                filas,
                total,
            },
        );

        let color = if self.peligro {
            tema.paleta.critico
        } else {
            tema.paleta.acento
        };
        let estilo_titulo = Style::default().fg(color).add_modifier(Modifier::BOLD);
        let titulo = disposicion::recortar(
            &texto_de(&self.titulo, tema.ascii),
            usize::from(recta.width.saturating_sub(4)),
            tema.ascii,
        );
        let mut bloque = Block::default()
            .borders(Borders::ALL)
            .border_set(tema.bordes())
            .border_style(Style::default().fg(color))
            .title(Span::styled(format!(" {titulo} "), estilo_titulo));
        if total > filas && filas > 0 {
            bloque = bloque.title_bottom(
                Line::from(Span::styled(
                    // En los formularios el cuerpo sigue al foco y ↑↓ no
                    // desplazan: solo se dice que hay más.
                    if self.foco.is_some() {
                        posicion(inicio, filas, total)
                    } else {
                        indicador(inicio, filas, total, tema.ascii)
                    },
                    Style::default().fg(color),
                ))
                .right_aligned(),
            );
        }
        let interior = bloque.inner(recta);
        marco.render_widget(Clear, recta);
        marco.render_widget(bloque, recta);
        if ancho_texto == 0 || util == 0 {
            return;
        }
        let x = interior.x + margen;
        let y = interior.y + arriba as u16;
        let visibles: Vec<Line<'static>> = cuerpo.into_iter().skip(inicio).take(filas).collect();
        marco.render_widget(
            Paragraph::new(visibles),
            Rect::new(x, y, ancho_texto as u16, filas as u16),
        );
        if pie_visible > 0 {
            // El pie va pegado abajo (con alto mínimo, el hueco queda encima).
            let y_pie = interior.y + (alto_interior - abajo - pie_visible) as u16;
            let pie: Vec<Line<'static>> = pie.into_iter().take(pie_visible).collect();
            marco.render_widget(
                Paragraph::new(pie),
                Rect::new(x, y_pie, ancho_texto as u16, pie_visible as u16),
            );
        }
    }
}

/// `↑↓ i/n` (`^v i/n` en ASCII): posición del desplazamiento entre las
/// posibles, de 1 (arriba del todo) a n (abajo del todo).
pub(crate) fn indicador(inicio: usize, filas: usize, total: usize, ascii: bool) -> String {
    let flechas = if ascii { "^v" } else { "↑↓" };
    let posiciones = total.saturating_sub(filas) + 1;
    format!(" {flechas} {}/{posiciones} ", inicio + 1)
}

/// `i/n` sin flechas: la posición en un cuerpo que sigue al foco.
pub(crate) fn posicion(inicio: usize, filas: usize, total: usize) -> String {
    let posiciones = total.saturating_sub(filas) + 1;
    format!(" {}/{posiciones} ", inicio + 1)
}

/// Margen horizontal y ancho del texto de una hoja de `ancho` deseado en
/// `area`: margen de 2 con sitio, de 1 en diálogos estrechos y ninguno en
/// los diminutos.
pub(crate) fn medidas(area: Rect, ancho: u16) -> (u16, usize) {
    let ancho_recta = centrar(area, ancho, 3).width;
    let margen = if ancho_recta >= 40 {
        2
    } else if ancho_recta >= 16 {
        1
    } else {
        0
    };
    (
        margen,
        usize::from(ancho_recta.saturating_sub(2 + 2 * margen)),
    )
}

/// Parte las líneas en renglones de `ancho` (pasadas a ASCII si toca) y
/// devuelve también el primer y el último renglón de cada línea.
fn partir(
    lineas: Vec<Line<'static>>,
    ancho: usize,
    ascii: bool,
) -> (Vec<Line<'static>>, Vec<(usize, usize)>) {
    let mut renglones = Vec::new();
    let mut rangos = Vec::with_capacity(lineas.len());
    for linea in lineas {
        let linea = if ascii { linea_ascii(linea) } else { linea };
        let primera = renglones.len();
        let sangria = sangria_de(&linea);
        renglones.extend(envolver(&linea, ancho, sangria));
        rangos.push((primera, renglones.len().saturating_sub(1).max(primera)));
    }
    (renglones, rangos)
}

/// Quita los renglones en blanco y recoloca los rangos de cada línea (una
/// línea en blanco pasa a apuntar al renglón siguiente).
fn sin_blancos(
    renglones: Vec<Line<'static>>,
    rangos: Vec<(usize, usize)>,
) -> (Vec<Line<'static>>, Vec<(usize, usize)>) {
    let mut nuevo_indice = Vec::with_capacity(renglones.len());
    let mut compactos = Vec::with_capacity(renglones.len());
    for renglon in renglones {
        nuevo_indice.push(compactos.len());
        let en_blanco = renglon
            .spans
            .iter()
            .all(|span| span.content.trim().is_empty());
        if !en_blanco {
            compactos.push(renglon);
        }
    }
    let ultimo = compactos.len().saturating_sub(1);
    let rangos = rangos
        .into_iter()
        .map(|(primera, ultima)| {
            let recolocar =
                |indice: usize| nuevo_indice.get(indice).copied().unwrap_or(0).min(ultimo);
            (recolocar(primera), recolocar(ultima))
        })
        .collect();
    (compactos, rangos)
}

/// Sangría de los renglones de continuación de una línea: la de sus espacios
/// iniciales o, en las líneas de etiqueta y valor (`Huella: SHA256…`, con la
/// etiqueta corta en su propio tramo), el ancho de la etiqueta, para que el
/// valor partido siga debajo de sí mismo.
fn sangria_de(linea: &Line<'static>) -> usize {
    let iniciales = linea
        .spans
        .iter()
        .flat_map(|span| span.content.chars())
        .take_while(|caracter| *caracter == ' ')
        .count();
    if iniciales > 0 {
        return iniciales;
    }
    match linea.spans.first() {
        Some(etiqueta)
            if linea.spans.len() > 1
                && etiqueta.content.ends_with(' ')
                && etiqueta.content.chars().count() <= ETIQUETA_MAXIMA =>
        {
            etiqueta.content.chars().count()
        }
        _ => 0,
    }
}

/// Parte una línea con estilos en renglones de como mucho `ancho`
/// caracteres. Corta preferentemente entre atajos (dos o más espacios),
/// luego entre palabras y, si no hay espacio, a la fuerza. Los renglones de
/// continuación van sangrados `sangria` columnas (si caben).
pub(crate) fn envolver(linea: &Line<'static>, ancho: usize, sangria: usize) -> Vec<Line<'static>> {
    let celdas: Vec<(char, Style)> = linea
        .spans
        .iter()
        .flat_map(|span| {
            span.content
                .chars()
                .map(move |caracter| (caracter, span.style))
        })
        .collect();
    if ancho == 0 || celdas.len() <= ancho {
        return vec![linea.clone()];
    }
    let sangria = if sangria * 2 <= ancho { sangria } else { 0 };
    let mut renglones = Vec::new();
    let mut posicion = 0;
    let mut primero = true;
    while posicion < celdas.len() {
        let prefijo = if primero { 0 } else { sangria };
        let cabe = ancho - prefijo;
        let fin = if celdas.len() - posicion <= cabe {
            celdas.len()
        } else {
            let limite = posicion + cabe;
            // No se corta dentro de la sangría de la primera línea.
            let desde = posicion + if primero { sangria } else { 0 } + 1;
            let espacio = |indice: &usize| celdas[*indice].0 == ' ';
            if let Some(doble) = (desde..=limite)
                .rev()
                .find(|indice| espacio(indice) && celdas[*indice - 1].0 == ' ')
            {
                // Corte entre atajos: el renglón acaba antes de los espacios.
                (desde..doble)
                    .rev()
                    .find(|indice| !espacio(indice))
                    .map_or(doble, |indice| indice + 1)
            } else if espacio(&limite) {
                limite
            } else if let Some(simple) = (desde..limite).rev().find(espacio) {
                simple
            } else {
                limite
            }
        };
        let mut spans: Vec<Span<'static>> = Vec::new();
        if prefijo > 0 {
            spans.push(Span::raw(" ".repeat(prefijo)));
        }
        let mut trozo = String::new();
        let mut estilo_trozo = None;
        let fin_visible = (posicion..fin)
            .rev()
            .find(|indice| celdas[*indice].0 != ' ')
            .map_or(posicion, |indice| indice + 1);
        for (caracter, estilo) in &celdas[posicion..fin_visible] {
            if estilo_trozo.is_some_and(|actual| actual != *estilo) {
                spans.push(Span::styled(
                    std::mem::take(&mut trozo),
                    estilo_trozo.unwrap_or_default(),
                ));
            }
            estilo_trozo = Some(*estilo);
            trozo.push(*caracter);
        }
        if !trozo.is_empty() {
            spans.push(Span::styled(trozo, estilo_trozo.unwrap_or_default()));
        }
        let mut renglon = Line::from(spans).style(linea.style);
        renglon.alignment = linea.alignment;
        renglones.push(renglon);
        posicion = fin;
        while posicion < celdas.len() && celdas[posicion].0 == ' ' {
            posicion += 1;
        }
        primero = false;
    }
    renglones
}

/// La misma línea con el texto pasado a ASCII.
pub(crate) fn linea_ascii(linea: Line<'static>) -> Line<'static> {
    let spans = linea
        .spans
        .into_iter()
        .map(|span| Span::styled(disposicion::texto_ascii(&span.content), span.style))
        .collect::<Vec<_>>();
    let mut nueva = Line::from(spans).style(linea.style);
    nueva.alignment = linea.alignment;
    nueva
}

/// El texto tal cual o, en modo ASCII, sin glifos Unicode.
pub(crate) fn texto_de(texto: &str, ascii: bool) -> String {
    disposicion::adaptar(texto, ascii)
}

/// Modal de tamaño fijo que usan otras vistas (EJECUTAR, formulario de
/// snippet, deliberación): borde y margen de 2×1; ellas calculan su propio
/// contenido para el hueco que deja.
pub(crate) fn modal(
    marco: &mut Frame,
    recta: Rect,
    titulo: &str,
    contenido: Vec<Line<'static>>,
    peligro: bool,
    tema: &Tema,
) {
    let color: Color = if peligro {
        tema.paleta.critico
    } else {
        tema.paleta.acento
    };
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {} ", texto_de(titulo, tema.ascii)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    let interior = bloque.inner(recta);
    marco.render_widget(Clear, recta);
    marco.render_widget(bloque, recta);
    let area_texto = Rect {
        x: interior.x + 2,
        y: interior.y + 1,
        width: interior.width.saturating_sub(4),
        height: interior.height.saturating_sub(2),
    };
    // Lo que traen hecho EJECUTAR, el formulario y la deliberación (errores
    // con «», la pregunta con ¿, datos de los hosts) se degrada como en Hoja.
    let contenido = if tema.ascii {
        contenido.into_iter().map(linea_ascii).collect()
    } else {
        contenido
    };
    marco.render_widget(Paragraph::new(contenido), area_texto);
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn texto(renglones: &[Line<'static>]) -> Vec<String> {
        renglones
            .iter()
            .map(|renglon| {
                renglon
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn envolver_corta_entre_palabras_y_a_la_fuerza() {
        let linea = Line::from("La huella no está en known_hosts. Comprueba");
        assert_eq!(
            texto(&envolver(&linea, 20, 0)),
            vec!["La huella no está en", "known_hosts.", "Comprueba"]
        );
        let largo = Line::from("SHA256:abcdefghijklmnopqrstuvwxyz");
        assert_eq!(
            texto(&envolver(&largo, 10, 0)),
            vec!["SHA256:abc", "defghijklm", "nopqrstuvw", "xyz"]
        );
    }

    #[test]
    fn envolver_prefiere_cortar_entre_atajos_y_conserva_estilos() {
        let tema = Tema::respaldo();
        let linea = teclas(&[("s", "confirmar"), ("n / esc", "cancelar")], &tema);
        let renglones = envolver(&linea, 20, 0);
        assert_eq!(texto(&renglones), vec!["s confirmar", "n / esc cancelar"]);
        // La tecla sigue en su estilo de atajo en el segundo renglón.
        assert_eq!(renglones[1].spans[0].content, "n / esc");
        assert_eq!(renglones[1].spans[0].style.fg, Some(tema.paleta.acento));
    }

    #[test]
    fn envolver_sangra_las_continuaciones() {
        let linea = Line::from("  a b c d e f g h i j");
        assert_eq!(
            texto(&envolver(&linea, 8, 2)),
            vec!["  a b c", "  d e f", "  g h i", "  j"]
        );
    }

    #[test]
    fn en_ascii_deja_letras_y_cambia_glifos() {
        assert_eq!(
            disposicion::texto_ascii("«hetzner-01» · intento 1 de 3 → ↵ ⇥ ┃ ✕ …"),
            "\"hetzner-01\" - intento 1 de 3 -> enter tab | x ~"
        );
        assert_eq!(
            disposicion::texto_ascii("Contraseña TÚNEL"),
            "Contraseña TÚNEL"
        );
        assert_eq!(
            disposicion::texto_ascii("¿Borrar? ¡Hecho!"),
            "Borrar? Hecho!"
        );
        assert!(disposicion::texto_ascii("☃").is_ascii());
    }

    #[test]
    fn sin_blancos_recoloca_los_rangos() {
        let (renglones, rangos) = partir(
            vec![
                Line::from("uno"),
                Line::from(""),
                Line::from("dos tres"),
                Line::from("  "),
            ],
            4,
            false,
        );
        assert_eq!(renglones.len(), 5);
        let (renglones, rangos) = sin_blancos(renglones, rangos);
        assert_eq!(texto(&renglones), vec!["uno", "dos", "tres"]);
        assert_eq!(rangos, vec![(0, 0), (1, 1), (1, 2), (2, 2)]);
    }

    #[test]
    fn las_lineas_de_etiqueta_siguen_bajo_su_valor() {
        let linea = Line::from(vec![Span::raw("Huella: "), Span::raw("SHA256:abcdefghij")]);
        assert_eq!(sangria_de(&linea), 8);
        let (renglones, _) = partir(vec![linea], 16, false);
        assert_eq!(
            texto(&renglones),
            vec!["Huella: SHA256:a", "        bcdefghi", "        j"]
        );
    }

    #[test]
    fn el_indicador_va_de_1_a_n() {
        assert_eq!(indicador(0, 5, 12, false), " ↑↓ 1/8 ");
        assert_eq!(indicador(7, 5, 12, true), " ^v 8/8 ");
    }
}
