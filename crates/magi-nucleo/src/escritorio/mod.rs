//! Escritorio remoto (Fase 12): lo común a RDP, VNC y voxl.
//!
//! Aquí viven los tipos del inventario (TipoHost, OpcionesEscritorio), los
//! eventos que producen los decodificadores (EventoEscritorio), las órdenes que
//! llegan del visor (OrdenEscritorio) y los diálogos que el servidor pide al
//! cliente solicitante (DialogosEscritorio).
//!
//! La decodificación de cada sesión corre en un hilo propio (ver
//! lanzar_en_hilo): el runtime de red del servidor solo mueve bytes.

pub mod puente;
pub mod rdp;
pub mod rfb;
pub mod teclas;
pub mod visor_http;
pub mod voxl;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

// ---------------------------------------------------------------- tipos de host

/// Tipo de host del inventario (HOSTS.tipo). Se elige al crear el host y no se
/// puede cambiar después: para otro tipo se crea otro host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TipoHost {
    Ssh,
    Rdp,
    Vnc,
    Voxl,
}

impl TipoHost {
    pub const TODOS: [TipoHost; 4] = [TipoHost::Ssh, TipoHost::Rdp, TipoHost::Vnc, TipoHost::Voxl];

    /// Valor que se guarda en HOSTS.tipo (se valida en código, sin CHECK).
    pub fn como_texto(self) -> &'static str {
        match self {
            TipoHost::Ssh => "ssh",
            TipoHost::Rdp => "rdp",
            TipoHost::Vnc => "vnc",
            TipoHost::Voxl => "voxl",
        }
    }

    /// Lee el tipo de la base de datos o de la sincronización. None si no se
    /// entiende: una fila con un tipo desconocido se rechaza al leerla (T38).
    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "ssh" => Some(TipoHost::Ssh),
            "rdp" => Some(TipoHost::Rdp),
            "vnc" => Some(TipoHost::Vnc),
            "voxl" => Some(TipoHost::Voxl),
            _ => None,
        }
    }

    /// Etiqueta que enseña la interfaz (el tipo se ve siempre, T5).
    pub fn etiqueta(self) -> &'static str {
        match self {
            TipoHost::Ssh => "SSH",
            TipoHost::Rdp => "RDP",
            TipoHost::Vnc => "VNC",
            TipoHost::Voxl => "voxl",
        }
    }

    /// Puerto por defecto del tipo (§3.1).
    pub fn puerto_por_defecto(self) -> u16 {
        match self {
            TipoHost::Ssh => 22,
            TipoHost::Rdp => 3389,
            TipoHost::Vnc => 5900,
            TipoHost::Voxl => 6901,
        }
    }

    pub fn es_ssh(self) -> bool {
        self == TipoHost::Ssh
    }

    pub fn es_escritorio(self) -> bool {
        !self.es_ssh()
    }

    /// Glifo de la lista de hosts: un cuadrado para un escritorio (ASCII [D]).
    pub fn glifo(self, ascii: bool) -> &'static str {
        if self.es_ssh() {
            ""
        } else if ascii {
            "[D]"
        } else {
            "▣"
        }
    }
}

impl fmt::Display for TipoHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.como_texto())
    }
}

// ---------------------------------------------------------------- tamaño

/// Ancho mínimo de un escritorio de tamaño fijo.
pub const ANCHO_MIN: u16 = 640;
/// Alto mínimo de un escritorio de tamaño fijo.
pub const ALTO_MIN: u16 = 480;
/// Lado máximo del escritorio (entrada no fiable: nunca más de 8192).
pub const LADO_MAX: u16 = 8192;

/// Tamaño del escritorio remoto: se ajusta a la ventana del visor o es fijo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TamanoEscritorio {
    /// Se ajusta a la ventana del visor (Display Control o SetDesktopSize).
    #[default]
    Ventana,
    /// Tamaño fijo, entre 640×480 y 8192×8192.
    Fijo { ancho: u16, alto: u16 },
}

impl TamanoEscritorio {
    /// Forma que se guarda en escritorio_opciones: ventana o AxH.
    pub fn como_texto(self) -> String {
        match self {
            TamanoEscritorio::Ventana => "ventana".to_string(),
            TamanoEscritorio::Fijo { ancho, alto } => format!("{ancho}x{alto}"),
        }
    }

