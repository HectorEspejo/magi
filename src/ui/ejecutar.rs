//! Diálogo EJECUTAR (maqueta §6.3): hosts con casillas en una rejilla de tres
//! columnas, una fila por variable, «parar al primer fallo» con el timeout,
//! el error y el pie. Todo texto que viene del usuario o de la base se sanea
//! al pintar.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::lanzar::{DialogoEjecutar, FocoEjecutar, ModoLanzamiento, COLUMNAS_HOSTS};
use crate::app::App;
use crate::tema::Tema;
use crate::ui::componentes::{casilla, estilo_campo, CampoTexto};
use crate::ui::dialogos::{atajo, modal};
use crate::ui::{centrar, tecla};

/// Ancho del modal (se encoge si la terminal no da para tanto).
const ANCHO_MODAL: u16 = 72;
/// Filas de la rejilla de hosts y de variables visibles a la vez.
const FILAS_HOSTS_MAX: usize = 6;
const FILAS_VARIABLES_MAX: usize = 6;
/// Ancho máximo de la columna con el nombre de cada variable.
const ANCHO_NOMBRE_VARIABLE_MAX: usize = 16;
/// Marca del cursor, la misma que `CampoTexto::span`.
const CURSOR: char = '\u{2503}';
/// Sustituto visible de un carácter de control.
const SUSTITUTO: char = '\u{FFFD}';

pub fn dibujar(marco: &mut Frame, area: Rect, app: &App, dialogo: &DialogoEjecutar) {
    dibujar_con_tema(marco, area, &app.tema, dialogo);
}

fn dibujar_con_tema(marco: &mut Frame, area: Rect, tema: &Tema, dialogo: &DialogoEjecutar) {
    let filas_hosts = dialogo
        .hosts()
        .len()
        .div_ceil(COLUMNAS_HOSTS)
        .clamp(1, FILAS_HOSTS_MAX);
    let filas_variables = dialogo.variables().len().min(FILAS_VARIABLES_MAX);
    // Cabecera y filas de hosts, cabecera y filas de variables, «parar», la
    // línea en blanco, el error y el pie; más el borde y el margen del modal.
    let variables = if filas_variables > 0 {
        1 + filas_variables
    } else {
        0
    };
    let alto = 1 + filas_hosts + variables + 1 + 3 + 4;
    let recta = centrar(area, ANCHO_MODAL, alto as u16);
    // Hueco de texto que deja `modal`: borde más margen de 2×1.
    let ancho = recta.width.saturating_sub(6) as usize;
    let alto_texto = recta.height.saturating_sub(4) as usize;
    let contenido = Pintor {
        tema,
        dialogo,
        ancho,
    }
    .lineas(alto_texto, filas_hosts, filas_variables);
    let titulo = acortar(
        &format!("EJECUTAR · {}", sanear(&dialogo.nombre)),
        (recta.width as usize).saturating_sub(6),
    );
    modal(marco, recta, &titulo, contenido, false, tema);
}

/// Construye las líneas del diálogo para un ancho de texto dado.
struct Pintor<'a> {
    tema: &'a Tema,
    dialogo: &'a DialogoEjecutar,
    ancho: usize,
}

