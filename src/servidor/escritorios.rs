//! Custodia de las sesiones de escritorio remoto del servidor de sesiones
//! (Fase 12): RDP, VNC y voxl.
//!
//! Una sesión de escritorio vive aquí, como las de terminal: sobrevive a
//! cerrar la ventana de MAGI, se difunde entera (Escritorios) y se reconecta
//! sin perder la pestaña. El servidor no dialoga ni toca el llavero: el
//! certificado RDP y las credenciales se piden al cliente solicitante por el
//! protocolo (T25).
//!
//! El camino hasta el escritorio usa el pool de conexiones SSH
//! (conexion_para_canal con uso Escritorio) y un canal direct-tcpip, o un TCP
//! directo; en las dos formas con plazo de 10 s. La decodificación corre en un
//! hilo propio por sesión (lanzar_en_hilo): el runtime de red solo mueve
//! bytes.
//!
//! El servidor HTTP/WS del visor (pista visor-http) se arranca con la primera
//! sesión y se para con la última. Aquí se le registra cada sesión y se
//! consume su AvisoVisor para actualizar el estado, la gracia y la
//! reconexión.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use russh::client::Handle;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use magi_nucleo::escritorio::visor_http::{AvisoVisor, DatosVoxl, ErrorVisor, RegistroVisor};
use magi_nucleo::escritorio::voxl::{self, HostSsh};
use magi_nucleo::escritorio::{
    self, Conexion, ConfigEscritorio, CredencialesEscritorio, DatosCertificado, DialogosEscritorio,
    ErrorEscritorio, EstadoEscritorio, EventoEscritorio, FaseEscritorio, ImagenEscritorio,
    InfoEscritorio, OrdenEscritorio, TipoHost,
};

use crate::conexion::cliente::Cliente;
use crate::config::Rutas;
use crate::modelo::{Host, ResultadoRegistro};
use crate::protocolo::{
    MensajeServidor, PeticionCredenciales, RANGO_ESCRITORIOS, RANGO_ESCRITORIOS_SERVIDOR,
};
use crate::registro;

use super::conexiones::{self, ConexionTomada, PoliticaConexion, UsoCanal};
use super::{difusion, EstadoServidor, OrdenBd};

/// Plazo de red del camino hasta el escritorio: TCP, saludo y apertura del
/// canal direct-tcpip. Nunca se espera a la red sin plazo.
pub const PLAZO_CAMINO: Duration = Duration::from_secs(10);

/// Plazo de un diálogo con el usuario (certificado, credenciales). El usuario
/// puede tardar; el resto de esperas no.
pub const PLAZO_DIALOGO: Duration = Duration::from_secs(5 * 60);

/// Intentos de credenciales de una apertura: 1 y, tras un rechazo de
/// autenticación, 2 y 3. Cada intento reabre el camino porque RFB y RDP
/// cierran la conexión al rechazar las credenciales.
pub const INTENTOS_CREDENCIALES: u8 = 3;

/// Tamaño inicial de un escritorio que se ajusta a la ventana, hasta que el
/// cliente RDP o el servidor VNC anuncien el suyo.
const TAMANO_INICIAL: (u16, u16) = (1920, 1080);

/// Base del id con el que viajan los diálogos SSH del host de «a través de»
/// (huella, frase y contraseña): es un u32 en el protocolo y no puede chocar
/// con los ids de las sesiones de terminal.
const BASE_DIALOGO_SSH: u32 = 1 << 30;

/// Cada cuánto se revisan la gracia, la conexión del salto y las sesiones voxl.
const PASO_REVISION: Duration = Duration::from_secs(1);

/// Estado compartido del servidor, para no repetir el tipo en cada firma.
type EstadoCompartido = Arc<tokio::sync::Mutex<EstadoServidor>>;

// ---------------------------------------------------------------- estado

/// Sesiones de escritorio que custodia el servidor, con sus diálogos en vuelo
/// y el registro del visor.
pub struct Escritorios {
    sesiones: HashMap<u32, Escritorio>,
    siguiente_id: u32,
    /// Diálogos de escritorio en vuelo, por (cliente solicitante, peticion_id
    /// de la apertura). El cliente va en la clave porque cada ventana numera
    /// sus peticiones por su cuenta (dos ventanas pueden usar el mismo id).
    pendientes: HashMap<(u32, u64), PendienteDialogo>,
    /// Registro del servidor HTTP/WS del visor y los avisos que manda.
    registro: Arc<RegistroVisor>,
    /// Avisos del visor: se toman una sola vez al arrancar el servidor.
    avisos: Option<mpsc::Receiver<AvisoVisor>>,
    /// Cerrojo del arranque del servidor del visor (la primera sesión lo
    /// arranca; dos aperturas a la vez no pueden pelearse).
    cerrojo_visor: Arc<tokio::sync::Mutex<()>>,
    /// Siguiente peticion_id que emite el propio servidor para una
    /// reconexión pedida desde el visor o para rehacer sin AVC420
    /// (rango [2^47, 2^48), distinto del de los clientes).
    siguiente_peticion_servidor: u64,
}

impl Escritorios {
    pub fn nuevo() -> Self {
        let (registro, avisos) = RegistroVisor::nuevo();
        Self {
            sesiones: HashMap::new(),
            siguiente_id: 1,
            pendientes: HashMap::new(),
            registro,
            avisos: Some(avisos),
            cerrojo_visor: Arc::new(tokio::sync::Mutex::new(())),
            siguiente_peticion_servidor: RANGO_ESCRITORIOS_SERVIDOR,
        }
    }

    /// Un peticion_id nuevo del rango del servidor ([2^47, 2^48)) para una
    /// reconexión que no viene de una petición de un cliente.
    fn nueva_peticion_servidor(&mut self) -> u64 {
        let peticion = self.siguiente_peticion_servidor;
        self.siguiente_peticion_servidor += 1;
        if self.siguiente_peticion_servidor >= (RANGO_ESCRITORIOS_SERVIDOR << 1) {
            self.siguiente_peticion_servidor = RANGO_ESCRITORIOS_SERVIDOR;
        }
        peticion
    }

    /// Se lleva el receptor de avisos del visor: solo se toma una vez, al
    /// arrancar el servidor de sesiones.
    pub fn tomar_avisos(&mut self) -> Option<mpsc::Receiver<AvisoVisor>> {
        self.avisos.take()
    }

    /// Sesiones custodiadas (para TrabajoServidor.escritorios).
    pub fn cuenta(&self) -> u32 {
        self.sesiones.len() as u32
    }

    /// Lista completa que se difunde y que recibe una ventana nueva.
    pub fn lista(&self) -> Vec<InfoEscritorio> {
        let mut lista = self.registro.lista();
        lista.sort_by_key(|info| info.id);
        lista
    }

    fn actualizar_info(&self, id: u32) {
        if let Some(info) = self.sesiones.get(&id).map(Escritorio::info) {
            self.registro.actualizar_info(info);
        }
    }

    fn actualizar_info_de_todas(&self) {
        for id in self.sesiones.keys().copied().collect::<Vec<_>>() {
            self.actualizar_info(id);
        }
    }

    fn anular_dialogos(&mut self, cliente_id: u32, peticion_id: u64) {
        self.pendientes.remove(&(cliente_id, peticion_id));
    }

    /// Respuesta del cliente al diálogo de certificado. Solo vale del
    /// solicitante y con un diálogo en vuelo.
    pub fn resolver_huella(&mut self, cliente_id: u32, peticion_id: u64, aceptada: bool) {
        let Some(pendiente) = self.pendientes.remove(&(cliente_id, peticion_id)) else {
            warn!(
                peticion = peticion_id,
                cliente = cliente_id,
                "respuesta de huella sin diálogo pendiente"
            );
            return;
        };
        match pendiente {
            PendienteDialogo::Huella { responder, .. } => {
                let _ = responder.send(aceptada);
            }
            otro => {
                self.pendientes.insert((cliente_id, peticion_id), otro);
                warn!(
                    peticion = peticion_id,
                    "la respuesta de huella no casa con el diálogo pendiente"
                );
            }
        }
    }

    /// Respuesta del cliente al diálogo de credenciales.
    pub fn resolver_credenciales(
        &mut self,
        cliente_id: u32,
        peticion_id: u64,
        credenciales: CredencialesEscritorio,
    ) {
        let Some(pendiente) = self.pendientes.remove(&(cliente_id, peticion_id)) else {
            warn!(
                peticion = peticion_id,
                cliente = cliente_id,
                "respuesta de credenciales sin diálogo pendiente"
            );
            return;
        };
        match pendiente {
            PendienteDialogo::Credenciales { responder, .. } => {
                let _ = responder.send(Some(credenciales));
            }
            otro => {
                self.pendientes.insert((cliente_id, peticion_id), otro);
                warn!(
                    peticion = peticion_id,
                    "la respuesta de credenciales no casa con el diálogo pendiente"
                );
            }
        }
    }

    /// Prepara el apagado: cierra las sesiones, anota y devuelve lo que hay
    /// que parar por la red (voxl y sus conexiones del pool). El servidor del
    /// visor se para aquí mismo: no habla con la red.
    pub fn preparar_apagado(&mut self, bd: &std::sync::mpsc::Sender<OrdenBd>) -> RecursosApagado {
        let ids: Vec<u32> = self.sesiones.keys().copied().collect();
        let mut sesiones = Vec::new();
        for id in ids {
            let Some(sesion) = self.sesiones.remove(&id) else {
                continue;
            };
            sesion.cancelar.cancel();
            let (tipo, detalle, resultado) = if sesion.estado == EstadoEscritorio::Abriendo {
                (
                    registro::ESCRITORIO_FALLIDO,
                    "el servidor se apaga".to_string(),
                    ResultadoRegistro::Error,
                )
            } else {
                let duracion = chrono::Utc::now().timestamp() - sesion.desde;
                (
                    registro::ESCRITORIO_CERRADO,
                    format!("{duracion} s · el servidor se apaga"),
                    ResultadoRegistro::Ok,
                )
            };
            let _ = bd.send(OrdenBd::Anotar {
                tipo: tipo.to_string(),
                host_id: Some(sesion.host_id),
                identidad_id: None,
                detalle,
                resultado,
            });
            self.registro.baja(id);
            sesiones.push(sesion);
        }
        self.pendientes.clear();
        self.registro.parar();
        RecursosApagado { sesiones }
    }
}

/// Lo que queda por cerrar por la red al apagar: las sesiones voxl y sus
/// conexiones del pool.
pub struct RecursosApagado {
    pub sesiones: Vec<Escritorio>,
}

/// Cierra con plazo lo que el apagado dejó pendiente (voxl y las conexiones
/// tomadas por identidad). Nunca cuelga el apagado.
pub async fn cerrar_recursos(recursos: RecursosApagado) {
    for sesion in recursos.sesiones {
        sesion.cancelar.cancel();
        if let Some(mut voxl) = sesion.voxl {
            if tokio::time::timeout(PLAZO_CAMINO, voxl.parar())
                .await
                .is_err()
            {
                warn!(
                    escritorio = sesion.id,
                    "voxl no se paró a tiempo al apagar el servidor"
                );
            }
        }
        if let Some(tomada) = sesion.voxl_tomada {
            tomada.soltar().await;
        }
    }
}

