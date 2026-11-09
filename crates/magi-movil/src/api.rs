//! Contrato de la API que la app Android consume por UniFFI (informe §5.1).
//!
//! Los DTO de este módulo son propios de la puerta móvil: no se reexportan
//! tipos de magi-nucleo (el núcleo no depende de UniFFI). Las funciones de red
//! son async y en Kotlin se convierten en funciones suspend.
//!
//! La puerta está implementada sobre magi-nucleo: inventario de solo lectura,
//! sincronización en modo lector, clave del móvil por Firmante, sesiones con
//! terminal por tramos, túneles y diálogos. Los DTO de UniFFI son propios y no
//! dependen de los tipos internos del núcleo.

use std::sync::Arc;

use crate::retrollamadas::{CarpetaMovil, FirmanteMovil, LlaveroMovil, OyenteNucleo};

// ===========================================================================
// Errores
// ===========================================================================

/// Error único de la puerta móvil. En Kotlin se genera como excepción con una
/// subclase por variante; el mensaje de thiserror es el que ve el usuario.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ErrorNucleo {
    /// El núcleo no está iniciado todavía.
    #[error("el núcleo no está iniciado")]
    NoIniciado,
    /// El elemento pedido no existe (host, sesión, túnel, conflicto).
    #[error("no existe: {detalle}")]
    NoEncontrado { detalle: String },
    /// La operación no vale en el estado actual (sesión ya abierta, etc.).
    #[error("estado no válido: {detalle}")]
    EstadoInvalido { detalle: String },
    /// La carpeta compartida no se puede usar (permiso retirado, sin ficheros).
    #[error("carpeta: {detalle}")]
    Carpeta { detalle: String },
    /// Falla del llavero de la plataforma (Keystore).
    #[error("llavero: {detalle}")]
    Llavero { detalle: String },
    /// Falla de la clave del móvil (Keystore inválido, teléfono bloqueado).
    #[error("clave del móvil: {detalle}")]
    Firmante { detalle: String },
    /// El usuario canceló el diálogo.
    #[error("cancelado")]
    Cancelado,
    /// Operación en curso; se ignora (p. ej. una pasada de sincronización).
    #[error("ocupado: {detalle}")]
    Ocupado { detalle: String },
    /// La conexión automática no puede abrir un diálogo y la huella del host no
    /// está aceptada todavía (informe §7.2). La instrucción dice qué hacer.
    #[error("huella sin aceptar: conéctate una vez desde la ficha")]
    HuellaSinAceptar,
    /// La conexión automática no tiene ni la clave del móvil instalada en el
    /// host ni una contraseña guardada (informe §7.2).
    #[error("sin credenciales: instala tu clave o guarda la contraseña")]
    SinCredenciales,
    /// El host no contesta dentro del plazo (DNS, TCP, saludo o comando).
    #[error("el host no responde")]
    NoResponde,
    /// El host rechazó la contraseña de sudo: se ha olvidado y hay que pedirla
    /// otra vez (informe §7.5).
    #[error("sudo rechazó la contraseña: olvídala y pídela otra vez")]
    SudoRechazada,
    /// El usuario no puede usar sudo o no tiene permiso para el diario del
    /// sistema o para el motor de contenedores (informe §6.3).
    #[error("sin permiso: pide la contraseña de sudo")]
    SinPermiso,
    /// Cualquier otro error con su motivo legible.
    #[error("{detalle}")]
    General { detalle: String },
}

// ===========================================================================
// Arranque
// ===========================================================================

/// Rutas del móvil que la app calcula dentro de su almacenamiento privado.
/// El núcleo no lee el entorno: recibe las rutas ya resueltas.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Rutas {
    /// Directorio de datos (magi.db, known_hosts privado, temporales).
    pub datos: String,
    /// Directorio de configuración.
    pub config: String,
    /// Directorio de estado (logs).
    pub estado: String,
    /// Directorio hogar privado de la app (raíz de .ssh/known_hosts).
    pub hogar: String,
    /// Directorio de ejecución; vacío en el móvil, que no tiene servidor.
    pub runtime: String,
}

/// Identidad del equipo en la sincronización: modelo del teléfono y android.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Equipo {
    pub nombre: String,
    pub plataforma: String,
}

/// Retrollamadas que implementa Kotlin y que el núcleo usa durante toda la vida
/// de la app (informe §5.2).
#[derive(uniffi::Record)]
pub struct Retrollamadas {
    pub carpeta: Arc<dyn CarpetaMovil>,
    pub llavero: Arc<dyn LlaveroMovil>,
    pub firmante: Arc<dyn FirmanteMovil>,
    pub oyente: Arc<dyn OyenteNucleo>,
}

/// Versión del núcleo, formato de sincronización y user_version de magi.db.
/// La versión de la app la pone Kotlin desde BuildConfig (la app no pasa su
/// versión al núcleo en el contrato de §5.1).
#[derive(Debug, Clone, uniffi::Record)]
pub struct VersionNucleo {
    pub nucleo: String,
    pub formato: u32,
    pub user_version: u32,
}

// ===========================================================================
// Inventario (solo lectura en el móvil)
// ===========================================================================

/// Grupo del inventario con su número de hosts y el plegado propio del móvil.
#[derive(Debug, Clone, uniffi::Record)]
pub struct GrupoInventario {
    pub uuid: String,
    pub nombre: String,
    pub num_hosts: u32,
    pub plegado: bool,
}

/// Estado de un host en la lista (glifos de §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoHost {
    /// Sin sesión: glifo ○.
    SinSesion,
    /// Conectando: glifo ◐.
    Conectando,
    /// Con sesiones abiertas: glifo ●N.
    ConSesiones,
    /// Último intento con error: glifo ✕.
    Error,
}

/// Tipo de host del inventario (Fase 12). Los que no son `ssh` son
/// escritorios: el móvil los enseña pero no los abre ni les crea túneles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TipoHostMovil {
    Ssh,
    Rdp,
    Vnc,
    Voxl,
}

/// Fila de host de la pantalla Hosts.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HostInventario {
    pub uuid: String,
    pub nombre: String,
    pub direccion: String,
    pub puerto: u32,
    /// Tipo de host: SSH o un escritorio (RDP, VNC o voxl).
    pub tipo: TipoHostMovil,
    pub usuario: String,
    pub grupo_uuid: Option<String>,
    pub etiquetas: Vec<String>,
    pub estado: EstadoHost,
    pub sesiones_abiertas: u32,
    /// Último error legible del host (campo propio del móvil), si lo hubo.
    pub ultimo_error: Option<String>,
    /// Instante de la última conexión correcta (epoch en segundos).
    pub ultima_conexion_en: Option<i64>,
}

/// Huella conocida de un host en el known_hosts privado del móvil.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HuellaHost {
    pub tipo: String,
    pub sha256: String,
}

/// Ficha de host de solo lectura (informe §6.2).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FichaHost {
    pub uuid: String,
    pub nombre: String,
    pub direccion: String,
    pub puerto: u32,
    /// Tipo de host: SSH o un escritorio (RDP, VNC o voxl).
    pub tipo: TipoHostMovil,
    pub usuario: String,
    pub grupo_uuid: Option<String>,
    pub grupo_nombre: Option<String>,
    pub etiquetas: Vec<String>,
    pub salto_uuid: Option<String>,
    pub salto_nombre: Option<String>,
    /// Keepalive del inventario en segundos; None = el móvil aplica 30 s (D180).
    pub keepalive_segundos: Option<u32>,
    /// Identidad del escritorio (auto, agente:..., fichero:...), solo informativa.
    pub identidad_escritorio: Option<String>,
    pub huellas: Vec<HuellaHost>,
    /// Si hay contraseña guardada en este móvil para el host.
    pub tiene_contrasena: bool,
    /// Bloque FLOTA de la ficha (informe §6.2): métricas completas, vigilados
    /// con su estado, fallos con nombres y hora y duración del último sondeo.
    pub flota: Option<DetalleFlota>,
    /// Verificaciones previas del host (informe §6.2): de solo lectura, se
    /// editan en el escritorio.
    pub verificaciones: Option<VerificacionesMovil>,
    /// Snippet que se escribe al conectar (informe §6.2 y §7.9): de solo
    /// lectura, se elige en el escritorio.
    pub snippet_al_conectar: Option<SnippetAlConectarMovil>,
}

// ===========================================================================
// Sincronización (lector)
// ===========================================================================

/// Fase del estado de sincronización (diagrama de §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FaseSincronizacion {
    SinConfigurar,
    SinClave,
    Activa,
    Sincronizando,
    EsperandoClave,
    ErrorCarpeta,
}

/// Estado de otro equipo en la pantalla Sincronización (F10 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoEquipo {
    EsteEquipo,
    AlDia,
    Ilegible,
    Danado,
    FormatoNuevo,
    Quitado,
}

/// Un equipo de la carpeta compartida, tal como lo enseña Ajustes.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EquipoSincronizacion {
    pub nombre: String,
    pub plataforma: String,
    pub version: String,
    pub ultimo_fichero: Option<String>,
    pub estado: EstadoEquipo,
    /// Instante de la última novedad (epoch en segundos).
    pub ultima_vez: Option<i64>,
}

/// Estado completo de la sincronización en el móvil.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EstadoSincronizacion {
    pub fase: FaseSincronizacion,
    /// URI SAF del árbol elegido, para enseñarlo en Ajustes.
    pub carpeta_uri: Option<String>,
    /// Nombre legible de la carpeta (si se conoce).
    pub carpeta_nombre: Option<String>,
    /// Huella corta de la clave de sincronización (p. ej. 7f3a·91c2).
    pub huella_corta_clave: Option<String>,
    /// Instante de la última pasada (epoch en segundos).
    pub ultima_pasada: Option<i64>,
    pub equipos: Vec<EquipoSincronizacion>,
    /// Número de conflictos de huella pendientes de resolver.
    pub conflictos_huella: u32,
    /// Error legible de la última pasada, si lo hubo.
    pub error: Option<String>,
}

