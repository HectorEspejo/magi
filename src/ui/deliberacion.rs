//! Diálogo de deliberación MAGI (maqueta §6.2): acción, tabla host ×
//! comprobación con glifo y dato de cada veredicto, barra de consenso y
//! estado (DELIBERANDO, APROBADO, BLOQUEADO, FORZADO). Los nombres MAGI van
//! siempre con la palabra que dice qué comprueban.
//!
//! Disposición (Fase 7, maqueta §6.5): con la terminal estrecha (< 100
//! columnas) o baja (< 20 filas) el diálogo pasa a compacto: la acción va en
//! el título, los nombres MAGI se abrevian (`M-1 salud`, `B-2 backup`,
//! `C-3 tests`), los datos se recortan y consenso y estado comparten línea.
//! En los dos modos los hosts se desplazan con `↑` `↓` si no caben: su
//! ventana se deriva del área en cada pintado y se registra en la
//! `Disposicion` (`Lista::DeliberacionHosts`).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, DeliberacionAbierta};
use crate::deliberacion::estado::Fase;
use crate::deliberacion::{Comprobacion, ComprobacionesHost, Veredicto};
use crate::tema::Tema;
use crate::ui::dialogos::{atajo, modal};
use crate::ui::disposicion::{self, Atajo, Disposicion, Lista, VentanaLista, ANCHO_MAX_DIALOGO};
use crate::ui::ejecutar::{campo_visible, pie_por_prioridad};
use crate::ui::{centrar, tecla};

/// Ancho de la columna de hosts y largo de la barra de consenso.
const ANCHO_HOST: usize = 16;
const LARGO_BARRA: usize = 21;

/// Modo compacto: ancho máximo del nombre del host, de cada columna de
/// comprobación (mínimo y máximo), largo de la barra y ancho mínimo del
/// texto (para que quepan el consenso y las teclas).
const ANCHO_HOST_COMPACTO: usize = 14;
const COLUMNA_COMPACTA_MIN: usize = 7;
const COLUMNA_COMPACTA_MAX: usize = 18;
const LARGO_BARRA_COMPACTA: usize = 10;
const TEXTO_COMPACTO_MIN: usize = 36;

pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    deliberacion: &DeliberacionAbierta,
    disp: &mut Disposicion,
) {
    dibujar_con_tema(marco, area, &app.tema, deliberacion, disp);
}

/// El diálogo sobre `area` con un tema dado (lo usan las pruebas).
pub fn dibujar_con_tema(
    marco: &mut Frame,
    area: Rect,
    tema: &Tema,
    deliberacion: &DeliberacionAbierta,
    disp: &mut Disposicion,
) {
    if es_compacto(area) {
        dibujar_compacto(marco, area, tema, deliberacion, disp);
    } else {
        dibujar_completo(marco, area, tema, deliberacion, disp);
    }
}

/// ¿Va compacto? Con la terminal estrecha o baja (§7.2).
pub fn es_compacto(area: Rect) -> bool {
    disposicion::es_estrecho(area.width) || disposicion::es_bajo(area.height)
}

/// Ventana de hosts visible: parte del desplazamiento guardado y deja la
/// selección a la vista sin huecos al final. La registra en `disp`.
fn ventana_hosts(
    deliberacion: &DeliberacionAbierta,
    filas: usize,
    disp: &mut Disposicion,
) -> (usize, usize) {
    let total = deliberacion.filas.len();
    let seleccion = deliberacion.seleccion.min(total.saturating_sub(1));
    let inicio = disposicion::ventana(deliberacion.desplazamiento, seleccion, filas, total);
    disp.registrar(
        Lista::DeliberacionHosts,
        VentanaLista {
            inicio,
            filas: filas.max(1),
            total,
        },
    );
    (inicio, seleccion)
}

fn dibujar_completo(
    marco: &mut Frame,
    area: Rect,
    tema: &Tema,
    deliberacion: &DeliberacionAbierta,
    disp: &mut Disposicion,
) {
    let ancho = area.width.saturating_sub(2).min(ANCHO_MAX_DIALOGO);
    // Lo que no es la tabla: bordes y márgenes (4), acción y motivos (2),
    // separadores (2), cabecera (2), detalle (1), consenso y estado (2),
    // hueco y teclas (2); forzando, el rótulo y el campo (2) y, sin
    // comprobaciones activas, el aviso (1).
    let fijo = 15
        + match deliberacion.maquina.fase {
            Fase::Motivo => 2,
            Fase::Aprobada if deliberacion.consenso().activas == 0 => 1,
            _ => 0,
        };
    let caben = (area.height.saturating_sub(2) as usize)
        .saturating_sub(fijo)
        .max(1);
    let alto = (fijo + deliberacion.filas.len().min(caben)) as u16;
    let recta = centrar(area, ancho, alto);
    let interior = recta.width.saturating_sub(6) as usize;
    let (desde, _) = ventana_hosts(deliberacion, caben, disp);
    let lineas = lineas(deliberacion, tema, interior, caben, desde);
    let peligro = matches!(deliberacion.maquina.fase, Fase::Bloqueada | Fase::Motivo);
    modal(marco, recta, "DELIBERACIÓN MAGI", lineas, peligro, tema);
}

