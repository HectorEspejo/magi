//! Vista Resultados (subvista de `F8`): las ejecuciones que difunde el
//! servidor (panel superior), los hosts de la seleccionada (panel de abajo) y
//! el visor de salida de un host (stdout y stderr separados y desplazables,
//! sin secuencias de escape). `s` guarda la salida a fichero, `x` cancela,
//! `r` repite y `C` limpia las terminadas.
//!
//! La lógica vive en `EstadoResultados` (sin red ni disco, con el reloj como
//! argumento) y `App` solo envía los mensajes, escribe el fichero y avisa.
//! Una respuesta que no se pidió (una `Salida` que no espera ni el visor ni
//! el guardado, un `Hecho`/`Error` de un lanzamiento que ya no está en vuelo)
//! se descarta.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use tracing::warn;

use crate::deliberacion::EjecucionResultado;
use crate::protocolo::{
    EstadoEjecucion, EstadoHostEjecucion, InfoEjecucion, InfoEjecucionHost, MensajeCliente,
};
use crate::snippets::salida::{fichero_de_salida, lineas_limpias, ruta_por_defecto, ParteSalida};
use crate::ui::componentes::CampoTexto;
use crate::ui::disposicion::{Lista, VentanaLista};
use crate::ui::resultados::{pantalla, sin_marcas};
use crate::ui::snippets::{limpio, partir, texto_hosts};
use crate::ui::Vista;

use super::lanzar::{OrigenLanzamiento, PlanEjecucion};
use super::{AccionDialogo, App, Dialogo, EntradaTextoAccion};

/// Primer `peticion_id` de `LanzarEjecucion` (T39: Archivos desde 0, túneles
/// desde 2^40, ejecuciones desde 2^44 y peticiones esperables desde 2^48).
pub const RANGO_EJECUCIONES: u64 = 1 << 44;

/// Plazo para reunir las salidas de un guardado (`s`).
pub const PLAZO_GUARDADO: Duration = Duration::from_secs(10);

/// Cada cuánto se vuelve a pedir la salida mientras el host está en marcha.
pub const REFRESCO_VISOR: Duration = Duration::from_secs(1);

/// Una `PedirSalida` del visor sin respuesta en este plazo se da por perdida:
/// el servidor no contesta si la ejecución ya no existe.
pub const PLAZO_SALIDA: Duration = Duration::from_secs(5);

/// Columnas que mueve `←`/`→` en el visor.
pub const PASO_HORIZONTAL: usize = 8;

/// Ancho útil de las líneas de un diálogo de confirmación (el modal no parte
/// las líneas largas).
const ANCHO_CONFIRMACION: usize = 58;

/// ¿Es un `peticion_id` de ejecuciones? (T39)
pub fn es_de_ejecuciones(peticion_id: u64) -> bool {
    (RANGO_EJECUCIONES..crate::cliente::RANGO_ESPERAS).contains(&peticion_id)
}

/// Panel activo de la vista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelResultados {
    #[default]
    Ejecuciones,
    Hosts,
}

/// Flujo de salida del visor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flujo {
    #[default]
    Stdout,
    Stderr,
}

impl Flujo {
    pub fn indice(self) -> usize {
        match self {
            Flujo::Stdout => 0,
            Flujo::Stderr => 1,
        }
    }

    pub fn otro(self) -> Self {
        match self {
            Flujo::Stdout => Flujo::Stderr,
            Flujo::Stderr => Flujo::Stdout,
        }
    }
}

/// Mayor desplazamiento que deja la última línea en la última fila.
pub fn maximo_desplazamiento(total: usize, alto: usize) -> usize {
    total.saturating_sub(alto.max(1))
}

/// Visor de la salida de un host: la ejecución y el host van fijados al
/// abrirlo; las líneas son las limpias (sin escapes ni controles), calculadas
/// al recibir. Sin `Debug`: lleva salida remota, que no puede acabar en un
/// log (T44).
pub struct Visor {
    pub ejecucion_id: u32,
    pub host_id: i64,
    /// Nombres fijados al abrir y ya saneados (la ejecución puede desaparecer
    /// de la difusión con el visor abierto).
    pub snippet: String,
    pub host: String,
    /// Líneas limpias de stdout (0) y stderr (1).
    pub lineas: [Vec<String>; 2],
    pub truncada: bool,
    /// Ya llegó alguna salida.
    pub recibida: bool,
    pub foco: Flujo,
    /// Primera línea visible de cada flujo.
    pub desplazamiento: [usize; 2],
    /// Primera columna visible de cada flujo.
    pub horizontal: [usize; 2],
    /// Pegado al final: al llegar más salida, se sigue viendo la última línea.
    pub siguiendo: [bool; 2],
    /// Momento en que salió la petición que se espera.
    pub en_vuelo: Option<Instant>,
    pub ultima_peticion: Option<Instant>,
    /// Llegó la salida definitiva (host terminado y todos sus bytes): no se
    /// vuelve a pedir.
    pub completa: bool,
}

impl Visor {
    pub fn nuevo(
        ejecucion_id: u32,
        host_id: i64,
        snippet: String,
        host: String,
        en_marcha: bool,
    ) -> Self {
        Self {
            ejecucion_id,
            host_id,
            snippet,
            host,
            lineas: [Vec::new(), Vec::new()],
            truncada: false,
            recibida: false,
            foco: Flujo::Stdout,
            desplazamiento: [0, 0],
            horizontal: [0, 0],
            // Un host que aún corre se mira como `tail -f`; uno terminado,
            // desde el principio.
            siguiendo: [en_marcha, en_marcha],
            en_vuelo: None,
            ultima_peticion: None,
            completa: false,
        }
    }

    /// Sale una `PedirSalida` de este visor.
    pub fn pedir(&mut self, ahora: Instant) {
        self.en_vuelo = Some(ahora);
        self.ultima_peticion = Some(ahora);
    }

    /// ¿Espera la salida de este host? Solo con una petición en vuelo.
    pub fn espera(&self, ejecucion_id: u32, host_id: i64) -> bool {
        self.en_vuelo.is_some() && self.ejecucion_id == ejecucion_id && self.host_id == host_id
    }

    /// Una petición sin respuesta en `PLAZO_SALIDA` se da por perdida.
    pub fn caducar(&mut self, ahora: Instant) -> bool {
        match self.en_vuelo {
            Some(desde) if ahora.saturating_duration_since(desde) >= PLAZO_SALIDA => {
                self.en_vuelo = None;
                true
            }
            _ => false,
        }
    }

    /// ¿Toca volver a pedir la salida? Cada `REFRESCO_VISOR` mientras el host
    /// está en marcha y una vez más (la definitiva) cuando termina. Nunca en
    /// cola, con una petición en vuelo o si el host ya no está en el servidor.
    pub fn toca_pedir(&self, ahora: Instant, estado: Option<EstadoHostEjecucion>) -> bool {
        if self.completa || self.en_vuelo.is_some() {
            return false;
        }
        match estado {
            None | Some(EstadoHostEjecucion::EnCola) => false,
            Some(estado) if estado.es_final() => true,
            Some(_) => self
                .ultima_peticion
                .is_none_or(|ultima| ahora.saturating_duration_since(ultima) >= REFRESCO_VISOR),
        }
    }

    /// Salida recibida: líneas limpias y desplazamientos al día (pegado al
    /// final si se estaba siguiendo; si no, acotado a lo que haya).
    pub fn recibir(
        &mut self,
        stdout: &[u8],
        stderr: &[u8],
        truncada: bool,
        completa: bool,
        altos: [usize; 2],
    ) {
        self.lineas = [lineas_pantalla(stdout), lineas_pantalla(stderr)];
        self.truncada = truncada;
        self.recibida = true;
        self.completa = completa;
        self.en_vuelo = None;
        for (indice, alto) in altos.into_iter().enumerate() {
            let maximo = maximo_desplazamiento(self.lineas[indice].len(), alto);
            self.desplazamiento[indice] = if self.siguiendo[indice] {
                maximo
            } else {
                self.desplazamiento[indice].min(maximo)
            };
            self.horizontal[indice] = self.horizontal[indice].min(self.ancho_maximo(indice));
        }
    }

    /// Mueve el flujo con el foco `delta` líneas (negativo, hacia arriba).
    pub fn desplazar(&mut self, delta: isize, alto: usize) {
        let indice = self.foco.indice();
        let maximo = maximo_desplazamiento(self.lineas[indice].len(), alto);
        let actual = self.desplazamiento[indice].min(maximo);
        let nuevo = if delta < 0 {
            actual.saturating_sub(delta.unsigned_abs())
        } else {
            actual.saturating_add(delta as usize).min(maximo)
        };
        self.desplazamiento[indice] = nuevo;
        self.siguiendo[indice] = delta >= 0 && nuevo >= maximo;
    }

    pub fn al_principio(&mut self) {
        let indice = self.foco.indice();
        self.desplazamiento[indice] = 0;
        self.siguiendo[indice] = false;
    }

    pub fn al_final(&mut self, alto: usize) {
        let indice = self.foco.indice();
        self.desplazamiento[indice] = maximo_desplazamiento(self.lineas[indice].len(), alto);
        self.siguiendo[indice] = true;
    }