/// Conflicto de huella de un host que llega de otro equipo (informe §3).
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConflictoHuella {
    pub id: u64,
    pub host_uuid: String,
    pub host_nombre: String,
    pub direccion: String,
    /// Huella que tiene el móvil (o ninguna, cadena vacía).
    pub huella_mia: String,
    /// Huella que propone el otro equipo.
    pub huella_otra: String,
    pub equipo_origen: Option<String>,
}

// ===========================================================================
// Sesiones y terminal
// ===========================================================================

/// Estado de una sesión en memoria (diagrama de §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoSesion {
    Abriendo,
    Abierta,
    Caida,
}

/// Entrada de la lista completa de sesiones que recibe OyenteNucleo::sesiones.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Sesion {
    pub id: u64,
    pub host_uuid: String,
    pub host_nombre: String,
    pub estado: EstadoSesion,
    /// Fase del abriendo (resolviendo, conectando, verificando huella) o el
    /// motivo legible de la caída.
    pub detalle: Option<String>,
    /// Instante de apertura (epoch en segundos).
    pub abierta_en: Option<i64>,
}

/// Color de un tramo de pantalla (informe §7.7).
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum Color {
    Predeterminado,
    /// Índice de la paleta de 256 colores.
    Indice(u8),
    /// Color de 24 bits.
    Rgb(u8, u8, u8),
}

/// Tramo de texto con el mismo estilo (informe §7.7); un carácter ancho ocupa
/// dos celdas.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Tramo {
    pub texto: String,
    pub primer_plano: Color,
    pub fondo: Color,
    pub negrita: bool,
    pub cursiva: bool,
    pub subrayado: bool,
    pub inverso: bool,
}

/// Fila de pantalla que cambió, con sus tramos.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FilaPantalla {
    pub fila: u32,
    pub tramos: Vec<Tramo>,
}

/// Posición de una celda del terminal (columna, fila), base 0.
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct Posicion {
    pub columna: u32,
    pub fila: u32,
}

/// Pantalla por tramos que recibe OyenteNucleo::pantalla (informe §7.7): solo
/// las filas que cambiaron y, como mucho, cada 33 ms.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Pantalla {
    pub sesion: u64,
    pub columnas: u32,
    pub filas: u32,
    pub filas_cambiadas: Vec<FilaPantalla>,
    /// Cursor visible; None si el remoto lo oculta.
    pub cursor: Option<Posicion>,
    /// Filas de historial por encima (0 = al final).
    pub desplazamiento: u32,
    /// True al abrir, al redimensionar o al volver a la vista.
    pub completa: bool,
}

/// Modificadores de una pulsación de tecla (informe §7.6).
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct Modificadores {
    pub ctrl: bool,
    pub alt: bool,
    pub mayus: bool,
}

/// Tecla especial o carácter que la app manda al núcleo; el núcleo la codifica
/// según el modo del terminal (informe §7.6).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum Tecla {
    Escape,
    Tab,
    Enter,
    Retroceso,
    Arriba,
    Abajo,
    Izquierda,
    Derecha,
    Inicio,
    Fin,
    RePag,
    AvPag,
    Supr,
    Insert,
    /// F1-F12.
    F(u8),
    /// Un carácter (para Ctrl+letra, Alt+tecla, etc.).
    Caracter(String),
}

// ===========================================================================
// Túneles
// ===========================================================================

/// Tipo de túnel (mecánica del escritorio, F5 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TipoTunel {
    Local,
    Remoto,
    Dinamico,
}

/// Estado vivo de un túnel en el móvil (diagrama de §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoTunel {
    Inactivo,
    Activando,
    Activo,
    Caido,
}

/// Túnel del inventario con su estado en vivo y sus contadores.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TunelVivo {
    pub uuid: String,
    pub host_uuid: String,
    pub host_nombre: String,
    pub nombre: String,
    pub tipo: TipoTunel,
    /// Dirección de escucha del inventario, tal cual viene (p. ej. 0.0.0.0:8080).
    pub escucha: String,
    /// True si la dirección del inventario no era 127.0.0.1/localhost: el móvil
    /// escucha igualmente en 127.0.0.1 y la fila lo avisa (D181).
    pub escucha_aviso: bool,
    pub destino: String,
    pub automatico: bool,
    pub estado: EstadoTunel,
    /// Último error legible, si el túnel está caído.
    pub error: Option<String>,
    /// Conexiones abiertas ahora mismo.
    pub conexiones: u32,
    /// Conexiones aceptadas desde que se abrió el túnel (acumulado).
    pub aceptadas: u32,
    /// Último error de una conexión suelta, aunque el túnel siga activo.
    pub ultimo_error: Option<String>,
    /// Instante (epoch en segundos) del último error.
    pub ultimo_error_en: Option<i64>,
    pub bytes_subida: u64,
    pub bytes_bajada: u64,
    /// Puerto en el que quedó escuchando cuando se abrió en un puerto libre.
    pub puerto_efectivo: Option<u32>,
}

// ===========================================================================
// Diálogos
// ===========================================================================

/// Petición de diálogo que el núcleo manda a Kotlin por
/// OyenteNucleo::dialogo (informe §6.6 y §7.1). El id correlaciona la respuesta
/// con la operación que espera (una petición que ya no está en vuelo se
/// descarta).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum PeticionDialogo {
    /// Huella desconocida: aceptar o cancelar.
    HuellaNueva {
        id: u64,
        host_uuid: String,
        host_nombre: String,
        /// Tipo de la clave (p. ej. ed25519).
        tipo: String,
        sha256: String,
    },
    /// Huella cambiada: bloqueo en rojo; sustituir exige escribir el nombre.
    HuellaCambiada {
        id: u64,
        host_uuid: String,
        host_nombre: String,
        tipo: String,
        anterior: String,
        nueva: String,
        /// El nombre del host que hay que escribir para confirmar.
        nombre_a_escribir: String,
    },
    /// Contraseña de host (3 intentos, con opción de recordarla en el móvil).
    Contrasena {
        id: u64,
        host_uuid: String,
        host_nombre: String,
        usuario: String,
        direccion: String,
        intentos_restantes: u32,
        /// True si el host tiene identidad de contraseña y se ofrece recordar.
        recordar_ofrecido: bool,
    },
}

/// Respuesta a un diálogo (Nucleo::responder_dialogo). Los bytes de la
/// contraseña se borran en el núcleo tras usarlos.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum RespuestaDialogo {
    AceptarHuella,
    CancelarHuella,
    SustituirHuella { nombre_escrito: String },
    Contrasena { bytes: Vec<u8>, recordar: bool },
    CancelarContrasena,
}

// ===========================================================================
// Clave del móvil y datos
// ===========================================================================

/// Clave pública de la clave del móvil, a partir de la X.509 que da
/// FirmanteMovil (informe §7.2).
#[derive(Debug, Clone, uniffi::Record, PartialEq, Eq)]
pub struct ClavePublicaMovil {
    /// Línea OpenSSH completa (ecdsa-sha2-nistp256 AAAA... comentario).
    pub linea_openssh: String,
    /// Huella SHA256 en formato OpenSSH (SHA256:...).
    pub huella_sha256: String,
}

/// Resultado de «instalar mi clave en este host» (informe §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResultadoInstalacionClave {
    YaEstaba,
    InstaladaYAceptada,
    InstaladaPeroNoAceptada,
}

/// Alcance de «borrar todos los datos de MAGI» (informe §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AlcanceBorrado {
    /// Lo que hace «dejar de sincronizar»: clave de sincronización e inventario.
    Sincronizacion,
    /// Todo: inventario, clave de sincronización, contraseñas, huellas, REGISTRO
    /// y la clave del móvil.
    Todo,
}

// ===========================================================================
// Ajustes del móvil (informe §5.4)
// ===========================================================================

/// Ajustes del móvil que en el escritorio viven en `config.toml` y aquí vienen
/// de DataStore (informe §5.4). La app los manda con `aplicar_ajustes`; el
/// núcleo los acota a los rangos de siempre y devuelve los avisos.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AjustesMovil {
    /// Auto-refresco de Flota: 0 (apagado) o 15-300 s; por defecto 30.
    pub flota_refresco_seg: u64,
    /// Umbral de carga por núcleo; por defecto 1,0.
    pub umbral_carga_por_nucleo: f64,
    /// Umbral de memoria en porcentaje; por defecto 90.
    pub umbral_memoria_pct: f64,
    /// Umbral de disco en porcentaje; por defecto 90.
    pub umbral_disco_pct: f64,
    /// Refresco de la vista Servicios: 0 (solo al entrar) o 5-300 s; por
    /// defecto 10.
    pub servicios_refresco_seg: u64,
    /// Espera máxima al estado final de una acción: 5-300 s; por defecto 30.
    pub servicios_espera_seg: u64,
    /// Motores de contenedores, sin repetir y en orden de preferencia;
    /// vacío = sin contenedores. Por defecto ["docker", "podman"].
    pub servicios_contenedores: Vec<String>,
    /// Minutos sin uso tras los que se olvida la contraseña de sudo: 0-15
    /// (0 = pedirla siempre); por defecto 5.
    pub sudo_memoria_min: u64,
    /// BALTHASAR-2: antigüedad máxima del último backup en horas; por defecto
    /// 24.
    pub deliberacion_backup_horas: u64,
    /// MELCHIOR-1: antigüedad máxima del último sondeo en minutos; por defecto
    /// 5.
    pub deliberacion_salud_max_min: u64,
}

// ===========================================================================
// Flota (informe §5.1, §6.1 y §6.2)
// ===========================================================================