/// Una sesión de escritorio custodiada por el servidor (estado en memoria; no
/// hay tabla).
pub struct Escritorio {
    pub id: u32,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    pub tipo: TipoHost,
    pub estado: EstadoEscritorio,
    pub fase: Option<FaseEscritorio>,
    pub motivo: Option<String>,
    /// Ventana que pidió abrir o reconectar; los diálogos van a ella.
    pub solicitante: u32,
    pub peticion_id: u64,
    /// Época en segundos de la apertura, para la duración del REGISTRO.
    pub desde: i64,
    pub tamano: (u16, u16),
    pub imagen: Option<ImagenEscritorio>,
    pub fps: u16,
    pub directo: bool,
    pub a_traves_de: Option<String>,
    pub redimensionable: bool,
    pub visor: bool,
    pub sin_visor_desde: Option<i64>,
    /// La apertura que corre es una reconexión (falla a caído, no se quita).
    pub reconectando: bool,
    /// Responde al solicitante con el enlace al quedar abierta. Una
    /// reconexión pedida desde el visor no tiene petición que contestar.
    pub responder_al_solicitante: bool,
    /// Generación de la apertura en curso: las tareas viejas no tocan el
    /// estado de una sesión que ya se reconectó o se cerró.
    pub generacion: u64,
    /// Ofrecer AVC420 a Windows en esta sesión (false tras SinH264).
    pub h264: bool,
    /// Cancela la apertura en curso (cierre, apagado, ventana que se va).
    pub cancelar: tokio_util::sync::CancellationToken,
    /// Conexión del pool que sostiene el canal, para detectar su caída.
    pub conexion_salto: Option<Arc<Handle<Cliente>>>,
    /// Sesión de voxl viva (el túnel y el script del host).
    pub voxl: Option<voxl::SesionVoxl>,
    /// Conexión del pool que sostiene el canal de voxl.
    pub voxl_tomada: Option<ConexionTomada>,
}

impl Escritorio {
    /// Resumen para el registro del visor y para la difusión Escritorios. El
    /// ancho, el alto, la imagen y los fps reales los conserva el puente.
    pub fn info(&self) -> InfoEscritorio {
        let (ancho, alto) = self.tamano;
        InfoEscritorio {
            id: self.id,
            host_id: self.host_id,
            nombre: self.nombre.clone(),
            tipo: self.tipo,
            estado: self.estado,
            fase: self.fase,
            motivo: self.motivo.clone(),
            visor: self.visor,
            sin_visor_desde: self.sin_visor_desde,
            ancho,
            alto,
            imagen: self.imagen,
            fps: self.fps,
            directo: self.directo,
            a_traves_de: self.a_traves_de.clone(),
            desde: self.desde,
            redimensionable: self.redimensionable,
        }
    }
}

/// Un diálogo de escritorio esperando la respuesta de un cliente.
enum PendienteDialogo {
    Huella {
        solicitante: u32,
        responder: oneshot::Sender<bool>,
    },
    Credenciales {
        solicitante: u32,
        responder: oneshot::Sender<Option<CredencialesEscritorio>>,
    },
}

impl PendienteDialogo {
    fn solicitante(&self) -> u32 {
        match self {
            PendienteDialogo::Huella { solicitante, .. }
            | PendienteDialogo::Credenciales { solicitante, .. } => *solicitante,
        }
    }
}

// ---------------------------------------------------------------- difusión

/// Difunde la lista completa de escritorios a todos los clientes (T27). La
/// lista sale del registro del visor, que es quien mantiene los fps y el
/// tamaño al día.
pub fn difundir_lista(estado: &mut EstadoServidor) {
    let lista = estado.escritorios.lista();
    difusion::difundir(&estado.clientes, MensajeServidor::Escritorios { lista });
}

// ---------------------------------------------------------------- apertura

/// Datos fijos de una apertura (o reconexión). Se leen una vez y no se
/// releen al confirmar un diálogo.
struct DatosSesion {
    escritorio_id: u32,
    host: Host,
    saltos: HashMap<i64, Host>,
    rutas: Rutas,
    solicitante: u32,
    peticion_id: u64,
    generacion: u64,
    reconectando: bool,
    cancelar: tokio_util::sync::CancellationToken,
    h264: bool,
    /// Distribución efectiva de la configuración global (auto ya resuelta):
    /// el host puede imponer la suya y se resuelve al construir la conexión.
    distribucion_auto: String,
    /// Canales de la primera fuente ya registrados con el visor.
    primer_eventos: Option<mpsc::Sender<EventoEscritorio>>,
    primer_ordenes: Option<mpsc::Receiver<OrdenEscritorio>>,
}

/// AbrirEscritorio: valida la fila del host, arranca el visor si hace falta y
/// lanza la sesión en estado «abriendo».
pub async fn abrir(estado: &EstadoCompartido, cliente_id: u32, peticion_id: u64, host_id: i64) {
    if let Err(motivo) = comprobar_peticion(peticion_id) {
        super::responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let (host, saltos, rutas) = leer_hosts(estado, host_id).await;
    let Some(host) = host else {
        super::responder_error(
            estado,
            cliente_id,
            "el host ya no existe: actualiza la lista".to_string(),
            Some(peticion_id),
        )
        .await;
        return;
    };
    if let Err(motivo) = validar(&host, &saltos) {
        super::responder_error(
            estado,
            cliente_id,
            format!("edita el host: {motivo}"),
            Some(peticion_id),
        )
        .await;
        return;
    }
    let existente = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .sesiones
            .values()
            .find(|sesion| sesion.host_id == host_id)
            .map(|sesion| sesion.id)
    };
    if let Some(id) = existente {
        super::responder_error(
            estado,
            cliente_id,
            format!(
                "«{}» ya tiene una sesión de escritorio (id {id}): ve a su pestaña",
                host.nombre
            ),
            Some(peticion_id),
        )
        .await;
        return;
    }
    if estado.lock().await.apagando {
        super::responder_error(
            estado,
            cliente_id,
            conexiones::APAGANDO.to_string(),
            Some(peticion_id),
        )
        .await;
        return;
    }
    if let Err(motivo) = asegurar_visor(estado).await {
        super::responder_error(
            estado,
            cliente_id,
            format!("no se pudo arrancar el servidor del visor: {motivo}"),
            Some(peticion_id),
        )
        .await;
        return;
    }

    let (h264, distribucion_auto) = {
        let estado_bloqueado = estado.lock().await;
        (
            estado_bloqueado.config.escritorio.h264,
            estado_bloqueado.config.escritorio.distribucion_efectiva(),
        )
    };
    let (sesion_id, sesion, generacion) = {
        let mut estado_bloqueado = estado.lock().await;
        if estado_bloqueado.apagando {
            // El apagado empezó entre la comprobación anterior y esta: la
            // petición no puede quedarse sin respuesta (M8).
            drop(estado_bloqueado);
            super::responder_error(
                estado,
                cliente_id,
                conexiones::APAGANDO.to_string(),
                Some(peticion_id),
            )
            .await;
            return;
        }
        let id = estado_bloqueado.escritorios.siguiente_id;
        estado_bloqueado.escritorios.siguiente_id += 1;
        let sesion = nueva_sesion(id, &host, &saltos, cliente_id, peticion_id, h264);
        (id, sesion, 1)
    };
    // Las sesiones RDP y VNC se dan de alta en el registro del visor con su
    // par de canales; voxl no tiene puente (el navegador va a su página).
    let mut primer_eventos = None;
    let mut primer_ordenes = None;
    if host.tipo != TipoHost::Voxl {
        let (tx_eventos, rx_eventos) = mpsc::channel::<EventoEscritorio>(64);
        let (tx_ordenes, rx_ordenes) = mpsc::channel::<OrdenEscritorio>(16);
        estado.lock().await.escritorios.registro.alta(
            sesion.info(),
            sesion_portapapeles(&host),
            rx_eventos,
            tx_ordenes,
        );
        primer_eventos = Some(tx_eventos);
        primer_ordenes = Some(rx_ordenes);
    }
    let duplicada = {
        let mut estado_bloqueado = estado.lock().await;
        // Otra apertura simultánea del mismo host pudo ganar la carrera: la
        // primera sesión se queda y esta se descarta sin dejar canales y con
        // su Error, para que la ventana perdedora no se quede «abriendo» (M8).
        if estado_bloqueado
            .escritorios
            .sesiones
            .values()
            .any(|existente| existente.host_id == host_id)
        {
            estado_bloqueado.escritorios.registro.baja(sesion_id);
            true
        } else {
            estado_bloqueado
                .escritorios
                .sesiones
                .insert(sesion_id, sesion);
            estado_bloqueado.vacio_desde = None;
            difundir_lista(&mut estado_bloqueado);
            false
        }
    };
    if duplicada {
        sincronizar_visor(estado).await;
        super::responder_error(
            estado,
            cliente_id,
            format!("«{}» ya tiene una sesión de escritorio", host.nombre),
            Some(peticion_id),
        )
        .await;
        return;
    }
    let datos = DatosSesion {
        escritorio_id: sesion_id,
        host,
        saltos,
        rutas,
        solicitante: cliente_id,
        peticion_id,
        generacion,
        reconectando: false,
        cancelar: cancelar_de(estado, sesion_id).await,
        h264,
        distribucion_auto,
        primer_eventos,
        primer_ordenes,
    };
    tokio::spawn(tarea_apertura(estado.clone(), datos));
}

