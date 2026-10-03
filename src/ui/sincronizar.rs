//! Diálogo SINCRONIZAR (Fase 8, §6.2) y vista previa del plan (§6.3), con los
//! signos `+ ~ − ·`. La lista de guardadas (§6.5) vive en
//! `ui::sincronizaciones`. Mínimos y modos estrechos en `disposicion`
//! (`MINIMO_SINCRONIZAR`, `MINIMO_VISTA_PREVIA`, `MINIMO_SINCRONIZACIONES`).
//!
//! El diálogo es una hoja de formulario (sigue al campo con foco). La vista
//! previa es un diálogo grande con su lista del plan, que se desplaza con
//! `↑` `↓` `PgUp` `PgDn` y se registra como `Lista::VistaPrevia`; por debajo
//! de `ESTRECHO_VISTA_PREVIA` columnas pierde la columna de notas y el resumen
//! se abrevia. Todo se deriva del área en cada pintado.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::sincronizar::{
    DialogoSincronizar, FiltroPlan, FocoSincronizar, FormularioSincronizar, VistaPrevia,
};
use crate::app::App;
use crate::archivos::marcas::TOLERANCIA_MTIME;
use crate::archivos::plan::{Cambio, TipoCambio};
use crate::archivos::tamano_legible;
use crate::tema::Tema;
use crate::ui::centrar;
use crate::ui::componentes::casilla;
use crate::ui::dialogos::{atajo, indicador, linea_ascii, Hoja};
use crate::ui::disposicion::{
    self, Atajo, Disposicion, Lista, Minimo, VentanaLista, ESTRECHO_SINCRONIZAR,
    ESTRECHO_VISTA_PREVIA, MINIMO_SINCRONIZAR, MINIMO_VISTA_PREVIA,
};
use crate::ui::ejecutar::pie_por_prioridad;

/// Ancho deseado del diálogo SINCRONIZAR y de la vista previa.
const ANCHO_FORMULARIO: u16 = 62;
const ANCHO_VISTA_PREVIA: u16 = 100;
/// Columna del tamaño en la lista del plan.
const ANCHO_TAMANO: usize = 8;
/// La ruta se queda al menos con esta fracción de la fila (o con lo que mida
/// la más larga, si es menos); las notas se llevan el resto que necesiten.
const PARTE_RUTA: usize = 2;
/// Patrones de `[archivos] excluir` que se nombran en el rótulo.
const PATRONES_EN_ROTULO: usize = 3;

pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoSincronizar,
    disp: &mut Disposicion,
) {
    match dialogo {
        DialogoSincronizar::Formulario(formulario) => {
            dibujar_formulario(marco, area, app, formulario, disp)
        }
        DialogoSincronizar::VistaPrevia(vista) => {
            dibujar_vista_previa(marco, area, app, vista, disp)
        }
    }
}

/// Mínimo que declara el diálogo abierto (T45).
pub fn minimo(dialogo: &DialogoSincronizar) -> Minimo {
    match dialogo {
        DialogoSincronizar::Formulario(_) => Minimo {
            tamano: MINIMO_SINCRONIZAR,
            exige: "diálogo SINCRONIZAR",
        },
        DialogoSincronizar::VistaPrevia(_) => Minimo {
            tamano: MINIMO_VISTA_PREVIA,
            exige: "vista previa",
        },
    }
}

/// Teclas del pie, como en los demás diálogos: `tecla texto` separadas por
/// tres espacios.
fn teclas(pares: &[(&str, &str)], tema: &Tema) -> Line<'static> {
    let mut spans = Vec::new();
    for (indice, (tecla, texto)) in pares.iter().enumerate() {
        if indice > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(atajo(tecla, tema));
        spans.push(Span::raw(format!(" {texto}")));
    }
    Line::from(spans)
}

// ---------------------------------------------------------------- diálogo

/// Estilo de una casilla o campo: ámbar y en negrita con el foco.
fn estilo_foco(foco: bool, tema: &Tema) -> Style {
    if foco {
        Style::default()
            .fg(tema.paleta.acento)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tema.paleta.texto)
    }
}

/// `[ texto┃ ]` de un campo, con los corchetes en ámbar si tiene el foco.
fn campo(
    campo: &crate::ui::componentes::CampoTexto,
    foco: bool,
    tema: &Tema,
) -> Vec<Span<'static>> {
    let corchetes = Style::default().fg(if foco {
        tema.paleta.acento
    } else {
        tema.paleta.inactivo
    });
    vec![
        Span::styled("[ ", corchetes),
        campo.span(foco, Style::default().fg(tema.paleta.texto)),
        Span::styled(" ]", corchetes),
    ]
}

