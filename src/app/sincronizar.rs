//! Sincronizar directorio (`S`) y sincronizaciones guardadas (`L`) (Fase 8,
//! §4.3): el cliente recorre el árbol local, pide el remoto con `ListarArbol`,
//! aplica las exclusiones, calcula el plan y lo enseña en la vista previa; la
//! ejecución es una `Transferir` plana con `borrar_al_terminar`. Una subida a
//! un host con verificaciones o un plan que borra pasa por la deliberación.
//!
//! El plan y la vista previa viven en la `App`, no en `EstadoArchivos`: una
//! guardada se lanza desde la paleta en cualquier vista.
//!
//! Las exclusiones van en el orden de §7.2: `[archivos] excluir`, el
//! `.magiignore` de la raíz del origen y las extras. En una subida el
//! `.magiignore` es local y viaja al servidor como patrones normales; en una
//! bajada lo lee el servidor, lo devuelve en el primer `Arbol` y con él se
//! filtra también el árbol local de destino (por eso su recorrido espera a ese
//! bloque). El mismo filtro en los dos árboles: lo excluido nunca se borra.
//! El árbol remoto llega ya podado por el servidor y el cliente le pasa el
//! filtro otra vez antes del plan, para no depender solo del otro proceso.
//!
//! Todo lo que se decide queda fijado (T33): la vista previa lleva el plan
//! entero, la deliberación lo recibe tal cual y la `Transferir` sale de él.

use std::collections::HashMap;
use std::fs;
use std::io::Read as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};

use crate::archivos::exclusiones::{contar_patrones, Exclusiones};
use crate::archivos::plan::{self, Arbol, Cambio, Plan, TipoCambio};
use crate::archivos::TipoEntrada;
use crate::deliberacion::{EjecucionResultado, Verificaciones};
use crate::protocolo::{
    DeliberacionLanzada, Direccion, ElementoTransferencia, EntradaArbol, EtiquetaTransferencia,
    InfoTransferencia, MensajeCliente, Politica, SincronizacionLanzada,
};
use crate::snippets::MotivoDeliberacion;
use crate::ui::componentes::CampoTexto;
use crate::ui::disposicion::Lista;

use super::{
    AccionDialogo, App, Dialogo, Evento, EventoArchivos, PeticionArchivos, RespuestaArchivos,
};

/// Con más entradas que estas entre los dos árboles, se confirma antes de
/// calcular el plan (§7.6).
pub const MAXIMO_SIN_CONFIRMAR: usize = 50_000;

/// Tope de lectura de un `.magiignore` (el local o el temporal del remoto).
const TOPE_MAGIIGNORE: u64 = 1024 * 1024;

const MAGIIGNORE: &str = ".magiignore";

/// Diálogos propios (SINCRONIZAR, vista previa, lista de guardadas).
pub enum DialogoSincronizar {
    /// Diálogo SINCRONIZAR (§6.2).
    Formulario(Box<FormularioSincronizar>),
    /// Vista previa del plan (§6.3).
    VistaPrevia(Box<VistaPrevia>),
}

/// Acciones confirmadas (viajan en `AccionDialogo::Sincronizar`).
pub enum AccionSincronizar {
    /// Árboles de más de `MAXIMO_SIN_CONFIRMAR` entradas: calcular el plan.
    Calcular(Box<ArbolesListos>),
    /// Aviso de sensibles aceptado: deliberar o lanzar.
    Sensibles {
        plan: Box<PlanSincronizacion>,
        motivos: Vec<MotivoDeliberacion>,
    },
}

/// Peticiones al servidor en vuelo.
#[derive(Debug)]
pub enum PeticionSincronizacion {
    /// El `.magiignore` del origen remoto, para el recuento del diálogo con
    /// ese `token`.
    Magiignore { token: u64 },
    /// Un bloque del árbol remoto del plan con ese `token`.
    Arbol { token: u64 },
    /// La `Transferir` de una sincronización lanzada.
    Transferir,
}

/// Resultados de los hilos locales (recorrido del árbol).
#[derive(Debug)]
pub enum EventoSincronizacion {
    /// El árbol local del plan con ese `token` y lo que se dejó fuera, o por
    /// qué no se pudo recorrer.
    ArbolLocal {
        token: u64,
        resultado: Result<(Arbol, u32), String>,
    },
}

/// Qué sincronizar: lo fijan `S` (desde los paneles), `L` y la paleta (desde
/// una guardada). La planificación no relee nada de esto (T33).
#[derive(Debug, Clone, PartialEq)]
pub struct SolicitudSincronizacion {
    pub host_id: i64,
    pub host_nombre: String,
    /// `(id, nombre)` de la guardada; ninguna si es «ad hoc».
    pub guardada: Option<(i64, String)>,
    pub direccion: crate::protocolo::Direccion,
    pub ruta_local: String,
    pub ruta_remota: String,
    /// Borrar en el destino lo que no está en el origen.
    pub borrar: bool,
    /// Exclusiones extra (además de `[archivos] excluir` y el `.magiignore`).
    pub extras: Vec<String>,
}

impl SolicitudSincronizacion {
    /// Nombre de la guardada o «ad hoc».
    pub fn nombre(&self) -> &str {
        self.guardada
            .as_ref()
            .map(|(_, nombre)| nombre.as_str())
            .unwrap_or("ad hoc")
    }

    /// Raíces del origen y del destino.
    fn raices(&self) -> (&str, &str) {
        match self.direccion {
            Direccion::Subida => (&self.ruta_local, &self.ruta_remota),
            Direccion::Bajada => (&self.ruta_remota, &self.ruta_local),
        }
    }

    fn local(&self, relativa: &str) -> String {
        Path::new(&self.ruta_local)
            .join(relativa)
            .display()
            .to_string()
    }

    fn remota(&self, relativa: &str) -> String {
        crate::servidor::sftp::join(&self.ruta_remota, relativa)
    }

    /// Ruta absoluta en el origen de una relativa del plan.
    fn en_origen(&self, relativa: &str) -> String {
        match self.direccion {
            Direccion::Subida => self.local(relativa),
            Direccion::Bajada => self.remota(relativa),
        }
    }

    /// Ruta absoluta en el destino de una relativa del plan.
    fn en_destino(&self, relativa: &str) -> String {
        match self.direccion {
            Direccion::Subida => self.remota(relativa),
            Direccion::Bajada => self.local(relativa),
        }
    }
}

/// Campo del diálogo SINCRONIZAR con el foco (`Tab` / `Shift+Tab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocoSincronizar {
    Borrar,
    Extras,
    Guardar,
    /// El nombre de la guardada (solo con «guardar como» marcado).
    Nombre,
}

/// Lo que se sabe del `.magiignore` del origen, para el recuento del diálogo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoMagiignore {
    /// Pidiéndolo al servidor (bajada).
    Leyendo,
    Ninguno,
    Patrones(usize),
    /// Existe pero no se pudo leer (o pasa de 1 MiB): planificar fallará.
    Ilegible(String),
}

impl EstadoMagiignore {
    pub fn texto(&self) -> String {
        match self {
            EstadoMagiignore::Leyendo => ".magiignore del origen: leyendo…".to_string(),
            EstadoMagiignore::Ninguno => "sin .magiignore en el origen".to_string(),
            EstadoMagiignore::Patrones(1) => ".magiignore del origen: 1 patrón".to_string(),
            EstadoMagiignore::Patrones(cuantos) => {
                format!(".magiignore del origen: {cuantos} patrones")
            }
            EstadoMagiignore::Ilegible(motivo) => format!(".magiignore ilegible: {motivo}"),
        }
    }
}

/// Diálogo SINCRONIZAR (§6.2). Lleva fijados el host, las rutas y la
/// dirección desde que se abre (T33).
pub struct FormularioSincronizar {
    /// Identifica el diálogo ante la respuesta del `.magiignore` remoto.
    pub token: u64,
    pub host_id: i64,
    pub host_nombre: String,
    pub direccion: Direccion,
    pub ruta_local: String,
    pub ruta_remota: String,
    /// Origen y destino como se pintan (`~/…`, `host:/…`).
    pub origen: String,
    pub destino: String,
    pub borrar: bool,
    /// Patrones extra separados por espacios.
    pub extras: CampoTexto,
    pub guardar: bool,
    pub nombre: CampoTexto,
    pub foco: FocoSincronizar,
    pub magiignore: EstadoMagiignore,
    /// Error de validación (patrón, nombre repetido…), dentro del diálogo.
    pub error: Option<String>,
}

