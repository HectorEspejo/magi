//! Lanzar un snippet (Fase 6): del snippet guardado a una ejecución resuelta.
//! Destinos resueltos, diálogo EJECUTAR (hosts con casillas, variables y
//! «parar al primer fallo»), sustitución escapada de variables, decisión de
//! deliberación y lanzamiento en el servidor (`LanzarEjecucion`) o en
//! pestañas (`p`, con `AbrirSesion.comandos_iniciales`). También el
//! «snippet al conectar», los atajos de Flota y `!` en Hosts y Flota.
//!
//! Todo diálogo lleva fijado lo que va a ejecutar (snippet, hosts, valores)
//! desde que se abre (T33): no se relee el estado al confirmar.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::modelo::{Grupo, Host};
use crate::protocolo::{ComandoInicial, DeliberacionLanzada, EstadoSesionRemota, MensajeCliente};
use crate::snippets::variables::{self, ErrorSustitucion, Variable};
use crate::snippets::Snippet;
use crate::ui::componentes::CampoTexto;
use crate::ui::snippets::{limpio, partir};

use super::{AccionDialogo, AccionPaleta, App, Dialogo, EntradaPaleta};

/// Con más pestañas que estas, `p` pide confirmación (antes de deliberar).
pub const MAX_PESTANAS_SIN_CONFIRMAR: usize = 5;

/// Columnas de la rejilla de hosts del diálogo EJECUTAR, como mucho: con
/// menos ancho se pintan menos (`ui::ejecutar::columnas_rejilla`) y las
/// flechas usan las del último pintado.
pub const COLUMNAS_HOSTS: usize = 3;

/// Plazo del seguimiento de una pestaña abierta con `p`: pasado, cuenta como
/// fallida (los diálogos de la apertura esperan como mucho 5 min).
const PLAZO_SEGUIMIENTO: Duration = Duration::from_secs(6 * 60);

/// Ancho útil de las líneas del diálogo de confirmación (66 de ancho menos
/// bordes y márgenes): el modal no parte las líneas largas.
const ANCHO_CONFIRMACION: usize = 58;

/// Líneas de hosts que enseña como mucho la confirmación de `p`.
const LINEAS_HOSTS_CONFIRMACION: usize = 3;

/// Dónde se ejecuta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoLanzamiento {
    /// En el servidor, sin terminal, con la salida capturada.
    Servidor,
    /// Una pestaña por host con el comando escrito al abrir la shell.
    Pestanas,
}

/// Una ejecución ya resuelta: comando sustituido y hosts elegidos.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanEjecucion {
    pub snippet_id: i64,
    pub nombre: String,
    /// El comando con las variables ya sustituidas (escapadas).
    pub comando: String,
    pub hosts: Vec<(i64, String)>,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub critico: bool,
    /// Valores de las variables, para repetir la ejecución.
    pub valores: BTreeMap<String, String>,
    pub modo: ModoLanzamiento,
}

impl PlanEjecucion {
    /// «reiniciar nginx → hetzner-01, hetzner-02» (la acción de la
    /// deliberación), con «(en pestaña)» si es `p`.
    pub fn accion(&self) -> String {
        let hosts: Vec<&str> = self
            .hosts
            .iter()
            .map(|(_, nombre)| nombre.as_str())
            .collect();
        let mut accion = format!("{} → {}", self.nombre, hosts.join(", "));
        if self.modo == ModoLanzamiento::Pestanas {
            accion.push_str(" (en pestaña)");
        }
        accion
    }
}

/// De dónde viene la orden de ejecutar.
#[derive(Debug, Clone, PartialEq)]
pub enum OrigenLanzamiento {
    /// `↵` en F8 o `snippet · <nombre>` en la paleta: diálogo EJECUTAR.
    Dialogo,
    /// `a`: todos los destinos, sin diálogo de hosts (sí de variables).
    Todos,
    /// `p`: abrir en pestaña.
    Pestanas,
    /// Solo este host (`snippet · <nombre> · <host>`, `!`, atajo de Flota).
    Host(i64),
    /// `r` en Resultados: mismos hosts y valores (si se conocen).
    Repetir {
        host_ids: Vec<i64>,
        valores: Option<BTreeMap<String, String>>,
    },
}

// ---------------------------------------------------------------- diálogo

/// Zona del diálogo EJECUTAR con el foco (`Tab` / `Shift+Tab` las recorren).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocoEjecutar {
    /// La rejilla de hosts (solo si se pueden elegir).
    Hosts,
    /// El campo de la variable con ese índice.
    Variable(usize),
    /// La casilla «parar al primer fallo» (solo en el servidor).
    Parar,
}

/// Lo que pide una tecla del diálogo EJECUTAR.
#[derive(Debug, Clone, PartialEq)]
pub enum AccionEjecutar {
    Nada,
    Cancelar,
    /// `↵` con todo en regla: el plan ya armado.
    Continuar(PlanEjecucion),
}

/// Diálogo EJECUTAR (maqueta §6.3).
pub struct DialogoEjecutar {
    pub snippet_id: i64,
    pub nombre: String,
    /// Datos fijados y campos, en caja: el diálogo viaja dentro de `Dialogo`
    /// y no debe agrandar el enum.
    estado: Box<EstadoEjecutar>,
}

/// Lo que el diálogo fija al abrirse y lo que edita el usuario.
struct EstadoEjecutar {
    /// El comando con las variables sin sustituir.
    comando: String,
    critico: bool,
    timeout_seg: u32,
    modo: ModoLanzamiento,
    /// Hosts resueltos al abrir (id y nombre), en el orden de la vista Hosts.
    hosts: Vec<(i64, String)>,
    marcados: Vec<bool>,
    /// `false` en el modo «solo variables»: los hosts vienen fijados.
    hosts_editables: bool,
    cursor_host: usize,
    variables: Vec<Variable>,
    /// Un campo por variable, en el orden de `variables`.
    valores: Vec<CampoTexto>,
    parar_al_fallo: bool,
    foco: FocoEjecutar,
    error: Option<String>,
}

impl DialogoEjecutar {
    /// Diálogo completo (`↵`, paleta): todos los hosts marcados, variables
    /// con su defecto y «parar al primer fallo» con el valor del snippet.
    pub fn completo(snippet: &Snippet, hosts: Vec<(i64, String)>) -> Self {
        Self::nuevo(
            snippet,
            hosts,
            true,
            ModoLanzamiento::Servidor,
            &BTreeMap::new(),
        )
    }

    /// Modo «solo variables» (`a`, `p`, un solo host, repetir): los hosts
    /// van fijos; los valores conocidos se precargan y el resto, su defecto.
    pub fn solo_variables(
        snippet: &Snippet,
        hosts: Vec<(i64, String)>,
        modo: ModoLanzamiento,
        conocidos: &BTreeMap<String, String>,
    ) -> Self {
        Self::nuevo(snippet, hosts, false, modo, conocidos)
    }

    fn nuevo(
        snippet: &Snippet,
        hosts: Vec<(i64, String)>,
        hosts_editables: bool,
        modo: ModoLanzamiento,
        conocidos: &BTreeMap<String, String>,
    ) -> Self {
        let variables = variables::detectar(&snippet.comando);
        let valores = variables
            .iter()
            .map(|variable| {
                CampoTexto::nuevo(
                    conocidos
                        .get(&variable.nombre)
                        .filter(|valor| !valor.is_empty())
                        .cloned()
                        .or_else(|| variable.defecto.clone())
                        .unwrap_or_default(),
                )
            })
            .collect();
        let foco = if hosts_editables && !hosts.is_empty() {
            FocoEjecutar::Hosts
        } else if !variables.is_empty() {
            FocoEjecutar::Variable(0)
        } else {
            FocoEjecutar::Parar
        };
        let marcados = vec![true; hosts.len()];
        Self {
            snippet_id: snippet.id,
            nombre: snippet.nombre.clone(),
            estado: Box::new(EstadoEjecutar {
                comando: snippet.comando.clone(),
                critico: snippet.critico,
                timeout_seg: snippet.timeout_seg,
                modo,
                hosts,
                marcados,
                hosts_editables,
                cursor_host: 0,
                variables,
                valores,
                parar_al_fallo: snippet.parar_al_fallo,
                foco,
                error: None,
            }),
        }
    }

    // ------------------------------------------------------------ lectura

    pub fn hosts(&self) -> &[(i64, String)] {
        &self.estado.hosts
    }

    pub fn marcado(&self, indice: usize) -> bool {
        self.estado.marcados.get(indice).copied().unwrap_or(false)
    }

    pub fn marcados(&self) -> usize {
        self.estado
            .marcados
            .iter()
            .filter(|marcado| **marcado)
            .count()
    }

    pub fn hosts_editables(&self) -> bool {
        self.estado.hosts_editables
    }

    pub fn cursor_host(&self) -> usize {
        self.estado.cursor_host
    }

    pub fn variables(&self) -> &[Variable] {
        &self.estado.variables
    }

    pub fn valor(&self, indice: usize) -> Option<&CampoTexto> {
        self.estado.valores.get(indice)
    }

    pub fn parar_al_fallo(&self) -> bool {
        self.estado.parar_al_fallo
    }

    pub fn timeout_seg(&self) -> u32 {
        self.estado.timeout_seg
    }

    pub fn modo(&self) -> ModoLanzamiento {
        self.estado.modo
    }

    pub fn foco(&self) -> FocoEjecutar {
        self.estado.foco
    }

    pub fn error(&self) -> Option<&str> {
        self.estado.error.as_deref()
    }

    // ------------------------------------------------------------ teclas