/// Estado de un host en Flota (informe §6.1). Los cinco primeros salen del
/// último sondeo; `Sondeando` y `SinSondeo` son propios del móvil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoFlotaMovil {
    /// Nunca se ha sondeado (glifo ○ y «FRÍA»).
    Fria,
    /// Sondeo en curso (glifo ◐).
    Sondeando,
    /// El último sondeo falló (glifo ✕ y «CAÍDA»).
    Caida,
    /// Responde pero sin métricas («ALCANZ.»).
    Alcanzable,
    /// Algún umbral superado o un vigilado caído («CARGA»).
    Carga,
    /// Todo dentro de los umbrales («NOMINAL»).
    Nominal,
    /// Host que no se sondea (no es SSH): se abre en el ordenador.
    SinSondeo,
}

/// Resumen de métricas de una fila de Flota (segunda línea, informe §6.1).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ResumenMetricas {
    pub nucleos: Option<u32>,
    pub carga_1m: Option<f64>,
    pub memoria_pct: Option<f64>,
    pub disco_pct: Option<f64>,
    /// Tasa de red de bajada en bytes por segundo (contra el sondeo anterior).
    pub red_rx_bps: Option<f64>,
    /// Tasa de red de subida en bytes por segundo (contra el sondeo anterior).
    pub red_tx_bps: Option<f64>,
    pub uptime_seg: Option<i64>,
}

/// Fila de la lista de Flota (informe §6.1).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HostFlota {
    pub uuid: String,
    pub nombre: String,
    pub direccion: String,
    pub puerto: u32,
    pub tipo: TipoHostMovil,
    pub grupo_uuid: Option<String>,
    pub etiquetas: Vec<String>,
    pub estado: EstadoFlotaMovil,
    /// Culpables del estado CARGA («carga», «memoria», «disco» o unidades).
    pub culpables: Vec<String>,
    /// Resumen de métricas para la segunda línea; None si no hay sondeo.
    pub resumen: Option<ResumenMetricas>,
    /// Número de fallos del último sondeo (marca ⚠N).
    pub fallos: u32,
    /// Sesiones abiertas del host ahora mismo.
    pub sesiones: u32,
    /// Instante del último sondeo (época en segundos).
    pub ultimo_sondeo_en: Option<i64>,
    /// Motivo legible de la caída o de no tener sondeo.
    pub motivo: Option<String>,
    /// Duración del último sondeo en milisegundos.
    pub duracion_ms: Option<i64>,
}

/// Tipo de fallo del sondeo ampliado (informe §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TipoFalloMovil {
    Servicio,
    Contenedor,
}

/// Un fallo del último sondeo: una unidad fallida o un contenedor caído.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FalloFlota {
    pub tipo: TipoFalloMovil,
    /// Nombre de la unidad o del contenedor.
    pub nombre: String,
    /// Estado con el que se vio («fallido», «caído», «unhealthy»...).
    pub estado: String,
    /// Motor que vio el contenedor, si es un contenedor.
    pub motor: Option<MotorNombreMovil>,
}

/// Un servicio vigilado del host con su estado (bloque VIGILADOS, §6.2).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VigiladoFlota {
    pub nombre: String,
    /// Estado tal cual lo dio el host («active», «failed», «inactive»...).
    pub estado: String,
    /// True si el estado es «active» (el glifo lo pone la app).
    pub activo: bool,
}

/// Métricas completas del último sondeo (bloque FLOTA de la ficha, §6.2).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MetricasFlota {
    pub nucleos: Option<u32>,
    pub carga_1m: Option<f64>,
    pub carga_5m: Option<f64>,
    pub carga_15m: Option<f64>,
    pub memoria_total_kb: Option<i64>,
    pub memoria_disponible_kb: Option<i64>,
    pub memoria_pct: Option<f64>,
    pub disco_total_kb: Option<i64>,
    pub disco_usado_kb: Option<i64>,
    pub disco_pct: Option<f64>,
    pub red_rx_bytes: Option<i64>,
    pub red_tx_bytes: Option<i64>,
    pub red_rx_bps: Option<f64>,
    pub red_tx_bps: Option<f64>,
    pub uptime_seg: Option<i64>,
}

/// Detalle de Flota de la ficha (informe §6.2): métricas completas, vigilados
/// con su estado y fallos con nombres, más la hora y la duración del sondeo.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DetalleFlota {
    pub estado: EstadoFlotaMovil,
    pub culpables: Vec<String>,
    /// Motivo legible de la caída o de no tener sondeo.
    pub motivo: Option<String>,
    /// Métricas completas; None si no hay sondeo o no trajo métricas.
    pub metricas: Option<MetricasFlota>,
    /// Servicios vigilados del host con su estado.
    pub vigilados: Vec<VigiladoFlota>,
    /// Fallos del último sondeo con sus nombres.
    pub fallos: Vec<FalloFlota>,
    /// Instante del último sondeo (época en segundos).
    pub ultimo_sondeo_en: Option<i64>,
    /// Duración del último sondeo en milisegundos.
    pub duracion_ms: Option<i64>,
    /// Aviso de datos del sondeo ignorados (p. ej. unidades vigiladas que no
    /// casan con el patrón válido).
    pub aviso: Option<String>,
}

/// Verificaciones previas de un host (informe §6.2 y §7.11). El móvil las lee
/// al deliberar y al actuar; nunca las escribe: se editan en el escritorio.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VerificacionesMovil {
    /// MELCHIOR-1: comprobar la salud del host.
    pub salud: bool,
    /// BALTHASAR-2: comprobar la antigüedad del último backup.
    pub backup: bool,
    /// Directorio remoto de los backups.
    pub backup_ruta: Option<String>,
    /// Glob opcional sobre el nombre de fichero (`*.sql.gz`).
    pub backup_patron: Option<String>,
    /// CASPER-3: comprobar los tests (solo en el escritorio).
    pub tests: bool,
    /// Comando local (`sh -c`) que dice si los tests están en verde.
    pub tests_comando: Option<String>,
}

/// Snipet que se escribe al conectar con un host (informe §6.2 y §7.9). El
/// móvil vuelve a comprobarlo antes de escribirlo: si es crítico o tiene
/// variables, no se escribe y se avisa en la sesión.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SnippetAlConectarMovil {
    /// uuid de sincronización del snippet.
    pub uuid: String,
    pub nombre: String,
    /// True si el móvil lo puede escribir (no es crítico y no tiene variables).
    pub valido: bool,
}

// ===========================================================================
// Servicios (informe §5.1, §6.3 y §6.4)
// ===========================================================================

/// Tipo de objetivo de una acción o de un detalle (espejo del núcleo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TipoObjetivoMovil {
    Unidad,
    Timer,
    Contenedor,
    Proyecto,
}

impl TipoObjetivoMovil {
    /// Texto del tipo de objetivo, el mismo del núcleo. Es un método Rust: no
    /// cruza UniFFI.
    pub fn texto(self) -> &'static str {
        match self {
            TipoObjetivoMovil::Unidad => "unidad",
            TipoObjetivoMovil::Timer => "timer",
            TipoObjetivoMovil::Contenedor => "contenedor",
            TipoObjetivoMovil::Proyecto => "proyecto",
        }
    }
}

/// Objetivo concreto: tipo y nombre (unidad, timer, contenedor o proyecto).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ObjetivoMovil {
    pub tipo: TipoObjetivoMovil,
    pub nombre: String,
}

/// Acción sobre un servicio, un timer, un contenedor o un proyecto (§5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AccionMovil {
    Arrancar,
    Parar,
    Reiniciar,
    Recargar,
    /// Activar el arranque al inicio.
    Activar,
    /// Desactivar el arranque al inicio.
    Desactivar,
    /// Lanzar ahora el servicio de un timer.
    Lanzar,
}

impl AccionMovil {
    /// Texto de la acción en infinitivo, el mismo del núcleo. No cruza UniFFI.
    pub fn texto(self) -> &'static str {
        match self {
            AccionMovil::Arrancar => "arrancar",
            AccionMovil::Parar => "parar",
            AccionMovil::Reiniciar => "reiniciar",
            AccionMovil::Recargar => "recargar",
            AccionMovil::Activar => "activar",
            AccionMovil::Desactivar => "desactivar",
            AccionMovil::Lanzar => "lanzar",
        }
    }

    /// Texto en pasado, para el registro y los mensajes («reiniciado»).
    pub fn participio(self) -> &'static str {
        match self {
            AccionMovil::Arrancar => "arrancado",
            AccionMovil::Parar => "parado",
            AccionMovil::Reiniciar => "reiniciado",
            AccionMovil::Recargar => "recargado",
            AccionMovil::Activar => "activado",
            AccionMovil::Desactivar => "desactivado",
            AccionMovil::Lanzar => "lanzado",
        }
    }
}

/// Privilegio del usuario SSH en el host (informe §7.3 de la Fase 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PrivilegioMovil {
    Root,
    SudoSinClave,
    SudoConClave,
    SinSudo,
}

/// Permiso para leer el diario del sistema (informe §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PermisoDiarioMovil {
    Todo,
    Propio,
}

/// Estado de systemd en el host (informe §7.3 de la Fase 13).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SystemdMovil {
    /// Versión de systemd (p. ej. «252»).
    Disponible(String),
    /// El host no tiene systemd.
    Ausente,
    /// systemd anterior a 230 (se ve la versión).
    Antiguo(String),
}

/// Motor de contenedores (Docker o Podman).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MotorNombreMovil {
    Docker,
    Podman,
}

/// Motor de contenedores con su versión.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MotorMovil {
    pub nombre: MotorNombreMovil,
    pub version: String,
}

/// Estado de los contenedores en el host (informe §6.3).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MotorEstadoMovil {
    /// No hay Docker ni Podman instalados.
    No,
    /// Hay motor pero el usuario no tiene permiso.
    SinPermiso,
    /// Hay motor y se puede usar.
    Disponible(MotorMovil),
}

/// Estado de arranque de una unidad (`UnitFileState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoArranqueMovil {
    Activado,
    Desactivado,
    Estatico,
    Indirecto,
    Generado,
    Transitorio,
    Alias,
    Enmascarado,
    Desconocido,
}

/// Reloj del host: época y uptime, para calcular «desde» y «hace» con su reloj.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct RelojHostMovil {
    /// Época en segundos según el host.
    pub epoch: i64,
    /// Uptime en segundos según el host.
    pub uptime_seg: u64,
}

