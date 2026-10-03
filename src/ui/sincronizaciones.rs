//! Lista de sincronizaciones guardadas del host (`L`, Fase 8, §6.5): nombre,
//! dirección con `−` si borra, rutas y último resultado, con detalle
//! inferior. Mínimo y modo estrecho en `disposicion::MINIMO_SINCRONIZACIONES`
//! / `ESTRECHO_SINCRONIZACIONES`.
//!
//! La lista es un diálogo grande sobre la vista: el recuadro se ajusta a las
//! filas y, si no cabe, pierde por este orden el detalle, la cabecera y el
//! pie; las filas se desplazan con la selección y su ventana se registra como
//! `Lista::Modal`. En modo estrecho cae la columna de rutas. Todo se deriva
//! del área en cada pintado.
//!
//! El formulario «SINCRONIZACIÓN GUARDADA» es una `Hoja` que sigue al foco,
//! con el error de la validación fijo en el pie.

use std::path::Path;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::guardadas::{CampoGuardada, DialogoGuardadas, FormularioGuardada, ListaGuardadas};
use crate::app::App;
use crate::modelo::{ResultadoSincronizacion, Sincronizacion};
use crate::protocolo::Direccion;
use crate::tema::Tema;
use crate::ui::componentes::{casilla, estilo_campo, CampoTexto};
use crate::ui::dialogos::{indicador, medidas, Hoja};
use crate::ui::disposicion::{
    self, Atajo, Columna, Disposicion, Lista, Minimo, VentanaLista, ANCHO_MAX_DIALOGO,
    ESTRECHO_SINCRONIZACIONES, MINIMO_SINCRONIZACIONES,
};
use crate::ui::ejecutar::pie_por_prioridad;
use crate::ui::hosts::{titulo_derecho, titulo_derecho_cabe};
use crate::ui::snippets::limpio;
use crate::ui::tuneles::{estilo_fila, prefijo, Tabla, ANCHO_PREFIJO, SEPARACION};
use crate::ui::{centrar, tecla};

/// Ancho que pide la lista como poco: caben el título con el host y el pie
/// entero.
const ANCHO_LISTA_MINIMO: u16 = 60;
/// Ancho del formulario.
const ANCHO_FORMULARIO: u16 = 72;
/// Ancho de las etiquetas del formulario.
const ETIQUETA: usize = 11;
/// Ancho máximo de la columna del nombre cuando no cabe todo.
const NOMBRE_MAXIMO: u16 = 16;
/// Ancho mínimo de la columna de rutas.
const RUTAS_MINIMO: u16 = 12;

pub fn dibujar(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoGuardadas,
    disp: &mut Disposicion,
) {
    match dialogo {
        DialogoGuardadas::Lista(lista) => dibujar_lista(marco, area, app, lista, disp),
        DialogoGuardadas::Formulario(formulario) => {
            dibujar_formulario(marco, area, app, formulario, disp);
        }
    }
}

/// Mínimo que declara el diálogo abierto (T45).
pub fn minimo(_dialogo: &DialogoGuardadas) -> Minimo {
    Minimo {
        tamano: MINIMO_SINCRONIZACIONES,
        exige: "SINCRONIZACIONES",
    }
}

// ---------------------------------------------------------------- lista

/// Qué partes fijas de la lista se pintan y cuántas filas quedan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reparto {
    cabecera: bool,
    /// Separador y línea de detalle.
    detalle: bool,
    /// Divisoria y teclas.
    pie: bool,
    filas: usize,
}

/// Reparte el alto interior: sin sitio para una fila se pierden, por este
/// orden, el detalle, la cabecera y el pie. Sin tabla (lista vacía) no hay
/// cabecera ni detalle.
fn reparto(alto: usize, con_tabla: bool) -> Reparto {
    let mut reparto = Reparto {
        cabecera: con_tabla,
        detalle: con_tabla,
        pie: true,
        filas: 0,
    };
    let fijo = |reparto: &Reparto| {
        usize::from(reparto.cabecera)
            + 2 * usize::from(reparto.detalle)
            + 2 * usize::from(reparto.pie)
    };
    if alto < fijo(&reparto) + 1 {
        reparto.detalle = false;
    }
    if alto < fijo(&reparto) + 1 {
        reparto.cabecera = false;
    }
    if alto < fijo(&reparto) + 1 {
        reparto.pie = false;
    }
    reparto.filas = alto.saturating_sub(fijo(&reparto));
    reparto
}