    /// `↵` continúa desde cualquier campo; `Esc` cancela; `Tab` y
    /// `Shift+Tab` cambian de zona; `↑↓←→` mueven por la rejilla de hosts
    /// (de `columnas` columnas, las que se ven) y `espacio` marca.
    pub fn manejar_tecla(&mut self, tecla: &KeyEvent, columnas: usize) -> AccionEjecutar {
        match tecla.code {
            KeyCode::Esc => return AccionEjecutar::Cancelar,
            KeyCode::Enter => {
                return match self.continuar() {
                    Some(plan) => AccionEjecutar::Continuar(plan),
                    None => AccionEjecutar::Nada,
                };
            }
            _ => {}
        }
        // Un error ya enseñado se retira con la siguiente tecla.
        self.estado.error = None;
        if tecla.modifiers.contains(KeyModifiers::CONTROL) {
            return AccionEjecutar::Nada;
        }
        match tecla.code {
            KeyCode::BackTab => self.mover_zona(-1, true),
            KeyCode::Tab if tecla.modifiers.contains(KeyModifiers::SHIFT) => {
                self.mover_zona(-1, true)
            }
            KeyCode::Tab => self.mover_zona(1, true),
            _ => match self.estado.foco {
                FocoEjecutar::Hosts => self.tecla_hosts(tecla.code, columnas),
                FocoEjecutar::Variable(indice) => self.tecla_variable(indice, tecla),
                FocoEjecutar::Parar => self.tecla_parar(tecla.code),
            },
        }
        AccionEjecutar::Nada
    }

    /// Zonas que recorre `Tab`, en orden.
    fn zonas(&self) -> Vec<FocoEjecutar> {
        let mut zonas = Vec::new();
        if self.estado.hosts_editables && !self.estado.hosts.is_empty() {
            zonas.push(FocoEjecutar::Hosts);
        }
        zonas.extend((0..self.estado.variables.len()).map(FocoEjecutar::Variable));
        if self.estado.modo == ModoLanzamiento::Servidor {
            zonas.push(FocoEjecutar::Parar);
        }
        zonas
    }

    /// Mueve el foco `delta` zonas; con `envolver`, de la última a la
    /// primera (Tab) y, sin él, se queda en el borde (flechas).
    fn mover_zona(&mut self, delta: isize, envolver: bool) {
        let zonas = self.zonas();
        if zonas.is_empty() {
            return;
        }
        let total = zonas.len() as isize;
        let actual = zonas
            .iter()
            .position(|zona| *zona == self.estado.foco)
            .unwrap_or(0) as isize;
        let siguiente = if envolver {
            (actual + delta).rem_euclid(total)
        } else {
            (actual + delta).clamp(0, total - 1)
        };
        self.estado.foco = zonas[siguiente as usize];
    }

    /// Rejilla de `columnas` columnas: flechas y espacio.
    fn tecla_hosts(&mut self, codigo: KeyCode, columnas: usize) {
        let total = self.estado.hosts.len();
        if total == 0 {
            return;
        }
        let columnas = columnas.clamp(1, COLUMNAS_HOSTS);
        let cursor = self.estado.cursor_host.min(total - 1);
        match codigo {
            KeyCode::Left => self.estado.cursor_host = cursor.saturating_sub(1),
            KeyCode::Right => self.estado.cursor_host = (cursor + 1).min(total - 1),
            KeyCode::Up => {
                if cursor >= columnas {
                    self.estado.cursor_host = cursor - columnas;
                }
            }
            KeyCode::Down => {
                let fila = cursor / columnas;
                if fila + 1 < total.div_ceil(columnas) {
                    // En una última fila incompleta, al último host.
                    self.estado.cursor_host = (cursor + columnas).min(total - 1);
                } else {
                    // Bajar desde la última fila pasa a las variables.
                    self.estado.cursor_host = cursor;
                    self.mover_zona(1, false);
                }
            }
            KeyCode::Char(' ') => {
                if let Some(marcado) = self.estado.marcados.get_mut(cursor) {
                    *marcado = !*marcado;
                }
                self.estado.cursor_host = cursor;
            }
            _ => {}
        }
    }

    fn tecla_variable(&mut self, indice: usize, tecla: &KeyEvent) {
        match tecla.code {
            KeyCode::Up => self.mover_zona(-1, false),
            KeyCode::Down => self.mover_zona(1, false),
            _ => {
                if let Some(campo) = self.estado.valores.get_mut(indice) {
                    campo.manejar_tecla(tecla);
                }
            }
        }
    }

    fn tecla_parar(&mut self, codigo: KeyCode) {
        match codigo {
            KeyCode::Char(' ') => self.estado.parar_al_fallo = !self.estado.parar_al_fallo,
            KeyCode::Up => self.mover_zona(-1, false),
            _ => {}
        }
    }

    /// `↵`: arma el plan o deja el error en el diálogo (y el foco en el
    /// campo que falta).
    pub fn continuar(&mut self) -> Option<PlanEjecucion> {
        match self.armar_plan() {
            Ok(plan) => Some(plan),
            Err((mensaje, foco)) => {
                self.estado.error = Some(mensaje);
                if let Some(foco) = foco {
                    self.estado.foco = foco;
                }
                None
            }
        }
    }

    /// El plan con lo que hay ahora en el diálogo: al menos un host marcado,
    /// ninguna variable en un contexto que el escapado no protege y todas
    /// con valor (lo escrito o su defecto).
    fn armar_plan(&self) -> Result<PlanEjecucion, (String, Option<FocoEjecutar>)> {
        let estado = &self.estado;
        let hosts: Vec<(i64, String)> = estado
            .hosts
            .iter()
            .zip(&estado.marcados)
            .filter(|(_, marcado)| **marcado)
            .map(|(host, _)| host.clone())
            .collect();
        if hosts.is_empty() {
            let foco = estado.hosts_editables.then_some(FocoEjecutar::Hosts);
            return Err(("marca al menos un host".to_string(), foco));
        }
        // Lo que ningún valor arregla va primero.
        variables::validar_contexto(&estado.comando).map_err(|motivo| (motivo, None))?;
        let mut valores = BTreeMap::new();
        for (indice, variable) in estado.variables.iter().enumerate() {
            let escrito = estado
                .valores
                .get(indice)
                .map_or("", |campo| campo.texto.as_str());
            match variables::valor_efectivo(variable, escrito) {
                Ok(valor) => {
                    valores.insert(variable.nombre.clone(), valor);
                }
                Err(_) => {
                    return Err((
                        falta_valor(&variable.nombre),
                        Some(FocoEjecutar::Variable(indice)),
                    ));
                }
            }
        }
        let comando =
            variables::sustituir(&estado.comando, &valores).map_err(|error| match error {
                ErrorSustitucion::Vacia(nombre) => {
                    let foco = estado
                        .variables
                        .iter()
                        .position(|variable| variable.nombre == nombre)
                        .map(FocoEjecutar::Variable);
                    (falta_valor(&nombre), foco)
                }
                ErrorSustitucion::Contexto(motivo) => (motivo, None),
            })?;
        Ok(PlanEjecucion {
            snippet_id: self.snippet_id,
            nombre: self.nombre.clone(),
            comando,
            hosts,
            timeout_seg: estado.timeout_seg,
            parar_al_fallo: estado.parar_al_fallo,
            critico: estado.critico,
            valores,
            modo: estado.modo,
        })
    }
}

fn falta_valor(nombre: &str) -> String {
    format!("falta el valor de «{nombre}»")
}

// ---------------------------------------------------------------- preparación

/// Lo que hace falta para lanzar tras resolver destinos y origen.
pub enum Preparacion {
    /// Abrir el diálogo EJECUTAR (completo o «solo variables»).
    Dialogo(DialogoEjecutar),
    /// Todo se sabe: el plan ya armado.
    Plan(PlanEjecucion),
}

/// Por qué no se puede lanzar (se avisa y no pasa nada más).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvisoPreparacion {
    /// Los destinos no resuelven a ningún host.
    SinHosts,
    /// El host pedido no es destino del snippet.
    NoApunta(i64),
    /// Ninguno de los hosts a repetir sigue siendo destino.
    NingunoDeRepetir,
    /// El comando no se puede sustituir (variable entre comillas…).
    Comando(String),
}

/// De un snippet recién leído y sus hosts resueltos al diálogo o al plan,
/// según el origen. `↵` siempre abre el diálogo completo; el resto solo
/// pregunta si hay variables sin valor conocido (los defectos se precargan,
/// pero se confirman).
pub fn preparar(
    snippet: &Snippet,
    resueltos: Vec<(i64, String)>,
    origen: &OrigenLanzamiento,
) -> Result<Preparacion, AvisoPreparacion> {
    if resueltos.is_empty() {
        return Err(AvisoPreparacion::SinHosts);
    }
    let sin_valores = BTreeMap::new();
    let (hosts, modo, conocidos) = match origen {
        OrigenLanzamiento::Dialogo => {
            return Ok(Preparacion::Dialogo(DialogoEjecutar::completo(
                snippet, resueltos,
            )));
        }
        OrigenLanzamiento::Todos => (resueltos, ModoLanzamiento::Servidor, &sin_valores),
        OrigenLanzamiento::Pestanas => (resueltos, ModoLanzamiento::Pestanas, &sin_valores),
        OrigenLanzamiento::Host(host_id) => {
            let hosts: Vec<(i64, String)> = resueltos
                .into_iter()
                .filter(|(id, _)| id == host_id)
                .collect();
            if hosts.is_empty() {
                return Err(AvisoPreparacion::NoApunta(*host_id));
            }
            (hosts, ModoLanzamiento::Servidor, &sin_valores)
        }
        OrigenLanzamiento::Repetir { host_ids, valores } => {
            let hosts: Vec<(i64, String)> = resueltos
                .into_iter()
                .filter(|(id, _)| host_ids.contains(id))
                .collect();
            if hosts.is_empty() {
                return Err(AvisoPreparacion::NingunoDeRepetir);
            }
            (
                hosts,
                ModoLanzamiento::Servidor,
                valores.as_ref().unwrap_or(&sin_valores),
            )
        }
    };
    variables::validar_contexto(&snippet.comando).map_err(AvisoPreparacion::Comando)?;
    let faltan = variables::detectar(&snippet.comando)
        .iter()
        .any(|variable| {
            conocidos
                .get(&variable.nombre)
                .is_none_or(|valor| valor.is_empty())
        });
    let dialogo = DialogoEjecutar::solo_variables(snippet, hosts, modo, conocidos);
    if faltan {
        return Ok(Preparacion::Dialogo(dialogo));
    }
    dialogo
        .armar_plan()
        .map(Preparacion::Plan)
        .map_err(|(motivo, _)| AvisoPreparacion::Comando(motivo))
}

