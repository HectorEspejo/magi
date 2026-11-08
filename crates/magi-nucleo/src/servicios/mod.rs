//! Servicios, contenedores y logs (Fase 13, §2 del informe).
//!
//! Aquí vive la lógica común de la vista Servicios: los tipos que viajan por
//! el protocolo, la lectura de las salidas de `servicios.sh`, las acciones
//! permitidas, los nombres válidos, los validadores de configuración y los
//! límites de la entrada no fiable. No depende de la TUI ni del servidor: la
//! usa el escritorio y, en la Fase 14, la app Android.
//!
//! Nada de este módulo ejecuta órdenes ni abre conexiones: solo compone
//! argumentos (`orden`), lee texto del host (`lectura`) y decide qué se puede
//! hacer (`acciones`).

pub mod acciones;
pub mod lectura;
pub mod limites;
pub mod orden;
pub mod validadores;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Tipo de objetivo sobre el que actúa la vista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoObjetivo {
    /// Unidad de systemd (servicio o timer).
    Unidad,
    /// Timer de systemd (se trata como unidad para las acciones y como timer
    /// en la vista).
    Timer,
    /// Contenedor de Docker o Podman.
    Contenedor,
    /// Proyecto Compose.
    Proyecto,
}

impl TipoObjetivo {
    pub fn texto(self) -> &'static str {
        match self {
            TipoObjetivo::Unidad => "unidad",
            TipoObjetivo::Timer => "timer",
            TipoObjetivo::Contenedor => "contenedor",
            TipoObjetivo::Proyecto => "proyecto",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "unidad" => Some(TipoObjetivo::Unidad),
            "timer" => Some(TipoObjetivo::Timer),
            "contenedor" => Some(TipoObjetivo::Contenedor),
            "proyecto" => Some(TipoObjetivo::Proyecto),
            _ => None,
        }
    }

    /// ¿Se maneja en el host con `systemctl` (unidad o timer)?
    pub fn es_systemd(self) -> bool {
        matches!(self, TipoObjetivo::Unidad | TipoObjetivo::Timer)
    }
}

/// Objetivo concreto: tipo y nombre (unidad, contenedor o proyecto).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Objetivo {
    pub tipo: TipoObjetivo,
    pub nombre: String,
}

impl Objetivo {
    pub fn unidad(nombre: impl Into<String>) -> Self {
        Self {
            tipo: TipoObjetivo::Unidad,
            nombre: nombre.into(),
        }
    }

    pub fn contenedor(nombre: impl Into<String>) -> Self {
        Self {
            tipo: TipoObjetivo::Contenedor,
            nombre: nombre.into(),
        }
    }

    pub fn proyecto(nombre: impl Into<String>) -> Self {
        Self {
            tipo: TipoObjetivo::Proyecto,
            nombre: nombre.into(),
        }
    }
}

/// Acción de la vista sobre un servicio, un timer, un contenedor o un proyecto
/// (§7.2). Los nombres viajan como estos valores cerrados; nunca como texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accion {
    Arrancar,
    Parar,
    Reiniciar,
    Recargar,
    /// Activar el arranque al inicio (`systemctl enable`).
    Activar,
    /// Desactivar el arranque al inicio (`systemctl disable`).
    Desactivar,
    /// Lanzar ahora el servicio de un timer (`systemctl start <servicio>`).
    Lanzar,
}

impl Accion {
    pub fn texto(self) -> &'static str {
        match self {
            Accion::Arrancar => "arrancar",
            Accion::Parar => "parar",
            Accion::Reiniciar => "reiniciar",
            Accion::Recargar => "recargar",
            Accion::Activar => "activar",
            Accion::Desactivar => "desactivar",
            Accion::Lanzar => "lanzar",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "arrancar" => Some(Accion::Arrancar),
            "parar" => Some(Accion::Parar),
            "reiniciar" => Some(Accion::Reiniciar),
            "recargar" => Some(Accion::Recargar),
            "activar" => Some(Accion::Activar),
            "desactivar" => Some(Accion::Desactivar),
            "lanzar" => Some(Accion::Lanzar),
            _ => None,
        }
    }

    /// Texto en pasado para el registro y el resultado («reiniciado»).
    pub fn participio(self) -> &'static str {
        match self {
            Accion::Arrancar => "arrancado",
            Accion::Parar => "parado",
            Accion::Reiniciar => "reiniciado",
            Accion::Recargar => "recargado",
            Accion::Activar => "activado",
            Accion::Desactivar => "desactivado",
            Accion::Lanzar => "lanzado",
        }
    }

    /// ¿La acción cambia algo en el host? (Las lecturas no pasan por aquí.)
    pub fn es_privilegiada(self) -> bool {
        true
    }
}

