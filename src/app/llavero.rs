//! Interfaz del llavero de fichero con frase maestra (Fase 17, Termux).
//!
//! Todo lo de aquí se activa por `App.llavero.de_fichero()` y por su
//! `estado()`, nunca por preguntar la plataforma: con el llavero del sistema
//! (`de_fichero() == None`) nada de este módulo hace nada y MAGI se porta como
//! antes. La frase maestra solo vive en los campos del diálogo (que se borran
//! al cerrar y al caer) y en el `LlaveroFichero`; nunca va al log, a REGISTRO,
//! al servidor ni a un `Debug`.
//!
//! Qué va en un hilo y qué no:
//! - `crear`, `desbloquear`, `cambiar_frase` y el guardado de la contraseña
//!   «recordada» gastan scrypt (~1 s en un móvil): van siempre en
//!   `spawn_blocking` y su resultado vuelve por `Evento::Llavero`, con un id
//!   por petición. El diálogo que espera muestra «trabajando…» y no atiende
//!   ninguna tecla (Esc tampoco: la tarea no se puede abortar y el llavero se
//!   crea o cambia igualmente, así que lo que se ve es lo que pasa); una
//!   respuesta cuyo id ya no está en vuelo se descarta.
//! - Quedan en el hilo de la UI (excepción aceptada por el informe, §8): las
//!   escrituras de `sincronizacion::equipos::{crear, unir, quitar, rotar,
//!   desactivar}`, que guardan u olvidan la clave de sincronización con un
//!   `guardar` u `olvidar` que reescribe el fichero (~1 s de scrypt); hacen su
//!   trabajo bajo el cerrojo de la carpeta y no se parten sin cambiar su
//!   contrato. También el guardado de la contraseña de un escritorio desde la
//!   ficha y `olvidar` una contraseña (paleta y ficha). `borrar` y
//!   `apartar_danado` no gastan scrypt y van también en el hilo de la UI.
//!
//! Una operación que necesita el llavero abre el diálogo que toque con su
//! `Continuacion`: lo que hay que hacer cuando el llavero quede abierto
//! (creado o desbloqueado) o cuando el usuario diga que no (Esc). Las
//! continuaciones se mueven, nunca se clonan, y lo que lleva secretos va en
//! `Zeroizing`. Una operación automática nunca abre un diálogo: da el error
//! `BLOQUEADO` y, desde la barra, la marca «llavero bloqueado».

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::oneshot;
use zeroize::{Zeroize, Zeroizing};

// `MARCA_SIN_CONTRASENA` es la constante del propio núcleo con la que explica
// que una conexión automática no tiene contraseña guardada: un cambio de su
// texto no rompe en silencio el reescrito. Con el llavero cerrado, el cliente
// la sustituye por `BLOQUEADO` en la pestaña (`normalizar_motivos_llavero`):
// es el único sitio donde se hace.
use magi_nucleo::conexion::cliente::{
    PLAZO_LLAVERO, SIN_CONTRASENA_EN_LLAVERO as MARCA_SIN_CONTRASENA,
};
use magi_nucleo::llavero_fichero::{
    preparar_frase, ErrorLlavero, ESPERA_SERIE, FALLOS_POR_SERIE, MAX_FRASE_BYTES, SIN_CREAR,
};

use crate::escritorio::{CredencialesEscritorio, EstadoEscritorio, PeticionCredenciales};
use crate::llavero::{es_bloqueado, EstadoLlavero, LlaveroSecreto, BLOQUEADO};
use crate::modelo::ResultadoRegistro;
use crate::protocolo::{self, EstadoSesionRemota};
use crate::ui::componentes::CampoTexto;

use super::{AccionInventario, App, Dialogo, EntradaPaleta, Evento};

/// Palabra que hay que escribir para borrar el llavero.
pub const PALABRA_BORRAR: &str = "borrar";

/// Texto de la barra cuando el usuario cierra un diálogo sin dar la frase y la
/// operación necesitaba guardar un secreto.
pub const SIN_LLAVERO_NO_GUARDA: &str = "sin llavero no se puede guardar la clave";

// ---------------------------------------------------------------- tipos

/// Qué pide el diálogo del llavero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoLlavero {
    /// LLAVERO DE MAGI: la frase abre el fichero.
    Desbloquear,
    /// CREAR LA FRASE MAESTRA: frase y repetición.
    Crear,
    /// CAMBIAR LA FRASE MAESTRA: la actual y la nueva dos veces.
    Cambiar,
    /// BORRAR EL LLAVERO: hay que escribir `borrar`.
    Borrar,
    /// LLAVERO DAÑADO: apartarlo o salir sin tocar nada.
    Danado,
}

impl ModoLlavero {
    /// Cuántos campos de texto tiene el modo.
    #[must_use]
    pub fn campos(self) -> usize {
        match self {
            ModoLlavero::Desbloquear | ModoLlavero::Borrar => 1,
            ModoLlavero::Crear => 2,
            ModoLlavero::Cambiar => 3,
            ModoLlavero::Danado => 0,
        }
    }

    /// ¿Sus campos son frases (enmascarados y borrados al cerrar)?
    #[must_use]
    pub fn es_secreto(self) -> bool {
        matches!(
            self,
            ModoLlavero::Desbloquear | ModoLlavero::Crear | ModoLlavero::Cambiar
        )
    }
}

/// Qué hay que hacer cuando el llavero queda abierto (o creado), o cuando el
/// usuario cierra el diálogo sin dar la frase. Se mueven (`Option::take`,
/// `mem::take`), nunca se clonan; lo que lleva secretos va en `Zeroizing`.
pub enum Continuacion {
    /// Una conexión interactiva pidió la contraseña y el host la guarda en el
    /// llavero: se reintenta con el llavero abierto. Esc: el diálogo de
    /// contraseña normal, sin «recordar».
    ReintentarContrasena {
        sesion_id: u32,
        host: String,
        intento: u8,
        recordar_por_defecto: bool,
    },
    /// El usuario tecleó una contraseña con «recordar» y el llavero no está
    /// listo: se envía al servidor y se guarda cuando la sesión abra. Esc: se
    /// usa sin guardarse (y no se crea ningún fichero).
    EnviarContrasena {
        sesion_id: u32,
        contrasena: Zeroizing<String>,
    },
    /// El servidor pidió las credenciales de un escritorio (RDP, VNC) y el host
    /// las guarda en el llavero, que está cerrado: se vuelve a mirar con él
    /// abierto. Esc: el diálogo de credenciales, sin «recordar».
    ReintentarCredenciales {
        peticion_id: u64,
        escritorio_id: u32,
        host_nombre: String,
        peticion: PeticionCredenciales,
    },
    /// El usuario tecleó las credenciales de un escritorio con «recordar» y el
    /// llavero no está listo: se envían y se guardan cuando la sesión abra.
    /// Esc: se usan sin guardarse.
    EnviarCredenciales {
        peticion_id: u64,
        escritorio_id: u32,
        credenciales: CredencialesEscritorio,
    },
    /// Lo mismo para una conexión efímera del propio cliente (la prueba de la
    /// ficha): la tarea de conexión espera por `responder`; con «sí» guarda la
    /// contraseña ella misma, con Esc la usa sin guardarla.
    ContrasenaLocal {
        contrasena: Zeroizing<String>,
        responder: oneshot::Sender<Option<(Zeroizing<String>, bool)>>,
    },
    /// Crear la sincronización en una carpeta vacía (la clave nueva va al
    /// llavero). `texto` es lo que el usuario escribió, para volver a pedirlo.
    CrearSincronizacion { carpeta: PathBuf, texto: String },
    /// Unir este equipo con la clave pegada.
    UnirEquipo {
        carpeta: PathBuf,
        clave: Zeroizing<String>,
    },
    /// k: mostrar la clave de sincronización.
    MostrarClave,
    /// Quitar un equipo (rota la clave).
    QuitarEquipo(String),
    /// Rotar la clave.
    RotarClave,
    /// Desactivar la sincronización (olvida la clave del llavero).
    Desactivar,
}

impl Continuacion {
    /// Nombre para los mensajes de prueba; nunca lleva datos.
    #[must_use]
    pub fn nombre(&self) -> &'static str {
        match self {
            Continuacion::ReintentarContrasena { .. } => "reintentar_contrasena",
            Continuacion::EnviarContrasena { .. } => "enviar_contrasena",
            Continuacion::ContrasenaLocal { .. } => "contrasena_local",
            Continuacion::ReintentarCredenciales { .. } => "reintentar_credenciales",
            Continuacion::EnviarCredenciales { .. } => "enviar_credenciales",
            Continuacion::CrearSincronizacion { .. } => "crear_sincronizacion",
            Continuacion::UnirEquipo { .. } => "unir_equipo",
            Continuacion::MostrarClave => "mostrar_clave",
            Continuacion::QuitarEquipo(_) => "quitar_equipo",
            Continuacion::RotarClave => "rotar_clave",
            Continuacion::Desactivar => "desactivar",
        }
    }

    /// ¿Contesta a una pregunta del servidor (una sesión o un escritorio)? Si
    /// el servidor cae, ya no hay a quién contestar y los ids de su sesión
    /// podrían reutilizarse en el servidor nuevo.
    #[must_use]
    pub fn es_del_servidor(&self) -> bool {
        matches!(
            self,
            Continuacion::ReintentarContrasena { .. }
                | Continuacion::EnviarContrasena { .. }
                | Continuacion::ReintentarCredenciales { .. }
                | Continuacion::EnviarCredenciales { .. }
        )
    }

    /// Para qué necesita el llavero: lo que decide si, con el llavero sin
    /// crear, hay algo que pedir.
    #[must_use]
    pub fn uso(&self) -> UsoLlavero {
        match self {
            Continuacion::ReintentarContrasena { .. }
            | Continuacion::ReintentarCredenciales { .. }
            | Continuacion::MostrarClave
            | Continuacion::Desactivar => UsoLlavero::Leer,
            Continuacion::EnviarContrasena { .. }
            | Continuacion::EnviarCredenciales { .. }
            | Continuacion::ContrasenaLocal { .. }
            | Continuacion::CrearSincronizacion { .. }
            | Continuacion::UnirEquipo { .. }
            | Continuacion::QuitarEquipo(_)
            | Continuacion::RotarClave => UsoLlavero::Escribir,
        }
    }
}