/// Una propiedad `clave=valor` (`systemctl show` o las etiquetas de `inspect`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PropiedadMovil {
    pub clave: String,
    pub valor: String,
}

/// Unidad de systemd del listado, con las acciones que se le permiten y los
/// datos ya derivados por el núcleo (la app no lee las propiedades del host).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UnidadMovil {
    pub nombre: String,
    pub props: Vec<PropiedadMovil>,
    /// True si está en `HOSTS.servicios` (★ y arriba en la lista).
    pub vigilada: bool,
    /// Acciones permitidas en su estado actual; la app solo ofrece estas.
    pub acciones: Vec<AccionMovil>,
    /// `UnitFileState` ya parseado (activado, desactivado, estático...).
    pub arranque: EstadoArranqueMovil,
    /// Instante (época del host) en el que entró en su estado actual.
    pub desde_en: Option<i64>,
    /// PID principal, si está corriendo.
    pub pid: Option<u32>,
    /// Memoria actual del proceso principal, en bytes.
    pub memoria_bytes: Option<u64>,
    /// Número de tareas del cgroup (`TasksCurrent`), si se conoce.
    pub tareas: Option<u32>,
}

/// Timer de systemd del listado, con las acciones que se le permiten y los
/// instantes ya calculados con el reloj del host.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TimerMovil {
    pub nombre: String,
    pub props: Vec<PropiedadMovil>,
    /// Servicio que activa, si se conoce.
    pub servicio: Option<String>,
    pub acciones: Vec<AccionMovil>,
    /// Próxima ejecución (época del host), si se conoce.
    pub proxima_en: Option<i64>,
    /// Última ejecución (época del host), si se conoce.
    pub ultima_en: Option<i64>,
}

/// Estado de un contenedor (informe §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoContenedorMovil {
    Creado,
    EnMarcha,
    Parado,
    Reiniciando,
    Pausado,
    Muerto,
    Desconocido,
}

/// Salud de un contenedor con `healthcheck`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SaludMovil {
    Ninguna,
    Sana,
    Insegura,
    Comprobando,
}

/// Un contenedor del listado, con las acciones que se le permiten.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ContenedorMovil {
    pub id: String,
    pub nombre: String,
    pub imagen: String,
    pub estado: EstadoContenedorMovil,
    pub codigo_salida: Option<i32>,
    pub salud: SaludMovil,
    pub puertos: String,
    pub proyecto: Option<String>,
    pub ficheros: Vec<String>,
    pub servicio: Option<String>,
    pub orden: Option<String>,
    pub etiquetas: Vec<PropiedadMovil>,
    pub acciones: Vec<AccionMovil>,
}

/// Proyecto Compose del listado, con las acciones que se le permiten.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProyectoMovil {
    pub nombre: String,
    pub ficheros: Vec<String>,
    pub acciones: Vec<AccionMovil>,
}

/// Listado completo de servicios de un host (espejo del `Listado` del núcleo,
/// más si hay contraseña de sudo en memoria y la hora de la lectura).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ListadoServicios {
    pub systemd: SystemdMovil,
    pub privilegio: PrivilegioMovil,
    pub diario: PermisoDiarioMovil,
    pub reloj: RelojHostMovil,
    pub motor: MotorEstadoMovil,
    pub unidades: Vec<UnidadMovil>,
    pub timers: Vec<TimerMovil>,
    pub contenedores: Vec<ContenedorMovil>,
    pub proyectos: Vec<ProyectoMovil>,
    /// Avisos del host («sin permiso para Docker: …»).
    pub avisos: Vec<String>,
    /// Se recortó por los límites de entrada no fiable.
    pub incompleto: bool,
    /// True si hay contraseña de sudo en memoria para ese host.
    pub sudo_en_memoria: bool,
    /// Instante de la lectura (época en segundos); None si no se ha leído.
    pub hora: Option<i64>,
}

/// Consumo de un contenedor en marcha (`stats --no-stream`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Consumo {
    pub id: String,
    pub cpu_pct: Option<f64>,
    pub memoria_bytes: Option<u64>,
    pub memoria_limite_bytes: Option<u64>,
}

/// Datos de un contenedor leídos de `inspect` (pestaña ESTADO del detalle).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DetalleContenedorMovil {
    pub id: String,
    pub nombre: String,
    pub imagen: String,
    pub creado: String,
    pub estado: EstadoContenedorMovil,
    pub codigo_salida: Option<i32>,
    pub reinicios: u64,
    pub salud: SaludMovil,
    pub puertos: Vec<String>,
    pub montajes: Vec<String>,
    pub redes: Vec<String>,
    pub proyecto: Option<String>,
    pub orden: Option<String>,
    /// Texto de `inspect` (hasta 256 KiB).
    pub texto: String,
}

/// Detalle de un objetivo (informe §6.4): propiedades, dependencias y el texto
/// de `systemctl cat` o de `inspect`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DetalleServicios {
    pub objetivo: ObjetivoMovil,
    pub props: Vec<PropiedadMovil>,
    /// Dependencias que necesita.
    pub necesita: Vec<String>,
    /// Dependencias que la necesitan.
    pub necesitada: Vec<String>,
    /// Texto de la unidad o del `inspect`.
    pub texto: String,
    /// Para un contenedor, sus datos ya leídos del `inspect`.
    pub contenedor: Option<DetalleContenedorMovil>,
}

/// Lo que la app necesita saber antes de confirmar o deliberar una acción
/// (informe §4.3 y §6.4). Lo calcula el núcleo: la app no decide nada.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InfoAccionMovil {
    /// Si la acción se permite en el estado actual.
    pub permitida: bool,
    /// Por qué no se permite, si no se permite.
    pub motivo: Option<String>,
    /// Si la acción deja algo parado y exige deliberación MAGI.
    pub delibera: bool,
    /// Si hace falta la contraseña de sudo para ejecutarla.
    pub pide_sudo: bool,
    /// Comprobación de configuración que se aplicará antes, si la hay.
    pub comprobacion: Option<String>,
    /// True si el objetivo es una unidad crítica (puede cortar la conexión).
    pub critica: bool,
    /// Hosts que van a través de este (para el aviso de unidad crítica).
    pub hosts_criticos: Vec<String>,
}

/// Resultado de una acción de servicio (espejo de `ResultadoServicio` del
/// núcleo, más el contexto para poder pintarlo y difundirlo).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ResultadoServicioMovil {
    pub host_uuid: String,
    pub host_nombre: String,
    pub objetivo: ObjetivoMovil,
    pub accion: AccionMovil,
    pub resultado: ResultadoAccionMovil,
    /// Estado final del objetivo («active», «inactive», «running»...).
    pub estado: String,
    /// Segundos que tardó la acción en el host.
    pub segundos: f32,
    /// Mensaje de la orden (nunca el diario entero).
    pub mensaje: String,
    /// Últimas 20 líneas del diario o la salida de la comprobación.
    pub lineas: Vec<String>,
    /// La comprobación de configuración que se aplicó, si hubo.
    pub comprobacion: Option<String>,
}

/// Resultado de una acción (informe §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResultadoAccionMovil {
    Hecho,
    Fallido,
    /// Se acabó el plazo: la orden sigue en el host.
    SinConfirmar,
    /// La comprobación de configuración falló y no se ejecutó nada.
    RechazadaConfiguracion,
    /// Error del host o de la conexión.
    Error,
}

impl ResultadoAccionMovil {
    /// Texto del resultado, el mismo que se guarda en el núcleo. No cruza
    /// UniFFI.
    pub fn texto(self) -> &'static str {
        match self {
            ResultadoAccionMovil::Hecho => "hecho",
            ResultadoAccionMovil::Fallido => "fallido",
            ResultadoAccionMovil::SinConfirmar => "sin_confirmar",
            ResultadoAccionMovil::RechazadaConfiguracion => "rechazada_configuracion",
            ResultadoAccionMovil::Error => "error",
        }
    }
}

/// Fase de una acción de servicio en curso (informe §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FaseAccionMovil {
    Comprobando,
    Ejecutando,
    Esperando,
}

/// Una acción de servicio en curso, tal como la difunde
/// `OyenteNucleo.acciones_servicio` (informe §3.1).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccionServicioMovil {
    pub id: u64,
    pub host_uuid: Option<String>,
    pub host_nombre: String,
    pub objetivo: ObjetivoMovil,
    pub accion: AccionMovil,
    pub fase: FaseAccionMovil,
    /// Deliberación que la autoriza, si la hubo.
    pub deliberacion_id: Option<u64>,
    /// Instante de inicio (época local en milisegundos).
    pub desde_ms: i64,
}

/// Resultado de comprobar la contraseña de sudo (`servicios.sh sudo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResultadoSudo {
    /// Correcta: queda en memoria hasta que caduque o se olvide.
    Ok,
    /// El host la rechazó.
    Rechazada,
    /// El usuario no puede usar sudo en ese host.
    SinSudo,
}

/// Una fila de «Toda la flota»: un fallo del último sondeo de un host.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FilaTodaLaFlota {
    pub host_uuid: String,
    pub host_nombre: String,
    pub fallo: FalloFlota,
    /// Instante del sondeo del que sale el fallo (época en segundos).
    pub sondeo_en: Option<i64>,
}

/// Un host SSH en la lista de «Toda la flota», con su estado y sus fallos.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HostTodaLaFlota {
    pub host_uuid: String,
    pub host_nombre: String,
    pub estado: EstadoFlotaMovil,
    /// Motivo legible de la caída, del estado o de no tener sondeo.
    pub motivo: Option<String>,
    /// Número de fallos del último sondeo.
    pub fallos: u32,
    /// Instante del último sondeo (época en segundos).
    pub sondeo_en: Option<i64>,
}