/// Las líneas del diálogo completo para un interior de `ancho` columnas,
/// como mucho `caben` filas de hosts y la primera visible en `desde`.
pub fn lineas(
    deliberacion: &DeliberacionAbierta,
    tema: &Tema,
    ancho: usize,
    caben: usize,
    desde: usize,
) -> Vec<Line<'static>> {
    let ascii = tema.ascii;
    let punto = tema.glifos.punto_medio;
    let seleccionado = tema.glifos.seleccion;
    let raya = if ascii { "-" } else { "—" };
    let texto = Style::default().fg(tema.paleta.texto);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let negrita = texto.add_modifier(Modifier::BOLD);
    let mut lineas = Vec::new();
    let plan = &deliberacion.plan;
    let destino = match plan.hosts.as_slice() {
        [(_, host)] => host.clone(),
        hosts => format!("{} hosts", hosts.len()),
    };
    let pestana = if plan.modo == crate::app::lanzar::ModoLanzamiento::Pestanas {
        " (en pestaña)"
    } else {
        ""
    };
    lineas.push(Line::from(vec![
        Span::styled("ACCIÓN: ", negrita),
        Span::styled(
            recortar(
                &format!(
                    "{} {} {destino}{pestana}",
                    plan.nombre,
                    tecla(tema, "→", "->")
                ),
                ancho.saturating_sub(8),
                ascii,
            ),
            texto,
        ),
    ]));
    lineas.push(Line::from(Span::styled(
        recortar(
            &format!(
                "requerida por: {}",
                deliberacion.motivos.join(&format!(" {punto} "))
            ),
            ancho,
            ascii,
        ),
        tenue,
    )));
    lineas.push(separador(ancho, tema));

    let columna = ancho.saturating_sub(ANCHO_HOST) / 3;
    let mut nombres = vec![Span::raw(rellenar("", ANCHO_HOST))];
    let mut palabras = vec![Span::raw(rellenar("", ANCHO_HOST))];
    for comprobacion in Comprobacion::TODAS {
        nombres.push(Span::styled(
            rellenar(comprobacion.nombre_magi(), columna),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
        let palabra = match comprobacion {
            Comprobacion::Backup => format!("backup<{} h", deliberacion.backup_horas),
            otra => otra.palabra().to_string(),
        };
        palabras.push(Span::styled(rellenar(&palabra, columna), tenue));
    }
    lineas.push(Line::from(nombres));
    lineas.push(Line::from(palabras));

    let total = deliberacion.filas.len();
    let seleccion = deliberacion.seleccion.min(total.saturating_sub(1));
    for (indice, fila) in deliberacion
        .filas
        .iter()
        .enumerate()
        .skip(desde)
        .take(caben)
    {
        let marcada = indice == seleccion && total > 1;
        let estilo_host = if marcada {
            negrita.add_modifier(Modifier::REVERSED)
        } else {
            texto
        };
        let mut spans = vec![Span::styled(
            rellenar(&recortar(&fila.host, ANCHO_HOST - 1, ascii), ANCHO_HOST),
            estilo_host,
        )];
        for comprobacion in Comprobacion::TODAS {
            let veredicto = fila.veredicto(comprobacion);
            let (glifo, color) = glifo(veredicto, tema);
            let dato = match veredicto {
                Veredicto::Pendiente => "comprobando".to_string(),
                otro => degradar(otro.detalle().unwrap_or(""), tema),
            };
            let celda = format!(
                "{glifo} {}",
                recortar(&dato, columna.saturating_sub(3), ascii)
            );
            spans.push(Span::styled(
                rellenar(&celda, columna),
                Style::default().fg(color),
            ));
        }
        lineas.push(Line::from(spans));
    }
    if total > caben {
        lineas.push(Line::from(Span::styled(
            format!(
                "  {} {} recorrer hosts ({} de {total})",
                tema.glifos.arriba,
                tema.glifos.abajo,
                seleccion + 1
            ),
            tenue,
        )));
    } else {
        lineas.push(detalle_seleccionado(deliberacion, seleccion, ancho, tema));
    }
    lineas.push(separador(ancho, tema));

    let consenso = deliberacion.consenso();
    let color_consenso = color_fase(deliberacion.maquina.fase, tema);
    let (lleno, vacio) = barra(consenso.aprobadas, consenso.activas, LARGO_BARRA, ascii);
    lineas.push(Line::from(vec![
        Span::styled("CONSENSO  ", negrita),
        Span::styled(
            format!("{} / {}", consenso.aprobadas, consenso.activas),
            negrita,
        ),
        Span::raw("     "),
        Span::styled(lleno, Style::default().fg(color_consenso)),
        Span::styled(vacio, tenue),
    ]));

    let maquina = &deliberacion.maquina;
    let estado = |texto: String, color: Color| {
        Line::from(Span::styled(
            texto,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
    };
    let mut teclas: Vec<Span<'static>> = Vec::new();
    match maquina.fase {
        Fase::Comprobando => {
            lineas.push(estado(
                format!(
                    "{seleccionado} DELIBERANDO {raya} {} comprobación(es) en marcha",
                    consenso.pendientes
                ),
                tema.paleta.acento,
            ));
            teclas.extend([atajo("esc", tema), Span::styled(" cancelar", tenue)]);
        }
        Fase::Aprobada => {
            if consenso.activas == 0 {
                lineas.push(Line::from(Span::styled(
                    "sin comprobaciones configuradas",
                    tenue,
                )));
            }
            lineas.push(estado(
                format!("{seleccionado} APROBADO {raya} ctrl+k ejecutar {punto} esc cancelar"),
                tema.paleta.correcto,
            ));
            teclas.extend([
                atajo("ctrl+k", tema),
                Span::styled(" ejecutar    ", tenue),
                atajo("esc", tema),
                Span::styled(" cancelar", tenue),
            ]);
        }
        Fase::Bloqueada => {
            lineas.push(estado(
                format!("{seleccionado} BLOQUEADO {raya} se requiere unanimidad"),
                tema.paleta.critico,
            ));
            teclas.extend([
                atajo("f", tema),
                Span::styled(" forzar (registra motivo)    ", tenue),
                atajo("esc", tema),
                Span::styled(" cancelar", tenue),
            ]);
        }
        Fase::Motivo => {
            lineas.push(Line::from(Span::styled(
                format!(
                    "Motivo del forzado (mín. {} caracteres):",
                    maquina.motivo_min()
                ),
                texto,
            )));
            lineas.push(Line::from(vec![
                Span::styled("[ ", tenue),
                Span::styled(
                    campo_visible(&maquina.motivo, true, ancho.saturating_sub(4), ascii),
                    crate::ui::componentes::estilo_campo(tema, true),
                ),
                Span::styled(" ]", tenue),
            ]));
            if maquina.forzado_listo() {
                lineas.push(estado(
                    format!("{seleccionado} FORZADO {raya} quedará registrado con usuario y hora"),
                    tema.paleta.critico,
                ));
            } else {
                let aviso = format!(
                    "mínimo {} caracteres ({}/{})",
                    maquina.motivo_min(),
                    maquina.largo_motivo(),
                    maquina.motivo_min()
                );
                let color = if maquina.aviso_minimo {
                    tema.paleta.critico
                } else {
                    tema.paleta.inactivo
                };
                lineas.push(estado(
                    format!("{seleccionado} BLOQUEADO {raya} {aviso}"),
                    color,
                ));
            }
            teclas.extend([
                atajo("ctrl+k", tema),
                Span::styled(" ejecutar    ", tenue),
                atajo("esc", tema),
                Span::styled(" volver", tenue),
            ]);
        }
    }
    lineas.push(Line::from(""));
    // Escribiendo el motivo, `?` es un carácter más del campo.
    if maquina.fase != Fase::Motivo {
        teclas.extend([
            Span::raw("    "),
            atajo("?", tema),
            Span::styled(" ayuda", tenue),
        ]);
    }
    lineas.push(Line::from(teclas));
    lineas
}

// ---------------------------------------------------------------- compacto

/// Anchos del modo compacto: columna de hosts, de cada comprobación y del
/// texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AnchosCompactos {
    host: usize,
    columna: usize,
    texto: usize,
}

/// Los anchos naturales (los nombres y los datos recortados que haya),
/// encogidos hasta `texto_max` si no caben: primero las columnas, sin bajar
/// de su cabecera abreviada (`B-2 backup` no se corta: es el nombre), luego
/// la de hosts.
fn anchos_compactos(deliberacion: &DeliberacionAbierta, texto_max: usize) -> AnchosCompactos {
    let nombre = deliberacion
        .filas
        .iter()
        .map(|fila| fila.host.chars().count())
        .max()
        .unwrap_or(4)
        .min(ANCHO_HOST_COMPACTO);
    let mut host = nombre + 2;
    let cabeceras = Comprobacion::TODAS
        .iter()
        .map(|comprobacion| cabecera_compacta(*comprobacion).chars().count());
    // La cabecera más larga y un espacio de separación.
    let columna_min = cabeceras
        .clone()
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .max(COLUMNA_COMPACTA_MIN);
    let celdas = deliberacion.filas.iter().flat_map(|fila| {
        Comprobacion::TODAS.iter().map(move |comprobacion| {
            2 + dato_compacto(fila.veredicto(*comprobacion)).chars().count()
        })
    });
    let mut columna = cabeceras
        .chain(celdas)
        .max()
        .unwrap_or(COLUMNA_COMPACTA_MIN)
        .min(COLUMNA_COMPACTA_MAX)
        + 2;
    if host + 3 * columna > texto_max {
        columna = (texto_max.saturating_sub(host) / 3).max(columna_min);
        if host + 3 * columna > texto_max {
            host = texto_max.saturating_sub(3 * columna).max(6);
        }
    }
    let texto = (host + 3 * columna).max(TEXTO_COMPACTO_MIN).min(texto_max);
    AnchosCompactos {
        host,
        columna,
        texto,
    }
}

fn dibujar_compacto(
    marco: &mut Frame,
    area: Rect,
    tema: &Tema,
    deliberacion: &DeliberacionAbierta,
    disp: &mut Disposicion,
) {
    let ascii = tema.ascii;
    let maquina = &deliberacion.maquina;
    let consenso = deliberacion.consenso();
    let total = deliberacion.filas.len();
    // Bordes y un margen de una columna a cada lado; sin margen vertical.
    let ancho_max = usize::from(area.width.saturating_sub(2).min(ANCHO_MAX_DIALOGO));
    let anchos = anchos_compactos(deliberacion, ancho_max.saturating_sub(4));

    let texto = Style::default().fg(tema.paleta.texto);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let color = color_fase(maquina.fase, tema);
    let (lleno, vacio) = barra(
        consenso.aprobadas,
        consenso.activas,
        LARGO_BARRA_COMPACTA,
        ascii,
    );
    let rotulo_consenso = format!("CONSENSO {}/{} ", consenso.aprobadas, consenso.activas);
    let estado = estado_compacto(deliberacion, tema);
    let largo_consenso = rotulo_consenso.chars().count() + LARGO_BARRA_COMPACTA;
    let en_una = largo_consenso + 2 + estado.chars().count() <= anchos.texto;
    let lineas_estado = if en_una { 1 } else { 2 };
    let motivo = usize::from(maquina.fase == Fase::Motivo);
    // Cabecera, estado, campo del motivo y teclas.
    let fijas = 1 + lineas_estado + motivo + 1;
    let alto_max = usize::from(area.height.saturating_sub(2));
    let caben = alto_max.saturating_sub(2 + fijas).max(1);
    let filas_hosts = total.min(caben).max(1);
    let alto = (2 + fijas + filas_hosts) as u16;
    let recta = centrar(area, (anchos.texto + 4) as u16, alto);
    let ancho_texto = usize::from(recta.width.saturating_sub(4));
    let (desde, seleccion) = ventana_hosts(deliberacion, filas_hosts, disp);

    let mut lineas: Vec<Line<'static>> = Vec::new();
    // Cabecera: `M-1 salud  B-2 backup  C-3 tests`.
    let mut cabecera = vec![Span::raw(" ".repeat(anchos.host))];
    for comprobacion in Comprobacion::TODAS {
        cabecera.push(Span::styled(
            rellenar(
                &recortar(
                    &cabecera_compacta(comprobacion),
                    anchos.columna.saturating_sub(1),
                    ascii,
                ),
                anchos.columna,
            ),
            Style::default()
                .fg(tema.paleta.acento)
                .add_modifier(Modifier::BOLD),
        ));
    }
    lineas.push(Line::from(cabecera));
    for (indice, fila) in deliberacion
        .filas
        .iter()
        .enumerate()
        .skip(desde)
        .take(filas_hosts)
    {
        lineas.push(fila_compacta(
            fila,
            indice == seleccion && total > 1,
            anchos,
            tema,
        ));
    }
    let consenso_spans = vec![
        Span::styled(rotulo_consenso, texto.add_modifier(Modifier::BOLD)),
        Span::styled(lleno, Style::default().fg(color)),
        Span::styled(vacio, tenue),
    ];
    let estado_span = Span::styled(
        recortar(&estado, ancho_texto, ascii),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    );
    if en_una {
        let mut spans = consenso_spans;
        spans.push(Span::raw("  "));
        spans.push(estado_span);
        lineas.push(Line::from(spans));
    } else {
        lineas.push(Line::from(consenso_spans));
        lineas.push(Line::from(estado_span));
    }
    if maquina.fase == Fase::Motivo {
        let etiqueta = "motivo [ ";
        let hueco = ancho_texto.saturating_sub(etiqueta.chars().count() + 2);
        lineas.push(Line::from(vec![
            Span::styled(etiqueta, tenue),
            Span::styled(
                campo_visible(&maquina.motivo, true, hueco, ascii),
                crate::ui::componentes::estilo_campo(tema, true),
            ),
            Span::styled(" ]", tenue),
        ]));
    }
    lineas.push(teclas_compactas(deliberacion, seleccion, ancho_texto, tema));

    let peligro = matches!(maquina.fase, Fase::Bloqueada | Fase::Motivo);
    let color_borde = if peligro {
        tema.paleta.critico
    } else {
        tema.paleta.acento
    };
    let titulo = titulo_compacto(deliberacion, tema, usize::from(recta.width));
    let bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(color_borde))
        .title(Span::styled(
            titulo,
            Style::default()
                .fg(color_borde)
                .add_modifier(Modifier::BOLD),
        ));
    let interior = bloque.inner(recta);
    marco.render_widget(Clear, recta);
    marco.render_widget(bloque, recta);
    let zona = Rect {
        x: interior.x + 1,
        width: interior.width.saturating_sub(2),
        ..interior
    };
    marco.render_widget(Paragraph::new(lineas), zona);
}

/// ` DELIBERACIÓN MAGI ─ reiniciar nginx → 3 `: la acción recortada a lo
/// que quepa en el borde de `ancho` columnas.
fn titulo_compacto(deliberacion: &DeliberacionAbierta, tema: &Tema, ancho: usize) -> String {
    let plan = &deliberacion.plan;
    let destino = match plan.hosts.as_slice() {
        [(_, host)] => host.clone(),
        hosts => hosts.len().to_string(),
    };
    let cabeza = format!(" DELIBERACIÓN MAGI {} ", tema.glifos.linea);
    let accion = format!("{} {} {destino}", plan.nombre, tecla(tema, "→", "->"));
    // Esquinas y un espacio al final.
    let hueco = ancho.saturating_sub(2 + cabeza.chars().count() + 1);
    if hueco < 4 {
        return " DELIBERACIÓN MAGI ".to_string();
    }
    format!("{cabeza}{} ", recortar(&accion, hueco, tema.ascii))
}

/// Una fila de host compacta: nombre y, por comprobación, glifo y dato
/// recortado.
fn fila_compacta(
    fila: &ComprobacionesHost,
    marcada: bool,
    anchos: AnchosCompactos,
    tema: &Tema,
) -> Line<'static> {
    let ascii = tema.ascii;
    let texto = Style::default().fg(tema.paleta.texto);
    let estilo_host = if marcada {
        texto
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED)
    } else {
        texto
    };
    let mut spans = vec![Span::styled(
        rellenar(
            &recortar(&fila.host, anchos.host.saturating_sub(1), ascii),
            anchos.host,
        ),
        estilo_host,
    )];
    for comprobacion in Comprobacion::TODAS {
        let veredicto = fila.veredicto(comprobacion);
        let (glifo, color) = glifo(veredicto, tema);
        let dato = recortar(
            &dato_compacto(veredicto),
            anchos.columna.saturating_sub(3),
            ascii,
        );
        let celda = if dato.is_empty() {
            glifo.to_string()
        } else {
            format!("{glifo} {dato}")
        };
        spans.push(Span::styled(
            rellenar(&celda, anchos.columna),
            Style::default().fg(color),
        ));
    }
    Line::from(spans)
}