impl std::fmt::Debug for Continuacion {
    /// Solo el nombre: nada de lo que lleva sale en un `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.nombre())
    }
}

/// Diálogo del llavero (los cinco modos). Los campos de frase se borran al
/// cerrar y al caer.
pub struct DialogoLlavero {
    pub modo: ModoLlavero,
    pub campos: Vec<CampoTexto>,
    /// Campo con el foco (índice en `campos`).
    pub foco: usize,
    /// Último error, en una línea (nunca lleva la frase).
    pub error: Option<String>,
    /// Id de la petición de scrypt en vuelo: mientras haya una, el diálogo
    /// muestra «trabajando…» y solo atiende a Esc.
    pub trabajando: Option<u64>,
    /// Fin de la espera impuesta por cinco fallos seguidos.
    pub espera_hasta: Option<Instant>,
    /// Por qué está dañado (modo `Danado`).
    pub motivo_danado: Option<String>,
    /// El diálogo salió al abrir MAGI: salir del dañado cierra MAGI.
    pub al_arrancar: bool,
    /// Qué hacer cuando quede abierto o creado (o al decir que no).
    pub continuaciones: Vec<Continuacion>,
}

impl DialogoLlavero {
    /// Diálogo vacío de un modo, con los campos de frase ya con capacidad para
    /// que teclear no reasigne el búfer (y deje copias sin borrar).
    #[must_use]
    pub fn nuevo(modo: ModoLlavero) -> Self {
        let campos = (0..modo.campos())
            .map(|_| {
                if modo.es_secreto() {
                    campo_secreto()
                } else {
                    CampoTexto::default()
                }
            })
            .collect();
        Self {
            modo,
            campos,
            foco: 0,
            error: None,
            trabajando: None,
            espera_hasta: None,
            motivo_danado: None,
            al_arrancar: false,
            continuaciones: Vec::new(),
        }
    }

    /// Diálogo LLAVERO DAÑADO con el motivo.
    #[must_use]
    pub fn danado(motivo: String) -> Self {
        let mut dialogo = Self::nuevo(ModoLlavero::Danado);
        dialogo.motivo_danado = Some(motivo);
        dialogo
    }

    /// Pega texto en el campo con foco (si el modo tiene campos y no está
    /// trabajando). Devuelve si lo consumió.
    pub fn pegar_texto(&mut self, texto: &str) -> bool {
        if self.trabajando.is_some() {
            return false;
        }
        let secreto = self.modo.es_secreto();
        match self.campos.get_mut(self.foco) {
            Some(campo) => {
                if secreto {
                    reservar_sin_dejar_copias(campo, texto.len());
                }
                campo.insertar_texto(texto);
                true
            }
            None => false,
        }
    }

    /// Escribe una tecla en el campo con foco. En un campo de frase, antes
    /// reserva sitio para que el búfer no se reasigne a mitad (la reasignación
    /// deja la frase a medias en la memoria que se libera sin borrar).
    fn teclear(&mut self, tecla: &KeyEvent) {
        let secreto = self.modo.es_secreto();
        if let Some(campo) = self.campos.get_mut(self.foco) {
            if secreto {
                // Una tecla inserta, como mucho, un carácter (4 bytes).
                reservar_sin_dejar_copias(campo, 4);
            }
            campo.manejar_tecla(tecla);
        }
    }

    /// Borra de la memoria todo lo escrito.
    pub fn borrar_campos(&mut self) {
        for campo in &mut self.campos {
            campo.texto.zeroize();
            campo.cursor = 0;
        }
    }

    /// Pasa el diálogo a LLAVERO DAÑADO con el motivo: ya no tiene campos ni
    /// espera nada de la frase. Las continuaciones se conservan.
    pub fn pasar_a_danado(&mut self, motivo: String) {
        self.borrar_campos();
        self.campos.clear();
        self.foco = 0;
        self.trabajando = None;
        self.espera_hasta = None;
        self.modo = ModoLlavero::Danado;
        self.motivo_danado = Some(motivo);
        self.error = None;
    }

    /// Saca las continuaciones que contestan al servidor (sesiones y
    /// escritorios) y deja las demás.
    fn sacar_continuaciones_del_servidor(&mut self) -> Vec<Continuacion> {
        let todas = std::mem::take(&mut self.continuaciones);
        let (del_servidor, otras): (Vec<_>, Vec<_>) =
            todas.into_iter().partition(Continuacion::es_del_servidor);
        self.continuaciones = otras;
        del_servidor
    }

    /// Segundos que faltan de la espera, redondeados hacia arriba.
    #[must_use]
    pub fn espera_restante(&self, ahora: Instant) -> Option<u64> {
        let hasta = self.espera_hasta?;
        let restante = hasta.checked_duration_since(ahora)?;
        if restante.is_zero() {
            return None;
        }
        Some(restante.as_secs() + u64::from(restante.subsec_nanos() > 0))
    }

    /// El diálogo es un texto largo sin campos (se desplaza con ↑↓).
    #[must_use]
    pub fn es_informativo(&self) -> bool {
        self.modo == ModoLlavero::Danado
    }

    /// Pone en el diálogo lo que dijo el llavero (la petición ya terminó).
    fn aplicar_error(&mut self, error: ErrorLlavero, tipo: TipoFrase, ahora: Instant) {
        self.foco = 0;
        match error {
            ErrorLlavero::FraseIncorrecta { intentos_seguidos } => {
                self.error = Some(match tipo {
                    TipoFrase::Cambio => "la frase actual no es correcta".to_string(),
                    _ => "frase incorrecta".to_string(),
                });
                if intentos_seguidos > 0 && intentos_seguidos % FALLOS_POR_SERIE == 0 {
                    self.espera_hasta = Some(ahora + ESPERA_SERIE);
                }
            }
            ErrorLlavero::Espera { segundos } => {
                self.espera_hasta = Some(ahora + Duration::from_secs(segundos));
                self.error = Some(texto_espera(segundos));
            }
            // El fichero ya no se puede leer: pasa a LLAVERO DAÑADO.
            ErrorLlavero::Danado(motivo) => self.pasar_a_danado(motivo),
            ErrorLlavero::Inexistente if self.modo == ModoLlavero::Desbloquear => {
                // Alguien borró llavero.age: ya no hay nada que desbloquear.
                self.borrar_campos();
                self.campos = vec![campo_secreto(), campo_secreto()];
                self.modo = ModoLlavero::Crear;
                self.error = Some("el llavero ya no existe: escribe una frase nueva".to_string());
            }
            ErrorLlavero::YaExiste => {
                // Otra ventana lo creó mientras se pedía la frase: se pasa a
                // pedir la suya, con las mismas continuaciones.
                self.borrar_campos();
                self.campos = vec![campo_secreto()];
                self.modo = ModoLlavero::Desbloquear;
                self.error =
                    Some("otra ventana ha creado el llavero: escribe su frase".to_string());
            }
            otro => self.error = Some(otro.to_string()),
        }
    }
}

impl Drop for DialogoLlavero {
    fn drop(&mut self) {
        self.borrar_campos();
    }
}

impl std::fmt::Debug for DialogoLlavero {
    /// Nunca imprime lo escrito en los campos ni las continuaciones.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialogoLlavero")
            .field("modo", &self.modo)
            .field("trabajando", &self.trabajando.is_some())
            .field("continuaciones", &self.continuaciones.len())
            .finish_non_exhaustive()
    }
}

/// Un campo de frase con el búfer reservado de una vez.
fn campo_secreto() -> CampoTexto {
    CampoTexto {
        texto: String::with_capacity(MAX_FRASE_BYTES + 16),
        cursor: 0,
    }
}

/// Asegura que caben `extra` bytes más en el campo sin que `String` reasigne
/// su búfer: una reasignación libera el anterior con la frase dentro y sin
/// borrar. Si no caben, el texto pasa a un búfer nuevo y el viejo se borra
/// con `zeroize` antes de soltarlo.
fn reservar_sin_dejar_copias(campo: &mut CampoTexto, extra: usize) {
    let necesario = campo.texto.len().saturating_add(extra);
    if necesario <= campo.texto.capacity() {
        return;
    }
    let mut nuevo = String::with_capacity(necesario.saturating_add(16));
    nuevo.push_str(&campo.texto);
    campo.texto.zeroize();
    campo.texto = nuevo;
}

/// Resultado de una tarea de scrypt, de vuelta en el bucle de la UI. Cada uno
/// lleva el id de su petición.
pub enum EventoLlavero {
    Desbloqueo {
        id: u64,
        resultado: Result<(), ErrorLlavero>,
    },
    Creacion {
        id: u64,
        resultado: Result<(), ErrorLlavero>,
    },
    CambioFrase {
        id: u64,
        resultado: Result<(), ErrorLlavero>,
    },
    /// La contraseña «recordada» ya está (o no) en el llavero.
    ContrasenaGuardada {
        id: u64,
        host_id: i64,
        host: String,
        resultado: Result<(), String>,
    },
}