/// Lo que se pinta de una guardada, ya adaptado al tema.
struct Celdas {
    nombre: String,
    /// `↑` subida, `↓` bajada (`^` `v` en ASCII).
    direccion: &'static str,
    borra: bool,
    /// `local → remoto` (`local ← remoto` en una bajada).
    rutas: String,
    glifo: &'static str,
    color: Color,
    /// Fecha legible de la última ejecución; vacía si no se lanzó nunca.
    fecha: String,
}

impl Celdas {
    fn de(guardada: &Sincronizacion, hogar: &Path, ahora: i64, tema: &Tema) -> Self {
        let ascii = tema.ascii;
        let flecha = match guardada.direccion {
            Direccion::Subida => "→",
            Direccion::Bajada => "←",
        };
        let rutas = format!(
            "{} {flecha} {}",
            ruta_visible(&limpio(&guardada.ruta_local), hogar),
            limpio(&guardada.ruta_remota)
        );
        let fecha = guardada
            .ultima_ejecucion_en
            .as_deref()
            .map(|texto| fecha_ultima(texto, ahora))
            .unwrap_or_default();
        let (glifo, color) = match guardada.ultimo_resultado {
            Some(resultado) => (resultado.glifo(ascii), color_resultado(resultado, tema)),
            // Lanzada y sin resultado: en curso, o el servidor cayó antes de
            // escribirlo (R40).
            None if !fecha.is_empty() => (tema.glifos.punto_medio, tema.paleta.inactivo),
            None => (if ascii { "-" } else { "—" }, tema.paleta.inactivo),
        };
        Self {
            nombre: disposicion::adaptar(&limpio(&guardada.nombre), ascii),
            direccion: guardada.direccion.glifo(ascii),
            borra: guardada.borrar,
            rutas: disposicion::adaptar(&rutas, ascii),
            glifo,
            color,
            fecha: disposicion::adaptar(&fecha, ascii),
        }
    }

    /// Texto de la columna ÚLTIMA: el glifo y, si se lanzó, la fecha.
    fn ultima(&self) -> String {
        if self.fecha.is_empty() {
            self.glifo.to_string()
        } else {
            format!("{} {}", self.glifo, self.fecha)
        }
    }
}

fn color_resultado(resultado: ResultadoSincronizacion, tema: &Tema) -> Color {
    match resultado {
        ResultadoSincronizacion::Ok => tema.paleta.correcto,
        ResultadoSincronizacion::Parcial => tema.paleta.acento,
        ResultadoSincronizacion::Error => tema.paleta.critico,
        ResultadoSincronizacion::Cancelada => tema.paleta.inactivo,
    }
}

/// Fecha legible de la última ejecución (`hoy`, `ayer`, `2 sep`, `2025`) en
/// la zona local; el texto guardado es RFC 3339. Vacía si no se entiende.
fn fecha_ultima(texto: &str, ahora: i64) -> String {
    let Ok(fecha) = chrono::DateTime::parse_from_rfc3339(texto) else {
        return String::new();
    };
    let legible = crate::archivos::fecha_legible(fecha.timestamp(), ahora);
    // `02 sep` → `2 sep`, como en la maqueta.
    match legible.strip_prefix('0') {
        Some(resto) => resto.to_string(),
        None => legible,
    }
}

/// Una ruta local con el hogar abreviado a `~`.
fn ruta_visible(ruta: &str, hogar: &Path) -> String {
    if !hogar.is_absolute() {
        return ruta.to_string();
    }
    match Path::new(ruta).strip_prefix(hogar) {
        Ok(resto) if resto.as_os_str().is_empty() => "~".to_string(),
        Ok(resto) => format!("~/{}", resto.display()),
        Err(_) => ruta.to_string(),
    }
}

