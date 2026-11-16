//! Protocolo entre la TUI (cliente) y el servidor de sesiones: JSON por
//! líneas sobre el socket Unix. Los bytes del terminal viajan en base64 y
//! los secretos en `Zeroizing`. Un mensaje desconocido o mal formado no se
//! interpreta: produce `Error` y desconexión de ese cliente.

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
pub use tokio_util::codec::LinesCodec;
use zeroize::Zeroizing;

use crate::archivos::{Entrada, Propietario, TipoEntrada};

/// Versión del protocolo. Se negocia en el saludo `Hola`/`Bienvenida`;
/// versiones distintas no cooperan.
///
/// v2 (Fase 4): archivos (SFTP) y cola de transferencias. Además, el
/// `sesion_id` de los diálogos de conexión pasa a significar «id de la
/// solicitud de conexión»: sirve igual para una sesión que para una apertura
/// de canal SFTP, que no tiene sesión propia.
///
/// v3 (Fase 5): túneles. El cliente hace el CRUD de `TUNELES` y el servidor
/// los ejecuta; `Tuneles{lista}` difunde la lista completa (definidos y su
/// estado en vivo) y `Bienvenida` la lleva para que una ventana nueva la vea.
///
/// v4 (Fase 6): corrección 3b del pool y snippets. `Ejecutar` y sus respuestas
/// llevan `peticion_id` (dos sondeos al mismo host ya no se cruzan);
/// `VersionIncompatible` lleva el pid del servidor; `PideLlavero` pide al
/// solicitante una contraseña que solo puede salir de su llavero (nunca de un
/// diálogo); `Cerrar` cancela también los diálogos pendientes de aperturas que
/// no son sesiones (SFTP, túneles, ejecuciones). Snippets: `LanzarEjecucion`,
/// `CancelarEjecucion`, `LimpiarEjecuciones`, `PedirSalida` → `Salida`, la
/// difusión `Ejecuciones{lista}` (también en `Bienvenida`) y los comandos
/// iniciales de `AbrirSesion`.
///
/// v5 (Fase 8): archivos, segunda vuelta. `StatRemoto` → `Stat`,
/// `CambiarPermisos` (con máscara y alcance recursivo) y `ListarArbol` →
/// `Arbol` en bloques; `Hecho` gana `detalle`; `Transferir` gana permisos por
/// elemento, `peticion_id` (contestado con `Hecho`/`Error`), `borrar_al_terminar`,
/// deliberación, sincronización y etiqueta; la cola gana la fase `Borrando`;
/// las entradas llevan uid, gid y sus nombres; `SftpAbierto` dice el usuario
/// de la conexión; `DescargarTemporal` distingue los temporales de edición.
///
/// v6 (Fase 9): actualización segura entre versiones. `Hola` y
/// `VersionIncompatible` son contrato estable (nunca se quitan ni renombran
/// campos, solo se añaden opcionales): un binario nuevo puede leer el rechazo
/// de un servidor anterior y decidir si lo sustituye (sin preguntar solo si no
/// custodia nada) o pregunta al usuario. `VersionIncompatible` gana
/// `version_app`, `clientes` y `trabajo`; `Bienvenida` gana `version_app`.
///
/// v7 (Fase 12): escritorios remotos. `AbrirEscritorio`, `PedirEnlace`,
/// `ReconectarEscritorio`, `CerrarEscritorio`, `RespuestaHuellaEscritorio` y
/// `Credenciales`; `EscritorioAbierto`, `Enlace`, `Escritorios{lista}`,
/// `PideHuellaEscritorio` y `PideCredenciales`.
///
/// v9 (Fase 15): terminal, segunda vuelta. `CapturarHistorial` → `HistorialLote`,
/// `Foco`, `IniciarEscrituraVarios`/`EscribirVarios`/`TerminarEscrituraVarios` →
/// `EscrituraVariosIniciada`/`EscrituraVariosTerminada`, `AbrirTunelTemporal` →
/// `TunelTemporal` y `TunelesTemporales{lista}`, `ListarTmux` → `SesionesTmux`,
/// `TerminarTmux` → `TmuxTerminada`, `Portapapeles`/`PortapapelesPedido`/
/// `PortapapelesPermitido`, `RenombrarSesion` y `MoverSesion`. `Adjuntar` gana
/// `colores`, `AbrirSesion` gana `tmux` y `Sesiones{lista}` gana
/// `reconexion`, `titulo`, `aviso` y `tmux`. `TrabajoServidor` gana
/// `tuneles_temporales`.
///
/// v8 (Fase 13): servicios, contenedores y logs. `ListarServicios`,
/// `PedirConsumo`, `DetalleServicio`, `ComprobarSudo`, `AccionServicio`,
/// `SeguirLog` y `PararLog`; `Servicios`, `Consumo`, `Detalle`,
/// `SudoComprobado`, `AccionServicioHecha`, `AccionesServicio{lista}`,
/// `LineasLog` y `FinLog`; `AbrirSesion` gana `orden` (pestañas con orden) y
/// `TrabajoServidor.acciones_servicio`. Los `peticion_id` de servicios van
/// desde 2^45.
pub const VERSION_PROTOCOLO: u32 = 9;

/// Los `peticion_id` del terminal (capturas, tmux y túneles temporales) viven
/// desde 2^49, por encima de todo lo demás (Fase 15).
pub const RANGO_TERMINAL: u64 = 1 << 49;

/// Los `peticion_id` de escritorios que genera el cliente empiezan en 2^46
/// (Fase 12); por debajo están los de archivos, túneles y ejecuciones
/// (CLAUDE.md).
pub const RANGO_ESCRITORIOS: u64 = 1 << 46;

/// Los `peticion_id` de escritorios que genera el SERVIDOR (diálogos de una
/// reconexión pedida desde el visor) viven en [2^47, 2^48), un tramo libre
/// entre los del cliente y los esperables del cliente (2^48).
pub const RANGO_ESCRITORIOS_SERVIDOR: u64 = 1 << 47;

/// Los `peticion_id` de la vista Servicios (Fase 13) empiezan en 2^45, entre
/// los de ejecuciones (2^44) y los de escritorios (2^46).
pub const RANGO_SERVICIOS: u64 = 1 << 45;

/// Línea máxima de un mensaje (las pantallas completas son lo más grande).
pub const LINEA_MAXIMA: usize = 4 * 1024 * 1024;

pub fn codec() -> LinesCodec {
    LinesCodec::new_with_max_length(LINEA_MAXIMA)
}

// ---------------------------------------------------------------- base64

/// Codificación base64 para los campos de bytes del terminal.
mod base64_bytes {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializador: S) -> Result<S::Ok, S::Error> {
        base64::engine::general_purpose::STANDARD
            .encode(bytes)
            .serialize(serializador)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let texto = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(&texto)
            .map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------- secretos

/// Cadena con `zeroize` que se muestra como `***` en el depurado y viaja
/// como un `String` normal dentro del mensaje.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secreto(pub Zeroizing<String>);

impl Secreto {
    pub fn nuevo(texto: impl Into<String>) -> Self {
        Self(Zeroizing::new(texto.into()))
    }

    pub fn como_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secreto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl Serialize for Secreto {
    fn serialize<S: serde::Serializer>(&self, serializador: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializador)
    }
}

impl<'de> Deserialize<'de> for Secreto {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(Zeroizing::new(String::deserialize(d)?)))
    }
}

// ---------------------------------------------------------------- escritorios

pub use magi_nucleo::escritorio::{
    DatosCertificado, InfoEscritorio, PeticionCredenciales, TipoHost,
};

// ---------------------------------------------------------------- servicios

pub use magi_nucleo::servicios::{
    lectura::ComprobacionSudo, Accion, Consumo, Contenedor, Detalle, FaseAccion, InfoAccion,
    LineaLog, Listado, MotivoFinLog, MotorNombre, Objetivo, OrdenPestana, Privilegio,
    ResultadoAccion, ResultadoServicio, TipoObjetivo,
};

// ---------------------------------------------------------------- estados

/// Estado de una sesión visto desde fuera (el detalle fino de «abriendo» lo
/// explica el `motivo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoSesionRemota {
    Abriendo,
    Abierta,
    Caida,
    Cerrada,
}

// --------------------------------------------------- terminal (v9)

pub use magi_nucleo::terminal::emulador::ColoresTema;
pub use magi_nucleo::terminal::historial::LineaHistorial;
pub use magi_nucleo::terminal::tmux::SesionTmux as InfoSesionTmux;

/// Estado de la reconexión automática de una pestaña.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoReconexion {
    /// Intento en curso, desde 1.
    pub intento: u32,
    /// Época en segundos del próximo intento.
    pub proximo_en: i64,
    /// Época en segundos de la caída.
    pub desde: i64,
    pub causa: String,
}

/// Último aviso del remoto de una pestaña (campana, OSC 9 u OSC 777).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoAviso {
    pub texto: String,
    /// Época en segundos en la que llegó.
    pub hora: i64,
}

/// Un túnel temporal de una pestaña: escucha en `127.0.0.1` del equipo y
/// reenvía a un localhost del host mientras vive la pestaña.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoTunelTemporal {
    pub id: u32,
    pub sesion_id: u32,
    pub host_id: i64,
    pub host_nombre: String,
    pub puerto_local: u16,
    /// `localhost`, `127.0.0.1` o `::1`.
    pub destino: String,
    pub puerto: u16,
    pub conexiones: u32,
}

/// Bytes ya codificados de una pestaña en la escritura en varios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytesPestana {
    pub sesion_id: u32,
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
}

/// Resultado de `TerminarTmux`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultadoTmux {
    Ok,
    NoExiste,
}

/// Resumen de una sesión para la lista que se difunde a los clientes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoSesion {
    pub id: u32,
    pub nombre: String,
    pub host_id: i64,
    pub host_nombre: String,
    pub estado: EstadoSesionRemota,
    pub motivo: Option<String>,
    pub identidad: String,
    /// Época en segundos en la que se abrió la sesión.
    pub abierta_en: i64,
    pub ventanas: u32,
    /// Llegaron datos del remoto sin que nadie la estuviera viendo.
    pub actividad_no_vista: bool,
    /// Estado de la reconexión automática, si está reconectando (Fase 15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconexion: Option<InfoReconexion>,
    /// Título que puso el programa remoto (OSC 0 u OSC 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titulo: Option<String>,
    /// Último aviso del remoto sin ver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aviso: Option<InfoAviso>,
    /// Nombre de la sesión de tmux, si la pestaña es persistente.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux: Option<String>,
    /// La pestaña está en una escritura en varios activa (la marca `»`). Lo
    /// ve cualquier ventana, no solo la que la activó (§7.14).
    #[serde(default)]
    pub escritura_varios: bool,
}

// ---------------------------------------------------------------- archivos

pub use magi_nucleo::modelo::Direccion;

/// Qué hace el servidor cuando el destino de un elemento ya existe. La decide
/// el cliente antes de encolar; el servidor la aplica sin dialogar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Politica {
    Sobrescribir,
    Omitir,
}

/// Estado de una transferencia en la cola.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoTransferencia {
    EnCola,
    EnCurso,
    /// La copia terminó sin errores y se están borrando en el destino las
    /// rutas de `borrar_al_terminar` (sincronización con «borrar»).
    Borrando,
    Hecha,
    Error,
    Cancelada,
}

impl EstadoTransferencia {
    pub fn texto(self) -> &'static str {
        match self {
            EstadoTransferencia::EnCola => "en cola",
            EstadoTransferencia::EnCurso => "en curso",
            EstadoTransferencia::Borrando => "borrando",
            EstadoTransferencia::Hecha => "hecha",
            EstadoTransferencia::Error => "error",
            EstadoTransferencia::Cancelada => "cancelada",
        }
    }

    /// Glifo de la vista Transferencias.
    pub fn glifo(self, ascii: bool) -> &'static str {
        match (self, ascii) {
            (EstadoTransferencia::EnCola, false) => "○",
            (EstadoTransferencia::EnCurso, false) => "◐",
            (EstadoTransferencia::Borrando, false) => "◑",
            (EstadoTransferencia::Hecha, false) => "●",
            (EstadoTransferencia::Error, false) => "✕",
            (EstadoTransferencia::Cancelada, false) => "⊘",
            (EstadoTransferencia::EnCola, true) => "o",
            (EstadoTransferencia::EnCurso, true) => "o",
            (EstadoTransferencia::Borrando, true) => "o",
            (EstadoTransferencia::Hecha, true) => "*",
            (EstadoTransferencia::Error, true) => "x",
            (EstadoTransferencia::Cancelada, true) => "-",
        }
    }

    pub fn terminada(self) -> bool {
        matches!(
            self,
            EstadoTransferencia::Hecha
                | EstadoTransferencia::Error
                | EstadoTransferencia::Cancelada
        )
    }
}