    /// Desplazamiento horizontal del flujo con el foco.
    pub fn mover_horizontal(&mut self, delta: isize) {
        let indice = self.foco.indice();
        let actual = self.horizontal[indice];
        let nuevo = if delta < 0 {
            actual.saturating_sub(delta.unsigned_abs())
        } else {
            actual.saturating_add(delta as usize)
        };
        self.horizontal[indice] = nuevo.min(self.ancho_maximo(indice));
    }

    /// Hasta dónde se puede desplazar en horizontal: que quede a la vista al
    /// menos el último carácter de la línea más larga.
    fn ancho_maximo(&self, indice: usize) -> usize {
        self.lineas[indice]
            .iter()
            .map(|linea| linea.chars().count())
            .max()
            .unwrap_or(0)
            .saturating_sub(1)
    }

    /// Teclas de movimiento del visor; `false` si la tecla no es suya.
    pub fn tecla(&mut self, codigo: KeyCode, altos: [usize; 2]) -> bool {
        let alto = altos[self.foco.indice()].max(1);
        match codigo {
            KeyCode::Tab | KeyCode::BackTab => self.foco = self.foco.otro(),
            KeyCode::Up | KeyCode::Char('k') => self.desplazar(-1, alto),
            KeyCode::Down | KeyCode::Char('j') => self.desplazar(1, alto),
            KeyCode::PageUp => self.desplazar(-(alto as isize), alto),
            KeyCode::PageDown => self.desplazar(alto as isize, alto),
            KeyCode::Home => self.al_principio(),
            KeyCode::End => self.al_final(alto),
            KeyCode::Left => self.mover_horizontal(-(PASO_HORIZONTAL as isize)),
            KeyCode::Right => self.mover_horizontal(PASO_HORIZONTAL as isize),
            _ => return false,
        }
        true
    }
}

/// Líneas de una salida tal como se pintan: limpias (sin escapes ni
/// controles) y sin marcas de dirección de texto.
fn lineas_pantalla(bytes: &[u8]) -> Vec<String> {
    lineas_limpias(bytes)
        .into_iter()
        .map(|linea| sin_marcas(&linea))
        .collect()
}

/// ¿Es esta la salida definitiva del host? Terminado y con todos sus bytes
/// (hasta el tope de 1 MiB por flujo), según la última difusión: así una
/// respuesta atrasada nunca pasa por la definitiva.
fn salida_completa(host: &InfoEjecucionHost, stdout: usize, stderr: usize) -> bool {
    let tope = crate::servidor::ejecuciones::TOPE_SALIDA as u64;
    host.estado.es_final()
        && stdout as u64 >= host.bytes_stdout.min(tope)
        && stderr as u64 >= host.bytes_stderr.min(tope)
}

/// Un guardado de `s` a la espera de las salidas de sus hosts. Sin `Debug`,
/// como el visor (T44).
pub struct GuardadoPendiente {
    pub ruta: PathBuf,
    /// La ejecución al empezar; al completarse se cambia por la de la última
    /// difusión si sigue ahí (estados y códigos finales).
    pub ejecucion: InfoEjecucion,
    /// Hosts a guardar, en el orden de la ejecución.
    pub host_ids: Vec<i64>,
    /// Salidas recibidas: stdout, stderr y si se truncó.
    pub partes: HashMap<i64, (Vec<u8>, Vec<u8>, bool)>,
    pub desde: Instant,
}

impl GuardadoPendiente {
    /// Hosts cuya salida aún no ha llegado.
    pub fn faltan(&self) -> usize {
        self.host_ids
            .iter()
            .filter(|id| !self.partes.contains_key(id))
            .count()
    }

    pub fn completo(&self) -> bool {
        self.faltan() == 0
    }

    /// Acepta la salida si es de un host que falta; `false` si no la pidió.
    pub fn recibir(
        &mut self,
        ejecucion_id: u32,
        host_id: i64,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        truncada: bool,
    ) -> bool {
        if ejecucion_id != self.ejecucion.id
            || !self.host_ids.contains(&host_id)
            || self.partes.contains_key(&host_id)
        {
            return false;
        }
        self.partes.insert(host_id, (stdout, stderr, truncada));
        true
    }

    /// La ruta y el contenido del fichero: cabecera «snippet · fecha» y, por
    /// host, su estado y sus dos flujos tal cual llegaron.
    pub fn armar(mut self) -> (PathBuf, Vec<u8>) {
        let cabecera = format!(
            "{} · {}",
            self.ejecucion.nombre,
            fecha_larga(self.ejecucion.creada_en)
        );
        let mut partes = Vec::new();
        for host_id in &self.host_ids {
            let Some((stdout, stderr, truncada)) = self.partes.remove(host_id) else {
                continue;
            };
            let host = self
                .ejecucion
                .hosts
                .iter()
                .find(|host| host.host_id == *host_id);
            let estado = match host {
                Some(host) => match &host.error {
                    Some(error) => format!("{} ({})", host.estado.texto(), limpio(error)),
                    None => host.estado.texto().to_string(),
                },
                None => "desconocido".to_string(),
            };
            partes.push(ParteSalida {
                host: host
                    .map(|host| host.nombre.clone())
                    .unwrap_or_else(|| format!("host {host_id}")),
                estado,
                codigo: host.and_then(|host| host.codigo),
                duracion_ms: host.and_then(|host| host.duracion_ms),
                stdout,
                stderr,
                truncada,
            });
        }
        let contenido = fichero_de_salida(&cabecera, &partes);
        (self.ruta, contenido)
    }
}

/// Fecha y hora locales de una época en segundos.
fn fecha_larga(epoca: i64) -> String {
    use chrono::TimeZone as _;
    match chrono::Local.timestamp_opt(epoca, 0).single() {
        Some(fecha) => fecha.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => epoca.to_string(),
    }
}

/// Qué se guarda con `s`: el host del visor o del panel de hosts, o todos los
/// de la ejecución desde el panel de ejecuciones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjetivoGuardado {
    pub ejecucion_id: u32,
    pub snippet: String,
    pub host_ids: Vec<i64>,
    pub host: Option<String>,
}

/// Resultado de recibir una `Salida`.
#[derive(Default)]
pub struct Recepcion {
    /// La esperaba el visor o el guardado.
    pub aceptada: bool,
    /// Guardado que acaba de completarse: toca escribirlo.
    pub completo: Option<GuardadoPendiente>,
}

/// Lo que pide el reloj de la vista.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tick {
    pub repintar: bool,
    /// Volver a pedir la salida del visor (ejecución, host).
    pub pedir: Option<(u32, i64)>,
    /// Un guardado caducó: cuántos hosts faltaban.
    pub guardado_caducado: Option<usize>,
}

/// Estado de la vista Resultados.
pub struct EstadoResultados {
    /// Última difusión de ejecuciones, la más reciente primero.
    pub ejecuciones: Vec<InfoEjecucion>,
    /// Ejecución seleccionada, por id (la lista se reordena con cada
    /// difusión).
    pub seleccionada: Option<u32>,
    pub panel: PanelResultados,
    /// Host seleccionado dentro de la ejecución seleccionada.
    pub host_seleccionado: usize,
    pub desplazamiento_ejecuciones: usize,
    pub desplazamiento_hosts: usize,
    pub visor: Option<Visor>,
    /// Lanzamientos en vuelo de esta ventana, por `peticion_id`.
    pub lanzamientos: HashMap<u64, Lanzamiento>,
    /// Aceptados por el servidor (`Hecho`) cuya ejecución aún no ha llegado en
    /// la difusión, por `peticion_id`.
    aceptados: HashMap<u64, PlanEjecucion>,
    /// Planes de las ejecuciones que lanzó esta ventana, por id de ejecución
    /// (para `r`).
    pub planes: HashMap<u32, PlanEjecucion>,
    /// El lanzamiento cuya ejecución hay que seleccionar cuando aparezca.
    seleccionar_peticion: Option<u64>,
    pub guardado: Option<GuardadoPendiente>,
    siguiente_peticion: u64,
}

impl Default for EstadoResultados {
    fn default() -> Self {
        Self {
            ejecuciones: Vec::new(),
            seleccionada: None,
            panel: PanelResultados::Ejecuciones,
            host_seleccionado: 0,
            desplazamiento_ejecuciones: 0,
            desplazamiento_hosts: 0,
            visor: None,
            lanzamientos: HashMap::new(),
            aceptados: HashMap::new(),
            planes: HashMap::new(),
            seleccionar_peticion: None,
            guardado: None,
            siguiente_peticion: RANGO_EJECUCIONES,
        }
    }
}

/// Un `LanzarEjecucion` de esta ventana esperando respuesta.
#[derive(Debug, Clone)]
pub struct Lanzamiento {
    pub plan: PlanEjecucion,
    /// La deliberación que lo autorizó: si el servidor lo rechaza, su fila se
    /// cierra con `ejecucion_resultado = error` (nadie más lo haría).
    pub deliberacion_id: Option<i64>,
}

/// Acciones confirmadas de la vista (viajan en `AccionDialogo::Resultados`).
pub enum AccionResultados {
    /// Cancelar una ejecución en curso (id fijado al confirmar).
    Cancelar(u32),
    /// Sobrescribir un fichero de salida que ya existe.
    Sobrescribir {
        ruta: String,
        ejecucion_id: u32,
        host_ids: Vec<i64>,
    },
}

impl EstadoResultados {
    // ------------------------------------------------------------ consultas

    pub fn ejecucion(&self, id: u32) -> Option<&InfoEjecucion> {
        self.ejecuciones.iter().find(|ejecucion| ejecucion.id == id)
    }