impl EventoLlavero {
    /// Id de la petición a la que responde.
    #[must_use]
    pub fn id(&self) -> u64 {
        match self {
            EventoLlavero::Desbloqueo { id, .. }
            | EventoLlavero::Creacion { id, .. }
            | EventoLlavero::CambioFrase { id, .. }
            | EventoLlavero::ContrasenaGuardada { id, .. } => *id,
        }
    }
}

impl std::fmt::Debug for EventoLlavero {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nombre = match self {
            EventoLlavero::Desbloqueo { .. } => "Desbloqueo",
            EventoLlavero::Creacion { .. } => "Creacion",
            EventoLlavero::CambioFrase { .. } => "CambioFrase",
            EventoLlavero::ContrasenaGuardada { .. } => "ContrasenaGuardada",
        };
        write!(f, "EventoLlavero::{nombre}#{}", self.id())
    }
}

/// Tope de peticiones de pestañas que se recuerdan con el llavero cerrado.
const TOPE_PETICIONES_IGNORADAS: usize = 64;

/// Texto de una pestaña caída cuyo motivo era «llavero bloqueado» y el
/// llavero ya se ha desbloqueado.
const MOTIVO_DESBLOQUEADO: &str = "llavero desbloqueado: reconecta la pestaña";

/// Un `PideLlavero` de una pestaña que se dejó sin contestar porque el
/// llavero estaba cerrado: si el usuario lo desbloquea antes de que venza el
/// plazo del servidor, todavía se puede contestar. No lleva ningún secreto.
struct PeticionIgnorada {
    sesion_id: u32,
    host: String,
    usuario: String,
    desde: Instant,
}

impl PeticionIgnorada {
    /// El servidor ya no espera la respuesta (pasó `PLAZO_LLAVERO`).
    fn vencida(&self, ahora: Instant) -> bool {
        ahora.saturating_duration_since(self.desde) >= PLAZO_LLAVERO
    }
}

impl std::fmt::Debug for PeticionIgnorada {
    /// Solo la sesión: ni el host ni el usuario salen en un `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PeticionIgnorada#{}", self.sesion_id)
    }
}

/// Estado de la interfaz del llavero dentro de la `App`.
#[derive(Debug, Default)]
pub struct UiLlavero {
    /// Último id de petición repartido.
    siguiente: u64,
    /// Peticiones de scrypt en vuelo. Una respuesta cuyo id no esté aquí se
    /// descarta.
    en_vuelo: HashSet<u64>,
    /// Algo necesitó el llavero y estaba cerrado: la barra lo dice mientras
    /// siga bloqueado («llavero bloqueado»).
    pub necesitado: bool,
    /// Lo que pinta la barra: `necesitado` y el llavero sigue cerrado. Se
    /// recalcula en el tick y cuando cambia algo (`refrescar_marca_llavero`),
    /// nunca al pintar: leer el estado del llavero es entrada/salida.
    marca: bool,
    /// Peticiones de contraseña de pestañas que se ignoraron con el llavero
    /// cerrado, de la más antigua a la más reciente, con tope.
    ignoradas: VecDeque<PeticionIgnorada>,
}

impl UiLlavero {
    /// ¿Hay alguna petición de scrypt en vuelo?
    #[must_use]
    pub fn hay_en_vuelo(&self) -> bool {
        !self.en_vuelo.is_empty()
    }

    /// Recuerda una petición ignorada. Una sesión solo tiene una (la última
    /// sustituye a la anterior), las vencidas se descartan y, con el tope, se
    /// olvida la más antigua.
    fn recordar_peticion(&mut self, peticion: PeticionIgnorada) {
        let ahora = peticion.desde;
        self.ignoradas
            .retain(|otra| otra.sesion_id != peticion.sesion_id && !otra.vencida(ahora));
        while self.ignoradas.len() >= TOPE_PETICIONES_IGNORADAS {
            self.ignoradas.pop_front();
        }
        self.ignoradas.push_back(peticion);
    }

    /// Saca las peticiones recordadas que siguen dentro del plazo.
    fn tomar_vigentes(&mut self, ahora: Instant) -> Vec<PeticionIgnorada> {
        std::mem::take(&mut self.ignoradas)
            .into_iter()
            .filter(|peticion| !peticion.vencida(ahora))
            .collect()
    }

    /// Descarta las que ya vencieron.
    fn descartar_vencidas(&mut self, ahora: Instant) {
        self.ignoradas.retain(|peticion| !peticion.vencida(ahora));
    }
}

/// Qué ha pasado al buscar la contraseña de una conexión interactiva en el
/// llavero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentoLlavero {
    /// Se envió la contraseña guardada.
    Enviada,
    /// El host la guarda en el llavero pero está cerrado: hay que pedir la
    /// frase antes.
    Bloqueado,
    /// No hay contraseña guardada (o el host no la usa): diálogo normal.
    NoHay,
}

/// Para qué se necesita el llavero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsoLlavero {
    /// Va a escribir: necesita el llavero abierto (o creado).
    Escribir,
    /// Solo va a leer o borrar lo que haya: con el fichero inexistente no hay
    /// nada que pedir.
    Leer,
}

/// El llavero visto por una operación que solo quiere olvidar secretos
/// (desactivar la sincronización): sin fichero, no hay nada que olvidar y eso
/// no es un fallo. Con fichero, todo pasa tal cual al llavero.
pub struct OlvidoSinFichero<'a>(pub &'a dyn LlaveroSecreto);

impl LlaveroSecreto for OlvidoSinFichero<'_> {
    fn guardar(&self, cuenta: &str, valor: &str) -> Result<(), String> {
        self.0.guardar(cuenta, valor)
    }

    fn recuperar(&self, cuenta: &str) -> Result<Option<Zeroizing<String>>, String> {
        self.0.recuperar(cuenta)
    }

    fn olvidar(&self, cuenta: &str) -> Result<bool, String> {
        match self.0.olvidar(cuenta) {
            Err(error) if error == SIN_CREAR => Ok(false),
            otro => otro,
        }
    }
}

/// Dónde estaba un diálogo del llavero cuando llegó una respuesta.
enum Sitio {
    Superior,
    Pila(usize),
}

// ---------------------------------------------------------------- la App

impl App {
    /// ¿El llavero de este equipo es el de fichero con frase maestra?
    #[must_use]
    pub fn llavero_con_frase(&self) -> bool {
        self.llavero.de_fichero().is_some()
    }

    /// ¿El llavero de fichero existe y está cerrado (hay que pedir la frase)?
    #[must_use]
    pub fn llavero_cerrado(&self) -> bool {
        self.llavero_con_frase() && self.llavero.estado() == EstadoLlavero::Bloqueado
    }

    /// Qué diálogo hace falta para poder usar el llavero, si hace falta. Con
    /// el llavero del sistema, ninguno. `Leer` no pide crear nada.
    #[must_use]
    pub fn necesidad_llavero(&self, uso: UsoLlavero) -> Option<ModoLlavero> {
        self.llavero.de_fichero()?;
        match self.llavero.estado() {
            EstadoLlavero::Abierto => None,
            EstadoLlavero::Bloqueado => Some(ModoLlavero::Desbloquear),
            EstadoLlavero::SinCrear => match uso {
                UsoLlavero::Escribir => Some(ModoLlavero::Crear),
                UsoLlavero::Leer => None,
            },
            EstadoLlavero::Danado(_) => Some(ModoLlavero::Danado),
        }
    }

    /// Al abrir MAGI: con el fichero cerrado, LLAVERO DE MAGI; dañado, LLAVERO
    /// DAÑADO. Esc sigue sin llavero (en el dañado, sale de MAGI sin tocar
    /// nada). No hace nada con el llavero del sistema.
    pub fn abrir_llavero_del_arranque(&mut self) {
        let cerrado = self.llavero_con_frase()
            && matches!(
                self.llavero.estado(),
                EstadoLlavero::Bloqueado | EstadoLlavero::Danado(_)
            );
        if cerrado {
            self.abrir_dialogo_llavero(None, true);
        }
    }

    /// Pide el llavero para una operación interactiva: abre (o amplía) el
    /// diálogo que toque con la continuación. Si el llavero ya está listo, la
    /// continuación corre ya. Las operaciones automáticas no pasan por aquí.
    pub fn pedir_llavero(&mut self, continuacion: Option<Continuacion>) {
        self.abrir_dialogo_llavero(continuacion, false);
    }

    fn abrir_dialogo_llavero(&mut self, continuacion: Option<Continuacion>, al_arrancar: bool) {
        let (modo, motivo) = match self.llavero.de_fichero().map(|_| self.llavero.estado()) {
            None | Some(EstadoLlavero::Abierto) => {
                if let Some(continuacion) = continuacion {
                    self.continuar_tras_llavero(continuacion);
                }
                return;
            }
            Some(EstadoLlavero::Bloqueado) => {
                self.poner_llavero_necesitado(true);
                (ModoLlavero::Desbloquear, None)
            }
            Some(EstadoLlavero::SinCrear) => (ModoLlavero::Crear, None),
            Some(EstadoLlavero::Danado(motivo)) => (ModoLlavero::Danado, Some(motivo)),
        };
        // Coalescencia: una segunda petición añade su continuación al diálogo
        // que ya está abierto en lugar de abrir otro.
        if let Some(existente) = self.dialogo_llavero_abierto(modo) {
            existente.continuaciones.extend(continuacion);
            return;
        }
        let mut dialogo = match motivo {
            Some(motivo) => DialogoLlavero::danado(motivo),
            None => DialogoLlavero::nuevo(modo),
        };
        dialogo.al_arrancar = al_arrancar;
        dialogo.continuaciones.extend(continuacion);
        self.mostrar_dialogo_servidor(Dialogo::Llavero(Box::new(dialogo)));
        self.sucio = true;
    }

