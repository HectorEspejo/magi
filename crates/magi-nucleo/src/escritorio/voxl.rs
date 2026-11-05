//! voxl por SSH (Fase 12): arranque con el script embebido
//! escritorio_voxl.sh, contraseña aleatoria por la entrada estándar, espera de
//! /healthz, túnel de la sesión en 127.0.0.1 y parada al cerrar.
//!
//! El servidor de escritorios aporta la conexión SSH ya tomada del pool. Para que
//! esta pieza no dependa de src/servidor, aquí vive una abstracción mínima del
//! host (HostSsh) y un adaptador listo para russh (HostSshRussh).
//!
//! Nada de lo que llega del host (versión, líneas de error) se interpola en la
//! orden remota: el script viaja como argumento de sh -c y los datos como
//! argumentos ya validados. La contraseña solo va por la entrada estándar y vive
//! en memoria como Zeroizing; no aparece en la línea de órdenes ni en los
//! mensajes de error.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::ChannelMsg;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

use super::{pantalla_valida, programa_valido, ErrorEscritorio, Flujo, OpcionesEscritorio};
use crate::conexion::abrir_con_plazo;
use crate::conexion::cliente::Cliente;

/// Script embebido que arranca voxl y lo para cuando se cierra su entrada.
pub const SCRIPT_VOXL: &str = include_str!("../../scripts/escritorio_voxl.sh");

/// Plazo de cada espera del arranque: la versión, el LISTO y /healthz.
pub const PLAZO_ARRANQUE: Duration = Duration::from_secs(10);

/// Plazo de la parada completa (fin de la entrada, TERM y, a los 5 s, KILL).
pub const PLAZO_PARADA: Duration = Duration::from_secs(10);

/// Plazo de la apertura de un canal SSH (exec o direct-tcpip).
const PLAZO_CANAL: Duration = Duration::from_secs(10);

/// Plazo de un único intento de /healthz (el total lo marca PLAZO_ARRANQUE).
const PLAZO_INTENTO_SALUD: Duration = Duration::from_secs(2);

/// Pausa entre intentos de /healthz mientras voxl arranca.
const PAUSA_SALUD: Duration = Duration::from_millis(150);

/// Puerto mínimo de voxl en el host (§7.13: desde 1024).
pub const PUERTO_MIN: u16 = 1024;

/// Lo máximo de stderr que se guarda para traducir un fallo (nunca se registra).
const LIMITE_DIAGNOSTICO: usize = 4 * 1024;

/// Lo máximo de una línea de stdout o stderr del script (1 MB). Un host
/// comprometido no puede agotar la memoria con un flujo sin saltos de línea.
const LIMITE_LINEA: usize = 1024 * 1024;

/// Plazo para que el script termine al cerrarle la entrada antes de forzar el
/// cierre del canal. El script manda KILL a los 5 s, así que 6 s deja margen y
/// mantiene la parada completa por debajo de los 10 s que espera el servidor.
const PLAZO_FIN_CANAL: Duration = Duration::from_secs(6);

// ---------------------------------------------------------------- configuración

/// Datos del host que necesita el arranque de voxl.
#[derive(Debug, Clone)]
pub struct ConfigVoxl {
    /// Nombre legible del host, para los mensajes de error.
    pub nombre: String,
    /// Puerto en el que voxl escuchará en el host (el de la ficha).
    pub puerto: u16,
}

impl ConfigVoxl {
    fn nombre_legible(&self) -> &str {
        if self.nombre.trim().is_empty() {
            "el host"
        } else {
            &self.nombre
        }
    }
}

// ---------------------------------------------------------------- host SSH

/// Abstracción del host SSH por el que se arranca voxl.
///
/// La implementa el servidor de escritorios (con el Handle de russh de la
/// conexión que ha tomado del pool) y, en las pruebas, un canal directo. La
/// implementación compone la orden remota escapando programa y argumentos:
/// nunca recibe el texto del comando ya montado.
#[async_trait::async_trait]
pub trait HostSsh: Send + Sync {
    /// Abre un canal exec en el host para el programa con sus argumentos.
    async fn exec(
        &self,
        programa: &str,
        argumentos: &[String],
    ) -> Result<CanalExec, ErrorEscritorio>;

    /// Abre un direct-tcpip desde el host hasta direccion:puerto.
    async fn direct_tcpip(&self, direccion: &str, puerto: u16) -> Result<Flujo, ErrorEscritorio>;

    /// ¿Sigue viva la conexión SSH? Distingue «se perdió la conexión» de «voxl
    /// terminó».
    fn conexion_viva(&self) -> bool {
        true
    }
}

/// Adaptador de HostSsh sobre una conexión de russh ya autenticada.
///
/// El servidor solo tiene que crear uno con el mango de su ConexionTomada y
/// guardar la conexión tomada en la sesión hasta que deje de usarla.
pub struct HostSshRussh {
    handle: Arc<russh::client::Handle<Cliente>>,
}

impl HostSshRussh {
    pub fn nuevo(handle: Arc<russh::client::Handle<Cliente>>) -> Self {
        Self { handle }
    }
}

#[async_trait::async_trait]
impl HostSsh for HostSshRussh {
    async fn exec(
        &self,
        programa: &str,
        argumentos: &[String],
    ) -> Result<CanalExec, ErrorEscritorio> {
        let comando = comando_remoto(programa, argumentos);
        let handle = self.handle.clone();
        let canal = abrir_con_plazo(PLAZO_CANAL, async move {
            let canal = handle.channel_open_session().await?;
            canal.exec(true, comando.into_bytes()).await?;
            Ok::<russh::Channel<russh::client::Msg>, russh::Error>(canal)
        })
        .await
        .map_err(|error| {
            ErrorEscritorio::Conexion(format!("no se pudo abrir el canal de voxl: {error}"))
        })?;
        Ok(controlar_canal(canal))
    }