/// Estado en pocas palabras: `▸ BLOQUEADO`, `▸ FORZADO`…
fn estado_compacto(deliberacion: &DeliberacionAbierta, tema: &Tema) -> String {
    let marca = tema.glifos.seleccion;
    let punto = tema.glifos.punto_medio;
    let maquina = &deliberacion.maquina;
    let consenso = deliberacion.consenso();
    match maquina.fase {
        Fase::Comprobando => format!("{marca} DELIBERANDO ({})", consenso.pendientes),
        Fase::Aprobada if consenso.activas == 0 => {
            format!("{marca} APROBADO {punto} sin comprobaciones")
        }
        Fase::Aprobada => format!("{marca} APROBADO"),
        Fase::Bloqueada => format!("{marca} BLOQUEADO"),
        Fase::Motivo if maquina.forzado_listo() => format!("{marca} FORZADO"),
        Fase::Motivo => format!(
            "{marca} BLOQUEADO {punto} mín. {} ({}/{})",
            maquina.motivo_min(),
            maquina.largo_motivo(),
            maquina.motivo_min()
        ),
    }
}

/// Teclas de la fase por prioridad y, a la derecha, la posición en la
/// lista de hosts (`↑↓ 1/3`).
fn teclas_compactas(
    deliberacion: &DeliberacionAbierta,
    seleccion: usize,
    ancho: usize,
    tema: &Tema,
) -> Line<'static> {
    let total = deliberacion.filas.len();
    let posicion = if total > 1 {
        format!(
            "{}{} {}/{total}",
            tema.glifos.arriba,
            tema.glifos.abajo,
            seleccion + 1
        )
    } else {
        String::new()
    };
    let mut atajos = match deliberacion.maquina.fase {
        Fase::Comprobando => vec![Atajo::new("esc", "cancelar", 1)],
        Fase::Aprobada => vec![
            Atajo::new("ctrl+k", "ejecutar", 1),
            Atajo::new("esc", "cancelar", 1),
        ],
        Fase::Bloqueada => vec![
            Atajo::new("f", "forzar", 1),
            Atajo::new("esc", "cancelar", 1),
        ],
        Fase::Motivo => vec![
            Atajo::new("ctrl+k", "ejecutar", 1),
            Atajo::new("esc", "volver", 1),
        ],
    };
    // Escribiendo el motivo, `?` es un carácter más del campo.
    if deliberacion.maquina.fase != Fase::Motivo {
        atajos.push(Atajo::new("?", "ayuda", 3));
    }
    let largo_posicion = posicion.chars().count();
    let hueco = if largo_posicion > 0 {
        ancho.saturating_sub(largo_posicion + 2)
    } else {
        ancho
    };
    let mut linea = pie_por_prioridad(&atajos, hueco, tema);
    if largo_posicion > 0 {
        let usado: usize = linea
            .spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        let relleno = ancho.saturating_sub(usado + largo_posicion);
        linea.spans.push(Span::raw(" ".repeat(relleno.max(1))));
        linea.spans.push(Span::styled(
            posicion,
            Style::default().fg(tema.paleta.inactivo),
        ));
    }
    linea
}