/// Privilegio del usuario SSH en el host (§7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privilegio {
    Root,
    SudoSinClave,
    SudoConClave,
    SinSudo,
}

impl Privilegio {
    pub fn texto(self) -> &'static str {
        match self {
            Privilegio::Root => "root",
            Privilegio::SudoSinClave => "sudo_sin_clave",
            Privilegio::SudoConClave => "sudo_con_clave",
            Privilegio::SinSudo => "sin_sudo",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "root" => Some(Privilegio::Root),
            "sudo_sin_clave" => Some(Privilegio::SudoSinClave),
            "sudo_con_clave" => Some(Privilegio::SudoConClave),
            "sin_sudo" => Some(Privilegio::SinSudo),
            _ => None,
        }
    }

    /// ¿Usa `sudo` (con o sin contraseña)?
    pub fn usa_sudo(self) -> bool {
        matches!(self, Privilegio::SudoSinClave | Privilegio::SudoConClave)
    }

    /// ¿Hay que pedir la contraseña para una orden privilegiada?
    pub fn pide_contrasena(self) -> bool {
        self == Privilegio::SudoConClave
    }

    /// ¿Puede ejecutar órdenes privilegiadas?
    pub fn puede(self) -> bool {
        self != Privilegio::SinSudo
    }
}

/// ¿Se puede leer el diario del sistema entero o solo lo del usuario?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermisoDiario {
    /// El usuario ve el diario del sistema (grupo `systemd-journal`, `adm` o
    /// `wheel`, o root).
    Todo,
    /// Solo el diario propio del usuario.
    Propio,
}

impl PermisoDiario {
    pub fn texto(self) -> &'static str {
        match self {
            PermisoDiario::Todo => "todo",
            PermisoDiario::Propio => "propio",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "todo" => Some(PermisoDiario::Todo),
            "propio" => Some(PermisoDiario::Propio),
            _ => None,
        }
    }
}

/// Estado de systemd en el host (§5.1, §7.12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Systemd {
    /// Versión de systemd (p. ej. «252»).
    Disponible(String),
    /// El host no tiene systemd.
    Ausente,
    /// systemd anterior a 230 (se ve la versión).
    Antiguo(String),
}

impl Systemd {
    /// ¿Se puede usar systemd en este host?
    pub fn utilizable(&self) -> bool {
        matches!(self, Systemd::Disponible(_))
    }

    pub fn version(&self) -> Option<&str> {
        match self {
            Systemd::Disponible(version) | Systemd::Antiguo(version) => Some(version),
            Systemd::Ausente => None,
        }
    }
}

/// Motor de contenedores detectado en el host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorNombre {
    Docker,
    Podman,
}

impl MotorNombre {
    pub fn texto(self) -> &'static str {
        match self {
            MotorNombre::Docker => "docker",
            MotorNombre::Podman => "podman",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "docker" => Some(MotorNombre::Docker),
            "podman" => Some(MotorNombre::Podman),
            _ => None,
        }
    }

    /// ¿Con este motor se usa sudo? Con Podman nunca (§7.3).
    pub fn usa_sudo(self) -> bool {
        self == MotorNombre::Docker
    }

    /// Suborden de Compose del motor.
    pub fn compose(self) -> &'static str {
        match self {
            MotorNombre::Docker => "docker compose",
            MotorNombre::Podman => "podman compose",
        }
    }
}