    /// Valida un tamaño escrito a mano. Devuelve el motivo legible del error.
    pub fn desde_texto(texto: &str) -> Result<Self, String> {
        let texto = texto.trim();
        if texto == "ventana" {
            return Ok(TamanoEscritorio::Ventana);
        }
        let (ancho, alto) = texto.split_once(['x', 'X']).ok_or_else(|| {
            "el tamaño debe ser «ventana» o «AxH» (por ejemplo 1920x1080)".to_string()
        })?;
        let ancho: u16 = ancho
            .parse()
            .map_err(|_| "el ancho del escritorio no es un número".to_string())?;
        let alto: u16 = alto
            .parse()
            .map_err(|_| "el alto del escritorio no es un número".to_string())?;
        validar_tamano(ancho, alto)?;
        Ok(TamanoEscritorio::Fijo { ancho, alto })
    }

    /// Tamaño en píxeles, si es fijo.
    pub fn fijo(self) -> Option<(u16, u16)> {
        match self {
            TamanoEscritorio::Ventana => None,
            TamanoEscritorio::Fijo { ancho, alto } => Some((ancho, alto)),
        }
    }

    /// Etiqueta corta para la ficha y la pestaña.
    pub fn etiqueta(self) -> String {
        match self {
            TamanoEscritorio::Ventana => "se ajusta a la ventana".to_string(),
            TamanoEscritorio::Fijo { ancho, alto } => format!("{ancho}×{alto} fijo"),
        }
    }
}