/// Elemento de primer nivel de una transferencia: lo que el usuario marcó en
/// el panel, no cada fichero del interior de un directorio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementoTransferencia {
    pub origen: String,
    pub destino: String,
    /// Bytes del elemento completo (para un directorio, la suma de su contenido).
    pub bytes: u64,
    pub es_directorio: bool,
    /// Política decidida para este elemento; sin ella manda la del mensaje.
    pub politica: Option<Politica>,
    /// Permisos que se aplican al destino tras el `rename` del parcial (o al
    /// crear el directorio). Sin ellos, los que deje el sistema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permisos: Option<u32>,
}

/// Para qué es una transferencia, más allá de copiar lo marcado. Cambia cómo
/// se anota y, en una sincronización, que la lista llega ya plana (el servidor
/// no expande directorios: un directorio es solo «crearlo»).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EtiquetaTransferencia {
    /// La subida de una edición remota (`E`).
    Edicion,
    /// La ejecución de un plan de sincronización (`S`, `L`).
    Sincronizacion,
}

impl EtiquetaTransferencia {
    pub fn texto(self) -> &'static str {
        match self {
            EtiquetaTransferencia::Edicion => "edición",
            EtiquetaTransferencia::Sincronizacion => "sincronización",
        }
    }
}

/// Datos de la sincronización que lanza una transferencia: con ellos el
/// servidor anota `sincronizacion` y, si es una guardada, escribe su
/// `ultimo_resultado` al terminar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SincronizacionLanzada {
    /// Fila de `SINCRONIZACIONES_DIR`; ninguna si es «ad hoc».
    pub id: Option<i64>,
    /// Nombre de la guardada; ninguno si es «ad hoc».
    pub nombre: Option<String>,
    /// Recuentos del plan fijado por el cliente.
    pub creados: u32,
    pub actualizados: u32,
    pub omitidos: u32,
    /// Raíces del origen y del destino: el servidor rechaza cualquier ruta de
    /// `borrar_al_terminar` o de destino que no cuelgue de `raiz_destino`, y
    /// la cola las enseña como «origen → destino».
    pub raiz_origen: String,
    pub raiz_destino: String,
}

/// Alcance recursivo de `CambiarPermisos`. Sin alcance, solo las rutas
/// marcadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlcancePermisos {
    /// Las rutas y todo lo que contienen.
    Todo,
    /// Los directorios marcados y sus subdirectorios.
    Directorios,
    /// Los ficheros marcados y los ficheros que contienen los directorios.
    Ficheros,
}

impl AlcancePermisos {
    pub fn texto(self) -> &'static str {
        match self {
            AlcancePermisos::Todo => "todo",
            AlcancePermisos::Directorios => "solo directorios",
            AlcancePermisos::Ficheros => "solo ficheros",
        }
    }
}

/// Una entrada de `Arbol`: la ruta va relativa a la raíz pedida, con `/`.
/// Un enlace a fichero viaja como `Enlace` con el tamaño y la fecha de su
/// destino; uno a directorio, con `enlace_a_dir` (el plan lo omite).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntradaArbol {
    pub ruta: String,
    pub tipo: TipoEntrada,
    pub tamano: u64,
    pub mtime: i64,
    pub permisos: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub propietario: Option<Propietario>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enlace_a_dir: bool,
}

/// Fila de la cola de transferencias que ve el cliente. No lleva la lista de
/// ficheros: la cola se difunde entera y a menudo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoTransferencia {
    pub id: u32,
    pub host_id: i64,
    pub host_nombre: String,
    pub direccion: Direccion,
    pub estado: EstadoTransferencia,
    pub origen: String,
    pub destino: String,
    /// Nombre del elemento de primer nivel, para la columna «origen → destino».
    pub es_directorio: bool,
    pub ficheros_total: u32,
    pub ficheros_hechos: u32,
    /// Ficheros que ya existían en el destino y se omitieron.
    pub omitidos: u32,
    pub bytes_total: u64,
    pub bytes_hechos: u64,
    pub fichero_actual: Option<String>,
    pub error: Option<String>,
    pub borrar_origen: bool,
    /// Cliente que la encoló (para que borre él el origen local de una subida).
    pub solicitante: u32,
    pub creada_en: i64,
    pub terminada_en: Option<i64>,
    /// El de la `Transferir` que la creó: con `solicitante`, la ventana que
    /// la pidió reconoce su fila en la difusión.
    #[serde(default)]
    pub peticion_id: Option<u64>,
    #[serde(default)]
    pub etiqueta: Option<EtiquetaTransferencia>,
    /// Rutas de `borrar_al_terminar` ya borradas y su total.
    #[serde(default)]
    pub borrados: u32,
    #[serde(default)]
    pub borrados_total: u32,
}

impl InfoTransferencia {
    /// Porcentaje de progreso, redondeado; 0 si no se conocen los bytes.
    pub fn porcentaje(&self) -> u16 {
        if self.bytes_total == 0 {
            return 0;
        }
        let tanto = self.bytes_hechos.saturating_mul(100) / self.bytes_total;
        tanto.min(100) as u16
    }
}

// ---------------------------------------------------------------- túneles

/// Estado de un túnel visto desde fuera. `Inactivo` es una fila de `TUNELES`
/// sin nadie escuchando todavía.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoTunelRemoto {
    Inactivo,
    Activando,
    Activo,
    Parando,
    Caido,
}

impl EstadoTunelRemoto {
    pub fn texto(self) -> &'static str {
        match self {
            EstadoTunelRemoto::Inactivo => "inactivo",
            EstadoTunelRemoto::Activando => "activando",
            EstadoTunelRemoto::Activo => "activo",
            EstadoTunelRemoto::Parando => "parando",
            EstadoTunelRemoto::Caido => "caído",
        }
    }

    /// Glifo de la vista Túneles, con doble codificación.
    pub fn glifo(self, ascii: bool) -> &'static str {
        match (self, ascii) {
            (EstadoTunelRemoto::Activo, false) => "●",
            (EstadoTunelRemoto::Activando, false) => "◐",
            (EstadoTunelRemoto::Parando, false) => "◐",
            (EstadoTunelRemoto::Inactivo, false) => "○",
            (EstadoTunelRemoto::Caido, false) => "✕",
            (EstadoTunelRemoto::Activo, true) => "*",
            (EstadoTunelRemoto::Activando, true) => "o",
            (EstadoTunelRemoto::Parando, true) => "o",
            (EstadoTunelRemoto::Inactivo, true) => "o",
            (EstadoTunelRemoto::Caido, true) => "x",
        }
    }

    /// ¿Hay alguien escuchando o a punto de hacerlo?
    pub fn en_marcha(self) -> bool {
        matches!(
            self,
            EstadoTunelRemoto::Activo | EstadoTunelRemoto::Activando
        )
    }
}

/// Quién levantó el túnel: la ventana que lo pidió o el ciclo automático.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigenTunel {
    Manual,
    Automatico,
}

impl OrigenTunel {
    pub fn texto(self) -> &'static str {
        match self {
            OrigenTunel::Manual => "manual",
            OrigenTunel::Automatico => "automático",
        }
    }
}

/// Resumen de un túnel para la lista que se difunde: la fila de `TUNELES` más
/// el estado en vivo que lleva el servidor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoTunel {
    pub tunel_id: i64,
    pub host_id: i64,
    pub host_nombre: String,
    pub nombre: String,
    /// `local` | `remoto` | `dinamico`.
    pub tipo: String,
    /// La escucha configurada en `TUNELES`.
    pub escucha: String,
    /// La que está escuchando de verdad: cambia si la escucha pedía el puerto
    /// 0 (local y dinámico) o si el host confirma otro (remoto).
    pub escucha_efectiva: String,
    pub destino: Option<String>,
    pub automatico: bool,
    pub estado: EstadoTunelRemoto,
    /// Quién lo levantó; nulo si nadie.
    pub origen: Option<OrigenTunel>,
    /// Ventana que lo activó, si sigue conectada.
    pub solicitante: Option<u32>,
    pub conexiones: u32,
    pub aceptadas: u64,
    pub bytes_subidos: u64,
    pub bytes_bajados: u64,
    /// Época en segundos en la que se levantó.
    pub desde: Option<i64>,
    pub ultimo_error: Option<String>,
}

impl InfoTunel {
    /// Escucha que se pinta en la tabla: la efectiva mientras hay algo
    /// escuchando, la configurada cuando no.
    pub fn escucha_mostrada(&self) -> &str {
        if self.estado == EstadoTunelRemoto::Inactivo {
            &self.escucha
        } else {
            &self.escucha_efectiva
        }
    }
}

// ---------------------------------------------------------------- ejecuciones

/// Bytes de la salida de un comando remoto: viajan en base64 y el depurado
/// solo dice cuántos son (T44: la salida no puede acabar en un log por
/// accidente).
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalidaRemota(#[serde(with = "base64_bytes")] pub Vec<u8>);

impl fmt::Debug for SalidaRemota {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} B>", self.0.len())
    }
}

/// Texto que se escribe en una pestaña nada más abrir la shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComandoInicial {
    pub texto: String,
    /// Se vuelve a escribir al reconectar: el «snippet al conectar». El de
    /// «abrir en pestaña» se escribe una sola vez.
    pub repetir: bool,
}

/// La deliberación que autoriza una ejecución: el servidor lee su fila de
/// `DELIBERACIONES`, la valida y, al terminar, anota y rellena el resultado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliberacionLanzada {
    pub id: i64,
    pub forzada: bool,
    pub motivo: Option<String>,
}

/// Estado de una ejecución.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoEjecucion {
    EnCurso,
    Terminada,
    Cancelada,
}

impl EstadoEjecucion {
    pub fn texto(self) -> &'static str {
        match self {
            EstadoEjecucion::EnCurso => "en curso",
            EstadoEjecucion::Terminada => "terminada",
            EstadoEjecucion::Cancelada => "cancelada",
        }
    }
}

/// Estado de un host dentro de una ejecución.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoHostEjecucion {
    EnCola,
    Conectando,
    Ejecutando,
    Ok,
    Fallo,
    Error,
    Cancelado,
    Omitido,
}

impl EstadoHostEjecucion {
    pub fn texto(self) -> &'static str {
        match self {
            EstadoHostEjecucion::EnCola => "en cola",
            EstadoHostEjecucion::Conectando => "conectando",
            EstadoHostEjecucion::Ejecutando => "ejecutando",
            EstadoHostEjecucion::Ok => "ok",
            EstadoHostEjecucion::Fallo => "fallo",
            EstadoHostEjecucion::Error => "error",
            EstadoHostEjecucion::Cancelado => "cancelado",
            EstadoHostEjecucion::Omitido => "omitido",
        }
    }

    /// `●` ok, `✕` fallo/error, `◐` en marcha, `○` en cola, `–` omitido o
    /// cancelado (ASCII `* x o o -`).
    pub fn glifo(self, ascii: bool) -> &'static str {
        match (self, ascii) {
            (EstadoHostEjecucion::Ok, false) => "●",
            (EstadoHostEjecucion::Fallo | EstadoHostEjecucion::Error, false) => "✕",
            (EstadoHostEjecucion::Conectando | EstadoHostEjecucion::Ejecutando, false) => "◐",
            (EstadoHostEjecucion::EnCola, false) => "○",
            (EstadoHostEjecucion::Cancelado | EstadoHostEjecucion::Omitido, false) => "–",
            (EstadoHostEjecucion::Ok, true) => "*",
            (EstadoHostEjecucion::Fallo | EstadoHostEjecucion::Error, true) => "x",
            (
                EstadoHostEjecucion::Conectando
                | EstadoHostEjecucion::Ejecutando
                | EstadoHostEjecucion::EnCola,
                true,
            ) => "o",
            (EstadoHostEjecucion::Cancelado | EstadoHostEjecucion::Omitido, true) => "-",
        }
    }

    /// ¿Ya no va a cambiar?
    pub fn es_final(self) -> bool {
        matches!(
            self,
            EstadoHostEjecucion::Ok
                | EstadoHostEjecucion::Fallo
                | EstadoHostEjecucion::Error
                | EstadoHostEjecucion::Cancelado
                | EstadoHostEjecucion::Omitido
        )
    }

    /// ¿Está conectando o ejecutando?
    pub fn en_marcha(self) -> bool {
        matches!(
            self,
            EstadoHostEjecucion::Conectando | EstadoHostEjecucion::Ejecutando
        )
    }
}

/// Un host de una ejecución tal como se difunde: metadatos, nunca la salida.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoEjecucionHost {
    pub host_id: i64,
    pub nombre: String,
    pub estado: EstadoHostEjecucion,
    pub codigo: Option<i32>,
    /// Época en milisegundos en la que empezó a conectar.
    pub inicio_ms: Option<i64>,
    pub duracion_ms: Option<u64>,
    pub bytes_stdout: u64,
    pub bytes_stderr: u64,
    pub truncada: bool,
    pub error: Option<String>,
}