/// Motor con su versión.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Motor {
    pub nombre: MotorNombre,
    pub version: String,
}

/// Estado de los contenedores en el host (§5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorEstado {
    /// No hay Docker ni Podman instalados.
    No,
    /// Hay motor pero el usuario no tiene permiso (solo Docker: con Podman
    /// sin root se ven los del usuario).
    SinPermiso,
    /// Hay motor y se puede usar.
    Disponible(Motor),
}

impl MotorEstado {
    pub fn disponible(&self) -> Option<&Motor> {
        match self {
            MotorEstado::Disponible(motor) => Some(motor),
            _ => None,
        }
    }
}

/// Reloj del host: época en segundos y uptime, para calcular «desde» y «hace»
/// con el reloj del host y no con el local (§7.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelojHost {
    /// Época en segundos según el host.
    pub epoch: i64,
    /// Uptime en segundos según el host.
    pub uptime_seg: u64,
}

impl RelojHost {
    /// Instante (época del host) de un sello monotónico de systemd (en
    /// microsegundos) o `None` si el sello no es válido o es cero.
    pub fn instante_desde_monotonico(&self, monotonic_us: u64) -> Option<i64> {
        if monotonic_us == 0 {
            return None;
        }
        let uptime_us = self.uptime_seg.saturating_mul(1_000_000);
        let hace_us = uptime_us.saturating_sub(monotonic_us);
        Some(self.epoch - (hace_us / 1_000_000) as i64)
    }
}

/// Estado de arranque de una unidad (`UnitFileState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoArranque {
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

impl EstadoArranque {
    pub fn desde_texto(texto: &str) -> Self {
        match texto {
            "enabled" | "enabled-runtime" => EstadoArranque::Activado,
            "disabled" => EstadoArranque::Desactivado,
            "static" => EstadoArranque::Estatico,
            "indirect" => EstadoArranque::Indirecto,
            "generated" => EstadoArranque::Generado,
            "transient" => EstadoArranque::Transitorio,
            "alias" => EstadoArranque::Alias,
            "masked" | "masked-runtime" => EstadoArranque::Enmascarado,
            _ => EstadoArranque::Desconocido,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            EstadoArranque::Activado => "activado",
            EstadoArranque::Desactivado => "desactivado",
            EstadoArranque::Estatico => "estático",
            EstadoArranque::Indirecto => "indirecto",
            EstadoArranque::Generado => "generado",
            EstadoArranque::Transitorio => "transitorio",
            EstadoArranque::Alias => "alias",
            EstadoArranque::Enmascarado => "enmascarado",
            EstadoArranque::Desconocido => "desconocido",
        }
    }

    /// ¿Se puede activar o desactivar el arranque?
    pub fn conmutable(self) -> bool {
        matches!(self, EstadoArranque::Activado | EstadoArranque::Desactivado)
    }
}

/// Unidad de systemd tal como llega del host: sus propiedades de
/// `systemctl show` (clave=valor) y si está vigilada en la ficha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unidad {
    pub nombre: String,
    #[serde(default)]
    pub props: BTreeMap<String, String>,
    /// La unidad está en `HOSTS.servicios` (se pinta con ★ y arriba).
    #[serde(default)]
    pub vigilada: bool,
}

impl Unidad {
    pub fn prop(&self, clave: &str) -> &str {
        self.props.get(clave).map(String::as_str).unwrap_or("")
    }

    /// `ActiveState` (`active`, `inactive`, `failed`, `activating`,
    /// `deactivating`, `reloading`).
    pub fn activo(&self) -> &str {
        self.prop("ActiveState")
    }

    pub fn subestado(&self) -> &str {
        self.prop("SubState")
    }

    pub fn descripcion(&self) -> &str {
        self.prop("Description")
    }

    /// `Type` (simple, oneshot, forking...).
    pub fn tipo(&self) -> &str {
        self.prop("Type")
    }

    /// `Result` de la última parada (success, exit-code, signal...).
    pub fn resultado(&self) -> &str {
        self.prop("Result")
    }

    pub fn arranque(&self) -> EstadoArranque {
        EstadoArranque::desde_texto(self.prop("UnitFileState"))
    }