/// «Toda la flota» (informe §5.1 y §6.4): los fallos del último sondeo de cada
/// host, con su hora, y los hosts caídos o sin sondeo. No abre conexiones.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TodaLaFlota {
    pub fallos: Vec<FilaTodaLaFlota>,
    pub hosts: Vec<HostTodaLaFlota>,
}

// ===========================================================================
// Logs (informe §5.1, §5.2 y §6.5)
// ===========================================================================

/// Filtro por nivel del panel de logs (informe §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NivelLogMovil {
    Todo,
    Info,
    Aviso,
    Error,
}

/// Estado de una suscripción de logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoSuscripcionLog {
    /// Llegan líneas.
    Siguiendo,
    /// Parada por la app (fin de la entrada estándar del script).
    Parada,
    /// El proceso remoto terminó solo.
    Terminada,
    /// Sin permiso para el diario del sistema.
    SinPermiso,
    /// Se cayó la conexión con el host.
    ConexionCaida,
    /// El objetivo ya no existe.
    NoEncontrada,
}

/// Motivo del fin de una suscripción de logs (espejo del núcleo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MotivoFinLog {
    /// La app la paró.
    Cerrado,
    /// El proceso remoto terminó solo.
    Terminado,
    /// El usuario no tiene permiso para el diario.
    SinPermiso,
    /// Se cayó la conexión con el host.
    ConexionCaida,
    /// El objetivo ya no existe.
    NoEncontrado,
}

/// Una línea del anillo de logs (informe §7.4).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LineaLogMovil {
    /// Número que no se repite: la vista en pausa se ancla a él.
    pub numero: u64,
    /// Hora en la época del host (segundos).
    pub hora: Option<i64>,
    /// Prioridad de journald (0 emerg … 7 debug); en contenedores, 6 o 3.
    pub prioridad: u8,
    /// True si llegó por la salida de errores (contenedores).
    pub stderr: bool,
    pub texto: String,
}

/// Filtro y ventana que pide la app para pintar los logs (informe §7.4).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FiltroLogs {
    /// Texto a buscar (vacío = sin filtro de texto).
    pub texto: String,
    pub nivel: NivelLogMovil,
    /// Solo líneas de la salida de errores (contenedores).
    pub solo_stderr: bool,
    /// Número de línea desde el que empezar (en pausa); None = al final.
    pub desde: Option<u64>,
    /// Líneas pedidas; el núcleo lo acota a 500.
    pub cuantas: u32,
}

/// Novedades de una suscripción que llegan por `OyenteNucleo.logs` (informe
/// §5.2), como mucho cada 100 ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct EstadoLogs {
    pub suscripcion: u64,
    /// Total de líneas en el anillo de esa suscripción.
    pub total: u64,
    /// Líneas nuevas desde el aviso anterior.
    pub nuevas: u64,
    /// Líneas descartadas (anillo o lectura del script) desde el anterior.
    pub omitidas: u64,
    pub estado: EstadoSuscripcionLog,
}

/// Ventana ya filtrada de hasta 500 líneas (informe §5.1 y §7.4).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VentanaLogs {
    pub lineas: Vec<LineaLogMovil>,
    /// Total de líneas que cumplen el filtro en el anillo.
    pub total: u64,
    /// Líneas omitidas que hay que enseñar donde faltan.
    pub omitidas: u64,
    pub estado: EstadoSuscripcionLog,
    /// Motivo del fin, si la suscripción ya terminó.
    pub motivo: Option<MotivoFinLog>,
}

// ===========================================================================
// Sesiones con orden (informe §5.1 y §7.3)
// ===========================================================================

/// Orden con la que se abre una pestaña desde Servicios (tipo cerrado del
/// núcleo, T108): `magi-movil` arma la orden real; la app nunca manda texto.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum OrdenPestanaMovil {
    /// Shell dentro de un contenedor.
    ShellContenedor { motor: MotorNombreMovil, id: String },
    /// Seguimiento del diario de una unidad.
    SeguirDiario { unidad: String },
    /// Seguimiento de los logs de un contenedor.
    SeguirContenedor { motor: MotorNombreMovil, id: String },
}

// ===========================================================================
// Snippets (informe §5.1, §6.8 y §7.6)
// ===========================================================================

/// Un host al que apunta un snippet (uuid de la sincronización y nombre).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HostDestinoMovil {
    pub uuid: String,
    pub nombre: String,
}

/// Un destino del snippet: una etiqueta (por nombre) o un host suelto.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DestinoMovil {
    Etiqueta(String),
    Host(HostDestinoMovil),
}

/// Una variable del comando de un snippet con su valor por defecto y el valor
/// que haya escrito el usuario.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VariableSnippetMovil {
    /// Nombre tal como aparece en el comando (`[a-z_][a-z0-9_]*`).
    pub nombre: String,
    /// Valor por defecto del snippet, si lo tiene.
    pub valor_defecto: Option<String>,
    /// Valor escrito por el usuario; None = usar el valor por defecto.
    pub valor: Option<String>,
}

/// Un snippet del inventario (solo lectura en el móvil), con sus destinos ya
/// resueltos y sus variables.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SnippetMovil {
    /// uuid de sincronización: identifica el snippet en toda la API.
    pub uuid: String,
    pub nombre: String,
    pub comando: String,
    pub descripcion: String,
    /// Clasificación propia del snippet (no son etiquetas de host).
    pub etiquetas: Vec<String>,
    /// Crítico: siempre delibera.
    pub critico: bool,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub usado_veces: u64,
    /// Instante del último uso (época en segundos); None si no se ha usado.
    pub ultimo_uso_en: Option<i64>,
    pub destinos: Vec<DestinoMovil>,
    /// Hosts a los que apunta, ya resueltos y en el orden de la vista Hosts.
    pub hosts: Vec<HostDestinoMovil>,
    pub variables: Vec<VariableSnippetMovil>,
    /// Destinos que son escritorios (RDP, VNC o voxl): no ejecutan snippets.
    pub escritorios_omitidos: u32,
}

/// Un error de validación de la preparación, en su campo (informe §4.5).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ErrorCampoMovil {
    /// Campo al que afecta: «snippet», «hosts» o el nombre de una variable.
    pub campo: String,
    pub mensaje: String,
}

/// Ejecución preparada por el núcleo (informe §4.5): el comando con las
/// variables ya sustituidas y escapadas, los hosts elegidos y si delibera.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PreparacionEjecucion {
    pub snippet_uuid: String,
    pub snippet_nombre: String,
    /// El comando con las variables ya sustituidas (escapadas).
    pub comando: String,
    pub hosts: Vec<HostDestinoMovil>,
    /// Variables con los valores usados en la sustitución.
    pub variables: Vec<VariableSnippetMovil>,
    /// Errores por campo; si hay alguno, la app no deja lanzar.
    pub errores: Vec<ErrorCampoMovil>,
    /// Si hace falta deliberar (crítico, varios hosts o verificaciones).
    pub delibera: bool,
    /// Motivos por los que delibera, ya legibles.
    pub motivos: Vec<String>,
    pub critico: bool,
    pub timeout_seg: u32,
    /// Valor inicial de «parar al primer fallo» (el del snippet).
    pub parar_al_fallo: bool,
    /// Valores de las variables, para repetir la ejecución.
    pub valores: Vec<PropiedadMovil>,
}

// ===========================================================================
// Ejecuciones de snippets (informe §5.1 y §6.9)
// ===========================================================================

/// Estado de un host dentro de una ejecución (diagrama de §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoHostEjecucionMovil {
    /// En espera de turno del semáforo.
    EnCola,
    /// Abriendo la conexión automática con el host.
    Conectando,
    /// Comando en marcha.
    Ejecutando,
    /// Terminó con código 0.
    Ok,
    /// Terminó con código distinto de 0.
    Fallo,
    /// No se pudo ejecutar (huella, credenciales, red, plazo).
    Error,
    /// Cancelada por el usuario.
    Cancelado,
    /// No llegó a ejecutarse por «parar al primer fallo» o por cancelar.
    Omitido,
}

/// Resultado final de una ejecución (informe §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResultadoEjecucionMovil {
    Ok,
    Parcial,
    Error,
    Cancelada,
}

/// Un host de una ejecución, con su estado y lo que duró y salió.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HostEjecucionMovil {
    pub host_uuid: String,
    pub host_nombre: String,
    pub estado: EstadoHostEjecucionMovil,
    /// Código de salida, si llegó a ejecutarse.
    pub codigo: Option<i32>,
    pub duracion_ms: i64,
    pub bytes_stdout: u64,
    pub bytes_stderr: u64,
    /// Motivo del error, si lo hubo (con instrucción cuando toca).
    pub error: Option<String>,
}

/// Una ejecución de un snippet en uno o varios hosts, sin la salida (informe
/// §5.1 y §6.9).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EjecucionMovil {
    pub id: u64,
    pub snippet_uuid: Option<String>,
    pub snippet_nombre: String,
    pub hosts: Vec<HostEjecucionMovil>,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    /// Deliberación que la autoriza, si la hubo.
    pub deliberacion_id: Option<u64>,
    /// Instante de creación (época en segundos).
    pub creada_en: i64,
    /// Instante de fin (época en segundos); None mientras siga en curso.
    pub terminada_en: Option<i64>,
    /// Resultado final; None mientras siga en curso.
    pub resultado: Option<ResultadoEjecucionMovil>,
    pub cancelada: bool,
    /// True si la deliberación que la autorizó fue forzada (marca ⚑).
    pub forzada: bool,
}

/// Salida de un host de una ejecución, sin secuencias de escape (informe §6.9).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SalidaEjecucion {
    pub host_uuid: String,
    pub host_nombre: String,
    pub codigo: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// True si stdout pasó de 1 MiB y se recortó.
    pub stdout_truncada: bool,
    /// True si stderr pasó de 1 MiB y se recortó.
    pub stderr_truncada: bool,
    pub duracion_ms: i64,
}

// ===========================================================================
// Deliberación (informe §5.1, §6.6, §6.7 y §7.7)
// ===========================================================================

