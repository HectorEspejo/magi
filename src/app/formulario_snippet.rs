//! Formulario de alta y edición de snippets (`n` / `e` en F8): nombre,
//! comando multilínea, descripción, etiquetas del snippet, destinos
//! (etiquetas de host con autocompletado y hosts sueltos con desplegable),
//! crítico, timeout y «parar al primer fallo». `Ctrl+S` guarda.
//!
//! Lleva fijados los datos sobre los que actúa (el id del snippet, la lista
//! de hosts y de etiquetas de host) desde que se abre: no relee el estado.

use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::modelo::Host;
use crate::snippets::variables::{self, Variable};
use crate::snippets::{self, DatosSnippet, Destino, Snippet};
use crate::ui::componentes::{
    AreaTexto, CampoTexto, Desplegable, Opcion, ResultadoDesplegable, ValorOpcion,
};

/// Error de `Ctrl+S` cuando el campo del timeout no es un número.
const ERROR_TIMEOUT: &str = "el timeout debe ser un número de segundos";

/// Sugerencias de etiqueta de host que se ofrecen como mucho a la vez.
const MAX_SUGERENCIAS: usize = 8;

/// Lo que pide una tecla del formulario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionFormulario {
    Nada,
    /// `Ctrl+S` con datos que pasan la validación local.
    Guardar,
    /// `Esc` sin cambios, o descarte confirmado.
    Cancelar,
}

/// Campos del formulario, en el orden en el que los recorre `Tab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoSnippet {
    Nombre,
    Comando,
    Descripcion,
    Etiquetas,
    DestinosEtiqueta,
    DestinosHost,
    Critico,
    Timeout,
    PararAlFallo,
}

pub const ORDEN_CAMPOS_SNIPPET: [CampoSnippet; 9] = [
    CampoSnippet::Nombre,
    CampoSnippet::Comando,
    CampoSnippet::Descripcion,
    CampoSnippet::Etiquetas,
    CampoSnippet::DestinosEtiqueta,
    CampoSnippet::DestinosHost,
    CampoSnippet::Critico,
    CampoSnippet::Timeout,
    CampoSnippet::PararAlFallo,
];

/// Lo que enseña la línea de vista previa de destinos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VistaPreviaDestinos {
    /// Todavía no hay ningún destino: así no se puede guardar.
    SinDestinos,
    /// Nombres de los hosts a los que apunta, en el orden de la vista Hosts.
    /// Vacío si los destinos no resuelven a ningún host (se guarda y avisa).
    Hosts(Vec<String>),
}

/// Estado del formulario.
pub struct FormularioSnippet {
    /// `None` = alta.
    pub id: Option<i64>,
    /// Error de validación o de guardado que enseña el pie del formulario.
    pub error: Option<String>,
    /// Campos y datos fijados, en caja: el formulario viaja dentro de
    /// `Dialogo` y no debe agrandar el enum.
    estado: Box<Estado>,
}

/// Campos del formulario y datos fijados al abrir.
struct Estado {
    /// Nombre con el que se abrió la edición (título del diálogo).
    nombre_original: Option<String>,
    foco: CampoSnippet,
    nombre: CampoTexto,
    comando: AreaTexto,
    descripcion: CampoTexto,
    /// Etiquetas propias del snippet, separadas por espacio.
    etiquetas: CampoTexto,
    /// Etiquetas de host destino, separadas por espacio.
    destinos_etiqueta: CampoTexto,
    indice_sugerencia: usize,
    /// Hosts sueltos elegidos (id y nombre), en el orden en que se añadieron.
    hosts_elegidos: Vec<(i64, String)>,
    /// Host elegido marcado con `←` `→` (el que quitan `x` / `Supr` / `⌫`).
    host_marcado: usize,
    /// Desplegable con todos los hosts: `ValorOpcion::Salto` lleva el id.
    desplegable_hosts: Desplegable,
    critico: bool,
    timeout: CampoTexto,
    parar_al_fallo: bool,
    /// `Esc` con cambios: se pregunta «¿Descartar los cambios? s/n».
    confirmando_descarte: bool,
    /// Hosts fijados al abrir (vista previa y desplegable).
    hosts: Vec<Host>,
    /// Etiquetas de host conocidas al abrir, en minúsculas y sin repetir.
    etiquetas_host: Vec<String>,
    /// Datos normalizados al abrir, para saber si hay cambios.
    apertura: DatosSnippet,
}

impl FormularioSnippet {
    /// Alta de un snippet nuevo. `hosts` y `etiquetas_host` alimentan el
    /// desplegable de hosts sueltos y el autocompletado de etiquetas.
    pub fn nuevo(hosts: &[Host], etiquetas_host: &[String]) -> Self {
        let datos = DatosSnippet::default();
        let estado = Estado {
            nombre_original: None,
            foco: CampoSnippet::Nombre,
            nombre: CampoTexto::default(),
            comando: AreaTexto::nuevo(""),
            descripcion: CampoTexto::default(),
            etiquetas: CampoTexto::default(),
            destinos_etiqueta: CampoTexto::default(),
            indice_sugerencia: 0,
            hosts_elegidos: Vec::new(),
            host_marcado: 0,
            desplegable_hosts: Desplegable::nuevo(
                hosts
                    .iter()
                    .map(|host| Opcion {
                        etiqueta: host.nombre.clone(),
                        valor: ValorOpcion::Salto(host.id),
                    })
                    .collect(),
            ),
            critico: datos.critico,
            timeout: CampoTexto::nuevo(datos.timeout_seg.to_string()),
            parar_al_fallo: datos.parar_al_fallo,
            confirmando_descarte: false,
            hosts: hosts.to_vec(),
            etiquetas_host: etiquetas_conocidas(hosts, etiquetas_host),
            apertura: DatosSnippet::default(),
        };
        let mut formulario = Self {
            id: None,
            error: None,
            estado: Box::new(estado),
        };
        formulario.estado.apertura = formulario.datos();
        formulario
    }