    /// Posición de la seleccionada en la lista.
    pub fn indice_seleccionada(&self) -> Option<usize> {
        let id = self.seleccionada?;
        self.ejecuciones
            .iter()
            .position(|ejecucion| ejecucion.id == id)
    }

    pub fn ejecucion_seleccionada(&self) -> Option<&InfoEjecucion> {
        self.ejecucion(self.seleccionada?)
    }

    /// El host seleccionado de la ejecución seleccionada.
    pub fn host_actual(&self) -> Option<&InfoEjecucionHost> {
        self.ejecucion_seleccionada()?
            .hosts
            .get(self.host_seleccionado)
    }

    pub fn panel_hosts(&self) -> bool {
        self.panel == PanelResultados::Hosts
    }

    pub fn estado_host(&self, ejecucion_id: u32, host_id: i64) -> Option<&InfoEjecucionHost> {
        self.ejecucion(ejecucion_id)?
            .hosts
            .iter()
            .find(|host| host.host_id == host_id)
    }

    /// Ejecuciones en curso.
    pub fn en_curso(&self) -> usize {
        self.ejecuciones
            .iter()
            .filter(|ejecucion| ejecucion.estado == EstadoEjecucion::EnCurso)
            .count()
    }

    /// La ejecución sobre la que actúan `x` y `r`: la del visor o la
    /// seleccionada.
    pub fn id_objetivo(&self) -> Option<u32> {
        match &self.visor {
            Some(visor) => Some(visor.ejecucion_id),
            None => self.seleccionada,
        }
    }

    /// Qué guarda `s` (ver `ObjetivoGuardado`).
    pub fn objetivo_guardado(&self) -> Option<ObjetivoGuardado> {
        if let Some(visor) = &self.visor {
            let ejecucion = self.ejecucion(visor.ejecucion_id);
            let host = ejecucion
                .and_then(|ejecucion| {
                    ejecucion
                        .hosts
                        .iter()
                        .find(|host| host.host_id == visor.host_id)
                })
                .map(|host| host.nombre.clone())
                .unwrap_or_else(|| visor.host.clone());
            return Some(ObjetivoGuardado {
                ejecucion_id: visor.ejecucion_id,
                snippet: ejecucion
                    .map(|ejecucion| ejecucion.nombre.clone())
                    .unwrap_or_else(|| visor.snippet.clone()),
                host_ids: vec![visor.host_id],
                host: Some(host),
            });
        }
        let ejecucion = self.ejecucion_seleccionada()?;
        if self.panel_hosts() {
            let host = ejecucion.hosts.get(self.host_seleccionado)?;
            return Some(ObjetivoGuardado {
                ejecucion_id: ejecucion.id,
                snippet: ejecucion.nombre.clone(),
                host_ids: vec![host.host_id],
                host: Some(host.nombre.clone()),
            });
        }
        Some(ObjetivoGuardado {
            ejecucion_id: ejecucion.id,
            snippet: ejecucion.nombre.clone(),
            host_ids: ejecucion.hosts.iter().map(|host| host.host_id).collect(),
            host: None,
        })
    }

    // ------------------------------------------------------------ lanzamientos

    /// Registra un lanzamiento y devuelve su `peticion_id`; su ejecución se
    /// seleccionará en cuanto llegue en la difusión.
    pub fn registrar(&mut self, plan: PlanEjecucion, deliberacion_id: Option<i64>) -> u64 {
        let peticion_id = self.siguiente_peticion;
        self.siguiente_peticion += 1;
        self.lanzamientos.insert(
            peticion_id,
            Lanzamiento {
                plan,
                deliberacion_id,
            },
        );
        self.seleccionar_peticion = Some(peticion_id);
        peticion_id
    }

    /// `Hecho` de un lanzamiento: devuelve su plan si estaba en vuelo (si no,
    /// la respuesta se descarta).
    pub fn hecho(&mut self, peticion_id: u64, cliente_id: Option<u32>) -> Option<PlanEjecucion> {
        let lanzamiento = self.lanzamientos.remove(&peticion_id)?;
        // Si la difusión ya la trajo, su plan ya está en `planes`.
        let ya_llego = cliente_id.is_some_and(|cliente| {
            self.ejecuciones.iter().any(|ejecucion| {
                ejecucion.solicitante == cliente && ejecucion.peticion_id == peticion_id
            })
        });
        if !ya_llego {
            self.aceptados.insert(peticion_id, lanzamiento.plan.clone());
        }
        Some(lanzamiento.plan)
    }

    /// `Error` de un lanzamiento: lo devuelve si estaba en vuelo (si no, la
    /// respuesta se descarta).
    pub fn error(&mut self, peticion_id: u64) -> Option<Lanzamiento> {
        let lanzamiento = self.lanzamientos.remove(&peticion_id)?;
        if self.seleccionar_peticion == Some(peticion_id) {
            self.seleccionar_peticion = None;
        }
        Some(lanzamiento)
    }

    /// Difusión `Ejecuciones`: guarda la lista (la más reciente primero),
    /// reconoce los lanzamientos propios (y selecciona el último, una vez) y
    /// conserva la selección por id; si la seleccionada desaparece, se queda
    /// la que ocupe su sitio.
    pub fn actualizar(&mut self, mut lista: Vec<InfoEjecucion>, cliente_id: Option<u32>) {
        let indice_previo = self.indice_seleccionada();
        lista.sort_by_key(|ejecucion| std::cmp::Reverse(ejecucion.id));
        self.ejecuciones = lista;

        if let Some(id) = self.emparejar(cliente_id) {
            self.seleccionada = Some(id);
            self.panel = PanelResultados::Ejecuciones;
            self.host_seleccionado = 0;
            self.desplazamiento_hosts = 0;
            if self
                .visor
                .as_ref()
                .is_some_and(|visor| visor.ejecucion_id != id)
            {
                self.visor = None;
            }
        }

        let sigue = self
            .seleccionada
            .is_some_and(|id| self.ejecucion(id).is_some());
        if !sigue {
            let nueva = if self.ejecuciones.is_empty() {
                None
            } else {
                let indice = indice_previo.unwrap_or(0).min(self.ejecuciones.len() - 1);
                Some(self.ejecuciones[indice].id)
            };
            if nueva != self.seleccionada {
                self.host_seleccionado = 0;
                self.desplazamiento_hosts = 0;
            }
            self.seleccionada = nueva;
            if nueva.is_none() {
                self.panel = PanelResultados::Ejecuciones;
            }
        }

        let hosts = self
            .ejecucion_seleccionada()
            .map_or(0, |ejecucion| ejecucion.hosts.len());
        if self.host_seleccionado >= hosts {
            self.host_seleccionado = hosts.saturating_sub(1);
        }
        // Lo que ya no está en el servidor no se puede repetir con su plan.
        let ejecuciones = &self.ejecuciones;
        self.planes
            .retain(|id, _| ejecuciones.iter().any(|ejecucion| ejecucion.id == *id));
    }

    /// Reconoce en la lista los lanzamientos de esta ventana (por
    /// `peticion_id` y solicitante): pasa su plan a `planes` y devuelve la
    /// ejecución que hay que seleccionar, si llegó.
    fn emparejar(&mut self, cliente_id: Option<u32>) -> Option<u32> {
        let cliente = cliente_id?;
        let mut elegida = None;
        for ejecucion in &self.ejecuciones {
            if ejecucion.solicitante != cliente {
                continue;
            }
            let peticion = ejecucion.peticion_id;
            let plan = match self.aceptados.remove(&peticion) {
                Some(plan) => plan,
                None => match self.lanzamientos.get(&peticion) {
                    Some(lanzamiento) => lanzamiento.plan.clone(),
                    None => continue,
                },
            };
            self.planes.entry(ejecucion.id).or_insert(plan);
            if self.seleccionar_peticion == Some(peticion) {
                self.seleccionar_peticion = None;
                elegida = Some(ejecucion.id);
            }
        }
        elegida
    }

    /// El servidor ha caído: sus ejecuciones, las salidas y los lanzamientos
    /// en vuelo se olvidan.
    pub fn servidor_caido(&mut self) {
        self.ejecuciones.clear();
        self.seleccionada = None;
        self.panel = PanelResultados::Ejecuciones;
        self.host_seleccionado = 0;
        self.desplazamiento_ejecuciones = 0;
        self.desplazamiento_hosts = 0;
        self.visor = None;
        self.lanzamientos.clear();
        self.aceptados.clear();
        self.planes.clear();
        self.seleccionar_peticion = None;
        self.guardado = None;
    }

    // ------------------------------------------------------------ navegación

    /// Lleva la selección del panel activo a `destino` (acotado); `alturas`
    /// son las filas visibles de cada panel.
    pub fn mover(&mut self, destino: usize, alturas: (usize, usize)) {
        match self.panel {
            PanelResultados::Ejecuciones => {
                if self.ejecuciones.is_empty() {
                    return;
                }
                let indice = destino.min(self.ejecuciones.len() - 1);
                let id = self.ejecuciones[indice].id;
                if self.seleccionada != Some(id) {
                    self.seleccionada = Some(id);
                    self.host_seleccionado = 0;
                    self.desplazamiento_hosts = 0;
                }
                self.desplazamiento_ejecuciones = desplazamiento_ajustado(
                    indice,
                    self.desplazamiento_ejecuciones,
                    alturas.0,
                    self.ejecuciones.len(),
                );
            }
            PanelResultados::Hosts => {
                let total = self
                    .ejecucion_seleccionada()
                    .map_or(0, |ejecucion| ejecucion.hosts.len());
                if total == 0 {
                    return;
                }
                self.host_seleccionado = destino.min(total - 1);
                self.desplazamiento_hosts = desplazamiento_ajustado(
                    self.host_seleccionado,
                    self.desplazamiento_hosts,
                    alturas.1,
                    total,
                );
            }
        }
    }