/// Rótulo de las extras: «excluir (además de .git/ target/ node_modules/ …)».
fn rotulo_excluir(por_defecto: &[String], estrecho: bool) -> String {
    if estrecho || por_defecto.is_empty() {
        return if por_defecto.is_empty() {
            "excluir".to_string()
        } else {
            "excluir además:".to_string()
        };
    }
    let mut nombrados: Vec<&str> = por_defecto
        .iter()
        .take(PATRONES_EN_ROTULO)
        .map(String::as_str)
        .collect();
    if por_defecto.len() > PATRONES_EN_ROTULO {
        nombrados.push("…");
    }
    format!("excluir (además de {})", nombrados.join(" "))
}

fn dibujar_formulario(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    formulario: &FormularioSincronizar,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let estrecho = area.width < ESTRECHO_SINCRONIZAR;
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let texto = Style::default().fg(tema.paleta.texto);
    let foco = formulario.foco;
    let etiqueta = |larga: &'static str, corta: &'static str| {
        Span::styled(if estrecho { corta } else { larga }, tenue)
    };
    let mut cuerpo = vec![
        Line::from(vec![
            etiqueta("origen   ", "de "),
            Span::styled(formulario.origen.clone(), texto),
        ]),
        Line::from(vec![
            etiqueta("destino  ", "a  "),
            Span::styled(formulario.destino.clone(), texto),
        ]),
        Line::from(Span::styled(
            format!(
                "{} {}",
                casilla(formulario.borrar),
                if estrecho {
                    "borrar lo que sobra en destino"
                } else {
                    "borrar en destino lo que no está en origen"
                }
            ),
            estilo_foco(foco == FocoSincronizar::Borrar, tema),
        )),
        Line::from(Span::styled(
            rotulo_excluir(&app.config.archivos.excluir, estrecho),
            tenue,
        )),
        Line::from(campo(
            &formulario.extras,
            foco == FocoSincronizar::Extras,
            tema,
        )),
        Line::from(Span::styled(formulario.magiignore.texto(), tenue)),
    ];
    let mut guardar = vec![Span::styled(
        format!(
            "{} {}",
            casilla(formulario.guardar),
            if estrecho { "guardar" } else { "guardar como" }
        ),
        estilo_foco(foco == FocoSincronizar::Guardar, tema),
    )];
    if formulario.guardar {
        guardar.push(Span::raw("  "));
        guardar.extend(campo(
            &formulario.nombre,
            foco == FocoSincronizar::Nombre,
            tema,
        ));
    }
    cuerpo.push(Line::from(guardar));
    if let Some(error) = &formulario.error {
        cuerpo.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    // Con un error se sigue al error, la última línea: el campo que lo
    // provocó (las extras o el nombre) queda a unas líneas y también se ve.
    let linea_foco = match (foco, &formulario.error) {
        (_, Some(_)) => 7,
        (FocoSincronizar::Borrar, None) => 2,
        (FocoSincronizar::Extras, None) => 4,
        (FocoSincronizar::Guardar | FocoSincronizar::Nombre, None) => 6,
    };
    let pie = vec![teclas(
        &[
            (tema.glifos.intro, "planificar"),
            (tema.glifos.tab, "campo"),
            ("espacio", "marcar"),
            ("esc", "cancelar"),
        ],
        tema,
    )];
    let titulo = format!(
        "SINCRONIZAR {} {}",
        tema.glifos.punto_medio,
        formulario.direccion.texto()
    );
    Hoja::nueva(titulo, cuerpo, pie)
        .foco(linea_foco)
        .ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO_FORMULARIO, tema, disp);
}

// ---------------------------------------------------------------- vista previa

/// Color del signo de cada tipo de cambio: verde, ámbar, rojo y gris.
fn color(tipo: TipoCambio, tema: &Tema) -> Color {
    match tipo {
        TipoCambio::Crear => tema.paleta.correcto,
        TipoCambio::Actualizar => tema.paleta.acento,
        TipoCambio::Borrar => tema.paleta.critico,
        TipoCambio::Omitir => tema.paleta.inactivo,
    }
}