/// `web-prod · subida · sin borrar · excluye *.log`.
fn detalle(guardada: &Sincronizacion, tema: &Tema) -> String {
    let punto = tema.glifos.punto_medio;
    let mut partes = vec![
        limpio(&guardada.nombre),
        guardada.direccion.texto().to_string(),
        if guardada.borrar {
            "borra en destino lo que sobra".to_string()
        } else {
            "sin borrar".to_string()
        },
    ];
    if !guardada.exclusiones.is_empty() {
        partes.push(format!(
            "excluye {}",
            limpio(&guardada.exclusiones.join(" "))
        ));
    }
    disposicion::adaptar(&partes.join(&format!(" {punto} ")), tema.ascii)
}

/// Columnas de la tabla: nombre, dirección, rutas y última. Las rutas son
/// lo primero que cae y, en modo estrecho, no están; el nombre (acotado), la
/// dirección y la última no se ocultan nunca.
fn tabla(celdas: &[Celdas], disponible: u16, estrecho: bool, tema: &Tema) -> (Tabla, Vec<usize>) {
    let cabeceras = cabeceras(tema);
    let nombre = celdas
        .iter()
        .map(|celda| largo_acotado(&celda.nombre))
        .chain([largo_acotado(&cabeceras[0])])
        .max()
        .unwrap_or(6);
    let rutas = celdas
        .iter()
        .map(|celda| largo_acotado(&celda.rutas))
        .chain([largo_acotado(&cabeceras[2])])
        .max()
        .unwrap_or(RUTAS_MINIMO);
    let ultima = celdas
        .iter()
        .map(|celda| largo_acotado(&celda.ultima()))
        .chain([largo_acotado(&cabeceras[3])])
        .max()
        .unwrap_or(6);
    let indices: Vec<usize> = if estrecho {
        vec![0, 1, 3]
    } else {
        vec![0, 1, 2, 3]
    };
    let todas = [
        if estrecho {
            Columna::flexible(nombre.min(NOMBRE_MAXIMO), 1)
        } else {
            Columna::fija(nombre.min(NOMBRE_MAXIMO), 1)
        },
        Columna::fija(4, 1),
        Columna::flexible(RUTAS_MINIMO, 2),
        Columna::fija(ultima, 1),
    ];
    let naturales = [nombre, 4, rutas, ultima];
    let columnas: Vec<Columna> = indices.iter().map(|indice| todas[*indice]).collect();
    let anchos: Vec<u16> = indices.iter().map(|indice| naturales[*indice]).collect();
    (Tabla::nueva(disponible, &columnas, &anchos), indices)
}

/// Largo de un texto para medir columnas, acotado al ancho máximo de un
/// diálogo: una ruta enorme (la fila la escribe otro proceso) no desborda las
/// sumas de anchos.
fn largo_acotado(texto: &str) -> u16 {
    texto.chars().count().min(usize::from(ANCHO_MAX_DIALOGO)) as u16
}

fn cabeceras(tema: &Tema) -> [String; 4] {
    ["NOMBRE", "DIR.", "LOCAL → REMOTO", "ÚLTIMA"]
        .map(|texto| disposicion::adaptar(texto, tema.ascii))
}

/// Ancho natural de una fila (prefijo, columnas y separaciones).
fn ancho_natural(celdas: &[Celdas], tema: &Tema) -> u16 {
    let (tabla, _) = tabla(celdas, u16::MAX, false, tema);
    let columnas: u16 = tabla.anchos.iter().sum();
    ANCHO_PREFIJO + columnas + SEPARACION * (tabla.anchos.len() as u16).saturating_sub(1)
}

