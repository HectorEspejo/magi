use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::modelo::IdentidadRef;
use crate::tema::Tema;

/// Campo de texto de una línea con cursor propio.
#[derive(Debug, Clone, Default)]
pub struct CampoTexto {
    pub texto: String,
    pub cursor: usize,
}

impl CampoTexto {
    pub fn nuevo(texto: impl Into<String>) -> Self {
        let texto = texto.into();
        let cursor = texto.chars().count();
        Self { texto, cursor }
    }

    pub fn limpiar(&mut self) {
        self.texto.clear();
        self.cursor = 0;
    }

    pub fn insertar(&mut self, caracter: char) {
        let byte = self.byte_cursor();
        self.texto.insert(byte, caracter);
        self.cursor += 1;
    }

    pub fn insertar_texto(&mut self, texto: &str) {
        for caracter in texto.chars() {
            self.insertar(caracter);
        }
    }

    pub fn retroceso(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let byte = self.byte_cursor();
        let anterior = self.texto[..byte]
            .char_indices()
            .next_back()
            .map(|(indice, _)| indice)
            .unwrap_or(0);
        self.texto.remove(anterior);
        self.cursor -= 1;
    }

    pub fn suprimir(&mut self) {
        let byte = self.byte_cursor();
        if byte < self.texto.len() {
            self.texto.remove(byte);
        }
    }

    pub fn izquierda(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn derecha(&mut self) {
        if self.cursor < self.texto.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn inicio(&mut self) {
        self.cursor = 0;
    }

    pub fn fin(&mut self) {
        self.cursor = self.texto.chars().count();
    }

    fn byte_cursor(&self) -> usize {
        self.texto
            .char_indices()
            .nth(self.cursor)
            .map(|(indice, _)| indice)
            .unwrap_or(self.texto.len())
    }

    /// Procesa una tecla de edición. Devuelve `true` si la consumió.
    pub fn manejar_tecla(&mut self, tecla: &KeyEvent) -> bool {
        let control = tecla.modifiers.contains(KeyModifiers::CONTROL);
        match tecla.code {
            KeyCode::Char(caracter) if !control => {
                self.insertar(caracter);
                true
            }
            KeyCode::Backspace => {
                self.retroceso();
                true
            }
            KeyCode::Delete => {
                self.suprimir();
                true
            }
            KeyCode::Left => {
                self.izquierda();
                true
            }
            KeyCode::Right => {
                self.derecha();
                true
            }
            KeyCode::Home => {
                self.inicio();
                true
            }
            KeyCode::End => {
                self.fin();
                true
            }
            _ => false,
        }
    }

    /// Texto visible con el cursor marcado cuando está activo.
    pub fn span(&self, activo: bool, estilo: Style) -> Span<'static> {
        if !activo {
            return Span::styled(self.texto.clone(), estilo);
        }
        let caracteres: Vec<char> = self.texto.chars().collect();
        let mut texto = String::new();
        let mut cursor_puesto = false;
        for (indice, caracter) in caracteres.iter().enumerate() {
            if indice == self.cursor {
                texto.push('\u{2503}');
                cursor_puesto = true;
            }
            texto.push(*caracter);
        }
        if !cursor_puesto {
            texto.push('\u{2503}');
        }
        Span::styled(texto, estilo)
    }
}

/// Área de texto multilínea sencilla.
#[derive(Debug, Clone, Default)]
pub struct AreaTexto {
    pub lineas: Vec<String>,
    pub fila: usize,
    pub columna: usize,
}

impl AreaTexto {
    pub fn nuevo(texto: &str) -> Self {
        let lineas: Vec<String> = if texto.is_empty() {
            vec![String::new()]
        } else {
            texto.lines().map(str::to_string).collect()
        };
        let fila = lineas.len().saturating_sub(1);
        let columna = lineas[fila].chars().count();
        Self {
            lineas,
            fila,
            columna,
        }
    }

    pub fn texto(&self) -> String {
        self.lineas.join("\n").trim_end().to_string()
    }

    fn byte_columna(&self) -> usize {
        self.lineas[self.fila]
            .char_indices()
            .nth(self.columna)
            .map(|(indice, _)| indice)
            .unwrap_or(self.lineas[self.fila].len())
    }

    pub fn manejar_tecla(&mut self, tecla: &KeyEvent) -> bool {
        let control = tecla.modifiers.contains(KeyModifiers::CONTROL);
        match tecla.code {
            KeyCode::Char(caracter) if !control => {
                let byte = self.byte_columna();
                self.lineas[self.fila].insert(byte, caracter);
                self.columna += 1;
                true
            }
            KeyCode::Enter => {
                let byte = self.byte_columna();
                let resto = self.lineas[self.fila].split_off(byte);
                self.lineas.insert(self.fila + 1, resto);
                self.fila += 1;
                self.columna = 0;
                true
            }
            KeyCode::Backspace => {
                if self.columna > 0 {
                    let byte = self.byte_columna();
                    let anterior = self.lineas[self.fila][..byte]
                        .char_indices()
                        .next_back()
                        .map(|(indice, _)| indice)
                        .unwrap_or(0);
                    self.lineas[self.fila].remove(anterior);
                    self.columna -= 1;
                } else if self.fila > 0 {
                    let actual = self.lineas.remove(self.fila);
                    self.fila -= 1;
                    self.columna = self.lineas[self.fila].chars().count();
                    self.lineas[self.fila].push_str(&actual);
                }
                true
            }
            KeyCode::Delete => {
                let byte = self.byte_columna();
                if byte < self.lineas[self.fila].len() {
                    self.lineas[self.fila].remove(byte);
                } else if self.fila + 1 < self.lineas.len() {
                    let siguiente = self.lineas.remove(self.fila + 1);
                    self.lineas[self.fila].push_str(&siguiente);
                }
                true
            }
            KeyCode::Left => {
                self.columna = self.columna.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                if self.columna < self.lineas[self.fila].chars().count() {
                    self.columna += 1;
                } else if self.fila + 1 < self.lineas.len() {
                    self.fila += 1;
                    self.columna = 0;
                }
                true
            }
            KeyCode::Up => {
                if self.fila > 0 {
                    self.fila -= 1;
                    self.columna = self.columna.min(self.lineas[self.fila].chars().count());
                }
                true
            }
            KeyCode::Down => {
                if self.fila + 1 < self.lineas.len() {
                    self.fila += 1;
                    self.columna = self.columna.min(self.lineas[self.fila].chars().count());
                }
                true
            }
            KeyCode::Home => {
                self.columna = 0;
                true
            }
            KeyCode::End => {
                self.columna = self.lineas[self.fila].chars().count();
                true
            }
            _ => false,
        }
    }