/// ReconectarEscritorio: vuelve a abrir una sesión caída conservando su id.
pub async fn reconectar(
    estado: &EstadoCompartido,
    cliente_id: u32,
    peticion_id: u64,
    escritorio_id: u32,
) {
    if let Err(motivo) = comprobar_peticion(peticion_id) {
        super::responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let Some((host_id, estado_actual)) = ({
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .sesiones
            .get(&escritorio_id)
            .map(|sesion| (sesion.host_id, sesion.estado))
    }) else {
        super::responder_error(
            estado,
            cliente_id,
            "el escritorio ya no existe".to_string(),
            Some(peticion_id),
        )
        .await;
        return;
    };
    if estado_actual != EstadoEscritorio::Caido {
        super::responder_error(
            estado,
            cliente_id,
            "el escritorio no está caído: no hace falta reconectar".to_string(),
            Some(peticion_id),
        )
        .await;
        return;
    }
    let (host, saltos, _) = leer_hosts(estado, host_id).await;
    let Some(host) = host else {
        super::responder_error(
            estado,
            cliente_id,
            "el host ya no existe: la sesión no se puede reconectar".to_string(),
            Some(peticion_id),
        )
        .await;
        return;
    };
    if let Err(motivo) = validar(&host, &saltos) {
        super::responder_error(
            estado,
            cliente_id,
            format!("edita el host: {motivo}"),
            Some(peticion_id),
        )
        .await;
        return;
    }
    if let Err(motivo) = lanzar_reconexion(
        estado,
        escritorio_id,
        cliente_id,
        peticion_id,
        true,
        &host,
        saltos,
    )
    .await
    {
        super::responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
    }
}

/// Prepara y lanza una reconexión de una sesión caída (o abierta que debe
/// rehacerse, como al quitar AVC420). Devuelve el motivo del fallo inmediato.
async fn lanzar_reconexion(
    estado: &EstadoCompartido,
    escritorio_id: u32,
    cliente_id: u32,
    peticion_id: u64,
    responder_al_solicitante: bool,
    host: &Host,
    saltos: HashMap<i64, Host>,
) -> Result<(), String> {
    let rutas = estado.lock().await.rutas.clone();
    if let Err(motivo) = asegurar_visor(estado).await {
        return Err(format!(
            "no se pudo arrancar el servidor del visor: {motivo}"
        ));
    }
    let (h264, distribucion_auto) = {
        let estado_bloqueado = estado.lock().await;
        let h264 = estado_bloqueado
            .escritorios
            .sesiones
            .get(&escritorio_id)
            .map(|sesion| sesion.h264)
            .unwrap_or(estado_bloqueado.config.escritorio.h264);
        (
            h264,
            estado_bloqueado.config.escritorio.distribucion_efectiva(),
        )
    };
    // voxl no tiene puente ni WebSocket propios: se para la sesión anterior
    // (el script y su túnel) y se arranca otra.
    if host.tipo == TipoHost::Voxl {
        let voxl_anterior: Option<voxl::SesionVoxl>;
        let tomada_anterior: Option<ConexionTomada>;
        let generacion = {
            let mut estado_bloqueado = estado.lock().await;
            let Some(sesion) = estado_bloqueado
                .escritorios
                .sesiones
                .get_mut(&escritorio_id)
            else {
                return Err("el escritorio ya no existe".to_string());
            };
            if sesion.estado != EstadoEscritorio::Caido {
                return Err("el escritorio no está caído: no hace falta reconectar".to_string());
            }
            voxl_anterior = sesion.voxl.take();
            tomada_anterior = sesion.voxl_tomada.take();
            sesion.generacion += 1;
            sesion.estado = EstadoEscritorio::Abriendo;
            sesion.fase = Some(FaseEscritorio::Conectando);
            sesion.motivo = Some("abriendo".to_string());
            sesion.solicitante = cliente_id;
            sesion.peticion_id = peticion_id;
            sesion.reconectando = true;
            sesion.responder_al_solicitante = responder_al_solicitante;
            sesion.sin_visor_desde = None;
            sesion.cancelar = tokio_util::sync::CancellationToken::new();
            sesion.conexion_salto = None;
            let generacion = sesion.generacion;
            estado_bloqueado.escritorios.actualizar_info(escritorio_id);
            difundir_lista(&mut estado_bloqueado);
            generacion
        };
        if let Some(mut voxl) = voxl_anterior {
            if tokio::time::timeout(PLAZO_CAMINO, voxl.parar())
                .await
                .is_err()
            {
                warn!(
                    escritorio = escritorio_id,
                    "voxl no se paró a tiempo al reconectar el escritorio"
                );
            }
        }
        if let Some(tomada) = tomada_anterior {
            tomada.soltar().await;
        }
        let cancelar = cancelar_de(estado, escritorio_id).await;
        let datos = DatosSesion {
            escritorio_id,
            host: host.clone(),
            saltos,
            rutas,
            solicitante: cliente_id,
            peticion_id,
            generacion,
            reconectando: true,
            cancelar,
            h264,
            distribucion_auto,
            primer_eventos: None,
            primer_ordenes: None,
        };
        tokio::spawn(tarea_apertura(estado.clone(), datos));
        return Ok(());
    }
    let (tx_eventos, rx_eventos) = mpsc::channel::<EventoEscritorio>(64);
    let (tx_ordenes, rx_ordenes) = mpsc::channel::<OrdenEscritorio>(16);
    // La renovación de la fuente del visor manda por un canal acotado: se
    // hace fuera del cerrojo del estado para no retenerlo.
    let visor = {
        let estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.escritorios.sesiones.get(&escritorio_id) else {
            return Err("el escritorio ya no existe".to_string());
        };
        if !matches!(
            sesion.estado,
            EstadoEscritorio::Caido | EstadoEscritorio::Abierto
        ) {
            return Err("el escritorio no está caído: no hace falta reconectar".to_string());
        }
        estado_bloqueado.escritorios.registro.sesion(escritorio_id)
    };
    let Some(visor) = visor else {
        return Err("el servidor del visor no tiene la sesión".to_string());
    };
    // La renovación manda por un canal acotado del puente: con plazo, para no
    // quedarse esperando a un puente atascado.
    let renovado =
        tokio::time::timeout(PLAZO_CAMINO, visor.renovar_fuente(rx_eventos, tx_ordenes)).await;
    if renovado != Ok(true) {
        return Err("el servidor del visor no tiene la sesión".to_string());
    }
    let generacion = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get_mut(&escritorio_id)
        else {
            return Err("el escritorio ya no existe".to_string());
        };
        if !matches!(
            sesion.estado,
            EstadoEscritorio::Caido | EstadoEscritorio::Abierto
        ) {
            return Err("el escritorio no está caído: no hace falta reconectar".to_string());
        }
        sesion.generacion += 1;
        sesion.estado = EstadoEscritorio::Abriendo;
        sesion.fase = Some(FaseEscritorio::Conectando);
        sesion.motivo = Some("abriendo".to_string());
        sesion.solicitante = cliente_id;
        sesion.peticion_id = peticion_id;
        sesion.reconectando = true;
        sesion.responder_al_solicitante = responder_al_solicitante;
        sesion.sin_visor_desde = None;
        sesion.cancelar = tokio_util::sync::CancellationToken::new();
        sesion.conexion_salto = None;
        let generacion = sesion.generacion;
        estado_bloqueado.escritorios.actualizar_info(escritorio_id);
        difundir_lista(&mut estado_bloqueado);
        generacion
    };
    let cancelar = cancelar_de(estado, escritorio_id).await;
    let datos = DatosSesion {
        escritorio_id,
        host: host.clone(),
        saltos,
        rutas,
        solicitante: cliente_id,
        peticion_id,
        generacion,
        reconectando: true,
        cancelar,
        h264,
        distribucion_auto,
        primer_eventos: Some(tx_eventos),
        primer_ordenes: Some(rx_ordenes),
    };
    tokio::spawn(tarea_apertura(estado.clone(), datos));
    Ok(())
}

/// CerrarEscritorio: cierra una sesión abierta o caída. Si llega mientras la
/// sesión está «abriendo», es que el usuario canceló un diálogo: se aborta la
/// apertura y se anota escritorio_fallido, no escritorio_cerrado.
pub async fn cerrar(estado: &EstadoCompartido, _cliente_id: u32, escritorio_id: u32) {
    let accion = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .sesiones
            .get(&escritorio_id)
            .map(|sesion| (sesion.estado, sesion.peticion_id, sesion.solicitante))
    };
    let Some((estado_actual, peticion_id, solicitante)) = accion else {
        return;
    };
    if estado_actual == EstadoEscritorio::Abriendo {
        abortar_apertura(
            estado,
            escritorio_id,
            peticion_id,
            solicitante,
            "cancelado desde la ventana",
        )
        .await;
        return;
    }
    cerrar_sesion(estado, escritorio_id, "cerrado por el usuario").await;
}