impl Pintor<'_> {
    /// Todas las líneas, con el error y el pie siempre en las dos últimas
    /// filas: si no cabe todo, se recorta el cuerpo, no el pie.
    fn lineas(
        &self,
        alto: usize,
        filas_hosts: usize,
        filas_variables: usize,
    ) -> Vec<Line<'static>> {
        let pie = vec![self.linea_error(), self.linea_pie()];
        let mut cuerpo = vec![self.cabecera_hosts(filas_hosts)];
        cuerpo.extend(self.lineas_hosts(filas_hosts));
        if filas_variables > 0 {
            cuerpo.push(Line::from(Span::styled(
                "Variables",
                self.estilo_zona(matches!(self.dialogo.foco(), FocoEjecutar::Variable(_))),
            )));
            cuerpo.extend(self.lineas_variables(filas_variables));
        }
        cuerpo.push(self.linea_parar());
        let hueco = alto.saturating_sub(pie.len());
        if cuerpo.len() < hueco {
            cuerpo.push(Line::from(""));
        }
        cuerpo.truncate(hueco);
        while cuerpo.len() < hueco {
            cuerpo.push(Line::from(""));
        }
        cuerpo.extend(pie);
        cuerpo
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

    /// «Hosts (12)», con cuántos van marcados si no son todos, «fijos» en el
    /// modo «solo variables» y una pista si la rejilla no cabe entera.
    fn cabecera_hosts(&self, filas_visibles: usize) -> Line<'static> {
        let total = self.dialogo.hosts().len();
        let marcados = self.dialogo.marcados();
        let mut texto = format!("Hosts ({total})");
        if !self.dialogo.hosts_editables() {
            texto.push_str(" · fijos");
        } else if marcados < total {
            texto.push_str(&format!(" · {marcados} marcados"));
        }
        let filas = total.div_ceil(COLUMNAS_HOSTS);
        if filas > filas_visibles {
            let primera = ventana(self.dialogo.cursor_host() / COLUMNAS_HOSTS, filas_visibles);
            texto.push_str(&format!(
                " · filas {}-{} de {filas}",
                primera + 1,
                (primera + filas_visibles).min(filas)
            ));
        }
        Line::from(Span::styled(
            acortar(&texto, self.ancho),
            self.estilo_zona(self.dialogo.foco() == FocoEjecutar::Hosts),
        ))
    }

    /// La rejilla de hosts, con la ventana de filas siguiendo al cursor.
    fn lineas_hosts(&self, filas_visibles: usize) -> Vec<Line<'static>> {
        let hosts = self.dialogo.hosts();
        if hosts.is_empty() {
            return vec![Line::from(Span::styled(" ninguno", self.estilo_inactivo()))];
        }
        let activa = self.dialogo.foco() == FocoEjecutar::Hosts;
        let editables = self.dialogo.hosts_editables();
        let cursor = self.dialogo.cursor_host();
        let filas = hosts.len().div_ceil(COLUMNAS_HOSTS);
        let primera = ventana(cursor / COLUMNAS_HOSTS, filas_visibles);
        // Un espacio de sangría y tres celdas iguales.
        let celda = (self.ancho.saturating_sub(1) / COLUMNAS_HOSTS).max(6);
        (primera..(primera + filas_visibles).min(filas))
            .map(|fila| {
                let mut spans = vec![Span::raw(" ")];
                for columna in 0..COLUMNAS_HOSTS {
                    let indice = fila * COLUMNAS_HOSTS + columna;
                    let Some((_, nombre)) = hosts.get(indice) else {
                        break;
                    };
                    let marcado = self.dialogo.marcado(indice);
                    // «[x] nombre» y un espacio de separación.
                    let texto = format!(
                        "{} {}",
                        casilla(marcado),
                        acortar(&sanear(nombre), celda.saturating_sub(5))
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
    /// precargado); la ventana sigue a la variable con el foco.
    fn lineas_variables(&self, filas_visibles: usize) -> Vec<Line<'static>> {
        let variables = self.dialogo.variables();
        let columna = variables
            .iter()
            .map(|variable| variable.nombre.chars().count())
            .max()
            .unwrap_or(0)
            .min(ANCHO_NOMBRE_VARIABLE_MAX);
        let ancho_valor = self.ancho.saturating_sub(columna + 2 + 1 + 4);
        let enfocada = match self.dialogo.foco() {
            FocoEjecutar::Variable(indice) => Some(indice),
            _ => None,
        };
        let primera = ventana(enfocada.unwrap_or(0), filas_visibles);
        variables
            .iter()
            .enumerate()
            .skip(primera)
            .take(filas_visibles)
            .map(|(indice, variable)| {
                let activa = enfocada == Some(indice);
                let nombre = acortar(&sanear(&variable.nombre), columna);
                let valor = self
                    .dialogo
                    .valor(indice)
                    .map(|campo| campo_visible(campo, activa, ancho_valor))
                    .unwrap_or_default();
                Line::from(vec![
                    Span::styled(format!("  {nombre:<columna$} "), self.estilo_zona(activa)),
                    Span::styled("[ ", Style::default().fg(self.tema.paleta.acento)),
                    Span::styled(valor, estilo_campo(self.tema, activa)),
                    Span::styled(" ]", Style::default().fg(self.tema.paleta.acento)),
                ])
            })
            .collect()
    }

    /// «[ ] parar al primer fallo       timeout 60 s»; en pestaña no aplica.
    fn linea_parar(&self) -> Line<'static> {
        if self.dialogo.modo() == ModoLanzamiento::Pestanas {
            return Line::from(Span::styled(
                acortar(
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
        let relleno = self
            .ancho
            .saturating_sub(casilla.chars().count() + timeout.chars().count())
            .clamp(3, 7);
        Line::from(vec![
            Span::styled(casilla, estilo_campo(self.tema, activa)),
            Span::raw(" ".repeat(relleno)),
            Span::styled(timeout, self.estilo_inactivo()),
        ])
    }

    fn linea_error(&self) -> Line<'static> {
        match self.dialogo.error() {
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

    fn linea_pie(&self) -> Line<'static> {
        let tema = self.tema;
        Line::from(vec![
            atajo(tecla(tema, "↵", "enter"), tema),
            Span::raw(" continuar   "),
            atajo("espacio", tema),
            Span::raw(" marcar   "),
            atajo(tecla(tema, "⇥", "tab"), tema),
            Span::raw(" zona   "),
            atajo("esc", tema),
            Span::raw(" cancelar"),
        ])
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

/// Valor de un campo saneado, con el cursor si está activo y recortado a
/// `ancho` columnas de forma que el cursor quede a la vista.
fn campo_visible(campo: &CampoTexto, activo: bool, ancho: usize) -> String {
    if ancho == 0 {
        return String::new();
    }
    let caracteres: Vec<char> = sanear(&campo.texto).chars().collect();
    let cursor = activo.then_some(campo.cursor.min(caracteres.len()));
    let desde = cursor.map_or(0, |cursor| (cursor + 1).saturating_sub(ancho));
    let mut visible: Vec<char> = Vec::with_capacity(ancho + 1);
    for (indice, caracter) in caracteres.iter().enumerate().skip(desde) {
        if cursor == Some(indice) {
            visible.push(CURSOR);
        }
        visible.push(*caracter);
    }
    if cursor == Some(caracteres.len()) {
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
    if desde > 0 {
        if let Some(primero) = visible.first_mut() {
            if *primero != CURSOR {
                *primero = '…';
            }
        }
    }
    let mut texto: String = visible.iter().collect();
    texto.push_str(&" ".repeat(ancho.saturating_sub(visible.len())));
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
            .draw(|marco| dibujar_con_tema(marco, marco.area(), &tema, dialogo))
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
        dialogo.manejar_tecla(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        dialogo.manejar_tecla(&KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
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
        dialogo.manejar_tecla(&KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        dialogo.manejar_tecla(&KeyEvent::new(KeyCode::Char('\u{7}'), KeyModifiers::NONE));
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
            dialogo.manejar_tecla(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
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
        assert_eq!(campo_visible(&campo, true, 5), "…hij\u{2503}");
        assert_eq!(campo_visible(&campo, false, 5), "abcd…");
        campo.inicio();
        assert_eq!(campo_visible(&campo, true, 5), "\u{2503}abc…");
        assert_eq!(campo_visible(&CampoTexto::nuevo("ab"), false, 4), "ab  ");
        assert_eq!(campo_visible(&campo, true, 0), "");
    }
}