    async fn direct_tcpip(&self, direccion: &str, puerto: u16) -> Result<Flujo, ErrorEscritorio> {
        let handle = self.handle.clone();
        let direccion = direccion.to_string();
        let canal = abrir_con_plazo(PLAZO_CANAL, async move {
            handle
                .channel_open_direct_tcpip(direccion, u32::from(puerto), "127.0.0.1", 0)
                .await
        })
        .await
        .map_err(|error| {
            ErrorEscritorio::Conexion(format!(
                "el host no abrió el canal hasta 127.0.0.1:{puerto}: {error}"
            ))
        })?;
        Ok(Box::new(canal.into_stream()))
    }

    fn conexion_viva(&self) -> bool {
        !self.handle.is_closed()
    }
}

/// Compone la orden remota: programa y argumentos, cada pieza escapada para el
/// shell. El script (un argumento más de sh -c) nunca se interpola suelto.
fn comando_remoto(programa: &str, argumentos: &[String]) -> String {
    let mut comando = String::new();
    comando.push_str(&shell_escape::escape(Cow::Borrowed(programa)));
    for argumento in argumentos {
        comando.push(' ');
        comando.push_str(&shell_escape::escape(Cow::Borrowed(argumento.as_str())));
    }
    comando
}

/// Envuelve el canal de russh en el CanalExec de la sesión: una tarea reparte
/// stdout y stderr, escribe la entrada estándar y avisa del cierre.
fn controlar_canal(canal: russh::Channel<russh::client::Msg>) -> CanalExec {
    let (lectura, escritura) = canal.split();
    let (salida, salida_rx) = mpsc::unbounded_channel();
    let (errores, errores_rx) = mpsc::unbounded_channel();
    let (entrada, mut entrada_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (fin, fin_rx) = oneshot::channel::<()>();
    let (cierre, mut cierre_rx) = oneshot::channel::<()>();
    let lector = LectorCanal::nuevo(salida_rx, errores_rx);
    // El lector avisa aquí cuando una línea pasa del máximo: esta tarea deja de
    // traer datos del host y cierra el canal para no acumular memoria.
    let desbordado = lector.marca_desborde();
    tokio::spawn(async move {
        let mut lectura = lectura;
        let escritura = escritura;
        let mut entrada_abierta = true;
        loop {
            if desbordado.load(Ordering::Relaxed) {
                let _ = escritura.close().await;
                break;
            }
            tokio::select! {
                datos = entrada_rx.recv(), if entrada_abierta => match datos {
                    Some(datos) => {
                        if escritura.data_bytes(datos).await.is_err() {
                            break;
                        }
                    }
                    // Fin de la entrada estándar: el script manda TERM a voxl.
                    None => {
                        entrada_abierta = false;
                        let _ = escritura.eof().await;
                    }
                },
                _ = &mut cierre_rx => {
                    let _ = escritura.close().await;
                    break;
                }
                mensaje = lectura.wait() => match mensaje {
                    Some(ChannelMsg::Data { data }) => {
                        if desbordado.load(Ordering::Relaxed) {
                            let _ = escritura.close().await;
                            break;
                        }
                        if salida.send(data.to_vec()).is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        if desbordado.load(Ordering::Relaxed) {
                            let _ = escritura.close().await;
                            break;
                        }
                        if errores.send(data.to_vec()).is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::Close) | None => break,
                    Some(_) => {}
                },
            }
        }
        let _ = fin.send(());
    });
    CanalExec::con_partes(
        lector,
        ControlCanal {
            entrada: Some(entrada),
            fin: Some(fin_rx),
            cierre: Some(cierre),
        },
    )
}

// ---------------------------------------------------------------- canal exec

/// Canal exec abierto en el host: stdout y stderr del script y su entrada
/// estándar. Cerrar la entrada es la parada: el script manda TERM a voxl y, si a
/// los 5 s sigue, KILL.
pub struct CanalExec {
    lector: LectorCanal,
    control: ControlCanal,
}

impl CanalExec {
    /// Canal sin aviso de cierre ni cierre forzado (implementaciones sencillas).
    pub fn nuevo(
        salida: mpsc::UnboundedReceiver<Vec<u8>>,
        errores: mpsc::UnboundedReceiver<Vec<u8>>,
        entrada: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        Self {
            lector: LectorCanal::nuevo(salida, errores),
            control: ControlCanal {
                entrada: Some(entrada),
                fin: None,
                cierre: None,
            },
        }
    }

    /// Canal con el aviso de fin de canal y el cierre forzado.
    pub fn con_control(
        salida: mpsc::UnboundedReceiver<Vec<u8>>,
        errores: mpsc::UnboundedReceiver<Vec<u8>>,
        entrada: mpsc::UnboundedSender<Vec<u8>>,
        fin: oneshot::Receiver<()>,
        cierre: oneshot::Sender<()>,
    ) -> Self {
        Self {
            lector: LectorCanal::nuevo(salida, errores),
            control: ControlCanal {
                entrada: Some(entrada),
                fin: Some(fin),
                cierre: Some(cierre),
            },
        }
    }

    /// Canal con las dos mitades ya montadas (lo usa el adaptador de russh).
    fn con_partes(lector: LectorCanal, control: ControlCanal) -> Self {
        Self { lector, control }
    }

    /// Escribe en la entrada estándar del script (la contraseña y su salto).
    pub fn escribir(&mut self, datos: &[u8]) -> Result<(), ErrorEscritorio> {
        self.control.escribir(datos)
    }

    /// Fin de la entrada estándar: el script para voxl. Idempotente.
    pub fn cerrar_entrada(&mut self) {
        self.control.cerrar_entrada();
    }

    /// Espera a que el canal remoto se cierre, con plazo.
    pub async fn esperar_fin(&mut self, plazo: Duration) -> bool {
        self.control.esperar_fin(plazo).await
    }

    /// Fuerza el cierre del canal (para paradas que no terminan solas).
    pub fn forzar_cierre(&mut self) {
        self.control.forzar_cierre();
    }

    /// stderr acumulado del script, solo para traducir el motivo de un fallo.
    fn diagnostico(&self) -> &str {
        self.lector.diagnostico()
    }

    /// Siguiente línea del script (stdout o stderr); None al cerrarse el canal.
    async fn siguiente_linea(&mut self) -> Option<Linea> {
        self.lector.siguiente_linea().await
    }

    /// Separa la lectura (para la tarea vigía) del control (para la sesión).
    fn separar(self) -> (ControlCanal, LectorCanal) {
        (self.control, self.lector)
    }
}

/// Una línea del script, con su procedencia.
enum Linea {
    Salida(String),
    Error(String),
    /// El host pasó del máximo de una línea sin terminarla: se corta el canal.
    DemasiadoLarga,
}

/// Lectura de stdout y stderr del canal con búfer de líneas incompletas.
struct LectorCanal {
    salida: mpsc::UnboundedReceiver<Vec<u8>>,
    errores: mpsc::UnboundedReceiver<Vec<u8>>,
    salida_abierta: bool,
    errores_abierto: bool,
    bufer_salida: String,
    bufer_errores: String,
    // Solo se queda para traducir un fallo; nunca se registra ni se enseña tal cual.
    diagnostico: String,
    /// Se pone a true cuando una línea pasa de LIMITE_LINEA: la tarea del canal
    /// deja de traer datos y cierra el canal.
    desbordado: Arc<AtomicBool>,
}

impl LectorCanal {
    fn nuevo(
        salida: mpsc::UnboundedReceiver<Vec<u8>>,
        errores: mpsc::UnboundedReceiver<Vec<u8>>,
    ) -> Self {
        Self {
            salida,
            errores,
            salida_abierta: true,
            errores_abierto: true,
            bufer_salida: String::new(),
            bufer_errores: String::new(),
            diagnostico: String::new(),
            desbordado: Arc::new(AtomicBool::new(false)),
        }
    }

    fn diagnostico(&self) -> &str {
        &self.diagnostico
    }

    /// Marca compartida con la tarea del canal: avisa del corte por desborde.
    fn marca_desborde(&self) -> Arc<AtomicBool> {
        self.desbordado.clone()
    }

    /// Siguiente línea del script o None cuando el canal se ha cerrado.
    async fn siguiente_linea(&mut self) -> Option<Linea> {
        // Campos sueltos: el select! mantiene prestados los receptores
        // mientras el cuerpo usa los búferes.
        let Self {
            salida,
            errores,
            salida_abierta,
            errores_abierto,
            bufer_salida,
            bufer_errores,
            diagnostico,
            desbordado,
        } = self;
        loop {
            if let Some(linea) = extraer_linea(bufer_salida, false) {
                return Some(Linea::Salida(linea));
            }
            if let Some(linea) = extraer_linea(bufer_errores, false) {
                anotar_diagnostico(diagnostico, &linea);
                return Some(Linea::Error(linea));
            }
            if !*salida_abierta && !*errores_abierto {
                return None;
            }
            tokio::select! {
                dato = salida.recv(), if *salida_abierta => match dato {
                    Some(dato) => {
                        bufer_salida.push_str(&String::from_utf8_lossy(&dato));
                        if excede_limite(bufer_salida) {
                            desbordado.store(true, Ordering::Relaxed);
                            bufer_salida.clear();
                            bufer_errores.clear();
                            return Some(Linea::DemasiadoLarga);
                        }
                    }
                    None => {
                        *salida_abierta = false;
                        if let Some(linea) = extraer_linea(bufer_salida, true) {
                            return Some(Linea::Salida(linea));
                        }
                    }
                },
                dato = errores.recv(), if *errores_abierto => match dato {
                    Some(dato) => {
                        bufer_errores.push_str(&String::from_utf8_lossy(&dato));
                        if excede_limite(bufer_errores) {
                            desbordado.store(true, Ordering::Relaxed);
                            bufer_salida.clear();
                            bufer_errores.clear();
                            return Some(Linea::DemasiadoLarga);
                        }
                    }
                    None => {
                        *errores_abierto = false;
                        if let Some(linea) = extraer_linea(bufer_errores, true) {
                            anotar_diagnostico(diagnostico, &linea);
                            return Some(Linea::Error(linea));
                        }
                    }
                },
            }
        }
    }
}

/// Extrae la primera línea completa y la quita del búfer; con flush, también lo
/// que quede sin salto de línea (fin del flujo).
fn extraer_linea(bufer: &mut String, flush: bool) -> Option<String> {
    match bufer.find('\n') {
        Some(posicion) => {
            let mut linea: String = bufer.drain(..=posicion).collect();
            linea.pop();
            if linea.ends_with('\r') {
                linea.pop();
            }
            Some(linea)
        }
        None if flush && !bufer.is_empty() => Some(std::mem::take(bufer)),
        None => None,
    }
}

/// ¿La parte sin terminar del búfer pasa del máximo de una línea? Las líneas
/// completas ya extraídas no cuentan: el búfer solo acumula la que está a medias.
fn excede_limite(bufer: &str) -> bool {
    let desde = bufer.rfind('\n').map_or(0, |posicion| posicion + 1);
    bufer.len() - desde > LIMITE_LINEA
}

fn anotar_diagnostico(diagnostico: &mut String, linea: &str) {
    if diagnostico.len() < LIMITE_DIAGNOSTICO {
        diagnostico.push_str(linea);
        diagnostico.push('\n');
    }
}

/// Entrada estándar y parada de un canal exec.
struct ControlCanal {
    entrada: Option<mpsc::UnboundedSender<Vec<u8>>>,
    fin: Option<oneshot::Receiver<()>>,
    cierre: Option<oneshot::Sender<()>>,
}

impl ControlCanal {
    fn escribir(&mut self, datos: &[u8]) -> Result<(), ErrorEscritorio> {
        let Some(entrada) = self.entrada.as_ref() else {
            return Err(ErrorEscritorio::Conexion(
                "la entrada del script de voxl ya está cerrada".to_string(),
            ));
        };
        entrada
            .send(datos.to_vec())
            .map_err(|_| ErrorEscritorio::Conexion("se perdió el canal con voxl".to_string()))
    }

    fn cerrar_entrada(&mut self) {
        self.entrada = None;
    }

    async fn esperar_fin(&mut self, plazo: Duration) -> bool {
        let Some(fin) = self.fin.take() else {
            return true;
        };
        tokio::time::timeout(plazo, fin).await.is_ok()
    }

    fn forzar_cierre(&mut self) {
        if let Some(cierre) = self.cierre.take() {
            let _ = cierre.send(());
        }
    }
}

// ---------------------------------------------------------------- sesión

/// Estado de una sesión de voxl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoVoxl {
    /// Arrancada: /healthz responde y el túnel escucha.
    Abierto,
    /// Se está cerrando: ya se ha mandado el fin de la entrada.
    Cerrando,
    /// Cerrada a petición de MAGI.
    Cerrado { motivo: String },
    /// Caída: voxl terminó o se perdió la conexión con el host.
    Caido { motivo: String },
}