/// `MELCHIOR-1` → `M-1 salud`: inicial y número del nombre MAGI y la palabra
/// que dice qué comprueba (la metáfora nunca va sola).
pub fn cabecera_compacta(comprobacion: Comprobacion) -> String {
    format!(
        "{} {}",
        abreviar_magi(comprobacion.nombre_magi()),
        comprobacion.palabra()
    )
}

/// `MELCHIOR-1` → `M-1`.
pub fn abreviar_magi(nombre: &str) -> String {
    let inicial: String = nombre.chars().take(1).collect();
    match nombre.rsplit_once('-') {
        Some((_, numero)) => format!("{inicial}-{numero}"),
        None => inicial,
    }
}

/// Dato de una celda en el modo compacto: lo esencial del veredicto (hasta
/// el primer « · » o « (», sin «hace»): `NOMINAL`, `3 h`, `31 h`. Sin dato
/// en las celdas no activas; `comprobando` mientras espera.
pub fn dato_compacto(veredicto: &Veredicto) -> String {
    let dato = match veredicto {
        Veredicto::NoActiva => return String::new(),
        Veredicto::Pendiente => return "comprobando".to_string(),
        otro => otro.detalle().unwrap_or(""),
    };
    let dato = dato.split(" · ").next().unwrap_or("");
    let dato = dato.split(" (").next().unwrap_or("");
    let dato = dato.trim();
    let dato = dato
        .strip_prefix("último backup hace ")
        .or_else(|| dato.strip_prefix("hace "))
        .unwrap_or(dato);
    dato.to_string()
}

