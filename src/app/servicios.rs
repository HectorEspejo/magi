//! Estado del cliente de la vista Servicios (F9, Fase 13).
//!
//! Aquí vive todo lo que el cliente guarda de la vista y las peticiones que
//! manda al servidor: los listados por host con su hora, las selecciones y los
//! filtros, el detalle, el panel de logs con su anillo, la contraseña de sudo
//! por host (en Zeroizing, nunca a disco), las acciones en curso y «Toda la
//! flota». El pintado está en ui/servicios.rs; aquí no se pinta nada.
//!
//! Reglas (CLAUDE.md y §4/§7 del informe): los peticion_id de servicios
//! empiezan en 2^45 y el contador vive en la App, no en el panel; una
//! respuesta de una petición que ya no está en vuelo se descarta (T33); los
//! refrescos no se solapan ni abren diálogos; al salir de la vista se para el
//! log y se olvida la contraseña de sudo.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zeroize::Zeroizing;

use magi_nucleo::servicios::lectura::ComprobacionSudo;
use magi_nucleo::servicios::{
    acciones, validadores, Accion, Consumo, Detalle, Fallo, FallosSondeo, InfoAccion, LineaLog,
    Listado, MotivoFinLog, MotorNombre, NivelLog, Objetivo, OrdenPestana, Privilegio,
    ResultadoAccion, ResultadoServicio, TipoObjetivo,
};

use crate::modelo::Host;
use crate::protocolo::{DeliberacionLanzada, MensajeCliente, MensajeServidor, Secreto};
use crate::ui::componentes::CampoTexto;
use crate::ui::disposicion::{Lista, Minimo, Tamano};

use super::{App, Vista};

/// Presupuesto de bytes del anillo del panel de logs (M3): con líneas de
/// 16 KiB, el tope de 10 000 líneas serían 160 MiB; el anillo se queda en
/// 8 MiB y cuenta por el frente lo que descarta.
pub const PRESUPUESTO_LOG_BYTES: usize = 8 * 1024 * 1024;

/// Sección visible de la lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Seccion {
    Servicios,
    Timers,
    Contenedores,
}

impl Seccion {
    pub fn texto(self) -> &'static str {
        match self {
            Seccion::Servicios => "Servicios",
            Seccion::Timers => "Timers",
            Seccion::Contenedores => "Contenedores",
        }
    }

    pub fn de_numero(caracter: char) -> Option<Self> {
        match caracter {
            '1' => Some(Seccion::Servicios),
            '2' => Some(Seccion::Timers),
            '3' => Some(Seccion::Contenedores),
            _ => None,
        }
    }
}

/// Panel con el foco: la columna de hosts o la lista.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foco {
    Hosts,
    Lista,
}

/// Estado de la conexión con el host de la vista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoListado {
    /// Petición en vuelo (se conserva el listado anterior, si lo hay).
    Cargando,
    /// Último listado recibido.
    Ok,
    /// El host no respondió; se conserva el anterior con su hora.
    NoResponde(String),
    /// La conexión se cayó: «desconectado · r reconectar».
    Desconectado,
}

/// Último listado recibido de un host, con su hora local.
#[derive(Debug, Clone)]
pub struct ListadoHost {
    pub listado: Listado,
    /// Hora local (época) en la que se recibió.
    pub recibido_epoch: i64,
    pub recibido: Instant,
    pub estado: EstadoListado,
    /// Mensaje de la última desconexión (si la hubo), para enseñarlo junto a
    /// «desconectado · r reconectar».
    pub aviso: Option<String>,
}

impl ListadoHost {
    /// Segundos desde que se recibió (para «hace N s» de la cabecera).
    pub fn antiguedad_seg(&self) -> f64 {
        self.recibido.elapsed().as_secs_f64()
    }
}

/// Consumo de los contenedores de un host (stats --no-stream).
#[derive(Debug, Clone, Default)]
pub struct ConsumoHost {
    pub consumos: Vec<Consumo>,
    pub recibido_epoch: i64,
}

/// Contraseña de sudo en memoria para un host (nunca sale de aquí).
pub struct MemoriaSudo {
    pub contrasena: Zeroizing<String>,
    pub ultimo_uso: Instant,
}

/// Petición de servicios en vuelo, para descartar respuestas viejas (T33).
#[derive(Debug, Clone)]
pub enum PeticionServicio {
    Listar {
        host_id: i64,
    },
    Consumo {
        host_id: i64,
    },
    Detalle {
        host_id: i64,
        objetivo: Objetivo,
    },
    Sudo {
        host_id: i64,
        /// Contraseña a comprobar, en Zeroizing hasta que llegue la respuesta.
        contrasena: Zeroizing<String>,
    },
    Accion {
        host_id: i64,
        accion: AccionPendiente,
    },
    Log {
        host_id: i64,
        objetivo: Objetivo,
    },
}

/// Acción que el usuario pidió y que espera a la contraseña de sudo o a la
/// confirmación/deliberación. Lleva fijados los datos (T33).
#[derive(Debug, Clone, PartialEq)]
pub struct AccionPendiente {
    pub host_id: i64,
    pub host_nombre: String,
    pub objetivo: Objetivo,
    pub accion: Accion,
}

impl AccionPendiente {
    pub fn texto_accion(&self) -> String {
        format!("{} {}", self.accion.texto(), self.objetivo.nombre)
    }
}

/// Confirmación simple de una acción (T6).
#[derive(Debug, Clone)]
pub struct ConfirmacionServicio {
    pub pendiente: AccionPendiente,
    /// «Antes: nginx -t», si hay comprobación de configuración.
    pub comprobacion: Option<String>,
    /// Aviso de unidad crítica, si lo es.
    pub critica: Option<String>,
}

/// Diálogo de contraseña de sudo (3 intentos, enmascarada).
#[derive(Debug, Clone)]
pub struct SudoPedido {
    pub host_id: i64,
    pub host_nombre: String,
    pub usuario: Option<String>,
    pub campo: CampoTexto,
    pub intento: u8,
    pub error: Option<String>,
    /// Acción que se ejecutará al comprobar la contraseña; None para !.
    pub pendiente: Option<AccionPendiente>,
}

/// Editor de una comprobación de configuración propia.
#[derive(Debug, Clone)]
pub struct EditorComprobacion {
    pub host_id: i64,
    pub host_nombre: String,
    /// Objetivo ya fijado (desde c en la vista); None si se crea desde la
    /// ficha y hay que escribirlo.
    pub objetivo: Option<Objetivo>,
    /// Primer campo: tipo y nombre del objetivo (solo si objetivo es None).
    pub objetivo_campo: CampoTexto,
    pub comando: CampoTexto,
    /// Campo con el foco: 0 = objetivo, 1 = comando.
    pub foco: usize,
    /// La comprobación existente, si se está editando.
    pub existente: Option<i64>,
    pub error: Option<String>,
}

/// Diálogos propios de la vista Servicios (no van en App.dialogo: una
/// pregunta del servidor se apila encima sin pisarlos).
#[derive(Debug, Clone)]
pub enum DialogoServicio {
    Confirmar(ConfirmacionServicio),
    Resultado(ResultadoDialogo),
    Sudo(SudoPedido),
    Comprobacion(EditorComprobacion),
    BorrarComprobacion {
        host_id: i64,
        host_nombre: String,
        objetivo: Objetivo,
        id: i64,
    },
}

impl DialogoServicio {
    /// Mínimo de la pantalla que exige el diálogo (T45).
    pub fn minimo(&self) -> Minimo {
        Minimo {
            tamano: Tamano::new(40, 12),
            exige: match self {
                DialogoServicio::Confirmar(_) => "confirmación de servicio",
                DialogoServicio::Resultado(_) => "resultado de servicio",
                DialogoServicio::Sudo(_) => "contraseña de sudo",
                DialogoServicio::Comprobacion(_) => "comprobación de servicio",
                DialogoServicio::BorrarComprobacion { .. } => "borrar comprobación",
            },
        }
    }
}

/// Resultado de una acción, con lo que hay que enseñar (§6.5).
#[derive(Debug, Clone)]
pub struct ResultadoDialogo {
    pub host_id: i64,
    pub host_nombre: String,
    pub pendiente: AccionPendiente,
    pub resultado: ResultadoServicio,
}

/// Pestaña del detalle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PestanaDetalle {
    Estado,
    Dependencias,
    Unidad,
}

impl PestanaDetalle {
    pub fn siguiente(self) -> Self {
        match self {
            PestanaDetalle::Estado => PestanaDetalle::Dependencias,
            PestanaDetalle::Dependencias => PestanaDetalle::Unidad,
            PestanaDetalle::Unidad => PestanaDetalle::Estado,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            PestanaDetalle::Estado => "Estado",
            PestanaDetalle::Dependencias => "Dependencias",
            PestanaDetalle::Unidad => "Unidad",
        }
    }
}

/// Detalle abierto de un objetivo.
#[derive(Debug, Clone)]
pub struct DetalleAbierto {
    pub host_id: i64,
    pub host_nombre: String,
    pub objetivo: Objetivo,
    pub pestaña: PestanaDetalle,
    pub detalle: Option<Detalle>,
    pub error: Option<String>,
    pub peticion: Option<u64>,
    pub desplazamiento: usize,
}

impl DetalleAbierto {
    /// Texto completo de la pestaña «Unidad» (o inspect).
    pub fn texto_unidad(&self) -> &str {
        self.detalle
            .as_ref()
            .map(|detalle| detalle.texto.as_str())
            .unwrap_or("")
    }
}

/// Una línea del anillo del panel de logs, con las que se omitieron antes.
#[derive(Debug, Clone)]
pub struct LineaAnillo {
    pub linea: LineaLog,
    /// Líneas descartadas por contrapresión justo antes de esta.
    pub omitidas_antes: u32,
}

/// Panel de logs en vivo (§6.3).
#[derive(Debug, Clone)]
pub struct PanelLogs {
    pub host_id: i64,
    pub host_nombre: String,
    pub objetivo: Objetivo,
    pub peticion_id: u64,
    pub abriendo: bool,
    /// Motivo por el que el remoto cerró la suscripción, si terminó.
    pub motivo_fin: Option<MotivoFinLog>,
    pub anillo: VecDeque<LineaAnillo>,
    /// Líneas recibidas en total (para «N líneas»).
    pub total: usize,
    /// Líneas omitidas por contrapresión del servidor, sin pintar todavía.
    pub omitidas: u32,
    /// Bytes que ocupan ahora mismo las líneas del anillo.
    pub bytes: usize,
    /// Líneas que ha descartado el propio anillo (presupuesto de bytes o tope
    /// de líneas), para que el panel sepa cuántas faltan.
    pub omitidas_locales: u32,
    pub filtro: String,
    pub filtro_activo: bool,
    pub nivel: NivelLog,
    pub pausado: bool,
    /// Líneas de distancia al final (0 = pegado al final).
    pub desde_final: usize,
    /// Líneas nuevas desde que se puso en pausa.
    pub nuevas: usize,
    /// Filas visibles en el último pintado (para PgUp/PgDn).
    pub filas: usize,
}

impl PanelLogs {
    pub fn nuevo(host_id: i64, host_nombre: String, objetivo: Objetivo, peticion_id: u64) -> Self {
        Self {
            host_id,
            host_nombre,
            objetivo,
            peticion_id,
            abriendo: true,
            motivo_fin: None,
            anillo: VecDeque::new(),
            total: 0,
            omitidas: 0,
            bytes: 0,
            omitidas_locales: 0,
            filtro: String::new(),
            filtro_activo: false,
            nivel: NivelLog::Todo,
            pausado: false,
            desde_final: 0,
            nuevas: 0,
            filas: 1,
        }
    }

    /// ¿Es de un contenedor? (el filtro de nivel alterna todo / solo stderr).
    pub fn es_contenedor(&self) -> bool {
        self.objetivo.tipo == TipoObjetivo::Contenedor
    }

    /// ¿Pasa el filtro de texto y de nivel?
    pub fn visible(&self, linea: &LineaLog) -> bool {
        if !self.filtro.is_empty() {
            let aguja = self.filtro.to_lowercase();
            if !linea.texto.to_lowercase().contains(&aguja) {
                return false;
            }
        }
        if self.es_contenedor() {
            match self.nivel {
                NivelLog::Todo => true,
                _ => linea.stderr,
            }
        } else {
            linea.prioridad <= self.nivel.prioridad_maxima()
        }
    }

    /// Líneas visibles con su marca de omisiones, en orden.
    pub fn visibles(&self) -> Vec<&LineaAnillo> {
        self.anillo
            .iter()
            .filter(|anillo| self.visible(&anillo.linea))
            .collect()
    }
}

/// Un fallo del último sondeo de un host, para «Toda la flota».
#[derive(Debug, Clone)]
pub struct FalloFlota {
    pub host_id: i64,
    pub host_nombre: String,
    pub fallo: Fallo,
    /// Antigüedad del sondeo en segundos, para «hace 40 s».
    pub antiguedad_seg: f64,
}

/// Fila del panel «Toda la flota».
#[derive(Debug, Clone)]
pub enum FilaFlota {
    Fallo(FalloFlota, usize),
    Nota(String),
}

/// Un elemento seleccionable de la lista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elemento {
    Unidad(String),
    Timer(String),
    Proyecto(String),
    Contenedor(String),
}

impl Elemento {
    pub fn nombre(&self) -> &str {
        match self {
            Elemento::Unidad(nombre)
            | Elemento::Timer(nombre)
            | Elemento::Proyecto(nombre)
            | Elemento::Contenedor(nombre) => nombre,
        }
    }