/// Una ejecución tal como se difunde. No lleva el comando: puede contener el
/// valor de una variable que no tiene por qué ver otra ventana.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoEjecucion {
    pub id: u32,
    /// El de la `LanzarEjecucion` que la creó: con `solicitante`, la ventana
    /// que la pidió la reconoce en la difusión.
    pub peticion_id: u64,
    pub solicitante: u32,
    pub snippet_id: Option<i64>,
    pub nombre: String,
    pub hosts: Vec<InfoEjecucionHost>,
    pub timeout_seg: u32,
    pub parar_al_fallo: bool,
    pub deliberacion_id: Option<i64>,
    pub forzada: bool,
    pub estado: EstadoEjecucion,
    /// Épocas en segundos.
    pub creada_en: i64,
    pub terminada_en: Option<i64>,
}

// ---------------------------------------------------------------- cliente → servidor

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo")]
pub enum MensajeCliente {
    /// Saludo inicial del cliente. **Contrato estable** (Fase 9): no se
    /// quitan ni renombran campos, solo se añaden opcionales, para que
    /// cualquier versión del servidor pueda rechazarlo con
    /// `VersionIncompatible`.
    Hola {
        version: u32,
        pid: u32,
    },
    AbrirSesion {
        host_id: i64,
        cols: u16,
        filas: u16,
        /// Lo que se escribe en la shell al abrirla (snippet al conectar y el
        /// de «abrir en pestaña»); al reconectar, solo los que `repetir`.
        #[serde(default)]
        comandos_iniciales: Vec<ComandoInicial>,
        /// Orden con la que se abre la pestaña (Fase 13, D248): el servidor la
        /// convierte en el comando real y la escribe en la shell. El cliente
        /// nunca manda texto para ejecutar por esta vía.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        orden: Option<OrdenPestana>,
        /// Nombre de una sesión de tmux existente a la que enganchar la
        /// pestaña (Fase 15). Debe cumplir la validación del nombre.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tmux: Option<String>,
    },
    Adjuntar {
        sesion_id: u32,
        cols: u16,
        filas: u16,
        /// Colores del tema de la ventana para responder a OSC 10, 11 y 12
        /// (Fase 15). El servidor usa los del primer adjunto que los mande.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        colores: Option<ColoresTema>,
    },
    Desadjuntar {
        sesion_id: u32,
    },
    Teclas {
        sesion_id: u32,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    Redimensionar {
        sesion_id: u32,
        cols: u16,
        filas: u16,
    },
    Cerrar {
        sesion_id: u32,
    },
    Reconectar {
        sesion_id: u32,
    },
    DecisionHuella {
        sesion_id: u32,
        decision: bool,
    },
    Frase {
        sesion_id: u32,
        frase: Secreto,
    },
    Contrasena {
        sesion_id: u32,
        contrasena: Secreto,
        recordar: bool,
    },
    /// Ejecuta un comando en la conexión viva del host (sondeo de Flota).
    /// Responde `Ejecutado` o `SinSesion` con el mismo `peticion_id`.
    Ejecutar {
        host_id: i64,
        comando: String,
        peticion_id: u64,
    },
    /// Abre (o reutiliza) el canal SFTP del host. Responde `SftpAbierto` o
    /// `Error`; si no hay conexión viva, la abre con los diálogos de siempre.
    AbrirSftp {
        host_id: i64,
        /// Con él, `SftpAbierto` o `Error` lo devuelven (comprobaciones de la
        /// deliberación, que esperan su respuesta).
        #[serde(default)]
        peticion_id: Option<u64>,
        /// Sin diálogos y sin levantar túneles automáticos (BALTHASAR-2).
        #[serde(default)]
        no_interactivo: bool,
    },
    ListarDir {
        host_id: i64,
        ruta: String,
        peticion_id: u64,
    },
    /// Encola una transferencia. `elementos` es la lista de primer nivel; en
    /// una subida el cliente ya la ha expandido (incluidos los directorios) y
    /// en una bajada la expande el servidor (salvo en una sincronización, que
    /// llega plana).
    Transferir {
        host_id: i64,
        direccion: Direccion,
        elementos: Vec<ElementoTransferencia>,
        /// Política por defecto de los elementos que no traigan la suya.
        politica: Politica,
        borrar_origen: bool,
        /// Con él, el servidor contesta `Hecho` al encolar o `Error` si la
        /// rechaza, y la fila de la cola lo lleva.
        #[serde(default)]
        peticion_id: Option<u64>,
        /// Rutas absolutas del destino que se borran solo si la copia termina
        /// sin errores: ficheros primero, directorios después (de más profundo
        /// a menos, y solo si quedan vacíos). Las fija el cliente en el plan.
        #[serde(default)]
        borrar_al_terminar: Vec<String>,
        #[serde(default)]
        deliberacion: Option<DeliberacionLanzada>,
        #[serde(default)]
        sincronizacion: Option<SincronizacionLanzada>,
        #[serde(default)]
        etiqueta: Option<EtiquetaTransferencia>,
    },
    CancelarTransferencia {
        id: u32,
    },
    /// Quita de la cola las transferencias terminadas.
    LimpiarTransferencias,
    BorrarRemoto {
        host_id: i64,
        rutas: Vec<String>,
        peticion_id: u64,
    },
    RenombrarRemoto {
        host_id: i64,
        de: String,
        a: String,
        peticion_id: u64,
    },
    CrearDirRemoto {
        host_id: i64,
        ruta: String,
        peticion_id: u64,
    },
    /// Copia un fichero remoto a un temporal local para verlo con `$PAGER`
    /// o, con `edicion`, para editarlo: ese temporal vive aparte y el servidor
    /// nunca lo borra por su cuenta.
    DescargarTemporal {
        host_id: i64,
        ruta: String,
        peticion_id: u64,
        #[serde(default)]
        edicion: bool,
    },
    /// Borra un temporal creado por `DescargarTemporal`.
    BorrarTemporal {
        ruta: String,
    },
    /// Metadatos de una ruta remota sin seguir enlaces. Contesta `Stat`.
    StatRemoto {
        peticion_id: u64,
        host_id: i64,
        ruta: String,
    },
    /// `chmod` remoto: a cada ruta (y, con alcance, a lo que contiene) se le
    /// aplica `(viejo & !mascara) | (modo & mascara)`. Los enlaces nunca se
    /// tocan. Contesta `Hecho{detalle}` con afectados y errores.
    CambiarPermisos {
        peticion_id: u64,
        host_id: i64,
        rutas: Vec<String>,
        modo: u32,
        mascara: u32,
        alcance: Option<AlcancePermisos>,
    },
    /// Recorre un directorio remoto entero. Contesta con `Arbol` en bloques
    /// de 1000 entradas, el último con `fin`.
    ListarArbol {
        peticion_id: u64,
        host_id: i64,
        ruta: String,
        /// Patrones de gitignore que se podan al recorrer, en este orden:
        /// `exclusiones`, el `.magiignore` de la raíz (si `usar_magiignore`) y
        /// `exclusiones_extra` (§7.2: por defecto → `.magiignore` → extras).
        exclusiones: Vec<String>,
        usar_magiignore: bool,
        #[serde(default)]
        exclusiones_extra: Vec<String>,
    },
    /// Levanta un túnel de `TUNELES` (abre la conexión del host si hace
    /// falta; los diálogos de conexión van al solicitante).
    ActivarTunel {
        tunel_id: i64,
        peticion_id: u64,
    },
    PararTunel {
        tunel_id: i64,
        peticion_id: u64,
    },
    /// Vuelve a levantar un túnel caído (reinicia los contadores).
    RelanzarTunel {
        tunel_id: i64,
        peticion_id: u64,
    },
    /// El cliente ha creado, editado o borrado túneles de este host: el
    /// servidor relee `TUNELES` y para lo que ya no exista o haya cambiado.
    RecargarTuneles {
        host_id: i64,
    },
    /// Ejecuta un snippet ya resuelto (comando sustituido y hosts) en el
    /// servidor. Responde `Hecho` o `Error` con el `peticion_id`.
    LanzarEjecucion {
        peticion_id: u64,
        snippet_id: Option<i64>,
        nombre: String,
        comando: String,
        host_ids: Vec<i64>,
        timeout_seg: u32,
        parar_al_fallo: bool,
        deliberacion: Option<DeliberacionLanzada>,
    },
    /// Cancela una ejecución: los hosts en cola quedan omitidos y los que
    /// están en marcha, cancelados.
    CancelarEjecucion {
        id: u32,
    },
    /// Quita las ejecuciones terminadas.
    LimpiarEjecuciones,
    /// Pide la salida de un host de una ejecución; contesta `Salida`.
    PedirSalida {
        ejecucion_id: u32,
        host_id: i64,
    },
    // ------------------------------------------------------- servicios (v8)
    /// Listado de servicios, timers y contenedores de un host. Contesta
    /// `Servicios` o `Error` con el mismo `peticion_id`.
    ListarServicios {
        peticion_id: u64,
        host_id: i64,
        /// Contraseña de sudo para lo que la exige (diario o Docker): solo si
        /// el cliente la tiene en memoria o el usuario la acaba de dar.
        #[serde(default)]
        sudo: Option<Secreto>,
    },
    /// Consumo de los contenedores en marcha. Contesta `Consumo`.
    PedirConsumo {
        peticion_id: u64,
        host_id: i64,
        #[serde(default)]
        sudo: Option<Secreto>,
    },
    /// Detalle de un servicio, un timer o un contenedor. Contesta `Detalle`.
    DetalleServicio {
        peticion_id: u64,
        host_id: i64,
        objetivo: Objetivo,
        #[serde(default)]
        sudo: Option<Secreto>,
    },
    /// Comprueba una contraseña de sudo en el host antes de confirmar o
    /// deliberar (§7.3). No la guarda.
    ComprobarSudo {
        peticion_id: u64,
        host_id: i64,
        contrasena: Secreto,
    },
    /// Ejecuta una acción sobre un servicio, un timer, un contenedor o un
    /// proyecto. Contesta `AccionServicioHecha` con el mismo `peticion_id`.
    AccionServicio {
        peticion_id: u64,
        host_id: i64,
        objetivo: Objetivo,
        accion: Accion,
        /// La deliberación que la autoriza, si la acción lo exige (T62).
        #[serde(default)]
        deliberacion: Option<DeliberacionLanzada>,
        #[serde(default)]
        sudo: Option<Secreto>,
    },
    /// Abre una suscripción de logs. El servidor manda `LineasLog` hasta
    /// `PararLog` o hasta que el proceso remoto termina (`FinLog`).
    SeguirLog {
        peticion_id: u64,
        host_id: i64,
        objetivo: Objetivo,
        /// Líneas anteriores que se mandan al abrir (0 = ninguna).
        #[serde(default)]
        lineas_iniciales: u32,
        #[serde(default)]
        sudo: Option<Secreto>,
    },
    /// Cierra una suscripción de logs: el servidor cierra el canal y el script
    /// corta el seguimiento en el host.
    PararLog {
        peticion_id: u64,
    },
    // ------------------------------------------------------ terminal (v9)
    /// Captura el historial y la pantalla de una pestaña. El servidor
    /// responde con uno o varios `HistorialLote` con el mismo `peticion_id`
    /// (desde 2^49). Plazo de 5 s.
    CapturarHistorial {
        peticion_id: u64,
        sesion_id: u32,
    },
    /// La ventana tiene o deja de tener la pestaña en el panel enfocado y el
    /// terminal exterior enfocado: el servidor agrega el foco de todas y,
    /// con 1004 activo, manda `CSI I` o `CSI O` al remoto cuando cambia.
    Foco {
        sesion_id: u32,
        enfocada: bool,
    },
    /// Concede la escritura en varios sobre las pestañas dadas. El servidor
    /// recomprueba que siguen abiertas y, si hace falta deliberación, que la
    /// suya es válida y no se ha usado. Responde `EscrituraVariosIniciada`.
    IniciarEscrituraVarios {
        peticion_id: u64,
        sesiones: Vec<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deliberacion_id: Option<i64>,
    },
    /// Escribe los bytes de cada pestaña concedida; si nombra una que no lo
    /// está, se rechaza entero.
    EscribirVarios {
        id: u32,
        pestanas: Vec<BytesPestana>,
    },
    /// Termina la escritura en varios con su motivo.
    TerminarEscrituraVarios {
        id: u32,
        motivo: String,
    },
    /// Abre un túnel temporal a un localhost del host para una pestaña
    /// (enlaces, Fase 15). Responde `TunelTemporal` o `Error`.
    AbrirTunelTemporal {
        peticion_id: u64,
        sesion_id: u32,
        /// `localhost`, `127.0.0.1` o `::1`.
        destino: String,
        puerto: u16,
    },
    CerrarTunelTemporal {
        id: u32,
    },
    /// Lista las sesiones de tmux del host.
    ListarTmux {
        peticion_id: u64,
        host_id: i64,
    },
    /// Termina una sesión de tmux del host (el cliente ya confirmó).
    TerminarTmux {
        peticion_id: u64,
        host_id: i64,
        nombre: String,
    },
    /// Permite el portapapeles del remoto a este host y entrega la petición
    /// pendiente si sigue viva (60 s).
    PortapapelesPermitido {
        sesion_id: u32,
    },
    /// Renombra una pestaña para todas las ventanas (1-32 caracteres, sin
    /// controles; si se repite, se añade « (n)»).
    RenombrarSesion {
        sesion_id: u32,
        nombre: String,
    },
    /// Mueve una pestaña a otra posición (0 = la primera) para todas las
    /// ventanas.
    MoverSesion {
        sesion_id: u32,
        posicion: u32,
    },
    Listar,
    /// Apaga el servidor cerrando las sesiones que queden.
    Parar,
    // ------------------------------------------------------------ escritorios
    /// Abre un escritorio. El servidor contesta `EscritorioAbierto` o `Error`
    /// con el mismo `peticion_id` (rango de escritorios: desde 2^46).
    AbrirEscritorio {
        peticion_id: u64,
        host_id: i64,
    },
    /// Enlace de un solo uso nuevo para reabrir el visor o copiarlo.
    PedirEnlace {
        peticion_id: u64,
        escritorio_id: u32,
    },
    /// Vuelve a abrir una sesión de escritorio caída.
    ReconectarEscritorio {
        peticion_id: u64,
        escritorio_id: u32,
    },
    /// Cierra una sesión de escritorio (la TUI ya pidió confirmación).
    CerrarEscritorio {
        escritorio_id: u32,
    },
    /// Respuesta a `PideHuellaEscritorio`. El cliente ya guardó (o rechazó) la
    /// huella en `HOSTS.huella_escritorio` antes de contestar.
    RespuestaHuellaEscritorio {
        peticion_id: u64,
        aceptada: bool,
    },
    /// Credenciales de un escritorio, del llavero o del diálogo (T25). La
    /// contraseña viaja en `Zeroizing` y nunca se escribe en el log.
    Credenciales {
        peticion_id: u64,
        usuario: String,
        #[serde(default)]
        dominio: Option<String>,
        contrasena: Secreto,
        #[serde(default)]
        recordar: bool,
    },
    Adios,
}

// ---------------------------------------------------------------- servidor → cliente

/// Lo que custodia un servidor de sesiones, tal como lo informa en
/// `VersionIncompatible`. Si el JSON del servidor no trae un campo, se lee
/// como 1 («hay trabajo»): un servidor futuro que añada otro tipo de trabajo
/// nunca se para por error.
/// Cualquier campo desconocido lo invalida entero: el cliente lo lee como
/// «todo lleno» y nunca sustituye un servidor que custodia un trabajo que esta
/// versión no conoce.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrabajoServidor {
    #[serde(default = "uno")]
    pub sesiones: u32,
    #[serde(default = "uno")]
    pub transferencias: u32,
    #[serde(default = "uno")]
    pub tuneles: u32,
    #[serde(default = "uno")]
    pub ejecuciones: u32,
    /// Escritorios remotos abiertos (Fase 12).
    #[serde(default = "uno")]
    pub escritorios: u32,
    /// Acciones de servicio en curso (Fase 13): trabajo custodiado que sigue
    /// aunque se cierre la ventana que lo pidió (T67).
    #[serde(default = "uno")]
    pub acciones_servicio: u32,
    /// Túneles temporales abiertos (Fase 15): mueren con su pestaña, pero un
    /// servidor que los custodia no se sustituye sin preguntar (T67).
    #[serde(default = "uno")]
    pub tuneles_temporales: u32,
}