/// «hace 3 h», «hace 2 días»: antigüedad de una fecha del destino.
fn antiguedad(mtime: i64, ahora: i64) -> Option<String> {
    let segundos = ahora.checked_sub(mtime)?;
    if mtime <= 0 || segundos < 0 {
        return None;
    }
    Some(if segundos < 60 {
        format!("hace {segundos} s")
    } else if segundos < 3600 {
        format!("hace {} min", segundos / 60)
    } else if segundos < 86_400 {
        format!("hace {} h", segundos / 3600)
    } else if segundos < 2 * 86_400 {
        "hace 1 día".to_string()
    } else {
        format!("hace {} días", segundos / 86_400)
    })
}

/// Nota de una fila: lo que era el fichero que se actualiza, qué es lo que
/// se borra si es un directorio y por qué se omite lo omitido.
fn nota(cambio: &Cambio, ahora: i64) -> String {
    match cambio {
        Cambio::Actualizar {
            tamano,
            mtime,
            tamano_destino,
            mtime_destino,
            ..
        } => {
            let mut partes = Vec::new();
            if tamano != tamano_destino {
                partes.push(format!("era {}", tamano_legible(*tamano_destino)));
            }
            if (mtime - mtime_destino).abs() > TOLERANCIA_MTIME {
                partes.extend(antiguedad(*mtime_destino, ahora));
            }
            if partes.is_empty() {
                String::new()
            } else {
                format!("({})", partes.join(", "))
            }
        }
        Cambio::Borrar { es_dir: true, .. } => "(directorio vacío tras borrar)".to_string(),
        Cambio::Omitir { motivo, .. } => format!("{}: omitido", motivo.texto()),
        _ => String::new(),
    }
}

fn tamano_de(cambio: &Cambio) -> String {
    match cambio {
        Cambio::Crear { tamano, .. }
        | Cambio::Actualizar { tamano, .. }
        | Cambio::Borrar {
            tamano,
            es_dir: false,
            ..
        } => tamano_legible(*tamano),
        _ => String::new(),
    }
}

/// Resumen con los signos en su color: completo o, si no cabe en `ancho`,
/// abreviado (`+12 ~3 −2 · 4.2 MB · 37 excl. · 1 omit.`).
fn resumen(vista: &VistaPrevia, ancho: usize, tema: &Tema) -> Line<'static> {
    let ascii = tema.ascii;
    let datos = vista.plan.plan.resumen();
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let texto = Style::default().fg(tema.paleta.texto);
    let signo = |tipo: TipoCambio| {
        Style::default()
            .fg(color(tipo, tema))
            .add_modifier(Modifier::BOLD)
    };
    let omitidos = |corto: bool| match (datos.omitidos, corto) {
        (1, false) => "1 omitido".to_string(),
        (cuantos, false) => format!("{cuantos} omitidos"),
        (cuantos, true) => format!("{cuantos} omit."),
    };
    let completo = vec![
        Span::styled(
            format!("{} {} crear", TipoCambio::Crear.signo(ascii), datos.crear),
            signo(TipoCambio::Crear),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "{} {} actualizar",
                TipoCambio::Actualizar.signo(ascii),
                datos.actualizar
            ),
            signo(TipoCambio::Actualizar),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "{} {} borrar",
                TipoCambio::Borrar.signo(ascii),
                datos.borrar
            ),
            signo(TipoCambio::Borrar),
        ),
        Span::raw("  "),
        Span::styled(tamano_legible(datos.bytes), texto),
        Span::raw("  "),
        Span::styled(format!("{} excluidos", vista.plan.excluidos), tenue),
        Span::raw("  "),
        Span::styled(omitidos(false), tenue),
    ];
    let largo: usize = completo
        .iter()
        .map(|span| span.content.chars().count())
        .sum();
    if largo <= ancho {
        return Line::from(completo);
    }
    let punto = if ascii { "-" } else { "·" };
    Line::from(vec![
        Span::styled(
            format!("{}{}", TipoCambio::Crear.signo(ascii), datos.crear),
            signo(TipoCambio::Crear),
        ),
        Span::raw(" "),
        Span::styled(
            format!(
                "{}{}",
                TipoCambio::Actualizar.signo(ascii),
                datos.actualizar
            ),
            signo(TipoCambio::Actualizar),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{}{}", TipoCambio::Borrar.signo(ascii), datos.borrar),
            signo(TipoCambio::Borrar),
        ),
        Span::styled(format!(" {punto} {}", tamano_legible(datos.bytes)), texto),
        Span::styled(
            format!(
                " {punto} {} excl. {punto} {}",
                vista.plan.excluidos,
                omitidos(true)
            ),
            tenue,
        ),
    ])
}