    pub fn objetivo(&self) -> Objetivo {
        match self {
            Elemento::Unidad(nombre) => Objetivo::unidad(nombre.clone()),
            Elemento::Timer(nombre) => Objetivo {
                tipo: TipoObjetivo::Timer,
                nombre: nombre.clone(),
            },
            Elemento::Proyecto(nombre) => Objetivo::proyecto(nombre.clone()),
            Elemento::Contenedor(nombre) => Objetivo::contenedor(nombre.clone()),
        }
    }
}

/// Una fila de la lista, con su índice de selección si es seleccionable.
#[derive(Debug, Clone)]
pub enum Fila {
    Separador,
    /// Cabecera de un grupo de contenedores que no se selecciona (sin
    /// proyecto).
    Cabecera(String),
    /// Elemento seleccionable con su índice dentro de la lista.
    Elemento(Elemento, usize),
}

/// Estado completo de la vista Servicios en el cliente.
pub struct EstadoServicios {
    pub foco: Foco,
    pub seccion: Seccion,
    /// 0 = «Toda la flota»; 1.. = hosts ssh.
    pub seleccion_host: usize,
    pub seleccion: HashMap<(i64, Seccion), usize>,
    pub seleccion_flota: usize,
    pub listados: HashMap<i64, ListadoHost>,
    pub consumos: HashMap<i64, ConsumoHost>,
    pub filtro: String,
    pub filtro_activo: bool,
    pub detalle: Option<DetalleAbierto>,
    pub logs: Option<PanelLogs>,
    pub acciones: Vec<InfoAccion>,
    pub memoria_sudo: HashMap<i64, MemoriaSudo>,
    pub dialogo: Option<DialogoServicio>,
    /// Host con el que se entra en la vista desde u o desde la ficha.
    pub host_inicial: Option<i64>,
    /// Elemento que hay que seleccionar al llegar el listado (desde «Toda la
    /// flota» con ↵).
    pub elemento_pendiente: Option<(Seccion, String)>,
    /// Acción que hay que ofrecer al llegar el listado (desde «Toda la
    /// flota» con A o R).
    pub accion_pendiente: Option<(Objetivo, Accion)>,
    /// Logs que hay que abrir al llegar el listado (desde «Toda la flota»).
    pub logs_pendientes: Option<Objetivo>,
    /// El servidor de sesiones se cayó: r intenta relanzarlo.
    pub servidor_caido: bool,
    /// Última vista distinta de Servicios, para detectar la salida.
    pub vista_ultima: Option<Vista>,
    /// Desplazamientos de cada panel (los reescribe cada pintado).
    pub desplazamiento_hosts: usize,
    pub desplazamiento_lista: usize,
    pub desplazamiento_flota: usize,
}

impl Default for EstadoServicios {
    fn default() -> Self {
        Self {
            foco: Foco::Hosts,
            seccion: Seccion::Servicios,
            seleccion_host: 0,
            seleccion: HashMap::new(),
            seleccion_flota: 0,
            listados: HashMap::new(),
            consumos: HashMap::new(),
            filtro: String::new(),
            filtro_activo: false,
            detalle: None,
            logs: None,
            acciones: Vec::new(),
            memoria_sudo: HashMap::new(),
            dialogo: None,
            host_inicial: None,
            elemento_pendiente: None,
            accion_pendiente: None,
            logs_pendientes: None,
            servidor_caido: false,
            vista_ultima: None,
            desplazamiento_hosts: 0,
            desplazamiento_lista: 0,
            desplazamiento_flota: 0,
        }
    }
}

impl EstadoServicios {
    /// ¿Hay algo modal en la vista (diálogo)? Bloquea los refrescos.
    pub fn con_dialogo(&self) -> bool {
        self.dialogo.is_some()
    }

    /// Olvida la contraseña de sudo de un host.
    pub fn olvidar_sudo(&mut self, host_id: i64) {
        self.memoria_sudo.remove(&host_id);
    }

    pub fn olvidar_todo_sudo(&mut self) {
        self.memoria_sudo.clear();
    }

    /// Contraseña de sudo vigente del host, sin refrescar el uso.
    pub fn sudo(&self, host_id: i64) -> Option<&str> {
        self.memoria_sudo
            .get(&host_id)
            .map(|memoria| memoria.contrasena.as_str())
    }
}

impl App {
    // ------------------------------------------------------------- entrada

    /// Abre la vista Servicios (F9 o u desde Flota, Hosts o la ficha). Con
    /// host, se queda en su lista; sin él, en la columna de hosts.
    pub(super) fn abrir_servicios(&mut self, host_id: Option<i64>) {
        // Con un host explícito (u) se entra en su lista; con F9, en la
        // columna de hosts aunque se herede el host de la vista anterior.
        let explicito = host_id.is_some();
        if let Some(host_id) = host_id {
            self.servicios.host_inicial = Some(host_id);
        } else if self.vista == Vista::Ficha {
            if let Some(host_id) = self.ficha.as_ref().and_then(|ficha| ficha.host_id) {
                self.servicios.host_inicial = Some(host_id);
            } else {
                self.mensaje("guarda el host antes de ver sus servicios", true);
                return;
            }
        } else if self.vista == Vista::Flota {
            self.servicios.host_inicial = self.host_flota_seleccionado().map(|host| host.id);
        } else if self.vista == Vista::Hosts {
            self.servicios.host_inicial = self.host_seleccionado().map(|host| host.id);
        }
        if self.ficha.as_ref().is_some_and(|ficha| ficha.sucio()) {
            self.dialogo = Some(super::Dialogo::Confirmar {
                titulo: "DESCARTAR CAMBIOS".to_string(),
                lineas: vec!["Hay cambios sin guardar. ¿Descartarlos?".to_string()],
                peligro: true,
                accion: super::AccionDialogo::DescartarFichaHacia(Vista::Servicios),
            });
            return;
        }
        self.salir_de_sesion_si_hace_falta();
        if self.vista != Vista::Servicios {
            self.vista_previa = Some(self.vista);
        }
        self.vista = Vista::Servicios;
        self.ficha = None;
        self.paleta = None;
        self.servicios.vista_ultima = Some(Vista::Servicios);
        let hosts = self.servicios_hostes();
        let pedido = self
            .servicios
            .host_inicial
            .take()
            .filter(|host_id| hosts.contains(host_id));
        let anterior = self
            .servicios_host_seleccionado()
            .filter(|host_id| hosts.contains(host_id));
        let elegido = pedido.or(anterior).or_else(|| hosts.first().copied());
        self.servicios.seleccion_host = elegido
            .and_then(|host_id| hosts.iter().position(|otro| *otro == host_id))
            .map(|indice| indice + 1)
            .unwrap_or(0);
        self.servicios.foco = if explicito { Foco::Lista } else { Foco::Hosts };
        self.sucio = true;
        if let Some(host_id) = elegido {
            self.pedir_listado(host_id);
        }
    }

    /// Si se sale de la vista Servicios, se para todo lo que había en marcha
    /// (se llama tras cada evento y tras cada tecla).
    pub(super) fn vigilar_salida_de_servicios(&mut self) {
        if self.vista == Vista::Servicios {
            self.servicios.vista_ultima = Some(Vista::Servicios);
            return;
        }
        if self.servicios.vista_ultima != Some(Vista::Servicios) {
            return;
        }
        self.servicios.vista_ultima = Some(self.vista);
        self.salir_de_servicios();
    }

    /// Nada en marcha al salir: PararLog, diálogos cerrados y contraseña de
    /// sudo olvidada (el servidor termina sus acciones custodiadas).
    fn salir_de_servicios(&mut self) {
        self.cerrar_logs();
        self.servicios.detalle = None;
        self.servicios.dialogo = None;
        self.servicios.foco = Foco::Hosts;
        self.servicios.filtro_activo = false;
        self.servicios.elemento_pendiente = None;
        self.servicios.accion_pendiente = None;
        self.servicios.logs_pendientes = None;
        self.servicios.olvidar_todo_sudo();
        self.peticiones_servicio.clear();
    }

    /// Al caer el servidor, se conserva cada listado con su hora y se marca
    /// desconectado para que r reconecte.
    pub(super) fn servicios_desconectado(&mut self) {
        self.servicios.servidor_caido = true;
        for listado in self.servicios.listados.values_mut() {
            if listado.estado == EstadoListado::Ok {
                listado.estado = EstadoListado::Desconectado;
                listado.aviso = Some("el servidor de sesiones ha caído".to_string());
            }
        }
    }
}

impl App {
    // ------------------------------------------------------------ consultas

    /// Hosts de la vista: solo los de tipo ssh, en el orden del inventario.
    pub fn servicios_hostes(&self) -> Vec<i64> {
        self.hosts
            .iter()
            .filter(|host| host.tipo.es_ssh())
            .map(|host| host.id)
            .collect()
    }

    /// Host elegido; None es «Toda la flota».
    pub fn servicios_host_seleccionado(&self) -> Option<i64> {
        let hosts = self.servicios_hostes();
        self.servicios
            .seleccion_host
            .checked_sub(1)
            .and_then(|indice| hosts.get(indice).copied())
    }

    /// Nombre del host.
    pub fn servicios_host_nombre(&self, host_id: i64) -> String {
        self.hosts
            .iter()
            .find(|host| host.id == host_id)
            .map(|host| host.nombre.clone())
            .unwrap_or_else(|| format!("host #{host_id}"))
    }

    /// Host completo.
    pub fn servicios_host(&self, host_id: i64) -> Option<&Host> {
        self.hosts.iter().find(|host| host.id == host_id)
    }

    pub fn servicios_listado(&self, host_id: i64) -> Option<&ListadoHost> {
        self.servicios.listados.get(&host_id)
    }

    /// Mínimo que exige el diálogo de Servicios abierto (T45).
    pub fn servicios_minimo_dialogo(&self) -> Option<Minimo> {
        self.servicios.dialogo.as_ref().map(DialogoServicio::minimo)
    }

    /// Listado recibido del host elegido, si lo hay.
    pub fn servicios_listado_actual(&self) -> Option<&ListadoHost> {
        self.servicios_host_seleccionado()
            .and_then(|host_id| self.servicios.listados.get(&host_id))
    }

    /// Motor de contenedores disponible del host, si lo hay.
    pub fn servicios_motor(&self, host_id: i64) -> Option<MotorNombre> {
        self.servicios
            .listados
            .get(&host_id)
            .and_then(|listado| listado.listado.motor.disponible())
            .map(|motor| motor.nombre)
    }

    /// Filas de la lista de la sección visible (con separador y cabeceras).
    pub fn servicios_filas(&self, host_id: i64) -> Vec<Fila> {
        let Some(listado_host) = self.servicios.listados.get(&host_id) else {
            return Vec::new();
        };
        let listado = &listado_host.listado;
        let filtro = self.servicios.filtro.trim().to_lowercase();
        let pasa = |nombre: &str| filtro.is_empty() || nombre.to_lowercase().contains(&filtro);
        match self.servicios.seccion {
            Seccion::Servicios => {
                let mut arriba: Vec<&magi_nucleo::servicios::Unidad> = listado
                    .unidades
                    .iter()
                    .filter(|unidad| !unidad.plantilla() && pasa(&unidad.nombre))
                    .filter(|unidad| unidad.activo() == "failed" || unidad.vigilada)
                    .collect();
                let mut resto: Vec<&magi_nucleo::servicios::Unidad> = listado
                    .unidades
                    .iter()
                    .filter(|unidad| !unidad.plantilla() && pasa(&unidad.nombre))
                    .filter(|unidad| !(unidad.activo() == "failed" || unidad.vigilada))
                    .collect();
                arriba.sort_by_key(|unidad| (unidad.activo() != "failed", unidad.nombre.clone()));
                resto.sort_by_key(|unidad| unidad.nombre.clone());
                let mut filas = Vec::new();
                let mut indice = 0;
                for unidad in &arriba {
                    filas.push(Fila::Elemento(
                        Elemento::Unidad(unidad.nombre.clone()),
                        indice,
                    ));
                    indice += 1;
                }
                if !arriba.is_empty() && !resto.is_empty() {
                    filas.push(Fila::Separador);
                }
                for unidad in &resto {
                    filas.push(Fila::Elemento(
                        Elemento::Unidad(unidad.nombre.clone()),
                        indice,
                    ));
                    indice += 1;
                }
                filas
            }
            Seccion::Timers => {
                let mut timers: Vec<&magi_nucleo::servicios::Timer> = listado
                    .timers
                    .iter()
                    .filter(|timer| pasa(&timer.nombre))
                    .collect();
                timers.sort_by_key(|timer| timer.nombre.clone());
                let mut filas = Vec::new();
                for (indice, timer) in timers.into_iter().enumerate() {
                    filas.push(Fila::Elemento(
                        Elemento::Timer(timer.nombre.clone()),
                        indice,
                    ));
                }
                filas
            }
            Seccion::Contenedores => {
                let mut proyectos: Vec<String> = listado
                    .contenedores
                    .iter()
                    .filter(|contenedor| pasa(&contenedor.nombre))
                    .filter_map(|contenedor| contenedor.proyecto.clone())
                    .collect();
                proyectos.sort();
                proyectos.dedup();
                let mut filas = Vec::new();
                let mut indice = 0;
                for proyecto in &proyectos {
                    let mut contenedores: Vec<&magi_nucleo::servicios::Contenedor> = listado
                        .contenedores
                        .iter()
                        .filter(|contenedor| {
                            contenedor.proyecto.as_deref() == Some(proyecto.as_str())
                                && pasa(&contenedor.nombre)
                        })
                        .collect();
                    contenedores.sort_by_key(|contenedor| {
                        (
                            contenedor
                                .orden
                                .as_deref()
                                .and_then(|orden| orden.parse::<u32>().ok())
                                .unwrap_or(u32::MAX),
                            contenedor.nombre.clone(),
                        )
                    });
                    if contenedores.is_empty() {
                        continue;
                    }
                    filas.push(Fila::Elemento(Elemento::Proyecto(proyecto.clone()), indice));
                    indice += 1;
                    for contenedor in contenedores {
                        filas.push(Fila::Elemento(
                            Elemento::Contenedor(contenedor.nombre.clone()),
                            indice,
                        ));
                        indice += 1;
                    }
                }
                let mut sueltos: Vec<&magi_nucleo::servicios::Contenedor> = listado
                    .contenedores
                    .iter()
                    .filter(|contenedor| contenedor.proyecto.is_none() && pasa(&contenedor.nombre))
                    .collect();
                sueltos.sort_by_key(|contenedor| contenedor.nombre.clone());
                if !sueltos.is_empty() {
                    filas.push(Fila::Cabecera(format!(
                        "sin proyecto {} {}",
                        self.tema.glifos.punto_medio,
                        sueltos.len()
                    )));
                    for contenedor in sueltos {
                        filas.push(Fila::Elemento(
                            Elemento::Contenedor(contenedor.nombre.clone()),
                            indice,
                        ));
                        indice += 1;
                    }
                }
                filas
            }
        }
    }