/// Veredicto de una comprobación en un host (espejo del núcleo). Siempre con
/// el dato que lo justifica: nunca un ✕ sin motivo.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum VeredictoMovil {
    /// La comprobación no está activa en ese host (—).
    NoActiva,
    /// Aún comprobando (◐).
    Pendiente,
    Aprueba {
        detalle: String,
        ms: u64,
    },
    Rechaza {
        detalle: String,
        ms: u64,
    },
}

/// Una fila de la tabla de deliberación: un host y sus tres comprobaciones.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FilaHostDeliberacion {
    pub host_uuid: Option<String>,
    pub host_nombre: String,
    pub salud: VeredictoMovil,
    pub backup: VeredictoMovil,
    pub tests: VeredictoMovil,
}

/// Estado del diálogo MAGI (informe §6.6): APROBADO, BLOQUEADO o FORZADO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EstadoDeliberacion {
    Aprobado,
    Bloqueado,
    Forzado,
}

/// Resultado de `deliberar`: la tabla host × comprobación, el consenso y el
/// estado con el id de la deliberación en curso (informe §5.1).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TablaDeliberacion {
    /// Id de la deliberación en curso, el que se pasa a `resolver_deliberacion`.
    pub id: u64,
    pub filas: Vec<FilaHostDeliberacion>,
    pub aprobadas: u32,
    pub activas: u32,
    pub rechazos: u32,
    pub pendientes: u32,
    /// Unanimidad de las comprobaciones activas.
    pub unanime: bool,
    pub estado: EstadoDeliberacion,
}

/// Lo que se delibera en el móvil (informe §4.6): una ejecución de snippet o
/// una acción sobre un servicio.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PlanDeliberadoMovil {
    /// La ejecución preparada (comando resuelto y hosts elegidos).
    Snippet { preparacion: PreparacionEjecucion },
    /// Una acción sobre un objetivo de servicio.
    Servicio {
        /// uuid del host.
        host: String,
        objetivo: ObjetivoMovil,
        accion: AccionMovil,
        /// Comprobación de configuración que se aplicará antes, si la hay.
        comprobacion: Option<String>,
    },
}

/// Cómo se resuelve una deliberación (informe §5.1 y §6.6).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DecisionDeliberacion {
    Aprobar,
    /// Forzar con un motivo de 10 caracteres o más.
    Forzar {
        motivo: String,
    },
    Cancelar,
}

// ===========================================================================
// Registro (informe §5.1, §6.10 y §7.10)
// ===========================================================================

/// Resultado de una entrada del registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResultadoRegistroMovil {
    Ok,
    Error,
}

/// Filtros rápidos por tipo del registro (informe §7.10). La lista vacía en
/// `FiltroRegistro.tipos` equivale a «todos».
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TipoRegistroFiltro {
    Conexiones,
    Huellas,
    Tuneles,
    Sondeos,
    Snippets,
    Deliberaciones,
    Servicios,
    Claves,
    Sincronizacion,
}

/// Filtro de la pantalla Registro: host, tipos y texto (informe §6.10).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FiltroRegistro {
    /// uuid del host por el que filtrar; None = todos.
    pub host_uuid: Option<String>,
    /// Tipos por los que filtrar; vacío = todos.
    pub tipos: Vec<TipoRegistroFiltro>,
    /// Texto a buscar en el detalle (vacío = sin filtro).
    pub texto: String,
}

/// Una entrada del historial de MAGI (informe §6.10).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EntradaRegistro {
    pub id: u64,
    /// Instante de la entrada (época en segundos).
    pub fecha: i64,
    /// Tipo de evento (`conexion_abierta`, `servicio_accion`...).
    pub tipo: String,
    pub host_uuid: Option<String>,
    pub host_nombre: Option<String>,
    pub resultado: ResultadoRegistroMovil,
    pub detalle: String,
}

/// Una deliberación antigua, con sus comprobaciones, para el detalle de una
/// entrada `deliberacion_*` (informe §7.10).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DetalleDeliberacion {
    pub id: u64,
    /// Instante de la deliberación (época en segundos).
    pub fecha: i64,
    /// «reiniciar app.service → web-01».
    pub accion: String,
    pub resultado: ResultadoDeliberacionMovil,
    /// Hubo al menos un rechazo.
    pub bloqueada: bool,
    /// Motivo del forzado, si se forzó.
    pub motivo: Option<String>,
    /// Usuario de auditoría («móvil (<equipo>)»).
    pub usuario: String,
    /// Cómo acabó la ejecución; None si no consta.
    pub ejecucion_resultado: Option<ResultadoEjecucionMovil>,
    /// True si la deliberación se resolvió pero no consta su ejecución (MAGI
    /// se cerró durante la ejecución).
    pub sin_resultado: bool,
    pub filas: Vec<FilaHostDeliberacion>,
    pub aprobadas: u32,
    pub activas: u32,
}

/// Cómo se resolvió una deliberación antigua.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ResultadoDeliberacionMovil {
    Aprobada,
    Forzada,
    Cancelada,
}

/// Detalle completo de una entrada del registro (informe §7.10), con las
/// comprobaciones de su deliberación si es una `deliberacion_*`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DetalleRegistro {
    pub entrada: EntradaRegistro,
    pub deliberacion: Option<DetalleDeliberacion>,
}

// ===========================================================================
// El objeto que consume la app
// ===========================================================================

/// Puerta del núcleo móvil. La app tiene una sola instancia, creada con
/// Nucleo::iniciar al arrancar.
#[derive(uniffi::Object)]
pub struct Nucleo {
    estado: Arc<crate::estado::EstadoNucleo>,
}