    pub fn puede_recargar(&self) -> bool {
        self.prop("CanReload") == "yes"
    }

    /// Cargada o solo instalada: si no está cargada se pinta inactiva.
    pub fn cargada(&self) -> bool {
        self.prop("LoadState") != "not-found" && self.prop("LoadState") != "masked"
    }

    /// ¿Está enmascarada (`masked` en LoadState o UnitFileState)?
    pub fn enmascarada(&self) -> bool {
        self.prop("LoadState") == "masked" || self.arranque() == EstadoArranque::Enmascarado
    }

    /// ¿Es una plantilla (`app@.service`)? Las plantillas no se listan.
    pub fn plantilla(&self) -> bool {
        self.nombre
            .split('.')
            .next()
            .is_some_and(|antes| antes.ends_with('@'))
    }

    /// PID principal, si está corriendo.
    pub fn pid_principal(&self) -> Option<u32> {
        self.prop("ExecMainPID")
            .parse()
            .ok()
            .filter(|pid: &u32| *pid > 0)
    }

    /// Memoria actual en bytes (`MemoryCurrent`; «infinity» o vacío no valen).
    pub fn memoria_bytes(&self) -> Option<u64> {
        self.prop("MemoryCurrent")
            .parse()
            .ok()
            .filter(|bytes: &u64| *bytes != u64::MAX)
    }

    /// Instante (época del host) en el que entró en el estado actual.
    pub fn desde_epoch(&self, reloj: RelojHost) -> Option<i64> {
        let monotonic = self.prop("ActiveEnterTimestampMonotonic").parse().ok()?;
        reloj.instante_desde_monotonico(monotonic)
    }

    /// Fichero de la unidad.
    pub fn fichero(&self) -> Option<&str> {
        let ruta = self.prop("FragmentPath");
        (!ruta.is_empty()).then_some(ruta)
    }
}

/// Timer de systemd: sus propiedades y el nombre de su servicio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    pub nombre: String,
    #[serde(default)]
    pub props: BTreeMap<String, String>,
    /// Servicio que activa (`Unit` de `systemctl show`), si se conoce.
    #[serde(default)]
    pub servicio: Option<String>,
}

impl Timer {
    pub fn prop(&self, clave: &str) -> &str {
        self.props.get(clave).map(String::as_str).unwrap_or("")
    }

    pub fn descripcion(&self) -> &str {
        self.prop("Description")
    }

    pub fn activo(&self) -> &str {
        self.prop("ActiveState")
    }

    /// Próxima ejecución (época del host), de `NextElapseUSecRealtime`.
    pub fn proxima_epoch(&self) -> Option<i64> {
        let texto = self.prop("NextElapseUSecRealtime");
        let us: u64 = texto.trim_end_matches("us").parse().ok()?;
        (us > 0).then_some((us / 1_000_000) as i64)
    }

    /// Última ejecución (época del host), a partir del monotónico
    /// `LastTriggerUSecMonotonic`.
    pub fn ultima_epoch(&self, reloj: RelojHost) -> Option<i64> {
        let texto = self.prop("LastTriggerUSecMonotonic");
        let us: u64 = texto.trim_end_matches("us").parse().ok()?;
        reloj.instante_desde_monotonico(us)
    }

    /// El servicio que activa, del nombre de su pareja (`app.timer` →
    /// `app.service`) si no viene en `Unit`.
    pub fn servicio_activado(&self) -> Option<String> {
        if let Some(servicio) = &self.servicio {
            return Some(servicio.clone());
        }
        let base = self.nombre.strip_suffix(".timer")?;
        Some(format!("{base}.service"))
    }
}

/// Estado de un contenedor (§3.8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoContenedor {
    #[default]
    Creado,
    EnMarcha,
    Parado,
    Reiniciando,
    Pausado,
    Muerto,
    Desconocido,
}