/// PedirEnlace: enlace de un solo uso nuevo para reabrir el visor o copiarlo.
pub async fn pedir_enlace(
    estado: &EstadoCompartido,
    cliente_id: u32,
    peticion_id: u64,
    escritorio_id: u32,
) {
    if let Err(motivo) = comprobar_peticion(peticion_id) {
        super::responder_error(estado, cliente_id, motivo, Some(peticion_id)).await;
        return;
    }
    let enlace = {
        let estado_bloqueado = estado.lock().await;
        if estado_bloqueado
            .escritorios
            .sesiones
            .contains_key(&escritorio_id)
        {
            estado_bloqueado.escritorios.registro.enlace(escritorio_id)
        } else {
            None
        }
    };
    match enlace {
        Some(enlace) => {
            super::responder(
                estado,
                cliente_id,
                MensajeServidor::Enlace {
                    peticion_id,
                    escritorio_id,
                    enlace,
                },
            )
            .await;
        }
        None => {
            super::responder_error(
                estado,
                cliente_id,
                "el escritorio ya no existe".to_string(),
                Some(peticion_id),
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------- tarea

/// Tarea de una apertura: abre el camino, lanza el cliente de escritorio en su
/// hilo y reintenta las credenciales hasta tres veces.
async fn tarea_apertura(estado: EstadoCompartido, mut datos: DatosSesion) {
    if datos.host.tipo == TipoHost::Voxl {
        tarea_voxl(estado, datos).await;
        return;
    }
    let mut intento: u8 = 1;
    loop {
        if !vigente(&estado, &datos).await {
            return;
        }
        // La huella del certificado se relee en cada intento: si el cliente
        // acaba de aceptarla, ya está guardada en HOSTS.huella_escritorio y el
        // reintento no debe volver a pedir el mismo certificado.
        if let Ok(huella) = huella_actual(&estado, datos.host.id).await {
            datos.host.huella_escritorio = huella;
        }
        let camino = tokio::select! {
            _ = datos.cancelar.cancelled() => return,
            resultado = abrir_camino(&estado, &datos) => match resultado {
                Ok(camino) => camino,
                Err(motivo) => {
                    fallar_si_vigente(&estado, &datos, motivo).await;
                    return;
                }
            },
        };
        let CaminoAbierto { flujo, tomada } = camino;
        if !vigente(&estado, &datos).await {
            soltar_tomada(tomada).await;
            return;
        }
        {
            let mut estado_bloqueado = estado.lock().await;
            let Some(sesion) = estado_bloqueado
                .escritorios
                .sesiones
                .get_mut(&datos.escritorio_id)
            else {
                drop(estado_bloqueado);
                soltar_tomada(tomada).await;
                return;
            };
            if sesion.generacion != datos.generacion {
                drop(estado_bloqueado);
                soltar_tomada(tomada).await;
                return;
            }
            sesion.fase = Some(FaseEscritorio::Autenticando);
            sesion.conexion_salto = tomada.as_ref().map(|tomada| tomada.handle.clone());
            estado_bloqueado
                .escritorios
                .actualizar_info(datos.escritorio_id);
            difundir_lista(&mut estado_bloqueado);
        }

        // La primera fuente usa los canales creados al dar de alta la sesión;
        // los reintentos renuevan la fuente del visor.
        let (eventos, ordenes) = match (datos.primer_eventos.take(), datos.primer_ordenes.take()) {
            (Some(eventos), Some(ordenes)) => (eventos, ordenes),
            _ => match renovar_fuente(&estado, &datos).await {
                Some(canales) => canales,
                None => {
                    soltar_tomada(tomada).await;
                    fallar_si_vigente(
                        &estado,
                        &datos,
                        "el servidor del visor perdió la sesión: ábrelo otra vez".to_string(),
                    )
                    .await;
                    return;
                }
            },
        };

        let config = config_escritorio(&datos.host, datos.h264, &datos.distribucion_auto);
        let dialogo: escritorio::Dialogo = Arc::new(DialogosSolicitante {
            estado: estado.clone(),
            escritorio_id: datos.escritorio_id,
            peticion_id: datos.peticion_id,
            solicitante: datos.solicitante,
            host_id: datos.host.id,
            host_nombre: datos.host.nombre.clone(),
            generacion: datos.generacion,
        });
        let (tx_fin, rx_fin) = oneshot::channel();
        let conexion = Conexion {
            flujo,
            config,
            dialogo,
            eventos,
            ordenes,
            intento,
        };
        let tipo = datos.host.tipo;
        let nombre_hilo = format!("escritorio-{}", datos.escritorio_id);
        let lanzado = escritorio::lanzar_en_hilo_con(nombre_hilo, move || async move {
            let resultado = match tipo {
                TipoHost::Rdp => escritorio::rdp::abrir(conexion).await,
                TipoHost::Vnc => escritorio::rfb::abrir(conexion).await,
                _ => Err(ErrorEscritorio::Protocolo(
                    "tipo de escritorio no admitido".to_string(),
                )),
            };
            let _ = tx_fin.send(resultado);
        });
        if let Err(error) = lanzado {
            soltar_tomada(tomada).await;
            fallar_si_vigente(
                &estado,
                &datos,
                format!("no se pudo lanzar la sesión del escritorio: {error}"),
            )
            .await;
            return;
        }

        let desenlace = tokio::select! {
            _ = datos.cancelar.cancelled() => Desenlace::Cancelado,
            resultado = rx_fin => Desenlace::Terminado(resultado.unwrap_or_else(|_| {
                Err(ErrorEscritorio::Remoto(
                    "la sesión del escritorio terminó".to_string(),
                ))
            })),
        };
        soltar_tomada(tomada).await;
        match desenlace {
            Desenlace::Cancelado => return,
            Desenlace::Terminado(resultado) => match resultado {
                Err(ErrorEscritorio::Autenticacion(motivo)) if intento < INTENTOS_CREDENCIALES => {
                    intento += 1;
                    info!(
                        escritorio = datos.escritorio_id,
                        intento, motivo, "credenciales rechazadas: se reintenta"
                    );
                    {
                        let mut estado_bloqueado = estado.lock().await;
                        let Some(sesion) = estado_bloqueado
                            .escritorios
                            .sesiones
                            .get_mut(&datos.escritorio_id)
                        else {
                            return;
                        };
                        if sesion.generacion != datos.generacion
                            || sesion.estado != EstadoEscritorio::Abriendo
                        {
                            return;
                        }
                        sesion.fase = Some(FaseEscritorio::Autenticando);
                        sesion.motivo = Some("usuario o contraseña incorrectos".to_string());
                        estado_bloqueado
                            .escritorios
                            .actualizar_info(datos.escritorio_id);
                        difundir_lista(&mut estado_bloqueado);
                    }
                }
                Err(error) => {
                    fallar_si_vigente(&estado, &datos, error.motivo().to_string()).await;
                    return;
                }
                Ok(()) => {
                    // Terminó sin error mientras abría: el remoto cerró la
                    // sesión antes de estar lista. Si ya había quedado
                    // abierta, la caída la ha marcado el aviso del puente.
                    let motivo = {
                        let estado_bloqueado = estado.lock().await;
                        estado_bloqueado
                            .escritorios
                            .sesiones
                            .get(&datos.escritorio_id)
                            .filter(|sesion| sesion.generacion == datos.generacion)
                            .map(|sesion| (sesion.estado, sesion.motivo.clone()))
                    };
                    if let Some((EstadoEscritorio::Abriendo, motivo)) = motivo {
                        fallar_si_vigente(
                            &estado,
                            &datos,
                            motivo.unwrap_or_else(|| "el remoto cerró la sesión".to_string()),
                        )
                        .await;
                    }
                    return;
                }
            },
        }
    }
}

async fn soltar_tomada(tomada: Option<ConexionTomada>) {
    if let Some(tomada) = tomada {
        tomada.soltar().await;
    }
}

enum Desenlace {
    Terminado(Result<(), ErrorEscritorio>),
    Cancelado,
}

/// Abre el camino hasta el escritorio: TCP directo o canal direct-tcpip por el
/// host SSH de «a través de». En las dos formas, con plazo de 10 s.
async fn abrir_camino(
    estado: &EstadoCompartido,
    datos: &DatosSesion,
) -> Result<CaminoAbierto, String> {
    let host = &datos.host;
    let Some(salto_id) = host.salto_host_id else {
        let intento = tokio::time::timeout(
            PLAZO_CAMINO,
            tokio::net::TcpStream::connect((host.direccion.as_str(), host.puerto)),
        )
        .await;
        return match intento {
            Ok(Ok(flujo)) => {
                let _ = flujo.set_nodelay(true);
                Ok(CaminoAbierto {
                    flujo: Box::new(flujo),
                    tomada: None,
                })
            }
            Ok(Err(error)) => Err(format!(
                "no responde {}:{}: {error}",
                host.direccion, host.puerto
            )),
            Err(_) => Err(format!(
                "no responde {}:{} ({} s)",
                host.direccion,
                host.puerto,
                PLAZO_CAMINO.as_secs()
            )),
        };
    };
    let Some(salto) = datos.saltos.get(&salto_id).cloned() else {
        return Err("el host «a través de» ya no existe".to_string());
    };
    let tomada = conexiones::conexion_para_canal(
        estado,
        &salto,
        &datos.saltos,
        &datos.rutas,
        PoliticaConexion::Interactiva {
            solicitante: datos.solicitante,
            id_solicitud: id_dialogo_ssh(datos.escritorio_id),
        },
        UsoCanal::Escritorio,
    )
    .await
    .map_err(|motivo| format!("no se llega a «{}»: {motivo}", salto.nombre))?;
    let handle = tomada.handle.clone();
    let direccion = host.direccion.clone();
    let puerto = host.puerto;
    let apertura = conexiones::abrir_con_plazo(PLAZO_CAMINO, async move {
        handle
            .channel_open_direct_tcpip(direccion, u32::from(puerto), "127.0.0.1".to_string(), 0)
            .await
    })
    .await;
    match apertura {
        Ok(canal) => Ok(CaminoAbierto {
            flujo: Box::new(canal.into_stream()),
            tomada: Some(tomada),
        }),
        Err(error) => {
            // T34: en todas las ramas de error se suelta la conexión tomada,
            // por identidad.
            let tomada: ConexionTomada = tomada;
            tomada.soltar().await;
            Err(format!("no se llega a «{}»: {error}", salto.nombre))
        }
    }
}

/// El camino abierto hasta el escritorio, con la conexión del pool que lo
/// sostiene (si va por SSH).
struct CaminoAbierto {
    flujo: escritorio::Flujo,
    tomada: Option<ConexionTomada>,
}

/// ¿Sigue la sesión existiendo con esta generación y en «abriendo»?
async fn vigente(estado: &EstadoCompartido, datos: &DatosSesion) -> bool {
    let estado_bloqueado = estado.lock().await;
    estado_bloqueado
        .escritorios
        .sesiones
        .get(&datos.escritorio_id)
        .is_some_and(|sesion| {
            sesion.generacion == datos.generacion && sesion.estado == EstadoEscritorio::Abriendo
        })
}

async fn cancelar_de(estado: &EstadoCompartido, id: u32) -> tokio_util::sync::CancellationToken {
    estado
        .lock()
        .await
        .escritorios
        .sesiones
        .get(&id)
        .map(|sesion| sesion.cancelar.clone())
        .unwrap_or_default()
}

/// Crea una fuente nueva (eventos y órdenes) para una sesión ya registrada en
/// el visor. Devuelve lo que va a la Conexion.
async fn renovar_fuente(
    estado: &EstadoCompartido,
    datos: &DatosSesion,
) -> Option<(
    mpsc::Sender<EventoEscritorio>,
    mpsc::Receiver<OrdenEscritorio>,
)> {
    let (tx_eventos, rx_eventos) = mpsc::channel::<EventoEscritorio>(64);
    let (tx_ordenes, rx_ordenes) = mpsc::channel::<OrdenEscritorio>(16);
    let visor = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .registro
            .sesion(datos.escritorio_id)
    }?;
    match tokio::time::timeout(PLAZO_CAMINO, visor.renovar_fuente(rx_eventos, tx_ordenes)).await {
        Ok(true) => Some((tx_eventos, rx_ordenes)),
        _ => None,
    }
}

/// Marca la sesión como fallida: la quita (o la deja caída si era una
/// reconexión), anota escritorio_fallido y contesta al solicitante.
async fn fallar_si_vigente(estado: &EstadoCompartido, datos: &DatosSesion, motivo: String) {
    let (baja, visor, respondiente) = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .anular_dialogos(datos.solicitante, datos.peticion_id);
        let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get(&datos.escritorio_id)
        else {
            return;
        };
        if sesion.generacion != datos.generacion || sesion.estado != EstadoEscritorio::Abriendo {
            return;
        }
        let visor = estado_bloqueado
            .escritorios
            .registro
            .sesion(datos.escritorio_id);
        let respondiente = (
            sesion.responder_al_solicitante,
            sesion.solicitante,
            sesion.peticion_id,
        );
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(OrdenBd::Anotar {
            tipo: registro::ESCRITORIO_FALLIDO.to_string(),
            host_id: Some(datos.host.id),
            identidad_id: None,
            detalle: motivo.clone(),
            resultado: ResultadoRegistro::Error,
        });
        let baja = if datos.reconectando {
            if let Some(sesion) = estado_bloqueado
                .escritorios
                .sesiones
                .get_mut(&datos.escritorio_id)
            {
                sesion.estado = EstadoEscritorio::Caido;
                sesion.fase = None;
                sesion.motivo = Some(motivo.clone());
                sesion.sin_visor_desde = None;
                sesion.conexion_salto = None;
            }
            estado_bloqueado
                .escritorios
                .actualizar_info(datos.escritorio_id);
            false
        } else {
            estado_bloqueado
                .escritorios
                .sesiones
                .remove(&datos.escritorio_id);
            true
        };
        difundir_lista(&mut estado_bloqueado);
        (baja, visor, respondiente)
    };
    if let Some(visor) = visor {
        visor.estado_visor("caido", Some(&motivo));
    }
    if baja {
        let registro = estado.lock().await.escritorios.registro.clone();
        registro.baja(datos.escritorio_id);
        sincronizar_visor(estado).await;
        difundir_despues_de_baja(estado).await;
    }
    if respondiente.0 {
        super::responder_error(estado, respondiente.1, motivo, Some(respondiente.2)).await;
    }
}

/// Cancela una apertura que el usuario ha cancelado desde una ventana: se
/// anota como fallida, se quita la sesión y se contesta Error a la petición.
async fn abortar_apertura(
    estado: &EstadoCompartido,
    escritorio_id: u32,
    peticion_id: u64,
    solicitante: u32,
    motivo: &str,
) {
    let visor = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .anular_dialogos(solicitante, peticion_id);
        let Some(sesion) = estado_bloqueado.escritorios.sesiones.remove(&escritorio_id) else {
            return;
        };
        sesion.cancelar.cancel();
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(OrdenBd::Anotar {
            tipo: registro::ESCRITORIO_FALLIDO.to_string(),
            host_id: Some(sesion.host_id),
            identidad_id: None,
            detalle: motivo.to_string(),
            resultado: ResultadoRegistro::Error,
        });
        let visor = estado_bloqueado.escritorios.registro.sesion(escritorio_id);
        difundir_lista(&mut estado_bloqueado);
        visor
    };
    if let Some(visor) = visor {
        visor.estado_visor("cerrado", Some("apertura cancelada"));
    }
    let registro = estado.lock().await.escritorios.registro.clone();
    registro.baja(escritorio_id);
    sincronizar_visor(estado).await;
    difundir_despues_de_baja(estado).await;
    super::responder_error(
        estado,
        solicitante,
        format!("apertura cancelada: {motivo}"),
        Some(peticion_id),
    )
    .await;
}

/// Cierra una sesión ya abierta (o caída) con su anotación y su aviso al
/// visor. Difunde «cerrando» antes del efecto lento.
async fn cerrar_sesion(estado: &EstadoCompartido, escritorio_id: u32, motivo: &str) {
    let (sesion, visor) = {
        let mut estado_bloqueado = estado.lock().await;
        let Some((estado_actual, peticion_id, solicitante)) = estado_bloqueado
            .escritorios
            .sesiones
            .get(&escritorio_id)
            .map(|sesion| (sesion.estado, sesion.peticion_id, sesion.solicitante))
        else {
            return;
        };
        if estado_actual == EstadoEscritorio::Cerrando {
            return;
        }
        estado_bloqueado
            .escritorios
            .anular_dialogos(solicitante, peticion_id);
        if let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get_mut(&escritorio_id)
        {
            sesion.estado = EstadoEscritorio::Cerrando;
            sesion.fase = None;
            sesion.motivo = Some(motivo.to_string());
            sesion.cancelar.cancel();
        }
        estado_bloqueado.escritorios.actualizar_info(escritorio_id);
        difundir_lista(&mut estado_bloqueado);
        let Some(sesion) = estado_bloqueado.escritorios.sesiones.remove(&escritorio_id) else {
            return;
        };
        let bd = estado_bloqueado.bd.clone();
        let duracion = chrono::Utc::now().timestamp() - sesion.desde;
        let _ = bd.send(OrdenBd::Anotar {
            tipo: registro::ESCRITORIO_CERRADO.to_string(),
            host_id: Some(sesion.host_id),
            identidad_id: None,
            detalle: format!("{duracion} s · {motivo}"),
            resultado: ResultadoRegistro::Ok,
        });
        let visor = estado_bloqueado.escritorios.registro.sesion(escritorio_id);
        (sesion, visor)
    };
    if let Some(visor) = visor {
        visor.estado_visor("cerrado", Some(motivo));
    }
    let registro = estado.lock().await.escritorios.registro.clone();
    registro.baja(escritorio_id);
    sincronizar_visor(estado).await;
    difundir_despues_de_baja(estado).await;
    if let Some(mut voxl) = sesion.voxl {
        if tokio::time::timeout(PLAZO_CAMINO, voxl.parar())
            .await
            .is_err()
        {
            warn!(
                escritorio = escritorio_id,
                "voxl no se paró a tiempo al cerrar el escritorio"
            );
        }
    }
    if let Some(tomada) = sesion.voxl_tomada {
        tomada.soltar().await;
    }
}

// ---------------------------------------------------------------- avisos del visor

/// Bucle de avisos del servidor del visor: es el puente quien ve los eventos
/// de la sesión (rectángulos, caída, visor conectado) y aquí solo se toma el
/// estado de la sesión.
pub async fn bucle_avisos(estado: EstadoCompartido, mut avisos: mpsc::Receiver<AvisoVisor>) {
    while let Some(aviso) = avisos.recv().await {
        match aviso {
            AvisoVisor::Abierto { id } => marcar_abierto(&estado, id).await,
            AvisoVisor::Caido { id, motivo } => {
                marcar_aviso_caido(&estado, id, &motivo, false).await;
            }
            AvisoVisor::Cerrado { id, motivo } => {
                marcar_aviso_caido(&estado, id, &motivo, true).await;
            }
            AvisoVisor::VisorConectado { id, h264: _ } => {
                marcar_visor(&estado, id, true).await;
            }
            AvisoVisor::VisorDesconectado { id } => {
                marcar_visor(&estado, id, false).await;
            }
            AvisoVisor::SinH264 { id } => reconectar_sin_h264(&estado, id).await,
            AvisoVisor::Reconectar { id } => reconectar_desde_visor(&estado, id).await,
            AvisoVisor::Visible { .. } => {}
        }
    }
}

/// La sesión ha quedado abierta: se anota, se arranca la gracia y se contesta
/// al solicitante con el enlace del visor.
async fn marcar_abierto(estado: &EstadoCompartido, escritorio_id: u32) {
    struct AvisoAbierto {
        responder: bool,
        solicitante: u32,
        peticion_id: u64,
        host_id: i64,
        host_nombre: String,
        tipo: TipoHost,
        a_traves_de: Option<String>,
        imagen: Option<ImagenEscritorio>,
    }
    // El enlace se obtiene antes de dar la sesión por abierta: si el servidor
    // del visor no está en marcha (una baja pudo pararlo por carrera con esta
    // apertura), se vuelve a arrancar y, si aun así no hay enlace, la apertura
    // falla con Error en vez de quedarse «abriendo» para siempre (C1/M8).
    let responder = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .sesiones
            .get(&escritorio_id)
            .map(|sesion| sesion.responder_al_solicitante)
            .unwrap_or(false)
    };
    let enlace = if responder {
        let mut enlace = enlace_de(estado, escritorio_id).await;
        if enlace.is_none() {
            let _ = asegurar_visor(estado).await;
            enlace = enlace_de(estado, escritorio_id).await;
        }
        match enlace {
            Some(enlace) => Some(enlace),
            None => {
                fallar_abierto(
                    estado,
                    escritorio_id,
                    "no se pudo abrir el visor del escritorio: el servidor del visor no está en marcha"
                        .to_string(),
                )
                .await;
                return;
            }
        }
    } else {
        None
    };
    let aviso = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get_mut(&escritorio_id)
        else {
            return;
        };
        if sesion.estado != EstadoEscritorio::Abriendo {
            return;
        }
        sesion.estado = EstadoEscritorio::Abierto;
        sesion.fase = None;
        sesion.motivo = None;
        sesion.desde = chrono::Utc::now().timestamp();
        if !sesion.visor {
            sesion.sin_visor_desde = Some(sesion.desde);
        }
        let aviso = AvisoAbierto {
            responder: sesion.responder_al_solicitante,
            solicitante: sesion.solicitante,
            peticion_id: sesion.peticion_id,
            host_id: sesion.host_id,
            host_nombre: sesion.host_nombre.clone(),
            tipo: sesion.tipo,
            a_traves_de: sesion.a_traves_de.clone(),
            imagen: sesion.imagen,
        };
        let bd = estado_bloqueado.bd.clone();
        let camino = match &aviso.a_traves_de {
            Some(salto) => format!("a través de {salto}"),
            None => "directo".to_string(),
        };
        let imagen = aviso
            .imagen
            .map(|imagen| imagen.etiqueta())
            .unwrap_or("sin imagen");
        let _ = bd.send(OrdenBd::Anotar {
            tipo: registro::ESCRITORIO_ABIERTO.to_string(),
            host_id: Some(aviso.host_id),
            identidad_id: None,
            detalle: format!("{} · {camino} · imagen {imagen}", aviso.tipo.etiqueta()),
            resultado: ResultadoRegistro::Ok,
        });
        estado_bloqueado.escritorios.actualizar_info(escritorio_id);
        difundir_lista(&mut estado_bloqueado);
        aviso
    };
    if aviso.responder {
        if let Some(enlace) = enlace {
            super::responder(
                estado,
                aviso.solicitante,
                MensajeServidor::EscritorioAbierto {
                    peticion_id: aviso.peticion_id,
                    escritorio_id,
                    enlace,
                },
            )
            .await;
        }
    }
    info!(
        escritorio = escritorio_id,
        host = %aviso.host_nombre,
        tipo = %aviso.tipo,
        "escritorio abierto"
    );
}