    /// Elementos seleccionables en el orden que se pintan.
    pub fn servicios_elementos(&self, host_id: i64) -> Vec<Elemento> {
        self.servicios_filas(host_id)
            .into_iter()
            .filter_map(|fila| match fila {
                Fila::Elemento(elemento, _) => Some(elemento),
                Fila::Separador | Fila::Cabecera(_) => None,
            })
            .collect()
    }

    /// Selección vigente de la lista del host (sin pasarse del total).
    pub fn servicios_seleccion(&self, host_id: i64) -> usize {
        let total = self.servicios_elementos(host_id).len();
        self.servicios
            .seleccion
            .get(&(host_id, self.servicios.seccion))
            .copied()
            .unwrap_or(0)
            .min(total.saturating_sub(1))
    }

    /// Elemento seleccionado del host.
    pub fn servicios_elemento_seleccionado(&self, host_id: i64) -> Option<Elemento> {
        let elementos = self.servicios_elementos(host_id);
        elementos.get(self.servicios_seleccion(host_id)).cloned()
    }

    /// Objetivo seleccionado del host.
    pub fn servicios_objetivo_seleccionado(&self, host_id: i64) -> Option<Objetivo> {
        self.servicios_elemento_seleccionado(host_id)
            .map(|elemento| elemento.objetivo())
    }

    /// Acciones que se ofrecen para un objetivo según el listado (§7.2).
    pub fn servicios_acciones_de(&self, host_id: i64, objetivo: &Objetivo) -> Vec<Accion> {
        let Some(listado_host) = self.servicios.listados.get(&host_id) else {
            return Vec::new();
        };
        let listado = &listado_host.listado;
        match objetivo.tipo {
            TipoObjetivo::Unidad => listado
                .unidades
                .iter()
                .find(|unidad| unidad.nombre == objetivo.nombre)
                .map(acciones::acciones_de_unidad)
                .unwrap_or_default(),
            TipoObjetivo::Timer => listado
                .timers
                .iter()
                .find(|timer| timer.nombre == objetivo.nombre)
                .map(|timer| {
                    let servicio = timer.servicio_activado();
                    let estado = servicio.as_deref().and_then(|nombre| {
                        listado
                            .unidades
                            .iter()
                            .find(|unidad| unidad.nombre == nombre)
                            .map(|unidad| unidad.activo())
                    });
                    acciones::acciones_de_timer(timer, estado)
                })
                .unwrap_or_default(),
            TipoObjetivo::Contenedor => listado
                .contenedores
                .iter()
                .find(|contenedor| contenedor.nombre == objetivo.nombre)
                .map(acciones::acciones_de_contenedor)
                .unwrap_or_default(),
            TipoObjetivo::Proyecto => {
                // Sin los ficheros de Compose conocidos no hay acciones del
                // proyecto (§7.12): el motivo lo dice la vista.
                let con_ficheros = listado
                    .proyectos
                    .iter()
                    .find(|proyecto| proyecto.nombre == objetivo.nombre)
                    .is_some_and(|proyecto| proyecto.con_ficheros())
                    || listado.contenedores.iter().any(|contenedor| {
                        contenedor.proyecto.as_deref() == Some(&objetivo.nombre)
                            && !contenedor.ficheros.is_empty()
                    });
                if !con_ficheros {
                    return Vec::new();
                }
                let contenedores: Vec<magi_nucleo::servicios::Contenedor> = listado
                    .contenedores
                    .iter()
                    .filter(|contenedor| contenedor.proyecto.as_deref() == Some(&objetivo.nombre))
                    .cloned()
                    .collect();
                acciones::acciones_de_proyecto(&contenedores)
            }
        }
    }

    /// Acciones que se pueden ofrecer en la barra: las permitidas por el
    /// estado y, si el usuario no puede usar sudo, ninguna privilegiada.
    pub fn servicios_acciones_ofrecidas(&self, host_id: i64, objetivo: &Objetivo) -> Vec<Accion> {
        let Some(listado_host) = self.servicios.listados.get(&host_id) else {
            return Vec::new();
        };
        let motor = listado_host
            .listado
            .motor
            .disponible()
            .map(|motor| motor.nombre);
        let es_contenedor = matches!(
            objetivo.tipo,
            TipoObjetivo::Contenedor | TipoObjetivo::Proyecto
        );
        if es_contenedor && motor.is_none() {
            // Sin motor disponible (sin permiso) no hay acciones de contenedor
            // ni de proyecto (§7.12).
            return Vec::new();
        }
        // H1: con el motor Docker disponible y privilegio sin_sudo, el acceso
        // es directo (grupo docker): la acción no necesita sudo. Con Podman
        // tampoco se usa nunca sudo.
        let usa_sudo = if es_contenedor {
            motor == Some(MotorNombre::Docker) && listado_host.listado.privilegio.usa_sudo()
        } else {
            true
        };
        if usa_sudo && listado_host.listado.privilegio == Privilegio::SinSudo {
            return Vec::new();
        }
        self.servicios_acciones_de(host_id, objetivo)
    }

    /// ¿El objetivo es un proyecto Compose del que no se conocen ficheros?
    pub fn servicios_proyecto_sin_ficheros(&self, host_id: i64, objetivo: &Objetivo) -> bool {
        if objetivo.tipo != TipoObjetivo::Proyecto {
            return false;
        }
        let Some(listado_host) = self.servicios.listados.get(&host_id) else {
            return false;
        };
        let listado = &listado_host.listado;
        let con_ficheros = listado
            .proyectos
            .iter()
            .find(|proyecto| proyecto.nombre == objetivo.nombre)
            .is_some_and(|proyecto| proyecto.con_ficheros())
            || listado.contenedores.iter().any(|contenedor| {
                contenedor.proyecto.as_deref() == Some(&objetivo.nombre)
                    && !contenedor.ficheros.is_empty()
            });
        !con_ficheros
    }

    /// Acción en curso sobre un objetivo, de la difusión del servidor.
    pub fn servicios_accion_en_curso(
        &self,
        host_id: i64,
        objetivo: &Objetivo,
    ) -> Option<&InfoAccion> {
        self.servicios
            .acciones
            .iter()
            .find(|info| info.host_id == host_id && info.objetivo == *objetivo)
    }

    /// ¿Hay una acción del cliente en vuelo sobre ese objetivo? (feedback
    /// inmediato antes de que llegue la difusión).
    pub fn servicios_accion_propia_en_curso(&self, host_id: i64, objetivo: &Objetivo) -> bool {
        self.peticiones_servicio.values().any(|peticion| {
            matches!(
                peticion,
                PeticionServicio::Accion {
                    host_id: otro,
                    accion,
                } if *otro == host_id && accion.objetivo == *objetivo
            )
        })
    }
}

impl App {
    // ------------------------------------------------ toda la flota y avisos

    /// Fallos de «Toda la flota»: los del último sondeo de cada host ssh.
    pub fn servicios_fallos_flota(&self) -> Vec<FalloFlota> {
        let mut filas = Vec::new();
        for host in self.hosts.iter().filter(|host| host.tipo.es_ssh()) {
            let Some(sondeo) = self.sondeos.get(&host.id) else {
                continue;
            };
            let Some(fallos) = fallos_de_sondeo(sondeo) else {
                continue;
            };
            let antiguedad =
                crate::flota::estado::antiguedad_segundos(&sondeo.fecha).unwrap_or_default();
            for fallo in fallos.lista() {
                filas.push(FalloFlota {
                    host_id: host.id,
                    host_nombre: host.nombre.clone(),
                    fallo,
                    antiguedad_seg: antiguedad,
                });
            }
        }
        filas
    }

    /// Filas de «Toda la flota»: los fallos y, debajo, una nota por host.
    pub fn servicios_filas_flota(&self) -> Vec<FilaFlota> {
        let mut filas = Vec::new();
        let fallos = self.servicios_fallos_flota();
        for (indice, fallo) in fallos.iter().enumerate() {
            filas.push(FilaFlota::Fallo(fallo.clone(), indice));
        }
        if !fallos.is_empty() {
            filas.push(FilaFlota::Nota(String::new()));
        }
        let mut sin_fallos: Vec<String> = Vec::new();
        for host in self.hosts.iter().filter(|host| host.tipo.es_ssh()) {
            match self.sondeos.get(&host.id) {
                None => filas.push(FilaFlota::Nota(format!(
                    "{}: sin sondeo {} r para sondear",
                    host.nombre, self.tema.glifos.punto_medio
                ))),
                Some(sondeo)
                    if matches!(sondeo.resultado, crate::modelo::ResultadoSondeo::Error) =>
                {
                    let hace = crate::flota::estado::antiguedad_segundos(&sondeo.fecha)
                        .map(crate::flota::estado::formatear_antiguedad)
                        .unwrap_or_else(|| "sin datos".to_string());
                    filas.push(FilaFlota::Nota(format!(
                        "{}: CAÍDA en el último sondeo (hace {hace}) {} sin datos de servicios",
                        host.nombre, self.tema.glifos.punto_medio
                    )));
                }
                Some(sondeo) => {
                    let total = fallos_de_sondeo(sondeo)
                        .map(|fallos| fallos.total())
                        .unwrap_or(0);
                    if total == 0 {
                        sin_fallos.push(host.nombre.clone());
                    }
                }
            }
        }
        if !sin_fallos.is_empty() {
            filas.push(FilaFlota::Nota(format!(
                "{}: sin fallos",
                sin_fallos.join(" y ")
            )));
        }
        filas
    }

    /// Aviso de la lista del host (permisos, límites, errores).
    pub fn servicios_aviso(&self, host_id: i64) -> Option<(String, bool)> {
        let listado_host = self.servicios.listados.get(&host_id)?;
        if let EstadoListado::NoResponde(motivo) = &listado_host.estado {
            return Some((motivo.clone(), true));
        }
        if listado_host.estado == EstadoListado::Desconectado {
            let mut texto = format!("desconectado {} r reconectar", self.tema.glifos.punto_medio);
            if let Some(aviso) = &listado_host.aviso {
                texto.push_str(&format!(" {} {}", self.tema.glifos.punto_medio, aviso));
            }
            return Some((texto, true));
        }
        let listado = &listado_host.listado;
        if !listado.avisos.is_empty() {
            return Some((listado.avisos.join("; "), false));
        }
        if listado.incompleto {
            return Some((
                "lista incompleta: se han recortado datos del host".to_string(),
                false,
            ));
        }
        match &listado.systemd {
            magi_nucleo::servicios::Systemd::Ausente
                if self.servicios.seccion != Seccion::Contenedores =>
            {
                Some((
                    "sin systemd: solo se ven los contenedores".to_string(),
                    false,
                ))
            }
            magi_nucleo::servicios::Systemd::Antiguo(version)
                if self.servicios.seccion != Seccion::Contenedores =>
            {
                Some((
                    format!("systemd demasiado antiguo ({version}): solo se ven los contenedores"),
                    false,
                ))
            }
            _ => None,
        }
    }

    /// Antigüedad del listado en segundos, para la cabecera.
    pub fn servicios_hora(&self, host_id: i64) -> Option<f64> {
        self.servicios
            .listados
            .get(&host_id)
            .map(ListadoHost::antiguedad_seg)
    }

    /// ¿Hay que pedir el consumo de los contenedores? (solo con esa sección a
    /// la vista y con motor).
    pub fn servicios_pide_consumo(&self, host_id: i64) -> bool {
        self.vista == Vista::Servicios
            && self.servicios.seccion == Seccion::Contenedores
            && self.servicios_motor(host_id).is_some()
    }

    /// Texto de la comprobación de configuración que se aplicará, para la
    /// confirmación («nginx -t», «la comprobación propia»).
    pub fn servicios_comprobacion_texto(
        &self,
        host_id: i64,
        objetivo: &Objetivo,
    ) -> Option<String> {
        let propia = self.servicios_comprobacion_propia(host_id, objetivo);
        validadores::descripcion(&validadores::para_objetivo(objetivo, propia.as_deref()))
    }

    /// Aviso de unidad crítica (y hosts que van a través de ella).
    pub fn servicios_critica(&self, host_id: i64, objetivo: &Objetivo) -> Option<String> {
        if !matches!(objetivo.tipo, TipoObjetivo::Unidad) {
            return None;
        }
        if !acciones::es_critica(&self.config.servicios.criticas, &objetivo.nombre) {
            return None;
        }
        let host_nombre = self.servicios_host_nombre(host_id);
        let atraviesan: Vec<String> = self
            .hosts
            .iter()
            .filter(|host| host.salto_host_id == Some(host_id))
            .map(|host| host.nombre.clone())
            .collect();
        let mut aviso = format!("puede cortar tu conexión con {host_nombre}");
        if !atraviesan.is_empty() {
            aviso.push_str(&format!(
                " y la de los hosts que van a través de él: {}",
                atraviesan.join(", ")
            ));
        }
        Some(aviso)
    }