impl EstadoContenedor {
    pub fn desde_texto(texto: &str) -> Self {
        match texto {
            "created" => EstadoContenedor::Creado,
            "running" => EstadoContenedor::EnMarcha,
            "exited" => EstadoContenedor::Parado,
            "restarting" => EstadoContenedor::Reiniciando,
            "paused" => EstadoContenedor::Pausado,
            "dead" => EstadoContenedor::Muerto,
            _ => EstadoContenedor::Desconocido,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            EstadoContenedor::Creado => "creado",
            EstadoContenedor::EnMarcha => "en marcha",
            EstadoContenedor::Parado => "parado",
            EstadoContenedor::Reiniciando => "reiniciándose",
            EstadoContenedor::Pausado => "en pausa",
            EstadoContenedor::Muerto => "muerto",
            EstadoContenedor::Desconocido => "desconocido",
        }
    }

    /// ¿Está en marcha (o reiniciándose)?
    pub fn en_marcha(self) -> bool {
        matches!(
            self,
            EstadoContenedor::EnMarcha | EstadoContenedor::Reiniciando
        )
    }
}

/// Salud de un contenedor con `healthcheck`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Salud {
    #[default]
    Ninguna,
    Sana,
    Insegura,
    Comprobando,
}

impl Salud {
    pub fn desde_texto(texto: &str) -> Self {
        match texto {
            "healthy" => Salud::Sana,
            "unhealthy" => Salud::Insegura,
            "starting" => Salud::Comprobando,
            _ => Salud::Ninguna,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            Salud::Ninguna => "",
            Salud::Sana => "sana",
            Salud::Insegura => "unhealthy",
            Salud::Comprobando => "comprobando",
        }
    }
}

/// Un contenedor de Docker o Podman (`ps -a`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contenedor {
    pub id: String,
    pub nombre: String,
    pub imagen: String,
    pub estado: EstadoContenedor,
    #[serde(default)]
    pub codigo_salida: Option<i32>,
    #[serde(default)]
    pub salud: Salud,
    #[serde(default)]
    pub puertos: String,
    /// Proyecto Compose (`com.docker.compose.project`), si lo tiene.
    #[serde(default)]
    pub proyecto: Option<String>,
    /// Ficheros Compose del proyecto (`…project.config_files`), si los hay.
    #[serde(default)]
    pub ficheros: Vec<String>,
    /// Servicio Compose (`com.docker.compose.service`), si lo tiene.
    #[serde(default)]
    pub servicio: Option<String>,
    /// Orden dentro del proyecto (`com.docker.compose.container-number`).
    #[serde(default)]
    pub orden: Option<String>,
    /// Etiquetas del contenedor.
    #[serde(default)]
    pub etiquetas: BTreeMap<String, String>,
}

impl Contenedor {
    /// ¿Está caído (§4.6): salió con código distinto de 0, se reinicia una y
    /// otra vez o está `unhealthy`? Un contenedor parado a propósito con
    /// código 0 no cuenta.
    pub fn caido(&self) -> bool {
        match self.estado {
            EstadoContenedor::Reiniciando | EstadoContenedor::Muerto => true,
            EstadoContenedor::EnMarcha => self.salud == Salud::Insegura,
            EstadoContenedor::Parado => self.codigo_salida.is_some_and(|codigo| codigo != 0),
            _ => false,
        }
    }

    /// ¿Se puede abrir una shell? Solo si corre.
    pub fn admite_shell(&self) -> bool {
        self.estado == EstadoContenedor::EnMarcha
    }
}

/// Proyecto Compose: los contenedores que comparten etiqueta de proyecto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proyecto {
    pub nombre: String,
    #[serde(default)]
    pub ficheros: Vec<String>,
}

impl Proyecto {
    /// ¿Se conocen los ficheros de Compose del proyecto? Sin ellos no se
    /// ofrecen las acciones del proyecto.
    pub fn con_ficheros(&self) -> bool {
        !self.ficheros.is_empty()
    }
}

/// Línea de log del diario o de un contenedor (§6.3, §7.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineaLog {
    /// Hora en la época del host (segundos).
    pub hora: Option<i64>,
    /// Prioridad de journald (0 emerg … 7 debug). En contenedores, 6 o 3.
    pub prioridad: u8,
    /// `true` si llegó por la salida de errores (contenedores). En el diario
    /// se deduce de la prioridad.
    pub stderr: bool,
    pub texto: String,
}