/// Barra de consenso de `largo` celdas: llena en proporción a las
/// aprobadas. Sin comprobaciones activas va llena: no hay nada en contra.
fn barra(aprobadas: usize, activas: usize, largo: usize, ascii: bool) -> (String, String) {
    let llenas = (aprobadas * largo)
        .checked_div(activas)
        .unwrap_or(largo)
        .min(largo);
    let (lleno, vacio) = if ascii { ("#", ".") } else { ("█", "░") };
    (lleno.repeat(llenas), vacio.repeat(largo - llenas))
}

fn color_fase(fase: Fase, tema: &Tema) -> Color {
    match fase {
        Fase::Comprobando => tema.paleta.acento,
        Fase::Aprobada => tema.paleta.correcto,
        Fase::Bloqueada | Fase::Motivo => tema.paleta.critico,
    }
}

/// Los datos completos del host seleccionado (las celdas se recortan).
fn detalle_seleccionado(
    deliberacion: &DeliberacionAbierta,
    seleccion: usize,
    ancho: usize,
    tema: &Tema,
) -> Line<'static> {
    let Some(fila) = deliberacion.filas.get(seleccion) else {
        return Line::from("");
    };
    let punto = tema.glifos.punto_medio;
    let partes: Vec<String> = Comprobacion::TODAS
        .iter()
        .filter_map(|comprobacion| {
            let veredicto = fila.veredicto(*comprobacion);
            veredicto.rechaza().then(|| {
                format!(
                    "{}: {}",
                    comprobacion.palabra(),
                    degradar(veredicto.detalle().unwrap_or(""), tema)
                )
            })
        })
        .collect();
    if partes.is_empty() {
        return Line::from("");
    }
    Line::from(Span::styled(
        recortar(
            &format!(
                "  {} {punto} {}",
                fila.host,
                partes.join(&format!(" {punto} "))
            ),
            ancho,
            tema.ascii,
        ),
        Style::default().fg(tema.paleta.critico),
    ))
}

/// Glifo y color de una celda: `✓` `✕` `—` `◐` (ASCII `* x - o`).
pub fn glifo(veredicto: &Veredicto, tema: &Tema) -> (&'static str, Color) {
    let (unicode, ascii, color) = match veredicto {
        Veredicto::Aprueba { .. } => ("✓", "*", tema.paleta.correcto),
        Veredicto::Rechaza { .. } => ("✕", "x", tema.paleta.critico),
        Veredicto::NoActiva => ("—", "-", tema.paleta.inactivo),
        Veredicto::Pendiente => ("◐", "o", tema.paleta.acento),
    };
    (if tema.ascii { ascii } else { unicode }, color)
}

/// Los datos de los veredictos los escribe MAGI con `·`: en ASCII, con su
/// equivalente.
fn degradar(dato: &str, tema: &Tema) -> String {
    if tema.ascii {
        dato.replace('·', tema.glifos.punto_medio)
    } else {
        dato.to_string()
    }
}