    /// El diálogo del llavero de ese modo que ya está a la vista o apilado.
    fn dialogo_llavero_abierto(&mut self, modo: ModoLlavero) -> Option<&mut DialogoLlavero> {
        let del_modo = |dialogo: &Dialogo| matches!(dialogo, Dialogo::Llavero(llavero) if llavero.modo == modo);
        if self.dialogo.as_ref().is_some_and(del_modo) {
            return match self.dialogo.as_mut() {
                Some(Dialogo::Llavero(llavero)) => Some(&mut **llavero),
                _ => None,
            };
        }
        let indice = self.pila_dialogos.iter().position(del_modo)?;
        match self.pila_dialogos.get_mut(indice) {
            Some(Dialogo::Llavero(llavero)) => Some(&mut **llavero),
            _ => None,
        }
    }

    /// Si el llavero impide la operación, la aplaza abriendo el diálogo con su
    /// continuación y devuelve `true`. La continuación se construye solo si
    /// hace falta (lleva secretos).
    pub(crate) fn aplazar_por_llavero(
        &mut self,
        uso: UsoLlavero,
        continuacion: impl FnOnce() -> Continuacion,
    ) -> bool {
        if self.necesidad_llavero(uso).is_none() {
            return false;
        }
        self.pedir_llavero(Some(continuacion()));
        true
    }

    /// ¿Esta frase de error es de un llavero que no está listo (cerrado o sin
    /// crear)?
    #[must_use]
    pub(crate) fn es_error_de_llavero(error: &str) -> bool {
        es_bloqueado(error) || error == SIN_CREAR
    }

    // ------------------------------------------------ contraseñas de host

    /// Contraseña del llavero para una sesión que la pide. Con el llavero
    /// cerrado no la envía: lo dice, para que el llamante pida la frase.
    pub(crate) fn intentar_llavero(&mut self, sesion_id: u32, host: &str) -> IntentoLlavero {
        let Some(ficha) = self.hosts.iter().find(|ficha| ficha.nombre == *host) else {
            return IntentoLlavero::NoHay;
        };
        if ficha.identidad_ref != crate::modelo::IdentidadRef::ContrasenaLlavero {
            return IntentoLlavero::NoHay;
        }
        // El mismo usuario con el que se guardó: el de la ficha o, sin él, el
        // local (como hace la conexión al autenticar).
        let usuario = ficha
            .usuario
            .clone()
            .unwrap_or_else(crate::conexion::usuario_local);
        match self.llavero.recuperar_contrasena(host, &usuario) {
            Ok(Some(contrasena)) => {
                self.servidor.enviar(protocolo::MensajeCliente::Contrasena {
                    sesion_id,
                    contrasena: protocolo::Secreto::nuevo(contrasena.as_str()),
                    recordar: false,
                });
                IntentoLlavero::Enviada
            }
            Err(error) if es_bloqueado(&error) && self.llavero_con_frase() => {
                IntentoLlavero::Bloqueado
            }
            _ => IntentoLlavero::NoHay,
        }
    }

    /// El servidor pide la contraseña de una conexión interactiva: la del
    /// llavero si el host la guarda; si el llavero está cerrado, antes se pide
    /// la frase; si no hay ninguna, el diálogo normal.
    pub(crate) fn pedir_contrasena_interactiva(
        &mut self,
        sesion_id: u32,
        host: String,
        intento: u8,
        recordar_por_defecto: bool,
    ) {
        if intento == 1 {
            match self.intentar_llavero(sesion_id, &host) {
                IntentoLlavero::Enviada => return,
                IntentoLlavero::Bloqueado => {
                    self.pedir_llavero(Some(Continuacion::ReintentarContrasena {
                        sesion_id,
                        host,
                        intento,
                        recordar_por_defecto,
                    }));
                    return;
                }
                IntentoLlavero::NoHay => {}
            }
        }
        self.mostrar_dialogo_contrasena(sesion_id, host, intento, recordar_por_defecto);
    }

    /// El diálogo de contraseña de una conexión interactiva.
    pub(crate) fn mostrar_dialogo_contrasena(
        &mut self,
        sesion_id: u32,
        host: String,
        intento: u8,
        recordar_por_defecto: bool,
    ) {
        self.mostrar_dialogo_servidor(Dialogo::ContrasenaServidor {
            sesion_id,
            host,
            intento,
            campo: CampoTexto::default(),
            recordar: recordar_por_defecto && self.llavero.disponible(),
            foco_casilla: false,
        });
    }

    /// El usuario pulsó ↵ en el diálogo de contraseña. Con «recordar» y el
    /// llavero de fichero sin abrir, antes hay que abrirlo o crearlo: la
    /// contraseña espera en la continuación (y, con Esc, se usa sin
    /// guardarse). Devuelve `true` si la envió el llavero más tarde.
    pub(crate) fn diferir_contrasena_por_llavero(
        &mut self,
        sesion_id: u32,
        contrasena: &protocolo::Secreto,
        recordar: bool,
    ) -> bool {
        if recordar && self.necesidad_llavero(UsoLlavero::Escribir).is_some() {
            self.pedir_llavero(Some(Continuacion::EnviarContrasena {
                sesion_id,
                contrasena: Zeroizing::new(contrasena.como_str().to_string()),
            }));
            return true;
        }
        false
    }

    /// La contraseña tecleada con «recordar» sale ya hacia el servidor y se
    /// guardará cuando la sesión quede abierta.
    fn enviar_contrasena(&mut self, sesion_id: u32, contrasena: &str, recordar: bool) {
        let contrasena = protocolo::Secreto::nuevo(contrasena);
        if recordar {
            self.contrasenas_pendientes
                .insert(sesion_id, (contrasena.clone(), true));
        }
        self.servidor.enviar(protocolo::MensajeCliente::Contrasena {
            sesion_id,
            contrasena,
            recordar,
        });
    }

    /// ¿Sigue existiendo la sesión por la que se pidió algo? Con el servidor
    /// caído no hay ninguna: sus ids no valen para el servidor nuevo.
    fn sesion_viva(&self, sesion_id: u32) -> bool {
        !self.servidor_caido
            && self
                .pestanas
                .iter()
                .any(|pestaña| pestaña.sesion_id == sesion_id)
    }

    /// ¿Sigue abriéndose el escritorio por el que el servidor pidió las
    /// credenciales? El servidor solo pregunta mientras abre (o reabre) la
    /// sesión: si ya no está en su lista, o ya está abierta, caída o cerrando,
    /// la pregunta caducó (el diálogo de la frase puede haber tardado minutos)
    /// y contestarla no tendría a quién llegar.
    fn escritorio_abriendo(&self, escritorio_id: u32) -> bool {
        !self.servidor_caido
            && self
                .escritorios
                .get(&escritorio_id)
                .is_some_and(|info| info.estado == EstadoEscritorio::Abriendo)
    }

    /// El servidor pide la contraseña del llavero para una conexión
    /// automática (reconexión, ejecución…). Con el llavero cerrado no se abre
    /// ningún diálogo ni se contesta: marca «llavero bloqueado» en la barra y,
    /// si es una pestaña, se deja vencer el plazo del servidor (contestar con
    /// `Cerrar` la cerraría). Esa petición se recuerda hasta que venza el
    /// plazo: si el usuario desbloquea antes, se contesta
    /// (`contestar_peticiones_ignoradas`). Devuelve `true` si ya está atendida.
    pub(crate) fn automatica_con_llavero_cerrado(
        &mut self,
        sesion_id: u32,
        host: &str,
        usuario: &str,
    ) -> bool {
        if !self.llavero_con_frase() {
            return false;
        }
        let estado = self.llavero.estado();
        if !matches!(estado, EstadoLlavero::Bloqueado | EstadoLlavero::Danado(_)) {
            return false;
        }
        self.poner_llavero_necesitado(true);
        self.sucio = true;
        if !self.sesion_viva(sesion_id) {
            self.servidor
                .enviar(protocolo::MensajeCliente::Cerrar { sesion_id });
            self.mensaje(BLOQUEADO, true);
        } else if estado == EstadoLlavero::Bloqueado {
            // Un dañado no se desbloquea: no hay nada que recordar.
            self.llavero_ui.recordar_peticion(PeticionIgnorada {
                sesion_id,
                host: host.to_string(),
                usuario: usuario.to_string(),
                desde: Instant::now(),
            });
        }
        true
    }

    /// El llavero se acaba de abrir (desbloqueado, creado o con la frase
    /// cambiada): se contestan las peticiones de contraseña de pestañas que se
    /// ignoraron y siguen dentro del plazo del servidor, por el mismo camino
    /// que una petición normal, y los motivos que decían «llavero bloqueado»
    /// dejan de decirlo.
    fn llavero_abierto_ahora(&mut self) {
        let vigentes = self.llavero_ui.tomar_vigentes(Instant::now());
        for peticion in vigentes {
            if self.sesion_viva(peticion.sesion_id) {
                self.contestar_con_llavero(peticion.sesion_id, peticion.host, peticion.usuario);
            }
        }
        for pestaña in &mut self.pestanas {
            if pestaña
                .motivo
                .as_deref()
                .is_some_and(|motivo| motivo.contains(BLOQUEADO))
            {
                // Una pestaña caída lo dice con su instrucción; una que se
                // abre o está abierta no necesita motivo.
                pestaña.motivo = (pestaña.estado == EstadoSesionRemota::Caida)
                    .then(|| MOTIVO_DESBLOQUEADO.to_string());
            }
        }
    }