    /// Posición de la selección en el panel activo.
    pub fn posicion(&self) -> usize {
        match self.panel {
            PanelResultados::Ejecuciones => self.indice_seleccionada().unwrap_or(0),
            PanelResultados::Hosts => self.host_seleccionado,
        }
    }

    /// `Tab`: ejecuciones ⇄ hosts (sin selección no hay hosts).
    pub fn cambiar_panel(&mut self) {
        self.panel = match self.panel {
            PanelResultados::Ejecuciones if self.host_actual().is_some() => PanelResultados::Hosts,
            _ => PanelResultados::Ejecuciones,
        };
    }

    // ------------------------------------------------------------ salida

    /// Abre el visor sobre el host seleccionado y marca su petición como
    /// enviada; devuelve qué hay que pedir.
    pub fn abrir_visor(&mut self, ahora: Instant) -> Option<(u32, i64)> {
        let ejecucion = self.ejecucion_seleccionada()?;
        let host = ejecucion.hosts.get(self.host_seleccionado)?;
        let mut visor = Visor::nuevo(
            ejecucion.id,
            host.host_id,
            pantalla(&ejecucion.nombre),
            pantalla(&host.nombre),
            !host.estado.es_final(),
        );
        visor.pedir(ahora);
        let pedir = (visor.ejecucion_id, visor.host_id);
        self.visor = Some(visor);
        Some(pedir)
    }

    /// Una `Salida`: la toman el visor y el guardado si la esperan; lo demás
    /// se descarta.
    pub fn recibir_salida(
        &mut self,
        ejecucion_id: u32,
        host_id: i64,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        truncada: bool,
        altos: [usize; 2],
    ) -> Recepcion {
        let mut recepcion = Recepcion::default();
        let completa = self
            .estado_host(ejecucion_id, host_id)
            .is_some_and(|host| salida_completa(host, stdout.len(), stderr.len()));
        if let Some(visor) = &mut self.visor {
            if visor.espera(ejecucion_id, host_id) {
                visor.recibir(&stdout, &stderr, truncada, completa, altos);
                recepcion.aceptada = true;
            }
        }
        let mut terminado = false;
        if let Some(guardado) = &mut self.guardado {
            if guardado.recibir(ejecucion_id, host_id, stdout, stderr, truncada) {
                recepcion.aceptada = true;
                terminado = guardado.completo();
            }
        }
        if terminado {
            if let Some(mut guardado) = self.guardado.take() {
                if let Some(actual) = self.ejecucion(guardado.ejecucion.id) {
                    guardado.ejecucion = actual.clone();
                }
                recepcion.completo = Some(guardado);
            }
        }
        recepcion
    }

    /// Reloj de la vista (cada 200 ms): caduca el guardado, vuelve a pedir la
    /// salida del visor cuando toca (solo con la vista a la vista) y pide
    /// repintar mientras haya ejecuciones en curso (las duraciones cambian).
    pub fn tick(&mut self, ahora: Instant, en_vista: bool) -> Tick {
        let mut tick = Tick::default();
        if let Some(guardado) = &self.guardado {
            if ahora.saturating_duration_since(guardado.desde) >= PLAZO_GUARDADO {
                tick.guardado_caducado = Some(guardado.faltan());
                tick.repintar = true;
                self.guardado = None;
            }
        }
        if !en_vista {
            return tick;
        }
        let estado = self.visor.as_ref().and_then(|visor| {
            self.estado_host(visor.ejecucion_id, visor.host_id)
                .map(|host| host.estado)
        });
        if let Some(visor) = &mut self.visor {
            if visor.caducar(ahora) {
                tick.repintar = true;
            }
            if visor.toca_pedir(ahora, estado) {
                visor.pedir(ahora);
                tick.pedir = Some((visor.ejecucion_id, visor.host_id));
                tick.repintar = true;
            }
        }
        if self.en_curso() > 0 {
            tick.repintar = true;
        }
        tick
    }
}

/// Desplazamiento que deja la fila `seleccion` dentro de una ventana de
/// `altura` filas sobre `total`, moviéndolo lo mínimo.
fn desplazamiento_ajustado(
    seleccion: usize,
    desplazamiento: usize,
    altura: usize,
    total: usize,
) -> usize {
    let altura = altura.max(1);
    let mut desplazamiento = desplazamiento;
    if seleccion < desplazamiento {
        desplazamiento = seleccion;
    }
    if seleccion >= desplazamiento + altura {
        desplazamiento = seleccion + 1 - altura;
    }
    if desplazamiento + altura > total {
        desplazamiento = total.saturating_sub(altura);
    }
    desplazamiento
}

/// Ruta del fichero de salida tal como la escribió el usuario: `~` se
/// expande, una relativa cuelga del directorio personal (no del directorio
/// desde el que se arrancó MAGI), `.` y `..` se resuelven y dentro de
/// `~/.ssh` no se escribe nunca (MAGI no pisa ficheros de `~/.ssh`).
pub fn resolver_ruta_salida(texto: &str, hogar: &Path) -> Result<PathBuf, String> {
    let ruta = crate::conexion::cliente::expandir_home(texto.trim(), hogar);
    let ruta = if ruta.is_absolute() {
        ruta
    } else {
        hogar.join(ruta)
    };
    let ruta = normalizar(&ruta);
    if ruta.file_name().is_none() {
        return Err("la ruta no nombra un fichero".to_string());
    }
    let ssh = normalizar(&hogar.join(".ssh"));
    let dentro_de_ssh = ruta.starts_with(&ssh)
        || match (
            ruta.parent().and_then(|padre| padre.canonicalize().ok()),
            ssh.canonicalize().ok(),
        ) {
            (Some(padre), Some(ssh)) => padre.starts_with(ssh),
            _ => false,
        };
    if dentro_de_ssh {
        return Err("la salida no se guarda dentro de ~/.ssh".to_string());
    }
    Ok(ruta)
}

/// Resuelve `.` y `..` sin tocar el disco.
fn normalizar(ruta: &Path) -> PathBuf {
    let mut normalizada = PathBuf::new();
    for componente in ruta.components() {
        match componente {
            Component::CurDir => {}
            Component::ParentDir => {
                normalizada.pop();
            }
            otro => normalizada.push(otro.as_os_str()),
        }
    }
    normalizada
}

/// `~/…` si la ruta cuelga del directorio personal.
pub fn abreviar_hogar(ruta: &Path, hogar: &Path) -> String {
    match ruta.strip_prefix(hogar) {
        Ok(resto) if !resto.as_os_str().is_empty() => format!("~/{}", resto.display()),
        _ => ruta.display().to_string(),
    }
}

impl App {
    /// Entra en Resultados (desde `t` en Snippets o la paleta).
    pub(super) fn ir_a_resultados(&mut self) {
        self.salir_de_sesion_si_hace_falta();
        if !matches!(self.vista, Vista::Snippets | Vista::Resultados) {
            self.vista_previa = Some(self.vista);
        }
        self.vista = Vista::Resultados;
        self.ficha = None;
    }

    /// Filas de cada flujo del visor en el último pintado.
    fn altos_visor(&self) -> [usize; 2] {
        [
            self.disposicion.filas(Lista::VisorSalida),
            self.disposicion.filas(Lista::VisorErrores),
        ]
    }

    /// Los desplazamientos guardados pasan a ser los del último pintado
    /// (pudo hacerse con otro alto): así las teclas parten de lo que se ve.
    /// Solo si lo pintado sigue siendo la misma lista (mismo total): entre
    /// el pintado y la tecla pudo abrirse otro visor o llegar otra difusión,
    /// y su inicio no vale para la de ahora.
    pub(super) fn sincronizar_resultados(&mut self) {
        let ventana = |lista| self.disposicion.lista(lista);
        let (ejecuciones, hosts) = (
            ventana(Lista::ResultadosEjecuciones),
            ventana(Lista::ResultadosHosts),
        );
        let flujos = [ventana(Lista::VisorSalida), ventana(Lista::VisorErrores)];
        let estado = &mut self.resultados;
        let misma = |ventana: Option<VentanaLista>, total: usize| {
            ventana
                .filter(|ventana| ventana.total == total)
                .map(|ventana| ventana.inicio)
        };
        match estado.visor.as_mut() {
            Some(visor) => {
                for (indice, ventana) in flujos.into_iter().enumerate() {
                    if let Some(inicio) = misma(ventana, visor.lineas[indice].len()) {
                        visor.desplazamiento[indice] = inicio;
                    }
                }
            }
            None => {
                if let Some(inicio) = misma(ejecuciones, estado.ejecuciones.len()) {
                    estado.desplazamiento_ejecuciones = inicio;
                }
                let total_hosts = estado
                    .ejecucion_seleccionada()
                    .map_or(0, |ejecucion| ejecucion.hosts.len());
                if let Some(inicio) = misma(hosts, total_hosts) {
                    estado.desplazamiento_hosts = inicio;
                }
            }
        }
    }