impl LineaLog {
    /// ¿Es aviso o peor (prioridad 0-4)?
    pub fn es_aviso(&self) -> bool {
        self.prioridad <= 4
    }

    /// ¿Es error o peor (prioridad 0-3)?
    pub fn es_error(&self) -> bool {
        self.prioridad <= 3
    }
}

/// Filtro por nivel del panel de logs (§6.3, §7.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NivelLog {
    Todo,
    Info,
    Aviso,
    Error,
}

impl NivelLog {
    /// Siguiente nivel al pulsar `n`.
    pub fn siguiente(self) -> Self {
        match self {
            NivelLog::Todo => NivelLog::Info,
            NivelLog::Info => NivelLog::Aviso,
            NivelLog::Aviso => NivelLog::Error,
            NivelLog::Error => NivelLog::Todo,
        }
    }

    /// Prioridad máxima visible (0 = emerg; 7 = todo).
    pub fn prioridad_maxima(self) -> u8 {
        match self {
            NivelLog::Todo => 7,
            NivelLog::Info => 6,
            NivelLog::Aviso => 4,
            NivelLog::Error => 3,
        }
    }

    pub fn texto(self) -> &'static str {
        match self {
            NivelLog::Todo => "todo",
            NivelLog::Info => "info",
            NivelLog::Aviso => "aviso",
            NivelLog::Error => "error",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "todo" => Some(NivelLog::Todo),
            "info" => Some(NivelLog::Info),
            "aviso" => Some(NivelLog::Aviso),
            "error" => Some(NivelLog::Error),
            _ => None,
        }
    }
}

/// Motivo del fin de una suscripción de logs (§5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoFinLog {
    /// El panel se cerró y MAGI mandó `PararLog`.
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

impl MotivoFinLog {
    pub fn texto(self) -> &'static str {
        match self {
            MotivoFinLog::Cerrado => "cerrado",
            MotivoFinLog::Terminado => "el proceso remoto terminó",
            MotivoFinLog::SinPermiso => "sin permiso para el diario",
            MotivoFinLog::ConexionCaida => "se cortó la conexión",
            MotivoFinLog::NoEncontrado => "el objetivo ya no existe",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "cerrado" => Some(MotivoFinLog::Cerrado),
            "terminado" => Some(MotivoFinLog::Terminado),
            "sin_permiso" => Some(MotivoFinLog::SinPermiso),
            "conexion_caida" => Some(MotivoFinLog::ConexionCaida),
            "no_encontrado" => Some(MotivoFinLog::NoEncontrado),
            _ => None,
        }
    }
}

/// Resultado de una acción (§7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultadoAccion {
    Hecho,
    Fallido,
    /// Se acabó el plazo: la orden sigue en el host.
    SinConfirmar,
    /// La comprobación de configuración falló y no se ejecutó nada.
    RechazadaConfiguracion,
    /// Error del servidor o de la conexión.
    Error,
}

impl ResultadoAccion {
    pub fn texto(self) -> &'static str {
        match self {
            ResultadoAccion::Hecho => "hecho",
            ResultadoAccion::Fallido => "fallido",
            ResultadoAccion::SinConfirmar => "sin_confirmar",
            ResultadoAccion::RechazadaConfiguracion => "rechazada_configuracion",
            ResultadoAccion::Error => "error",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "hecho" => Some(ResultadoAccion::Hecho),
            "fallido" => Some(ResultadoAccion::Fallido),
            "sin_confirmar" => Some(ResultadoAccion::SinConfirmar),
            "rechazada_configuracion" => Some(ResultadoAccion::RechazadaConfiguracion),
            "error" => Some(ResultadoAccion::Error),
            _ => None,
        }
    }

    /// Resultado que se anota en REGISTRO: `ok` solo con `hecho`.
    pub fn es_ok(self) -> bool {
        self == ResultadoAccion::Hecho
    }
}

