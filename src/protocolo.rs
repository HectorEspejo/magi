//! Protocolo entre la TUI (cliente) y el servidor de sesiones: JSON por
//! líneas sobre el socket Unix. Los bytes del terminal viajan en base64 y
//! los secretos en `Zeroizing`. Un mensaje desconocido o mal formado no se
//! interpreta: produce `Error` y desconexión de ese cliente.

use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
pub use tokio_util::codec::LinesCodec;
use zeroize::Zeroizing;

use crate::archivos::Entrada;

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
pub const VERSION_PROTOCOLO: u32 = 4;

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
}

// ---------------------------------------------------------------- archivos

/// Sentido de una transferencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direccion {
    Subida,
    Bajada,
}

impl Direccion {
    pub fn texto(self) -> &'static str {
        match self {
            Direccion::Subida => "subida",
            Direccion::Bajada => "bajada",
        }
    }

    /// Glifo de la cola: hacia el remoto o hacia el local.
    pub fn glifo(self, ascii: bool) -> &'static str {
        match (self, ascii) {
            (Direccion::Subida, false) => "↑",
            (Direccion::Bajada, false) => "↓",
            (Direccion::Subida, true) => "^",
            (Direccion::Bajada, true) => "v",
        }
    }
}

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
    Hecha,
    Error,
    Cancelada,
}

impl EstadoTransferencia {
    pub fn texto(self) -> &'static str {
        match self {
            EstadoTransferencia::EnCola => "en cola",
            EstadoTransferencia::EnCurso => "en curso",
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
            (EstadoTransferencia::Hecha, false) => "●",
            (EstadoTransferencia::Error, false) => "✕",
            (EstadoTransferencia::Cancelada, false) => "⊘",
            (EstadoTransferencia::EnCola, true) => "o",
            (EstadoTransferencia::EnCurso, true) => "o",
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
    },
    Adjuntar {
        sesion_id: u32,
        cols: u16,
        filas: u16,
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
    /// en una bajada la expande el servidor.
    Transferir {
        host_id: i64,
        direccion: Direccion,
        elementos: Vec<ElementoTransferencia>,
        /// Política por defecto de los elementos que no traigan la suya.
        politica: Politica,
        borrar_origen: bool,
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
    /// Copia un fichero remoto a un temporal local para verlo con `$PAGER`.
    DescargarTemporal {
        host_id: i64,
        ruta: String,
        peticion_id: u64,
    },
    /// Borra un temporal creado por `DescargarTemporal`.
    BorrarTemporal {
        ruta: String,
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
    Listar,
    /// Apaga el servidor cerrando las sesiones que queden.
    Parar,
    Adios,
}

// ---------------------------------------------------------------- servidor → cliente

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo")]
pub enum MensajeServidor {
    Bienvenida {
        version: u32,
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
    },
    VersionIncompatible {
        version: u32,
        /// Pid del servidor, para poder pararlo desde un MAGI de otra versión.
        /// Un servidor anterior a la v4 no lo envía.
        #[serde(default)]
        pid: Option<u32>,
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
    /// `CrearDirRemoto`).
    Hecho {
        peticion_id: u64,
    },
    RutaTemporal {
        ruta: String,
        peticion_id: u64,
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
        });
        ida_y_vuelta_cliente(MensajeCliente::Adjuntar {
            sesion_id: 3,
            cols: 80,
            filas: 24,
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
            }],
            politica: Politica::Sobrescribir,
            borrar_origen: false,
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
        });
        ida_y_vuelta_cliente(MensajeCliente::BorrarTemporal {
            ruta: "/run/magi/tmp/8-error.log".to_string(),
        });

        ida_y_vuelta_servidor(MensajeServidor::SftpAbierto {
            host_id: 7,
            peticion_id: Some(1 << 48),
            dir_inicio: "/home/hector".to_string(),
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
                    propietario: Some("hector:hector".to_string()),
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
            }],
        });
        ida_y_vuelta_servidor(MensajeServidor::Hecho { peticion_id: 5 });
        ida_y_vuelta_servidor(MensajeServidor::RutaTemporal {
            ruta: "/run/magi/tmp/8-error.log".to_string(),
            peticion_id: 8,
        });
        ida_y_vuelta_servidor(MensajeServidor::Error {
            mensaje: "la ruta no existe".to_string(),
            peticion_id: Some(4),
        });
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
    fn la_version_del_protocolo_es_la_cuatro() {
        assert_eq!(VERSION_PROTOCOLO, 4);
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
            destino: Some("10.0.0.5:5432".to_string()),
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
            pid: 100,
            cliente_id: 3,
            clientes: 2,
            sesiones: Vec::new(),
            transferencias: Vec::new(),
            tuneles: vec![tunel],
            ejecuciones: Vec::new(),
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

    /// Un `AbrirSesion` de un cliente sin comandos iniciales se lee igual.
    #[test]
    fn abrir_sesion_sin_comandos_iniciales_se_lee() {
        let linea = r#"{"tipo":"AbrirSesion","host_id":1,"cols":80,"filas":24}"#;
        match decodificar::<MensajeCliente>(linea).unwrap() {
            MensajeCliente::AbrirSesion {
                comandos_iniciales, ..
            } => assert!(comandos_iniciales.is_empty()),
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
        }];
        ida_y_vuelta_servidor(MensajeServidor::Bienvenida {
            version: 1,
            pid: 100,
            cliente_id: 3,
            clientes: 2,
            sesiones: sesiones.clone(),
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
        });
        ida_y_vuelta_servidor(MensajeServidor::VersionIncompatible {
            version: 99,
            pid: Some(4321),
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
            usuario: "hector".to_string(),
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
}
