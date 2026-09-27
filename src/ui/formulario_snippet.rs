//! Diálogo del formulario de snippet (alta y edición): modal centrado con las
//! etiquetas de campo a la izquierda, el comando en varias filas con una
//! ventana que sigue al cursor, el desplegable de hosts en línea, la vista
//! previa de los destinos resueltos, las variables detectadas, el error y el
//! pie. Todo texto que viene del usuario o de la base se sanea al pintar.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::formulario_snippet::{CampoSnippet, FormularioSnippet, VistaPreviaDestinos};
use crate::app::App;
use crate::tema::Tema;
use crate::ui::componentes::{casilla, estilo_campo, CampoTexto};
use crate::ui::dialogos::{atajo, modal};
use crate::ui::{centrar, tecla};

/// Tamaño del modal (se encoge si la terminal no da para tanto).
const ANCHO_MODAL: u16 = 86;
const ALTO_MODAL: u16 = 28;
/// Columna de las etiquetas de campo.
const ANCHO_ETIQUETA: usize = 14;
/// Filas del comando: las que quepan, entre estas dos.
const FILAS_COMANDO_MIN: usize = 1;
const FILAS_COMANDO_MAX: usize = 7;
/// Opciones del desplegable de hosts visibles a la vez.
const OPCIONES_VISIBLES: usize = 5;
/// Ancho del campo del timeout.
const ANCHO_TIMEOUT: usize = 6;
/// Marca del cursor, la misma que `CampoTexto::span`.
const CURSOR: char = '\u{2503}';
/// Sustituto visible de un carácter de control.
const SUSTITUTO: char = '\u{FFFD}';

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, formulario: &FormularioSnippet) {
    dibujar_con_tema(marco, area, &app.tema, formulario);
}

fn dibujar_con_tema(marco: &mut Frame, area: Rect, tema: &Tema, formulario: &FormularioSnippet) {
    let recta = centrar(area, ANCHO_MODAL, ALTO_MODAL);
    // Hueco de texto que deja `modal`: borde más margen de 2×1.
    let ancho = recta.width.saturating_sub(6) as usize;
    let alto = recta.height.saturating_sub(4) as usize;
    let contenido = Pintor {
        tema,
        formulario,
        ancho,
    }
    .lineas(alto);
    let titulo = acortar(
        &sanear(&formulario.titulo()),
        (recta.width as usize).saturating_sub(6),
    );
    modal(marco, recta, &titulo, contenido, false, tema);
}

/// Construye las líneas del diálogo para un ancho de texto dado.
struct Pintor<'a> {
    tema: &'a Tema,
    formulario: &'a FormularioSnippet,
    ancho: usize,
}