/// `p` con más de cinco hosts pide confirmación antes de deliberar.
pub fn requiere_confirmacion(plan: &PlanEjecucion) -> bool {
    plan.modo == ModoLanzamiento::Pestanas && plan.hosts.len() > MAX_PESTANAS_SIN_CONFIRMAR
}

/// Líneas de la confirmación de `p` (saneadas y partidas al ancho).
fn lineas_confirmacion_pestanas(plan: &PlanEjecucion) -> Vec<String> {
    let mut lineas = partir(
        &limpio(&format!(
            "Se abrirán {} pestañas, una por host, con «{}» escrito al abrir la shell:",
            plan.hosts.len(),
            plan.nombre
        )),
        ANCHO_CONFIRMACION,
    );
    let nombres: Vec<String> = plan
        .hosts
        .iter()
        .map(|(_, nombre)| limpio(nombre))
        .collect();
    let mut hosts = partir(&nombres.join(", "), ANCHO_CONFIRMACION);
    if hosts.len() > LINEAS_HOSTS_CONFIRMACION {
        hosts.truncate(LINEAS_HOSTS_CONFIRMACION);
        if let Some(ultima) = hosts.last_mut() {
            let mut recortada: String = ultima
                .chars()
                .take(ANCHO_CONFIRMACION.saturating_sub(1))
                .collect();
            recortada.push('…');
            *ultima = recortada;
        }
    }
    lineas.extend(hosts);
    lineas
}

/// El «snippet al conectar» como comando inicial, si sigue siendo apto (no
/// crítico y sin variables): se repite al reconectar.
pub fn comando_al_conectar(snippet: &Snippet) -> Option<ComandoInicial> {
    snippet.apto_al_conectar().then(|| ComandoInicial {
        texto: crate::snippets::texto_comando_inicial(&snippet.comando),
        repetir: true,
    })
}

/// Comandos iniciales de una pestaña de `p`: el snippet al conectar del host
/// (se repite al reconectar) y después el del plan, una sola vez.
pub fn comandos_de_pestana(
    mut al_conectar: Vec<ComandoInicial>,
    comando: &str,
) -> Vec<ComandoInicial> {
    al_conectar.push(ComandoInicial {
        texto: crate::snippets::texto_comando_inicial(comando),
        repetir: false,
    });
    al_conectar
}

/// Etiquetas de la paleta: `snippet · <nombre>` y `snippet · <nombre> · <host>`.
fn etiqueta_snippet(nombre: &str) -> String {
    format!("snippet · {}", limpio(nombre))
}

fn etiqueta_snippet_en_host(nombre: &str, host: &str) -> String {
    format!("snippet · {} · {}", limpio(nombre), limpio(host))
}

/// Entradas de la paleta general: cada snippet y cada snippet en cada uno de
/// sus hosts resueltos.
fn entradas_de_snippets(
    snippets: &[Snippet],
    hosts: &[Host],
    grupos: &[Grupo],
) -> Vec<(String, AccionPaletaLanzar)> {
    let mut entradas = Vec::new();
    for snippet in snippets {
        entradas.push((
            etiqueta_snippet(&snippet.nombre),
            AccionPaletaLanzar::Ejecutar(snippet.id),
        ));
        for host in crate::snippets::resolver(&snippet.destinos, hosts, grupos) {
            entradas.push((
                etiqueta_snippet_en_host(&snippet.nombre, &host.nombre),
                AccionPaletaLanzar::EjecutarEnHost {
                    snippet_id: snippet.id,
                    host_id: host.id,
                },
            ));
        }
    }
    entradas
}

/// Entradas de `!`: solo los snippets que apuntan a ese host, para
/// ejecutarlos únicamente en él.
fn entradas_de_host(
    snippets: &[Snippet],
    hosts: &[Host],
    grupos: &[Grupo],
    host: &Host,
) -> Vec<(String, AccionPaletaLanzar)> {
    snippets
        .iter()
        .filter(|snippet| crate::snippets::apunta_a(&snippet.destinos, hosts, grupos, host.id))
        .map(|snippet| {
            (
                etiqueta_snippet_en_host(&snippet.nombre, &host.nombre),
                AccionPaletaLanzar::EjecutarEnHost {
                    snippet_id: snippet.id,
                    host_id: host.id,
                },
            )
        })
        .collect()
}

/// `[flota.atajos]` contra los snippets que existen.
fn validar_atajos_contra(
    atajos: &BTreeMap<String, String>,
    snippets: &[Snippet],
) -> (Vec<(char, String)>, Vec<String>) {
    let nombres: Vec<String> = snippets
        .iter()
        .map(|snippet| snippet.nombre.clone())
        .collect();
    crate::config::validar_atajos(atajos, &nombres)
}

/// Snippet del atajo de esa tecla, si lo es.
fn atajo_de(atajos: &[(char, String)], tecla: char) -> Option<&str> {
    atajos
        .iter()
        .find(|(caracter, _)| *caracter == tecla)
        .map(|(_, nombre)| nombre.as_str())
}

/// Texto del error al leer un snippet por id.
fn error_al_leer(error: &anyhow::Error) -> String {
    if matches!(
        error.downcast_ref::<rusqlite::Error>(),
        Some(rusqlite::Error::QueryReturnedNoRows)
    ) {
        "el snippet ya no existe".to_string()
    } else {
        format!("no se pudo leer el snippet: {error}")
    }
}

/// Acciones confirmadas del lanzamiento (viajan en `AccionDialogo::Lanzar`).
pub enum AccionLanzar {
    /// `p` con más de cinco hosts, ya confirmado.
    AbrirEnPestanas(PlanEjecucion),
}

/// Entradas de la paleta de ejecución.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccionPaletaLanzar {
    /// `snippet · <nombre>`: diálogo EJECUTAR.
    Ejecutar(i64),
    /// `snippet · <nombre> · <host>`: solo en ese host.
    EjecutarEnHost { snippet_id: i64, host_id: i64 },
}

// ---------------------------------------------------------------- seguimiento

/// Una pestaña pedida con `p` que aún no se sabe si abrió.
#[derive(Debug, Clone)]
struct SeguimientoPestana {
    /// Grupo del lanzamiento, si lo autorizó una deliberación.
    grupo: Option<u64>,
    deliberacion_id: Option<i64>,
    host_id: i64,
    host_nombre: String,
    snippet: String,
    /// Sesiones del host que ya existían al pedirla: la nueva es otra.
    existentes: HashSet<u32>,
    /// La sesión nueva, en cuanto aparece.
    sesion_id: Option<u32>,
    desde: Instant,
}

/// Las pestañas de un lanzamiento deliberado: cuando todas se resuelven, se
/// cierra su fila de `DELIBERACIONES` (el servidor no la ve).
#[derive(Debug, Clone)]
struct GrupoPestanas {
    deliberacion: DeliberacionLanzada,
    /// Hosts del plan (los que no se pudieron ni pedir cuentan como fallidos).
    total: usize,
    pendientes: usize,
    abiertas: usize,
}

/// Lo que se sabe de una pestaña para el seguimiento.
#[derive(Debug, Clone, Copy)]
struct PestanaVista {
    sesion_id: u32,
    host_id: i64,
    estado: EstadoSesionRemota,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EstadoSeguimiento {
    Pendiente,
    Abierta,
    Fallida,
}

/// Un grupo deliberado ya resuelto entero.
#[derive(Debug, Clone)]
struct CierreGrupo {
    deliberacion: DeliberacionLanzada,
    abiertas: usize,
    total: usize,
}

/// Resultado de una pasada del seguimiento.
#[derive(Debug, Default)]
struct Revision {
    /// Seguimientos resueltos: `true` si su pestaña abrió.
    resueltos: Vec<(SeguimientoPestana, bool)>,
    cierres: Vec<CierreGrupo>,
}

/// Estado de un seguimiento con las pestañas de ahora. Se ata a la primera
/// sesión nueva del host que nadie más reclama; abre si esa sesión llega a
/// `Abierta`, falla si desaparece, cae o no aparece y el host ya no tiene
/// apertura en curso, o si vence el plazo.
fn evaluar(
    seguimiento: &mut SeguimientoPestana,
    pestanas: &[PestanaVista],
    reclamadas: &HashSet<u32>,
    apertura_pendiente: bool,
    ahora: Instant,
) -> EstadoSeguimiento {
    if seguimiento.sesion_id.is_none() {
        seguimiento.sesion_id = pestanas
            .iter()
            .find(|pestana| {
                pestana.host_id == seguimiento.host_id
                    && !seguimiento.existentes.contains(&pestana.sesion_id)
                    && !reclamadas.contains(&pestana.sesion_id)
            })
            .map(|pestana| pestana.sesion_id);
    }
    match seguimiento.sesion_id {
        Some(sesion_id) => match pestanas
            .iter()
            .find(|pestana| pestana.sesion_id == sesion_id)
            .map(|pestana| pestana.estado)
        {
            Some(EstadoSesionRemota::Abierta) => return EstadoSeguimiento::Abierta,
            Some(EstadoSesionRemota::Abriendo) => {}
            _ => return EstadoSeguimiento::Fallida,
        },
        None if apertura_pendiente => {}
        None => return EstadoSeguimiento::Fallida,
    }
    if ahora.saturating_duration_since(seguimiento.desde) >= PLAZO_SEGUIMIENTO {
        EstadoSeguimiento::Fallida
    } else {
        EstadoSeguimiento::Pendiente
    }
}

/// Estado del lanzamiento que vive en `App`.
#[derive(Default)]
pub struct EstadoLanzar {
    /// Atajos de Flota válidos: tecla → nombre de snippet.
    pub atajos: Vec<(char, String)>,
    seguimientos: Vec<SeguimientoPestana>,
    grupos: HashMap<u64, GrupoPestanas>,
    siguiente_grupo: u64,
    /// Aviso que se enseña en el siguiente tick: el de «snippet al conectar»
    /// se da justo antes de que `abrir_sesion_con` pise el mensaje.
    aviso_diferido: Option<String>,
}

impl EstadoLanzar {
    fn nuevo_grupo(&mut self) -> u64 {
        self.siguiente_grupo += 1;
        self.siguiente_grupo
    }