/// Llega del puente: la sesión ha caído o el remoto la cerró. Si aún estaba
/// abriendo, se apunta el motivo y la tarea de apertura decide (puede ser un
/// fallo de credenciales que se reintenta); si estaba abierta, queda caída.
async fn marcar_aviso_caido(
    estado: &EstadoCompartido,
    escritorio_id: u32,
    motivo: &str,
    era_cierre_remoto: bool,
) {
    let voxl: Option<voxl::SesionVoxl>;
    let tomada: Option<ConexionTomada>;
    {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get_mut(&escritorio_id)
        else {
            return;
        };
        match sesion.estado {
            EstadoEscritorio::Abriendo => {
                // La apertura sigue en su tarea: el motivo se guarda para que
                // el fallo (o el reintento) lo use.
                sesion.motivo = Some(motivo.to_string());
                return;
            }
            EstadoEscritorio::Abierto | EstadoEscritorio::Caido => {
                sesion.estado = EstadoEscritorio::Caido;
                sesion.fase = None;
                sesion.motivo = Some(if era_cierre_remoto {
                    format!("el remoto cerró la sesión: {motivo}")
                } else {
                    motivo.to_string()
                });
                sesion.sin_visor_desde = None;
                sesion.conexion_salto = None;
                voxl = sesion.voxl.take();
                tomada = sesion.voxl_tomada.take();
            }
            EstadoEscritorio::Cerrando => return,
        }
        estado_bloqueado.escritorios.actualizar_info(escritorio_id);
        difundir_lista(&mut estado_bloqueado);
    }
    // El visor enseña «caído, con Reconectar» (o «cerrado» si fue el remoto).
    let visor = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado.escritorios.registro.sesion(escritorio_id)
    };
    if let Some(visor) = visor {
        if era_cierre_remoto {
            visor.estado_visor("cerrado", Some(motivo));
        } else {
            visor.estado_visor("caido", Some(motivo));
        }
    }
    if let Some(mut voxl) = voxl {
        if tokio::time::timeout(PLAZO_CAMINO, voxl.parar())
            .await
            .is_err()
        {
            warn!(
                escritorio = escritorio_id,
                "voxl no se paró a tiempo al caer el escritorio"
            );
        }
    }
    if let Some(tomada) = tomada {
        tomada.soltar().await;
    }
    info!(escritorio = escritorio_id, motivo, "escritorio caído");
}

/// El visor se ha conectado o se ha ido: la gracia empieza o se cancela.
async fn marcar_visor(estado: &EstadoCompartido, escritorio_id: u32, conectado: bool) {
    let mut estado_bloqueado = estado.lock().await;
    let Some(sesion) = estado_bloqueado
        .escritorios
        .sesiones
        .get_mut(&escritorio_id)
    else {
        return;
    };
    if sesion.visor == conectado {
        return;
    }
    sesion.visor = conectado;
    sesion.sin_visor_desde = if conectado {
        None
    } else {
        Some(chrono::Utc::now().timestamp())
    };
    estado_bloqueado.escritorios.actualizar_info(escritorio_id);
    difundir_lista(&mut estado_bloqueado);
}

/// El navegador no decodifica H.264: se rehace la sesión sin AVC420 (Windows
/// conserva su sesión y MAGI manda rectángulos).
async fn reconectar_sin_h264(estado: &EstadoCompartido, escritorio_id: u32) {
    let (host_id, solicitante) = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get_mut(&escritorio_id)
        else {
            return;
        };
        sesion.h264 = false;
        (sesion.host_id, sesion.solicitante)
    };
    let cliente = {
        let estado_bloqueado = estado.lock().await;
        if estado_bloqueado.clientes.contains_key(&solicitante) {
            Some(solicitante)
        } else {
            estado_bloqueado.clientes.keys().min().copied()
        }
    };
    let Some(cliente) = cliente else {
        let visor = {
            let estado_bloqueado = estado.lock().await;
            estado_bloqueado.escritorios.registro.sesion(escritorio_id)
        };
        if let Some(visor) = visor {
            visor.estado_visor(
                "caido",
                Some("abre MAGI para reconectar sin H.264: no hay ninguna ventana conectada"),
            );
        }
        return;
    };
    reconectar_con_cliente(estado, escritorio_id, host_id, Some(cliente), false).await;
}

/// Orden «rec» del visor: si hay una ventana de MAGI conectada se le piden las
/// credenciales y se reconecta; si no, el visor enseña «abre MAGI para
/// reconectar».
async fn reconectar_desde_visor(estado: &EstadoCompartido, escritorio_id: u32) {
    let (host_id, solicitante, caido) = {
        let estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.escritorios.sesiones.get(&escritorio_id) else {
            return;
        };
        (
            sesion.host_id,
            sesion.solicitante,
            sesion.estado == EstadoEscritorio::Caido,
        )
    };
    if !caido {
        return;
    }
    let cliente = {
        let estado_bloqueado = estado.lock().await;
        if estado_bloqueado.clientes.contains_key(&solicitante) {
            Some(solicitante)
        } else {
            estado_bloqueado.clientes.keys().min().copied()
        }
    };
    let Some(cliente) = cliente else {
        let visor = {
            let estado_bloqueado = estado.lock().await;
            estado_bloqueado.escritorios.registro.sesion(escritorio_id)
        };
        if let Some(visor) = visor {
            visor.estado_visor(
                "caido",
                Some("abre MAGI para reconectar: no hay ninguna ventana conectada"),
            );
        }
        return;
    };
    reconectar_con_cliente(estado, escritorio_id, host_id, Some(cliente), false).await;
}