impl Pintor<'_> {
    /// Todas las líneas, con el error y el pie siempre en las dos últimas
    /// filas: si no cabe todo, se recorta el cuerpo, no el pie.
    fn lineas(&self, alto: usize) -> Vec<Line<'static>> {
        let pie = vec![self.linea_error(), self.linea_pie()];
        let desplegable = self.lineas_desplegable();
        // Filas del cuerpo que no son del comando ni del desplegable.
        const FIJAS: usize = 13;
        let filas_comando = alto
            .saturating_sub(FIJAS + desplegable.len() + pie.len())
            .clamp(FILAS_COMANDO_MIN, FILAS_COMANDO_MAX);

        let mut cuerpo =
            vec![self.linea_texto("Nombre", CampoSnippet::Nombre, self.formulario.nombre())];
        cuerpo.extend(self.lineas_comando(filas_comando));
        cuerpo.push(self.linea_texto(
            "Descripción",
            CampoSnippet::Descripcion,
            self.formulario.descripcion(),
        ));
        cuerpo.push(self.linea_texto(
            "Etiquetas",
            CampoSnippet::Etiquetas,
            self.formulario.etiquetas(),
        ));
        cuerpo.push(self.cabecera_destinos());
        cuerpo.push(self.linea_texto(
            "  etiquetas",
            CampoSnippet::DestinosEtiqueta,
            self.formulario.destinos_etiqueta(),
        ));
        cuerpo.push(self.linea_sugerencias());
        cuerpo.push(self.linea_hosts());
        cuerpo.extend(desplegable);
        cuerpo.push(self.linea_casilla(
            "Crítico",
            CampoSnippet::Critico,
            self.formulario.critico(),
            "siempre pide la deliberación MAGI",
        ));
        cuerpo.push(self.linea_timeout());
        cuerpo.push(self.linea_casilla(
            "Parar",
            CampoSnippet::PararAlFallo,
            self.formulario.parar_al_fallo(),
            "al primer fallo",
        ));
        let hueco = alto.saturating_sub(pie.len());
        // La línea en blanco antes de la vista previa es lo primero que
        // sobra si la terminal no da para todo.
        if cuerpo.len() + 3 <= hueco {
            cuerpo.push(Line::from(""));
        }
        cuerpo.push(self.linea_vista_previa());
        cuerpo.push(self.linea_variables());
        cuerpo.truncate(hueco);
        while cuerpo.len() < hueco {
            cuerpo.push(Line::from(""));
        }
        cuerpo.extend(pie);
        cuerpo
    }

    fn activo(&self, campo: CampoSnippet) -> bool {
        self.formulario.foco() == campo
    }

    fn estilo_inactivo(&self) -> Style {
        Style::default().fg(self.tema.paleta.inactivo)
    }

    fn estilo_resaltado(&self) -> Style {
        Style::default()
            .bg(self.tema.paleta.acento)
            .fg(self.tema.paleta.fondo)
            .add_modifier(Modifier::BOLD)
    }

    fn etiqueta(&self, texto: &str, activo: bool) -> Span<'static> {
        Span::styled(
            format!("{texto:<ANCHO_ETIQUETA$}"),
            Style::default().fg(if activo {
                self.tema.paleta.acento
            } else {
                self.tema.paleta.inactivo
            }),
        )
    }

    fn corchete(&self, texto: &'static str) -> Span<'static> {
        Span::styled(texto, Style::default().fg(self.tema.paleta.acento))
    }

    /// Ancho del valor de un campo con corchetes a continuación de la
    /// etiqueta.
    fn ancho_valor(&self) -> usize {
        self.ancho.saturating_sub(ANCHO_ETIQUETA + 4)
    }

    fn linea_texto(
        &self,
        etiqueta: &str,
        campo: CampoSnippet,
        texto: &CampoTexto,
    ) -> Line<'static> {
        let activo = self.activo(campo);
        Line::from(vec![
            self.etiqueta(etiqueta, activo),
            self.corchete("[ "),
            Span::styled(
                campo_visible(texto, activo, self.ancho_valor()),
                estilo_campo(self.tema, activo),
            ),
            self.corchete(" ]"),
        ])
    }

    /// El comando en `filas` filas: activo, la ventana sigue a la fila y a la
    /// columna del cursor; inactivo, se ve desde el principio.
    fn lineas_comando(&self, filas: usize) -> Vec<Line<'static>> {
        let area = self.formulario.comando();
        let activo = self.activo(CampoSnippet::Comando);
        let total = area.lineas.len();
        let ancho = self.ancho.saturating_sub(ANCHO_ETIQUETA + 2);
        let (primera, desde) = if activo {
            (
                ventana(area.fila, filas),
                (area.columna + 1).saturating_sub(ancho),
            )
        } else {
            (0, 0)
        };
        let barra = tecla(self.tema, "\u{2502} ", "| ");
        let estilo = estilo_campo(self.tema, activo);
        (0..filas)
            .map(|posicion| {
                let indice = primera + posicion;
                let etiqueta = match posicion {
                    0 => self.etiqueta("Comando", activo),
                    // Posición en el comando cuando no cabe entero.
                    1 if total > filas => Span::styled(
                        format!(
                            "{:<ANCHO_ETIQUETA$}",
                            format!("  {}/{}", area.fila + 1, total)
                        ),
                        self.estilo_inactivo(),
                    ),
                    _ => Span::raw(" ".repeat(ANCHO_ETIQUETA)),
                };
                let texto = match area.lineas.get(indice) {
                    Some(linea) => {
                        let cursor = (activo && indice == area.fila).then_some(area.columna);
                        recortar(&caracteres_saneados(linea), cursor, desde, ancho)
                    }
                    None => String::new(),
                };
                Line::from(vec![
                    etiqueta,
                    self.corchete(barra),
                    Span::styled(texto, estilo),
                ])
            })
            .collect()
    }

    fn cabecera_destinos(&self) -> Line<'static> {
        let activo = matches!(
            self.formulario.foco(),
            CampoSnippet::DestinosEtiqueta | CampoSnippet::DestinosHost
        );
        Line::from(vec![
            self.etiqueta("Destinos", activo),
            Span::styled(
                acortar(
                    "etiquetas de host y hosts sueltos (al menos uno)",
                    self.ancho.saturating_sub(ANCHO_ETIQUETA),
                ),
                self.estilo_inactivo(),
            ),
        ])
    }

    /// Sugerencias de etiqueta de host bajo el campo, solo con el foco en él.
    /// La fila está siempre, para que el diálogo no salte al recorrerlo.
    fn linea_sugerencias(&self) -> Line<'static> {
        if !self.activo(CampoSnippet::DestinosEtiqueta) {
            return Line::from("");
        }
        // Cada sugerencia lleva un espacio a cada lado: con esta sangría el
        // texto queda en la columna del valor del campo.
        let hueco = self.ancho.saturating_sub(ANCHO_ETIQUETA + 1);
        let sangria = Span::raw(" ".repeat(ANCHO_ETIQUETA + 1));
        let sugerencias = self.formulario.sugerencias();
        if sugerencias.is_empty() {
            let aviso = if self.formulario.hay_etiquetas_host() {
                ""
            } else {
                "no hay etiquetas de host en el inventario"
            };
            return Line::from(vec![
                sangria,
                Span::styled(acortar(aviso, hueco), self.estilo_inactivo()),
            ]);
        }
        let elegida = self.formulario.indice_sugerencia();
        let textos: Vec<String> = sugerencias.iter().map(|texto| sanear(texto)).collect();
        let anchos: Vec<usize> = textos
            .iter()
            .map(|texto| texto.chars().count() + 2)
            .collect();
        let (inicio, fin) = tramo_visible(&anchos, elegida, hueco);
        let mut spans = vec![sangria];
        if inicio > 0 {
            spans.push(Span::styled("… ", self.estilo_inactivo()));
        }
        for (indice, texto) in textos.iter().enumerate().take(fin).skip(inicio) {
            let estilo = if indice == elegida {
                Style::default()
                    .bg(self.tema.paleta.correcto)
                    .fg(self.tema.paleta.fondo)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.tema.paleta.texto)
            };
            spans.push(Span::styled(format!(" {texto} "), estilo));
        }
        if fin < textos.len() {
            spans.push(Span::styled(" …", self.estilo_inactivo()));
        }
        Line::from(spans)
    }

    /// Hosts sueltos elegidos; con el foco, el marcado va resaltado.
    fn linea_hosts(&self) -> Line<'static> {
        let activo = self.activo(CampoSnippet::DestinosHost);
        // Cada nombre va con un espacio a cada lado (el resaltado se ve como
        // una pastilla), así que el hueco empieza justo tras el «[».
        let hueco = self.ancho_valor() + 1;
        let elegidos = self.formulario.hosts_elegidos();
        let mut spans = vec![self.etiqueta("  hosts", activo), self.corchete("[")];
        let mut usado = 0;
        if elegidos.is_empty() {
            let texto = if self.formulario.desplegable_hosts().opciones.is_empty() {
                " ninguno (no hay hosts en el inventario)"
            } else if activo {
                " ninguno: ↵ para añadir"
            } else {
                " ninguno"
            };
            let texto = acortar(texto, hueco);
            usado = texto.chars().count();
            spans.push(Span::styled(texto, self.estilo_inactivo()));
        } else {
            let marcado = self.formulario.host_marcado();
            // Nombres recortados a lo que quepa en el campo.
            let nombres: Vec<String> = elegidos
                .iter()
                .map(|(_, nombre)| acortar(&sanear(nombre), hueco.saturating_sub(6).max(1)))
                .collect();
            let anchos: Vec<usize> = nombres
                .iter()
                .map(|nombre| nombre.chars().count() + 2)
                .collect();
            let (inicio, fin) = tramo_visible(&anchos, marcado, hueco);
            if inicio > 0 {
                spans.push(Span::styled(" …", self.estilo_inactivo()));
                usado += 2;
            }
            for (indice, nombre) in nombres.iter().enumerate().take(fin).skip(inicio) {
                let estilo = if activo && indice == marcado {
                    self.estilo_resaltado()
                } else {
                    estilo_campo(self.tema, activo)
                };
                spans.push(Span::styled(format!(" {nombre} "), estilo));
                usado += anchos[indice];
            }
            if fin < nombres.len() {
                spans.push(Span::styled(" …", self.estilo_inactivo()));
                usado += 2;
            }
        }
        spans.push(Span::raw(" ".repeat(hueco.saturating_sub(usado))));
        spans.push(self.corchete(" ]"));
        Line::from(spans)
    }

    /// El desplegable de hosts, en línea bajo el campo, como en el diálogo de
    /// túnel: opciones filtradas con ventana sobre la resaltada y el filtro.
    fn lineas_desplegable(&self) -> Vec<Line<'static>> {
        let desplegable = self.formulario.desplegable_hosts();
        if !desplegable.abierto {
            return Vec::new();
        }
        let sangria = " ".repeat(ANCHO_ETIQUETA);
        let hueco = self.ancho.saturating_sub(ANCHO_ETIQUETA + 4);
        let filtradas = desplegable.filtradas();
        let mut lineas = Vec::new();
        if filtradas.is_empty() {
            lineas.push(Line::from(Span::styled(
                format!("{sangria}  ningún host coincide con el filtro"),
                self.estilo_inactivo(),
            )));
        }
        let primera = ventana(desplegable.resaltado, OPCIONES_VISIBLES);
        if primera > 0 {
            lineas.push(Line::from(Span::styled(
                format!("{sangria}    … {} más arriba", primera),
                self.estilo_inactivo(),
            )));
        }
        for (posicion, indice) in filtradas
            .iter()
            .enumerate()
            .skip(primera)
            .take(OPCIONES_VISIBLES)
        {
            let Some(opcion) = desplegable.opciones.get(*indice) else {
                continue;
            };
            let resaltada = posicion == desplegable.resaltado;
            let ya = match opcion.valor {
                crate::ui::componentes::ValorOpcion::Salto(id) => self.formulario.host_elegido(id),
                _ => false,
            };
            let marca = if resaltada {
                tecla(self.tema, "\u{25b8}", ">")
            } else {
                " "
            };
            let mut spans = vec![Span::styled(
                format!(
                    "{sangria}  {marca} {}",
                    acortar(&sanear(&opcion.etiqueta), hueco)
                ),
                if resaltada {
                    self.estilo_resaltado()
                } else {
                    Style::default().fg(self.tema.paleta.texto)
                },
            )];
            if ya {
                spans.push(Span::styled("  ya elegido", self.estilo_inactivo()));
            }
            lineas.push(Line::from(spans));
        }
        let debajo = filtradas.len().saturating_sub(primera + OPCIONES_VISIBLES);
        if debajo > 0 {
            lineas.push(Line::from(Span::styled(
                format!("{sangria}    … {debajo} más"),
                self.estilo_inactivo(),
            )));
        }
        lineas.push(Line::from(vec![
            Span::styled(format!("{sangria}  filtro: "), self.estilo_inactivo()),
            Span::styled(
                campo_visible(&desplegable.filtro, true, hueco.saturating_sub(8)),
                Style::default().fg(self.tema.paleta.texto),
            ),
        ]));
        lineas
    }

    fn linea_casilla(
        &self,
        etiqueta: &str,
        campo: CampoSnippet,
        marcada: bool,
        texto: &str,
    ) -> Line<'static> {
        let activo = self.activo(campo);
        Line::from(vec![
            self.etiqueta(etiqueta, activo),
            Span::styled(casilla(marcada), estilo_campo(self.tema, activo)),
            Span::styled(
                format!(
                    " {}",
                    acortar(texto, self.ancho.saturating_sub(ANCHO_ETIQUETA + 4))
                ),
                Style::default().fg(self.tema.paleta.texto),
            ),
        ])
    }

    fn linea_timeout(&self) -> Line<'static> {
        let activo = self.activo(CampoSnippet::Timeout);
        Line::from(vec![
            self.etiqueta("Timeout", activo),
            self.corchete("[ "),
            Span::styled(
                campo_visible(self.formulario.timeout(), activo, ANCHO_TIMEOUT),
                estilo_campo(self.tema, activo),
            ),
            self.corchete(" ]"),
            Span::styled(
                format!(
                    " segundos ({}-{})",
                    crate::snippets::TIMEOUT_MIN,
                    crate::snippets::TIMEOUT_MAX
                ),
                self.estilo_inactivo(),
            ),
        ])
    }

    /// «→ N hosts: a, b, c…» con los destinos actuales; en ámbar si no apunta
    /// a ninguno (se puede guardar igual).
    fn linea_vista_previa(&self) -> Line<'static> {
        let flecha = tecla(self.tema, "\u{2192}", "->");
        match self.formulario.vista_previa() {
            VistaPreviaDestinos::SinDestinos => Line::from(Span::styled(
                acortar(
                    &format!("{flecha} sin destinos: añade una etiqueta de host o un host"),
                    self.ancho,
                ),
                self.estilo_inactivo(),
            )),
            VistaPreviaDestinos::Hosts(nombres) if nombres.is_empty() => Line::from(Span::styled(
                acortar(
                    &format!("{flecha} 0 hosts · no apunta a ningún host: se puede guardar"),
                    self.ancho,
                ),
                Style::default().fg(self.tema.paleta.acento),
            )),
            VistaPreviaDestinos::Hosts(nombres) => {
                let cabeza = format!(
                    "{flecha} {} {}: ",
                    nombres.len(),
                    if nombres.len() == 1 { "host" } else { "hosts" }
                );
                let nombres: Vec<String> = nombres.iter().map(|nombre| sanear(nombre)).collect();
                let lista =
                    lista_recortada(&nombres, self.ancho.saturating_sub(cabeza.chars().count()));
                Line::from(vec![
                    Span::styled(cabeza, Style::default().fg(self.tema.paleta.acento)),
                    Span::styled(lista, Style::default().fg(self.tema.paleta.texto)),
                ])
            }
        }
    }

    /// «variables: dias (7), unidad».
    fn linea_variables(&self) -> Line<'static> {
        let variables = self.formulario.variables();
        let cabeza = "variables: ";
        if variables.is_empty() {
            return Line::from(Span::styled(
                format!("{cabeza}ninguna"),
                self.estilo_inactivo(),
            ));
        }
        let textos: Vec<String> = variables
            .iter()
            .map(|variable| match &variable.defecto {
                Some(defecto) => format!("{} ({})", variable.nombre, sanear(defecto)),
                None => variable.nombre.clone(),
            })
            .collect();
        Line::from(vec![
            Span::styled(cabeza, self.estilo_inactivo()),
            Span::styled(
                lista_recortada(&textos, self.ancho.saturating_sub(cabeza.chars().count())),
                Style::default().fg(self.tema.paleta.texto),
            ),
        ])
    }

    fn linea_error(&self) -> Line<'static> {
        match &self.formulario.error {
            Some(error) => Line::from(Span::styled(
                acortar(
                    &format!("{} {}", self.tema.glifos.error, sanear(error)),
                    self.ancho,
                ),
                Style::default()
                    .fg(self.tema.paleta.critico)
                    .add_modifier(Modifier::BOLD),
            )),
            None => Line::from(""),
        }
    }

    /// Pie: atajos generales a la izquierda y los del campo con el foco a la
    /// derecha; con la pregunta de descarte, solo la pregunta.
    fn linea_pie(&self) -> Line<'static> {
        let tema = self.tema;
        if self.formulario.confirmando_descarte() {
            return Line::from(vec![
                Span::styled(
                    "¿Descartar los cambios? ",
                    Style::default()
                        .fg(tema.paleta.critico)
                        .add_modifier(Modifier::BOLD),
                ),
                atajo("s", tema),
                Span::raw("/"),
                atajo("n", tema),
            ]);
        }
        let tabulador = tecla(tema, "\u{21e5}", "tab");
        let mut spans = vec![
            atajo("^s", tema),
            Span::raw(" guardar · "),
            atajo(tabulador, tema),
            Span::raw(" campo · "),
            atajo("esc", tema),
            Span::raw(" cancelar"),
        ];
        let usado: usize = spans.iter().map(|span| span.content.chars().count()).sum();
        let pista = acortar(&self.pista(), self.ancho.saturating_sub(usado + 3));
        if !pista.is_empty() {
            let relleno = self.ancho.saturating_sub(usado + pista.chars().count());
            spans.push(Span::raw(" ".repeat(relleno)));
            spans.push(Span::styled(pista, self.estilo_inactivo()));
        }
        Line::from(spans)
    }

    /// Teclas propias del campo con el foco.
    fn pista(&self) -> String {
        let tema = self.tema;
        let intro = tecla(tema, "↵", "enter");
        match self.formulario.foco() {
            CampoSnippet::Comando => format!("{intro} nueva línea"),
            CampoSnippet::DestinosEtiqueta => format!(
                "{intro} {} acepta · {} elige",
                tecla(tema, "→", "->"),
                tecla(tema, "↑↓", "arriba/abajo")
            ),
            CampoSnippet::DestinosHost if self.formulario.desplegable_hosts().abierto => {
                format!("{intro} añade · esc cierra")
            }
            CampoSnippet::DestinosHost => format!(
                "{intro} añadir · {} marcar · x quitar",
                tecla(tema, "←→", "izq/der")
            ),
            CampoSnippet::Critico | CampoSnippet::PararAlFallo => "espacio marca".to_string(),
            _ => String::new(),
        }
    }
}