/// Lo que devuelve una acción: resultado, estado final y lo que hay que
/// enseñar (§5.1, §7.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultadoServicio {
    pub resultado: ResultadoAccion,
    /// Estado final del objetivo (`active`, `inactive`, `failed`, `running`,
    /// `exited`...). Vacío si no se llegó a mirar.
    #[serde(default)]
    pub estado: String,
    /// Segundos que tardó la acción en el host.
    #[serde(default)]
    pub segundos: f32,
    /// Mensaje de la orden (nunca el diario entero).
    #[serde(default)]
    pub mensaje: String,
    /// Últimas 20 líneas del diario o la salida de la comprobación (16 KiB).
    #[serde(default)]
    pub lineas: Vec<String>,
    /// La comprobación de configuración que se aplicó, si hubo.
    #[serde(default)]
    pub comprobacion: Option<String>,
}

impl Default for ResultadoServicio {
    fn default() -> Self {
        Self {
            resultado: ResultadoAccion::Error,
            estado: String::new(),
            segundos: 0.0,
            mensaje: String::new(),
            lineas: Vec::new(),
            comprobacion: None,
        }
    }
}

/// Fase de una acción en curso (§3.3, §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaseAccion {
    Comprobando,
    Ejecutando,
    Esperando,
}

impl FaseAccion {
    pub fn texto(self) -> &'static str {
        match self {
            FaseAccion::Comprobando => "comprobando",
            FaseAccion::Ejecutando => "ejecutando",
            FaseAccion::Esperando => "esperando",
        }
    }
}

/// Una acción en curso tal como la difunde el servidor (§3.3, T27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoAccion {
    /// El `peticion_id` de la `AccionServicio` que la pidió.
    pub id: u64,
    pub host_id: i64,
    pub host_nombre: String,
    pub objetivo: Objetivo,
    pub accion: Accion,
    pub fase: FaseAccion,
    /// Época local en milisegundos en la que empezó.
    pub desde_ms: i64,
    /// Cliente que la pidió (para que lo reconozca en la difusión).
    pub solicitante: u32,
}

/// Datos de un contenedor para el detalle (§6.1): lo que se saca de
/// `inspect`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DetalleContenedor {
    pub id: String,
    pub nombre: String,
    pub imagen: String,
    pub creado: String,
    pub estado: EstadoContenedor,
    pub codigo_salida: Option<i32>,
    pub reinicios: u64,
    pub salud: Salud,
    pub puertos: Vec<String>,
    pub montajes: Vec<String>,
    pub redes: Vec<String>,
    pub proyecto: Option<String>,
    pub orden: Option<String>,
    /// Texto de `inspect` (hasta 256 KiB).
    pub texto: String,
}

/// Detalle de un objetivo (§6.1): propiedades, dependencias y el texto de
/// `systemctl cat` o de `inspect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detalle {
    pub objetivo: Objetivo,
    #[serde(default)]
    pub props: BTreeMap<String, String>,
    /// Dependencias que necesita («necesita»).
    #[serde(default)]
    pub necesita: Vec<String>,
    /// Dependencias que la necesitan («la necesitan»).
    #[serde(default)]
    pub necesitada: Vec<String>,
    /// Texto de la unidad (`systemctl cat`) o `inspect` del contenedor.
    #[serde(default)]
    pub texto: String,
    /// Para un contenedor, sus datos ya leídos del `inspect`.
    #[serde(default)]
    pub contenedor: Option<DetalleContenedor>,
}

/// Consumo de un contenedor en marcha (`stats --no-stream`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consumo {
    pub id: String,
    pub cpu_pct: Option<f64>,
    pub memoria_bytes: Option<u64>,
    pub memoria_limite_bytes: Option<u64>,
}

