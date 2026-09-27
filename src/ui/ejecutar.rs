//! Diálogo EJECUTAR (maqueta §6.3): hosts con casillas en una rejilla que se
//! adapta al ancho (de una a tres columnas), una fila por variable, «parar
//! al primer fallo» con el timeout, el error y el pie. Todo texto que viene
//! del usuario o de la base se sanea al pintar.
//!
//! Disposición (Fase 7): las columnas de la rejilla y las filas de cada zona
//! se derivan del área en cada pintado; las columnas se registran en la
//! `Disposicion` para que las flechas recorran la misma rejilla que se ve.
//! Si el diálogo no cabe entero, el cuerpo se desplaza para que la zona con
//! el foco quede siempre a la vista; el error y el pie no se pierden nunca.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::lanzar::{DialogoEjecutar, FocoEjecutar, ModoLanzamiento, COLUMNAS_HOSTS};
use crate::app::App;
use crate::tema::Tema;
use crate::ui::centrar;
use crate::ui::componentes::{casilla, estilo_campo, CampoTexto};
use crate::ui::dialogos::{atajo, modal};
use crate::ui::disposicion::{Atajo, Disposicion};

/// Ancho del modal (se encoge si la terminal no da para tanto).
const ANCHO_MODAL: u16 = 72;
/// Filas de la rejilla de hosts y de variables visibles a la vez, como
/// mucho.
const FILAS_HOSTS_MAX: usize = 6;
const FILAS_VARIABLES_MAX: usize = 6;
/// Ancho máximo de la columna con el nombre de cada variable.
const ANCHO_NOMBRE_VARIABLE_MAX: usize = 16;
/// Ancho mínimo de una celda de la rejilla: la casilla, un espacio, un
/// nombre de once caracteres y la separación.
const ANCHO_CELDA_MIN: usize = 16;
/// Sustituto visible de un carácter de control.
const SUSTITUTO: char = '\u{FFFD}';

/// Columnas de la rejilla de hosts con el diálogo pintado en `area`: las que
/// caben con celdas de al menos `ANCHO_CELDA_MIN`, entre una y
/// `COLUMNAS_HOSTS`. Solo depende del ancho.
pub fn columnas_rejilla(area: Rect) -> usize {
    let recta = centrar(area, ANCHO_MODAL, 3);
    let ancho = recta.width.saturating_sub(6) as usize;
    (ancho.saturating_sub(1) / ANCHO_CELDA_MIN).clamp(1, COLUMNAS_HOSTS)
}

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, dialogo: &DialogoEjecutar) {
    dibujar_con_tema(marco, area, &app.tema, dialogo);
}

/// Como `dibujar`, y registra en `disp` las columnas de la rejilla que se
/// han pintado (las que usan las flechas).
pub fn dibujar_con_disposicion(
    marco: &mut Frame,
    area: Rect,
    app: &App,
    dialogo: &DialogoEjecutar,
    disp: &mut Disposicion,
) {
    disp.columnas_rejilla = Some(dibujar_con_tema(marco, area, &app.tema, dialogo));
}

/// Pinta el diálogo y devuelve las columnas de su rejilla.
fn dibujar_con_tema(
    marco: &mut Frame,
    area: Rect,
    tema: &Tema,
    dialogo: &DialogoEjecutar,
) -> usize {
    let columnas = columnas_rejilla(area);
    let filas_hosts = dialogo.hosts().len().div_ceil(columnas).max(1);
    let variables = dialogo.variables().len();
    // Alto natural: cabecera y filas de hosts, cabecera y filas de
    // variables, «parar», la línea en blanco, el error y el pie; más el
    // borde y el margen del modal.
    let con_variables = if variables > 0 {
        1 + variables.min(FILAS_VARIABLES_MAX)
    } else {
        0
    };
    let alto = 1 + filas_hosts.min(FILAS_HOSTS_MAX) + con_variables + 1 + 3 + 4;
    let recta = centrar(area, ANCHO_MODAL, alto as u16);
    // Hueco de texto que deja `modal`: borde más margen de 2×1.
    let ancho = recta.width.saturating_sub(6) as usize;
    let alto_texto = recta.height.saturating_sub(4) as usize;
    let reparto = repartir(
        alto_texto,
        filas_hosts,
        variables,
        dialogo.error().is_some(),
    );
    let contenido = Pintor {
        tema,
        dialogo,
        ancho,
        columnas,
    }
    .lineas(alto_texto, &reparto);
    let titulo = acortar(
        &format!(
            "EJECUTAR {} {}",
            tema.glifos.punto_medio,
            sanear(&dialogo.nombre)
        ),
        (recta.width as usize).saturating_sub(6),
        tema.ascii,
    );
    modal(marco, recta, &titulo, contenido, false, tema);
    columnas
}