impl EstadoVoxl {
    pub fn esta_abierto(&self) -> bool {
        matches!(self, EstadoVoxl::Abierto)
    }

    pub fn caido(&self) -> bool {
        matches!(self, EstadoVoxl::Caido { .. })
    }

    pub fn motivo(&self) -> Option<&str> {
        match self {
            EstadoVoxl::Cerrado { motivo } | EstadoVoxl::Caido { motivo } => Some(motivo),
            EstadoVoxl::Abierto | EstadoVoxl::Cerrando => None,
        }
    }
}

/// Sesión de voxl arrancada por SSH: el túnel de la sesión, la contraseña y la
/// forma de pararla.
///
/// La URL que se lleva al navegador es http://127.0.0.1:<puerto_tunel>/?token=<clave>;
/// este túnel no se guarda en TUNELES ni sale en la vista Túneles.
pub struct SesionVoxl {
    /// Puerto libre del túnel de la sesión en 127.0.0.1.
    pub puerto_tunel: u16,
    /// Contraseña aleatoria de voxl (32 hex). Nunca va al log ni a un mensaje.
    pub clave: Zeroizing<String>,
    /// Versión de voxl que se arrancó (p. ej. «0.3.3»).
    pub version: String,
    config: ConfigVoxl,
    estado: Arc<Mutex<EstadoVoxl>>,
    conexiones: Arc<AtomicUsize>,
    parada: Option<oneshot::Sender<()>>,
    control: ControlCanal,
}