/// Una fila: el prefijo y las celdas visibles, cada una con sus partes
/// recortadas juntas a su ancho. `base` es el estilo de la fila.
fn linea_tabla(
    tabla: &Tabla,
    seleccionada: bool,
    celdas: Vec<Vec<(String, Style)>>,
    base: Style,
    tema: &Tema,
) -> Line<'static> {
    let mut spans = vec![prefijo(seleccionada, tema)];
    let mut primera = true;
    for (indice, partes) in celdas.into_iter().enumerate() {
        if !tabla.visible(indice) {
            continue;
        }
        if !primera {
            spans.push(Span::styled(" ".repeat(usize::from(SEPARACION)), base));
        }
        primera = false;
        let mut restante = usize::from(tabla.ancho(indice));
        for (texto, estilo) in partes {
            let trozo = disposicion::recortar(&texto, restante, tema.ascii);
            restante -= trozo.chars().count();
            if !trozo.is_empty() {
                spans.push(Span::styled(trozo, estilo));
            }
        }
        if restante > 0 {
            spans.push(Span::styled(" ".repeat(restante), base));
        }
    }
    Line::from(spans)
}

fn linea_fila(
    celdas: &Celdas,
    seleccionada: bool,
    tabla: &Tabla,
    indices: &[usize],
    tema: &Tema,
) -> Line<'static> {
    let base = estilo_fila(tema, seleccionada);
    // Sobre la fila seleccionada los colores son los de la selección.
    let color = |color: Color| {
        if seleccionada {
            base
        } else {
            Style::default().fg(color)
        }
    };
    let mut direccion = vec![(celdas.direccion.to_string(), base)];
    if celdas.borra {
        direccion.push((" ".to_string(), base));
        direccion.push((
            if tema.ascii { "-" } else { "−" }.to_string(),
            color(tema.paleta.critico),
        ));
    }
    let mut ultima = vec![(celdas.glifo.to_string(), color(celdas.color))];
    if !celdas.fecha.is_empty() {
        ultima.push((format!(" {}", celdas.fecha), base));
    }
    let todas = [
        vec![(celdas.nombre.clone(), base)],
        direccion,
        vec![(celdas.rutas.clone(), base)],
        ultima,
    ];
    let elegidas = indices
        .iter()
        .map(|indice| todas[*indice].clone())
        .collect();
    linea_tabla(tabla, seleccionada, elegidas, base, tema)
}

/// Título con el host y, si cabe, el total a la derecha; sin sitio, sin el
/// «MAGI ·» y después recortado.
fn titulo(lista: &ListaGuardadas, ancho: u16, tema: &Tema) -> (String, Option<String>) {
    let punto = tema.glifos.punto_medio;
    let host = limpio(&lista.host_nombre);
    let derecho = format!(" {} ", lista.filas.len());
    for candidato in [
        format!(" MAGI {punto} SINCRONIZACIONES {punto} {host} "),
        format!(" SINCRONIZACIONES {punto} {host} "),
    ] {
        if titulo_derecho_cabe(ancho, &candidato, &derecho) {
            return (candidato, Some(derecho));
        }
    }
    let corto = disposicion::recortar(
        &format!("SINCRONIZACIONES {punto} {host}"),
        usize::from(ancho.saturating_sub(4)),
        tema.ascii,
    );
    (format!(" {corto} "), None)
}