    fn diferir_aviso(&mut self, aviso: String) {
        match &mut self.aviso_diferido {
            Some(previo) if *previo != aviso => {
                previo.push_str(" · ");
                previo.push_str(&aviso);
            }
            Some(_) => {}
            None => self.aviso_diferido = Some(aviso),
        }
    }

    /// Una pasada del seguimiento: resuelve lo que se pueda y cierra los
    /// grupos deliberados que ya no tienen pestañas pendientes.
    fn revisar(
        &mut self,
        pestanas: &[PestanaVista],
        aperturas_pendientes: &HashSet<i64>,
        ahora: Instant,
    ) -> Revision {
        let mut reclamadas: HashSet<u32> = self
            .seguimientos
            .iter()
            .filter_map(|seguimiento| seguimiento.sesion_id)
            .collect();
        let mut revision = Revision::default();
        let mut siguen = Vec::new();
        for mut seguimiento in std::mem::take(&mut self.seguimientos) {
            let apertura_pendiente = aperturas_pendientes.contains(&seguimiento.host_id);
            let estado = evaluar(
                &mut seguimiento,
                pestanas,
                &reclamadas,
                apertura_pendiente,
                ahora,
            );
            if let Some(sesion_id) = seguimiento.sesion_id {
                reclamadas.insert(sesion_id);
            }
            match estado {
                EstadoSeguimiento::Pendiente => siguen.push(seguimiento),
                EstadoSeguimiento::Abierta => revision.resueltos.push((seguimiento, true)),
                EstadoSeguimiento::Fallida => revision.resueltos.push((seguimiento, false)),
            }
        }
        self.seguimientos = siguen;
        for (seguimiento, abierta) in &revision.resueltos {
            let Some(clave) = seguimiento.grupo else {
                continue;
            };
            let Some(grupo) = self.grupos.get_mut(&clave) else {
                continue;
            };
            grupo.pendientes = grupo.pendientes.saturating_sub(1);
            if *abierta {
                grupo.abiertas += 1;
            }
            if grupo.pendientes == 0 {
                if let Some(grupo) = self.grupos.remove(&clave) {
                    revision.cierres.push(CierreGrupo {
                        deliberacion: grupo.deliberacion,
                        abiertas: grupo.abiertas,
                        total: grupo.total,
                    });
                }
            }
        }
        revision
    }
}

impl App {
    /// Empieza a ejecutar un snippet desde cualquier origen: lo relee de la
    /// base (lo que se ejecuta queda fijado desde aquí), resuelve sus hosts
    /// y abre el diálogo EJECUTAR o arma el plan.
    pub(super) fn iniciar_snippet(&mut self, snippet_id: i64, origen: OrigenLanzamiento) {
        if self.avisar_si_no_hay_servidor() {
            return;
        }
        let snippet = match self.almacen.obtener_snippet(snippet_id) {
            Ok(snippet) => snippet,
            Err(error) => {
                self.mensaje(error_al_leer(&error), true);
                return;
            }
        };
        let resueltos: Vec<(i64, String)> =
            crate::snippets::resolver(&snippet.destinos, &self.hosts, &self.grupos)
                .into_iter()
                .map(|host| (host.id, host.nombre.clone()))
                .collect();
        match preparar(&snippet, resueltos, &origen) {
            Ok(Preparacion::Dialogo(dialogo)) => {
                self.dialogo = Some(Dialogo::Ejecutar(dialogo));
            }
            Ok(Preparacion::Plan(plan)) => self.plan_listo(plan),
            Err(aviso) => {
                let texto = self.texto_aviso(&snippet.nombre, aviso);
                self.mensaje(texto, true);
            }
        }
    }

    fn texto_aviso(&self, snippet: &str, aviso: AvisoPreparacion) -> String {
        let snippet = limpio(snippet);
        match aviso {
            AvisoPreparacion::SinHosts => "el snippet no apunta a ningún host".to_string(),
            AvisoPreparacion::NoApunta(host_id) => {
                match self.hosts.iter().find(|host| host.id == host_id) {
                    Some(host) => format!("«{snippet}» no apunta a «{}»", limpio(&host.nombre)),
                    None => "ese host ya no existe".to_string(),
                }
            }
            AvisoPreparacion::NingunoDeRepetir => {
                format!("«{snippet}» ya no apunta a ninguno de esos hosts")
            }
            AvisoPreparacion::Comando(motivo) => format!("«{snippet}»: {motivo}"),
        }
    }

    /// Sin servidor (caído o de otra versión) no hay nada que lanzar.
    fn avisar_si_no_hay_servidor(&mut self) -> bool {
        if self.servidor_incompatible.is_some() {
            self.mensaje(
                "servidor de otra versión de protocolo: magi servidor parar y volver a abrir",
                true,
            );
            return true;
        }
        if self.servidor_caido {
            self.mensaje("el servidor de sesiones ha caído; relánzalo primero", true);
            return true;
        }
        false
    }

    /// Tecla con el diálogo EJECUTAR abierto (sacado del hueco: si sigue
    /// abierto, hay que devolverlo a `self.dialogo`).
    pub(super) fn tecla_dialogo_ejecutar(&mut self, mut dialogo: DialogoEjecutar, tecla: KeyEvent) {
        match dialogo.manejar_tecla(&tecla, self.columnas_rejilla()) {
            AccionEjecutar::Nada => self.dialogo = Some(Dialogo::Ejecutar(dialogo)),
            AccionEjecutar::Cancelar => {}
            AccionEjecutar::Continuar(plan) => self.plan_listo(plan),
        }
    }

    /// Columnas de la rejilla de hosts del diálogo EJECUTAR en el último
    /// pintado: las registradas en la disposición o, si el diálogo aún no
    /// las registró, las que salen del área pintada (solo dependen del
    /// ancho).
    #[doc(hidden)]
    pub fn columnas_rejilla(&self) -> usize {
        self.disposicion
            .columnas_rejilla
            .unwrap_or_else(|| crate::ui::ejecutar::columnas_rejilla(self.disposicion.area))
    }

    /// Plan armado: `p` con más de cinco hosts se confirma antes de deliberar.
    fn plan_listo(&mut self, plan: PlanEjecucion) {
        if requiere_confirmacion(&plan) {
            self.dialogo = Some(Dialogo::Confirmar {
                titulo: "ABRIR EN PESTAÑAS".to_string(),
                lineas: lineas_confirmacion_pestanas(&plan),
                peligro: true,
                accion: AccionDialogo::Lanzar(AccionLanzar::AbrirEnPestanas(plan)),
            });
        } else {
            self.continuar_plan(plan);
        }
    }

    /// Decide si hay deliberación y, si no, lanza.
    pub(super) fn continuar_plan(&mut self, plan: PlanEjecucion) {
        // Sin saber qué hosts piden verificaciones no se puede decidir si hay
        // que deliberar: no se lanza.
        let verificaciones = match self.almacen.verificaciones_por_host() {
            Ok(verificaciones) => verificaciones,
            Err(error) => {
                self.mensaje(
                    format!("no se pudieron leer las verificaciones previas: {error}"),
                    true,
                );
                return;
            }
        };
        let motivos =
            crate::snippets::motivos_deliberacion(plan.critico, &plan.hosts, &verificaciones);
        if motivos.is_empty() {
            self.lanzar_plan(plan, None);
        } else {
            self.abrir_deliberacion(plan, motivos);
        }
    }

    /// Lanza el plan (en el servidor o en pestañas), con la deliberación que
    /// lo autoriza si la hubo.
    pub(super) fn lanzar_plan(
        &mut self,
        plan: PlanEjecucion,
        deliberacion: Option<DeliberacionLanzada>,
    ) {
        match plan.modo {
            ModoLanzamiento::Servidor => self.lanzar_en_servidor(plan, deliberacion),
            ModoLanzamiento::Pestanas => self.abrir_en_pestanas(plan, deliberacion),
        }
    }

    fn lanzar_en_servidor(
        &mut self,
        plan: PlanEjecucion,
        deliberacion: Option<DeliberacionLanzada>,
    ) {
        if self.avisar_si_no_hay_servidor() {
            // La deliberación ya está en la base: nadie más la cerraría.
            if let Some(deliberacion) = &deliberacion {
                self.cerrar_deliberacion(
                    deliberacion,
                    crate::deliberacion::EjecucionResultado::Error,
                );
            }
            return;
        }
        let peticion_id =
            self.registrar_lanzamiento(plan.clone(), deliberacion.as_ref().map(|d| d.id));
        let hosts = plan.hosts.len();
        self.servidor.enviar(MensajeCliente::LanzarEjecucion {
            peticion_id,
            snippet_id: Some(plan.snippet_id),
            nombre: plan.nombre.clone(),
            comando: plan.comando,
            host_ids: plan.hosts.iter().map(|(id, _)| *id).collect(),
            timeout_seg: plan.timeout_seg,
            parar_al_fallo: plan.parar_al_fallo,
            deliberacion,
        });
        self.ir_a_resultados();
        self.mensaje(
            format!(
                "lanzando «{}» en {hosts} {}…",
                plan.nombre,
                if hosts == 1 { "host" } else { "hosts" }
            ),
            false,
        );
    }