impl SesionVoxl {
    /// Puerto de voxl en el host.
    pub fn puerto_remoto(&self) -> u16 {
        self.config.puerto
    }

    /// Estado actual de la sesión.
    pub fn estado(&self) -> EstadoVoxl {
        self.estado.lock().expect("estado de voxl").clone()
    }

    /// Conexiones vivas del túnel ahora mismo (la gracia de voxl se basa en
    /// esto: sin ninguna durante gracia_seg, la sesión se cierra).
    pub fn conexiones_abiertas(&self) -> usize {
        self.conexiones.load(Ordering::SeqCst)
    }

    /// URL de la página de voxl a través del túnel de la sesión.
    pub fn url(&self) -> Zeroizing<String> {
        Zeroizing::new(format!(
            "http://127.0.0.1:{}/?token={}",
            self.puerto_tunel,
            self.clave.as_str()
        ))
    }

    /// Para la sesión: fin de la entrada estándar (el script manda TERM y, a los
    /// 5 s, KILL) y cierre del túnel. Idempotente y con plazo; si la conexión SSH
    /// ya se cayó, la sesión queda caída con su motivo.
    pub async fn parar(&mut self) -> Result<(), ErrorEscritorio> {
        if let Some(parada) = self.parada.take() {
            let _ = parada.send(());
        }
        {
            let mut estado = self.estado.lock().expect("estado de voxl");
            if matches!(*estado, EstadoVoxl::Abierto) {
                *estado = EstadoVoxl::Cerrando;
            }
        }
        self.control.cerrar_entrada();
        if !self.control.esperar_fin(PLAZO_FIN_CANAL).await {
            self.control.forzar_cierre();
            let _ = self.control.esperar_fin(Duration::from_secs(2)).await;
        }
        {
            let mut estado = self.estado.lock().expect("estado de voxl");
            if matches!(*estado, EstadoVoxl::Abierto | EstadoVoxl::Cerrando) {
                *estado = EstadoVoxl::Cerrado {
                    motivo: "cerrado por MAGI".to_string(),
                };
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for SesionVoxl {
    fn fmt(&self, formato: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // La clave de voxl nunca se escribe, ni siquiera en un mensaje de error.
        formato
            .debug_struct("SesionVoxl")
            .field("puerto_tunel", &self.puerto_tunel)
            .field("puerto_remoto", &self.config.puerto)
            .field("version", &self.version)
            .field("clave", &"<oculta>")
            .field("estado", &self.estado())
            .finish()
    }
}

impl Drop for SesionVoxl {
    fn drop(&mut self) {
        // Red de seguridad: al soltar la sesión se cierra la entrada (el script
        // para voxl) y el túnel. Lo normal es haber llamado a parar.
        self.control.cerrar_entrada();
        self.parada.take();
    }
}

// ---------------------------------------------------------------- arranque

/// Arranca voxl en el host y deja abierta la sesión con su túnel.
///
/// 1. Valida pantalla, puerto y programa en Rust (el script lo repite).
/// 2. Abre un canal exec con sh -c "$SCRIPT" magi-voxl ... y le pasa una
///    contraseña aleatoria por la entrada estándar.
/// 3. Espera VOXL <versión> (exige 0.3 o posterior) y LISTO <pid>.
/// 4. Sondea GET /healthz por un direct-tcpip hasta 10 s.
/// 5. Abre el túnel de la sesión 127.0.0.1:<libre> hacia 127.0.0.1:<puerto>.
pub async fn arrancar(
    config: ConfigVoxl,
    host: Arc<dyn HostSsh>,
    opciones: OpcionesEscritorio,
) -> Result<SesionVoxl, ErrorEscritorio> {
    let pantalla = opciones
        .pantalla
        .clone()
        .unwrap_or_else(|| ":0".to_string());
    let programa = opciones
        .programa
        .clone()
        .unwrap_or_else(|| "voxl".to_string());
    validar_argumentos(&config, &pantalla, &programa)?;

    let clave = clave_aleatoria()?;
    let argumentos = argumentos_voxl(&pantalla, config.puerto, &programa, opciones.portapapeles);
    let mut canal = host.exec("sh", &argumentos).await?;

    // La contraseña va por la entrada estándar, nunca en la orden. Si la
    // escritura falla es que el script ya terminó: se traduce al leer.
    let _ = canal.escribir(clave.as_bytes());
    let _ = canal.escribir(b"\n");

    let version = match leer_version(&mut canal, &host, &config, &pantalla).await {
        Ok(version) => version,
        Err(error) => return Err(fallar(canal, error).await),
    };
    if let Err(error) = leer_listo(&mut canal, &host, &config, &pantalla).await {
        return Err(fallar(canal, error).await);
    }
    if let Err(error) = esperar_salud(&host, &config, &pantalla, &mut canal).await {
        return Err(fallar(canal, error).await);
    }

    let conexiones = Arc::new(AtomicUsize::new(0));
    let (puerto_tunel, parada) =
        match abrir_tunel(host.clone(), config.puerto, conexiones.clone()).await {
            Ok(tunel) => tunel,
            Err(error) => return Err(fallar(canal, error).await),
        };

    let estado = Arc::new(Mutex::new(EstadoVoxl::Abierto));
    let (control, lector) = canal.separar();
    tokio::spawn(monitorear(lector, host, estado.clone()));
    Ok(SesionVoxl {
        puerto_tunel,
        clave,
        version,
        config,
        estado,
        conexiones,
        parada: Some(parada),
        control,
    })
}

/// Cierra la entrada del canal para no dejar voxl vivo y devuelve el error.
async fn fallar(mut canal: CanalExec, error: ErrorEscritorio) -> ErrorEscritorio {
    canal.cerrar_entrada();
    if !canal.esperar_fin(PLAZO_FIN_CANAL).await {
        canal.forzar_cierre();
        let _ = canal.esperar_fin(Duration::from_secs(2)).await;
    }
    error
}

/// Vigila el canal ya arrancado: si se cierra sin que MAGI lo haya pedido, la
/// sesión queda caída con el motivo (voxl terminó o se perdió la conexión).
async fn monitorear(
    mut lector: LectorCanal,
    host: Arc<dyn HostSsh>,
    estado: Arc<Mutex<EstadoVoxl>>,
) {
    loop {
        match lector.siguiente_linea().await {
            None => break,
            Some(Linea::DemasiadoLarga) => {
                // El host mandó más de lo que cabe en una línea: no se sigue
                // leyendo y la sesión queda caída con un motivo fijo.
                *estado.lock().expect("estado de voxl") = EstadoVoxl::Caido {
                    motivo: motivo_datos_sin_fin(),
                };
                return;
            }
            Some(_) => {}
        }
    }
    // La caída del SSH cierra el canal un instante antes de que russh marque el
    // mango como cerrado: se espera un poco para distinguirla de un voxl que
    // termina por su cuenta.
    for _ in 0..10 {
        if !host.conexion_viva() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut actual = estado.lock().expect("estado de voxl");
    match &*actual {
        EstadoVoxl::Cerrando => {
            *actual = EstadoVoxl::Cerrado {
                motivo: "cerrado por MAGI".to_string(),
            };
        }
        EstadoVoxl::Cerrado { .. } | EstadoVoxl::Caido { .. } => {}
        EstadoVoxl::Abierto => {
            *actual = if host.conexion_viva() {
                EstadoVoxl::Caido {
                    motivo: "voxl terminó".to_string(),
                }
            } else {
                EstadoVoxl::Caido {
                    motivo: "la conexión con el host se perdió".to_string(),
                }
            };
        }
    }
}

/// Abre el túnel de la sesión: escucha en 127.0.0.1:<libre> y lleva cada conexión
/// a 127.0.0.1:<puerto_destino> del host por direct-tcpip.
async fn abrir_tunel(
    host: Arc<dyn HostSsh>,
    puerto_destino: u16,
    conexiones: Arc<AtomicUsize>,
) -> Result<(u16, oneshot::Sender<()>), ErrorEscritorio> {
    let escucha = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|error| {
        ErrorEscritorio::Conexion(format!(
            "no se pudo abrir el túnel de voxl en 127.0.0.1: {error}"
        ))
    })?;
    let puerto = escucha
        .local_addr()
        .map_err(|error| {
            ErrorEscritorio::Conexion(format!(
                "no se pudo leer el puerto del túnel de voxl: {error}"
            ))
        })?
        .port();
    let (parada, senal) = oneshot::channel();
    tokio::spawn(async move {
        let mut senal = senal;
        loop {
            tokio::select! {
                _ = &mut senal => return,
                aceptada = escucha.accept() => {
                    let Ok((mut flujo, _origen)) = aceptada else {
                        return;
                    };
                    let host = host.clone();
                    let conexiones = conexiones.clone();
                    tokio::spawn(async move {
                        let Ok(mut remoto) = host.direct_tcpip("127.0.0.1", puerto_destino).await
                        else {
                            // El host no abre ese destino: cae esta conexión del
                            // navegador, no el túnel entero.
                            tracing::warn!(
                                puerto = puerto_destino,
                                "el host rechazó una conexión del túnel de voxl"
                            );
                            return;
                        };
                        let _ = flujo.set_nodelay(true);
                        conexiones.fetch_add(1, Ordering::SeqCst);
                        let _ = tokio::io::copy_bidirectional(&mut flujo, &mut remoto).await;
                        conexiones.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            }
        }
    });
    Ok((puerto, parada))
}

// ---------------------------------------------------------------- esperas

/// Lo que devuelve una lectura del canal con plazo.
enum Lectura {
    Linea(Linea),
    Plazo,
    Fin,
}

async fn esperar_linea(canal: &mut CanalExec, plazo: Duration) -> Lectura {
    match tokio::time::timeout(plazo, canal.siguiente_linea()).await {
        Ok(Some(linea)) => Lectura::Linea(linea),
        Ok(None) => Lectura::Fin,
        Err(_) => Lectura::Plazo,
    }
}

fn restante(deadline: tokio::time::Instant) -> Result<Duration, ErrorEscritorio> {
    let restante = deadline.saturating_duration_since(tokio::time::Instant::now());
    if restante.is_zero() {
        Err(no_respondio())
    } else {
        Ok(restante)
    }
}

/// Espera la línea VOXL <versión> y comprueba que es 0.3 o posterior.
async fn leer_version(
    canal: &mut CanalExec,
    host: &Arc<dyn HostSsh>,
    config: &ConfigVoxl,
    pantalla: &str,
) -> Result<String, ErrorEscritorio> {
    let deadline = tokio::time::Instant::now() + PLAZO_ARRANQUE;
    loop {
        let plazo = restante(deadline)?;
        match esperar_linea(canal, plazo).await {
            Lectura::Linea(Linea::Salida(linea)) => {
                let Some(resto) = linea.strip_prefix("VOXL ") else {
                    continue;
                };
                let Some(version) = leer_version_voxl(resto) else {
                    return Err(ErrorEscritorio::Protocolo(
                        "no se pudo leer la versión de voxl".to_string(),
                    ));
                };
                if (version.0, version.1) < (0, 3) {
                    return Err(ErrorEscritorio::Remoto(
                        "actualiza voxl a la 0.3 o posterior".to_string(),
                    ));
                }
                return Ok(format!("{}.{}.{}", version.0, version.1, version.2));
            }
            Lectura::Linea(Linea::Error(linea)) => {
                if let Some(error) = traducir_error(&linea, config, pantalla) {
                    return Err(error);
                }
            }
            Lectura::Linea(Linea::DemasiadoLarga) => return Err(datos_sin_fin()),
            Lectura::Fin => return Err(motivo_cierre(host, canal, config, pantalla)),
            Lectura::Plazo => return Err(no_respondio()),
        }
    }
}

/// Espera la línea LISTO <pid>.
async fn leer_listo(
    canal: &mut CanalExec,
    host: &Arc<dyn HostSsh>,
    config: &ConfigVoxl,
    pantalla: &str,
) -> Result<(), ErrorEscritorio> {
    let deadline = tokio::time::Instant::now() + PLAZO_ARRANQUE;
    loop {
        let plazo = restante(deadline)?;
        match esperar_linea(canal, plazo).await {
            Lectura::Linea(Linea::Salida(linea)) => {
                if linea.starts_with("LISTO ") {
                    return Ok(());
                }
            }
            Lectura::Linea(Linea::Error(linea)) => {
                if let Some(error) = traducir_error(&linea, config, pantalla) {
                    return Err(error);
                }
            }
            Lectura::Linea(Linea::DemasiadoLarga) => return Err(datos_sin_fin()),
            Lectura::Fin => return Err(motivo_cierre(host, canal, config, pantalla)),
            Lectura::Plazo => return Err(no_respondio()),
        }
    }
}

/// Sondea GET /healthz por un direct-tcpip hasta que responde 200, con el plazo
/// de PLAZO_ARRANQUE. Mientras espera, sigue leyendo el stderr del script: si
/// voxl murió (pantalla que no existe, puerto ocupado), se traduce su error y se
/// falla antes de agotar el plazo.
async fn esperar_salud(
    host: &Arc<dyn HostSsh>,
    config: &ConfigVoxl,
    pantalla: &str,
    canal: &mut CanalExec,
) -> Result<(), ErrorEscritorio> {
    let deadline = tokio::time::Instant::now() + PLAZO_ARRANQUE;
    let mut salud = Box::pin(bucle_salud(host.clone(), config.puerto));
    loop {
        let plazo = restante(deadline)?;
        tokio::select! {
            resultado = &mut salud => return resultado,
            linea = canal.siguiente_linea() => match linea {
                Some(Linea::Error(linea)) => {
                    if let Some(error) = traducir_error(&linea, config, pantalla) {
                        return Err(error);
                    }
                }
                Some(Linea::Salida(_)) => {}
                Some(Linea::DemasiadoLarga) => return Err(datos_sin_fin()),
                None => return Err(motivo_cierre(host, canal, config, pantalla)),
            },
            _ = tokio::time::sleep(plazo) => return Err(no_respondio()),
        }
    }
}

/// Reintenta /healthz hasta que responde 200 o se pierde la conexión.
async fn bucle_salud(host: Arc<dyn HostSsh>, puerto: u16) -> Result<(), ErrorEscritorio> {
    loop {
        if !host.conexion_viva() {
            return Err(ErrorEscritorio::Conexion(
                "la conexión con el host se perdió".to_string(),
            ));
        }
        if intento_salud(host.as_ref(), puerto).await {
            return Ok(());
        }
        tokio::time::sleep(PAUSA_SALUD).await;
    }
}

/// Un intento de GET /healthz; false si el puerto aún no responde o la respuesta
/// no es 200 (se reintenta dentro del plazo total).
async fn intento_salud(host: &dyn HostSsh, puerto: u16) -> bool {
    let Ok(mut flujo) = host.direct_tcpip("127.0.0.1", puerto).await else {
        return false;
    };
    let peticion =
        format!("GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{puerto}\r\nConnection: close\r\n\r\n");
    let escritura = async {
        flujo.write_all(peticion.as_bytes()).await?;
        flujo.flush().await
    };
    if !matches!(
        tokio::time::timeout(PLAZO_INTENTO_SALUD, escritura).await,
        Ok(Ok(()))
    ) {
        return false;
    }
    let mut respuesta = Vec::new();
    let lectura = async {
        let mut bufer = [0u8; 512];
        loop {
            match flujo.read(&mut bufer).await {
                Ok(0) | Err(_) => return,
                Ok(leidos) => {
                    respuesta.extend_from_slice(&bufer[..leidos]);
                    if respuesta.windows(4).any(|trozo| trozo == b"\r\n\r\n")
                        || respuesta.len() >= LIMITE_DIAGNOSTICO
                    {
                        return;
                    }
                }
            }
        }
    };
    let _ = tokio::time::timeout(PLAZO_INTENTO_SALUD, lectura).await;
    es_respuesta_200(&respuesta)
}

/// ¿La respuesta HTTP empieza por un estado 200?
fn es_respuesta_200(respuesta: &[u8]) -> bool {
    let texto = String::from_utf8_lossy(respuesta);
    let Some(primera) = texto.lines().next() else {
        return false;
    };
    let mut partes = primera.split_whitespace();
    let _protocolo = partes.next();
    partes.next() == Some("200")
}

/// Motivo de un canal que se cierra antes de estar listo: se traduce el stderr
/// del script y, si no dice nada reconocible, se distingue la caída de la
/// conexión del cierre de voxl.
fn motivo_cierre(
    host: &Arc<dyn HostSsh>,
    canal: &CanalExec,
    config: &ConfigVoxl,
    pantalla: &str,
) -> ErrorEscritorio {
    if let Some(error) = traducir_error(canal.diagnostico(), config, pantalla) {
        return error;
    }
    if !host.conexion_viva() {
        return ErrorEscritorio::Conexion("la conexión con el host se perdió".to_string());
    }
    ErrorEscritorio::Remoto(format!(
        "voxl terminó al arrancar en {}",
        config.nombre_legible()
    ))
}

fn no_respondio() -> ErrorEscritorio {
    ErrorEscritorio::Conexion(format!(
        "voxl no respondió en {} s",
        PLAZO_ARRANQUE.as_secs()
    ))
}

/// Motivo legible de un flujo del host que no termina una línea dentro del
/// máximo: un host comprometido no puede agotar la memoria del servidor.
fn motivo_datos_sin_fin() -> String {
    format!(
        "el host mandó datos de voxl sin fin (más de {} MB)",
        LIMITE_LINEA / (1024 * 1024)
    )
}

fn datos_sin_fin() -> ErrorEscritorio {
    ErrorEscritorio::Protocolo(motivo_datos_sin_fin())
}

// ---------------------------------------------------------------- traducciones

/// Traduce el stderr del script (y de voxl) a los mensajes del §7.14. Son
/// mensajes fijos de MAGI: del host solo se miran palabras clave, nunca se
/// enseña su salida tal cual.
fn traducir_error(texto: &str, config: &ConfigVoxl, pantalla: &str) -> Option<ErrorEscritorio> {
    let texto = texto.to_ascii_lowercase();
    if texto.contains("error sin_voxl") {
        return Some(ErrorEscritorio::Remoto(format!(
            "voxl no está instalado en {}: instala voxl-server",
            config.nombre_legible()
        )));
    }
    if texto.contains("error pantalla") || texto.contains("error programa") {
        return Some(ErrorEscritorio::Conexion(
            "pantalla o programa inválidos".to_string(),
        ));
    }
    if texto.contains("error puerto") || texto.contains("error portapapeles") {
        return Some(ErrorEscritorio::Conexion(
            "el puerto o el portapapeles de voxl no son válidos".to_string(),
        ));
    }
    if texto.contains("error sin_clave") {
        return Some(ErrorEscritorio::Protocolo(
            "el script de voxl no recibió la contraseña".to_string(),
        ));
    }
    if texto.contains("address already in use")
        || texto.contains("eaddrinuse")
        || texto.contains("os error 98")
    {
        return Some(ErrorEscritorio::Remoto(format!(
            "el puerto {} del host está ocupado: cambia el puerto de voxl en la ficha",
            config.puerto
        )));
    }
    if texto.contains("cannot open display")
        || texto.contains("x connect")
        || texto.contains("failed to connect to display")
        || texto.contains("display")
    {
        return Some(ErrorEscritorio::Remoto(format!(
            "la pantalla {pantalla} no existe"
        )));
    }
    None
}

/// Valida en Rust lo mismo que el script valida antes de lanzar nada.
fn validar_argumentos(
    config: &ConfigVoxl,
    pantalla: &str,
    programa: &str,
) -> Result<(), ErrorEscritorio> {
    if !pantalla_valida(pantalla) || !programa_valido(programa) {
        return Err(ErrorEscritorio::Conexion(
            "pantalla o programa inválidos".to_string(),
        ));
    }
    if config.puerto < PUERTO_MIN {
        return Err(ErrorEscritorio::Conexion(format!(
            "el puerto de voxl debe estar entre {PUERTO_MIN} y 65535: \
             cambia el puerto de voxl en la ficha"
        )));
    }
    Ok(())
}

/// Argumentos de sh: el script y los cuatro datos, siempre como argumentos (el
/// script los lee de $1 a $4), nunca interpolados en el texto.
fn argumentos_voxl(pantalla: &str, puerto: u16, programa: &str, portapapeles: bool) -> Vec<String> {
    vec![
        "-c".to_string(),
        SCRIPT_VOXL.to_string(),
        "magi-voxl".to_string(),
        pantalla.to_string(),
        puerto.to_string(),
        programa.to_string(),
        portapapeles.to_string(),
    ]
}

/// Contraseña aleatoria de 32 hex (16 bytes de getrandom), en Zeroizing.
fn clave_aleatoria() -> Result<Zeroizing<String>, ErrorEscritorio> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ErrorEscritorio::Conexion(format!("no se pudo generar la contraseña de voxl: {error}"))
    })?;
    let mut clave = Zeroizing::new(String::with_capacity(32));
    {
        let texto: &mut String = &mut clave;
        for byte in bytes {
            let _ = write!(texto, "{byte:02x}");
        }
    }
    Ok(clave)
}

/// Lee la versión de voxl de una línea como «voxl 0.3.3» (o «0.3»).
fn leer_version_voxl(texto: &str) -> Option<(u32, u32, u32)> {
    for token in texto.split_whitespace() {
        let token = token.trim_start_matches('v');
        let mut partes = token.split('.');
        let Some(mayor) = partes.next().and_then(|parte| parte.parse::<u32>().ok()) else {
            continue;
        };
        let Some(menor) = partes.next().and_then(|parte| parte.parse::<u32>().ok()) else {
            continue;
        };
        let parche = partes
            .next()
            .and_then(|parte| parte.parse::<u32>().ok())
            .unwrap_or(0);
        return Some((mayor, menor, parche));
    }
    None
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn config_de_prueba(puerto: u16) -> ConfigVoxl {
        ConfigVoxl {
            nombre: "web-01".to_string(),
            puerto,
        }
    }

    #[test]
    fn los_argumentos_validos_se_aceptan() {
        assert!(validar_argumentos(&config_de_prueba(6901), ":0", "voxl").is_ok());
        assert!(validar_argumentos(&config_de_prueba(6901), ":12.0", "/usr/bin/voxl").is_ok());
    }

    #[test]
    fn los_argumentos_invalidos_se_rechazan() {
        let error = validar_argumentos(&config_de_prueba(6901), ":1; rm -rf", "voxl")
            .expect_err("pantalla inválida");
        assert_eq!(error.motivo(), "pantalla o programa inválidos");
        let error = validar_argumentos(&config_de_prueba(6901), ":1", "voxl --foo")
            .expect_err("programa inválido");
        assert_eq!(error.motivo(), "pantalla o programa inválidos");
        let error =
            validar_argumentos(&config_de_prueba(80), ":1", "voxl").expect_err("puerto reservado");
        assert!(error.motivo().contains("1024"), "{}", error.motivo());
    }

    #[test]
    fn el_script_va_como_argumento_de_sh_y_escapado() {
        let comando = comando_remoto("sh", &argumentos_voxl(":1", 6901, "/usr/bin/voxl", true));
        assert!(comando.starts_with("sh -c '"), "{comando}");
        assert!(comando.contains("magi-voxl"), "{comando}");
        assert!(comando.contains("--addr=127.0.0.1"), "{comando}");
        assert!(comando.contains("6901"), "{comando}");
        assert!(comando.contains("/usr/bin/voxl"), "{comando}");
        assert!(!comando.contains("VOXL_PASSWORD="), "{comando}");
    }

    #[test]
    fn la_version_se_lee_y_se_compara() {
        assert_eq!(leer_version_voxl("voxl 0.3.3"), Some((0, 3, 3)));
        assert_eq!(leer_version_voxl("0.4"), Some((0, 4, 0)));
        assert_eq!(leer_version_voxl("v1.2.0-rc1"), Some((1, 2, 0)));
        assert_eq!(leer_version_voxl("voxl"), None);
    }

    #[test]
    fn los_errores_del_script_se_traducen() {
        let config = config_de_prueba(6901);
        let error = traducir_error("ERROR sin_voxl", &config, ":1").expect("sin voxl");
        assert_eq!(
            error.motivo(),
            "voxl no está instalado en web-01: instala voxl-server"
        );
        let error = traducir_error("X connect: X11 error: no such display", &config, ":1")
            .expect("pantalla");
        assert_eq!(error.motivo(), "la pantalla :1 no existe");
        let error = traducir_error(
            "bind: Os { code: 98, kind: AddrInUse, message: \"Address already in use\" }",
            &config,
            ":1",
        )
        .expect("puerto ocupado");
        assert_eq!(
            error.motivo(),
            "el puerto 6901 del host está ocupado: cambia el puerto de voxl en la ficha"
        );
        assert_eq!(
            traducir_error("ERROR pantalla", &config, ":1")
                .expect("pantalla inválida")
                .motivo(),
            "pantalla o programa inválidos"
        );
        assert!(traducir_error("algo sin relación", &config, ":1").is_none());
    }

    #[test]
    fn la_clave_es_aleatoria_y_de_32_hex() {
        let una = clave_aleatoria().expect("clave");
        let otra = clave_aleatoria().expect("clave");
        assert_eq!(una.len(), 32);
        assert!(una.chars().all(|caracter| caracter.is_ascii_hexdigit()));
        assert_ne!(*una, *otra);
    }

    #[test]
    fn las_lineas_incompletas_se_parten_al_cerrar_el_flujo() {
        let mut bufer = String::from("primera\nsegun");
        assert_eq!(extraer_linea(&mut bufer, false).as_deref(), Some("primera"));
        assert_eq!(extraer_linea(&mut bufer, false), None);
        assert_eq!(extraer_linea(&mut bufer, true).as_deref(), Some("segun"));
    }

    #[test]
    fn la_cota_de_linea_solo_cuenta_lo_que_esta_a_medias() {
        assert!(!excede_limite(&"a".repeat(LIMITE_LINEA)));
        assert!(excede_limite(&"a".repeat(LIMITE_LINEA + 1)));
        // Las líneas ya completas no cuentan: solo la que está sin terminar.
        let mut con_saltos = "a".repeat(LIMITE_LINEA);
        con_saltos.push('\n');
        con_saltos.push('b');
        assert!(!excede_limite(&con_saltos));
    }

    #[tokio::test]
    async fn una_linea_sin_fin_se_corta_y_avisa_al_canal() {
        let (salida, salida_rx) = mpsc::unbounded_channel();
        let (_errores, errores_rx) = mpsc::unbounded_channel();
        let mut lector = LectorCanal::nuevo(salida_rx, errores_rx);
        let mut enviado = 0;
        while enviado <= LIMITE_LINEA {
            salida
                .send(vec![b'A'; 64 * 1024])
                .expect("enviando la línea sin fin");
            enviado += 64 * 1024;
        }
        match lector.siguiente_linea().await {
            Some(Linea::DemasiadoLarga) => {}
            _ => panic!("el lector debe cortar la línea sin fin"),
        }
        assert!(
            lector.marca_desborde().load(Ordering::Relaxed),
            "el canal debe enterarse del corte"
        );
    }
}