fn dibujar_lista(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    lista: &ListaGuardadas,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let ascii = tema.ascii;
    let estrecho = area.width < ESTRECHO_SINCRONIZACIONES;
    let ahora = crate::modelo::fecha_ahora_epoca();
    let celdas: Vec<Celdas> = lista
        .filas
        .iter()
        .map(|guardada| Celdas::de(guardada, &app.rutas.hogar, ahora, tema))
        .collect();
    let detalles: Vec<String> = lista
        .filas
        .iter()
        .map(|guardada| detalle(guardada, tema))
        .collect();
    let total = lista.filas.len();

    // Ancho: el natural de las filas y los detalles (bordes y márgenes), entre
    // el mínimo de la lista y el máximo de un diálogo.
    let natural = ancho_natural(&celdas, tema).max(
        detalles
            .iter()
            .map(|texto| largo_acotado(texto) + 3)
            .max()
            .unwrap_or(0),
    );
    let ancho = (natural + 3).clamp(ANCHO_LISTA_MINIMO, ANCHO_MAX_DIALOGO);
    // Alto: bordes, cabecera, filas, separador y detalle, divisoria y teclas
    // (vacía: bordes, el aviso, divisoria y teclas).
    let fijo = if total > 0 { 5 } else { 2 };
    let alto = u16::try_from(2 + fijo + total.max(1)).unwrap_or(u16::MAX);
    let recta = centrar(area, ancho, alto);

    let estilo_borde = Style::default().fg(tema.paleta.acento);
    let (texto_titulo, derecho) = titulo(lista, recta.width, tema);
    let mut bloque = Block::default()
        .borders(Borders::ALL)
        .border_set(tema.bordes())
        .border_style(estilo_borde)
        .title(Span::styled(
            texto_titulo,
            estilo_borde.add_modifier(Modifier::BOLD),
        ));
    if let Some(derecho) = derecho {
        bloque = bloque.title_top(titulo_derecho(derecho, tema));
    }
    let interior = bloque.inner(recta);
    let reparto = reparto(usize::from(interior.height), total > 0);
    let filas = reparto.filas.min(total.max(1));
    let seleccion = lista.seleccion.min(total.saturating_sub(1));
    let inicio = disposicion::ventana(app.desplazamiento_modal, seleccion, filas, total);
    disp.registrar(
        Lista::Modal,
        VentanaLista {
            inicio,
            filas,
            total,
        },
    );
    if total > filas && filas > 0 {
        bloque = bloque.title_bottom(
            Line::from(Span::styled(
                indicador(inicio, filas, total, ascii),
                estilo_borde,
            ))
            .right_aligned(),
        );
    }
    marco.render_widget(Clear, recta);
    marco.render_widget(bloque, recta);
    if interior.width == 0 || interior.height == 0 {
        return;
    }

    let ancho_filas = interior.width.saturating_sub(1);
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let mut lineas: Vec<Line<'static>> = Vec::new();
    if total == 0 {
        lineas.push(Line::from(Span::styled(
            format!(
                "  {}",
                disposicion::recortar(
                    "sin sincronizaciones guardadas: n para crear una",
                    usize::from(ancho_filas.saturating_sub(2)),
                    ascii,
                )
            ),
            tenue,
        )));
    } else {
        let (tabla, indices) = tabla(
            &celdas,
            ancho_filas.saturating_sub(ANCHO_PREFIJO),
            estrecho,
            tema,
        );
        if reparto.cabecera {
            let estilo = tenue.add_modifier(Modifier::BOLD);
            let todas = cabeceras(tema);
            let elegidas = indices
                .iter()
                .map(|indice| vec![(todas[*indice].clone(), estilo)])
                .collect();
            lineas.push(linea_tabla(&tabla, false, elegidas, estilo, tema));
        }
        for (posicion, celda) in celdas.iter().enumerate().skip(inicio).take(filas) {
            lineas.push(linea_fila(
                celda,
                posicion == seleccion,
                &tabla,
                &indices,
                tema,
            ));
        }
        if reparto.detalle {
            let ancho_detalle = usize::from(interior.width.saturating_sub(4));
            lineas.push(Line::from(Span::styled(
                format!("  {}", tema.glifos.linea.repeat(ancho_detalle)),
                tenue,
            )));
            lineas.push(Line::from(Span::styled(
                format!(
                    "  {}",
                    disposicion::recortar(&detalles[seleccion], ancho_detalle, ascii)
                ),
                Style::default().fg(tema.paleta.texto),
            )));
        }
    }
    let alto_pie = if reparto.pie { 2 } else { 0 };
    marco.render_widget(
        Paragraph::new(lineas),
        Rect {
            width: ancho_filas,
            height: interior.height.saturating_sub(alto_pie),
            ..interior
        },
    );
    if reparto.pie {
        // La divisoria cruza el recuadro de lado a lado (├───┤).
        let (izquierda, derecha) = if ascii { ("+", "+") } else { ("├", "┤") };
        let divisoria = format!(
            "{izquierda}{}{derecha}",
            tema.glifos
                .linea
                .repeat(usize::from(recta.width.saturating_sub(2)))
        );
        let y_pie = interior.y + interior.height - 1;
        marco.render_widget(
            Paragraph::new(Line::from(Span::styled(divisoria, estilo_borde))),
            Rect::new(recta.x, y_pie - 1, recta.width, 1),
        );
        let pie = pie_por_prioridad(
            &atajos_lista(total > 0, tema),
            usize::from(interior.width.saturating_sub(2)),
            tema,
        );
        let mut spans = vec![Span::raw(" ")];
        spans.extend(pie.spans);
        marco.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(interior.x, y_pie, interior.width, 1),
        );
    }
}