fn uno() -> u32 {
    1
}

impl TrabajoServidor {
    pub const CAMPOS: [&'static str; 7] = [
        "sesiones",
        "transferencias",
        "tuneles",
        "ejecuciones",
        "escritorios",
        "acciones_servicio",
        "tuneles_temporales",
    ];

    /// No custodia nada de lo que hoy conoce MAGI.
    pub fn vacio(&self) -> bool {
        self.sesiones == 0
            && self.transferencias == 0
            && self.tuneles == 0
            && self.ejecuciones == 0
            && self.escritorios == 0
            && self.acciones_servicio == 0
            && self.tuneles_temporales == 0
    }

    /// Suma de todo lo que custodia (para decidir y para el resumen).
    pub fn total(&self) -> u64 {
        u64::from(self.sesiones)
            + u64::from(self.transferencias)
            + u64::from(self.tuneles)
            + u64::from(self.ejecuciones)
            + u64::from(self.escritorios)
            + u64::from(self.acciones_servicio)
            + u64::from(self.tuneles_temporales)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo")]
pub enum MensajeServidor {
    Bienvenida {
        version: u32,
        /// Versión de la app del servidor («1.1.0»).
        #[serde(default)]
        version_app: String,
        pid: u32,
        /// Id que el servidor asigna a este cliente (para «mín. ventana»).
        cliente_id: u32,
        clientes: u32,
        sesiones: Vec<InfoSesion>,
        /// Cola de transferencias actual, para que una ventana nueva la vea.
        transferencias: Vec<InfoTransferencia>,
        /// Túneles definidos y su estado en vivo, con lo mismo que difunde
        /// `Tuneles`.
        tuneles: Vec<InfoTunel>,
        /// Ejecuciones en memoria (en curso y terminadas de la última hora).
        #[serde(default)]
        ejecuciones: Vec<InfoEjecucion>,
        /// Acciones de servicio en curso (Fase 13), para que una ventana nueva
        /// las vea al entrar (T27).
        #[serde(default)]
        acciones_servicio: Vec<InfoAccion>,
        /// Túneles temporales abiertos, para que una ventana nueva los vea.
        #[serde(default)]
        tuneles_temporales: Vec<InfoTunelTemporal>,
    },
    /// Rechazo del saludo porque el servidor habla otra versión de
    /// protocolo. **Contrato estable** (Fase 9): no se quitan ni renombran
    /// campos, solo se añaden opcionales. El cliente lee este mensaje como
    /// JSON genérico, así entiende el rechazo de cualquier versión, pasada o
    /// futura.
    VersionIncompatible {
        version: u32,
        /// Pid del servidor, para poder pararlo desde un MAGI de otra versión.
        /// Un servidor anterior a la v4 no lo envía.
        #[serde(default)]
        pid: Option<u32>,
        /// Versión de la app del servidor, si la informa.
        #[serde(default)]
        version_app: Option<String>,
        /// Ventanas adjuntas además de la que pregunta.
        #[serde(default)]
        clientes: Option<u32>,
        /// Lo que custodia el servidor; `None` si su versión no lo informa.
        #[serde(default)]
        trabajo: Option<TrabajoServidor>,
    },
    Sesiones {
        lista: Vec<InfoSesion>,
    },
    PantallaCompleta {
        sesion_id: u32,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
        cols: u16,
        filas: u16,
    },
    Datos {
        sesion_id: u32,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
    Redimensionada {
        sesion_id: u32,
        cols: u16,
        filas: u16,
        /// Cliente cuya ventana impone el tamaño mínimo, si lo hay.
        ventana_minima: Option<u32>,
    },
    Estado {
        sesion_id: u32,
        estado: EstadoSesionRemota,
        motivo: Option<String>,
    },
    HuellaDesconocida {
        sesion_id: u32,
        host: String,
        tipo_clave: String,
        huella: String,
    },
    HuellaCambiada {
        sesion_id: u32,
        host: String,
        tipo_clave: String,
        anterior: String,
        nueva: String,
    },
    PideFrase {
        sesion_id: u32,
        host: String,
        intento: u8,
    },
    PideContrasena {
        sesion_id: u32,
        host: String,
        intento: u8,
        recordar_por_defecto: bool,
    },
    /// La conexión de una operación automática (una ejecución, un túnel)
    /// autentica con la contraseña del llavero: el solicitante contesta con
    /// `Contrasena` si la tiene guardada o con `Cerrar` si no. Nunca abre un
    /// diálogo.
    PideLlavero {
        sesion_id: u32,
        host: String,
        usuario: String,
    },
    Ejecutado {
        host_id: i64,
        peticion_id: u64,
        salida: String,
        codigo: i32,
    },
    SinSesion {
        host_id: i64,
        peticion_id: u64,
    },
    /// Canal SFTP listo. `dir_inicio` es el directorio de inicio del usuario
    /// remoto (el que se usa si el host no tiene guardado el suyo).
    SftpAbierto {
        host_id: i64,
        dir_inicio: String,
        #[serde(default)]
        peticion_id: Option<u64>,
        /// Usuario con el que está abierta la conexión y su uid en el host
        /// (del `/etc/passwd` remoto o, si no está, el dueño de `dir_inicio`).
        #[serde(default)]
        usuario_conexion: Option<String>,
        #[serde(default)]
        uid_conexion: Option<u32>,
    },
    DirListado {
        host_id: i64,
        ruta: String,
        entradas: Vec<Entrada>,
        peticion_id: u64,
    },
    /// Difusión de la cola completa: al menos en cada cambio de estado y como
    /// mucho cuatro veces por segundo durante el progreso.
    Transferencias {
        lista: Vec<InfoTransferencia>,
    },
    /// Difusión de los túneles completos (definidos y su estado): en cada
    /// cambio de estado y, para los contadores, como mucho dos veces por
    /// segundo y solo si cambian.
    Tuneles {
        lista: Vec<InfoTunel>,
    },
    /// Difusión de las ejecuciones completas: en cada cambio de estado.
    Ejecuciones {
        lista: Vec<InfoEjecucion>,
    },
    /// Salida de un host de una ejecución (solo a quien la pidió).
    Salida {
        ejecucion_id: u32,
        host_id: i64,
        #[serde(rename = "stdout_b64")]
        stdout: SalidaRemota,
        #[serde(rename = "stderr_b64")]
        stderr: SalidaRemota,
        truncada: bool,
    },
    /// Operación remota terminada (`BorrarRemoto`, `RenombrarRemoto`,
    /// `CrearDirRemoto`, `CambiarPermisos`, `Transferir` encolada…).
    Hecho {
        peticion_id: u64,
        /// Resumen legible (afectados y errores de `CambiarPermisos`).
        #[serde(default)]
        detalle: Option<String>,
    },
    /// Respuesta de `StatRemoto`: sin entrada si la ruta no existe.
    Stat {
        peticion_id: u64,
        entrada: Option<Entrada>,
    },
    /// Un bloque del recorrido de `ListarArbol`. El primero trae el texto del
    /// `.magiignore` si se pidió y existe; el último, `fin` y el recuento de
    /// lo que se dejó fuera por las exclusiones.
    Arbol {
        peticion_id: u64,
        entradas: Vec<EntradaArbol>,
        #[serde(default)]
        magiignore: Option<String>,
        #[serde(default)]
        excluidos: u32,
        fin: bool,
    },
    RutaTemporal {
        ruta: String,
        peticion_id: u64,
    },
    // ------------------------------------------------------------ escritorios
    /// Escritorio abierto, con el enlace de un solo uso para el visor.
    EscritorioAbierto {
        peticion_id: u64,
        escritorio_id: u32,
        enlace: String,
    },
    /// Enlace nuevo pedido con `PedirEnlace`.
    Enlace {
        peticion_id: u64,
        escritorio_id: u32,
        enlace: String,
    },
    /// Difusión de la lista completa de escritorios en cada cambio (T27).
    Escritorios {
        lista: Vec<InfoEscritorio>,
    },
    /// Certificado RDP nuevo o cambiado: el cliente decide y, si acepta, guarda
    /// la huella antes de responder (D218).
    PideHuellaEscritorio {
        peticion_id: u64,
        escritorio_id: u32,
        host_id: i64,
        host_nombre: String,
        datos: DatosCertificado,
    },
    /// Credenciales para un escritorio: las saca el cliente del llavero o del
    /// diálogo (intento 1..=3). El servidor nunca toca el llavero (T25).
    PideCredenciales {
        peticion_id: u64,
        escritorio_id: u32,
        host_nombre: String,
        peticion: PeticionCredenciales,
    },
    // ------------------------------------------------------- servicios (v8)
    /// Listado de servicios, timers y contenedores de un host.
    Servicios {
        peticion_id: u64,
        host_id: i64,
        listado: Listado,
    },
    /// Consumo de los contenedores en marcha de un host.
    Consumo {
        peticion_id: u64,
        host_id: i64,
        consumos: Vec<Consumo>,
    },
    /// Detalle de un servicio, un timer o un contenedor.
    Detalle {
        peticion_id: u64,
        host_id: i64,
        detalle: magi_nucleo::servicios::Detalle,
    },
    /// Resultado de `ComprobarSudo`.
    SudoComprobado {
        peticion_id: u64,
        resultado: ComprobacionSudo,
    },
    /// Resultado de una acción de servicio, cuando termina (o falla al
    /// empezar). Solo al solicitante.
    AccionServicioHecha {
        peticion_id: u64,
        resultado: ResultadoServicio,
    },
    /// Difusión de las acciones de servicio en curso en cada cambio (T27).
    AccionesServicio {
        lista: Vec<InfoAccion>,
    },
    /// Lote de líneas de una suscripción de logs (solo al solicitante).
    LineasLog {
        peticion_id: u64,
        lineas: Vec<LineaLog>,
        /// Líneas descartadas por contrapresión desde el lote anterior.
        #[serde(default)]
        omitidas: u32,
    },
    /// Fin de una suscripción de logs, con su motivo.
    FinLog {
        peticion_id: u64,
        motivo: MotivoFinLog,
    },
    // ------------------------------------------------------ terminal (v9)
    /// Un lote de la captura del historial. `lote` es el índice y `ultimo`
    /// dice si es el último; los lotes se cortan por tamaño codificado.
    HistorialLote {
        peticion_id: u64,
        sesion_id: u32,
        lote: u32,
        ultimo: bool,
        /// Líneas totales de la captura (el historial más la pantalla).
        total: u32,
        lineas: Vec<LineaHistorial>,
    },
    /// La escritura en varios se ha concedido.
    EscrituraVariosIniciada {
        peticion_id: u64,
        id: u32,
    },
    /// La escritura en varios ha terminado (por el cliente, por la caída de
    /// una pestaña o por inactividad).
    EscrituraVariosTerminada {
        id: u32,
        motivo: String,
    },
    /// Túnel temporal abierto.
    TunelTemporal {
        peticion_id: u64,
        id: u32,
        puerto_local: u16,
    },
    /// Difusión de los túneles temporales completos en cada cambio (T27).
    TunelesTemporales {
        lista: Vec<InfoTunelTemporal>,
    },
    /// Sesiones de tmux del host. `sin_tmux` lleva el motivo si no hay tmux.
    SesionesTmux {
        peticion_id: u64,
        host_id: i64,
        sesiones: Vec<InfoSesionTmux>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sin_tmux: Option<String>,
    },
    /// Resultado de terminar una sesión de tmux.
    TmuxTerminada {
        peticion_id: u64,
        resultado: ResultadoTmux,
    },
    /// Copia del remoto por OSC 52 con permiso del host: al portapapeles del
    /// sistema (u OSC 52 hacia fuera).
    Portapapeles {
        sesion_id: u32,
        texto: String,
    },
    /// El remoto quiere copiar y este host no lo tiene permitido: aviso con el
    /// tamaño y `prefijo P`.
    PortapapelesPedido {
        sesion_id: u32,
        bytes: u64,
    },
    Error {
        mensaje: String,
        /// Petición que provocó el error, si venía de una.
        #[serde(default)]
        peticion_id: Option<u64>,
    },
}

// ---------------------------------------------------------------- codificación

/// Serializa un mensaje en una línea JSON (sin el `\n`, que pone el codec).
pub fn codificar<M: Serialize>(mensaje: &M) -> Result<String, serde_json::Error> {
    serde_json::to_string(mensaje)
}

/// Descodifica una línea JSON. Cualquier fallo (JSON inválido o tipo
/// desconocido) se convierte en motivo de `Error` y desconexión.
pub fn decodificar<M: DeserializeOwned>(linea: &str) -> Result<M, String> {
    serde_json::from_str(linea).map_err(|error| error.to_string())
}

#[cfg(test)]
mod pruebas {

    use super::*;

    use magi_nucleo::terminal::tramos::{Color, Tramo};

    fn ida_y_vuelta_cliente(mensaje: MensajeCliente) {
        let linea = codificar(&mensaje).unwrap();
        assert_eq!(decodificar::<MensajeCliente>(&linea).unwrap(), mensaje);
    }

    fn ida_y_vuelta_servidor(mensaje: MensajeServidor) {
        let linea = codificar(&mensaje).unwrap();
        assert_eq!(decodificar::<MensajeServidor>(&linea).unwrap(), mensaje);
    }

    #[test]
    fn los_mensajes_del_cliente_hacen_ida_y_vuelta() {
        ida_y_vuelta_cliente(MensajeCliente::Hola {
            version: 1,
            pid: 4321,
        });
        ida_y_vuelta_cliente(MensajeCliente::AbrirSesion {
            host_id: 7,
            cols: 120,
            filas: 38,
            comandos_iniciales: vec![ComandoInicial {
                texto: "tmux attach\n".to_string(),
                repetir: true,
            }],
            orden: None,
            tmux: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::Adjuntar {
            sesion_id: 3,
            cols: 80,
            filas: 24,
            colores: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::Desadjuntar { sesion_id: 3 });
        ida_y_vuelta_cliente(MensajeCliente::Teclas {
            sesion_id: 3,
            bytes: b"ls -la\r".to_vec(),
        });
        ida_y_vuelta_cliente(MensajeCliente::Redimensionar {
            sesion_id: 3,
            cols: 100,
            filas: 30,
        });
        ida_y_vuelta_cliente(MensajeCliente::Cerrar { sesion_id: 3 });
        ida_y_vuelta_cliente(MensajeCliente::Reconectar { sesion_id: 3 });
        ida_y_vuelta_cliente(MensajeCliente::DecisionHuella {
            sesion_id: 3,
            decision: true,
        });
        ida_y_vuelta_cliente(MensajeCliente::Frase {
            sesion_id: 3,
            frase: Secreto::nuevo("secreta"),
        });
        ida_y_vuelta_cliente(MensajeCliente::Contrasena {
            sesion_id: 3,
            contrasena: Secreto::nuevo("muy secreta"),
            recordar: true,
        });
        ida_y_vuelta_cliente(MensajeCliente::Ejecutar {
            host_id: 7,
            comando: "uptime".to_string(),
            peticion_id: 1 << 48,
        });
        ida_y_vuelta_cliente(MensajeCliente::Listar);
        ida_y_vuelta_cliente(MensajeCliente::Parar);
        ida_y_vuelta_cliente(MensajeCliente::Adios);
    }

    /// Los mensajes que estrena la Fase 4 (v2): archivos y transferencias.
    #[test]
    fn los_mensajes_de_archivos_hacen_ida_y_vuelta() {
        ida_y_vuelta_cliente(MensajeCliente::AbrirSftp {
            host_id: 7,
            peticion_id: None,
            no_interactivo: false,
        });
        ida_y_vuelta_cliente(MensajeCliente::AbrirSftp {
            host_id: 7,
            peticion_id: Some(1 << 48),
            no_interactivo: true,
        });
        ida_y_vuelta_cliente(MensajeCliente::ListarDir {
            host_id: 7,
            ruta: "/var/www".to_string(),
            peticion_id: 4,
        });
        ida_y_vuelta_cliente(MensajeCliente::Transferir {
            host_id: 7,
            direccion: Direccion::Subida,
            elementos: vec![ElementoTransferencia {
                origen: "static".to_string(),
                destino: "/var/www/static".to_string(),
                bytes: 4096,
                es_directorio: true,
                politica: Some(Politica::Omitir),
                permisos: None,
            }],
            politica: Politica::Sobrescribir,
            borrar_origen: false,
            peticion_id: None,
            borrar_al_terminar: Vec::new(),
            deliberacion: None,
            sincronizacion: None,
            etiqueta: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::CancelarTransferencia { id: 2 });
        ida_y_vuelta_cliente(MensajeCliente::LimpiarTransferencias);
        ida_y_vuelta_cliente(MensajeCliente::BorrarRemoto {
            host_id: 7,
            rutas: vec!["/var/www/viejo".to_string()],
            peticion_id: 5,
        });
        ida_y_vuelta_cliente(MensajeCliente::RenombrarRemoto {
            host_id: 7,
            de: "/var/www/a".to_string(),
            a: "/var/www/b".to_string(),
            peticion_id: 6,
        });
        ida_y_vuelta_cliente(MensajeCliente::CrearDirRemoto {
            host_id: 7,
            ruta: "/var/www/nuevo".to_string(),
            peticion_id: 7,
        });
        ida_y_vuelta_cliente(MensajeCliente::DescargarTemporal {
            host_id: 7,
            ruta: "/var/log/nginx/error.log".to_string(),
            peticion_id: 8,
            edicion: false,
        });
        ida_y_vuelta_cliente(MensajeCliente::BorrarTemporal {
            ruta: "/run/magi/tmp/8-error.log".to_string(),
        });

        ida_y_vuelta_servidor(MensajeServidor::SftpAbierto {
            host_id: 7,
            peticion_id: Some(1 << 48),
            dir_inicio: "/home/usuario".to_string(),
            usuario_conexion: None,
            uid_conexion: None,
        });
        ida_y_vuelta_servidor(MensajeServidor::DirListado {
            host_id: 7,
            ruta: "/var/www".to_string(),
            entradas: vec![
                Entrada {
                    nombre: "app".to_string(),
                    tipo: crate::archivos::TipoEntrada::Directorio,
                    tamano: 4096,
                    mtime: 1_700_000_000,
                    permisos: Some(0o755),
                    propietario: Some(crate::archivos::Propietario {
                        uid: Some(1000),
                        gid: Some(1000),
                        usuario: Some("usuario".to_string()),
                        grupo: Some("usuario".to_string()),
                    }),
                    enlace: None,
                    marca: Default::default(),
                },
                Entrada {
                    nombre: "index.html".to_string(),
                    tipo: crate::archivos::TipoEntrada::Fichero,
                    tamano: 14_208,
                    mtime: 1_700_000_000,
                    permisos: Some(0o644),
                    propietario: None,
                    enlace: None,
                    marca: crate::archivos::Marca::Distinta,
                },
            ],
            peticion_id: 4,
        });
        ida_y_vuelta_servidor(MensajeServidor::Transferencias {
            lista: vec![InfoTransferencia {
                id: 1,
                host_id: 7,
                host_nombre: "hetzner-01".to_string(),
                direccion: Direccion::Bajada,
                estado: EstadoTransferencia::EnCurso,
                origen: "/var/log/nginx".to_string(),
                destino: "~/logs".to_string(),
                es_directorio: true,
                ficheros_total: 14,
                ficheros_hechos: 3,
                omitidos: 2,
                bytes_total: 12_582_912,
                bytes_hechos: 3_145_728,
                fichero_actual: Some("access.log".to_string()),
                error: None,
                borrar_origen: false,
                solicitante: 1,
                creada_en: 1_700_000_000,
                terminada_en: None,
                peticion_id: None,
                etiqueta: None,
                borrados: 0,
                borrados_total: 0,
            }],
        });
        ida_y_vuelta_servidor(MensajeServidor::Hecho {
            peticion_id: 5,
            detalle: None,
        });
        ida_y_vuelta_servidor(MensajeServidor::RutaTemporal {
            ruta: "/run/magi/tmp/8-error.log".to_string(),
            peticion_id: 8,
        });
        ida_y_vuelta_servidor(MensajeServidor::Error {
            mensaje: "la ruta no existe".to_string(),
            peticion_id: Some(4),
        });
    }

    /// Los mensajes que estrena la Fase 8 (v5) y las ampliaciones de los de
    /// archivos hacen ida y vuelta.
    #[test]
    fn los_mensajes_de_la_fase_8_hacen_ida_y_vuelta() {
        ida_y_vuelta_cliente(MensajeCliente::StatRemoto {
            peticion_id: (1 << 42) + 1,
            host_id: 7,
            ruta: "/etc/nginx/nginx.conf".to_string(),
        });
        ida_y_vuelta_cliente(MensajeCliente::CambiarPermisos {
            peticion_id: (1 << 42) + 2,
            host_id: 7,
            rutas: vec!["/var/www".to_string(), "/var/www/index.html".to_string()],
            modo: 0o644,
            mascara: 0o777,
            alcance: Some(AlcancePermisos::Ficheros),
        });
        ida_y_vuelta_cliente(MensajeCliente::ListarArbol {
            peticion_id: (1 << 42) + 3,
            host_id: 7,
            ruta: "/var/www/app".to_string(),
            exclusiones: vec![".git/".to_string(), "*.log".to_string()],
            usar_magiignore: true,
            exclusiones_extra: vec!["tmp/".to_string()],
        });
        ida_y_vuelta_cliente(MensajeCliente::DescargarTemporal {
            host_id: 7,
            ruta: "/etc/hosts".to_string(),
            peticion_id: (1 << 42) + 4,
            edicion: true,
        });
        ida_y_vuelta_cliente(MensajeCliente::Transferir {
            host_id: 7,
            direccion: Direccion::Subida,
            elementos: vec![ElementoTransferencia {
                origen: "/home/usuario/web/index.html".to_string(),
                destino: "/var/www/index.html".to_string(),
                bytes: 120,
                es_directorio: false,
                politica: Some(Politica::Sobrescribir),
                permisos: Some(0o644),
            }],
            politica: Politica::Sobrescribir,
            borrar_origen: false,
            peticion_id: Some((1 << 42) + 5),
            borrar_al_terminar: vec!["/var/www/viejo.css".to_string()],
            deliberacion: Some(DeliberacionLanzada {
                id: 3,
                forzada: false,
                motivo: None,
            }),
            sincronizacion: Some(SincronizacionLanzada {
                id: Some(2),
                nombre: Some("web-prod".to_string()),
                creados: 1,
                actualizados: 0,
                omitidos: 0,
                raiz_origen: "/home/usuario/web".to_string(),
                raiz_destino: "/var/www".to_string(),
            }),
            etiqueta: Some(EtiquetaTransferencia::Sincronizacion),
        });
        ida_y_vuelta_servidor(MensajeServidor::Stat {
            peticion_id: (1 << 42) + 1,
            entrada: Some(Entrada {
                nombre: "nginx.conf".to_string(),
                tipo: TipoEntrada::Fichero,
                tamano: 2041,
                mtime: 1_700_000_000,
                permisos: Some(0o100644),
                propietario: Some(Propietario {
                    uid: Some(0),
                    gid: Some(0),
                    usuario: Some("root".to_string()),
                    grupo: None,
                }),
                enlace: None,
                marca: Default::default(),
            }),
        });
        ida_y_vuelta_servidor(MensajeServidor::Stat {
            peticion_id: (1 << 42) + 1,
            entrada: None,
        });
        ida_y_vuelta_servidor(MensajeServidor::Arbol {
            peticion_id: (1 << 42) + 3,
            entradas: vec![
                EntradaArbol {
                    ruta: "static/app.js".to_string(),
                    tipo: TipoEntrada::Fichero,
                    tamano: 88_000,
                    mtime: 1_700_000_000,
                    permisos: Some(0o100644),
                    propietario: None,
                    enlace_a_dir: false,
                },
                EntradaArbol {
                    ruta: "static/compartido".to_string(),
                    tipo: TipoEntrada::Enlace,
                    tamano: 0,
                    mtime: 0,
                    permisos: None,
                    propietario: None,
                    enlace_a_dir: true,
                },
            ],
            magiignore: Some("*.log\n".to_string()),
            excluidos: 0,
            fin: false,
        });
        ida_y_vuelta_servidor(MensajeServidor::Arbol {
            peticion_id: (1 << 42) + 3,
            entradas: Vec::new(),
            magiignore: None,
            excluidos: 37,
            fin: true,
        });
        ida_y_vuelta_servidor(MensajeServidor::Hecho {
            peticion_id: (1 << 42) + 2,
            detalle: Some("2 afectados · 0 errores".to_string()),
        });
        ida_y_vuelta_servidor(MensajeServidor::SftpAbierto {
            host_id: 7,
            dir_inicio: "/home/deploy".to_string(),
            peticion_id: None,
            usuario_conexion: Some("deploy".to_string()),
            uid_conexion: Some(1001),
        });
    }

    /// Un `Transferir` de la v4 (sin los campos nuevos) se lee con sus
    /// valores por defecto: los campos nuevos son opcionales en el cable.
    #[test]
    fn un_transferir_sin_los_campos_nuevos_se_lee_con_defectos() {
        let linea = r#"{"tipo":"Transferir","host_id":7,"direccion":"Subida","elementos":[],"politica":"Omitir","borrar_origen":false}"#;
        match decodificar::<MensajeCliente>(linea).unwrap() {
            MensajeCliente::Transferir {
                peticion_id,
                borrar_al_terminar,
                deliberacion,
                sincronizacion,
                etiqueta,
                ..
            } => {
                assert_eq!(peticion_id, None);
                assert!(borrar_al_terminar.is_empty());
                assert!(deliberacion.is_none() && sincronizacion.is_none() && etiqueta.is_none());
            }
            otro => panic!("{otro:?}"),
        }
    }

    /// Un `Error` sin `peticion_id` (el que manda la versión 1 del protocolo)
    /// se decodifica como si no trajera petición.
    #[test]
    fn un_error_sin_peticion_se_decodifica_como_ninguno() {
        let linea = r#"{"tipo":"Error","mensaje":"algo"}"#;
        assert_eq!(
            decodificar::<MensajeServidor>(linea).unwrap(),
            MensajeServidor::Error {
                mensaje: "algo".to_string(),
                peticion_id: None,
            }
        );
    }

    #[test]
    fn la_version_del_protocolo_es_la_nueve() {
        assert_eq!(VERSION_PROTOCOLO, 9);
    }

    /// Un servidor anterior a la v4 no manda su pid: se lee como ninguno.
    #[test]
    fn version_incompatible_sin_pid_se_lee_como_ninguno() {
        let linea = r#"{"tipo":"VersionIncompatible","version":3}"#;
        assert_eq!(
            decodificar::<MensajeServidor>(linea).unwrap(),
            MensajeServidor::VersionIncompatible {
                version: 3,
                pid: None,
                version_app: None,
                clientes: None,
                trabajo: None,
            }
        );
    }

    /// Los mensajes de túneles hacen ida y vuelta por los dos sentidos.
    #[test]
    fn los_mensajes_de_tuneles_hacen_ida_y_vuelta() {
        for mensaje in [
            MensajeCliente::ActivarTunel {
                tunel_id: 3,
                peticion_id: 10,
            },
            MensajeCliente::PararTunel {
                tunel_id: 3,
                peticion_id: 11,
            },
            MensajeCliente::RelanzarTunel {
                tunel_id: 3,
                peticion_id: 12,
            },
            MensajeCliente::RecargarTuneles { host_id: 7 },
        ] {
            ida_y_vuelta_cliente(mensaje);
        }
        let tunel = InfoTunel {
            tunel_id: 3,
            host_id: 7,
            host_nombre: "hetzner-01".to_string(),
            nombre: "pg-prod".to_string(),
            tipo: "local".to_string(),
            escucha: "127.0.0.1:5432".to_string(),
            escucha_efectiva: "127.0.0.1:5432".to_string(),
            destino: Some("192.0.2.5:5432".to_string()),
            automatico: true,
            estado: EstadoTunelRemoto::Activo,
            origen: Some(OrigenTunel::Automatico),
            solicitante: Some(2),
            conexiones: 1,
            aceptadas: 3,
            bytes_subidos: 1024,
            bytes_bajados: 2048,
            desde: Some(1_700_000_000),
            ultimo_error: None,
        };
        ida_y_vuelta_servidor(MensajeServidor::Tuneles {
            lista: vec![tunel.clone()],
        });
        ida_y_vuelta_servidor(MensajeServidor::Bienvenida {
            version: VERSION_PROTOCOLO,
            version_app: "1.0.0".to_string(),
            pid: 100,
            cliente_id: 3,
            clientes: 2,
            sesiones: Vec::new(),
            transferencias: Vec::new(),
            tuneles: vec![tunel],
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: Vec::new(),
        });
    }

    /// Los mensajes de ejecuciones (Fase 6) hacen ida y vuelta.
    #[test]
    fn los_mensajes_de_ejecuciones_hacen_ida_y_vuelta() {
        ida_y_vuelta_cliente(MensajeCliente::LanzarEjecucion {
            peticion_id: 1 << 44,
            snippet_id: Some(3),
            nombre: "reiniciar nginx".to_string(),
            comando: "systemctl restart nginx".to_string(),
            host_ids: vec![1, 2],
            timeout_seg: 60,
            parar_al_fallo: true,
            deliberacion: Some(DeliberacionLanzada {
                id: 12,
                forzada: true,
                motivo: Some("revisado a mano".to_string()),
            }),
        });
        ida_y_vuelta_cliente(MensajeCliente::CancelarEjecucion { id: 4 });
        ida_y_vuelta_cliente(MensajeCliente::LimpiarEjecuciones);
        ida_y_vuelta_cliente(MensajeCliente::PedirSalida {
            ejecucion_id: 4,
            host_id: 2,
        });
        let ejecucion = InfoEjecucion {
            id: 4,
            peticion_id: 1 << 44,
            solicitante: 1,
            snippet_id: Some(3),
            nombre: "reiniciar nginx".to_string(),
            hosts: vec![InfoEjecucionHost {
                host_id: 2,
                nombre: "hetzner-02".to_string(),
                estado: EstadoHostEjecucion::Fallo,
                codigo: Some(1),
                inicio_ms: Some(1_700_000_000_000),
                duracion_ms: Some(800),
                bytes_stdout: 0,
                bytes_stderr: 1400,
                truncada: false,
                error: None,
            }],
            timeout_seg: 60,
            parar_al_fallo: true,
            deliberacion_id: Some(12),
            forzada: true,
            estado: EstadoEjecucion::Terminada,
            creada_en: 1_700_000_000,
            terminada_en: Some(1_700_000_002),
        };
        ida_y_vuelta_servidor(MensajeServidor::Ejecuciones {
            lista: vec![ejecucion],
        });
        ida_y_vuelta_servidor(MensajeServidor::Salida {
            ejecucion_id: 4,
            host_id: 2,
            stdout: SalidaRemota(b"hola\n".to_vec()),
            stderr: SalidaRemota(b"\x1b[31merror\x1b[0m".to_vec()),
            truncada: false,
        });
    }

    /// La salida viaja en base64 con el nombre del informe y el depurado no
    /// enseña los bytes.
    #[test]
    fn la_salida_viaja_en_base64_y_no_se_depura() {
        let mensaje = MensajeServidor::Salida {
            ejecucion_id: 1,
            host_id: 1,
            stdout: SalidaRemota(b"secreto".to_vec()),
            stderr: SalidaRemota::default(),
            truncada: false,
        };
        let linea = codificar(&mensaje).unwrap();
        assert!(linea.contains("\"stdout_b64\":\"c2VjcmV0bw==\""), "{linea}");
        let depurado = format!("{mensaje:?}");
        assert!(!depurado.contains("secreto"), "{depurado}");
        assert!(depurado.contains("<7 B>"), "{depurado}");
    }

    /// Una salida máxima (1 MiB por flujo) cabe en una línea del protocolo.
    #[test]
    fn una_salida_maxima_cabe_en_una_linea() {
        let mensaje = MensajeServidor::Salida {
            ejecucion_id: 1,
            host_id: 1,
            stdout: SalidaRemota(vec![0xff; 1024 * 1024]),
            stderr: SalidaRemota(vec![0xfe; 1024 * 1024]),
            truncada: true,
        };
        assert!(codificar(&mensaje).unwrap().len() < LINEA_MAXIMA);
    }

    /// Un `AbrirSesion` de un cliente sin comandos iniciales ni orden se lee
    /// igual (v8 añade `orden` como opcional en el cable).
    #[test]
    fn abrir_sesion_sin_comandos_iniciales_se_lee() {
        let linea = r#"{"tipo":"AbrirSesion","host_id":1,"cols":80,"filas":24}"#;
        match decodificar::<MensajeCliente>(linea).unwrap() {
            MensajeCliente::AbrirSesion {
                comandos_iniciales,
                orden,
                ..
            } => {
                assert!(comandos_iniciales.is_empty());
                assert_eq!(orden, None);
            }
            otro => panic!("{otro:?}"),
        }
    }

    /// Los mensajes de servicios (v8) hacen ida y vuelta.
    #[test]
    fn los_mensajes_de_servicios_hacen_ida_y_vuelta() {
        let objetivo = Objetivo::unidad("app.service");
        ida_y_vuelta_cliente(MensajeCliente::ListarServicios {
            peticion_id: RANGO_SERVICIOS + 1,
            host_id: 7,
            sudo: Some(Secreto::nuevo("secreta")),
        });
        ida_y_vuelta_cliente(MensajeCliente::PedirConsumo {
            peticion_id: RANGO_SERVICIOS + 2,
            host_id: 7,
            sudo: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::DetalleServicio {
            peticion_id: RANGO_SERVICIOS + 3,
            host_id: 7,
            objetivo: objetivo.clone(),
            sudo: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::ComprobarSudo {
            peticion_id: RANGO_SERVICIOS + 4,
            host_id: 7,
            contrasena: Secreto::nuevo("secreta"),
        });
        ida_y_vuelta_cliente(MensajeCliente::AccionServicio {
            peticion_id: RANGO_SERVICIOS + 5,
            host_id: 7,
            objetivo: objetivo.clone(),
            accion: Accion::Reiniciar,
            deliberacion: Some(DeliberacionLanzada {
                id: 12,
                forzada: false,
                motivo: None,
            }),
            sudo: Some(Secreto::nuevo("secreta")),
        });
        ida_y_vuelta_cliente(MensajeCliente::SeguirLog {
            peticion_id: RANGO_SERVICIOS + 6,
            host_id: 7,
            objetivo: objetivo.clone(),
            lineas_iniciales: 200,
            sudo: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::PararLog {
            peticion_id: RANGO_SERVICIOS + 6,
        });
        // Pestañas con orden (D248).
        ida_y_vuelta_cliente(MensajeCliente::AbrirSesion {
            host_id: 7,
            cols: 120,
            filas: 38,
            comandos_iniciales: Vec::new(),
            orden: Some(OrdenPestana::ShellContenedor {
                motor: MotorNombre::Docker,
                id: "web-app-1".to_string(),
            }),
            tmux: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::AbrirSesion {
            host_id: 7,
            cols: 80,
            filas: 24,
            comandos_iniciales: Vec::new(),
            orden: Some(OrdenPestana::SeguirDiario {
                unidad: "app.service".to_string(),
            }),
            tmux: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::AbrirSesion {
            host_id: 7,
            cols: 80,
            filas: 24,
            comandos_iniciales: Vec::new(),
            orden: Some(OrdenPestana::SeguirContenedor {
                motor: MotorNombre::Podman,
                id: "0af5c1e9a1b2".to_string(),
            }),
            tmux: None,
        });

        let listado = Listado::vacio(Privilegio::SudoSinClave);
        let accion = InfoAccion {
            id: RANGO_SERVICIOS + 5,
            host_id: 7,
            host_nombre: "prueba-1".to_string(),
            objetivo: objetivo.clone(),
            accion: Accion::Reiniciar,
            fase: FaseAccion::Esperando,
            desde_ms: 1_700_000_000_000,
            solicitante: 2,
        };
        ida_y_vuelta_servidor(MensajeServidor::Servicios {
            peticion_id: RANGO_SERVICIOS + 1,
            host_id: 7,
            listado,
        });
        ida_y_vuelta_servidor(MensajeServidor::Consumo {
            peticion_id: RANGO_SERVICIOS + 2,
            host_id: 7,
            consumos: vec![Consumo {
                id: "web-app-1".to_string(),
                cpu_pct: Some(2.5),
                memoria_bytes: Some(64 * 1024 * 1024),
                memoria_limite_bytes: Some(256 * 1024 * 1024),
            }],
        });
        ida_y_vuelta_servidor(MensajeServidor::Detalle {
            peticion_id: RANGO_SERVICIOS + 3,
            host_id: 7,
            detalle: Detalle {
                objetivo: objetivo.clone(),
                props: std::collections::BTreeMap::new(),
                necesita: vec!["network.target".to_string()],
                necesitada: Vec::new(),
                texto: "[Unit]".to_string(),
                contenedor: None,
            },
        });
        ida_y_vuelta_servidor(MensajeServidor::SudoComprobado {
            peticion_id: RANGO_SERVICIOS + 4,
            resultado: ComprobacionSudo::Ok,
        });
        ida_y_vuelta_servidor(MensajeServidor::AccionServicioHecha {
            peticion_id: RANGO_SERVICIOS + 5,
            resultado: ResultadoServicio {
                resultado: ResultadoAccion::Hecho,
                estado: "active".to_string(),
                segundos: 2.3,
                mensaje: String::new(),
                lineas: vec!["línea del diario".to_string()],
                comprobacion: None,
            },
        });
        ida_y_vuelta_servidor(MensajeServidor::AccionesServicio {
            lista: vec![accion],
        });
        ida_y_vuelta_servidor(MensajeServidor::LineasLog {
            peticion_id: RANGO_SERVICIOS + 6,
            lineas: vec![LineaLog {
                hora: Some(1_700_000_000),
                prioridad: 3,
                stderr: false,
                texto: "error de prueba".to_string(),
            }],
            omitidas: 4,
        });
        ida_y_vuelta_servidor(MensajeServidor::FinLog {
            peticion_id: RANGO_SERVICIOS + 6,
            motivo: MotivoFinLog::Cerrado,
        });
        ida_y_vuelta_servidor(MensajeServidor::Bienvenida {
            version: VERSION_PROTOCOLO,
            version_app: "1.3.0".to_string(),
            pid: 100,
            cliente_id: 3,
            clientes: 1,
            sesiones: Vec::new(),
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: Vec::new(),
        });
    }

    /// Un `Bienvenida` de una versión anterior (sin `acciones_servicio`) se
    /// lee como lista vacía.
    #[test]
    fn una_bienvenida_sin_acciones_se_lee_vacia() {
        let linea = r#"{"tipo":"Bienvenida","version":8,"pid":1,"cliente_id":1,"clientes":1,"sesiones":[],"transferencias":[],"tuneles":[],"ejecuciones":[]}"#;
        match decodificar::<MensajeServidor>(linea).unwrap() {
            MensajeServidor::Bienvenida {
                acciones_servicio, ..
            } => assert!(acciones_servicio.is_empty()),
            otro => panic!("{otro:?}"),
        }
    }

    /// `TrabajoServidor` cuenta las acciones de servicio en curso: un
    /// servidor con una acción a medias no se sustituye sin preguntar (T67).
    #[test]
    fn el_trabajo_cuenta_las_acciones_de_servicio() {
        let trabajo = TrabajoServidor {
            sesiones: 0,
            transferencias: 0,
            tuneles: 0,
            ejecuciones: 0,
            escritorios: 0,
            acciones_servicio: 1,
            tuneles_temporales: 0,
        };
        assert!(!trabajo.vacio());
        assert_eq!(trabajo.total(), 1);
        assert!(TrabajoServidor::CAMPOS.contains(&"acciones_servicio"));
        assert_eq!(TrabajoServidor::CAMPOS.len(), 7);
        // Un servidor que no manda el campo vale 1 («puede haber trabajo»).
        let linea = r#"{"tipo":"VersionIncompatible","version":8,"trabajo":{"sesiones":0,"transferencias":0,"tuneles":0,"ejecuciones":0,"escritorios":0}}"#;
        match decodificar::<MensajeServidor>(linea).unwrap() {
            MensajeServidor::VersionIncompatible {
                trabajo: Some(trabajo),
                ..
            } => assert_eq!(trabajo.acciones_servicio, 1),
            otro => panic!("{otro:?}"),
        }
    }

    /// El glifo y el texto de cada estado, para que la vista no se desvíe.
    #[test]
    fn cada_estado_de_tunel_tiene_texto_y_glifo() {
        for estado in [
            EstadoTunelRemoto::Inactivo,
            EstadoTunelRemoto::Activando,
            EstadoTunelRemoto::Activo,
            EstadoTunelRemoto::Parando,
            EstadoTunelRemoto::Caido,
        ] {
            assert!(!estado.texto().is_empty());
            assert!(!estado.glifo(false).is_empty());
            assert!(!estado.glifo(true).is_empty());
        }
    }

    #[test]
    fn los_mensajes_del_servidor_hacen_ida_y_vuelta() {
        let sesiones = vec![InfoSesion {
            id: 1,
            nombre: "hetzner-01".to_string(),
            host_id: 7,
            host_nombre: "hetzner-01".to_string(),
            estado: EstadoSesionRemota::Abierta,
            motivo: None,
            identidad: "ed25519 · agente".to_string(),
            abierta_en: 1_700_000_000,
            ventanas: 2,
            actividad_no_vista: false,
            reconexion: None,
            titulo: None,
            aviso: None,
            tmux: None,
            escritura_varios: false,
        }];
        ida_y_vuelta_servidor(MensajeServidor::Bienvenida {
            version: 1,
            version_app: "0.9.0".to_string(),
            pid: 100,
            cliente_id: 3,
            clientes: 2,
            sesiones: sesiones.clone(),
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: Vec::new(),
        });
        ida_y_vuelta_servidor(MensajeServidor::VersionIncompatible {
            version: 99,
            pid: Some(4321),
            version_app: Some("2.0.0".to_string()),
            clientes: Some(2),
            trabajo: Some(TrabajoServidor {
                sesiones: 2,
                transferencias: 0,
                tuneles: 1,
                ejecuciones: 0,
                escritorios: 0,
                acciones_servicio: 0,
                tuneles_temporales: 0,
            }),
        });
        ida_y_vuelta_servidor(MensajeServidor::Sesiones { lista: sesiones });
        ida_y_vuelta_servidor(MensajeServidor::PantallaCompleta {
            sesion_id: 1,
            bytes: b"\x1b[31mrojo\x1b[0m".to_vec(),
            cols: 120,
            filas: 38,
        });
        ida_y_vuelta_servidor(MensajeServidor::Datos {
            sesion_id: 1,
            bytes: b"salida\r\n".to_vec(),
        });
        ida_y_vuelta_servidor(MensajeServidor::Redimensionada {
            sesion_id: 1,
            cols: 100,
            filas: 30,
            ventana_minima: Some(2),
        });
        ida_y_vuelta_servidor(MensajeServidor::Estado {
            sesion_id: 1,
            estado: EstadoSesionRemota::Caida,
            motivo: Some("conexión cerrada por el remoto".to_string()),
        });
        ida_y_vuelta_servidor(MensajeServidor::HuellaDesconocida {
            sesion_id: 1,
            host: "hetzner-01".to_string(),
            tipo_clave: "ed25519".to_string(),
            huella: "SHA256:abc".to_string(),
        });
        ida_y_vuelta_servidor(MensajeServidor::HuellaCambiada {
            sesion_id: 1,
            host: "hetzner-01".to_string(),
            tipo_clave: "ed25519".to_string(),
            anterior: "SHA256:vieja".to_string(),
            nueva: "SHA256:nueva".to_string(),
        });
        ida_y_vuelta_servidor(MensajeServidor::PideFrase {
            sesion_id: 1,
            host: "hetzner-01".to_string(),
            intento: 2,
        });
        ida_y_vuelta_servidor(MensajeServidor::PideContrasena {
            sesion_id: 1,
            host: "hetzner-01".to_string(),
            intento: 1,
            recordar_por_defecto: true,
        });
        ida_y_vuelta_servidor(MensajeServidor::PideLlavero {
            sesion_id: 9,
            host: "hetzner-01".to_string(),
            usuario: "usuario".to_string(),
        });
        ida_y_vuelta_servidor(MensajeServidor::Ejecutado {
            host_id: 7,
            peticion_id: 1 << 48,
            salida: "load average".to_string(),
            codigo: 0,
        });
        ida_y_vuelta_servidor(MensajeServidor::SinSesion {
            host_id: 7,
            peticion_id: 1 << 48,
        });
        ida_y_vuelta_servidor(MensajeServidor::Error {
            mensaje: "mensaje desconocido".to_string(),
            peticion_id: None,
        });
    }

    #[test]
    fn los_bytes_viajan_en_base64() {
        let linea = codificar(&MensajeServidor::Datos {
            sesion_id: 1,
            bytes: b"texto".to_vec(),
        })
        .unwrap();
        assert!(linea.contains("dGV4dG8="), "{linea}");
        assert!(!linea.contains("texto"));
    }

    #[test]
    fn los_secretos_no_se_pintan_en_el_depurado() {
        let mensaje = MensajeCliente::Frase {
            sesion_id: 1,
            frase: Secreto::nuevo("frase super secreta"),
        };
        let depurado = format!("{mensaje:?}");
        assert!(!depurado.contains("secreta"), "{depurado}");
        assert!(depurado.contains("***"));
    }

    #[test]
    fn un_mensaje_desconocido_no_se_decodifica() {
        let linea = r#"{"tipo":"RotarColumnas","sesion_id":1}"#;
        assert!(decodificar::<MensajeCliente>(linea).is_err());
        let linea_mala = "{esto no es json";
        assert!(decodificar::<MensajeCliente>(linea_mala).is_err());
    }

    #[test]
    fn la_version_del_saludo_viaja_y_se_lee() {
        let linea = codificar(&MensajeCliente::Hola {
            version: 99,
            pid: 1,
        })
        .unwrap();
        match decodificar::<MensajeCliente>(&linea).unwrap() {
            MensajeCliente::Hola { version, .. } => assert_eq!(version, 99),
            otro => panic!("mensaje inesperado: {otro:?}"),
        }
    }

    /// Los mensajes que estrena el terminal en la Fase 15 (v9) hacen ida y
    /// vuelta por el lado del cliente.
    #[test]
    fn los_mensajes_de_terminal_hacen_ida_y_vuelta() {
        ida_y_vuelta_cliente(MensajeCliente::CapturarHistorial {
            peticion_id: RANGO_TERMINAL,
            sesion_id: 3,
        });
        ida_y_vuelta_cliente(MensajeCliente::Foco {
            sesion_id: 3,
            enfocada: true,
        });
        ida_y_vuelta_cliente(MensajeCliente::IniciarEscrituraVarios {
            peticion_id: RANGO_TERMINAL + 1,
            sesiones: vec![3, 4],
            deliberacion_id: None,
        });
        ida_y_vuelta_cliente(MensajeCliente::IniciarEscrituraVarios {
            peticion_id: RANGO_TERMINAL + 2,
            sesiones: vec![3, 4],
            deliberacion_id: Some(12),
        });
        ida_y_vuelta_cliente(MensajeCliente::EscribirVarios {
            id: 7,
            pestanas: vec![
                BytesPestana {
                    sesion_id: 3,
                    bytes: b"\x1b[13;2u".to_vec(),
                },
                BytesPestana {
                    sesion_id: 4,
                    bytes: b"\r".to_vec(),
                },
            ],
        });
        ida_y_vuelta_cliente(MensajeCliente::TerminarEscrituraVarios {
            id: 7,
            motivo: "el usuario lo paró".to_string(),
        });
        ida_y_vuelta_cliente(MensajeCliente::AbrirTunelTemporal {
            peticion_id: RANGO_TERMINAL + 3,
            sesion_id: 3,
            destino: "localhost".to_string(),
            puerto: 5173,
        });
        ida_y_vuelta_cliente(MensajeCliente::CerrarTunelTemporal { id: 5 });
        ida_y_vuelta_cliente(MensajeCliente::ListarTmux {
            peticion_id: RANGO_TERMINAL + 4,
            host_id: 7,
        });
        ida_y_vuelta_cliente(MensajeCliente::TerminarTmux {
            peticion_id: RANGO_TERMINAL + 5,
            host_id: 7,
            nombre: "magi-portatil-1".to_string(),
        });
        ida_y_vuelta_cliente(MensajeCliente::PortapapelesPermitido { sesion_id: 3 });
        ida_y_vuelta_cliente(MensajeCliente::RenombrarSesion {
            sesion_id: 3,
            nombre: "web-01".to_string(),
        });
        ida_y_vuelta_cliente(MensajeCliente::MoverSesion {
            sesion_id: 3,
            posicion: 1,
        });
    }

    /// Los mensajes de respuesta del terminal (v9) hacen ida y vuelta por el
    /// lado del servidor, con tramos de colores, envolturas y los dos estados
    /// de tmux.
    #[test]
    fn los_mensajes_de_respuesta_del_terminal_hacen_ida_y_vuelta() {
        ida_y_vuelta_servidor(MensajeServidor::HistorialLote {
            peticion_id: RANGO_TERMINAL,
            sesion_id: 3,
            lote: 0,
            ultimo: false,
            total: 42,
            lineas: vec![
                LineaHistorial {
                    tramos: vec![
                        Tramo {
                            texto: "hola ".to_string(),
                            primer_plano: Color::Indice(2),
                            fondo: Color::Predeterminado,
                            negrita: false,
                            cursiva: false,
                            subrayado: false,
                            inverso: false,
                        },
                        Tramo {
                            texto: "mundo".to_string(),
                            primer_plano: Color::Rgb(0xd0, 0xd0, 0xd0),
                            fondo: Color::Predeterminado,
                            negrita: true,
                            cursiva: false,
                            subrayado: false,
                            inverso: false,
                        },
                    ],
                    envuelta: false,
                },
                LineaHistorial {
                    tramos: vec![Tramo {
                        texto: "ERROR".to_string(),
                        primer_plano: Color::Predeterminado,
                        fondo: Color::Indice(1),
                        negrita: false,
                        cursiva: true,
                        subrayado: true,
                        inverso: true,
                    }],
                    envuelta: true,
                },
            ],
        });
        ida_y_vuelta_servidor(MensajeServidor::HistorialLote {
            peticion_id: RANGO_TERMINAL,
            sesion_id: 3,
            lote: 1,
            ultimo: true,
            total: 1,
            lineas: Vec::new(),
        });
        ida_y_vuelta_servidor(MensajeServidor::EscrituraVariosIniciada {
            peticion_id: RANGO_TERMINAL + 1,
            id: 7,
        });
        ida_y_vuelta_servidor(MensajeServidor::EscrituraVariosTerminada {
            id: 7,
            motivo: "inactividad (10 min)".to_string(),
        });
        ida_y_vuelta_servidor(MensajeServidor::TunelTemporal {
            peticion_id: RANGO_TERMINAL + 3,
            id: 5,
            puerto_local: 41_234,
        });
        let tunel_temporal = InfoTunelTemporal {
            id: 5,
            sesion_id: 3,
            host_id: 7,
            host_nombre: "web-01".to_string(),
            puerto_local: 41_234,
            destino: "127.0.0.1".to_string(),
            puerto: 5173,
            conexiones: 2,
        };
        ida_y_vuelta_servidor(MensajeServidor::TunelesTemporales {
            lista: vec![tunel_temporal.clone()],
        });
        let sesiones_tmux = vec![
            InfoSesionTmux {
                nombre: "magi-portatil-1".to_string(),
                enganchada: true,
                ventanas: 3,
                creada: "Mon Nov 9 19:00:00 2026".to_string(),
            },
            InfoSesionTmux {
                nombre: "trabajo".to_string(),
                enganchada: false,
                ventanas: 1,
                creada: "Sun Nov 8 10:00:00 2026".to_string(),
            },
        ];
        ida_y_vuelta_servidor(MensajeServidor::SesionesTmux {
            peticion_id: RANGO_TERMINAL + 4,
            host_id: 7,
            sesiones: sesiones_tmux,
            sin_tmux: None,
        });
        ida_y_vuelta_servidor(MensajeServidor::SesionesTmux {
            peticion_id: RANGO_TERMINAL + 4,
            host_id: 7,
            sesiones: Vec::new(),
            sin_tmux: Some("no hay tmux en el host".to_string()),
        });
        ida_y_vuelta_servidor(MensajeServidor::TmuxTerminada {
            peticion_id: RANGO_TERMINAL + 5,
            resultado: ResultadoTmux::Ok,
        });
        ida_y_vuelta_servidor(MensajeServidor::TmuxTerminada {
            peticion_id: RANGO_TERMINAL + 6,
            resultado: ResultadoTmux::NoExiste,
        });
        ida_y_vuelta_servidor(MensajeServidor::Portapapeles {
            sesion_id: 3,
            texto: "copiado del remoto".to_string(),
        });
        ida_y_vuelta_servidor(MensajeServidor::PortapapelesPedido {
            sesion_id: 3,
            bytes: 4096,
        });
    }

    /// `AbrirSesion` con tmux y `Adjuntar` con los colores del tema (v9)
    /// hacen ida y vuelta.
    #[test]
    fn abrir_y_adjuntar_llevan_lo_nuevo_del_terminal() {
        ida_y_vuelta_cliente(MensajeCliente::AbrirSesion {
            host_id: 7,
            cols: 120,
            filas: 38,
            comandos_iniciales: Vec::new(),
            orden: None,
            tmux: Some("magi-portatil-1".to_string()),
        });
        ida_y_vuelta_cliente(MensajeCliente::Adjuntar {
            sesion_id: 3,
            cols: 80,
            filas: 24,
            colores: Some(ColoresTema {
                primer_plano: (0xd0, 0xd0, 0xd0),
                fondo: (0x10, 0x10, 0x10),
                cursor: (0xf0, 0xf0, 0xf0),
            }),
        });
    }

    /// Una sesión con reconexión, título, aviso y tmux hace ida y vuelta en
    /// `Sesiones` y en la `Bienvenida`, que además lleva los túneles
    /// temporales.
    #[test]
    fn la_sesion_lleva_lo_nuevo_del_terminal() {
        let sesion = InfoSesion {
            id: 1,
            nombre: "web-01".to_string(),
            host_id: 7,
            host_nombre: "web-01".to_string(),
            estado: EstadoSesionRemota::Caida,
            motivo: Some("conexión perdida".to_string()),
            identidad: "ed25519 · agente".to_string(),
            abierta_en: 1_700_000_000,
            ventanas: 2,
            actividad_no_vista: false,
            reconexion: Some(InfoReconexion {
                intento: 3,
                proximo_en: 1_700_000_008,
                desde: 1_700_000_000,
                causa: "conexión perdida".to_string(),
            }),
            titulo: Some("Claude Code".to_string()),
            aviso: Some(InfoAviso {
                texto: "compilación terminada".to_string(),
                hora: 1_700_000_005,
            }),
            tmux: Some("magi-portatil-1".to_string()),
            escritura_varios: true,
        };
        ida_y_vuelta_servidor(MensajeServidor::Sesiones {
            lista: vec![sesion.clone()],
        });
        ida_y_vuelta_servidor(MensajeServidor::Bienvenida {
            version: VERSION_PROTOCOLO,
            version_app: "1.4.0".to_string(),
            pid: 100,
            cliente_id: 3,
            clientes: 1,
            sesiones: vec![sesion],
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: vec![InfoTunelTemporal {
                id: 5,
                sesion_id: 1,
                host_id: 7,
                host_nombre: "web-01".to_string(),
                puerto_local: 41_234,
                destino: "localhost".to_string(),
                puerto: 5173,
                conexiones: 1,
            }],
        });
    }

    /// Los mensajes de una versión anterior a la v9 se leen con sus valores
    /// por defecto: los campos nuevos son opcionales en el cable.
    #[test]
    fn los_mensajes_de_una_version_anterior_se_leen_con_defectos() {
        // AbrirSesion sin tmux (v8).
        let linea = r#"{"tipo":"AbrirSesion","host_id":1,"cols":80,"filas":24}"#;
        match decodificar::<MensajeCliente>(linea).unwrap() {
            MensajeCliente::AbrirSesion { tmux, .. } => assert_eq!(tmux, None),
            otro => panic!("{otro:?}"),
        }
        // Adjuntar sin los colores del tema (v8).
        let linea = r#"{"tipo":"Adjuntar","sesion_id":1,"cols":80,"filas":24}"#;
        match decodificar::<MensajeCliente>(linea).unwrap() {
            MensajeCliente::Adjuntar { colores, .. } => assert_eq!(colores, None),
            otro => panic!("{otro:?}"),
        }
        // IniciarEscrituraVarios sin deliberación (siempre es opcional).
        let linea = r#"{"tipo":"IniciarEscrituraVarios","peticion_id":2,"sesiones":[1,2]}"#;
        match decodificar::<MensajeCliente>(linea).unwrap() {
            MensajeCliente::IniciarEscrituraVarios {
                deliberacion_id, ..
            } => assert_eq!(deliberacion_id, None),
            otro => panic!("{otro:?}"),
        }
        // Bienvenida sin túneles temporales (v8).
        let linea = r#"{"tipo":"Bienvenida","version":8,"version_app":"1.3.0","pid":1,"cliente_id":1,"clientes":1,"sesiones":[],"transferencias":[],"tuneles":[],"ejecuciones":[],"acciones_servicio":[]}"#;
        match decodificar::<MensajeServidor>(linea).unwrap() {
            MensajeServidor::Bienvenida {
                tuneles_temporales, ..
            } => assert!(tuneles_temporales.is_empty()),
            otro => panic!("{otro:?}"),
        }
        // SesionesTmux sin el motivo de que no haya tmux.
        let linea = r#"{"tipo":"SesionesTmux","peticion_id":2,"host_id":7,"sesiones":[]}"#;
        match decodificar::<MensajeServidor>(linea).unwrap() {
            MensajeServidor::SesionesTmux { sin_tmux, .. } => assert_eq!(sin_tmux, None),
            otro => panic!("{otro:?}"),
        }
        // InfoSesion sin reconexión, título, aviso ni tmux (v8).
        let linea = r#"{"tipo":"Sesiones","lista":[{"id":1,"nombre":"web-01","host_id":7,"host_nombre":"web-01","estado":"Abierta","motivo":null,"identidad":"ed25519 · agente","abierta_en":1700000000,"ventanas":1,"actividad_no_vista":false}]}"#;
        match decodificar::<MensajeServidor>(linea).unwrap() {
            MensajeServidor::Sesiones { lista } => {
                assert_eq!(lista.len(), 1);
                let sesion = &lista[0];
                assert!(sesion.reconexion.is_none());
                assert!(sesion.titulo.is_none());
                assert!(sesion.aviso.is_none());
                assert!(sesion.tmux.is_none());
            }
            otro => panic!("{otro:?}"),
        }
    }

    /// Los `peticion_id` del terminal viven desde 2^49 y el protocolo es el 9,
    /// por encima de los rangos de las fases anteriores.
    #[test]
    fn el_terminal_estrena_el_rango_y_la_version() {
        assert_eq!(RANGO_TERMINAL, 1 << 49);
        assert_eq!(VERSION_PROTOCOLO, 9);
        // Por debajo quedan los rangos de las fases anteriores.
        const {
            assert!(RANGO_TERMINAL > 1 << 48);
            assert!(RANGO_TERMINAL > RANGO_SERVICIOS);
            assert!(RANGO_TERMINAL > RANGO_ESCRITORIOS);
            assert!(RANGO_TERMINAL > RANGO_ESCRITORIOS_SERVIDOR);
        }
    }
}