    /// ¿Una petición automática sin contraseña guardada debe cerrar la
    /// conexión (`Cerrar`)? Con el llavero de fichero y una pestaña no: cerrar
    /// quitaría la pestaña que se reconecta; se deja vencer el plazo.
    pub(crate) fn cerrar_si_no_hay_contrasena(&self, sesion_id: u32) -> bool {
        !(self.llavero_con_frase() && self.sesion_viva(sesion_id))
    }

    /// Con el llavero cerrado, la causa de una pestaña caída no es «no hay
    /// contraseña guardada» sino que está bloqueado. Se dice en un solo sitio,
    /// después de cada actualización de las pestañas.
    pub(crate) fn normalizar_motivos_llavero(&mut self) {
        let hay_que_reescribir = self.pestanas.iter().any(|pestaña| {
            pestaña
                .motivo
                .as_deref()
                .is_some_and(|motivo| motivo.contains(MARCA_SIN_CONTRASENA))
        });
        if !hay_que_reescribir
            || !self.llavero_con_frase()
            || self.llavero.estado() != EstadoLlavero::Bloqueado
        {
            return;
        }
        let mut reescrito = false;
        for pestaña in &mut self.pestanas {
            if pestaña
                .motivo
                .as_deref()
                .is_some_and(|motivo| motivo.contains(MARCA_SIN_CONTRASENA))
            {
                pestaña.motivo = Some(BLOQUEADO.to_string());
                reescrito = true;
            }
        }
        if reescrito {
            self.poner_llavero_necesitado(true);
        }
    }

    /// Una contraseña tecleada con «recordar» pasa al llavero cuando la
    /// sesión abre. Con el llavero de fichero se hace en un hilo (reescribe el
    /// fichero con scrypt) y el resultado vuelve por `Evento::Llavero`.
    /// Devuelve `false` si el llavero no es de fichero (lo guarda el llamante
    /// como siempre).
    pub(crate) fn guardar_contrasena_en_segundo_plano(
        &mut self,
        host_id: i64,
        host: String,
        usuario: String,
        contrasena: &protocolo::Secreto,
    ) -> bool {
        if !self.llavero_con_frase() {
            return false;
        }
        let id = self.nueva_peticion_llavero();
        let llavero = self.llavero.clone();
        let eventos = self.eventos_tx.clone();
        let contrasena = Zeroizing::new(contrasena.como_str().to_string());
        self.runtime.spawn(async move {
            let nombre = host.clone();
            let resultado = tokio::task::spawn_blocking(move || {
                llavero.guardar_contrasena(&nombre, &usuario, contrasena.as_str())
            })
            .await
            .unwrap_or_else(|_| Err("la tarea del llavero terminó con un fallo".to_string()));
            let _ = eventos.send(Evento::Llavero(EventoLlavero::ContrasenaGuardada {
                id,
                host_id,
                host,
                resultado,
            }));
        });
        true
    }

    // ------------------------------------------------ peticiones de scrypt

    fn nueva_peticion_llavero(&mut self) -> u64 {
        self.llavero_ui.siguiente += 1;
        let id = self.llavero_ui.siguiente;
        self.llavero_ui.en_vuelo.insert(id);
        id
    }

    /// Lanza `desbloquear` en un hilo.
    fn lanzar_desbloqueo(&mut self, id: u64, frase: Zeroizing<String>) {
        let llavero = self.llavero.clone();
        let eventos = self.eventos_tx.clone();
        self.runtime.spawn(async move {
            let resultado = tokio::task::spawn_blocking(move || match llavero.de_fichero() {
                Some(fichero) => fichero.desbloquear(&frase),
                None => Err(ErrorLlavero::Inexistente),
            })
            .await
            .unwrap_or_else(|_| Err(error_de_tarea()));
            let _ = eventos.send(Evento::Llavero(EventoLlavero::Desbloqueo { id, resultado }));
        });
    }

    /// Lanza `crear` en un hilo.
    fn lanzar_creacion(&mut self, id: u64, frase: Zeroizing<String>, repetida: Zeroizing<String>) {
        let llavero = self.llavero.clone();
        let eventos = self.eventos_tx.clone();
        self.runtime.spawn(async move {
            let resultado = tokio::task::spawn_blocking(move || match llavero.de_fichero() {
                Some(fichero) => fichero.crear(&frase, &repetida),
                None => Err(ErrorLlavero::Inexistente),
            })
            .await
            .unwrap_or_else(|_| Err(error_de_tarea()));
            let _ = eventos.send(Evento::Llavero(EventoLlavero::Creacion { id, resultado }));
        });
    }

    /// Lanza `cambiar_frase` en un hilo.
    fn lanzar_cambio_de_frase(
        &mut self,
        id: u64,
        actual: Zeroizing<String>,
        nueva: Zeroizing<String>,
        repetida: Zeroizing<String>,
    ) {
        let llavero = self.llavero.clone();
        let eventos = self.eventos_tx.clone();
        self.runtime.spawn(async move {
            let resultado = tokio::task::spawn_blocking(move || match llavero.de_fichero() {
                Some(fichero) => fichero.cambiar_frase(&actual, &nueva, &repetida),
                None => Err(ErrorLlavero::Inexistente),
            })
            .await
            .unwrap_or_else(|_| Err(error_de_tarea()));
            let _ = eventos.send(Evento::Llavero(EventoLlavero::CambioFrase {
                id,
                resultado,
            }));
        });
    }

    // ------------------------------------------------ teclas del diálogo

    /// Teclas de los cinco diálogos del llavero.
    pub(crate) fn tecla_dialogo_llavero(
        &mut self,
        mut dialogo: Box<DialogoLlavero>,
        tecla: KeyEvent,
    ) {
        // Con scrypt en marcha no vale ninguna tecla, Esc tampoco: la tarea no
        // se puede abortar y el llavero se crea, se desbloquea o cambia de
        // frase igualmente, así que cerrar el diálogo diría lo contrario de lo
        // que pasa. El pie lo dice («espera…»).
        if dialogo.trabajando.is_some() {
            self.dialogo = Some(Dialogo::Llavero(dialogo));
            return;
        }
        if dialogo.modo == ModoLlavero::Danado {
            self.tecla_llavero_danado(dialogo, tecla);
            return;
        }
        match tecla.code {
            KeyCode::Esc => self.cancelar_dialogo_llavero(dialogo),
            KeyCode::Enter => self.confirmar_dialogo_llavero(dialogo),
            KeyCode::Tab | KeyCode::Down => {
                if !dialogo.campos.is_empty() {
                    dialogo.foco = (dialogo.foco + 1) % dialogo.campos.len();
                }
                self.dialogo = Some(Dialogo::Llavero(dialogo));
            }
            KeyCode::BackTab | KeyCode::Up => {
                let total = dialogo.campos.len();
                if total > 0 {
                    dialogo.foco = (dialogo.foco + total - 1) % total;
                }
                self.dialogo = Some(Dialogo::Llavero(dialogo));
            }
            _ => {
                // Ctrl+<letra> no escribe: son atajos del terminal.
                if !tecla.modifiers.contains(KeyModifiers::CONTROL) {
                    dialogo.teclear(&tecla);
                    // Lo escrito invalida el error anterior (no la espera).
                    if dialogo.espera_hasta.is_none() {
                        dialogo.error = None;
                    }
                }
                self.dialogo = Some(Dialogo::Llavero(dialogo));
            }
        }
    }

    fn tecla_llavero_danado(&mut self, dialogo: Box<DialogoLlavero>, tecla: KeyEvent) {
        match tecla.code {
            KeyCode::Char('a') | KeyCode::Char('A') => self.apartar_llavero_danado(dialogo),
            KeyCode::Esc | KeyCode::Char('s') | KeyCode::Char('S') => {
                self.cancelar_dialogo_llavero(dialogo);
            }
            _ => self.dialogo = Some(Dialogo::Llavero(dialogo)),
        }
    }

    /// Esc (o salir): se cierra el diálogo y cada continuación sabe qué hacer
    /// sin llavero. En el dañado de arranque, sale de MAGI sin tocar nada.
    fn cancelar_dialogo_llavero(&mut self, mut dialogo: Box<DialogoLlavero>) {
        let continuaciones = std::mem::take(&mut dialogo.continuaciones);
        let salir = dialogo.modo == ModoLlavero::Danado && dialogo.al_arrancar;
        // Aquí no hay petición en vuelo del propio diálogo (con una, ninguna
        // tecla llega). Si el diálogo se sustituyó por otro (servidor caído),
        // la respuesta llega sin diálogo y se aplica igual.
        drop(dialogo);
        if salir {
            self.salir = true;
        }
        for continuacion in continuaciones {
            self.cancelar_continuacion(continuacion);
        }
        self.sucio = true;
    }