impl FormularioSincronizar {
    /// Un diálogo con los datos fijados y las opciones por defecto (borrar
    /// desmarcado, sin extras, sin guardar).
    #[allow(clippy::too_many_arguments)]
    pub fn nuevo(
        token: u64,
        host_id: i64,
        host_nombre: String,
        direccion: Direccion,
        ruta_local: String,
        ruta_remota: String,
        origen: String,
        destino: String,
        magiignore: EstadoMagiignore,
    ) -> Self {
        Self {
            token,
            host_id,
            host_nombre,
            direccion,
            ruta_local,
            ruta_remota,
            origen,
            destino,
            borrar: false,
            extras: CampoTexto::default(),
            guardar: false,
            nombre: CampoTexto::default(),
            foco: FocoSincronizar::Borrar,
            magiignore,
            error: None,
        }
    }

    /// Campo siguiente (o anterior) del recorrido con `Tab`: el nombre solo
    /// cuenta con «guardar como» marcado.
    fn mover_foco(&mut self, adelante: bool) {
        let mut campos = vec![
            FocoSincronizar::Borrar,
            FocoSincronizar::Extras,
            FocoSincronizar::Guardar,
        ];
        if self.guardar {
            campos.push(FocoSincronizar::Nombre);
        }
        let actual = campos
            .iter()
            .position(|campo| *campo == self.foco)
            .unwrap_or(0);
        let siguiente = if adelante {
            (actual + 1) % campos.len()
        } else {
            (actual + campos.len() - 1) % campos.len()
        };
        self.foco = campos[siguiente];
    }

    fn extras(&self) -> Vec<String> {
        self.extras
            .texto
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }
}

/// Filtro de la vista previa (`f`): todos → crear → actualizar → borrar →
/// omitidos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FiltroPlan {
    #[default]
    Todos,
    Solo(TipoCambio),
}

impl FiltroPlan {
    pub fn siguiente(self) -> Self {
        match self {
            FiltroPlan::Todos => FiltroPlan::Solo(TipoCambio::Crear),
            FiltroPlan::Solo(TipoCambio::Crear) => FiltroPlan::Solo(TipoCambio::Actualizar),
            FiltroPlan::Solo(TipoCambio::Actualizar) => FiltroPlan::Solo(TipoCambio::Borrar),
            FiltroPlan::Solo(TipoCambio::Borrar) => FiltroPlan::Solo(TipoCambio::Omitir),
            FiltroPlan::Solo(TipoCambio::Omitir) => FiltroPlan::Todos,
        }
    }

    pub fn admite(self, tipo: TipoCambio) -> bool {
        match self {
            FiltroPlan::Todos => true,
            FiltroPlan::Solo(solo) => solo == tipo,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            FiltroPlan::Todos => "todos",
            FiltroPlan::Solo(tipo) => tipo.texto(),
        }
    }
}

/// Un plan calculado: lo que se enseña, se delibera y se ejecuta tal cual.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanSincronizacion {
    pub solicitud: SolicitudSincronizacion,
    pub plan: Plan,
    /// Lo que las exclusiones dejaron fuera del origen.
    pub excluidos: u32,
}

impl PlanSincronizacion {
    pub fn nombre(&self) -> &str {
        self.solicitud.nombre()
    }

    pub fn host(&self) -> (i64, String) {
        (self.solicitud.host_id, self.solicitud.host_nombre.clone())
    }

    fn flecha(&self, ascii: bool) -> &'static str {
        match (self.solicitud.direccion, ascii) {
            (Direccion::Subida, false) => "→",
            (Direccion::Subida, true) => "->",
            (Direccion::Bajada, false) => "←",
            (Direccion::Bajada, true) => "<-",
        }
    }

    /// «sync web-prod → hetzner-01: 15 ficheros, 2 borrados» (la acción de
    /// la deliberación; `←` en una bajada).
    pub fn accion(&self, ascii: bool) -> String {
        let resumen = self.plan.resumen();
        format!(
            "{}: {} ficheros, {} borrados",
            self.accion_corta(ascii),
            resumen.ficheros,
            resumen.borrar
        )
    }

    /// «sync web-prod → hetzner-01».
    pub fn accion_corta(&self, ascii: bool) -> String {
        format!(
            "sync {} {} {}",
            self.nombre(),
            self.flecha(ascii),
            self.solicitud.host_nombre
        )
    }

    /// Los elementos planos de la `Transferir`, en el orden del plan (un
    /// directorio antes que su contenido): crear un directorio o un fichero
    /// con los permisos del origen, actualizar con los del destino.
    pub fn elementos(&self) -> Vec<ElementoTransferencia> {
        let solicitud = &self.solicitud;
        self.plan
            .cambios
            .iter()
            .filter_map(|cambio| {
                let (ruta, bytes, es_directorio, permisos) = match cambio {
                    Cambio::CrearDir { ruta, permisos } => (ruta, 0, true, *permisos),
                    Cambio::Crear {
                        ruta,
                        tamano,
                        permisos,
                    }
                    | Cambio::Actualizar {
                        ruta,
                        tamano,
                        permisos,
                        ..
                    } => (ruta, *tamano, false, *permisos),
                    Cambio::Borrar { .. } | Cambio::Omitir { .. } => return None,
                };
                Some(ElementoTransferencia {
                    origen: solicitud.en_origen(ruta),
                    destino: solicitud.en_destino(ruta),
                    bytes,
                    es_directorio,
                    politica: Some(Politica::Sobrescribir),
                    permisos,
                })
            })
            .collect()
    }

    /// Rutas absolutas del destino que se borran al terminar, en el orden
    /// del plan (ficheros y después directorios de más profundo a menos).
    pub fn borrar_al_terminar(&self) -> Vec<String> {
        self.plan
            .borrados()
            .map(|ruta| self.solicitud.en_destino(ruta))
            .collect()
    }

    /// Lo que el servidor necesita para anotar y cerrar la sincronización.
    pub fn lanzada(&self) -> SincronizacionLanzada {
        let resumen = self.plan.resumen();
        let (origen, destino) = self.solicitud.raices();
        SincronizacionLanzada {
            id: self.solicitud.guardada.as_ref().map(|(id, _)| *id),
            nombre: self
                .solicitud
                .guardada
                .as_ref()
                .map(|(_, nombre)| nombre.clone()),
            creados: resumen.crear as u32,
            actualizados: resumen.actualizar as u32,
            omitidos: resumen.omitidos as u32,
            raiz_origen: origen.to_string(),
            raiz_destino: destino.to_string(),
        }
    }
}

/// Vista previa del plan (§6.3): es la confirmación de la sincronización.
pub struct VistaPrevia {
    pub plan: PlanSincronizacion,
    /// Origen y destino como se pintan (`~/…`, `host:/…`).
    pub origen: String,
    pub destino: String,
    /// Por qué hay que deliberar; vacío = `↵` ejecuta. Se decide al calcular
    /// el plan y queda fijado.
    pub motivos: Vec<MotivoDeliberacion>,
    pub filtro: FiltroPlan,
    /// Fila seleccionada entre las que deja ver el filtro.
    pub seleccion: usize,
    /// Primera fila visible en el último pintado.
    pub desplazamiento: usize,
}

impl VistaPrevia {
    pub fn nueva(
        plan: PlanSincronizacion,
        origen: String,
        destino: String,
        motivos: Vec<MotivoDeliberacion>,
    ) -> Self {
        Self {
            plan,
            origen,
            destino,
            motivos,
            filtro: FiltroPlan::Todos,
            seleccion: 0,
            desplazamiento: 0,
        }
    }

    /// Índices de los cambios que deja ver el filtro.
    pub fn visibles(&self) -> Vec<usize> {
        self.plan
            .plan
            .cambios
            .iter()
            .enumerate()
            .filter(|(_, cambio)| self.filtro.admite(cambio.tipo()))
            .map(|(indice, _)| indice)
            .collect()
    }

    pub fn requiere_deliberacion(&self) -> bool {
        !self.motivos.is_empty()
    }
}