    pub fn lineas_con_cursor(&self, activo: bool, estilo: Style) -> Vec<Line<'static>> {
        let mut salida = Vec::with_capacity(self.lineas.len());
        for (indice, linea) in self.lineas.iter().enumerate() {
            let mut texto = String::new();
            let caracteres: Vec<char> = linea.chars().collect();
            let mut puesto = false;
            for (posicion, caracter) in caracteres.iter().enumerate() {
                if activo && indice == self.fila && posicion == self.columna {
                    texto.push('\u{2503}');
                    puesto = true;
                }
                texto.push(*caracter);
            }
            if activo && indice == self.fila && !puesto {
                texto.push('\u{2503}');
            }
            salida.push(Line::from(Span::styled(texto, estilo)));
        }
        salida
    }
}

/// Valor asociado a una opción de desplegable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValorOpcion {
    Ninguno,
    Grupo(i64),
    Identidad(IdentidadRef),
    Salto(i64),
}

#[derive(Debug, Clone)]
pub struct Opcion {
    pub etiqueta: String,
    pub valor: ValorOpcion,
}

#[derive(Debug, Clone)]
pub struct Desplegable {
    pub opciones: Vec<Opcion>,
    pub seleccion: usize,
    pub abierto: bool,
    pub filtro: CampoTexto,
    pub resaltado: usize,
}

pub enum ResultadoDesplegable {
    SinCambio,
    Cerrado,
    Seleccionado(ValorOpcion),
}

impl Desplegable {
    pub fn nuevo(opciones: Vec<Opcion>) -> Self {
        Self {
            opciones,
            seleccion: 0,
            abierto: false,
            filtro: CampoTexto::default(),
            resaltado: 0,
        }
    }

    pub fn etiqueta_seleccionada(&self) -> &str {
        self.opciones
            .get(self.seleccion)
            .map(|opcion| opcion.etiqueta.as_str())
            .unwrap_or("")
    }

    pub fn valor_seleccionado(&self) -> ValorOpcion {
        self.opciones
            .get(self.seleccion)
            .map(|opcion| opcion.valor.clone())
            .unwrap_or(ValorOpcion::Ninguno)
    }

    pub fn seleccionar_etiqueta(&mut self, etiqueta: &str) {
        if let Some(indice) = self
            .opciones
            .iter()
            .position(|opcion| opcion.etiqueta == etiqueta)
        {
            self.seleccion = indice;
        }
    }

    pub fn seleccionar_valor(&mut self, valor: &ValorOpcion) {
        if let Some(indice) = self
            .opciones
            .iter()
            .position(|opcion| &opcion.valor == valor)
        {
            self.seleccion = indice;
        }
    }

    pub fn filtradas(&self) -> Vec<usize> {
        let filtro = self.filtro.texto.to_lowercase();
        self.opciones
            .iter()
            .enumerate()
            .filter(|(_, opcion)| opcion.etiqueta.to_lowercase().contains(&filtro))
            .map(|(indice, _)| indice)
            .collect()
    }

    pub fn abrir(&mut self) {
        self.abierto = true;
        self.filtro.limpiar();
        self.resaltado = 0;
    }

    pub fn manejar_tecla(&mut self, tecla: &KeyEvent) -> ResultadoDesplegable {
        let filtradas = self.filtradas();
        match tecla.code {
            KeyCode::Esc => {
                self.abierto = false;
                ResultadoDesplegable::Cerrado
            }
            KeyCode::Enter => {
                if let Some(indice) = filtradas.get(self.resaltado) {
                    self.seleccion = *indice;
                }
                self.abierto = false;
                ResultadoDesplegable::Seleccionado(self.valor_seleccionado())
            }
            KeyCode::Up => {
                self.resaltado = self.resaltado.saturating_sub(1);
                ResultadoDesplegable::SinCambio
            }
            KeyCode::Down => {
                if self.resaltado + 1 < filtradas.len() {
                    self.resaltado += 1;
                }
                ResultadoDesplegable::SinCambio
            }
            _ => {
                if self.filtro.manejar_tecla(tecla) {
                    self.resaltado = 0;
                }
                ResultadoDesplegable::SinCambio
            }
        }
    }
}

/// Símbolo de casilla, igual en Unicode y en ASCII.
pub fn casilla(marcada: bool) -> &'static str {
    if marcada {
        "[x]"
    } else {
        "[ ]"
    }
}

pub fn estilo_campo(tema: &Tema, activo: bool) -> Style {
    let base = Style::default().fg(tema.paleta.texto);
    if activo {
        base.fg(tema.paleta.acento).add_modifier(Modifier::BOLD)
    } else {
        base
    }
}