/// `↵ ejecutar  n nueva  e editar  x borrar  q volver`, por prioridad; con la
/// lista vacía solo `n` y `q`.
fn atajos_lista(con_filas: bool, tema: &Tema) -> Vec<Atajo> {
    if !con_filas {
        return vec![Atajo::new("n", "nueva", 1), Atajo::new("q", "volver", 1)];
    }
    vec![
        Atajo::new(tecla(tema, "↵", "enter"), "ejecutar", 1),
        Atajo::new("n", "nueva", 2),
        Atajo::new("e", "editar", 3),
        Atajo::new("x", "borrar", 3),
        Atajo::new("q", "volver", 1),
    ]
}

// ---------------------------------------------------------------- formulario

fn dibujar_formulario(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    formulario: &FormularioGuardada,
    disp: &mut Disposicion,
) {
    let tema = &app.tema;
    let (_, ancho_texto) = medidas(area, ANCHO_FORMULARIO);
    let (cuerpo, foco) = cuerpo_formulario(formulario, tema, &app.config.archivos.excluir);
    // El error va en el pie fijo: se ve aunque el cuerpo esté desplazado.
    let mut pie = Vec::new();
    if let Some(error) = &formulario.error {
        pie.push(Line::from(Span::styled(
            format!("{} {}", tema.glifos.error, limpio(error)),
            Style::default().fg(tema.paleta.critico),
        )));
    }
    pie.push(pie_por_prioridad(
        &[
            Atajo::new(tecla(tema, "^s/↵", "^s/enter"), "guardar", 1),
            Atajo::new(tecla(tema, "⇥", "tab"), "campo", 2),
            Atajo::new("espacio", "marcar", 3),
            Atajo::new("esc", "volver", 1),
        ],
        ancho_texto,
        tema,
    ));
    let punto = tema.glifos.punto_medio;
    let titulo = format!(
        "SINCRONIZACIÓN GUARDADA {punto} {}",
        if formulario.id.is_some() {
            "editar"
        } else {
            "nueva"
        }
    );
    Hoja::nueva(titulo, cuerpo, pie)
        .foco(foco)
        .ancla(app.desplazamiento_modal)
        .pintar(marco, area, ANCHO_FORMULARIO, tema, disp);
}

