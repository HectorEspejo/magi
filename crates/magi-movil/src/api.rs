//! Contrato de la API que la app Android consume por UniFFI (informe §5.1).
//!
//! Los DTO de este módulo son propios de la puerta móvil: no se reexportan
//! tipos de magi-nucleo (el núcleo no depende de UniFFI). Las funciones de red
//! son async y en Kotlin se convierten en funciones suspend.
//!
//! En esta entrega el crate es el contrato compilable: solo version() está
//! implementada; el resto son stubs con unimplemented! y TODO(F11) que el Lead
//! sustituye al integrar la lógica.

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

/// Fila de host de la pantalla Hosts.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HostInventario {
    pub uuid: String,
    pub nombre: String,
    pub direccion: String,
    pub puerto: u32,
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
    pub conexiones: u32,
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
#[derive(Debug, Clone, uniffi::Record)]
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
            formato: magi_nucleo::sincronizacion::formato::FORMATO,
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

    /// Olvida la contraseña guardada de un host; devuelve si había una.
    pub fn olvidar_contrasena(&self, host_uuid: String) -> Result<bool, ErrorNucleo> {
        self.estado.llavero.olvidar_contrasena(&host_uuid)
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
}