fn separador(ancho: usize, tema: &Tema) -> Line<'static> {
    Line::from(Span::styled(
        tema.glifos.linea.repeat(ancho),
        Style::default().fg(tema.paleta.inactivo),
    ))
}

/// Recorta a `ancho` caracteres con «…» (`~` en ASCII), sin caracteres de
/// control ni de dirección (nombres de host o de snippet importados) y sin
/// espacios colgando antes de la marca (`hace 31…`, no `hace 31 …`).
pub fn recortar(texto: &str, ancho: usize, ascii: bool) -> String {
    let limpio: String = texto
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect();
    let recortado = disposicion::recortar(&limpio, ancho, ascii);
    let marca = if ascii { '~' } else { '…' };
    match recortado.strip_suffix(marca) {
        Some(resto) if recortado != limpio && resto.ends_with(' ') => {
            format!("{}{marca}", resto.trim_end())
        }
        _ => recortado,
    }
}

fn rellenar(texto: &str, ancho: usize) -> String {
    let largo = texto.chars().count();
    if largo >= ancho {
        texto.to_string()
    } else {
        format!("{texto}{}", " ".repeat(ancho - largo))
    }
}

#[cfg(test)]
mod pruebas {
    use std::collections::BTreeMap;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::app::lanzar::{ModoLanzamiento, PlanEjecucion};
    use crate::deliberacion::estado::AccionMaquina;
    use crate::deliberacion::ComprobacionesHost;

    fn plan(hosts: &[(i64, &str)]) -> PlanEjecucion {
        PlanEjecucion {
            snippet_id: 1,
            nombre: "reiniciar nginx".to_string(),
            comando: "systemctl restart nginx".to_string(),
            hosts: hosts.iter().map(|(id, n)| (*id, n.to_string())).collect(),
            timeout_seg: 60,
            parar_al_fallo: false,
            critico: true,
            valores: BTreeMap::new(),
            modo: ModoLanzamiento::Servidor,
        }
    }

    fn aprueba(detalle: &str) -> Veredicto {
        Veredicto::Aprueba {
            detalle: detalle.to_string(),
            ms: 1,
        }
    }

    fn rechaza(detalle: &str) -> Veredicto {
        Veredicto::Rechaza {
            detalle: detalle.to_string(),
            ms: 1,
        }
    }

    fn fila(id: i64, host: &str, salud: Veredicto, backup: Veredicto) -> ComprobacionesHost {
        ComprobacionesHost {
            host_id: id,
            host: host.to_string(),
            salud,
            backup,
            tests: Veredicto::NoActiva,
        }
    }

    fn bloqueada() -> DeliberacionAbierta {
        DeliberacionAbierta::de_prueba(
            plan(&[(1, "hetzner-01"), (2, "hetzner-02")]),
            vec![
                fila(
                    1,
                    "hetzner-01",
                    aprueba("NOMINAL · 0,2 s"),
                    aprueba("hace 3 h"),
                ),
                fila(
                    2,
                    "hetzner-02",
                    aprueba("NOMINAL · 0,1 s"),
                    rechaza("hace 31 h"),
                ),
            ],
            10,
        )
    }

    /// Pinta el diálogo en una terminal de `ancho`×`alto` y devuelve el
    /// texto y la disposición que registró.
    fn pintar_en(
        ancho: u16,
        alto: u16,
        deliberacion: &DeliberacionAbierta,
        tema: &Tema,
    ) -> (String, Disposicion) {
        let mut terminal = Terminal::new(TestBackend::new(ancho, alto)).unwrap();
        let mut disp = Disposicion::default();
        terminal
            .draw(|marco| {
                let area = marco.area();
                disp = Disposicion::nueva(
                    area,
                    disposicion::minimo_de(crate::ui::Vista::Snippets, true),
                );
                dibujar_con_tema(marco, area, tema, deliberacion, &mut disp);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let texto = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (texto, disp)
    }

    fn tema(ascii: bool) -> Tema {
        let mut tema = Tema::respaldo();
        if ascii {
            tema.ascii = true;
            tema.glifos = crate::tema::Glifos::ascii();
        }
        tema
    }

    /// El diálogo completo (110×30: ni estrecho ni bajo).
    fn pintar(deliberacion: &DeliberacionAbierta, ascii: bool) -> String {
        pintar_en(110, 30, deliberacion, &tema(ascii)).0
    }

    /// La maqueta §6.5: tres hosts, uno con el backup viejo.
    fn maqueta() -> DeliberacionAbierta {
        let filas = vec![
            ComprobacionesHost {
                host_id: 1,
                host: "hetzner-01".to_string(),
                salud: aprueba("0,2 s"),
                backup: aprueba("hace 3 h"),
                tests: aprueba("verde"),
            },
            ComprobacionesHost {
                host_id: 2,
                host: "hetzner-02".to_string(),
                salud: aprueba("0,1 s"),
                backup: rechaza("hace 31 h"),
                tests: aprueba("verde"),
            },
            ComprobacionesHost {
                host_id: 3,
                host: "vps-openclaw".to_string(),
                salud: aprueba("0,4 s"),
                backup: Veredicto::NoActiva,
                tests: Veredicto::NoActiva,
            },
        ];
        DeliberacionAbierta::de_prueba(
            plan(&[(1, "hetzner-01"), (2, "hetzner-02"), (3, "vps-openclaw")]),
            filas,
            10,
        )
    }

    fn tecla(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn bloqueada_muestra_la_tabla_el_consenso_y_el_forzado() {
        let deliberacion = bloqueada();
        assert_eq!(deliberacion.maquina.fase, Fase::Bloqueada);
        let pantalla = pintar(&deliberacion, false);
        for esperado in [
            "DELIBERACIÓN MAGI",
            "ACCIÓN: reiniciar nginx → 2 hosts",
            "requerida por: crítico",
            "MELCHIOR-1",
            "BALTHASAR-2",
            "CASPER-3",
            "backup<24 h",
            "✕ hace 31 h",
            "✓ hace 3 h",
            "— ",
            "CONSENSO  3 / 4",
            "▸ BLOQUEADO — se requiere unanimidad",
            "f forzar (registra motivo)",
        ] {
            assert!(
                pantalla.contains(esperado),
                "falta «{esperado}» en\n{pantalla}"
            );
        }
        assert!(!pantalla.contains("ctrl+k ejecutar"));
    }

    #[test]
    fn un_motivo_corto_con_ctrl_k_no_ejecuta_y_enseña_el_minimo() {
        let mut deliberacion = bloqueada();
        let maquina = &mut deliberacion.maquina;
        assert_eq!(
            maquina.pulsar(&tecla(KeyCode::Char('f'))),
            AccionMaquina::Nada
        );
        for c in "ok".chars() {
            maquina.pulsar(&tecla(KeyCode::Char(c)));
        }
        let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(maquina.pulsar(&ctrl_k), AccionMaquina::Nada);
        assert_eq!(maquina.pulsar(&tecla(KeyCode::Enter)), AccionMaquina::Nada);
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("Motivo del forzado (mín. 10 caracteres):"));
        assert!(
            pantalla.contains("mínimo 10 caracteres (2/10)"),
            "{pantalla}"
        );
        assert!(!pantalla.contains("FORZADO"));
    }

    #[test]
    fn con_motivo_suficiente_queda_forzado() {
        let mut deliberacion = bloqueada();
        let maquina = &mut deliberacion.maquina;
        maquina.pulsar(&tecla(KeyCode::Char('f')));
        for c in "backup revisado a mano".chars() {
            maquina.pulsar(&tecla(KeyCode::Char(c)));
        }
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("▸ FORZADO — quedará registrado con usuario y hora"));
        let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(
            deliberacion.maquina.pulsar(&ctrl_k),
            AccionMaquina::Ejecutar {
                forzada: true,
                motivo: Some("backup revisado a mano".to_string()),
            }
        );
    }

    #[test]
    fn sin_comprobaciones_activas_se_aprueba_y_sigue_pidiendo_ctrl_k() {
        let deliberacion = DeliberacionAbierta::de_prueba(
            plan(&[(1, "vps-openclaw")]),
            vec![fila(
                1,
                "vps-openclaw",
                Veredicto::NoActiva,
                Veredicto::NoActiva,
            )],
            10,
        );
        assert_eq!(deliberacion.maquina.fase, Fase::Aprobada);
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("ACCIÓN: reiniciar nginx → vps-openclaw"));
        assert!(pantalla.contains("sin comprobaciones configuradas"));
        assert!(pantalla.contains("▸ APROBADO — ctrl+k ejecutar · esc cancelar"));
        assert!(pantalla.contains("CONSENSO  0 / 0"));
    }