/// Rehace una sesión sin petición de un cliente (la orden viene del visor o
/// del puente). No contesta con EscritorioAbierto: la ventana ve el cambio por
/// la difusión Escritorios.
async fn reconectar_con_cliente(
    estado: &EstadoCompartido,
    escritorio_id: u32,
    host_id: i64,
    cliente: Option<u32>,
    responder: bool,
) {
    let Some(cliente) = cliente else {
        return;
    };
    let (host, saltos, _) = leer_hosts(estado, host_id).await;
    let Some(host) = host else {
        return;
    };
    if validar(&host, &saltos).is_err() {
        return;
    }
    // Una reconexión sin petición de un cliente usa un peticion_id nuevo del
    // rango del servidor: nunca se reutiliza uno que ya no está en vuelo.
    let peticion_id = {
        let mut estado_bloqueado = estado.lock().await;
        let _ = responder;
        estado_bloqueado.escritorios.nueva_peticion_servidor()
    };
    let _ = lanzar_reconexion(
        estado,
        escritorio_id,
        cliente,
        peticion_id,
        responder,
        &host,
        saltos,
    )
    .await;
}

// ---------------------------------------------------------------- voxl

/// Tarea de una apertura voxl: conexión por SSH, arranque del script embebido,
/// /healthz, túnel de la sesión y aviso al visor.
async fn tarea_voxl(estado: EstadoCompartido, datos: DatosSesion) {
    let Some(salto_id) = datos.host.salto_host_id else {
        fallar_si_vigente(
            &estado,
            &datos,
            "voxl necesita un host SSH por el que abrirse".to_string(),
        )
        .await;
        return;
    };
    let Some(salto) = datos.saltos.get(&salto_id).cloned() else {
        fallar_si_vigente(
            &estado,
            &datos,
            "el host «a través de» ya no existe".to_string(),
        )
        .await;
        return;
    };
    {
        let mut estado_bloqueado = estado.lock().await;
        if let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get_mut(&datos.escritorio_id)
        {
            sesion.fase = Some(FaseEscritorio::Arrancando);
        }
        estado_bloqueado
            .escritorios
            .actualizar_info(datos.escritorio_id);
        difundir_lista(&mut estado_bloqueado);
    }
    let tomada = tokio::select! {
        _ = datos.cancelar.cancelled() => return,
        resultado = conexiones::conexion_para_canal(
            &estado,
            &salto,
            &datos.saltos,
            &datos.rutas,
            PoliticaConexion::Interactiva {
                solicitante: datos.solicitante,
                id_solicitud: id_dialogo_ssh(datos.escritorio_id),
            },
            UsoCanal::Escritorio,
        ) => match resultado {
            Ok(tomada) => tomada,
            Err(motivo) => {
                fallar_si_vigente(
                    &estado,
                    &datos,
                    format!("no se llega a «{}»: {motivo}", salto.nombre),
                )
                .await;
                return;
            }
        },
    };
    let host_ssh: Arc<dyn HostSsh> = Arc::new(voxl::HostSshRussh::nuevo(tomada.handle.clone()));
    let config = voxl::ConfigVoxl {
        nombre: datos.host.nombre.clone(),
        puerto: datos.host.puerto,
    };
    let arranque = tokio::select! {
        _ = datos.cancelar.cancelled() => {
            tomada.soltar().await;
            return;
        }
        resultado = voxl::arrancar(config, host_ssh, datos.host.escritorio_opciones.clone()) => resultado,
    };
    let sesion_voxl = match arranque {
        Ok(sesion) => sesion,
        Err(error) => {
            tomada.soltar().await;
            fallar_si_vigente(&estado, &datos, error.motivo().to_string()).await;
            return;
        }
    };
    let datos_voxl = DatosVoxl {
        puerto_tunel: sesion_voxl.puerto_tunel,
        clave: sesion_voxl.clave.clone(),
    };
    {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado
            .escritorios
            .sesiones
            .get_mut(&datos.escritorio_id)
        else {
            drop(estado_bloqueado);
            tomada.soltar().await;
            super::responder_error(
                &estado,
                datos.solicitante,
                "el escritorio ya no existe".to_string(),
                Some(datos.peticion_id),
            )
            .await;
            return;
        };
        if sesion.generacion != datos.generacion || sesion.estado != EstadoEscritorio::Abriendo {
            drop(estado_bloqueado);
            tomada.soltar().await;
            return;
        }
        sesion.voxl = Some(sesion_voxl);
        sesion.voxl_tomada = Some(tomada);
        let info = sesion.info();
        estado_bloqueado
            .escritorios
            .registro
            .alta_voxl(info, datos_voxl);
        estado_bloqueado
            .escritorios
            .actualizar_info(datos.escritorio_id);
        difundir_lista(&mut estado_bloqueado);
    }
    // Queda abierta: marcar_abierto anota, arranca la gracia, obtiene el
    // enlace (re-arrancando el visor si hizo falta) y contesta al solicitante;
    // si no hay enlace, falla con Error en vez de dejar la TUI colgada.
    marcar_abierto(&estado, datos.escritorio_id).await;
    info!(
        escritorio = datos.escritorio_id,
        host = %datos.host.nombre,
        "escritorio voxl abierto"
    );
}

// ---------------------------------------------------------------- gracia

/// Una revisión de la gracia: cierra las sesiones abiertas sin visor durante
/// gracia_seg y detecta las conexiones caídas. Lo que hay que cerrar o marcar
/// caído se aplica fuera del cerrojo.
pub struct Revision {
    pub cerrar: Vec<(u32, String)>,
    pub caidos: Vec<(u32, String)>,
    pub difundir: bool,
}

/// Decide la gracia de una sesión abierta. Función pura para poder probarla.
pub fn decidir_gracia(
    visto: bool,
    sin_visor_desde: Option<i64>,
    ahora: i64,
    gracia_seg: u64,
) -> (Option<i64>, bool) {
    if visto {
        return (None, false);
    }
    let desde = sin_visor_desde.unwrap_or(ahora);
    let gracia = gracia_seg.min(i64::MAX as u64) as i64;
    (Some(desde), ahora.saturating_sub(desde) >= gracia)
}

/// Tarea de revisión del servidor: gracia de los escritorios, caída de la
/// conexión del salto y estado de voxl. Una vez por segundo.
pub async fn revisar(estado: EstadoCompartido) {
    loop {
        tokio::time::sleep(PASO_REVISION).await;
        let revision = revisar_una_vez(&estado).await;
        for (id, motivo) in revision.cerrar {
            cerrar_sesion(&estado, id, &motivo).await;
        }
        for (id, motivo) in revision.caidos {
            marcar_aviso_caido(&estado, id, &motivo, false).await;
        }
        if revision.difundir {
            let mut estado_bloqueado = estado.lock().await;
            difundir_lista(&mut estado_bloqueado);
        }
        // El servidor del visor sigue a las sesiones del servidor, también a
        // las que todavía están abriendo (voxl tarda en registrarse).
        sincronizar_visor(&estado).await;
    }
}

async fn revisar_una_vez(estado: &EstadoCompartido) -> Revision {
    let mut revision = Revision {
        cerrar: Vec::new(),
        caidos: Vec::new(),
        difundir: false,
    };
    let mut actualizar = false;
    {
        let mut estado_bloqueado = estado.lock().await;
        let gracia_seg = estado_bloqueado.config.escritorio.gracia_seg;
        let ahora = chrono::Utc::now().timestamp();
        let ids: Vec<u32> = estado_bloqueado
            .escritorios
            .sesiones
            .keys()
            .copied()
            .collect();
        for id in ids {
            let Some(sesion) = estado_bloqueado.escritorios.sesiones.get_mut(&id) else {
                continue;
            };
            match sesion.estado {
                EstadoEscritorio::Abierto => {
                    // Conexión del salto caída: el canal murió con ella.
                    if let Some(handle) = &sesion.conexion_salto {
                        if handle.is_closed() {
                            let nombre = sesion
                                .a_traves_de
                                .clone()
                                .unwrap_or_else(|| sesion.host_nombre.clone());
                            revision
                                .caidos
                                .push((id, format!("se perdió la conexión con «{nombre}»")));
                            continue;
                        }
                    }
                    // voxl: si el túnel o el script terminaron, la sesión cae.
                    let mut visto = sesion.visor;
                    if let Some(v) = &sesion.voxl {
                        match v.estado() {
                            voxl::EstadoVoxl::Abierto => {
                                let conexiones = v.conexiones_abiertas() > 0;
                                if conexiones != sesion.visor {
                                    sesion.visor = conexiones;
                                    actualizar = true;
                                }
                                visto = conexiones;
                            }
                            voxl::EstadoVoxl::Caido { motivo } => {
                                revision.caidos.push((id, motivo));
                                continue;
                            }
                            voxl::EstadoVoxl::Cerrado { motivo } => {
                                revision.caidos.push((id, motivo));
                                continue;
                            }
                            voxl::EstadoVoxl::Cerrando => continue,
                        }
                    }
                    let (desde, cerrar) =
                        decidir_gracia(visto, sesion.sin_visor_desde, ahora, gracia_seg);
                    if desde != sesion.sin_visor_desde {
                        sesion.sin_visor_desde = desde;
                        actualizar = true;
                    }
                    if cerrar {
                        revision
                            .cerrar
                            .push((id, format!("sin visor durante {gracia_seg} s")));
                    }
                }
                EstadoEscritorio::Caido | EstadoEscritorio::Cerrando => {
                    if sesion.sin_visor_desde.take().is_some() {
                        actualizar = true;
                    }
                }
                EstadoEscritorio::Abriendo => {}
            }
        }
        if actualizar {
            estado_bloqueado.escritorios.actualizar_info_de_todas();
            revision.difundir = true;
        }
    }
    revision
}

// ---------------------------------------------------------------- diálogos

/// Adaptador de los diálogos del escritorio (certificado y credenciales) al
/// cliente solicitante: el servidor nunca toca el llavero (T25).
struct DialogosSolicitante {
    estado: EstadoCompartido,
    escritorio_id: u32,
    peticion_id: u64,
    solicitante: u32,
    host_id: i64,
    host_nombre: String,
    generacion: u64,
}