    pub(super) fn tecla_resultados(&mut self, tecla: KeyEvent) {
        self.sincronizar_resultados();
        if self.resultados.visor.is_some() {
            self.tecla_visor(tecla);
            return;
        }
        let alturas = (
            self.disposicion.filas(Lista::ResultadosEjecuciones),
            self.disposicion.filas(Lista::ResultadosHosts),
        );
        let altura = if self.resultados.panel_hosts() {
            alturas.1
        } else {
            alturas.0
        }
        .max(1);
        let posicion = self.resultados.posicion();
        match tecla.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.resultados.mover(posicion.saturating_sub(1), alturas)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.resultados.mover(posicion.saturating_add(1), alturas)
            }
            KeyCode::PageUp => self
                .resultados
                .mover(posicion.saturating_sub(altura), alturas),
            KeyCode::PageDown => self
                .resultados
                .mover(posicion.saturating_add(altura), alturas),
            KeyCode::Home => self.resultados.mover(0, alturas),
            KeyCode::End => self.resultados.mover(usize::MAX, alturas),
            KeyCode::Tab | KeyCode::BackTab => self.resultados.cambiar_panel(),
            KeyCode::Enter => {
                if self.resultados.panel_hosts() {
                    self.abrir_visor();
                } else {
                    self.resultados.cambiar_panel();
                }
            }
            KeyCode::Char('s') => self.pedir_ruta_salida(),
            KeyCode::Char('x') => self.confirmar_cancelacion(),
            KeyCode::Char('r') => self.repetir_ejecucion(),
            KeyCode::Char('C') => self.limpiar_ejecuciones(),
            KeyCode::Char('q') | KeyCode::Esc => self.ir_a_snippets(),
            KeyCode::Char('?') => self.ayuda = true,
            _ => {}
        }
    }

    /// Teclas con el visor abierto.
    fn tecla_visor(&mut self, tecla: KeyEvent) {
        let altos = self.altos_visor();
        if let Some(visor) = self.resultados.visor.as_mut() {
            if visor.tecla(tecla.code, altos) {
                return;
            }
        }
        match tecla.code {
            KeyCode::Char('q') | KeyCode::Esc => self.resultados.visor = None,
            KeyCode::Char('s') => self.pedir_ruta_salida(),
            KeyCode::Char('x') => self.confirmar_cancelacion(),
            KeyCode::Char('r') => self.repetir_ejecucion(),
            KeyCode::Char('C') => self.limpiar_ejecuciones(),
            KeyCode::Char('?') => self.ayuda = true,
            _ => {}
        }
    }

    /// `↵` sobre un host: abre el visor y pide su salida.
    fn abrir_visor(&mut self) {
        if let Some((ejecucion_id, host_id)) = self.resultados.abrir_visor(Instant::now()) {
            self.servidor.enviar(MensajeCliente::PedirSalida {
                ejecucion_id,
                host_id,
            });
        }
    }

    /// `s`: diálogo de ruta con la de por defecto; la ejecución y los hosts
    /// van fijados en la acción (T33).
    fn pedir_ruta_salida(&mut self) {
        if self.resultados.guardado.is_some() {
            self.mensaje(
                "ya se está guardando una salida; espera a que termine",
                true,
            );
            return;
        }
        let Some(objetivo) = self.resultados.objetivo_guardado() else {
            return;
        };
        let ruta = ruta_por_defecto(
            &self.rutas.hogar,
            &objetivo.snippet,
            objetivo.host.as_deref(),
            chrono::Local::now(),
        );
        let titulo = if objetivo.host_ids.len() == 1 {
            "GUARDAR SALIDA"
        } else {
            "GUARDAR SALIDA DE TODOS LOS HOSTS"
        };
        self.dialogo = Some(Dialogo::EntradaTexto {
            titulo: titulo.to_string(),
            etiqueta: "Ruta".to_string(),
            campo: CampoTexto::nuevo(abreviar_hogar(Path::new(&ruta), &self.rutas.hogar)),
            accion: EntradaTextoAccion::GuardarSalida {
                ejecucion_id: objetivo.ejecucion_id,
                host_ids: objetivo.host_ids,
            },
        });
    }

    /// `x`: confirmación si está en curso; si no, aviso.
    fn confirmar_cancelacion(&mut self) {
        let Some(id) = self.resultados.id_objetivo() else {
            return;
        };
        let Some(ejecucion) = self.resultados.ejecucion(id) else {
            self.mensaje("la ejecución ya no está en el servidor", true);
            return;
        };
        let nombre = limpio(&ejecucion.nombre);
        if ejecucion.estado != EstadoEjecucion::EnCurso {
            self.mensaje(format!("«{nombre}» ya ha terminado"), false);
            return;
        }
        let pendientes = ejecucion
            .hosts
            .iter()
            .filter(|host| !host.estado.es_final())
            .count();
        let mut lineas = partir(
            &format!(
                "¿Cancelar «{nombre}»? Quedan {} sin terminar.",
                texto_hosts(pendientes)
            ),
            ANCHO_CONFIRMACION,
        );
        lineas.extend(partir(
            "Los hosts en cola se omiten y a los que están en marcha se les corta el comando.",
            ANCHO_CONFIRMACION,
        ));
        self.dialogo = Some(Dialogo::Confirmar {
            titulo: "CANCELAR EJECUCIÓN".to_string(),
            lineas,
            peligro: true,
            accion: AccionDialogo::Resultados(AccionResultados::Cancelar(id)),
        });
    }

    /// `r`: mismo snippet, hosts y variables (si los lanzó esta ventana); pasa
    /// otra vez por el diálogo o la deliberación que toque.
    fn repetir_ejecucion(&mut self) {
        let Some(id) = self.resultados.id_objetivo() else {
            return;
        };
        if let Some(plan) = self.resultados.planes.get(&id) {
            let snippet_id = plan.snippet_id;
            let origen = OrigenLanzamiento::Repetir {
                host_ids: plan.hosts.iter().map(|(host_id, _)| *host_id).collect(),
                valores: Some(plan.valores.clone()),
            };
            self.iniciar_snippet(snippet_id, origen);
            return;
        }
        let Some(ejecucion) = self.resultados.ejecucion(id) else {
            self.mensaje("la ejecución ya no está en el servidor", true);
            return;
        };
        match ejecucion.snippet_id {
            Some(snippet_id) => {
                let origen = OrigenLanzamiento::Repetir {
                    host_ids: ejecucion.hosts.iter().map(|host| host.host_id).collect(),
                    valores: None,
                };
                self.iniciar_snippet(snippet_id, origen);
            }
            None => self.mensaje(
                "esta ejecución no tiene snippet asociado: no se puede repetir",
                true,
            ),
        }
    }

    /// `C`: el servidor quita las terminadas y lo difunde.
    fn limpiar_ejecuciones(&mut self) {
        let terminadas = self
            .resultados
            .ejecuciones
            .iter()
            .filter(|ejecucion| ejecucion.estado != EstadoEjecucion::EnCurso)
            .count();
        if terminadas == 0 {
            self.mensaje("no hay ejecuciones terminadas que limpiar", false);
            return;
        }
        self.servidor.enviar(MensajeCliente::LimpiarEjecuciones);
        self.mensaje(
            format!("limpiando {terminadas} ejecución(es) terminada(s)"),
            false,
        );
    }

    /// Registra un lanzamiento de esta ventana y devuelve su `peticion_id`;
    /// la vista pasa a Resultados y seleccionará la ejecución cuando llegue.
    /// Con la ficha a medio editar no se cambia de vista (se perderían los
    /// cambios): la ejecución quedará seleccionada igualmente.
    pub(super) fn registrar_lanzamiento(
        &mut self,
        plan: PlanEjecucion,
        deliberacion_id: Option<i64>,
    ) -> u64 {
        let peticion_id = self.resultados.registrar(plan, deliberacion_id);
        if !self.ficha.as_ref().is_some_and(|ficha| ficha.sucio()) {
            self.ir_a_resultados();
        }
        peticion_id
    }

    /// Difusión `Ejecuciones` (y la lista de `Bienvenida`).
    pub(super) fn actualizar_ejecuciones(&mut self, lista: Vec<InfoEjecucion>) {
        self.resultados.actualizar(lista, self.cliente_id);
    }

    /// Salida de un host pedida con `PedirSalida`.
    pub(super) fn salida_recibida(
        &mut self,
        ejecucion_id: u32,
        host_id: i64,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        truncada: bool,
    ) {
        let altos = self.altos_visor();
        let recepcion =
            self.resultados
                .recibir_salida(ejecucion_id, host_id, stdout, stderr, truncada, altos);
        if let Some(guardado) = recepcion.completo {
            self.escribir_guardado(guardado);
        }
    }

    /// `Hecho` de un `LanzarEjecucion`; `true` si el id es de ejecuciones.
    pub(super) fn hecho_de_ejecucion(&mut self, peticion_id: u64) -> bool {
        if !es_de_ejecuciones(peticion_id) {
            return false;
        }
        let Some(plan) = self.resultados.hecho(peticion_id, self.cliente_id) else {
            return true;
        };
        if let Err(error) = self.almacen.marcar_uso_snippet(plan.snippet_id) {
            warn!(
                "no se pudo anotar el uso del snippet {}: {error}",
                plan.snippet_id
            );
        }
        self.recargar_snippets();
        self.mensaje(
            format!(
                "«{}» lanzado en {}",
                limpio(&plan.nombre),
                texto_hosts(plan.hosts.len())
            ),
            false,
        );
        true
    }

    /// `Error` de un `LanzarEjecucion`; `true` si el id es de ejecuciones
    /// (aunque ya no esté en vuelo: entonces se descarta).
    pub(super) fn error_de_ejecucion(&mut self, peticion_id: u64, mensaje: &str) -> bool {
        if !es_de_ejecuciones(peticion_id) {
            return false;
        }
        let Some(lanzamiento) = self.resultados.error(peticion_id) else {
            return true;
        };
        self.mensaje(
            format!(
                "no se pudo lanzar «{}»: {}",
                limpio(&lanzamiento.plan.nombre),
                limpio(mensaje)
            ),
            true,
        );
        if let Some(deliberacion_id) = lanzamiento.deliberacion_id {
            if let Err(error) = self
                .almacen
                .fijar_resultado_deliberacion(deliberacion_id, EjecucionResultado::Error)
            {
                warn!("no se pudo cerrar la deliberación {deliberacion_id}: {error}");
            }
        }
        true
    }

    /// El servidor ha caído: las ejecuciones, los lanzamientos en vuelo, el
    /// visor y el guardado se olvidan.
    pub(super) fn resultados_servidor_caido(&mut self) {
        self.resultados.servidor_caido();
    }

    /// Refresco del visor mientras el host corre y caducidad del guardado;
    /// devuelve si hay que repintar.
    pub(super) fn tick_resultados(&mut self) -> bool {
        let en_vista = self.vista == Vista::Resultados;
        let tick = self.resultados.tick(Instant::now(), en_vista);
        if let Some((ejecucion_id, host_id)) = tick.pedir {
            self.servidor.enviar(MensajeCliente::PedirSalida {
                ejecucion_id,
                host_id,
            });
        }
        if let Some(faltan) = tick.guardado_caducado {
            self.mensaje(
                format!(
                    "no llegó a tiempo la salida de {}: no se ha guardado nada",
                    texto_hosts(faltan)
                ),
                true,
            );
        }
        tick.repintar
    }

    /// Ruta elegida en el diálogo de `s`: si el fichero existe se pide
    /// confirmación; si no, se piden las salidas y se escriben al llegar.
    pub(super) fn guardar_salida_en(
        &mut self,
        ruta: String,
        ejecucion_id: u32,
        host_ids: Vec<i64>,
    ) {
        let ruta = match resolver_ruta_salida(&ruta, &self.rutas.hogar) {
            Ok(ruta) => ruta,
            Err(error) => {
                self.mensaje(error, true);
                return;
            }
        };
        let mostrada = limpio(&abreviar_hogar(&ruta, &self.rutas.hogar));
        match std::fs::metadata(&ruta) {
            Ok(datos) if datos.is_dir() => {
                self.mensaje(format!("«{mostrada}» es un directorio"), true);
            }
            Ok(_) => {
                self.dialogo = Some(Dialogo::Confirmar {
                    titulo: "SOBRESCRIBIR FICHERO".to_string(),
                    lineas: partir(
                        &format!("«{mostrada}» ya existe. ¿Sobrescribirlo con la salida?"),
                        ANCHO_CONFIRMACION,
                    ),
                    peligro: true,
                    accion: AccionDialogo::Resultados(AccionResultados::Sobrescribir {
                        ruta: ruta.display().to_string(),
                        ejecucion_id,
                        host_ids,
                    }),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.empezar_guardado(ruta, ejecucion_id, host_ids);
            }
            Err(error) => {
                self.mensaje(format!("no se puede usar «{mostrada}»: {error}"), true);
            }
        }
    }

    /// Pide la salida de cada host y deja el guardado esperándolas (con
    /// plazo); se escribe cuando han llegado todas.
    fn empezar_guardado(&mut self, ruta: PathBuf, ejecucion_id: u32, host_ids: Vec<i64>) {
        if self.resultados.guardado.is_some() {
            self.mensaje(
                "ya se está guardando una salida; espera a que termine",
                true,
            );
            return;
        }
        let Some(ejecucion) = self.resultados.ejecucion(ejecucion_id).cloned() else {
            self.mensaje(
                "la ejecución ya no está en el servidor: no hay salida que guardar",
                true,
            );
            return;
        };
        // En el orden de la ejecución, sin repetidos ni hosts ajenos.
        let host_ids: Vec<i64> = ejecucion
            .hosts
            .iter()
            .map(|host| host.host_id)
            .filter(|id| host_ids.contains(id))
            .collect();
        if host_ids.is_empty() {
            self.mensaje("esos hosts ya no están en la ejecución", true);
            return;
        }
        for host_id in &host_ids {
            self.servidor.enviar(MensajeCliente::PedirSalida {
                ejecucion_id,
                host_id: *host_id,
            });
        }
        self.resultados.guardado = Some(GuardadoPendiente {
            ruta,
            ejecucion,
            host_ids,
            partes: HashMap::new(),
            desde: Instant::now(),
        });
        self.mensaje("pidiendo la salida al servidor…", false);
    }

    /// Escribe el fichero de un guardado completo (`ficheros.rs`, 600).
    fn escribir_guardado(&mut self, guardado: GuardadoPendiente) {
        let (ruta, contenido) = guardado.armar();
        let mostrada = limpio(&abreviar_hogar(&ruta, &self.rutas.hogar));
        match crate::ficheros::escribir_atomico(&ruta, &contenido, 0o600) {
            Ok(()) => self.mensaje(format!("salida guardada en {mostrada}"), false),
            Err(error) => self.mensaje(
                limpio(&format!(
                    "no se pudo guardar la salida en {mostrada}: {error:#}"
                )),
                true,
            ),
        }
    }

    pub(super) fn ejecutar_accion_resultados(&mut self, accion: AccionResultados) {
        match accion {
            AccionResultados::Cancelar(id) => {
                self.servidor
                    .enviar(MensajeCliente::CancelarEjecucion { id });
                self.mensaje("cancelando la ejecución…", false);
            }
            AccionResultados::Sobrescribir {
                ruta,
                ejecucion_id,
                host_ids,
            } => self.empezar_guardado(PathBuf::from(ruta), ejecucion_id, host_ids),
        }
    }
}

#[cfg(test)]
mod pruebas {
    use std::collections::BTreeMap;

    use super::*;
    use crate::app::lanzar::ModoLanzamiento;

    fn host(host_id: i64, nombre: &str, estado: EstadoHostEjecucion) -> InfoEjecucionHost {
        InfoEjecucionHost {
            host_id,
            nombre: nombre.to_string(),
            estado,
            codigo: None,
            inicio_ms: None,
            duracion_ms: None,
            bytes_stdout: 0,
            bytes_stderr: 0,
            truncada: false,
            error: None,
        }
    }

    fn ejecucion(id: u32, peticion_id: u64, solicitante: u32) -> InfoEjecucion {
        InfoEjecucion {
            id,
            peticion_id,
            solicitante,
            snippet_id: Some(7),
            nombre: "reiniciar nginx".to_string(),
            hosts: vec![
                host(10, "web-01", EstadoHostEjecucion::Ok),
                host(11, "web-02", EstadoHostEjecucion::Ejecutando),
            ],
            timeout_seg: 60,
            parar_al_fallo: false,
            deliberacion_id: None,
            forzada: false,
            estado: EstadoEjecucion::EnCurso,
            creada_en: 1_790_000_000,
            terminada_en: None,
        }
    }

    fn plan(nombre: &str) -> PlanEjecucion {
        PlanEjecucion {
            snippet_id: 7,
            nombre: nombre.to_string(),
            comando: "systemctl restart nginx".to_string(),
            hosts: vec![(10, "web-01".to_string()), (11, "web-02".to_string())],
            timeout_seg: 60,
            parar_al_fallo: false,
            critico: false,
            valores: BTreeMap::from([("servicio".to_string(), "nginx".to_string())]),
            modo: ModoLanzamiento::Servidor,
        }
    }

    #[test]
    fn el_rango_de_peticion_id_es_el_de_ejecuciones() {
        assert!(!es_de_ejecuciones(0));
        assert!(!es_de_ejecuciones(1 << 40));
        assert!(!es_de_ejecuciones(RANGO_EJECUCIONES - 1));
        assert!(es_de_ejecuciones(RANGO_EJECUCIONES));
        assert!(es_de_ejecuciones(crate::cliente::RANGO_ESPERAS - 1));
        assert!(!es_de_ejecuciones(crate::cliente::RANGO_ESPERAS));

        let mut estado = EstadoResultados::default();
        let primera = estado.registrar(plan("a"), None);
        let segunda = estado.registrar(plan("b"), None);
        assert_eq!(primera, RANGO_EJECUCIONES);
        assert_eq!(segunda, RANGO_EJECUCIONES + 1);
        assert!(es_de_ejecuciones(primera) && es_de_ejecuciones(segunda));
    }

    #[test]
    fn se_selecciona_el_lanzamiento_propio_cuando_llega_tras_el_hecho() {
        let mut estado = EstadoResultados::default();
        estado.actualizar(vec![ejecucion(1, 99, 3)], Some(3));
        assert_eq!(estado.seleccionada, Some(1));

        let peticion = estado.registrar(plan("reiniciar nginx"), None);
        let aceptado = estado.hecho(peticion, Some(3)).expect("en vuelo");
        assert_eq!(aceptado.nombre, "reiniciar nginx");
        // Un segundo `Hecho` del mismo id ya no está en vuelo.
        assert!(estado.hecho(peticion, Some(3)).is_none());

        // La misma petición de otra ventana no es la nuestra.
        estado.actualizar(
            vec![ejecucion(1, 99, 3), ejecucion(2, peticion, 4)],
            Some(3),
        );
        assert_eq!(estado.seleccionada, Some(1));
        assert!(estado.planes.is_empty());

        estado.actualizar(
            vec![
                ejecucion(1, 99, 3),
                ejecucion(2, peticion, 4),
                ejecucion(3, peticion, 3),
            ],
            Some(3),
        );
        assert_eq!(estado.seleccionada, Some(3));
        assert_eq!(estado.planes.get(&3).map(|plan| plan.snippet_id), Some(7));
        // La más reciente primero.
        let ids: Vec<u32> = estado.ejecuciones.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![3, 2, 1]);

        // Se selecciona una vez: después manda el usuario.
        estado.mover(2, (5, 5));
        assert_eq!(estado.seleccionada, Some(1));
        estado.actualizar(
            vec![
                ejecucion(1, 99, 3),
                ejecucion(2, peticion, 4),
                ejecucion(3, peticion, 3),
            ],
            Some(3),
        );
        assert_eq!(estado.seleccionada, Some(1));
        assert!(estado.planes.contains_key(&3));
    }

    #[test]
    fn se_selecciona_el_lanzamiento_propio_aunque_la_difusion_llegue_antes() {
        let mut estado = EstadoResultados::default();
        let peticion = estado.registrar(plan("uptime"), Some(5));
        estado.actualizar(vec![ejecucion(8, peticion, 1)], Some(1));
        assert_eq!(estado.seleccionada, Some(8));
        assert!(estado.planes.contains_key(&8));
        // El `Hecho` que llega después aún marca el uso (devuelve el plan).
        assert!(estado.hecho(peticion, Some(1)).is_some());
        assert!(estado.lanzamientos.is_empty());
        assert!(estado.aceptados.is_empty());
    }

    #[test]
    fn un_error_solo_cuenta_si_el_lanzamiento_estaba_en_vuelo() {
        let mut estado = EstadoResultados::default();
        let peticion = estado.registrar(plan("borrar logs"), Some(42));
        let lanzamiento = estado.error(peticion).expect("en vuelo");
        assert_eq!(lanzamiento.deliberacion_id, Some(42));
        assert!(estado.error(peticion).is_none());
        assert!(estado.hecho(peticion, Some(1)).is_none());
        // Ya no se selecciona nada con ese id.
        estado.actualizar(vec![ejecucion(4, 99, 1)], Some(1));
        assert!(estado.planes.is_empty());
    }

    #[test]
    fn la_seleccion_se_conserva_por_id_y_se_ajusta_si_desaparece() {
        let mut estado = EstadoResultados::default();
        estado.actualizar(
            vec![ejecucion(1, 1, 9), ejecucion(2, 2, 9), ejecucion(3, 3, 9)],
            None,
        );
        assert_eq!(estado.seleccionada, Some(3));
        estado.mover(1, (5, 5));
        assert_eq!(estado.seleccionada, Some(2));
        estado.cambiar_panel();
        assert!(estado.panel_hosts());
        estado.mover(1, (5, 5));
        assert_eq!(estado.host_seleccionado, 1);

        // Llega una nueva por arriba: la selección sigue en la misma.
        estado.actualizar(
            vec![
                ejecucion(1, 1, 9),
                ejecucion(2, 2, 9),
                ejecucion(3, 3, 9),
                ejecucion(4, 4, 9),
            ],
            None,
        );
        assert_eq!(estado.seleccionada, Some(2));
        assert_eq!(estado.host_seleccionado, 1);

        // Desaparece: se queda la que ocupa su sitio, con el primer host.
        estado.actualizar(
            vec![ejecucion(1, 1, 9), ejecucion(3, 3, 9), ejecucion(4, 4, 9)],
            None,
        );
        assert_eq!(estado.seleccionada, Some(1));
        assert_eq!(estado.host_seleccionado, 0);

        estado.actualizar(Vec::new(), None);
        assert_eq!(estado.seleccionada, None);
        assert!(!estado.panel_hosts());
    }

    fn estado_con_visor(ahora: Instant) -> EstadoResultados {
        let mut estado = EstadoResultados::default();
        estado.actualizar(vec![ejecucion(1, 1, 1), ejecucion(2, 2, 1)], None);
        estado.mover(1, (5, 5));
        estado.cambiar_panel();
        estado.mover(1, (5, 5));
        assert_eq!(estado.abrir_visor(ahora), Some((1, 11)));
        estado
    }

    #[test]
    fn la_salida_no_pedida_se_descarta() {
        let ahora = Instant::now();
        let mut estado = estado_con_visor(ahora);
        let altos = [5, 5];
        // Otro host, otra ejecución: no.
        let otra = estado.recibir_salida(1, 10, b"no".to_vec(), Vec::new(), false, altos);
        assert!(!otra.aceptada);
        let otra = estado.recibir_salida(2, 11, b"no".to_vec(), Vec::new(), false, altos);
        assert!(!otra.aceptada);
        assert!(!estado.visor.as_ref().unwrap().recibida);

        // La pedida, sí.
        let buena = estado.recibir_salida(
            1,
            11,
            b"\x1b[31mrojo\x1b[0m\nfin\n".to_vec(),
            b"aviso\n".to_vec(),
            false,
            altos,
        );
        assert!(buena.aceptada);
        let visor = estado.visor.as_ref().unwrap();
        assert_eq!(visor.lineas[0], vec!["rojo", "fin"]);
        assert_eq!(visor.lineas[1], vec!["aviso"]);
        assert!(visor.en_vuelo.is_none());

        // Una repetida sin petición en vuelo ya no se toma.
        let repetida = estado.recibir_salida(1, 11, b"otra".to_vec(), Vec::new(), false, altos);
        assert!(!repetida.aceptada);
        assert_eq!(
            estado.visor.as_ref().unwrap().lineas[0],
            vec!["rojo", "fin"]
        );
    }

    #[test]
    fn el_guardado_arma_el_fichero_con_todos_sus_hosts() {
        let mut estado = EstadoResultados::default();
        let mut info = ejecucion(5, 1, 1);
        info.estado = EstadoEjecucion::Terminada;
        info.hosts[0].codigo = Some(0);
        info.hosts[0].duracion_ms = Some(1_200);
        info.hosts[1].estado = EstadoHostEjecucion::Error;
        info.hosts[1].error = Some("tiempo\nagotado".to_string());
        estado.actualizar(vec![info.clone()], None);
        estado.guardado = Some(GuardadoPendiente {
            ruta: PathBuf::from("/tmp/salida.txt"),
            ejecucion: info,
            host_ids: vec![10, 11],
            partes: HashMap::new(),
            desde: Instant::now(),
        });

        // No pedida: otra ejecución.
        let ajena = estado.recibir_salida(6, 10, b"x".to_vec(), Vec::new(), false, [5, 5]);
        assert!(!ajena.aceptada);
        let primera = estado.recibir_salida(5, 11, Vec::new(), b"fallo\n".to_vec(), true, [5, 5]);
        assert!(primera.aceptada && primera.completo.is_none());
        assert_eq!(estado.guardado.as_ref().unwrap().faltan(), 1);
        // Repetida: ya estaba.
        let repetida = estado.recibir_salida(5, 11, b"z".to_vec(), Vec::new(), false, [5, 5]);
        assert!(!repetida.aceptada);

        let ultima = estado.recibir_salida(
            5,
            10,
            b"\x1b[32mok\x1b[0m".to_vec(),
            Vec::new(),
            false,
            [5, 5],
        );
        let guardado = ultima.completo.expect("completo");
        assert!(estado.guardado.is_none());
        let (ruta, contenido) = guardado.armar();
        assert_eq!(ruta, PathBuf::from("/tmp/salida.txt"));
        let texto = String::from_utf8(contenido).unwrap();
        assert!(texto.starts_with(&format!(
            "reiniciar nginx · {}\n",
            fecha_larga(1_790_000_000)
        )));
        // En el orden de la ejecución, con los bytes tal cual.
        let web01 = texto
            .find("## web-01 · ok · código 0 · 1,2 s")
            .expect("web-01");
        let web02 = texto
            .find("## web-02 · error (tiempo agotado) · salida truncada a 1 MiB por flujo")
            .expect("web-02");
        assert!(web01 < web02);
        assert!(texto.contains("--- stdout ---\n\x1b[32mok\x1b[0m\n--- stderr ---\n"));
        assert!(texto.contains("--- stderr ---\nfallo\n"));
    }

    #[test]
    fn el_guardado_caduca_a_los_diez_segundos() {
        let ahora = Instant::now();
        let mut estado = EstadoResultados {
            guardado: Some(GuardadoPendiente {
                ruta: PathBuf::from("/tmp/x"),
                ejecucion: ejecucion(1, 1, 1),
                host_ids: vec![10, 11],
                partes: HashMap::new(),
                desde: ahora,
            }),
            ..Default::default()
        };
        let tick = estado.tick(ahora + Duration::from_secs(9), false);
        assert_eq!(tick.guardado_caducado, None);
        let tick = estado.tick(ahora + PLAZO_GUARDADO, false);
        assert_eq!(tick.guardado_caducado, Some(2));
        assert!(tick.repintar);
        assert!(estado.guardado.is_none());
    }

    #[test]
    fn el_visor_se_refresca_cada_segundo_mientras_el_host_corre() {
        let ahora = Instant::now();
        let mut estado = estado_con_visor(ahora);
        // Con la petición de abrir en vuelo, nada.
        assert_eq!(
            estado.tick(ahora + Duration::from_millis(200), true).pedir,
            None
        );
        estado.recibir_salida(1, 11, b"a\n".to_vec(), Vec::new(), false, [5, 5]);
        // Antes del segundo, no; al segundo, sí.
        assert_eq!(
            estado.tick(ahora + Duration::from_millis(800), true).pedir,
            None
        );
        let tick = estado.tick(ahora + REFRESCO_VISOR, true);
        assert_eq!(tick.pedir, Some((1, 11)));
        // Fuera de la vista no se pide nada.
        estado.visor.as_mut().unwrap().en_vuelo = None;
        assert_eq!(
            estado.tick(ahora + Duration::from_secs(3), false).pedir,
            None
        );

        // Una petición sin respuesta caduca y se vuelve a pedir.
        let t = ahora + Duration::from_secs(4);
        assert_eq!(estado.tick(t, true).pedir, Some((1, 11)));
        assert_eq!(estado.tick(t + Duration::from_secs(1), true).pedir, None);
        let tick = estado.tick(t + PLAZO_SALIDA, true);
        assert_eq!(tick.pedir, Some((1, 11)));

        // Termina: una respuesta atrasada (le faltan bytes) no es la
        // definitiva y se vuelve a pedir enseguida; la completa, sí.
        let mut info = ejecucion(1, 1, 1);
        info.hosts[1].estado = EstadoHostEjecucion::Ok;
        info.hosts[1].bytes_stdout = 4;
        estado.actualizar(vec![info, ejecucion(2, 2, 1)], None);
        estado.recibir_salida(1, 11, b"a\n".to_vec(), Vec::new(), false, [5, 5]);
        assert!(!estado.visor.as_ref().unwrap().completa);
        let t = t + Duration::from_secs(10);
        assert_eq!(estado.tick(t, true).pedir, Some((1, 11)));
        estado.recibir_salida(1, 11, b"a\nb\n".to_vec(), Vec::new(), false, [5, 5]);
        assert!(estado.visor.as_ref().unwrap().completa);
        assert_eq!(estado.tick(t + Duration::from_secs(5), true).pedir, None);

        // En cola no se pide; si la ejecución desaparece, tampoco.
        let mut visor = Visor::nuevo(1, 11, String::new(), String::new(), true);
        assert!(!visor.toca_pedir(ahora, Some(EstadoHostEjecucion::EnCola)));
        assert!(!visor.toca_pedir(ahora, None));
        assert!(visor.toca_pedir(ahora, Some(EstadoHostEjecucion::Conectando)));
        visor.pedir(ahora);
        assert!(!visor.toca_pedir(
            ahora + REFRESCO_VISOR,
            Some(EstadoHostEjecucion::Ejecutando)
        ));
    }

    #[test]
    fn los_desplazamientos_del_visor() {
        let lineas: Vec<u8> = (0..20)
            .map(|n| format!("línea {n}\n"))
            .collect::<String>()
            .into_bytes();
        let mut visor = Visor::nuevo(1, 1, String::new(), String::new(), false);
        visor.pedir(Instant::now());
        visor.recibir(&lineas, b"uno\ndos\n", false, true, [5, 5]);
        // Terminado: se empieza por arriba.
        assert_eq!(visor.desplazamiento, [0, 0]);

        let altos = [5, 5];
        assert!(visor.tecla(KeyCode::Down, altos));
        assert_eq!(visor.desplazamiento[0], 1);
        assert!(visor.tecla(KeyCode::PageDown, altos));
        assert_eq!(visor.desplazamiento[0], 6);
        assert!(visor.tecla(KeyCode::End, altos));
        assert_eq!(visor.desplazamiento[0], 15);
        assert!(visor.siguiendo[0]);
        assert!(visor.tecla(KeyCode::PageDown, altos));
        assert_eq!(visor.desplazamiento[0], 15);
        assert!(visor.tecla(KeyCode::Up, altos));
        assert_eq!(visor.desplazamiento[0], 14);
        assert!(!visor.siguiendo[0]);
        assert!(visor.tecla(KeyCode::PageUp, altos));
        assert!(visor.tecla(KeyCode::PageUp, altos));
        assert!(visor.tecla(KeyCode::PageUp, altos));
        assert_eq!(visor.desplazamiento[0], 0);
        assert!(visor.tecla(KeyCode::Home, altos));
        assert_eq!(visor.desplazamiento[0], 0);

        // Horizontal, acotado a la línea más larga («línea 19»: 8 caracteres).
        assert!(visor.tecla(KeyCode::Right, altos));
        assert_eq!(visor.horizontal[0], 7);
        assert!(visor.tecla(KeyCode::Left, altos));
        assert_eq!(visor.horizontal[0], 0);

        // Tab cambia de flujo; stderr cabe entero y no se mueve.
        assert!(visor.tecla(KeyCode::Tab, altos));
        assert_eq!(visor.foco, Flujo::Stderr);
        assert!(visor.tecla(KeyCode::Down, altos));
        assert_eq!(visor.desplazamiento[1], 0);
        assert_eq!(visor.desplazamiento[0], 0);
        assert!(!visor.tecla(KeyCode::Char('s'), altos));

        // Siguiendo el final, la salida nueva arrastra; sin seguir, se acota.
        visor.foco = Flujo::Stdout;
        visor.tecla(KeyCode::End, altos);
        let mas: Vec<u8> = (0..30)
            .map(|n| format!("l{n}\n"))
            .collect::<String>()
            .into_bytes();
        visor.pedir(Instant::now());
        visor.recibir(&mas, b"", false, false, altos);
        assert_eq!(visor.desplazamiento[0], 25);
        visor.tecla(KeyCode::Up, altos);
        visor.pedir(Instant::now());
        visor.recibir(b"a\nb\n", b"", false, false, altos);
        assert_eq!(visor.desplazamiento[0], 0);

        // Un host que aún corre se abre pegado al final.
        let mut vivo = Visor::nuevo(1, 1, String::new(), String::new(), true);
        vivo.pedir(Instant::now());
        vivo.recibir(&lineas, b"", false, false, altos);
        assert_eq!(vivo.desplazamiento[0], 15);
    }

    #[test]
    fn la_ruta_de_salida_se_resuelve_en_el_hogar_y_nunca_en_ssh() {
        let hogar = Path::new("/home/prueba-magi");
        assert_eq!(
            resolver_ruta_salida("~/salida.txt", hogar),
            Ok(PathBuf::from("/home/prueba-magi/salida.txt"))
        );
        assert_eq!(
            resolver_ruta_salida("  informes/./a.txt ", hogar),
            Ok(PathBuf::from("/home/prueba-magi/informes/a.txt"))
        );
        assert_eq!(
            resolver_ruta_salida("/tmp/x/../y.txt", hogar),
            Ok(PathBuf::from("/tmp/y.txt"))
        );
        assert!(resolver_ruta_salida("~/.ssh/config", hogar).is_err());
        assert!(resolver_ruta_salida("~/a/../.ssh/id_ed25519", hogar).is_err());
        assert!(resolver_ruta_salida("/", hogar).is_err());
        assert_eq!(
            abreviar_hogar(Path::new("/home/prueba-magi/a.txt"), hogar),
            "~/a.txt"
        );
        assert_eq!(abreviar_hogar(Path::new("/tmp/a.txt"), hogar), "/tmp/a.txt");
    }

    #[test]
    fn el_objetivo_del_guardado_depende_del_panel() {
        let ahora = Instant::now();
        let mut estado = EstadoResultados::default();
        estado.actualizar(vec![ejecucion(1, 1, 1)], None);
        let todos = estado.objetivo_guardado().unwrap();
        assert_eq!(todos.host_ids, vec![10, 11]);
        assert_eq!(todos.host, None);
        estado.cambiar_panel();
        estado.mover(1, (5, 5));
        let uno = estado.objetivo_guardado().unwrap();
        assert_eq!(uno.host_ids, vec![11]);
        assert_eq!(uno.host.as_deref(), Some("web-02"));
        estado.abrir_visor(ahora);
        // Aunque la selección cambie, el visor manda.
        estado.host_seleccionado = 0;
        assert_eq!(estado.objetivo_guardado().unwrap().host_ids, vec![11]);
        assert_eq!(estado.id_objetivo(), Some(1));
    }

    #[test]
    fn la_caida_del_servidor_lo_olvida_todo() {
        let ahora = Instant::now();
        let mut estado = estado_con_visor(ahora);
        estado.registrar(plan("x"), None);
        estado.guardado = Some(GuardadoPendiente {
            ruta: PathBuf::from("/tmp/x"),
            ejecucion: ejecucion(1, 1, 1),
            host_ids: vec![10],
            partes: HashMap::new(),
            desde: ahora,
        });
        estado.servidor_caido();
        assert!(estado.ejecuciones.is_empty());
        assert!(estado.visor.is_none());
        assert!(estado.guardado.is_none());
        assert!(estado.lanzamientos.is_empty());
        assert_eq!(estado.seleccionada, None);
    }
}
