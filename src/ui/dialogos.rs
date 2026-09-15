use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, Dialogo};
use crate::tema::Tema;
use crate::ui::centrar;

fn linea(contenido: &str) -> Line<'static> {
    Line::from(Span::raw(contenido.to_string()))
}

fn span_texto(contenido: &str) -> Span<'static> {
    Span::raw(contenido.to_string())
}

fn atajo(tecla: &str, tema: &Tema) -> Span<'static> {
    Span::styled(
        tecla.to_string(),
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, dialogo: &Dialogo) {
    let tema = &app.tema;
    match dialogo {
        Dialogo::Confirmar {
            titulo,
            lineas,
            peligro,
            ..
        } => {
            let alto = lineas.len() as u16 + 4;
            let recta = centrar(area, 66, alto);
            let mut contenido: Vec<Line> = lineas.iter().map(|texto| linea(texto)).collect();
            contenido.push(Line::from(""));
            contenido.push(Line::from(vec![
                atajo("s", tema),
                span_texto(" confirmar   "),
                atajo("n / esc", tema),
                span_texto(" cancelar"),
            ]));
            modal(marco, recta, titulo, contenido, *peligro, tema);
        }
        Dialogo::ServidorCaido { perdidas } => {
            let recta = centrar(area, 70, 10);
            let mut contenido = vec![Line::from(Span::styled(
                "✕ El servidor de sesiones ha dejado de responder",
                Style::default()
                    .fg(tema.paleta.critico)
                    .add_modifier(Modifier::BOLD),
            ))];
            if *perdidas > 0 {
                contenido.push(linea(&format!("  {perdidas} sesión(es) se han perdido.")));
            }
            contenido.push(Line::from(""));
            contenido.push(linea("  Detalle en el registro y en logs/servidor.log."));
            contenido.push(Line::from(""));
            contenido.push(Line::from(vec![
                atajo("s", tema),
                span_texto(" relanzar servidor   "),
                atajo("n", tema),
                span_texto(" seguir sin sesiones"),
            ]));
            modal(marco, recta, "SERVIDOR CAÍDO", contenido, true, tema);
        }
        Dialogo::HuellaServidor {
            host, tipo, huella, ..
        } => {
            let recta = centrar(area, 66, 12);
            let contenido = vec![
                Line::from(vec![
                    span_texto("Host: "),
                    Span::styled(host.clone(), Style::default().fg(tema.paleta.texto)),
                ]),
                Line::from(vec![
                    span_texto("Tipo:  "),
                    Span::styled(tipo.clone(), Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(vec![
                    span_texto("Huella: "),
                    Span::styled(huella.clone(), Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(""),
                linea("La huella no está en known_hosts. Comprueba que es la correcta."),
                Line::from(""),
                Line::from(vec![
                    atajo("a", tema),
                    span_texto(" aceptar y añadir   "),
                    atajo("x", tema),
                    span_texto(" cancelar la apertura"),
                ]),
            ];
            modal(marco, recta, "HUELLA DESCONOCIDA", contenido, false, tema);
        }
        Dialogo::HuellaCambiadaServidor {
            host,
            anterior,
            nueva,
            campo,
            ..
        } => {
            let recta = centrar(area, 74, 15);
            let contenido = vec![
                Line::from(Span::styled(
                    "✕ La clave del servidor NO coincide con known_hosts",
                    Style::default()
                        .fg(tema.paleta.critico)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    span_texto("Anterior  "),
                    Span::styled(anterior.clone(), Style::default().fg(tema.paleta.inactivo)),
                ]),
                Line::from(vec![
                    span_texto("Nueva     "),
                    Span::styled(nueva.clone(), Style::default().fg(tema.paleta.critico)),
                ]),
                Line::from(""),
                linea("Puede ser una reinstalación… o un intermediario."),
                linea(&format!(
                    "Para sustituir la huella escribe el nombre del host ({host}):"
                )),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.critico)),
                    campo.span(true, Style::default().fg(tema.paleta.critico)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.critico)),
                ]),
                Line::from(""),
                Line::from(vec![
                    atajo("r", tema),
                    span_texto(" sustituir   "),
                    atajo("esc", tema),
                    span_texto(" cancelar (recomendado)"),
                ]),
            ];
            modal(marco, recta, "HUELLA CAMBIADA", contenido, true, tema);
        }
        Dialogo::FraseServidor {
            host,
            intento,
            campo,
            ..
        } => {
            let recta = centrar(area, 60, 9);
            let enmascarado: String = campo
                .texto
                .chars()
                .map(|_| if tema.ascii { '*' } else { '•' })
                .collect();
            let visible = if campo.texto.is_empty() {
                "\u{2503}".to_string()
            } else {
                enmascarado
            };
            let contenido = vec![
                linea(&format!(
                    "Clave con frase para «{host}» · intento {intento} de 3"
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(visible, Style::default().fg(tema.paleta.texto)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(""),
                Line::from(vec![
                    atajo("↵", tema),
                    span_texto(" aceptar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar la apertura"),
                ]),
            ];
            modal(marco, recta, "FRASE DE LA CLAVE", contenido, false, tema);
        }
        Dialogo::ContrasenaServidor {
            host,
            intento,
            campo,
            recordar,
            foco_casilla,
            ..
        } => {
            let recta = centrar(area, 64, 11);
            let mascara = |_: char| if tema.ascii { '*' } else { '•' };
            let mut visible = String::new();
            if !*foco_casilla {
                let mut cursor_puesto = false;
                for (indice, caracter) in campo.texto.chars().enumerate() {
                    if indice == campo.cursor {
                        visible.push('\u{2503}');
                        cursor_puesto = true;
                    }
                    visible.push(mascara(caracter));
                }
                if !cursor_puesto {
                    visible.push('\u{2503}');
                }
            } else if campo.texto.is_empty() {
                visible.push('\u{2503}');
            } else {
                visible = campo.texto.chars().map(mascara).collect();
            }
            let estilo_casilla = if *foco_casilla {
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD)
            } else if *recordar {
                Style::default().fg(tema.paleta.correcto)
            } else {
                Style::default().fg(tema.paleta.texto)
            };
            let contenido = vec![
                linea(&format!(
                    "Contraseña para «{host}» · intento {intento} de 3"
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(
                        visible,
                        if *foco_casilla {
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
                            crate::ui::componentes::casilla(*recordar)
                        ),
                        estilo_casilla,
                    ),
                ]),
                Line::from(Span::styled(
                    if *recordar {
                        "  la contraseña queda cifrada en el llavero, nunca en magi.db"
                    } else {
                        "  sin marcar: se pedirá en cada conexión y no se guarda"
                    },
                    Style::default().fg(tema.paleta.inactivo),
                )),
                Line::from(vec![
                    atajo("↵", tema),
                    span_texto(" aceptar   "),
                    atajo("⇥", tema),
                    span_texto(" campo/casilla   "),
                    atajo("espacio", tema),
                    span_texto(" marcar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, "CONTRASEÑA", contenido, false, tema);
        }
        Dialogo::HuellaDesconocida {
            host, tipo, huella, ..
        } => {
            let recta = centrar(area, 66, 12);
            let contenido = vec![
                Line::from(vec![
                    span_texto("Host: "),
                    Span::styled(host.clone(), Style::default().fg(tema.paleta.texto)),
                ]),
                Line::from(vec![
                    span_texto("Tipo:  "),
                    Span::styled(tipo.clone(), Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(vec![
                    span_texto("Huella: "),
                    Span::styled(huella.clone(), Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(""),
                linea("La huella no está en known_hosts. Comprueba que es la correcta."),
                Line::from(""),
                Line::from(vec![
                    atajo("a", tema),
                    span_texto(" aceptar y añadir   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, "HUELLA DESCONOCIDA", contenido, false, tema);
        }
        Dialogo::HuellaCambiada {
            host,
            anterior,
            nueva,
            campo,
            ..
        } => {
            let recta = centrar(area, 74, 15);
            let contenido = vec![
                Line::from(Span::styled(
                    "✕ La clave del servidor NO coincide con known_hosts",
                    Style::default()
                        .fg(tema.paleta.critico)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    span_texto("Anterior  "),
                    Span::styled(anterior.clone(), Style::default().fg(tema.paleta.inactivo)),
                ]),
                Line::from(vec![
                    span_texto("Nueva     "),
                    Span::styled(nueva.clone(), Style::default().fg(tema.paleta.critico)),
                ]),
                Line::from(""),
                linea("Puede ser una reinstalación… o un intermediario."),
                linea(&format!(
                    "Para sustituir la huella escribe el nombre del host ({host}):"
                )),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.critico)),
                    campo.span(true, Style::default().fg(tema.paleta.critico)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.critico)),
                ]),
                Line::from(""),
                Line::from(vec![
                    atajo("r", tema),
                    span_texto(" sustituir   "),
                    atajo("esc", tema),
                    span_texto(" cancelar (recomendado)"),
                ]),
            ];
            modal(marco, recta, "HUELLA CAMBIADA", contenido, true, tema);
        }
        Dialogo::Frase {
            host,
            intento,
            campo,
            ..
        } => {
            let recta = centrar(area, 60, 9);
            let enmascarado: String = campo
                .texto
                .chars()
                .map(|_| if tema.ascii { '*' } else { '•' })
                .collect();
            let visible = if campo.texto.is_empty() {
                "\u{2503}".to_string()
            } else {
                enmascarado
            };
            let contenido = vec![
                linea(&format!(
                    "Clave con frase para «{host}» · intento {intento} de 3"
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(visible, Style::default().fg(tema.paleta.texto)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(""),
                Line::from(vec![
                    atajo("↵", tema),
                    span_texto(" aceptar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, "FRASE DE LA CLAVE", contenido, false, tema);
        }
        Dialogo::Contrasena {
            host,
            intento,
            campo,
            recordar,
            foco_casilla,
            ..
        } => {
            let recta = centrar(area, 64, 11);
            let mascara = |_: char| if tema.ascii { '*' } else { '•' };
            let mut visible = String::new();
            if !*foco_casilla {
                let mut cursor_puesto = false;
                for (indice, caracter) in campo.texto.chars().enumerate() {
                    if indice == campo.cursor {
                        visible.push('\u{2503}');
                        cursor_puesto = true;
                    }
                    visible.push(mascara(caracter));
                }
                if !cursor_puesto {
                    visible.push('\u{2503}');
                }
            } else if campo.texto.is_empty() {
                visible.push('\u{2503}');
            } else {
                visible = campo.texto.chars().map(mascara).collect();
            }
            let estilo_casilla = if *foco_casilla {
                Style::default()
                    .fg(tema.paleta.acento)
                    .add_modifier(Modifier::BOLD)
            } else if *recordar {
                Style::default().fg(tema.paleta.correcto)
            } else {
                Style::default().fg(tema.paleta.texto)
            };
            let contenido = vec![
                linea(&format!(
                    "Contraseña para «{host}» · intento {intento} de 3"
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(
                        visible,
                        if *foco_casilla {
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
                            crate::ui::componentes::casilla(*recordar)
                        ),
                        estilo_casilla,
                    ),
                ]),
                Line::from(Span::styled(
                    if *recordar {
                        "  la contraseña queda cifrada en el llavero, nunca en magi.db"
                    } else {
                        "  sin marcar: se pedirá en cada conexión y no se guarda"
                    },
                    Style::default().fg(tema.paleta.inactivo),
                )),
                Line::from(vec![
                    atajo("↵", tema),
                    span_texto(" aceptar   "),
                    atajo("⇥", tema),
                    span_texto(" campo/casilla   "),
                    atajo("espacio", tema),
                    span_texto(" marcar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, "CONTRASEÑA", contenido, false, tema);
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
            let alto = opciones.len() as u16 + 3;
            let recta = centrar(area, 46, alto);
            let mut contenido = Vec::new();
            for (indice, opcion) in opciones.iter().enumerate() {
                let elegida = indice == *seleccion;
                let estilo = if elegida {
                    Style::default()
                        .bg(tema.paleta.acento)
                        .fg(tema.paleta.fondo)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(tema.paleta.texto)
                };
                contenido.push(Line::from(Span::styled(
                    format!(" {}  {opcion}", if elegida { "▸" } else { " " }),
                    estilo,
                )));
            }
            contenido.push(Line::from(""));
            contenido.push(Line::from(vec![
                atajo("↵", tema),
                span_texto(" elegir   "),
                atajo("esc", tema),
                span_texto(" cerrar"),
            ]));
            modal(marco, recta, "MENÚ DE GRUPO", contenido, false, tema);
        }
        Dialogo::EntradaTexto {
            titulo,
            etiqueta,
            campo,
            ..
        } => {
            let recta = centrar(area, 56, 7);
            let contenido = vec![
                Line::from(vec![
                    span_texto(&format!("{etiqueta}: ")),
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    campo.span(true, Style::default().fg(tema.paleta.texto)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(""),
                Line::from(vec![
                    atajo("↵", tema),
                    span_texto(" aceptar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, titulo, contenido, false, tema);
        }
        Dialogo::MoverHost {
            host_nombre,
            desplegable,
            ..
        } => {
            let recta = centrar(area, 52, 14);
            let bloque = super::bloque("MOVER HOST A GRUPO", tema);
            let interior = bloque.inner(recta);
            marco.render_widget(Clear, recta);
            marco.render_widget(bloque, recta);
            let trozos = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(2),
                ])
                .split(interior);
            marco.render_widget(
                Paragraph::new(Line::from(vec![
                    span_texto("Host: "),
                    Span::styled(host_nombre.clone(), Style::default().fg(tema.paleta.acento)),
                ])),
                trozos[0],
            );
            let mut lineas = Vec::new();
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
                lineas.push(Line::from(Span::styled(
                    format!(" {}", opcion.etiqueta),
                    estilo,
                )));
            }
            marco.render_widget(Paragraph::new(lineas), trozos[1]);
            let filtro = if desplegable.filtro.texto.is_empty() {
                String::new()
            } else {
                format!("filtro: {}", desplegable.filtro.texto)
            };
            marco.render_widget(
                Paragraph::new(vec![
                    Line::from(vec![
                        atajo("↵", tema),
                        span_texto(" mover   "),
                        atajo("esc", tema),
                        span_texto(" cancelar"),
                    ]),
                    Line::from(Span::styled(
                        format!(" {filtro}"),
                        Style::default().fg(tema.paleta.inactivo),
                    )),
                ]),
                trozos[2],
            );
        }
        Dialogo::ResumenImportacion { titulo, lineas } => {
            let alto = lineas.len() as u16 + 3;
            let recta = centrar(area, 74, alto);
            let mut contenido: Vec<Line> = lineas.iter().map(|texto| linea(texto)).collect();
            contenido.push(Line::from(""));
            contenido.push(Line::from(vec![
                atajo("↵ / esc", tema),
                span_texto(" cerrar"),
            ]));
            modal(marco, recta, titulo, contenido, false, tema);
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
            let recta = centrar(area, 74, 12);
            let (etiqueta_origen, etiqueta_destino) = match lado_origen {
                crate::archivos::Lado::Local => ("local", "remoto"),
                crate::archivos::Lado::Remoto => ("remoto", "local"),
            };
            let contenido = vec![
                Line::from(""),
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
                Line::from(""),
                Line::from(vec![
                    atajo("s", tema),
                    span_texto(" sobrescribir   "),
                    atajo("o", tema),
                    span_texto(" omitir   "),
                    atajo("S", tema),
                    span_texto(" todos   "),
                    atajo("O", tema),
                    span_texto(" omitir todos"),
                ]),
                Line::from(vec![
                    atajo("esc", tema),
                    span_texto(" cancelar la operación entera"),
                ]),
            ];
            modal(
                marco,
                recta,
                "YA EXISTE EN EL DESTINO",
                contenido,
                true,
                tema,
            );
        }
        Dialogo::Detalle { titulo, lineas } => {
            let alto = lineas.len() as u16 + 4;
            let recta = centrar(area, 74, alto);
            let mut contenido: Vec<Line> = lineas.iter().map(|texto| linea(texto)).collect();
            contenido.push(Line::from(""));
            contenido.push(Line::from(vec![
                atajo("↵ / esc", tema),
                span_texto(" cerrar"),
            ]));
            modal(marco, recta, titulo, contenido, false, tema);
        }
        Dialogo::ExportarRegistro { estado } => {
            let recta = centrar(area, 70, 11);
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
            let contenido = vec![
                Line::from(vec![
                    Span::styled("Formato  ", estilo_foco(estado.foco == 0)),
                    Span::raw(format!(
                        "{} CSV   {} JSON",
                        marca(!estado.json),
                        marca(estado.json)
                    )),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Ruta     ", estilo_foco(estado.foco == 1)),
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    estado
                        .campo
                        .span(estado.foco == 1, Style::default().fg(tema.paleta.texto)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(""),
                Line::from(Span::styled(
                    "  el fichero se escribe con permisos 600",
                    Style::default().fg(tema.paleta.inactivo),
                )),
                Line::from(""),
                Line::from(vec![
                    atajo("⇥", tema),
                    span_texto(" cambiar campo   "),
                    atajo("espacio", tema),
                    span_texto(" alternar formato   "),
                    atajo("↵", tema),
                    span_texto(" exportar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, "EXPORTAR REGISTRO", contenido, false, tema);
        }
        Dialogo::GenerarClave { estado } => {
            use crate::app::CampoGeneracion;
            let recta = centrar(area, 74, 17);
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
            let enmascarar = |texto: &str| -> String {
                if texto.is_empty() {
                    "\u{2503}".to_string()
                } else {
                    texto
                        .chars()
                        .map(|_| if tema.ascii { '*' } else { '•' })
                        .collect()
                }
            };
            let mut frase_visible = enmascarar(&estado.frase.texto);
            let mut repetir_visible = enmascarar(&estado.repetir.texto);
            if activo(CampoGeneracion::Frase) && !estado.frase.texto.is_empty() {
                frase_visible.push('\u{2503}');
            }
            if activo(CampoGeneracion::Repetir) && !estado.repetir.texto.is_empty() {
                repetir_visible.push('\u{2503}');
            }
            let contenido = vec![
                Line::from(vec![
                    etiqueta(CampoGeneracion::Fichero, "Fichero"),
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    estado.fichero.span(
                        activo(CampoGeneracion::Fichero),
                        estilo(CampoGeneracion::Fichero),
                    ),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
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
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    estado.comentario.span(
                        activo(CampoGeneracion::Comentario),
                        estilo(CampoGeneracion::Comentario),
                    ),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(vec![
                    etiqueta(CampoGeneracion::Frase, "Frase"),
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(frase_visible, estilo(CampoGeneracion::Frase)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(vec![
                    etiqueta(CampoGeneracion::Repetir, "Repetir"),
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(repetir_visible, estilo(CampoGeneracion::Repetir)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(vec![
                    Span::raw("  "),
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
                Line::from(""),
                Line::from(Span::styled(
                    if estado.frase.texto.is_empty() {
                        "  sin frase: la clave quedará sin cifrar en ~/.ssh"
                    } else {
                        "  la frase cifra la clave en formato OpenSSH"
                    },
                    Style::default().fg(tema.paleta.inactivo),
                )),
                Line::from(""),
                Line::from(vec![
                    atajo("^s", tema),
                    span_texto(" generar   "),
                    atajo("⇥", tema),
                    span_texto(" campo   "),
                    atajo("espacio", tema),
                    span_texto(" marcar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, "GENERAR CLAVE", contenido, false, tema);
        }
        Dialogo::FraseImportacion { campo, .. } => {
            let recta = centrar(area, 60, 9);
            let enmascarado: String = campo
                .texto
                .chars()
                .map(|_| if tema.ascii { '*' } else { '•' })
                .collect();
            let visible = if campo.texto.is_empty() {
                "\u{2503}".to_string()
            } else {
                enmascarado
            };
            let contenido = vec![
                linea("La clave importada está cifrada."),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[ ", Style::default().fg(tema.paleta.acento)),
                    Span::styled(visible, Style::default().fg(tema.paleta.texto)),
                    Span::styled(" ]", Style::default().fg(tema.paleta.acento)),
                ]),
                Line::from(""),
                Line::from(vec![
                    atajo("↵", tema),
                    span_texto(" aceptar   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(marco, recta, "FRASE DE LA CLAVE", contenido, false, tema);
        }
        Dialogo::ConflictoImportacion { nombre, restantes } => {
            let recta = centrar(area, 74, 12);
            let contenido = vec![
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
                Line::from(""),
                Line::from(vec![
                    atajo("s", tema),
                    span_texto(" sobrescribir   "),
                    atajo("o", tema),
                    span_texto(" omitir"),
                ]),
                Line::from(vec![
                    atajo("S", tema),
                    span_texto(" sobrescribir todos   "),
                    atajo("O", tema),
                    span_texto(" omitir todos   "),
                    atajo("esc", tema),
                    span_texto(" cancelar"),
                ]),
            ];
            modal(
                marco,
                recta,
                "CONFLICTO DE IMPORTACIÓN",
                contenido,
                true,
                tema,
            );
        }
    }
}

fn modal(
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
            format!(" {titulo} "),
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
    marco.render_widget(Paragraph::new(contenido), area_texto);
}