    /// `p`: una pestaña por host con el comando escrito al abrir la shell.
    /// Cada una se sigue hasta que abre (anotación «en pestaña») o falla.
    fn abrir_en_pestanas(
        &mut self,
        plan: PlanEjecucion,
        deliberacion: Option<DeliberacionLanzada>,
    ) {
        let grupo = deliberacion.as_ref().map(|_| self.lanzar.nuevo_grupo());
        let mut pedidas = 0;
        for (host_id, host_nombre) in &plan.hosts {
            let existentes: HashSet<u32> = self
                .pestanas
                .iter()
                .filter(|pestana| pestana.host_id == *host_id)
                .map(|pestana| pestana.sesion_id)
                .collect();
            let comandos = comandos_de_pestana(self.comandos_al_conectar(*host_id), &plan.comando);
            if self.abrir_sesion_con(*host_id, comandos) {
                pedidas += 1;
                self.lanzar.seguimientos.push(SeguimientoPestana {
                    grupo,
                    deliberacion_id: deliberacion.as_ref().map(|d| d.id),
                    host_id: *host_id,
                    host_nombre: host_nombre.clone(),
                    snippet: plan.nombre.clone(),
                    existentes,
                    sesion_id: None,
                    desde: Instant::now(),
                });
            }
        }
        if let (Some(grupo), Some(deliberacion)) = (grupo, deliberacion) {
            if pedidas == 0 {
                self.cerrar_deliberacion(
                    &deliberacion,
                    crate::deliberacion::EjecucionResultado::Error,
                );
            } else {
                self.lanzar.grupos.insert(
                    grupo,
                    GrupoPestanas {
                        deliberacion,
                        total: plan.hosts.len(),
                        pendientes: pedidas,
                        abiertas: 0,
                    },
                );
            }
        }
        if pedidas == 0 {
            // `abrir_sesion_con` ya dijo por qué (servidor, apertura en curso).
            return;
        }
        if let Err(error) = self.almacen.marcar_uso_snippet(plan.snippet_id) {
            tracing::warn!("no se pudo marcar el uso del snippet: {error}");
        }
        self.recargar_snippets();
        let total = plan.hosts.len();
        let texto = if pedidas == total {
            format!(
                "«{}» en pestaña: abriendo {pedidas} {}",
                plan.nombre,
                if pedidas == 1 {
                    "pestaña"
                } else {
                    "pestañas"
                }
            )
        } else {
            format!(
                "«{}» en pestaña: abriendo {pedidas} de {total} (el resto ya tenía una conexión en curso)",
                plan.nombre
            )
        };
        self.mensaje(texto, pedidas < total);
    }

    /// Rellena `ejecucion_resultado` de una deliberación que no llegó al
    /// servidor (o que abrió pestañas) y anota `deliberacion_aprobada` o
    /// `deliberacion_forzada` con el resultado, como haría el servidor.
    fn cerrar_deliberacion(
        &mut self,
        deliberacion: &DeliberacionLanzada,
        resultado: crate::deliberacion::EjecucionResultado,
    ) {
        match self
            .almacen
            .fijar_resultado_deliberacion(deliberacion.id, resultado)
        {
            Ok(true) => {}
            // Ya la cerró otro: su anotación ya está.
            Ok(false) => return,
            Err(error) => {
                self.mensaje(
                    format!(
                        "no se pudo cerrar la deliberación #{}: {error}",
                        deliberacion.id
                    ),
                    true,
                );
                return;
            }
        }
        let registro = match self.almacen.obtener_deliberacion(deliberacion.id) {
            Ok(registro) => registro,
            Err(error) => {
                tracing::warn!(
                    "no se pudo leer la deliberación #{}: {error}",
                    deliberacion.id
                );
                return;
            }
        };
        self.anotar(
            if deliberacion.forzada {
                crate::registro::DELIBERACION_FORZADA
            } else {
                crate::registro::DELIBERACION_APROBADA
            },
            None,
            None,
            &crate::deliberacion::detalle_registro(&registro, Some(resultado)),
            if resultado == crate::deliberacion::EjecucionResultado::Ok {
                crate::modelo::ResultadoRegistro::Ok
            } else {
                crate::modelo::ResultadoRegistro::Error
            },
        );
    }

    pub(super) fn ejecutar_accion_lanzar(&mut self, accion: AccionLanzar) {
        match accion {
            AccionLanzar::AbrirEnPestanas(plan) => self.continuar_plan(plan),
        }
    }

    /// `snippet · <nombre>` y `snippet · <nombre> · <host>`.
    pub(super) fn entradas_paleta_lanzar(&self) -> Vec<EntradaPaleta> {
        // Recién leídos: otra ventana pudo crear o renombrar snippets.
        let leidos = self.almacen.listar_snippets().ok();
        let snippets = leidos.as_deref().unwrap_or(&self.snippets.lista);
        entradas_de_snippets(snippets, &self.hosts, &self.grupos)
            .into_iter()
            .map(|(etiqueta, accion)| EntradaPaleta {
                etiqueta,
                categoria: "snippets",
                accion: AccionPaleta::Lanzar(accion),
            })
            .collect()
    }

    pub(super) fn accion_paleta_lanzar(&mut self, accion: AccionPaletaLanzar) {
        match accion {
            AccionPaletaLanzar::Ejecutar(snippet_id) => {
                self.iniciar_snippet(snippet_id, OrigenLanzamiento::Dialogo);
            }
            AccionPaletaLanzar::EjecutarEnHost {
                snippet_id,
                host_id,
            } => self.iniciar_snippet(snippet_id, OrigenLanzamiento::Host(host_id)),
        }
    }

    /// `!` en Hosts y Flota: paleta con los snippets que apuntan al host.
    pub(super) fn paleta_snippets_de_host(&mut self, host_id: i64) {
        let Some(host) = self.hosts.iter().find(|host| host.id == host_id).cloned() else {
            return;
        };
        let snippets = match self.almacen.listar_snippets() {
            Ok(snippets) => snippets,
            Err(error) => {
                self.mensaje(format!("no se pudieron leer los snippets: {error}"), true);
                return;
            }
        };
        let entradas: Vec<EntradaPaleta> =
            entradas_de_host(&snippets, &self.hosts, &self.grupos, &host)
                .into_iter()
                .map(|(etiqueta, accion)| EntradaPaleta {
                    etiqueta,
                    categoria: "snippets",
                    accion: AccionPaleta::Lanzar(accion),
                })
                .collect();
        if entradas.is_empty() {
            self.mensaje(
                format!("ningún snippet apunta a «{}»", limpio(&host.nombre)),
                true,
            );
            return;
        }
        self.abrir_paleta_con(entradas, "");
    }

    /// Valida `[flota.atajos]` contra los snippets; devuelve los avisos.
    pub(super) fn validar_atajos_flota(&mut self) -> Vec<String> {
        let (validos, avisos) =
            validar_atajos_contra(&self.config.flota.atajos, &self.snippets.lista);
        self.lanzar.atajos = validos;
        avisos
    }

    /// Atajo de Flota sobre el host seleccionado; `false` si la tecla no es
    /// un atajo. El snippet se busca por nombre en ese momento: si ya no
    /// existe, se avisa y la tecla no hace nada.
    pub(super) fn ejecutar_atajo_flota(&mut self, tecla: char) -> bool {
        let Some(nombre) = atajo_de(&self.lanzar.atajos, tecla).map(str::to_string) else {
            return false;
        };
        let Some(host_id) = self.host_flota_seleccionado().map(|host| host.id) else {
            return true;
        };
        match self.almacen.snippet_por_nombre(&nombre) {
            Ok(Some(snippet)) => self.iniciar_snippet(snippet.id, OrigenLanzamiento::Host(host_id)),
            Ok(None) => self.mensaje(
                format!("atajo «{tecla}»: el snippet «{nombre}» ya no existe"),
                true,
            ),
            Err(error) => self.mensaje(
                format!("atajo «{tecla}»: no se pudo leer «{nombre}»: {error}"),
                true,
            ),
        }
        true
    }

    /// Atajos válidos, para la barra de Flota.
    pub fn atajos_flota(&self) -> &[(char, String)] {
        &self.lanzar.atajos
    }

    /// Comandos iniciales de una pestaña nueva al host: su snippet al
    /// conectar, si lo tiene y sigue siendo apto. Nunca delibera.
    pub(super) fn comandos_al_conectar(&mut self, host_id: i64) -> Vec<ComandoInicial> {
        let Some(host) = self.hosts.iter().find(|host| host.id == host_id) else {
            return Vec::new();
        };
        let Some(snippet_id) = host.snippet_al_conectar_id else {
            return Vec::new();
        };
        let host_nombre = host.nombre.clone();
        let aviso = match self.almacen.obtener_snippet(snippet_id) {
            Ok(snippet) => match comando_al_conectar(&snippet) {
                Some(comando) => return vec![comando],
                None => format!(
                    "el snippet al conectar «{}» de «{host_nombre}» es crítico o tiene variables: no se escribe",
                    snippet.nombre
                ),
            },
            Err(error) => format!("snippet al conectar de «{host_nombre}»: {}", error_al_leer(&error)),
        };
        tracing::warn!("{aviso}");
        self.lanzar.diferir_aviso(limpio(&aviso));
        Vec::new()
    }

    /// Tras cada difusión de sesiones: seguimiento de las pestañas abiertas
    /// con `p` (anotación «en pestaña» y cierre de su deliberación).
    pub(super) fn revisar_pestanas_snippet(&mut self) {
        if self.lanzar.seguimientos.is_empty() {
            return;
        }
        let pestanas: Vec<PestanaVista> = self
            .pestanas
            .iter()
            .map(|pestana| PestanaVista {
                sesion_id: pestana.sesion_id,
                host_id: pestana.host_id,
                estado: pestana.estado,
            })
            .collect();
        let revision = self
            .lanzar
            .revisar(&pestanas, &self.aperturas_pendientes, Instant::now());
        for (seguimiento, abierta) in revision.resueltos {
            if !abierta {
                // Una apertura fallida ya la anota el servidor
                // (`conexion_fallida`); el snippet no llegó a escribirse.
                continue;
            }
            let mut detalle = format!(
                "«{}» · {} · en pestaña",
                seguimiento.snippet, seguimiento.host_nombre
            );
            if let Some(id) = seguimiento.deliberacion_id {
                detalle.push_str(&format!(" · deliberación #{id}"));
            }
            self.anotar(
                crate::registro::SNIPPET_EJECUTADO,
                Some(seguimiento.host_id),
                None,
                &detalle,
                crate::modelo::ResultadoRegistro::Ok,
            );
        }
        for cierre in revision.cierres {
            let resultado = crate::deliberacion::EjecucionResultado::de_hosts(
                cierre.abiertas,
                cierre.total,
                false,
            );
            self.cerrar_deliberacion(&cierre.deliberacion, resultado);
        }
    }