/// Sustituye los caracteres de control (y los de control de dirección del
/// texto) por uno visible, uno por uno para no descolocar el cursor; el
/// tabulador pasa a espacio.
fn sanear(texto: &str) -> String {
    texto
        .chars()
        .map(|caracter| match caracter {
            '\t' => ' ',
            caracter if caracter.is_control() || es_control_de_direccion(caracter) => SUSTITUTO,
            caracter => caracter,
        })
        .collect()
}

fn caracteres_saneados(texto: &str) -> Vec<char> {
    sanear(texto).chars().collect()
}

/// Marcas de dirección bidireccional (U+061C, U+200E-F, U+202A-E, U+2066-9):
/// no son `is_control`, pero reordenan lo que se pinta en algunas terminales.
fn es_control_de_direccion(caracter: char) -> bool {
    matches!(
        caracter,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

/// Primera fila de una ventana de `filas` que deja a la vista la fila
/// `cursor`, sin estado: arriba del todo mientras quepa y, si no, con el
/// cursor en la última fila visible.
fn ventana(cursor: usize, filas: usize) -> usize {
    (cursor + 1).saturating_sub(filas.max(1))
}

/// Valor de un campo de una línea saneado, con el cursor si está activo y
/// recortado a `ancho` columnas de forma que el cursor quede a la vista.
fn campo_visible(campo: &CampoTexto, activo: bool, ancho: usize) -> String {
    let caracteres = caracteres_saneados(&campo.texto);
    let cursor = activo.then_some(campo.cursor.min(caracteres.len()));
    let desde = cursor.map_or(0, |cursor| (cursor + 1).saturating_sub(ancho));
    recortar(&caracteres, cursor, desde, ancho)
}

/// Los caracteres desde la columna `desde`, con la marca del cursor si hay y
/// rellenos o recortados a `ancho`. Un `…` marca lo que queda fuera por cada
/// lado (salvo donde está el cursor).
fn recortar(caracteres: &[char], cursor: Option<usize>, desde: usize, ancho: usize) -> String {
    if ancho == 0 {
        return String::new();
    }
    let cursor = cursor.map(|cursor| cursor.min(caracteres.len()));
    let mut visible: Vec<char> = Vec::with_capacity(ancho + 1);
    for (indice, caracter) in caracteres.iter().enumerate().skip(desde) {
        if cursor == Some(indice) {
            visible.push(CURSOR);
        }
        visible.push(*caracter);
    }
    if cursor == Some(caracteres.len()) && caracteres.len() >= desde {
        visible.push(CURSOR);
    }
    if visible.len() > ancho {
        visible.truncate(ancho);
        if let Some(ultimo) = visible.last_mut() {
            if *ultimo != CURSOR {
                *ultimo = '…';
            }
        }
    }
    if desde > 0 && !caracteres.is_empty() {
        match visible.first_mut() {
            Some(primero) if *primero != CURSOR => *primero = '…',
            Some(_) => {}
            None => visible.push('…'),
        }
    }
    let mut texto: String = visible.iter().collect();
    let relleno = ancho.saturating_sub(visible.len());
    texto.push_str(&" ".repeat(relleno));
    texto
}

/// Tramo `[inicio, fin)` de elementos de los anchos dados que cabe en
/// `hueco` columnas y contiene a `elegido` (reserva sitio para los `…`).
fn tramo_visible(anchos: &[usize], elegido: usize, hueco: usize) -> (usize, usize) {
    if anchos.is_empty() {
        return (0, 0);
    }
    let elegido = elegido.min(anchos.len() - 1);
    let util = hueco.saturating_sub(4);
    let mut inicio = 0;
    while inicio < elegido && anchos[inicio..=elegido].iter().sum::<usize>() > util {
        inicio += 1;
    }
    let mut fin = inicio;
    let mut usado = 0;
    while fin < anchos.len() && (fin <= elegido || usado + anchos[fin] <= util) {
        usado += anchos[fin];
        fin += 1;
    }
    (inicio, fin)
}

/// «a, b, c…» recortada a `ancho` columnas.
fn lista_recortada(elementos: &[String], ancho: usize) -> String {
    let mut texto = String::new();
    for (indice, elemento) in elementos.iter().enumerate() {
        let separador = if indice == 0 { "" } else { ", " };
        let siguiente = format!("{separador}{elemento}");
        let queda = indice + 1 < elementos.len();
        // Si detrás vienen más, se deja sitio para el «…».
        let reserva = if queda { 1 } else { 0 };
        if texto.chars().count() + siguiente.chars().count() + reserva > ancho {
            if texto.is_empty() {
                return acortar(elemento, ancho);
            }
            texto.push('…');
            return texto;
        }
        texto.push_str(&siguiente);
    }
    texto
}

fn acortar(texto: &str, ancho: usize) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    if ancho == 0 {
        return String::new();
    }
    let recortado: String = texto.chars().take(ancho - 1).collect();
    format!("{recortado}…")
}

#[cfg(test)]
mod pruebas {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::snippets::{Destino, Snippet};

    fn host(id: i64, nombre: &str, etiquetas: &[&str]) -> crate::modelo::Host {
        let mut host = crate::modelo::host_de_prueba();
        host.id = id;
        host.nombre = nombre.to_string();
        host.etiquetas = etiquetas.iter().map(|e| e.to_string()).collect();
        host
    }

    fn snippet_malicioso() -> Snippet {
        Snippet {
            id: 3,
            nombre: "rojo\u{1b}[31m\u{7}".to_string(),
            comando: "echo \u{1b}]0;titulo\u{7}\nls\t-l\r".to_string(),
            descripcion: "\u{202E}al revés".to_string(),
            etiquetas: vec!["x\u{9b}2J".to_string()],
            critico: false,
            timeout_seg: 60,
            parar_al_fallo: false,
            usado_veces: 0,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: vec![
                Destino::Etiqueta("web".to_string()),
                Destino::Host {
                    id: 2,
                    nombre: "malo\u{1b}[2J".to_string(),
                },
            ],
        }
    }

    fn pintar(ancho: u16, alto: u16, formulario: &FormularioSnippet) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(ancho, alto)).unwrap();
        let tema = Tema::respaldo();
        terminal
            .draw(|marco| dibujar_con_tema(marco, marco.area(), &tema, formulario))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn sanear_quita_los_caracteres_de_control_uno_por_uno() {
        assert_eq!(sanear("a\u{1b}[31mb"), "a\u{FFFD}[31mb");
        assert_eq!(sanear("x\ty\u{7}\u{9b}"), "x y\u{FFFD}\u{FFFD}");
        assert_eq!(sanear("\u{202E}abc"), "\u{FFFD}abc");
        assert_eq!(sanear("ñandú"), "ñandú");
    }

    #[test]
    fn recortar_deja_el_cursor_a_la_vista() {
        let texto: Vec<char> = "abcdefghij".chars().collect();
        // Cabe entero: relleno a la derecha.
        assert_eq!(recortar(&texto[..3], Some(3), 0, 6), "abc\u{2503}  ");
        // Cursor al final de un texto largo: se ve el final.
        let desde = (10 + 1) - 5;
        assert_eq!(recortar(&texto, Some(10), desde, 5), "…hij\u{2503}");
        // Sin cursor: se corta a la derecha con «…».
        assert_eq!(recortar(&texto, None, 0, 5), "abcd…");
        assert_eq!(recortar(&texto, None, 0, 0), "");
    }

    #[test]
    fn tramo_visible_incluye_al_elegido() {
        let anchos = vec![5, 5, 5, 5, 5];
        assert_eq!(tramo_visible(&anchos, 0, 14), (0, 2));
        let (inicio, fin) = tramo_visible(&anchos, 4, 14);
        assert!(inicio <= 4 && fin == 5);
        assert_eq!(tramo_visible(&[], 0, 10), (0, 0));
    }

    #[test]
    fn la_lista_se_recorta_con_puntos() {
        let nombres: Vec<String> = ["alfa", "beta", "gamma"]
            .iter()
            .map(|n| n.to_string())
            .collect();
        assert_eq!(lista_recortada(&nombres, 40), "alfa, beta, gamma");
        assert_eq!(lista_recortada(&nombres, 12), "alfa, beta…");
    }

    #[test]
    fn pinta_sin_caracteres_de_control_y_con_el_pie() {
        let hosts = vec![host(1, "alfa", &["web"]), host(2, "malo\u{1b}[2J", &[])];
        let mut formulario =
            FormularioSnippet::editar(&snippet_malicioso(), &hosts, &["web".to_string()]);
        // Abre el desplegable para pintar también sus opciones.
        for _ in 0..5 {
            formulario.manejar_tecla(&KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }
        formulario.manejar_tecla(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(formulario.desplegable_hosts().abierto);
        for (ancho, alto) in [(100, 32), (60, 14), (20, 5)] {
            let filas = pintar(ancho, alto, &formulario);
            for fila in &filas {
                assert!(
                    !fila.chars().any(|c| c.is_control()),
                    "carácter de control pintado en {fila:?}"
                );
            }
            if alto >= 14 {
                let todo = filas.join("\n");
                assert!(todo.contains("EDITAR SNIPPET"), "{todo}");
                assert!(todo.contains("guardar"), "{todo}");
            }
        }
    }

    #[test]
    fn pinta_la_vista_previa_y_las_variables() {
        let hosts = vec![host(1, "alfa", &["web"]), host(2, "beta", &["web"])];
        let mut formulario = FormularioSnippet::nuevo(&hosts, &[]);
        for _ in 0..4 {
            formulario.manejar_tecla(&KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }
        for caracter in "web".chars() {
            formulario.manejar_tecla(&KeyEvent::new(KeyCode::Char(caracter), KeyModifiers::NONE));
        }
        let todo = pintar(100, 32, &formulario).join("\n");
        assert!(todo.contains("NUEVO SNIPPET"), "{todo}");
        assert!(todo.contains("2 hosts: alfa, beta"), "{todo}");
        assert!(todo.contains("variables: ninguna"), "{todo}");
        assert!(todo.contains("^s guardar"), "{todo}");
    }
}
