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
pub const VERSION_PROTOCOLO: u32 = 2;

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
    Ejecutar {
        host_id: i64,
        comando: String,
    },
    /// Abre (o reutiliza) el canal SFTP del host. Responde `SftpAbierto` o
    /// `Error`; si no hay conexión viva, la abre con los diálogos de siempre.
    AbrirSftp {
        host_id: i64,
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
    },
    VersionIncompatible {
        version: u32,
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
    Ejecutado {
        host_id: i64,
        salida: String,
        codigo: i32,
    },
    SinSesion {
        host_id: i64,
    },
    /// Canal SFTP listo. `dir_inicio` es el directorio de inicio del usuario
    /// remoto (el que se usa si el host no tiene guardado el suyo).
    SftpAbierto {
        host_id: i64,
        dir_inicio: String,
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
        });
        ida_y_vuelta_cliente(MensajeCliente::Listar);
        ida_y_vuelta_cliente(MensajeCliente::Parar);
        ida_y_vuelta_cliente(MensajeCliente::Adios);
    }

    /// Los mensajes que estrena la Fase 4 (v2): archivos y transferencias.
    #[test]
    fn los_mensajes_de_archivos_hacen_ida_y_vuelta() {
        ida_y_vuelta_cliente(MensajeCliente::AbrirSftp { host_id: 7 });
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
    fn la_version_del_protocolo_es_la_dos() {
        assert_eq!(VERSION_PROTOCOLO, 2);
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
        });
        ida_y_vuelta_servidor(MensajeServidor::VersionIncompatible { version: 99 });
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
        ida_y_vuelta_servidor(MensajeServidor::Ejecutado {
            host_id: 7,
            salida: "load average".to_string(),
            codigo: 0,
        });
        ida_y_vuelta_servidor(MensajeServidor::SinSesion { host_id: 7 });
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