/// Los dos árboles completos, a falta de calcular el plan.
pub struct ArbolesListos {
    solicitud: SolicitudSincronizacion,
    origen: Arbol,
    destino: Arbol,
    excluidos: u32,
}

/// El árbol local de una planificación.
enum Local {
    /// Bajada: espera al `.magiignore` del primer `Arbol`.
    Pendiente,
    Recorriendo,
    Listo(Arbol, u32),
}

/// Una planificación en marcha: los dos árboles llegando.
struct Planificacion {
    token: u64,
    solicitud: SolicitudSincronizacion,
    /// Las exclusiones completas (en una bajada, desde el primer `Arbol`).
    filtro: Option<Exclusiones>,
    local: Local,
    remoto: Arbol,
    bloques: usize,
    remoto_completo: bool,
    /// Excluidos del árbol remoto (los del origen en una bajada).
    excluidos_remoto: u32,
}

/// Una sincronización enviada al servidor: se sigue en la cola por
/// `(solicitante, peticion_id)` hasta que termina.
struct Lanzada {
    nombre: String,
    guardada: Option<i64>,
    deliberacion: Option<DeliberacionLanzada>,
    /// Su fila ya salió en una difusión de la cola.
    vista: bool,
}

/// Planificaciones y sincronizaciones lanzadas por esta ventana.
#[derive(Default)]
pub struct EstadoSincronizar {
    siguiente_token: u64,
    planificando: Option<Planificacion>,
    lanzadas: HashMap<u64, Lanzada>,
}

impl EstadoSincronizar {
    fn nuevo_token(&mut self) -> u64 {
        self.siguiente_token += 1;
        self.siguiente_token
    }

    /// Hay un plan en preparación (árboles llegando).
    pub fn planificando(&self) -> bool {
        self.planificando.is_some()
    }

    /// Sincronizaciones lanzadas que aún no han terminado.
    pub fn lanzadas(&self) -> usize {
        self.lanzadas.len()
    }
}

/// «destino dentro del origen» (o al revés) si una ruta cuelga estrictamente
/// de la otra, comparando por componentes. La misma ruta se permite.
pub fn anidadas(origen: &str, destino: &str) -> Option<&'static str> {
    fn componentes(ruta: &str) -> Vec<std::path::Component<'_>> {
        Path::new(ruta).components().collect()
    }
    let (origen, destino) = (componentes(origen), componentes(destino));
    if destino.len() > origen.len() && destino.starts_with(&origen) {
        Some("destino dentro del origen")
    } else if origen.len() > destino.len() && origen.starts_with(&destino) {
        Some("origen dentro del destino")
    } else {
        None
    }
}