/// Los avisos de §6.3: lo que borrará y si habrá deliberación; abreviados
/// si no caben en `ancho`.
fn avisos(vista: &VistaPrevia, ancho: usize, tema: &Tema) -> Option<Line<'static>> {
    let borrar = vista.plan.plan.resumen().borrar;
    let componer = |corto: bool| {
        let mut partes = Vec::new();
        if borrar > 0 {
            partes.push(match (corto, borrar) {
                (true, _) => format!("borrará {borrar}"),
                (false, 1) => "borrará 1 elemento en el destino".to_string(),
                (false, _) => format!("borrará {borrar} elementos en el destino"),
            });
        }
        if vista.requiere_deliberacion() {
            partes.push(if corto {
                "deliberación MAGI".to_string()
            } else {
                "requerirá deliberación MAGI".to_string()
            });
        }
        partes
    };
    let mut partes = componer(false);
    if partes.is_empty() {
        return None;
    }
    let largo = 2 + partes
        .iter()
        .map(|parte| parte.chars().count() + 3)
        .sum::<usize>();
    if largo > ancho {
        partes = componer(true);
    }
    let color = if borrar > 0 {
        tema.paleta.critico
    } else {
        tema.paleta.acento
    };
    Some(Line::from(Span::styled(
        format!(
            "{} {}",
            if tema.ascii { "!" } else { "⚠" },
            partes.join(&format!(" {} ", tema.glifos.punto_medio))
        ),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )))
}

/// Pie de la vista previa, por prioridad: lo que ejecuta y lo que cancela
/// antes que el filtro y las teclas de desplazamiento.
fn pie(vista: &VistaPrevia, ancho: usize, tema: &Tema) -> Line<'static> {
    let filtro = match vista.filtro {
        FiltroPlan::Todos => "filtrar".to_string(),
        otro => format!("filtro: {}", otro.texto()),
    };
    let desplazar = if tema.ascii {
        "^v PgUp PgDn"
    } else {
        "↑↓ PgUp PgDn"
    };
    let atajos = [
        Atajo::new(tema.glifos.intro, "continuar", 1),
        Atajo::new("f", filtro, 2),
        Atajo::new(desplazar, "", 3),
        Atajo::new("esc", "cancelar", 1),
    ];
    pie_por_prioridad(&atajos, ancho, tema)
}

/// Anchos de las columnas de la lista del plan en este pintado.
#[derive(Clone, Copy)]
struct Columnas {
    ruta: usize,
    /// Cero: sin columna de notas (modo estrecho).
    nota: usize,
    tamano: bool,
}

/// Una fila del plan con sus columnas: signo, ruta, tamaño y nota.
fn fila(
    cambio: &Cambio,
    seleccionada: bool,
    columnas: Columnas,
    ahora: i64,
    tema: &Tema,
) -> Line<'static> {
    let ascii = tema.ascii;
    let tipo = cambio.tipo();
    let mut ruta = cambio.ruta().to_string();
    if cambio.es_dir() {
        ruta.push('/');
    }
    let mut base = Style::default().fg(if tipo == TipoCambio::Omitir {
        tema.paleta.inactivo
    } else {
        tema.paleta.texto
    });
    let mut estilo_signo = Style::default()
        .fg(color(tipo, tema))
        .add_modifier(Modifier::BOLD);
    if seleccionada {
        base = base.add_modifier(Modifier::REVERSED);
        estilo_signo = estilo_signo.add_modifier(Modifier::REVERSED);
    }
    let mut spans = vec![
        Span::styled(tipo.signo(ascii).to_string(), estilo_signo),
        Span::styled(" ", base),
        Span::styled(
            disposicion::columna(&disposicion::adaptar(&ruta, ascii), columnas.ruta, ascii),
            base,
        ),
    ];
    if columnas.tamano {
        spans.push(Span::styled(
            format!(" {:>ANCHO_TAMANO$}", tamano_de(cambio)),
            base,
        ));
    }
    if columnas.nota > 0 {
        spans.push(Span::styled(
            format!(
                "  {}",
                disposicion::columna(
                    &disposicion::adaptar(&nota(cambio, ahora), ascii),
                    columnas.nota,
                    ascii
                )
            ),
            base,
        ));
    }
    Line::from(spans)
}