#[async_trait::async_trait]
impl DialogosEscritorio for DialogosSolicitante {
    async fn certificado(&self, datos: DatosCertificado) -> Result<bool, ErrorEscritorio> {
        let (tx, rx) = oneshot::channel();
        {
            let mut estado_bloqueado = self.estado.lock().await;
            if !estado_bloqueado.clientes.contains_key(&self.solicitante) {
                return Err(ErrorEscritorio::Cancelado(
                    "la ventana que abrió el escritorio se cerró".to_string(),
                ));
            }
            estado_bloqueado.escritorios.pendientes.insert(
                (self.solicitante, self.peticion_id),
                PendienteDialogo::Huella {
                    solicitante: self.solicitante,
                    responder: tx,
                },
            );
            if let Some(sesion) = estado_bloqueado
                .escritorios
                .sesiones
                .get_mut(&self.escritorio_id)
            {
                if sesion.generacion == self.generacion {
                    sesion.fase = Some(FaseEscritorio::VerificandoCertificado);
                }
            }
            let mensaje = MensajeServidor::PideHuellaEscritorio {
                peticion_id: self.peticion_id,
                escritorio_id: self.escritorio_id,
                host_id: self.host_id,
                host_nombre: self.host_nombre.clone(),
                datos,
            };
            if let Some(cliente) = estado_bloqueado.clientes.get(&self.solicitante) {
                difusion::enviar(cliente, mensaje);
            }
            estado_bloqueado
                .escritorios
                .actualizar_info(self.escritorio_id);
            difundir_lista(&mut estado_bloqueado);
        }
        match tokio::time::timeout(PLAZO_DIALOGO, rx).await {
            Ok(Ok(aceptada)) => Ok(aceptada),
            Ok(Err(_)) => Err(ErrorEscritorio::Cancelado(
                "la ventana que abrió el escritorio se cerró".to_string(),
            )),
            Err(_) => {
                descartar_dialogo(&self.estado, self.solicitante, self.peticion_id).await;
                Err(ErrorEscritorio::Cancelado(
                    "el certificado no se contestó en 5 minutos".to_string(),
                ))
            }
        }
    }

    async fn credenciales(
        &self,
        peticion: PeticionCredenciales,
    ) -> Result<Option<CredencialesEscritorio>, ErrorEscritorio> {
        let (tx, rx) = oneshot::channel();
        {
            let mut estado_bloqueado = self.estado.lock().await;
            if !estado_bloqueado.clientes.contains_key(&self.solicitante) {
                return Err(ErrorEscritorio::Cancelado(
                    "la ventana que abrió el escritorio se cerró".to_string(),
                ));
            }
            estado_bloqueado.escritorios.pendientes.insert(
                (self.solicitante, self.peticion_id),
                PendienteDialogo::Credenciales {
                    solicitante: self.solicitante,
                    responder: tx,
                },
            );
            let mensaje = MensajeServidor::PideCredenciales {
                peticion_id: self.peticion_id,
                escritorio_id: self.escritorio_id,
                host_nombre: self.host_nombre.clone(),
                peticion,
            };
            if let Some(cliente) = estado_bloqueado.clientes.get(&self.solicitante) {
                difusion::enviar(cliente, mensaje);
            }
        }
        match tokio::time::timeout(PLAZO_DIALOGO, rx).await {
            Ok(Ok(credenciales)) => Ok(credenciales),
            Ok(Err(_)) => Err(ErrorEscritorio::Cancelado(
                "la ventana que abrió el escritorio se cerró".to_string(),
            )),
            Err(_) => {
                descartar_dialogo(&self.estado, self.solicitante, self.peticion_id).await;
                Err(ErrorEscritorio::Cancelado(
                    "las credenciales no se contestaron en 5 minutos".to_string(),
                ))
            }
        }
    }
}

async fn descartar_dialogo(estado: &EstadoCompartido, cliente_id: u32, peticion_id: u64) {
    let mut estado_bloqueado = estado.lock().await;
    estado_bloqueado
        .escritorios
        .pendientes
        .remove(&(cliente_id, peticion_id));
}

// ---------------------------------------------------------------- ayudas

/// Comprueba que la petición viene del rango de escritorios (2^46).
fn comprobar_peticion(peticion_id: u64) -> Result<(), String> {
    if peticion_id < RANGO_ESCRITORIOS {
        return Err(format!(
            "peticion_id fuera del rango de escritorios (desde 2^46): {peticion_id}"
        ));
    }
    Ok(())
}

/// Huella del certificado aceptada del host, leída del almacén de solo
/// lectura (HOSTS.huella_escritorio, propia del equipo).
fn huella_de(lectura: &crate::almacen::Almacen, host_id: i64) -> Option<String> {
    lectura
        .obtener_host(host_id)
        .ok()
        .and_then(|host| host.huella_escritorio)
}

async fn huella_actual(estado: &EstadoCompartido, host_id: i64) -> Result<Option<String>, String> {
    let estado_bloqueado = estado.lock().await;
    let lectura = estado_bloqueado
        .lectura
        .lock()
        .map_err(|_| "el almacén del servidor no está disponible".to_string())?;
    Ok(huella_de(&lectura, host_id))
}

/// Lee el host y todos los hosts (para resolver «a través de» y validar).
async fn leer_hosts(
    estado: &EstadoCompartido,
    host_id: i64,
) -> (Option<Host>, HashMap<i64, Host>, Rutas) {
    let estado_bloqueado = estado.lock().await;
    let rutas = estado_bloqueado.rutas.clone();
    let Ok(lectura) = estado_bloqueado.lectura.lock() else {
        return (None, HashMap::new(), rutas);
    };
    let saltos: HashMap<i64, Host> = lectura
        .listar_hosts()
        .unwrap_or_default()
        .into_iter()
        .map(|host| (host.id, host))
        .collect();
    let host = lectura.obtener_host(host_id).ok();
    (host, saltos, rutas)
}

/// Valida la fila del host antes de abrir un escritorio (T38). El motivo es
/// legible y se antepone «edita el host: » al contestar.
pub fn validar(host: &Host, saltos: &HashMap<i64, Host>) -> Result<(), String> {
    if host.tipo.es_ssh() {
        return Err(format!(
            "«{}» es un host SSH: se abre como terminal, no como escritorio",
            host.nombre
        ));
    }
    host.escritorio_opciones.validar(host.tipo)?;
    if let Some(salto_id) = host.salto_host_id {
        let Some(salto) = saltos.get(&salto_id) else {
            return Err("el host «a través de» ya no existe".to_string());
        };
        if !salto.tipo.es_ssh() {
            return Err(format!(
                "«a través de» debe ser un host SSH y «{}» no lo es",
                salto.nombre
            ));
        }
    } else if host.tipo == TipoHost::Voxl {
        return Err(
            "voxl necesita un host SSH por el que abrirse: elige uno en «a través de»".to_string(),
        );
    }
    if host.tipo == TipoHost::Vnc
        && host.salto_host_id.is_none()
        && !host.escritorio_opciones.directo
        && !escritorio::es_loopback(&host.direccion)
    {
        return Err(
            "la imagen y el teclado viajarán sin cifrar: confirma «directo» en la ficha o elige un host SSH en «a través de»"
                .to_string(),
        );
    }
    if host.tipo == TipoHost::Voxl && host.puerto < 1024 {
        return Err("el puerto de voxl debe ser 1024 o mayor".to_string());
    }
    Ok(())
}

fn sesion_portapapeles(host: &Host) -> bool {
    host.escritorio_opciones.portapapeles
}

/// Datos de una sesión nueva en estado «abriendo».
fn nueva_sesion(
    id: u32,
    host: &Host,
    saltos: &HashMap<i64, Host>,
    solicitante: u32,
    peticion_id: u64,
    h264: bool,
) -> Escritorio {
    let tamano = host
        .escritorio_opciones
        .tamano
        .fijo()
        .unwrap_or(TAMANO_INICIAL);
    let imagen = match host.tipo {
        TipoHost::Rdp => Some(if h264 {
            ImagenEscritorio::H264
        } else {
            ImagenEscritorio::Rectangulos
        }),
        TipoHost::Vnc => Some(ImagenEscritorio::Rectangulos),
        TipoHost::Voxl => Some(ImagenEscritorio::Voxl),
        TipoHost::Ssh => None,
    };
    let a_traves_de = host
        .salto_host_id
        .and_then(|salto_id| saltos.get(&salto_id))
        .map(|salto| salto.nombre.clone());
    Escritorio {
        id,
        host_id: host.id,
        host_nombre: host.nombre.clone(),
        nombre: host.nombre.clone(),
        tipo: host.tipo,
        estado: EstadoEscritorio::Abriendo,
        fase: Some(FaseEscritorio::Conectando),
        motivo: Some("abriendo".to_string()),
        solicitante,
        peticion_id,
        desde: chrono::Utc::now().timestamp(),
        tamano,
        imagen,
        fps: 0,
        directo: host.salto_host_id.is_none(),
        a_traves_de,
        redimensionable: host.escritorio_opciones.tamano == escritorio::TamanoEscritorio::Ventana,
        visor: false,
        sin_visor_desde: None,
        reconectando: false,
        responder_al_solicitante: true,
        generacion: 1,
        h264,
        cancelar: tokio_util::sync::CancellationToken::new(),
        conexion_salto: None,
        voxl: None,
        voxl_tomada: None,
    }
}

/// Configuración ya resuelta que recibe el cliente RDP o RFB. La
/// distribución de teclado es la del host si la ficha la fija y, si no, la
/// efectiva de la configuración global (auto ya resuelta).
fn config_escritorio(host: &Host, h264_sesion: bool, distribucion_auto: &str) -> ConfigEscritorio {
    ConfigEscritorio {
        tipo: host.tipo,
        direccion: host.direccion.clone(),
        puerto: host.puerto,
        usuario: host.usuario.clone(),
        dominio: host.escritorio_dominio.clone(),
        opciones: host.escritorio_opciones.clone(),
        distribucion_teclado: host
            .escritorio_opciones
            .distribucion_efectiva(distribucion_auto),
        h264: h264_sesion,
        tamano_inicial: host
            .escritorio_opciones
            .tamano
            .fijo()
            .unwrap_or(TAMANO_INICIAL),
        portapapeles: host.escritorio_opciones.portapapeles,
        huella_escritorio: host.huella_escritorio.clone(),
    }
}

/// Id con el que viajan los diálogos SSH del host «a través de»: no puede
/// chocar con un id de sesión de terminal.
fn id_dialogo_ssh(escritorio_id: u32) -> u32 {
    BASE_DIALOGO_SSH | escritorio_id
}

/// Enlace de un solo uso para el visor de una sesión, si el servidor está en
/// marcha.
async fn enlace_de(estado: &EstadoCompartido, escritorio_id: u32) -> Option<String> {
    let estado_bloqueado = estado.lock().await;
    estado_bloqueado.escritorios.registro.enlace(escritorio_id)
}

/// Ajusta el servidor del visor al número de sesiones de escritorio: debe
/// estar en marcha mientras haya alguna (también las que aún están abriendo y
/// por tanto no están en el registro del visor) y pararse cuando no quede
/// ninguna. Si una baja lo paró por carrera con otra apertura, se vuelve a
/// arrancar.
async fn sincronizar_visor(estado: &EstadoCompartido) {
    let (hay_sesiones, apagando, registro) = {
        let estado_bloqueado = estado.lock().await;
        (
            !estado_bloqueado.escritorios.sesiones.is_empty(),
            estado_bloqueado.apagando,
            estado_bloqueado.escritorios.registro.clone(),
        )
    };
    if apagando {
        return;
    }
    if !hay_sesiones {
        registro.parar_si_vacio();
        return;
    }
    if registro.puerto().is_none() {
        if let Err(motivo) = asegurar_visor(estado).await {
            warn!("no se pudo arrancar el servidor del visor: {motivo}");
        }
    }
}