/// Valida un tamaño de escritorio: entre 640×480 y 8192×8192.
pub fn validar_tamano(ancho: u16, alto: u16) -> Result<(), String> {
    if ancho < ANCHO_MIN || alto < ALTO_MIN {
        return Err(format!(
            "el escritorio no puede ser menor de {ANCHO_MIN}×{ALTO_MIN}"
        ));
    }
    if ancho > LADO_MAX || alto > LADO_MAX {
        return Err(format!(
            "el escritorio no puede pasar de {LADO_MAX}×{LADO_MAX}"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------- opciones

/// Opciones del escritorio según el tipo (§3.2). Se guardan como JSON en
/// HOSTS.escritorio_opciones; las claves desconocidas se conservan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpcionesEscritorio {
    pub tamano: TamanoEscritorio,
    /// Texto en los dos sentidos (solo si el host lo tiene activado).
    pub portapapeles: bool,
    /// Distribución que se anuncia a Windows (RDP); None es auto.
    pub teclado: Option<String>,
    /// Pantalla X11 de voxl (:N o :N.M).
    pub pantalla: Option<String>,
    /// Programa de voxl (voxl o una ruta absoluta sin espacios).
    pub programa: Option<String>,
    /// VNC directo ya confirmado por el usuario (sin cifrar).
    pub directo: bool,
    /// Claves que no conoce esta versión: se conservan y se ignoran.
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for OpcionesEscritorio {
    fn default() -> Self {
        Self {
            tamano: TamanoEscritorio::Ventana,
            portapapeles: true,
            teclado: None,
            pantalla: None,
            programa: None,
            directo: false,
            extra: BTreeMap::new(),
        }
    }
}

impl OpcionesEscritorio {
    /// Opciones con los valores por defecto del tipo (un host nuevo).
    pub fn por_defecto(tipo: TipoHost) -> Self {
        let mut opciones = Self::default();
        match tipo {
            TipoHost::Ssh => {
                opciones.portapapeles = false;
            }
            TipoHost::Rdp => {}
            TipoHost::Vnc => {
                opciones.portapapeles = true;
            }
            TipoHost::Voxl => {
                opciones.pantalla = Some(":0".to_string());
                opciones.programa = Some("voxl".to_string());
            }
        }
        opciones
    }

    /// Lee el JSON guardado. Un valor inválido se rechaza al leer (T38); un
    /// JSON corrupto también, con un motivo legible.
    pub fn desde_json(texto: Option<&str>) -> Result<Self, String> {
        let Some(texto) = texto else {
            return Ok(Self::default());
        };
        let texto = texto.trim();
        if texto.is_empty() || texto == "null" {
            return Ok(Self::default());
        }
        let valor: serde_json::Value = serde_json::from_str(texto)
            .map_err(|error| format!("escritorio_opciones no es JSON válido: {error}"))?;
        let objeto = valor
            .as_object()
            .ok_or_else(|| "escritorio_opciones debe ser un objeto JSON".to_string())?;
        let mut opciones = Self::default();
        for (clave, valor) in objeto {
            match clave.as_str() {
                "tamano" => {
                    let texto = valor
                        .as_str()
                        .ok_or_else(|| "«tamano» debe ser texto".to_string())?;
                    opciones.tamano = TamanoEscritorio::desde_texto(texto)?;
                }
                "portapapeles" => {
                    opciones.portapapeles = valor
                        .as_bool()
                        .ok_or_else(|| "«portapapeles» debe ser verdadero o falso".to_string())?;
                }
                "teclado" => {
                    let texto = valor
                        .as_str()
                        .ok_or_else(|| "«teclado» debe ser texto".to_string())?;
                    opciones.teclado = Some(texto.to_string());
                }
                "pantalla" => {
                    let texto = valor
                        .as_str()
                        .ok_or_else(|| "«pantalla» debe ser texto".to_string())?;
                    opciones.pantalla = Some(texto.to_string());
                }
                "programa" => {
                    let texto = valor
                        .as_str()
                        .ok_or_else(|| "«programa» debe ser texto".to_string())?;
                    opciones.programa = Some(texto.to_string());
                }
                "directo" => {
                    opciones.directo = valor
                        .as_bool()
                        .ok_or_else(|| "«directo» debe ser verdadero o falso".to_string())?;
                }
                _ => {
                    opciones.extra.insert(clave.clone(), valor.clone());
                }
            }
        }
        Ok(opciones)
    }

    /// Serializa las opciones a JSON, conservando las claves desconocidas.
    pub fn a_json(&self) -> String {
        let mut objeto = serde_json::Map::new();
        objeto.insert(
            "tamano".to_string(),
            serde_json::Value::String(self.tamano.como_texto()),
        );
        objeto.insert(
            "portapapeles".to_string(),
            serde_json::Value::Bool(self.portapapeles),
        );
        if let Some(teclado) = &self.teclado {
            objeto.insert(
                "teclado".to_string(),
                serde_json::Value::String(teclado.clone()),
            );
        }
        if let Some(pantalla) = &self.pantalla {
            objeto.insert(
                "pantalla".to_string(),
                serde_json::Value::String(pantalla.clone()),
            );
        }
        if let Some(programa) = &self.programa {
            objeto.insert(
                "programa".to_string(),
                serde_json::Value::String(programa.clone()),
            );
        }
        if self.directo {
            objeto.insert("directo".to_string(), serde_json::Value::Bool(true));
        }
        for (clave, valor) in &self.extra {
            objeto.insert(clave.clone(), valor.clone());
        }
        serde_json::Value::Object(objeto).to_string()
    }

    /// Valida las opciones del tipo. Devuelve el primer error legible.
    pub fn validar(&self, tipo: TipoHost) -> Result<(), String> {
        if let TamanoEscritorio::Fijo { ancho, alto } = self.tamano {
            validar_tamano(ancho, alto)?;
        }
        if let Some(teclado) = &self.teclado {
            validar_teclado(teclado)?;
        }
        if tipo == TipoHost::Voxl {
            let pantalla = self.pantalla.as_deref().unwrap_or(":0");
            if !pantalla_valida(pantalla) {
                return Err("pantalla X11 inválida: usa :1 o :1.0".to_string());
            }
            let programa = self.programa.as_deref().unwrap_or("voxl");
            if !programa_valido(programa) {
                return Err("programa inválido: voxl o una ruta absoluta sin espacios".to_string());
            }
        } else if self.pantalla.is_some() {
            return Err("«pantalla» solo vale en un host voxl".to_string());
        }
        if tipo != TipoHost::Voxl && self.programa.is_some() {
            return Err("«programa» solo vale en un host voxl".to_string());
        }
        if tipo != TipoHost::Rdp && self.teclado.is_some() {
            return Err("«teclado» solo vale en un host RDP".to_string());
        }
        Ok(())
    }

    /// Distribución efectiva: la del host o, si es auto, la configurada (que a
    /// su vez sale de LANG).
    pub fn distribucion_efectiva(&self, distribucion_auto: &str) -> String {
        match self.teclado.as_deref() {
            Some(teclado) if teclado != "auto" => teclado.to_string(),
            _ => distribucion_auto.to_string(),
        }
    }
}

/// Pantalla X11 válida: :N o :N.M, con N de una o dos cifras.
pub fn pantalla_valida(pantalla: &str) -> bool {
    let Some(resto) = pantalla.strip_prefix(':') else {
        return false;
    };
    let (numero, decimal) = match resto.split_once('.') {
        Some((numero, decimal)) => (numero, Some(decimal)),
        None => (resto, None),
    };
    let cifras =
        !numero.is_empty() && numero.len() <= 2 && numero.chars().all(|c| c.is_ascii_digit());
    let decimal_ok = match decimal {
        None => true,
        Some(decimal) => decimal.len() == 1 && decimal.chars().all(|c| c.is_ascii_digit()),
    };
    cifras && decimal_ok
}

/// Programa de voxl válido: voxl o una ruta absoluta sin espacios ni
/// caracteres de shell.
pub fn programa_valido(programa: &str) -> bool {
    if programa == "voxl" {
        return true;
    }
    programa.starts_with('/')
        && programa
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

/// Distribución de teclado válida: auto o algo como es-ES.
pub fn validar_teclado(teclado: &str) -> Result<(), String> {
    if teclado == "auto" {
        return Ok(());
    }
    let valido = !teclado.is_empty()
        && teclado.len() <= 32
        && teclado
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if valido {
        Ok(())
    } else {
        Err("distribución de teclado inválida: usa auto o algo como es-ES".to_string())
    }
}

/// ¿Es una dirección de loopback? (VNC directo solo se admite con confirmación
/// fuera de loopback; en loopback es el caso normal de una VM local.)
pub fn es_loopback(direccion: &str) -> bool {
    let direccion = direccion.trim();
    direccion == "localhost"
        || direccion == "::1"
        || direccion == "[::1]"
        || direccion.starts_with("127.")
}

// ---------------------------------------------------------------- sesión

/// Estado de una sesión de escritorio (§3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoEscritorio {
    Abriendo,
    Abierto,
    Caido,
    Cerrando,
}

impl EstadoEscritorio {
    pub fn como_texto(self) -> &'static str {
        match self {
            EstadoEscritorio::Abriendo => "abriendo",
            EstadoEscritorio::Abierto => "abierto",
            EstadoEscritorio::Caido => "caido",
            EstadoEscritorio::Cerrando => "cerrando",
        }
    }

    pub fn etiqueta(self) -> &'static str {
        match self {
            EstadoEscritorio::Abriendo => "abriendo",
            EstadoEscritorio::Abierto => "abierto",
            EstadoEscritorio::Caido => "caído",
            EstadoEscritorio::Cerrando => "cerrando",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "abriendo" => Some(EstadoEscritorio::Abriendo),
            "abierto" => Some(EstadoEscritorio::Abierto),
            "caido" => Some(EstadoEscritorio::Caido),
            "cerrando" => Some(EstadoEscritorio::Cerrando),
            _ => None,
        }
    }
}

/// Fase de la apertura (solo cuando el estado es abriendo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaseEscritorio {
    Conectando,
    VerificandoCertificado,
    Autenticando,
    Arrancando,
}

impl FaseEscritorio {
    pub fn como_texto(self) -> &'static str {
        match self {
            FaseEscritorio::Conectando => "conectando",
            FaseEscritorio::VerificandoCertificado => "verificando_certificado",
            FaseEscritorio::Autenticando => "autenticando",
            FaseEscritorio::Arrancando => "arrancando",
        }
    }

    pub fn etiqueta(self) -> &'static str {
        match self {
            FaseEscritorio::Conectando => "conectando",
            FaseEscritorio::VerificandoCertificado => "verificando certificado",
            FaseEscritorio::Autenticando => "autenticando",
            FaseEscritorio::Arrancando => "arrancando voxl",
        }
    }
}