fn dibujar_vista_previa(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    vista: &VistaPrevia,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let estrecho = area.width < ESTRECHO_VISTA_PREVIA;
    let visibles = vista.visibles();
    let ancho_previsto = usize::from(centrar(area, ANCHO_VISTA_PREVIA, 3).width.saturating_sub(4));
    let aviso = avisos(vista, ancho_previsto, tema);
    // Bordes (2), resumen, dos separadores, aviso y teclas: lo demás, lista.
    let fijas = 2 + 1 + 2 + usize::from(aviso.is_some()) + 1;
    let ideal = u16::try_from(fijas + visibles.len().max(1)).unwrap_or(u16::MAX);
    let recta = centrar(area, ANCHO_VISTA_PREVIA, ideal);
    let peligro = vista.plan.plan.resumen().borrar > 0;
    let color_borde = if peligro {
        tema.paleta.critico
    } else {
        tema.paleta.acento
    };

    // Lo que cabe, por orden: teclas, resumen, lista (al menos dos filas),
    // aviso y separadores.
    let total = visibles.len();
    let disponible = usize::from(recta.height.saturating_sub(2));
    let con_aviso = aviso.is_some() && disponible >= 5;
    let base = usize::from(disponible >= 1) + usize::from(disponible >= 2) + usize::from(con_aviso);
    let con_separadores = disponible >= base + 2 + total.clamp(1, 2);
    let fijo = base + 2 * usize::from(con_separadores);
    let filas_lista = disponible.saturating_sub(fijo);

    let seleccion = vista.seleccion.min(total.saturating_sub(1));
    let inicio = disposicion::ventana(vista.desplazamiento, seleccion, filas_lista.max(1), total);
    disp.registrar(
        Lista::VistaPrevia,
        VentanaLista {
            inicio,
            filas: filas_lista.max(1),
            total,
        },
    );

    let titulo = format!(
        "SINCRONIZAR {} {} {} {} {} {}",
        tema.glifos.punto_medio,
        vista.plan.nombre(),
        tema.glifos.linea.repeat(2),
        vista.origen,
        if ascii { "->" } else { "→" },
        vista.destino
    );
    let titulo = disposicion::recortar(
        &disposicion::adaptar(&titulo, ascii),
        usize::from(recta.width.saturating_sub(4)),
        ascii,
    );
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(Style::default().fg(color_borde))
        .title(Span::styled(
            format!(" {titulo} "),
            Style::default()
                .fg(color_borde)
                .add_modifier(Modifier::BOLD),
        ));
    if total > filas_lista && filas_lista > 0 {
        bloque = bloque.title_bottom(
            Line::from(Span::styled(
                indicador(inicio, filas_lista, total, ascii),
                Style::default().fg(color_borde),
            ))
            .right_aligned(),
        );
    }
    let interior = bloque.inner(recta);
    marco.render_widget(Clear, recta);
    marco.render_widget(bloque, recta);
    let zona = Rect {
        x: interior.x + 1.min(interior.width),
        width: interior.width.saturating_sub(2),
        ..interior
    };
    let ancho = usize::from(zona.width);
    if ancho == 0 || zona.height == 0 {
        return;
    }

    // Columnas de la lista: la nota solo fuera del modo estrecho; la ruta
    // conserva lo que mide la más larga (hasta la mitad de la fila) y la nota
    // se lleva lo que necesite del resto; el tamaño, mientras quepa.
    let ahora = crate::modelo::fecha_ahora_epoca();
    let largo_ruta = visibles
        .iter()
        .map(|indice| {
            let cambio = &vista.plan.plan.cambios[*indice];
            cambio.ruta().chars().count() + usize::from(cambio.es_dir())
        })
        .max()
        .unwrap_or(0);
    let largo_nota = visibles
        .iter()
        .map(|indice| {
            nota(&vista.plan.plan.cambios[*indice], ahora)
                .chars()
                .count()
        })
        .max()
        .unwrap_or(0);
    let libre = ancho.saturating_sub(2 + ANCHO_TAMANO + 1 + 2);
    let ancho_nota = if estrecho {
        0
    } else {
        largo_nota.min(libre.saturating_sub(largo_ruta.min(ancho / PARTE_RUTA)))
    };
    let fijo_fila = 2 + if ancho_nota > 0 { ancho_nota + 2 } else { 0 };
    let con_tamano = ancho >= fijo_fila + ANCHO_TAMANO + 1 + 12;
    let columnas = Columnas {
        ruta: ancho.saturating_sub(fijo_fila + if con_tamano { ANCHO_TAMANO + 1 } else { 0 }),
        nota: ancho_nota,
        tamano: con_tamano,
    };

    let separador = || {
        Line::from(Span::styled(
            tema.glifos.linea.repeat(ancho),
            Style::default().fg(tema.paleta.inactivo),
        ))
    };
    let mut lineas: Vec<Line<'static>> = Vec::new();
    if disponible >= 2 {
        lineas.push(resumen(vista, ancho, tema));
    }
    if con_separadores {
        lineas.push(separador());
    }
    if total == 0 {
        if filas_lista > 0 {
            lineas.push(Line::from(Span::styled(
                format!("nada que {} con este filtro", vista.filtro.texto()),
                Style::default().fg(tema.paleta.inactivo),
            )));
            lineas.extend((1..filas_lista).map(|_| Line::from("")));
        }
    } else {
        for (posicion, indice) in visibles.iter().enumerate().skip(inicio).take(filas_lista) {
            let cambio = &vista.plan.plan.cambios[*indice];
            lineas.push(fila(cambio, posicion == seleccion, columnas, ahora, tema));
        }
        let pintadas = (inicio + filas_lista).min(total) - inicio;
        lineas.extend((pintadas..filas_lista).map(|_| Line::from("")));
    }
    if con_separadores {
        lineas.push(separador());
    }
    if con_aviso {
        if let Some(aviso) = aviso {
            lineas.push(aviso);
        }
    }
    if disponible >= 1 {
        lineas.push(pie(vista, ancho, tema));
    }
    let lineas: Vec<Line<'static>> = if ascii {
        lineas.into_iter().map(linea_ascii).collect()
    } else {
        lineas
    };
    let lineas: Vec<Line<'static>> = lineas
        .into_iter()
        .map(|linea| recortar_linea(linea, ancho, ascii))
        .collect();
    marco.render_widget(Paragraph::new(lineas), zona);
}