    fn confirmar_dialogo_llavero(&mut self, mut dialogo: Box<DialogoLlavero>) {
        match dialogo.modo {
            ModoLlavero::Desbloquear => {
                let ahora = Instant::now();
                if let Some(segundos) = dialogo.espera_restante(ahora) {
                    dialogo.error = Some(texto_espera(segundos));
                } else if dialogo.campos[0].texto.is_empty() {
                    dialogo.error = Some("escribe la frase maestra".to_string());
                } else {
                    let frase = sacar_texto(&mut dialogo.campos[0]);
                    let id = self.nueva_peticion_llavero();
                    dialogo.trabajando = Some(id);
                    dialogo.error = None;
                    self.lanzar_desbloqueo(id, frase);
                }
            }
            ModoLlavero::Crear => {
                // La validación no gasta scrypt: se dice antes de gastarlo.
                match preparar_frase(&dialogo.campos[0].texto, Some(&dialogo.campos[1].texto)) {
                    Ok(_) => {
                        let frase = sacar_texto(&mut dialogo.campos[0]);
                        let repetida = sacar_texto(&mut dialogo.campos[1]);
                        let id = self.nueva_peticion_llavero();
                        dialogo.trabajando = Some(id);
                        dialogo.error = None;
                        self.lanzar_creacion(id, frase, repetida);
                    }
                    Err(error) => dialogo.error = Some(error.to_string()),
                }
            }
            ModoLlavero::Cambiar => {
                let ahora = Instant::now();
                if let Some(segundos) = dialogo.espera_restante(ahora) {
                    dialogo.error = Some(texto_espera(segundos));
                } else if dialogo.campos[0].texto.is_empty() {
                    dialogo.error = Some("escribe la frase actual".to_string());
                } else {
                    match preparar_frase(&dialogo.campos[1].texto, Some(&dialogo.campos[2].texto)) {
                        Ok(_) => {
                            let actual = sacar_texto(&mut dialogo.campos[0]);
                            let nueva = sacar_texto(&mut dialogo.campos[1]);
                            let repetida = sacar_texto(&mut dialogo.campos[2]);
                            let id = self.nueva_peticion_llavero();
                            dialogo.trabajando = Some(id);
                            dialogo.error = None;
                            self.lanzar_cambio_de_frase(id, actual, nueva, repetida);
                        }
                        Err(error) => dialogo.error = Some(error.to_string()),
                    }
                }
            }
            ModoLlavero::Borrar => {
                if !dialogo.campos[0]
                    .texto
                    .trim()
                    .eq_ignore_ascii_case(PALABRA_BORRAR)
                {
                    dialogo.error = Some(format!("escribe «{PALABRA_BORRAR}» para borrarlo"));
                } else {
                    self.borrar_llavero(dialogo);
                    return;
                }
            }
            ModoLlavero::Danado => {}
        }
        self.dialogo = Some(Dialogo::Llavero(dialogo));
    }

    /// Borra `llavero.age` (el diálogo ya pidió escribir `borrar`). El núcleo
    /// mira el fichero que hay ahora, bajo el cerrojo, y no el que vio el
    /// diálogo al abrirse: si entretanto se estropeó, no se borra.
    fn borrar_llavero(&mut self, mut dialogo: Box<DialogoLlavero>) {
        let resultado = match self.llavero.de_fichero() {
            Some(fichero) => fichero.borrar(),
            None => Err(ErrorLlavero::Inexistente),
        };
        match resultado {
            Ok(()) => {
                drop(dialogo);
                self.anotar(
                    crate::registro::LLAVERO_BORRADO,
                    None,
                    None,
                    "",
                    ResultadoRegistro::Ok,
                );
                self.poner_llavero_necesitado(false);
                self.mensaje("llavero borrado", false);
                self.sucio = true;
            }
            Err(ErrorLlavero::NoSeBorraDanado) => {
                // Un llavero dañado no se borra: se aparta. El diálogo pasa a
                // LLAVERO DAÑADO, que ofrece apartarlo, con el motivo de ahora.
                let motivo = match self.llavero.estado() {
                    EstadoLlavero::Danado(motivo) => motivo,
                    _ => ErrorLlavero::NoSeBorraDanado.to_string(),
                };
                dialogo.pasar_a_danado(motivo);
                self.dialogo = Some(Dialogo::Llavero(dialogo));
                self.sucio = true;
            }
            Err(error) => {
                dialogo.error = Some(error.to_string());
                self.dialogo = Some(Dialogo::Llavero(dialogo));
            }
        }
    }

    /// Aparta el llavero dañado como `llavero.age.danado-<fecha>` y sigue sin
    /// llavero. Nunca se borra, y solo se aparta lo que sigue dañado: el
    /// núcleo lo mira bajo el cerrojo, con el fichero de ahora.
    fn apartar_llavero_danado(&mut self, mut dialogo: Box<DialogoLlavero>) {
        let resultado = match self.llavero.de_fichero() {
            Some(fichero) => fichero.apartar_danado(),
            None => Err(ErrorLlavero::Inexistente),
        };
        match resultado {
            Ok(ruta) => {
                let nombre = ruta
                    .file_name()
                    .map(|nombre| nombre.to_string_lossy().to_string())
                    .unwrap_or_default();
                // Sin llavero ya no hay nada que continuar: cada continuación
                // sigue como si el usuario hubiera dicho que no.
                let continuaciones = std::mem::take(&mut dialogo.continuaciones);
                drop(dialogo);
                self.anotar(
                    crate::registro::LLAVERO_APARTADO,
                    None,
                    None,
                    &nombre,
                    ResultadoRegistro::Ok,
                );
                self.mensaje(
                    format!("llavero apartado como {nombre}: sigues sin llavero"),
                    false,
                );
                for continuacion in continuaciones {
                    self.cancelar_continuacion(continuacion);
                }
                self.refrescar_marca_llavero();
                self.sucio = true;
            }
            Err(ErrorLlavero::NoEstaDanado) => self.llavero_ya_no_esta_danado(dialogo),
            Err(ErrorLlavero::Inexistente) => {
                // Otra ventana lo apartó antes: ya no hay nada que apartar. Se
                // sigue sin llavero como si lo hubiera apartado este diálogo,
                // pero sin anotar `llavero_apartado` (no lo apartó este
                // equipo desde aquí).
                let continuaciones = std::mem::take(&mut dialogo.continuaciones);
                drop(dialogo);
                self.mensaje("el llavero ya no existe: sigues sin llavero", false);
                for continuacion in continuaciones {
                    self.cancelar_continuacion(continuacion);
                }
                self.refrescar_marca_llavero();
                self.sucio = true;
            }
            Err(error) => {
                dialogo.error = Some(error.to_string());
                self.dialogo = Some(Dialogo::Llavero(dialogo));
            }
        }
    }

    /// El fichero que el diálogo daba por dañado ya no lo está (otra ventana
    /// lo restauró o creó uno nuevo): no se toca. Se cierra el diálogo, se
    /// avisa y se vuelve a mirar el estado de ahora, como al arrancar, para
    /// abrir el que toque (la frase, crear o el mismo dañado si lo está otra
    /// vez) con las mismas continuaciones.
    fn llavero_ya_no_esta_danado(&mut self, mut dialogo: Box<DialogoLlavero>) {
        let continuaciones = std::mem::take(&mut dialogo.continuaciones);
        let al_arrancar = dialogo.al_arrancar;
        drop(dialogo);
        self.mensaje("el llavero ya no está dañado", false);
        if continuaciones.is_empty() {
            // Como al arrancar: solo hay diálogo si hay que dar la frase (o
            // si sigue sin poderse leer).
            if matches!(
                self.llavero.estado(),
                EstadoLlavero::Bloqueado | EstadoLlavero::Danado(_)
            ) {
                self.abrir_dialogo_llavero(None, al_arrancar);
            }
        } else {
            for continuacion in continuaciones {
                if self.necesidad_llavero(continuacion.uso()).is_none() {
                    self.continuar_tras_llavero(continuacion);
                } else {
                    self.abrir_dialogo_llavero(Some(continuacion), al_arrancar);
                }
            }
        }
        self.refrescar_marca_llavero();
        self.sucio = true;
    }

    // ------------------------------------------------ respuestas de scrypt

    /// Llega el resultado de una petición de scrypt. Si ya no está en vuelo
    /// (otra respuesta la sustituyó), se descarta.
    pub(crate) fn evento_llavero(&mut self, evento: EventoLlavero) {
        if !self.llavero_ui.en_vuelo.remove(&evento.id()) {
            return;
        }
        self.sucio = true;
        match evento {
            EventoLlavero::Desbloqueo { id, resultado } => {
                self.resultado_de_frase(id, resultado, TipoFrase::Desbloqueo);
            }
            EventoLlavero::Creacion { id, resultado } => {
                // El fichero ya está escrito aunque el diálogo ya no esté: el
                // efecto se anota (la frase nunca).
                if resultado.is_ok() {
                    self.anotar(
                        crate::registro::LLAVERO_CREADO,
                        None,
                        None,
                        "",
                        ResultadoRegistro::Ok,
                    );
                }
                self.resultado_de_frase(id, resultado, TipoFrase::Creacion);
            }
            EventoLlavero::CambioFrase { id, resultado } => {
                if resultado.is_ok() {
                    self.anotar(
                        crate::registro::LLAVERO_FRASE_CAMBIADA,
                        None,
                        None,
                        "",
                        ResultadoRegistro::Ok,
                    );
                }
                self.resultado_de_frase(id, resultado, TipoFrase::Cambio);
            }
            EventoLlavero::ContrasenaGuardada {
                host_id,
                host,
                resultado,
                ..
            } => self.contrasena_guardada_por_el_llavero(host_id, &host, resultado),
        }
    }