    #[test]
    fn comprobando_enseña_el_glifo_de_espera_y_en_ascii_otro() {
        let deliberacion = DeliberacionAbierta::de_prueba(
            plan(&[(1, "hetzner-01")]),
            vec![fila(
                1,
                "hetzner-01",
                Veredicto::Pendiente,
                rechaza("hace 31 h"),
            )],
            10,
        );
        assert_eq!(deliberacion.maquina.fase, Fase::Comprobando);
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("◐ comprobando"));
        assert!(pantalla.contains("DELIBERANDO"));
        let ascii = pintar(&deliberacion, true);
        assert!(ascii.contains("o comprobando"));
        assert!(ascii.contains("x hace 31 h"));
        assert!(!ascii.contains('◐'));
    }

    #[test]
    fn un_nombre_con_escapes_no_llega_a_la_terminal() {
        let deliberacion = DeliberacionAbierta::de_prueba(
            plan(&[(1, "malo\u{1b}[2J")]),
            vec![fila(
                1,
                "malo\u{1b}[2J",
                aprueba("NOMINAL"),
                Veredicto::NoActiva,
            )],
            10,
        );
        let pantalla = pintar(&deliberacion, false);
        assert!(!pantalla.contains('\u{1b}'));
        assert!(pantalla.contains("malo\u{FFFD}[2J"));
    }

    #[test]
    fn muchos_hosts_se_recorren_con_flechas() {
        let hosts: Vec<(i64, String)> = (1..=20).map(|i| (i, format!("host-{i:02}"))).collect();
        let referencias: Vec<(i64, &str)> = hosts.iter().map(|(i, n)| (*i, n.as_str())).collect();
        let filas = hosts
            .iter()
            .map(|(i, n)| fila(*i, n, aprueba("NOMINAL"), Veredicto::NoActiva))
            .collect();
        let mut deliberacion = DeliberacionAbierta::de_prueba(plan(&referencias), filas, 10);
        deliberacion.seleccion = 15;
        let pantalla = pintar(&deliberacion, false);
        assert!(pantalla.contains("host-16"));
        assert!(!pantalla.contains("host-01 "));
        assert!(pantalla.contains("recorrer hosts (16 de 20)"));
    }

    #[test]
    fn compacto_como_la_maqueta_a_70x18() {
        let deliberacion = maqueta();
        assert_eq!(deliberacion.maquina.fase, Fase::Bloqueada);
        let (pantalla, disp) = pintar_en(70, 18, &deliberacion, &tema(false));
        for esperado in [
            "DELIBERACIÓN MAGI ─ reiniciar nginx → 3",
            "M-1 salud",
            "B-2 backup",
            "C-3 tests",
            "hetzner-01    ✓ 0,2 s     ✓ 3 h",
            "hetzner-02    ✓ 0,1 s     ✕ 31 h",
            "vps-openclaw  ✓ 0,4 s     —           —",
            "CONSENSO 6/7 ████████░░  ▸ BLOQUEADO",
            "f forzar  esc cancelar",
            "↑↓ 1/3",
        ] {
            assert!(
                pantalla.contains(esperado),
                "falta «{esperado}» en\n{pantalla}"
            );
        }
        // Los nombres largos no aparecen: van abreviados.
        assert!(!pantalla.contains("MELCHIOR"), "{pantalla}");
        let ventana = disp.lista(Lista::DeliberacionHosts).unwrap();
        assert_eq!((ventana.inicio, ventana.filas, ventana.total), (0, 3, 3));
    }

    #[test]
    fn compacto_con_muchos_hosts_se_desplaza_y_sigue_a_la_seleccion() {
        let hosts: Vec<(i64, String)> = (1..=20).map(|i| (i, format!("host-{i:02}"))).collect();
        let referencias: Vec<(i64, &str)> = hosts.iter().map(|(i, n)| (*i, n.as_str())).collect();
        let filas = hosts
            .iter()
            .map(|(i, n)| fila(*i, n, aprueba("NOMINAL · hace 1 min"), Veredicto::NoActiva))
            .collect();
        let mut deliberacion = DeliberacionAbierta::de_prueba(plan(&referencias), filas, 10);
        deliberacion.seleccion = 15;
        let (pantalla, disp) = pintar_en(50, 12, &deliberacion, &tema(false));
        let ventana = disp.lista(Lista::DeliberacionHosts).unwrap();
        assert!(ventana.inicio <= 15 && 15 < ventana.inicio + ventana.filas);
        assert!(ventana.inicio + ventana.filas <= ventana.total);
        assert!(pantalla.contains("host-16"), "{pantalla}");
        assert!(pantalla.contains("16/20"), "{pantalla}");
        assert!(pantalla.contains("✓ NOMINAL"), "{pantalla}");
        // El desplazamiento guardado es el ancla: con la selección dentro de
        // la ventana, no se mueve.
        deliberacion.desplazamiento = ventana.inicio;
        deliberacion.seleccion = ventana.inicio;
        let (_, disp) = pintar_en(50, 12, &deliberacion, &tema(false));
        assert_eq!(
            disp.lista(Lista::DeliberacionHosts).unwrap().inicio,
            ventana.inicio
        );
    }

    #[test]
    fn compacto_con_el_motivo_y_en_ascii() {
        let mut deliberacion = maqueta();
        deliberacion.maquina.pulsar(&tecla(KeyCode::Char('f')));
        for c in "ok".chars() {
            deliberacion.maquina.pulsar(&tecla(KeyCode::Char(c)));
        }
        let (pantalla, _) = pintar_en(70, 18, &deliberacion, &tema(false));
        assert!(pantalla.contains("motivo [ ok┃"), "{pantalla}");
        assert!(pantalla.contains("mín. 10 (2/10)"), "{pantalla}");
        assert!(
            pantalla.contains("ctrl+k ejecutar  esc volver"),
            "{pantalla}"
        );
        for (ancho, alto) in [(70, 18), (50, 12), (99, 30), (120, 19)] {
            let (pantalla, _) = pintar_en(ancho, alto, &deliberacion, &tema(true));
            assert!(
                pantalla.chars().all(|c| c.is_ascii() || c.is_alphabetic()),
                "glifo Unicode a {ancho}×{alto}:\n{pantalla}"
            );
        }
        let (pantalla, _) = pintar_en(70, 18, &deliberacion, &tema(true));
        assert!(pantalla.contains("motivo [ ok|"), "{pantalla}");
        assert!(
            pantalla.contains("DELIBERACIÓN MAGI - reiniciar nginx -> 3"),
            "{pantalla}"
        );
    }

    #[test]
    fn completo_en_ascii_sin_glifos_unicode() {
        for deliberacion in [bloqueada(), maqueta()] {
            let pantalla = pintar(&deliberacion, true);
            assert!(
                pantalla.chars().all(|c| c.is_ascii() || c.is_alphabetic()),
                "{pantalla}"
            );
            assert!(
                pantalla.contains("> BLOQUEADO - se requiere unanimidad"),
                "{pantalla}"
            );
        }
    }

    /// Al mínimo del diálogo (50×12) los nombres abreviados se ven enteros:
    /// lo que se recorta es el nombre del host.
    #[test]
    fn compacto_al_minimo_no_corta_los_nombres_magi() {
        for tema in [tema(false), tema(true)] {
            let (pantalla, _) = pintar_en(50, 12, &maqueta(), &tema);
            for esperado in ["M-1 salud", "B-2 backup", "C-3 tests", "hetzner-01"] {
                assert!(
                    pantalla.contains(esperado),
                    "falta «{esperado}» en\n{pantalla}"
                );
            }
            let marca = if tema.ascii { "~" } else { "…" };
            assert!(
                pantalla.contains(&format!("vps-openc{marca}")),
                "{pantalla}"
            );
        }
    }

    #[test]
    fn el_recorte_no_deja_un_espacio_antes_de_la_marca() {
        assert_eq!(
            recortar("último backup hace 31 h (pg.sql.gz)", 23, false),
            "último backup hace 31…"
        );
        assert_eq!(recortar("hace 31 h (x)", 9, true), "hace 31~");
        // Sin recorte, igual; un texto que ya acaba en «…» no se toca.
        assert_eq!(recortar("verde ", 10, false), "verde ");
        assert_eq!(recortar("espera…", 10, false), "espera…");
    }

    #[test]
    fn nombres_y_datos_abreviados() {
        assert_eq!(abreviar_magi("MELCHIOR-1"), "M-1");
        assert_eq!(abreviar_magi("BALTHASAR-2"), "B-2");
        assert_eq!(cabecera_compacta(Comprobacion::Tests), "C-3 tests");
        assert_eq!(dato_compacto(&aprueba("NOMINAL · hace 2 min")), "NOMINAL");
        assert_eq!(dato_compacto(&aprueba("hace 3 h (db.sql.gz)")), "3 h");
        assert_eq!(
            dato_compacto(&rechaza("último backup hace 31 h (db.sql.gz)")),
            "31 h"
        );
        assert_eq!(dato_compacto(&Veredicto::NoActiva), "");
        assert_eq!(dato_compacto(&Veredicto::Pendiente), "comprobando");
    }
}