/// Recorta una línea con estilos a `ancho` columnas, con la marca de corte.
fn recortar_linea(linea: Line<'static>, ancho: usize, ascii: bool) -> Line<'static> {
    let largo: usize = linea
        .spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum();
    if largo <= ancho {
        return linea;
    }
    let mut quedan = ancho;
    let mut spans = Vec::new();
    for span in linea.spans {
        if quedan == 0 {
            break;
        }
        let cuenta = span.content.chars().count();
        if cuenta < quedan {
            quedan -= cuenta;
            spans.push(span);
        } else {
            spans.push(Span::styled(
                disposicion::recortar(&span.content, quedan, ascii),
                span.style,
            ));
            quedan = 0;
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_antiguedad_se_dice_en_la_unidad_que_toca() {
        let ahora = 1_800_000_000;
        assert_eq!(antiguedad(ahora - 30, ahora).as_deref(), Some("hace 30 s"));
        assert_eq!(
            antiguedad(ahora - 600, ahora).as_deref(),
            Some("hace 10 min")
        );
        assert_eq!(
            antiguedad(ahora - 3 * 3600, ahora).as_deref(),
            Some("hace 3 h")
        );
        assert_eq!(
            antiguedad(ahora - 86_400 - 5, ahora).as_deref(),
            Some("hace 1 día")
        );
        assert_eq!(
            antiguedad(ahora - 2 * 86_400 - 5, ahora).as_deref(),
            Some("hace 2 días")
        );
        assert_eq!(antiguedad(ahora + 10, ahora), None, "fecha futura");
        assert_eq!(antiguedad(0, ahora), None, "fecha desconocida");
    }

    #[test]
    fn la_nota_de_actualizar_dice_lo_que_era() {
        let ahora = 1_800_000_000;
        let destino = ahora - 2 * 86_400 - 10;
        let actualizar = |tamano, mtime, tamano_destino| Cambio::Actualizar {
            ruta: "main.py".to_string(),
            tamano,
            mtime,
            permisos: None,
            tamano_destino,
            mtime_destino: destino,
        };
        assert_eq!(
            nota(&actualizar(14_336, ahora, 12_288), ahora),
            "(era 12 kB, hace 2 días)"
        );
        assert_eq!(nota(&actualizar(10, ahora, 10), ahora), "(hace 2 días)");
        assert_eq!(
            nota(&actualizar(14_336, destino + 1, 12_288), ahora),
            "(era 12 kB)",
            "con la misma fecha solo cuenta el tamaño"
        );
    }

    #[test]
    fn recortar_linea_conserva_estilos_y_marca_el_corte() {
        let linea = Line::from(vec![Span::raw("abc"), Span::raw("defgh")]);
        let recortada = recortar_linea(linea, 6, true);
        let texto: String = recortada
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(texto, "abcde~");
    }
}