/// Filas de cada zona para un alto de texto dado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reparto {
    filas_hosts: usize,
    filas_variables: usize,
    /// Fila del error: siempre que lo haya y, si sobra sitio, reservada
    /// (así el diálogo no salta al aparecer).
    linea_error: bool,
    /// Línea en blanco antes del pie.
    blanco: bool,
}

/// Reparte `alto` filas de texto: una fila como mínimo por lista y el resto,
/// alternando, hasta sus máximos; luego la fila del error y la línea en
/// blanco.
fn repartir(alto: usize, filas_hosts: usize, variables: usize, con_error: bool) -> Reparto {
    let hosts_max = filas_hosts.clamp(1, FILAS_HOSTS_MAX);
    let variables_max = variables.min(FILAS_VARIABLES_MAX);
    // Cabecera de hosts, «parar» y pie; la cabecera de variables si hay, y
    // el error si lo hay.
    let fijas = 3 + usize::from(variables > 0) + usize::from(con_error);
    let mut hosts = 1;
    let mut filas_variables = variables_max.min(1);
    let mut resto = alto.saturating_sub(fijas + hosts + filas_variables);
    while resto > 0 && (hosts < hosts_max || filas_variables < variables_max) {
        if hosts < hosts_max {
            hosts += 1;
            resto -= 1;
        }
        if resto > 0 && filas_variables < variables_max {
            filas_variables += 1;
            resto -= 1;
        }
    }
    let reservada = !con_error && resto > 0;
    if reservada {
        resto -= 1;
    }
    Reparto {
        filas_hosts: hosts,
        filas_variables,
        linea_error: con_error || reservada,
        blanco: resto > 0,
    }
}

/// Primera línea de una ventana de `hueco` líneas sobre `total` que deja a
/// la vista el tramo con el foco, que acaba en `hasta`: arriba del todo
/// mientras quepa y, si no, con el final del tramo en la última línea (en
/// un tramo más alto que el hueco, lo que se ve es su final: donde está el
/// cursor o el filtro).
pub(crate) fn ventana_foco(total: usize, hueco: usize, hasta: usize) -> usize {
    hasta.saturating_sub(hueco).min(total.saturating_sub(hueco))
}

/// Pie de un diálogo con los atajos que caben en `ancho`, por prioridad y
/// en su orden (los diálogos no llevan «? más»: `?` puede ser texto).
pub fn pie_por_prioridad(atajos: &[Atajo], ancho: usize, tema: &Tema) -> Line<'static> {
    let mut orden: Vec<usize> = (0..atajos.len()).collect();
    orden.sort_by_key(|indice| (atajos[*indice].prioridad, *indice));
    let mut visibles = vec![false; atajos.len()];
    let mut usado = 0;
    for indice in orden {
        // `Atajo::ancho` cuenta los dos espacios de separación.
        let propio = usize::from(atajos[indice].ancho()).saturating_sub(2);
        let necesario = if usado == 0 { propio } else { propio + 2 };
        if usado + necesario <= ancho {
            visibles[indice] = true;
            usado += necesario;
        }
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (atajo_pie, visible) in atajos.iter().zip(visibles) {
        if !visible {
            continue;
        }
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(atajo(&atajo_pie.tecla, tema));
        if !atajo_pie.texto.is_empty() {
            spans.push(Span::raw(format!(" {}", atajo_pie.texto)));
        }
    }
    Line::from(spans)
}

/// Construye las líneas del diálogo para un ancho de texto dado.
struct Pintor<'a> {
    tema: &'a Tema,
    dialogo: &'a DialogoEjecutar,
    ancho: usize,
    /// Columnas de la rejilla de hosts.
    columnas: usize,
}