/// Cómo llega la imagen al visor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImagenEscritorio {
    /// AVC420 tal cual, sin recomprimir (D211).
    H264,
    /// Rectángulos en bruto desde el búfer del escritorio.
    Rectangulos,
    /// La página propia de voxl por el túnel de la sesión.
    Voxl,
}

impl ImagenEscritorio {
    pub fn etiqueta(self) -> &'static str {
        match self {
            ImagenEscritorio::H264 => "H.264",
            ImagenEscritorio::Rectangulos => "rectángulos",
            ImagenEscritorio::Voxl => "voxl",
        }
    }
}

/// Descripción de una sesión de escritorio que se difunde con Escritorios.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoEscritorio {
    pub id: u32,
    pub host_id: i64,
    pub nombre: String,
    pub tipo: TipoHost,
    pub estado: EstadoEscritorio,
    #[serde(default)]
    pub fase: Option<FaseEscritorio>,
    #[serde(default)]
    pub motivo: Option<String>,
    /// Hay un visor conectado.
    pub visor: bool,
    /// Época en segundos desde la que no hay visor (la gracia empieza aquí).
    #[serde(default)]
    pub sin_visor_desde: Option<i64>,
    pub ancho: u16,
    pub alto: u16,
    #[serde(default)]
    pub imagen: Option<ImagenEscritorio>,
    pub fps: u16,
    /// Va directo, sin SSH.
    pub directo: bool,
    #[serde(default)]
    pub a_traves_de: Option<String>,
    /// Época en segundos de la apertura (para la duración).
    pub desde: i64,
    /// El tamaño se ajusta a la ventana del visor.
    pub redimensionable: bool,
}