/// Lee un fichero de texto con el tope del `.magiignore`. `Ok(None)` si no
/// existe.
fn leer_con_tope(ruta: &Path) -> Result<Option<String>, String> {
    let fichero = match fs::File::open(ruta) {
        Ok(fichero) => fichero,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", ruta.display())),
    };
    let mut bytes = Vec::new();
    fichero
        .take(TOPE_MAGIIGNORE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", ruta.display()))?;
    if bytes.len() as u64 > TOPE_MAGIIGNORE {
        return Err(format!("{} pasa de 1 MiB", ruta.display()));
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Los patrones de un `.magiignore` tal cual, sin vacías ni comentarios
/// (para mandarlos al servidor detrás de `[archivos] excluir`).
fn patrones_de(texto: &str) -> impl Iterator<Item = String> + '_ {
    texto
        .lines()
        .filter(|linea| {
            let linea = linea.trim();
            !linea.is_empty() && !linea.starts_with('#')
        })
        .map(str::to_string)
}

/// Una ruta relativa de `Arbol` que se puede unir a la raíz sin salirse de
/// ella.
fn ruta_relativa_valida(ruta: &str) -> bool {
    !ruta.is_empty()
        && !ruta.starts_with('/')
        && ruta
            .split('/')
            .all(|trozo| !trozo.is_empty() && trozo != "." && trozo != "..")
}

/// Recorre un directorio local entero para el plan: no sigue los enlaces a
/// directorio (se marcan y el plan los omite), poda lo excluido sin entrar y
/// cuenta lo que deja fuera. Un enlace a fichero lleva el tamaño y la fecha
/// de su destino; lo que no es fichero, directorio ni enlace (sockets,
/// tuberías) no se copia nunca y se salta.
pub fn arbol_local(raiz: &Path, exclusiones: &Exclusiones) -> Result<(Arbol, u32), String> {
    let mut arbol = Arbol::new();
    let mut excluidos = 0u32;
    recorrer_en(raiz, "", exclusiones, &mut arbol, &mut excluidos)?;
    Ok((arbol, excluidos))
}

fn recorrer_en(
    directorio: &Path,
    prefijo: &str,
    exclusiones: &Exclusiones,
    arbol: &mut Arbol,
    excluidos: &mut u32,
) -> Result<(), String> {
    // Un directorio que no se puede leer para el plan: con «borrar», lo que
    // no se ve en el origen se borraría en el destino.
    let lectura =
        fs::read_dir(directorio).map_err(|error| format!("{}: {error}", directorio.display()))?;
    let mut hijos = Vec::new();
    for elemento in lectura {
        let elemento = elemento.map_err(|error| format!("{}: {error}", directorio.display()))?;
        hijos.push((
            elemento.path(),
            elemento.file_name().to_string_lossy().to_string(),
        ));
    }
    hijos.sort_by(|uno, otro| uno.1.cmp(&otro.1));
    for (ruta, nombre) in hijos {
        let relativa = if prefijo.is_empty() {
            nombre
        } else {
            format!("{prefijo}/{nombre}")
        };
        // Lo que desaparece entre el listado y el `stat` no está.
        let Ok(metadata) = fs::symlink_metadata(&ruta) else {
            continue;
        };
        let tipo = metadata.file_type();
        let entrada = if tipo.is_symlink() {
            let apuntado = fs::metadata(&ruta).ok();
            let a_dir = apuntado.as_ref().is_some_and(fs::Metadata::is_dir);
            if exclusiones.excluida(&relativa, a_dir) {
                *excluidos += 1;
                continue;
            }
            let datos = apuntado.as_ref().unwrap_or(&metadata);
            EntradaArbol {
                ruta: relativa.clone(),
                tipo: TipoEntrada::Enlace,
                tamano: if a_dir { 0 } else { datos.len() },
                mtime: if a_dir {
                    0
                } else {
                    crate::archivos::local::mtime_de(datos)
                },
                permisos: (!a_dir).then(|| datos.mode()),
                propietario: None,
                enlace_a_dir: a_dir,
            }
        } else if tipo.is_dir() {
            if exclusiones.excluida(&relativa, true) {
                *excluidos += 1;
                continue;
            }
            arbol.insert(
                relativa.clone(),
                EntradaArbol {
                    ruta: relativa.clone(),
                    tipo: TipoEntrada::Directorio,
                    tamano: 0,
                    mtime: crate::archivos::local::mtime_de(&metadata),
                    permisos: Some(metadata.mode()),
                    propietario: None,
                    enlace_a_dir: false,
                },
            );
            recorrer_en(&ruta, &relativa, exclusiones, arbol, excluidos)?;
            continue;
        } else if tipo.is_file() {
            if exclusiones.excluida(&relativa, false) {
                *excluidos += 1;
                continue;
            }
            EntradaArbol {
                ruta: relativa.clone(),
                tipo: TipoEntrada::Fichero,
                tamano: metadata.len(),
                mtime: crate::archivos::local::mtime_de(&metadata),
                permisos: Some(metadata.mode()),
                propietario: None,
                enlace_a_dir: false,
            }
        } else {
            continue;
        };
        arbol.insert(relativa, entrada);
    }
    Ok(())
}

impl App {
    /// `S` en Archivos: origen el panel activo, destino el otro; la dirección
    /// sale del lado activo (local → subida, remoto → bajada).
    pub(super) fn abrir_sincronizar(&mut self) {
        let Some(estado) = &self.archivos else {
            self.mensaje("sincronizar: abre Archivos con un host", true);
            return;
        };
        if estado.solo_local {
            self.mensaje("este host no ofrece SFTP: no hay con qué sincronizar", true);
            return;
        }
        let abriendo = estado
            .peticiones
            .values()
            .any(|peticion| matches!(peticion, crate::archivos::Peticion::AbrirSftp));
        if abriendo || estado.remoto.ruta.is_empty() || estado.local.ruta.is_empty() {
            self.mensaje(
                "los dos paneles necesitan un directorio para sincronizar",
                true,
            );
            return;
        }
        let direccion = match estado.activo {
            crate::archivos::Lado::Local => Direccion::Subida,
            crate::archivos::Lado::Remoto => Direccion::Bajada,
        };
        let (host_id, host_nombre) = (estado.host_id, estado.host_nombre.clone());
        let (ruta_local, ruta_remota) = (estado.local.ruta.clone(), estado.remoto.ruta.clone());
        let (origen, destino) = match direccion {
            Direccion::Subida => (&ruta_local, &ruta_remota),
            Direccion::Bajada => (&ruta_remota, &ruta_local),
        };
        if let Some(motivo) = anidadas(origen, destino) {
            self.mensaje(format!("no se puede sincronizar: {motivo}"), true);
            return;
        }
        let token = self.sincronizar.nuevo_token();
        let magiignore = match direccion {
            Direccion::Subida => match leer_con_tope(&Path::new(&ruta_local).join(MAGIIGNORE)) {
                Ok(Some(texto)) => EstadoMagiignore::Patrones(contar_patrones(&texto)),
                Ok(None) => EstadoMagiignore::Ninguno,
                Err(motivo) => EstadoMagiignore::Ilegible(motivo),
            },
            Direccion::Bajada => {
                // Se trae a un temporal para contarlo; se borra al leerlo.
                let peticion_id = self.nueva_peticion_archivos(PeticionArchivos::Sincronizacion(
                    PeticionSincronizacion::Magiignore { token },
                ));
                self.servidor.enviar(MensajeCliente::DescargarTemporal {
                    host_id,
                    ruta: crate::servidor::sftp::join(&ruta_remota, MAGIIGNORE),
                    peticion_id,
                    edicion: false,
                });
                EstadoMagiignore::Leyendo
            }
        };
        let (origen, destino) =
            self.extremos_visibles(&host_nombre, direccion, &ruta_local, &ruta_remota);
        self.dialogo = Some(Dialogo::Sincronizar(DialogoSincronizar::Formulario(
            Box::new(FormularioSincronizar::nuevo(
                token,
                host_id,
                host_nombre,
                direccion,
                ruta_local,
                ruta_remota,
                origen,
                destino,
                magiignore,
            )),
        )));
    }

    /// Origen y destino como se pintan: `~/…` en local y `host:/…` en remoto.
    fn extremos_visibles(
        &self,
        host_nombre: &str,
        direccion: Direccion,
        ruta_local: &str,
        ruta_remota: &str,
    ) -> (String, String) {
        let local = super::acortar_hogar(ruta_local, &self.rutas.hogar);
        let remota = format!("{host_nombre}:{ruta_remota}");
        match direccion {
            Direccion::Subida => (local, remota),
            Direccion::Bajada => (remota, local),
        }
    }

    /// Por qué no se puede planificar una solicitud (§7.6): el host o la ruta
    /// local ya no existen, o una ruta cuelga de la otra.
    fn validar_solicitud(&self, solicitud: &SolicitudSincronizacion) -> Result<(), String> {
        if !self.hosts.iter().any(|host| host.id == solicitud.host_id) {
            return Err(format!("el host «{}» ya no existe", solicitud.host_nombre));
        }
        if !Path::new(&solicitud.ruta_local).is_dir() {
            return Err(format!(
                "la ruta local «{}» ya no existe",
                solicitud.ruta_local
            ));
        }
        if solicitud.ruta_remota.trim().is_empty() {
            return Err("falta la ruta remota".to_string());
        }
        let (origen, destino) = solicitud.raices();
        if let Some(motivo) = anidadas(origen, destino) {
            return Err(motivo.to_string());
        }
        Ok(())
    }

    /// Planifica una sincronización y abre su vista previa: recorre el árbol
    /// local, pide el remoto, aplica las exclusiones y calcula el plan. Es la
    /// entrada común de `S` (tras su diálogo), `L` y la paleta.
    pub(super) fn planificar_sincronizacion(&mut self, solicitud: SolicitudSincronizacion) {
        if self.sincronizar.planificando.is_some() {
            self.mensaje(
                "ya se está planificando una sincronización: espera a su vista previa",
                true,
            );
            return;
        }
        if let Err(motivo) = self.validar_solicitud(&solicitud) {
            self.mensaje(
                format!("no se puede sincronizar «{}»: {motivo}", solicitud.nombre()),
                true,
            );
            return;
        }
        if self.avisar_si_no_hay_servidor() {
            return;
        }
        let excluir = self.config.archivos.excluir.clone();
        let token = self.sincronizar.nuevo_token();
        let fallo =
            |motivo: String| format!("no se puede sincronizar «{}»: {motivo}", solicitud.nombre());
        let (exclusiones, usar_magiignore, filtro, local) = match solicitud.direccion {
            Direccion::Subida => {
                let magiignore =
                    match leer_con_tope(&Path::new(&solicitud.ruta_local).join(MAGIIGNORE)) {
                        Ok(magiignore) => magiignore,
                        Err(motivo) => {
                            self.mensaje(fallo(motivo), true);
                            return;
                        }
                    };
                let filtro =
                    match Exclusiones::nueva(&excluir, magiignore.as_deref(), &solicitud.extras) {
                        Ok(filtro) => filtro,
                        Err(motivo) => {
                            self.mensaje(fallo(motivo), true);
                            return;
                        }
                    };
                // El remoto se filtra igual: `[archivos] excluir` y el
                // `.magiignore` local, por delante de las extras.
                let mut exclusiones = excluir;
                if let Some(texto) = &magiignore {
                    exclusiones.extend(patrones_de(texto));
                }
                if !self.recorrer_local(token, solicitud.ruta_local.clone(), filtro.clone()) {
                    return;
                }
                (exclusiones, false, Some(filtro), Local::Recorriendo)
            }
            Direccion::Bajada => {
                // Los patrones se validan ya; el `.magiignore` llega después.
                if let Err(motivo) = Exclusiones::nueva(&excluir, None, &solicitud.extras) {
                    self.mensaje(fallo(motivo), true);
                    return;
                }
                (excluir, true, None, Local::Pendiente)
            }
        };
        let peticion_id = self.nueva_peticion_archivos(PeticionArchivos::Sincronizacion(
            PeticionSincronizacion::Arbol { token },
        ));
        self.servidor.enviar(MensajeCliente::ListarArbol {
            peticion_id,
            host_id: solicitud.host_id,
            ruta: solicitud.ruta_remota.clone(),
            exclusiones,
            usar_magiignore,
            exclusiones_extra: solicitud.extras.clone(),
        });
        self.mensaje(format!("planificando «{}»…", solicitud.nombre()), false);
        self.sincronizar.planificando = Some(Planificacion {
            token,
            solicitud,
            filtro,
            local,
            remoto: Arbol::new(),
            bloques: 0,
            remoto_completo: false,
            excluidos_remoto: 0,
        });
    }

    /// Recorre el árbol local en un hilo; el resultado vuelve como
    /// `Evento::Archivos`. Devuelve `false` (y lo dice) si no se pudo lanzar.
    fn recorrer_local(&mut self, token: u64, raiz: String, exclusiones: Exclusiones) -> bool {
        let eventos = self.eventos_tx.clone();
        let lanzado = std::thread::Builder::new()
            .name("magi-arbol".to_string())
            .spawn(move || {
                let resultado = arbol_local(Path::new(&raiz), &exclusiones);
                let _ = eventos.send(Evento::Archivos(EventoArchivos::Sincronizacion(
                    EventoSincronizacion::ArbolLocal { token, resultado },
                )));
            });
        if let Err(error) = lanzado {
            self.mensaje(format!("no se pudo recorrer el árbol local: {error}"), true);
            return false;
        }
        true
    }

    pub(super) fn respuesta_sincronizacion(
        &mut self,
        peticion_id: u64,
        peticion: PeticionSincronizacion,
        respuesta: RespuestaArchivos,
    ) {
        match peticion {
            PeticionSincronizacion::Magiignore { token } => {
                self.magiignore_remoto(token, respuesta)
            }
            PeticionSincronizacion::Arbol { token } => {
                self.bloque_de_arbol(peticion_id, token, respuesta)
            }
            PeticionSincronizacion::Transferir => self.respuesta_transferir(peticion_id, respuesta),
        }
    }

    /// El `.magiignore` remoto para el recuento del diálogo: se lee el
    /// temporal, se borra y se cuenta en el diálogo de ese `token` (abierto o
    /// apartado por una pregunta del servidor). Sin fichero, «sin .magiignore».
    fn magiignore_remoto(&mut self, token: u64, respuesta: RespuestaArchivos) {
        let estado = match respuesta {
            RespuestaArchivos::RutaTemporal(ruta) => {
                let leido = leer_con_tope(Path::new(&ruta));
                self.servidor
                    .enviar(MensajeCliente::BorrarTemporal { ruta });
                match leido {
                    Ok(Some(texto)) => EstadoMagiignore::Patrones(contar_patrones(&texto)),
                    Ok(None) => EstadoMagiignore::Ninguno,
                    Err(motivo) => EstadoMagiignore::Ilegible(motivo),
                }
            }
            RespuestaArchivos::Error(_) => EstadoMagiignore::Ninguno,
            _ => return,
        };
        for dialogo in self.dialogo.iter_mut().chain(self.pila_dialogos.iter_mut()) {
            if let Dialogo::Sincronizar(DialogoSincronizar::Formulario(formulario)) = dialogo {
                if formulario.token == token {
                    formulario.magiignore = estado.clone();
                }
            }
        }
    }

    /// Un bloque de `ListarArbol`. El primero trae el `.magiignore` remoto, con
    /// el que en una bajada se recorre el destino local.
    fn bloque_de_arbol(&mut self, peticion_id: u64, token: u64, respuesta: RespuestaArchivos) {
        let Some(planificacion) = self
            .sincronizar
            .planificando
            .as_mut()
            .filter(|planificacion| planificacion.token == token)
        else {
            // De una planificación abandonada.
            return;
        };
        let (entradas, magiignore, excluidos, fin) = match respuesta {
            RespuestaArchivos::Arbol {
                entradas,
                magiignore,
                excluidos,
                fin,
            } => (entradas, magiignore, excluidos, fin),
            RespuestaArchivos::Error(motivo) => {
                let nombre = planificacion.solicitud.nombre().to_string();
                self.sincronizar.planificando = None;
                self.mensaje(
                    format!("no se puede sincronizar «{nombre}»: {motivo}"),
                    true,
                );
                return;
            }
            _ => return,
        };
        let primero = planificacion.bloques == 0;
        planificacion.bloques += 1;
        for entrada in entradas {
            if ruta_relativa_valida(&entrada.ruta) {
                planificacion.remoto.insert(entrada.ruta.clone(), entrada);
            }
        }
        if fin {
            planificacion.remoto_completo = true;
            planificacion.excluidos_remoto = excluidos;
        } else {
            // Quedan bloques: la petición sigue en vuelo con el mismo id.
            self.peticiones_archivos.insert(
                peticion_id,
                PeticionArchivos::Sincronizacion(PeticionSincronizacion::Arbol { token }),
            );
        }
        if primero && matches!(planificacion.local, Local::Pendiente) {
            let solicitud = planificacion.solicitud.clone();
            match Exclusiones::nueva(
                &self.config.archivos.excluir,
                magiignore.as_deref(),
                &solicitud.extras,
            ) {
                Ok(filtro) => {
                    planificacion.local = Local::Recorriendo;
                    planificacion.filtro = Some(filtro.clone());
                    if !self.recorrer_local(token, solicitud.ruta_local.clone(), filtro) {
                        self.sincronizar.planificando = None;
                        return;
                    }
                }
                Err(motivo) => {
                    self.sincronizar.planificando = None;
                    self.mensaje(
                        format!("no se puede sincronizar «{}»: {motivo}", solicitud.nombre()),
                        true,
                    );
                    return;
                }
            }
        }
        self.intentar_planificar();
    }

    pub(super) fn evento_sincronizacion(&mut self, evento: EventoSincronizacion) {
        match evento {
            EventoSincronizacion::ArbolLocal { token, resultado } => {
                let Some(planificacion) = self
                    .sincronizar
                    .planificando
                    .as_mut()
                    .filter(|planificacion| planificacion.token == token)
                else {
                    return;
                };
                match resultado {
                    Ok((arbol, excluidos)) => {
                        planificacion.local = Local::Listo(arbol, excluidos);
                        self.intentar_planificar();
                    }
                    Err(motivo) => {
                        let nombre = planificacion.solicitud.nombre().to_string();
                        self.sincronizar.planificando = None;
                        self.mensaje(
                            format!("no se puede sincronizar «{nombre}»: {motivo}"),
                            true,
                        );
                    }
                }
            }
        }
    }

    /// Con los dos árboles completos: confirmación si son enormes y, si no,
    /// el plan.
    fn intentar_planificar(&mut self) {
        let listos = self
            .sincronizar
            .planificando
            .as_ref()
            .is_some_and(|p| p.remoto_completo && matches!(p.local, Local::Listo(..)));
        if !listos {
            return;
        }
        let Some(planificacion) = self.sincronizar.planificando.take() else {
            return;
        };
        let (Local::Listo(local, excluidos_local), Some(filtro)) =
            (planificacion.local, planificacion.filtro)
        else {
            return;
        };
        // Lo excluido no entra en el plan aunque el servidor lo hubiera
        // dejado pasar: ni se crea, ni se actualiza, ni se borra.
        let mut remoto = planificacion.remoto;
        let antes = remoto.len();
        remoto.retain(|ruta, entrada| {
            !filtro.excluida(
                ruta,
                entrada.tipo == TipoEntrada::Directorio || entrada.enlace_a_dir,
            )
        });
        let podados = (antes - remoto.len()) as u32;
        let (origen, destino, excluidos) = match planificacion.solicitud.direccion {
            Direccion::Subida => (local, remoto, excluidos_local),
            Direccion::Bajada => (remoto, local, planificacion.excluidos_remoto + podados),
        };
        let total = origen.len() + destino.len();
        let arboles = ArbolesListos {
            solicitud: planificacion.solicitud,
            origen,
            destino,
            excluidos,
        };
        if total > MAXIMO_SIN_CONFIRMAR {
            self.mostrar_dialogo_sincronizar(Dialogo::Confirmar {
                titulo: "ÁRBOL GRANDE".to_string(),
                lineas: vec![
                    format!(
                        "El origen y el destino de «{}» suman {total} entradas.",
                        arboles.solicitud.nombre()
                    ),
                    "Calcular el plan puede tardar. ¿Planificar igualmente?".to_string(),
                ],
                peligro: false,
                accion: AccionDialogo::Sincronizar(AccionSincronizar::Calcular(Box::new(arboles))),
            });
            return;
        }
        self.calcular_plan(arboles);
    }

    /// Un diálogo que llega sin que el usuario lo pida (vista previa, árbol
    /// grande): si hay otro abierto, espera debajo y sale al cerrarlo.
    fn mostrar_dialogo_sincronizar(&mut self, dialogo: Dialogo) {
        if self.dialogo.is_some() {
            self.pila_dialogos.push(dialogo);
        } else {
            self.dialogo = Some(dialogo);
        }
    }

    /// Calcula el plan y abre la vista previa (o dice «todo al día»). Si hay
    /// que deliberar se decide aquí y queda fijado con el plan.
    fn calcular_plan(&mut self, arboles: ArbolesListos) {
        let ArbolesListos {
            solicitud,
            origen,
            destino,
            excluidos,
        } = arboles;
        let plan = PlanSincronizacion {
            plan: plan::planificar(&origen, &destino, solicitud.borrar),
            solicitud,
            excluidos,
        };
        if plan.plan.al_dia() {
            let omitidos = plan.plan.resumen().omitidos;
            let mut texto = format!("«{}»: todo al día", plan.nombre());
            if omitidos > 0 {
                texto.push_str(&format!(" ({omitidos} omitido(s))"));
            }
            self.mensaje(texto, false);
            return;
        }
        let motivos = match self.motivos_sincronizacion(&plan) {
            Ok(motivos) => motivos,
            Err(motivo) => {
                self.mensaje(motivo, true);
                return;
            }
        };
        let (origen, destino) = self.extremos_visibles(
            &plan.solicitud.host_nombre,
            plan.solicitud.direccion,
            &plan.solicitud.ruta_local,
            &plan.solicitud.ruta_remota,
        );
        let vista = VistaPrevia::nueva(plan, origen, destino, motivos);
        self.mostrar_dialogo_sincronizar(Dialogo::Sincronizar(DialogoSincronizar::VistaPrevia(
            Box::new(vista),
        )));
    }

    /// Motivos para deliberar (§4.3): una subida a un host con alguna
    /// verificación activa o «borrar» marcado.
    fn motivos_sincronizacion(
        &self,
        plan: &PlanSincronizacion,
    ) -> Result<Vec<MotivoDeliberacion>, String> {
        let mut motivos = Vec::new();
        if plan.solicitud.direccion == Direccion::Subida {
            // Sin saber si el host pide verificaciones no se puede decidir.
            let verificaciones = self.almacen.verificaciones_por_host().map_err(|error| {
                format!("no se pudieron leer las verificaciones previas: {error}")
            })?;
            if verificaciones
                .get(&plan.solicitud.host_id)
                .is_some_and(Verificaciones::alguna_activa)
            {
                motivos.push(MotivoDeliberacion::Verificaciones(vec![plan
                    .solicitud
                    .host_nombre
                    .clone()]));
            }
        }
        if plan.solicitud.borrar {
            motivos.push(MotivoDeliberacion::Borrar(plan.plan.resumen().borrar));
        }
        Ok(motivos)
    }

    fn tecla_vista_previa(&mut self, mut vista: Box<VistaPrevia>, tecla: KeyEvent) {
        // Las teclas parten de lo que se ve: el pintado pudo mover la ventana.
        if let Some(ventana) = self.disposicion.lista(Lista::VistaPrevia) {
            vista.desplazamiento = ventana.inicio;
        }
        let pagina = self.disposicion.filas(Lista::VistaPrevia) as i64;
        let total = vista.visibles().len() as i64;
        let mover = |vista: &mut VistaPrevia, paso: i64| {
            vista.seleccion = (vista.seleccion as i64 + paso).clamp(0, (total - 1).max(0)) as usize;
        };
        match tecla.code {
            KeyCode::Esc => {
                self.mensaje("sincronización cancelada: no se ha hecho nada", false);
                return;
            }
            KeyCode::Enter => {
                self.continuar_sincronizacion(*vista);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') => mover(&mut vista, -1),
            KeyCode::Down | KeyCode::Char('j') => mover(&mut vista, 1),
            KeyCode::PageUp => mover(&mut vista, -pagina),
            KeyCode::PageDown => mover(&mut vista, pagina),
            KeyCode::Home => vista.seleccion = 0,
            KeyCode::End => mover(&mut vista, total),
            KeyCode::Char('f') => {
                vista.filtro = vista.filtro.siguiente();
                vista.seleccion = 0;
                vista.desplazamiento = 0;
            }
            _ => {}
        }
        self.dialogo = Some(Dialogo::Sincronizar(DialogoSincronizar::VistaPrevia(vista)));
    }

    /// `↵` en la vista previa: aviso de sensibles en las subidas y, después,
    /// deliberación o ejecución.
    fn continuar_sincronizacion(&mut self, vista: VistaPrevia) {
        let VistaPrevia {
            plan,
            motivos,
            destino,
            ..
        } = vista;
        if plan.solicitud.direccion == Direccion::Subida {
            let sensibles =
                crate::archivos::sensibles::Sensibles::nuevo(&self.config.archivos.avisar);
            // El aviso mira el nombre de cada fichero que se crea o actualiza
            // (como el de la F4); se enseña con su ruta relativa.
            let ficheros: Vec<(&str, &str, u64)> = plan
                .plan
                .cambios
                .iter()
                .filter_map(|cambio| match cambio {
                    Cambio::Crear { ruta, tamano, .. }
                    | Cambio::Actualizar { ruta, tamano, .. } => {
                        let nombre = ruta.rsplit('/').next().unwrap_or(ruta);
                        Some((ruta.as_str(), nombre, *tamano))
                    }
                    _ => None,
                })
                .collect();
            let mut coincidencias = Vec::new();
            for (ruta, nombre, bytes) in &ficheros {
                if sensibles.coincide(nombre).is_some() {
                    coincidencias.push((*ruta, *bytes));
                }
            }
            if !coincidencias.is_empty() {
                let mostradas = crate::archivos::sensibles::MAXIMO_EN_EL_AVISO;
                let mut lineas = vec![
                    "Vas a subir ficheros que coinciden con los patrones de aviso:".to_string(),
                ];
                for (ruta, bytes) in coincidencias.iter().take(mostradas) {
                    lineas.push(format!(
                        "  {ruta}  ({})",
                        crate::archivos::tamano_legible(*bytes)
                    ));
                }
                if coincidencias.len() > mostradas {
                    lineas.push(format!("  y {} más", coincidencias.len() - mostradas));
                }
                lineas.push(format!("Destino: {destino}"));
                lineas.push("Suelen contener secretos. ¿Subirlos igualmente?".to_string());
                self.dialogo = Some(Dialogo::Confirmar {
                    titulo: "AVISO".to_string(),
                    lineas,
                    peligro: true,
                    accion: AccionDialogo::Sincronizar(AccionSincronizar::Sensibles {
                        plan: Box::new(plan),
                        motivos,
                    }),
                });
                return;
            }
        }
        self.deliberar_o_lanzar(plan, motivos);
    }

    fn deliberar_o_lanzar(&mut self, plan: PlanSincronizacion, motivos: Vec<MotivoDeliberacion>) {
        if motivos.is_empty() {
            self.lanzar_sincronizacion(plan, None);
        } else {
            self.abrir_deliberacion(plan, motivos);
        }
    }

    /// Envía la sincronización al servidor como una `Transferir` plana, con la
    /// deliberación que la autoriza si la hubo.
    pub(super) fn lanzar_sincronizacion(
        &mut self,
        plan: PlanSincronizacion,
        deliberacion: Option<DeliberacionLanzada>,
    ) {
        if self.avisar_si_no_hay_servidor() {
            // La deliberación ya está en la base: nadie más la cerraría.
            if let Some(deliberacion) = &deliberacion {
                self.cerrar_deliberacion(deliberacion, EjecucionResultado::Error);
            }
            return;
        }
        let peticion_id = self.nueva_peticion_archivos(PeticionArchivos::Sincronizacion(
            PeticionSincronizacion::Transferir,
        ));
        self.sincronizar.lanzadas.insert(
            peticion_id,
            Lanzada {
                nombre: plan.nombre().to_string(),
                guardada: plan.solicitud.guardada.as_ref().map(|(id, _)| *id),
                deliberacion: deliberacion.clone(),
                vista: false,
            },
        );
        self.servidor.enviar(MensajeCliente::Transferir {
            host_id: plan.solicitud.host_id,
            direccion: plan.solicitud.direccion,
            elementos: plan.elementos(),
            politica: Politica::Sobrescribir,
            borrar_origen: false,
            peticion_id: Some(peticion_id),
            borrar_al_terminar: plan.borrar_al_terminar(),
            deliberacion,
            sincronizacion: Some(plan.lanzada()),
            etiqueta: Some(EtiquetaTransferencia::Sincronizacion),
        });
        self.mensaje(
            format!("lanzando la sincronización «{}»…", plan.nombre()),
            false,
        );
    }

    /// El servidor encoló la sincronización (`Hecho`) o la rechazó (`Error`).
    fn respuesta_transferir(&mut self, peticion_id: u64, respuesta: RespuestaArchivos) {
        match respuesta {
            RespuestaArchivos::Hecho(_) => {
                let Some(lanzada) = self.sincronizar.lanzadas.get(&peticion_id) else {
                    return;
                };
                let (nombre, guardada) = (lanzada.nombre.clone(), lanzada.guardada);
                // La guardada se ha lanzado de verdad: su última ejecución.
                if let Some(id) = guardada {
                    if let Err(error) = self.almacen.marcar_ejecucion_sincronizacion(id) {
                        tracing::warn!(
                            "no se pudo marcar la ejecución de la sincronización: {error}"
                        );
                    }
                }
                self.mensaje(
                    format!("sincronización «{nombre}» encolada · t cola"),
                    false,
                );
            }
            RespuestaArchivos::Error(motivo) => {
                let Some(lanzada) = self.sincronizar.lanzadas.remove(&peticion_id) else {
                    return;
                };
                self.mensaje(
                    format!("sincronización «{}» rechazada: {motivo}", lanzada.nombre),
                    true,
                );
                if let Some(deliberacion) = &lanzada.deliberacion {
                    self.cerrar_deliberacion(deliberacion, EjecucionResultado::Error);
                }
            }
            _ => {}
        }
    }

    /// La cola cambió: las sincronizaciones lanzadas que terminan.
    pub(super) fn cola_de_sincronizaciones(&mut self, lista: &[InfoTransferencia]) {
        if self.sincronizar.lanzadas.is_empty() {
            return;
        }
        let cliente = self.cliente_id;
        let mut terminadas = Vec::new();
        self.sincronizar.lanzadas.retain(|peticion_id, lanzada| {
            let fila = lista.iter().find(|fila| {
                fila.peticion_id == Some(*peticion_id) && Some(fila.solicitante) == cliente
            });
            match fila {
                Some(fila) if fila.estado.terminada() => {
                    terminadas.push((lanzada.nombre.clone(), fila.clone()));
                    false
                }
                Some(_) => {
                    lanzada.vista = true;
                    true
                }
                // Su fila se limpió de la cola sin verla terminar.
                None => !lanzada.vista,
            }
        });
        for (nombre, fila) in terminadas {
            self.sincronizacion_terminada(&nombre, &fila);
        }
    }

    /// Mensaje con el resultado y refresco de los paneles si Archivos está en
    /// ese host.
    fn sincronizacion_terminada(&mut self, nombre: &str, fila: &InfoTransferencia) {
        let nada_borrado = if fila.borrados_total > 0 && fila.borrados == 0 {
            " · no se ha borrado nada"
        } else {
            ""
        };
        match fila.estado {
            crate::protocolo::EstadoTransferencia::Hecha => {
                let mut texto = format!("sincronización «{nombre}» hecha");
                if fila.borrados_total > 0 {
                    texto.push_str(&format!(
                        " · {} de {} borrados",
                        fila.borrados, fila.borrados_total
                    ));
                }
                self.mensaje(texto, false);
            }
            crate::protocolo::EstadoTransferencia::Cancelada => {
                self.mensaje(
                    format!("sincronización «{nombre}» cancelada{nada_borrado}"),
                    true,
                );
            }
            _ => self.mensaje(
                format!(
                    "sincronización «{nombre}» con errores: {}{nada_borrado}",
                    fila.error.as_deref().unwrap_or("falló")
                ),
                true,
            ),
        }
        if self
            .archivos
            .as_ref()
            .is_some_and(|estado| estado.host_id == fila.host_id)
        {
            // Una sola vez: la cola no la vuelve a refrescar por su cuenta.
            self.transferencias_refrescadas.insert(fila.id);
            self.refrescar_archivos(crate::archivos::panel::MotivoListado::Operacion);
        }
    }

    /// El servidor cayó: los planes a medias se abandonan. Las lanzadas se
    /// pierden con la cola (F3) y no se borra nada (R40).
    pub(super) fn sincronizaciones_servidor_caido(&mut self) {
        self.sincronizar.planificando = None;
        self.sincronizar.lanzadas.clear();
    }

    pub(super) fn tecla_dialogo_sincronizar(
        &mut self,
        dialogo: DialogoSincronizar,
        tecla: KeyEvent,
    ) {
        match dialogo {
            DialogoSincronizar::Formulario(formulario) => {
                self.tecla_formulario_sincronizar(formulario, tecla)
            }
            DialogoSincronizar::VistaPrevia(vista) => self.tecla_vista_previa(vista, tecla),
        }
    }

    fn tecla_formulario_sincronizar(
        &mut self,
        mut formulario: Box<FormularioSincronizar>,
        tecla: KeyEvent,
    ) {
        match tecla.code {
            KeyCode::Esc => return,
            KeyCode::Enter => {
                self.enviar_formulario_sincronizar(formulario);
                return;
            }
            KeyCode::Tab => formulario.mover_foco(true),
            KeyCode::BackTab => formulario.mover_foco(false),
            KeyCode::Char(' ') if formulario.foco == FocoSincronizar::Borrar => {
                formulario.borrar = !formulario.borrar;
            }
            KeyCode::Char(' ') if formulario.foco == FocoSincronizar::Guardar => {
                formulario.guardar = !formulario.guardar;
                formulario.error = None;
            }
            _ => {
                let campo = match formulario.foco {
                    FocoSincronizar::Extras => Some(&mut formulario.extras),
                    FocoSincronizar::Nombre => Some(&mut formulario.nombre),
                    _ => None,
                };
                if let Some(campo) = campo {
                    if campo.manejar_tecla(&tecla) {
                        formulario.error = None;
                    }
                }
            }
        }
        self.dialogo = Some(Dialogo::Sincronizar(DialogoSincronizar::Formulario(
            formulario,
        )));
    }

    /// `↵` en SINCRONIZAR: valida los patrones, guarda si se pidió (un error
    /// se queda en el diálogo) y planifica.
    fn enviar_formulario_sincronizar(&mut self, mut formulario: Box<FormularioSincronizar>) {
        // Antes de guardar nada: con otro plan en preparación no se planifica.
        if self.sincronizar.planificando() {
            formulario.error = Some(
                "ya se está planificando otra sincronización: espera a su vista previa".to_string(),
            );
            self.dialogo = Some(Dialogo::Sincronizar(DialogoSincronizar::Formulario(
                formulario,
            )));
            return;
        }
        let extras = formulario.extras();
        if let Err(motivo) = Exclusiones::nueva(&self.config.archivos.excluir, None, &extras) {
            formulario.error = Some(motivo);
            formulario.foco = FocoSincronizar::Extras;
            self.dialogo = Some(Dialogo::Sincronizar(DialogoSincronizar::Formulario(
                formulario,
            )));
            return;
        }
        let mut guardada = None;
        if formulario.guardar {
            let datos = crate::modelo::DatosSincronizacion {
                host_id: formulario.host_id,
                nombre: formulario.nombre.texto.trim().to_string(),
                ruta_local: formulario.ruta_local.clone(),
                ruta_remota: formulario.ruta_remota.clone(),
                direccion: formulario.direccion,
                borrar: formulario.borrar,
                exclusiones: extras.clone(),
            };
            match self.almacen.crear_sincronizacion(&datos) {
                Ok(id) => guardada = Some((id, datos.nombre)),
                Err(error) => {
                    formulario.error = Some(error.to_string());
                    formulario.foco = FocoSincronizar::Nombre;
                    self.dialogo = Some(Dialogo::Sincronizar(DialogoSincronizar::Formulario(
                        formulario,
                    )));
                    return;
                }
            }
        }
        self.planificar_sincronizacion(SolicitudSincronizacion {
            host_id: formulario.host_id,
            host_nombre: formulario.host_nombre,
            guardada,
            direccion: formulario.direccion,
            ruta_local: formulario.ruta_local,
            ruta_remota: formulario.ruta_remota,
            borrar: formulario.borrar,
            extras,
        });
    }

    pub(super) fn ejecutar_accion_sincronizar(&mut self, accion: AccionSincronizar) {
        match accion {
            AccionSincronizar::Calcular(arboles) => self.calcular_plan(*arboles),
            AccionSincronizar::Sensibles { plan, motivos } => {
                self.deliberar_o_lanzar(*plan, motivos)
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn por_defecto() -> Vec<String> {
        crate::config::EXCLUIR_POR_DEFECTO
            .iter()
            .map(|patron| patron.to_string())
            .collect()
    }

    #[test]
    fn anidadas_por_componentes_y_la_igualdad_vale() {
        assert_eq!(
            anidadas("/var/www", "/var/www/app"),
            Some("destino dentro del origen")
        );
        assert_eq!(
            anidadas("/var/www/app/", "/var/www"),
            Some("origen dentro del destino")
        );
        assert_eq!(anidadas("/var/www", "/var/www/"), None, "la misma ruta");
        assert_eq!(
            anidadas("/var/www", "/var/www-viejo"),
            None,
            "por componentes"
        );
        assert_eq!(anidadas("/home/hector/web", "/srv/web"), None);
    }

    #[test]
    fn el_recorrido_local_poda_lo_excluido_y_marca_los_enlaces() {
        let temporal = tempfile::tempdir().unwrap();
        let raiz = temporal.path();
        fs::create_dir_all(raiz.join("src")).unwrap();
        fs::create_dir_all(raiz.join("node_modules/react")).unwrap();
        fs::create_dir_all(raiz.join("compartido")).unwrap();
        fs::write(raiz.join("src/main.rs"), b"fn main() {}").unwrap();
        fs::write(raiz.join("node_modules/react/index.js"), b"x").unwrap();
        fs::write(raiz.join("error.log"), b"x").unwrap();
        fs::write(raiz.join(".magiignore"), b"*.log\n").unwrap();
        std::os::unix::fs::symlink(raiz.join("compartido"), raiz.join("atajo")).unwrap();
        std::os::unix::fs::symlink(raiz.join("src/main.rs"), raiz.join("principal.rs")).unwrap();

        let exclusiones = Exclusiones::nueva(&por_defecto(), Some("*.log\n"), &[]).unwrap();
        let (arbol, excluidos) = arbol_local(raiz, &exclusiones).unwrap();
        let rutas: Vec<&str> = arbol.keys().map(String::as_str).collect();
        assert_eq!(
            rutas,
            vec!["atajo", "compartido", "principal.rs", "src", "src/main.rs"]
        );
        // node_modules/ (sin entrar), error.log y el propio .magiignore.
        assert_eq!(excluidos, 3);
        assert!(arbol["atajo"].enlace_a_dir);
        let enlace = &arbol["principal.rs"];
        assert_eq!(
            (enlace.tipo, enlace.tamano, enlace.enlace_a_dir),
            (TipoEntrada::Enlace, 12, false),
            "un enlace a fichero lleva el tamaño de su destino"
        );
        assert_eq!(arbol["src"].tipo, TipoEntrada::Directorio);
    }

    #[test]
    fn un_directorio_ilegible_hace_fallar_el_recorrido() {
        let temporal = tempfile::tempdir().unwrap();
        let resultado = arbol_local(&temporal.path().join("no-existe"), &Exclusiones::ninguna());
        assert!(resultado.is_err());
    }

    #[test]
    fn el_magiignore_se_lee_con_tope_y_sin_fichero_es_ninguno() {
        let temporal = tempfile::tempdir().unwrap();
        let ruta = temporal.path().join(".magiignore");
        assert_eq!(leer_con_tope(&ruta), Ok(None));
        fs::write(&ruta, "# comentario\n*.log\n\ntmp/\n").unwrap();
        let texto = leer_con_tope(&ruta).unwrap().unwrap();
        assert_eq!(contar_patrones(&texto), 2);
        assert_eq!(
            patrones_de(&texto).collect::<Vec<_>>(),
            vec!["*.log".to_string(), "tmp/".to_string()]
        );
        fs::write(&ruta, vec![b'x'; (TOPE_MAGIIGNORE + 1) as usize]).unwrap();
        assert!(leer_con_tope(&ruta).is_err());
    }

    #[test]
    fn solo_valen_rutas_relativas_sin_puntos() {
        assert!(ruta_relativa_valida("static/app.js"));
        for mala in ["", "/etc/passwd", "../fuera", "a/../b", "a//b", "./a"] {
            assert!(!ruta_relativa_valida(mala), "{mala}");
        }
    }

    #[test]
    fn el_filtro_recorre_los_cuatro_tipos_y_vuelve() {
        let mut filtro = FiltroPlan::Todos;
        let mut vistos = Vec::new();
        for _ in 0..5 {
            filtro = filtro.siguiente();
            vistos.push(filtro.texto());
        }
        assert_eq!(
            vistos,
            vec!["crear", "actualizar", "borrar", "omitidos", "todos"]
        );
        assert!(FiltroPlan::Todos.admite(TipoCambio::Borrar));
        assert!(!FiltroPlan::Solo(TipoCambio::Crear).admite(TipoCambio::Borrar));
    }

    fn solicitud(direccion: Direccion, guardada: bool) -> SolicitudSincronizacion {
        SolicitudSincronizacion {
            host_id: 3,
            host_nombre: "hetzner-01".to_string(),
            guardada: guardada.then(|| (7, "web-prod".to_string())),
            direccion,
            ruta_local: "/home/hector/web".to_string(),
            ruta_remota: "/var/www/app".to_string(),
            borrar: true,
            extras: Vec::new(),
        }
    }

    fn plan_de_ejemplo() -> Plan {
        Plan {
            cambios: vec![
                Cambio::CrearDir {
                    ruta: "static".to_string(),
                    permisos: Some(0o755),
                },
                Cambio::Crear {
                    ruta: "static/app.js".to_string(),
                    tamano: 88,
                    permisos: Some(0o644),
                },
                Cambio::Actualizar {
                    ruta: "main.py".to_string(),
                    tamano: 14,
                    mtime: 10,
                    permisos: Some(0o2640),
                    tamano_destino: 12,
                    mtime_destino: 0,
                },
                Cambio::Omitir {
                    ruta: "enlace".to_string(),
                    motivo: plan::MotivoOmision::EnlaceADirectorio,
                },
                Cambio::Borrar {
                    ruta: "viejo.css".to_string(),
                    es_dir: false,
                    tamano: 3,
                },
                Cambio::Borrar {
                    ruta: "tmp".to_string(),
                    es_dir: true,
                    tamano: 0,
                },
            ],
        }
    }

    #[test]
    fn la_transferencia_sale_plana_con_permisos_y_rutas_absolutas() {
        let plan = PlanSincronizacion {
            solicitud: solicitud(Direccion::Subida, true),
            plan: plan_de_ejemplo(),
            excluidos: 4,
        };
        let elementos = plan.elementos();
        let resumen: Vec<(&str, &str, bool, Option<u32>)> = elementos
            .iter()
            .map(|elemento| {
                (
                    elemento.origen.as_str(),
                    elemento.destino.as_str(),
                    elemento.es_directorio,
                    elemento.permisos,
                )
            })
            .collect();
        assert_eq!(
            resumen,
            vec![
                (
                    "/home/hector/web/static",
                    "/var/www/app/static",
                    true,
                    Some(0o755)
                ),
                (
                    "/home/hector/web/static/app.js",
                    "/var/www/app/static/app.js",
                    false,
                    Some(0o644)
                ),
                (
                    "/home/hector/web/main.py",
                    "/var/www/app/main.py",
                    false,
                    Some(0o2640)
                ),
            ]
        );
        assert!(elementos
            .iter()
            .all(|elemento| elemento.politica == Some(Politica::Sobrescribir)));
        assert_eq!(
            plan.borrar_al_terminar(),
            vec!["/var/www/app/viejo.css", "/var/www/app/tmp"]
        );
        let lanzada = plan.lanzada();
        assert_eq!(lanzada.id, Some(7));
        assert_eq!(lanzada.nombre.as_deref(), Some("web-prod"));
        assert_eq!(
            (lanzada.creados, lanzada.actualizados, lanzada.omitidos),
            (2, 1, 1)
        );
        assert_eq!(
            (lanzada.raiz_origen.as_str(), lanzada.raiz_destino.as_str()),
            ("/home/hector/web", "/var/www/app")
        );
        assert_eq!(
            plan.accion(false),
            "sync web-prod → hetzner-01: 2 ficheros, 2 borrados"
        );
    }

    #[test]
    fn en_una_bajada_el_origen_es_remoto_y_se_borra_en_local() {
        let plan = PlanSincronizacion {
            solicitud: solicitud(Direccion::Bajada, false),
            plan: plan_de_ejemplo(),
            excluidos: 0,
        };
        let elementos = plan.elementos();
        assert_eq!(elementos[1].origen, "/var/www/app/static/app.js");
        assert_eq!(elementos[1].destino, "/home/hector/web/static/app.js");
        assert_eq!(
            plan.borrar_al_terminar(),
            vec!["/home/hector/web/viejo.css", "/home/hector/web/tmp"]
        );
        let lanzada = plan.lanzada();
        assert_eq!((lanzada.id, lanzada.nombre), (None, None));
        assert_eq!(lanzada.raiz_origen, "/var/www/app");
        assert_eq!(
            plan.accion(true),
            "sync ad hoc <- hetzner-01: 2 ficheros, 2 borrados"
        );
    }
}