    /// Edición de un snippet existente.
    pub fn editar(snippet: &Snippet, hosts: &[Host], etiquetas_host: &[String]) -> Self {
        let mut formulario = Self::nuevo(hosts, etiquetas_host);
        formulario.id = Some(snippet.id);
        formulario.estado.nombre_original = Some(snippet.nombre.clone());
        formulario.estado.nombre = CampoTexto::nuevo(snippet.nombre.clone());
        formulario.estado.comando = AreaTexto::nuevo(&snippet.comando);
        formulario.estado.descripcion = CampoTexto::nuevo(snippet.descripcion.clone());
        formulario.estado.etiquetas = CampoTexto::nuevo(snippet.etiquetas.join(" "));
        let mut etiquetas_destino: Vec<&str> = Vec::new();
        for destino in &snippet.destinos {
            match destino {
                Destino::Etiqueta(etiqueta) => etiquetas_destino.push(etiqueta),
                Destino::Host { id, nombre } => {
                    if !formulario
                        .estado
                        .hosts_elegidos
                        .iter()
                        .any(|(visto, _)| visto == id)
                    {
                        formulario.estado.hosts_elegidos.push((*id, nombre.clone()));
                    }
                }
            }
        }
        formulario.estado.destinos_etiqueta = CampoTexto::nuevo(etiquetas_destino.join(" "));
        formulario.estado.critico = snippet.critico;
        formulario.estado.timeout = CampoTexto::nuevo(snippet.timeout_seg.to_string());
        formulario.estado.parar_al_fallo = snippet.parar_al_fallo;
        formulario.estado.apertura = formulario.datos();
        formulario
    }

    /// Procesa una tecla.
    pub fn manejar_tecla(&mut self, tecla: &KeyEvent) -> AccionFormulario {
        let control = tecla.modifiers.contains(KeyModifiers::CONTROL);
        // Guardar vale siempre, también con el desplegable abierto o con la
        // pregunta de descarte en pantalla: si no, `^s` se perdería dentro
        // del filtro de hosts.
        if control && matches!(tecla.code, KeyCode::Char('s') | KeyCode::Char('S')) {
            self.estado.desplegable_hosts.abierto = false;
            self.estado.confirmando_descarte = false;
            return self.guardar();
        }
        if self.estado.confirmando_descarte {
            return match tecla.code {
                KeyCode::Char('s') | KeyCode::Char('S') if !control => AccionFormulario::Cancelar,
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc if !control => {
                    self.estado.confirmando_descarte = false;
                    AccionFormulario::Nada
                }
                _ => AccionFormulario::Nada,
            };
        }
        // Un error ya enseñado se retira en cuanto cambia lo escrito.
        let antes = self.error.is_some().then(|| self.datos());
        let accion = self.tecla_edicion(tecla, control);
        if antes.is_some_and(|antes| antes != self.datos()) {
            self.error = None;
        }
        accion
    }

    /// Tecla sin pregunta de descarte pendiente.
    fn tecla_edicion(&mut self, tecla: &KeyEvent, control: bool) -> AccionFormulario {
        // El desplegable abierto se queda con las teclas (filtro, ↑↓, ↵, esc):
        // `Esc` solo lo cierra.
        if self.estado.desplegable_hosts.abierto {
            self.tecla_desplegable(tecla);
            return AccionFormulario::Nada;
        }
        if control {
            return AccionFormulario::Nada;
        }
        match tecla.code {
            KeyCode::Esc => {
                if self.sucio() {
                    self.estado.confirmando_descarte = true;
                    return AccionFormulario::Nada;
                }
                return AccionFormulario::Cancelar;
            }
            KeyCode::Tab => self.avanzar(1),
            KeyCode::BackTab => self.avanzar(-1),
            _ => self.editar_campo(tecla),
        }
        AccionFormulario::Nada
    }

    /// Lo que se guardaría: lo escrito, armado y normalizado. Un timeout que
    /// no es un número queda en 0 (`Ctrl+S` lo rechaza con su propio aviso).
    pub fn datos(&self) -> DatosSnippet {
        snippets::normalizar(self.armar())
    }

    /// ¿Hay cambios respecto a lo que se abrió?
    pub fn sucio(&self) -> bool {
        self.datos() != self.estado.apertura
    }

    // ------------------------------------------------------------ lectura

    /// Título del diálogo: «NUEVO SNIPPET» o «EDITAR SNIPPET · nombre».
    pub fn titulo(&self) -> String {
        match &self.estado.nombre_original {
            None => "NUEVO SNIPPET".to_string(),
            Some(nombre) => format!("EDITAR SNIPPET · {nombre}"),
        }
    }

    pub fn foco(&self) -> CampoSnippet {
        self.estado.foco
    }

    pub fn nombre(&self) -> &CampoTexto {
        &self.estado.nombre
    }

    pub fn comando(&self) -> &AreaTexto {
        &self.estado.comando
    }

    pub fn descripcion(&self) -> &CampoTexto {
        &self.estado.descripcion
    }

    pub fn etiquetas(&self) -> &CampoTexto {
        &self.estado.etiquetas
    }

    pub fn destinos_etiqueta(&self) -> &CampoTexto {
        &self.estado.destinos_etiqueta
    }

    pub fn hosts_elegidos(&self) -> &[(i64, String)] {
        &self.estado.hosts_elegidos
    }

    /// Índice del host elegido marcado, acotado a la lista.
    pub fn host_marcado(&self) -> usize {
        self.estado
            .host_marcado
            .min(self.estado.hosts_elegidos.len().saturating_sub(1))
    }