impl InfoEscritorio {
    pub fn abierto(&self) -> bool {
        self.estado == EstadoEscritorio::Abierto
    }
}

// ---------------------------------------------------------------- diálogos

/// Tipo de cambio de certificado RDP (misma política que las huellas SSH).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoCambioCertificado {
    Nuevo,
    Cambiado,
}

/// Datos del certificado que se enseñan en el diálogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosCertificado {
    pub tipo: TipoCambioCertificado,
    pub sujeto: String,
    pub caducidad: String,
    pub huella: String,
    #[serde(default)]
    pub anterior: Option<String>,
}

/// Petición de credenciales al cliente solicitante (T25).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeticionCredenciales {
    pub tipo: TipoHost,
    #[serde(default)]
    pub usuario: Option<String>,
    #[serde(default)]
    pub dominio: Option<String>,
    /// 1 en el primer intento; tras un fallo de autenticación, 2 y 3.
    pub intento: u8,
}

/// Credenciales que devuelve el cliente (nunca van al log, T44).
#[derive(Clone, PartialEq, Eq)]
pub struct CredencialesEscritorio {
    pub usuario: String,
    pub dominio: Option<String>,
    pub contrasena: Zeroizing<String>,
    pub recordar: bool,
}

impl fmt::Debug for CredencialesEscritorio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredencialesEscritorio")
            .field("usuario", &self.usuario)
            .field("dominio", &self.dominio)
            .field("contrasena", &"<oculta>")
            .field("recordar", &self.recordar)
            .finish()
    }
}

/// Diálogos que el servidor pide al cliente que abrió el escritorio: el
/// servidor no dialoga ni toca el llavero (T25).
#[async_trait::async_trait]
pub trait DialogosEscritorio: Send + Sync {
    /// ¿Se acepta el certificado? En uno cambiado, el cliente exige escribir
    /// el nombre del host antes de responder true.
    async fn certificado(&self, datos: DatosCertificado) -> Result<bool, ErrorEscritorio>;

    /// Credenciales del llavero o del diálogo. None si el usuario cancela.
    async fn credenciales(
        &self,
        peticion: PeticionCredenciales,
    ) -> Result<Option<CredencialesEscritorio>, ErrorEscritorio>;
}

/// Diálogos compartidos entre la tarea de conexión y el servidor.
pub type Dialogo = std::sync::Arc<dyn DialogosEscritorio>;

// ---------------------------------------------------------------- eventos

/// Zona rectangular del escritorio (píxeles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u16,
    pub y: u16,
    pub ancho: u16,
    pub alto: u16,
}

impl Region {
    pub fn nueva(x: u16, y: u16, ancho: u16, alto: u16) -> Self {
        Self { x, y, ancho, alto }
    }

    pub fn vacia(&self) -> bool {
        self.ancho == 0 || self.alto == 0
    }

    /// Unión de dos regiones.
    pub fn unir(&self, otra: &Region) -> Region {
        let x = self.x.min(otra.x);
        let y = self.y.min(otra.y);
        let derecha = (self.x + self.ancho).max(otra.x + otra.ancho);
        let abajo = (self.y + self.alto).max(otra.y + otra.alto);
        Region::nueva(x, y, derecha - x, abajo - y)
    }

    /// ¿La zona está dentro de un escritorio de este tamaño?
    pub fn dentro_de(&self, ancho: u16, alto: u16) -> bool {
        u32::from(self.x) + u32::from(self.ancho) <= u32::from(ancho)
            && u32::from(self.y) + u32::from(self.alto) <= u32::from(alto)
    }
}