/// Funciones síncronas del contrato (informe §5.1).
#[uniffi::export]
impl Nucleo {
    /// Abre o crea magi.db (migraciones 1-7) y arranca el runtime y el hilo
    /// escritor. La app lo llama una vez, al arrancar, tras desbloquear.
    #[uniffi::constructor]
    pub fn iniciar(
        rutas: Rutas,
        equipo: Equipo,
        retrollamadas: Retrollamadas,
    ) -> Result<Arc<Self>, ErrorNucleo> {
        let rutas_nucleo = magi_nucleo::rutas::Rutas::nuevas(
            rutas.datos.into(),
            rutas.config.into(),
            rutas.estado.into(),
            rutas.hogar.into(),
            rutas.runtime.into(),
        );
        let equipo_nucleo = magi_nucleo::rutas::Equipo::nuevo(equipo.nombre, equipo.plataforma);
        let estado =
            crate::estado::EstadoNucleo::nuevo(rutas_nucleo, equipo_nucleo, retrollamadas)?;
        // La purga del registro de más de 90 días sigue como en la F11 y se
        // hace al arrancar (informe §7.10). Un fallo de la purga no impide
        // arrancar: el registro se queda como estaba y se reintenta al volver.
        let corte = (chrono::Local::now() - chrono::Duration::days(90))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string();
        let _ = estado
            .escritor
            .con_almacen(move |almacen| almacen.purgar_registro(&corte));
        let ya_anotada = estado
            .escritor
            .con_almacen(|almacen| {
                almacen
                    .conexion()
                    .query_row(
                        "SELECT COUNT(*) FROM REGISTRO WHERE tipo = 'clave_generada'",
                        [],
                        |fila| fila.get::<_, i64>(0),
                    )
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        if ya_anotada == 0 && estado.clave_publica().is_ok() {
            crate::sincro::anotar_clave_generada(&estado);
        }
        Ok(Arc::new(Self { estado }))
    }

    /// Versión del núcleo, formato de sincronización y user_version.
    pub fn version(&self) -> VersionNucleo {
        VersionNucleo {
            nucleo: magi_nucleo::VERSION_NUCLEO.to_string(),
            formato: magi_nucleo::sincronizacion::formato::FORMATO_MAXIMO,
            user_version: magi_nucleo::almacen::migraciones::VERSION_BD as u32,
        }
    }

    /// Grupos con número de hosts y plegado.
    pub fn inventario_grupos(&self) -> Result<Vec<GrupoInventario>, ErrorNucleo> {
        crate::inventario::grupos(&self.estado)
    }

    /// Hosts con estado, sesiones abiertas y etiquetas; filtro por subcadena.
    pub fn inventario_hosts(
        &self,
        filtro: Option<String>,
    ) -> Result<Vec<HostInventario>, ErrorNucleo> {
        crate::inventario::hosts(&self.estado, filtro.as_deref())
    }

    /// Ficha de solo lectura, huellas del móvil y si hay contraseña guardada.
    pub fn ficha_host(&self, uuid: String) -> Result<FichaHost, ErrorNucleo> {
        crate::inventario::ficha(&self.estado, &uuid)
    }

    /// Guarda el plegado de un grupo (campo propio del móvil; no viaja).
    pub fn plegar_grupo(&self, uuid: String, plegado: bool) -> Result<(), ErrorNucleo> {
        crate::inventario::plegar(&self.estado, &uuid, plegado)
    }

    /// Estado de la sincronización en el móvil.
    pub fn sincronizacion_estado(&self) -> Result<EstadoSincronizacion, ErrorNucleo> {
        crate::sincro::estado_publico(&self.estado)
    }

    /// Conflictos de huella pendientes de resolver en el móvil.
    pub fn conflictos_huella(&self) -> Result<Vec<ConflictoHuella>, ErrorNucleo> {
        crate::sincro::conflictos_huella(&self.estado)
    }

    /// Escribe bytes tal cual en el PTY de la sesión.
    pub fn escribir(&self, sesion: u64, bytes: Vec<u8>) -> Result<(), ErrorNucleo> {
        crate::gestor::escribir(&self.estado, sesion, bytes)
    }

    /// Codifica una tecla con sus modificadores y la escribe en la sesión.
    pub fn tecla(
        &self,
        sesion: u64,
        tecla: Tecla,
        modificadores: Modificadores,
    ) -> Result<(), ErrorNucleo> {
        crate::gestor::tecla(&self.estado, sesion, tecla, modificadores)
    }

    /// Pega texto (pegado entre corchetes si el remoto lo activó).
    pub fn pegar(&self, sesion: u64, texto: String) -> Result<(), ErrorNucleo> {
        crate::gestor::pegar(&self.estado, sesion, texto)
    }

    /// ¿El remoto de la sesión activó el pegado entre corchetes (modo 2004)?
    pub fn pegado_entre_corchetes(&self, sesion: u64) -> Result<bool, ErrorNucleo> {
        crate::gestor::pegado_entre_corchetes(&self.estado, sesion)
    }

    /// Redimensiona el PTY (saneado a mínimo 2x1; agrupado 50 ms).
    pub fn redimensionar(&self, sesion: u64, columnas: u32, filas: u32) -> Result<(), ErrorNucleo> {
        crate::gestor::redimensionar(&self.estado, sesion, columnas, filas)
    }

    /// Desplaza el historial (positivo = hacia arriba).
    pub fn desplazar(&self, sesion: u64, filas: i32) -> Result<(), ErrorNucleo> {
        crate::gestor::desplazar(&self.estado, sesion, filas)
    }

    /// Texto seleccionado entre dos celdas de la pantalla (para copiar).
    pub fn texto_seleccion(
        &self,
        sesion: u64,
        desde: Posicion,
        hasta: Posicion,
    ) -> Result<String, ErrorNucleo> {
        crate::gestor::texto_seleccion(&self.estado, sesion, desde, hasta)
    }

    /// Cierra una sesión y libera sus recursos.
    pub fn cerrar_sesion(&self, sesion: u64) -> Result<(), ErrorNucleo> {
        crate::gestor::cerrar_sesion(&self.estado, sesion)
    }

    /// Túneles del inventario con su estado en vivo; host_uuid None = todos.
    pub fn tuneles(&self, host_uuid: Option<String>) -> Result<Vec<TunelVivo>, ErrorNucleo> {
        crate::gestor::tuneles(&self.estado, host_uuid.as_deref())
    }

    /// Para un túnel activo (la tarea de parada corre en el runtime del núcleo).
    pub fn parar_tunel(&self, uuid: String) -> Result<(), ErrorNucleo> {
        let estado_tarea = self.estado.clone();
        let manejador = self.estado.runtime.handle().clone();
        manejador.spawn(async move {
            let _ = crate::gestor::parar_tunel(&estado_tarea, &uuid).await;
        });
        Ok(())
    }

    /// Responde a un diálogo abierto (huella nueva, cambiada o contraseña).
    pub fn responder_dialogo(
        &self,
        id: u64,
        respuesta: RespuestaDialogo,
    ) -> Result<(), ErrorNucleo> {
        self.estado.dialogos.responder(id, respuesta);
        Ok(())
    }

    /// Línea OpenSSH y huella SHA256 de la clave del móvil.
    pub fn clave_publica_movil(&self) -> Result<ClavePublicaMovil, ErrorNucleo> {
        self.estado.clave_publica()
    }

    /// Olvida la clave pública del móvil cacheada en el núcleo y vuelve a
    /// leerla del Keystore: la app lo llama después de recrear la clave cuando
    /// Android la invalidó (se quitó el bloqueo de pantalla). Queda anotada en
    /// REGISTRO como clave nueva.
    pub fn olvidar_cache_clave_movil(&self) -> Result<(), ErrorNucleo> {
        let anterior = self
            .estado
            .clave_publica
            .lock()
            .ok()
            .and_then(|mut cache| cache.take());
        let nueva = self.estado.clave_publica()?;
        if anterior.as_ref() != Some(&nueva) {
            crate::sincro::anotar_clave_generada(&self.estado);
        }
        Ok(())
    }

    /// Olvida la contraseña guardada de un host; devuelve si había una.
    pub fn olvidar_contrasena(&self, host_uuid: String) -> Result<bool, ErrorNucleo> {
        self.estado.llavero.olvidar_contrasena(&host_uuid)
    }

    // ------------------------------------------------------------------
    // Fase 14: ajustes, Flota, Servicios, Logs, Snippets, Ejecuciones,
    // Deliberación y Registro. Las de red van en el bloque async.
    // ------------------------------------------------------------------

    /// Pasa al núcleo los ajustes del móvil (informe §5.4). Acota los valores
    /// fuera de rango a los del escritorio y devuelve un aviso por cada uno.
    pub fn aplicar_ajustes(&self, ajustes: AjustesMovil) -> Result<Vec<String>, ErrorNucleo> {
        crate::ajustes::aplicar(&self.estado, ajustes)
    }

    /// Hosts de Flota con su estado, culpables, resumen, fallos, sesiones y
    /// hora del último sondeo; mismo filtro que la Fase 11 (informe §5.1).
    pub fn flota(&self, filtro: Option<String>) -> Result<Vec<HostFlota>, ErrorNucleo> {
        crate::flota::hosts_flota(&self.estado, filtro.as_deref())
    }

    /// Fallos de todos los hosts según el último sondeo del móvil, con su hora,
    /// y los hosts caídos o sin sondeo. No abre conexiones (informe §5.1).
    pub fn servicios_toda_la_flota(&self) -> Result<TodaLaFlota, ErrorNucleo> {
        crate::servicios::toda_la_flota(&self.estado)
    }

    /// Lo que la app necesita saber antes de confirmar o deliberar una acción:
    /// si se permite, si delibera, si pide sudo, la comprobación y si la unidad
    /// es crítica (informe §5.1 y §4.3).
    pub fn preparar_accion(
        &self,
        host: String,
        objetivo: ObjetivoMovil,
        accion: AccionMovil,
    ) -> Result<InfoAccionMovil, ErrorNucleo> {
        crate::servicios::preparar_accion(&self.estado, &host, &objetivo, accion)
    }

    /// Olvida la contraseña de sudo en memoria de un host (informe §7.5).
    pub fn sudo_olvidar(&self, host: String) -> Result<(), ErrorNucleo> {
        crate::servicios::sudo_olvidar(&self.estado, &host)
    }

    /// Olvida todas las contraseñas de sudo en memoria: la app lo llama al
    /// bloquearse MAGI (informe §7.5 y D268).
    pub fn olvidar_sudo_al_bloquear(&self) -> Result<(), ErrorNucleo> {
        crate::servicios::olvidar_sudo_al_bloquear(&self.estado);
        Ok(())
    }

    /// Hasta 500 líneas ya filtradas de una suscripción, desde un número de
    /// línea o desde el final, con el total, las omitidas y el estado (informe
    /// §5.1 y §7.4).
    pub fn logs_ventana(
        &self,
        suscripcion: u64,
        filtro: FiltroLogs,
    ) -> Result<VentanaLogs, ErrorNucleo> {
        crate::logs::ventana(&self.estado, suscripcion, &filtro)
    }

    /// Cierra el canal de una suscripción (fin de la entrada estándar). Las
    /// líneas se quedan en el anillo hasta que se suelte la pantalla.
    pub fn logs_parar(&self, suscripcion: u64) -> Result<(), ErrorNucleo> {
        crate::logs::parar(&self.estado, suscripcion)
    }

    /// Snippets del inventario (solo lectura en el móvil); filtro por
    /// subcadena como en el resto de listas (informe §5.1).
    pub fn snippets(&self, filtro: Option<String>) -> Result<Vec<SnippetMovil>, ErrorNucleo> {
        crate::snippets::snippets(&self.estado, filtro.as_deref())
    }

    /// Los snippets que tienen ese host entre sus destinos (informe §6.2).
    pub fn snippets_de_host(&self, host: String) -> Result<Vec<SnippetMovil>, ErrorNucleo> {
        crate::snippets::snippets_de_host(&self.estado, &host)
    }

    /// Detalle de un snippet: comando, descripción, destinos resueltos,
    /// variables, crítico, timeout y uso (informe §5.1).
    pub fn snippet(&self, uuid: String) -> Result<SnippetMovil, ErrorNucleo> {
        crate::snippets::snippet(&self.estado, &uuid)
    }

    /// Prepara una ejecución: el núcleo sustituye las variables con escape,
    /// valida y dice si delibera. Devuelve los errores por campo (informe §5.1
    /// y §7.6).
    pub fn preparar_ejecucion(
        &self,
        snippet: String,
        hosts: Vec<String>,
        variables: Vec<VariableSnippetMovil>,
    ) -> Result<PreparacionEjecucion, ErrorNucleo> {
        crate::snippets::preparar_ejecucion(&self.estado, &snippet, &hosts, &variables)
    }

    /// Ejecuciones en curso y terminadas, sin la salida (informe §5.1).
    pub fn ejecuciones(&self) -> Result<Vec<EjecucionMovil>, ErrorNucleo> {
        crate::ejecuciones::listar(&self.estado)
    }

    /// Salida de un host de una ejecución, sin secuencias de escape.
    pub fn salida_ejecucion(
        &self,
        ejecucion: u64,
        host: String,
    ) -> Result<SalidaEjecucion, ErrorNucleo> {
        crate::ejecuciones::salida(&self.estado, ejecucion, &host)
    }

    /// Cancela una ejecución en curso (informe §5.1).
    pub fn cancelar_ejecucion(&self, id: u64) -> Result<(), ErrorNucleo> {
        crate::ejecuciones::cancelar(&self.estado, id)
    }

    /// Prepara otra vez una ejecución terminada; volverá a deliberar si hace
    /// falta (informe §5.1).
    pub fn repetir_ejecucion(&self, id: u64) -> Result<PreparacionEjecucion, ErrorNucleo> {
        crate::ejecuciones::repetir(&self.estado, id)
    }

    /// Olvida las ejecuciones terminadas (informe §5.1).
    pub fn limpiar_ejecuciones(&self) -> Result<(), ErrorNucleo> {
        crate::ejecuciones::limpiar(&self.estado)
    }

    /// Una página del registro del móvil, de la más nueva a la más vieja
    /// (informe §5.1 y §7.10). `desde` es el id de la última entrada de la
    /// página anterior; None empieza por la más nueva.
    pub fn registro(
        &self,
        filtro: FiltroRegistro,
        desde: Option<u64>,
        cuantas: u32,
    ) -> Result<Vec<EntradaRegistro>, ErrorNucleo> {
        crate::registro::entradas(&self.estado, &filtro, desde, cuantas)
    }

    /// Detalle de una entrada del registro, con las comprobaciones de su
    /// deliberación si es una `deliberacion_*` (informe §7.10).
    pub fn registro_detalle(&self, id: u64) -> Result<DetalleRegistro, ErrorNucleo> {
        crate::registro::detalle(&self.estado, id)
    }

    /// `aprobar`, `forzar(motivo)` o `cancelar` una deliberación en curso;
    /// guarda la fila de DELIBERACIONES y devuelve su id para la acción o la
    /// ejecución (informe §5.1 y §7.7).
    pub fn resolver_deliberacion(
        &self,
        id: u64,
        decision: DecisionDeliberacion,
    ) -> Result<u64, ErrorNucleo> {
        crate::deliberacion::resolver(&self.estado, id, decision)
    }
}

/// Funciones de red y de disco lento del contrato: en Kotlin son suspend.
#[uniffi::export(async_runtime = "tokio")]
impl Nucleo {
    /// Guarda el árbol SAF elegido y comprueba que hay ficheros de MAGI.
    pub async fn elegir_carpeta(&self, uri: String) -> Result<(), ErrorNucleo> {
        crate::sincro::elegir_carpeta(&self.estado, uri).await
    }

    /// Normaliza y valida la clave de sincronización, la guarda y sincroniza.
    pub async fn unir_con_clave(&self, texto: String) -> Result<(), ErrorNucleo> {
        crate::sincro::unir_con_clave(&self.estado, texto).await
    }

    /// Pasada en modo lector; si ya hay una en curso no hace nada.
    pub async fn sincronizar_ahora(&self) -> Result<(), ErrorNucleo> {
        crate::sincro::sincronizar_ahora(&self.estado).await
    }

    /// Deja de sincronizar: falla si hay sesiones o túneles; borra la clave y
    /// el inventario (conserva la clave del móvil, contraseñas y huellas).
    pub async fn dejar_de_sincronizar(&self) -> Result<(), ErrorNucleo> {
        crate::sincro::dejar_de_sincronizar(&self.estado).await
    }

    /// Acepta la huella del otro equipo (sustituye la del known_hosts privado).
    pub async fn aceptar_huella_de_otro_equipo(&self, id: u64) -> Result<(), ErrorNucleo> {
        crate::sincro::aceptar_huella_de_otro_equipo(&self.estado, id).await
    }

    /// Mantiene mi huella y descarta la del otro equipo.
    pub async fn mantener_mi_huella(&self, id: u64) -> Result<(), ErrorNucleo> {
        crate::sincro::mantener_mi_huella(&self.estado, id).await
    }

    /// Abre una sesión (flujo §4.3) y devuelve su id; los diálogos llegan por
    /// OyenteNucleo.
    pub async fn abrir_sesion(&self, host_uuid: String) -> Result<u64, ErrorNucleo> {
        crate::gestor::abrir_sesion(&self.estado, &host_uuid).await
    }

    /// Reconecta una sesión caída con el mismo nombre.
    pub async fn reconectar(&self, sesion: u64) -> Result<(), ErrorNucleo> {
        crate::gestor::reconectar(&self.estado, sesion).await
    }

    /// Activa un túnel (conectando el host si hace falta).
    pub async fn activar_tunel(&self, uuid: String) -> Result<(), ErrorNucleo> {
        crate::gestor::activar_tunel(&self.estado, &uuid, false).await
    }

    /// Relanza un túnel caído.
    pub async fn relanzar_tunel(&self, uuid: String) -> Result<(), ErrorNucleo> {
        crate::gestor::relanzar_tunel(&self.estado, &uuid).await
    }

    /// Activa un túnel local o dinámico en un puerto libre del sistema.
    pub async fn activar_en_puerto_libre(&self, uuid: String) -> Result<(), ErrorNucleo> {
        crate::gestor::activar_tunel(&self.estado, &uuid, true).await
    }

    /// Instala la clave del móvil en el host (flujo §4.4 y script de §7.9).
    pub async fn instalar_clave_en_host(
        &self,
        host_uuid: String,
    ) -> Result<ResultadoInstalacionClave, ErrorNucleo> {
        crate::gestor::instalar_clave_en_host(&self.estado, &host_uuid).await
    }

    /// Borra los datos del móvil según el alcance (pide confirmación en la app).
    pub async fn borrar_datos(&self, alcance: AlcanceBorrado) -> Result<(), ErrorNucleo> {
        crate::sincro::borrar_datos(&self.estado, alcance).await
    }

    // ------------------------------------------------------------------
    // Fase 14: red y operaciones lentas (en Kotlin son `suspend`).
    // ------------------------------------------------------------------

    /// Sondea esos hosts (con la lista vacía, todos los `ssh`): semáforo de 8
    /// y 5 s por host; un host que ya se está sondeando se salta. Los
    /// resultados llegan por `OyenteNucleo.flota` (informe §5.1 y §4.1).
    pub async fn sondear(&self, hosts: Vec<String>) -> Result<(), ErrorNucleo> {
        crate::flota::sondear(&self.estado, hosts).await
    }

    /// `servicios.sh listar` (10 s) con los motores de los ajustes. Devuelve el
    /// listado del núcleo, si hay contraseña de sudo en memoria y su hora. Con
    /// `refresco = false` (lo pide el usuario) puede abrir la conexión con
    /// diálogos; con `true` es automática y no dialoga (informe §5.1).
    pub async fn servicios_listar(
        &self,
        host: String,
        refresco: bool,
    ) -> Result<ListadoServicios, ErrorNucleo> {
        crate::servicios::listar(&self.estado, &host, refresco).await
    }

    /// CPU y memoria de los contenedores en marcha (suborden `consumo`).
    pub async fn servicios_consumo(&self, host: String) -> Result<Vec<Consumo>, ErrorNucleo> {
        crate::servicios::consumo(&self.estado, &host).await
    }

    /// Propiedades, dependencias y texto de la unidad o de `inspect`.
    pub async fn servicios_detalle(
        &self,
        host: String,
        objetivo: ObjetivoMovil,
    ) -> Result<DetalleServicios, ErrorNucleo> {
        crate::servicios::detalle(&self.estado, &host, &objetivo).await
    }

    /// Ejecuta la acción y devuelve el resultado cuando ya está anotado; además
    /// lo difunde por `OyenteNucleo.accion_servicio_hecha` (informe §5.1 y
    /// §4.3). `deliberacion` es el id de la deliberación que la autoriza, si la
    /// acción la exige.
    pub async fn servicio_accion(
        &self,
        host: String,
        objetivo: ObjetivoMovil,
        accion: AccionMovil,
        deliberacion: Option<u64>,
    ) -> Result<ResultadoServicioMovil, ErrorNucleo> {
        crate::servicios::accion(&self.estado, &host, &objetivo, accion, deliberacion).await
    }

    /// Comprueba la contraseña de sudo con `servicios.sh sudo`: `ok` (queda en
    /// memoria), `rechazada` o `sin_sudo` (informe §5.1 y §7.5).
    pub async fn sudo_comprobar(
        &self,
        host: String,
        contrasena: Vec<u8>,
    ) -> Result<ResultadoSudo, ErrorNucleo> {
        crate::servicios::sudo_comprobar(&self.estado, &host, contrasena).await
    }

    /// Abre la suscripción de logs (`servicios.sh logs`) y devuelve su id. Hay
    /// una por objetivo (informe §5.1 y §4.4).
    pub async fn logs_seguir(
        &self,
        host: String,
        objetivo: ObjetivoMovil,
        lineas: u32,
    ) -> Result<u64, ErrorNucleo> {
        crate::logs::seguir(&self.estado, &host, &objetivo, lineas).await
    }

    /// Abre una sesión con PTY para una `OrdenPestana` del núcleo. `magi-movil`
    /// arma la orden desde el tipo; la app nunca manda texto (informe §5.1).
    pub async fn abrir_sesion_con_orden(
        &self,
        host: String,
        orden: OrdenPestanaMovil,
    ) -> Result<u64, ErrorNucleo> {
        crate::servicios::abrir_sesion_con_orden(&self.estado, &host, orden).await
    }

    /// Lanza la ejecución de un snippet preparado y devuelve su id (informe
    /// §5.1 y §4.5). `deliberacion` es el id de la deliberación que la autoriza,
    /// si la ejecución la exige.
    pub async fn ejecutar_snippet(
        &self,
        preparacion: PreparacionEjecucion,
        parar_al_fallo: bool,
        deliberacion: Option<u64>,
    ) -> Result<u64, ErrorNucleo> {
        crate::ejecuciones::ejecutar(&self.estado, preparacion, parar_al_fallo, deliberacion).await
    }

    /// Las comprobaciones de §4.6 en paralelo (2 s cada una). Devuelve la
    /// tabla, el consenso y el estado (informe §5.1 y §7.7).
    pub async fn deliberar(
        &self,
        plan: PlanDeliberadoMovil,
    ) -> Result<TablaDeliberacion, ErrorNucleo> {
        crate::deliberacion::deliberar(&self.estado, plan).await
    }
}

/// Superficie de pruebas de extremo a extremo de `magi-movil`: fuera de los
/// bloques `#[uniffi::export]`, así que no cruza UniFFI. Permite que las
/// pruebas de `tests/` ejerzan los módulos del núcleo con el mismo estado que
/// crea `Nucleo::iniciar` (conexiones, flota, servicios, logs, ejecuciones y
/// deliberación).
#[doc(hidden)]
impl Nucleo {
    /// Estado interno del núcleo, solo para las pruebas de `tests/`.
    pub fn estado_de_prueba(&self) -> &Arc<crate::estado::EstadoNucleo> {
        &self.estado
    }
}