    pub fn desplegable_hosts(&self) -> &Desplegable {
        &self.estado.desplegable_hosts
    }

    /// ¿Está ya elegido este host?
    pub fn host_elegido(&self, id: i64) -> bool {
        self.estado
            .hosts_elegidos
            .iter()
            .any(|(elegido, _)| *elegido == id)
    }

    pub fn critico(&self) -> bool {
        self.estado.critico
    }

    pub fn timeout(&self) -> &CampoTexto {
        &self.estado.timeout
    }

    pub fn parar_al_fallo(&self) -> bool {
        self.estado.parar_al_fallo
    }

    /// ¿Está en pantalla la pregunta «¿Descartar los cambios? s/n»?
    pub fn confirmando_descarte(&self) -> bool {
        self.estado.confirmando_descarte
    }

    /// ¿Se conoce alguna etiqueta de host para autocompletar?
    pub fn hay_etiquetas_host(&self) -> bool {
        !self.estado.etiquetas_host.is_empty()
    }

    /// Etiquetas de host que completan la palabra bajo el cursor del campo
    /// de destinos por etiqueta (sin las ya escritas ni la palabra exacta).
    pub fn sugerencias(&self) -> Vec<String> {
        let caracteres: Vec<char> = self.estado.destinos_etiqueta.texto.chars().collect();
        let (inicio, fin) = self.palabra_actual();
        let palabra: String = caracteres[inicio..fin]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let resto: String = caracteres[..inicio]
            .iter()
            .chain(std::iter::once(&' '))
            .chain(caracteres[fin..].iter())
            .collect();
        let puestas: BTreeSet<String> = resto.split_whitespace().map(str::to_lowercase).collect();
        self.estado
            .etiquetas_host
            .iter()
            .filter(|etiqueta| {
                etiqueta.starts_with(&palabra)
                    && **etiqueta != palabra
                    && !puestas.contains(*etiqueta)
            })
            .take(MAX_SUGERENCIAS)
            .cloned()
            .collect()
    }

    /// Sugerencia resaltada, acotada a las que hay.
    pub fn indice_sugerencia(&self) -> usize {
        self.estado
            .indice_sugerencia
            .min(self.sugerencias().len().saturating_sub(1))
    }

    /// Hosts a los que apuntan los destinos actuales, con los hosts fijados
    /// al abrir (sin grupos: el orden es por nombre).
    pub fn vista_previa(&self) -> VistaPreviaDestinos {
        let datos = self.datos();
        if datos.destinos.is_empty() {
            return VistaPreviaDestinos::SinDestinos;
        }
        VistaPreviaDestinos::Hosts(
            snippets::resolver(&datos.destinos, &self.estado.hosts, &[])
                .iter()
                .map(|host| host.nombre.clone())
                .collect(),
        )
    }

    /// Variables del comando tal como está escrito, en orden de aparición.
    pub fn variables(&self) -> Vec<Variable> {
        variables::detectar(&self.texto_comando())
    }

    // ------------------------------------------------------------ interno

    fn texto_comando(&self) -> String {
        self.estado.comando.lineas.join("\n")
    }

    /// Timeout escrito: `None` si no es un número. Un número que no cabe en
    /// `u32` es «demasiado grande» (la validación dice el rango).
    fn timeout_seg(&self) -> Option<u32> {
        let texto = self.estado.timeout.texto.trim();
        if texto.is_empty() || !texto.chars().all(|caracter| caracter.is_ascii_digit()) {
            return None;
        }
        Some(texto.parse::<u32>().unwrap_or(u32::MAX))
    }

    /// Lo escrito en forma de `DatosSnippet`, sin normalizar.
    fn armar(&self) -> DatosSnippet {
        let mut destinos: Vec<Destino> = self
            .estado
            .destinos_etiqueta
            .texto
            .split_whitespace()
            .map(|etiqueta| Destino::Etiqueta(etiqueta.to_string()))
            .collect();
        destinos.extend(
            self.estado
                .hosts_elegidos
                .iter()
                .map(|(id, nombre)| Destino::Host {
                    id: *id,
                    nombre: nombre.clone(),
                }),
        );
        DatosSnippet {
            nombre: self.estado.nombre.texto.clone(),
            comando: self.texto_comando(),
            descripcion: self.estado.descripcion.texto.clone(),
            etiquetas: self
                .estado
                .etiquetas
                .texto
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            critico: self.estado.critico,
            timeout_seg: self.timeout_seg().unwrap_or(0),
            parar_al_fallo: self.estado.parar_al_fallo,
            destinos,
        }
    }

    /// `Ctrl+S`: arma, normaliza y valida. Si no vale, deja el motivo en
    /// `error` y no guarda.
    fn guardar(&mut self) -> AccionFormulario {
        let resultado = match self.timeout_seg() {
            None => Err(ERROR_TIMEOUT.to_string()),
            Some(_) => snippets::validar(&self.datos()),
        };
        match resultado {
            Ok(()) => {
                self.error = None;
                AccionFormulario::Guardar
            }
            Err(motivo) => {
                self.error = Some(motivo);
                AccionFormulario::Nada
            }
        }
    }

    /// Siguiente (o anterior) campo en el orden del `Tab`.
    fn avanzar(&mut self, paso: i32) {
        let total = ORDEN_CAMPOS_SNIPPET.len() as i32;
        let posicion = ORDEN_CAMPOS_SNIPPET
            .iter()
            .position(|campo| *campo == self.estado.foco)
            .unwrap_or(0) as i32;
        self.estado.foco = ORDEN_CAMPOS_SNIPPET[(posicion + paso).rem_euclid(total) as usize];
        self.estado.indice_sugerencia = 0;
    }