/// Evento que produce un decodificador para el puente.
#[derive(Debug)]
pub enum EventoEscritorio {
    /// La sesión ya está lista (saludo, seguridad y autenticación terminados y
    /// etapa activa construida). Se emite UNA vez, antes de los primeros
    /// píxeles; el servidor pasa la sesión a abierto y emite el enlace. Si un
    /// cliente no lo emitiera, el servidor acepta como señal el primer
    /// Imagen/Rect/Video (compatibilidad).
    Abierto,
    /// Zona cambiada con sus píxeles RGBA (4 bytes por píxel, opacos).
    Rect {
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
        pixeles: Vec<u8>,
    },
    /// CopyRect de VNC: copiar una zona ya pintada.
    Copia {
        origen: (u16, u16),
        destino: (u16, u16),
        ancho: u16,
        alto: u16,
    },
    /// AVC420 de Windows tal cual: H.264 Annex-B y sus regiones (D211).
    Video {
        marco: u32,
        regiones: Vec<Region>,
        h264: Vec<u8>,
    },
    /// El escritorio cambia de tamaño (DesktopSize, ExtendedDesktopSize o
    /// Display Control).
    Tamano { ancho: u16, alto: u16 },
    /// Cursor remoto (RGBA y punto caliente); visible: false lo oculta.
    Cursor {
        visible: bool,
        x: u16,
        y: u16,
        punto: (u16, u16),
        ancho: u16,
        alto: u16,
        pixeles: Vec<u8>,
    },
    /// El remoto tiene texto nuevo en el portapapeles.
    Portapapeles(String),
    /// El remoto pide el texto del portapapeles local.
    PedirPortapapeles,
    /// El tipo de imagen que se está mandando.
    Imagen(ImagenEscritorio),
    /// La conexión se ha caído: la sesión queda caida con el motivo.
    Caido(String),
    /// La conexión se ha cerrado limpiamente (el remoto cerró).
    Cerrado(String),
    /// Fin de la tarea: el puente lo usa para no esperar más.
    Fin,
}

/// Orden que llega del visor.
#[derive(Debug, Clone, PartialEq)]
pub enum OrdenEscritorio {
    /// Tecla: keysym de X11 y KeyboardEvent.code del navegador (vacío si no
    /// lo hay).
    Tecla {
        codigo: String,
        keysym: u32,
        abajo: bool,
    },
    Raton {
        x: u16,
        y: u16,
    },
    Boton {
        boton: u8,
        abajo: bool,
    },
    /// Rueda en píxeles del escritorio (desplazamiento suave).
    Rueda {
        dx: f32,
        dy: f32,
    },
    /// Suelta todas las teclas (al perder el foco, D222).
    SoltarTodo,
    /// El visor pide un tamaño de escritorio nuevo.
    Tamano {
        ancho: u16,
        alto: u16,
    },
    CtrlAltSupr,
    TeclaWindows,
    /// Texto pegado en el visor: va al portapapeles del remoto.
    Portapapeles(String),
    /// El visor pide el portapapeles del remoto.
    PedirPortapapeles,
    /// La pestaña se oculta o se vuelve a ver (v,0/v,1).
    Visible(bool),
    /// El visor pide reconectar la sesión caída (orden rec, desviación
    /// anotada: el informe no la lista en §5.2).
    Reconectar,
}

// ---------------------------------------------------------------- errores

/// Error de una sesión de escritorio, con el motivo legible que se enseña.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorEscritorio {
    /// No se puede llegar al destino (TCP o canal SSH, con plazo).
    Conexion(String),
    /// El servidor habla algo que MAGI no admite.
    Protocolo(String),
    /// Los datos del servidor no cumplen los límites: se cierra sin pánico.
    Datos(String),
    /// Usuario o contraseña incorrectos: se puede reintentar.
    Autenticacion(String),
    /// El usuario canceló un diálogo.
    Cancelado(String),
    /// El remoto cerró la sesión.
    Remoto(String),
}

impl ErrorEscritorio {
    pub fn motivo(&self) -> &str {
        match self {
            ErrorEscritorio::Conexion(motivo)
            | ErrorEscritorio::Protocolo(motivo)
            | ErrorEscritorio::Datos(motivo)
            | ErrorEscritorio::Autenticacion(motivo)
            | ErrorEscritorio::Cancelado(motivo)
            | ErrorEscritorio::Remoto(motivo) => motivo,
        }
    }

    /// ¿Se puede reintentar con otras credenciales?
    pub fn es_autenticacion(&self) -> bool {
        matches!(self, ErrorEscritorio::Autenticacion(_))
    }
}

impl fmt::Display for ErrorEscritorio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.motivo())
    }
}

impl std::error::Error for ErrorEscritorio {}

// ---------------------------------------------------------------- conexión

/// Flujo de bytes del camino hasta el escritorio: un canal direct-tcpip del
/// pool SSH o un TCP directo.
pub trait FlujoEscritorio: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> FlujoEscritorio for T {}