    /// Caducidades del seguimiento de pestañas y avisos diferidos; devuelve
    /// si hay que repintar.
    pub(super) fn tick_lanzar(&mut self) -> bool {
        self.revisar_pestanas_snippet();
        let Some(aviso) = self.lanzar.aviso_diferido.take() else {
            return false;
        };
        // Un error ya en pantalla (servidor caído, apertura en curso) no se
        // pierde: el aviso va detrás.
        let texto = match &self.mensaje {
            Some(actual) if actual.error => format!("{} · {aviso}", actual.texto),
            _ => aviso,
        };
        self.mensaje(texto, true);
        true
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::snippets::Destino;

    fn snippet(comando: &str) -> Snippet {
        Snippet {
            id: 7,
            nombre: "limpiar journald".to_string(),
            comando: comando.to_string(),
            descripcion: String::new(),
            etiquetas: Vec::new(),
            critico: false,
            timeout_seg: 60,
            parar_al_fallo: true,
            usado_veces: 0,
            ultimo_uso_en: None,
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos: vec![Destino::Etiqueta("web".to_string())],
        }
    }

    fn host(id: i64, nombre: &str, etiquetas: &[&str]) -> Host {
        let mut host = crate::modelo::host_de_prueba();
        host.id = id;
        host.nombre = nombre.to_string();
        host.grupo_id = None;
        host.etiquetas = etiquetas.iter().map(|e| e.to_string()).collect();
        host
    }

    fn hosts(cuantos: i64) -> Vec<(i64, String)> {
        (1..=cuantos).map(|id| (id, format!("h{id}"))).collect()
    }

    fn tecla(codigo: KeyCode) -> KeyEvent {
        KeyEvent::new(codigo, KeyModifiers::NONE)
    }

    fn escribir(dialogo: &mut DialogoEjecutar, texto: &str) {
        for caracter in texto.chars() {
            dialogo.manejar_tecla(&tecla(KeyCode::Char(caracter)), COLUMNAS_HOSTS);
        }
    }

    fn borrar(dialogo: &mut DialogoEjecutar, veces: usize) {
        for _ in 0..veces {
            dialogo.manejar_tecla(&tecla(KeyCode::Backspace), COLUMNAS_HOSTS);
        }
    }

    fn dialogo_de(preparacion: Preparacion) -> DialogoEjecutar {
        match preparacion {
            Preparacion::Dialogo(dialogo) => dialogo,
            Preparacion::Plan(plan) => panic!("se esperaba el diálogo y llegó {plan:?}"),
        }
    }

    fn plan_de(preparacion: Preparacion) -> PlanEjecucion {
        match preparacion {
            Preparacion::Plan(plan) => plan,
            Preparacion::Dialogo(_) => panic!("se esperaba el plan y llegó el diálogo"),
        }
    }

    fn continuar(dialogo: &mut DialogoEjecutar) -> Option<PlanEjecucion> {
        match dialogo.manejar_tecla(&tecla(KeyCode::Enter), COLUMNAS_HOSTS) {
            AccionEjecutar::Continuar(plan) => Some(plan),
            AccionEjecutar::Nada => None,
            AccionEjecutar::Cancelar => panic!("↵ no cancela"),
        }
    }

    #[test]
    fn el_dialogo_abre_con_todo_marcado_y_los_defectos() {
        let snippet = snippet("journalctl --vacuum-time={{dias:7}}d");
        let dialogo =
            dialogo_de(preparar(&snippet, hosts(4), &OrigenLanzamiento::Dialogo).unwrap());
        assert_eq!(dialogo.snippet_id, 7);
        assert_eq!(dialogo.nombre, "limpiar journald");
        assert!(dialogo.hosts_editables());
        assert_eq!(dialogo.marcados(), 4);
        assert_eq!(dialogo.valor(0).unwrap().texto, "7");
        assert!(dialogo.parar_al_fallo());
        assert_eq!(dialogo.timeout_seg(), 60);
        assert_eq!(dialogo.foco(), FocoEjecutar::Hosts);
        assert_eq!(dialogo.modo(), ModoLanzamiento::Servidor);
    }

    #[test]
    fn continuar_arma_el_plan_con_los_hosts_marcados() {
        let snippet = snippet("uptime");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(5), &OrigenLanzamiento::Dialogo).unwrap());
        // Desmarca h2 (→ espacio) y h5 (↓ → espacio).
        dialogo.manejar_tecla(&tecla(KeyCode::Right), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Char(' ')), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Down), COLUMNAS_HOSTS);
        assert_eq!(dialogo.cursor_host(), 4);
        dialogo.manejar_tecla(&tecla(KeyCode::Char(' ')), COLUMNAS_HOSTS);
        // Y cambia «parar al primer fallo».
        dialogo.manejar_tecla(&tecla(KeyCode::Tab), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Parar);
        dialogo.manejar_tecla(&tecla(KeyCode::Char(' ')), COLUMNAS_HOSTS);
        let plan = continuar(&mut dialogo).unwrap();
        assert_eq!(
            plan.hosts,
            vec![
                (1, "h1".to_string()),
                (3, "h3".to_string()),
                (4, "h4".to_string())
            ]
        );
        assert_eq!(plan.comando, "uptime");
        assert!(!plan.parar_al_fallo);
        assert_eq!(plan.timeout_seg, 60);
        assert_eq!(plan.modo, ModoLanzamiento::Servidor);
        assert!(plan.valores.is_empty());
    }

    #[test]
    fn sin_hosts_marcados_no_continua() {
        let snippet = snippet("uptime");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(2), &OrigenLanzamiento::Dialogo).unwrap());
        dialogo.manejar_tecla(&tecla(KeyCode::Char(' ')), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Right), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Char(' ')), COLUMNAS_HOSTS);
        assert_eq!(dialogo.marcados(), 0);
        assert!(continuar(&mut dialogo).is_none());
        assert_eq!(dialogo.error(), Some("marca al menos un host"));
        assert_eq!(dialogo.foco(), FocoEjecutar::Hosts);
        // La siguiente tecla retira el error.
        dialogo.manejar_tecla(&tecla(KeyCode::Char(' ')), COLUMNAS_HOSTS);
        assert_eq!(dialogo.error(), None);
        assert!(continuar(&mut dialogo).is_some());
    }

    #[test]
    fn una_variable_vacia_sin_defecto_no_deja_continuar() {
        let snippet = snippet("systemctl restart {{servicio}} {{modo:suave}}");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(1), &OrigenLanzamiento::Dialogo).unwrap());
        // Lleva el foco a «parar»: ↵ vale desde cualquier campo.
        dialogo.manejar_tecla(&tecla(KeyCode::BackTab), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Parar);
        assert!(continuar(&mut dialogo).is_none());
        assert_eq!(dialogo.error(), Some("falta el valor de «servicio»"));
        assert_eq!(dialogo.foco(), FocoEjecutar::Variable(0));
        escribir(&mut dialogo, "nginx");
        let plan = continuar(&mut dialogo).unwrap();
        assert_eq!(plan.comando, "systemctl restart nginx suave");
        assert_eq!(plan.valores.get("servicio").unwrap(), "nginx");
        assert_eq!(plan.valores.get("modo").unwrap(), "suave");
    }

    #[test]
    fn una_variable_con_defecto_vaciada_toma_el_defecto() {
        let snippet = snippet("journalctl --vacuum-time={{dias:7}}d");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(1), &OrigenLanzamiento::Todos).unwrap());
        assert!(!dialogo.hosts_editables());
        assert_eq!(dialogo.foco(), FocoEjecutar::Variable(0));
        borrar(&mut dialogo, 3);
        assert_eq!(dialogo.valor(0).unwrap().texto, "");
        let plan = continuar(&mut dialogo).unwrap();
        assert_eq!(plan.comando, "journalctl --vacuum-time=7d");
        assert_eq!(plan.valores.get("dias").unwrap(), "7");
    }

    #[test]
    fn los_valores_se_escapan_para_el_shell() {
        let snippet = snippet("systemctl restart {{servicio}}");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(1), &OrigenLanzamiento::Todos).unwrap());
        escribir(&mut dialogo, "a b; rm -rf /");
        let plan = continuar(&mut dialogo).unwrap();
        assert_eq!(plan.comando, "systemctl restart 'a b; rm -rf /'");
        // El valor guardado para repetir es el escrito, sin escapar.
        assert_eq!(plan.valores.get("servicio").unwrap(), "a b; rm -rf /");
    }

    #[test]
    fn una_variable_entre_comillas_se_rechaza() {
        // Un comando así no se puede guardar; si llega (base tocada a mano),
        // no se lanza.
        let snippet = snippet("grep \"{{patron}}\" /var/log/syslog");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(1), &OrigenLanzamiento::Dialogo).unwrap());
        dialogo.manejar_tecla(&tecla(KeyCode::Tab), COLUMNAS_HOSTS);
        escribir(&mut dialogo, "x");
        assert!(continuar(&mut dialogo).is_none());
        assert!(
            dialogo.error().unwrap().contains("comillas"),
            "{:?}",
            dialogo.error()
        );
        // Sin diálogo, es un aviso.
        let aviso = preparar(&snippet, hosts(1), &OrigenLanzamiento::Todos)
            .err()
            .unwrap();
        assert!(matches!(aviso, AvisoPreparacion::Comando(motivo) if motivo.contains("comillas")));
        let repetir = OrigenLanzamiento::Repetir {
            host_ids: vec![1],
            valores: Some(BTreeMap::from([("patron".to_string(), "x".to_string())])),
        };
        assert!(matches!(
            preparar(&snippet, hosts(1), &repetir),
            Err(AvisoPreparacion::Comando(_))
        ));
    }

    #[test]
    fn sin_variables_a_y_p_arman_el_plan_directamente() {
        let snippet = snippet("uptime");
        let plan = plan_de(preparar(&snippet, hosts(3), &OrigenLanzamiento::Todos).unwrap());
        assert_eq!(plan.hosts.len(), 3);
        assert_eq!(plan.modo, ModoLanzamiento::Servidor);
        assert!(plan.parar_al_fallo);
        let plan = plan_de(preparar(&snippet, hosts(3), &OrigenLanzamiento::Pestanas).unwrap());
        assert_eq!(plan.modo, ModoLanzamiento::Pestanas);
        assert_eq!(plan.accion(), "limpiar journald → h1, h2, h3 (en pestaña)");
    }

    #[test]
    fn con_variables_a_pregunta_solo_las_variables() {
        let snippet = snippet("journalctl --vacuum-time={{dias:7}}d");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(3), &OrigenLanzamiento::Pestanas).unwrap());
        assert!(!dialogo.hosts_editables());
        assert_eq!(dialogo.modo(), ModoLanzamiento::Pestanas);
        // En pestaña no hay «parar al primer fallo»: Tab no sale de la variable.
        dialogo.manejar_tecla(&tecla(KeyCode::Tab), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Variable(0));
        // Las flechas no tocan los hosts fijos.
        dialogo.manejar_tecla(&tecla(KeyCode::Up), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Variable(0));
        let plan = continuar(&mut dialogo).unwrap();
        assert_eq!(plan.hosts.len(), 3);
        assert_eq!(plan.modo, ModoLanzamiento::Pestanas);
    }

    #[test]
    fn repetir_con_valores_no_pregunta() {
        let snippet = snippet("systemctl restart {{servicio}}");
        let valores = BTreeMap::from([("servicio".to_string(), "nginx".to_string())]);
        let repetir = OrigenLanzamiento::Repetir {
            host_ids: vec![3, 1, 99],
            valores: Some(valores.clone()),
        };
        let plan = plan_de(preparar(&snippet, hosts(4), &repetir).unwrap());
        // Los hosts que siguen siendo destino, en el orden resuelto.
        assert_eq!(
            plan.hosts,
            vec![(1, "h1".to_string()), (3, "h3".to_string())]
        );
        assert_eq!(plan.comando, "systemctl restart nginx");
        assert_eq!(plan.valores, valores);
        assert_eq!(plan.modo, ModoLanzamiento::Servidor);
    }

    #[test]
    fn repetir_sin_valores_precarga_los_conocidos_y_pregunta() {
        let snippet = snippet("rsync {{origen}} {{destino:/srv}}");
        let sin_valores = OrigenLanzamiento::Repetir {
            host_ids: vec![1],
            valores: None,
        };
        let dialogo = dialogo_de(preparar(&snippet, hosts(2), &sin_valores).unwrap());
        assert!(!dialogo.hosts_editables());
        assert_eq!(dialogo.hosts(), &[(1, "h1".to_string())]);
        // Un valor conocido pero con una variable nueva en el snippet.
        let parcial = OrigenLanzamiento::Repetir {
            host_ids: vec![1],
            valores: Some(BTreeMap::from([(
                "destino".to_string(),
                "/tmp".to_string(),
            )])),
        };
        let dialogo = dialogo_de(preparar(&snippet, hosts(2), &parcial).unwrap());
        assert_eq!(dialogo.valor(0).unwrap().texto, "");
        assert_eq!(dialogo.valor(1).unwrap().texto, "/tmp");
    }

    #[test]
    fn repetir_sin_hosts_que_sigan_siendo_destino_avisa() {
        let repetir = OrigenLanzamiento::Repetir {
            host_ids: vec![42],
            valores: None,
        };
        assert_eq!(
            preparar(&snippet("uptime"), hosts(2), &repetir).err(),
            Some(AvisoPreparacion::NingunoDeRepetir)
        );
    }

    #[test]
    fn sin_hosts_resueltos_avisa() {
        for origen in [
            OrigenLanzamiento::Dialogo,
            OrigenLanzamiento::Todos,
            OrigenLanzamiento::Pestanas,
            OrigenLanzamiento::Host(1),
        ] {
            assert_eq!(
                preparar(&snippet("uptime"), Vec::new(), &origen).err(),
                Some(AvisoPreparacion::SinHosts)
            );
        }
    }

    /// Decisión 3: un host que no es destino del snippet no ejecuta.
    #[test]
    fn un_host_que_no_es_destino_avisa_y_no_ejecuta() {
        assert_eq!(
            preparar(&snippet("uptime"), hosts(2), &OrigenLanzamiento::Host(9)).err(),
            Some(AvisoPreparacion::NoApunta(9))
        );
        let plan =
            plan_de(preparar(&snippet("uptime"), hosts(2), &OrigenLanzamiento::Host(2)).unwrap());
        assert_eq!(plan.hosts, vec![(2, "h2".to_string())]);
    }

    /// Con menos ancho la rejilla tiene menos columnas (Fase 7) y las
    /// flechas recorren la que se ve.
    #[test]
    fn la_rejilla_usa_las_columnas_que_se_ven() {
        let snippet = snippet("uptime");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(5), &OrigenLanzamiento::Dialogo).unwrap());
        // Una columna: ↓ baja de uno en uno.
        dialogo.manejar_tecla(&tecla(KeyCode::Down), 1);
        assert_eq!(dialogo.cursor_host(), 1);
        // Dos columnas: h1 h2 / h3 h4 / h5.
        dialogo.manejar_tecla(&tecla(KeyCode::Down), 2);
        assert_eq!(dialogo.cursor_host(), 3);
        dialogo.manejar_tecla(&tecla(KeyCode::Up), 2);
        assert_eq!(dialogo.cursor_host(), 1);
        // Fuera de rango se acota a entre una y tres columnas.
        dialogo.manejar_tecla(&tecla(KeyCode::Down), 0);
        assert_eq!(dialogo.cursor_host(), 2);
        dialogo.manejar_tecla(&tecla(KeyCode::Down), 9);
        assert_eq!(dialogo.cursor_host(), 4);
    }

    #[test]
    fn la_rejilla_de_hosts_se_recorre_con_flechas() {
        let snippet = snippet("uptime {{x:1}}");
        let mut dialogo =
            dialogo_de(preparar(&snippet, hosts(7), &OrigenLanzamiento::Dialogo).unwrap());
        // 3 columnas: h1 h2 h3 / h4 h5 h6 / h7.
        dialogo.manejar_tecla(&tecla(KeyCode::Down), COLUMNAS_HOSTS);
        assert_eq!(dialogo.cursor_host(), 3);
        dialogo.manejar_tecla(&tecla(KeyCode::Right), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Right), COLUMNAS_HOSTS);
        assert_eq!(dialogo.cursor_host(), 5);
        // A la última fila incompleta: al último host.
        dialogo.manejar_tecla(&tecla(KeyCode::Down), COLUMNAS_HOSTS);
        assert_eq!(dialogo.cursor_host(), 6);
        dialogo.manejar_tecla(&tecla(KeyCode::Right), COLUMNAS_HOSTS);
        assert_eq!(dialogo.cursor_host(), 6);
        dialogo.manejar_tecla(&tecla(KeyCode::Up), COLUMNAS_HOSTS);
        assert_eq!(dialogo.cursor_host(), 3);
        // Desde la última fila, ↓ pasa a las variables; ↑ vuelve.
        dialogo.manejar_tecla(&tecla(KeyCode::Down), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Down), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Variable(0));
        dialogo.manejar_tecla(&tecla(KeyCode::Up), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Hosts);
        // Tab recorre las zonas y vuelve a empezar; Shift+Tab al revés.
        dialogo.manejar_tecla(&tecla(KeyCode::Tab), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Tab), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Parar);
        dialogo.manejar_tecla(&tecla(KeyCode::Tab), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Hosts);
        dialogo.manejar_tecla(&tecla(KeyCode::BackTab), COLUMNAS_HOSTS);
        assert_eq!(dialogo.foco(), FocoEjecutar::Parar);
        // El espacio en una variable es texto, no marca.
        dialogo.manejar_tecla(&tecla(KeyCode::BackTab), COLUMNAS_HOSTS);
        dialogo.manejar_tecla(&tecla(KeyCode::Char(' ')), COLUMNAS_HOSTS);
        assert_eq!(dialogo.valor(0).unwrap().texto, "1 ");
        assert_eq!(dialogo.marcados(), 7);
        assert_eq!(
            dialogo.manejar_tecla(&tecla(KeyCode::Esc), COLUMNAS_HOSTS),
            AccionEjecutar::Cancelar
        );
    }

    /// Decisión 4: la confirmación de `p` con más de cinco hosts.
    #[test]
    fn p_con_mas_de_cinco_hosts_pide_confirmacion() {
        let snippet = snippet("uptime");
        let cinco = plan_de(preparar(&snippet, hosts(5), &OrigenLanzamiento::Pestanas).unwrap());
        assert!(!requiere_confirmacion(&cinco));
        let seis = plan_de(preparar(&snippet, hosts(6), &OrigenLanzamiento::Pestanas).unwrap());
        assert!(requiere_confirmacion(&seis));
        // En el servidor no se confirma por número (delibera).
        let servidor = plan_de(preparar(&snippet, hosts(6), &OrigenLanzamiento::Todos).unwrap());
        assert!(!requiere_confirmacion(&servidor));
        let lineas = lineas_confirmacion_pestanas(&seis);
        assert!(lineas[0].starts_with("Se abrirán 6 pestañas"), "{lineas:?}");
        assert!(lineas.join(" ").contains("h1, h2"));
        assert!(lineas
            .iter()
            .all(|linea| linea.chars().count() <= ANCHO_CONFIRMACION));
        // Muchos hosts: la lista se recorta.
        let mut muchos = seis.clone();
        muchos.hosts = (1..=200).map(|id| (id, format!("host-{id}"))).collect();
        let lineas = lineas_confirmacion_pestanas(&muchos);
        assert!(lineas.len() <= 2 + LINEAS_HOSTS_CONFIRMACION, "{lineas:?}");
        assert!(lineas.last().unwrap().ends_with('…'));
    }

    #[test]
    fn el_comando_inicial_de_la_pestana() {
        let mut al_conectar = snippet("tmux attach\n");
        al_conectar.critico = false;
        let inicial = comando_al_conectar(&al_conectar).unwrap();
        assert_eq!(inicial.texto, "tmux attach\n");
        assert!(inicial.repetir);
        // Crítico o con variables: ya no es apto.
        let mut critico = al_conectar.clone();
        critico.critico = true;
        assert!(comando_al_conectar(&critico).is_none());
        assert!(comando_al_conectar(&snippet("cd {{dir}}")).is_none());
        // `p`: primero el de al conectar (repetir) y luego el del plan (una vez).
        let comandos = comandos_de_pestana(vec![inicial.clone()], "systemctl status nginx\n\n");
        assert_eq!(
            comandos,
            vec![
                inicial,
                ComandoInicial {
                    texto: "systemctl status nginx\n".to_string(),
                    repetir: false,
                }
            ]
        );
        let solo = comandos_de_pestana(Vec::new(), "ls");
        assert_eq!(solo.len(), 1);
        assert!(!solo[0].repetir);
        assert_eq!(solo[0].texto, "ls\n");
    }

    #[test]
    fn los_atajos_se_validan_contra_los_snippets() {
        let atajos: BTreeMap<String, String> = [
            ("u", "limpiar journald"),
            ("r", "limpiar journald"),
            ("z", "borrado"),
        ]
        .into_iter()
        .map(|(tecla, nombre)| (tecla.to_string(), nombre.to_string()))
        .collect();
        let (validos, avisos) = validar_atajos_contra(&atajos, &[snippet("uptime")]);
        assert_eq!(validos, vec![('u', "limpiar journald".to_string())]);
        assert_eq!(avisos.len(), 2, "{avisos:?}");
        assert_eq!(atajo_de(&validos, 'u'), Some("limpiar journald"));
        assert_eq!(atajo_de(&validos, 'z'), None);
        let (validos, avisos) = validar_atajos_contra(&atajos, &[]);
        assert!(validos.is_empty());
        assert_eq!(avisos.len(), 3);
    }

    #[test]
    fn la_paleta_de_un_host_solo_trae_sus_snippets() {
        let inventario = vec![
            host(1, "web-01", &["web"]),
            host(2, "db-01", &["db"]),
            host(3, "web-02", &["web"]),
        ];
        let mut web = snippet("uptime");
        web.id = 1;
        web.nombre = "ver carga".to_string();
        let mut db = snippet("pg_dump");
        db.id = 2;
        db.nombre = "backup".to_string();
        db.destinos = vec![
            Destino::Etiqueta("db".to_string()),
            Destino::Host {
                id: 3,
                nombre: "web-02".to_string(),
            },
        ];
        let snippets = vec![web, db];
        let de_web01 = entradas_de_host(&snippets, &inventario, &[], &inventario[0]);
        assert_eq!(
            de_web01,
            vec![(
                "snippet · ver carga · web-01".to_string(),
                AccionPaletaLanzar::EjecutarEnHost {
                    snippet_id: 1,
                    host_id: 1
                }
            )]
        );
        let de_web02 = entradas_de_host(&snippets, &inventario, &[], &inventario[2]);
        assert_eq!(de_web02.len(), 2);
        let de_db = entradas_de_host(&snippets, &inventario, &[], &inventario[1]);
        assert_eq!(de_db[0].0, "snippet · backup · db-01");
        // Un host al que no apunta nadie: vacía (y `!` avisa).
        let suelto = host(4, "suelto", &[]);
        assert!(entradas_de_host(&snippets, &inventario, &[], &suelto).is_empty());
        // La paleta general: cada snippet y cada snippet en cada host.
        let todas = entradas_de_snippets(&snippets, &inventario, &[]);
        let etiquetas: Vec<&str> = todas
            .iter()
            .map(|(etiqueta, _)| etiqueta.as_str())
            .collect();
        assert_eq!(
            etiquetas,
            vec![
                "snippet · ver carga",
                "snippet · ver carga · web-01",
                "snippet · ver carga · web-02",
                "snippet · backup",
                "snippet · backup · db-01",
                "snippet · backup · web-02",
            ]
        );
        assert_eq!(todas[0].1, AccionPaletaLanzar::Ejecutar(1));
    }

    #[test]
    fn las_etiquetas_de_la_paleta_van_saneadas() {
        assert_eq!(
            etiqueta_snippet_en_host("mal\u{1b}[2Jo", "h\u{7}ost"),
            "snippet · malo · host"
        );
    }

    // ------------------------------------------------------------ seguimiento

    fn seguimiento(host_id: i64, existentes: &[u32], grupo: Option<u64>) -> SeguimientoPestana {
        SeguimientoPestana {
            grupo,
            deliberacion_id: grupo.map(|g| g as i64),
            host_id,
            host_nombre: format!("h{host_id}"),
            snippet: "ver carga".to_string(),
            existentes: existentes.iter().copied().collect(),
            sesion_id: None,
            desde: Instant::now(),
        }
    }

    fn pestana(sesion_id: u32, host_id: i64, estado: EstadoSesionRemota) -> PestanaVista {
        PestanaVista {
            sesion_id,
            host_id,
            estado,
        }
    }

    fn deliberacion(id: i64) -> DeliberacionLanzada {
        DeliberacionLanzada {
            id,
            forzada: false,
            motivo: None,
        }
    }

    #[test]
    fn el_seguimiento_espera_a_que_la_pestana_nueva_abra() {
        let mut estado = EstadoLanzar::default();
        estado.seguimientos.push(seguimiento(1, &[5], None));
        let ahora = Instant::now();
        let pendientes = HashSet::from([1]);
        // Solo está la que ya existía y la apertura sigue en vuelo.
        let vieja = pestana(5, 1, EstadoSesionRemota::Abierta);
        let revision = estado.revisar(&[vieja], &pendientes, ahora);
        assert!(revision.resueltos.is_empty());
        // Aparece la nueva, abriendo (y la apertura deja de estar en vuelo).
        let abriendo = pestana(7, 1, EstadoSesionRemota::Abriendo);
        let revision = estado.revisar(&[vieja, abriendo], &HashSet::new(), ahora);
        assert!(revision.resueltos.is_empty());
        assert_eq!(estado.seguimientos[0].sesion_id, Some(7));
        // Abre: resuelto como abierta.
        let abierta = pestana(7, 1, EstadoSesionRemota::Abierta);
        let revision = estado.revisar(&[vieja, abierta], &HashSet::new(), ahora);
        assert_eq!(revision.resueltos.len(), 1);
        assert!(revision.resueltos[0].1);
        assert!(estado.seguimientos.is_empty());
    }

    #[test]
    fn el_seguimiento_falla_si_la_pestana_no_llega_a_abrir() {
        let ahora = Instant::now();
        // La apertura terminó sin pestaña nueva.
        let mut estado = EstadoLanzar::default();
        estado.seguimientos.push(seguimiento(1, &[], None));
        let revision = estado.revisar(&[], &HashSet::new(), ahora);
        assert!(!revision.resueltos[0].1);
        // La pestaña apareció y desapareció (huella rechazada).
        let mut estado = EstadoLanzar::default();
        estado.seguimientos.push(seguimiento(1, &[], None));
        let abriendo = pestana(3, 1, EstadoSesionRemota::Abriendo);
        assert!(estado
            .revisar(&[abriendo], &HashSet::new(), ahora)
            .resueltos
            .is_empty());
        let revision = estado.revisar(&[], &HashSet::new(), ahora);
        assert!(!revision.resueltos[0].1);
        // Vence el plazo sin abrir.
        let mut estado = EstadoLanzar::default();
        estado.seguimientos.push(seguimiento(1, &[], None));
        let despues = ahora + PLAZO_SEGUIMIENTO + Duration::from_secs(1);
        let revision = estado.revisar(&[abriendo], &HashSet::from([1]), despues);
        assert!(!revision.resueltos[0].1);
    }

    #[test]
    fn dos_seguimientos_del_mismo_host_no_comparten_pestana() {
        let ahora = Instant::now();
        let mut estado = EstadoLanzar::default();
        estado.seguimientos.push(seguimiento(1, &[], None));
        estado.seguimientos.push(seguimiento(1, &[], None));
        let a = pestana(10, 1, EstadoSesionRemota::Abriendo);
        let b = pestana(11, 1, EstadoSesionRemota::Abriendo);
        estado.revisar(&[a, b], &HashSet::new(), ahora);
        let atadas: HashSet<Option<u32>> = estado
            .seguimientos
            .iter()
            .map(|seguimiento| seguimiento.sesion_id)
            .collect();
        assert_eq!(atadas, HashSet::from([Some(10), Some(11)]));
    }

    #[test]
    fn el_grupo_deliberado_se_cierra_cuando_todas_se_resuelven() {
        let ahora = Instant::now();
        let mut estado = EstadoLanzar::default();
        let grupo = estado.nuevo_grupo();
        estado.grupos.insert(
            grupo,
            GrupoPestanas {
                deliberacion: deliberacion(4),
                total: 3,
                pendientes: 2,
                abiertas: 0,
            },
        );
        estado.seguimientos.push(seguimiento(1, &[], Some(grupo)));
        estado.seguimientos.push(seguimiento(2, &[], Some(grupo)));
        // h1 abre; h2 sigue en vuelo: el grupo aún no se cierra.
        let h1 = pestana(20, 1, EstadoSesionRemota::Abierta);
        let revision = estado.revisar(&[h1], &HashSet::from([2]), ahora);
        assert_eq!(revision.resueltos.len(), 1);
        assert!(revision.cierres.is_empty());
        // h2 falla: cierre con 1 abierta de 3 (una ni se pudo pedir).
        let revision = estado.revisar(&[h1], &HashSet::new(), ahora);
        assert_eq!(revision.cierres.len(), 1);
        let cierre = &revision.cierres[0];
        assert_eq!(cierre.deliberacion.id, 4);
        assert_eq!((cierre.abiertas, cierre.total), (1, 3));
        assert_eq!(
            crate::deliberacion::EjecucionResultado::de_hosts(cierre.abiertas, cierre.total, false),
            crate::deliberacion::EjecucionResultado::Parcial
        );
        assert!(estado.grupos.is_empty());
    }

    #[test]
    fn los_avisos_diferidos_se_juntan_sin_repetirse() {
        let mut estado = EstadoLanzar::default();
        estado.diferir_aviso("uno".to_string());
        estado.diferir_aviso("uno".to_string());
        estado.diferir_aviso("dos".to_string());
        assert_eq!(estado.aviso_diferido.as_deref(), Some("uno · dos"));
    }
}