    /// Comprobación propia guardada para el objetivo, si la hay.
    pub fn servicios_comprobacion_propia(
        &self,
        host_id: i64,
        objetivo: &Objetivo,
    ) -> Option<String> {
        self.comprobacion_propia(host_id, objetivo)
    }

    /// Contraseña de sudo vigente para una LECTURA (listado, consumo, detalle
    /// o logs): no renueva el último uso, así que caduca a los
    /// sudo_memoria_min sin acciones aunque la vista siga abierta (B1).
    pub fn servicios_sudo_lectura(&mut self, host_id: i64) -> Option<Zeroizing<String>> {
        let minutos = self.config.servicios.sudo_memoria_min;
        let caducidad = std::time::Duration::from_secs(minutos.saturating_mul(60));
        let vigente = match self.servicios.memoria_sudo.get(&host_id) {
            Some(_) if minutos == 0 => false,
            Some(memoria) => memoria.ultimo_uso.elapsed() <= caducidad,
            None => false,
        };
        if !vigente {
            self.servicios.memoria_sudo.remove(&host_id);
            return None;
        }
        let memoria = self.servicios.memoria_sudo.get(&host_id)?;
        Some(memoria.contrasena.clone())
    }

    /// Contraseña de sudo vigente para una ACCIÓN o una comprobación:
    /// refresca su último uso (solo estas cuentan como uso, B1).
    pub fn servicios_sudo_accion(&mut self, host_id: i64) -> Option<Zeroizing<String>> {
        let minutos = self.config.servicios.sudo_memoria_min;
        let caducidad = std::time::Duration::from_secs(minutos.saturating_mul(60));
        let vigente = match self.servicios.memoria_sudo.get(&host_id) {
            Some(_) if minutos == 0 => false,
            Some(memoria) => memoria.ultimo_uso.elapsed() <= caducidad,
            None => false,
        };
        if !vigente {
            self.servicios.memoria_sudo.remove(&host_id);
            return None;
        }
        let memoria = self.servicios.memoria_sudo.get_mut(&host_id)?;
        memoria.ultimo_uso = Instant::now();
        Some(memoria.contrasena.clone())
    }

    /// Borra las contraseñas de sudo caducadas (se llama en cada tick).
    pub fn servicios_purgar_sudo(&mut self) {
        let minutos = self.config.servicios.sudo_memoria_min;
        let caducidad = std::time::Duration::from_secs(minutos.saturating_mul(60));
        self.servicios
            .memoria_sudo
            .retain(|_, memoria| minutos > 0 && memoria.ultimo_uso.elapsed() <= caducidad);
    }
}

/// Fallos del último sondeo de un host (SONDEOS.fallos_json).
fn fallos_de_sondeo(sondeo: &crate::modelo::Sondeo) -> Option<&FallosSondeo> {
    sondeo.fallos.as_ref()
}

impl App {
    // ----------------------------------------------------------- peticiones

    /// Pide el listado de un host (sin solapar peticiones del mismo host).
    pub(super) fn pedir_listado(&mut self, host_id: i64) {
        if let Some(motivo) = self.motivo_servidor_incompatible() {
            self.mensaje(motivo, true);
            return;
        }
        if self.servidor_caido {
            self.mensaje("el servidor de sesiones ha caído; relánzalo primero", true);
            return;
        }
        let en_vuelo = self.peticiones_servicio.values().any(|peticion| {
            matches!(peticion, PeticionServicio::Listar { host_id: otro } if *otro == host_id)
        });
        if en_vuelo {
            return;
        }
        let sudo = self.sudo_para_lectura(host_id);
        let peticion_id = self.siguiente_peticion_servicios;
        self.siguiente_peticion_servicios += 1;
        self.peticiones_servicio
            .insert(peticion_id, PeticionServicio::Listar { host_id });
        if let Some(listado) = self.servicios.listados.get_mut(&host_id) {
            listado.estado = EstadoListado::Cargando;
        }
        self.servidor.enviar(MensajeCliente::ListarServicios {
            peticion_id,
            host_id,
            sudo,
        });
        self.sucio = true;
    }

    /// Pide el consumo de los contenedores del host (va aparte del listado).
    fn pedir_consumo(&mut self, host_id: i64) {
        if self.servicios_motor(host_id).is_none() || !self.servidor_para_servicios() {
            return;
        }
        let en_vuelo = self.peticiones_servicio.values().any(|peticion| {
            matches!(peticion, PeticionServicio::Consumo { host_id: otro } if *otro == host_id)
        });
        if en_vuelo {
            return;
        }
        let sudo = self.sudo_para_lectura(host_id);
        let peticion_id = self.siguiente_peticion_servicios;
        self.siguiente_peticion_servicios += 1;
        self.peticiones_servicio
            .insert(peticion_id, PeticionServicio::Consumo { host_id });
        self.servidor.enviar(MensajeCliente::PedirConsumo {
            peticion_id,
            host_id,
            sudo,
        });
        self.sucio = true;
    }

    /// Abre el detalle de un objetivo (↵).
    fn pedir_detalle(&mut self, host_id: i64, objetivo: Objetivo) {
        if !self.servidor_para_servicios() {
            return;
        }
        let host_nombre = self.servicios_host_nombre(host_id);
        let sudo = self.sudo_para_lectura(host_id);
        let peticion_id = self.siguiente_peticion_servicios;
        self.siguiente_peticion_servicios += 1;
        self.peticiones_servicio.insert(
            peticion_id,
            PeticionServicio::Detalle {
                host_id,
                objetivo: objetivo.clone(),
            },
        );
        self.servidor.enviar(MensajeCliente::DetalleServicio {
            peticion_id,
            host_id,
            objetivo: objetivo.clone(),
            sudo,
        });
        self.servicios.detalle = Some(DetalleAbierto {
            host_id,
            host_nombre,
            objetivo,
            pestaña: PestanaDetalle::Estado,
            detalle: None,
            error: None,
            peticion: Some(peticion_id),
            desplazamiento: 0,
        });
        self.sucio = true;
    }

    /// ¿Se puede hablar con el servidor de sesiones? Si no, avisa y devuelve
    /// falso (las lecturas no deben quedarse «cargando» para siempre).
    fn servidor_para_servicios(&mut self) -> bool {
        if let Some(motivo) = self.motivo_servidor_incompatible() {
            self.mensaje(motivo, true);
            return false;
        }
        if self.servidor_caido {
            self.mensaje(
                "el servidor de sesiones ha caído: pulsa r en Servicios para relanzarlo",
                true,
            );
            return false;
        }
        true
    }

    /// Contraseña de sudo para una lectura: si está vigente se adjunta, pero
    /// no se renueva su uso (B1).
    fn sudo_para_lectura(&mut self, host_id: i64) -> Option<Secreto> {
        self.servicios_sudo_lectura(host_id)
            .map(|contrasena| Secreto::nuevo(contrasena.as_str()))
    }

    /// Contraseña de sudo para una acción: si está vigente se adjunta y se
    /// renueva su último uso.
    fn sudo_para_accion(&mut self, host_id: i64) -> Option<Secreto> {
        self.servicios_sudo_accion(host_id)
            .map(|contrasena| Secreto::nuevo(contrasena.as_str()))
    }

    /// Abre el diálogo de contraseña de sudo. Con pendiente, al aceptar se
    /// sigue con esa acción; sin él (!), solo se comprueba y se refresca.
    fn pedir_sudo(&mut self, host_id: i64, pendiente: Option<AccionPendiente>) {
        let host = self.servicios_host(host_id);
        let host_nombre = self.servicios_host_nombre(host_id);
        self.servicios.dialogo = Some(DialogoServicio::Sudo(SudoPedido {
            host_id,
            host_nombre,
            usuario: host.and_then(|host| host.usuario.clone()),
            campo: CampoTexto::default(),
            intento: 0,
            error: None,
            pendiente,
        }));
        self.sucio = true;
    }

    // ------------------------------------------------------------ refresco

    /// Cada tick: olvida contraseñas caducadas y, con la vista abierta y sin
    /// diálogos, refresca al ritmo de [servicios] refresco_seg (sin solapes).
    pub(super) fn tick_servicios(&mut self) {
        self.servicios_purgar_sudo();
        if self.vista != Vista::Servicios {
            return;
        }
        if self.servicios.con_dialogo() || self.deliberacion.is_some() || self.dialogo.is_some() {
            return;
        }
        let Some(host_id) = self.servicios_host_seleccionado() else {
            return;
        };
        let refresco = self.config.servicios.refresco_seg;
        if refresco == 0 {
            return;
        }
        let (vencido, caido) = match self.servicios.listados.get(&host_id) {
            Some(listado) => (
                listado.recibido.elapsed().as_secs() >= refresco,
                matches!(
                    listado.estado,
                    EstadoListado::NoResponde(_) | EstadoListado::Desconectado
                ),
            ),
            None => (true, false),
        };
        if !vencido || caido {
            return;
        }
        self.pedir_listado(host_id);
        if self.servicios_pide_consumo(host_id) {
            self.pedir_consumo(host_id);
        }
    }
}

impl App {
    // --------------------------------------------------------------- teclas

    /// Teclas de la vista Servicios (los diálogos y la ayuda se atienden
    /// antes, en procesar_tecla).
    pub(super) fn tecla_servicios(&mut self, tecla: KeyEvent) {
        if self.servicios.logs.is_some() {
            self.tecla_logs(tecla);
            return;
        }
        if self.servicios.filtro_activo {
            self.tecla_filtro_servicios(tecla);
            return;
        }
        if tecla.code == KeyCode::Esc {
            if self.servicios.detalle.is_some() {
                self.servicios.detalle = None;
            } else if !self.servicios.filtro.is_empty() {
                self.servicios.filtro.clear();
            } else {
                self.volver_de_servicios();
            }
            self.sucio = true;
            return;
        }
        if tecla.code == KeyCode::Char('q') && !tecla.modifiers.contains(KeyModifiers::CONTROL) {
            if self.servicios.detalle.is_some() {
                self.servicios.detalle = None;
            } else {
                self.volver_de_servicios();
            }
            self.sucio = true;
            return;
        }
        if self.servicios.detalle.is_some() {
            self.tecla_detalle_servicio(tecla);
            return;
        }
        if let KeyCode::Char(caracter) = tecla.code {
            if let Some(seccion) = Seccion::de_numero(caracter) {
                self.cambiar_seccion(seccion);
                return;
            }
        }
        if self.servicios_host_seleccionado().is_none() {
            self.tecla_toda_la_flota(tecla);
            return;
        }
        if tecla.code == KeyCode::Tab || tecla.code == KeyCode::BackTab {
            self.servicios.foco = match self.servicios.foco {
                Foco::Hosts => Foco::Lista,
                Foco::Lista => Foco::Hosts,
            };
            self.sucio = true;
            return;
        }
        match self.servicios.foco {
            Foco::Hosts => self.tecla_hosts_servicios(tecla),
            Foco::Lista => self.tecla_lista_servicios(tecla),
        }
    }

    /// Vuelve a la vista de la que se entró (o a Hosts).
    fn volver_de_servicios(&mut self) {
        let destino = match self.vista_previa {
            Some(Vista::Flota) => Vista::Flota,
            Some(Vista::Ficha) | None => Vista::Hosts,
            Some(vista) if vista != Vista::Servicios => vista,
            _ => Vista::Hosts,
        };
        match destino {
            Vista::Flota => self.entrar_en_flota(),
            Vista::Hosts => self.ir_a_hosts(),
            otro => self.vista = otro,
        }
    }

    // ------------------------------------------------------- columna de hosts