    fn resultado_de_frase(
        &mut self,
        id: u64,
        resultado: Result<(), ErrorLlavero>,
        tipo: TipoFrase,
    ) {
        // Cada quinto fallo seguido anota una línea (el LlaveroFichero cuenta
        // la serie y la espera), aunque el diálogo ya no esté.
        if let Err(ErrorLlavero::FraseIncorrecta { intentos_seguidos }) = &resultado {
            if *intentos_seguidos > 0 && *intentos_seguidos % FALLOS_POR_SERIE == 0 {
                self.anotar(
                    crate::registro::LLAVERO_DESBLOQUEO_FALLIDO,
                    None,
                    None,
                    &intentos_seguidos.to_string(),
                    ResultadoRegistro::Error,
                );
            }
        }
        let vivo = self.sacar_dialogo_llavero(id);
        match resultado {
            Ok(()) => {
                self.poner_llavero_necesitado(false);
                let (continuaciones, estaba_encima) = match vivo {
                    Some((mut dialogo, sitio)) => {
                        let continuaciones = std::mem::take(&mut dialogo.continuaciones);
                        drop(dialogo);
                        (continuaciones, matches!(sitio, Sitio::Superior))
                    }
                    None => (Vec::new(), false),
                };
                self.mensaje(
                    match tipo {
                        TipoFrase::Desbloqueo => "llavero desbloqueado",
                        TipoFrase::Creacion => "llavero creado",
                        TipoFrase::Cambio => "frase maestra cambiada",
                    },
                    false,
                );
                // La sincronización que falló por estar cerrado se reintenta.
                if tipo != TipoFrase::Cambio && self.pasada_esperaba_el_llavero() {
                    self.solicitar_pasada();
                }
                // Las pestañas que pidieron su contraseña mientras estaba
                // cerrado, si siguen dentro del plazo del servidor.
                self.llavero_abierto_ahora();
                self.continuar_sin_pisar(continuaciones);
                // Si el diálogo del llavero estaba encima, vuelve el que
                // apartó (si una continuación no abrió otro).
                if estaba_encima {
                    self.restaurar_dialogo_apilado();
                }
            }
            Err(error) => {
                let Some((mut dialogo, sitio)) = vivo else {
                    return;
                };
                dialogo.trabajando = None;
                dialogo.aplicar_error(error, tipo, Instant::now());
                self.devolver_dialogo_llavero(dialogo, sitio);
            }
        }
    }

    /// Saca de donde esté el diálogo que espera la petición `id`.
    fn sacar_dialogo_llavero(&mut self, id: u64) -> Option<(Box<DialogoLlavero>, Sitio)> {
        let es_ese = |dialogo: &Dialogo| matches!(dialogo, Dialogo::Llavero(llavero) if llavero.trabajando == Some(id));
        if self.dialogo.as_ref().is_some_and(es_ese) {
            return match self.dialogo.take() {
                Some(Dialogo::Llavero(dialogo)) => Some((dialogo, Sitio::Superior)),
                otro => {
                    self.dialogo = otro;
                    None
                }
            };
        }
        let indice = self.pila_dialogos.iter().position(es_ese)?;
        match self.pila_dialogos.remove(indice) {
            Dialogo::Llavero(dialogo) => Some((dialogo, Sitio::Pila(indice))),
            otro => {
                self.pila_dialogos.insert(indice, otro);
                None
            }
        }
    }

    fn devolver_dialogo_llavero(&mut self, dialogo: Box<DialogoLlavero>, sitio: Sitio) {
        match sitio {
            Sitio::Superior => self.dialogo = Some(Dialogo::Llavero(dialogo)),
            Sitio::Pila(indice) => {
                let indice = indice.min(self.pila_dialogos.len());
                self.pila_dialogos.insert(indice, Dialogo::Llavero(dialogo));
            }
        }
    }

    /// La pasada de sincronización quedó parada por el llavero cerrado.
    fn pasada_esperaba_el_llavero(&self) -> bool {
        matches!(
            &self.sincronizacion.estado,
            crate::sincronizacion::EstadoPasada::Error(motivo) if es_bloqueado(motivo)
        )
    }

    /// La contraseña recordada llegó (o no) al llavero.
    fn contrasena_guardada_por_el_llavero(
        &mut self,
        host_id: i64,
        host: &str,
        resultado: Result<(), String>,
    ) {
        match resultado {
            Ok(()) => {
                if let Err(error) = self
                    .almacen
                    .marcar_identidad_ref(host_id, &crate::modelo::IdentidadRef::ContrasenaLlavero)
                {
                    tracing::warn!("no se pudo marcar la identidad de llavero: {error}");
                }
                if let Some(ficha) = self.hosts.iter_mut().find(|ficha| ficha.id == host_id) {
                    ficha.identidad_ref = crate::modelo::IdentidadRef::ContrasenaLlavero;
                }
                self.mensaje(
                    format!("contraseña de «{host}» guardada en el llavero de MAGI"),
                    false,
                );
            }
            Err(error) => {
                if es_bloqueado(&error) {
                    self.poner_llavero_necesitado(true);
                }
                self.mensaje(format!("no se pudo guardar la contraseña: {error}"), true);
            }
        }
    }

    // ------------------------------------------------ continuaciones

    /// El llavero está abierto (desbloqueado o creado): sigue lo que esperaba.
    pub(crate) fn continuar_tras_llavero(&mut self, continuacion: Continuacion) {
        match continuacion {
            Continuacion::ReintentarContrasena {
                sesion_id,
                host,
                intento,
                recordar_por_defecto,
            } => {
                if !self.sesion_viva(sesion_id) {
                    return;
                }
                if self.intentar_llavero(sesion_id, &host) != IntentoLlavero::Enviada {
                    self.mostrar_dialogo_contrasena(sesion_id, host, intento, recordar_por_defecto);
                }
            }
            Continuacion::EnviarContrasena {
                sesion_id,
                contrasena,
            } => {
                if self.sesion_viva(sesion_id) {
                    self.enviar_contrasena(sesion_id, contrasena.as_str(), true);
                }
            }
            Continuacion::ContrasenaLocal {
                contrasena,
                responder,
            } => {
                let _ = responder.send(Some((contrasena, true)));
            }
            Continuacion::ReintentarCredenciales {
                peticion_id,
                escritorio_id,
                host_nombre,
                peticion,
            } => {
                // El diálogo de la frase pudo tardar minutos: si el escritorio
                // ya no se está abriendo, la pregunta caducó.
                if self.escritorio_abriendo(escritorio_id) {
                    self.pedir_credenciales_escritorio(
                        peticion_id,
                        escritorio_id,
                        host_nombre,
                        peticion,
                    );
                }
            }
            Continuacion::EnviarCredenciales {
                peticion_id,
                escritorio_id,
                credenciales,
            } => {
                // Sin escritorio que abrir no hay a quién enviarlas ni sesión
                // que las guarde después: se descartan (con su `Zeroizing`).
                if !self.escritorio_abriendo(escritorio_id) {
                    self.mensaje(
                        "el escritorio ya no se está abriendo: credenciales descartadas",
                        false,
                    );
                    return;
                }
                // Se guardan cuando el servidor confirme que la sesión abrió.
                self.contrasenas_escritorio_pendientes.insert(
                    escritorio_id,
                    (
                        credenciales.usuario.clone(),
                        Zeroizing::new(credenciales.contrasena.to_string()),
                    ),
                );
                self.enviar_credenciales(credenciales, peticion_id);
            }
            Continuacion::CrearSincronizacion { carpeta, texto } => {
                self.crear_sincronizacion(&carpeta, &texto);
            }
            Continuacion::UnirEquipo { carpeta, clave } => {
                self.unir_equipo(&carpeta, clave.as_str());
            }
            Continuacion::MostrarClave => self.mostrar_clave(),
            Continuacion::QuitarEquipo(uuid) => {
                self.ejecutar_accion_inventario(AccionInventario::QuitarEquipo(uuid));
            }
            Continuacion::RotarClave => {
                self.ejecutar_accion_inventario(AccionInventario::RotarClave)
            }
            Continuacion::Desactivar => {
                self.ejecutar_accion_inventario(AccionInventario::Desactivar)
            }
        }
    }

    /// Corre las continuaciones sin pisar el diálogo que haya a la vista: si
    /// alguna abre uno propio, el que estaba pasa a la pila y vuelve cuando
    /// ese se cierre (una pregunta del servidor no se pierde).
    fn continuar_sin_pisar(&mut self, continuaciones: Vec<Continuacion>) {
        let anterior = self.dialogo.take();
        for continuacion in continuaciones {
            self.continuar_tras_llavero(continuacion);
        }
        if let Some(anterior) = anterior {
            match self.dialogo.take() {
                Some(nuevo) => {
                    self.pila_dialogos.push(anterior);
                    self.dialogo = Some(nuevo);
                }
                None => self.dialogo = Some(anterior),
            }
        }
    }

    /// El usuario cerró el diálogo sin dar (o crear) la frase: cada operación
    /// sigue sin llavero o se cancela, diciéndolo.
    pub(crate) fn cancelar_continuacion(&mut self, continuacion: Continuacion) {
        match continuacion {
            Continuacion::ReintentarContrasena {
                sesion_id,
                host,
                intento,
                ..
            } => {
                // Diálogo de contraseña normal, sin «recordar».
                if self.sesion_viva(sesion_id) {
                    self.mostrar_dialogo_contrasena(sesion_id, host, intento, false);
                }
            }
            Continuacion::EnviarContrasena {
                sesion_id,
                contrasena,
            } => {
                // Se usa sin guardarse y no se crea ningún fichero.
                if self.sesion_viva(sesion_id) {
                    self.enviar_contrasena(sesion_id, contrasena.as_str(), false);
                    self.mensaje("contraseña usada sin guardar: no hay llavero", false);
                }
            }
            Continuacion::ContrasenaLocal {
                contrasena,
                responder,
            } => {
                // Se usa sin guardarse y no se crea ningún fichero.
                let _ = responder.send(Some((contrasena, false)));
                self.mensaje("contraseña usada sin guardar: no hay llavero", false);
            }
            Continuacion::ReintentarCredenciales {
                peticion_id,
                escritorio_id,
                host_nombre,
                peticion,
            } => {
                // Diálogo de credenciales normal, sin «recordar», si el
                // escritorio sigue abriéndose.
                if self.escritorio_abriendo(escritorio_id) {
                    self.abrir_dialogo_credenciales(
                        peticion_id,
                        escritorio_id,
                        host_nombre,
                        peticion,
                        false,
                    );
                }
            }
            Continuacion::EnviarCredenciales {
                peticion_id,
                escritorio_id,
                mut credenciales,
            } => {
                // Se usan sin guardarse y no se crea ningún fichero.
                if self.escritorio_abriendo(escritorio_id) {
                    credenciales.recordar = false;
                    self.enviar_credenciales(credenciales, peticion_id);
                    self.mensaje("contraseña usada sin guardar: no hay llavero", false);
                }
            }
            Continuacion::CrearSincronizacion { texto, .. } => {
                self.mensaje(
                    format!("{SIN_LLAVERO_NO_GUARDA}: sincronización cancelada"),
                    true,
                );
                self.reabrir_carpeta(&texto);
            }
            Continuacion::UnirEquipo { carpeta, clave } => {
                self.abrir_unir_equipo_para(&carpeta);
                if let Some(Dialogo::UnirEquipo { campo, error, .. }) = &mut self.dialogo {
                    *campo = CampoTexto::nuevo(clave.as_str());
                    *error = Some(format!("{SIN_LLAVERO_NO_GUARDA}: unión cancelada"));
                }
            }
            Continuacion::MostrarClave => self.mensaje(BLOQUEADO, true),
            Continuacion::QuitarEquipo(_) => {
                self.mensaje(format!("{BLOQUEADO}: no se ha quitado el equipo"), true);
            }
            Continuacion::RotarClave => {
                self.mensaje(format!("{BLOQUEADO}: la clave no se ha rotado"), true);
            }
            Continuacion::Desactivar => {
                self.mensaje(format!("{BLOQUEADO}: la sincronización sigue activa"), true);
            }
        }
    }