    fn editar_campo(&mut self, tecla: &KeyEvent) {
        match self.estado.foco {
            CampoSnippet::Nombre => {
                self.estado.nombre.manejar_tecla(tecla);
            }
            // `↵` inserta un salto de línea; `Tab` no llega aquí.
            CampoSnippet::Comando => {
                self.estado.comando.manejar_tecla(tecla);
            }
            CampoSnippet::Descripcion => {
                self.estado.descripcion.manejar_tecla(tecla);
            }
            CampoSnippet::Etiquetas => {
                self.estado.etiquetas.manejar_tecla(tecla);
            }
            CampoSnippet::DestinosEtiqueta => self.tecla_destinos_etiqueta(tecla),
            CampoSnippet::DestinosHost => self.tecla_destinos_host(tecla),
            CampoSnippet::Critico => {
                if tecla.code == KeyCode::Char(' ') {
                    self.estado.critico = !self.estado.critico;
                }
            }
            CampoSnippet::Timeout => {
                // Solo entran dígitos; el resto de teclas de edición, sí.
                let admitida = match tecla.code {
                    KeyCode::Char(caracter) => caracter.is_ascii_digit(),
                    _ => true,
                };
                if admitida {
                    self.estado.timeout.manejar_tecla(tecla);
                }
            }
            CampoSnippet::PararAlFallo => {
                if tecla.code == KeyCode::Char(' ') {
                    self.estado.parar_al_fallo = !self.estado.parar_al_fallo;
                }
            }
        }
    }

    /// Destinos por etiqueta: `↑` `↓` eligen sugerencia; `↵` la acepta, y `→`
    /// también si el cursor está al final de una palabra empezada.
    fn tecla_destinos_etiqueta(&mut self, tecla: &KeyEvent) {
        let total = self.sugerencias().len();
        match tecla.code {
            KeyCode::Up if total > 0 => {
                self.estado.indice_sugerencia = self.indice_sugerencia().saturating_sub(1);
            }
            KeyCode::Down if total > 0 => {
                self.estado.indice_sugerencia = (self.indice_sugerencia() + 1).min(total - 1);
            }
            KeyCode::Enter => {
                self.aceptar_sugerencia();
            }
            KeyCode::Right if total > 0 && self.al_final_de_palabra() => {
                self.aceptar_sugerencia();
            }
            _ => {
                if self.estado.destinos_etiqueta.manejar_tecla(tecla) {
                    self.estado.indice_sugerencia = 0;
                }
            }
        }
    }

    /// Límites (en caracteres) de la palabra bajo el cursor del campo de
    /// destinos por etiqueta.
    fn palabra_actual(&self) -> (usize, usize) {
        let caracteres: Vec<char> = self.estado.destinos_etiqueta.texto.chars().collect();
        let cursor = self.estado.destinos_etiqueta.cursor.min(caracteres.len());
        let mut inicio = cursor;
        while inicio > 0 && !caracteres[inicio - 1].is_whitespace() {
            inicio -= 1;
        }
        let mut fin = cursor;
        while fin < caracteres.len() && !caracteres[fin].is_whitespace() {
            fin += 1;
        }
        (inicio, fin)
    }

    /// ¿Está el cursor al final del texto, tras una palabra empezada?
    fn al_final_de_palabra(&self) -> bool {
        let (inicio, fin) = self.palabra_actual();
        fin > inicio
            && self.estado.destinos_etiqueta.cursor
                >= self.estado.destinos_etiqueta.texto.chars().count()
    }

    /// Sustituye la palabra bajo el cursor por la sugerencia resaltada y deja
    /// el cursor tras un espacio, listo para la siguiente.
    fn aceptar_sugerencia(&mut self) {
        let sugerencias = self.sugerencias();
        let Some(elegida) = sugerencias.get(self.indice_sugerencia()) else {
            return;
        };
        let caracteres: Vec<char> = self.estado.destinos_etiqueta.texto.chars().collect();
        let (inicio, fin) = self.palabra_actual();
        let mut texto: String = caracteres[..inicio].iter().collect();
        texto.push_str(elegida);
        if fin == caracteres.len() {
            texto.push(' ');
        } else {
            texto.extend(&caracteres[fin..]);
        }
        self.estado.destinos_etiqueta = CampoTexto {
            texto,
            cursor: inicio + elegida.chars().count() + 1,
        };
        self.estado.indice_sugerencia = 0;
    }

    /// Destinos por host: `↵` abre el desplegable, `←` `→` marcan un elegido
    /// y `x` / `Supr` / `⌫` lo quitan.
    fn tecla_destinos_host(&mut self, tecla: &KeyEvent) {
        match tecla.code {
            KeyCode::Enter => {
                if !self.estado.desplegable_hosts.opciones.is_empty() {
                    self.estado.desplegable_hosts.abrir();
                }
            }
            KeyCode::Left => self.estado.host_marcado = self.host_marcado().saturating_sub(1),
            KeyCode::Right => {
                self.estado.host_marcado = (self.host_marcado() + 1)
                    .min(self.estado.hosts_elegidos.len().saturating_sub(1));
            }
            KeyCode::Home => self.estado.host_marcado = 0,
            KeyCode::End => {
                self.estado.host_marcado = self.estado.hosts_elegidos.len().saturating_sub(1)
            }
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => self.quitar_host(),
            _ => {}
        }
    }

    fn tecla_desplegable(&mut self, tecla: &KeyEvent) {
        // `↵` sin ninguna opción que pase el filtro solo cierra: el
        // desplegable devolvería la última seleccionada.
        if tecla.code == KeyCode::Enter
            && self
                .estado
                .desplegable_hosts
                .filtradas()
                .get(self.estado.desplegable_hosts.resaltado)
                .is_none()
        {
            self.estado.desplegable_hosts.abierto = false;
            return;
        }
        if let ResultadoDesplegable::Seleccionado(ValorOpcion::Salto(id)) =
            self.estado.desplegable_hosts.manejar_tecla(tecla)
        {
            self.anadir_host(id);
        }
    }