    fn tecla_hosts_servicios(&mut self, tecla: KeyEvent) {
        let hosts = self.servicios_hostes();
        let total = hosts.len() + 1;
        match tecla.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.servicios.seleccion_host = self.servicios.seleccion_host.saturating_sub(1);
                self.al_cambiar_host_servicios();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.servicios.seleccion_host + 1 < total {
                    self.servicios.seleccion_host += 1;
                    self.al_cambiar_host_servicios();
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if self.servicios.seleccion_host > 0 {
                    self.servicios.foco = Foco::Lista;
                    self.sucio = true;
                }
            }
            KeyCode::Char('/') => {
                self.servicios.foco = Foco::Lista;
                self.servicios.filtro_activo = true;
                self.servicios.filtro.clear();
                self.sucio = true;
            }
            KeyCode::Char('r') => match self.servicios_host_seleccionado() {
                Some(host_id) => self.refrescar_host_servicios(host_id),
                None => self.sondear_flota_desde_servicios(),
            },
            KeyCode::Char('!') => match self.servicios_host_seleccionado() {
                Some(host_id) => self.pedir_sudo(host_id, None),
                None => self.mensaje("elige un host para usar sudo", true),
            },
            _ => {}
        }
    }

    /// Al moverse por la columna de hosts: se para lo del host anterior y se
    /// pide el listado del nuevo (si no está en vuelo).
    fn al_cambiar_host_servicios(&mut self) {
        // Las respuestas del host que se deja ya no interesan (T33): si
        // llegasen, se descartarían por no estar en vuelo.
        let nuevo = self.servicios_host_seleccionado();
        self.peticiones_servicio
            .retain(|_, peticion| match peticion {
                PeticionServicio::Listar { host_id } => Some(*host_id) == nuevo,
                PeticionServicio::Consumo { host_id } => Some(*host_id) == nuevo,
                _ => true,
            });
        self.cerrar_logs();
        self.servicios.detalle = None;
        self.servicios.filtro.clear();
        self.servicios.filtro_activo = false;
        self.sucio = true;
        if let Some(host_id) = self.servicios_host_seleccionado() {
            let ya_visto = self
                .servicios
                .listados
                .get(&host_id)
                .is_some_and(|listado| {
                    listado.estado == EstadoListado::Ok
                        && listado.recibido.elapsed().as_secs()
                            < self.config.servicios.refresco_seg.max(1)
                });
            if !ya_visto {
                self.pedir_listado(host_id);
            }
            if self.servicios_pide_consumo(host_id) {
                self.pedir_consumo(host_id);
            }
        }
    }

    /// Refresca un host (r): se olvida el estado de desconexión y se reintenta.
    /// Si el que se cayó es el servidor de sesiones, primero se relanza (§4.1).
    pub(super) fn refrescar_host_servicios(&mut self, host_id: i64) {
        let desconectado = self
            .servicios
            .listados
            .get(&host_id)
            .is_some_and(|listado| listado.estado == EstadoListado::Desconectado);
        if self.servidor_caido || self.servicios.servidor_caido {
            self.reconectar_servidor_para_servicios();
            return;
        }
        if let Some(listado) = self.servicios.listados.get_mut(&host_id) {
            if desconectado {
                listado.estado = EstadoListado::Ok;
                listado.aviso = None;
            }
        }
        self.pedir_listado(host_id);
        if self.servicios_pide_consumo(host_id) {
            self.pedir_consumo(host_id);
        }
    }

    /// Relanza la conexión con el servidor de sesiones para poder reconectar
    /// con los hosts. No bloquea la UI: el resultado llega por eventos.
    fn reconectar_servidor_para_servicios(&mut self) {
        let rutas = self.rutas.clone();
        let tx = self.eventos_tx.clone();
        self.runtime.spawn(async move {
            match crate::cliente::conectar(&rutas, tx.clone()).await {
                Ok(cliente) => {
                    let _ = tx.send(super::Evento::ServidorConectado(Ok(cliente)));
                }
                Err(fallo) => {
                    let _ = tx.send(super::Evento::ServidorConectado(Err(
                        crate::cliente::describe_fallo(&fallo),
                    )));
                }
            }
        });
        self.mensaje("reconectando con el servidor de sesiones…", false);
        self.sucio = true;
    }

    /// r en «Toda la flota»: sondea todos los hosts ssh, como R en Flota.
    fn sondear_flota_desde_servicios(&mut self) {
        let ids = self.servicios_hostes();
        if ids.is_empty() {
            self.mensaje("no hay hosts ssh para sondear", true);
            return;
        }
        self.sondear_hosts(ids);
        self.mensaje("sondeando todos los hosts…", false);
    }

    // --------------------------------------------------------- lista del host

    fn tecla_lista_servicios(&mut self, tecla: KeyEvent) {
        let Some(host_id) = self.servicios_host_seleccionado() else {
            return;
        };
        let pagina = self.disposicion.filas(Lista::ServiciosLista) as i32;
        match tecla.code {
            KeyCode::Up | KeyCode::Char('k') => self.mover_seleccion_lista(-1),
            KeyCode::Down | KeyCode::Char('j') => self.mover_seleccion_lista(1),
            KeyCode::PageUp => self.mover_seleccion_lista(-pagina),
            KeyCode::PageDown => self.mover_seleccion_lista(pagina),
            KeyCode::Home => self.mover_seleccion_lista(-(i32::MAX / 2)),
            KeyCode::End => self.mover_seleccion_lista(i32::MAX / 2),
            KeyCode::Enter => {
                if let Some(objetivo) = self.servicios_objetivo_seleccionado(host_id) {
                    self.pedir_detalle(host_id, objetivo);
                }
            }
            KeyCode::Char('l') => {
                if let Some(objetivo) = self.servicios_objetivo_seleccionado(host_id) {
                    self.abrir_logs_servicio(host_id, objetivo);
                }
            }
            KeyCode::Char('A') => self.accion_servicios(Accion::Arrancar),
            KeyCode::Char('P') => self.accion_servicios(Accion::Parar),
            KeyCode::Char('R') => self.accion_servicios(Accion::Reiniciar),
            KeyCode::Char('G') => self.accion_servicios(Accion::Recargar),
            KeyCode::Char('H') => {
                let Some(objetivo) = self.servicios_objetivo_seleccionado(host_id) else {
                    return;
                };
                let acciones = self.servicios_acciones_de(host_id, &objetivo);
                if acciones.contains(&Accion::Activar) {
                    self.iniciar_accion_servicio(host_id, objetivo, Accion::Activar);
                } else if acciones.contains(&Accion::Desactivar) {
                    self.iniciar_accion_servicio(host_id, objetivo, Accion::Desactivar);
                } else {
                    let motivo =
                        acciones::motivo_no_permitida(&objetivo, Accion::Activar, &acciones)
                            .unwrap_or_else(|| {
                                format!("{} no se puede activar ni desactivar", objetivo.nombre)
                            });
                    self.mensaje(motivo, true);
                }
            }
            KeyCode::Char('S') => self.shell_de_contenedor(host_id),
            KeyCode::Char('c') => {
                if let Some(objetivo) = self.servicios_objetivo_seleccionado(host_id) {
                    self.abrir_editor_comprobacion(host_id, Some(objetivo));
                }
            }
            KeyCode::Char('!') => self.pedir_sudo(host_id, None),
            KeyCode::Char('/') => {
                self.servicios.filtro_activo = true;
                self.sucio = true;
            }
            KeyCode::Char('r') => self.refrescar_host_servicios(host_id),
            KeyCode::Tab | KeyCode::BackTab => {
                self.servicios.foco = Foco::Hosts;
                self.sucio = true;
            }
            _ => {}
        }
    }

    fn mover_seleccion_lista(&mut self, delta: i32) {
        let Some(host_id) = self.servicios_host_seleccionado() else {
            return;
        };
        let total = self.servicios_elementos(host_id).len();
        if total == 0 {
            return;
        }
        let actual = self.servicios_seleccion(host_id) as i32;
        let nuevo = (actual.saturating_add(delta)).clamp(0, total as i32 - 1);
        self.servicios
            .seleccion
            .insert((host_id, self.servicios.seccion), nuevo as usize);
        self.sucio = true;
    }

    /// Cambia de sección con 1, 2 y 3 (y pide el consumo si toca).
    fn cambiar_seccion(&mut self, seccion: Seccion) {
        if self.servicios.seccion != seccion {
            self.servicios.seccion = seccion;
            self.sucio = true;
        }
        if let Some(host_id) = self.servicios_host_seleccionado() {
            if self.servicios_pide_consumo(host_id) {
                self.pedir_consumo(host_id);
            }
        }
    }

    // ------------------------------------------------------- toda la flota

    fn tecla_toda_la_flota(&mut self, tecla: KeyEvent) {
        let fallos = self.servicios_fallos_flota();
        let total = fallos.len();
        let pagina = self.disposicion.filas(Lista::ServiciosTodaLaFlota);
        match tecla.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.servicios.seleccion_flota = self.servicios.seleccion_flota.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.servicios.seleccion_flota + 1 < total {
                    self.servicios.seleccion_flota += 1;
                }
            }
            KeyCode::PageUp => {
                self.servicios.seleccion_flota =
                    self.servicios.seleccion_flota.saturating_sub(pagina);
            }
            KeyCode::PageDown => {
                self.servicios.seleccion_flota =
                    (self.servicios.seleccion_flota + pagina).min(total.saturating_sub(1));
            }
            KeyCode::Enter => self.ir_a_fallo_flota(AccionFlota::Ir),
            KeyCode::Char('l') => self.ir_a_fallo_flota(AccionFlota::Logs),
            KeyCode::Char('A') => self.ir_a_fallo_flota(AccionFlota::Accion(Accion::Arrancar)),
            KeyCode::Char('R') => self.ir_a_fallo_flota(AccionFlota::Accion(Accion::Reiniciar)),
            KeyCode::Char('r') => self.sondear_flota_desde_servicios(),
            KeyCode::Tab | KeyCode::BackTab => {
                // Vuelve a la columna de hosts con el primero seleccionado.
                self.servicios.foco = Foco::Hosts;
                if self.servicios.seleccion_host == 0 && !self.servicios_hostes().is_empty() {
                    self.servicios.seleccion_host = 1;
                    self.al_cambiar_host_servicios();
                }
                self.sucio = true;
            }
            _ => {}
        }
    }

    /// Abre el host del fallo seleccionado y guarda lo que hay que hacer al
    /// llegar el listado (↵, l, A o R).
    fn ir_a_fallo_flota(&mut self, que: AccionFlota) {
        let fallos = self.servicios_fallos_flota();
        let Some(fallo) = fallos.get(self.servicios.seleccion_flota) else {
            if self.servicios.seleccion_flota > 0 {
                self.servicios.seleccion_flota -= 1;
            }
            return;
        };
        let host_id = fallo.host_id;
        let objetivo = match fallo.fallo.tipo {
            magi_nucleo::servicios::TipoFallo::Servicio => {
                Objetivo::unidad(fallo.fallo.nombre.clone())
            }
            magi_nucleo::servicios::TipoFallo::Contenedor => {
                Objetivo::contenedor(fallo.fallo.nombre.clone())
            }
        };
        let seccion = match fallo.fallo.tipo {
            magi_nucleo::servicios::TipoFallo::Servicio => Seccion::Servicios,
            magi_nucleo::servicios::TipoFallo::Contenedor => Seccion::Contenedores,
        };
        self.servicios.seccion = seccion;
        self.abrir_servicios(Some(host_id));
        self.servicios.foco = Foco::Lista;
        match que {
            AccionFlota::Ir => {
                self.servicios.elemento_pendiente = Some((seccion, fallo.fallo.nombre.clone()));
            }
            AccionFlota::Logs => {
                self.servicios.logs_pendientes = Some(objetivo.clone());
            }
            AccionFlota::Accion(accion) => {
                self.servicios.elemento_pendiente = Some((seccion, fallo.fallo.nombre.clone()));
                self.servicios.accion_pendiente = Some((objetivo, accion));
            }
        }
        self.sucio = true;
    }
}

/// Qué hacer al ir a un host desde «Toda la flota».
enum AccionFlota {
    Ir,
    Logs,
    Accion(Accion),
}

impl App {
    // ---------------------------------------------------------------- filtro