/// Flujo en caja: el cliente RDP y el RFB no saben de dónde viene.
pub type Flujo = Box<dyn FlujoEscritorio>;

/// Datos de un escritorio que el servidor ya ha validado y resuelto.
#[derive(Debug, Clone)]
pub struct ConfigEscritorio {
    pub tipo: TipoHost,
    pub direccion: String,
    pub puerto: u16,
    pub usuario: Option<String>,
    pub dominio: Option<String>,
    pub opciones: OpcionesEscritorio,
    /// Distribución anunciada a Windows (auto ya resuelta).
    pub distribucion_teclado: String,
    /// Ofrecer AVC420 a Windows.
    pub h264: bool,
    /// Tamaño inicial del escritorio (si es fijo, ya es ese).
    pub tamano_inicial: (u16, u16),
    /// Texto en los dos sentidos.
    pub portapapeles: bool,
    /// Huella del certificado RDP aceptada para este host (HOSTS.huella_escritorio),
    /// propia del equipo. La usa el cliente RDP para decidir si el certificado
    /// es nuevo, el mismo o ha cambiado antes de mandar credenciales.
    pub huella_escritorio: Option<String>,
}

impl ConfigEscritorio {
    /// Destino legible dirección:puerto.
    pub fn destino(&self) -> String {
        format!("{}:{}", self.direccion, self.puerto)
    }

    /// Completar usuario y dominio con lo que tenga el host.
    pub fn credenciales(&self, credenciales: CredencialesEscritorio) -> CredencialesEscritorio {
        let mut credenciales = credenciales;
        if credenciales.usuario.trim().is_empty() {
            if let Some(usuario) = &self.usuario {
                credenciales.usuario = usuario.clone();
            }
        }
        if credenciales.dominio.is_none() {
            credenciales.dominio = self.dominio.clone();
        }
        credenciales
    }
}

/// Todo lo que necesita un cliente de escritorio para correr.
pub struct Conexion {
    /// Canal ya abierto hasta el servidor de escritorio.
    pub flujo: Flujo,
    pub config: ConfigEscritorio,
    pub dialogo: Dialogo,
    pub eventos: mpsc::Sender<EventoEscritorio>,
    pub ordenes: mpsc::Receiver<OrdenEscritorio>,
    /// Intento de credenciales (1 en el primero; 2 y 3 tras un rechazo). Lo
    /// fija la capa de sesión, que reabre el camino en cada intento porque
    /// RFB y RDP cierran la conexión al rechazar las credenciales.
    pub intento: u8,
}

/// Lanza una sesión de escritorio en un hilo propio con un runtime actual: la
/// decodificación queda fuera del runtime de red del servidor (D225).
pub fn lanzar_en_hilo<F>(nombre: String, tarea: F) -> std::io::Result<std::thread::JoinHandle<()>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    lanzar_en_hilo_con(nombre, move || tarea)
}

/// Igual que lanzar_en_hilo, pero recibe una fábrica y crea el future DENTRO
/// del hilo. Hace falta cuando el future no es Send: el ActiveStage de IronRDP
/// guarda canales estáticos (`dyn SvcProcessor`) que no lo son, y construir el
/// future fuera obligaría a moverlo entre hilos.
pub fn lanzar_en_hilo_con<F, Fut>(
    nombre: String,
    fabrica: F,
) -> std::io::Result<std::thread::JoinHandle<()>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    std::thread::Builder::new().name(nombre).spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::error!(%error, "no se pudo crear el runtime del escritorio");
                return;
            }
        };
        runtime.block_on(fabrica());
    })
}