/// Cuerpo del formulario y la línea del campo con el foco.
fn cuerpo_formulario(
    formulario: &FormularioGuardada,
    tema: &Tema,
    por_defecto: &[String],
) -> (Vec<Line<'static>>, usize) {
    let activo = |campo: CampoGuardada| formulario.foco == campo;
    let estilo = |campo: CampoGuardada| estilo_campo(tema, activo(campo));
    let tenue = Style::default().fg(tema.paleta.inactivo);
    let etiqueta = |campo: Option<CampoGuardada>, texto: &str| {
        Span::styled(
            format!("{texto:<ETIQUETA$}"),
            if campo.is_some_and(activo) {
                Style::default().fg(tema.paleta.acento)
            } else {
                tenue
            },
        )
    };
    let corchete =
        |texto: &'static str| Span::styled(texto, Style::default().fg(tema.paleta.acento));
    let campo_texto = |campo: CampoGuardada, rotulo: &str, valor: &CampoTexto| {
        Line::from(vec![
            etiqueta(Some(campo), rotulo),
            corchete("[ "),
            valor.span(activo(campo), estilo(campo)),
            corchete(" ]"),
        ])
    };
    let sangria = || Span::raw(" ".repeat(ETIQUETA));
    let marca = |elegido: bool| if elegido { "(•)" } else { "( )" };
    let sentido = match formulario.direccion {
        Direccion::Subida => "local → remoto",
        Direccion::Bajada => "remoto → local",
    };
    let lineas: Vec<(Option<CampoGuardada>, Line<'static>)> = vec![
        (
            None,
            Line::from(vec![
                etiqueta(None, "host"),
                Span::styled(
                    limpio(&formulario.host_nombre),
                    Style::default().fg(tema.paleta.texto),
                ),
            ]),
        ),
        (
            Some(CampoGuardada::Nombre),
            campo_texto(CampoGuardada::Nombre, "nombre", &formulario.nombre),
        ),
        (
            Some(CampoGuardada::Local),
            campo_texto(CampoGuardada::Local, "local", &formulario.ruta_local),
        ),
        (
            Some(CampoGuardada::Remoto),
            campo_texto(CampoGuardada::Remoto, "remoto", &formulario.ruta_remota),
        ),
        (
            Some(CampoGuardada::Direccion),
            Line::from(vec![
                etiqueta(Some(CampoGuardada::Direccion), "dirección"),
                Span::styled(
                    format!(
                        "{} subida   {} bajada",
                        marca(formulario.direccion == Direccion::Subida),
                        marca(formulario.direccion == Direccion::Bajada),
                    ),
                    estilo(CampoGuardada::Direccion),
                ),
                Span::styled(format!("   {sentido}"), tenue),
            ]),
        ),
        (
            Some(CampoGuardada::Borrar),
            Line::from(vec![
                sangria(),
                Span::styled(
                    format!(
                        "{} borrar en destino lo que no está en origen",
                        casilla(formulario.borrar)
                    ),
                    estilo(CampoGuardada::Borrar),
                ),
            ]),
        ),
        (
            Some(CampoGuardada::Exclusiones),
            campo_texto(
                CampoGuardada::Exclusiones,
                "excluir",
                &formulario.exclusiones,
            ),
        ),
        (
            None,
            Line::from(vec![
                sangria(),
                Span::styled(
                    format!(
                        "separados por espacios; además de {}",
                        resumen_por_defecto(por_defecto)
                    ),
                    tenue,
                ),
            ]),
        ),
    ];
    let foco = lineas
        .iter()
        .position(|(campo, _)| *campo == Some(formulario.foco))
        .unwrap_or(0);
    (lineas.into_iter().map(|(_, linea)| linea).collect(), foco)
}