    /// Añade un host de los fijados al abrir, si no estaba, y lo marca.
    fn anadir_host(&mut self, id: i64) {
        if let Some(posicion) = self
            .estado
            .hosts_elegidos
            .iter()
            .position(|(elegido, _)| *elegido == id)
        {
            self.estado.host_marcado = posicion;
            return;
        }
        let Some(host) = self.estado.hosts.iter().find(|host| host.id == id) else {
            return;
        };
        self.estado
            .hosts_elegidos
            .push((host.id, host.nombre.clone()));
        self.estado.host_marcado = self.estado.hosts_elegidos.len() - 1;
    }

    fn quitar_host(&mut self) {
        if self.estado.hosts_elegidos.is_empty() {
            return;
        }
        let marcado = self.host_marcado();
        self.estado.hosts_elegidos.remove(marcado);
        self.estado.host_marcado = marcado.min(self.estado.hosts_elegidos.len().saturating_sub(1));
    }
}

/// Etiquetas de host para el autocompletado: las que pasa quien abre más las
/// de los hosts fijados, en minúsculas, sin repetir y ordenadas.
fn etiquetas_conocidas(hosts: &[Host], etiquetas_host: &[String]) -> Vec<String> {
    etiquetas_host
        .iter()
        .chain(hosts.iter().flat_map(|host| host.etiquetas.iter()))
        .map(|etiqueta| etiqueta.trim().to_lowercase())
        .filter(|etiqueta| !etiqueta.is_empty() && !etiqueta.contains(char::is_whitespace))
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tecla(codigo: KeyCode) -> KeyEvent {
        KeyEvent::new(codigo, KeyModifiers::NONE)
    }

    fn ctrl(caracter: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(caracter), KeyModifiers::CONTROL)
    }

    fn pulsar(formulario: &mut FormularioSnippet, codigo: KeyCode) -> AccionFormulario {
        formulario.manejar_tecla(&tecla(codigo))
    }

    fn escribir(formulario: &mut FormularioSnippet, texto: &str) {
        for caracter in texto.chars() {
            let codigo = if caracter == '\n' {
                KeyCode::Enter
            } else {
                KeyCode::Char(caracter)
            };
            assert_eq!(pulsar(formulario, codigo), AccionFormulario::Nada);
        }
    }

    fn ir_a(formulario: &mut FormularioSnippet, campo: CampoSnippet) {
        for _ in 0..ORDEN_CAMPOS_SNIPPET.len() {
            if formulario.foco() == campo {
                return;
            }
            pulsar(formulario, KeyCode::Tab);
        }
        panic!("el campo {campo:?} no se alcanza con Tab");
    }

    fn host(id: i64, nombre: &str, etiquetas: &[&str]) -> Host {
        let mut host = crate::modelo::host_de_prueba();
        host.id = id;
        host.nombre = nombre.to_string();
        host.grupo_id = None;
        host.etiquetas = etiquetas.iter().map(|e| e.to_string()).collect();
        host
    }

    fn hosts() -> Vec<Host> {
        vec![
            host(1, "alfa", &["web"]),
            host(2, "beta", &["db"]),
            host(3, "gamma", &["web", "web-front"]),
        ]
    }

    fn snippet() -> Snippet {
        Snippet {
            id: 7,
            nombre: "limpiar journald".to_string(),
            comando: "journalctl --vacuum-time={{dias:7}}d\nsystemctl status {{unidad}}"
                .to_string(),
            descripcion: "libera disco".to_string(),
            etiquetas: vec!["disco".to_string(), "mantenimiento".to_string()],
            critico: true,
            timeout_seg: 120,
            parar_al_fallo: true,
            usado_veces: 3,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: vec![
                Destino::Etiqueta("web".to_string()),
                Destino::Host {
                    id: 2,
                    nombre: "beta".to_string(),
                },
            ],
        }
    }

    /// Nombre, comando y un destino: lo mínimo para poder guardar.
    fn rellenar_minimo(formulario: &mut FormularioSnippet) {
        ir_a(formulario, CampoSnippet::Nombre);
        escribir(formulario, "uptime");
        ir_a(formulario, CampoSnippet::Comando);
        escribir(formulario, "uptime");
        ir_a(formulario, CampoSnippet::DestinosEtiqueta);
        escribir(formulario, "web");
    }

    #[test]
    fn tab_y_shift_tab_recorren_los_campos_en_orden() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        assert_eq!(formulario.foco(), CampoSnippet::Nombre);
        let mut vistos = vec![formulario.foco()];
        for _ in 1..ORDEN_CAMPOS_SNIPPET.len() {
            pulsar(&mut formulario, KeyCode::Tab);
            vistos.push(formulario.foco());
        }
        assert_eq!(vistos, ORDEN_CAMPOS_SNIPPET.to_vec());
        pulsar(&mut formulario, KeyCode::Tab);
        assert_eq!(formulario.foco(), CampoSnippet::Nombre);
        pulsar(&mut formulario, KeyCode::BackTab);
        assert_eq!(formulario.foco(), CampoSnippet::PararAlFallo);
        pulsar(&mut formulario, KeyCode::BackTab);
        assert_eq!(formulario.foco(), CampoSnippet::Timeout);
    }

    #[test]
    fn intro_en_el_comando_inserta_un_salto_y_tab_sale_del_campo() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::Comando);
        escribir(&mut formulario, "uptime\ndf -h");
        assert_eq!(formulario.foco(), CampoSnippet::Comando);
        assert_eq!(formulario.datos().comando, "uptime\ndf -h");
        pulsar(&mut formulario, KeyCode::Tab);
        assert_eq!(formulario.foco(), CampoSnippet::Descripcion);
        assert_eq!(formulario.datos().comando, "uptime\ndf -h");
    }

    #[test]
    fn las_casillas_cambian_con_espacio() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::Critico);
        pulsar(&mut formulario, KeyCode::Char(' '));
        assert!(formulario.critico());
        assert!(formulario.datos().critico);
        pulsar(&mut formulario, KeyCode::Char(' '));
        assert!(!formulario.datos().critico);
        ir_a(&mut formulario, CampoSnippet::PararAlFallo);
        pulsar(&mut formulario, KeyCode::Char(' '));
        assert!(formulario.datos().parar_al_fallo);
        // En un campo de texto el espacio es un carácter más.
        ir_a(&mut formulario, CampoSnippet::Nombre);
        escribir(&mut formulario, "a b");
        assert_eq!(formulario.nombre().texto, "a b");
        assert!(!formulario.critico());
    }

    #[test]
    fn el_timeout_solo_admite_digitos_y_vacio_no_guarda() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        rellenar_minimo(&mut formulario);
        ir_a(&mut formulario, CampoSnippet::Timeout);
        assert_eq!(formulario.timeout().texto, "60");
        pulsar(&mut formulario, KeyCode::Backspace);
        pulsar(&mut formulario, KeyCode::Backspace);
        escribir(&mut formulario, "9x0");
        assert_eq!(formulario.timeout().texto, "90");
        assert_eq!(formulario.datos().timeout_seg, 90);
        pulsar(&mut formulario, KeyCode::Backspace);
        pulsar(&mut formulario, KeyCode::Backspace);
        assert_eq!(formulario.manejar_tecla(&ctrl('s')), AccionFormulario::Nada);
        assert_eq!(formulario.error.as_deref(), Some(ERROR_TIMEOUT));
        escribir(&mut formulario, "3");
        assert!(formulario.error.is_none());
        assert_eq!(formulario.manejar_tecla(&ctrl('s')), AccionFormulario::Nada);
        assert!(formulario
            .error
            .as_deref()
            .unwrap()
            .contains("entre 5 y 3600"));
        escribir(&mut formulario, "0");
        assert_eq!(
            formulario.manejar_tecla(&ctrl('s')),
            AccionFormulario::Guardar
        );
        assert_eq!(formulario.datos().timeout_seg, 30);
    }

    #[test]
    fn anadir_y_quitar_hosts_sueltos() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::DestinosHost);
        // ↵ abre el desplegable; ↓ ↵ elige el segundo host.
        pulsar(&mut formulario, KeyCode::Enter);
        assert!(formulario.desplegable_hosts().abierto);
        pulsar(&mut formulario, KeyCode::Down);
        pulsar(&mut formulario, KeyCode::Enter);
        assert!(!formulario.desplegable_hosts().abierto);
        assert_eq!(formulario.hosts_elegidos(), &[(2, "beta".to_string())]);
        // Con filtro.
        pulsar(&mut formulario, KeyCode::Enter);
        escribir(&mut formulario, "gam");
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(
            formulario.hosts_elegidos(),
            &[(2, "beta".to_string()), (3, "gamma".to_string())]
        );
        assert_eq!(formulario.host_marcado(), 1);
        // Elegir otra vez uno que ya está no lo repite: solo lo marca.
        pulsar(&mut formulario, KeyCode::Enter);
        pulsar(&mut formulario, KeyCode::Down);
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(formulario.hosts_elegidos().len(), 2);
        assert_eq!(formulario.host_marcado(), 0);
        // Un filtro sin coincidencias no añade nada al pulsar ↵.
        pulsar(&mut formulario, KeyCode::Enter);
        escribir(&mut formulario, "zzz");
        pulsar(&mut formulario, KeyCode::Enter);
        assert!(!formulario.desplegable_hosts().abierto);
        assert_eq!(formulario.hosts_elegidos().len(), 2);
        // ← → marcan; x, Supr y ⌫ quitan el marcado.
        pulsar(&mut formulario, KeyCode::Right);
        assert_eq!(formulario.host_marcado(), 1);
        pulsar(&mut formulario, KeyCode::Char('x'));
        assert_eq!(formulario.hosts_elegidos(), &[(2, "beta".to_string())]);
        assert_eq!(formulario.host_marcado(), 0);
        pulsar(&mut formulario, KeyCode::Delete);
        assert!(formulario.hosts_elegidos().is_empty());
        pulsar(&mut formulario, KeyCode::Enter);
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(formulario.hosts_elegidos(), &[(1, "alfa".to_string())]);
        pulsar(&mut formulario, KeyCode::Backspace);
        assert!(formulario.hosts_elegidos().is_empty());
        assert!(formulario.datos().destinos.is_empty());
    }

    #[test]
    fn los_hosts_elegidos_van_a_los_destinos() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::DestinosEtiqueta);
        escribir(&mut formulario, "WEB");
        ir_a(&mut formulario, CampoSnippet::DestinosHost);
        pulsar(&mut formulario, KeyCode::Enter);
        pulsar(&mut formulario, KeyCode::Down);
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(
            formulario.datos().destinos,
            vec![
                Destino::Etiqueta("web".to_string()),
                Destino::Host {
                    id: 2,
                    nombre: "beta".to_string()
                }
            ]
        );
    }

    #[test]
    fn esc_con_el_desplegable_abierto_solo_lo_cierra() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::DestinosHost);
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Esc),
            AccionFormulario::Nada
        );
        assert!(!formulario.desplegable_hosts().abierto);
        assert!(!formulario.confirmando_descarte());
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Esc),
            AccionFormulario::Cancelar
        );
    }

    #[test]
    fn ctrl_s_con_el_nombre_vacio_da_error_y_no_guarda() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::Comando);
        escribir(&mut formulario, "uptime");
        ir_a(&mut formulario, CampoSnippet::DestinosEtiqueta);
        escribir(&mut formulario, "web");
        assert_eq!(formulario.manejar_tecla(&ctrl('s')), AccionFormulario::Nada);
        assert_eq!(
            formulario.error.as_deref(),
            Some("el snippet necesita un nombre")
        );
        // Moverse no retira el aviso; escribir, sí.
        pulsar(&mut formulario, KeyCode::Tab);
        assert!(formulario.error.is_some());
        ir_a(&mut formulario, CampoSnippet::Nombre);
        escribir(&mut formulario, "x");
        assert!(formulario.error.is_none());
    }

    #[test]
    fn ctrl_s_sin_destinos_da_error() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        escribir(&mut formulario, "x");
        ir_a(&mut formulario, CampoSnippet::Comando);
        escribir(&mut formulario, "uptime");
        assert_eq!(formulario.manejar_tecla(&ctrl('s')), AccionFormulario::Nada);
        assert!(formulario.error.as_deref().unwrap().contains("destino"));
    }

    #[test]
    fn ctrl_s_con_datos_validos_guarda_lo_normalizado() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        escribir(&mut formulario, "  reiniciar nginx ");
        ir_a(&mut formulario, CampoSnippet::Comando);
        escribir(&mut formulario, "systemctl restart nginx\n");
        ir_a(&mut formulario, CampoSnippet::Etiquetas);
        escribir(&mut formulario, "Web servicios web");
        ir_a(&mut formulario, CampoSnippet::DestinosEtiqueta);
        escribir(&mut formulario, "web nadie");
        assert_eq!(
            formulario.manejar_tecla(&ctrl('s')),
            AccionFormulario::Guardar
        );
        assert!(formulario.error.is_none());
        let datos = formulario.datos();
        assert_eq!(datos.nombre, "reiniciar nginx");
        assert_eq!(datos.comando, "systemctl restart nginx");
        assert_eq!(datos.etiquetas, vec!["servicios", "web"]);
        assert_eq!(
            datos.destinos,
            vec![
                Destino::Etiqueta("web".to_string()),
                Destino::Etiqueta("nadie".to_string())
            ]
        );
        assert_eq!(datos.timeout_seg, crate::snippets::TIMEOUT_DEFECTO);
    }

    #[test]
    fn ctrl_s_vale_con_el_desplegable_abierto() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        rellenar_minimo(&mut formulario);
        ir_a(&mut formulario, CampoSnippet::DestinosHost);
        pulsar(&mut formulario, KeyCode::Enter);
        assert!(formulario.desplegable_hosts().abierto);
        assert_eq!(
            formulario.manejar_tecla(&ctrl('s')),
            AccionFormulario::Guardar
        );
        assert!(!formulario.desplegable_hosts().abierto);
    }

    #[test]
    fn una_variable_entre_comillas_no_se_guarda() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        rellenar_minimo(&mut formulario);
        ir_a(&mut formulario, CampoSnippet::Comando);
        escribir(&mut formulario, " '{{x}}'");
        assert_eq!(formulario.manejar_tecla(&ctrl('s')), AccionFormulario::Nada);
        assert!(formulario.error.is_some());
    }

    #[test]
    fn esc_con_cambios_pide_confirmacion() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        escribir(&mut formulario, "x");
        assert!(formulario.sucio());
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Esc),
            AccionFormulario::Nada
        );
        assert!(formulario.confirmando_descarte());
        // Mientras pregunta, las demás teclas no escriben.
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Char('q')),
            AccionFormulario::Nada
        );
        assert_eq!(formulario.nombre().texto, "x");
        // n sigue editando.
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Char('n')),
            AccionFormulario::Nada
        );
        assert!(!formulario.confirmando_descarte());
        escribir(&mut formulario, "y");
        assert_eq!(formulario.nombre().texto, "xy");
        // Esc también sigue editando.
        pulsar(&mut formulario, KeyCode::Esc);
        assert!(formulario.confirmando_descarte());
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Esc),
            AccionFormulario::Nada
        );
        assert!(!formulario.confirmando_descarte());
        // s descarta.
        pulsar(&mut formulario, KeyCode::Esc);
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Char('s')),
            AccionFormulario::Cancelar
        );
    }

    #[test]
    fn esc_sin_cambios_cancela() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        assert!(!formulario.sucio());
        assert_eq!(
            pulsar(&mut formulario, KeyCode::Esc),
            AccionFormulario::Cancelar
        );
        let mut edicion = FormularioSnippet::editar(&snippet(), &hosts(), &[]);
        pulsar(&mut edicion, KeyCode::Tab);
        assert_eq!(
            pulsar(&mut edicion, KeyCode::Esc),
            AccionFormulario::Cancelar
        );
    }

    #[test]
    fn editar_conserva_los_datos_del_snippet() {
        let snippet = snippet();
        let formulario = FormularioSnippet::editar(&snippet, &hosts(), &[]);
        assert_eq!(formulario.id, Some(7));
        assert_eq!(formulario.titulo(), "EDITAR SNIPPET · limpiar journald");
        assert_eq!(formulario.datos(), snippet.datos());
        assert_eq!(formulario.comando().lineas.len(), 2);
        assert_eq!(formulario.destinos_etiqueta().texto, "web");
        assert_eq!(formulario.hosts_elegidos(), &[(2, "beta".to_string())]);
        assert!(formulario.critico());
        assert!(formulario.parar_al_fallo());
        assert_eq!(formulario.timeout().texto, "120");
        assert!(!formulario.sucio());
        let mut formulario = formulario;
        assert_eq!(
            formulario.manejar_tecla(&ctrl('s')),
            AccionFormulario::Guardar
        );
        assert_eq!(formulario.datos(), snippet.datos());
        assert_eq!(
            FormularioSnippet::nuevo(&hosts(), &[]).titulo(),
            "NUEVO SNIPPET"
        );
    }

    #[test]
    fn sucio_compara_con_lo_que_se_abrio() {
        let mut formulario = FormularioSnippet::editar(&snippet(), &hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::Descripcion);
        escribir(&mut formulario, "!");
        assert!(formulario.sucio());
        pulsar(&mut formulario, KeyCode::Backspace);
        assert!(!formulario.sucio());
        // Espacios al final del nombre no son un cambio (se normalizan).
        ir_a(&mut formulario, CampoSnippet::Nombre);
        escribir(&mut formulario, "  ");
        assert!(!formulario.sucio());
        ir_a(&mut formulario, CampoSnippet::Critico);
        pulsar(&mut formulario, KeyCode::Char(' '));
        assert!(formulario.sucio());
        pulsar(&mut formulario, KeyCode::Char(' '));
        assert!(!formulario.sucio());
        ir_a(&mut formulario, CampoSnippet::DestinosHost);
        pulsar(&mut formulario, KeyCode::Char('x'));
        assert!(formulario.sucio());
    }

    #[test]
    fn autocompleta_las_etiquetas_de_host() {
        let etiquetas = vec!["DB".to_string(), "backup".to_string()];
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &etiquetas);
        ir_a(&mut formulario, CampoSnippet::DestinosEtiqueta);
        // Sin palabra empezada se ofrecen todas.
        assert_eq!(
            formulario.sugerencias(),
            vec!["backup", "db", "web", "web-front"]
        );
        escribir(&mut formulario, "w");
        assert_eq!(formulario.sugerencias(), vec!["web", "web-front"]);
        pulsar(&mut formulario, KeyCode::Down);
        assert_eq!(formulario.indice_sugerencia(), 1);
        pulsar(&mut formulario, KeyCode::Down);
        assert_eq!(formulario.indice_sugerencia(), 1);
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(formulario.destinos_etiqueta().texto, "web-front ");
        // Las ya puestas no se vuelven a ofrecer.
        assert_eq!(formulario.sugerencias(), vec!["backup", "db", "web"]);
        // → al final de una palabra empezada también acepta.
        escribir(&mut formulario, "d");
        pulsar(&mut formulario, KeyCode::Right);
        assert_eq!(formulario.destinos_etiqueta().texto, "web-front db ");
        // La palabra exacta no se sugiere a sí misma.
        escribir(&mut formulario, "web");
        assert!(formulario.sugerencias().is_empty());
        assert_eq!(
            formulario.datos().destinos,
            vec![
                Destino::Etiqueta("web-front".to_string()),
                Destino::Etiqueta("db".to_string()),
                Destino::Etiqueta("web".to_string()),
            ]
        );
    }

    #[test]
    fn aceptar_en_mitad_del_texto_sustituye_la_palabra() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        ir_a(&mut formulario, CampoSnippet::DestinosEtiqueta);
        escribir(&mut formulario, "d web");
        for _ in 0..4 {
            pulsar(&mut formulario, KeyCode::Left);
        }
        assert_eq!(formulario.sugerencias(), vec!["db"]);
        // En mitad del texto, → mueve el cursor; ↵ acepta y sustituye la
        // palabra entera.
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(formulario.destinos_etiqueta().texto, "db web");
        assert_eq!(formulario.destinos_etiqueta().cursor, 3);
    }

    #[test]
    fn la_vista_previa_resuelve_con_los_hosts_fijados() {
        let mut formulario = FormularioSnippet::nuevo(&hosts(), &[]);
        assert_eq!(formulario.vista_previa(), VistaPreviaDestinos::SinDestinos);
        ir_a(&mut formulario, CampoSnippet::DestinosEtiqueta);
        escribir(&mut formulario, "nadie");
        assert_eq!(
            formulario.vista_previa(),
            VistaPreviaDestinos::Hosts(Vec::new())
        );
        for _ in 0..5 {
            pulsar(&mut formulario, KeyCode::Backspace);
        }
        escribir(&mut formulario, "web");
        ir_a(&mut formulario, CampoSnippet::DestinosHost);
        pulsar(&mut formulario, KeyCode::Enter);
        pulsar(&mut formulario, KeyCode::Down);
        pulsar(&mut formulario, KeyCode::Enter);
        assert_eq!(
            formulario.vista_previa(),
            VistaPreviaDestinos::Hosts(vec![
                "alfa".to_string(),
                "beta".to_string(),
                "gamma".to_string()
            ])
        );
    }

    #[test]
    fn detecta_las_variables_del_comando() {
        let formulario = FormularioSnippet::editar(&snippet(), &hosts(), &[]);
        let variables = formulario.variables();
        assert_eq!(variables.len(), 2);
        assert_eq!(variables[0].nombre, "dias");
        assert_eq!(variables[0].defecto.as_deref(), Some("7"));
        assert_eq!(variables[1].nombre, "unidad");
        assert_eq!(variables[1].defecto, None);
    }

    #[test]
    fn sin_hosts_el_desplegable_no_se_abre() {
        let mut formulario = FormularioSnippet::nuevo(&[], &[]);
        ir_a(&mut formulario, CampoSnippet::DestinosHost);
        pulsar(&mut formulario, KeyCode::Enter);
        assert!(!formulario.desplegable_hosts().abierto);
        pulsar(&mut formulario, KeyCode::Char('x'));
        assert!(formulario.hosts_elegidos().is_empty());
    }
}