    fn tecla_filtro_servicios(&mut self, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Esc => {
                self.servicios.filtro.clear();
                self.servicios.filtro_activo = false;
                self.sucio = true;
            }
            KeyCode::Enter => {
                self.servicios.filtro_activo = false;
                self.sucio = true;
            }
            _ => {
                let mut campo = CampoTexto::nuevo(self.servicios.filtro.clone());
                if campo.manejar_tecla(&tecla) {
                    self.servicios.filtro = campo.texto;
                    let host = self.servicios_host_seleccionado();
                    if let Some(host_id) = host {
                        let total = self.servicios_elementos(host_id).len();
                        let seleccion = self
                            .servicios
                            .seleccion
                            .entry((host_id, self.servicios.seccion))
                            .or_insert(0);
                        if *seleccion >= total {
                            *seleccion = total.saturating_sub(1);
                        }
                    }
                    self.sucio = true;
                }
            }
        }
    }

    // --------------------------------------------------------------- detalle

    fn tecla_detalle_servicio(&mut self, tecla: KeyEvent) {
        let Some(detalle) = self.servicios.detalle.as_mut() else {
            return;
        };
        let pagina = self.disposicion.filas(Lista::ServiciosDetalle);
        match tecla.code {
            KeyCode::Esc => {
                self.servicios.detalle = None;
                self.sucio = true;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                detalle.pestaña = detalle.pestaña.siguiente();
                detalle.desplazamiento = 0;
                self.sucio = true;
            }
            KeyCode::Char('1') => {
                detalle.pestaña = PestanaDetalle::Estado;
                detalle.desplazamiento = 0;
            }
            KeyCode::Char('2') => {
                detalle.pestaña = PestanaDetalle::Dependencias;
                detalle.desplazamiento = 0;
            }
            KeyCode::Char('3') => {
                detalle.pestaña = PestanaDetalle::Unidad;
                detalle.desplazamiento = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                detalle.desplazamiento = detalle.desplazamiento.saturating_sub(1);
                self.sucio = true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                detalle.desplazamiento = detalle.desplazamiento.saturating_add(1);
                self.sucio = true;
            }
            KeyCode::PageUp => {
                detalle.desplazamiento = detalle.desplazamiento.saturating_sub(pagina);
                self.sucio = true;
            }
            KeyCode::PageDown => {
                detalle.desplazamiento = detalle.desplazamiento.saturating_add(pagina);
                self.sucio = true;
            }
            KeyCode::Home => {
                detalle.desplazamiento = 0;
                self.sucio = true;
            }
            KeyCode::End => {
                detalle.desplazamiento = usize::MAX / 2;
                self.sucio = true;
            }
            KeyCode::Char('l') => {
                let (host_id, objetivo) = (detalle.host_id, detalle.objetivo.clone());
                self.abrir_logs_servicio(host_id, objetivo);
            }
            KeyCode::Char('c') => {
                let (host_id, objetivo) = (detalle.host_id, detalle.objetivo.clone());
                self.abrir_editor_comprobacion(host_id, Some(objetivo));
            }
            KeyCode::Char('r') => {
                let (host_id, objetivo) = (detalle.host_id, detalle.objetivo.clone());
                self.pedir_detalle(host_id, objetivo);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ logs

    /// Abre el panel de logs de un objetivo (l).
    fn abrir_logs_servicio(&mut self, host_id: i64, objetivo: Objetivo) {
        if !self.servidor_para_servicios() {
            return;
        }
        if self.servicios.logs.is_some() {
            self.cerrar_logs();
        }
        let host_nombre = self.servicios_host_nombre(host_id);
        let sudo = self.sudo_para_lectura(host_id);
        let peticion_id = self.siguiente_peticion_servicios;
        self.siguiente_peticion_servicios += 1;
        self.peticiones_servicio.insert(
            peticion_id,
            PeticionServicio::Log {
                host_id,
                objetivo: objetivo.clone(),
            },
        );
        self.servidor.enviar(MensajeCliente::SeguirLog {
            peticion_id,
            host_id,
            objetivo: objetivo.clone(),
            lineas_iniciales: self.config.servicios.lineas_iniciales,
            sudo,
        });
        self.servicios.logs = Some(PanelLogs::nuevo(
            host_id,
            host_nombre,
            objetivo,
            peticion_id,
        ));
        self.sucio = true;
    }

    /// Cierra el panel de logs y para la suscripción en el servidor.
    fn cerrar_logs(&mut self) {
        let Some(logs) = self.servicios.logs.take() else {
            return;
        };
        if self.peticiones_servicio.remove(&logs.peticion_id).is_some() {
            self.servidor.enviar(MensajeCliente::PararLog {
                peticion_id: logs.peticion_id,
            });
        }
        self.sucio = true;
    }

    /// Añade un lote al anillo del panel, respetando el tope de lineas_log.
    fn anadir_lineas_log(&mut self, peticion_id: u64, lineas: Vec<LineaLog>, omitidas: u32) {
        let limite = self.config.servicios.lineas_log.max(1);
        let pausado;
        let peticion_vigente;
        {
            let Some(logs) = self.servicios.logs.as_mut() else {
                return;
            };
            if logs.peticion_id != peticion_id {
                return;
            }
            logs.abriendo = false;
            peticion_vigente = true;
            let cuantas = lineas.len();
            // Las omitidas del servidor y las que descarte el anillo local se
            // marcan en la primera línea que sí se guarda («… N líneas
            // omitidas»).
            let mut pendiente_omision = omitidas;
            for linea in lineas {
                let bytes_linea = linea.texto.len();
                // Presupuesto de bytes (M3): fuera las más viejas.
                while logs.bytes.saturating_add(bytes_linea) > PRESUPUESTO_LOG_BYTES
                    && !logs.anillo.is_empty()
                {
                    if let Some(vieja) = logs.anillo.pop_front() {
                        logs.bytes = logs.bytes.saturating_sub(vieja.linea.texto.len());
                        logs.omitidas_locales = logs.omitidas_locales.saturating_add(1);
                        pendiente_omision = pendiente_omision.saturating_add(1);
                    }
                }
                // Tope de líneas de [servicios] lineas_log.
                while logs.anillo.len() >= limite {
                    if let Some(vieja) = logs.anillo.pop_front() {
                        logs.bytes = logs.bytes.saturating_sub(vieja.linea.texto.len());
                        logs.omitidas_locales = logs.omitidas_locales.saturating_add(1);
                        pendiente_omision = pendiente_omision.saturating_add(1);
                    }
                }
                logs.anillo.push_back(LineaAnillo {
                    linea,
                    omitidas_antes: std::mem::take(&mut pendiente_omision),
                });
                logs.bytes = logs.bytes.saturating_add(bytes_linea);
                logs.total += 1;
            }
            logs.omitidas = logs.omitidas.saturating_add(omitidas);
            pausado = logs.pausado;
            if pausado {
                logs.nuevas += cuantas;
            } else {
                logs.nuevas = 0;
            }
            self.sucio = true;
        }
        if peticion_vigente && !pausado {
            if let Some(logs) = self.servicios.logs.as_mut() {
                logs.desde_final = 0;
            }
        }
    }

    fn tecla_logs(&mut self, tecla: KeyEvent) {
        let contenedor = self
            .servicios
            .logs
            .as_ref()
            .is_some_and(PanelLogs::es_contenedor);
        {
            let Some(logs) = self.servicios.logs.as_mut() else {
                return;
            };
            if logs.filtro_activo {
                match tecla.code {
                    KeyCode::Esc => {
                        logs.filtro.clear();
                        logs.filtro_activo = false;
                    }
                    KeyCode::Enter => logs.filtro_activo = false,
                    _ => {
                        let mut campo = CampoTexto::nuevo(logs.filtro.clone());
                        if campo.manejar_tecla(&tecla) {
                            logs.filtro = campo.texto;
                            logs.desde_final = 0;
                        }
                    }
                }
                self.sucio = true;
                return;
            }
        }
        let pagina = self.disposicion.filas(Lista::ServiciosLogs);
        if tecla.code == KeyCode::Esc {
            self.cerrar_logs();
            return;
        }
        if tecla.code == KeyCode::Char('t') {
            self.abrir_pestana_de_logs();
            return;
        }
        let Some(logs) = self.servicios.logs.as_mut() else {
            return;
        };
        match tecla.code {
            KeyCode::Char('/') => {
                logs.filtro_activo = true;
                logs.filtro.clear();
            }
            KeyCode::Char('n') => {
                logs.nivel = if contenedor {
                    match logs.nivel {
                        NivelLog::Todo => NivelLog::Error,
                        _ => NivelLog::Todo,
                    }
                } else {
                    logs.nivel.siguiente()
                };
                logs.desde_final = 0;
            }
            KeyCode::Char(' ') => {
                logs.pausado = !logs.pausado;
                if logs.pausado {
                    logs.nuevas = 0;
                } else {
                    logs.desde_final = 0;
                }
            }
            KeyCode::End => {
                logs.desde_final = 0;
                logs.pausado = false;
                logs.nuevas = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let visibles = logs.visibles().len();
                if visibles > 0 {
                    logs.desde_final = (logs.desde_final + 1).min(visibles.saturating_sub(1));
                    logs.pausado = true;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                logs.desde_final = logs.desde_final.saturating_sub(1);
                if logs.desde_final == 0 {
                    logs.nuevas = 0;
                }
            }
            KeyCode::PageUp => {
                let visibles = logs.visibles().len();
                logs.desde_final = (logs.desde_final + pagina).min(visibles.saturating_sub(1));
                logs.pausado = true;
            }
            KeyCode::PageDown => {
                logs.desde_final = logs.desde_final.saturating_sub(pagina);
                if logs.desde_final == 0 {
                    logs.nuevas = 0;
                }
            }
            KeyCode::Home => {
                let visibles = logs.visibles().len();
                logs.desde_final = visibles.saturating_sub(1);
                logs.pausado = true;
            }
            _ => return,
        }
        self.sucio = true;
    }
}

impl App {
    // -------------------------------------------------------------- acciones

    /// Tecla de acción sobre el elemento seleccionado de la lista.
    fn accion_servicios(&mut self, accion: Accion) {
        let Some(host_id) = self.servicios_host_seleccionado() else {
            self.mensaje(
                "en «Toda la flota» las acciones van sobre el fallo seleccionado",
                true,
            );
            return;
        };
        let Some(objetivo) = self.servicios_objetivo_seleccionado(host_id) else {
            self.mensaje("no hay nada seleccionado en la lista", true);
            return;
        };
        let accion = if objetivo.tipo == TipoObjetivo::Timer && accion == Accion::Arrancar {
            Accion::Lanzar
        } else {
            accion
        };
        self.iniciar_accion_servicio(host_id, objetivo, accion);
    }

    /// Comprueba que la acción vale, pide la contraseña de sudo si hace falta
    /// y sigue con la confirmación o la deliberación (§4.2).
    fn iniciar_accion_servicio(&mut self, host_id: i64, objetivo: Objetivo, accion: Accion) {
        let (privilegio, motor) = match self.servicios.listados.get(&host_id) {
            Some(listado_host) => (
                listado_host.listado.privilegio,
                listado_host
                    .listado
                    .motor
                    .disponible()
                    .map(|motor| motor.nombre),
            ),
            None => {
                self.mensaje("todavía no hay listado de ese host", true);
                return;
            }
        };
        let permitidas = self.servicios_acciones_de(host_id, &objetivo);
        if !permitidas.contains(&accion) {
            if self.servicios_proyecto_sin_ficheros(host_id, &objetivo) {
                self.mensaje(
                    "sin ficheros de compose conocidos: solo acciones por contenedor",
                    true,
                );
                return;
            }
            let motivo = acciones::motivo_no_permitida(&objetivo, accion, &permitidas)
                .unwrap_or_else(|| {
                    format!("{} no se puede {} ahora", objetivo.nombre, accion.texto())
                });
            self.mensaje(motivo, true);
            return;
        }
        if self.servicios_accion_en_curso(host_id, &objetivo).is_some()
            || self.servicios_accion_propia_en_curso(host_id, &objetivo)
        {
            self.mensaje(
                format!("hay otra acción en curso sobre {}", objetivo.nombre),
                true,
            );
            return;
        }
        // Con Podman nunca se usa sudo: sus contenedores son los del usuario.
        let es_contenedor = matches!(
            objetivo.tipo,
            TipoObjetivo::Contenedor | TipoObjetivo::Proyecto
        );
        if es_contenedor && motor.is_none() {
            self.mensaje(
                "sin permiso para el motor de contenedores: añade el usuario al grupo docker o pulsa ! para usar sudo",
                true,
            );
            return;
        }
        // H1: con Docker disponible y privilegio sin_sudo el acceso es directo
        // (grupo docker); solo se pide sudo si el listado lo usa.
        let usa_sudo_privilegiado = if es_contenedor {
            motor == Some(MotorNombre::Docker) && privilegio.usa_sudo()
        } else {
            true
        };
        if usa_sudo_privilegiado && !privilegio.puede() {
            self.mensaje(
                format!(
                    "usuario no puede usar sudo en {}",
                    self.servicios_host_nombre(host_id)
                ),
                true,
            );
            return;
        }
        let host_nombre = self.servicios_host_nombre(host_id);
        let pendiente = AccionPendiente {
            host_id,
            host_nombre,
            objetivo,
            accion,
        };
        let necesita_sudo = usa_sudo_privilegiado
            && privilegio.pide_contrasena()
            && self.servicios_sudo_accion(host_id).is_none();
        if necesita_sudo {
            self.pedir_sudo(host_id, Some(pendiente));
            return;
        }
        self.continuar_accion(pendiente);
    }

    /// Con la contraseña ya disponible: confirmación simple o deliberación.
    fn continuar_accion(&mut self, pendiente: AccionPendiente) {
        if acciones::exige_deliberacion(&pendiente.objetivo, pendiente.accion) {
            self.abrir_deliberacion_servicio(pendiente);
        } else {
            self.abrir_confirmacion_servicio(pendiente);
        }
    }

    /// Confirmación simple con lo que se comprobará antes y el aviso de
    /// unidad crítica.
    fn abrir_confirmacion_servicio(&mut self, pendiente: AccionPendiente) {
        let comprobacion =
            if acciones::comprueba_configuracion(&pendiente.objetivo, pendiente.accion) {
                self.servicios_comprobacion_texto(pendiente.host_id, &pendiente.objetivo)
            } else {
                None
            };
        let critica = self.servicios_critica(pendiente.host_id, &pendiente.objetivo);
        self.servicios.dialogo = Some(DialogoServicio::Confirmar(ConfirmacionServicio {
            pendiente,
            comprobacion,
            critica,
        }));
        self.sucio = true;
    }

    /// Manda la acción al servidor con su deliberación, si la hay.
    pub(super) fn ejecutar_accion_servicio(
        &mut self,
        pendiente: AccionPendiente,
        deliberacion: Option<DeliberacionLanzada>,
    ) {
        let sudo = self.sudo_para_accion(pendiente.host_id);
        let peticion_id = self.siguiente_peticion_servicios;
        self.siguiente_peticion_servicios += 1;
        self.peticiones_servicio.insert(
            peticion_id,
            PeticionServicio::Accion {
                host_id: pendiente.host_id,
                accion: pendiente.clone(),
            },
        );
        self.servidor.enviar(MensajeCliente::AccionServicio {
            peticion_id,
            host_id: pendiente.host_id,
            objetivo: pendiente.objetivo.clone(),
            accion: pendiente.accion,
            deliberacion,
            sudo,
        });
        self.sucio = true;
    }
}

impl App {
    /// Abre una shell en el contenedor seleccionado (S) como pestaña con
    /// orden: el servidor arma la orden, el cliente no manda texto (D248).
    fn shell_de_contenedor(&mut self, host_id: i64) {
        let Some(objetivo) = self.servicios_objetivo_seleccionado(host_id) else {
            return;
        };
        if objetivo.tipo != TipoObjetivo::Contenedor {
            self.mensaje("la shell solo vale para un contenedor", true);
            return;
        }
        let Some(listado_host) = self.servicios.listados.get(&host_id) else {
            return;
        };
        let Some(contenedor) = listado_host
            .listado
            .contenedores
            .iter()
            .find(|contenedor| contenedor.nombre == objetivo.nombre)
        else {
            return;
        };
        if !contenedor.admite_shell() {
            self.mensaje(
                format!(
                    "{} no está en marcha: no se puede abrir su shell",
                    contenedor.nombre
                ),
                true,
            );
            return;
        }
        let Some(motor) = self.servicios_motor(host_id) else {
            return;
        };
        let orden = OrdenPestana::ShellContenedor {
            motor,
            id: contenedor.id.clone(),
        };
        self.abrir_pestana_con_orden(host_id, orden, true);
    }

    /// Abre una pestaña con orden (seguir logs desde el panel o shell del
    /// contenedor). El servidor arma la orden; el cliente no manda texto.
    pub(super) fn abrir_pestana_con_orden(
        &mut self,
        host_id: i64,
        orden: OrdenPestana,
        activar: bool,
    ) {
        if let Some(motivo) = self.motivo_servidor_incompatible() {
            self.mensaje(motivo, true);
            return;
        }
        if self.servidor_caido {
            self.mensaje("el servidor de sesiones ha caído; relánzalo primero", true);
            return;
        }
        let comandos = self.comandos_al_conectar(host_id);
        if self.abrir_sesion_con_orden(host_id, comandos, Some(orden), activar) {
            let nombre = self.servicios_host_nombre(host_id);
            self.mensaje(format!("abriendo pestaña con orden en «{nombre}»…"), false);
        }
    }
}

impl App {
    /// t en el panel de logs: abre el seguimiento completo en una pestaña.
    fn abrir_pestana_de_logs(&mut self) {
        let Some(logs) = self.servicios.logs.as_ref() else {
            return;
        };
        let host_id = logs.host_id;
        let objetivo = logs.objetivo.clone();
        let Some(listado_host) = self.servicios.listados.get(&host_id) else {
            self.mensaje("hace falta el listado del host para abrir la pestaña", true);
            return;
        };
        let orden = match objetivo.tipo {
            TipoObjetivo::Unidad | TipoObjetivo::Timer => {
                let unidad = if objetivo.tipo == TipoObjetivo::Timer {
                    listado_host
                        .listado
                        .timers
                        .iter()
                        .find(|timer| timer.nombre == objetivo.nombre)
                        .and_then(|timer| timer.servicio_activado())
                        .unwrap_or_else(|| objetivo.nombre.clone())
                } else {
                    objetivo.nombre.clone()
                };
                OrdenPestana::SeguirDiario { unidad }
            }
            TipoObjetivo::Contenedor => {
                let Some(contenedor) = listado_host
                    .listado
                    .contenedores
                    .iter()
                    .find(|contenedor| contenedor.nombre == objetivo.nombre)
                else {
                    self.mensaje("ese contenedor ya no está en el listado", true);
                    return;
                };
                let Some(motor) = self.servicios_motor(host_id) else {
                    return;
                };
                OrdenPestana::SeguirContenedor {
                    motor,
                    id: contenedor.id.clone(),
                }
            }
            TipoObjetivo::Proyecto => {
                self.mensaje(
                    "un proyecto no tiene logs propios: elige un contenedor",
                    true,
                );
                return;
            }
        };
        // El panel sigue abierto: la pestaña se abre en segundo plano.
        self.abrir_pestana_con_orden(host_id, orden, false);
    }

    /// Abre el editor de la comprobación propia de un objetivo (c) o para
    /// crear una desde la ficha (sin objetivo).
    fn abrir_editor_comprobacion(&mut self, host_id: i64, objetivo: Option<Objetivo>) {
        let host_nombre = self.servicios_host_nombre(host_id);
        let (existente, comando) = match &objetivo {
            Some(objetivo) => match self.comprobacion_existente(host_id, objetivo) {
                Some((id, comando)) => (Some(id), comando),
                None => (None, String::new()),
            },
            None => (None, String::new()),
        };
        let objetivo_campo = objetivo
            .as_ref()
            .map(|objetivo| CampoTexto::nuevo(objetivo.nombre.clone()))
            .unwrap_or_default();
        self.servicios.dialogo = Some(DialogoServicio::Comprobacion(EditorComprobacion {
            host_id,
            host_nombre,
            objetivo: objetivo.clone(),
            objetivo_campo,
            comando: CampoTexto::nuevo(comando),
            foco: if objetivo.is_some() { 1 } else { 0 },
            existente,
            error: None,
        }));
        self.sucio = true;
    }
}

impl App {
    // ------------------------------------------------------------- diálogos

    /// Teclas de los diálogos propios de Servicios.
    pub(super) fn tecla_dialogo_servicio(&mut self, dialogo: DialogoServicio, tecla: KeyEvent) {
        match dialogo {
            DialogoServicio::Confirmar(confirmacion) => match tecla.code {
                KeyCode::Enter => {
                    self.ejecutar_accion_servicio(confirmacion.pendiente, None);
                }
                KeyCode::Esc | KeyCode::Char('q') => {}
                _ => {
                    self.servicios.dialogo = Some(DialogoServicio::Confirmar(confirmacion));
                }
            },
            DialogoServicio::Resultado(resultado) => {
                let accion = resultado.resultado.resultado;
                match tecla.code {
                    KeyCode::Esc | KeyCode::Char('q') => {}
                    KeyCode::Char('l') => {
                        let (host_id, objetivo) =
                            (resultado.host_id, resultado.pendiente.objetivo.clone());
                        self.abrir_logs_servicio(host_id, objetivo);
                    }
                    KeyCode::Enter => {
                        let (host_id, objetivo) =
                            (resultado.host_id, resultado.pendiente.objetivo.clone());
                        self.pedir_detalle(host_id, objetivo);
                    }
                    KeyCode::Char('c') if accion == ResultadoAccion::RechazadaConfiguracion => {
                        let (host_id, objetivo) =
                            (resultado.host_id, resultado.pendiente.objetivo.clone());
                        self.abrir_editor_comprobacion(host_id, Some(objetivo));
                    }
                    _ => {
                        self.servicios.dialogo = Some(DialogoServicio::Resultado(resultado));
                    }
                }
            }
            DialogoServicio::Sudo(mut pedido) => match tecla.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    if self.peticiones_servicio.values().any(|peticion| {
                        matches!(
                            peticion,
                            PeticionServicio::Sudo { host_id, .. } if *host_id == pedido.host_id
                        )
                    }) {
                        // Ya hay una comprobación en vuelo para ese host.
                        self.servicios.dialogo = Some(DialogoServicio::Sudo(pedido));
                        return;
                    }
                    if pedido.campo.texto.is_empty() {
                        pedido.error = Some("escribe la contraseña".to_string());
                        self.servicios.dialogo = Some(DialogoServicio::Sudo(pedido));
                        return;
                    }
                    let contrasena = Zeroizing::new(std::mem::take(&mut pedido.campo.texto));
                    pedido.campo.cursor = 0;
                    pedido.error = None;
                    let peticion_id = self.siguiente_peticion_servicios;
                    self.siguiente_peticion_servicios += 1;
                    self.peticiones_servicio.insert(
                        peticion_id,
                        PeticionServicio::Sudo {
                            host_id: pedido.host_id,
                            contrasena: contrasena.clone(),
                        },
                    );
                    self.servidor.enviar(MensajeCliente::ComprobarSudo {
                        peticion_id,
                        host_id: pedido.host_id,
                        contrasena: Secreto::nuevo(contrasena.as_str()),
                    });
                    self.servicios.dialogo = Some(DialogoServicio::Sudo(pedido));
                    self.sucio = true;
                }
                _ => {
                    pedido.campo.manejar_tecla(&tecla);
                    self.servicios.dialogo = Some(DialogoServicio::Sudo(pedido));
                    self.sucio = true;
                }
            },
            DialogoServicio::Comprobacion(mut editor) => match tecla.code {
                KeyCode::Esc => {}
                KeyCode::Tab | KeyCode::BackTab if editor.objetivo.is_none() => {
                    editor.foco = if editor.foco == 0 { 1 } else { 0 };
                    self.servicios.dialogo = Some(DialogoServicio::Comprobacion(editor));
                }
                KeyCode::Enter => self.guardar_comprobacion_editor(editor),
                KeyCode::Char('x') if editor.existente.is_some() => {
                    let id = editor.existente.unwrap_or_default();
                    let objetivo = if let Some(objetivo) = editor.objetivo.clone() {
                        objetivo
                    } else {
                        match objetivo_de_comprobacion(&editor.objetivo_campo.texto) {
                            Ok(objetivo) => objetivo,
                            Err(motivo) => {
                                editor.error = Some(motivo);
                                self.servicios.dialogo =
                                    Some(DialogoServicio::Comprobacion(editor));
                                return;
                            }
                        }
                    };
                    self.servicios.dialogo = Some(DialogoServicio::BorrarComprobacion {
                        host_id: editor.host_id,
                        host_nombre: editor.host_nombre.clone(),
                        objetivo,
                        id,
                    });
                }
                _ => {
                    let campo = if editor.objetivo.is_none() && editor.foco == 0 {
                        &mut editor.objetivo_campo
                    } else {
                        &mut editor.comando
                    };
                    campo.manejar_tecla(&tecla);
                    self.servicios.dialogo = Some(DialogoServicio::Comprobacion(editor));
                    self.sucio = true;
                }
            },
            DialogoServicio::BorrarComprobacion {
                host_id,
                host_nombre,
                objetivo,
                id,
            } => match tecla.code {
                KeyCode::Enter => {
                    self.borrar_comprobacion_servicio(host_id, &host_nombre, &objetivo, id);
                }
                KeyCode::Esc => {}
                _ => {
                    self.servicios.dialogo = Some(DialogoServicio::BorrarComprobacion {
                        host_id,
                        host_nombre,
                        objetivo,
                        id,
                    });
                }
            },
        }
    }

    /// Resuelve la comprobación de sudo: la guarda en memoria y sigue con la
    /// acción, o cuenta el intento.
    fn responder_sudo(
        &mut self,
        mut pedido: SudoPedido,
        resultado: ComprobacionSudo,
        contrasena: Zeroizing<String>,
    ) {
        match resultado {
            ComprobacionSudo::Ok => {
                let pendiente = pedido.pendiente.clone();
                let host_id = pedido.host_id;
                self.servicios.memoria_sudo.insert(
                    host_id,
                    MemoriaSudo {
                        contrasena,
                        ultimo_uso: Instant::now(),
                    },
                );
                self.mensaje(
                    format!("contraseña de sudo comprobada para {}", pedido.host_nombre),
                    false,
                );
                match pendiente {
                    Some(pendiente) => self.continuar_accion(pendiente),
                    None => self.refrescar_host_servicios(host_id),
                }
            }
            ComprobacionSudo::Rechazada => {
                pedido.intento = pedido.intento.saturating_add(1);
                if pedido.intento >= 3 {
                    self.mensaje("sudo rechazó la contraseña 3 veces", true);
                } else {
                    pedido.campo.limpiar();
                    pedido.error = Some(format!(
                        "contraseña rechazada (intento {} de 3)",
                        pedido.intento
                    ));
                    self.servicios.dialogo = Some(DialogoServicio::Sudo(pedido));
                }
            }
            ComprobacionSudo::SinSudo => {
                self.servicios.olvidar_sudo(pedido.host_id);
                self.mensaje(
                    format!("usuario no puede usar sudo en {}", pedido.host_nombre),
                    true,
                );
            }
        }
        self.sucio = true;
    }

    // ------------------------------------------- comprobaciones (almacén)

    /// Comprobación propia del objetivo, leída del almacén.
    fn comprobacion_propia(&self, host_id: i64, objetivo: &Objetivo) -> Option<String> {
        self.almacen
            .obtener_comprobacion(host_id, objetivo.tipo, &objetivo.nombre)
            .ok()
            .flatten()
            .map(|comprobacion| comprobacion.comando)
    }

    /// Comprobación existente, con su id, para editar o borrar.
    fn comprobacion_existente(&self, host_id: i64, objetivo: &Objetivo) -> Option<(i64, String)> {
        self.almacen
            .obtener_comprobacion(host_id, objetivo.tipo, &objetivo.nombre)
            .ok()
            .flatten()
            .map(|comprobacion| (comprobacion.id, comprobacion.comando))
    }

    /// Comprobaciones propias de un host, para el bloque de la ficha.
    pub fn servicios_comprobaciones_de_host(
        &self,
        host_id: i64,
    ) -> Vec<crate::modelo::ComprobacionServicio> {
        self.almacen
            .comprobaciones_de_host(host_id)
            .unwrap_or_default()
    }

    /// Fallos del último sondeo de un host, si los hay.
    pub fn servicios_fallos_de_host(&self, host_id: i64) -> Option<&FallosSondeo> {
        self.sondeos
            .get(&host_id)
            .and_then(|sondeo| sondeo.fallos.as_ref())
    }

    /// Número de fallos del último sondeo de un host (para la marca ⚠N).
    pub fn servicios_fallos_total(&self, host_id: i64) -> u32 {
        self.servicios_fallos_de_host(host_id)
            .map(FallosSondeo::total)
            .unwrap_or(0)
    }

    /// Valida y guarda lo que hay en el editor de comprobación propia.
    fn guardar_comprobacion_editor(&mut self, mut editor: EditorComprobacion) {
        let objetivo = match editor.objetivo.clone() {
            Some(objetivo) => objetivo,
            None => match objetivo_de_comprobacion(&editor.objetivo_campo.texto) {
                Ok(objetivo) => objetivo,
                Err(motivo) => {
                    editor.error = Some(motivo);
                    self.servicios.dialogo = Some(DialogoServicio::Comprobacion(editor));
                    return;
                }
            },
        };
        if let Err(motivo) = magi_nucleo::servicios::orden::validar_objetivo(&objetivo) {
            editor.error = Some(motivo);
            self.servicios.dialogo = Some(DialogoServicio::Comprobacion(editor));
            return;
        }
        if !validadores::aplica(objetivo.tipo) {
            editor.error = Some("solo vale para un servicio o un proyecto Compose".to_string());
            self.servicios.dialogo = Some(DialogoServicio::Comprobacion(editor));
            return;
        }
        if let Err(motivo) =
            magi_nucleo::servicios::orden::validar_comando_propio(&editor.comando.texto)
        {
            editor.error = Some(motivo);
            self.servicios.dialogo = Some(DialogoServicio::Comprobacion(editor));
            return;
        }
        // La primera comprobación propia del inventario sube el formato de la
        // sincronización a 3: los equipos con MAGI 1.2 dejan de leerlo (§4.7).
        let primera = self
            .almacen
            .comprobaciones_servicio()
            .map(|comprobaciones| comprobaciones.is_empty())
            .unwrap_or(false);
        match self.almacen.guardar_comprobacion(
            editor.host_id,
            objetivo.tipo,
            &objetivo.nombre,
            &editor.comando.texto,
        ) {
            Ok(_) => {
                let host_id = editor.host_id;
                if primera {
                    self.mensaje(
                        "comprobación guardada · los equipos con MAGI 1.2 dejarán de leer este equipo hasta que se actualicen",
                        false,
                    );
                } else {
                    self.mensaje("comprobación guardada", false);
                }
                self.servicios.dialogo = None;
                self.refrescar_comprobaciones_ficha(host_id);
            }
            Err(error) => {
                editor.error = Some(error.to_string());
                self.servicios.dialogo = Some(DialogoServicio::Comprobacion(editor));
            }
        }
        self.sucio = true;
    }

    /// Borra una comprobación propia (deja lápida; lo hace el almacén).
    fn borrar_comprobacion_servicio(
        &mut self,
        host_id: i64,
        host_nombre: &str,
        objetivo: &Objetivo,
        id: i64,
    ) {
        let _ = (host_nombre, objetivo);
        match self.almacen.borrar_comprobacion(id) {
            Ok(()) => {
                self.mensaje("comprobación borrada", false);
                self.refrescar_comprobaciones_ficha(host_id);
            }
            Err(error) => self.mensaje(error.to_string(), true),
        }
        self.servicios.dialogo = None;
        self.sucio = true;
    }
}

impl App {
    // --------------------------------------- comprobaciones desde la ficha

    /// «n» en el bloque de comprobaciones de la ficha: alta con objetivo
    /// escrito a mano.
    pub(super) fn ficha_comprobacion_nueva(&mut self) {
        let Some(host_id) = self.ficha.as_ref().and_then(|ficha| ficha.host_id) else {
            self.mensaje("guarda el host antes de darle comprobaciones", true);
            return;
        };
        self.abrir_editor_comprobacion(host_id, None);
    }

    /// «e» en el bloque: edita la comprobación seleccionada.
    pub(super) fn ficha_comprobacion_editar(&mut self) {
        let Some(host_id) = self.ficha.as_ref().and_then(|ficha| ficha.host_id) else {
            return;
        };
        let Some(objetivo) = self
            .ficha
            .as_ref()
            .and_then(|ficha| ficha.comprobacion_seleccionada())
            .map(|comprobacion| comprobacion.objetivo.clone())
        else {
            self.mensaje("no hay comprobación seleccionada", true);
            return;
        };
        self.abrir_editor_comprobacion(host_id, Some(objetivo));
    }

    /// «x» en el bloque: borra la comprobación seleccionada (con confirmación).
    pub(super) fn ficha_comprobacion_borrar(&mut self) {
        let Some(ficha) = self.ficha.as_ref() else {
            return;
        };
        let Some(comprobacion) = ficha.comprobacion_seleccionada() else {
            self.mensaje("no hay comprobación seleccionada", true);
            return;
        };
        let host_id = ficha.host_id.unwrap_or_default();
        let host_nombre = ficha.titulo.clone();
        let objetivo = comprobacion.objetivo.clone();
        let id = comprobacion.id;
        self.servicios.dialogo = Some(DialogoServicio::BorrarComprobacion {
            host_id,
            host_nombre,
            objetivo,
            id,
        });
        self.sucio = true;
    }

    /// Deja el índice del bloque de comprobaciones dentro de la lista.
    pub(super) fn ajustar_indice_comprobacion(&mut self) {
        if let Some(ficha) = &mut self.ficha {
            let total = ficha.comprobaciones.len();
            ficha.indice_comprobacion = ficha.indice_comprobacion_visible(total);
        }
    }

    /// Relee las comprobaciones del host en la ficha abierta (tras crear,
    /// editar o borrar).
    pub(super) fn refrescar_comprobaciones_ficha(&mut self, host_id: i64) {
        let Some(ficha) = self.ficha.as_mut() else {
            return;
        };
        if ficha.host_id != Some(host_id) {
            return;
        }
        ficha.comprobaciones = self
            .almacen
            .comprobaciones_de_host(host_id)
            .unwrap_or_default();
        ficha.indice_comprobacion = ficha.indice_comprobacion_visible(ficha.comprobaciones.len());
    }
}

/// Objetivo de una comprobación escrita a mano: una unidad o un proyecto.
fn objetivo_de_comprobacion(texto: &str) -> Result<Objetivo, String> {
    let texto = texto.trim();
    if texto.is_empty() {
        return Err("escribe la unidad o el proyecto".to_string());
    }
    if magi_nucleo::servicios::orden::nombre_unidad_valido(texto) {
        return Ok(Objetivo::unidad(texto));
    }
    if magi_nucleo::servicios::orden::nombre_proyecto_valido(texto) {
        return Ok(Objetivo::proyecto(texto));
    }
    Err("nombre no válido: una unidad .service o .timer, o un proyecto Compose".to_string())
}

impl App {
    // -------------------------------------------------------------- eventos

    /// Atiende los mensajes de la Fase 13. Devuelve si era uno de ellos (el
    /// resto sigue por el camino de siempre en app.rs).
    pub(super) fn evento_servicios(&mut self, mensaje: &MensajeServidor) -> bool {
        match mensaje {
            MensajeServidor::Servicios {
                peticion_id,
                host_id,
                listado,
            } => {
                let esperado = matches!(
                    self.peticiones_servicio.remove(peticion_id),
                    Some(PeticionServicio::Listar { host_id: otro }) if otro == *host_id
                );
                if esperado {
                    self.guardar_listado(*host_id, listado.clone());
                }
                true
            }
            MensajeServidor::Consumo {
                peticion_id,
                host_id,
                consumos,
            } => {
                let esperado = matches!(
                    self.peticiones_servicio.remove(peticion_id),
                    Some(PeticionServicio::Consumo { host_id: otro }) if otro == *host_id
                );
                if esperado {
                    self.servicios.consumos.insert(
                        *host_id,
                        ConsumoHost {
                            consumos: consumos.clone(),
                            recibido_epoch: chrono::Local::now().timestamp(),
                        },
                    );
                    self.sucio = true;
                }
                true
            }
            MensajeServidor::Detalle {
                peticion_id,
                host_id,
                detalle,
            } => {
                let objetivo = match self.peticiones_servicio.remove(peticion_id) {
                    Some(PeticionServicio::Detalle {
                        host_id: otro,
                        objetivo,
                    }) if otro == *host_id => Some(objetivo),
                    _ => None,
                };
                if let Some(objetivo) = objetivo {
                    let mut aplicado = false;
                    if let Some(abierto) = self.servicios.detalle.as_mut() {
                        if abierto.host_id == *host_id
                            && abierto.objetivo == objetivo
                            && abierto.peticion == Some(*peticion_id)
                        {
                            abierto.detalle = Some(detalle.clone());
                            abierto.peticion = None;
                            abierto.error = None;
                            aplicado = true;
                        }
                    }
                    if aplicado {
                        self.sucio = true;
                    }
                }
                true
            }
            MensajeServidor::SudoComprobado {
                peticion_id,
                resultado,
            } => {
                let Some(PeticionServicio::Sudo {
                    host_id,
                    contrasena,
                }) = self.peticiones_servicio.remove(peticion_id)
                else {
                    return true;
                };
                match self.servicios.dialogo.take() {
                    Some(DialogoServicio::Sudo(pedido)) => {
                        self.responder_sudo(pedido, *resultado, contrasena);
                    }
                    otro => {
                        // El diálogo se cerró mientras llegaba la respuesta: si
                        // la contraseña valía, se recuerda igualmente.
                        self.servicios.dialogo = otro;
                        if *resultado == ComprobacionSudo::Ok {
                            self.servicios.memoria_sudo.insert(
                                host_id,
                                MemoriaSudo {
                                    contrasena,
                                    ultimo_uso: Instant::now(),
                                },
                            );
                        }
                    }
                }
                true
            }
            MensajeServidor::AccionServicioHecha {
                peticion_id,
                resultado,
            } => {
                let Some(PeticionServicio::Accion { host_id, accion }) =
                    self.peticiones_servicio.remove(peticion_id)
                else {
                    return true;
                };
                if self.vista == Vista::Servicios {
                    self.servicios.dialogo = Some(DialogoServicio::Resultado(ResultadoDialogo {
                        host_id,
                        host_nombre: accion.host_nombre.clone(),
                        pendiente: accion.clone(),
                        resultado: resultado.clone(),
                    }));
                    self.sucio = true;
                } else {
                    self.mensaje(
                        format!(
                            "{} {} en {}: {}",
                            accion.accion.participio(),
                            accion.objetivo.nombre,
                            accion.host_nombre,
                            resultado.resultado.texto()
                        ),
                        !resultado.resultado.es_ok(),
                    );
                }
                self.refrescar_host_servicios(host_id);
                true
            }
            MensajeServidor::AccionesServicio { lista } => {
                self.servicios.acciones = lista.clone();
                self.sucio = true;
                true
            }
            MensajeServidor::LineasLog {
                peticion_id,
                lineas,
                omitidas,
            } => {
                if matches!(
                    self.peticiones_servicio.get(peticion_id),
                    Some(PeticionServicio::Log { .. })
                ) {
                    self.anadir_lineas_log(*peticion_id, lineas.clone(), *omitidas);
                }
                true
            }
            MensajeServidor::FinLog {
                peticion_id,
                motivo,
            } => {
                if let Some(PeticionServicio::Log { .. }) =
                    self.peticiones_servicio.remove(peticion_id)
                {
                    if let Some(logs) = self.servicios.logs.as_mut() {
                        if logs.peticion_id == *peticion_id {
                            logs.abriendo = false;
                            logs.motivo_fin = Some(*motivo);
                        }
                    }
                    self.sucio = true;
                }
                true
            }
            MensajeServidor::Error {
                mensaje,
                peticion_id: Some(peticion_id),
            } => {
                // Solo los errores de peticiones de servicios son de esta
                // vista; los de otros subsistemas siguen su camino (T33).
                if !self.peticiones_servicio.contains_key(peticion_id) {
                    return false;
                }
                self.servicios_manejar_error(*peticion_id, mensaje);
                true
            }
            _ => false,
        }
    }

    /// Un error con peticion_id: se aplica a lo que hubiera en vuelo.
    fn servicios_manejar_error(&mut self, peticion_id: u64, mensaje: &str) -> bool {
        let Some(peticion) = self.peticiones_servicio.remove(&peticion_id) else {
            // Un error de una petición que ya no está en vuelo se descarta.
            return true;
        };
        match peticion {
            PeticionServicio::Listar { host_id } => {
                // Se conserva el último listado con su hora y se marca
                // desconectado: «desconectado · r reconectar» más el mensaje.
                if let Some(listado) = self.servicios.listados.get_mut(&host_id) {
                    listado.estado = EstadoListado::Desconectado;
                    listado.aviso = Some(mensaje.to_string());
                }
                self.mensaje(mensaje.to_string(), true);
            }
            PeticionServicio::Consumo { host_id: _ } => {
                self.mensaje(mensaje.to_string(), true);
            }
            PeticionServicio::Detalle { host_id, objetivo } => {
                if let Some(abierto) = self.servicios.detalle.as_mut() {
                    if abierto.host_id == host_id && abierto.objetivo == objetivo {
                        abierto.peticion = None;
                        abierto.error = Some(mensaje.to_string());
                    }
                }
            }
            PeticionServicio::Sudo { host_id, .. } => {
                if let Some(DialogoServicio::Sudo(mut pedido)) = self.servicios.dialogo.take() {
                    let _ = host_id;
                    pedido.intento = pedido.intento.saturating_add(1);
                    if pedido.intento >= 3 {
                        self.mensaje(mensaje.to_string(), true);
                    } else {
                        pedido.campo.limpiar();
                        pedido.error = Some(mensaje.to_string());
                        self.servicios.dialogo = Some(DialogoServicio::Sudo(pedido));
                    }
                } else {
                    self.mensaje(mensaje.to_string(), true);
                }
            }
            PeticionServicio::Accion { host_id, accion } => {
                if self.vista == Vista::Servicios {
                    self.servicios.dialogo = Some(DialogoServicio::Resultado(ResultadoDialogo {
                        host_id,
                        host_nombre: accion.host_nombre.clone(),
                        pendiente: accion,
                        resultado: ResultadoServicio {
                            resultado: ResultadoAccion::Error,
                            mensaje: mensaje.to_string(),
                            ..ResultadoServicio::default()
                        },
                    }));
                } else {
                    self.mensaje(mensaje.to_string(), true);
                }
                self.refrescar_host_servicios(host_id);
            }
            PeticionServicio::Log { .. } => {
                if let Some(logs) = self.servicios.logs.as_mut() {
                    logs.abriendo = false;
                    logs.motivo_fin = Some(MotivoFinLog::ConexionCaida);
                }
                self.mensaje(mensaje.to_string(), true);
            }
        }
        true
    }

    /// Guarda un listado y resuelve lo que quedaba pendiente para ese host.
    fn guardar_listado(&mut self, host_id: i64, listado: Listado) {
        self.servicios.listados.insert(
            host_id,
            ListadoHost {
                listado,
                recibido_epoch: chrono::Local::now().timestamp(),
                recibido: Instant::now(),
                estado: EstadoListado::Ok,
                aviso: None,
            },
        );
        self.servicios.servidor_caido = false;
        self.sucio = true;
        if self.servicios_host_seleccionado() != Some(host_id) {
            return;
        }
        if let Some((seccion, nombre)) = self.servicios.elemento_pendiente.take() {
            self.servicios.seccion = seccion;
            if let Some(indice) = self
                .servicios_elementos(host_id)
                .iter()
                .position(|elemento| elemento.nombre() == nombre)
            {
                self.servicios.seleccion.insert((host_id, seccion), indice);
            }
            self.servicios.foco = Foco::Lista;
        }
        if let Some(objetivo) = self.servicios.logs_pendientes.take() {
            self.abrir_logs_servicio(host_id, objetivo);
        }
        if self.servicios_pide_consumo(host_id) {
            self.pedir_consumo(host_id);
        }
        if let Some((objetivo, accion)) = self.servicios.accion_pendiente.take() {
            self.iniciar_accion_servicio(host_id, objetivo, accion);
        }
    }
}