    // ------------------------------------------------ paleta y barra

    /// Entradas de la paleta del llavero: solo con el llavero de fichero.
    pub(crate) fn entradas_paleta_llavero(&self) -> Vec<EntradaPaleta> {
        use super::AccionPaleta;
        let mut entradas = Vec::new();
        if !self.llavero_con_frase() {
            return entradas;
        }
        let mut entrada = |etiqueta: &str, accion: AccionPaleta| {
            entradas.push(EntradaPaleta {
                etiqueta: etiqueta.to_string(),
                categoria: "llavero",
                accion,
            });
        };
        match self.llavero.estado() {
            EstadoLlavero::Bloqueado => {
                entrada("Desbloquear el llavero", AccionPaleta::DesbloquearLlavero);
                entrada(
                    "Cambiar la frase maestra",
                    AccionPaleta::CambiarFraseLlavero,
                );
                entrada("Borrar el llavero", AccionPaleta::BorrarLlavero);
            }
            EstadoLlavero::Abierto => {
                entrada("Bloquear el llavero", AccionPaleta::BloquearLlavero);
                entrada(
                    "Cambiar la frase maestra",
                    AccionPaleta::CambiarFraseLlavero,
                );
                entrada("Borrar el llavero", AccionPaleta::BorrarLlavero);
            }
            // Sin fichero o dañado no hay nada que desbloquear, cambiar ni
            // borrar (un llavero dañado se aparta, nunca se borra).
            EstadoLlavero::SinCrear | EstadoLlavero::Danado(_) => {}
        }
        entradas
    }

    /// Paleta: «Desbloquear el llavero».
    pub(crate) fn paleta_desbloquear_llavero(&mut self) {
        match self.llavero.estado() {
            EstadoLlavero::Abierto => self.mensaje("el llavero ya está desbloqueado", false),
            _ => self.pedir_llavero(None),
        }
    }

    /// Paleta: «Bloquear el llavero». La frase y las entradas salen de la
    /// memoria.
    pub fn bloquear_llavero(&mut self) {
        if let Some(fichero) = self.llavero.de_fichero() {
            fichero.bloquear();
            self.poner_llavero_necesitado(false);
            self.mensaje("llavero bloqueado: la frase ya no está en memoria", false);
            self.sucio = true;
        }
    }

    /// Paleta: «Cambiar la frase maestra» o «Borrar el llavero».
    pub(crate) fn paleta_dialogo_llavero(&mut self, modo: ModoLlavero) {
        if !self.llavero_con_frase()
            || !matches!(
                self.llavero.estado(),
                EstadoLlavero::Abierto | EstadoLlavero::Bloqueado
            )
        {
            self.mensaje("no hay llavero", true);
            return;
        }
        self.mostrar_dialogo_servidor(Dialogo::Llavero(Box::new(DialogoLlavero::nuevo(modo))));
        self.sucio = true;
    }

    /// Cuántos diálogos hay apilados debajo del que se ve (para las pruebas).
    #[doc(hidden)]
    #[must_use]
    pub fn dialogos_apilados(&self) -> usize {
        self.pila_dialogos.len()
    }

    /// ¿Hay que decir en la barra que el llavero está bloqueado? Solo lee un
    /// booleano: la barra se pinta hasta 30 veces por segundo y mirar el
    /// estado del llavero es entrada/salida (`refrescar_marca_llavero` lo
    /// recalcula en el tick y cuando algo cambia).
    #[must_use]
    pub fn marca_llavero_bloqueado(&self) -> bool {
        self.llavero_ui.marca
    }

    /// Algo necesitó (o ya no necesita) el llavero: se apunta y se recalcula
    /// la marca de la barra.
    pub(crate) fn poner_llavero_necesitado(&mut self, necesitado: bool) {
        self.llavero_ui.necesitado = necesitado;
        self.refrescar_marca_llavero();
    }

    /// Recalcula la marca de la barra: algo necesitó el llavero y sigue
    /// cerrado. Es la única entrada/salida de la marca; sin nada que decir
    /// (`necesitado` apagado) no mira el fichero.
    pub(crate) fn refrescar_marca_llavero(&mut self) {
        let marca = self.llavero_ui.necesitado
            && self.llavero_con_frase()
            && self.llavero.estado() == EstadoLlavero::Bloqueado;
        if marca != self.llavero_ui.marca {
            self.llavero_ui.marca = marca;
            self.sucio = true;
        }
    }

    /// Cuántas peticiones de pestañas se recuerdan a la espera de que se abra
    /// el llavero (para las pruebas).
    #[doc(hidden)]
    #[must_use]
    pub fn peticiones_llavero_recordadas(&self) -> usize {
        self.llavero_ui.ignoradas.len()
    }

    /// Hace más viejas las peticiones recordadas (para las pruebas del plazo,
    /// sin esperar 10 s de verdad).
    #[doc(hidden)]
    pub fn envejecer_peticiones_llavero(&mut self, por: Duration) {
        for peticion in &mut self.llavero_ui.ignoradas {
            peticion.desde = peticion.desde.checked_sub(por).unwrap_or(peticion.desde);
        }
    }

    /// El servidor cayó: lo que el llavero guardaba para contestarle (la
    /// contraseña o las credenciales de una sesión o un escritorio) ya no tiene
    /// a quién llegar, y sus ids podrían significar otra cosa en el servidor
    /// nuevo. Se cancelan, con el mismo camino que un Esc, las continuaciones
    /// de los diálogos del llavero (el abierto y los apilados) y se olvidan
    /// las peticiones recordadas; las demás continuaciones siguen donde
    /// estaban. Se llama con la marca de servidor caído ya puesta, de modo que
    /// cancelar no abre diálogos ni contesta a nadie.
    pub(crate) fn cancelar_continuaciones_del_servidor(&mut self) {
        let mut cancelables = Vec::new();
        if let Some(Dialogo::Llavero(dialogo)) = self.dialogo.as_mut() {
            cancelables.extend(dialogo.sacar_continuaciones_del_servidor());
        }
        for dialogo in &mut self.pila_dialogos {
            if let Dialogo::Llavero(dialogo) = dialogo {
                cancelables.extend(dialogo.sacar_continuaciones_del_servidor());
            }
        }
        self.llavero_ui.ignoradas.clear();
        for continuacion in cancelables {
            self.cancelar_continuacion(continuacion);
        }
    }

    /// Cada tick: la espera de un diálogo a la vista se repinta sola, la
    /// marca de la barra se pone al día y se olvidan las peticiones vencidas.
    pub(crate) fn tick_llavero(&mut self) {
        self.refrescar_marca_llavero();
        self.llavero_ui.descartar_vencidas(Instant::now());
        if let Some(Dialogo::Llavero(dialogo)) = &mut self.dialogo {
            if dialogo.espera_hasta.is_some() {
                if dialogo.espera_restante(Instant::now()).is_none() {
                    dialogo.espera_hasta = None;
                    dialogo.error = None;
                }
                self.sucio = true;
            }
        }
    }

    /// Un sondeo que falló por el llavero cerrado también deja la marca.
    #[doc(hidden)]
    pub fn nota_llavero_necesitado(&mut self, error: Option<&str>) {
        if error.is_some_and(|error| error.contains(BLOQUEADO)) {
            self.poner_llavero_necesitado(true);
        }
    }

    /// Al salir de MAGI: la frase y las entradas fuera de la memoria.
    pub(crate) fn bloquear_llavero_al_salir(&mut self) {
        if let Some(fichero) = self.llavero.de_fichero() {
            fichero.bloquear();
        }
    }
}

// ---------------------------------------------------------------- utilidades

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TipoFrase {
    Desbloqueo,
    Creacion,
    Cambio,
}

/// Saca lo escrito de un campo (moviéndolo: no queda copia) y lo deja vacío.
fn sacar_texto(campo: &mut CampoTexto) -> Zeroizing<String> {
    let texto = std::mem::replace(
        &mut campo.texto,
        String::with_capacity(MAX_FRASE_BYTES + 16),
    );
    campo.cursor = 0;
    Zeroizing::new(texto)
}

fn texto_espera(segundos: u64) -> String {
    format!("demasiados intentos: espera {segundos} s antes de reintentar")
}

fn error_de_tarea() -> ErrorLlavero {
    ErrorLlavero::Io("la tarea del llavero terminó con un fallo".to_string())
}