/// `.git/ target/ … y el .magiignore del origen`: los primeros patrones de
/// `[archivos] excluir`.
fn resumen_por_defecto(patrones: &[String]) -> String {
    let primeros: Vec<&str> = patrones.iter().take(2).map(String::as_str).collect();
    if primeros.is_empty() {
        return "el .magiignore del origen".to_string();
    }
    let mas = if patrones.len() > primeros.len() {
        " …"
    } else {
        ""
    };
    format!("{}{mas} y el .magiignore del origen", primeros.join(" "))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_reparto_pierde_detalle_cabecera_y_pie_por_ese_orden() {
        let completo = Reparto {
            cabecera: true,
            detalle: true,
            pie: true,
            filas: 3,
        };
        assert_eq!(reparto(8, true), completo);
        assert_eq!(
            reparto(5, true),
            Reparto {
                detalle: false,
                filas: 2,
                ..completo
            }
        );
        assert_eq!(
            reparto(3, true),
            Reparto {
                cabecera: false,
                detalle: false,
                filas: 1,
                ..completo
            }
        );
        assert_eq!(
            reparto(2, true),
            Reparto {
                cabecera: false,
                detalle: false,
                pie: false,
                filas: 2,
            }
        );
        assert_eq!(reparto(0, true).filas, 0);
        // Vacía: solo el aviso y el pie.
        assert_eq!(
            reparto(3, false),
            Reparto {
                cabecera: false,
                detalle: false,
                pie: true,
                filas: 1,
            }
        );
    }

    #[test]
    fn la_fecha_de_la_ultima_es_legible_y_sin_cero_delante() {
        use chrono::TimeZone as _;
        let ahora = chrono::Local
            .with_ymd_and_hms(2026, 10, 3, 12, 0, 0)
            .single()
            .expect("fecha de prueba");
        let texto = |fecha: chrono::DateTime<chrono::Local>| fecha.to_rfc3339();
        let epoca = ahora.timestamp();
        assert_eq!(fecha_ultima(&texto(ahora), epoca), "hoy");
        assert_eq!(
            fecha_ultima(&texto(ahora - chrono::Duration::days(1)), epoca),
            "ayer"
        );
        let dos_sep = chrono::Local
            .with_ymd_and_hms(2026, 9, 2, 10, 0, 0)
            .single()
            .expect("fecha de prueba");
        assert_eq!(fecha_ultima(&texto(dos_sep), epoca), "2 sep");
        let doce_sep = chrono::Local
            .with_ymd_and_hms(2026, 9, 12, 10, 0, 0)
            .single()
            .expect("fecha de prueba");
        assert_eq!(fecha_ultima(&texto(doce_sep), epoca), "12 sep");
        let otro_ano = chrono::Local
            .with_ymd_and_hms(2025, 9, 2, 10, 0, 0)
            .single()
            .expect("fecha de prueba");
        assert_eq!(fecha_ultima(&texto(otro_ano), epoca), "2025");
        assert_eq!(fecha_ultima("ayer", epoca), "");
    }

    #[test]
    fn el_hogar_se_abrevia_a_la_tilde() {
        let hogar = Path::new("/home/hector");
        assert_eq!(
            ruta_visible("/home/hector/proyectos/app", hogar),
            "~/proyectos/app"
        );
        assert_eq!(ruta_visible("/home/hector", hogar), "~");
        assert_eq!(ruta_visible("/home/hectorb/x", hogar), "/home/hectorb/x");
        assert_eq!(ruta_visible("~/web/static", hogar), "~/web/static");
        assert_eq!(ruta_visible("/srv/static", hogar), "/srv/static");
        assert_eq!(ruta_visible("/srv/static", Path::new("")), "/srv/static");
    }

    #[test]
    fn el_detalle_dice_direccion_borrado_y_exclusiones() {
        let tema = Tema::respaldo();
        let mut guardada = Sincronizacion {
            id: 1,
            host_id: 1,
            host_nombre: "hetzner-01".to_string(),
            nombre: "web-prod".to_string(),
            ruta_local: "/home/hector/proyectos/cooperapp".to_string(),
            ruta_remota: "/var/www/cooperapp".to_string(),
            direccion: Direccion::Subida,
            borrar: false,
            exclusiones: vec!["*.log".to_string()],
            ultima_ejecucion_en: None,
            ultimo_resultado: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
        };
        assert_eq!(
            detalle(&guardada, &tema),
            "web-prod · subida · sin borrar · excluye *.log"
        );
        guardada.direccion = Direccion::Bajada;
        guardada.borrar = true;
        guardada.exclusiones.clear();
        assert_eq!(
            detalle(&guardada, &tema),
            "web-prod · bajada · borra en destino lo que sobra"
        );
        let mut ascii = Tema::respaldo();
        ascii.ascii = true;
        ascii.glifos = crate::tema::Glifos::ascii();
        assert_eq!(
            detalle(&guardada, &ascii),
            "web-prod - bajada - borra en destino lo que sobra"
        );
    }

    #[test]
    fn el_resumen_de_las_exclusiones_por_defecto() {
        let patrones: Vec<String> = crate::config::EXCLUIR_POR_DEFECTO
            .iter()
            .map(|patron| patron.to_string())
            .collect();
        assert_eq!(
            resumen_por_defecto(&patrones),
            ".git/ target/ … y el .magiignore del origen"
        );
        assert_eq!(
            resumen_por_defecto(&[".git/".to_string()]),
            ".git/ y el .magiignore del origen"
        );
        assert_eq!(resumen_por_defecto(&[]), "el .magiignore del origen");
    }
}