impl Pintor<'_> {
    /// Todas las líneas, con el error y el pie siempre al final: si no cabe
    /// todo, el cuerpo se desplaza hasta la zona con el foco, nunca el pie.
    fn lineas(&self, alto: usize, reparto: &Reparto) -> Vec<Line<'static>> {
        let mut pie = Vec::new();
        if reparto.linea_error {
            pie.push(self.linea_error());
        }
        pie.push(self.linea_pie());
        let foco = self.dialogo.foco();
        let mut cuerpo = vec![self.cabecera_hosts(reparto.filas_hosts)];
        cuerpo.extend(self.lineas_hosts(reparto.filas_hosts));
        // Fin del tramo con el foco (la rejilla, por defecto).
        let mut hasta = cuerpo.len();
        if reparto.filas_variables > 0 {
            let cabecera = cuerpo.len();
            cuerpo.push(Line::from(Span::styled(
                "Variables",
                self.estilo_zona(matches!(foco, FocoEjecutar::Variable(_))),
            )));
            let (lineas, posicion) = self.lineas_variables(reparto.filas_variables);
            cuerpo.extend(lineas);
            if let (FocoEjecutar::Variable(_), Some(posicion)) = (foco, posicion) {
                hasta = cabecera + 2 + posicion;
            }
        }
        if foco == FocoEjecutar::Parar {
            hasta = cuerpo.len() + 1;
        }
        cuerpo.push(self.linea_parar());
        if reparto.blanco {
            cuerpo.push(Line::from(""));
        }
        let hueco = alto.saturating_sub(pie.len());
        let inicio = ventana_foco(cuerpo.len(), hueco, hasta);
        let mut visibles: Vec<Line<'static>> =
            cuerpo.into_iter().skip(inicio).take(hueco).collect();
        while visibles.len() < hueco {
            visibles.push(Line::from(""));
        }
        visibles.extend(pie);
        visibles
    }

    fn estilo_inactivo(&self) -> Style {
        Style::default().fg(self.tema.paleta.inactivo)
    }

    fn estilo_zona(&self, activa: bool) -> Style {
        if activa {
            Style::default()
                .fg(self.tema.paleta.acento)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.tema.paleta.inactivo)
        }
    }

    fn estilo_resaltado(&self) -> Style {
        Style::default()
            .bg(self.tema.paleta.acento)
            .fg(self.tema.paleta.fondo)
            .add_modifier(Modifier::BOLD)
    }

    fn acortar(&self, texto: &str, ancho: usize) -> String {
        acortar(texto, ancho, self.tema.ascii)
    }

    /// «Hosts (12)», con cuántos van marcados si no son todos, «fijos» en el
    /// modo «solo variables» y una pista si la rejilla no cabe entera.
    fn cabecera_hosts(&self, filas_visibles: usize) -> Line<'static> {
        let punto = self.tema.glifos.punto_medio;
        let total = self.dialogo.hosts().len();
        let marcados = self.dialogo.marcados();
        let mut texto = format!("Hosts ({total})");
        if !self.dialogo.hosts_editables() {
            texto.push_str(&format!(" {punto} fijos"));
        } else if marcados < total {
            texto.push_str(&format!(" {punto} {marcados} marcados"));
        }
        let filas = total.div_ceil(self.columnas);
        if filas > filas_visibles {
            let primera = ventana(self.dialogo.cursor_host() / self.columnas, filas_visibles);
            texto.push_str(&format!(
                " {punto} filas {}-{} de {filas}",
                primera + 1,
                (primera + filas_visibles).min(filas)
            ));
        }
        Line::from(Span::styled(
            self.acortar(&texto, self.ancho),
            self.estilo_zona(self.dialogo.foco() == FocoEjecutar::Hosts),
        ))
    }

    /// La rejilla de hosts, con la ventana de filas siguiendo al cursor.
    fn lineas_hosts(&self, filas_visibles: usize) -> Vec<Line<'static>> {
        let hosts = self.dialogo.hosts();
        if hosts.is_empty() {
            return vec![Line::from(Span::styled(" ninguno", self.estilo_inactivo()))];
        }
        let columnas = self.columnas;
        let activa = self.dialogo.foco() == FocoEjecutar::Hosts;
        let editables = self.dialogo.hosts_editables();
        let cursor = self.dialogo.cursor_host();
        let filas = hosts.len().div_ceil(columnas);
        let primera = ventana(cursor / columnas, filas_visibles);
        // Un espacio de sangría y celdas iguales.
        let celda = (self.ancho.saturating_sub(1) / columnas).max(6);
        (primera..(primera + filas_visibles).min(filas))
            .map(|fila| {
                let mut spans = vec![Span::raw(" ")];
                for columna in 0..columnas {
                    let indice = fila * columnas + columna;
                    let Some((_, nombre)) = hosts.get(indice) else {
                        break;
                    };
                    let marcado = self.dialogo.marcado(indice);
                    // «[x] nombre» y un espacio de separación.
                    let texto = format!(
                        "{} {}",
                        casilla(marcado),
                        self.acortar(&sanear(nombre), celda.saturating_sub(5))
                    );
                    let estilo = if activa && indice == cursor {
                        self.estilo_resaltado()
                    } else if editables && marcado {
                        Style::default().fg(self.tema.paleta.texto)
                    } else {
                        self.estilo_inactivo()
                    };
                    let relleno = celda.saturating_sub(texto.chars().count() + 1);
                    spans.push(Span::styled(texto, estilo));
                    spans.push(Span::raw(" ".repeat(relleno + 1)));
                }
                Line::from(spans)
            })
            .collect()
    }

    /// Una fila por variable: nombre y campo con su valor (el defecto
    /// precargado); la ventana sigue a la variable con el foco. Devuelve
    /// también la posición de la enfocada entre las visibles.
    fn lineas_variables(&self, filas_visibles: usize) -> (Vec<Line<'static>>, Option<usize>) {
        let variables = self.dialogo.variables();
        let columna = variables
            .iter()
            .map(|variable| variable.nombre.chars().count())
            .max()
            .unwrap_or(0)
            .min(ANCHO_NOMBRE_VARIABLE_MAX)
            .min(self.ancho / 3);
        let ancho_valor = self.ancho.saturating_sub(columna + 2 + 1 + 4);
        let enfocada = match self.dialogo.foco() {
            FocoEjecutar::Variable(indice) => Some(indice),
            _ => None,
        };
        let primera = ventana(enfocada.unwrap_or(0), filas_visibles);
        let lineas = variables
            .iter()
            .enumerate()
            .skip(primera)
            .take(filas_visibles)
            .map(|(indice, variable)| {
                let activa = enfocada == Some(indice);
                let nombre = self.acortar(&sanear(&variable.nombre), columna);
                let valor = self
                    .dialogo
                    .valor(indice)
                    .map(|campo| campo_visible(campo, activa, ancho_valor, self.tema.ascii))
                    .unwrap_or_default();
                Line::from(vec![
                    Span::styled(format!("  {nombre:<columna$} "), self.estilo_zona(activa)),
                    Span::styled("[ ", Style::default().fg(self.tema.paleta.acento)),
                    Span::styled(valor, estilo_campo(self.tema, activa)),
                    Span::styled(" ]", Style::default().fg(self.tema.paleta.acento)),
                ])
            })
            .collect();
        (
            lineas,
            enfocada.map(|indice| indice.saturating_sub(primera)),
        )
    }

    /// «[ ] parar al primer fallo       timeout 60 s»; en pestaña no aplica.
    /// Si no cabe todo, lo que sobra es el timeout.
    fn linea_parar(&self) -> Line<'static> {
        if self.dialogo.modo() == ModoLanzamiento::Pestanas {
            return Line::from(Span::styled(
                self.acortar(
                    "en pestaña: una por host, con el comando escrito al abrir la shell",
                    self.ancho,
                ),
                self.estilo_inactivo(),
            ));
        }
        let activa = self.dialogo.foco() == FocoEjecutar::Parar;
        let casilla = format!(
            "{} parar al primer fallo",
            casilla(self.dialogo.parar_al_fallo())
        );
        let timeout = format!("timeout {} s", self.dialogo.timeout_seg());
        let largo = casilla.chars().count() + timeout.chars().count();
        let mut spans = vec![Span::styled(
            self.acortar(&casilla, self.ancho),
            estilo_campo(self.tema, activa),
        )];
        if largo + 3 <= self.ancho {
            let relleno = self.ancho.saturating_sub(largo).clamp(3, 7);
            spans.push(Span::raw(" ".repeat(relleno)));
            spans.push(Span::styled(timeout, self.estilo_inactivo()));
        }
        Line::from(spans)
    }

    fn linea_error(&self) -> Line<'static> {
        match self.dialogo.error() {
            Some(error) => Line::from(Span::styled(
                self.acortar(
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

    /// Pie por prioridad: continuar y cancelar antes que marcar y zona.
    fn linea_pie(&self) -> Line<'static> {
        let glifos = &self.tema.glifos;
        let atajos = [
            Atajo::new(glifos.intro, "continuar", 1),
            Atajo::new("espacio", "marcar", 2),
            Atajo::new(glifos.tab, "zona", 3),
            Atajo::new("esc", "cancelar", 1),
        ];
        pie_por_prioridad(&atajos, self.ancho, self.tema)
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

/// Marcas de dirección bidireccional (U+061C, U+200E-F, U+202A-E, U+2066-9):
/// no son `is_control`, pero reordenan lo que se pinta en algunas terminales.
fn es_control_de_direccion(caracter: char) -> bool {
    matches!(
        caracter,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

/// Primera fila de una ventana de `filas` que deja a la vista la fila
/// `cursor`: arriba del todo mientras quepa y, si no, con el cursor en la
/// última fila visible.
fn ventana(cursor: usize, filas: usize) -> usize {
    (cursor + 1).saturating_sub(filas.max(1))
}

/// Marca del cursor en un campo de texto: la de `CampoTexto::span` y, en
/// ASCII, una barra.
pub(crate) fn marca_cursor(ascii: bool) -> char {
    if ascii {
        '|'
    } else {
        '\u{2503}'
    }
}

/// Marca de texto recortado (`…`; en ASCII, `~`).
pub(crate) fn marca_recorte(ascii: bool) -> char {
    if ascii {
        '~'
    } else {
        '…'
    }
}

/// Valor de un campo saneado, con el cursor si está activo y recortado a
/// `ancho` columnas de forma que el cursor quede a la vista.
pub(crate) fn campo_visible(campo: &CampoTexto, activo: bool, ancho: usize, ascii: bool) -> String {
    if ancho == 0 {
        return String::new();
    }
    let marca = marca_cursor(ascii);
    let recorte = marca_recorte(ascii);
    let caracteres: Vec<char> = sanear(&campo.texto).chars().collect();
    let cursor = activo.then_some(campo.cursor.min(caracteres.len()));
    let desde = cursor.map_or(0, |cursor| (cursor + 1).saturating_sub(ancho));
    let mut visible: Vec<char> = Vec::with_capacity(ancho + 1);
    for (indice, caracter) in caracteres.iter().enumerate().skip(desde) {
        if cursor == Some(indice) {
            visible.push(marca);
        }
        visible.push(*caracter);
    }
    if cursor == Some(caracteres.len()) {
        visible.push(marca);
    }
    if visible.len() > ancho {
        visible.truncate(ancho);
        if let Some(ultimo) = visible.last_mut() {
            if *ultimo != marca {
                *ultimo = recorte;
            }
        }
    }
    if desde > 0 {
        if let Some(primero) = visible.first_mut() {
            if *primero != marca {
                *primero = recorte;
            }
        }
    }
    let mut texto: String = visible.iter().collect();
    texto.push_str(&" ".repeat(ancho.saturating_sub(visible.len())));
    texto
}

fn acortar(texto: &str, ancho: usize, ascii: bool) -> String {
    crate::ui::disposicion::recortar(texto, ancho, ascii)
}

#[cfg(test)]
mod pruebas {
    use std::collections::BTreeMap;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::snippets::{Destino, Snippet};

    fn snippet(nombre: &str, comando: &str) -> Snippet {
        Snippet {
            id: 1,
            nombre: nombre.to_string(),
            comando: comando.to_string(),
            descripcion: String::new(),
            etiquetas: Vec::new(),
            critico: false,
            timeout_seg: 60,
            parar_al_fallo: false,
            usado_veces: 0,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: vec![Destino::Etiqueta("web".to_string())],
        }
    }

    fn pintar(ancho: u16, alto: u16, dialogo: &DialogoEjecutar) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(ancho, alto)).unwrap();
        let tema = Tema::respaldo();
        terminal
            .draw(|marco| {
                dibujar_con_tema(marco, marco.area(), &tema, dialogo);
            })
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
    fn pinta_la_maqueta() {
        let hosts: Vec<(i64, String)> = ["hetzner-01", "hetzner-02", "vps-openclaw", "backup-nas"]
            .iter()
            .enumerate()
            .map(|(indice, nombre)| (indice as i64 + 1, nombre.to_string()))
            .collect();
        let mut dialogo = DialogoEjecutar::completo(
            &snippet("limpiar journald", "journalctl --vacuum-time={{dias:7}}d"),
            hosts,
        );
        // Desmarca backup-nas.
        dialogo.manejar_tecla(
            &KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            COLUMNAS_HOSTS,
        );
        dialogo.manejar_tecla(
            &KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            COLUMNAS_HOSTS,
        );
        let todo = pintar(90, 24, &dialogo).join("\n");
        assert!(todo.contains("EJECUTAR · limpiar journald"), "{todo}");
        assert!(todo.contains("Hosts (4) · 3 marcados"), "{todo}");
        assert!(todo.contains("[x] hetzner-01"), "{todo}");
        assert!(todo.contains("[ ] backup-nas"), "{todo}");
        assert!(todo.contains("Variables"), "{todo}");
        assert!(todo.contains("dias [ 7"), "{todo}");
        assert!(todo.contains("[ ] parar al primer fallo"), "{todo}");
        assert!(todo.contains("timeout 60 s"), "{todo}");
        assert!(todo.contains("continuar"), "{todo}");
        assert!(todo.contains("esc cancelar"), "{todo}");
    }

    #[test]
    fn pinta_el_error_y_el_modo_en_pestana() {
        let mut dialogo = DialogoEjecutar::solo_variables(
            &snippet("reiniciar", "systemctl restart {{servicio}}"),
            vec![(1, "web-01".to_string())],
            ModoLanzamiento::Pestanas,
            &BTreeMap::new(),
        );
        assert!(dialogo.continuar().is_none());
        let todo = pintar(90, 24, &dialogo).join("\n");
        assert!(todo.contains("falta el valor de «servicio»"), "{todo}");
        assert!(todo.contains("Hosts (1) · fijos"), "{todo}");
        assert!(todo.contains("en pestaña: una por host"), "{todo}");
        assert!(!todo.contains("parar al primer fallo"), "{todo}");
    }

    #[test]
    fn pinta_sin_caracteres_de_control_a_cualquier_tamano() {
        let hosts: Vec<(i64, String)> = (1..=40)
            .map(|id| (id, format!("malo-{id}\u{1b}[2J\u{202E}")))
            .collect();
        let mut dialogo = DialogoEjecutar::completo(
            &snippet(
                "rojo\u{1b}[31m\u{7}",
                "echo {{texto}} {{otra:\u{1b}]0;x\u{7}}}",
            ),
            hosts,
        );
        dialogo.manejar_tecla(
            &KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            COLUMNAS_HOSTS,
        );
        dialogo.manejar_tecla(
            &KeyEvent::new(KeyCode::Char('\u{7}'), KeyModifiers::NONE),
            COLUMNAS_HOSTS,
        );
        for (ancho, alto) in [(100, 32), (60, 14), (20, 5), (12, 3)] {
            let filas = pintar(ancho, alto, &dialogo);
            for fila in &filas {
                assert!(
                    !fila
                        .chars()
                        .any(|c| c.is_control() || es_control_de_direccion(c)),
                    "carácter de control pintado en {fila:?}"
                );
            }
            if alto >= 14 {
                let todo = filas.join("\n");
                assert!(todo.contains("EJECUTAR"), "{todo}");
                assert!(todo.contains("continuar"), "{todo}");
            }
        }
    }

    #[test]
    fn la_rejilla_sigue_al_cursor() {
        let hosts: Vec<(i64, String)> = (1..=30).map(|id| (id, format!("h{id:02}"))).collect();
        let mut dialogo = DialogoEjecutar::completo(&snippet("x", "uptime"), hosts);
        for _ in 0..9 {
            dialogo.manejar_tecla(
                &KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                COLUMNAS_HOSTS,
            );
        }
        assert_eq!(dialogo.cursor_host(), 27);
        let todo = pintar(90, 24, &dialogo).join("\n");
        assert!(todo.contains("h28"), "{todo}");
        assert!(!todo.contains("h01"), "{todo}");
        assert!(todo.contains("filas 5-10 de 10"), "{todo}");
    }

    #[test]
    fn el_campo_deja_el_cursor_a_la_vista() {
        let mut campo = CampoTexto::nuevo("abcdefghij");
        assert_eq!(campo_visible(&campo, true, 5, false), "…hij\u{2503}");
        assert_eq!(campo_visible(&campo, false, 5, false), "abcd…");
        campo.inicio();
        assert_eq!(campo_visible(&campo, true, 5, false), "\u{2503}abc…");
        assert_eq!(
            campo_visible(&CampoTexto::nuevo("ab"), false, 4, false),
            "ab  "
        );
        assert_eq!(campo_visible(&campo, true, 0, false), "");
    }
}