/// Distribución de teclado que MAGI anuncia a Windows cuando la configuración
/// dice auto: sale de LANG y, si no, en-US.
pub fn distribucion_desde_lang(lang: Option<&str>) -> String {
    let Some(lang) = lang else {
        return "en-US".to_string();
    };
    let base = lang
        .split(['.', '@'])
        .next()
        .unwrap_or(lang)
        .replace('_', "-");
    match base.as_str() {
        "es-ES" | "es-AR" | "es-MX" | "es" => "es-ES".to_string(),
        "pt-BR" | "pt" => "pt-BR".to_string(),
        "fr-FR" | "fr" => "fr-FR".to_string(),
        "de-DE" | "de" => "de-DE".to_string(),
        "it-IT" | "it" => "it-IT".to_string(),
        otro if otro.contains('-') => otro.to_string(),
        _ => "en-US".to_string(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_tipos_van_y_vuelven() {
        for tipo in TipoHost::TODOS {
            assert_eq!(TipoHost::desde_texto(tipo.como_texto()), Some(tipo));
        }
        assert_eq!(TipoHost::desde_texto("rdp"), Some(TipoHost::Rdp));
        assert_eq!(TipoHost::desde_texto("otro"), None);
        assert!(TipoHost::Ssh.es_ssh());
        assert!(TipoHost::Vnc.es_escritorio());
        assert_eq!(TipoHost::Rdp.puerto_por_defecto(), 3389);
        assert_eq!(TipoHost::Voxl.puerto_por_defecto(), 6901);
        assert_eq!(TipoHost::Vnc.glifo(false), "▣");
        assert_eq!(TipoHost::Vnc.glifo(true), "[D]");
    }

    #[test]
    fn los_tamanos_se_validan() {
        assert_eq!(
            TamanoEscritorio::desde_texto("ventana").unwrap(),
            TamanoEscritorio::Ventana
        );
        assert_eq!(
            TamanoEscritorio::desde_texto("1920x1080").unwrap(),
            TamanoEscritorio::Fijo {
                ancho: 1920,
                alto: 1080
            }
        );
        assert!(TamanoEscritorio::desde_texto("100x100").is_err());
        assert!(TamanoEscritorio::desde_texto("9000x1080").is_err());
        assert!(TamanoEscritorio::desde_texto("1920").is_err());
    }

    #[test]
    fn las_opciones_van_y_vuelven_conservando_las_claves_desconocidas() {
        let opciones = OpcionesEscritorio::desde_json(Some(
            r#"{"tamano":"1920x1080","portapapeles":false,"teclado":"es-ES","otra":"x"}"#,
        ))
        .unwrap();
        assert_eq!(
            opciones.tamano,
            TamanoEscritorio::Fijo {
                ancho: 1920,
                alto: 1080
            }
        );
        assert!(!opciones.portapapeles);
        assert_eq!(opciones.teclado.as_deref(), Some("es-ES"));
        assert!(opciones.extra.contains_key("otra"));
        let json = opciones.a_json();
        let otra_vez = OpcionesEscritorio::desde_json(Some(&json)).unwrap();
        assert_eq!(otra_vez, opciones);
        assert!(OpcionesEscritorio::desde_json(Some("no es json")).is_err());
        assert!(OpcionesEscritorio::desde_json(Some(r#"{"tamano":"9x9"}"#)).is_err());
    }

    #[test]
    fn voxl_valida_pantalla_programa_y_puerto() {
        assert!(pantalla_valida(":1"));
        assert!(pantalla_valida(":12"));
        assert!(pantalla_valida(":1.0"));
        assert!(!pantalla_valida(":1; rm"));
        assert!(!pantalla_valida("1"));
        assert!(programa_valido("voxl"));
        assert!(programa_valido("/usr/local/bin/voxl"));
        assert!(!programa_valido("voxl --foo"));
        assert!(!programa_valido("dos palabras"));
        assert!(!programa_valido(""));
        let mut opciones = OpcionesEscritorio::por_defecto(TipoHost::Voxl);
        assert!(opciones.validar(TipoHost::Voxl).is_ok());
        opciones.pantalla = Some(":1; rm".to_string());
        assert!(opciones.validar(TipoHost::Voxl).is_err());
    }

    #[test]
    fn las_regiones_se_unen_y_se_comprueban() {
        let a = Region::nueva(10, 10, 20, 20);
        let b = Region::nueva(40, 5, 10, 10);
        let union = a.unir(&b);
        assert_eq!(union, Region::nueva(10, 5, 40, 25));
        assert!(a.dentro_de(100, 100));
        assert!(!Region::nueva(90, 90, 20, 20).dentro_de(100, 100));
    }

    #[test]
    fn la_distribucion_sale_de_lang() {
        assert_eq!(distribucion_desde_lang(Some("es_ES.UTF-8")), "es-ES");
        assert_eq!(distribucion_desde_lang(Some("en_US.UTF-8")), "en-US");
        assert_eq!(distribucion_desde_lang(None), "en-US");
        assert_eq!(distribucion_desde_lang(Some("de_DE.UTF-8")), "de-DE");
    }

    #[test]
    fn el_loopback_se_reconoce() {
        assert!(es_loopback("127.0.0.1"));
        assert!(es_loopback("localhost"));
        assert!(es_loopback("::1"));
        assert!(!es_loopback("192.0.2.30"));
    }
}