/// Falla una sesión que ya había quedado abierta pero no puede dar visor (sin
/// enlace): la quita, anota escritorio_fallido, avisa al visor y contesta
/// Error a la petición. Nunca deja a la TUI esperando.
async fn fallar_abierto(estado: &EstadoCompartido, escritorio_id: u32, motivo: String) {
    let (solicitante, peticion_id, responder, visor, voxl, tomada) = {
        let mut estado_bloqueado = estado.lock().await;
        let Some(sesion) = estado_bloqueado.escritorios.sesiones.get(&escritorio_id) else {
            return;
        };
        let solicitante = sesion.solicitante;
        let peticion_id = sesion.peticion_id;
        let responder = sesion.responder_al_solicitante;
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(OrdenBd::Anotar {
            tipo: registro::ESCRITORIO_FALLIDO.to_string(),
            host_id: Some(sesion.host_id),
            identidad_id: None,
            detalle: motivo.clone(),
            resultado: ResultadoRegistro::Error,
        });
        let visor = estado_bloqueado.escritorios.registro.sesion(escritorio_id);
        let Some(mut sesion) = estado_bloqueado.escritorios.sesiones.remove(&escritorio_id) else {
            return;
        };
        sesion.cancelar.cancel();
        let voxl = sesion.voxl.take();
        let tomada = sesion.voxl_tomada.take();
        difundir_lista(&mut estado_bloqueado);
        drop(estado_bloqueado);
        (solicitante, peticion_id, responder, visor, voxl, tomada)
    };
    if let Some(visor) = visor {
        visor.estado_visor("caido", Some(&motivo));
    }
    let registro = estado.lock().await.escritorios.registro.clone();
    registro.baja(escritorio_id);
    sincronizar_visor(estado).await;
    difundir_despues_de_baja(estado).await;
    if let Some(mut voxl) = voxl {
        if tokio::time::timeout(PLAZO_CAMINO, voxl.parar())
            .await
            .is_err()
        {
            warn!(
                escritorio = escritorio_id,
                "voxl no se paró a tiempo al fallar la apertura del escritorio"
            );
        }
    }
    if let Some(tomada) = tomada {
        tomada.soltar().await;
    }
    if responder {
        super::responder_error(estado, solicitante, motivo, Some(peticion_id)).await;
    }
}

/// Arranca el servidor del visor si no está en marcha (primera sesión) y
/// devuelve su puerto.
async fn asegurar_visor(estado: &EstadoCompartido) -> Result<u16, String> {
    let cerrojo = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado.escritorios.cerrojo_visor.clone()
    };
    let _guardia = cerrojo.lock().await;
    let (puerto, registro) = {
        let estado_bloqueado = estado.lock().await;
        (
            estado_bloqueado.config.escritorio.puerto_visor,
            estado_bloqueado.escritorios.registro.clone(),
        )
    };
    if let Some(puerto) = registro.puerto() {
        return Ok(puerto);
    }
    registro
        .arrancar(puerto)
        .await
        .map_err(|error: ErrorVisor| error.to_string())
}

/// Difunde la lista después de dar de baja sesiones en el registro del visor:
/// las ventanas tienen que ver que ya no están.
async fn difundir_despues_de_baja(estado: &EstadoCompartido) {
    let mut estado_bloqueado = estado.lock().await;
    difundir_lista(&mut estado_bloqueado);
}

/// Anula los diálogos de un cliente que se ha ido y da por fallidas las
/// aperturas que esperaban su respuesta.
pub async fn cliente_desconectado(estado: &EstadoCompartido, cliente_id: u32) {
    let mut bajas: Vec<u32> = Vec::new();
    {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .escritorios
            .pendientes
            .retain(|_, pendiente| pendiente.solicitante() != cliente_id);
        let ids: Vec<u32> = estado_bloqueado
            .escritorios
            .sesiones
            .iter()
            .filter(|(_, sesion)| {
                sesion.estado == EstadoEscritorio::Abriendo && sesion.solicitante == cliente_id
            })
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let Some((host_id, reconectando)) = estado_bloqueado
                .escritorios
                .sesiones
                .get(&id)
                .map(|sesion| (sesion.host_id, sesion.reconectando))
            else {
                continue;
            };
            let bd = estado_bloqueado.bd.clone();
            let _ = bd.send(OrdenBd::Anotar {
                tipo: registro::ESCRITORIO_FALLIDO.to_string(),
                host_id: Some(host_id),
                identidad_id: None,
                detalle: "la ventana que abrió el escritorio se cerró".to_string(),
                resultado: ResultadoRegistro::Error,
            });
            if let Some(sesion) = estado_bloqueado.escritorios.sesiones.get_mut(&id) {
                sesion.cancelar.cancel();
                if reconectando {
                    sesion.estado = EstadoEscritorio::Caido;
                    sesion.fase = None;
                    sesion.motivo = Some("la ventana que abrió el escritorio se cerró".to_string());
                    sesion.sin_visor_desde = None;
                }
            }
            if !reconectando {
                estado_bloqueado.escritorios.sesiones.remove(&id);
                bajas.push(id);
            }
            estado_bloqueado.escritorios.actualizar_info(id);
        }
        difundir_lista(&mut estado_bloqueado);
    }
    if !bajas.is_empty() {
        let registro = estado.lock().await.escritorios.registro.clone();
        for id in bajas {
            registro.baja(id);
        }
        sincronizar_visor(estado).await;
        difundir_despues_de_baja(estado).await;
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::escritorio::OpcionesEscritorio;

    fn host_de_escritorio(tipo: TipoHost) -> Host {
        let mut host = crate::modelo::host_de_prueba();
        host.tipo = tipo;
        host.puerto = tipo.puerto_por_defecto();
        host.escritorio_opciones = OpcionesEscritorio::por_defecto(tipo);
        host
    }

    #[test]
    fn la_peticion_tiene_que_venir_del_rango_de_escritorios() {
        assert!(comprobar_peticion(RANGO_ESCRITORIOS).is_ok());
        assert!(comprobar_peticion(RANGO_ESCRITORIOS + 7).is_ok());
        assert!(comprobar_peticion(RANGO_ESCRITORIOS - 1).is_err());
        assert!(comprobar_peticion(1).is_err());
    }

    #[test]
    fn un_host_ssh_no_se_abre_como_escritorio() {
        let host = host_de_escritorio(TipoHost::Ssh);
        assert!(validar(&host, &HashMap::new()).is_err());
    }

    #[test]
    fn voxl_necesita_un_host_ssh_por_el_que_abrirse() {
        let host = host_de_escritorio(TipoHost::Voxl);
        let error = validar(&host, &HashMap::new()).unwrap_err();
        assert!(error.contains("a través de"), "{error}");
    }

    #[test]
    fn voxl_a_traves_de_un_host_ssh_se_acepta() {
        let mut host = host_de_escritorio(TipoHost::Voxl);
        host.salto_host_id = Some(9);
        let mut salto = host_de_escritorio(TipoHost::Ssh);
        salto.id = 9;
        salto.nombre = "web-01".to_string();
        let saltos = HashMap::from([(9, salto)]);
        assert!(validar(&host, &saltos).is_ok());
    }

    #[test]
    fn un_vnc_directo_sin_confirmar_avisa() {
        let mut host = host_de_escritorio(TipoHost::Vnc);
        host.direccion = "192.0.2.40".to_string();
        let error = validar(&host, &HashMap::new()).unwrap_err();
        assert!(error.contains("sin cifrar"), "{error}");
        // En loopback no hace falta confirmarlo.
        host.direccion = "127.0.0.1".to_string();
        assert!(validar(&host, &HashMap::new()).is_ok());
        // Y con «directo» confirmado, tampoco.
        host.direccion = "192.0.2.40".to_string();
        host.escritorio_opciones.directo = true;
        assert!(validar(&host, &HashMap::new()).is_ok());
    }

    #[test]
    fn la_gracia_cierra_al_pasar_los_segundos_configurados() {
        let (desde, cerrar) = decidir_gracia(false, None, 1_000, 60);
        assert_eq!(desde, Some(1_000));
        assert!(!cerrar);
        let (desde, cerrar) = decidir_gracia(false, Some(900), 1_000, 60);
        assert_eq!(desde, Some(900));
        assert!(cerrar);
        let (desde, cerrar) = decidir_gracia(true, Some(900), 1_000, 60);
        assert_eq!(desde, None);
        assert!(!cerrar);
    }

    #[test]
    fn el_id_de_los_dialogos_ssh_no_choca_con_las_sesiones() {
        assert_eq!(id_dialogo_ssh(1), BASE_DIALOGO_SSH | 1);
        assert!(id_dialogo_ssh(1) >= BASE_DIALOGO_SSH);
    }

    /// M2: cada ventana numera sus peticiones por su cuenta, así que el mapa
    /// de diálogos va por (cliente, peticion_id) y la respuesta de una ventana
    /// no puede pisar ni consumir el diálogo de otra.
    #[test]
    fn dos_ventanas_con_el_mismo_peticion_id_conservan_su_dialogo() {
        let mut escritorios = Escritorios::nuevo();
        let peticion = RANGO_ESCRITORIOS + 1;
        let (tx_a, mut rx_a) = oneshot::channel();
        let (tx_b, mut rx_b) = oneshot::channel();
        escritorios.pendientes.insert(
            (1, peticion),
            PendienteDialogo::Huella {
                solicitante: 1,
                responder: tx_a,
            },
        );
        escritorios.pendientes.insert(
            (2, peticion),
            PendienteDialogo::Huella {
                solicitante: 2,
                responder: tx_b,
            },
        );
        // Una respuesta de una ventana que no tiene ese diálogo no lo toca.
        escritorios.resolver_huella(9, peticion, true);
        assert_eq!(escritorios.pendientes.len(), 2);
        // La ventana 2 contesta al suyo; el de la 1 sigue en vuelo.
        escritorios.resolver_huella(2, peticion, true);
        assert!(matches!(rx_b.try_recv(), Ok(true)));
        assert!(
            rx_a.try_recv().is_err(),
            "el diálogo de la otra ventana sigue en vuelo"
        );
        escritorios.resolver_huella(1, peticion, false);
        assert!(matches!(rx_a.try_recv(), Ok(false)));
        assert!(escritorios.pendientes.is_empty());
    }

    /// M3: entre intentos de credenciales se relee HOSTS.huella_escritorio,
    /// que el cliente acaba de guardar al aceptar el certificado; así el
    /// reintento no vuelve a pedir el mismo certificado.
    #[test]
    fn la_huella_del_host_se_relee_entre_intentos() {
        let almacen = crate::almacen::abrir_en_memoria().expect("almacén en memoria");
        let datos = crate::modelo::DatosHost {
            nombre: "win-01".to_string(),
            direccion: "192.0.2.30".to_string(),
            puerto: 3389,
            tipo: TipoHost::Rdp,
            ..crate::modelo::DatosHost::default()
        };
        let id =
            crate::almacen::hosts::crear(almacen.conexion(), &datos, crate::modelo::Origen::Manual)
                .expect("creando el host");
        assert_eq!(huella_de(&almacen, id), None);
        // El cliente acepta el certificado y lo guarda: el siguiente intento
        // ve la huella nueva.
        almacen
            .fijar_huella_escritorio(id, Some("SHA256:T2qR…9cQ"))
            .expect("fijando la huella");
        assert_eq!(huella_de(&almacen, id), Some("SHA256:T2qR…9cQ".to_string()));
    }

    /// M9: la distribución de teclado que anuncia RDP es la del host si la
    /// ficha la fija y, si no, la efectiva de la configuración global.
    #[test]
    fn el_teclado_del_host_manda_sobre_el_global() {
        let mut host = host_de_escritorio(TipoHost::Rdp);
        host.escritorio_opciones.teclado = Some("es-ES".to_string());
        assert_eq!(
            config_escritorio(&host, true, "en-US").distribucion_teclado,
            "es-ES"
        );
        host.escritorio_opciones.teclado = Some("auto".to_string());
        assert_eq!(
            config_escritorio(&host, true, "en-US").distribucion_teclado,
            "en-US"
        );
        host.escritorio_opciones.teclado = None;
        assert_eq!(
            config_escritorio(&host, true, "es-ES").distribucion_teclado,
            "es-ES"
        );
    }
}