/// Listado completo de un host (§5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listado {
    pub systemd: Systemd,
    pub privilegio: Privilegio,
    pub diario: PermisoDiario,
    pub reloj: RelojHost,
    pub motor: MotorEstado,
    #[serde(default)]
    pub unidades: Vec<Unidad>,
    #[serde(default)]
    pub timers: Vec<Timer>,
    #[serde(default)]
    pub contenedores: Vec<Contenedor>,
    #[serde(default)]
    pub proyectos: Vec<Proyecto>,
    /// Avisos del host («sin permiso para Docker: …»).
    #[serde(default)]
    pub avisos: Vec<String>,
    /// Se ha recortado por los límites de entrada no fiable (§8).
    #[serde(default)]
    pub incompleto: bool,
}

impl Listado {
    /// Un listado vacío de un host sin systemd ni motor, para las pruebas y
    /// para el estado inicial.
    pub fn vacio(privilegio: Privilegio) -> Self {
        Self {
            systemd: Systemd::Ausente,
            privilegio,
            diario: PermisoDiario::Propio,
            reloj: RelojHost {
                epoch: 0,
                uptime_seg: 0,
            },
            motor: MotorEstado::No,
            unidades: Vec::new(),
            timers: Vec::new(),
            contenedores: Vec::new(),
            proyectos: Vec::new(),
            avisos: Vec::new(),
            incompleto: false,
        }
    }
}

/// Orden con la que se abre una pestaña desde la vista Servicios (D248).
/// El servidor la convierte en la orden real; el cliente nunca manda texto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum OrdenPestana {
    /// Shell dentro de un contenedor (`S`).
    ShellContenedor { motor: MotorNombre, id: String },
    /// Seguimiento del diario de una unidad (`t`).
    SeguirDiario { unidad: String },
    /// Seguimiento de los logs de un contenedor (`t`).
    SeguirContenedor { motor: MotorNombre, id: String },
}

/// Un fallo del sondeo ampliado (§7.8): una unidad fallida o un contenedor
/// caído.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fallo {
    pub tipo: TipoFallo,
    /// Nombre de la unidad o del contenedor.
    pub nombre: String,
    /// Estado con el que se vio (texto corto del host).
    #[serde(default)]
    pub estado: String,
    /// Para un contenedor, el motor que lo vio.
    #[serde(default)]
    pub motor: Option<MotorNombre>,
}

/// Tipo de fallo del sondeo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoFallo {
    Servicio,
    Contenedor,
}

impl TipoFallo {
    pub fn texto(self) -> &'static str {
        match self {
            TipoFallo::Servicio => "servicio",
            TipoFallo::Contenedor => "contenedor",
        }
    }
}

/// Fallos del último sondeo de un host (`SONDEOS.fallos_json`, §3.2).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FallosSondeo {
    #[serde(default)]
    pub unidades: FallosUnidades,
    /// `None` = no hay motor de contenedores o el usuario no tiene permiso
    /// (el sondeo nunca usa sudo).
    #[serde(default)]
    pub contenedores: Option<FallosContenedores>,
}

/// Unidades fallidas del sondeo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FallosUnidades {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub nombres: Vec<String>,
}

/// Contenedores caídos del sondeo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FallosContenedores {
    #[serde(default)]
    pub motor: Option<MotorNombre>,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub nombres: Vec<String>,
}

impl FallosSondeo {
    /// Todos los fallos como lista plana, para «Toda la flota» y Flota.
    pub fn lista(&self) -> Vec<Fallo> {
        let mut fallos: Vec<Fallo> = self
            .unidades
            .nombres
            .iter()
            .map(|nombre| Fallo {
                tipo: TipoFallo::Servicio,
                nombre: nombre.clone(),
                estado: "fallido".to_string(),
                motor: None,
            })
            .collect();
        if let Some(contenedores) = &self.contenedores {
            fallos.extend(contenedores.nombres.iter().map(|nombre| Fallo {
                tipo: TipoFallo::Contenedor,
                nombre: nombre.clone(),
                estado: "caído".to_string(),
                motor: contenedores.motor,
            }));
        }
        fallos
    }

    pub fn total(&self) -> u32 {
        self.unidades.total
            + self
                .contenedores
                .as_ref()
                .map(|contenedores| contenedores.total)
                .unwrap_or(0)
    }

    /// ¿No hay ningún fallo?
    pub fn vacio(&self) -> bool {
        self.total() == 0
    }
}
